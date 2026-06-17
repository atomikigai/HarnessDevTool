use std::sync::Arc;

use axum::extract::{Multipart, Path, State};
use axum::http::{header, StatusCode};
use axum::{response::Response, Json};
use serde::Serialize;

use crate::error::ApiError;
use crate::state::AppState;

/// Per-attachment hard cap. The MCP `attach.read` tool base64-encodes bytes
/// back, so anything north of ~100 MiB hurts more than it helps.
const MAX_ATTACHMENT_BYTES: usize = 100 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct AttachedFile {
    pub name: String,
    pub size: u64,
    pub mime: String,
    pub path: String,
}

pub async fn attach_files(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<Vec<AttachedFile>>, ApiError> {
    state
        .manager
        .get(&sid)
        .ok_or_else(|| ApiError::NotFound(format!("session {sid}")))?;

    let dir = state.harness_home.join(".runtime/attach").join(&sid);
    std::fs::create_dir_all(&dir).map_err(ApiError::internal)?;

    let mut saved: Vec<AttachedFile> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("multipart error: {e}")))?
    {
        let raw_name = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("attachment-{}", uuid::Uuid::new_v4()));
        let safe_name = sanitize_filename(&raw_name);
        let declared_mime = field
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "application/octet-stream".into());

        let data = field
            .bytes()
            .await
            .map_err(|e| ApiError::BadRequest(format!("read body: {e}")))?;

        if data.len() > MAX_ATTACHMENT_BYTES {
            return Err(ApiError::BadRequest(format!(
                "attachment '{safe_name}' is {} bytes; limit is {} bytes",
                data.len(),
                MAX_ATTACHMENT_BYTES
            )));
        }

        let target = dir.join(&safe_name);
        std::fs::write(&target, &data).map_err(ApiError::internal)?;

        saved.push(AttachedFile {
            name: safe_name,
            size: data.len() as u64,
            mime: declared_mime,
            path: target.to_string_lossy().to_string(),
        });
    }

    if saved.is_empty() {
        return Err(ApiError::BadRequest(
            "no file parts in multipart body".into(),
        ));
    }

    tracing::info!(session = %sid, count = saved.len(), "attached files");
    Ok(Json(saved))
}

pub async fn list_attachments(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
) -> Result<Json<Vec<AttachedFile>>, ApiError> {
    state
        .manager
        .get(&sid)
        .ok_or_else(|| ApiError::NotFound(format!("session {sid}")))?;

    let dir = state.harness_home.join(".runtime/attach").join(&sid);
    if !dir.exists() {
        return Ok(Json(Vec::new()));
    }
    let mut out: Vec<AttachedFile> = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(ApiError::internal)? {
        let entry = entry.map_err(ApiError::internal)?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let meta = entry.metadata().map_err(ApiError::internal)?;
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let mime = attachment_content_type(&name).to_string();
        out.push(AttachedFile {
            name,
            size: meta.len(),
            mime,
            path: path.to_string_lossy().to_string(),
        });
    }
    Ok(Json(out))
}

// This route is deliberately reachable without Authorization or
// X-Protocol-Version because browsers load it directly via plain <img src>.
// Strict path validation and CSP sandbox compensate for that exception.
pub async fn get_attachment(
    State(state): State<Arc<AppState>>,
    Path((sid, name)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    if !is_safe_attachment_segment(&sid) {
        return Err(ApiError::BadRequest(format!("invalid session id '{sid}'")));
    }
    if !is_safe_attachment_segment(&name) || sanitize_filename(&name) != name {
        return Err(ApiError::BadRequest(format!(
            "invalid attachment name '{name}'"
        )));
    }

    let dir = state.harness_home.join(".runtime/attach").join(&sid);
    serve_attachment(&dir, &name).await
}

async fn serve_attachment(dir: &std::path::Path, name: &str) -> Result<Response, ApiError> {
    let canon_dir = dir
        .canonicalize()
        .map_err(|_| ApiError::NotFound(format!("attachment {name}")))?;
    let canon_file = dir
        .join(name)
        .canonicalize()
        .map_err(|_| ApiError::NotFound(format!("attachment {name}")))?;
    if !canon_file.starts_with(&canon_dir) || !canon_file.is_file() {
        return Err(ApiError::BadRequest(format!(
            "attachment '{name}' escapes the attachment directory"
        )));
    }

    let bytes = tokio::fs::read(&canon_file)
        .await
        .map_err(|e| ApiError::internal_context("read attachment", e))?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, attachment_content_type(name))
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{name}\""),
        )
        .header(header::CONTENT_SECURITY_POLICY, "sandbox")
        .body(axum::body::Body::from(bytes))
        .map_err(|e| ApiError::internal_context("build attachment response", e))
}

fn is_safe_attachment_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && !segment.contains("..")
        && !segment.contains(['/', '\\'])
}

fn attachment_content_type(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "txt" => "text/plain; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",
        "json" => "application/json",
        "csv" => "text/csv; charset=utf-8",
        "html" | "htm" => "text/plain; charset=utf-8",
        "excalidraw" => "application/json",
        _ => "application/octet-stream",
    }
}

/// Block path separators, leading dots, and oversized names. Falls back to a
/// UUID-named file when sanitisation would leave us empty-handed.
fn sanitize_filename(raw: &str) -> String {
    let trimmed = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim_matches('.')
        .trim();
    let cleaned: String = trimmed
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    if cleaned.is_empty() {
        return format!("attachment-{}", uuid::Uuid::new_v4());
    }
    if cleaned.len() > 200 {
        return cleaned.chars().take(200).collect();
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn serve_validated(dir: &std::path::Path, name: &str) -> Result<Response, ApiError> {
        if !is_safe_attachment_segment(name) || sanitize_filename(name) != name {
            return Err(ApiError::BadRequest(format!(
                "invalid attachment name '{name}'"
            )));
        }
        serve_attachment(dir, name).await
    }

    #[tokio::test]
    async fn get_attachment_serves_png_with_inline_headers() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("shot.png"), b"\x89PNG\r\n\x1a\nfake").unwrap();

        let resp = serve_validated(tmp.path(), "shot.png").await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let headers = resp.headers();
        assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "image/png");
        assert_eq!(
            headers.get(header::CONTENT_DISPOSITION).unwrap(),
            "inline; filename=\"shot.png\""
        );
        assert_eq!(
            headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "sandbox"
        );
    }

    #[tokio::test]
    async fn get_attachment_missing_file_is_404() {
        let tmp = tempfile::tempdir().unwrap();

        let err = serve_validated(tmp.path(), "nope.png").await.unwrap_err();

        assert!(matches!(err, ApiError::NotFound(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn get_attachment_rejects_traversal_names() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["../secret.png", "sub/secret.png", "sub\\secret.png", ".."] {
            let err = serve_validated(tmp.path(), name).await.unwrap_err();
            assert!(
                matches!(err, ApiError::BadRequest(_)),
                "{name} should be a 400, got {err:?}"
            );
        }
    }

    #[test]
    fn attachment_content_type_maps_extensions() {
        assert_eq!(attachment_content_type("a.PNG"), "image/png");
        assert_eq!(attachment_content_type("a.jpeg"), "image/jpeg");
        assert_eq!(attachment_content_type("a.svg"), "image/svg+xml");
        assert_eq!(
            attachment_content_type("a.html"),
            "text/plain; charset=utf-8"
        );
        assert_eq!(attachment_content_type("a.excalidraw"), "application/json");
        assert_eq!(attachment_content_type("noext"), "application/octet-stream");
        assert_eq!(attachment_content_type("screenshot.png"), "image/png");
        assert_eq!(
            attachment_content_type("notes.txt"),
            "text/plain; charset=utf-8"
        );
    }
}
