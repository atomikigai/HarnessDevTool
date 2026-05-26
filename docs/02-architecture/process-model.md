---
id: architecture/process-model
title: Modelo de procesos
shard: 02-architecture
tags: [architecture, process, tokio, child-process]
summary: Quién corre dónde: App Server, sandboxed tools, MCP servers, módulos.
related: [architecture/ipc-protocol, harness-core/sandbox, harness-core/mcp-integration]
sources: []
---

# Procesos en runtime

## Principales
| Proceso | Dueño | Vida | Notas |
|---|---|---|---|
| `harness-app-server` | bundled by surface | larga | aloja N threads |
| Surface (Tauri / CLI) | usuario | hasta cerrar | lanza app-server como child |
| Tool exec (shell, build, ...) | core | corta | sandboxed, kill-on-cancel |
| MCP server | core (child stdio) | larga | uno por server registrado |
| Módulo `module-agents` Claude PTY | core | larga | un PTY por agent session |

## Hilado (dentro del App Server)
- Runtime **Tokio multi-thread**.
- 1 task por thread del usuario (cada thread del core en su task root).
- Tool execution en `FuturesOrdered` → paralelismo manteniendo orden.
- 1 task por MCP client connection.
- 1 task por surface conectada (lee stdin, escribe stdout).

## Lifecycle del App Server bundled
1. Surface arranca → spawnea `harness-app-server` como child con pipes.
2. Surface envía `initialize` (versión protocolo, capabilities).
3. App Server responde con capabilities y abre threads persistidos.
4. Surface envía operaciones; App Server emite eventos.
5. Cierre limpio: `shutdown` → flush event logs → exit 0.
6. Cierre sucio: surface detecta SIGCHLD → notifica usuario → resume al re-abrir.

## Web deployment
- App Server vive en un container worker.
- Cliente browser habla **HTTP + SSE** con un proxy delgado que mapea a JSON-RPC.
- El agente continúa aunque el tab cierre; reconexión = catch-up del event log.

## Aislamiento por usuario
- Un App Server por instalación local del usuario; threads aislados por filesystem (`~/.harness/threads/<uuid>/`).
- Web: un App Server por sesión del usuario.

## Por qué no in-process en Tauri
- Si el frontend crashea, el agente sigue.
- Misma topología que CLI / Web sin duplicar lógica.
- Sandboxing del tool exec aislado del proceso UI.
