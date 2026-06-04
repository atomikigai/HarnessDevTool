---
name: opensrc
description: Fetch source code of npm, PyPI, crates.io or GitHub packages so agents can read real implementations. Use when you need to understand how a library works internally, debug unexpected behavior, or verify edge case handling. Triggers: "fetch source for", "read the source of", "how does X work internally", "get the implementation of".
metadata:
  short-description: Fetch dependency source code for deeper context
  version: "0.7.2"
  install: npm install -g opensrc
---

# opensrc — Fetch Dependency Source Code

Clones packages at the exact installed version and caches globally at `~/.opensrc/`.
Progress goes to stderr, path to stdout — compose freely with other tools.

## Core Pattern

```bash
rg "parse" $(opensrc path zod)
cat $(opensrc path zod)/src/types.ts
find $(opensrc path zod) -name "*.test.ts"
```

## Fetch by Registry

```bash
opensrc path zod                     # npm (auto-detects version from lockfile)
opensrc path pypi:requests           # PyPI
opensrc path crates:serde            # crates.io
opensrc path vercel/next.js          # GitHub repo
opensrc path zod react next          # multiple at once
opensrc path zod@3.22.0              # specific version
opensrc path crates:serde@1.0.200
```

## Version Resolution (npm)

Auto-detects from pnpm-lock.yaml, package-lock.json, yarn.lock:

```bash
opensrc path zod --cwd /path/to/project
```

## Cache Management

```bash
opensrc list                  # show all cached
opensrc list --json
opensrc remove zod
opensrc clean                 # remove all
opensrc clean --crates        # only crates.io
```

## When NOT to Use

- Simple API questions that types or docs already answer
- When you only need the README (use WebFetch instead)
