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

## Estado actual del slice F3

Audit rápido del 2026-05-27:
- Scheduler base ya existe en `harness-core::scheduler`: tick 2s, auto-unblock, asignación `queued → generator`, ruteo `pending_verify → evaluator`, cooldown tras verify-fail, cap global de concurrencia y spans `scheduling.*`.
- Observación 2026-06-04: el scheduler ya lee `budget.max_concurrent_workers` por thread y aplica affinity básica por archivo usando artifacts recientes.
- Budget base ya existe: `harness-core::budget`, reporters Claude/Codex-stub, endpoint `/api/threads/:id/budget`, SSE `budget.warning` y hard-cap que activa `pause-all`.
- Kill-switch base ya existe: `/api/pause-all`, `/api/resume-all`, store frontend y control en `TopBar`.
- Observación 2026-06-04: el kill-switch también responde a `Cmd/Ctrl+Shift+.` desde `TopBar`.
- Observación 2026-06-04: readiness ahora cubre deps instaladas, puertos auxiliares configurados, budget por thread y señales de recursos externos.
- Observación 2026-06-04: `repo_write_file` ya es path-gated por `Task.write_paths` / `forbidden_paths` y requiere `task_id` + `scope task:<id>` confiables del MCP spawn.
- Zeus/sub-agentes está en estado puente: `kind=zeus` corre sobre Codex y el MCP expone `session_spawn_child/list/send_input/cancel/read_child_summary` más mailbox auditable.
- Observación 2026-06-04: el panel Agents consume sesiones hijas reales y Zeus usa Codex como CLI principal.
- Observación 2026-06-04: el spawner del scheduler ya respeta `Role.cli` como fuente de verdad para elegir Claude/Codex; roles `generic` conservan el kind pedido por el scheduler.
- Pendiente para cerrar F3 completo: roles por perfil con autorización fuerte, sandbox propio del harness, UI de spec/live cost completa y test de aceptación "TODO app" end-to-end.
- Ajuste de coherencia: antes de endurecer roles/capabilities, implementar [[agents/autonomy-protocol]] para readiness check, execution modes, autonomy profiles y handoffs. Sin esto el equipo puede planificar de mas en tareas cortas o bloquearse tarde por credenciales/env faltantes.

### Backend — scheduler
- [x] `harness-core::scheduler`:
  - [x] Loop con tick 2s.
  - [x] Recoge tasks `queued` con deps `done` → asigna a un generator idle.
  - [x] Recoge tasks `pending_verify` → asigna a evaluator idle.
  - [x] Affinity: prefiere asignar tasks del mismo archivo al mismo agente reciente.
  - [x] Concurrency cap global `max_concurrent` (default 3) con override `thread.budget.max_concurrent_workers`.
  - [x] Cooldown tras `verify-fail`: no re-asignar misma task al mismo generator inmediatamente.
- [x] Logging del scheduler como spans `tracing`: `scheduling.tick { queued, in_progress, pending_verify, idle_agents }`.

### Backend — autonomia y readiness
- [x] `ReadinessReport` por thread con checks iniciales repo/commands/cli_auth/env.
- [x] `execution_mode`: `quick | standard | project | exploratory | blocked`.
- [x] `autonomy_profile`: `manual | assisted | autonomous | ci`.
- [x] Mapping inicial de autonomy profile a approval behavior (`autonomous`/`ci` auto-allow).
- [x] Eventos append-only `thread.readiness.checked`, `thread.autonomy.changed`, `handoff.created`.
- [x] Schema `handoff.v1.json` y persistencia append-only de handoffs por task.
- [x] Checks profundos: deps, ports, budget, external resources.
- [x] Enforcement obligatorio `generator -> evaluator` antes de `pending_verify`.

### Backend — Zeus orchestrator (work item)
- [x] Implementar el routing rol → CLI base según `~/.harness/profiles/<p>/roles/*.toml`: `Role.cli` fuerza Claude/Codex y `generic` conserva el kind pedido por el scheduler.
- [ ] Routing especial frontend visual: tasks de pantallas, CSS, layout,
  responsive, shadcn/polish y a11y visual se delegan a Cursor primero; frontend
  logic/API/stores usa Codex/Claude.
- [x] Selector con fallback a Claude cuando falta el binario del CLI primario (`reason: binary_missing`).
- [ ] Selector con fallback a Claude para `quota_exceeded` / `runtime_error` clasificados por CLI.
- [x] Audit log append-only para fallback `binary_missing` (`scheduler.spawn.fallback`).
- [ ] Audit log para fallbacks `quota_exceeded` / `runtime_error`.
- [x] `POST /api/threads/:tid/sessions { kind: "zeus" }` deja de devolver 400 BadRequest y resuelve a Codex como CLI principal.
- [ ] UI: tab carrusel principal "Zeus session" + sub-tabs por hija con `parent_session_id`.
- [ ] Test de aceptación: dado un goal sintético, verificar que cada rol se delega al CLI esperado y que el fallback dispara cuando el primario está bloqueado.

### Backend — authorization (Q9 follow-ups)
- [x] **Capability policy loader**: el dispatcher del `harness-mcp-server` lee `~/.harness/profiles/<p>/policy.toml` al boot. Online delega al server; offline aplica reglas explícitas locales y falla cerrado si la policy está corrupta para tools sensibles.
- [x] **`check_capability(caller, tool, resource, scope)` base**: middleware MCP `check_tool_policy` envuelve cada handler de tool. Devuelve deny recuperable al modelo como tool result `isError`.
- [x] **Nueva tool MCP `task.propose`**: workers no pueden `task.create`; en su lugar encolan propuestas que el planner convierte (o no) en tasks reales.
- [x] **`spec.set_section` con version check**: exige `spec_version_required` que matchee la versión actual; rechazo si stale.
- [x] **`repo.write` path-gated**: la task lleva `write_paths` / `forbidden_paths`; el bridge rechaza writes fuera del allowlist aunque el rol tenga la capability.
- [x] **Audit log base**: sink append-only en `$HARNESS_HOME/.runtime/audit/bridge.jsonl`. Una entrada por cada decisión `allow`/`deny`/`ask` resuelta por `/api/approvals/check`, con `actor_id`, `actor_role`, `tool`, `resource`, `decision`, `reason`, `input_hash`, `result_hash`.
- [x] **Audit rotation**: rotación zstd del bridge audit cuando `bridge.jsonl` crece.
- [ ] **Tests de invariantes**: cobertura bridge parcial para invariantes ya implementables (`task_create`, `task_propose`, planner no-claim, worker no-spec, evaluator deny sensitive, repo_write path-gated, policy local). Pendiente completar cuando existan `memory.*`, `assigned_to/allowed_roles` y QA-only claim.

### Backend — roles
- [x] Plantillas en `~/.harness/profiles/<p>/roles/{planner,generator,evaluator}.toml`:
  ```toml
  name = "planner"
  cli = "claude"               # | codex
  prompt_template = "..."      # se inyecta al spawn como mensaje inicial
  enabled_tools = ["task.*", "spec.*"]   # metadata; enforcement real vive en policy.toml / capability defaults
  disabled_tools = []
  ```
- [x] Al spawn de una sesión con rol, el `harness-session::Manager` envía el prompt-template como primer input.
- [x] Cuando un rol llama a una tool MCP no permitida → respuesta `denied_by_role` (tool result `isError`, no error duro JSON-RPC).

### Backend — budget
- [x] Crate `harness-core::budget`:
  - [x] Schema `budget.v1.json`.
  - [x] Tracking en RAM y persistencia a `~/.harness/.../budget.toml` desde el budget pass.
  - [x] `max_concurrent_workers` opcional por thread usado por el scheduler.
  - [x] Soft cap → notification `budget.warning` (bandas 75/90/100).
  - [x] Hard cap → pausa todas las tasks `in_progress`, marca `paused` con `why_paused="budget cap: usd"`.
  - [x] Hard cap activa `pause-all` global.
- [x] Endpoint `POST /api/threads/:id/budget` para subir caps.

### Backend — kill-switch
- [x] `POST /api/pause-all` y `POST /api/resume-all`.
- [x] Persistente entre reboots.
- [x] UI atajo `Cmd/Ctrl+Shift+.`.

### Backend — sandbox
- [ ] Crate **`harness-sandbox`**:
  - [ ] Niveles: `none | workspace | workspace-net | strict`.
  - [ ] Linux: `seccompiler` + bind mounts.
  - [ ] macOS: `Command` con `sandbox-exec` profile.
  - [ ] Windows: stub (warning) en F3; implementación real en F6.
- [ ] Toda invocación de tool del módulo (DB/SSH en F4) y todo `shell.exec` del CLI pasa por el sandbox.
- [ ] Importante: el `claude`/`codex` child **ya tiene su propio sandbox/approval**. Aquí sandbox-eamos los **child-of-child** que ellos ejecuten.

### Backend — file-based coordination
- [x] `spec.md` por thread: planner lo crea/mantiene; resto lo lee.
- [ ] `artifacts/` por task: workers escriben aquí (mounted al sandbox).
- [x] Eventos `spec.changed`, `artifact.added` → SSE → UI.

### Frontend
- [x] Dashboard del thread muestra:
  - [x] `<TaskGraph>` en vivo (estados con colores).
  - [x] `<BudgetMeter>` con barra de progreso + soft/hard caps.
  - [x] Panel "Live cost" desglosado por agente.
  - [x] Lista de sesiones activas con su rol.
- [x] `<SpecViewer>` lateral muestra `spec.md` con highlight de secciones referenciadas por tasks.
- [x] Botón "Pause/Resume" en dashboard de tasks conectado al kill-switch global del scheduler.
- [ ] Pausa/resume scoped por thread (si se decide separar del kill-switch global).

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
