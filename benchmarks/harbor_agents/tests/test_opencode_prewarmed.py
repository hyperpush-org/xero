from __future__ import annotations

import unittest

from benchmarks.harbor_agents.opencode_prewarmed import OpenCodePrewarmedAgent


class CapturingOpenCodePrewarmedAgent(OpenCodePrewarmedAgent):
    def __init__(self) -> None:
        super().__init__()
        self.commands: list[tuple[str, dict[str, str] | None]] = []

    async def exec_as_agent(self, environment, command, env=None):  # type: ignore[no-untyped-def]
        self.commands.append((command, env))


class OpenCodePrewarmedTests(unittest.IsolatedAsyncioTestCase):
    async def test_install_runs_opencode_db_migrate_after_base_install(self):
        agent = CapturingOpenCodePrewarmedAgent()

        await agent.install(object())

        self.assertEqual(len(agent.commands), 1)
        command, env = agent.commands[0]
        self.assertIn("opencode db migrate", command)
        self.assertEqual(env["OPENCODE_FAKE_VCS"], "git")
        self.assertEqual(env["OPENCODE_DISABLE_AUTOUPDATE"], "1")


if __name__ == "__main__":
    unittest.main()
