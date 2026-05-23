# Cursor Through Xero Harness Production Plan

## Objective

Enable Cursor subscription-backed agents to run inside Xero as close to a native Xero owned-agent run as the Cursor SDK allows.

The target is not just "launch Cursor from Xero." The target is:

- Cursor uses the operator's Cursor API key or service account key.
- Cursor receives Xero's tool surface through MCP.
- Tool calls that mutate or inspect the workspace are dispatched by Xero's Tool Registry V2 wherever possible.
- Xero persists run, message, tool, file-change, policy, sandbox, and trace events in OS app-data state.
- Any Cursor-native action that bypasses Xero is detected, labeled, and treated as a policy/audit exception.
- The normal user experience feels like selecting and using any other model provider in Xero.

## UX Principle

Cursor-specific implementation details should stay out of the primary user experience.

The user should see:

- a Cursor provider/model option where provider selection normally lives
- normal Xero run progress
- normal messages, tool activity, file changes, approvals, and trace exports
- normal recovery messaging only when user action is actually required

The user should not see:

- Cursor-specific warning banners for recoverable behavior
- separate Cursor-only run screens
- raw Cursor SDK event names
- MCP transport details
- native-tool bypass language unless the bypass cannot be recovered automatically or the user is inspecting an audit trace

Cursor provenance should still be persisted for audit, diagnostics, and support bundles. The distinction is that provenance is trace detail, not routine product chrome.

## Current Findings

- Cursor SDK supports local and cloud agents with `CURSOR_API_KEY` or explicit `apiKey`.
- Cursor SDK supports inline `mcpServers` on `Agent.create()` and `agent.send()`.
- Cursor SDK streams normalized `assistant`, `thinking`, `tool_call`, `status`, and `task` events.
- Xero already has `xero mcp serve`, but it only exposes conservative harness tools such as start/query/index/memory/trace.
- Xero's full headless Tool Registry V2 surface already exists in `xero-agent-core` as `read`, `list`, `write`, `patch`, `delete`, `move`, `replace`, and `command`.
- Cursor SDK does not currently document a way to disable Cursor's native tools or force all work through MCP.

## Native Cursor Tool Risk

Cursor-native tools are not necessarily process-breaking for Xero, but they are not benign for production-grade Xero semantics.

If Cursor reads a file with its own read tool, Xero may still finish the run, but Xero will not have a first-class Xero `tool_started` / `tool_completed` event for that read.

If Cursor writes, patches, deletes, or runs commands with native Cursor tools, Xero can lose:

- file reservations
- rollback checkpoints
- sandbox decisions
- approval decisions
- Stage gates
- command output capture
- durable file-change events
- proof that `.git`, `.xero`, app-data, and outside-workspace paths were protected

That means Cursor-native tools should be treated as an audit and policy bypass. They may be acceptable in an early experiment, but production must detect them, mark the run degraded or failed depending on policy, and compare actual workspace changes against Xero-recorded tool events.

Production recovery should avoid weird user-visible failures. A direct Cursor edit should first be treated as an untracked workspace transaction that Xero tries to reconcile:

- if it only touched allowed workspace files, import it into Xero's trace as a recovered change
- if it touched denied locations, restore from the pre-run baseline and report only the actionable recovery outcome
- if it cannot be reconciled safely, ask for operator review using the same review/approval surfaces Xero already uses

## Architecture Target

Use Cursor as an external agent runtime, not as a normal OpenAI-compatible model provider.

Recommended shape:

1. Xero starts a Cursor SDK bridge process.
2. The bridge creates a Cursor local agent with `local.cwd` set to the registered project workspace.
3. The bridge passes Xero's MCP tool server inline through `mcpServers`.
4. Cursor calls Xero MCP tools.
5. Xero MCP dispatches Tool Registry V2 calls and persists native-style Xero events.
6. The bridge streams Cursor SDK events back to Xero as JSONL.
7. Xero persists Cursor stream events into the same run trace.
8. Xero audits the final workspace diff against Xero-recorded file changes.

Cloud Cursor agents are a later target. Local Cursor agents can reach a local stdio MCP server directly. Cursor cloud agents cannot reach local stdio tools unless the tool server is available inside the Cursor VM or exposed as authenticated HTTP/SSE.

## Stage 0: Product Contract And Terminology

Deliverables:

- Decide the public provider id, likely `external_cursor_sdk` or `cursor_sdk`.
- Internally label it as an external-agent adapter until Xero can guarantee native tool-only execution.
- Externally present it as a normal provider option wherever possible.
- Define user-facing language:
  - normal provider surfaces: "Cursor"
  - trace/support/audit surfaces: "Cursor SDK via Xero MCP harness"
  - recovery surfaces: describe the recovered change, not the underlying MCP or SDK mechanism
- Define what counts as native-equivalent:
  - Xero run record exists before Cursor starts.
  - Cursor messages are persisted as Xero messages/events.
  - Xero MCP tool calls persist Tool Registry V2 events.
  - direct Cursor-native mutation is detected.
  - safe direct Cursor-native mutation is recovered into Xero trace semantics.
  - final trace exports cleanly through existing conversation/trace commands.

Acceptance criteria:

- Routine UI does not expose a Cursor-specific workflow.
- Audit docs do not call this a fully native Xero owned-agent provider before enforcement is implemented.
- "Stages" remains reserved for gated phases inside an agent run. Do not rename existing `workflowStructure.phases` DTOs as part of this work.

## Stage 1: Minimal Cursor SDK Bridge

Deliverables:

- Add a small Node/TypeScript bridge package or script that depends on `@cursor/sdk`.
- Inputs:
  - `prompt`
  - `repoRoot`
  - `projectId`
  - `runId`
  - `sessionId`
  - `model`
  - `apiKeyEnv` or inherited `CURSOR_API_KEY`
  - `xeroCliPath`
  - `xeroStateDir`
- Outputs:
  - newline-delimited JSON events for `started`, `sdk_message`, `delta`, `step`, `completed`, `failed`.
- Configure Cursor local agent:
  - `local.cwd = repoRoot`
  - `local.settingSources = []` for deterministic MVP
  - inline `mcpServers.xero`
  - `platform.stateRoot` under Xero OS app-data if supported by the SDK type surface

Acceptance criteria:

- Bridge can create a Cursor local agent with an inline test MCP server.
- Bridge can stream all SDK run events without buffering the whole run.
- Bridge exits nonzero with structured error JSON on authentication, model, or SDK failures.

## Stage 2: Add Xero Tool Registry MCP Server Mode

Deliverables:

- Add a new CLI path, for example:
  - `xero mcp serve-tools --project-id PROJECT --run-id RUN --session-id SESSION --repo REPO`
- Reuse `HeadlessProductionToolRuntime` from `xero-agent-core` where possible.
- Expose MCP tool definitions from Tool Registry V2 descriptors:
  - `read`
  - `list`
  - optional write tools when explicitly allowed: `write`, `patch`, `delete`, `move`, `replace`, `command`
- Support modes:
  - observe-only
  - workspace-write
  - command-enabled
- Return MCP `structuredContent` with the same summary/output shape the provider loop expects.

Acceptance criteria:

- `tools/list` returns only tools allowed by the requested mode.
- `tools/call` dispatches through Tool Registry V2, not ad hoc filesystem code.
- `.xero/`, `.git`, app-data, outside-workspace writes, and denied command/network intents are blocked by existing sandbox policy.
- Read-only mode cannot mutate files or run commands.

## Stage 3: Native-Style Run Persistence For MCP Tool Calls

Deliverables:

- Create or attach a Xero run before Cursor starts.
- Persist:
  - `run_started`
  - `message_delta` for user and Cursor assistant text
  - `tool_registry_snapshot`
  - `tool_started`
  - `tool_completed`
  - `file_changed`
  - `command_output`
  - `policy_decision`
  - `run_completed` or `run_failed`
- Reuse the headless provider event payload conventions where possible.
- Include Cursor provenance:
  - Cursor agent id
  - Cursor run id
  - Cursor model
  - SDK version
  - local/cloud runtime
  - bridge version

Acceptance criteria:

- `xero conversation dump RUN_ID --json` shows a coherent Cursor-backed run.
- Trace export works without special casing.
- Tool and file-change events are distinguishable as `tool_registry_v2` dispatch.
- Cursor-native events are captured separately and not confused with Xero-dispatched tools.

## Stage 4: External Adapter Integration

Deliverables:

- Add provider catalog entry for Cursor as an external-agent adapter.
- Extend `xero agent host` or add a sibling command for structured Cursor SDK runs.
- Keep explicit subprocess approval semantics.
- Add support for:
  - model selection
  - timeout
  - max tool calls
  - max command calls
  - allow writes
  - allow commands
  - observe-only default
- Persist bridge stdout/stderr separately from Cursor assistant text.

Acceptance criteria:

- Cursor can be launched from Xero CLI with one command after `CURSOR_API_KEY` is configured.
- The run is labeled as Cursor, not custom external.
- Errors produce actionable Xero diagnostics.

## Stage 5: Native Tool Bypass Detection

Deliverables:

- Parse Cursor SDK `tool_call` events.
- Maintain an allowlist of expected MCP tool names or MCP provider/tool ids.
- Flag native Cursor tool calls such as read/write/shell/edit/delete/grep/glob if they appear.
- Add run policy:
  - `recover`: default production UX; attempt automatic reconciliation before surfacing a problem
  - `warn`: record degraded audit detail but continue
  - `fail_on_unrecoverable_native_mutation`: fail only if reconciliation cannot make the workspace and trace safe
  - `fail_on_any_native_tool`: strict diagnostic mode for testing and compliance
- Persist native-tool observations as policy events.

Acceptance criteria:

- A run that uses Cursor-native read is recorded in audit detail but does not interrupt normal UX.
- A run that uses Cursor-native write/patch/delete/shell enters recovery under default production policy.
- A recoverable direct edit completes as a normal run with recovered-change provenance in trace detail.
- An unrecoverable direct edit uses standard Xero review/failure surfaces, not Cursor-specific UI.
- Cursor-native calls are never counted as Xero Tool Registry V2 calls.

## Stage 6: Workspace Diff Reconciliation And Recovery

Deliverables:

- Capture workspace baseline before Cursor starts.
- Capture final workspace diff after Cursor exits.
- Compare final file changes against Xero `file_changed` events.
- Classify:
  - fully accounted by Xero tools
  - untracked direct edit
  - untracked delete
  - untracked generated file
  - ignored build/cache artifact
- Persist reconciliation report in the run trace.
- Add recovery actions:
  - auto-import safe direct edits into the run trace
  - synthesize rollback checkpoints from the pre-run baseline
  - synthesize `file_changed` events with `recovered_cursor_direct_edit` provenance
  - convert safe recovered edits into a Xero-owned patch record where possible
  - auto-revert denied paths from the baseline
  - ask for review only when import/revert cannot be performed safely
- Keep recovery messaging provider-like:
  - "Xero recovered and recorded direct workspace changes."
  - "Xero reverted unsafe changes outside the allowed workspace."
  - avoid exposing raw Cursor/MCP details in routine UI

Acceptance criteria:

- If Cursor writes without Xero MCP, the run records an untracked mutation.
- Safe untracked mutation is automatically imported and appears as recovered Xero file changes.
- Unsafe untracked mutation is automatically reverted when baseline data is available.
- Policy can fail the run when untracked mutation is unrecoverable.
- Ignored directories such as `node_modules`, `target`, `.next`, and caches do not create noisy false failures unless explicitly configured.
- The happy path does not show a Cursor-specific warning to the user.

## Stage 7: Containment And Sandboxing

Deliverables:

- Default to a containment mode for production:
  - disposable copy, temporary worktree, or workspace snapshot strategy
  - no branch/stash creation unless explicitly requested by user policy
- Ensure Cursor state and SDK state live under OS app-data, not `.xero/`.
- Pass a sanitized environment to the bridge.
- Preserve only required variables:
  - `PATH`
  - `HOME` if required by Cursor SDK
  - `CURSOR_API_KEY`
  - controlled Xero variables
- Add kill/cancel behavior for bridge and child MCP server.
- Promote results from containment through Xero tools:
  - compute final diff in the contained workspace
  - apply accepted changes to the real workspace through Xero `patch` or equivalent recovery path
  - persist the promotion as normal Xero file-change events
- Fall back to live-workspace reconciliation only when containment is unavailable or explicitly disabled.

Acceptance criteria:

- Cursor cannot silently corrupt Xero app-data or legacy `.xero/`.
- Cancellation terminates Cursor bridge and MCP server.
- Direct workspace mutations are either contained or reconciled.
- The default production path makes direct Cursor edits recoverable without user intervention.

## Stage 8: Approval And Stage Gate Alignment

Deliverables:

- Map Xero run controls to MCP tool exposure:
  - Ask/Plan: observe-only
  - Engineer/Debug: writes allowed by policy
  - command tools gated separately
- Respect custom Stage allowlists where a run has Stage policy.
- Make MCP server reject tools disallowed by current Stage.
- Persist policy denial details for rejected MCP calls.

Acceptance criteria:

- A Stage that disallows `command` cannot execute `command` through Cursor MCP.
- A write-denied run cannot expose or execute write tools.
- Tool policy behavior matches owned-agent expectations as closely as possible.

## Stage 9: Cloud Cursor Path

Deliverables:

- Keep local Cursor as the production MVP.
- Evaluate cloud only after local is stable.
- For cloud, choose one:
  - package `xero mcp serve-tools` into the Cursor VM
  - expose an authenticated Xero HTTP/SSE MCP bridge
  - require self-hosted Cursor pool with Xero installed
- Add auth for remote MCP:
  - per-run token
  - short TTL
  - scoped project/run permissions
  - audit log for connection lifecycle

Acceptance criteria:

- Cloud path does not expose a local unauthenticated MCP server to the internet.
- Cloud path has the same tool policy and reconciliation semantics as local.

## Stage 10: Tests

Focused test matrix:

- MCP initialize/list/call for observe-only tools.
- MCP write tools unavailable without allow-writes.
- MCP command unavailable without allow-commands.
- Write to `.xero/` denied.
- Write to `.git/` denied.
- Write outside workspace denied.
- Command with network intent denied when policy denies network.
- Cursor bridge streams fixture SDK events into Xero events.
- Native Cursor tool event creates degraded/failure policy event.
- Diff reconciliation catches direct untracked mutation.
- Safe direct edit is auto-imported as recovered file changes.
- Unsafe direct edit is auto-reverted from baseline.
- Unrecoverable direct edit uses standard Xero review/failure response.
- Contained Cursor run promotes accepted diff through Xero patch path.
- Trace export includes Cursor provenance and Xero tool events.
- Cancellation terminates bridge and MCP server.

Acceptance criteria:

- Rust tests are scoped to touched crates.
- Node bridge tests use mocked SDK where possible.
- No test requires a real Cursor subscription by default.
- Optional live test is gated behind explicit environment variables.

## Stage 11: Observability And Diagnostics

Deliverables:

- Add preflight:
  - Node available
  - `@cursor/sdk` available
  - `CURSOR_API_KEY` present or configured
  - model accessible when safe to check
  - xero CLI path resolvable
  - MCP server self-test passes
- Add structured failure codes:
  - cursor_auth_missing
  - cursor_auth_failed
  - cursor_model_unavailable
  - cursor_sdk_bridge_failed
  - cursor_mcp_server_failed
  - cursor_native_tool_bypass
  - cursor_untracked_mutation
  - cursor_run_timeout
- Add support bundle redaction for Cursor API key and MCP auth headers.

Acceptance criteria:

- A failed setup gives the operator a clear next action.
- Support bundles do not leak secrets.
- Logs are sufficient to debug bridge, MCP, and reconciliation separately.

## Stage 12: Documentation And Operator UX

Deliverables:

- Document setup:
  - Cursor dashboard API key
  - local Cursor SDK bridge requirements
  - model selection
  - local-only MVP caveat
- Document safety modes:
  - observe-only
  - write-enabled
  - command-enabled
  - strict-native-tool-deny
- Document expected run labels and trace fields.
- Document the UX contract:
  - Cursor appears as a normal provider/model choice.
  - recovery is automatic for safe changes.
  - Cursor-specific details are available in traces and support bundles.
  - routine UI should not expose bridge, MCP, or SDK internals.
- Add troubleshooting:
  - missing API key
  - Cursor rate/usage limits
  - MCP server not starting
  - native-tool bypass warnings
  - untracked mutation failures

Acceptance criteria:

- A developer can run a smoke test from a fresh checkout without reading source code.
- Documentation does not promise fully native semantics until strict policy and reconciliation are enabled.

## Stage 13: Production Readiness Gate

Required before calling this production-ready:

- Local Cursor SDK path works end to end.
- Xero MCP Tool Registry V2 server exposes only policy-allowed tools.
- Xero persists native-style events for all Xero MCP calls.
- Native Cursor tool bypass is detected.
- Direct workspace mutation reconciliation is implemented.
- Safe direct workspace mutation recovery is implemented.
- Unsafe direct workspace mutation auto-revert is implemented where baseline data is available.
- Routine UI treats Cursor like a normal provider and hides implementation details unless action is required.
- Production policy can fail untracked mutation.
- Cancellation works.
- Trace export works.
- Support bundle redaction works.
- Scoped Rust and Node tests pass.
- Manual live Cursor smoke test passes with a real Cursor API key.

## Out Of Scope For First Production Cut

- Treating Cursor as a normal OpenAI-compatible model provider.
- Claiming strict tool-only enforcement without bypass detection and reconciliation.
- Cursor cloud agents with local stdio Xero tools.
- Adding compatibility for legacy `.xero/` state.
- Branch creation, stashing, or automatic git operations unless explicitly requested.

## Recommended Milestone Slices

1. Cursor SDK bridge with mocked MCP server and JSONL event stream.
2. `xero mcp serve-tools` observe-only Tool Registry V2 wrapper.
3. Write/command gated MCP tools with sandbox denial tests.
4. Xero run persistence for Cursor-backed runs.
5. Native-tool bypass detection from Cursor SDK stream.
6. Workspace diff reconciliation with auto-import and auto-revert recovery.
7. Contained workspace execution with patch promotion.
8. Provider catalog and provider-like UX.
9. Production hardening, docs, and live smoke gate.
