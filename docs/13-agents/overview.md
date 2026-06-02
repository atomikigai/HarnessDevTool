---
id: agents/overview
title: Agentes — overview
shard: 13-agents
tags: [agents, overview, registry, roles]
summary: Índice de agentes, mapa rol × dominio y diagrama del loop con rails de Rust.
related: [agents/autonomy-protocol, agents/spawn-lifecycle, agents/smart-loading, agents/capability-registry, agents/rust-rails, foundations/lessons-learned]
sources: [foundations/lessons-learned, foundations/anthropic-principles]
---

# Agentes — overview

> Modelo: **agentes efímeros con plantillas componibles** (opción C). Cada task spawnea un `claude`/`codex` fresh con una plantilla declarada + capacidades cargadas según el contexto. Sin pool de procesos vivos.

## Roles canónicos

| Rol | Función | Output |
|---|---|---|
| **planner** | Analiza prompt, clarifica, descompone, declara contratos | `spec.md` + grafo de tasks + `contract_declared` |
| **generator** | Ejecuta una task concreta | artifacts + `contract_real` |
| **evaluator** | Escribe tests y verifica; **Rust los corre** | verdict + feedback |
| **arbitrator** | Resuelve `drift_minor` (declared vs real) | decisión: elevar contrato vs forzar real |
| **curator** | Mantenimiento de skills en background (F6) | reports, archives |
| **learner** | Propone skills/ajustes desde traces (F5 async) | drafts en `proposed/` |
| **psychologist** | Mantiene `USER.md` con preferencias del usuario (F6) | `USER.md` actualizado |

## Set de agentes inicial

| Shard | Rol | Dominio | Fase |
|---|---|---|---|
| [[agents/orchestrator]] | planner | — | F2 |
| [[agents/frontend]] | generator | frontend (SvelteKit/Tailwind/shadcn) | F3 |
| [[agents/backend]] | generator | backend (Rust/Axum/sqlx) | F3 |
| [[agents/database]] | generator | database (SQL, migraciones) | F3 |
| [[agents/devops]] | generator | devops (Docker, CI, deploy) | F3 |
| [[agents/qa]] | evaluator | qa (escribe tests; Rust los corre) | F3 |
| [[agents/generic]] | generator | sin dominio (fallback) | F3 |
| [[agents/arbitrator]] | arbitrator | — | F3 |
| [[agents/learner]] | learner | — | F5 (stub en F3) |
| [[agents/curator]] | curator | — | F6 (stub en F5) |
| [[agents/psychologist]] | learner | usuario | F6 |

## Filosofía

1. **Plantillas declaradas, capacidades activadas**. El shard de cada agente declara lo que **PUEDE** cargar (MCPs, skills, tools). El spawn decide lo que **CARGA** según la task. Ver [[agents/smart-loading]].
2. **Efímeros**. Un agente vive lo que dura su task. Recuperación de crash = nuevo spawn. Ver [[agents/spawn-lifecycle]].
3. **Razonan sobre menús, no inventan**. Toda decisión del agente que requiera "saber qué hay" pasa por una rail de Rust. Ver [[agents/rust-rails]].
4. **Comunican por archivos**. Ningún agente habla con otro en vivo. spec.md / tasks/*.toml / artifacts/ son el bus.
5. **Autonomía proporcional**. El harness decide `quick | standard | project | exploratory | blocked` antes de gastar tokens y aplica el perfil `manual | assisted | autonomous | ci`. Ver [[agents/autonomy-protocol]].
6. **Rol ≠ dominio**. Un agente con rol `generator` puede tener dominio `frontend` o ninguno; el dominio es un módulo componible.

## Diagrama de roles en el loop

```
            USER
             │ prompt
             ▼
     ┌─────────────────┐
     │ readiness check │  (repo/env/cli_auth/budget)
     └────────┬────────┘
              │ execution_mode + autonomy_profile
              ▼
     ┌─────────────────┐
     │  orchestrator   │  (planner; F2)
     │  planner        │  ── usa rails: agents.*, repo.*, budget.*
     └──┬──────┬───┬───┘
        │ task │   │ contract.declared
        ▼      ▼   ▼
   ┌──────────────────┐
   │  task TOML       │ ← contiene spawn_hint
   └────────┬─────────┘
            │ scheduler claim
            ▼
     ┌───────────────┐         ┌──────────────┐
     │  generator    │  ───►   │ contract.real│
     │ (frontend|    │         └──────┬───────┘
     │  backend|     │                │
     │  database|    │                ▼
     │  devops|      │         ┌──────────────┐
     │  generic)     │         │  evaluator   │ ─► Rust corre tests
     └───────────────┘         │  qa          │
                               └──────┬───────┘
                                      │ pasa
                                      ▼
                               ┌──────────────┐
                               │ Rust: diff   │
                               │ declared vs  │
                               │ real         │
                               └──┬───────────┘
                                 │ drift_minor?
                                 ▼
                          ┌──────────────┐
                          │ arbitrator   │ (decide elevar vs forzar)
                          └──────────────┘
   
   Asíncronos (fuera del loop):
     ├─ learner   (propone skills al ver traces)
     ├─ curator   (mantiene corpus de skills)
     └─ psychologist (USER.md, F6)
```

## Reglas duras

- Ningún rol se auto-aprueba: `verified_by != assignee` (anti auto-elogio Anthropic).
- Ningún agente borra skills; solo archiva (Hermes).
- Toda capacidad activada en un spawn debe estar declarada en el shard del agente.
- Todo cambio de contrato lo decide el arbitrator o el humano, nunca el generator.
- Re-plan cap `K = 2` por task (ver [[build-plan/decisions-locked]]).
