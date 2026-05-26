---
id: harness-core/tool-execution
title: Tools que el harness EXPONE (no ejecuta)
shard: 03-harness-core
tags: [tools, mcp, server, rails]
summary: Las tools del harness-bridge son rails Rust expuestas vía MCP al CLI hijo.
related: [agents/rust-rails, agents/capability-registry, harness-core/mcp-integration]
sources: []
---

# Tools del harness

> Cambio: en el modelo Codex original, "tool execution" era nosotros ejecutando código (shell, edit, read). **Ahora**: las tools son **rails Rust** que el CLI hijo consume vía MCP.

## Las dos categorías

| Categoría | Quién ejecuta | Ejemplo |
|---|---|---|
| **Rails del harness** | `harness-mcp-server` (Rust) | `task.claim`, `memory.search`, `repo.scan` |
| **Tools del CLI** | `claude`/`codex` mismos (shell, edit, browser) | `shell.exec`, `str_replace`, `web_search` |

El harness **no ejecuta** shell ni edita archivos. Eso lo hace el CLI hijo internamente (con su propio sandbox/approval). Nosotros **proveemos rails determinísticas** para el meta-trabajo (tasks, memoria, skills, repo introspection).

## Trait core (Rust)

```rust
#[async_trait::async_trait]
pub trait HarnessTool: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;     // JSON schema para el CLI
    fn requires_approval(&self, args: &Value) -> bool;
    async fn execute(
        &self,
        args: Value,
        ctx: ToolCtx<'_>,
    ) -> Result<ToolOutput, ToolError>;
}
```

`ToolCtx` da: thread_id, spawn_id, agent_id, cancellation token, event sink.

Tools registradas en `harness-mcp-server` vía `inventory` o macro `#[harness_tool]`.

## Catálogo

Ver [[agents/rust-rails]] para el listado completo organizado por familia (`agents.*`, `tasks.*`, `memory.*`, `skills.*`, `repo.*`, `budget.*`, `contracts.*`, etc.).

## Aprobación

Algunas tools requieren approval del humano:
- `memory.note`, `memory.update` (siempre, salvo el orchestrator)
- `skills.manage(action="create"|"edit")` antes de F5 LLM review
- `contracts.elevate_declared` (sensible)
- Cualquier tool con `requires_approval(args) == true`

Mecanismo: ver [[harness-core/approval-flow]].

## Paralelismo

El CLI hijo decide internamente si llama tools en paralelo. Nuestro server:
- Atiende calls concurrentes (cada una en su propia task tokio).
- Garantiza consistencia con locks (e.g., `task.claim` usa flock).
- Devuelve resultados en el orden que el CLI los pidió (correlación por `id` JSON-RPC).

## Cancelación

- Si el spawn muere → MCP conn se cierra → tools in-flight reciben cancellation.
- Si el humano cancela el thread → cancellation token cae → tools liberan recursos.

## Tools que NUNCA exponemos

Por scope/seguridad:
- Mutación de profiles, USER.md global, config global (eso es del humano).
- Modificación directa de `events.jsonl` (es append-only del backend).
- Cambio de budget caps en runtime (decisión humana, no automatizable).
- Operaciones de red no whitelistadas.

Si el CLI necesita una capacidad ausente, usa `capability.request` (ver [[agents/smart-loading]] §"Nivel 3").

## Anti-patrones

| Mal | Bien |
|---|---|
| Reimplementar shell.exec en el harness | El CLI ya lo tiene; nosotros solo rails |
| Tools que devuelven texto libre | Tools que devuelven JSON tipado |
| Tools que mutan sin validar schemas | Validación obligatoria pre-mutación |
| Rails async lentas (> 100ms p99) | Rails son rápidas; lo lento va aparte |
| Crear una rail por cada query DB del usuario | Las queries de aplicación son del CLI; rails son del **harness** |
