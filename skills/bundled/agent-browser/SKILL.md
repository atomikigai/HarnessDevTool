---
name: agent-browser
description: Unified frontend/browser testing skill for HarnessDevTool agents. Use for QA, frontend validation, exploratory testing, browser rendering checks, screenshots, rendered DOM inspection, forms, navigation, local webapp verification, console/network evidence, responsive/layout checks, and deciding whether a stable Playwright regression should be added. Prefer agent-browser runtime skills for exploratory browser work; use Playwright only for narrow repeatable regressions.
metadata:
  short-description: Unified frontend QA and browser validation with agent-browser first
  install: just setup
  upstream: https://www.npmjs.com/package/agent-browser
capabilities:
  kind: skill
  requires:
    - cli:agent-browser
  suggests:
    - cli:pnpm
    - cli:python3
    - skill:frontend-design
    - mcp:playwright
  trigger:
    paths:
      - frontend/**
      - "**/*.svelte"
      - "**/*.html"
      - "**/*.css"
      - "**/*.tsx"
      - "**/*.jsx"
    keywords:
      - browser
      - frontend test
      - webapp test
      - qa
      - dogfood
      - exploratory test
      - screenshot
      - inspect page
      - rendered dom
      - console log
      - responsive
      - e2e
---

# Agent Browser

Use this as the single browser/frontend testing skill. It covers three jobs:

1. Harness frontend validation (`pnpm check`, real browser inspection).
2. QA/exploratory testing (`agent-browser` runtime skills, especially dogfood).
3. Regression decisions (write or run Playwright only for stable contracts).

`agent-browser` is the primary browser tool for QA/frontend agents. Playwright is
not the default exploration tool; it is for focused repeatable tests after the
behavior is understood.

## Install

`just setup` installs the CLI globally and runs the browser bootstrap:

```bash
npm install -g agent-browser
agent-browser install
```

If the browser runtime is missing:

```bash
agent-browser install
```

## Runtime Skills

The CLI serves current instructions that match the installed `agent-browser`
version. Load them at runtime instead of relying on stale copied command docs.

```bash
agent-browser skills list
agent-browser skills get core --full
agent-browser skills get dogfood --full
```

Use `core --full` for normal browser automation: navigation, snapshots, forms,
screenshots, extraction, sessions, authentication, and command reference.

Use `dogfood --full` for systematic exploratory QA: navigate like a real user,
find bugs/UX issues, and produce a structured report with evidence.

Use `agent-browser skills get <name>` for other installed runtime skills only
when the task clearly matches them. All `agent-browser skills` commands support
`--json`.

## Harness Frontend Checks

For HarnessDevTool SvelteKit frontend work:

```bash
cd frontend
pnpm check
```

Use lint/format only when touching shared style, test config, or broad frontend
areas:

```bash
cd frontend
pnpm lint
```

Then validate in a real browser:

```bash
agent-browser skills get core --full
agent-browser open http://localhost:5173
agent-browser open http://localhost:5173/threads
```

Check rendering, overflow, clipping, density, responsive behavior, forms,
dialogs, tabs, navigation, disabled/loading states, and stream-driven UI state.

## QA Workflow

Use this sequence for bugs and exploratory testing:

1. Define the target URL, role/persona, and workflow under test.
2. Load `agent-browser skills get core --full`; load `dogfood --full` for broad
   exploratory QA.
3. Navigate and inspect rendered state before acting.
4. Capture relevant screenshot/DOM/console/network evidence.
5. Reproduce the smallest action sequence that proves the bug or fix.
6. Report whether the issue is reproduced, fixed, or inconclusive.
7. Add or run Playwright only if the behavior is stable enough to guard as a
   regression.

Evidence report:

```text
URL:
Viewport:
Tool: agent-browser core|dogfood
Observed:
Evidence:
Console/network:
Result: reproduced|fixed|inconclusive
Playwright follow-up: yes|no, reason
```

## Safe Server Helper

If the app is not running, use the bundled helper to start one or more servers,
wait for ports, run the browser check, and clean up.

Always run `--help` first:

```bash
python3 skills/bundled/agent-browser/scripts/with_server.py --help
```

Single server:

```bash
python3 skills/bundled/agent-browser/scripts/with_server.py \
  --server "pnpm dev --host 127.0.0.1" --cwd frontend --port 5173 \
  -- agent-browser open http://127.0.0.1:5173
```

Multiple servers:

```bash
python3 skills/bundled/agent-browser/scripts/with_server.py \
  --server "cargo run -p harness-server" --cwd backend --port 7777 \
  --server "pnpm dev --host 127.0.0.1" --cwd frontend --port 5173 \
  -- agent-browser open http://127.0.0.1:5173
```

The helper avoids shell execution by default. Use `--cwd` instead of
`cd frontend && ...`. Use `--shell` only for trusted human-authored commands
that genuinely need shell features.

## Playwright Rule

Use Playwright for focused repeatable checks, not as the first source of truth.
Write or run a Playwright test when a bug has a stable browser-observable
contract that should not regress.

```bash
cd frontend
pnpm test:e2e -- tests/e2e/thread.spec.ts
pnpm test:e2e -- --grep "opens thread"
```

Keep Playwright runs narrow. The repo config is intentionally fast-fail:
Chromium only, 15s test timeout, 3s expect timeout, 5s action timeout.

Do not use Playwright as a crawler or general exploratory browser.

## When Not To Use

- Simple static HTTP/text retrieval: use `xh`, `curl`, or official API/docs.
- Docs crawling: use `crawl4ai-context`.
- Pure visual design direction without validation: use `frontend-design`, then
  return here for browser evidence.
