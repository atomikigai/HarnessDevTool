---
id: build-plan/task31-ab-experiment-2026-06-08
title: Task 31 A/B Experiment Run 2026-06-08
status: completed
---

# Task 31 A/B Experiment Run 2026-06-08

## Question

Compare session efficiency by capability set loaded at spawn time:

- `none`
- `harness`
- `harness+crawl4ai`

Primary metrics:

- prompt tokens
- output tokens
- tool call count
- tool call breakdown by tool name
- cost USD

## Initial Local Run

Command:

```bash
python3 scripts/analyze-session-metrics.py --profile default
```

Result:

| Metric | Value |
|---|---:|
| Sessions scanned | 5 |
| Eligible instrumented sessions | 0 |
| Groups observed | `none` |
| Sessions with normalized transcript | 0 |
| Minimum viable A/B ready | false |

Observed sessions were all pre-instrumentation Codex sessions. They have no
normalized transcript and no recorded `loaded_capabilities`, so they cannot be
used to compare capability profiles.

## Conclusion

No valid A/B conclusion can be drawn from the current local Harness data. The
instrumentation is in place, but the sample predates it.

## Controlled Sample Run

Isolated backend:

```bash
HARNESS_HOME=/tmp/harness-ab-home-real \
HARNESS_BIND=127.0.0.1:43177 \
HARNESS_CORS_ORIGIN=http://localhost:43178 \
cargo run -p harness-server
```

Sample launcher:

```bash
python3 scripts/run-session-ab-sample.py \
  --base-url http://127.0.0.1:43177 \
  --cwd /tmp/harness-ab-work \
  --kind claude
```

Sample shape:

| Task type | Profiles |
|---|---|
| `plan` | `none`, `harness`, `harness_crawl4ai` |
| `refactor` | `none`, `harness`, `harness_crawl4ai` |
| `code-write` | `none`, `harness`, `harness_crawl4ai` |

All 9 sessions reached `AB_SAMPLE_DONE` and produced normalized transcript
events.

### Aggregate Results

| Profile | Sessions | Avg prompt tokens | Avg output tokens | Avg tool calls | Avg cost USD | Done |
|---|---:|---:|---:|---:|---:|---:|
| `none` | 3 | 10.7 | 648.0 | 2.00 | 0.17358 | 3 |
| `harness` | 3 | 11.3 | 834.7 | 2.33 | 0.18337 | 3 |
| `harness_crawl4ai` | 3 | 11.3 | 755.3 | 2.33 | 0.18629 | 3 |

Tool call totals:

| Profile | Breakdown |
|---|---|
| `none` | `Bash=3`, `Read=3` |
| `harness` | `Bash=3`, `Read=3`, `Edit=1` |
| `harness_crawl4ai` | `Bash=3`, `Read=4` |

No Harness MCP task/repo tools and no Crawl4AI tools were called in this
sample. Work was handled entirely with built-in file/shell tools.

### Bash Command Breakdown

The analysis was extended to inspect the command strings inside `Bash` tool
calls, so we can tell whether agents used shell alternatives such as `rg`,
`fd`, `ast-grep` or `difftastic`.

| Profile | Avg Bash commands | Breakdown |
|---|---:|---|
| `none` | 1.00 | `find=3` |
| `harness` | 2.33 | `find=3`, `head=2`, `ls=1`, `echo=1` |
| `harness_crawl4ai` | 1.33 | `find=3`, `sort=1` |

No `rg`, `fd`, `ast-grep`, `difftastic` or other efficient-cli tools appeared
in this sample. The agents consistently chose `find` for file discovery.

Command categories:

| Profile | POSIX search | POSIX inspect | Efficient search | Semantic diff |
|---|---:|---:|---:|---:|
| `none` | 3 | 0 | 0 | 0 |
| `harness` | 3 | 3 | 0 | 0 |
| `harness_crawl4ai` | 3 | 1 | 0 | 0 |

Automatic findings from the strengthened analyzer:

- No efficient search or semantic diff CLIs were used; agents relied on POSIX
  search commands.
- Harness/Crawl4AI capability groups did not call MCP tools in this sample.
- Capability-enabled groups had higher average tool or shell-command counts
  than the control.

### Interpretation

For small local repo tasks, forcing Harness MCP or Crawl4AI increased average
cost and tool calls without improving completion rate. The cheapest profile was
`none`, with all 3 runs completed.

This does not mean Harness MCP should be removed globally: orchestrated sessions
need task/spec/session tools for append-only coordination. It does mean
user-started simple local sessions should prefer the lightest capability set
unless the task asks for Harness task coordination or external documentation.

The Bash command breakdown adds a second conclusion: capability profiles do not
by themselves steer agents toward the repo's preferred efficient CLI tools. If
we want consistent use of `rg`/`fd` for local search, we need either stronger
agent instructions or deterministic Harness rails for common repo scans.

### Complete Readout

What we can determine from this run:

| Signal | Determination |
|---|---|
| Completion | All profiles completed all 3 sessions, so there is no quality win visible from extra capabilities in this small sample. |
| Cost | `none` was cheapest on average: `$0.17358` vs `$0.18337` for `harness` and `$0.18629` for `harness_crawl4ai`. |
| Tool overhead | `none` averaged `2.00` tool calls; both capability profiles averaged `2.33`. |
| Shell overhead | `none` averaged `1.00` shell command; `harness` averaged `2.33`; `harness_crawl4ai` averaged `1.33`. |
| MCP utilization | No Harness MCP or Crawl4AI tools were called, so their loaded capability cost was unused in this sample. |
| Search behavior | All profiles used POSIX `find`; no profile selected `rg`, `fd`, `ast-grep` or `difftastic`. |
| Current confidence | Directional only. The sample is intentionally small and the toy repo does not stress search, indexing or external-doc workflows. |

Decision implications:

| Area | Decision |
|---|---|
| Simple local sessions | Prefer `none` or a lightweight profile when task/spec coordination is not required. |
| Orchestrated sessions | Keep Harness MCP enabled because scheduler/agent workflows need append-only task, spec, mailbox and session operations. |
| External docs/web tasks | Keep Crawl4AI heuristic or explicit; do not preload it for local-only code work. |
| Efficient CLI adoption | Capability loading is insufficient. Add explicit instructions and/or a deterministic repo-search rail. |
| Next experiment | Use a larger, search-heavy repo workload so `rg`/`fd` behavior can be measured directly. |

### Recommended Adjustments

1. Keep `capability_profile=auto` as default.
2. Keep Crawl4AI strictly heuristic/explicit; do not preload it for local-only
   code tasks.
3. For ad-hoc user sessions without task/spec coordination, consider defaulting
   to `none` or exposing the `none` profile in the UI as the lightweight mode.
4. Keep Harness MCP enabled for scheduler/orchestrator child sessions because
   those agents need append-only task, spec, mailbox and session operations.
5. Convert frequently repeated deterministic repo reads to Rust rails only if a
   larger sample shows repeated `Bash`/`Read` patterns beyond this toy project.
6. Add a follow-up search-heavy sample that explicitly measures `find` vs
   `rg`/`fd` behavior across a larger repo tree.
7. Consider a lightweight `repo_scan`/`repo_find` rail if repeated local search
   remains a common `Bash` pattern.
8. Add explicit agent guidance that says to prefer `rg` for text search and
   `fd` for file discovery when those binaries are available.
9. Track `efficient_cli_command_rate` as a first-class metric in future A/B
   runs.

## Next Run Criteria

Run at least two sessions per task type and capability profile:

| Task type | Capability profiles |
|---|---|
| `plan` | `none`, `harness`, `harness+crawl4ai` |
| `code-write` | `none`, `harness`, `harness+crawl4ai` |
| `refactor` | `none`, `harness`, `harness+crawl4ai` |

Minimum viable sample:

- 18 sessions total.
- Every session created after `loaded_capabilities` instrumentation.
- Claude sessions preferred until Codex transcript/cost reporting is wired.
- Each run must mark success/failure manually until evaluator pass/fail is
  folded into the metrics report.

## Follow-up Completed

Explicit spawn-time capability profiles were added after this inconclusive
run, so the next A/B can create controlled groups instead of relying on
heuristic `crawl4ai` loading.

Profiles:

| `capability_profile` | Behavior |
|---|---|
| `auto` | Existing behavior: Harness MCP when available, Crawl4AI only by heuristic. |
| `none` | Skip Harness MCP injection; records `agent_builtin` as the control group. |
| `harness` | Force Harness MCP only. |
| `harness_crawl4ai` | Force Harness MCP plus Crawl4AI. |

Example:

```bash
curl -sS -X POST "$HARNESS_URL/api/threads/$THREAD/sessions" \
  -H 'Content-Type: application/json' \
  -H "X-Protocol-Version: $HARNESS_PROTOCOL_VERSION" \
  -d '{
    "kind": "claude",
    "cwd": "/path/to/repo",
    "capability_profile": "harness"
  }'
```

## Remaining Follow-up

Run the same matrix against a real feature/refactor workload with at least two
runs per cell. This first controlled sample is intentionally small and should be
treated as directional, not statistically conclusive.

## Forced Efficient CLI Run

Follow-up question: does explicitly forcing efficient CLI usage improve
measured session efficiency?

Prompt change:

```text
You must use fd for file discovery and rg for text search when available.
Do not use find or grep unless fd/rg are unavailable.
```

Local availability before the run:

| Tool | Available |
|---|---|
| `rg` | yes |
| `fd` | yes |
| `sg` | yes |
| `ast-grep` | no |
| `difftastic` | no |

The same 3x3 matrix was run in an isolated Harness home against a fresh copy of
the toy repo:

```bash
HARNESS_HOME=/tmp/harness-ab-home-efficient \
HARNESS_BIND=127.0.0.1:43211 \
HARNESS_CORS_ORIGIN=http://localhost:43212 \
cargo run -p harness-server

python3 scripts/run-session-ab-sample.py \
  --base-url http://127.0.0.1:43211 \
  --cwd /tmp/harness-ab-work-efficient \
  --kind claude \
  --force-efficient-cli
```

All 9 sessions reached `AB_SAMPLE_DONE`.

### Forced Run Results

| Profile | Avg cost USD | Avg output tokens | Avg tool calls | Avg Bash commands | Efficient CLI rate | Commands |
|---|---:|---:|---:|---:|---:|---|
| `none` | 0.16784 | 981.0 | 2.33 | 1.00 | 1.00 | `fd=3` |
| `harness` | 0.18530 | 860.3 | 2.67 | 1.00 | 1.00 | `fd=3` |
| `harness_crawl4ai` | 0.19285 | 795.3 | 3.33 | 1.67 | 1.00 | `fd=3`, `rg=2` |

Comparison to the non-forced baseline:

| Profile | Base cost | Forced cost | Delta | Base tools | Forced tools | Base efficient rate | Forced efficient rate |
|---|---:|---:|---:|---:|---:|---:|---:|
| `none` | 0.17358 | 0.16784 | -0.00573 | 2.00 | 2.33 | 0.00 | 1.00 |
| `harness` | 0.18337 | 0.18530 | +0.00193 | 2.33 | 2.67 | 0.00 | 1.00 |
| `harness_crawl4ai` | 0.18629 | 0.19285 | +0.00656 | 2.33 | 3.33 | 0.00 | 1.00 |

### Forced Run Interpretation

Forcing the instruction successfully changed agent behavior:

- `find`/`grep` usage dropped to zero.
- `fd` was used in every session.
- `rg` appeared where the agent chose text search.
- `efficient_cli_command_rate` rose from `0.00` to `1.00` in all profiles.

It did not produce a broad efficiency win in this sample:

- `none` got slightly cheaper by about `$0.00573` per session, but tool calls
  increased from `2.00` to `2.33`.
- `harness` got slightly more expensive and increased tool calls.
- `harness_crawl4ai` got more expensive and had the largest tool-call increase.

The likely reason is workload size: the toy repo has one target source file, so
`fd`/`rg` do not have enough search surface to offset the extra instruction and
agent reasoning. The benefit of `rg`/`fd` should be tested on a larger repo with
multi-file search tasks, where POSIX `find`/`grep` would be meaningfully slower
or noisier.

Decision: keep the efficient CLI instruction as a useful behavior rail, but do
not count it as a measured cost optimization yet. The measured optimization
still comes from capability selection: use `none`/lightweight profiles for
simple local sessions and reserve Harness/Crawl4AI capabilities for workflows
that actually need them.

## Heavy Repo Efficient CLI Run

Follow-up target:

```text
/home/jostick/Desktop/Personal/Projects/workspaces/aventi-workspace/
```

This run used a heavier real repository and a read-only `repo-search` matrix:

| Task type | Prompt intent |
|---|---|
| `repo-map` | Identify stacks, entrypoints, manifests and build/test commands. |
| `config-search` | Find environment/config loading, routes and DB settings without reading `.env` values. |
| `domain-trace` | Trace user/account/session/auth-related code paths. |

Two isolated Harness homes were used:

- baseline: `/tmp/harness-ab-home-aventi-base`
- forced efficient CLI: `/tmp/harness-ab-home-aventi-forced`

Commands:

```bash
python3 scripts/run-session-ab-sample.py \
  --base-url http://127.0.0.1:43221 \
  --cwd /home/jostick/Desktop/Personal/Projects/workspaces/aventi-workspace \
  --kind claude \
  --task-set repo-search

python3 scripts/run-session-ab-sample.py \
  --base-url http://127.0.0.1:43231 \
  --cwd /home/jostick/Desktop/Personal/Projects/workspaces/aventi-workspace \
  --kind claude \
  --task-set repo-search \
  --force-efficient-cli
```

All 18 sessions reached `AB_SAMPLE_DONE`. The target repo remained clean after
both runs.

### Heavy Repo Results

By capability profile:

| Profile | Base cost | Forced cost | Delta | Base tools | Forced tools | Base Bash cmds | Forced Bash cmds | Base efficient rate | Forced efficient rate |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `none` | 0.33215 | 0.33003 | -0.00211 | 6.00 | 7.33 | 16.67 | 20.67 | 0.00 | 0.27 |
| `harness` | 0.42088 | 0.48817 | +0.06729 | 9.67 | 13.33 | 21.33 | 29.00 | 0.00 | 0.36 |
| `harness_crawl4ai` | 0.42641 | 0.38146 | -0.04496 | 7.67 | 8.67 | 16.33 | 21.00 | 0.00 | 0.37 |

Baseline included one `none/domain-trace` session that reached the done marker
without tool calls. Active-only view:

| Profile | Base active n | Forced active n | Base active cost | Forced active cost | Delta | Base active tools | Forced active tools |
|---|---:|---:|---:|---:|---:|---:|---:|
| `none` | 2 | 3 | 0.42461 | 0.33003 | -0.09457 | 9.00 | 7.33 |
| `harness` | 3 | 3 | 0.42088 | 0.48817 | +0.06729 | 9.67 | 13.33 |
| `harness_crawl4ai` | 3 | 3 | 0.42641 | 0.38146 | -0.04496 | 7.67 | 8.67 |

By task type:

| Task | Base cost | Forced cost | Delta | Base tools | Forced tools |
|---|---:|---:|---:|---:|---:|
| `repo-map` | 0.35212 | 0.31070 | -0.04141 | 5.67 | 7.00 |
| `config-search` | 0.48866 | 0.52114 | +0.03249 | 11.00 | 13.00 |
| `domain-trace` | 0.33866 | 0.36781 | +0.02915 | 6.67 | 9.33 |

Command behavior:

| Run | `none` | `harness` | `harness_crawl4ai` |
|---|---|---|---|
| Baseline | `find=1`, `grep=8`, no `fd`/`rg` | `find=11`, `grep=6`, no `fd`/`rg` | `find=6`, `grep=8`, no `fd`/`rg` |
| Forced | `fd=15`, `rg=2`, no `find`/`grep` | `fd=16`, `rg=15`, `grep=3` | `fd=15`, `rg=8`, no `find`/`grep` |

### Heavy Repo Interpretation

The forced instruction worked as a behavior rail in the larger repo:

- Efficient CLI usage moved from `0.00` to `0.27`-`0.37` of all shell command
  fragments.
- Baseline relied on `find`/`grep`; forced runs primarily used `fd`/`rg`.
- `harness` still leaked `grep=3`, so the instruction is strong but not
  absolute.

Efficiency did not improve universally:

- `repo-map` got cheaper with forced `fd/rg`.
- `config-search` and `domain-trace` got more expensive and used more tools.
- `none` improved on active-only comparison, but aggregate `none` is distorted
  by one baseline session that did no tool work.
- `harness` got clearly worse under forced efficient CLI in this run.
- `harness_crawl4ai` got cheaper in cost but still used more tool calls.

Determination: on a real heavier repo, forcing `fd`/`rg` improves search-tool
selection but does not automatically reduce agent cost. It can make agents
search more thoroughly, which increases shell/tool count. The practical win is
quality and consistency of repository discovery, not guaranteed token/cost
reduction.

Next decision: keep efficient CLI preference as a default instruction for
repository-analysis tasks, but measure pass/fail quality alongside cost before
calling it an optimization. For cost control, prefer a deterministic Harness
`repo_scan`/`repo_find` rail that performs bounded `fd`/`rg` searches and
returns compact structured results.

## Decisions Applied

Accepted on 2026-06-08:

| Decision | Status |
|---|---|
| Keep `capability_profile=auto` as the default. | Applied. |
| Expose `none` as a lightweight user-selectable profile. | Applied in `NewSessionDialog`. |
| Implement deterministic repository search rail. | Applied as MCP `repo_find`. |
| Add quality metric to the experiment analyzer. | Applied as `completion_marker_rate`, `active_tool_work_rate`, and `quality_pass_rate`. |

The quality metric is intentionally conservative and mechanical:

- `completion_marker_rate`: session emitted `AB_SAMPLE_DONE`.
- `active_tool_work_rate`: session made at least one tool call.
- `quality_pass_rate`: both conditions were true.

This catches no-op completions, including the `none/domain-trace` heavy-repo
baseline session that reached the marker without inspecting files. It is not a
semantic evaluator; the next quality step is an evaluator pass/fail rubric that
checks factual completeness and path evidence.
