import unittest

from benchmarks.report import summarize, wilson


def manifest(harness: str, task: str, status: str, wall: int = 1000):
    return {
        "benchmark": {
            "name": "terminal-bench",
            "datasetId": "terminal-bench@2.0",
            "taskId": task,
        },
        "harness": {"name": harness},
        "model": {"provider": "openai_api", "modelId": "gpt-5.4"},
        "run": {"status": status},
        "verifier": {"resolved": status == "completed"},
        "metrics": {"wallTimeMs": wall},
        "_manifestPath": f"/tmp/{harness}/{task}/manifest.json",
    }


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


if __name__ == "__main__":
    unittest.main()
