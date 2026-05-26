---
id: architecture/ipc-protocol
title: Protocolos de comunicación
shard: 02-architecture
tags: [ipc, http, sse, mcp, protocol]
summary: HTTP REST + SSE entre browser y backend; MCP stdio JSONL entre backend y CLI hijos.
related: [architecture/system-overview, app-server/overview, agents/spawn-lifecycle]
sources: []
---

# Protocolos de comunicación

> Dos canales distintos: **browser ↔ backend** (HTTP+SSE) y **backend ↔ CLI hijo** (MCP JSONL stdio). No confundir.

## Canal 1 — Browser ↔ harness-server

### Transport
- **HTTP REST** para CRUD bajo `/api/*`.
- **SSE** para streaming bajo `/api/events`.
- JSON body para requests/responses.
- **CORS** habilitado en backend para `http://localhost:8080` (frontend) y según config para otros orígenes (LAN).
- Encoding UTF-8 estricto.

### Mensajes REST
Formas tipadas con `ts-rs` (Rust source-of-truth). Ejemplo:
```jsonc
// POST /api/threads/:id/tasks  body
{ "title": "Paginación en /orders", "domain": "frontend", "spawn_hint": { ... }, "contract_declared": { ... } }

// response 201
{ "id": "T-0042", "status": "queued", "created_at": "..." }
```

Errores:
```jsonc
// 4xx/5xx body
{ "error": { "code": "task.touches_conflict", "message": "...", "data": { ... } } }
```

### SSE format

```
event: item
data: {"thread":"...","turn":"...","kind":"pty.output","seq":42,"data":"..."}

event: task
data: {"task_id":"T-0042","prev_status":"queued","next_status":"in_progress"}

event: approval
data: {"id":"req-123","tool":"memory.note","args":{...}}

event: ping
data: {"at":"2026-05-26T19:30:00Z"}
```

Cada evento lleva un `event:` (tipo) y `data:` JSON. El cliente filtra por tipo.

### Versionado
- Header `X-Protocol-Version: 1.0` en todas las requests/responses.
- Endpoint `/api/capabilities` enumera features soportadas.

Ver [[app-server/backward-compat]].

### Reconexión SSE
- El cliente reintenta con backoff exponencial.
- Header `Last-Event-ID` permite resume desde último evento procesado.
- Backend mantiene buffer de últimos N eventos por thread (default 1000).

## Canal 2 — harness-server ↔ CLI hijo (claude/codex)

### Transport
- **JSONL sobre stdio** del CLI hijo (cada línea = un mensaje JSON-RPC 2.0).
- Iniciado pasando `--mcp-config <ruta>` al CLI al spawn.
- El config apunta a `harness-mcp-server` (sub-proceso del backend o instancia in-process).

### Mensajes (MCP spec)
```jsonc
// CLI → harness-mcp-server (tool call)
{ "jsonrpc": "2.0", "id": "1", "method": "tools/call",
  "params": { "name": "task.claim", "arguments": { "task_id": "T-0042", "agent_id": "...", "ttl_s": 300 } } }

// harness-mcp-server → CLI (response)
{ "jsonrpc": "2.0", "id": "1", "result": { "ok": true, "lease_until": "..." } }

// errores
{ "jsonrpc": "2.0", "id": "1",
  "error": { "code": -32000, "message": "Task already claimed", "data": { "current_holder": "..." } } }
```

### Tools expuestas
Catalogadas en [[agents/rust-rails]]:
- `task.*` — claim/update/release
- `spec.*` — read/append
- `memory.*` — search/note/continuity
- `skills.*` — search/manage (F5+)
- `agents.*` — list/describe
- `capability.*` — request/list_loaded
- `repo.*` — scan/read_file
- `budget.*` — remaining
- `contracts.*` — validate/diff

### Vida del canal
- Se abre al spawn del CLI.
- Se cierra cuando el CLI termina (graceful o crash).
- Backend detecta cierre del pipe → marca spawn `finished`.

## Por qué dos canales distintos

| | Canal 1 (HTTP+SSE) | Canal 2 (MCP stdio) |
|---|---|---|
| Quién habla | Browser | CLI hijo (claude/codex) |
| Estándar | Web nativo | MCP spec |
| Multiplex | SSE multiplexa por thread | 1 canal por CLI hijo |
| Auth | CORS + (futuro) cookie | Confianza implícita (es child del backend) |
| Cancelación | DELETE / abort | SIGINT del child |
| Latencia | ~5–15ms | ~1ms (in-process) |

Mezclar los dos crearía acoplamiento innecesario y duplicación de schemas.

## Anti-patrones

| Mal | Bien |
|---|---|
| Browser habla MCP al backend | HTTP+SSE; MCP es solo para CLI hijos |
| WebSockets en vez de SSE | SSE es suficiente y más simple; WS solo si bidireccional fuerte |
| JSON-RPC también entre browser y backend | HTTP REST tipado con ts-rs |
| Tools del MCP visibles al frontend | Frontend usa endpoints REST; tools son del CLI |
| Buffering grande en proxy SSE | `proxy_buffering off`, `X-Accel-Buffering: no` |
