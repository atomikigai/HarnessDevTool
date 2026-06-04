---
name: rust-tooling
description: Rust backend quality and performance tools for HarnessDevTool. Use when touching backend crates, dependencies, tests, release size, or async/performance behavior.
metadata:
  short-description: cargo-nextest, cargo-audit, cargo-deny, cargo-machete, cargo-bloat, cargo-edit, cargo-flamegraph
  install: just setup
---

# Rust Tooling

Use these tools from `backend/` unless noted otherwise.

## Test

```bash
cargo nextest run
cargo nextest run -p harness-core
cargo nextest run threads::routes
```

Use `cargo nextest` for normal Rust test runs. Fall back to `cargo test` only when nextest is unavailable or when doctest behavior is the target.

## Dependencies and Security

```bash
cargo audit
cargo deny check
cargo machete
```

Use `cargo audit` for RustSec advisories. Use `cargo deny check` before dependency policy changes or releases. Use `cargo machete` after removing code or feature flags.

## Dependency Editing

```bash
cargo add tracing-subscriber --workspace
cargo rm unused-crate -p harness-core
cargo upgrade --workspace
```

Use `cargo add`/`cargo rm` instead of manually editing dependency tables when possible.

## Binary and Performance

```bash
cargo bloat --release -p harness-server --bin harness-server
cargo flamegraph -p harness-server --bin harness-server
```

Use `cargo bloat` when release size or image weight matters. Use `cargo flamegraph` for CPU-heavy backend behavior; it may need host perf permissions.

## When Not to Use

- Do not treat `cargo check` as enough for changes in PTY, append-only logs, policy, or protocol contracts.
- Do not run broad dependency upgrades inside unrelated feature work.
- Do not commit generated TS bindings unless the Rust `ts-rs` source changed and `just gen-types` was run.
