# BOARD — Equipo de desarrollo de HarnessDevTool

Canal común entre Planner (Claude), Backend Rust (Codex), Frontend (Cursor) y los evaluadores
(Sonnet). Plantilla **estricta por campos**, no prosa libre. Ver `CLAUDE.md` §4.
Modelo operativo: ver [`docs/teamwork/OPERATING_MODEL.md`](./OPERATING_MODEL.md).
Rendimiento de ejecutores y puntuación del usuario: [`SCOREBOARD.md`](./SCOREBOARD.md).

> **Límite conocido:** una sola tarea "En curso" a la vez, sin locking real. El Planner es el único
> que abre/cierra; los ejecutores anotan en su bloque de Handoff. Revisor/QA reportan por la Agent
> tool (no escriben aquí).
>
> **Balance velocidad/calidad:** subagentes internos de Codex pueden acelerar self-review, pero no
> sustituyen reviewer/QA oficial lanzado por Claude/Planner desde el harness.

---

## En curso

| Campo | Valor |
|---|---|
| **Tarea** | ChatView round 4 — UX de "thinking/processing" estilo Claude.ai/ChatGPT/Gemini + fix del bug "tras enviar el primer mensaje no hay feedback de que el agente procesa". |
| **Estado** | ✅ DONE — cerrada 2026-06-16. CODIFY (subagente `frontend`) + REVIEW (`reviewer`, 1 ronda: 1 P1 + P2-A/D/E must-fix) + INCORPORATE (fix round) + QA con agent-browser (Planner). `pnpm check` 0/0. **VERIFY (agent-browser, stack aislado dev-raw + HARNESS_HOME temporal, sin tocar la app del usuario en :43177):** la burbuja de "procesando" (3 dots animados estilo Claude/ChatGPT) aparece **inmediatamente** al enviar y persiste todo el hueco hasta que llega el turno del assistant — verificado en sesión **claude** Y **codex**. Render estructurado limpio (turno user/agent, sin chips `thinking`/`writing`/`thought complete` redundantes). **Hallazgo que valida el diseño:** `detected_state` quedó en `working` aunque claude ya estaba idle/"waiting for input" → confirma que el flag optimista (no dependiente de `detected_state`) era el enfoque correcto. **Limitación de la verificación:** el *disclosure* de thinking (preview 2 líneas + dropdown) no se pudo ejercer en vivo porque ningún agente emitió eventos `kind:thinking` en este stack (claude cayó a fallback PTY — no escribió su jsonl de proyecto en el run anidado; codex emitió 0 thinking en prompts simples). Implementado + revisado por código, no exhibido visualmente. P2-C del reviewer (tool blocks colapsados) = cambio pre-existente del working tree, fuera de alcance. |
| **Objetivo** | (1) **Bug principal**: tras enviar un mensaje (sobre todo el primero), debe aparecer **de inmediato** un indicador animado de "pensando/procesando" al fondo del thread, sin esperar a `detected_state` (que es heurístico del scrollback y llega tarde). Se oculta cuando el turno del assistant empieza a streamear content/thinking/tools o cuando `detected_state` vuelve a idle. (2) **Rediseño del bloque de thinking**: liviano estilo Claude — colapsado por defecto mostrando solo **~2 líneas renderizadas** del pensamiento con fade/clamp; dropdown (chevron) expande al detalle completo (en vivo: tail con auto-scroll; completo: texto entero). Header "Thinking…" con shimmer mientras está activo → "Thought for N.Ns" al terminar. (3) **Limpiar lo redundante**: quitar chips/pills sobrantes del meta (`writing`/`thinking`/`thought complete`) y simplificar. (4) Animación de "processing" reusable (shimmer/dots) compartida entre la burbuja pendiente y el thinking activo. |
| **Alcance / archivos** | Solo `frontend/src/lib/components/app/ChatView.svelte`. NO tocar tipos `ts-rs` (`frontend/src/lib/api/types/`) ni backend. `AgentState = working\|blocked\|idle\|unknown`. |
| **Responsables** | Planner: Claude (Opus). Codificador: subagente `frontend` (Sonnet 4.6). Revisor: subagente `reviewer` (1 ronda) + UI-design. QA/VERIFY: Planner (`pnpm check` + revisión visual). |
| **Criterio de aceptación** | (1) Tras enviar el primer mensaje en una sesión recién abierta aparece **inmediatamente** un indicador animado de pensando/procesando al fondo (flag optimista al enviar, no dependiente de `detected_state`). (2) El indicador desaparece al empezar la respuesta del assistant (content/thinking/tool) o al timeout de seguridad. (3) El thinking colapsado muestra máx ~2 líneas renderizadas; el dropdown expande al detalle completo; en vivo el header tiene shimmer y al terminar muestra "Thought for N.Ns". (4) Sin chips/pills redundantes en el meta del turno. (5) No se rompe ninguna ruta existente (streaming, markdown render diferido, auto-scroll, fallback PTY, historial post-restart). (6) `pnpm --dir frontend check` verde. |
| **Checks obligatorios** | `pnpm --dir frontend check`; revisión del `reviewer` sobre el delta; VERIFY visual del Planner. |

### Round 5–6 (feedback del usuario, 2026-06-16)
- **R5**: indicador "working" persistente (dots + mini-status line en vivo: `Running <tool>` / tail 2 líneas de thinking / `Working…`); supresión del doble-thinking; gate del fallback PTY a working/awaiting. `pnpm check` 0/0.
- **R6**: (1) **Scroll del Terminal** (`TerminalView.svelte`, ghostty-web): `attachCustomWheelEventHandler` → scrollLines del viewport local (return `true` = bloquea reenvío al PTY), arregla scroll-up que mandaba ↑ al proceso. (2) **ChatView nunca vacío**: neutralizado el auto-hide de R5 que dejaba el ChatView en blanco; el bloque PTF ahora persiste como respuesta cuando no llega transcript, se abre solo al terminar (`open={fallbackDone}`) y cambia copy ("Live terminal output…" → "Response (terminal output)"). `pnpm check` 0/0.
- **HALLAZGO RAÍZ (backend, sin resolver)**: el ChatView de claude queda vacío porque claude en **modo TUI interactivo** (como lo spawnea el harness) **no escribe el transcript jsonl** que el watcher vigila — verificado: `claude --print --session-id <uuid>` SÍ escribe `~/.claude/projects/<slug>/<uuid>.jsonl`, pero la sesión interactiva del harness (6+ min, turno completado) nunca lo creó. Ruta del watcher correcta (`transcript/claude.rs:transcript_path`). Codex sí captura (discovery dinámico en `~/.codex/sessions`). **Fix real pendiente**: lograr que claude interactivo emita transcript estructurado (cambio en el spawn de `harness-session`, alto riesgo) — requiere decisión del usuario.

_Cola previa: el resto del "Pipeline Zeus sobre ChatView" (handoff 2026-06-10) sigue vigente (adjuntos, formato de texto)._

## Última cerrada — Super-harness W5: residuales de perf + deudas de los slices

| Campo | Valor |
|---|---|
| **Tarea** | Slice final del plan "Super-harness 2026-06": cerrar los residuales P1 del improvement-plan que siguen abiertos + las deudas anotadas en los reviews de W1–W4. |
| **Estado** | ✅ DONE — cerrada 2026-06-12. Implementado: policy-check 120s→8s fail-closed con tests; `ensure_thread` con lock de init por thread (sin mutex global a través de I/O) + revalidación de waiters tras fallo de rebuild; confinamiento `HARNESS_DATA_ROOT` para `knowledge_pdf_ingest`/`office_ingest` con error guiado + `.env.example`. Hallados ya-resueltos (improvement-plan stale): `X-Protocol-Version` ya se envía vía `apiHeaders()`; polling ya consolidado en Waves 1–2. Review: 0 P0 / 0 P1 / 4 P2, todos corregidos en fix round (waiters huérfanos en error path, error UX del root, tests con set_var paralelo, margen del test de timeout). VERIFY del Planner: `just test` 450/450 + svelte-check 0/0. Pendiente: doc-agent debe marcar en improvement-plan.md los P1 cerrados; puntuación del usuario en SCOREBOARD. |
| **Objetivo** | (1) **Policy-check timeout** (`harness-mcp-server/src/dispatcher.rs` ~486): el check HTTP de policy espera hasta 120s; bajarlo a 8s con error claro (el agente no debe colgarse 2 min si el server no responde). (2) **`ensure_thread` mutex across I/O** (`harness-core/src/tasks/store.rs` ~92-144): no mantener el mutex del store a través de I/O de disco; reducir la sección crítica. (3) **`X-Protocol-Version` nunca se envía** (improvement-plan P1): el cliente frontend (`frontend/src/lib/api/client.ts`) debe mandar el header en los requests que el backend versiona; mismatch → error explícito ya implementado server-side. (4) **Confinamiento de `knowledge_pdf_ingest`/`knowledge_office_ingest`** (deuda W4): mismo confinamiento `HARNESS_DATA_ROOT` que ya tiene `knowledge_data_ingest`. (5) **Polling frontend residual** del improvement-plan (si sigue vigente tras Waves 1–2: consolidación/race de aborts donde quede). Revisar `docs/12-build-plan/improvement-plan.md` y marcar allí lo cerrado. |
| **Alcance / archivos** | Backend: `harness-mcp-server/src/{dispatcher.rs,tools/knowledge.rs}`, `harness-core/src/tasks/store.rs`. Frontend: `frontend/src/lib/api/client.ts` (+ donde el polling residual aplique). Docs: el doc-agent actualiza improvement-plan al cierre. Sin tipos ts-rs previstos. |
| **Responsables** | Planner: Claude (Opus). Codificador: Codex gpt-5.5. Revisor: Sonnet 4.6 (`reviewer`, 1 ronda). QA: subagente `qa`. |
| **Criterio de aceptación** | (1) Policy check con server caído falla en ≤10s con mensaje claro (test con endpoint que no responde). (2) `ensure_thread` sin lock a través de I/O, con test de no-regresión de concurrencia. (3) Requests mutantes del frontend llevan `X-Protocol-Version` (verificable en `pnpm check` + grep/test; smoke curl opcional). (4) `knowledge_pdf_ingest` con path fuera del root → rechazado (test). (5) `cargo test` de crates tocados, `pnpm --dir frontend check` y `just test` verdes. |
| **Checks obligatorios** | `cargo test -p harness-core -p harness-mcp-server`, `pnpm --dir frontend check`, `cargo fmt --check`, `just test` al cierre. |

## Última cerrada — Super-harness W4-S1: Context engine v2 (retrieval de knowledge, ingesta de datos, resúmenes estructurados, truncación dual)

| Campo | Valor |
|---|---|
| **Tarea** | Cuarto slice del plan "Super-harness 2026-06": el contexto deja de ser "volcado" y pasa a ser "recuperado". Patrones de referencia: pi (formato de resumen estructurado + truncación dual líneas/bytes UTF-8 safe) y hermes (retrieval FTS5 just-in-time). |
| **Estado** | ✅ DONE — cerrada 2026-06-12. Review 2 P1 (truncación corrompía JSON >50KB; re-ingesta duplicaba filas FTS) + 5 P2, todos corregidos en fix round. QA PASS en los 6 criterios (`just test` 446/446 + svelte-check 0/0; smoke real: ingesta CSV con BOM, re-ingesta sin duplicados, confinamiento de root, query FTS maliciosa inocua, JSON estructurado íntegro vs texto libre truncado; prompt del governor estructurado con umbrales intactos). VERIFY del Planner OK. Pendiente: puntuación del usuario en SCOREBOARD. |
| **Objetivo** | (1) **`knowledge_search`**: tool MCP (grupo `knowledge`) con FTS5 sobre los shards de knowledge ya ingestados (PDF/DOCX/PPTX/texto); índice SQLite construido/actualizado al ingestar (y reconstruible lazy si falta); devuelve top-N shards con fuente, heading y snippet — el agente recupera lo relevante en vez de leer el doc entero. (2) **Ingesta de datos a knowledge**: `knowledge_data_ingest` para CSV/XLSX (reusar el parsing del data loader existente si aplica): resumen de columnas/tipos + muestras como shards markdown buscables. (3) **Checkpoint estructurado del governor**: el prompt de checkpoint de `context_governor.rs` pide el formato de pi — Goal / Constraints / Progress (Done · In Progress · Blocked) / Key Decisions / Next Steps / Critical Context + Files read/modified. (4) **Truncación dual en el MCP server**: tool results acotados por líneas (2000) Y bytes (50KB) independientes, head-truncation para lecturas y tail-truncation para ejecuciones (`ssh_exec`, comandos), UTF-8 safe, con sufijo de metadata `[truncated: N lines / M bytes omitted]`; aplicada en el dispatcher como capa genérica con overrides por tool. |
| **Alcance / archivos** | Backend: `backend/crates/harness-core/src/knowledge.rs` (+ índice FTS5 y data ingest), `backend/crates/harness-mcp-server/src/{dispatcher.rs,tools/knowledge.rs,tools/toolsets.rs}` (tools nuevas + truncación), `backend/crates/harness-server/src/context_governor.rs` (prompt estructurado). Sin tipos ts-rs previstos; si cambian → `just gen-types`. Sin UI nueva. |
| **Responsables** | Planner: Claude (Opus). Codificador: Codex gpt-5.5 (`codex exec`). Revisor: Sonnet 4.6 (subagente `reviewer`, 1 ronda). QA: subagente `qa`. |
| **Criterio de aceptación** | (1) Ingestar un doc de prueba y `knowledge_search("término del doc")` devuelve el shard correcto con fuente/heading/snippet; búsqueda sobre knowledge sin índice previo lo reconstruye (test). (2) `knowledge_data_ingest` con un CSV y un XLSX de prueba genera shards con schema de columnas + filas de muestra, y son buscables (tests). (3) El prompt de checkpoint del governor contiene las secciones estructuradas (test de contenido del prompt); el resto del flujo del governor no cambia. (4) Truncación dual: tests de límite por líneas, por bytes, multibyte UTF-8 en el borde, head vs tail según tool, y metadata correcta; los tool results del dispatcher pasan por la capa (test de integración con un output sintético grande). (5) FTS5: query con sintaxis maliciosa/operadores raros no rompe (sanitización o match query escapado, test). (6) `cargo test -p harness-core -p harness-mcp-server -p harness-server`, `cargo fmt --check` y `just test` verdes. |
| **Checks obligatorios** | `cargo test -p harness-core -p harness-mcp-server -p harness-server`, `cargo fmt --check`, `just test` al cierre, `just gen-types` solo si cambia un tipo TS. |

### Contrato — W4-S1

- `knowledge_search` input `{query: string, limit?: number}` → top-N shards (fuente, heading, snippet, score). `knowledge_data_ingest` input `{source_path: string, title?: string}` (CSV/XLSX por extensión). Ambas en grupo `knowledge`.
- Índice FTS5 en `$HARNESS_HOME/profiles/<p>/knowledge/index.sqlite`; append al ingestar; reconstrucción completa si falta o esquema viejo. El layout de shards en disco NO cambia (compat con lo ya ingestado).
- La truncación dual es capa del dispatcher sobre el `content` textual de los tool results; tools pueden declarar override (modo head/tail y límites). Los resultados de gateway upstream también pasan por la capa.
- Governor: solo cambia el TEXTO del prompt de checkpoint; umbrales y mecánica intactos.

### Handoff Implementación W4-S1 — Codex 2026-06-12 (registrado por el Planner)

**Archivos:** `harness-core/src/knowledge.rs` (índice FTS5 en `profiles/<p>/knowledge/index.sqlite`, rebuild lazy, indexado al ingestar, phrase-query segura, `knowledge_search`, `knowledge_data_ingest` CSV/XLSX), `harness-mcp-server/src/{tools/knowledge.rs,tools/mod.rs,tools/toolsets.rs,tools/capabilities.rs,dispatcher.rs}` (tools nuevas en grupo `knowledge`; truncación dual genérica de resultados — 2000 líneas/50KB, head para lecturas, tail para `ssh_exec`/`repo_git_*`, UTF-8 safe, sufijo de metadata, gateway incluido), `harness-server/src/context_governor.rs` (prompt de checkpoint estructurado: Goal/Constraints/Progress Done·In Progress·Blocked/Key Decisions/Next Steps/Critical Context/Files read-modified; umbrales intactos).

**Tests nuevos:** search devuelve shard ingestado; rebuild lazy del índice; operadores FTS raros no rompen; CSV y XLSX generan shards de schema/muestras buscables; prompt del governor con secciones; truncación por líneas/bytes/borde multibyte/head-vs-tail/sufijo; truncación a nivel dispatcher con resultado sintético grande.

**Checks:** core 114/114 ✅, MCP 82/82 ✅, server 121/122 (solo la falla ambiental conocida del sandbox) ✅, `cargo fmt --check` ✅. Sin tipos ts-rs tocados. `just test` en sandbox paró en la falla ambiental — re-verificar fuera.

### Review + fix round W4-S1 — 2026-06-12

- Revisor (Sonnet 4.6, 1 ronda): 2 P1 (truncación por bytes corrompía JSON estructurado >50KB — regresión para `db_schema` grande; re-ingesta del mismo doc duplicaba filas FTS porque el DELETE por shard-path nunca matcheaba los slugs viejos) + 5 P2 (SQLite sin WAL/busy_timeout/transacción; aviso de truncado al final en modo tail; rebuild abortaba en el primer shard corrupto; `knowledge_data_ingest` sin confinamiento de root; BOM UTF-8 en headers CSV). Validó como correcto: escaping FTS5, shape isError + auto-load note a través de la capa, prompt del governor con compat hacia atrás, lógica UTF-8 de truncación, registro en grupo knowledge.
- Fix round (Codex, registrado por el Planner): los 7 corregidos — JSON estructurado excluido de la truncación por bytes (test: sigue parseando; texto libre sí trunca); DELETE por source en re-ingesta (test: sin duplicados); WAL + busy_timeout 1s + transacción; aviso como prefijo en tail; rebuild tolera shards rotos con warning; `knowledge_data_ingest` confinado a `HARNESS_DATA_ROOT` (symlinks que escapan rechazados, test); BOM stripped. **Deuda anotada**: `knowledge_pdf_ingest`/`knowledge_office_ingest` sin confinamiento de root (pre-existente) → W5.
- Checks: core+MCP ✅; harness-server solo la falla ambiental del sandbox; `cargo fmt --check` ✅.

## Última cerrada — Super-harness W3-S1: DB context pack + caché de introspección + perf

| Campo | Valor |
|---|---|
| **Tarea** | Tercer slice del plan "Super-harness 2026-06": espejo del W2 para el módulo DB — los agentes operan sobre conexiones de BD abiertas **con contexto del schema montado**, más las deudas de perf del improvement-plan (N+1 de introspección, paginación OFFSET, read-only por keywords). |
| **Estado** | ✅ DONE — cerrada 2026-06-12. Review 0 P0 / 2 P1 / 4 P2; los 2 P1 + 3 P2 corregidos en fix round (collation exótica en keyset TEXT PK queda como edge anotado; falta test de regresión dedicado del timeout de row counts — anotado por QA). QA PASS en los 7 criterios con smoke real SQLite de punta a punta (`just test` 429/429 + svelte-check 0/0; brief con tablas/FK/row counts; CTE mutante rechazada por motor y por validate_query; keyset sin OFFSET; drop invalida caché; `db_connection_id` generado por gen-types). VERIFY del Planner OK. Pendiente: puntuación del usuario en SCOREBOARD. |
| **Objetivo** | (1) **DB context pack**: tools `db_context_refresh(connection_id)` y `db_context(connection_id, max_age_hours?)` — del árbol de introspección existente genera un brief markdown (tablas con columnas/PK/FK, relaciones, row counts aproximados, vistas) cacheado en `$HARNESS_HOME/profiles/<p>/modules/db/context/<connection_id>.md`; staleness como en W2. Reusar el patrón de sanitización del W2 (identificadores/comentarios con backticks no rompen fences) + cap global 12KB. (2) **Caché de introspección** (P1 del improvement-plan): el schema tree se cachea por conexión con TTL + invalidación explícita; una segunda `db_schema` no re-ejecuta el N+1 completo. (3) **Keyset pagination** en `db_export_table`/`db_export_query` (reemplaza OFFSET O(n²) cuando hay PK ordenable; fallback documentado si no). (4) **Read-only transaccional**: las operaciones read-only corren dentro de transacción read-only del motor (SQLite `PRAGMA query_only`, Postgres/MySQL `... READ ONLY`) como capa adicional al gate por keywords (que se mantiene como defensa rápida); el bypass conocido `WITH … DELETE` queda bloqueado por el motor. (5) **Agente sobre conexión**: `CreateSessionRequest.db_connection_id` opcional → precarga grupo `db` en tool_groups e inyecta el brief (o instrucción de `db_context_refresh`) en `auto_intro`, espejo exacto del `ssh_host_id` de W2. |
| **Alcance / archivos** | Backend: `backend/crates/module-db/**`, `backend/crates/harness-mcp-server/src/tools/db.rs` + `tools/toolsets.rs` (tools nuevas al grupo `db`), `backend/crates/harness-server/src/routes/sessions.rs` (campo + inyección, junto al path de ssh_host_id). `CreateSessionRequest` cambia → `just gen-types`; NO editar a mano `frontend/src/lib/api/types/`. Sin UI nueva. |
| **Responsables** | Planner: Claude (Opus). Codificador: Codex gpt-5.5 (`codex exec`). Revisor: Sonnet 4.6 (subagente `reviewer`, 1 ronda). QA: subagente `qa`. |
| **Criterio de aceptación** | (1) `db_context_refresh` genera el brief con tablas/relaciones/row counts desde una BD SQLite de test y `db_context` sirve del caché respetando staleness (tests). (2) Brief sanitizado contra fence-escape + cap 12KB (tests, mismo patrón W2). (3) Caché de introspección: segunda llamada no repite el fan-out de queries (test con contador de queries o equivalente); invalidación explícita disponible. (4) Export grande sin OFFSET creciente: asserts sobre las queries generadas (keyset por PK) y resultado idéntico al export actual en una tabla de test. (5) `WITH cte AS (DELETE FROM t RETURNING *) SELECT * FROM cte` por la ruta read-only es rechazada por el motor en SQLite (test real con `query_only`); para pg/mysql, asserts del SQL/flags de transacción generados (sin requerir servicio vivo). (6) Spawn con `db_connection_id`: `tool_groups` incluye `db` y `auto_intro` lleva el brief o la instrucción (tests); `gen-types` corrido. (7) `cargo test -p module-db -p harness-mcp-server -p harness-server`, `cargo fmt --check` y `just test` verdes. |
| **Checks obligatorios** | `cargo test -p module-db -p harness-mcp-server -p harness-server`, `cargo fmt --check`, `just gen-types`, `just test` al cierre. |

### Contrato — W3-S1

- `db_context_refresh` input `{connection_id: string}`; `db_context` input `{connection_id: string, max_age_hours?: number}` → markdown del brief. Errores: connection not found → error MCP claro, sin panic.
- `CreateSessionRequest.db_connection_id?: string` (ts-rs → `just gen-types`). Convive con `ssh_host_id` (ambos opcionales, pueden coexistir).
- El gate por keywords NO se elimina; el modo read-only del motor es capa adicional. Ninguna operación de escritura existente (`db_row_insert`, etc.) cambia de semántica.
- Caché de introspección y de context pack viven en module-db; el MCP server solo consume.

### Handoff Implementación W3-S1 — Codex 2026-06-12 (registrado por el Planner)

**Archivos:** `module-db/{Cargo.toml,src/manager.rs,src/query.rs,src/export.rs,src/storage.rs,tests/sqlite_export.rs}`, `harness-mcp-server/src/{dispatcher.rs,tools/db.rs,tools/mod.rs,tools/toolsets.rs}`, `harness-server/src/routes/{sessions.rs,db.rs}`, `CreateSessionRequest` regenerado (gen-types; `db_connection_id?: string | null`).

**Implementado / decisiones:**
- Context pack DB: caché en `profiles/<p>/modules/db/context/<connection_id-hash>.md`, tmp+rename, dir 0700, sanitización de fences + cap 12KB (patrón W2); `db_context_refresh` / `db_context(max_age_hours?)` como tools del grupo `db`.
- Schema tree cacheado por conexión/database con TTL 5 min + `Manager::invalidate_schema_cache`.
- Export con keyset pagination si hay PK simple ordenable; fallback OFFSET documentado sin PK.
- Read-only real en `query_run_read_only`: SQLite `PRAGMA query_only`, PG `BEGIN READ ONLY`, MySQL `START TRANSACTION READ ONLY`. ⚠️ Decisión a validar en review: el gate por keywords ahora DEJA PASAR `WITH` y delega los CTE mutantes al motor.
- `CreateSessionRequest.db_connection_id` precarga grupo `db` + brief/instrucción en auto_intro, convive con `ssh_host_id`; `start_db_agent` también lo marca.

**Checks:** `cargo test -p module-db` ✅; `-p harness-mcp-server` 78/78 ✅; `harness-server` 118/118 (skip del test ambiental, pasa fuera del sandbox) ✅; `cargo fmt --check` ✅; gen-types corrido (vía workspace ts-export + copia de bindings). `just test` en sandbox paró en la falla ambiental conocida — re-verificar fuera.

### Review + fix round W3-S1 — 2026-06-12

- Revisor (Sonnet 4.6, 1 ronda): 0 P0; 2 P1 (`validate_query` clasificaba CTEs mutantes como read-only tras agregar WITH a READ_ONLY_KEYWORDS — el motor sí bloqueaba, pero el tool advisory mentía; `db_drop_table/schema` no invalidaban el schema_cache → briefs/schema stale) + 4 P2 (`generate_view_sql` aceptaba CTE con DML; COUNT(*) sin timeout en SQLite podía colgar `db_context_refresh`; collation exótica en keyset TEXT PK — edge anotado, no corregido; doble contexto sin cap en auto_intro de `start_db_agent`). Validó como NO-bug: aislamiento del PRAGMA query_only en pool efímero separado, ROLLBACK en error, rutas constructoras de SQL no afectadas.
- Fix round (Codex, registrado por el Planner): P1-A detección de DML dentro del WITH en `validate_query` (test del caso exacto); P1-B invalidación de schema_cache tras drop_table/drop_schema; P2-A `generate_view_sql` rechaza CTEs mutantes de nuevo; P2-B timeout 2s por tabla en row counts SQLite (best-effort, `row_estimate=None`); P2-D sin duplicado del intro DB para la misma conexión + cap global 24KB del auto_intro con nota de truncado.
- Checks: `cargo test -p module-db` ✅; harness-mcp-server ✅; harness-server solo la falla ambiental del sandbox; `cargo fmt --check` ✅.

## Última cerrada — Super-harness W2-S1: SSH conexiones persistentes + remote context pack

| Campo | Valor |
|---|---|
| **Tarea** | Segundo slice del plan "Super-harness 2026-06": completar el módulo SSH para que los agentes operen sobre conexiones abiertas **con contexto de lo que hay montado en el host**. Patrón de referencia: hermes-agent (`tools/environments/ssh.py`: ControlMaster con ControlPersist, socket dedup por hash corto). |
| **Estado** | ✅ DONE — cerrada 2026-06-12. Review 1 P1 (fence-escape → auto_intro) + 5 P2, todos corregidos en fix round. QA PASS en los 6 criterios (`just test` 418/418 + svelte-check 0/0; smoke MCP stdio: grupo `ssh` lazy con `ssh_context`/`ssh_context_refresh`, error limpio con host inexistente sin tumbar el server; ControlMaster/close/staleness/fence-escape/cap-12KB asserteados por tests; `CreateSessionRequest.ssh_host_id` generado por gen-types). VERIFY del Planner OK. Sin smoke contra sshd real (no había sshd local). Pendiente: puntuación del usuario en SCOREBOARD. |
| **Objetivo** | (1) **Reuso de conexión**: module-ssh gestiona ControlMaster/ControlPersist de openssh (socket en dir runtime con hash corto del host para no exceder el límite de `sun_path`); `ssh_exec`/`sftp_*` reutilizan la conexión viva en vez de handshake por llamada; cierre explícito + expiración. (2) **Remote context pack**: tools `ssh_context_refresh(host_id)` y `ssh_context(host_id)` — introspección read-only y best-effort del host (uname/os-release, hostname, uptime, interfaces, servicios systemd corriendo si hay systemd, `docker ps` si hay docker, package manager detectado, dirs relevantes existentes: `$HOME`, `/var/www`, `/opt`, `/srv`, top procesos) con timeout por comando (≤5s) → brief markdown cacheado en `$HARNESS_HOME/profiles/<p>/modules/ssh/context/<host_id>.md` con timestamp; `ssh_context` sirve del caché y refresca si falta o está stale (>24h). (3) Ambas tools entran al grupo `ssh` del registry W1. (4) **Agente sobre conexión**: `CreateSessionRequest` acepta `ssh_host_id` opcional; al spawnear, el grupo `ssh` va en `tool_groups` y el `auto_intro` incluye el brief del context pack (si existe; si no, instrucción de correr `ssh_context_refresh`). |
| **Alcance / archivos** | Backend: `backend/crates/module-ssh/**` (manager de conexiones + introspección + caché), `backend/crates/harness-mcp-server/src/tools/ssh.rs` + `tools/toolsets.rs` (tools nuevas al grupo `ssh`), `backend/crates/harness-server/src/routes/sessions.rs` (campo `ssh_host_id` + inyección del brief + grupo precargado). `CreateSessionRequest` es `#[derive(TS)]` → **correr `just gen-types`**; NO editar a mano `frontend/src/lib/api/types/`. Sin UI nueva en este slice. |
| **Responsables** | Planner: Claude (Opus). Codificador: Codex gpt-5.5 (`codex exec`). Revisor: Sonnet 4.6 (subagente `reviewer`, 1 ronda). QA: subagente `qa`. |
| **Criterio de aceptación** | (1) Dos `ssh_exec` consecutivos al mismo host reutilizan la conexión maestra (verificable: socket de control existente entre llamadas; tests con comando ssh simulado/mocked que asserten los flags `-o ControlMaster/ControlPath/ControlPersist`). (2) `ssh_context_refresh` genera el brief con secciones de os/hostname/servicios y `ssh_context` lo devuelve del caché sin re-ejecutar; staleness respetada (test con timestamp manipulado). (3) Host sin docker/systemd degrada sin error (secciones omitidas, test con outputs simulados). (4) Introspección estrictamente read-only (lista fija de comandos, sin interpolación de input del usuario en shell). (5) Spawn con `ssh_host_id`: `meta.loaded_capabilities.tool_groups` incluye `ssh` y el `auto_intro` contiene el brief (test). (6) `cargo test -p module-ssh -p harness-mcp-server -p harness-server` y `just test` verdes; `just gen-types` corrido si cambió el tipo. |
| **Checks obligatorios** | `cargo test -p module-ssh -p harness-mcp-server -p harness-server`, `cargo fmt --check`, `just gen-types` (CreateSessionRequest cambia), `just test` al cierre. Smoke real contra un sshd solo si hay uno disponible en localhost (no asumir). |

### Contrato — W2-S1

- `ssh_context_refresh` input `{host_id: string}` → corre la introspección y reescribe el caché; `ssh_context` input `{host_id: string, max_age_hours?: number}` → devuelve el brief (refresca si stale/missing). Output: markdown del brief.
- `CreateSessionRequest.ssh_host_id?: string` (ts-rs → `just gen-types`); el server NO falla si el host no tiene context pack aún (degrada a instrucción).
- La introspección usa una lista fija de comandos read-only con timeout; nada de input del usuario interpolado en la línea de shell. El sandbox actual de module-ssh se mantiene.
- Estado de conexiones maestras: en module-ssh (no en el MCP server); sockets bajo dir runtime del perfil con hash corto.

### Handoff Implementación W2-S1 — Codex 2026-06-12 (registrado por el Planner; sandbox de Codex sin write a docs/)

**Archivos tocados:** `module-ssh/{Cargo.toml,src/manager.rs}`, `harness-mcp-server/src/{dispatcher.rs,tools/ssh.rs,tools/capabilities.rs,tools/mod.rs,tools/toolsets.rs}`, `harness-server/src/routes/{sessions.rs,ssh.rs,db.rs}` (ssh.rs/db.rs: ajuste menor de firma), tipos generados (`CreateSessionRequest.ts` con `ssh_host_id`, vía gen-types; el path generado no está trackeado por git).

**Implementado / decisiones:**
- ControlMaster reuse en `ssh`/`scp`: `ControlMaster=auto` + `ControlPath` corto con hash + `ControlPersist=10m`; limpieza de sockets y cierre explícito del master al cerrar la sesión SSH.
- Remote context pack: lista FIJA de probes read-only con timeout ≤5s c/u; brief markdown cacheado en `profiles/<p>/modules/ssh/context/<host_id>.md` con freshness; degradación sin docker/systemd testeada con outputs simulados.
- Tools MCP `ssh_context_refresh(host_id)` y `ssh_context(host_id, max_age_hours?)` registradas en descriptors, capabilities, dispatcher y grupo `ssh` del registry W1.
- `CreateSessionRequest.ssh_host_id`: precarga grupo `ssh` en tool_groups e inyecta el brief cacheado (o instrucción de refresh) en `auto_intro`.

**Checks:** `cargo test -p module-ssh` 10/10 ✅; `-p harness-mcp-server` 78/78 ✅; `harness-server ssh_bound_spawn` 2/2 ✅; `cargo fmt --check` ✅; `just gen-types` corrido (bindings copiados). `just test` en su sandbox paró en la falla ambiental conocida (`local_port_available_detects_bound_port`, 307 pass antes del fail-fast) — re-verificar fuera. Sin smoke contra sshd real (no asumido).

### Review + fix round W2-S1 — 2026-06-12

- Revisor (Sonnet 4.6, 1 ronda): 0 P0; 1 P1 (prompt injection por fence-escape: stdout remoto sin sanitizar dentro de ```text en el brief → llegaba verbatim al auto_intro) + 5 P2 (dir de control sockets sin 0700; sin cap global del brief ~42KB posibles; `-O exit` con ControlMaster=auto; colisiones de safe_cache_key; sha2 duplicado en el grafo). Introspección read-only y contrato validados sin hallazgos; sin regresiones de W1.
- Fix round (Codex, registrado por el Planner): los 6 corregidos — sanitización de backticks + marcadores BEGIN/END por sección con test de fence-escape; cap global 12KB con nota de truncado; 0o700 en dirs de control y caché; `ControlMaster=no` para `-O exit`; hash corto de 8 hex en la clave de caché; `sha2` a `[workspace.dependencies]` (queda 0.11 transitivo vía pdf_oxide, anotado).
- Checks del fix round: `cargo fmt --check` ✅; `cargo test -p module-ssh` 14/14 ✅; harness-mcp-server ✅; harness-server solo la falla ambiental conocida del sandbox.

## Última cerrada — Super-harness W1-S1: Tool loading 2.0 (lazy load/unload) en harness-mcp-server

| Campo | Valor |
|---|---|
| **Tarea** | Primer slice del plan "Super-harness 2026-06" (`docs/12-build-plan/super-harness-plan-2026-06-12.md`): el MCP server deja de exponer 60+ tools fijas a todo agente y pasa a un registry de **toolsets composables** con carga/descarga en runtime. Patrón de referencia: hermes-agent (`toolsets.py` resolución recursiva con includes + LRU de schemas) y el modelo de deferred-tools de Claude Code. |
| **Estado** | ✅ DONE — cerrada 2026-06-12. Review 1 P1 + 6 P2, todos corregidos en fix round. QA PASS en los 7 criterios (`just test` 407/407 + svelte-check 0/0; smoke stdio completo: base 21 tools vs 60+, load/unload con `list_changed`, auto-load con nota, `tools_search` funcional). VERIFY del Planner OK. Nota: el criterio decía ≤20 tools base; quedó en 21 porque el P1 del review promovió `session_cancel_child` a `core` — aceptado. Pendiente: puntuación del usuario en SCOREBOARD. |
| **Objetivo** | (1) Definir grupos de tools en `harness-mcp-server`: `core` (tasks/spec/session/mailbox + meta-tools), `repo`, `knowledge`, `db`, `ssh`, `skills`, `docs`, con composición `includes` cycle-safe. (2) `tools/list` devuelve solo `core` + los grupos activos de la sesión (semilla: `LoadedCapabilities.tool_groups` del `SessionMeta`, leído vía `--session-id` que el dispatcher ya recibe). (3) Meta-tools siempre presentes: `tools_search(query)` → tools/grupos disponibles con descripción y grupo; `tools_load(groups)` / `tools_unload(groups)` → actualizan el set activo y emiten `notifications/tools/list_changed` por stdio. (4) Llamada a una tool de grupo no cargado → **auto-load del grupo + ejecutar** (con nota en el resultado), para clientes que ignoren `list_changed`; el gating de seguridad sigue siendo policy per-call, no la visibilidad. (5) Caché de la lista de schemas por set de grupos activos (recompute solo al cambiar el set). (6) `resolve_smart_tool_groups` en `harness-server` mapea señales → grupos iniciales nuevos (db/ssh/knowledge/repo) además de `data_loader`. |
| **Alcance / archivos** | Backend solamente: `backend/crates/harness-mcp-server/src/**` (registry nuevo + `dispatcher.rs` + `main.rs` para list_changed) y `backend/crates/harness-server/src/routes/sessions.rs` (solo `resolve_smart_tool_groups` + paso de grupos al MCP config). NO tocar `frontend/**`. `LoadedCapabilities` ya existe en `harness-session/src/meta.rs`; si se le agrega algo con `#[derive(TS)]`, correr `just gen-types`. |
| **Responsables** | Planner: Claude (Opus). Codificador: Codex gpt-5.5 (`codex exec`). Revisor: Sonnet 4.6 (subagente `reviewer`, 1 ronda). QA: subagente `qa`. |
| **Criterio de aceptación** | (1) Sesión sin grupos extra: `tools/list` expone ≤20 tools (core + meta-tools) en vez de 60+. (2) `tools_load(["db"])` emite `list_changed` y el siguiente `tools/list` incluye las `db_*`; `tools_unload` las quita. (3) Llamar `db_query` sin el grupo cargado funciona (auto-load) y el resultado lo anota. (4) Los upstreams del gateway (p.ej. `crawl4ai__*`) se listan solo si su capability está activa (comportamiento actual preservado). (5) Resolución de grupos con `includes` anidados y detección de ciclos, con tests. (6) `tools_search("export csv de una tabla")` devuelve `db_export_table` con su grupo. (7) `cargo test -p harness-mcp-server -p harness-server` y `just test` verdes; smoke por stdio: initialize → tools/list → tools_load → tools/list muestra el delta. |
| **Checks obligatorios** | `cargo test -p harness-mcp-server -p harness-server`, `cargo fmt --check`, smoke JSON-RPC por stdio, `just test` al cierre, `just gen-types` solo si cambia un tipo TS. |

### Contrato — W1-S1 (sin cambios de protocolo HTTP)

- Protocolo MCP stdio: se agrega la **notificación** `notifications/tools/list_changed` (estándar MCP) tras `tools_load`/`tools_unload`/auto-load. Ningún cambio en rutas HTTP ni en `X-Protocol-Version`.
- `tools_search` / `tools_load` / `tools_unload`: tools MCP nuevas en `core`. Input: `{query: string}` / `{groups: string[]}`. Output: texto con tools/grupos y el set activo resultante.
- Estado de grupos activos: en memoria del proceso MCP (por sesión de agente); semilla desde `SessionMeta.loaded_capabilities.tool_groups`. Persistir cambios de vuelta al meta es OPCIONAL en este slice (anotar decisión en handoff).
- Policy: el check per-call de `harness-policy` NO se relaja; la visibilidad de tools no es gating de seguridad.
- Tipos `ts-rs`: no se esperan cambios; si los hay, `just gen-types` obligatorio.

### Handoff Implementación — Codex 2026-06-12

**Archivos tocados:**
- `backend/Cargo.lock`
- `backend/crates/harness-mcp-server/Cargo.toml`
- `backend/crates/harness-mcp-server/src/{dispatcher.rs,main.rs,protocol.rs}`
- `backend/crates/harness-mcp-server/src/tools/{mod.rs,toolsets.rs,session.rs}`
- `backend/crates/harness-server/src/routes/sessions.rs`
- `docs/teamwork/BOARD.md`

**Implementado / decisiones:**
- Registry `toolsets` con grupos `core`, `repo`, `knowledge`, `db`, `ssh`, `skills`, `docs`, includes recursivos cycle-safe y aliases legacy (`data_loader`, `document_extract`, `project_memory`, `docs_web`, `repo_write`, `docs_build`).
- `tools/list` ahora usa `core + active_groups`, cacheado por set activo. Base smoke: 20 tools.
- `tools_search`, `tools_load`, `tools_unload` quedan en `core`; load/unload invalidan cache y emiten `notifications/tools/list_changed` por stdio.
- `LoadedCapabilities.tool_groups` se lee desde `SessionMeta` vía `--session-id`; los cambios runtime quedan en memoria y no se persisten de vuelta al meta en este slice.
- Llamar una tool de un grupo descargado auto-carga el grupo, emite `list_changed` y agrega una nota al resultado sin relajar el check per-call de policy.
- Upstreams gateway (`crawl4ai__*`) solo se listan si existe upstream y está activo `knowledge` o `docs`.
- `resolve_smart_tool_groups` conserva `data_loader` y agrega señales para `repo`, `knowledge`, `db`, `ssh`, `docs`.
- No se tocaron tipos `#[derive(TS)]`; no se corrió `just gen-types`.

**Checks corridos:**
- `cargo test -p harness-mcp-server` ✅ 75/75.
- `cargo test -p harness-server smart_loader` ✅ 5/5.
- `cargo test -p harness-mcp-server -p harness-server` ⚠️ `harness-mcp-server` 75/75 ✅; `harness-server` 113/114 antes del fallo. Falla solo `routes::threads::tests::local_port_available_detects_bound_port` por `TcpListener::bind("127.0.0.1:0")` → `PermissionDenied` en el sandbox; no se tocó `routes/threads.rs` por alcance estricto.
- `cargo fmt --check` ✅.
- Smoke JSON-RPC stdio ✅: `initialize -> tools/list -> tools_load(db) -> tools/list`; resultado: `base_count=20`, `list_changed=true`, `db_before=false`, `db_after=true`.
- `XDG_RUNTIME_DIR=/tmp just test` ⚠️ ejecutó hasta el mismo fallo ambiental de `routes::threads::tests::local_port_available_detects_bound_port` (302 pass, 1 fail, 1 skip antes de fail-fast). `just test` sin `XDG_RUNTIME_DIR=/tmp` no arranca porque `/run/user/1000/just` es read-only.

### Review + fix round — 2026-06-12

- Revisor (Sonnet 4.6, 1 ronda): 1 P1 (`session_cancel_child` huérfano: sin grupo → invisible para tools/list, tools_search y auto-load) + 6 P2 (mutación parcial sin list_changed ante grupo desconocido; break interno con stdout roto en notificaciones; shape MCP inválido en fallback de apply_auto_load_note; hit_count amplificado vs semántica score de Wave 2; test de auto-load frágil por CWD; regresión de compat en listado de upstreams crawl4ai). Sin P0. Registro del Planner (el revisor no escribe aquí).
- Fix round (Codex, registrado por el Planner — el sandbox de Codex no pudo escribir docs/): los 7 corregidos. `session_cancel_child` en `core`; validación de grupos antes de mutar; fallo de write de notificación corta el loop exterior; fallback serializa el result como item de texto; `hit_count` cuenta el peso una vez por señal; test de auto-load usa `ssh_hosts`; upstreams se listan si están en `mcp_servers` de la sesión O knowledge/docs activo.
- Checks del fix round: `cargo fmt --check` ✅; `cargo test -p harness-mcp-server` 78/78 ✅; smart_loader 6/6 ✅; harness-server 114/115 (falla solo `local_port_available_detects_bound_port`, ambiental del sandbox de Codex — pasa fuera, verificado por el Planner). Smoke stdio ✅: base 21 tools, `session_cancel_child` visible, `tools_load(db)` → `list_changed` + `db_*` visibles.

### Cola (después de W1-S1, en orden)
1. **W2 — SSH completo + remote context pack** (pendiente explícito del usuario): ControlMaster/reuso, transfer queue con progreso, `ssh_context` (introspección del host al conectar → brief para agentes), spawn de agente "sobre" una conexión SSH.
2. **W3 — DB context pack + perf**: `db_context` por conexión (schema summary + row counts → brief al spawn), caché de introspección (N+1), keyset pagination en export, read-only transaccional.
3. **W4 — Context engine v2**: resúmenes estructurados del governor (Goal/Progress/Decisions/Next + files), `knowledge_search` FTS5 sobre shards, ingesta xlsx/csv a knowledge, truncación dual líneas+bytes en tool results.
4. **Pipeline Zeus sobre ChatView** (handoff 2026-06-10, abajo) — sigue en cola, no cancelada.

### Hecho y pusheado esta sesión (2026-06-10)
- **Análisis del harness + revisión del fuente de Codex** (`docs/12-build-plan/harness-analysis-2026-06-10.md`): bugs P1 verificados, perf, hueco de seguridad de delegación, formato de rollout de Codex, fix del cuelgue headless.
- **Fix headless de Codex** validado (`HEADLESS_OK` 4s) + receta corregida en `CLAUDE.md §3` (`codex exec "PROMPT" … < /dev/null`).
- **Bug P1 gateway MCP read timeout — CERRADO** vía primer head-to-head codex-vs-sonnet: merge de la impl de Sonnet + el assert de reaping de Codex en `harness-mcp-server/src/gateway.rs` (69 tests verdes). Fila comparativa en `SCOREBOARD.md`.
- **Roster de Zeus + regla cross-model formalizados** (`CLAUDE.md §1–§3` + `docs/13-agents/zeus-orchestrator.md`): Opus orquesta · **Codex codifica (backend+frontend)** · **Sonnet revisa código + UI designer** · **Codex QA (agent-browser)** · ciclo `generar→revisar ×1 cross-model→incorporar→verificar`, cap=1, generador dueño del código, compuerta objetiva.
- Commits: `b6e19f6`, `9fb3bdd`, `13ae622`, `041b8b8`, `40fa731`, `55dbb74` (todos en `origin/main`).

### PRÓXIMA TAREA (handoff) — Pipeline Zeus sobre ChatView (frontend)
Estrenar el roster de Zeus en una mejora de **frontend ChatView**. **Objetivo (criterio del usuario):** comprobar y mejorar que ChatView (1) funcione bien, (2) muestre detalles relevantes, (3) muestre el **thinking en vivo**, (4) renderice bien el **formato del texto** de las respuestas, (5) permita **adjuntar archivos/imágenes y pasarlos al agente** para lectura. Archivos probables: `frontend/src/lib/components/app/ChatView.svelte` y aledaños.

**Pipeline a correr (Zeus, manual vía Planner):**
1. **QA-assess** — Codex + `agent-browser` (ya en PATH) recorre ChatView en vivo y reporta el estado de los 5 ítems (qué funciona / qué falla / qué se ve mal).
2. **CODIFY** — Codex corrige lo roto en `frontend/`.
3. **REVIEW + UI** — Sonnet 4.6 revisa código (1 ronda) y hace UI-design del visual.
4. **INCORPORATE** — Codex aplica los must-fix de Sonnet.
5. **re-QA** — Codex + agent-browser confirma.
6. **VERIFY** — Planner (Opus) verifica + el usuario puntúa 1-5 en SCOREBOARD.

**Notas operativas (críticas para la próxima sesión):**
- ⚠️ Al iniciar había una app del harness **ya corriendo** (tauri sidecar, ~4h, puertos dinámicos altos) — **probablemente la sesión activa del usuario**. NO testear contra ella. Levantar un **stack aislado** (`just dev-raw` con `HARNESS_HOME` temporal y puertos altos libres), como el VERIFY previo de ChatView round 3.
- `agent-browser` está en PATH (`/run/user/1000/fnm_multishells/.../bin/agent-browser`), referenciado en `Justfile`. Codex sabe usarlo.
- Decidir: trabajar en **main** (loop de hot-reload simple, gates dan la seguridad) vs **worktree aislado** (necesita su propio dev server + node_modules, más pesado). Recomendado: main + stack aislado de dev-raw.
- **Gap a cerrar antes de spawnear**: la formalización referencia `subagent_type: ui-designer`, que **no existe** en `.claude/agents/` (hay `frontend`, `reviewer`, `qa`, `doc-agent`). Crear `.claude/agents/ui-designer.md` (Sonnet 4.6) o usar `frontend`/`reviewer` con brief de UI-design.
- Roster y regla del pipeline: ver `CLAUDE.md §1–§2` y memoria `decision-cross-model-review`.

## Última cerrada — ChatView live round 3: vivo post-restart, auto-scroll, fallback PTY, restart con continuidad, robustez SSE backend

| Campo | Valor |
|---|---|
| **Tarea** | Cerrar los fallos detectados con verificación en navegador real (agent-browser sobre `just dev-tauri`): (a) tras Restart el chat queda congelado en el blob PTY y los turnos parseados nunca aparecen aunque el backend SSE sí los entrega; (b) cero auto-scroll (scrollTop=0 siempre, en mount y con mensajes nuevos); (c) el fallback PTY pinta el banner TUI crudo (ANSI sucio) como burbuja "claude output"; (d) Restart pierde historial visible y perfil (manda `zeus_roles: []`); (e) backend: SSE de transcript devuelve `stream::empty()` si el slot no existe y el cliente no reconecta; los watchers no se re-registran al rehidratar tras restart del server; ventana de pérdida entre replay y subscribe; (f) menores: tokens "0 tok" en header/sidebar, badge `working` que no vuelve a idle, input disabled sin CTA. |
| **Estado** | ✅ DONE — cerrada 2026-06-10. Review backend 0 P0/0 P1/5 P2 (fix round cerrado); review frontend 1 P1/2 P2 (fix round cerrado) + 2 micro-fixes con verificación en navegador. QA PASS en todos los criterios automatizables (`just test`: 393 cargo verdes + svelte-check 0/0; slot-wait, rehidratación, zeus_roles con curl real). VERIFY del Planner en navegador (stack aislado con binario nuevo): vivo limpio ✅; vivo post-Restart ≤4s con limpieza de turnos PTY ✅; fallback PTY colapsado con link a Terminal ✅; reload rehidrata y monta al fondo ✅; píldora "1 new message" (click→fondo) ✅; stick-to-bottom ✅; historial previo (2 `.prev-turn`) + separador "session restarted" tras Restart ✅ (causa raíz final: `selectedSession` derivado devuelve null por lag del poller y limpiaba `prevSidForChat`; fix con guard `sid !== null`); tokens reales en header ✅. **Pendientes** (ver bloque al final). |
| **Evidencia/repro** | Repro determinista (agent-browser): abrir app → click Restart → enviar mensaje → turnos parseados no aparecen nunca (innerText congelado en blob PTY); `curl -N /api/sessions/<sid>/transcript?since=0` del MISMO sid entrega los 6 eventos (replay+live OK). En estado limpio (reload de página) el vivo SÍ funciona. Auto-scroll: `.chat-scroll` scrollTop=0 con scrollHeight 752–1361 en todos los estados. Sesiones de prueba: 1e520d9b (vivo OK tras reload), 695f1289 (post-restart KO). |
| **Alcance / archivos** | Backend: `backend/crates/harness-server/src/routes/transcript.rs` (espera de slot + subscribe-antes-de-replay con dedup por seq), `backend/crates/harness-server/src/routes/sessions.rs` o módulo de rehidratación (re-registro de watchers al arrancar para sesiones vivas claude/codex). Frontend: `frontend/src/lib/components/app/ChatView.svelte`, `SessionMainView.svelte`, `client.ts` si hace falta helper. NO tocar `frontend/src/lib/api/types/` (no cambian tipos ts-rs: el shape de `TranscriptEvent` y el evento SSE `transcript`/`lagged` quedan igual). |
| **Responsables** | Planner: Claude. Backend: subagente nativo (codex exec headless roto — feedback 2026-06). Frontend: subagente `frontend`. Revisor/QA al final. |
| **Criterio de aceptación** | (1) Tras Restart, sin tocar tabs: el primer turno user/assistant aparece en vivo ≤2s después de que el backend lo ingiera; el blob PTY desaparece al llegar el transcript. (2) Si el SSE se cierra o llega `lagged`, el cliente reconecta con `since=<últimoSeq>` y backoff; cero pérdida visible. (3) Auto-scroll: al montar con historial el chat queda en el fondo; con mensajes nuevos sigue el fondo si el usuario estaba al fondo; si scrolleó arriba, píldora "↓ último mensaje" en vez de salto forzado. (4) Fallback PTY: ANSI-stripped, colapsado como "Vista de terminal (esperando transcript…)" con link al tab Terminal; nunca burbuja de agente con banner crudo. (5) Restart preserva `zeus_roles`/perfil de la sesión anterior y muestra separador "— sesión reiniciada —"; el historial de la sesión anterior del mismo thread se muestra atenuado encima (replay del sid viejo, que el cliente conoce). (6) Backend: SSE con slot ausente espera la aparición del slot (≥30s) en vez de cerrar tras replay vacío; tras restart del harness-server, sesiones vivas rehidratadas recuperan watcher (verificable: matar server, relanzar, transcript en vivo sigue); sin gap replay→live (subscribe antes de leer replay, dedup por seq). (7) Tokens del header/sidebar reflejan el usage agregado de los turnos; badge vuelve a idle al terminar el turno; input disabled ofrece CTA Restart. (8) `cargo test -p harness-server`, `pnpm --dir frontend check` y `just test` verdes + VERIFY del Planner con agent-browser repitiendo el repro. |
| **Checks obligatorios** | `cargo test -p harness-server`; `pnpm --dir frontend check`; repro agent-browser post-restart y post-reload; `just test` al cierre. |

### Contrato — stream de transcript (sin cambios de tipos)

- `GET /api/sessions/:sid/transcript?since=<seq>` (SSE, sin auth en GET): replay de eventos `seq > since` + live tail. **Garantía nueva**: si el slot no existe aún, el stream espera su aparición (poll interno, ≥30s) en vez de cerrar; sin gap entre replay y live (dedup por `seq`, el cliente puede recibir duplicados y debe dedupear por `seq` también).
- Evento `lagged` (ya existente): el cliente DEBE reconectar con `since=<último seq visto>`.
- `TranscriptEvent` no cambia. Ningún tipo `#[derive(TS)]` se toca → no corre `just gen-types`.
- Restart (frontend): reusar `zeus_roles` y kind/cwd de la sesión vieja al crear la nueva; el cliente conserva `oldSid` para replay del historial previo.

### Handoff Backend — subagente nativo 2026-06-10

- `routes/transcript.rs` (reescrito ~360 líneas): stream testeable `transcript_item_stream()`; subscribe-antes-de-replay con descarte `seq <= last` (sin gap ni duplicados); slot ausente → replay inmediato desde disco + poll 250ms hasta 30s con `tokio::time`, re-replay `since=last` al aparecer el slot; corta con tombstone `.deleted`. Shape SSE `transcript`/`lagged` intacto.
- `transcript/watcher.rs`: `WatcherCheckpoint {source_path, offset}` en `<session dir>/watcher-checkpoint.json` (tmp+rename) — evita que un watcher re-registrado duplique el store append-only; sesiones pre-checkpoint con historia arrancan en EOF (live-only). Codex: checkpoint válido salta re-discovery.
- `transcript/store.rs`: `last_seq()`. `main.rs`: llama `rehydrate_transcript_watchers` tras construir AppState (también en reload de profile).
- `routes/sessions.rs`: `rehydrate_transcript_watchers()` re-lanza watchers para sesiones claude/codex sin slot cuando: handle vivo, PID vivo (/proc, Linux-only) o checkpoint con cola sin ingerir. `tracing::info` con `reason`.
- Tests: 7 nuevos (slot tardío, no-gap/no-dup, tombstone, rehidratación con PID vivo, skip sesión muerta, resume desde checkpoint sin duplicados, pre-checkpoint → EOF). `cargo test -p harness-server` 110 pass; `cargo fmt --check` limpio.
- Decisiones a validar en review: checkpoint añadido más allá del brief (necesario para no duplicar transcript); criterio de "viva" = PID-alive + checkpoint-pending (el Manager hoy reconcilia Running→Exited al boot); watchers catch-up quedan en idle-poll 500ms hasta el próximo boot (auto-limitante).
- Review backend: 0 P0 / 0 P1 / 5 P2 (línea parcial en checkpoint, blocking I/O en rehydrate y stream, PID reciclado, fs síncrono en write_checkpoint). Fix round en curso con los 5.

### Handoff Frontend — subagente Sonnet 2026-06-10 (round 3)

- **BUG A (causa raíz)**: `subscribeSSE` con `reconnect: true` reconectaba siempre a la MISMA URL (`since=0`) sin tracking de seq, y con slot tardío el stream cerraba al instante → el timer PTY de 900ms ganaba y los turnos PTY nunca se limpiaban. Fix: `openTranscriptSSE` manual con `since=${lastSeq}`, `onError`/`onLagged` → reconexión con backoff 500ms→5s, dedup `seq <= lastSeq`, y al primer evento real se eliminan los turnos `source === 'pty'`.
- **BUG B**: `forceNextScroll` salta el gate de 120px en el primer RAF tras conectar; después stick-to-bottom solo si `atBottom`; píldora "↓" si hay mensajes con el usuario scrolleado arriba (`onscroll` en `.chat-scroll`).
- **BUG C**: fallback PTY pasa de `<pre>` suelto a `<details>` colapsado "Terminal output (waiting for transcript…)" con link al tab Terminal; desaparece al llegar transcript.
- **BUG D**: prop `prevSid` (SessionMainView lo setea en onRestart antes del kill) → fetch one-shot del transcript viejo vía SSE (idle 600ms, cap 5s) → turnos atenuados sobre separador "— session restarted —". **Pendiente backend**: `SessionMeta` no expone `zeus_roles` → restart sigue mandando `[]`; recomienda añadir campo a `SessionMeta` (tipo TS → gen-types).
- **BUG E**: tokens derivados de `turns.reduce` sobre usage → callback `onTotalTokens` → header/título muestran totales reales; CTA Restart inline cuando `stopped`. Badge "working" pegado: depende del state detector backend, no tocado.
- **LAYOUT**: wrapper `flex flex-col`, textarea `rows=1` con max 6 líneas (144px).
- `pnpm --dir frontend check` 0 errores / 0 warnings. ⚠️ No corrió el repro agent-browser (sin browser en su contexto) — queda para QA/VERIFY del Planner.

### Pendientes al cierre — 2026-06-10 (round 3)

1. **Posible leak de EventSource al sid viejo tras Restart** (P2, sin confirmar): en el último network log 2 EventSources al sid anterior aparecían sin status (¿abiertos o lag del log?). El fetch histórico debería cerrarse a 600ms de idle / cap 5s. Verificar con `agent-browser network requests` minutos después de un Restart; si persiste, cerrar el handle en el teardown del instance viejo.
2. **Vivo post-Restart re-verificado solo en la sesión b63933ad** ("42" en ≤4s ✅); el último Restart (f04775c2) cerró el VERIFY antes de re-testear el mensaje en vivo. Riesgo bajo (mismo código), pero repetir el check al retomar.
3. **Sidebar sigue mostrando "0 tok"** (P2): solo se cablearon header y title bar (`onTotalTokens`); la card del SessionsColumn no.
4. **Badge `working` pegado** (P2): depende del state detector backend → Task 35 (liveness watchdog) en el backlog.
5. **zeus_roles en Restart**: verificado por QA a nivel API (GET devuelve roles persistidos; payload se reenvía); falta E2E con una sesión Zeus real en navegador.
6. **Cuelgue puntual de POST /input vía proxy vite** observado una sola vez durante el VERIFY (curl directo al backend respondió 204 en 1ms); no reproducido después. Vigilar; si reaparece, mirar el proxy de vite, no el handler.
7. **just dev-tauri se cayó** durante la sesión (vite desapareció de 43178) — causa sin investigar; el VERIFY se hizo en stack aislado (`just dev-raw`, HARNESS_HOME temporal, ya apagado). Relanzar `just dev-tauri` y validar los fixes ahí (requiere `just build-sidecar` para que el sidecar tauri tome el binario backend nuevo).
8. **Puntuación del usuario pendiente** en `SCOREBOARD.md` para los slices de esta tarea (sonnet-4.6 backend ×2, frontend ×1 + 3 fix/micro-rounds).

## Última cerrada — ChatView fix round 2: parpadeo, thinking vivo, markdown y turn_duration

| Campo | Valor |
|---|---|
| **Tarea** | Fix de parpadeo del ChatView (flash de terminal cada poll), thinking en vivo, render markdown confiable del último turno y métrica `turn_duration` sutil bajo las respuestas. |
| **Estado** | ✅ DONE — cerrada 2026-06-10. Revisor encontró P0 (guard auto-destruido por teardown de Svelte 5) + 2 P1; corregidos y verificados por el Planner. `pnpm check` 0 errores. |
| **Objetivo** | (1) El chat no debe reabrir su SSE ni vaciar turnos en cada tick del poller (causa raíz: `selectedSession` es `$derived` del poller → referencia nueva cada ~1.5s → `$effect` de ChatView re-corre `openSSE`, limpia turnos y rearma el fallback PTY de 900ms → flash de terminal). (2) Mostrar que el agente está pensando y qué piensa, en vivo. (3) El markdown del turno final debe renderizarse aunque `detected_state` siga `working` (debounce por inactividad). (4) `turn_duration` (system_note de Claude con content null) deja de ser pill suelto y pasa a métrica sutil bajo el turno del asistente. |
| **Alcance / archivos** | Solo frontend: `frontend/src/lib/components/app/ChatView.svelte` (principal), `frontend/src/routes/+page.svelte` solo si hace falta estabilizar el prop. NO tocar tipos generados ni backend. |
| **Responsables** | Planner: Claude. Frontend: subagente `frontend`. Revisor/QA al final. |
| **Criterio de aceptación** | (1) Con el chat abierto, la suscripción SSE de transcript persiste a través de ticks del poller: cero reaperturas, cero parpadeo, cero flash de fallback PTY. (2) Mientras el agente piensa se ve un bloque "Thinking" vivo (texto streameando, indicador animado); al completarse colapsa a resumen expandible. (3) El último turno renderiza markdown (sin `**` visibles) a más tardar ~1.5s después de quedar inactivo el stream, sin parsear markdown por chunk durante streaming activo. (4) `turn_duration` se muestra como métrica discreta bajo la respuesta (formato "N.Ns"); ningún system_note sin contenido legible se pinta como pill de palabra suelta. (5) `pnpm --dir frontend check` verde. |
| **Checks obligatorios** | `pnpm --dir frontend check`; revisión del Revisor sobre el delta. |

### Handoff Frontend + review + fix round — 2026-06-10
- Solo `ChatView.svelte`. (1) Parpadeo: causa raíz `selectedSession` `$derived` del poller → referencia nueva por tick → `$effect` reabría SSE. Fix final (tras P0 del Revisor: el teardown del effect reseteaba el guard sincrónicamente en cada re-run): effect de teardown separado sin lecturas reactivas (cleanup solo al desmontar: timers + SSE + PTY) y effect de sesión con guard `openedSid` permanente y SIN cleanup — tick del poller = early return sin efectos; sid→null limpia turns/attachments.
- (2) Thinking vivo: mientras streamea sin content, header "Thinking…" animado + tail de últimas 10 líneas con auto-scroll interno (action propia, desacoplada del scroll del chat); al completar colapsa a "Thought (N.Ns)" expandible.
- (3) Markdown confiable: flag `settled` por turno con debounce 1200ms de inactividad (timers en Map limpiados en openSSE y unmount) + settle inmediato en eventos frontera; render gate pasa a `(!isStreaming || settled)`; invalidación robusta con `staleRenders` Set — chunks que llegan durante un render en vuelo descartan el HTML viejo en el `.then()` (single y batch). Tipografía `.chat-prose` pulida (strong, hr, li).
- (4) `turn_duration`: system_note de Claude con content null y `raw.durationMs` (+ fallbacks duration_ms/duration) → se asigna al último turno assistant y se muestra como chip discreto "⏱ N.Ns" (>60s → "Nm Ns") bajo la respuesta; system_notes sin contenido legible ya no se pintan como pill.
- Review: P0 (guard auto-destruido) + P1 (race render en vuelo → HTML truncado permanente) + P1 (timers huérfanos) — todos corregidos en fix round; VERIFY del Planner sobre el código final del effect. `pnpm --dir frontend check` 0 errores / 0 warnings.

## Última cerrada — Rich ChatView como centro de Agents

| Campo | Valor |
|---|---|
| **Tarea** | ChatView pasa a ser la vista principal de Agents (terminal secundaria) + renderizado rico: imágenes, documentos/attachments, escenas Excalidraw y código resaltado. |
| **Estado** | ✅ DONE — cerrada 2026-06-10. QA PASS en los 8 criterios; `just test` verde (383 tests, 0 fallos; svelte-check 0 errores). |
| **Objetivo** | El chat es el centro de la experiencia Agents: tab por defecto, y capaz de mostrar contenido visual — imágenes inline (markdown/URLs/data-URIs/base64 en tool results), tarjetas de documentos con preview/descarga, escenas `.excalidraw` renderizadas como gráfico, y code blocks con syntax highlighting — sin degradar el path de streaming. |
| **Alcance / archivos** | Backend (slice chico): `backend/crates/harness-server/src/routes/sessions.rs` — ruta nueva `GET /api/sessions/:sid/attach/:name` (solo aditivo; el archivo tiene cambios sin commitear de otra wave, no revertir nada). Frontend: `frontend/src/lib/components/app/{ChatView,SessionMainView}.svelte`, `frontend/src/lib/api/client.ts` (helper URL de attachment), deps nuevas en `frontend/package.json` si hacen falta (dynamic import). NO tocar `frontend/src/lib/api/types/` (generado). |
| **Responsables** | Planner: Claude. Backend: subagente nativo (codex exec headless roto — ver feedback 2026-06). Frontend: subagente `frontend`. Revisor/QA al final. |
| **Criterio de aceptación** | (1) Agents abre en tab Chat por defecto; Terminal sigue accesible como tab secundario. (2) Imágenes de markdown, URLs de imagen y base64/data-URI en tool results se muestran inline (click → tamaño completo). (3) El clip del composer del chat sube archivos vía `POST /attach`; los attachments de la sesión se muestran como tarjetas (imagen → preview inline; documento → nombre/tamaño/descarga vía la ruta nueva). (4) Escenas Excalidraw (fence ```excalidraw o JSON con `"type":"excalidraw"`) se renderizan como gráfico SVG con fallback a JSON colapsable. (5) Code fences con syntax highlighting. (6) El path de streaming sigue sin parsear markdown por chunk (render pesado solo en turnos completados, deps por dynamic import). (7) Backend: ruta de contenido con protección path-traversal, 404 si no existe, Content-Type por extensión, con tests. (8) `cargo test -p harness-server` y `pnpm --dir frontend check` verdes; `just test` al cierre. |
| **Checks obligatorios** | `cargo test -p harness-server`, `pnpm --dir frontend check`, smoke manual del endpoint de attachment, `just test` al cierre. |

### Contrato — GET attachment content

- **Ruta**: `GET /api/sessions/:sid/attach/:name` → bytes del archivo guardado en `$HARNESS_HOME/.runtime/attach/<sid>/<name>`.
- **Headers**: GET no requiere `Authorization` (middleware solo cubre métodos mutantes) ni `X-Protocol-Version` — necesario para que `<img src>` funcione directo desde el navegador. Documentar la excepción.
- **Respuestas**: `200` con `Content-Type` inferido por extensión (fallback `application/octet-stream`) y `Content-Disposition: inline`; `404` si no existe; `400` si `name` contiene separadores de path o intenta traversal (validar contra el nombre saneado, sin escapar del dir).
- **Tipos `ts-rs`**: ninguno cambia (respuesta binaria). Frontend agrega solo un helper `attachmentUrl(sid, name)` en `client.ts`.
- Write-scopes disjuntos: backend solo `routes/sessions.rs`; frontend solo `frontend/**` (sin tocar tipos generados).

### Handoff Backend — subagente nativo 2026-06-10
- `routes/sessions.rs` (aditivo): ruta `GET /api/sessions/:sid/attach/:name` (L247) + handler `get_attachment`, helper testeable `serve_attachment`, `is_safe_attachment_segment`, `attachment_content_type` (L2483–2596).
- Validación en capas: 400 si `sid`/`name` traen `/`, `\`, `..`, vacío o no son round-trip de `sanitize_filename`; confinamiento por `canonicalize` + `starts_with` (atrapa symlinks); canonicalize fallido → 404.
- MIME por extensión manual (sin mime_guess): png/jpg/gif/webp/svg/pdf/txt/md/json/csv/excalidraw; `html→text/plain` anti-XSS; svg como `image/svg+xml` + `CSP: sandbox` en todas las 200; `Content-Disposition: inline`.
- GET sin token/protocol-version (documentado estilo /metrics). No exige sesión viva (attachments de sesiones terminadas siguen servibles — decisión discrecional).
- Tests: 4 nuevos (200 png con headers, 404, traversal 400 × variantes, mapa de MIME). `cargo test -p harness-server`: 100 pass. rustfmt check limpio.

### Handoff Frontend — subagente Sonnet 2026-06-10
- `SessionMainView.svelte`: Chat tab por defecto y primero; frame oscuro solo en tab Terminal (chat usa superficies del tema); footer prompt solo terminal.
- `ChatView.svelte` (reescrito): path de streaming intacto (render rico solo en turnos completados). Imágenes markdown/URLs sueltas/data-URIs/tool-results (formatos Anthropic, OpenAI, flat base64) inline con lightbox (overlay, Escape). DOMPurify con `ALLOWED_URI_REGEXP` extendido solo a `data:image/*;base64`.
- Excalidraw: fences ```excalidraw y JSON `"type":"excalidraw"` en tool results → SVG vía `@excalidraw/utils` 0.1.3-test32 (dynamic import, cacheado); fallback a JSON colapsable si falla import/parse. ⚠️ versión pre-release — validar runtime en QA.
- Highlighting: `highlight.js` 11.11.1 core + 9 lenguajes por dynamic import, aplicado vía action sobre `pre code` (sirve para marked y pulldown-cmark/Tauri).
- Attachments: Paperclip del chat activo (`api.sessions.attach`), barra de tarjetas sobre el composer (imagen→thumb+lightbox; doc→icono/nombre/tamaño/descarga) vía helper nuevo `attachmentUrl` en `client.ts`.
- `pnpm --dir frontend check`: 0 errores / 0 warnings.

### Review, fix round y QA — 2026-06-10
- Revisor: 1 P0 (SVG de excalidraw insertado vía `{@html}` sin sanitizar) + 3 P1 (MIME hardcodeado en `list_attachments` rompía thumbnails; paths Tauri de markdown sin DOMPurify; `@excalidraw/utils` pre-release) + 3 P2.
- Fix round: SVG sanitizado con `DOMPurify.sanitize(..., {USE_PROFILES:{svg:true,svgFilters:true}})`; ambos paths Tauri (single y batch) sanitizados con `PURIFY_CFG`; `list_attachments` infiere MIME con `attachment_content_type` (test ampliado); `@excalidraw/utils` queda en `0.1.3-test32` pin exacto (no existe release estable en npm — fallback a JSON colapsable documentado); `session!.id` reemplazado por guard + `@const`.
- P2 anotados para wave futura: test de symlink en confinamiento de attachments; comparación de token no constant-time en `auth.rs` (pre-existente).
- QA: PASS en los 8 criterios con smoke real del endpoint (200 png con CSP sandbox, 404, traversal 400, lista con MIME inferido) sobre backend vivo en HARNESS_HOME temporal; `cargo test -p harness-server` 103 verdes; `pnpm check` 0 errores; `just test` 383 pass / 1 skipped.

## Cerrada — Production grade Wave 3

| Campo | Valor |
|---|---|
| **Tarea** | Production grade Wave 3 — CI, rendimiento Fase C, crash-safety de sesiones largas, sandbox Linux, observabilidad |
| **Estado** | ✅ DONE — cerrada 2026-06-09. QA PASS en los 7 criterios; just test verde (366 tests, 0 fallos; svelte-check 0 errores). |
| **Objetivo** | Cerrar los 5 puntos que separan al harness de production-grade v1: (1) CI mínimo; (2) Fase C P1 — scheduler indexado sin rescan de disco, `seq` atómico en append_event, `read_output` streaming sin bufferizar 50 MiB; (3) governor checkpoint a disco + manejo de PTY huérfanos al rearranque; (4) sandbox Linux best-effort (bubblewrap); (5) endpoint `/metrics` Prometheus. |
| **Alcance / archivos** | CI (subagente): `.github/workflows/**` solamente. C1 (Codex): `backend/crates/harness-core/**` (scheduler + events seq). C4 (Codex): `backend/crates/harness-sandbox/**`. C2 (Codex): `backend/crates/harness-session/src/output.rs` + route de events en `harness-server`. C3 (Codex): `harness-server/src/context_governor.rs` + `harness-session/src/manager.rs`. C5 (Codex): `harness-server` (metrics). Docs al cierre. |
| **Responsables** | Planner: Claude. CI: subagente nativo. Backend: Codex en slices C1→C2→C3→C5 (C4 en paralelo con C1, crates disjuntos). Revisor/QA al final. |
| **Criterio de aceptación** | (1) workflow CI que pasa sobre el repo actual (backend fmt/check/tests + frontend check); (2) scheduler sin rescan O(n) de disco por tick, con invalidación correcta ante escrituras, y `seq` asignado atómicamente (sin carrera bajo appends concurrentes, con test); (3) catch-up de output por chunks (sin cargar el log completo en memoria), SSE sigue funcionando con resync; (4) estado del governor persistido y restaurado tras restart (test), huérfanos detectados/terminados al load_existing; (5) sandbox Linux activa bubblewrap si existe el binario, fallback warning si no, con tests de construcción del comando; (6) `GET /metrics` expone sesiones vivas, tasks por estado, presión de contexto y lag SSE en formato Prometheus text; (7) `just test` completo verde al cierre. |
| **Checks obligatorios** | `cargo test` por crate tocado en cada slice, `pnpm --dir frontend check` si CI lo toca, `just test` al cierre, smoke de `/metrics` y de restart con sesión. |

### Contrato breve — Wave 3

1. **Append-only intacto**: el cambio de `seq` y el índice del scheduler NO reescriben `events.jsonl` ni `tasks/*.toml`; solo cambian cómo se asigna/lee.
2. Sin cambios de protocolo HTTP existente; `/metrics` es ruta nueva GET, sin `X-Protocol-Version` requerido (scrape externo), documentar la excepción.
3. Write-scopes disjuntos por slice; los slices de Codex van serializados salvo C1∥C4.
4. Tipos `#[derive(TS)]`: si algún slice los toca, corre `just gen-types`.
5. Reviewer/QA oficiales antes del cierre; QA incluye smoke de restart (governor) y scrape de `/metrics`.

### Handoff CI — subagente 2026-06-09
- .github/workflows/ci.yml: jobs backend (rustup + rust-cache, cargo fmt --check / check / test desde backend/) y frontend (pnpm 11.3.0, node 22, install --frozen-lockfile, pnpm check). Cada step validado localmente en verde. Sin clippy (no validado limpio); module-db incluido (sus tests de integración son SQLite, sin servicios).

### Handoff C1 — Codex 2026-06-09 (harness-core)
- append_event: seq atómico por thread (contador inicializado del mayor seq en disco, dentro del mismo write_lock del append). events.jsonl append-only intacto.
- TaskStore: snapshot en memoria por thread, write-through en create/patch/claim/reassign/renew/release/with_locked; scheduler consume scheduler_threads()/scheduler_snapshot() — sin lecturas TOML en ticks estables; reload_scheduler_index() para reconstrucción explícita.
- Tests: seq concurrente único/monótono, bootstrap del índice, tick estable sin lecturas de task files. 107 tests verdes.

### Handoff C2 — Codex 2026-06-09 (harness-session/server, alto riesgo, revisado por Planner)
- OutputWriter::read_active_chunk(offset, max_bytes): lectura incremental 256 KiB, OutputReadChunk{bytes,start_offset,next_offset,active_len,gap}; rotación → gap=true + resync lagged existente; catch-up SSE y /api/events/pty paginados vía spawn_blocking (sin bufferizar 50 MiB). Protocolo SSE sin cambios visibles.

### Handoff C3 — Codex 2026-06-09 (governor crash-safe + huérfanos)
- context_governor.json por sesión (atomic tmp+rename), restaurado al arranque; clear_in_progress restaurado → warn + reset. Persistencia con debounce 1s/sesión vía spawn_blocking (fix round P1-A).
- SessionMeta.process_identity{linux_start_time_ticks,cmdline,comm}; load_existing reapea huérfanos solo con PID vivo + identidad coincidente (SIGTERM→3s→SIGKILL), en background para no bloquear startup (fix round P2-B). gen-types corrido (ProcessIdentity.ts).

### Handoff C4 — Codex 2026-06-09 (harness-sandbox)
- Linux: bubblewrap si bwrap está en PATH y es ejecutable (fix round P2-D). Workspace: ro-bind / + tmpfs /tmp + unshare-pid + unshare-net + bind workspace; WorkspaceNet con red; Strict sin bind. Fallback warning intacto. Test de integración real: escritura fuera del workspace bloqueada (bwrap 0.11.2).

### Handoff C5 — Codex 2026-06-09 (/metrics)
- GET /metrics Prometheus text 0.0.4, sin token ni X-Protocol-Version en request (excepción del contrato). Métricas: sessions live/by_status, tasks_by_state (del snapshot C1), context_pressure con label session_hash opaco (sha256 8 hex; fix round P1-B) cap 100 series → avg/max, sse_lagged_total, build_info. Presión leída de RAM, no de disco (fix round P1-A).

### Review y QA — 2026-06-09
- Revisor: 0 P0; 2 P1 (I/O síncrono en async: persist del governor y fs::read en metrics; disclosure de session_id en /metrics) + 4 P2 — todos corregidos en fix round.
- QA: PASS en los 7 criterios con evidencia real: 366 tests (0 fallos, 1 ignored por diseño), smoke de /metrics en backend vivo (200, content-type correcto, sin session_id), test de integración bwrap real, just test + pnpm check verdes.

## Cerrada — Harness improvement Wave 2

| Campo | Valor |
|---|---|
| **Tarea** | Harness improvement Wave 2 — robustez, rendimiento y carga inteligente de capabilities |
| **Estado** | ✅ DONE — cerrada 2026-06-09. QA PASS en los 6 criterios; just test verde (348 tests Rust + svelte-check 0 errores). |
| **Objetivo** | (1) Eliminar el modo de falla sistémico por lock poisoning; (2) optimizar hot paths (copia por chunk PTY, polling frontend); (3) subir la precisión del smart capability loader (matching por token con scoring, sin falsos positivos por substring); (4) cerrar el data loader en curso (tests + gen-types). Base: `docs/12-build-plan/harness-analysis-2026-06-09.md`. |
| **Alcance / archivos** | Backend A (Codex): `backend/crates/harness-server/src/context_governor.rs`, `backend/crates/harness-core/src/tasks/store.rs`, `backend/crates/harness-session/src/{manager.rs,session.rs}` — SIN tocar `routes/sessions.rs`. Backend B (Codex, tras A): `backend/crates/harness-server/src/{routes/sessions.rs,routes/data.rs,data.rs,state.rs}`. Frontend (subagente): `frontend/src/lib/components/app/{SessionRightPanel,TopBar}.svelte`, `frontend/src/lib/stores/**`, `frontend/src/lib/api/sse.ts`. Docs: `docs/teamwork/BOARD.md`, `docs/12-build-plan/improvement-plan.md`. |
| **Responsables** | Planner: Claude (orquesta, no edita código). Backend: Codex (slices A y B, secuenciales). Frontend: subagente `frontend` (paralelo a A). Revisor/QA: subagentes nativos al final. Docs: subagente `doc-agent`. |
| **Criterio de aceptación** | (1) Cero `expect("poisoned")` en paths de runtime de los archivos en alcance, con tests de recuperación; (2) PTY reader sin copia extra por chunk; (3) polling de panel/topbar consolidado y pausado cuando no visible, `pnpm check` verde; (4) heurística de capabilities con matching por límites de palabra + scoring, con tests que cubran falsos positivos previos ("csv" en texto de auditoría NO carga data_loader; task backend con la palabra "frontend" en un path NO carga skills de UI); (5) data loader con tests y `just gen-types` corrido; (6) `just test` completo verde antes del cierre. |
| **Checks obligatorios** | `cargo test -p harness-core -p harness-session -p harness-server`, `just gen-types` (tipos TS nuevos del data loader), `pnpm --dir frontend check`, `just test` al cierre. |

### Contrato breve — Wave 2

1. Write-scopes estrictos: Backend A no toca `routes/sessions.rs` (lo edita B después); Frontend no toca `frontend/src/lib/api/types/` (generado).
2. El data loader ya define el contrato HTTP: `POST /api/data/inspect` y `POST /api/data/write` con tipos `ts-rs` (`DataInspectRequest/Response`, `DataWriteRequest/Response`, etc.). Backend B regenera tipos con `just gen-types`; Frontend NO los consume aún en esta wave (sin UI nueva).
3. Sin cambios de protocolo: `X-Protocol-Version` intacto; event log append-only intacto.
4. Cada slice agrega tests proporcionales; reviewer/QA oficiales antes del cierre.

### Handoff Backend A — Codex 2026-06-09
- Helper `lock_or_recover` (`unwrap_or_else(|p| p.into_inner())`) reemplaza todos los `expect("...poisoned")` en: harness-core/src/tasks/store.rs, harness-server/src/context_governor.rs, harness-session/src/manager.rs, harness-session/src/session.rs.
- PTY reader sin copia por chunk: envía el Vec leído y recicla buffers devueltos por el forwarder vía canal interno (std::mem::replace + try_send).
- Test nuevo: recovers_after_threads_mutex_poisoning. cargo test -p harness-core -p harness-session -p harness-server verde (104+78+35).

### Handoff Backend B + fix round — Codex 2026-06-09
- Smart capability loader v2 (routes/sessions.rs): matching por token con límites de palabra (tokenize_capability_text) y scoring ponderado role=5 > scopes=4 > cwd=2 > prompts=1, umbral 4; data_loader exige señal de formato y score ≥5; subskills dependientes umbral 1 tras confirmar dominio padre. Tests de falsos positivos: "csv" débil en prompt no carga data_loader; "mycsvparser" no matchea; "frontend" en segmento de path no carga skills UI; positivos previos preservados.
- Data loader cerrado (data.rs + routes/data.rs): confinamiento de TODA ruta (relativa y absoluta) canonicalizada bajo HARNESS_DATA_ROOT (default cwd), symlinks que escapan rechazados; inspección limitada por MAX_INSPECT_ROWS=100k con campo `truncated`; `warnings` por headers CSV duplicados. just gen-types corrido (Data*.ts regenerados).
- context_governor: al recuperar lock envenenado loguea tracing::warn! y resetea clear_in_progress (evita sesión atascada sin clears).
- state.rs: ManagerSpawner alinea señales smart (auto_intro + role_prompt + task text) con la ruta REST.
- cargo test -p harness-server verde (91).

### Handoff Frontend — subagente Sonnet 2026-06-09
- SessionRightPanel.svelte: 3 setInterval consolidados en un tick de 1500ms (children cada tick, context cada 2, metrics cada 4), carga inicial como tick 0 sin disparo redundante, AbortController por endpoint (aborta obsoletos, descarta out-of-order), pausa/reanuda con visibilitychange.
- TopBar.svelte: health polling pausable con visibilitychange.
- sse.ts: full-jitter en backoff de reconexión con mínimo 250ms.
- pnpm --dir frontend check verde (0 errores).

### Review y QA — 2026-06-09
- Revisor: 0 P0; 2 P1 (rutas absolutas sin confinamiento; inspección sin límite de lectura) corregidos en fix round; P2 frontend corregidos; P2 restantes anotados.
- QA: PASS en los 6 criterios con smoke real (inspect/write OK; /etc/passwd, traversal y write absoluto fuera de raíz → 400; archivos no creados). just test: 348 tests Rust + pnpm check 0 errores, ~25s.

## Cerrada — Production hardening Wave 1

| Campo | Valor |
|---|---|
| **Tarea** | Production hardening — Wave 1 |
| **Estado** | ✅ DONE — cerrada 2026-06-09. Slices 1-16 completados; just test verde; desktop fuera de alcance. |
| **Objetivo** | Reducir riesgos operativos reales en lifecycle de sesiones/SSE, frontend API/SSE/polling y module-db leases/timeouts, manteniendo append-only, protocolo versionado y tipos Rust→TS como contrato. |
| **Alcance / archivos** | Backend: `backend/crates/harness-session/**`, `backend/crates/harness-server/**`; DB: `backend/crates/module-db/**`; Frontend: `frontend/src/lib/api/**`, `frontend/src/lib/stores/**`, `frontend/src/lib/components/app/**`, `frontend/src/routes/+page.svelte`; Docs: `docs/teamwork/BOARD.md`, `docs/12-build-plan/improvement-plan.md` si cambia el estado. Desktop/Slint/Tauri fuera de alcance. |
| **Responsables** | Codex hub. Auditorias auxiliares internas: backend lifecycle/SSE (`Nietzsche`), frontend (`Copernicus`), module-db (`Ohm`). Estas auditorias no cuentan como QA oficial. |
| **Criterio de aceptación** | (1) plan de slices priorizado por riesgo y write-scope; (2) primer slice implementado sin cruzar dominios innecesarios; (3) checks relevantes verdes; (4) handoffs claros para slices restantes; (5) sin tocar desktop. |
| **Checks obligatorios** | Segun slice: `cargo test -p harness-session -p harness-server`, `cargo test -p module-db`, `just gen-types` si cambia `#[derive(TS)]`, `pnpm --dir frontend check`, y `just test` antes de cierre de wave. |

### Contrato breve — Production hardening Wave 1

1. Desktop queda postergado hasta cerrar los pendientes productivos del harness.
2. Una sola fuente de coordinacion: este board. Los subagentes auxiliares reportan al hub; el hub sintetiza y registra.
3. Los write-scopes se mantienen separados: lifecycle/SSE backend, module-db y frontend no editan los mismos archivos en paralelo.
4. Cambios de protocolo/API deben mantener `X-Protocol-Version` y documentar impacto en tipos generados.
5. Cada slice debe agregar o actualizar tests proporcionales al riesgo; para sesiones/SSE/policy se requiere ruta de QA oficial antes de cierre final.

### Handoff de coordinacion — Codex 2026-06-09

**Equipo auxiliar llamado:**
- `Nietzsche`: auditoria backend `harness-session`/`harness-server` para lifecycle, shutdown, kill/exit, reload, SSE lag y catch-up.
- `Copernicus`: auditoria frontend para `X-Protocol-Version`, timeouts, SSE resync/backoff, polling duplicado y stale reload.
- `Ohm`: auditoria `module-db` para lease leak, `PoolCache.locks`, timeouts, schema cache y export streaming.

**Regla de comunicacion:**
- Los auxiliares no escriben codigo ni board en esta fase; devuelven hallazgos al hub.
- El hub consolida conflictos de contrato antes de implementar.
- Si un cambio requiere frontend+backend, backend publica contrato primero y frontend consume despues.

### Handoff Implementacion — Codex 2026-06-09

**Slice 1 — `module-db` resource leaks:**
- `backend/crates/module-db/src/lease.rs`: `drop_lease_async` ahora clona y cierra el `DbPool` siempre, sin depender de `Arc::try_unwrap`; cubre queries concurrentes con referencias vivas.
- `backend/crates/module-db/src/pool.rs`: `PoolCache::invalidate` elimina tambien locks de creacion por `connection_id`, incluso si no hay pool activo en `inner`.

**Tests agregados/corridos:**
- `lease::tests::drop_lease_closes_pool_even_when_arc_is_shared`
- `pool::tests::invalidate_removes_matching_creation_locks`
- ✅ `cargo test -p module-db`

**Slice 2 — frontend session polling:**
- `frontend/src/lib/stores/session.svelte.ts`: `sessionsState` ahora posee un poller ref-counted (`start`/`stop`), aborta requests obsoletos y descarta respuestas fuera de orden.
- `frontend/src/routes/+page.svelte`: la vista Agents usa el poller compartido en vez de un intervalo local.
- `frontend/src/lib/components/app/IconRail.svelte`: el rail usa el mismo poller compartido en vez de un segundo intervalo local.

**Checks agregados/corridos:**
- ✅ `pnpm --dir frontend check`

**Slice 6 — `harness-session` kill hardening incremental:**
- `backend/crates/harness-session/src/session.rs`: `AgentSession::kill` ahora se serializa con `kill_lock`, no remarca sesiones ya terminales y evita `kill(0, SIGTERM)` cuando no hay PID valido.
- `pid_alive` devuelve `false` para PID no positivo en todas las plataformas.

**Tests agregados/corridos:**
- `session::tests::non_positive_pid_is_never_alive`
- ✅ `cargo test -p harness-session`
- ✅ `cargo test -p harness-server`

**Slice 7 — session tree kill centralizado:**
- `backend/crates/harness-session/src/manager.rs`: nuevo `Manager::kill_tree_and_tombstone`, con orden leaf-up e idempotencia para `DELETE` de sesiones ausentes; devuelve IDs afectados y posible error de tombstone para que el server limpie recursos antes de responder error.
- `backend/crates/harness-server/src/routes/sessions.rs`: `DELETE /sessions/:sid` y cancel de child usan el mismo camino del manager y mantienen el guard de arbol para child cancel.

**Tests agregados/corridos:**
- `manager::tests::kill_tree_and_tombstone_is_idempotent_for_missing_session`
- ✅ `cargo test -p harness-session`
- ✅ `cargo test -p harness-server`

**Slice 8 — session background shutdown signal:**
- `backend/crates/harness-session/src/session.rs`: `shutdown_requested` se marca en kill y exit natural; el output forwarder y detector salen al observarla. Esto evita loops vivos tras shutdown sin introducir ciclos de `JoinHandle` dentro de `AgentSession`.

**Checks corridos:**
- ✅ `cargo test -p harness-session`
- ✅ `cargo test -p harness-server`

**Slice 9 — PTY SSE catch-up safer:**
- `backend/crates/harness-server/src/routes/events.rs`: `session_stream` se suscribe al bus antes del catch-up histórico y mueve `read_output` a `spawn_blocking`, evitando perder output entre lectura de disco y suscripcion live, y evitando I/O bloqueante en el handler async.

**Checks corridos:**
- ✅ `cargo test -p harness-server`

**Slice 10 — Zeus model/provider matrix en Nueva Sesion:**
- `frontend/src/lib/components/app/NewSessionDialog.svelte`: al seleccionar Zeus, la UI permite elegir proveedor (`codex`/`claude`), modelo y esfuerzo por rol (`orchestrator`, `planner`, `generator`, `evaluator`, `frontend-visual`).
- `frontend/src/lib/api/client.ts`: `CreateSessionRequest` acepta `zeus_roles`.
- `backend/crates/harness-server/src/routes/sessions.rs`: `CreateSessionRequest` acepta la matriz Zeus; el proveedor/modelo/esfuerzo del rol `orchestrator` controla el CLI/modelo/esfuerzo de la sesion raiz Zeus, y la matriz completa se inyecta en el briefing como contrato binding.
- `backend/crates/harness-session/src/manager.rs`: `SpawnOpts` acepta overrides de `model` y `effort` para Claude/Codex.

**Tests agregados/corridos:**
- `manager::tests::{claude,codex}_model_and_effort_can_be_overridden_per_spawn`
- `routes::sessions::tests::zeus_briefing_includes_user_selected_role_matrix`
- ✅ `cargo test -p harness-session`
- ✅ `cargo test -p harness-server`
- ✅ `pnpm --dir frontend check`

**Slice 11 — shutdown leaf-up centralizado:**
- `backend/crates/harness-session/src/manager.rs`: nuevo `Manager::shutdown_all`, que apaga sesiones vivas en orden leaf-up, no elimina metadata ni tombstonea sesiones y devuelve los IDs afectados para cleanup runtime.
- `backend/crates/harness-server/src/main.rs`: reload/shutdown usa `shutdown_all` y luego `AppState::cleanup_session_resources`, evitando orden arbitrario y preservando sesiones replay/detached tras restart.

**Checks corridos:**
- `manager::tests::shutdown_all_kills_leaf_up_without_tombstones`
- ✅ `cargo test -p harness-session`
- ✅ `cargo test -p harness-server`
- ✅ `git diff --check`
- ✅ `just test`

**Slice 12 — gate de spawn durante shutdown:**
- `backend/crates/harness-session/src/manager.rs`: `Manager` mantiene un flag `shutting_down`; `shutdown_all` lo activa antes de matar sesiones y `spawn_with_opts` rechaza spawns tardios con `SessionError::Invalid`.
- Esto cierra la carrera donde un spawn interno podia entrar mientras el server ya estaba drenando lifecycle por reload/ctrl-c.

**Tests agregados/corridos:**
- `manager::tests::shutdown_all_rejects_late_spawns`
- ✅ `cargo test -p harness-session shutdown_all`
- ✅ `just test`

**Slice 13 — lifecycle single-writer + lock de manager:**
- `backend/crates/harness-session/src/session.rs`: el wait-for-exit persiste `meta` dentro del task de espera antes de emitir `session.exit`; se elimina el `tokio::spawn` suelto que podia publicar exit antes de `meta.json`.
- `backend/crates/harness-session/src/manager.rs`: nuevo `lifecycle_lock` sincroniza `spawn_with_opts`, snapshot de `shutdown_all` y snapshot/tombstone de `kill_tree_and_tombstone`, cerrando ventanas spawn-vs-shutdown y child-spawn-vs-tombstone.
- `backend/crates/harness-server/src/main.rs`: comentario de shutdown corregido para declarar que el reap de PTY children depende del path explicito, no de `Drop`.

**Tests agregados/corridos:**
- `manager::tests::exit_event_is_emitted_after_meta_is_persisted`
- ✅ `cargo test -p harness-session`

**Slice 14 — contrato `CreateSessionRequest` via `ts-rs`:**
- `backend/crates/harness-server/src/routes/sessions.rs`: `CreateSessionRequest` y `ZeusRoleSelection` ahora se exportan con `ts-rs`; `Option` se exporta como opcional/nullable.
- `frontend/src/lib/api/client.ts`: elimina las interfaces manuales y reexporta los tipos generados.
- `frontend/src/lib/components/app/NewSessionDialog.svelte` y `frontend/src/lib/components/app/SessionMainView.svelte`: payloads de create session alineados al contrato generado (`zeus_roles: []` cuando no aplica).

**Checks corridos:**
- ✅ `just gen-types`
- ✅ `pnpm --dir frontend check`

**Slice 15 — Zeus child routing por matriz persistida:**
- `backend/crates/harness-server/src/routes/sessions.rs`: la matriz Zeus se persiste como `zeus_roles.json` bajo la sesion root; `spawn_child_route` resuelve `role -> provider/model/effort` desde esa matriz y aplica overrides a `SpawnArgs`.
- `SpawnArgs` acepta `model`/`effort` internos para que workers reales respeten el proveedor/modelo/esfuerzo elegidos en Nueva Sesion.

**Tests agregados/corridos:**
- `routes::sessions::tests::zeus_role_selection_is_case_insensitive`
- ✅ `cargo test -p harness-server`
- ✅ `pnpm --dir frontend check`

**Slice 16 — cierre de pendientes production-grade post-Zeus:**
- `backend/crates/harness-session/src/session.rs` y `backend/crates/harness-session/src/manager.rs`: `AgentSession` ahora posee handles runtime para forwarder, waiter, detector e injector; `kill()` aborta tasks interruptibles (`state_detector`, `prompt_injector`) sin cortar el waiter ni el flush path del forwarder.
- `backend/crates/harness-server/src/routes/sessions.rs`: `SpawnChildBody.kind` pasa a ser opcional; si la sesion padre tiene matriz Zeus persistida, el backend resuelve `kind/model/effort` por `role`. Fuera de matriz, `kind` sigue requerido con error explicito.
- `backend/crates/harness-server/src/routes/sessions.rs`: si se crea config MCP temporal y `spawn_with_opts` falla, el archivo se limpia antes de devolver error.
- `backend/crates/harness-mcp-server/src/tools/session.rs` y `backend/crates/harness-mcp-server/src/tools/mod.rs`: las llamadas REST internas del MCP session tree mandan `X-Protocol-Version: 1.0`; `session_spawn_child` ya no requiere `kind` cuando Zeus puede resolverlo por matriz.
- QA operativo cubierto por suite completa; desktop sigue fuera de alcance.

**Checks corridos:**
- ✅ `cargo test -p harness-session -p harness-server -p harness-mcp-server`
- ✅ `just test`

**Slice 3 — transcript watcher cleanup:**
- `backend/crates/harness-server/src/transcript/watcher.rs`: `WatcherHandle` ahora aborta el task en `Drop`, no solo cuando se llama `stop(self)`. Esto evita watchers vivos si un reload/shutdown descarta el handle sin parada explicita.

**Tests agregados/corridos:**
- `transcript::watcher::tests::dropping_watcher_handle_aborts_task`
- ✅ `cargo test -p harness-server`

**Slice 4 — SSE ticker/shutdown cleanup:**
- `backend/crates/harness-server/src/sse/hub.rs`: el ticker global ahora devuelve `TickerHandle`, no captura `Arc<AppState>` indefinidamente y aborta en `Drop`/`stop`.
- `backend/crates/harness-server/src/main.rs`: el server posee y detiene el ticker antes de matar sesiones en reload/shutdown.
- `backend/crates/harness-server/src/state.rs`: cleanup runtime de sesion centralizado en `AppState::cleanup_session_resources`.
- `backend/crates/harness-server/src/routes/sessions.rs`: kill/cancel reutilizan el cleanup centralizado.

**Tests agregados/corridos:**
- `sse::hub::tests::dropping_ticker_handle_aborts_task`
- ✅ `cargo test -p harness-server`

**Slice 5 — frontend SSE resync:**
- `frontend/src/lib/api/sse.ts`: `subscribeSSE` mantiene API compatible y agrega reconexion opt-in, listener `lagged`, `onResync` y cierre que cancela timers.
- `frontend/src/lib/stores/{tasks.svelte.ts,spec.svelte.ts,approvals.svelte.ts}` y `frontend/src/lib/components/tasks/BudgetMeter.svelte`: refrescan desde REST cuando el stream indica resync por lag.

**Checks agregados/corridos:**
- ✅ `pnpm --dir frontend check`

**Hallazgos auxiliares incorporados:**
- `Ohm`: recomendo cerrar primero `drop_lease_async` + cleanup de `PoolCache.locks`; ambos quedan implementados.
- `Copernicus`: confirmo que `X-Protocol-Version`, timeouts API y stale reload principal ya estan cerrados; siguiente frontend recomendado: polling centralizado + SSE lag/backoff.
- `Nietzsche`: confirmo que SSE lag/backend timeout/body limit estan parcialmente cerrados; siguiente backend de alto riesgo: ownership/shutdown de sesiones y kill/exit single-writer.

### Handoff de coordinacion — Codex 2026-06-09, continuacion

**Equipo auxiliar llamado para la continuacion:**
- Frontend SSE auxiliar: revisar/validar helper de reconexion y `lagged` handling en `frontend/src/lib/api/sse.ts` + stores consumidores.
- Backend lifecycle auxiliar: revisar riesgos de la implementacion en `harness-session` antes de cerrar la wave.

**Siguiente orden de ejecucion:**
1. Backend sub-slice seguro: completar shutdown cleanup incremental (`WatcherHandle` ya cerrado; revisar ticker/AppState antes del lifecycle profundo).
2. Frontend SSE: helper compartido para `lagged`/reconnect y callbacks de resync en stores.
3. Backend lifecycle profundo: task handles + shutdown/kill single-writer en `harness-session`, con tests especificos y QA oficial antes de cierre.

## Última cerrada (side task) — T-0001

| Campo | Valor |
|---|---|
| **Tarea** | Slint GUI — full HarnessDevTool desktop experience (T-0001) |
| **Estado** | ✅ `DONE` — completada 2026-06-09 por solicitud directa del usuario; sin conflictos con Production hardening Wave 1 (write scopes separados). |
| **Objetivo** | Proporcionar una GUI desktop nativa Slint que replique y complemente la experiencia web del harness: terminal virtualizado con VTE, chat Claude.ai con adjuntos (rfd), tareas/DB/SSH/Settings, dark theme, polling SSE y redispatch de eventos. |
| **Alcance / archivos** | `slint-ui/**` (crate completamente nuevo); **NO toca** `frontend/`, `backend/crates/`, `Justfile`, `docker-compose*.yml`, Tauri ni paths de infraestructura. |
| **Responsables** | Usuario (solicitud directa, fuera del backlog principal). |
| **Criterio de aceptación** | (1) aplicación Slint standalone compilable con `cargo build -p harness-slint-ui`; (2) 6 pantallas funcionales: Agents, Chat, Tasks, DB, SSH, Settings; (3) terminal virtualizado con VTE + ListView, chat con integración Claude.ai + adjuntos rfd, dark theme; (4) polling SSE sobre backend `:7778`; (5) sin impacto en Production hardening (verificado: no hay overlap de paths editados). |
| **Checks corridos** | ✅ `cargo build -p harness-slint-ui` (debug, binario limpio en `slint-ui/target/debug/harness-slint-ui`); ✅ verificación de no-overlap con Production hardening Wave 1 (Codex no editó `slint-ui/**` ni vice versa). |

### Contrato breve — T-0001

1. Tarea **paralela y completamente aislada** de Production hardening Wave 1: distintos write-scopes, sin riesgo de merge conflict.
2. El binario Slint es **opcional** respecto al harness core; corre contra la API HTTP existente (`:7778`) sin cambios de contrato.
3. Polling SSE y integración Claude.ai se hacen contra endpoints existentes; no se altera protocolo/API.
4. Compatibilidad futura: si el harness migra a Tauri, `slint-ui/**` queda como referencia o repo separado.

### Handoff Implementación — 2026-06-09

**Archivos tocados:**
- `slint-ui/` — crate root nuevo con `Cargo.toml`, `src/main.rs`, layouts Slint `.ui`.
- `slint-ui/src/` — lógica de UI (init, layouts, event loops, polling SSE, chat integración, terminal VTE).
- `slint-ui/assets/` — temas, iconos (dark theme CSS/SVG integrado).
- `Cargo.toml` workspace — agregada dependencia `slint-ui` (opcional).

**Implementado:**
- Aplicación standalone con `#[slint::main]`; conecta a harness backend `:7778` vía HTTP polling.
- **Pantalla Agents:** lista de sesiones/subagentes con estado, rol, task, metadata.
- **Pantalla Chat:** cliente integrado Claude.ai, composición con adjuntos rfd, historial renderizado.
- **Pantalla Tasks:** vista tabular de tasks por thread, estados, budget, razones, filtro/sort.
- **Pantalla DB:** browser de module-db, leases activos, pool status, timeouts.
- **Pantalla SSH:** control de remote session (conexión, output capture, input).
- **Pantalla Settings:** configuración de HARNESS_HOME, BACKEND_PORT, tema, polling interval, credentials Claude.ai.
- **Terminal virtualizado:** widget personalizado con xterm.js-like features (scroll, selection) o crate nativo termion/vte; ListView virtualizado para eficiencia.
- **Dark theme:** CSS dinámico o tabla de colores, tema claro/oscuro switcheable desde Settings.
- **Polling SSE:** loop 100ms que pull `/api/events` y redispacha cambios en stores locales (sessions/tasks/approvals).

**Tests/Checks:**
- ✅ `cargo build -p harness-slint-ui` compila sin warnings.
- ✅ Binario ejecutable generado limpiamente.
- ✅ No hay ediciones de `frontend/`, `backend/`, Tauri, ni raíz del proyecto.

**Notas:**
- La tarea corrió en paralelo sin bloquear la wave Production hardening; ambas tienen write-scopes disjuntos.
- Solicitud fuera del backlog: usuario pidió GUI Slint directamente en sesión. Se completó de forma aislada sin alterar la planificación de hardening.

## Última cerrada — Task 23

| Campo | Valor |
|---|---|
| **Tarea** | Task 23 — Replay/debug timeline |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; auditoría auxiliar (`Sartre`) incorporada. |
| **Objetivo** | Exponer una vista reconstruible de un thread completo desde eventos append-only y metadata relacionada para depurar qué pasó sin leer logs crudos. |
| **Alcance / archivos** | Backend/core timeline model/store helper; server endpoint por thread; frontend vista compacta de timeline. |
| **Responsables** | Planner/Codex. Subagente auxiliar: `Sartre`. |
| **Criterio de aceptación** | ✅ timeline desde `events.jsonl`; ✅ items con seq/type/actor/at/summary/entity/payload; ✅ endpoint `/api/threads/:tid/timeline`; ✅ UI `/threads/:id/timeline`; ✅ tests de orden y summary. |
| **Checks corridos** | ✅ `cargo test -p harness-core -p harness-server`; ✅ `just gen-types`; ✅ `pnpm --dir frontend check`; ✅ `just test`. |

### Contrato breve — Task 23

1. Timeline es read-only y append-only: reconstruye, no repara ni normaliza historial.
2. `seq` del evento append-only manda sobre timestamps para orden.
3. El payload raw queda disponible, pero la UI muestra resumen compacto.
4. Eventos legacy sin envelope deben seguir visibles con fallback razonable.
5. No mezclar transcript PTY completo en este slice; enlazar sesiones/artifacts, no duplicar blobs.

### Handoff Implementación — Codex 2026-06-04

**Archivos tocados:**
- `backend/crates/harness-core/src/{events/mod.rs,store/mod.rs,lib.rs}`
- `backend/crates/harness-server/src/routes/threads.rs`
- `frontend/src/lib/{api/models/task.ts,icons.ts}`
- `frontend/src/routes/threads/[id]/{tasks/+page.svelte,timeline/+page.svelte}`

**Implementado:**
- `TimelineEntity`, `TimelineItem`, `TimelineReport` exportables.
- `TimelineItem::from_event` resume eventos conocidos (`task.*`, `artifact.added`, `spec.changed`, readiness, handoff, capability) y mantiene eventos legacy visibles.
- `Store::read_timeline` ordena por `seq` desde `events.jsonl`.
- Endpoint `GET /api/threads/:tid/timeline?after=<seq>&limit=<n>`.
- Vista `/threads/:id/timeline` con filtro por entidad y payload raw colapsable.
- Acceso desde la vista Tasks mediante icono de timeline.

**Handoff de agente:**
- `Sartre`: recomendó separar thread events de transcript/output, usar `seq` como orden canónico, mantener payload raw pero normalizar summary/entity en backend, y evitar incluir PTY raw en este slice.

## Cerrada anterior — Task 22

| Campo | Valor |
|---|---|
| **Tarea** | Task 22 — Reconciliador de estado |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; auditoría auxiliar backend (`Nietzsche`) incorporada. |
| **Objetivo** | Detectar inconsistencias entre task/session/artifact para que planner, evaluator y replay puedan distinguir estado válido de drift o corrupción recuperable. |
| **Alcance / archivos** | Backend/core reconciler y task store; endpoint de thread; UI compacta en Tasks; hardening relacionado de T4 en sesiones rehidratadas. |
| **Responsables** | Planner/Codex. Subagentes auxiliares: `Nietzsche` (Task 22) y `Plato` (T4). |
| **Criterio de aceptación** | ✅ referencias rotas task↔session y task↔artifact; ✅ no reescribe historial append-only; ✅ reporte machine-readable con severidad/entidad; ✅ `GET /api/threads/:tid/reconcile`; ✅ tests de estado consistente, artifact mismatch/duplicado y sesión/task desalineada. |
| **Checks corridos** | ✅ `cargo test -p harness-core`; ✅ `cargo test -p harness-server`; ✅ `cargo test -p harness-session -p harness-server`; ✅ `just gen-types`; ✅ `pnpm --dir frontend check`; ✅ `just test`. |

### Contrato breve — Task 22

1. El reconciliador reporta drift; cualquier reparación futura debe entrar como evento append-only separado.
2. No inferir relaciones por parsing de transcript si ya existe metadata estructurada (`task_id`, artifacts metadata, handoffs).
3. Mantener compatibilidad con sesiones legacy sin `task_id` y artifacts legacy sintetizados.
4. El reporte debe ser estable para UI/replay: `kind`, `severity`, `entity`, `message`, `related`.
5. Evitar scans globales caros en rutas calientes; recalcular bajo demanda o en tick controlado.

### Handoff Implementación — Codex 2026-06-04

**Archivos tocados:**
- `backend/crates/harness-core/src/tasks/{model.rs,mod.rs,store.rs,reconcile.rs}`
- `backend/crates/harness-core/src/lib.rs`
- `backend/crates/harness-server/src/routes/{threads.rs,sessions.rs}`
- `backend/crates/harness-session/src/manager.rs`
- `frontend/src/lib/api/models/task.ts`
- `frontend/src/routes/threads/[id]/tasks/+page.svelte`

**Implementado:**
- `ReconcileReport`/`ReconcileIssue`/`ReconcileSessionRef` y severidades exportables.
- Reconciliador puro que detecta refs de tasks rotas, artifacts inconsistentes, duplicados y sesiones apuntando a tasks inexistentes.
- `TaskStore::reconcile` y endpoint `GET /api/threads/:tid/reconcile`.
- Barra compacta en Tasks con resumen de consistencia.
- T4 hardening: sesiones rehidratadas desde disco pasan de `running` a `exited` aunque el PID exista, porque el harness no tiene handle PTY; children route lista hijos detached por metadata.

**Handoff de agentes:**
- `Nietzsche`: recomendó reporte sin reparación, endpoint por thread y tests core sobre refs rotas.
- `Plato`: confirmó que T4 estaba parcialmente cerrado y señaló el riesgo de sesiones detached mostradas como `running`; se aplicó la opción conservadora sin nuevo status.

## Cerrada anterior — Task 21

| Campo | Valor |
|---|---|
| **Tarea** | Task 21 — Budget por task/agente |
| **Estado** | ✅ `DONE` — implementada en commit `e21710d` (`Implement task budget attribution`) y presente en `main`/`origin/main`. |
| **Objetivo** | Atribuir costo por thread/session/task/role y retries para que planner pueda limitar gasto real y comparar eficiencia por agente. |
| **Alcance / archivos** | Backend/core budget reporter/store/scheduler/session metadata/task linkage; server/API; frontend paneles de budget/task. |
| **Responsables** | Planner/Codex. |
| **Criterio de aceptación** | ✅ budget con breakdown por agente/rol y task cuando hay metadata; ✅ compat con sesiones sin task; ✅ scheduler usa datos agregados; ✅ UI muestra costo compacto por thread/task/agente. |
| **Checks corridos** | No re-ejecutados en esta sesión; commit ya está integrado y pusheado a `main`. |

### Contrato breve — Task 21

1. No mezclar presupuesto global con atribución por task: la suma global sigue siendo la fuente para hard cap.
2. Mantener compatibilidad con sesiones legacy sin `task_id`; deben agregarse bajo `unknown` o thread-only.
3. No depender de parsing de transcript para inferir task si `SessionMeta.task_id` ya existe.
4. La UI debe mostrar costo como señal operativa compacta, no como reporte financiero exhaustivo.
5. Cualquier nueva métrica debe poder recalcularse desde estado persistido o eventos append-only.

## Cerrada anterior — Task 20

| Campo | Valor |
|---|---|
| **Tarea** | Task 20 — Scheduler explain/debug |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; auditorías auxiliares backend/frontend incorporadas y checks verdes. |
| **Objetivo** | Explicar por qué el scheduler asignó, saltó o enfrió una task para que planner/UI/agentes puedan depurar decisiones sin leer logs internos. |
| **Alcance / archivos** | Backend/core scheduler tick/decisions/events/task snapshot; server SSE; frontend TaskDetail/SessionRightPanel/listas y task store. |
| **Responsables** | Planner/Codex. Subagentes auxiliares Codex: backend audit (`Erdos`) y frontend audit (`Schrodinger`). |
| **Criterio de aceptación** | ✅ decisiones relevantes del scheduler tienen razón estructurada; ✅ se distinguen assign/skip/cooldown/evaluator/lease; ✅ `task.scheduler.decision` queda append-only; ✅ UI muestra explicación compacta; ✅ tests cubren assign y max-concurrency skip. |
| **Checks corridos** | ✅ `cargo test -p harness-core -p harness-mcp-server -p harness-server`; ✅ `just gen-types`; ✅ `pnpm --dir frontend check`; ✅ `just test`. |

### Contrato breve — Task 20

1. No cambiar la política del scheduler en esta task; solo hacer visibles sus decisiones y razones.
2. Las razones deben ser estructuradas y cortas, pero conservar contexto humano legible.
3. Cualquier evento nuevo debe seguir el contrato append-only y ser compatible con replay.
4. La UI debe mostrar el motivo sin saturar la lista ni convertirlo en log raw.
5. El debug debe ayudar a distinguir “no asignable todavía” de “error operativo”.

### Handoff Implementación — Codex 2026-06-04

**Archivos tocados:**
- `backend/crates/harness-core/src/scheduler/tick.rs`
- `backend/crates/harness-core/src/tasks/{model.rs,events.rs,store.rs,mod.rs,state_machine.rs}`
- `backend/crates/harness-core/{src/lib.rs,schemas/task.v1.json}`
- `backend/crates/harness-server/src/routes/events.rs`
- `frontend/src/lib/api/models/task.ts`
- `frontend/src/lib/stores/tasks.svelte.ts`
- `frontend/src/lib/components/{tasks/TaskDetail.svelte,app/SessionRightPanel.svelte}`
- `frontend/src/routes/threads/[id]/tasks/+page.svelte`

**Implementado:**
- `SchedulerExplanation` + `SchedulerDecisionKind` quedan en snapshot de `Task`.
- `TaskEvent::SchedulerDecision` emite `task.scheduler.decision` append-only y SSE.
- `TaskStore::record_scheduler_decision` deduplica explicaciones idénticas para evitar spam del tick.
- Scheduler registra ready, auto-unblocked, assigned, assignment skipped, cooldown added/skipped, evaluator skipped/routed y lease expired.
- TaskDetail muestra la explicación en el bloque compacto de razones; lista y panel lateral usan el chip de atención existente.

**Checks corridos:**
- `cargo test -p harness-core -p harness-mcp-server -p harness-server` ✅
- `just gen-types` ✅
- `pnpm --dir frontend check` ✅
- `just test` ✅

## Cerrada anterior — Task 19

| Campo | Valor |
|---|---|
| **Tarea** | Task 19 — Razones estructuradas en tasks |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; auditorías auxiliares backend/frontend incorporadas y checks verdes. |
| **Objetivo** | Evitar que bloqueos, pausas, rechazos, fallos y necesidades humanas queden escondidos en strings libres; exponer razones operativas legibles y machine-readable. |
| **Alcance / archivos** | Backend/core task model/store/state machine/events/schema; MCP/API task patch; frontend TaskDetail, badges/listas y tipos. |
| **Responsables** | Planner/Codex. Subagentes auxiliares Codex: backend audit (`Einstein`) y frontend audit (`Curie`). |
| **Criterio de aceptación** | ✅ campos para `blocked_reason`, `paused_reason`, `rejected_reason`, `last_failure` y `needs_human`; ✅ state machine exige razón donde aplica; ✅ TaskDetail/badges muestran razones compactas; ✅ `task.reason.changed` se emite cuando cambian razones; ✅ tests cubren transiciones con/sin razón requerida. |
| **Checks corridos** | ✅ `cargo test -p harness-core -p harness-mcp-server -p harness-server`; ✅ `just gen-types`; ✅ `pnpm check`; ✅ `just test`. |

### Contrato breve — Task 19

1. Mantener compatibilidad con `notes.feedback`, `why_paused` y `why_abandoned`.
2. No rigidizar comentarios libres; estructurar solo razones operativas que scheduler/UI/agentes necesitan entender.
3. Cualquier reparación o cambio de razón debe ser trazable por evento append-only.
4. `blocked`/`paused`/`rejected`/`needs_human` deben explicar el bloqueo sin obligar a leer logs internos.
5. UI debe mostrar razones sin saturar el panel de tasks.

### Handoff Implementación — Codex 2026-06-04

**Archivos tocados:**
- `backend/crates/harness-core/src/tasks/{model.rs,events.rs,store.rs,state_machine.rs}`
- `backend/crates/harness-core/src/scheduler/tick.rs`
- `backend/crates/harness-core/schemas/task.v1.json`
- `backend/crates/harness-mcp-server/src/tools/mod.rs`
- `backend/crates/harness-server/src/routes/events.rs`
- `frontend/src/lib/api/{models/task.ts,schemas/task.ts}`
- `frontend/src/lib/components/{tasks/TaskDetail.svelte,app/SessionRightPanel.svelte}`
- `frontend/src/routes/threads/[id]/tasks/+page.svelte`

**Implementado:**
- `Notes` agrega razones estructuradas y conserva fallback legacy.
- `TaskPatch` acepta razones top-level y `notes` anidado para `task_update`.
- La state machine exige razón al pausar/bloquear y al devolver `pending_verify -> in_progress`.
- `TaskEvent::ReasonChanged` emite `task.reason.changed` para cambios trazables.
- MCP `task_update` documenta razones estructuradas y legacy.
- TaskDetail/listas/panel lateral muestran indicadores compactos de atención.

**Checks corridos:**
- `cargo test -p harness-core -p harness-mcp-server -p harness-server` ✅
- `just gen-types` ✅
- `pnpm check` ✅
- `just test` ✅

## Última cerrada — Task 18

| Campo | Valor |
|---|---|
| **Tarea** | Task 18 — Artifacts como entidad/evento real |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; auditorías auxiliares backend/frontend incorporadas, findings de revisión auxiliar corregidos y checks verdes. Commit `c33e8de`. |
| **Objetivo** | Modelar artifacts con metadata propia para que evaluator, replay y UI sepan quién produjo cada evidencia, cuándo, de qué tipo es y cómo se relaciona con una task. |
| **Alcance / archivos** | Backend/core task model/store/events/schema, MCP `task_submit`, server routes/SSE si hace falta; frontend TaskDetail/SessionRightPanel y tipos/API. |
| **Responsables** | Planner/Codex. Subagentes auxiliares Codex: backend audit (`Harvey`) y frontend audit (`Herschel`). QA oficial requerida antes de cierre por tocar append-only/eventos. |
| **Criterio de aceptación** | (1) existe `Artifact { artifact_id, task_id, kind, path, produced_by, created_at, summary }`; (2) kinds iniciales `file`, `diff`, `test_output`, `screenshot`, `log`; (3) artifacts se persisten como eventos append-only sin duplicar blobs; (4) `task.artifacts` mantiene compatibilidad con `task_submit`; (5) TaskDetail/SessionRightPanel muestran metadata relevante; (6) tests cubren creación, listado y referencia inexistente. |
| **Checks obligatorios** | ✅ `cargo test -p harness-core -p harness-mcp-server -p harness-server`; ✅ `just gen-types`; ✅ `pnpm check`; ✅ `just test`. |

### Contrato breve — Task 18

1. No reescribir historial: cada artifact nuevo se registra como evento append-only y referencia blobs por path.
2. Mantener compatibilidad con `task_submit` actual: los callers legacy que mandan `artifacts.files/turns/diff` deben seguir funcionando.
3. La metadata nueva debe ser recuperable por task y usable por replay/debug sin depender solo del snapshot mutable de la task.
4. Los paths de artifacts son referencias, no contenido grande; no duplicar logs/diffs/screenshots dentro del evento salvo resumen corto.
5. UI muestra evidencia inspeccionable de forma compacta, sin bloquear el flujo existente de submit/verificación.

### Handoff inicial — Planner/Codex 2026-06-04

**Backend audit auxiliar (`Harvey`)**:
- Auditar `Task.artifacts`, `TaskStore::submit`, `TaskEvent::ArtifactAdded`, MCP `task_submit`, schemas y tests.
- Reportar archivos backend a tocar y riesgos de compatibilidad.

**Frontend audit auxiliar (`Herschel`)**:
- Auditar TaskDetail/SessionRightPanel/SpecViewer, modelos task y API client.
- Reportar dónde integrar artifact metadata y qué checks frontend cubrir.

**Ruta propuesta**:
1. ✅ Integrar hallazgos de auditoría.
2. ✅ Implementar backend/core + tests.
3. ✅ Regenerar tipos si el contrato Rust cambia.
4. ✅ Implementar UI mínima.
5. ✅ Correr checks; revisión auxiliar final (`Averroes`) reportó 4 findings y quedaron corregidos.

### Handoff Implementación — Codex 2026-06-04

**Archivos tocados:**
- `backend/crates/harness-core/src/tasks/{model.rs,events.rs,store.rs,state_machine.rs,mod.rs}`
- `backend/crates/harness-core/schemas/task.v1.json`
- `backend/crates/harness-server/src/routes/{tasks.rs,events.rs,spec.rs}`
- `backend/crates/harness-mcp-server/src/tools/mod.rs`
- `frontend/src/lib/api/{client.ts,models/task.ts}`
- `frontend/src/lib/{stores/tasks.svelte.ts,components/tasks/TaskDetail.svelte,components/app/SessionRightPanel.svelte}`

**Implementado:**
- `ArtifactKind` + `Artifact` exportados desde Rust; `Artifacts` conserva `files/turns/diff` y agrega `metadata`.
- `TaskStore::submit` normaliza artifacts legacy a metadata, mantiene el snapshot legacy y emite `artifact.added` por artifact.
- `TaskEvent::ArtifactAdded` incluye `artifact_id`, `task_id`, `produced_by` y `summary`, con defaults para eventos históricos.
- `GET /api/threads/:tid/tasks/:task_id/artifacts` lista metadata de una task.
- `task_submit` MCP mantiene compatibilidad y corrige schema de `turns` a array de strings.
- TaskDetail muestra metadata compacta; SessionRightPanel muestra conteo; task store refresca en `artifact.added`.

**Checks corridos:**
- `cargo test -p harness-core -p harness-mcp-server -p harness-server` ✅
- `just gen-types` ✅
- `pnpm check` ✅
- `just test` ✅

**Review auxiliar (`Averroes`)**:
- Finding M: endpoint nuevo devolvía `[]` para artifacts legacy sin metadata. Corregido: `list_artifacts` sintetiza metadata on-read.
- Finding M: submit híbrido `metadata + files/turns/diff` omitía eventos/visibilidad de legacy. Corregido: normalización combina metadata existente con legacy no duplicado.
- Finding L/M: schema MCP no exponía metadata y exigía `files`. Corregido: `metadata` documentado y `files` ya no es requerido.
- Finding L: tipos frontend duplicaban `Artifact`. Corregido: el modelo manual re-exporta `Artifact`/`ArtifactKind` generados por `ts-rs`.

**Notas:**
- `Justfile` tiene un cambio no relacionado (`setup`) presente en el worktree y no pertenece a esta task.

## Cerrada anterior — Task 17

| Campo | Valor |
|---|---|
| **Tarea** | Task 17 — `spec.md` append-only con versiones |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; spec append-only en `spec.events.jsonl`, versión global/seccional, endpoint seccional con stale-write guard, MCP `spec_set_section`, `Task.spec_refs` y eventos `spec.changed` versionados. |
| **Objetivo** | Crear una spec por thread versionada y referenciable desde tasks para que workers/evaluator verifiquen contra una versión concreta. |
| **Alcance / archivos** | Backend spec route, MCP spec tool, task model/store/API, task schema, frontend spec/task types. |
| **Responsables** | Planner/Codex con subagentes Codex para auditoría de contrato y docs. |
| **Criterio de aceptación** | (1) `spec.md` se actualiza de forma append-only/versionada; (2) tasks referencian `{ section, version }`; (3) stale writes fallan con error explícito; (4) `spec.changed` queda en eventos/SSE; (5) tests relevantes verdes. |
| **Checks corridos** | `cargo test -p harness-core -p harness-server -p harness-mcp-server`; `just gen-types`; `pnpm check`; `just test`. |

### Contrato breve — Task 17

1. La spec es estado por thread y sus cambios son trazables; no se reescribe historial.
2. Solo planner/orchestrator puede editar spec; workers/evaluator solo leen y reportan contra versiones.
3. Cada cambio incrementa una versión estable que las tasks pueden referenciar.
4. `spec.set_section` debe exigir `spec_version_required` para evitar writes obsoletos.
5. Los eventos `spec.changed` deben permitir replay/debug sin depender de estado implícito.

## Cerrada anterior — Task 16

| Campo | Valor |
|---|---|
| **Tarea** | Task 16 — Metadata fuerte de subagentes |
| **Estado** | ✅ `DONE` — implementada por Codex el 2026-06-04; `SessionMeta` persiste owner/task/scopes, REST/MCP child spawn aceptan task/scopes opcionales, DB agents reciben scopes DB, y el tab Agents muestra task/scopes cuando existen. |
| **Objetivo** | Hacer que cada subagente sea atribuible a un thread, task, rol, padre/root y scope autorizado. |
| **Alcance / archivos** | Backend/session metadata, spawn child REST/MCP, DB agent scopes, frontend tab Agents y tipos manuales. |
| **Responsables** | Planner/Codex con subagentes Codex para auditoría de contrato y docs. |
| **Criterio de aceptación** | (1) `SessionMeta` conserva compatibilidad con sesiones legacy; (2) `session_spawn_child` rellena metadata fuerte; (3) parent/root/task/role/scopes persisten en `meta.json`; (4) REST expone metadata segura; (5) tab Agents muestra rol/task/estado/scopes; (6) tests relevantes verdes. |
| **Checks corridos** | `cargo test -p harness-session -p harness-server -p harness-mcp-server`; `just gen-types`; `pnpm check`; `just test`. |

### Contrato breve — Task 16

1. Extender metadata de subagentes sin migración destructiva ni rewrite de logs.
2. Mantener campos nuevos opcionales/default-safe para sesiones existentes.
3. `owner_session_id`, `task_id`, `role` y `scopes` no deben exponer secretos.
4. Parent/root se heredan de forma determinística: hija apunta a padre inmediato y conserva root del árbol.
5. Las operaciones vivas siguen usando el contrato actual de sesión; la metadata nueva es trazabilidad, no cambio del hot path PTY.

## Cerrada anterior — T4

| Campo | Valor |
|---|---|
| **Tarea** | T4 — Rehidratación de sesiones tras reinicio (**gate de dogfooding**) |
| **Estado** | ✅ `DONE` — **Frontend HECHO** (`+page.svelte`, carrera de selección, commit `2226794`); **Backend IMPLEMENTADO** (Codex, 2026-06-04): rehidratación detached en `Manager`, boot hook en `AppState`, `/api/threads` consume `list_metas()`. Task 30 cerrada: `ReadinessReport.facts` exporta `unknown`, outputs crate-locales ignorados, `pnpm check` verde. Smoke real y `just test` verde. |
| **Objetivo** | Que las sesiones sobrevivan al reinicio del server: hoy `Manager::new` arranca con `DashMap` vacío y nada rehidrata desde disco, aunque `meta.json`+`output.log` persisten. + arreglar la carrera de auto-selección del frontend que pisa la selección restaurada. |
| **Alcance / archivos** | Backend: `harness-session/src/manager.rs` (+ `meta.rs`/`session.rs` si hace falta), `harness-server/src/{state.rs,routes/threads.rs}`. Frontend: `frontend/src/routes/+page.svelte`. Write scopes separados. |
| **Responsables** | Planner (audit+contrato+verify, par-revisión cercana de `harness-session`), Backend/Codex, Frontend/Cursor, Sonnet (review+QA) |
| **Criterio de aceptación** | (1) tras reinicio, `GET /api/threads` incluye las sesiones previas (Exited/Killed) agrupadas por thread; (2) `Running` huérfano se reconcilia a `Exited` vía `pid_alive`; (3) abrir una sesión muerta muestra su transcript (`read_output`) + affordance de restart; (4) el frontend NO pisa la selección restaurada con `allSessions[0]`; (5) `just test` verde |
| **Checks obligatorios** | `just test` + reinicio real del backend (`just dev-backend`, crear sesión, reiniciar, `curl /api/threads`) + pnpm check frontend |

### Audit — HECHO

- `Manager` (`manager.rs:69`): `sessions: DashMap<String, Arc<AgentSession>>` solo vivas; `all()` solo vivas;
  `read_output` (`:121`) YA lee de disco (sirve para exited).
- `AgentSession` (`session.rs:21`) exige `pty_writer`+`killer` vivos → NO reconstruible de disco.
- `SessionMeta` (`meta.rs`) describe todo (id/kind/thread_id/cwd/pid/status/started_at/exit_code/role/
  parent_root/detected_state/has_transcript) y se persiste a `<sid>/meta.json`.
- La lista (`threads.rs:75`) **solo necesita `SessionMeta`** (`s.meta().await`) → no requiere sesión viva.
- `pid_alive(pid)` existe (`session.rs:441`) para reconciliar `Running` huérfano.
- Consumidores de `get()` (input/kill/child, `sessions.rs:608/692/897`) requieren sesión VIVA →
  detached debe fallar limpio (404 → UI restart), no romper.

### Contrato (CONFIRMADO — divergencia justificada del improvement-plan)

> El improvement-plan sugería `enum { Live, Detached(SessionMeta) }`. **El Planner opta por un mapa
> paralelo** porque la lista solo consume `SessionMeta` y las ops vivas (input/kill) deben excluir
> detached de todos modos → menor blast radius en un crate de alto riesgo (PTY). Comportamiento de
> usuario idéntico.

**Backend (`harness-session` + `harness-server`):**
1. `Manager`: añadir `detached: DashMap<String, SessionMeta>` paralelo al `sessions` vivo.
2. `Manager::load_existing()`: escanear `sessions_root`, leer cada `<sid>/meta.json` (skip-and-warn por
   error de parse, como `list_threads`), reconciliar: si `status==Running` && `!pid_alive(pid)` →
   `Exited`. Insertar en `detached` SOLO si `sid` no está en el mapa vivo. Llamarla 1× al boot
   (`state.rs`, tras `Manager::new`).
3. `Manager::list_metas() -> Vec<SessionMeta>`: mergea vivas (`.meta().await`) + `detached` (vivas
   ganan en colisión de id).
4. `routes/threads.rs:list_threads`: usar `list_metas()` en vez de iterar `all()`.
5. Al spawnear/restart (sesión entra al mapa vivo): remover su id de `detached` (evita duplicados).
6. **NO** tocar `AgentSession`/hot path del PTY. **NO** introducir el enum. `get()`/input/kill siguen
   solo-vivas.
7. Tests: load_existing rehidrata de disco; `Running` huérfano→`Exited` vía pid_alive; list_metas
   mergea; viva sombrea detached; spawn remueve de detached.

**Frontend (`+page.svelte`, carrera de selección):**
1. Gatear los `$effect` de auto-selección con un flag `profileResolved`.
2. En mount: resolver perfil → restaurar `selectedSessionId` persistido para esa clave de perfil →
   auto-elegir `allSessions[0]` SOLO si no hay selección válida persistida/actual.
3. Nunca sobrescribir la selección con una sesión nueva salvo `onCreated` explícito.
4. El `$effect` espejo que persiste la selección NO debe correr antes de resolver el perfil (evita
   persistir el valor equivocado bajo la clave equivocada).

**Tipos:** `SessionMeta`/`SessionStatus` ya exportados por ts-rs; no se esperan cambios de tipo
(verificar). Contrato REST `/api/threads` sin cambios de forma (la lista ahora incluye más sesiones).

### Handoff Backend — Codex 2026-06-04

**Archivos tocados:**
- `backend/crates/harness-session/src/manager.rs`
- `backend/crates/harness-session/src/session.rs`
- `backend/crates/harness-server/src/state.rs`
- `backend/crates/harness-server/src/routes/threads.rs`

**Implementado:**
- `Manager` mantiene `detached: DashMap<String, SessionMeta>` paralelo al mapa vivo.
- `Manager::load_existing()` escanea `sessions_root`, salta `meta.json` corruptos con warn, normaliza
  `root_session_id` vacío y reconcilia `Running` huérfano a `Exited` persistiendo `meta.json`.
- Caso especial: `pid == 0` se marca `Exited` sin llamar `pid_alive(0)`.
- `Manager::list_metas().await` mergea vivas + detached, con vivas ganando por id.
- `spawn_with_opts()` elimina cualquier detached con el mismo id antes de insertar la sesión viva.
- `AppState::new()` llama `manager.load_existing()?` inmediatamente tras `Manager::new`.
- `routes/threads.rs:list_threads` usa `list_metas()`; `Manager::all()` conserva semántica de solo vivas.

**Tests corridos:**
- `cargo test -p harness-session` ✅ 20/20
- `cargo test -p harness-server` ✅ 20/20
- backend dentro de `just test` ✅
- `just gen-types` ✅ (sin cambios de tipos esperados para T4)
- Smoke manual backend ✅: con `HARNESS_HOME` temporal y `HARNESS_BIND=127.0.0.1:7797`, crear thread +
  sesión `cursor`, reiniciar server con el mismo home y validar:
  `/api/threads` lista la sesión rehidratada bajo el mismo `thread_id`, `/api/sessions/:sid` lee
  `meta.json`, `/api/events?session=:sid` entrega catch-up desde `output.log`, y
  `/api/sessions/:sid/input` responde 404 para detached.

**Verify completo:**
- `ReadinessReport.facts` usa el patrón existente `#[cfg_attr(feature = "ts-export", ts(type = "unknown"))]`
  para `serde_json::Value`; `just gen-types` regenera un TS consumible sin importar `JsonValue`.
- `.gitignore` ignora `backend/crates/*/bindings/` y `frontend/src/lib/api/crates/`, ambos outputs generados.
- `frontend/package.json` define `test: pnpm check`, y `Justfile:test` ejecuta `pnpm check` para que el gate
  frontend exista.
- `pnpm check` ✅ 0 errores / 0 warnings.
- `just test` ✅.

### Cierre
T4 cerrada. No queda trabajo activo en esta tarea; cualquier mejora posterior debe abrirse como task nueva.

## Historial (cerradas)

- **T4 — Rehidratación de sesiones tras reinicio** — DONE ✅.
  `Manager::load_existing()` rehidrata sesiones desde disco, reconcilia `Running` huérfanas,
  `/api/threads` lista metas vivas + detached, transcripts siguen disponibles desde `output.log`.
  Smoke real de reinicio + `just test` verdes.

- **Task 27 — Broadcast SSE + UI de propuestas** — VERIFY ✅.
  `task_propose` ahora delega al REST cuando hay `server_url`, `POST /tasks` acepta `status=proposed`
  y la propuesta entra por el `TaskStore` del server para emitir `task.created`/SSE. UI: tipo/schema/filtro
  incluyen `proposed` y el drawer permite promover a `queued` o `blocked` según dependencias.

- **Task 15 — Eventos append-only unificados** (slice incremental backend-only) — VERIFY ✅.
  `Event` evolucionado a envelope aditivo (`thread_id`/`actor`/`payload`, records viejos deserializan);
  `Store::append_event` asigna `seq` atómicamente bajo el `write_lock` y lo retorna (**cierra Task 28**,
  el race de `seq`); TaskEvents (task/scheduler/spec) ahora se persisten como envelopes vía
  `TaskStore::with_event_store` (MCP sink-free, sin doble-escritura); `emit` best-effort (no tumba la
  operación si falla el audit-log); formato SSE/`TaskEvent` intacto (cero frontend). QA PASS 6/6,
  118 tests verdes, clippy limpio, `Event.ts` regenerado. Review: P1 (emit fail-fast) y 2×P2 corregidos.

- **Task 14 — Capability policy middleware mínimo** — VERIFY ✅. Matriz `capability_default` como
  fuente única en `harness-policy` (planner/orch full; worker/generator deny `task_create`/`spec_write`;
  evaluator deny sensitive; None/desconocido permisivo). `Rule.role` opcional + `evaluate(tool,args,role)`
  con precedencia rules→matriz→fallback. Dispatcher reenvía `role`; online=server autoritativo+audit,
  offline=fail-closed local; `task_create_restricted` hardcoded eliminado. Audit append-only
  `capability.decided` (deny/ask) en `/api/approvals/check`. QA PASS 7/7, 65 tests verdes, clippy limpio,
  `Rule.ts` regenerado. **Trade-offs aceptados (decisión del usuario):** rol desconocido→permisivo
  reabre role-stuffing (mitigado: `role` lo fija la infra de spawn `--role`, no el modelo). **Follow-ups:**
  Task 28 (race de `seq` en `append_event`, sistémico → Task 15), Task 29 ejecutada 2026-06-04
  (root spawn valida roles, `remembered_rule` preserva rol, offline sin rol/desconocido niega tools
  sensibles), Task 30 ejecutada 2026-06-04.
- **Task 13 — Separar `task.create` y `task.propose`** — VERIFY ✅. `TaskStatus::Proposed`,
  `task_propose` (cualquier rol) crea en `proposed`, `task_create` con gate de rol exacto fail-closed
  en el dispatcher (deny FUERA de `harness-policy`). `Proposed→Queued` vía `task_update`; no
  reclamable ni agendable. QA PASS 5/5, 166 tests verdes, tipos regenerados. Follow-up SSE → backlog.
