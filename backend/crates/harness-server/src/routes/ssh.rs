//! REST surface for `module-ssh`.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use module_ssh::{Host, HostInput, HostTestResult, SftpListResult, SftpTransfer, SshExecResult};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/ssh/hosts", get(list_hosts).post(add_host))
        .route("/api/ssh/hosts/:id", delete(remove_host))
        .route("/api/ssh/hosts/:id/test", post(test_host))
        .route("/api/ssh/hosts/:id/exec", post(ssh_exec))
        .route("/api/ssh/hosts/:id/sftp", get(sftp_list))
        .route("/api/ssh/hosts/:id/sftp/mkdir", post(sftp_mkdir))
        .route("/api/ssh/hosts/:id/sftp/rmdir", post(sftp_rmdir))
        .route("/api/ssh/hosts/:id/sftp/unlink", post(sftp_unlink))
        .route("/api/ssh/hosts/:id/sftp/rename", post(sftp_rename))
        .route("/api/ssh/hosts/:id/sftp/get", post(sftp_get))
        .route("/api/ssh/hosts/:id/sftp/put", post(sftp_put))
        .route("/api/ssh/hosts/:id/sessions", post(open_session))
        .route("/api/ssh/sessions/:id", delete(close_session))
}

#[derive(Debug, Serialize)]
pub struct RemovedResponse {
    removed: bool,
}

#[derive(Debug, Deserialize)]
struct SftpListQuery {
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SftpTransferBody {
    remote_path: String,
    local_path: String,
}

#[derive(Debug, Deserialize)]
struct SshExecBody {
    cmd: String,
}

#[derive(Debug, Deserialize)]
struct SftpPathBody {
    path: String,
}

#[derive(Debug, Deserialize)]
struct SftpRenameBody {
    from_path: String,
    to_path: String,
}

async fn list_hosts(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<Host>>> {
    Ok(Json(state.ssh.list_hosts()?))
}

async fn add_host(
    State(state): State<Arc<AppState>>,
    Json(input): Json<HostInput>,
) -> ApiResult<(StatusCode, Json<Host>)> {
    Ok((StatusCode::CREATED, Json(state.ssh.add_host(input)?)))
}

async fn remove_host(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<RemovedResponse>> {
    Ok(Json(RemovedResponse {
        removed: state.ssh.remove_host(&id)?,
    }))
}

async fn test_host(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<HostTestResult>> {
    Ok(Json(state.ssh.test_host(&id).await?))
}

async fn sftp_list(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(query): Query<SftpListQuery>,
) -> ApiResult<Json<SftpListResult>> {
    let path = query.path.as_deref().unwrap_or(".");
    Ok(Json(state.ssh.sftp_list(&id, path).await?))
}

async fn ssh_exec(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<SshExecBody>,
) -> ApiResult<Json<SshExecResult>> {
    Ok(Json(state.ssh.exec(&id, &body.cmd).await?))
}

async fn sftp_mkdir(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<SftpPathBody>,
) -> ApiResult<Json<SshExecResult>> {
    Ok(Json(state.ssh.sftp_mkdir(&id, &body.path).await?))
}

async fn sftp_rmdir(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<SftpPathBody>,
) -> ApiResult<Json<SshExecResult>> {
    Ok(Json(state.ssh.sftp_rmdir(&id, &body.path).await?))
}

async fn sftp_unlink(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<SftpPathBody>,
) -> ApiResult<Json<SshExecResult>> {
    Ok(Json(state.ssh.sftp_unlink(&id, &body.path).await?))
}

async fn sftp_rename(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<SftpRenameBody>,
) -> ApiResult<Json<SshExecResult>> {
    Ok(Json(
        state
            .ssh
            .sftp_rename(&id, &body.from_path, &body.to_path)
            .await?,
    ))
}

async fn sftp_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<SftpTransferBody>,
) -> ApiResult<Json<SftpTransfer>> {
    Ok(Json(
        state
            .ssh
            .sftp_get(
                &id,
                &body.remote_path,
                std::path::Path::new(&body.local_path),
            )
            .await?,
    ))
}

async fn sftp_put(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<SftpTransferBody>,
) -> ApiResult<Json<SftpTransfer>> {
    Ok(Json(
        state
            .ssh
            .sftp_put(
                &id,
                std::path::Path::new(&body.local_path),
                &body.remote_path,
            )
            .await?,
    ))
}

async fn open_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _ = state.ssh.open_session(&id).await?;
    Ok(StatusCode::CREATED)
}

async fn close_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<RemovedResponse>> {
    Ok(Json(RemovedResponse {
        removed: state.ssh.close_session(&id)?,
    }))
}
