//! Wire types — REST request/response shapes and ts-rs exports.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Sqlite,
    Postgres,
    Mysql,
}

impl Engine {
    pub fn as_str(self) -> &'static str {
        match self {
            Engine::Sqlite => "sqlite",
            Engine::Postgres => "postgres",
            Engine::Mysql => "mysql",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(rename_all = "lowercase")]
pub enum SslMode {
    Disable,
    Prefer,
    Require,
}

/// A saved connection. Never contains the password — `password_ref` is the
/// keyring key (`harness:db:<id>`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub engine: Engine,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub database: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_mode: Option<SslMode>,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input shape for add/update. Optional `password` is moved to the keyring.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ConnectionInput {
    pub name: String,
    pub engine: Engine,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    pub database: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub ssl_mode: Option<SslMode>,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct TestResult {
    pub ok: bool,
    pub latency_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(rename_all = "lowercase")]
pub enum TableKind {
    Table,
    View,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Column {
    pub name: String,
    #[serde(rename = "data_type")]
    #[cfg_attr(feature = "ts-export", ts(rename = "data_type"))]
    pub r#type: String,
    pub nullable: bool,
    pub pk: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ColumnKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
#[serde(tag = "kind")]
pub enum ColumnKind {
    Enum { variants: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Index {
    pub name: String,
    #[serde(rename = "columns")]
    #[cfg_attr(feature = "ts-export", ts(rename = "columns"))]
    pub cols: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ForeignKey {
    pub name: String,
    #[serde(rename = "columns")]
    #[cfg_attr(feature = "ts-export", ts(rename = "columns"))]
    pub cols: Vec<String>,
    pub ref_table: String,
    #[serde(rename = "ref_columns")]
    #[cfg_attr(feature = "ts-export", ts(rename = "ref_columns"))]
    pub ref_cols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Table {
    pub name: String,
    pub kind: TableKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_estimate: Option<i64>,
    pub columns: Vec<Column>,
    pub indexes: Vec<Index>,
    pub foreign_keys: Vec<ForeignKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct SchemaTreeSchema {
    pub name: String,
    pub tables: Vec<Table>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct SchemaTree {
    pub schemas: Vec<SchemaTreeSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct ResultColumn {
    pub name: String,
    pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct QueryResult {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_rows: Option<u64>,
    pub truncated: bool,
    pub elapsed_ms: u64,
    pub query_id: String,
}

/// A single row returned by row-level CRUD. `cells` is keyed by column name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct Row {
    pub cells: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct AffectedResult {
    pub affected: u64,
}
