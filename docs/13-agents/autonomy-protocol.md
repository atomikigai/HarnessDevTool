---
id: agents/autonomy-protocol
title: Protocolo de autonomia del equipo
shard: 13-agents
tags: [agents, autonomy, readiness, planning, handoff, preflight]
summary: Contratos para que el harness decida cuanto planificar, detecte bloqueos antes de gastar tokens y ejecute con autonomia controlada.
related: [agents/overview, agents/orchestrator, agents/spawn-lifecycle, harness-core/approval-flow, cross-cutting/config-files]
sources: []
---

# Protocolo de autonomia

Objetivo: el harness debe poder resolver trabajos cortos sin ceremonia y trabajos
largos con equipo, sin quedarse bloqueado por permisos, credenciales o contexto
faltante que pudo detectar al inicio.

Este protocolo corre antes de crear el plan final de un thread y vuelve a correr
cuando una task cambia de scope o falla por entorno.

## 1. Readiness check

Antes de spawnear planner/generator, Rust genera un `readiness_report` append-only
para el thread.

Checks minimos:

| Check | Que valida | Resultado |
|---|---|---|
| `repo` | cwd existe, git status, rama, cambios sin commit, AGENTS.md/ARCHITECTURE.md | facts + warnings |
| `commands` | binarios requeridos: `git`, `just`, `cargo`, `pnpm`, `docker`, CLIs de agentes | pass/warn/block |
| `cli_auth` | token store de `claude`, `codex`, `cursor-agent`, `agy` cuando se van a usar | pass/block + install/login hint |
| `env` | `.env.example`, `.env`, vars declaradas en config y project.toml | missing_required/missing_optional |
| `deps` | lockfiles, node_modules/target si aplica, comandos de install disponibles | pass/warn |
| `ports` | puertos declarados por el proyecto o defaults del harness | conflict/warn |
| `budget` | cap, saldo estimado, concurrency cap | pass/warn/block |
| `secrets` | nunca lee valores; solo presencia/nombre/fuente | pass/block |
| `external` | DB/SSH/API declaradas como necesarias para la task | pass/block |

Resultado:

```jsonc
{
  "status": "ready" | "ready_with_warnings" | "blocked",
  "blocking": [
    {
      "id": "missing_env.DATABASE_URL",
      "kind": "env",
      "message": "DATABASE_URL es requerida para correr tests de DB",
      "how_to_fix": "Definirla en <project>/.env o marcar la task como mock-db"
    }
  ],
  "warnings": [],
  "facts": {
    "package_manager": "pnpm",
    "test_command": "just test",
    "available_clis": ["claude", "codex"]
  }
}
```

Reglas:
- Si `status=blocked`, el harness no arranca trabajo caro. Muestra el bloqueo y
  crea una task `blocked` con razon estructurada.
- Si `ready_with_warnings`, el planner puede seguir y debe registrar supuestos.
- Un agente nunca debe descubrir una credencial faltante tarde si el readiness
  check podia detectarla temprano.
- El check no imprime secretos ni los manda al modelo.

## 2. Execution mode

El planner no siempre construye un DAG completo. Primero clasifica el request:

| Modo | Uso | Flujo |
|---|---|---|
| `quick` | Cambio pequeno, bug puntual, doc, comando unico | 1 generator + verificacion local ligera |
| `standard` | Feature mediana o refactor acotado | planner compacto + 1-2 generators + QA |
| `project` | App/feature grande, varios dominios, dependencias | spec.md + DAG + scheduler + evaluator |
| `exploratory` | Diagnostico, investigacion, no se sabe el cambio | agente lee y reporta; no escribe salvo approval |
| `blocked` | Falta credencial, decision o recurso externo | no spawnea workers; pide solo lo necesario |

Heuristica inicial:
- `quick`: toca pocos archivos, bajo riesgo, acceptance <= 3, no credenciales.
- `standard`: requiere plan, pero no paralelismo amplio.
- `project`: requiere varias tasks, varios dominios o budget/concurrency.
- `exploratory`: el usuario pregunta "analiza", "investiga", "revisa" o el repo
  no esta claro.
- `blocked`: readiness report tiene `blocking`.

El modo queda persistido en `thread.meta.toml` y en el primer evento
`thread.execution_mode.selected`.

## 3. Autonomy profile

El perfil de autonomia define cuanto puede hacer el harness sin interrumpir al
usuario.

| Perfil | Uso | Comportamiento |
|---|---|---|
| `manual` | Debug, repos sensibles | pregunta antes de mutaciones y comandos riesgosos |
| `assisted` | Default humano | asume decisiones reversibles; pregunta bloqueos reales |
| `autonomous` | Trabajo largo confiable | ejecuta, instala deps permitidas, corre tests, reintenta dentro de budget |
| `ci` | Batch/headless | no pregunta; falla con reporte si falta algo |

Defaults:
- Proyecto nuevo: `assisted`.
- Thread marcado como batch: `ci`.
- User opt-in: `autonomous`.

Interaccion con approvals:
- `manual` fuerza `approval_mode=every-call` para tools sensibles.
- `assisted` usa `risky-only`.
- `autonomous` usa `auto` para tools permitidas por policy y `risky-only` para
  acciones fuera del allowlist del proyecto.
- `ci` usa `auto`, pero cualquier bloqueo produce error estructurado en vez de
  esperar al humano.

El harness nunca convierte `assisted -> autonomous` solo. El humano lo declara
por profile, project.toml o thread.

## 4. Team handoff

Los agentes se comunican por artefactos estructurados, no por chat libre. Cada
handoff entre roles debe incluir:

```yaml
from: agent:frontend-1
to_role: qa
task_id: T-0042
status: ready_for_verification
goal: "Verificar paginacion de pedidos"
assumptions:
  - "La API mantiene page_size default 20"
files_changed:
  - "src/orders.rs"
commands_run:
  - "cargo test orders_pagination"
verification_status:
  passed:
    - "cargo test orders_pagination"
  not_run:
    - "e2e browser: no fixture de auth"
blocked_on: []
next_agent_action: "QA debe agregar caso de ultima pagina y verificar contrato"
```

Reglas:
- `generator -> evaluator`: obligatorio antes de `pending_verify`.
- `evaluator -> generator`: obligatorio en `verify-fail`, con feedback accionable.
- `planner -> generator`: la task TOML es el handoff primario; no duplicar en prosa.
- `worker -> planner`: usar `task.propose` para scope nuevo, no crear tasks.
- Cada handoff se guarda como evento append-only y se resume en `task.notes`.

## 5. Preguntas al humano

El harness pregunta solo cuando la respuesta cambia de forma material el plan o
desbloquea recursos.

Reglas:
- Maximo 3 preguntas por ronda.
- Preguntas especificas, no "que prefieres hacer?".
- Si existe una opcion razonable y reversible, asumirla y registrarla.
- Si falta una credencial, pedir el nombre/fuente necesaria, no el valor secreto
  dentro del chat.
- Si el usuario no responde y el perfil es `ci`, fallar con `blocked`.

## 6. Calidad

Cada modo mantiene verificacion proporcional:

| Modo | Verificacion minima |
|---|---|
| `quick` | comando focal o typecheck/lint si existe |
| `standard` | tests focales + diff review del evaluator cuando aplica |
| `project` | evaluator separado + acceptance checks + budget report |
| `exploratory` | reporte con evidencias y limites |
| `blocked` | no verifica cambios porque no debe escribir |

No existe `done` sin evidencia. En modo `solo`, usar `done_unverified` con badge
visible si se permite completar sin evaluator.

## 7. Implementacion incremental

Orden recomendado:
1. Persistir `thread.execution_mode` y `autonomy_profile`.
2. Implementar readiness check basico: repo, commands, cli_auth, env, budget.
3. Hacer que orchestrator lea `readiness_report` antes de planificar.
4. Agregar handoff schema y eventos `handoff.created`.
5. Ajustar approvals segun `autonomy_profile`.
6. Extender UI con banner de readiness y selector de autonomia por thread.
