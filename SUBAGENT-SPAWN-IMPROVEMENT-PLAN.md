# Subagent Spawn Improvement Plan

## Goal

Make subagent spawning easier to trust, easier to observe, and harder to leave in a confusing state.

The feature already exists: top-level owned-agent runs can use the `subagent` tool to spawn one level of child owned-agent runs with role-scoped tool policy, write-set boundaries, delegated budgets, persisted task records, lineage, lifecycle stream events, and grouped UI transcript cards. This plan focuses on tightening rough edges, not replacing the design.

## Non-Goals

- Do not build the future multi-agent Workflow pipeline.
- Do not add recursive sub-subagent spawning.
- Do not rename existing Stage/Workflow DTOs or introduce user-facing "workflow phase" language.
- Do not add temporary debug UI.

## Current Gaps

1. Lifecycle events are incomplete for parent-side resolution actions.
   - `spawned`, `running`, and terminal child-run completion are emitted from the owned-agent executor.
   - `close`, `integrate`, and `interrupt` update persisted task state but do not consistently emit a new `subagent_lifecycle` event for the parent stream.
   - Result: the UI can lag behind the real task state.

2. Runtime stream and UI status sets do not fully match backend task statuses.
   - Backend task statuses include `closed` and `interrupted`.
   - Frontend stream schema and UI terminal status sets omit at least `closed` and `interrupted`.
   - Result: resolved tasks may remain visually active or render with generic styling.

3. Spawn prompt persistence is inconsistent.
   - Durable task storage keeps a prompt hash plus redacted/truncated preview.
   - Spawn lifecycle events include the raw `task.prompt` in `subagentPrompt`.
   - Result: a subagent prompt can be persisted and forwarded more verbosely than the task table intends.

4. Orphan recovery is unclear.
   - Durable task loading exists for resumed parent runs.
   - There is no obvious reconciliation step for stale `running`, `starting`, or `cancelling` child tasks after app/process interruption.
   - Result: completion gates may block on tasks that no longer have a live worker.

5. The UI is mostly read-only.
   - The subagent card shows status, budgets, prompt, child transcript, and result summary.
   - It does not expose obvious user actions like open child run, wait, cancel, integrate, close, or export trace.
   - Result: users must rely on the parent model to manage subagents.

6. End-to-end coverage is thin.
   - There are unit tests for policy and UI grouping.
   - There is no obvious full spawn-through-lifecycle integration test covering child run creation, forwarded events, budget/status updates, and final parent resolution.

## Implementation Slices

### Slice 1: Align Status Contracts

Update all status schemas and rendering sets to include every backend task status:

- `spawned`
- `pending`
- `registered`
- `starting`
- `running`
- `paused`
- `cancelling`
- `cancelled`
- `handed_off`
- `completed`
- `failed`
- `interrupted`
- `closed`
- `budget_exhausted`

Touch points:

- `packages/ui/src/model/runtime-stream.ts`
- `packages/ui/src/components/transcript/conversation-section.tsx`
- Relevant stream/model tests

Expected behavior:

- `closed` and `interrupted` render as terminal states.
- Unknown status fallbacks remain conservative.
- Terminal subagent cards auto-collapse consistently.

Verification:

- Run focused UI/model tests for runtime stream parsing and subagent card rendering.

### Slice 2: Emit Lifecycle Events For Resolution Actions

Emit `subagent_lifecycle` events whenever parent-side lifecycle state changes:

- `interrupt`
- `cancel`
- `close`
- `integrate`
- `send_input` / `follow_up` when status or input log changes materially
- `export_trace` only if useful as an activity event, not necessarily lifecycle

Touch points:

- `client/src-tauri/src/runtime/autonomous_tool_runtime/priority_tools.rs`
- `client/src-tauri/src/runtime/agent_core/run.rs` if shared helpers should move or become reusable

Expected behavior:

- Parent UI updates immediately after integration/closure.
- Result summary and parent decision are visible in the lifecycle payload when applicable.
- Completion gate state matches what the user sees.

Verification:

- Add focused Rust tests for each parent-side action updating persisted task state and appending lifecycle events.
- Add frontend projection tests for terminal `closed` and `interrupted` events.

### Slice 3: Redact Lifecycle Prompt Payloads

Use the same prompt preview/redaction semantics for `subagentPrompt` in lifecycle events that durable task storage uses for `prompt_preview`.

Touch points:

- `client/src-tauri/src/runtime/autonomous_tool_runtime/mod.rs`
- `client/src-tauri/src/runtime/agent_core/run.rs`
- Possibly expose a small helper instead of duplicating redaction logic

Expected behavior:

- Spawn lifecycle event shows a safe, bounded prompt preview.
- Raw prompt still reaches the child provider as needed.
- Persisted events do not leak sensitive-looking prompt text.

Verification:

- Add a Rust test with prohibited persistence content in a subagent prompt.
- Assert task storage and lifecycle payload both use a redacted preview.

### Slice 4: Reconcile Stale Child Tasks On Parent Resume

When a parent run reloads durable subagent tasks, reconcile tasks in active states:

- If the child run exists and is terminal, apply the child snapshot to the task.
- If the child run exists and is paused, mark task `paused`.
- If the child run exists and is still running but no worker token exists after restart, decide whether to mark `paused` or `interrupted`.
- If the child run is missing, mark task `failed` or `interrupted` with a diagnostic summary.

Touch points:

- `client/src-tauri/src/runtime/agent_core/run.rs`
- `client/src-tauri/src/runtime/autonomous_tool_runtime/mod.rs`
- `client/src-tauri/src/runtime/autonomous_tool_runtime/priority_tools.rs`

Expected behavior:

- Parent completion gates do not block forever on dead worker state.
- Reconciled status emits or persists enough information for the UI and final summary.

Verification:

- Add Rust tests with durable `running` tasks and mocked child-run statuses.
- Verify parent resume sees a resolved or actionable state.

### Slice 5: Add User-Facing Subagent Actions

Add real user-facing controls to the subagent card where appropriate:

- Open child run or trace.
- Cancel active task.
- Close terminal output with a decision.
- Integrate terminal output with a decision.
- Export trace.

Keep controls minimal and use existing ShadCN/lucide patterns. No temporary debug UI.

Touch points:

- `packages/ui/src/components/transcript/conversation-section.tsx`
- `client/components/xero/agent-runtime.tsx`
- Runtime command surface if direct UI actions need backend commands

Expected behavior:

- Users can resolve subagents without prompting the parent model to do it.
- Action-required subagent resolution has an obvious path in the UI.
- The card remains compact and readable.

Verification:

- Add component tests for available actions by status.
- Add command tests for direct resolution actions if new commands are introduced.

### Slice 6: End-To-End Spawn Coverage

Add a focused integration-style test for the happy path:

1. Parent run spawns a reviewer or researcher subagent.
2. Child run starts and emits transcript/tool events.
3. Parent stream receives grouped child items.
4. Child completes.
5. Parent integrates or closes with a decision.
6. Parent can complete without subagent resolution gate blocking.

Touch points:

- `client/src-tauri/src/runtime/agent_core/run.rs`
- `client/src-tauri/src/runtime/agent_core/provider_loop.rs`
- `client/components/xero/agent-runtime.test.tsx`
- `packages/ui/src/model/runtime-stream.test.ts`

Expected behavior:

- A future regression in spawn, lifecycle forwarding, or completion gating fails a focused test.

Verification:

- Run only scoped Rust tests and focused frontend tests.
- Do not run repo-wide Cargo unless explicitly needed.

## Suggested Order

1. Align frontend/backend status contracts.
2. Emit lifecycle events for resolution actions.
3. Redact lifecycle prompt payloads.
4. Add stale child-task reconciliation.
5. Add user-facing controls.
6. Add the end-to-end regression test once contracts are stable.

## Risks

- Lifecycle event changes may alter transcript ordering. Keep event sequence assertions focused on behavior, not exact incidental ordering.
- Direct UI controls may need a new backend command surface. Keep it narrow and reuse existing `subagent` action semantics.
- Stale task reconciliation needs careful wording. Marking dead workers as `failed` may be too harsh; `interrupted` is probably closer to user intent after app restart.

## Done Criteria

- All backend task statuses are accepted by stream DTOs and rendered coherently.
- Parent-side `close`, `integrate`, `interrupt`, and `cancel` produce visible lifecycle updates.
- Spawn lifecycle prompt payloads are bounded and redacted.
- Resumed parent runs do not get stuck on stale active subagent tasks.
- Users can resolve subagent tasks from the UI, or the absence of direct UI controls is an explicit product decision.
- Focused Rust and frontend tests cover policy, spawn, lifecycle projection, UI rendering, and completion-gate resolution.
