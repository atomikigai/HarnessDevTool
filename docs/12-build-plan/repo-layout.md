---
id: build-plan/repo-layout
title: Layout final del repo
shard: 12-build-plan
tags: [layout, repo, monorepo]
summary: Estructura de carpetas final tras las decisiones de planning.
related: [build-plan/tech-stack-locked, references/file-tree]
sources: []
---

# Layout del repo

```
HarnessDevTool/
├── README.md
├── AGENTS.md                          # instrucciones para agentes en este repo
├── Justfile                           # orquestación dev/build/test/docker
├── docker-compose.yml                 # prod (build images)
├── docker-compose.dev.yml             # dev (volúmenes mount, hot reload)
├── .env.example
├── .gitignore
├── .editorconfig
├── docs/                              # shards de documentación
│   ├── README.md
│   ├── architecture.html
│   └── ...
│
├── backend/
│   ├── Dockerfile                     # multi-stage: rust:alpine builder → distroless
│   ├── .dockerignore
│   ├── Cargo.toml                     # [workspace]
│   ├── Cargo.lock
│   ├── rust-toolchain.toml
│   ├── clippy.toml
│   ├── rustfmt.toml
│   ├── bindings/                      # output de ts-rs (gitignored)
│   └── crates/
│       ├── harness-server/            # bin: Axum, routes, SSE, CORS
│       │   ├── Cargo.toml
│       │   └── src/
│       │       ├── main.rs
│       │       ├── app.rs
│       │       ├── state.rs
│       │       ├── config.rs
│       │       ├── error.rs
│       │       ├── extractors.rs
│       │       ├── routes/
│       │       │   ├── mod.rs
│       │       │   ├── health.rs
│       │       │   ├── threads.rs
│       │       │   ├── tasks.rs
│       │       │   ├── sessions.rs
│       │       │   ├── events.rs      # SSE
│       │       │   ├── skills.rs      # F5
│       │       │   └── modules/
│       │       │       ├── db.rs      # F4
│       │       │       └── ssh.rs     # F4
│       │       └── sse/
│       │           ├── hub.rs
│       │           └── encoding.rs
│       │
│       ├── harness-core/              # lógica pura (no Axum)
│       │   ├── Cargo.toml
│       │   ├── schemas/               # JSON Schemas versionados
│       │   │   ├── thread.v1.json
│       │   │   ├── task.v1.json
│       │   │   ├── skill.v1.json
│       │   │   └── budget.v1.json
│       │   └── src/
│       │       ├── lib.rs
│       │       ├── threads/
│       │       ├── tasks/
│       │       ├── events/
│       │       ├── store/
│       │       └── scheduler/         # F3
│       │
│       ├── harness-session/           # PTY manager (F1)
│       ├── harness-mcp-server/        # MCP server expuesto al CLI (F2)
│       ├── harness-sandbox/           # F3
│       ├── harness-skills/            # F5
│       ├── module-db/                 # F4
│       └── module-ssh/                # F4
│
├── frontend/
│   ├── Dockerfile                     # multi-stage: node:alpine → runtime adapter-node
│   ├── .dockerignore
│   ├── package.json
│   ├── pnpm-lock.yaml
│   ├── svelte.config.js               # adapter-node
│   ├── vite.config.ts                 # proxy dev a :7777
│   ├── tailwind.config.ts
│   ├── postcss.config.cjs
│   ├── tsconfig.json
│   ├── components.json                # shadcn-svelte
│   ├── eslint.config.js
│   ├── .prettierrc
│   └── src/
│       ├── app.html
│       ├── app.css
│       ├── app.d.ts
│       ├── lib/
│       │   ├── api/
│       │   │   ├── client.ts          # fetch + SSE wrapper
│       │   │   ├── sse.ts
│       │   │   └── types/             # ← ts-rs output (gitignored)
│       │   ├── components/
│       │   │   ├── ui/                # shadcn-svelte
│       │   │   └── app/
│       │   │       ├── Sidebar.svelte
│       │   │       ├── ThreadList.svelte
│       │   │       ├── TaskCard.svelte
│       │   │       ├── TaskGraph.svelte
│       │   │       └── TerminalView.svelte
│       │   ├── stores/
│       │   │   ├── session.ts
│       │   │   ├── threads.ts
│       │   │   ├── thread.ts
│       │   │   └── tasks.ts
│       │   ├── hooks/
│       │   ├── utils/
│       │   ├── validators/            # valibot schemas (F2+)
│       │   └── icons.ts
│       └── routes/
│           ├── +layout.svelte
│           ├── +layout.ts
│           ├── +page.svelte           # dashboard
│           ├── threads/
│           │   ├── +page.svelte
│           │   └── [id]/
│           │       ├── +layout.svelte
│           │       ├── +page.svelte
│           │       ├── tasks/+page.svelte
│           │       └── sessions/[sid]/+page.svelte
│           ├── agents/+page.svelte
│           ├── skills/+page.svelte    # F5
│           ├── db/+page.svelte        # F4
│           ├── ssh/+page.svelte       # F4
│           └── settings/+page.svelte
│
└── .github/
    └── workflows/
        ├── ci.yml                     # cargo test + pnpm test + lints
        ├── docker.yml                 # build & push images on tag
        └── docs.yml                   # opcional: validar shards
```

## Notas
- `apps/desktop` ya no existe (Tauri descartado).
- `apps/cli` pospuesto post-F6.
- `shared/` no es necesario porque `ts-rs` resuelve el contrato.
- Cada crate sigue el patrón `Cargo.toml + src/lib.rs` (bin solo `harness-server` por ahora; en F6 puede añadirse un `harness-curator-cli` opcional).
- Schemas JSON viven con `harness-core` (fuente de verdad lógica); el frontend los puede consumir si quiere validación runtime.
