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

### Q7 · MCP config format para claude/codex `[CRÍTICA, SPIKE PENDIENTE]`
- Riesgo R1 — bloquea F2 entero.
- ¿`claude` acepta `--mcp-config <file.json>` con nuestro formato? ¿`codex` también?
- **Spike obligatorio en F1**: probar con un MCP "hello world" antes de declarar F1 done.

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

### N2 · Cómo el harness inyecta el prompt inicial al CLI hijo `[PENDIENTE]`
- ¿Lo envía como primer mensaje "user input" al CLI? ¿Como parte del system prompt vía un mecanismo del CLI?
- `claude` admite `--append-system-prompt` y `--system-prompt`. `codex` por confirmar.
- **Spike en F1** junto con Q7.

### N3 · Sandbox de las tools que el CLI ejecuta `[PENDIENTE]`
- `claude` tiene su propio sandbox/approval para `shell.exec`. ¿Necesitamos sandbox adicional desde el harness?
- **Propuesta**: confiamos en el sandbox del CLI hijo para sus tools. Nuestro `harness-sandbox` envuelve solo lo que el harness-bridge ejecuta directamente (raro: la mayoría son rails read-only).
- **Decidir en F3**.

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

**Resueltas**: Q1, Q6, Q8, Q10, Q12, Q15, Q16, Q17, Q20 (9 de 20 originales).
**Pendientes originales**: Q2, Q3, Q4, Q5, Q7, Q9, Q11, Q13, Q14, Q18, Q19 (11 de 20).
**Nuevas del cleanup**: N1, N2, N3, N4 (4).

**Total pendiente**: **15** preguntas.
**Críticas/bloqueantes**: **Q7 + N2** (spike F1) y **Q9** (antes de F2).
