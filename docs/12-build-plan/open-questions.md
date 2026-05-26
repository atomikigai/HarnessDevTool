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

### Q2 · `AGENTS.md` snapshot del proyecto del usuario `[PENDIENTE]`
- Cuando el thread tiene `working_dir = /home/me/proj/myapp`, ¿cómo encuentra el harness el `AGENTS.md`?
- **Propuesta**: subir desde `working_dir` buscando git root; si existe `<git-root>/AGENTS.md`, snapshot. Si no, fallback a `~/AGENTS.md` global del usuario. Si tampoco, vacío.
- **Decisión requerida antes de F1**.

### Q3 · Correlación de logs cross-process `[PENDIENTE]`
- El `harness-server` loggea con `tracing`. El `claude`/`codex` hijo escribe a su PTY. ¿Cómo correlacionamos?
- **Propuesta**: cada spawn lleva `spawn_id` (UUID). Spans del backend lo incluyen como atributo; `spawns/<sid>/output.log` lleva el id en su path. Cross-ref por timestamp + id.
- **Decisión menor, no bloqueante**.

## F1 — Sesiones

### Q4 · Múltiples sesiones simultáneas en UI desde F1 `[PENDIENTE]`
- ¿Permitimos lista + tabs desde F1 o esperamos a F3?
- **Propuesta**: F1 = lista en sidebar muestra activas pero vista activa **una sola** a la vez. Multi-tab en F3 cuando el equipo lo necesita.

### Q5 · CLIs desconocidos (no `claude` ni `codex`) `[PENDIENTE]`
- ¿Soportamos otros (aider, cursor-cli)?
- **Propuesta**: F1 hardcodea dos opciones. F4+ generaliza con `agent_kind: "custom"` + plantilla del usuario.

### Q6 · Persistencia del PTY raw `[RESUELTA]`
→ 50 MiB con rotación zstd. Documentado en [[agents/spawn-lifecycle]].

## F2 — Tasks + MCP

### Q7 · MCP config format para claude/codex `[CRÍTICA, SPIKE AL INICIO DE F2]`
- Riesgo R1 — bloquea F2 entero.
- ¿`claude` acepta `--mcp-config <file.json>` con nuestro formato? ¿`codex` también?
- **Spike al arrancar F2** (no antes): F1 es PTY interactivo puro sin MCP, así que descubrir esto al iniciar F2 no añade rework — F2 es donde se introduce el MCP de todos modos.

### Q8 · Granularidad de tasks `[RESUELTA]`
→ ≤6 `acceptance.checks` por task. Validation warning (no error). Documentado en [[agents/orchestrator]] y [[foundations/lessons-learned]] §D4.

### Q9 · Matriz roles × tools MCP permitidas `[PENDIENTE]`
- ¿El planner puede `task.create` pero no `task.claim`? ¿El generator al revés?
- **Decisión requerida antes de F2**. Lo formalizo como tabla en [[agents/capability-registry]] o en un shard nuevo.

### Q10 · Roles concurrentes del mismo tipo `[RESUELTA]`
→ `max_concurrent_spawns = 3` por thread, configurable en `budget.toml`. Documentado en [[build-plan/phase-3-team]].

## F3 — Equipo

### Q11 · `spec.md` lock vs concurrencia `[PENDIENTE]`
- ¿El planner puede editar `spec.md` mientras hay workers activos?
- **Propuesta**: spec append-only durante un thread activo; solo planner edita; secciones individuales pueden actualizarse vía `spec.set_section` con lock por sección.

### Q12 · Recovery de un agente muerto `[RESUELTA]`
→ Tras `TTL + grace 30min` sin renew, scheduler mueve task a `queued` con `notes.recovered_from_crash`. Documentado en [[agents/spawn-lifecycle]].

## F4 — Módulos

### Q13 · Multi-tab queries y conexiones DB `[PENDIENTE]`
- ¿Cada tab "Editor SQL" comparte conexión del pool o usa su propia?
- **Propuesta**: comparten; el pool gestiona.

### Q14 · SFTP transfer policies default `[PENDIENTE]`
- ¿`overwrite`, `skip`, `resume`, `ask`?
- **Propuesta**: `resume` por default; UI permite override por batch. Para conflictos sin resume posible (size mismatch): `ask`.

## F5 — Skills

### Q15 · memory vs skills (semántica clara) `[RESUELTA]`
→ Memory = qué pasó/decidimos; Skills = cómo hacer bien una clase de tareas. Documentado en [[foundations/lessons-learned]] §H8 y [[memory/search-and-index]].

### Q16 · Learner auto-promote `[RESUELTA]`
→ Siempre `proposed/` en F5; F6 puede abrir `auto-promote-if-confidence > N` con review humano todavía. Documentado en [[agents/learner]].

### Q17 · Skills compartibles entre profiles `[RESUELTA]`
→ Default profile-scoped; `harness skills promote` mueve a `shared/` con review. Documentado en [[memory/layout]] y [[cross-cutting/profiles]].

## F6 — Polish

### Q18 · Tasks-target reproducibles para GEPA `[PENDIENTE]`
- ¿Cómo se construye? ¿Generated o curated?
- **Propuesta**: curated manual al cierre de F3 (5 tasks-target representativas). Mantener en `tests/eval/targets/`.

### Q19 · Distribución `[PENDIENTE]`
- Docker Hub público vs ghcr.io vs solo self-host?
- **Decidir en F6**, no urgente.

### Q20 · IDE integration (ACP-style) `[RESUELTA — fuera de scope]`
→ Fuera de scope hasta haber estabilizado todo lo demás.

---

## Nuevas surgidas en cleanup (no estaban antes)

### N1 · `harness-mcp-server`: sub-binario vs in-process `[PENDIENTE]`
- ¿Lo spawneamos como child process del backend o lo linkeamos in-process?
- **Trade-off**: child = aislamiento + Codex-like + más memoria; in-process = más rápido + simpler + más acoplado.
- **Propuesta**: in-process por default (`feature = "embedded"`); habilitar child como fallback si surgen problemas.
- **Decidir en F2**.

### N2 · Cómo el harness inyecta el prompt inicial al CLI hijo `[RESUELTA]`
- **Mecanismo**: stdin como primer "user input" al PTY. Portable a cualquier CLI, no depende de flags.
- **Patrón uniforme**: una sola función `harness::spawn(role, initial_context) → spawn_id`. Quien spawneа siempre es el harness; el contenido del prompt inicial varía por contexto:
  - **F1 raíz interactivo**: `role=None`, sin inyección — el humano escribe directo en el PTY.
  - **F3 orchestrator**: `role=Planner`, harness inyecta plantilla + user goal + tools disponibles (`task.create`, `task.list`).
  - **F3 worker**: `role=Frontend|Backend|...`, harness inyecta plantilla + `task_id` asignada + refs a tasks relacionadas (solo ids, no contenido — el worker usa `task.get` / `memory.search` si los necesita).
- El orchestrator no spawneа workers directamente; pide al harness vía MCP (`task.spawn_worker` o similar). El scheduler del harness es el único que llama `spawn()`.
- Fallback futuro si el rol "se olvida" turn-tras-turn: añadir `--append-system-prompt` por CLI (requiere confirmar soporte en `codex`). No bloquea F3.
- Documentar mecanismo final en [[agents/spawn-lifecycle]] al implementar F3.

### N3 · Sandbox de las tools que el CLI ejecuta `[PENDIENTE]`
- `claude` tiene su propio sandbox/approval para `shell.exec`. ¿Necesitamos sandbox adicional desde el harness?
- **Propuesta**: confiamos en el sandbox del CLI hijo para sus tools. Nuestro `harness-sandbox` envuelve solo lo que el harness-bridge ejecuta directamente (raro: la mayoría son rails read-only).
- **Decidir en F3**.

### N7 · Implementar módulos SQL y SSH (F4) `[PENDIENTE, F4]`
Hay diseño UI ya hecho para ambos módulos, vive en `DEVTOOL - GUI/` (gitignored, copia local del usuario):
- `harness-table-v2.jsx` — vista de tabla virtualizada estilo "paper" con row-detail panel derecho, breadcrumbs sara/public/users, tabs Data/Query/Schema/Relations. Es la referencia para **SQL** (DB Manager).
- `harness-ssh.jsx` — referencia para **SSH Manager** (FileZilla-style 2-paneles, drag&drop, transfer queue).
- Screenshots: `screenshots/preview.png` y `paper-interactive.png` muestran SQL en uso.

Lo que falta:
- **Backend SQL** (crate `module-db`): drivers sqlx para SQLite/Postgres/MySQL; pool por conexión guardada; query runner con pagination + cancel; introspección de schema (DB→tabla→columna); endpoints REST (`GET /api/db/connections`, `POST /api/db/query`, etc) + SSE para streaming de filas grandes.
- **Backend SSH** (crate `module-ssh`): russh + russh-sftp; gestión de identidades + agente + host keys; cola de transferencias resumable; endpoints REST + SSE para progreso.
- **Frontend SQL**: ruta `/sql/+page.svelte` con layout 3-col (sidebar conexiones → tablas → tabla virtualizada). Adaptar `harness-table-v2.jsx` a Svelte 5 + Tailwind v4.
- **Frontend SSH**: ruta `/ssh/+page.svelte` con dos paneles local↔remote, drag&drop, queue panel inferior.
- **IconRail**: las entradas SQL y SSH están hoy disabled con badge "soon"; habilitarlas cuando el módulo esté listo.

Este es el alcance entero de **F4** (ver `phase-4-modules.md`). Anotado aquí porque ya existe diseño UI listo para arrancar.

### N6 · Llenar los tabs del SessionRightPanel con datos reales `[PENDIENTE, F2.5/F3]`
El panel derecho de la sesión (`SessionRightPanel.svelte`) tiene 3 tabs y solo el de Tasks lee data parcial. Pendiente:

- **Tasks**: hoy lee de `tasksState` que se suscribe vía SSE solo a la thread seleccionada. Funciona PARCIAL porque la sesión claude actual NO crea tasks vía MCP todavía (espera spawn vía orchestrator F3). **Acción**: verificar que cuando el claude de la sesión llame `mcp__harness__task_create` (cuando exista, hoy solo está `task_list/get/claim/etc`), las nuevas tasks aparezcan en el panel en tiempo real vía el SSE `task.created` que ya existe.
  - Hay que añadir la tool MCP `task.create` (no está en el set F2; el shard la lista pero la matriz Q9 solo permite create al orchestrator). En F3 cuando el orchestrator exista, esto cierra el loop.
  - Para sesiones humanas (sin orchestrator), añadir un botón "+ task" en el tab que llame al REST endpoint con `created_by: "human"`.

- **Agents (sub-agentes)**: el tab debe mostrar los agentes paralelos spawneados POR esta sesión claude (no por el harness directamente). Esto requiere:
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

### N5 · Adjuntar archivos a las sesiones `[PENDIENTE, F3+]`
- El footer del SessionMainView tiene un botón clip (icono Paperclip) puesto visualmente pero `disabled`.
- Hace falta: endpoint backend que acepte multipart (archivos) y los inyecte como contexto al PTY (¿escribir paths en el stdin? ¿exponer un "drop zone" que el agente lea via tool MCP?).
- **Decidir el mecanismo en F3** (cuando el orchestrator pueda pedir adjuntos como parte de una task). Posibles caminos:
  - (a) Endpoint `POST /api/sessions/:sid/attach` que copia el archivo a `$HARNESS_HOME/.runtime/attach/<sid>/<name>` y emite SSE `session.attachment` con el path; el agente lo lee del FS.
  - (b) Endpoint que mete el path como texto en stdin (más sucio; depende del CLI).
- Habilitar el botón cuando el mecanismo esté claro.

### N4 · Auth re-login dentro del container `[PENDIENTE]`
- Si el bind-mount de `~/.claude/` es del host y el CLI hace refresh de token, ¿escribe sobre el host?
- **Propuesta**: bind-mount RW por default; el container y el host comparten `~/.claude/` literalmente (el host no debe usar `claude` con otra cuenta en paralelo).
- Alternativa: copy-on-launch dentro del container; trade-off es perder refresh tokens al destruir el container.

---

## Reglas de cierre

1. Discutir con el usuario o tomar decisión documentada.
2. Mover a [[build-plan/decisions-locked]] con razón.
3. Marcar `[RESUELTA]` aquí con link a donde quedó.
4. Si afecta shards ya escritos, parchearlos.

## Estado de cierre

**Resueltas**: Q1, Q6, Q7, Q8, Q10, Q12, Q15, Q16, Q17, Q20, N2 (11). Q7 cerrada con spike F2 (claude OK, codex deferred).
**Pendientes originales**: Q2, Q3, Q4, Q5, Q9, Q11, Q13, Q14, Q18, Q19 (10 de 20).
**Nuevas pendientes**: N1, N3, N4, N5, N6, N7 (6).

**Total pendiente**: **16** preguntas.
**Críticas/bloqueantes**: **Q9** (matriz roles × tools, antes de F3 — propuesta ya en chat, falta confirmar y persistir aquí).
