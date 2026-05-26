---
id: cross-cutting/error-model
title: Modelo de errores
shard: 09-cross-cutting
tags: [errors, thiserror, jsonrpc]
summary: Errores tipados con `thiserror`, mapeo a códigos JSON-RPC y separación log/modelo.
related: [architecture/ipc-protocol, harness-core/tool-execution]
sources: []
---

# Modelo de errores

## Capas

```rust
// harness-core
#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    #[error("thread not found: {0}")]   ThreadNotFound(ThreadId),
    #[error("tool denied by sandbox: {0}")] SandboxDenied(String),
    #[error("provider rate limited (retry in {0}s)")] RateLimited(u64),
    #[error(transparent)] Tool(#[from] ToolError),
    #[error(transparent)] Llm(#[from] LlmError),
    #[error(transparent)] Persist(#[from] PersistError),
}
```

## Mapeo a JSON-RPC

| Rust variant | Code | Message convention |
|---|---|---|
| `CoreError::ThreadNotFound` | -32001 | "Thread {id} not found" |
| `CoreError::SandboxDenied` | -32000 | "Tool denied: {reason}" |
| `CoreError::RateLimited` | -32010 | "Provider rate limit; retry in {n}s" |
| `Tool(ToolError::Timeout)` | -32011 | "Tool timeout: {tool}" |
| Otros | -32603 | "Internal error" + `data.trace_id` |

`data` incluye un `trace_id` para correlacionar con logs.

## Lo que ve el modelo
Los errores que retornan a un tool call son **destilados**:
```json
{ "error": "Permission denied writing /etc/passwd", "hint": "This path is outside the workspace sandbox." }
```
Nunca stack traces, nunca paths del harness, nunca tokens.

## Lo que ve el humano (UI)
- Toast con `message` + botón "Details".
- Details abre modal con `data.trace_id` clickeable → abre el log filtrado.

## Lo que ve el desarrollador (logs)
- Stack completo (anyhow chain).
- Variables locales relevantes (no secretos).
- Reproducer: ¿qué params + qué estado?

## Retries
- Errores marcados `transient = true` (rate limit, network) → retry automático con backoff exponencial (1, 4, 16s).
- Otros → fallar al primer intento.
- El cliente JSON-RPC ve solo la última falla con `attempts: N` en `data`.
