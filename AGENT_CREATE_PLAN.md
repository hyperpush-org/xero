# Agent Create Plan Draft

Reader: an internal Xero engineer who will implement user-created agents.

Post-read action: build a first-class built-in agent named **Agent Create** that can interview the user, draft a high-quality custom agent, validate it, persist it in app-data-backed state, and make the created agent usable through the same owned-agent runtime as Ask, Engineer, and Debug.

Status: draft.

## Goal

Xero currently has three first-class runtime agents: Ask, Engineer, and Debug. That model is good, but the set is closed. Users should be able to create their own agents by talking to an agent.

Add **Agent Create** as a fourth built-in runtime agent. Agent Create is not a thin prompt preset. It is a guided agent-design workflow that produces durable custom agent definitions with the same quality bar as the built-ins: explicit system contract, safe tool policy, LanceDB-backed project context and memory integration, context manifests, handoff support, usage tracking, validation, and tests.

Custom agents should then appear in the normal agent selector and run through the normal owned-agent runtime.

## Product Shape

Agent Create is selected like Ask, Engineer, or Debug.

The user describes the agent they want. Agent Create conducts a short design conversation, then produces an agent definition draft. The draft is shown to the user for review with clear user-facing fields:

- Name and short label.
- Purpose and best-use cases.
- Default model and approval mode.
- Capabilities and tool access.
- Memory and retrieval behavior.
- Workflow instructions.
- Final response contract.
- Safety limits.
- Example prompts the agent should handle.

The user can ask Agent Create to revise the draft. When the user approves it, Agent Create saves the agent. The saved agent becomes available for future runs.

Created agents can be global or project-scoped. Prefer global as the default when the agent is general-purpose, and project-scoped when the definition depends on project-specific constraints, tools, memories, or terminology.

## Design Principle

Custom agents must be registry-backed runtime agents, not prompt snippets.

The current architecture treats runtime agent identity as a closed set. User-created agents need a durable registry. Built-ins should become seeded registry definitions, and custom agents should use the same definition shape. Runtime runs should reference an agent definition id and a pinned definition version so old runs remain explainable after an agent is edited.

## Agent Definition Contract

Every built-in or custom agent definition should contain:

- Stable id.
- Version.
- Display name.
- Short label.
- Description.
- Task purpose.
- Scope: built-in, global custom, or project custom.
- Lifecycle state: draft, active, archived.
- Base capability profile.
- Default approval mode.
- Allowed approval modes.
- Tool policy.
- Prompt fragments.
- Workflow contract.
- Final response contract.
- Project data policy.
- Memory candidate policy.
- Retrieval defaults.
- Handoff policy.
- Validation report.
- Created and updated metadata.

Base capability profiles should remain small and explicit:

- `observe_only`: Ask-like read-only behavior.
- `engineering`: Engineer-like repository mutation with plan, approval, and verification gates.
- `debugging`: Debug-like engineering behavior with stronger evidence and root-cause expectations.
- `agent_builder`: Agent Create's controlled app-data mutation profile.

Custom agents choose one base capability profile and then narrow it with a tool policy. They should not get broader powers than the profile permits.

## Agent Create Contract

Agent Create should use the `agent_builder` capability profile.

It can:

- Read durable project context and approved memory.
- Inspect the available tool catalog and tool metadata.
- Draft an agent definition.
- Validate an agent definition.
- Run lightweight definition evaluations.
- Save, update, archive, and clone custom agent definitions after user approval.

It cannot:

- Edit repository files.
- Run shell commands.
- Start or stop processes.
- Control browsers or devices.
- Invoke arbitrary MCP or external-service tools.
- Create new tool implementations.
- Bypass approval or redaction policy.
- Persist unreviewed memory as approved memory.

Agent Create can choose existing tool access for the new agent, but v1 should not let users generate new tools. New tool integrations should remain a separate engineering or plugin workflow.

## Runtime Integration

Replace the closed runtime-agent enum at the model boundary with registry-backed descriptors.

Runs should store:

- Selected agent definition id.
- Selected agent definition version.
- Base capability profile.
- Display label snapshot.
- Provider and model snapshot.
- Approval mode snapshot.

The runtime should resolve the pinned definition before compiling a system prompt. If the definition is missing or archived, existing runs continue from the pinned snapshot stored with the run; new runs cannot start from inactive definitions.

Prompt compilation should remain fragment-based:

- Xero system policy.
- Agent definition policy.
- Active tool policy.
- Repository instructions.
- Project code map.
- Skill context.
- Owned process state.
- Approved memory.
- Retrieved project records.

The agent definition policy fragment is where a custom agent's purpose, workflow, and final response contract live. It must never outrank Xero system policy, tool policy, user approvals, or repository instructions.

## Tool Policy

Tool access should be derived from the base capability profile, then narrowed by the agent definition.

The policy shape should support:

- Allowed tool groups.
- Allowed exact tools.
- Denied exact tools.
- Allowed effect classes.
- External-service allowance.
- Browser/device-control allowance.
- Skill-runtime allowance.
- Subagent allowance.
- Command allowance.
- Destructive-write allowance.
- Required approval modes for risky effects.

Default custom agents to least privilege. Agent Create should ask before granting write, command, browser, device, skill, subagent, MCP, or external-service access.

The runtime should validate the effective policy at run start and at tool dispatch. A custom agent definition must not be able to smuggle a broader policy through prompt text.

## LanceDB, Retrieval, And Memory

Custom agents should use the same Lance-backed project record and approved-memory stores as built-ins.

Lance-backed records should continue to carry runtime agent metadata, but the metadata must support custom ids and definition versions. Retrieval filters should work for built-ins, custom agents, base capability profiles, sessions, tags, record kinds, memory kinds, related paths, recency, confidence, and importance.

Agent Create should persist useful design outcomes as project records, not approved memory. Examples:

- Why the agent exists.
- Chosen base capability profile.
- Tool policy rationale.
- Safety constraints.
- Example tasks.
- Evaluation results.

Created agents should inherit normal memory behavior:

- Approved memory is injected as lower-priority context.
- Memory candidates are extracted after completion, pause, failure, and handoff.
- Candidates remain disabled until reviewed.
- Prompt-injection-shaped or secret-bearing candidates are blocked or redacted.

Agent definitions themselves should be stored in transactional app-data state, with Lance records used for retrieval and auditability.

## Data Model

Add an agent-definition registry in app-data-backed state.

Suggested tables:

- `agent_definitions`: current definition metadata and lifecycle state.
- `agent_definition_versions`: immutable version snapshots.
- `agent_definition_validation_runs`: validation and eval results.
- `agent_definition_usage`: optional aggregate usage and quality signals.

Built-ins are seeded records in the same registry:

- Ask.
- Engineer.
- Debug.
- Agent Create.

Because this is a new application and backwards compatibility is not required, update the fresh schema baseline instead of carrying compatibility shims. Remove hard-coded checks that only allow `ask`, `engineer`, and `debug`; replace them with non-empty ids plus registry validation at command/runtime boundaries. Where foreign keys are practical, prefer them. Where LanceDB or denormalized snapshots need plain text, store the pinned id and version.

Do not write new state under `.xero/`. Definitions, validation reports, Lance records, and usage history belong under the OS app-data-backed storage model.

## User Interface

Use ShadCN components where possible.

Required surfaces:

- Agent selector includes built-ins and active custom agents.
- Agent Create conversation can draft, revise, validate, and save an agent.
- A review surface shows the draft definition before activation.
- Agent management allows rename, clone, archive, delete, and version history.
- Project-scoped agents are visually distinct from global agents.
- Invalid or blocked definitions show actionable diagnostics.

Do not add temporary development UI. Every UI surface should be user-facing.

## Agent Creation Flow

1. User selects Agent Create.
2. User describes the agent they want.
3. Agent Create gathers missing intent: purpose, scope, tools, risk tolerance, expected outputs, project specificity, and example tasks.
4. Agent Create retrieves relevant project context if the agent is project-specific.
5. Agent Create inspects the tool catalog and proposes the smallest safe capability set.
6. Agent Create drafts a definition.
7. Runtime validates the definition structurally and semantically.
8. Optional lightweight evals run against the user's example prompts.
9. User reviews and approves activation.
10. Runtime saves an immutable definition version and marks the agent active.
11. Agent selector refreshes and the new agent can start runs.

## Validation

Every saved agent definition must pass validation.

Validation should check:

- Name, id, labels, and descriptions are non-empty and length-bounded.
- Base capability profile is valid.
- Tool policy is a subset of the base profile.
- Approval modes are compatible with the profile.
- Prompt fragments do not contain instruction-hierarchy violations.
- Prompt fragments do not claim unavailable tools.
- Retrieval and memory policies use known record and memory kinds.
- Output contract is clear enough for continuation and handoff.
- External services, commands, browser control, device control, skills, subagents, and destructive writes require explicit user opt-in.
- Secret-shaped text is redacted or rejected.
- Definition version is immutable after activation.

Validation failures keep the definition in draft and return diagnostics to Agent Create and the UI.

## Quality Bar

Agent Create should produce definitions that are closer to built-in descriptors than ad hoc personas.

Each created agent should have:

- A narrow purpose.
- A clear workflow.
- A concrete final response contract.
- Explicit tool boundaries.
- A retrieval strategy.
- A memory strategy.
- A safety and approval posture.
- At least three example tasks.
- At least three refusal or escalation cases.
- A validation report.

The default behavior should be conservative: if the user asks for a broad "do everything" agent, Agent Create should split it into a narrower recommendation or explain the risks before saving.

## Handoff, Compaction, And Continuity

Custom agents should participate in the same continuity machinery as built-ins.

Same-type handoff should mean same agent definition id and pinned version. If a definition was edited after the source run began, the target handoff run should continue with the source run's pinned version unless the user explicitly starts a new run on the latest version.

Context manifests should include:

- Agent definition id.
- Agent definition version.
- Base capability profile.
- Prompt fragment hashes.
- Tool policy hash.
- Retrieval query ids.
- Included and excluded memory ids.
- Validation version.

Auto-compact and auto-handoff should be controlled by the definition's policy and the user's runtime settings.

## Security

Agent definitions are untrusted user-authored configuration.

Security requirements:

- Prompt text cannot change Xero's instruction hierarchy.
- Tool access is enforced outside the prompt.
- Agent Create cannot save a definition without user approval.
- Agent Create cannot create arbitrary executable tools.
- Custom agents cannot self-escalate tool access during a run.
- Redaction runs before persistence, retrieval display, prompt injection, handoff generation, and validation reports.
- App-data state is the source of truth, not repo-local files.
- Definition exports, if added later, must be explicit and redacted.

## Implementation Phases

### Phase 1: Registry Foundation

Introduce the registry-backed agent descriptor model. Seed Ask, Engineer, Debug, and Agent Create as built-ins. Update frontend schemas, runtime contracts, controls, run snapshots, and tests to accept registry descriptors instead of a closed three-value enum.

Success condition: the app still supports Ask, Engineer, and Debug, and Agent Create appears as a built-in descriptor without creating agents yet.

### Phase 2: Persistence And Versioning

Add app-data-backed storage for agent definitions and immutable versions. Persist selected definition id and version on runs. Update context manifests, handoff lineage, retrieval logs, project records, and usage records to store custom ids and pinned versions.

Success condition: a custom definition can be inserted by a test, selected for a run, and recovered after reload with the same pinned version.

### Phase 3: Agent Create Runtime

Add the `agent_builder` capability profile, Agent Create prompt contract, and controlled agent-definition tools. The tools should draft, validate, save, update, archive, clone, and list definitions. Saving requires explicit user approval.

Success condition: Agent Create can create a valid observe-only custom agent in a test without repository mutation.

### Phase 4: Custom Agent Execution

Compile custom agent prompts from definition fragments. Enforce custom tool policies at registry resolution, tool discovery, tool activation, and dispatch. Integrate custom ids with Lance retrieval, approved memory, memory candidates, project records, handoff, compaction, and run summaries.

Success condition: a custom engineering-capable agent can inspect, edit, and verify through the same gates as Engineer, while an observe-only custom agent is blocked from mutation like Ask.

### Phase 5: User-Facing UI

Add the ShadCN-backed creation and management surfaces. Keep the flow inside the existing runtime experience: select Agent Create, converse, review draft, activate, then select the created agent.

Success condition: no temporary UI exists, and a user can create, revise, activate, archive, and start a custom agent from normal app surfaces.

### Phase 6: Quality Evals

Add scoped eval fixtures for agent definitions. Cover prompt quality, tool-policy narrowing, retrieval behavior, memory candidate behavior, handoff behavior, prompt-injection rejection, and version pinning.

Success condition: built-ins and representative custom agents pass the same harness quality gates.

## Test Plan

Frontend tests:

- Runtime schemas accept built-ins and custom descriptors.
- Agent selector renders built-ins and active custom agents.
- Agent Create review flow validates draft, blocked, and active states.
- Ask-like custom agents force suggest-only approval.
- Engineering custom agents expose plan and verification controls.

Rust tests:

- Fresh schema includes agent definition registry.
- Built-ins seed deterministically.
- Definition versions are immutable.
- Run creation pins definition id and version.
- Missing or archived definitions cannot start new runs.
- Existing pinned runs can load from stored snapshots.
- Tool policy cannot exceed base capability profile.
- Agent Create cannot access repository mutation, command, browser, device, MCP, skill, subagent, or external-service tools.
- Custom observe-only agents cannot mutate.
- Custom engineering agents use normal stale-write, approval, rollback, and verification gates.
- Lance project records and memory retrieval work with custom ids.
- Same-agent handoff preserves definition id and version.
- Redaction blocks unsafe definition content and memory candidates.

Run focused tests and scoped formatting. Only run one Cargo command at a time.

## Acceptance Criteria

- Agent Create exists as a first-class built-in agent.
- A user can create a custom agent through conversation.
- The custom agent is persisted under app-data-backed state, not `.xero/`.
- The custom agent appears in the agent selector.
- The custom agent runs through the owned-agent runtime.
- Tool access is enforced by runtime policy, not prompt text.
- LanceDB-backed project records, approved memory, retrieval logs, context manifests, memory candidates, and handoff all include custom agent metadata.
- Agent definitions are versioned and pinned per run.
- Unsafe definitions are blocked before activation.
- Scoped frontend and Rust tests cover the core behavior.

## Open Questions

- Should global custom agents be available to every project immediately, or should each project opt in?
- Should Agent Create allow users to import and export agent definitions in v1?
- Should custom agents support subagents in v1, or should that be reserved for built-ins until policy is stronger?
- Should model/provider defaults be part of the definition, or only a suggestion applied to runtime controls?
- Should a saved custom agent be activated immediately after approval, or should the user explicitly click an activation control after validation?

## Recommended V1 Decisions

- Support both global and project-scoped agents.
- Do not support import/export yet.
- Do not let Agent Create create new tools.
- Disable subagent access for custom agents by default.
- Store provider/model defaults as suggestions, not hard requirements.
- Activate immediately after explicit approval and successful validation.

