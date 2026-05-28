---
id: agents/spawn-lifecycle
title: Lifecycle del spawn (efímero)
shard: 13-agents
tags: [spawn, lifecycle, lease, recovery]
summary: Un agente vive lo que dura su task. Mecánica de spawn, lease, recovery.
related: [agents/overview, agents/smart-loading, harness-core/thread-lifecycle, foundations/lessons-learned]
sources: [foundations/lessons-learned]
---

# Lifecycle del spawn

## Principio
Un **spawn** = un proceso `claude`/`codex` lanzado para resolver **una** task. Vive lo que dura. Al `done` (o `abandoned`/`fail`), muere. Si la task se re-asigna → spawn **nuevo**, no resucitamos.

Ventajas:
- Aislamiento perfecto entre tasks.
- Contexto siempre limpio.
- Recovery trivial.
- No hay "pool de agentes" que dimensionar.

## Estados del spawn

```
pending ─► launching ─► running ─► (done | failed | killed)
               │            │
               │ crash      │ timeout/cancel
               └────────────┴───► failed
```

| Estado | Significado |
|---|---|
| `pending` | Scheduler decidió usarlo; aún no se llamó al `Command::spawn` |
| `launching` | Proceso arrancando (cargando MCP server, prompt template, skills) |
| `running` | Activo; lease vigente |
| `done` | La task se marcó `pending_verify` o `done` por el agente |
| `failed` | Crash (ver `exit_code`, `signal`) |
| `killed` | Cancelación externa (humano, timeout, budget cap) |

## Datos persistidos por spawn

Bajo `~/.harness/profiles/<p>/threads/<tid>/spawns/<spawn_id>/`:

```
meta.toml          # agente, task asignada, capacidades cargadas, started_at, finished_at, exit
mcp_config.json    # config inyectado al CLI (--mcp-config)
output.log         # PTY raw (rotación a 50 MiB)
events.jsonl       # eventos del spawn (item.started/delta/completed)
```

`meta.toml` ejemplo:
```toml
spawn_id      = "01HX8E..."
agent         = "frontend"
task_id       = "T-0042"
cli           = "claude"
started_at    = "2026-05-26T11:00:00Z"
finished_at   = "2026-05-26T11:08:42Z"
exit_code     = 0

[loaded_capabilities]
mcp           = ["harness-bridge"]
skill_tags    = ["svelte"]
tools         = ["task.*", "spec.read", "shell.exec"]

[lease]
acquired_at   = "2026-05-26T11:00:01Z"
ttl_s         = 300
renewed_count = 12
released_at   = "2026-05-26T11:08:40Z"
```

## Lease + heartbeat

- Al `launching`, scheduler adquiere `claim_lease` en la task TOML con `holder = spawn_id`.
- El spawn (vía MCP `task.renew`) refresca cada `ttl/2` = 150s default.
- Si crashea sin release:
  - Lease expira tras TTL.
  - `assignee` se mueve a `previous_assignees[]`.
  - Task queda `in_progress` con `assignee = null` hasta otro claim.
  - Tras `expire + grace (30 min)` sin nuevo claim → scheduler la mueve a `queued` con `notes.recovered_from_crash = true`.

## Recovery del harness mismo (no del spawn)

Si el `harness-server` crashea durante un spawn activo:
1. El spawn (proceso `claude`) puede seguir vivo unos segundos hasta detectar pipe roto.
2. Al re-arrancar el `harness-server`:
   - Lee `~/.harness/.../spawns/*/meta.toml`.
   - Cualquier spawn sin `finished_at` se marca `killed` con causa `harness-restart`.
   - El proceso del CLI se mata si sigue vivo (PID en meta).
   - La task vuelve al pool según las reglas de lease.

## Bootstrap del spawn (paso a paso)

1. **Resolver capabilities**: combinar `spawn_hint` de la task + defaults del agente. Validar contra el shard del agente (ver [[agents/smart-loading]]).
2. **Generar `mcp_config.json`** apuntando a la instancia del `harness-mcp-server` para este spawn.
3. **Generar prompt inicial**: base prompt del agente + fragmento de dominio + skills cargadas + spec.md slice + task TOML.
4. **Lanzar MCP server** como child del backend (stdio).
5. **Lanzar CLI** (`claude` o `codex`) con `--mcp-config <ruta>` y el prompt como primer mensaje.
6. **Adquirir lease** sobre la task.
7. Stream del PTY → SSE → UI; eventos estructurados → `events.jsonl`.

## Subagentes iniciados por agentes

Un agente activo puede iniciar subagentes cuando el trabajo lo justifica. Esto
no es exclusivo de Zeus: cualquier sesión conectada al bridge MCP y autorizada
puede pedir un `session_spawn_child` para abrir una sesión hija bajo su
`parent_session_id`.

Uso esperado:
- El agente padre detecta un subproblema separable (tests, refactor mecánico,
  revisión DB, frontend puntual).
- Spawnea una hija con rol, CLI sugerido y prompt acotado.
- La hija hereda el thread/cwd salvo override explícito.
- El padre sigue siendo responsable de integrar, validar y cancelar la hija si
  se sale de scope.

Reglas:
- Registrar relación `parent_session_id` / `root_session_id` en metadata.
- Aplicar los mismos caps de budget y concurrencia del thread al árbol entero.
- No contar una hija como "done" solo porque fue spawneada; el padre debe leer
  su estado/salida y cerrar el handoff.
- La UI debe mostrar las hijas activas en el panel de Agents del padre. Bug
  conocido 2026-05-27: Claude pudo spawnear Codex y Codex quedó trabajando,
  pero el panel derecho no mostró el agente activo.

## Cancelación

- Humano: `DELETE /api/spawns/:id` o "Stop" en UI.
- Budget hard cap: scheduler kill-ea todos los spawns del thread.
- Re-plan (K=2 alcanzado): la task pasa a `abandoned`, sus spawns activos se kill-ean.
- Cancelación graceful: SIGINT, espera 3s, luego SIGKILL.

## Concurrency cap

- Por thread: `budget.max_concurrent_spawns` (default **3**).
- Por host: `runtime.max_concurrent_spawns_total` (default **6**).
- Scheduler respeta ambos; tasks esperan si caps alcanzados.

## Anti-patrones

| Mal | Bien |
|---|---|
| Reusar un spawn para varias tasks | Spawn fresh por task |
| Mantener pool de agentes vivos "por las dudas" | Spawn on-demand |
| Cargar todas las capacidades del agente por default | `spawn_hint` define solo lo necesario |
| Crashear el spawn → marcar task `failed` | Re-asignar con nuevo spawn (hasta cap N) |
| Lease sin TTL | TTL obligatorio + heartbeat |
