"""Harbor installed-agent adapter for Xero Terminal-Bench trials.

The adapter intentionally keeps Harbor as the benchmark authority: Harbor
materializes tasks, owns the sandbox, and runs verifiers. Xero only receives the
rendered instruction and writes trial-local artifacts through its headless CLI.
"""

from __future__ import annotations

import json
import os
import re
import shlex
import subprocess
from pathlib import Path
from typing import Any

try:
    from harbor.agents.installed.base import BaseInstalledAgent, with_prompt_template
    from harbor.environments.base import BaseEnvironment
    from harbor.models.agent.context import AgentContext
except ModuleNotFoundError:  # pragma: no cover - exercised by local unit tests without Harbor.
    BaseEnvironment = Any
    AgentContext = Any

    def with_prompt_template(fn: Any) -> Any:
        return fn

    class BaseInstalledAgent:  # type: ignore[no-redef]
        def __init__(
            self,
            logs_dir: Path,
            version: str | None = None,
            extra_env: dict[str, str] | None = None,
            *args: Any,
            **kwargs: Any,
        ) -> None:
            self.logs_dir = Path(logs_dir)
            self._version = version
            self._extra_env = dict(extra_env or {})
            self.model_name = kwargs.get("model_name")
            self.logger = _FallbackLogger()

        def version(self) -> str | None:
            return self._version

        async def exec_as_agent(self, *args: Any, **kwargs: Any) -> Any:
            raise RuntimeError("Harbor is required to execute the Xero installed agent.")

        async def exec_as_root(self, *args: Any, **kwargs: Any) -> Any:
            raise RuntimeError("Harbor is required to execute the Xero installed agent.")


class _FallbackLogger:
    def debug(self, *args: Any, **kwargs: Any) -> None:
        pass

    def exception(self, *args: Any, **kwargs: Any) -> None:
        pass


ADAPTER_VERSION = "xero-terminal-bench-harbor-adapter.v1"

APPROVED_ENV_BY_PROVIDER: dict[str, tuple[str, ...]] = {
    "openai_api": ("OPENAI_API_KEY", "OPENAI_BASE_URL"),
    "openrouter": ("OPENROUTER_API_KEY",),
    "github_models": ("GITHUB_TOKEN",),
    "gemini_ai_studio": ("GEMINI_API_KEY", "GOOGLE_API_KEY"),
    "ollama": (),
    "fake_provider": (),
}

PROVIDER_ALIASES = {
    "openai": "openai_api",
    "google": "gemini_ai_studio",
    "gemini": "gemini_ai_studio",
    "github": "github_models",
}


def sanitize_identifier(value: str, fallback: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9_.-]+", "-", value).strip("-")
    return cleaned or fallback


def split_harbor_model(
    model_name: str | None,
    explicit_provider: str | None = None,
    explicit_model: str | None = None,
) -> tuple[str, str]:
    if explicit_provider:
        provider = PROVIDER_ALIASES.get(explicit_provider, explicit_provider)
        model = explicit_model or model_name or "default"
        if "/" in model and provider != "openrouter":
            model = model.split("/", 1)[1]
        return provider, model

    if not model_name:
        return "fake_provider", explicit_model or "fake-model"

    provider, _, model = model_name.partition("/")
    if not model:
        return PROVIDER_ALIASES.get(provider, provider), explicit_model or model_name
    provider = PROVIDER_ALIASES.get(provider, provider)
    return provider, explicit_model or model


def approved_provider_env(provider_id: str, extra_env: dict[str, str] | None = None) -> dict[str, str]:
    env: dict[str, str] = {}
    source = {**os.environ, **(extra_env or {})}
    for key in APPROVED_ENV_BY_PROVIDER.get(provider_id, ()):
        if key in source:
            env[key] = source[key]
    return env


def api_key_env_name(provider_id: str) -> str | None:
    for key in APPROVED_ENV_BY_PROVIDER.get(provider_id, ()):
        if key.endswith("_API_KEY") or key == "GITHUB_TOKEN":
            return key
    return None


def git_source_revision(source_dir: str | None) -> str | None:
    if not source_dir:
        return os.environ.get("XERO_SOURCE_REVISION")
    try:
        result = subprocess.run(
            ["git", "-C", source_dir, "rev-parse", "HEAD"],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            timeout=5,
        )
    except (OSError, subprocess.TimeoutExpired):
        return os.environ.get("XERO_SOURCE_REVISION")
    revision = result.stdout.strip()
    return revision or os.environ.get("XERO_SOURCE_REVISION")


class XeroInstalledAgent(BaseInstalledAgent):
    """Run Xero's owned-agent harness as a Harbor installed agent."""

    SUPPORTS_ATIF = True

    def __init__(
        self,
        *args: Any,
        xero_cli_path: str | None = None,
        xero_source_dir: str | None = None,
        provider_id: str | None = None,
        model_id: str | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init__(*args, version=kwargs.pop("version", None), **kwargs)
        self.xero_cli_path = xero_cli_path or os.environ.get("XERO_CLI_PATH") or "xero"
        self.xero_source_dir = xero_source_dir or os.environ.get("XERO_SOURCE_DIR")
        self.explicit_provider_id = provider_id or os.environ.get("XERO_PROVIDER_ID")
        self.explicit_model_id = model_id or os.environ.get("XERO_MODEL_ID")

    @staticmethod
    def name() -> str:
        return "xero"

    def get_version_command(self) -> str | None:
        return f"{shlex.quote(self.xero_cli_path)} --version"

    async def install(self, environment: BaseEnvironment) -> None:
        if self.xero_source_dir:
            source_dir = shlex.quote(self.xero_source_dir)
            await self.exec_as_agent(environment, command="command -v protoc >/dev/null")
            await self.exec_as_agent(
                environment,
                command=(
                    "cargo build "
                    f"--manifest-path {source_dir}/client/src-tauri/Cargo.toml "
                    "-p xero-cli --bin xero"
                ),
            )
            self.xero_cli_path = str(
                Path(self.xero_source_dir) / "client/src-tauri/target/debug/xero"
            )
        await self.exec_as_agent(
            environment,
            command=(
                f"test -x {shlex.quote(self.xero_cli_path)} || "
                f"command -v {shlex.quote(self.xero_cli_path)}"
            ),
        )
        await self.exec_as_agent(
            environment,
            command=f"{shlex.quote(self.xero_cli_path)} --version",
        )

    def _task_identity(self, context: AgentContext) -> tuple[str, int]:
        metadata = getattr(context, "metadata", None) or {}
        task_id = (
            metadata.get("task_id")
            or metadata.get("taskId")
            or os.environ.get("HARBOR_TASK_ID")
            or os.environ.get("TASK_ID")
            or "unknown-task"
        )
        attempt = metadata.get("attempt_index") or metadata.get("attemptIndex") or os.environ.get(
            "HARBOR_ATTEMPT_INDEX", "0"
        )
        try:
            attempt_index = int(attempt)
        except (TypeError, ValueError):
            attempt_index = 0
        return str(task_id), attempt_index

    def build_run_command(
        self,
        instruction: str,
        context: AgentContext,
    ) -> tuple[str, dict[str, str], Path]:
        provider_id, model_id = split_harbor_model(
            getattr(self, "model_name", None),
            self.explicit_provider_id,
            self.explicit_model_id,
        )
        task_id, attempt_index = self._task_identity(context)
        run_id = sanitize_identifier(
            os.environ.get("XERO_BENCHMARK_RUN_ID", f"harbor-{task_id}-{attempt_index}"),
            "harbor-run",
        )
        project_id = sanitize_identifier(
            os.environ.get("XERO_BENCHMARK_PROJECT_ID", f"xero-{task_id}"),
            "xero-benchmark-project",
        )
        output_dir = self.logs_dir
        app_data_root = output_dir / "xero-app-data"
        dataset_id = os.environ.get("TERMINAL_BENCH_DATASET", "terminal-bench@2.0")
        api_env = api_key_env_name(provider_id)
        env = approved_provider_env(provider_id, getattr(self, "_extra_env", {}))
        env["XERO_BENCHMARK_ADAPTER"] = ADAPTER_VERSION

        flags = [
            "benchmark",
            "terminal-bench",
            "--instruction-file",
            "-",
            "--workspace-root",
            ".",
            "--trial-app-data-root",
            str(app_data_root),
            "--output-dir",
            str(output_dir),
            "--project-id",
            project_id,
            "--session-id",
            f"{run_id}-session",
            "--run-id",
            run_id,
            "--task-id",
            task_id,
            "--attempt-index",
            str(attempt_index),
            "--dataset-id",
            dataset_id,
            "--provider",
            provider_id,
            "--model",
            model_id,
            "--adapter-version",
            ADAPTER_VERSION,
            "--harness-version",
            os.environ.get("HARBOR_VERSION", "harbor"),
            "--comparison-mode",
            os.environ.get("XERO_BENCHMARK_MODE", "fixed-model"),
            "--approval-mode",
            os.environ.get("XERO_APPROVAL_MODE", "strict"),
            "--sandbox-policy",
            os.environ.get("XERO_SANDBOX_POLICY", "harbor_task_sandbox"),
            "--network-policy",
            os.environ.get("XERO_NETWORK_POLICY", "harbor_controlled"),
        ]
        optional_flags = {
            "--dataset-digest": os.environ.get("TERMINAL_BENCH_DATASET_DIGEST"),
            "--xero-source-revision": git_source_revision(self.xero_source_dir),
            "--api-key-env": api_env,
            "--base-url": os.environ.get("XERO_PROVIDER_BASE_URL"),
            "--temperature": os.environ.get("XERO_TEMPERATURE"),
            "--reasoning-effort": os.environ.get("XERO_REASONING_EFFORT"),
            "--max-output-tokens": os.environ.get("XERO_MAX_OUTPUT_TOKENS"),
            "--context-budget": os.environ.get("XERO_CONTEXT_BUDGET"),
            "--wall-time-seconds": os.environ.get("XERO_WALL_TIME_SECONDS"),
            "--max-turns": os.environ.get("XERO_MAX_TURNS"),
            "--max-tool-calls": os.environ.get("XERO_MAX_TOOL_CALLS"),
            "--max-command-calls": os.environ.get("XERO_MAX_COMMAND_CALLS"),
            "--max-cost-usd": os.environ.get("XERO_MAX_COST_USD"),
            "--sandbox-provider": os.environ.get("XERO_SANDBOX_PROVIDER", "harbor"),
            "--environment-id": os.environ.get("HARBOR_ENVIRONMENT_ID"),
            "--image-digest": os.environ.get("HARBOR_IMAGE_DIGEST"),
            "--provider-account-class": os.environ.get("XERO_PROVIDER_ACCOUNT_CLASS"),
            "--endpoint-class": os.environ.get("XERO_ENDPOINT_CLASS"),
        }
        for flag, value in optional_flags.items():
            if value:
                flags.extend([flag, value])
        if provider_id == "fake_provider":
            flags.append("--allow-fake-provider-fixture")

        quoted_flags = " ".join(shlex.quote(part) for part in flags)
        command = (
            f"mkdir -p {shlex.quote(str(output_dir))} && "
            f"printf %s {shlex.quote(instruction)} | "
            f"{shlex.quote(self.xero_cli_path)} {quoted_flags}"
        )
        return command, env, output_dir

    @with_prompt_template
    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        command, env, output_dir = self.build_run_command(instruction, context)
        await self.exec_as_agent(
            environment,
            command=(
                f"{command} "
                f"> {shlex.quote(str(output_dir / 'process-stdout.txt'))} "
                f"2> {shlex.quote(str(output_dir / 'process-stderr.txt'))}"
            ),
            env=env,
            timeout_sec=int(os.environ.get("XERO_WALL_TIME_SECONDS", "3600")),
        )

    def populate_context_post_run(self, context: AgentContext) -> None:
        manifest_path = self.logs_dir / "manifest.json"
        if not manifest_path.exists():
            return
        try:
            manifest = json.loads(manifest_path.read_text())
        except (OSError, json.JSONDecodeError):
            self.logger.exception("Failed to parse Xero benchmark manifest")
            return

        metrics = manifest.get("metrics") or {}
        if metrics.get("costUsd") is not None:
            context.cost_usd = metrics["costUsd"]
        if metrics.get("inputTokens") is not None:
            context.n_input_tokens = metrics["inputTokens"]
        if metrics.get("outputTokens") is not None:
            context.n_output_tokens = metrics["outputTokens"]
        context.metadata = {
            **(getattr(context, "metadata", None) or {}),
            "xero_manifest": str(manifest_path),
            "xero_status": (manifest.get("run") or {}).get("status"),
            "xero_failure_category": (manifest.get("run") or {}).get("failureCategory"),
            "xero_artifacts": manifest.get("artifacts") or {},
        }
