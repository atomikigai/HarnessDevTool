use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::Utc;
use harness_core::{validate_thread_id, TaskEvent};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

const MAX_SPEC_BYTES: usize = 1_048_576;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/threads/:tid/spec", get(read).put(write))
        .route("/api/threads/:tid/spec/sections/:section", put(set_section))
}

#[derive(Debug, Serialize)]
pub struct ReadResponse {
    pub content: String,
    pub etag: String,
    pub version: u64,
}

async fn read(
    State(s): State<Arc<AppState>>,
    AxumPath(tid): AxumPath<String>,
) -> ApiResult<Json<ReadResponse>> {
    validate_thread_id(&tid).map_err(ApiError::BadRequest)?;
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
    let version = spec_version(&spec_events_path(&s.harness_home, &tid))
        .map_err(|e| ApiError::Internal(format!("read spec version: {e}")))?;
    Ok(Json(ReadResponse {
        content,
        etag,
        version,
    }))
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
    pub version: u64,
    pub bytes: u64,
    pub created: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetSectionBody {
    pub content: String,
    #[serde(default)]
    pub spec_version_required: Option<u64>,
    #[serde(default)]
    pub by: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SetSectionResponse {
    pub etag: String,
    pub version: u64,
    pub section: String,
    pub section_version: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecChangeRecord {
    version: u64,
    section: String,
    section_version: u64,
    etag: String,
    bytes: u64,
    by: String,
    at: i64,
}

async fn write(
    State(s): State<Arc<AppState>>,
    AxumPath(tid): AxumPath<String>,
    Json(body): Json<WriteBody>,
) -> Result<Json<WriteResponse>, Response> {
    validate_thread_id(&tid)
        .map_err(ApiError::BadRequest)
        .map_err(IntoResponse::into_response)?;
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
    let version = append_spec_change(
        &s.harness_home,
        &tid,
        "__full__",
        bytes,
        &etag,
        "legacy_put",
    )
    .map_err(|e| ApiError::Internal(format!("append spec change: {e}")))
    .map_err(IntoResponse::into_response)?;
    s.tasks.emit(
        &tid,
        TaskEvent::SpecChanged {
            thread_id: tid.clone(),
            etag: etag.clone(),
            version: version.version,
            section: None,
            section_version: None,
            bytes,
            at: Utc::now(),
        },
    );
    if created {
        s.tasks.emit(
            &tid,
            TaskEvent::ArtifactAdded {
                thread_id: tid.clone(),
                artifact_id: format!("spec-v{}", version.version),
                task_id: String::new(),
                path: "spec.md".to_string(),
                kind: "spec".to_string(),
                produced_by: "legacy_put".to_string(),
                summary: "Thread spec created".to_string(),
                at: Utc::now(),
            },
        );
    }

    Ok(Json(WriteResponse {
        etag,
        version: version.version,
        bytes,
        created,
    }))
}

async fn set_section(
    State(s): State<Arc<AppState>>,
    AxumPath((tid, section)): AxumPath<(String, String)>,
    Json(body): Json<SetSectionBody>,
) -> Result<Json<SetSectionResponse>, Response> {
    validate_thread_id(&tid)
        .map_err(ApiError::BadRequest)
        .map_err(IntoResponse::into_response)?;
    validate_section(&section).map_err(IntoResponse::into_response)?;
    validate_content(&body.content).map_err(IntoResponse::into_response)?;

    let events_path = spec_events_path(&s.harness_home, &tid);
    let current_version = spec_version(&events_path)
        .map_err(|e| ApiError::Internal(format!("read spec version: {e}")))
        .map_err(IntoResponse::into_response)?;
    if let Some(required) = body.spec_version_required {
        if required != current_version {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "spec_version_mismatch",
                    "current_version": current_version
                })),
            )
                .into_response());
        }
    }

    let path = spec_path(&s.harness_home, &tid);
    let current = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(ApiError::Internal(format!("read current spec: {e}")).into_response());
        }
    };
    let next_content = set_marked_section(&current, &section, &body.content);
    write_materialized_spec(&path, &next_content).map_err(IntoResponse::into_response)?;

    let etag = sha256_hex(next_content.as_bytes());
    let bytes = next_content.len() as u64;
    let by = body.by.as_deref().unwrap_or("spec.set_section");
    let record = append_spec_change(&s.harness_home, &tid, &section, bytes, &etag, by)
        .map_err(|e| ApiError::Internal(format!("append spec change: {e}")))
        .map_err(IntoResponse::into_response)?;

    s.tasks.emit(
        &tid,
        TaskEvent::SpecChanged {
            thread_id: tid.clone(),
            etag: etag.clone(),
            version: record.version,
            section: Some(section.clone()),
            section_version: Some(record.section_version),
            bytes,
            at: Utc::now(),
        },
    );

    Ok(Json(SetSectionResponse {
        etag,
        version: record.version,
        section,
        section_version: record.section_version,
        bytes,
    }))
}

fn spec_path(home: &Path, thread_id: &str) -> PathBuf {
    home.join("profiles")
        .join("default")
        .join("threads")
        .join(thread_id)
        .join("spec.md")
}

fn spec_events_path(home: &Path, thread_id: &str) -> PathBuf {
    home.join("profiles")
        .join("default")
        .join("threads")
        .join(thread_id)
        .join("spec.events.jsonl")
}

fn write_materialized_spec(path: &Path, content: &str) -> ApiResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ApiError::Internal("invalid spec path".to_string()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| ApiError::Internal(format!("create spec parent: {e}")))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| ApiError::Internal(format!("create temp spec: {e}")))?;
    tmp.write_all(content.as_bytes())
        .map_err(|e| ApiError::Internal(format!("write temp spec: {e}")))?;
    tmp.flush()
        .map_err(|e| ApiError::Internal(format!("flush temp spec: {e}")))?;
    tmp.persist(path)
        .map_err(|e| ApiError::Internal(format!("persist spec: {}", e.error)))?;
    Ok(())
}

fn spec_version(path: &Path) -> std::io::Result<u64> {
    Ok(read_spec_changes(path)?
        .last()
        .map(|r| r.version)
        .unwrap_or(0))
}

fn read_spec_changes(path: &Path) -> std::io::Result<Vec<SpecChangeRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = std::fs::File::open(path)?;
    let mut out = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<SpecChangeRecord>(&line) {
            Ok(record) => out.push(record),
            Err(error) => {
                tracing::warn!(path = %path.display(), error = %error, "skipping corrupt spec event");
            }
        }
    }
    Ok(out)
}

fn append_spec_change(
    home: &Path,
    thread_id: &str,
    section: &str,
    bytes: u64,
    etag: &str,
    by: &str,
) -> std::io::Result<SpecChangeRecord> {
    let path = spec_events_path(home, thread_id);
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("invalid spec events path"))?;
    std::fs::create_dir_all(parent)?;
    let existing = read_spec_changes(&path)?;
    let version = existing.last().map(|r| r.version + 1).unwrap_or(1);
    let section_version = existing
        .iter()
        .filter(|record| record.section == section)
        .map(|record| record.section_version)
        .max()
        .unwrap_or(0)
        + 1;
    let record = SpecChangeRecord {
        version,
        section: section.to_string(),
        section_version,
        etag: etag.to_string(),
        bytes,
        by: by.to_string(),
        at: Utc::now().timestamp_millis(),
    };
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(serde_json::to_string(&record)?.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(record)
}

fn set_marked_section(current: &str, section: &str, section_content: &str) -> String {
    let start = format!("<!-- harness:section {section} -->");
    let end = format!("<!-- /harness:section {section} -->");
    let replacement = format!("{start}\n{}\n{end}", section_content.trim_matches('\n'));
    let Some(start_idx) = current.find(&start) else {
        let mut next = current.trim_end().to_string();
        if !next.is_empty() {
            next.push_str("\n\n");
        }
        next.push_str(&replacement);
        next.push('\n');
        return next;
    };
    let Some(end_rel) = current[start_idx..].find(&end) else {
        let mut next = current.trim_end().to_string();
        next.push_str("\n\n");
        next.push_str(&replacement);
        next.push('\n');
        return next;
    };
    let end_idx = start_idx + end_rel + end.len();
    let mut next = String::new();
    next.push_str(&current[..start_idx]);
    next.push_str(&replacement);
    next.push_str(&current[end_idx..]);
    next
}

fn validate_section(section: &str) -> ApiResult<()> {
    let valid = !section.is_empty()
        && section.len() <= 128
        && section
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'));
    if valid {
        Ok(())
    } else {
        Err(ApiError::BadRequest(
            "section must be 1-128 chars of [A-Za-z0-9_.:-]".to_string(),
        ))
    }
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
                api_token: None,
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
        assert_eq!(body["version"], 1);
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
        assert_eq!(body["version"], 1);

        let ev = rx.recv().await.unwrap();
        assert!(matches!(
            ev,
            TaskEvent::SpecChanged {
                thread_id,
                version: 1,
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

    #[tokio::test]
    async fn set_section_versions_and_rejects_stale_spec_version() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path().to_path_buf());
        let app = router().with_state(state.clone());
        let mut rx = state.tasks.subscribe("t1");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/threads/t1/spec/sections/requirements")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"content":"First section","spec_version_required":0,"by":"planner"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body["version"], 1);
        assert_eq!(body["section"], "requirements");
        assert_eq!(body["section_version"], 1);

        let ev = rx.recv().await.unwrap();
        assert!(matches!(
            ev,
            TaskEvent::SpecChanged {
                thread_id,
                version: 1,
                section: Some(section),
                section_version: Some(1),
                ..
            } if thread_id == "t1" && section == "requirements"
        ));

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
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body["version"], 1);
        assert!(body["content"]
            .as_str()
            .unwrap()
            .contains("<!-- harness:section requirements -->"));

        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/threads/t1/spec/sections/requirements")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"content":"Stale update","spec_version_required":0}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body["error"], "spec_version_mismatch");
        assert_eq!(body["current_version"], 1);
    }
}
