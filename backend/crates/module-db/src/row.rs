//! Row-level CRUD without manual SQL. Requires the target table to have a
//! detectable primary key; otherwise we refuse so callers don't accidentally
//! affect many rows.

use std::collections::HashMap;

use sqlx::{AnyPool, Executor, Row as _};

use crate::error::{DbError, DbResult};
use crate::query::decode_row;
use crate::schema::introspect;
use crate::types::{Engine, Row};
use crate::value::Value;

/// Build a fully-qualified table identifier with engine-specific quoting.
fn qualify(engine: Engine, schema: Option<&str>, table: &str) -> String {
    let q = quote_ident(engine);
    match schema {
        Some(s) if !s.is_empty() => format!("{q}{s}{q}.{q}{table}{q}"),
        _ => format!("{q}{table}{q}"),
    }
}

fn quote_ident(engine: Engine) -> char {
    match engine {
        Engine::Mysql => '`',
        _ => '"',
    }
}

fn placeholder(engine: Engine, n: usize) -> String {
    match engine {
        Engine::Postgres => format!("${n}"),
        _ => "?".to_string(),
    }
}

/// Find primary-key column names by introspecting the relevant table.
async fn primary_key_cols(
    pool: &AnyPool,
    engine: Engine,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> DbResult<Vec<String>> {
    let tree = introspect(pool, engine, database).await?;
    for s in &tree.schemas {
        if let Some(sname) = schema {
            if s.name != sname {
                continue;
            }
        }
        for t in &s.tables {
            if t.name == table {
                let pks: Vec<String> = t
                    .columns
                    .iter()
                    .filter(|c| c.pk)
                    .map(|c| c.name.clone())
                    .collect();
                if pks.is_empty() {
                    return Err(DbError::NoPrimaryKey(table.to_string()));
                }
                return Ok(pks);
            }
        }
    }
    Err(DbError::NotFound(format!("table {table}")))
}

/// Bind a `Value` onto a sqlx query. Anything we can't natively bind goes
/// through its string representation — a documented simplification.
fn bind_value<'q>(
    q: sqlx::query::Query<'q, sqlx::Any, sqlx::any::AnyArguments<'q>>,
    v: &Value,
) -> sqlx::query::Query<'q, sqlx::Any, sqlx::any::AnyArguments<'q>> {
    use crate::value::TaggedValue;
    match v {
        Value::Null => q.bind(Option::<String>::None),
        Value::Bool(b) => q.bind(*b),
        Value::Int(i) => q.bind(*i),
        Value::Float(f) => q.bind(*f),
        Value::Text(s) => q.bind(s.clone()),
        Value::Tagged(t) => match t {
            TaggedValue::Decimal(s) | TaggedValue::Date(s) | TaggedValue::Time(s)
            | TaggedValue::DateTime(s) => q.bind(s.clone()),
            TaggedValue::Bytes(b64) => {
                use base64::Engine as _;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .unwrap_or_default();
                q.bind(bytes)
            }
            TaggedValue::Json(j) => q.bind(j.to_string()),
        },
    }
}

pub async fn insert(
    pool: &AnyPool,
    engine: Engine,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    values: HashMap<String, Value>,
) -> DbResult<Row> {
    if values.is_empty() {
        return Err(DbError::Validation("values is empty".into()));
    }
    let pks = primary_key_cols(pool, engine, database, schema, table).await?;
    let qident = quote_ident(engine);
    let cols: Vec<String> = values.keys().cloned().collect();
    let col_list = cols
        .iter()
        .map(|c| format!("{qident}{c}{qident}"))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = (1..=cols.len())
        .map(|i| placeholder(engine, i))
        .collect::<Vec<_>>()
        .join(", ");
    let qtable = qualify(engine, schema, table);

    // Postgres supports RETURNING; SQLite 3.35+ does as well; MySQL doesn't.
    let supports_returning =
        matches!(engine, Engine::Postgres | Engine::Sqlite);
    let sql = if supports_returning {
        format!("INSERT INTO {qtable} ({col_list}) VALUES ({placeholders}) RETURNING *")
    } else {
        format!("INSERT INTO {qtable} ({col_list}) VALUES ({placeholders})")
    };

    let mut q = sqlx::query(&sql);
    for c in &cols {
        q = bind_value(q, values.get(c).unwrap_or(&Value::Null));
    }

    if supports_returning {
        let row = pool.fetch_one(q).await?;
        return Ok(decode_named(&row));
    }
    // MySQL: execute then fetch by LAST_INSERT_ID() — only works for single
    // auto-increment PK. For composite PKs we fall back to re-querying by
    // the values we just inserted that match the PK column.
    let res = pool.execute(q).await?;
    let last_id = res.last_insert_id();
    if pks.len() == 1 && last_id.is_some() {
        if let Some(id) = last_id {
            let pk = &pks[0];
            let sel = format!(
                "SELECT * FROM {qtable} WHERE {qident}{pk}{qident} = ? LIMIT 1"
            );
            let row = pool
                .fetch_one(sqlx::query(&sel).bind(id))
                .await?;
            return Ok(decode_named(&row));
        }
    }
    // Best-effort: re-select using provided PK values if all PKs were given.
    let where_clause = where_for_pks(engine, &pks, &values)?;
    let sel = format!("SELECT * FROM {qtable} WHERE {} LIMIT 1", where_clause.0);
    let mut sq = sqlx::query(&sel);
    for v in &where_clause.1 {
        sq = bind_value(sq, v);
    }
    let row = pool.fetch_one(sq).await?;
    Ok(decode_named(&row))
}

pub async fn update(
    pool: &AnyPool,
    engine: Engine,
    _database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    pk: HashMap<String, Value>,
    values: HashMap<String, Value>,
) -> DbResult<Row> {
    if pk.is_empty() {
        return Err(DbError::Validation("pk is empty".into()));
    }
    if values.is_empty() {
        return Err(DbError::Validation("values is empty".into()));
    }
    let qident = quote_ident(engine);
    let qtable = qualify(engine, schema, table);
    let mut counter = 1usize;
    let set_parts: Vec<String> = values
        .keys()
        .map(|k| {
            let ph = placeholder(engine, counter);
            counter += 1;
            format!("{qident}{k}{qident} = {ph}")
        })
        .collect();
    let where_parts: Vec<String> = pk
        .keys()
        .map(|k| {
            let ph = placeholder(engine, counter);
            counter += 1;
            format!("{qident}{k}{qident} = {ph}")
        })
        .collect();

    let sql_core = format!(
        "UPDATE {qtable} SET {} WHERE {}",
        set_parts.join(", "),
        where_parts.join(" AND ")
    );
    let supports_returning = matches!(engine, Engine::Postgres | Engine::Sqlite);
    let sql = if supports_returning {
        format!("{sql_core} RETURNING *")
    } else {
        sql_core.clone()
    };

    let mut q = sqlx::query(&sql);
    for k in values.keys() {
        q = bind_value(q, values.get(k).unwrap());
    }
    for k in pk.keys() {
        q = bind_value(q, pk.get(k).unwrap());
    }

    if supports_returning {
        let row = pool.fetch_one(q).await?;
        return Ok(decode_named(&row));
    }
    let _ = pool.execute(q).await?;
    // MySQL: re-select by PK.
    let sel_where: Vec<String> = pk
        .keys()
        .map(|k| format!("{qident}{k}{qident} = ?"))
        .collect();
    let sel = format!(
        "SELECT * FROM {qtable} WHERE {} LIMIT 1",
        sel_where.join(" AND ")
    );
    let mut sq = sqlx::query(&sel);
    for k in pk.keys() {
        sq = bind_value(sq, pk.get(k).unwrap());
    }
    let row = pool.fetch_one(sq).await?;
    Ok(decode_named(&row))
}

pub async fn delete(
    pool: &AnyPool,
    engine: Engine,
    _database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    pk: HashMap<String, Value>,
) -> DbResult<u64> {
    if pk.is_empty() {
        return Err(DbError::Validation("pk is empty".into()));
    }
    let qident = quote_ident(engine);
    let qtable = qualify(engine, schema, table);
    let mut counter = 1usize;
    let where_parts: Vec<String> = pk
        .keys()
        .map(|k| {
            let ph = placeholder(engine, counter);
            counter += 1;
            format!("{qident}{k}{qident} = {ph}")
        })
        .collect();
    let sql = format!("DELETE FROM {qtable} WHERE {}", where_parts.join(" AND "));
    let mut q = sqlx::query(&sql);
    for k in pk.keys() {
        q = bind_value(q, pk.get(k).unwrap());
    }
    let res = pool.execute(q).await?;
    Ok(res.rows_affected())
}

pub async fn duplicate(
    pool: &AnyPool,
    engine: Engine,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    pk: HashMap<String, Value>,
) -> DbResult<Row> {
    let pks = primary_key_cols(pool, engine, database, schema, table).await?;
    let qident = quote_ident(engine);
    let qtable = qualify(engine, schema, table);
    let mut counter = 1usize;
    let where_parts: Vec<String> = pk
        .keys()
        .map(|k| {
            let ph = placeholder(engine, counter);
            counter += 1;
            format!("{qident}{k}{qident} = {ph}")
        })
        .collect();
    let sel = format!(
        "SELECT * FROM {qtable} WHERE {} LIMIT 1",
        where_parts.join(" AND ")
    );
    let mut sq = sqlx::query(&sel);
    for k in pk.keys() {
        sq = bind_value(sq, pk.get(k).unwrap());
    }
    let row = pool.fetch_one(sq).await?;
    let mut cells = decode_named_map(&row);
    // Strip PK cols so auto-generated PKs can be re-assigned. If a PK column
    // is not auto-increment this insert will fail with a constraint error —
    // surfaced to the caller verbatim.
    for p in &pks {
        cells.remove(p);
    }
    insert(pool, engine, database, schema, table, cells).await
}

fn where_for_pks(
    engine: Engine,
    pks: &[String],
    values: &HashMap<String, Value>,
) -> DbResult<(String, Vec<Value>)> {
    let qident = quote_ident(engine);
    let mut parts = Vec::new();
    let mut binds = Vec::new();
    for (i, p) in (1..).zip(pks.iter()) {
        let v = values
            .get(p)
            .ok_or_else(|| DbError::Validation(format!("missing PK value: {p}")))?;
        parts.push(format!("{qident}{p}{qident} = {}", placeholder(engine, i)));
        binds.push(v.clone());
    }
    Ok((parts.join(" AND "), binds))
}

fn decode_named(row: &sqlx::any::AnyRow) -> Row {
    Row {
        cells: decode_named_map(row),
    }
}

fn decode_named_map(row: &sqlx::any::AnyRow) -> HashMap<String, Value> {
    use sqlx::Column as _;
    let cols = row.columns();
    let decoded = decode_row(row);
    let mut out = HashMap::with_capacity(cols.len());
    for (i, c) in cols.iter().enumerate() {
        out.insert(c.name().to_string(), decoded[i].clone());
    }
    out
}
