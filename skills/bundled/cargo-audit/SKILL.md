---
name: cargo-audit
description: Audit Rust dependencies for known CVEs using the RustSec advisory database. Use when adding new dependencies, before releases, or when asked about security. Triggers: "check for vulnerabilities", "audit dependencies", "are there any CVEs", "security check".
metadata:
  short-description: Audit Cargo.lock for known CVEs via RustSec
  version: "0.22.1"
  install: cargo install cargo-audit
---

# cargo-audit — Rust Dependency Security Audit

Checks `Cargo.lock` against the RustSec Advisory Database for CVEs, unmaintained crates, and unsound code.

## Basic Usage

```bash
# Run from backend/ (reads Cargo.lock)
cargo audit

# Strict mode — fail on warnings too
cargo audit --deny warnings
cargo audit --deny unmaintained
cargo audit --deny unsound
cargo audit --deny yanked
```

## Ignore Specific Advisories

```bash
cargo audit --ignore RUSTSEC-2024-0001
```

## Output Options

```bash
cargo audit --json           # machine-readable JSON
cargo audit --no-fetch       # use cached DB
```

## Audit Compiled Binaries

```bash
cargo audit bin ./target/release/harness-server
```

## In the Harness

`just audit` runs `cargo audit` from `backend/`.
`just setup` installs cargo-audit automatically.

## Permanent Ignore (`backend/audit.toml`)

```toml
[advisories]
ignore = ["RUSTSEC-2024-0001"]
```

## When NOT to Use

- Frontend deps → use `pnpm audit` instead
