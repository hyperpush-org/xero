# Session Memory And Context

Xero keeps session history durable while giving users control over what becomes model-visible context. This guide is for users and support engineers who need to find prior work, export a transcript, compact a long session, recover a conversation branch, undo selected code changes, or return one agent session to an earlier code boundary without editing raw history. Memory-review backend records exist, but the permanent user-facing memory review surface is still deferred.

## What Xero Preserves

Owned-agent runs keep their raw transcript rows, tool summaries, file-change records, code undo events, session rollback events, checkpoints, action requests, usage records, compaction records, memory records, and branch lineage. User-facing projections are redacted views over that durable history. Compaction, branch, rewind, code undo, and session rollback do not delete the original transcript.

## Search And Export

Use session search when you remember a prompt, assistant response, tool summary, changed file label, checkpoint, or session title. Search can include archived sessions and can be scoped to one session or run. Results show safe snippets and can reopen the matching session or run.

Use export when you need to share or inspect a selected run or a whole session outside Xero. Markdown export is readable for humans. JSON export is structured for support and future automation. Both include run boundaries, prompts, assistant responses, tool summaries, checkpoints, file changes, action requests, usage totals, and redaction metadata.

Exports intentionally keep enough metadata to debug a session: project id, session id, run id, provider id, model id, timestamps, status, and token totals. They do not include raw secret-bearing values.

## Context Visualization

The Context panel explains what Xero expects to send on the next owned-agent provider turn. Contributors can include:

- The active system prompt.
- Project instructions from supported instruction files.
- Reviewed and enabled memory.
- An active compaction summary.
- The current conversation tail.
- Tool results that remain in replay.
- Tool descriptors selected for the run.
- Recent code undo or session rollback events that affect the run's workspace assumptions.
- Provider usage totals, shown for visibility but not sent as model-visible context.

Budget pressure is an estimate unless the provider returned usage data. Known provider budgets are classified as low, medium, high, or over budget. Unknown budgets remain non-blocking and are shown separately so users can decide whether to compact or continue.

## Manual Compact

Manual compact is useful when a session is long, tool-heavy, or near the provider context budget. It asks the active provider to summarize older history and preserves a recent raw tail for replay. Raw transcript rows remain searchable and exportable.

Manual compact is safe to run when:

- The current provider/profile can summarize the selected session.
- There is enough prior conversation to reduce replay context.
- Pending action requests are still pending and should not be summarized as completed work.

After compacting, the Context panel shows the active compaction summary and the covered range. Continuing the session uses the summary plus the raw tail instead of replaying all covered messages.

## Auto-Compact

Auto-compact is opt-in. When enabled, Xero checks context pressure before continuing an owned-agent run. If the configured threshold is crossed and the active provider can summarize, Xero runs the same compaction pipeline used by manual compact before the next provider turn.

Auto-compact never runs when disabled, when the provider cannot summarize, when the session is below threshold, or when the provider budget is unknown. If auto-compact fails, Xero preserves the raw transcript and reports a diagnostic rather than silently mutating replay state.

## Memory Review Status

Memory candidates are suggestions, not instructions. Xero may propose candidates from completed runs, file changes, user preferences, project decisions, and durable troubleshooting facts. Candidates are not model-visible until a user approves them.

The durable backend tracks candidate, approved, rejected, disabled, secret-bearing, and instruction-override-shaped memory states. The permanent Memory surface for approving, rejecting, editing, enabling, disabling, deleting, filtering, and inspecting provenance is planned but not currently shipped. Until that surface exists, docs and support flows must not describe memory review as a normal visible workflow.

Approved memory is treated as durable context, not higher-priority policy. The system prompt explicitly tells the agent to ignore memory text that tries to change system or tool rules.

When code has been undone or a session has been rolled back, Xero treats implementation facts from the removed code as historical unless the undo provenance is included. This keeps memory review from teaching future agents that a reverted implementation is still the current project truth.

## Instruction Files

Xero includes supported project instruction files in the system prompt and shows them as context contributors. Missing or malformed instruction content should produce diagnostics or empty contributors rather than failing the provider call. Instruction text is counted separately from the system prompt when context visualization can identify it.

## Branch And Rewind

Branch creates a new active session from a historical run. The source session is not changed, and continuing the branch does not append to the original session.

Rewind is a branch from a more precise boundary, such as a message or checkpoint. It replays only the selected prefix plus any relevant compaction context. Rewind does not roll files back on disk. The lineage explains the source boundary and any file-change or checkpoint metadata available before that point.

Use branch when you want to explore from the end of a prior run. Use rewind when you want to recover from a specific earlier message or checkpoint without deleting the later transcript.

## Code Undo And Session Rollback

Conversation rewind, selective code undo, and session rollback are separate recovery tools:

- Conversation rewind creates a new conversation branch from an earlier message or checkpoint. It changes replay context for the new branch, but it does not edit files on disk.
- Selective code undo removes a chosen code change, file change, or selected diff hunk from the current workspace. It is applied on top of the current files, so unrelated user edits and sibling-agent work are preserved when they do not overlap.
- Session rollback returns one agent session or run lineage to an earlier code boundary by undoing that session's later code change groups. It does not rewind other sessions or remove independent current work.

Code undo and session rollback are recorded as new append-only history events with affected paths, operation mode, target, result commit, and conflict details when applicable. Search, export, context visualization, and session history should show those events chronologically instead of rewriting older transcript rows.

If Xero cannot preserve unrelated current work safely, the operation conflicts before writing files. Conflict results should identify the affected paths and concise reasons. A failed undo or session rollback leaves the workspace unchanged.

Undo only covers project file state and Xero's app-data history metadata. External side effects are out of scope, including remote services, databases outside the project store, emulator state, Docker volumes, package manager caches, deployed programs, transactions, and commands that already affected systems outside the workspace.

## Privacy Guarantees

Session projections, exports, search snippets, context visualization, compaction summaries, memory candidates, approved memory, branch/rewind metadata, and code undo/session rollback metadata are redacted before they are copied, exported, or shown as model-visible context.

Xero redacts:

- API keys, OAuth tokens, bearer headers, session ids, cloud access keys, GitHub tokens, Slack tokens, and private-key markers.
- Secret-bearing assignments and nested JSON fields such as token, API key, authorization, and cloud credential names.
- Endpoint credentials in URLs, including usernames, passwords, and sensitive query parameters.
- Local credential paths, keychain paths, environment files, and common cloud credential locations.
- Raw payload markers and terminal control bytes.
- Memory text that looks like an attempt to override system, developer, or tool instructions.

Redaction favors safety over perfect partial preservation. When a value looks unsafe, Xero may replace the whole text field with a redaction marker while preserving ids, timestamps, source kind, status, and remediation metadata.

## Support Triage

For a long-session issue, start with the Context panel. Check whether the session is over budget, whether a compaction summary is active, whether approved memory is injected, and whether instruction files are present.

For a missing-history issue, use session search with archived sessions enabled, then export the selected run as JSON if support needs structured evidence.

For memory issues, inspect candidates and approved memory through backend/support diagnostics. Confirm that the item is approved, enabled, in scope, and not redacted for privacy or integrity. Do not assume a permanent in-app memory review surface exists until the deferred UI work is implemented.

For conversation recovery issues, inspect lineage on the branched session. Branch and rewind preserve the source transcript; they do not imply file rollback.

For code recovery issues, inspect the undo or session rollback history event. Confirm whether the user selected a file, hunk, change group, or session boundary; check affected paths; and verify whether the result was completed or conflicted. Code undo should preserve unrelated current work. Session rollback should target only the selected session lineage. Neither operation can reverse external side effects outside the project file state.
