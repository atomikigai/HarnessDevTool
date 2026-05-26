---
id: references/rust-crates
title: Crates Rust recomendados
shard: 11-references
tags: [references, rust, crates]
summary: Tabla de crates externos con justificación.
related: [harness-core/rust-crate-layout]
sources: []
---

# Crates externos

## Runtime / IO
| Crate | Uso |
|---|---|
| `tokio` | runtime async multi-thread |
| `tokio-util` | utilities (cancellation, codec) |
| `futures` | `FuturesOrdered`, combinators |
| `async-trait` | traits async en bordes |

## Serialization / Schemas
| Crate | Uso |
|---|---|
| `serde` + `serde_json` | JSON |
| `toml` | configs |
| `serde_yaml` | frontmatter de specs |
| `jsonschema` | validación runtime |
| `schemars` | derive de JSON Schema desde structs |

## Storage
| Crate | Uso |
|---|---|
| `sqlx` | DBs (sqlite/postgres/mysql) |
| `rusqlite` | si necesitas algo más bare-metal en módulo-db |

## SSH / PTY / Sandbox
| Crate | Uso |
|---|---|
| `russh` | cliente SSH puro Rust |
| `russh-sftp` | SFTP |
| `russh-keys` | parser de claves |
| `portable-pty` | PTY cross-OS |
| `seccompiler` | seccomp en Linux |
| `nix` | syscalls Unix (namespaces, mounts) |
| `windows-rs` | AppContainer en Windows |

## Observabilidad
| Crate | Uso |
|---|---|
| `tracing` + `tracing-subscriber` | logs + spans |
| `tracing-appender` | rotación |

## Errores
| Crate | Uso |
|---|---|
| `thiserror` | errores tipados en libs |
| `anyhow` | errores ad-hoc en binarios / tests |

## HTTP / Providers
| Crate | Uso |
|---|---|
| `reqwest` (con `stream`) | HTTP client + SSE |
| `eventsource-client` | SSE robusto |
| `hyper` (si necesitas servidor HTTP, p.ej. web gateway) | |

## CLI / UX
| Crate | Uso |
|---|---|
| `clap` | parsing de flags |
| `indicatif` | progress bars |
| `crossterm` | TTY |

## Crypto / Secrets
| Crate | Uso |
|---|---|
| `keyring` | secret store nativo |
| `age` | cifrado opcional de threads |
| `rustls` | TLS puro Rust |

## Tauri
| Crate | Uso |
|---|---|
| `tauri` | desktop shell |
| `tauri-plugin-shell` | spawn sidecar |
