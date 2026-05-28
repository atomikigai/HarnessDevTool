# Q7 Spike — `claude --mcp-config` validation

**Date:** 2026-05-26
**Claude version:** `2.1.150 (Claude Code)` (binary: `/home/jostick/.local/bin/claude`)
**Status:** PASS for `claude`. SKIP / DEFER for `codex` (no equivalent flag).

## What was tested

A hand-written minimal MCP stdio server (`/tmp/hello-mcp.js`) implementing the
`2024-11-05` protocol with three handlers (`initialize`, `tools/list`,
`tools/call`) and one dummy tool `hello_world`.

## Config file format (LOCKED)

```json
{
  "mcpServers": {
    "harness-hello": {
      "command": "node",
      "args": ["/tmp/hello-mcp.js"]
    }
  }
}
```

`mcpServers.<name>` is mandatory; `command` + `args` follow standard MCP stdio
launch semantics. We will use the server name `harness` (singular) for our real
server, which makes tools appear in claude as `mcp__harness__task_list`,
`mcp__harness__task_get`, etc.

> Note on tool naming: claude exposes MCP tools to its model as
> `mcp__<server-name>__<tool-name>`. The MCP spec allows arbitrary tool names,
> but claude requires them to be `[a-zA-Z0-9_-]+`. We therefore use `task_list`
> rather than `task.list` on the wire; the brief's `task.list` style is the
> conceptual name.

## Commands that work

Listing tools (no permission needed for list):
```bash
claude --mcp-config /tmp/mcp-test.json --strict-mcp-config \
  --print "List your available MCP tools"
# → "Available MCP tools:\n- `mcp__harness-hello__hello_world` — Returns a greeting"
```

Actually calling a tool (must allowlist it):
```bash
claude --mcp-config /tmp/mcp-test.json --strict-mcp-config \
  --allowedTools "mcp__harness-hello__hello_world" \
  --print "Call hello_world with name=spike and show me the result verbatim"
# → "Hello, spike!"
```

Without `--strict-mcp-config`, claude merges our config with any user-level
`.mcp.json`. We pass `--strict-mcp-config` from `harness-session` so spawned
agents only see Harness tools.

## Handshake observed

Stderr trace from the hello server:
```
[hello-mcp] recv: initialize id= 0
[hello-mcp] recv: notifications/initialized id= undefined
[hello-mcp] recv: tools/list id= 1
[hello-mcp] recv: tools/call id= N
```

So the real server must respond to:
- `initialize` → reply with `protocolVersion`, `capabilities.tools = {}`,
  `serverInfo`.
- `notifications/initialized` → notification, no reply.
- `tools/list` → reply with `{ tools: [...] }`.
- `tools/call` → reply with `{ content: [{ type: "text", text: "..." }] }` or an
  error.

## codex status

`codex` ships an MCP **server** mode (`codex mcp-server`) and persistent MCP
**client** config in `~/.codex/config.toml`, but **no equivalent of
`--mcp-config <file>`** to inject a per-invocation server set. `codex --help`
exposes only:
- `codex mcp`            — manage external MCP servers (persistent config)
- `codex mcp-server`     — run codex itself as an MCP server (the opposite direction)
- `-c key=value`         — config overrides

Update: Codex MCP injection is now wired without mutating global config. The
harness passes per-invocation overrides:

- `-c mcp_servers.harness.command="..."`
- `-c mcp_servers.harness.args=[...]`

This keeps each session pointed at its own `harness-mcp-server` instance while
leaving `$CODEX_HOME/config.toml` untouched.

## Decisions locked in

1. `claude` invocation: append `--mcp-config <path> --strict-mcp-config`.
2. Per-session config file at `<HARNESS_HOME>/.runtime/mcp-configs/<sid>.json`.
3. Server name on the wire: `harness`.
4. Tool names use underscores not dots (claude restriction); JSON shape per tool
   matches the brief's contract.
5. `codex` receives equivalent MCP wiring through `-c mcp_servers.harness.*`
   overrides.
