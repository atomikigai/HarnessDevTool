---
name: frontend-design
description: Create distinctive, production-grade frontend interfaces with high visual quality. Use when an agent is asked to build or beautify web components, pages, dashboards, SvelteKit routes, HTML/CSS layouts, applications, landing pages, posters, visual artifacts, or any UI that needs a polished aesthetic direction rather than generic AI-looking design.
metadata:
  short-description: Distinctive production-grade frontend visual design
  upstream-inspiration: https://github.com/anthropics/skills/blob/main/skills/frontend-design/SKILL.md
capabilities:
  kind: skill
  requires:
    - skill:agent-browser
    - skill:design-md
  suggests:
    - skill:shadcn-svelte
    - cli:agent-browser
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
      - frontend design
      - visual design
      - polish ui
      - beautify
      - make it look better
      - landing page
      - dashboard
      - visual hierarchy
      - distinctive ui
---

# Frontend Design

Use this skill to create frontend interfaces that feel intentionally designed,
production-grade, and specific to the product context. Avoid generic AI-looking
UI: predictable card grids, default gradients, bland typography, and decorative
effects that do not support the user goal.

## Choose The Design Mode

First decide which mode the task needs:

- **Operational app mode**: devtools, dashboards, CRM, admin, DB/SSH/task
  workflows, session panels, repeated-use product screens. Optimize for density,
  scanning, clarity, stable controls, and quiet polish.
- **Expressive visual mode**: websites, landing pages, posters, portfolio
  pieces, demos, hero sections, marketing pages, playful artifacts, or user
  requests that explicitly ask for visual impact. Commit to a bold aesthetic
  direction.

HarnessDevTool product screens usually use operational app mode. Do not turn
operational workflows into marketing pages.

## Design Thinking

Before coding, identify:

1. Purpose: what problem does the interface solve?
2. Audience: who uses it, and how often?
3. Tone: operational, brutalist, editorial, playful, industrial, refined,
   retro-futuristic, organic, geometric, minimal, maximal, or another clear
   direction.
4. Constraints: framework, accessibility, performance, responsiveness, existing
   component patterns, and repo ownership.
5. Differentiation: what is the one thing someone should remember?

Choose a clear conceptual direction and execute it precisely. Bold maximalism
and refined minimalism both work; intentionality matters more than intensity.

## Visual Aesthetics

Typography:

- Use local product typography/tokens for Harness operational screens.
- For expressive pages, choose distinctive, characterful type pairings instead
  of default Arial/Roboto/Inter/system stacks unless existing brand constraints
  require them.
- Match type scale to context. Hero-size type belongs in heroes, not compact
  sidebars or tool panels.

Color and theme:

- Use CSS variables and existing tokens first in app surfaces.
- Commit to a cohesive palette with meaningful accents.
- Avoid generic purple gradients, beige/tan sameness, and one-note hue families.
- Use color to encode state, focus, hierarchy, and brand mood, not filler.

Spatial composition:

- Operational screens: clear panes, tables, tabs, toolbars, dense but breathable
  lists, predictable alignment, stable control sizes.
- Expressive screens: asymmetry, overlap, diagonal flow, editorial grids,
  generous negative space, or controlled density when it fits the concept.

Motion:

- Operational screens: subtle transitions for focus, selection, expansion,
  loading, and drag/drop.
- Expressive screens: one or two high-impact moments such as staggered reveals,
  scroll-triggered transitions, or memorable hover states.

Details:

- Add texture, depth, borders, shadows, custom backgrounds, or unusual layout
  only when they match the concept and do not harm usability.
- Avoid decorative orbs, generic bokeh, and stock-feeling gradient blobs.

## Harness UI Rules

For HarnessDevTool screens:

- Keep the first screen usable as the actual app, not a landing page.
- Preserve task/session/thread state, errors, loading, empty states, and
  capability labels.
- Do not put cards inside cards.
- Use cards only for repeated items, modals, or genuinely framed tools.
- Prefer full-width sections, split panes, tabs, compact panels, toolbars, and
  dense lists.
- Use icons for common actions when available.
- Keep labels and text fitting inside buttons, tabs, cards, and sidebars.
- Do not edit generated API types; regenerate from Rust when needed.

## Implementation Workflow

1. Read `frontend/DESIGN.md` or repo-root `DESIGN.md` when present.
2. Read nearby components and CSS before inventing patterns.
3. Pick operational app mode or expressive visual mode.
4. Define the visual hierarchy before changing details.
5. Implement working code, not just a static mock.
6. Add responsive constraints with grid/flex, min/max widths, stable heights,
   aspect ratios, and overflow behavior.
7. Update `DESIGN.md` when the task intentionally changes visual tokens,
   component styling, layout rules, or design direction.
8. Validate with `agent-browser`: load `core --full`, inspect the rendered page,
   and capture evidence when layout or interactions matter.
9. Use Playwright only for stable regressions after the UI behavior is known.

## Review Checklist

- The UI has a clear point of view.
- It fits the product/domain instead of applying a generic aesthetic.
- The main workflow is visible and usable immediately.
- Typography, spacing, color, motion, and composition support hierarchy.
- Text does not clip or overlap across mobile and desktop.
- Interactive controls have hover/focus/disabled states.
- Browser validation was run or explicitly skipped with a reason.
