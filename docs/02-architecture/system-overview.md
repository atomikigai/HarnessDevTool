---
id: architecture/system-overview
title: Vista general del sistema
shard: 02-architecture
tags: [architecture, overview, diagram]
summary: Browser ↔ harness-server (Axum) ↔ CLIs hijos (claude/codex) + storage local.
related: [architecture/layered-architecture, architecture/process-model, architecture/ipc-protocol, app-server/overview, agents/overview]
sources: []
---

# Vista general

```
                          host del usuario
┌────────────────────────────────────────────────────────────────────────┐
│                                                                        │
│  Browser  ────HTTP────►  frontend container (SvelteKit adapter-node)   │
│  Browser  ────HTTP+SSE──►  backend container (harness-server, Axum)    │
│                                                                        │
│  ┌─────────────────────┐         ┌─────────────────────────────────┐   │
│  │ SvelteKit + Tailwind│         │ harness-server (Axum)           │   │
│  │ + shadcn-svelte     │         │ ├─ routes/* (REST + SSE)        │   │
│  │ + xterm.js          │         │ ├─ harness-core (threads,tasks) │   │
│  │ + valibot           │         │ ├─ harness-session (PTY mgr)    │   │
│  │ stores reactivos    │         │ ├─ harness-mcp-server (stdio)   │   │
│  └─────────────────────┘         │ ├─ harness-sandbox              │   │
│                                  │ ├─ harness-skills    (F5)       │   │
│                                  │ ├─ module-db, module-ssh (F4)   │   │
│                                  └────┬────────────────────────────┘   │
│                                       │ spawn (PTY+stdio MCP)          │
│                                       ▼                                │
│                                  claude / codex                        │
│                                  (binarios del host, bind-mounted)     │
│                                                                        │
│  Storage local (montado /data en backend):                             │
│  ~/.harness/                                                           │
│   ├─ USER.md                                                           │
│   ├─ shared/skills/                                                    │
│   └─ profiles/<active>/{memory, skills, threads, cli-state, .git}      │
└────────────────────────────────────────────────────────────────────────┘
```

## Flujo de un request humano (end-to-end)

1. Usuario en browser escribe prompt → POST `/api/threads/:id/sessions` con `{ kind: "claude" }`.
2. `harness-server` valida, crea spawn record, lanza `claude` child con PTY + `--mcp-config` apuntando al harness-mcp-server local.
3. Browser abre SSE `/api/events?thread=:id` (multiplex de eventos del thread).
4. PTY output del `claude` se streamea: `harness-session` lee bytes → SSE → xterm.js en browser.
5. El `claude` invoca tools MCP (`task.list`, `task.claim`, `skills.search`, `memory.search`, ...) → `harness-mcp-server` responde con datos del store.
6. Eventos estructurados (task transitions, approvals, etc.) se emiten también vía SSE.
7. Al cierre del CLI hijo → `spawn.exited` event → status persistido → SSE final.

## Roles del sistema

| Rol | Software |
|---|---|
| **UI** | SvelteKit + adapter-node + xterm.js |
| **Wire** | HTTP REST + SSE |
| **Backend** | `harness-server` (Axum, Rust) |
| **State** | `~/.harness/` (filesystem + SQLite FTS5 + git por profile) |
| **Agentes** | `claude` / `codex` CLIs del usuario (spawn como children) |
| **Bridge** | `harness-mcp-server` (stdio MCP, expuesto al CLI hijo) |

## Ejes de extensión

- **Nuevo agente** (rol/dominio nuevo): nuevo shard en [[agents/overview]] + plantilla TOML.
- **Nuevo módulo vertical** (DB/SSH/etc): crate `module-X` + routes en `harness-server` + tools MCP. Ver [[recipes/bootstrap-new-tool]].
- **Nuevo MCP externo** (context7, playwright): config en `~/.harness/config.toml`. Ver [[recipes/add-mcp-server]].
- **Nueva ruta UI**: SvelteKit route + cliente RPC. Ver [[recipes/add-frontend-route]].

## Lo que NO está

- ❌ Llamadas directas a APIs de Anthropic/OpenAI (el CLI las hace).
- ❌ Tauri / Electron.
- ❌ Pool de agentes vivos (efímeros: 1 spawn por task).
- ❌ JSON-RPC stdio entre browser y backend (HTTP+SSE; JSON-RPC sí entre backend y CLI hijo, pero ese es canal interno).
- ❌ Multi-tenancy (single-user local).
- ❌ Cloud por default (todo local; sync opt-in via git remote propio del usuario).

Ver [[build-plan/tech-stack-locked]] para el stack completo y [[build-plan/decisions-locked]] para las decisiones que cierran cada elección.
