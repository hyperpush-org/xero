# Agent Harness Benchmarking Implementation Plan

Reader: an internal Xero engineer or evaluation owner.

Post-read action: implement a reproducible benchmark path that compares Xero's owned-agent harness against OpenCode, ForgeCode, and other coding harnesses under the same model, task set, limits, and scoring rules.

Last researched: 2026-05-05.

## Decision

Use Harbor plus Terminal-Bench 2.0 as the first external benchmark integration. It is the best fit for Xero because it evaluates agents in terminal-oriented, sandboxed tasks and records verifier output, trajectories, and job artifacts. Use SWE-bench next when the question is patch quality on real software issues. Keep the existing Xero quality eval suite as a fast preflight, but do not treat it as public benchmark evidence.

The benchmark claim must always separate two modes:

- Fixed-model harness comparison: Xero, OpenCode, ForgeCode, and other baselines run the same model, benchmark version, task ids, time limit, cost limit, provider endpoint, and attempt count.
- Product-mode comparison: each harness runs its recommended setup. This is useful for positioning, but it is not an apples-to-apples harness comparison.

For the first milestone, optimize for a small honest run over leaderboard ambition: 5 to 10 Terminal-Bench tasks, 5 to 10 SWE-style tasks, the same model for every harness, full artifacts, and a report that explains every failure category.

## Current External Ground Truth

Harbor is the official runner for Terminal-Bench 2.0. The current Harbor docs show Terminal-Bench runs through the published dataset package and custom agents through an importable agent class. Harbor also stores run output as jobs with per-trial config, result, trajectory, and verifier artifacts.

Harbor's public docs list built-in integrations such as Terminus-2, Claude Code, Codex CLI, Gemini CLI, OpenHands, and Mini-SWE-Agent. The Harbor repository's own agent guidance also lists additional installed agents, including OpenCode, Aider, SWE-agent, Cursor CLI, and others. Treat the exact installed-agent list as versioned toolchain state: discover it with `harbor run --help` during preflight and record the result in every benchmark manifest.

The Harbor examples currently appear in two dataset naming styles: `terminal-bench/terminal-bench-2` in the hosted docs, and `terminal-bench@2.0` in some repository guidance and third-party examples. Xero's benchmark runner should support a configurable dataset id and store the resolved dataset package/version in the run manifest instead of hardcoding one spelling everywhere.

SWE-bench's official evaluator expects patch predictions and grades them in Docker. For Xero, SWE-bench support should be a patch-export adapter around the same headless run contract used by Harbor, not a separate agent implementation.

OpenCode has first-class CLI and non-interactive modes, including `opencode run`, headless server/web modes, and an ACP server mode. Prefer Harbor's built-in OpenCode integration when available; otherwise wrap the official non-interactive CLI.

ForgeCode is a CLI-based coding harness with Zsh-oriented interactive usage, config isolation through `FORGE_CONFIG`, MCP support, custom agents, and stdin prompting. Because ForgeCode is not guaranteed to be a built-in Harbor agent in the current Harbor docs, treat ForgeCode support as an installed-agent wrapper that must pass a smoke test before it is included in comparisons.

## What Xero Must Build

The benchmark system has four parts.

The first part is a headless Xero agent runner. It must drive the same owned-agent runtime as the desktop app without opening a Tauri window, without adding temporary UI, and without falling back to fake-provider behavior except in explicit fixture tests. It should import or materialize a project, apply selected provider/model controls, run the agent against an instruction, enforce limits, persist events in app-data state, export the final artifact, and return a process exit code that benchmark runners can trust.

The second part is a normalized artifact contract. Every run should emit a manifest, a trajectory, a redacted support bundle, final workspace metadata, and benchmark-specific outputs such as a patch or Harbor ATIF trajectory. The existing `xero_harness_evals` quality gate can remain the fast static preflight, but external benchmark runs need richer per-task artifacts and comparable metrics.

The third part is the Harbor adapter. Xero should expose a Python installed-agent adapter that Harbor can import with `--agent-import-path`. The adapter installs or locates the Xero headless command in the task container, prepares an isolated app-data directory for the trial, forwards the task instruction and model settings to Xero, lets Xero act inside the benchmark workspace, and attaches Xero's manifest and trajectory back to the Harbor context.

The fourth part is the competitor registry. It should describe how to run Xero, OpenCode, ForgeCode, and other baselines with the same benchmark, model, limits, environment variables, and artifact collection. Competitors that Harbor already supports should use Harbor's built-in `--agent` integration. Competitors that Harbor does not support should use the same installed-agent adapter shape as Xero.

## Command Contract

The exact Harbor flags should come from the installed Harbor version's help output, but Xero should support these command shapes:

- Xero on Terminal-Bench through Harbor: `harbor run -d <terminal-bench-dataset> -m <model> --agent-import-path <xero-agent-adapter>`.
- OpenCode on Terminal-Bench through Harbor: `harbor run -d <terminal-bench-dataset> -m <model> -a opencode`, when the installed Harbor version lists `opencode`.
- ForgeCode on Terminal-Bench through Harbor: `harbor run -d <terminal-bench-dataset> -m <model> --agent-import-path <forge-agent-adapter>`, until ForgeCode is available as a verified built-in agent.
- Xero on SWE-style tasks: run the Xero headless benchmark command to produce `predictions.jsonl`, then run `python3 -m swebench.harness.run_evaluation` with the selected dataset and run id.

The implementation should not bake these examples into code. It should store dataset id, agent adapter, model, attempts, concurrency, and environment as configuration so the same runner can reproduce an old run even after Harbor changes aliases or defaults.

## Run Manifest

Every benchmark attempt must record:

- Run identity: run id, task id, attempt index, benchmark name, dataset id, dataset version or digest, task selection rule, and run date.
- Harness identity: harness name, harness version, adapter version, source revision, prompt bundle version, tool policy version, and whether it is fixed-model or product mode.
- Model identity: provider, model id, endpoint class, temperature, reasoning effort, context budget, max output tokens, seed when available, and provider account class.
- Execution limits: wall time, max turns, max tool calls, max command calls, max cost, approval mode, sandbox profile, network policy, and retry policy.
- Environment: local Docker, Daytona, Modal, or another environment; image digest; OS and architecture; installed CLI versions; relevant env vars by name only; and whether secrets were redacted.
- Results: status, verifier status, score or reward, resolved flag, failure category, cost, token counts, wall time, command count, tool-call count, patch stats, and final artifact references.
- Evidence: Xero trajectory, Harbor trajectory, terminal recording if available, final diff, verifier logs, redaction summary, and support-bundle pointer.

No benchmark report should publish a success rate without the manifest fields above. Scores without benchmark version, model version, and harness version are not useful evidence.

## Metrics

Primary metrics:

- Task success rate or resolve rate.
- Pass@1 for single-attempt runs.
- pass^k or repeated-trial success when running multiple attempts.
- Wilson or bootstrap confidence interval for success rate.
- Mean and p95 wall time.
- Mean and p95 cost.
- Mean and p95 token usage.

Secondary metrics:

- Completion, timeout, crash, and invalid-output rates.
- Tool-call validity rate.
- Verification-evidence rate.
- Approval/manual-intervention count.
- Patch size, edited-file count, and unnecessary-change rate.
- Regression rate for tasks with pass-to-pass tests.
- Failure category distribution.

Failure categories should include setup failure, provider/auth failure, harness crash, timeout, budget exhausted, policy blocked, invalid output, verifier failed, flaky verifier, and benchmark infrastructure failure.

## Phase 0: Baseline And Preflight

Keep the existing Xero quality eval suite as the zero-cost preflight. Extend its output only enough to share the benchmark manifest vocabulary: suite id, fixture id, harness revision, tool policy version, metrics, failures, and artifact paths.

Add an external benchmark preflight command that checks:

- Harbor is installed and can run the Terminal-Bench oracle or a tiny sample task.
- Docker or the selected cloud sandbox provider is available.
- `python3` can run the SWE-bench evaluator when SWE-style tasks are selected.
- The selected model provider credentials are present without printing secrets.
- The requested competitor CLIs are installed or can be installed in the benchmark container.
- The configured dataset id resolves.
- Xero can create a fresh app-data directory for the run.

Exit criteria:

- A maintainer can run a local preflight before spending model tokens.
- The preflight prints exact versions for Harbor, Docker, Xero, OpenCode, ForgeCode when present, and the selected provider/model route.
- No benchmark path writes new state to the legacy repo-local `.xero` directory.

## Phase 1: Headless Xero Runner

Build a headless benchmark command around the real owned-agent runtime. It should be usable by Harbor, SWE-style adapters, CI, and local smoke tests.

Required behavior:

- Accept an instruction, workspace root, benchmark metadata, provider/model controls, approval mode, and limits.
- Create or select an isolated app-data root for the trial.
- Import the benchmark workspace as a Xero project without opening the desktop UI.
- Drive the real provider loop and real tool runtime.
- Enforce wall-time, turn, tool-call, command, and cost limits.
- Stop cleanly on completion, timeout, budget exhaustion, crash, cancellation, or policy block.
- Export a normalized manifest, trajectory, redacted log bundle, and final workspace diff.
- Return stable exit codes for success, verifier-independent failure, infrastructure failure, and invalid configuration.

Exit criteria:

- A synthetic repository task runs through the real runtime headlessly and produces a manifest plus trajectory.
- Fake-provider mode is explicit and labeled as fixture-only.
- The command is covered by focused Rust tests and a CLI smoke test.

## Phase 2: Harbor Adapter For Terminal-Bench

Implement Xero as a Harbor installed agent. This is the first public-comparison adapter.

Adapter behavior:

- `install` locates or installs the Xero headless command and benchmark dependencies inside the container.
- `run` forwards the rendered Terminal-Bench instruction to Xero, with the benchmark workspace as the project root.
- The adapter sets a trial-local app-data directory and passes only approved environment variables.
- Xero writes its own artifacts, while Harbor remains the source of task execution, sandboxing, verifier execution, and job layout.
- `populate_context_post_run` attaches Xero's manifest, cost, failure category, and trajectory references.

Initial run matrix:

- Xero vs Harbor `nop` and `oracle` for adapter sanity.
- Xero vs OpenCode using Harbor's built-in `opencode` agent when `harbor run --help` lists it.
- Xero vs Mini-SWE-Agent or OpenHands as a stable open-source baseline.
- Xero vs ForgeCode once the Forge installed-agent wrapper passes smoke.

Exit criteria:

- Xero completes a 5-task Terminal-Bench smoke run through Harbor.
- The same task ids and model run through at least one competitor.
- The report includes success rate, confidence interval, cost, time, verifier logs, and trajectory links.

## Phase 3: Competitor Support

Competitor support should be configuration-driven, not hardcoded into the Xero runner.

OpenCode support:

- Prefer Harbor's built-in `opencode` installed agent.
- If unavailable, use an installed-agent wrapper around `opencode run`.
- Record OpenCode version, config source, model flag, permission mode, and whether ACP/server mode was used.
- Export OpenCode sessions when possible and attach sanitized logs.

ForgeCode support:

- Use a dedicated trial-local `FORGE_CONFIG` so benchmark runs never reuse a developer's personal Forge state.
- Prefer stdin prompting or the most stable documented non-interactive path confirmed by smoke tests.
- Record ForgeCode version, config directory, provider/model settings, service features enabled, MCP state, and whether semantic sync was enabled.
- Disable auto-opening browser artifacts during benchmark runs.
- Treat ForgeCode Services and semantic sync as product-mode features unless the same retrieval capability is also enabled for Xero and other harnesses in a fixed-model harness comparison.

Other baselines:

- Add Mini-SWE-Agent, SWE-agent, OpenHands, Aider, Claude Code, Codex CLI, Gemini CLI, and Cursor CLI as registry entries only when their install path and non-interactive mode are verified.
- Keep a "do nothing" or `nop` baseline and the Harbor `oracle` run in smoke reports. They catch broken scoring and broken task setup.

Exit criteria:

- A benchmark owner can add or disable a competitor without changing Xero runtime code.
- Every competitor has a smoke status: supported, blocked, or experimental.
- Unsupported competitors fail closed with an explanation before paid runs start.

## Phase 4: SWE-Bench Adapter

Add a SWE-style adapter after Terminal-Bench works. This adapter measures patch quality rather than terminal-task behavior.

Adapter flow:

1. Materialize each task repository at the exact dataset commit.
2. Import the repository into an isolated Xero app-data root.
3. Prompt Xero with the issue statement and benchmark rules.
4. Let Xero edit and verify through the normal runtime.
5. Capture the final unified diff.
6. Write `predictions.jsonl` with the official `instance_id`, `model_name_or_path`, and `model_patch` fields.
7. Run the official evaluator with `python3 -m swebench.harness.run_evaluation`.
8. Attach evaluator logs and resolved status back to the Xero report.

Use SWE-bench Lite or Verified for integration smoke, then expand to the current dataset needed for the claim. On Apple Silicon, record whether evaluation images were pulled or built locally because official SWE-bench notes different behavior for ARM machines.

Exit criteria:

- Xero produces valid predictions for a small SWE-style subset.
- The official evaluator grades those predictions without manual repair.
- The same task ids and model run through at least one baseline harness.

## Phase 5: Private Xero Eval Set

Public benchmarks will not cover all product promises. Build a private eval set that looks like real Xero work and run it before releases.

Task types:

- Rust backend changes with scoped Cargo tests.
- React and ShadCN UI changes with unit or component tests.
- Tauri command-surface changes.
- App-data persistence and migration tasks.
- Provider setup and diagnostic failures.
- Session memory, compaction, branch, rewind, and retrieval behavior.
- Dirty-worktree conflict handling.
- Prompt-injection handling from untrusted files.
- Tool-policy and approval-boundary tasks.
- MCP, browser, emulator, and Solana workbench tasks when those surfaces are part of the release claim.

Each task should include a setup script, task prompt, hidden verifier, expected safety constraints, oracle solution, and artifact-retention policy.

Exit criteria:

- The private set catches at least one class of regression that public Terminal-Bench and SWE-style tasks do not.
- Private tasks are versioned, rotated, and kept out of public reports unless explicitly approved.

## Phase 6: Reporting

Produce two report shapes.

The engineering report is complete and reproducible. It includes manifests, trajectories, verifier logs, final diffs, failure categories, costs, timings, task ids, adapter versions, and environment details.

The public summary is concise. It includes benchmark name and version, run date, model, harness version, task count, success rate with confidence interval, cost/time summary, and links to public trajectories where licenses and privacy allow.

Reports should group results by task type, language, repository, edited-file count, failure category, and harness. A harness that scores well overall but fails Rust, TypeScript, or desktop-app tasks is not good enough for Xero's own development loop.

Exit criteria:

- A benchmark owner can regenerate the report from stored artifacts.
- The report clearly marks fixed-model comparison versus product-mode comparison.
- A failed run is as useful as a successful one because it identifies failure category and supporting evidence.

## Phase 7: Operational Support

Support this as a maintained benchmark harness, not a one-off script.

Ongoing responsibilities:

- Pin and record Harbor, dataset, Docker image, Xero, and competitor versions.
- Run a weekly smoke job against a tiny Terminal-Bench subset.
- Run a release-candidate job against the private Xero eval set.
- Keep a compatibility matrix for competitor CLIs and Harbor built-in agents.
- Rotate benchmark API keys and use benchmark-only provider accounts.
- Redact prompts, file contents, env values, and credentials before sharing logs.
- Preserve raw artifacts internally long enough to debug regressions.
- Track cost budgets per benchmark suite.
- Review task contamination and benchmark drift before making public claims.

Support triage should answer these questions in order:

1. Did the task environment build and verify with oracle?
2. Did the model/provider route work outside the harness?
3. Did the adapter invoke the intended harness version and model?
4. Did the harness fail before, during, or after tool execution?
5. Did the verifier fail because the solution was wrong, the patch was invalid, or the benchmark infrastructure was unhealthy?
6. Is the failure reproducible with the same seed, task id, and artifact bundle?

## First Milestone

The first shippable milestone is "Xero can be benchmarked fairly."

Deliverables:

- Extend the existing Xero quality eval report with benchmark-compatible manifest metadata.
- Add the headless Xero benchmark command around the real runtime.
- Add the Harbor installed-agent adapter for Xero.
- Run 5 to 10 Terminal-Bench tasks through Harbor for Xero and OpenCode with the same model.
- Add a ForgeCode wrapper spike and mark it supported only after a smoke run passes.
- Add a SWE-style smoke adapter that produces valid `predictions.jsonl` and runs the official evaluator.
- Generate one engineering report with manifests, trajectories, verifier logs, cost, time, and failure categories.

Success criteria:

- The same task ids, model, limits, and environment are used for Xero and at least one competitor.
- Every run is reproducible from stored configuration.
- Every artifact is redacted before it leaves local/internal storage.
- The report is useful even if Xero loses the first comparison.

## Sources

- [Harbor: Running Terminal-Bench](https://www.harborframework.com/docs/tutorials/running-terminal-bench)
- [Harbor: Agents](https://www.harborframework.com/docs/agents)
- [Harbor: Run Evals](https://www.harborframework.com/docs/run-jobs/run-evals)
- [Harbor repository agent guidance](https://github.com/harbor-framework/harbor/blob/main/AGENTS.md)
- [Terminal-Bench 2.0 announcement](https://www.tbench.ai/news/announcement-2-0)
- [SWE-bench repository and evaluator README](https://github.com/SWE-bench/SWE-bench)
- [OpenCode CLI docs](https://opencode.ai/docs/cli/)
- [ForgeCode setup docs](https://forgecode.dev/docs/)
- [ForgeCode piping guide](https://forgecode.dev/docs/piping-guide/)
- [ForgeCode config docs](https://forgecode.dev/docs/forge-config/)
- [ForgeCode MCP docs](https://forgecode.dev/docs/mcp-integration/)
