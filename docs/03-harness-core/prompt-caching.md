---
id: harness-core/prompt-caching
title: "[Tombstone] Prompt caching"
shard: 03-harness-core
tags: [tombstone, deprecated]
summary: Obsoleto. El prefix caching es responsabilidad del CLI hijo y su provider.
related: [agents/spawn-lifecycle, foundations/lessons-learned]
sources: []
---

# [Tombstone] Prompt caching

> Este shard cubría el manejo de prefix caching contra el provider. **Ya no aplica**: lo gestiona el `claude`/`codex` con su propia API.

## Implicaciones para nosotros

Lo que el harness puede hacer:
- **Spawn fresh por task** (efímeros) → cada CLI hijo paga su cold cache la primera vez.
- **No reusar procesos** entre tasks (decisión bloqueada).
- **Smart loading** (cargar solo MCPs/skills necesarios) reduce el tamaño del prompt inicial, beneficio independiente.

Ya no tenemos que pensar en:
- Orden estricto de segmentos
- `BTreeMap` de tool defs
- Append-only del prompt
- Hashes de prefix

## Ver en su lugar

- [[agents/spawn-lifecycle]] — modelo efímero
- [[agents/smart-loading]] — minimización del prompt inicial
- [[foundations/lessons-learned]] §A3 — el principio sigue vigente para `events.jsonl` y memoria
