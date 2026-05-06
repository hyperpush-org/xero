# Concurrent Agent Code Undo Plan

Reader: internal Xero engineer implementing concurrent-safe code undo and session rollback.

Post-read action: replace snapshot-style rollback with a git-like internal code history that can undo selected code pieces or return one agent session to an earlier point while preserving unrelated user and sibling-agent work.

Last reviewed: 2026-05-06.

## Decision

Replace hard snapshot restore semantics with an internal change ledger, patch rebase engine, and workspace epoch model.

The existing rollback implementation captures app-data snapshots and applies the selected before-snapshot directly to the project. That restores bytes, but it also rewinds later work from the user or other active agent sessions. The new behavior must be closer to `git revert` and `git rebase` than `git reset`.

The core behavior:

- Every agent-owned file mutation becomes an internal commit-like change record with parent head, patchset, before/after file state, affected paths, and causal session metadata.
- Undoing a selected code piece applies the inverse of that selected patch onto the current workspace head.
- Returning a session to an earlier point undoes only the selected session or lineage's later change groups, in reverse order, while preserving non-target user and sibling-agent changes when they do not overlap.
- Undo and rollback operations are themselves new change groups and commit-like records, so they can be audited and undone.
- Conflicting undos fail before writing files and return path/hunk conflict diagnostics instead of partially applying.
- Active sessions learn about undo and rollback through the coordination event stream, mailbox delivery, context package, and stale workspace epoch checks.
- All new state lives in the OS app-data project store. Legacy `.xero/` state is not used.
- The implementation must not create git commits, branches, stashes, worktrees, or hidden repository state.

This plan supersedes the earlier hard-restore behavior described by `AGENT_CODE_ROLLBACK_PLAN.md`.

## User Model

Users experience code history at the conversation level:

- "Undo this piece" removes one selected file, hunk, or change group from the current workspace while keeping later unrelated work.
- "Go back to here" returns the selected agent pane/session to that earlier code boundary by undoing that session's later changes, not by restoring the entire repository to an old snapshot.
- If another agent or the user edited the same lines or file state in a way that cannot be merged, Xero reports a conflict and leaves the workspace unchanged.
- Other active agent panes are notified when an undo changes paths they may be reading, editing, testing, or reserving.

## Scope

In scope:

- Agent-owned file writes, edits, patches, deletes, renames, mkdirs, notebook edits, mutating commands, mutating MCP calls, and undo operations.
- Selective undo by change group, file entry, and text hunk.
- Session rollback to a selected conversation/code boundary that preserves non-target current work.
- Tracked, untracked, ignored-but-explicitly-edited, text, binary, symlink, permission, rename, delete, and create operations.
- Dirty current workspace changes that were not captured as agent changes, treated as current overlays that must be preserved or reported as conflicts.
- Coordination events, mailbox notices, file reservation invalidation, stale-run detection, and provider context updates.
- User-facing ShadCN UI for real undo and rollback controls. No temporary debug or test-only UI.

Out of scope:

- Rewriting raw conversation history.
- Reverting remote services, databases outside the project store, emulator state, Docker volumes, package manager caches, deployed programs, or other external side effects.
- Using real git history as the implementation mechanism.
- Preserving backward compatibility with the snapshot-only app-data schema once the new migration lands.
- Browser validation. This is a Tauri app; verification must use Rust tests, TypeScript tests, and Tauri-oriented harnesses.

## Current Foundations

Xero already has useful pieces to build on:

- App-data snapshot/blob storage for code rollback.
- Change groups and file versions attached to agent sessions, runs, tool calls, and runtime events.
- Exact-path capture for direct file tools and broad capture for commands and mutating MCP tools.
- A Tauri command that applies a selected change group's before snapshot as a hard restore.
- Runtime stream and session history projections that can carry code change group ids.
- Active-agent coordination with presence, file reservations, recent events, and a TTL mailbox.
- Provider context packaging that includes active coordination state and mailbox deliveries.

The missing pieces are patch semantics, causal heads, conflict-safe inverse application, and making active sessions treat history operations as workspace-changing events.

## Target Concepts

Internal code commit: a commit-like app-data record for one code change group. It records parent head id, resulting tree id, patchset id, agent session, run, tool call, timestamps, summary, and workspace epoch. It is not a git commit.

Code tree: a content-addressed manifest for the project file state at a head. Existing snapshot manifests can evolve into this role.

Patchset: the replayable delta from parent tree to result tree. It contains per-file changes, hunks for text files, blob ids for binary or exact content changes, rename/mode/symlink metadata, and base hashes.

Workspace head: the current materialized internal head for the project, plus a monotonic workspace epoch. Every successful agent change, undo, or recovered mutation advances the head and epoch.

Path epoch: the latest workspace epoch that affected a path. Active sessions and file reservations use this to detect stale observations.

Selective undo: an inverse patch operation for one selected change group, file change, or hunk, rebased onto the current workspace head.

Session rollback: an inverse batch operation that targets all change groups after a selected boundary for one agent session or run lineage, while leaving other sessions' changes in the current workspace.

Conflict: a clean pre-write failure when the inverse patch cannot be rebased onto the current file state without overwriting unrelated work.

## Invariants

- Conversation history remains append-only.
- Code undo changes only project files and app-data history metadata.
- Undo never intentionally overwrites unrelated user or sibling-agent work.
- Every successful undo creates a new code change group, patchset, internal commit, coordination event, and session history event.
- Every failed undo records an operation row with conflict or operational diagnostics.
- No partial apply is reported as success.
- The current workspace is the authority for conflict checks, not stale transcript text.
- A run cannot write to paths whose path epochs advanced after its last observed workspace epoch unless it refreshes or acknowledges the change.
- File reservations overlapping changed paths are invalidated or marked stale by undo operations.
- Mailbox notices are advisory but must be included in context for affected active sessions.
- App-data storage is source of truth. `.xero/` is not used.

## Data Model Direction

Reuse the existing content-addressed blob store where possible, but replace snapshot-only semantics with commit and patch semantics.

Add or reshape records:

| Entity | Purpose |
| --- | --- |
| `code_workspace_heads` | Current internal head, tree id, workspace epoch, and last history operation per project. |
| `code_commits` | Commit-like change records for agent changes, recovered mutations, undo, and session rollback. |
| `code_trees` | Manifest/tree records keyed by content or generated ids. |
| `code_patchsets` | Patchset header for a change group or undo operation. |
| `code_patch_files` | Per-path file delta metadata, including base/current blob ids, operation, hunk ids, and merge policy. |
| `code_patch_hunks` | Text hunk ranges and line payloads needed for selective hunk undo. |
| `code_history_operations` | User-requested undo/session rollback operation audit records with status, mode, conflicts, and result commit id. |
| `code_path_epochs` | Latest workspace epoch and commit id for each affected path. |
| `agent_coordination_invalidations` | Durable enough records for active runs to see path reservations invalidated by history operations. |

Existing `code_change_groups` and `code_file_versions` can remain as user-facing projections during the rewrite, but their restore meaning must change from "snapshot restore available" to "patch undo availability".

## Undo Semantics

### Selective Undo

Input can target:

- A whole code change group.
- One file inside a code change group.
- One or more text hunks inside a file change.

Planner behavior:

1. Load the selected patch piece and its base file state.
2. Scan the current workspace state for affected paths.
3. Build the inverse delta from selected after-state back to selected before-state.
4. Rebase that inverse delta onto the current state using a three-way merge:
   - base: file state before the selected change
   - selected: file state after the selected change
   - current: file state now
5. If the inverse touches lines or metadata that unrelated current work also changed, return conflicts before writing.
6. If clean, apply all file writes atomically enough that failure cannot be mistaken for success.
7. Record the result as a new undo commit and change group.

Binary, symlink, mode, create, delete, and rename operations use exact-state rules until a safe merge strategy exists. If current state differs from the selected after-state in an overlapping way, the undo conflicts instead of guessing.

### Session Rollback

Input targets a session/run boundary, such as "return this agent pane to before change group X".

Planner behavior:

1. Resolve the selected boundary to a session or lineage-specific internal commit.
2. Collect later completed change groups owned by that session or run lineage.
3. Exclude change groups owned by other sessions, user/recovered current overlays, and sibling agents unless the user explicitly selected them.
4. Reverse the targeted groups in newest-to-oldest order.
5. Rebase each inverse patch onto the current workspace state.
6. If an inverse patch conflicts with preserved current work, stop before writing and return conflicts for the whole batch.
7. If clean, apply as one session rollback operation with one resulting commit and one user-visible history event.

This produces "my agent goes back to there" behavior without deleting unrelated later changes.

## Coordination And Mailbox Semantics

Undo and session rollback are same-project coordination events.

Each successful or failed operation should:

- Append a coordination event with operation id, mode, affected paths, status, target session/run/change group, result commit id, and conflict summary when relevant.
- Publish high-priority mailbox notices to active runs with overlapping reservations or recent path activity.
- Publish normal-priority mailbox notices to other active runs in the same project when the operation changes broad shared state.
- Invalidate or mark stale active file reservations for affected paths.
- Advance workspace and path epochs.
- Cause future provider context packages to include recent history operations and pending mailbox items.
- Cause mutating tool preflight to fail closed when a run's observed epoch is stale for its intended paths.

Add mailbox item types or equivalent event categories for:

- `history_rewrite_notice`
- `undo_conflict_notice`
- `reservation_invalidated`
- `workspace_epoch_advanced`

Agents should see concise model-visible guidance: what changed, which paths are affected, whether their reservation is stale, and that they must re-read current files before overlapping writes.

## UI Direction

Use existing conversation changed-file entries and diff surfaces as the entry points.

Expected user-facing controls:

- A ShadCN action menu on code-changed file entries with "Undo this file change" and "Return session to here".
- Hunk selection in the diff viewer for text changes, using ShadCN checkboxes or command actions where the existing design system fits.
- A real conflict surface when an undo cannot apply. It should show affected paths and a concise reason, not a debug dump.
- Existing repository status and diff views refresh after successful operations.
- Active panes receive normal session/runtime updates; do not add temporary banners or debug-only panels.

Terminology should shift from "rollback" to "undo" for selective operations and "return session to here" for time-bound operations. "Rollback" may remain an internal compatibility alias only while the rewrite is in progress.

## Milestones And Slices

Each slice should include focused tests for the behavior it changes. Rust work should use scoped tests and formatting; do not run repo-wide Rust commands unless a slice explicitly requires it.

### Milestone 1: Commit Ledger Foundation

Goal: capture new changes as patchable internal commits while keeping the current app functional.

- [ ] **S01: Add History Operation Contracts** `risk:medium` `depends:[]`
  - Add request/response DTOs for selective undo, session rollback, operation status, conflict records, workspace head, and patch availability.
  - Acceptance: TypeScript and Rust contract tests reject unknown modes/statuses and require operation ids, target ids, affected paths, and conflict payloads.

- [ ] **S02: Add Workspace Head And Epoch Storage** `risk:medium` `depends:[S01]`
  - Add app-data records for current head, workspace epoch, path epochs, and latest history operation.
  - Acceptance: scoped DB tests initialize a project head, advance epochs for paths, and reload the same head after reopening the store.

- [ ] **S03: Add Commit And Patchset Storage** `risk:high` `depends:[S02]`
  - Persist internal commits, patchsets, per-file patch metadata, and text hunk rows for one completed change group.
  - Acceptance: a synthetic modify/create/delete patchset round trips with blob ids, base hashes, hunk payloads, and session/run metadata.

- [ ] **S04: Convert Exact-Path Capture To Commit Creation** `risk:high` `depends:[S03]`
  - When file tools complete, create a patchset commit from before/after file versions and advance workspace/path epochs.
  - Acceptance: write, edit, patch, delete, rename, mkdir, and notebook edit captures still emit file-change events and now also produce a current head commit.

- [ ] **S05: Convert Broad Capture To Commit Creation** `risk:high` `depends:[S03]`
  - When command or mutating MCP capture completes, create a patchset commit from manifest differences.
  - Acceptance: scoped command-capture tests produce patch files for changed text files, binary files, deletes, and generated ignored paths that were explicitly edited.

- [ ] **S06: Surface Head Metadata In Runtime History** `risk:low` `depends:[S04,S05]`
  - Include commit id, workspace epoch, and patch availability in runtime stream/session history projections.
  - Acceptance: TypeScript schemas parse the metadata and existing transcript rendering keeps working without new visible controls.

### Milestone 2: Selective Undo Engine

Goal: undo one selected change piece on top of the current workspace without losing unrelated work.

- [ ] **S07: Implement Text Inverse Patch Planner** `risk:high` `depends:[S04]`
  - Build inverse hunks for one text-file change and classify clean apply vs conflict against current content.
  - Acceptance: tests cover clean revert, unrelated later edits preserved, overlapping line conflict, file missing conflict, and newline edge cases.

- [ ] **S08: Implement File Operation Inverse Planner** `risk:high` `depends:[S07]`
  - Add inverse planning for create, delete, rename, mode, symlink, and binary file changes using exact-state conflict rules.
  - Acceptance: tests cover clean inverse and conflict cases for each operation type.

- [ ] **S09: Apply One File Undo Atomically** `risk:high` `depends:[S08]`
  - Apply a clean inverse for one selected file change, record an undo commit, update path epochs, and refresh repository status.
  - Acceptance: a file-level undo preserves an unrelated edit in another file and leaves no pending operation on success.

- [ ] **S10: Apply Multi-File Change Group Undo** `risk:high` `depends:[S09]`
  - Plan and apply all selected files in one change group as a single operation with preflight conflict detection for every affected path.
  - Acceptance: if any file conflicts, no file is written; if all are clean, one undo commit records the full batch.

- [ ] **S11: Add Hunk-Level Undo Selection** `risk:medium` `depends:[S10]`
  - Support operation requests that select specific hunk ids within a text file patch.
  - Acceptance: undoing one hunk leaves other hunks from the same original change intact and records selected hunk ids in the operation audit.

- [ ] **S12: Replace Hard Restore Command Path** `risk:medium` `depends:[S10]`
  - Route the existing rollback command surface to the new undo operation for whole change groups, then remove hard snapshot apply from user-triggered paths.
  - Acceptance: tests prove the old command no longer restores an entire before-snapshot over unrelated current changes.

### Milestone 3: Session Rollback That Preserves Other Work

Goal: return one agent session or lineage to an earlier boundary without undoing unrelated user or sibling-agent changes.

- [ ] **S13: Resolve Session Boundary Targets** `risk:medium` `depends:[S06]`
  - Map a conversation item, change group, checkpoint, or run boundary to the corresponding internal commit and session lineage.
  - Acceptance: tests resolve top-level runs, child runs, and missing/non-code boundaries with deterministic errors.

- [ ] **S14: Build Targeted Lineage Undo Plan** `risk:high` `depends:[S13]`
  - Collect completed change groups after the boundary for the selected session/lineage and order them newest-to-oldest.
  - Acceptance: sibling-session and user/recovered changes are excluded unless explicitly selected.

- [ ] **S15: Preserve Dirty Current Workspace Overlay** `risk:high` `depends:[S14]`
  - Scan current files before planning and treat uncaptured current edits as preserved overlays that can cause conflicts.
  - Acceptance: user edits after the target boundary survive a clean session rollback and conflict when they overlap targeted inverse hunks.

- [ ] **S16: Apply Session Rollback Batch** `risk:high` `depends:[S15]`
  - Apply all targeted inverse patchsets as one operation with one resulting commit and one session history event.
  - Acceptance: three-session tests show rolling back session A preserves later session B changes and reports conflict when both changed the same lines.

- [ ] **S17: Recover Interrupted History Operations** `risk:medium` `depends:[S16]`
  - Make pending undo/session rollback operations idempotent across startup and prevent misleading success after interruption.
  - Acceptance: simulated crash tests converge to completed, failed, or repair-needed state without duplicate commits.

### Milestone 4: Coordination And Mailbox Awareness

Goal: active agent panes know when undo changed their world and cannot blindly write stale assumptions.

- [ ] **S18: Publish History Coordination Events** `risk:medium` `depends:[S09]`
  - Emit coordination events for successful and failed undo operations with mode, paths, status, conflicts, target ids, and result commit.
  - Acceptance: active coordination context returns recent history events and expires them through the existing TTL cleanup path.

- [ ] **S19: Add Mailbox Notices For Affected Runs** `risk:medium` `depends:[S18]`
  - Publish targeted mailbox notices to active runs with overlapping reservations or recent path activity.
  - Acceptance: affected runs receive high-priority inbox items; unaffected runs either receive normal project notices or no notice according to the delivery rule.

- [ ] **S20: Invalidate Overlapping Reservations** `risk:high` `depends:[S18]`
  - Mark active reservations stale or invalidated when an undo changes their paths.
  - Acceptance: reservation queries show invalidation reason and affected runs can no longer rely on the old lease without renewal.

- [ ] **S21: Enforce Workspace Epoch On Mutating Preflight** `risk:high` `depends:[S02,S20]`
  - Block mutating tool preflight when the run's observed epoch is older than the affected path epoch.
  - Acceptance: an active run that has not observed an undo cannot write overlapping paths until it refreshes context or acknowledges the notice.

- [ ] **S22: Inject History Notices Into Provider Context** `risk:medium` `depends:[S19]`
  - Include recent undo/session rollback events and mailbox notices in active coordination prompt fragments and manifests.
  - Acceptance: context package tests show concise model-visible guidance for stale paths without promoting mailbox content to durable memory.

- [ ] **S23: Add Agent Acknowledge/Refresh Flow** `risk:medium` `depends:[S21,S22]`
  - Extend coordination tooling so agents can acknowledge a history notice and renew stale reservations after re-reading current files.
  - Acceptance: a blocked run can read inbox, acknowledge, re-check reservations, and then pass mutating preflight with the new observed epoch.

### Milestone 5: User-Facing Undo Controls

Goal: expose the new behavior from existing conversation and diff surfaces using real ShadCN UI.

- [ ] **S24: Rename Frontend Contracts From Rollback To Undo** `risk:medium` `depends:[S12]`
  - Add frontend model methods and schemas for undo/session rollback while keeping any temporary internal alias hidden from user-facing copy.
  - Acceptance: UI tests and schema tests use "undo" and "return session to here" terminology.

- [ ] **S25: Add Change-Entry Undo Menu** `risk:medium` `depends:[S24]`
  - Add a ShadCN action menu to changed-file entries for "Undo this file change" and whole-change undo.
  - Acceptance: selecting the action calls the new command, refreshes repository/session state, preserves scroll, and shows existing error handling on conflict.

- [ ] **S26: Add Hunk Selection UI** `risk:medium` `depends:[S11,S25]`
  - Let users select one or more diff hunks and submit a hunk-level undo operation.
  - Acceptance: selected hunks are represented in the command payload; no test-only or debug-only controls are added.

- [ ] **S27: Add Return-Session-To-Here Control** `risk:medium` `depends:[S16,S25]`
  - Add the session-boundary action on eligible conversation/code boundary entries.
  - Acceptance: the UI passes a session/run boundary target, shows success as a new history event, and does not imply unrelated sessions were rewound.

- [ ] **S28: Add Conflict UX** `risk:medium` `depends:[S25,S26,S27]`
  - Display conflict results with affected paths, selected hunks/files, and concise reasons.
  - Acceptance: conflict UI is user-facing, keyboard accessible, and uses existing design patterns rather than temporary debug output.

### Milestone 6: History, Search, Export, And Memory Correctness

Goal: all durable projections understand undo without teaching agents false facts.

- [ ] **S29: Session History Projection For Undo Operations** `risk:medium` `depends:[S16]`
  - Render undo/session rollback operations as append-only history items with operation mode, target, result commit, and affected paths.
  - Acceptance: export and search include undo events chronologically without mutating old transcript rows.

- [ ] **S30: Context Visualization Contributors** `risk:medium` `depends:[S22,S29]`
  - Show undo-related context contributors in the existing context visualization model.
  - Acceptance: context tests show history events and mailbox notices with bounded token estimates and redaction.

- [ ] **S31: Memory Candidate Guardrails** `risk:medium` `depends:[S29]`
  - Prevent memory extraction from promoting rolled-back implementation details as durable facts unless the undo operation provenance is included.
  - Acceptance: tests show reverted code facts are rejected, scoped, or explicitly marked as historical.

- [ ] **S32: Update User And Support Docs** `risk:low` `depends:[S29,S31]`
  - Update session memory/context documentation to distinguish conversation rewind, selective undo, and session rollback.
  - Acceptance: docs explain that code undo preserves unrelated current work and that external side effects are out of scope.

### Milestone 7: Cleanup And Hardening

Goal: remove snapshot-restore behavior and harden performance, retention, and edge cases.

- [ ] **S33: Remove User-Reachable Hard Snapshot Apply** `risk:medium` `depends:[S12,S29]`
  - Delete or privatize code paths that apply a whole old snapshot to the project as a user rollback action.
  - Acceptance: no public Tauri command or frontend method can trigger destructive whole-project restore by selecting an old change group.

- [ ] **S34: Storage Retention For Patch History** `risk:medium` `depends:[S03,S17]`
  - Update blob/tree pruning so no blob reachable from commits, patchsets, undo operations, or conflict diagnostics is deleted.
  - Acceptance: retention tests keep reachable before/after blobs and prune only unreachable old blobs.

- [ ] **S35: Performance Budget For Large Repos** `risk:medium` `depends:[S05,S16]`
  - Add bounded scanning, diff limits, and payload budget behavior for broad patch capture and session rollback planning.
  - Acceptance: large synthetic tree tests avoid unbounded payloads and preserve exact explicit edits even under skip rules.

- [ ] **S36: End-To-End Multi-Agent Harness** `risk:high` `depends:[S23,S28,S31]`
  - Add a Tauri/Rust integration scenario with two active sessions editing adjacent and overlapping paths, then selective undo and session rollback.
  - Acceptance: adjacent changes are preserved, overlapping changes conflict, mailbox notices are delivered, stale writes are blocked, and acknowledgements allow continued work.

## Proof Strategy

Use scoped verification at each slice:

- Rust DB tests for schema, storage, commit ledger, patch planning, apply/recovery, coordination, and mailbox behavior.
- TypeScript schema and state tests for contracts, runtime stream normalization, session history projections, and UI event payloads.
- Component tests for ShadCN action menus, hunk selection, conflict display, and scroll/selection preservation.
- Tauri-oriented integration tests for multi-session concurrency. Do not validate by opening the app in a browser.
- Focused formatting for touched Rust and TypeScript files only.

Minimum scenario matrix:

| Scenario | Expected Result |
| --- | --- |
| Undo selected text hunk with unrelated later edit in same file | Hunk is removed; unrelated edit remains. |
| Undo selected text hunk with overlapping later edit | Operation conflicts; workspace unchanged. |
| Undo file create | File removed only if current state still safely matches or has no preserved overlay. |
| Undo file delete | File restored unless path was reused by unrelated current work. |
| Undo rename | Rename is reversed unless source/target paths conflict. |
| Session A rollback after Session B edited different files | Session A changes are removed; Session B files remain. |
| Session A rollback after Session B edited same lines | Operation conflicts; no files change. |
| User edits file outside agent after target boundary | Edit is preserved or reported as conflict if overlapping. |
| Active session has reservation on changed path | Reservation becomes stale/invalidated and mailbox notice is delivered. |
| Active stale session attempts write | Mutating preflight blocks until refresh/acknowledgement. |
| Interrupted undo operation | Startup recovery records completed/failed/repair-needed without duplicate commits. |

## Implementation Notes

- Keep snapshots as raw material for trees and blobs during the transition, but stop presenting snapshot restore as the user model.
- Prefer deterministic patch and merge code over shelling out to git. Git can inspire the model, but real git state must not be mutated.
- Record enough conflict detail for users and agents to act: path, selected file/hunk, conflict kind, base hash, current hash, and concise message.
- For broad command captures, patch generation can start conservative. Exact text hunk undo should be reliable before broad command hunk undo is exposed.
- Unknown external side effects remain marked untracked. Undo only covers project file state.
- Agents should be told to re-read affected files after history notices; mailbox content is advisory and never overrides current file evidence or user instructions.

## Definition Of Done

- Users can undo a selected file/hunk/change group without losing unrelated current work.
- Users can return one agent session to an earlier code boundary without undoing other sessions' independent changes.
- Conflicts are detected before writes and leave the workspace unchanged.
- Undo/session rollback operations are visible in session history, search, export, context visualization, and runtime stream updates.
- Active sibling agents receive mailbox/coordination notices and cannot write stale overlapping paths without refresh.
- File reservations become stale or invalidated when history operations affect them.
- No user-reachable command performs hard whole-project snapshot restore for normal rollback/undo.
- All state lives in the app-data project store, not `.xero/`.
- Scoped tests cover clean apply, conflicts, multi-session preservation, mailbox delivery, stale preflight blocking, and recovery.
