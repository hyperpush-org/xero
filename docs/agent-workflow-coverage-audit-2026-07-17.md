# Agent, Stage, and Workflow Coverage Audit — 2026-07-17

## Outcome

This audit found and fixed critical defects in owned-agent continuation, Stage replay, subagent execution, mutation batching, approval replay, runtime-control reconciliation, and test isolation. The first expansion added 35 desktop-library tests and fixed four defects in Workflow condition scoping, input binding, JSON path evaluation, and provider error classification. The current follow-up found and fixed another eleven product and fixture-contract issues, including a capability schema mismatch, non-idempotent handoff retries, an unbounded Stage-gate reprompt loop, invalid Workflow resume paths, and stale compare-and-swap handling.

The deterministic test matrix is green with no skipped agent/Stage/Workflow frontend tests. The original completed matrix included 1,568 desktop-library tests, 130 `xero-agent-core` tests, 114 focused Rust integrations, and 1,231 focused frontend tests. A continuation expansion added four fixture-backed Rust command integrations and seven frontend contract/adapter cases; the current complete frontend matrices pass 1,409 client tests and 127 shared-UI tests.

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

## End-to-end GSD evidence

`gsd_auto_runs_all_phases_with_fixture_llm_responses_and_archives_the_milestone` passes normally on the final code. It completes the full fixture in 53.87 seconds.

The fixture verifies:

1. the checked-in GSD Workflow definition validates through the Rust registry validator;
2. Workflow nodes receive deterministic provider responses in the expected order;
3. Plan produces its required plan artifact;
4. Engineer completes Survey, Plan, Implement, and Verify Stages with guarded file edits;
5. Debug records reproduction and hypothesis evidence, applies its fix, and verifies it;
6. routing, loop/checkpoint, state, and artifact contracts reconcile durably;
7. the Workflow reaches its terminal status and archives the milestone.

This is a real HTTP/SSE provider-adapter path with local fixtures, not the fake prompt-directive provider. A fresh LLVM-instrumented rerun was also attempted. Process-isolated edit workers spent enough time flushing profiler data to exhaust the production 120-second tool-group safety budget (49/76 fixture responses in the bounded attempt). The safety budget was not weakened to make coverage instrumentation pass. The normal end-to-end fixture is green, and the complete desktop-library and core coverage profiles completed successfully.

## Final verification matrix

| Surface | Result |
| --- | ---: |
| `xero-agent-core` unit tests | 128 passed |
| `xero-agent-core` provider/protocol integrations | 2 passed |
| Desktop Rust library | 1,568 passed |
| Owned-agent runtime integration | 55 passed |
| Workflow execution, including GSD Auto | 4 passed normally |
| Workflow agent catalog/detail | 10 passed |
| Agent coordination/mailbox/reservation | 12 passed |
| Agent context continuity | 12 passed |
| Runtime-run persistence | 9 passed |
| Session-history commands | 11 passed |
| Agent-run wakeups | 1 passed |
| Added agent report command integration | 1 passed |
| Added agent default-model command integration | 1 passed |
| Added agent definition command integration | 1 passed |
| Added Agent Tooling settings integration | 1 passed |
| Complete client frontend matrix | 122 files; 1,409 passed; 0 skipped |
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
rustup run stable cargo llvm-cov --lib --json --output-path /tmp/xero-desktop-lib-coverage-goal.json -- --test-threads=1
cargo llvm-cov -p xero-agent-core --json --output-path /tmp/xero-agent-core-coverage-goal.json -- --test-threads=1
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
