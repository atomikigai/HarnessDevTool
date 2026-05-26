---
id: build-plan/phase-5-skills
title: F5 — Auto-mejora (Skills + Learner + Curator)
shard: 12-build-plan
tags: [phase, f5, skills, learner, curator, self-improvement]
summary: Skills MD+YAML, learner en modo proposed/, curator determinístico, FTS5 memoria.
related: [build-plan/phase-3-team, foundations/lessons-learned, harness-core/mcp-integration]
sources: [foundations/lessons-learned]
---

# F5 — Auto-mejora

## Meta
Empezar el closed-loop learning: el harness observa lo que los agentes hacen bien (y mal), propone skills nuevas en `proposed/` para revisión humana, el Curator mantiene el corpus. Sin LLM review costoso ni GEPA aún — eso es F6.

## Entregables

### Backend — formato Skill
- [ ] Schema `skill.v1.json` (frontmatter YAML) + parser/validator en `harness-skills`.
- [ ] Storage:
  ```
  ~/.harness/profiles/<p>/skills/
  ├── index.db                    # SQLite + FTS5 sobre triggers/body
  ├── .usage.json                 # mirror ligero de telemetría
  ├── .skill_backups/             # snapshots tar.zst
  ├── .archive/
  ├── agent_created/
  ├── proposed/                   # ← donde aterrizan las sugerencias del learner
  ├── bundled/                    # read-only, viene con el harness
  └── hub/                        # opcional, instaladas del hub público
  ```
- [ ] Operaciones:
  - [ ] `skill.create/edit/patch/archive/restore/pin/unpin`.
  - [ ] `skill.search { query, top_k }` con FTS5.
  - [ ] `skill.view { id }`.
  - [ ] `skill.history { id }` → diff entries.

### Backend — Learner (modo `proposed/` por default)
- [ ] Policy en `harness-skills::learner`:
  - [ ] Hook `on_turn_completed(turn)`:
    - [ ] Si `tool_calls.len() >= 5` y `outcome == success` → propose extract.
    - [ ] Si hubo `dead_end + recovery` → propose extract con foco en recovery.
    - [ ] Si el evaluator devolvió `verify-fail` y luego `verify-ok` → propose patch o skill nueva.
  - [ ] Propose extract = generar archivo `skills/proposed/<slug>.md` con borrador inferido del trace.
- [ ] Endpoint `GET /api/skills/proposed`, `POST /api/skills/proposed/:id/promote`, `DELETE /api/skills/proposed/:id`.

### Backend — Curator (fase determinística solamente)
- [ ] Crate `harness-skills::curator`:
  - [ ] Loop background: tick diario (configurable).
  - [ ] Trigger: `≥ interval_hours (7d)` desde último run y `min_idle_hours (2h)` agente idle.
  - [ ] Phase 1 determinístico:
    - [ ] Skills sin uso `≥ stale_after_days (30)` → `state=stale`.
    - [ ] Skills sin uso `≥ archive_after_days (90)` → mover a `.archive/`.
  - [ ] Snapshot tar.zst en `.skill_backups/<ts>.tar.zst` antes de cada pasada.
  - [ ] Report a `~/.harness/logs/curator/<ts>/REPORT.md` + `run.json`.
- [ ] Endpoint `GET /api/curator/status`, `POST /api/curator/run` (force).
- [ ] **NO** ejecuta LLM review en F5 (eso es F6).

### Backend — MCP tools nuevas para skills
- [ ] `skill_manage { action: create|patch|edit|delete|archive, id, body?, patch?, reason }` (los agentes pueden auto-mejorar).
- [ ] `skills.search { query, top_k }` (ya stub en F2, ahora funcional con FTS5).
- [ ] **Política por default**: `skill_manage` requiere approval excepto `patch` con `patch_count < 3` (heurística para auto-mejoras pequeñas).

### Backend — FTS5 memoria
- [ ] Crate `harness-skills::memory`:
  - [ ] Indexar `events.jsonl` en SQLite FTS5 al cierre de cada turn.
  - [ ] Tools MCP:
    - [ ] `memory.search { query, top_k, scope: "thread"|"profile" }`.
    - [ ] `memory.get { item_id }`.
- [ ] Storage `~/.harness/profiles/<p>/memory.db`.

### Backend — Trajectories
- [ ] Endpoint `POST /api/export/trajectories { since, redact: bool }` → tarball ShareGPT-style.

### Backend — Skills bajo git (opt-in, default ON)
- [ ] Al inicializar el perfil: `git init` en `~/.harness/profiles/<p>/skills/`.
- [ ] Cada mutación de skill: `git add . && git commit -m "<reason>"`.
- [ ] `harness skills log` → wrapper sobre `git log`.

### Frontend
- [ ] Ruta `/skills/+page.svelte`:
  - [ ] Tabs: `Active`, `Proposed`, `Stale`, `Archived`, `Bundled`.
  - [ ] Lista virtualizada por tab; click → drawer con render del MD + frontmatter.
  - [ ] En `Proposed`: botones "Promote" / "Reject" / "Edit & Promote".
  - [ ] En `Active`: "Pin / Unpin", "Archive", "View history" (git log).
- [ ] Ruta `/skills/[id]/+page.svelte`: editor MD (CodeMirror) con preview + valibot validation del frontmatter.
- [ ] Sidebar muestra contador "X proposed" si hay pendientes.

## Test de aceptación
1. Correr el "TODO app" challenge (mismo de F3).
2. Tras un turn con ≥5 tool calls → aparece `skills/proposed/refactor-svelte-stores.md`.
3. La UI muestra "1 proposed" en sidebar.
4. Click "Promote" → archivo se mueve a `agent_created/`, git commit creado.
5. En el siguiente thread, el planner llama `skills.search "refactor svelte"` y recibe la skill creada.
6. Forzar `curator run` con timestamps falsificados → skills "viejas" pasan a `stale`/`archived`, snapshot creado.
7. `harness skills log <id>` muestra el historial de patches.

## Lo que NO está en F5
- LLM review del Curator (F6).
- GEPA (F6).
- `USER.md` con sub-agente psicólogo (F6).
- Importar/instalar skills de un hub público.

## Riesgos
- **Drafts de learner mediocres**: el primer draft generado puede ser ruido. Importante: van a `proposed/`, no aplicados. Iterar la heurística de extracción.
- **FTS5 + JSONL**: indexar eficientemente requiere parsing. Throughput bajo no es crítico (no se busca en hot path).
- **Skills inyectadas como prompt injection**: una skill maliciosa promovida sin review puede sesgar al equipo. Mitigación: `proposed/` por default + sandbox aplica igual.
- **`skill_manage` auto-aplicado**: el agente puede crear basura. Heurística pequeña (patch_count<3) ayuda; loggear todo a `skill.history.jsonl`.

## Decisiones a confirmar
- ¿`stale_after_days` y `archive_after_days` defaults? **30 / 90** (Hermes).
- ¿`min_idle_hours` para Curator trigger? **2** (Hermes).
- ¿Learner aplica `auto` en algún caso, o siempre `proposed`? Recomiendo **siempre proposed en F5**; auto solo tras evaluación humana de calidad.
