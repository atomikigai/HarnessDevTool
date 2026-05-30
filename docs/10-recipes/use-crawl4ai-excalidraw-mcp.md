---
id: recipes/use-crawl4ai-excalidraw-mcp
title: Usar Crawl4AI y Excalidraw MCP
shard: 10-recipes
tags: [recipe, mcp, skills, crawl4ai, excalidraw, docker]
summary: Levantar servicios MCP opcionales para extracción web y tableros Excalidraw.
related: [recipes/add-mcp-server, harness-core/mcp-integration, agents/capability-registry]
sources:
  - https://github.com/unclecode/crawl4ai
  - https://docs.crawl4ai.com/core/self-hosting/
  - https://github.com/excalidraw/excalidraw-mcp
---

# Usar Crawl4AI y Excalidraw MCP

Este repo incluye dos skills bundled y un compose opcional para MCPs externos:

- `skills/bundled/crawl4ai-context` — extraer contexto/documentación de páginas web.
- `skills/bundled/excalidraw-board` — crear y editar diagramas en Excalidraw.

## Levantar servicios

```bash
just mcp-up
```

Equivalente:

```bash
docker compose -f docker-compose.mcp.yml up -d --build
```

## Endpoints

| Servicio | URL |
| --- | --- |
| Crawl4AI dashboard | `http://localhost:11235/dashboard` |
| Crawl4AI playground | `http://localhost:11235/playground` |
| Crawl4AI MCP SSE | `http://localhost:11235/mcp/sse` |
| Crawl4AI MCP schema | `http://localhost:11235/mcp/schema` |
| Excalidraw MCP | `http://localhost:3001/mcp` |

## Config MCP sugerida

Para clientes stdio-only, Crawl4AI puede ir por `mcp-remote`:

```json
{
  "mcpServers": {
    "crawl4ai": {
      "command": "npx",
      "args": ["-y", "mcp-remote", "http://localhost:11235/mcp/sse"]
    }
  }
}
```

Excalidraw MCP usa transporte HTTP streamable:

```json
{
  "mcpServers": {
    "excalidraw": {
      "type": "http",
      "url": "http://localhost:3001/mcp"
    }
  }
}
```

El harness debe filtrar el acceso a estos MCPs por capability policy antes de
exponerlos a subagentes. Por defecto:

- `crawl4ai`: permitido para roles que necesiten contexto/documentación externa.
- `excalidraw`: permitido para planner/orchestrator, architect/frontend y roles
  de documentación.

## Apagar servicios

```bash
just mcp-down
```

