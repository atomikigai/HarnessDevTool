---
id: foundations/lessons-learned
title: Lecciones de OpenAI y Anthropic para nuestro harness
shard: 01-foundations
tags: [lessons, principles, continuity, tasks, structured-data, team-of-agents]
summary: Síntesis prescriptiva: continuidad local, formatos estructurados, máquina de tareas y orquestación de equipo.
related: [foundations/anthropic-principles, foundations/openai-codex-architecture, foundations/design-tradeoffs, architecture/state-persistence, harness-core/thread-lifecycle, module-agents/multi-agent]
sources: [foundations/anthropic-principles, foundations/openai-codex-architecture]
---

# Lecciones — qué tomar de OpenAI y Anthropic

> Prescriptivo, no descriptivo. Cada sección dice **qué hacer en este repo** y **por qué** — anclado a fuentes.

---

## A. Fundamentos (de la arquitectura Codex y Anthropic)

### A1 · Núcleo único en Rust, no por surface
**Fuente**: Codex core (OpenAI) es una librería Rust compartida por CLI, IDE, web y macOS.
**Aquí**: `harness-core` es la fuente única de verdad. Surfaces hablan JSON-RPC. Si hay lógica de agente fuera del core, es bug. Ver [[harness-core/rust-crate-layout]].

### A2 · Estado vive fuera del cliente
**Fuente**: el Codex App Server sobrevive al cierre del tab; reconectar = catch-up.
**Aquí**: Tauri lanza `harness-app-server` como **sidecar**. UI guarda **cero estado** de threads. Toda persistencia en `~/.harness/`. Ver [[architecture/state-persistence]].

### A3 · Append-only es ley
**Fuente**: prompts se construyen append-only, estático antes que dinámico; reordenar/reescribir invalida prefix cache → costos x N.
**Aquí**:
- `events.jsonl` por thread, append-only.
- Tool defs serializadas con `BTreeMap` (orden determinista).
- Cambios de config tardíos → developer_message **apendizado**, jamás insertado atrás.
- Hash del prefix por request en logs para detectar misses silenciosos.

### A4 · Compaction y reset son herramientas distintas
**Fuentes**: Codex usa `encrypted_content` opaco; Anthropic combate context anxiety con resets + handoff estructurado.
**Aquí**:
- Auto-compaction al 75% del límite del modelo.
- Tool `thread.reset_with_handoff` que el propio agente puede invocar al detectar anxiety.
- Handoff materializado en `~/.harness/threads/<id>/handoffs/handoff-<ts>.md` (legible humano + consumible por el thread siguiente).

### A5 · Generator ≠ Evaluator (anti auto-elogio)
**Fuente**: Anthropic — separar adversariamente romp el sesgo de auto-elogio.
**Aquí**: ninguna task transiciona a `done` si `verified_by == assignee`. Excepción: modo `solo` (config) que permite `done_unverified` con badge visible.

### A6 · Cada componente codifica una asunción del modelo
**Fuente**: Anthropic — al subir el modelo, retira componentes que dejaron de ser load-bearing.
**Aquí**: roles `planner / generator / evaluator` son **perfiles** togglables. Métricas por rol detectan si dejan de aportar valor → sugieren desactivarlos. Ver [[foundations/design-tradeoffs]].

---

## B. La filosofía Rust llevada al harness: **estructura > prosa**

> El usuario aprecia Rust porque "todo está estructurado y es difícil saber que faltó". Trasladar esa propiedad a los artefactos del harness es el cambio de mayor impacto.

### B1 · Cada artefacto, un formato adecuado

| Artefacto | Formato | Por qué |
|---|---|---|
| Config global | TOML | humano-editable, perfiles, comentarios |
| Config por proyecto | TOML | idem |
| AGENTS.md | Markdown | prosa para el modelo, no para el harness |
| Tareas | TOML (1 archivo/task) | edición humana + diff + lock fino |
| Index de threads / tasks | SQLite | queries por estado/label |
| Event log | JSONL | append-only, streaming, pipeable |
| Specs | Markdown + frontmatter YAML | doc legible + metadata estructurada |
| Tool defs (runtime) | JSON Schema | contrato con el modelo |
| Protocolo IPC | JSON-RPC 2.0 (JSONL) | universal, debugable |
| Handoffs | Markdown | denso semánticamente, regenerable |

**Regla de oro**: si un campo importa a la **lógica**, va en TOML/YAML/JSON con schema. Si importa al **modelo**, va en markdown. Nunca mezclar significado en prosa libre que el harness deba parsear con regex.

### B2 · Schemas validados, no fe ciega

Cada `.toml`/`.yaml`/`.json` consumido por el harness tiene un **JSON Schema** versionado en `crates/harness-core/schemas/`. Validación obligatoria al leer. Beneficios:
- Atrapa typos del humano editando manualmente.
- Atrapa drift entre versiones (un thread viejo con tasks de schema antiguo migra explícitamente).
- Habilita autocompletado en VSCode (`json.schemas`).

---

## C. Continuidad local **por diseño**

### C1 · Layout en disco (canon)

```
~/.harness/
├── config.toml                          # preferencias del usuario
├── profiles/                            # roles: planner.toml, generator.toml, evaluator.toml
├── agents/                              # registry de agentes (id, kind, profile)
│   └── registry.toml
├── threads/
│   ├── index.db                         # SQLite global
│   └── <thread-uuid>/
│       ├── meta.json                    # modelo, sandbox, AGENTS.md snapshot
│       ├── spec.md                      # qué se construye (mantenido por planner)
│       ├── events.jsonl                 # event log append-only
│       ├── tasks/
│       │   ├── index.db                 # SQLite por thread
│       │   ├── T-0042.toml              # 1 archivo por task
│       │   └── ...
│       ├── handoffs/
│       │   └── handoff-<ts>.md
│       ├── budget.toml                  # límites y consumo
│       └── files/                       # adjuntos referenciados
└── logs/
```

### C2 · Test de aceptación: **roundtripability**

`harness export --thread X > t.tar` → en otra máquina → `harness import t.tar && harness resume --thread X` → el agente continúa la última task pendiente sin pérdida visible.

Si esto falla, hay un bug de continuidad — la propiedad #1 del proyecto.

### C3 · Resume desde cualquier punto
- Cualquier surface puede `thread.resume`.
- Replay reconstruye prompt en **mismo orden** → cache hit.
- Resume es la operación **default** al abrir la app si hay threads activos.

---

## D. Máquina de tareas — el corazón de la consistencia

### D1 · Una tarea, un archivo TOML

`~/.harness/threads/<id>/tasks/T-0042.toml`:

```toml
# Schema: harness.task.v1
schema_version = 1
id            = "T-0042"
title         = "Implementar paginación en lista de pedidos"
status        = "in_progress"      # ver D2
created_at    = "2026-05-26T10:00:00Z"
created_by    = "agent:planner-1"
updated_at    = "2026-05-26T11:14:22Z"
updated_by    = "agent:generator-1"

# árbol
parent        = "T-0041"
children      = []                  # poblado por planner al descomponer

# dependencias
blocked_by    = ["T-0040"]          # ids que deben estar `done`
unblocks      = []                  # inverso (mantenido en sync)

# asignación
assignee      = "agent:generator-1"
claim_lease   = { holder = "agent:generator-1", until = "2026-05-26T11:19:22Z" }

labels        = ["backend", "feature"]

[acceptance]                        # criterios verificables — al estilo sprint-contract Anthropic
checks = [
  { id = "C1", text = "endpoint GET /orders acepta page y page_size", verified = false },
  { id = "C2", text = "tests cubren última página y página vacía",     verified = false },
  { id = "C3", text = "OpenAPI actualizada",                            verified = false },
]
# pasa a `done` solo si todos verified=true y verified_by != assignee

[artifacts]
files  = ["src/orders.rs", "tests/orders_pagination.rs"]
turns  = ["turn-uuid-1", "turn-uuid-5"]     # turns donde se trabajó
diff   = "git:abc123..def456"               # rango de commits

[notes]
why_paused = ""
why_abandoned = ""

[history]
# cada transición de estado deja un evento aquí (compacto; el detalle está en events.jsonl)
events = [
  { at = "2026-05-26T10:00:00Z", by = "agent:planner-1",    from = "*",          to = "queued"      },
  { at = "2026-05-26T10:43:00Z", by = "agent:generator-1",  from = "queued",     to = "in_progress" },
]
```

**Por qué 1 archivo por task** (no `tasks.json` único):
- Locking fino (varios agentes en paralelo sin contención).
- `git blame` por task.
- Atomic write con rename trivial.
- Migración / export individual.
- Merge conflicts limitados al archivo afectado.

### D2 · Máquina de estados (canon)

```
                       ┌──────────┐
                       │ abandoned│   ◄── desde cualquier estado, solo humano
                       └──────────┘

  create        claim         submit            verify-ok
 ──────► queued ─────► in_progress ─────► pending_verify ─────► done
            ▲              │  ▲              │       │
            │              │  │              │       │ verify-fail
            │       pause  │  │  resume      │       │
            │              ▼  │              │       ▼
            │           paused┘              └──► in_progress (con feedback)
            │
            │   ◄── unblocked (auto, cuando todas las deps están done)
            │
        ┌───┴─────────────┐
        │ blocked          │   ◄── (in_progress │ queued) → blocked si aparece dep
        └──────────────────┘
```

Estados canónicos:
- `queued` — planificada, ningún agente trabaja.
- `in_progress` — un agente con `claim_lease` activo.
- `pending_verify` — el generator terminó, hay artifacts, espera evaluator.
- `paused` — pausada deliberadamente; `notes.why_paused` obligatorio.
- `blocked` — espera deps; `blocked_by` no vacío.
- `done` — todos los checks `verified=true` con `verified_by != assignee`.
- `abandoned` — desecho; `notes.why_abandoned` obligatorio.

Reglas duras:
- `queued → in_progress` requiere `claim` con lease (D3).
- `in_progress → pending_verify` requiere `artifacts.files` no vacío + el generator libera el lease.
- `pending_verify → done` requiere todos `acceptance.checks[].verified = true` **y** `verified_by != assignee` (excepto modo `solo`, que va a `done_unverified`).
- `pending_verify → in_progress` cuando el evaluator rechaza: agrega entry en `notes.feedback[]`, devuelve al pool con `verified=false` reseteados.
- `in_progress → paused` válido siempre; obliga `notes.why_paused`.
- `(in_progress|queued) → blocked` si aparece dep no resuelta.
- `blocked → queued` **automático**: el task manager observa `done` de deps y desbloquea.
- `* → abandoned` solo humano. Ningún agente abandona.

### D3 · Claim con lease (lock + heartbeat)

Bug típico: dos agentes editan la misma task. Solución estilo Codex/Postgres:

```
claim(task_id, agent_id, ttl=5min):
  flock exclusive on task.toml
  if claim_lease.holder is None or claim_lease.until < now:
     set claim_lease = { holder: agent_id, until: now + ttl }
     persist
     return OK
  else:
     return BUSY(holder=current)
```

- Heartbeat: el holder llama `renew_lease` cada `ttl/2` mientras trabaja.
- Si crashea: lease expira → otro agente puede reclamar.
- Cancelación graceful: `release_lease` libera antes de tiempo.

**Qué pasa con `assignee` al expirar lease**:
- Se mueve a `previous_assignees[]` (auditoría).
- `assignee = null` hasta el siguiente claim.
- Una entry en `history.events` marca el `lease-expired` con timestamp y holder previo.
- El estado **no cambia** automáticamente — la task sigue `in_progress` hasta que otro la reclame o un humano la pase a `paused`/`queued`. Esto evita oscilaciones.

CLI: `harness tasks stale` lista tareas con lease expirado → candidatas a recuperar.

### D4 · Qué es una buena task (granularidad)

Heurística Anthropic: un sprint complejo tenía 27+ criterios. Demasiados para una sola task del nuestro.

Reglas:
- **Atómica**: una task se puede empezar y terminar sin esperar otra task hermana.
- **Verificable**: ≤ 6 `acceptance.checks`. Si tienes 10, parte en dos.
- **Acotada**: estimación implícita ≤ 1 turn largo del generator (< 30 min de modelo).
- **Sin solapamiento de archivos** con tasks `in_progress` paralelas (el planner intenta esto al descomponer).

El planner que viole estas reglas recibe feedback del evaluator y aprende; configurable también vía constraint en su prompt.

### D5 · Scheduler (componente del core)

El scheduler es una task de fondo en `harness-core` que mantiene fluyendo el equipo.

Bucle:
```
loop {
    for thread in active_threads {
        if thread.paused_by_budget_or_human { continue }
        let ready = thread.tasks.filter(status=queued, blocked_by∅)
        let idle_agents = registry.agents_for(thread).filter(busy=false)
        match(ready, idle_agents) -> claim(task, agent)         // policy: round-robin por kind
        let to_verify = thread.tasks.filter(status=pending_verify)
        idle_evaluators -> claim(verify) -> run_verification
    }
    sleep(scheduling_tick)   // default 2s
}
```

Políticas:
- **Affinity por archivos**: si un agente ya trabajó en `src/orders.rs` recientemente, prioriza tasks que tocan ese archivo (cache de contexto local).
- **Concurrency cap por thread**: `thread.budget.max_concurrent_workers` (default 3) para no saturar al humano de cambios.
- **Cooldown tras `verify-fail`**: la misma task no se reasigna inmediatamente al mismo generator; intenta otro o espera.

Observabilidad: el scheduler loggea `scheduling.tick` con counts de cada estado. UI muestra "Próximas a ejecutar" en una columna.

### D6 · Cero ambigüedad en finalización
- `harness tasks list --status in_progress` muestra exactamente quién las tiene, hace cuánto.
- No existe "casi listo". O `acceptance.checks` están `verified=true`, o no.
- `harness tasks dep-graph` produce un DOT con dependencias y estados.

---

## E. Equipo de agentes especializados — la meta del proyecto

> "manejar un equipo de agentes especializados en desarrollo de software y ser capaces de terminar una aplicación entera o cambiar una feature en donde exista"

### E1 · Registry de agentes

`~/.harness/agents/registry.toml`:

```toml
[[agents]]
id      = "planner-1"
kind    = "planner"                # planner | generator | evaluator | custom
profile = "profiles/planner.toml"  # prompt-template, modelo, tools habilitadas
runtime = "internal"               # internal (thread del harness) | claude-cli | codex-cli

[[agents]]
id      = "generator-1"
kind    = "generator"
profile = "profiles/generator.toml"
runtime = "claude-cli"
cli     = { cmd = "claude", args = ["--profile", "harness-worker"] }

[[agents]]
id      = "generator-2"
kind    = "generator"
profile = "profiles/generator.toml"
runtime = "internal"

[[agents]]
id      = "evaluator-1"
kind    = "evaluator"
profile = "profiles/evaluator.toml"
runtime = "internal"
```

Cualquier transición de task referencia un `agent:<id>` del registry. Esto da identidad estable a través de sesiones.

### E2 · Cómo se "habla" un agente externo (Claude CLI) con el harness

Pregunta clave: si lanzo `claude` como CLI en un PTY, ¿cómo sabe del task TOML?

Dos modos, configurables:

1. **MCP harness-bridge** (preferido)
   - El harness expone un servidor MCP local con tools: `task.claim`, `task.update_check`, `task.append_artifact`, `task.release`, `spec.read`.
   - `claude` se lanza con `--mcp-config` apuntando a este server.
   - El agente, por su prompt-template, sabe llamar estas tools.
   - Beneficio: idiomático para el CLI; sin parsear texto.

2. **Tool-via-prompt**
   - El prompt-template incluye instrucciones de escribir/leer archivos TOML directamente bajo `~/.harness/threads/<id>/tasks/`.
   - El agente usa su tool nativa de FS.
   - Beneficio: cero infra adicional; costo: más frágil al modelo.

Los `internal` runtimes (threads del harness-core) usan capa nativa Rust → más rápidos y robustos. Se reservan los CLI externos para reutilizar tooling del usuario.

### E3 · Orquestación (file-based)

**Patrón**: nadie habla "en vivo". Todo deja rastro en archivos.

```
                ┌─────────────────┐
                │   spec.md       │   ← planner mantiene
                └────┬────────────┘
                     │ descompone
                     ▼
                ┌─────────────────┐
                │ tasks/*.toml    │   ← grafo de tareas (DAG)
                └────┬────────────┘
   claim (lease)     │
   ┌─────────────────┴────────────────────┐
   ▼                                      ▼
generator-1 (T-0042)            generator-2 (T-0043)
   │                                      │
   │ artifacts                            │
   ▼                                      ▼
src/orders.rs                    src/api.rs
   │                                      │
   │ marca acceptance.checks*[verified=false→pending_verify]   
   └─────────────────┬────────────────────┘
                     ▼
              ┌───────────────┐
              │ evaluator-1   │  ← tomas pending_verify, decide done o devuelve
              └───────────────┘
```

Beneficios (heredados de Anthropic):
- **Reanudable**: si todos caen, al despertar el estado está en disco.
- **Auditable**: `git log threads/<id>/tasks/` cuenta la historia entera.
- **Determinista**: dos runs con el mismo estado inicial convergen.
- **Inspectable**: el humano puede entrar a cualquier punto y ver qué pasa sin pausar nada.

### E4 · Walk-through: "construir una app TODO"

```
Usuario en UI:  "Quiero una app TODO con SvelteKit + SQLite, deploy a Vercel"

1. Director (planner-1) crea thread T:
   - spec.md con secciones: stack, modelo de datos, endpoints, UI, deploy
   - tasks descompuestas (15-20 nodos), enraizadas, con blocked_by entre sí
   - budget.toml: cap $20 USD, max 8h wallclock

2. Scheduler interno del harness recoge tasks `queued` cuyo blocked_by está vacío.
   - generator-1 claim T-0001 (init repo)
   - generator-2 claim T-0002 (data model)  // en paralelo, files distintos
   
3. Cada generator:
   - lee spec.md (o un slice relevante)
   - implementa, persiste en files/ o directo en el workspace del usuario
   - llena artifacts.files, artifacts.diff
   - marca acceptance.checks pendientes
   - status → pending_verify; release lease

4. evaluator-1 toma pending_verify:
   - corre tests (tool `shell.exec` sandbox)
   - revisa contra spec
   - marca checks verified=true o devuelve con notes (status → in_progress)

5. Al cerrar T-0002 (done):
   - task manager dispara unblock de T-0003 (blocked_by=[T-0002]) → queued
   - scheduler lo entrega a un generator libre

6. Bucle hasta que todas las tasks raíz estén done.
   - Director emite reporte final: spec cumplida, tests pasando, deploy URL.
```

### E5 · Modo "feature en proyecto existente"
Mismo flujo pero `spec.md` se inicializa con:
- Snapshot del repo (`ls -R`, `git log --oneline -20`, AGENTS.md).
- Descripción humana de la feature.
- Limitaciones: tasks **no pueden** tocar archivos fuera de los listados en `spec.scope.files` salvo `spec.scope.files_extension_allowed = true` (con review humano).

---

## F. Salvaguardas para autonomía

Un equipo autónomo puede gastar mucho rápido. Sin estos límites, el proyecto es irresponsable.

### F1 · Budget por thread (`budget.toml`)

```toml
schema_version = 1
[caps]
usd_max          = 20.00
tokens_max       = 5_000_000
wallclock_max_s  = 28_800            # 8h
turns_max        = 200

[consumed]
usd     = 3.42
tokens  = 412_330
elapsed = 5_400
turns   = 18

[on_soft_cap]
threshold = 0.80                     # al 80% de cualquier cap → warning, no pausa
notify    = ["desktop-toast"]

[on_cap]
action  = "pause"                    # pause | abort | notify-only
notify  = ["desktop-toast"]
```

Comportamiento:
- **Soft cap (80%)**: notification + entry en `events.jsonl`; nada se detiene. Da chance al humano de subir el cap antes del corte.
- **Hard cap (100%)**: el task manager **pausa** todas las tasks `in_progress` del thread; marca `paused` con `why_paused = "budget cap reached: usd"`. Humano decide subir cap o abandonar.
- `consumed` se persiste en cada `turn.completed` (no diferido) → si el proceso muere, el contador sobrevive.

### F2 · Kill-switch global
- `harness pause --all` pausa todos los threads.
- Atajo de teclado en UI (`Cmd/Ctrl+Shift+.`).
- Estado persistente: al re-abrir, `paused` se respeta hasta resume explícito.

### F3 · Approval-gate para acciones irreversibles
- `git push`, `npm publish`, `terraform apply`, `rm -rf`: aprobación humana obligatoria (no overrideable por `auto`).
- Lista en `cross-cutting/security-model.md` (whitelist de cmds peligrosos).

### F4 · Observabilidad por defecto
**Fuente Codex**: hash del prefix por request, métricas por turn.
**Aquí**:
- `tracing` spans: `thread.run > turn.run > tool.exec`.
- Métricas: tokens IN/OUT, costo, cache hit-rate, # tools/turn, duración por estado de task.
- Todo a disco bajo `~/.harness/logs/` rotado.
- UI muestra panel "Live cost" con descomposición por agente.

### F5 · Errores como datos
**Aquí**: `ToolError`, `RpcError`, `TaskError` tipados con `thiserror`, serializables. El agente recibe causa + sugerencia ("Try X"), no un stack trace. Logs guardan trace completo; el modelo recibe lo destilado. Ver [[cross-cutting/error-model]].

---

## H. Auto-mejora — qué aprender de Hermes Agent (Nous Research)

> Hermes introduce un **closed-loop learning system**: el agente crea sus propias capacidades, las refina con el uso y un componente de fondo las cura. Es la pieza que **les falta** a los harnesses Codex/Claude — y la que más alinea con la meta del usuario ("un equipo capaz de terminar una aplicación entera").

### H1 · Skills como memoria procedimental

**Hermes**: Skills son archivos **Markdown con frontmatter YAML** en `~/.hermes/skills/`. Cada uno: pasos, pitfalls, criterios de verificación. Es exactamente el formato de un shard de docs (feliz convergencia).

**Aquí**: introducir el concepto `Skill` como artefacto persistente del agente. Layout sugerido:

```
~/.harness/
└── skills/
    ├── index.db                          # SQLite: telemetría de uso
    ├── .usage.json                       # contadores ligeros (mirror del index)
    ├── .skill_backups/                   # snapshots tar.zst antes de cada curator pass
    ├── .archive/                         # skills movidas, recoverable
    ├── agent_created/                    # skills generadas por el agente
    │   ├── refactor-svelte-store.md
    │   ├── debug-postgres-deadlock.md
    │   └── ...
    ├── bundled/                          # skills enviadas con el harness (read-only)
    └── hub/                              # skills instaladas desde un hub público (read-only)
```

Schema del frontmatter (validado):
```yaml
---
id: refactor-svelte-store
title: Refactor de un store Svelte a derived
source: agent_created                    # agent_created | bundled | hub
created_at: 2026-05-20T10:00:00Z
created_by: agent:generator-1
triggers:                                # cuándo recordar/cargar la skill
  intents: ["refactor svelte stores", "convert writable to derived"]
  file_patterns: ["**/*.svelte", "**/stores/*.ts"]
verification:                            # cómo saber si funcionó al aplicarla
  - "svelte-check sin nuevos errores"
  - "tests del store pasan"
state: active                            # active | stale | archived
pinned: false
patch_count: 3
use_count: 12
last_used_at: 2026-05-26T09:14:00Z
---
```

Cuerpo en markdown libre: procedimiento + pitfalls + ejemplos.

### H2 · El Learner — creación autónoma de skills

**Hermes** dispara `skill_manage(action="create")` cuando:
- El agente completa una tarea compleja (≥ 5 tool calls).
- Hay error / dead-end y luego se encuentra la ruta correcta.
- El usuario corrige la trayectoria.
- Se descubre un workflow reusable.

**Aquí**: el "Learner" no es un agente separado, es una **policy** del task manager + hooks en el agent loop:

```
on turn.completed:
  if turn.tool_calls.len() >= 5 AND turn.outcome == success:
     propose_skill_extract(turn) → genera draft en skills/agent_created/<slug>.md
  if turn.had_correction (user message corrected the path):
     propose_skill_patch_or_create(turn)
  if turn.had_dead_end_then_recovery:
     propose_skill_create(turn, focus="recovery path")
```

Proponer **no** es ejecutar. La política por defecto:
- `auto`: crea/parcha la skill y la persiste.
- `proposed`: deja un archivo `~/.harness/skills/proposed/*.md` para review humano antes de promover.
- `off`: solo registra una sugerencia en logs.

Para v1 recomendamos `proposed`: alinea con la filosofía de **estructura visible** del usuario (Rust-like) — nada cambia sin que se vea.

### H3 · skill_manage — tool del agente

Tool nativa expuesta al modelo (ver [[harness-core/tool-execution]]):
```jsonc
{
  "name": "skill_manage",
  "params": {
    "action": "create | patch | edit | delete | write_file | remove_file",
    "skill_id": "refactor-svelte-store",
    "body": "...",                          // para create/edit
    "patch": { "checks": ["..."] },         // para patch (delta JSON Patch RFC 6902)
    "reason": "Por qué"
  }
}
```

`patch` es preferida sobre `edit` (Hermes): menos tokens, diff trazable. El harness aplica `patch` y registra `patch_count++`.

### H4 · El Curator — mantenimiento en segundo plano

**Hermes**: dos fases:
1. **Determinística**: skills sin uso `≥ stale_after_days (30)` → `stale`; `≥ archive_after_days (90)` → mover a `.archive/`.
2. **LLM review** (forked agent, prompt cache propia, hasta 8 iteraciones): por cada skill decide `keep | patch | consolidate | archive`.

Trigger: `interval_hours = 7d` desde el último run **y** `min_idle_hours = 2h` de agente ocioso.

**Aquí**: el Curator es un **agente interno especial** del harness:
- Perfil `profiles/curator.toml`: prompt template + tools restringidas (`skill_view`, `skill_patch`, `skill_archive`, NO `skill_delete`).
- Corre como `internal` runtime (thread del core).
- Snapshot tar.zst antes de cada pasada en `~/.harness/skills/.skill_backups/<ts>.tar.zst`.
- Genera reportes en `~/.harness/logs/curator/<ts>/`:
  - `run.json` — máquina-legible.
  - `REPORT.md` — humano-legible.

**Reglas duras** (heredadas de Hermes):
- **Nunca auto-elimina**. Lo peor que pasa es archivar (recuperable con `harness skills restore <id>`).
- Solo toca `agent_created/`. **No** modifica `bundled/` ni `hub/`.
- Skills `pinned: true` son inmunes a transiciones automáticas y a borrado por el agente; el `skill_manage` del agente puede patchearlas pero no eliminarlas.

CLI:
```
harness curator status                    # último run, counts, pinned, LRU
harness curator run [--background|--dry-run]
harness curator backup | rollback | restore <skill>
harness curator pin | unpin <skill>
harness curator pause | resume
harness curator prune                     # ofrece archivar candidatos
```

### H5 · GEPA — aprender de los fallos sin auto-engaño

**Hermes** usa **GEPA** (Genetic-Pareto Prompt Evolution): un proceso **offline** que lee execution traces, identifica failure points y propone variantes evolutivas. **Genera PRs, no commits**. ~$2–10 USD por run.

Por qué es importante: **el agente no es buen juez de sí mismo** (sesgo Anthropic §A5). Un componente offline que ve los traces sin involucrarse en el rol del operador da una segunda opinión.

**Aquí**: replicar el patrón sin atarse al algoritmo concreto:
- Cada `events.jsonl` es un trace evaluable.
- Proceso `harness gepa --since 1w --target profile.generator`:
  1. Carga traces de la última semana del rol `generator`.
  2. Identifica turns que fallaron (rejected por evaluator, retry > 1, costo anómalo).
  3. Llama a un modelo "judge" (separado y configurable) que propone N variantes del prompt-template + N variantes de las skills más involucradas.
  4. Evalúa variantes contra un set de tasks-target reproducibles.
  5. Emite un **Pull Request** al repo de configs: `profiles/generator.toml` y/o `skills/agent_created/*.md`, con métricas comparativas.

Crítico: el PR es **revisado por humano** (o por el evaluator en modo solo). Cero auto-mutación silenciosa de prompts.

### H6 · Auditoría de la evolución (la lección de Roan Monteiro)

> "A self-learning agent without auditing isn't dangerous because it's autonomous. It's dangerous because its behavior changes in ways you don't see, don't track, and therefore can't correct."

Esto colisiona frontalmente con la **filosofía Rust** del usuario (estructura visible, nada implícito). Salvaguardas:

- Toda mutación de skill/prompt deja entrada en `~/.harness/skills/<id>.history.jsonl` (append-only).
- `harness skills diff <id> --since <ts>` muestra evolución.
- Los snapshots del Curator viven 90 días (configurable). Permite `rollback`.
- Métricas por skill: `success_rate_used`, `success_rate_unused` (counterfactual estimado). Si una skill *baja* el éxito, el Curator la marca `under_review` y la deja sin patchear hasta intervención humana.
- Las skills bajo `agent_created/` deben ir a un repo git (opcional): el usuario obtiene `git log` gratis sobre la evolución.

### H7 · Tres niveles de memoria (Hermes → nuestro mapeo)

Hermes divide explícitamente memoria en tres archivos canónicos cargados por `prompt_builder.py`:

| Hermes | Qué guarda | Nuestra contraparte |
|---|---|---|
| `SOUL.md` | personalidad del agente, estilo, valores | `profiles/<rol>.toml` + plantilla `system.md` por perfil |
| `USER.md` | modelo de **usuario** a largo plazo (preferencias, rol, contexto) | `~/.harness/memory/USER.md` — escribible por un sub-agente "psicólogo" (stretch v1.1) |
| `MEMORY.md` | memoria **episódica** indexada (decisiones tomadas, hechos) | `~/.harness/memory/MEMORY.md` (índice) + FTS5 sobre `events.jsonl` |

| Nivel | Carga | Tamaño objetivo |
|---|---|---|
| **Identidad / SOUL** | siempre al inicio del thread | < 2 KB |
| **Usuario / USER** | siempre al inicio del thread (filtrado por perfil) | < 4 KB |
| **Episódica / MEMORY + FTS5** | índice top-level siempre; búsqueda on-demand via tool `memory.search` | índice < 4 KB |

Reglas (heredadas del [[meta/conventions]] de docs y del auto-memory):
- `MEMORY.md` es **índice**, no contenido. Cada entrada apunta a un archivo bajo `memory/*.md`.
- `USER.md` se actualiza solo cuando hay aprendizaje no-obvio sobre el usuario. No es un log.
- Memorias con `name`/`description`/`type` en frontmatter → buscables por filtro.

Para v1: nivel 1+3. Nivel 2 (modelo de usuario evolutivo) queda como stretch.

### H8 · Skills vs Tasks vs Memory — qué guarda qué

Tres artefactos persistentes; cada uno con un rol claro:

| Artefacto | Vida | Quién escribe | Para qué |
|---|---|---|---|
| **Task** (`tasks/*.toml`) | corta (días) | planner + workers + evaluator | el "qué hay que hacer ahora" |
| **Skill** (`skills/*.md`) | larga (meses) | learner + curator | el "cómo se hace bien una clase de tareas" |
| **Memory** (`MEMORY.md` + FTS5) | larga (siempre) | agente + humano | quién es el usuario, qué decisiones se tomaron, contexto |

No mezclar. Una task no es una skill (no se reusa textualmente; se completa y se cierra). Una skill no es memoria (no es sobre el usuario, es sobre la técnica).

### H9 · Composabilidad: skills + tasks + equipo

El payoff conjunto:

1. **Planner** descompone una request → tasks.
2. Al asignar una task a un generator, el harness hace `skills.search(task.title + task.acceptance.checks)` y **carga las top-K skills relevantes** en el prompt inicial del worker.
3. El worker resuelve la task. Si genera ≥5 tool calls o tuvo recovery → learner propone una skill nueva o patch.
4. El evaluator verifica la task. Si rechaza, registra el motivo en el trace.
5. Una vez por semana / agente idle 2h → curator pasa.
6. Mensualmente → GEPA evolutiona prompts y skills de bajo rendimiento → PR.

Resultado: el equipo **se vuelve más rápido y más barato con el tiempo** sin perder auditabilidad.

### H11 · Patrones arquitectónicos de Hermes que vale la pena adoptar

De `hermes-agent.nousresearch.com/docs/developer-guide/architecture`:

#### a) Tool registry como raíz del grafo de dependencias
**Hermes**: `tools/registry.py` sin dependencias internas; cada tool se auto-registra al importarse (`registry.register()` top-level). Esto rompe ciclos y permite añadir tools sin tocar el core.

**Aquí**: análogo en Rust con `inventory` o un macro `#[harness_tool]` que registra en `linkme` distributed slice. El `harness-core` no conoce las tools concretas; las descubre al arrancar.

#### b) Provider abstraction por "API mode"
**Hermes** soporta 3 API modes: `chat_completions`, `codex_responses`, `anthropic_messages`. Un `runtime_provider.py` resuelve `(provider, model) → (api_mode, key, base_url)`.

**Aquí**: ya planteado en `harness-llm` (ver [[harness-core/rust-crate-layout]]). Confirmamos que **el api_mode es ortogonal al provider**: GPT-OSS local puede hablar `codex_responses`. Conservar esa flexibilidad.

#### c) Context engine pluggable
**Hermes**: `context_engine.py` es abstract base; `context_compressor.py` (lossy summary) es la implementación default; otros vienen como plugins.

**Aquí**: trait `ContextEngine { fn maybe_compact(&self, history: &[Item]) -> Option<Compaction>; }` con implementaciones intercambiables. Default = provider-native compaction; alternativa = handoff-style reset (ver [[harness-core/context-compaction]]).

#### d) Profile isolation
**Hermes**: cada `hermes -p <name>` tiene su propio `HERMES_HOME`, config, sessions, PID del gateway. Permite varios "yo" en la misma máquina sin chocar.

**Aquí**: replicar exactamente.
```
~/.harness/
├── profiles/
│   ├── personal/                      # HARNESS_HOME=$HOME/.harness/profiles/personal
│   │   ├── config.toml
│   │   ├── threads/...
│   │   └── skills/...
│   └── work/
│       └── ...
└── active_profile -> profiles/personal/    # symlink
```
CLI: `harness -p work` o env `HARNESS_PROFILE=work`. Cada perfil con su propio App Server PID.

#### e) Hook system + cron como primera clase
**Hermes**: hooks en `gateway/hooks.py`, cron jobs almacenados como JSON con skills adjuntas. Un cron job es un agente prog­ramable que puede usar skills.

**Aquí**:
- Hooks de ciclo: `on_turn_start`, `on_turn_end`, `on_task_done`, `on_curator_pass`. Plugins se enganchan.
- Cron jobs: archivo `~/.harness/cron/<id>.toml` con `schedule`, `prompt`, `attached_skills`, `target` (qué thread / qué módulo). Scheduler del core los corre.

#### f) Trajectories — exporta data para entrenamiento
**Hermes**: `agent/trajectory.py` exporta sesiones a ShareGPT format → material de fine-tuning.

**Aquí**: stretch. `harness export trajectories --since 1w --redact` genera un dataset reproducible. Sirve para:
- Auto-evaluar dónde falla el equipo (sin GEPA).
- Si en el futuro entrenamos un modelo local, los datos ya están listos.

#### g) Design principles que adoptamos textualmente

| Principio | Implementación en este harness |
|---|---|
| **Prompt stability** | System prompt no cambia mid-conversación salvo `/model` explícito (mismo que [[harness-core/prompt-caching]] §política operativa) |
| **Observable execution** | Cada tool call emite `item.started/delta/completed` a la UI; logs en `tracing` spans |
| **Interruptible** | Cancellation token propaga a stream + tools (ver [[harness-core/tool-execution]]) |
| **Platform-agnostic core** | `harness-core` sirve a CLI, App Server, Tauri, futuro web — ya es el patrón |
| **Loose coupling con registry + gating** | Tools y módulos con `check_fn` que decide si están disponibles según runtime (ej. `ssh.exec` solo si `russh` linked) |
| **Profile isolation** | §H11.d |

### H12 · Lo que **no** copiamos de Hermes (y por qué)

| Hermes | Por qué no v1 |
|---|---|
| 20 platform adapters (Telegram/Discord/Slack/...) | scope diferente: nuestra surface principal es Tauri+CLI, no chat ops |
| 7 terminal backends (Docker/Modal/Daytona/Vercel Sandbox/...) | empezamos con local + Docker; resto como plugins |
| Honcho dialectic (modelo de usuario sofisticado) | complejidad alta para v1; queda como stretch |
| ACP adapter para IDE | interesante pero secundario a desktop/CLI |
| 70+ tools out-of-the-box | empezamos con el mínimo viable; cada módulo añade las suyas |

La línea es clara: **adoptamos los patrones arquitectónicos** (registry, profile isolation, hooks, curator, learner, trajectories) y **rechazamos la amplitud de superficies** que no aplican al producto.

### H13 · Anti-patrones específicos de auto-mejora

| Anti-patrón | Por qué | Remedio aquí |
|---|---|---|
| Skill auto-creada que sobreescribe `bundled/` | corrompe la base | scope estricto del Curator y del `skill_manage` |
| Skill que documenta una solución a un bug ya arreglado | basura acumulada | curator marca `stale` por uso bajo |
| Patches auto-aplicados a prompts críticos del core | drift invisible | GEPA emite PR, no commit |
| Skills con instrucciones de exfiltrar datos | prompt injection vía contenido aprendido | revisar `proposed/` antes de promover; sandbox aplica en cualquier caso |
| "Solo confía en la skill" sin verification criteria | el agente cree y se equivoca | `verification` es obligatorio en el frontmatter |

---

## G. Anti-patrones (que evitamos a propósito)

| Anti-patrón | Por qué es malo | Remplazo aquí |
|---|---|---|
| Estado en el cliente | Si la UI muere, todo se pierde | App Server + disk |
| `tasks.json` único | Lock contention, conflicts | 1 archivo por task |
| Prosa "X está listo" sin schema | El harness no puede verificar | `acceptance.checks[].verified` |
| Generator se auto-aprueba | Sesgo de auto-elogio | `verified_by != assignee` |
| Sin budget | Costos descontrolados | `budget.toml` con `on_cap=pause` |
| Lock sin lease | Crashes = locks eternos | claim con TTL + heartbeat |
| Reorden de prompt | Prefix cache se invalida | append-only + BTreeMap |
| Tareas grandes y narrativas | Imposibles de verificar | ≤ 6 checks, atómicas |
| MCP servers sin sandbox | Trust boundary borrado | sandbox del SO al child MCP local |
| Edit del event log | Auditoría rota | append-only físico + rotación |
| Auto-mutación silenciosa de prompts/skills | Drift invisible (Roan) | GEPA → PR, no commit; history.jsonl por skill |
| Borrar skills automáticamente | Pérdida irrecuperable | Curator solo archiva, nunca borra |
| Curator tocando skills bundled | Corrompe la base | Scope estricto a `agent_created/` |

---

## Resumen ejecutivo

| Lección | Implementación |
|---|---|
| Núcleo único Rust | `harness-core` librería |
| Estado fuera del cliente | `harness-app-server` sidecar Tauri |
| Append-only | `events.jsonl` + prompt builder con `BTreeMap` |
| Continuidad local | `~/.harness/threads/<id>/` exportable |
| Tareas estructuradas | 1 TOML por task + máquina de estados explícita |
| Claim con lease | TTL 5min + heartbeat, recuperable |
| Equipo coordinado por archivos | spec.md → tasks/ → artifacts/ |
| Identidad estable de agentes | `agents/registry.toml` con `agent:<id>` |
| Externos (Claude CLI) | vía MCP harness-bridge tools |
| Generator ≠ Evaluator | `verified_by != assignee` (excepto modo solo) |
| Budget + kill-switch | `budget.toml on_cap = pause` |
| Schemas validados | JSON Schemas versionados en `harness-core/schemas/` |
| Observabilidad | tracing spans + métricas + panel live cost |
| Roundtripability | export/import/resume sin pérdida |
| **Skills (Hermes)** | `~/.harness/skills/agent_created/*.md` con frontmatter YAML |
| **Learner (auto-creación)** | policy on `turn.completed` (≥5 tools, recovery, corrección) → `proposed/*.md` |
| **Curator (auto-mantenimiento)** | agente fork, 2 fases (determinístico + LLM); nunca borra, solo archiva |
| **GEPA (auto-mejora offline)** | reads traces → propone PR (no commit) — humano aprueba |
| **Profile isolation** | `~/.harness/profiles/<name>/` con HARNESS_HOME propio |
| **Auditoría de evolución** | `<skill>.history.jsonl` + snapshots tar.zst + `harness skills diff` |

> Nada de esto depende del modelo concreto. Si subimos Sonnet → Opus, los roles `planner`/`evaluator` pueden desactivarse, pero la base estructural (tasks, eventos, schemas, budgets) sobrevive y sigue garantizando consistencia. Esa es la ventaja Rust: la forma del problema no se borra.
