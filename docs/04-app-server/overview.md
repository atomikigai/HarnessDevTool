---
id: app-server/overview
title: App Server — visión general
shard: 04-app-server
tags: [app-server, broker, overview]
summary: Por qué un proceso largo dedicado entre surfaces y core.
related: [architecture/process-model, app-server/jsonrpc-transport, app-server/message-processor]
sources: [foundations/openai-codex-architecture]
---

# App Server

## Por qué existe
- **Aislamiento**: si la UI muere, el agente sigue.
- **Multi-surface**: misma sesión observable desde CLI y desktop.
- **Web**: el cliente web no embebe `harness-core`; habla con un App Server remoto.
- **Backward-compat**: el contrato JSON-RPC sobrevive cambios internos del core.

## Anatomía

```
┌────────────────────────────────────────────────┐
│  stdio transport     (lee/escribe JSONL)       │
├────────────────────────────────────────────────┤
│  message processor   (rutea por namespace)     │
├────────────────────────────────────────────────┤
│  thread manager      (1 task por thread)       │
├────────────────────────────────────────────────┤
│  core threads        (harness-core)            │
└────────────────────────────────────────────────┘
```

## Vida del proceso
1. `initialize` handshake.
2. Carga índice de threads desde disco.
3. Acepta operaciones hasta `shutdown`.
4. Al recibir `shutdown` o SIGTERM: flush event logs, cierra tools MCP, exit 0.

## Capabilities negociadas
En `initialize`:
- `protocolVersion`
- `modules`: lista de módulos disponibles (agentes, db, ssh)
- `mcpServers`: precargados desde config

El cliente decide qué módulos exponer en UI según capabilities.

## Errores fatales
Si el processor recibe un mensaje malformado: log + respuesta -32600 + continuar. **Nunca** crashear por input cliente.

## Modo embebido (opt)
Feature `embed-app-server` permite a la surface linkear el core in-process sin spawn. Útil para tests E2E. Producción usa siempre child process.
