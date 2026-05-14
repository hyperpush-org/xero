import unittest
from tempfile import TemporaryDirectory
from pathlib import Path

from benchmarks.report import load_manifests, summarize, wilson


def manifest(harness: str, task: str, status: str, wall: int = 1000):
    return {
        "benchmark": {
            "name": "terminal-bench",
            "datasetId": "terminal-bench@2.0",
            "taskId": task,
        },
        "harness": {"name": harness},
        "model": {"provider": "openai_api", "modelId": "gpt-5.5"},
        "run": {"status": status},
        "verifier": {"resolved": status == "completed"},
        "metrics": {"wallTimeMs": wall},
        "_manifestPath": f"/tmp/{harness}/{task}/manifest.json",
    }


def fake_fixture_manifest(task: str):
    row = manifest("xero", task, "completed")
    row["harness"]["fakeProviderFixture"] = True
    row["_manifestPath"] = f"/tmp/xero/{task}/manifest.json"
    return row


class ReportTests(unittest.TestCase):
    def test_wilson_handles_empty_sample(self):
        self.assertEqual(wilson(0, 0), {"low": None, "high": None})

    def test_summarize_groups_harnesses_and_pairs_tasks(self):
        report = summarize(
            [
                manifest("xero", "task-1", "completed", 100),
                manifest("xero", "task-2", "agent_failure", 200),
                manifest("opencode", "task-1", "completed", 150),
            ]
        )

        self.assertEqual(report["harnesses"]["xero"]["taskCount"], 2)
        self.assertEqual(report["harnesses"]["xero"]["successes"], 1)
        self.assertEqual(report["harnesses"]["xero"]["agentCompletions"], 1)
        self.assertEqual(report["harnesses"]["opencode"]["passAt1"], 1)
        self.assertIs(report["pairedOutcomes"]["task-1"]["xero"]["resolved"], True)
        self.assertIs(report["pairedOutcomes"]["task-2"]["xero"]["resolved"], False)

    def test_summarize_excludes_fake_provider_fixtures_from_scores(self):
        report = summarize(
            [
                fake_fixture_manifest("fixture-task"),
                manifest("xero", "real-task", "completed", 100),
            ]
        )

        self.assertEqual(report["manifestCount"], 2)
        self.assertEqual(report["scoredManifestCount"], 1)
        self.assertEqual(report["excludedManifestCount"], 1)
        self.assertEqual(report["harnesses"]["xero"]["taskCount"], 1)
        self.assertNotIn("fixture-task", report["pairedOutcomes"])
        self.assertEqual(
            report["excludedManifests"][0]["reason"],
            "fake_provider_fixture",
        )

    def test_load_manifests_skips_harbor_collection_manifests(self):
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            harbor_manifest = root / "trial" / "artifacts" / "manifest.json"
            harbor_manifest.parent.mkdir(parents=True)
            harbor_manifest.write_text('[{"source": "/logs/artifacts", "status": "ok"}]')
            benchmark_manifest = root / "trial" / "agent" / "manifest.json"
            benchmark_manifest.parent.mkdir(parents=True)
            benchmark_manifest.write_text(
                """{
                  "benchmark": {"name": "terminal-bench", "datasetId": "terminal-bench@2.0", "taskId": "task-1"},
                  "harness": {"name": "xero"},
                  "run": {"status": "completed"},
                  "verifier": {"resolved": true}
                }"""
            )

            manifests = load_manifests(root)

        self.assertEqual(len(manifests), 1)
        self.assertEqual(manifests[0]["benchmark"]["taskId"], "task-1")


if __name__ == "__main__":
    unittest.main()
