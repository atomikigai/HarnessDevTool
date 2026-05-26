---
id: memory/git
title: Memoria — versionado con git
shard: 14-memory
tags: [memory, git, audit, sync]
summary: Cada profile tiene su git; commits automáticos por cada cambio; remotes opcionales para sync.
related: [memory/overview, memory/layout, cross-cutting/profiles]
sources: []
---

# Versionado con git

## Topología

Tres repos git posibles, todos opcionales-pero-recomendados:

```
~/.harness/
├── .git/                       # (opcional) USER.md global, config.toml global
├── shared/.git/                # skills compartidas cross-profile
└── profiles/
    ├── personal/.git/          # repo del profile personal
    └── work-acme/.git/         # repo del profile work-acme (distinto remote)
```

Cada uno con su propio `remote`, permitiendo:
- `profiles/personal/.git/` → push a tu GH personal.
- `profiles/work-acme/.git/` → push a un GitLab/Gitea de la empresa.
- `shared/.git/` → push a tu GH personal (skills "públicas").

## Inicialización

Al primer arranque del harness, o vía CLI:
```bash
harness profile init personal               # crea profile + git
harness profile init work-acme              # crea otro
harness shared init                         # crea shared
```

`harness profile init` ejecuta:
1. `mkdir -p profiles/<name>/{memory/{decisions,pending,in-flight,facts,snapshots},skills/{agent_created,proposed,.archive},threads,cli-state}`.
2. Escribe templates iniciales de `config.toml` y `PROFILE.md`.
3. Genera un `.gitignore` apropiado.
4. `git init` + commit inicial.
5. Registra el profile en `~/.harness/config.toml`.

## .gitignore por profile

```gitignore
# regenerables
search.db
search.db-*
*.lock
.runtime/

# secretos / auth
cli-state/

# event logs (grandes, append-only, no en git)
threads/*/events.jsonl
threads/*/events-*.jsonl.zst
threads/*/spawns/*/output.log

# backups del curator
skills/.skill_backups/

# por default no commit threads completos — opt-in con `harness profile track-thread <id>`
threads/

# logs
logs/
```

Al "track-ear" un thread:
```bash
harness profile track-thread <thread-uuid>
# añade !threads/<uuid>/ al .gitignore (unignored)
# añade !threads/<uuid>/{spec.md,tasks/,artifacts/,meta.json,budget.toml}
# pero excluye events.jsonl y spawns/
```

## Commits automáticos

El harness emite commits automáticos al:
- Crear/editar/transition de una entrada de memoria.
- Crear/promover/archivar una skill.
- Cambio en `PROFILE.md` o `config.toml` del profile.

Convención de mensaje:
```
<área>: <verbo> <objetivo>

[contexto opcional]
```

Ejemplos:
- `memory: create decision 2026-05-26-tauri-out`
- `memory: promote in-flight 2026-05-26-memory-design → decision`
- `skills: patch refactor-svelte-store (patch #3)`
- `skills: archive unused-stripe-integration`
- `config: change default sandbox level workspace → workspace-net`

Author del commit:
- Cuando lo dispara el humano: `Human <user@host>`.
- Cuando lo dispara un agente: `Agent <name> <agent@harness>`.

Esto facilita `git log --author=Agent` para auditar mutaciones automáticas.

## Throttling

Para no inflar el git log con micro-commits durante operación intensa:
- `CONTINUITY.md` refresh **no** dispara commit por defecto (es derivado).
- Cambios consecutivos al mismo archivo en < 30s → `--amend` del commit anterior.
- Override: `harness profile commit-now` fuerza un commit + push si hay cambios pendientes.

## Remotes

Setup manual una vez:
```bash
cd ~/.harness/profiles/personal
git remote add origin git@github.com:tu-usuario/harness-personal-memory.git

cd ~/.harness/profiles/work-acme
git remote add origin git@gitlab.acme.com:tu/harness-acme-memory.git
```

Sync:
```bash
harness profile sync                       # pull + push del profile activo
harness profile sync --all                 # todos los profiles + shared
```

Auto-sync opcional:
```toml
# en profiles/<p>/config.toml
[git]
auto_sync = true
sync_interval_min = 30
```

Conflictos en pull:
- Por default, el harness **se detiene** y muestra el conflicto en UI.
- El humano resuelve via UI (3-way merge para entradas markdown) o CLI (`harness profile resolve`).
- Nunca auto-merge silencioso.

## Operaciones útiles

```bash
harness skills log                         # git log -- skills/
harness skills diff --since 7d             # qué cambió en skills/
harness memory log --kind decision         # git log filtrado a memory/decisions/
harness profile diff personal --since 1m   # qué cambió en 1 mes
harness profile restore <commit> --path memory/decisions/foo.md
```

Comandos son wrappers thin sobre `git`; el usuario avanzado puede entrar a `~/.harness/profiles/personal/` y usar git directo.

## Backup vs sync

- **Sync** (push/pull): replicación entre máquinas tuyas.
- **Backup** (tarball): para archivo a largo plazo o transferir el profile.

```bash
harness profile export personal > personal-2026-05-26.tar.zst
harness profile import personal-2026-05-26.tar.zst --as personal-restored
```

El tarball incluye el `.git/` completo: la historia se preserva al importar.

## Privacidad

- Los repos git pueden ser privados. El harness no asume otra cosa.
- `shared/` es el único pensado como "puede ser público". Aún así: solo si el usuario lo decide.
- Secretos nunca en commits: el harness sustituye `{{secret:<ref>}}` antes de persistir cualquier cosa que vaya a memoria.

## CI opt-in sobre el repo de memoria

Si el usuario hace push a un repo:
- Puede correr `harness profile validate` en CI para validar schemas.
- Útil para detectar entradas mal formadas tras edits manuales.

## Anti-patrones

| Mal | Bien |
|---|---|
| Un solo git para todos los profiles | Uno por profile + shared aparte |
| Commits automáticos sin convención | Convención `<área>: <verbo> <obj>` |
| Auto-merge silencioso de conflictos | Detener + UI 3-way |
| Push automático a remote sin opt-in | `auto_sync` explícito |
| Commit-ear secretos por accidente | `{{secret:*}}` placeholders obligatorios |
| Track-ear todos los threads (inflación) | Opt-in por thread importante |
