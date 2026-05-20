# TUI Conversation File Attachments Plan

## Reader And Outcome

This plan is for an engineer implementing terminal-native file attachments in the Xero agent TUI.

After reading it, they should be able to add file attachments to TUI conversation sends while reusing the existing app-data attachment storage, transcript persistence, provider preflight, and owned-agent runtime contracts.

## Goal

Users can attach supported local files to the next conversation message in the TUI, see what is pending before sending, remove pending files, and send the prompt plus attachments through the same owned-agent runtime path used by the desktop-backed product.

The shipped behavior should feel terminal-native:

- Attach one or more files by path from the composer.
- Show pending attachments inside the composer area in a compact, non-debug surface.
- Remove individual attachments or clear all pending attachments before sending.
- Persist sent attachments with the user message in the conversation transcript.
- Fail closed when the selected provider cannot accept attachments.

## Non-Goals

- No graphical file picker, drag-and-drop, browser surface, or desktop-only window affordance.
- No temporary debug UI.
- No TUI-only attachment store.
- No legacy `.xero/` state.
- No backward-compatibility glue for incompatible state.
- No workaround that inlines arbitrary file contents into the prompt body instead of using attachment metadata and provider attachment support.

## Existing Ground Truth

The repo already has most of the backend shape needed for this feature:

- Attachment classification exists for image, PDF document, and text-like files, with a 20 MB per-file limit in both frontend and Rust staging paths. The frontend also defines a 50 MB total budget that the TUI should mirror.
- Desktop staging writes files under project OS app-data, scoped by project and run id.
- Runtime start and update DTOs already accept staged attachment DTOs.
- Owned-agent runtime requests already carry message attachments.
- User messages can already be persisted with attachment records.
- Provider replay reconstructs message attachments from persisted records.
- Anthropic-owned adapters already serialize image, document, and text attachments.
- OpenAI-style owned adapters currently reject attachments instead of silently dropping them.
- Provider preflight already models whether attachment support is required and supported.
- Remote/cloud attachment staging already sends a staged attachment payload into the desktop bridge.

The main gap is the TUI path. The composer currently tracks text only, and prompt submission shells through the CLI agent execution path with no attachment arguments. The reusable headless core request shape is also text-only, while the richer desktop runtime path already has attachment DTOs.

## User Experience

Add three composer commands:

- `/attach <path> [path...]`
  Stages one or more files for the next message. Relative paths resolve from the active project root. Absolute paths and `~` expansion are accepted. Missing project selection returns a footer error.
- `/detach <index|name|all>`
  Removes one pending attachment or clears the pending set. Removing a staged attachment discards the copied app-data file.
- `/attachments`
  Shows a palette/detail view of pending attachments with index, name, kind, size, and staging status.

Render pending attachments in the composer surface above the agent footer. Keep it compact:

```text
attachments: [1] screenshot.png 42 KB  [2] notes.md 3 KB
```

Long file names should truncate from the middle or end without resizing the composer unpredictably. If there are too many files for one row, summarize the overflow:

```text
attachments: screenshot.png 42 KB, notes.md 3 KB, +3 more
```

Sending rules:

- `Enter` sends the current prompt plus all ready pending attachments.
- A text prompt is still required for v1. Attachment-only sends can be added later if the desktop and cloud composers also accept them.
- If any attachment is still staging or has an error, the prompt does not send and the pending list remains intact.
- If provider preflight rejects attachments, the prompt does not send and the pending list remains intact.
- After a successful run start or continuation queue, pending attachments clear.
- `/new`, session switch, project switch, and quit should discard pending staged attachments.

## Architecture

Use the desktop-backed TUI adapter for the production path. This keeps attachments on the same backend as desktop and cloud instead of extending a parallel text-only headless path first.

Implementation pieces:

1. Extract attachment staging into a reusable Rust helper.
   The current Tauri command should become a thin wrapper over a pure helper that accepts project root, project id, run id, source bytes, original name, and media type. The same helper should power desktop Tauri commands, remote bridge staging, and the TUI adapter.

2. Add desktop-backed TUI adapter commands.
   Add adapter support for:

   ```text
   attachment stage --project-id <id> --run-id <id> --path <path>
   attachment discard --project-id <id> --absolute-path <path>
   ```

   The stage command should classify the path, enforce per-file and total limits, copy into project OS app-data, and return the existing staged attachment DTO shape.

3. Add an attachment-capable TUI run submission path.
   The TUI should not rely on the text-only CLI core when attachments are present. Add a desktop-backed adapter route for TUI prompt submission that parses the existing agent execution arguments plus a serialized attachment list, then builds the desktop owned-agent run or continuation request with message attachments.

   Recommended v1 contract:

   ```text
   agent exec ... --attachments-json <json-array-of-staged-attachment-dtos>
   ```

   In desktop-backed `xero-tui`, the adapter intercepts this and calls the desktop owned-agent runtime. In a pure CLI-core TUI build, the same flag should fail with a clear "attachments require the desktop-backed TUI runtime" error instead of silently ignoring files.

4. Add pending attachment state to the TUI app model.
   Track:

   - A draft run id allocated when the first attachment is staged.
   - Pending staged attachments.
   - A stable display index.
   - Source path for display and diagnostics.
   - Aggregate byte count.

   Use the draft run id as the run id when the prompt is finally submitted. If a prompt is sent without attachments, keep the existing run id generation.

5. Update composer and transcript rendering.
   Composer rendering should show the compact pending attachment row. Transcript message rows should gain attachment summaries so sent user messages display their attached files after reload, resize replay, and `conversation show` polling.

6. Keep provider capability checks centralized.
   Do not duplicate provider-specific logic in the TUI. The TUI should submit staged attachment DTOs and rely on provider preflight/runtime errors to reject unsupported providers. The TUI only formats those errors clearly and keeps pending attachments available for retry.

## Implementation Phases

### Phase 1: Shared Staging Contract

- Extract Rust attachment classification, extension fallback, app-data destination, image dimension probing, and discard validation into a shared helper.
- Mirror frontend total-size behavior in Rust so TUI and remote uploads enforce the same limits.
- Add unit tests for:
  - supported image, PDF, and text-like extensions
  - empty file rejection
  - per-file limit rejection
  - total pending limit rejection
  - discard path containment
  - app-data destination, not repo-local state

### Phase 2: Desktop-Backed TUI Adapter

- Add `attachment stage` and `attachment discard` to the desktop TUI adapter.
- Add adapter tests with isolated app-data directories and temporary project files.
- Ensure staging works without a Tauri window.
- Ensure missing project state produces a user-fixable error.

### Phase 3: TUI Composer State And Commands

- Add pending attachment fields to the TUI app state.
- Add slash command parsing for `/attach`, `/detach`, and `/attachments`.
- Add path parsing with quotes and whitespace, reusing the existing slash parser behavior where possible.
- Allocate a draft run id on first attachment and use it for all staged files in that pending set.
- Discard pending files on detach, clear, session switch, project switch, new session, and quit.
- Block prompt send if any pending attachment is not ready.

### Phase 4: Attachment-Aware Prompt Submission

- Extend TUI prompt submission to include staged attachment DTOs.
- Add an adapter-backed submission branch when attachments are present.
- Preserve existing no-attachment prompt behavior unless the adapter path is intentionally promoted to the default TUI run path.
- Keep pending attachments after any failed submission.
- Clear pending attachments only after the backend accepts the run or continuation.

### Phase 5: Transcript And Recovery Surfaces

- Extend runtime message rows with attachment summaries.
- Parse attachment DTOs from conversation snapshots.
- Render sent attachments under user messages in scrollback and resize replay.
- Include attachments in conversation dump/export output if the existing export DTO does not already surface them.
- Add recovery checks so rewind/branch preserves attachment metadata with the copied message prefix.

### Phase 6: Provider And Error UX

- Surface provider preflight failures in the footer and keep attachments pending.
- Make unsupported-provider messages actionable: choose an attachment-capable provider or detach files.
- Keep OpenAI-style adapter failure behavior closed until those adapters serialize attachments.
- Verify Anthropic image, PDF, and text attachments flow end to end through existing provider blocks.

## Verification

Run focused checks only.

Rust/unit:

```bash
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-desktop --lib stage_agent_attachment
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-cli --lib tui
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-agent-core provider_preflight
```

TUI smoke:

```bash
pnpm run dev:tui -- --smoke
pnpm run dev:tui -- --smoke-run
```

Manual terminal checks:

- Attach a small text file, send a prompt, confirm the user message shows the attachment summary.
- Attach an image, send with an attachment-capable provider, confirm provider receives image content.
- Attach a PDF, send with an attachment-capable provider, confirm provider receives document content.
- Try an unsupported media type and confirm it is rejected before send.
- Try a supported file over 20 MB and confirm it is rejected before send.
- Try attachments totaling over 50 MB and confirm the pending set rejects the last file.
- Try sending attachments with a provider that cannot accept them and confirm the prompt is not sent and attachments remain pending.
- Switch sessions with pending attachments and confirm staged app-data copies are discarded.

## Risks And Decisions

- The biggest implementation decision is whether to route all TUI prompt submissions through the desktop-owned runtime adapter or only route attachment-bearing sends through it. Attachment-bearing sends must use the desktop-backed path in v1 because that is where staged attachment DTOs, message persistence, and provider serialization already exist.
- Draft run ids can leave orphan app-data files if the TUI process is killed. This is acceptable for v1 if normal detach/session/quit paths discard correctly, and project-state cleanup continues to support attachment cleanup.
- Text attachment handling reads files into provider content blocks for supported adapters. Keep size limits strict and avoid adding separate prompt inlining in the TUI.
- Attachment-only messages are useful, but v1 should require a text prompt unless the desktop and cloud composers are intentionally updated to allow empty text with ready attachments.
- The term "attachments" here means files attached to a conversation message. Do not mix this with attached skills on agent definitions.
