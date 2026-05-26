---
id: agents/arbitrator
title: Agent — Arbitrator (drift minor resolver)
shard: 13-agents
tags: [agent, arbitrator, drift, contract]
role: arbitrator
domain: none
cli: claude
summary: Llamada LLM ligera y específica que decide qué hacer con drift_minor en contratos.
related: [agents/overview, agents/qa, agents/rust-rails, foundations/lessons-learned]
sources: []
---

# Agent — Arbitrator

## Cuándo se invoca
Cuando `contracts.diff(declared, real)` devuelve `minor_extension` o `minor_omission`. El arbitrator decide:
- **Elevar contrato**: el generator añadió valor real; ampliar el `contract_declared` a incluir lo nuevo.
- **Forzar real**: el generator se extralimitó; revertir/ajustar para encajar en lo declarado.

**No** se invoca para `none` (no hace falta) ni `major` (re-plan directo al orchestrator).

## Naturaleza

Es un **call corto, barato, focused**:
- Modelo: `claude-haiku` o equivalente económico (configurable).
- Prompt: ≤ 2 KB.
- Output: decisión + 1-2 líneas de razonamiento.
- Latencia objetivo: < 5s.
- Costo objetivo: < $0.01 por call.

A diferencia de planner/generator/evaluator, el arbitrator **no es un thread largo**; es una llamada one-shot.

## Capabilities declaradas

### MCPs disponibles
| MCP | Cuándo cargarlo |
|---|---|
| `harness-bridge` | **siempre** |

### Skill tags
- ninguno por default (es focused; no necesita memoria procedimental).

### Tools permitidas
- `contracts.validate`, `contracts.diff` (lectura)
- `memory.search` (puede consultar decisiones previas sobre contratos similares)
- `tasks.get` (lectura de la task)
- `repo.read_file` (puede mirar el código real para juzgar)

**No** tiene `shell.exec`, `task.update`, ni nada que muta estado. Su output es **solo una recomendación**; el harness aplica la decisión.

## Input → Output

**Input** (construido por Rust al invocar):
```jsonc
{
  "task": { "id": "T-0042", "title": "...", "domain": "frontend" },
  "contract_declared": { "outputs": { "page_size": "u32" } },
  "contract_real":     { "outputs": { "page_size": 25, "url_param_default": 25 } },
  "diff_kind": "minor_extension",
  "extra_fields": ["url_param_default"],
  "related_memory": [...]   // decisiones previas sobre contratos
}
```

**Output**:
```jsonc
{
  "decision": "elevate",          // elevate | force_real
  "reasoning": "El campo url_param_default es información derivada útil para frontend tests; consistente con el patrón previo en T-0030.",
  "elevated_contract": {           // si decision=elevate
    "outputs": { "page_size": "u32", "url_param_default": "u32" }
  },
  "rollback_required": false      // solo true si force_real implica revertir cambios
}
```

## Lógica de decisión (guía del prompt)

```
ELEVATE si:
- El campo extra es derivado, no inventado.
- Es consistente con patrón en otros contratos del thread/proyecto.
- El generator declaró la intención en su submit.
- No introduce dependencias externas no autorizadas.

FORCE_REAL si:
- El campo extra es secreto / información sensible que no debería estar.
- Rompe consistencia con el resto del proyecto.
- El campo extra implica side effects no documentados.
- Hay drift en tipos (declared = u32, real = string).
```

## Prompt base (bosquejo, corto)

```
Eres un Arbitrator. Recibes un drift_minor entre contract_declared y contract_real.
Tu trabajo es decidir: elevate (subir el declared para incluir lo nuevo) o force_real
(forzar al real a ajustarse al declared).

CRITERIOS DE ELEVATE
- Campo extra es valor derivado útil.
- Consistente con patrones previos (consulta memory.search).
- Sin secretos ni side effects no documentados.

CRITERIOS DE FORCE_REAL
- Campo extra es secreto / sensible.
- Rompe consistencia.
- Drift de tipos.

OUTPUT FORMAT (json estricto)
{
  "decision": "elevate" | "force_real",
  "reasoning": "<1-2 líneas>",
  "elevated_contract": <object si elevate>,
  "rollback_required": <bool si force_real>
}

NO HACER
- Acción más allá de devolver el JSON.
- Modificar tasks o estado.
- Sugerir cambios al código de aplicación.
```

## Operación interna

```
Rust detecta drift_minor en QA verification
    │
    ▼
contracts.arbitrate_minor(task_id)
    │
    ▼
Spawn temporal de arbitrator (claude/codex con prompt-template arriba)
    │
    ▼
Recibe decision JSON
    │
    ▼
Si elevate:
   - Rust actualiza task.contract_declared con elevated_contract
   - task.status → done con verified_by=arbitrator+qa
   - memory.note (auto-approved si el patrón se repite >2 veces): "elevated contract for X-style cases"
Si force_real:
   - task.status → in_progress con feedback: "rollback X field; arbitrator decided force_real because Y"
   - assignee re-recibe la task
```

## Skills relevantes (cuando F5)

El arbitrator puede acumular skills tipo "cómo decidir drift sobre Y patrón". Pero son skills cortas y se mantienen en `skills/agent_created/` con tag `arbitrator`.

## Auditoría

Cada arbitración deja:
- `events.jsonl` entry `arbitration.decided { task, decision, reasoning, cost_usd, model }`.
- Si elevate → diff del contract_declared visible en git history.
- Si force_real → feedback visible en next attempt del generator.

## Anti-patrones

| Mal | Bien |
|---|---|
| Arbitrator decide major drift | Major drift va directo al orchestrator (re-plan); arbitrator es solo minor |
| Arbitrator modifica state directo | Devuelve recomendación; Rust aplica |
| Modelo caro (Opus) para arbitrator | Modelo barato (Haiku); es una decisión simple |
| Arbitrator hace múltiples calls | One-shot; si dudas, force_real para que humano vea |
