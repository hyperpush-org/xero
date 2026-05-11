# Built-In Agent Stages — Implementation Plan

## Why

Six built-in agents ship today: `ask`, `plan`, `engineer`, `debug`, `crawl`, `agent_create`. The runtime supports a `workflowStructure` (state machine of phases / "stages") that restricts tools per phase and advances on gates (`todo_completed`, `tool_succeeded`) — see `client/src-tauri/src/runtime/autonomous_tool_runtime/mod.rs:1160-1270` for the policy machine and `mod.rs:3111-3135` for the per-call enforcement. None of the built-ins use it. Stage presets exist (`client/src/lib/xero-model/stage-presets.ts`) but only as a one-click drop for *custom* agents on the canvas.

Result: `engineer` can write before reading; `debug` can edit code before forming a hypothesis. The stage machinery exists to prevent exactly that.

Goal: give `engineer` and `debug` real multi-stage workflows, plus lighter staging on `plan` and `agent_create`. Leave `ask` and `crawl` single-stage — they're already read-only and single-purpose.

## Scope

| Agent | Action | Stages |
| --- | --- | --- |
| `engineer` | Add `workflowStructure` | 4: `survey → plan → implement → verify` |
| `debug` | Add `workflowStructure` | 4: `reproduce → hypothesize → fix → verify` |
| `plan` | Add `workflowStructure` | 3: `discover → draft → accept` |
| `agent_create` | Add `workflowStructure` | 2: `interview → draft` |
| `ask` | No change | — |
| `crawl` | No change (already structured by its crawl_report contract) | — |

Out of scope for v1: per-stage prompts, dynamic branching on classifier output, user-visible "skip to next stage" affordance from chat, telemetry on stage advancement.

## Stage Designs

Tool name strings reference constants in `client/src-tauri/src/runtime/autonomous_tool_runtime/mod.rs:124-333`.

### `engineer` — `survey → plan → implement → verify`

| Stage | Allowed tools | Gate to next phase |
| --- | --- | --- |
| `survey` | `read`, `search`, `find`, `list`, `file_hash`, `git_status`, `git_diff`, `code_intel`, `lsp`, `workspace_index`, `tool_access`, `tool_search`, `project_context_search`, `project_context_get`, `environment_context`, `command_probe`, `todo` | `tool_succeeded: read >= 2` (must read before planning) |
| `plan` | survey set ∪ `project_context_record` | `todo_completed: implementation_plan` (named todo the agent must close) |
| `implement` | plan set ∪ `edit`, `write`, `notebook_edit`, `command_run` | `tool_succeeded: edit >= 1` OR `tool_succeeded: write >= 1` |
| `verify` | implement set ∪ `command_verify`, `system_diagnostics` | terminal |

`retryLimit: 2` on `implement` (after two failed tool calls in `implement`, the runtime blocks until the agent re-plans).

Rationale: forces the canonical "read → plan → write → verify" loop. The `tool_succeeded: read >= 2` gate is intentionally small — one read isn't a survey, three is overkill, two is "you looked at the change site and one neighbor."

### `debug` — `reproduce → hypothesize → fix → verify`

| Stage | Allowed tools | Gate to next phase |
| --- | --- | --- |
| `reproduce` | `read`, `search`, `find`, `list`, `git_status`, `git_diff`, `command_probe`, `command_verify`, `system_diagnostics`, `environment_context`, `code_intel`, `lsp`, `todo` | `todo_completed: reproduction_steps` |
| `hypothesize` | reproduce set ∪ `project_context_record` | `todo_completed: hypothesis` |
| `fix` | hypothesize set ∪ `edit`, `write`, `command_run` | `tool_succeeded: edit >= 1` |
| `verify` | fix set ∪ `command_verify` | terminal |

`retryLimit: 3` on `fix` — debugging often takes several tries; cap is generous.

Rationale: the existing description literally enumerates this loop ("evidence, hypotheses, fixes, verification") but nothing enforces order today. The reproduction todo and hypothesis todo are durable records — they become memory candidates after the run, so the *next* session for the same bug gets a starting point.

### `plan` — `discover → draft → accept`

| Stage | Allowed tools | Gate to next phase |
| --- | --- | --- |
| `discover` | `read`, `search`, `find`, `list`, `git_status`, `git_diff`, `code_intel`, `lsp`, `workspace_index`, `project_context_search`, `project_context_get`, `tool_access`, `tool_search`, `todo` | `tool_succeeded: read >= 1` |
| `draft` | discover set ∪ `project_context_record` | `todo_completed: plan_draft` |
| `accept` | draft set | terminal |

Stages here are lighter — `plan` is already tool-restricted at the policy level. Value is mostly: distinct stage in the canvas UI so the user can see where the agent currently is.

### `agent_create` — `interview → draft`

| Stage | Allowed tools | Gate to next phase |
| --- | --- | --- |
| `interview` | `read`, `search`, `find`, `tool_access`, `tool_search`, `todo` | `todo_completed: interview_complete` |
| `draft` | interview set ∪ `agent_definition` | terminal |

Marginal but cheap — prevents the agent from writing a definition before completing the interview.

## Implementation Steps

1. **Add `workflowStructure` JSON to the four agent snapshots** in `client/src-tauri/src/db/migrations.rs:1544-1549`. Bump `current_version: 1 → 2` for each in both the summary and version tables. Note: `engineer`, `debug`, `agent_create` currently have stub snapshots (e.g. `{"id":"engineer","version":1,"scope":"built_in",...}`) — they need fuller schemas matching the shape of the existing `plan` and `crawl` snapshots, including `schema`, `schemaVersion`, `displayName`, `toolPolicy`, etc. Snapshot JSON is validated by `customAgentWorkflowStructureSchema` in `client/src/lib/xero-model/agent-definition.ts:548-584`.

2. **Verify runtime pickup.** The runtime reads `workflowStructure` from the snapshot at `mod.rs:1196` and uses it in `record_agent_workflow_before_tool` at `mod.rs:3111`. No code change needed in the runtime if the snapshot JSON is valid — but add a regression test in `client/src-tauri/tests/agent_core_runtime.rs` that loads each built-in and asserts `workflowStructure.phases.len() >= 2`.

3. **Define the named-todo identifiers** the gates reference (`implementation_plan`, `reproduction_steps`, `hypothesis`, `plan_draft`, `interview_complete`). Add them to the per-agent prompt body so the model knows it must call `todo` with those ids to advance. Prompts live alongside the policy definitions — check `client/src-tauri/src/runtime/autonomous_tool_runtime/` for prompt templates per agent.

4. **Surface stage state in the UI.** The canvas already visualizes `workflowStructure` for custom agents (`client/components/xero/workflow-canvas/agent-visualization.tsx`). Confirm built-ins render the same way — they should, since the visualizer reads `workflowStructure` agnostically. The agent runtime panel should show "Stage: Survey (1/4)" or similar; check `client/components/xero/workflow-canvas/effective-runtime-panel.tsx`.

5. **Add an escape hatch.** Edge case: user asks `engineer` to "just fix this one typo." Forcing `survey → plan → implement` is theater. Two options:
   - **A.** Add a `--quick` runtime flag that bypasses `workflowStructure` for that session.
   - **B.** Accept the rigidity; user can use `ask` or the manual edit instead.
   Recommend A — but defer to v2 if the survey gate (`read >= 2`) turns out to feel fast enough in practice.

6. **Tests.** Per `CLAUDE.md`: scoped Rust tests only.
   - `agent_core_runtime.rs` — assert each built-in's snapshot validates and exposes the expected phase count.
   - New scenario test: run `engineer` with a prompt that asks it to `write` immediately; assert the runtime returns `policy_denied` with the "phase X has not satisfied its required gates" message from `mod.rs:3131`.
   - Same shape of test for `debug` (edit before hypothesis todo) and `plan` (record before discover gate).

7. **Update `agent_create`'s drafting prompt** so it can produce *new* custom agents that themselves use `workflowStructure` — using the four built-ins as worked examples. Stage presets already exist in `stage-presets.ts`; the agent-builder prompt should reference them.

## Open Design Questions

1. **Should stage failures roll back the agent's last message, or surface as a tool-denied response the model has to react to?** Current code path (`mod.rs:3111-3135`) returns `policy_denied`, which the model sees as a tool error. That works but is implicit — the model has to figure out "oh I need to advance the stage." Consider an explicit `advance_stage` tool the model can call, gated by the same conditions. **Recommendation:** start implicit; if model evals show confusion, add the explicit advance tool.

2. **Gate strictness on `engineer.survey`.** `tool_succeeded: read >= 2` is a heuristic. Alternative: require a `todo_completed: survey_findings` todo. Todo gates are more deliberate but add more model overhead. **Recommendation:** start with the read-count heuristic, switch to a todo gate if engineers skip survey in evals.

3. **Should `verify` failure branch back to `implement` automatically?** Today the policy supports `branches` with conditions. We could add `verify → implement` on `tool_succeeded: command_verify` returning failure. But the runtime doesn't distinguish failed-tool-call from tool-that-reported-failure — `command_verify` succeeded as a *tool call* even if the test failed. Need to either expose a new condition kind (`tool_returned_failure`) or rely on the model to call `edit` from `verify`, which would now fail because `edit` isn't allowed there. **Recommendation:** v1 doesn't auto-branch; user re-prompts to restart `implement`.

4. **Versioning collision.** Bumping `current_version: 1 → 2` means existing installs would need a migration to re-seed. `CLAUDE.md` says backwards compatibility is not required, but verify no projects in dev have v1 sessions mid-flight. Otherwise wipe local app-data and re-seed.

## Verification

Definition of done:

- [ ] `cargo test --test agent_core_runtime` passes with new phase-count assertions.
- [ ] Manually invoke `engineer` in the app; confirm canvas shows 4-stage progression bar.
- [ ] Trigger a `policy_denied` by asking `engineer` to write before reading; confirm the error message names the phase.
- [ ] Trigger the same on `debug` for edit-before-hypothesis.
- [ ] Confirm `agent_create` can draft a multi-stage custom agent that itself runs with stages enforced.
- [ ] No regressions in `ask` / `crawl` (still single-stage, still functioning).

## Risks

- **Rigidity backlash.** Forcing read-before-write on a typo fix will annoy. Mitigated by step 5's escape hatch, or by keeping gates lenient (read-count of 1 instead of 2).
- **Prompt drift.** If the prompt doesn't tell the model the named todo ids, it'll never advance. Step 3 is load-bearing.
- **Surface area.** Each new built-in version is durable in the database; rolling back requires another version bump. Prefer to land all four agents together to amortize the version churn.
