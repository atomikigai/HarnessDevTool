---
id: agents/rust-rails
title: Rust rails — funciones determinísticas para los agentes
shard: 13-agents
tags: [rails, rust, deterministic, tools, mcp]
summary: Catálogo de funciones Rust expuestas vía MCP que reducen alucinación del LLM.
related: [agents/overview, agents/smart-loading, agents/capability-registry, harness-core/tool-execution]
sources: []
---

# Rust rails

> **Filosofía**: el LLM no inventa, **elige de un menú** que Rust le ofrece. Cada función rail es:
> - **Determinística**: misma entrada = misma salida.
> - **Barata**: latencia sub-ms en mayoría.
> - **Validada**: input checks + output schema.
> - **Auditable**: cada call queda en `events.jsonl`.

## Por qué rails > LLM-todo

| Sin rails | Con rails |
|---|---|
| LLM alucina nombres de agentes | `agents.list()` devuelve set real |
| LLM inventa estructura de contrato | `contracts.validate()` aplica JSON Schema |
| LLM "estima" budget | `budget.remaining()` da el número |
| LLM asume archivos del repo | `repo.scan()` los enumera |
| LLM olvida deps entre tasks | `tasks.dependencies()` las lista |

Resultado: agentes **más precisos**, menos tokens en "explorar", menos retries.

## Catálogo completo (por familia)

### `agents.*`
| Tool | Args | Devuelve |
|---|---|---|
| `agents.list` | — | `[{ name, role, domain }]` de todos los agentes registrados |
| `agents.describe` | `name` | shard parseado: capabilities, default_spawn_hint, base prompt ref |
| `agents.match` | `task` | ranked: `[{ name, score, reason }]` que mejor encajan |
| `agents.validate_spawn` | `name, hint` | `{ ok, errors[] }` — verifica hint ⊆ capabilities |

### `tasks.*`
| Tool | Args | Devuelve |
|---|---|---|
| `tasks.list` | `thread, filters?` | tasks filtradas por status/labels |
| `tasks.get` | `id` | task completa |
| `tasks.create` | `body (Task)` | crea + valida schema + valida touches sin colisiones |
| `tasks.update` | `id, patch` | transición validada por state machine |
| `tasks.claim` | `id, agent, ttl` | adquiere lease o falla con holder current |
| `tasks.renew` | `id, agent` | extiende lease |
| `tasks.release` | `id, agent` | libera lease (graceful) |
| `tasks.submit` | `id, artifacts, contract_real` | pasa a `pending_verify` |
| `tasks.dependencies` | `id` | upstream + downstream del DAG |
| `tasks.touches_conflict` | `id` | `[task_id]` de tasks `in_progress` que tocan archivos solapados |
| `tasks.dep_graph` | `thread` | DAG completo (DOT) |
| `tasks.list_ready` | `thread` | tasks `queued` con `blocked_by∅` |

### `contracts.*`
| Tool | Args | Devuelve |
|---|---|---|
| `contracts.validate` | `task_id, json` | `{ ok, errors[] }` contra `contract_declared` |
| `contracts.diff` | `declared, real` | `none \| minor_extension \| minor_omission \| major` |
| `contracts.arbitrate_minor` | `task_id` | llama al arbitrator LLM, devuelve `elevate \| force_real` |

### `spec.*`
| Tool | Args | Devuelve |
|---|---|---|
| `spec.read` | `thread, scope?` | markdown completo o sección |
| `spec.append_section` | `thread, heading, body` | append-only |
| `spec.set_section` | `thread, slug, body` | edita sección específica (planner only) |
| `spec.toc` | `thread` | tabla de contenidos |

### `skills.*` (F5)
| Tool | Args | Devuelve |
|---|---|---|
| `skills.search` | `query, top_k, tags?` | FTS5 results (antes de F5 devuelve `[]`) |
| `skills.get` | `id` | skill MD + frontmatter |
| `skills.manage` | `action, id, body?, patch?` | create/patch/edit/archive |
| `skills.history` | `id` | git log entries de esa skill |

### `memory.*` (F5)
| Tool | Args | Devuelve |
|---|---|---|
| `memory.search` | `query, scope, top_k` | items relevantes de `events.jsonl` |
| `memory.get` | `item_id` | item completo |

### `repo.*`
| Tool | Args | Devuelve |
|---|---|---|
| `repo.analyze` | `path?` | stack, package manager, scripts, key files, git state, codebase-memory status |
| `repo.scan` | `path?, max_depth?, limit?` | árbol limitado de archivos del workspace |
| `repo.read_file` | `path, max_bytes?, head_lines?` | contenido truncable |
| `repo.git_status` | — | branch actual, tracking y cambios |
| `repo.git_log` | `path?, limit?` | últimos N commits |
| `repo.git_diff` | `path?, staged?, max_bytes?` | diff truncable |
| `repo.codebase_memory_status` | — | estado del acelerador opcional `codebase-memory-mcp` |

### `budget.*`
| Tool | Args | Devuelve |
|---|---|---|
| `budget.remaining` | `thread` | `{ usd_left, tokens_left, wallclock_left_s, turns_left }` |
| `budget.set_cap` | `thread, key, value` | (planner/humano) sube/baja caps |

### `mcps.*` y `capability.*`
| Tool | Args | Devuelve |
|---|---|---|
| `mcps.list_available` | — | catálogo de [[agents/capability-registry]] |
| `mcps.describe` | `name` | detalles del MCP |
| `capability.request` | `mcp?, skill?` | grant/deny según declared del agente |
| `capability.list_loaded` | — | qué está cargado ahora mismo |

### `runtime.*`
| Tool | Args | Devuelve |
|---|---|---|
| `runtime.now` | — | timestamp ISO 8601 |
| `runtime.profile_active` | — | nombre del profile |
| `runtime.health` | — | metrics del server (uptime, threads, etc.) |
| `policy.get_approval_rules` | — | reglas allow-and-remember |

## Ejemplo: orchestrator usando rails

```
LLM ─► repo.analyze()
Rust ◄─ { stack: ["rust", "svelte"], package_manager: "pnpm", key_files: [...] }

LLM ─► repo.scan(path="src", max_depth=3, limit=120)
Rust ◄─ { files: ["src/main.rs", "src/routes/+page.svelte", ...] }

LLM ─► agents.list()
Rust ◄─ [{ name: "frontend", ... }, { name: "backend", ... }, ...]

LLM ─► agents.match({ title: "Paginación en /orders", touches: [".svelte"] })
Rust ◄─ [{ name: "frontend", score: 0.92, reason: "domain match + file pattern" }]

LLM ─► budget.remaining(thread)
Rust ◄─ { usd_left: 9.83, ... }

LLM razona: "OK, frontend y backend. Creo dos tasks."

LLM ─► tasks.create({ id: "T-0042", domain: "frontend", touches: [...], 
                      spawn_hint: { mcp: ["harness-bridge"], skills: ["svelte"] },
                      contract_declared: { outputs: {...} } })
Rust ◄─ valida → ✓ → persiste → { ok: true }

LLM ─► tasks.create({ id: "T-0043", domain: "backend", ... })
Rust ◄─ ✓

LLM ─► tasks.create({ id: "T-0044", domain: "qa", blocked_by: ["T-0042","T-0043"], ... })
Rust ◄─ ✓
```

Nada se inventó. Todo se eligió desde menús con tipos.

## Implementación

Cada rail es:
1. Función Rust en `harness-core` o crate específico.
2. Expuesta como tool del `harness-bridge` MCP server (un solo wrapper).
3. Validación de input contra JSON Schema.
4. Output serializado vía `ts-rs` para frontend matching.

```rust
// harness-mcp-server/src/tools/agents.rs
async fn agents_list(_args: Value, ctx: Ctx) -> Result<Value, Error> {
    let agents = ctx.core.agents.list().await?;
    Ok(serde_json::to_value(agents)?)
}
```

## Métricas

Rails caen en spans `tracing`:
- `rail.invoke { name, latency_us, ok }` por call.
- Si una rail empieza a llamarse > 50 veces por turn → posible loop; warning.

## Anti-patrones

| Mal | Bien |
|---|---|
| Tools que devuelven texto libre | Tools que devuelven JSON tipado |
| Tools que mutan sin validar | Toda mutación valida schema antes |
| Tools async lentas (> 100ms p99) | Rails son rápidas; lo lento va aparte |
| Tools que requieren conocer estructuras internas | API estable que oculta el storage |
| Rail que llama a un LLM | Rails son **determinísticas**; el arbitrator es la excepción explícita |
