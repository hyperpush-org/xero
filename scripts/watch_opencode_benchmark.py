#!/usr/bin/env python3
from __future__ import annotations

import datetime as dt
import json
import pathlib
import sys
import time


DEFAULT_JOB = pathlib.Path(
    "/tmp/xero-terminal-bench-full-opencode-openai-medium-c3-detached-20260514"
    "/jobs/opencode-openai-gpt55-medium-c3-full-k1-20260514"
)


def load_result(job: pathlib.Path) -> dict:
    return json.loads((job / "result.json").read_text())


def reward_counts(result: dict) -> tuple[int, int, float | None]:
    stats = result["stats"]
    evals = stats.get("evals") or {}
    if not evals:
        return 0, 0, None
    first_eval = next(iter(evals.values()))
    rewards = (first_eval.get("reward_stats") or {}).get("reward") or {}
    metrics = first_eval.get("metrics") or [{}]
    return len(rewards.get("1.0", [])), len(rewards.get("0.0", [])), metrics[0].get("mean")


def print_status(job: pathlib.Path) -> None:
    if not (job / "result.json").exists():
        print("waiting for result.json")
        return

    result = load_result(job)
    stats = result["stats"]
    passed, failed, mean = reward_counts(result)
    finished = "done" if result.get("finished_at") else "running"
    now = dt.datetime.now().strftime("%H:%M:%S")
    print(
        f"{now} {finished} | "
        f"{stats['n_completed_trials']}/{result['n_total_trials']} tasks | "
        f"pass {passed} fail {failed} err {stats['n_errored_trials']} | "
        f"running {stats['n_running_trials']} pending {stats['n_pending_trials']} "
        f"retries {stats['n_retries']} | mean {mean}",
        flush=True,
    )


def main() -> int:
    job = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_JOB
    interval = int(sys.argv[2]) if len(sys.argv) > 2 else 30
    while True:
        print_status(job)
        time.sleep(interval)


if __name__ == "__main__":
    raise SystemExit(main())
