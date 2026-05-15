import tempfile
import unittest
from pathlib import Path

from benchmarks.cleanup import cleanup_benchmark_storage, remove_tree


class CleanupTests(unittest.TestCase):
    def test_remove_tree_handles_read_only_benchmark_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "run-root"
            nested = root / "xero-oauth-readable"
            nested.mkdir(parents=True)
            db = nested / "xero.db"
            db.write_text("copy")
            db.chmod(0o444)
            nested.chmod(0o555)

            result = remove_tree(root)

        self.assertEqual(result["status"], "passed")
        self.assertFalse(root.exists())

    def test_cleanup_prunes_docker_and_preserves_results_by_default(self):
        calls: list[list[str]] = []

        def fake_runner(argv):
            calls.append(argv)
            return {"status": "passed", "command": argv}

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "run-root"
            root.mkdir()

            result = cleanup_benchmark_storage(
                run_root=root,
                clean_tool_cache=False,
                command_runner=fake_runner,
            )

            self.assertEqual(result["status"], "passed")
            self.assertTrue(root.exists())

        self.assertEqual(
            calls,
            [
                ["docker", "builder", "prune", "-af"],
                ["docker", "system", "prune", "-af"],
            ],
        )

    def test_cleanup_removes_regenerated_tool_caches_when_enabled(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            uv_archive = home / ".cache" / "uv" / "archive-v0"
            harbor_tasks = home / ".cache" / "harbor" / "tasks"
            uv_archive.mkdir(parents=True)
            harbor_tasks.mkdir(parents=True)
            (uv_archive / "payload").write_text("uv")
            (harbor_tasks / "payload").write_text("harbor")

            result = cleanup_benchmark_storage(
                clean_docker=False,
                clean_tool_cache=True,
                home=home,
            )

            self.assertEqual(result["status"], "passed")
            self.assertFalse(uv_archive.exists())
            self.assertFalse(harbor_tasks.exists())


if __name__ == "__main__":
    unittest.main()
