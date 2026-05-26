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

> Lo que **no** está bloqueado en [[build-plan/decisions-locked]]. Cada pregunta indica la fase donde se vuelve crítica.

## Cross-cutting (impactan F0+)

### Q1 · Identidad del usuario
- ¿Hay concepto de "usuario logueado" en single-user local? Probablemente no — el usuario del SO es la identidad.
- Pero si hay `profiles/`, ¿el profile activo está atado a una sesión del browser (cookie) o al global del backend?
- **Sugerencia**: profile activo es global del backend; cambiar profile en UI = restart suave del server. Decidir antes de F0.

### Q2 · `AGENTS.md` snapshot al iniciar thread
- ¿Snapshot del `AGENTS.md` del **repo del usuario** o del propio harness?
- ¿Cómo encuentra el harness el `AGENTS.md` cuando los `claude`/`codex` corren con un `cwd` arbitrario?
- **Pendiente**: definir resolución (git root del cwd, fallback a `$HOME/AGENTS.md`?).

### Q3 · Logging cross-process
- El harness backend tiene su propio `tracing`. El `claude`/`codex` hijo tiene los suyos.
- ¿Cómo correlacionamos un span del backend con un PTY output del child?
- **Sugerencia**: cada session_id ID inyectado en `X-Session-Id` y propagado a logs del backend. Los logs del CLI quedan separados pero con timestamp para correlación visual.

## F1 — Sesiones

### Q4 · Múltiples sesiones simultáneas en UI
- F1 dice "una activa". ¿Permitimos lista + tabs desde F1 o esperamos a F3?
- **Sugerencia**: F1 = una sesión por vez en la vista; lista en sidebar muestra activas pero solo una abierta. Multi-tab en F3.

### Q5 · CLI desconocidos (no `claude` ni `codex`)
- ¿Soportamos otros CLIs agénticos (aider, cursor-cli, etc.) desde F1?
- **Sugerencia**: F1 hardcodea dos opciones; F4 generaliza a `agent_kind: "custom"` con plantilla.

### Q6 · Persistencia del PTY raw
- ¿Cuánto guardamos del `output.log` (PTY raw)?
- 50 MiB por sesión es agresivo si hay muchas sesiones largas.
- **Sugerencia**: 50 MiB con rotación zstd; compresión típica ANSI ~10x → ~5 MiB físico.

## F2 — Tasks + MCP

### Q7 · ¿Cómo se descubre el MCP server desde `claude`/`codex`?
- `claude` admite `--mcp-config` con JSON. `codex` aún incierto (revisar versión actual).
- ¿El backend escribe un JSON temporal por sesión y pasa el path?
- **Sugerencia**: archivo temporal en `~/.harness/.runtime/sessions/<sid>/mcp.json`, limpieza al kill.

### Q8 · Granularidad de tasks creadas por el planner
- F3 propone "≤6 acceptance.checks por task". ¿Forzamos esto en validation o solo lo sugerimos en prompt?
- **Sugerencia**: warning en validation (no error), métrica para feedback al planner.

### Q9 · Permisos por rol en MCP tools
- F3 introduce `enabled_tools` / `disabled_tools` por rol.
- ¿El planner puede `task.create` pero no `task.claim`? ¿El generator al revés?
- **Pendiente**: definir matriz roles × tools antes de F3.

## F3 — Equipo

### Q10 · Roles concurrentes del mismo tipo
- ¿Cuántos generators simultáneos por default? F3 sugiere `max_concurrent_workers=3`.
- ¿Es por thread o global?
- **Sugerencia**: por thread; el budget global limita el total.

### Q11 · Spec.md "lock" vs concurrencia
- ¿El planner puede editar `spec.md` mientras hay workers activos?
- **Sugerencia**: spec append-only durante un thread activo; sólo el planner edita pero respeta ordering.

### Q12 · Recovery de un agente muerto
- Si un `claude` child crashea mid-task, ¿qué pasa?
- Lease expira tras TTL pero la task queda `in_progress`. ¿Auto-mover a `queued`? ¿Pedir intervención humana?
- **Sugerencia**: tras TTL+5min sin renew, scheduler emite warning. Tras TTL+30min, auto-pasa a `queued` con entry en `notes.recovered_from_crash`.

## F4 — Módulos

### Q13 · Multi-tab queries y conexiones
- ¿Cada tab "Editor SQL" comparte conexión del pool o usa su propia?
- **Sugerencia**: comparten; el pool gestiona.

### Q14 · SFTP transfer policies
- ¿Por defecto `overwrite`, `skip`, `resume`, o `ask`?
- **Sugerencia**: `resume` por default; UI permite override por batch.

## F5 — Skills

### Q15 · ¿`memory.search` vs `skills.search` — diferencia clara?
- Memory = qué pasó en threads pasados. Skills = cómo se hace algo.
- ¿Hay overlap? El agente puede confundir cuándo usar cuál.
- **Sugerencia**: documentar diferencia en el prompt-template de cada rol.

### Q16 · ¿Cuándo el learner "promueve" automáticamente vs siempre proposed?
- F5 fija "siempre proposed" inicialmente.
- ¿En F6 abrimos una política `auto-promote-if-success-rate > N`?
- **Pendiente**: decidir en F6.

### Q17 · Skills compartibles entre perfiles
- Si el usuario tiene perfiles `personal` y `work`, ¿las skills viven aisladas?
- **Sugerencia**: aisladas, pero con comando `harness skills copy <id> --from personal --to work`.

## F6 — Polish

### Q18 · Tasks-target reproducibles para GEPA
- ¿Cómo se construye este set? ¿Generated o curated?
- **Pendiente**: definir formato + responsable.

### Q19 · Distribución
- ¿Docker images en ghcr público o privado?
- ¿Self-host instructions vs hub público?
- **Pendiente** F6.

### Q20 · IDE integration (ACP-style)
- Hermes tiene `acp_adapter/` para VSCode/Zed.
- ¿Fuera de scope o stretch post-F6?
- **Default**: fuera de scope hasta haber estabilizado todo lo demás.

---

## Cómo se cierra una pregunta abierta

1. Discutirla con el usuario o tomar decisión y documentar.
2. Mover entry a [[build-plan/decisions-locked]] con `Razón`.
3. Eliminar de este shard (o dejar tombstone con link).
4. Si afecta a un shard ya escrito, parchearlo en la misma sesión.
