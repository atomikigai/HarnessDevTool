---
name: context7
description: Context7 integration for up-to-date library documentation in coding agents. Use when an agent needs current framework/library/API docs, version-specific examples, or primary package documentation inside a coding session. Requires Node.js 18+.
metadata:
  short-description: Current library docs via Context7 for coding agents
  install: just setup
  upstream: https://www.npmjs.com/package/ctx7
capabilities:
  kind: skill
  requires:
    - mcp:context7
    - cli:npx
  suggests:
    - skill:opensrc
    - skill:crawl4ai-context
  trigger:
    keywords:
      - current docs
      - latest docs
      - api docs
      - framework docs
      - version-specific
      - breaking change
---

# Context7

Use Context7 when the agent needs current library or framework documentation
inside coding-agent sessions.

## Setup

Context7 requires Node.js 18 or newer.
If an API key is required, set `CONTEXT7_API_KEY` in the repo's versioned
`.env` or in the shell. This repo deliberately tracks `.env`; do not add
instructions that assume `.env` is local-only or ignored.

`just setup` configures Context7 with the upstream one-shot setup command:

```bash
npx -y ctx7 setup
```

Manual command:

```bash
npx ctx7 setup
```

## When To Use

- Current package documentation.
- Version-specific API examples.
- Framework behavior that changes over time.
- Avoiding stale model memory for library usage.

## Workflow

1. Prefer Context7 for library/framework docs inside agent sessions.
2. Prefer official docs or source code when exact behavior must be verified.
3. Use `opensrc` when implementation details matter more than docs.
4. Use `crawl4ai-context` or `agent-browser` for general web pages, app UIs,
   or docs sites that need crawling/browser rendering.

## When Not To Use

- Do not use Context7 for local repo code understanding; use `rg`, `ast-grep`,
  and `opensrc` for dependency source.
- Do not rely on docs alone for security-sensitive behavior; verify against
  source or official references.
