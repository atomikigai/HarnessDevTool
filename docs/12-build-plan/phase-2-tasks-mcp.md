---
id: build-plan/phase-2-tasks-mcp
title: F2 — Tasks + MCP bridge
shard: 12-build-plan
tags: [phase, f2, tasks, mcp]
summary: Máquina de tareas TOML completa; el CLI hijo claim/update vía MCP server local.
related: [build-plan/phase-1-sessions, build-plan/phase-3-team, foundations/lessons-learned, harness-core/mcp-integration]
sources: []
---

# F2 — Tasks + MCP bridge

## Meta
Implementar la máquina de tareas como en [[foundations/lessons-learned]] §D y exponerla al `claude`/`codex` hijo a través de un **MCP server local** que corre como child del backend. El agente puede `task.claim`, `task.update`, `skills.search`, `spec.read`. Sin equipo aún (un solo agente activo).

## Entregables

### Backend — task engine
- [ ] `harness-core::tasks`:
  - [ ] Struct `Task` mapeada al TOML de §D1.
  - [ ] State machine implementada con transiciones validadas (§D2).
  - [ ] `claim(task_id, agent_id, ttl)` con lock por file flock + lease en TOML.
  - [ ] `renew_lease`, `release_lease`.
  - [ ] Heartbeat watcher (loop ticks 30s, marca leases expirados).
  - [ ] Auto-unblock al detectar `done` en deps.
- [ ] Storage:
  - [ ] 1 archivo TOML por task bajo `~/.harness/profiles/<p>/threads/<id>/tasks/`.
  - [ ] `tasks/index.db` (SQLite) con columnas para queries rápidos.
  - [ ] Schema versionado `task.v1.json` en `harness-core/schemas/`.
- [ ] Validación: cualquier lectura valida contra el JSON Schema; falla loud.

### Backend — endpoints
- [ ] `GET /api/threads/:id/tasks?status=...&label=...` → lista.
- [ ] `POST /api/threads/:id/tasks` (crea task; humano o agente).
- [ ] `GET /api/threads/:id/tasks/:tid`.
- [ ] `PATCH /api/threads/:id/tasks/:tid` (transición de estado, claim, update, etc.).
- [ ] `DELETE /api/threads/:id/tasks/:tid` (a `abandoned`, solo humano).
- [ ] SSE emite `task.changed { task_id, prev_status, next_status, by }`.

### Backend — MCP server
- [ ] Crate **`harness-mcp-server`**:
  - [ ] Sub-binario `harness-mcp-server` o modo del binario principal (`harness-server mcp`).
  - [ ] Transport: stdio JSONL (MCP spec).
  - [ ] Tools registradas:
    - `task.list` { thread_id, filters }
    - `task.get` { task_id }
    - `task.claim` { task_id, agent_id, ttl_s }
    - `task.renew` { task_id, agent_id }
    - `task.update` { task_id, patch }
    - `task.release` { task_id, agent_id }
    - `task.submit` { task_id, artifacts } → `pending_verify`
    - `spec.read` { thread_id, scope? }
    - `skills.search` { query, top_k }   // devuelve [] hasta F5
  - [ ] Cada tool valida params, llama al core, devuelve resultado MCP.
- [ ] El `harness-session::Manager` al hacer `spawn(claude|codex|cursor, ...)`:
  - [ ] Lanza el MCP server como child del child (o como peer process).
  - [ ] Inyecta `--mcp-config` al CLI con un JSON apuntando al socket/stdin.
  - [ ] Esto requiere **un MCP server por sesión** (o uno compartido con auth por session token).
  - [ ] **Bypass de permissions del CLI hijo**: el CLI se arranca con su flag de skip-approval (ej. `claude --dangerously-skip-permissions`), de modo que las tools MCP que el harness expone se ejecutan sin prompts del CLI. El control vive en `harness-sandbox` + el set de tools que el bridge decide registrar (ver [[build-plan/decisions-locked]] → "Permissions del CLI hijo").

### Backend — scheduler básico
- [ ] Loop background: cada 2s recoge tasks `queued` con `blocked_by∅` y notifica vía SSE (no asigna automáticamente; en F2 el humano dispara los workers).

### Frontend
- [ ] Página `/threads/[id]/tasks/+page.svelte`:
  - [ ] Tabla virtualizada con todas las tasks del thread.
  - [ ] Columnas: id, title, status, assignee, updated_at, blocked_by.
  - [ ] Filtros por status + label.
  - [ ] Click → drawer/modal con `<TaskDetail>` que muestra el TOML render + acceptance checks + history.
- [ ] `<TaskGraph>`: vista DAG opcional usando `@xyflow/svelte` o `d3-dag`. Muestra dependencias.
- [ ] Crear task manual: `<TaskCreateForm>` con valibot validation.
- [ ] Editor in-place del TOML (CodeMirror) con valibot pre-save check.

### Identidad de agentes
- [ ] `~/.harness/profiles/<p>/agents/registry.toml` (ver [[foundations/lessons-learned]] §E1).
- [ ] Endpoint `GET/POST /api/agents`.
- [ ] Cada `session.spawn` referencia un `agent_id` del registry.

## Test de aceptación

1. Crear thread + `spec.md` (manual).
2. Crear task `T-0001` con `acceptance.checks = ["..."]`.
3. Spawn `claude` con MCP-config apuntado al server local.
4. Dentro de `claude`, el modelo debe poder llamar `task.list` y ver `T-0001`.
5. `claude` llama `task.claim T-0001` → en disco el TOML cambia `status=in_progress`, `claim_lease.holder=agent:claude-1`.
6. Esperar 5min sin renew → lease expira; CLI puede `task.list` y `T-0001` aparece sin assignee activo aunque sigue `in_progress`.
7. UI muestra todos esos cambios en vivo vía SSE.
8. PATCH manual desde UI: `T-0001 → paused` con `why_paused` → backend valida y persiste.

## Lo que NO está en F2
- Roles (planner/generator/evaluator).
- Scheduler que asigna automáticamente a workers libres (F3).
- Budget hard-cap (F3).
- Skills (F5).
- Tools del agente sobre módulos DB/SSH (F4).

## Riesgos
- **Configurar el MCP en `claude`/`codex`**: cada CLI tiene su propio formato (`--mcp-config` con JSON inline vs path). Validar **temprano** con un MCP "hello world".
- **Auth del MCP local**: si solo el child habla con él por stdio, no hay riesgo de exposure. Pero si exponemos HTTP, requiere token. Mantener stdio por ahora.
- **Lease + flock cross-platform**: `flock` no existe nativo en Windows; usar `fs2` crate o lock-file convention.
- **TOML round-trip**: serde + toml puede reordenar campos al re-serializar. Usar `toml_edit` para preservar formato/comentarios del usuario.

## Decisiones a confirmar
- ¿Un MCP server **por sesión** o **uno global** con session token? Recomiendo **uno por sesión** (aislamiento, simpler), aunque consume más memoria.
- ¿`skills.search` devuelve `[]` o falla `not_implemented` hasta F5? **Devuelve `[]`** para no romper agentes que ya la usen.
