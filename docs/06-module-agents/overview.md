---
id: module-agents/overview
title: "[Tombstone] Módulo Agentes"
shard: 06-module-agents
tags: [tombstone, deprecated]
summary: El "módulo Agentes" se promovió a runtime principal. Ver sección 13-agents.
related: [agents/overview, agents/spawn-lifecycle]
sources: []
---

# [Tombstone] Módulo Agentes

> En el modelo original, "Agentes" era un **módulo vertical** (como DB o SSH) que lanzaba sesiones de `claude` CLI. Tras el pivote (delegamos a CLIs como mecanismo central), los agentes son **el runtime principal**, no un módulo opcional.

## Ver en su lugar

- [[agents/overview]] — set completo de roles y diagrama
- [[agents/spawn-lifecycle]] — PTY, lease, recovery
- [[agents/smart-loading]] — carga inteligente de capacidades
- [[agents/orchestrator]], [[agents/frontend]], [[agents/backend]], [[agents/database]], [[agents/devops]], [[agents/qa]], [[agents/generic]], [[agents/arbitrator]] — agentes runtime
- [[agents/learner]], [[agents/curator]], [[agents/psychologist]] — agentes de auto-mejora
