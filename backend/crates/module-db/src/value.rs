//! JSON-friendly cell value + per-engine row decoders.
//!
//! `AnyRow` decoding was replaced with engine-specific paths because sqlx's
//! `Any` driver does not support engine-native types like Postgres `uuid`,
//! `jsonb`, `numeric`, `timestamptz`, MySQL `decimal`/`datetime`, etc.

use base64::Engine as _;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// Polymorphic cell value. Serialized as plain JSON values; non-JSON-native
/// types use string forms to preserve precision/bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(untagged)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Tagged(TaggedValue),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(tag = "_t", content = "v", rename_all = "snake_case")]
pub enum TaggedValue {
    Decimal(String),
    Bytes(String), // base64
    Date(String),
    Time(String),
    DateTime(String),
    Json(#[cfg_attr(feature = "ts-export", ts(type = "unknown"))] serde_json::Value),
}

impl Value {
    pub fn decimal(s: impl Into<String>) -> Self {
        Value::Tagged(TaggedValue::Decimal(s.into()))
    }
    pub fn bytes(b: &[u8]) -> Self {
        Value::Tagged(TaggedValue::Bytes(
            base64::engine::general_purpose::STANDARD.encode(b),
        ))
    }
    pub fn datetime(s: impl Into<String>) -> Self {
        Value::Tagged(TaggedValue::DateTime(s.into()))
    }
    pub fn date(s: impl Into<String>) -> Self {
        Value::Tagged(TaggedValue::Date(s.into()))
    }
    pub fn time(s: impl Into<String>) -> Self {
        Value::Tagged(TaggedValue::Time(s.into()))
    }
    pub fn json(v: serde_json::Value) -> Self {
        Value::Tagged(TaggedValue::Json(v))
    }
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

impl From<Value> for serde_json::Value {
    fn from(v: Value) -> Self {
        v.to_json()
    }
}

// ============================================================================
// Per-engine row decoders
// ============================================================================

use sqlx::{Row, TypeInfo, ValueRef};

// ---- SQLite ----------------------------------------------------------------

pub fn decode_sqlite_row(row: &sqlx::sqlite::SqliteRow) -> Vec<Value> {
    (0..row.columns().len())
        .map(|i| decode_sqlite_cell(row, i))
        .collect()
}

fn decode_sqlite_cell(row: &sqlx::sqlite::SqliteRow, idx: usize) -> Value {
    let raw = match row.try_get_raw(idx) {
        Ok(v) => v,
        Err(_) => return Value::Null,
    };
    if raw.is_null() {
        return Value::Null;
    }
    let type_name = raw.type_info().name().to_ascii_uppercase();
    // SQLite native affinities: INTEGER, REAL, TEXT, BLOB, NULL, BOOLEAN.
    match type_name.as_str() {
        "INTEGER" | "INT" | "INT8" | "BIGINT" => {
            if let Ok(v) = row.try_get::<i64, _>(idx) {
                return Value::Int(v);
            }
        }
        "REAL" | "FLOAT" | "DOUBLE" => {
            if let Ok(v) = row.try_get::<f64, _>(idx) {
                return Value::Float(v);
            }
        }
        "BOOLEAN" => {
            if let Ok(v) = row.try_get::<bool, _>(idx) {
                return Value::Bool(v);
            }
            if let Ok(v) = row.try_get::<i64, _>(idx) {
                return Value::Bool(v != 0);
            }
        }
        "BLOB" => {
            if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
                return Value::bytes(&v);
            }
        }
        _ => {}
    }
    // Try ladder.
    if let Ok(v) = row.try_get::<i64, _>(idx) {
        return Value::Int(v);
    }
    if let Ok(v) = row.try_get::<f64, _>(idx) {
        return Value::Float(v);
    }
    if let Ok(v) = row.try_get::<String, _>(idx) {
        return Value::Text(v);
    }
    if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
        return Value::bytes(&v);
    }
    Value::Text(format!("<unsupported:{type_name}>"))
}

// ---- Postgres --------------------------------------------------------------

pub fn decode_postgres_row(row: &sqlx::postgres::PgRow) -> Vec<Value> {
    (0..row.columns().len())
        .map(|i| decode_postgres_cell(row, i))
        .collect()
}

fn decode_postgres_cell(row: &sqlx::postgres::PgRow, idx: usize) -> Value {
    let raw = match row.try_get_raw(idx) {
        Ok(v) => v,
        Err(_) => return Value::Null,
    };
    if raw.is_null() {
        return Value::Null;
    }
    let type_name = raw.type_info().name().to_string();

    // Dispatch on the pg type name for hot paths.
    match type_name.as_str() {
        "BOOL" => {
            if let Ok(v) = row.try_get::<bool, _>(idx) {
                return Value::Bool(v);
            }
        }
        "INT2" => {
            if let Ok(v) = row.try_get::<i16, _>(idx) {
                return Value::Int(v as i64);
            }
        }
        "INT4" => {
            if let Ok(v) = row.try_get::<i32, _>(idx) {
                return Value::Int(v as i64);
            }
        }
        "INT8" => {
            if let Ok(v) = row.try_get::<i64, _>(idx) {
                return Value::Int(v);
            }
        }
        "FLOAT4" => {
            if let Ok(v) = row.try_get::<f32, _>(idx) {
                return Value::Float(v as f64);
            }
        }
        "FLOAT8" => {
            if let Ok(v) = row.try_get::<f64, _>(idx) {
                return Value::Float(v);
            }
        }
        "NUMERIC" => {
            if let Ok(v) = row.try_get::<rust_decimal::Decimal, _>(idx) {
                return Value::decimal(v.to_string());
            }
            if let Ok(v) = row.try_get::<String, _>(idx) {
                return Value::decimal(v);
            }
        }
        "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "CHAR" | "CITEXT" => {
            if let Ok(v) = row.try_get::<String, _>(idx) {
                return Value::Text(v);
            }
        }
        "UUID" => {
            if let Ok(v) = row.try_get::<uuid::Uuid, _>(idx) {
                return Value::Text(v.to_string());
            }
        }
        "JSON" | "JSONB" => {
            if let Ok(v) = row.try_get::<serde_json::Value, _>(idx) {
                return Value::json(v);
            }
        }
        "TIMESTAMP" => {
            if let Ok(v) = row.try_get::<chrono::NaiveDateTime, _>(idx) {
                return Value::datetime(v.format("%Y-%m-%dT%H:%M:%S%.f").to_string());
            }
        }
        "TIMESTAMPTZ" => {
            if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(idx) {
                return Value::datetime(v.to_rfc3339());
            }
        }
        "DATE" => {
            if let Ok(v) = row.try_get::<chrono::NaiveDate, _>(idx) {
                return Value::date(v.to_string());
            }
        }
        "TIME" | "TIMETZ" => {
            if let Ok(v) = row.try_get::<chrono::NaiveTime, _>(idx) {
                return Value::time(v.to_string());
            }
        }
        "BYTEA" => {
            if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
                return Value::bytes(&v);
            }
        }
        _ => {}
    }
    // Fallback ladder for less-common types.
    if let Ok(v) = row.try_get::<String, _>(idx) {
        return Value::Text(v);
    }
    if let Ok(v) = row.try_get::<i64, _>(idx) {
        return Value::Int(v);
    }
    if let Ok(v) = row.try_get::<bool, _>(idx) {
        return Value::Bool(v);
    }
    if let Ok(v) = row.try_get::<f64, _>(idx) {
        return Value::Float(v);
    }
    if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
        return Value::bytes(&v);
    }
    // Arrays, hstore, ranges, enums, composites, etc. land here.
    Value::Text(format!("<unsupported:{type_name}>"))
}

// ---- MySQL -----------------------------------------------------------------

pub fn decode_mysql_row(row: &sqlx::mysql::MySqlRow) -> Vec<Value> {
    (0..row.columns().len())
        .map(|i| decode_mysql_cell(row, i))
        .collect()
}

fn decode_mysql_cell(row: &sqlx::mysql::MySqlRow, idx: usize) -> Value {
    let raw = match row.try_get_raw(idx) {
        Ok(v) => v,
        Err(_) => return Value::Null,
    };
    if raw.is_null() {
        return Value::Null;
    }
    let type_name = raw.type_info().name().to_ascii_uppercase();

    match type_name.as_str() {
        "BOOLEAN" => {
            if let Ok(v) = row.try_get::<bool, _>(idx) {
                return Value::Bool(v);
            }
        }
        "TINYINT" => {
            if let Ok(v) = row.try_get::<i8, _>(idx) {
                return Value::Int(v as i64);
            }
            if let Ok(v) = row.try_get::<i16, _>(idx) {
                return Value::Int(v as i64);
            }
        }
        "TINYINT UNSIGNED" => {
            if let Ok(v) = row.try_get::<u8, _>(idx) {
                return Value::Int(v as i64);
            }
        }
        "SMALLINT" => {
            if let Ok(v) = row.try_get::<i16, _>(idx) {
                return Value::Int(v as i64);
            }
        }
        "SMALLINT UNSIGNED" => {
            if let Ok(v) = row.try_get::<u16, _>(idx) {
                return Value::Int(v as i64);
            }
        }
        "INT" | "MEDIUMINT" => {
            if let Ok(v) = row.try_get::<i32, _>(idx) {
                return Value::Int(v as i64);
            }
        }
        "INT UNSIGNED" | "MEDIUMINT UNSIGNED" => {
            if let Ok(v) = row.try_get::<u32, _>(idx) {
                return Value::Int(v as i64);
            }
        }
        "BIGINT" => {
            if let Ok(v) = row.try_get::<i64, _>(idx) {
                return Value::Int(v);
            }
        }
        "BIGINT UNSIGNED" => {
            if let Ok(v) = row.try_get::<u64, _>(idx) {
                return Value::Int(v as i64);
            }
        }
        "FLOAT" => {
            if let Ok(v) = row.try_get::<f32, _>(idx) {
                return Value::Float(v as f64);
            }
        }
        "DOUBLE" => {
            if let Ok(v) = row.try_get::<f64, _>(idx) {
                return Value::Float(v);
            }
        }
        "DECIMAL" | "NEWDECIMAL" => {
            if let Ok(v) = row.try_get::<rust_decimal::Decimal, _>(idx) {
                return Value::decimal(v.to_string());
            }
            if let Ok(v) = row.try_get::<String, _>(idx) {
                return Value::decimal(v);
            }
        }
        "VARCHAR" | "CHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM" | "SET" => {
            if let Ok(v) = row.try_get::<String, _>(idx) {
                return Value::Text(v);
            }
        }
        "JSON" => {
            if let Ok(v) = row.try_get::<String, _>(idx) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&v) {
                    return Value::json(parsed);
                }
                return Value::Text(v);
            }
        }
        "DATETIME" | "TIMESTAMP" => {
            if let Ok(v) = row.try_get::<chrono::NaiveDateTime, _>(idx) {
                return Value::datetime(v.format("%Y-%m-%dT%H:%M:%S%.f").to_string());
            }
            if let Ok(v) = row.try_get::<chrono::DateTime<chrono::Utc>, _>(idx) {
                return Value::datetime(v.to_rfc3339());
            }
        }
        "DATE" => {
            if let Ok(v) = row.try_get::<chrono::NaiveDate, _>(idx) {
                return Value::date(v.to_string());
            }
        }
        "TIME" => {
            if let Ok(v) = row.try_get::<chrono::NaiveTime, _>(idx) {
                return Value::time(v.to_string());
            }
        }
        "YEAR" => {
            if let Ok(v) = row.try_get::<u16, _>(idx) {
                return Value::Int(v as i64);
            }
        }
        "BINARY" | "VARBINARY" | "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" | "GEOMETRY" => {
            if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
                // UUIDs in MySQL are often BINARY(16); surface as text best-effort.
                if v.len() == 16 {
                    if let Ok(u) = uuid::Uuid::from_slice(&v) {
                        return Value::Text(u.to_string());
                    }
                }
                return Value::bytes(&v);
            }
        }
        // BIT(n) — sqlx returns Vec<u8>. For BIT(1) treat as bool.
        "BIT" => {
            if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
                if v.len() == 1 {
                    return Value::Bool(v[0] != 0);
                }
                return Value::bytes(&v);
            }
            if let Ok(v) = row.try_get::<u64, _>(idx) {
                return Value::Int(v as i64);
            }
        }
        _ => {}
    }
    // Fallback ladder.
    if let Ok(v) = row.try_get::<String, _>(idx) {
        return Value::Text(v);
    }
    if let Ok(v) = row.try_get::<i64, _>(idx) {
        return Value::Int(v);
    }
    if let Ok(v) = row.try_get::<f64, _>(idx) {
        return Value::Float(v);
    }
    if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
        return Value::bytes(&v);
    }
    Value::Text(format!("<unsupported:{type_name}>"))
}
