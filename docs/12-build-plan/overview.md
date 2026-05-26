---
id: build-plan/overview
title: Plan de construcción — overview
shard: 12-build-plan
tags: [plan, phases, roadmap]
summary: Hoja de ruta F0–F6 con dependencias entre fases y criterio de "done" por fase.
related: [build-plan/tech-stack-locked, build-plan/repo-layout, build-plan/decisions-locked, build-plan/open-questions, foundations/lessons-learned]
sources: []
---

# Plan F0–F6

> Pivote vigente: la UI primaria es **web (SvelteKit)**, no desktop. El backend Rust **orquesta** instancias de `claude`/`codex` CLI; no habla con la API del modelo directamente.

## Tabla maestra

| Fase | Meta de una línea | Test de "done" |
|---|---|---|
| **F0** [[build-plan/phase-0-skeleton]] | Server arranca, persiste, sirve UI vacía | `docker compose up` levanta y el browser pinta la shell |
| **F1** [[build-plan/phase-1-sessions]] | Lanzar 1 `claude`/`codex` desde la UI con PTY visible | Terminal en vivo, input bidireccional, kill limpio |
| **F2** [[build-plan/phase-2-tasks-mcp]] | El CLI puede claim/update tasks vía MCP | `claude` llama `task.claim`, lease expira, state machine respetada |
| **F3** [[build-plan/phase-3-team]] | Planner/Generator/Evaluator trabajando un spec | "Build a TODO app" termina con tasks `done` verificadas dentro de budget |
| **F4** [[build-plan/phase-4-modules]] | Módulos DB + SSH usables por humano y por agente | Query paginada, SFTP transfer con resume |
| **F5** [[build-plan/phase-5-skills]] | Skills + Learner `proposed/` + Curator determinístico | Tras una semana hay skills sugeridas y stale marcadas |
| **F6** [[build-plan/phase-6-polish]] | Curator LLM + GEPA + USER.md + packaging | GEPA emite un PR con métricas; instalador firmado |

## Dependencias entre fases

```
F0 ─► F1 ─► F2 ─► F3 ─► F4 ─► F5 ─► F6
              │     │
              └─► F4 puede arrancar en paralelo a F3
                  (no comparten estado salvo persistence)
```

F4 puede paralelizarse con F3 si hay manos suficientes; F5 requiere F3 (necesitas traces de equipo para tener algo que aprender).

## Reglas que aplican a todas las fases

1. **Roundtripability** desde F0: `export thread → import en otra máquina → resume → no se nota diferencia`. Si esta propiedad se rompe, es bug crítico.
2. **Append-only**: `events.jsonl` y prompt construction. Ver [[harness-core/prompt-caching]].
3. **Schemas validados**: todo TOML/JSON/YAML tiene JSON Schema versionado bajo `backend/crates/harness-core/schemas/`. Drift = build break.
4. **Backward compat del protocolo**: versionado desde F0 (`X-Protocol-Version: 1.0`). Ver [[app-server/backward-compat]].
5. **Tipos**: `ts-rs` regenera `frontend/src/lib/api/types/` antes de cada commit que toca structs expuestas.
6. **Observabilidad**: cada fase añade spans `tracing` para sus operaciones nuevas.

## Lo que **no** está en F0–F6

- CLI cliente (`harness chat ...`) — pospuesto hasta después de F6.
- Multi-tenant / multi-user — fuera de scope; este plan es single-user local self-host.
- Tauri desktop — descartado.
- API directa al modelo (sin pasar por `claude`/`codex`) — descartado para v1.
- Telegram/Discord/Slack adapters — fuera de scope.

## Criterio para avanzar de fase

Cada fase tiene un **test de aceptación binario** (ver shard de la fase). No se empieza la siguiente hasta:
- Tests del test de aceptación pasan.
- `cargo clippy -- -D warnings` y `pnpm lint` limpios.
- Docs de la fase actualizadas (shards correspondientes).
- `just docker-build` + `just docker-up` levantan sin warnings.

Ver [[build-plan/decisions-locked]] para los defaults ya fijados y [[build-plan/open-questions]] para lo que aún hay que aclarar.
