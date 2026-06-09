# Análisis integral del harness — 2026-06-09

Evaluación hecha por el Planner con 5 agentes de exploración en paralelo (backend Rust,
frontend/Tauri, docs/visión, tooling del equipo, y carga de capabilities + rendimiento).
Responde: ¿está listo para producción? ¿para sesiones largas? ¿para los objetivos? ¿para
tareas pequeñas? ¿es rápido? ¿es un equipo de desarrollo especializado? ¿faltan
tools/MCPs/skills? ¿la carga de capabilities es inteligente?

---

## Veredicto rápido

| Pregunta | Veredicto |
|---|---|
| ¿Listo para producción? | 🟡 **Casi** — listo para uso local single-user (su objetivo v1); no para multi-usuario ni exposición a red |
| ¿Listo para sesiones largas? | 🟡 **Parcial** — context governor funciona, pero no es crash-safe ni hay rehidratación de PTY vivo |
| ¿Listo para lograr los objetivos? | ✅ **En rumbo** — F0–F3 done, F4 parcial, F5 en curso; la visión y la implementación están alineadas |
| ¿Atiende tareas pequeñas? | 🟡 **Funciona pero con overhead** — el ciclo completo planner→generator→evaluator es pesado para un one-liner |
| ¿Es rápido? | 🟡 **Aceptable, con hot paths conocidos** — P1 de rendimiento sigue abierto (scheduler O(n), polling, copias en PTY) |
| ¿Es un equipo de desarrollo especializado? | ✅ **Sí, en diseño y en práctica** — roles claros, rails de Rust, comunicación por archivos |
| ¿Faltan tools/MCPs/skills? | ❌ **No faltan más; falta gobernarlos** — el cuello de botella es el Gateway MCP y la precisión de carga, no la cantidad |
| ¿La carga de capabilities es inteligente? | 🟡 **Inteligente a medias** — carga selectiva al spawn (bien), pero por keywords literales, sin descarga ni re-resolución mid-session |

---

## 1. ¿Listo para producción?

**Para su definición de producción (self-hosted, single-user, local): casi.** Los 10 P0 de
seguridad se cerraron el 2026-06-03 (path traversal, SQLi, auth token, política default-ask,
crash-safety de ids, etc.). Docker compose, perfiles aislados y append-only están operativos.

**Lo que lo separa de un "sí" pleno:**

1. **Lock poisoning sistémico (P1 residual).** ~24 `expect("...poisoned")` sobre mutexes en
   `context_governor.rs` (11), `tasks/store.rs` (10) y `session/manager.rs` (3). Un panic de
   una task tokio sosteniendo el lock tumba en cascada el governor o el task store completo.
   Migrar a `parking_lot` o a `unwrap_or_else(|e| e.into_inner())` (patrón ya usado en
   `store.rs:66`) es barato y de alto impacto. **Es la mejora #1 que haría.**
2. **Sandbox solo en macOS.** `harness-sandbox` usa `sandbox-exec`; en Linux (donde corre este
   repo) no hay enforcement — la policy es la única barrera. Considerar landlock/bubblewrap.
3. **GETs sin auth.** El token solo protege rutas mutantes; cualquier proceso local lee
   sesiones, transcripts y schema de BD. Aceptable single-user, peligroso en cuanto haya LAN.
4. **Sin métricas exportables.** Budget/presión de contexto viven in-process; no hay
   Prometheus/OTel. Para operar esto "en producción" hace falta ver colas, sesiones y memoria.
5. **Sin CI.** No hay `.github/workflows`; la puerta de calidad es manual (`just test` +
   reviewer/qa). Un CI mínimo (check + clippy + nextest + pnpm check) cerraría regresiones.

## 2. ¿Listo para sesiones largas de programación?

**Parcial.** Lo bueno: el context governor con umbrales 35% (checkpoint) / 40% (clear +
resume con continuidad) es exactamente el mecanismo correcto, y el modelo de memoria de 7
capas (events append-only, spec, tasks, skills, USER/PROFILE, CONTINUITY.md, FTS5) está
diseñado para que el agente sea efímero y la memoria viva en el harness.

**Lo que falla en sesiones de 8+ horas:**

- **El governor no se checkpointea a disco**: si el backend se reinicia a mitad de sesión, se
  pierde el estado de presión/checkpoint pendiente. Las sesiones detached se rehidratan como
  `Exited` con output legible, pero el PTY vivo se huérfana (procesos hijos sin reap).
- **`read_output` bufferiza hasta 50 MiB** para catch-up de SSE (`output.rs:62-70`) — en una
  sesión larga eso bloquea y consume memoria. Ya está en el backlog (T5): stremear.
- **Sin límite duro de memoria** por sesión ni monitoreo de crecimiento de DashMap/Arc.
- **Replay a escala no probado**: el E2E cubre 120 turnos; una sesión de un día son miles.

**Conclusión:** para sesiones largas *supervisadas* sí; para sesiones largas *desatendidas*
falta checkpoint del governor + streaming de output + métricas de memoria.

## 3. ¿Listo para lograr los objetivos?

**Sí, va en rumbo.** La visión (harness multi-surface, agentes efímeros planner/generator/
evaluator con rails de Rust, memoria auditable, autonomía proporcional) está bien documentada
y las fases avanzan: F0–F3 done, F4 (DB ✅ / SSH parcial), F5 en curso (smart skill loader
mergeado), F6 sin empezar. El gate de dogfooding histórico está cerrado.

**Los tres bloqueos reales hacia la visión completa:**

1. **Gateway MCP (P3)** — hoy los MCPs externos saltean el gate de policy/approvals. Sin el
   proxy agregador, la "autonomía proporcional" tiene un agujero: la policy gobierna tools
   nativas pero no todo lo que entra por MCP. Es el prerequisito de F5/F6 serio.
2. **Enforcement generator→evaluator (Task A3)** — el handoff obligatorio antes de
   `pending_verify` es lo que convierte el modelo de roles en regla y no en convención.
3. **Repo intelligence (Task A4)** — symbols/callers/blast-radius cacheados por HEAD es lo
   que hará a los generators realmente eficientes en repos grandes.

## 4. ¿Atiende tareas pequeñas?

**Funciona, pero el costo fijo es alto.** El diseño ya contempla `execution_mode: quick` en el
orchestrator, lo cual es la respuesta correcta. Pero hoy cada spawn paga: proceso MCP propio
por sesión, intro de capabilities, resolución de skills, y el ciclo plan→execute→review→qa.
Para "cambia este string" eso es desproporcionado.

**Recomendaciones:**
- Implementar de verdad el camino `quick`: una sola sesión generator sin evaluator separado,
  con verificación inline (test del archivo tocado) y sin spawnear MCPs que no use.
- Considerar un pool o reuso del proceso `harness-mcp-server` (es stateless por request — un
  proceso por sesión es puro overhead de arranque).
- UI: el botón `+ task` (Task 11) ayuda a que lo pequeño no requiera ceremonia de spec.

## 5. ¿Es rápido?

**Aceptable hoy, con deuda P1 identificada y bien diagnosticada.** Hot paths concretos:

| Hallazgo | Dónde | Impacto |
|---|---|---|
| Scheduler rescanea disco cada 2s, O(n) tareas | `harness-core/scheduler` | Alto con muchas tasks; mover a índice SQLite (ya en backlog T5) |
| Copia `to_vec()` por cada chunk PTY de 16 KB | `harness-session/src/session.rs:273` | Presión de allocator en el path más caliente; esfuerzo bajo |
| `read_output` carga hasta 50 MiB en memoria | `harness-session/src/output.rs:62-70` | Bloquea workers en catch-up; stremear |
| `seq = read_events().len()` racy | events append | Corrección + costo; asignar seq atómico |
| Polling HTTP en frontend: children 1.5s, context 3s, metrics 5s, health 10s, más polling duplicado de sesiones (`+page` + `IconRail`) | `SessionRightPanel.svelte:268-301`, `TopBar.svelte:16` | Latencia percibida y requests que se pisan; migrar a SSE (el canal ya existe) |
| Tauri: `to_vec()` + `drain()` por frame en el parser PTY | `src-tauri/src/lib.rs:189-190` | Menor; índice de lectura en vez de drain |

**Lo que ya está bien:** SSE con backoff exponencial y resync, batching por RAF en terminal y
chat, markdown nativo paralelo con rayon en Tauri, DashMap para sesiones vivas, sidecar con
Drop limpio. La base es sólida; es cuestión de cerrar la Fase C (P1 rendimiento) del plan.

## 6. ¿Es un equipo de desarrollo especializado?

**Sí — es de lo más maduro del proyecto.** Dos niveles coherentes que comparten vocabulario:

- **Equipo nativo (construye el harness):** Planner que no edita, Codex para Rust de sistemas,
  subagentes frontend/doc/reviewer/qa con prompts auto-contenidos, scopes de escritura
  estrictos, board con plantilla, hook de SessionStart con contexto barato. El ritmo lo
  demuestra: Tasks 18–23 + hardening Wave 1 cerrados en días, con handoffs documentados.
- **Equipo runtime (lo que el harness construye):** planner/generator/evaluator/arbitrator +
  learner/curator/psychologist, con reglas duras (nadie se auto-aprueba, re-plan cap K=2,
  comunicación por archivos).

**Fricciones honestas:** board sin locking (una tarea en curso a la vez), Codex stateless
(el Planner re-inyecta contexto en cada brief), sin CI que respalde el VERIFY humano, y los
crates de alto riesgo dependen de revisión cercana del Planner — correcto, pero no escala.

## 7. ¿Faltan tools, MCPs, skills?

**No. Sobra inventario y falta gobernanza.** Hay 20+ skills bundled (rust-tooling, nextest,
ast-grep, security-tooling, context7, crawl4ai, excalidraw, agent-browser…) que cubren bien
desarrollo, calidad, seguridad y docs. Agregar más hoy empeoraría el problema real, que es de
**selección y control**, no de cobertura:

1. **Gateway MCP primero** — sin gate de policy sobre MCPs externos, cada MCP nuevo es un
   bypass nuevo.
2. **Carga por sesión más precisa** (ver §8) antes que más skills.
3. Los únicos huecos genuinos que vale considerar después: `codebase-memory-mcp` detrás del
   gateway (ya planeado, Task A4) y un exportador de métricas (no es skill, es feature).

## 8. ¿La carga de skills/MCPs/tools es inteligente? ¿Los agentes quedan ligeros?

**A medias — la arquitectura es la correcta, la heurística y el ciclo de vida no llegan aún.**

**Lo que está bien:**
- Carga **selectiva al spawn**: `resolve_smart_skills()` / `resolve_smart_tool_groups()`
  (`routes/sessions.rs:1248-1465`) eligen capabilities según role, cwd, scopes y prompts, en
  vez de cargar todo siempre.
- El intro de capabilities es liviano: **~1–2k tokens** inyectados en `auto_intro`. Los
  agentes arrancan ligeros de verdad.
- Cleanup al morir la sesión: `cleanup_session_resources()` (`state.rs:224-245`) elimina
  configs MCP y aborta watchers.
- El context governor (35%/40%, checkpoint→clear→resume con continuidad) mantiene el
  *contexto conversacional* ligero a lo largo de la sesión — eso sí funciona hoy.

**Lo que no llega:**
1. **La heurística es `contains()` de keywords literales** sobre un haystack lowercase. "csv"
   mencionado de pasada en un prompt de auditoría carga `data_loader`; "frontend" en un path
   carga skills de UI en una task de backend. Sin stemming, sin pesos, sin feedback. Es la
   versión F5-temprana esperable, pero hay que decirlo: es matching, no inteligencia.
2. **No hay descarga ni re-resolución mid-session.** `LoadedCapabilities` queda **fijo** al
   spawn para toda la vida de la sesión. No existe unload de un MCP que ya no se usa, ni
   lazy-load de uno que resulta necesario a mitad de tarea (el agente queda sin la tool o el
   Planner mata y re-spawnea). El objetivo "cargan lo necesario y lo liberan cuando ya no lo
   necesitan" está cumplido solo en la primera mitad.
3. **Un proceso MCP por sesión, sin compartir.** Cada spawn levanta su harness-mcp-server (y
   crawl4ai vía `npx mcp-remote` si aplica). Para agentes efímeros que viven minutos, el
   costo de arranque por sesión pesa; siendo stateless por request, es candidato natural a
   proceso compartido o pool.

**Cómo lo evolucionaría (en orden):**
1. **Re-resolución en los checkpoints del governor**: el checkpoint a 35% ya pausa y resume la
   sesión — es el punto natural para recalcular capabilities (cargar lo que la fase siguiente
   necesita, soltar lo que no se ha usado). Da el "load/unload dinámico" sin inventar otro
   mecanismo.
2. **Telemetría de uso de tools** (el audit log de bridge ya registra cada tool call): con eso
   se mide qué skills/tool-groups cargados *nunca se usan* por tipo de task, y la heurística
   pasa de keywords a datos. Es además el insumo del learner/curator de F5/F6.
3. **Scoring en vez de contains**: pesos por señal (role > scopes > cwd > prompt), umbral por
   capability, y FTS5 (que ya existe en el stack) para matching menos literal.
4. **MCP server compartido/pooled** detrás del Gateway MCP — resuelve arranque y gobernanza a
   la vez.

## 9. Trabajo en curso (no commiteado)

Feature **data loader** (CSV/XLSX inspect/write): `data.rs` (646 líneas, tipos ts-rs +
inspección/escritura con límites), `routes/data.rs` (2 endpoints con `spawn_blocking`), e
integración con el smart loader (`resolve_smart_tool_groups`, intro condicional). `cargo
check` verde y un test unitario del intro. **Pendiente para cerrar:** `just gen-types`
(tipos `#[derive(TS)]` nuevos), ciclo Revisor→QA con curl a `/api/data/inspect|write`, y
registro del handoff en el board. Nota: keyword "csv" como trigger es el ejemplo vivo del
falso positivo de §8.

## 10. Plan recomendado (orden de ataque)

1. ✅ **(Wave 2, 2026-06-09) Cerrar data loader** (gen-types + review/QA + board) — completado en Wave 2.
2. ✅ **(Wave 2, 2026-06-09) Lock poisoning** → `parking_lot` en governor/store/manager. Barato, elimina el peor modo de falla sistémico. — completado.
3. **Fase C / P1 rendimiento**: scheduler indexado, `read_output` streaming, seq atómico,
   SSE lagged/resync, y de paso la copia en `session.rs:273`.
4. **CI mínimo** (check + clippy + nextest + pnpm check + verificación de gen-types limpio).
5. **Gateway MCP (P3)** — prerequisito de todo lo demás de autonomía.
6. **Capabilities v2**: telemetría de uso → scoring (✅ scoring ponderado role/scopes/cwd/prompts e implementado en Wave 2) → re-resolución en checkpoints → MCP pooled (§8).
7. **Governor checkpoint a disco + métricas exportadas** — lo que falta para sesiones largas
   desatendidas y para llamarlo producción sin asterisco.
8. **Frontend**: consolidar polling→SSE, Vitest, y los stubs F3 (TaskGraph DAG, panel de
   agentes).

---

*Generado por el Planner (Fable 5) con 5 exploraciones en paralelo. Rutas y líneas citadas
verificadas a fecha 2026-06-09; los residuales P1/P2 referencian
`docs/12-build-plan/improvement-plan.md`.*
