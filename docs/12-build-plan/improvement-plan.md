# Plan de mejoras — Auditoría 2026-06-02

> Auditoría de código completa del harness (backend Rust + frontend SvelteKit)
> ejecutada por el orquestador con 6 revisores en paralelo, uno por dominio:
> `harness-core`, `harness-server`, `harness-mcp-server` + `harness-policy`,
> `harness-session`, `module-db`, y `frontend`.
>
> Cobertura: ~21k LOC Rust (6 crates) + 140 archivos del frontend.
> Cada hallazgo lleva `[SEVERIDAD]` y `[ESFUERZO: S/M/L]` y referencia `archivo:línea`.

## Cómo leer este plan

- **P0 — Seguridad y corrupción de datos**: explotables o que pierden datos. Hacer ya.
- **P1 — Correctitud y rendimiento**: bugs reales y cuellos de botella en caliente
  (incluye el bug conocido de persistencia de sesión).
- **P2 — Calidad, deuda técnica y tests**: mantenibilidad y red de seguridad.

La sección final ("Roadmap por fases") agrupa el trabajo en tandas ejecutables.

---

## Temas transversales (aparecen en ≥2 dominios → máxima prioridad)

| # | Tema | Dominios afectados | Severidad |
|---|------|--------------------|-----------|
| T1 | **Path traversal** vía `thread_id`/`task_id`/`profile id` sin validar, concatenados a rutas | core, server, mcp-server | **ALTA** |
| T2 | **SQL injection** por identificadores sin escapar + gate read-only solo por keyword (bypass con CTE `WITH`) | module-db, mcp-server | **ALTA** |
| T3 | **API sin autenticación** en endpoints mutadores (spawn, PTY input, SQL) | server | **ALTA** |
| T4 | ✅ **Bug de persistencia de sesión** (estado no sobrevive al reabrir/reiniciar) | session (causa raíz), frontend (carrera de selección) | **CERRADO 2026-06-04** |
| T5 | **I/O bloqueante en rutas async** (lecturas de archivo completas, `block_on`, rescans del scheduler) | core, server, session, module-db | **ALTA** |
| T6 | **SSE pierde eventos en silencio bajo lag** (`Lagged` → `None` sin resync) | server, session, frontend | **MEDIA** |
| T7 | **`seq` calculado con `read_events().len()`** → relee todo el log y es racy | server (core lo origina) | **MEDIA** |
| T8 | **Panics por lock poisoning** (`RwLock::expect`) que tumban subsistemas | server, policy, mcp-server | **MEDIA** |
| T9 | **Durabilidad/crash-safety** (sin fsync de directorio, `next_id` no atómico, sin recuperación de logs corruptos) | core | **MEDIA** |
| T10 | **Sin tests** en las zonas más sensibles; frontend con cero infra de test | todos | **MEDIA** |

---

## P0 — Seguridad y corrupción de datos

> **ESTADO (actualizado 2026-06-03): los 10 P0 están CERRADOS.** Verificado contra `main` (código +
> git log). El detalle de cada sección queda como referencia histórica; el estado real es:
>
> | P0 | Estado | Commit(s) |
> |---|---|---|
> | S1 Path traversal | ✅ cerrado | `eca4658` (validate filesystem ids before path joins) |
> | S2 SQL injection + read-only | ✅ cerrado | `a698213` (escape db identifiers) + `9565b68` (tighten read-only gate) |
> | S3 API sin auth | ✅ cerrado | `8421623` (token middleware en rutas mutadoras: `auth::require_api_token`) |
> | S4 Política default-open | ✅ cerrado | `8e1234c` (ask-by-default para tools sensibles; `PolicyEngine` cableado en `/api/approvals/check`) |
> | S5 Panic UTF-8 | ✅ cerrado | `f38513a` (utf8-safe truncation + test) |
> | S6 `next_id` no crash-safe | ✅ cerrado | `f8826d5` (temp+rename) |
> | S7 `events.jsonl` corrupto | ✅ cerrado | `28aab93` (skip-and-warn por línea) |
> | S8 Paste-end injection | ✅ cerrado | `4309c58` (sanitize pty prompt + test) |
> | S9 `server_url` por env | ✅ cerrado | `ffa496f` (trusted mcp url flag) |
> | S10 sqlite ro + log 0600 | ✅ cerrado | `dd7caba` (read-only pools) + `6814e30` (output.log 0600) |
>
> **Residuales de S10 (MEDIA, NO bloqueantes, pendientes):** `DefaultBodyLimit` global y `TimeoutLayer`
> de request en `harness-server/app.rs`; rutas absolutas filtradas al cliente. Tracked aparte (no P0).
>
> **Gate de dogfooding (CLAUDE.md §6):** 10 P0 ✅ + rehidratación de sesiones (T4) ✅.
> T4 queda cerrado para historial/selección/read-only tras reinicio; reattach interactivo al mismo PTY
> no se soporta con el modelo actual y las sesiones rehidratadas se exponen como no-live.
> T7 (`seq` racy) también cerrado por Task 15.

### S1. Path traversal por IDs sin validar (T1) — **ALTA / M**
`thread_id`, `task_id` y `profile id` fluyen del agente/API directamente a `join(...)`.
- `harness-mcp-server/src/tools/spec.rs:12-29` — `spec_read` NO valida (a diferencia de `spec_write`).
- `harness-mcp-server/src/tools/tasks.rs:236,292,311-393` — ningún `task_*` valida.
- `harness-server/src/routes/profiles.rs:230-244` — `activate` persiste el id verbatim en `active_profile`; un `../../etc` escapa de `HARNESS_HOME` en el siguiente boot.
- Sinks: `harness-core/src/tasks/store.rs:84-90`, `harness-core/src/store/mod.rs:155,238`.
- **Fix**: validación central `^[A-Za-z0-9_-]+$` en los helpers de ruta de `TaskStore`/`Store`, y reusar la validación de `create_profile` en `activate`. Tests de traversal (`../`, absolutos, NUL) en cada tool.

### S2. Inyección SQL por identificadores + gate read-only bypassable (T2) — **ALTA / M**
- `module-db/src/row.rs:16-36,149,166-178`, `module-db/src/export.rs:435-518,638-648` — nombres de tabla/columna/schema (claves JSON controladas por el agente) se envuelven en comillas **sin escapar la comilla embebida**. Una columna `a" ; DROP TABLE x --` rompe el quoting.
  - **Fix**: doblar la comilla en `quote_ident`/`qualify` (`replace('"', "\"\"")`, `` replace('`',"``") ``), rechazar NUL, y validar columnas contra el schema introspectado (ya se introspecta para PK).
- `harness-mcp-server/src/tools/db.rs:41-65` — `is_read_only` mira solo la keyword inicial; `WITH x AS (DELETE … RETURNING *) SELECT …`, statements apilados y `EXPLAIN ANALYZE <INSERT>` pasan como "lectura".
  - **Fix**: aplicar read-only a nivel de conexión (`SET TRANSACTION READ ONLY` PG, `START TRANSACTION READ ONLY` MySQL, `mode=ro`/`PRAGMA query_only` SQLite) en lugar de confiar en el prefijo. Como mínimo, quitar `WITH` del set auto-aprobado.

### S3. API mutadora sin autenticación (T3) — **ALTA / S**
`harness-server` no tiene capa de auth; CORS limita navegadores pero no clientes HTTP directos. Cualquiera que alcance el bind puede spawnear agentes, escribir bytes crudos al PTY y ejecutar SQL arbitrario.
- Endpoints: `routes/db.rs:513` (`run_query`), `routes/sessions.rs:595` (`post_input`), `routes/sessions.rs:729` (`spawn_child_route`).
- Default es loopback (`config.rs:30`) pero `HARNESS_BIND=0.0.0.0` está soportado (`state.rs:145`).
- **Fix**: middleware de token compartido (CORS ya permite `Authorization`), obligatorio en rutas no-health, al menos cuando el bind no es loopback.

### S4. Política por defecto abierta (`Decision::Allow`) — **ALTA / M**
`harness-policy/engine.rs:60-62` devuelve `Allow` por defecto. El cliente MCP solo "falla cerrado" ante errores de transporte (`dispatcher.rs:230-252`); si el server no tiene reglas, el sistema es default-open para tools sensibles.
- **Fix**: cambiar el default a `Ask`/`Deny` para tools sensibles y documentar la postura. Confirmar que `PolicyEngine` está realmente cableado en `/api/approvals/check`.

### S5. Panic UTF-8 en `repo_read_file` — **ALTA / S**
`harness-mcp-server/src/tools/repo.rs:97-100` — `content.truncate(max_bytes)` paniquea si el límite cae a mitad de un codepoint. El loop MCP es single-threaded → un archivo UTF-8 con `max_bytes` no-default crashea el server.
- **Fix**: leer bytes con `Read::take(max_bytes)` + `from_utf8_lossy`, o `while !content.is_char_boundary(max_bytes) { max_bytes -= 1 }`.

### S6. `next_id` no es crash-safe → IDs de tarea duplicados (T9) — **ALTA / S**
`harness-core/src/tasks/ids.rs:11-32` — `set_len(0)` + write sin temp+rename. Un crash entre truncar y escribir deja el archivo vacío → `unwrap_or(0)` reinicia el contador y el próximo `create` pisa `T-0001.toml`.
- **Fix**: escribir vía temp+rename, o sembrar el máximo escaneando `T-*.toml` ante fallo de parse en vez de default 0.

### S7. Sin recuperación de `events.jsonl` corrupto (T9) — **ALTA / M**
`harness-core/src/store/mod.rs:237-253` (y `read_handoffs:211-217`) — una sola línea truncada (artefacto típico de crash a mitad de append) hace ilegible TODO el historial del thread, violando el invariante append-only.
- **Fix**: skip-and-warn por línea (o cortar en el primer error y tratar el resto como truncado). Comparar con `list_threads` que ya hace log-and-skip.

### S8. Inyección por paste-end en prompts inyectados al PTY — **MEDIA / S**
`harness-session/src/manager.rs:204-231` — para Claude/Cursor el prompt va envuelto en bracketed paste; un payload con `\x1b[201~` rompe el sobre y puede inyectar secuencias de control.
- **Fix**: stripear/escapar `\x1b[201~` y ESC del `role_prompt` antes de inyectar.

### S9. Endpoint de approval influenciable por env del agente — **MEDIA / S**
`harness-mcp-server/src/dispatcher.rs:215-222` + `main.rs:119` — `server_url` viene de `HARNESS_SERVER_URL`. Si el agente puede influir su propio env, apunta el check a un server que controla.
- **Fix**: fijar el endpoint solo vía flag CLI del padre confiable; ignorar el env.

### S10. Otros endurecimientos
- SQLite siempre abre `mode=rwc` incluso para introspección → crea DB vacía ante typo (`module-db/src/storage.rs:267`). Usar `mode=ro` en rutas de lectura. — MEDIA / S
- Sin límite de tamaño de body; el cap de adjuntos (100 MiB) se chequea **después** de bufferizar todo en memoria (`harness-server/routes/sessions.rs:931`, sin `DefaultBodyLimit` global). — MEDIA / S
- Sin timeouts de request ni de statement SQL → DoS (`harness-server/app.rs:32-36`, `module-db/src/query.rs:42-140`). Añadir `TimeoutLayer` (excluyendo SSE) y `statement_timeout`/`busy_timeout`/`acquire_timeout`. — MEDIA / S
- Rutas absolutas del server filtradas al cliente (`routes/sessions.rs:946,991`). — BAJA / S
- `output.log` world-readable puede contener tokens; usar `0600` en el dir de sesión. — BAJA / S

---

## P1 — Correctitud y rendimiento

### El bug conocido: persistencia de estado de sesión (T4) — ✅ cerrado 2026-06-04

Estado implementado:
- `Manager::load_existing()` escanea `sessions_root`, carga `meta.json` y conserva sesiones detached en vistas read-only.
- `GET /api/threads` usa `manager.list_metas()` para listar sesiones live + detached.
- `output.log` sigue disponible para replay/catch-up de sesiones detached.
- Una sesión rehidratada con `status=running` se reconcilia a `exited` aunque el PID exista, porque el harness ya no tiene writer/killer/read tasks para controlar ese PTY.
- `GET /api/sessions/:sid/children` lista hijos detached desde metadata para mantener visible el árbol de agentes tras reinicio.
- Tests cubren rehidratación de sesión exited, running huérfana, running con PID vivo y merge live/detached.

**Causa raíz (backend)** — `harness-session/src/manager.rs:84-95` — `Manager::new` solo crea un `DashMap` vacío; `all()` (`:114`) es la **única** fuente para listar sesiones (`harness-server/routes/threads.rs:75`). Tras reiniciar el server (o el hot-reload de perfil que mata todas las sesiones, `main.rs:85`) el mapa está vacío aunque `meta.json` + `output.log` siguen en disco. **Nada rehidrata desde disco.**
- **Fix**: `Manager::load_existing()` que escanee `sessions_root`, lea cada `meta.json` e inserte una representación "detached/exited" (necesita un tipo read-only o `enum { Live, Detached(SessionMeta) }`, porque `AgentSession` exige `pty_writer`/`killer` vivos). Reconciliar `Running` obsoleto vía `pid_alive` al cargar (`session.rs:307-311`). — **ALTA / L**

**Contribuyente (frontend)** — `frontend/src/routes/+page.svelte:57-146` — la restauración persistida en `onMount` corre async tras resolver el perfil, pero dos `$effect` de auto-selección pueden dispararse antes con el valor legacy `'default'`, sobrescribir `selectedSessionId` con `allSessions[0]` y luego el effect espejo (`:75`) **persiste el valor equivocado** bajo la clave de perfil.
- **Fix**: gatear la auto-selección con un flag `profileResolved`; restaurar persistido → auto-elegir solo si la selección actual está ausente; nunca saltar a una sesión nueva salvo creación explícita (`onCreated`). — **ALTA / M**

**Relacionados**:
- `seq` del stream de output reinicia a 0 por proceso (`session.rs:193,482`) → reconexión puede desordenar. — BAJA / S
- "Restart" crea sesión a 80x24 sin `cols/rows` (`frontend/.../SessionMainView.svelte:141-144`) → primer frame del TUI roto. — MEDIA / M

### Rendimiento — backend (T5)

- **Scheduler reescanea todo el disco cada tick** — `harness-core/scheduler/tick.rs` (passes `run_ready`/`run_assign`/`run_lease`) llaman `store.list()` (`tasks/store.rs:147`) que re-lee y re-parsea **todos** los TOML por thread, 2-3× cada 2s. Driver: usar el índice SQLite (ya tiene status/assignee/updated_at) y compartir un `list` por thread por tick. — **ALTA / M**
- **`ensure_thread` reconstruye el índice completo sosteniendo el mutex** — `tasks/store.rs:92-144` — el rebuild sostiene `index.lock()` Y `self.threads` durante todo el I/O, serializando el arranque. Soltar el lock antes de reconstruir; batch de upserts en una transacción. — **ALTA / M**
- **`read_output` lee el `output.log` entero (hasta 50 MiB) en el handler SSE async** — `harness-server/routes/events.rs:100` → `OutputWriter::read_active` (`harness-session/output.rs:66`). Bloquea el worker + base64 de todo antes de emitir. Mover a `spawn_blocking` o stremear; capar el catch-up a los últimos N KiB. — **ALTA / M**
- **`find_existing` hace `block_in_place`+`block_on(meta())` por sesión en cada spawn del scheduler** — `harness-server/state.rs:351-373`. Indexar sesiones por `(thread_id, agent_id)` en un `DashMap` para lookup O(1) sin await. — **ALTA / M**
- **`seq = read_events().len()`** relee y deserializa todo el log por append (T7) — `routes/threads.rs:126,187`, `routes/tasks.rs:221`. Además es racy (dos appends concurrentes calculan el mismo `len()`). El store debe asignar `seq` atómicamente en el append. — **MEDIA / S**
- **Compresión aplicada a SSE** bufferiza y retrasa eventos — `harness-server/app.rs:36`. Excluir `text/event-stream`. — **MEDIA / S**
- **`module-db`: export bufferiza toda la tabla (hasta 5M filas) ×2 en memoria con paginación `OFFSET` O(n²)** — `module-db/src/export.rs:566-625`. Stremear chunks al sink y usar keyset pagination (también arregla el orden inconsistente sin `ORDER BY`). — **ALTA / M**
- **`module-db`: introspección de schema N+1 y sin cache** — `src/schema.rs`, y `row.rs::primary_key_cols` (`:44`) introspecta el schema completo en **cada** CRUD de fila. Batch + cache de `SchemaTree` por conexión; en `primary_key_cols` traer solo la PK de la tabla objetivo. — **MEDIA / M**
- **`module-db`: `pg_backend_pid()`/`CONNECTION_ID()` añade un round-trip por query** — `src/query.rs:85-110`. Hacerlo lazy (solo al pedir cancel). — **MEDIA / S**
- **`harness-session`: detector relee+re-parsea 8 KiB del log cada 600ms por sesión** — `session.rs:329-364`, `detect.rs:57-86`, aun en reposo. Saltar la lectura si `metadata().len()` no cambió; reusar buffer; `Cow` en `replace_all`. — **MEDIA / M**
- **`harness-session`: doble copia por chunk del PTY** — `session.rs:220`. Enviar `Box<[u8]>`/`bytes::Bytes`. — MEDIA / S

### Concurrencia y correctitud — backend

- **SSE pierde eventos en silencio bajo `Lagged` (T6)** — `harness-server/routes/events.rs:48,72,135`, `routes/transcript.rs:70`, `harness-session/manager.rs:89`. Emitir un frame explícito `event: lagged`/`seq-reset` para que el frontend reconecte/refresque. — **ALTA / M**
- **Reload no aborta los transcript watchers ni el ticker** — `harness-server/main.rs:85-90`; los watchers (`watcher.rs:84`, poll cada 500ms) siguen vivos vía `Arc`. Guardar handles y abortarlos en reload. — **ALTA / M**
- **Tareas de fondo de sesión fire-and-forget sin handles** — `harness-session/session.rs:213-364`; reader thread + 3 tasks no se pueden parar; `Manager::remove` no las detiene. Guardar `JoinHandle`/`AbortHandle` y abortar en `shutdown()`/`Drop`. — **ALTA / M**
- **Carrera kill/exit + reuso de PID** — `harness-session/session.rs:303-312` vs `403-437`; `kill()` pre-escribe `Killed` mientras la wait-task escribe status/exit_code, y `libc::kill(child_pid,…)` sobre un PID posiblemente reciclado señala a otro proceso. Consolidar la decisión de status en una sección crítica y gatear señales con un flag `exited`. — **ALTA / S**
- **Approval pendiente se filtra al desconectar el cliente** — `harness-server/approvals.rs:90-96`; el `select!` se cancela y la entrada en `pending` queda hasta el exit. Usar guard RAII que limpie al dropear el future. — **MEDIA / M**
- **Lock poisoning (T8)** — `harness-server/approvals.rs:80,93,105,122`, `harness-policy/engine.rs:87,97,141`. Usar `parking_lot::RwLock` o recuperar el guard envenenado. — **MEDIA / S**
- **`module-db`: fuga de conexión en `drop_lease_async`** — `src/lease.rs:217-226`; `Arc::try_unwrap` falla con queries en vuelo → `close()` nunca corre y el pool size-1 queda abierto para siempre. Clonar y cerrar el `DbPool` directamente (es `Clone`, `close()` idempotente). — **ALTA / S**
- **`module-db`: `locks` en `PoolCache` crece sin límite** — `src/pool.rs:60,98-103`; `invalidate` no limpia `locks`. — MEDIA / S
- **Sin fsync del directorio padre tras create/rename (T9)** — `harness-core/store/mod.rs:233,66-73`, `tasks/store.rs:648`. Para un log "append-only invariante", fsync del dir padre. — MEDIA / S
- **`set_spent` sobrescribe el acumulado con la suma del tick** — `harness-core/budget/mod.rs:204-213`; una sesión que desaparece entre ticks baja `spent_usd` y puede des-disparar el hard-cap. Documentar el contrato cumulative-vs-delta o hacer la actualización monotónica. — MEDIA / S

### Rendimiento y correctitud — frontend

- **Polling de sesiones duplicado (2× cada 5s) con carrera de aborts** — `+page.svelte:162-181` e `IconRail.svelte:42-60` corren su propio `setInterval` sobre el mismo `sessionsState`; dos requests in-flight resuelven fuera de orden y se pisan. Mover el polling al store (ref-counted `start()`/`stop()`). — **ALTA / M**
- **Polling de children 1.5s por sesión seleccionada** — `SessionRightPanel.svelte:131`, sin importar tab ni rol. Gatear a `tab==='agents'`/rol Zeus, o suscribir al SSE `/events`. — MEDIA / M
- **`TerminalView` reconexión puede duplicar timers / fugar EventSource** — `TerminalView.svelte:199-218`; guardar con `if (reconnectTimer) return;`. — MEDIA / S
- **`tasksState.start()` no refresca al re-entrar al mismo thread** — `stores/tasks.svelte.ts:65`; muestra datos stale en la ruta de tasks. — MEDIA / S
- **Header `X-Protocol-Version` nunca se envía en requests** — `api/client.ts:57-65` (solo se lee de la respuesta). — ALTA / S
- **Prefijo SSE inconsistente** `/api/events` vs `/events` — `stores/spec.svelte.ts:56` vs el resto. Estandarizar en `/events`. — MEDIA / S
- **Sin timeout en `apiRequest`** → spinners infinitos — `api/client.ts:67-72`. Añadir `AbortSignal.timeout`. — MEDIA / S
- **SSE de stores sin backoff/resync** (solo `TerminalView` lo implementa) — `api/sse.ts:47`. Helper reconnecting-SSE compartido. — MEDIA / M

---

## P2 — Calidad, deuda técnica y tests

### Duplicación y refactors
- **Generación de config MCP duplicada y divergente** entre `harness-server/state.rs:430-478` y `routes/sessions.rs:377-482` — los agentes spawneados por el scheduler reciben una superficie MCP distinta a los de REST (drift real, no solo duplicación). Extraer un helper compartido. — **MEDIA / M**
- **`module-db`: duplicación masiva por engine** en CRUD de filas y decoders (`src/row.rs`, `src/value.rs`, `query.rs:142-179`) — el grueso de los ~4900 LOC; un fix hay que aplicarlo 3×. Trait `DbBackend` o macros. — MEDIA / M
- **Componentes gigantes**: `TerminalView.svelte` (592 LOC), `ResultGrid.svelte` (826 LOC) mezclan demasiadas responsabilidades. Extraer clipboard/context-menu y el reducer de edición inline. — MEDIA / L
- **Helpers `str_arg`/`opt_str` duplicados** en 6 módulos de tools (`harness-mcp-server/src/tools/*`). Extraer a `tools::args`. — BAJA / S
- **Extracción de error body copy-pasteada ~6×** en el frontend. Un `apiErrorMessage(err)` en `client.ts`. — BAJA / S
- **Métodos read-meta→mutar→write casi idénticos** en `harness-core/store/mod.rs:115-147`. Helper `update_meta(id, |t| …)`. — MEDIA / S

### Tipos de error y código muerto
- `harness-core`: `StoreError` vs `crate::Error` (dos enums paralelos); `module-db`: `Toml`/`Keyring`/`Internal` stringly sin `#[from]`. Unificar/estructurar. — BAJA / S
- Código muerto / `#[allow(dead_code)]` y stubs keep-alive: `harness-mcp-server/dispatcher.rs:257-261`, `tools/db.rs:438-443`, `module-db/export.rs:614-616`, varios en `harness-core` (`scheduler/mod.rs` doc obsoleta, `index.rs:60,94`). Remover o cablear. — BAJA / S
- `module-db`: `.unwrap_or_default()` sobre `try_get` traga errores de decode como `""` (`schema.rs`, `manager.rs:128-136`). Al menos `tracing::warn!`. — BAJA / S
- Frontend: store legacy `session.svelte.ts:19` sin uso; `tokensLabel(null)` placeholder; `as any` en refs de CodeMirror. — BAJA / S

### Tests (T10)
- **Frontend: cero infra de test** (sin vitest/playwright, sin specs). Lo más riesgoso (backoff SSE, efectos de selección, reducer de `ResultGrid`, `leadingSqlKeyword`) está sin verificar. Montar Vitest para lógica pura + smoke Playwright. — **ALTA / L**
- **`harness-mcp-server`/`module-db`: sin tests de path-traversal ni de bypass read-only** (`WITH … DELETE`, `SELECT; DROP`, `EXPLAIN ANALYZE INSERT`). — ALTA / M
- **`harness-core`: sin tests de concurrencia del flock** ni de recuperación de log/TOML corrupto ni de crash-safety de `next_id`. — MEDIA / M
- **`harness-server`: sin tests de los endpoints SSE** (contigüidad de seq, lag, replay `since`) ni de los sanitizers de path. — MEDIA / M
- **`harness-session`: sin tests del ciclo de vida** donde vive el bug (survival al detach, carrera kill/exit, persistencia de status, `read_active`, rotación de log). — MEDIA / M
- **`module-db`: sin tests del ciclo lease/transacción** (auto-pin en BEGIN, misma conexión entre BEGIN/COMMIT, reap); MySQL sin integración alguna. — MEDIA / M
- A11y frontend: radiogroups sin navegación por flechas (`NewSessionDialog.svelte:153-211`), modal de approval sin guard de Escape, acciones solo-hover y estado solo-color. — MEDIA / S

---

## Roadmap por fases

### Fase A — Seguridad (P0, hacer primero)
1. S1 Path traversal centralizado (core + mcp + profiles activate).
2. S2 Escapar identificadores SQL + read-only a nivel de conexión.
3. S3 Middleware de auth por token (al menos para bind no-loopback).
4. S5 Fix panic UTF-8 en `repo_read_file` (trivial, alto impacto).
5. S6 `next_id` atómico + S7 recuperación de log corrupto.
6. S4/S9 Postura de política y endpoint de approval fijado.
7. S10 Endurecimientos (mode=ro, body limit, timeouts).

> Recomendado tras la Fase A: correr `/security-review` sobre el diff.

### Fase B — El bug de sesión + correctitud caliente (P1)
1. ✅ T4 `Manager::load_existing()` + sesiones detached no-live (backend).
2. ✅ T4 selección frontend estable con sesiones rehidratadas.
3. Ciclo de vida de tareas de fondo de sesión (handles + shutdown) y carrera kill/exit.
4. Reload aborta watchers/ticker; approval RAII cleanup; lock poisoning → parking_lot.
5. `module-db` fuga de lease (`drop_lease_async`) y `PoolCache.locks`.

### Fase C — Rendimiento (P1)
1. Scheduler off-index (no rescan de disco) + `ensure_thread` sin lock-across-I/O.
2. `read_output`/catch-up off-thread y capado; `find_existing` O(1).
3. `seq` atómico en el store (elimina T7 y la carrera).
4. SSE: excluir de compresión + frame `lagged`/resync (T6).
5. `module-db`: export stremeado + keyset, cache de schema, pid lazy.
6. Frontend: consolidar polling en store, gatear children-poll, timeouts + header de protocolo.

### Fase D — Calidad y tests (P2)
1. Infra de test frontend (Vitest) + tests de seguridad backend (traversal, read-only).
2. Refactors de duplicación (config MCP compartida, trait `DbBackend`, helpers de args).
3. Componer down de componentes gigantes; limpiar código muerto y tipos de error.
4. Tests de concurrencia/ciclo de vida (flock, sesión, lease, SSE) y a11y.

---

## P3 — Autonomía: gateway MCP, carga inteligente e indexación de proyecto

> Objetivo del usuario: que el harness sea lo más autónomo y completo posible.
> Tres piezas: (1) un **gateway MCP** que haga pasar *todos* los MCP —nativos y
> externos— por el gate de policy/approvals; (2) **carga inteligente** de MCPs
> por sesión; (3) un MCP/tool que **analice la estructura del proyecto** y
> mantenga un mapa actualizado para que los agentes la conozcan rápido.
> Más memoria de código vía `codebase-memory-mcp` y endurecer `crawl4ai`.

### Estado actual (línea base)
- **Modelo de runtime**: los MCP externos corren como **contenedores Docker
  persistentes** (`docker-compose.mcp.yml`: `crawl4ai`, `excalidraw-mcp`) que
  quedan **levantados y disponibles** todo el tiempo. El harness NO los arranca
  por sesión; solo se conecta a los que ya están up. La capa de contenedores =
  *disponibilidad*; la decisión de qué exponer a cada agente es aparte.
- Inyección MCP por sesión ya existe: cada agente recibe `mcpServers` con el
  server nativo `harness` + `extra_mcp_servers` opcionales
  (`harness-server/src/routes/sessions.rs:438-472`, `state.rs:453-463`).
- `crawl4ai` **ya integrado** pero cargado por heurística
  (`should_load_crawl4ai_context`, `sessions.rs:531`). `excalidraw-mcp` también.
- **Problema clave**: los `extra_mcp_servers` (crawl4ai, etc.) hablan
  directo al CLI del agente, **sin pasar** por `/api/approvals/check`
  (`harness-mcp-server/dispatcher.rs:214-254`). Solo las tools nativas
  del harness cruzan el gate. Esto contradice el requisito.
- **Nuevos servicios** (codebase-memory, analizador de estructura si fuera
  externo) se suman como contenedores al stack `docker-compose.mcp.yml` con su
  healthcheck, siguiendo el mismo patrón "siempre disponible".

### A1. Gateway MCP — todo pasa por el gate — **ALTA / L**
Convertir `harness-mcp-server` en **agregador/proxy** de los MCP externos en
lugar de inyectarlos como servers paralelos.
- Al spawnear, NO añadir crawl4ai/codebase-memory como `extra_mcp_servers`
  directos; en su lugar el `harness` MCP se conecta *él* a esos upstreams
  (SSE/stdio) y **re-expone** sus tools con prefijo (`crawl4ai__crawl`,
  `memory__search`, …).
- Así cada `tools/call` —sea nativa o proxiada— pasa por el mismo
  `policy_check` del `dispatcher.rs` antes de reenviarse al upstream.
- Beneficios: un solo punto de auditoría/approval, logging unificado,
  rate-limit y timeouts centralizados, y el agente ve un único server.
- Implementación: cliente MCP upstream en `harness-mcp-server`
  (conexión + `tools/list` + passthrough de `tools/call`), tabla de ruteo
  `prefijo → upstream`, y reuso del `policy_check` existente. Cachear el
  `tools/list` de cada upstream (hoy se reconstruye por llamada — ver T-perf
  del dispatcher). Reusa el fix de `harness-policy` default-open (S4):
  los tools proxiados sensibles deben caer en `Ask`/`Deny` por defecto.

### A2. Carga inteligente de MCPs por sesión — **MEDIA / M**
Los contenedores ya están up (A-estado-actual); la "carga inteligente" decide
**qué de lo disponible se expone a cada sesión** a través del gateway.
Generalizar la heurística crawl4ai a un **matcher de capacidades**.
- Registro declarativo `capability → { upstream, tools, triggers, roles }`
  (extiende `docs/13-agents/capability-registry.md`).
- Decidir qué capacidades exponer por sesión según: contenido del
  prompt/tarea (palabras clave, URLs, lenguaje), `role` del agente
  (p.ej. Zeus siempre obtiene `memory` + `repo_map`; un worker de docs
  obtiene `crawl4ai`), y disponibilidad/health del contenedor.
- Filtrado de superficie: el gateway expone al CLI solo las tools de las
  capacidades concedidas; las demás existen pero quedan ocultas para esa
  sesión (menos ruido + menos superficie a auditar).
- Lazy-connect: el gateway solo abre la conexión al contenedor up la primera
  vez que se invoca una tool de esa capacidad (ahorra recursos, no arranca
  contenedores — ya están disponibles).
- Health-aware: si el contenedor de una capacidad concedida está down, el
  gateway lo reporta como indisponible en vez de fallar la tool opaco.
- Reemplaza las 3 evaluaciones repetidas de `should_load_crawl4ai_context`
  (`sessions.rs:194-209`) por una sola pasada del matcher.

### A2b. Routing Zeus por fortaleza de CLI — **MEDIA / S**
Zeus no debe elegir CLIs solo por dominio grueso (`frontend`, `backend`), sino
por la clase concreta de trabajo.
- Frontend visual: pantallas, CSS, layout, responsive, componentes shadcn,
  estados hover/focus, densidad visual, polish y a11y visual → **Cursor
  primero**, luego Codex, luego Claude.
- Frontend logic: stores, API client, validación, rutas, modelos TS y cambios
  mecánicos → **Codex primero**, luego Claude.
- Arquitectura, DB, decisiones de producto y re-plan → **Claude primero**.
- Tests, refactors mecánicos y PR cleanup → **Codex primero**.
- Implementación: `harness-core::scheduler::routing` debe clasificar por
  `task.domain`, `task.touches`, labels (`ui`, `css`, `layout`, `a11y`,
  `visual`) y texto del brief. Cada fallback queda auditado.
- Restricción: Cursor puede ser primario visual solo si el spawn puede operar
  con el mismo contrato de task/handoff/audit que los demás; mientras no tenga
  MCP injection equivalente, Zeus debe darle contexto explícito por prompt y
  exigir evidencia verificable al volver.

### A3. Indexador de estructura de proyecto (repo map) — **ALTA / M**
Tool nativa en `harness-mcp-server` (junto a `tools/repo.rs`) que genera y
mantiene un **mapa del proyecto** consultable.
- Tool `repo_map` / `project_outline`: produce un resumen estructurado
  (árbol de dirs relevante, módulos/crates, símbolos top-level por archivo
  vía tree-sitter o heurística por lenguaje, puntos de entrada, comandos de
  build/test del `Justfile`).
- Persistir en `.harness/project-map.json` (+ vista `PROJECT_MAP.md`) dentro
  del workspace/perfil; inyectarlo (o un resumen) en el `auto_intro` del
  spawn para que el agente conozca la estructura sin explorar.
- **Auto-actualización tras cambios**: watcher con debounce (reusar el patrón
  de `transcript/watcher.rs`) sobre el workspace, o regenerar en hooks
  `git post-commit` / al cerrar sesión / on-demand. Cachear con invalidación
  por mtime para que no sea costoso (mismo anti-patrón que evitamos en el
  scheduler, T5).
- Por ser tool nativa, hereda el gate de approvals y el sandbox de rutas
  (aplicar el fix de traversal S1 a sus paths).
- Alternativa externa (tree-sitter code-graph MCP): viable, pero entonces
  **debe** ir tras el gateway A1; preferimos nativo por sandbox + approval.

### A4. Memoria de código (`codebase-memory-mcp`) — **MEDIA / M**
Alinear con el workitem F5 ya planificado (handoff events + SQLite FTS5 +
`memory_search`).
- Opción preferida: tool **nativa** `memory_*` (FTS5) en el harness, que
  cumple F5 y queda bajo el gate/approval y el `HARNESS_HOME` del perfil.
- Si se adopta el `codebase-memory-mcp` externo: montarlo **detrás del
  gateway A1** (no como `extra_mcp_server` directo) para no abrir un canal
  fuera del gate, y persistir su store dentro de `HARNESS_HOME` por perfil.
- Sinergia: el repo map (A3) alimenta el índice de memoria; el `memory_search`
  resuelve "¿dónde está X?" sin re-escanear, reforzando la autonomía.

### A5. Endurecer `crawl4ai` para recuperación web/docs — **MEDIA / S**
- Pasarlo por el gateway A1 (hoy es `extra_mcp_server` directo).
- Mejorar el disparo: además de la heurística, exponerlo como capacidad
  "web/docs" del matcher A2 y permitir activación explícita por el agente.
- Robustez: health-check antes de inyectar (ya hay healthcheck en compose),
  fallback claro si el contenedor no está arriba, y respetar el "Output
  Shape" del skill `crawl4ai-context` para no volcar páginas enteras a logs.
- Considerar añadir una capacidad de *web search* (motor de búsqueda) que
  alimente URLs a crawl4ai, para el flujo "buscar → extraer lo mejor".

### Roadmap P3 (tras P0/P1)
1. A1 Gateway MCP (prerequisito de todo lo demás para cumplir el gate).
2. A3 Repo map nativo + auto-update (mayor ganancia de autonomía inmediata).
3. A2 Carga inteligente por capacidades (sustituye la heurística ad-hoc).
4. A2b Routing Zeus por fortaleza de CLI, con Cursor primario para frontend visual.
5. A4 Memoria de código (F5) tras el gateway.
6. A5 Endurecer crawl4ai + web search detrás del gateway.

> Dependencias: A1 depende de S4 (postura de policy) y del fix de cache de
> `tools/list` del dispatcher. A3 reusa el sandbox de rutas de S1.

---

### Apéndice — Top-3 por dominio (de cada revisor)

- **harness-core**: `next_id` crash-safe · recuperación de `events.jsonl` · stop al rescan del scheduler.
- **harness-server**: auth + `activate` traversal · `read_output`/`find_existing` bloqueantes · `spec_path` hardcodea `"default"` (fuga cross-workspace, `routes/spec.rs:146`).
- **mcp-server + policy**: traversal `thread_id`/`task_id` · panic UTF-8 `repo_read_file` · gate read-only `WITH` + default-open.
- **harness-session**: rehidratar sesiones desde disco (el bug) · ownership de tareas de fondo · carrera kill/exit + reuso PID.
- **module-db**: inyección por identificadores · read-only solo-keyword · export bufferiza todo ×2 con OFFSET O(n²) (+ fuga `drop_lease_async`).
- **frontend**: máquina de selección de sesión · consolidar polling · montar infra de tests.
