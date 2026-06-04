---
name: difftastic
description: Structural diff that understands syntax. Use instead of git diff when reviewing code changes — shows semantic changes without noise. Triggers: "show me what changed", "review this diff", "compare these files structurally".
metadata:
  short-description: Structural diff using tree-sitter — no noise, real changes
  version: "0.69.0"
  install: cargo install difftastic
---

# difftastic — Structural Diff Tool

Parses files with tree-sitter and diffs at the AST level. Renamed variables and moved blocks show as what they are, not as delete+add.

## Compare Two Files

```bash
difft old.rs new.rs
difft old.ts new.ts
```

## Use as git diff driver

```bash
GIT_EXTERNAL_DIFF=difft git diff              # staged changes
GIT_EXTERNAL_DIFF=difft git show              # last commit
GIT_EXTERNAL_DIFF=difft git diff main..HEAD   # between branches
```

## Display Modes

```bash
difft --display side-by-side old.rs new.rs    # default
difft --display inline old.rs new.rs          # single column
difft --display json old.rs new.rs            # machine-readable
```

## Options

```bash
difft --context 5 old.rs new.rs              # lines of context (default: 3)
difft --width 120 old.rs new.rs              # column width
difft --background light old.rs new.rs       # for light terminals
```

## In the Harness

The `git_diff` MCP tool uses difftastic automatically when `difft` is in PATH.
Pass `"raw": true` in tool arguments to force plain git diff.

## When NOT to Use

- Generating patch format for `git apply` → difftastic is display-only
- Very large files (>10k lines) → may be slow
