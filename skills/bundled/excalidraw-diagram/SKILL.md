---
name: excalidraw-diagram
description: Create practical Excalidraw diagrams that make a visual argument. Use when an agent needs to diagram architecture, workflows, protocols, task flows, state machines, agent orchestration, backend/frontend contracts, data pipelines, or technical concepts as an editable Excalidraw artifact with meaningful layout, evidence snippets, and visual validation.
metadata:
  short-description: Excalidraw diagrams that argue visually, not just boxes and arrows
  upstream-inspiration: https://github.com/coleam00/excalidraw-diagram-skill
capabilities:
  kind: skill
  requires:
    - skill:excalidraw-board
  suggests:
    - mcp:excalidraw
    - skill:agent-browser
    - skill:design-md
  trigger:
    paths:
      - docs/**
      - "**/*.excalidraw"
    keywords:
      - excalidraw diagram
      - architecture diagram
      - workflow diagram
      - sequence diagram
      - visual explanation
      - state machine
      - protocol diagram
      - system map
---

# Excalidraw Diagram

Use this skill to create editable Excalidraw diagrams that teach. A good diagram
is a visual argument: the shape, grouping, flow, and evidence communicate the
system even before labels are read.

This complements `excalidraw-board`, which covers the MCP/runtime mechanics.
Use this skill for diagram design methodology.

## Core Tests

- **Isomorphism test**: if labels disappeared, would the structure still convey
  the concept?
- **Education test**: does the diagram teach something concrete, or merely label
  boxes?
- **Evidence test**: for technical diagrams, does it show real names, payloads,
  code snippets, states, or UI examples instead of placeholders?

## Depth Decision

Choose the depth before drawing:

- **Simple/conceptual**: mental models, quick overviews, philosophy, rough
  planning. Use abstract shapes and minimal labels.
- **Comprehensive/technical**: real systems, protocols, tutorials, architecture,
  handoff docs. Use concrete examples and evidence artifacts.

For comprehensive diagrams, research actual APIs, events, payloads, file names,
tools, commands, or state transitions before drawing.

## Visual Patterns

Map behavior to shape:

- One source spawning many outputs: fan-out.
- Many inputs producing one result: convergence/funnel.
- Sequence or lifecycle: timeline.
- Feedback or iteration: cycle/loop.
- Nested ownership: tree or containment.
- Transformation: before -> process -> after.
- Comparison: side-by-side contrast.
- Separate phases or trust boundaries: bands, gaps, or zones.

Avoid uniform card grids unless the system is genuinely a set of peer items.

## Evidence Artifacts

Use concrete artifacts inside technical diagrams:

- Code snippets for APIs, handlers, commands, or integration points.
- JSON examples for payloads, events, schemas, and tool calls.
- Timeline dots for lifecycle events.
- UI mockups for user-visible results.
- Real names for endpoints, tools, crates, components, and files.

Evidence should be short, readable, and true to the source.

## Large Diagram Workflow

Build large diagrams section by section:

1. Start with a compact overview: target 20-30 elements for the first pass.
2. Add one section per pass.
3. Use stable descriptive IDs.
4. Keep each section visually distinct.
5. Connect sections after both sides exist.
6. Validate spacing, text fit, and arrow routing.

Do not generate a comprehensive diagram as one giant unreviewed blob. If the
diagram needs more detail, deliver the overview first and ask whether to expand
specific sections.

## Harness Diagram Defaults

For HarnessDevTool architecture diagrams, include:

- User/browser surface.
- `harness-server`.
- `harness-mcp-server`.
- PTY child agents.
- `HARNESS_HOME` storage.
- Protocol boundaries: HTTP/SSE, PTY, stdio MCP.
- Append-only event flow when relevant.
- Capability/skill/MCP relationships when relevant.

Use warm operational colors from `frontend/DESIGN.md` where appropriate. Keep
diagrams readable in exported screenshots and editable in Excalidraw.

## Validation

Before delivering:

- Check for overlapping text and arrows.
- Ensure text is large enough to read.
- Ensure the eye has a clear path through the diagram.
- Confirm technical names and examples match the repo/docs.
- Prefer visual validation through the Excalidraw MCP/board workflow when
  available.
