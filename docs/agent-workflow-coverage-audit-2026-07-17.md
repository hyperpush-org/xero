# Agent, Stage, and Workflow Coverage Audit — 2026-07-17

## Outcome

This audit found and fixed critical defects in owned-agent continuation, Stage replay, subagent execution, mutation batching, approval replay, runtime-control reconciliation, and test isolation. The first expansion added 35 desktop-library tests and fixed four defects in Workflow condition scoping, input binding, JSON path evaluation, and provider error classification. The next follow-up found and fixed another eleven product and fixture-contract issues, including a capability schema mismatch, non-idempotent handoff retries, an unbounded Stage-gate reprompt loop, invalid Workflow resume paths, and stale compare-and-swap handling. The headless extension found and fixed seven more defects in approval enforcement, continuation/retry identity, provider failure persistence, reasoning controls, and obsolete release-gate plumbing. The 2026-07-18 continuation found and fixed seven additional defects in wakeup scheduling/persistence, the agent wire contract, Workflow update projection, and the production headless harness.

The deterministic test matrix is green with no skipped agent/Stage/Workflow frontend tests. The original completed matrix included 1,568 desktop-library tests, 130 `xero-agent-core` tests, 114 focused Rust integrations, and 1,231 focused frontend tests. A continuation expansion added four fixture-backed Rust command integrations and seven frontend contract/adapter cases; the current complete frontend matrices pass 1,409 client tests and 127 shared-UI tests. The headless extension adds six fixture-backed CLI regressions and brings the complete `xero-cli` matrix to 206 tests.

No known critical or high-severity finding from this pass remains open.

The original audited 36-file Rust surface measures:

| Metric | Covered | Total | Coverage |
| --- | ---: | ---: | ---: |
| Lines | 94,066 | 123,183 | 76.36% |
| Functions | 5,320 | 7,626 | 69.76% |
| Regions | 113,039 | 149,156 | 75.79% |

This is the original conservative audited-surface metric, not a claim of blanket 100% coverage. LLVM includes inline test bodies, monomorphized/generated paths, host-specific branches, and provider/platform code that cannot all execute on one macOS host. The critical deterministic contracts listed below have direct regression or end-to-end evidence even where their containing file has lower aggregate coverage.

The expansion deliberately concentrated on the weakest deterministic files. The before profile is `/tmp/xero-desktop-lib-coverage-final.json`; the after profile combines `/tmp/xero-desktop-lib-coverage-expanded.json` with the instrumented Workflow execution in `/tmp/xero-agent-workflow-coverage-expanded.json`:

| File | Lines before | Lines after | Functions before | Functions after |
| --- | ---: | ---: | ---: | ---: |
| `runtime/workflow_orchestrator/condition_eval.rs` | 180/356 (50.56%) | 567/569 (99.65%) | 13/26 (50.00%) | 30/30 (100.00%) |
| `runtime/workflow_orchestrator/artifacts.rs` | 314/525 (59.81%) | 741/751 (98.67%) | 23/46 (50.00%) | 49/50 (98.00%) |
| `runtime/workflow_orchestrator/command_policy.rs` | 209/259 (80.69%) | 302/335 (90.15%) | 16/19 (84.21%) | 23/26 (88.46%) |
| `provider_preflight.rs` | 666/1,299 (51.27%) | 1,081/1,613 (67.02%) | 48/109 (44.04%) | 73/123 (59.35%) |
| `commands/agent_session.rs` | 20/281 (7.12%) | 236/439 (53.76%) | 1/15 (6.67%) | 14/24 (58.33%) |
| `runtime/agent_core/facade.rs` | 54/754 (7.16%) | 494/1,013 (48.77%) | 4/45 (8.89%) | 28/56 (50.00%) |

LLVM counts inline test bodies, so adding tests increases both covered lines and the denominator. The ratios above therefore use each profile's actual source totals rather than holding the old denominator constant.

The current follow-up generated fresh standalone profiles after all fixes:

| Profile | Lines | Functions | Regions |
| --- | ---: | ---: | ---: |
| Desktop library (`/tmp/xero-desktop-lib-coverage-goal.json`) | 176,751/301,021 (58.72%) | 11,460/21,785 (52.61%) | 225,151/385,971 (58.33%) |
| `xero-agent-core` (`/tmp/xero-agent-core-coverage-goal.json`) | 12,838/20,150 (63.71%) | 944/1,479 (63.83%) | 16,222/25,324 (64.06%) |

The desktop total deliberately includes the entire library, not only the 36-file audit surface, and the library-only profile does not credit integration-test processes. Representative current line coverage on the changed deterministic surface is:

| File | Line coverage |
| --- | ---: |
| `commands/workflows.rs` | 844/1,295 (65.17%) |
| `commands/agent_reports.rs` | 104/194 (53.61%) |
| `db/migrations.rs` | 96.98% |
| `db/project_store/agent_audit.rs` | 90.73% |
| `db/project_store/runtime.rs` | 55.17% |
| `runtime/agent_core/provider_loop.rs` | 84.38% |
| `runtime/agent_core/run.rs` | 70.44% |
| `runtime/agent_core/tool_descriptors.rs` | 90.90% |
| `runtime/workflow_orchestrator/definition_validator.rs` | 80.13% |

The Workflow command tests grew from five to ten and now directly cover required-input diagnostics, deduplicated labels, root/nested resume writes, invalid resume paths, delivery lookup and sorting variants, loop-resume fallbacks, and blocker priority.

The final headless extension generated fresh post-fix profiles:

| Profile | Lines | Functions | Regions |
| --- | ---: | ---: | ---: |
| Desktop library (`/tmp/xero-desktop-lib-coverage-headless-audit.json`) | 176,732/301,023 (58.71%) | 11,458/21,784 (52.60%) | 225,137/385,972 (58.33%) |
| Core + CLI headless workspace (`/tmp/xero-headless-workspace-coverage-audit.json`) | 35,484/53,424 (66.42%) | 2,653/4,332 (61.24%) | 47,466/73,578 (64.51%) |
| `xero-agent-core` standalone (`/tmp/xero-agent-core-coverage-headless-audit.json`) | 12,850/20,233 (63.51%) | 946/1,490 (63.49%) | 16,235/25,444 (63.81%) |

The combined headless profile is the representative metric for the production headless path because it credits the mock HTTP/provider and app-data fixtures housed in `xero-cli`. In that profile, `headless_runtime.rs` reaches 2,245/3,888 lines (57.74%), 149/269 functions (55.39%), and 3,022/5,301 regions (57.01%); `xero-cli/src/lib.rs` reaches 9,377/13,455 lines (69.69%). The remaining headless branches are primarily alternate tools/providers, platform-specific sandbox behavior, and defensive failures that cannot all execute in one local fixture matrix.

## 2026-07-18 coverage continuation

The latest TDD pass added fixture-backed coverage for durable wakeups, process-output polling, the complete agent DTO conversion surface, Workflow driver update detection, headless file/command lifecycle and rollback, and local HTTP provider contracts. The final merged profiles are:

| Profile | Lines | Functions | Regions |
| --- | ---: | ---: | ---: |
| Desktop library + focused agent/Workflow integrations (`/tmp/xero-agent-workflow-merged-coverage-2026-07-18.json`) | 195,236/302,470 (64.55%) | 58.52% | 63.84% |
| Core + CLI headless packages (`/tmp/xero-headless-merged-coverage-2026-07-18.json`) | 37,571/55,009 (68.30%) | 62.03% | 66.53% |

Representative post-fix source coverage is:

| File | Lines | Functions | Regions |
| --- | ---: | ---: | ---: |
| `commands/contracts/agent.rs` | 625/625 (100.00%) | 100.00% | 100.00% |
| `db/project_store/agent_wakeups.rs` | 832/869 (95.74%) | 80.77% | 92.27% |
| `runtime/agent_core/wakeup_scheduler.rs` | 743/1,084 (68.54%) | 70.69% | 69.43% |
| `commands/workflows.rs` | 926/1,295 (71.51%) | 72.22% | 70.73% |
| `runtime/workflow_orchestrator/driver.rs` | 467/630 (74.13%) | 74.19% | 72.92% |
| `runtime/workflow_orchestrator/reconcile.rs` | 4,775/6,864 (69.57%) | 72.13% | 72.35% |
| `xero-agent-core/src/headless_runtime.rs` | 4,318/5,473 (78.90%) | 66.21% | 78.93% |
| `xero-cli/src/lib.rs` | 9,377/13,455 (69.69%) | 59.69% | 66.64% |

The headless runtime rose from 57.74% to 78.90% line coverage. Its deterministic matrix now exercises local OpenAI-compatible JSON and Codex SSE requests, authenticated headers, provider-specific reasoning payloads, streamed text/reasoning/tool-call reconstruction, malformed/status failures, every production file tool, command policies and timeouts, rollback, path traversal/symlink defenses, and intentional empty-file writes. The remaining gap is concentrated in full live-provider orchestration, host/platform failures, and defensive I/O branches.

## Continuation test expansion

After the main audit, four new Tauri/mock-app integration fixtures were added without changing production behavior:

| Fixture | Direct command contracts covered |
| --- | --- |
| `agent_report_commands.rs` | Capability permissions, database touchpoints, run-start explanation, run-scoped knowledge inspection, available/unavailable support diagnostics, all three handoff selector routes, ambiguous/missing selectors, and invalid inputs. |
| `agent_default_model_commands.rs` | Built-in default-model create, replace, reset, all identifier validations, and custom-agent project routing. |
| `agent_definition_commands.rs` | Invalid/valid preview, pre-save approval review, approved save, list, version lookup, missing version, update review, approved update, version diff, archive, archived filtering, and version validation. |
| `agent_tooling_settings_commands.rs` | Default settings, global/override updates, identifier normalization, deterministic ordering, override removal, duplicate-request atomicity, invalid identifiers, unsupported schema state, and duplicate persisted state. |

Frontend additions directly cover the shared Agent Tooling schemas, agent default-model command schemas, and `XeroDesktopAdapter` request/response boundaries. These tests verify normalization before native invocation, reset payloads, strict malformed inputs, synchronous request validation, and asynchronous native-response validation.

These integrations ran before the reproducible Cargo target was cleaned. `cargo clean` removed 33.7 GiB of build artifacts and increased available workspace-disk capacity from 6 GiB to 37 GiB. The retained JSON coverage profiles predate these new integration-only fixtures; no unmeasured percentage increase is claimed.

## Final 2026-07-18 continuation addendum

The final continuation added a command-level agent-task fixture, default-model corruption coverage, shared core-store failure-atomicity fixtures, and process-tree race coverage. The authoritative post-fix merged profiles supersede the earlier 2026-07-18 snapshots above:

| Profile | Lines | Functions | Regions |
| --- | ---: | ---: | ---: |
| Desktop library + focused agent/Workflow integrations (`/tmp/xero-agent-workflow-merged-coverage-continued-2026-07-18.json`) | 195,754/302,788 (64.65%) | 12,807/21,848 (58.62%) | 248,307/388,352 (63.94%) |
| Core + CLI headless packages (`/tmp/xero-headless-merged-coverage-continued-2026-07-18.json`) | 38,272/55,984 (68.36%) | 2,788/4,469 (62.39%) | 51,638/77,585 (66.56%) |

The focused headless runtime now reaches 4,318/4,993 lines (86.48%). The shared core store reaches 1,486/2,680 lines (55.45%) in the combined headless profile, and the CLI remains at 9,377/13,455 lines (69.69%). On the desktop side, `commands/agent_task.rs` rose from 36.25% to 60.70% line coverage and `commands/agent_default_models.rs` reaches 64.17%. The complete agent DTO conversion surface remains at 100%, durable wakeups remain at 95.74%, and the shared process-tree implementation reaches 92.51%.

The final merged desktop command executed 1,591 library tests plus 119 focused integration tests. The final merged headless command executed 148 core unit tests, two core provider/protocol integrations, and 206 CLI/headless tests. Both coverage commands completed with zero failures.

## Final 2026-07-18 continuation II

The next TDD expansion added the reusable core facade, typed protocol routing, file-store integrity, real-provider continuation preflight, desktop command lifecycle, and custom-agent default-model fixtures. The refreshed profiles are:

| Profile | Lines | Functions | Regions |
| --- | ---: | ---: | ---: |
| Desktop library + focused agent/Workflow integrations (`/tmp/xero-agent-workflow-merged-coverage-continued-2-2026-07-18.json`) | 195,806/302,788 (64.67%) | 12,810/21,848 (58.63%) | 248,433/388,352 (63.97%) |
| Core + CLI headless packages (`/tmp/xero-headless-merged-coverage-continued-3-2026-07-18.json`) | 38,980/56,094 (69.49%) | 2,827/4,431 (63.80%) | 52,734/77,763 (67.81%) |

The desktop command profile executed 1,591 library tests plus 120 focused integrations with zero failures. `commands/agent_task.rs` rose again, from 678/1,117 lines (60.70%) to 763/1,117 (68.31%); `commands/contracts/agent.rs` remains at 100%, durable wakeups at 95.74%, process-tree handling at 92.51%, and the Workflow driver at 74.13%. A subsequent custom-default-model fixture covers the previously unexecuted custom definition save/reset path and brings the focused integration matrix to 121 tests. Its merged library + command profile (`/tmp/xero-agent-default-model-merged-coverage-continued-2026-07-18.json`) raises `commands/agent_default_models.rs` from 154/240 lines (64.17%) to 188/240 (78.33%).

The final clean merged headless command executed 153 core unit tests, ten core provider/protocol integrations, and 206 CLI tests. `headless_runtime.rs` reaches 4,377/5,000 lines (87.54%), the shared core store reaches 2,121/2,783 (76.21%), and `xero-cli/src/lib.rs` remains at 9,377/13,455 (69.69%). LLVM still counts monomorphized/generated paths and inline tests, so these raw figures are retained exactly as emitted. New direct evidence covers fake facade start/continue/input/approval/rejection/cancel/resume/fork/compact/export, every typed submission variant, blocked real-provider continuation atomicity, failed JSON-store commits, corrupt-store reopen rejection, and the desktop start/send/resume lifecycle.

## Final 2026-07-18 continuation III

The latest store-integrity TDD pass expanded manifest identity, ownership, provenance, trace, event-boundary, required-field, and ID-exhaustion fixtures. The refreshed headless profile is:

| Profile | Lines | Functions | Regions |
| --- | ---: | ---: | ---: |
| Core + CLI headless packages (`/tmp/xero-headless-merged-coverage-continued-4-2026-07-18.json`) | 39,535/57,413 (68.86%) | 2,865/4,541 (63.09%) | 53,534/79,723 (67.15%) |

The merged command executed 159 core unit tests, ten provider/protocol integrations, and 206 CLI/headless tests with zero failures. `headless_runtime.rs` remains at 4,377/5,000 lines (87.54%), `xero-cli/src/lib.rs` remains at 9,377/13,455 (69.69%), and the expanded shared core file records 2,685/4,102 raw LLVM lines (65.46%). Compared with the preceding profile, absolute covered lines increased by 555. The raw percentage decreased because LLVM added 1,319 counted lines from the substantially expanded inline fixture and monomorphized generic-store surface; the emitted numerator and denominator are retained rather than normalizing away that instrumentation effect.

New direct evidence proves both in-memory and file stores reject non-positive definition versions, blank manifest provider/model identities, cross-session manifests, duplicate project-scoped manifest IDs, and valid-but-wrong manifest traces without mutation. File-reopen mutation fixtures additionally reject missing provenance fields, invalid nested ownership and timestamps, noncanonical manifest traces, dangling event boundaries, invalid counters, and duplicate manifests. Checked sequence allocation now fails closed at `i64::MAX` instead of reusing saturated message/event IDs.

## Final 2026-07-18 continuation IV

Rust build storage was audited after the expanded coverage runs. `client/src-tauri/target` had reached 39 GiB: 29 GiB of debug/test output, including 2.6 GiB of incremental state, plus a retained 10 GiB LLVM coverage target. The workspace now disables incremental compilation globally and in dev, test, and release profiles; dev/test artifacts omit debug information and split debug output; release binaries strip symbols. The app and test wrappers force `CARGO_INCREMENTAL=0` even when inherited environment state requests otherwise, and routine test pruning now uses a one-hour retention window.

The invalidated target was cleaned explicitly (`38.8 GiB` removed). A complete clean `xero-desktop` development build recreated a 4.3 GiB target with an empty incremental directory; after stabilization, no-op app builds completed in 1.10–1.15 seconds. The final target, including the expanded core tests, Clippy artifacts, and relinked desktop app, is 4.5 GiB with zero incremental entries. Focused tests expanded store input-boundary coverage for blank project/run lookups, status mutations, project listings, empty store paths, and relative store paths.

## 2026-07-19 90% source-line hard gate

The final fixture-driven TDD pass raises both release-critical production scopes above the 90% source-line gate. Functions and regions are retained as supplemental diagnostics; the hard gate is line coverage because it is the stable, user-requested coverage measure used for this continuation.

| Audited production scope | Files | Lines | Functions | Regions |
| --- | ---: | ---: | ---: | ---: |
| Desktop agents, Stages, and Workflow (`/tmp/xero-desktop-agent-workflow-round6.json`) | 58 | 101,689/112,920 (90.0540%) | 5,456/6,639 (82.18%) | 125,460/140,600 (89.23%) |
| Headless agent core (`/tmp/xero-core-headless-round6.json`) | 10 | 24,466/26,394 (92.6953%) | 1,496/1,714 (87.28%) | 31,868/34,408 (92.62%) |

The desktop scope includes every file under `runtime/agent_core/` and `runtime/workflow_orchestrator/`; the agent and Stage definition runtime; top-level `commands/agent_*.rs`; Workflow command/contracts; and the agent/Workflow project stores. The headless scope includes every production source file in `crates/xero-agent-core/src/`. The complete `xero-cli` package remains a required green 206-test harness but is not folded into the core percentage because it also contains the TUI, remote client, updater, and other non-core surfaces.

The authoritative desktop profile passed 1,680 library tests plus the focused command/runtime suites: 55 owned-agent runtime, 12 context-continuity, 12 coordination, ten Workflow-agent, nine Workflow-execution, and the complete fixture-backed GSD Auto Workflow. The independent core/headless profile passed 164 core unit tests, ten provider/protocol integrations, and all 206 CLI tests. New fixtures directly exercise extension install/list/enable/disable/remove validation; bounded provider process and dynamic-tool diagnostics; required-state schema and agent-reference validation; lease loss and terminal-driver failure; scoped Stage resume/replay; and parent-process admission for mutating desktop handlers.

The final instrumented GSD Auto run completed its real local HTTP/SSE provider path in 60.64 seconds and consumed the full fixture contract. The hard-gate reports were generated independently so a desktop integration cannot accidentally inflate the headless core result.

The coverage-only Cargo target contained 16.6 GiB of instrumented build output and was explicitly cleaned after both JSON reports were retained. Routine target pruning then removed another 466.5 MiB of stale test executables. The retained workspace target is 6.5 GiB after the complete matrix and contains zero incremental entries; Cargo config, dev/test/release profiles, and the app/test wrappers all force incremental compilation off.

## Scope and method

The 36-file metric covers 27 desktop files and nine `xero-agent-core` files across:

- agent commands and runtime-run control boundaries;
- durable agent, mailbox, coordination, runtime-run, definition, and Workflow stores;
- provider loops, context/persistence, state machine, supervisor, tool descriptors, and tool dispatch;
- Stage definition, policy, replay, gate, and tool enforcement;
- Workflow definition validation, driver ownership, reconciliation, routing, checkpoints, commands, and artifacts;
- environment lifecycle, provider preflight, sandboxing, protocol, tool packs, and the process-isolated tool registry.

Provider behavior was exercised with deterministic OpenAI-compatible SSE fixtures. File mutations use exact SHA-256 guards. The GSD Auto Workflow uses the checked-in definition and LLM response fixtures under `client/test-fixtures/workflows/`.

## New findings and resolutions

| ID | Severity | Finding | Resolution and regression evidence |
| --- | --- | --- | --- |
| AW-01 | Critical | Runtime prompt-worker registration constructed and dropped a guard while its registry mutex was still held. The guard's `Drop` relocked the same mutex, deadlocking prompt continuation. | Replaced eager `then_some` construction with an explicit insert check and lock release. The runtime-control continuation and worker-registration tests pass. |
| AW-02 | High | A successful Stage todo artifact was recorded after Stage replay reconciliation. Reconciliation could therefore erase newly completed plan evidence or reuse stale evidence across Stage attempts. | Record plan artifacts before replay snapshot/reconciliation and scope completion evidence to the current Stage attempt. Added stale-todo and same-todo-per-attempt regressions. |
| AW-03 | High | Same-run continuations could enter the provider loop without running the environment lifecycle or delivering messages queued while setup completed. | Continuations now start/reload environment state, fail closed on setup errors, wait for readiness, deliver queued messages, and then rebuild the provider context. Canonical trace and lifecycle tests pass. |
| AW-04 | High | Runtime start preflight validated the model selected in UI controls instead of the actual resolved provider adapter. A provider override or profile resolution could make the admitted identity differ from the submitted identity. | Resolve the provider configuration first and preflight its real `(provider_id, model_id)` pair. Provider preflight manifest binding passes. |
| AW-05 | High | A subagent could be reported `running` before its durable child run existed. A crash in that window left coordination state referring to no run. | Create and bind the child run synchronously before reporting it running; only the provider drive remains asynchronous. |
| AW-06 | Critical | Subagent delegation executed inside a short-lived fork-isolation child. That child spawned a background thread and exited, killing the delegated work immediately. | Added an explicit parent-process execution contract for handlers with process-local lifecycle state. Subagent delegation stays behind policy, sandbox, checkpoint, rollback, budget, and cancellation gates but is supervised by the durable parent. The priority-one subagent integration passes under LLVM coverage. |
| AW-07 | High | Child context retrieval and audit rows used a transient child snapshot definition ID that had no relational definition-version row, causing foreign-key failures. | Provider context persistence now uses the durable run definition identity while retaining the typed child snapshot separately. Subagent lineage/journal persistence passes. |
| AW-08 | High | Successful isolated mutations were finalized only after the entire provider tool batch. A dependent sequence such as write → rename → delete could not observe the prior mutation's durable state. | Added a per-execution-group dispatch hook and finalize each isolated success before the next mutating group. Core hook and fixture-backed file sequence tests pass. |
| AW-09 | High | If one provider tool call named an unavailable tool, sibling calls were already marked `running` and could remain orphaned. | Validate the complete batch surface before starting records; persist only the rejected call as failed. The fixture-backed atomic-admission regression proves no running orphan remains. |
| AW-10 | High | Approved existing-file writes could replay without binding the current expected hash. Running the replay in a forked child could also deadlock against inherited SQLite/runtime locks. | Bind exact current hashes for edit, write, patch, copy, transaction, notebook, delete, and rename requests. Approved existing-write replay executes in the supervising process while keeping all safety gates. Command and file approval replay tests pass under LLVM coverage. |
| AW-11 | High | Archiving an agent session could race a final runtime-run projection and lose its compare-and-swap update. | Reload and retry the idle-runtime stop on bounded write conflicts; never overwrite an active or replacement run. Archive-after-interaction passes under LLVM coverage. |
| AW-12 | High | A user runtime-control update could lose a compare-and-swap race to the final agent projection even after the supervisor became inactive, surfacing a needless retry error. | Runtime-control updates now rebase and retry bounded, side-effect-free control writes against the latest durable snapshot. Provider-switch regression passes. |
| AW-13 | High | Provider-only switches retained the same agent run ID. The recovery worker exited when the continuation became `driving`, so pending provider controls were never consumed; agent switches hid the bug through their separate handoff path. | Keep the worker alive, wait for supervisor idleness, reconcile by durable continuation ID, then apply and consume pending controls without replaying provider dispatch. The provider-only boundary test verifies the active profile and empty pending state. |
| AW-14 | Medium | Several integration tests used fake prompt directives instead of provider-shaped LLM fixtures, masking Stage contracts, exact file guards, and provider batching behavior. | Replaced affected file, write, agent-switch, and batch cases with bounded SSE fixtures and exact hashes. Engineer switching now completes Survey, Plan, Implement, Verify, and final-response turns end to end. |
| AW-15 | Medium | Mock provider listeners could block forever waiting for an unused response, making failures look like runtime hangs. | Listeners are nonblocking with a five-second idle deadline; accepted streams switch back to blocking mode and report the exact served-response count. |
| AW-16 | Medium | Runtime-stream and Workflow-agent tests reused fixed project/run/definition identities while project database paths are process-global. Test order could produce uniqueness failures. | Generate unique fixture project and run identities from each temporary root. Persisted stream replay and custom Workflow-agent catalog tests now pass together and in the full suite. |
| AW-17 | Medium | Two core agent start/send UI tests were skipped after the composer model-selection contract changed. Their stale fixtures silently removed coverage from first-run provider binding. | Updated them to use canonical composer model options and the expanded control envelope, then enabled them. The frontend matrix now has 1,230 passes and zero skips. |
| AW-18 | Low | Wall-clock, schema sample, tool inventory, async-wait, and workspace-index fixtures had become stale or asserted snapshots that could precede the intended continuation. | Use current RFC3339 fixture times, valid schema samples, current tool names, supported indexed files, predicate-based waits, and last-error diagnostics. |
| AW-19 | High | `FailureClassIs` with an explicit `nodeId` fell back to the latest failure when that node had no recorded failure. An unrelated node could therefore satisfy a failure route. | Explicit node-scoped conditions now consult only that node; the latest failure is used only when no node is requested. Added positive, negative, and fallback regression cases. |
| AW-20 | High | An Artifact or State input binding with an explicit missing JSON path silently fell back to the producer's entire payload. Downstream agents could receive structurally invalid input while required-input validation appeared to pass. | Whole-payload binding is now limited to bindings with no path. Explicit missing paths stay missing and fail the required-input contract. Added Artifact and State regressions. |
| AW-21 | Medium | The shared JSON path evaluator rejected root-array expressions such as `$[0]`, although Workflow artifact rendering accepts JSON payloads whose root is an array. | Added root-array traversal while preserving strict rejection of malformed, trailing, nonnumeric, and out-of-range segments. Root-object and root-array paths are both covered. |
| AW-22 | Medium | Provider errors containing `authorization` matched the generic `auth` branch first and were reported as authentication failures. | Authorization signals now take precedence over generic authentication matching. The classifier table now directly covers credit, authentication, authorization, model, rate-limit, network, and unknown failures. |
| AW-23 | High | Capability reports accepted `skill_runtime_tool` and `attached_skill_context` in the frontend, but the native command/store allowlists and SQLite `CHECK` constraint rejected them. Valid reports could therefore fail only after crossing the Tauri or persistence boundary. | Unified all three allowlists, updated both current and baseline schemas, and advanced the schema epoch to 55 so incompatible app-data is rebuilt under the new-application policy. Rust command/store and frontend regressions cover both subjects. |
| AW-24 | High | Handoff summaries allowed multiple selectors (`handoffId`, `targetRunId`, and `sourceRunId`) at once. Different layers could choose different selectors and return an unintended handoff. | Require exactly one nonblank selector in both TypeScript and Rust. Added zero-selector, multi-selector, and valid-selector tests. |
| AW-25 | High | Retrying an already accepted cross-agent routing switch attempted to create a second target run and handoff, turning a successful request into a conflict. | Replay now verifies lineage, target agent, definition hash, and recovery payload, then reuses the durable target run and handoff. Exact replay creates no duplicates; mismatched replay fails closed. |
| AW-26 | High | A provider that repeatedly returned a final answer before satisfying required Stage checks could be reprompted forever. | Permit one corrective Stage-gate reprompt; a second no-progress final now terminates with `agent_stage_incomplete`. Successful tool dispatch resets the guard. Provider-loop and continuity regressions cover both paths. |
| AW-27 | Medium | Fenced handoff JSON could be scanned as a filesystem path, producing spurious path-policy failures. | Strip fenced blocks before inline path extraction and reject multiline path candidates while preserving legitimate inline paths. Added both positive and negative descriptor tests. |
| AW-28 | High | Reconfiguring app-data database paths cleared only connection state, leaving project-path and project-key maps pointed at the prior app-data root. | `configure_project_database_paths` now atomically clears all derived database maps. A temporary-root regression proves the second configuration cannot reuse the first root. |
| AW-29 | High | Compare-and-swap updates that expected derived `Stale` status could not match raw `starting`, `running`, or already `stale` rows even when every other expected field matched. Runtime recovery could fail with a false write conflict. | CAS matching now treats those raw states as the persisted representation of derived `Stale`, with unit and runtime-persistence integration coverage. |
| AW-30 | High | Workflow collection resume paths were accepted without checking the path language later used by the runtime writer. Array paths, whitespace-only segments, and malformed object paths could validate and then fail during execution. | Added the same strict object-path contract to the definition validator, Tauri write boundary, and Zod model: `$` or dot-separated object fields only, with outer whitespace normalized. Rust and frontend regressions cover root, nested, and invalid paths. |
| AW-31 | High | Delivery-resume lookup stopped at a blank or null `phaseKey`, so a valid fallback `phaseId`/`nodeId` was ignored. | Candidate selection now skips unusable keys with `find_map` and continues through the documented fallback order. Tests cover blank, null, ID fallback, status, and ordering variants. |
| AW-32 | Medium | Integration fixtures lagged the continuation snapshot contract and reused global app-data registry locations, making valid behavior fail depending on test order. | Added continuation IDs and expected snapshots to runtime/session fixtures, corrected the remote-bridge registry location, and made cross-agent lineage setup explicit. The affected focused suites and the complete desktop library now pass together. |
| AW-33 | Medium | Session-memory fixtures treated manual extraction as immediate even though the product correctly gates it for review, and compaction assertions looked at an obsolete action snapshot. | Fixtures now use realistic Symptom/Fix content, assert pending/disabled state, explicitly approve extraction, and inspect the current unresolved action set. |
| AW-34 | Critical | Headless `suggest`, `auto_edit`, and `yolo` modes exposed the same mutating tool surface. Read-only Crawl and Computer Use agents were also classified as write-capable. | Tool registration now derives file and command capability separately from both the selected agent and approval mode: Suggest is read-only, Auto Edit permits file mutations only, and Yolo adds commands. A three-mode mock-provider fixture asserts the exact registry surface. |
| AW-35 | High | `--approval-mode` accepted arbitrary values, normal execution silently defaulted to undocumented `on_request`, and response metadata always reported `on_request` instead of the effective policy. | Normalize and validate the public modes before creating state, default to Suggest, and project the effective mode in `sandboxDefaults`. Invalid-mode and metadata regressions pass. |
| AW-36 | Critical | A headless continuation restarted provider turn numbering at zero, colliding with the original context-manifest ID. Even without the collision it reconstructed Suggest/no-thinking controls instead of the admitted run controls. | Derive the next turn from durable tool-registry events and restore approval/thinking controls from `run_started`. The continuation fixture proves unique manifests, retained Yolo tools, and retained OpenAI reasoning effort. |
| AW-37 | High | HTTP/provider failures escaped the headless provider loop while leaving the durable run in `running`, producing a ghost active run. | Provider-turn failures now atomically project a Failed status and typed `run_failed` event before returning the original error. A 503 fixture verifies both the caller error and persisted terminal state. |
| AW-38 | High | `conversation retry` created a new run with `on_request` and no thinking effort, losing the original execution policy even after continuation was fixed. | Retry controls are reconstructed from the original durable `run_started` event, with CI still forcing Strict. The fixture verifies Yolo file/command tools plus the retained thinking effort. |
| AW-39 | High | The headless OpenAI API request path accepted and persisted `--thinking-effort` but omitted `reasoning_effort` from the provider request. | OpenAI-compatible OpenAI API requests now transmit the admitted effort. The capturing HTTP fixture asserts the continuation request body contains `reasoning_effort: high`. |
| AW-40 | Medium | The root `agent-system:verify-plan` command and support diagnostics referenced a plan, release checklist, and dogfood notes that had been deleted months earlier. The command failed with `ENOENT` and diagnostics advertised nonexistent evidence. | Retired the obsolete command/script and legacy S70 diagnostics entry, replaced the README link with this current audit, and added a support-bundle regression that permits only the three live deferred surfaces. |
| AW-41 | High | A process-output wakeup matched each polling response independently and inserted newlines between output chunks. A regex spanning either a chunk boundary or two polls could be missed forever. | Persist a bounded, Unicode-safe rolling output window, append chunks byte-for-byte, and evaluate the regex over the complete window. Fixtures cover same-poll and cross-poll boundaries plus the 64K bound. |
| AW-42 | High | Process status/output lookup used `?` before the scheduler's documented not-found recovery branch. If cleanup removed the process first, the wakeup errored instead of resuming the run with `process_state_missing`. | Normalize both manager invocation and typed-output conversion before matching not-found. Real-process fixtures cover natural exit, explicit cleanup, pending output, readiness, and terminal run cancellation. |
| AW-43 | Critical | The durable runtime emitted `route_requested`, but `AgentRunEventKindDto` omitted that variant. Converting such an event for the desktop bridge entered an unreachable arm and could panic. | Added `RouteRequested` to the Rust DTO and TypeScript schema. The complete DTO conversion file is now 100% covered and the frontend runtime-event parser has a direct regression. |
| AW-44 | High | Unknown persisted wakeup kinds/statuses silently decoded as `sleep`/`pending`, allowing corrupt or future-incompatible state to execute with different semantics. | Decode every known value explicitly and return a typed SQLite conversion failure for anything else. Corruption fixtures bypass the database check constraint and prove reads fail closed. |
| AW-45 | High | Workflow driver update fingerprints omitted node runtime links and event contents. Legitimate durable changes could therefore share a fingerprint and suppress UI/event updates. | Fingerprint the complete deterministic serialized run projection, retaining a nonpanicking diagnostic fallback. Fixtures prove link, event, lease, and error-counter changes invalidate the fingerprint. |
| AW-46 | Critical | Headless HTTP endpoint admission used string-prefix matching, so hosts such as `localhost.evil.example` and `127.example.com` were treated as local and allowed without TLS. | Parse the URL and require an exact `localhost`, loopback IP, or `0.0.0.0` host. Security regressions cover prefix-confusion hosts and valid local endpoints. |
| AW-47 | High | The production headless `write` tool used the nonblank-string validator for file content, preventing creation or truncation of an intentionally empty file. | Split required-path validation from required-but-empty-allowed content validation. A full registry dispatch fixture proves an empty write succeeds with zero bytes. |
| AW-48 | High | Durable agent stream liveness used `state == scheduled_wait OR stopReason == scheduled_wait`. Contradictory persisted markers could therefore keep a stream alive even when one marker required user action or approval. | When both markers exist they must agree on `scheduled_wait`; a single marker remains supported. A contradictory-marker regression proves the stream fails closed. |
| AW-49 | Critical | `FileAgentCoreStore` mutated its in-memory state before persisting. If directory preparation, serialization, or rename failed, the caller received an error while the rejected run/message/event/status mutation remained visible in memory. | Snapshot the committed state and restore it on every persistence failure. Fixture-backed tests force all mutators to fail, verify no leaked state, and reopen the last committed disk snapshot. |
| AW-50 | High | Rejected in-memory message and event appends incremented their IDs before checking that the target run existed. Invalid writes consumed durable sequence numbers and introduced unexplained gaps. | Validate run/trace ownership before allocating IDs. Missing-run regressions prove the next successful message and event both receive the first ID. |
| AW-51 | High | On macOS, a short-lived owned process could exit between process-group discovery and signaling. The stale group signal could return `EPERM`, causing cleanup to report failure even though the owned root had already exited. | Recheck and reap the root on `EPERM`; suppress the signal error only when exit is proven, while retaining the error for a live root. Deterministic live/exited fixtures and the shell lifecycle regression pass. |
| AW-52 | Medium | Coverage-instrumented runtime and GSD fixtures used production-scale 90/120-second wall clocks and released shared runtime guards before supervisors deregistered. Correct long paths could time out or inherit prior background work. | Drain every completed supervisor before releasing the fixture guard and give only the multi-turn coverage fixtures a 300-second harness budget. Production provider, tool-group, and workflow safety budgets were not changed. Both instrumented end-to-end suites pass in the final merge. |
| AW-53 | High | Reusable fake and real headless continuations accepted whitespace-only prompts. The fake facade persisted an empty user turn; the real provider path could also begin mutating durable state. | Validate continuation text before loading or mutating a run. Regressions prove blank continuations return the typed required-field error and leave snapshots byte-for-byte unchanged. |
| AW-54 | High | Typed protocol envelopes and control submissions accepted blank submission IDs/timestamps, grant IDs, tool names, provider/model IDs, and optional project/run IDs. Some invalid requests were misreported as not-found while others emitted malformed events. | Validate envelope identity, trace shape, and every variant-specific identifier before dispatch. The protocol matrix covers every submission variant and proves rejected grants/provider changes append no event. |
| AW-55 | High | Reusable fake and headless approval/rejection facades accepted blank action IDs, producing approval text or policy decisions that could not be correlated to an action. | Require project, run, and action identity before either approval path. Both facades now reject blank action IDs without changing the run. |
| AW-56 | Medium | A failed atomic file-store rename left the `.json.tmp` payload behind even though in-memory state was rolled back. Stale temporary state could confuse inspection or a later recovery attempt. | Remove the temporary payload on every replace/rename failure while preserving the original typed error. The commit-failure fixture blocks the final path with a directory and verifies both cleanup and memory rollback. |
| AW-57 | High | Real-provider continuation ran preflight after changing status and appending the user message/event. A blocked provider therefore returned an error but left the run `running` with a continuation that was never dispatched. | Complete injected/live preflight and blocker admission before any durable mutation. The blocked-preflight fixture proves the complete persisted snapshot remains unchanged. |
| AW-58 | High | Public stores accepted malformed explicit trace IDs, malformed trace contexts, and valid trace contexts owned by a different run. Exported timelines could consequently become invalid or non-replayable. | Validate explicit run traces and trace ownership before allocating IDs or persisting events/manifests in both stores. Rejected writes do not consume sequence numbers. |
| AW-59 | High | The file-backed headless store trusted decoded run identity, nested ownership, trace ownership, and next-ID counters. Corrupt state could reopen successfully and later reuse message/event IDs. | Reopen now validates required run identity/version, nested record ownership, trace validity, global positive ID uniqueness, and counters at or above persisted maxima. Fixture mutations cover every invariant and prove the original valid snapshot still reopens. |
| AW-60 | High | Core stores allowed duplicate context-manifest IDs within a project, including across runs. The desktop SQLite contract rejects the same state, and duplicate IDs make manifest lookup and trace binding ambiguous. | Enforce project-scoped manifest identity before mutation in both stores and during file reopen. Fixtures cover same-run duplicates, cross-run duplicates, rollback, durable reopen, and the original valid state. |
| AW-61 | High | Live manifest writes accepted a session that did not own the run, blank provider/model identities, and a valid same-run trace derived from a different manifest. File state could also contain dangling event boundaries and noncanonical manifest traces. | Validate run/session ownership, required provider identity, exact canonical manifest trace derivation, and same-run event boundaries at write and reopen boundaries. Public generic-store and JSON corruption fixtures fail closed without mutation. |
| AW-62 | High | Run insertion accepted zero or negative agent-definition versions, while file reopen rejected them. Persisted runs also silently defaulted missing agent/definition/system-prompt provenance to an `engineer` v1 identity. | Require positive versions on both live stores and remove persisted provenance defaults. Missing required provenance now produces a typed decode failure instead of fabricating an identity. |
| AW-63 | High | Message/event ID allocation used saturating arithmetic. A valid store counter at `i64::MAX` could append repeated maximum IDs and corrupt global identity. | Use checked allocation with a typed exhaustion error. A durable counter-exhaustion fixture proves both append paths fail and leave the run unchanged. |
| AW-64 | High | Rust dev builds still allowed incremental state and emitted line-table debug data, test/app wrappers could inherit `CARGO_INCREMENTAL=1`, and retained coverage/test artifacts grew the workspace target to 39 GiB. | Disable incremental compilation in Cargo config and every profile/wrapper, omit dev/test debug data, strip release symbols, shorten routine test-artifact retention, clean 38.8 GiB of invalidated output, and verify the rebuilt target is 4.3 GiB with no incremental entries. |
| AW-65 | Medium | Store lookups, status updates, and project listings handled blank identifiers as missing records or empty results; an empty file-store path was accepted until a later mutation failed. | Validate identities at the public boundary before locking or mutating. Generic in-memory/file fixtures prove typed rejection and unchanged durable state; relative paths remain supported. |
| AW-66 | Critical | Mutating desktop tool handlers executed in a post-`fork` child even though they own shared mutexes, SQLite-backed state, and asynchronous process registries. A multithreaded parent could fork while one of those locks was held, leaving the child permanently blocked; LLVM coverage exposed this as a 120-second GSD edit timeout followed by a denied Verify Stage. | Execute every mutating desktop handler in the supervising parent process while retaining policy, sandbox, checkpoint, rollback, budget, and cooperative-cancellation gates. A focused admission regression and the normal and instrumented GSD Auto fixtures pass; the instrumented run completes in 60.64 seconds. |

## End-to-end GSD evidence

`gsd_auto_runs_all_phases_with_fixture_llm_responses_and_archives_the_milestone` passes normally and under LLVM coverage on the final code. The authoritative merged coverage run completes the full fixture in 56.04 seconds.

The fixture verifies:

1. the checked-in GSD Workflow definition validates through the Rust registry validator;
2. Workflow nodes receive deterministic provider responses in the expected order;
3. Plan produces its required plan artifact;
4. Engineer completes Survey, Plan, Implement, and Verify Stages with guarded file edits;
5. Debug records reproduction and hypothesis evidence, applies its fix, and verifies it;
6. routing, loop/checkpoint, state, and artifact contracts reconcile durably;
7. the Workflow reaches its terminal status and archives the milestone.

This is a real HTTP/SSE provider-adapter path with local fixtures, not the fake prompt-directive provider. The fixture-owned end-to-end deadline is 300 seconds so profiler overhead cannot expire the mock provider midway through its 76-request contract. Production provider and tool-group safety budgets were not weakened. The instrumented fixture consumes every expected response and the complete merged profile is green.

## Final verification matrix

| Surface | Result |
| --- | ---: |
| `xero-agent-core` unit tests | 164 passed |
| `xero-agent-core` provider/protocol integrations | 10 passed |
| Complete `xero-cli` / headless harness matrix | 206 passed |
| Desktop Rust library | 1,591 passed |
| Owned-agent runtime integration | 55 passed |
| Workflow execution, including GSD Auto | 4 passed normally |
| Workflow agent catalog/detail | 10 passed |
| Agent coordination/mailbox/reservation | 12 passed |
| Agent context continuity | 12 passed |
| Runtime-run persistence | 9 passed |
| Session-history commands | 11 passed |
| Agent-run wakeups | 1 passed |
| Added agent report command integration | 1 passed |
| Added agent default-model command integrations | 2 passed |
| Added agent definition command integration | 1 passed |
| Added Agent Tooling settings integration | 1 passed |
| Added agent task command integrations | 2 passed |
| Complete client frontend matrix | 122 files; 1,410 passed; 0 skipped |
| Complete shared-UI frontend matrix | 16 files; 127 passed; 0 skipped |
| TypeScript typecheck | Passed |
| Targeted frontend ESLint | Passed |
| Rust formatting | Passed |
| Rust Clippy with known unrelated structural lints suppressed | Passed with `-D warnings` |
| `git diff --check` | Passed |

Key commands:

```bash
cd client/src-tauri
cargo test -p xero-agent-core -- --test-threads=1
cargo test -p xero-cli -- --test-threads=1
cargo test --lib -- --test-threads=1
cargo test --test agent_core_runtime -- --test-threads=1
cargo test --test workflow_run_execution -- --test-threads=1
cargo test --test workflow_agents -- --test-threads=1
cargo test --test agent_coordination -- --test-threads=1
cargo test --test agent_context_continuity -- --test-threads=1
cargo test --test runtime_run_persistence -- --test-threads=1
cargo test --test session_history_commands -- --test-threads=1
cargo test --test agent_run_wakeups -- --test-threads=1
cargo test --test agent_report_commands -- --test-threads=1
cargo test --test agent_default_model_commands -- --test-threads=1
cargo test --test agent_definition_commands -- --test-threads=1
cargo test --test agent_tooling_settings_commands -- --test-threads=1
cargo test --test agent_task_commands -- --test-threads=1
rustup run stable cargo llvm-cov --lib --json --output-path /tmp/xero-desktop-lib-coverage-goal.json -- --test-threads=1
cargo llvm-cov -p xero-agent-core --json --output-path /tmp/xero-agent-core-coverage-goal.json -- --test-threads=1
rustup run stable cargo llvm-cov --workspace --exclude xero-desktop --exclude xero-remote-bridge --exclude xero-desktop-control-ipc --exclude xero-redaction --exclude xero-cursor-sidecar --exclude xero-desktop-sidecar --json --output-path /tmp/xero-headless-workspace-coverage-audit.json -- --test-threads=1
rustup run stable cargo llvm-cov -p xero-agent-core -p xero-cli --json --output-path /tmp/xero-headless-merged-coverage-2026-07-18.json -- --test-threads=1
rustup run stable cargo llvm-cov --lib --test agent_task_commands --test agent_core_runtime --test workflow_run_execution --test workflow_agents --test workflow_graph_persistence --test agent_coordination --test agent_context_continuity --test runtime_run_persistence --test session_history_commands --test agent_run_wakeups --test agent_report_commands --test agent_default_model_commands --test agent_definition_commands --test agent_tooling_settings_commands --json --output-path /tmp/xero-agent-workflow-merged-coverage-continued-2026-07-18.json -- --test-threads=1
cargo clippy --lib -- -D warnings -A clippy::too-many-arguments -A clippy::large-enum-variant -A clippy::needless-borrow -A clippy::question-mark -A clippy::needless-return -A unfulfilled-lint-expectations

cd ../
pnpm exec vitest run
pnpm exec tsc --noEmit
pnpm exec eslint src/lib/xero-model/agent-reports.ts src/lib/xero-model/agent-reports.test.ts src/lib/xero-model/workflow-definition.ts src/lib/xero-model/workflow-definition.test.ts

cd ../../packages/ui
pnpm exec vitest run
pnpm exec tsc --noEmit
```

## Remaining coverage boundaries

The remaining uncovered source is concentrated in paths that cannot all be made deterministic on this host:

- Windows/Linux sandbox and process-tree implementations;
- live provider authentication, rate limits, outages, and vendor-specific streaming failures;
- desktop/browser/device permission states that require real OS services;
- true process death during narrow persistence windows;
- large defensive schema/error branches and inline test code counted by LLVM.

These are not evidence that coverage is complete. They explain why forcing the source percentage toward 100% would require either nonrepresentative tests or a multi-platform, credentialed fault-injection environment. The release-critical deterministic contracts are now directly exercised; multi-platform CI and live-provider canaries remain the appropriate next layer.
