#!/usr/bin/env python3
"""Start servers, wait for ports, run a command, then clean up.

Default mode is shell-free:

  python3 scripts/with_server.py \
    --server "pnpm dev --host 127.0.0.1" --cwd frontend --port 5173 \
    -- agent-browser open http://127.0.0.1:5173

Use --shell only for trusted commands that require shell features.
"""

from __future__ import annotations

import argparse
import os
import shlex
import signal
import socket
import subprocess
import time
from dataclasses import dataclass
from typing import Sequence


@dataclass
class Server:
    command: str
    port: int
    cwd: str | None


def port_ready(port: int, host: str, timeout: float) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection((host, port), timeout=1):
                return True
        except OSError:
            time.sleep(0.25)
    return False


def process_args(command: str, shell: bool) -> str | list[str]:
    if shell:
        return command
    return shlex.split(command, posix=(os.name != "nt"))


def start_server(server: Server, shell: bool) -> subprocess.Popen[bytes]:
    kwargs = {}
    if os.name == "posix":
        kwargs["start_new_session"] = True
    return subprocess.Popen(
        process_args(server.command, shell),
        cwd=server.cwd,
        shell=shell,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        **kwargs,
    )


def stop_process(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    try:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGTERM)
        else:
            process.terminate()
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        else:
            process.kill()
        process.wait(timeout=5)


def parse_servers(args: argparse.Namespace) -> list[Server]:
    if len(args.server) != len(args.port):
        raise SystemExit("error: --server and --port counts must match")
    cwd_values = args.cwd or []
    if cwd_values and len(cwd_values) not in (1, len(args.server)):
        raise SystemExit("error: pass either one --cwd or one --cwd per --server")
    if len(cwd_values) == 1 and len(args.server) > 1:
        cwd_values = cwd_values * len(args.server)
    if not cwd_values:
        cwd_values = [None] * len(args.server)
    return [
        Server(command=command, port=port, cwd=cwd)
        for command, port, cwd in zip(args.server, args.port, cwd_values)
    ]


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Run a command with one or more local web servers."
    )
    parser.add_argument("--server", action="append", required=True)
    parser.add_argument("--port", action="append", type=int, required=True)
    parser.add_argument("--cwd", action="append", help="Working directory for server")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument(
        "--shell",
        action="store_true",
        help="Run server commands through the shell; trusted commands only.",
    )
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)

    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        raise SystemExit("error: command after -- is required")

    servers = parse_servers(args)
    processes: list[subprocess.Popen[bytes]] = []
    try:
        for index, server in enumerate(servers, start=1):
            print(f"starting server {index}: {server.command}", flush=True)
            process = start_server(server, args.shell)
            processes.append(process)
            if not port_ready(server.port, args.host, args.timeout):
                raise RuntimeError(
                    f"server {index} did not open {args.host}:{server.port}"
                )
            print(f"server {index} ready on {args.host}:{server.port}", flush=True)
        return subprocess.run(command).returncode
    finally:
        for process in reversed(processes):
            stop_process(process)


if __name__ == "__main__":
    raise SystemExit(main())
