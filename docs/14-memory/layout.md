---
id: memory/layout
title: Memoria — layout en disco
shard: 14-memory
tags: [memory, layout, files, paths]
summary: Estructura de directorios con profiles, shared y memoria estructurada.
related: [memory/overview, memory/git, cross-cutting/profiles]
sources: []
---

# Layout en disco

```
~/.harness/
├── config.toml                          # config global (default profile, telemetry, etc.)
├── USER.md                              # capa 5a — global, quién eres como persona
├── credentials/                         # refs cifradas (no en git)
│
├── shared/                              # cross-profile, opt-in
│   ├── skills/
│   │   ├── agent_created/               # promoted desde algún profile
│   │   └── bundled/                     # viene con el harness, read-only
│   ├── .archive/
│   └── .git/                            # remote = repo "public-safe" tuyo
│
├── profiles/
│   ├── personal/
│   │   ├── config.toml                  # config del profile (modelo, MCPs, sandbox)
│   │   ├── PROFILE.md                   # contexto laboral: rol, equipo, estilo, no-goals
│   │   ├── USER.md                      # opcional; override del global si presente
│   │   │
│   │   ├── memory/                      # capa 6
│   │   │   ├── README.md                # índice humano-legible
│   │   │   ├── INDEX.toml               # índice machine-readable
│   │   │   ├── CONTINUITY.md            # auto-regenerado; qué hay en marcha
│   │   │   ├── decisions/               # decisions firmes
│   │   │   │   ├── 2026-05-26-tauri-out.md
│   │   │   │   └── ...
│   │   │   ├── pending/                 # cosas postergadas
│   │   │   │   └── 2026-05-26-windows-support.md
│   │   │   ├── in-flight/               # temas en discusión actual
│   │   │   │   └── 2026-05-26-memory-design.md
│   │   │   ├── facts/                   # patrones aprendidos del proyecto/contexto
│   │   │   │   ├── uses-pnpm.md
│   │   │   │   └── prefers-toml.md
│   │   │   └── snapshots/               # auto cada N horas
│   │   │       └── 2026-05-26T19-00.md
│   │   │
│   │   ├── skills/                      # capa 4 — privadas del profile
│   │   │   ├── agent_created/
│   │   │   ├── proposed/                # learner deja aquí
│   │   │   ├── .archive/
│   │   │   └── .skill_backups/
│   │   │
│   │   ├── threads/
│   │   │   ├── index.db                 # SQLite global de threads del profile
│   │   │   └── <thread-uuid>/
│   │   │       ├── meta.json            # working_dir, modelo, sandbox, AGENTS.md snapshot
│   │   │       ├── spec.md
│   │   │       ├── events.jsonl
│   │   │       ├── tasks/*.toml
│   │   │       ├── artifacts/
│   │   │       ├── budget.toml
│   │   │       └── spawns/
│   │   │           └── <spawn-uuid>/meta.toml + output.log
│   │   │
│   │   ├── cli-state/                   # auth aislada de claude/codex
│   │   │   ├── .claude/                 # bind-mount al container cuando este profile activo
│   │   │   └── .codex/
│   │   │
│   │   ├── search.db                    # SQLite FTS5: skills + memory + events indexados
│   │   │
│   │   └── .git/                        # repo git del profile
│   │
│   └── work-acme/                       # otro profile, misma estructura, .git separado
│       └── ...
│
├── active_profile -> profiles/personal  # symlink; cambiable via `harness profile use`
├── logs/                                # tracing (no en git)
└── .runtime/                            # PIDs, locks (no en git)
```

## Reglas de paths

- **`~/.harness/USER.md`** se carga **siempre** al inicio de cualquier spawn (capa 5a global).
- **`profiles/<active>/PROFILE.md`** se carga después (overlay laboral).
- Si **`profiles/<active>/USER.md`** existe → sustituye al global (override total).
- **`profiles/<active>/memory/`** se carga selectivamente vía `memory.search`.
- **`profiles/<active>/skills/` + `shared/skills/`** se combinan en el corpus de búsqueda.

## Qué entra en git, qué no

Bajo `.git/` de cada profile (`profiles/<p>/.git/`):
- ✅ `memory/` completo (excepto `INDEX.toml` regenerable)
- ✅ `skills/agent_created/`, `skills/proposed/`
- ✅ `PROFILE.md`, `USER.md` (si override)
- ✅ `config.toml` del profile
- ❌ `threads/` (gitignored por default; opt-in `harness profile track-thread <id>`)
- ❌ `cli-state/` (auth, secrets)
- ❌ `search.db` (regenerable)
- ❌ `skills/.skill_backups/`

Bajo `~/.harness/shared/.git/`:
- ✅ `skills/agent_created/` (promoted)
- ❌ `skills/bundled/` (viene con el binario)

Bajo `~/.harness/.git/` (opcional, raíz):
- ✅ `USER.md` global
- ✅ `config.toml` global
- Si no quieres este git → `USER.md` puede vivir sin versionar; el global no es crítico para auditoría.

## Volumen mountable en Docker

`docker-compose.yml`:
```yaml
volumes:
  - ./.harness-data:/data            # mapea a /data en el container
  # Bind-mounts dinámicos del cli-state se hacen al cambiar profile:
  - ./.harness-data/profiles/personal/cli-state/.claude:/root/.claude
  - ./.harness-data/profiles/personal/cli-state/.codex:/root/.codex
```

Cuando `harness profile use work-acme` se ejecuta:
1. Backend para gracefully.
2. docker-compose regenera el bind-mount con la ruta del nuevo profile.
3. Backend re-arranca.
4. El `claude` hijo verá las creds del nuevo trabajo.

Detalle de implementación: alternativa más simple es **symlink** dentro del container.
`/root/.claude` es un symlink a `/data/profiles/<active>/cli-state/.claude/`. Al cambiar profile, solo se actualiza el symlink. Cero docker-compose restart.

## Tamaños típicos

| Archivo | Tamaño objetivo |
|---|---|
| `USER.md` (global) | ≤ 4 KB |
| `PROFILE.md` | ≤ 4 KB |
| Entrada de `memory/decisions/*.md` | ≤ 1.5 KB |
| `CONTINUITY.md` | ≤ 8 KB |
| Skill MD | ≤ 3 KB |
| `events.jsonl` por thread | crece; rotación a 50 MiB → `.jsonl.zst` |
| `search.db` | ~10 MiB por cada 10K items |
