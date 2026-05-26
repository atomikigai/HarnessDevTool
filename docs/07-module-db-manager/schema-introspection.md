---
id: module-db-manager/schema-introspection
title: Introspección de schema
shard: 07-module-db-manager
tags: [schema, introspection, metadata]
summary: Árbol jerárquico de catálogo / schema / objeto / columna por engine.
related: [module-db-manager/overview, module-db-manager/sveltekit-views]
sources: []
---

# Schema introspection

## Modelo unificado

```rust
pub struct SchemaTree {
    pub catalogs: Vec<Catalog>,
}
pub struct Catalog { pub name: String, pub schemas: Vec<Schema> }
pub struct Schema  { pub name: String, pub tables: Vec<Table>, pub views: Vec<View>, pub functions: Vec<Func> }
pub struct Table   { pub name: String, pub columns: Vec<Column>, pub pk: Vec<String>, pub indexes: Vec<Index>, pub fks: Vec<FK> }
pub struct Column  { pub name: String, pub data_type: String, pub nullable: bool, pub default: Option<String> }
```

## Fuente por engine

| Engine | Fuente |
|---|---|
| SQLite | `sqlite_master`, `pragma table_info`, `pragma foreign_key_list`, `pragma index_list` |
| Postgres | `information_schema.tables/columns/...` + `pg_catalog.*` para tipos custom |
| MySQL | `information_schema.tables/columns/...` |

## Caching
- Tree completo se construye on-demand (al abrir el panel).
- TTL 5 minutos en memoria.
- Botón "Refresh" invalida.

## Lazy loading
Para DBs grandes, expandir un nodo `Table` carga columnas e índices on-click (no upfront).

## Búsqueda
`module.db.schema.search { connection, query }` → coincidencias por nombre de tabla/columna con fuzzy match. Útil para que el agente encuentre dónde vive un dato.

## Para el agente
`db.schema(connection, scope?)` con `scope` opcional ("public.orders.*") para no devolver todo el schema y gastar tokens. Default: top-level (schemas + nombres de tablas).
