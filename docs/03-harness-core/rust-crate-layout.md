---
id: harness-core/rust-crate-layout
title: Layout del workspace Cargo (backend)
shard: 03-harness-core
tags: [rust, cargo, workspace, layout]
summary: Crates internos del workspace backend/ y sus dependencias.
related: [build-plan/repo-layout, architecture/layered-architecture, build-plan/tech-stack-locked]
sources: []
---

# Workspace Cargo (backend)

```toml
# backend/Cargo.toml (root)
[workspace]
resolver = "2"
members = [
  "crates/harness-server",
  "crates/harness-core",
  "crates/harness-session",
  "crates/harness-mcp-server",
  "crates/harness-sandbox",
  "crates/harness-skills",
  "crates/module-db",
  "crates/module-ssh",
]
```

## Crates

| Crate | Bin/Lib | Rol | Depende de |
|---|---|---|---|
| `harness-server` | bin | Axum HTTP+SSE; único binario | `harness-core`, `harness-session`, `harness-mcp-server`, `harness-skills`, `module-*` |
| `harness-core` | lib | Threads, tasks (state machine), scheduler, storage | (crates ext) |
| `harness-session` | lib | PTY manager (portable-pty), detección claude/codex | (crates ext) |
| `harness-mcp-server` | lib + bin opcional | MCP server stdio expuesto al CLI hijo | `harness-core`, `harness-skills` |
| `harness-sandbox` | lib | seccomp / sandbox-exec / AppContainer | (crates ext) |
| `harness-skills` | lib | Skills + Learner + Curator (F5+) | `harness-core` |
| `module-db` | lib | DB lite con `sqlx` | `harness-core` (trait `HarnessTool`) |
| `module-ssh` | lib | SSH/SFTP con `russh` | `harness-core` |

## Features

- `harness-core/git-profile` → enable git automation on profile dirs.
- `module-db/postgres`, `module-db/mysql` → opt-in para reducir tamaño.
- `harness-mcp-server/embedded` → linkear in-process en vez de spawnear como child.
- `harness-skills/curator-llm` → fase 2 del Curator (F6).

## Por qué este layout

- **`harness-server` es el único bin** → un binario que distribuir.
- **`harness-core` no conoce Axum** → testeable sin HTTP.
- **`harness-mcp-server` separable como bin** → si en algún momento queremos correrlo como child process independiente (Codex-style), ya está aislado.
- **`module-*` son features verticales opt-in** → pueden estar deshabilitadas al compilar.

## Versionado

Workspace usa `version.workspace = true`. Bump unificado.

Breaking en el protocolo HTTP+SSE requiere bump del header `X-Protocol-Version`. Ver [[app-server/backward-compat]].

## Lo que NO está

- ❌ `harness-llm` (provider adapters): no llamamos a Anthropic/OpenAI directo; lo hace el CLI hijo.
- ❌ `apps/cli`: pospuesto post-F6.
- ❌ Apps Tauri: descartado.
- ❌ Workspace cubriendo el frontend: `frontend/` es proyecto Node aparte; no comparte `Cargo.toml`.
