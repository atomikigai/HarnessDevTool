---
name: HarnessDevTool Frontend
version: 1
mode: operational-app
source:
  css: frontend/src/app.css
  component_config: frontend/components.json
  shell:
    - frontend/src/routes/+layout.svelte
    - frontend/src/routes/+page.svelte
tokens:
  light:
    surface_window: "#faf8f2"
    surface_panel: "#f5f2ea"
    surface_canvas: "#ffffff"
    surface_rail: "#ede9e0"
    accent: "#0e7864"
    text_default: "#2e2a22"
    text_muted: "#8a8278"
    border_subtle: "#e2ddd4"
  dark:
    surface_window: "#1c1915"
    surface_canvas: "#17140f"
    surface_rail: "#141210"
    accent: "#e8a87c"
    text_default: "#d6c8b8"
    text_muted: "#7a6d5f"
    border_subtle: "#2e2620"
typography:
  sans: "Inter, ui-sans-serif, system-ui, -apple-system, Segoe UI, sans-serif"
  serif: "Fraunces, Source Serif Pro, Georgia, Times New Roman, serif"
  mono: "JetBrains Mono, Fira Code, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
radius:
  base: "0.5rem"
---

# DESIGN.md

## Design Intent

HarnessDevTool is an operational coding-agent workbench. The interface should
feel focused, calm, technical, and user-friendly for repeated daily use. The
visual system favors dense but readable panes, clear state, compact controls,
and warm tactile surfaces over decorative hero layouts.

The product is not a marketing site. Do not replace application screens with
landing-page composition unless the task explicitly asks for a public website or
visual artifact.

## Modes

- **Operational app mode** is the default. Use rails, panes, tabs, toolbars,
  status badges, compact lists, terminal surfaces, and clear hierarchy.
- **Expressive visual mode** is allowed only for isolated artifacts, public
  pages, demos, or explicit visual exploration requests. It must not degrade
  operational density or task clarity.

## Color Roles

The frontend uses CSS custom properties in `frontend/src/app.css`.

Light mode is a warm paper theme:

- Window: `#faf8f2`
- Panel: `#f5f2ea`
- Canvas: `#ffffff`
- Rail/title/status surfaces: warm grays around `#ede9e0`
- Text: dark warm brown `#2e2a22`
- Muted text: `#8a8278`
- Accent: deep teal `#0e7864`
- Borders: subtle warm gray `#e2ddd4`

Dark mode is a warm gruvbox-like theme:

- Window/panel: `#1c1915`
- Canvas: `#17140f`
- Rail/status: `#141210`
- Text: warm beige `#d6c8b8`
- Muted text: `#7a6d5f`
- Accent: amber-peach `#e8a87c`
- Borders: dark warm brown `#2e2620`

Status colors:

- Success: green (`#2d9d5b` light, `#a3be8c` dark)
- Warning: ochre/amber (`#c08030` light, `#e8c460` dark)
- Danger: red (`#cc4444` light, `#e07070` dark)

Use accent color for active state, primary action, focus, and selected context.
Do not create a new dominant hue family without updating this file.

## Typography

Use the tokenized font stacks from `app.css`:

- Sans for body, controls, operational labels, terminal-adjacent UI.
- Serif for light-mode `h1`/`h2` headings when a softer editorial touch helps.
- Mono for terminal output, IDs, metrics, protocol labels, keyboard hints, and
  compact diagnostic text.

Do not scale fonts with viewport width. Keep letter spacing at normal values
except for small uppercase eyebrow labels.

## Spacing, Radius, and Density

Default radius is `0.5rem`. Cards and controls should stay compact; avoid large
rounded decorative containers.

Operational density is intentional:

- Preserve visible session/task context.
- Keep repeated lists scannable.
- Use stable dimensions for tabs, controls, counters, badges, and panels.
- Avoid layout shifts from dynamic labels or hover states.

## Surfaces and Layout

The main shell is:

1. Top bar.
2. Narrow icon rail.
3. Main content canvas.
4. Route-specific columns/panes.

Use full-height panes and constrained overflow instead of page-length marketing
sections. For the root agents view, the expected rhythm is sessions column,
main session/terminal view, and right panel for tasks/agents/info.

Use cards only for repeated items, dialogs, and genuinely framed tools. Do not
nest cards inside cards.

## Components and Controls

Buttons, tabs, badges, status dots, and toolbar controls should be compact and
predictable. Use icons for common actions when available. Pair icons with text
when the command would otherwise be ambiguous.

Data-heavy surfaces must prioritize:

- Legible values.
- Clear selected/active state.
- Empty/loading/error states.
- Timestamps and metadata that are muted but still readable.
- Stable scroll containers.

## States

Every meaningful workflow should expose:

- Loading state.
- Empty state.
- Error state.
- Disabled state.
- Selected/current state.
- Running/blocked/success/failure state where applicable.

Do not hide operational uncertainty. If backend data is unavailable, show a
clear recoverable state instead of pretending the screen is complete.

## Motion

Motion should be subtle and functional: focus, hover, selection, expansion,
loading, and drag/drop. Avoid decorative motion in operational screens.

## Do / Do Not

Do:

- Preserve warm paper/light and warm dark token systems.
- Keep task/session state visible.
- Use panes, rails, tables, lists, tabs, toolbars, terminal surfaces.
- Validate visual changes with `agent-browser` as a user.
- Update this file when styles or visual rules intentionally change.

Do not:

- Use generic purple-blue gradient AI aesthetics.
- Add decorative orbs, bokeh blobs, or stock-looking hero art to app screens.
- Turn dashboards or tools into landing pages.
- Introduce a new palette, radius scale, or font system without updating
  `frontend/DESIGN.md`.
- Edit generated API types by hand.

## QA Expectations

Any frontend change requires real-user validation with `agent-browser`.

QA must check:

- The worked flow completes from the UI.
- Data is legible.
- The screen is user friendly for the intended workflow.
- States are visible and understandable.
- No text clips, overlaps, or becomes unreadable on mobile/desktop.
- Console/network issues are noted when relevant.

If backend/frontend contracts changed, run both services and validate through
the UI, not just API calls.

## Maintenance

Read this file before UI design work. If a frontend task changes the visual
system, component styling, global CSS tokens, or UX quality rules, update this
file in the same task.

This file is expected to evolve. The frontend, QA, or design-focused agent may
improve it when style changes make it stale.

## shadcn-svelte

HarnessDevTool uses shadcn-svelte-style local components under
`frontend/src/lib/components/ui/`, backed by Svelte 5, Tailwind v4, Bits UI,
`tailwind-variants`, `tailwind-merge`, `mode-watcher`, and `svelte-sonner`.

When adding or changing these components:

- Keep local token mapping in `frontend/src/app.css`.
- Preserve accessibility behavior from Bits UI primitives.
- Prefer existing component folder/export conventions.
- Do not import a generic theme that overwrites this design system.
- Validate in both light and dark mode when the component is visual or
  interaction-heavy.
