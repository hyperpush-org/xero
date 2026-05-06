# Agent Code Rollback Plan

Reader: internal Xero engineer implementing owned-agent code rollback.

Post-read action: implement a durable rollback system that lets a user restore project files to any earlier agent edit boundary without rewinding or deleting the conversation.

Last reviewed: 2026-05-06.

## Decision

Add a code-state timeline that is separate from the conversation timeline.

Xero already preserves agent sessions, transcripts, file-change metadata, checkpoints, branch lineage, and rewind lineage. Today, rewind can recover a conversation prefix but deliberately does not roll files back on disk. This plan fills that gap by making every agent-owned workspace mutation restorable.

The core behavior:

- Every mutating agent action creates a durable code change group.
- Each change group is tied to whole-project code snapshots, not only the files touched by that action.
- The conversation remains append-only. Rollback adds a new rollback event and updates file state only.
- A rollback operation is itself recorded as a change group, so the user can undo the rollback too.
- Rollback is a hard restore to the selected snapshot. Later workspace changes are overwritten, including user edits.
- Rollback state lives in the OS app-data project store, not in legacy repo-local state.
- The implementation must not create git commits, stashes, branches, or hidden worktrees.

## User Model

The user sees agent edits as historical moments inside the conversation. If a conversation has three edit groups, the user can choose the rollback action on a file changed by the second group and restore the whole workspace to the exact snapshot from immediately before that change. Later conversation turns remain visible because they still explain what happened historically.

The action should feel like:

1. The user reviews a changed file entry in the conversation.
2. The user clicks the rollback button for that changed file.
3. Xero restores the project to the snapshot attached to that change boundary.
4. Xero records that restoration in the same session.
5. The user can continue the conversation from the restored code state without restating history.

Do not add new UI except the rollback button on code-changed file entries. Do not add preview dialogs, conflict dialogs, status panels, banners, temporary debug surfaces, or test-only UI.

## Scope

In scope:

- Files under the imported project root that the owned agent changes.
- Changes made through file tools, patch tools, write tools, rename/delete/mkdir tools, notebooks, mutating commands, mutating MCP tools, and rollback itself.
- Tracked, untracked, ignored-but-explicitly-edited, text, binary, symlink, permission, rename, delete, and create operations.
- Restore to an arbitrary prior agent edit boundary.
- Hard snapshot restore that overwrites the current project file state with the selected snapshot.
- Search, export, and session history projections that show rollback events without mutating old transcript rows.

Out of scope:

- Rewinding the conversation transcript.
- Reverting remote services, cloud state, databases outside the project store, emulator state, Docker volumes, package manager caches, or deployed Solana programs.
- Reconstructing code state before Xero started tracking rollback snapshots.
- Using git history mutations as the rollback mechanism.

If an agent action can mutate state outside the project file tree, rollback still restores the project file snapshot only. External side effects are out of scope and are not represented as rollback state.

## Invariants

- Conversation history is append-only.
- Code rollback changes only project files and app-data rollback metadata.
- Rollback intentionally overwrites later project file changes, including human or unattributed edits, because the selected snapshot is the source of truth.
- Every successful rollback has a durable audit record.
- Rollback works without a clean git working tree.
- Rollback does not require the project to have a remote.
- Rollback does not require commits, stashes, branches, or worktrees.
- Rollback has no conflict model. Restore can fail only for operational reasons such as missing snapshot data, filesystem errors, or an unavailable project root.
- Snapshot capture failure blocks mutating agent actions unless the action is explicitly classified as non-rollbackable and approved.
- Project state belongs in app data. Legacy repo-local state is not used.

## Current Context

Xero already has useful foundations:

- Owned-agent runs persist messages, events, tool calls, file-change rows, checkpoints, action requests, and usage records.
- Session branch and rewind lineage can point at prior runs, messages, and checkpoints.
- The runtime stream can surface tool activity and file-change activity in the conversation.
- Git status and diff commands already summarize the current repository state.
- The frontend already groups transcript, tool, activity, completion, and failure items into a conversation projection.

The missing piece is durable file content. Current file-change metadata records paths, operations, and hashes, but that is not enough to restore deleted files, binary files, or prior content after later edits. The rollback system must store restorable bytes and restore metadata, not just hashes.

## Terminology

Change group: a logical edit boundary caused by one mutating agent action or a tightly grouped batch of mutating actions.

Code snapshot: a manifest of whole-project file state at a boundary. A snapshot records path, file type, size, hash, permissions, symlink target, and blob references when content may be needed for restore.

File version: the before and after state of one path within a change group.

Restore target: the snapshot Xero wants the project files to match after rollback.

Rollback operation: an audit record for a user-initiated restore. It records the target snapshot, files changed by the restore, and the resulting snapshot.

Pre-rollback snapshot: the current project state captured immediately before Xero applies a rollback. This exists so the rollback can itself be rolled back. It is not used to protect, merge, or preserve later user edits during the selected restore.

## Data Model

Add app-data-backed persistence for code rollback.

Tables or equivalent records:

| Entity | Purpose |
| --- | --- |
| `code_snapshots` | Boundary-level file manifests for a project, session, run, and change group. |
| `code_change_groups` | User-visible edit groups attached to runs, tool calls, transcript boundaries, and runtime events. |
| `code_file_versions` | Per-path before and after state for each change group. |
| `code_blobs` | Content-addressed file bytes used for restore. |
| `code_rollback_operations` | Audit trail for applied rollbacks and failed restore attempts. |

Minimum fields for `code_change_groups`:

- Project id.
- Agent session id.
- Run id.
- Change group id.
- Parent change group id.
- Tool call id or runtime event id when available.
- Conversation sequence or transcript item anchor when available.
- Started and completed timestamps.
- Change kind: file tool, command, MCP, rollback, recovered mutation, or imported baseline.
- Summary label.
- Before snapshot id.
- After snapshot id.
- Restore state: snapshot_available, snapshot_missing, or external_effects_untracked.
- Status: open, completed, superseded, rolled back, failed.

Minimum fields for `code_file_versions`:

- Change group id.
- Path before.
- Path after.
- Operation: create, modify, delete, rename, mode change, symlink change.
- Before file kind.
- After file kind.
- Before hash.
- After hash.
- Before blob id when needed.
- After blob id when needed.
- Size and mode metadata.
- Explicitly edited flag.
- Generated or ignored classification.

Content blobs should be content-addressed by SHA-256 and stored once per project store. Blobs may be compressed. Reference counts or reachability pruning can be added after the first implementation, but correctness comes before cleanup.

## Capture Strategy

The runtime needs two capture paths because not all mutating tools know their paths up front.

Exact-path change capture:

- Used by write, patch, edit, delete, rename, mkdir, notebook edit, and project-file commands.
- Capture before state for the exact path set before execution.
- Capture after state for the exact path set after execution.
- Include parent directory entries only when needed for create/delete/rename restore.
- Store before bytes for existing files and after bytes for new or modified files.
- Also attach the exact-path change to the nearest whole-project boundary snapshots so rollback can restore the entire project, not just the touched paths.

Whole-project boundary capture:

- Used for shell commands, long-running command sessions, mutating MCP calls, code generators, package managers, rollback operations, and any tool boundary that can be selected for rollback.
- Capture a project manifest before execution.
- Capture another manifest after execution.
- Hash files whose size, mtime, mode, or type changed.
- Store blobs only for files that changed.
- Skip dependency/build/cache directories by default, but capture any path the agent explicitly edited even if ignored.
- Record ignored generated artifact changes as untracked external artifacts unless they are included in the project snapshot policy.

Read-only and verification commands:

- Run with a lightweight pre-manifest.
- If a supposedly read-only command changes project files, record a recovered mutation change group.
- Mark the run with a diagnostic so the tool policy can be tightened later.

Snapshot capture must happen inside the mutating tool runtime, not in the conversation UI. The backend is the authority because it can enforce sequencing, locking, filesystem metadata, and app-data writes.

## Rollback Semantics

The rollback action restores the whole project to the snapshot immediately before the selected code-changed file entry. This is a time-travel restore, not an inverse patch.

For example:

| Timeline | Code State |
| --- | --- |
| Baseline | A |
| Edit group 1 | B |
| Edit group 2 | C |
| Edit group 3 | D |

Clicking rollback on a file entry from edit group 2 restores the whole project to state B. It does not selectively undo only the files in edit group 2. It restores the exact project snapshot from before that edit group.

Later file changes are not analyzed as conflicts. They are overwritten by the selected snapshot. This includes user edits made after the snapshot. The pre-rollback current state may be captured so the rollback operation itself can be rolled back, but it does not change the selected restore behavior.

Rollback is not a reverse patch replay. It is a restore-to-snapshot operation. This avoids fragile patch application and avoids conflict handling because the selected snapshot is the complete desired project state.

## Restore Policy

Rollback applies the selected snapshot directly.

There is no conflict policy:

- Xero does not compare current files to decide whether user changes should be preserved.
- Xero does not merge current files with the selected snapshot.
- Xero does not show a preview or conflict resolution flow.
- Xero does not block because the current file state differs from the last agent-owned state.

Restore can fail only when the snapshot cannot be applied:

- Required snapshot or blob data is missing.
- File permissions prevent writes or deletes.
- The project root moved or no longer resolves.
- Another active owned-agent run currently owns the project mutation lock.
- The filesystem reports an unrecoverable write, rename, symlink, or delete error.

Operational failures should be recorded in rollback history and surfaced through existing error handling. They should not introduce new rollback-specific UI beyond the rollback button.

## Apply Algorithm

1. Resolve the project and acquire a project-level code-restore lock.
2. Stop or block new mutating agent actions for the project while the restore is planned and applied.
3. Load the target snapshot from immediately before the selected changed-file entry.
4. Capture a pre-rollback snapshot of the current project state so the rollback operation can be reversed later.
5. Build a whole-project restore plan from the selected snapshot.
6. Before applying, record a pending rollback operation.
7. Write changed content to temporary files in the same filesystem when possible.
8. Atomically replace files by rename where supported.
9. Remove files that should not exist in the target snapshot.
10. Restore permissions and symlink targets.
11. Re-scan affected paths.
12. Record the rollback as a completed change group with before and after snapshots.
13. Emit repository status, diff, session history, and runtime stream updates.
14. Release the restore lock.

If application fails midway, Xero should keep the pending rollback operation and the pre-rollback snapshot. The next startup can detect the incomplete operation, re-scan the project, and either complete the audit record or surface a repair diagnostic through existing diagnostics surfaces.

## Runtime Integration

Add a backend rollback service used by the owned-agent tool runtime.

Responsibilities:

- Start a change group before mutating tool execution.
- Capture before state.
- Complete the change group after tool execution.
- Detect recovered mutations from tools classified as read-only.
- Attach change group ids to runtime events.
- Persist snapshots before emitting completion events.
- Block mutating actions when capture fails.
- Mark non-restorable external effects in tool results.

Tool policy changes:

- Mutating file tools must be exact-path captured.
- Mutating command tools must be broad-action captured.
- Long-running command sessions get a change group per write-producing command boundary, plus a fallback group when the session closes and changed files are detected.
- MCP tools need action-level rollback metadata. Unknown mutating MCP calls can only record project-file snapshots and cannot represent external side effects.
- Verification commands can run with recovered-mutation detection.

The verification gate should continue to require evidence after file changes. A rollback counts as a file change, so continuing work after rollback must verify again when the selected agent requires verification.

## Frontend Integration

Add only the rollback button where the conversation already renders code-changed file entries.

User-facing surfaces:

- Code-changed file entries show a small rollback button.
- The button applies the associated snapshot restore.
- The button is disabled only when no restorable snapshot exists.
- The active repository/diff view refreshes after restore.

State hooks:

- Load rollback metadata with session history or runtime stream hydration.
- Add a mutation action for apply.
- Keep high-churn repository status updates out of transcript rendering where possible.
- Preserve selection and scroll position after rollback.

Accessibility:

- The rollback button must have an accessible label.
- The rollback button must be keyboard operable.
- The rollback button must expose disabled state when no snapshot is available.

## Session And Context Behavior

Rollback is a code event, not a transcript edit.

After rollback:

- The current session remains selected.
- Older conversation turns remain visible.
- Search and export include the rollback operation.
- The context panel includes the rollback event as recent runtime state.
- Continuing the agent run tells the model that code was restored to a prior snapshot and that later transcript content may describe code no longer present.
- Memory extraction should not treat rolled-back implementation details as durable project facts unless the rollback event is part of the provenance.

Branch and rewind behavior:

- Conversation branch and rewind remain conversation-lineage features.
- A rewound session can optionally start from the current code state or from a selected code snapshot, but that should be a later explicit feature.
- The first milestone should keep rollback in the active session only.

## Commands And Contracts

Add typed Tauri command contracts for:

- List change groups for a session or run.
- Get change group details.
- Apply rollback for a change group.
- Load rollback operation history.

Responses should include:

- Project id.
- Agent session id.
- Run id.
- Change group id.
- Target snapshot id.
- Restore state.
- Affected files.
- Operation id.
- Resulting repository status revision.

The TypeScript schemas and Rust DTOs must agree exactly. Contract tests should reject unknown fields and invalid boundary combinations.

## Implementation Phases

## Phase 0: Define The Contract

Goal: make rollback semantics explicit before writing storage code.

Work:

- Define change group, snapshot, file version, rollback operation, and restore failure DTOs.
- Add strict TypeScript schemas and Rust DTOs.
- Define the rollback button boundary as the snapshot immediately before the selected changed-file entry.
- Add test fixtures for simple modify, create, delete, rename, binary, symlink, permission, and destructive user-edit overwrite cases.
- Document which tool effect classes have project-file snapshot coverage and which have external side effects outside rollback scope.

Acceptance:

- Contract tests validate representative rollback payloads in both TypeScript and Rust.
- Invalid requests cannot mix project/session/run/change group identifiers from different lineages.
- Unknown restore-state values and file operation values fail closed.

## Phase 1: Build App-Data Snapshot Storage

Goal: store enough file bytes and metadata to restore agent changes without touching git history.

Work:

- Add app-data persistence for snapshots, change groups, file versions, blobs, and rollback operations.
- Implement content-addressed blob writes and reads.
- Implement exact-path before/after capture.
- Implement manifest scan for broad actions with lazy hashing.
- Preserve file mode, symlink, binary, and deletion state.
- Add startup validation for incomplete snapshot writes.

Acceptance:

- A scoped backend test can capture and restore modify, create, delete, rename, binary, and symlink cases.
- Snapshot data is written under app data.
- No git branch, stash, commit, or worktree is created.
- Missing blobs fail restore with a clear operational error instead of applying an incomplete restore.

## Phase 2: Attach Capture To Mutating Tools

Goal: make every owned-agent mutation produce a change group.

Work:

- Wrap file tools with exact-path capture.
- Wrap mutating command execution with broad-action capture.
- Detect recovered mutations from read-only or verification commands.
- Attach change group ids to runtime events and file-change records.
- Mark external side effects as non-restorable metadata.
- Block mutating execution when snapshot capture fails unless policy explicitly allows external-effect-only execution with user approval.

Acceptance:

- Each mutating tool call produces exactly one completed change group or one failed group with a diagnostic.
- A test command that edits an unexpected file is recorded as a recovered mutation.
- Verification gate behavior treats rollback and recovered mutations as file changes.
- Runtime stream events can point the conversation UI to the associated change group.

## Phase 3: Implement Snapshot Restore Apply

Goal: restore code safely to any prior change boundary.

Work:

- Implement restore planning from a selected snapshot.
- Implement pending and completed rollback operation audit records.
- Implement atomic write/delete/rename application where the platform supports it.
- Record rollback itself as a new change group.
- Refresh repository status and diff after apply.
- Add recovery handling for interrupted rollback operations.

Acceptance:

- Rolling back edit group 2 in a three-edit session restores the code to the state after edit group 1.
- Later conversation turns remain intact.
- Human edits made after the selected snapshot are overwritten by restore.
- The rollback operation can itself be rolled back.
- Failed rollback attempts leave an inspectable audit record and no misleading success state.

## Phase 4: Add Rollback Button

Goal: make rollback available exactly where users review changed files.

Work:

- Load change group summaries alongside session/runtime history.
- Render rollback buttons on code-changed file entries.
- Refresh repository status and diff views after successful rollback.
- Add component tests for enabled, disabled, click, success, and failure states.

Acceptance:

- A user can trigger rollback from a changed file entry in the conversation.
- No temporary debug/test UI is added.
- The conversation scroll position is preserved after rollback.
- Keyboard and screen-reader flows work for the button.

## Phase 5: Preserve Context Correctness After Rollback

Goal: make future agent turns understand current code state without rewriting history.

Work:

- Add rollback events to session export and search.
- Include recent rollback state in context visualization.
- Add provider-turn guidance that later transcript edits may have been reverted.
- Prevent memory extraction from promoting rolled-back implementation details without rollback provenance.
- Update branch/rewind copy to distinguish conversation rewind from code rollback.

Acceptance:

- Continuing after rollback sends current code state and rollback metadata to the provider.
- Exported transcripts show the rollback operation in chronological order.
- Search can find rollback events by file path, change group summary, and operation id.
- Context visualization explains that code state changed while conversation history remained.

## Phase 6: Harden And Prune

Goal: make the feature reliable enough for daily agent work.

Work:

- Add retention policy for old blobs with reachability checks.
- Add repair diagnostics for missing blobs, moved project roots, and incomplete operations.
- Add performance tests for broad-action scans on large repositories.
- Add platform-specific tests for permissions and symlinks where supported.
- Add telemetry-free local diagnostics for support triage.

Acceptance:

- Snapshot cleanup never deletes a blob reachable from an active snapshot or rollback operation.
- Startup diagnostics surface incomplete or corrupted rollback metadata.
- Scoped tests cover macOS-relevant filesystem behavior.

## First Milestone

Ship a narrow but real vertical slice:

- Exact-path capture for file write, patch, edit, delete, and rename.
- App-data blob storage.
- Rollback apply for fully captured text and binary files.
- Conversation rollback button on changed file entries.
- Destructive overwrite behavior for later user-edited files.
- Rollback event persisted in session history.

Do not include command-session broad capture in the first milestone unless exact-path rollback is already stable. Command capture is important, but exact-path tools deliver the core user-visible behavior with less risk.

## Testing Plan

Backend tests:

- Capture and restore a modified text file.
- Capture and restore a new file.
- Capture and restore a deleted file.
- Capture and restore a rename.
- Capture and restore a binary file.
- Capture symlink and permission metadata when the platform supports it.
- Overwrite later user edits when restoring an older snapshot.
- Record rollback as a new change group.
- Recover or diagnose an interrupted rollback operation.

Frontend tests:

- Render rollback button for a rollbackable changed file entry.
- Hide or disable rollback for changed file entries without a restorable snapshot.
- Preserve conversation history and scroll after success.

Integration tests:

- Run a fake owned-agent sequence with three edit groups.
- Roll back the second group.
- Assert code matches the post-first-edit state.
- Assert transcript rows for all three edit groups still exist.
- Assert repository status refreshes.
- Assert continuing the session includes rollback metadata.
- Assert a user edit made after the selected snapshot is overwritten by the restore.

Use scoped tests and scoped formatting for this work. Do not run browser-only end-to-end tests for the Tauri app.

## Risks

Broad command capture can be expensive. Use lazy hashing and exact path capture where possible.

Generated files can flood storage. Exclude dependency/build/cache directories by default, but always capture explicit agent edits.

External effects are not file rollback. Mark them clearly as non-restorable and require approval for risky actions.

Concurrent agents can race restore. A project-level restore lock and mutating-action gate are required before apply.

Transcript semantics can confuse users. The agent context must state that code was restored while history remains intact.

## Open Questions

- Should ignored but non-explicit generated files be restorable when they are small, or always summarized as non-restorable artifacts?
- How long should old snapshots be retained by default once no active session references them?
- Should branch/rewind later offer an optional code snapshot restore during session creation?

## Completion Criteria

The feature is complete when a user can run an owned-agent conversation with multiple edit groups, choose any prior edit group, restore the project files to that boundary, continue the same conversation, and later undo the rollback, all without git history mutations or transcript rewrites.
