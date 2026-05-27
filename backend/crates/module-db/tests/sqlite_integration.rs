//! Integration tests against an in-temp SQLite DB. No external services.

use std::collections::HashMap;

use module_db::{ConnectionInput, Engine, Manager, Value};
use tempfile::TempDir;

fn fresh_manager() -> (Manager, TempDir) {
    let dir = TempDir::new().unwrap();
    let mgr = Manager::new(dir.path(), "default").unwrap();
    (mgr, dir)
}

fn sqlite_input(db_path: &std::path::Path, name: &str) -> ConnectionInput {
    ConnectionInput {
        name: name.into(),
        engine: Engine::Sqlite,
        database: db_path.to_string_lossy().to_string(),
        ..Default::default()
    }
}

#[tokio::test]
async fn connection_crud_roundtrip() {
    let (mgr, dir) = fresh_manager();
    let db_path = dir.path().join("test1.db");

    // list empty
    assert!(mgr.connections_list().unwrap().is_empty());

    // add
    let c = mgr
        .connections_add(sqlite_input(&db_path, "one"))
        .expect("add");
    assert_eq!(c.name, "one");
    assert!(c.password_ref.is_none());

    // list shows one
    assert_eq!(mgr.connections_list().unwrap().len(), 1);

    // update name
    let mut input = sqlite_input(&db_path, "renamed");
    input.engine = Engine::Sqlite;
    let updated = mgr.connections_update(&c.id, input).unwrap();
    assert_eq!(updated.name, "renamed");

    // remove
    mgr.connections_remove(&c.id).unwrap();
    assert!(mgr.connections_list().unwrap().is_empty());
}

#[tokio::test]
async fn schema_and_query_pagination() {
    let (mgr, dir) = fresh_manager();
    let db_path = dir.path().join("test2.db");
    let c = mgr
        .connections_add(sqlite_input(&db_path, "two"))
        .unwrap();

    // Create a table and rows via query_run.
    mgr.query_run(
        &c.id,
        None,
        "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        None,
        10,
        0,
    )
    .await
    .unwrap();

    for i in 0..25 {
        let sql = format!("INSERT INTO items (name) VALUES ('item-{i}')");
        mgr.query_run(&c.id, None, &sql, None, 10, 0).await.unwrap();
    }

    // schema_tree shows the table with a PK.
    let tree = mgr.schema_tree(&c.id, None).await.unwrap();
    let table = tree
        .schemas
        .iter()
        .flat_map(|s| s.tables.iter())
        .find(|t| t.name == "items")
        .expect("table present");
    let pk_col = table.columns.iter().find(|c| c.pk).expect("pk");
    assert_eq!(pk_col.name, "id");

    // page 0 has 10 rows + truncated=true (since 11 fetched)
    let p0 = mgr
        .query_run(&c.id, None, "SELECT * FROM items ORDER BY id", None, 10, 0)
        .await
        .unwrap();
    assert_eq!(p0.rows.len(), 10);
    assert!(p0.truncated);

    // page 2 has the final 5 (rows 20..25), not truncated.
    let p2 = mgr
        .query_run(&c.id, None, "SELECT * FROM items ORDER BY id", None, 10, 2)
        .await
        .unwrap();
    assert_eq!(p2.rows.len(), 5);
    assert!(!p2.truncated);
}

#[tokio::test]
async fn row_crud_composite_pk() {
    let (mgr, dir) = fresh_manager();
    let db_path = dir.path().join("test3.db");
    let c = mgr
        .connections_add(sqlite_input(&db_path, "three"))
        .unwrap();

    mgr.query_run(
        &c.id,
        None,
        "CREATE TABLE memberships (org_id INTEGER, user_id INTEGER, role TEXT, PRIMARY KEY (org_id, user_id))",
        None,
        10,
        0,
    )
    .await
    .unwrap();

    // insert one row via row_insert
    let mut values = HashMap::new();
    values.insert("org_id".to_string(), Value::Int(1));
    values.insert("user_id".to_string(), Value::Int(42));
    values.insert("role".to_string(), Value::Text("admin".into()));
    let row = mgr
        .row_insert(&c.id, None, None, "memberships", values)
        .await
        .expect("insert");
    assert_eq!(row.cells.get("role").unwrap(), &Value::Text("admin".into()));

    // update
    let mut pk = HashMap::new();
    pk.insert("org_id".to_string(), Value::Int(1));
    pk.insert("user_id".to_string(), Value::Int(42));
    let mut new_vals = HashMap::new();
    new_vals.insert("role".to_string(), Value::Text("member".into()));
    let updated = mgr
        .row_update(&c.id, None, None, "memberships", pk.clone(), new_vals)
        .await
        .expect("update");
    assert_eq!(
        updated.cells.get("role").unwrap(),
        &Value::Text("member".into())
    );

    // delete
    let n = mgr
        .row_delete(&c.id, None, None, "memberships", pk)
        .await
        .expect("delete");
    assert_eq!(n, 1);
}

#[tokio::test]
async fn row_insert_refuses_pkless_table() {
    let (mgr, dir) = fresh_manager();
    let db_path = dir.path().join("test4.db");
    let c = mgr
        .connections_add(sqlite_input(&db_path, "four"))
        .unwrap();
    mgr.query_run(
        &c.id,
        None,
        "CREATE TABLE log (msg TEXT NOT NULL)",
        None,
        10,
        0,
    )
    .await
    .unwrap();
    let mut values = HashMap::new();
    values.insert("msg".to_string(), Value::Text("hi".into()));
    let err = mgr
        .row_insert(&c.id, None, None, "log", values)
        .await
        .expect_err("should refuse");
    let msg = err.to_string();
    assert!(msg.contains("primary key"), "got: {msg}");
}

#[tokio::test]
async fn test_input_works_for_sqlite_file() {
    let (mgr, dir) = fresh_manager();
    let db_path = dir.path().join("test5.db");
    let res = mgr
        .connections_test_input(sqlite_input(&db_path, "tst"))
        .await
        .unwrap();
    assert!(res.ok, "{res:?}");
    assert!(res.server_version.is_some());
}
