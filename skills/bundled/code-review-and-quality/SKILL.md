---
name: code-review-and-quality
description: Conduct multi-axis code review for HarnessDevTool changes. Use before merging, before committing substantial work, when reviewing code written by an agent or human, after bug fixes, after refactors, or when assessing correctness, readability, architecture, security, performance, tests, frontend QA evidence, and project convention compliance.
metadata:
  short-description: Multi-axis code review and quality gates
  upstream-inspiration: https://github.com/addyosmani/agent-skills/blob/main/skills/code-review-and-quality/SKILL.md
capabilities:
  kind: skill
  requires:
    - tool:repo.git_diff
    - tool:repo.read_file
  suggests:
    - skill:kiss
    - skill:difftastic
    - skill:security-tooling
    - skill:performance-optimization
    - skill:code-simplification
    - skill:agent-browser
  trigger:
    paths:
      - backend/**
      - frontend/**
      - skills/**
      - docs/**
    keywords:
      - code review
      - review this
      - quality gate
      - before merge
      - audit change
      - assess diff
      - lgtm
      - KISS review
      - over engineered
      - what can we delete
---

# Code Review and Quality

Use this skill to review changes before they enter main. Review the diff, not
the author. Approve when the change improves the codebase and follows local
conventions; do not block on personal preference.

## Review Axes

1. **Correctness**: Does it satisfy the task? Are edge cases and error paths
   handled? Do tests verify behavior?
2. **Readability**: Are names clear? Is control flow easy to follow? Are
   abstractions earning their complexity?
3. **Architecture**: Does it respect domain ownership and module boundaries?
   Does it fit existing patterns?
4. **Security**: Are inputs validated, secrets protected, SQL/HTML safe, and
   untrusted content treated carefully?
5. **Performance**: Any N+1 patterns, unbounded fetching, needless rerenders,
   large payloads, or hot-path sync work?
6. **Verification**: Were the right checks run? For frontend, is there
   real-user `agent-browser` evidence?
7. **KISS**: Can the same behavior be achieved by deleting code, using stdlib,
   using native platform/framework behavior, avoiding a new dependency, or
   delaying speculative abstraction?

## Harness-Specific Gates

- Conversation logs remain append-only.
- HTTP frontend/backend calls keep `X-Protocol-Version`.
- Rust shared types remain the source of truth; generated TS is not edited by
  hand.
- `.env` remains versioned by policy.
- Frontend changes include real-user QA with `agent-browser` or a concrete
  blocker.
- Backend changes affecting frontend contracts are validated through the UI.
- `DESIGN.md` is read/updated when visual system rules change.
- Domain ownership is respected unless the task justifies crossing paths.

## Review Process

1. Understand intent: what was supposed to change?
2. Inspect changed files and relevant local context.
3. Review tests/checks first when available.
4. Review implementation across the axes above.
5. Classify findings by severity.
6. Summarize residual risk and missing verification.

## Finding Format

Lead with findings. Use file and line references when possible.

```text
Critical: ...
Important: ...
Suggestion: ...
Nit: ...
```

Critical blocks merge. Important should be addressed before merge. Suggestions
are optional tradeoffs. Nits are style-only and should be rare.

## Change Size

Small focused changes are safer. If a change mixes feature work, refactor,
generated files, dependency changes, and UI polish, recommend splitting unless
the coupling is necessary.

As a rule of thumb:

- About 100 changed lines: reviewable.
- About 300 changed lines: acceptable for one logical change.
- About 1000 changed lines: ask to split unless mostly mechanical deletion or
  generated output.

## Dependency Review

Before approving a new dependency:

- Can the existing stack solve this?
- Is it maintained?
- Does it affect frontend bundle size or backend image size?
- Are there known vulnerabilities?
- Is the license acceptable?

Prefer local utilities and existing dependencies over new ones.

## Verification Checklist

- Correct tests/checks ran.
- Frontend UI was inspected as a user when relevant.
- Security and performance risks were considered.
- No generated files were hand-edited.
- No unrelated changes were mixed in.
- The final summary states remaining risk.

## KISS Review Mode

When the request is specifically about KISS, over-engineering, deletion, or
unnecessary code, keep the review focused on complexity. Use one line per
finding:

```text
<file>:L<line>: <tag> <what to cut>. <replacement>.
```

Tags:

- `delete:` dead code, unused flexibility, speculative feature
- `stdlib:` custom code replaced by stdlib
- `native:` custom/dependency code replaced by platform/framework behavior
- `yagni:` abstraction, config, or extension point with no current use
- `shrink:` same behavior with less code or fewer files

End with `net: -N lines possible` when you can estimate it. If there is
nothing worthwhile to cut, say `Lean already. Ship.`
