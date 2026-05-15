import tempfile
import unittest
from pathlib import Path

from benchmarks.harbor_config import build_opencode_harbor_config, dataset_from_id


def config_fixture():
    return {
        "schemaVersion": 1,
        "benchmark": {"name": "terminal-bench", "datasetId": "terminal-bench@2.0"},
        "model": {
            "provider": "openai_api",
            "modelId": "gpt-5.5",
            "reasoningEffort": "medium",
        },
        "limits": {
            "attempts": 1,
            "concurrency": 3,
            "timeoutMultiplier": 1,
            "agentTimeoutMultiplier": 2,
            "verifierTimeoutMultiplier": 1,
            "agentSetupTimeoutMultiplier": 2,
            "environmentBuildTimeoutMultiplier": 3,
            "retry": {
                "maxRetries": 2,
                "includeExceptions": [
                    "EnvironmentStartTimeoutError",
                    "NonZeroAgentExitCodeError",
                ],
                "excludeExceptions": [
                    "AgentTimeoutError",
                    "VerifierTimeoutError",
                    "RewardFileNotFoundError",
                    "RewardFileEmptyError",
                    "VerifierOutputParseError",
                ],
            },
        },
        "taskSets": {
            "comparison-smoke": {
                "description": "smoke",
                "taskIds": ["fix-git", "regex-log"],
            },
            "full-terminal-bench-2": {"description": "full", "taskIds": []},
        },
        "harnesses": {
            "xero": {
                "kind": "harbor-installed-agent",
                "agentImportPath": "benchmarks.harbor_agents.xero:XeroInstalledAgent",
            },
            "opencode": {
                "preferredKind": "harbor-built-in",
                "agentName": "opencode",
                "prewarmedImportPath": (
                    "benchmarks.harbor_agents.opencode_prewarmed:"
                    "OpenCodePrewarmedAgent"
                ),
                "fallbackImportPath": (
                    "benchmarks.harbor_agents.opencode_fallback:"
                    "OpenCodeFallbackAgent"
                ),
            },
        },
    }


class HarborConfigTests(unittest.TestCase):
    def test_dataset_from_id_splits_version(self):
        self.assertEqual(
            dataset_from_id("terminal-bench@2.0"),
            {"name": "terminal-bench", "version": "2.0"},
        )

    def test_full_opencode_config_uses_safe_retry_and_timeout_policy(self):
        with tempfile.TemporaryDirectory() as tmp:
            auth_path = Path(tmp) / "auth.json"
            auth_path.write_text("{}")
            harbor_config = build_opencode_harbor_config(
                config_fixture(),
                task_set="full-terminal-bench-2",
                jobs_dir=Path(tmp) / "jobs",
                job_name="opencode-full",
                auth_path=auth_path,
            )

        self.assertEqual(harbor_config["n_concurrent_trials"], 3)
        self.assertEqual(harbor_config["agent_timeout_multiplier"], 2)
        self.assertEqual(harbor_config["environment_build_timeout_multiplier"], 3)
        self.assertEqual(
            set(harbor_config["retry"]["include_exceptions"]),
            {"EnvironmentStartTimeoutError", "NonZeroAgentExitCodeError"},
        )
        self.assertIn(
            "AgentTimeoutError",
            harbor_config["retry"]["exclude_exceptions"],
        )
        self.assertEqual(
            harbor_config["agents"][0]["import_path"],
            "benchmarks.harbor_agents.opencode_prewarmed:OpenCodePrewarmedAgent",
        )
        self.assertEqual(harbor_config["agents"][0]["model_name"], "openai/gpt-5.5")
        self.assertEqual(harbor_config["agents"][0]["kwargs"]["variant"], "medium")
        self.assertNotIn("task_names", harbor_config["datasets"][0])

    def test_smoke_task_set_adds_task_filter(self):
        with tempfile.TemporaryDirectory() as tmp:
            harbor_config = build_opencode_harbor_config(
                config_fixture(),
                task_set="comparison-smoke",
                jobs_dir=Path(tmp) / "jobs",
                job_name="opencode-smoke",
                auth_path=Path(tmp) / "auth.json",
            )

        self.assertEqual(
            harbor_config["datasets"][0]["task_names"],
            ["fix-git", "regex-log"],
        )

    def test_concurrency_override_wins_over_config_default(self):
        with tempfile.TemporaryDirectory() as tmp:
            harbor_config = build_opencode_harbor_config(
                config_fixture(),
                task_set="full-terminal-bench-2",
                jobs_dir=Path(tmp) / "jobs",
                job_name="opencode-full",
                concurrency=1,
                auth_path=Path(tmp) / "auth.json",
            )

        self.assertEqual(harbor_config["n_concurrent_trials"], 1)


if __name__ == "__main__":
    unittest.main()
