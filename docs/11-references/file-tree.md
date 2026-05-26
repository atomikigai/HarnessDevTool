---
id: references/file-tree
title: Layout sugerido del repo
shard: 11-references
tags: [references, layout, repo]
summary: Mono-repo backend Rust + frontend SvelteKit + Docker.
related: [build-plan/repo-layout, harness-core/rust-crate-layout]
sources: []
---

# Layout del repo

> Fuente canónica: [[build-plan/repo-layout]]. Este shard es un resumen rápido.

```
HarnessDevTool/
├── README.md
├── AGENTS.md
├── Justfile                           # orquesta dev/build/test/docker
├── docker-compose.yml
├── docker-compose.dev.yml
├── .env.example
├── docs/                              # 14+ secciones de shards
├── backend/                           # Rust workspace (Axum)
│   ├── Dockerfile                     # rust:1-alpine → distroless
│   ├── Cargo.toml
│   ├── bindings/                      # output ts-rs (gitignored)
│   └── crates/
│       ├── harness-server/            # único binario
│       ├── harness-core/              # threads, tasks, scheduler
│       ├── harness-session/           # PTY
│       ├── harness-mcp-server/        # MCP stdio para CLI hijo
│       ├── harness-sandbox/
│       ├── harness-skills/            # F5+
│       ├── module-db/                 # F4
│       └── module-ssh/                # F4
├── frontend/                          # SvelteKit + adapter-node
│   ├── Dockerfile                     # node:alpine + pnpm
│   ├── package.json
│   ├── svelte.config.js
│   ├── vite.config.ts
│   ├── tailwind.config.ts
│   ├── components.json                # shadcn-svelte
│   └── src/
│       ├── lib/
│       │   ├── api/                   # client + sse + types/ (ts-rs)
│       │   ├── components/{ui,app}/
│       │   ├── stores/
│       │   ├── validators/            # valibot
│       │   └── icons.ts
│       └── routes/
└── .github/
    └── workflows/
```

## Estado del usuario (runtime, no en el repo del código)

```
~/.harness/                            # ← mapeado al container como /data
├── config.toml                        # global
├── USER.md                            # global (capa 5a)
├── shared/                            # cross-profile skills
│   └── skills/
└── profiles/
    └── <active>/
        ├── config.toml
        ├── PROFILE.md
        ├── memory/
        ├── skills/
        ├── threads/
        ├── cli-state/                 # auth claude/codex
        ├── search.db                  # FTS5
        └── .git/                      # versionado (opt-in remote)
```

Ver [[build-plan/repo-layout]] y [[memory/layout]] para detalle completo.

## Lo que NO existe (descartado/pospuesto)

- ❌ `apps/desktop/` (Tauri) — descartado.
- ❌ `apps/cli/` — pospuesto post-F6.
- ❌ `crates/harness-llm/` — el CLI hijo habla con el provider; nosotros no.
- ❌ `shared/` (en repo) — `ts-rs` cubre el contrato; no hace falta dir compartida en el repo del código.

## Notas

- Workspace Cargo cubre solo `backend/crates/*`. El frontend es proyecto independiente.
- `tests/eval/` (F2+) corre el harness contra un set de tasks-target en CI.
- `just gen-types` regenera tipos TS desde Rust (`ts-rs`).
