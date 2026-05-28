use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use harness_core::{
    check_pdftotext, ingest_pdf, KnowledgeIngestRequest, KnowledgeIngestResult, PdfTextToolStatus,
};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/knowledge/pdf", post(ingest_pdf_route))
        .route("/api/knowledge/pdf/pdftotext", get(check_pdftotext_route))
}

#[derive(Debug, Deserialize)]
pub struct PdfIngestBody {
    pub source_path: String,
    #[serde(default)]
    pub title: Option<String>,
}

async fn ingest_pdf_route(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PdfIngestBody>,
) -> ApiResult<Json<KnowledgeIngestResult>> {
    let result = ingest_pdf(
        &state.harness_home,
        &state.profile,
        KnowledgeIngestRequest {
            source_path: body.source_path.into(),
            title: body.title,
        },
    )
    .map_err(ApiError::from)?;
    Ok(Json(result))
}

async fn check_pdftotext_route() -> Json<PdfTextToolStatus> {
    Json(check_pdftotext())
}
