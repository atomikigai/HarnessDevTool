---
id: meta/conventions
title: Convenciones de redacción y nombres
shard: 00-meta
tags: [meta, convention, style]
summary: Estilo de prosa, nombres de crates/módulos y reglas anti-bloat.
related: [meta/shard-format, meta/glossary]
sources: []
---

# Convenciones

## Prosa
- Voz directa. Imperativo para guías ("Crea el crate", no "Se debe crear el crate").
- Evitar adjetivos vacíos ("robusto", "potente").
- Definir un término solo una vez; el resto enlaza a [[meta/glossary]].

## Nombres de código
- Crates Rust: `harness-core`, `harness-app-server`, `harness-sandbox`, `harness-mcp`, `module-agents`, `module-db`, `module-ssh`.
- Módulos: `snake_case`. Tipos: `PascalCase`. Funciones: `snake_case`.
- Eventos JSON-RPC: `dominio.accion` (ej. `thread.create`, `item.delta`).
- IDs de shard: `grupo/sub-tema` en kebab-case.

## Anti-bloat
- No documentar lo que el código ya dice (firmas, nombres). Documentar **el porqué**.
- Si un shard solo repite otro, fusionar.
- Si tres shards comparten un fragmento, extraerlo a un shard nuevo y enlazar.

## Versionado de docs
- Cambios mayores: bump `summary` y añadir nota al final del shard.
- Borrados: dejar tombstone (`id` + redirección) durante una iteración antes de eliminar.
