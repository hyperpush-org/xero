# Advanced Agentic Commands Plan

Reader: an internal Xero engineer working on the owned-agent harness.

Post-read action: implement typed, policy-aware advanced diagnostics commands that cover the macOS and process-forensics workflows currently possible only through raw shell snippets.

Last updated: 2026-05-03.

## Short Answer

Xero already has the foundation for advanced agentic commands: repo-scoped shell commands, command sessions, an owned-process manager, system process and port visibility, and basic macOS app/window/screenshot automation. The gap is not raw capability. The gap is typed, audited, model-discoverable diagnostics for command shapes like process open-file inspection, CPU/thread sampling, unified log queries, process snapshots, and Accessibility-style macOS window introspection.

Build this as a new diagnostics layer on top of the existing owned-agent tool policy, not as a loose expansion of arbitrary shell access.

## Principles

- Keep every capability model-visible as a typed tool with a narrow schema.
- Preserve the existing approval model. Read-only system observation can be allowed when bounded; screenshots, app control, and external process signaling require approval.
- Prefer structured platform APIs or stable machine-readable command output over free-form shell pipelines.
- Keep output bounded, redacted, and persistable.
- Do not add temporary debug UI. Any UI added later must be real user-facing product UI and use existing ShadCN conventions where possible.
- New project or run state belongs in app-data-backed storage, not legacy repo-local state.
- No backwards-compatibility shim unless explicitly requested.

## Current Baseline

The current harness can already:

- Run short repo-scoped commands with timeout, sanitized environment, output capture, redaction, and command-policy classification.
- Start, read, and stop long-running command sessions.
- Manage Xero-owned long-running and async processes, including output cursors, highlights, grouped lifecycle operations, readiness probes, and cleanup.
- Observe system processes, process trees, and local listening ports.
- Approval-gate external process signaling and external process tree termination.
- Check macOS permissions, list running apps, list windows, focus windows, launch or quit apps, and capture screenshots behind approval.

This means most screenshot-style commands can be forced through raw shell today after policy review. That is useful as a fallback, but it is not good enough as the primary harness surface.

## Gaps To Close

1. Process open files
   - Missing typed equivalent for `lsof -nP -p PID`.
   - Current typed port support only covers listening ports.
   - Need bounded rows for file descriptors, sockets, cwd, executable, and deleted files where the platform exposes them.

2. Process resource snapshot
   - Missing typed equivalent for `top -l 1 -pid PID` or similar.
   - Need CPU, memory, threads, ports, state, elapsed time, virtual size, resident size, and platform-specific fields where available.

3. Thread inspection
   - Missing typed equivalent for `ps -M PID`.
   - Need bounded per-thread rows with ids, state, priority, wait channel, and command/name where available.

4. Process sampling
   - Missing typed equivalent for `sample PID duration interval`.
   - Need approval-aware, time-bounded sampling with output artifact support because profiles can be large.

5. Unified log query
   - Missing typed equivalent for `log show --last ... --predicate ...`.
   - Need strict time windows, process filters, substring filters, severity filters, max rows, and redaction before persistence.

6. macOS Accessibility snapshot
   - Current macOS automation can list apps and windows, but does not expose a structured Accessibility tree or window attributes such as frontmost, visible, position, size, role, title, and selected UI state.
   - Need an approval-aware typed snapshot that avoids arbitrary AppleScript strings.

7. Command history and evidence display
   - Tool results already persist, but advanced diagnostics should produce compact summaries and optional full artifacts so the agent can reason without flooding context.

## Proposed Tool Surface

Add one new deferred group: `system_diagnostics`.

Expose it as one typed tool, `system_diagnostics`, with action-specific schemas. A single tool keeps policy and artifact handling centralized while still giving the model discoverable actions.

Initial actions:

- `process_open_files`
- `process_resource_snapshot`
- `process_threads`
- `process_sample`
- `system_log_query`
- `macos_accessibility_snapshot`
- `diagnostics_bundle`

The `diagnostics_bundle` action should run a safe preset over a target PID or app name and return a compact multi-section report. It should not bypass the policy for any underlying action.

## Request Shape

Common fields:

- `action`
- `pid`
- `processName`
- `bundleId`
- `appName`
- `windowId`
- `since`
- `durationMs`
- `intervalMs`
- `limit`
- `filter`
- `includeChildren`
- `artifactMode`

Action-specific fields:

- Open files: `fdKinds`, `includeSockets`, `includeFiles`, `includeDeleted`.
- Resource snapshot: `sampleCount`, `includePorts`, `includeThreadsSummary`.
- Threads: `includeWaitChannel`, `includeStackHints`.
- Process sample: `durationMs`, `intervalMs`, `maxArtifactBytes`.
- Log query: `lastMs`, `level`, `subsystem`, `category`, `messageContains`, `processPredicate`.
- macOS Accessibility snapshot: `includeChildren`, `maxDepth`, `focusedOnly`, `attributes`.

## Output Shape

Every action should return:

- `action`
- `platformSupported`
- `performed`
- `target`
- `policy`
- `summary`
- `rows`
- `truncated`
- `redacted`
- `artifact`
- `diagnostics`

Rows should be structured objects, not preformatted terminal text. Full raw output, when needed, should be written as a redacted artifact with byte counts and truncation metadata.

## Policy Model

Use these default risk levels:

- System process, thread, resource, and port observation: read-only system observation.
- Open-file inspection: read-only system observation with possible sensitive paths.
- Log query: read-only system observation with possible sensitive messages.
- Process sampling: system read, approval required by default.
- Accessibility snapshot: macOS system read, approval required unless limited to window metadata already available without Accessibility privileges.
- Screenshot or app control remains under the existing macOS automation approval boundary.
- External process signaling stays in the existing process manager.

The model should see why a call is blocked, whether approval would allow it, and what narrower action it can try first.

## Implementation Plan

### Phase 1: Tool Descriptor And Policy Skeleton

Add the deferred `system_diagnostics` group and descriptor.

Add request and response DTOs with deny-unknown-fields behavior. Wire the action enum into central tool dispatch, safety policy evaluation, persistence, and tool-result redaction.

Success condition: the tool appears in tool search, can be activated by group or exact name, rejects malformed requests, and returns unsupported-platform diagnostics without panics.

### Phase 2: Process Open Files And Ports

Implement `process_open_files` first because it maps most directly to the screenshot's `lsof` workflow.

On macOS and other Unix hosts, prefer `lsof` field output for stable parsing. On Linux, prefer `/proc` where practical and fall back to `lsof` when available. On Windows, return unsupported until there is a tested handle-enumeration strategy.

Success condition: a test-owned process with a temp file and local listener produces bounded open-file and socket rows without raw shell text.

### Phase 3: Resource And Thread Snapshots

Implement `process_resource_snapshot` and `process_threads`.

Use platform APIs or stable command output. Normalize the cross-platform fields, and preserve platform-specific extras under a bounded `platform` object.

Success condition: a spawned process can be inspected for pid, state, memory-ish data, CPU-ish data when available, and a bounded thread list without requiring shell pipelines.

### Phase 4: Unified Log Query

Implement `system_log_query` for macOS first.

Require process, subsystem, category, or message filters. Clamp the time window and row count. Return compact structured rows with timestamp, level, process, subsystem, category, and message excerpt. Persist full redacted output only when explicitly requested.

Success condition: querying recent logs for a known process returns bounded rows or a clear permission/availability diagnostic.

### Phase 5: Process Sampling

Implement `process_sample` as an approval-required action.

Keep short defaults, strict maximum duration, cancellation support, and artifact-backed output. The model should receive a compact summary plus the artifact path, not the entire profile.

Success condition: sampling a spawned test process produces a bounded artifact and a summary, times out cleanly, and never leaves child processes behind.

### Phase 6: macOS Accessibility Snapshot

Implement `macos_accessibility_snapshot` behind the same approval and permission posture as macOS automation.

Start with target resolution by pid, app name, bundle id, or window id. Return frontmost, visible, role, title, focused state, position, size, and a bounded child tree when requested. Avoid arbitrary AppleScript execution in the model-facing API.

Success condition: with permissions available, a running app returns structured window/app attributes; without permissions, the tool returns a user-actionable permission diagnostic.

### Phase 7: Diagnostics Bundle Presets

Add `diagnostics_bundle` presets for common cases:

- `hung_process`
- `port_conflict`
- `tauri_window_issue`
- `macos_app_focus_issue`
- `high_cpu_process`

Each preset should compose existing typed actions and return a compact report. It must stop at approval boundaries instead of silently skipping sensitive actions.

Success condition: a single target PID can produce a useful report with process metadata, port data, resource data, recent logs when supported, and clear blocked-action summaries.

### Phase 8: Product UI And Reporting

Only after the typed tool is stable, add user-facing UI for reviewing diagnostics artifacts and approving sensitive actions.

Use existing UI components and avoid any temporary debug panels. The UI should show the action, target, risk, approval state, compact result, and artifact links.

Success condition: a user can understand what the agent wants to inspect before approving it, and can review the resulting evidence after the run.

## Test Plan

Add focused Rust tests for:

- Descriptor registration and tool-search activation.
- Policy outcomes for each action.
- Malformed request rejection.
- Unsupported-platform diagnostics.
- Open-file parsing fixtures.
- System process and port integration on Unix.
- Resource snapshot parsing fixtures.
- Thread-list parsing fixtures.
- Log-query clamping and redaction.
- Sampling timeout and cleanup.
- macOS permission-gated Accessibility behavior.
- Persistence shape and artifact redaction.

Run scoped tests while iterating. Do not run broad Cargo commands unless the change crosses enough shared runtime boundaries to justify it.

## Migration And Storage

No repository-local `.xero/` state is needed.

Diagnostics artifacts should use app-data or temporary artifact storage consistent with existing process and screenshot artifacts. If artifacts become durable, store metadata in the app-data-backed run persistence tables with redaction status, byte count, source action, target pid/app, and retention policy.

## Rollout

1. Land the descriptor, DTO, policy, and unsupported-platform skeleton.
2. Land process open-file inspection.
3. Land resource and thread snapshots.
4. Land macOS log queries.
5. Land process sampling.
6. Land macOS Accessibility snapshot.
7. Land diagnostics bundle presets.
8. Add product UI only after the backend contract is stable.

Each step should be independently shippable and should improve the agent's ability to debug without increasing raw shell dependence.

## Completion Criteria

The work is complete when Xero can reproduce the intent of the screenshot command set through typed tools:

- Find which process owns a port or open file.
- Inspect a target process tree.
- Inspect resource and thread state.
- Capture a short process sample behind approval.
- Query recent logs for a target process behind bounded filters.
- Inspect macOS app/window state and Accessibility attributes behind approval.
- Persist compact evidence and redacted artifacts.
- Explain every approval boundary clearly to the user and model.

Raw shell remains available as a fallback, but the happy path becomes structured, discoverable, testable, and safer.
