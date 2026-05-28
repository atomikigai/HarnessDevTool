//! Profiles (workspaces) — list / create / activate.
//!
//! A "profile" is an isolated workspace inside `$HARNESS_HOME/profiles/<id>/`.
//! Each profile has its own sessions, threads, tasks, budgets, DB connections
//! and (in a later slice) CLI auth state via symlinks.
//!
//! Activation is **NOT hot**: the backend reads the active profile at startup
//! from `HARNESS_PROFILE` env or `$HARNESS_HOME/active_profile`. The activate
//! endpoint persists the chosen id to that pointer file and tells the caller
//! to restart the server. Hot-swap is a future slice.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Path as AxPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/profiles", get(list_profiles).post(create_profile))
        .route("/api/profiles/active", get(get_active))
        .route("/api/profiles/:id/activate", post(activate))
}

/// Per-workspace metadata persisted at `profiles/<id>/workspace.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// Stable profile id (also the directory name under `profiles/`). Kebab.
    pub id: String,
    /// Human-readable name for the UI.
    pub display_name: String,
    /// Absolute path to the project this workspace owns. Sessions spawned in
    /// this profile default their `cwd` to this path. Optional — a profile
    /// can exist without an associated repo (rare).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// RFC 3339 timestamp of profile creation.
    pub created_at: String,
}

impl Workspace {
    fn read(dir: &Path) -> Option<Self> {
        let path = dir.join("workspace.toml");
        let text = std::fs::read_to_string(&path).ok()?;
        toml_edit::de::from_str(&text).ok()
    }
}

#[derive(Debug, Serialize)]
struct ProfileSummary {
    id: String,
    display_name: String,
    path: Option<String>,
    created_at: String,
    /// True when this profile is the one the running backend was started
    /// against (`cfg.profile`).
    active: bool,
}

async fn list_profiles(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<ProfileSummary>>> {
    let profiles_dir = state.harness_home.join("profiles");
    std::fs::create_dir_all(&profiles_dir)
        .map_err(|e| ApiError::Internal(format!("create profiles dir: {e}")))?;

    let mut out: Vec<ProfileSummary> = Vec::new();
    for entry in std::fs::read_dir(&profiles_dir)
        .map_err(|e| ApiError::Internal(format!("read profiles dir: {e}")))?
    {
        let entry = entry.map_err(|e| ApiError::Internal(e.to_string()))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        // Workspaces created the old way (pre-profiles) don't have a
        // workspace.toml — synthesize one so they still appear in the picker.
        let ws = Workspace::read(&path).unwrap_or_else(|| Workspace {
            id: id.clone(),
            display_name: id.clone(),
            path: None,
            created_at: String::new(),
        });
        out.push(ProfileSummary {
            id: ws.id,
            display_name: ws.display_name,
            path: ws.path,
            created_at: ws.created_at,
            active: id == state.profile,
        });
    }

    // Always surface "default" even if its dir doesn't exist yet — the rest
    // of the stack falls back to it.
    if !out.iter().any(|p| p.id == "default") {
        out.push(ProfileSummary {
            id: "default".into(),
            display_name: "default".into(),
            path: None,
            created_at: String::new(),
            active: state.profile == "default",
        });
    }
    out.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
    });
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    /// Stable id (becomes the dir name). Lowercase / kebab recommended.
    id: String,
    display_name: String,
    #[serde(default)]
    path: Option<String>,
}

async fn create_profile(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateBody>,
) -> ApiResult<(StatusCode, Json<ProfileSummary>)> {
    let id = body.id.trim().to_string();
    if id.is_empty() {
        return Err(ApiError::BadRequest("id required".into()));
    }
    // Defensive — only allow filesystem-safe ids. No path traversal.
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(ApiError::BadRequest(
            "id must be ascii alphanumeric + `-` or `_`".into(),
        ));
    }

    let dir = state.harness_home.join("profiles").join(&id);
    if dir.exists() {
        return Err(ApiError::BadRequest(format!(
            "profile '{id}' already exists"
        )));
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| ApiError::Internal(format!("create profile dir: {e}")))?;

    // Validate path if given.
    let cwd = match body
        .path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(p) => {
            let pb = PathBuf::from(p);
            if !pb.exists() {
                return Err(ApiError::BadRequest(format!(
                    "path does not exist: {}",
                    pb.display()
                )));
            }
            Some(pb.to_string_lossy().to_string())
        }
        None => None,
    };

    let ws = Workspace {
        id: id.clone(),
        display_name: body.display_name.trim().to_string(),
        path: cwd.clone(),
        created_at: Utc::now().to_rfc3339(),
    };
    let toml = toml_edit::ser::to_string_pretty(&ws)
        .map_err(|e| ApiError::Internal(format!("serialize workspace: {e}")))?;
    std::fs::write(dir.join("workspace.toml"), toml)
        .map_err(|e| ApiError::Internal(format!("write workspace.toml: {e}")))?;

    Ok((
        StatusCode::CREATED,
        Json(ProfileSummary {
            id: ws.id,
            display_name: ws.display_name,
            path: ws.path,
            created_at: ws.created_at,
            active: false,
        }),
    ))
}

#[derive(Debug, Serialize)]
struct ActiveResponse {
    /// The profile id this backend is currently serving (loaded at startup).
    active: String,
    /// Pointer file contents — what will be picked up on next restart. May
    /// differ from `active` after the user calls activate.
    pending: Option<String>,
}

async fn get_active(State(state): State<Arc<AppState>>) -> ApiResult<Json<ActiveResponse>> {
    let pointer = state.harness_home.join("active_profile");
    let pending = std::fs::read_to_string(&pointer)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(Json(ActiveResponse {
        active: state.profile.clone(),
        pending,
    }))
}

#[derive(Debug, Serialize)]
struct ActivateResponse {
    /// Profile that will be loaded on next backend restart.
    pending: String,
    /// Whether the backend currently serves a different profile (so the
    /// caller must restart to see the switch take effect).
    requires_restart: bool,
}

async fn activate(
    State(state): State<Arc<AppState>>,
    AxPath(id): AxPath<String>,
) -> ApiResult<Json<ActivateResponse>> {
    let dir = state.harness_home.join("profiles").join(&id);
    if !dir.exists() {
        // Allow activating "default" even if its dir is empty — it gets
        // materialised by the stores on first use.
        if id != "default" {
            return Err(ApiError::NotFound(format!("profile '{id}'")));
        }
    }
    let pointer = state.harness_home.join("active_profile");
    std::fs::write(&pointer, id.as_bytes())
        .map_err(|e| ApiError::Internal(format!("write active_profile: {e}")))?;

    let needs_swap = id != state.profile;
    if needs_swap {
        // Trigger the in-process reload — `main` is looping on a Notify and
        // will rebuild AppState against the new active_profile we just
        // persisted. The client will get a brief connection blip while the
        // axum graceful shutdown completes; the frontend handles that by
        // re-polling /health.
        tracing::info!(
            current = %state.profile,
            next = %id,
            "profile activate: triggering hot-swap"
        );
        crate::trigger_reload();
    }
    Ok(Json(ActivateResponse {
        pending: id.clone(),
        // Hot-swap is automatic now; the field stays for backwards compat
        // but always reports `false` when the swap was successfully fired.
        requires_restart: false,
    }))
}
