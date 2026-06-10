---
id: build-plan/harness-analysis-2026-06-10
title: Análisis del harness + revisión del fuente de Codex — bugs, perf, medición
shard: 12-build-plan
tags: [analysis, audit, codex, measurement, cost, bugs, performance, normalization]
summary: Auditoría del harness (bugs verificados, perf, hueco de seguridad de delegación) + revisión del fuente de openai/codex que desbloquea el reporter de costo de Codex, arregla el cuelgue headless y mapea la paridad Claude↔Codex. Incluye plan §7 re-costado para habilitar la medición codex vs sonnet y configs de Zeus. No se tocó código.
related: [build-plan/planning-codex-delegation-2026-06-10, build-plan/harness-analysis-2026-06-09, build-plan/improvement-plan, build-plan/pending-implementation-tasks, teamwork/SCOREBOARD]
sources: []
---

# Análisis del harness + revisión de Codex — 2026-06-10

> Sesión de análisis (sin tocar código). Encargo del usuario: analizar el harness (bugs, huecos, perf),
> y preparar la **medición de codex gpt-5.5 vs sonnet 4.6** en código orquestado por Opus 4.8 y de
> **configuraciones de modelos en modo Zeus**. Se clonó `openai/codex` en `../codex-upstream` para
> revisar el fuente. Cada hallazgo de Codex marcado ✅ fue verificado a mano en el fuente.

## 0. Hallazgo central

Las dos peticiones empíricas (medir codex vs sonnet; medir configs de Zeus) **hoy no se pueden ejecutar
con rigor**: el harness es el cuello de botella. Tres bloqueadores: (1) el reporter de costo de Codex es
un STUB que devuelve $0 (`harness-core/src/budget/reporter.rs:143-157`); (2) `codex exec` headless
cuelga; (3) no hay métrica de éxito/calidad/turnos/latencia por tarea. **La revisión del fuente de Codex
resolvió o abarató (1) y (2).** Ver §2 y §7.

## 1. Estado del harness (resumen)

- 8 crates Rust (~49k LOC). Alto riesgo: `harness-session` (PTY/lifecycle), `harness-policy` (gating),
  `harness-mcp-server` (MCP stdio + gateway).
- **Zeus hoy NO delega multi-CLI**: es una sesión PTY única de Codex con un system-prompt orquestador
  (`harness-session/src/kind.rs:67-72` mapea `Zeus → Codex`). La matriz rol→CLI/modelo
  (`ZeusRoleSelection { provider, model, effort }`) es configurable y se persiste; la delegación real
  por rol es F3 (no implementada). El fallback rol→CLI está documentado, no automatizado.
- Modelos por defecto (`harness-session/src/manager.rs:17-19`): Claude=`sonnet`/effort medium,
  Codex=`gpt-5.5`. Precedencia de override: matriz Zeus > params del request > default.

## 2. Revisión del fuente de Codex (✅ = verificado en `../codex-upstream/codex-rs`)

| Hallazgo | Veredicto | Impacto |
|---|---|---|
| **Formato de rollout + tokens** | ✅ `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-<ts>-<thread_id>.jsonl`, JSONL append-only. Tokens en eventos `token_count`: `TokenUsageInfo.total_token_usage` (acumulativo) con `TokenUsage { input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens }` (`protocol/src/protocol.rs:1938`). Modelo en `turn_context.model`. `RolloutItem` es enum etiquetado `{"type","payload"}` (`protocol.rs:2882`). | Desbloquea el reporter de costo de Codex: leer el último `token_count.total_token_usage` × pricing. Falta agregar pricing de `gpt-5.5`. |
| **Cuelgue headless** | ✅ Causa raíz: el prompt es arg posicional `Option<String>` (`exec/src/cli.rs:84`). Con prompt presente, modo `OptionalAppend`: si stdin **es** TTY → no lee; si stdin **no** es TTY (background, pipe sin EOF) → `read_to_end()` bloquea (`exec/src/lib.rs:1858-1868`). Por eso funciona vía PTY (slave = terminal) y cuelga headless. | Fix: pasar prompt posicional **y** `< /dev/null`. Receta: `codex exec "PROMPT" --json --skip-git-repo-check -c sandbox_mode=workspace-write < /dev/null`. |
| **`--json` event stream** | ✅ `codex exec --json` emite JSONL de eventos parseables; `--output-last-message FILE`, `--ephemeral` también. | Bonus: turnos, tool-calls y latencia de Codex se derivan del stream (no heurística de strings). |
| **Supresión de tools nativas (M13)** | ✅ Codex NO tiene equivalente granular a `--disallowed-tools`. Solo `features.shell_tool=false` (todo-o-nada) y `enabled_tools`/`disabled_tools` por MCP server (`config/src/mcp_types.rs`). | El control sobre Codex debe ser verificación posterior (M2/M3), no enrutamiento de tools. |
| **System-prompt / house-rules** | ✅ `developer_instructions` = paridad con `--append-system-prompt` (ya lo usa el harness). AGENTS.md auto-load jerárquico cwd→root, sin opt-out por flag (solo `project_doc_max_bytes=0`). | Paridad de system-prompt confirmada; M14 (house-rules único) sigue válido. |
| **Sandbox / approvals** | ✅ `sandbox_mode`: read-only / workspace-write / danger-full-access. `approval_policy`: untrusted / on-failure / on-request / never / granular. `--dangerously-bypass-approvals-and-sandbox` ≡ `approval_policy=never` + `sandbox_mode=danger-full-access`. | Insumo para M15 (modelo de contención único). |
| **MCP como cliente** | ✅ Codex es cliente MCP (stdio o http) vía `[mcp_servers.<name>]` en TOML o `-c`; soporta `enabled_tools`/`disabled_tools` y `approval_mode` por tool. No pasa `session-id`. | El diseño del gateway MCP del harness puede reusar este modelo de allow/deny + approval por tool. |

## 3. Bugs y correctitud

### Nuevos (verificados por el Planner)

| Sev | Bug | Ubicación |
|---|---|---|
| P1 | Gateway MCP sin timeout de lectura — `read_response` es `loop {}` sobre lectura bloqueante; un upstream que no responde o manda otro `id` bloquea el hilo del agente para siempre, y `child.kill()` (línea 160) nunca corre porque está después del read. | `harness-mcp-server/gateway.rs:174-190` |
| P1 | Timeout de policy-check de 120s — cada tool call gateada hace POST a `/api/approvals/check`; un approval server colgado congela al agente 2 min por llamada. Bajar a 5-10s. | `harness-mcp-server/dispatcher.rs:486` |
| P2 | PID fallback a 0 — `child.process_id().unwrap_or(0)`; con PID 0, `kill()` es no-op → zombie. Baja probabilidad. | `harness-session/session.rs:200` |

> Descartado (falso positivo de un revisor): "Ask sin server = allow silencioso". Verificado: offline,
> `Decision::Ask` devuelve `Some(...)` → bloquea (fail-closed). Online lo auto-resuelve
> `/api/approvals/check` según `autonomy_profile`. No hay allow ciego (`dispatcher.rs:542`).

### Residuales abiertos del improvement-plan

| Sev | Issue | Ubicación |
|---|---|---|
| P1 | `ensure_thread` reconstruye el índice sosteniendo el mutex durante todo el I/O → serializa el arranque | `harness-core/tasks/store.rs:92-144` |
| P1 | Fuga de conexión en `drop_lease_async` (Arc::try_unwrap falla con queries en vuelo) | `module-db/lease.rs:217-226` |
| MEDIA | Approval pendiente se filtra al desconectar el cliente (sin guard RAII) | `harness-server/approvals.rs:90-96` |
| MEDIA | Sin fsync del directorio padre tras create/rename (debilita append-only ante crash) | `harness-core/store/mod.rs:233`, `tasks/store.rs:648` |
| MEDIA | Falta `DefaultBodyLimit` global + `TimeoutLayer` de request (residual S10) | `harness-server/app.rs` |
| MEDIA | Confirmar que `read_handoffs` tiene el mismo skip-and-warn que S7 (una línea truncada no debe romper el historial) | `harness-core/store/mod.rs:211-253` |

### Robustez de sistemas (confianza media — revisar de cerca, no delegar a ciegas)

- Kill tree por PID único, no por process group (`harness-session/session.rs:535-565`): SIGTERM solo al
  líder; nietos fuera del group pueden quedar zombies. Sugerido: `killpg` o `/proc/<pid>/task`.
- Poisoning de locks con `.expect()` en `harness-policy` (`engine.rs:88,100,109,165`): Wave 2/3 metió
  recovery con `into_inner()` en otros subsistemas; el policy engine sigue con `.expect()`.
- Estado inconsistente en rotación de `output.log` ante poison parcial (`harness-session/output.rs:58-62`).

## 4. Performance

Hecho (Wave 2/3): scheduler indexado en memoria, `read_output` streaming 256 KiB con `spawn_blocking`,
`seq` atómico, recovery de lock poisoning. Restante priorizado:

| Esfuerzo | Impacto | Oportunidad |
|---|---|---|
| M | ALTA | Frontend: dos `setInterval` sobre `sessionsState` en paralelo (`+page.svelte:162-181` + `IconRail.svelte:42-60`) → carrera de selección. Consolidar en store ref-counted. |
| S | MEDIA | `flush_chunk` clona el buffer entero (~16ms, hasta 32KB) → `mem::take()`/recycle (`harness-session/session.rs:665-682`). |
| S | MEDIA | Children-poll del frontend (1.5s) corre sin gatear a tab/rol (`SessionRightPanel.svelte:131`). |
| L | MEDIA | State detector (600ms) y transcript watcher (500ms) son polling → inotify/kqueue (post-dogfooding). |
| M | ALTA | No hay benchmark reproducible (Criterion) para detect/flush/read_tail. |

## 5. Seguridad de delegación a Codex

Confirmado: Codex se spawnea SIEMPRE con `--dangerously-bypass-approvals-and-sandbox` incondicionalmente
(`harness-session/manager.rs:655`), sin mirar `autonomy_profile`, y edita con sus propias tools (no por
`repo_write_file`). ⇒ el gating runtime no lo constriñe; `manual` no le da protección extra. Como Codex
tampoco puede enrutar selectivamente por las tools del harness (no hay `--disallowed-tools`, ver §2), el
único control efectivo es **contrato previo + verificación posterior**: M1 (briefing rico), M2
(scope-drift `git diff` vs `write_paths`/`forbidden_paths` — mayor ROI), M3 (compuerta de verificación
dura). Roadmap completo en [[build-plan/planning-codex-delegation-2026-06-10]] (M1–M17).

## 6. Medición codex vs sonnet + configs de Zeus

> **Corrección 2026-06-10 (aclaración del usuario):** usa Codex Pro (costo plano) → **no se mide USD**; el
> reporter de costo (1a/§7) queda **descartado**. Las menciones a "USD"/"reporter de costo" abajo quedan
> superseded: se mide **calidad + performance de codificación** (ver SCOREBOARD). Primer head-to-head
> (gateway-timeout) ya ejecutado y registrado: empate técnico, Sonnet con mejor cobertura de test, Codex más
> rápido codificando → se mergeó Sonnet + el assert de reaping de Codex.

**Existe:** cost tracking de Claude, `SessionMeta` + `loaded_capabilities`, transcript watcher, budget API,
endpoint Prometheus, scripts A/B (Task 31), `SCOREBOARD.md` manual (n=6, sesgado a Sonnet por el cuelgue
de Codex). **Falta:** reporter de costo de Codex (desbloqueado, §2), fix headless (desbloqueado, §2),
`SessionResult { success, quality_score, turns, wall_seconds }`, evaluador post-sesión (Sonnet-juez),
análisis estadístico (n por celda, CI, p-value, effect size).

**Diseño experimental recomendado:** matriz `{codex-gpt5.5, sonnet-4.6, opus-4.8, haiku-4.5} × {profile
none, harness} × {5 tipos de tarea: write simple, refactor, bug-fix, code-review, doc}`, ≥3-5 runs por
celda (~120-200 sesiones), Opus 4.8 fijo como orquestador, midiendo éxito · calidad rúbrica · turnos ·
wall-clock · USD. Zeus configs: no medibles end-to-end hasta F3 (delegación real). Recomendación a-priori
(a validar): orquestador=Opus 4.8; backend alto riesgo=Opus/Sonnet con par-revisión; backend
greenfield=Codex gpt-5.5 vía PTY + scope-drift; frontend=Sonnet 4.6; docs=Haiku 4.5; evaluator=Sonnet 4.6.

## 7. Plan re-costado

| # | Tarea | Esfuerzo | Nota |
|---|---|---|---|
| 1b | Fix invocación headless de Codex (prompt posicional + `< /dev/null` + `--json`) | XS | Reactiva Codex headless; da stream de eventos. |
| 5 | Corregir receta de Codex en CLAUDE.md §3 | XS | Doc-only. |
| 1a | Reporter de costo de Codex (parser de rollout JSONL) | S | Reemplaza el stub $0; agregar pricing gpt-5.5. |
| 1c | `SessionResult { success, quality_score, turns, wall_seconds }` | S/M | Derivable del `--json` de Codex y del transcript de Claude. |
| 1d | Evaluador post-sesión (Sonnet-juez con rúbrica) | M | Único bloque de fondo restante. |
| 2 | Seguridad de delegación: M1 + M2 + M3 | M | Ver §5 / planning-codex-delegation. |
| 3 | Bugs P1: gateway timeout, policy-check 120s→5-10s, drop_lease_async, ensure_thread mutex | S-M | §3. |
| 4 | Perf quick wins: polling frontend, `mem::take()` en flush_chunk | S | §4. |

Secuencia: 1b → 5 → 1a → 1c → 1d → 2 → bugs P1 → perf. Con 1a+1c+1d se puede correr la matriz de §6.

> Pendiente de decisión del usuario (de [[build-plan/planning-codex-delegation-2026-06-10]] §6): (1)
> ¿Codex siempre sin sandbox o por autonomy_profile?; (2) ¿worktrees por worker ya o scope-drift por
> ahora?; (3) ¿planner runtime auto-arranca o sigue manual?
