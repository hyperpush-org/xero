import json
import sqlite3
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from benchmarks.preflight import (
    config_check,
    provider_credential_check,
    xero_fake_provider_fixture_check,
)


def write_openai_oauth_store(root: Path) -> None:
    connection = sqlite3.connect(root / "xero.db")
    with connection:
        connection.execute(
            """
            CREATE TABLE provider_credentials (
                provider_id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                oauth_account_id TEXT,
                oauth_session_id TEXT,
                oauth_access_token TEXT,
                oauth_expires_at INTEGER,
                updated_at TEXT NOT NULL
            )
            """
        )
        connection.execute(
            """
            INSERT INTO provider_credentials (
                provider_id,
                kind,
                oauth_account_id,
                oauth_session_id,
                oauth_access_token,
                oauth_expires_at,
                updated_at
            ) VALUES ('openai_codex', 'oauth_session', 'acct_123', 'sess_123', 'token', ?, 'now')
            """,
            (int(time.time()) + 3600,),
        )


def valid_config():
    return {
        "schemaVersion": 1,
        "benchmark": {"name": "terminal-bench", "datasetId": "terminal-bench@2.0"},
        "model": {"provider": "openai_api", "modelId": "gpt-5.4"},
        "limits": {"attempts": 1},
        "taskSets": {"comparison-smoke": {"description": "smoke", "taskIds": []}},
        "harnesses": {
            "xero": {
                "kind": "harbor-installed-agent",
                "agentImportPath": "benchmarks.harbor_agents.xero:XeroInstalledAgent",
            },
            "opencode": {
                "preferredKind": "fallback-installed-agent",
                "fallbackImportPath": "benchmarks.harbor_agents.opencode_fallback:OpenCodeFallbackAgent",
            },
        },
    }


class PreflightTests(unittest.TestCase):
    def test_config_check_rejects_malformed_task_sets(self):
        config = valid_config()
        config["taskSets"] = {"broken": []}

        self.assertEqual(config_check(config)["code"], "task_set_invalid")

    def test_config_check_requires_fallback_import_path_when_selected(self):
        config = valid_config()
        del config["harnesses"]["opencode"]["fallbackImportPath"]

        self.assertEqual(
            config_check(config)["code"],
            "opencode_fallback_import_path_missing",
        )

    def test_provider_credential_check_accepts_openai_oauth_store(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "app-data"
            root.mkdir()
            write_openai_oauth_store(root)
            config = valid_config()
            config["model"] = {
                "provider": "openai_codex",
                "modelId": "gpt-5.4",
                "credentialMode": "app_openai_oauth",
                "oauthAppDataRoot": str(root),
            }

            result = provider_credential_check(config)

        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["code"], "ok")
        self.assertEqual(result["credentialMode"], "app_openai_oauth")

    def test_provider_credential_check_uses_openai_oauth_env_override(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "app-data"
            root.mkdir()
            write_openai_oauth_store(root)
            with patch.dict(
                "os.environ",
                {
                    "XERO_PROVIDER_ID": "openai_codex",
                    "XERO_OPENAI_OAUTH_APP_DATA_ROOT": str(root),
                },
                clear=True,
            ):
                result = provider_credential_check(valid_config())

        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["code"], "ok")

    def test_xero_fixture_check_verifies_expected_artifacts_and_label(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            args = SimpleNamespace(
                skip_xero_fixture=False,
                workspace_root=str(root / "workspace"),
                trial_app_data_root=str(root / "state"),
                output_root=str(root / "artifacts"),
            )
            Path(args.workspace_root).mkdir()
            output_dir = Path(args.output_root) / "preflight-xero-fixture"
            output_dir.mkdir(parents=True)

            def fake_run_command(argv, timeout=30):
                for name in (
                    "trajectory.json",
                    "xero-trace.json",
                    "final.diff",
                    "support-bundle.zip",
                    "stdout.txt",
                    "stderr.txt",
                ):
                    (output_dir / name).write_text("{}")
                (output_dir / "manifest.json").write_text(
                    json.dumps({"harness": {"fakeProviderFixture": True}})
                )
                return {"status": "passed", "code": "ok", "command": argv}

            with patch("benchmarks.preflight.run_command", side_effect=fake_run_command):
                result = xero_fake_provider_fixture_check(valid_config(), "xero", args)

        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["code"], "ok")
        self.assertIn("manifest.json", result["verifiedArtifacts"])


if __name__ == "__main__":
    unittest.main()
