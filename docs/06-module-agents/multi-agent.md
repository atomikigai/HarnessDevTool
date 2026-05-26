---
id: module-agents/multi-agent
title: Múltiples sesiones en paralelo
shard: 06-module-agents
tags: [multi-agent, sessions, orchestration]
summary: N PTYs concurrentes; UI con tabs; recursos por sesión.
related: [module-agents/overview, foundations/anthropic-principles]
sources: [foundations/anthropic-principles]
---

# Multi-agent

## Modelo
- N `AgentSession` en un `DashMap<SessionId, AgentSession>` dentro del módulo.
- Cada una con su propio `cwd`, `args` y `profile`.
- UI: tabs en la vista `/agents`. Tab dedicada por sesión.

## Casos de uso
- Trabajar en varios repos a la vez.
- Patrón **tri-agente** Anthropic emulado lanzando 3 sesiones (planner / generator / evaluator).
- Comparar respuestas de modelos diferentes en paralelo (un perfil por modelo).

## Coordinación opcional
- Una tool `agents.broadcast { session_ids, text }` permite mandar el mismo input a varias.
- Una tool `agents.collect { session_ids, until }` recolecta output hasta un marker (p.ej. "DONE").

Estas tools las usa un thread del harness-core orquestador, no la UI directamente.

## Recursos
- CPU: cada PTY child de `claude` puede consumir mucho. UI muestra warning al pasar de 4 concurrentes.
- Memoria: el output log se rota a 50 MiB.
- Disco: agregar pago de espacio en `~/.harness/modules/agents/` al cuotaje global.

## Aislamiento
- Cada sesión hereda solo env vars whitelisteadas (ver [[module-agents/claude-cli-bootstrap]]).
- `cwd` distinto por sesión → sin conflictos.
- No comparten state entre sí salvo lo que el usuario mueva manualmente vía UI.
