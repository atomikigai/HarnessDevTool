---
name: cargo-nextest
description: Faster Rust test runner. Drop-in replacement for cargo test — each test runs in its own process. Use for running Rust tests in the harness backend. Triggers: "run tests", "run cargo test", "check if tests pass".
metadata:
  short-description: Next-generation Rust test runner — up to 3× faster
  version: "0.9.137"
  install: cargo install --locked cargo-nextest
---

# cargo-nextest — Next-Generation Rust Test Runner

Each test runs in its own process. Up to 3× faster than `cargo test`, cleaner output, better isolation.

## Basic Usage

```bash
# Run from backend/ directory
cargo nextest run                        # all tests in workspace
cargo nextest run -p harness-core        # specific package
cargo nextest run scheduler              # tests matching filter
cargo nextest run -v                     # verbose
```

## Filtering

```bash
cargo nextest run scheduler              # name contains "scheduler"
cargo nextest run -E 'test(exact:"test_append_event")'  # exact name
cargo nextest run -p harness-core event  # package + pattern
cargo nextest run -- --ignored           # only ignored tests
```

## Output

```bash
cargo nextest run --no-capture           # show stdout/stderr live
cargo nextest run --failure-output immediate
cargo nextest list                       # list without running
cargo nextest list -p harness-core
```

## In the Harness

`just test` uses cargo-nextest automatically when installed.
Run from `backend/` or via `just test` from repo root.

## When NOT to Use

- `just gen-types` → uses `cargo test --features ts-export` directly
- Benchmarks → use `cargo bench` (nextest bench is experimental)
