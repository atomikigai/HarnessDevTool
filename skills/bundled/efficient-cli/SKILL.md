---
name: efficient-cli
description: Fast Rust/Go command-line tools for searching, inspecting, benchmarking, HTTP checks, file watching, and terminal plumbing. Use by default before slower POSIX fallbacks when working in this repo.
metadata:
  short-description: Fast agent toolbox: rg, fd, fzf/sk, xh, jaq/jq, dasel, hyperfine, tokei, watchexec, websocat, sd, uv, duckdb
  install: just setup
---

# Efficient CLI Toolbox

Prefer these tools for routine agent work. They are fast, composable, and avoid noisy output.

## Search and Select

```bash
rg "X-Protocol-Version" backend frontend docs
rg --files -g '*.rs' backend
fd 'Cargo.toml|package.json'
rg --files | fzf
rg --files | sk
```

Use `rg` for content search and `fd` for file discovery. Use `fzf` when installed; `sk` is the Rust fallback installed by `just setup`.

## Inspect Structured Data

```bash
jaq '.scripts' frontend/package.json
jq '.scripts' frontend/package.json
dasel -f docker-compose.yml '.services.backend.environment'
dasel -f .env.example '.HARNESS_HOME'
```

Use `jaq` for fast JSON work and `jq` when compatibility with existing snippets matters. Use `dasel` for YAML/TOML/XML and mixed config formats.

## Replace, Typos, and Data

```bash
sd 'oldName' 'newName' backend/crates/harness-core/src/**/*.rs
typos
duckdb -c "select count(*) from read_json_auto('events.jsonl')"
uv run scripts/analyze.py
```

Use `sd` for simple safe replacements, `typos` for quick spelling checks, `duckdb` for local log/data analysis, and `uv` for fast Python scripts and virtualenv-free one-offs.

## HTTP and Protocol Checks

```bash
xh :7777/api/health X-Protocol-Version:1
xh GET :7777/api/threads X-Protocol-Version:1 Authorization:"Bearer $HARNESS_API_TOKEN"
```

Prefer `xh` for readable manual API checks. Keep protocol headers explicit.

## Benchmark and Size Checks

```bash
hyperfine 'cargo test -p harness-core' 'cargo nextest run -p harness-core'
tokei backend frontend
```

Use `hyperfine` when comparing test runners, startup commands, or log-processing paths. Use `tokei` for quick codebase sizing.

## Watch and Stream

```bash
watchexec -e rs,toml -w backend 'cd backend && cargo check --workspace'
websocat ws://localhost:7777/api/events
lsof -i :7777
strace -f -p "$PID"
socat - TCP:localhost:7777
```

Use `watchexec` for local feedback loops. Use `websocat` for WebSocket/SSE-adjacent debugging when the app exposes stream endpoints. Use `lsof`, `strace`, and `socat` for process, PTY, port, and socket debugging.

## When Not to Use

- Do not use fuzzy selectors in non-interactive scripts.
- Do not use `jaq` output as a patch format; use proper file edits.
- Do not benchmark commands while background dev servers are rebuilding unless that noise is the thing being measured.
- Do not run `strace` broadly unless you know which child process or PID you are diagnosing.
