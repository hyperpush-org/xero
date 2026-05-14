# Xero Terminal-Bench Benchmark Runbook

Reader: internal Xero engineer or evaluation owner.

Post-read action: run Xero against OpenCode on Terminal-Bench safely, starting with smoke validation and only moving to paid/full runs after each gate passes.

## Position

Xero is ready for a real smoke benchmark path, not yet for a confident full-result claim.

The goal of the first runs is to prove that the benchmark adapter, Harbor invocation, Xero headless owned-agent runtime, artifact contract, and report generator all work under the same Terminal-Bench task setup used for OpenCode.

Do not start with the full benchmark. Start small, freeze task ids before running, and treat every failure as useful signal.

## Run Order

### 1. Preflight

Status: Completed on 2026-05-14. Fresh preflight passed after fixing Harbor OpenCode detection; Harbor selected built-in `opencode`, Docker was usable, Xero CLI reported a version, OpenAI OAuth app-data credentials were present, and the fake-provider fixture wrote the expected artifacts.

Purpose: catch setup problems before spending model tokens.

Run preflight from the repo root and require it to pass before any paid model run.

Checks that matter:

- `python3` is available.
- `protoc` is available.
- Harbor can be discovered through `uvx harbor run --help`.
- Docker or the selected sandbox provider is usable.
- Xero CLI is available and reports a version.
- OpenCode is available through Harbor built-in support, or the fallback adapter path is clearly labeled.
- Provider credentials are present: API-key environment variables for API-key routes, or the app-data OpenAI OAuth store for `openai_codex`.
- Trial state and output roots are outside legacy `.xero/` state.
- Optional Xero fake-provider fixture writes the expected artifacts.

Gate: all required preflight checks pass. If `opencode` is missing from Harbor, decide whether to use the labeled fallback before continuing.

For Xero runs that should use the already logged-in OpenAI OAuth session, select the `openai_codex` provider path:

```sh
export XERO_PROVIDER_ID=openai_codex
export XERO_OPENAI_OAUTH_APP_DATA_ROOT="$HOME/Library/Application Support/dev.sn0w.xero"
# Optional when more than one OpenAI account is present:
export XERO_OPENAI_OAUTH_ACCOUNT_ID="acct_..."
```

This uses the app-local `xero.db` OAuth session and does not require `OPENAI_API_KEY`. Keep the manifest labels: `credentialMode=app_openai_oauth` and `endpointClass=chatgpt-codex-oauth`.

### 2. Adapter Smoke

Status: Completed on 2026-05-14. The adapter smoke fixture completed with all required artifacts present, `fakeProviderFixture` set to `true`, and no benchmark state written to legacy `.xero/` state.

Purpose: prove Xero plumbing without judging model quality.

Use one or two cheap Terminal-Bench tasks, or a fixture task, with the Xero fake-provider fixture where appropriate. This verifies that Xero can:

- Register a trial-local app-data project.
- Avoid writing benchmark state to `.xero/`.
- Emit `manifest.json`, `trajectory.json`, `xero-trace.json`, `final.diff`, `support-bundle.zip`, `stdout.txt`, and `stderr.txt`.
- Preserve the fake-provider label so fixture runs cannot be mixed into score tables.

Gate: artifacts are complete, manifests label fake-provider runs as fixture-only, and no state lands in `.xero/`.

### 3. Oracle Smoke

Status: Completed on 2026-05-14. Harbor oracle completed the frozen adapter-smoke task ids `break-filter-js-from-html` and `log-summary-date-ranges` with 2/2 trials complete, zero exceptions, and mean reward 1.000.

Purpose: confirm the chosen tasks and environment are valid before comparing agents.

Run Harbor oracle on the frozen smoke task ids. Harbor remains the task materialization, sandbox, verifier, and scoring authority.

Gate: oracle can complete the selected smoke tasks, or failures are understood as task/environment issues rather than Xero/OpenCode issues.

### 4. OpenCode Smoke

Status: Completed on 2026-05-14 in product-mode route. Harbor built-in `opencode` ran the frozen adapter-smoke task ids `break-filter-js-from-html` and `log-summary-date-ranges` with `opencode/gpt-5.5` and OpenCode CLI `1.14.50`. Both trials produced Harbor-owned verifier outcomes with zero harness exceptions; verifier reward was 0.0 for both tasks, so this is model-quality signal rather than an adapter/runtime failure. The fixed-model OpenAI API route remains unrun until `OPENAI_API_KEY` is available.

Purpose: establish the baseline under the same Harbor task set and model route.

Prefer Harbor built-in `opencode`. Use the fallback wrapper only when the installed Harbor version lacks built-in support, and label fallback results separately.

Hold constant:

- Dataset id and resolved version.
- Task ids.
- Attempt count.
- Model provider, model id, credential mode, and endpoint class.
- Temperature, reasoning effort, output limit, and context budget where supported.
- Wall time and cost limits.
- Sandbox and network policy.

Gate: OpenCode produces Harbor-owned outcomes and stored artifacts for the frozen smoke task ids.

### 5. Xero Comparison Smoke

Status: Completed on 2026-05-14 in product-mode route. The frozen comparison-smoke task ids were `log-summary-date-ranges`, `fix-git`, `cobol-modernization`, `db-wal-recovery`, and `polyglot-c-py`. Harbor oracle validated the set with 5/5 trials complete, zero exceptions, and mean reward 1.000. Harbor built-in `opencode` then ran the same ids with `opencode/gpt-5.5`, OpenCode CLI `1.14.50`, one attempt each, zero exceptions, and mean reward 0.000. Xero ran the same ids with `openai_codex`/`gpt-5.5`, app-data OAuth credentials, one attempt each, zero exceptions, and mean reward 0.800. Every Xero trial emitted `manifest.json`, `trajectory.json`, `xero-trace.json`, `final.diff`, `support-bundle.zip`, `stdout.txt`, and `stderr.txt`; no manifest pointed at legacy `.xero` state. The only Xero verifier failure was `db-wal-recovery`, categorized as model-quality/verifier-failed because the agent completed and Harbor's verifier returned reward 0.0. Result roots: `/tmp/xero-terminal-bench-step5/jobs/oracle-comparison-smoke-v2-20260514`, `/tmp/xero-terminal-bench-step5/jobs/opencode-gpt55-comparison-smoke-20260514`, and `/tmp/xero-terminal-bench-step5/jobs/xero-gpt55-comparison-smoke-20260514`.

Purpose: run the first real Xero scoring path against the same tasks.

Use the Xero Harbor installed-agent adapter and real provider mode. The run should use Xero's headless owned-agent path with app-data project state and Tool Registry V2 capabilities, including file operations, patch application, and bounded command execution.

The smoke set should contain 5 to 10 predeclared tasks covering:

- Simple terminal/control flow.
- Git repair.
- Code edit.
- Filesystem or database recovery.
- Polyglot or compiled-language work.

Gate: Xero and OpenCode ran the same task ids and attempts, every Xero trial has a manifest and trace, and every failure has a category.

## Triage After Smoke

Before any full run, cold-read the smoke results.

Look for:

- Adapter failures: Xero never started, CLI not found, missing artifacts, bad import path.
- Runtime failures: provider preflight blocked, limits exceeded too early, missing command or patch capability.
- Tool-policy failures: commands denied unexpectedly, writes blocked inside the workspace, network policy mismatch.
- Model-quality failures: agent completed but verifier failed.
- Harness mismatches: OpenCode and Xero did not run the same task, model, limits, or sandbox.

Fix adapter/runtime/harness failures before running more tasks. Model-quality failures can proceed to broader comparison only if artifacts are complete and the failure is genuine.

## Full Benchmark

Run the full Terminal-Bench 2.0 comparison only after comparison-smoke is clean enough to trust the pipeline.

For a public or leaderboard-adjacent claim, require:

- Full dataset or explicitly named task set.
- Pass@1 as the primary metric.
- Same model route and same attempts for Xero and OpenCode.
- Wilson confidence interval.
- Paired per-task Xero/OpenCode outcome table.
- Mean and p95 wall time.
- Mean and p95 cost.
- Mean and p95 input/output tokens when available.
- Failure category distribution.
- Redacted artifacts before sharing outside local/internal storage.

Never publish a success rate without model version, dataset version, harness version, and task count.

## Good-Result Expectation

Expect the first smoke to be informative, not flattering.

A good first outcome is:

- Preflight passes.
- Adapter smoke writes complete artifacts.
- OpenCode and Xero both run the same frozen smoke tasks.
- Xero failures are categorized and debuggable.
- At least some Xero tasks reach verifier-owned outcomes rather than failing in setup.

Do not expect strong full-benchmark results until comparison-smoke shows the headless prompt/tool loop behaves well on real Terminal-Bench tasks.

## Stop Conditions

Stop and fix before continuing if:

- Any trial writes under `.xero/`.
- Fake-provider fixture results appear in a score table.
- Xero real-provider runs use the harness JSON store instead of app-data project state.
- Harbor does not own verifier execution.
- Xero and OpenCode use different task ids, model routes, attempts, limits, or sandbox policy.
- Artifacts are missing or cannot be parsed by the report generator.
- Secret values appear in logs, manifests, or support bundles.

## Reporting

Generate reports from stored artifacts only. Do not recompute outcomes by reading the workspace after the fact.

The engineering report should include:

- Config snapshot.
- Preflight manifest.
- Per-task manifest table.
- Xero/OpenCode paired outcomes.
- Pass@1 with confidence interval.
- Cost, time, and token summaries.
- Failure categories.
- Artifact pointers.
- Notes for invalid or flaky trials.

The shareable summary should be shorter and clearly labeled as either fixed-model mode or product mode.

## Recommended Sequence

1. [x] Run preflight.
2. [x] Run adapter smoke.
3. [x] Run oracle smoke.
4. [x] Run OpenCode smoke.
5. [x] Run Xero comparison smoke.
6. [ ] Generate the engineering report.
7. [ ] Triage failures.
8. [ ] Decide whether to run the full benchmark.
