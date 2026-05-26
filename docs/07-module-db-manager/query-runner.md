---
id: module-db-manager/query-runner
title: Query runner
shard: 07-module-db-manager
tags: [query, runner, pagination, cancellation]
summary: Ejecutar, paginar y cancelar queries; streaming de filas.
related: [module-db-manager/connection-pool, module-db-manager/sveltekit-views]
sources: []
---

# Query runner

## Flujo
1. `module.db.query.run { connection_id, sql, params?, page_size? }` → `{ query_id, columns, page_size }`.
2. Backend ejecuta y empieza a fetchar filas.
3. Notifications `module.db.query.rows { query_id, rows: [...], next_cursor }` hasta agotar.
4. `module.db.query.completed { query_id, total_rows?, elapsed_ms }`.

## Paginación
- `page_size` default 500.
- Cursor opaco (offset interno).
- UI puede pedir "siguiente página" o detenerse.

## Cancelación
- `module.db.query.cancel { query_id }`.
- Postgres: usa el cancel_handle.
- MySQL: abre una conexión auxiliar, ejecuta `KILL QUERY <conn_id>`.
- SQLite: setea un flag interno; el step interrumpe el `Statement`.

## Parametrización
- `params` array tipo `[String|Number|Bool|Null|{type:'datetime', value:'2026-...'}]`.
- El frontend nunca interpola SQL crudo; el agente recibe la guía de usar params.

## Errores
- SQL parse / runtime → mensaje crudo del driver + posición (cuando disponible).
- Timeout en `acquire` → error específico ("no connection available").
- Sin permisos → mensaje literal del servidor.

## Streaming a la UI
La tabla virtualizada en SvelteKit reduce sobre `rows` deltas, mantiene altura virtual. Ver [[module-db-manager/sveltekit-views]].

## Export
- `module.db.export { query_id, format: "csv"|"json"|"parquet", path }`.
- Si `query_id` ya terminó: re-ejecuta (re-fetch).
- Si está activo: aprovecha el stream actual.
- `path` es ruta absoluta dentro del workspace; sandbox aplica.

## Tool para el agente
```jsonc
{ "tool": "db.query", "args": {
    "connection": "prod-readonly",
    "sql": "SELECT count(*) FROM orders WHERE created_at > $1",
    "params": ["2026-01-01"]
}}
```
Devuelve solo las primeras N filas (configurable) para no flood al contexto del modelo.
