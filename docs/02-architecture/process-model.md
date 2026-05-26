---
id: architecture/process-model
title: Modelo de procesos
shard: 02-architecture
tags: [architecture, process, tokio, child-process, docker]
summary: Frontend container + backend container + child CLIs; runtime tokio multi-thread.
related: [architecture/system-overview, app-server/web-deployment, agents/spawn-lifecycle]
sources: []
---

# Modelo de procesos

## Procesos principales

| Proceso | Container | Vida | Notas |
|---|---|---|---|
| `harness-server` (Axum) | backend | larga (mientras corra) | aloja N threads, scheduler, sessions |
| SvelteKit Node (`adapter-node`) | frontend | larga | sirve UI, no toca state |
| Spawn `claude` / `codex` | backend (child) | corta (una task) | PTY + stdio MCP al backend |
| `harness-mcp-server` (1 por spawn) | backend (child) | corta | bridge stdio expuesto al CLI hijo |
| Browser tab | host | usuario | habla HTTP+SSE con backend |

## Concurrencia dentro del backend

Runtime **Tokio multi-thread**. Tasks principales:
- 1 task root por **thread del usuario** activo (orquesta scheduler dispatches del thread).
- 1 task por **PTY reader** (lee bytes del child, empuja a SSE hub).
- 1 task por **MCP server** instancia (atiende JSON-RPC stdio del child).
- 1 task del **SSE hub** (broadcast a clientes conectados).
- 1 task del **scheduler** (tick cada 2s, asigna tasks `queued`).
- 1 task de **regenerador de CONTINUITY.md** (debounce).

## Lifecycle del backend container

1. Container boot → `harness-server` arranca.
2. Lee config, abre `search.db`, escanea threads.
3. Listen en `:7777`.
4. Accept HTTP requests + opens SSE channels.
5. Spawn child processes (`claude`/`codex` + `harness-mcp-server`) según tasks.
6. SIGTERM → grace 5s → flush logs, close PTYs (SIGINT children), exit 0.

## Bind-mounts críticos

```
host                                  container backend
─────────────────────────────────────────────────────────────────
~/.harness/                       →   /data
/usr/local/bin/claude             →   /usr/local/bin/claude:ro
/usr/local/bin/codex              →   /usr/local/bin/codex:ro
```

Los binarios `claude`/`codex` se bind-mountean del host. Su auth vive en `cli-state/` del profile activo y se expone al container vía symlink interno `/root/.claude → /data/profiles/<active>/cli-state/.claude/`.

## Aislamiento

- **Profiles**: aislados a nivel filesystem (subdirs distintos bajo `~/.harness/profiles/`).
- **Spawns**: aislados en procesos distintos; comparten storage pero su contexto vivo es separado.
- **Sandbox**: tools peligrosas corren con `harness-sandbox` envolviendo el shell (seccomp Linux, sandbox-exec macOS, AppContainer Windows en F6).

## Recovery

- **Backend crash**: container reinicia (`restart: unless-stopped`). Al re-arrancar:
  - Spawns activos: sus PTYs murieron con el parent. Marcadas `killed` con causa `harness-restart`.
  - Tasks `in_progress` con lease expirado: scheduler las pasa a `queued` tras grace.
- **Frontend crash**: usuario refresca; SSE reconnecta con `Last-Event-ID`.
- **CLI hijo crash**: spawn marcado `failed`; según retry policy (cap N) → re-spawn o re-plan.

## Por qué dos containers

| | 1 container | 2 containers (elegido) |
|---|---|---|
| Aislamiento | bajo | mejor; UI puede caer sin matar backend |
| Deploy update | uno solo | independiente FE/BE |
| Imagen tamaño total | mayor | sumatoria ~150 MB |
| Cross-talk | in-process | HTTP (sobrecarga mínima localhost) |

Decisión bloqueada por simplicidad operativa + aislamiento. Ver [[app-server/web-deployment]].

## Por qué no in-process Axum + adapter-node

- Axum es Rust, SvelteKit corre Node. No comparten runtime.
- Embeddings cross-runtime (deno_core, neon) añaden complejidad sin payoff aquí.
- Two-container es **el** patrón estándar para split BE/FE.
