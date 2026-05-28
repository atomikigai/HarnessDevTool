//! `module-db` — DB Manager backend module.
//!
//! Manages SQLite/Postgres/MySQL connections, schema introspection, query
//! execution with pagination and cancellation, and row-level CRUD. Designed
//! to be embedded inside `harness-server` (REST surface) and exposed via
//! `harness-mcp-server` (`db.query`, `db.schema`, `db.explain`).
//!
//! Storage layout (per profile):
//!   ~/.harness/profiles/<profile>/modules/db/connections.toml
//! Passwords NEVER live in TOML; they are stored via the OS keyring under
//! `harness:db:<connection_id>`.

pub mod error;
pub mod export;
pub mod lease;
pub mod manager;
pub mod pool;
pub mod query;
pub mod row;
pub mod schema;
pub mod storage;
pub mod types;
pub mod value;

pub use error::{DbError, DbResult};
pub use export::{ExportFormat, ExportRequest, ExportResult, ExportScope, ExportTarget};
pub use lease::PinnedTab;
pub use manager::Manager;
pub use types::{
    Column, ColumnKind, Connection, ConnectionInput, Engine, ForeignKey, Index, QueryResult, Row,
    SchemaTree, SchemaTreeSchema, SslMode, Table, TableKind, TestResult,
};
pub use value::Value;

/// Re-export of `query::leading_keyword` for MCP tools that need to decide
/// whether a SQL statement is read-only. Underscored to discourage frontend
/// dependence.
#[doc(hidden)]
pub fn __leading_keyword(sql: &str) -> String {
    query::leading_keyword(sql)
}
