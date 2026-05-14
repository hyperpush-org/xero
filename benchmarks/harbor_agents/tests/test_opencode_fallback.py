import asyncio
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from benchmarks.harbor_agents.opencode_fallback import OpenCodeFallbackAgent


class CapturingOpenCodeFallbackAgent(OpenCodeFallbackAgent):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.agent_calls = []

    async def exec_as_agent(self, environment, command, env=None, timeout_sec=None):
        self.agent_calls.append(
            {
                "environment": environment,
                "command": command,
                "env": env or {},
                "timeoutSec": timeout_sec,
            }
        )


class OpenCodeFallbackTests(unittest.TestCase):
    def test_run_uses_documented_noninteractive_env_and_trial_config(self):
        with tempfile.TemporaryDirectory() as tmp:
            agent = CapturingOpenCodeFallbackAgent(
                logs_dir=Path(tmp),
                model_name="openai/gpt-5.4",
            )

            with patch.dict("os.environ", {"OPENAI_API_KEY": "secret"}, clear=True):
                asyncio.run(agent.run("Fix the task", object(), SimpleNamespace()))

            self.assertEqual(len(agent.agent_calls), 1)
            call = agent.agent_calls[0]
            self.assertIn("opencode run --model openai/gpt-5.4", call["command"])
            self.assertIn("--format json", call["command"])
            self.assertEqual(call["env"]["OPENCODE_DISABLE_AUTOUPDATE"], "1")
            self.assertNotIn("OPENCODE_DISABLE_AUTO_UPDATE", call["env"])
            self.assertEqual(call["env"]["OPENAI_API_KEY"], "secret")
            self.assertTrue(call["env"]["OPENCODE_CONFIG_DIR"].endswith("opencode-config"))


if __name__ == "__main__":
    unittest.main()
