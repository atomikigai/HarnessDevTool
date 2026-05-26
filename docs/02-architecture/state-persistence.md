---
id: architecture/state-persistence
title: Persistencia de estado
shard: 02-architecture
tags: [persistence, storage, threads, event-log]
summary: Event log append-only por thread + índice SQLite; resume y fork operan sobre eventos.
related: [harness-core/thread-lifecycle, harness-core/turn-and-item-primitives]
sources: []
---

# Persistencia

## Layout en disco
```
~/.harness/
├── config.toml
├── credentials/             # cifrado (keyring del SO o age)
├── threads/
│   ├── index.db             # SQLite: id, title, created_at, model, archived
│   └── <thread-uuid>/
│       ├── events.jsonl     # append-only event log (UNA línea = un item event)
│       ├── files/           # adjuntos / sandbox writes ref'd por items
│       └── meta.json        # spec, AGENTS.md snapshot, sandbox config
├── modules/
│   ├── db/connections.db    # SQLite: conexiones guardadas (cifradas)
│   └── ssh/identities.db    # SQLite: hosts, claves, known_hosts
└── cache/
    └── prompt-prefix/       # hashes para diagnóstico de cache misses
```

## Event log
- **Append-only**, JSONL, una línea por evento `item/*` o `turn/*`.
- Lectura secuencial reconstruye estado exacto de UI y prompt.
- Rotación: cuando supera 256 MiB se snapshotea a `events-<ts>.jsonl.zst` y se reinicia.

## Resume
1. Cliente pide `thread.resume { id }`.
2. Core abre `events.jsonl`, replay → UI recibe los items relevantes desde un cursor.
3. Prompt se re-construye **en el mismo orden** que en la sesión original → cache hit.

## Fork
1. `thread.fork { from: id, at_turn: N }`.
2. Copia events hasta el item `turn/completed` N en un nuevo thread uuid.
3. Diverge desde ahí. Útil para A/B de prompts.

## Archive
- Mueve la entrada en `index.db` con `archived = 1`.
- No borra; lista filtrada por defecto.

## Concurrencia
- Un único writer por `events.jsonl` (el thread task). Lock por advisory file lock.
- Readers (resume, exports) abren read-only.

## Backups
- Comando `harness export --thread <id>` empaqueta a un tarball: events + files + meta + spec.
- Re-importable en otra instalación.

## Privacidad
- Credenciales nunca en eventos. El core sustituye por placeholders `{{secret:<ref>}}` al persistir.
- Cifrado opcional del directorio entero vía LUKS / FileVault — fuera del scope del harness.
