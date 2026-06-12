//! Export pipeline integration tests against in-temp SQLite.

use module_db::{
    export::export_page_sql_for_test, ConnectionInput, Engine, ExportFormat, ExportRequest,
    ExportScope, ExportTarget, Manager,
};
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

/// Seed `contacts` with a deliberately nasty value set to exercise CSV
/// escaping (comma, embedded newline, embedded quote, NULL).
async fn seed(mgr: &Manager, conn_id: &str) {
    mgr.query_run(
        conn_id,
        None,
        "CREATE TABLE contacts (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT, note TEXT)",
        None,
        10,
        0,
    )
    .await
    .unwrap();
    let rows = [
        ("Alice", "alice@x.com", "Some, comma"),
        ("Bob", "bob@x.com", "line1\nline2"),
        ("Carol", "carol@x.com", "she said \"hi\" and 'hello'"),
        ("Dan", "dan@x.com", ""), // empty string
    ];
    for (name, email, note) in rows {
        let sql = format!(
            "INSERT INTO contacts (name, email, note) VALUES ('{}', '{}', '{}')",
            name,
            email,
            note.replace('\'', "''")
        );
        mgr.query_run(conn_id, None, &sql, None, 10, 0)
            .await
            .unwrap();
    }
    // Row with a SQL NULL note.
    mgr.query_run(
        conn_id,
        None,
        "INSERT INTO contacts (name, email, note) VALUES ('Eve', 'eve@x.com', NULL)",
        None,
        10,
        0,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn csv_export_data_only_handles_rfc4180_edge_cases() {
    let (mgr, dir) = fresh_manager();
    let path = dir.path().join("contacts.db");
    let c = mgr.connections_add(sqlite_input(&path, "c")).unwrap();
    seed(&mgr, &c.id).await;

    let req = ExportRequest {
        database: None,
        target: ExportTarget::Table {
            schema: Some("main".into()),
            name: "contacts".into(),
            columns: None,
        },
        format: ExportFormat::Csv,
        scope: ExportScope::DataOnly,
    };
    let res = mgr.export(&c.id, req).await.expect("csv export");
    assert_eq!(res.content_type, "text/csv");
    assert!(res.filename.ends_with(".csv"));
    let body = String::from_utf8(res.body).unwrap();

    // Header row.
    assert!(body.starts_with("id,name,email,note\r\n"), "header: {body}");
    // Comma cell is quoted.
    assert!(
        body.contains("\"Some, comma\""),
        "expected quoted comma cell"
    );
    // Embedded newline quoted.
    assert!(
        body.contains("\"line1\nline2\""),
        "expected quoted newline cell"
    );
    // Embedded double-quote doubled and wrapped (RFC 4180).
    assert!(
        body.contains("\"she said \"\"hi\"\" and 'hello'\""),
        "expected doubled quotes; got:\n{body}"
    );
    // NULL is empty field.
    let last_line = body.lines().find(|l| l.starts_with("5,Eve")).unwrap();
    assert!(
        last_line.ends_with(",eve@x.com,"),
        "expected empty trailing NULL, got: {last_line}"
    );
}

#[tokio::test]
async fn json_export_schema_and_data() {
    let (mgr, dir) = fresh_manager();
    let path = dir.path().join("c.db");
    let c = mgr.connections_add(sqlite_input(&path, "c")).unwrap();
    seed(&mgr, &c.id).await;

    let req = ExportRequest {
        database: None,
        target: ExportTarget::Table {
            schema: Some("main".into()),
            name: "contacts".into(),
            columns: None,
        },
        format: ExportFormat::Json,
        scope: ExportScope::SchemaAndData,
    };
    let res = mgr.export(&c.id, req).await.expect("json export");
    assert_eq!(res.content_type, "application/json");
    let v: serde_json::Value = serde_json::from_slice(&res.body).unwrap();
    assert_eq!(v["table"], "contacts");
    let cols = v["columns"].as_array().unwrap();
    assert_eq!(cols.len(), 4);
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 5);
    // Eve's note must serialize as null.
    let eve = rows.iter().find(|r| r[1] == "Eve").unwrap();
    assert!(eve[3].is_null(), "eve.note expected null, got {eve:?}");
}

#[tokio::test]
async fn sql_export_schema_only_and_schema_and_data() {
    let (mgr, dir) = fresh_manager();
    let path = dir.path().join("c.db");
    let c = mgr.connections_add(sqlite_input(&path, "c")).unwrap();
    seed(&mgr, &c.id).await;

    // schema-only
    let req = ExportRequest {
        database: None,
        target: ExportTarget::Table {
            schema: Some("main".into()),
            name: "contacts".into(),
            columns: None,
        },
        format: ExportFormat::SqlInsert,
        scope: ExportScope::SchemaOnly,
    };
    let res = mgr.export(&c.id, req).await.unwrap();
    let s = String::from_utf8(res.body).unwrap();
    assert!(s.contains("CREATE TABLE \"contacts\""), "no CREATE: {s}");
    assert!(s.contains("\"id\""), "no id column: {s}");
    assert!(s.contains("PRIMARY KEY (\"id\")"), "no PK clause: {s}");
    assert!(
        !s.contains("INSERT INTO"),
        "schema-only must not emit INSERT"
    );

    // schema+data
    let req = ExportRequest {
        database: None,
        target: ExportTarget::Table {
            schema: Some("main".into()),
            name: "contacts".into(),
            columns: None,
        },
        format: ExportFormat::SqlInsert,
        scope: ExportScope::SchemaAndData,
    };
    let res = mgr.export(&c.id, req).await.unwrap();
    let s = String::from_utf8(res.body).unwrap();
    assert!(s.contains("CREATE TABLE"));
    assert!(
        s.contains("INSERT INTO \"contacts\" (\"id\", \"name\", \"email\", \"note\")"),
        "no INSERT batch: {s}"
    );
    // Single quotes in stored value were 'hello' — must be doubled.
    assert!(s.contains("''hello''"), "missing single-quote escape: {s}");
}

#[tokio::test]
async fn export_table_uses_keyset_sql_for_simple_pk_and_matches_data() {
    let (mgr, dir) = fresh_manager();
    let path = dir.path().join("keyset.db");
    let c = mgr.connections_add(sqlite_input(&path, "keyset")).unwrap();
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
    for id in 1..=12 {
        mgr.query_run(
            &c.id,
            None,
            &format!("INSERT INTO items (id, name) VALUES ({id}, 'item-{id}')"),
            None,
            10,
            0,
        )
        .await
        .unwrap();
    }

    let tree = mgr.schema_tree(&c.id, None).await.unwrap();
    let table = tree.schemas[0]
        .tables
        .iter()
        .find(|table| table.name == "items")
        .unwrap();
    let selected = table.columns.iter().collect::<Vec<_>>();
    let first_sql =
        export_page_sql_for_test(Engine::Sqlite, Some("main"), table, &selected, None, 5, 0);
    let next_sql = export_page_sql_for_test(
        Engine::Sqlite,
        Some("main"),
        table,
        &selected,
        Some(&module_db::Value::Int(5)),
        5,
        5,
    );
    assert_eq!(
        first_sql,
        "SELECT \"id\", \"name\" FROM \"items\" ORDER BY \"id\" ASC LIMIT 5"
    );
    assert_eq!(
        next_sql,
        "SELECT \"id\", \"name\" FROM \"items\" WHERE \"id\" > 5 ORDER BY \"id\" ASC LIMIT 5"
    );
    assert!(!first_sql.contains("OFFSET"));
    assert!(!next_sql.contains("OFFSET"));

    let req = ExportRequest {
        database: None,
        target: ExportTarget::Table {
            schema: Some("main".into()),
            name: "items".into(),
            columns: None,
        },
        format: ExportFormat::Json,
        scope: ExportScope::DataOnly,
    };
    let res = mgr.export(&c.id, req).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&res.body).unwrap();
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 12);
    assert_eq!(rows[0], serde_json::json!([1, "item-1"]));
    assert_eq!(rows[11], serde_json::json!([12, "item-12"]));
}

#[test]
fn export_sql_falls_back_to_offset_without_simple_pk() {
    let table = module_db::Table {
        name: "events".into(),
        kind: module_db::TableKind::Table,
        row_estimate: None,
        columns: vec![module_db::Column {
            name: "name".into(),
            r#type: "TEXT".into(),
            nullable: false,
            pk: false,
            default: None,
            kind: None,
        }],
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
    };
    let selected = table.columns.iter().collect::<Vec<_>>();

    let sql = export_page_sql_for_test(
        Engine::Sqlite,
        Some("main"),
        &table,
        &selected,
        None,
        10,
        20,
    );

    assert_eq!(sql, "SELECT \"name\" FROM \"events\" LIMIT 10 OFFSET 20");
}

#[tokio::test]
async fn sql_export_quotes_embedded_identifier_quotes() {
    let (mgr, dir) = fresh_manager();
    let path = dir.path().join("quoted.db");
    let c = mgr.connections_add(sqlite_input(&path, "quoted")).unwrap();
    mgr.query_run(
        &c.id,
        None,
        r#"CREATE TABLE "odd""table" ("id" INTEGER PRIMARY KEY, "a""b" TEXT NOT NULL)"#,
        None,
        10,
        0,
    )
    .await
    .unwrap();
    mgr.query_run(
        &c.id,
        None,
        r#"INSERT INTO "odd""table" ("a""b") VALUES ('value')"#,
        None,
        10,
        0,
    )
    .await
    .unwrap();

    let req = ExportRequest {
        database: None,
        target: ExportTarget::Table {
            schema: Some("main".into()),
            name: "odd\"table".into(),
            columns: None,
        },
        format: ExportFormat::SqlInsert,
        scope: ExportScope::SchemaAndData,
    };
    let res = mgr.export(&c.id, req).await.unwrap();
    let s = String::from_utf8(res.body).unwrap();
    assert!(s.contains("CREATE TABLE \"odd\"\"table\""), "{s}");
    assert!(s.contains("\"a\"\"b\" TEXT NOT NULL"), "{s}");
    assert!(
        s.contains("INSERT INTO \"odd\"\"table\" (\"id\", \"a\"\"b\")"),
        "{s}"
    );
}

#[tokio::test]
async fn column_subset_applies_to_all_formats() {
    let (mgr, dir) = fresh_manager();
    let path = dir.path().join("c.db");
    let c = mgr.connections_add(sqlite_input(&path, "c")).unwrap();
    seed(&mgr, &c.id).await;

    let cols = Some(vec!["name".to_string(), "email".to_string()]);

    // CSV
    let req = ExportRequest {
        database: None,
        target: ExportTarget::Table {
            schema: Some("main".into()),
            name: "contacts".into(),
            columns: cols.clone(),
        },
        format: ExportFormat::Csv,
        scope: ExportScope::DataOnly,
    };
    let s = String::from_utf8(mgr.export(&c.id, req).await.unwrap().body).unwrap();
    assert!(s.starts_with("name,email\r\n"), "header: {s}");
    assert!(!s.contains("note"), "note must be excluded");

    // JSON
    let req = ExportRequest {
        database: None,
        target: ExportTarget::Table {
            schema: Some("main".into()),
            name: "contacts".into(),
            columns: cols.clone(),
        },
        format: ExportFormat::Json,
        scope: ExportScope::SchemaAndData,
    };
    let v: serde_json::Value =
        serde_json::from_slice(&mgr.export(&c.id, req).await.unwrap().body).unwrap();
    let names: Vec<String> = v["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, vec!["name", "email"]);

    // SQL
    let req = ExportRequest {
        database: None,
        target: ExportTarget::Table {
            schema: Some("main".into()),
            name: "contacts".into(),
            columns: cols,
        },
        format: ExportFormat::SqlInsert,
        scope: ExportScope::SchemaAndData,
    };
    let s = String::from_utf8(mgr.export(&c.id, req).await.unwrap().body).unwrap();
    assert!(s.contains("INSERT INTO \"contacts\" (\"name\", \"email\")"));
    assert!(!s.contains("\"note\""));
    // The id column was dropped from the subset, so the implicit PK clause
    // should also be gone (no PK columns survived).
    assert!(!s.contains("PRIMARY KEY"), "stale PK clause: {s}");
}

#[tokio::test]
async fn column_subset_rejects_unknown_columns() {
    let (mgr, dir) = fresh_manager();
    let path = dir.path().join("c.db");
    let c = mgr.connections_add(sqlite_input(&path, "c")).unwrap();
    seed(&mgr, &c.id).await;

    let req = ExportRequest {
        database: None,
        target: ExportTarget::Table {
            schema: Some("main".into()),
            name: "contacts".into(),
            columns: Some(vec!["name".into(), "ghost".into(), "phantom".into()]),
        },
        format: ExportFormat::Csv,
        scope: ExportScope::DataOnly,
    };
    let err = mgr.export(&c.id, req).await.expect_err("should reject");
    let msg = err.to_string();
    assert!(msg.contains("ghost"), "expected ghost in error: {msg}");
    assert!(msg.contains("phantom"), "expected phantom in error: {msg}");
}

#[tokio::test]
async fn schema_target_json_includes_all_tables_csv_refused() {
    let (mgr, dir) = fresh_manager();
    let path = dir.path().join("c.db");
    let c = mgr.connections_add(sqlite_input(&path, "c")).unwrap();
    seed(&mgr, &c.id).await;
    mgr.query_run(
        &c.id,
        None,
        "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT)",
        None,
        10,
        0,
    )
    .await
    .unwrap();
    mgr.query_run(
        &c.id,
        None,
        "INSERT INTO notes (body) VALUES ('hello')",
        None,
        10,
        0,
    )
    .await
    .unwrap();

    // JSON schema-target lists both tables.
    let req = ExportRequest {
        database: None,
        target: ExportTarget::Schema {
            name: "main".into(),
        },
        format: ExportFormat::Json,
        scope: ExportScope::SchemaAndData,
    };
    let res = mgr.export(&c.id, req).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&res.body).unwrap();
    assert_eq!(v["schema"], "main");
    let tables = v["tables"].as_array().unwrap();
    let names: Vec<&str> = tables
        .iter()
        .map(|t| t["table"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"contacts"));
    assert!(names.contains(&"notes"));

    // CSV schema-target is refused with a clear error.
    let req = ExportRequest {
        database: None,
        target: ExportTarget::Schema {
            name: "main".into(),
        },
        format: ExportFormat::Csv,
        scope: ExportScope::DataOnly,
    };
    let err = mgr
        .export(&c.id, req)
        .await
        .expect_err("csv schema refused");
    let msg = err.to_string();
    assert!(
        msg.to_lowercase().contains("csv") && msg.to_lowercase().contains("schema"),
        "expected csv/schema in error: {msg}"
    );
}
