//! Skill storage and lightweight evolution primitives.
//!
//! This is intentionally deterministic. Learner and curator flows can propose
//! and organize skills, but promotion/archival is explicit and snapshot-backed.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{validate_profile_id, Error};

const MAX_SKILL_BYTES: u64 = 256 * 1024;
const MAX_OBSERVATION_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillStatus {
    Bundled,
    Active,
    Proposed,
    Stale,
    Archived,
}

impl SkillStatus {
    fn as_str(&self) -> &'static str {
        match self {
            SkillStatus::Bundled => "bundled",
            SkillStatus::Active => "active",
            SkillStatus::Proposed => "proposed",
            SkillStatus::Stale => "stale",
            SkillStatus::Archived => "archived",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "bundled" => SkillStatus::Bundled,
            "proposed" => SkillStatus::Proposed,
            "stale" => SkillStatus::Stale,
            "archived" => SkillStatus::Archived,
            _ => SkillStatus::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub id: String,
    pub title: String,
    pub status: SkillStatus,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub path: PathBuf,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSearchHit {
    pub id: String,
    pub title: String,
    pub status: SkillStatus,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub path: PathBuf,
    pub score: u32,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillProposal {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUsage {
    pub skill_id: String,
    pub outcome: String,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub loaded: bool,
    pub used: bool,
    pub duration_ms: Option<u64>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionObservation {
    pub kind: String,
    pub summary: String,
    pub thread_id: Option<String>,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
    pub signals: Vec<String>,
    pub evidence: Vec<String>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionRunReport {
    pub generated_at: DateTime<Utc>,
    pub observations_read: usize,
    pub proposals_created: Vec<SkillProposal>,
    pub skipped: Vec<String>,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuratorReport {
    pub generated_at: DateTime<Utc>,
    pub dry_run: bool,
    pub total_skills: usize,
    pub stale_candidates: Vec<CuratorCandidate>,
    pub archived: Vec<String>,
    pub snapshot_path: Option<PathBuf>,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuratorCandidate {
    pub id: String,
    pub path: PathBuf,
    pub reason: String,
}

pub struct SkillStore {
    home: PathBuf,
    profile: String,
}

impl SkillStore {
    pub fn new(home: impl AsRef<Path>, profile: &str) -> Result<Self, Error> {
        validate_profile_id(profile).map_err(Error::Validation)?;
        let store = Self {
            home: home.as_ref().to_path_buf(),
            profile: profile.to_string(),
        };
        store.ensure_dirs()?;
        Ok(store)
    }

    pub fn search(&self, query: &str, top_k: usize) -> Result<Vec<SkillSearchHit>, Error> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return Ok(Vec::new());
        }
        let mut hits = Vec::new();
        for record in self.list_skills(false)? {
            if record.status == SkillStatus::Archived {
                continue;
            }
            let content = read_limited(&record.path)?;
            let haystack = format!(
                "{}\n{}\n{}\n{}",
                record.id,
                record.title,
                record.tags.join(" "),
                content
            );
            let hay_tokens = tokenize(&haystack);
            let score = score_tokens(&query_tokens, &hay_tokens, &record);
            if score == 0 {
                continue;
            }
            hits.push(SkillSearchHit {
                id: record.id,
                title: record.title,
                status: record.status,
                tags: record.tags,
                summary: record.summary,
                path: record.path,
                score,
                snippet: snippet(&content, &query_tokens),
            });
        }
        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
        hits.truncate(top_k.clamp(1, 20));
        Ok(hits)
    }

    pub fn list_skills(&self, include_archived: bool) -> Result<Vec<SkillRecord>, Error> {
        let mut out = Vec::new();
        for (status, dir) in [
            (SkillStatus::Active, self.active_dir()),
            (SkillStatus::Proposed, self.proposed_dir()),
            (SkillStatus::Archived, self.archive_dir()),
        ] {
            collect_skill_dir(&mut out, &dir, status, include_archived)?;
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    pub fn propose(
        &self,
        title: &str,
        body: &str,
        tags: Vec<String>,
        reason: &str,
    ) -> Result<SkillProposal, Error> {
        let title = normalize_title(title)?;
        let reason = non_empty(reason, "reason")?;
        let id = unique_skill_id(&self.proposed_dir(), &slugify(&title))?;
        let path = self.proposed_dir().join(format!("{id}.md"));
        let now = Utc::now();
        let clean_tags = normalize_tags(tags);
        let content = render_skill(
            &id,
            &title,
            SkillStatus::Proposed,
            &clean_tags,
            Some(reason),
            now,
            now,
            body,
        );
        atomic_write(&path, &content)?;
        self.append_history(
            "skill.proposed",
            &serde_json::json!({
                "id": id,
                "title": title,
                "tags": clean_tags,
                "path": path,
                "reason": reason,
                "at": now,
            }),
        )?;
        Ok(SkillProposal {
            id,
            title,
            tags: clean_tags,
            path,
            reason: reason.to_string(),
        })
    }

    pub fn promote(&self, id: &str, reason: &str) -> Result<SkillRecord, Error> {
        let id = validate_skill_id(id)?;
        let reason = non_empty(reason, "reason")?;
        let from = self.proposed_dir().join(format!("{id}.md"));
        if !from.exists() {
            return Err(Error::NotFound(format!("proposed skill {id}")));
        }
        let snapshot = self.snapshot("promote")?;
        let content = read_limited(&from)?;
        let updated = rewrite_status(&content, SkillStatus::Active, Some(reason));
        let to = self.active_dir().join(format!("{id}.md"));
        atomic_write(&to, &updated)?;
        fs::remove_file(&from)?;
        self.append_history(
            "skill.promoted",
            &serde_json::json!({
                "id": id,
                "from": from,
                "to": to,
                "snapshot": snapshot,
                "reason": reason,
                "at": Utc::now(),
            }),
        )?;
        parse_skill_file(&to, SkillStatus::Active)
    }

    pub fn archive(&self, id: &str, reason: &str) -> Result<SkillRecord, Error> {
        let id = validate_skill_id(id)?;
        let reason = non_empty(reason, "reason")?;
        let Some((from, prior_status)) = self.find_mutable_skill_path(id) else {
            return Err(Error::NotFound(format!("skill {id}")));
        };
        let snapshot = self.snapshot("archive")?;
        let content = read_limited(&from)?;
        let updated = rewrite_status(&content, SkillStatus::Archived, Some(reason));
        let to = unique_archive_path(&self.archive_dir(), id)?;
        atomic_write(&to, &updated)?;
        fs::remove_file(&from)?;
        self.append_history(
            "skill.archived",
            &serde_json::json!({
                "id": id,
                "from": from,
                "from_status": prior_status,
                "to": to,
                "snapshot": snapshot,
                "reason": reason,
                "at": Utc::now(),
            }),
        )?;
        parse_skill_file(&to, SkillStatus::Archived)
    }

    pub fn record_usage(&self, usage: SkillUsage) -> Result<(), Error> {
        validate_skill_id(&usage.skill_id)?;
        self.append_jsonl(&self.usage_log(), &usage)
    }

    pub fn observe(&self, mut observation: EvolutionObservation) -> Result<(), Error> {
        non_empty(&observation.kind, "kind")?;
        non_empty(&observation.summary, "summary")?;
        let approx = serde_json::to_string(&observation)
            .map_err(|e| Error::Other(e.into()))?
            .len();
        if approx > MAX_OBSERVATION_BYTES {
            return Err(Error::LimitExceeded(format!(
                "observation exceeds {MAX_OBSERVATION_BYTES} bytes"
            )));
        }
        if observation.recorded_at.timestamp_millis() == 0 {
            observation.recorded_at = Utc::now();
        }
        self.append_jsonl(&self.observations_log(), &observation)
    }

    pub fn evolve_run(&self, limit: usize) -> Result<EvolutionRunReport, Error> {
        let observations = self.read_observations(limit.clamp(1, 200))?;
        let mut proposals = Vec::new();
        let mut skipped = Vec::new();
        let mut grouped: BTreeMap<String, Vec<EvolutionObservation>> = BTreeMap::new();
        for obs in observations.iter().cloned() {
            grouped.entry(obs.kind.clone()).or_default().push(obs);
        }
        for (kind, group) in grouped {
            if group.len() < 2 && !group.iter().any(|o| o.signals.len() >= 3) {
                skipped.push(format!("{kind}: insufficient repeated signal"));
                continue;
            }
            let title = format!("{} workflow pattern", title_case(&kind));
            let body = render_proposed_from_observations(&kind, &group);
            let tags = vec!["learned".to_string(), slugify(&kind)];
            match self.propose(
                &title,
                &body,
                tags,
                "learner batch proposal from observed traces",
            ) {
                Ok(p) => proposals.push(p),
                Err(e) => skipped.push(format!("{kind}: {e}")),
            }
        }
        let generated_at = Utc::now();
        let report_path = self
            .learner_dir()
            .join("reports")
            .join(format!("{}.json", timestamp_id(generated_at)));
        let report = EvolutionRunReport {
            generated_at,
            observations_read: observations.len(),
            proposals_created: proposals,
            skipped,
            report_path: report_path.clone(),
        };
        atomic_write(
            &report_path,
            &serde_json::to_string_pretty(&report).map_err(|e| Error::Other(e.into()))?,
        )?;
        Ok(report)
    }

    pub fn curator_run(&self, dry_run: bool) -> Result<CuratorReport, Error> {
        let skills = self.list_skills(false)?;
        let usage = self.usage_counts()?;
        let mut stale = Vec::new();
        for skill in skills.iter().filter(|s| s.status == SkillStatus::Active) {
            let count = usage.get(&skill.id).copied().unwrap_or(0);
            if count == 0 {
                stale.push(CuratorCandidate {
                    id: skill.id.clone(),
                    path: skill.path.clone(),
                    reason: "active skill has no recorded usage".to_string(),
                });
            }
        }
        let mut archived = Vec::new();
        let snapshot = if dry_run || stale.is_empty() {
            None
        } else {
            Some(self.snapshot("curator")?)
        };
        if !dry_run {
            for candidate in &stale {
                if self
                    .archive(&candidate.id, "curator archived unused active skill")
                    .is_ok()
                {
                    archived.push(candidate.id.clone());
                }
            }
        }
        let generated_at = Utc::now();
        let report_path = self
            .curator_dir()
            .join(format!("{}.json", timestamp_id(generated_at)));
        let report = CuratorReport {
            generated_at,
            dry_run,
            total_skills: skills.len(),
            stale_candidates: stale,
            archived,
            snapshot_path: snapshot,
            report_path: report_path.clone(),
        };
        atomic_write(
            &report_path,
            &serde_json::to_string_pretty(&report).map_err(|e| Error::Other(e.into()))?,
        )?;
        Ok(report)
    }

    fn profile_dir(&self) -> PathBuf {
        self.home.join("profiles").join(&self.profile)
    }

    fn skills_dir(&self) -> PathBuf {
        self.profile_dir().join("skills")
    }

    fn active_dir(&self) -> PathBuf {
        self.skills_dir().join("agent_created")
    }

    fn proposed_dir(&self) -> PathBuf {
        self.skills_dir().join("proposed")
    }

    fn archive_dir(&self) -> PathBuf {
        self.skills_dir().join(".archive")
    }

    fn backups_dir(&self) -> PathBuf {
        self.skills_dir().join(".skill_backups")
    }

    fn learner_dir(&self) -> PathBuf {
        self.profile_dir().join("learner")
    }

    fn curator_dir(&self) -> PathBuf {
        self.profile_dir().join("logs").join("curator")
    }

    fn usage_log(&self) -> PathBuf {
        self.skills_dir().join(".usage.jsonl")
    }

    fn history_log(&self) -> PathBuf {
        self.skills_dir().join(".history.jsonl")
    }

    fn observations_log(&self) -> PathBuf {
        self.learner_dir().join("observations.jsonl")
    }

    fn ensure_dirs(&self) -> Result<(), Error> {
        for dir in [
            self.active_dir(),
            self.proposed_dir(),
            self.archive_dir(),
            self.backups_dir(),
            self.learner_dir().join("reports"),
            self.curator_dir(),
        ] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    fn append_history(&self, event: &str, payload: &serde_json::Value) -> Result<(), Error> {
        self.append_jsonl(
            &self.history_log(),
            &serde_json::json!({
                "event": event,
                "payload": payload,
                "at": Utc::now(),
            }),
        )
    }

    fn append_jsonl<T: Serialize>(&self, path: &Path, value: &T) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        serde_json::to_writer(&mut file, value).map_err(|e| Error::Other(e.into()))?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    }

    fn snapshot(&self, reason: &str) -> Result<PathBuf, Error> {
        let id = format!("{}-{}", timestamp_id(Utc::now()), slugify(reason));
        let target = self.backups_dir().join(id);
        fs::create_dir_all(&target)?;
        for dir_name in ["agent_created", "proposed", ".archive"] {
            let src = self.skills_dir().join(dir_name);
            let dst = target.join(dir_name);
            copy_dir(&src, &dst)?;
        }
        atomic_write(
            &target.join("manifest.json"),
            &serde_json::to_string_pretty(&serde_json::json!({
                "created_at": Utc::now(),
                "reason": reason,
                "format": "directory-snapshot-v1"
            }))
            .map_err(|e| Error::Other(e.into()))?,
        )?;
        Ok(target)
    }

    fn find_mutable_skill_path(&self, id: &str) -> Option<(PathBuf, SkillStatus)> {
        for (status, dir) in [
            (SkillStatus::Proposed, self.proposed_dir()),
            (SkillStatus::Active, self.active_dir()),
        ] {
            let path = dir.join(format!("{id}.md"));
            if path.exists() {
                return Some((path, status));
            }
        }
        None
    }

    fn read_observations(&self, limit: usize) -> Result<Vec<EvolutionObservation>, Error> {
        let path = self.observations_log();
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for line in text.lines().rev() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(obs) = serde_json::from_str::<EvolutionObservation>(line) {
                out.push(obs);
                if out.len() >= limit {
                    break;
                }
            }
        }
        out.reverse();
        Ok(out)
    }

    fn usage_counts(&self) -> Result<HashMap<String, usize>, Error> {
        let text = match fs::read_to_string(self.usage_log()) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(e) => return Err(e.into()),
        };
        let mut counts = HashMap::new();
        for line in text.lines() {
            if let Ok(usage) = serde_json::from_str::<SkillUsage>(line) {
                if usage.used {
                    *counts.entry(usage.skill_id).or_insert(0) += 1;
                }
            }
        }
        Ok(counts)
    }
}

fn collect_skill_dir(
    out: &mut Vec<SkillRecord>,
    dir: &Path,
    status: SkillStatus,
    include_archived: bool,
) -> Result<(), Error> {
    if status == SkillStatus::Archived && !include_archived {
        return Ok(());
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file()
            || entry.path().extension().and_then(|e| e.to_str()) != Some("md")
        {
            continue;
        }
        match parse_skill_file(&entry.path(), status.clone()) {
            Ok(record) => out.push(record),
            Err(e) => {
                tracing::warn!(path = %entry.path().display(), error = %e, "skipping unreadable skill")
            }
        }
    }
    Ok(())
}

fn parse_skill_file(path: &Path, fallback_status: SkillStatus) -> Result<SkillRecord, Error> {
    let content = read_limited(path)?;
    let (meta, body) = parse_frontmatter(&content);
    let id = meta.get("id").cloned().unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("skill")
            .to_string()
    });
    let title = meta
        .get("title")
        .cloned()
        .or_else(|| first_heading(body))
        .unwrap_or_else(|| id.clone());
    let status = meta
        .get("status")
        .map(|s| SkillStatus::from_str(s))
        .unwrap_or(fallback_status);
    let tags = meta
        .get("tags")
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .map(str::to_string)
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let created_at = meta.get("created_at").and_then(|s| s.parse().ok());
    let updated_at = meta.get("updated_at").and_then(|s| s.parse().ok());
    Ok(SkillRecord {
        id,
        title,
        status,
        tags,
        summary: meta.get("summary").cloned(),
        path: path.to_path_buf(),
        created_at,
        updated_at,
    })
}

fn parse_frontmatter(content: &str) -> (BTreeMap<String, String>, &str) {
    let mut meta = BTreeMap::new();
    if !content.starts_with("---\n") {
        return (meta, content);
    }
    let Some(end) = content[4..].find("\n---\n") else {
        return (meta, content);
    };
    let meta_text = &content[4..4 + end];
    for line in meta_text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        meta.insert(
            key.trim().to_string(),
            value.trim().trim_matches('"').to_string(),
        );
    }
    (meta, &content[4 + end + 5..])
}

fn render_skill(
    id: &str,
    title: &str,
    status: SkillStatus,
    tags: &[String],
    summary: Option<&str>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    body: &str,
) -> String {
    format!(
        "---\nid: {id}\ntitle: {}\nstatus: {}\ntags: {}\nsummary: {}\ncreated_at: {}\nupdated_at: {}\n---\n\n{}",
        escape_meta(title),
        status.as_str(),
        tags.join(", "),
        escape_meta(summary.unwrap_or("")),
        created_at.to_rfc3339(),
        updated_at.to_rfc3339(),
        body.trim(),
    )
}

fn rewrite_status(content: &str, status: SkillStatus, summary: Option<&str>) -> String {
    let (mut meta, body) = parse_frontmatter(content);
    meta.insert("status".into(), status.as_str().into());
    meta.insert("updated_at".into(), Utc::now().to_rfc3339());
    if let Some(summary) = summary {
        meta.insert("summary".into(), summary.to_string());
    }
    let mut rendered = String::from("---\n");
    for (key, value) in meta {
        rendered.push_str(&format!("{key}: {}\n", escape_meta(&value)));
    }
    rendered.push_str("---\n");
    rendered.push_str(body);
    rendered
}

fn render_proposed_from_observations(kind: &str, observations: &[EvolutionObservation]) -> String {
    let mut body = format!("# {} workflow pattern\n\n## When To Use\n\nUse this when similar `{kind}` work repeats or recovery patterns recur.\n\n## Observed Signals\n\n", title_case(kind));
    for obs in observations.iter().take(8) {
        body.push_str(&format!("- {}: {}\n", obs.kind, obs.summary));
        for signal in obs.signals.iter().take(4) {
            body.push_str(&format!("  - signal: {signal}\n"));
        }
    }
    body.push_str("\n## Procedure\n\n1. Check whether the current task matches the observed pattern.\n2. Reuse the successful recovery or workflow steps from the evidence.\n3. Verify with focused checks before reporting completion.\n\n## Evidence\n\n");
    for obs in observations.iter().take(8) {
        for evidence in obs.evidence.iter().take(3) {
            body.push_str(&format!("- {evidence}\n"));
        }
    }
    body
}

fn read_limited(path: &Path) -> Result<String, Error> {
    let meta = fs::metadata(path)?;
    if meta.len() > MAX_SKILL_BYTES {
        return Err(Error::LimitExceeded(format!(
            "{} exceeds {MAX_SKILL_BYTES} bytes",
            path.display()
        )));
    }
    Ok(fs::read_to_string(path)?)
}

fn atomic_write(path: &Path, content: &str) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    if let Some(parent) = path.parent() {
        let dir = fs::File::open(parent)?;
        dir.sync_all()?;
    }
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), Error> {
    fs::create_dir_all(dst)?;
    if !src.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn unique_skill_id(dir: &Path, base: &str) -> Result<String, Error> {
    let base = if base.is_empty() { "skill" } else { base };
    for n in 0..1000 {
        let id = if n == 0 {
            base.to_string()
        } else {
            format!("{base}-{n}")
        };
        if !dir.join(format!("{id}.md")).exists() {
            return Ok(id);
        }
    }
    Err(Error::LimitExceeded(
        "could not allocate unique skill id".into(),
    ))
}

fn unique_archive_path(dir: &Path, id: &str) -> Result<PathBuf, Error> {
    fs::create_dir_all(dir)?;
    for n in 0..1000 {
        let suffix = if n == 0 {
            String::new()
        } else {
            format!("-{n}")
        };
        let path = dir.join(format!("{id}{suffix}.md"));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(Error::LimitExceeded(
        "could not allocate archive path".into(),
    ))
}

fn score_tokens(query: &HashSet<String>, hay: &HashSet<String>, record: &SkillRecord) -> u32 {
    let mut score = 0;
    for token in query {
        if record.id.contains(token) {
            score += 5;
        }
        if record.title.to_ascii_lowercase().contains(token) {
            score += 4;
        }
        if record.tags.iter().any(|t| t == token) {
            score += 3;
        }
        if hay.contains(token) {
            score += 1;
        }
    }
    score
}

fn snippet(content: &str, query: &HashSet<String>) -> String {
    for line in content.lines() {
        let lower = line.to_ascii_lowercase();
        if query.iter().any(|q| lower.contains(q)) {
            return line.trim().chars().take(240).collect();
        }
    }
    content
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .chars()
        .take(240)
        .collect()
}

fn tokenize(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .map(|s| s.to_ascii_lowercase())
        .filter(|s| s.len() >= 3)
        .collect()
}

fn normalize_title(title: &str) -> Result<String, Error> {
    let title = non_empty(title, "title")?;
    if title.len() > 160 {
        return Err(Error::LimitExceeded("title exceeds 160 chars".into()));
    }
    Ok(title.to_string())
}

fn non_empty<'a>(value: &'a str, field: &str) -> Result<&'a str, Error> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::Validation(format!("{field} must not be empty")));
    }
    Ok(value)
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for tag in tags {
        let tag = slugify(&tag);
        if !tag.is_empty() && seen.insert(tag.clone()) {
            out.push(tag);
        }
    }
    out
}

fn validate_skill_id(id: &str) -> Result<&str, Error> {
    if id.is_empty()
        || id.len() > 120
        || id.contains("..")
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(Error::Validation(format!("invalid skill id: {id}")));
    }
    Ok(id)
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for c in value.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').chars().take(80).collect()
}

fn title_case(value: &str) -> String {
    value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut chars = s.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape_meta(value: &str) -> String {
    value
        .replace('\n', " ")
        .replace('\r', " ")
        .replace(':', " -")
}

fn first_heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.strip_prefix("# ").map(|s| s.trim().to_string()))
}

fn timestamp_id(ts: DateTime<Utc>) -> String {
    ts.format("%Y%m%dT%H%M%S%.3fZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, SkillStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SkillStore::new(dir.path(), "default").unwrap();
        (dir, store)
    }

    #[test]
    fn propose_search_promote_and_archive_roundtrip() {
        let (_dir, store) = store();
        let proposal = store
            .propose(
                "Svelte store cleanup",
                "# Svelte store cleanup\n\nUse derived stores for repeated polling cleanup.",
                vec!["frontend".into(), "svelte".into()],
                "observed repeated frontend cleanup",
            )
            .unwrap();

        let hits = store.search("svelte polling", 5).unwrap();
        assert_eq!(hits[0].id, proposal.id);
        assert_eq!(hits[0].status, SkillStatus::Proposed);

        let promoted = store.promote(&proposal.id, "reviewed").unwrap();
        assert_eq!(promoted.status, SkillStatus::Active);
        assert!(promoted
            .path
            .ends_with("agent_created/svelte-store-cleanup.md"));
        assert!(!proposal.path.exists());

        let archived = store.archive(&proposal.id, "unused").unwrap();
        assert_eq!(archived.status, SkillStatus::Archived);
        assert!(archived.path.to_string_lossy().contains(".archive"));
        assert!(store.backups_dir().read_dir().unwrap().next().is_some());
    }

    #[test]
    fn evolve_run_creates_proposed_skill_from_repeated_observations() {
        let (_dir, store) = store();
        for n in 0..2 {
            store
                .observe(EvolutionObservation {
                    kind: "frontend qa".into(),
                    summary: format!("manual browser QA caught layout issue {n}"),
                    thread_id: Some("thread-1".into()),
                    session_id: None,
                    task_id: None,
                    signals: vec!["qa-fail".into(), "rework".into()],
                    evidence: vec!["agent-browser found overlap".into()],
                    recorded_at: Utc::now(),
                })
                .unwrap();
        }
        let report = store.evolve_run(20).unwrap();
        assert_eq!(report.observations_read, 2);
        assert_eq!(report.proposals_created.len(), 1);
        let hits = store.search("frontend qa", 5).unwrap();
        assert!(!hits.is_empty());
    }

    #[test]
    fn curator_dry_run_reports_unused_active_skills_without_archiving() {
        let (_dir, store) = store();
        let proposal = store
            .propose(
                "Rust audit",
                "# Rust audit\n\nRun cargo audit.",
                vec!["rust".into()],
                "seed",
            )
            .unwrap();
        store.promote(&proposal.id, "seed active").unwrap();

        let report = store.curator_run(true).unwrap();
        assert!(report.dry_run);
        assert_eq!(report.stale_candidates.len(), 1);
        assert!(store.active_dir().join("rust-audit.md").exists());
    }
}
