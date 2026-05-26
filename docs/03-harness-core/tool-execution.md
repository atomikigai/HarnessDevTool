---
id: harness-core/tool-execution
title: Ejecución de tools
shard: 03-harness-core
tags: [tools, execution, futures-ordered, parallelism]
summary: Paralelismo con orden preservado, cancelación y aprobación previa.
related: [harness-core/sandbox, harness-core/approval-flow, harness-core/mcp-integration]
sources: [foundations/openai-codex-architecture]
---

# Tool execution

## Trait core

```rust
#[async_trait::async_trait]
pub trait HarnessTool: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;     // JSON schema para el modelo
    fn requires_approval(&self, args: &Value) -> bool;
    async fn execute(
        &self,
        args: Value,
        ctx: ToolCtx<'_>,
    ) -> Result<ToolOutput, ToolError>;
}
```

`ToolCtx` da: sandbox handle, cancellation token, event sink (para streaming intermedio), thread/turn ids.

## Paralelismo con orden

Cuando el modelo emite K tool calls en un mismo response:

```rust
use futures::stream::{FuturesOrdered, StreamExt};

let mut fo = FuturesOrdered::new();
for tc in tool_calls {
    fo.push_back(self.run_one(tc));   // arranca todas en paralelo
}
let mut results = Vec::with_capacity(fo.len());
while let Some(r) = fo.next().await { results.push(r?); }
// `results` está en el mismo orden que `tool_calls`
```

Esto es el patrón de Codex y es crítico: el orden devuelto debe coincidir con el orden pedido para no confundir al modelo.

## Aprobación
Antes de ejecutar:
- Si `tool.requires_approval(args)` y `cfg.approval_mode != "auto"` → emitir `approval.request` y suspender.
- El cliente devuelve `approval.respond { allow | deny }`.
- Timeout configurable → error `-32002`.

Ver [[harness-core/approval-flow]].

## Cancelación
- Cancelar el turn → cancela el `CancellationToken` del ctx.
- Tools deben respetar el token (chequeos en bucles, `select!` con `token.cancelled()`).
- Si no respetan, el harness `abort()` la task tras grace period.

## Errores
`ToolError` se serializa al modelo como mensaje claro pero no contiene secretos. La UI también lo ve. Stack traces internos quedan en logs.

## Tools nativas vs MCP vs módulos
- **Nativas** (shell, edit, read): sandboxed. Ver [[harness-core/sandbox]].
- **MCP**: child process con stdio propio. El sandbox lo aplica el server MCP.
- **Módulos** (`module-db`, `module-ssh`): exponen tools vía `HarnessTool`. Sandboxing decidido por el módulo (DB con permisos read-only, SSH con allowlist de hosts, etc.).
