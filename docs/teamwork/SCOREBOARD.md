# SCOREBOARD — Rendimiento de ejecutores (Codex CLI vs subagentes Claude)

**Propósito:** tabla comparativa de rendimiento y calidad de entrega según ejecutor (Codex CLI vs subagentes Sonnet 4.6 / Haiku), medida por datos objetivos de cada tarea cerrada y puntuación subjetiva del usuario al cierre. La métrica rige decisiones sobre quién ejecuta cada tipo de trabajo: Codex para backend de sistemas, subagentes para lograr velocidad y evitar cuelgues de `codex exec` headless.

**Quién llena qué:**
- **Métricas objetivas** (P0/P1/P2, rondas de rework, duración, cuelgues): las anota el Planner al cerrar cada tarea en el board.
- **Puntuación usuario (1–5):** solo la llena el **usuario final** cuando el Planner se la pide en el handoff de cierre. Dejar en blanco hasta entonces.

---

## Tabla de desempeño

| Fecha | Tarea | Ejecutor | Dominio | P0/P1/P2 revisión | Rondas rework | Duración aprox | Cuelgues/incidentes | Puntuación usuario (1–5) | Notas |
|---|---|---|---|---|---|---|---|---|---|
| 2026-06-10 | Gateway MCP read timeout (**head-to-head**) | Codex gpt-5.5 (headless) | backend (mcp-server, alto riesgo) | 0/0/0 | 0 | ~4m22s coding (+~5m QA build aparte) | 0 | — | Hilo lector→canal + `recv_timeout`, deadline compartido entre los 2 reads; +1 test (silencioso + **assert de reaping `kill -0`**); 67/67 + fmt verde. Solo `gateway.rs`. Headless OK con prompt posicional + `< /dev/null`. |
| 2026-06-10 | Gateway MCP read timeout (**head-to-head**) | sonnet-4.6 (subagente) | backend (mcp-server, alto riesgo) | 0/0/0 | 0 | ~12m11s (incl. su build+test en frío) | 0 | — | Un solo hilo para toda la conversación (más simple) + mejores docs; +2 tests (silencioso + **id-que-nunca-matchea = el vector real de DoS**); 68/68 + fmt verde. Solo `gateway.rs`. (El Planner diagnosticó mal "muerto" a media corrida — error de diagnóstico del Planner, no del ejecutor.) |
| 2026-06-10 | ChatView live round 3 — Backend transcript | sonnet-4.6 (subagente nativo) | backend | 0 P0 / 0 P1 / 5 P2 | 1 | ~15 min | 0 | — | `routes/transcript.rs` reescrito ~360 líneas; subscribe-antes-de-replay, slot tardío, watcher checkpoint. 5 P2 corregibles (línea parcial, I/O síncrono, PID reciclado). Fix round en curso. |
| 2026-06-10 | ChatView live round 3 — Frontend ChatView | sonnet-4.6 (subagente nativo) | frontend | 1 P1 / 2 P2 | 1 (en curso) | ~26 min | 0 | — | 5 bugs arreglados (SSE reconnect, auto-scroll, fallback PTY, restart continuidad, tokens). `pnpm check` verde. No corrió repro agent-browser (pendiente QA). |
| 2026-06-10 | ChatView live round 3 — Backend zeus_roles | sonnet-4.6 (subagente nativo) | backend | 0 hallazgos aún | 0 | ~6 min | 0 | — | Slice menor de rehidratación de watchers / Zeus profile. Espera revisión dedicada del Planner. |
| — (ref. histórica) | codex exec headless — múltiples intentos | Codex CLI | backend/cualq. | — | — | — | cuelgue stdin exit 144 | N/A | Feedback 2026-06: `codex exec` sin tty en context background ingresa en deadlock por stdin bloqueante. 0 tareas completadas headless via CLI desde entonces. Subagentes nativos usados como fallback. |
| 2026-06-09 | Production grade Wave 3 | Codex (5 slices: C1→C5) | backend + CI | 0 P0 / 2 P1 | 1 (fix round) | ~6h | 0 | — | CI, scheduler, output streaming, sandbox Linux, metrics. `just test` 366 pass. QA PASS 7 criterios. |
| 2026-06-09 | Harness improvement Wave 2 | Codex (A+B) + sonnet-4.6 (frontend) | backend + frontend | 0 P0 / 2 P1 (backend) | 1 | ~4h | 0 | — | Lock poisoning recovery, smart capability loader v2, data loader confinamiento. QA PASS 6 criterios. |

---

## Cómo puntuar (criterio usuario, 1–5)

Escala de calidad entregada **según la experiencia del usuario** al usar la tarea:

- **5 = listo a la primera:** sin retoques visibles, cumple 100% criterio aceptación, flujo limpio, cero bugs posteriores detectados.
- **4 = bien con detalles menores:** cumple criterio, pero hay detalles UX o edge cases pequeños que podrían pulirse (impacto bajo, no bloqueante).
- **3 = cumple con rework notable:** tuvo que reiterarse o dejó P2/hallazgos menores documentados, pero funciona; usuario puede trabajar con ello sin fricción.
- **2 = rework mayor o criterio a medias:** requirió varias rondas o falta un aspecto importante del criterio; funciona parcialmente.
- **1 = inutilizable:** no cumple criterio o introduce regresiones graves; no es deliverable tal como está.

**Instrucciones al usuario (cuando Planner cierra tarea):**
Puntúa con un número 1–5 en la columna correspondiente. Justifica brevemente en **Notas** si quieres contexto (ej. "buena pero compilación lenta" en 4; "buena arquitectura pero crash raro en edge case" en 3).

---

## Resumen técnico por tipo de trabajo (draft, por completar)

| Tipo de trabajo | Mejor ejecutor (por ahora) | Evidencia | Notas |
|---|---|---|---|
| Backend Rust de sistemas (session, policy, MCP) | Empate técnico (Codex ≈ Sonnet 4.6) — pendiente nota 1-5 + más muestras | Wave 3 por Codex (366 tests). **Head-to-head gateway-timeout 2026-06-10**: ambos 0/0/0 y QA verde; Sonnet testeó mejor el vector real (wrong-id) y fue más limpio, Codex ~3× más rápido en coding y aserta el reaping. Headless de Codex **arreglado** (`< /dev/null`). | Codex = rápido + dominio Rust; Sonnet 4.6 = limpio + buena cobertura de test. Decidir por más head-to-heads y nota del usuario. |
| Frontend SvelteKit | sonnet-4.6 + manual tests | ChatView rounds estables con 5 bugs por sesión, fix rápido en paralelo. | Subagente mejor para iteración rápida UX. |
| Docs / organización | doc-agent (Haiku 4.5) | tareas administrativas (board, backlog, changelogs) cerradas sin deuda. | Rápido y económico; no inventa hechos. |

---

## Notas operativas

1. **Codex headless — causa raíz + fix (2026-06-10):** revisado el fuente de `openai/codex`, el cuelgue es `read_to_end()` de stdin no-TTY sin EOF cuando el prompt va por arg posicional (`exec/src/lib.rs:1858-1868`). **Fix:** `codex exec \"PROMPT\" --json --skip-git-repo-check -c sandbox_mode=workspace-write < /dev/null`. Codex headless es recuperable; la postura \"Sonnet como fallback por cuelgue\" deja de ser forzosa una vez aplicado `< /dev/null`. Detalle: [[build-plan/harness-analysis-2026-06-10]].
2. **Subagentes nativos en paralelo:** Frontend y Backend pueden correr a la vez con write-scopes disjuntos, sin necesidad de serializar; mejora velocidad al mitigar cuelgues de CLI externos.
3. **Duración aprox:** incluye lectura de brief, ejecución y handoff en el board; **no** incluye revisión/QA oficial (eso entra como tarea separada post-handoff).
4. **Puntuación usuario:** decisión de producto, no técnica. Espera a que el usuario valore en vivo antes de ajustar roster/ejecución.
5. **Qué medimos — NO costo (aclaración usuario 2026-06-10):** el usuario usa el CLI de Codex con **suscripción Pro ($200/mes)** → costo plano; **no interesa medir USD/tokens**. Se mide **performance de codificación + calidad**: P0/P1/P2 del revisor, rondas de rework, PASS/FAIL de QA, tiempo de pared, cuelgues + nota 1-5 del usuario. El reporter de costo en USD queda **descartado**; el rollout de Codex (`$CODEX_HOME/sessions/**/rollout-*.jsonl`) se usa para señales de performance (turnos/tool-calls/duración), no dólares.
