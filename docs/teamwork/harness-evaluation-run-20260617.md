# Ejecucion evaluacion integral del harness - 2026-06-17

Plan base: `docs/teamwork/harness-evaluation-plan.md`

## Entorno

- Repo: `/home/jostick/Desktop/personal/Projects/workspaces/HarnessDevTool`
- `HARNESS_HOME`: `/home/jostick/.harness`
- Perfil: `default`
- Backend dev: `http://127.0.0.1:58111`
- Frontend dev: `http://localhost:58551`
- CLIs detectadas por servidor: `claude`, `codex`, `cursor-agent`, `agy`

## Limpieza inicial

Antes de ejecutar la bateria se limpio el perfil activo:

- `threads/`: 0 entradas
- `sessions/`: 0 entradas
- `budgets/`: 0 entradas
- `context.sqlite`: movido fuera del perfil activo

Backup recuperable:

- `/home/jostick/.harness/cleanup-backups/20260617-073440-default-profile`

## Casos

### Caso 0: smoke de servidor y routing

Estado: completado con hallazgos.

Objetivo: comprobar que backend/frontend levantan con perfil limpio y que las
cuatro CLIs estan disponibles antes de lanzar tareas complejas.

Thread:

- `5f403b99-7211-41f5-9a0a-fdb1c08267ec`

Readiness:

- `ready_with_warnings`
- Warnings: falta auth dir de Antigravity segun readiness, aunque la CLI logro
  iniciar sesion interactiva despues; no hay budget configurado.

Sesiones creadas:

| Kind solicitado | Session id | Routing observado | Resultado |
|---|---|---|---|
| `claude` | `1dee90ab-c444-4cb4-b32c-f4dc513dbfca` | `claude -> claude` | Respondio `OK-claude` despues de enviar `\r`. |
| `codex` | `6beb8ab1-0470-4da1-8bd1-46831ae84765` | `codex -> codex` | Respondio `OK-codex` despues de enviar `\r`. |
| `cursor` | `7eafc98b-138d-4b97-a162-6e66410d05af` | `cursor -> cursor` | Quedo bloqueado en `Workspace Trust Required`; `a\r` no lo destrabo. |
| `antigravity` | `6317e344-c742-4fb2-8585-87b5ad49b45e` | `antigravity -> antigravity` | Arranco, pero derivo en tool calls y pidio permiso para `just test`, no cumplio smoke simple. |
| `zeus` | `e43a8ba6-b7a9-495f-8c7a-7e075319a18d` | `zeus -> codex` (`default_underlying`) | Respondio `OK-zeus` despues de enviar `\r`. |

Metricas resumidas tras smoke:

| Kind | Transcript events | Assistant messages | Tool calls | Nota |
|---|---:|---:|---:|---|
| Claude | 6 | 1 | 0 | Modelo reportado: `claude-sonnet-4-6`. |
| Codex | 12 | 1 | 0 | Tokens/coste no poblados. |
| Zeus/Codex | 12 | 1 | 0 | Zeus queda persistido como `kind=codex`, role `zeus-orchestrator`. |
| Cursor | 0 | 0 | 0 | Bloqueo inicial no se convierte en transcript estructurado. |
| Antigravity | 0 | 0 | 0 | Output PTY existe, pero no hay transcript estructurado ni metricas. |

Hallazgos:

- `POST /api/sessions/:sid/input` con `\n` escribe texto en el TUI, pero no
  lo envia en Claude/Codex/Zeus; fue necesario mandar `\r`.
- Cursor necesita manejo especial de primer arranque/trust de workspace.
- Antigravity necesita politica de permisos y/o modo no interactivo antes de
  considerarlo comparable para tareas autonomas.
- La metrica estructurada depende mucho de cada CLI: PTY output existe, pero
  algunas sesiones reportan cero transcript events.
- `GET /health` devuelve 404; el smoke real de backend fue `GET /api/threads`.

### Caso 1: mejora backend

Estado: pendiente.

### Caso 2: feature end-to-end

Estado: pendiente.

### Caso 3: bug hard delete

Estado: reproducido; implementacion minima aplicada.

Reproduccion usando la sesion Claude del smoke:

- Session id: `1dee90ab-c444-4cb4-b32c-f4dc513dbfca`
- Antes de borrar: directorio de sesion existe.
- `DELETE /api/sessions/:sid`: devuelve `204`.
- Despues de borrar:
  - `GET /api/sessions/:sid`: devuelve `404`.
  - `~/.harness/profiles/default/sessions/:sid` sigue existiendo.
  - Quedan 7 archivos bajo el directorio de sesion.
  - El thread sigue conteniendo referencias al session id en eventos/indices.

Conclusion: el delete actual es tombstone/kill para API y proceso, no hard
delete real. La UI no debe presentarlo como borrado definitivo hasta que exista
una ruta/accion con semantica destructiva clara.

Implementacion aplicada:

- Se mantiene `DELETE /api/sessions/:sid` como kill/tombstone compatible con la
  semantica previa.
- Se agrega `POST /api/sessions/:sid/hard-delete` como accion destructiva
  explicita.
- `Manager::hard_delete_tree` mata/remueve el arbol de sesiones y elimina los
  directorios persistentes `sessions/<sid>` de los ids afectados.
- El borrado fisico valida que cada id sea un segmento simple antes de llamar a
  `remove_dir_all`.
- Se agrego el test de regresion
  `hard_delete_tree_removes_persisted_session_dir`.

Verificacion ejecutada:

- `cargo test -p harness-session hard_delete_tree_removes_persisted_session_dir`
  -> pasa.
- `cargo check -p harness-server` -> pasa.

Riesgo restante:

- Esta iteracion minima garantiza el borrado del directorio persistente de
  sesion y sus archivos derivados locales. No reescribe logs append-only de
  threads ni purga referencias historicas dentro de `threads/:tid/events.jsonl`.
  Para borrar tambien referencias de thread/index se necesita definir una
  politica destructiva separada para historial append-only.

Intento de ejecucion via scheduler:

- Thread: `71fb6afa-b22a-4d9d-8045-0c6e9a0e7a70`
- Task: `T-0001`
- Primer intento: scheduler marco `assignment_skipped` porque no habia
  generador idle.
- Se creo `/api/agents` con `{ kind: "codex", role: "generator" }`.
- Scheduler asigno la tarea a `agent:codex-1`.
- Hallazgo: el proceso spawneado fue Claude (`agent:claude-58f23dbe`,
  session `68b1adb1-a583-4345-863d-7b5e29ae8900`) aunque la asignacion decia
  Codex.
- El worker Claude no pudo reclamar normalmente la tarea porque estaba asignada
  a `agent:codex-1`; quedo intentando `task_claim` y luego en spinner sin
  producir cambios.
- Se detuvo la sesion `68b1adb1-a583-4345-863d-7b5e29ae8900` con
  `POST /api/sessions/:sid/stop`.

Conclusion adicional: la orquestacion scheduler/registry no es aun
suficientemente agnostica; hay desalineacion entre agente asignado, CLI
spawneada e identidad MCP usada para reclamar tareas.

Intento de ejecucion directa con Codex:

- Session: `68d519ad-5e0b-492b-a54a-bd24697a2e1a`
- Routing: `codex -> codex`
- Resultado: implemento una primera version de hard delete.
- Archivos modificados:
  - `backend/crates/harness-session/src/manager.rs`
  - `backend/crates/harness-server/src/routes/sessions.rs`
- Nueva ruta backend:
  - `POST /api/sessions/:sid/hard-delete`
- Tests/checks reportados y verificados:
  - `cargo test -p harness-session hard_delete_tree_removes_persisted_session_dir`
  - `cargo check -p harness-server`

Validacion manual tras reiniciar backend:

- Backend reiniciado en `http://127.0.0.1:49303`.
- Target de prueba: session `7eafc98b-138d-4b97-a162-6e66410d05af`.
- Antes:
  - `sessions/:sid` existia.
  - Habia 2 archivos/referencias con el session id entre thread/session.
- `POST /api/sessions/:sid/hard-delete`: `204`.
- Despues:
  - `sessions/:sid` ya no existe.
  - `GET /api/sessions/:sid`: `404`.
  - Todavia queda 1 referencia en el thread, esperable porque no se reescribe
    `events.jsonl`.

Estado de aceptacion:

- Parcial. Se resolvio hard delete del directorio persistente de sesion.
- No se resolvio hard delete completo de thread/budgets/context indexes ni la
  decision de politica sobre reescritura/eliminacion de logs append-only.
- `T-0001` se pauso para detener el scheduler despues de capturar los hallazgos.

### Caso 4: refactor mantenibilidad

Estado: pendiente.

### Caso 5: flujo multiagente ChatView + QA + reviewer

Estado: pendiente.

## Evidencia

Se completara con ids de thread/session/task, metricas de `/api/sessions/:sid/metrics`,
conteos de filesystem/SQLite y observaciones de QA browser.

## Observabilidad de subagentes

La corrida confirma que cada subagente/sesion deja varias fuentes medibles:

- `~/.harness/profiles/default/sessions/:sid/meta.json`
- `~/.harness/profiles/default/sessions/:sid/output.log`
- `~/.harness/profiles/default/sessions/:sid/transcript.jsonl` cuando el parser
  de la CLI lo soporta.
- `~/.harness/profiles/default/sessions/:sid/transcript_index.sqlite`
- `~/.harness/profiles/default/threads/:tid/events.jsonl`
- `/api/sessions/:sid/metrics`
- Eventos `task.scheduler.decision`, `session.spawn.routing.resolved`,
  `session.spawn.started`, `session.capabilities.resolved`,
  `capability.decided`, `session.context.*`.

Brechas observadas:

- Cursor quedo bloqueado en `Workspace Trust Required`; habia `output.log`, pero
  no transcript estructurado ni metricas conversacionales.
- Antigravity tuvo `output.log`, pero cero transcript events y cero metricas
  estructuradas; ademas pidio permiso para `just test`.
- Scheduler asigno `T-0001` a `agent:codex-1`, pero spawneo una sesion Claude
  con `agent:claude-58f23dbe`, rompiendo la trazabilidad assignee -> CLI ->
  identidad MCP.
- Codex directo (`68d519ad-5e0b-492b-a54a-bd24697a2e1a`) si produjo metricas:
  15 tool calls, breakdown por tool, duracion por tool, 72 transcript events,
  max gap y eventos de context pressure.

## Correccion de spawn scheduler/proveedor

El resultado anterior no fue satisfactorio para probar Zeus ni combinaciones de
agentes: si el scheduler asigna `agent:codex-1` pero el spawner lanza Claude,
las metricas y claims por subagente dejan de ser confiables.

Causa raiz encontrada:

- `request_spawn` enviaba correctamente `kind = "codex"` para `agent:codex-1`.
- `ManagerSpawner::kind_for_role` ignoraba ese `kind` cuando el rol tenia
  `cli = "claude"` en el perfil.
- La configuracion MCP generaba otro `agent_id` sintetico basado en el CLI
  lanzado (`agent:claude-*` / `agent:codex-*`) en vez de usar el assignee real
  del scheduler.

Correccion aplicada:

- El `kind` concreto del scheduler ahora gana sobre el default del rol.
- El default del rol se usa solo cuando el request pide `generic`.
- El MCP del subagente recibe el `agent_id` real de `SpawnRequest`.

Validacion:

- `cargo test --manifest-path backend/Cargo.toml -p harness-server state::tests::`
  paso: 9 tests.
- `cargo check --manifest-path backend/Cargo.toml -p harness-server` paso.
- Prueba live con backend `http://127.0.0.1:51547`:
  - Thread: `b5c13d0d-3494-4ea9-a1bb-d9073d8beaec`
  - Task: `T-0001`
  - Scheduler asigno `agent:codex-1`.
  - Server log mostro spawn `kind = "codex"`.
  - MCP args incluyeron `--agent-id agent:codex-1`.
  - Timeline persistio `capability.decided` con actor `agent:codex-1`.
  - Session smoke detenida y task pausada despues de la prueba.

Decision operativa para las siguientes pruebas:

- Mantener Zeus como objetivo principal porque es donde se valida la
  combinacion real de agentes/proveedores.
- Repetir primero el flujo con `codex` puro + subagentes, porque en esta corrida
  fue el camino con mejor trazabilidad y metricas estructuradas.
- Volver a Zeus despues de esta correccion para separar fallos de orquestacion
  de fallos propios de cada proveedor.

## Retest post-correccion

Fecha: 2026-06-17.

Checks locales:

- `cargo test --manifest-path backend/Cargo.toml -p harness-server state::tests::`
  paso: 9 tests.
- `cargo check --manifest-path backend/Cargo.toml -p harness-server` paso.

Backend/frontend dev:

- Backend: `http://127.0.0.1:50234`
- Frontend: `http://localhost:43142`
- Binarios detectados: `claude`, `codex`, `cursor-agent`, `agy`.

Retest scheduler + Codex:

- Thread: `f492eea4-a2be-4867-84bb-58e18c1b63c5`
- Task: `T-0001`
- Session: `ec125cc4-e8f9-43bb-be4b-e4603a818b7c`
- Resultado:
  - Scheduler asigno `agent:codex-1`.
  - Server log mostro spawn `kind = "codex"`.
  - MCP args incluyeron `--agent-id agent:codex-1`.
  - Timeline persistio `capability.decided` con actor `agent:codex-1`.
  - `/api/sessions/:sid/metrics` respondio con `kind = "codex"` y capacidades
    `harness` + `codebase_memory`.
- Observacion:
  - Esta sesion corta no produjo transcript conversacional estructurado
    (`transcript_event_count = 0`, `tool_call_count = 0`), aunque si dejo
    eventos MCP en timeline.
- Cleanup:
  - Session stop: `204`.
  - Task pause: `200`.

Retest Zeus directo:

- Thread: `07a82dd6-adb5-4c56-ac90-8219d8b471e6`
- Session: `8af257ec-8c29-41c2-af30-e05ec2dcefd0`
- Request:
  - `kind = "zeus"`
  - `role = "planner"`
  - matrix orchestrator: `provider = "codex"`, `model = "gpt-5.5"`,
    `effort = "medium"`
- Resultado:
  - `session.spawn.routing.resolved` registro `requested_kind = "zeus"`,
    `resolved_provider = "codex"`, `underlying_cli = "codex"`,
    `source = "zeus_matrix"`, `matrix_matched = true`.
  - `session.spawn.started` registro `kind = "codex"` y
    `underlying_cli = "codex"`.
  - `/api/sessions/:sid/metrics` produjo metricas conversacionales:
    `transcript_event_count = 14`, `user_message_count = 1`,
    `assistant_message_count = 1`, `tool_result_count = 1`,
    `conversation_duration_ms = 4058`, `max_gap_ms = 2889`,
    `tool_duration_ms_by_name.spec_read.count = 1`.
- Cleanup:
  - Session stop: `204`.

Estado tras retest:

- El bug principal de routing/identidad queda corregido para Codex scheduler y
  para Zeus con matriz Codex.
- Zeus ya es medible cuando el orchestrator resuelve a Codex.
- Persisten brechas a probar despues:
  - subagentes hijos creados desde Zeus, no solo la sesion root;
  - Cursor sigue pendiente por `Workspace Trust Required`;
  - Antigravity sigue pendiente por permisos/transcript;
  - ChatView y QA browser aun no se repitieron con el routing corregido.
