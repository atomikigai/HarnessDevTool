---
id: cross-cutting/telemetry
title: Telemetría
shard: 09-cross-cutting
tags: [telemetry, metrics, opt-in]
summary: Opt-in, agregada, sin contenido de prompts.
related: [cross-cutting/logging-tracing, cross-cutting/security-model]
sources: []
---

# Telemetría

## Postura
**Off por defecto.** El usuario opta in explícitamente con `harness telemetry enable`.

## Qué se envía (cuando habilitada)
- Versiones (harness, OS, modelo).
- Conteos agregados:
  - turns iniciados / completados / cancelados
  - tareas done / abandoned
  - tool calls por nombre (no args)
  - duración percentiles
  - tokens IN/OUT por modelo (no contenido)
  - errores agrupados por código

## Qué **nunca** se envía
- Contenido de prompts, mensajes, código del usuario.
- Paths del FS del usuario.
- Credenciales, host names, IPs.
- Output de tools.

## Cómo se envía
- Endpoint propio (`telemetry.harness.example.org`) o auto-host.
- HTTPS con pin de cert.
- Batched (1/hora) + jitter para evitar tráfico predecible.

## Inspección local
`harness telemetry show --pending` lista lo que se enviaría antes del flush. Da confianza al usuario.

## Opt-out granular
```toml
[telemetry]
enabled = true
include = ["versions", "counts", "tokens"]
exclude = ["errors_grouped"]
```

## Auto-host
Para deploys corporativos: `telemetry.endpoint = "https://mi-empresa.tld/h"`. Mismo formato.

## Métricas locales (siempre on)
Para que el usuario inspeccione su propio uso sin enviar nada:
- `harness stats --since 1d` lee `~/.harness/logs/` y agrega.
- Panel UI "Mi semana": gráficas locales, ningún envío.
