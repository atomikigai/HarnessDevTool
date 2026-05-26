---
id: architecture/ipc-protocol
title: Protocolo IPC (JSON-RPC sobre stdio)
shard: 02-architecture
tags: [ipc, jsonrpc, protocol, jsonl]
summary: Mensajes line-delimited JSON; namespaces dominio.accion; bidireccional con notifications.
related: [app-server/jsonrpc-transport, app-server/message-processor, app-server/backward-compat]
sources: []
---

# Protocolo IPC

## Transporte
- **JSONL** sobre stdio: cada mensaje en una línea (`\n` terminator).
- Sin framing binario. Facilita debug con `tee` y `jq`.
- UTF-8. Tamaño máximo por línea: 16 MiB (configurable).
- En web: mismo JSON envuelto en SSE (`event: rpc\ndata: {...}\n\n`).

## Forma del mensaje
JSON-RPC 2.0 + extensiones:

```jsonc
// request (cliente → server)
{ "jsonrpc": "2.0", "id": "uuid", "method": "thread.create", "params": { ... } }

// response (server → cliente)
{ "jsonrpc": "2.0", "id": "uuid", "result": { ... } }
// o
{ "jsonrpc": "2.0", "id": "uuid", "error": { "code": -32000, "message": "...", "data": { ... } } }

// notification (cualquier sentido, sin id)
{ "jsonrpc": "2.0", "method": "item.delta", "params": { "thread": "...", "turn": "...", "item": "...", "text": "..." } }
```

## Namespaces

| Namespace | Métodos clave |
|---|---|
| `session` | `initialize`, `shutdown`, `capabilities.get` |
| `thread` | `create`, `resume`, `fork`, `archive`, `list`, `send` |
| `turn` | `cancel`, `get` |
| `item` | `started`, `delta`, `completed` (notifications server→client) |
| `approval` | `request` (server→cliente), `respond` (cliente→server) |
| `tool` | `list`, `describe` |
| `mcp` | `list`, `add`, `remove` |
| `module.agents` | `session.spawn`, `session.input`, `session.kill` |
| `module.db` | `connection.add`, `query.run`, `query.cancel`, `schema.tree` |
| `module.ssh` | `session.open`, `sftp.list`, `transfer.queue`, `transfer.cancel` |

## Cancelación
Todo método long-running puede cancelarse con `<namespace>.cancel { id }`. El server emite `<x>.cancelled` y libera recursos.

## Versionado
- `session.initialize` incluye `protocolVersion: "1.0"`.
- Server responde con `protocolVersion` y `supportedFeatures: [...]`.
- Cliente nuevo + server viejo: cliente cae a features subset.
- Server nuevo + cliente viejo: server emite formato legacy si el cliente no anuncia la feature.

Ver [[app-server/backward-compat]].

## Códigos de error
| Code | Significado |
|---|---|
| -32700 | Parse error |
| -32600 | Invalid request |
| -32601 | Method not found |
| -32602 | Invalid params |
| -32000 | App: tool denied by sandbox |
| -32001 | App: thread not found |
| -32002 | App: approval timeout |
| -32010 | App: provider rate limit |

Cuerpo: ver [[cross-cutting/error-model]].
