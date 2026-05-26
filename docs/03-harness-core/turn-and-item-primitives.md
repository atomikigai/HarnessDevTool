---
id: harness-core/turn-and-item-primitives
title: Primitivas turn e item
shard: 03-harness-core
tags: [turn, item, primitives, streaming]
summary: Unidades atómicas del protocolo y su ciclo de vida.
related: [harness-core/streaming-events, architecture/ipc-protocol, harness-core/thread-lifecycle]
sources: [foundations/openai-codex-architecture]
---

# Turn e Item

## Item — unidad atómica de I/O
Eventos:
```
item.started   { id, kind, turn, thread }
item.delta     { id, payload-delta }      // 0..N veces
item.completed { id, payload-final }
```

`kind` ∈ `{ user_message, assistant_message, tool_call, tool_result, approval_request, system_note, compaction }`.

Esto desacopla **render incremental** de **lógica de payload**: la UI puede pintar texto en streaming sin esperar al final.

## Turn — unidad de trabajo
- Inicia cuando llega un mensaje de usuario.
- Termina cuando el modelo emite mensaje final, se cancela, o aborta por límite.
- Contiene N items.

Eventos:
```
turn.started   { id, thread, user_item }
turn.cancelled { id, reason }
turn.completed { id, stats }
turn.aborted   { id, reason }
```

## Identificadores
- `thread`: UUID v7 (orden temporal natural).
- `turn`: UUID v7.
- `item`: UUID v7. Nunca cambia tras `started`.

## Ordering
- Dentro de un turn, items se ordenan por `started_at`.
- Persisten en `events.jsonl` en orden de emisión, **no** de finalización.

## Idempotencia
- Re-emitir `item.started` con mismo id es no-op (resume).
- `item.delta` lleva `seq` monotónico para detectar pérdidas.
- `item.completed` es terminal — duplicados se ignoran.

## Mapping al historial del modelo
Algunos items entran al prompt (user_message, assistant_message, tool_call, tool_result, compaction). Otros son solo para UI (system_note, approval_request). El `prompt_builder` filtra por `kind.is_prompt_relevant()`.
