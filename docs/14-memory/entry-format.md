---
id: memory/entry-format
title: Memoria — formato de entrada
shard: 14-memory
tags: [memory, format, schema, yaml, frontmatter]
summary: Spec del frontmatter YAML, kinds, status, validación.
related: [memory/overview, memory/lifecycle, memory/search-and-index]
sources: []
---

# Formato de una entrada de memoria

> Todas las entradas son **archivos Markdown con frontmatter YAML**. El frontmatter es estructurado (parseable); el cuerpo es prosa para el modelo (humano-legible).

## Frontmatter obligatorio

```yaml
---
id: memory/<kind>/<YYYY-MM-DD>-<slug>     # único en todo el profile
title: <una línea humana>
kind: decision | pending | in_flight | fact | snapshot
status: open | settled | resolved | obsolete | active
created_at: 2026-05-26T11:20:00Z          # ISO 8601 UTC
updated_at: 2026-05-26T11:20:00Z          # actualizado en cada edit
created_by: user | orchestrator | learner | psychologist | <agent-name>
tags: [tag1, tag2, ...]                   # minúsculas, kebab-case
related_threads: [<thread-uuid>, ...]     # IDs de threads relacionados
related_tasks: [<task-id>, ...]           # IDs de tasks relacionadas
related_shards: [<doc-shard-id>, ...]     # docs en /docs/*
related_memory: [<memory-id>, ...]        # otras entradas de memoria
expires: never | YYYY-MM-DD               # solo relevante para in_flight; default never
---
```

## Mapping kind × status

| kind | status válidos | semántica |
|---|---|---|
| `decision` | `settled` (default), `obsolete` | Decisión firme. Obsolete ≠ borrada; permanece para auditoría. |
| `pending` | `open` (default), `resolved` | Algo postergado pero no abandonado. Resolved cuando se retoma. |
| `in_flight` | `active` (default), `resolved` | Tema en discusión ahora. Al cerrarse se promueve a `decision` o se demote a `pending`. |
| `fact` | `active` (default), `obsolete` | Patrón aprendido sobre proyecto/usuario. Obsolete cuando cambia. |
| `snapshot` | `active` (default), `obsolete` | Foto en el tiempo. Los viejos se marcan obsoletos tras 30d, archivan tras 90d. |

## Convenciones de cuerpo

Estructura recomendada por kind:

### `decision`
```markdown
# <Título>

**Razón**: por qué se tomó esta decisión.

**Decisión**: qué se decidió concretamente.

**Alternativas consideradas**: las que se descartaron + por qué.

**Reabrir si**: condiciones bajo las cuales esta decisión debería revisitarse.
```

### `pending`
```markdown
# <Título>

**Qué quedó pendiente**: descripción concreta.

**Por qué se postergó**: contexto.

**Para retomar**: qué necesitamos hacer cuando volvamos.

**Bloqueado por**: dependencias externas si las hay.
```

### `in_flight`
```markdown
# <Título>

**Qué estamos discutiendo**: el tema.

**Opciones sobre la mesa**:
- A: ...
- B: ...

**Próximo paso**: qué necesita pasar para resolver esto.

> Esta entrada se mueve a `decision/` cuando se resuelve, o a `pending/` si se posterga.
```

### `fact`
```markdown
# <Título>

**Hecho**: enunciado breve y concreto.

**Evidencia**: cómo lo aprendimos (thread, task, observación).

**Aplica cuando**: condiciones de uso.

**Anti-ejemplo**: cuándo NO aplicar.
```

### `snapshot`
```markdown
# Snapshot 2026-05-26T19:00:00Z

## Threads activos
- <thread-uuid> "...": <N> tasks (X in_progress, Y queued)

## Decisiones recientes
- ...

## Pendientes
- ...

## Aprendizajes destacados
- ...
```

## Schema JSON Schema (resumido)

Bajo `backend/crates/harness-core/schemas/memory-entry.v1.json`:

```jsonc
{
  "$id": "memory-entry.v1",
  "type": "object",
  "required": ["id", "title", "kind", "status", "created_at", "updated_at", "created_by"],
  "properties": {
    "id": { "type": "string", "pattern": "^memory/[a-z_-]+/\\d{4}-\\d{2}-\\d{2}-[a-z0-9-]+$" },
    "title": { "type": "string", "maxLength": 120 },
    "kind": { "enum": ["decision", "pending", "in_flight", "fact", "snapshot"] },
    "status": { "enum": ["open", "settled", "resolved", "obsolete", "active"] },
    "created_at": { "type": "string", "format": "date-time" },
    "updated_at": { "type": "string", "format": "date-time" },
    "created_by": { "type": "string", "pattern": "^(user|orchestrator|learner|psychologist|agent:[a-z0-9-]+)$" },
    "tags": { "type": "array", "items": { "type": "string", "pattern": "^[a-z0-9-]+$" } },
    "related_threads": { "type": "array", "items": { "type": "string" } },
    "related_tasks": { "type": "array", "items": { "type": "string" } },
    "related_shards": { "type": "array", "items": { "type": "string" } },
    "related_memory": { "type": "array", "items": { "type": "string", "pattern": "^memory/" } },
    "expires": { "type": "string" }
  },
  "allOf": [
    {
      "if": { "properties": { "kind": { "const": "decision" } } },
      "then": { "properties": { "status": { "enum": ["settled", "obsolete"] } } }
    },
    {
      "if": { "properties": { "kind": { "const": "pending" } } },
      "then": { "properties": { "status": { "enum": ["open", "resolved"] } } }
    }
    // ... otros condicionales por kind
  ]
}
```

## Validación al escribir

Toda creación o update vía `memory.note` / `memory.update` pasa por:
1. Parse frontmatter YAML.
2. Validar contra JSON Schema.
3. Verificar referencias (`related_threads`, `related_tasks` deben existir; si no, warning no error).
4. Calcular nuevo `updated_at`.
5. Commit en git del profile.

Si falla validación → error tipado con campo y mensaje claro; nada se persiste.

## Ejemplo completo

```yaml
---
id: memory/decisions/2026-05-26-tauri-out
title: Descartar Tauri para v1
kind: decision
status: settled
created_at: 2026-05-26T11:20:00Z
updated_at: 2026-05-26T11:20:00Z
created_by: user
tags: [architecture, frontend, tauri, web-ui]
related_threads: [01HX8E1RAA...]
related_tasks: []
related_shards: [build-plan/decisions-locked, agents/overview]
related_memory: []
expires: never
---

# Descartar Tauri para v1

**Razón**: el usuario priorizó WEB UI con acceso remoto sobre native shell.
Tauri cerraba puertas (no acceso desde otra máquina, deploy por OS, no self-host).

**Decisión**: F0–F6 sin Tauri. WEB UI con PWA install cubre 80% de "feel" nativo.
Tauri como opción aditiva post-F6 si surge demanda real.

**Alternativas consideradas**:
- Tauri ahora: rechazado por las razones de arriba.
- Web + Tauri en F7+: aceptado como camino futuro.

**Reabrir si**: regresión real de performance de xterm.js + WebGL2 que no se pueda
mitigar; o requirement nuevo de integración OS (system tray, native dialogs).
```

## Reglas duras

- `id` es **inmutable**; renombres usan tombstone (status=obsolete) + nueva entrada con `related_memory: [old_id]`.
- `updated_at` se toca en CADA edit; cliffhanger detectable si quedan iguales tras un update visible.
- Cuerpo ≤ 1.5 KB por default. Si más → partir en N entradas con `related_memory`.
- Sin secretos en cuerpo ni en frontmatter. Si necesitas referenciar credenciales, usa `{{secret:<ref>}}`.
