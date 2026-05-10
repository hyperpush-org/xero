# Agent Hard-Attached Skills Plan

## Reader And Outcome

Reader: an internal Xero engineer landing cold in agent runtime, Agent Create, and workflow-canvas code.

Post-read action: implement hard-attached skills so a user or Agent Create can attach skills to an agent definition, and every run of that agent receives those skill instructions automatically.

## Current State

Xero already has a durable skill registry, a model-visible `skill` tool, and prompt support for invoked skill context. Agents can discover, install, resolve, and invoke skills during a run when their runtime policy allows the skill tool.

Custom agents are already saved as canonical agent-definition snapshots. Those snapshots drive runtime selection, prompt compilation, tool policy, authoring previews, workflow-agent details, and the visual canvas. Agent Create can draft and save definitions through the agent-definition tool, and the canvas can create, edit, duplicate, serialize, and validate custom agents.

The missing product primitive is different from the current skill tool: an attached skill is not a tool call the model decides to make. It is user-selected agent configuration. Once attached, it should be injected on every run of that agent, without granting the model general skill-discovery or skill-invocation authority.

## Goals

- Add `attachedSkills` as first-class canonical agent-definition data.
- Let users attach skills while creating or editing an agent.
- Let Agent Create draft, validate, preview, save, update, and clone definitions that include attached skills.
- Inject attached skill context every time the agent runs.
- Keep attachment resolution durable, auditable, and app-data-backed.
- Add a real Skills node type to canvas view and edit modes.
- Preserve the existing skill tool as optional runtime capability, separate from attached-skill injection.
- Fail closed when an attached skill is unavailable, blocked, untrusted, stale, or content-pinned incorrectly.

## Non-Goals

- Do not add temporary debug or test UI.
- Do not use repo-local `.xero/` state for attached-skill data.
- Do not make attached skills silently grant `skillRuntimeAllowed`.
- Do not support legacy agent-definition snapshots that omit the new field. This is a new app; the canonical schema should move forward.
- Do not let Agent Create invoke arbitrary skill content just because it can author an attachment.

## Product Semantics

Hard-attached means:

- The attachment lives on the agent definition, not on a single prompt or run.
- Every new run of that agent resolves the configured skill before provider execution.
- The resolved skill context is included in prompt compilation as lower-priority, untrusted skill context.
- The model does not need to call the `skill` tool to receive the attached skill.
- If resolution fails, the run should not start with a partially configured agent.

The initial version should pin skill content by `versionHash` at attachment time. This gives reproducible behavior and makes local or plugin skill edits visible as stale attachments instead of silently changing an agent's behavior. A later explicit "refresh attachment" action can update the pin.

## Data Contract

Introduce agent-definition schema version 2 and require every custom definition to include:

```json
"attachedSkills": []
```

Each entry should carry enough durable identity and UI metadata to validate and render without path leakage:

```json
{
  "id": "rust-best-practices",
  "sourceId": "skill-source:v1:...",
  "skillId": "rust-best-practices",
  "name": "Rust Best Practices",
  "description": "Guide for writing idiomatic Rust code.",
  "sourceKind": "local",
  "scope": "global",
  "versionHash": "sha256-or-source-version",
  "includeSupportingAssets": false,
  "required": true
}
```

Rules:

- `sourceId`, `skillId`, `name`, `sourceKind`, `scope`, `versionHash`, `includeSupportingAssets`, and `required` are required.
- `id` is the stable canvas node id seed and must be unique within the definition.
- `sourceId` must be unique within the definition.
- `required` is always `true` in the first release; keep the field so optional attachments can be added later without changing the shape again.
- `versionHash` must match the currently resolved skill unless the user explicitly refreshes the attachment.
- Attachments may reference bundled, local, project, GitHub, dynamic, MCP, or plugin skills only if the registry says the source is enabled and trusted enough for model-visible use.

## Architecture Decisions

1. Attached skills are agent definition state.

   Store them in the canonical snapshot and version history. Do not add a separate repo-local attachment table. The persisted agent definition remains the source of truth, and the run records store the resolved attachment context used for that run.

2. Injection reuses skill context prompt machinery.

   The runtime should resolve attached skills into the same validated context payload used by invoked skills, then pass them into prompt compilation with a distinct inclusion reason such as `attached_agent_skill`.

3. Attachment does not imply skill-tool authority.

   The agent may receive attached skill instructions even when the `skill` tool is not exposed. `skillRuntimeAllowed` is only required when the agent may call the skill tool during a run.

4. Attachments fail closed.

   A missing, disabled, blocked, approval-required, untrusted, stale, or hash-mismatched required attachment should produce a user-fixable run-start diagnostic and block the run.

5. The run uses a resolved attachment snapshot.

   Resolve and persist attached skill context at run creation so resume, continuation, and provider-loop turns use the same skill content even if the registry changes mid-run.

6. Canvas Skills nodes are not Tool nodes.

   A Skills node represents always-injected context. A Tool node represents callable runtime capability. They need separate DTOs, node components, edges, validation, and serialization.

## Implementation Slices

### S1. Canonical Agent Definition Contract

Add attached-skill DTOs to Rust and TypeScript agent-definition models.

Required behavior:

- Bump the canonical custom-agent definition schema to version 2.
- Add strict `attachedSkills` validation in frontend Zod and backend normalization.
- Emit `attachedSkills: []` from blank canvas details, authoring templates, duplicate flows, tests, and Agent Create examples.
- Include attached skills in definition diffs, preview policies, validation reports, and authoring graph canonical data.
- Validate duplicate ids, duplicate source ids, required fields, attachable source state, trust state, and pinned version hash.

Acceptance checks:

- A definition with `attachedSkills: []` parses and saves.
- A definition with duplicate attached source ids fails validation.
- A definition referencing a blocked or stale source fails validation with a repair hint.
- Version history and diff output report attached-skill changes.

### S2. Skill Registry Attachment Resolver

Add a backend resolver that converts attached-skill refs into validated skill context payloads.

Required behavior:

- Resolve by `sourceId` first; use `skillId` only for diagnostics and display.
- Require enabled, trusted or user-approved sources.
- Reject approval-required, untrusted, blocked, disabled, stale, missing, or hash-mismatched sources.
- Load `SKILL.md` content and optional supporting assets through existing skill runtime validators.
- Return sanitized model-visible context plus structured diagnostics.
- Persist the resolved run-level attachment snapshot under app-data project runtime state.

Acceptance checks:

- Bundled, local, project, plugin, and dynamic skill attachments resolve through the same source contract.
- A changed local `SKILL.md` blocks a pinned attachment until refreshed.
- A run resume uses the persisted run-level attachment context, not a newly changed source.

### S3. Runtime Prompt Injection

Wire resolved attached skills into owned-agent run creation, continuation, and provider-loop prompt compilation.

Required behavior:

- During run creation, resolve attached skills before assembling the persisted system prompt.
- During provider turns, merge run-level attached skill contexts with skill contexts produced by actual `skill` tool invocations.
- Dedupe by `sourceId` and content hash; attached context should win over duplicate invoked context.
- Use a prompt fragment title and provenance that make the attachment visible in context manifests.
- Keep attached skills lower priority than Xero system/runtime/developer policy, tool policy, repository instructions, and user messages.
- Record context manifest contributors for attached skills.

Acceptance checks:

- Starting an agent with an attached skill includes that skill in the compiled prompt.
- The same agent can still be denied access to the `skill` tool.
- A model-invoked skill and an attached skill do not create duplicate prompt fragments.
- Context manifests show which attached skills were injected.

### S4. Agent Create Support

Teach Agent Create to author attached skills without granting broad skill invocation.

Required behavior:

- Extend the agent-definition tool contract and examples so Agent Create can emit `attachedSkills`.
- Provide Agent Create a read-only attachable-skill catalog through the agent-definition tool or authoring catalog projection.
- Update validation and preview output so Agent Create receives precise diagnostics for missing, stale, or unavailable attachments.
- Update Agent Create prompt/eval fixtures to include the new schema and explain that attached skills are always-injected context, not callable tools.

Acceptance checks:

- Agent Create can draft a custom agent with attached skills when the user requests it.
- Agent Create cannot invoke arbitrary skill bodies merely to author an attachment.
- Agent Create validation fails closed for unknown or untrusted attached skills.

### S5. Workflow Detail And Authoring Catalog

Expose attached skills through workflow-agent detail and the authoring catalog.

Required behavior:

- Add `attachedSkills` to workflow-agent detail DTOs.
- Add attachable skill entries to the authoring catalog, using the skill registry for enabled/trusted candidates.
- Include availability metadata so the UI can show stale or blocked attachment states in view mode.
- Include attached skills in authoring graph `editableFields` and canonical graph projection.

Acceptance checks:

- Built-in and custom detail responses include an empty attached-skill list when none exist.
- A custom agent detail round-trips attached skills from snapshot to DTO.
- The authoring catalog offers only attachable skills by default and can surface unavailable entries for diagnostics.

### S6. Canvas Skills Node Type

Add Skills nodes to visual canvas view and edit modes.

Required behavior:

- Add a `skills` node kind and `SkillNode` component.
- Add a header summary count for attached skills.
- Render skill nodes in view mode with name, skill id, source kind, required state, and hash/staleness status.
- Add edit-mode creation through the existing drag/drop picker pattern.
- Add a properties panel for selecting a skill from the catalog, toggling supporting assets, refreshing a pin, and removing the node.
- Serialize Skills nodes into `attachedSkills` during save.
- Rebuild Skills nodes from `attachedSkills` during edit, duplicate, and view.
- Keep ShadCN UI patterns consistent with the existing canvas controls.

Acceptance checks:

- Creating an agent can start from an empty Skills lane.
- Dragging or picking a skill adds a Skills node, not a Tool node.
- Saving and reopening the agent preserves attached skills.
- Duplicating an agent preserves attached skills while generating a fresh agent id.

### S7. Policy, Preview, And Diagnostics

Make attached-skill state visible and safe.

Required behavior:

- Add attached-skill diagnostics to effective runtime preview.
- Show whether each attachment is resolved, stale, unavailable, or blocked.
- Ensure capability permission explanations distinguish attached skill context from skill-tool runtime access.
- Emit audit events when an agent definition saves, updates, clones, or archives attached skills.
- Add repair hints for "enable source", "approve source", "refresh pin", and "remove attachment".

Acceptance checks:

- Preview explains why an attachment will or will not inject.
- A stale hash points at refresh/removal, not a generic validation failure.
- Audit payloads identify attached skill source ids without exposing local paths.

### S8. Verification Coverage

Add focused tests around the new contract.

Required coverage:

- TypeScript schema validation for `attachedSkills`.
- Rust normalization and validation for schema v2.
- Skill attachment resolver success and failure cases.
- Prompt compiler includes attached skills and preserves policy ordering.
- Run creation persists resolved attached contexts.
- Run resume uses persisted attachment context.
- Agent Create eval fixture emits schema v2 with `attachedSkills`.
- Workflow detail and authoring catalog include attached skills.
- Canvas graph build, edit, save, duplicate, and snapshot round-trip for Skills nodes.

Run scoped tests only. For Rust, run one Cargo command at a time.

## Rollout Order

1. Land schema v2 and empty `attachedSkills` round-trip across backend, frontend, templates, and tests.
2. Land skill attachment resolver and validation diagnostics.
3. Land runtime prompt injection and run-level persistence.
4. Land workflow detail/catalog support.
5. Land canvas Skills nodes in view and edit modes.
6. Land Agent Create prompt/tool/eval updates.
7. Land preview, audit, repair-hint, and stale-pin polish.

## Release Gate

The feature is complete only when all of these are true:

- Every custom agent definition has schema v2 and `attachedSkills`.
- Users can attach, view, edit, remove, save, duplicate, and reopen Skills nodes.
- Agent Create can draft and validate attached-skill definitions.
- Starting a run injects all required attached skills before the provider turn.
- The agent does not need skill-tool access to receive attached skills.
- Unavailable, blocked, untrusted, stale, or hash-mismatched attachments block the run with user-fixable diagnostics.
- Context manifests and preview output clearly show attached skill injection.
- Focused Rust and TypeScript tests cover schema, resolver, runtime injection, Agent Create, and canvas round-trip behavior.

## Reader-Test Notes

A cold reader should start with S1 and make `attachedSkills: []` a required schema v2 field everywhere custom definitions are produced. After that, S2 and S3 create the real runtime behavior, while S5 and S6 make the feature visible and editable. The key invariant is that attached skills are always-injected context, not callable tools, so policy and UI must keep those concepts separate throughout the implementation.
