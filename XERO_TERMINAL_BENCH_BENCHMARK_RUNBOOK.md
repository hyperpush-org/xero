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

Status: Completed on 2026-05-14 in product-mode route, then rerun after the path-policy and read-only OAuth-store patches. The frozen comparison-smoke task ids were `log-summary-date-ranges`, `fix-git`, `cobol-modernization`, `db-wal-recovery`, and `polyglot-c-py`. Harbor oracle validated the set with 5/5 trials complete, zero exceptions, and mean reward 1.000. Harbor built-in `opencode` then ran the same ids with `opencode/gpt-5.5`, OpenCode CLI `1.14.50`, one attempt each, zero exceptions, and mean reward 0.000. Xero's first post-patch rerun with `openai_codex`/`gpt-5.5` had mean reward 0.600, with `db-wal-recovery` and `polyglot-c-py` verifier failures. After prompt v2 benchmark-hygiene guidance, Xero reran the same five task ids with zero exceptions and mean reward 0.800, passing `cobol-modernization`, `db-wal-recovery`, `fix-git`, and `polyglot-c-py`; only `log-summary-date-ranges` failed verifier counts. Every Xero trial emitted `manifest.json`, `trajectory.json`, `xero-trace.json`, `final.diff`, `support-bundle.zip`, `stdout.txt`, and `stderr.txt`; no manifest pointed at legacy `.xero` state. Result roots: `/tmp/xero-terminal-bench-rerun-20260514/jobs/xero-gpt55-comparison-smoke-rerun2-20260514` for the 0.600 rerun and `/tmp/xero-terminal-bench-smoke-v2-20260514/jobs/xero-gpt55-promptv2-same-smoke-20260514` for the prompt v2 rerun.

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

Status: Completed on 2026-05-14, then repeated against the post-patch rerun under `/tmp/xero-terminal-bench-rerun-20260514/jobs`.

Findings:

- Adapter/runtime basics were sound in the post-patch rerun: oracle, OpenCode, and Xero all ran the same five task ids, the same Terminal-Bench git commit and task checksums, one attempt each, Docker sandboxing, and zero Harbor exceptions.
- Xero artifact contract was complete and parseable for all five trials: `manifest.json`, `trajectory.json`, `xero-trace.json`, `final.diff`, `support-bundle.zip`, `stdout.txt`, and `stderr.txt` were present; support bundles opened successfully; no trial-created `.xero/` directory was found.
- Harness route matched the intended GPT-5.5 product-mode comparison: OpenCode used `opencode/gpt-5.5`, and Xero used `openai_codex`/`gpt-5.5` with app-data OAuth. A stale Xero capability default that still reported `gpt-5.4` in provider metadata was fixed so future artifacts advertise `gpt-5.5`.
- The first rerun after path-policy patches exposed a real container credential root cause: SQLite could open the app-data OAuth database on a read-only bind mount, but preparing the provider credential query failed with `unable to open database file`. This was fixed in `xero-cli` by opening the benchmark OAuth store through a read-only immutable SQLite URI (`mode=ro&immutable=1`). A container-level OAuth read smoke then progressed past SQLite access.
- The previous tool-policy friction did not recur: post-patch Xero traces contained no `agent_core_headless_path_denied`, `agent_core_headless_path_protected`, `agent_sandbox_path_denied`, `agent_sandbox_network_denied`, or `agent_sandbox_write_denied` markers.
- The report generator was fixed to join Harbor verifier artifacts from each trial's `result.json` or `verifier/reward.txt` into Xero manifests before summarizing. The regenerated engineering report shows Xero pass@1 `0.600`, 3/5 successes, zero missing verifier outcomes, and `verifier_failed: 2`.
- Triage of the two Xero verifier failures found harness-quality risks that can depress benchmark score even when the adapter is healthy. `polyglot-c-py` failed because Xero verified C compilation in place and left `/app/polyglot/cmain`; the verifier required only `main.py.c` in that directory. `db-wal-recovery` failed because Xero opened/probed the SQLite database before preserving the WAL evidence, then produced guessed records with stale WAL-updated values.
- Prompt version `xero-terminal-bench-prompt.v2` adds benchmark hygiene guidance: use scratch locations such as `/tmp` for build outputs and verification debris, remove temporary workspace files before finishing, and copy fragile recovery inputs before probing them.
- Artifact/reporting quality was improved for future triage: non-git workspaces now get a final file listing fallback instead of an empty `git diff` failure, and reports include the first verifier failure summary from `verifier/ctrf.json`.
- Prompt v2 rerun evidence supports the harness-quality fix: `db-wal-recovery` passed after the fragile-input guidance, and `polyglot-c-py` passed with the final file listing showing only `polyglot/main.py.c` and no `polyglot/cmain` debris.
- A new fast/easy verifier smoke avoided the HTML/filter tasks and used `vulnerable-secret`, `openssl-selfsigned-cert`, and `regex-log`. Oracle validated the set with 3/3 complete, zero exceptions, and mean reward 1.000 under `/tmp/xero-terminal-bench-smoke-v2-20260514/jobs/oracle-alt-fast-smoke-20260514`. Xero then ran the same ids with `openai_codex`/`gpt-5.5`, prompt v2, zero exceptions, and mean reward 1.000 under `/tmp/xero-terminal-bench-smoke-v2-20260514/jobs/xero-gpt55-promptv2-alt-fast-smoke-20260514`.
- Remaining score failure in the prompt v2 five-task rerun is a verifier outcome, not an adapter failure: `log-summary-date-ranges` completed but counted `today,ERROR` as `414` instead of the expected `370`, likely because it counted severity words outside the intended severity field.

Gate outcome: the 5-task comparison smoke is clean enough to trust the benchmark pipeline. Decide whether to broaden the task set or run the full benchmark based on evaluation budget and the expected variance from only five smoke tasks.

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

Use the repo launcher to generate Harbor configs instead of hand-writing `/tmp/full-config.json`. The launcher pins the OpenCode GPT-5.5 medium route, applies the prewarmed OpenCode wrapper, mounts the logged-in OpenCode auth store, and carries the benchmark hygiene policy that prevented the failed full-run attempt from being clean:

- Retry infrastructure/transport failures only: `EnvironmentStartTimeoutError` and `NonZeroAgentExitCodeError`.
- Do not retry agent/verifier outcome failures: `AgentTimeoutError`, `VerifierTimeoutError`, reward-file errors, or verifier parse errors.
- Give slow Docker starts and model runs explicit room with `environmentBuildTimeoutMultiplier=3`, `agentSetupTimeoutMultiplier=2`, and `agentTimeoutMultiplier=2`.
- Apply the same timeout/retry policy to Xero and OpenCode when results are compared, and disclose the multipliers in any shareable result.

Generate, review, and optionally launch the OpenCode full-run config:

```sh
python3 scripts/run_opencode_benchmark.py \
  --config benchmarks/config/terminal_bench_opencode_smoke.json \
  --task-set full-terminal-bench-2 \
  --concurrency 3
```

Add `--detach` only when the generated config looks right and the machine is ready to spend the run.

Every benchmark run must be followed by storage cleanup. Prefer launching with `--run` or `--detach`, because cleanup is enabled on exit by default for both modes. The cleanup step preserves the benchmark result directory unless `--delete-run-root-after-cleanup` is passed, but it prunes inactive Docker build/image/network debris and clears regenerated `uv`/Harbor task caches. If a run is started another way, run cleanup manually after the verifier/report artifacts have been captured:

```sh
python3 scripts/clean_benchmark_storage.py \
  --run-root /tmp/path-to-benchmark-run \
  --clean-tool-cache
```

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
6. [x] Generate the engineering report.
7. [x] Triage failures.
8. [ ] Decide whether to run the full benchmark.
