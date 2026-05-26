---
id: frontend-shell/tauri-vs-app-server
title: Tauri sidecar vs App Server externo
shard: 05-frontend-shell
tags: [tauri, deployment, sidecar]
summary: Por qué Tauri lanza app-server como sidecar y no embebe el core.
related: [architecture/process-model, app-server/overview]
sources: []
---

# Tauri ↔ App Server

## Opciones
1. **Sidecar** (elegido) — Tauri bundlea el binario `harness-app-server` y lo lanza como child.
2. **Embedded core** — el crate Tauri linkea `harness-core` y expone funciones via `#[tauri::command]`.
3. **Sidecar + embed opcional** — vía feature flag, usable para tests.

## Por qué sidecar

| | Sidecar | Embedded |
|---|---|---|
| Crash UI no mata agente | ✅ | ❌ |
| Paridad con CLI/web | ✅ | ❌ |
| Update independiente del binario core | ✅ | ❌ |
| Latencia IPC | un poco más alta (~50µs/msg) | inmediata |
| Tamaño binario final | mayor (~+15 MiB) | menor |

Para v1 priorizamos resiliencia y paridad.

## Bundle
`tauri.conf.json`:

```json
{
  "bundle": {
    "externalBin": ["binaries/harness-app-server"],
    "resources": ["resources/agents.md.template"]
  }
}
```

`build.rs` del crate Tauri copia el binario compilado desde `target/release/harness-app-server` al directorio esperado por la plataforma.

## Comandos Tauri expuestos
- `rpc_connect(channel)` — abre stdio al sidecar y empieza a rutear al canal.
- `rpc_send(line)` — escribe una línea JSON al sidecar.
- `open_path(path)` — abre con el handler del SO (para "ver en explorador").
- `pick_file(filters)` — diálogo nativo.

Todo lo demás va por JSON-RPC, no por comandos Tauri. Mantiene el contrato unificado.
