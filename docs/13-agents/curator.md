---
id: agents/curator
title: Agent — Curator (skill maintenance, background)
shard: 13-agents
tags: [agent, curator, skills, maintenance, async]
role: curator
domain: none
cli: claude
summary: Background. Mantiene corpus de skills. Default stub F5 (determinístico); LLM review en F6. Nunca borra.
related: [agents/overview, agents/learner, foundations/lessons-learned]
sources: [foundations/lessons-learned]
---

# Agent — Curator

> **Estado**: fase determinística en **F5**; fase LLM review en **F6**. Inspirado directamente en Hermes Agent Curator.

## Cuándo se ejecuta

**Background, no por task**. Trigger automático cuando:
- `≥ interval_hours (default 7d)` desde el último run.
- `≥ min_idle_hours (default 2h)` del backend sin actividad de tasks.

Manual:
- `harness curator run` (force).
- `harness curator run --dry-run` (preview sin tocar disco).

## Filosofía (locks heredados)

| Regla | Significado |
|---|---|
| **Nunca borra** | Solo archiva (`skills/.archive/`). `harness curator restore <id>` recupera. |
| **Snapshot antes de tocar** | Tar.zst en `skills/.skill_backups/<ts>.tar.zst`. Permite `harness curator rollback`. |
| **No toca `bundled/`** | Las skills que vienen con el harness son read-only. |
| **No toca `pinned=true`** | El humano protegió esa skill. |
| **No actúa sobre memoria** | Solo skills. Memoria es del humano y aprovisionada por agentes con approval. |

## Capabilities declaradas

### MCPs disponibles
| MCP | Cuándo cargarlo |
|---|---|
| `harness-bridge` | **siempre** |

### Skill tags
- ninguna (no consume skills para hacer su trabajo)

### Tools permitidas
- `skills.search`, `skills.get`, `skills.list_meta`
- `skill_manage(action="patch")` — F6 LLM review
- `skill_manage(action="archive")` — F6 LLM review; F5 archive es del Rust determinístico
- `skill_manage(action="consolidate")` — F6
- `memory.search` (consultar decisiones previas sobre curación)

**No** tiene `skill_manage(action="delete")`. Aunque la tool exista, la lista de actions del curator está restringida por el harness.

## Fase 1 — Determinística (F5)

Implementada en Rust, **sin LLM**:

```rust
async fn curator_phase1(profile: &Profile) -> CuratorReport {
    let now = profile.runtime.now();
    let mut report = CuratorReport::new();

    for skill in profile.skills.list().agent_created() {
        if skill.pinned { continue; }
        let unused_for = now - skill.last_used_at.unwrap_or(skill.created_at);

        if unused_for >= Duration::days(90) && skill.state != "archived" {
            archive(skill).await?;
            report.archived.push(skill.id);
        } else if unused_for >= Duration::days(30) && skill.state == "active" {
            mark_stale(skill).await?;
            report.staled.push(skill.id);
        }
    }

    report
}
```

Snapshot antes de mutar:
```bash
tar -cf - skills/agent_created/ skills/.archive/ | zstd > .skill_backups/<ts>.tar.zst
```

## Fase 2 — LLM review (F6)

Tras la fase 1, **fork agent**:
- Hasta **8 iteraciones**.
- Por cada skill (filtrada por: agent_created, no pinned, modificada en último mes):
  - `skill_view` la skill.
  - Decide: `keep | patch | consolidate | archive`.
  - Aplica decisión vía `skill_manage`.

Decisiones permitidas:

| Decisión | Acción |
|---|---|
| `keep` | nada |
| `patch` | mejora el cuerpo con cambios incrementales (corrige errores, clarifica) |
| `consolidate` | merge con otra skill similar (referencia explícita; archiva la dominada) |
| `archive` | mueve a `.archive/` con razón |

**Cero borrados**.

## Reports

Tras cada pasada, escribe a `~/.harness/profiles/<p>/logs/curator/<YYYY-MM-DDTHH:MM>/`:

- `run.json` — machine-readable: counts, decisions, timings, cost.
- `REPORT.md` — humano-legible: tabla de decisiones, razonamiento por skill (F6), diffs aplicados.

Ejemplo `REPORT.md` (F6):
```markdown
# Curator run 2026-06-15T03:00:00Z

## Phase 1 — Deterministic
- Staled: 3 skills (last_used > 30d)
- Archived: 1 skill (last_used > 90d)

## Phase 2 — LLM Review
| Skill | Decision | Reasoning |
|---|---|---|
| refactor-svelte-store | patch | Añadido caso edge sobre stores derivados |
| stripe-integration-deprecated | archive | Patrón abandonado, sin uso 3 meses |
| ... |

## Stats
- Duration: 8.2 min
- Cost: $0.41
- Snapshots: .skill_backups/20260615T030000.tar.zst
```

## CLI

```bash
harness curator status                    # último run, counts, pinned, LRU top-5
harness curator run [--background|--dry-run]
harness curator backup                    # snapshot manual
harness curator rollback                  # revertir a snapshot anterior
harness curator restore <skill-id>        # recuperar desde .archive/
harness curator pin <skill-id>            # protección anti-curator
harness curator unpin <skill-id>
harness curator pause | resume            # deshabilita curator
```

## Configuración

`profiles/<p>/config.toml`:
```toml
[curator]
enabled = true
interval_hours = 168              # 7 días
min_idle_hours = 2
stale_after_days = 30
archive_after_days = 90
llm_review_enabled = false        # F5: false; F6: true (opt-in)
llm_review_model = "claude-haiku" # económico
max_iterations = 8
```

## Activación por fase

| Fase | Comportamiento |
|---|---|
| F3, F4 | Disabled |
| **F5** | Phase 1 determinística activa; phase 2 disabled |
| **F6** | Phase 1 + Phase 2 (LLM); activación opt-in por usuario |

## Spawn hint default (F6)
```toml
mcp     = ["harness-bridge"]
skills  = []
tools   = ["skills.*", "skill_manage", "memory.search"]
```

## Anti-patrones

| Mal | Bien |
|---|---|
| Borrar skills viejas | Solo archive; restore disponible |
| Tocar bundled/ | Estrictamente off-limits |
| LLM review patcheando agresivo | Cada patch genera diff visible + rollback fácil |
| Sin snapshot previo | Snapshot tar.zst obligatorio |
| Curator activo durante tasks intensas | Trigger requiere min_idle_hours = 2 |
| Mutar memoria | Curator solo skills; memoria es del humano |
