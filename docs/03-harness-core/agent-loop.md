---
id: harness-core/agent-loop
title: "[Tombstone] Agent loop"
shard: 03-harness-core
tags: [tombstone, deprecated]
summary: Obsoleto tras pivote. El agent loop vive en el CLI hijo (claude/codex), no en nuestro código.
related: [agents/overview, agents/spawn-lifecycle, agents/orchestrator, foundations/lessons-learned]
sources: []
---

# [Tombstone] Agent loop

> Este shard se escribió cuando el plan era construir nuestro propio agent loop (modelo Codex). **Ya no aplica**.

## Estado actual

El usuario tiene `claude` o `codex` instalados. **Esos CLIs corren su propio agent loop** (prompt → modelo → tool calls → repetir). Nuestro `harness-server` orquesta **al nivel de tasks**: spawnea CLIs, expone tools vía MCP, gestiona memoria/skills/budgets.

No construimos:
- Bucle prompt-respuesta
- Construcción de prompt para el modelo
- Caching de prefix
- Compaction
- Streaming desde provider

Lo hace todo el CLI hijo. Nosotros somos **el harness alrededor**.

## Ver en su lugar

- [[agents/overview]] — roles y diagrama actual del loop
- [[agents/spawn-lifecycle]] — cómo orquestamos el CLI (efímero, lease)
- [[agents/orchestrator]] — el "planner LLM" que SÍ tenemos (delegado al CLI)
- [[foundations/lessons-learned]] §A1 — la decisión de delegar
