---
id: harness-core/streaming-events
title: Streaming de eventos (SSE)
shard: 03-harness-core
tags: [streaming, sse, events]
summary: Items del event log se publican al SSE hub; browser filtra por thread y tipo.
related: [harness-core/turn-and-item-primitives, architecture/ipc-protocol, app-server/overview]
sources: []
---

# Streaming via SSE

> Reemplazo del modelo JSON-RPC notification original. Ahora usamos **SSE** (Server-Sent Events) que es web-nativo, simple y robusto.

## De backend a UI

```
events.jsonl (write)  →  SSE Hub  →  HTTP SSE channels  →  Browser EventSource
       ▲                              (uno por cliente)        │
       │                                                       │ filtra por tipo
       │ append                                                ▼
       │                                              ┌──────────────────┐
       │                                              │ stores reactivos │
       └────  spawns / scheduler / tasks              │ Svelte           │
                                                     └──────────────────┘
```

## Hub design

```rust
pub struct SseHub {
    senders: DashMap<ClientId, Sender<ItemEvent>>,   // un sender por cliente conectado
}

impl SseHub {
    pub fn subscribe(&self, client: ClientId, thread_filter: Option<ThreadId>) -> Receiver<ItemEvent>;
    pub fn broadcast(&self, item: ItemEvent);
    pub fn unsubscribe(&self, client: ClientId);
}
```

Cada cliente browser → un `ClientId` único + opcionalmente filtro por thread.

## Formato SSE

```
event: item
data: {"id":"01HX...","ts":"...","kind":"spawn.output","thread":"...","spawn":"...","payload":{"data_b64":"..."}}

event: task
data: {"id":"01HX...","kind":"task.transitioned","task":"T-0042","from":"queued","to":"in_progress","actor":"agent:frontend-1"}

event: approval
data: {"id":"req-01HX...","tool":"memory.note","args_preview":{...}}

event: ping
data: {"at":"2026-05-26T19:30:00Z"}
```

`event:` es el tipo (filterable lado cliente). `data:` es JSON.

Cada item lleva su id (UUID v7); ordenable + idempotente.

## Backpressure

- Cada cliente tiene un canal MPSC bounded (default 1024 items).
- Si el cliente se atrasa → coalesce de `spawn.output` adyacentes (concat de `data_b64`).
- Si sigue saturado → drop de outputs intermedios (los `completed` finales **siempre llegan**).
- Otros tipos (task, approval, budget) **nunca se droppean**.

## Reconexión

- Header `Last-Event-ID: <id-último>` permite resume.
- Server lookup en buffer in-memory (últimos 1000 items por thread) → reenvía los faltantes.
- Si el item ya rotó fuera del buffer → server responde 410 con sugerencia de full reload.

## Heartbeat

Server envía `event: ping` cada 30s. Si el cliente no recibe nada en 90s → reconecta.

## Cliente Svelte

```ts
// $lib/api/sse.ts
import { writable } from "svelte/store";

export function subscribeToThread(threadId: string) {
  const events = writable<ItemEvent[]>([]);
  const es = new EventSource(`/api/events?thread=${threadId}`);

  es.addEventListener("item", (e) => {
    const item: ItemEvent = JSON.parse(e.data);
    events.update(arr => [...arr, item].slice(-500));   // keep last 500
  });
  es.addEventListener("task", (e) => { /* update tasks store */ });
  es.addEventListener("approval", (e) => { /* update approvals inbox */ });

  return { events, close: () => es.close() };
}
```

Stores reactivos derivados (`derived(events, ...)`) dan vistas filtradas por kind o spawn.

## Tipos de events vs Items del log

Items se persisten **siempre** en `events.jsonl`. SSE solo los **publica**. Esto significa:
- Si el browser no está conectado, los items se persisten igual.
- Al reconectar, replay desde último ID.
- Frontend nunca pierde nada relevante (salvo `spawn.output` muy viejos rotados).

## Anti-patrones

| Mal | Bien |
|---|---|
| WebSockets cuando SSE basta | SSE es más simple, mismo poder unidireccional |
| Sin reconexión exponencial | Backoff con jitter, max 30s |
| Sin filtros lado servidor | Filtrar por thread reduce ancho banda |
| Buffer infinito en cliente | Slice a últimos N (500) + paginación API si quieres más |
| Sin heartbeat | Conexiones zombie indetectables |
