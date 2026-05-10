# Agent System Audit & Improvement Plan

Date: 2026-05-10
Scope: `client/src-tauri/src/runtime/agent_core/**`,
       `client/src-tauri/src/runtime/autonomous_*/**`,
       `client/src-tauri/src/commands/agent_*.rs`,
       `client/src-tauri/src/commands/workflow_agents.rs`,
       `client/components/xero/workflow-canvas/**`,
       `client/components/xero/agent-runtime*/**`,
       `client/components/xero/settings-dialog/agents-section.tsx`,
       `docs/agent-*.md`.

This is an audit of how Xero creates, runs, and customizes agents, plus a concrete
improvement backlog. The audit was done by reading the source — file:line references
are what was true on the working tree at audit time.

---

## 1. How agents are created today

### 1.1 Two coexisting authoring paths

Xero has **two parallel surfaces** for producing an agent definition, and both
ultimately write the same `xero.agent_definition.v1` snapshot to the per-project
SQLite store:

1. **Conversational "Agent Create"** — the built-in `agent_create` runtime agent
   (`client/src-tauri/src/commands/contracts/runtime.rs:421-435`,
   `client/src-tauri/src/runtime/agent_core/tool_descriptors.rs:740-751`).
   The user chats; the agent calls the `agent_definition` tool
   (`client/src-tauri/src/runtime/autonomous_tool_runtime/agent_definition.rs:240-278`)
   with actions `Draft`, `Validate`, `Preview`, `Save`, `Update`, `Archive`,
   `Clone`, `List`, `ListAttachableSkills`. Save/Update/Archive/Clone require
   explicit operator approval and go through
   `agent_definition_with_operator_approval`.
2. **Canvas-native "by hand"** — ReactFlow canvas in
   `client/components/xero/workflow-canvas/agent-visualization.tsx` switches into
   `editing` mode (line 283), reuses the same node types as read-only viewing
   (header, prompt, skills, tool, db-table, output, output-section,
   consumed-artifact), exposes a palette (`canvas-palette.tsx`,
   `drop-picker.tsx`), and serializes the graph back to a snapshot via
   `buildSnapshotFromGraph` (line 3272). The snapshot is then submitted through
   `save_agent_definition` / `update_agent_definition`
   (`client/src-tauri/src/commands/agent_definition.rs:97-141`), which still
   route through the runtime tool with `operator_approved = true`.

Both paths converge on `validate_definition_snapshot_with_registry`
(`client/src-tauri/src/runtime/autonomous_tool_runtime/agent_definition.rs:1982`)
and `project_store::insert_agent_definition`. **Validation is the source of truth
for what is allowed,** not the UI.

### 1.2 Built-in vs. custom

Built-in agents are not stored as definitions; they are Rust descriptors:
`Ask`, `Plan`, `Engineer`, `Debug`, `Crawl`, `AgentCreate`
(`client/src-tauri/src/commands/contracts/runtime.rs:99-106` and
`builtin_runtime_agent_descriptors`). Each maps to a fixed
`base_capability_profile` (observe_only, planning, engineering, debugging,
repository_recon, agent_builder), prompt policy, tool policy, and output
contract. The settings dialog's "Built-in" group (`agents-section.tsx:335`) is
read-only because of this — there is nothing in the DB to mutate.

### 1.3 Definition lifecycle

`AgentDefinitionLifecycleStateDto`: `draft`, `valid`, `active`, `archived`,
`blocked`. Save sets `lifecycleState = active`
(`agent_definition.rs:391`). Updates increment `version` and persist a
`record_agent_definition_custom_audit_event`. Versions are immutable: the row
table is `agent_definitions` (current pointer) plus per-version snapshots
queried by `load_agent_definition_version`
(`commands/agent_definition.rs:60-73`). Diff is server-rendered via
`load_agent_definition_version_diff`.

---

## 2. What agents can do (capability surface)

### 2.1 Tools

Tool catalog is centrally registered in `runtime::autonomous_tool_runtime` and
`tool_descriptors.rs`. Effect classes
(`mod.rs:622-659`): `observe`, `runtime_state`, `write`, `destructive_write`,
`command`, `process_control`, `browser_control`, `device_control`,
`external_service`, `skill_runtime`, `agent_delegation`, `unknown`.

Tools currently registered (non-exhaustive): `read`, `write`, `edit`, `patch`,
`delete`, `mkdir`, `rename`, `find`, `list`, `search`, `hash`,
`workspace_index`, `git_status`, `git_diff`, `command`, `command_run`,
`command_session_*`, `command_probe`, `command_verify`, `process_manager`,
`browser`, `browser_observe`, `browser_control`, `emulator`,
`macos_automation`, `system_diagnostics`, `system_diagnostics_observe`,
`system_diagnostics_privileged`, `mcp` (+ list/call/get-prompt/read-resource),
`subagent`, `skill`, `tool_search`, `tool_access`, `todo`, `harness_runner`,
`web_search`, `web_fetch`, `notebook_edit`, `agent_definition`, `agent_coordination`,
`environment_context`, `code_intel`, `lsp`, `project_context_*` (read/get/search/
record/update/refresh), and a long Solana group
(`AUTONOMOUS_TOOL_SOLANA_*`). Plus dynamic MCP tools and the entire skill
catalog (each skill becomes a callable tool when `skill_runtime` is allowed).

### 2.2 Tool policy resolution

`AutonomousAgentToolPolicy::from_definition_snapshot`
(`mod.rs:717`) reads `toolPolicy` from the snapshot. It can be:

- A string preset: `engineering`, `agent_builder`, `repository_recon`,
  `planning`, `observe_only` (`from_policy_label`, lines 759–870).
- A structured object: `allowedEffectClasses`, `allowedTools`, `deniedTools`,
  `allowedToolPacks`, `deniedToolPacks`, `allowedToolGroups`,
  `externalServiceAllowed`, `browserControlAllowed`, `skillRuntimeAllowed`,
  `subagentAllowed`, `allowedSubagentRoles`, `deniedSubagentRoles`,
  `commandAllowed`, `destructiveWriteAllowed`.

Tool groups are registered in `TOOL_ACCESS_GROUP_DEFINITIONS`
(referenced via `tool_access_group_descriptors`, lines 553–567). Tool packs are
domain-defined in the `xero_agent_core` crate
(`tool_packs.rs`) and resolved with
`xero_agent_core::domain_tool_pack_tools(pack_id)`.

### 2.3 Workflow contract (state machine)

`AutonomousAgentWorkflowPolicy::from_definition_snapshot`
(`mod.rs:1104`) parses an optional `workflowStructure` object on the snapshot:
phases with `id`, `title`, `allowedTools`, `requiredChecks`, `retryLimit`,
`branches`, plus `startPhaseId`. Conditions are `always`, `todo_completed`,
`tool_succeeded`. This is a *real* state machine that gates tool exposure
per-phase at runtime — Xero already supports authored workflows on top of base
profiles. (Validation in `agent_definition.rs:3047-3345`.)

### 2.4 Customization knobs that exist on the snapshot

- Identity: `id`, `displayName`, `shortLabel`, `description`, `taskPurpose`,
  `scope`, `lifecycleState`, `baseCapabilityProfile`.
- Approval posture: `defaultApprovalMode`, `allowedApprovalModes`.
- Prompts: `prompts[]` (system / developer / task roles, custom or built-in
  source).
- Tools: `tools[]` (catalog snapshot) plus `toolPolicy` (above).
- Workflow: `workflowContract` (text), `workflowStructure` (state machine),
  `finalResponseContract`.
- Output: `output.contract`, sections (`id`, `label`, `description`,
  `emphasis`, `producedByTools`).
- Database touchpoints: `dbTouchpoints` (`reads`, `writes`, `encouraged` with
  table, kind, purpose, triggers, columns).
- Consumed artifacts: `consumes[]` (upstream contracts: plan_pack, etc.).
- Project data: `projectDataPolicy` (`recordKinds`, `structuredSchemas`).
- Memory: `memoryCandidatePolicy` (`memoryKinds`, `reviewRequired`).
- Retrieval: `retrievalDefaults` (`enabled`, `recordKinds`, `memoryKinds`,
  `limit`).
- Handoff: `handoffPolicy` (`enabled`, `preserveDefinitionVersion`).
- UX hints: `examplePrompts[]`, `refusalEscalationCases[]`.
- Skills: `attachedSkills[]` (always-injected lower-priority context, not
  callable — see `agent_definition.rs:618-624`).

### 2.5 Runtime selection of effective state

Per-run selection happens in `run_owned_agent_task`
(`runtime/agent_core/run.rs:25-200`):

1. `resolve_agent_definition_for_run` picks the definition for the run (custom
   if requested, otherwise default for the runtime agent).
2. Definition version is **pinned** onto the run record
   (`agent_definition_version`, line 64).
3. `effective_agent_tool_policy` intersects definition's tool policy with the
   tool runtime's host availability.
4. `ToolRegistry::for_prompt_with_options` produces the per-turn registry.
5. `assemble_system_prompt_for_session_with_attached` adds an
   `agent_definition_policy_fragment` *as a low-priority fragment* that the
   instruction-hierarchy explicitly allows tool/system policy to override
   (`tool_descriptors.rs:756`).
6. `AutonomousAgentWorkflowPolicy::from_definition_snapshot` is loaded into
   `state_machine` on every drive turn so phase gating is enforced.

### 2.6 Prompt assembly

`PromptCompiler` (`tool_descriptors.rs:101-360`) composes a deterministic set of
fragments with priorities:

- 1000: `xero.system_policy` (built-in agent contract).
- 990: `xero.runtime_metadata`.
- 975: `xero.soul` (selected soul settings).
- 900: `xero.tool_policy` (active tool contract).
- (variable): `xero.agent_definition_policy` (custom agent text, lower
  priority).
- 260: `project.workspace_manifest`.
- 245: `xero.working_set_context`.
- 240: `xero.durable_context_tools`.
- 230: `xero.active_coordination`.
- (skill contexts inserted separately).

Budget enforcement, summarization, and inclusion reasons are tracked per
fragment so a given prompt is auditable.

---

## 3. Backend reporting / inspection contracts already shipped

These exist as Tauri commands but **have no UI in the app yet**:

| Contract | Tauri command | Schema |
| --- | --- | --- |
| Run-start explanation | `get_agent_run_start_explanation` | `xero.agent_run_start_explanation.v1` |
| Pre-run knowledge | `get_agent_knowledge_inspection` | `xero.agent_knowledge_inspection.v1` |
| Effective custom-agent preview | `preview_agent_definition` | `xero.agent_definition_preview_command.v1` |
| Saved version diff | `get_agent_definition_version_diff` | `xero.agent_definition_version_diff.v1` |
| Database touchpoints | `get_agent_database_touchpoint_explanation` | `xero.agent_database_touchpoint_explanation.v1` |
| Capability permission | `get_capability_permission_explanation` | (per-subject) |
| Handoff context summary | `get_agent_handoff_context_summary` | `xero.agent_handoff_context_summary.v1` |
| Tool-pack catalog | `get_agent_tool_pack_catalog` | (manifest-shaped) |
| Tool extension manifest validation | `validate_agent_tool_extension_manifest` | `xero.agent_tool_extension_manifest_validation.v1` |
| Authoring catalog | `get_agent_authoring_catalog` | (skills + DB tables + upstream) |
| Attachable skills | `search_agent_authoring_skills`, `resolve_agent_authoring_skill` | — |
| Support diagnostics | `get_agent_support_diagnostics_bundle` | `xero.agent_support_diagnostics_bundle.v1` |
| Memory review queue | `get_session_memory_review_queue`, `update/correct/delete_session_memory` | — |
| Project state backup/restore/repair | `create/restore/repair_project_state*` | — |

`docs/agent-system-release-checklist.md` calls these "backend evidence gates
while UI is deferred." `docs/agent-system-dogfood-notes.md` lists S04, S07,
S08, S09, S10, S11, S12, S13, S15, S20, S21, S25, S28, S43, S46, S52, S61,
S62, S63, S64, S65, S66, and S70 as still release-blocking unless their UI is
implemented or product explicitly waives.

---

## 4. Findings — what is good, weak, or missing

### 4.1 Strengths

- **Schema-first.** A canonical snapshot drives prompt assembly, tool gating,
  workflow phases, retrieval, memory, handoff, and DB touchpoints. There is
  one source of truth and it is versioned with audit events.
- **Pinned-version runtime.** Runs record `definition_id + version`, so a
  definition update can never silently change a live run's behavior.
- **Fail-closed validation.** Invalid snapshots are rejected before save and
  before runtime activation. Diagnostics include `repair_hint` codes the UI
  can render structurally rather than as free text.
- **Real workflow state machine.** The `workflowStructure` runtime gating goes
  beyond what most "custom agent" features in competing tools provide.
- **Two authoring paths converge on the same contract.** No drift risk between
  the Agent Create chat experience and the canvas builder.
- **Capability ceilings.** Subagents intersect with parent policy (`mod.rs:679`),
  attached skills are explicitly *not* callable tools, prompt-injection phrases
  are blocked at definition save (`agent_definition.rs:46-72`).

### 4.2 Weaknesses & gaps

The numbers in parentheses are slice IDs from the existing release checklist
where applicable; the rest are issues I observed reading the code.

1. **Runtime intelligence has no surface.** A user can run an agent but cannot
   ask "what does this agent know about my project?" or "what tools does it
   actually have?" or "what changed between version 7 and 8?" without a
   developer console. (S08, S15, S52, S64, S66.)
2. **`agents-section.tsx` is the only definition-management UI**, and it can
   only **list, archive, and view recent versions**. It cannot edit, preview,
   diff, clone, or import definitions. There's no way to trigger a Save from
   anywhere outside an agent-create chat or the (existing) canvas authoring
   mode launched via `onCreateAgentByHand`.
3. **Discoverability of authored capabilities is weak.** The agent dock and
   composer expose runtime agent IDs but custom agents don't surface
   profile-aware availability, validation status, archive state, or "what
   this can do" hints in the picker. (S07.)
4. **No memory review surface.** Memory candidates are extracted on completion,
   pause, failure, and handoff (per `docs/agent-runtime-continuity.md`) but
   they sit waiting indefinitely because there is no review queue UI. The
   contract `get_session_memory_review_queue` is wired but unused on the
   client. (S28, S65.)
5. **No handoff visibility beyond a single notice.** The runtime persists rich
   handoff bundles (`xero.agent_handoff.bundle.v1`); the UI shows only a "Run
   continued in a fresh session" line. The user cannot see what carried over,
   what was redacted, or jump back to the source run. (S46, S64.)
6. **Effective-runtime preview is only available pre-save.** `preview_agent_definition`
   is called during save flows but the snapshot it produces (compiled prompt
   SHA, fragments, budget tokens, effective tool list, capability explanations,
   workflow phase admittance, attached-skill resolution) is not viewable for
   *active* definitions. Engineers debugging behavior have to rebuild this
   manually.
7. **Tool packs and tool extensions are headless.** `get_agent_tool_pack_catalog`
   and `validate_agent_tool_extension_manifest` exist but no UI lets a user
   browse packs or load an extension manifest. (S20, S21.)
8. **Built-in agents are immutable but not extensible by overlay.** A user
   who wants Engineer + a tighter command policy must clone and rename it;
   they cannot ship "Engineer with my safety overlay" without a fresh
   definition. There is no inheritance / overlay mechanism.
9. **Workflow editor doesn't exist on the canvas.** `workflowStructure` is a
   first-class snapshot field with full validation and runtime gating, but
   the canvas surface has no nodes/edges for "phase", "branch", or
   "required check". The only way to author it is to type JSON via Agent
   Create. (S62, S63 area.)
10. **No definition templates.** Each new custom agent starts from scratch
    in the chat or with the canvas's blank header. There is no "duplicate from
    Plan with these tools removed" flow despite `Clone` being supported by
    the runtime tool. (S11–S13 area.)
11. **`AGENT_SYSTEM_IMPROVEMENT_PLAN.md` is referenced from multiple docs but
    does not exist** at the repo root. The release checklist and dogfood notes
    treat it as the source of truth for slice IDs; readers cannot cross-check
    coverage. This is a real bug — `grep -l AGENT_SYSTEM_IMPROVEMENT_PLAN`
    against the working tree returns only the docs that *reference* it.
12. **Definition snapshot validation is one giant function**
    (`agent_definition.rs:1982-2070+`). It mixes schema shape checks,
    capability-profile rules, tool-policy normalization, workflow validation,
    and project-store registry checks. As more profiles or capability
    families ship, this will be hard to evolve without regression. The
    `validate_workflow_*` helpers are already split out — the rest should
    follow.
13. **Skills are exposed two ways with subtly different semantics.** A skill
    can be (a) attached to a definition (always-injected lower-priority
    context, never callable) or (b) exposed as a callable tool when
    `skillRuntimeAllowed = true`. There is no UI cue to explain the
    difference, and the validation diagnostic catalog
    (`AGENT_ATTACHED_SKILL_INJECTION_PREVIEW_SCHEMA`) does not feed the UI
    yet. Users can author broken combinations.
14. **Per-run telemetry surface has no consumer.** Run-start explanation,
    knowledge inspection, capability explanations, and DB-touchpoint
    explanation all return JSON with `uiDeferred: true`. They are tested but
    unviewable in the product.
15. **No "test this agent" path.** Custom-agent simulation harness exists in
    `agent_core::evals` but is dev-only. There is no way for a user to dry-run
    a saved definition against a fixture project to see how its tool policy,
    output contract, and prompt resolve before pointing it at real work.
16. **MCP servers and dynamic tools are global.** A custom agent cannot scope
    or deny specific MCP servers or specific dynamic tools — the policy only
    talks about generic effect classes and the static catalog. As MCP grows,
    this becomes a blast-radius risk.
17. **Operator approval is binary at definition save.** There is no "approve
    only the changed slices since v7" review surface. Diffs exist on the
    backend (`get_agent_definition_version_diff`) but the approval prompt is
    a single yes/no.

---

## 5. Improvement plan

Plan is grouped into milestones. Each item carries:
- **Outcome:** what changes for a user.
- **Code anchors:** files / functions to touch.
- **Test:** how we will know it works (preferring unit + Vitest + scoped
  integration over manual UI testing, per repo CLAUDE.md).
- **Risk / dependencies:** what to be careful of.

Where backend contracts already exist, the milestone is mostly UI; that is
called out explicitly so we don't re-implement validation or schema work.

### Milestone A — Make the existing backend visible

These four ship the largest user-perceived improvement at the lowest cost
because the backend is already done.

#### A1. Effective-Runtime panel for active agents
- **Outcome:** From the agent picker (or a "details" affordance on the canvas
  header), the user can open a panel showing the compiled prompt summary,
  prompt budget, fragment list, effective tool access (allowed/denied with
  reasons), capability explanations, workflow phases, attached-skill
  resolution status, and `validation` diagnostics — for *both* the saved
  active version and any unsaved canvas edit.
- **Code anchors:** new `client/components/xero/workflow-canvas/effective-runtime-panel.tsx`,
  consume `preview_agent_definition` (already exposed in
  `xero-desktop.ts`). Run preview against the active saved version on demand
  by re-submitting the stored snapshot. Mount inside the existing canvas
  details panel (`workflow-canvas/node-details-panel.tsx`).
- **Test:** Vitest snapshot test with seeded preview JSON; unit tests for
  diagnostic-to-row mapping; existing
  `client/src/lib/xero-model/agent-definition.test.ts` already covers the
  contract.
- **Risk:** Avoid building a generic JSON viewer. Render the contract
  explicitly so future schema fields surface as gaps instead of silently
  hiding.

#### A2. Saved-version diff view
- **Outcome:** In the existing version history panel
  (`agents-section.tsx:528`), clicking a version pair opens a diff that
  groups changes into the same buckets the validator uses (prompt, tool
  policy, output contract, memory, retrieval, handoff, workflow, db
  touchpoints, attached skills, safety limits).
- **Code anchors:** wire `getAgentDefinitionVersionDiff`, render through a
  new `version-diff-section.tsx`. The diff already classifies sections —
  consume those keys directly rather than rolling our own JSON diff.
- **Test:** Vitest with a hand-built diff payload covering each section
  type.

#### A3. Knowledge inspection for the active session
- **Outcome:** A "What this agent can see right now" link on the agent
  composer that opens a panel listing project records, approved memory,
  handoff records, and continuity records currently visible to the active
  run, scoped via `runId` so retrieval-policy filters are applied. Numbers
  match what would be retrieved on the next turn.
- **Code anchors:** `getAgentKnowledgeInspection` is already in the desktop
  adapter contract; add it as a TS adapter method (currently only the Rust
  command exists) and a `knowledge-inspection-panel.tsx` consumed from
  `agent-runtime/conversation-section.tsx`.
- **Test:** Vitest UI tests with a fake adapter; existing backend test in
  `client/src-tauri/tests/agent_context_continuity.rs`.

#### A4. Handoff context surface
- **Outcome:** When a same-type handoff completes, the existing notice
  becomes clickable and opens a dialog summarizing the handoff bundle
  (carried context, omitted, redacted, lineage, source run id, target run
  id, definition pin). Replaces the "you'll have to trust us" UX.
- **Code anchors:** consume `getAgentHandoffContextSummary` from a new
  `handoff-context-dialog.tsx` mounted from the existing notice in
  `agent-runtime.tsx`. Add an adapter method.
- **Test:** Vitest renders the dialog from a fixture summary; backend
  contract already covered by integration tests.

### Milestone B — Close the visual authoring gap

Items that make canvas authoring complete enough that no one needs to drop
back to JSON. This honors the canvas-first preference recorded in user memory.

#### B1. Workflow phase nodes on the canvas
- **Outcome:** The canvas gains a new node kind (`workflow-phase`) plus a
  `phase-branch` edge type so a user can lay out the same `workflowStructure`
  the runtime already enforces. Required checks become small badges; allowed
  tools rendered as chips that overlap with the existing tool nodes.
- **Code anchors:** extend `AgentGraphNodeKind`
  (`workflow-canvas/build-agent-graph.ts:21`), add a node renderer in
  `workflow-canvas/nodes/`, extend `buildSnapshotFromGraph` to emit
  `workflowStructure`, and bind into validator output paths
  (`workflowStructure.phases[i]....`) for inline diagnostics.
- **Test:** Unit tests for snapshot round-trip (`build-snapshot.test.ts`
  already exists), Vitest for node rendering, integration test verifying a
  canvas-built workflow gates tools at runtime
  (`agent_context_continuity.rs` style).
- **Risk:** Keep workflow nodes optional — a definition with no
  `workflowStructure` keeps current behavior.

#### B2. Granular policy editor
- **Outcome:** The header node's "advanced" panel
  (`workflow-canvas/node-properties-panel.tsx:43-119`) becomes a structured
  policy editor: per-tool allow/deny toggles backed by the catalog, effect
  class checkboxes, tool-pack picker, subagent role picker, with inline
  diagnostics for `agent_definition_tool_denied_*` codes.
- **Code anchors:** `node-properties-panel.tsx`, consume
  `getAgentToolPackCatalog`. Reuse existing CAPABILITY_FLAGS list.
- **Test:** Vitest covering pack expansion (one `allowedToolPacks` entry
  resolves to many tools) and conflict detection.
- **Risk:** Don't re-implement runtime resolution in TS — call the existing
  `previewAgentDefinition` to compute effective access on each edit.

#### B3. Templates and clone-with-overlay
- **Outcome:** "New agent from template" picker seeded by built-in
  descriptors and any saved custom agents. Selecting one loads the snapshot
  into the canvas pre-filled, leaving only edits the user wants. Backed by
  the existing `Clone` action.
- **Code anchors:** new `agent-create-draft-section.tsx` companion that
  invokes `agent_definition` with `Clone`, plus a snapshot loader for
  built-ins (synthesize from `RuntimeAgentDescriptorDto`).
- **Test:** Vitest for picker rendering and adapter wiring; integration test
  ensuring clone + edit yields a valid v1 definition.

#### B4. Profile-aware authoring catalog
- **Outcome:** The drop picker
  (`workflow-canvas/drop-picker.tsx`) hides tools the chosen base capability
  profile cannot use (e.g. `command_run` greyed for `observe_only`) and
  explains why. Currently it shows everything.
- **Code anchors:** consume `getAgentAuthoringCatalog` (existing) plus the
  profile-aware filter from
  `effective_tool_access_preview`'s reasons. Add a profile filter to
  `CatalogPicker`.
- **Test:** Vitest with profile fixtures; covers per-profile entry counts.

### Milestone C — Make it safer to evolve

#### C1. Per-version operator approval review
- **Outcome:** The Save action surfaces a structured diff against the prior
  version (sections changed, effects added/removed) before the user clicks
  approve. Prevents accidental capability widening on update.
- **Code anchors:** Combine A2 (diff view) with the existing
  `agent_definition` runtime tool's `approval_required_output` path. Replace
  the bare yes/no with a "what changes" preview.
- **Test:** Vitest for diff rendering, plus a backend test confirming Update
  with no changes still requires approval but flags zero-delta.

#### C2. Definition validation refactor
- **Outcome:** `validate_definition_snapshot_with_registry`
  (`agent_definition.rs:1982`) split into one validator per concern
  (identity, profile, tool policy, output contract, db touchpoints, memory
  policy, retrieval defaults, handoff policy, workflow structure, attached
  skills, safety limits). Each validator owns its diagnostic codes.
- **Code anchors:** new module
  `runtime/autonomous_tool_runtime/agent_definition/validators/{identity,
  profile, tool_policy, output, db_touchpoints, memory, retrieval, handoff,
  workflow, attached_skills}.rs`. Re-export through the existing entry
  point.
- **Test:** Existing tests stay green; add per-validator unit tests so each
  diagnostic code is exercised in isolation.

#### C3. Built-in overlay support
- **Outcome:** Allow a custom definition to declare `extends:
  "engineer@1"` and inherit the built-in's prompt + tool policy, with the
  custom snapshot acting as overlay (additive: prompts, attachedSkills,
  consumes; restrictive: toolPolicy, memoryCandidatePolicy,
  retrievalDefaults; overrideable: workflowContract, finalResponseContract,
  output, dbTouchpoints, handoffPolicy).
- **Code anchors:** new `extends` field in `xero.agent_definition.v1`,
  resolution in `resolve_agent_definition_for_run`. Validator must reject
  cycles and capability widening (overlay can only narrow tool policy).
- **Test:** Integration tests proving overlay narrows but never widens; tests
  that the resolved snapshot pins to base-version + overlay-version pair.
- **Risk:** This is a contract change — keep behind a `schemaVersion = 3`
  flag and reject mixed v2/v3 definitions cleanly. Per CLAUDE.md, no
  backwards-compat unless asked: a clean cutover is fine.

#### C4. MCP / dynamic tool policy
- **Outcome:** Definitions can list `allowedMcpServers` /
  `deniedMcpServers` and `allowedDynamicTools` /
  `deniedDynamicTools`. The runtime registry
  (`ToolRegistry::for_prompt_with_options`) honors them.
- **Code anchors:** extend `AutonomousAgentToolPolicy` (`mod.rs:662`), wire
  through `tool_dispatch.rs`. Add validation diagnostics for unknown server
  ids.
- **Test:** Integration test in `agent_core_runtime.rs` style.

### Milestone D — Memory & retrieval surfaces

#### D1. Memory review queue UI
- **Outcome:** A panel listing pending memory candidates (per session and
  per project) with approve/edit/reject/correct/delete actions. Clears the
  backlog from `extract_session_memory_candidates`.
- **Code anchors:** consume the existing `get_session_memory_review_queue`,
  `update_session_memory`, `correct_session_memory`,
  `delete_session_memory`. New section under settings or session
  conversation.
- **Test:** Vitest for the queue grid; redaction-preserving display
  verified with a fixture containing secret-shaped memory.

#### D2. Project record correction
- **Outcome:** From the workspace index / project records views, allow
  delete and supersede operations. Backend exists
  (`delete_project_context_record`, `supersede_project_context_record`).
- **Code anchors:** new toolbar in the existing project-records surface.
- **Test:** Vitest covering supersede chain rendering.

#### D3. Backup / restore / repair controls
- **Outcome:** Settings → Project state shows current backup state and lets
  the user create / restore / repair. Backend already wired
  (`commands::project_state::*`).
- **Code anchors:** new `settings-dialog/project-state-section.tsx`.
- **Test:** Vitest with mock adapter; backend already covered.

### Milestone E — Test & dry-run

#### E1. Definition simulation runner
- **Outcome:** "Test this agent" button runs `run_custom_agent_simulation_harness`
  (`runtime/agent_core/evals.rs`) against a fixture project, returns prompt
  resolution + tool exposure plan + a synthetic transcript without contacting
  a provider.
- **Code anchors:** expose a new Tauri command wrapping the existing eval
  harness; UI panel renders the result.
- **Test:** Backend command test; UI snapshot.
- **Risk:** Make sure the harness runs offline; we already have
  `Fake` provider configs for this.

#### E2. Tool extension manifest browser
- **Outcome:** A small dev surface that loads a manifest JSON, calls
  `validate_agent_tool_extension_manifest`, and shows descriptor + permission
  summary + fixture list. Unblocks third-party tool adoption.
- **Code anchors:** new `settings-dialog/tool-extensions-section.tsx`.
- **Test:** Vitest using the same fixture from
  `agent_extensions.rs:tests::valid_manifest`.

### Milestone F — Documentation & state hygiene

#### F1. Restore the missing `AGENT_SYSTEM_IMPROVEMENT_PLAN.md`
- **Outcome:** Either re-add the file at the repo root with the actual slice
  ids/audit, or update `docs/agent-system-release-checklist.md` and
  `docs/agent-system-dogfood-notes.md` to point at this audit instead.
  Right now both docs reference a non-existent file. **Pick one before any
  other Milestone F work.**
- **Test:** Doc lint that flags broken intra-repo refs.

#### F2. Document the agent definition schema
- **Outcome:** A reference doc that enumerates every field of
  `xero.agent_definition.v1`, the validator codes, the runtime effects, and
  the failure-closed behaviors. Currently this only exists implicitly across
  multiple Rust files.
- **Code anchors:** new `docs/agent-definition-schema.md`. Source from
  `validate_definition_snapshot_with_registry` and
  `effective_runtime_preview`.
- **Test:** Doc compares schema field list against the validator (script).

#### F3. Auto-generated capability matrix
- **Outcome:** A markdown table generated at build/test time from the Rust
  catalog (`tool_access_group_descriptors`,
  `tool_catalog_metadata_for_tool`, `tool_effect_class`) showing which
  built-in agent or capability profile can run which tool, with effect
  class. Drives both docs and the UI's "what does this agent do" copy.
- **Code anchors:** new bin in `client/src-tauri/src/bin/` (or a
  `cargo test` golden file), output committed to `docs/`.
- **Test:** Snapshot drift test fails if the matrix and the registry diverge.

---

## 6. Sequencing recommendation

Ship in this order — each milestone is independently valuable and unblocks
the next:

1. **F1 first** (10 min, removes a real correctness bug in the docs).
2. **A1 + A2** (visible Effective Runtime + diff). Highest ratio of user value
   to engineering cost: backend already done.
3. **A4** (handoff dialog). Same reasoning.
4. **A3** (knowledge inspection).
5. **B4 + B2** (catalog filtering + granular policy editor). Hardens the
   existing canvas-first authoring path.
6. **D1** (memory review). Stops the candidate backlog from growing
   indefinitely.
7. **B1** (workflow phase nodes on canvas). Largest authoring win, but
   requires snapshot-builder changes and node renderers.
8. **C1** (per-version approval review). Falls out of A2.
9. **B3** (templates / clone-with-overlay), **D2 + D3** (record correction,
   backup), **E1 + E2** (simulation, extensions).
10. **C2** (validator refactor) before **C3** (built-in overlay) and
    **C4** (MCP scoping). C3/C4 are contract changes — do them after the
    refactor is in.
11. **F2 + F3** (schema + capability docs) once the contract has settled.

## 7. Out of scope (intentionally)

- Provider routing / pricing changes — covered by `provider-setup-and-diagnostics.md`.
- Session memory mechanics — covered by `session-memory-and-context.md` and
  already shipped.
- Browser / emulator / Solana tool capabilities — they are tool-surface
  concerns, not agent-system concerns.
- Multi-tenant or remote agent execution — Xero is single-user desktop.

## 8. Decisions to confirm before writing code

1. Schema bump: should built-in overlay (C3) ship as `schemaVersion = 3` with
   v2 rejected, or should it be additive on v2? CLAUDE.md says no backwards
   compat unless asked.
2. Surface for A1 and B-series panels: extend the existing canvas details
   panel, or open a side sheet? The canvas-first feedback memory says extend
   the canvas; pick one to keep the UI coherent.
3. Whether to keep `agent_create` as a built-in agent at all once visual
   authoring covers everything. (My recommendation: keep it — chat is faster
   for "draft me a planning agent that…" than building from scratch on the
   canvas.)
