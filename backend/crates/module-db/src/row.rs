//! Row-level CRUD without manual SQL. Requires the target table to have a
//! detectable primary key; otherwise we refuse so callers don't accidentally
//! affect many rows.

use std::collections::HashMap;

use sqlx::{Column as _, Row as _};

use crate::error::{DbError, DbResult};
use crate::pool::DbPool;
use crate::schema::introspect;
use crate::types::{Engine, Row};
use crate::value::{decode_mysql_row, decode_postgres_row, decode_sqlite_row, Value};

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

async fn primary_key_cols(
    pool: &DbPool,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> DbResult<Vec<String>> {
    let tree = introspect(pool, pool.engine(), database).await?;
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

// ---- Per-engine bind helpers ----------------------------------------------
//
// sqlx's `query::Query<DB, ...>` differs per DB so we can't share a generic
// `bind_value`. Each engine gets its own.

macro_rules! bind_impl {
    ($q:ident, $v:expr) => {{
        use crate::value::TaggedValue;
        match $v {
            Value::Null => $q.bind(Option::<String>::None),
            Value::Bool(b) => $q.bind(*b),
            Value::Int(i) => $q.bind(*i),
            Value::Float(f) => $q.bind(*f),
            Value::Text(s) => $q.bind(s.clone()),
            Value::Tagged(t) => match t {
                TaggedValue::Decimal(s)
                | TaggedValue::Date(s)
                | TaggedValue::Time(s)
                | TaggedValue::DateTime(s) => $q.bind(s.clone()),
                TaggedValue::Bytes(b64) => {
                    use base64::Engine as _;
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(b64)
                        .unwrap_or_default();
                    $q.bind(bytes)
                }
                TaggedValue::Json(j) => $q.bind(j.to_string()),
            },
        }
    }};
}

fn bind_sqlite<'q>(
    q: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    v: &Value,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    bind_impl!(q, v)
}

fn bind_pg<'q>(
    q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    v: &Value,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    bind_impl!(q, v)
}

fn bind_mysql<'q>(
    q: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    v: &Value,
) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    bind_impl!(q, v)
}

// ---- Insert ----------------------------------------------------------------

pub async fn insert(
    pool: &DbPool,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    values: HashMap<String, Value>,
) -> DbResult<Row> {
    if values.is_empty() {
        return Err(DbError::Validation("values is empty".into()));
    }
    let engine = pool.engine();
    let pks = primary_key_cols(pool, database, schema, table).await?;
    let qident = quote_ident(engine);

    // Drop NULL/empty values so the database picks up column defaults (serial
    // sequences, CURRENT_TIMESTAMP, etc.). Binding NULL through sqlx::Any has
    // to choose a Rust type (Option<String>) which postgres then rejects for
    // non-text columns. Letting the engine fall back to its own DEFAULT is
    // both safer and matches the "empty = default" UX the inline insert form
    // promises.
    let cols: Vec<String> = values
        .iter()
        .filter(|(_, v)| !matches!(v, Value::Null))
        .map(|(k, _)| k.clone())
        .collect();
    let qtable = qualify(engine, schema, table);

    let supports_returning = matches!(engine, Engine::Postgres | Engine::Sqlite);
    let sql = if cols.is_empty() {
        // All-defaults insert. Postgres/SQLite have DEFAULT VALUES; MySQL needs
        // a different shape.
        match engine {
            Engine::Postgres | Engine::Sqlite => {
                if supports_returning {
                    format!("INSERT INTO {qtable} DEFAULT VALUES RETURNING *")
                } else {
                    format!("INSERT INTO {qtable} DEFAULT VALUES")
                }
            }
            Engine::Mysql => format!("INSERT INTO {qtable} () VALUES ()"),
        }
    } else {
        let col_list = cols
            .iter()
            .map(|c| format!("{qident}{c}{qident}"))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = (1..=cols.len())
            .map(|i| placeholder(engine, i))
            .collect::<Vec<_>>()
            .join(", ");
        if supports_returning {
            format!("INSERT INTO {qtable} ({col_list}) VALUES ({placeholders}) RETURNING *")
        } else {
            format!("INSERT INTO {qtable} ({col_list}) VALUES ({placeholders})")
        }
    };

    match pool {
        DbPool::Sqlite(p) => {
            let mut q = sqlx::query(&sql);
            for c in &cols {
                q = bind_sqlite(q, values.get(c).unwrap_or(&Value::Null));
            }
            let row = q.fetch_one(p).await?;
            let cells = named_map_sqlite(&row);
            Ok(Row { cells })
        }
        DbPool::Postgres(p) => {
            let mut q = sqlx::query(&sql);
            for c in &cols {
                q = bind_pg(q, values.get(c).unwrap_or(&Value::Null));
            }
            let row = q.fetch_one(p).await?;
            let cells = named_map_pg(&row);
            Ok(Row { cells })
        }
        DbPool::Mysql(p) => {
            let mut q = sqlx::query(&sql);
            for c in &cols {
                q = bind_mysql(q, values.get(c).unwrap_or(&Value::Null));
            }
            let res = q.execute(p).await?;
            let last_id = res.last_insert_id();
            // MySQL: re-select by LAST_INSERT_ID() for single auto-PK, else by provided PKs.
            if pks.len() == 1 && last_id != 0 {
                let pk = &pks[0];
                let sel = format!("SELECT * FROM {qtable} WHERE {qident}{pk}{qident} = ? LIMIT 1");
                let row = sqlx::query(&sel).bind(last_id).fetch_one(p).await?;
                let cells = named_map_mysql(&row);
                return Ok(Row { cells });
            }
            let where_clause = where_for_pks(engine, &pks, &values)?;
            let sel = format!("SELECT * FROM {qtable} WHERE {} LIMIT 1", where_clause.0);
            let mut sq = sqlx::query(&sel);
            for v in &where_clause.1 {
                sq = bind_mysql(sq, v);
            }
            let row = sq.fetch_one(p).await?;
            Ok(Row {
                cells: named_map_mysql(&row),
            })
        }
    }
}

// ---- Update ----------------------------------------------------------------

pub async fn update(
    pool: &DbPool,
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
    let engine = pool.engine();
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

    match pool {
        DbPool::Sqlite(p) => {
            let mut q = sqlx::query(&sql);
            for k in values.keys() {
                q = bind_sqlite(q, values.get(k).unwrap());
            }
            for k in pk.keys() {
                q = bind_sqlite(q, pk.get(k).unwrap());
            }
            let row = q.fetch_one(p).await?;
            Ok(Row {
                cells: named_map_sqlite(&row),
            })
        }
        DbPool::Postgres(p) => {
            let mut q = sqlx::query(&sql);
            for k in values.keys() {
                q = bind_pg(q, values.get(k).unwrap());
            }
            for k in pk.keys() {
                q = bind_pg(q, pk.get(k).unwrap());
            }
            let row = q.fetch_one(p).await?;
            Ok(Row {
                cells: named_map_pg(&row),
            })
        }
        DbPool::Mysql(p) => {
            let mut q = sqlx::query(&sql);
            for k in values.keys() {
                q = bind_mysql(q, values.get(k).unwrap());
            }
            for k in pk.keys() {
                q = bind_mysql(q, pk.get(k).unwrap());
            }
            let _ = q.execute(p).await?;
            // Re-select by PK.
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
                sq = bind_mysql(sq, pk.get(k).unwrap());
            }
            let row = sq.fetch_one(p).await?;
            Ok(Row {
                cells: named_map_mysql(&row),
            })
        }
    }
}

// ---- Delete ----------------------------------------------------------------

pub async fn delete(
    pool: &DbPool,
    _database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    pk: HashMap<String, Value>,
) -> DbResult<u64> {
    if pk.is_empty() {
        return Err(DbError::Validation("pk is empty".into()));
    }
    let engine = pool.engine();
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
    match pool {
        DbPool::Sqlite(p) => {
            let mut q = sqlx::query(&sql);
            for k in pk.keys() {
                q = bind_sqlite(q, pk.get(k).unwrap());
            }
            let res = q.execute(p).await?;
            Ok(res.rows_affected())
        }
        DbPool::Postgres(p) => {
            let mut q = sqlx::query(&sql);
            for k in pk.keys() {
                q = bind_pg(q, pk.get(k).unwrap());
            }
            let res = q.execute(p).await?;
            Ok(res.rows_affected())
        }
        DbPool::Mysql(p) => {
            let mut q = sqlx::query(&sql);
            for k in pk.keys() {
                q = bind_mysql(q, pk.get(k).unwrap());
            }
            let res = q.execute(p).await?;
            Ok(res.rows_affected())
        }
    }
}

// ---- Duplicate -------------------------------------------------------------

pub async fn duplicate(
    pool: &DbPool,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    pk: HashMap<String, Value>,
) -> DbResult<Row> {
    let engine = pool.engine();
    let pks = primary_key_cols(pool, database, schema, table).await?;
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
    let mut cells = match pool {
        DbPool::Sqlite(p) => {
            let mut sq = sqlx::query(&sel);
            for k in pk.keys() {
                sq = bind_sqlite(sq, pk.get(k).unwrap());
            }
            let row = sq.fetch_one(p).await?;
            named_map_sqlite(&row)
        }
        DbPool::Postgres(p) => {
            let mut sq = sqlx::query(&sel);
            for k in pk.keys() {
                sq = bind_pg(sq, pk.get(k).unwrap());
            }
            let row = sq.fetch_one(p).await?;
            named_map_pg(&row)
        }
        DbPool::Mysql(p) => {
            let mut sq = sqlx::query(&sel);
            for k in pk.keys() {
                sq = bind_mysql(sq, pk.get(k).unwrap());
            }
            let row = sq.fetch_one(p).await?;
            named_map_mysql(&row)
        }
    };
    // Strip PK cols so auto-generated PKs can be re-assigned.
    for p in &pks {
        cells.remove(p);
    }
    insert(pool, database, schema, table, cells).await
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

fn named_map_sqlite(row: &sqlx::sqlite::SqliteRow) -> HashMap<String, Value> {
    let cols = row.columns();
    let decoded = decode_sqlite_row(row);
    let mut out = HashMap::with_capacity(cols.len());
    for (i, c) in cols.iter().enumerate() {
        out.insert(c.name().to_string(), decoded[i].clone());
    }
    out
}

fn named_map_pg(row: &sqlx::postgres::PgRow) -> HashMap<String, Value> {
    let cols = row.columns();
    let decoded = decode_postgres_row(row);
    let mut out = HashMap::with_capacity(cols.len());
    for (i, c) in cols.iter().enumerate() {
        out.insert(c.name().to_string(), decoded[i].clone());
    }
    out
}

fn named_map_mysql(row: &sqlx::mysql::MySqlRow) -> HashMap<String, Value> {
    let cols = row.columns();
    let decoded = decode_mysql_row(row);
    let mut out = HashMap::with_capacity(cols.len());
    for (i, c) in cols.iter().enumerate() {
        out.insert(c.name().to_string(), decoded[i].clone());
    }
    out
}
