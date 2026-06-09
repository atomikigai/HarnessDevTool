---
name: frontend-testing
description: Frontend validation for HarnessDevTool SvelteKit UI. Use when changing routes, layouts, interaction flows, rendering, streaming UI, responsive behavior, or user-facing state. Prefer real browser inspection with agent-browser for usability and visual truth; use Playwright only for narrow repeatable regressions.
metadata:
  short-description: Browser-first frontend validation with agent-browser, svelte-check, and focused Playwright
  install: just setup
---

# Frontend Testing

Validate frontend work in the same order a user experiences it: compile, open
the app in a real browser, inspect behavior, then add or run automated browser
tests only when they protect a stable regression.

## Baseline Checks

Run static checks after TypeScript/Svelte changes:

```bash
cd frontend
pnpm check
```

Use lint/format when touching shared style, test config, or broader frontend
areas:

```bash
cd frontend
pnpm lint
```

## Browser-First Validation

Use `agent-browser` for manual-quality browser validation. It gives compact
agent-readable output while still exercising a real browser.

```bash
agent-browser open http://localhost:5173
agent-browser open http://localhost:5173/threads
```

Prefer this path when checking:

- whether the app actually renders;
- layout, overflow, clipping, density, and responsive behavior;
- keyboard/mouse flows;
- stream-driven UI state;
- forms, dialogs, tabs, navigation, and disabled/loading states;
- whether visible copy and controls make sense to a user.

If the browser runtime is missing:

```bash
agent-browser install
```

## Playwright

Use Playwright for focused repeatable checks, not as the only source of truth.
Write or run a Playwright test when a bug has a stable browser-observable
contract that should not regress.

```bash
cd frontend
pnpm test:e2e -- tests/e2e/thread.spec.ts
pnpm test:e2e -- --grep "opens thread"
```

Keep Playwright runs narrow. The repo config is intentionally fast-fail:
Chromium only, 15s test timeout, 3s expect timeout, 5s action timeout.

## Choosing The Tool

- `pnpm check`: catches Svelte/TypeScript contract failures.
- `agent-browser`: validates real user behavior and visual/runtime truth.
- Playwright: locks down a known, repeatable regression.
- Browser screenshots/traces: use only when visual state cannot be summarized
  clearly in text.

## When Not To Use

- Do not treat passing E2E tests as proof the UI is good.
- Do not run broad Playwright suites repeatedly while iterating.
- Do not use Playwright as a crawler or exploratory browser.
- Do not skip real browser inspection for layout or interaction changes.
