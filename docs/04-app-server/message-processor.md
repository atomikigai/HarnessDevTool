---
id: app-server/message-processor
title: "[Tombstone] Message processor"
shard: 04-app-server
tags: [tombstone, deprecated]
summary: Obsoleto. La routing es por Axum Router, no por dispatcher JSON-RPC.
related: [app-server/overview, build-plan/repo-layout]
sources: []
---

# [Tombstone] Message processor

> Concepto del modelo Codex (dispatch por namespace JSON-RPC). **Ya no aplica**.

## Estado actual

Con Axum:
- Routes declarados en `harness-server/src/routes/*.rs`.
- Cada handler es una función async tipada con extractors (`State<AppState>`, `Path<...>`, `Json<...>`).
- Errores propagan vía `Result<impl IntoResponse, AppError>`.
- Tipos compartidos vía `ts-rs`.

No hay un "message processor" central; el Axum `Router` ES el dispatcher.

## Ver en su lugar

- [[app-server/overview]] — descripción de `harness-server` Axum
- [[build-plan/repo-layout]] — layout de `harness-server/src/routes/`
- [[architecture/ipc-protocol]] — wire HTTP+SSE
