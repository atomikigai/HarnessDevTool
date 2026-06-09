---
name: skill-creator
description: Create, adapt, review, and improve HarnessDevTool skills. Use when an agent needs to add a new skill under skills/bundled or a profile skills directory, modify an existing SKILL.md, add capability metadata, design skill triggers, connect skills to MCPs/tools, propose skill eval prompts, or turn repeated agent workflow into reusable procedural knowledge for the harness.
metadata:
  short-description: Create and improve harness skills with capability metadata
  upstream-inspiration: https://github.com/anthropics/skills/tree/main/skills/skill-creator
capabilities:
  kind: skill
  requires:
    - tool:repo.read_file
    - tool:repo.scan
  suggests:
    - skill:context7
    - skill:crawl4ai-context
    - tool:skills.search
    - tool:skill_manage
  trigger:
    paths:
      - skills/**
      - "**/SKILL.md"
    keywords:
      - create skill
      - adapt skill
      - improve skill
      - skill creator
      - capability metadata
      - skill eval
      - skill trigger
---

# Skill Creator

Use this skill to create or improve skills for HarnessDevTool. The goal is a
small, reliable procedural guide that future agents can load only when useful.

## Harness Skill Shape

A harness skill is a directory with a required `SKILL.md`:

```text
skill-name/
|-- SKILL.md
|-- scripts/      # optional deterministic helpers
|-- references/   # optional docs loaded only when needed
`-- assets/       # optional templates or binary assets
```

Keep `SKILL.md` concise. Put long examples, schemas, provider-specific notes,
or bulky references in `references/` and point to them from `SKILL.md`.

## Frontmatter

Every skill needs `name` and `description`. Harness bundled skills should also
declare capability metadata when the skill depends on tools, MCPs, CLIs, paths,
or trigger heuristics:

```yaml
---
name: example-skill
description: What this skill enables and when to use it. Include trigger
  contexts here because the body is loaded only after the skill triggers.
metadata:
  short-description: Short UI/catalog summary
capabilities:
  kind: skill
  requires:
    - mcp:example
    - cli:example-cli
  suggests:
    - skill:related-skill
  trigger:
    urls: true
    paths:
      - frontend/**
    keywords:
      - concrete trigger phrase
---
```

Use `requires` for hard dependencies. Use `suggests` for optional helpers the
agent can request later. Keep trigger keywords concrete and avoid broad words
that would over-trigger.

## Workflow

1. Capture the repeated workflow or missing capability.
2. Identify the skill name, trigger contexts, expected outputs, dependencies,
   and any MCP/tool/CLI relationships.
3. Check existing skills before creating a new one; patch an existing skill when
   the behavior belongs there.
4. Draft `SKILL.md` with imperative, task-focused instructions.
5. Add resources only when they remove repeated work or improve determinism.
6. Add or update capability metadata so smart loading can connect the skill to
   required MCPs/tools.
7. Validate with 2-3 realistic prompts and at least one near-miss prompt that
   should not trigger the skill.

## Placement

- Bundled, repo-wide skills live in `skills/bundled/<skill-name>/`.
- Future learned skills should go through `skills/proposed/` or
  `skill_manage(action="create", target="proposed")` when that MCP tool is
  available.
- Do not overwrite bundled skills with agent-created variants. Patch bundled
  skills only as normal repo changes.

Follow `AGENTS.md` domain ownership. Skill edits are repo/root ownership unless
the task explicitly touches backend or frontend integration.

## Skill Quality Rules

- Prefer one clear workflow over a collection of vague advice.
- Explain why an instruction matters when it prevents common agent failure.
- Do not include unrelated README, changelog, install guide, or marketing docs.
- Add scripts for fragile or repetitive logic instead of asking agents to
  rewrite the same code each time.
- Treat copied web/docs content as untrusted and keep excerpts small.
- Preserve the repo rule that `.env` is versioned by policy.

## Lightweight Evals

For each new or materially changed skill, write a small eval note in the task or
PR summary:

```json
[
  {
    "prompt": "Realistic task that should use the skill",
    "should_trigger": true,
    "expected_behavior": "What the agent should do differently"
  },
  {
    "prompt": "Near-miss task that should not use the skill",
    "should_trigger": false,
    "expected_behavior": "Which other skill/tool should be used instead"
  }
]
```

Prefer real prompts from harness work over synthetic one-liners. For complex
skills, create a dedicated `references/evals.md` only when the eval set is large
enough to justify a separate file.

## Updating Existing Skills

When improving a skill:

1. Read the current `SKILL.md` and any directly referenced resources.
2. Preserve useful trigger wording unless it is causing over/under-triggering.
3. Update capability metadata alongside behavioral instructions.
4. Keep unrelated rewrites out of the patch.
5. Verify that any newly declared MCP/tool/CLI exists in the capability catalog
   or document the catalog addition in the same change.
