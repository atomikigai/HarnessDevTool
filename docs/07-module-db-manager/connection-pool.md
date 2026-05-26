---
id: module-db-manager/connection-pool
title: Pool de conexiones
shard: 07-module-db-manager
tags: [pool, sqlx, lifecycle]
summary: Un pool por conexión guardada; lifecycle y settings.
related: [module-db-manager/supported-engines, module-db-manager/query-runner]
sources: []
---

# Pool

## Forma
```rust
pub enum AnyPool {
    Sqlite(sqlx::SqlitePool),
    Postgres(sqlx::PgPool),
    Mysql(sqlx::MySqlPool),
}

pub struct ConnectionRuntime {
    pub id: ConnectionId,
    pub pool: AnyPool,
    pub spec: ConnectionSpec, // url, name, options
}
```

## Settings por defecto
- `max_connections = 5`.
- `idle_timeout = 5 min`.
- `acquire_timeout = 10s`.

Configurable por conexión en su spec.

## Lifecycle
- Lazy: el pool se crea en el **primer** `query.run` o `schema.tree`.
- Cierre: si la conexión se elimina o el módulo se shut down.
- Reconnect: sqlx maneja reconexión transparente.

## Credenciales
- Spec persiste URL **sin password**.
- Password en keyring referenciado por `keyring:db-<connection-id>`.
- Al abrir pool: leer keyring, inyectar password, construir URL final en memoria.

## Test de conexión
`module.db.connection.test { spec }` → intenta `SELECT 1` (o `PING` según engine) con timeout 5s. Devuelve OK + version del server o error humano.

## Métricas
- `db.pool.acquired`, `db.pool.timeouts`, `db.pool.size` como spans `tracing`.
- UI muestra count de conexiones activas en el footer del módulo.
