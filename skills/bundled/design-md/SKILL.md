---
name: design-md
description: Create, audit, and maintain DESIGN.md files for repos that pass through HarnessDevTool. Use when an agent needs to extract a frontend design system from existing UI/CSS/screens, document visual tokens and rationale, keep UI changes consistent, update design documentation after style changes, or give frontend-design a stable source of truth before changing UI.
metadata:
  short-description: Generate and maintain DESIGN.md as design source of truth
  upstream-inspiration: https://officialskills.sh/google-labs-code/skills/design-md
capabilities:
  kind: skill
  requires:
    - skill:agent-browser
    - tool:repo.scan
    - tool:repo.read_file
  suggests:
    - skill:frontend-design
    - cli:agent-browser
  trigger:
    paths:
      - DESIGN.md
      - frontend/DESIGN.md
      - frontend/**
      - "**/*.css"
      - "**/*.svelte"
    keywords:
      - design.md
      - design system
      - visual consistency
      - extract design
      - document frontend style
      - update design docs
      - design source of truth
---

# DESIGN.md

Use this skill to create or update a `DESIGN.md` file that gives agents a
persistent visual source of truth for a repo. Every repo with meaningful
frontend work should have one.

## Placement

- If the repo has `frontend/`, use `frontend/DESIGN.md`.
- If the frontend lives at repo root, use `DESIGN.md`.
- If there are multiple independent apps, create one `DESIGN.md` per app root.

Update the file whenever a task changes design tokens, global CSS, layout
patterns, component styling, or the product's visual direction.

## Source Material

Inspect:

1. Global CSS and theme tokens.
2. Shared components, shells, panels, forms, tables, badges, tabs, dialogs.
3. Existing screens and route layout.
4. Rendered UI evidence with `agent-browser` when available.
5. Any screenshots, design notes, or user feedback attached to the task.

Do not infer a brand system from one isolated component unless that is all the
repo has. Mark uncertain rules as provisional.

## File Shape

Use YAML frontmatter for exact tokens and Markdown for rationale:

```md
---
name: Product Name
version: 1
modes:
  default: operational-app
colors:
  accent: "#0e7864"
typography:
  sans: "Inter, ui-sans-serif, system-ui"
  mono: "JetBrains Mono, ui-monospace"
---

# DESIGN.md

## Design Intent
...
```

Prefer semantic roles over raw implementation details. Include exact values
where they matter, but explain when and why to use them.

## Required Sections

- `Design Intent`
- `Modes`
- `Color Roles`
- `Typography`
- `Spacing, Radius, and Density`
- `Surfaces and Layout`
- `Components and Controls`
- `States`
- `Motion`
- `Do / Do Not`
- `QA Expectations`
- `Maintenance`

## Harness Defaults

For HarnessDevTool-style operational apps:

- Default to dense, scannable, user-friendly product UI.
- Preserve task/session/state visibility.
- Prefer panes, rails, tables, tabs, compact panels, and stable controls.
- Avoid marketing-page composition unless explicitly requested.
- Validate changes as a user with `agent-browser`.

## Maintenance Rules

When changing frontend styles:

1. Read `DESIGN.md` before editing UI.
2. Keep implementation aligned with it where still valid.
3. If the style system intentionally changes, update `DESIGN.md` in the same
   task.
4. In the final report, mention whether `DESIGN.md` was read and whether it
   changed.

`DESIGN.md` is documentation, not generated bindings. Agents may edit it when
the visual source of truth changes.
