---
id: build-plan/phase-3-team
title: F3 — Equipo de agentes
shard: 12-build-plan
tags: [phase, f3, team, scheduler, planner, evaluator, sandbox, budget]
summary: Planner + Generator + Evaluator coordinados por archivos terminan una spec dentro de budget.
related: [build-plan/phase-2-tasks-mcp, foundations/lessons-learned, foundations/anthropic-principles, harness-core/sandbox]
sources: [foundations/anthropic-principles]
---

# F3 — Equipo

## Meta
Lanzar una **request humana de alto nivel** ("construye una app TODO", "agrega paginación a /orders") y que un equipo de agentes (planner + N generators + evaluator) coordinándose por archivos la complete dentro de un budget. Los agentes son instancias de `claude`/`codex` con prompt-templates distintos según rol.

## Entregables

### Backend — scheduler
- [ ] `harness-core::scheduler`:
  - [ ] Loop con tick 2s.
  - [ ] Recoge tasks `queued` con deps `done` → asigna a un generator idle.
  - [ ] Recoge tasks `pending_verify` → asigna a evaluator idle.
  - [ ] Affinity: prefiere asignar tasks del mismo archivo al mismo agente reciente.
  - [ ] Concurrency cap: `thread.budget.max_concurrent_workers` (default 3).
  - [ ] Cooldown tras `verify-fail`: no re-asignar misma task al mismo generator inmediatamente.
- [ ] Logging del scheduler como spans `tracing`: `scheduling.tick { queued, in_progress, pending_verify, idle_agents }`.

### Backend — roles
- [ ] Plantillas en `~/.harness/profiles/<p>/roles/{planner,generator,evaluator}.toml`:
  ```toml
  name = "planner"
  cli = "claude"               # | codex
  prompt_template = "..."      # se inyecta al spawn como mensaje inicial
  enabled_tools = ["task.*", "spec.*"]   # whitelist sobre MCP
  disabled_tools = []
  ```
- [ ] Al spawn de una sesión con rol, el `harness-session::Manager` envía el prompt-template como primer input.
- [ ] Cuando un rol llama a una tool MCP no permitida → respuesta `denied_by_role` (no error duro, mensaje claro al modelo).

### Backend — budget
- [ ] Crate `harness-core::budget`:
  - [ ] Schema `budget.v1.json`.
  - [ ] Tracking en RAM y persistencia a `~/.harness/.../budget.toml` al cierre de cada turn (no diferido).
  - [ ] Soft cap (80%) → notification `budget.warning`.
  - [ ] Hard cap → pausa todas las tasks `in_progress`, marca `paused` con `why_paused="budget cap: usd"`.
- [ ] Endpoint `POST /api/threads/:id/budget` para subir caps.

### Backend — kill-switch
- [ ] `POST /api/pause-all` y `POST /api/resume-all`.
- [ ] Persistente entre reboots.
- [ ] UI atajo `Cmd/Ctrl+Shift+.`.

### Backend — sandbox
- [ ] Crate **`harness-sandbox`**:
  - [ ] Niveles: `none | workspace | workspace-net | strict`.
  - [ ] Linux: `seccompiler` + bind mounts.
  - [ ] macOS: `Command` con `sandbox-exec` profile.
  - [ ] Windows: stub (warning) en F3; implementación real en F6.
- [ ] Toda invocación de tool del módulo (DB/SSH en F4) y todo `shell.exec` del CLI pasa por el sandbox.
- [ ] Importante: el `claude`/`codex` child **ya tiene su propio sandbox/approval**. Aquí sandbox-eamos los **child-of-child** que ellos ejecuten.

### Backend — file-based coordination
- [ ] `spec.md` por thread: planner lo crea/mantiene; resto lo lee.
- [ ] `artifacts/` por task: workers escriben aquí (mounted al sandbox).
- [ ] Eventos `spec.changed`, `artifact.added` → SSE → UI.

### Frontend
- [ ] Dashboard del thread muestra:
  - [ ] `<TaskGraph>` en vivo (estados con colores).
  - [ ] `<BudgetMeter>` con barra de progreso + soft/hard caps.
  - [ ] Panel "Live cost" desglosado por agente.
  - [ ] Lista de sesiones activas con su rol.
- [ ] `<SpecViewer>` lateral muestra `spec.md` con highlight de secciones referenciadas por tasks.
- [ ] Botón "Pause thread" / "Resume thread".

## Test de aceptación — el "TODO app" challenge

1. Crear thread con prompt humano: "Build a TODO app: SvelteKit + SQLite + REST API. Deploy script for Vercel.".
2. Spawn planner → genera `spec.md` (~10 secciones) + ~12 tasks con `blocked_by` poblado.
3. Scheduler asigna primeras tasks (`init repo`, `data model`) a generators libres en paralelo.
4. Cada generator implementa, llena `artifacts.files`, transita a `pending_verify`.
5. Evaluator toma `pending_verify`, corre tests (vía sandbox shell), marca `verified=true` o devuelve a `in_progress` con feedback.
6. Unblock automático: al `done T-002`, su dependiente `T-005` pasa de `blocked` a `queued`.
7. Budget hard cap dispara antes de terminar (escenario forzado) → todas las tasks `in_progress` pasan a `paused` automáticamente.
8. Tras subir cap por UI → resume → terminar.
9. Reporte final del planner: tasks `done` count, costo, wallclock.

## Lo que NO está en F3
- Tools de módulos DB/SSH (F4).
- Skills auto-creadas (F5).
- GEPA (F6).
- Multi-thread coordinado entre threads (fuera de scope).

## Riesgos
- **Prompts de los roles**: son la pieza más sensible. Pueden requerir iteración. Tener un set de prompts baseline + evaluar contra tasks-target reproducibles desde F3.
- **Sandboxing del shell.exec del CLI**: si `claude` corre dentro del container y a su vez ejecuta `npm install`, el sandbox del harness debe envolver eso. Verificar early.
- **Costos en pruebas**: cada eval del "TODO app" gasta dinero del usuario. Recomendar caps bajos (~$5 USD) durante desarrollo.
- **Deadlocks de tasks**: dos workers reclamando deps mutuamente. Mitigación: el scheduler detecta ciclos y emite warning.

## Decisiones a confirmar
- ¿Roles cargados desde **archivos por perfil** (flexible, el usuario edita) o **builtin en código** (consistente)? Recomiendo archivos con plantillas builtin pre-pobladas en primera ejecución.
- ¿Default `max_concurrent_workers`? **3** (validable).
- ¿Default budget? **`usd_max=10`, `wallclock_max_s=3600`** para development; documentar cómo subirlo.
