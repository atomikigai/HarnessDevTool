---
id: build-plan/open-questions
title: Preguntas abiertas (a aclarar)
shard: 12-build-plan
tags: [questions, pending, todo]
summary: Lo que sigue sin decidir y debe resolverse antes/durante cada fase.
related: [build-plan/overview, build-plan/decisions-locked]
sources: []
---

# Preguntas abiertas

> Estado tras las discusiones de memoria, agentes y cleanup. Marca `[RESUELTA]` en las que ya cerramos; el resto sigue requiriendo decisión.

## Cross-cutting

### Q1 · Identidad/profile activo `[RESUELTA]`
→ Profile activo es **global del backend**, resuelto vía symlink `~/.harness/active_profile` + env `HARNESS_PROFILE`. Cambio de profile en UI dispara symlink update + restart suave. Ver [[cross-cutting/profiles]] y [[build-plan/decisions-locked]].

### Q2 · `AGENTS.md` snapshot del proyecto del usuario `[RESUELTA — reformulada]`
→ `AGENTS.md` **no** vive en el repo del usuario. En su lugar: cuando se abre una sesión a una carpeta nueva, el harness corre un **análisis inicial del repo** y genera/actualiza un `ARCHITECTURE.md` **dentro del propio repo del usuario**. Diseño:
- **Corto, indexable, referenciable** — frontmatter + shards atómicos (≤ 80 líneas cada uno).
- Sub-shards en `architecture/` cuando una sección crece (`architecture/data-model.md`, `architecture/routes.md`, etc.), linked desde el índice raíz.
- Generado por un agente "repo-mapper" (rol nuevo) que corre `repo.scan` + heurísticas (lenguajes, frameworks, entrypoints) + LLM summarization.
- Refresh manual (botón "Re-analyze repo") o automático con threshold de cambios (decidir en F3).
- Convive con cualquier `AGENTS.md` del proyecto si el usuario ya tiene uno; no lo pisa.
- **Documentar shard nuevo**: [[recipes/architecture-md-generation]] al implementar.

### Q3 · Correlación de logs cross-process `[RESUELTA]`
→ Cada spawn lleva `spawn_id` (UUID v4) asignado al crear el child. Spans `tracing` del backend lo incluyen como atributo; `spawns/<sid>/output.log` lleva el id en su path. Cross-ref por timestamp + id.

## F1 — Sesiones

### Q4 · Múltiples sesiones simultáneas en UI desde F1 `[RESUELTA]`
→ **Sí, multi-sesión real**. Backend ya soporta múltiples sesiones; la clave es que la UI **no cierre nada al cambiar de pantalla**. Implicaciones:
- Cada sesión/conexión vive en el backend independiente de la ruta UI activa.
- Frontend monta/desmonta vistas pero el state persistente (PTY buffers, DB pools, SSH channels, transfer queues) sigue corriendo.
- Ir de `/agents` a `/db` y volver: la sesión PTY sigue viva; el output que llegó mientras estabas en `/db` se reprodujo via SSE buffer y la vista lo muestra al re-montar.
- Lo mismo para `/db` ↔ `/ssh`: pool y conexiones no se destruyen al navegar.
- **Regla**: solo se cierra cuando el usuario explícitamente cierra la sesión/conexión, o por TTL/idle timeout del backend.
- UI: tabs (no una vista única) para sesiones de agentes, mostrando indicador "live" cuando llega output mientras estás en otra tab.

### Q5 · CLIs soportados `[RESUELTA]`
→ Set fijo de **4 CLIs hardcoded**: `claude`, `codex`, `cursor` (cursor-agent), `antigravity` (`agy`). No hay `agent_kind: custom` por ahora. Cada CLI necesita:
- Detector binario (path discovery).
- Plantilla de spawn (cómo se inyecta el prompt inicial — ver [[agents/spawn-lifecycle]] N2).
- Mapeo de flags (`--append-system-prompt` equivalente si existe).
- Test de smoke: spawn + saludo + exit.
- **Acción**: ampliar selector del UI de "claude/codex" a los 4. Crear shard [[agents/supported-clis]] con la matriz de features por CLI.

### Q6 · Persistencia del PTY raw `[RESUELTA]`
→ 50 MiB con rotación zstd. Documentado en [[agents/spawn-lifecycle]].

## F2 — Tasks + MCP

### Q7 · MCP config format para claude/codex `[CRÍTICA, SPIKE AL INICIO DE F2]`
- Riesgo R1 — bloquea F2 entero.
- ¿`claude` acepta `--mcp-config <file.json>` con nuestro formato? ¿`codex` también?
- **Spike al arrancar F2** (no antes): F1 es PTY interactivo puro sin MCP, así que descubrir esto al iniciar F2 no añade rework — F2 es donde se introduce el MCP de todos modos.

### Q8 · Granularidad de tasks `[RESUELTA]`
→ ≤6 `acceptance.checks` por task. Validation warning (no error). Documentado en [[agents/orchestrator]] y [[foundations/lessons-learned]] §D4.

### Q9 · Matriz roles × tools MCP permitidas `[RESUELTA]`
→ Cerrada en [[agents/role-capability-matrix]]. Modelo: `role + tool + resource + scope + ownership + thread_id + path_policy`. `task.create` solo planner/orchestrator; workers usan nueva tool `task.propose`. `spec.set_section` exige version check. Workers no tienen `memory.search:global`. `repo.write` atado a `task.write_paths`. Audit obligatorio para allow y deny.

### Q10 · Roles concurrentes del mismo tipo `[RESUELTA]`
→ `max_concurrent_spawns = 3` por thread, configurable en `budget.toml`. Documentado en [[build-plan/phase-3-team]].

## F3 — Equipo

### Q11 · `spec.md` lock vs concurrencia `[RESUELTA]`
→ `spec.md` es **append-only durante thread activo**; solo planner/orchestrator edita. `spec.append_section` no necesita lock (es append). `spec.set_section` exige `spec_version_required` + section lock atómico — rechaza si la versión está stale (ver [[agents/role-capability-matrix]] §spec.md). Workers nunca tocan spec; escriben en `task.notes`/`task.artifacts`/`qa.results`/`learner.observations`.

### Q12 · Recovery de un agente muerto `[RESUELTA]`
→ Tras `TTL + grace 30min` sin renew, scheduler mueve task a `queued` con `notes.recovered_from_crash`. Documentado en [[agents/spawn-lifecycle]].

## F4 — Módulos

### Q13 · Multi-tab queries y conexiones DB `[RESUELTA]`
→ **Shared pool por default + pin opt-in cuando hace falta session state**.
- Default: cada query SQL del editor pide una conexión al pool, la usa, la devuelve. El pool ya respeta `max_connections`.
- Casos que requieren la misma conexión (transacciones largas, `SET search_path`, temp tables, `LISTEN/NOTIFY`): la tab obtiene un **lease** de una conexión específica.
- Trigger del lease: (a) automático al detectar `BEGIN` en el SQL ejecutado, (b) manual via toggle "🔒 Pin session" en el header del editor.
- Liberación: `COMMIT`/`ROLLBACK`, cerrar tab, o timeout de inactividad (5min) → libera la conexión y warning al usuario si había transacción abierta.
- Cancelación de queries usa una conexión auxiliar del pool (MySQL/PG), ortogonal al lease.

### Q14 · SFTP transfer policies default `[RESUELTA]`
→ Default **`resume`**. Si la transferencia no es resumable (size mismatch / file vanished / hash divergente): fallback a **`ask`** con modal por archivo (opción "apply to all" en el modal). UI permite override por batch al encolar. **Nunca `overwrite` silencioso.**

## F5 — Skills

### Q15 · memory vs skills (semántica clara) `[RESUELTA]`
→ Memory = qué pasó/decidimos; Skills = cómo hacer bien una clase de tareas. Documentado en [[foundations/lessons-learned]] §H8 y [[memory/search-and-index]].

### Q16 · Learner auto-promote `[RESUELTA]`
→ Siempre `proposed/` en F5; F6 puede abrir `auto-promote-if-confidence > N` con review humano todavía. Documentado en [[agents/learner]].

### Q17 · Skills compartibles entre profiles `[RESUELTA]`
→ Default profile-scoped; `harness skills promote` mueve a `shared/` con review. Documentado en [[memory/layout]] y [[cross-cutting/profiles]].

## F6 — Polish

### Q18 · Tasks-target reproducibles para GEPA `[RESUELTA]`
→ **Curated manual** al cierre de F3: 5 tasks-target representativas cubriendo (a) frontend simple, (b) backend CRUD, (c) bug fix, (d) refactor, (e) DB schema change. Viven en `tests/eval/targets/`. F6 puede ampliar a generated/expandido si hace falta más coverage.

### Q19 · Distribución `[RESUELTA]`
→ **Self-hosted only**. Dockerfile + `docker compose` en el repo; el usuario clona, builda y corre local. No publicamos imagen a registries públicos por ahora. Reduce superficie de ataque y mantenimiento. Re-abrir si surge demanda real.

### Q20 · IDE integration (ACP-style) `[RESUELTA — fuera de scope]`
→ Fuera de scope hasta haber estabilizado todo lo demás.

---

## Nuevas surgidas en cleanup (no estaban antes)

### N1 · `harness-mcp-server`: sub-binario vs in-process `[RESUELTA]`
→ **In-process por default** vía feature `embedded`. Child process como fallback documentado si surgen problemas de aislamiento/crash. Más simple para arrancar; fácil cambiar después porque la interfaz MCP stdio JSONL es la misma.

### N2 · Cómo el harness inyecta el prompt inicial al CLI hijo `[RESUELTA]`
- **Mecanismo**: stdin como primer "user input" al PTY. Portable a cualquier CLI, no depende de flags.
- **Patrón uniforme**: una sola función `harness::spawn(role, initial_context) → spawn_id`. Quien spawneа siempre es el harness; el contenido del prompt inicial varía por contexto:
  - **F1 raíz interactivo**: `role=None`, sin inyección — el humano escribe directo en el PTY.
  - **F3 orchestrator**: `role=Planner`, harness inyecta plantilla + user goal + tools disponibles (`task.create`, `task.list`).
  - **F3 worker**: `role=Frontend|Backend|...`, harness inyecta plantilla + `task_id` asignada + refs a tasks relacionadas (solo ids, no contenido — el worker usa `task.get` / `memory.search` si los necesita).
- El orchestrator no spawneа workers directamente; pide al harness vía MCP (`task.spawn_worker` o similar). El scheduler del harness es el único que llama `spawn()`.
- Fallback futuro si el rol "se olvida" turn-tras-turn: añadir `--append-system-prompt` por CLI (requiere confirmar soporte en `codex`). No bloquea F3.
- Documentar mecanismo final en [[agents/spawn-lifecycle]] al implementar F3.

### N3 · Sandbox de las tools que el CLI ejecuta `[RESUELTA]`
→ Confiamos en el sandbox del CLI hijo (`claude`/`codex`/`cursor`/`agy`) para sus propios `shell.exec` y tools. **No duplicamos.** `harness-sandbox` envuelve solamente lo que el bridge ejecuta directamente — y casi todo el bridge es read-only (`repo.scan`, `memory.search`, `task.list`...), así que el sandbox del harness es mínimo.

### N7 · Implementar módulos SQL y SSH (F4) `[WORK ITEM — SQL done, SSH pendiente en F4]`
Hay diseño UI ya hecho para ambos módulos, vive en `DEVTOOL - GUI/` (gitignored, copia local del usuario):
- `harness-table-v2.jsx` — vista de tabla virtualizada estilo "paper" con row-detail panel derecho, breadcrumbs sara/public/users, tabs Data/Query/Schema/Relations. Es la referencia para **SQL** (DB Manager).
- `harness-ssh.jsx` — referencia para **SSH Manager** (FileZilla-style 2-paneles, drag&drop, transfer queue).
- Screenshots: `screenshots/preview.png` y `paper-interactive.png` muestran SQL en uso.

Lo que falta:
- ~~**Backend SQL** (crate `module-db`)~~ ✅ DONE: pools per-engine SQLite/Postgres/MySQL, query.run/cancel/export, schema.tree, row CRUD, MCP tools (`db_query/schema/explain`).
- **Backend SSH** (crate `module-ssh`): russh + russh-sftp; gestión de identidades + agente + host keys; cola de transferencias resumable; endpoints REST + SSE para progreso. **No iniciado.**
- ~~**Frontend SQL**~~ ✅ DONE en `/db`: schema tree + SQL editor + ResultGrid virtualizado + RowEditor + ExportDialog. Falta solo: schema valibot en el ConnectionFormDialog.
- **Frontend SSH**: ruta `/ssh/+page.svelte` con dos paneles local↔remote, drag&drop, queue panel inferior. **No iniciado.**
- **IconRail**: SQL ya habilitado; SSH sigue disabled con badge "soon".

Este es el alcance entero de **F4** (ver `phase-4-modules.md`). Anotado aquí porque ya existe diseño UI listo para arrancar.

### N6 · Llenar los tabs del SessionRightPanel con datos reales `[WORK ITEM — revisar estado actual antes de F3]`
> **Nota**: ya hubo cambios incrementales sobre estos tabs en commits previos. Antes de retomar, hacer un audit del estado real de `SessionRightPanel.svelte` (Tasks/Agents/Info) y actualizar este shard con lo que falta de verdad.
El panel derecho de la sesión (`SessionRightPanel.svelte`) tiene 3 tabs y solo el de Tasks lee data parcial. Pendiente:

- **Tasks**: hoy lee de `tasksState` que se suscribe vía SSE solo a la thread seleccionada. Funciona PARCIAL porque la sesión claude actual NO crea tasks vía MCP todavía (espera spawn vía orchestrator F3). **Acción**: verificar que cuando el claude de la sesión llame `mcp__harness__task_create` (cuando exista, hoy solo está `task_list/get/claim/etc`), las nuevas tasks aparezcan en el panel en tiempo real vía el SSE `task.created` que ya existe.
  - Hay que añadir la tool MCP `task.create` (no está en el set F2; el shard la lista pero la matriz Q9 solo permite create al orchestrator). En F3 cuando el orchestrator exista, esto cierra el loop.
  - Para sesiones humanas (sin orchestrator), añadir un botón "+ task" en el tab que llame al REST endpoint con `created_by: "human"`.

- **Agents (sub-agentes)**: el tab debe mostrar los agentes paralelos spawneados POR esta sesión claude (no por el harness directamente). Esto requiere:
  - Observado 2026-05-27: la orquestación Claude → Codex funciona; Claude pudo iniciar Codex y Codex quedó trabajando. Bug pendiente: el tab Agents del panel derecho no reflejó esa sesión hija activa.
  - Regla de producto: agentes autorizados pueden iniciar subagentes si lo necesitan; no es exclusivo del Zeus/root orchestrator. La UI debe mostrar el árbol completo padre → hijas → nietas si se permite más de un nivel.
  - Que claude pueda llamar a una tool MCP tipo `agent.spawn { role, prompt, ... }` que vaya al harness y arranque una sub-sesión hija marcada con `parent_session_id = <sid>`.
  - Backend: el shape `SessionMeta` necesita campo opcional `parent_session_id`. El Manager lista hijas vía `list_children(parent_sid)`.
  - SSE: emitir `subagent.spawned/started/exited` filtrables por `?parent_session=<sid>`.
  - Frontend: el componente `SessionRightPanel.svelte` tiene markup preparado en el tab Agents (con comentario TODO(F3)) — enchufar al store cuando exista.
  - **Esto es esencialmente F3 entero** (el orchestrator y sus workers). Solo anotar aquí el requisito de UI: tab debe reflejar live.

- **Info**: hoy muestra metadata estática (id, kind, cwd, status, pid). Faltan:
  - Token usage real (requiere parseo del output del CLI o tool MCP `session.stats`).
  - Costo acumulado en USD (idem).
  - Modelo exacto (claude-3.5-sonnet, claude-opus, etc — derivable del CLI output o de un flag al spawn).
  - Tiempo total wallclock vs tiempo del modelo.
  - **Mecanismo**: cuando el CLI hijo expone esto en su output, parsear; alternativa, el harness lleva contadores con base en heartbeats del proceso. Decidir en F3.

### N5 · Adjuntar archivos a las sesiones `[RESUELTA — opción (a)]`
→ Endpoint `POST /api/sessions/:sid/attach` multipart copia archivos a `$HARNESS_HOME/.runtime/attach/<sid>/<filename>` y emite SSE `session.attachment { path, mime, size }`. El CLI hijo accede via tools MCP nuevas `attach.list { session_id }` / `attach.read { session_id, name }`.
- **Propósito**: que `claude`/`codex`/`cursor`/`agy` puedan ver imágenes, documentos (PDF, MD), o archivos arbitrarios pasados por el usuario.
- MIME detection en backend; tool `attach.read` devuelve binario base64 + mime para que el CLI decida cómo procesar.
- Cleanup: directorio `attach/<sid>/` se purga cuando la sesión cierra (o por TTL si la sesión sigue viva > 24h).
- Habilitar el botón Paperclip del SessionMainView cuando endpoint exista.

### N4 · Auth re-login dentro del container `[RESUELTA]`
→ **Bind-mount RW compartido**. Container y host comparten literalmente `~/.claude/`, `~/.codex/`, `~/.cursor/`, `~/.antigravity/`. Refresh tokens sobreviven a destrucción del container. **Restricción documentada**: el host no debe correr el mismo CLI con otra cuenta en paralelo mientras hay sesión activa en el harness (puede confundir el token store del CLI).

---

## Reglas de cierre

1. Discutir con el usuario o tomar decisión documentada.
2. Mover a [[build-plan/decisions-locked]] con razón.
3. Marcar `[RESUELTA]` aquí con link a donde quedó.
4. Si afecta shards ya escritos, parchearlos.

## Estado de cierre

**Resueltas**: Q1, Q2, Q3, Q4, Q5, Q6, Q7, Q8, Q9, Q10, Q11, Q12, Q13, Q14, Q15, Q16, Q17, Q18, Q19, Q20, N1, N2, N3, N4, N5 (25/26 preguntas + sub-items).
**Work items abiertos (no preguntas, trabajo a ejecutar)**: N6 (audit + completar tabs SessionRightPanel, F3), N7 (SSH backend + frontend, F4).

**Total pendiente de decisión: 0.** Todas las preguntas cerradas. Solo quedan slices de implementación.
