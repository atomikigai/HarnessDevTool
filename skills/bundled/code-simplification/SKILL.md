---
name: code-simplification
description: Simplify code for clarity while preserving behavior in HarnessDevTool. Use when code works but is harder to read, nested, duplicated, over-abstracted, poorly named, or accumulated complexity; use after a feature works and tests pass, during review cleanup, or when asked to refactor for readability without changing behavior.
metadata:
  short-description: Behavior-preserving simplification and readability refactors
  upstream-inspiration: https://github.com/addyosmani/agent-skills/blob/main/skills/code-simplification/SKILL.md
capabilities:
  kind: skill
  requires:
    - tool:repo.scan
    - tool:repo.read_file
  suggests:
    - skill:kiss
    - skill:ast-grep
    - skill:difftastic
    - skill:code-review-and-quality
    - skill:rust-tooling
  trigger:
    paths:
      - backend/**
      - frontend/**
    keywords:
      - simplify
      - refactor for clarity
      - reduce complexity
      - clean up
      - readability
      - duplicate logic
      - over engineered
      - nested logic
      - KISS
      - YAGNI
      - less code
      - delete code
---

# Code Simplification

Simplify code by making it easier to understand, modify, and debug while
preserving exact behavior. Fewer lines are not the goal; faster comprehension is
the goal.

## Non-Negotiables

- Preserve behavior, side effects, ordering, error paths, and public contracts.
- Follow project conventions and nearby patterns.
- Keep scope tight to the requested area or recently changed code.
- Do not mix broad refactors with feature work unless explicitly requested.
- Do not remove error handling, validation, or security checks for cleanliness.
- Do not simplify performance-critical code into a measurably slower version.

## Workflow

1. Understand before touching. Identify responsibility, callers, edge cases,
   tests, and historical context when relevant.
2. Run the KISS ladder before adding code: skip unnecessary work, use stdlib,
   use native platform/framework behavior, reuse installed dependencies, then
   prefer the smallest local change.
3. Locate concrete complexity: deep nesting, long functions, unclear names,
   duplicated logic, dead code, boolean flag arguments, wrappers with no value,
   repeated conditionals.
4. Apply one simplification at a time.
5. Run focused checks after meaningful changes.
6. Review the diff with `difftastic` or normal git diff.
7. Revert any "simplification" that is harder to understand or review.

## KISS Rails

- Prefer deletion over addition when behavior is unused or speculative.
- Prefer stdlib, browser, database, OS, CLI, and existing framework behavior
  over custom code.
- Prefer an installed dependency over adding a new dependency.
- Do not add an abstraction until it removes current complexity or matches a
  clear local pattern.
- Mark intentional shortcuts with `kiss: <ceiling>; upgrade when <trigger>`.

## Good Simplifications

- Replace deep nesting with guard clauses when error behavior is identical.
- Extract a helper when it names a real concept.
- Inline a wrapper that adds no semantics.
- Rename vague variables to domain terms.
- Remove confirmed dead code and unused imports.
- Consolidate duplicated logic when the shared abstraction is obvious.

## Bad Simplifications

- Dense ternary chains.
- Clever one-liners that require mental parsing.
- Removing a helper that gave a useful concept a name.
- Merging unrelated functions.
- Refactoring unrelated modules opportunistically.
- Changing tests to fit the refactor.

## KISS Debt

If you encounter `kiss:` markers, keep them honest. A valid marker names both
the ceiling and the trigger to revisit it. Markers without a trigger should be
reported as cleanup debt, not silently preserved.

## Harness Checks

Use checks proportional to touched domain:

```bash
cd backend && cargo test -p <crate>
cd frontend && pnpm check
```

For frontend simplification that affects UI behavior, validate with
`agent-browser` as a user. For backend/frontend contract simplification, run the
flow through the UI.

## Final Report

Include:

- What got simpler and why.
- What behavior was preserved.
- Checks run.
- Any intentionally deferred cleanup.
