---
id: module-db-manager/supported-engines
title: Engines soportados
shard: 07-module-db-manager
tags: [db, sqlite, postgres, mysql, sqlx]
summary: Drivers, features, peculiaridades por engine.
related: [module-db-manager/overview, module-db-manager/connection-pool]
sources: []
---

# Engines

## SQLite (default, opt-out imposible)
- `sqlx::sqlite`.
- Conexiones por path al archivo o `:memory:`.
- WAL recomendado para concurrencia lectura.
- Restricción: una sola conexión escribe al mismo tiempo.

## Postgres
- `sqlx::postgres`.
- URL `postgres://user:pass@host:port/db`.
- SSL: `?sslmode=require`. Certs en `~/.harness/modules/db/ca/`.
- Cancellation: `sqlx::postgres::PgConnection::cancel_handle()` para abortar queries en vuelo.

## MySQL
- `sqlx::mysql`.
- URL `mysql://user:pass@host:port/db`.
- TLS configurable.
- Cancellation menos elegante: `KILL QUERY <id>` desde una conexión paralela.

## Posibles adiciones v2
- DuckDB (vía `duckdb-rs`).
- ClickHouse (vía HTTP).
- SQL Server (vía `tiberius`).

## Mapping de tipos
| SQL | Rust → JSON |
|---|---|
| INTEGER / BIGINT | i64 → number (si excede JS safe → string) |
| NUMERIC / DECIMAL | string (preserva precisión) |
| TEXT / VARCHAR | string |
| BLOB / BYTEA | base64 string con marker |
| BOOL | bool |
| DATE / TIME / TIMESTAMP | ISO 8601 string |
| UUID | string |
| JSON / JSONB | objeto JSON inline |
| ARRAY | array JSON |
| NULL | null |

Conversión documentada explícitamente para que el agente no se confunda con tipos.

## Features de Cargo
```toml
module-db = { default-features = ["sqlite"], features = [] }
# habilitar adicionales:
# cargo build -p module-db --features "postgres mysql"
```
