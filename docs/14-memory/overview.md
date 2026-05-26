---
id: memory/overview
title: Memoria — overview
shard: 14-memory
tags: [memory, overview, continuity]
summary: La memoria vive en el harness, estructurada en capas, versionada con git y aislada por profile.
related: [memory/layout, memory/entry-format, memory/lifecycle, memory/continuity, memory/search-and-index, memory/git, cross-cutting/profiles]
sources: [foundations/lessons-learned]
---

# Memoria — overview

> **La memoria vive en el harness, nunca en el modelo.** Cada spawn de `claude`/`codex` arranca con contexto fresco; el harness reconstruye el "qué sabemos" desde disco al construir el prompt inicial.

## Las 7 capas

| # | Capa | Vida | Donde vive |
|---|---|---|---|
| 0 | Contexto del CLI vivo | Efímera (muere con el spawn) | RAM del proceso `claude` |
| 1 | Event log | Permanente | `profiles/<p>/threads/<tid>/events.jsonl` |
| 2 | Tasks | Permanente | `profiles/<p>/threads/<tid>/tasks/*.toml` |
| 3 | Spec | Permanente | `profiles/<p>/threads/<tid>/spec.md` |
| 4 | Skills | Permanente, cross-thread | `profiles/<p>/skills/` + `shared/skills/` |
| 5a | USER.md global | Permanente, cross-profile | `~/.harness/USER.md` |
| 5b | PROFILE.md | Permanente, per profile | `profiles/<p>/PROFILE.md` |
| 6 | Memory estructurada (decisions/pending/in-flight/facts) | Permanente, per profile | `profiles/<p>/memory/` |
| 7 | Project memory | Vive en el repo del usuario | `<repo>/AGENTS.md` |

La capa 0 **no se preserva** entre spawns. Las capas 1–6 se reconstruyen al iniciar.

## Filosofía

1. **Estructura > prosa**. Toda memoria tiene frontmatter YAML validado; el cuerpo es Markdown para el modelo.
2. **Continuidad explícita**. `CONTINUITY.md` regenerado automáticamente; UI banner al volver lo muestra.
3. **Carga inteligente**. Top-level índices se cargan siempre; entradas específicas se buscan vía `memory.search` cuando hacen falta.
4. **Auditable**. Toda memoria bajo git por profile; cada cambio es un commit.
5. **Solo el humano escribe libre**. Los agentes pueden `memory.note` con approval del humano.
6. **Local-first**. Nada va a la nube; opt-in a sincronizar via tu propio remote git.

## Decisiones bloqueadas (tras discusión)

| Decisión | Valor |
|---|---|
| Profiles existen | **Sí** (caso real: dos trabajos, mismo stack) |
| Skills: default scope | **Profile-scoped**; `promote` a `shared/` con review |
| USER.md | **Global** + `PROFILE.md` por profile |
| Auth de `claude`/`codex` | **Per profile** vía `cli-state/` aislado |
| Inyección de continuidad al prompt | **Solo al resume** de thread existente |
| `memory.note` de agentes | **Approval obligatorio** del humano |
| Regeneración de `CONTINUITY.md` | **On change** + 1h fallback |
| Formato de entradas | **YAML frontmatter + Markdown body** |
| Índice de búsqueda | **SQLite FTS5** |
| Versionado | **Git por profile + git para shared** |

Ver [[memory/layout]] para el layout en disco, [[memory/lifecycle]] para cómo evolucionan las entradas, [[memory/continuity]] para el snapshot vivo y [[cross-cutting/profiles]] para el concepto de profile.

## Cómo "recuerda" un spawn nuevo

Al construir el prompt inicial:
```
1. USER.md (global)             — quién es el humano
2. PROFILE.md (active)          — contexto laboral del profile activo
3. AGENTS.md (proyecto del cwd) — instrucciones del repo del usuario
4. Spec.md (slice relevante)    — qué se construye en este thread
5. Task TOML                    — qué hay que hacer
6. Skills relevantes (top-K)    — memory procedimental cargada
7. CONTINUITY.md (si es resume) — qué quedó pendiente del thread
8. Memory search top-level      — índice; el agente busca on-demand
```

Total típico: 6–12 KB de contexto persistente cargado upfront. Resto bajo demanda.

## Tools expuestas a los agentes (vía `harness-bridge`)

| Tool | Acción |
|---|---|
| `memory.search` | FTS5 sobre todas las capas de memoria |
| `memory.read` | leer una entrada completa por id |
| `memory.continuity` | leer `CONTINUITY.md` |
| `memory.note` | proponer una entrada (approval obligatorio) |
| `memory.update` | proponer cambio de entrada (approval obligatorio) |
| `memory.resolve` | mover in_flight → decisions/pending |

Ver [[agents/rust-rails]] §`memory.*` para firmas completas.

## Anti-patrones

| Mal | Bien |
|---|---|
| Memoria en el modelo (asumir que `claude` recuerda ayer) | Memoria en el harness, reconstruida al spawn |
| Dump completo de pendientes en cada prompt nuevo | Solo al resume del thread específico |
| Agentes escribiendo memoria libremente | Approval del humano para `memory.note` |
| Prosa libre buscable solo por regex | Frontmatter + FTS5 |
| Sin versionado | Git per profile |
| Un solo archivo grande de memoria | Shards pequeños indexables |
