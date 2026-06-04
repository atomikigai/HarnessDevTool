# BOARD — Equipo de desarrollo de HarnessDevTool

Canal común entre Planner (Claude), Backend Rust (Codex), Frontend (Cursor) y los evaluadores
(Sonnet). Plantilla **estricta por campos**, no prosa libre. Ver `CLAUDE.md` §4.
Modelo operativo: ver [`docs/teamwork/OPERATING_MODEL.md`](./OPERATING_MODEL.md).

> **Límite conocido:** una sola tarea "En curso" a la vez, sin locking real. El Planner es el único
> que abre/cierra; los ejecutores anotan en su bloque de Handoff. Revisor/QA reportan por la Agent
> tool (no escriben aquí).
>
> **Balance velocidad/calidad:** subagentes internos de Codex pueden acelerar self-review, pero no
> sustituyen reviewer/QA oficial lanzado por Claude/Planner desde el harness.

---

## En curso

| Campo | Valor |
|---|---|
| **Tarea** | Task 21 — Budget por task/agente |
| **Estado** | `OPEN` — pendiente inicializar auditoría antes de implementar. |
| **Objetivo** | Atribuir costo por thread/session/task/role y retries para que planner pueda limitar gasto real y comparar eficiencia por agente. |
| **Alcance / archivos** | Backend/core budget reporter/store/scheduler/session metadata/task linkage; server/API si aplica; frontend paneles de budget/task; tests de agregación y compat. |
| **Responsables** | Planner/Codex. Usar auditoría auxiliar backend/frontend antes del primer patch. |
| **Criterio de aceptación** | (1) el budget conserva breakdown por agente/rol y puede asociarse a task cuando hay metadata; (2) retries/reasignaciones no duplican costo; (3) scheduler/pausa usan datos agregados correctamente; (4) UI muestra costo compacto por thread/task/agente; (5) tests cubren sesiones sin task y sesiones con task. |
| **Checks obligatorios** | `cargo test -p harness-core -p harness-mcp-server -p harness-server`; `just gen-types` si cambian tipos exportados; `pnpm check`; `just test` al cierre. |

### Contrato breve — Task 21

1. No mezclar presupuesto global con atribución por task: la suma global sigue siendo la fuente para hard cap.
2. Mantener compatibilidad con sesiones legacy sin `task_id`; deben agregarse bajo `unknown` o thread-only.
3. No depender de parsing de transcript para inferir task si `SessionMeta.task_id` ya existe.
4. La UI debe mostrar costo como señal operativa compacta, no como reporte financiero exhaustivo.
5. Cualquier nueva métrica debe poder recalcularse desde estado persistido o eventos append-only.

## Última cerrada — Task 20

| Campo | Valor |
|---|---|
| **Tarea** | Task 20 — Scheduler explain/debug |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; auditorías auxiliares backend/frontend incorporadas y checks verdes. |
| **Objetivo** | Explicar por qué el scheduler asignó, saltó o enfrió una task para que planner/UI/agentes puedan depurar decisiones sin leer logs internos. |
| **Alcance / archivos** | Backend/core scheduler tick/decisions/events/task snapshot; server SSE; frontend TaskDetail/SessionRightPanel/listas y task store. |
| **Responsables** | Planner/Codex. Subagentes auxiliares Codex: backend audit (`Erdos`) y frontend audit (`Schrodinger`). |
| **Criterio de aceptación** | ✅ decisiones relevantes del scheduler tienen razón estructurada; ✅ se distinguen assign/skip/cooldown/evaluator/lease; ✅ `task.scheduler.decision` queda append-only; ✅ UI muestra explicación compacta; ✅ tests cubren assign y max-concurrency skip. |
| **Checks corridos** | ✅ `cargo test -p harness-core -p harness-mcp-server -p harness-server`; ✅ `just gen-types`; ✅ `pnpm --dir frontend check`; ✅ `just test`. |

### Contrato breve — Task 20

1. No cambiar la política del scheduler en esta task; solo hacer visibles sus decisiones y razones.
2. Las razones deben ser estructuradas y cortas, pero conservar contexto humano legible.
3. Cualquier evento nuevo debe seguir el contrato append-only y ser compatible con replay.
4. La UI debe mostrar el motivo sin saturar la lista ni convertirlo en log raw.
5. El debug debe ayudar a distinguir “no asignable todavía” de “error operativo”.

### Handoff Implementación — Codex 2026-06-04

**Archivos tocados:**
- `backend/crates/harness-core/src/scheduler/tick.rs`
- `backend/crates/harness-core/src/tasks/{model.rs,events.rs,store.rs,mod.rs,state_machine.rs}`
- `backend/crates/harness-core/{src/lib.rs,schemas/task.v1.json}`
- `backend/crates/harness-server/src/routes/events.rs`
- `frontend/src/lib/api/models/task.ts`
- `frontend/src/lib/stores/tasks.svelte.ts`
- `frontend/src/lib/components/{tasks/TaskDetail.svelte,app/SessionRightPanel.svelte}`
- `frontend/src/routes/threads/[id]/tasks/+page.svelte`

**Implementado:**
- `SchedulerExplanation` + `SchedulerDecisionKind` quedan en snapshot de `Task`.
- `TaskEvent::SchedulerDecision` emite `task.scheduler.decision` append-only y SSE.
- `TaskStore::record_scheduler_decision` deduplica explicaciones idénticas para evitar spam del tick.
- Scheduler registra ready, auto-unblocked, assigned, assignment skipped, cooldown added/skipped, evaluator skipped/routed y lease expired.
- TaskDetail muestra la explicación en el bloque compacto de razones; lista y panel lateral usan el chip de atención existente.

**Checks corridos:**
- `cargo test -p harness-core -p harness-mcp-server -p harness-server` ✅
- `just gen-types` ✅
- `pnpm --dir frontend check` ✅
- `just test` ✅

## Cerrada anterior — Task 19

| Campo | Valor |
|---|---|
| **Tarea** | Task 19 — Razones estructuradas en tasks |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; auditorías auxiliares backend/frontend incorporadas y checks verdes. |
| **Objetivo** | Evitar que bloqueos, pausas, rechazos, fallos y necesidades humanas queden escondidos en strings libres; exponer razones operativas legibles y machine-readable. |
| **Alcance / archivos** | Backend/core task model/store/state machine/events/schema; MCP/API task patch; frontend TaskDetail, badges/listas y tipos. |
| **Responsables** | Planner/Codex. Subagentes auxiliares Codex: backend audit (`Einstein`) y frontend audit (`Curie`). |
| **Criterio de aceptación** | ✅ campos para `blocked_reason`, `paused_reason`, `rejected_reason`, `last_failure` y `needs_human`; ✅ state machine exige razón donde aplica; ✅ TaskDetail/badges muestran razones compactas; ✅ `task.reason.changed` se emite cuando cambian razones; ✅ tests cubren transiciones con/sin razón requerida. |
| **Checks corridos** | ✅ `cargo test -p harness-core -p harness-mcp-server -p harness-server`; ✅ `just gen-types`; ✅ `pnpm check`; ✅ `just test`. |

### Contrato breve — Task 19

1. Mantener compatibilidad con `notes.feedback`, `why_paused` y `why_abandoned`.
2. No rigidizar comentarios libres; estructurar solo razones operativas que scheduler/UI/agentes necesitan entender.
3. Cualquier reparación o cambio de razón debe ser trazable por evento append-only.
4. `blocked`/`paused`/`rejected`/`needs_human` deben explicar el bloqueo sin obligar a leer logs internos.
5. UI debe mostrar razones sin saturar el panel de tasks.

### Handoff Implementación — Codex 2026-06-04

**Archivos tocados:**
- `backend/crates/harness-core/src/tasks/{model.rs,events.rs,store.rs,state_machine.rs}`
- `backend/crates/harness-core/src/scheduler/tick.rs`
- `backend/crates/harness-core/schemas/task.v1.json`
- `backend/crates/harness-mcp-server/src/tools/mod.rs`
- `backend/crates/harness-server/src/routes/events.rs`
- `frontend/src/lib/api/{models/task.ts,schemas/task.ts}`
- `frontend/src/lib/components/{tasks/TaskDetail.svelte,app/SessionRightPanel.svelte}`
- `frontend/src/routes/threads/[id]/tasks/+page.svelte`

**Implementado:**
- `Notes` agrega razones estructuradas y conserva fallback legacy.
- `TaskPatch` acepta razones top-level y `notes` anidado para `task_update`.
- La state machine exige razón al pausar/bloquear y al devolver `pending_verify -> in_progress`.
- `TaskEvent::ReasonChanged` emite `task.reason.changed` para cambios trazables.
- MCP `task_update` documenta razones estructuradas y legacy.
- TaskDetail/listas/panel lateral muestran indicadores compactos de atención.

**Checks corridos:**
- `cargo test -p harness-core -p harness-mcp-server -p harness-server` ✅
- `just gen-types` ✅
- `pnpm check` ✅
- `just test` ✅

## Última cerrada — Task 18

| Campo | Valor |
|---|---|
| **Tarea** | Task 18 — Artifacts como entidad/evento real |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; auditorías auxiliares backend/frontend incorporadas, findings de revisión auxiliar corregidos y checks verdes. Commit `c33e8de`. |
| **Objetivo** | Modelar artifacts con metadata propia para que evaluator, replay y UI sepan quién produjo cada evidencia, cuándo, de qué tipo es y cómo se relaciona con una task. |
| **Alcance / archivos** | Backend/core task model/store/events/schema, MCP `task_submit`, server routes/SSE si hace falta; frontend TaskDetail/SessionRightPanel y tipos/API. |
| **Responsables** | Planner/Codex. Subagentes auxiliares Codex: backend audit (`Harvey`) y frontend audit (`Herschel`). QA oficial requerida antes de cierre por tocar append-only/eventos. |
| **Criterio de aceptación** | (1) existe `Artifact { artifact_id, task_id, kind, path, produced_by, created_at, summary }`; (2) kinds iniciales `file`, `diff`, `test_output`, `screenshot`, `log`; (3) artifacts se persisten como eventos append-only sin duplicar blobs; (4) `task.artifacts` mantiene compatibilidad con `task_submit`; (5) TaskDetail/SessionRightPanel muestran metadata relevante; (6) tests cubren creación, listado y referencia inexistente. |
| **Checks obligatorios** | ✅ `cargo test -p harness-core -p harness-mcp-server -p harness-server`; ✅ `just gen-types`; ✅ `pnpm check`; ✅ `just test`. |

### Contrato breve — Task 18

1. No reescribir historial: cada artifact nuevo se registra como evento append-only y referencia blobs por path.
2. Mantener compatibilidad con `task_submit` actual: los callers legacy que mandan `artifacts.files/turns/diff` deben seguir funcionando.
3. La metadata nueva debe ser recuperable por task y usable por replay/debug sin depender solo del snapshot mutable de la task.
4. Los paths de artifacts son referencias, no contenido grande; no duplicar logs/diffs/screenshots dentro del evento salvo resumen corto.
5. UI muestra evidencia inspeccionable de forma compacta, sin bloquear el flujo existente de submit/verificación.

### Handoff inicial — Planner/Codex 2026-06-04

**Backend audit auxiliar (`Harvey`)**:
- Auditar `Task.artifacts`, `TaskStore::submit`, `TaskEvent::ArtifactAdded`, MCP `task_submit`, schemas y tests.
- Reportar archivos backend a tocar y riesgos de compatibilidad.

**Frontend audit auxiliar (`Herschel`)**:
- Auditar TaskDetail/SessionRightPanel/SpecViewer, modelos task y API client.
- Reportar dónde integrar artifact metadata y qué checks frontend cubrir.

**Ruta propuesta**:
1. ✅ Integrar hallazgos de auditoría.
2. ✅ Implementar backend/core + tests.
3. ✅ Regenerar tipos si el contrato Rust cambia.
4. ✅ Implementar UI mínima.
5. ✅ Correr checks; revisión auxiliar final (`Averroes`) reportó 4 findings y quedaron corregidos.

### Handoff Implementación — Codex 2026-06-04

**Archivos tocados:**
- `backend/crates/harness-core/src/tasks/{model.rs,events.rs,store.rs,state_machine.rs,mod.rs}`
- `backend/crates/harness-core/schemas/task.v1.json`
- `backend/crates/harness-server/src/routes/{tasks.rs,events.rs,spec.rs}`
- `backend/crates/harness-mcp-server/src/tools/mod.rs`
- `frontend/src/lib/api/{client.ts,models/task.ts}`
- `frontend/src/lib/{stores/tasks.svelte.ts,components/tasks/TaskDetail.svelte,components/app/SessionRightPanel.svelte}`

**Implementado:**
- `ArtifactKind` + `Artifact` exportados desde Rust; `Artifacts` conserva `files/turns/diff` y agrega `metadata`.
- `TaskStore::submit` normaliza artifacts legacy a metadata, mantiene el snapshot legacy y emite `artifact.added` por artifact.
- `TaskEvent::ArtifactAdded` incluye `artifact_id`, `task_id`, `produced_by` y `summary`, con defaults para eventos históricos.
- `GET /api/threads/:tid/tasks/:task_id/artifacts` lista metadata de una task.
- `task_submit` MCP mantiene compatibilidad y corrige schema de `turns` a array de strings.
- TaskDetail muestra metadata compacta; SessionRightPanel muestra conteo; task store refresca en `artifact.added`.

**Checks corridos:**
- `cargo test -p harness-core -p harness-mcp-server -p harness-server` ✅
- `just gen-types` ✅
- `pnpm check` ✅
- `just test` ✅

**Review auxiliar (`Averroes`)**:
- Finding M: endpoint nuevo devolvía `[]` para artifacts legacy sin metadata. Corregido: `list_artifacts` sintetiza metadata on-read.
- Finding M: submit híbrido `metadata + files/turns/diff` omitía eventos/visibilidad de legacy. Corregido: normalización combina metadata existente con legacy no duplicado.
- Finding L/M: schema MCP no exponía metadata y exigía `files`. Corregido: `metadata` documentado y `files` ya no es requerido.
- Finding L: tipos frontend duplicaban `Artifact`. Corregido: el modelo manual re-exporta `Artifact`/`ArtifactKind` generados por `ts-rs`.

**Notas:**
- `Justfile` tiene un cambio no relacionado (`setup`) presente en el worktree y no pertenece a esta task.

## Cerrada anterior — Task 17

| Campo | Valor |
|---|---|
| **Tarea** | Task 17 — `spec.md` append-only con versiones |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; spec append-only en `spec.events.jsonl`, versión global/seccional, endpoint seccional con stale-write guard, MCP `spec_set_section`, `Task.spec_refs` y eventos `spec.changed` versionados. |
| **Objetivo** | Crear una spec por thread versionada y referenciable desde tasks para que workers/evaluator verifiquen contra una versión concreta. |
| **Alcance / archivos** | Backend spec route, MCP spec tool, task model/store/API, task schema, frontend spec/task types. |
| **Responsables** | Planner/Codex con subagentes Codex para auditoría de contrato y docs. |
| **Criterio de aceptación** | (1) `spec.md` se actualiza de forma append-only/versionada; (2) tasks referencian `{ section, version }`; (3) stale writes fallan con error explícito; (4) `spec.changed` queda en eventos/SSE; (5) tests relevantes verdes. |
| **Checks corridos** | `cargo test -p harness-core -p harness-server -p harness-mcp-server`; `just gen-types`; `pnpm check`; `just test`. |

### Contrato breve — Task 17

1. La spec es estado por thread y sus cambios son trazables; no se reescribe historial.
2. Solo planner/orchestrator puede editar spec; workers/evaluator solo leen y reportan contra versiones.
3. Cada cambio incrementa una versión estable que las tasks pueden referenciar.
4. `spec.set_section` debe exigir `spec_version_required` para evitar writes obsoletos.
5. Los eventos `spec.changed` deben permitir replay/debug sin depender de estado implícito.

## Cerrada anterior — Task 16

| Campo | Valor |
|---|---|
| **Tarea** | Task 16 — Metadata fuerte de subagentes |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; `SessionMeta` persiste owner/task/scopes, REST/MCP child spawn aceptan task/scopes opcionales, DB agents reciben scopes DB, y el tab Agents muestra task/scopes cuando existen. |
| **Objetivo** | Hacer que cada subagente sea atribuible a un thread, task, rol, padre/root y scope autorizado. |
| **Alcance / archivos** | Backend/session metadata, spawn child REST/MCP, DB agent scopes, frontend tab Agents y tipos manuales. |
| **Responsables** | Planner/Codex con subagentes Codex para auditoría de contrato y docs. |
| **Criterio de aceptación** | (1) `SessionMeta` conserva compatibilidad con sesiones legacy; (2) `session_spawn_child` rellena metadata fuerte; (3) parent/root/task/role/scopes persisten en `meta.json`; (4) REST expone metadata segura; (5) tab Agents muestra rol/task/estado/scopes; (6) tests relevantes verdes. |
| **Checks corridos** | `cargo test -p harness-session -p harness-server -p harness-mcp-server`; `just gen-types`; `pnpm check`; `just test`. |

### Contrato breve — Task 16

1. Extender metadata de subagentes sin migración destructiva ni rewrite de logs.
2. Mantener campos nuevos opcionales/default-safe para sesiones existentes.
3. `owner_session_id`, `task_id`, `role` y `scopes` no deben exponer secretos.
4. Parent/root se heredan de forma determinística: hija apunta a padre inmediato y conserva root del árbol.
5. Las operaciones vivas siguen usando el contrato actual de sesión; la metadata nueva es trazabilidad, no cambio del hot path PTY.

## Cerrada anterior — T4

| Campo | Valor |
|---|---|
| **Tarea** | T4 — Rehidratación de sesiones tras reinicio (**gate de dogfooding**) |
| **Estado** | ✅ `DONE` — **Frontend HECHO** (`+page.svelte`, carrera de selección, commit `2226794`); **Backend IMPLEMENTADO** (Codex, 2026-06-04): rehidratación detached en `Manager`, boot hook en `AppState`, `/api/threads` consume `list_metas()`. Task 30 cerrada: `ReadinessReport.facts` exporta `unknown`, outputs crate-locales ignorados, `pnpm check` verde. Smoke real y `just test` verde. |
| **Objetivo** | Que las sesiones sobrevivan al reinicio del server: hoy `Manager::new` arranca con `DashMap` vacío y nada rehidrata desde disco, aunque `meta.json`+`output.log` persisten. + arreglar la carrera de auto-selección del frontend que pisa la selección restaurada. |
| **Alcance / archivos** | Backend: `harness-session/src/manager.rs` (+ `meta.rs`/`session.rs` si hace falta), `harness-server/src/{state.rs,routes/threads.rs}`. Frontend: `frontend/src/routes/+page.svelte`. Write scopes separados. |
| **Responsables** | Planner (audit+contrato+verify, par-revisión cercana de `harness-session`), Backend/Codex, Frontend/Cursor, Sonnet (review+QA) |
| **Criterio de aceptación** | (1) tras reinicio, `GET /api/threads` incluye las sesiones previas (Exited/Killed) agrupadas por thread; (2) `Running` huérfano se reconcilia a `Exited` vía `pid_alive`; (3) abrir una sesión muerta muestra su transcript (`read_output`) + affordance de restart; (4) el frontend NO pisa la selección restaurada con `allSessions[0]`; (5) `just test` verde |
| **Checks obligatorios** | `just test` + reinicio real del backend (`just dev-backend`, crear sesión, reiniciar, `curl /api/threads`) + pnpm check frontend |

### Audit — HECHO

- `Manager` (`manager.rs:69`): `sessions: DashMap<String, Arc<AgentSession>>` solo vivas; `all()` solo vivas;
  `read_output` (`:121`) YA lee de disco (sirve para exited).
- `AgentSession` (`session.rs:21`) exige `pty_writer`+`killer` vivos → NO reconstruible de disco.
- `SessionMeta` (`meta.rs`) describe todo (id/kind/thread_id/cwd/pid/status/started_at/exit_code/role/
  parent_root/detected_state/has_transcript) y se persiste a `<sid>/meta.json`.
- La lista (`threads.rs:75`) **solo necesita `SessionMeta`** (`s.meta().await`) → no requiere sesión viva.
- `pid_alive(pid)` existe (`session.rs:441`) para reconciliar `Running` huérfano.
- Consumidores de `get()` (input/kill/child, `sessions.rs:608/692/897`) requieren sesión VIVA →
  detached debe fallar limpio (404 → UI restart), no romper.

### Contrato (CONFIRMADO — divergencia justificada del improvement-plan)

> El improvement-plan sugería `enum { Live, Detached(SessionMeta) }`. **El Planner opta por un mapa
> paralelo** porque la lista solo consume `SessionMeta` y las ops vivas (input/kill) deben excluir
> detached de todos modos → menor blast radius en un crate de alto riesgo (PTY). Comportamiento de
> usuario idéntico.

**Backend (`harness-session` + `harness-server`):**
1. `Manager`: añadir `detached: DashMap<String, SessionMeta>` paralelo al `sessions` vivo.
2. `Manager::load_existing()`: escanear `sessions_root`, leer cada `<sid>/meta.json` (skip-and-warn por
   error de parse, como `list_threads`), reconciliar: si `status==Running` && `!pid_alive(pid)` →
   `Exited`. Insertar en `detached` SOLO si `sid` no está en el mapa vivo. Llamarla 1× al boot
   (`state.rs`, tras `Manager::new`).
3. `Manager::list_metas() -> Vec<SessionMeta>`: mergea vivas (`.meta().await`) + `detached` (vivas
   ganan en colisión de id).
4. `routes/threads.rs:list_threads`: usar `list_metas()` en vez de iterar `all()`.
5. Al spawnear/restart (sesión entra al mapa vivo): remover su id de `detached` (evita duplicados).
6. **NO** tocar `AgentSession`/hot path del PTY. **NO** introducir el enum. `get()`/input/kill siguen
   solo-vivas.
7. Tests: load_existing rehidrata de disco; `Running` huérfano→`Exited` vía pid_alive; list_metas
   mergea; viva sombrea detached; spawn remueve de detached.

**Frontend (`+page.svelte`, carrera de selección):**
1. Gatear los `$effect` de auto-selección con un flag `profileResolved`.
2. En mount: resolver perfil → restaurar `selectedSessionId` persistido para esa clave de perfil →
   auto-elegir `allSessions[0]` SOLO si no hay selección válida persistida/actual.
3. Nunca sobrescribir la selección con una sesión nueva salvo `onCreated` explícito.
4. El `$effect` espejo que persiste la selección NO debe correr antes de resolver el perfil (evita
   persistir el valor equivocado bajo la clave equivocada).

**Tipos:** `SessionMeta`/`SessionStatus` ya exportados por ts-rs; no se esperan cambios de tipo
(verificar). Contrato REST `/api/threads` sin cambios de forma (la lista ahora incluye más sesiones).

### Handoff Backend — Codex 2026-06-04

**Archivos tocados:**
- `backend/crates/harness-session/src/manager.rs`
- `backend/crates/harness-session/src/session.rs`
- `backend/crates/harness-server/src/state.rs`
- `backend/crates/harness-server/src/routes/threads.rs`

**Implementado:**
- `Manager` mantiene `detached: DashMap<String, SessionMeta>` paralelo al mapa vivo.
- `Manager::load_existing()` escanea `sessions_root`, salta `meta.json` corruptos con warn, normaliza
  `root_session_id` vacío y reconcilia `Running` huérfano a `Exited` persistiendo `meta.json`.
- Caso especial: `pid == 0` se marca `Exited` sin llamar `pid_alive(0)`.
- `Manager::list_metas().await` mergea vivas + detached, con vivas ganando por id.
- `spawn_with_opts()` elimina cualquier detached con el mismo id antes de insertar la sesión viva.
- `AppState::new()` llama `manager.load_existing()?` inmediatamente tras `Manager::new`.
- `routes/threads.rs:list_threads` usa `list_metas()`; `Manager::all()` conserva semántica de solo vivas.

**Tests corridos:**
- `cargo test -p harness-session` ✅ 20/20
- `cargo test -p harness-server` ✅ 20/20
- backend dentro de `just test` ✅
- `just gen-types` ✅ (sin cambios de tipos esperados para T4)
- Smoke manual backend ✅: con `HARNESS_HOME` temporal y `HARNESS_BIND=127.0.0.1:7797`, crear thread +
  sesión `cursor`, reiniciar server con el mismo home y validar:
  `/api/threads` lista la sesión rehidratada bajo el mismo `thread_id`, `/api/sessions/:sid` lee
  `meta.json`, `/api/events?session=:sid` entrega catch-up desde `output.log`, y
  `/api/sessions/:sid/input` responde 404 para detached.

**Verify completo:**
- `ReadinessReport.facts` usa el patrón existente `#[cfg_attr(feature = "ts-export", ts(type = "unknown"))]`
  para `serde_json::Value`; `just gen-types` regenera un TS consumible sin importar `JsonValue`.
- `.gitignore` ignora `backend/crates/*/bindings/` y `frontend/src/lib/api/crates/`, ambos outputs generados.
- `frontend/package.json` define `test: pnpm check`, y `Justfile:test` ejecuta `pnpm check` para que el gate
  frontend exista.
- `pnpm check` ✅ 0 errores / 0 warnings.
- `just test` ✅.

### Cierre
T4 cerrada. No queda trabajo activo en esta tarea; cualquier mejora posterior debe abrirse como task nueva.

## Historial (cerradas)

- **T4 — Rehidratación de sesiones tras reinicio** — DONE ✅.
  `Manager::load_existing()` rehidrata sesiones desde disco, reconcilia `Running` huérfanas,
  `/api/threads` lista metas vivas + detached, transcripts siguen disponibles desde `output.log`.
  Smoke real de reinicio + `just test` verdes.

- **Task 27 — Broadcast SSE + UI de propuestas** — VERIFY ✅.
  `task_propose` ahora delega al REST cuando hay `server_url`, `POST /tasks` acepta `status=proposed`
  y la propuesta entra por el `TaskStore` del server para emitir `task.created`/SSE. UI: tipo/schema/filtro
  incluyen `proposed` y el drawer permite promover a `queued` o `blocked` según dependencias.

- **Task 15 — Eventos append-only unificados** (slice incremental backend-only) — VERIFY ✅.
  `Event` evolucionado a envelope aditivo (`thread_id`/`actor`/`payload`, records viejos deserializan);
  `Store::append_event` asigna `seq` atómicamente bajo el `write_lock` y lo retorna (**cierra Task 28**,
  el race de `seq`); TaskEvents (task/scheduler/spec) ahora se persisten como envelopes vía
  `TaskStore::with_event_store` (MCP sink-free, sin doble-escritura); `emit` best-effort (no tumba la
  operación si falla el audit-log); formato SSE/`TaskEvent` intacto (cero frontend). QA PASS 6/6,
  118 tests verdes, clippy limpio, `Event.ts` regenerado. Review: P1 (emit fail-fast) y 2×P2 corregidos.

- **Task 14 — Capability policy middleware mínimo** — VERIFY ✅. Matriz `capability_default` como
  fuente única en `harness-policy` (planner/orch full; worker/generator deny `task_create`/`spec_write`;
  evaluator deny sensitive; None/desconocido permisivo). `Rule.role` opcional + `evaluate(tool,args,role)`
  con precedencia rules→matriz→fallback. Dispatcher reenvía `role`; online=server autoritativo+audit,
  offline=fail-closed local; `task_create_restricted` hardcoded eliminado. Audit append-only
  `capability.decided` (deny/ask) en `/api/approvals/check`. QA PASS 7/7, 65 tests verdes, clippy limpio,
  `Rule.ts` regenerado. **Trade-offs aceptados (decisión del usuario):** rol desconocido→permisivo
  reabre role-stuffing (mitigado: `role` lo fija la infra de spawn `--role`, no el modelo). **Follow-ups:**
  Task 28 (race de `seq` en `append_event`, sistémico → Task 15), Task 29 ejecutada 2026-06-04
  (root spawn valida roles, `remembered_rule` preserva rol, offline sin rol/desconocido niega tools
  sensibles), Task 30 ejecutada 2026-06-04.
- **Task 13 — Separar `task.create` y `task.propose`** — VERIFY ✅. `TaskStatus::Proposed`,
  `task_propose` (cualquier rol) crea en `proposed`, `task_create` con gate de rol exacto fail-closed
  en el dispatcher (deny FUERA de `harness-policy`). `Proposed→Queued` vía `task_update`; no
  reclamable ni agendable. QA PASS 5/5, 166 tests verdes, tipos regenerados. Follow-up SSE → backlog.
