//! Env-gated integration tests for Postgres. Skipped cleanly when
//! `HARNESS_TEST_PG_URL` is not set. Expected format:
//!   postgres://user:pass@host:5432/dbname
//!
//! These cover the AnyPool regression: decoding `uuid`, `jsonb`, `numeric`,
//! `timestamptz` from a real postgres connection.

use module_db::value::TaggedValue;
use module_db::{ConnectionInput, Engine, Manager, Value};
use tempfile::TempDir;

/// Very small DSN parser — good enough for typical test URLs without
/// pulling in the `url` crate.
fn parse_pg_url(url: &str) -> Option<ConnectionInput> {
    let rest = url
        .strip_prefix("postgres://")
        .or_else(|| url.strip_prefix("postgresql://"))?;
    let (cred_host, db) = rest.split_once('/').unwrap_or((rest, ""));
    let (cred, host_port) = match cred_host.rsplit_once('@') {
        Some((c, h)) => (Some(c), h),
        None => (None, cred_host),
    };
    let (username, password) = match cred {
        Some(c) => match c.split_once(':') {
            Some((u, p)) => (Some(u.to_string()), Some(p.to_string())),
            None => (Some(c.to_string()), None),
        },
        None => (None, None),
    };
    let (host, port) = match host_port.split_once(':') {
        Some((h, p)) => (Some(h.to_string()), p.parse::<u16>().ok()),
        None => (Some(host_port.to_string()), None),
    };
    let database = db.split('?').next().unwrap_or("").to_string();
    Some(ConnectionInput {
        name: "pg-it".into(),
        engine: Engine::Postgres,
        host,
        port,
        database,
        username,
        password,
        ssl_mode: None,
        params: Default::default(),
    })
}

#[tokio::test]
async fn pg_decodes_engine_specific_types() {
    let Ok(url) = std::env::var("HARNESS_TEST_PG_URL") else {
        eprintln!("HARNESS_TEST_PG_URL not set — skipping");
        return;
    };
    let Some(input) = parse_pg_url(&url) else {
        panic!("HARNESS_TEST_PG_URL is not a valid postgres URL");
    };
    let dir = TempDir::new().unwrap();
    let mgr = Manager::new(dir.path(), "default").unwrap();
    let conn = mgr.connections_add(input).expect("add pg connection");

    let dbs = mgr.databases_list(&conn.id).await.expect("list dbs");
    assert!(!dbs.is_empty(), "expected at least one database");

    let tree = mgr.schema_tree(&conn.id, None).await.expect("schema tree");
    assert!(!tree.schemas.is_empty(), "expected at least one schema");

    let res = mgr
        .query_run(
            &conn.id,
            None,
            "SELECT gen_random_uuid() AS u, \
                    '{\"k\":1}'::jsonb AS j, \
                    123.456::numeric AS n, \
                    now() AS ts",
            None,
            10,
            0,
        )
        .await
        .expect("query");
    assert_eq!(res.rows.len(), 1);
    let row = &res.rows[0];
    assert!(matches!(row[0], Value::Text(_)), "uuid: {:?}", row[0]);
    assert!(
        matches!(row[1], Value::Tagged(TaggedValue::Json(_))),
        "jsonb: {:?}",
        row[1]
    );
    assert!(
        matches!(row[2], Value::Tagged(TaggedValue::Decimal(_))),
        "numeric: {:?}",
        row[2]
    );
    assert!(
        matches!(row[3], Value::Tagged(TaggedValue::DateTime(_))),
        "timestamptz: {:?}",
        row[3]
    );
}
