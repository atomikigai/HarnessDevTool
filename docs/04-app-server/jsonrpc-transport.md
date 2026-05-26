---
id: app-server/jsonrpc-transport
title: Transport JSON-RPC (stdio)
shard: 04-app-server
tags: [transport, stdio, jsonl]
summary: Implementación del reader/writer JSONL.
related: [architecture/ipc-protocol, app-server/message-processor]
sources: []
---

# Transport

## Reader (stdin)

```rust
let stdin = tokio::io::stdin();
let reader = tokio::io::BufReader::new(stdin);
let mut lines = reader.lines();
while let Some(line) = lines.next_line().await? {
    if line.is_empty() { continue; }
    match serde_json::from_str::<Message>(&line) {
        Ok(msg) => tx_to_processor.send(msg).await?,
        Err(e) => emit_parse_error(e),
    }
}
```

## Writer (stdout)
Canal MPSC: cualquier task del server publica un `Message` → un writer task único lo serializa y escribe con `\n`. **Crítico** que un único writer toque stdout, si no las líneas se entrelazan.

## Logging
**Nunca** a stdout. `tracing` configurado para escribir a stderr o a un file appender bajo `~/.harness/logs/`.

## Flow control
- MPSC bounded (default 4096).
- Si el cliente no lee, el writer se bloquea → eventualmente backpressure llega al core via `EventSink` → ver [[harness-core/streaming-events]].

## Tamaño de mensaje
- Default max 16 MiB / línea.
- Adjuntos grandes (transferencias SFTP, queries con MB de resultado) no van por JSON-RPC sino vía referencias a archivos (`file://`).

## Encoding
UTF-8 estricto. JSON sin `NaN`/`Infinity` (no son válidos en JSON). Bytes binarios → base64 con prefijo `data:application/octet-stream;base64,`.

## Heartbeat
Notification `session.ping` cada 30s en ambos sentidos. Si el peer no responde en 90s, se considera muerto.
