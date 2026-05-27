//! Schema introspection. Per-engine SQL — kept simple; advanced metadata
//! (check constraints, partial indexes, etc.) is intentionally omitted.

use std::collections::BTreeMap;

use sqlx::{AnyPool, Row};

use crate::error::DbResult;
use crate::types::{
    Column, Engine, ForeignKey, Index, SchemaTree, SchemaTreeSchema, Table, TableKind,
};

pub async fn introspect(
    pool: &AnyPool,
    engine: Engine,
    database: Option<&str>,
) -> DbResult<SchemaTree> {
    match engine {
        Engine::Sqlite => sqlite_tree(pool).await,
        Engine::Postgres => postgres_tree(pool, database).await,
        Engine::Mysql => mysql_tree(pool, database).await,
    }
}

async fn sqlite_tree(pool: &AnyPool) -> DbResult<SchemaTree> {
    let tables = sqlx::query(
        "SELECT name, type FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for t in tables {
        let name: String = t.try_get(0).unwrap_or_default();
        let kind_s: String = t.try_get(1).unwrap_or_else(|_| "table".to_string());
        let kind = if kind_s == "view" {
            TableKind::View
        } else {
            TableKind::Table
        };

        let cols = sqlx::query(&format!(
            "SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info('{}')",
            name.replace('\'', "''")
        ))
        .fetch_all(pool)
        .await?;
        let columns: Vec<Column> = cols
            .iter()
            .map(|c| {
                let cname: String = c.try_get(0).unwrap_or_default();
                let ctype: String = c.try_get(1).unwrap_or_default();
                let notnull: i64 = c.try_get(2).unwrap_or(0);
                let dflt: Option<String> = c.try_get(3).ok();
                let pk: i64 = c.try_get(4).unwrap_or(0);
                Column {
                    name: cname,
                    r#type: ctype,
                    nullable: notnull == 0,
                    pk: pk > 0,
                    default: dflt,
                }
            })
            .collect();

        let idx_rows = sqlx::query(&format!(
            "SELECT name, \"unique\" FROM pragma_index_list('{}')",
            name.replace('\'', "''")
        ))
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        let mut indexes = Vec::new();
        for ir in idx_rows {
            let iname: String = ir.try_get(0).unwrap_or_default();
            let uniq: i64 = ir.try_get(1).unwrap_or(0);
            let ic = sqlx::query(&format!(
                "SELECT name FROM pragma_index_info('{}')",
                iname.replace('\'', "''")
            ))
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            let cols: Vec<String> = ic
                .iter()
                .map(|r| r.try_get::<String, _>(0).unwrap_or_default())
                .collect();
            indexes.push(Index {
                name: iname,
                cols,
                unique: uniq != 0,
            });
        }

        let fk_rows = sqlx::query(&format!(
            "SELECT id, \"from\", \"table\", \"to\" FROM pragma_foreign_key_list('{}')",
            name.replace('\'', "''")
        ))
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        let mut by_id: BTreeMap<i64, ForeignKey> = BTreeMap::new();
        for fr in fk_rows {
            let id: i64 = fr.try_get(0).unwrap_or(0);
            let from_c: String = fr.try_get(1).unwrap_or_default();
            let ref_t: String = fr.try_get(2).unwrap_or_default();
            let to_c: String = fr.try_get(3).unwrap_or_default();
            let e = by_id.entry(id).or_insert_with(|| ForeignKey {
                name: format!("fk_{name}_{id}"),
                cols: Vec::new(),
                ref_table: ref_t.clone(),
                ref_cols: Vec::new(),
            });
            e.cols.push(from_c);
            e.ref_cols.push(to_c);
        }

        out.push(Table {
            name,
            kind,
            row_estimate: None,
            columns,
            indexes,
            foreign_keys: by_id.into_values().collect(),
        });
    }
    Ok(SchemaTree {
        schemas: vec![SchemaTreeSchema {
            name: "main".to_string(),
            tables: out,
        }],
    })
}

async fn postgres_tree(pool: &AnyPool, _database: Option<&str>) -> DbResult<SchemaTree> {
    // Tables grouped by schema (excluding pg internals).
    let rows = sqlx::query(
        "SELECT table_schema, table_name, table_type
         FROM information_schema.tables
         WHERE table_schema NOT IN ('pg_catalog','information_schema')
         ORDER BY table_schema, table_name",
    )
    .fetch_all(pool)
    .await?;

    let mut by_schema: BTreeMap<String, Vec<(String, TableKind)>> = BTreeMap::new();
    for r in rows {
        let s: String = r.try_get(0).unwrap_or_default();
        let n: String = r.try_get(1).unwrap_or_default();
        let t: String = r.try_get(2).unwrap_or_default();
        let kind = if t.eq_ignore_ascii_case("VIEW") {
            TableKind::View
        } else {
            TableKind::Table
        };
        by_schema.entry(s).or_default().push((n, kind));
    }

    let mut schemas = Vec::new();
    for (sname, tables) in by_schema {
        let mut out_tables = Vec::new();
        for (tname, kind) in tables {
            let cols = sqlx::query(
                "SELECT column_name, data_type, is_nullable, column_default
                 FROM information_schema.columns
                 WHERE table_schema = $1 AND table_name = $2
                 ORDER BY ordinal_position",
            )
            .bind(&sname)
            .bind(&tname)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            let pk_cols = sqlx::query(
                "SELECT kcu.column_name
                 FROM information_schema.table_constraints tc
                 JOIN information_schema.key_column_usage kcu
                   ON kcu.constraint_name = tc.constraint_name
                  AND kcu.table_schema = tc.table_schema
                 WHERE tc.constraint_type = 'PRIMARY KEY'
                   AND tc.table_schema = $1 AND tc.table_name = $2",
            )
            .bind(&sname)
            .bind(&tname)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            let pk_set: std::collections::HashSet<String> = pk_cols
                .iter()
                .map(|r| r.try_get::<String, _>(0).unwrap_or_default())
                .collect();
            let columns: Vec<Column> = cols
                .iter()
                .map(|c| {
                    let cname: String = c.try_get(0).unwrap_or_default();
                    let ctype: String = c.try_get(1).unwrap_or_default();
                    let nul: String = c.try_get(2).unwrap_or_default();
                    let dflt: Option<String> = c.try_get(3).ok();
                    Column {
                        pk: pk_set.contains(&cname),
                        name: cname,
                        r#type: ctype,
                        nullable: nul.eq_ignore_ascii_case("YES"),
                        default: dflt,
                    }
                })
                .collect();
            out_tables.push(Table {
                name: tname,
                kind,
                row_estimate: None,
                columns,
                indexes: Vec::new(),
                foreign_keys: Vec::new(),
            });
        }
        schemas.push(SchemaTreeSchema {
            name: sname,
            tables: out_tables,
        });
    }
    Ok(SchemaTree { schemas })
}

async fn mysql_tree(pool: &AnyPool, database: Option<&str>) -> DbResult<SchemaTree> {
    let db = match database {
        Some(d) => d.to_string(),
        None => {
            let r = sqlx::query("SELECT DATABASE()").fetch_one(pool).await?;
            r.try_get::<String, _>(0).unwrap_or_default()
        }
    };
    let rows = sqlx::query(
        "SELECT table_name, table_type FROM information_schema.tables
         WHERE table_schema = ? ORDER BY table_name",
    )
    .bind(&db)
    .fetch_all(pool)
    .await?;
    let mut tables = Vec::new();
    for r in rows {
        let n: String = r.try_get(0).unwrap_or_default();
        let t: String = r.try_get(1).unwrap_or_default();
        let kind = if t.to_uppercase().contains("VIEW") {
            TableKind::View
        } else {
            TableKind::Table
        };
        let cols = sqlx::query(
            "SELECT column_name, column_type, is_nullable, column_default, column_key
             FROM information_schema.columns
             WHERE table_schema = ? AND table_name = ?
             ORDER BY ordinal_position",
        )
        .bind(&db)
        .bind(&n)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        let columns: Vec<Column> = cols
            .iter()
            .map(|c| {
                let cname: String = c.try_get(0).unwrap_or_default();
                let ctype: String = c.try_get(1).unwrap_or_default();
                let nul: String = c.try_get(2).unwrap_or_default();
                let dflt: Option<String> = c.try_get(3).ok();
                let key: String = c.try_get(4).unwrap_or_default();
                Column {
                    name: cname,
                    r#type: ctype,
                    nullable: nul.eq_ignore_ascii_case("YES"),
                    pk: key.eq_ignore_ascii_case("PRI"),
                    default: dflt,
                }
            })
            .collect();
        tables.push(Table {
            name: n,
            kind,
            row_estimate: None,
            columns,
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
        });
    }
    Ok(SchemaTree {
        schemas: vec![SchemaTreeSchema {
            name: db,
            tables,
        }],
    })
}
