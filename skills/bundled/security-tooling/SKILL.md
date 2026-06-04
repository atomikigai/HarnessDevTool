---
name: security-tooling
description: Fast security and supply-chain checks for secrets, Rust advisories, multi-ecosystem CVEs, containers, Dockerfiles, and shell scripts.
metadata:
  short-description: gitleaks, osv-scanner, trivy, cargo-audit, cargo-deny, shellcheck, hadolint, typos
  install: just setup
---

# Security Tooling

Use these checks before releases, Docker changes, dependency changes, and auth/token-store changes.

## Secrets

```bash
gitleaks detect --source . --redact
gitleaks protect --staged --redact
```

Use `--redact` by default. Do not paste secrets found by scanners into issues or logs.

## Vulnerabilities

```bash
osv-scanner --lockfile backend/Cargo.lock
osv-scanner --lockfile frontend/pnpm-lock.yaml
cd backend && cargo audit
cd backend && cargo deny check
```

Use `osv-scanner` for cross-ecosystem lockfiles. Keep `cargo audit` and `cargo deny` for Rust-specific advisories and policy.

## Containers and Scripts

```bash
trivy fs .
trivy image harness/backend:latest
hadolint backend/Dockerfile frontend/Dockerfile
shellcheck scripts/*.sh
typos
```

Use `trivy fs` for repo-level dependency and config scanning. Use `trivy image` after building Docker images. Use `typos` as a fast low-risk docs/code spelling pass.

## When Not to Use

- Do not run full image scans in tight edit loops unless the change affects Docker or dependencies.
- Do not auto-fix scanner findings without checking whether they are exploitable in the harness threat model.
- Do not suppress findings without a documented reason.
