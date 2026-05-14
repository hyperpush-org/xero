#!/usr/bin/env python3
"""Generate Xero/OpenCode benchmark reports from stored artifacts only."""

from __future__ import annotations

import argparse
import json
import math
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path
from statistics import mean
from typing import Any

MAX_FAILURE_TRACE_CHARS = 2_000


def _coerce_reward(value: Any) -> float | None:
    try:
        reward = float(value)
    except (TypeError, ValueError):
        return None
    if math.isnan(reward) or math.isinf(reward):
        return None
    return reward


def _reward_from_harbor_result(result_path: Path) -> float | None:
    try:
        payload = json.loads(result_path.read_text())
    except (OSError, json.JSONDecodeError):
        return None
    if not isinstance(payload, dict):
        return None

    verifier_result = payload.get("verifier_result")
    if not isinstance(verifier_result, dict):
        return None
    rewards = verifier_result.get("rewards")
    if not isinstance(rewards, dict):
        return None
    return _coerce_reward(rewards.get("reward"))


def _reward_from_harbor_reward_file(reward_path: Path) -> float | None:
    try:
        return _coerce_reward(reward_path.read_text().strip())
    except OSError:
        return None


def _verifier_test_details(trial_dir: Path) -> dict[str, Any]:
    ctrf_path = trial_dir / "verifier" / "ctrf.json"
    try:
        payload = json.loads(ctrf_path.read_text())
    except (OSError, json.JSONDecodeError):
        return {}
    results = payload.get("results")
    if not isinstance(results, dict):
        return {}
    details: dict[str, Any] = {"sourcePath": str(ctrf_path)}
    summary = results.get("summary")
    if isinstance(summary, dict):
        details["summary"] = {
            key: summary.get(key)
            for key in ("tests", "passed", "failed", "skipped", "pending", "other")
            if summary.get(key) is not None
        }
    tests = results.get("tests")
    if isinstance(tests, list):
        first_failure = next(
            (
                test
                for test in tests
                if isinstance(test, dict)
                and str(test.get("status") or test.get("raw_status") or "").lower()
                in {"failed", "call_failed", "error"}
            ),
            None,
        )
        if first_failure:
            trace = first_failure.get("trace")
            if isinstance(trace, str) and len(trace) > MAX_FAILURE_TRACE_CHARS:
                trace = trace[:MAX_FAILURE_TRACE_CHARS] + "..."
            details["firstFailure"] = {
                "name": first_failure.get("name"),
                "status": first_failure.get("status"),
                "message": first_failure.get("message"),
                "trace": trace,
            }
    return details


def _attach_harbor_verifier(manifest: dict[str, Any], path: Path) -> None:
    if manifest.get("verifier"):
        return

    trial_dir = path.parent.parent if path.parent.name == "agent" else path.parent
    result_path = trial_dir / "result.json"
    reward = _reward_from_harbor_result(result_path)
    source_path = result_path
    source = "harbor_result"
    if reward is None:
        reward_path = trial_dir / "verifier" / "reward.txt"
        reward = _reward_from_harbor_reward_file(reward_path)
        source_path = reward_path
        source = "harbor_reward_file"
    if reward is None:
        return

    manifest["verifier"] = {
        "source": source,
        "sourcePath": str(source_path),
        "reward": reward,
        "resolved": math.isclose(reward, 1.0),
    }
    verifier_tests = _verifier_test_details(trial_dir)
    if verifier_tests:
        manifest["verifier"]["tests"] = verifier_tests


def load_manifests(root: Path) -> list[dict[str, Any]]:
    manifests: list[dict[str, Any]] = []
    for path in sorted(root.rglob("manifest.json")):
        try:
            payload = json.loads(path.read_text())
        except (OSError, json.JSONDecodeError):
            continue
        if not isinstance(payload, dict):
            continue
        if not isinstance(payload.get("benchmark"), dict) or not isinstance(
            payload.get("harness"), dict
        ):
            continue
        _attach_harbor_verifier(payload, path)
        payload["_manifestPath"] = str(path)
        manifests.append(payload)
    return manifests


def percentile(values: list[float], p: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    index = math.ceil((p / 100) * len(ordered)) - 1
    return ordered[max(0, min(index, len(ordered) - 1))]


def wilson(successes: int, total: int, z: float = 1.96) -> dict[str, float | None]:
    if total == 0:
        return {"low": None, "high": None}
    phat = successes / total
    denom = 1 + z * z / total
    centre = phat + z * z / (2 * total)
    margin = z * math.sqrt((phat * (1 - phat) + z * z / (4 * total)) / total)
    return {"low": (centre - margin) / denom, "high": (centre + margin) / denom}


def resolved(manifest: dict[str, Any]) -> bool:
    verifier = manifest.get("verifier") or {}
    if isinstance(verifier.get("resolved"), bool):
        return verifier["resolved"]
    if isinstance(verifier.get("passed"), bool):
        return verifier["passed"]
    return False


def agent_completed(manifest: dict[str, Any]) -> bool:
    run = manifest.get("run") or {}
    return run.get("status") == "completed"


def harness_name(manifest: dict[str, Any]) -> str:
    harness = manifest.get("harness") or {}
    name = harness.get("name") or "unknown"
    if name == "opencode" and harness.get("integrationPath"):
        return f"opencode:{harness['integrationPath']}"
    return name


def excluded_manifest_reason(manifest: dict[str, Any]) -> str | None:
    harness = manifest.get("harness") or {}
    if harness.get("fakeProviderFixture") is True:
        return "fake_provider_fixture"
    return None


def failure_category(manifest: dict[str, Any]) -> str | None:
    if resolved(manifest):
        return None
    if not manifest.get("verifier"):
        return "verifier_missing"
    return (manifest.get("run") or {}).get("failureCategory") or "verifier_failed"


def summarize(manifests: list[dict[str, Any]]) -> dict[str, Any]:
    scored_manifests = [
        manifest for manifest in manifests if excluded_manifest_reason(manifest) is None
    ]
    excluded_manifests = [
        {
            "reason": excluded_manifest_reason(manifest),
            "harness": harness_name(manifest),
            "taskId": (manifest.get("benchmark") or {}).get("taskId"),
            "manifest": manifest.get("_manifestPath"),
        }
        for manifest in manifests
        if excluded_manifest_reason(manifest) is not None
    ]

    by_harness: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for manifest in scored_manifests:
        by_harness[harness_name(manifest)].append(manifest)

    harnesses: dict[str, Any] = {}
    for harness, rows in sorted(by_harness.items()):
        successes = sum(1 for row in rows if resolved(row))
        completions = sum(1 for row in rows if agent_completed(row))
        missing_verifier = sum(1 for row in rows if not row.get("verifier"))
        wall = [
            float((row.get("metrics") or {}).get("wallTimeMs"))
            for row in rows
            if (row.get("metrics") or {}).get("wallTimeMs") is not None
        ]
        cost = [
            float((row.get("metrics") or {}).get("costUsd"))
            for row in rows
            if (row.get("metrics") or {}).get("costUsd") is not None
        ]
        input_tokens = [
            float((row.get("metrics") or {}).get("inputTokens"))
            for row in rows
            if (row.get("metrics") or {}).get("inputTokens") is not None
        ]
        output_tokens = [
            float((row.get("metrics") or {}).get("outputTokens"))
            for row in rows
            if (row.get("metrics") or {}).get("outputTokens") is not None
        ]
        failures: dict[str, int] = defaultdict(int)
        for row in rows:
            category = failure_category(row)
            if category:
                failures[category] += 1
        harnesses[harness] = {
            "taskCount": len(rows),
            "successes": successes,
            "passAt1": successes / len(rows) if rows else None,
            "wilson95": wilson(successes, len(rows)),
            "agentCompletions": completions,
            "agentCompletionRate": completions / len(rows) if rows else None,
            "missingVerifierOutcomes": missing_verifier,
            "meanWallTimeMs": mean(wall) if wall else None,
            "p95WallTimeMs": percentile(wall, 95),
            "meanCostUsd": mean(cost) if cost else None,
            "p95CostUsd": percentile(cost, 95),
            "meanInputTokens": mean(input_tokens) if input_tokens else None,
            "p95InputTokens": percentile(input_tokens, 95),
            "meanOutputTokens": mean(output_tokens) if output_tokens else None,
            "p95OutputTokens": percentile(output_tokens, 95),
            "failureCategories": dict(sorted(failures.items())),
        }

    paired: dict[str, dict[str, Any]] = defaultdict(dict)
    for manifest in scored_manifests:
        bench = manifest.get("benchmark") or {}
        task_id = bench.get("taskId") or "unknown-task"
        paired[task_id][harness_name(manifest)] = {
            "resolved": resolved(manifest),
            "manifest": manifest.get("_manifestPath"),
            "failureCategory": failure_category(manifest),
            "verifierReward": (manifest.get("verifier") or {}).get("reward"),
            "verifierSummary": ((manifest.get("verifier") or {}).get("tests") or {}).get(
                "summary"
            ),
            "firstVerifierFailure": ((manifest.get("verifier") or {}).get("tests") or {}).get(
                "firstFailure"
            ),
        }

    first = scored_manifests[0] if scored_manifests else (manifests[0] if manifests else {})
    return {
        "schema": "xero.benchmark.report.v1",
        "generatedAt": datetime.now(timezone.utc).isoformat(),
        "benchmark": (first.get("benchmark") or {}).get("name"),
        "datasetId": (first.get("benchmark") or {}).get("datasetId"),
        "model": first.get("model"),
        "harnesses": harnesses,
        "pairedOutcomes": dict(sorted(paired.items())),
        "manifestCount": len(manifests),
        "scoredManifestCount": len(scored_manifests),
        "excludedManifestCount": len(excluded_manifests),
        "excludedManifests": excluded_manifests,
    }


def markdown_report(report: dict[str, Any]) -> str:
    lines = [
        "# Terminal-Bench Comparison Report",
        "",
        f"Generated: {report['generatedAt']}",
        f"Dataset: {report.get('datasetId') or 'unknown'}",
        "",
        "## Harness Summary",
        "",
        "| Harness | Tasks | Pass@1 | 95% CI | Mean Wall ms | Mean Cost |",
        "| --- | ---: | ---: | --- | ---: | ---: |",
    ]
    for harness, summary in report["harnesses"].items():
        ci = summary["wilson95"]
        ci_text = (
            "n/a"
            if ci["low"] is None
            else f"{ci['low']:.3f}-{ci['high']:.3f}"
        )
        pass_at_1 = summary["passAt1"]
        lines.append(
            "| {harness} | {tasks} | {pass_at_1} | {ci} | {wall} | {cost} |".format(
                harness=harness,
                tasks=summary["taskCount"],
                pass_at_1="n/a" if pass_at_1 is None else f"{pass_at_1:.3f}",
                ci=ci_text,
                wall="n/a"
                if summary["meanWallTimeMs"] is None
                else f"{summary['meanWallTimeMs']:.0f}",
                cost="n/a"
                if summary["meanCostUsd"] is None
                else f"{summary['meanCostUsd']:.4f}",
            )
        )
    lines.extend(["", "## Paired Outcomes", ""])
    for task_id, outcomes in report["pairedOutcomes"].items():
        labels = ", ".join(
            f"{name}={'pass' if row['resolved'] else 'fail'}" for name, row in outcomes.items()
        )
        lines.append(f"- `{task_id}`: {labels}")
        for name, row in outcomes.items():
            failure = row.get("firstVerifierFailure")
            if not failure:
                continue
            failure_name = failure.get("name") or "unknown verifier test"
            message = failure.get("message") or row.get("failureCategory") or "failed"
            lines.append(f"  - `{name}` verifier: {failure_name}: {message}")
    if report.get("excludedManifests"):
        lines.extend(["", "## Excluded Manifests", ""])
        for row in report["excludedManifests"]:
            lines.append(
                f"- `{row.get('taskId') or 'unknown-task'}` ({row.get('harness')}): {row.get('reason')}"
            )
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifacts-root", type=Path, required=True)
    parser.add_argument("--output-json", type=Path, required=True)
    parser.add_argument("--output-md", type=Path)
    args = parser.parse_args()

    report = summarize(load_manifests(args.artifacts_root))
    args.output_json.parent.mkdir(parents=True, exist_ok=True)
    args.output_json.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if args.output_md:
        args.output_md.parent.mkdir(parents=True, exist_ok=True)
        args.output_md.write_text(markdown_report(report))
    print(json.dumps({"manifestCount": report["manifestCount"], "output": str(args.output_json)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
