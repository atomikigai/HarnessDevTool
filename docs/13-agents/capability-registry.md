---
id: agents/capability-registry
title: Catálogo de capabilities (MCPs, skill-tags, tools)
shard: 13-agents
tags: [capabilities, mcp, skills, tools, catalog]
summary: Fuente única de MCPs disponibles, skill-tags válidos y tools del harness.
related: [agents/smart-loading, harness-core/mcp-integration, harness-skills]
sources: []
---

# Catálogo de capabilities

> Este shard es el **catálogo canónico**. Si un agente declara un MCP/skill-tag/tool que no está aquí → error de validación al cargar el shard.

## MCPs disponibles

| Nombre | Transport | Provee | Default load | Notas |
|---|---|---|---|---|
| **harness-bridge** | stdio (internal) | `task.*`, `spec.*`, `skills.*`, `capability.*`, `memory.*` | **Siempre** | Es el bridge nuestro; sin esto el agente no puede operar. |
| **context7** | stdio (npm) | `docs.search`, `docs.read` | On-demand | Búsqueda de docs de libs/frameworks. |
| **playwright** | stdio (npm) | `browser.*`, `e2e.*` | On-demand | Automatización de browser; E2E tests. |
| **fetch** | stdio (npm) | `http.get`, `http.post` (allow-listed) | On-demand | Llamadas HTTP simples; sandboxed allowlist. |
| **github** | http (oauth) | `gh.repo.*`, `gh.pr.*`, `gh.issue.*` | On-demand | F4+; auth con token del usuario. |
| **filesystem** | stdio (npm) | `fs.read`, `fs.write`, `fs.tree` | Mediado | En desuso: usamos `shell.exec` sandboxed para FS. |

### Reglas
- `harness-bridge` siempre cargado para todos los agentes.
- MCPs externos requieren validación al instalar el harness; configurados en `~/.harness/config.toml`.
- Cualquier MCP local corre bajo sandbox del SO (ver [[cross-cutting/security-model]]).

## Skill-tags

Los tags son **claves de búsqueda** del corpus de skills. Una skill puede tener varios tags. El catálogo es **abierto** (el learner crea tags nuevos), pero hay un conjunto canónico.

### Dominio: frontend
| Tag | Cubre |
|---|---|
| `svelte` | SvelteKit, Svelte 5, stores, runes |
| `tailwind` | Tailwind v4, tokens, utility patterns |
| `shadcn` | shadcn-svelte components |
| `frontend-design` | layout, spacing, color, typography |
| `a11y` | accessibility (ARIA, semantic HTML) |
| `forms` | form validation con valibot |
| `xterm` | xterm.js, ANSI rendering |
| `codemirror` | CodeMirror 6 setup |

### Dominio: backend
| Tag | Cubre |
|---|---|
| `rust-patterns` | idioms Rust modernos, `?`, `Result`, owners |
| `axum` | handlers, extractors, state, middleware |
| `tokio` | async, spawn, select |
| `tracing` | spans, structured logs |
| `sqlx` | queries, pool, migrations |
| `serde` | (de)serialization, custom impl |
| `ts-rs` | binding generation |
| `error-modeling` | thiserror, anyhow patterns |

### Dominio: database
| Tag | Cubre |
|---|---|
| `sql` | DDL/DML genérico |
| `sqlite` | quirks SQLite (WAL, journaling) |
| `postgres` | queries Postgres, EXPLAIN, índices |
| `migrations` | versionado de schema |
| `query-perf` | optimización de queries |

### Dominio: devops
| Tag | Cubre |
|---|---|
| `docker` | Dockerfile multi-stage, layer cache |
| `compose` | docker-compose, networks, volumes |
| `ci` | GitHub Actions, caching |
| `release` | tagging, semver, changelog |
| `nginx` | configs, proxy_pass, SSE |

### Dominio: qa
| Tag | Cubre |
|---|---|
| `unit-tests` | tests unitarios (cargo test, vitest) |
| `integration-tests` | tests cross-service |
| `e2e-tests` | Playwright, escenarios end-to-end |
| `assertions` | patrones de assertion (msg claros) |
| `mocking` | wiremock, msw |

### Cross-cutting
| Tag | Cubre |
|---|---|
| `git` | flujos, conflict resolution |
| `markdown` | docs editing, frontmatter |
| `security` | auth, secrets, sandboxing |
| `performance` | profiling, optimization |
| `refactor` | técnicas de refactor seguro |

## Tools del harness-bridge (siempre disponibles)

Estas son las MCP tools que expone `harness-bridge`. Patrón de namespace:

| Namespace | Tools | Descripción |
|---|---|---|
| `task.*` | `list`, `get`, `claim`, `renew`, `update`, `release`, `submit` | Operaciones sobre tasks |
| `spec.*` | `read`, `append_section`, `set_section` | Mantenimiento de spec.md |
| `skills.*` | `search`, `get`, `manage` | F5; antes de F5 devuelven `[]` |
| `capability.*` | `request`, `list_loaded` | Solicitar/listar capabilities en runtime |
| `memory.*` | `search`, `get` | F5; FTS5 sobre events.jsonl |
| `repo.*` | `scan`, `read_file`, `git_log`, `git_diff` | Read-only del workspace |
| `budget.*` | `remaining` | Solo lectura |
| `agents.*` | `list`, `describe` | Para el orchestrator |
| `mcps.*` | `list_available`, `describe` | Inspección del catálogo |
| `policy.*` | `get_approval_rules` | Lectura de policy |
| `runtime.*` | `now`, `profile_active`, `health` | Utilidades |

Tools fuera del harness-bridge (provistas por otros MCPs o por el CLI):
- `shell.exec` (provista por el CLI mismo bajo sandbox).
- `browser.*` (provista por playwright MCP cuando cargado).
- `docs.*` (provista por context7 MCP cuando cargado).

## Validación

Al cargar un shard de agente:
1. Cada item en `capabilities.mcp_available` debe estar en este catálogo.
2. Cada item en `capabilities.skill_tags` debe estar en este catálogo (excepto tags creados por el learner que se auto-registran).
3. Cada glob en `capabilities.tools_allowed` debe matchear al menos una tool del catálogo.

Drift = build break, no warning silencioso.

## Cómo extender el catálogo

1. Añadir entry aquí con descripción.
2. Si es un MCP nuevo: documentar instalación en [[recipes/add-mcp-server]].
3. Si es un skill-tag nuevo: documentar (mínimo descripción de 1 línea).
4. Si es una tool nueva: añadir en el handler del namespace correspondiente del `harness-bridge`.
5. Bump del `protocol_version` si el cambio rompe agentes viejos.
