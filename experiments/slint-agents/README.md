# Harness Slint Agents Spike

Desktop-native Slint experiment for the Harness Agents tab.

This is intentionally outside the main backend Cargo workspace so Slint's GUI
dependencies do not affect server builds or the SvelteKit frontend.

## Run

Start `harness-server`, then run:

```bash
cargo run --manifest-path experiments/slint-agents/Cargo.toml -- \
  --base-url http://127.0.0.1:7777
```

The app polls:

```text
GET /api/threads
X-Protocol-Version: 1.0
```

It renders a desktop Agents module with all known sessions, grouped by the
thread metadata returned by the backend:

- session id
- role
- CLI kind
- status
- pid
- task id
- scopes
- parent/root relation
- detected state
- relative start time

To narrow the view to a root, parent or specific session id:

```bash
cargo run --manifest-path experiments/slint-agents/Cargo.toml -- \
  --base-url http://127.0.0.1:7777 \
  --session-id <session-id>
```

## Decision Gate

Keep this as an experiment until it proves a concrete advantage over the web UI
for the Agents surface:

- lower memory or startup time
- smoother rendering with many child sessions
- simpler native desktop packaging
- acceptable effort to reproduce terminal, tasks and metrics views

Do not migrate the SvelteKit Agents tab until the desktop app can cover the
main workflow end to end.
