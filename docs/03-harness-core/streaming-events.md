---
id: harness-core/streaming-events
title: Streaming de eventos
shard: 03-harness-core
tags: [streaming, sse, events]
summary: Cómo se traduce el SSE del provider a notifications JSON-RPC.
related: [harness-core/turn-and-item-primitives, architecture/ipc-protocol]
sources: []
---

# Streaming

## De provider a UI

```
Provider SSE  →  llm-client  →  core agent loop  →  EventSink  →  app-server JSON-RPC  →  surface
```

## Tipos de chunk del provider (normalizados)

```rust
pub enum ChunkKind {
    TextDelta(String),
    ToolCallStart { id, name },
    ToolCallArgsDelta { id, delta },
    ToolCallEnd { id },
    Reasoning(String),       // si el modelo expone razonamiento
    Final { stop_reason },
}
```

## Mapeo a items

| ChunkKind | Item event |
|---|---|
| TextDelta | `item.delta { kind: assistant_message, text }` |
| ToolCallStart | `item.started { kind: tool_call, name }` |
| ToolCallArgsDelta | `item.delta { args-json delta }` |
| ToolCallEnd | `item.completed { kind: tool_call }` |
| Final | `item.completed { kind: assistant_message }` |

## EventSink
Trait abstracto. Implementaciones:
- `JsonRpcSink` — empuja a stdout del App Server.
- `InMemorySink` — para tests.
- `FanoutSink` — multiplexa a varias surfaces.

```rust
pub trait EventSink: Send + Sync {
    fn emit(&self, ev: ItemEvent);
}
```

## Backpressure
- Si la surface es lenta, el sink encola hasta `max_pending` (default 1024).
- Pasado el límite, se aplica **coalesce** de deltas adyacentes del mismo item (concatena texto) sin perder datos.
- Si sigue saturado, se descartan deltas intermedios pero `completed` siempre llega con payload final.

## Persistencia vs streaming
Cada item se persiste al `events.jsonl` en `completed` (no en deltas) → la UI ve más detalle del que se guarda. En `resume`, se replayean solo items completados.
