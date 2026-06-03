# BOARD — Equipo de desarrollo de HarnessDevTool

Canal común entre Planner (Claude), Backend Rust (Codex), Frontend (Cursor) y los evaluadores
(Sonnet). Plantilla **estricta por campos**, no prosa libre. Ver `CLAUDE.md` §4.

> **Límite conocido:** una sola tarea "En curso" a la vez, sin locking real. El Planner es el único
> que abre/cierra; los ejecutores anotan en su bloque de Handoff. Revisor/QA reportan por la Agent
> tool (no escriben aquí).

---

## En curso

| Campo | Valor |
|---|---|
| **Tarea** | Task 13 — Separar `task.create` y `task.propose` |
| **Estado** | `VERIFY` ✅ — QA PASS (5/5), review aplicado, tests verdes |
| **Objetivo** | Los workers **proponen** tareas; el planner/orchestrator las **crea**. Check mínimo de rol; el middleware completo de capacidades queda para Task 14. |
| **Alcance / archivos** | `backend/crates/harness-mcp-server/src/tools/tasks.rs`, `dispatcher.rs`, `harness-core/src/tasks/{model,state_machine,store}.rs`; tipos `ts-rs` → `frontend/src/lib/api/types/`; render de estado en frontend |
| **Responsables** | Planner (audit + verify), Backend Rust/Codex (exec), Frontend/Cursor (render `proposed`), Sonnet (review+QA) |
| **Criterio de aceptación** | (1) existe `task_propose` que cualquier agente puede llamar y crea task en estado propuesto; (2) `task_create` rechaza a workers con hint a `task_propose`; (3) el planner puede promover propuesta→cola; (4) deny NO vive en `harness-policy`; (5) `just test` verde + tipos regenerados |
| **Checks obligatorios** | `just test` + endpoint afectado + `just gen-types` (TaskStatus cambia) |

### Audit de `harness-policy` (requisito previo de la tarea) — HECHO

- **`PolicyEngine::evaluate(tool, args) -> Decision` es ciego al rol.** Solo casa por nombre de
  tool + globs de args (`engine.rs:86`). `task_create` ya está en `is_sensitive_tool` (`engine.rs:154`)
  → default `Ask`, pero **no puede distinguir worker de planner**.
- Meter contexto de rol en el engine = el **middleware de capacidades completo** = **Task 14**.
  **Conclusión: el deny de `task_create` a workers NO va en `harness-policy` ahora.**
- El único señalador de identidad disponible al crear task es `Dispatcher.agent_id`
  (`dispatcher.rs:36`, ej. `agent:planner`). No hay campo `role` estructurado todavía (eso es
  Task 16). El check mínimo de Task 13 debe hilar un `role` en el dispatcher (default permisivo
  para back-compat) o derivarlo del `agent_id`.
- `TaskStatus` (`model.rs:15`) no tiene estado de propuesta. La state machine
  (`state_machine.rs:27`) lista transiciones explícitas; promover propuesta→cola exige una arista
  nueva.

### Contrato API + tipos (CONFIRMADO — forma: nuevo estado `Proposed`; rol vía `Dispatcher`)

- **Tipo afectado:** `TaskStatus` (`#[derive(TS)]`) → regenerar con `just gen-types`; frontend
  renderiza el estado nuevo.
- **MCP `task_propose`** (nuevo): args = los mismos de `task_create` (`title`, `brief`, `labels`,
  `acceptance`, `depends_on`, `parent`). Crea task en estado propuesto. Permitido a cualquier rol.
- **MCP `task_create`** (modificado): gate mínimo de rol en el dispatcher. Si el caller no es
  planner/orchestrator → error estructurado `isError:true` con hint "usa task_propose".
- **Promoción:** planner mueve propuesta→`queued` vía `task_update` (nueva transición en la state
  machine). El scheduler ignora tasks propuestas hasta promoción.

### Handoffs
- **Backend Rust (Codex)** — listo para consumo. Tocó `harness-core` (model/state_machine/store),
  `harness-mcp-server` (dispatcher/main/tools/mod/tasks), `harness-server` (state.rs,
  routes/sessions.rs). `TaskStatus::Proposed`, `TaskStore::propose()`, `task_propose` tool, gate de
  rol en dispatcher, `--role` hilado de punta a punta. `cargo test` (166) verde, `just gen-types`
  corrido (`TaskStatus.ts` incluye `"proposed"`).
- **REVIEW (Sonnet)** — 0 P0. P2 (gate `.contains` evadible) + P1 (`task_propose` no sensitive)
  corregidos por el Planner. P1 (delegación SSE de propose) → follow-up.
- **QA (Sonnet)** — PASS 5/5 con evidencia (166 tests, clippy limpio, tipos sync, deny fuera de
  `harness-policy`).
- **Planner (par-revisión)** — match de rol exacto (fail-closed) + `task_propose` en
  `is_sensitive_tool` + tests; `cargo test -p harness-policy -p harness-mcp-server` verde.

### Follow-up (no bloquea Task 13)
- **`task_propose` no delega al REST de harness-server** → sin broadcast SSE; el panel no refresca
  en vivo cuando se llama con `--server-url` (el `task_list` sí ve la propuesta). Arreglo propio:
  endpoint REST que acepte `status=proposed` + UI de propuestas/promoción (solapa con Task 11 y
  trabajo de frontend). Registrar como tarea nueva.

### Preguntas al Planner
_(ninguna)_

---

## Historial (cerradas)

_(el Planner mueve aquí las tareas con VERIFY verde, una línea por tarea)_
