---
id: references/file-tree
title: Layout sugerido del repo
shard: 11-references
tags: [references, layout, repo]
summary: Estructura de directorios del workspace y aplicaciones.
related: [harness-core/rust-crate-layout, architecture/state-persistence]
sources: []
---

# Layout del repo

```
HarnessDevTool/
├── Cargo.toml                          # workspace root
├── Cargo.lock
├── rust-toolchain.toml
├── README.md
├── AGENTS.md                           # instrucciones para agentes corriendo en este repo
├── docs/                               # esta documentación shardeada
│   ├── README.md
│   ├── architecture.html
│   ├── 00-meta/
│   ├── 01-foundations/
│   ├── 02-architecture/
│   ├── 03-harness-core/
│   ├── 04-app-server/
│   ├── 05-frontend-shell/
│   ├── 06-module-agents/
│   ├── 07-module-db-manager/
│   ├── 08-module-ssh-manager/
│   ├── 09-cross-cutting/
│   ├── 10-recipes/
│   └── 11-references/
├── crates/
│   ├── harness-core/
│   │   ├── Cargo.toml
│   │   ├── schemas/                    # JSON Schemas versionados
│   │   │   ├── task.v1.json
│   │   │   ├── thread.v1.json
│   │   │   └── budget.v1.json
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── agent_loop.rs
│   │       ├── prompt.rs
│   │       ├── thread.rs
│   │       ├── tasks/
│   │       └── streaming.rs
│   ├── harness-app-server/
│   │   ├── src/main.rs
│   │   ├── src/transport.rs
│   │   ├── src/processor.rs
│   │   └── src/namespaces/
│   ├── harness-sandbox/
│   ├── harness-mcp/
│   ├── harness-llm/
│   ├── module-agents/
│   ├── module-db/
│   └── module-ssh/
├── apps/
│   ├── cli/
│   │   └── src/main.rs
│   └── desktop/                        # Tauri + SvelteKit (proyecto independiente)
│       ├── package.json
│       ├── svelte.config.js
│       ├── vite.config.ts
│       ├── tailwind.config.js
│       ├── tauri.conf.json
│       ├── src-tauri/
│       │   ├── Cargo.toml
│       │   └── src/main.rs
│       └── src/
│           ├── lib/
│           │   ├── rpc/
│           │   ├── stores/
│           │   └── components/
│           ├── routes/
│           │   ├── +layout.svelte
│           │   ├── agents/
│           │   ├── db/
│           │   ├── ssh/
│           │   └── threads/
│           └── app.css
├── tests/
│   ├── integration/
│   ├── golden/
│   ├── fixtures/
│   └── eval/
├── scripts/
│   ├── gen-ts-types.sh
│   └── package-binaries.sh
└── .github/
    └── workflows/
        ├── ci.yml
        └── release.yml
```

## Notas
- Workspace Cargo cubre `crates/*` y `apps/cli`. `apps/desktop` es independiente.
- `apps/desktop/src-tauri/Cargo.toml` enlaza el binario `harness-app-server` como sidecar via `tauri.conf.json`.
- `tests/eval/` corre el harness completo contra un set de tasks-target (CI nocturno).
- `scripts/gen-ts-types.sh` regenera tipos TS desde los JSON Schemas del core.
