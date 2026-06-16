---
name: kiss
description: Apply KISS/YAGNI rails for HarnessDevTool work. Use when the user asks for KISS, simplest/minimal solutions, less code, YAGNI, avoiding over-engineering, deleting boilerplate, replacing custom code with stdlib/native platform behavior, or reviewing whether a change can be smaller.
metadata:
  short-description: KISS/YAGNI decision rail for smaller, safer changes
  upstream-inspiration: https://github.com/DietrichGebert/ponytail
capabilities:
  kind: skill
  suggests:
    - skill:code-simplification
    - skill:code-review-and-quality
    - skill:difftastic
  trigger:
    paths:
      - backend/**
      - frontend/**
      - skills/**
      - docs/**
    keywords:
      - kiss
      - yagni
      - keep it simple
      - simplest solution
      - minimal solution
      - over engineered
      - boilerplate
      - less code
      - delete code
      - standard library
      - native platform
---

# KISS

Use the smallest solution that satisfies the task and preserves HarnessDevTool
contracts. KISS means less owned complexity, not weaker correctness.

## Decision Ladder

Before adding code, stop at the first rung that holds:

1. Does this need to exist? If not, skip it and say why.
2. Does stdlib already do it?
3. Does the browser, OS, database, CLI, or framework already do it?
4. Does an installed dependency already cover it?
5. Can a local one-liner or small helper solve it?
6. Only then add the minimum new code.

Prefer deletion over addition, fewer files over more files, and boring local
logic over new abstractions.

## Harness Boundaries

Do not simplify away:

- append-only conversation logs
- `X-Protocol-Version` frontend/backend contracts
- Rust `ts-rs` shared types as the source of truth
- input validation at trust boundaries
- security, auth, secret handling, and sandbox/permission checks
- error handling that prevents data loss
- required frontend real-user QA and `DESIGN.md` updates
- explicit user requirements

## Review Tags

For KISS review, report one finding per line:

- `delete:` dead code, unused flexibility, speculative feature
- `stdlib:` custom code replaced by stdlib
- `native:` custom/dependency code replaced by platform/framework behavior
- `yagni:` abstraction, config, or extension point with no current use
- `shrink:` same behavior with less code or fewer files

End with `net: -N lines possible` when you can estimate it. If nothing should
be cut, say `Lean already. Ship.`

## Debt Marker

When intentionally taking a shortcut with a known ceiling, leave a marker:

```text
// kiss: <ceiling>; upgrade when <trigger>
```

Examples:

```text
// kiss: linear scan is fine for small session lists; upgrade when profiles exceed 1000 sessions
// kiss: no cache until measurements show repeated calls; add cache when p95 exceeds 200ms
```

Markers without an upgrade trigger are incomplete.
