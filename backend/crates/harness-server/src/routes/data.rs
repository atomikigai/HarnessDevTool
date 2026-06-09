use std::sync::Arc;

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};

use crate::data::{
    inspect_data_file, write_data_file, DataInspectRequest, DataInspectResponse, DataWriteRequest,
    DataWriteResponse,
};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/data/inspect", post(inspect_data))
        .route("/api/data/write", post(write_data))
}

async fn inspect_data(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<DataInspectRequest>,
) -> ApiResult<Json<DataInspectResponse>> {
    let result = tokio::task::spawn_blocking(move || inspect_data_file(body))
        .await
        .map_err(|e| ApiError::Internal(format!("data inspect task failed: {e}")))??;
    Ok(Json(result))
}

async fn write_data(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<DataWriteRequest>,
) -> ApiResult<Json<DataWriteResponse>> {
    let result = tokio::task::spawn_blocking(move || write_data_file(body))
        .await
        .map_err(|e| ApiError::Internal(format!("data write task failed: {e}")))??;
    Ok(Json(result))
}
