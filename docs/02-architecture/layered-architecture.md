---
id: architecture/layered-architecture
title: Arquitectura por capas
shard: 02-architecture
tags: [architecture, layers]
summary: Cinco capas con dependencias unidireccionales (hacia abajo).
related: [architecture/system-overview, architecture/process-model]
sources: []
---

# Capas

| # | Capa | Crates / paquetes | Responsabilidad |
|---|---|---|---|
| 5 | Surface | `apps/desktop` (SvelteKit+Tauri), `apps/cli` | UX, render, atajos |
| 4 | Transporte | `harness-app-server` | JSON-RPC stdio / HTTP+SSE |
| 3 | Orquestación | `harness-core` | Agent loop, threads, turns |
| 2 | Servicios | `harness-sandbox`, `harness-mcp`, `harness-llm` | Capacidades transversales |
| 1 | Módulos verticales | `module-agents`, `module-db`, `module-ssh` | Dominios concretos |
| 0 | Plataforma | Tokio, sqlx, russh, seccompiler | Crates externos |

## Reglas
- Dependencias **estrictamente hacia abajo**. Nunca un módulo (1) depende de App Server (4).
- Módulos (1) **exponen** tools al core (3) por un trait `HarnessTool` (un sólo punto de contacto).
- Surfaces (5) **solo** hablan con (4). Nunca importan core directo.
- (3) no conoce el transporte: emite eventos a un `EventSink` abstracto que (4) implementa.

## Beneficios
- Probar el core sin levantar App Server.
- Añadir un módulo no toca capas superiores.
- Sustituir transporte (stdio → websocket) sin tocar core.

## Anti-patrones
- Lógica de negocio en el message processor (capa 4): debe ir al core.
- UI invocando `module-db` directamente: rompe la capa 4 y la auditoría/seguridad.
- Sandbox como wrapper opcional: debe ser obligatorio para tools nativas.
