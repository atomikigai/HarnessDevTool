---
id: build-plan/phase-6-polish
title: F6 — Polish (Curator LLM + GEPA + USER.md + packaging)
shard: 12-build-plan
tags: [phase, f6, polish, gepa, curator-llm, user-model, packaging]
summary: Auto-mejora avanzada con LLM review + GEPA + modelo de usuario + binarios distribuibles.
related: [build-plan/phase-5-skills, foundations/lessons-learned]
sources: [foundations/lessons-learned]
---

# F6 — Polish

## Meta
Cerrar el ciclo de auto-mejora con sus partes "caras" (LLM review del Curator + GEPA offline) y dejar el producto **packageable**: instalador, binarios firmados, docs de usuario, Windows como target soportado.

## Entregables

### Curator — fase LLM review
- [ ] Sobre la fase determinística de F5, añadir Phase 2:
  - [ ] Cuando el Curator dispara, lanza un **fork agent** (puede ser un `claude` propio con un prompt-template `curator`).
  - [ ] Itera hasta 8 rondas. Por cada skill:
    - [ ] `skill_view` → lectura.
    - [ ] Decide: `keep | patch | consolidate | archive`.
    - [ ] `patch` aplica delta (registra en `.history.jsonl`).
    - [ ] `consolidate` merge con otra skill (referencia explícita).
    - [ ] `archive` mueve a `.archive/`.
  - [ ] **Nunca borra**.
  - [ ] **No toca** `bundled/` ni `hub/` ni `pinned=true`.
- [ ] Report markdown con before/after por skill tocada.

### GEPA offline
- [ ] Comando `harness gepa --since 1w --target <profile.role>`:
  - [ ] Recoge traces del role objetivo desde `events.jsonl`.
  - [ ] Identifica turns con score bajo (verify-fail, retries>1, costo anómalo).
  - [ ] Llama a un modelo "judge" separado (configurable) que propone N variantes del prompt-template + N variantes de skills más involucradas.
  - [ ] Evalúa variantes contra un set de **tasks-target reproducibles** (preparado en F3, ver `tests/eval/`).
  - [ ] Emite un **Pull Request** al repo de configs (`profiles/`, `skills/agent_created/`) con métricas comparativas.
- [ ] PR template incluye: skills modificadas, prompt diff, métricas baseline vs candidate, cost de la corrida GEPA.
- [ ] Cero auto-merge: humano (o `evaluator` en modo solo) revisa.

### Modelo de usuario (`USER.md`)
- [ ] Sub-agente "psicólogo" (rol nuevo opcional) corre periódicamente:
  - [ ] Lee últimos N threads del perfil.
  - [ ] Extrae preferencias persistentes (no facts efímeros).
  - [ ] Actualiza `~/.harness/profiles/<p>/memory/USER.md` con frontmatter `last_updated`, `confidence`.
- [ ] Cargado por `prompt_builder` siempre al iniciar cualquier thread.
- [ ] Endpoint para que el usuario vea/edite/elimine entradas.

### Windows como target soportado
- [ ] PTY: validar `portable-pty` con ConPTY + `claude`/`codex` Windows.
- [ ] Sandbox: AppContainer + Job Object (puede ser "best-effort" con warnings, no bloqueante).
- [ ] Bind-mounts en docker-desktop Windows: documentar paths Windows-style.
- [ ] CI matrix incluye windows-latest.

### Packaging
- [ ] Release pipeline (`.github/workflows/release.yml`):
  - [ ] Cross-compile backend para `linux-x86_64-musl`, `darwin-aarch64`, `darwin-x86_64`, `windows-x86_64`.
  - [ ] Build frontend bundle.
  - [ ] Imágenes Docker `ghcr.io/<user>/harness-backend:vX.Y.Z` y `harness-frontend:vX.Y.Z`.
  - [ ] Tarball con ambos binarios + `docker-compose.yml` para self-host sin Docker registry.
  - [ ] Firma:
    - macOS: notarización Developer ID (si hay budget) o instrucciones de Gatekeeper bypass.
    - Windows: signtool con cert.
- [ ] Instalador opcional: `homebrew tap`, `aur` (Arch), `winget`.

### Telemetría (opt-in, off por defecto)
- [ ] Implementar lo descrito en [[cross-cutting/telemetry]].
- [ ] `harness telemetry enable/disable/show --pending`.
- [ ] Endpoint configurable para auto-host.

### Docs de usuario
- [ ] `docs/user/` con guías para usuario final (separadas de los shards de arquitectura):
  - [ ] Quickstart
  - [ ] Cómo configurar `claude`/`codex`.
  - [ ] Concepts: threads, tasks, skills, agents.
  - [ ] Troubleshooting.
  - [ ] FAQ.

### Limpieza
- [ ] Auditar shards de docs y consolidar lo que el plan reveló obsoleto.
- [ ] `cargo audit` + `pnpm audit` limpios.
- [ ] Cobertura objetivo: `harness-core` ≥ 70%, `module-*` ≥ 50%.

## Test de aceptación
1. Curator LLM review corre con set de skills mock; produce reporte sensato (verificable manualmente).
2. GEPA run con un set de 5 tasks-target reproduce métricas baseline; emite PR con candidate; métricas del candidate son ≥ baseline.
3. Build de release: cross-compile OK en CI; instalador macOS abre sin Gatekeeper bypass.
4. `harness` funciona en Windows desktop con docker-desktop.
5. `USER.md` se autopopula tras 3 threads con preferencias capturadas (verificable manualmente).
6. `cargo audit` y `pnpm audit` sin vulnerabilidades críticas.

## Lo que NO está en F6
- Multi-tenant hosting.
- API directa al modelo (sin `claude`/`codex`).
- IDE adapter (ACP-style) para VSCode/Zed.
- Telegram/Discord adapters.

## Riesgos
- **GEPA caro y lento**: $2–10 por corrida + ~30min wallclock. Necesita set reproducible robusto. Si las tasks-target son frágiles, GEPA emite PRs malos.
- **Notarización macOS**: requiere Apple Developer ID ($99/año). Si no se invierte, el usuario debe bypass Gatekeeper.
- **Windows quirks**: ConPTY tiene bugs sutiles; documentar workarounds.
- **Drift entre LLM review del Curator y la base**: si el LLM patchea agresivamente, el corpus drift-ea. Mitigación: snapshots tar.zst diarios + `harness curator rollback`.

## Decisiones a confirmar (cerca de F6)
- ¿`USER.md` siempre cargado o solo opt-in? Recomiendo opt-in con default-on en perfil "personal".
- ¿GEPA usa el mismo modelo del usuario o un "judge" separado configurado? **Judge separado** para evitar sesgo de auto-eval.
- ¿Distribución vía Docker Hub público, ghcr.io, o self-host instructions? Probablemente **ghcr** + tarball self-host.
