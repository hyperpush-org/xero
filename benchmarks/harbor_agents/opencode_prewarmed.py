"""Harbor OpenCode wrapper that prewarms OpenCode's SQLite migration.

OpenCode 1.14.x exits after its first one-time database migration in fresh
containers. Terminal-Bench gives every trial a fresh container, so without this
prewarm the first real `opencode run` can exit successfully without running the
task. This wrapper keeps Harbor's built-in OpenCode behavior and only performs
the migration during agent setup.
"""

from __future__ import annotations

from typing import Any

try:
    from harbor.agents.installed.opencode import OpenCode
    from harbor.environments.base import BaseEnvironment
except ModuleNotFoundError:  # pragma: no cover - local tests can run without Harbor.
    BaseEnvironment = Any

    class OpenCode:  # type: ignore[no-redef]
        def __init__(self, *args: Any, **kwargs: Any) -> None:
            self._version = kwargs.get("version")

        @staticmethod
        def name() -> str:
            return "opencode"

        async def install(self, environment: BaseEnvironment) -> None:
            return None

        async def exec_as_agent(
            self,
            environment: BaseEnvironment,
            command: str,
            env: dict[str, str] | None = None,
        ) -> None:
            raise RuntimeError("Harbor is required to execute OpenCode.")


class OpenCodePrewarmedAgent(OpenCode):
    """Built-in Harbor OpenCode plus setup-time `opencode db migrate`."""

    @staticmethod
    def name() -> str:
        return "opencode-prewarmed"

    async def install(self, environment: BaseEnvironment) -> None:
        await super().install(environment)
        await self.exec_as_agent(
            environment,
            command=". ~/.nvm/nvm.sh; opencode db migrate",
            env={
                "OPENCODE_DISABLE_AUTOUPDATE": "1",
                "OPENCODE_FAKE_VCS": "git",
            },
        )
