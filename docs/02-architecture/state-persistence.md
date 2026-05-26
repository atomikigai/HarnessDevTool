---
id: architecture/state-persistence
title: Persistencia de estado
shard: 02-architecture
tags: [persistence, storage, threads, event-log, profiles]
summary: Event log append-only + SQLite FTS5 + git por profile; todo bajo ~/.harness/.
related: [harness-core/thread-lifecycle, memory/layout, memory/git, cross-cutting/profiles]
sources: []
---

# Persistencia

## Layout en disco

Ver [[memory/layout]] para el árbol completo. Resumen relevante para estado de threads:

```
~/.harness/                                     # ← /data en container
├── USER.md                                     # global capa 5a
├── shared/skills/                              # cross-profile
├── profiles/<active>/
│   ├── config.toml
│   ├── PROFILE.md
│   ├── memory/                                 # decisions/pending/in-flight/facts/snapshots
│   ├── skills/                                 # profile-scoped
│   ├── threads/
│   │   ├── index.db                            # SQLite por profile
│   │   └── <thread-uuid>/
│   │       ├── meta.json                       # snapshot inicial
│   │       ├── spec.md
│   │       ├── events.jsonl                    # append-only
│   │       ├── tasks/*.toml                    # 1 archivo por task
│   │       ├── artifacts/
│   │       ├── budget.toml
│   │       └── spawns/<sid>/
│   │           ├── meta.toml
│   │           └── output.log                  # PTY raw
│   ├── cli-state/.claude/.codex/               # auth aislada
│   ├── search.db                               # FTS5
│   └── .git/                                   # versionado opcional con remote
├── active_profile -> profiles/personal
└── logs/
```

## Event log (`events.jsonl`)

- **Append-only**, JSONL, una línea por item.
- Items detallados en [[harness-core/turn-and-item-primitives]].
- Lectura secuencial reconstruye estado exacto de UI y task state machine.
- Rotación: al pasar 50 MiB → comprime a `events-<ts>.jsonl.zst`.

## Resume

1. Cliente browser pide `POST /api/threads/:id/resume`.
2. Backend lee `events.jsonl` (+ rotated zst files) → replay items.
3. Reconstruye state in-memory (tasks, sessions activas, etc.).
4. Lease-expired tasks pasan a `queued`.
5. SSE emite snapshot del state al cliente.

## Fork

`POST /api/threads/:id/fork { at_task? }`:
- Copia eventos hasta el `task.done` indicado (o último estado).
- Genera nuevo UUID con `parent_id`.
- Diverge desde ahí. Útil para A/B de approaches.

## Archive

- Marca `archived = true` en `index.db`.
- Conserva todo en disco.
- Lista de threads por default filtra archivados.

## Concurrencia

- Un único writer por `events.jsonl` (el thread task del scheduler). Lock por advisory file lock (`fs2` o equivalent).
- Lecturas son seq scan (read-only opens).
- Múltiples spawns escribiendo a sus respectivos `output.log` en paralelo, sin contención.

## Backups

- `harness profile export <name>` empaqueta el profile completo (incluye .git history).
- `harness profile import <tarball>` restora.
- Cross-machine sync vía git remote opcional. Ver [[memory/git]].

## Privacidad

- Credenciales del provider NO viven aquí (en `cli-state/` del CLI; eso es del CLI, no editado por nosotros).
- Si una entrada de memoria menciona algo sensible, el harness sustituye `{{secret:<ref>}}` antes de persistir.
- Cifrado opcional del directorio entero vía LUKS / FileVault (responsabilidad del SO) o `harness profile export --encrypt` con `age`.

## Performance

- Append a `events.jsonl`: fsync periódico (no por línea). Trade-off: en crash perdemos ≤ 1s de items.
- `index.db` con WAL mode → lectura concurrente sin bloqueo.
- `search.db` (FTS5) actualizado incrementalmente al persistir.

## Aislamiento por profile

- Cambiar profile = cambiar el symlink `active_profile`.
- Backend re-resuelve paths al `~/.harness/profiles/<nuevo>/`.
- Threads del profile anterior siguen en disco, no se mezclan.

Ver [[cross-cutting/profiles]] para el detalle.
