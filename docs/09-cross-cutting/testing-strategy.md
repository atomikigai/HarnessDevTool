---
id: cross-cutting/testing-strategy
title: Estrategia de testing
shard: 09-cross-cutting
tags: [testing, unit, integration, eval]
summary: Unit, integration, golden protocol, sandbox suite, eval del harness completo.
related: [cross-cutting/logging-tracing, architecture/ipc-protocol]
sources: []
---

# Testing

## Niveles

### Unit (por crate)
- `cargo test` en cada crate.
- Mocks: `mockall` para traits, `wiremock` para HTTP del provider.
- Cobertura objetivo 70%+ para `harness-core` (core lógico), 50% otros.

### Integration (harness-server end-to-end)
- Carpeta `backend/tests/integration/`.
- Arranca `harness-server` real (in-process via `axum::serve`), hace requests HTTP + escucha SSE, verifica respuestas y events.
- CLI hijo (`claude`/`codex`) **stubbed** en tests integración (mock que responde a llamadas MCP del harness-bridge).

### Golden protocol
- `tests/golden/` con fixtures `request.json` → `expected_response.json`.
- Cubre todas las versiones soportadas del protocolo (v1.0, v1.1, ...).
- Diff legible cuando falla.

### Sandbox suite
- Catálogo de tools "maliciosas":
  - `rm -rf $HOME`
  - `curl evil.com`
  - `fork-bomb`
  - escribir a `~/.ssh/`
- Cada una debe **fallar** con causa explícita en cada SO target.
- CI matriz: linux + macos + windows.

### Eval del harness completo
Inspirado en el ciclo Anthropic v1/v2:
- Set de tareas-target: "construir CRUD básico", "agregar paginación", "fix bug X".
- Métricas: tasks resueltas, costo, wallclock, calidad evaluada por un LLM-judge separado.
- Corre en CI nocturno; alerta si regresión > 10%.

## Tests reproducibles
- Tasks de eval se ejecutan con seed fija (modelo `temperature=0` cuando posible, sandbox determinista).
- Diffs entre runs son trazables.

## Fixture de threads
- `tests/fixtures/threads/` con threads pre-construidos (event log + tasks).
- Permite tests de `resume` y backward compat sin generación.

## CI
- PR: unit + integration + golden + sandbox.
- Main: + eval rápido (3 tasks).
- Nightly: + eval completo (todo el set).
