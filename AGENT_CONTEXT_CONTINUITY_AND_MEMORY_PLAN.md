# Agent Context Continuity and Memory Implementation Plan

## Reader And Outcome

Reader: an internal Xero engineer or agent implementing the next production slice of the agent runtime.

Post-read action: implement a production-grade continuity and memory system for Ask, Engineer, and Debug so every agent has the right project context, can safely continue in a new same-type run before context exhaustion, and stores durable knowledge in the app-data backed databases.

## Golden Rule

Agents must always be aware of the project and have the proper context for the work they are doing. If context matters, it must be persisted in the database, assembled deterministically into the next provider request or made available through a model-safe retrieval tool, and traceable back to its source.

## Source Of Truth

This document is the implementation source of truth for production agent continuity, context assembly, LanceDB retrieval, and durable memory.

Older Markdown plans for the initial addition of individual agents are historical only. They may help explain how the current implementation got here, but they must not drive this work. If an older plan conflicts with this document, follow this document.

Corollaries:

- No provider turn may be sent without a persisted context manifest.
- No important decision, handoff, finding, verification result, project fact, memory candidate, or active task may live only in chat text, local component state, local storage, or a temporary file.
- LanceDB is the retrieval store for durable agent knowledge. SQLite is the transactional store for session, run, policy, lineage, and context assembly state.
- App-data storage is the only valid location for new project state. Repo-local legacy state is not used for new continuity or memory data.
- This is a new application. Do not add compatibility shims unless the user explicitly asks for them.

## Scope

This plan covers all three runtime agents:

- Ask: answer-only, observe-only, project-aware.
- Engineer: implementation agent with planning, editing, verification, and durable engineering records.
- Debug: investigation agent with evidence capture, hypotheses, root cause, fixes, verification, and durable troubleshooting records.

The implementation must provide:

- Same-type automatic handoff before context exhaustion.
- Durable project records in LanceDB.
- Reviewed agent memory in LanceDB.
- Model-visible retrieval for relevant LanceDB data.
- Deterministic context packaging for every provider turn.
- Tests that prove these guarantees for Ask, Engineer, and Debug.

## Current Gaps To Close

- Context pressure currently results in optional compaction or a user-facing block, not automatic same-type handoff.
- Auto-compaction depends on UI preference state and skips when an active compaction already exists, even if the run is still under pressure.
- Final run handoffs are written to LanceDB project records, but the next agent cannot reliably retrieve those records.
- Approved memory is injected into prompts, but memory extraction and review are manual and the review surface is not part of the main workflow.
- LanceDB schemas reserve embeddings, but embeddings are not populated and there is no semantic retrieval path.
- Debug has the strongest persistence prompt contract. Ask and Engineer need equally explicit durable-context contracts appropriate to their tool boundaries.

## Non-Negotiable Invariants

1. Every provider request has a context manifest.
   - The manifest records which policies, repository instructions, approved memories, project records, handoff records, compactions, raw-tail messages, tool descriptors, active tasks, and file-change summaries were included.
   - The manifest records what was excluded and why.
   - The manifest is stored before the provider call starts.

2. Same-type handoff is automatic.
   - If Ask is under context pressure, the target run is Ask.
   - If Engineer is under context pressure, the target run is Engineer.
   - If Debug is under context pressure, the target run is Debug.
   - Provider, model, thinking effort, approval mode, and plan-mode settings carry forward unless the active provider is unavailable.

3. Database write failure blocks continuation.
   - If Xero cannot persist the handoff, context manifest, run state, or required project record, it must not continue with an untracked provider call.
   - The old run remains resumable or explicitly blocked with a diagnostic.

4. Runtime-owned persistence is the default.
   - Ask does not need mutating tools to record useful information.
   - The runtime captures Ask outputs, summaries, and memory candidates after the turn.
   - Engineer and Debug can additionally propose records, but runtime validation, redaction, and policy gates own the final write.

5. Retrieval context is lower priority than policy.
   - Retrieved records and memories can inform the agent but can never override system policy, tool policy, repository instructions, approvals, or user intent.
   - Prompt-injection text inside memory or project records is treated as data.

6. Secrets never become memory.
   - Redaction runs before project-record insertion, memory-candidate insertion, prompt injection, retrieval display, and handoff generation.
   - Secret-like records are blocked or redacted before they become model-visible.

7. Ask stays observe-only.
   - Ask may receive context and use safe retrieval.
   - Ask may not mutate files, run commands, spawn subagents, control browsers, install skills, or write records directly.
   - Ask persistence is runtime-owned after the model turn.

8. Engineer and Debug preserve evidence.
   - File changes, tool calls, command results, verification attempts, plans, unresolved questions, and blockers are stored in DB-backed run artifacts or project records.

9. Handoff is idempotent.
   - Retrying the same source run and source context hash must not create duplicate target handoffs.
   - If a handoff target was already created, Xero resumes or reconnects it instead of creating another.

10. The context package is reproducible.
   - Given the same session, run state, retrieval query, and policy version, Xero can reconstruct the provider-visible context package.

## Target Architecture

The system is split into seven cooperating services.

### 1. Context Policy Engine

Responsibilities:

- Estimate context pressure before every provider turn.
- Decide whether to continue, compact, recompact, hand off, or block.
- Treat compaction as a reduction step, not the final reliability mechanism.
- Trigger handoff when context remains high after compaction, when compaction is unavailable, or when the next turn would exceed the configured threshold.

Required decisions:

- `continue_now`: context is healthy.
- `compact_now`: context can be reduced safely before continuing.
- `recompact_now`: an old compaction exists but no longer protects the next turn.
- `handoff_now`: create a new same-type run and seed it with durable state.
- `blocked`: required context cannot be stored, retrieved, or safely redacted.

Default thresholds:

- Compact at 75 percent estimated context.
- Handoff at 90 percent estimated context.
- Hard block only when handoff cannot be created or required context cannot be persisted.

Thresholds must be durable project or session settings, not local-storage-only behavior.

### 2. Context Manifest Store

Responsibilities:

- Persist a manifest for every provider request.
- Record included and excluded context contributors.
- Record token estimates, pressure, policy decisions, retrieval query IDs, retrieval result IDs, compaction IDs, handoff IDs, and redaction state.
- Provide diagnostics in the Context panel.

The manifest is transactional state and belongs in SQLite.

### 3. Project Knowledge Store

Responsibilities:

- Store durable retrieval records in LanceDB.
- Support record kinds for handoffs, project facts, decisions, constraints, plans, findings, verification, questions, artifacts, context notes, and diagnostics.
- Populate embeddings for every retrievable record.
- Support keyword and metadata filtering when semantic search is unavailable.
- Track source item IDs, run IDs, session IDs, agent IDs, related paths, schema name, schema version, importance, confidence, tags, visibility, redaction state, and timestamps.

LanceDB records are append-friendly, but writes must be idempotent using source IDs and content hashes.

### 4. Reviewed Memory Store

Responsibilities:

- Store memory candidates and approved memories in LanceDB.
- Distinguish project-scoped and session-scoped memory.
- Keep approval, enablement, confidence, source, and diagnostic metadata.
- Inject only approved and enabled memories.
- Automatically create candidates after completion, pause, failure, and handoff.
- Require review before candidates become approved memory unless the user later opts into an explicit auto-approval policy.

### 5. Retrieval Service

Responsibilities:

- Search project records and reviewed memories.
- Support hybrid retrieval: vector similarity, keyword search, tags, kinds, related paths, runtime agent, session, recency, importance, and confidence.
- Return compact, cited snippets with record IDs and source metadata.
- Log retrieval queries and selected results in SQLite for replay and diagnostics.
- Expose safe read-only retrieval to all three agents.

Retrieval must be available in two ways:

- Automatic prompt injection of top relevant context.
- A read-only context tool the model can call when it needs more detail.

### 6. Handoff Orchestrator

Responsibilities:

- Create a durable handoff bundle from current DB state.
- Persist the bundle to LanceDB as a project record.
- Persist handoff lineage in SQLite.
- Create or reconnect a new same-type target run.
- Seed the target run with the handoff bundle, approved memory, relevant records, active tasks, recent raw tail, and current user intent.
- Mark the source run as handed off when the target run is durably created.
- Recover cleanly after process crash.

### 7. Agent Contract Compiler

Responsibilities:

- Compile agent-specific prompt contracts for Ask, Engineer, and Debug.
- Include the same persistence and retrieval guarantees across all agents, adjusted for each tool boundary.
- Make it explicit that durable context comes from Xero and is lower priority than system and tool policy.
- Enforce Ask observe-only rules while still giving Ask access to read-only project context.

## Data Model Plan

### SQLite Transactional State

Store these as durable transactional records:

- Runtime sessions and runtime runs.
- Agent runs, messages, events, tool calls, tool results, file changes, checkpoints, action requests, approvals, todos, and usage.
- Context manifests for every provider request.
- Context policy decisions.
- Compaction metadata and source hashes.
- Handoff lineage and target/source run links.
- Handoff creation attempts and failure diagnostics.
- Retrieval query logs and selected retrieval results.
- Memory extraction jobs and diagnostics.
- Durable project or session settings for context thresholds and auto-handoff behavior.
- Schema and policy versions used to assemble each provider request.

### LanceDB Retrieval State

Store these as retrievable records:

- Agent handoffs.
- Project facts.
- User decisions that affect the project.
- Constraints from users, repository instructions, or runtime policy.
- Plans and active task summaries.
- Findings and evidence.
- Verification commands and results.
- Open questions and blockers.
- Artifact summaries and references.
- Context notes.
- Diagnostics.
- Reviewed memories.
- Memory candidates.

Every retrievable row must include:

- Stable ID.
- Project ID.
- Optional session ID.
- Optional run ID.
- Runtime agent ID.
- Record kind.
- Title.
- Summary.
- Full text.
- Structured content JSON.
- Source item IDs.
- Related paths or symbols when known.
- Tags.
- Importance.
- Confidence.
- Visibility.
- Redaction state.
- Embedding vector.
- Embedding model and embedding version.
- Created and updated timestamps.

### Embeddings

Implement embeddings as a real service, not a placeholder column.

Requirements:

- Use a provider-neutral embedding interface.
- Store embedding model, dimension, and version with each embedded row.
- Refuse semantic retrieval if the configured embedding dimension does not match the table.
- Provide deterministic keyword fallback when embeddings are unavailable.
- Backfill missing embeddings through a durable job queue.
- Never inject unembedded records solely because semantic search failed; use recency, importance, kind, and keyword filters as fallback.

## Handoff Design

### Trigger Points

Evaluate context policy at these points:

- Before initial provider call.
- Before every user continuation.
- Before every provider call after tool results are appended.
- After large tool results are stored.
- After compaction is created.
- After memory or project-record injection changes the prompt.
- Before resuming from an approval wait.

### Handoff Bundle

A handoff bundle must be structured enough for a new agent to continue without asking the user to restate context.

Required fields:

- Source project, session, and run IDs.
- Target runtime agent ID.
- Provider and model settings.
- User goal and current task.
- Current status.
- Completed work.
- Pending work.
- Active todo items.
- Important decisions.
- Constraints.
- Relevant project facts.
- Recent file changes.
- Tool and command evidence.
- Verification status.
- Known risks.
- Open questions.
- Relevant approved memories.
- Relevant project records.
- Recent raw-tail message references.
- Source context hash.
- Redaction state.

Debug handoffs additionally require:

- Symptom.
- Reproduction path.
- Evidence ledger.
- Hypotheses tested.
- Root cause.
- Fix rationale.
- Verification evidence.
- Reusable troubleshooting facts.

Engineer handoffs additionally require:

- Implementation plan state.
- Files changed or intended.
- Build and test status.
- Remaining edits.
- Review risks.

Ask handoffs additionally require:

- Question being answered.
- Project context used.
- Uncertainties.
- Follow-up information needed.

### Handoff Algorithm

1. Flush run state.
   - Persist pending messages, tool results, file-change summaries, todo state, approvals, usage, and diagnostics.

2. Recompute context pressure.
   - If pressure is below threshold, continue normally.
   - If pressure is high, try compaction or recompaction.
   - If pressure remains high or compaction is unavailable, hand off.

3. Build a source manifest.
   - Include active compaction, raw tail, important records, memories, tool summaries, file changes, and open tasks.

4. Generate the handoff bundle.
   - Prefer deterministic extraction from DB state.
   - Use provider summarization only to improve wording and condensation.
   - Validate that required fields are present.

5. Redact and validate.
   - Block secret-bearing or instruction-overriding content.
   - Preserve source IDs for redacted entries.

6. Persist handoff.
   - Write SQLite handoff lineage first as `pending`.
   - Write LanceDB project record with idempotency key.
   - Update SQLite lineage to `recorded`.

7. Create the target run.
   - Same runtime agent type.
   - Same session unless a new session boundary is required by the product design.
   - Same provider controls unless unavailable.
   - Seed the target run with a system-owned handoff message and the pending user prompt when applicable.

8. Mark source and target.
   - Source run becomes `handed_off`.
   - Target run becomes `running` or `ready`.
   - Runtime UI points the composer to the target run.

9. Continue.
   - The next provider request uses the target run context manifest.

### Failure Handling

- If handoff generation fails, retry with deterministic DB-only bundle.
- If LanceDB insert fails, do not create the target run.
- If target run creation fails after handoff record insertion, keep lineage as `recorded` and retry target creation by idempotency key.
- If the app crashes mid-handoff, startup recovery resumes pending handoffs or marks them failed with diagnostics.
- If redaction blocks required context, stop and ask the user how to proceed.
- If retrieval fails, continue only with required context that is already in the manifest; otherwise block.

## Context Package Design

Every provider request is assembled from a context package.

Required contributors:

- Runtime system policy.
- Active tool policy.
- Agent-specific contract.
- Repository instructions.
- Current user prompt or queued prompt.
- Current run and session state.
- Context pressure and policy decision.
- Active compaction summary when present.
- Recent raw conversation tail.
- Approved memory.
- Relevant project records.
- Active handoff bundle when present.
- Active todo or plan state.
- File-change summary.
- Required tool descriptors.

Optional contributors:

- Process state digest.
- Code map.
- Artifact summaries.
- Retrieval snippets requested by the model.
- Lower-priority historical records.

Priority rules:

1. System and runtime policy.
2. User request and operator approvals.
3. Repository instructions.
4. Active task state and handoff bundle.
5. Current raw tail.
6. Approved memory.
7. Relevant project records.
8. Tool output summaries and artifacts.
9. Deferred historical context.

If required contributors cannot fit, Xero must hand off or block. It must not silently drop required context and continue.

## Agent-Specific Requirements

### Ask

Ask must:

- Receive the same project context package as other agents.
- Have read-only access to approved memory and relevant project records.
- Be able to call a safe retrieval tool.
- Never mutate files, app state, processes, browser state, external services, skills, subagents, or DB records directly.
- Have its final answer captured by the runtime as a project record when useful.
- Produce memory candidates through runtime-owned extraction after completion.

Ask prompt contract must include:

- Answer directly.
- Cite project facts or uncertainty when relevant.
- Name important files, symbols, decisions, or constraints when useful.
- Include a concise handoff-quality final answer when the conversation may continue.
- Do not include secrets.

### Engineer

Engineer must:

- Receive the full project context package.
- Use planning and verification gates for non-trivial work.
- Store meaningful plans, decisions, file-change summaries, verification results, blockers, and final handoffs.
- Use retrieval before acting when the task references prior work, project decisions, known constraints, or previous failures.
- Create memory candidates after completion or handoff.

Engineer prompt contract must include:

- Inspect before editing.
- Keep changes scoped.
- Preserve dirty worktree safety.
- Record decisions and verification evidence.
- Summarize changed files, tests, blockers, and follow-ups in a durable handoff-friendly final answer.

### Debug

Debug must:

- Receive the full project context package.
- Retrieve prior debugging records and troubleshooting memories before investigating.
- Maintain evidence, hypotheses, experiments, root cause, fix rationale, and verification.
- Store debugging findings, root causes, fixes, verification records, and troubleshooting facts.
- Create high-importance project records for durable debugging knowledge.

Debug prompt contract must include:

- Prefer evidence over confidence.
- Reproduce or tightly simulate the issue.
- Test falsifiable hypotheses.
- Preserve reusable troubleshooting knowledge.
- Include symptom, root cause, fix, verification, remaining risks, and saved debugging knowledge in the final answer.

## Model-Visible Tools

Add a read-only project context tool available to Ask, Engineer, and Debug.

Actions:

- Search project records.
- Search approved memory.
- Get a project record by ID.
- Get a memory by ID.
- List recent handoffs.
- List active decisions and constraints.
- List open questions and blockers.
- Explain current context package.

Tool constraints:

- Ask can only use read-only actions.
- Engineer and Debug can use read-only actions and may propose new records through a separate candidate action if policy allows.
- Any write-like action creates a candidate or runtime-owned request, not an immediately trusted memory.
- Tool results include source IDs and redaction state.
- Tool results are recorded in the run log.

Add a runtime-owned record capture path.

Capture sources:

- Final assistant messages.
- Handoff bundles.
- Plans.
- Todo state transitions.
- Verification results.
- Debug findings.
- Tool-result summaries.
- User decisions and constraints.
- Memory extraction candidates.

Capture must be automatic for all agents.

## UI Requirements

Use the existing ShadCN-based UI patterns.

Required surfaces:

- Context panel showing current pressure, manifest, included contributors, excluded contributors, and policy decisions.
- Handoff event display showing source run, target run, agent type, status, and diagnostics.
- Memory review surface mounted in the normal runtime workflow.
- Project records surface or context inspector for recent handoffs, decisions, constraints, findings, and verification records.
- Retrieval diagnostics for what was injected into the prompt.
- User controls for durable context thresholds and auto-handoff settings.

UI rules:

- Do not add temporary debug or test UI.
- Do not require a browser workflow for verification because this is a Tauri app.
- Do not make persistence depend on whether a UI panel is open.
- UI preferences may live in local state, but core continuity policy must live in DB-backed settings.

## Implementation Phases

### Phase 1: Contracts And Durable Settings

Deliverables:

- Define context policy actions, including handoff.
- Define handoff lineage records.
- Define context manifest records.
- Define durable context policy settings.
- Define retrieval query/result logs.
- Update agent prompt contracts for Ask, Engineer, and Debug.
- Add schema tests for the new contracts.

Acceptance criteria:

- All three agents have explicit persistence and retrieval contracts.
- Auto-handoff thresholds are DB-backed.
- A context manifest can be persisted without a provider call.
- Tests prove same-type handoff decisions preserve the runtime agent ID.

### Phase 2: LanceDB Retrieval Foundation

Deliverables:

- Populate embeddings for project records and reviewed memories.
- Add embedding model and version metadata.
- Add hybrid search over project records and memories.
- Add keyword fallback.
- Add idempotent insert and backfill jobs.
- Add retrieval query/result logging.

Acceptance criteria:

- Inserted project records and memories have non-null embeddings when embedding service is configured.
- Search returns filtered, cited results by kind, tag, path, agent, session, recency, importance, and text.
- Retrieval failures are diagnosable and do not silently inject empty context.
- Tests cover embedding mismatch, fallback retrieval, redaction, and deduplication.

### Phase 3: Context Package Assembler

Deliverables:

- Build a deterministic context package for every provider request.
- Include approved memory and relevant project records for all agents.
- Persist context manifests before provider calls.
- Add contributor priority and exclusion reasons.
- Add prompt-injection defenses for retrieved content.

Acceptance criteria:

- Every provider request has a stored manifest.
- Ask, Engineer, and Debug all receive approved memory and relevant project records.
- Required context is never silently dropped.
- Tests prove manifests are reproducible from DB state.

### Phase 4: Handoff Orchestrator

Deliverables:

- Add handoff trigger evaluation after compaction.
- Generate structured handoff bundles.
- Persist handoff bundles to LanceDB.
- Persist handoff lineage to SQLite.
- Create or reconnect same-type target runs.
- Seed target runs with handoff context.
- Add crash recovery for pending handoffs.

Acceptance criteria:

- A synthetic long Ask run hands off to Ask.
- A synthetic long Engineer run hands off to Engineer.
- A synthetic long Debug run hands off to Debug.
- Target runs can continue without the user restating the task.
- Duplicate retries do not create duplicate handoffs or target runs.
- If handoff persistence fails, no provider call is made.

### Phase 5: Automatic Record Capture And Memory Candidates

Deliverables:

- Capture final answers, plans, decisions, verification, findings, diagnostics, and handoffs as project records.
- Run memory extraction after completion, pause, failure, and handoff.
- Store memory candidates disabled until reviewed.
- Mount the memory review workflow.
- Add user-visible diagnostics for rejected candidates.

Acceptance criteria:

- Useful run information is stored even when the model did not explicitly call a record tool.
- Approved memories are injected into future Ask, Engineer, and Debug runs.
- Candidate memories never become approved without review.
- Secret-like and instruction-overriding candidates are blocked.

### Phase 6: Model-Visible Context Tooling

Deliverables:

- Add read-only project context retrieval tool.
- Make it available to all agents.
- Keep Ask observe-only.
- Add candidate record proposal actions for agents that are allowed to request writes.
- Record every retrieval tool call and result.

Acceptance criteria:

- Ask can search and read records but cannot write.
- Engineer and Debug can retrieve context before acting.
- Tool results are source-cited, redacted, and logged.
- Tests prove permission boundaries.

### Phase 7: UI And Operator Experience

Deliverables:

- Context panel shows manifests, budget pressure, handoff policy, retrieval injections, and diagnostics.
- Handoff status appears in the runtime stream.
- Memory review is reachable from normal workflow.
- Project records can be inspected without developer-only UI.
- Durable policy settings are editable if product design allows.

Acceptance criteria:

- Users can understand why handoff happened.
- Users can review memory candidates.
- Users can inspect what context was used.
- No temporary or test-only UI is introduced.

### Phase 8: Hardening And Release Gate

Deliverables:

- Scoped Rust tests.
- Scoped TypeScript tests.
- Tauri command contract tests.
- Crash recovery tests.
- Redaction and prompt-injection tests.
- Context pressure and handoff stress tests.
- Documentation for the runtime behavior.

Acceptance criteria:

- All scoped tests pass.
- No new state is written to legacy repo-local locations.
- Long-running sessions automatically continue through same-type handoff.
- LanceDB retrieval is available to all three agents.
- DB failures block unsafe continuation.
- The app can restart during a pending handoff and recover deterministically.

## Test Matrix

### Rust Unit Tests

- Context policy chooses compact below handoff threshold.
- Context policy chooses handoff above handoff threshold.
- Active compaction does not prevent handoff when context remains high.
- Unknown provider budget falls back to configurable conservative thresholds or blocks with diagnostics.
- Same-type handoff preserves Ask, Engineer, and Debug.
- Handoff bundle validation rejects missing required fields.
- Handoff insert is idempotent by source hash.
- Context manifest persists before provider call.
- Prompt compiler includes approved memory for all agents.
- Prompt compiler includes relevant project records for all agents.
- Ask tool permissions remain read-only.
- Engineer and Debug keep engineering tool access.
- Redaction blocks secret-bearing records.
- Retrieval fallback works without embeddings.
- Embedding mismatch is detected.
- No new state writes to legacy repo-local storage.

### TypeScript Unit Tests

- Agent descriptors expose persistence and retrieval policies for all three agents.
- Context panel renders budget pressure and policy decisions.
- Handoff events render source and target run IDs.
- Memory review workflow is mounted and can approve, reject, enable, disable, and delete records.
- Ask cannot display write-only controls.
- Durable settings are not represented as local-storage-only core behavior.

### Integration Tests

- Long Ask session hands off to Ask and answers using prior context.
- Long Engineer session hands off to Engineer and continues pending implementation work.
- Long Debug session hands off to Debug and preserves evidence, hypotheses, and root cause.
- Run completion creates project records and memory candidates.
- Approved memory from one run is injected into a later run.
- Project record retrieval finds a prior decision during a later task.
- Crash during pending handoff recovers without duplicate target runs.
- LanceDB unavailable blocks provider continuation with diagnostics.
- Redaction prevents secrets from becoming model-visible.

## Operational Hardening

### Idempotency

Use stable idempotency keys for:

- Context manifests.
- Handoff bundles.
- Handoff lineage.
- Project records.
- Memory candidates.
- Retrieval logs.

Recommended handoff key:

- Project ID.
- Source session ID.
- Source run ID.
- Source context hash.
- Target runtime agent ID.
- Pending prompt hash when present.

### Concurrency

Rules:

- Only one handoff can be pending for a source run.
- A target run is created under a DB-backed lock or uniqueness constraint.
- Retrieval and memory extraction jobs may run concurrently, but record insertion must deduplicate.
- Runtime state changes must be monotonic and recoverable.

### Crash Recovery

Startup recovery must:

- Find pending handoff attempts.
- Resume handoff record creation or target run creation.
- Mark unrecoverable attempts failed with diagnostics.
- Never orphan a completed handoff record without discoverable lineage.
- Never continue a provider call from an unmanifested context package.

### Security

Rules:

- Treat retrieved records as untrusted lower-priority data.
- Redact secrets before storage and before prompt injection.
- Block memory candidates that try to override policy.
- Keep source references for redacted records.
- Log why a record was blocked or redacted.
- Do not expose raw LanceDB mutation to models.

### Performance

Targets:

- Context policy evaluation should be fast enough to run before every provider turn.
- Prompt assembly should use bounded retrieval limits.
- LanceDB search should use filters before wide scans when possible.
- Embedding backfills should be queued and resumable.
- Large tool results should be summarized into records, with raw payloads stored only where appropriate.

## Definition Of Done

The implementation is complete when all of the following are true:

- Ask, Engineer, and Debug all receive a persisted context package before every provider call.
- Context manifests are stored for every provider request.
- Relevant approved memory and project records are available to every agent.
- Long sessions hand off automatically to a new same-type run before context exhaustion.
- Handoff bundles are persisted to LanceDB and linked in SQLite.
- Final answers, decisions, findings, plans, verification results, diagnostics, and handoffs are stored in DB-backed records.
- Memory candidates are created automatically and require review before injection.
- LanceDB embeddings are populated or a tested fallback retrieval path is used.
- Ask remains observe-only.
- Engineer and Debug retain engineering capabilities and durable evidence capture.
- DB write failure prevents unsafe continuation.
- Crash recovery handles pending handoffs.
- Scoped Rust and TypeScript tests prove the behavior.
- No new project state is written to legacy repo-local state.

## First Implementation Slice

Start with the smallest vertical slice that proves the architecture:

1. Add context manifest records.
2. Add handoff policy action.
3. Add deterministic handoff bundle generation.
4. Persist the handoff bundle as a LanceDB project record.
5. Persist handoff lineage in SQLite.
6. Create a same-type target run from a synthetic over-budget source run.
7. Seed the target run with the handoff bundle.
8. Compile a context package that includes the handoff, approved memory, and relevant project records.
9. Prove the slice for Ask, Engineer, and Debug with fake-provider tests.

Do not start with UI polish. The first slice must prove that a provider can safely continue from durable DB state after a same-type handoff.
