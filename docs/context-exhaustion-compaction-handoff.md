# Context Exhaustion, Compaction, And Handoff Strategy

## Reader And Action

This document is for Xero engineers changing owned-agent continuity behavior. After reading it, an engineer should be able to implement or review a compaction and handoff model-routing change without weakening current continuity, privacy, or cost controls.

## Recommendation

Add an opt-in continuity model routing policy, then make it the default only after controlled tests prove it is better than active-provider compaction. The recommended shape is a routing policy that can use a fast, large-context model for compaction and handoff summaries, with an explicit user override and an active-provider fallback.

Do not add a hidden provider that silently bypasses the user's active profile. Compaction sees raw conversation history, tool evidence, local paths, and sometimes redacted or partially redacted project context. Users need to know which provider receives that payload, what it costs, and how fallback works.

## Current Behavior

Owned-agent continuation performs continuity checks before the user's next message is appended.

1. The continuation prompt is validated. If the run is still starting or the lifecycle has queued environment messages, Xero queues the user message instead of driving a provider turn immediately.
2. Xero creates a provider adapter from the current provider profile and model.
3. If the request carries an enabled auto-compact preference and no active compaction exists for the session, Xero estimates the next turn size. When the configured threshold is reached, it asks the active run provider to summarize covered history and stores one active compaction record for the session.
4. Xero reloads the run and evaluates handoff policy using project or session context-policy settings. Defaults are auto-compact enabled, auto-handoff enabled, compact at 75 percent pressure, handoff at 90 percent pressure, and keep 8 raw-tail messages.
5. If the handoff policy blocks because pressure requires handoff but auto-handoff is disabled, continuation fails with a user-visible error.
6. If the handoff policy decides to hand off, Xero creates or reuses a same-type target run, seeds it with a durable handoff bundle, marks the source run handed off, and returns the target run for driving.
7. If no handoff occurs, Xero performs a hard budget check. Known over-budget continuations fail with a user-visible context-budget error. Unknown-budget continuations are allowed.
8. Pending operator actions can then be answered, interrupted tool calls are marked, the user's continuation message is appended, and the run resumes.

The continuation path has two policy layers. The auto-compact request preference can run compaction before handoff. The context-policy evaluator can also return compact or recompact decisions, but the current continuation flow only acts on blocked and handoff decisions from that evaluator. That means recompact is represented in policy and manifests but is not executed by the continuation preflight today.

## Compaction Path

Compaction builds a transcript source from non-system messages across the selected session or run scope. It excludes a raw tail by counting recent messages, then asks the active provider/model for a concise factual summary. The stored record includes coverage ranges, covered run ids, input token estimate, summary token estimate, raw-tail count, policy reason, trigger kind, provider id, model id, and a source hash. Inserting a compaction supersedes any previous active compaction for the same session.

Replay only uses an active compaction when it covers the current run and was created by the same provider/model. Before replay, Xero recomputes the covered-source hash. If the covered transcript changed, replay fails and asks the user to refresh context and compact again. If replay succeeds, the provider receives a synthetic user message containing the compaction summary, then raw uncovered messages are replayed normally. Tool outputs are repaired when possible so provider state remains coherent.

Important edge cases:

- Known budget: pressure is computed from estimated next-turn tokens and the model's effective input budget, which reserves output tokens and a safety margin.
- Unknown budget: auto-compact is skipped, handoff policy continues, and hard blocking is skipped because no limit is known.
- Active compaction present: request-level auto-compaction does nothing. Replay later validates provider/model and source hash.
- Active compaction stale: source-hash validation fails during provider-message reconstruction. The broader policy may say "continue" because it currently treats any active compaction as current.
- Compaction provider failure: empty summaries, secret-bearing summaries, tool-call requests, transport failures, and provider rejections surface as retryable or user-fixable compaction errors. The continuation is not appended before those errors.
- Provider mismatch: manual/auto compaction and replay both require the current provider/model to match the run or compaction record.
- Provider capability: current policy inputs assume provider compaction support instead of deriving it from model capabilities.

## Handoff Path

Handoff builds a same-type target run rather than changing agent type. The handoff bundle includes the original goal, pending continuation prompt, source status, recent assistant summaries, active todo items, event-derived decisions/risks/questions, recent file changes, tool evidence, verification events, source-cited continuity records, approved memory and project-record retrieval results, a bounded raw-tail preview, active compaction metadata, and runtime-agent-specific fields.

The bundle is stored in handoff lineage and as a high-importance project record. The target run is seeded with a developer message containing the redacted bundle and then receives the pending user prompt. The source run is marked handed off, and memory-candidate extraction is attempted for the source run.

If context pressure reaches the handoff threshold and auto-handoff is disabled, continuation is blocked with a user-visible handoff error before the prompt is appended. Handoff also requires the current provider/model to match the source run provider/model, because the target run is created with that same provider identity.

Run continuation after handoff works through the target run returned by the handoff preparation result. A direct continuation against the old source run is not explicitly rejected by status alone today; it can re-enter the normal continuation gates. The source-context hash gives idempotency for identical prompts, but a future implementation should hard-route source-run continuations to the target or block them with a clear message.

## Preservation Audit

| Context surface | Compaction | Handoff | Current risk |
| --- | --- | --- | --- |
| User goal and pending task | Provider summary plus raw tail | Explicit fields for goal and pending work | Summary can omit nuance unless evaluated |
| Current task status | Summary and replayed messages | Source status plus runtime-specific section | Source status may say handed off/completed while target still has pending work |
| Tool and command evidence | Prompt asks provider to preserve it; raw tail may include recent tools | Recent tool calls with state, inputs, and errors | Older evidence depends on summary quality |
| Changed files | Usually summary only unless in raw tail | Recent file changes with hashes and paths | No full diff is carried |
| Pending approvals/actions | Prompt asks provider to preserve unresolved actions | Pending prompt and tool evidence are carried | Action request table state is not serialized as a first-class bundle field |
| Mailbox and coordination | Provider-visible context manifest can include active coordination | Not first-class in the handoff bundle except through retrieval/events | Temporary coordination can expire or be missed |
| Stage/progress state | System prompt and events may include stage state | Active todos and event summaries may include it | Stage id/current gate is not a dedicated handoff field |
| Memory candidates | Not extracted by compaction | Extraction is attempted when source is marked handed off | Candidate extraction depends on provider availability |
| Verification state | Prompt asks provider to preserve it | Verification events are included | Older verification can be summarized too weakly |
| Context manifests | Compaction artifact manifests are recorded | Target manifests include handoff identifiers and context policy | Existing automated coverage is mostly contract-focused |

## Cost And Latency

Token estimation is a simple character-count heuristic, roughly one token per four characters. The hard budget uses the model's effective input budget: context window minus output reserve minus a 15 percent safety reserve. For example, a 272,000-token model with a 4,096-token output reserve has an effective input budget of about 232,819 tokens.

Current compaction cost is one extra provider call using the active run provider/model. Input size is the rendered covered transcript. Output is capped by prompt intent and recorded as a summary token estimate, with the request carrying a 1,500-token summary target. Replay savings are approximately covered-transcript tokens minus summary tokens, minus the retained raw tail. With the default raw-tail count of 8, savings should be large for long sessions, but latency and cost are paid on the active model, which may be the most expensive or slowest model in the user's setup.

## Active-Provider Dependency

Using the active run provider has a good privacy property: compaction does not introduce a new recipient for conversation history. It also preserves provider-specific formatting and reduces settings complexity.

The failure modes are substantial:

- Small-context active models may be unable to read the transcript that needs summarizing.
- Slow or expensive active models make compaction feel punitive right when the user is trying to continue.
- Unavailable active providers block both continuation compaction and handoff preparation.
- Some models are poor summarizers or return tool calls even when no tools are exposed.
- Provider/model mismatch rules prevent using a better summarizer even when a compatible profile exists.
- Active-provider use makes cost display harder because compaction spend is coupled to the run model rather than an explicit continuity model.

## Design Comparison

| Design | Shape | Benefits | Risks |
| --- | --- | --- | --- |
| Active-provider compaction | Keep current behavior | Simple, no extra provider disclosure, preserves current behavior | Slow/expensive/small/unavailable active models block continuity |
| Dedicated compaction and handoff provider | Add explicit continuity provider/profile fields | Clear cost and provider ownership, easy to reason about | Too rigid; hidden defaults would create privacy surprises |
| Routing policy with override | Pick a configured or recommended fast large-context model, allow user override, fall back by policy | Best balance of reliability, cost, and user control | Requires model capability metadata, fallback diagnostics, and new test coverage |

Use the routing-policy design. Store configuration in app-data-backed settings, not repo-local state. The first version should support global default plus per-project override. Per-session override can come later when users need different privacy or cost behavior inside the same project.

## Payload Shape

Do not send only raw transcript forever. The continuity model should receive a structured bundle with:

- Redacted transcript excerpts and bounded raw tail.
- Context manifest summaries, not full manifest JSON by default.
- Source-cited file changes, tool outcomes, pending actions, verification events, todos, and handoff lineage.
- Existing active compaction metadata when present.
- A strict output schema for summary, pending work, evidence, risks, and omissions.

Raw transcript can remain an opt-in fallback when structured extraction is incomplete. This improves privacy, cost, and auditability because the model sees the fields Xero intends to preserve rather than every historical byte.

## Settings And Fallback

Add a continuity model setting with:

- Scope: global app-data default and per-project app-data override.
- Selector: provider profile plus model id, filtered to text-capable models with a known or high-confidence context window.
- Capability labels: context window, estimated input budget, expected cost class, and privacy boundary.
- Fallback modes: active provider, retry same continuity provider, or fail closed.
- Disclosure: show that compaction/handoff may send conversation history, tool evidence, file paths, and redacted durable context to the selected provider.
- Cost display: record compaction input tokens, summary tokens, provider/model, and reported cost when the provider supplies it.

Default rollout should preserve current active-provider behavior unless the user explicitly configures a continuity model or joins an experiment.

## Success Metrics

Track these before changing defaults:

- Handoff context carryover rate for goal, pending work, file changes, tool evidence, and verification.
- Hallucinated completed-work rate in summaries and handoff bundles.
- Pending-work preservation rate.
- Token savings: covered transcript tokens versus replayed summary plus raw tail.
- Continuation latency added by compaction and handoff preparation.
- Provider failure recovery rate and user-visible retry quality.
- User trust signals: clear provider disclosure, understandable cost, and low surprise in target-run first turns.

## Test Plan

Add focused coverage before shipping routing:

1. Unit tests for continuity-model settings validation, app-data persistence, and fallback policy.
2. Provider-adapter tests that force compaction failure, tool-call responses, empty summaries, provider mismatch, and source-hash mismatch.
3. Continuation tests for known budget, unknown budget, stale active compaction, auto-handoff disabled, and source-run continuation after handoff.
4. Structured-output tests comparing active-provider, dedicated-provider, and routed-provider summaries against the same transcript.
5. Handoff target first-turn tests that assert pending work, verification evidence, active todos, and source citations are present before the provider call.
6. Cost/latency instrumentation tests that verify compaction token counts and provider/model metadata are recorded.

## Staged Rollout

1. Instrument current behavior. Record compaction latency, input tokens, summary tokens, provider/model, replay savings, failure codes, and handoff target first-turn quality.
2. Add the app-data continuity-model setting behind an opt-in flag. Keep active-provider fallback as the default.
3. Add structured compaction input and schema-checked output while preserving the raw-transcript path as fallback.
4. Run side-by-side tests for active provider, explicit continuity provider, and automatic routing.
5. Enable user-configured continuity model routing for opted-in projects.
6. Promote automatic routing only when controlled tests beat active-provider compaction on carryover, hallucination, latency, and failure recovery.

## Implementation Notes

The change should touch the session-context contracts, provider model catalog and context-limit metadata, runtime controls/settings, session-history compaction, provider adapters, context manifests, handoff lineage and bundle creation, and the handoff-context test coverage.

Keep new state in OS app-data/global app-data. Do not add repo-local state under legacy project directories. Because this is new behavior, do not add backwards-compatible migration glue unless compatibility is explicitly requested.
