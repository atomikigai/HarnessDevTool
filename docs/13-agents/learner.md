---
id: agents/learner
title: Agent — Learner (skills proposer)
shard: 13-agents
tags: [agent, learner, skills, async, proposed]
role: learner
domain: none
cli: claude
summary: Async batch. Observa traces, propone skills/patches en `proposed/`. Default fase F5 (stub en F3).
related: [agents/overview, agents/curator, foundations/lessons-learned]
sources: [foundations/lessons-learned]
---

# Agent — Learner

> **Estado**: stub funcional en F3; activación real en **F5**. Antes de F5 el learner no propone skills, solo registra observaciones en `~/.harness/profiles/<p>/learner/observations.jsonl` para análisis posterior.

## Cuándo se invoca

**Asíncrono, fuera del loop principal**. Trigger:
- Al cierre de cada `turn` con éxito (hook `on_turn_completed`).
- Al cierre de cada task (`done` o `verify-fail` final tras N retries).
- Bajo demanda: `harness learner run --since 7d`.

**Batch**: el learner real no corre por cada turn (caro). Acumula observations y dispara una pasada cada N turns o cada X horas.

## Filosofía

> "PROPONE, NUNCA aplica en caliente." (Hermes, Anthropic). El learner crea drafts en `skills/proposed/`. El humano revisa y promueve.

## Capabilities declaradas

### MCPs disponibles
| MCP | Cuándo cargarlo |
|---|---|
| `harness-bridge` | **siempre** |

### Skill tags
- `markdown` (para escribir bien las skills propuestas)

### Tools permitidas
- `memory.search`, `memory.search_events`, `memory.read`
- `skills.search`, `skills.get` (para evitar duplicar lo que ya existe)
- `tasks.get` (para mirar tasks completadas)
- `skill_manage(action="create", target="proposed")` (con approval implícito; va a `proposed/` no a `agent_created/`)
- `repo.read_file` (para mirar artifacts referenciados)

**No** modifica skills existentes en `agent_created/`. Solo escribe en `proposed/`.

## Triggers concretos para proponer skill

Heurística (Rust + LLM para refinar):

1. **Turn complejo exitoso**: ≥ 5 tool calls, completó sin retry, contract sin drift_major.
   → propose skill "cómo X se hace bien".

2. **Recovery exitoso**: el agente tuvo un dead-end, encontró el camino correcto en N+1 attempt.
   → propose skill con foco en el recovery path.

3. **User corrigió trayectoria**: humano envió input intermedio que cambió la dirección.
   → propose skill con la lección de la corrección.

4. **Patrón repetido**: el mismo tipo de tool call sequence aparece en N (≥3) tasks distintas.
   → propose skill de consolidación del patrón.

## Formato de skill propuesta

`profiles/<p>/skills/proposed/<YYYY-MM-DD>-<slug>.md`:

```yaml
---
id: refactor-svelte-store
title: Refactor de writable Svelte store a derived
source: agent_created                   # se hace agent_created al promover
status: proposed                        # cambia a active al promover
proposed_at: 2026-05-26T13:00:00Z
proposed_by: agent:learner-1
reason: "Patrón observado en T-0042, T-0048, T-0051 (3 tasks last 7d)"
confidence: 0.78                        # learner reporta su propia confianza
triggers:
  intents: ["refactor svelte stores", "convert writable to derived"]
  file_patterns: ["**/*.svelte", "**/stores/*.ts"]
verification:
  - "svelte-check sin nuevos errores"
  - "tests del store pasan"
related_memory: []
related_traces: ["T-0042", "T-0048", "T-0051"]   # tasks donde se observó
---

# Refactor de writable Svelte store a derived

[cuerpo del procedimiento + pitfalls]
```

## Lo que NO debe hacer el learner

- ❌ Modificar skills existentes en `agent_created/`.
- ❌ Borrar nada.
- ❌ Aplicar cambios "auto" sin pasar por humano.
- ❌ Acumular drafts sin filtrar — si `confidence < 0.5`, descartar.
- ❌ Proponer skills con info propietaria (info que claramente menciona detalles de un cliente).

## Pipeline al "promover" (lo hace el humano vía UI)

```
1. Humano abre /skills → tab "Proposed" → click sobre el draft.
2. Lee el cuerpo + cuerpo + reason + confidence.
3. Decide:
   a. Promote: skill_manage(action="promote", from="proposed", to="agent_created").
      Opcionalmente edita el body antes de promover.
   b. Promote to shared: si determina que es genérico → goes to shared/skills/.
   c. Reject: borrar de proposed/ con log de razón.
   d. Pin para revisar después: queda en proposed/ con tag pinned.
4. Si promove → entry en git history del profile (o shared).
```

## Activación por fase

| Fase | Comportamiento |
|---|---|
| F3 | Stub: solo registra observations en `learner/observations.jsonl` |
| F4 | Stub |
| **F5** | Activo: propone drafts en `proposed/`; modo `auto-promote` desactivado |
| F6 | Activo + opción `auto-promote-if-confidence > N` (todavía con review humano) |

## Spawn hint default (cuando F5)
```toml
mcp     = ["harness-bridge"]
skills  = []
tools   = ["memory.*", "skills.search", "skills.get", "skill_manage", "repo.read_file"]
```

## Costos esperados

Por pasada (batch sobre 100 turns nuevos): ~$0.50–$2.00. Configurable `learner.max_cost_per_run_usd`.

## Anti-patrones

| Mal | Bien |
|---|---|
| Aplicar auto sin review humano | Siempre `proposed/`; humano promueve |
| Proponer skills idénticas a las existentes | `skills.search` antes de proponer; merge si overlap |
| Drafts con info sensible (nombres de cliente) | Sanitizar; humano filtra al promover |
| Ejecutar por cada turn (caro) | Batch cada N turns / X horas |
| Confianza alta sin evidencia | Score basado en repeticiones y outcome real |
