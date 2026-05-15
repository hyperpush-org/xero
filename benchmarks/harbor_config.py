#!/usr/bin/env python3
"""Build reproducible Harbor job configs for Terminal-Bench runs."""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from typing import Any

from benchmarks.cleanup import cleanup_benchmark_storage

DEFAULT_RETRY_INCLUDE = (
    "EnvironmentStartTimeoutError",
    "NonZeroAgentExitCodeError",
)
DEFAULT_RETRY_EXCLUDE = (
    "AgentTimeoutError",
    "VerifierTimeoutError",
    "RewardFileNotFoundError",
    "RewardFileEmptyError",
    "VerifierOutputParseError",
)
DEFAULT_OPENCODE_AUTH_PATH = Path.home() / ".local/share/opencode/auth.json"


def load_config(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def dataset_from_id(dataset_id: str) -> dict[str, str]:
    name, separator, version = dataset_id.partition("@")
    dataset = {"name": name}
    if separator and version:
        dataset["version"] = version
    return dataset


def task_names_for_set(config: dict[str, Any], task_set: str) -> list[str]:
    task_sets = config.get("taskSets") or {}
    if task_set not in task_sets:
        available = ", ".join(sorted(task_sets))
        raise ValueError(
            f"Unknown task set '{task_set}'. Available task sets: {available}"
        )
    task_ids = task_sets[task_set].get("taskIds") or []
    return [str(task_id) for task_id in task_ids]


def retry_config(limits: dict[str, Any]) -> dict[str, Any]:
    retry = limits.get("retry") or {}
    return {
        "max_retries": int(retry.get("maxRetries", 2)),
        "include_exceptions": list(
            retry.get("includeExceptions") or DEFAULT_RETRY_INCLUDE
        ),
        "exclude_exceptions": list(
            retry.get("excludeExceptions") or DEFAULT_RETRY_EXCLUDE
        ),
    }


def opencode_model_name(config: dict[str, Any]) -> str:
    model = config.get("model") or {}
    provider = model.get("opencodeProvider") or "openai"
    model_id = model.get("modelId")
    if not model_id:
        raise ValueError("model.modelId is required")
    return f"{provider}/{model_id}"


def opencode_agent_config(config: dict[str, Any]) -> dict[str, Any]:
    harness = ((config.get("harnesses") or {}).get("opencode")) or {}
    model = config.get("model") or {}
    agent: dict[str, Any] = {
        "model_name": opencode_model_name(config),
        "kwargs": {},
    }

    prewarmed_import_path = harness.get("prewarmedImportPath")
    if prewarmed_import_path:
        agent["import_path"] = prewarmed_import_path
    elif harness.get("preferredKind") == "fallback-installed-agent":
        agent["import_path"] = harness.get("fallbackImportPath")
    else:
        agent["name"] = harness.get("agentName") or "opencode"

    reasoning_effort = model.get("reasoningEffort")
    if reasoning_effort:
        agent["kwargs"]["variant"] = reasoning_effort
    if not agent["kwargs"]:
        del agent["kwargs"]
    return agent


def opencode_auth_mount(auth_path: Path) -> list[dict[str, Any]]:
    return [
        {
            "type": "bind",
            "source": str(auth_path),
            "target": "/root/.local/share/opencode/auth.json",
            "read_only": True,
            "bind": {"create_host_path": False},
        }
    ]


def build_opencode_harbor_config(
    config: dict[str, Any],
    *,
    task_set: str,
    jobs_dir: Path,
    job_name: str,
    concurrency: int | None = None,
    auth_path: Path = DEFAULT_OPENCODE_AUTH_PATH,
) -> dict[str, Any]:
    limits = config.get("limits") or {}
    dataset_id = (
        ((config.get("benchmark") or {}).get("datasetId")) or "terminal-bench@2.0"
    )
    dataset = dataset_from_id(dataset_id)
    task_names = task_names_for_set(config, task_set)
    if task_names:
        dataset["task_names"] = task_names

    harbor_config: dict[str, Any] = {
        "job_name": job_name,
        "jobs_dir": str(jobs_dir),
        "n_attempts": int(limits.get("attempts", 1)),
        "n_concurrent_trials": int(concurrency or limits.get("concurrency", 1)),
        "retry": retry_config(limits),
        "timeout_multiplier": float(limits.get("timeoutMultiplier", 1.0)),
        "agent_timeout_multiplier": float(limits.get("agentTimeoutMultiplier", 2.0)),
        "verifier_timeout_multiplier": float(
            limits.get("verifierTimeoutMultiplier", 1.0)
        ),
        "agent_setup_timeout_multiplier": float(
            limits.get("agentSetupTimeoutMultiplier", 2.0)
        ),
        "environment_build_timeout_multiplier": float(
            limits.get("environmentBuildTimeoutMultiplier", 3.0)
        ),
        "agents": [opencode_agent_config(config)],
        "datasets": [dataset],
        "environment": {
            "type": "docker",
            "mounts_json": opencode_auth_mount(auth_path),
        },
    }
    return harbor_config


def write_harbor_config(path: Path, config: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(config, indent=2, sort_keys=True) + "\n")


def default_run_root(task_set: str, concurrency: int, model_id: str) -> Path:
    date = datetime.now().strftime("%Y%m%d-%H%M%S")
    safe_model = model_id.replace(".", "")
    return Path(
        f"/tmp/xero-terminal-bench-{task_set}-opencode-openai-"
        f"{safe_model}-medium-c{concurrency}-{date}"
    )


def launch_detached(run_root: Path, config_path: Path) -> int:
    command = harbor_command(config_path)
    log_path = run_root / "harbor.nohup.log"
    log_handle = log_path.open("ab")
    try:
        process = subprocess.Popen(
            command,
            stdout=log_handle,
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
            start_new_session=True,
        )
    finally:
        log_handle.close()
    (run_root / "harbor.pid").write_text(f"{process.pid}\n")
    return process.pid


def harbor_command(config_path: Path) -> list[str]:
    uvx = shutil.which("uvx")
    harbor = shutil.which("harbor")
    if uvx:
        return [uvx, "harbor", "run", "--config", str(config_path)]
    elif harbor:
        return [harbor, "run", "--config", str(config_path)]
    raise RuntimeError("Neither uvx nor harbor is on PATH.")

def run_foreground(config_path: Path) -> int:
    return subprocess.run(harbor_command(config_path), check=False).returncode


def launch_cleanup_monitor(
    *,
    pid: int,
    run_root: Path,
    clean_tool_cache: bool,
    delete_run_root: bool,
) -> int:
    script = Path(__file__).resolve().parents[1] / "scripts" / "clean_benchmark_storage.py"
    command = [
        sys.executable,
        str(script),
        "--wait-pid",
        str(pid),
        "--run-root",
        str(run_root),
    ]
    if clean_tool_cache:
        command.append("--clean-tool-cache")
    if delete_run_root:
        command.append("--delete-run-root")

    cleanup_log = run_root / "cleanup.log"
    log_handle = cleanup_log.open("ab")
    try:
        process = subprocess.Popen(
            command,
            stdout=log_handle,
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
            start_new_session=True,
        )
    finally:
        log_handle.close()
    return process.pid


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Write a reproducible Harbor config for OpenCode Terminal-Bench."
    )
    parser.add_argument(
        "--config",
        type=Path,
        default=Path("benchmarks/config/terminal_bench_opencode_smoke.json"),
    )
    parser.add_argument("--task-set", default="full-terminal-bench-2")
    parser.add_argument("--concurrency", type=int)
    parser.add_argument("--job-name")
    parser.add_argument("--run-root", type=Path)
    parser.add_argument("--auth-path", type=Path, default=DEFAULT_OPENCODE_AUTH_PATH)
    parser.add_argument(
        "--run",
        action="store_true",
        help="Run Harbor in the foreground and clean benchmark storage on exit.",
    )
    parser.add_argument(
        "--detach",
        action="store_true",
        help="Start Harbor after writing the config.",
    )
    parser.add_argument(
        "--no-cleanup-on-exit",
        action="store_true",
        help="Do not run Docker/cache cleanup after a foreground or detached run exits.",
    )
    parser.add_argument(
        "--keep-tool-cache",
        action="store_true",
        help="Keep regenerated uv/Harbor caches during post-run cleanup.",
    )
    parser.add_argument(
        "--delete-run-root-after-cleanup",
        action="store_true",
        help="Delete benchmark result artifacts after the run exits.",
    )
    args = parser.parse_args()
    if args.run and args.detach:
        raise SystemExit("Use only one of --run or --detach.")

    config = load_config(args.config)
    limits = config.get("limits") or {}
    model_id = ((config.get("model") or {}).get("modelId")) or "gpt-5.5"
    concurrency = int(args.concurrency or limits.get("concurrency", 1))
    run_root = args.run_root or default_run_root(args.task_set, concurrency, model_id)
    job_name = args.job_name or run_root.name
    jobs_dir = run_root / "jobs"
    harbor_config_path = run_root / "full-config.json"

    if not args.auth_path.is_file():
        raise SystemExit(f"OpenCode auth file is missing: {args.auth_path}")

    harbor_config = build_opencode_harbor_config(
        config,
        task_set=args.task_set,
        jobs_dir=jobs_dir,
        job_name=job_name,
        concurrency=concurrency,
        auth_path=args.auth_path,
    )
    write_harbor_config(harbor_config_path, harbor_config)

    output = {
        "status": "started" if args.detach else "configured",
        "runRoot": str(run_root),
        "config": str(harbor_config_path),
        "job": str(jobs_dir / job_name),
        "watchCommand": (
            f"python3 scripts/watch_opencode_benchmark.py {jobs_dir / job_name}"
        ),
        "cleanupCommand": (
            f"python3 scripts/clean_benchmark_storage.py "
            f"--run-root {run_root} --clean-tool-cache"
        ),
    }

    cleanup_on_exit = (args.run or args.detach) and not args.no_cleanup_on_exit
    clean_tool_cache = not args.keep_tool_cache
    if args.detach:
        pid = launch_detached(run_root, harbor_config_path)
        output["pid"] = pid
        if cleanup_on_exit:
            output["cleanupMonitorPid"] = launch_cleanup_monitor(
                pid=pid,
                run_root=run_root,
                clean_tool_cache=clean_tool_cache,
                delete_run_root=args.delete_run_root_after_cleanup,
            )
            output["cleanupLog"] = str(run_root / "cleanup.log")
    print(json.dumps(output, indent=2, sort_keys=True))

    if args.run:
        exit_code = 1
        try:
            exit_code = run_foreground(harbor_config_path)
        finally:
            if cleanup_on_exit:
                cleanup_result = cleanup_benchmark_storage(
                    run_root=run_root,
                    delete_run_root=args.delete_run_root_after_cleanup,
                    clean_tool_cache=clean_tool_cache,
                )
                print(json.dumps({"cleanup": cleanup_result}, indent=2, sort_keys=True))
        return exit_code
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
