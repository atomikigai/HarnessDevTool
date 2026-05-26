---
id: harness-core/prompt-caching
title: Prompt caching (prefix)
shard: 03-harness-core
tags: [cache, prompt, performance]
summary: Qué preserva y qué invalida el prefix cache del provider.
related: [harness-core/prompt-construction, foundations/openai-codex-architecture]
sources: [foundations/openai-codex-architecture]
---

# Prefix caching

## Garantía
Si dos requests comparten exactamente el mismo prefijo de tokens, el provider reutiliza la computación del prefijo. Costos y latencia bajan **dramáticamente** en conversaciones largas.

## Invalida el cache (todos)
- Reordenar tool definitions.
- Cambiar `model`.
- Cambiar `sandbox` o `approval_mode` y **reescribir** el developer_message original (en vez de apendizar).
- Cambiar `cwd` y reescribir env_context.
- Cualquier mutación retroactiva del historial (edit, delete).
- Añadir o quitar una tool en mitad de conversación si se edita la sección original (apendizar uno nuevo está OK pero rompe el cache desde ese punto adelante igualmente — al menos preserva todo lo previo).

## **No** invalida
- Apendizar nuevos items al final.
- Crear un nuevo turn dentro del mismo thread.
- Streaming parcial (los chunks no afectan el cache del request siguiente).

## Política operativa
- Toda mutación tardía → **apendizar** un nuevo developer_message marcador (`<<config_update>>`) en lugar de editar arriba.
- Tool defs ordenadas siempre `BTreeMap<name, def>` antes de serializar.
- `prompt_prefix_hash` (sha256 del JSON serializado) se loggea por request. Lo usamos para diagnosticar misses.

## Anti-patrón: "limpiar"
Quitar items antiguos para "ahorrar tokens" rompe el cache → más caro. Si necesitas reducir tamaño, usa [[harness-core/context-compaction]], que el provider entiende y respeta cache de manera específica.
