---
id: agents/psychologist
title: Agent — Psychologist (USER.md updater)
shard: 13-agents
tags: [agent, psychologist, user-model, memory, async]
role: learner
domain: user
cli: claude
summary: F6 only. Lee threads recientes y actualiza USER.md con preferencias persistentes del humano.
related: [agents/overview, memory/overview, foundations/lessons-learned]
sources: [foundations/lessons-learned]
---

# Agent — Psychologist

> **Estado**: F6. **No activado** en fases anteriores. Concepto inspirado en "Honcho dialectic" de Hermes Agent.

## Qué hace

Lee periódicamente los threads recientes del usuario (en el profile activo o global) y **propone updates a USER.md** capturando preferencias persistentes.

**Distinción clave**: actualiza `USER.md` (global, capa 5a) y/o `PROFILE.md` (overlay laboral, capa 5b). No toca skills, no toca memoria estructurada — esos son territorios de otros agentes.

## Cuándo se ejecuta

- Trigger automático: cada **2 semanas** si hay ≥ 5 threads completados desde el último run. Configurable.
- Manual: `harness psychologist run --scope global|profile`.
- Nunca en hot path. Es batch, asíncrono, **caro y poco frecuente**.

## Filosofía

Tres principios duros:
1. **Solo cambios persistentes**. Si el usuario expresó preferencia una vez, es ruido. Si la expresó 3+ veces de formas distintas, es señal.
2. **Approval obligatorio del humano**. Toda propuesta de update va a inbox.
3. **Cero psychoanalysis**. Captura preferencias de trabajo (estilo, lenguaje, frameworks), no interpretaciones de personalidad.

## Capabilities declaradas

### MCPs disponibles
| MCP | Cuándo cargarlo |
|---|---|
| `harness-bridge` | **siempre** |

### Skill tags
- `markdown` (escribe en USER.md/PROFILE.md, que son markdown)

### Tools permitidas
- `memory.search`, `memory.search_events`
- `repo.read_file` (lee USER.md / PROFILE.md actuales)
- `user_model.propose_update(target: "global"|"profile", patch: ...)` (custom tool del harness-bridge; dispara approval)

**No** tiene `shell.exec`, `task.*`, `skills.*`. Su scope es muy estrecho.

## Tipo de cambios que propone

| Aceptable | No aceptable |
|---|---|
| "Prefiere TOML sobre JSON para configs" | "Es introvertido" |
| "Trabaja con dos contextos: personal y work-acme" | "Está estresado últimamente" |
| "Pide ejemplos antes de cambios grandes" | "Necesita validación emocional" |
| "Idioma neutro español, identifiers en inglés" | "Le frustra cuando..." |
| "Stack preferido: Rust + SvelteKit + pnpm" | (especulaciones psicológicas) |

## Pipeline

```
1. Cada 2 semanas (o manual):
   a. Recoge threads completados desde last_run.
   b. Recoge user_messages relevantes (no tool_calls, no assistant_msg).
   
2. Cluster por temas: estilo, stack, idioma, no-goals, etc.

3. Para cada cluster con ≥ 3 menciones:
   - Mira el USER.md/PROFILE.md actual.
   - Compara: ¿ya está? ¿contradice? ¿amplía?
   - Si cambio neto → genera propose_update.

4. Dispara approval.request con preview del diff.

5. Si humano aprueba → patch aplicado + commit en git del profile (o del global si scope=global).
6. Si humano rechaza → log en learner/observations.jsonl para no re-proponer el mismo cluster pronto.
```

## Spawn hint default (F6)
```toml
mcp     = ["harness-bridge"]
skills  = ["markdown"]
tools   = ["memory.search", "memory.search_events", "repo.read_file", "user_model.propose_update"]
```

## Costos esperados

Por pasada: ~$1.00–$3.00 (lee N threads, analiza). Configurable `psychologist.max_cost_per_run_usd`.

## Output: ejemplo de propose

```jsonc
{
  "scope": "profile",                  // profile | global
  "target_file": "profiles/personal/PROFILE.md",
  "diff": [
    { "section": "## Estilo de comunicación esperado",
      "operation": "append",
      "content": "- Prefiere ejemplos concretos antes de cambios grandes en arquitectura."
    }
  ],
  "evidence": [
    "Thread 01HX...: 'dame un ejemplo para ver si estamos claros'",
    "Thread 01HY...: 'antes de hacer cambios pesados dame casos'",
    "Thread 01HZ...: 'me gustaria ver como se ve esto antes'"
  ],
  "confidence": 0.86
}
```

UI: modal "Preference detected" con el diff + evidencia. Botones Allow / Edit & Allow / Deny.

## Lo que NO hace (límites estrictos)

- ❌ No actualiza `USER.md` sin approval.
- ❌ No modifica skills.
- ❌ No emite juicios de personalidad.
- ❌ No infiere estado emocional.
- ❌ No accede a contenido externo al harness (emails, social media, etc.).
- ❌ No retiene observaciones rechazadas indefinidamente; ttl de 6 meses.

## Privacidad

Si el usuario activa `psychologist`, todas las observaciones rechazadas se purgan del log tras 6 meses. Si el usuario lo desactiva, el archivo `learner/psychologist_observations.jsonl` se elimina automáticamente.

## Activación por fase

| Fase | Comportamiento |
|---|---|
| F3–F5 | Disabled |
| **F6** | Disabled por default; opt-in vía `harness psychologist enable` |

## Anti-patrones

| Mal | Bien |
|---|---|
| Auto-apply sin review | Siempre approval del humano |
| Analizar personalidad | Solo preferencias de trabajo |
| Cluster por 1 mención | Mínimo 3 menciones distintas |
| Re-proponer cluster ya rechazado | TTL 6 meses sobre rechazos |
| Modelo caro para cada run | Haiku o equivalente económico |
