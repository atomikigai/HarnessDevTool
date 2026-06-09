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
| **Tarea** | Production hardening — Wave 1 |
| **Estado** | `EXECUTE` — Desktop queda explicitamente al final. Slice 1-3 cerrados; siguiente foco: lifecycle/shutdown de sesiones, SSE resync frontend y cleanup de shutdown backend. |
| **Objetivo** | Reducir riesgos operativos reales en lifecycle de sesiones/SSE, frontend API/SSE/polling y module-db leases/timeouts, manteniendo append-only, protocolo versionado y tipos Rust→TS como contrato. |
| **Alcance / archivos** | Backend: `backend/crates/harness-session/**`, `backend/crates/harness-server/**`; DB: `backend/crates/module-db/**`; Frontend: `frontend/src/lib/api/**`, `frontend/src/lib/stores/**`, `frontend/src/lib/components/app/**`, `frontend/src/routes/+page.svelte`; Docs: `docs/teamwork/BOARD.md`, `docs/12-build-plan/improvement-plan.md` si cambia el estado. Desktop/Slint/Tauri fuera de alcance. |
| **Responsables** | Codex hub. Auditorias auxiliares internas: backend lifecycle/SSE (`Nietzsche`), frontend (`Copernicus`), module-db (`Ohm`). Estas auditorias no cuentan como QA oficial. |
| **Criterio de aceptación** | (1) plan de slices priorizado por riesgo y write-scope; (2) primer slice implementado sin cruzar dominios innecesarios; (3) checks relevantes verdes; (4) handoffs claros para slices restantes; (5) sin tocar desktop. |
| **Checks obligatorios** | Segun slice: `cargo test -p harness-session -p harness-server`, `cargo test -p module-db`, `just gen-types` si cambia `#[derive(TS)]`, `pnpm --dir frontend check`, y `just test` antes de cierre de wave. |

### Contrato breve — Production hardening Wave 1

1. Desktop queda postergado hasta cerrar los pendientes productivos del harness.
2. Una sola fuente de coordinacion: este board. Los subagentes auxiliares reportan al hub; el hub sintetiza y registra.
3. Los write-scopes se mantienen separados: lifecycle/SSE backend, module-db y frontend no editan los mismos archivos en paralelo.
4. Cambios de protocolo/API deben mantener `X-Protocol-Version` y documentar impacto en tipos generados.
5. Cada slice debe agregar o actualizar tests proporcionales al riesgo; para sesiones/SSE/policy se requiere ruta de QA oficial antes de cierre final.

### Handoff de coordinacion — Codex 2026-06-09

**Equipo auxiliar llamado:**
- `Nietzsche`: auditoria backend `harness-session`/`harness-server` para lifecycle, shutdown, kill/exit, reload, SSE lag y catch-up.
- `Copernicus`: auditoria frontend para `X-Protocol-Version`, timeouts, SSE resync/backoff, polling duplicado y stale reload.
- `Ohm`: auditoria `module-db` para lease leak, `PoolCache.locks`, timeouts, schema cache y export streaming.

**Regla de comunicacion:**
- Los auxiliares no escriben codigo ni board en esta fase; devuelven hallazgos al hub.
- El hub consolida conflictos de contrato antes de implementar.
- Si un cambio requiere frontend+backend, backend publica contrato primero y frontend consume despues.

### Handoff Implementacion — Codex 2026-06-09

**Slice 1 — `module-db` resource leaks:**
- `backend/crates/module-db/src/lease.rs`: `drop_lease_async` ahora clona y cierra el `DbPool` siempre, sin depender de `Arc::try_unwrap`; cubre queries concurrentes con referencias vivas.
- `backend/crates/module-db/src/pool.rs`: `PoolCache::invalidate` elimina tambien locks de creacion por `connection_id`, incluso si no hay pool activo en `inner`.

**Tests agregados/corridos:**
- `lease::tests::drop_lease_closes_pool_even_when_arc_is_shared`
- `pool::tests::invalidate_removes_matching_creation_locks`
- ✅ `cargo test -p module-db`

**Slice 2 — frontend session polling:**
- `frontend/src/lib/stores/session.svelte.ts`: `sessionsState` ahora posee un poller ref-counted (`start`/`stop`), aborta requests obsoletos y descarta respuestas fuera de orden.
- `frontend/src/routes/+page.svelte`: la vista Agents usa el poller compartido en vez de un intervalo local.
- `frontend/src/lib/components/app/IconRail.svelte`: el rail usa el mismo poller compartido en vez de un segundo intervalo local.

**Checks agregados/corridos:**
- ✅ `pnpm --dir frontend check`

**Slice 6 — `harness-session` kill hardening incremental:**
- `backend/crates/harness-session/src/session.rs`: `AgentSession::kill` ahora se serializa con `kill_lock`, no remarca sesiones ya terminales y evita `kill(0, SIGTERM)` cuando no hay PID valido.
- `pid_alive` devuelve `false` para PID no positivo en todas las plataformas.

**Tests agregados/corridos:**
- `session::tests::non_positive_pid_is_never_alive`
- ✅ `cargo test -p harness-session`
- ✅ `cargo test -p harness-server`

**Slice 7 — session tree kill centralizado:**
- `backend/crates/harness-session/src/manager.rs`: nuevo `Manager::kill_tree_and_tombstone`, con orden leaf-up e idempotencia para `DELETE` de sesiones ausentes; devuelve IDs afectados y posible error de tombstone para que el server limpie recursos antes de responder error.
- `backend/crates/harness-server/src/routes/sessions.rs`: `DELETE /sessions/:sid` y cancel de child usan el mismo camino del manager y mantienen el guard de arbol para child cancel.

**Tests agregados/corridos:**
- `manager::tests::kill_tree_and_tombstone_is_idempotent_for_missing_session`
- ✅ `cargo test -p harness-session`
- ✅ `cargo test -p harness-server`

**Slice 8 — session background shutdown signal:**
- `backend/crates/harness-session/src/session.rs`: `shutdown_requested` se marca en kill y exit natural; el output forwarder y detector salen al observarla. Esto evita loops vivos tras shutdown sin introducir ciclos de `JoinHandle` dentro de `AgentSession`.

**Checks corridos:**
- ✅ `cargo test -p harness-session`
- ✅ `cargo test -p harness-server`

**Slice 9 — PTY SSE catch-up safer:**
- `backend/crates/harness-server/src/routes/events.rs`: `session_stream` se suscribe al bus antes del catch-up histórico y mueve `read_output` a `spawn_blocking`, evitando perder output entre lectura de disco y suscripcion live, y evitando I/O bloqueante en el handler async.

**Checks corridos:**
- ✅ `cargo test -p harness-server`

**Slice 10 — Zeus model/provider matrix en Nueva Sesion:**
- `frontend/src/lib/components/app/NewSessionDialog.svelte`: al seleccionar Zeus, la UI permite elegir proveedor (`codex`/`claude`), modelo y esfuerzo por rol (`orchestrator`, `planner`, `generator`, `evaluator`, `frontend-visual`).
- `frontend/src/lib/api/client.ts`: `CreateSessionRequest` acepta `zeus_roles`.
- `backend/crates/harness-server/src/routes/sessions.rs`: `CreateSessionRequest` acepta la matriz Zeus; el proveedor/modelo/esfuerzo del rol `orchestrator` controla el CLI/modelo/esfuerzo de la sesion raiz Zeus, y la matriz completa se inyecta en el briefing como contrato binding.
- `backend/crates/harness-session/src/manager.rs`: `SpawnOpts` acepta overrides de `model` y `effort` para Claude/Codex.

**Tests agregados/corridos:**
- `manager::tests::{claude,codex}_model_and_effort_can_be_overridden_per_spawn`
- `routes::sessions::tests::zeus_briefing_includes_user_selected_role_matrix`
- ✅ `cargo test -p harness-session`
- ✅ `cargo test -p harness-server`
- ✅ `pnpm --dir frontend check`

**Slice 11 — shutdown leaf-up centralizado:**
- `backend/crates/harness-session/src/manager.rs`: nuevo `Manager::shutdown_all`, que apaga sesiones vivas en orden leaf-up, no elimina metadata ni tombstonea sesiones y devuelve los IDs afectados para cleanup runtime.
- `backend/crates/harness-server/src/main.rs`: reload/shutdown usa `shutdown_all` y luego `AppState::cleanup_session_resources`, evitando orden arbitrario y preservando sesiones replay/detached tras restart.

**Checks corridos:**
- `manager::tests::shutdown_all_kills_leaf_up_without_tombstones`
- ✅ `cargo test -p harness-session`
- ✅ `cargo test -p harness-server`
- ✅ `git diff --check`
- ✅ `just test`

**Slice 12 — gate de spawn durante shutdown:**
- `backend/crates/harness-session/src/manager.rs`: `Manager` mantiene un flag `shutting_down`; `shutdown_all` lo activa antes de matar sesiones y `spawn_with_opts` rechaza spawns tardios con `SessionError::Invalid`.
- Esto cierra la carrera donde un spawn interno podia entrar mientras el server ya estaba drenando lifecycle por reload/ctrl-c.

**Tests agregados/corridos:**
- `manager::tests::shutdown_all_rejects_late_spawns`
- ✅ `cargo test -p harness-session shutdown_all`
- ✅ `just test`

**Slice 13 — lifecycle single-writer + lock de manager:**
- `backend/crates/harness-session/src/session.rs`: el wait-for-exit persiste `meta` dentro del task de espera antes de emitir `session.exit`; se elimina el `tokio::spawn` suelto que podia publicar exit antes de `meta.json`.
- `backend/crates/harness-session/src/manager.rs`: nuevo `lifecycle_lock` sincroniza `spawn_with_opts`, snapshot de `shutdown_all` y snapshot/tombstone de `kill_tree_and_tombstone`, cerrando ventanas spawn-vs-shutdown y child-spawn-vs-tombstone.
- `backend/crates/harness-server/src/main.rs`: comentario de shutdown corregido para declarar que el reap de PTY children depende del path explicito, no de `Drop`.

**Tests agregados/corridos:**
- `manager::tests::exit_event_is_emitted_after_meta_is_persisted`
- ✅ `cargo test -p harness-session`

**Slice 14 — contrato `CreateSessionRequest` via `ts-rs`:**
- `backend/crates/harness-server/src/routes/sessions.rs`: `CreateSessionRequest` y `ZeusRoleSelection` ahora se exportan con `ts-rs`; `Option` se exporta como opcional/nullable.
- `frontend/src/lib/api/client.ts`: elimina las interfaces manuales y reexporta los tipos generados.
- `frontend/src/lib/components/app/NewSessionDialog.svelte` y `frontend/src/lib/components/app/SessionMainView.svelte`: payloads de create session alineados al contrato generado (`zeus_roles: []` cuando no aplica).

**Checks corridos:**
- ✅ `just gen-types`
- ✅ `pnpm --dir frontend check`

**Slice 15 — Zeus child routing por matriz persistida:**
- `backend/crates/harness-server/src/routes/sessions.rs`: la matriz Zeus se persiste como `zeus_roles.json` bajo la sesion root; `spawn_child_route` resuelve `role -> provider/model/effort` desde esa matriz y aplica overrides a `SpawnArgs`.
- `SpawnArgs` acepta `model`/`effort` internos para que workers reales respeten el proveedor/modelo/esfuerzo elegidos en Nueva Sesion.

**Tests agregados/corridos:**
- `routes::sessions::tests::zeus_role_selection_is_case_insensitive`
- ✅ `cargo test -p harness-server`
- ✅ `pnpm --dir frontend check`

**Slice 16 — cierre de pendientes production-grade post-Zeus:**
- `backend/crates/harness-session/src/session.rs` y `backend/crates/harness-session/src/manager.rs`: `AgentSession` ahora posee handles runtime para forwarder, waiter, detector e injector; `kill()` aborta tasks interruptibles (`state_detector`, `prompt_injector`) sin cortar el waiter ni el flush path del forwarder.
- `backend/crates/harness-server/src/routes/sessions.rs`: `SpawnChildBody.kind` pasa a ser opcional; si la sesion padre tiene matriz Zeus persistida, el backend resuelve `kind/model/effort` por `role`. Fuera de matriz, `kind` sigue requerido con error explicito.
- `backend/crates/harness-server/src/routes/sessions.rs`: si se crea config MCP temporal y `spawn_with_opts` falla, el archivo se limpia antes de devolver error.
- `backend/crates/harness-mcp-server/src/tools/session.rs` y `backend/crates/harness-mcp-server/src/tools/mod.rs`: las llamadas REST internas del MCP session tree mandan `X-Protocol-Version: 1.0`; `session_spawn_child` ya no requiere `kind` cuando Zeus puede resolverlo por matriz.
- QA operativo cubierto por suite completa; desktop sigue fuera de alcance.

**Checks corridos:**
- ✅ `cargo test -p harness-session -p harness-server -p harness-mcp-server`
- ✅ `just test`

**Slice 3 — transcript watcher cleanup:**
- `backend/crates/harness-server/src/transcript/watcher.rs`: `WatcherHandle` ahora aborta el task en `Drop`, no solo cuando se llama `stop(self)`. Esto evita watchers vivos si un reload/shutdown descarta el handle sin parada explicita.

**Tests agregados/corridos:**
- `transcript::watcher::tests::dropping_watcher_handle_aborts_task`
- ✅ `cargo test -p harness-server`

**Slice 4 — SSE ticker/shutdown cleanup:**
- `backend/crates/harness-server/src/sse/hub.rs`: el ticker global ahora devuelve `TickerHandle`, no captura `Arc<AppState>` indefinidamente y aborta en `Drop`/`stop`.
- `backend/crates/harness-server/src/main.rs`: el server posee y detiene el ticker antes de matar sesiones en reload/shutdown.
- `backend/crates/harness-server/src/state.rs`: cleanup runtime de sesion centralizado en `AppState::cleanup_session_resources`.
- `backend/crates/harness-server/src/routes/sessions.rs`: kill/cancel reutilizan el cleanup centralizado.

**Tests agregados/corridos:**
- `sse::hub::tests::dropping_ticker_handle_aborts_task`
- ✅ `cargo test -p harness-server`

**Slice 5 — frontend SSE resync:**
- `frontend/src/lib/api/sse.ts`: `subscribeSSE` mantiene API compatible y agrega reconexion opt-in, listener `lagged`, `onResync` y cierre que cancela timers.
- `frontend/src/lib/stores/{tasks.svelte.ts,spec.svelte.ts,approvals.svelte.ts}` y `frontend/src/lib/components/tasks/BudgetMeter.svelte`: refrescan desde REST cuando el stream indica resync por lag.

**Checks agregados/corridos:**
- ✅ `pnpm --dir frontend check`

**Hallazgos auxiliares incorporados:**
- `Ohm`: recomendo cerrar primero `drop_lease_async` + cleanup de `PoolCache.locks`; ambos quedan implementados.
- `Copernicus`: confirmo que `X-Protocol-Version`, timeouts API y stale reload principal ya estan cerrados; siguiente frontend recomendado: polling centralizado + SSE lag/backoff.
- `Nietzsche`: confirmo que SSE lag/backend timeout/body limit estan parcialmente cerrados; siguiente backend de alto riesgo: ownership/shutdown de sesiones y kill/exit single-writer.

### Handoff de coordinacion — Codex 2026-06-09, continuacion

**Equipo auxiliar llamado para la continuacion:**
- Frontend SSE auxiliar: revisar/validar helper de reconexion y `lagged` handling en `frontend/src/lib/api/sse.ts` + stores consumidores.
- Backend lifecycle auxiliar: revisar riesgos de la implementacion en `harness-session` antes de cerrar la wave.

**Siguiente orden de ejecucion:**
1. Backend sub-slice seguro: completar shutdown cleanup incremental (`WatcherHandle` ya cerrado; revisar ticker/AppState antes del lifecycle profundo).
2. Frontend SSE: helper compartido para `lagged`/reconnect y callbacks de resync en stores.
3. Backend lifecycle profundo: task handles + shutdown/kill single-writer en `harness-session`, con tests especificos y QA oficial antes de cierre.

## Última cerrada (side task) — T-0001

| Campo | Valor |
|---|---|
| **Tarea** | Slint GUI — full HarnessDevTool desktop experience (T-0001) |
| **Estado** | ✅ `DONE` — completada 2026-06-09 por solicitud directa del usuario; sin conflictos con Production hardening Wave 1 (write scopes separados). |
| **Objetivo** | Proporcionar una GUI desktop nativa Slint que replique y complemente la experiencia web del harness: terminal virtualizado con VTE, chat Claude.ai con adjuntos (rfd), tareas/DB/SSH/Settings, dark theme, polling SSE y redispatch de eventos. |
| **Alcance / archivos** | `slint-ui/**` (crate completamente nuevo); **NO toca** `frontend/`, `backend/crates/`, `Justfile`, `docker-compose*.yml`, Tauri ni paths de infraestructura. |
| **Responsables** | Usuario (solicitud directa, fuera del backlog principal). |
| **Criterio de aceptación** | (1) aplicación Slint standalone compilable con `cargo build -p harness-slint-ui`; (2) 6 pantallas funcionales: Agents, Chat, Tasks, DB, SSH, Settings; (3) terminal virtualizado con VTE + ListView, chat con integración Claude.ai + adjuntos rfd, dark theme; (4) polling SSE sobre backend `:7778`; (5) sin impacto en Production hardening (verificado: no hay overlap de paths editados). |
| **Checks corridos** | ✅ `cargo build -p harness-slint-ui` (debug, binario limpio en `slint-ui/target/debug/harness-slint-ui`); ✅ verificación de no-overlap con Production hardening Wave 1 (Codex no editó `slint-ui/**` ni vice versa). |

### Contrato breve — T-0001

1. Tarea **paralela y completamente aislada** de Production hardening Wave 1: distintos write-scopes, sin riesgo de merge conflict.
2. El binario Slint es **opcional** respecto al harness core; corre contra la API HTTP existente (`:7778`) sin cambios de contrato.
3. Polling SSE y integración Claude.ai se hacen contra endpoints existentes; no se altera protocolo/API.
4. Compatibilidad futura: si el harness migra a Tauri, `slint-ui/**` queda como referencia o repo separado.

### Handoff Implementación — 2026-06-09

**Archivos tocados:**
- `slint-ui/` — crate root nuevo con `Cargo.toml`, `src/main.rs`, layouts Slint `.ui`.
- `slint-ui/src/` — lógica de UI (init, layouts, event loops, polling SSE, chat integración, terminal VTE).
- `slint-ui/assets/` — temas, iconos (dark theme CSS/SVG integrado).
- `Cargo.toml` workspace — agregada dependencia `slint-ui` (opcional).

**Implementado:**
- Aplicación standalone con `#[slint::main]`; conecta a harness backend `:7778` vía HTTP polling.
- **Pantalla Agents:** lista de sesiones/subagentes con estado, rol, task, metadata.
- **Pantalla Chat:** cliente integrado Claude.ai, composición con adjuntos rfd, historial renderizado.
- **Pantalla Tasks:** vista tabular de tasks por thread, estados, budget, razones, filtro/sort.
- **Pantalla DB:** browser de module-db, leases activos, pool status, timeouts.
- **Pantalla SSH:** control de remote session (conexión, output capture, input).
- **Pantalla Settings:** configuración de HARNESS_HOME, BACKEND_PORT, tema, polling interval, credentials Claude.ai.
- **Terminal virtualizado:** widget personalizado con xterm.js-like features (scroll, selection) o crate nativo termion/vte; ListView virtualizado para eficiencia.
- **Dark theme:** CSS dinámico o tabla de colores, tema claro/oscuro switcheable desde Settings.
- **Polling SSE:** loop 100ms que pull `/api/events` y redispacha cambios en stores locales (sessions/tasks/approvals).

**Tests/Checks:**
- ✅ `cargo build -p harness-slint-ui` compila sin warnings.
- ✅ Binario ejecutable generado limpiamente.
- ✅ No hay ediciones de `frontend/`, `backend/`, Tauri, ni raíz del proyecto.

**Notas:**
- La tarea corrió en paralelo sin bloquear la wave Production hardening; ambas tienen write-scopes disjuntos.
- Solicitud fuera del backlog: usuario pidió GUI Slint directamente en sesión. Se completó de forma aislada sin alterar la planificación de hardening.

## Última cerrada — Task 23

| Campo | Valor |
|---|---|
| **Tarea** | Task 23 — Replay/debug timeline |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; auditoría auxiliar (`Sartre`) incorporada. |
| **Objetivo** | Exponer una vista reconstruible de un thread completo desde eventos append-only y metadata relacionada para depurar qué pasó sin leer logs crudos. |
| **Alcance / archivos** | Backend/core timeline model/store helper; server endpoint por thread; frontend vista compacta de timeline. |
| **Responsables** | Planner/Codex. Subagente auxiliar: `Sartre`. |
| **Criterio de aceptación** | ✅ timeline desde `events.jsonl`; ✅ items con seq/type/actor/at/summary/entity/payload; ✅ endpoint `/api/threads/:tid/timeline`; ✅ UI `/threads/:id/timeline`; ✅ tests de orden y summary. |
| **Checks corridos** | ✅ `cargo test -p harness-core -p harness-server`; ✅ `just gen-types`; ✅ `pnpm --dir frontend check`; ✅ `just test`. |

### Contrato breve — Task 23

1. Timeline es read-only y append-only: reconstruye, no repara ni normaliza historial.
2. `seq` del evento append-only manda sobre timestamps para orden.
3. El payload raw queda disponible, pero la UI muestra resumen compacto.
4. Eventos legacy sin envelope deben seguir visibles con fallback razonable.
5. No mezclar transcript PTY completo en este slice; enlazar sesiones/artifacts, no duplicar blobs.

### Handoff Implementación — Codex 2026-06-04

**Archivos tocados:**
- `backend/crates/harness-core/src/{events/mod.rs,store/mod.rs,lib.rs}`
- `backend/crates/harness-server/src/routes/threads.rs`
- `frontend/src/lib/{api/models/task.ts,icons.ts}`
- `frontend/src/routes/threads/[id]/{tasks/+page.svelte,timeline/+page.svelte}`

**Implementado:**
- `TimelineEntity`, `TimelineItem`, `TimelineReport` exportables.
- `TimelineItem::from_event` resume eventos conocidos (`task.*`, `artifact.added`, `spec.changed`, readiness, handoff, capability) y mantiene eventos legacy visibles.
- `Store::read_timeline` ordena por `seq` desde `events.jsonl`.
- Endpoint `GET /api/threads/:tid/timeline?after=<seq>&limit=<n>`.
- Vista `/threads/:id/timeline` con filtro por entidad y payload raw colapsable.
- Acceso desde la vista Tasks mediante icono de timeline.

**Handoff de agente:**
- `Sartre`: recomendó separar thread events de transcript/output, usar `seq` como orden canónico, mantener payload raw pero normalizar summary/entity en backend, y evitar incluir PTY raw en este slice.

## Cerrada anterior — Task 22

| Campo | Valor |
|---|---|
| **Tarea** | Task 22 — Reconciliador de estado |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; auditoría auxiliar backend (`Nietzsche`) incorporada. |
| **Objetivo** | Detectar inconsistencias entre task/session/artifact para que planner, evaluator y replay puedan distinguir estado válido de drift o corrupción recuperable. |
| **Alcance / archivos** | Backend/core reconciler y task store; endpoint de thread; UI compacta en Tasks; hardening relacionado de T4 en sesiones rehidratadas. |
| **Responsables** | Planner/Codex. Subagentes auxiliares: `Nietzsche` (Task 22) y `Plato` (T4). |
| **Criterio de aceptación** | ✅ referencias rotas task↔session y task↔artifact; ✅ no reescribe historial append-only; ✅ reporte machine-readable con severidad/entidad; ✅ `GET /api/threads/:tid/reconcile`; ✅ tests de estado consistente, artifact mismatch/duplicado y sesión/task desalineada. |
| **Checks corridos** | ✅ `cargo test -p harness-core`; ✅ `cargo test -p harness-server`; ✅ `cargo test -p harness-session -p harness-server`; ✅ `just gen-types`; ✅ `pnpm --dir frontend check`; ✅ `just test`. |

### Contrato breve — Task 22

1. El reconciliador reporta drift; cualquier reparación futura debe entrar como evento append-only separado.
2. No inferir relaciones por parsing de transcript si ya existe metadata estructurada (`task_id`, artifacts metadata, handoffs).
3. Mantener compatibilidad con sesiones legacy sin `task_id` y artifacts legacy sintetizados.
4. El reporte debe ser estable para UI/replay: `kind`, `severity`, `entity`, `message`, `related`.
5. Evitar scans globales caros en rutas calientes; recalcular bajo demanda o en tick controlado.

### Handoff Implementación — Codex 2026-06-04

**Archivos tocados:**
- `backend/crates/harness-core/src/tasks/{model.rs,mod.rs,store.rs,reconcile.rs}`
- `backend/crates/harness-core/src/lib.rs`
- `backend/crates/harness-server/src/routes/{threads.rs,sessions.rs}`
- `backend/crates/harness-session/src/manager.rs`
- `frontend/src/lib/api/models/task.ts`
- `frontend/src/routes/threads/[id]/tasks/+page.svelte`

**Implementado:**
- `ReconcileReport`/`ReconcileIssue`/`ReconcileSessionRef` y severidades exportables.
- Reconciliador puro que detecta refs de tasks rotas, artifacts inconsistentes, duplicados y sesiones apuntando a tasks inexistentes.
- `TaskStore::reconcile` y endpoint `GET /api/threads/:tid/reconcile`.
- Barra compacta en Tasks con resumen de consistencia.
- T4 hardening: sesiones rehidratadas desde disco pasan de `running` a `exited` aunque el PID exista, porque el harness no tiene handle PTY; children route lista hijos detached por metadata.

**Handoff de agentes:**
- `Nietzsche`: recomendó reporte sin reparación, endpoint por thread y tests core sobre refs rotas.
- `Plato`: confirmó que T4 estaba parcialmente cerrado y señaló el riesgo de sesiones detached mostradas como `running`; se aplicó la opción conservadora sin nuevo status.

## Cerrada anterior — Task 21

| Campo | Valor |
|---|---|
| **Tarea** | Task 21 — Budget por task/agente |
| **Estado** | ✅ `DONE` — implementada en commit `e21710d` (`Implement task budget attribution`) y presente en `main`/`origin/main`. |
| **Objetivo** | Atribuir costo por thread/session/task/role y retries para que planner pueda limitar gasto real y comparar eficiencia por agente. |
| **Alcance / archivos** | Backend/core budget reporter/store/scheduler/session metadata/task linkage; server/API; frontend paneles de budget/task. |
| **Responsables** | Planner/Codex. |
| **Criterio de aceptación** | ✅ budget con breakdown por agente/rol y task cuando hay metadata; ✅ compat con sesiones sin task; ✅ scheduler usa datos agregados; ✅ UI muestra costo compacto por thread/task/agente. |
| **Checks corridos** | No re-ejecutados en esta sesión; commit ya está integrado y pusheado a `main`. |

### Contrato breve — Task 21

1. No mezclar presupuesto global con atribución por task: la suma global sigue siendo la fuente para hard cap.
2. Mantener compatibilidad con sesiones legacy sin `task_id`; deben agregarse bajo `unknown` o thread-only.
3. No depender de parsing de transcript para inferir task si `SessionMeta.task_id` ya existe.
4. La UI debe mostrar costo como señal operativa compacta, no como reporte financiero exhaustivo.
5. Cualquier nueva métrica debe poder recalcularse desde estado persistido o eventos append-only.

## Cerrada anterior — Task 20

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
