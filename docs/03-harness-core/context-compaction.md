---
id: harness-core/context-compaction
title: Compaction y context reset
shard: 03-harness-core
tags: [compaction, context-window, reset]
summary: Dos estrategias para vivir más allá del límite de tokens.
related: [foundations/anthropic-principles, harness-core/prompt-caching]
sources: [foundations/openai-codex-architecture, foundations/anthropic-principles]
---

# Compaction y reset

## Compaction (por defecto)
Cuando el conteo de tokens excede `auto_compact_limit` (configurable, p.ej. 75% del límite del modelo):
1. Core llama al endpoint `/responses/compact` del provider.
2. Recibe una lista de items reducida que incluye un item especial `kind = compaction` con `encrypted_content` (blob opaco).
3. El blob codifica entendimiento latente — más rico que un resumen textual y privacy-preserving.
4. El historial in-memory se reemplaza por la lista compactada; el `events.jsonl` registra el evento `compaction.applied`.

Beneficios:
- Conserva intención y referencias específicas.
- Reduce tokens drásticamente.
- ZDR-friendly: el provider solo guarda claves.

Coste:
- Una llamada extra al provider.
- Invalida el prefix cache desde ese punto.

## Context reset (estrategia adversaria a context anxiety)
Cuando el modelo muestra signos de "wrap up prematuro":
1. El harness extrae un **handoff document** estructurado (resumen de spec + lo hecho + próximos pasos).
2. Crea un thread nuevo (o un sub-thread) inicializado con el handoff.
3. El generator empieza con contexto fresco; nunca "siente" cercanía al límite.

Cuándo usarlo:
- Tareas largas con generator (no QA).
- Modelos con anxiety conocida (Sonnet 4.5). Opus 4.6+ rara vez lo necesita.

## Política por defecto del proyecto
- Compaction automática al 75%.
- Reset manual (operación `thread.reset_with_handoff`) — el agente puede pedirlo via tool.
- Loggear ratio de tokens ahorrados y caídas de cache hit-rate por compaction.

## Heurísticas para detectar anxiety
- El modelo emite cierres ("In summary, ..." con tareas a medio terminar).
- Tasa decreciente de tool calls cerca del límite.
- "I'll continue this in another session" en texto.

Si se detecta dos veces seguidas → recomendar reset al usuario.
