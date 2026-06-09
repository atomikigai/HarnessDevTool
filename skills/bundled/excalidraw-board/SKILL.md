---
name: excalidraw-board
description: Create, edit, and reason about diagrams, architecture sketches, flows, wireframes, and planning boards using Excalidraw MCP.
metadata:
  short-description: Create and edit Excalidraw diagrams via MCP
  upstream: https://github.com/excalidraw/excalidraw-mcp
  mcp-server: excalidraw
  mcp-url: http://localhost:3001/mcp
capabilities:
  kind: skill
  requires:
    - mcp:excalidraw
  trigger:
    keywords:
      - diagram
      - architecture sketch
      - board
      - wireframe
      - flow map
      - editable diagram
---

# Excalidraw Board

Use this skill when an agent needs a visual board: architecture diagrams,
workflow maps, task decomposition, UI wireframes, sequence diagrams, or planning
artifacts that should be editable in Excalidraw.

## Runtime

The local MCP service is provided by `docker-compose.mcp.yml`.

- Service: `excalidraw-mcp`
- Streamable HTTP MCP endpoint: `http://localhost:3001/mcp`
- Default port: `3001`

## Workflow

1. Use Excalidraw MCP when the output should be an editable board, not just a
   markdown diagram.
2. Keep diagrams scannable:
   - fewer boxes with strong labels;
   - left-to-right or top-to-bottom flow;
   - clear runtime/trust boundaries;
   - arrows labeled with protocol or event names.
3. For architecture diagrams, include actors, services, queues/events, storage,
   MCP boundaries, and external systems.
4. For product flows, include screens/states, primary actions, empty/loading/error
   states, and handoff points.
5. Preserve existing board elements unless the task explicitly asks to replace
   them.

## Diagram Rules

- Use color to encode meaning, not decoration.
- Avoid tiny text and overlapping arrows.
- Prefer stable element names/ids when updating a board.
- For large diagrams, group sections by domain and keep a readable overview.
