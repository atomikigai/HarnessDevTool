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
| UI primaria | **Web (SvelteKit) servida por adapter-node en container** | el usuario priorizó web/self-host/LAN; no tocar la web UI para el track desktop |
| Desktop app post-F6 | **Track paralelo, basado funcionalmente en la UI actual, con decisión por métricas** | Decisión revisitada 2026-06-08: la app desktop se trabaja en paralelo sin modificar la web UI. Tauri es el baseline más fácil porque puede reutilizar SvelteKit; Slint sigue como candidato performance-first aunque requiere reconstruir módulos (`Agents`, terminal, tasks, métricas) en `.slint`. La elección final exige medición real de startup, memoria, CPU/render y paridad funcional |
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
| Autonomy profile default | **`assisted`** | permite asumir decisiones reversibles sin convertir proyectos nuevos en ejecucion headless |
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
| Autorización MCP por rol | **`role + tool + resource + scope + ownership + thread_id + path_policy`** (ver [[agents/role-capability-matrix]]) | Q9 cerrada: `task.create` planner-only, `task.propose` para workers, `repo.write` atado a paths de la task, audit obligatorio en allow y deny |
| CLIs soportados | **`claude`, `codex`, `cursor` (cursor-agent), `antigravity` (`agy`)** — set fijo, sin `agent_kind: custom` | Q5 cerrada. Cada CLI necesita detector + plantilla de spawn + mapeo de flags + smoke test. Matriz en [[agents/supported-clis]] |
| Contexto inicial del repo del usuario | **`ARCHITECTURE.md` generado en el repo** por un agente repo-mapper en la primera apertura; corto, indexable, shards atómicos | Q2 reformulada: `AGENTS.md` global del usuario no es esencial; lo que importa es contexto por-repo, generado y refrescable. No pisa `AGENTS.md` si el repo ya tiene uno |
| Multi-sesión en UI | **Vivas en backend independientes de la ruta UI activa** | Q4 cerrada. PTYs, pools DB, channels SSH no se cierran al navegar; tabs para sesiones de agentes con indicador "live"; cierre solo por acción explícita o TTL |
| Multi-tab DB queries | **Shared pool default + pin opt-in (lease de conexión)** al detectar `BEGIN` o toggle manual | Q13 cerrada. Trigger automático en `BEGIN`, libera en `COMMIT`/`ROLLBACK`/timeout 5min. Cancelación usa conexión auxiliar del pool, ortogonal al lease |
| Tracing cross-process | **`spawn_id` UUID v4** propagado en spans `tracing` + path `spawns/<sid>/output.log` | Q3 cerrada. Cross-ref por timestamp + id |
| `spec.md` durante thread activo | **Append-only**; solo planner/orchestrator edita. `set_section` exige version check + section lock atómico | Q11 cerrada. Workers escriben en `task.notes`/`task.artifacts`/`qa.results`, nunca en spec |
| SFTP transfer default policy | **`resume`**; fallback `ask` con modal por archivo si no es resumable. **Nunca `overwrite` silencioso** | Q14 cerrada |
| GEPA tasks-target | **5 curated manual** al cierre de F3 en `tests/eval/targets/` (frontend simple, backend CRUD, bug fix, refactor, DB schema change) | Q18 cerrada. F6 puede ampliar a generated |
| Distribución | **Self-hosted only** — Dockerfile + compose en el repo, usuario clona y builda. No publicar a registries públicos | Q19 cerrada. Re-abrir si surge demanda real |
| `harness-mcp-server` runtime | **In-process** vía feature `embedded` por default; child process documentado como fallback | N1 cerrada. Interfaz MCP stdio JSONL idéntica en ambos modos |
| Sandbox de tools del CLI hijo | **Confiamos en el sandbox del CLI** (`claude`/`codex`/`cursor`/`agy`); `harness-sandbox` envuelve solo lo que el bridge ejecuta directamente | N3 cerrada. Mayoría del bridge es read-only |
| Protocolo de autonomia | **Readiness check + execution mode + autonomy profile + handoff schema** antes de planificar caro | Evita bloqueos tardios por credenciales/env y evita usar DAG completo para cambios cortos |
| Auth bind-mount container | **Bind-mount RW compartido** de `~/.claude/`, `~/.codex/`, `~/.cursor/`, `~/.antigravity/`. Refresh tokens sobreviven destruir el container | N4 cerrada. Restricción documentada: no correr el mismo CLI con otra cuenta en host paralelo |
| Adjuntos a sesiones | **`POST /api/sessions/:sid/attach`** multipart → `$HARNESS_HOME/.runtime/attach/<sid>/`. Tools MCP `attach.list/read` exponen archivos al CLI hijo | N5 cerrada. Habilita que CLIs vean imágenes/PDF/docs/archivos arbitrarios. Cleanup al cerrar sesión o TTL 24h |

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
