---
id: build-plan/pending-implementation-tasks
title: Tareas pendientes de implementación
shard: 12-build-plan
tags: [plan, backlog, f3, f4, implementation]
summary: Backlog secuencial de tareas pendientes para ejecutar F3/F4 con cambios mínimos.
related: [build-plan/phase-3-team, build-plan/phase-4-modules, build-plan/open-questions]
sources: []
---

# Tareas pendientes de implementación

Backlog ordenado para retomar el harness tarea por tarea. Cada bloque se puede
revisar, aprobar y ejecutar sin mezclar scopes.

> **Plan maestro: Super-harness 2026-06** — ver [[build-plan/super-harness-plan-2026-06-12]]
> para la síntesis post-análisis de referencias (pi, hermes-agent) y workstreams W1–W5
> (tool loading 2.0, SSH completo, DB context pack, context engine v2, residuales perf).
> Ese documento articula las prioridades estratégicas y el cronograma estimado.

> **Análisis pendiente de convertir en tareas (2026-06-10):**
> [[build-plan/planning-codex-delegation-2026-06-10]] — auditoría de la planificación y la delegación
> a Codex (`--dangerously-bypass-approvals-and-sandbox`). **Objetivo primario (aclaraciones del
> usuario): (1) normalización Claude↔Codex** — un contrato CLI-agnóstico entregado idéntico a ambos; la
> orquestación (modo Zeus) es libre encima. **(2) Harness agnóstico al agente** — núcleo sin ramas por
> CLI (un adaptador por CLI) que trabaje bien con *cualquier* agente, no solo Claude/Codex. Hallazgo
> central: Codex corre sin sandbox y edita con sus propias tools, así que el gating runtime no lo
> constriñe → peso en contrato (M1, M12) + verificación dura (M2, M3) + aislamiento. Ejes de paridad:
> M13 (supresión tools nativas Codex), M14 (house-rules único), M15 (contención), M6 (unificar config
> MCP). Agnosticismo: M16 (`AgentAdapter` + descriptor), M17 (Cursor/Antigravity + degradación).
> Roadmap M1–M17, matriz de paridad §1·B y principio agnóstico §1·C en el doc.

## Orden recomendado

1. **Tab Agents con sesiones hijas reales** — ejecutada; corrige el bug observado y valida la base de sub-agentes.
2. **Smoke test backend de spawn child** — ejecutada; fija el contrato backend antes de extender UI.
3. **Tool MCP `task.create` con brief para orchestrator** — ejecutada; cierra el loop de creación de tasks por agentes.
4. **Validación valibot en Add DB Connection** — ejecutada; pendiente pequeño y aislado de DB.
5. **Mejorar visualización y edición de tipos especiales en DB tables** — ejecutada; fechas, bytes, boolean/null y arrays.
6. **Mejoras y bugs del DB Manager** — ejecutada; tarea creada desde la inspección de validación DB.
7. **Iconos lucide para schemas, tablas y vistas en DB** — ejecutada; mejora visual pequeña del árbol DB.
8. **Context menu avanzado para tablas/vistas DB** — ejecutada; exportar formatos y generar queries en nueva pestaña.
9. **Task A1: Readiness check + execution mode** — ejecutada; readiness cubre repo/commands/cli_auth/env/deps/ports/budget/external resources, persiste evento append-only y ajusta `execution_mode`.
10. **Task A2: Autonomy profile + approvals policy** — base ejecutada; follow-up: allowlists por project.toml/policy y selector editable en thread activo.
11. **Task A3: Team handoff schema** — base ejecutada; follow-up: enforcement obligatorio `generator -> evaluator` antes de `pending_verify`.
12. **Task A4: Repo intelligence + codebase-memory-mcp** — base ejecutada; follow-up: index orchestration/cache y wrappers profundos de grafo.
13. **Task 12: TaskBrief first-class** — ejecutada; brief estructurado (objective/context/tasks/rules/expected_result) como campo propio del Task, fuera de acceptance checks, con compat de brief string legacy. Rebaseada sobre el batch de hardening de seguridad y pusheada a main.
14. **Task 13: Separar `task.create` y `task.propose`** — ejecutada; `TaskStatus::Proposed`, `task_propose` (cualquier rol) crea en `proposed`, `task_create` con gate mínimo de rol en el dispatcher (deny FUERA de `harness-policy`, confirmado por audit: el `PolicyEngine` es ciego al rol → el middleware completo es Task 14). `role: Option<String>` hilado por dispatcher/server (default `None` permisivo; match exacto fail-closed). Transición `Proposed→Queued`; `Proposed` no reclamable ni agendable. Tipos `ts-rs` regenerados. Follow-up SSE/UI cerrado por Task 27.
15. **Task 14: Capability policy middleware mínimo** — ejecutada: matriz `capability_default` en `harness-policy`, reglas role-aware, dispatcher MCP consulta `/api/approvals/check` con `role`, offline fail-closed para tools sensibles sin rol confiable, deny claro al modelo y audit append-only `capability.decided`. Follow-ups de hardening cerrados por Task 29.
16. **Task 15: Eventos append-only unificados** — ejecutada (slice incremental backend-only): `Event` con envelope aditivo (`thread_id`/`actor`/`payload`), `seq` atómico en `append_event` (cierra Task 28), TaskEvents persistidos como envelopes vía sink server-side (MCP sink-free), `emit` best-effort, SSE intacto (cero frontend). Diferido a follow-up: broadcast en vivo de capability/handoff/readiness por SSE; envelope en el cable (opción full); endpoint/UI de replay (Task 23).
17. **Task 16: Metadata fuerte de subagentes** — ejecutada 2026-06-04: `SessionMeta` persiste `owner_session_id`, `task_id` y `scopes`; `session_spawn_child`/REST aceptan task/scopes opcionales; children/API/UI exponen metadata segura; DB agents salen con scope de conexión/base. `just gen-types`, `pnpm check` y `just test` verdes.
18. **Task 17: `spec.md` append-only con versiones** — ejecutada 2026-06-04: `spec.events.jsonl` append-only versiona cambios; `GET/PUT /spec` mantienen compat y exponen `version`; `PUT /spec/sections/:section` y MCP `spec_set_section` validan `spec_version_required`; `Task.spec_refs` permite `{ section, version }`; `spec.changed` incluye versión/sección. `just gen-types`, `pnpm check` y `just test` verdes.
19. **Task 18: Artifacts como entidad/evento real** — ejecutada; metadata recuperable para diff, logs, screenshots, endpoint de artifacts y eventos `artifact.added`.
20. **Task 19: Razones estructuradas en tasks** — ejecutada; blocked/paused/rejected/needs_human en `Notes`, eventos `task.reason.changed` y UI en `TaskDetail`.
21. **Task 20: Scheduler explain/debug** — ejecutada; `SchedulerExplanation` persistido en task, evento `task.scheduler.decision` y UI compacta de razón.
22. **Task 21: Budget por task/agente** — ejecutada 2026-06-04 en commit `e21710d`; costo por thread/session/task/role, retries con UI compacta y `max_concurrent_workers` opcional aplicado por el scheduler.
23. **Task 22: Reconciliador de estado** — ejecutada 2026-06-04; reporte por thread para inconsistencias task/session/artifact, endpoint `/api/threads/:tid/reconcile`, UI compacta y hardening T4 de sesiones detached.
24. **Task 23: Replay/debug timeline** — ejecutada 2026-06-04; timeline read-only desde `events.jsonl`, endpoint `/api/threads/:tid/timeline`, UI `/threads/:id/timeline` con filtros y payload raw.
25. **Task 24: Tipos TS generados desde Rust para tasks** — ejecutada; frontend re-exporta tipos generados y `just gen-types` cubre tasks.
26. **Task 25: E2E pequeño planner→worker→evaluator** — ejecutada; test de scheduler cubre planner/generator/evaluator con handoff y unblock de dependencias.
27. **Task 26: Árbol aislado de sesiones y mailbox de subagentes** — ejecutada; sesiones hijas multi-nivel y mailbox append-only.
28. **Task 9: Agente DB para conexión activa** — agente especializado con acceso controlado a la BD, backups y puente con Agents.
29. **Task 10: Esqueleto mínimo del módulo SSH** — ejecutada parcialmente y usable: crate `module-ssh`, REST, MCP `ssh_exec`/`sftp_list`/`sftp_get`/`sftp_put`/`sftp_mkdir`/`sftp_rmdir`/`sftp_unlink`/`sftp_rename`, UI `/ssh` y `/ssh/[host]`; pendiente transfer queue con resume/progreso, known_hosts fuerte, sesiones SSH interactivas e implementación pure Rust `russh`.
30. **Task 11: Botón `+ task` en tab Tasks** — mejora secundaria para control manual.
31. **Task 27: Broadcast SSE + UI de propuestas** — ejecutada 2026-06-04: `POST /api/threads/:tid/tasks` acepta `status=proposed`, `task_propose` delega al REST cuando hay `server_url` para disparar `task.created`/SSE del server, y la UI lista/filtra `proposed` con promoción humana a `queued` o `blocked` según dependencias.
32. **Task 28: `seq` atómico en `append_event`** — ejecutada (absorbida por Task 15): `append_event` asigna `seq` contando records bajo el `write_lock` y lo retorna; los 3 callers (approvals/tasks/threads) dejaron de pre-calcular con `read_events().len()`. Test de append concurrente verde.
33. **Task 29: Hardening de capability policy** — ejecutada 2026-06-04: root spawn rechaza roles desconocidos contra `RolesRegistry`; `remembered_rule` persiste el rol de `ApprovalSummary`; offline sin rol o con rol desconocido niega tools sensibles y conserva read-only.
34. **Task 30: gitignore de `backend/crates/*/bindings/`** — ejecutada 2026-06-04: `.gitignore` cubre outputs crate-locales de `ts-rs`, `ReadinessReport.facts` exporta `unknown` desde Rust en vez de importar `JsonValue`, y el gate frontend queda en `pnpm check`.
35. **Task 31: Medición de eficiencia de tools por spawn** — ejecutada; `capability_profile=auto` se mantiene como default, UI expone `none` como modo liviano, `repo_find` queda como rail determinístico de búsqueda y el analizador reporta calidad básica (`completion_marker_rate`, `active_tool_work_rate`, `quality_pass_rate`).
36. **Task 32: Reemplazar `pdftotext` por `pdf_oxide` embebido** — ejecutada; knowledge PDF usa `pdf_oxide` pure Rust sin subprocess ni Poppler, y `pdf_oxide_mcp` queda disponible como MCP stdio opcional en `docker-compose.mcp.yml`.
37. **Task 33: Capacidad `docs.build` con Starlight como backend default** — ejecutada; `docs_build` genera scaffold/copia Markdown para Starlight/mdBook/VitePress, auto-selecciona mdBook solo para repos Rust puros y corre build cuando las deps locales están disponibles.
38. **Task 34: Project Memory Binding** — base ejecutada; el harness detecta/indexa repos por profile, enlaza threads/sesiones, expone continuity compacta en `repos/current`, inyecta project context controlable al spawn y la UI permite resume/context/fresh. Follow-up: endpoint/write explícito para `.harness/project.toml` y continuity más profunda de archivos tocados.

## Tareas abiertas 2026-06-10 (post-revisión de Codex)

> Fuente: [[build-plan/harness-analysis-2026-06-10]] (análisis + revisión del fuente de `openai/codex`).
> **Estado 2026-06-10:** H1+H2 ✅ hechos (commit `13ae622`); **H3 ❌ descartado** (el usuario usa Codex Pro,
> costo plano — **no se mide USD**, ver SCOREBOARD nota 5); **primer bug P1 (gateway timeout) ✅ cerrado** por
> el head-to-head codex-vs-sonnet. Resto awaiting aprobación. Secuencia restante: H4 → H5 → H6 → seguridad → resto bugs P1 → perf.

- **H1: Fix invocación headless de Codex** — ✅ HECHO (validado en vivo, `HEADLESS_OK` en 4 s; commit `13ae622`): prompt posicional + `< /dev/null` (+ `--json`/`--ephemeral`). Causa raíz: `read_to_end()` de stdin no-TTY sin EOF (`codex-rs/exec/src/lib.rs:1858-1868`).
- **H2: Corregir receta de Codex en CLAUDE.md §3** — ✅ HECHO (commit `13ae622`): `codex exec "PROMPT" --json --skip-git-repo-check -c sandbox_mode=workspace-write < /dev/null`.
- **H3: ~~Reporter de costo de Codex~~** — ❌ DESCARTADO. El usuario usa el CLI de Codex con suscripción Pro (costo plano); no interesa medir USD. El rollout JSONL de Codex se reutiliza para señales de **performance** (turnos/tool-calls/duración), no dólares.
- **H4: `SessionResult` por sesión** — `{ success, quality_score, turns, wall_seconds }` persistido (**sin** `cost_usd`). `wall_seconds` de started_at→exit; `turns`/tool-calls del `--json` de Codex y del transcript de Claude. Esfuerzo S/M.
- **H5: Evaluador post-sesión** — subagente Sonnet-juez con rúbrica que puntúa éxito/calidad post-sesión y persiste en `SessionResult`. Esfuerzo M.
- **H6: Harness de head-to-head** — misma tarea por dos ejecutores en worktrees aislados + mismo revisor/QA + fila al SCOREBOARD (como el de gateway-timeout, ya registrado). Acumular muestras para decidir roster por calidad/perf.
- **Seguridad de delegación (M1/M2/M3) y normalización (M5/M11–M17)**: ver [[build-plan/planning-codex-delegation-2026-06-10]].
- **Bugs P1 verificados**: ✅ gateway MCP sin timeout — **cerrado 2026-06-10** (head-to-head; merge de la impl de Sonnet + el assert de reaping de Codex en `gateway.rs`, 69 tests). Pendientes: policy-check 120s→5-10s (`dispatcher.rs:486`); fuga `drop_lease_async` (`module-db/lease.rs:217-226`); `ensure_thread` con mutex (`harness-core/tasks/store.rs:92-144`).
- **Perf quick wins**: consolidar polling frontend (`+page.svelte` + `IconRail.svelte`); `mem::take()` en `flush_chunk` (`harness-session/session.rs:665-682`).

## Experimentos activos

- **Slint desktop Agents spike** — creado en `experiments/slint-agents`; app
  desktop nativa que consume `GET /api/threads` y renderiza una vista global de
  Agents fuera de SvelteKit. Decisión 2026-06-08: el track desktop corre en
  paralelo y no modifica la web UI; la UI SvelteKit actual es referencia
  funcional. Slint queda como candidato performance-first y debe compararse con
  un spike Tauri baseline antes de cerrar tecnología.

Nota F3 2026-06-04:
- El routing base rol → CLI quedó cerrado para el scheduler: `Role.cli`
  fuerza Claude/Codex, `generic` conserva el kind pedido y Zeus se resuelve a
  su CLI subyacente antes de spawnear.
- El selector del scheduler cae a Claude cuando el CLI primario no tiene
  binario detectado y registra `scheduler.spawn.fallback` append-only con
  `reason=binary_missing`.
- `budget.v1.json` agregado en `harness-core/schemas` y cubierto por test de
  smoke contra los campos persistidos.
- Dashboard F3: `/threads/:id/tasks` expone control Pause/Resume conectado al
  kill-switch global del scheduler.
- Handoff de cierre temporal: los commits hasta `2d56e64` quedaron subidos a
  `main`. `harness-sandbox` existe con niveles `none|workspace|workspace-net|strict`
  y warning best-effort; `module-ssh` usa ese perfil para comandos directos
  `ssh`/`scp`. Verificado con `cargo test -p module-ssh`,
  `cargo check --workspace` y `git diff --check`.
- Al retomar F3, no implementar sandbox duplicado sobre `shell.exec` de los CLIs:
  N3 dice confiar en el sandbox de `claude`/`codex`/`cursor`/`agy` y envolver solo
  procesos directos del bridge. Slice posterior: checklist F3 ajustado a N3 y
  padres de fallback `quota_exceeded`/`runtime_error` cerrados; la cobertura
  incluye clasificación, selección de Claude, evento append-only de runtime y
  acceptance sintético con audit de quota.
- Handoff sandbox posterior: `harness-sandbox` ahora genera perfiles
  `sandbox-exec` y `SandboxCommand` envuelve comandos en macOS; `module-ssh`
  construye `ssh`/`scp` con `SandboxCommand`. Pendiente para F3: Linux
  `seccompiler` + bind mounts sigue abierto.

## A1. Readiness check + execution mode

Objetivo:
Detectar bloqueos de entorno antes de gastar tokens y elegir el flujo correcto
para trabajos cortos o largos.

Contexto:
Ver [[agents/autonomy-protocol]]. Debe vivir en backend/core, exponerse por API
y quedar persistido como evento append-only por thread.

Tarea:
1. Agregar modelo `ReadinessReport` con status `ready|ready_with_warnings|blocked`.
2. Implementar checks basicos: repo, commands, cli_auth, env, budget.
3. Persistir evento `thread.readiness.checked`.
4. Agregar `execution_mode` en metadata del thread.
5. Exponer endpoint para recalcular readiness.
6. UI muestra banner compacto con blockers/warnings.

Resultado esperado:
Al crear un thread, el harness sabe si puede trabajar, que falta y si el request
debe ir por `quick`, `standard`, `project`, `exploratory` o `blocked`.

Estado implementado:
- `ReadinessReport` persistido en `readiness.json`.
- Eventos `thread.readiness.checked`.
- Endpoint `GET/POST /api/threads/:id/readiness`.
- Banner UI en Dashboard para el thread seleccionado.
- Checks iniciales: repo, commands, cli_auth, env.
- `execution_mode` persistido en `meta.json`.

## A2. Autonomy profile + approvals policy

Objetivo:
Permitir ejecucion sin interrupciones cuando el usuario lo habilita, sin perder
controles de seguridad por defecto.

Contexto:
Config `autonomy_profile` y approval flow. Perfiles: `manual`, `assisted`,
`autonomous`, `ci`.

Tarea:
1. Agregar config resuelta por profile/project/thread.
2. Mapear autonomy profile a approval behavior.
3. Hacer que `ci` falle con error estructurado en vez de esperar input humano.
4. Hacer que `autonomous` respete allowlists de project.toml/policy.
5. UI selector por thread con descripcion corta.

Resultado esperado:
El usuario puede escoger cuanto permiso tiene el equipo antes de iniciar el
trabajo y el scheduler/bridge actuan de forma consistente.

Estado implementado:
- `AutonomyProfile` persistido en `meta.json`.
- Default backend via `HARNESS_AUTONOMY_PROFILE` con fallback `assisted`.
- Selector en New Session.
- Endpoint `POST /api/threads/:id/autonomy`.
- Approval check auto-resuelve `Ask -> Allow` para threads `autonomous` y `ci`.

## A3. Team handoff schema

Objetivo:
Hacer comunicacion entre agentes eficiente, auditable y accionable.

Contexto:
Los agentes no hablan entre ellos en vivo; usan task notes, artifacts y eventos.

Tarea:
1. Definir schema `handoff.v1.json`.
2. Agregar evento `handoff.created`.
3. Exigir handoff `generator -> evaluator` antes de `pending_verify`.
4. Exigir handoff `evaluator -> generator` en `verify-fail`.
5. Mostrar handoffs en TaskDetail.

Resultado esperado:
QA recibe evidencia clara, feedback vuelve accionable y los threads largos se
pueden resumir sin releer todo el PTY.

Estado implementado:
- Schema `handoff.v1.json`.
- Handoffs append-only por task en `handoffs/<task>.jsonl`.
- Evento `handoff.created` en `events.jsonl`.
- Endpoint `GET/POST /api/threads/:tid/tasks/:task_id/handoffs`.
- TaskDetail muestra handoffs existentes.

## A4. Repo intelligence + codebase-memory-mcp

Objetivo:
Dar al planner y workers una vista estructural del repo antes de explorar
archivo por archivo, y usar `codebase-memory-mcp` como acelerador opcional de
grafo/call-chain cuando esté instalado.

Contexto:
El catálogo de rails prometía `repo.*`, pero el bridge real no las exponía.
Esta tarea cierra el primer corte: rails determinísticas propias y detección de
`codebase-memory-mcp`.

Tarea:
1. Pasar el `cwd` de cada sesión al `harness-mcp-server`.
2. Exponer MCP tools `repo_analyze`, `repo_scan`, `repo_read_file`,
   `repo_git_status`, `repo_git_log`, `repo_git_diff`,
   `repo_codebase_memory_status`.
3. Hacer que las tools rechacen paths fuera del workspace.
4. Añadir `codebase-memory-mcp` al readiness report como acelerador opcional.
5. Cambiar approval check del MCP a fail-closed cuando el server responde mal.
6. Briefing del harness indica usar `repo_analyze` en repos desconocidos.

Resultado esperado:
El agente puede entender stack, scripts, archivos clave, git state y estructura
básica del repo por rails tipadas antes de leer archivos manualmente.

Estado implementado:
- Rails `repo_*` en `harness-mcp-server`.
- `codebase-memory-mcp` visible en readiness y `repo_codebase_memory_status`.
- Path safety para lectura/scan.
- Policy check fail-closed si `/api/approvals/check` falla o responde inválido.

Follow-up:
- Orquestar instalación/configuración de `codebase-memory-mcp` desde el harness.
- Cachear índice por repo/HEAD.
- Wrappers profundos sobre grafo: symbols, callers, callees, routes,
  blast_radius.
- Generar `ARCHITECTURE.md` desde `repo_analyze` + grafo.

## 1. Tab Agents con sesiones hijas reales

Objetivo:
Mostrar en vivo las sesiones hijas/sub-agentes de una sesión padre.

Contexto:
Frontend `SessionRightPanel.svelte`. Backend `routes/sessions.rs`.
Existe metadata `parent_session_id` / `root_session_id` y una ruta de hijos que
hay que auditar antes de tocar UI.

Tarea:
1. Auditar qué devuelve `GET /api/sessions/:id/children`.
2. Conectar el tab Agents a sesiones hijas reales.
3. Refrescar con el patrón existente de polling/store del panel.
4. Mostrar estados `running`, `exited` y `killed` con estilo consistente.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
Cuando una sesión spawnea un sub-agente, el tab Agents lo muestra sin refrescar
la página y permite abrir la sesión hija.

## 2. Smoke test backend de spawn child

Objetivo:
Fijar por test que una sesión hija queda enlazada correctamente a su padre.

Contexto:
Backend `routes/sessions.rs` y MCP/session tools en `harness-mcp-server/src/tools/session.rs`.
Este test protege el contrato que consume el tab Agents.

Tarea:
1. Identificar o crear el punto de test para sesiones.
2. Crear una sesión padre y una hija con `parent_session_id`.
3. Verificar `parent_session_id`, `root_session_id` y listado de children.
4. Cubrir hija activa y finalizada si el harness de test lo permite.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
`GET /api/sessions/:id/children` devuelve hijas correctas y estables para la UI.

## 3. Tool MCP `task.create` con brief para orchestrator

Objetivo:
Permitir que una sesión/orchestrator cree tasks vía MCP usando el formato
estándar de brief.

Contexto:
Backend MCP `harness-mcp-server/src/tools/tasks.rs`.
Core task store `harness-core/src/tasks/store.rs`.
F3 permite creación directa por planner/orchestrator; workers usan propuestas después.

Tarea:
1. Auditar tools MCP actuales de tasks y sus tests.
2. Analizar la implementación actual del harness para adaptar el formato de brief
   a tasks, memoria y continuidad entre sesiones sin migraciones grandes.
3. Agregar soporte de `brief` en `task_create` usando el store existente.
4. Convertir el brief al formato textual estándar y persistirlo de forma recuperable.
5. Respetar validaciones y state machine actuales.
6. Persistir/emitir eventos con el flujo existente para que SSE/UI lo vea.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
Un agente autorizado llama `task_create` con `brief`; la task queda persistida,
la UI la refleja por el flujo normal y un worker puede recuperar el contrato
con `task_get`.

## 4. Validación valibot en Add DB Connection

Objetivo:
Cerrar el pendiente menor del módulo DB validando el formulario de conexión.

Contexto:
Frontend `ConnectionFormDialog.svelte` y `api/schemas/db.ts`.
SQL ya está operativo; falta validación cliente para entradas inválidas.

Tarea:
1. Revisar campos actuales del dialog y shape esperado por el API.
2. Analizar e inspeccionar el gestor de BD actual en busca de bugs, deuda y
   posibles mejoras; crear una tarea separada con esos hallazgos antes de
   implementar cambios fuera de la validación.
3. Crear o extender un schema valibot para URL, engine y opciones.
4. Mostrar errores por campo sin cambiar el flujo exitoso.
5. Mantener compatibilidad con conexiones SQLite locales.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
El formulario rechaza datos inválidos antes de llamar al backend y conserva el
flujo actual para conexiones válidas.

## 5. Mejorar visualización y edición de tipos especiales en DB tables

Objetivo:
Mejorar cómo el gestor de BD muestra y edita valores especiales en las tablas.

Contexto:
Frontend `/db/[id]`, `ResultGrid.svelte`, `RowEditorPanel.svelte` y helpers de
serialización/edición de valores.
Backend `module-db` devuelve valores tipados como JSON, por ejemplo:
`{ "_t": "date_time", "v": "2025-06-27T15:26:02.651197" }`,
`{ "_t": "bytes", "v": "QUy2uHsMT8T+L68+YobBso4ZZOEhpXLzlzlU/XfMJW0dOCOhUvzFP9P6auyaL/85" }`
y actualmente algunos arrays aparecen como `<unsupported:TEXT[]>`.

Tarea:
1. Auditar cómo `ResultGrid` y `RowEditorPanel` renderizan valores tipados (`date_time`, `bytes`, boolean, null, arrays).
2. Mostrar fechas de forma legible en celdas, conservando el valor original para edición/envío.
3. Mostrar bytes como valor compacto con affordance de inspección/copia, evitando pintar el base64 completo por defecto.
4. Cambiar la edición inline de booleanos a selector `TRUE` / `FALSE`; si la columna acepta `NULL`, incluir opción `NULL`.
5. Mejorar visualización de arrays (`TEXT[]` y equivalentes) para no mostrar `<unsupported:...>` cuando se pueda representar como lista/JSON editable.
6. Agregar tests o checks focalizados para los helpers de render/parse si existen; si no, cubrir con el test disponible del frontend.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
Las tablas DB muestran fechas, bytes, booleanos/null y arrays de forma legible;
la edición inline de booleanos usa selector seguro; y los arrays dejan de verse
como `<unsupported:TEXT[]>` cuando el backend provee datos representables.

## 6. Mejoras y bugs del DB Manager

Objetivo:
Resolver bugs y mejoras detectadas durante la inspección del gestor de BD.

Contexto:
Frontend `/db`, `/db/[id]`, `ConnectionFormDialog.svelte`, `dbStore`.
Backend `module-db` y `routes/db.rs`.
No mezclar con la validación valibot; ejecutar como tarea aparte.

Tarea:
1. Mostrar errores inline para todos los campos validados, no solo name/database/host/params.
2. Revisar UX de password en edición: aclarar que vacío conserva el password actual.
3. Revisar validación backend de `ConnectionInput`: hoy solo valida name/database.
4. Revisar si el selector de SQLite debería tener picker/path helper o mejor copy clara.
5. Auditar estados de query larga/cancelación para asegurar feedback consistente en UI.
6. Auditar export filename parsing y errores de export para mejorar mensajes.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
El DB Manager queda con validaciones y mensajes más consistentes, y los bugs
detectados se cierran sin cambiar el alcance funcional del módulo.

## 7. Iconos lucide para schemas, tablas y vistas en DB

Objetivo:
Mejorar visualmente la representación de schemas, tablas y vistas en el árbol
del gestor de BD usando iconos adecuados de `lucide-svelte`.

Contexto:
Frontend `/db/[id]`, componente `frontend/src/lib/components/db/SchemaTree.svelte`
y re-export central `frontend/src/lib/icons.ts`.
El árbol actualmente usa símbolos manuales para tablas/vistas y texto plano para
schemas. El proyecto ya importa iconos desde `$lib/icons`, que re-exporta
`lucide-svelte`.

Tarea:
1. Auditar cómo `SchemaTree.svelte` representa schemas, tablas y vistas hoy.
2. Seleccionar iconos lucide consistentes para schema/database, table, view y
   materialized view si aplica.
3. Agregar los iconos necesarios al re-export central `$lib/icons` si no existen.
4. Reemplazar símbolos manuales por iconos lucide manteniendo tamaño, color,
   alineación, estado activo y hover actuales.
5. Verificar que el árbol siga siendo legible con filtros, schemas colapsados y
   tablas con `row_estimate`.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
En el gestor de BD, schemas, tablas y vistas se distinguen visualmente con
iconos lucide consistentes, sin cambiar el comportamiento de navegación,
filtro, menú contextual ni apertura de tablas.

## 8. Context menu avanzado para tablas/vistas DB

Objetivo:
Agregar un context menu para tablas y vistas que permita exportar datos en varios
formatos y generar queries base en una pestaña SQL nueva.

Contexto:
Frontend `/db/[id]`, `SchemaTree.svelte`, `ExportDialog.svelte`, `dbStore` y
tabs SQL/table del workspace DB.
Backend `module-db` y rutas `/api/db/*` ya tienen export parcial para JSON, CSV
y SQL inserts; XLSX y Markdown pueden requerir ampliar contrato o implementar
generación frontend según alcance.
El menú contextual actual solo expone export básico para schema/table.

Tarea:
1. Auditar el context menu actual de `SchemaTree.svelte` y el flujo existente de
   `ExportDialog`.
2. Definir acciones para tablas y vistas: exportar `JSON`, `CSV`, `XLSX` y
   `Markdown`.
3. Definir acciones para generar queries `SELECT`, `INSERT`, `UPDATE` y `DELETE`
   usando metadata de columnas y primary keys cuando existan.
4. Al generar una query, abrir una pestaña SQL nueva con el texto preparado para
   copiar o ejecutar, sin modificar datos automáticamente.
5. Validar restricciones por tipo: vistas pueden generar `SELECT` y exportar,
   pero `INSERT`/`UPDATE`/`DELETE` deben ocultarse o quedar deshabilitados si no
   son seguros.
6. Ampliar export backend o helper frontend solo lo mínimo necesario para soportar
   los formatos faltantes.
7. Agregar tests/checks para generación de queries y validación de formatos.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
Al hacer click derecho sobre una tabla o vista, el usuario puede exportarla en
`JSON`, `CSV`, `XLSX` o `Markdown`; también puede generar queries base que se
abren en una nueva pestaña SQL listas para copiar, revisar o ejecutar.

## 9. Agente DB para conexión activa

Objetivo:
Inicializar un agente especializado dentro del gestor DB para la conexión activa,
capaz de consultar, analizar, documentar y asistir con cambios de base de datos
con permisos controlados y coordinación con los agentes del panel Agents.

Contexto:
Frontend DB `/db/[id]`, panel Agents (`SessionRightPanel.svelte`), backend
`harness-server`, `module-db`, sesiones/PTY de agentes y MCP tools.
La conexión activa ya existe en el workspace DB. El agente debe partir en modo
solo lectura y no debe ejecutar modificaciones sin solicitud explícita,
backup previo y trazabilidad. Respetar append-only, `X-Protocol-Version` y tipos
generados desde Rust cuando haya contrato compartido.

Tarea:
1. Auditar la implementación actual de sesiones/agentes, MCP tools y DB Manager
   para definir el mínimo contrato entre un agente DB y la conexión activa.
2. Agregar un botón en el gestor DB para iniciar un agente asociado a la conexión
   y base de datos actualmente seleccionadas.
3. Crear el contexto inicial del agente con metadata de conexión segura, schema
   introspectado, restricciones de permisos y modo inicial de solo lectura.
4. Exponer tools DB controladas para el agente: listar schema, ejecutar queries
   de lectura, documentar estructura y proponer acciones sin modificar.
5. Diseñar el flujo de elevación para escrituras: el agente solo puede modificar
   cuando el usuario lo solicita explícitamente y el sistema valida que no está
   en modo solo lectura.
6. Antes de cualquier modificación, ejecutar backup obligatorio. Crear un helper
   Rust reutilizable para backup por engine (`sqlite`, `postgres`, `mysql`) o
   una estrategia equivalente mínima y testeable.
7. Persistir la documentación/análisis del agente DB como contexto recuperable
   para la sesión y visible/usable por el harness.
8. Definir el puente de comunicación entre el agente DB y los agentes del panel
   Agents: compartir hallazgos, schema docs, riesgos y propuestas de migración
   sin romper el modelo append-only.
9. Para cambios de estructura o código, priorizar que el agente DB proponga una
   migración o task al agente de coding en vez de modificar directamente cuando
   el contexto sea desarrollo.
10. Agregar tests backend para permisos read-only, bloqueo de escrituras sin
    backup, creación de backup y contrato de contexto compartido.
11. Agregar checks frontend para el botón/estado de agente DB y probar el flujo
    completo con una conexión SQLite local.

Reglas:
- No romper.
- Seguir arquitectura existente.
- Mantener modo inicial solo lectura.
- No exponer secretos de conexión al frontend ni al log de conversación.
- Agregar test y probar.

Resultado esperado:
Desde el gestor DB puedo iniciar un agente ligado a la conexión activa. El
agente puede responder preguntas sobre la BD, ejecutar consultas, analizar
estado, documentar estructura, proponer mejoras y coordinar información con el
panel Agents. No modifica datos ni schema salvo solicitud explícita; antes de
cualquier modificación crea backup obligatorio y deja trazabilidad. Cuando el
contexto sea desarrollo, prefiere proponer una migración/task para un agente de
coding antes de tocar la BD directamente.

## 10. Esqueleto mínimo del módulo SSH

Objetivo:
Arrancar el SSH Manager con un slice mínimo y usable.

Contexto:
El slice inicial ya existe y es usable. `module-ssh` está integrado al workspace,
`IconRail` habilita SSH, hay rutas REST/MCP para hosts, `ssh.exec`, listado SFTP,
transferencias básicas y mutaciones remotas. Diseño objetivo completo en
[[build-plan/phase-4-modules]].

Estado ejecutado:
1. Crate `module-ssh` con storage privado de hosts y password auth redacted en REST.
2. Endpoints REST para `host.list/add/remove/test`, `ssh.exec`, `sftp.list`,
   `sftp.get`, `sftp.put`, `sftp.mkdir`, `sftp.rmdir`, `sftp.unlink` y
   `sftp.rename`.
3. Tools MCP equivalentes para exec, listado, transferencias básicas y mutaciones
   remotas, con policy sensible para escrituras/exec.
4. Frontend `/ssh` y `/ssh/[host]` con listado de hosts, test/delete, navegador
   remoto, upload/download y acciones mkdir/rename/rmdir/unlink.

Pendiente:
1. Cola de transferencias con progreso, pause/resume/cancel y resume real.
2. `known_hosts` fuerte y bloqueo claro ante cambio de host key.
3. Identidades/keyring/passphrase.
4. Sesión SSH interactiva.
5. Reemplazar el cliente `ssh`/`scp` del sistema por implementación pure Rust
   `russh`/`russh-sftp` cuando los conflictos de compilación estén resueltos.
6. Mantener las invocaciones directas SSH bajo `harness-sandbox`; desde `2d56e64`
   `module-ssh` ya aplica perfil `workspace` antes de ejecutar `ssh`/`scp`.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
Se pueden guardar hosts SSH, listarlos, probar conexión, ejecutar comandos no
interactivos, listar directorios remotos, subir/bajar archivos pequeños y crear,
renombrar o borrar paths remotos. Smoke ejecutado 2026-06-04 contra host real vía
REST con cleanup final `cleanup-ok`.

## 11. Botón `+ task` en tab Tasks

Objetivo:
Permitir crear una task manual desde el tab Tasks del panel derecho.

Contexto:
Frontend `SessionRightPanel.svelte` y `stores/tasks.svelte.ts`.
Backend REST de tasks en `routes/tasks.rs`.
Mejora secundaria: no bloquea la autonomía de agentes.

Tarea:
1. Agregar un botón pequeño `+ task` en el tab Tasks.
2. Reusar el endpoint REST existente de creación de task.
3. Crear la task con autor humano usando el shape actual del API.
4. Refrescar el listado/store para que aparezca inmediatamente.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
Desde una sesión abierta se crea una task asociada al thread y se ve
inmediatamente en el panel.

## 12. TaskBrief first-class

Objetivo:
Convertir el brief de una task en un campo estructurado de primer nivel en vez
de guardarlo como `acceptance.checks[BRIEF]`.

Contexto:
Hoy `task_create` acepta `brief`, lo renderiza como texto y lo persiste dentro
de acceptance. Eso permite recuperar el contrato, pero mezcla instrucciones de
trabajo con checks verificables.

Tarea:
1. Agregar un tipo Rust `TaskBrief` con `objective`, `context`, `steps`,
   `rules`, `expected_result`, `write_paths`, `forbidden_paths`, `risks` y
   `test_plan` opcionales según alcance.
2. Agregar `brief: Option<TaskBrief>` al modelo `Task`, schema JSON y create
   request REST/MCP.
3. Migrar `task_create` para persistir `brief` como campo propio y mantener
   compatibilidad con tasks antiguas que tengan `acceptance.checks[BRIEF]`.
4. Actualizar frontend para mostrar el brief separado de acceptance checks.
5. Regenerar/actualizar tipos compartidos siguiendo `ts-rs` como fuente de verdad.
6. Agregar tests de create/get/list para brief estructurado y compatibilidad legacy.

Reglas:
- No romper tasks existentes.
- No duplicar el brief en acceptance salvo fallback temporal explícito.
- Mantener `acceptance.checks` para criterios verificables.
- Agregar test.

Resultado esperado:
Una task expone su contrato de trabajo en `task.brief`, mientras los acceptance
checks quedan limpios para verificación.

## 13. Separar `task.create` y `task.propose`

Objetivo:
Evitar que cualquier worker pueda abrir scope real creando tasks directamente.

Contexto:
`task_create` ya existe y funciona. Para F3, la regla de producto es que
planner/orchestrator y humano crean tasks reales; workers solo proponen trabajo
descubierto para que el planner lo acepte o descarte.

Tarea:
1. Agregar modelo y storage append-only para `TaskProposal`.
2. Exponer MCP `task_propose` con `{ parent_task_id, discovered_by_role,
   rationale, suggested_title, suggested_acceptance_criteria }`.
3. Aplicar capability policy: workers no pueden `task_create`; sí pueden
   `task_propose`.
4. Agregar endpoints/listado mínimo para que planner/humano revisen propuestas.
5. Permitir promover una propuesta a task real preservando trazabilidad.
6. Agregar tests de allow/deny por rol y promoción de propuesta.

Reglas:
- No confiar en prompts para enforcement.
- Mantener append-only: aceptar/rechazar/promover son eventos nuevos.
- Agregar test.

Resultado esperado:
Los workers pueden descubrir trabajo sin expandir el scope por su cuenta; el
planner/humano decide qué se convierte en task real.

## 14. Capability policy middleware mínimo

Objetivo:
Crear enforcement real para permisos de tools MCP según actor, rol, recurso y
scope.

Contexto:
La matriz roles × tools ya está decidida en docs, pero debe vivir en el bridge
como middleware, no como convención.

Tarea:
1. Definir `CapabilityCheck { actor_id, actor_role, tool, resource, scope,
   thread_id, task_id }`.
2. Cargar una policy mínima desde `capability-policy.yaml` o defaults builtin.
3. Envolver cada handler MCP con `check_capability`.
4. Devolver `permission_denied` claro al CLI hijo cuando no tenga permiso.
5. Emitir audit event para cada allow/deny.
6. Cubrir invariantes críticas con tests de integración.

Reglas:
- Deny debe ser explícito y recuperable por el modelo.
- No romper tools existentes para sesiones humanas autorizadas.
- Agregar test.

Resultado esperado:
Las tools dejan de depender de instrucciones blandas; el harness bloquea
acciones fuera de rol/scope.

Estado implementado:
- `harness-policy` define la matriz builtin `capability_default`, evalua reglas
  por `tool` + `args` + `role` y pide approval para tools sensibles sin regla.
- El dispatcher MCP envuelve tools con `check_tool_policy`; online delega a
  `/api/approvals/check`, offline aplica la matriz local y niega tools
  sensibles cuando el rol falta o no es confiable.
- El MCP offline carga `~/.harness/profiles/<p>/policy.toml` al boot: reglas
  explícitas locales pueden permitir/denegar tools; `ask` falla cerrado sin
  server de approvals y policy corrupta bloquea tools sensibles.
- Invariantes bridge ampliadas 2026-06-04: worker no `task_create`,
  worker/generator no `spec_write`/`spec_set_section`, planner no
  `task_claim`, evaluator niega tools sensibles, repo write exige scope/path y
  policy local offline respeta allow/deny/ask.
- El server persiste decisiones como evento append-only `capability.decided`
  y escribe audit bridge en `$HARNESS_HOME/.runtime/audit/bridge.jsonl` para
  cada `allow`/`deny` resuelto por `/api/approvals/check`, con actor, rol,
  tool, recurso, decisión, razón y hashes. Preserva el `role` al recordar
  approvals. El archivo activo rota a zstd cuando crece.
- Task 29 cerró los hardenings posteriores: root spawn valida roles conocidos,
  `remembered_rule` conserva rol y el modo offline sin rol/desconocido niega
  tools sensibles.
- Slice `repo_write_file` path-gated cerrado 2026-06-04: `Task` persiste
  `write_paths` / `forbidden_paths`, los spawns MCP reciben `--task-id` y
  `--scope task:<id>` confiables, y la tool falla cerrado fuera del allowlist.
- Verificado con `cargo test -p harness-policy`,
  `cargo test -p harness-mcp-server` y `cargo test -p harness-server`.

## 15. Eventos append-only unificados

Objetivo:
Normalizar eventos de tasks, agentes, specs, artifacts, budget y audit para
replay, UI y debugging.

Contexto:
El repo ya tiene logs append-only, task history, SSE y transcript logs. Falta
una semántica común para eventos de dominio.

Tarea:
1. Definir un envelope común `{ seq, at, kind, thread_id, actor, payload }`.
2. Mapear eventos existentes: `task.created`, `task.claimed`, `task.submitted`,
   `task.verified`, `agent.spawned`, `agent.exited`, `spec.changed`,
   `artifact.added`, `capability.denied`, `budget.warning`.
3. Persistir eventos append-only por thread y emitirlos vía SSE.
4. Mantener compatibilidad con los stores actuales durante la transición.
5. Agregar tests de orden, append-only y replay básico.

Reglas:
- Nunca reescribir eventos existentes.
- Evitar migración grande si un adapter incremental basta.
- Agregar test.

Resultado esperado:
La UI y los agentes pueden reconstruir qué pasó en un thread desde un stream
ordenado y auditable.

## 16. Metadata fuerte de subagentes

Objetivo:
Hacer que cada subagente sea atribuible a un thread, task, rol, padre/root y
scope autorizado.

Estado implementado:
- `SessionMeta` agrega metadata compatible para ownership (`owner_session_id`),
  task asignada (`task_id`) y scopes.
- `SpawnArgs`/`SpawnOpts`/`AgentSession` persisten la metadata en `meta.json`.
- `session_spawn_child` y `POST /api/sessions/:sid/children` aceptan `task_id`
  y `scopes` opcionales; el parent queda como owner por defecto.
- `ChildSummary` y el tab Agents muestran task/scopes cuando existen.
- DB agents se inicializan con scopes `db:connection:<id>` y
  `db:database:<name>` cuando aplica.
- Verificado con `just gen-types`, `pnpm check` y `just test`.

Contexto:
Las sesiones hijas existen, pero para operar equipos hace falta explicar qué
hace cada proceso y bajo qué permisos.

Tarea:
1. Extender `SessionMeta` con `thread_id`, `task_id`, `role`, `spawned_by`,
   `parent_session_id`, `root_session_id`, `allowed_tools`, `write_paths` y
   `forbidden_paths`.
2. Asegurar que `session_spawn_child` rellene esta metadata.
3. Exponer la metadata en REST/SSE para el tab Agents.
4. Mostrar en UI rol, task asociada y estado sin exponer secretos.
5. Agregar tests de herencia parent/root y persistencia tras restart.

Reglas:
- No romper sesiones existentes sin metadata nueva.
- No exponer prompts secretos ni tokens.
- Agregar test.

Resultado esperado:
El tab Agents muestra no solo procesos, sino ownership real: quién lo creó, para
qué task, con qué rol y con qué límites.

## 17. `spec.md` append-only con versiones

Objetivo:
Crear una spec por thread versionada y referenciable desde tasks.

Estado implementado:
- `spec.events.jsonl` registra cambios append-only por thread con versión global
  y versión por sección.
- `GET /api/threads/:tid/spec` mantiene `content`/`etag` y agrega `version`.
- `PUT /api/threads/:tid/spec` mantiene compat legacy, incrementa versión y
  emite `spec.changed`.
- `PUT /api/threads/:tid/spec/sections/:section` actualiza una sección marcada
  y rechaza writes obsoletos con `spec_version_mismatch`.
- MCP agrega `spec_set_section`; `spec_read` devuelve `version`.
- `Task.spec_refs` permite referenciar `{ section, version }` desde REST/MCP.
- Verificado con `cargo test -p harness-core -p harness-server -p
  harness-mcp-server`, `just gen-types`, `pnpm check` y `just test`.

Contexto:
F3 requiere que planner mantenga `spec.md` y que workers/evaluator sepan contra
qué versión verificar.

Tarea:
1. Implementar `spec.md` por thread con operaciones append-only.
2. Agregar versionado incremental por cambio o sección.
3. Permitir que tasks referencien `{ section, version }`.
4. Implementar `spec.set_section` con `spec_version_required` para evitar stale writes.
5. Emitir `spec.changed` vía evento/SSE.
6. Agregar tests de version check y referencia desde task.

Reglas:
- Workers no editan spec.
- Planner/orchestrator conserva trazabilidad de cambios.
- Agregar test.

Resultado esperado:
Cada task puede decir qué parte y versión de la spec debe cumplir.

## 18. Artifacts como entidad/evento real

Objetivo:
Modelar artifacts con metadata propia en vez de depender solo de strings dentro
de `task.artifacts`.

Contexto:
Para evaluator y replay, no basta saber que hay un archivo; importa quién lo
produjo, cuándo, de qué tipo es y cómo se relaciona con la task.

Tarea:
1. Definir `Artifact { artifact_id, task_id, kind, path, produced_by,
   created_at, summary }`.
2. Soportar kinds iniciales: `file`, `diff`, `test_output`, `screenshot`, `log`.
3. Persistir artifact events append-only.
4. Mantener `task.artifacts` como vista resumida si conviene para compatibilidad.
5. Mostrar artifacts relevantes en TaskDetail/SessionRightPanel.
6. Agregar tests de creación, listado y referencia inexistente.

Reglas:
- No perder compatibilidad con `task.submit` actual.
- No duplicar blobs grandes dentro del evento; referenciar paths.
- Agregar test.

Resultado esperado:
El evaluator y la UI pueden inspeccionar artifacts con contexto suficiente.

## 19. Razones estructuradas en tasks

Objetivo:
Evitar que razones importantes queden escondidas solo en strings libres.

Contexto:
Hoy `notes.feedback`, `why_paused` y `why_abandoned` cubren parte del caso. Para
operar equipos conviene estructurar bloqueo, pausa, rechazo y necesidad humana.

Tarea:
1. Agregar campos o eventos para `blocked_reason`, `paused_reason`,
   `rejected_reason`, `last_failure` y `needs_human`.
2. Ajustar state machine para exigir razón donde aplique.
3. Mostrar razones en TaskDetail y badges.
4. Emitir eventos cuando cambien esas razones.
5. Agregar tests de transiciones con/sin razón requerida.

Reglas:
- Mantener compatibilidad con `notes.feedback`.
- No convertir cada comentario en schema rígido; estructurar solo razones operativas.
- Agregar test.

Resultado esperado:
Una task bloqueada, pausada o rechazada explica por qué de forma legible para UI
y machine-readable para scheduler/agentes.

## 20. Scheduler explain/debug

Objetivo:
Que el scheduler pueda explicar por qué asignó, saltó o bloqueó una task.

Contexto:
Cuando hay varios agentes, el síntoma "no avanzó" necesita diagnóstico directo:
deps pendientes, no hay evaluator idle, cooldown, cap de concurrencia, budget, etc.

Tarea:
1. Emitir decisiones del scheduler como eventos/debug records.
2. Cubrir causas: deps pendientes, no idle agent, cooldown verify-fail, cap de
   concurrencia, pause-all, budget cap, task inválida.
3. Exponer una vista/endpoint mínimo para últimas decisiones por thread.
4. Mostrar explicación compacta en UI donde aplique.
5. Agregar tests de causas principales.

Reglas:
- No generar ruido excesivo en logs normales.
- Agrupar decisiones repetidas cuando sea necesario.
- Agregar test.

Resultado esperado:
El usuario puede saber por qué una task no fue asignada sin leer logs internos.

## 21. Budget por task/agente

Objetivo:
Desglosar costo y uso por thread, sesión, task, rol y retry.

Contexto:
El budget base ya existe. Para equipos de agentes, el costo útil es el
desglose: qué task gastó, qué agente reintentó y qué rechazo fue costoso.

Tarea:
1. Enriquecer reportes de uso con `thread_id`, `session_id`, `task_id` y `role`.
2. Acumular totales por task/agente/rol.
3. Marcar costo asociado a retries y verify-fail.
4. Mostrar desglose en BudgetMeter/Live cost.
5. Agregar tests de acumulación y hard-cap por thread.

Reglas:
- No bloquear si un CLI no reporta tokens exactos; usar `unknown`/stub explícito.
- Mantener hard cap actual funcionando.
- Agregar test.

Resultado esperado:
El usuario puede ver qué parte del equipo consume presupuesto y dónde se pierde
costo en reintentos.

## 22. Reconciliador de estado

Objetivo:
Detectar y reparar o reportar inconsistencias entre tasks, sesiones, artifacts y
spec.

Contexto:
En un sistema con procesos hijos, reinicios y logs append-only, se necesitan
checks periódicos de salud del estado.

Tarea:
1. Implementar un pass de reconciliación por thread.
2. Detectar task `in_progress` con session muerta, session viva sin task,
   child sin parent válido, artifact referenciado inexistente y task bloqueada
   por task inexistente.
3. Para casos seguros, emitir evento de reparación; para casos ambiguos, marcar
   `needs_human`.
4. Exponer reporte mínimo en logs/UI.
5. Agregar tests con fixtures corruptos/huérfanos.

Reglas:
- No borrar datos automáticamente.
- Reparaciones deben ser append-only y trazables.
- Agregar test.

Resultado esperado:
Después de crashes o cambios manuales, el harness puede explicar y estabilizar
su estado sin perder historial.

## 23. Replay/debug timeline

Objetivo:
Reconstruir la historia completa de un thread como línea de tiempo auditable.

Contexto:
Para depurar F3/F4 hace falta ver el flujo: human prompt, planner, tasks,
workers, artifacts, evaluator, budget y fallbacks.

Tarea:
1. Crear un replay reader sobre eventos append-only.
2. Construir timeline ordenada con eventos humanos, task, agent, spec, artifact
   y budget.
3. Exponer endpoint o CLI mínimo para inspección.
4. Agregar vista frontend básica si el endpoint ya está estable.
5. Agregar tests de replay determinístico.

Reglas:
- No depender de estado mutable para reconstruir la historia.
- Manejar eventos desconocidos sin fallar.
- Agregar test.

Resultado esperado:
Un thread se puede auditar de principio a fin y compartir como timeline de
debug.

## 24. Tipos TS generados desde Rust para tasks

Objetivo:
Eliminar drift entre modelos Rust y TypeScript para tasks.

Contexto:
`frontend/src/lib/api/models/task.ts` dice que está hand-rolled hasta que
`ts-rs` exporte bindings. La convención crítica del repo exige `ts-rs` como
fuente de verdad.

Tarea:
1. Auditar exports actuales de `ts-rs` para `Task`, `TaskStatus` y structs
   relacionados.
2. Hacer que `just gen-types` genere bindings consumibles por frontend.
3. Reemplazar tipos hand-rolled por imports generados.
4. Ajustar schemas valibot si necesitan coexistir con tipos generados.
5. Agregar check para detectar tipos generados desactualizados.

Reglas:
- No editar a mano archivos generados en `frontend/src/lib/api/types/`.
- Mantener compatibilidad de imports de UI.
- Agregar test/check.

Resultado esperado:
Los tipos de task del frontend salen de Rust y dejan de divergir silenciosamente.

## 25. E2E pequeño planner→worker→evaluator

Objetivo:
Crear una prueba end-to-end pequeña antes del "TODO app challenge" completo.

Contexto:
El challenge completo gasta tiempo/costo y mezcla muchas variables. Un E2E
mínimo permite fijar el ciclo central de F3.

Tarea:
1. Crear thread fixture con goal sintético.
2. Simular o spawnear planner que crea 2 tasks.
3. Verificar que scheduler asigna una task a worker.
4. Worker submit artifacts.
5. Evaluator rechaza una vez con feedback.
6. Scheduler reasigna y luego evaluator acepta.
7. Verificar estado final, eventos, artifacts y budget básico.

Reglas:
- Preferir stubs determinísticos para no gastar tokens en CI.
- Mantener una variante manual real opcional fuera del test rápido.
- Agregar test.

Resultado esperado:
El loop planner → worker → evaluator queda probado sin depender del challenge
grande.

## 26. Árbol aislado de sesiones y mailbox de subagentes

Objetivo:
Formalizar que cada sesión raíz en Agents es un árbol aislado de contexto, con
subagentes multi-nivel que pueden comunicarse entre sí de forma estructurada,
auditable y sin mezclar contexto con otros árboles.

Contexto:
El modelo visual esperado es que en `Agents` el usuario inicia una sesión raíz,
esa sesión tiene su contexto, tasks y subagentes, y esos subagentes viven dentro
del árbol de esa sesión. Puede haber varias sesiones raíz en paralelo; una
sesión con una task y otra sesión sin task no deben interrumpirse ni compartir
contexto vivo accidentalmente. También queremos permitir que un subagente cree
sus propios subagentes y mantenga comunicación con parent/root/orchestrator,
pero sin chat libre opaco entre PTYs.

Tarea:
1. Definir el modelo de aislamiento: `root_session_id` como frontera fuerte de
   contexto, con `parent_session_id` para árbol multi-nivel y `thread_id` /
   `task_id` para scope operativo.
2. Extender metadata de sesión/subagente con `root_session_id`,
   `parent_session_id`, `spawned_by`, `spawn_reason`, `role`, `task_id`,
   `allowed_tools`, `write_paths`, `forbidden_paths`, `max_depth` y
   `max_children` según alcance mínimo.
3. Crear un `AgentMailbox` append-only para mensajes estructurados entre
   agentes: `{ id, root_session_id, from_session_id, to_session_id|role|parent|children|orchestrator,
   task_id, kind, body, created_at, requires_ack }`.
4. Permitir comunicación child→parent, parent→child, child→sibling y
   child→root/orchestrator solo dentro del mismo `root_session_id`, registrando
   cada mensaje como evento.
5. Bloquear por defecto comunicación entre árboles distintos (`Session A` ↔
   `Session B`) salvo acción humana o bridge explícito futuro.
6. Permitir que subagentes creen subagentes propios solo si capability, budget,
   concurrency, `max_depth`, `max_children` y scope lo permiten; el child hereda
   permisos iguales o más restringidos, nunca ampliados.
7. Filtrar herramientas de contexto (`task.get`, `artifact.list`,
   `spec.get_section`, `mailbox.read`) por `root_session_id`, `task_id` y
   capability policy.
8. Agregar eventos `agent.message.sent`, `agent.message.acknowledged`,
   `agent.spawn.requested`, `agent.spawned`, `agent.spawn.denied` y
   `agent.finding.reported`.
9. Actualizar UI Agents para mostrar árbol root→child→grandchild, mensajes
   recientes relevantes y razón de spawn sin mezclar sesiones raíz.
10. Agregar tests de aislamiento: dos sesiones raíz en paralelo no comparten
    mailbox ni contexto; un child no puede cruzar a otro root; un subagente no
    puede crear un child con más permisos.

Reglas:
- Una sesión raíz es frontera de contexto por defecto.
- La comunicación entre agentes debe ser append-only y visible en replay.
- No permitir comunicación PTY directa no auditada entre subagentes.
- Los permisos solo se heredan o reducen; nunca se amplían descendiendo el árbol.
- Agregar test.

Resultado esperado:
El harness soporta múltiples árboles de agentes en paralelo. Dentro de un árbol,
los subagentes pueden coordinarse y crear subagentes propios con límites claros;
fuera del árbol no se mezcla contexto ni comunicación. La UI y el replay pueden
explicar quién habló con quién, por qué se creó cada subagente y bajo qué scope
trabajó.

## Task 31: Medición de eficiencia de tools por spawn

Objetivo:
Medir el impacto real de cargar más o menos tools en cada spawn y determinar
qué operaciones deben pasar de tool LLM a rail Rust o contexto pre-inyectado.
Motivación: Vercel removió el 80% de las tools de su agente y pasó de 80% a 100%
de éxito con menos tokens y menos pasos. La hipótesis es que el mismo patrón
aplica aquí, especialmente para tasks de escritura de código.

Contexto:
Ver agents/smart-loading y agents/rust-rails. El smart-loading mínimo ya registra
`loaded_capabilities` por sesión (`harness` y `crawl4ai` cuando aplica) para que
la medición compare configuraciones reales. El endpoint de métricas por sesión
deriva tokens/costo desde el reporter de transcript y tool calls desde el
transcript normalizado append-only.

Tarea:
1. Ejecutado: snapshot por sesión con prompt_tokens, output_tokens, costo,
   tool_call_count, tool_call_breakdown (map de tool_name → count) y
   loaded_capabilities.
2. Ejecutado: endpoint GET /api/sessions/:id/metrics y tarjeta compacta en la
   UI de sesión.
3. Diseñar experimento A/B: misma task, distintas configuraciones de
   loaded_capabilities. Grupos mínimos: (a) tools completas del agente,
   (b) solo harness-bridge + bash, (c) rails Rust + contexto pre-inyectado.
4. Correr el experimento sobre al menos 3 tipos de task: code-write, plan,
   refactor.
5. Con los datos: identificar qué tools tienen call_count bajo o cero (candidatas
   a eliminar del default), qué operaciones se repiten y son determinísticas
   (candidatas a rail), y qué información se busca siempre al inicio
   (candidata a contexto pre-inyectado).
6. Proponer ajustes al capability-registry y a los spawn_hints del orchestrator.

Resultado esperado:
Tabla de ganancia por tipo de task: tokens ahorrados, pasos reducidos, delta
en tasa de éxito. Al menos una tool promovida a rail Rust o a contexto
pre-inyectado con evidencia de mejora.

Estado: ejecutada 2026-06-08. Instrumentación base y perfiles controlados
(`auto`, `none`, `harness`, `harness_crawl4ai`) listos. Se corrieron tres
muestras reales: repo temporal pequeño, variante forzada con `fd`/`rg`, y repo
pesado `aventi-workspace` en modo read-only. Decisión: mantener
`capability_profile=auto` como default, exponer `none` como perfil liviano para
sesiones locales simples, mantener Crawl4AI heuristic/explicit, y usar
preferencia `fd`/`rg` como rail de comportamiento pero no como optimización de
costo garantizada. Implementado follow-up técnico: MCP `repo_find` para búsqueda
determinística bounded por nombre/extensión/contenido y métrica de calidad en
`scripts/analyze-session-metrics.py` (`completion_marker_rate`,
`active_tool_work_rate`, `quality_pass_rate`). Reporte:
`docs/12-build-plan/task31-ab-experiment-2026-06-08.md`.

Follow-up abierto:
- Medir pass/fail semántico con evaluador, no solo marcador + evidencia de
  tools.
- Considerar que `repo_scan`/`repo_find` usen cache por repo/HEAD si aparecen
  como ruta caliente.
- Llevar `efficient_cli_command_rate` y `quality_pass_rate` al panel de métricas
  cuando el endpoint agregue categorías de comandos.

## Task 32: Reemplazar `pdftotext` por `pdf_oxide` embebido

Objetivo:
Eliminar la dependencia de sistema `pdftotext` (poppler-utils) en
`harness-core/knowledge.rs` y reemplazarla con el crate `pdf_oxide` embebido
directamente en Rust. Sin subprocess, sin dependencia externa del sistema.
Como bonus, agregar `pdf_oxide_mcp` como MCP opcional para que los agentes
puedan leer PDFs como tool.

Contexto:
`harness-core/src/knowledge.rs` usa `Command::new("pdftotext")` como subprocess.
Poppler es una dependencia del sistema (no Rust) que falla silenciosamente si no
está instalada. `pdf_oxide` (crates.io v0.3.60) es pure Rust, 0.8ms por PDF en
promedio, 100% pass rate en 3.830 PDFs reales, 5× más rápido que `pdf-extract`.
También existe `pdf_oxide_mcp` (v0.3.60) — MCP server que expone extracción de
PDFs como tool para agentes.

Tarea:
1. Agregar `pdf_oxide = "0.3"` en `backend/crates/harness-core/Cargo.toml`.
2. Reemplazar `extract_pdf_text` (y helpers `check_pdftotext`, `pdftotext_install_hint`)
   en `knowledge.rs` usando la API oficial:
   ```rust
   use pdf_oxide::PdfDocument;
   use pdf_oxide::converters::ConversionOptions;
   let mut doc = PdfDocument::open(path)?;
   let options = ConversionOptions { detect_headings: true, ..Default::default() };
   let md = doc.to_markdown(page, &options)?;
   ```
   Iterar todas las páginas del doc (`doc.page_count()`) y concatenar el markdown.
   Usar `to_markdown()` — agentes reciben markdown estructurado (headings, tablas,
   columnas) en vez de texto plano.
3. Eliminar la struct `PdfTextToolStatus` y su check de binario externo si queda
   sin uso; actualizar el `ReadinessReport` para que `pdftotext` deje de ser un
   check requerido.
4. Agregar `pdf_oxide_mcp` como servicio opcional en `docker-compose.mcp.yml`
   (misma estructura que `excalidraw-mcp`).
5. Actualizar `scripts/dev-mcp.sh` para incluir `pdf_oxide_mcp` en los servicios
   opcionales si el usuario lo activa.
6. Actualizar `just setup` para quitar el warn de `pdftotext` (ya no es necesario).
7. Correr `just test` para verificar que los tests de knowledge siguen verdes.

Resultado esperado:
`just setup` no menciona `pdftotext`. El knowledge base ingiere PDFs sin depender
de ningún binario del sistema. `pdf_oxide_mcp` disponible como MCP opcional para
agentes. `just test` verde.

Estado: ejecutada 2026-06-08.

## Task 33: Capacidad `docs.build` con Starlight como backend default

Objetivo:
Que el harness pueda generar documentación navegable para los proyectos donde
está desplegado. El doc-agent ya produce markdown; este task añade el paso de
compilar ese markdown en un sitio estático. Starlight (Astro) como backend
default — funciona igual para proyectos Rust, Node, Python o mixtos.

Contexto:
El rail `repo.analyze` ya detecta el stack del proyecto (rust/node/python/svelte).
El doc-agent escribe markdown en docs/**. La capacidad `docs.build` es el paso
final: toma ese markdown y produce un sitio estático desplegable. El backend
se elige según el stack detectado, con override manual posible.

Backends planificados:
- `starlight` (Astro) — default universal, TypeScript-friendly, sitio moderno
- `mdbook` — proyectos Rust puros (encaja con ecosistema docs.rs)
- `vitepress` — proyectos Vue/Vite

Tarea:
1. Definir `DocsBackend` enum en harness-core (starlight | mdbook | vitepress).
2. Implementar rail `docs.build(backend, source_dir, output_dir)` en harness-mcp-server.
3. Lógica de selección automática de backend en `infer_docs_backend(stack)`:
   - stack contiene solo "rust" → mdbook
   - default → starlight
4. Scaffold mínimo de Starlight: `package.json` + `astro.config.mjs` + estructura
   `src/content/docs/` donde el doc-agent deposita los archivos.
5. Agregar `docs.build` como tool MCP expuesta al orchestrator y doc-agent.
6. Agregar `starlight` y `mdbook` a la sección de CLIs opcionales en `just setup`.
7. Documentar en `docs/10-recipes/` cómo activar docs para un proyecto nuevo.

Resultado esperado:
El orchestrator puede pedir `docs.build` y obtener un sitio estático en
`<project>/docs-site/` listo para desplegar. `just setup` informa si Starlight
(npx astro) está disponible.

Estado: ejecutada 2026-06-08.

## Task 34: Project Memory Binding

Objetivo:
Evitar que una sesión nueva arranque ciega cuando el usuario vuelve a trabajar
en un repo que el harness ya conoce. El harness debe detectar el repo actual,
enlazarlo con threads/sesiones previas y ofrecer resume o contexto de proyecto.

Contexto:
La memoria dinámica y privada vive en `HARNESS_HOME`, aislada por profile. El
repo solo debe contener instrucciones estables (`AGENTS.md`) y, si el usuario
lo acepta, un marcador mínimo de identidad. No guardar logs, transcripts,
tasks ni memoria privada dentro del repo.

Decisión de storage:
- Fuente de verdad: SQLite por profile para el índice operativo de repos.
- JSON/Markdown: solo snapshots legibles, debug/export o continuity derivada.
- Motivo: el backend necesita consultas rápidas por repo/path/remote/thread,
  updates transaccionales, orden por `last_seen_at` y migraciones limpias.

Esquema inicial propuesto:
```sql
repos (
  id TEXT PRIMARY KEY,
  profile TEXT NOT NULL,
  project_id TEXT,
  root_path TEXT NOT NULL,
  canonical_path TEXT NOT NULL,
  remote_url TEXT,
  default_branch TEXT,
  last_branch TEXT,
  last_head_sha TEXT,
  last_thread_id TEXT,
  last_session_id TEXT,
  summary TEXT,
  first_seen_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  UNIQUE(profile, canonical_path),
  UNIQUE(profile, remote_url)
);

repo_threads (
  repo_id TEXT NOT NULL,
  thread_id TEXT NOT NULL,
  branch TEXT,
  head_sha TEXT,
  started_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  summary TEXT,
  PRIMARY KEY (repo_id, thread_id)
);
```

Tarea:
1. Implementar detector de identidad de repo: git root, remote principal,
   branch, `HEAD`, canonical path y fallback para repos sin remote.
2. Crear índice SQLite por profile para `repos` y `repo_threads`.
3. Registrar/actualizar el repo al crear thread/sesión con `cwd`.
4. Guardar en thread/session metadata el `repo_id`, `root_path`, `remote_url`,
   `branch` y `head_sha` observados al spawn.
5. Generar continuity breve por repo: último objetivo, pending tasks, blockers,
   últimos archivos tocados y thread recomendado para resume.
6. Inyectar un bloque corto de project context al spawn si el cwd pertenece a
   un repo conocido.
7. Exponer endpoints mínimos:
   - `GET /api/repos/current?cwd=<path>`
   - `GET /api/repos/:id`
   - `GET /api/repos/:id/threads`
8. UI al iniciar sesión en repo conocido:
   - Resume last thread.
   - Start fresh with project context.
   - Start completely fresh.
9. Agregar marcador opcional `.harness/project.toml` con `project_id`,
   `profile_hint` y `harness_memory = "external"`; nunca escribirlo sin acción
   humana explícita.

Reglas:
- El harness es la fuente de verdad del estado dinámico.
- El repo no recibe memoria privada ni logs.
- `AGENTS.md` queda para instrucciones estables del proyecto, no para estado de
  sesión.
- Si el repo se mueve o se clona, el marcador opcional permite reconectar con
  el mismo `project_id`; si no existe, se usa canonical path + remote.
- Agregar tests de detector, migración SQLite e endpoint de repo conocido.

Resultado esperado:
Al abrir una sesión nueva dentro de un repo ya visto, el harness reconoce el
proyecto, muestra la continuidad relevante y permite reanudar o arrancar fresco
con contexto sin depender de memoria del modelo.

Estado: base ejecutada 2026-06-08.

## Task 35: Agent liveness & health watchdog (anti-cuelgue, ocio y cortes de red)

Objetivo:
Medir y visualizar el estado de liveness de agentes en vivo, distinguir entre
cuelgue real, inactividad normal e interrupción transitoria de red, sin perder
trazabilidad de lo que cada agente estaba haciendo.

Contexto:
Cuando el harness lanza agentes (o un agente lanza subagentes en F4), no hay
forma de saber si están colgados, si están ociosos esperando task, o si es un
corte de red momentáneo. El usuario ve tareas `in_progress` sin progreso aparente
sin poder diagnosticar la causa. Los agentes del árbol pueden quedar huérfanos sin
feedback visual sobre su estado real.

Tarea:
1. Exponer señales de actividad por sesión en `SessionMeta`: `last_output_at`
   (PTY), `last_transcript_event_at`, `process_identity` con PID vivo,
   `start_time_ticks`, delta de CPU del proceso (via /proc/<pid>/stat o
   process stats).
2. Implementar detección de cuelgue: state machine con `working` → `stalled` si
   no hay output/eventos por > umbral configurable (default 60s); `tool_call`
   sin result por > umbral → `stalled` con razón "sin actividad desde NN min".
   Badge distintivo en UI/SessionRightPanel.
3. Medir ocio: prompt en reposo sin task pendiente → estado `idle` con
   `idle_since`; dashboard agrega contadores `N_idle / N_stalled` (hoy binario
   active/idle). Mostrar en tab Agents/metrics.
4. Resiliencia a cortes de red: reconexión SSE con `since` offset y backoff
   exponencial ya implementada para transcript (round 3, 2026-06-10); extender
   a `/api/sessions/:id/events` si falta. Banner "desconectado" en UI cuando
   SSE fall-off. El state detector debe reconocer patrones CLI (`API Error`,
   `Connection lost`, etc.) en tail ANSI → estado `blocked(network)` visible
   en vez de parecer cuelgue. El harness no mata sesión, solo señala.
5. Roll-up por árbol de subagentes: sesión raíz muestra el peor estado de sus
   hijos (parent_session_id/root_session_id ya en meta); child `stalled` →
   parent indica `child_stalled`. Indicador visual abierto/cerrado en árbol
   Agents.

Dependencias:
- Agent state detector (heurística por CLI tail ANSI para detectar state fino)
  es prerequisito para los estados `stalled` y `blocked(network)`; las señales
  del punto 1 no dependen y pueden ir primero.
- Eventos append-only de transición de state (punto 2) reutilizan el envelope
  de Task 15 (Eventos append-only unificados).

Reglas:
- No romper sesiones existentes sin state detector; hacer state opcional si es
  necesario (degradación).
- Cambios mínimos en dispatcher/routes; state es aditivo.
- SSE y metrics deben quedar consistentes.
- Agregar test de state transitions y cortes de red simulados.

Resultado esperado:
En Agents, cada sesión/subagente muestra badge `working`, `idle`, `stalled`
o `blocked(network)` con timestamp y razón. El usuario puede diagnosticar
si un agente está cuelgado (sin actividad 60s+), ocioso (esperando task) o
desconectado (error de red). El árbol de sesiones muestra roll-up del peor
estado de hijos. El harness no mata sesiones huérfanas, pero las señala
visiblemente para recuperación manual o automática.

## Task 36: Zeus: fallbacks configurables por rol (modelo/proveedor de respaldo)

Objetivo:
Permitir que cada rol en el modo Zeus declare proveedores/modelos de respaldo
ordenados, de forma que cuando el primario falla (proceso muere, cuelgue,
rate limit), el harness intente automáticamente con el siguiente de la lista,
manteniendo trazabilidad completa y telemetría para ajustar la selección de
primarios.

Contexto:
Hoy el modo Zeus configura roles con `ZeusRoleSelection { role, provider,
model?, effort? }` (ts-rs, usado en `CreateSessionRequest.zeus_roles` y
expuesto en `GET /api/sessions/:sid`). No hay forma de declarar qué pasa si
el primario falla: CLIs que se cuelgan (ej. codex exec headless, feedback
2026-06), APIs caídas, rate limits o crashes. El fallback hoy es manual o
requiere intervención externa. La detección de cuelgue es prerequisito de
Task 35 (watchdog); la V1 razonable puede cubrir fallos de arranque (exit ≠ 0)
y reintentos exhaustos.

Diseño propuesto:
1. Extender tipo Rust `ZeusRoleSelection` con campo `fallbacks?: Vec<ZeusRoleTarget>`
   donde `ZeusRoleTarget { provider, model?, effort? }` — lista ordenada de
   alternativas. Nuevo tipo ts-rs: correr `just gen-types` tras cambio de tipos.
2. UI (frontend, pantalla de configuración de roles Zeus): para cada rol,
   además del selector primario, agregar lista ordenable de fallbacks
   (añadir/quitar/reordenar via drag-drop o botones +/−).
3. Runtime (backend spawner): cuando el spawner de un rol detecta fallo del
   primario (proceso exit != 0 al arrancar, cuelgue por > umbral de Task 35,
   o N reintentos fallidos), recrea la sesión del rol con el siguiente target
   de la lista `fallbacks`. Anota el evento append-only en el transcript/board
   de la sesión: `system_note "role X fell back to provider/model Y, reason Z"`.
4. Persistencia: los fallbacks viajan en `zeus_roles` (ya persistidos vía
   `persist_zeus_roles`); se preservan en Restart (round 3 reenvía `zeus_roles`
   verbatim).
5. Telemetría: cada activación de fallback registra (contador por
   proveedor/modelo, razón, timestamp) → alimenta `docs/teamwork/SCOREBOARD.md`
   para decidir primarios con datos reales de fallos.

Tarea:
1. Auditar `ZeusRoleSelection` en `harness-core/src/` y en el contrato
   `CreateSessionRequest` (ts-rs, `POST /api/threads/:tid/sessions`).
2. Extender `ZeusRoleSelection` con `fallbacks: Option<Vec<ZeusRoleTarget>>`.
   Definir `ZeusRoleTarget { provider: String, model: Option<String>,
   effort: Option<String> }`. Ambos deben derivar `#[derive(TS)]`.
3. Ejecutar `just gen-types` y verificar que frontend/src/lib/api/types/ incluya
   los nuevos tipos.
4. Actualizar UI del formulario de creación/edición de sesión Zeus en
   `frontend/src/lib/components/` para exponer lista de fallbacks por rol
   con interfaz ordenable.
5. En backend spawner (probablemente en `harness-mcp-server/src/dispatcher.rs`
   o `harness-server/src/routes/sessions.rs`), detección de fallo primario e
   iteración sobre fallbacks.
6. Al cambiar a un fallback, emitir evento append-only con razón clara
   (`exit_code`, `timeout`, `max_retries_exceeded`, etc.).
7. Acumular telemetría por rol/proveedor/modelo de fallback activaciones →
   guardar snapshot en `~/.harness/profiles/<p>/telemetry/fallback-activations.jsonl`.
8. Agregar tests: crear sesión Zeus con fallbacks, simular fallo primario y
   verificar que el segundo se intenta; verificar evento append-only y
   telemetría grabada.

Reglas:
- No romper sesiones Zeus sin fallbacks; campo `fallbacks` es opcional.
- V1 no requiere Task 35 (watchdog); solo cubre exit != 0 al arrancar y
  reintentos exhaustos. Fallback por cuelgue (stalled > threshold) llega
  cuando Task 35 esté listo.
- Cambios mínimos en contratos existentes.
- Mantener append-only en eventos de fallback.
- Agregar test.

Resultado esperado:
Al crear una sesión Zeus, el usuario puede declarar para cada rol un primario
y una lista de fallbacks. Si el primario falla al arrancar o tras N reintentos,
el harness intenta automáticamente con el siguiente de la lista, anota el
evento en el transcript y registra telemetría. La trazabilidad es completa
(quién/cuándo/por qué se activó el fallback) y reutilizable para decisiones
de selección de primarios.

Dependencias:
- Task 35 (watchdog) proporciona detección fina de cuelgue; sin ella, V1 cubre
  solo fallos de arranque (exit != 0) y reintentos exhaustos.
- Tipos `ts-rs` generados (gate: `just gen-types` debe correr tras cambios).
- Tests: `cargo test -p harness-server -p harness-mcp-server` y
  `pnpm check` deben pasar.
