//! Structured read-only query helpers.
//!
//! These APIs let agents provide query intent as JSON-like structures while the
//! backend handles identifier quoting, basic schema validation, bind values,
//! limits, and engine-specific execution.

use std::collections::HashSet;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use sqlx::{Column as _, QueryBuilder, Row as _, TypeInfo};

use crate::error::{DbError, DbResult};
use crate::pool::DbPool;
use crate::types::{Engine, QueryResult, ResultColumn};
use crate::value::{decode_mysql_row, decode_postgres_row, decode_sqlite_row, TaggedValue, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectRequest {
    #[serde(default)]
    pub schema: Option<String>,
    pub table: String,
    #[serde(default)]
    pub columns: Option<Vec<String>>,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub order_by: Vec<OrderBy>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub column: String,
    pub op: FilterOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    Ilike,
    Contains,
    StartsWith,
    In,
    IsNull,
    IsNotNull,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBy {
    pub column: String,
    #[serde(default = "default_order_dir")]
    pub dir: OrderDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderDir {
    Asc,
    Desc,
}

fn default_order_dir() -> OrderDir {
    OrderDir::Asc
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectResponse {
    pub sql: String,
    pub result: QueryResult,
}

pub async fn select(pool: &DbPool, req: SelectRequest) -> DbResult<SelectResponse> {
    validate_request_shape(&req)?;
    let engine = pool.engine();
    let (valid_schema, valid_table, valid_columns) = validate_table_and_columns(pool, &req).await?;
    let schema = req.schema.as_deref().or(valid_schema.as_deref());
    let table = valid_table.as_str();
    let selected_columns = selected_columns(&req, &valid_columns)?;
    let limit = req.limit.unwrap_or(20).clamp(1, 500);

    match pool {
        DbPool::Sqlite(p) => {
            let mut builder = QueryBuilder::<sqlx::Sqlite>::new("SELECT ");
            push_select_list(&mut builder, engine, &selected_columns)?;
            push_from_where_order_limit_sqlite(&mut builder, engine, schema, table, &req, limit)?;
            let sql = builder.sql().to_string();
            let start = Instant::now();
            let rows = builder.build().fetch_all(p).await?;
            let columns = first_row_cols_sqlite(rows.first());
            let decoded: Vec<Vec<Value>> = rows.iter().map(decode_sqlite_row).collect();
            Ok(SelectResponse {
                sql,
                result: query_result(columns, decoded, start),
            })
        }
        DbPool::Postgres(p) => {
            let mut builder = QueryBuilder::<sqlx::Postgres>::new("SELECT ");
            push_select_list(&mut builder, engine, &selected_columns)?;
            push_from_where_order_limit_pg(&mut builder, engine, schema, table, &req, limit)?;
            let sql = builder.sql().to_string();
            let start = Instant::now();
            let rows = builder.build().fetch_all(p).await?;
            let columns = first_row_cols_pg(rows.first());
            let decoded: Vec<Vec<Value>> = rows.iter().map(decode_postgres_row).collect();
            Ok(SelectResponse {
                sql,
                result: query_result(columns, decoded, start),
            })
        }
        DbPool::Mysql(p) => {
            let mut builder = QueryBuilder::<sqlx::MySql>::new("SELECT ");
            push_select_list(&mut builder, engine, &selected_columns)?;
            push_from_where_order_limit_mysql(&mut builder, engine, schema, table, &req, limit)?;
            let sql = builder.sql().to_string();
            let start = Instant::now();
            let rows = builder.build().fetch_all(p).await?;
            let columns = first_row_cols_mysql(rows.first());
            let decoded: Vec<Vec<Value>> = rows.iter().map(decode_mysql_row).collect();
            Ok(SelectResponse {
                sql,
                result: query_result(columns, decoded, start),
            })
        }
    }
}

fn validate_request_shape(req: &SelectRequest) -> DbResult<()> {
    if req.table.trim().is_empty() {
        return Err(DbError::Validation("table must not be empty".into()));
    }
    if matches!(req.columns.as_ref(), Some(columns) if columns.is_empty()) {
        return Err(DbError::Validation("columns must not be empty".into()));
    }
    if req.filters.len() > 20 {
        return Err(DbError::Validation("too many filters; max 20".into()));
    }
    if req.order_by.len() > 5 {
        return Err(DbError::Validation(
            "too many order_by entries; max 5".into(),
        ));
    }
    Ok(())
}

async fn validate_table_and_columns(
    pool: &DbPool,
    req: &SelectRequest,
) -> DbResult<(Option<String>, String, HashSet<String>)> {
    let tree = crate::schema::introspect_filtered(
        pool,
        pool.engine(),
        None,
        req.schema.as_deref(),
        Some(req.table.as_str()),
    )
    .await?;
    let table = tree
        .schemas
        .iter()
        .find(|schema| {
            req.schema
                .as_deref()
                .map(|s| s == schema.name)
                .unwrap_or(true)
        })
        .and_then(|schema| {
            schema
                .tables
                .iter()
                .find(|table| table.name == req.table)
                .map(|table| {
                    (
                        schema.name.clone(),
                        table.name.clone(),
                        table.columns.clone(),
                    )
                })
        });
    let Some((schema, table, columns)) = table else {
        return Err(DbError::NotFound(format!("table {}", req.table)));
    };
    let valid_columns = columns
        .into_iter()
        .map(|column| column.name)
        .collect::<HashSet<_>>();
    Ok((Some(schema), table, valid_columns))
}

fn selected_columns(req: &SelectRequest, valid_columns: &HashSet<String>) -> DbResult<Vec<String>> {
    let columns = req
        .columns
        .clone()
        .unwrap_or_else(|| valid_columns.iter().cloned().collect());
    for column in &columns {
        require_column(valid_columns, column)?;
    }
    for filter in &req.filters {
        require_column(valid_columns, &filter.column)?;
    }
    for order in &req.order_by {
        require_column(valid_columns, &order.column)?;
    }
    Ok(columns)
}

fn require_column(valid_columns: &HashSet<String>, column: &str) -> DbResult<()> {
    if !valid_columns.contains(column) {
        return Err(DbError::Validation(format!("unknown column: {column}")));
    }
    Ok(())
}

fn push_select_list<'args, DB>(
    builder: &mut QueryBuilder<'args, DB>,
    engine: Engine,
    columns: &[String],
) -> DbResult<()>
where
    DB: sqlx::Database,
{
    let mut separated = builder.separated(", ");
    for column in columns {
        separated.push(quote_ident(engine, column)?);
    }
    Ok(())
}

fn push_from_where_order_limit_sqlite<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Sqlite>,
    engine: Engine,
    schema: Option<&str>,
    table: &str,
    req: &'args SelectRequest,
    limit: usize,
) -> DbResult<()> {
    push_from(builder, engine, schema, table)?;
    push_filters_sqlite(builder, engine, &req.filters)?;
    push_order(builder, engine, &req.order_by)?;
    builder.push(" LIMIT ");
    builder.push_bind(limit as i64);
    Ok(())
}

fn push_from_where_order_limit_pg<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Postgres>,
    engine: Engine,
    schema: Option<&str>,
    table: &str,
    req: &'args SelectRequest,
    limit: usize,
) -> DbResult<()> {
    push_from(builder, engine, schema, table)?;
    push_filters_pg(builder, engine, &req.filters)?;
    push_order(builder, engine, &req.order_by)?;
    builder.push(" LIMIT ");
    builder.push_bind(limit as i64);
    Ok(())
}

fn push_from_where_order_limit_mysql<'args>(
    builder: &mut QueryBuilder<'args, sqlx::MySql>,
    engine: Engine,
    schema: Option<&str>,
    table: &str,
    req: &'args SelectRequest,
    limit: usize,
) -> DbResult<()> {
    push_from(builder, engine, schema, table)?;
    push_filters_mysql(builder, engine, &req.filters)?;
    push_order(builder, engine, &req.order_by)?;
    builder.push(" LIMIT ");
    builder.push_bind(limit as i64);
    Ok(())
}

fn push_from<'args, DB>(
    builder: &mut QueryBuilder<'args, DB>,
    engine: Engine,
    schema: Option<&str>,
    table: &str,
) -> DbResult<()>
where
    DB: sqlx::Database,
{
    builder.push(" FROM ");
    builder.push(qualify(engine, schema, table)?);
    Ok(())
}

fn push_order<'args, DB>(
    builder: &mut QueryBuilder<'args, DB>,
    engine: Engine,
    order_by: &[OrderBy],
) -> DbResult<()>
where
    DB: sqlx::Database,
{
    if order_by.is_empty() {
        return Ok(());
    }
    builder.push(" ORDER BY ");
    let mut separated = builder.separated(", ");
    for order in order_by {
        separated.push(quote_ident(engine, &order.column)?);
        separated.push_unseparated(match order.dir {
            OrderDir::Asc => " ASC",
            OrderDir::Desc => " DESC",
        });
    }
    Ok(())
}

fn push_filters_sqlite<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Sqlite>,
    engine: Engine,
    filters: &'args [Filter],
) -> DbResult<()> {
    if filters.is_empty() {
        return Ok(());
    }
    builder.push(" WHERE ");
    for (idx, filter) in filters.iter().enumerate() {
        if idx > 0 {
            builder.push(" AND ");
        }
        push_filter_head(builder, engine, filter, false)?;
        push_filter_value_sqlite(builder, filter)?;
    }
    Ok(())
}

fn push_filters_pg<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Postgres>,
    engine: Engine,
    filters: &'args [Filter],
) -> DbResult<()> {
    if filters.is_empty() {
        return Ok(());
    }
    builder.push(" WHERE ");
    for (idx, filter) in filters.iter().enumerate() {
        if idx > 0 {
            builder.push(" AND ");
        }
        push_filter_head(builder, engine, filter, true)?;
        push_filter_value_pg(builder, filter)?;
    }
    Ok(())
}

fn push_filters_mysql<'args>(
    builder: &mut QueryBuilder<'args, sqlx::MySql>,
    engine: Engine,
    filters: &'args [Filter],
) -> DbResult<()> {
    if filters.is_empty() {
        return Ok(());
    }
    builder.push(" WHERE ");
    for (idx, filter) in filters.iter().enumerate() {
        if idx > 0 {
            builder.push(" AND ");
        }
        push_filter_head(builder, engine, filter, false)?;
        push_filter_value_mysql(builder, filter)?;
    }
    Ok(())
}

fn push_filter_head<'args, DB>(
    builder: &mut QueryBuilder<'args, DB>,
    engine: Engine,
    filter: &Filter,
    supports_ilike: bool,
) -> DbResult<()>
where
    DB: sqlx::Database,
{
    let column = quote_ident(engine, &filter.column)?;
    match filter.op {
        FilterOp::IsNull => builder.push(format!("{column} IS NULL")),
        FilterOp::IsNotNull => builder.push(format!("{column} IS NOT NULL")),
        FilterOp::In => builder.push(format!("{column} IN (")),
        FilterOp::Contains | FilterOp::StartsWith | FilterOp::Like => {
            builder.push(format!("{column} LIKE "))
        }
        FilterOp::Ilike => {
            let op = if supports_ilike { "ILIKE" } else { "LIKE" };
            builder.push(format!("{column} {op} "))
        }
        op => builder.push(format!("{column} {} ", sql_op(op)?)),
    };
    Ok(())
}

fn push_filter_value_sqlite<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Sqlite>,
    filter: &'args Filter,
) -> DbResult<()> {
    match filter.op {
        FilterOp::IsNull | FilterOp::IsNotNull => Ok(()),
        FilterOp::In => {
            let values = in_values(filter)?;
            for (idx, value) in values.iter().enumerate() {
                if idx > 0 {
                    builder.push(", ");
                }
                bind_sqlite(builder, value);
            }
            builder.push(")");
            Ok(())
        }
        FilterOp::Contains | FilterOp::StartsWith | FilterOp::Like | FilterOp::Ilike => {
            let pattern = pattern_value(filter)?;
            builder.push_bind(pattern);
            Ok(())
        }
        _ => {
            bind_sqlite(builder, required_value(filter)?);
            Ok(())
        }
    }
}

fn push_filter_value_pg<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Postgres>,
    filter: &'args Filter,
) -> DbResult<()> {
    match filter.op {
        FilterOp::IsNull | FilterOp::IsNotNull => Ok(()),
        FilterOp::In => {
            let values = in_values(filter)?;
            for (idx, value) in values.iter().enumerate() {
                if idx > 0 {
                    builder.push(", ");
                }
                bind_pg(builder, value);
            }
            builder.push(")");
            Ok(())
        }
        FilterOp::Contains | FilterOp::StartsWith | FilterOp::Like | FilterOp::Ilike => {
            let pattern = pattern_value(filter)?;
            builder.push_bind(pattern);
            Ok(())
        }
        _ => {
            bind_pg(builder, required_value(filter)?);
            Ok(())
        }
    }
}

fn push_filter_value_mysql<'args>(
    builder: &mut QueryBuilder<'args, sqlx::MySql>,
    filter: &'args Filter,
) -> DbResult<()> {
    match filter.op {
        FilterOp::IsNull | FilterOp::IsNotNull => Ok(()),
        FilterOp::In => {
            let values = in_values(filter)?;
            for (idx, value) in values.iter().enumerate() {
                if idx > 0 {
                    builder.push(", ");
                }
                bind_mysql(builder, value);
            }
            builder.push(")");
            Ok(())
        }
        FilterOp::Contains | FilterOp::StartsWith | FilterOp::Like | FilterOp::Ilike => {
            let pattern = pattern_value(filter)?;
            builder.push_bind(pattern);
            Ok(())
        }
        _ => {
            bind_mysql(builder, required_value(filter)?);
            Ok(())
        }
    }
}

fn required_value(filter: &Filter) -> DbResult<&Value> {
    filter
        .value
        .as_ref()
        .ok_or_else(|| DbError::Validation(format!("filter op `{:?}` requires value", filter.op)))
}

fn in_values(filter: &Filter) -> DbResult<&[Value]> {
    let values = filter
        .values
        .as_deref()
        .ok_or_else(|| DbError::Validation("filter op `in` requires `values`".into()))?;
    if values.is_empty() {
        return Err(DbError::Validation(
            "filter op `in` requires at least one value".into(),
        ));
    }
    Ok(values)
}

fn pattern_value(filter: &Filter) -> DbResult<String> {
    let value = filter_value_text(filter)?;
    Ok(match filter.op {
        FilterOp::Contains => format!("%{value}%"),
        FilterOp::StartsWith => format!("{value}%"),
        _ => value,
    })
}

fn bind_sqlite<'args>(builder: &mut QueryBuilder<'args, sqlx::Sqlite>, value: &'args Value) {
    match value {
        Value::Null => {
            builder.push_bind(Option::<String>::None);
        }
        Value::Bool(value) => {
            builder.push_bind(*value);
        }
        Value::Int(value) => {
            builder.push_bind(*value);
        }
        Value::Float(value) => {
            builder.push_bind(*value);
        }
        Value::Text(value) => {
            builder.push_bind(value);
        }
        Value::Tagged(value) => {
            builder.push_bind(tagged_to_string(value));
        }
    };
}

fn bind_pg<'args>(builder: &mut QueryBuilder<'args, sqlx::Postgres>, value: &'args Value) {
    match value {
        Value::Null => {
            builder.push_bind(Option::<String>::None);
        }
        Value::Bool(value) => {
            builder.push_bind(*value);
        }
        Value::Int(value) => {
            builder.push_bind(*value);
        }
        Value::Float(value) => {
            builder.push_bind(*value);
        }
        Value::Text(value) => {
            builder.push_bind(value);
        }
        Value::Tagged(TaggedValue::Json(value)) => {
            builder.push_bind(value);
        }
        Value::Tagged(value) => {
            builder.push_bind(tagged_to_string(value));
        }
    };
}

fn bind_mysql<'args>(builder: &mut QueryBuilder<'args, sqlx::MySql>, value: &'args Value) {
    match value {
        Value::Null => {
            builder.push_bind(Option::<String>::None);
        }
        Value::Bool(value) => {
            builder.push_bind(*value);
        }
        Value::Int(value) => {
            builder.push_bind(*value);
        }
        Value::Float(value) => {
            builder.push_bind(*value);
        }
        Value::Text(value) => {
            builder.push_bind(value);
        }
        Value::Tagged(value) => {
            builder.push_bind(tagged_to_string(value));
        }
    };
}

fn tagged_to_string(value: &TaggedValue) -> String {
    match value {
        TaggedValue::Decimal(value)
        | TaggedValue::Bytes(value)
        | TaggedValue::Date(value)
        | TaggedValue::Time(value)
        | TaggedValue::DateTime(value) => value.clone(),
        TaggedValue::Json(value) => value.to_string(),
    }
}

fn filter_value_text(filter: &Filter) -> DbResult<String> {
    match filter.value.as_ref() {
        Some(Value::Text(value)) => Ok(value.clone()),
        Some(value) => Ok(value_to_string(value)),
        None => Err(DbError::Validation(format!(
            "filter op `{:?}` requires value",
            filter.op
        ))),
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::Text(value) => value.clone(),
        Value::Tagged(TaggedValue::Decimal(value))
        | Value::Tagged(TaggedValue::Bytes(value))
        | Value::Tagged(TaggedValue::Date(value))
        | Value::Tagged(TaggedValue::Time(value))
        | Value::Tagged(TaggedValue::DateTime(value)) => value.clone(),
        Value::Tagged(TaggedValue::Json(value)) => value.to_string(),
    }
}

fn sql_op(op: FilterOp) -> DbResult<&'static str> {
    match op {
        FilterOp::Eq => Ok("="),
        FilterOp::Neq => Ok("<>"),
        FilterOp::Gt => Ok(">"),
        FilterOp::Gte => Ok(">="),
        FilterOp::Lt => Ok("<"),
        FilterOp::Lte => Ok("<="),
        _ => Err(DbError::Validation(format!(
            "unsupported scalar operator: {op:?}"
        ))),
    }
}

fn ident_quote(engine: Engine) -> char {
    match engine {
        Engine::Mysql => '`',
        _ => '"',
    }
}

fn quote_ident(engine: Engine, ident: &str) -> DbResult<String> {
    if ident.is_empty() {
        return Err(DbError::Validation("identifier must not be empty".into()));
    }
    if ident.contains('\0') {
        return Err(DbError::Validation(
            "identifier must not contain NUL".into(),
        ));
    }
    let quote = ident_quote(engine);
    let escaped = ident.replace(quote, &format!("{quote}{quote}"));
    Ok(format!("{quote}{escaped}{quote}"))
}

fn qualify(engine: Engine, schema: Option<&str>, table: &str) -> DbResult<String> {
    let qtable = quote_ident(engine, table)?;
    match schema {
        Some(schema) if !schema.trim().is_empty() => {
            Ok(format!("{}.{}", quote_ident(engine, schema)?, qtable))
        }
        _ => Ok(qtable),
    }
}

fn query_result(columns: Vec<ResultColumn>, rows: Vec<Vec<Value>>, start: Instant) -> QueryResult {
    QueryResult {
        columns,
        rows,
        total_rows: None,
        truncated: false,
        elapsed_ms: start.elapsed().as_millis() as u64,
        query_id: uuid::Uuid::new_v4().to_string(),
    }
}

fn first_row_cols_sqlite(r: Option<&sqlx::sqlite::SqliteRow>) -> Vec<ResultColumn> {
    r.map(|r| {
        r.columns()
            .iter()
            .map(|c| ResultColumn {
                name: c.name().to_string(),
                r#type: c.type_info().name().to_string(),
            })
            .collect()
    })
    .unwrap_or_default()
}

fn first_row_cols_pg(r: Option<&sqlx::postgres::PgRow>) -> Vec<ResultColumn> {
    r.map(|r| {
        r.columns()
            .iter()
            .map(|c| ResultColumn {
                name: c.name().to_string(),
                r#type: c.type_info().name().to_string(),
            })
            .collect()
    })
    .unwrap_or_default()
}

fn first_row_cols_mysql(r: Option<&sqlx::mysql::MySqlRow>) -> Vec<ResultColumn> {
    r.map(|r| {
        r.columns()
            .iter()
            .map(|c| ResultColumn {
                name: c.name().to_string(),
                r#type: c.type_info().name().to_string(),
            })
            .collect()
    })
    .unwrap_or_default()
}
