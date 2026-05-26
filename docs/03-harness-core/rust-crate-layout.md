---
id: harness-core/rust-crate-layout
title: Layout del workspace Cargo
shard: 03-harness-core
tags: [rust, cargo, workspace, layout]
summary: Crates internos del workspace y sus dependencias.
related: [architecture/layered-architecture, references/file-tree]
sources: []
---

# Workspace Cargo

```toml
# Cargo.toml (root)
[workspace]
resolver = "2"
members = [
  "crates/harness-core",
  "crates/harness-app-server",
  "crates/harness-sandbox",
  "crates/harness-mcp",
  "crates/harness-llm",
  "crates/module-agents",
  "crates/module-db",
  "crates/module-ssh",
  "apps/cli",
]
```

## Crates

| Crate | Rol | Depende de |
|---|---|---|
| `harness-core` | Agent loop, threads, prompt | `harness-sandbox`, `harness-mcp`, `harness-llm` |
| `harness-app-server` | JSON-RPC broker | `harness-core` |
| `harness-sandbox` | Aislamiento OS | nada interno |
| `harness-mcp` | Cliente MCP | `harness-sandbox` (opt) |
| `harness-llm` | Adaptadores provider | nada interno |
| `module-agents` | Sesiones Claude CLI | `harness-core` (trait `HarnessTool`) |
| `module-db` | DB lite | `harness-core` |
| `module-ssh` | SSH/SFTP | `harness-core` |
| `apps/cli` | Surface CLI | `harness-app-server` (spawn) |

`apps/desktop` es un proyecto Tauri **separado** que no comparte `Cargo.toml` raíz (evita arrastrar dependencias UI a los crates server). Bundle del binario via build script.

## Features
- `harness-core/embed-app-server` → permite linkear App Server in-process si la surface lo prefiere.
- `module-db/postgres`, `module-db/mysql` → opt-in para reducir tamaño en builds que solo necesitan SQLite.
- `harness-mcp/http` → cliente MCP por HTTP además de stdio.

## Versionado
Workspace usa `version.workspace = true`. Bump unificado. Breaking en el protocolo JSON-RPC requiere bump mayor del campo `protocolVersion`.
