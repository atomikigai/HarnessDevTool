---
name: performance-optimization
description: Measure, diagnose, fix, and verify performance issues in HarnessDevTool. Use when a task mentions slow UI, sluggish interactions, slow API endpoints, large payloads, N+1 queries, excessive bundle/runtime cost, Core Web Vitals, regressions, profiling, latency, throughput, memory growth, or when a frontend/backend change may affect user-perceived performance.
metadata:
  short-description: Measure-first performance optimization for frontend and backend
  upstream-inspiration: https://github.com/addyosmani/agent-skills/blob/main/skills/performance-optimization/SKILL.md
capabilities:
  kind: skill
  requires:
    - tool:repo.scan
    - tool:repo.read_file
  suggests:
    - skill:agent-browser
    - skill:rust-tooling
    - skill:efficient-cli
    - skill:code-review-and-quality
    - mcp:playwright
  trigger:
    paths:
      - frontend/**
      - backend/**
    keywords:
      - performance
      - slow
      - latency
      - sluggish
      - profile
      - benchmark
      - core web vitals
      - bundle size
      - n+1
      - memory growth
---

# Performance Optimization

Use this skill for evidence-driven performance work. Measure before optimizing.
Do not add complexity for a guessed bottleneck.

## Workflow

1. Define the user-visible symptom or performance requirement.
2. Establish a baseline with concrete numbers.
3. Identify the bottleneck from evidence.
4. Fix the specific bottleneck.
5. Measure again and compare before/after.
6. Add a guard when practical: test, benchmark, budget, alert, or review note.

Final reports for performance work must include before/after measurements or a
clear reason measurement was blocked.

## Frontend Measurement

Use `agent-browser` for real-user validation and visible performance symptoms.
Inspect loading, interaction lag, layout shift, render stalls, console errors,
network waterfalls, and oversized UI states.

Core Web Vitals targets:

- LCP good: <= 2.5s.
- INP good: <= 200ms.
- CLS good: <= 0.1.

For HarnessDevTool frontend work:

```bash
cd frontend
pnpm check
pnpm build
```

Use Playwright only for stable repeatable performance-sensitive regressions. Do
not use it as the primary exploratory profiler.

## Backend Measurement

For Rust/backend work, prefer focused timings and existing tests before broad
profiling:

```bash
cd backend
cargo test -p harness-server routes::sessions
cargo nextest run -p harness-core
```

Use `hyperfine` for command comparisons, `cargo flamegraph` for CPU-heavy
paths, `cargo bloat` for binary size concerns, and targeted tracing/log timing
for API or store operations.

## Common Bottlenecks

- N+1 queries or repeated store reads.
- Unbounded list endpoints or missing pagination.
- Large payloads crossing frontend/backend boundaries.
- Excessive Svelte rerendering or derived state churn.
- Layout shift from unstable dimensions.
- Long synchronous CPU work on request or UI paths.
- Missing cache for frequently-read rarely-changed data.
- Repeated shell/process startup in hot paths.

## Harness-Specific Guardrails

- Preserve append-only event semantics while optimizing storage paths.
- Do not weaken `X-Protocol-Version` checks for speed.
- Do not edit generated frontend API types by hand.
- If frontend behavior changes, validate with `agent-browser` as a user.
- If a backend/frontend contract affects UI flow, run both services and validate
  from the UI.

## Verification Checklist

- Baseline and after numbers are recorded.
- The specific bottleneck is named.
- The fix addresses that bottleneck, not a nearby guess.
- Existing tests/checks pass.
- UI performance changes have browser evidence.
- No new complexity was added without measurable benefit.
