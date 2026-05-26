---
id: harness-core/turn-and-item-primitives
title: Items y eventos del thread
shard: 03-harness-core
tags: [items, events, eventlog, streaming]
summary: Items son la unidad atómica del event log; el "turn" del modelo es opaco al harness.
related: [architecture/state-persistence, harness-core/streaming-events, agents/spawn-lifecycle]
sources: []
---

# Items (event log)

> Cambio respecto al modelo original: **no manejamos "turns" del LLM** (eso vive en el CLI hijo). Manejamos **items** en el event log del thread, que son la unidad atómica de cambio observable.

## Item — unidad del event log

Cada línea de `events.jsonl` es un **item**. Forma:

```jsonc
{
  "id": "01HX8E1RAA...",
  "ts": "2026-05-26T11:20:00.123Z",
  "kind": "task.transitioned",
  "thread": "<thread-uuid>",
  "spawn": "<spawn-uuid?>",          // si aplica
  "task": "<task-id?>",              // si aplica
  "payload": { ... },                // depende del kind
  "actor": "user | agent:<id> | orchestrator | scheduler | harness"
}
```

## Kinds de item

### Sistema
- `thread.created`, `thread.archived`, `thread.resumed`, `thread.paused`
- `system.init` (snapshot al arrancar)
- `config.changed`

### Spawn (CLI hijo)
- `spawn.launching`
- `spawn.running`
- `spawn.output` (PTY bytes; `payload.data_b64`)
- `spawn.exited` (`payload.code`, `payload.signal?`)
- `spawn.cancelled`

### Task
- `task.created`
- `task.claimed` (con `lease_until`)
- `task.lease_renewed`
- `task.lease_expired`
- `task.transitioned` (con `from`, `to`)
- `task.submitted` (con `contract_real` resumen)
- `task.verified` (con `verified_by`)

### Memoria/Skills
- `memory.created`, `memory.updated`, `memory.transitioned`
- `skill.proposed`, `skill.promoted`, `skill.patched`, `skill.archived`

### Aprobaciones
- `approval.requested` (`payload.tool`, `payload.args_preview`)
- `approval.decided` (`payload.decision`, `payload.by`)

### Costos
- `budget.consumed` (deltas)
- `budget.cap_warning` (soft)
- `budget.cap_reached` (hard, pause)

## Garantías

- **Append-only**: nunca se sobreescribe.
- **Ordenado por timestamp + secuencia**.
- **Inmutable**: si se equivoca, **se añade un item correctivo**, no se edita el viejo.
- **Cada item tiene un id estable** (UUID v7).
- **Persistencia atómica**: append + fsync periódico (no por línea; trade-off perf/safety).

## Streaming

Los items se publican al SSE hub al persistirse. Frontend filtra por tipo y por thread relevante. Ver [[harness-core/streaming-events]].

## Resume

Replay del `events.jsonl` reconstruye el estado de UI exactamente. La UI puede usar `Last-Event-ID` para resume parcial; el backend mantiene buffer de últimos N items por thread.

## ¿Y los "turns" del LLM?

El CLI hijo (claude/codex) opera en **turns** internos al modelo (prompt → respuesta → tool calls → ...). Eso es **opaco al harness**. Lo que vemos:
- `spawn.output` bytes que vienen del PTY.
- `task.claimed`/`task.submitted` cuando el CLI llama tools MCP nuestras.
- `spawn.exited` al final.

Si quieres ver "qué hizo el LLM exactamente turn por turn", lo encuentras en `spawns/<sid>/output.log` (PTY raw con su renderizado markdown). El event log es **a nivel de harness**, no de modelo.

## Cómo se usa para UI

| UI panel | Items consumidos |
|---|---|
| Terminal xterm.js | `spawn.output` filtrados por spawn_id |
| Task list | `task.created/transitioned/submitted/verified` |
| Live cost | `budget.consumed` (agregado) |
| Approvals inbox | `approval.requested` (pending) |
| Activity feed | todos en orden temporal |

## Tamaño y rotación

- `events.jsonl` crece append-only.
- Al pasar **50 MiB**, rota: `events-<ts>.jsonl.zst` (comprimido), nuevo `events.jsonl` vacío.
- Rotated files se leen al reconstruir history completo (mergeados por timestamp).
- Threads archivados pueden tener varias rotaciones; comprimidas ocupan ~5-10x menos.

## Anti-patrones

| Mal | Bien |
|---|---|
| Editar items pasados | Añadir item correctivo |
| Borrar items | Append `tombstone` con razón |
| Inferir state de prosa libre del PTY | Usar items estructurados (`task.*`, etc.) |
| Streaming sin buffer en el servidor | Buffer + `Last-Event-ID` para resume |
| Rotar en hot path bloqueante | Rotación async background |
