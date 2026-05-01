# Agent Context Meter Implementation Plan

## Reader And Outcome

Reader: an internal Xero engineer or agent implementing the next production slice of the owned-agent runtime.

Post-read action: add a correct, intelligent, model-aware context-remaining indicator to the Agent tab composer, with a compact donut that fills as the active session approaches its effective context limit.

## Goal

Users should be able to glance at the Agent tab composer and know how much usable context remains before the next owned-agent turn becomes risky. The indicator must use the same backend context estimate and model budget that the runtime uses for compaction, handoff, and provider preflight. It must not invent a separate frontend estimate.

The first visible surface is a small donut meter inside the composer box, next to the auto-compact button/toggle. The donut fills as remaining context gets lower, matching the provided visual direction: a quiet circular ring beside a short numeric label. Expanding or hovering the meter shows the evidence behind the number.

## Current State

Xero already has the important pieces, but they are not yet wired into the Agent tab as a lightweight live meter:

- Session context snapshots include provider ID, model ID, estimated model-visible tokens, deferred tokens, usage totals, budget pressure, contributors, policy decisions, redaction state, and a provider request hash.
- The Tauri session-history command can build a context snapshot for a selected project, session, run, provider, model, and optional pending prompt.
- Runtime continuation preflight separately estimates the next provider request before compaction and handoff decisions.
- Provider/model catalogs currently know model identity and thinking capability, but not a first-class context-window contract.
- The existing budget resolver is heuristic and model-family based. It is useful as a fallback, but not authoritative enough to be the final model-aware source.
- The Agent tab renders session title, conversation, memory review, and composer controls, but it does not carry a live context snapshot in its view model.
- Project usage summaries are aggregate spend and activity visibility. They are not the same thing as remaining context for the current session.

## Product Behavior

The Agent tab composer shows a context meter whenever a project session is selected.

Known budget behavior:

- Show a donut and short label such as `58% left`, `12.4K left`, or `Full`.
- The donut fill represents pressure, not remaining space. More fill means less context remains.
- Tooltip or hover-card detail shows provider, model, budget source, estimated next-turn tokens, effective context budget, reserved output/safety budget, remaining tokens, pressure state, and last refresh time.
- When the user types in the composer, the meter updates after a short debounce and includes the unsent prompt in the estimate.
- When a run streams, completes, compacts, hands off, memory changes, or usage updates, the meter refreshes from the backend snapshot.

Unknown budget behavior:

- Show a stable unknown state instead of a fake percentage.
- Keep the label explicit, such as `Context unknown`.
- The detail surface explains that Xero can estimate the next request size but does not know the selected model's context window.
- Unknown budgets stay non-blocking in the UI and should not produce alarming colors.

High pressure behavior:

- At the compact threshold, the meter shifts to a warning treatment and the detail copy points at auto-compact/manual compact behavior.
- At the handoff threshold, the meter shifts to a danger treatment and the detail copy explains that Xero may continue in a fresh same-type run.
- Over budget shows a filled danger ring and reports the overflow estimate.

## Definition Of "Context Left"

Context left is the estimated amount of model input budget still available for the next owned-agent provider request.

Use this formula conceptually:

```text
effective_input_budget = model_context_window - output_reserve - safety_reserve
context_used = backend_estimated_model_visible_tokens_for_next_request
context_left = max(0, effective_input_budget - context_used)
pressure = context_used / effective_input_budget
donut_fill = clamp(pressure, 0, 1)
```

Important details:

- The backend owns the calculation. The UI only formats the returned projection.
- The estimate must include the active system prompt, supported instruction files, approved memory, active compaction summary, selected tool descriptors, replayed conversation/tool context, code-map contributors that are model-visible, and the pending prompt when present.
- Provider usage totals can appear in the detail view, but they must not be counted as model-visible context unless the backend snapshot marks them model-visible.
- Reasoning or thinking effort is model-aware metadata, but it should not silently change the context window unless the provider or model capability contract exposes that behavior.
- Output reserve should come from the same provider adapter defaults and model capability metadata used to send requests.
- Safety reserve should align with the context planner's existing reserve policy so the meter agrees with compaction and handoff decisions.

## Source Of Truth

Create one shared context-limit resolver used by:

- Session context snapshots.
- Owned-agent continuation preflight.
- Context manifests.
- Compaction and handoff policy decisions.
- Agent tab context meter projection.

The resolver returns a structured result, not just a token count:

```text
provider_id
model_id
context_window_tokens
effective_input_budget_tokens
max_output_tokens
output_reserve_tokens
safety_reserve_tokens
source
confidence
diagnostic
fetched_at
```

Source priority:

1. Live provider/model catalog metadata when the provider exposes a trustworthy context window or max input token value.
2. App-local provider profile or manually configured model metadata.
3. Versioned built-in model capability registry for well-known model families.
4. Heuristic model-family fallback.
5. Unknown.

Unknown is a valid result. It is better to show no percentage than to show a confident-looking lie.

## Model-Aware Capability Plan

Extend provider/model capability contracts so context limits are first-class:

- Add optional context-window and output-limit fields to model catalog rows.
- Store the source and freshness of those fields separately from display name and thinking support.
- Preserve manual catalog behavior for providers that cannot expose live model details.
- Allow provider adapters to parse common metadata names without leaking provider-specific shapes into the UI.
- Keep a versioned built-in registry for known families where the live provider does not return context limits.
- Make stale or inferred values visible in the detail surface.

The built-in registry must be easy to update because model limits change. It should be tested as data, not scattered through unrelated runtime code.

## Backend Plan

1. Introduce the shared context-limit resolver.
   - Replace direct heuristic calls in session snapshots and continuation preflight.
   - Return source, confidence, reserve, and diagnostic metadata.
   - Keep unknown budget behavior explicit.

2. Extend the context budget contract.
   - Add effective input budget, remaining tokens, pressure percent, limit source, confidence, output reserve, and safety reserve.
   - Keep existing pressure states, but derive them from the effective input budget.
   - Validate that known-budget snapshots cannot report negative remaining tokens.

3. Align snapshot and preflight estimates.
   - Ensure the Agent tab snapshot and the actual continuation preflight use the same prompt assembly, tool descriptor selection, pending prompt inclusion, and reserve policy.
   - Add a test fixture that proves a typed pending prompt changes the estimate before it is sent.

4. Expose a lightweight projection for the UI.
   - The full context snapshot can remain available for diagnostics.
   - The Agent tab should consume a compact projection with budget, model, provider, generated time, policy action, and optional top contributor groups.

5. Refresh at the right lifecycle points.
   - On project/session selection.
   - On run creation or run switch.
   - On runtime stream completion, failure, compaction, handoff, or recovered run state.
   - On memory approval, disable, delete, or extraction completion.
   - On provider/model selection change.
   - On debounced composer draft changes.

## Frontend State Plan

Add context-meter state alongside the Agent view projection:

- `idle`: no project/session selected yet.
- `loading`: snapshot requested.
- `ready`: known or unknown budget projection returned.
- `stale`: showing the last successful projection while a refresh is in flight.
- `error`: snapshot failed; detail surface shows a retryable diagnostic.

The Agent view should carry:

- The latest projection.
- Load status.
- Load error.
- A refresh action for retries and memory-review callbacks.

Do not calculate token estimates in React. React can derive presentation-only values such as label text, ring dash offset, and color class from backend-provided pressure and remaining token fields.

## UI Plan

Build a dedicated Agent tab context meter component.

Visual structure:

- A fixed-size donut ring using an SVG circle.
- A short text label to the right.
- Stable dimensions so loading, unknown, high pressure, and normal states do not shift the composer action row.
- ShadCN Tooltip for the compact explanation.
- ShadCN HoverCard or Popover for detailed breakdown.
- Optional refresh icon button in the detail surface only when the snapshot failed or is stale.

Placement:

- Put the meter inside the composer box, in the right-side action cluster next to the auto-compact button/toggle.
- Keep it compact enough to coexist with the compact toggle, dictation button, and send button.
- On narrow widths, keep the donut visible and allow the numeric label to collapse to a tooltip-only state before squeezing the icon buttons.

Ring behavior:

- `donut_fill = pressure`.
- Low pressure: subtle neutral/accent ring.
- Medium pressure: stronger informative ring.
- High pressure: warning ring.
- Over budget: danger ring, fully filled.
- Unknown: dashed or muted ring with no fake fill.
- Loading: spinner-like treatment inside the same fixed ring footprint.

Accessibility:

- Use `role="progressbar"` only when the budget is known.
- Set `aria-valuemin`, `aria-valuemax`, and `aria-valuenow` from pressure percent.
- Use `aria-valuetext` such as `42 percent context remaining for GPT-5.4`.
- For unknown budgets, use plain status text rather than progress semantics.
- Respect reduced-motion preferences.

## Detail Surface Content

The expanded detail should answer four questions quickly:

- What model is this based on?
- How much context is estimated for the next turn?
- How much usable budget remains?
- What will Xero do if pressure gets too high?

Suggested fields:

- Provider and model.
- Context left.
- Next-turn estimate.
- Effective input budget.
- Total model window when known.
- Output reserve and safety reserve.
- Budget source and confidence.
- Pressure state.
- Policy decision, such as continue, compact, recompact, handoff, or blocked.
- Active compaction summary presence.
- Top contributor categories by estimated tokens.
- Generated timestamp.

Avoid a full manifest inspector in the first version. The meter should stay a user-facing glanceable control, not a diagnostics console.

## Correctness Invariants

1. The meter never reports a percentage for an unknown model budget.
2. The meter's pressure value comes from the same backend budget resolver as compaction and handoff.
3. The meter includes the pending prompt after debounce.
4. The meter does not count aggregate project usage as current model-visible context.
5. The meter distinguishes estimated tokens from provider-returned usage.
6. The meter preserves redaction guarantees and never exposes raw secret-bearing contributor text.
7. Same session, run, model, provider, prompt, and context state produce the same provider request hash.
8. Model changes refresh the budget before updating the visible percentage.
9. Stale values are labeled as stale.
10. Unknown or stale provider catalogs degrade gracefully instead of blocking normal Agent tab use.

## Implementation Slices

### Slice 1: Context-Limit Resolver

- Create the shared resolver and structured result.
- Move built-in model-family limits into versioned capability data.
- Teach provider catalog mapping to attach context limits when available.
- Replace session snapshot and continuation preflight heuristic calls.
- Add Rust unit tests for known, inferred, manual, stale, and unknown budget paths.

### Slice 2: Budget Contract Projection

- Extend the session context budget DTOs in Rust and TypeScript.
- Add validation for effective budget, remaining tokens, source, confidence, and reserve fields.
- Update snapshot construction and context manifest payloads.
- Add contract tests proving the UI projection and runtime policy see the same budget.

### Slice 3: Agent View State

- Add context-meter state to the desktop state hook and Agent view projection.
- Call the context snapshot command on project/session/run/model changes.
- Debounce composer draft prompt refreshes.
- Refresh after memory and runtime lifecycle events.
- Preserve last successful projection while loading a new one.

### Slice 4: Donut Meter UI

- Build the reusable context meter component using ShadCN Tooltip/HoverCard wrappers.
- Integrate it into the composer action row beside the auto-compact button/toggle.
- Add formatting helpers for token counts, remaining percent, source labels, and pressure labels.
- Add responsive collapse behavior.
- Add accessibility states for known, unknown, loading, stale, error, and over-budget conditions.

### Slice 5: Policy-Aware Details

- Surface compact/handoff thresholds and current policy decision.
- Show active compaction and top contributor category totals.
- Add a retry action for failed projections.
- Keep advanced diagnostics out of the default visible chrome.

### Slice 6: Verification And Docs

- Add focused React tests for every meter state and responsive label behavior.
- Add focused state-hook tests for refresh triggers and debounced pending prompt changes.
- Add Rust tests for resolver, budget contract validation, snapshot/preflight alignment, and unknown-budget behavior.
- Update the session memory/context documentation after the implementation lands.

## Test Plan

Use scoped tests only.

Frontend:

- Agent runtime renders the meter in the composer action row when a session is selected.
- Known budget displays percent or token-left label and correct progress semantics.
- Unknown budget displays no progress percentage.
- High and over-budget states use warning/danger copy.
- Debounced draft prompt refresh calls the snapshot path with pending prompt text.
- Model selection refreshes the meter and avoids showing the old model as current.
- Memory review callbacks refresh the meter after approval/disable/delete.

Backend:

- Context-limit resolver covers live metadata, manual metadata, built-in registry, heuristic fallback, and unknown.
- Snapshot budget fields validate effective budget and remaining tokens.
- Continuation preflight and snapshot projection use the same resolver.
- Pending prompt inclusion increases the next-turn estimate.
- Unknown provider/model returns unknown pressure, not a fabricated token budget.
- Over-budget snapshots report zero remaining tokens and over pressure.

Recommended scoped commands:

```text
pnpm --dir ./client vitest run agent-runtime
pnpm --dir ./client vitest run session-context
pnpm --dir ./client run rust:test -- session_context
pnpm --dir ./client run rust:test -- agent_core_runtime
```

Run only one Cargo-backed command at a time. Format or lint only touched files where the project tooling supports it cleanly.

## Risks And Mitigations

- Model limits change over time. Mitigation: prefer live provider metadata, store source/freshness, and keep the built-in registry centralized and tested.
- Token estimation is approximate for some providers. Mitigation: label estimates clearly and use provider usage only when it is actually available for the relevant request.
- The UI could imply precision it does not have. Mitigation: unknown budgets never show percentages, inferred limits show source/confidence, and stale projections are labeled.
- Draft-prompt refreshes could be noisy. Mitigation: debounce, cancel outdated requests, and preserve last successful projection while refreshing.
- Context meter and runtime policy could drift. Mitigation: one shared backend resolver and tests that compare snapshot and continuation preflight outputs.

## Out Of Scope

- A full context manifest inspector.
- A per-message token ledger in the conversation.
- Provider billing or spend visualization.
- Browser-based verification, because this is a Tauri app.
- Legacy `.xero/` state migration or compatibility shims.

## Done Criteria

- Agent tab composer shows the donut meter for the active session next to the auto-compact button/toggle.
- The donut fills as context remaining decreases.
- Known model budgets show remaining context and detail evidence.
- Unknown model budgets are honest and non-blocking.
- The selected model and provider determine the budget.
- The pending composer prompt is included in the estimate after debounce.
- Runtime preflight, context snapshot, context manifest, compaction, handoff, and UI projection all use the same context-limit resolver.
- Scoped frontend and Rust tests pass.
