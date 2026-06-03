# Project Context Deduplication Plan

## Reader And Outcome

This plan is for the engineer reducing redundant project-context work in owned-agent runs. After reading it, they should be able to distinguish runtime context setup from model-requested context tools, stop unnecessary context-manifest reads, and make the transcript show what actually happened.

## Problem

Two different things currently look too similar in the transcript:

1. Runtime context setup before a provider turn.
   - Xero records a context manifest.
   - Xero may also run pre-provider durable context retrieval if the prompt looks project-sensitive.

2. Model-requested project-context tools during the provider turn.
   - The model can call `project_context` with `explain_current_context_package`.
   - That action reads the latest manifest that was already recorded.
   - It also logs a manual retrieval row for the manifest and returns model-visible output.

The second screenshot points at this broader issue. Even without accepting an agent switch, the run showed an initial manifest/retrieval pair, then the model called `workspace_index` and `project_context` with the context-manifest action. That is not necessarily a second LanceDB retrieval, but it is still extra context work, extra model-visible tokens, and confusing UI.

## Goals

- Keep one required runtime manifest per provider turn for audit and trace quality.
- Avoid model calls that only re-read the manifest the runtime just recorded.
- Stop classifying manifest inspection as durable context retrieval.
- Make runtime setup, model tool calls, manifest inspection, and durable retrieval visually distinct.
- Deduplicate repeated context actions within a run or turn.
- Keep routing/handoff reuse as one part of the larger dedup story, not the whole story.

## Non-Goals

- Do not remove context manifests before provider turns.
- Do not preload raw project records or approved memory into provider prompts.
- Do not hide real tool calls from the user.
- Do not add temporary debug UI.
- Do not introduce backwards-compatible app-data migrations unless explicitly requested.

## Current Findings

- `assemble_provider_context_package` records a manifest before provider calls and performs first-turn retrieval when the prompt has project-context signals.
- `project_context` exposes `explain_current_context_package` as a normal search action.
- `explain_current_context_package` loads the latest manifest for the run, compacts it, then calls the manual retrieval logger with `sourceKind: context_manifest`.
- Stream projection renders runtime manifest/retrieval events as successful `project_context` tool calls, so runtime setup can look like model tool use.
- Tool prompt text tells agents that context manifests are available through `project_context`, but does not strongly say "do not inspect the current manifest unless auditing context packaging."
- `workspace_index` may also be called for broad project overview prompts even when the index is empty, creating another low-value context call.

## Proposed Model

Treat project context as four separate layers with clear owners:

1. Runtime context package
   - Owner: provider-loop runtime.
   - Records the manifest.
   - Decides whether pre-provider durable retrieval is needed.
   - Not a model tool call.

2. Durable project records and approved memory
   - Owner: `project_context_search` and `project_context_get`.
   - Used when prior decisions, constraints, handoffs, memories, or records may matter.
   - Should remain source-cited and tool-mediated.

3. Context package inspection
   - Owner: runtime diagnostics.
   - Used only when the user asks what context Xero assembled, when a harness probes runtime state, or when debugging context packaging.
   - Should not be treated as durable context retrieval.

4. Workspace index
   - Owner: workspace index service.
   - Used for file/symbol discovery.
   - Should be cached by run, index version, and HEAD so empty/stale status is not repeatedly rediscovered.

## Implementation Slices

### Slice 1: Rename And Separate Transcript Surfaces

Change stream projection so runtime context setup does not masquerade as normal model tool use.

Expected behavior:

- Runtime manifest event displays as `runtime context manifest`.
- Runtime pre-provider retrieval displays as `runtime durable context retrieval`.
- Model `project_context` calls remain normal tool calls.
- Model manifest inspection displays as `context package inspection`, not as "latest project context retrieval."

Verification:

- Add projection tests for runtime manifest, runtime retrieval, model `project_context` search, and model manifest inspection.
- Confirm the screenshot pattern is visually explainable: one runtime setup group plus one optional model inspection group.

### Slice 2: Restrict Manifest Inspection From Normal Agent Behavior

Make `explain_current_context_package` a diagnostic action, not a default retrieval action.

Options, in preferred order:

1. Move it out of the normal `project_context` search enum and expose it only through a diagnostic/context-inspection capability.
2. Keep the enum but gate the action unless the request is a harness probe, explicit user audit request, or context-debugging task.
3. Keep it available but strengthen tool instructions: do not call it for ordinary project understanding, coding, planning, or debugging.

Expected behavior:

- For "What is this project about?", agents inspect repository files and relevant durable records, but do not call `explain_current_context_package`.
- Harness/runtime tests can still inspect context packaging intentionally.

Verification:

- Add prompt/tool-policy tests that the default agent guidance marks manifest inspection as diagnostic-only.
- Add backend tests that unauthorized manifest inspection is skipped or rejected with a user-fixable diagnostic.
- Keep harness coverage by opting harness probes into the diagnostic path.

### Slice 3: Stop Logging Manifest Inspection As Retrieval

Change `explain_current_context_package` so it does not call the manual retrieval logger by default.

Expected behavior:

- Inspecting the current manifest returns a compact manifest summary.
- It records an inspection/audit event if needed.
- It does not create `agent_retrieval_queries` or `agent_retrieval_results` rows that imply durable context retrieval.

Verification:

- Add Rust tests that call manifest inspection and assert no retrieval query/result rows are added.
- Add a separate test for explicit diagnostic logging if an audit event is introduced.

### Slice 4: Add A Context Access Ledger

Add a per-run or per-turn ledger that records context actions already performed:

- Manifest recorded.
- Pre-provider retrieval query and result IDs.
- Model `project_context` searches by normalized action/query/filter.
- Manifest inspection by manifest ID.
- Workspace index status by index version and HEAD.

Use the ledger to deduplicate low-value repeats.

Expected behavior:

- A repeated manifest inspection for the same manifest returns a cached compact summary or a skip diagnostic.
- A repeated durable search with the same normalized request can reuse previous result IDs within the run.
- A repeated workspace-index status call returns cached status without another full lookup when index version and HEAD are unchanged.

Verification:

- Add Rust tests for duplicate manifest inspection, duplicate project-context search, and duplicate workspace-index status.
- Add transcript tests that repeated context actions show `reused` or `cached`, not fresh retrieval.

### Slice 5: Keep Runtime Pre-Provider Retrieval Focused

Revisit the first-turn retrieval trigger so broad prompts do not over-fetch.

Expected behavior:

- Prompts about "this project" may justify lightweight repository reads and maybe a workspace summary.
- Durable context retrieval should run only when previous decisions, memory, handoffs, constraints, or project records are likely useful.
- If retrieval runs and returns no useful results, that fact should be summarized once and not cause the model to inspect the manifest.

Verification:

- Add tests for project overview, prior-work-sensitive prompts, explicit memory/history prompts, and generic coding questions.
- Assert only the prior-work-sensitive cases perform durable retrieval.

### Slice 6: Routing And Handoff Reuse

Keep the earlier routing fix, but place it behind the broader context ledger.

Expected behavior:

- If an agent emits a routing suggestion after runtime retrieval, accepting or declining the suggestion reuses the already-recorded context evidence when still valid.
- A new provider turn still records its own manifest.
- It does not run the same retrieval again unless target policy, prompt hash, attachments, consumed artifacts, or freshness state changed.

Verification:

- Add decline and accept regression tests.
- Assert the post-routing turn records a manifest with reused retrieval IDs and no duplicate retrieval event.

## Acceptance Criteria

- The second screenshot flow no longer shows a model-initiated manifest inspection for an ordinary project overview prompt.
- If manifest inspection does occur, it is labeled as diagnostic inspection and does not create retrieval-query rows.
- Runtime context setup is visually distinct from model tool calls.
- Duplicate context actions within a run are cached or marked reused.
- Routing continuation no longer repeats retrieval for the same source context.
- Focused Rust and frontend tests cover projection, tool gating, retrieval logging, dedup ledger, and routing reuse.

## Suggested Scoped Verification

```text
pnpm --dir client test -- agent-runtime
pnpm --dir client test -- session-history-projection
cargo test -p xero-desktop-lib project_context
cargo test -p xero-desktop-lib agent_core::context_package
```

Run one Cargo command at a time.
