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
| **Tarea** | _(ninguna)_ |
| **Estado** | `IDLE` |
| **Objetivo** | — |
| **Alcance / archivos** | — |
| **Responsables** | — |
| **Criterio de aceptación** | — |
| **Checks obligatorios** | `just test` + endpoint afectado + `just gen-types` si tocó tipos |

### Contrato API + tipos
_(endpoints, método, payload, response, errores y tipos `ts-rs` afectados — Backend es dueño)_

### Handoffs
_(cada ejecutor: "listo para consumo" con endpoints, tipos, archivos tocados, comandos corridos)_

### Preguntas al Planner
_(dudas de ejecutores; el Planner responde, no asumen)_

---

## Historial (cerradas)

_(el Planner mueve aquí las tareas con VERIFY verde, una línea por tarea)_
