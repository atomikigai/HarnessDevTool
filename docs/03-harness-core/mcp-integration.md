---
id: harness-core/mcp-integration
title: Integración MCP
shard: 03-harness-core
tags: [mcp, integration, external-tools]
summary: Cliente MCP que conecta a servers externos por stdio o HTTP.
related: [harness-core/tool-execution, recipes/add-mcp-server]
sources: []
---

# MCP (Model Context Protocol)

## Qué es
Un protocolo abierto para que herramientas externas expongan tools al harness. Cada server MCP es un proceso independiente que habla JSON-RPC.

## Cliente en `harness-mcp`

```rust
pub struct McpClient {
    transport: McpTransport,        // Stdio { child } | Http { url, auth }
    capabilities: McpCapabilities,
    tools: Vec<ToolDefinition>,
}

impl McpClient {
    pub async fn connect(spec: McpServerSpec) -> Result<Self> { /* ... */ }
    pub async fn list_tools(&self) -> Result<Vec<ToolDefinition>> { /* ... */ }
    pub async fn call(&self, name: &str, args: Value) -> Result<Value> { /* ... */ }
}
```

## Registro
Servers MCP se declaran en `~/.harness/config.toml`:

```toml
[[mcp.servers]]
name = "playwright"
transport = "stdio"
command = "npx"
args = ["@modelcontextprotocol/server-playwright"]

[[mcp.servers]]
name = "linear"
transport = "http"
url = "https://mcp.linear.app"
auth = { kind = "oauth", token_ref = "linear" }
```

## Sandboxing
**MCP servers son trust boundary**: el harness no puede inspeccionar su código. Recomendaciones:
- Validar `args` enviados (el modelo podría exfiltrar datos).
- Limitar qué tools del MCP están habilitadas por defecto.
- Ejecutar el child process MCP bajo sandbox del SO si es local.

## Lifecycle
- Conectar al iniciar el thread → handshake → cachear `tools`.
- Reusar conexión durante la sesión.
- Reconnect con backoff exponencial si cae.

## Exposición al modelo
Las tools de MCP se mezclan con las nativas en `tool_definitions`, prefijadas con el server: `playwright.click`, `linear.create_issue`. Esto evita choques de nombres.

## Ver también
[[recipes/add-mcp-server]] — guía paso a paso.
