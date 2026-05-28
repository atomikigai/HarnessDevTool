---
id: agents/role-capability-matrix
title: Matriz de capabilities por rol (Q9)
shard: 13-agents
tags: [roles, capabilities, policy, mcp, security, audit]
summary: Source-of-truth de qué rol puede llamar qué tool del harness-bridge, con qué scope y bajo qué ownership.
related: [agents/capability-registry, agents/orchestrator, harness-core/mcp-integration, build-plan/phase-3-team, build-plan/open-questions]
sources: []
---

# Matriz de capabilities por rol

> Cierre de **Q9**. Esta matriz es la fuente única que el dispatcher del `harness-mcp-server` consulta para autorizar cada llamada. Drift entre código y este shard = build break.

## Principio

La autorización **no** es `role → puede llamar tool`. Es:

```
role + tool + resource + scope + ownership + thread_id + path_policy
```

Un check del bridge evalúa los seis ejes antes de admitir la llamada. Esto da: aislamiento real, validación antes-de-daño, auditoría útil.

## Decisiones core

### task.create — planner/orchestrator only
Workers no crean tasks. Para "self-spinoff" usan `task.propose` (ver abajo). El planner decide si lo convierte en `task.create`. Mantiene loop limpio + auditoría explícita ("worker descubrió subtrabajo" vs "planner expandió scope").

### task.propose — todos los roles
Nueva tool. Shape:

```jsonc
task.propose {
  parent_task_id,
  discovered_by_role,
  rationale,
  suggested_title,
  suggested_acceptance_criteria
}
```

No crea task; encola una propuesta para el planner.

### task.claim / task.release — workers + QA, con constraints
Reglas del bridge:

```
allow task.claim iff:
  caller.role in task.allowed_roles
  AND task.status == "open"
  AND (task.assigned_to is null
       OR task.assigned_to == caller.role
       OR task.assigned_to == caller.agent_id)
  AND task.claimed_by is null

allow task.release iff:
  task.claimed_by == caller.agent_id
  OR caller.role == "orchestrator"
```

Crítico: `assigned_to: other_role` es **rechazo del bridge**, no convención del CLI. Planner nunca claim.

### spec.md — planner/orchestrator only
Workers, QA y learner escriben en `task.notes`, `task.artifacts`, `qa.results`, `learner.observations`. El planner consolida vía `spec.append_section`.

```
spec.append_section: planner (active thread)
spec.set_section:    orchestrator only, OR planner con explicit lock + version check
```

Versionado obligatorio para `set_section`:

```
{
  spec_version_required,
  spec_section_id,
  thread_id,
  actor_role,
  reason
}
```

Evita que un agente pise una sección vieja.

### memory.* — separar read/write y scope
Workers leen su thread; planner/learner pueden global. Workers globales = roto el aislamiento + ruido.

```
memory.search(scope="thread"):  todos
memory.search(scope="project"): planner, QA, learner
memory.search(scope="global"):  planner/orchestrator, learner
memory.write:                   planner/orchestrator
memory.write(proposed_learning): learner (namespace propio)
```

### skills.* — read abierto, execute por capability de la task
```
skills.read:    todos
skills.execute: si la task lo permite en sus capabilities
```

### repo.* — read abierto, write atado a task
```
repo.read:  todos
repo.write: solo si task.claimed_by == actor
            AND path matches task.write_paths
            AND path NOT in task.forbidden_paths
```

Shape de allowlists en la task:

```yaml
task:
  id: F3-frontend-012
  allowed_roles: [frontend-worker]
  write_paths:
    - apps/web/**
    - packages/ui/**
  forbidden_paths:
    - spec.md
    - harness.config.json
    - .claude/**
```

## Policy declarativa (source of truth)

Vive en `capability-policy.yaml` (o embebida en `capability-registry.md`). El dispatcher la carga al boot del MCP server y la consulta en cada llamada.

```yaml
roles:
  orchestrator:
    allow:
      - task.create
      - task.release:any
      - task.update:any
      - spec.append_section
      - spec.set_section
      - memory.search:global
      - memory.write:project
      - repo.read
      - policy.read
      - policy.write

  planner:
    allow:
      - task.create
      - task.update:planned
      - task.comment
      - spec.append_section
      - memory.search:project
      - memory.write:thread
      - repo.read
      - policy.read
    deny:
      - task.claim

  frontend-worker:
    allow:
      - task.claim:role_compatible
      - task.release:self
      - task.update:self
      - task.comment
      - task.propose
      - memory.search:thread
      - skills.read
      - repo.read
      - repo.write:task_paths
      - policy.read
    deny:
      - task.create
      - spec.set_section
      - spec.append_section

  backend-worker:
    allow:
      - task.claim:role_compatible
      - task.release:self
      - task.update:self
      - task.comment
      - task.propose
      - memory.search:thread
      - skills.read
      - repo.read
      - repo.write:task_paths
      - policy.read
    deny:
      - task.create
      - spec.set_section
      - spec.append_section

  qa:
    allow:
      - task.claim:qa_only
      - task.release:self
      - task.update:self
      - task.comment
      - task.propose
      - memory.search:project
      - repo.read
      - repo.write:test_artifacts
      - policy.read
    deny:
      - task.create
      - spec.set_section
      - spec.append_section

  learner:
    allow:
      - task.comment
      - task.propose
      - memory.search:global
      - memory.write:proposed_learning
      - repo.read
      - policy.read
    deny:
      - task.create
      - task.claim
      - spec.set_section
      - spec.append_section
      - repo.write
```

## Vista tabular (resumen rápido)

| Tool                          | orchestrator | planner    | fe-worker  | be-worker  | qa         | learner    |
|-------------------------------|--------------|------------|------------|------------|------------|------------|
| `task.create`                 | ✓            | ✓          | ✗          | ✗          | ✗          | ✗          |
| `task.propose`                | ✓            | ✓          | ✓          | ✓          | ✓          | ✓          |
| `task.claim`                  | ✗            | ✗          | ✓ role-cmp | ✓ role-cmp | ✓ qa-only  | ✗          |
| `task.release`                | ✓ any        | ✗          | ✓ self     | ✓ self     | ✓ self     | ✗          |
| `task.update`                 | ✓ any        | ✓ planned  | ✓ self     | ✓ self     | ✓ self     | ✗          |
| `task.comment`                | ✓            | ✓          | ✓          | ✓          | ✓          | ✓          |
| `spec.append_section`         | ✓            | ✓ active   | ✗          | ✗          | ✗          | ✗          |
| `spec.set_section`            | ✓            | ✓ +ver-chk | ✗          | ✗          | ✗          | ✗          |
| `memory.search:thread`        | ✓            | ✓          | ✓          | ✓          | ✓          | ✓          |
| `memory.search:project`       | ✓            | ✓          | ✗          | ✗          | ✓          | ✓          |
| `memory.search:global`        | ✓            | ✓          | ✗          | ✗          | ✗          | ✓          |
| `memory.write:thread`         | ✓            | ✓          | ✗          | ✗          | ✗          | ✗          |
| `memory.write:project`        | ✓            | ✗          | ✗          | ✗          | ✗          | ✗          |
| `memory.write:proposed`       | ✓            | ✓          | ✗          | ✗          | ✗          | ✓          |
| `skills.read`                 | ✓            | ✓          | ✓          | ✓          | ✓          | ✓          |
| `skills.execute`              | task-gated   | task-gated | task-gated | task-gated | task-gated | task-gated |
| `repo.read`                   | ✓            | ✓          | ✓          | ✓          | ✓          | ✓          |
| `repo.write`                  | ✓            | ✗          | ✓ paths    | ✓ paths    | ✓ tests    | ✗          |
| `policy.read`                 | ✓            | ✓          | ✓          | ✓          | ✓          | ✓          |
| `policy.write`                | ✓            | ✗          | ✗          | ✗          | ✗          | ✗          |
| `agents.spawn` (F3)           | ✓            | ✗          | ✗          | ✗          | ✗          | ✗          |

Sufijos:
- `:any` — sobre cualquier recurso
- `:self` — solo recursos donde `actor == owner`
- `:role_compatible` — `caller.role in task.allowed_roles`
- `:qa_only` — solo tasks con role=qa o phase=validation
- `:task_paths` — path debe matchear `task.write_paths` y no `task.forbidden_paths`
- `+ver-chk` — exige `spec_version_required` que matchee versión actual

## Invariantes (tests obligatorios del bridge)

1. Worker no puede `task.create`.
2. Worker puede `task.propose`.
3. Worker no puede `claim` task `assigned_to` otro rol.
4. Worker no puede `release` task `claimed_by` otro `agent_id`.
5. Planner no puede `task.claim`.
6. QA solo puede claim tasks con `role=qa` o `phase=validation`.
7. Solo planner/orchestrator puede `spec.append_section`.
8. Solo orchestrator (o planner con version check) puede `spec.set_section`.
9. Workers no pueden `memory.search:global`.
10. Learner puede `memory.search:global` pero no muta task/spec/repo.
11. `repo.write` exige task activa + claim propio + path permitido.
12. Toda denegación queda auditada con `reason` code.

## Audit log

Cada llamada del bridge (allow Y deny) escribe una entrada:

```json
{
  "timestamp": "...",
  "thread_id": "...",
  "actor_id": "...",
  "actor_role": "frontend-worker",
  "tool": "task.claim",
  "resource": "task:F3-123",
  "decision": "allow|deny",
  "reason": "role_mismatch|claimed_by_other|allowed",
  "input_hash": "...",
  "result_hash": "..."
}
```

Para denegaciones, el log es tan valioso como para mutaciones exitosas — es donde se detectan bugs de comportamiento del CLI hijo.

Sink: `$HARNESS_HOME/.runtime/audit/bridge.jsonl` (rotación zstd como en [[agents/spawn-lifecycle]]).

## Enforcement

- El dispatcher del `harness-mcp-server` carga `capability-policy.yaml` al boot.
- Cada handler de tool envuelve la lógica con `check_capability(caller, tool, resource, scope)` antes de ejecutar.
- Resultado del check (`Allow` / `Deny{reason}`) se loggea + se devuelve al CLI hijo como `permission_denied` cuando es Deny.
- Cambios en la policy requieren reinicio suave del MCP server (no afecta sesiones PTY).

## Extensión

Añadir un rol nuevo:
1. Entry en `roles:` de la policy YAML.
2. Fila en la tabla de este shard.
3. Si introduce una capability nueva, columnar en la tabla + handler en el dispatcher.
4. Tests de invariantes para el rol nuevo.

## Atadura con otros shards

- [[agents/orchestrator]]: usa esta matriz para saber qué tools listar al planner cuando spawneа workers.
- [[agents/spawn-lifecycle]]: el `spawn_id` y `agent_id` que entran al audit log salen de acá.
- [[build-plan/phase-3-team]]: F3 implementa el dispatcher con esta matriz desde el primer commit.
- [[harness-core/approval-flow]]: approval-and-remember opera **encima** de esta matriz — solo se pregunta al usuario si la matriz dijo `allow` pero la policy del usuario quiere confirmación extra.
