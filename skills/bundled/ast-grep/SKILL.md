---
name: ast-grep
description: Structural code search and rewrite using AST patterns. Use when ripgrep is not precise enough — when you need to find code by structure, not text. Triggers: "find all X that do Y", "find functions without error handling", "rename this pattern everywhere", "find all usages of this API".
metadata:
  short-description: Search and rewrite code by AST structure
  version: "0.43.0"
  install: npm install -g @ast-grep/cli
---

# ast-grep — Structural Code Search & Rewrite

Searches code by AST pattern, not text. Understands Rust, TypeScript, Svelte, Python, Go and 30+ languages.

## Core Pattern

```bash
# -p: pattern  -l: language  -r: rewrite (optional)
ast-grep run -p 'console.log($A)' -l ts
ast-grep run -p 'fn $NAME($$$) { $$$ }' -l rust .
```

## Metavariables

- `$NAME` — matches any single AST node
- `$$$` — matches zero or more nodes (variadic)
- `$_` — matches any node, unnamed

## Useful Patterns

```bash
# Rust: find all .unwrap() calls
ast-grep run -p '$X.unwrap()' -l rust backend/

# Rust: find async functions
ast-grep run -p 'async fn $NAME($$$) { $$$ }' -l rust backend/

# TypeScript: find all console.log
ast-grep run -p 'console.log($$$)' -l ts frontend/src/

# Svelte: find on:click handlers
ast-grep run -p 'on:click={$HANDLER}' -l svelte frontend/

# Rewrite: rename a function call
ast-grep run -p 'oldFn($ARGS)' -r 'newFn($ARGS)' -l ts frontend/src/
```

## Output Formats

```bash
ast-grep run -p '$X.unwrap()' -l rust --json     # machine-readable
```

## When NOT to Use

- Simple string search → use `rg` (faster for exact text)
- Searching comments or string literals (AST ignores those)
