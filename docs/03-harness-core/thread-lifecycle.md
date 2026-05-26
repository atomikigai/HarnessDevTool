---
id: harness-core/thread-lifecycle
title: Lifecycle de threads
shard: 03-harness-core
tags: [thread, lifecycle, persistence]
summary: Estados y operaciones canónicas; persistido por profile.
related: [architecture/state-persistence, harness-core/turn-and-item-primitives, memory/continuity, cross-cutting/profiles]
sources: []
---

# Thread lifecycle

## Qué es un thread

Un thread es una **conversación durable** sobre un trabajo concreto. Cada thread tiene:
- Un `working_dir` (ruta al proyecto del usuario).
- Un `spec.md` (qué se construye).
- N `tasks/*.toml` (descomposición).
- Un `events.jsonl` (event log append-only).
- N spawns durante su vida (efímeros).
- Un `budget.toml` (cap de USD/tokens/wallclock).

Threads viven dentro del profile activo: `~/.harness/profiles/<active>/threads/<uuid>/`.

## Estados

```
[creating] → [active] ⇄ [idle] ⇄ [paused] → [archived]
                ↘     ↘
                  cancelled  faulted
```

- **creating**: durante el setup (orchestrator está creando spec + tasks).
- **active**: tiene al menos un spawn ejecutando.
- **idle**: sin spawns activos pero no archivado.
- **paused**: pausado por budget cap, kill-switch o humano.
- **archived**: solo lectura.
- **cancelled**: cancelación explícita; conserva events.
- **faulted**: error irrecuperable; conserva events para diagnóstico.

## Operaciones

### `POST /api/threads { title, working_dir, model_hint? }`
- Genera UUID v7.
- Inicializa `events.jsonl` con item `system/init` (snapshot de config + AGENTS.md del working_dir si existe).
- Lanza orchestrator → estado `creating` → estado `active` cuando tasks estén creadas.

### `GET /api/threads/:id`
Detalles + lista de tasks + último snapshot.

### `POST /api/threads/:id/resume`
Reanuda un thread idle/paused.
- Lee `events.jsonl` → reconstrucción de UI cursor.
- Re-asigna lease-expired tasks a `queued`.
- Inyecta `CONTINUITY.md` slice **del thread** al prompt del orchestrator si lo invocas.

### `POST /api/threads/:id/fork { at_task? }`
- Copia el thread hasta una task específica (o el último estado).
- Crea nuevo UUID con `parent_id`.
- Útil para A/B de approaches.

### `DELETE /api/threads/:id`
- Mueve a `archived`. **No borra**.
- Conserva todos los artifacts.

## Snapshot al iniciar

El item `system/init` en `events.jsonl` guarda:
- modelo hint (qué CLI: `claude` o `codex` + version)
- sandbox level
- working_dir + git_root
- `AGENTS.md` snapshot del repo del usuario (si existe)
- versión del harness
- profile activo al momento de creación

Esto hace el thread **autodescriptivo**: un resume meses después es reproducible.

## Multi-spawn

Un thread puede tener varios spawns en paralelo (cap por `budget.max_concurrent_spawns`, default 3). Cada uno trabaja en una task diferente.

Cuando todas las tasks raíz están `done`, el thread pasa naturalmente a `idle`. El orchestrator puede emitir un reporte final.

## Persistencia en disco

```
profiles/<active>/threads/<thread-uuid>/
├── meta.json                  # working_dir, modelo, config snapshot
├── spec.md                    # mantenido por el orchestrator
├── events.jsonl               # append-only (rotación a .jsonl.zst en 50 MiB)
├── tasks/                     # TOML por task
├── artifacts/                 # outputs de tasks (files producidos)
├── budget.toml                # caps + consumed
└── spawns/                    # uno por spawn histórico (incluye output.log)
```

Ver [[architecture/state-persistence]] y [[memory/layout]] para el detalle.

## Cross-thread vistas

`CONTINUITY.md` agrega un slice de **todos los threads activos** del profile (no de threads archivados). Se regenera on-change. Ver [[memory/continuity]].

## Anti-patrones

| Mal | Bien |
|---|---|
| Borrar threads | Solo archivar |
| Mutar `events.jsonl` | Append-only físico |
| Thread sin `working_dir` | Siempre asociar a un directorio del usuario |
| Resume sin restaurar leases | Lease-expired tasks → `queued` automático |
| Threads compartidos entre profiles | Aislados por profile |
