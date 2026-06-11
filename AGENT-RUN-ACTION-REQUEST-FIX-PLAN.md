# Agent Run Action Request Fix Plan

## Reader And Outcome

This plan is for a Xero engineer fixing the owned-agent approval and tool-run regressions exposed by the latest agent run audit.

After reading it, the engineer should be able to implement the fixes, wipe affected dev app-data state as required by project policy, and run scoped regressions that prove this failure class cannot recur.

## Problem Statement

The latest owned-agent run produced several user-visible failures:

- Approval cards rendered with Approve and Reject buttons, but clicking them did not resolve the underlying action.
- Some shell command review events appeared as successful tool calls even though no command spawned.
- `command_verify` rejected a valid package-manager script named `type-check`.
- Operator/action errors were stored in state but not shown where the user was trying to act.
- Invalid tool input was rendered like an approvable operator decision even though approval could not fix it.
- The agent reasoned about missing or confusing tool availability instead of receiving clear stage/tool guidance.
- File edit mismatches were recoverable, but noisy and not clearly separated from hard failures.
- The target app update skipped at least one likely app surface, so the original “all apps” intent was not clearly completed.

The root cause is not one isolated UI bug. The runtime, persistence, stream projection, frontend routing, command policy, and agent guidance each allow a different part of the bad experience.

## Goals

- Action-required events must carry durable action identity from backend persistence to frontend cards.
- Approval and rejection buttons must route to the owned-agent action APIs when the card represents an owned-agent action.
- Approving one card must not accidentally approve unrelated pending actions from the same run.
- Pending command reviews must be displayed as pending review, not succeeded command executions.
- Valid package-manager verification scripts such as `type-check` must pass the validator and policy allowlist.
- Operator/action errors must be visible near the card or composer that triggered them.
- Invalid tool input must render as a non-approvable diagnostic or failure state, not as an Approve/Reject card.
- Agent stage and tool guidance must reduce confusion about available edit/command tools and package-manager workflows.
- Scoped tests must cover the bad paths from the audited run.

## Non-Goals

- Do not add backwards-compatible migrations or glue for stale malformed dev state unless explicitly requested.
- Do not create temporary debug UI.
- Do not finish unrelated Tokenloom product work as part of this fix except where a fixture or regression test needs a representative monorepo scenario.
- Do not rename on-wire `workflowStructure.phases` or other legacy DTO names while fixing Stages behavior.

## Phase 1: Preserve Durable Action Identity

Fix owned-agent `action_required` event emission so the stream can render real, resolvable cards.

Relevant implementation areas:

- `client/src-tauri/src/runtime/agent_core/persistence.rs`
- `client/src-tauri/src/runtime/agent_core/tool_dispatch.rs`
- `client/src-tauri/src/commands/subscribe_runtime_stream.rs`

Tasks:

- In `record_command_action_required`, append the same durable `actionId` that was inserted into `agent_action_requests`.
- Include `actionType`, `title`, `detail`, and the decision shape needed by the frontend.
- For command approval boundaries, use an explicit action type such as `command_approval`.
- For safety-boundary policy denials, append the durable action id and use an explicit action type such as `safety_boundary`.
- Include enough command context for the UI to display the exact pending command without inferring from a sibling tool call.
- Stop inventing resolvable fallback ids like `owned-agent-action-{event_id}` for owned-agent action cards.
- If a historical or malformed event has no durable action id, project it as a noninteractive diagnostic with clear copy instead of rendering live Approve and Reject buttons.

Acceptance checks:

- A newly emitted command-review event contains the same action id as its `agent_action_requests` row.
- A newly emitted safety-boundary event contains the same action id as its `agent_action_requests` row.
- The frontend never receives a clickable owned-agent action card without a durable action id.

## Phase 2: Make Approve And Reject Action-Scoped

Fix the backend and frontend path so each card resolves exactly the intended action.

Relevant implementation areas:

- `client/src-tauri/src/db/project_store/agent_core.rs`
- `client/src-tauri/src/commands/agent_task.rs`
- `client/src-tauri/src/runtime/agent_core/facade.rs`
- `client/components/xero/agent-runtime.tsx`
- `client/components/xero/agent-runtime/use-agent-runtime-controller.ts`

Tasks:

- Audit `answer_pending_agent_action_requests`, which currently resolves pending action requests by run rather than by specific action id.
- Add an action-scoped answer path, or make existing owned-agent approval APIs require and honor `actionId`.
- Ensure approving one card cannot approve every pending request in the run.
- Ensure rejecting one card cannot reject unrelated pending requests.
- Update the frontend action card routing to pass `runId`, `actionId`, and `actionType` to the owned-agent action APIs.
- Keep operator-review actions separate from owned-agent actions so cards do not silently route to `resolve_operator_action`.
- Add stale-run reconciliation so a paused run with pending action requests is still resolvable even if the supervisor row is marked stale.
- If a run cannot be resumed because runtime state is truly stale, show a direct error and recovery action instead of leaving buttons inert.

Acceptance checks:

- Approving one of two pending action cards resolves only that card's row.
- Rejecting one of two pending action cards resolves only that card's row.
- A paused run with pending action requests can be resumed or produces a visible recovery error.
- No owned-agent action card calls the operator-review mutation path.

## Phase 3: Surface Action Errors Where The User Acts

The audited UI had errors in state, but the user could not see them from the card or composer.

Relevant implementation areas:

- `client/src/features/xero/use-xero-desktop-state/operator-auth-mutations.ts`
- `client/components/xero/agent-runtime/use-agent-runtime-controller.ts`
- `client/components/xero/agent-runtime.tsx`

Tasks:

- Thread `operatorActionError` and owned-agent action errors through the runtime controller.
- Render action failure feedback inline on the relevant card when possible.
- Also expose the latest action error in the composer area if the card has scrolled away.
- Clear stale action errors after a successful retry or when a different action is selected.
- Avoid generic silent failures; include the backend reason if it is safe and user-actionable.

Acceptance checks:

- A failed Approve click shows visible feedback without requiring logs.
- A failed Reject click shows visible feedback without requiring logs.
- Retrying after fixing the cause clears the old error.

## Phase 4: Correct Command Review Display Semantics

Some command-review events were marked successful even though the command never spawned.

Relevant implementation areas:

- `client/src-tauri/src/commands/subscribe_runtime_stream.rs`
- `client/components/xero/agent-runtime.tsx`

Tasks:

- Treat command tool results with `spawned=false`, `exitCode=null`, and a review/escalation outcome as pending review.
- Do not render those tool completions as successful executions.
- Collapse or visually associate the pending command tool event with its action-required card so the stream does not show both a green success and a pending approval for the same command.
- Preserve true success styling only for commands that actually spawned and exited successfully.
- Preserve true failure styling for commands that spawned and returned nonzero.

Acceptance checks:

- A command waiting for approval is labeled as needing review.
- A command waiting for approval has no green success affordance.
- A spawned command with exit code 0 still renders as succeeded.
- A spawned command with nonzero exit code still renders as failed.

## Phase 5: Accept Safe `type-check` Verification Scripts

The audited target repo used `type-check`, and direct package-manager execution passed. Xero rejected the script before it could run.

Relevant implementation areas:

- `client/src-tauri/src/runtime/agent_core/types.rs`
- `client/src-tauri/src/runtime/autonomous_tool_runtime/policy.rs`

Tasks:

- Add `type-check` to the safe package-manager verification names.
- Keep `typecheck` accepted.
- Keep `test`, `tests`, `lint`, `check`, and `build` behavior unchanged.
- Decide whether scoped variants such as `type-check:ci` are safe. If supported, implement intentionally and test them; otherwise reject them with clear copy.
- Update validator error text so it lists both `typecheck` and `type-check`.
- Keep the command verification validator and the autonomous tool policy allowlist aligned.

Acceptance checks:

- `pnpm --filter <pkg> type-check` validates.
- `pnpm --filter <pkg> run type-check` validates if the existing validator supports `run` script form.
- Equivalent npm and yarn safe script forms validate where currently supported.
- Unsafe package scripts remain rejected.

## Phase 6: Render Invalid Tool Input As A Diagnostic

`agent_action_tool_input_invalid` is not fixed by user approval, so it should not look like an approvable action.

Relevant implementation areas:

- `client/src-tauri/src/runtime/agent_core/tool_dispatch.rs`
- `client/src-tauri/src/commands/subscribe_runtime_stream.rs`
- `client/components/xero/agent-runtime.tsx`

Tasks:

- Classify invalid tool input separately from action-required approval boundaries.
- Render invalid tool input as a failed tool-call diagnostic with the validation message.
- If the run pauses on invalid input, explain that the next agent turn must retry with corrected input.
- Do not show Approve and Reject buttons for invalid tool input.
- Avoid storing invalid input events as pending `agent_action_requests` unless there is a real decision the operator can make.

Acceptance checks:

- An invalid `command_verify` input displays as a validation failure.
- The card has no Approve or Reject buttons.
- The run state and UI copy agree on whether the agent can continue.

## Phase 7: Reduce Agent Tool Confusion

The agent reasoned about missing or surprising tools because the stage prompt and available tools were not explained clearly enough.

Relevant implementation areas:

- `client/src-tauri/src/db/migrations.rs`
- Runtime prompt or stage-policy generation for Engineer mode.
- Any tests covering Stages tool allowlists.

Tasks:

- Update Engineer stage guidance to clearly explain available edit tools in the current runtime.
- If `patch` is intentionally unavailable, say so through tool instructions and explain the expected `edit` or `write` workflow.
- Make command stages explicit: `command_verify` is for verification commands; package-manager mutation commands need the appropriate reviewed command path.
- Add lockfile guidance: when package manifests change, the agent should use the package manager to update the lockfile through an approved command rather than hand-editing it.
- Keep user-facing terminology as "Stages" and avoid introducing "workflow phases" in UI copy.

Acceptance checks:

- A new run receives clear tool guidance before the first edit or command decision.
- The agent no longer needs to infer why `patch` is unavailable.
- Package manifest changes trigger a clear lockfile update path.

## Phase 8: Add Target-Scope Completion Guardrails

The audited run changed web, landing, and admin surfaces, but did not clearly resolve whether mobile was in scope.

Relevant implementation areas:

- Agent instructions/prompt planning for implementation tasks.
- E2E or integration fixture representing a multi-app workspace.

Tasks:

- Add a representative fixture or scripted scenario for a monorepo with shared UI, web, landing, admin, and mobile surfaces.
- Ensure the agent records which app surfaces are in scope before claiming "all apps" are complete.
- If a surface is incompatible with the chosen implementation, the agent must either implement the compatible alternative or explicitly report that it was not changed.
- Ensure final responses distinguish verified surfaces from skipped surfaces.

Acceptance checks:

- A multi-app request cannot be marked complete while silently skipping a likely app surface.
- Verification output names the surfaces actually tested.

## Phase 9: Wipe Affected Dev App-Data State Before Final QA

Project policy prohibits compatibility glue for stale state in this new app unless explicitly requested.

Tasks:

- After code fixes are in place, wipe the affected state under `~/Library/Application Support/dev.sn0w.xero`.
- Prefer the narrowest affected project state wipe if enough context is known.
- Use only `Support/dev.sn0w.xero` data during development.
- Do not use `.xero/` repo-local state for new project state.
- Reproduce the audited failure class from fresh dev app-data.

Acceptance checks:

- Fresh dev app-data produces durable action ids on new action-required events.
- No stale malformed action cards remain after reset.
- No compatibility migration was added for the malformed dev-only action events.

## Testing Checklist

Run scoped tests only.

- Rust tests for action-required persistence payloads.
- Rust tests for stream projection of owned-agent action cards.
- Rust tests for action-scoped approval and rejection.
- Rust tests for `type-check` command verification and unsafe script rejection.
- Frontend tests for owned-agent Approve and Reject routing.
- Frontend tests for inline action error display.
- Frontend tests for pending command review display.
- Integration smoke test for a paused owned-agent run with a pending command approval.
- Integration smoke test for invalid tool input rendering as a diagnostic.
- Manual Tauri QA in development data if automated UI coverage cannot cover the full resume path.

## Final Acceptance Criteria

- The screenshot failure class no longer reproduces: Approve and Reject do something visible and correct.
- Every clickable owned-agent action card has a durable action id.
- Clicking Approve or Reject mutates exactly the intended pending action row.
- Command approvals do not render as successful command executions before spawning.
- `type-check` verification scripts are accepted by both validator and policy.
- Invalid tool input is shown as a validation failure, not an approval request.
- Action errors are visible to the user without reading logs.
- Fresh dev app-data verifies the fixed behavior.
- Scoped tests and formatting relevant to touched files pass.

## Risks And Open Questions

- The current run-level approval path may have been intentionally broad. If so, the product needs an explicit "approve all pending actions" affordance rather than making individual cards behave that way.
- Resuming a paused run whose supervisor state is stale may need a small reconciliation layer before action resolution. Keep this narrow and observable.
- Decide whether command approval should replay the exact pending command or only resume the model with the approval answer. The safer behavior is to bind approval to the exact stored command request.
- Decide whether `type-check:*` script names are safe enough to allow. Avoid broad wildcard script approval unless there is a clear policy reason.
- Historical malformed action events can remain noninteractive diagnostics after dev app-data reset. Do not add backwards compatibility code for stale development state unless explicitly requested.
