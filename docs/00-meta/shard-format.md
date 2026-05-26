---
id: meta/shard-format
title: Formato de un shard
shard: 00-meta
tags: [meta, convention, indexing]
summary: Frontmatter YAML obligatorio y reglas de enlaces cruzados.
related: [meta/conventions, meta/glossary]
sources: []
---

# Formato de shard

Cada archivo `*.md` bajo `docs/` (excepto `README.md`) es un **shard**: una unidad atómica, ≤ ~150 líneas, un concepto.

## Frontmatter obligatorio

```yaml
---
id: <grupo>/<slug-kebab>          # único en todo docs/. Ej: harness-core/agent-loop
title: <Título humano>
shard: <NN-grupo>                 # carpeta contenedora
tags: [tag1, tag2, ...]           # minúsculas, kebab-case
summary: <una línea, ≤ 120 chars> # se usa para búsqueda rápida del LLM
related: [<id>, <id>, ...]        # IDs de shards vecinos en el grafo
sources: [<id>, ...]              # ids bajo references/* o foundations/*
---
```

## Reglas de redacción
- Una idea por shard. Si crece > 150 líneas, partir.
- Empezar con `# <Title>` y un párrafo de contexto (qué es, cuándo importa).
- Usar listas y tablas para densidad. Evitar prosa larga.
- Cross-refs: `[[grupo/slug]]` resuelve al `id` de otro shard. Nunca enlazar rutas relativas.
- Código: bloques con lenguaje (` ```rust `, ` ```toml `).
- Sin emojis salvo que el lector humano lo pida.

## Idioma
- Documentación en **español** (es-LA, neutro).
- Identificadores, código, frontmatter y términos técnicos canónicos (turn, item, thread, prompt, sandbox) en inglés.

## Por qué shards pequeños
Un LLM cargando contexto puede leer 5 shards (~600 líneas) en vez de un manual de 8000 líneas. El frontmatter + el índice [[../README]] permiten resolver "¿dónde está X?" en una sola consulta de grep.
