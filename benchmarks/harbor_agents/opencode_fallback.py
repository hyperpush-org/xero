"""Labeled fallback Harbor installed-agent wrapper for OpenCode.

Use this only when the installed Harbor version does not expose its built-in
`opencode` agent. Reports must label fallback results separately from built-in
OpenCode results.
"""

from __future__ import annotations

import json
import os
import shlex
from pathlib import Path
from typing import Any

try:
    from harbor.agents.installed.base import BaseInstalledAgent, with_prompt_template
    from harbor.environments.base import BaseEnvironment
    from harbor.models.agent.context import AgentContext
except ModuleNotFoundError:  # pragma: no cover
    BaseEnvironment = Any
    AgentContext = Any

    def with_prompt_template(fn: Any) -> Any:
        return fn

    class BaseInstalledAgent:  # type: ignore[no-redef]
        def __init__(self, logs_dir: Path, version: str | None = None, *args: Any, **kwargs: Any):
            self.logs_dir = Path(logs_dir)
            self._version = version
            self.model_name = kwargs.get("model_name")

        def version(self) -> str | None:
            return self._version


class OpenCodeFallbackAgent(BaseInstalledAgent):
    """Run `opencode run` non-interactively as a labeled fallback."""

    SUPPORTS_ATIF = False

    @staticmethod
    def name() -> str:
        return "opencode-fallback"

    def get_version_command(self) -> str | None:
        return "opencode --version"

    async def install(self, environment: BaseEnvironment) -> None:
        version_spec = f"@{self._version}" if self._version else "@latest"
        await self.exec_as_root(
            environment,
            command="apt-get update && apt-get install -y curl",
            env={"DEBIAN_FRONTEND": "noninteractive"},
        )
        await self.exec_as_agent(
            environment,
            command=(
                "curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.2/install.sh | bash && "
                'export NVM_DIR="$HOME/.nvm" && . "$NVM_DIR/nvm.sh" && '
                "nvm install 22 && "
                f"npm i -g opencode-ai{version_spec} && "
                "opencode --version"
            ),
        )

    @with_prompt_template
    async def run(
        self,
        instruction: str,
        environment: BaseEnvironment,
        context: AgentContext,
    ) -> None:
        if not self.model_name:
            raise ValueError("OpenCode fallback requires Harbor to provide a model name.")
        config_dir = self.logs_dir / "opencode-config"
        output_path = self.logs_dir / "opencode.jsonl"
        env = {
            "OPENCODE_CONFIG_DIR": str(config_dir),
            "OPENCODE_DISABLE_AUTOUPDATE": "1",
            "OPENCODE_FAKE_VCS": "git",
        }
        for key in (
            "OPENAI_API_KEY",
            "OPENAI_BASE_URL",
            "OPENROUTER_API_KEY",
            "ANTHROPIC_API_KEY",
            "GEMINI_API_KEY",
            "GOOGLE_API_KEY",
        ):
            if key in os.environ:
                env[key] = os.environ[key]
        await self.exec_as_agent(
            environment,
            command=(
                f"mkdir -p {shlex.quote(str(config_dir))} && "
                f"opencode run --model {shlex.quote(self.model_name)} "
                f"--dir . --format json -- {shlex.quote(instruction)} "
                f"> {shlex.quote(str(output_path))} 2>&1"
            ),
            env=env,
        )

    def populate_context_post_run(self, context: AgentContext) -> None:
        output_path = self.logs_dir / "opencode.jsonl"
        metadata = getattr(context, "metadata", None) or {}
        metadata["opencode_fallback_output"] = str(output_path)
        metadata["opencode_integration"] = "fallback-installed-agent"
        context.metadata = metadata
        if output_path.exists():
            for line in output_path.read_text().splitlines():
                try:
                    event = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if event.get("type") == "step_finish":
                    part = event.get("part") or {}
                    tokens = part.get("tokens") or {}
                    if tokens.get("input") is not None:
                        context.n_input_tokens = (context.n_input_tokens or 0) + tokens.get("input", 0)
                    if tokens.get("output") is not None:
                        context.n_output_tokens = (context.n_output_tokens or 0) + tokens.get("output", 0)
                    if part.get("cost") is not None:
                        context.cost_usd = (context.cost_usd or 0) + part.get("cost", 0)
