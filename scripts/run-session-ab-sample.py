#!/usr/bin/env python3
"""Launch a small controlled Harness A/B sample.

Creates one session for each task-type/capability-profile pair and sends a
bounded prompt. Intended for Task 31 sampling against a local dev server.
"""

from __future__ import annotations

import argparse
import json
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path


PROTOCOL = "1.0"


@dataclass(frozen=True)
class Case:
    task_type: str
    profile: str
    prompt: str


def request(base: str, method: str, path: str, body: object | bytes | None = None) -> object | None:
    headers = {"X-Protocol-Version": PROTOCOL}
    data: bytes | None = None
    if isinstance(body, bytes):
        data = body
        headers["Content-Type"] = "application/octet-stream"
    elif body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(
        base.rstrip("/") + path,
        data=data,
        headers=headers,
        method=method,
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as res:
            raw = res.read()
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode(errors="replace")
        raise RuntimeError(f"{method} {path} failed: HTTP {exc.code}: {detail}") from exc
    if not raw:
        return None
    return json.loads(raw.decode())


def cases(force_efficient_cli: bool = False, task_set: str = "toy") -> list[Case]:
    base_prompt = (
        "You are generating Harness Task 31 A/B metrics. Work only inside the "
        "current cwd. Keep the response concise. Do not access private files. "
    )
    if force_efficient_cli:
        base_prompt += (
            "You must use fd for file discovery and rg for text search when available. "
            "Do not use find or grep unless fd/rg are unavailable. "
        )
    if task_set == "repo-search":
        base_prompt += "This is a read-only repo analysis task. Do not edit files. "
        return [
            Case(
                "repo-map",
                profile,
                base_prompt
                + "Map the repository structure: identify the main app/runtime stacks, "
                "entrypoints, package/workspace manifests, and test/build commands. "
                "Cite only file paths you inspected. End with 'AB_SAMPLE_DONE'.",
            )
            for profile in ("none", "harness", "harness_crawl4ai")
        ] + [
            Case(
                "config-search",
                profile,
                base_prompt
                + "Find where environment variables, configuration loading, routes, "
                "and database settings are defined. Summarize the key files and "
                "symbols. Avoid reading .env secret values. End with 'AB_SAMPLE_DONE'.",
            )
            for profile in ("none", "harness", "harness_crawl4ai")
        ] + [
            Case(
                "domain-trace",
                profile,
                base_prompt
                + "Trace the code paths for user/account/session or authentication "
                "logic. Identify important files, structs/functions/components, and "
                "how data flows at a high level. End with 'AB_SAMPLE_DONE'.",
            )
            for profile in ("none", "harness", "harness_crawl4ai")
        ]

    return [
        Case(
            "plan",
            profile,
            base_prompt
            + "Plan how to add median() and mode() helpers to math.ts. Do not edit files. "
            "End with 'AB_SAMPLE_DONE'.",
        )
        for profile in ("none", "harness", "harness_crawl4ai")
    ] + [
        Case(
            "refactor",
            profile,
            base_prompt
            + "Inspect math.ts and propose a low-risk refactor to reduce duplication. "
            "Do not edit files. End with 'AB_SAMPLE_DONE'.",
        )
        for profile in ("none", "harness", "harness_crawl4ai")
    ] + [
        Case(
            "code-write",
            profile,
            base_prompt
            + "Edit math.ts to add an exported median(values: number[]): number function. "
            "Keep existing behavior. End with 'AB_SAMPLE_DONE'.",
        )
        for profile in ("none", "harness", "harness_crawl4ai")
    ]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default="http://127.0.0.1:43177")
    parser.add_argument("--cwd", required=True)
    parser.add_argument("--kind", default="claude")
    parser.add_argument("--pause", type=float, default=2.0)
    parser.add_argument(
        "--force-efficient-cli",
        action="store_true",
        help="Prompt agents to prefer fd/rg over find/grep.",
    )
    parser.add_argument(
        "--task-set",
        choices=("toy", "repo-search"),
        default="toy",
        help="Prompt matrix to run.",
    )
    args = parser.parse_args()

    cwd = str(Path(args.cwd).resolve())
    launched: list[dict[str, str]] = []
    for case in cases(args.force_efficient_cli, args.task_set):
        suffix = " efficient-cli" if args.force_efficient_cli else ""
        title = f"Task31 AB{suffix} {args.task_set} {case.task_type} {case.profile}"
        thread = request(
            args.base_url,
            "POST",
            "/api/threads",
            {"title": title, "cwd": cwd},
        )
        thread_id = str(thread["id"])
        session = request(
            args.base_url,
            "POST",
            f"/api/threads/{thread_id}/sessions",
            {
                "kind": args.kind,
                "cwd": cwd,
                "capability_profile": case.profile,
                "include_project_context": False,
                "cols": 100,
                "rows": 30,
            },
        )
        session_id = str(session["session_id"])
        time.sleep(args.pause)
        payload = b"\x1b[200~" + case.prompt.encode() + b"\x1b[201~\r"
        request(args.base_url, "POST", f"/api/sessions/{session_id}/input", payload)
        row = {
            "task_type": case.task_type,
            "profile": case.profile,
            "force_efficient_cli": str(args.force_efficient_cli).lower(),
            "thread_id": thread_id,
            "session_id": session_id,
        }
        launched.append(row)
        print(json.dumps(row, sort_keys=True), flush=True)
        time.sleep(args.pause)

    print(json.dumps({"launched": launched}, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
