---
id: module-db-manager/overview
title: DB Manager — overview
shard: 07-module-db-manager
tags: [module, db, sqlx]
summary: Gestor lite tipo DBeaver para SQLite/Postgres/MySQL con vista en SvelteKit.
related: [module-db-manager/supported-engines, module-db-manager/sveltekit-views]
sources: []
---

# Módulo DB Manager

## Alcance v1
- Conexiones guardadas (cifradas en disco).
- Árbol de schema: catálogo → schema → tabla/vista → columnas/índices.
- Editor SQL (CodeMirror) con autocomplete básico.
- Query runner con paginación, cancelación, export (CSV/JSON).
- Browser de tabla virtualizado (filas grandes sin congelar UI).
- Tools expuestas al harness-core: `db.query`, `db.schema`, `db.explain`.

## Fuera de scope v1
- Migraciones / DDL desde UI (solo SELECT y DDL crudo si usuario quiere).
- Diagramas ER.
- Sync entre DBs.

## Stack Rust
- `sqlx` (compile-time queries opcional; runtime aquí).
- Drivers: `sqlx` features `sqlite`, `postgres`, `mysql`.
- `rust_decimal` para tipos numéricos exactos.
- `csv` para export.

## Diagrama
```
SvelteKit /db
   │
   ▼ JSON-RPC module.db.*
harness-app-server
   │
   ▼ in-process
module-db
   │ sqlx::Pool
   ▼
SQLite | Postgres | MySQL
```

## API JSON-RPC (resumen)

```
module.db.connection.list / add / remove / test
module.db.schema.tree { connection_id } → árbol jerárquico
module.db.query.run { connection_id, sql, params?, page_size?, page? } → cursor + rows
module.db.query.cancel { query_id }
module.db.export { query_id, format, path }
```

## Tools para el agente
- `db.query(connection, sql)` — sandboxed por permisos del usuario en la DB (no del harness).
- `db.schema(connection, scope?)` — devuelve schema serializado.
- `db.explain(connection, sql)` — EXPLAIN para debug de queries.

Estas tools permiten que un thread del harness analice una DB, sugiera índices, escriba migraciones.
