---
id: agents/zeus-orchestrator
title: Zeus — orquestador multi-CLI
shard: 13-agents
tags: [zeus, orchestrator, claude, codex, cursor, gemini, antigravity, fallback]
summary: Zeus es una sesión virtual que delega cada rol al CLI más adecuado, con fallback uniforme a Claude.
related: [agents/supported-clis, agents/role-capability-matrix, build-plan/phase-3-team]
sources: []
---

# Zeus — orquestador

> Zeus **no es un CLI**. Es una sesión virtual que el harness sintetiza: planifica el trabajo, designa un CLI por rol según la matriz de abajo, y delega. Sus métricas son la suma de las sesiones-hijas que lanza.

## Por qué Zeus

Cada CLI tiene fortalezas distintas. Forzar a uno solo (Claude) a hacer todo es ineficiente:
- Cursor es mejor para iteración visual / IDE-in-the-loop.
- Codex es muy fuerte para tests, PR y refactors mecánicos.
- Gemini conecta naturalmente con Google Cloud / Workspace.
- Claude lidera arquitectura, reasoning, validación.

Zeus elige por componente y deja a Claude como **fallback uniforme** cuando un CLI hijo se queda sin cuota o falla.

## Matriz rol → CLI

| Componente                  | Mejor opción            | Rol                                                                              |
| --------------------------- | ----------------------- | -------------------------------------------------------------------------------- |
| Orquestador central         | **Claude**              | Planifica, delega, valida, coordina handoffs.                                    |
| IDE / humano en loop        | **Cursor**              | Revisión, edición, reglas del proyecto, control manual.                          |
| Backend worker              | Claude o Codex          | Claude para arquitectura; Codex para implementación.                             |
| Frontend worker             | Cursor / Codex / Claude | Cursor para iteración visual; Codex para cambios rápidos; Claude para estructura.|
| DB worker                   | **Claude**              | Migraciones, impacto, consistencia, reasoning.                                   |
| QA worker                   | Codex o Claude          | Codex para tests; Claude para escenarios y criterios.                            |
| PR / refactor worker        | **Codex**               | Cambios de código, tests, reviews, releases.                                     |
| Cloud / context / search    | **Antigravity**         | Google Cloud, Workspace, contexto externo (binario `agy`).                       |

## Política de fallback

**Claude es el destino de fallback universal.** Si el CLI primario para un rol está sin cuota / errored / no instalado, Zeus reintenta con Claude. Esto se basa en que Claude tiene cuota más generosa en el setup actual del usuario y reasoning aceptable en todos los roles.

Orden de selección por rol (ejemplo, frontend worker):
1. Primario: `Cursor` (mejor en iteración visual).
2. Secundario: `Codex` (cambios rápidos).
3. Fallback: `Claude` (estructura — y siempre disponible).

El fallback dispara una entrada en el audit log con `reason: quota_exceeded | binary_missing | runtime_error`.

## Spawn semántico

Hoy Zeus corre como un **MVP funcional**: una sesión Claude PTY con un system-prompt especial (el "Zeus orchestrator briefing") inyectado vía `--append-system-prompt`. El briefing le explica al Claude:
- Que es el orquestador Zeus.
- La matriz rol → CLI completa.
- La política de fallback (todo cae a él).
- Que debe usar las tools MCP `session_spawn_child`, `session_list_children`,
  `session_send_input`, `session_cancel_child` y `session_read_child_summary`
  para delegar cuando el trabajo lo justifique.

Importante: la capacidad de iniciar subagentes no pertenece solo a Zeus. Una
sesión Claude/Codex/Cursor/Antigravity conectada al bridge puede iniciar hijas
si su rol y policy lo permiten. Zeus es el caso raíz/orquestador; los workers
también pueden subdividir trabajo puntual bajo su propio `parent_session_id`.

Flujo en el código:
1. Frontend envía `POST /api/threads/:tid/sessions { kind: "zeus" }`.
2. Backend resuelve `kind.underlying_cli()` → Claude.
3. Builda `SpawnOpts` con MCP injection (porque underlying = Claude) + `auto_intro = zeus_orchestrator_briefing()`.
4. Spawnea como `AgentKind::Claude` (para que `--session-id`/`--mcp-config`/etc. encajen) con `role = "zeus-orchestrator"` en el meta.
5. La UI muestra la sesión como "Zeus" usando ese `role`.
6. Cuando Zeus o cualquier agente autorizado llama `session_spawn_child`, el
   backend crea una sesión hija con `parent_session_id = <sid padre>`.

En F3 esto se endurece con:
1. Crear un meta "Zeus session" (`role = "orchestrator"`) sin PTY propio.
2. Scheduler toma el goal, genera un plan, designa tasks por rol, lanza sub-sesiones (CLIs hijos) cada una con `parent_session_id = <zeus-sid>`.
3. UI: tab principal Zeus + sub-tabs por hija.

## Restricciones

- Zeus solo orquesta CLIs **del set canónico** (ver [[agents/supported-clis]]). No hay agentes custom.
- Capabilities MCP por sub-sesión las define [[agents/role-capability-matrix]] según el rol asignado, no según el CLI elegido.
- Budget hard cap del thread aplica al conjunto Zeus + hijas — no por hija.
- Si Claude (el fallback) está también caído, Zeus marca la task `blocked` con `why_blocked = "no fallback CLI available"`.
- Los subagentes iniciados por workers son válidos, pero heredan el mismo árbol
  de budget/cancelación y deben quedar visibles en UI como descendientes.

## Cómo extender la matriz

1. Editar la tabla de este shard.
2. Codificar la regla en el selector del orchestrator (`harness-core::scheduler::routing`).
3. Test de aceptación: para un goal sintético, verificar que el rol se delega al CLI esperado y que el fallback dispara cuando el primario está bloqueado.

## Atadura con otros shards

- [[agents/supported-clis]]: la matriz de features del CLI subyacente sigue siendo la fuente de verdad.
- [[agents/role-capability-matrix]]: cada hija de Zeus opera bajo la matriz de capabilities del rol asignado.
- [[build-plan/phase-3-team]]: F3 implementa el routing real; antes de F3 Zeus devuelve 400.
