---
id: recipes/add-mcp-server
title: Receta — conectar un MCP server
shard: 10-recipes
tags: [recipe, mcp, howto]
summary: Añadir un servidor MCP (stdio o HTTP) y exponerlo al agente.
related: [harness-core/mcp-integration, cross-cutting/security-model]
sources: []
---

# Conectar un MCP server

## Stdio (server local)
Editar `~/.harness/config.toml`:
```toml
[[mcp.servers]]
name = "playwright"
transport = "stdio"
command = "npx"
args = ["@modelcontextprotocol/server-playwright"]

# opcional
env = { PLAYWRIGHT_BROWSERS_PATH = "/path/to/browsers" }
enabled = true
allowed_tools = ["browser.navigate", "browser.click", "browser.screenshot"]
# si vacío, todas las tools del server se exponen
```

## HTTP (server remoto)
```toml
[[mcp.servers]]
name = "linear"
transport = "http"
url = "https://mcp.linear.app"
auth = { kind = "oauth", token_ref = "keyring:linear" }
```

Auth `kind`:
- `none`
- `bearer { token_ref = "keyring:<name>" }`
- `oauth { token_ref = "keyring:<name>" }`
- `header { name = "X-API-Key", value_ref = "keyring:<name>" }`

## Verificar

```
harness mcp list                   # lista servers + estado conexión
harness mcp tools <server>         # lista tools expuestas
harness mcp call <server>.<tool> --args '{"...":"..."}'   # smoke test
```

## Sandbox del MCP local
Para mitigar trust boundary, ejecuta el child bajo sandbox del SO:
```toml
[[mcp.servers]]
name = "playwright"
transport = "stdio"
command = "npx"
args = ["@modelcontextprotocol/server-playwright"]
sandbox = "workspace-net"          # opcional
```

## Approval por defecto
Tools de MCP arrancan con `requires_approval=true`. Tras primer "allow-and-remember", el usuario puede automatizarlas.

## Troubleshooting
- Server no arranca: log en `~/.harness/logs/mcp-<name>.log`.
- Handshake falla: verificar versión del MCP server compatible.
- Reconexión: el cliente reintenta con backoff; `harness mcp restart <name>` fuerza.
