use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use harness_session::{MailboxMessage, MailboxStore};
use serde::Deserialize;

use crate::error::ApiError;
use crate::state::AppState;

const MAX_MAILBOX_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
pub struct MailboxSendBody {
    pub to_session_id: String,
    pub body: String,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

fn mailbox_store(state: &AppState) -> MailboxStore {
    MailboxStore::new(
        state
            .harness_home
            .join("profiles")
            .join(&state.profile)
            .join("sessions"),
    )
}

pub async fn send_mailbox_route(
    State(state): State<Arc<AppState>>,
    Path(from_sid): Path<String>,
    Json(body): Json<MailboxSendBody>,
) -> Result<(StatusCode, Json<MailboxMessage>), ApiError> {
    if body.body.trim().is_empty() {
        return Err(ApiError::BadRequest("mailbox body cannot be empty".into()));
    }
    if body.body.len() > MAX_MAILBOX_BODY_BYTES {
        return Err(ApiError::BadRequest(format!(
            "mailbox body too large ({} bytes); cap is {MAX_MAILBOX_BODY_BYTES}",
            body.body.len()
        )));
    }
    if !state.manager.is_in_tree(&from_sid, &body.to_session_id) || from_sid == body.to_session_id {
        return Err(ApiError::BadRequest(
            "target session is not a descendant of the sender".into(),
        ));
    }

    let msg = mailbox_store(&state)
        .send(
            &from_sid,
            &body.to_session_id,
            body.body,
            body.task_id,
            body.scopes,
        )
        .map_err(|e| ApiError::internal_context("mailbox send", e))?;
    Ok((StatusCode::CREATED, Json(msg)))
}

pub async fn list_mailbox_route(
    State(state): State<Arc<AppState>>,
    Path(sid): Path<String>,
) -> Result<Json<Vec<MailboxMessage>>, ApiError> {
    if !state
        .manager
        .list_metas()
        .await
        .iter()
        .any(|meta| meta.id == sid)
    {
        return Err(ApiError::NotFound(format!("session {sid}")));
    }
    let messages = mailbox_store(&state)
        .list(&sid)
        .map_err(|e| ApiError::internal_context("mailbox list", e))?;
    Ok(Json(messages))
}

pub async fn ack_mailbox_route(
    State(state): State<Arc<AppState>>,
    Path((sid, message_id)): Path<(String, String)>,
) -> Result<Json<MailboxMessage>, ApiError> {
    if !state
        .manager
        .list_metas()
        .await
        .iter()
        .any(|meta| meta.id == sid)
    {
        return Err(ApiError::NotFound(format!("session {sid}")));
    }
    let Some(message) = mailbox_store(&state)
        .ack(&sid, &message_id, &sid)
        .map_err(|e| ApiError::internal_context("mailbox ack", e))?
    else {
        return Err(ApiError::NotFound(format!("mailbox message {message_id}")));
    };
    Ok(Json(message))
}
