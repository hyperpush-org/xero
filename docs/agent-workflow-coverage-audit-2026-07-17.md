# Agent, Stage, and Workflow Coverage Audit — 2026-07-17

## Outcome

This pass found and fixed critical defects in owned-agent continuation, Stage replay, subagent execution, mutation batching, approval replay, runtime-control reconciliation, and test isolation. A follow-up expansion added 35 desktop-library tests and fixed four more defects in Workflow condition scoping, input binding, JSON path evaluation, and provider error classification. The final deterministic test matrix is green with no skipped agent/Stage/Workflow frontend tests.

No known critical or high-severity finding from this pass remains open.

The original audited 36-file Rust surface measures:

| Metric | Covered | Total | Coverage |
| --- | ---: | ---: | ---: |
| Lines | 94,066 | 123,183 | 76.36% |
| Functions | 5,320 | 7,626 | 69.76% |
| Regions | 113,039 | 149,156 | 75.79% |

This is a conservative source metric, not a claim of blanket 100% coverage. LLVM includes inline test bodies, generated paths, host-specific branches, and provider/platform code that cannot all execute on one macOS host. The critical deterministic contracts listed below have direct regression or end-to-end evidence even where their containing file has lower aggregate coverage.

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

## End-to-end GSD evidence

`gsd_auto_runs_all_phases_with_fixture_llm_responses_and_archives_the_milestone` passed both normally and under LLVM instrumentation.

The fixture verifies:

1. the checked-in GSD Workflow definition validates through the Rust registry validator;
2. Workflow nodes receive deterministic provider responses in the expected order;
3. Plan produces its required plan artifact;
4. Engineer completes Survey, Plan, Implement, and Verify Stages with guarded file edits;
5. Debug records reproduction and hypothesis evidence, applies its fix, and verifies it;
6. routing, loop/checkpoint, state, and artifact contracts reconcile durably;
7. the Workflow reaches its terminal status and archives the milestone.

This is a real HTTP/SSE provider-adapter path with local fixtures, not the fake prompt-directive provider.

## Final verification matrix

| Surface | Result |
| --- | ---: |
| `xero-agent-core` unit tests | 128 passed |
| `xero-agent-core` provider/protocol integrations | 2 passed |
| Desktop Rust library | 1,554 passed |
| Workflow orchestrator focused unit surface | 97 passed |
| Owned-agent runtime integration | 55 passed |
| Workflow execution, including GSD Auto | 4 passed normally; 4 passed instrumented |
| Workflow agent catalog/detail | 10 passed |
| Agent coordination/mailbox/reservation | 12 passed |
| Frontend agent/Stage/Workflow matrix | 99 files; 1,230 passed; 0 skipped |
| TypeScript typecheck | Passed |
| Rust formatting | Passed |
| `git diff --check` | Passed |

Key commands:

```bash
cd client/src-tauri
cargo test -p xero-agent-core -- --test-threads=1
cargo test --lib -- --test-threads=1
cargo test --test agent_core_runtime -- --test-threads=1
cargo test --test workflow_run_execution -- --test-threads=1
cargo test --test workflow_agents -- --test-threads=1
cargo test --test agent_coordination -- --test-threads=1
rustup run stable cargo llvm-cov --lib --json --output-path /tmp/xero-desktop-lib-coverage-expanded.json -- --test-threads=1
rustup run stable cargo llvm-cov --no-clean --test workflow_run_execution --json --output-path /tmp/xero-agent-workflow-coverage-expanded.json -- --test-threads=1

cd ../
pnpm exec vitest run components/xero src/features/xero src/lib/xero-model lib/agent-attachments.test.ts lib/agent-workspace-layout.test.ts
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
