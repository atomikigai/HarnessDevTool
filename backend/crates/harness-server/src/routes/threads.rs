use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use harness_core::{
    AutonomyProfile, Event, ExecutionMode, Item, ReadinessIssue, ReadinessReport, ReconcileReport,
    ReconcileSessionRef, Thread, TimelineQueryOptions,
};
use harness_session::{SessionMeta, SessionStatus};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Default, Deserialize)]
pub struct CreateThreadRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub autonomy_profile: Option<AutonomyProfile>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateThreadResponse {
    pub id: String,
    pub readiness: ReadinessReport,
}

/// Thread enriched with the live sessions attached to it. The frontend uses
/// this shape to render the Sessions column without a second round-trip.
#[derive(Debug, Serialize)]
pub struct ThreadWithSessions {
    #[serde(flatten)]
    pub thread: Thread,
    pub sessions: Vec<SessionMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness: Option<ReadinessReport>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ReadinessQuery {
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TimelineQuery {
    #[serde(default)]
    pub after: Option<u64>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub event_type: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetAutonomyRequest {
    pub autonomy_profile: AutonomyProfile,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/threads", get(list_threads).post(create_thread))
        .route(
            "/api/threads/:id/readiness",
            get(get_readiness).post(recalculate_readiness),
        )
        .route("/api/threads/:id/reconcile", get(reconcile_thread))
        .route("/api/threads/:id/timeline", get(get_timeline))
        .route(
            "/api/threads/:id/autonomy",
            axum::routing::post(set_autonomy),
        )
}

async fn list_threads(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ThreadWithSessions>>, ApiError> {
    let threads = state.store.list_threads()?;
    // Group live + detached session metadata by thread_id.
    let mut by_thread: HashMap<String, Vec<SessionMeta>> = HashMap::new();
    for meta in state.manager.list_metas().await {
        by_thread
            .entry(meta.thread_id.clone())
            .or_default()
            .push(meta);
    }
    let enriched = threads
        .into_iter()
        .map(|t| ThreadWithSessions {
            readiness: state.store.read_readiness_report(&t.id).ok().flatten(),
            sessions: by_thread.remove(&t.id).unwrap_or_default(),
            thread: t,
        })
        .collect();
    Ok(Json(enriched))
}

async fn create_thread(
    State(state): State<Arc<AppState>>,
    body: Option<Json<CreateThreadRequest>>,
) -> Result<(StatusCode, Json<CreateThreadResponse>), ApiError> {
    let body = body.map(|b| b.0).unwrap_or_default();
    let title = body.title;
    let thread = state.store.create_thread(title)?;
    let autonomy = body.autonomy_profile.unwrap_or(state.autonomy_profile);
    state.store.set_autonomy_profile(&thread.id, autonomy)?;
    let cwd = body
        .cwd
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    if let Ok(identity) = state.repos.detect(&cwd) {
        match state.repos.touch(&identity, Some(&thread.id), None, None) {
            Ok((_record, context)) => {
                if let Err(e) = state.store.set_thread_repo(&thread.id, context) {
                    tracing::warn!(thread_id = %thread.id, error = %e, "failed to persist thread repo context");
                }
            }
            Err(e) => {
                tracing::warn!(thread_id = %thread.id, cwd = %cwd.display(), error = %e, "repo index update failed");
            }
        }
    }
    let readiness = calculate_readiness(&state, &thread.id, &cwd, thread.title.as_deref());
    state.store.write_readiness_report(&thread.id, &readiness)?;
    state
        .store
        .set_execution_mode(&thread.id, readiness.suggested_execution_mode)?;
    append_readiness_event(&state, &thread.id, &readiness)?;
    Ok((
        StatusCode::CREATED,
        Json(CreateThreadResponse {
            id: thread.id,
            readiness,
        }),
    ))
}

async fn reconcile_thread(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ReconcileReport>, ApiError> {
    let sessions = state
        .manager
        .list_metas()
        .await
        .into_iter()
        .filter(|meta| meta.thread_id == id)
        .map(session_ref)
        .collect();
    Ok(Json(state.tasks.reconcile(&id, sessions)?))
}

async fn get_timeline(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<TimelineQuery>,
) -> Result<Json<harness_core::TimelineReport>, ApiError> {
    let has_query = q.after.is_some()
        || q.limit.is_some()
        || q.event_type.as_ref().is_some_and(|v| !v.trim().is_empty())
        || q.actor.as_ref().is_some_and(|v| !v.trim().is_empty())
        || q.task_id.as_ref().is_some_and(|v| !v.trim().is_empty())
        || q.session_id.as_ref().is_some_and(|v| !v.trim().is_empty())
        || q.q.as_ref().is_some_and(|v| !v.trim().is_empty());
    let limit = if has_query {
        Some(q.limit.unwrap_or(200).clamp(1, 1000))
    } else {
        None
    };
    let items = state.store.query_timeline(
        &id,
        TimelineQueryOptions {
            after: q.after,
            limit,
            event_type: q.event_type,
            actor: q.actor,
            task_id: q.task_id,
            session_id: q.session_id,
            q: q.q,
        },
    )?;
    let report = harness_core::TimelineReport {
        thread_id: id,
        generated_at: Utc::now().timestamp_millis(),
        event_count: items.len(),
        items,
    };
    Ok(Json(report))
}

fn session_ref(meta: SessionMeta) -> ReconcileSessionRef {
    ReconcileSessionRef {
        session_id: meta.id,
        thread_id: meta.thread_id,
        task_id: meta.task_id,
        parent_session_id: meta.parent_session_id,
        owner_session_id: meta.owner_session_id,
        root_session_id: if meta.root_session_id.is_empty() {
            None
        } else {
            Some(meta.root_session_id)
        },
        status: match meta.status {
            SessionStatus::Running => "running",
            SessionStatus::Exited => "exited",
            SessionStatus::Killed => "killed",
        }
        .into(),
    }
}

async fn set_autonomy(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<SetAutonomyRequest>,
) -> Result<Json<Thread>, ApiError> {
    let thread = state
        .store
        .set_autonomy_profile(&id, body.autonomy_profile)?;
    let event = Event {
        seq: 0,
        at: Utc::now().timestamp_millis(),
        event_type: "thread.autonomy.changed".to_string(),
        items: vec![Item::Text {
            text: serde_json::to_string(&json!({
                "autonomy_profile": body.autonomy_profile,
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        }],
        thread_id: Some(id.clone()),
        actor: None,
        payload: Some(json!({
            "autonomy_profile": body.autonomy_profile,
        })),
    };
    state.store.append_event(&id, &event)?;
    Ok(Json(thread))
}

async fn get_readiness(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<ReadinessReport>, ApiError> {
    state.store.get_thread(&id)?;
    if let Some(report) = state.store.read_readiness_report(&id)? {
        return Ok(Json(report));
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let thread = state.store.get_thread(&id)?;
    let report = calculate_readiness(&state, &id, &cwd, thread.title.as_deref());
    state.store.write_readiness_report(&id, &report)?;
    state
        .store
        .set_execution_mode(&id, report.suggested_execution_mode)?;
    append_readiness_event(&state, &id, &report)?;
    Ok(Json(report))
}

async fn recalculate_readiness(
    State(state): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Query(q): Query<ReadinessQuery>,
) -> Result<Json<ReadinessReport>, ApiError> {
    let thread = state.store.get_thread(&id)?;
    let cwd = q
        .cwd
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let report = calculate_readiness(&state, &id, &cwd, thread.title.as_deref());
    state.store.write_readiness_report(&id, &report)?;
    state
        .store
        .set_execution_mode(&id, report.suggested_execution_mode)?;
    append_readiness_event(&state, &id, &report)?;
    Ok(Json(report))
}

fn append_readiness_event(
    state: &AppState,
    thread_id: &str,
    report: &ReadinessReport,
) -> Result<(), ApiError> {
    let event = Event {
        seq: 0,
        at: Utc::now().timestamp_millis(),
        event_type: "thread.readiness.checked".to_string(),
        items: vec![Item::Text {
            text: serde_json::to_string(&json!({
                "status": report.status,
                "suggested_execution_mode": report.suggested_execution_mode,
                "blocking": report.blocking.len(),
                "warnings": report.warnings.len(),
            }))
            .unwrap_or_else(|_| "{}".to_string()),
        }],
        thread_id: Some(thread_id.to_string()),
        actor: None,
        payload: Some(json!({
            "status": report.status,
            "suggested_execution_mode": report.suggested_execution_mode,
            "blocking": report.blocking.len(),
            "warnings": report.warnings.len(),
        })),
    };
    state.store.append_event(thread_id, &event)?;
    Ok(())
}

fn calculate_readiness(
    state: &AppState,
    thread_id: &str,
    cwd: &Path,
    title: Option<&str>,
) -> ReadinessReport {
    let checked_at = Utc::now().timestamp_millis();
    let mut blocking = Vec::new();
    let mut warnings = Vec::new();

    check_repo(cwd, &mut blocking, &mut warnings);
    let command_facts = check_commands(&mut warnings);
    let cli_facts = check_cli_auth(state, &mut blocking, &mut warnings);
    let env_facts = check_env(cwd, &mut blocking, &mut warnings);
    let deps_facts = check_deps(cwd, &mut warnings);
    let port_facts = check_ports(cwd, &mut warnings);
    let budget_facts = check_budget(state, thread_id, &mut blocking, &mut warnings);
    let external_facts = check_external_resources(title, &mut warnings);
    let codebase_memory = check_codebase_memory(cwd);
    let suggested_execution_mode = suggest_execution_mode(title, &blocking, &warnings);

    let facts = json!({
        "cwd": cwd.display().to_string(),
        "commands": command_facts,
        "agent_clis": cli_facts,
        "env": env_facts,
        "deps": deps_facts,
        "ports": port_facts,
        "budget": budget_facts,
        "external_resources": external_facts,
        "codebase_memory": codebase_memory,
    });

    ReadinessReport::new(
        checked_at,
        cwd.display().to_string(),
        blocking,
        warnings,
        facts,
        suggested_execution_mode,
    )
}

fn check_codebase_memory(cwd: &Path) -> serde_json::Value {
    let binary = which::which("codebase-memory-mcp").ok();
    let markers = [
        cwd.join(".codebase-memory"),
        cwd.join(".codebase-memory-mcp"),
        cwd.join(".cbm"),
    ];
    let marker = markers
        .iter()
        .find(|p| p.exists())
        .map(|p| p.display().to_string());
    json!({
        "installed": binary.is_some(),
        "binary": binary.map(|p| p.display().to_string()),
        "index_marker": marker,
        "recommended_for": ["project", "exploratory", "large_repo", "unknown_stack"],
        "install_hint": "Optional: install codebase-memory-mcp for structural code intelligence and fast project indexing."
    })
}

fn check_repo(cwd: &Path, blocking: &mut Vec<ReadinessIssue>, warnings: &mut Vec<ReadinessIssue>) {
    if !cwd.exists() {
        blocking.push(ReadinessIssue::new(
            "repo.cwd_missing",
            "repo",
            format!("Working directory does not exist: {}", cwd.display()),
            Some("Choose an existing project directory before starting work".to_string()),
        ));
        return;
    }
    if !cwd.is_dir() {
        blocking.push(ReadinessIssue::new(
            "repo.cwd_not_dir",
            "repo",
            format!("Working directory is not a directory: {}", cwd.display()),
            Some("Use a directory as the working directory".to_string()),
        ));
        return;
    }
    if !cwd.join(".git").exists() {
        warnings.push(ReadinessIssue::new(
            "repo.no_git",
            "repo",
            "No .git directory found in the working directory",
            Some("Initialize git or accept reduced audit/recovery context".to_string()),
        ));
    }
    if !cwd.join("AGENTS.md").exists() && !cwd.join("ARCHITECTURE.md").exists() {
        warnings.push(ReadinessIssue::new(
            "repo.no_context_doc",
            "repo",
            "No AGENTS.md or ARCHITECTURE.md found for project context",
            Some("Add AGENTS.md or run repo analysis before long autonomous work".to_string()),
        ));
    }
}

fn check_commands(warnings: &mut Vec<ReadinessIssue>) -> serde_json::Value {
    let mut facts = serde_json::Map::new();
    for cmd in ["git", "just", "cargo", "pnpm", "docker"] {
        match which::which(cmd) {
            Ok(path) => {
                facts.insert(
                    cmd.to_string(),
                    json!({ "status": "present", "path": path }),
                );
            }
            Err(_) => {
                facts.insert(cmd.to_string(), json!({ "status": "missing" }));
                warnings.push(ReadinessIssue::new(
                    format!("command.missing.{cmd}"),
                    "commands",
                    format!("Command `{cmd}` was not found on PATH"),
                    Some(format!(
                        "Install `{cmd}` or adjust PATH before tasks that need it"
                    )),
                ));
            }
        }
    }
    serde_json::Value::Object(facts)
}

fn check_cli_auth(
    state: &AppState,
    blocking: &mut Vec<ReadinessIssue>,
    warnings: &mut Vec<ReadinessIssue>,
) -> serde_json::Value {
    let home = std::env::var("HOME").map(PathBuf::from).ok();
    let cli_dirs = [
        ("claude", ".claude", harness_session::AgentKind::Claude),
        ("codex", ".codex", harness_session::AgentKind::Codex),
        ("cursor", ".cursor", harness_session::AgentKind::Cursor),
        (
            "antigravity",
            ".antigravity",
            harness_session::AgentKind::Antigravity,
        ),
    ];
    let mut facts = serde_json::Map::new();
    let mut available_count = 0usize;
    for (name, dir, kind) in cli_dirs {
        let binary = state.binaries.get(&kind);
        let auth_dir = home.as_ref().map(|h| h.join(dir));
        let auth_present = auth_dir.as_ref().is_some_and(|p| p.exists());
        if binary.is_some() {
            available_count += 1;
        }
        if binary.is_some() && !auth_present {
            warnings.push(ReadinessIssue::new(
                format!("cli_auth.missing.{name}"),
                "cli_auth",
                format!("`{name}` binary is available but auth directory `{dir}` was not found"),
                Some(format!(
                    "Run `{}` login on the host before autonomous work",
                    name
                )),
            ));
        }
        facts.insert(
            name.to_string(),
            json!({
                "binary": binary.map(|p| p.display().to_string()),
                "auth_dir": auth_dir.map(|p| p.display().to_string()),
                "auth_present": auth_present,
            }),
        );
    }
    if available_count == 0 {
        blocking.push(ReadinessIssue::new(
            "cli.none_available",
            "cli_auth",
            "No supported agent CLI binaries were found on PATH",
            Some("Install at least Claude Code or Codex before starting agent work".to_string()),
        ));
    }
    serde_json::Value::Object(facts)
}

fn check_env(
    cwd: &Path,
    blocking: &mut Vec<ReadinessIssue>,
    warnings: &mut Vec<ReadinessIssue>,
) -> serde_json::Value {
    let example = cwd.join(".env.example");
    let env_file = cwd.join(".env");
    let env_values = parse_env_file(&env_file);
    let mut required_missing = Vec::new();
    let mut optional_missing = Vec::new();

    if example.exists() {
        for (key, value) in parse_env_file(&example) {
            let present = std::env::var(&key)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
                || env_values
                    .get(&key)
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false);
            if present {
                continue;
            }
            if value.trim().is_empty() {
                required_missing.push(key);
            } else {
                optional_missing.push(key);
            }
        }
    }

    for key in &required_missing {
        blocking.push(ReadinessIssue::new(
            format!("env.missing_required.{key}"),
            "env",
            format!("Required env var `{key}` is missing"),
            Some(format!(
                "Define `{key}` in the process environment or in {}",
                env_file.display()
            )),
        ));
    }
    for key in &optional_missing {
        warnings.push(ReadinessIssue::new(
            format!("env.missing_optional.{key}"),
            "env",
            format!("Optional/sample env var `{key}` is not set"),
            Some(format!("Set `{key}` if this task needs that integration")),
        ));
    }

    json!({
        "env_example": example.exists(),
        "env_file": env_file.exists(),
        "required_missing": required_missing,
        "optional_missing": optional_missing,
    })
}

fn check_deps(cwd: &Path, warnings: &mut Vec<ReadinessIssue>) -> serde_json::Value {
    let js_roots = [cwd.to_path_buf(), cwd.join("frontend")]
        .into_iter()
        .filter(|root| root.join("package.json").exists())
        .map(|root| {
            let node_modules = root.join("node_modules");
            let lockfile = [
                "pnpm-lock.yaml",
                "package-lock.json",
                "yarn.lock",
                "bun.lockb",
            ]
            .iter()
            .find(|name| root.join(name).exists())
            .copied();
            if !node_modules.exists() {
                warnings.push(ReadinessIssue::new(
                    format!(
                        "deps.node_modules_missing.{}",
                        readiness_path_id(cwd, &root)
                    ),
                    "deps",
                    format!("Node dependencies are not installed in {}", root.display()),
                    Some("Run `pnpm install` before frontend or full-stack work".to_string()),
                ));
            }
            json!({
                "root": root.display().to_string(),
                "package_json": true,
                "lockfile": lockfile,
                "node_modules": node_modules.exists(),
            })
        })
        .collect::<Vec<_>>();

    let rust_roots = [cwd.to_path_buf(), cwd.join("backend")]
        .into_iter()
        .filter(|root| root.join("Cargo.toml").exists())
        .map(|root| {
            json!({
                "root": root.display().to_string(),
                "cargo_toml": true,
                "cargo_lock": root.join("Cargo.lock").exists(),
                "target_dir": root.join("target").exists(),
            })
        })
        .collect::<Vec<_>>();

    json!({
        "javascript": js_roots,
        "rust": rust_roots,
    })
}

fn check_ports(cwd: &Path, warnings: &mut Vec<ReadinessIssue>) -> serde_json::Value {
    let env_file = parse_env_file(&cwd.join(".env"));
    let mut facts = serde_json::Map::new();

    for key in ["FRONTEND_PORT", "CRAWL4AI_PORT"] {
        let raw = std::env::var(key)
            .ok()
            .or_else(|| env_file.get(key).cloned());
        let Some(raw) = raw else {
            facts.insert(key.to_string(), json!({ "configured": false }));
            continue;
        };
        match raw.parse::<u16>() {
            Ok(port) => {
                let available = local_port_available(port);
                if !available {
                    warnings.push(ReadinessIssue::new(
                        format!("ports.unavailable.{key}"),
                        "ports",
                        format!("Configured port `{key}={port}` is already in use"),
                        Some(format!(
                            "Choose a free port for `{key}` or stop the process using it"
                        )),
                    ));
                }
                facts.insert(
                    key.to_string(),
                    json!({ "configured": true, "port": port, "available": available }),
                );
            }
            Err(_) => {
                warnings.push(ReadinessIssue::new(
                    format!("ports.invalid.{key}"),
                    "ports",
                    format!("Configured port `{key}={raw}` is not a valid TCP port"),
                    Some(format!("Set `{key}` to a number between 1 and 65535")),
                ));
                facts.insert(
                    key.to_string(),
                    json!({ "configured": true, "raw": raw, "valid": false }),
                );
            }
        }
    }

    serde_json::Value::Object(facts)
}

fn local_port_available(port: u16) -> bool {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    TcpListener::bind(addr).is_ok()
}

fn check_budget(
    state: &AppState,
    thread_id: &str,
    blocking: &mut Vec<ReadinessIssue>,
    warnings: &mut Vec<ReadinessIssue>,
) -> serde_json::Value {
    let budget = state.budgets.get(thread_id);
    if budget.limit_usd <= 0.0 {
        warnings.push(ReadinessIssue::new(
            "budget.no_limit",
            "budget",
            "No budget limit is configured for this thread",
            Some("Set a thread budget before autonomous/project mode work".to_string()),
        ));
    } else if budget.over_hard() {
        blocking.push(ReadinessIssue::new(
            "budget.hard_cap_reached",
            "budget",
            format!(
                "Budget hard cap is already reached: ${:.4} spent of ${:.4}",
                budget.spent_usd, budget.limit_usd
            ),
            Some("Increase the budget limit or reduce spend before resuming work".to_string()),
        ));
    } else if budget.over_soft() {
        warnings.push(ReadinessIssue::new(
            "budget.soft_cap_reached",
            "budget",
            format!(
                "Budget soft cap is reached: {}% of ${:.4}",
                budget.pct_spent(),
                budget.limit_usd
            ),
            Some("Review spend before spawning additional agents".to_string()),
        ));
    }

    json!({
        "limit_usd": budget.limit_usd,
        "spent_usd": budget.spent_usd,
        "pct": budget.pct_spent(),
        "soft_pct": budget.soft_pct,
        "hard_pct": budget.hard_pct,
        "max_concurrent_workers": budget.max_concurrent_workers,
    })
}

fn check_external_resources(
    title: Option<&str>,
    warnings: &mut Vec<ReadinessIssue>,
) -> serde_json::Value {
    let title = title.unwrap_or_default();
    let needs_docs = looks_like_external_resource_request(title);
    let crawl4ai_url = std::env::var("CRAWL4AI_MCP_URL").ok();
    let crawl4ai_binary = which::which("crawl4ai").ok();
    if needs_docs && crawl4ai_url.is_none() && crawl4ai_binary.is_none() {
        warnings.push(ReadinessIssue::new(
            "external.docs_tool_missing",
            "external_resources",
            "Request appears to need external documentation, but no crawl4ai endpoint or binary is configured",
            Some("Set CRAWL4AI_MCP_URL or install crawl4ai before docs-heavy autonomous work".to_string()),
        ));
    }

    json!({
        "needs_external_docs": needs_docs,
        "crawl4ai_url": crawl4ai_url,
        "crawl4ai_binary": crawl4ai_binary.map(|p| p.display().to_string()),
    })
}

fn looks_like_external_resource_request(title: &str) -> bool {
    let lower = title.to_ascii_lowercase();
    lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("docs")
        || lower.contains("documentation")
        || lower.contains("documentacion")
        || lower.contains("documentación")
}

fn readiness_path_id(cwd: &Path, path: &Path) -> String {
    let id = path
        .strip_prefix(cwd)
        .unwrap_or(path)
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(".")
        .replace(|c: char| !c.is_ascii_alphanumeric() && c != '.', "_");
    if id.is_empty() {
        "root".into()
    } else {
        id
    }
}

fn parse_env_file(path: &Path) -> HashMap<String, String> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        out.insert(key.to_string(), value);
    }
    out
}

fn suggest_execution_mode(
    title: Option<&str>,
    blocking: &[ReadinessIssue],
    warnings: &[ReadinessIssue],
) -> ExecutionMode {
    if !blocking.is_empty() {
        return ExecutionMode::Blocked;
    }
    let title = title.unwrap_or_default().to_lowercase();
    if title.contains("analiza")
        || title.contains("analyze")
        || title.contains("investiga")
        || title.contains("review")
        || title.contains("revisa")
    {
        return ExecutionMode::Exploratory;
    }
    if title.contains("app")
        || title.contains("project")
        || title.contains("proyecto")
        || title.contains("feature completa")
    {
        return ExecutionMode::Project;
    }
    if warnings.len() <= 2 {
        ExecutionMode::Quick
    } else {
        ExecutionMode::Standard
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn readiness_path_id_uses_root_fallback_and_relative_segments() {
        let dir = tempdir().unwrap();
        assert_eq!(readiness_path_id(dir.path(), dir.path()), "root");
        assert_eq!(
            readiness_path_id(dir.path(), &dir.path().join("frontend/app")),
            "frontend.app"
        );
    }

    #[test]
    fn external_resource_detection_catches_urls_and_docs() {
        assert!(looks_like_external_resource_request(
            "Read https://example.com docs before coding"
        ));
        assert!(looks_like_external_resource_request(
            "actualiza la documentación de auth"
        ));
        assert!(!looks_like_external_resource_request("rename local helper"));
    }

    #[test]
    fn deps_check_warns_when_frontend_dependencies_are_missing() {
        let dir = tempdir().unwrap();
        let frontend = dir.path().join("frontend");
        std::fs::create_dir_all(&frontend).unwrap();
        std::fs::write(frontend.join("package.json"), "{}").unwrap();
        std::fs::write(frontend.join("pnpm-lock.yaml"), "").unwrap();

        let mut warnings = Vec::new();
        let facts = check_deps(dir.path(), &mut warnings);

        assert!(warnings
            .iter()
            .any(|issue| issue.id == "deps.node_modules_missing.frontend"));
        assert_eq!(facts["javascript"][0]["node_modules"], false);
        assert_eq!(facts["javascript"][0]["lockfile"], "pnpm-lock.yaml");
    }

    #[test]
    fn local_port_available_detects_bound_port() {
        let listener =
            TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).unwrap();
        let port = listener.local_addr().unwrap().port();

        assert!(!local_port_available(port));
    }
}
