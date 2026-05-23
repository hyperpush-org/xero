# Default Agent Models Implementation Plan

## Reader And Outcome

This plan is for an engineer implementing per-agent default model selection in Xero. After reading it, they should be able to add a model default to each agent definition, expose it in the agent authoring UI, and make new runs start on that agent's configured provider/model unless the user explicitly overrides it.

## Product Goal

Users can configure a default provider/model pair for any agent. For example, one agent can default to Anthropic Sonnet while another defaults to xAI Grok. Switching agents in the composer should also switch the suggested model to that agent's default, while still allowing a per-run override.

The feature should apply to built-in agents and custom agents, but persistence differs:

- Custom agents store the default in the agent definition snapshot.
- Built-in agents use app-data state keyed by runtime agent id.

This avoids repo-local `.xero/` state and keeps project state in the supported app-data path.

## Current System Shape

Agent definitions already have a loose optional `defaultModel` field. The TypeScript schema accepts it as unknown, and the Rust agent-definition normalizer preserves it in saved snapshots. That means the implementation can promote an existing placeholder into a real contract instead of creating a parallel concept.

Runtime model selection currently comes from run controls. The composer stores the last chosen model locally, builds a `RuntimeRunControlInput`, and sends it with `startRuntimeRun`. If run controls omit a model, Rust falls back to the active provider profile's default model. That fallback is provider-centric, not agent-centric.

Provider credentials already store provider-profile defaults, and the provider model catalog exposes selectable models with provider ids, provider profile ids, model ids, labels, and thinking capability. Those catalog rows should be reused by the agent-default picker.

## Data Contract

Promote `defaultModel` to a typed optional object:

```ts
{
  providerId: string,
  providerProfileId?: string | null,
  modelId: string,
  selectionKey?: string,
  thinkingEffort?: "none" | "minimal" | "low" | "medium" | "high" | "x_high" | null
}
```

Rules:

- `modelId` is required when `defaultModel` exists.
- `providerId` identifies the provider family.
- `providerProfileId` pins a specific credential profile when present.
- `selectionKey` is UI convenience only and must be reconstructable.
- `thinkingEffort` is optional and must be supported by the selected model when known.
- Missing `defaultModel` means "inherit the existing composer or provider-profile default."

Because this is a new application and backwards compatibility is prohibited, increment the agent-definition schema version and reject malformed `defaultModel` instead of silently accepting legacy shapes.

## Persistence Plan

Custom agents:

- Add a first-class schema for agent default model selection to the frontend agent-definition model.
- Add matching Rust validation in the agent-definition runtime validator.
- Keep the field inside the saved definition snapshot so version history captures default model changes.
- Include default-model changes in the pre-save review/diff so users can approve them like any other agent definition change.

Built-in agents:

- Add an app-data table or app-data JSON record keyed by runtime agent id.
- Store the same default-model object shape, plus timestamps.
- Provide commands to list and upsert built-in agent defaults.
- Do not write these defaults into repo-local state.

Rationale: custom agent defaults belong to the definition because they travel with the agent version. Built-in agent defaults are user preferences because built-ins do not have editable definition snapshots.

## Runtime Resolution

Introduce a single resolver that determines the initial model for a new run:

1. Explicit run controls from the composer or remote bridge.
2. The selected custom agent definition's `defaultModel`.
3. The selected built-in agent's app-data default.
4. Existing provider-profile default fallback.

The resolver should return a complete run-control model selection: provider profile id, provider id, model id, thinking effort, and any validation diagnostic.

Important behaviors:

- Existing active runs keep their current model. Agent defaults only apply when starting a new run or when the user switches the draft composer agent before starting.
- If the default references a deleted provider profile, show it as unavailable and fall back only after warning the user in the composer.
- If the default references a model that is not in the live catalog, keep the configured model visible as an orphaned option and allow the backend preflight to report the real problem.
- If the user manually changes the model after selecting an agent, that draft selection wins for the next run until they change agents again or reset to default.

## UI Plan

Use ShadCN controls for all new UI.

Agent composer:

- When the user selects an agent, load that agent's default model into the draft model selector.
- Show the normal model selector; do not add temporary debug UI.
- Indicate unavailable configured defaults using the existing orphaned model pattern.
- Preserve explicit user overrides while the draft agent remains unchanged.

Custom agent authoring:

- Add a "Default model" control to the agent details/properties surface.
- Reuse the provider model catalog grouped by provider/profile.
- Include optional thinking effort only when the selected model supports it.
- Store the selection into the definition snapshot as `defaultModel`.

Built-in agent defaults:

- Add a compact settings section for built-in agent defaults.
- List built-in agents with their configured default or "Use provider default."
- Each row uses a model picker sourced from the same provider model catalog.
- Include a clear/reset action to return an agent to provider default.

Do not use the word "Workflow" for this feature except for the top-bar Workflow tab. User-facing copy should say "agent default model" or "default model."

## Backend Commands

Add or extend commands so the frontend can:

- Read effective agent default models for built-in and custom agents.
- Upsert/reset a built-in agent default model.
- Save a custom agent definition with a validated `defaultModel`.
- Preview custom agent changes with default-model diagnostics.

The custom-agent save path should continue to use the existing definition write approval flow.

## Validation And Error Handling

Frontend validation:

- Prevent empty model ids.
- Prevent impossible thinking-effort selections when catalog capability is known.
- Keep unavailable defaults renderable so users can repair them.

Backend validation:

- Reject invalid `defaultModel` object shapes.
- Reject empty provider ids and model ids.
- Verify provider profile references when a profile id is present.
- Produce diagnostics when the model is not available in the provider catalog, but do not require a live catalog for every save.
- Require explicit app-data wipe only for stale incompatible state during development, consistent with project policy.

## Tests

Frontend unit tests:

- Agent-definition schema accepts the typed default model and rejects malformed objects.
- Composer switches the draft model when selecting agents with different defaults.
- Manual composer model override wins until agent selection changes.
- Custom agent authoring saves `defaultModel` in the snapshot.
- Built-in settings upsert and reset defaults.
- Unavailable default models render as orphaned options.

Rust tests:

- Agent-definition validation accepts valid `defaultModel`.
- Agent-definition validation rejects empty or malformed default models.
- Run-control resolution chooses explicit controls before agent defaults.
- Custom agent default beats provider-profile default.
- Built-in agent app-data default beats provider-profile default.
- Deleted provider profile yields a repairable diagnostic.

Run only scoped tests for changed TypeScript and Rust modules.

## Implementation Slices

1. Define the shared default-model contract.
   Add TypeScript and Rust schemas, increment the agent-definition schema version, and update validation tests.

2. Persist built-in defaults in app data.
   Add read/upsert/reset commands and tests for built-in runtime agent defaults.

3. Resolve defaults when starting runs.
   Add the backend resolver and make run start use it before falling back to provider-profile defaults.

4. Apply defaults in the composer.
   When draft agent selection changes, update the draft model selection from the effective default unless the user has already made an explicit model choice for the current agent.

5. Add authoring UI for custom agent defaults.
   Reuse the existing model catalog picker and persist `defaultModel` through preview/save.

6. Add settings UI for built-in agent defaults.
   Provide per-built-in-agent model pickers and reset actions using ShadCN controls.

7. Polish diagnostics and repair states.
   Make stale provider profiles and orphaned model ids understandable without hiding the configured value.

## Acceptance Criteria

- A custom agent can be saved with a default model and new runs use it.
- Built-in agents can each have independent defaults stored outside repo-local state.
- Switching from one agent to another updates the draft model to that agent's default.
- Manual per-run model selection still overrides the default.
- Active runs do not unexpectedly change model when defaults are edited.
- Unavailable defaults are visible and repairable.
- Scoped tests cover schema, persistence, resolver priority, and composer behavior.
