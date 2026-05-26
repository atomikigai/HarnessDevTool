---
id: harness-core/context-compaction
title: "[Tombstone] Context compaction"
shard: 03-harness-core
tags: [tombstone, deprecated]
summary: Obsoleto. Compaction es responsabilidad del CLI hijo; nuestro modelo equivalente es spawn fresh + handoff.
related: [agents/spawn-lifecycle, memory/continuity, foundations/anthropic-principles]
sources: []
---

# [Tombstone] Context compaction

> Este shard cubría auto-compaction y context reset al estilo Codex/Anthropic. **Ya no aplica directamente**: el `claude`/`codex` maneja su propia compaction interna.

## Equivalente en nuestra arquitectura

Como los spawns son **efímeros** (uno por task), el "reset" sucede naturalmente:
- Cada task = nuevo proceso → contexto fresco.
- El handoff entre tasks es **estructural**: spec.md + task.toml + skills cargadas.
- No hay "contexto que llenar" del lado nuestro; el CLI hijo es quien podría llenarse y manejarlo.

Si una task crashea por límite de contexto del CLI:
1. Se marca `verify_fail` con razón.
2. El orchestrator re-plana (cap K=2).
3. El nuevo spawn arranca limpio.

## Ver en su lugar

- [[agents/spawn-lifecycle]] — modelo efímero como "reset implícito"
- [[memory/continuity]] — handoff vía CONTINUITY.md (entre sesiones humanas, no entre spawns)
- [[foundations/anthropic-principles]] §"Context anxiety" — lección original, sigue siendo válida conceptualmente
