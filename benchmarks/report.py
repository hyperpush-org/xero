#!/usr/bin/env python3
"""Generate Xero/OpenCode benchmark reports from stored manifests only."""

from __future__ import annotations

import argparse
import json
import math
from collections import defaultdict
from datetime import datetime, timezone
from pathlib import Path
from statistics import mean
from typing import Any


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
            if not resolved(row):
                if not row.get("verifier"):
                    failures["verifier_missing"] += 1
                else:
                    failures[
                        ((row.get("run") or {}).get("failureCategory") or "verifier_failed")
                    ] += 1
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
            "failureCategory": (manifest.get("run") or {}).get("failureCategory"),
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
