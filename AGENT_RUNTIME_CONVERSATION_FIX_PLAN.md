# Agent Runtime Conversation Fix Plan

## Reader And Goal

This plan is for the engineer fixing the Agent tab conversation experience. After reading it, they should be able to implement the fixes for broken streaming text, generic tool activity cards, top-biased message placement, and unstable scrolling without adding temporary debug UI.

## Problem Statement

The screenshots from May 2, 2026 show four related user-facing failures:

1. The user's first prompt appears near the top of an otherwise empty conversation instead of feeling anchored to the composer/latest message area.
2. The agent rapidly emits many tool cards, but each card only shows state plus the tool name and usually says `Tool activity recorded.` It does not show which path/pattern/command was inspected or what the result was.
3. While the assistant response streams, the visible area appears to chase only a couple of text rows at a time.
4. The assistant response formatting is corrupted. Examples visible in the screenshots include split words such as `mon om orph ization`, split command/code text such as `` `mesh c build` ``, and broken markdown such as `Native binary **`.

These failures have separate causes but compound into one broken experience.

## Root Cause Summary

The highest-confidence root cause is in the client-side transcript joiner. Provider deltas are already emitted and persisted as exact string deltas, but the Agent UI reconstructs adjacent transcript items by inserting spaces between chunks. That mutates streamed text and breaks words, markdown markers, and inline code.

The generic tool card problem is a runtime projection/rendering gap. Owned-agent tool events contain useful `input`, `summary`, and `output` payloads, but the Tauri stream projection does not place those values into the fields the React tool card reads.

The top placement and rapid scroll behavior come from layout and scroll-state gaps. The Agent pane has an overflow container, but no scroll anchor, no "stick to bottom unless the user scrolled away" behavior, and no bottom-aligned empty/small conversation layout.

## Evidence And Proof

### 1. Streamed text is corrupted by the React joiner

`client/components/xero/agent-runtime.tsx` defines `appendTranscriptDelta`:

```ts
if (/\s$/.test(current) || /^\s/.test(delta) || /^[.,!?;:%)\]}]/.test(delta)) {
  return `${current}${delta}`
}

return `${current} ${delta}`
```

This runs inside `buildConversationTurns` when adjacent stream transcript items have the same role. It assumes deltas are word tokens. Provider deltas are not word tokens; they can split anywhere inside a word or markdown marker.

Backend/provider evidence:

- `client/src-tauri/src/runtime/agent_core/provider_adapters.rs` appends OpenAI chat deltas with `message.push_str(&content)` and emits the exact `content`.
- The Responses parser also uses `message.push_str(&delta)` and emits the exact `delta`.
- `client/src-tauri/src/runtime/agent_core/provider_loop.rs` persists provider deltas as `AgentRunEventKind::MessageDelta` with `{ "role": "assistant", "text": text }`.
- `client/src-tauri/src/commands/subscribe_runtime_stream.rs` projects those event payloads directly into runtime stream transcript item `text`.

Local reproduction of the current joiner:

```text
["**Native binary","** Main modules"] => "**Native binary ** Main modules"
["mon","om","orph","ization"] => "mon om orph ization"
["`mesh","c","build`"] => "`mesh c build`"
["crate","-by","-crate"] => "crate -by -crate"
```

That matches the visible screenshot artifacts closely enough to treat this as confirmed.

### 2. Existing tests encode an incomplete streaming model

`client/components/xero/agent-runtime.test.tsx` has a test that expects chunks `Hi`, `!`, `What`, `can`, `I` to render as `Hi! What can I`. That test covers word-like chunks, not arbitrary provider deltas. It does not cover subword chunks, markdown delimiters split across chunks, or inline code split across chunks.

### 3. Tool cards discard the useful owned-agent event payloads

`client/src-tauri/src/runtime/agent_core/tool_dispatch.rs` records useful tool event payloads:

- `ToolStarted` includes `toolCallId`, `toolName`, and redacted `input`.
- `ToolCompleted` includes `toolCallId`, `toolName`, `ok`, `summary`, and full structured `output`.

`client/src-tauri/src/commands/subscribe_runtime_stream.rs` projects tool events, but for owned-agent tools it only sets:

- `tool_call_id`
- `tool_name`
- `tool_state`
- `text` for started/completed summaries

It does not set `detail`, and it does not set `tool_summary`.

The React side ignores `text` for tool cards:

- `normalizeRuntimeStreamItem` in `client/src/lib/xero-model/runtime-stream.ts` maps tool items to `detail: normalizeOptionalText(event.item.detail)` and `toolSummary`.
- `actionTurnFromItem` in `client/components/xero/agent-runtime.tsx` renders `item.detail ?? summary ?? 'Tool activity recorded.'`.
- `getToolSummaryContext` currently only formats `mcp_capability` and `browser_computer_use` summaries, even though the shared schema has `command`, `file`, `git`, and `web` summary variants.

Therefore an owned-agent `read`, `find`, or `list` can contain useful data in the backend event and still render as only `RUNNING list` / `SUCCEEDED list` / `Tool activity recorded.`

### 4. Activity events are hidden from the main conversation

The frontend asks for runtime stream kinds including `activity`, but `buildConversationTurns` only admits `transcript`, `tool`, and `failure`. Tool argument deltas, command output summaries, file-change summaries, validation, plan updates, and policy decisions are filtered out of the visible conversation.

Some activity is too noisy for the main chat, so filtering is reasonable. The problem is that there is no replacement disclosure that lets the user inspect the tool call's input/result at the card level.

### 5. The conversation is top-biased and has no scroll owner

In `client/components/xero/agent-runtime.tsx`, the message viewport is:

```tsx
<div className="flex-1 overflow-y-auto scrollbar-thin px-4 pt-14 pb-4">
  <div className="mx-auto flex max-w-4xl flex-col gap-4">
    <ConversationSection ... />
  </div>
</div>
```

There is no scroll container ref, no bottom sentinel, no `scrollIntoView`, no near-bottom tracking, and no jump-to-latest affordance. With a short conversation, content naturally starts at the top. With streaming content and many tool cards, browser scroll anchoring/layout updates can make the viewport feel like it is rapidly chasing the active rows.

### 6. Feed caps add churn

The client keeps only recent stream items:

- `MAX_RUNTIME_STREAM_ITEMS = 40` in `client/src/lib/xero-model/runtime-stream.ts`
- `MAX_VISIBLE_RUNTIME_FEED_ITEMS = 24` in `client/components/xero/agent-runtime.tsx`

This is fine for memory, but combined with one card per tool state transition it makes long tool bursts feel like a constantly rotating list. It also risks dropping the user's initial prompt from the visible feed during high-volume activity.

## Fix Plan

### Phase 1: Preserve transcript bytes exactly

Replace the spacing joiner for streamed assistant transcript chunks with exact concatenation.

Recommended behavior:

- Assistant transcript deltas from the same run should concatenate as `current + delta`.
- User transcript items should normally render as distinct user messages, because they are submitted as full prompts rather than provider-style deltas.
- If a future stream contract needs explicit message grouping, add a `messageId`/`turnId` to the stream item instead of guessing from role adjacency.

Implementation targets:

- `client/components/xero/agent-runtime.tsx`
- `client/components/xero/agent-runtime.test.tsx`

Tests to add/update:

- Chunks `["mon", "om", "orph", "ization"]` render as `monomorphization`.
- Chunks `["**Native binary", "** Main modules"]` preserve valid bold marker placement.
- Chunks ``["`mesh", "c", " build`"]`` render without inserted spaces.
- Consecutive full user prompts do not silently merge into one bubble.

### Phase 2: Project owned-agent tool details into the runtime stream

Populate the fields the UI already reads.

Recommended projection changes in `client/src-tauri/src/commands/subscribe_runtime_stream.rs`:

- For `ToolStarted`, set `detail` from the redacted `input` in a concise form. Examples: `path: client/components/xero/agent-runtime.tsx`, `pattern: appendTranscriptDelta`, `cwd: /repo, cmd: rg ...`.
- For `ToolCompleted`, set `detail` from `summary` or `message`.
- For successful tool completions, derive `tool_summary` from structured `output` when possible using the existing `ToolResultSummaryDto` variants: `command`, `file`, `git`, `web`, `browser_computer_use`, `mcp_capability`.
- For failures, set `detail` to the diagnostic message and keep `code`/`message`.

Tests to add/update:

- Rust unit tests in `subscribe_runtime_stream.rs` proving `ToolStarted` carries a redacted useful detail.
- Rust unit tests proving `ToolCompleted` maps `summary` into `detail`.
- Rust unit tests proving at least file/read, search/find, command, and git summaries map into `tool_summary`.

### Phase 3: Render useful, compact tool cards

Update the React tool card layer so the user can understand what happened without opening devtools.

Implementation targets:

- `client/components/xero/agent-runtime/conversation-section.tsx`
- `client/components/xero/agent-runtime/runtime-stream-helpers.ts`
- `client/components/xero/agent-runtime.test.tsx`
- `client/components/xero/agent-runtime/helpers.test.ts`

Recommended UI:

- Collapse duplicate started/completed events for the same `toolCallId` into one card that updates state.
- Show primary label as action plus target, not only tool name. Examples: `read agent-runtime.tsx`, `find appendTranscriptDelta`, `list client/components/xero`.
- Show one-line outcome detail by default.
- Add a ShadCN `Collapsible` or `Accordion` section for sanitized details/output summaries. This is user-facing inspection UI, not temporary debug UI.
- Continue to avoid dumping raw full stdout or raw file contents into the main feed.

Helper coverage to add:

- `getToolSummaryContext` should format `command`, `file`, `git`, and `web` summary variants, not only MCP/browser summaries.
- Long paths and commands should truncate visually but remain available through title/tooltip/copy affordances if already present in local UI patterns.

### Phase 4: Add a real conversation scroll model

Give the Agent pane explicit ownership of scroll behavior.

Implementation target:

- `client/components/xero/agent-runtime.tsx`

Recommended behavior:

- Keep a ref to the scroll viewport and a bottom sentinel.
- Track whether the user is near the bottom.
- On new user submission, scroll to bottom.
- While streaming and still near bottom, keep the bottom sentinel visible with `behavior: 'auto'` or a throttled non-animated scroll.
- If the user scrolls upward, stop auto-following and show a small user-facing "jump to latest" button.
- For short conversations, use a `min-h-full` inner layout with `justify-end` so the latest prompt starts near the composer instead of pinned under the breadcrumb.
- Add enough bottom padding so the composer gradient never visually occludes the final rows.

Tests:

- Use unit tests around the near-bottom helper math if extracted.
- Add a component test that dispatches scroll events and asserts auto-follow is disabled when the user scrolls away.
- Use Tauri/e2e visual verification if the repo has a Tauri harness. Do not open the app in a normal browser.

### Phase 5: Keep or replace the markdown renderer based on evidence

The immediate markdown corruption should disappear after exact delta concatenation. After Phase 1, retest with streamed markdown:

- headings
- bullets
- inline code
- bold text
- fenced code

If failures remain, replace the custom renderer in `client/components/xero/agent-runtime/conversation-markdown.tsx` with a small CommonMark-compatible renderer. If keeping the custom renderer, add tests for split markdown tokens so this regression cannot return.

### Phase 6: Reduce feed churn

After tool details render correctly, reduce visual churn:

- Dedupe tool state transitions by `toolCallId` before building visible turns.
- Keep transcript turns independent from the activity cap so a burst of tool calls cannot evict the user's prompt or the active assistant reply.
- Consider a compact "N tool calls" grouped activity section when many tools run in sequence.

## Verification Plan

Run scoped checks only.

Frontend:

```bash
pnpm --dir client test --run client/components/xero/agent-runtime.test.tsx client/components/xero/agent-runtime/helpers.test.ts
pnpm --dir client test --run client/src/lib/xero-model/runtime-stream.test.ts
```

Rust:

```bash
cd client/src-tauri
cargo test subscribe_runtime_stream
```

Only run one Cargo command at a time.

Manual/Tauri verification:

1. Start a real Agent session in the Tauri app.
2. Ask for a codebase walkthrough.
3. Confirm streamed markdown no longer splits words or code spans.
4. Confirm tool cards show paths/patterns/results.
5. Confirm the latest content remains readable while streaming.
6. Scroll upward during streaming and confirm the app does not force-scroll until the user jumps to latest.

## Risks And Notes

- The current test that expects `Hi`, `!`, `What`, `can`, `I` to render as `Hi! What can I` will need to change; it encodes an unrealistic provider-delta model.
- Exact concatenation may reveal that some non-provider transcript events need explicit separators. If that happens, fix the stream contract with message grouping metadata rather than reintroducing heuristic spacing.
- Tool result summaries must remain sanitized. Do not put raw file contents, secrets, or giant stdout into always-visible cards.
- The runtime already has summary DTOs for command/file/git/web. Prefer using those existing contracts before adding new UI-only shapes.
- This is a new application, so prefer correcting the contract and tests over maintaining compatibility with the broken spacing behavior.
