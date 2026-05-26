---
id: app-server/web-deployment
title: Despliegue web (HTTP + SSE)
shard: 04-app-server
tags: [web, sse, deployment]
summary: Wrap del JSON-RPC stdio en HTTP+SSE para clientes browser.
related: [app-server/jsonrpc-transport, app-server/overview]
sources: [foundations/openai-codex-architecture]
---

# Despliegue web

## Topología

```
Browser ── HTTPS ──► reverse proxy ── HTTP ──► web-gateway ── stdio ──► app-server (container worker)
                                                ▲ SSE responses          │
                                                └────────────────────────┘
```

## Endpoints
- `POST /rpc` — request body = JSON-RPC; respuesta = result/error.
- `GET /events?thread=<id>&cursor=<seq>` — SSE stream de notifications para ese thread.
- `POST /upload` — adjuntos grandes (no por JSON-RPC).

## El gateway
- Una conexión `stdio` al App Server por sesión del usuario.
- Mantiene un `cursor` por (thread, cliente).
- Al reconectar el browser, replayea eventos desde el cursor.

## Sesión sobrevive al tab
- El usuario cierra el browser → el App Server sigue.
- El thread se persiste continuamente.
- Al re-abrir → catch-up del event log.

## Auth web
- Cookie de sesión / token OIDC.
- El gateway resuelve al App Server del usuario.
- El App Server nunca expone credenciales del provider al browser.

## Multi-tenant
- 1 App Server por usuario en su container (aislamiento fuerte).
- Alternativa multi-tenant en un solo proceso: posible pero requiere repo de threads particionado y aislamiento de recursos; **no recomendado para v1**.

## Latencia
- SSE conserva el streaming token-a-token.
- Idealmente, gateway en la misma región que el App Server.

## Ofline / desconexión
- El browser detecta `error: 0` en SSE → reconnect con backoff.
- El App Server marca al cliente `disconnected` tras 30s sin heartbeat; sigue ejecutando turns activos.
