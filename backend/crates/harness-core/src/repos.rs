use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::validate_profile_id;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("validation: {0}")]
    Validation(String),
    #[error("repo not found: {0}")]
    NotFound(String),
    #[error("not a git repository: {0}")]
    NotGitRepo(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct RepoContext {
    pub repo_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub root_path: String,
    pub canonical_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct RepoIdentity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub root_path: String,
    pub canonical_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct RepoRecord {
    pub id: String,
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub root_path: String,
    pub canonical_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_head_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct RepoThreadRecord {
    pub repo_id: String,
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    pub started_at: i64,
    pub last_seen_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct RepoContinuity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_goal: Option<String>,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub recent_threads: Vec<RepoThreadRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct CurrentRepoReport {
    pub detected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<RepoIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<RepoRecord>,
    #[serde(default)]
    pub threads: Vec<RepoThreadRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuity: Option<RepoContinuity>,
}

#[derive(Debug, Clone)]
pub struct RepoIndex {
    profile: String,
    db_path: PathBuf,
}

impl RepoIndex {
    pub fn with_profile(home: impl AsRef<Path>, profile: &str) -> Result<Self, RepoError> {
        validate_profile_id(profile).map_err(RepoError::Validation)?;
        let dir = home.as_ref().join("profiles").join(profile).join("repos");
        std::fs::create_dir_all(&dir)?;
        let index = Self {
            profile: profile.to_string(),
            db_path: dir.join("index.db"),
        };
        index.init()?;
        Ok(index)
    }

    fn conn(&self) -> Result<Connection, RepoError> {
        Ok(Connection::open(&self.db_path)?)
    }

    fn init(&self) -> Result<(), RepoError> {
        let conn = self.conn()?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS repos (
              id TEXT PRIMARY KEY,
              profile TEXT NOT NULL,
              project_id TEXT,
              root_path TEXT NOT NULL,
              canonical_path TEXT NOT NULL,
              remote_url TEXT,
              default_branch TEXT,
              last_branch TEXT,
              last_head_sha TEXT,
              last_thread_id TEXT,
              last_session_id TEXT,
              summary TEXT,
              first_seen_at INTEGER NOT NULL,
              last_seen_at INTEGER NOT NULL,
              UNIQUE(profile, canonical_path),
              UNIQUE(profile, remote_url)
            );
            CREATE TABLE IF NOT EXISTS repo_threads (
              repo_id TEXT NOT NULL,
              thread_id TEXT NOT NULL,
              branch TEXT,
              head_sha TEXT,
              started_at INTEGER NOT NULL,
              last_seen_at INTEGER NOT NULL,
              summary TEXT,
              PRIMARY KEY (repo_id, thread_id)
            );
            CREATE INDEX IF NOT EXISTS idx_repos_profile_seen
              ON repos(profile, last_seen_at DESC);
            CREATE INDEX IF NOT EXISTS idx_repo_threads_repo_seen
              ON repo_threads(repo_id, last_seen_at DESC);
            "#,
        )?;
        Ok(())
    }

    pub fn detect(&self, cwd: &Path) -> Result<RepoIdentity, RepoError> {
        detect_repo(cwd)
    }

    pub fn current_report(&self, cwd: &Path) -> Result<CurrentRepoReport, RepoError> {
        let identity = match self.detect(cwd) {
            Ok(identity) => identity,
            Err(RepoError::NotGitRepo(_)) => {
                return Ok(CurrentRepoReport {
                    detected: false,
                    identity: None,
                    repo: None,
                    threads: Vec::new(),
                    continuity: None,
                });
            }
            Err(e) => return Err(e),
        };
        let repo = self.find_by_identity(&identity)?;
        let threads = match repo.as_ref() {
            Some(repo) => self.list_threads(&repo.id)?,
            None => Vec::new(),
        };
        Ok(CurrentRepoReport {
            detected: true,
            identity: Some(identity),
            repo,
            threads,
            continuity: None,
        })
    }

    pub fn touch(
        &self,
        identity: &RepoIdentity,
        thread_id: Option<&str>,
        session_id: Option<&str>,
        summary: Option<&str>,
    ) -> Result<(RepoRecord, RepoContext), RepoError> {
        let conn = self.conn()?;
        let now = Utc::now().timestamp_millis();
        let existing = find_record(&conn, &self.profile, identity)?;
        let id = existing
            .as_ref()
            .map(|r| r.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let first_seen_at = existing.as_ref().map(|r| r.first_seen_at).unwrap_or(now);
        let last_thread_id = thread_id
            .map(str::to_string)
            .or_else(|| existing.as_ref().and_then(|r| r.last_thread_id.clone()));
        let last_session_id = session_id
            .map(str::to_string)
            .or_else(|| existing.as_ref().and_then(|r| r.last_session_id.clone()));
        let summary = summary
            .map(str::to_string)
            .or_else(|| existing.as_ref().and_then(|r| r.summary.clone()));

        conn.execute(
            r#"
            INSERT INTO repos (
              id, profile, project_id, root_path, canonical_path, remote_url,
              default_branch, last_branch, last_head_sha, last_thread_id,
              last_session_id, summary, first_seen_at, last_seen_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(id) DO UPDATE SET
              project_id=excluded.project_id,
              root_path=excluded.root_path,
              canonical_path=excluded.canonical_path,
              remote_url=excluded.remote_url,
              default_branch=excluded.default_branch,
              last_branch=excluded.last_branch,
              last_head_sha=excluded.last_head_sha,
              last_thread_id=excluded.last_thread_id,
              last_session_id=excluded.last_session_id,
              summary=excluded.summary,
              last_seen_at=excluded.last_seen_at
            "#,
            params![
                id,
                self.profile,
                identity.project_id,
                identity.root_path,
                identity.canonical_path,
                identity.remote_url,
                identity.default_branch,
                identity.branch,
                identity.head_sha,
                last_thread_id,
                last_session_id,
                summary,
                first_seen_at,
                now,
            ],
        )?;

        if let Some(thread_id) = thread_id {
            conn.execute(
                r#"
                INSERT INTO repo_threads (
                  repo_id, thread_id, branch, head_sha, started_at, last_seen_at, summary
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(repo_id, thread_id) DO UPDATE SET
                  branch=excluded.branch,
                  head_sha=excluded.head_sha,
                  last_seen_at=excluded.last_seen_at,
                  summary=COALESCE(excluded.summary, repo_threads.summary)
                "#,
                params![
                    id,
                    thread_id,
                    identity.branch,
                    identity.head_sha,
                    now,
                    now,
                    summary,
                ],
            )?;
        }

        let record = self.get(&id)?;
        let context = record.context(identity.branch.clone(), identity.head_sha.clone());
        Ok((record, context))
    }

    pub fn get(&self, id: &str) -> Result<RepoRecord, RepoError> {
        let conn = self.conn()?;
        get_record(&conn, id)?.ok_or_else(|| RepoError::NotFound(id.to_string()))
    }

    pub fn find_by_identity(
        &self,
        identity: &RepoIdentity,
    ) -> Result<Option<RepoRecord>, RepoError> {
        let conn = self.conn()?;
        find_record(&conn, &self.profile, identity)
    }

    pub fn list_threads(&self, repo_id: &str) -> Result<Vec<RepoThreadRecord>, RepoError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT repo_id, thread_id, branch, head_sha, started_at, last_seen_at, summary
            FROM repo_threads
            WHERE repo_id = ?1
            ORDER BY last_seen_at DESC
            "#,
        )?;
        let rows = stmt.query_map(params![repo_id], row_to_repo_thread)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

impl RepoRecord {
    pub fn context(&self, branch: Option<String>, head_sha: Option<String>) -> RepoContext {
        RepoContext {
            repo_id: self.id.clone(),
            project_id: self.project_id.clone(),
            root_path: self.root_path.clone(),
            canonical_path: self.canonical_path.clone(),
            remote_url: self.remote_url.clone(),
            branch: branch.or_else(|| self.last_branch.clone()),
            head_sha: head_sha.or_else(|| self.last_head_sha.clone()),
        }
    }
}

fn detect_repo(cwd: &Path) -> Result<RepoIdentity, RepoError> {
    let root = git(cwd, &["rev-parse", "--show-toplevel"])
        .map_err(|_| RepoError::NotGitRepo(cwd.display().to_string()))?;
    let root_path = PathBuf::from(root.trim());
    let canonical_path = root_path
        .canonicalize()
        .unwrap_or_else(|_| root_path.clone())
        .to_string_lossy()
        .to_string();
    let root_display = root_path.to_string_lossy().to_string();
    let remote_url = git_optional(&root_path, &["remote", "get-url", "origin"])
        .or_else(|| first_remote_url(&root_path))
        .map(normalize_remote_url);
    let default_branch = git_optional(
        &root_path,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .and_then(|s| s.strip_prefix("origin/").map(str::to_string).or(Some(s)));
    let branch = git_optional(&root_path, &["branch", "--show-current"]).filter(|s| !s.is_empty());
    let head_sha = git_optional(&root_path, &["rev-parse", "HEAD"]);
    let project_id = read_project_marker(&root_path);

    Ok(RepoIdentity {
        project_id,
        root_path: root_display,
        canonical_path,
        remote_url,
        default_branch,
        branch,
        head_sha,
    })
}

fn git(cwd: &Path, args: &[&str]) -> Result<String, std::io::Error> {
    let output = Command::new("git").args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("git command failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_optional(cwd: &Path, args: &[&str]) -> Option<String> {
    git(cwd, args)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn first_remote_url(root: &Path) -> Option<String> {
    let remotes = git_optional(root, &["remote"])?;
    for remote in remotes.lines().map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(url) = git_optional(root, &["remote", "get-url", remote]) {
            return Some(url);
        }
    }
    None
}

fn normalize_remote_url(raw: String) -> String {
    raw.trim().trim_end_matches(".git").to_string()
}

fn read_project_marker(root: &Path) -> Option<String> {
    let path = root.join(".harness").join("project.toml");
    let raw = std::fs::read_to_string(path).ok()?;
    let doc = raw.parse::<toml_edit::DocumentMut>().ok()?;
    doc.get("project_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn find_record(
    conn: &Connection,
    profile: &str,
    identity: &RepoIdentity,
) -> Result<Option<RepoRecord>, RepoError> {
    if let Some(project_id) = identity.project_id.as_deref() {
        if let Some(record) = query_record(
            conn,
            "SELECT * FROM repos WHERE profile = ?1 AND project_id = ?2 LIMIT 1",
            params![profile, project_id],
        )? {
            return Ok(Some(record));
        }
    }
    if let Some(record) = query_record(
        conn,
        "SELECT * FROM repos WHERE profile = ?1 AND canonical_path = ?2 LIMIT 1",
        params![profile, identity.canonical_path],
    )? {
        return Ok(Some(record));
    }
    if let Some(remote_url) = identity.remote_url.as_deref() {
        if let Some(record) = query_record(
            conn,
            "SELECT * FROM repos WHERE profile = ?1 AND remote_url = ?2 LIMIT 1",
            params![profile, remote_url],
        )? {
            return Ok(Some(record));
        }
    }
    Ok(None)
}

fn get_record(conn: &Connection, id: &str) -> Result<Option<RepoRecord>, RepoError> {
    query_record(
        conn,
        "SELECT * FROM repos WHERE id = ?1 LIMIT 1",
        params![id],
    )
}

fn query_record<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> Result<Option<RepoRecord>, RepoError> {
    Ok(conn.query_row(sql, params, row_to_repo_record).optional()?)
}

fn row_to_repo_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<RepoRecord> {
    Ok(RepoRecord {
        id: row.get("id")?,
        profile: row.get("profile")?,
        project_id: row.get("project_id")?,
        root_path: row.get("root_path")?,
        canonical_path: row.get("canonical_path")?,
        remote_url: row.get("remote_url")?,
        default_branch: row.get("default_branch")?,
        last_branch: row.get("last_branch")?,
        last_head_sha: row.get("last_head_sha")?,
        last_thread_id: row.get("last_thread_id")?,
        last_session_id: row.get("last_session_id")?,
        summary: row.get("summary")?,
        first_seen_at: row.get("first_seen_at")?,
        last_seen_at: row.get("last_seen_at")?,
    })
}

fn row_to_repo_thread(row: &rusqlite::Row<'_>) -> rusqlite::Result<RepoThreadRecord> {
    Ok(RepoThreadRecord {
        repo_id: row.get("repo_id")?,
        thread_id: row.get("thread_id")?,
        branch: row.get("branch")?,
        head_sha: row.get("head_sha")?,
        started_at: row.get("started_at")?,
        last_seen_at: row.get("last_seen_at")?,
        summary: row.get("summary")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_home() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn touch_repo_indexes_by_canonical_path() {
        let home = tmp_home();
        let index = RepoIndex::with_profile(home.path(), "default").unwrap();
        let identity = RepoIdentity {
            project_id: None,
            root_path: "/tmp/project".into(),
            canonical_path: "/tmp/project".into(),
            remote_url: Some("git@example.com:repo/project".into()),
            default_branch: Some("main".into()),
            branch: Some("feature".into()),
            head_sha: Some("abc".into()),
        };

        let (repo, ctx) = index
            .touch(
                &identity,
                Some("thread-1"),
                Some("session-1"),
                Some("summary"),
            )
            .unwrap();
        let (repo2, _) = index
            .touch(&identity, Some("thread-2"), Some("session-2"), None)
            .unwrap();

        assert_eq!(repo.id, repo2.id);
        assert_eq!(ctx.repo_id, repo.id);
        assert_eq!(index.list_threads(&repo.id).unwrap().len(), 2);
        assert_eq!(
            index.get(&repo.id).unwrap().last_session_id.as_deref(),
            Some("session-2")
        );
    }

    #[test]
    fn current_report_returns_not_detected_outside_git() {
        let home = tmp_home();
        let index = RepoIndex::with_profile(home.path(), "default").unwrap();
        let report = index.current_report(home.path()).unwrap();
        assert!(!report.detected);
        assert!(report.repo.is_none());
    }
}
