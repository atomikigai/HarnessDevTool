//! REST surface for `module-db`. See `module-db::Manager` for the actual ops.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use module_db::{
    Connection, ConnectionInput, QueryResult, Row, SchemaTree, TestResult, Value,
};
use serde::Deserialize;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/db/connections",
            get(list_connections).post(create_connection),
        )
        .route(
            "/api/db/connections/:id",
            put(update_connection).delete(delete_connection),
        )
        .route("/api/db/connections/:id/test", post(test_connection))
        .route("/api/db/test", post(test_input))
        .route("/api/db/connections/:id/databases", get(list_databases))
        .route("/api/db/connections/:id/schema", get(schema_tree))
        .route("/api/db/connections/:id/query", post(run_query))
        .route(
            "/api/db/connections/:id/query/:query_id/cancel",
            post(cancel_query),
        )
        .route(
            "/api/db/connections/:id/tables/:table/rows",
            post(insert_row).put(update_row).delete(delete_row),
        )
        .route(
            "/api/db/connections/:id/tables/:table/rows/duplicate",
            post(duplicate_row),
        )
}

fn map_db_err(e: module_db::DbError) -> ApiError {
    use module_db::DbError;
    match e {
        DbError::NotFound(s) => ApiError::NotFound(s),
        DbError::Validation(s) | DbError::Unsupported(s) | DbError::NoPrimaryKey(s) => {
            ApiError::BadRequest(s)
        }
        DbError::QueryNotFound(s) => ApiError::NotFound(s),
        other => ApiError::Internal(other.to_string()),
    }
}

async fn list_connections(
    State(s): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<Connection>>> {
    s.db.connections_list().map(Json).map_err(map_db_err)
}

async fn create_connection(
    State(s): State<Arc<AppState>>,
    Json(input): Json<ConnectionInput>,
) -> ApiResult<(StatusCode, Json<Connection>)> {
    let c = s.db.connections_add(input).map_err(map_db_err)?;
    Ok((StatusCode::CREATED, Json(c)))
}

async fn update_connection(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<ConnectionInput>,
) -> ApiResult<Json<Connection>> {
    s.db.connections_update(&id, input)
        .map(Json)
        .map_err(map_db_err)
}

async fn delete_connection(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    s.db.connections_remove(&id).map_err(map_db_err)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn test_connection(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<TestResult>> {
    s.db.connections_test_stored(&id)
        .await
        .map(Json)
        .map_err(map_db_err)
}

async fn test_input(
    State(s): State<Arc<AppState>>,
    Json(input): Json<ConnectionInput>,
) -> ApiResult<Json<TestResult>> {
    s.db.connections_test_input(input)
        .await
        .map(Json)
        .map_err(map_db_err)
}

async fn list_databases(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<String>>> {
    s.db.databases_list(&id).await.map(Json).map_err(map_db_err)
}

#[derive(Deserialize)]
struct SchemaQuery {
    database: Option<String>,
}

async fn schema_tree(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(q): Query<SchemaQuery>,
) -> ApiResult<Json<SchemaTree>> {
    s.db.schema_tree(&id, q.database.as_deref())
        .await
        .map(Json)
        .map_err(map_db_err)
}

#[derive(Deserialize)]
struct QueryBody {
    database: Option<String>,
    sql: String,
    #[serde(default)]
    params: Option<Vec<Value>>,
    #[serde(default)]
    page_size: Option<usize>,
    #[serde(default)]
    page: Option<usize>,
}

async fn run_query(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<QueryBody>,
) -> ApiResult<Json<QueryResult>> {
    s.db.query_run(
        &id,
        body.database.as_deref(),
        &body.sql,
        body.params,
        body.page_size.unwrap_or(100),
        body.page.unwrap_or(0),
    )
    .await
    .map(Json)
    .map_err(map_db_err)
}

async fn cancel_query(
    State(s): State<Arc<AppState>>,
    Path((_id, qid)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let ok = s.db.query_cancel(&qid).await.map_err(map_db_err)?;
    Ok(Json(serde_json::json!({ "ok": ok })))
}

#[derive(Deserialize)]
struct RowInsertBody {
    database: Option<String>,
    schema: Option<String>,
    values: HashMap<String, Value>,
}

async fn insert_row(
    State(s): State<Arc<AppState>>,
    Path((id, table)): Path<(String, String)>,
    Json(b): Json<RowInsertBody>,
) -> ApiResult<Json<Row>> {
    s.db.row_insert(
        &id,
        b.database.as_deref(),
        b.schema.as_deref(),
        &table,
        b.values,
    )
    .await
    .map(Json)
    .map_err(map_db_err)
}

#[derive(Deserialize)]
struct RowUpdateBody {
    database: Option<String>,
    schema: Option<String>,
    pk: HashMap<String, Value>,
    values: HashMap<String, Value>,
}

async fn update_row(
    State(s): State<Arc<AppState>>,
    Path((id, table)): Path<(String, String)>,
    Json(b): Json<RowUpdateBody>,
) -> ApiResult<Json<Row>> {
    s.db.row_update(
        &id,
        b.database.as_deref(),
        b.schema.as_deref(),
        &table,
        b.pk,
        b.values,
    )
    .await
    .map(Json)
    .map_err(map_db_err)
}

#[derive(Deserialize)]
struct RowDeleteBody {
    database: Option<String>,
    schema: Option<String>,
    pk: HashMap<String, Value>,
}

async fn delete_row(
    State(s): State<Arc<AppState>>,
    Path((id, table)): Path<(String, String)>,
    Json(b): Json<RowDeleteBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let n = s
        .db
        .row_delete(&id, b.database.as_deref(), b.schema.as_deref(), &table, b.pk)
        .await
        .map_err(map_db_err)?;
    Ok(Json(serde_json::json!({ "affected": n })))
}

async fn duplicate_row(
    State(s): State<Arc<AppState>>,
    Path((id, table)): Path<(String, String)>,
    Json(b): Json<RowDeleteBody>,
) -> ApiResult<Json<Row>> {
    s.db.row_duplicate(
        &id,
        b.database.as_deref(),
        b.schema.as_deref(),
        &table,
        b.pk,
    )
    .await
    .map(Json)
    .map_err(map_db_err)
}

