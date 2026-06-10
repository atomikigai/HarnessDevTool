# CLAUDE.md — Equipo nativo de desarrollo de HarnessDevTool

Este archivo lo lee **Claude Code** automáticamente al iniciar sesión en este repo. Define el
**equipo de desarrollo** que mejora el harness "desde afuera" (Claude Code nativo + CLIs externos),
distinto del modelo de roles *runtime* que el harness implementa en `docs/13-agents/`.

> Reusa a propósito el vocabulario de `docs/13-agents/` (`planner / generator / evaluator`) para que
> el equipo que **construye** el harness hable el mismo idioma que el harness **construye**. Eso
> prepara el terreno para el dogfooding (ver §6).

Antes de escribir código, todo agente lee también `AGENTS.md` y `docs/ARCHITECTURE.md` /
`docs/README.md`.

---

## 1. Roster y roles (2026-06-10)

| Rol (runtime) | Quién | Cómo se invoca | Scope de escritura |
|---|---|---|---|
| **Planner / Orquestador** (`planner`) | Claude Opus (loop principal) | nativo | ❌ no edita código; orquesta y verifica |
| **Codificador** (`generator`) | **Codex gpt-5.5** | `codex exec` vía Bash (§3) | `backend/crates/**` + `frontend/**` |
| **Revisor de código** (`evaluator`) | **Claude Sonnet 4.6** (subagente) | **Agent tool** nativa + sub-modelo | ❌ aconseja en 1 ronda; no parchea |
| **UI designer** (`evaluator`) | **Claude Sonnet 4.6** (subagente) | **Agent tool** nativa + sub-modelo | ❌ revisa visual (CSS, responsive, a11y) |
| **Doc-agent** (`generator`) | **Claude Haiku 4.5** (subagente) | **Agent tool** nativa (§3) | `docs/**` |
| **QA funcional** (`evaluator`) | **Codex gpt-5.5** (agent-browser) | `codex exec` vía Bash (§3) | ❌ tests con browser; solo reporta |

Reglas de rol:

- El **Planner no edita código**: descompone, redacta el brief, delega, y verifica que se cumplió el
  objetivo. Es el hub de comunicación.
- **Codificador (Codex gpt-5.5)** cubre `backend/crates/**` + `frontend/**`. Genera código para ambos
  dominios. Aquí el backend es **Rust de sistemas** (PTY, MCP, scheduler, policy), no CRUD. No se parte
  en "logic/UI".
- **Crates de alto riesgo** — `harness-session` (PTY), `harness-policy`, `harness-mcp-server`: el
  Planner los revisa de cerca o delega con validación de Sonnet revisor. **No** acepta cambios sin
  1 ronda de revisión. `cargo check` verde ≠ correcto en código de sistemas.
- **Revisor de código (Sonnet 4.6)** valida correctitud, regresiones y arquitectura en **1 sola ronda**.
  Aconseja, no parchea. El generador (Codex) es dueño del código final e incorpora feedback.
- **UI designer (Sonnet 4.6)** revisa frontend visual (`*.svelte`, CSS, responsive, a11y, shadcn consistency)
  en **1 sola ronda**. Genera feedback, el codificador incorpora.
- **Doc-agent (Haiku 4.5)** edita `docs/**` (documentación, changelogs, backlog).
- **QA funcional (Codex gpt-5.5)** corre tests con agent-browser e-2-e; solo reporta.
- **Regla universal cross-model (cap=1):** revision nunca por el mismo modelo que generó. Revisor y
  generador son de distinto CLI (Codex vs Sonnet aquí). Solo para trabajo no trivial. Compuerta objetiva:
  tests/fmt verdes + criterio de aceptación, no opinión del revisor.
- Cualquier ejecutor con una duda la escribe en el board (§4) y espera al Planner; no asume.

---

## 2. Ciclo de vida de una tarea (cap=1 ronda, cross-model)

```
PLAN (Planner)
  └─ brief + contrato en docs/teamwork/BOARD.md
        └─ CODIFY (Codex backend+frontend, write scopes separados si aplica)
              └─ REVIEW (Sonnet revisor código + Sonnet UI designer, 1 sola ronda)
                    └─ INCORPORATE (Codex ajusta por feedback, dueño de código final)
                          └─ QA (Codex agent-browser)  →  VERIFY objetivo (Planner)
                                └─ si falla QA: vuelve a CODIFY con notas. Si pasa: cerrar en el board.
```

- **PLAN**: el Planner escribe en el board objetivo, alcance, archivos probables, criterio de
  aceptación, responsables y **contrato** front/back si aplica.
- **CONTRATO**: antes de ejecutar, Codex publica rutas, métodos, payloads, respuestas, errores
  **y los tipos `ts-rs` afectados**. Aplica a cambios de API o tipos. Si cambia, Codex actualiza
  el board.
- **CODIFY**: Codex codifica backend + frontend con write scopes separados. Si necesitan el mismo
  archivo, el Planner serializa esa parte.
- **HANDOFF**: Codex anota en el board "listo para review" con archivos tocados, comandos y comprobaciones
  locales.
- **REVIEW**: Sonnet revisor valida correctitud, regresiones, arquitectura (1 ronda). Sonnet UI designer
  revisa visual frontend (1 ronda). Ambos aconsejan, no parchean. El generador (Codex) es dueño.
- **INCORPORATE**: Codex ajusta por feedback (cap=1 ronda). Compuerta objetiva: tests/fmt verdes
  + criterio de aceptación, no opinión del revisor.
- **QA**: Codex corre agent-browser (tests e-2-e) contra criterio de aceptación.
- **VERIFY**: el Planner confirma objetivo cumplido y la puerta de calidad de §5 verde.

---

## 3. Comandos de invocación (headless)

Correr siempre desde la **raíz del repo**.

### Codificador (Codex gpt-5.5)
```bash
codex exec -s workspace-write -c approval_policy=never --skip-git-repo-check \
  "BRIEF Codex: <objetivo>. Sigue AGENTS.md y docs/. Alcance: backend/crates/<crate> y/o frontend/. \
Criterio: <aceptación>. Si tocas un tipo #[derive(TS)], corre just gen-types. Contrato: [detalle \
API/tipos en BOARD.md]." \
  < /dev/null
```

Codex codifica **backend + frontend** (ambos dominios). El prompt **debe** ir como argumento posicional
y hay que redirigir **`< /dev/null`** (fix headless 2026-06-10 validado). `approval_policy=never` evita
prompt de aprobación sin TTY; `-s workspace-write` confina al repo. Opciones útiles: `--json` (stream
de eventos), `--output-last-message FILE`, `--ephemeral`.

Tras codificar, Codex anota el handoff en el board (archivos, comandos locales, comprobaciones).

### Revisor de código (Claude Sonnet 4.6 — subagente nativo)
Se spawnea con la **Agent tool** nativa (`subagent_type: reviewer`, modelo Sonnet 4.6 como sub-modelo
de Opus), **no** por CLI. Lee el handoff de Codex en el board. Valida en **1 sola ronda**: correctitud,
regresiones, arquitectura, contrato API/tipos. Aconseja, no parchea. Devuelve su análisis como tool result
(el Planner lo registra en el board).

### UI Designer (Claude Sonnet 4.6 — subagente nativo)
Se spawnea con la **Agent tool** nativa (`subagent_type: ui-designer`, modelo Sonnet 4.6 como sub-modelo),
**no** por CLI. Si Codex tocó `frontend/`, Sonnet revisa en **1 sola ronda**: CSS, responsive, a11y,
shadcn consistency, densidad visual. Aconseja, no edita. Devuelve feedback como tool result.

### Doc-agent (Claude Haiku 4.5 — subagente nativo, rápido)
Se spawnea con la **Agent tool** nativa (`subagent_type: doc-agent`, def. en `.claude/agents/doc-agent.md`,
modelo Haiku 4.5 por velocidad/costo). Edita **`docs/**`** (y, si se le pide, `README`/comentarios de
doc): actualizar docs, changelogs, notas de tareas, sincronizar el estado del backlog. No toca código
de `backend/**` ni `frontend/**`. Devuelve handoff como tool result.

### QA funcional (Codex gpt-5.5 — agent-browser)
```bash
codex exec -s workspace-write -c approval_policy=never --skip-git-repo-check \
  "BRIEF QA Codex: corre agent-browser contra criterio de aceptación en BOARD.md. \
Valida: [flujos e-2-e listados]. Reporta resultados, screenshares, logs. No edites código." \
  < /dev/null
```

Codex con agent-browser corre tests e-2-e. El Planner o Sonnet revisor pueden también hacer QA
funcional observacional, pero Codex es más rápido con el browser.

**Notas integrales:**
- **Codex (gpt-5.5)**: CLI externo para Backend + Frontend + QA (agent-browser). Se invoca por `codex exec`
  vía Bash con `< /dev/null`.
- **Sonnet 4.6**: Revisor de código + UI designer, subagentes Claude nativos (Agent tool).
- **Haiku 4.5**: Doc-agent, subagente nativo (Agent tool).
- **Dueño de código:** Codex. Revisor aconseja (cap=1), Codex incorpora feedback.
- **Cross-model:** Revisor (Sonnet) ≠ Generador (Codex), nunca el mismo modelo revisándose a sí mismo.

---

## 4. Comunicación y coherencia

- **Board compartido:** `docs/teamwork/BOARD.md`. Canal común con Codex (CLI externo stateless).
  El Planner abre/cierra; cada ejecutor anota archivos tocados, cómo probar y preguntas. Plantilla
  estricta por campos (no prosa libre). **Límite conocido**: una tarea "en curso" a la vez, sin
  locking real — mitigado porque Revisor/QA son subagentes nativos, no escritores del board.
- **Contrato API + tipos compartido:** toda tarea full-stack lleva en el board endpoints, método,
  payload, response, errores **y tipos `ts-rs`**. Backend es dueño; Frontend lo consume.
- **Contexto en el prompt:** cada CLI es stateless entre invocaciones; el Planner inyecta el contexto
  necesario y apunta al board y a los docs.
- **Decisiones de producto** que no se deducen del código → las resuelve el usuario vía el Planner.

---

## 5. Reglas de casa específicas del harness (detalle en `AGENTS.md` y `docs/`)

- **Append-only**: el log de conversación nunca se reescribe; toda "edición" es un evento nuevo.
- **`X-Protocol-Version`**: todo request/response HTTP declara la versión; mismatch → error explícito.
- **`ts-rs` es la fuente de verdad de tipos** (type-bridge gate): quien toque un tipo `#[derive(TS)]`
  **debe** correr `just gen-types`; **nunca** editar a mano `frontend/src/lib/api/types/`. El Planner
  lo verifica en VERIFY.
- **Puertos locales dinámicos**: `just dev`/`just dev-raw`/`just docker-dev` eligen puertos altos libres
  si `BACKEND_PORT`/`FRONTEND_PORT` no están definidos; `HARNESS_CORS_ORIGIN` se deriva del frontend.
- **`HARNESS_HOME`**: raíz de estado (default `~/.harness`, `/data` en container).
- **Propiedad por dominio** (no cruzar paths): backend `backend/**`, frontend `frontend/**`,
  infra/raíz `Justfile`/`docker-compose*.yml`/`.env.example`/`.gitignore`/`AGENTS.md`, docs `docs/**`.
- **VERIFY de verdad, no solo compilación**: la puerta de cierre es `just test` (cargo + pnpm) **y**
  correr el endpoint/flujo afectado cuando sea viable (`just dev-backend` + curl), no solo
  `cargo check` / `pnpm check`. Si tocaste tipos, `just gen-types` antes.
- **No commitear ni pushear** salvo que el usuario lo pida.

---

## 6. Dogfooding — hito condicionado (aún no)

La meta es que el harness se desarrolle a sí mismo (usar HarnessDevTool para mejorar
HarnessDevTool). El gate histórico era: 10 P0 cerrados **y** sesiones rehidratadas tras
reinicio; ambos quedaron cerrados el 2026-06-04 (ver `docs/12-build-plan/improvement-plan.md`).

**Criterio operativo para dogfooding:** iniciar con tareas pequeñas y reversibles, manteniendo
review/QA externo hasta que replay/debug y reconciliación den suficiente observabilidad. Buen
criterio de madurez: *el harness está listo cuando puede desarrollarse a sí mismo.*

---

## 7. Backlog

El trabajo pendiente vive en `docs/12-build-plan/pending-implementation-tasks.md`. El board (§4)
refleja la tarea **en curso** y sus handoffs. El hook `SessionStart` (`scripts/session-context.sh`)
carga al iniciar sesión: rama, últimos commits, próxima tarea, board abierto y P0 pendientes.
