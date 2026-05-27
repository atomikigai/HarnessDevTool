use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use harness_session::{AgentKind, SessionError, SessionMeta, SpawnOpts};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::ApiError;
use crate::state::AppState;

const MAX_INPUT_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub kind: AgentKind,
    #[serde(default)]
    pub cwd: Option<String>,
    /// Optional role-template name (resolved against `AppState.roles`). When
    /// supplied, the role's `prompt_template` is written to the PTY shortly
    /// after spawn.
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ResizeRequest {
    pub cols: u16,
    pub rows: u16,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/threads/:tid/sessions", post(create_session))
        .route("/api/sessions/:sid", get(get_session))
        .route("/api/sessions/:sid/input", post(post_input))
        .route("/api/sessions/:sid/resize", post(post_resize))
        .route("/api/sessions/:sid", delete(kill_session))
}

async fn create_session(
    State(state): State<Arc<AppState>>,
    Path(tid): Path<String>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<CreateSessionResponse>), ApiError> {
    // 1) Thread must exist.
    state.store.get_thread(&tid)?;

    // 2) Binary must be detected.
    let binary = state
        .binaries
        .get(&req.kind)
        .cloned()
        .ok_or(ApiError::from(SessionError::BinaryNotFound(req.kind)))?;

    // 3) Resolve cwd.
    let cwd = match req.cwd {
        Some(c) => PathBuf::from(c),
        None => dirs::home_dir()
            .ok_or_else(|| ApiError::Internal("cannot resolve $HOME for default cwd".into()))?,
    };
    if !cwd.exists() {
        return Err(ApiError::BadRequest(format!(
            "cwd does not exist: {}",
            cwd.display()
        )));
    }

    // 4) Build MCP injection opts (one MCP server per session) and spawn.
    //    The config file path is opaque (a UUID), already known to claude via
    //    --mcp-config; we cannot rename it after spawn because claude resolves
    //    that arg on startup. Instead we remember sid → config_path so kill
    //    can clean up.
    let (mut opts, config_path) = build_spawn_opts(&state, req.kind, &tid)?;

    // 5) Resolve optional role template and seed the initial prompt.
    if let Some(role_name) = req.role.as_deref() {
        let role = state
            .roles
            .get(role_name)
            .ok_or_else(|| ApiError::BadRequest(format!("unknown role: {role_name}")))?;
        opts.role_prompt = Some(role.prompt_template.clone());
        opts.role = Some(role.name.clone());
    }

    let session = state
        .manager
        .spawn_with_opts(req.kind, binary, tid, cwd, opts)?;
    let meta = session.meta().await;
    if let Some(path) = config_path {
        state.mcp_configs.insert(meta.id.clone(), path);
    }
    Ok((
        StatusCode::CREATED,
        Json(CreateSessionResponse {
            session_id: meta.id,
        }),
    ))
}

/// Build `SpawnOpts` carrying the per-session MCP config path. Returns
/// `Ok(SpawnOpts::default())` if MCP injection is disabled (no binary, or
/// the kind doesn't support it yet).
fn build_spawn_opts(
    state: &AppState,
    kind: AgentKind,
    thread_id: &str,
) -> Result<(SpawnOpts, Option<PathBuf>), ApiError> {
    // Codex has no per-invocation MCP flag; skip.
    if matches!(kind, AgentKind::Codex) {
        return Ok((SpawnOpts::default(), None));
    }
    let mcp_bin = match state.mcp_server_binary.as_ref() {
        Some(p) => p.clone(),
        None => {
            tracing::warn!(
                "spawning {kind} without MCP injection (no harness-mcp-server binary)"
            );
            return Ok((SpawnOpts::default(), None));
        }
    };

    // Pre-issue a stable id we can use both for the config filename and for
    // the `--agent-id` arg passed to the MCP server. We can't read the sid
    // the Manager picks until after spawn, but a UUID per spawn request is
    // sufficient — the MCP server identity only needs to be unique enough to
    // distinguish concurrent agents inside a thread.
    let mcp_id = uuid::Uuid::new_v4().to_string();
    let agent_id = format!("agent:{}-{}", kind.as_str(), &mcp_id[..8]);

    let configs_dir = state.harness_home.join(".runtime").join("mcp-configs");
    std::fs::create_dir_all(&configs_dir)
        .map_err(|e| ApiError::Internal(format!("create mcp-configs dir: {e}")))?;
    let config_path = configs_dir.join(format!("{mcp_id}.json"));

    let config = json!({
        "mcpServers": {
            "harness": {
                "command": mcp_bin.display().to_string(),
                "args": [
                    "--thread", thread_id,
                    "--agent-id", agent_id,
                    "--harness-home", state.harness_home.display().to_string(),
                ]
            }
        }
    });
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap())
        .map_err(|e| ApiError::Internal(format!("write mcp config: {e}")))?;
    tracing::info!(
        path = %config_path.display(),
        agent_id = %agent_id,
        "wrote per-session MCP config"
    );

    Ok((
        SpawnOpts {
            mcp_config_path: Some(config_path.clone()),
            ..SpawnOpts::default()
        },
        Some(config_path),
    ))
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
) -> Result<Json<SessionMeta>, ApiError> {
    if let Some(s) = state.manager.get(&sid) {
        return Ok(Json(s.meta().await));
    }
    // Fall back to on-disk meta (session exited and may have been forgotten).
    let path = state.manager.sessions_root().join(&sid).join("meta.json");
    if !path.exists() {
        return Err(ApiError::SessionNotFound(sid));
    }
    let bytes = std::fs::read(&path).map_err(|e| ApiError::Internal(e.to_string()))?;
    let meta: SessionMeta =
        serde_json::from_slice(&bytes).map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(meta))
}

async fn post_input(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    if body.len() > MAX_INPUT_BYTES {
        return Err(ApiError::BadRequest(format!(
            "input exceeds {MAX_INPUT_BYTES} bytes",
        )));
    }
    let session = state
        .manager
        .get(&sid)
        .ok_or_else(|| ApiError::SessionNotFound(sid.clone()))?;
    session.write_input(&body).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn post_resize(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
    Json(req): Json<ResizeRequest>,
) -> Result<StatusCode, ApiError> {
    if req.cols == 0 || req.rows == 0 {
        return Err(ApiError::BadRequest("cols/rows must be > 0".into()));
    }
    let session = state
        .manager
        .get(&sid)
        .ok_or_else(|| ApiError::SessionNotFound(sid.clone()))?;
    session.resize(req.cols, req.rows).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn kill_session(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
) -> Result<StatusCode, ApiError> {
    let session = state
        .manager
        .get(&sid)
        .ok_or_else(|| ApiError::SessionNotFound(sid.clone()))?;
    session.kill().await?;
    // Best-effort: remove the per-session MCP config file via the registry
    // we populated at spawn time. The MCP server child dies on stdio close
    // when claude exits, so there's nothing else to clean up.
    if let Some((_, path)) = state.mcp_configs.remove(&sid) {
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!(path = %path.display(), error = %e, "could not remove mcp config");
            }
        }
    }
    Ok(StatusCode::NO_CONTENT)
}
