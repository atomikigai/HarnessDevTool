---
id: build-plan/super-harness-plan-2026-06-12
title: Plan Super-harness 2026-06
shard: 12-build-plan
tags: [plan, super-harness, tool-loading, ssh, db, context-engine, roadmap]
summary: Plan maestro "Super-harness 2026-06" — síntesis post-análisis de dos harnesses de referencia (pi y hermes-agent). Workstreams W1–W5 para tool loading 2.0, SSH/DB completo, context engine mejorado y residuales de perf.
related: [build-plan/improvement-plan, build-plan/pending-implementation-tasks, teamwork/BOARD]
sources: []
---

# Plan Super-harness 2026-06

> **Síntesis 2026-06-12**: tras analizar dos harnesses de referencia (earendil-works/pi y nousresearch/hermes-agent)
> en `/tmp/ref-pi` y `/tmp/ref-hermes`, el Planner ha consolidado hallazgos en un plan maestro de 5 workstreams (W1–W5).
> **Objetivo del usuario**: harness rápido, eficiente, capaz de tareas grandes sin saturarse, con tool
> loading *just-in-time* y liberación de recursos, módulos SSH/DB con conexiones reutilizables y contexto abierto.
> Referencia de arquitectura: ver `improvement-plan.md` §P3 (autonomía y gateway MCP).

---

## Motivación

El usuario busca un harness que:

1. **No se sature de contexto**: tool loading selectivo y lazy (cargar solo lo que se usa, liberar cuando no).
2. **Sea rápido**: evitar rescans costosos, caché inteligente, evitar bufferización innecesaria.
3. **Maneje tareas de todos los tamaños**: desde cambios rápidos (segundos) hasta apps completas (horas).
4. **Exponga módulos SSH/DB con continuidad**: sesiones SSH con ControlMaster/reuso, contexto pre-inyectado al spawn (schema summary, host introspection).
5. **Resuma sin perder detalle**: checkpoints estructurados (Goal/Progress/Decisions/Key Context) que el agente pueda reutilizar al reanudar.

**Patrón observado en referencias**: compaction iterativa (pi), toolsets composables (hermes), trajectory compression, memory manager async, lazy-loading de capacidades.

---

## Hallazgos de los repos de referencia

### pi (earendil-works/pi, TypeScript)

**Compaction iterativa y resúmenes estructurados:**
- Thresholds duales (`reserveTokens 16K` / `keepRecentTokens 20K`), cut points que respetan turn boundaries.
- Resúmenes que se **actualizan** (no regeneran) en cada compaction (`packages/agent/src/harness/compaction/compaction.ts`).
- Formato de resumen fijo estructurado: **Goal** / **Constraints** / **Progress** (Done/In Progress/Blocked) / **Key Decisions** / **Next Steps** / **Critical Context** + tracking de `readFiles[]` / `modifiedFiles[]`.
- Branch summarization al navegar ramas de sesión; split-turn summarization.

**Tool output truncation dual:**
- Límites independientes de **líneas** (2000) y **bytes** (50KB), distintos patrones por tipo (head-truncation para reads, tail-truncation para bash).
- UTF-8 safe (`utils/truncate.ts`); nunca corta a mitad de codepoint.

**Ejecución paralela y caching:**
- Tool execution paralela **por defecto** con override secuencial per-tool.
- Prompt caching con `cache_control` de Anthropic.
- System prompt como función async (dinámico por turno).
- Extensiones TypeScript cargadas en runtime con event hooks.

### hermes-agent (nousresearch/hermes-agent, Python)

**Toolsets composables y resolución dinámica:**
- `TOOLSETS` dict con **includes recursivos cycle-safe** (`toolsets.py resolve_toolset`).
- Filtrado dinámico por `check_fn(agent)` — cada agente ve solo sus tools activas.
- Caché LRU de schemas keyed por `(toolsets, registry_generation, config_mtime)` (`model_tools.py`).
- Toolset "base" siempre presente; toolsets extra se cargan on-demand o al spawn según rol/contexto.

**Trajectory compression — "protect-head-tail-compress-middle":**
- Protege system message + primer turno + **últimos 4 turnos** (borde caliente).
- Comprime el **medio** a un mensaje resumen generado por LLM auxiliar barato (`trajectory_compressor.py`).
- **~75% ahorro de input tokens** con mínima pérdida de contexto.
- Resumen incluye eventos clave, herramientas invocadas y transiciones de estado.

**Memory manager plugin async:**
- Prefetch **pre-turno**: inyecta context en `<memory-context>` fences sin bloquear.
- Sync **post-turno**: background async, nunca bloquea el turno.
- Caché persistent basada en embeddings + BM25 para retrieval rápido.

**SSH con reuso y sync de skills:**
- `ControlMaster` + `ControlPersist=300s` — reusan la conexión.
- Bulk upload vía tar pipe + sync de skills al remoto (`tools/environments/ssh.py`).
- Checkpoints con git shadow store (versionado local del estado remoto).
- SQLite WAL + retry con jitter para durabilidad.

---

## Estado actual del harness (gaps)

### Lazy loading de capacidades (parcial, mejorable)

**Existe**: `SmartCapabilitySignals` con scoring (role=5, scopes=4, cwd=2, prompt=1) en `harness-server/src/routes/sessions.rs`; `capability_profile` Auto/None/Harness/HarnessCrawl4ai.

**Problema**: el MCP server expone **60+ tools fijas** a todo agente, no hay unload en runtime, `LoadedCapabilities` es inmutable post-spawn.

**Solución**: Workstream **W1** (ver abajo) reemplaza con registry de **toolsets composables**, meta-tools `tools_load`/`tools_unload`/`tools_search`, y auto-load on-call.

### Knowledge ingestion (existe, sin retrieval)

**Existe**: PDF vía `pdf_oxide`, DOCX/PPTX vía `undoc`, shards 2-7.5KB en `harness-core/src/knowledge.rs`.

**Falta**: retrieval/búsqueda sobre shards; ingesta xlsx/csv a knowledge.

**Solución**: Workstream **W4** agrega FTS5 sobre shards con `knowledge_search` tool.

### SSH (module-ssh) — slice inicial usable, pendientes claros

**Existe** (desde 2026-06-04): crate `module-ssh`, manager + 8 tools MCP + tipos TOFU, REST endpoints, UI `/ssh` y `/ssh/[host]`.

**Falta**:
1. Transfer queue con progreso/resume.
2. Sesiones SSH interactivas.
3. `ssh_context`: introspección del host al conectar (os-release, hostname, servicios, docker ps, dirs clave, package manager) → brief pre-inyectable al agente.
4. Reuso via ControlMaster.

**Solución**: Workstream **W2** (pendiente explícito del usuario).

### DB (module-db) — completo, perf degradada

**Existe**: integración completa, REST, MCP tools, UI `/db/[id]`.

**Problemas**:
- N+1 en introspección sin caché.
- Export bufferiza toda tabla ×2 en memoria, paginación OFFSET O(n²).
- `db_context` no existe: al spawn sobre una conexión no hay resumen de schema/relaciones.

**Solución**: Workstream **W3** (schema summary + caché + keyset pagination).

### Context governor — solo checkpoint/clear, sin resúmenes estructurados

**Existe**: dispara checkpoint (35%) / clear (40%) cuando se alcanza threshold de tokens.

**Falta**: 
- Resúmenes estructurados en el checkpoint (Goal/Progress/Decisions/Next Steps + file tracking).
- Truncación dual (líneas+bytes) en tool results del MCP server.
- `knowledge_search` FTS5 en lugar de volcado completo de shards.

**Solución**: Workstream **W4**.

---

## Workstreams

### W1 — Tool loading 2.0 (EN CURSO, abrir en BOARD.md)

**Objetivo:** reemplazar la exposición fija de 60+ tools con un registry de **toolsets composables** que se cargan/descargan en runtime por sesión.

**Inspiración:** hermes-agent (`toolsets.py` con includes recursivos cycle-safe + LRU de schemas).

**Solución:**

1. Registry de toolsets en `harness-mcp-server`:
   - Grupos: `core` (tasks/spec/session/mailbox + meta-tools), `repo`, `knowledge`, `db`, `ssh`, `skills`, `docs`.
   - Soporte para `includes` recursivos cycle-safe (p.ej. `db` incluye `repo` para queries).
   - Resolución en `resolve_smart_tool_groups` en `harness-server/routes/sessions.rs` (scoring existente mejorado).

2. `tools/list` dinámico:
   - Devuelve solo `core` + grupos activos según `LoadedCapabilities.tool_groups` del `SessionMeta` (semilla al spawn).
   - Caché de schemas por set activo (recompute solo al cambiar set).

3. Meta-tools (siempre presentes):
   - `tools_search(query: string)` → tools/grupos disponibles con descripción y grupo.
   - `tools_load(groups: [string])` → actualiza set activo, emite `notifications/tools/list_changed` por stdio.
   - `tools_unload(groups: [string])` → lo mismo.

4. Auto-load on-call:
   - Si se llama una tool de grupo no cargado → auto-load del grupo + ejecutar.
   - Nota en el resultado para clientes que ignoren `list_changed`.
   - Gating de seguridad sigue siendo per-call, no la visibilidad.

**Criterio de aceptación:**
- Sesión sin grupos extra: `tools/list` expone ≤20 tools (core + meta) en lugar de 60+.
- `tools_load(["db"])` emite `list_changed` y siguiente `tools/list` incluye `db_*`; `tools_unload` las quita.
- Llamar `db_query` sin grupo cargado funciona (auto-load), resultado lo anota.
- Resolución de grupos con includes anidados, detección de ciclos, tests.
- `tools_search("export csv")` devuelve `db_export_table`.
- `cargo test -p harness-mcp-server`, `just test` verdes; smoke JSON-RPC.

**Estimación:** M–L; requisito para W2 y W3.

---

### W2 — SSH completo + remote context pack (pendiente explícito del usuario)

**Objetivo:** harness SSH maduro con continuidad de contexto y reuso de conexiones.

**Inspiración:** hermes-agent `tools/environments/ssh.py` (ControlMaster, tar bulk upload, skills sync).

**Solución:**

1. **ControlMaster + reuso de conexiones:**
   - `ControlMaster auto` + `ControlPersist 300s` en config SSH.
   - Evita auth recurrente y overhead de handshake.

2. **Transfer queue con progreso:**
   - Cola de transferencias (SFTP up/down) con pause/resume/cancel.
   - Resume real (reutiliza sesión SSH abierta, bytes offset).
   - Progreso visible en API y UI.

3. **`ssh_context` — introspección pre-spawn:**
   - Al conectar, introspección del host (os-release, hostname, servicios, docker ps, dirs clave, package manager) cacheada por host.
   - Brief inyectable al agente ("Host Linux Debian, servicios: postgresql, redis, docker").
   - Resultado exportable como JSON en `~/.harness/profiles/<p>/ssh-contexts/<host>.json`.

4. **Spawn de agente "sobre" SSH:**
   - Agente se crea con grupo `ssh` precargado.
   - Contexto pack inyectado en system prompt.
   - Tools SSH elevadas (exec, sftp) disponibles sin extra load.

**Criterio de aceptación:**
- Guardar hosts, conectar, ejecutar comando no-interactivo, listar/subir/bajar archivos (funciona hoy).
- Transfer queue con progreso (barra % en UI), resume tras desconexión.
- `ssh_context` generado en primer uso, cacheado por mtime del host.
- Spawn con grupo `ssh` + context pack → agente "ve" el host sin re-introspeccionar.
- Tests de ControlMaster reuso (2× comando en sesión ≈ 1× handshake), transfer resume, context pack.

**Estimación:** L; requiere W1 (toolsets).

---

### W3 — DB context pack + perf

**Objetivo:** contexto pre-inyectado al spawn sobre una conexión, y perf en introspección/export.

**Inspiración:** tabla de relaciones pre-cargada en hermes, cacheo en memoria.

**Solución:**

1. **`db_context` por conexión:**
   - Al seleccionar/conectar a una BD: schema summary (tablas, relaciones, row counts aproximados, índices clave).
   - Cacheado por `(engine, host, database)` en `~/.harness/profiles/<p>/db-contexts/<conn_id>.json`.
   - Brief inyectable al spawn: "DB Postgres app_db: 8 tablas (users, posts, comments, …), 15K filas, PK: id, FK users→posts."

2. **Caché de introspección:**
   - Batch de llamadas a `information_schema` / `pg_catalog` por conexión.
   - `SchemaTree` en memoria, mtime-invalidación.
   - Fix N+1 en `primary_key_cols` — traer solo PK de tabla objetivo en lugar de introspeccionar todo.

3. **Keyset pagination en export:**
   - Reemplazar `OFFSET` con keyset (last PK visto).
   - Streamer chunks al sink, no bufferiza todo en memoria.
   - Fix order inconsistente (agregar `ORDER BY` implícito).

4. **Read-only transaccional:**
   - `SET TRANSACTION READ ONLY` en lugar de keyword-check (PG/MySQL/SQLite).
   - Bloquea `WITH DELETE`, `EXPLAIN ANALYZE INSERT`, etc.

**Criterio de aceptación:**
- `db_context` generado en primer uso, cacheado.
- Spawn sobre conexión con grupo `db` → context pack inyectado automáticamente.
- Export de tabla grande (1M filas) no bufferiza, devuelve chunks progresivamente.
- Introspección 2× más rápida con caché (benchmark: lista de schemas antes/después).
- Read-only falla cerrado: `DROP TABLE`, `WITH DELETE`, `PRAGMA query_only` sí se aplica.
- Tests de caché invalidación, keyset pagination, transacción read-only.

**Estimación:** M; requiere W1 (toolsets).

---

### W4 — Context engine v2

**Objetivo:** resúmenes estructurados, retrieval FTS5 sobre knowledge, truncación dual inteligente.

**Inspiración:** pi (formato Goal/Progress/Decisions/Next + file tracking), hermes (trajectory compression).

**Solución:**

1. **Checkpoint con formato estructurado:**
   - En lugar de volcado de token: **Goal**, **Constraints**, **Progress** (Done/In Progress/Blocked), **Key Decisions**, **Next Steps**, **Critical Context**.
   - Tracking de `readFiles[]` / `modifiedFiles[]`.
   - Actualización (no regeneración) de resumen en cada checkpoint: merge de nuevas acciones en `Progress`, remove de `Done` si vuelve a aparecer, etc.

2. **`knowledge_search` FTS5:**
   - Indexar shards de knowledge en FTS5 por thread/profile.
   - Tool `knowledge_search(query)` devuelve top-5 shards relevantes (BM25) + snippet.
   - Ingesta xlsx/csv a knowledge (conversión a shards 2-7.5KB).
   - Evita volcado completo de 50+ shards al prompt.

3. **Truncación dual en MCP results:**
   - `tools/call` response: límites independientes de líneas (2000) y bytes (50KB).
   - Patrones distintos: head-truncation para reads, tail-truncation para bash, UTF-8 safe.
   - Aplicar al servidor antes de devolver al CLI.

4. **Memory manager plugin async:**
   - Pre-turno: prefetch context (vía `knowledge_search` o embeddings) sin bloquear.
   - Post-turno: background sync a SQLite FTS5, nunca bloquea.
   - Helper Rust en `harness-core` para inyectar context en prompts.

**Criterio de aceptación:**
- Checkpoint con formato Goal/Progress/Decisions/Next/Critical + file tracking.
- Update de checkpoint (not regen): nuevas acciones merge en sections existentes.
- `knowledge_search("cómo se autentican los usuarios?")` devuelve shards relevantes, no todo.
- CSV ingested → shards en knowledge + searchable.
- Tool output `> 50KB` o `> 2000 líneas` se trunca (head-para-reads, tail-para-bash).
- Benchmark: checkpoint con context mejorado → prompt token budget 15–20% mejor sin perder recall.
- Tests de update de checkpoint, search FTS5, truncación UTF-8.

**Estimación:** M–L; requiere W1 (para contexto inyectable cleanly).

---

### W5 — Residuales de perf (del improvement-plan)

**Objetivo:** cerrar los últimos cuellos de botella identificados en auditoría 2026-06-02.

**Items (referncias improvement-plan.md §P1 / §C):**

| # | Item | Archivo | Causa | Fix | Severidad |
|---|------|---------|-------|-----|-----------|
| T5a | Policy-check timeout 120s→5-10s | `harness-mcp-server/src/dispatcher.rs:486` | Llamada bloqueante a `/api/approvals/check` sin timeout. | Agregar `timeout(Duration::from_secs(5))` en `policy_check`. | MEDIA |
| T5b | Header `X-Protocol-Version` nunca enviado | `frontend/src/api/client.ts:57-65` | Se lee de response pero no se envía en request. | Agregar header de request (valor actual hardcoded o desde `api/types`). | MEDIA |
| T5c | Consolidación de polling frontend | `frontend/src/routes/+page.svelte:162-181`, `IconRail.svelte:42-60` | Dos setInterval sobre mismo `sessionsState`, carrera de aborts. | Mover polling al store (ref-counted `start()`/`stop()`). | ALTA |
| T5d | Caché de `tools/list` en dispatcher | `harness-mcp-server/src/dispatcher.rs` (hoy recomputa per-call) | Regenera schemas de todos los tools en cada `tools/list`. | Caché por `(active_toolsets, registry_version)` con invalidación al `tools_load`/`tools_unload`. | MEDIA |
| T5e | Compresión en SSE para `/events` | `harness-server/app.rs:36` | Compression middleware aplica a `text/event-stream`, causando bufferización/retraso. | Excluir `text/event-stream` de compression layer (usar `CompressionLayer::skip_all` o wildcard inverso). | MEDIA |

**Criterio de aceptación:**
- Policy check timeout dispara `GatewayTimeout` (policy denial clara).
- Requests HTTP envían `X-Protocol-Version: v1` (o el valor actual).
- Polling frontend consolidado en store, ref-counted, cero carreras.
- `tools/list` no regenera schemas si `active_toolsets` no cambió (medir: 20 `tools/list` en loop ≤ tiempo de 1).
- SSE `/events` y `/api/sessions/.../transcript` no bufferizan por compresión.
- Benchmark: end-to-end test (planner spawn → worker task → approval → result) ≤ 200ms δ promedio vs hoy.
- Tests de timeout, header, store consolidation, caché invalidation, SSE latency.

**Estimación:** S–M; items pequeños, rápidos.

---

## Orden y justificación

```
W1 (tool loading 2.0)
  ├─ Beneficia a TODO agente (incluidos futuros de SSH/DB)
  ├─ Desbloqueador de W2 (SSH context pack) y W3 (DB context pack)
  └─ Base para gatekeeping de seguridad (tool visibility)

W2 (SSH completo)
  ├─ Pendiente explícito del usuario
  ├─ Requiere W1 (toolsets)
  └─ Alto valor: continuidad remota, context pack

W3 (DB context pack + perf)
  ├─ Requiere W1 (toolsets)
  ├─ Perf crítica (N+1, OFFSET O(n²))
  └─ Contexto pre-inyectable → autonomía de agentes sobre BD

W4 (Context engine v2)
  ├─ Requiere W1 (inyección clean de context)
  ├─ Checkpoint estructurado + knowledge_search + truncación dual
  └─ Mayor recall sin saturar tokens (15–20% mejora estimada)

W5 (Residuales perf)
  ├─ Se cuela donde se toque archivo (refactor incremental)
  └─ Quick wins + consolidación polling (ALTA severidad, quick)
```

**Paralelismo:** W1 solamente al principio; W2 y W3 se pueden hacer en paralelo (ambas son W1-dependent, no entre sí); W4 tras W1; W5 incremental.

**Ciclo por tarea:** cada workstream slice sigue el ciclo de `CLAUDE.md §2`:
1. **PLAN** — Planner abre en BOARD.md con objetivo, alcance, archivos, criterio de aceptación.
2. **CODIFY** — Codex genera código (backend+frontend si aplica) con contrato en el board.
3. **REVIEW** — Sonnet revisor + UI designer (si frontend) validan en 1 ronda.
4. **INCORPORATE** — Codex ajusta feedback (cap=1 ronda).
5. **QA** — Codex + agent-browser corre tests e-2-e.
6. **VERIFY** — Planner cierra objetivo cumplido, puerta de calidad verde.

---

## Referencias

- **improvement-plan.md** — auditoría completa 2026-06-02, gaps P0/P1/P2, top-3 por dominio.
- **pi** (earendil-works/pi) — compaction iterativa, resúmenes estructurados, tool truncation dual, caching.
- **hermes-agent** (nousresearch/hermes-agent) — toolsets composables, trajectory compression, memory manager async, SSH ControlMaster.
- **CLAUDE.md §1–§3** — roster de Zeus (Opus/Codex/Sonnet), ciclo cap=1, cross-model review.
- **docs/13-agents/capability-registry.md** — matriz de tools/roles (fuente de verdad para W1 registry).
- **docs/13-agents/zeus-orchestrator.md** — routing del scheduler, autonomy profiles.

---

## Apéndice — Matriz de dependencias e impacto

| Workstream | Depende | Desbloqueador | Impacto (ganancia estimada) |
|---|---|---|---|
| **W1** | — | — | ✓ Visibility base para 60→20 tools; +autonomía |
| **W2** | W1 | ControlMaster + ssh_context + queue | +Continuidad remota, reuso sesión SSH |
| **W3** | W1 | db_context + caché + keyset | +15% query perf; contexto pre-inyectable; N+1 fix |
| **W4** | W1 | checkpoint struct + knowledge_search + truncate | +15–20% token budget sin perder recall |
| **W5** | — | Incremental en cada refactor | +Latencia polling (100ms típico), SSE fluido |

---

## Cronograma estimado (sujeto a cambios)

- **Semana 1–2**: W1 (tool loading 2.0) — Codex + Sonnet review.
- **Semana 2–3**: W2 (SSH) + W3 (DB) en paralelo.
- **Semana 3–4**: W4 (Context engine v2).
- **Semana 4+**: W5 (residuales) incremental + next features (e.g., pipeline Zeus frontend).

**Hito de validación**: end-to-end test (W1+W2+W3+W4 integrados) con Zeus spawn → worker → approvals → artifact con overhead <10% vs baseline.

