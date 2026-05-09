# Agent System Improvement Plan

This plan is grounded in the current implementation of agent construction, persistence, visual authoring, runtime execution, retrieval, memory, handoff, SQLite state, and LanceDB state.

It intentionally describes most work in terms of system behavior instead of file names. File paths can change; the behavioral contract should not.

## Investigation Summary

### Direct answers

- The system is not yet as good as it can be. It has a strong foundation, but the visual builder is not currently a faithful source of truth for every node and setting it lets the user author.
- The user cannot yet build whatever agent they want. Custom agents are configurable profiles over fixed runtime agent types, fixed tool gates, fixed built-in catalog concepts, and prompt/policy fragments. That is safer than arbitrary execution, but it is still constrained.
- The visual builder is more constrained than it looks. It can emit custom tools, outputs, database touchpoints, and consumed artifacts, but the backend currently normalizes only a narrower canonical definition. When a custom visual agent is saved and reloaded, several visual choices can be replaced by defaults from the selected runtime profile.
- SQLite is a reasonable local transactional foundation. It already uses versioned agent definitions, run pinning, foreign keys, WAL, and migration infrastructure. It still needs production hardening around project database pragmas, recovery, backup, reconciliation, integrity checks, and operational diagnostics.
- LanceDB is a promising local retrieval foundation, but the current usage is not production-grade for scale or durable memory trust. Retrieval uses deterministic local hash embeddings, full-table scans, Rust-side ranking, schema reset behavior, and non-atomic coordination with SQLite.
- The context and handoff architecture is stronger than a prototype. Runs persist context manifests before provider calls, handoff lineage is modeled, same-type handoff preserves runtime identity and pinned custom definitions, and tests cover crash recovery. It still needs stronger completeness contracts, user-visible inspection, cross-store reconciliation, and first-turn context quality.
- Agents do not fully "know what is going on right away" yet. Durable context is primarily tool-mediated, not automatically injected as raw prompt text. That is good for control and security, but it means the model must choose to call the context tool unless a concise working-set summary is admitted into the initial context.
- Memory review is not sufficiently user-facing. The backend has candidate extraction and approval paths, but the selected agent runtime UI intentionally hides memory management today. Production memory needs a visible review, provenance, correction, and deletion workflow.

### Current strengths to keep

- Agent definitions are versioned and runs pin the exact definition version they started with.
- Custom definitions cannot silently expand beyond fixed runtime safety gates.
- Operator approval exists for saving, updating, archiving, and cloning custom definitions through the runtime tool path.
- The runtime computes effective tool access by intersecting the custom definition policy with the fixed runtime policy.
- Provider turns persist context manifests before model submission.
- Durable context is delivered through a controlled tool path instead of raw uncontrolled prompt injection.
- Handoff keeps source and target lineage, preserves runtime type, and has idempotency protection.
- Existing tests already cover important retrieval, manifest, handoff, and schema behaviors.

### Highest-risk gaps

- The visual builder can lose user-authored graph data on save and reload.
- Custom agent detail hydration can show runtime defaults instead of the saved custom graph.
- Granular custom tool policy does not round-trip cleanly into the visual editor.
- Output contracts, database touchpoints, and consumed artifacts are not yet fully honored as custom runtime behavior.
- Retrieval is not true semantic search yet and does not use LanceDB vector indexes or filter pushdown as the primary query path.
- LanceDB schema drift handling can be destructive.
- SQLite and LanceDB writes are not coordinated by a robust outbox or reconciliation loop.
- The first model turn in a new or handed-off session may not receive enough concise, source-cited working context.
- The user lacks a complete inspectable view of what an agent will actually run with: prompt, tools, memory policy, retrieval policy, output contract, handoff policy, and risky capabilities.

## Completion Rules

Every slice below starts unchecked. A slice may only be marked complete when implementation and verification evidence have both been added.

For every completed slice, record:

- The user-visible behavior that changed.
- The runtime or storage contract that changed.
- The scoped tests or checks that prove the behavior.
- Any migration, repair, or rollout consequence.

General execution rules:

- Use scoped tests and scoped formatting.
- Run only one Cargo command at a time.
- Do not create branches or stashes unless explicitly requested.
- Do not add temporary debug or test UI.
- Use app-data storage for new project state, not legacy repo-local state.
- Do not add backwards compatibility unless explicitly requested; this is a new application.
- Prefer ShadCN components for user-facing UI.
- Keep risky agent capabilities explicit, auditable, and user-confirmed.

## Milestone 1 - Canonical Agent Definition Contract

Goal: make saved custom agents a durable, versioned, lossless source of truth for what the user authored.

- [ ] S01 - Define one canonical custom-agent contract shared by visual authoring, validation, persistence, runtime loading, and detail hydration.
  - Depends: none.
  - Implement: document and enforce the complete shape of a custom agent, including identity, runtime profile, prompts, workflow contract, final response contract, tool policy, output contract, database touchpoints, consumed artifacts, memory policy, retrieval policy, handoff policy, examples, escalation cases, capability flags, and safety limits.
  - Evidence required: schema validation tests prove accepted and rejected examples; generated or inferred types match across frontend and backend boundaries.

- [ ] S02 - Preserve every visual graph field durably.
  - Depends: S01.
  - Implement: ensure saved custom agents retain authored tools, tool groups, output sections, output contract text, read/write/encouraged database touchpoints, consumed artifacts, advanced capability flags, and future policy fields.
  - Evidence required: save, reload, edit, and save again without losing or adding graph nodes or policy flags.

- [ ] S03 - Hydrate custom agent detail from the saved definition, not from runtime-profile defaults.
  - Depends: S01, S02.
  - Implement: custom agent detail should display the saved custom graph first, using runtime defaults only as explicit fallbacks for new drafts.
  - Evidence required: a custom agent with a narrow tool set, custom outputs, custom touchpoints, and custom consumed artifacts reloads exactly as authored.

- [ ] S04 - Make granular tool policy round-trip through authoring.
  - Depends: S01, S02, S03.
  - Implement: granular allowed tools, denied tools, allowed packs, denied packs, allowed groups, effect classes, browser control, external service use, command use, subagent use, skill runtime use, and destructive-write permission must survive save and reload.
  - Evidence required: policy tests cover restrictive, broad, and denied-over-allowed cases; UI tests prove the advanced panel reflects saved policy instead of inferred defaults.

- [ ] S05 - Add version-aware custom-agent validation and upgrade handling.
  - Depends: S01.
  - Implement: every saved custom definition carries an explicit schema version, and unsupported future versions fail with a clear repair path instead of silent partial loading.
  - Evidence required: tests cover current, missing, malformed, and future schema versions.

- [ ] S06 - Add immutable-version acceptance checks for custom agents.
  - Depends: S01, S02.
  - Implement: updating an agent creates a new immutable version and existing runs continue using the pinned version they started with.
  - Evidence required: a run created before an update continues to use the old definition, while a new run uses the new one.

## Milestone 2 - Visual Builder As A Real Agent Workbench

Goal: make the visual builder accurately show what the agent is, what it can do, and what will happen at runtime.

- [ ] S07 - Make the authoring catalog profile-aware.
  - Depends: Milestone 1.
  - Implement: tools, outputs, database touchpoints, upstream artifacts, and capability controls should show whether they are available, unavailable, or require a runtime profile change.
  - Evidence required: the user cannot save a graph that appears valid visually but is rejected later because the selected runtime profile cannot run it.

- [ ] S08 - Add effective-runtime preview before saving.
  - Depends: Milestone 1.
  - Implement: show the compiled effective prompt sections, effective tool access, denied capabilities, output contract, context policy, memory policy, retrieval policy, handoff policy, and risky capability prompts.
  - Evidence required: preview output matches what the runtime actually loads for the next run.

- [ ] S09 - Add first-class memory, retrieval, and handoff controls to the builder.
  - Depends: S01, S08.
  - Implement: let users configure what the agent should remember, what requires review, what to retrieve automatically, what stays tool-mediated, and what must be included during context exhaustion or handoff.
  - Evidence required: saved policies affect runtime context manifests and handoff bundles.

- [ ] S10 - Add clear visual validation for invalid edges and unreachable intent.
  - Depends: Milestone 1.
  - Implement: validation should explain why a graph cannot run, including unavailable tools, impossible output contracts, unsupported database touchpoints, missing prompt intent, missing handoff policy, and risky capabilities without confirmation.
  - Evidence required: UI tests cover each invalid condition and the user-facing message.

- [ ] S11 - Add visual diff for edits to existing agents.
  - Depends: S02, S03.
  - Implement: before saving an update, show what changed in prompts, policy, tools, memory, retrieval, handoff, outputs, and database access.
  - Evidence required: update flow proves the diff is derived from saved versions, not from current UI defaults.

- [ ] S12 - Add authoring affordances for reusable templates and examples.
  - Depends: S01.
  - Implement: let users start from proven templates for engineering, debugging, planning, repository reconnaissance, support triage, and agent-builder tasks without hiding the effective runtime constraints.
  - Evidence required: template-created agents are ordinary custom agents that pass the same save/reload/runtime tests.

- [ ] S13 - Make generated agents from the agent-builder path open as editable graphs.
  - Depends: Milestone 1, S12.
  - Implement: an agent drafted by the agent-builder runtime path should produce the same canonical graph shape that the visual builder edits.
  - Evidence required: draft, validate, save, reload, and edit flows are identical whether the agent began from a visual template or the agent-builder path.

## Milestone 3 - Custom Agent Runtime Fidelity

Goal: make runtime behavior honor the custom graph instead of treating it as mostly descriptive metadata.

- [ ] S14 - Honor custom output contracts at runtime.
  - Depends: Milestone 1.
  - Implement: final response instructions and validation should use the saved output contract, sections, required artifacts, and completion rules for the pinned agent version.
  - Evidence required: provider prompts and completion checks reflect custom output sections without leaking higher-priority policy.

- [ ] S15 - Honor custom database touchpoints as runtime guidance and audit metadata.
  - Depends: Milestone 1.
  - Implement: database read/write/encouraged touchpoints should guide context selection, tool descriptions, audit records, and user-visible capability explanations.
  - Evidence required: manifests and audit events show the saved touchpoints for runs using that custom agent.

- [ ] S16 - Honor consumed artifacts as context and workflow expectations.
  - Depends: Milestone 1.
  - Implement: consumed artifacts should inform what the runtime retrieves, what the prompt expects, and what preflight validation checks before a run begins.
  - Evidence required: runs with missing expected artifacts produce a clear blocked state or user request instead of vague failures.

- [ ] S17 - Add custom-agent preflight before activation.
  - Depends: S08, S14, S15, S16.
  - Implement: validate effective prompt, effective tools, storage access, context policy, output contract, and risky capability confirmations before an agent can be used.
  - Evidence required: preflight catches invalid runtime combinations that static schema validation cannot catch.

- [ ] S18 - Add a simulation harness for custom agents.
  - Depends: S17.
  - Implement: let developers and tests simulate representative tasks without real destructive effects, external services, or temporary UI.
  - Evidence required: simulations prove prompts, tool gates, retrieval policy, memory policy, handoff policy, and output contract work together.

- [ ] S19 - Add explicit custom-agent activation state.
  - Depends: S17, S18.
  - Implement: distinguish draft, valid, active, archived, and blocked definitions. Only active definitions should be offered for normal user selection.
  - Evidence required: blocked or invalid custom agents cannot be selected for production runs.

## Milestone 4 - Agent Capability Expansion Without Losing Safety

Goal: let users build meaningfully new agents, not only narrower variants of existing profiles.

- [ ] S20 - Define a safe extension model for new tools.
  - Depends: Milestone 1, S17.
  - Implement: support adding tool capabilities through explicit manifests with schemas, permissions, effect classes, audit labels, and test fixtures.
  - Evidence required: a new non-built-in tool can be added, permissioned, tested, shown in the builder, and invoked by a custom agent.

- [ ] S21 - Add user-configurable tool packs.
  - Depends: S20.
  - Implement: allow reusable tool packs with explicit allowed effects, denied effects, review requirements, and display metadata.
  - Evidence required: tool packs round-trip through custom agents and runtime policy intersection.

- [ ] S22 - Add controlled workflow structure beyond prompt-only behavior.
  - Depends: S14, S18.
  - Implement: support sequential phases, gates, retry limits, required checks, and conditional branches where the runtime can enforce them instead of merely asking the model to comply.
  - Evidence required: a custom workflow fails closed when a required gate is not satisfied.

- [ ] S23 - Add subagent composition as an explicit capability.
  - Depends: S20, S22.
  - Implement: let an agent delegate to approved child agents with clear task boundaries, inherited constraints, and summarized results.
  - Evidence required: custom agents cannot spawn undeclared child capabilities and delegated results are recorded in manifests.

- [ ] S24 - Add external service and browser-control capability contracts.
  - Depends: S20.
  - Implement: risky capabilities must have named permissions, user-facing explanations, audit records, and revocation.
  - Evidence required: the same tool behaves differently when external services or browser control are not permitted.

- [ ] S25 - Add natural-language-to-graph repair.
  - Depends: S13, S20.
  - Implement: when the user describes an agent that requires unavailable capabilities, generate a graph plus explicit missing capability notes instead of silently narrowing intent.
  - Evidence required: generated plans distinguish supported, partially supported, and unsupported capabilities.

## Milestone 5 - First-Turn Context And Memory That Actually Helps

Goal: make a new or handed-off session start with enough accurate context that the user does not need to restate the problem.

- [ ] S26 - Introduce a source-cited working-set context layer.
  - Depends: current context manifest infrastructure.
  - Implement: admit a concise, current, source-cited working-set summary into the initial context while keeping bulk durable records tool-mediated.
  - Evidence required: manifests distinguish admitted working-set context from tool-mediated durable context and cite source records.

- [ ] S27 - Define per-agent first-turn context policy.
  - Depends: S09, S26.
  - Implement: custom agents can choose what is always summarized, what is only retrievable by tool, and what must never be auto-included.
  - Evidence required: different agents receive different first-turn context packages for the same project state.

- [ ] S28 - Make memory review visible and usable.
  - Depends: current memory candidate infrastructure.
  - Implement: users can review, approve, reject, edit, disable, delete, and inspect provenance for memory candidates in a permanent user-facing surface.
  - Evidence required: approved memory becomes retrievable, rejected memory does not, edited memory records provenance, and deleted memory is removed from retrieval.

- [ ] S29 - Add memory freshness and invalidation rules.
  - Depends: S28.
  - Implement: memory records should become stale when source files, decisions, project facts, or user corrections contradict them.
  - Evidence required: stale memory is deprioritized or blocked and the user can see why.

- [ ] S30 - Add current-problem continuity records.
  - Depends: S26, S28.
  - Implement: record active goal, current task state, blockers, recent decisions, changed files, test evidence, open questions, and next actions as structured continuity records.
  - Evidence required: starting a new session retrieves the current problem without requiring a manual recap.

- [ ] S31 - Add retrieval evaluations for "no re-description needed".
  - Depends: S26, S30.
  - Implement: create repeatable scenarios where a new session must answer what is happening, what changed, what remains, and what evidence exists.
  - Evidence required: evals fail if the agent misses important current context on the first turn.

## Milestone 6 - LanceDB Retrieval And Memory Production Hardening

Goal: make vector memory reliable, scalable, repairable, and semantically useful.

- [ ] S32 - Replace deterministic hash embeddings as the production default.
  - Depends: current embedding abstraction.
  - Implement: support a real embedding model with explicit model identity, dimension, version, provider, migration state, and fallback behavior.
  - Evidence required: records store embedding metadata and retrieval reports which embedding model ranked each result.

- [ ] S33 - Add embedding migration and re-embedding jobs.
  - Depends: S32.
  - Implement: detect stale embedding dimensions or model versions and re-embed records safely without blocking normal app use.
  - Evidence required: mixed-version stores are repaired or queried predictably.

- [ ] S34 - Use LanceDB vector search and filter pushdown for primary retrieval.
  - Depends: S32.
  - Implement: replace full-table scans as the main path with indexed vector search, metadata filters, and bounded result windows.
  - Evidence required: performance tests cover large record counts and prove bounded latency.

- [ ] S35 - Keep deterministic lexical fallback for degraded mode.
  - Depends: S34.
  - Implement: fallback remains available when embeddings are missing, corrupt, disabled, or not yet migrated, but degraded mode is visible in diagnostics.
  - Evidence required: retrieval tests cover both semantic and fallback modes.

- [ ] S36 - Replace destructive schema reset with repair and quarantine.
  - Depends: current LanceDB schema checks.
  - Implement: when schema drift is detected, preserve existing data, quarantine unsupported tables, expose repair diagnostics, and create a clean table without silent loss.
  - Evidence required: malformed or old tables do not disappear without a recoverable copy.

- [ ] S37 - Add LanceDB compaction, index maintenance, and health diagnostics.
  - Depends: S34, S36.
  - Implement: expose durable maintenance operations for index health, table size, stale rows, compaction needs, schema version, and query latency.
  - Evidence required: diagnostics can explain slow or degraded retrieval.

- [ ] S38 - Add trust scoring and contradiction handling.
  - Depends: S29, S32.
  - Implement: retrieval should rank by relevance, freshness, confidence, provenance, user approval, contradiction state, and agent-specific policy.
  - Evidence required: evals prove stale or contradicted records do not outrank current approved facts.

## Milestone 7 - SQLite And Cross-Store Durability

Goal: make project state durable under crashes, partial writes, migrations, and repair.

- [ ] S39 - Harden project database pragmas and connection policy.
  - Depends: current project database initialization.
  - Implement: enforce foreign keys, WAL, busy timeout, synchronous policy, checkpoint behavior, and connection lifecycle consistently for project state.
  - Evidence required: database initialization tests verify expected pragmas for project stores.

- [ ] S40 - Add integrity checks and startup diagnostics.
  - Depends: S39.
  - Implement: detect database corruption, migration mismatch, failed checkpoints, missing project directories, and invalid state before normal runtime use.
  - Evidence required: startup reports actionable diagnostics instead of vague runtime failures.

- [ ] S41 - Add cross-store outbox and reconciliation.
  - Depends: current SQLite and LanceDB write paths.
  - Implement: when an operation must update both SQLite and LanceDB, record intent transactionally, complete side effects idempotently, and reconcile incomplete operations after restart.
  - Evidence required: injected failures between SQLite and LanceDB writes recover without duplicate records or lost records.

- [ ] S42 - Make delete-replace operations safe.
  - Depends: S41.
  - Implement: replacing LanceDB records should not lose the previous record if the replacement insert fails.
  - Evidence required: failure tests prove old data remains available after a failed replacement.

- [ ] S43 - Add backup, restore, and repair flows for project state.
  - Depends: S40, S41.
  - Implement: users can create a project-state backup, restore it, and run repair diagnostics without touching legacy repo-local state.
  - Evidence required: restore tests prove SQLite state, LanceDB records, manifests, memory, and agent definitions remain consistent.

- [ ] S44 - Add storage observability for production support.
  - Depends: S37, S40.
  - Implement: expose state size, migration version, table health, retrieval health, pending outbox count, failed reconciliation count, and last successful maintenance time.
  - Evidence required: support diagnostics can identify whether a problem is SQLite, LanceDB, embedding, retrieval policy, or runtime policy.

## Milestone 8 - Context Exhaustion And Handoff Reliability

Goal: make handoff complete, inspectable, recoverable, and useful on the first message after context exhaustion.

- [ ] S45 - Define a handoff completeness contract for each runtime type.
  - Depends: current handoff bundle infrastructure.
  - Implement: every handoff bundle must include goal, status, completed work, pending work, decisions, constraints, project facts, file changes, tool evidence, verification, risks, questions, memory references, source context hash, and runtime-specific details.
  - Evidence required: tests fail if required bundle fields are missing for ask, planning, engineering, debugging, and custom agents.

- [ ] S46 - Make handoff context visible to the user.
  - Depends: S45.
  - Implement: the user can see what was carried forward, what was omitted, what was redacted, and why the target session is expected to continue safely.
  - Evidence required: UI tests prove the handoff notice links to a readable carried-context summary.

- [ ] S47 - Add handoff bundle quality scoring.
  - Depends: S45.
  - Implement: score handoff bundles for missing evidence, vague next steps, missing verification, unresolved blockers, stale context, and excessive raw transcript dependence.
  - Evidence required: low-quality handoffs block automatic continuation or request clarification.

- [ ] S48 - Reconcile incomplete handoffs across SQLite and LanceDB.
  - Depends: S41, S45.
  - Implement: recover from crashes or failures at every handoff step, including bundle write, lineage update, target run creation, and source run marking.
  - Evidence required: failure-injection tests cover each intermediate state.

- [ ] S49 - Ensure handoff target sessions receive first-turn working context.
  - Depends: S26, S45.
  - Implement: target sessions should start with the handoff bundle, current working-set summary, source-cited continuity records, and the pending user prompt.
  - Evidence required: target manifests prove the needed context was available before the first provider call.

- [ ] S50 - Add handoff comparison diagnostics.
  - Depends: S49.
  - Implement: compare source and target runtime kind, provider, model, thinking settings, approval policy, pinned definition version, tool policy, context policy, and manifest records.
  - Evidence required: diagnostics flag any unexpected drift between source and target sessions.

## Milestone 9 - Security, Trust, And User Control

Goal: keep expanded agent construction safe, inspectable, and revocable.

- [ ] S51 - Strengthen prompt hierarchy checks for custom definitions.
  - Depends: Milestone 1.
  - Implement: custom prompts, examples, escalation cases, memory, project records, and retrieved content must remain lower priority than system and developer policy.
  - Evidence required: injection tests prove custom content cannot override tool gates, approval rules, or redaction rules.

- [ ] S52 - Add capability permission explanations.
  - Depends: S08, S20.
  - Implement: every risky capability should explain what it can do, what data it can touch, whether it can leave the local machine, whether it can mutate files, and whether user confirmation is required.
  - Evidence required: effective-runtime preview and audit logs show the same permission explanation.

- [ ] S53 - Add revocation and emergency disable.
  - Depends: S19, S52.
  - Implement: users can disable a custom agent, tool pack, external integration, browser-control grant, or destructive-write grant without deleting historical runs.
  - Evidence required: disabled capabilities are unavailable to new runs but historical audit records remain readable.

- [ ] S54 - Expand secret and sensitive-data redaction coverage.
  - Depends: S26, S28, S45.
  - Implement: redaction applies consistently to memory extraction, retrieval records, working-set summaries, context manifests, handoff bundles, diagnostics, and exports.
  - Evidence required: tests cover secret-like values across every context surface.

- [ ] S55 - Add audit trails for custom-agent changes and risky actions.
  - Depends: S06, S52.
  - Implement: record who or what changed an agent, what changed, what approval was given, what risky tools were available, and which run used which version.
  - Evidence required: audit export reconstructs the lifecycle of a custom agent and its runs.

## Milestone 10 - Observability, Evaluation, And Regression Coverage

Goal: make agent behavior debuggable without temporary UI and prevent regressions in construction, memory, and handoff.

- [ ] S56 - Add an agent runtime audit export.
  - Depends: S08, S55.
  - Implement: export effective prompt sections, tool policy, memory policy, retrieval policy, output contract, handoff policy, pinned definition version, context manifest references, and risky capability approvals.
  - Evidence required: export can explain why a custom agent did or did not have a capability.

- [ ] S57 - Add visual-builder round-trip regression coverage.
  - Depends: Milestone 1, Milestone 2.
  - Implement: cover creating, editing, duplicating, saving, reloading, and comparing custom agents with narrow and broad policies.
  - Evidence required: tests fail if saved graph fields are dropped, inflated, or replaced by runtime defaults.

- [ ] S58 - Add retrieval and memory quality evaluations.
  - Depends: Milestone 5, Milestone 6.
  - Implement: evaluate relevance, freshness, contradiction handling, first-turn continuity, user-approved memory recall, and degraded fallback behavior.
  - Evidence required: eval reports show pass/fail criteria and sample failures.

- [ ] S59 - Add handoff and context exhaustion evaluations.
  - Depends: Milestone 8.
  - Implement: evaluate context exhaustion, compaction, handoff, crash recovery, and target-run first-turn quality.
  - Evidence required: evals prove target sessions can continue without manual re-description in representative tasks.

- [ ] S60 - Add performance budgets for storage and retrieval.
  - Depends: S34, S37, S44.
  - Implement: define budgets for project open, agent selection, custom detail load, retrieval latency, memory review, handoff preparation, and startup diagnostics.
  - Evidence required: benchmark output tracks budgets and fails on meaningful regressions.

- [ ] S61 - Add support diagnostics for user reports.
  - Depends: S44, S56.
  - Implement: provide a permanent user-facing diagnostic bundle that summarizes app state without leaking secrets or requiring temporary debug UI.
  - Evidence required: diagnostic bundle can distinguish visual builder, runtime policy, storage, retrieval, memory, and handoff failures.

## Milestone 11 - Product Finish For Agent Authoring

Goal: make the system feel understandable and powerful to real users, not just technically correct.

- [ ] S62 - Make constraints visible before the user hits them.
  - Depends: S07, S08.
  - Implement: when a profile cannot use a tool, memory behavior, external capability, or workflow structure, the builder should show the reason and the available upgrade path.
  - Evidence required: user-facing copy is specific, not generic rejection text.

- [ ] S63 - Add task-oriented agent creation flows.
  - Depends: S12, S13, S20.
  - Implement: users can create agents by starting from common tasks, describing intent, or composing existing templates while seeing the resulting graph.
  - Evidence required: flows create ordinary custom agents that pass the same runtime and persistence checks.

- [ ] S64 - Add "what this agent knows" inspection.
  - Depends: S26, S28, S56.
  - Implement: users can inspect the memories, project records, continuity records, and handoff records likely to influence an agent before starting a run.
  - Evidence required: inspection matches retrieval policy and does not expose redacted content.

- [ ] S65 - Add safe correction flows when the agent remembers wrong information.
  - Depends: S28, S29, S38.
  - Implement: users can correct, supersede, or delete stale memories and project facts, and the system records why retrieval changed.
  - Evidence required: corrected information outranks stale information in future runs.

- [ ] S66 - Add clear run-start explanation.
  - Depends: S08, S26, S64.
  - Implement: when starting an agent, users can see what definition version, model, tool policy, context policy, memory policy, and approval mode will apply.
  - Evidence required: explanation matches the actual run manifest.

## Milestone 12 - Rollout And Documentation

Goal: ship the improved system without stale docs or hidden mismatch between intended and actual behavior.

- [ ] S67 - Align user documentation with actual behavior.
  - Depends: prior milestones as implemented.
  - Implement: update docs so they distinguish shipped behavior, degraded modes, limitations, and future work.
  - Evidence required: docs no longer describe hidden or aspirational UI as if it were available.

- [ ] S68 - Add migration or reset policy for existing custom definitions.
  - Depends: Milestone 1.
  - Implement: because the app is new and backwards compatibility is prohibited unless requested, choose either explicit reset or explicit one-time migration for existing custom definitions.
  - Evidence required: users are not left with silently partial custom agents.

- [ ] S69 - Add release acceptance checklist.
  - Depends: all production-target milestones.
  - Implement: define release gates for visual round-trip, runtime fidelity, first-turn context, storage repair, handoff recovery, security, diagnostics, and documentation.
  - Evidence required: release cannot be marked complete with unchecked critical gates.

- [ ] S70 - Dogfood with representative real workflows.
  - Depends: S69.
  - Implement: test engineering, debugging, planning, repository reconnaissance, custom support, and long-running handoff workflows using real projects and realistic context limits.
  - Evidence required: dogfood notes identify whether users needed to re-describe context, whether custom agents behaved as authored, and whether diagnostics explained failures.

## Recommended Implementation Order

1. Fix the canonical custom-agent contract and visual round-trip first. Without this, later runtime and storage improvements can still be hidden behind lossy authoring.
2. Add effective-runtime preview next. Users and developers need one trustworthy view of what an agent will actually run with.
3. Make output, database touchpoints, consumed artifacts, memory policy, retrieval policy, and handoff policy runtime-effective.
4. Improve first-turn working context before claiming agents can remember enough across sessions.
5. Harden LanceDB, SQLite, and cross-store reconciliation before scaling memory.
6. Expand custom capabilities only after the policy, audit, and preflight model is trustworthy.
7. Finish with observability, documentation, and dogfood gates.

## Definition Of "Good Enough"

The system should not be considered production-ready until all of the following are true:

- A custom agent saved from the visual builder reloads exactly as authored.
- The effective-runtime preview matches the actual provider run.
- Custom output, tool, memory, retrieval, handoff, consumed-artifact, and database-touchpoint settings affect runtime behavior.
- A new session can understand the active problem from source-cited working context and approved memory without requiring the user to manually restate everything.
- Handoff target sessions can continue from context exhaustion with enough structured evidence to work safely.
- SQLite and LanceDB can recover from partial writes, schema drift, and startup integrity problems without silent data loss.
- Retrieval uses real semantic embeddings and indexed search in normal operation, with visible degraded fallback.
- Users can inspect, approve, correct, disable, and delete memories that influence agents.
- Risky capabilities are explicitly granted, auditable, revocable, and visible before the run starts.
- Scoped regression tests and evaluations cover the construction, persistence, runtime, memory, retrieval, and handoff paths together.
