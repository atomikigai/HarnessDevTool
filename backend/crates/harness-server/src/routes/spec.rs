use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use harness_core::TaskEvent;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

const MAX_SPEC_BYTES: usize = 1_048_576;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/threads/:tid/spec", get(read).put(write))
}

#[derive(Debug, Serialize)]
pub struct ReadResponse {
    pub content: String,
    pub etag: String,
}

async fn read(
    State(s): State<Arc<AppState>>,
    AxumPath(tid): AxumPath<String>,
) -> ApiResult<Json<ReadResponse>> {
    validate_thread_id(&tid)?;
    let path = spec_path(&s.harness_home, &tid);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(ApiError::Internal(format!("read spec: {e}"))),
    };
    let content = String::from_utf8(bytes.clone())
        .map_err(|e| ApiError::Internal(format!("read spec utf8: {e}")))?;
    let etag = if bytes.is_empty() && !path.exists() {
        String::new()
    } else {
        sha256_hex(&bytes)
    };
    Ok(Json(ReadResponse { content, etag }))
}

#[derive(Debug, Deserialize)]
pub struct WriteBody {
    pub content: String,
    #[serde(default)]
    pub etag: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WriteResponse {
    pub etag: String,
    pub bytes: u64,
    pub created: bool,
}

async fn write(
    State(s): State<Arc<AppState>>,
    AxumPath(tid): AxumPath<String>,
    Json(body): Json<WriteBody>,
) -> Result<Json<WriteResponse>, Response> {
    validate_thread_id(&tid).map_err(IntoResponse::into_response)?;
    validate_content(&body.content).map_err(IntoResponse::into_response)?;

    let path = spec_path(&s.harness_home, &tid);
    let current = match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(ApiError::Internal(format!("read current spec: {e}")).into_response());
        }
    };
    if let Some(expected) = body.etag.as_deref() {
        let current_etag = current
            .as_deref()
            .map(sha256_hex)
            .unwrap_or_else(String::new);
        if current_etag != expected || current.is_none() {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({ "error": "etag_mismatch", "current_etag": current_etag })),
            )
                .into_response());
        }
    }

    let parent = path
        .parent()
        .ok_or_else(|| ApiError::Internal("invalid spec path".to_string()))
        .map_err(IntoResponse::into_response)?;
    std::fs::create_dir_all(parent)
        .map_err(|e| ApiError::Internal(format!("create spec parent: {e}")))
        .map_err(IntoResponse::into_response)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| ApiError::Internal(format!("create temp spec: {e}")))
        .map_err(IntoResponse::into_response)?;
    tmp.write_all(body.content.as_bytes())
        .map_err(|e| ApiError::Internal(format!("write temp spec: {e}")))
        .map_err(IntoResponse::into_response)?;
    tmp.flush()
        .map_err(|e| ApiError::Internal(format!("flush temp spec: {e}")))
        .map_err(IntoResponse::into_response)?;
    tmp.persist(&path)
        .map_err(|e| ApiError::Internal(format!("persist spec: {}", e.error)))
        .map_err(IntoResponse::into_response)?;

    let etag = sha256_hex(body.content.as_bytes());
    let bytes = body.content.len() as u64;
    let created = current.is_none();
    let tx = s
        .tasks
        .sender(&tid)
        .map_err(ApiError::from)
        .map_err(IntoResponse::into_response)?;
    let _ = tx.send(TaskEvent::SpecChanged {
        thread_id: tid.clone(),
        etag: etag.clone(),
        bytes,
        at: Utc::now(),
    });
    if created {
        let _ = tx.send(TaskEvent::ArtifactAdded {
            thread_id: tid,
            path: "spec.md".to_string(),
            kind: "spec".to_string(),
            at: Utc::now(),
        });
    }

    Ok(Json(WriteResponse {
        etag,
        bytes,
        created,
    }))
}

fn spec_path(home: &Path, thread_id: &str) -> PathBuf {
    home.join("profiles")
        .join("default")
        .join("threads")
        .join(thread_id)
        .join("spec.md")
}

fn validate_thread_id(thread_id: &str) -> ApiResult<()> {
    if thread_id.is_empty() {
        return Err(ApiError::BadRequest(
            "thread_id must not be empty".to_string(),
        ));
    }
    if !thread_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(ApiError::BadRequest("invalid thread_id".to_string()));
    }
    Ok(())
}

fn validate_content(content: &str) -> ApiResult<()> {
    if content.len() > MAX_SPEC_BYTES {
        return Err(ApiError::BadRequest(format!(
            "content exceeds {MAX_SPEC_BYTES} byte limit"
        )));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn state(home: std::path::PathBuf) -> Arc<AppState> {
        Arc::new(
            AppState::new(&Config {
                bind: "127.0.0.1:7777".parse().unwrap(),
                home,
                cors_origin: "http://localhost:8080".to_string(),
                profile: "default".to_string(),
                autonomy_profile: harness_core::AutonomyProfile::Assisted,
            })
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn put_emits_events_and_stale_etag_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path().to_path_buf());
        let app = router().with_state(state.clone());
        let mut rx = state.tasks.subscribe("t1");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/threads/t1/spec")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"content":"hello"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body["created"], true);
        assert_eq!(body["bytes"], 5);
        let etag = body["etag"].as_str().unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/threads/t1/spec")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body["content"], "hello");
        assert_eq!(body["etag"], etag);

        let ev = rx.recv().await.unwrap();
        assert!(matches!(
            ev,
            TaskEvent::SpecChanged {
                thread_id,
                bytes: 5,
                ..
            } if thread_id == "t1"
        ));
        let ev = rx.recv().await.unwrap();
        assert!(matches!(
            ev,
            TaskEvent::ArtifactAdded {
                thread_id,
                path,
                kind,
                ..
            } if thread_id == "t1" && path == "spec.md" && kind == "spec"
        ));

        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/threads/t1/spec")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"content":"new","etag":"stale"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body["error"], "etag_mismatch");
        assert_eq!(body["current_etag"], etag);
    }
}
