---
id: architecture/layered-architecture
title: Arquitectura por capas
shard: 02-architecture
tags: [architecture, layers]
summary: 5 capas con dependencias estrictamente hacia abajo.
related: [architecture/system-overview, architecture/process-model, build-plan/repo-layout]
sources: []
---

# Capas

| # | Capa | Crates / paquetes | Responsabilidad |
|---|---|---|---|
| 5 | Surface (UI) | `frontend/` (SvelteKit + adapter-node) | UX, render, atajos |
| 4 | Server (HTTP+SSE) | `harness-server` (Axum) | Routes REST, SSE hub, CORS |
| 3 | Orquestación | `harness-core` | Threads, tasks (state machine), scheduler |
| 2 | Servicios | `harness-session`, `harness-mcp-server`, `harness-sandbox`, `harness-skills` | Capacidades transversales |
| 1 | Módulos verticales | `module-db`, `module-ssh` | Dominios concretos (F4) |
| 0 | Plataforma | tokio, sqlx, russh, portable-pty, axum, tower-http, ... | Crates externos |

## Reglas

- Dependencias **estrictamente hacia abajo**. Nunca un módulo vertical (1) depende de un route handler (4).
- Módulos (1) **exponen** tools al `harness-mcp-server` (2). El core (3) las descubre vía registry.
- La UI (5) **solo** habla con HTTP+SSE de (4). Nunca importa core directo.
- (3) emite eventos a un `EventSink` abstracto que (4) implementa como hub SSE.
- Cualquier crate en (2/3) es **testeable sin levantar el servidor** (4).

## Beneficios

- Cambiar Axum por otro framework solo toca (4).
- Probar el scheduler sin tocar HTTP.
- Añadir un módulo no toca capas superiores.
- Sustituir SQLite por otro motor solo toca crates de (2/3) según features.

## Anti-patrones

| Mal | Bien |
|---|---|
| Lógica de negocio en route handlers (capa 4) | Handlers thin; lógica en `harness-core` |
| UI invocando `module-db` directamente | UI → HTTP REST → harness-server → module |
| Sandbox como wrapper opcional | Obligatorio para tools que mutan FS o ejecutan código |
| Crates de la capa 2 importando `axum` | (2) no conoce el transport |
| Sesiones PTY vivas como singletons | Manejadas por `harness-session` con cleanup determinista |
