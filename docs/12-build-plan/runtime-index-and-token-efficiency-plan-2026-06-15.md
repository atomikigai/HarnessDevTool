---
id: build-plan/runtime-index-and-token-efficiency-plan-2026-06-15
title: Plan runtime indexes y eficiencia de agentes
shard: 12-build-plan
tags: [plan, sqlite, fts5, runtime, memory, context, agents, performance, token-efficiency]
summary: Plan ejecutable para convertir IO repetitivo y contexto crudo en indices SQLite/FTS5 y rails Rust que reduzcan latencia, tokens y relectura de transcript.
related: [build-plan/super-harness-plan-2026-06-12, build-plan/pending-implementation-tasks, agents/rust-rails, agents/smart-loading, memory/search-and-index]
sources: []
---

# Plan runtime indexes y eficiencia de agentes

Fecha: 2026-06-15.

Objetivo: hacer que el harness sea mas rapido y que los agentes gasten menos
tokens usando Rust para producir vistas compactas, deterministicas y
consultables. La fuente de verdad sigue siendo append-only/TOML/Markdown; SQLite
es un indice derivado, reconstruible y cacheable.

## Principio base

No reemplazar la verdad canonica:

- `events.jsonl`, `transcript.jsonl`, handoffs JSONL, tasks TOML y memoria
  Markdown siguen siendo auditables y append-only donde aplique.
- SQLite/FTS5 materializa vistas para consulta rapida: ultimos eventos,
  busqueda, summaries, ledgers y paquetes de contexto.
- Si un indice se borra o corrompe, el harness debe reconstruirlo desde la
  fuente canonica.

## Inspiracion de `codebase-memory-mcp`

El repo `codebase-memory-mcp` aporta ideas utiles para el runtime, pero no se
debe copiar su base C ni vendorizar su stack dentro de Harness. La division
correcta es:

- Harness Rust mantiene la memoria operacional: agentes, objetivos, handoffs,
  contexto, transcript, timeline, tasks y smart loading.
- `codebase-memory-mcp` puede ser un acelerador opcional para grafo de codigo:
  simbolos, llamadas, rutas, arquitectura, impacto y snippets.
- Las ideas reutilizables son de diseno: indices derivados, SQLite/FTS5,
  incrementalidad por hashes, outputs paginados, hints, modos de indexado y
  graph search compacto.

Patrones a incorporar en Harness:

- Indices derivados reconstruibles, nunca canonicos.
- `fast | moderate | full` como modos de trabajo para consultas caras.
- `total`, `has_more`, `limit`, `offset` y `hints` en tools con listas.
- FTS5 con normalizacion de identificadores `camelCase`/`snake_case`.
- Watcher basado en git/HEAD/dirty state para invalidacion gruesa.
- Bulk writes con transacciones, indices recreables y `PRAGMA optimize`.
- Tool workflow compacto: buscar grafo -> pedir snippet -> pedir impacto.

Patrones a evitar en Harness:

- Portar el parser/indexador C al runtime Rust.
- Duplicar la base completa del grafo de codigo.
- Hacer embeddings/vector search antes de medir BM25/FTS5 + grafo.
- Arrancar un MCP pesado por cada tool call si puede mantenerse vivo.

## Por que Rust rails para IO

Conviene mover IO repetitivo a funciones Rust cuando:

- El agente tendria que leer archivos grandes para extraer 5-20 datos.
- La misma consulta ocurre muchas veces: transcript replay, timeline, latest
  handoff, tasks list, context search, skills search.
- Hay que filtrar, ordenar, paginar o truncar de forma estable.
- El resultado puede ser un JSON compacto en vez de texto crudo.
- Queremos cache, limites, validacion y permisos consistentes.

No conviene moverlo cuando:

- Es una lectura unica de un archivo pequeno.
- El agente necesita razonar sobre el contenido completo.
- La logica cambia a cada tarea y no hay patron estable.
- El costo de mantener un indice supera el ahorro.

Regla practica: si el LLM va a hacer "abrir mucho, buscar poco, resumir y
descartar", debe existir una rail Rust.

## Workstream R1 — Agent ledger y context packs

Objetivo: que Zeus/orchestrator recuerde objetivos, estado y next action de
subagentes sin releer transcripts.

### Cambios

1. Crear `agent_ledger.sqlite` por profile.
2. Materializar por session:
   - `session_id`, `parent_session_id`, `root_session_id`
   - `thread_id`, `task_id`, `role`, `scopes`
   - `objective`, `status`, `detected_state`
   - `loaded_capabilities`
   - `latest_pressure`, `latest_checkpoint_seq`
   - `latest_handoff_at`, `latest_handoff_status`
   - `next_action`, `blocked_on`, `files_changed`, `commands_run`
3. Exponer rails MCP:
   - `session_context_pack`
   - `agent_ledger_list`
   - `agent_ledger_get`
   - `handoff_latest`
   - `session_handoff_submit`

### Criterios de aceptacion

- `session_read_child_summary` o su reemplazo devuelve handoff estructurado,
  no solo metadata.
- Orchestrator puede listar hijos y ver objetivo/next action/bloqueos en una
  llamada compacta.
- Handoff submit persiste append-only y actualiza indice derivado.
- Tests cubren reconstruccion del ledger desde meta + handoffs + context events.

## Workstream R2 — Context index incremental

Objetivo: eliminar reindex on-demand de eventos de contexto.

### Cambios

1. Agregar tabla `index_offsets(thread_id, last_seq)`.
2. Indexar `session.context.*` incrementalmente al append o desde watcher.
3. Mantener `context.sqlite` como indice derivado por profile.
4. Exponer rail MCP:
   - `context_status`
   - `context_search`
   - `context_checkpoint_request`

### Criterios de aceptacion

- Buscar contexto no relee todo `events.jsonl`.
- Si `context.sqlite` falta, se reconstruye una vez y guarda offset.
- `GET /api/sessions/:sid/context/search` mantiene comportamiento compatible.
- Tests con thread largo verifican que segunda busqueda no reindexa todo.

## Workstream R3 — Timeline/event index

Objetivo: paginar timeline y depurar eventos sin cargar el log completo.

### Cambios

1. Crear `events_index.sqlite` por profile o por thread.
2. Indexar:
   - `thread_id`, `seq`, `at`, `event_type`, `actor`
   - `session_id`, `task_id` extraidos del payload cuando existan
   - `summary`, `payload_json`
3. Exponer rail/API:
   - `timeline_query({ thread_id, after, limit, event_type?, actor?, task_id?, session_id? })`
   - `timeline_search({ q, thread_id?, limit? })` con FTS5 opcional.

### Criterios de aceptacion

- `/api/threads/:id/timeline?after=&limit=` no llama `read_timeline` completo.
- Query por task/session/event_type es O(log n + page).
- Indice es reconstruible desde `events.jsonl`.

## Workstream R4 — Task index extendido

Objetivo: evitar leer todos los TOML para vistas y planning.

### Cambios

1. Expandir `tasks/index.db` con campos resumidos:
   - `title`, `status`, `assignee`, `updated_at`
   - `labels`, `blocked_by`, `depends_on`
   - `acceptance_count`, `artifact_count`
   - `latest_handoff_status`, `latest_handoff_at`
   - `summary_preview`
2. Agregar metodos:
   - `TaskStore::list_summaries`
   - `TaskStore::latest_active_summary`
   - `TaskStore::ready_queue`
3. Exponer MCP:
   - `task_list_summary`
   - `task_next_best`

### Criterios de aceptacion

- Listar tasks para dashboard/scheduler no relee cada TOML.
- `task_list` completo sigue disponible cuando el agente necesita detalles.
- Scheduler usa summaries donde no necesita task completo.

## Workstream R5 — Transcript index y compact replay

Objetivo: replay y busqueda de transcript sin leer todo el JSONL.

### Cambios

1. Crear `transcript_index.sqlite` por session o profile.
2. Indexar al `TranscriptStore::ingest`:
   - `session_id`, `seq`, `at`, `source`, `kind`, `role`
   - `content_preview`, `content_text` para FTS, `payload_json`
   - tool call/result ids cuando existan.
3. Exponer:
   - `transcript_query({ session_id, since, limit, kind?, role? })`
   - `transcript_search({ session_id, q, limit })`
   - `transcript_tool_results({ session_id, tool_name?, limit })`

### Criterios de aceptacion

- SSE reconnect con `since` usa query por seq.
- Busqueda de errores/tool results no lee todo transcript.
- JSONL sigue siendo canonico y exportable.

## Workstream R6 — Skills/memory FTS real en MCP

Objetivo: cerrar la brecha entre los docs de `memory.*` y las rails realmente
disponibles.

### Cambios

1. Crear/activar indice `memory_fts` por profile:
   - entries Markdown, skills, decisions, pending, in-flight, facts.
2. Exponer MCP:
   - `memory_search`
   - `memory_read`
   - `memory_continuity`
   - `memory_note_propose`
3. Indexar skills con usage counts:
   - `skills_search` puede rankear por relevancia + uso + freshness.

### Criterios de aceptacion

- El agente recupera memoria on-demand en vez de recibir dumps.
- `memory_note_propose` no escribe memoria libre sin approval.
- Search soporta top-K, kind/status/tags y snippets.

## Workstream R7 — Repo manifest y symbol index

Objetivo: reducir exploracion de repo y lecturas de archivos grandes.

### Cambios

1. Crear cache por repo/head/mtime:
   - file manifest: path, extension, size, mtime, git status
   - important files: package manifests, routes, configs, tests
2. Agregar symbol index ligero con `ast-grep` o `tree-sitter`:
   - Rust funcs/types/modules
   - Svelte components/routes
   - TS exports
3. Exponer MCP:
   - `repo_manifest`
   - `repo_symbol_search`
   - `repo_related_files`

### Criterios de aceptacion

- `repo_find` puede usar cache cuando esta fresco.
- El agente encuentra funciones/componentes sin abrir todo el archivo.
- Invalida por git HEAD o mtime.

## Workstream R9 — Code graph MCP adapter

Objetivo: aprovechar `codebase-memory-mcp` como acelerador opcional sin acoplar
Harness a su implementacion interna.

### Cambios

1. Agregar capability/tool group `code_graph`, cargado solo bajo smart loading.
2. Detectar disponibilidad de `codebase-memory-mcp` y estado del indice.
3. Mantener un upstream MCP persistente por profile/repo con idle timeout.
   El gateway actual one-shot sirve para MCPs chicos, pero para grafo de codigo
   desperdicia tiempo en spawn, handshake y apertura de SQLite por request.
4. Exponer wrappers Harness con contratos compactos:
   - `repo_code_graph_status`
   - `repo_code_graph_index`
   - `repo_symbol_search`
   - `repo_related_files`
   - `repo_change_impact`
   - `repo_architecture_pack`
   - `repo_code_snippet`
5. Guardar en SQLite propio solo cache/resumen pequeno:
   - repo root, head, dirty hash, index status
   - ultima arquitectura compacta
   - ultimas consultas y simbolos relevantes
   - impactos por archivo para el task activo

### Smart loading

`code_graph` se carga cuando la task menciona arquitectura, simbolos,
callers/callees, impacto, rutas HTTP, refactor amplio o investigacion de repo.
No se carga para preguntas generales, docs simples, UI-only acotado o comandos
de git basicos.

### Criterios de aceptacion

- `planning_pack` recomienda `code_graph` solo cuando la consulta lo amerita.
- `resolve_smart_tool_groups` activa `code_graph` solo ante senales de
  arquitectura, simbolos, impacto o grafo.
- Si `codebase-memory-mcp` esta instalado, el config MCP por sesion lo monta
  como upstream persistente con idle timeout; si no, no se carga upstream.
- Un repo sin `codebase-memory-mcp` instalado degrada a `repo_manifest`,
  `repo_find` y `repo_symbol_search` ligero.
- Las respuestas devuelven JSON compacto con limites, no dumps de grafo.
- El proceso upstream se reutiliza entre calls y se cierra por idle timeout.
- Indexar o consultar grafo registra spans con latencia, bytes/tokens estimados
  y si el resultado vino de cache.

## Workstream R8 — Evidence pack para review/QA

Objetivo: que reviewers y QA reciban evidencia compacta y verificable.

### Cambios

1. Crear rail:
   - `evidence_pack({ task_id?, session_id?, paths? })`
2. Recolectar:
   - diff resumido
   - archivos modificados
   - comandos corridos
   - resultados de tests
   - screenshots/artifacts
   - riesgos y checks no ejecutados
3. Integrar con `session_handoff_submit`.

### Criterios de aceptacion

- Un reviewer puede iniciar con un JSON pequeno de evidencia.
- QA no necesita releer todo el transcript para saber que validar.

## Orden recomendado

1. R1: `agent_ledger` + `session_context_pack`.
2. R2: context index incremental.
3. R4: task summaries en SQLite.
4. R3: timeline/event index.
5. R5: transcript index.
6. R6: memory/skills FTS real.
7. R7: repo manifest/symbol index ligero.
8. R9: code graph MCP adapter persistente.
9. R8: evidence pack.

R1 es primero porque mejora directamente la coordinacion de subagentes y
reduce la necesidad de releer conversaciones. R2/R4 son los mayores quick wins
de rendimiento backend. R5/R6/R7 multiplican el ahorro de tokens. R9 debe
venir despues del indice repo ligero para tener fallback local y no depender
del MCP externo.

## Herramientas/runtime recomendados

- SQLite WAL + FTS5: indice local principal.
- `rusqlite`: indices derivados simples y sincronicos controlados.
- `sqlx`: seguir usandolo para conexiones DB externas donde ya aporta async.
- `notify`: invalidacion de caches por cambios de archivos.
- `ast-grep` o `tree-sitter`: indice simbolico de repo.
- `codebase-memory-mcp`: acelerador opcional de grafo de codigo via MCP
  persistente, no dependencia obligatoria.
- `difftastic`: semantic diff para evidence/review.
- `tantivy`: considerar solo si FTS5 queda corto.
- Embeddings/vector search: diferir hasta tener BM25/FTS5 bien instrumentado.

## Métricas

Medir antes/despues por endpoint/tool:

- p50/p95 latency.
- bytes leidos de disco por request.
- records JSONL parseados por request.
- tokens enviados al agente por respuesta.
- numero de tool calls necesarias para retomar una tarea.
- porcentaje de spawns que cargan capabilities no usadas.

## Riesgos

- Indices desincronizados: mitigar con offsets, rebuild y tests de recovery.
- Duplicar fuente de verdad: SQLite nunca debe ser canonico.
- Over-indexing temprano: empezar por ledger/context/tasks antes de symbols.
- Token savings falsos: una rail rapida que devuelve demasiado texto sigue
  siendo cara para el agente; todos los outputs deben tener summary/limit.
- MCP pesado one-shot: mitigar con proceso persistente por repo/profile e idle
  timeout antes de depender de grafo de codigo para flujos frecuentes.
