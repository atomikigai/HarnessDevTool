//! REST surface for `module-db`. See `module-db::Manager` for the actual ops.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use harness_session::AgentKind;
use module_db::{
    Connection, ConnectionInput, ExportRequest, PinnedTab, QueryResult, Row, SchemaTree,
    TestResult, Value,
};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::routes::sessions::{spawn_session_internal, SpawnArgs};
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
        .route("/api/db/connections/:id/agent", post(start_db_agent))
        .route("/api/db/connections/:id/query", post(run_query))
        .route("/api/db/connections/:id/explain", post(explain_query))
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
        .route("/api/db/connections/:id/export", post(export_data))
        .route("/api/db/tabs", get(list_pinned_tabs))
        .route("/api/db/tabs/:tab_id/pin", post(pin_tab).delete(unpin_tab))
}

#[derive(Debug, Deserialize)]
struct StartDbAgentBody {
    #[serde(default)]
    database: Option<String>,
    #[serde(default)]
    kind: Option<AgentKind>,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Serialize)]
struct StartDbAgentResponse {
    thread_id: String,
    session_id: String,
}

async fn start_db_agent(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<StartDbAgentBody>,
) -> ApiResult<Json<StartDbAgentResponse>> {
    let conn = s.db.connections_get(&id).map_err(map_db_err)?;
    let thread = s
        .store
        .create_thread(Some(format!("DB Agent: {}", conn.name)))?;
    let cwd = resolve_agent_cwd(body.cwd.as_deref(), &s.harness_home)?;
    let memory_path = ensure_db_memory(&s, &conn, body.database.as_deref())?;
    let prompt = db_agent_prompt(&conn, body.database.as_deref(), &memory_path);
    let kind = body.kind.unwrap_or(AgentKind::Claude);
    let supports_system_context = db_agent_supports_system_context(kind);
    let session_id = spawn_session_internal(
        &s,
        SpawnArgs {
            kind,
            thread_id: thread.id.clone(),
            cwd,
            role: None,
            owner_session_id: None,
            task_id: None,
            scopes: db_agent_scopes(&id, body.database.as_deref()),
            auto_intro: supports_system_context.then(|| prompt.clone()),
            initial_prompt: (!supports_system_context).then_some(prompt),
            parent_session_id: None,
            initial_size: None,
            include_project_context: true,
            capability_profile: crate::routes::sessions::CapabilityProfile::Auto,
            zeus_roles: Vec::new(),
            model: None,
            effort: None,
        },
    )
    .await?;
    Ok(Json(StartDbAgentResponse {
        thread_id: thread.id,
        session_id,
    }))
}

fn db_agent_supports_system_context(kind: AgentKind) -> bool {
    matches!(kind, AgentKind::Claude | AgentKind::Codex)
}

fn db_agent_scopes(connection_id: &str, database: Option<&str>) -> Vec<String> {
    let mut scopes = vec![format!("db:connection:{connection_id}")];
    if let Some(database) = database.filter(|database| !database.trim().is_empty()) {
        scopes.push(format!("db:database:{database}"));
    }
    scopes
}

fn resolve_agent_cwd(
    raw: Option<&str>,
    default_cwd: &std::path::Path,
) -> Result<PathBuf, ApiError> {
    let cwd = match raw {
        Some(cwd) if !cwd.is_empty() => PathBuf::from(cwd),
        _ => default_cwd.to_path_buf(),
    };
    if !cwd.exists() {
        return Err(ApiError::BadRequest(format!(
            "cwd does not exist: {}",
            cwd.display()
        )));
    }
    Ok(cwd)
}

fn db_agent_prompt(
    conn: &Connection,
    database: Option<&str>,
    memory_path: &std::path::Path,
) -> String {
    let selected_database = database.unwrap_or(&conn.database);
    let host = conn.host.as_deref().unwrap_or("(none)");
    let port = conn
        .port
        .map(|p| p.to_string())
        .unwrap_or_else(|| "(none)".into());
    let username = conn.username.as_deref().unwrap_or("(none)");
    let ssl_mode = conn
        .ssl_mode
        .map(|m| format!("{m:?}").to_ascii_lowercase())
        .unwrap_or_else(|| "(none)".into());
    let params = redacted_connection_params(conn);

    format!(
        r#"[harness-db-agent]
You are a database expert for the active Harness DB connection. Respond in the user's language.

Connection defaults (redacted):
- connection id: {connection_id}
- connection name: {connection_name}
- engine: {engine}
- host: {host}
- port: {port}
- selected database: {database}
- username: {username}
- ssl_mode: {ssl_mode}
- params: {params}

Use these defaults in DB tools: connection="{connection_id}", database="{database}".
Persistent DB memory path: {memory_path}

Operate lazily:
- Simple table/performance questions: use semantic helpers first (`db_table_info`, `db_search_tables`, `db_sample`, `db_count`, `db_distinct_values`, `db_find_rows`, `db_aggregate`, `db_relation_performance`) or targeted `db_schema`.
- Extraction requests that need related-table text/labels: use `db_extract_enriched`.
- Complex SELECT exports: use `db_export_query`. View/migration requests: use `db_generate_view_sql` unless the user explicitly asks to execute DDL.
- Filtered reads: prefer structured `db_select`; use `db_validate_query` before uncertain raw SQL.
- Custom raw SQL: use small read-only `db_query` calls with exact filters and `LIMIT 20` unless asked otherwise.
- Broad architecture, relationships, performance, or business context: read DB memory first, then verify only the needed facts.
- Do not introspect the full database, run parallel broad probes, or load extra context unless the current question requires it.

Safety:
- Stay read-only unless the user explicitly asks for a modification.
- Before any write, create a `db_backup`, show the backup path, and wait for explicit confirmation.
- If DB tools are unavailable, say exactly: "DB MCP tools are not loaded for this session; restart the DB Agent/backend."
- Never reveal secrets, passwords, DSNs, tokens, or credential material.
"#,
        connection_id = conn.id,
        connection_name = conn.name,
        engine = conn.engine.as_str(),
        host = host,
        port = port,
        database = selected_database,
        username = username,
        ssl_mode = ssl_mode,
        params = params,
        memory_path = memory_path.display()
    )
}

fn ensure_db_memory(
    state: &AppState,
    conn: &Connection,
    database: Option<&str>,
) -> Result<PathBuf, ApiError> {
    let database = database.unwrap_or(&conn.database);
    let path = db_memory_path(&state.harness_home, &state.profile, &conn.id, database);
    if path.exists() {
        return Ok(path);
    }
    let parent = path
        .parent()
        .ok_or_else(|| ApiError::Internal("invalid DB memory path".into()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| ApiError::Internal(format!("create DB memory dir: {e}")))?;
    let content = initial_db_memory(conn, database);
    std::fs::write(&path, content)
        .map_err(|e| ApiError::Internal(format!("write DB memory: {e}")))?;
    Ok(path)
}

fn db_memory_path(
    home: &std::path::Path,
    profile: &str,
    connection_id: &str,
    database: &str,
) -> PathBuf {
    home.join("profiles")
        .join(sanitize_path_segment(profile))
        .join("db-memory")
        .join(sanitize_path_segment(connection_id))
        .join(format!("{}.md", sanitize_path_segment(database)))
}

fn initial_db_memory(conn: &Connection, database: &str) -> String {
    format!(
        r#"# DB Memory: {connection_name} / {database}

## Overview
- Connection id: `{connection_id}`
- Engine: `{engine}`
- Database: `{database}`
- Status: initialized empty; populate incrementally from verified `db_schema`, `db_query`, and `db_explain` findings.

## Schemas
- Pending targeted inspection.

## Relationships
- Pending deeper analysis.

## Indexes
- Pending deeper analysis.

## Known Queries
- Pending.

## Risks
- Pending.

## Open Questions
- Pending.

## Changelog
- Initialized as an empty incremental memory file.
"#,
        connection_name = conn.name,
        connection_id = conn.id,
        engine = conn.engine.as_str(),
        database = database
    )
}

fn sanitize_path_segment(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn redacted_connection_params(conn: &Connection) -> String {
    if conn.params.is_empty() {
        return "{}".into();
    }
    let redacted = conn
        .params
        .iter()
        .map(|(k, v)| {
            let lower = k.to_ascii_lowercase();
            let value = if lower.contains("password")
                || lower.contains("secret")
                || lower.contains("token")
                || lower.contains("key")
            {
                "[redacted]"
            } else {
                v
            };
            (k.clone(), value.to_string())
        })
        .collect::<HashMap<_, _>>();
    serde_json::to_string(&redacted).unwrap_or_else(|_| "{}".into())
}

async fn export_data(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ExportRequest>,
) -> ApiResult<Response> {
    let result = s.db.export(&id, req).await.map_err(map_db_err)?;
    let disposition = format!("attachment; filename=\"{}\"", result.filename);
    let resp = Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&result.content_type)
                .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
        )
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&disposition)
                .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
        )
        .body(Body::from(result.body))
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(resp)
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

async fn list_connections(State(s): State<Arc<AppState>>) -> ApiResult<Json<Vec<Connection>>> {
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
    /// Optional editor-tab id (Q13). When present, the query participates in
    /// the per-tab lease system: auto-pins on `BEGIN`, auto-unpins on
    /// `COMMIT`/`ROLLBACK`, and reuses the leased connection in between.
    #[serde(default)]
    tab_id: Option<String>,
}

async fn run_query(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<QueryBody>,
) -> ApiResult<Json<QueryResult>> {
    s.db.query_run_with_tab(
        &id,
        body.database.as_deref(),
        body.tab_id.as_deref(),
        &body.sql,
        body.params,
        body.page_size.unwrap_or(100),
        body.page.unwrap_or(0),
    )
    .await
    .map(Json)
    .map_err(map_db_err)
}

#[derive(Deserialize)]
struct ExplainBody {
    database: Option<String>,
    sql: String,
}

async fn explain_query(
    State(s): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ExplainBody>,
) -> ApiResult<Json<QueryResult>> {
    s.db.explain(&id, body.database.as_deref(), &body.sql)
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
    let n =
        s.db.row_delete(
            &id,
            b.database.as_deref(),
            b.schema.as_deref(),
            &table,
            b.pk,
        )
        .await
        .map_err(map_db_err)?;
    Ok(Json(serde_json::json!({ "affected": n })))
}

#[derive(Deserialize)]
struct PinTabBody {
    connection_id: String,
    #[serde(default)]
    database: Option<String>,
}

async fn pin_tab(
    State(s): State<Arc<AppState>>,
    Path(tab_id): Path<String>,
    Json(b): Json<PinTabBody>,
) -> ApiResult<Json<serde_json::Value>> {
    s.db.tab_pin(&tab_id, &b.connection_id, b.database.as_deref())
        .await
        .map_err(map_db_err)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn unpin_tab(
    State(s): State<Arc<AppState>>,
    Path(tab_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let removed = s.db.tab_unpin(&tab_id);
    Ok(Json(serde_json::json!({ "removed": removed })))
}

async fn list_pinned_tabs(State(s): State<Arc<AppState>>) -> ApiResult<Json<Vec<PinnedTab>>> {
    Ok(Json(s.db.tabs_pinned()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chrono::Utc;
    use module_db::Engine;
    use std::collections::BTreeMap;
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
    async fn explain_accepts_database_body_field() {
        let dir = tempfile::tempdir().unwrap();
        let state = state(dir.path().to_path_buf());
        let conn = state
            .db
            .connections_add(ConnectionInput {
                name: "sqlite".to_string(),
                engine: module_db::Engine::Sqlite,
                database: dir.path().join("explain.sqlite").display().to_string(),
                ..Default::default()
            })
            .unwrap();
        let app = router().with_state(state);
        let body = serde_json::json!({
            "database": "x",
            "sql": "SELECT 1"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/db/connections/{}/explain", conn.id))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn db_agent_prompt_is_read_only_and_redacted() {
        let conn = Connection {
            id: "conn-1".into(),
            name: "Local DB".into(),
            engine: Engine::Sqlite,
            host: None,
            port: None,
            database: "/tmp/app.db".into(),
            username: None,
            password_ref: Some("secret-ref".into()),
            ssl_mode: None,
            params: BTreeMap::from([
                ("application_name".into(), "harness-test".into()),
                ("api_token".into(), "token-value".into()),
            ]),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let memory_path = std::path::PathBuf::from("/tmp/db-memory/conn-1/main.md");
        let prompt = db_agent_prompt(&conn, Some("main"), &memory_path);
        assert!(prompt.contains("database expert"));
        assert!(prompt.contains("Operate lazily"));
        assert!(prompt.contains("db_table_info"));
        assert!(prompt.contains("db_relation_performance"));
        assert!(prompt.contains("db_distinct_values"));
        assert!(prompt.contains("db_find_rows"));
        assert!(prompt.contains("db_aggregate"));
        assert!(prompt.contains("db_extract_enriched"));
        assert!(prompt.contains("db_export_query"));
        assert!(prompt.contains("db_generate_view_sql"));
        assert!(prompt.contains("db_select"));
        assert!(prompt.contains("db_validate_query"));
        assert!(prompt.contains("targeted `db_schema`"));
        assert!(prompt.contains("LIMIT 20"));
        assert!(prompt.contains("Stay read-only"));
        assert!(prompt.contains("db_backup"));
        assert!(prompt.contains("DB MCP tools are not loaded"));
        assert!(prompt.contains("/tmp/db-memory/conn-1/main.md"));
        assert!(prompt.contains("connection id: conn-1"));
        assert!(prompt.contains("application_name"));
        assert!(prompt.contains("harness-test"));
        assert!(!prompt.contains("Current schema snapshot"));
        assert!(!prompt.contains("Available DB tools"));
        assert!(!prompt.contains("Query strategy"));
        assert!(!prompt.contains("main.users"));
        assert!(!prompt.contains("secret-ref"));
        assert!(!prompt.contains("token-value"));
    }

    #[test]
    fn codex_db_agent_uses_silent_context_channel() {
        assert!(db_agent_supports_system_context(AgentKind::Codex));
        assert!(db_agent_supports_system_context(AgentKind::Claude));
        assert!(!db_agent_supports_system_context(AgentKind::Zeus));
        assert!(!db_agent_supports_system_context(AgentKind::Cursor));
        assert!(!db_agent_supports_system_context(AgentKind::Antigravity));
    }
}
