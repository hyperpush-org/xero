# Agent Harness Benchmarking Research And Plan

Reader: an internal Xero engineer or evaluation owner.

Post-read action: implement a benchmark path that can compare Xero's owned agent harness against other agent harnesses with reproducible, defensible results.

Last researched: 2026-05-01.

## Short Answer

There is no single universally agreed benchmark for "agent harnesses." The agreement is emerging around a testing pattern:

1. Freeze the task set.
2. Run each agent in the same sandboxed environment.
3. Let the agent act through its real tools.
4. Grade the final state with objective tests or a tightly specified verifier.
5. Publish enough metadata, trajectories, patches, logs, costs, and configuration to make the number interpretable.

For Xero, the strongest benchmark stack is:

- Use SWE-Bench Pro or the current SWE-bench family for software issue resolution credibility.
- Use Terminal-Bench for terminal-native, end-to-end harness behavior.
- Use a private Xero-shaped eval set for product-relevant regressions that public leaderboards do not measure.
- Add OSWorld or WebArena only if Xero wants to make claims about general desktop or browser-control performance.

The key comparison rule is simple: to compare harnesses, hold the model fixed. To compare models, hold the harness fixed. Scores that change both at once are product demos, not clean benchmark evidence.

## What Public Benchmarks Measure

SWE-bench established the most common coding-agent evaluation shape: given a real GitHub issue and repository snapshot, produce a patch that resolves the issue. The official evaluation applies generated patches in Docker and reports resolved instances, completion, logs, and resolution rate. SWE-bench's evaluation guide expects JSONL predictions with `instance_id`, `model_name_or_path`, and `model_patch`, then runs the Dockerized harness over the predictions.

SWE-bench Verified was the public benchmark most often cited in model and harness comparisons. It is still historically useful and the SWE-bench site describes it as a 500-instance human-validated subset. However, as of February 23, 2026, OpenAI says SWE-bench Verified is increasingly contaminated and recommends SWE-bench Pro for frontier coding capability claims. That matters for Xero: Verified can be used for continuity with older posts, but it should not be the headline benchmark for a new serious claim.

SWE-Bench Pro is designed to answer the next problem: longer-horizon, less contaminated, more industrial coding tasks. Its public description emphasizes Docker-based environments, fail-to-pass tests for issue resolution, pass-to-pass tests for regression prevention, human-augmented task specs, and a primary Resolve Rate metric. It also publishes trajectories, which is important for inspecting whether a harness solved tasks cleanly or accidentally got lucky.

Terminal-Bench is the closest public match for comparing agent harness mechanics. It is explicitly a benchmark for AI agents in real terminal environments. Each task includes an English instruction, a sandbox, a test script, and an oracle solution. The harness connects a model or agent to a sandboxed terminal, records logs and terminal sessions, then validates the final state. This is valuable for Xero because terminal benchmarks stress tool orchestration, file operations, command execution, retries, and verification behavior rather than only final patch format.

Multi-language coding benchmarks matter because Xero itself is not a Python-only product. Multi-SWE-bench covers Java, TypeScript, JavaScript, Go, Rust, C, and C++, while SWE-PolyBench covers repository-level tasks in Java, JavaScript, TypeScript, and Python. These are better signals for Xero's Rust, TypeScript, and desktop-app work than Python-only issue resolution alone.

WebArena and OSWorld are useful if Xero wants to compare browser or general computer-control behavior. WebArena evaluates agents on reproducible web tasks across realistic sites. OSWorld evaluates multimodal agents in real computer environments with setup and execution-based evaluation scripts. They are broader than Xero's immediate owned-agent harness question, so they should be second-wave benchmarks unless desktop/browser autonomy becomes the benchmark claim.

tau-bench and GAIA are less direct matches. tau-bench is useful for multi-turn tool-agent-user policy adherence in simulated service domains. GAIA is useful for general assistant research and tool use. They can inspire metrics and reliability methodology, but they are not the first benchmark to run for a coding harness.

## What Counts As The Harness

When comparing Xero against another harness, the benchmark subject is not just the model. For Xero, "harness" should mean the owned-agent runtime around the model:

- System prompt and runtime-agent contract.
- Context assembly, compaction, memory, and instruction-file handling.
- Tool descriptors, tool selection, and tool validation.
- Filesystem, command, git, browser, emulator, Solana, MCP, and skill tool policies.
- Approval and continuation gates.
- Verification and completion gates.
- Rollback/checkpoint behavior.
- Persistence, transcript export, usage tracking, and redaction.

Xero already has an internal production harness eval suite that checks tool exposure, descriptor validation, plan gates, verification gates, rollback coverage, and fixture coverage. Keep that suite as a regression gate. It is not enough for public comparison because it does not run real model trajectories over public benchmark tasks.

## Benchmarking Protocol

Use two run modes.

Mode 1: fixed-model harness comparison.

Run Xero, OpenHands, SWE-agent or mini-SWE-agent, and any other target harness with the same model, provider, temperature or reasoning settings, context limit, max turns, max wall time, max cost, and benchmark split. This is the clean answer to "is Xero's harness better?"

Mode 2: product-mode comparison.

Run Xero with its best supported provider setup and compare against public product leaderboard numbers. This is useful for marketing or product positioning, but the report must say it is not an apples-to-apples harness comparison unless the model and settings match.

Every run should produce a manifest with:

- Benchmark name, version, split, and task ids.
- Harness name, git revision, prompt bundle version, tool policy version, and adapter version.
- Model, provider, endpoint class, temperature, reasoning effort, seed if available, context budget, and tool budget.
- Sandbox image digest or environment hash.
- Time limit, max turns, retry policy, and approval mode.
- Per-task cost, tokens, wall time, number of tool calls, number of command calls, and final status.
- Final patch or final-state artifact, full redacted trajectory, verifier output, and failure category.

The primary metrics should be:

- Resolve rate or task success rate.
- Pass@1 for deterministic or single-attempt runs.
- pass^k or repeated-trial success for stochastic agents when cost allows.
- Completion rate, timeout rate, crash rate, and invalid-output rate.
- Mean and p95 cost per task.
- Mean and p95 wall time per task.
- Tool-call validity rate.
- Verification-evidence rate.
- Regression rate for tasks with pass-to-pass tests.
- Manual intervention or approval count.
- Safety and policy violation count.
- Patch size, file count, and unnecessary-change rate.

Do not publish a single score without the model, benchmark version, harness version, and run mode. For agent harnesses, the scaffold often contributes as much as the model.

## Implementation Plan

### Phase 0: Establish A Local Baseline

Keep the existing internal harness eval suite as a fast preflight. Extend its report shape only if needed so it can emit the same run-manifest concepts as external benchmarks: suite id, fixture id, tool policy version, metrics, failures, and markdown/JSON output.

Success condition: a focused test can prove Xero's tool descriptors, gates, rollback expectations, and verification expectations before any paid benchmark run starts.

### Phase 1: Build A Headless Eval Runner

Add a headless evaluation entry point for owned-agent runs. Do not add temporary UI. The runner should be usable from tests or CLI automation and should drive the same owned-agent runtime that the desktop app uses.

The runner needs to:

- Create an isolated project sandbox from a benchmark task.
- Start an owned-agent run with a provided prompt, provider profile, model, tool policy, and limits.
- Stream and persist the trajectory.
- Stop cleanly on completion, timeout, budget exhaustion, crash, or policy block.
- Export a normalized run artifact.
- Redact secrets before writing shareable logs.

Success condition: one local synthetic task can run end-to-end through Xero's owned-agent harness and produce a stable manifest plus trajectory artifact.

### Phase 2: Add A SWE-style Patch Adapter

Build an adapter that converts a SWE-style task into a Xero owned-agent run and converts the final working tree diff into the JSONL prediction format expected by SWE-bench-compatible harnesses.

The adapter flow:

1. Materialize the task repository in an isolated sandbox.
2. Import that sandbox as a Xero project.
3. Prompt Xero with the issue statement and benchmark rules.
4. Let Xero edit, test, and verify through its normal tools.
5. Capture the final diff as the task prediction.
6. Run the official verifier with `python3 -m swebench.harness.run_evaluation` or the relevant benchmark CLI.
7. Attach verifier logs back to the Xero run report.

Start with a tiny smoke subset, then scale to a public SWE-Bench Pro or current SWE-bench-family split. Include Multi-SWE-bench or SWE-PolyBench early because TypeScript and Rust coverage is more relevant to Xero than Python-only performance.

Success condition: Xero can produce valid predictions for a small SWE-style subset, and the official verifier can grade them without manual repair.

### Phase 3: Add A Terminal-Bench Adapter

Use Terminal-Bench as the first real harness-vs-harness comparison. It exercises the command loop directly and avoids overfitting the plan around patch-only output.

There are two viable adapter shapes:

- Implement a Terminal-Bench-compatible agent adapter that forwards each task instruction to Xero and lets Xero operate in the benchmark sandbox.
- Or wrap Xero's owned-agent command execution behind the benchmark's terminal interface so Terminal-Bench can record the same terminal panes, commands, and tests it records for other agents.

Prefer the adapter that preserves the official Terminal-Bench logging and verifier flow. Xero's own transcript should be additional evidence, not a replacement for benchmark-native logs.

Success condition: Xero can run a versioned Terminal-Bench dataset subset with the same model and limits as at least one baseline harness.

### Phase 4: Create A Private Xero Eval Set

Public benchmarks will not measure Xero-specific product promises. Build a private set that looks like real Xero work:

- Rust backend changes with scoped Cargo tests.
- React/ShadCN UI changes with unit or component tests.
- Tauri command-surface changes.
- App-data persistence changes.
- Provider setup and diagnostic failures.
- Session memory, compaction, branch, rewind, and retrieval behavior.
- Dirty worktree conflict handling.
- Prompt-injection handling from untrusted files.
- Tool-policy and approval-boundary tasks.

Each task should include a setup script, task prompt, hidden verifier, expected safety constraints, and an oracle solution. Keep the task set private if it will be used for decision-making, and rotate it as Xero changes.

Success condition: the private set catches regressions that public SWE-style and terminal-style benchmarks miss.

### Phase 5: Reporting And Comparison

Publish two report types.

The engineering report is detailed and reproducible. It includes manifests, trajectories, verifier logs, failure categories, costs, timings, and patches.

The public summary is narrower. It includes benchmark version, run date, model, harness version, task count, success rate with confidence interval, cost/time summary, and links to public trajectories when licenses allow.

For every chart, split results by language, repository, task type, edit size, and failure category. A harness that scores well overall but fails Rust or TypeScript tasks is not good enough for Xero's own development loop.

## First Milestone

Build a small but honest benchmark loop before chasing a leaderboard:

1. Normalize the existing internal eval report into JSON and markdown artifacts.
2. Add the headless owned-agent eval runner.
3. Run 5 to 10 SWE-style smoke tasks and grade them with the official verifier.
4. Run 5 to 10 Terminal-Bench tasks through the official harness.
5. Run the same model through Xero and one baseline harness.
6. Produce a comparison report with success rate, verifier logs, cost, time, and trajectory links.

That milestone will answer whether Xero can be benchmarked fairly. Only after that should Xero spend money on large public runs or leaderboard submissions.

## Sources

- [SWE-bench GitHub repository](https://github.com/SWE-bench/SWE-bench)
- [SWE-bench evaluation guide](https://www.swebench.com/SWE-bench/guides/evaluation/)
- [SWE-bench Verified overview](https://www.swebench.com/verified.html)
- [OpenAI: Why SWE-bench Verified no longer measures frontier coding capabilities](https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/)
- [SWE-Bench Pro public leaderboard and methodology](https://labs.scale.com/leaderboard/swe_bench_pro_public)
- [SWE-Bench Pro paper](https://arxiv.org/abs/2509.16941)
- [Terminal-Bench repository](https://github.com/harbor-framework/terminal-bench)
- [Terminal-Bench harness docs](https://www.tbench.ai/docs/harness)
- [Terminal-Bench task docs](https://www.tbench.ai/docs/task-overview)
- [Terminal-Bench 2.0 paper](https://arxiv.org/abs/2601.11868)
- [Multi-SWE-bench paper](https://arxiv.org/abs/2504.02605)
- [SWE-PolyBench paper](https://arxiv.org/abs/2504.08703)
- [SWE-agent paper](https://arxiv.org/abs/2405.15793)
- [OpenHands evaluation harness docs](https://docs.openhands.dev/openhands/usage/developers/evaluation-harness)
- [OSWorld project](https://os-world.github.io/)
- [WebArena paper](https://arxiv.org/abs/2307.13854)
- [OpenAI evaluation best practices](https://developers.openai.com/api/docs/guides/evaluation-best-practices)
- [Inspect evaluation framework](https://inspect.aisi.org.uk/)
