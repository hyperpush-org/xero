# Subagent Gap Closure Plan

## Reader And Outcome

Reader: an internal Xero runtime engineer landing cold in the owned-agent harness.

Post-read action: implement the remaining work needed to move subagent support from experimental prototype to production-ready runtime feature.

## Current State

Xero already has a real subagent foundation. The parent agent can call a `subagent` tool, spawn a pane-contained child owned-agent run, assign a role, set write-set ownership, wait for status, cancel, close, integrate, and export a trace. The child run receives a role-scoped tool policy, linked cancellation token, parent lineage, trace identifiers, and file-write scope.

The foundation is promising, but it is not complete enough to treat as a dependable production feature. The parent-side task registry is volatile, follow-up messages do not reliably affect an already-running child loop, declared token and cost budgets are mostly advisory, parent completion does not require child integration, child behavioral instructions are weaker than runtime policy, and custom-agent authoring can enable subagents without emitting the required role allowlist.

## Goals

- Preserve subagent task state durably across process restarts, run resumes, app relaunches, and context handoffs.
- Make child lifecycle operations mean what the tool contract says: follow-up, wait, cancel, close, integrate, and export trace must have deterministic effects.
- Enforce delegated tool, token, and cost budgets through runtime policy, not just prompt text.
- Prevent parent runs from completing with unresolved subagent work unless they explicitly cancel, close, or integrate it.
- Teach Engineer and Debug agents when and how to delegate, wait, and integrate results.
- Make custom-agent authoring and validation agree on subagent role policy.
- Add focused tests and diagnostics that prove the contract works without broad repo-wide test runs.

## Non-Goals

- Do not add temporary debug UI.
- Do not introduce backward compatibility with legacy repo-local `.xero/` state.
- Do not enable recursive subagent trees beyond the configured depth.
- Do not allow Ask, Plan, Crawl, or Agent Create to bypass their current no-subagent boundaries.
- Do not use subagents as a substitute for same-type handoff or context compaction.

## Architecture Decisions

1. Persist parent-side subagent tasks in app-data project state.

   Child owned-agent runs already persist, but the parent task registry should also persist as first-class runtime state. Store task id, parent run id, child run id, role, prompt hash or redacted prompt preview, model route, write set, status, timestamps, budgets, integration decision, result artifact, input log, and latest summary.

2. Treat child interaction as continuation, not transcript append.

   A `send_input` or `follow_up` action must either continue a paused/waiting child run or return a clear `not_accepting_input` or `continue_required` state. Appending a user message without waking the child is not sufficient.

3. Enforce all delegated budgets centrally.

   Keep the existing delegated tool-call budget, then add provider usage accounting for child token and cost ceilings. If a child exceeds its delegated budget, stop the child run with a budget diagnostic and update the parent task status.

4. Require explicit parent resolution before final completion.

   If the parent spawned child work, completion should be held until every child is terminal and either integrated, cancelled, or explicitly closed with a decision. This should be a runtime completion gate, not only prompt guidance.

5. Keep child write policy hard and prompt guidance soft.

   Runtime write-set enforcement remains the authority. Prompt text can explain role behavior, but tool policy and write guards must be the final safety boundary.

6. Make role allowlists explicit in authoring.

   Any custom agent that enables subagent delegation must declare `allowedSubagentRoles`. The graph builder, validation preview, and saved snapshot should all round-trip this field.

## Implementation Slices

### S1. Durable Subagent Task Store

Create an app-data-backed subagent task store keyed by project id, parent run id, and subagent id.

Required behavior:
- Insert a task before child execution starts.
- Update task status when the child starts, finishes, fails, cancels, closes, or is integrated.
- Persist input log entries and parent decisions.
- Rebuild the in-memory task registry from durable state when continuing a parent run.
- Link task records to child run ids and trace ids after child creation.
- Redact or hash prompt content consistently with existing runtime persistence rules.

Acceptance checks:
- A parent run can spawn a subagent, reload runtime state, and still list the task.
- A completed child remains visible to the parent after app restart.
- Integration decisions survive reload and export.

### S2. True Child Continuation For Follow-Ups

Replace passive transcript append semantics with deterministic child continuation semantics.

Required behavior:
- `send_input` and `follow_up` append the user message and drive the child run when it is resumable.
- If the child is already actively running, return a clear status explaining whether the message was queued, rejected, or deferred.
- If the child is terminal, reject input unless a future reopen flow exists.
- Surface any continuation error on both the child task and parent event stream.

Acceptance checks:
- Sending input to a paused child causes a new provider turn.
- Sending input to a terminal child is rejected.
- Parent sees the updated child task status without manual trace export.

### S3. Budget Enforcement

Make delegated budgets real.

Required behavior:
- Keep delegated tool-call decrementing before each child tool call.
- Track child provider token usage against `maxTokens`.
- Track child provider cost against `maxCostMicros`.
- Stop the child when any delegated budget is exhausted.
- Persist the exhaustion reason as a diagnostic and task result summary.
- Include remaining budget in compact model-visible `subagent` output.

Acceptance checks:
- A child that exhausts tool calls is stopped before the next tool call.
- A child that exceeds token or cost budget is stopped with a budget diagnostic.
- Budget status is visible from `status`, `wait`, and `export_trace`.

### S4. Parent Resolution Gate

Add a completion gate for unresolved subagent work.

Required behavior:
- Before accepting a parent final response, inspect durable subagent task state.
- If any task is running or registered, prompt the parent to wait, cancel, or close it.
- If any terminal task is not integrated or explicitly closed with a decision, prompt the parent to integrate or close it.
- Allow an explicit user override only through the normal action/approval path if product requires one.

Acceptance checks:
- Parent cannot complete immediately after spawning a child.
- Parent can complete after integrating all terminal children.
- Parent can complete after cancelling or closing children with recorded decisions.

### S5. Prompt And Tool-Guidance Upgrade

Update Engineer and Debug prompt fragments so capable agents actually use subagents appropriately.

Required guidance:
- Use researcher/reviewer/planner subagents for bounded parallel investigation, independent review, and sidecar planning.
- Use engineer/debugger subagents only with disjoint write sets.
- Do not delegate immediate blocking work if the parent cannot proceed without the answer.
- After spawning, continue useful parent work, then wait only when needed.
- Always integrate, close, or cancel spawned tasks before final response.
- Include subagent results and parent decisions in the final handoff summary.

Keep existing prohibitions for Ask, Plan, Crawl, and Agent Create.

Acceptance checks:
- Prompt compilation includes delegation guidance for Engineer and Debug.
- Prompt compilation still forbids subagents for non-mutating agents.
- Tool exposure still requires policy permission and does not expose subagents to Ask/Plan/Crawl/Agent Create.

### S6. Custom-Agent Authoring Parity

Fix the mismatch between frontend graph snapshots and backend validation.

Required behavior:
- Expose role selection when subagent delegation is enabled.
- Emit `allowedSubagentRoles` in saved snapshots.
- Preserve `deniedSubagentRoles` when present.
- Preview effective tool access should explain missing role allowlists.
- Validation should keep failing closed when subagent delegation has no allowed roles.

Acceptance checks:
- A custom engineering agent can enable subagents with allowed reviewer/researcher roles.
- Saving a subagent-enabled custom agent without allowed roles fails with a repair hint.
- Round-tripping a saved definition preserves role policy.

### S7. Diagnostics And Model-Visible Output

Make subagent state easy for both the model and support to understand.

Required behavior:
- Compact `subagent` output should include active task count, blocked/terminal/integrated counts, budget status, child run id, role, write set, and next expected action.
- Trace export should redact raw transcripts but include useful event kinds, file changes, diagnostics, and integration decisions.
- Context manifests should include active sibling/child subagent state where relevant.
- Support diagnostics should classify child-run failures separately from parent failures.

Acceptance checks:
- A model can decide whether to wait, integrate, cancel, or proceed from one compact status result.
- Support export can explain a failed child run without exposing secrets or hidden prompts.

### S8. Verification Coverage

Add scoped tests for the production contract.

Required coverage:
- Durable task store insert, update, reload, and cleanup.
- Spawned child run lineage and trace linkage.
- Follow-up continuation behavior for resumable and terminal children.
- Tool-call, token, and cost budget exhaustion.
- Parent completion gate for unresolved child work.
- Write-set enforcement after reload.
- Custom-agent role allowlist validation and frontend snapshot round-trip.
- Compact output shape for `status`, `wait`, `integrate`, and `export_trace`.

Run only focused Rust and TypeScript tests while developing each slice. Avoid repo-wide test runs unless explicitly requested.

## Rollout Order

1. Land durable task persistence and reload first. Without this, the rest cannot be trusted across real app sessions.
2. Land parent completion gate next. This prevents new unresolved-child behavior from becoming normal.
3. Land follow-up continuation semantics.
4. Land token and cost budget enforcement.
5. Land prompt guidance and compact output improvements.
6. Land custom-agent authoring parity.
7. Land diagnostics polish and final focused coverage.

## Release Gate

Subagents are production-ready only when all of these are true:

- Parent-side task state survives restart and resume.
- Every spawned child has a durable child run, trace id, parent linkage, and terminal state.
- Parent final responses are blocked until child tasks are resolved.
- Tool, token, and cost budgets are enforced by runtime code.
- Follow-up actions either drive the child or fail with a precise reason.
- Custom agents cannot enable delegation without explicit role allowlists.
- Engineer and Debug prompts describe when to delegate and how to integrate.
- Focused tests cover persistence, lifecycle, budgets, prompt compilation, policy, and UI snapshot parity.

## Reader-Test Notes

A cold reader should be able to start at S1, implement the durable store, and continue slice-by-slice without needing the original audit conversation. The critical sequencing is persistence before lifecycle polish, because unresolved child state must be durable before gates and continuation semantics can be reliable.
