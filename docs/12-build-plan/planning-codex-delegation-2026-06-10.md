---
id: build-plan/planning-codex-delegation-2026-06-10
title: Planificación y delegación a Codex — análisis de huecos y plan
shard: 12-build-plan
tags: [plan, planner, codex, orchestration, delegation, contracts, verification, sandbox]
summary: Auditoría del pipeline de planificación y delegación a Codex (runtime + equipo externo). Hallazgo central, huecos diseño-vs-código, bugs y roadmap priorizado para una sesión futura. No se tocó código.
related: [agents/orchestrator, agents/zeus-orchestrator, agents/autonomy-protocol, build-plan/improvement-plan, build-plan/pending-implementation-tasks]
sources: [agents/orchestrator, agents/autonomy-protocol, teamwork/OPERATING_MODEL]
---

# Planificación y delegación a Codex — análisis 2026-06-10

> Encargo del usuario: *"mejorar la planificación del harness — que sea lo bastante buena para que
> cualquier agente (principalmente Opus 4.8) planifique correctamente y delegue tareas para que se
> ejecuten bien con `codex --dangerously-bypass-approvals-and-sandbox`; admitiendo que Codex es más
> rápido implementando y mejor si está bien orquestado. Buscar huecos, posibles bugs y mejoras para
> planificar una sesión futura."*
>
> **Esta sesión NO tocó código.** Es un análisis para ejecutar después. Cada hallazgo lleva
> `archivo:línea`, severidad y esfuerzo (S/M/L).

## 0. Alcance: dos niveles que convergen

El encargo toca dos planos que el repo, a propósito, hace hablar el mismo idioma (CLAUDE.md, para
dogfooding):

1. **Runtime (el producto):** el planner/scheduler *dentro* del harness que descompone un goal,
   crea tasks y spawnea workers Codex/Claude como sesiones PTY.
2. **Equipo externo (CLAUDE.md):** cómo el Planner humano-facing (Claude Code nativo) delega al CLI
   `codex` para construir el propio harness.

Los hallazgos aplican a ambos planos salvo nota. La mejora de planificación que pide el usuario es,
en esencia, la misma en los dos: **producir contratos que Codex pueda ejecutar de forma
determinística y verificar el resultado, porque Codex es rápido pero no se auto-controla.**

> **Aclaración del usuario (2026-06-10) — el objetivo primario es NORMALIZACIÓN, no "optimizar para
> Codex":** *"La orquestación puede variar — uso modo Zeus y combino los agentes que quiera — pero
> todos trabajan bajo las mismas instrucciones, tools, MCPs, etc. Todo debe estar normalizado para
> cualquiera de los 2: Claude o Codex."* Es decir: el planner emite **un solo contrato CLI-agnóstico**
> y el harness debe entregarlo **idéntico** sea quien sea el ejecutor. La orquestación (quién hace qué,
> qué combinas en Zeus) es una capa libre **encima** del sustrato normalizado. Esto reordena el
> análisis: ver **§1·B (matriz de paridad)**. Los huecos de contrato/verificación/aislamiento de §1/§2
> son *cómo* se logra la paridad, dado que **ningún** CLI está gateado a nivel de su tooling nativo.

---

## 1. Hallazgo central (reorienta todo lo demás)

**Codex se spawnea SIEMPRE con `--dangerously-bypass-approvals-and-sandbox`** y **edita archivos con
sus propias herramientas (apply_patch/shell), NO con el MCP `repo_write_file` del harness.**

- `backend/crates/harness-session/src/manager.rs:655` — el flag se añade **incondicionalmente** para
  `AgentKind::Codex`, sin depender del `autonomy_profile`. Comentario in-code: *"Codex harness
  workers run behind the harness' own policy, budget and audit rails. Avoid per-call Codex approval
  prompts."*
- El gate de policy/approvals (`harness-mcp-server/src/dispatcher.rs` → `/api/approvals/check`) y el
  path-gating de `write_paths`/`forbidden_paths` solo se aplican a **tools MCP** (p.ej.
  `repo_write_file`). Codex no necesita esas tools para programar: usa su propio editor.

**Consecuencia (el núcleo del problema):** con Codex rápido y sin sandbox, **la arquitectura de
seguridad runtime del harness no constriñe el trabajo real de Codex.** Los únicos controles efectivos
pasan a ser:

| Control | Estado actual | Veredicto |
|---|---|---|
| (a) **Contrato/brief previo** (qué construir, dónde escribir, qué no tocar, cómo se acepta) | El brief del planner es **una frase** (ver §2 G1); `TaskBrief` existe pero el contrato tipado no | **débil** |
| (b) **Verificación posterior** (evaluator + acceptance + `git diff` + tests) | Handoff no obligatorio, acceptance por flag booleano, **sin diff de scope** | **advisoria, no dura** |
| (c) **Aislamiento / contención de archivos** | Solo "el planner declara write_paths disjuntos" (que además no se enforce) | **inexistente** |

⇒ **Mejorar la planificación = mover el peso a (a) y (c), y convertir (b) en compuerta dura.** El
resto del documento se ordena por ese principio.

> **Sub-hallazgo de seguridad (inconsistencia real):** el `autonomy_profile=manual` está diseñado
> para *"preguntar antes de mutaciones y comandos riesgosos"* ([[agents/autonomy-protocol]] §3), pero
> Codex se spawnea con bypass total **sin mirar el profile** (`manager.rs:650-711`). El
> `autonomy_profile` solo influye en la auto-resolución de approvals (`routes/approvals.rs:155-169`),
> que Codex de todos modos no consulta para editar. **Resultado: `manual` no da ninguna protección
> extra a una sesión Codex.** Hay que decidir explícitamente (ver §6).

---

## 1·B. Normalización Claude↔Codex (la matriz de paridad — objetivo primario)

El planner debe emitir **un solo contrato CLI-agnóstico**; el harness garantiza paridad a lo largo de
estos ejes. Lo que ya está parejo conviene **preservarlo**; lo que diverge es el trabajo a planificar.

| Eje | ¿Normalizado hoy? | Evidencia | Acción |
|---|---|---|---|
| **A. Contenido del briefing** (rol, contrato, project_context, capability_intro) | ✅ **Sí** | `auto_intro`/`role_prompt` se arman CLI-agnósticos en `state.rs:628-660` y `routes/sessions.rs:516-681`; el mismo `SpawnOpts` alimenta a ambos | Preservar. Mejorar el contenido (M1), no el mecanismo |
| **B. Mecanismo de entrega del briefing** | ⚠️ **Difiere (aceptable)** | Claude: `--append-system-prompt` + paste PTY (`manager.rs:728-753`). Codex: `-c developer_instructions` + arg posicional (`manager.rs:694-710`) | Es legítimo que difiera por CLI; solo verificar que el contenido entregado sea idéntico (test de paridad) |
| **C. Superficie MCP / policy / capabilities** | ✅ **Sí en destino** / ⚠️ **rota por spawn-path** | Ambos llegan al mismo `harness` MCP server → mismas tools, mismo gate, misma carga inteligente. Pero el scheduler omite `--session-id`/`--profile` que sí pasa REST (B1) | Unificar las 2 configs MCP (M6) para que la superficie sea idéntica por cualquier ruta |
| **D. Nudge de routing de tools nativas** | ❌ **No** | Claude recibe `--disallowed-tools TodoWrite` para forzar `task_*` del harness (`manager.rs:737-740`); **Codex no tiene equivalente** → puede usar su plan/todo nativo en vez de las tools del harness | **M13:** mapa de "supresión de tools nativas" por CLI para que ambos enruten por el harness |
| **E. House rules / contexto de proyecto** | ⚠️ **Parcial** | El harness inyecta el mismo `project_context_brief` a ambos (`routes/sessions.rs:951-968`), pero Claude auto-carga `CLAUDE.md` y Codex auto-carga `AGENTS.md` (`threads.rs:392`). Si difieren, las reglas de casa divergen | **M14:** fuente única de house-rules inyectada a ambos (o garantizar CLAUDE.md≡AGENTS.md) |
| **F. Modelo de contención / permisos** | ❌ **No** | Claude `--permission-mode bypassPermissions`; Codex `--dangerously-bypass-approvals-and-sandbox` (niveles de contención distintos) | **M15:** decidir un modelo de contención único (ver §6 decisión 1) |
| **G. Edición de archivos nativa** | ❌ **Ninguno gateado** (pero parejo entre sí) | Ambos editan directo, no por `repo_write_file` | La paridad aquí es por verificación posterior: el scope-drift (M2) aplica igual a ambos |

**Lectura:** la base ya es bastante pareja (A, C-destino, G-simetría). Lo que **rompe la
normalización** y hay que planificar es: **D** (Codex no enruta por las tools del harness como Claude),
**E** (reglas de casa por archivo distinto), **F** (contención asimétrica) y **C** (la superficie MCP
no es idéntica por spawn-path). Estos se materializan como **M13–M15** más **M6** (ya existente).

---

## 1·C. Harness agnóstico al agente (principio rector)

> **Aclaración del usuario (2026-06-10):** *"Todo el harness debe ser agnóstico al agente, pero debe
> trabajar lo suficientemente bien con cualquier agente."*

Dos exigencias acopladas:

1. **Núcleo agnóstico:** planner, scheduler, policy, tasks, eventos, budget — **nunca** deben ramificar
   por CLI. Toda especificidad de agente vive detrás de **un adaptador por CLI** con un *descriptor de
   capacidades* (¿soporta MCP? ¿cómo?; ¿system-prompt silencioso?; flag de modo autónomo; supresión de
   tools nativas; pin de `--session-id`; dir de auth). Añadir un agente = implementar un adaptador; el
   núcleo no se toca.
2. **Trabajar bien con cualquiera:** cada adaptador debe cubrir un set de capacidades *suficiente*, y
   donde una capacidad falte, el harness **degrada con gracia** de forma explícita (p.ej. sin
   system-prompt silencioso → entregar por PTY; sin MCP injection → ese CLI no puede ser worker de
   tasks gateadas y se marca como degradado, no se usa a ciegas).

**Estado actual (no cumple aún):**
- La lógica per-CLI es un **`match AgentKind` disperso**, no un adaptador: `build_extra_args`
  (`harness-session/src/manager.rs:624-775`) más ramas en `routes/sessions.rs` y `state.rs` (las 2
  configs MCP divergentes, B1). El núcleo conoce los CLIs concretos.
- Por la matriz de [[agents/supported-clis]]: **solo Claude y Codex están completos.** Cursor y
  Antigravity **no tienen** MCP injection, ni system-prompt silencioso, ni `--disallowed-tools`, ni pin
  de `--session-id` (`manager.rs:712-719,759-765` son TODOs). ⇒ El harness **no** trabaja "lo
  suficientemente bien con cualquier agente" todavía; trabaja bien con 2 de 4.
- No hay contrato explícito de **degradación**: hoy un CLI sin MCP simplemente no recibe tools y nada
  lo declara como worker degradado.

Esto se ataca con **M16** (abstracción `AgentAdapter` + descriptor de capacidades; el núcleo deja de
ramificar) y **M17** (completar/clasificar los adaptadores Cursor/Antigravity y definir el contrato de
degradación). M6/M13/M15 pasan a ser *capacidades del adaptador*, no ramas sueltas. La matriz de
[[agents/supported-clis]] es la fuente de verdad del descriptor y debería quedar **derivada del
código**, no mantenida a mano.

---

## 2. Huecos de planificación (diseño documentado vs código real)

| # | Hueco | Diseño (docs) | Código real | Sev | Esf |
|---|---|---|---|---|---|
| **G1** | No hay loop de planner real ni briefing rico | [[agents/orchestrator]]: readiness → execution_mode → spec → DAG → contratos → spawn_hint → `submit_plan` | El rol `planner` es **una frase**: *"You are the planner. Read spec.md and create tasks via task.* MCP tools."* (`harness-core/src/roles/mod.rs:127`). El flujo rico es **doc-only**; no se inyecta en el spawn | **ALTA** | M |
| **G2** | El `Task` no tiene contrato tipado | "cada task lleva `contract_declared` con outputs tipados; Rust diffea declared vs real; arbitrator resuelve drift" | `Task` (`harness-core/src/tasks/model.rs:324-377`) tiene `brief`, `acceptance`, `write_paths`, `forbidden_paths`, pero **no** `contract_declared` / `contract_real` / `spawn_hint`. El arbitrator y el drift son doc-only | **ALTA** | L |
| **G3** | El spawn del scheduler descarta el scope | task lleva scope; el spawn lo inyecta | `SpawnRequest` (`harness-core/src/scheduler/spawner.rs:22-45`) **no** transporta `write_paths`/`forbidden_paths`/`scopes`. Además los MCP args del scheduler (`harness-server/src/state.rs:667-754`) omiten `--session-id` y `--profile` que sí pasa la ruta REST (`routes/sessions.rs:1053-1068`) | **ALTA** | M |
| **G4** | No hay routing a Codex | Zeus: Codex primero para impl/tests/refactor; A2b routing por fortaleza de CLI | Los roles baseline son **todos `cli=Claude`**, incluido `generator` (`roles/mod.rs:121-148`). No existe módulo `scheduler/routing`; solo el campo `Role.cli`. **Por defecto el harness spawnearía generators Claude, no Codex** — contradice el "Codex más rápido implementando" | **ALTA** | M |
| **G5** | Re-plan sin cap K=2 | "Cap K=2: a la tercera, párala y consulta al humano" | Solo hay cooldown de 60s al mismo generator (`scheduler/tick.rs:40,613-634`). **No hay contador de intentos**; otro generator reintenta de inmediato. Una task que Codex no logra puede ciclar y quemar budget | **MEDIA** | S |
| **G6** | Handoff `generator→evaluator` no obligatorio | "obligatorio antes de `pending_verify`" | Se rutea automáticamente al evaluator (`tick.rs:787-850`) y se persiste el handoff, pero **no se exige ni se valida el contenido**. Acceptance se verifica por flag booleano, sin chequear el deliverable | **ALTA** | M |
| **G7** | Sin diff de scope (declared vs real) | implícito en "Rust diffea declared vs real" | **No existe** ningún check que compare `write_paths`/`forbidden_paths` contra `git diff --name-only` real. `reconcile.rs:10-47` es estructural (parent/child/artifacts), no de scope. **Codex puede tocar archivos prohibidos y nada lo marca** | **ALTA** | M |
| **G8** | Sin aislamiento entre workers Codex paralelos | "dos tasks no tocan el mismo archivo; si no, `blocked_by`" | La no-colisión depende de que el planner declare `write_paths` disjuntos (que no se enforce, G3). **No hay git-worktree por worker.** Dos Codex en paralelo sobre el mismo repo, o un Codex desbocado, pueden corromper el árbol compartido | **MEDIA** | L |

---

## 3. Bugs / inconsistencias concretas

- **B1 — Dos generadores de config MCP divergentes (scheduler vs REST).** `state.rs:667-754`
  (scheduler) omite `--session-id` y `--profile`; `routes/sessions.rs:978-1147` (REST) sí los pasa.
  Impacto con Codex-directo: un worker Codex spawneado por el scheduler no puede usar bien
  `session_spawn_child` (sin session-id) y carga policy del profile equivocado (sin `--profile`).
  Ya estaba listado como P2 en [[build-plan/improvement-plan]], pero con la delegación a Codex sube de
  prioridad. **MEDIA / M.**
- **B2 — Acoplamiento sandbox ↔ policy gate (latente).** El MCP llama `/api/approvals/check` por HTTP
  loopback (`dispatcher.rs:474-518`). Hoy es inocuo porque Codex corre sin sandbox. **Pero si alguien
  reactiva el sandbox de Codex (p.ej. para `manual`), el policy-check fallaría en silencio**
  (`dispatcher.rs:511` solo `warn!` y devuelve "approval check failed"). Debe documentarse el
  invariante: *gating MCP de Codex exige sandbox off **o** localhost permitido.* Es exactamente el bug
  histórico de `bug_codex_mcp_sandbox` en memoria. **MEDIA / S (documentar) + condicional.**
- **B3 — `codex exec` headless cuelga (la receta de CLAUDE.md §3 está rota).** CLAUDE.md §3 prescribe
  `codex exec -s workspace-write "..."` vía Bash para delegar backend, pero `codex exec` en background
  cuelga en stdin (exit 144) — feedback en memoria (`feedback_codex_exec_broken`). **La ruta de
  delegación documentada del equipo externo no funciona como está escrita**, y el usuario ahora pide
  `codex --dangerously-bypass-approvals-and-sandbox`. CLAUDE.md §3 quedó stale/contradictoria con la
  realidad operativa. **ALTA (para el plano externo) / S.**
- **B4 — El contrato se entrega una sola vez al spawn.** Para Codex, `role_prompt` va como arg
  posicional y `auto_intro` como `-c developer_instructions=...` (`manager.rs:694-710`). **No hay
  mecanismo de top-up de contexto a mitad de task ni de re-inyección del contrato al re-planear.** En
  tasks largas el contrato puede salirse del contexto efectivo del modelo. **MEDIA / M.**

---

## 4. Mejoras propuestas — roadmap priorizado

Ordenado por el principio de §1: primero volver duras las compuertas que sí constriñen a Codex.

### P0·N — Normalización del sustrato (objetivo primario del usuario, §1·B)

> Cierran los ejes que rompen la paridad Claude↔Codex. M6 (abajo, en P1) también sirve a este fin
> (eje C). M1 y M2 son CLI-agnósticos por diseño y refuerzan la normalización del contrato y la
> verificación.

- **M13 — Paridad de supresión de tools nativas (eje D).** Mapa por CLI de tools nativas a desactivar
  para que ambos enruten el trabajo por las tools del harness (`task_*`, etc.). Hoy solo Claude recibe
  `--disallowed-tools TodoWrite`; definir el equivalente para Codex (o documentar por qué no aplica) y
  cubrirlo con un test de paridad. **ALTA / S.**
- **M14 — Fuente única de house-rules (eje E).** Garantizar que Claude (CLAUDE.md) y Codex (AGENTS.md)
  reciban las mismas reglas de casa: o inyectar un house-rules normalizado vía `auto_intro` a ambos, o
  mantener CLAUDE.md≡AGENTS.md por generación/symlink y verificarlo en VERIFY. **MEDIA / S.**
- **M15 — Modelo de contención único (eje F).** Decidir y aplicar un nivel de contención equivalente
  para ambos CLIs (ver §6 decisión 1); hoy `bypassPermissions` (Claude) y
  `bypass-approvals-and-sandbox` (Codex) no son simétricos. **MEDIA / M.**
- **M16 — Abstracción `AgentAdapter` + descriptor de capacidades (§1·C, núcleo agnóstico).** Reemplazar
  el `match AgentKind` disperso (`manager.rs:624-775` + ramas en `sessions.rs`/`state.rs`) por un
  adaptador por CLI que declare sus capacidades (MCP, system-prompt silencioso, modo autónomo,
  supresión de tools nativas, pin de session-id, auth dir). El núcleo (planner/scheduler/policy) deja
  de conocer CLIs concretos. M6/M13/M15 se implementan como capacidades del adaptador. La matriz de
  [[agents/supported-clis]] se deriva del descriptor. **ALTA / L.**
- **M17 — Completar/clasificar adaptadores + contrato de degradación (§1·C, "trabajar bien con
  cualquiera").** Completar Cursor/Antigravity (MCP injection, entrega de briefing) o marcarlos
  formalmente como degradados; definir qué pasa cuando falta una capacidad (sin MCP → no worker de
  tasks gateadas; sin system-prompt silencioso → entrega por PTY visible) para que ningún CLI se use a ciegas.
  **MEDIA / M-L.**

### P0 — Volver load-bearing el contrato previo y la verificación posterior

- **M1 — Briefing de planner rico, inyectado en el spawn.** Portar [[agents/orchestrator]] a un
  builder `planner_briefing()` (análogo al `zeus_orchestrator_briefing()` existente) y al
  `prompt_template`/`auto_intro` del rol planner: readiness → execution_mode → reglas de contrato
  (outputs tipados, ≤6 acceptance, write_paths disjuntos, spawn_hint por task) → `submit_plan`.
  *Sin esto, "cualquier Opus 4.8 planifica bien" es falso: hoy recibe una frase.* **ALTA / M.**
- **M2 — Verificador de scope-drift (mayor ROI de seguridad para Codex sin sandbox).** Tras el submit
  de una task: `git diff --name-only` contra `write_paths`/`forbidden_paths` declarados; si hay
  violación → `needs_human` + razón estructurada + evento append-only. Es el control que falta para
  que el sandbox-off de Codex no sea un agujero. **ALTA / M.**
- **M3 — Compuerta de verificación dura.** Exigir handoff `generator→evaluator` antes de
  `pending_verify` (cerrar el follow-up de Task A3) y que acceptance corra evidencia real
  (`just test` o comando focal) y la adjunte como artifact, no solo flip de booleano. **ALTA / M.**
- **M4 — Cap de re-plan K=2.** Contador de intentos en el `Task`; al 3º, `paused`/`needs_human` con
  razón. Evita que un Codex que no converge cicle quemando budget. **MEDIA / S.**

### P1 — Calidad de routing y delegación

- **M5 — Routing a Codex.** Baseline `generator.cli = codex` para implementación backend; permitir
  que el planner fije `cli`/`spawn_hint` por task; clasificar por `domain`/`touches`/labels
  (`ui|css|backend|tests`) en un módulo `scheduler/routing` (cerrar A2b de [[build-plan/improvement-plan]]).
  *Es lo que materializa el "Codex más rápido implementando".* **ALTA / M.**
- **M6 — Transportar scope por `SpawnRequest` + unificar config MCP.** Añadir
  `write_paths`/`forbidden_paths`/`scopes` a `SpawnRequest`; extraer un único generador de config MCP
  compartido por scheduler y REST (cierra B1, y la duplicación P2 del audit previo); arreglar
  `--session-id`/`--profile` faltantes en el scheduler. **ALTA / M.**
- **M7 — Contrato tipado en el `Task`.** Añadir `contract_declared` (outputs tipados) y `spawn_hint`
  al modelo; el planner los declara y M2/M3 los diffean. Opcional: arbitrator-lite (auto-elevar drift
  trivial; si no, `needs_human`). **MEDIA / L.**

### P2 — Aislamiento, robustez y ergonomía

- **M8 — Aislamiento por git-worktree por worker Codex paralelo.** Cada task de implementación
  paralela corre en su worktree; merge al pasar verificación. Hace innecesario confiar en write_paths
  disjuntos para evitar corrupción y contiene a un Codex desbocado. **MEDIA / L.**
- **M9 — Top-up de contexto / re-inyección de contrato al re-plan.** Mecanismo para reenviar el
  contrato comprimido a Codex a mitad de task larga y en cada re-plan (cierra B4). **MEDIA / M.**
- **M10 — `plan.lint` (preflight de plan antes de ejecutar).** Rail que valide el plan: write_paths
  disjuntos entre tasks paralelas, cada task con ≤6 acceptance + `spawn_hint` + `brief` + `test_plan`.
  Materializa los anti-patrones de [[agents/orchestrator]] como check determinístico. **MEDIA / M.**

### P3 — Alineación del equipo externo (CLAUDE.md)

- **M11 — Arreglar la receta de Codex en CLAUDE.md §3.** Reemplazar el `codex exec` headless roto
  (B3) por la invocación verificada (`codex --dangerously-bypass-approvals-and-sandbox` no
  interactivo, o documentar que el backend hoy se delega vía subagente nativo hasta arreglar
  `codex exec`). Cruzar con `feedback_codex_exec_broken`. **ALTA (plano externo) / S.**
- **M12 — Plantilla de BRIEF reutilizable para delegar a Codex.** Análogo externo de `TaskBrief`:
  objetivo, write-scope, forbidden, criterio de aceptación, comando de test, regla "no salgas del
  scope", formato de reporte. *Es la palanca #1 para "Codex ejecuta bien cuando está bien
  orquestado"*: un brief estándar reduce la varianza de un ejecutor rápido y sin sandbox. **ALTA / S.**

---

## 5. Quick wins vs estructural

- **Quick wins (S, alto impacto):** M1 (briefing), M13 (supresión de tools nativas Codex), M14
  (house-rules único), M4 (cap K=2), M11 + M12 (receta + plantilla Codex), documentar B2, arreglar
  `--session-id`/`--profile` del scheduler (parte de M6).
- **Estructural (M/L):** M16 (`AgentAdapter`), M17 (adaptadores Cursor/Antigravity + degradación),
  M15 (contención único), M2 (scope-drift), M3 (compuerta dura), M5 (routing), M6 (unificar config),
  M7 (contrato tipado), M8 (worktrees), M9, M10.

Secuencia sugerida para la próxima sesión (normalización + agnosticismo primero, por las aclaraciones
del usuario): **M13 → M14 → M6 → M1 → M12 → M2 → M5 → M3 → M4 → M15**, luego **M16 → M17** (refactor a
adaptadores, que absorbe M6/M13/M15 como capacidades), luego M7, luego M8/M9/M10. M13/M14/M6 normalizan
el sustrato (tools, house-rules, superficie MCP) para que cualquier CLI sea intercambiable; M16/M17
vuelven el núcleo agnóstico al agente y hacen que el harness trabaje bien con *cualquiera* (no solo
Claude/Codex); M1+M12 dan el contrato CLI-agnóstico; M2+M3 cierran el hueco de seguridad de un
ejecutor sin sandbox; M5 activa la velocidad de Codex.

> **Nota de orden:** si se prefiere atacar la causa estructural primero, M16 (`AgentAdapter`) puede ir
> al frente: M6/M13/M15 caen como capacidades del adaptador en vez de parches sueltos. El trade-off es
> L de refactor vs varios S/M incrementales. Decisión del usuario en la próxima sesión.

---

## 6. Decisiones para el usuario (producto, no deducibles del código)

1. **¿Codex siempre sin sandbox, o por `autonomy_profile`?** Hoy es siempre sin sandbox
   (`manager.rs:655`), así que `manual` no protege más que `autonomous` para Codex (§1 sub-hallazgo).
   Opciones: (a) dejarlo así y declararlo explícito (la seguridad vive en M2/M3, no en el sandbox);
   (b) hacer que `manual` reactive el sandbox de Codex — pero entonces hay que resolver B2 (el MCP
   gate por HTTP) para que las tools no fallen en silencio. **Recomendación: (a)** — apostar a
   contrato+verificación (M1/M2/M3), que es donde el usuario quiere fuerza, y documentar el trade-off.
2. **¿Worktrees por worker (M8) ya, o confiar en write_paths disjuntos + M2 por ahora?** M8 es L;
   M2 (scope-drift) da el 80% del beneficio de contención al 30% del costo. **Recomendación:** M2
   primero, M8 cuando haya ≥2 Codex en paralelo de rutina.
3. **¿El planner runtime debe auto-arrancar (loop real) o seguir manual vía `task_create`?** Hoy es
   manual (no hay orquestador que tome un goal y produzca spec+DAG). M1 mejora el briefing aunque siga
   manual; un loop automático es un salto mayor (atado a F3/Zeus). **Recomendación:** M1 ahora; loop
   automático como hito aparte.

---

## 7. Apéndice — referencias de código verificadas en esta auditoría

- Flag de Codex incondicional: `harness-session/src/manager.rs:650-711` (`build_extra_args`).
- Inyección de brief Codex (`developer_instructions` + arg posicional): `manager.rs:694-710`.
- Inyección Claude (`--append-system-prompt`, `--permission-mode bypassPermissions`): `manager.rs:728-753`.
- Roles baseline (todos Claude): `harness-core/src/roles/mod.rs:121-159`.
- `SpawnRequest` sin scope: `harness-core/src/scheduler/spawner.rs:22-45`.
- Config MCP scheduler (sin `--session-id`/`--profile`): `harness-server/src/state.rs:667-754`.
- Config MCP REST (completa): `harness-server/src/routes/sessions.rs:978-1147`.
- Cooldown 60s sin cap de re-plan: `harness-core/src/scheduler/tick.rs:40,613-634`.
- Ruteo a evaluator (sin enforcement de handoff): `tick.rs:787-850`.
- Reconcile estructural (no de scope): `harness-core/src/tasks/reconcile.rs:10-47`.
- Modelo `Task`/`TaskBrief` (sin contract_declared/spawn_hint): `harness-core/src/tasks/model.rs:324-377`.
- Policy check HTTP del MCP: `harness-mcp-server/src/dispatcher.rs:474-518`.
- Autonomy en el gate de approvals (no en flags de spawn): `harness-server/src/routes/approvals.rs:155-169`.
</content>
</invoke>
