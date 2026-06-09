---
name: shadcn-svelte
description: Use and extend shadcn-svelte components in HarnessDevTool. Use when an agent needs to add, modify, or compose Svelte/SvelteKit UI components using shadcn-svelte, Bits UI primitives, Tailwind CSS v4, components.json, accessible dialogs/forms/tabs/buttons/cards, or when it needs current shadcn-svelte docs from llms.txt.
metadata:
  short-description: shadcn-svelte components for SvelteKit, Tailwind v4, and Bits UI
  llms: https://www.shadcn-svelte.com/llms.txt
  docs: https://www.shadcn-svelte.com/docs
capabilities:
  kind: skill
  requires:
    - skill:design-md
    - skill:agent-browser
    - cli:pnpm
  suggests:
    - skill:frontend-design
    - skill:context7
    - mcp:playwright
  trigger:
    paths:
      - frontend/components.json
      - frontend/src/lib/components/ui/**
      - frontend/src/**/*.svelte
    keywords:
      - shadcn-svelte
      - shadcn
      - bits ui
      - components.json
      - dialog
      - tabs
      - button
      - form
      - card
      - accessible component
---

# shadcn-svelte

Use this skill when building or modifying UI with shadcn-svelte patterns.
HarnessDevTool already has `frontend/components.json`, Tailwind v4, Svelte 5,
Bits UI, `tailwind-variants`, `tailwind-merge`, `mode-watcher`, and several UI
components under `frontend/src/lib/components/ui/`.

## Official Docs

Use the official LLM index when current docs are needed:

```text
https://www.shadcn-svelte.com/llms.txt
```

The index points to CLI, `components.json`, theming, SvelteKit installation,
Tailwind v4 migration, Svelte 5 migration, registry docs, and component docs
for Button, Dialog, Tabs, Sidebar, Table, Data Table, Form, Sonner, Tooltip,
Sheet, Drawer, Resizable, and more.

## Harness Rules

1. Read `frontend/DESIGN.md` before changing component styling.
2. Prefer existing components in `frontend/src/lib/components/ui/` before
   adding new ones.
3. Keep shadcn-svelte components aligned with Harness tokens in
   `frontend/src/app.css`.
4. Do not paste a generic shadcn theme that overwrites the warm paper/dark
   token system.
5. Do not edit generated API types under `frontend/src/lib/api/types/`.
6. Validate UI changes with `agent-browser` as a user.

## Component Choice

Use familiar primitives:

- `Button` for clear commands.
- `Dialog`, `Alert Dialog`, `Sheet`, or `Drawer` for overlays.
- `Tabs` for layered task/session views.
- `Resizable` for split panels.
- `Input`, `Textarea`, `Label`, `Select`, `Switch`, `Checkbox`, `Slider` for
  forms and settings.
- `Badge`, `Alert`, `Empty`, `Progress`, `Skeleton`, `Spinner`, `Sonner` for
  feedback and state.
- `Table` or `Data Table` for structured data when the interaction needs real
  tabular scanning.

For operational Harness screens, compose compact controls and panes. Avoid
decorative card stacks and oversized marketing sections.

## Workflow

1. Inspect existing local component implementation and exports.
2. Check official docs through `llms.txt` when behavior/API is uncertain.
3. Add or adapt components through the repo's shadcn-svelte conventions.
4. Preserve accessibility semantics from Bits UI/shadcn-svelte primitives.
5. Keep styling token-driven and consistent with `frontend/DESIGN.md`.
6. Run:

```bash
cd frontend
pnpm check
```

7. Validate with `agent-browser` in the browser. If backend/frontend contracts
   are involved, run both services and test the flow from the UI.

## When Adding New Components

Prefer the shadcn-svelte CLI when the repo setup supports it. If adding
manually, keep the component local, typed, accessible, exported through
`index.ts`, and styled with existing token variables/classes.

After adding a reusable UI component:

- Confirm it follows the existing `frontend/src/lib/components/ui/<name>/`
  folder shape.
- Confirm imports use local aliases consistently.
- Confirm dark mode and light mode both work.
- Update `frontend/DESIGN.md` if the component introduces new visual rules.
