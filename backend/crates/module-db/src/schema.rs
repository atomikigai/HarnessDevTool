//! Schema introspection. Per-engine SQL — kept simple; advanced metadata
//! (check constraints, partial indexes, etc.) is intentionally omitted.

use std::collections::BTreeMap;

use sqlx::Row;

use crate::error::DbResult;
use crate::pool::DbPool;
use crate::types::{
    Column, ColumnKind, Engine, ForeignKey, Index, SchemaTree, SchemaTreeSchema, Table, TableKind,
};

pub async fn introspect(
    pool: &DbPool,
    _engine: Engine,
    database: Option<&str>,
) -> DbResult<SchemaTree> {
    match pool {
        DbPool::Sqlite(p) => sqlite_tree(p).await,
        DbPool::Postgres(p) => postgres_tree(p).await,
        DbPool::Mysql(p) => mysql_tree(p, database).await,
    }
}

async fn sqlite_tree(pool: &sqlx::SqlitePool) -> DbResult<SchemaTree> {
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
                    kind: None,
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

async fn postgres_tree(pool: &sqlx::PgPool) -> DbResult<SchemaTree> {
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
                "SELECT column_name, data_type, is_nullable, column_default, udt_name
                 FROM information_schema.columns
                 WHERE table_schema = $1 AND table_name = $2
                 ORDER BY ordinal_position",
            )
            .bind(&sname)
            .bind(&tname)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            let enum_type_names: Vec<String> = cols
                .iter()
                .filter_map(|c| {
                    let data_type: String = c.try_get(1).unwrap_or_default();
                    if data_type.eq_ignore_ascii_case("USER-DEFINED") {
                        c.try_get::<String, _>(4).ok()
                    } else {
                        None
                    }
                })
                .collect();
            let enum_variants = postgres_enum_variants(pool, &enum_type_names)
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
                    let udt_name: String = c.try_get(4).unwrap_or_default();
                    let kind = enum_variants
                        .get(&udt_name)
                        .map(|variants| ColumnKind::Enum {
                            variants: variants.clone(),
                        });
                    Column {
                        pk: pk_set.contains(&cname),
                        name: cname,
                        r#type: ctype,
                        nullable: nul.eq_ignore_ascii_case("YES"),
                        default: dflt,
                        kind,
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

async fn mysql_tree(pool: &sqlx::MySqlPool, database: Option<&str>) -> DbResult<SchemaTree> {
    let db = match database {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => {
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
                let kind = if ctype.to_ascii_lowercase().starts_with("enum(") {
                    Some(ColumnKind::Enum {
                        variants: parse_enum_column_type(&ctype),
                    })
                } else {
                    None
                };
                Column {
                    name: cname,
                    r#type: ctype,
                    nullable: nul.eq_ignore_ascii_case("YES"),
                    pk: key.eq_ignore_ascii_case("PRI"),
                    default: dflt,
                    kind,
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
        schemas: vec![SchemaTreeSchema { name: db, tables }],
    })
}

async fn postgres_enum_variants(
    pool: &sqlx::PgPool,
    enum_type_names: &[String],
) -> DbResult<BTreeMap<String, Vec<String>>> {
    if enum_type_names.is_empty() {
        return Ok(BTreeMap::new());
    }
    let rows = sqlx::query(
        "SELECT t.typname, e.enumlabel
         FROM pg_type t
         JOIN pg_enum e ON e.enumtypid = t.oid
         WHERE t.typtype = 'e' AND t.typname = ANY($1)
         ORDER BY t.typname, e.enumsortorder",
    )
    .bind(enum_type_names)
    .fetch_all(pool)
    .await?;
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        let typname: String = row.try_get(0).unwrap_or_default();
        let label: String = row.try_get(1).unwrap_or_default();
        out.entry(typname).or_default().push(label);
    }
    Ok(out)
}

pub(crate) fn parse_enum_column_type(column_type: &str) -> Vec<String> {
    let Some(inner) = column_type
        .trim()
        .strip_prefix("enum(")
        .and_then(|s| s.strip_suffix(')'))
    else {
        return Vec::new();
    };

    let mut variants = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' if in_quote && chars.peek() == Some(&'\'') => {
                current.push('\'');
                chars.next();
            }
            '\'' => in_quote = !in_quote,
            ',' if !in_quote => {
                variants.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() || inner.ends_with("''") {
        variants.push(current.trim().to_string());
    }
    variants
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mysql_enum_column_type() {
        assert_eq!(
            parse_enum_column_type("enum('a','b','c')"),
            vec!["a", "b", "c"]
        );
        assert_eq!(parse_enum_column_type("enum('a,b','c')"), vec!["a,b", "c"]);
        assert_eq!(
            parse_enum_column_type("enum('it''s','ok')"),
            vec!["it's", "ok"]
        );
        assert!(parse_enum_column_type("varchar(10)").is_empty());
    }
}
