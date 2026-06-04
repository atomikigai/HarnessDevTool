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

## 1. Roster y roles

| Rol (runtime) | Quién | Cómo se invoca | Scope de escritura |
|---|---|---|---|
| **Planner / Orquestador** (`planner`) | Claude Opus (loop principal) | nativo | ❌ no edita código; orquesta y verifica |
| **Backend Rust** (`generator`) | **Codex CLI** | `codex exec` vía Bash (§3) | `backend/crates/**` |
| **Frontend** (`generator`) | **Claude Sonnet 4.6** (subagente `frontend`) | **Agent tool** nativa (§3) | `frontend/**` |
| **Doc-agent** (`generator`) | **Claude Haiku 4.5** (subagente `doc-agent`) | **Agent tool** nativa (§3) | `docs/**` |
| **Revisor de bugs** (`evaluator`) | Claude Sonnet (subagente) | **Agent tool** nativa | ❌ solo reporta |
| **QA** (`evaluator`) | Claude Sonnet (subagente) | **Agent tool** nativa | ❌ solo reporta |

Reglas de rol:

- El **Planner no edita código**: descompone, redacta el brief, delega, y verifica que se cumplió el
  objetivo. Es el hub de comunicación.
- **Backend Rust (Codex)** cubre todo el workspace `backend/crates/**`. Aquí el backend es **Rust de
  sistemas** (PTY, MCP, scheduler, policy), no CRUD. No se parte en "logic/UI".
- **Crates de alto riesgo** — `harness-session` (PTY), `harness-policy`, `harness-mcp-server`: el
  Planner los revisa de cerca o los hace con par-revisión de Sonnet. **No** se delegan a Codex a
  ciegas. `cargo check` verde ≠ correcto en código de sistemas.
- **Frontend (Claude Sonnet 4.6)** cubre `frontend/**` (SvelteKit/Tailwind/shadcn). Subagente nativo
  `frontend` (`.claude/agents/frontend.md`) que **sí edita** su dominio. Un solo rol frontend.
- **Frontend, Doc-agent, Revisor y QA son subagentes Claude nativos** (Agent tool). Frontend edita
  `frontend/**` y Doc-agent edita `docs/**`; Revisor y QA solo reportan. Todos devuelven su resultado
  como tool result al Planner.
- **Codex (backend) es el único CLI externo**: el Planner lo lanza por `Bash`. La Agent tool nativa
  spawnea Claude (frontend/doc-agent/revisor/qa), no Codex. No confundir los dos mecanismos.
- Cualquier ejecutor con una duda la escribe en el board (§4) y espera al Planner; no asume.

---

## 2. Ciclo de vida de una tarea

```
PLAN (Planner)
  └─ brief + contrato en docs/teamwork/BOARD.md
        └─ EXECUTE paralelo (Backend Rust / Frontend según alcance, write scopes separados)
        └─ REVIEW de bugs (Sonnet)  →  QA (Sonnet)  →  VERIFY objetivo (Planner)
              └─ si falla cualquiera: vuelve a EXECUTE con notas. Si pasa: cerrar en el board.
```

- **PLAN**: el Planner escribe en el board objetivo, alcance, archivos probables, criterio de
  aceptación, responsables y **contrato** front/back si aplica.
- **CONTRATO**: antes de ejecutar en paralelo, Backend publica rutas, métodos, payloads, respuestas,
  errores **y los tipos `ts-rs` afectados**. Frontend implementa contra ese contrato. Si cambia,
  Backend actualiza el board.
- **EXECUTE paralelo**: Backend y Frontend trabajan a la vez con write scopes separados. Si necesitan
  el mismo archivo, el Planner serializa esa parte.
- **HANDOFF**: cada ejecutor anota en el board "listo para consumo" con endpoints, tipos, archivos
  tocados y comandos corridos.
- **REVIEW**: Sonnet busca bugs (correctitud, regresiones, contrato API/tipos, append-only,
  permisos).
- **QA**: Sonnet valida contra el criterio de aceptación, corriendo `just test` y/o el endpoint.
- **VERIFY**: el Planner confirma objetivo cumplido y la puerta de calidad de §5 verde.

---

## 3. Comandos de invocación (headless)

Correr siempre desde la **raíz del repo**.

### Backend Rust (Codex)
```bash
codex exec -s workspace-write "BRIEF Backend Rust: <objetivo>. Sigue AGENTS.md y docs/. \
Alcance: backend/crates/<crate>. Criterio: <aceptación>. Si tocas un tipo #[derive(TS)], corre \
just gen-types. No toques frontend/."
```

### Frontend (Claude Sonnet 4.6 — subagente nativo)
Se spawnea con la **Agent tool** nativa (`subagent_type: frontend`, def. en `.claude/agents/frontend.md`,
modelo Sonnet 4.6), **no** por CLI. El brief va en el prompt; el contrato vive en el board. El subagente
edita `frontend/**`, corre `pnpm check`, y devuelve su handoff como tool result (no escribe el board;
el Planner registra el handoff y dispara Revisor/QA).

### Doc-agent (Claude Haiku 4.5 — subagente nativo, rápido)
Se spawnea con la **Agent tool** nativa (`subagent_type: doc-agent`, def. en `.claude/agents/doc-agent.md`,
modelo Haiku 4.5 por velocidad/costo). Edita **`docs/**`** (y, si se le pide, `README`/comentarios de
doc): actualizar docs, changelogs, notas de tareas, sincronizar el estado del backlog. No toca código
de `backend/**` ni `frontend/**`. Devuelve handoff como tool result.

### Revisor de bugs y QA (Claude Sonnet)
Se spawnean con la **Agent tool** nativa (subagentes `reviewer` y `qa` en `.claude/agents/`), no por
CLI. El resultado del subagente vuelve como tool result, estructurado — no se pisa el board.

Notas: Codex usa sandbox `workspace-write` (escribe solo en el repo). El Frontend ya **no** usa Cursor:
es un subagente Claude nativo (Sonnet 4.6) vía Agent tool. **Codex (backend) es el único CLI externo**
del equipo; Frontend, Doc-agent, Revisor y QA son subagentes Claude nativos.

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
