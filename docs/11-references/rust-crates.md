---
id: references/rust-crates
title: Crates Rust recomendados
shard: 11-references
tags: [references, rust, crates]
summary: Tabla de crates externos con justificación.
related: [harness-core/rust-crate-layout, build-plan/tech-stack-locked]
sources: []
---

# Crates externos

## Runtime / IO
| Crate | Uso |
|---|---|
| `tokio` (`full`) | runtime async multi-thread |
| `tokio-util` | cancellation, codec, sync utils |
| `futures` | combinators, `FuturesOrdered` |
| `async-trait` | traits async |

## HTTP / Web
| Crate | Uso |
|---|---|
| `axum` | framework HTTP del `harness-server` |
| `tower-http` | cors, trace, compression, timeout, fs |
| `tower` | middleware |
| `hyper` | underlying HTTP (Axum lo usa) |
| `reqwest` (`stream`) | cliente HTTP (rara vez; mayoría va por CLI hijo) |

## Serialization / Schemas
| Crate | Uso |
|---|---|
| `serde` + `serde_json` | JSON |
| `toml` | configs simples |
| `toml_edit` | preservar comentarios y orden al editar TOML |
| `serde_yaml` | frontmatter de memory/skills |
| `jsonschema` | validación runtime |
| `schemars` | derive de JSON Schema |
| `ts-rs` | derive de tipos TS (bindings) |

## Storage
| Crate | Uso |
|---|---|
| `sqlx` (`sqlite-bundled`, `postgres`, `mysql` opt-in) | DBs |
| `rusqlite` | si necesitas algo más bare-metal |

## PTY / SSH
| Crate | Uso |
|---|---|
| `portable-pty` | PTY cross-OS (Linux pty, macOS pty, ConPTY) |
| `russh` | SSH client puro Rust |
| `russh-sftp` | SFTP extension |
| `russh-keys` | parser de claves OpenSSH |

## Sandbox / OS
| Crate | Uso |
|---|---|
| `seccompiler` | seccomp-bpf en Linux |
| `nix` | syscalls Unix (namespaces, mounts) |
| `windows-rs` | AppContainer en Windows (F6) |

## Observabilidad
| Crate | Uso |
|---|---|
| `tracing` + `tracing-subscriber` | logs + spans |
| `tracing-appender` | rotación |

## Errores
| Crate | Uso |
|---|---|
| `thiserror` | errores tipados en libs |
| `anyhow` | bins / tests |

## CLI / UX
| Crate | Uso |
|---|---|
| `clap` | parsing flags del `harness-server` |
| `indicatif` | progress bars (en CLI post-F6) |
| `crossterm` | TTY (post-F6) |

## Crypto / Secrets
| Crate | Uso |
|---|---|
| `keyring` | secret store nativo |
| `age` | cifrado opcional de threads/exports |
| `rustls` | TLS puro Rust |

## Git
| Crate | Uso |
|---|---|
| `git2` | acceso a libgit2 (commits del profile, log, diff) |
| (alternativa: `gix`) | git puro Rust, en evaluación |

## Identifiers / Time
| Crate | Uso |
|---|---|
| `uuid` (`v7`) | thread/turn/item/spawn ids con orden temporal |
| `time` | timestamps ISO 8601 (NO `chrono` por mantenibilidad) |

## Inventory / registry
| Crate | Uso |
|---|---|
| `linkme` | distributed slice para auto-registrar tools del harness-bridge |
| `inventory` | alternativa similar |

## Lo que NO está

- ❌ `tauri` — descartado.
- ❌ Crates de provider directo (`anthropic-sdk`, `openai-api-rust`) — el CLI hijo habla con providers.
- ❌ `napi-rs` / `pyo3` (FFI) — no aplican en v1.
- ❌ `gRPC` — usamos HTTP+SSE.
