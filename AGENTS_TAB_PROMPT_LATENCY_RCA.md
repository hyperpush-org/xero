# Agents Tab Prompt Latency RCA

Date: 2026-05-05

Scope: first prompt and follow-up prompt submission in the Agents tab for the Tauri desktop app. Line references reflect the current working tree on this date.

## Executive Summary

The first-prompt delay is not caused by a missing optimistic UI path. The React layer does try to render an optimistic user turn immediately, but the durable runtime path is blocked behind a synchronous Tauri command that performs expensive startup work before a run id and runtime stream exist.

Primary root cause: `start_runtime_run` does provider/model preflight and owned-agent run bootstrap synchronously before returning. For OpenAI-compatible providers, the preflight path performs a live HTTP probe using `reqwest::blocking::Client` with a default 30 second timeout. This happens before `persist_owned_runtime_run` emits `runtime_run:updated`, so the frontend cannot subscribe to the runtime stream or show durable progress until the preflight and bootstrap finish.

Secondary contributors can stack on top of this: first-run prompt assembly walks the repo and writes initial agent artifacts before returning; the context meter can build a code map while the user is typing/submitting; long-session stream subscription replays all stored events synchronously; continuation prompts can block on compaction/handoff preparation when enabled.

## User-Visible Symptom

- Sending a prompt, especially the first prompt in a session, takes a long time before visible agent feedback.
- The app can appear busy enough for the cursor to become a loading cursor.
- The first feedback gap is worst on cold provider profiles, stale preflight cache, unreachable/slow OpenAI-compatible endpoints, or cold repo/session state.

## Submission Path

1. The composer calls `handleSubmitDraftPrompt` from the send button or Enter handler.
   - `client/components/xero/agent-runtime/composer-dock.tsx:269`
   - `client/components/xero/agent-runtime/composer-dock.tsx:509`
2. The page creates an optimistic prompt turn, requests scroll-to-latest, then starts the controller submit promise.
   - `client/components/xero/agent-runtime.tsx:1441`
3. On first prompt, the controller awaits `onStartRuntimeRun` before clearing the draft or acknowledging the queued prompt.
   - `client/components/xero/agent-runtime/use-agent-runtime-controller.ts:475`
   - `client/components/xero/agent-runtime/use-agent-runtime-controller.ts:512`
4. Desktop state sets `runtimeRunActionStatus` and awaits `adapter.startRuntimeRun`.
   - `client/src/features/xero/use-xero-desktop-state/run-control-mutations.ts:186`
   - `client/src/features/xero/use-xero-desktop-state/run-control-mutations.ts:206`
5. The adapter invokes the Tauri command `start_runtime_run`.
   - `client/src/lib/xero-desktop.ts:2195`
   - `client/src-tauri/src/commands/start_runtime_run.rs:10`
6. The backend command calls `launch_or_reconnect_runtime_run`, which does synchronous preflight and bootstrap before the frontend gets a run response.
   - `client/src-tauri/src/commands/runtime_support/run.rs:191`
   - `client/src-tauri/src/commands/runtime_support/run.rs:220`

## Confirmed Root Causes

### 1. Provider preflight blocks first-run acceptance

`launch_owned_runtime_run` calls `ensure_owned_runtime_provider_turn_capabilities` before a run id is generated, before a runtime run is persisted, and before `runtime_run:updated` is emitted.

Evidence:

- Provider turn capability check happens before `generate_runtime_run_id`.
  - `client/src-tauri/src/commands/runtime_support/run.rs:257`
  - `client/src-tauri/src/commands/runtime_support/run.rs:266`
- Run persistence and `runtime_run:updated` are after preflight.
  - `client/src-tauri/src/commands/runtime_support/run.rs:267`
  - `client/src-tauri/src/commands/runtime_support/run.rs:281`
- Runtime stream subscription depends on having a persisted run id.
  - `client/src/features/xero/use-xero-desktop-state/runtime-stream.ts:772`
  - `client/src-tauri/src/commands/subscribe_runtime_stream.rs:31`

The slow path is especially clear for OpenAI-compatible providers:

- The selected provider preflight loads provider metadata and then runs live OpenAI Codex or OpenAI-compatible preflight.
  - `client/src-tauri/src/provider_preflight.rs:38`
  - `client/src-tauri/src/provider_preflight.rs:74`
  - `client/src-tauri/src/provider_preflight.rs:83`
- OpenAI-compatible preflight passes `timeout_ms: 0`.
  - `client/src-tauri/src/provider_preflight.rs:207`
  - `client/src-tauri/src/provider_preflight.rs:214`
- `timeout_ms: 0` normalizes to 30,000 ms.
  - `client/src-tauri/crates/xero-agent-core/src/provider_preflight.rs:14`
  - `client/src-tauri/crates/xero-agent-core/src/provider_preflight.rs:941`
- The HTTP probe uses a blocking reqwest client and blocking `send`.
  - `client/src-tauri/crates/xero-agent-core/src/provider_preflight.rs:3`
  - `client/src-tauri/crates/xero-agent-core/src/provider_preflight.rs:528`
  - `client/src-tauri/crates/xero-agent-core/src/provider_preflight.rs:576`

Impact: when the cache is missing or stale, first prompt acceptance can wait on live network I/O. During that wait, the frontend has no durable run id, cannot attach the runtime stream, and cannot receive provider or setup progress events.

### 2. First-run bootstrap is also synchronous before return

After preflight, `launch_owned_runtime_run` persists a runtime run and then calls `create_owned_agent_run` synchronously before returning to the frontend.

Evidence:

- `create_owned_agent_run` is called inline in the command path.
  - `client/src-tauri/src/commands/runtime_support/run.rs:284`
  - `client/src-tauri/src/commands/runtime_support/run.rs:309`
- The background provider-driving thread is spawned only after this inline bootstrap succeeds.
  - `client/src-tauri/src/commands/runtime_support/run.rs:332`
- `create_owned_agent_run` validates the prompt, resolves definitions, builds the tool registry, assembles the system prompt, creates the provider adapter, inserts records, appends initial events/messages, records artifacts, and then reloads the run.
  - `client/src-tauri/src/runtime/agent_core/run.rs:31`
  - `client/src-tauri/src/runtime/agent_core/run.rs:89`
  - `client/src-tauri/src/runtime/agent_core/run.rs:109`
  - `client/src-tauri/src/runtime/agent_core/run.rs:154`
  - `client/src-tauri/src/runtime/agent_core/run.rs:190`
  - `client/src-tauri/src/runtime/agent_core/run.rs:225`

The current workspace's basic git and file-list commands were fast, so repo scanning is probably not the dominant cause here. Still, the architecture puts this work on the accept path, so larger projects or cold disks can turn it into a visible hitch.

### 3. Prompt/context compilation walks the repository on startup paths

Prompt and tool context assembly performs repository discovery work:

- Repository instruction fragments walk for `AGENTS.md` and read matching files.
  - `client/src-tauri/src/runtime/agent_core/tool_descriptors.rs:549`
  - `client/src-tauri/src/runtime/agent_core/tool_descriptors.rs:569`
  - `client/src-tauri/src/runtime/agent_core/tool_descriptors.rs:592`
- The project code map walks source files and manifests.
  - `client/src-tauri/src/runtime/agent_core/tool_descriptors.rs:667`
  - `client/src-tauri/src/runtime/agent_core/tool_descriptors.rs:689`
  - `client/src-tauri/src/runtime/agent_core/tool_descriptors.rs:714`
- Repo fingerprint uses git status.
  - `client/src-tauri/src/runtime/agent_core/persistence.rs:1351`
  - `client/src-tauri/src/runtime/agent_core/persistence.rs:1356`

The current repo has substantial generated/build data on disk (`client/src-tauri/target`, `landing/.next`, `.tmp-gsd2-ref`), and the current ignore configuration mostly protects these paths. The risk remains structural: this work is synchronous and tied to prompt acceptance.

### 4. Feedback architecture makes durable feedback impossible until after expensive work

The UI does set an optimistic prompt turn before starting the backend promise:

- `client/components/xero/agent-runtime.tsx:1451`

But durable runtime feedback depends on `runtime_run:updated` and runtime stream subscription:

- `runtime_run:updated` is emitted only after provider preflight and run persistence.
  - `client/src-tauri/src/commands/runtime_support/run.rs:257`
  - `client/src-tauri/src/commands/runtime_support/run.rs:281`
- The frontend attaches stream subscription only after it has a runtime run/session to subscribe to.
  - `client/src/features/xero/use-xero-desktop-state/runtime-stream.ts:732`
  - `client/src/features/xero/use-xero-desktop-state/runtime-stream.ts:772`

Impact: even if React schedules the optimistic turn quickly, there is no streamed "accepted", "checking provider", or "preparing context" feedback during the highest-latency backend stages.

### 5. Context meter can compete with submit-time work

The context meter schedules backend snapshots from debounced draft/lifecycle changes:

- `client/components/xero/agent-runtime.tsx:798`
- `client/components/xero/agent-runtime.tsx:869`
- `client/components/xero/agent-runtime.tsx:1299`

The backend snapshot command is synchronous and builds context, including prompt compilation and a project code map:

- `client/src-tauri/src/commands/session_history.rs:247`
- `client/src-tauri/src/commands/session_history.rs:605`
- `client/src-tauri/src/commands/session_history.rs:648`
- `client/src-tauri/src/commands/session_history.rs:706`
- `client/src-tauri/src/commands/session_history.rs:2375`

The code map has caps and skips common heavy directories:

- `client/src-tauri/src/commands/session_history.rs:78`
- `client/src-tauri/src/commands/session_history.rs:2468`

Impact: this is not the primary first-prompt blocker, but it can add disk/DB/CPU work exactly while the user is typing or submitting.

### 6. Follow-up prompts can block on synchronous continuation preparation

Continuation prompts use `update_runtime_run_controls` and then backend continuation preparation:

- `client/components/xero/agent-runtime/use-agent-runtime-controller.ts:533`
- `client/src/features/xero/use-xero-desktop-state/run-control-mutations.ts:271`
- `client/src-tauri/src/commands/update_runtime_run_controls.rs:26`
- `client/src-tauri/src/commands/update_runtime_run_controls.rs:96`

The backend can perform synchronous continuation preparation, including context budget checks and optional auto-compaction/handoff work before the prompt is accepted for driving:

- `client/src-tauri/src/runtime/agent_core/run.rs:457`
- `client/src-tauri/src/runtime/agent_core/run.rs:648`
- `client/src-tauri/src/runtime/agent_core/run.rs:707`

Impact: this explains why later prompts can also feel slow, especially in long sessions or when auto compact is enabled.

### 7. Stream subscription replays all existing events synchronously

`subscribe_runtime_stream` loads the run and replays stored events before returning:

- `client/src-tauri/src/commands/subscribe_runtime_stream.rs:31`
- `client/src-tauri/src/commands/subscribe_runtime_stream.rs:96`
- `client/src-tauri/src/commands/subscribe_runtime_stream.rs:148`

This is unlikely to dominate the very first prompt because event count is low, but it can cause hitches when re-entering long sessions.

### 8. Dev-mode runtime stream validation can amplify local hitches

In dev/test mode, runtime stream channel items use full zod parsing:

- `client/src/lib/xero-desktop.ts:1151`
- `client/src/lib/xero-desktop.ts:1177`

The frontend caps stored stream slices, but high-volume event bursts still pay parse/merge costs before being capped.

## Ruled Out Or Lower-Probability Causes

- The UI does not simply forget to add first feedback. It sets an optimistic prompt turn in `client/components/xero/agent-runtime.tsx:1451`.
- Auto-naming is not the first-prompt blocker. It is fire-and-forget after `startRuntimeRun` returns.
  - `client/src/features/xero/use-xero-desktop-state/run-control-mutations.ts:221`
- Current workspace git status and file listing were fast during this analysis, so raw repo size is not the strongest explanation for the first delay in this repo. The blocking live provider preflight is a better match for the symptom.

## Required Fixes

### P0: Make first prompt acceptance fast

Change `start_runtime_run` so it returns after a lightweight accept phase:

1. Validate request and resolve the active project/session.
2. Generate a run id immediately.
3. Persist a runtime run in `Starting` or equivalent accepted state.
4. Persist/queue the initial prompt and attachments as pending work.
5. Emit `runtime_run:updated`.
6. Return the runtime run DTO to the frontend.
7. Continue provider preflight, agent-run creation, context assembly, and provider driving in a background task.

The codebase already has a `RuntimeRunStatus::Starting` state, so use that instead of marking the run `Running` before the agent run actually exists.

Success condition: `adapter.startRuntimeRun` resolves quickly enough for the UI to subscribe to the stream and render durable "queued/starting" feedback before provider/network work begins.

### P0: Move provider preflight off the synchronous submit path

Provider preflight should not block prompt acceptance.

Required changes:

- Run preflight proactively when provider credentials, provider profile, model, or required features change.
- Keep using app-data/global DB persisted preflight snapshots, not repo-local `.xero`.
- On submit, use a valid cached preflight if available.
- If no valid cache exists, start the runtime run in an accepted/checking state and perform preflight in the background.
- Emit stream/runtime events for "checking provider", preflight pass, warning, or failure.
- Reduce or make configurable the live probe timeout for submit-adjacent paths. A 30 second blocking timeout is not acceptable in the prompt acceptance path.
- Prefer async reqwest or a dedicated blocking worker for the probe, but treat that as secondary to the architectural requirement that submit returns before the probe completes.

Success condition: an unreachable OpenAI-compatible endpoint cannot prevent the user from seeing an accepted run and progress state within the first frame or two after submit.

### P0: Emit setup progress before expensive work

Add durable setup events/checkpoints to the runtime stream:

- Prompt queued.
- Checking provider.
- Preparing tools/context.
- Creating agent run.
- Starting provider stream.

These should be emitted from backend state, not as temporary debug UI. They should survive refresh and be visible in the normal user-facing runtime timeline.

Success condition: slow provider preflight or prompt assembly produces visible user-facing progress instead of a silent spinner.

### P1: Move owned-agent bootstrap out of the accept path

`create_owned_agent_run` should run after `start_runtime_run` returns. If it fails, the background task should update the persisted runtime run to failed and emit the diagnostic.

Required changes:

- Persist enough pending-start metadata for background bootstrap to be restartable or recoverable.
- Convert inline bootstrap failures into async run failure updates.
- Keep the existing supervisor lease semantics, but acquire/drive it from the background bootstrap path.

Success condition: slow tool registry creation, system prompt assembly, DB writes, or artifact recording no longer delays the first `startRuntimeRun` response.

### P1: Cache repository prompt-bootstrap inputs

Cache or memoize:

- `AGENTS.md` instruction fragments.
- Project code map.
- Manifest summary.
- Repo fingerprint/dirty status where possible.

Invalidation should be based on repo root plus cheap signals such as file mtimes, git head/index metadata, or a bounded project watcher. Add hard time/file/depth caps so a bad repo layout cannot stall prompt acceptance.

Success condition: first-run prompt assembly does not repeatedly walk the repository when no relevant files changed.

### P1: Suspend or cancel context-meter work during submit

The context meter should not compete with prompt submission.

Required changes:

- Suppress scheduled context snapshot refresh while `runtimeRunActionStatus === 'running'` or a submit is in flight.
- Cancel/ignore stale snapshot requests more aggressively.
- Avoid building a full project code map for every draft-driven snapshot unless the code map cache is warm.

Success condition: typing and pressing send cannot launch a context snapshot build that competes with first-run startup.

### P2: Limit stream replay

Change `subscribe_runtime_stream` to support replay by cursor or bounded latest count.

Required changes:

- Let the frontend send last seen event id/item timestamp when available.
- Replay only events after that cursor, or the latest bounded window for fresh subscribers.
- Avoid sending all historical events synchronously before returning from the command.

Success condition: opening a long session does not replay an unbounded event history on the command response path.

### P2: Reduce dev-mode stream parse overhead

Full zod validation for every runtime channel item is useful while developing schemas, but it can make local high-volume streams feel worse.

Required changes:

- Keep cheap production shape parsing as the default.
- Gate full zod channel validation behind an explicit debug flag or sample it.
- Preserve tests for schema coverage separately.

Success condition: local dev stream bursts no longer pay full schema validation for every item unless explicitly requested.

### P2: Add stage timing instrumentation

Add structured timings around:

- `start_runtime_run` total duration.
- Provider preflight cache lookup and live probe.
- `create_owned_agent_run`.
- `ToolRegistry::for_prompt_with_options`.
- `compile_system_prompt_for_session`.
- `repository_instruction_fragments`.
- `project_code_map_fragment`.
- `repo_fingerprint`.
- `get_session_context_snapshot`.
- `subscribe_runtime_stream` replay count and duration.

This should go to logs/diagnostics, not temporary test UI.

Success condition: future latency reports can identify which stage exceeded budget without guessing.

## Verification Plan

### Automated tests

- Rust test with an injected slow provider preflight: `start_runtime_run` returns an accepted run within a tight budget while the background preflight completes later.
- Rust test with an injected failed provider preflight: the run is first accepted, then transitions to failed with a durable diagnostic.
- Rust test with slow `create_owned_agent_run`: command returns before bootstrap completion.
- Prompt-context cache tests: cache hit avoids repo walk; AGENTS.md/manifest/source changes invalidate correctly.
- Stream subscription tests: replay respects cursor/window and does not replay all historical events by default.
- Frontend test: optimistic prompt turn renders while `onStartRuntimeRun` promise is unresolved.
- Frontend test: context meter does not call `getSessionContextSnapshot` during submit-in-flight unless manually forced.

### Manual/performance checks

- Configure an OpenAI-compatible provider with a slow or unreachable endpoint. First prompt should show queued/checking-provider feedback in under 100-200 ms and should not produce a busy cursor.
- Use a cold project and clear preflight cache. First prompt should still return a run id quickly; preflight status should appear as stream progress.
- Reopen a long agent session. Stream subscription should replay a bounded window or cursor delta, not the full event history.
- In development mode, high-volume stream bursts should not hitch unless full validation is explicitly enabled.

## Recommended Implementation Order

1. Split `start_runtime_run` into fast accept plus background bootstrap.
2. Move provider preflight behind that accepted run state.
3. Add durable setup progress events.
4. Add submit-time context-meter suppression.
5. Cache prompt-bootstrap repo context.
6. Bound stream replay.
7. Add stage timing instrumentation.
8. Reduce dev-mode channel validation overhead.

The first two items are the critical path. Without them, the app can still spend seconds in backend startup before the user sees durable agent feedback.
