# Session Memory And Context

Xero keeps session history durable while giving users control over what becomes model-visible context. This guide is for users and support engineers who need to find prior work, export a transcript, compact a long session, review memory, or recover from an earlier point without editing raw history.

## What Xero Preserves

Owned-agent runs keep their raw transcript rows, tool summaries, file-change records, checkpoints, action requests, usage records, compaction records, memory records, and branch lineage. User-facing projections are redacted views over that durable history. Compaction, branch, rewind, and memory review do not delete the original transcript.

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

## Memory Review

Memory candidates are suggestions, not instructions. Xero may propose candidates from completed runs, file changes, user preferences, project decisions, and durable troubleshooting facts. Candidates are not model-visible until a user approves them.

The Memory surface lets users approve, reject, enable, disable, delete, filter, and inspect provenance. Approved and enabled memory is injected deterministically into the owned-agent system prompt. Candidate, rejected, disabled, secret-bearing, or instruction-override-shaped memory is not injected.

Approved memory is treated as durable context, not higher-priority policy. The system prompt explicitly tells the agent to ignore memory text that tries to change system or tool rules.

## Instruction Files

Xero includes supported project instruction files in the system prompt and shows them as context contributors. Missing or malformed instruction content should produce diagnostics or empty contributors rather than failing the provider call. Instruction text is counted separately from the system prompt when context visualization can identify it.

## Branch And Rewind

Branch creates a new active session from a historical run. The source session is not changed, and continuing the branch does not append to the original session.

Rewind is a branch from a more precise boundary, such as a message or checkpoint. It replays only the selected prefix plus any relevant compaction context. Rewind does not roll files back on disk. The lineage explains the source boundary and any file-change or checkpoint metadata available before that point.

Use branch when you want to explore from the end of a prior run. Use rewind when you want to recover from a specific earlier message or checkpoint without deleting the later transcript.

## Privacy Guarantees

Session projections, exports, search snippets, context visualization, compaction summaries, memory candidates, approved memory, and branch/rewind metadata are redacted before they are copied, exported, or shown as model-visible context.

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

For memory issues, inspect candidates and approved memory. Confirm that the item is approved, enabled, in scope, and not redacted for privacy or integrity.

For recovery issues, inspect lineage on the branched session. Branch and rewind preserve the source transcript; they do not imply file rollback.
