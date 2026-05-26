---
id: architecture/system-overview
title: Vista general del sistema
shard: 02-architecture
tags: [architecture, overview, diagram]
summary: Diagrama de bloques: surfaces ↔ App Server ↔ core ↔ módulos (Agentes, DB, SSH).
related: [architecture/layered-architecture, architecture/process-model, app-server/overview]
sources: []
---

# Vista general

```
┌─────────────────────────────────────────────────────────────────┐
│                        Surfaces (UI)                            │
│  ┌──────────────────────┐  ┌──────────────────────────────────┐ │
│  │ Desktop (Tauri)      │  │ CLI (harness-cli)                │ │
│  │ SvelteKit shell      │  │ TTY rendering                    │ │
│  └──────────┬───────────┘  └──────────────┬───────────────────┘ │
└─────────────┼──────────────────────────────┼─────────────────────┘
              │  JSON-RPC / JSONL stdio      │
┌─────────────▼──────────────────────────────▼─────────────────────┐
│                     harness-app-server                           │
│  stdio transport · message processor · thread manager            │
└─────────────────────────────┬────────────────────────────────────┘
                              │  in-process API
┌─────────────────────────────▼────────────────────────────────────┐
│                       harness-core (Rust)                        │
│  agent loop · prompt builder · cache strategy · compaction       │
│  thread store · turn/item engine · approval flow · streaming     │
└──────┬─────────────┬──────────────────┬───────────────────┬──────┘
       │             │                  │                   │
┌──────▼────┐  ┌─────▼──────┐  ┌────────▼────────┐  ┌───────▼──────┐
│ sandbox   │  │ mcp client │  │  module-agents  │  │ module-db /  │
│ (seccomp, │  │ (stdio +   │  │  (claude CLI    │  │ module-ssh   │
│  jail FS) │  │  http MCP) │  │   PTY sessions) │  │ (sqlx/russh) │
└───────────┘  └────────────┘  └─────────────────┘  └──────────────┘
```

## Flujo de un request típico
1. Usuario escribe en la UI → SvelteKit emite `thread.send` por JSON-RPC.
2. App Server traduce → core abre un **turn** en el **thread** activo.
3. Core construye prompt → llama API del modelo (stream SSE).
4. Cada chunk produce `item/delta` → App Server lo reemite → UI lo renderiza.
5. Tool call → core decide: nativa (sandbox), MCP (cliente MCP), o módulo (DB/SSH/Agentes).
6. Resultado se apendiza al prompt → loop continúa.
7. Mensaje final → `turn.completed`.

## Persistencia
- Cada `item` se escribe al event log del thread (append-only).
- Thread store (SQLite o ficheros) bajo `~/.harness/threads/`.
- Resume = leer event log + restaurar prompt.

## Ejes de extensión
- **Nuevo modelo / provider** → swap del cliente HTTP en `harness-core/llm-client`.
- **Nuevo módulo** (DB, SSH, ...) → expone tools al core; UI propia en SvelteKit. Ver [[recipes/bootstrap-new-tool]].
- **Nueva surface** → habla JSON-RPC contra el App Server.
