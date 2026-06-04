---
name: frontend-testing
description: Frontend validation for the SvelteKit UI using svelte-check and bounded Playwright tests. Use when changing frontend routes, interactions, rendering, navigation, or stream-driven UI behavior.
metadata:
  short-description: Bounded Playwright E2E and Svelte checks for frontend work
  install: cd frontend && pnpm install
---

# Frontend Testing

Use Playwright for targeted browser validation, not open-ended exploration. Keep runs narrow and inspect failures directly.

## Static Checks

```bash
cd frontend
pnpm check
pnpm lint
```

Run `pnpm check` after TypeScript/Svelte changes. Run `pnpm lint` for formatting/lint coverage.

## Playwright

```bash
cd frontend
pnpm test:e2e
pnpm test:e2e -- tests/e2e/thread.spec.ts
pnpm test:e2e -- --grep "opens thread"
```

The repo config uses Chromium only, 15s test timeout, 3s expect timeout, 5s action timeout, and no video by default. This is intentional: browser tests should fail fast.

## Debug a Single Test

```bash
cd frontend
pnpm test:e2e:headed -- tests/e2e/thread.spec.ts --grep "opens thread"
```

Use headed mode only for one failing test. Do not leave it running while editing unrelated code.

## When Not to Use

- Do not run broad Playwright suites repeatedly while implementing backend-only changes.
- Do not use Playwright as a crawler to discover app behavior.
- Do not wait more than a few minutes on a browser test run without checking the active process, server logs, and screenshot/trace output.
