---
id: app-server/jsonrpc-transport
title: "[Tombstone] JSON-RPC transport"
shard: 04-app-server
tags: [tombstone, deprecated]
summary: Obsoleto. El transport hoy es HTTP+SSE de Axum, no JSON-RPC stdio.
related: [architecture/ipc-protocol, app-server/overview]
sources: []
---

# [Tombstone] JSON-RPC transport

> El modelo original copiaba a Codex (JSON-RPC bidireccional sobre stdio). El pivote a **WEB UI primary** cambió esto a **HTTP + SSE** directamente con Axum.

## Estado actual

- **HTTP REST** para CRUD en `/api/*`.
- **SSE** para streaming en `/api/events`.
- JSON con tipos generados por `ts-rs`.
- CORS habilitado para `http://localhost:8080`.

Para el MCP **interno** (entre `harness-server` y los CLIs hijos `claude`/`codex`), sí se usa JSON-RPC stdio — pero ese es **otro canal**, no el cliente del browser.

## Ver en su lugar

- [[architecture/ipc-protocol]] — HTTP + SSE actual
- [[app-server/overview]] — el `harness-server` Axum
- [[agents/spawn-lifecycle]] — MCP stdio interno hacia el CLI hijo
