#!/usr/bin/env python3
"""Summarise Harness session metrics for capability A/B experiments.

Reads persisted session metadata and normalized transcripts from HARNESS_HOME.
Outputs a JSON report suitable for attaching to Task 31 notes.
"""

from __future__ import annotations

import argparse
import json
import os
import shlex
from collections import Counter, defaultdict
from pathlib import Path
from statistics import mean
from typing import Any


EFFICIENT_SEARCH_TOOLS = {"rg", "fd", "ast-grep", "sg"}
SEMANTIC_DIFF_TOOLS = {"difft", "difftastic"}
POSIX_SEARCH_TOOLS = {"find", "grep", "egrep", "fgrep"}
POSIX_INSPECT_TOOLS = {"cat", "head", "tail", "ls", "pwd", "wc", "sort", "uniq"}
BUILD_TEST_TOOLS = {
    "cargo",
    "just",
    "make",
    "npm",
    "pnpm",
    "yarn",
    "node",
    "python",
    "python3",
}


def load_json(path: Path) -> dict[str, Any] | None:
    try:
        return json.loads(path.read_text())
    except Exception:
        return None


def group_for(meta: dict[str, Any]) -> str:
    caps = meta.get("loaded_capabilities") or {}
    parts: list[str] = []
    for key in ("mcp_servers", "skills", "tool_groups"):
        values = caps.get(key) or []
        parts.extend(str(v) for v in values if str(v).strip())
    if not parts:
        return "none"
    return "+".join(sorted(set(parts)))


def command_name(raw: str) -> str | None:
    raw = raw.strip()
    if not raw:
        return None
    try:
        parts = shlex.split(raw, comments=False, posix=True)
    except ValueError:
        parts = raw.split()
    if not parts:
        return None
    idx = 0
    while idx < len(parts) and "=" in parts[idx] and not parts[idx].startswith("-"):
        key = parts[idx].split("=", 1)[0]
        if not key.replace("_", "").isalnum():
            break
        idx += 1
    while idx < len(parts) and parts[idx] in {"env", "sudo", "command", "builtin", "time"}:
        idx += 1
        while idx < len(parts) and parts[idx].startswith("-"):
            idx += 1
    if idx >= len(parts):
        return None
    return Path(parts[idx]).name


def split_shell_commands(command: str) -> list[str]:
    segments: list[str] = []
    current: list[str] = []
    quote: str | None = None
    escaped = False
    idx = 0
    while idx < len(command):
        char = command[idx]
        if escaped:
            current.append(char)
            escaped = False
            idx += 1
            continue
        if char == "\\":
            current.append(char)
            escaped = True
            idx += 1
            continue
        if quote:
            current.append(char)
            if char == quote:
                quote = None
            idx += 1
            continue
        if char in {"'", '"'}:
            quote = char
            current.append(char)
            idx += 1
            continue
        if command.startswith("&&", idx) or command.startswith("||", idx):
            segment = "".join(current).strip()
            if segment:
                segments.append(segment)
            current = []
            idx += 2
            continue
        if char in {";", "|"}:
            segment = "".join(current).strip()
            if segment:
                segments.append(segment)
            current = []
            idx += 1
            continue
        current.append(char)
        idx += 1

    segment = "".join(current).strip()
    if segment:
        segments.append(segment)
    return segments


def bash_command_breakdown(command: str) -> Counter[str]:
    counts: Counter[str] = Counter()
    for segment in split_shell_commands(command):
        name = command_name(segment)
        if name:
            counts[name] += 1
    return counts


def command_category(name: str) -> str:
    if name in EFFICIENT_SEARCH_TOOLS:
        return "efficient_search"
    if name in SEMANTIC_DIFF_TOOLS:
        return "semantic_diff"
    if name in POSIX_SEARCH_TOOLS:
        return "posix_search"
    if name in POSIX_INSPECT_TOOLS:
        return "posix_inspect"
    if name in BUILD_TEST_TOOLS:
        return "build_test"
    return "other"


def command_categories(commands: Counter[str]) -> Counter[str]:
    counts: Counter[str] = Counter()
    for name, count in commands.items():
        counts[command_category(name)] += count
    return counts


def read_transcript_metrics(path: Path) -> tuple[int, Counter[str], Counter[str], bool]:
    calls: Counter[str] = Counter()
    bash_commands: Counter[str] = Counter()
    completion_marker_found = False
    events = 0
    if not path.exists():
        return events, calls, bash_commands, completion_marker_found
    for line in path.read_text(errors="replace").splitlines():
        if not line.strip():
            continue
        try:
            ev = json.loads(line)
        except json.JSONDecodeError:
            continue
        events += 1
        content = ev.get("content")
        if isinstance(content, str) and "AB_SAMPLE_DONE" in content:
            completion_marker_found = True
        if ev.get("kind") == "tool_call":
            tool_name = str(ev.get("tool_name") or "(unknown)")
            calls[tool_name] += 1
            if tool_name == "Bash":
                args = ev.get("tool_args") or {}
                command = args.get("command") if isinstance(args, dict) else None
                if isinstance(command, str):
                    bash_commands.update(bash_command_breakdown(command))
    return events, calls, bash_commands, completion_marker_found


def session_rows(home: Path, profile: str) -> list[dict[str, Any]]:
    sessions_dir = home / "profiles" / profile / "sessions"
    rows: list[dict[str, Any]] = []
    for meta_path in sorted(sessions_dir.glob("*/meta.json")):
        meta = load_json(meta_path)
        if not meta:
            continue
        session_dir = meta_path.parent
        events, calls, bash_commands, completion_marker_found = read_transcript_metrics(
            session_dir / "transcript.jsonl"
        )
        categories = command_categories(bash_commands)
        active_tool_work = sum(calls.values()) > 0
        rows.append(
            {
                "session_id": meta.get("id") or session_dir.name,
                "thread_id": meta.get("thread_id"),
                "kind": meta.get("kind"),
                "status": meta.get("status"),
                "task_id": meta.get("task_id"),
                "group": group_for(meta),
                "has_transcript": bool(meta.get("has_transcript")),
                "normalized_events": events,
                "completion_marker_found": completion_marker_found,
                "active_tool_work": active_tool_work,
                "quality_pass": completion_marker_found and active_tool_work,
                "tool_call_count": sum(calls.values()),
                "tool_call_breakdown": dict(sorted(calls.items())),
                "bash_command_count": sum(bash_commands.values()),
                "bash_command_breakdown": dict(sorted(bash_commands.items())),
                "bash_command_categories": dict(sorted(categories.items())),
                "efficient_cli_command_count": categories.get("efficient_search", 0)
                + categories.get("semantic_diff", 0),
                "posix_search_command_count": categories.get("posix_search", 0),
                "loaded_capabilities": meta.get("loaded_capabilities") or {
                    "mcp_servers": [],
                    "skills": [],
                    "tool_groups": [],
                },
            }
        )
    return rows


def summarize(rows: list[dict[str, Any]]) -> dict[str, Any]:
    groups: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for row in rows:
        groups[row["group"]].append(row)

    out: dict[str, Any] = {}
    for group, items in sorted(groups.items()):
        tool_counts: Counter[str] = Counter()
        bash_counts: Counter[str] = Counter()
        category_counts: Counter[str] = Counter()
        for item in items:
            tool_counts.update(item["tool_call_breakdown"])
            bash_counts.update(item["bash_command_breakdown"])
            category_counts.update(item["bash_command_categories"])
        efficient_cli_total = category_counts.get("efficient_search", 0) + category_counts.get(
            "semantic_diff", 0
        )
        out[group] = {
            "sessions": len(items),
            "sessions_with_transcript": sum(1 for item in items if item["normalized_events"] > 0),
            "completion_marker_rate": mean(
                [1 if item["completion_marker_found"] else 0 for item in items]
            )
            if items
            else 0,
            "active_tool_work_rate": mean([1 if item["active_tool_work"] else 0 for item in items])
            if items
            else 0,
            "quality_pass_rate": mean([1 if item["quality_pass"] else 0 for item in items])
            if items
            else 0,
            "avg_tool_calls": mean([item["tool_call_count"] for item in items]) if items else 0,
            "total_tool_calls": sum(item["tool_call_count"] for item in items),
            "tool_call_breakdown": dict(sorted(tool_counts.items())),
            "avg_bash_commands": mean([item["bash_command_count"] for item in items])
            if items
            else 0,
            "total_bash_commands": sum(item["bash_command_count"] for item in items),
            "bash_command_breakdown": dict(sorted(bash_counts.items())),
            "bash_command_categories": dict(sorted(category_counts.items())),
            "efficient_cli_command_total": efficient_cli_total,
            "efficient_cli_command_rate": efficient_cli_total / sum(bash_counts.values())
            if bash_counts
            else 0,
            "posix_search_command_total": category_counts.get("posix_search", 0),
        }
    return out


def findings(summary: dict[str, Any]) -> list[str]:
    notes: list[str] = []
    if not summary:
        return notes

    efficient_total = sum(
        int(group.get("efficient_cli_command_total", 0)) for group in summary.values()
    )
    posix_search_total = sum(
        int(group.get("posix_search_command_total", 0)) for group in summary.values()
    )
    if efficient_total == 0 and posix_search_total > 0:
        notes.append(
            "No efficient search or semantic diff CLIs were used; agents relied on POSIX search commands."
        )

    unused_mcp_groups = [
        name
        for name, group in summary.items()
        if "harness" in name
        and not any(str(tool).startswith("mcp__") for tool in group["tool_call_breakdown"])
    ]
    if unused_mcp_groups:
        notes.append(
            "Harness/Crawl4AI capability groups did not call MCP tools in this sample."
        )

    control = summary.get("agent_builtin") or summary.get("none")
    if control:
        control_tools = float(control.get("avg_tool_calls", 0))
        control_bash = float(control.get("avg_bash_commands", 0))
        higher_overhead = [
            name
            for name, group in summary.items()
            if group is not control
            and (
                float(group.get("avg_tool_calls", 0)) > control_tools
                or float(group.get("avg_bash_commands", 0)) > control_bash
            )
        ]
        if higher_overhead:
            notes.append(
                "Capability-enabled groups had higher average tool or shell-command counts than the control."
            )

    low_quality_groups = [
        name
        for name, group in summary.items()
        if float(group.get("quality_pass_rate", 0)) < 1.0
    ]
    if low_quality_groups:
        notes.append(
            "At least one group had sessions that completed without tool-work evidence."
        )

    return notes


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--home",
        default=os.environ.get("HARNESS_HOME", str(Path.home() / ".harness")),
        help="Harness home directory",
    )
    parser.add_argument("--profile", default="default")
    args = parser.parse_args()

    rows = session_rows(Path(args.home).expanduser(), args.profile)
    summary = summarize(rows)
    eligible = [
        row
        for row in rows
        if row["normalized_events"] > 0 and row["group"] != "none"
    ]
    report = {
        "profile": args.profile,
        "sessions_scanned": len(rows),
        "eligible_instrumented_sessions": len(eligible),
        "groups": summary,
        "findings": findings(summary),
        "sessions": rows,
        "minimum_viable_ab_ready": len({row["group"] for row in eligible}) >= 2,
    }
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
