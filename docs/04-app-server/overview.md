---
id: app-server/overview
title: harness-server (Axum) — overview
shard: 04-app-server
tags: [server, axum, overview]
summary: Backend Rust con Axum sirviendo HTTP+SSE al browser y MCP stdio a los CLIs hijos.
related: [build-plan/repo-layout, build-plan/tech-stack-locked, architecture/ipc-protocol, architecture/system-overview]
sources: []
---

# harness-server (Axum)

> Antes llamado "App Server" en el modelo Codex. **Renombrado a `harness-server`** con Axum como framework. El directorio `04-app-server` se mantiene por compatibilidad de IDs.

## Qué hace

Es el **único binario** del backend. Combina:
1. **HTTP REST API** bajo `/api/*` para CRUD de threads, tasks, sessions, skills, memory.
2. **SSE** bajo `/api/events` para streaming de items (PTY output, task transitions, etc.).
3. **MCP server** (sub-proceso o módulo interno) expuesto vía stdio a cada CLI hijo.
4. **Scheduler** y **session manager** in-process.
5. **Persistencia** en `/data` (montado desde `~/.harness/` del host).

## Anatomía

```
┌────────────────────────────────────────────┐
│              harness-server                │
├────────────────────────────────────────────┤
│ Axum Router  /api/health, /api/threads, ...│
│ + tower-http (cors, trace, compression)    │
├────────────────────────────────────────────┤
│ AppState (Arc): core, sessions, sse, cfg   │
├────────────────────────────────────────────┤
│ harness-core       (threads, tasks, etc.)  │
│ harness-session    (PTY manager)           │
│ harness-mcp-server (stdio bridge a CLIs)   │
│ harness-skills     (F5+)                   │
│ module-db, module-ssh (F4)                 │
└────────────────────────────────────────────┘
        │ spawn child                  │ stdio (MCP)
        ▼                              ▼
   claude / codex (PTY)         tools del harness-bridge
```

## Vida del proceso

1. **Boot**: lee `~/.harness/config.toml` + profile activo, abre `search.db`, escanea threads.
2. **Listen**: en `:7777` (configurable).
3. **Accept**: requests del frontend; abre canales SSE.
4. **Spawn**: child processes `claude`/`codex` cuando una task lo requiere.
5. **Persist**: events.jsonl, tasks/, memory/, skills/ ante cada cambio.
6. **Shutdown**: SIGTERM → grace 5s → flush logs → close PTY children → exit 0.

## Endpoints principales

| Ruta | Método | Propósito |
|---|---|---|
| `/api/health` | GET | health + versión + uptime |
| `/api/threads` | GET/POST | lista / crea |
| `/api/threads/:id` | GET/DELETE | detalle / archive |
| `/api/threads/:id/tasks` | GET/POST | tasks del thread |
| `/api/threads/:id/tasks/:tid` | PATCH | transiciones |
| `/api/threads/:id/sessions` | POST | spawn CLI |
| `/api/sessions/:sid` | DELETE | kill |
| `/api/sessions/:sid/input` | POST | bytes al PTY |
| `/api/sessions/:sid/resize` | POST | resize |
| `/api/events` | GET (SSE) | stream items |
| `/api/skills/*` | varios | F5 |
| `/api/memory/*` | varios | F5 |
| `/api/profile/*` | varios | switch, list |

Detalles del wire en [[architecture/ipc-protocol]].

## Por qué un solo binario

- Single-user local: no hay multi-tenant que justifique varios procesos.
- Simplifica deploy (Docker compose con 1 servicio backend).
- Permite compartir estado in-process (sessions table, scheduler).
- PTY children comparten parent → cleanup determinista.

## Por qué Axum (no Actix, Rocket, etc.)

- Construido sobre tokio + hyper + tower → idiomático del ecosistema actual.
- Extractors tipados (`State`, `Path`, `Json`, `Query`).
- `tower-http` cubre CORS, trace, compression, timeout sin código boilerplate.
- SSE soportado nativamente con `axum::response::sse::Sse`.
- Activo, bien mantenido.

## Modo dev vs prod

- **Dev**: `cargo run --bin harness-server` directo en host (sin Docker), accede a `~/.harness/` del usuario.
- **Prod**: corre en container distroless con `~/.harness/` montado como `/data`.

Ambos modos son compatibles con `just dev-backend` (dev) y `just docker-up` (prod).
