import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from benchmarks.harbor_agents.xero import (
    XeroInstalledAgent,
    approved_provider_env,
    sanitize_identifier,
    split_harbor_model,
)


class XeroAdapterTests(unittest.TestCase):
    def test_split_harbor_model_maps_openai_provider(self):
        self.assertEqual(split_harbor_model("openai/gpt-5.4"), ("openai_api", "gpt-5.4"))
        self.assertEqual(
            split_harbor_model("openrouter/openai/gpt-5.4"),
            ("openrouter", "openai/gpt-5.4"),
        )
        self.assertEqual(
            split_harbor_model(
                "anything",
                explicit_provider="google",
                explicit_model="gemini-2.5-pro",
            ),
            ("gemini_ai_studio", "gemini-2.5-pro"),
        )
        self.assertEqual(
            split_harbor_model(
                "openai/gpt-5.4",
                explicit_provider="openai_codex",
            ),
            ("openai_codex", "gpt-5.4"),
        )

    def test_approved_provider_env_passes_names_without_expanding_scope(self):
        with patch.dict(
            "os.environ",
            {"OPENAI_API_KEY": "secret", "UNRELATED_SECRET": "do-not-pass"},
            clear=True,
        ):
            self.assertEqual(approved_provider_env("openai_api"), {"OPENAI_API_KEY": "secret"})

    def test_build_run_command_uses_stdin_and_artifact_paths(self):
        with tempfile.TemporaryDirectory() as tmp:
            with patch.dict("os.environ", {"TERMINAL_BENCH_DATASET": "terminal-bench@2.0"}):
                agent = XeroInstalledAgent(
                    logs_dir=Path(tmp),
                    model_name="openai/gpt-5.4",
                    xero_cli_path="/bin/xero",
                )
                context = SimpleNamespace(
                    metadata={"task_id": "task/with spaces", "attempt_index": 2}
                )

                command, env, output_dir = agent.build_run_command("Do the task", context)

        self.assertEqual(output_dir, Path(tmp))
        self.assertIn("printf %s", command)
        self.assertIn("--instruction-file -", command)
        self.assertIn("--provider openai_api", command)
        self.assertIn("--model gpt-5.4", command)
        self.assertIn("--api-key-env OPENAI_API_KEY", command)
        self.assertNotIn("secret", command)
        self.assertTrue(env["XERO_BENCHMARK_ADAPTER"].startswith("xero-terminal-bench"))

    def test_build_run_command_supports_openai_oauth_without_api_key_env(self):
        with tempfile.TemporaryDirectory() as tmp:
            with patch.dict(
                "os.environ",
                {
                    "TERMINAL_BENCH_DATASET": "terminal-bench@2.0",
                    "XERO_PROVIDER_ID": "openai_codex",
                    "XERO_OPENAI_OAUTH_APP_DATA_ROOT": "/Users/sn0w/Library/Application Support/dev.sn0w.xero",
                    "XERO_OPENAI_OAUTH_ACCOUNT_ID": "acct_123",
                },
                clear=True,
            ):
                agent = XeroInstalledAgent(
                    logs_dir=Path(tmp),
                    model_name="openai/gpt-5.4",
                    xero_cli_path="/bin/xero",
                )
                context = SimpleNamespace(metadata={"task_id": "oauth-task"})

                command, env, _ = agent.build_run_command("Do the task", context)

        self.assertIn("--provider openai_codex", command)
        self.assertIn("--model gpt-5.4", command)
        self.assertIn("--oauth-app-data-root", command)
        self.assertIn("--oauth-account-id acct_123", command)
        self.assertNotIn("--api-key-env", command)
        self.assertEqual(env["XERO_BENCHMARK_ADAPTER"], "xero-terminal-bench-harbor-adapter.v1")

    def test_sanitize_identifier_has_fallback(self):
        self.assertEqual(sanitize_identifier("task/with spaces", "fallback"), "task-with-spaces")
        self.assertEqual(sanitize_identifier("???", "fallback"), "fallback")


if __name__ == "__main__":
    unittest.main()
