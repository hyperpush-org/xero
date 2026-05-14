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

    def test_load_manifests_attaches_harbor_verifier_outcomes(self):
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            passing_manifest = root / "task-pass__abc" / "agent" / "manifest.json"
            passing_manifest.parent.mkdir(parents=True)
            passing_manifest.write_text(
                """{
                  "benchmark": {"name": "terminal-bench", "datasetId": "terminal-bench@2.0", "taskId": "task-pass"},
                  "harness": {"name": "xero"},
                  "run": {"status": "completed"}
                }"""
            )
            passing_result = root / "task-pass__abc" / "result.json"
            passing_result.write_text(
                """{"verifier_result": {"rewards": {"reward": 1.0}}}"""
            )

            failing_manifest = root / "task-fail__def" / "agent" / "manifest.json"
            failing_manifest.parent.mkdir(parents=True)
            failing_manifest.write_text(
                """{
                  "benchmark": {"name": "terminal-bench", "datasetId": "terminal-bench@2.0", "taskId": "task-fail"},
                  "harness": {"name": "xero"},
                  "run": {"status": "completed"}
                }"""
            )
            reward_file = root / "task-fail__def" / "verifier" / "reward.txt"
            reward_file.parent.mkdir()
            reward_file.write_text("0\n")
            ctrf_file = root / "task-fail__def" / "verifier" / "ctrf.json"
            ctrf_file.write_text(
                """{
                  "results": {
                    "summary": {"tests": 1, "passed": 0, "failed": 1},
                    "tests": [{
                      "name": "test_outputs.py::test_expected_file",
                      "status": "failed",
                      "message": "The test failed in the call phase",
                      "trace": "AssertionError: expected only main.py.c"
                    }]
                  }
                }"""
            )

            manifests = load_manifests(root)
            report = summarize(manifests)

        by_task = {row["benchmark"]["taskId"]: row for row in manifests}
        self.assertEqual(by_task["task-pass"]["verifier"]["source"], "harbor_result")
        self.assertIs(by_task["task-pass"]["verifier"]["resolved"], True)
        self.assertEqual(by_task["task-fail"]["verifier"]["source"], "harbor_reward_file")
        self.assertIs(by_task["task-fail"]["verifier"]["resolved"], False)
        self.assertEqual(report["harnesses"]["xero"]["successes"], 1)
        self.assertEqual(report["harnesses"]["xero"]["missingVerifierOutcomes"], 0)
        self.assertEqual(
            report["harnesses"]["xero"]["failureCategories"],
            {"verifier_failed": 1},
        )
        self.assertEqual(
            report["pairedOutcomes"]["task-fail"]["xero"]["failureCategory"],
            "verifier_failed",
        )
        self.assertEqual(
            report["pairedOutcomes"]["task-fail"]["xero"]["verifierSummary"],
            {"tests": 1, "passed": 0, "failed": 1},
        )
        self.assertEqual(
            report["pairedOutcomes"]["task-fail"]["xero"]["firstVerifierFailure"]["name"],
            "test_outputs.py::test_expected_file",
        )


if __name__ == "__main__":
    unittest.main()
