#!/usr/bin/env python3
"""Cheap preflight for Xero vs OpenCode Terminal-Bench runs.

This script records toolchain readiness without running benchmark tasks or
spending model tokens. Oracle or smoke runs should be launched separately after
this manifest is clean and task ids are frozen in config.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def now() -> str:
    return datetime.now(timezone.utc).isoformat()


def run_command(argv: list[str], timeout: int = 30) -> dict[str, Any]:
    try:
        result = subprocess.run(
            argv,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except FileNotFoundError as exc:
        return {
            "status": "failed",
            "code": "command_missing",
            "command": argv,
            "message": str(exc),
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "status": "failed",
            "code": "command_timeout",
            "command": argv,
            "message": str(exc),
        }
    return {
        "status": "passed" if result.returncode == 0 else "failed",
        "code": "ok" if result.returncode == 0 else "nonzero_exit",
        "command": argv,
        "returnCode": result.returncode,
        "stdout": result.stdout[-4000:],
        "stderr": result.stderr[-4000:],
    }


def check_path_not_legacy(path: str | None) -> dict[str, Any]:
    if not path:
        return {"status": "skipped", "code": "not_configured"}
    parts = Path(path).parts
    if ".xero" in parts:
        return {
            "status": "failed",
            "code": "legacy_xero_state",
            "message": f"{path} is inside legacy repo-local .xero state",
        }
    return {"status": "passed", "code": "ok", "path": path}


def load_config(path: Path) -> dict[str, Any]:
    with path.open() as handle:
        return json.load(handle)


def config_check(config: dict[str, Any]) -> dict[str, Any]:
    required = ["schemaVersion", "benchmark", "model", "taskSets", "harnesses", "limits"]
    missing = [key for key in required if key not in config]
    if missing:
        return {"status": "failed", "code": "config_missing_keys", "missing": missing}
    task_sets = config.get("taskSets") or {}
    undeclared = [
        name
        for name, value in task_sets.items()
        if not isinstance(value.get("taskIds", []), list)
    ]
    if undeclared:
        return {"status": "failed", "code": "task_set_invalid", "taskSets": undeclared}
    return {"status": "passed", "code": "ok"}


def provider_credential_check(config: dict[str, Any]) -> dict[str, Any]:
    model = config.get("model") or {}
    credential_env = model.get("credentialEnv")
    if not credential_env:
        return {"status": "skipped", "code": "credential_env_not_configured"}
    return {
        "status": "passed" if os.environ.get(credential_env) else "failed",
        "code": "ok" if os.environ.get(credential_env) else "credential_env_missing",
        "env": credential_env,
        "valueRedacted": True,
    }


def harbor_help_check() -> dict[str, Any]:
    uvx = shutil.which("uvx")
    harbor = shutil.which("harbor")
    if uvx:
        result = run_command(["uvx", "harbor", "run", "--help"], timeout=60)
    elif harbor:
        result = run_command(["harbor", "run", "--help"], timeout=60)
    else:
        return {
            "status": "failed",
            "code": "harbor_launcher_missing",
            "message": "Neither uvx nor harbor is on PATH.",
        }
    help_text = f"{result.get('stdout', '')}\n{result.get('stderr', '')}".lower()
    result["opencodeBuiltIn"] = "opencode" in help_text
    return result


def build_manifest(config: dict[str, Any], args: argparse.Namespace) -> dict[str, Any]:
    xero_cli = os.environ.get("XERO_CLI_PATH", "xero")
    harbor = harbor_help_check()
    checks = {
        "config": config_check(config),
        "python3": run_command(["python3", "--version"]),
        "protoc": run_command(["protoc", "--version"]),
        "harborHelp": harbor,
        "docker": run_command(["docker", "info"], timeout=20),
        "xeroCli": run_command([xero_cli, "--version"]),
        "providerCredential": provider_credential_check(config),
        "trialState": check_path_not_legacy(args.trial_app_data_root),
        "outputRoot": check_path_not_legacy(args.output_root),
    }
    checks["xeroFakeProviderFixture"] = xero_fake_provider_fixture_check(
        config,
        xero_cli,
        args,
    )
    opencode_path = (config.get("harnesses") or {}).get("opencode", {})
    checks["opencodePath"] = {
        "status": "passed"
        if harbor.get("opencodeBuiltIn")
        else "skipped",
        "code": "harbor_builtin_opencode"
        if harbor.get("opencodeBuiltIn")
        else "fallback_wrapper_required",
        "preferredKind": opencode_path.get("preferredKind"),
        "agentName": opencode_path.get("agentName"),
        "fallbackImportPath": opencode_path.get("fallbackImportPath"),
    }
    failed = [name for name, check in checks.items() if check.get("status") == "failed"]
    return {
        "schema": "xero.benchmark.preflight.v1",
        "generatedAt": now(),
        "status": "failed" if failed else "passed",
        "failedChecks": failed,
        "benchmark": config.get("benchmark"),
        "model": {
            **(config.get("model") or {}),
            "credentialValueRedacted": True,
        },
        "environment": {
            "os": platform.system(),
            "architecture": platform.machine(),
            "pythonExecutable": sys.executable,
        },
        "checks": checks,
        "notes": [
            "This preflight does not run Terminal-Bench oracle, OpenCode, or paid Xero trials.",
            "The optional Xero fake-provider fixture is adapter plumbing only and must not be scored.",
            "Freeze concrete smoke task ids in config before paid comparison runs.",
        ],
    }


def xero_fake_provider_fixture_check(
    config: dict[str, Any],
    xero_cli: str,
    args: argparse.Namespace,
) -> dict[str, Any]:
    if args.skip_xero_fixture:
        return {"status": "skipped", "code": "disabled"}
    if not args.workspace_root or not args.trial_app_data_root or not args.output_root:
        return {
            "status": "skipped",
            "code": "paths_not_configured",
            "message": "Pass --workspace-root, --trial-app-data-root, and --output-root to run the fake-provider fixture.",
        }
    state_root = Path(args.trial_app_data_root) / "preflight-xero-app-data"
    output_dir = Path(args.output_root) / "preflight-xero-fixture"
    dataset_id = ((config.get("benchmark") or {}).get("datasetId")) or "terminal-bench@2.0"
    return run_command(
        [
            xero_cli,
            "--json",
            "benchmark",
            "terminal-bench",
            "--instruction",
            "Preflight fixture: inspect the workspace and finish without making changes.",
            "--workspace-root",
            args.workspace_root,
            "--trial-app-data-root",
            str(state_root),
            "--output-dir",
            str(output_dir),
            "--project-id",
            "preflight-project",
            "--session-id",
            "preflight-session",
            "--run-id",
            "preflight-run",
            "--task-id",
            "preflight-fixture",
            "--dataset-id",
            dataset_id,
            "--provider",
            "fake_provider",
            "--model",
            "fake-model",
            "--allow-fake-provider-fixture",
        ],
        timeout=60,
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--trial-app-data-root")
    parser.add_argument("--output-root")
    parser.add_argument("--workspace-root")
    parser.add_argument("--skip-xero-fixture", action="store_true")
    args = parser.parse_args()

    config = load_config(args.config)
    manifest = build_manifest(config, args)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": manifest["status"], "output": str(args.output)}))
    return 0 if manifest["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
