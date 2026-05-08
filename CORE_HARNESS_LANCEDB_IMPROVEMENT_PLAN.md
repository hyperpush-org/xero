# Core Harness and LanceDB Improvement Plan

This plan is for core runtime/harness work only. Do not add UI, temporary UI, browser-only flows, or product-surface changes while completing these slices. Each slice must be small enough for one agent to finish reliably, and each completed slice must include evidence before its checkbox is marked.

## Completion Rules

- Mark a slice complete only after the implementation is present in the working tree and the evidence line is filled in with concrete output, test names, file paths, or command results.
- Keep evidence scoped. Prefer targeted Rust tests and focused format/lint commands over repo-wide runs.
- Do not create branches or stash unless the user asks.
- Run only one Cargo command at a time.
- New project state must remain under OS app-data project state. Do not introduce `.xero/` writes.
- This is a new application. Do not add backwards compatibility unless the user explicitly requests it.

Use this checkbox pattern:

```md
- [x] S01 - Slice title
  Evidence: `cargo test ...` passed; changed `path/to/file.rs`; observed `fieldName` in persisted manifest.
```

## Investigation Summary

The relevant code was inspected in these areas:

- Core reusable harness crate: `client/src-tauri/crates/xero-agent-core/src/lib.rs`, `production_runtime.rs`, `headless_runtime.rs`, `tool_registry.rs`, `sandbox.rs`, `protocol.rs`.
- Desktop owned runtime: `client/src-tauri/src/runtime/agent_core/run.rs`, `facade.rs`, `provider_loop.rs`, `context_package.rs`, `tool_dispatch.rs`, `tool_descriptors.rs`, `harness_contract.rs`, `harness_order.rs`, `evals.rs`, `persistence.rs`.
- Runtime command bridge: `client/src-tauri/src/commands/runtime_support/run.rs` and related runtime command/stream code.
- LanceDB and retrieval: `client/src-tauri/src/db/project_store/agent_memory_lance.rs`, `project_record_lance.rs`, `agent_retrieval.rs`, `agent_embeddings.rs`, `freshness.rs`, `agent_memory.rs`, `project_record.rs`, `agent_continuity.rs`.
- Current tests: `client/src-tauri/tests/agent_core_runtime.rs`, `client/src-tauri/tests/lancedb_freshness_phase1.rs`, `client/src-tauri/src/runtime/agent_core/evals.rs`, and module tests in the files above.

Current behavior observed from code:

- Production runtime contracts already separate real provider runs from fake/headless harness storage. Real provider mode requires app-data `state.db`; fake harness mode may use in-memory or file-backed harness stores.
- The desktop owned runtime persists app-data project state through SQLite plus per-project LanceDB datasets under the project app-data directory.
- Provider turns assemble a context package before provider submission. The package retrieves project records, records query/result ids, persists a context manifest, and explicitly marks durable context as `tool_mediated` with `rawContextInjected: false`.
- `project_context` is the actual model-facing access path for durable project context. It supports search, get, manifest explanation, record, update/supersede, and freshness refresh actions, with runtime-agent write restrictions.
- LanceDB project records validate/reset stale table schemas before use. LanceDB agent memory does not yet have the same schema-reset guard.
- Retrieval uses deterministic local hash embeddings plus keyword/metadata/freshness scoring. This is useful and deterministic, but it should be treated as lexical/hash retrieval rather than true provider-semantic retrieval.
- Freshness is real source-fingerprint based state: current, source_unknown, stale, source_missing, superseded, and blocked.
- Current tests cover many LanceDB freshness, manifest, no-raw-prompt, project_context, backfill, and ranking cases. The quality/eval CLI does not yet make LanceDB/context-manifest behavior a first-class required quality gate.
- Tool Registry V2 validates object shape, required fields, and top-level primitive types. It does not fully enforce JSON Schema semantics such as enums, nested properties, arrays/items, bounds, or additionalProperties.

## Milestone 1 - Baseline Contracts and Non-UI Diagnostics

- [ ] S01 - Add a core runtime contract inventory test
  - Goal: Add a focused test that exports or inspects the harness contract and asserts the presence of provider modes, app-data store requirements, Tool Registry V2 descriptors, project_context action-level tools, and context manifest metadata fields.
  - Suggested files: `client/src-tauri/tests/agent_core_runtime.rs`, `client/src-tauri/src/runtime/agent_core/harness_contract.rs`.
  - Evidence: targeted Cargo test command and assertions covering production real-provider store, fake-provider harness store, `project_context_search`, `project_context_get`, and `retrieval.rawContextInjected`.

- [ ] S02 - Make runtime diagnostic events complete without adding UI
  - Goal: Ensure provider preflight, context package creation, retrieval query ids, tool registry snapshot ids, and run stop reasons are all persisted as structured events or manifest fields, not ad-hoc console-only output.
  - Suggested files: `client/src-tauri/src/runtime/agent_core/run.rs`, `provider_loop.rs`, `commands/runtime_support/run.rs`.
  - Evidence: targeted test showing a run snapshot contains provider preflight hash, context manifest id, retrieval query ids, tool registry snapshot, and final stop reason.

- [ ] S03 - Remove or gate production `eprintln!` runtime latency logs
  - Goal: Replace unconditional stderr logging in runtime startup/drive paths with structured persisted diagnostics or a test-only trace gate.
  - Suggested files: `client/src-tauri/src/commands/runtime_support/run.rs`.
  - Evidence: `rg -n "eprintln!" client/src-tauri/src/commands/runtime_support client/src-tauri/src/runtime/agent_core` has no unconditional production runtime-latency logging, plus a targeted runtime test still passes.

## Milestone 2 - LanceDB Storage Hardening

- [ ] S04 - Add agent-memory Lance schema drift recovery
  - Goal: Give `agent_memory_lance` the same stale-schema detection/reset behavior that `project_record_lance` already has.
  - Suggested files: `client/src-tauri/src/db/project_store/agent_memory_lance.rs`.
  - Evidence: unit test creates a legacy `agent_memories` table missing current columns, then `list` and `insert` succeed after reset.

- [ ] S05 - Add shared Lance table health helpers
  - Goal: Reduce drift between project-record and memory Lance table setup by sharing schema validation, connection reset, and error wording where practical.
  - Suggested files: `agent_memory_lance.rs`, `project_record_lance.rs`, or a small shared module under `project_store`.
  - Evidence: both Lance stores still pass their round-trip tests; schema drift tests exist for both tables.

- [ ] S06 - Make Lance scan/update costs measurable
  - Goal: Add non-UI diagnostics for `scan_all`, delete+insert replacement, and refresh passes so future performance work has evidence.
  - Suggested files: `agent_memory_lance.rs`, `project_record_lance.rs`, `agent_retrieval.rs`.
  - Evidence: tests assert diagnostic counters or timing metadata are emitted/persisted without changing returned retrieval semantics.

- [ ] S07 - Preserve app-data-only state invariants with regression tests
  - Goal: Add tests that insert records/memory, run retrieval, and verify Lance datasets live beside app-data `state.db` and never create `.xero/`.
  - Suggested files: `project_record.rs`, `agent_memory.rs`, `lancedb_freshness_phase1.rs`.
  - Evidence: targeted tests assert `database_path_for_repo`, Lance dataset directory, and absence of repo-local `.xero/`.

## Milestone 3 - Retrieval and Freshness Correctness

- [ ] S08 - Rename or document local hash retrieval semantics in code contracts
  - Goal: Make it impossible to confuse local hash embeddings with provider semantic embeddings in harness reports and diagnostics.
  - Suggested files: `agent_embeddings.rs`, `agent_retrieval.rs`, `harness_contract.rs`, `evals.rs`.
  - Evidence: tests assert the model/version strings and diagnostics say local hash or deterministic lexical/hash retrieval, not provider semantic retrieval.

- [ ] S09 - Add retrieval score breakdown contract tests
  - Goal: Assert each retrieval result carries keyword score, vector score, freshness metadata, trust envelope, and citation, for both project records and approved memory.
  - Suggested files: `agent_retrieval.rs`, `lancedb_freshness_phase1.rs`.
  - Evidence: targeted tests validate metadata fields for `ProjectRecords`, `ApprovedMemory`, and `HybridContext` searches.

- [ ] S10 - Add filtered retrieval edge-case tests
  - Goal: Cover tags, related paths, record kinds, memory kinds, runtime agent filters, created-after, min importance, and limit clamping.
  - Suggested files: `agent_retrieval.rs`, `lancedb_freshness_phase1.rs`.
  - Evidence: tests prove matching rows are included, nonmatching rows are excluded, and logs preserve filters JSON.

- [ ] S11 - Harden source-fingerprint path matching
  - Goal: Review and test relative/absolute path normalization, path overlap, missing file handling, and directory/non-file behavior.
  - Suggested files: `freshness.rs`, `agent_memory.rs`, `project_record.rs`.
  - Evidence: freshness unit tests cover absolute paths inside repo, parent-dir attempts, directories, deleted files, changed hashes, and source_unknown rows.

- [ ] S12 - Add blocked/redacted retrieval log contract tests
  - Goal: Prove blocked records are not exposed to model-visible results, while diagnostics can still count them safely.
  - Suggested files: `agent_retrieval.rs`, `project_context.rs`, `lancedb_freshness_phase1.rs`.
  - Evidence: test shows blocked row excluded from `project_context` output, diagnostic count present, and no blocked text in persisted model-visible tool content.

## Milestone 4 - Provider Context Package and `project_context`

- [ ] S13 - Add provider-turn retrieval-to-manifest end-to-end smoke
  - Goal: Start an owned fake-provider run with seeded Lance records, force a provider turn, and assert context package retrieval logs, manifest retrieval fields, and no raw durable text in prompt.
  - Suggested files: `client/src-tauri/tests/lancedb_freshness_phase1.rs`, `agent_core_runtime.rs`.
  - Evidence: targeted test passes and asserts query id, result id, `deliveryModel: tool_mediated`, `rawContextInjected: false`, freshness diagnostics, and no raw seeded text in provider prompt.

- [ ] S14 - Add `project_context_search` to `project_context_get` round-trip smoke
  - Goal: Prove a model-visible search result citation can be followed by get, with both actions logged as retrieval queries/results.
  - Suggested files: `project_context.rs`, `lancedb_freshness_phase1.rs`.
  - Evidence: test searches a seeded record, gets by returned id, and validates two persisted retrieval query logs with source-cited result logs.

- [ ] S15 - Add context manifest explanation trace coverage
  - Goal: Ensure `explain_current_context_package` returns a compact manifest, omits raw prompt/message/schema bodies, preserves citation, and logs manual retrieval.
  - Suggested files: `project_context.rs`, `provider_loop.rs`, `lancedb_freshness_phase1.rs`.
  - Evidence: targeted test asserts compact summary shape and absence of raw prompt fragments, messages, tool schema descriptions, and provider preflight secrets.

- [ ] S16 - Add project_context write restriction matrix tests
  - Goal: Lock down allowed/forbidden write actions by runtime agent: Ask/Engineer/Debug/Test, Plan accepted plan-pack only, Crawl read-only, AgentCreate definition-registry only.
  - Suggested files: `project_context.rs`, `types.rs`, `tool_descriptors.rs`.
  - Evidence: tests validate decode, registry exposure, policy decision, and execution result for each runtime agent/action pair.

- [ ] S17 - Make update/supersession evidence complete
  - Goal: Ensure `update_context` from record and memory targets captures supersession links, freshness state, source ids, related paths, and retrieval visibility correctly.
  - Suggested files: `project_context.rs`, `project_record.rs`, `agent_memory.rs`.
  - Evidence: targeted tests cover record supersession and memory correction; old context becomes superseded, new context cites old id.

## Milestone 5 - Tool Registry V2 Safety and Usability

- [ ] S18 - Enforce enum and additional-property validation in Tool Registry V2
  - Goal: Reject provider tool inputs that violate declared enums or include undeclared properties when schemas say to do so.
  - Suggested files: `client/src-tauri/crates/xero-agent-core/src/tool_registry.rs`, `tool_descriptors.rs`.
  - Evidence: unit tests reject an invalid `project_context_search.action` and an unexpected field when `additionalProperties: false` is declared.

- [ ] S19 - Enforce nested object and array item validation
  - Goal: Validate nested object fields, array item types, and primitive bounds used by runtime tool schemas.
  - Suggested files: `tool_registry.rs`.
  - Evidence: unit tests reject invalid arrays, nested wrong-type values, out-of-range integers where min/max is declared, and still accept valid existing sample calls.

- [ ] S20 - Add schema validation compatibility tests for every built-in descriptor
  - Goal: Build a table of representative valid and invalid sample calls for core harness tools, especially project_context, command, filesystem, tool_search, tool_access, and harness_runner.
  - Suggested files: `agent_core_runtime.rs`, `tool_descriptors.rs`, `tool_registry.rs`.
  - Evidence: targeted test validates every built-in descriptor can pass a valid sample and fail at least one meaningful invalid sample.

- [ ] S21 - Add tool registry failure persistence tests
  - Goal: Prove invalid-input, policy-denied, approval-required, sandbox-denied, timeout, and doom-loop failures persist enough machine-readable details for later diagnosis.
  - Suggested files: `tool_dispatch.rs`, `agent_core_runtime.rs`, `tool_registry.rs`.
  - Evidence: targeted tests inspect persisted tool call rows/events and assert category, code, model message, policy/sandbox metadata, and doom-loop signal.

## Milestone 6 - Headless Harness and Real-Provider Boundary

- [ ] S22 - Clarify headless real-provider storage contract
  - Goal: The reusable core headless runtime must either provide a real app-data `AgentCoreStore` adapter or fail with a direct, documented error when real provider mode is paired with file-backed harness JSON.
  - Suggested files: `xero-agent-core/src/headless_runtime.rs`, `production_runtime.rs`, crate README/docs if present.
  - Evidence: tests cover accepted fake/file harness mode, rejected real/file mode, and accepted real/app-data store mode if an adapter is added.

- [ ] S23 - Add headless real-provider preflight failure tests
  - Goal: Prove real provider runs fail before network submission when provider config, required features, CI mode, or production runtime contract are invalid.
  - Suggested files: `headless_runtime.rs`, `production_runtime.rs`, provider preflight modules.
  - Evidence: tests assert exact error codes and no run side effects beyond intended failed preflight records.

- [ ] S24 - Add trace export round-trip for context and LanceDB retrieval
  - Goal: Ensure runtime trace export includes context manifests, retrieval query/result ids, provider preflight hash, tool registry snapshots, and stop reason.
  - Suggested files: `facade.rs`, `environment_lifecycle.rs`, `agent_core_runtime.rs`.
  - Evidence: targeted test exports trace after a fake provider run and validates all fields needed to reproduce context package and retrieval decisions.

## Dependency Order

1. Do S01-S03 first to stabilize the baseline and diagnostics.
2. Do S04-S07 before retrieval performance or eval work, because storage drift can invalidate later evidence.
3. Do S08-S12 before provider-context expansion, because result metadata and freshness semantics are the contract provider turns depend on.
4. Do S13-S17 after storage/retrieval correctness is locked.
5. Do S18-S21 before broadening scripted evals, because stricter validation may change failure evidence.
6. Do S22-S24 for headless/trace parity once desktop-owned runtime behavior is well covered.
## Suggested Scoped Commands

Run these only as needed for the slice being completed:

```sh
cd client/src-tauri
cargo test -p xero-agent-core tool_registry --lib
cargo test --test lancedb_freshness_phase1 <test_name>
cargo test --test agent_core_runtime <test_name>
cargo run --bin xero-harness-evals -- --format json
cargo fmt -p xero-desktop
```

`protoc` is required on PATH for LanceDB-related builds. On this machine it was found at `/opt/homebrew/bin/protoc` during plan creation.
