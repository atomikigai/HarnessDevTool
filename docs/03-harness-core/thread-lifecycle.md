---
id: harness-core/thread-lifecycle
title: Lifecycle de threads
shard: 03-harness-core
tags: [thread, lifecycle, persistence]
summary: Estados y operaciones: create, resume, fork, archive.
related: [architecture/state-persistence, harness-core/turn-and-item-primitives]
sources: [foundations/openai-codex-architecture]
---

# Thread lifecycle

## Estados
```
[creating] → [active] ⇄ [idle] → [archived]
                ↘    ↘
                 cancelled  faulted
```

- **active**: tiene un turn en ejecución.
- **idle**: sin turn activo, pero abierto en alguna surface.
- **archived**: cerrado, solo lectura.
- **faulted**: error irrecuperable; conserva events para diagnóstico.

## Operaciones

### `thread.create { title?, model, sandbox, project_root }`
- Genera UUID.
- Inicializa `events.jsonl` con un item `system/init` (snapshot de config).
- Devuelve `thread_id`.

### `thread.resume { id }`
- Lee `events.jsonl` → replay para reconstruir prompt e historial.
- Emite a la UI todos los items desde un `cursor` opcional.
- Marca activo.

### `thread.fork { from, at_turn }`
- Copia eventos hasta el `turn.completed` indicado.
- Crea nuevo thread con `parent_id`.
- Útil para A/B de prompts sin perder el original.

### `thread.archive { id }`
- Cancela turn activo si lo hay.
- Marca `archived = 1` en index.
- Conserva archivos.

## Snapshot al iniciar
El item `system/init` guarda:
- modelo + provider
- sandbox config
- AGENTS.md cargado desde git root (texto íntegro)
- working directory
- versión del harness

Esto hace el thread **autodescriptivo**: un resume meses después es reproducible.

## Multi-surface
Un thread puede tener **varias surfaces conectadas** (CLI mirando, UI editando). El App Server reemite cada item a todas.
