---
id: harness-core/prompt-construction
title: "[Tombstone] Prompt construction"
shard: 03-harness-core
tags: [tombstone, deprecated]
summary: Obsoleto. El prompt lo construye el CLI hijo, no nuestro código.
related: [agents/spawn-lifecycle, agents/smart-loading, memory/overview]
sources: []
---

# [Tombstone] Prompt construction

> Este shard asumía que nosotros construíamos el prompt completo para el modelo. **Ya no aplica**: el `claude`/`codex` hijo lo hace.

## Lo que sí hacemos

Construimos un **prompt inicial pequeño** que inyectamos al CLI al spawn:
- USER.md global
- PROFILE.md del profile activo
- Spec slice del thread
- Task TOML
- Skills relevantes (top-K)
- CONTINUITY.md slice (solo en resume)

Esto va como **primer mensaje** al CLI hijo. De ahí en adelante, el CLI construye y mantiene su propio contexto.

## Ver en su lugar

- [[agents/spawn-lifecycle]] §"Bootstrap del spawn" — qué inyectamos al CLI
- [[agents/smart-loading]] — qué capacidades se cargan
- [[memory/overview]] §"Cómo recuerda un spawn nuevo" — la reconstrucción de contexto
