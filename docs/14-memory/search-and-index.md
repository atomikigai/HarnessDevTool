---
id: memory/search-and-index
title: Búsqueda e índice de memoria
shard: 14-memory
tags: [memory, search, fts5, sqlite, index]
summary: SQLite + FTS5 sobre el corpus de memoria + skills + events; semántica de memory.search.
related: [memory/overview, memory/entry-format, harness-skills]
sources: []
---

# Búsqueda e índice

## Stack

- **SQLite** con extensión **FTS5** (incluida en sqlx con feature `sqlite-bundled`).
- Un archivo `search.db` **por profile** en `profiles/<active>/search.db`.
- Indexa: entradas de memoria + skills (cuando F5) + items relevantes de `events.jsonl`.
- Regenerable: si se borra, el harness lo reconstruye al boot.

## Schema

```sql
CREATE VIRTUAL TABLE memory_fts USING fts5(
  id UNINDEXED,            -- memory/decisions/2026-05-26-tauri-out
  title,
  kind UNINDEXED,
  tags,
  body,
  related,                 -- tasks + threads + shards concatenados
  prefix='2 3 4',          -- prefix-matching para autocomplete
  tokenize='unicode61 remove_diacritics 2'
);

CREATE TABLE memory_meta (
  id        TEXT PRIMARY KEY,
  kind      TEXT NOT NULL,
  status    TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  path      TEXT NOT NULL,    -- ruta relativa al archivo
  size_bytes INTEGER NOT NULL
);

CREATE INDEX idx_memory_meta_kind   ON memory_meta(kind);
CREATE INDEX idx_memory_meta_status ON memory_meta(status);
CREATE INDEX idx_memory_meta_updated ON memory_meta(updated_at);
```

Análogamente `skills_fts`, `skills_meta`, `events_fts` (cuando F5).

## Mantenimiento del índice

- **Al boot**: scan de `memory/`, validación de schemas, reindex.
- **On write** (vía `memory.note` / `memory.update`): update incremental.
- **Al hacer git pull** (si el usuario tiene remote): reindex completo si cambios > 10 archivos.
- **Manual**: `harness memory reindex` regenera todo.

## Tool `memory.search`

Firma:
```ts
memory.search({
  query: string,                       // texto libre, FTS5 syntax
  kinds?: ("decision"|"pending"|"in_flight"|"fact"|"snapshot")[],
  tags?: string[],                     // intersección (AND)
  status?: ("open"|"settled"|"resolved"|"obsolete"|"active")[],
  top_k?: number,                      // default 10
  scope?: "active"|"all"               // "active" excluye status=obsolete; default active
}) → Array<{
  id: string,
  title: string,
  kind: string,
  status: string,
  snippet: string,                     // contexto con highlight
  rank: number,
  updated_at: string
}>
```

### FTS5 query syntax
- Frase exacta: `"tauri"`.
- AND implícito: `tauri descartar` = ambos.
- OR explícito: `tauri OR electron`.
- Negación: `tauri NOT obsolete`.
- Prefijo: `taur*`.
- Por columna: `title:tauri`.
- Boost: el harness no expone boosts complejos en v1; rank de FTS5 puro.

### Ejemplos de uso por agentes

**Orchestrator** al inicio de un thread nuevo:
```
memory.search({ query: "paginación", top_k: 5 })
→ encuentra decisión previa sobre paginación; carga al spec
```

**Generator** durante implementación:
```
memory.search({ query: "svelte stores derived", kinds: ["fact"], top_k: 3 })
→ encuentra fact "stores derived vs writable pattern"
```

**Evaluator** al validar:
```
memory.search({ query: "test pagination", kinds: ["decision"], tags: ["qa"], top_k: 3 })
→ encuentra decisión sobre cobertura de tests
```

## Tools relacionadas

### `memory.read(id)`
Lee una entrada completa. Devuelve frontmatter + body.

### `memory.list(filters)`
Lista entradas sin búsqueda libre, solo filtros estructurales (kind, status, tags). Útil para vistas tipo "todas las decisiones de este mes".

### `memory.related(id)`
Devuelve entradas relacionadas vía:
- `related_memory` (explícito en frontmatter).
- Tags compartidos (heurística).
- Co-citas en otras entradas (que mencionen el mismo task/thread).

## Performance

| Operación | Latencia objetivo |
|---|---|
| `memory.search` (top 10) | < 5 ms |
| `memory.read` (1 entrada) | < 1 ms |
| `memory.list` (filtered) | < 5 ms |
| Reindex completo (1000 entries) | < 2 s |

## Búsqueda cross-skill (cuando F5)

`skills.search` tiene la **misma API** que `memory.search` pero sobre `skills_fts`. Para los agentes, son tools separadas porque la **distinción semántica** importa:

- `memory.search` → "qué decidimos / qué quedó pendiente / qué hechos sabemos".
- `skills.search` → "cómo se hace bien esta clase de tarea".

Si el agente confunde cuál usar, el prompt-template del rol lo aclara explícitamente.

## Búsqueda en events (cuando F5)

`events_fts` indexa items de `events.jsonl` de todos los threads no archivados:
- `kind = assistant_message` (texto del modelo).
- `kind = user_message`.
- `kind = tool_call` (nombre + args resumidos).

Esto habilita `memory.search_events(query)` que **NO** es memoria estructurada, sino búsqueda libre en lo que el agente y el usuario han escrito antes. Útil para "¿en qué thread hablamos de X?".

## Anti-patrones

| Mal | Bien |
|---|---|
| Cargar todo el corpus al prompt | Buscar y traer top-K relevantes |
| Buscar sin filtros (devuelve ruido) | Usar `kinds` y `tags` cuando sea posible |
| Confundir `memory` con `skills` | Memoria = qué pasó/decidimos; Skills = cómo hacer |
| Índice como source of truth | Las entradas markdown son la verdad; el índice es derivado |
| Reindex completo en hot path | Incremental siempre; full solo en mantenimiento |
