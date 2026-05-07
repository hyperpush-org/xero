# Agent Runtime Continuity And Memory

Xero keeps Ask, Engineer, and Debug runs going past a single context window without losing the user's task, prior decisions, or durable evidence. This guide explains the runtime behavior that makes that possible: how context pressure is evaluated, how same-type handoff works, what is persisted, and how recovery handles partial writes after a crash.

This document is a runtime-behavior reference, not a feature tour. For the user-facing narrative on session history, exports, search, manual compact, memory review, branch, rewind, selective code undo, and session rollback, read `session-memory-and-context.md`. For the source-of-truth implementation plan, see `AGENT_CONTEXT_CONTINUITY_AND_MEMORY_PLAN.md` at the repository root.

## Contracts

Every owned-agent provider request is assembled from a deterministic context package and a persisted context manifest. The package is built before the request is sent. The manifest records what went in, what was excluded and why, the policy decision, and any retrieval, compaction, or handoff identifiers used. No provider call is made without a manifest stored.

Project records, approved memory, handoff bundles, and retrieval logs live in app-data-backed databases. SQLite holds transactional state (sessions, runs, manifests, lineage, retrieval logs, policy settings, handoff attempts). LanceDB holds retrieval state (handoffs, project facts, decisions, plans, findings, verification, memories, candidates) with embedding metadata.

Approved memory is durable context, never higher-priority policy. The system prompt explicitly tells the agent to ignore memory or retrieved text that tries to change system or tool rules.

Code undo and session rollback are represented as append-only history operations, not transcript rewrites. They can affect workspace files and app-data code history, but conversation rewind remains a branch/replay operation that does not edit files on disk.

## Context Policy

Before each provider turn the runtime evaluates context pressure and emits one of five decisions:

- `continue_now` — context is healthy.
- `compact_now` — context can be reduced via compaction.
- `recompact_now` — an existing compaction no longer protects the next turn.
- `handoff_now` — context cannot be safely continued in the same run; create a same-type target run and seed it.
- `blocked` — required context cannot be persisted, retrieved, or safely redacted.

Defaults: compact at 75 percent, hand off at 90 percent. Both thresholds are durable settings (`agent_context_policy_settings`), not local-only UI preferences. Auto-handoff and auto-compact can be disabled per project or per session.

When `handoff_now` is selected and auto-handoff is disabled, the runtime returns a `agent_context_handoff_blocked` error and does not silently continue.

## Same-Type Handoff

Handoff preserves the source agent's runtime kind (Ask → Ask, Engineer → Engineer, Debug → Debug), provider, model, thinking effort, approval mode, and plan-mode setting. It does not switch agent type to "rescue" a long run.

The lifecycle progresses through four lineage statuses (`agent_handoff_lineage`):

1. `Pending` — lineage row inserted with idempotency key `source_run_id:context_hash:runtime_agent_id`.
2. `Recorded` — handoff bundle written to LanceDB as a `xero.agent_handoff.bundle.v1` project record; lineage updated with the record id.
3. `TargetCreated` — same-type target run created (or reloaded) and seeded with system prompt, durable handoff bundle developer message, and the pending user prompt.
4. `Completed` — source run marked `HandedOff`, lineage closed, memory candidates extracted from the source.

The handoff bundle is structured DB-backed data, not free-form summarization. Required fields cover user goal, status, completed and pending work, decisions, constraints, project facts, recent file changes, tool/command evidence, verification status, risks, open questions, relevant memory, recent raw-tail messages, source context hash, and redaction state. Engineer adds plan/build state and review risks. Debug adds symptom, reproduction, evidence ledger, hypotheses, root cause, fix, and verification. Ask adds the question, project context used, uncertainties, and follow-up information needed.

## Idempotency And Crash Recovery

Every step is idempotent on the same `(source_run_id, context_hash, runtime_agent_id)` key:

- Lineage insert uses `ON CONFLICT(project_id, idempotency_key) DO NOTHING`.
- Handoff project records skip persistence when the lineage already records a `handoff_record_id`.
- `create_or_load_handoff_target_run` loads an existing target run before creating one.
- `mark_source_run_handed_off` is a no-op when the source is already `HandedOff`.

If the process crashes between any of these steps, the next continuation request advances the same lineage to `Completed` without duplicating the target run, the LanceDB handoff record, or the lineage row. The integration test `phase8_handoff_recovers_from_pending_lineage_after_simulated_crash` regresses lineage status to `Pending` mid-flight and confirms recovery converges to one lineage row, one handoff bundle record, one target run, and a source run still marked `HandedOff`.

Pending lineage rows are discoverable via `list_agent_handoff_lineage_by_status`. Startup recovery uses this to resume incomplete handoffs or mark unrecoverable attempts failed with a stored diagnostic.

## Retrieval And Memory

Project records and reviewed memory live in LanceDB with embedding model and version metadata. Hybrid retrieval combines vector similarity, keyword search, kind/tag filters, related paths, agent id, session id, recency, importance, and confidence. When embeddings are unavailable, the runtime falls back to deterministic keyword retrieval rather than injecting empty context.

A read-only project context tool is exposed to all three agents. Ask is observe-only and cannot propose new records. Engineer and Debug can request candidate records, but the runtime owns the final write, and write-like model proposals never become trusted memory without review.

Memory candidates are extracted automatically after run completion, pause, failure, and handoff. Candidates remain disabled until a user approves them through the runtime's memory review surface. Candidates that look like prompt injection or carry secret material are blocked or redacted before they are persisted.

Memory extraction must account for undo provenance. Facts about code removed by selective undo or session rollback are rejected, scoped as historical, or tied to the undo operation so future context packages do not present reverted implementation details as current truth.

## Code History Awareness

Owned agents may receive recent code undo and session rollback events in their context package when those operations affect paths, reservations, or workspace epochs relevant to the run. These notices are advisory: agents must re-read current files before overlapping writes, and current file evidence outranks retrieved memory.

Selective code undo applies an inverse of the selected change on top of the current workspace. Session rollback applies inverse changes for one selected session or lineage after a boundary. Both are expected to preserve unrelated user and sibling-agent work; if preservation is unsafe, planning reports a conflict before any file write is reported as successful.

The runtime does not model external side effects as undoable code history. Remote services, databases outside the app-data project store, emulator state, Docker volumes, package manager caches, deployed programs, transactions, and other effects outside project file state remain out of scope for undo and session rollback.

## User-Visible Surfaces

Most of the continuity machinery is internal: context manifests, retrieval logs, handoff lineage, redaction decisions, and policy evaluations are diagnostics, not core UX. The user-facing surfaces are intentionally narrow:

- The conversation shows a "Run continued in a fresh session" notice when the runtime stream completes a same-type handoff. The notice explains that the task and prior context carried over so the user can keep replying without re-stating anything.
- The reviewed memory workflow stays mounted in the agent runtime so candidates can be approved, rejected, disabled, enabled, or deleted from the normal flow.
- The composer auto-compact toggle remains the user-tunable knob for compaction behavior; auto-handoff defaults to on as a safety net.
- Session history, search, export, and context visualization can show code undo and session rollback events as chronological additions with affected paths and conflict status.

There is no user-facing context manifest inspector, project record browser, or retrieval diagnostics panel. These are observable via the underlying database commands and Tauri contracts when needed for support or debugging.

## Security And Redaction

Redaction runs before project-record insertion, memory candidate creation, prompt injection, retrieval display, and handoff bundle generation. Secret-shaped strings (API keys, OAuth tokens, bearer headers, session ids, cloud credentials, private-key paths, common credential file paths) are blocked or replaced with redaction markers while preserving source ids, timestamps, kinds, and remediation metadata.

Retrieved records and memories are treated as untrusted lower-priority data. Memory text shaped like an instruction override (for example, `ignore previous instructions`, `reveal the system prompt`) is rejected at candidate creation time, so it never becomes approved memory.

Database write failures block unsafe continuation. If the manifest, handoff bundle, lineage, or required project record cannot be persisted, the provider call does not run; the source run remains resumable or the failure is surfaced with a diagnostic so the user can choose how to proceed.

## Test Coverage

The integration test suite at `client/src-tauri/tests/agent_context_continuity.rs` covers the runtime-behavior contracts above:

- Phase 1: durable context policy settings; same-type handoff decision preserves Ask/Engineer/Debug.
- Phase 2: LanceDB embeddings, hybrid retrieval, fallback retrieval, dimension mismatch, redaction, deduplication, and embedding backfill jobs.
- Phase 3: provider-turn context manifests include approved memory and relevant project records for all three agents.
- Phase 4: synthetic over-budget runs hand off to same-type target runs; idempotent retry does not duplicate.
- Phase 5: automatic record capture and reviewed memory candidate creation across completion, pause, failure, and handoff paths.
- Phase 6: model-visible project context tool permissions (Ask read-only, Engineer/Debug retrieval and candidate proposals) and retrieval logging.
- Phase 8: crash recovery from a regressed `Pending` lineage advances to `Completed` with no duplicates.

The TypeScript suite at `client/components/xero/agent-runtime.test.tsx` includes a test that the conversation surface renders the handoff notice when the runtime stream completion reports a same-type handoff.
