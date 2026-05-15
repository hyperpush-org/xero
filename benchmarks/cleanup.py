#!/usr/bin/env python3
"""Cleanup helpers for benchmark storage created by Terminal-Bench runs."""

from __future__ import annotations

import argparse
import errno
import json
import os
import shutil
import stat
import subprocess
import time
from pathlib import Path
from typing import Any, Callable

CommandRunner = Callable[[list[str]], dict[str, Any]]


def run_command(argv: list[str]) -> dict[str, Any]:
    try:
        result = subprocess.run(
            argv,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    except FileNotFoundError as exc:
        return {
            "command": argv,
            "status": "skipped",
            "code": "command_missing",
            "message": str(exc),
        }
    return {
        "command": argv,
        "status": "passed" if result.returncode == 0 else "failed",
        "returnCode": result.returncode,
        "stdout": result.stdout[-4000:],
        "stderr": result.stderr[-4000:],
    }


def _make_writable(path: Path) -> None:
    try:
        mode = path.stat().st_mode
        path.chmod(mode | stat.S_IWUSR)
    except OSError:
        return


def _make_tree_writable(path: Path) -> None:
    if path.is_dir() and not path.is_symlink():
        for child in path.rglob("*"):
            _make_writable(child)
    _make_writable(path)


def remove_tree(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {"path": str(path), "status": "skipped", "code": "missing"}

    def on_error(function, failing_path, exc_info):  # type: ignore[no-untyped-def]
        _make_writable(Path(failing_path))
        function(failing_path)

    try:
        if path.is_dir() and not path.is_symlink():
            _make_tree_writable(path)
            shutil.rmtree(path, onerror=on_error)
        else:
            _make_writable(path)
            path.unlink()
    except OSError as exc:
        return {
            "path": str(path),
            "status": "failed",
            "code": "remove_failed",
            "message": str(exc),
        }
    return {"path": str(path), "status": "passed", "code": "removed"}


def wait_for_pid(pid: int, poll_interval: float = 5.0) -> None:
    while True:
        try:
            os.kill(pid, 0)
        except OSError as exc:
            if exc.errno == errno.ESRCH:
                return
            raise
        time.sleep(poll_interval)


def cleanup_benchmark_storage(
    *,
    run_root: Path | None = None,
    delete_run_root: bool = False,
    clean_docker: bool = True,
    clean_tool_cache: bool = False,
    home: Path | None = None,
    command_runner: CommandRunner = run_command,
) -> dict[str, Any]:
    home = home or Path.home()
    actions: dict[str, Any] = {}

    if clean_docker:
        actions["dockerBuilderPrune"] = command_runner(
            ["docker", "builder", "prune", "-af"]
        )
        actions["dockerSystemPrune"] = command_runner(
            ["docker", "system", "prune", "-af"]
        )

    if clean_tool_cache:
        actions["uvArchiveCache"] = remove_tree(home / ".cache" / "uv" / "archive-v0")
        actions["harborTaskCache"] = remove_tree(home / ".cache" / "harbor" / "tasks")

    if delete_run_root and run_root is not None:
        actions["runRoot"] = remove_tree(run_root)

    failed = [
        name
        for name, action in actions.items()
        if isinstance(action, dict) and action.get("status") == "failed"
    ]
    return {
        "status": "failed" if failed else "passed",
        "failedActions": failed,
        "runRootPreserved": str(run_root) if run_root and not delete_run_root else None,
        "actions": actions,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Clean benchmark-created storage.")
    parser.add_argument("--run-root", type=Path)
    parser.add_argument("--delete-run-root", action="store_true")
    parser.add_argument("--wait-pid", type=int)
    parser.add_argument("--poll-interval", type=float, default=5.0)
    parser.add_argument("--skip-docker", action="store_true")
    parser.add_argument("--clean-tool-cache", action="store_true")
    args = parser.parse_args()

    if args.wait_pid is not None:
        wait_for_pid(args.wait_pid, poll_interval=args.poll_interval)

    result = cleanup_benchmark_storage(
        run_root=args.run_root,
        delete_run_root=args.delete_run_root,
        clean_docker=not args.skip_docker,
        clean_tool_cache=args.clean_tool_cache,
    )
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
