---
id: cross-cutting/logging-tracing
title: Logging y tracing
shard: 09-cross-cutting
tags: [logging, tracing, observability]
summary: `tracing` con spans jerárquicos thread > turn > tool.exec.
related: [cross-cutting/telemetry, harness-core/agent-loop]
sources: []
---

# Logging y tracing

## Stack
- `tracing` + `tracing-subscriber` + `tracing-appender` (rotación diaria).
- Salida: `~/.harness/logs/harness.log` por defecto; JSON formatter para parseo.
- Nivel runtime configurable: `RUST_LOG=harness_core=debug,sqlx=warn`.

## Spans jerárquicos

```
session
└── thread.run (thread_id)
    └── turn.run (turn_id)
        ├── prompt.build (prefix_hash, segments)
        ├── llm.stream  (provider, model, tokens_in, tokens_out, cost)
        └── tool.exec (name, args_hash, duration, exit)
            └── sandbox.enforce (level, denials)
```

Cada span lleva atributos estables para correlación.

## Reglas
- **Stdout/stderr**: el `harness-server` (Axum) loggea a stderr en container; `docker compose logs backend` los muestra. El `harness-mcp-server` (sub-process por spawn) **sí** usa stdout para JSON-RPC al CLI hijo — su `tracing` va estrictamente a stderr.
- **No loggear secretos**: el prompt builder sustituye `{{secret:*}}` antes de loggear el prompt completo.
- **Spans cortos**: nada de spans abiertos por horas; cerrar al terminar la operación.
- **Nivel correcto**: `info` para eventos de negocio (turn iniciado), `debug` para detalle interno, `trace` para datos crudos.

## Métricas como atributos
En vez de exporters dedicados (v1), las métricas viven como atributos del span:
- `turn.run` ⇒ `iterations`, `tokens_in`, `tokens_out`, `cost_usd`, `cache_hit_rate`.
- `tool.exec` ⇒ `duration_ms`, `bytes_in`, `bytes_out`.

Un parser CLI (`harness stats --since 1d`) agrega del log JSON.

## Métricas consultables
- `GET /metrics` expone métricas Prometheus agregadas del proceso: sesiones,
  tasks, presión de contexto, lag SSE y build info. Es público para scraping y
  usa labels opacos cuando hay información de sesión.
- `GET /api/sessions/:sid/metrics` expone una foto privada por sesión. Además
  de tokens/costo/capabilities, el bloque `conversation` deriva observabilidad
  desde el transcript append-only: cantidad de eventos, mensajes user/assistant,
  thinking, resultados de tools, errores de tools, duración total, mayor gap
  entre eventos, mayor payload de argumentos/resultados y duración total/max
  por nombre de tool emparejando `tool_use_id`.

Usa las métricas por sesión para investigar conversaciones lentas, skills/MCPs
que generan payloads grandes, tools que fallan seguido o gaps largos donde el
agente parece quedarse sin feedback visible.

## UI debug
Atajo `Cmd/Ctrl+Shift+D` abre panel con últimos 100 spans del thread activo en tiempo real.
