---
id: build-plan/decisions-locked
title: Decisiones bloqueadas (fijadas)
shard: 12-build-plan
tags: [decisions, locked, rationale]
summary: Las decisiones que ya no se re-abren salvo motivo fuerte.
related: [build-plan/tech-stack-locked, build-plan/open-questions]
sources: []
---

# Decisiones bloqueadas

## Arquitectura

| Decisión | Valor fijo | Razón |
|---|---|---|
| UI primaria | **Web (SvelteKit) servida por adapter-node en container** | el usuario priorizó web; desktop descartado |
| UI secundaria | **CLI cliente** | pospuesto post-F6 |
| Backend lang | **Rust + Axum** | preferencia del usuario; Axum por idiomático y composable |
| Modelo de procesos | **Backend orquesta `claude`/`codex` CLI** | el usuario debe tener uno instalado; cero provider auth en harness |
| Transport browser ↔ backend | **HTTP + SSE** | nativo, simple, sin WS overhead |
| CORS dev/local | **Habilitado para `http://localhost:8080`** | dos containers en orígenes distintos |
| MCP entre harness ↔ child CLI | **Server local stdio JSONL** | el harness expone tools al CLI hijo (no al revés) |
| Type sharing | **`ts-rs`** | Rust es fuente única; cero drift |
| Runtime validation TS | **valibot** (no zod) | tree-shake, bundle chico |
| Mono-repo | **carpetas `backend/` y `frontend/` separadas, sin turborepo** | simple |
| Task runner | **`just` (Justfile)** | cross-platform, idiomático Rust |

## Storage

| Decisión | Valor fijo | Razón |
|---|---|---|
| Event log | **`events.jsonl` append-only por thread** | append-only es ley, ver [[harness-core/prompt-caching]] |
| Index global | **SQLite con FTS5** | un solo motor, sirve para memory search |
| Task storage | **1 archivo TOML por task** | locking fino, `git blame` por task |
| Schema validation | **JSON Schemas en `harness-core/schemas/`** | drift = build break |
| Skills format | **Markdown + frontmatter YAML** | mismo formato que docs; humano-editable |
| Skills bajo git | **opt-in, default ON** | auditoría gratis vía `git log` |
| Cifrado de datos | **off por default, depende del cifrado de disco del SO** | usuario puede activar `--encrypt` con `age` |

## Deploy

| Decisión | Valor fijo | Razón |
|---|---|---|
| Backend en Docker | **Sí, con bind-mount de `claude`/`codex` del host** | aislamiento + sin instalar CLI dentro |
| Frontend en Docker | **Sí, `node:alpine` + adapter-node** | container delgado, soporta SSR si lo activamos |
| Profile activo | **`HARNESS_PROFILE` env + symlink** | inspirado en Hermes; visible con `ls -la` |
| `HARNESS_HOME` default | **`/data` en container, `~/.harness/` en local** | volumen persistente nombrado |
| Web port default | **`8080`** (frontend) | |
| API port default | **`7777`** (backend) | |
| Protocol version header | **`X-Protocol-Version: 1.0`** desde F0 | backward compat desde el día 1 |

## Comportamiento del harness

| Decisión | Valor fijo | Razón |
|---|---|---|
| Single-user local self-host | **Sí** | scope del proyecto |
| Approval mode default | **`risky-only`** | balance fatiga/seguridad |
| Sandbox default | **`workspace`** | RW solo en project_root |
| Learner mode | **`proposed/` (revisión humana)** | filosofía Rust: nada cambia sin verse |
| Curator LLM review | **off en F5, on en F6** | empezar barato |
| Curator nunca borra | **Solo archiva** | recuperable con `harness curator restore` |
| Generator ≠ Evaluator | **`verified_by != assignee`** | anti auto-elogio (Anthropic) |
| Budget hard cap default | **$10 USD por thread** (dev); configurable | seguridad económica |
| `skills.search` antes de F5 | **devuelve `[]`** | no rompe agentes que ya la usen |
| Roundtripability | **Test obligatorio desde F0** | export → import → resume sin pérdida |

## Estética

| Decisión | Valor fijo | Razón |
|---|---|---|
| Tema default | **dark** | uso intensivo de pantalla |
| Componentes UI base | **shadcn-svelte añadidos selectivamente** | no bloat |
| Iconos | **lucide-svelte re-exported en `lib/icons.ts`** | tree-shake control |
| Idioma docs | **español neutro** | preferencia del usuario |
| Idioma identifiers/código | **inglés** | convención |

## Soporte de plataformas

| Decisión | Valor fijo | Razón |
|---|---|---|
| Linux + macOS desde F0 | **Sí** | targets principales |
| Windows | **F6 (soporte best-effort en F1–F5)** | ConPTY + sandbox espinosos |
| Engines DB por defecto compilados | **Solo SQLite** | postgres/mysql como features |

## Runtime de agentes

| Decisión | Valor fijo | Razón |
|---|---|---|
| Modelo de agentes | **Efímeros con plantillas componibles (opción C)** | Aislamiento entre tasks, sin pool |
| Roles canónicos | **planner / generator / evaluator / arbitrator / curator / learner** (+ psychologist F6) | Set finito, mapeable a shards |
| Dominios componibles | **frontend / backend / database / devops / qa / generic** | 7 dominios iniciales en F3 |
| Smart loading | **3 niveles: declared / recommended / runtime** | Mínimo overhead, defensa en profundidad |
| `harness-bridge` MCP | **Siempre cargado** en cualquier spawn | Sin esto no hay rails |
| Re-plan cap | **K = 2** | Tras 3 re-plans → abandoned + alerta humano |
| user_approval tras planning | **Default ON (confirm)** | Evita corridas largas sobre malas specs |
| Drift minor | **Rust diff + Arbitrator LLM ligero** | Decisión barata, auditable |
| Generator ≠ Evaluator | **`verified_by != assignee`** | Anti auto-elogio (Anthropic) |

## Memoria y profiles

| Decisión | Valor fijo | Razón |
|---|---|---|
| Profiles | **Existen como primera clase** | Caso real: dos trabajos, mismo stack |
| Profile activo | **Symlink `active_profile` + `HARNESS_PROFILE` env override** | Inspirado en Hermes |
| Skills scope default | **Profile-scoped**; `promote` a `shared/` con review | Privacidad por defecto |
| USER.md | **Global** en `~/.harness/USER.md` + `PROFILE.md` por profile | Una identidad, varios contextos |
| Auth `claude`/`codex` | **Per profile** vía `cli-state/`; symlink dinámico | Imposible "cuenta equivocada" |
| Memoria formato | **YAML frontmatter + Markdown body** | Estructura + prosa para el modelo |
| Memoria kinds | **decision / pending / in_flight / fact / snapshot** | Set finito y enumerable |
| `memory.note` de agentes | **Approval obligatorio** del humano | Filosofía Rust: nada cambia sin verse |
| Inyección de continuidad al prompt | **Solo al resume** de thread existente; slice del thread | Evita polución de contexto |
| `CONTINUITY.md` | **Auto-generado** on-change + 1h fallback, throttle 10s | Banner UI y resume útiles |
| Índice de búsqueda | **SQLite FTS5** por profile | Rápido, embebido, regenerable |
| Git por profile | **Default ON, remotes opcionales** | Auditoría + sync cross-machine |
| Threads bajo git | **Opt-in por thread** (`harness profile track-thread`) | Evitar inflar git con event logs |
| Push automático | **Off por default**; `auto_sync` opt-in | Control del usuario |

## Reglas meta

- Cualquier decisión puede **re-abrirse** con justificación escrita en este shard (sección "revisitada" al fondo).
- Si una decisión cambia, **migrar antes de invertir más código** en la asunción vieja.
- Estado de revisitas: ninguna al cierre de planning.
