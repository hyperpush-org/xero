# Agent Memory Layer Audit And Improvement Plan

This document audits the current Xero agent memory layer as implemented in the repo and proposes a concrete plan to make memory more useful, safer, and more consistently available to agents.

The most important finding is that Xero has several strong primitives already: app-data-backed durable storage, LanceDB embeddings, source citations, retrieval logs, freshness tracking, memory governance commands, mailbox promotion, code-history guardrails, and first-turn working-set summaries. The biggest weakness is runtime policy alignment: model-extracted memory should pass an explicit automated quality and safety gate before it becomes retrievable, but the automatic owned-agent extraction path currently inserts extracted memory as approved and enabled. That bypass turns noisy or incorrect model summaries into agent-visible memory too early.

## Scope

Audited areas:

- Durable agent memory records and their LanceDB store.
- Project records used as durable context.
- Automatic project-record and memory capture after owned-agent runs.
- The agent-facing `project_context` tool.
- First-turn context package assembly and provider context manifests.
- Handoff bundles and continuity records.
- Agent mailbox and active-agent coordination state.
- Memory governance commands and the existing Settings memory-review section, as audit context only, not as a required approval loop.
- Embedding, retrieval, freshness, supersession, redaction, and cross-store recovery paths.
- Relevant docs and runtime agent descriptors.

Important audit caveat: this was performed against the current working tree, which contains many unrelated in-progress changes. I did not repair or revert those changes. Before implementation, restore or confirm a compile-clean baseline for the affected Rust modules.

Implementation constraint: this improvement plan must be backend/runtime/API-only. It should not create new screens, panels, controls, settings sections, inspectors, or other new UI. The only acceptable visual exception is updating existing agent-canvas rendering when the existing create/view-agent canvas needs to show memory-related nodes or edges that are already part of the agent model.

Runtime autonomy constraint: the normal agent runtime must not put the user in the loop for saving, promoting, retrieving, disabling, or correcting memory. Memory decisions should be programmatic and/or agent-directed under runtime policy. Existing state-changing commands may remain as support, diagnostics, migration, or emergency override tools, but they must not be required for memory to work and should not be treated as a normal user approval flow.

## Executive Summary

Xero's memory layer is really four related systems, not one:

- Approved memory: compact durable facts, preferences, decisions, session summaries, and troubleshooting notes that agents can retrieve.
- Project records: broader durable context such as handoffs, plans, decisions, constraints, findings, verification, artifacts, diagnostics, and context notes.
- Mailbox: temporary, TTL-scoped coordination between active agents. It is not durable memory unless promoted to a project-record candidate.
- Context manifests and retrieval logs: audit evidence showing what context was available, what was selected, what was omitted, and how retrieval behaved.

The architecture is directionally good. It treats memory as lower-priority cited data, not policy. It stores new project state under app-data, not legacy `.xero/`. It has redaction, source IDs, freshness, supersession, and code-history awareness. It exposes memory through tools rather than dumping unlimited raw context into prompts.

The current effectiveness problem is that useful memory often arrives too late, with too little confidence calibration, or without a clear automated promotion decision. Meanwhile less useful memory can become approved automatically in one runtime path without an explicit named gate. Agents get a small first-turn working-set summary, but they must still decide to call `project_context` for exact content. Handoff bundles tell the target run to retrieve context but currently carry empty `approvedMemories` and `relevantProjectRecords` arrays, so continuation depends on the next agent doing the right retrieval.

The recommended plan is to add an automated memory promotion gate first, then tighten retrieval eligibility, improve capture quality, make memory more proactively visible at task and handoff boundaries, and add evals/observability so memory usefulness can be measured instead of guessed.

## Current Architecture

### Storage Layout

Xero splits memory state across SQLite and LanceDB under OS app-data.

SQLite stores transactional runtime state:

- Agent sessions, runs, messages, events, file changes, checkpoints, usage, action requests, and diagnostics.
- Context manifests and contributors.
- Handoff lineage.
- Retrieval query and result logs.
- Context policy settings.
- Agent coordination presence, events, file reservations, mailbox items, and mailbox acknowledgements.
- Cross-store outbox rows for LanceDB writes and replay/reconciliation.
- Embedding backfill jobs.

LanceDB stores retrieval state:

- `project_records`: durable project context rows with embeddings.
- `agent_memories`: approved/candidate/rejected memory rows with embeddings.

This split is sensible. SQLite is the transactional ledger; LanceDB is the searchable knowledge store. The cross-store outbox is the bridge between them.

### Durable Memory

Agent memory records include:

- `memory_id`, `project_id`, optional `agent_session_id`.
- Scope: `project` or `session`.
- Kind: `project_fact`, `user_preference`, `decision`, `session_summary`, or `troubleshooting`.
- Text, normalized text hash, source run ID, source item IDs, confidence, diagnostic.
- Review state: `candidate`, `approved`, or `rejected`. In the target architecture, this state is advanced by automated runtime/agent policy during normal operation, not by user approval.
- `enabled`.
- Freshness fields: `freshness_state`, `freshness_checked_at`, `stale_reason`, source fingerprints.
- Supersession fields: `supersedes_id`, `superseded_by_id`, `invalidated_at`, `fact_key`.
- Embedding metadata: provider/model/dimension/version.
- Created and updated timestamps.

Validation is mostly well-shaped:

- Session-scoped memory must have a session ID.
- Project-scoped memory must not have a session ID.
- Enabled memory must be approved by an automated promotion gate.
- Source item IDs are required.
- Redacted or prompt-injection-shaped content is blocked before promotion paths.
- Duplicate active memory is checked by text hash, scope, session, and kind.

The existing review queue exposes only previews, never raw full text by default. It marks whether a memory is retrievable and why not. In the improvement plan, this queue is diagnostic/support state, not the normal way memory becomes active.

### Project Records

Project records are wider durable context than memory. Kinds include:

- `agent_handoff`
- `project_fact`
- `decision`
- `constraint`
- `plan`
- `finding`
- `verification`
- `question`
- `artifact`
- `context_note`
- `diagnostic`

Visibility includes normal retrieval rows, inactive memory candidates, workflow rows, and diagnostics. Project records have text, title, summary, content JSON, tags, related paths, produced artifact refs, source IDs, confidence, importance, freshness, supersession, and embeddings.

Project records are currently the main durable continuity substrate. Automatic runtime capture records final answers, terminal summaries, current-problem continuity, latest plans, decisions, verification, diagnostics, debug findings, and crawl report topics.

### Mailbox And Coordination

The mailbox is intentionally temporary state. It is stored in SQLite, not LanceDB, and defaults to a 1-hour TTL.

Mailbox item types include:

- `heads_up`
- `question`
- `answer`
- `blocker`
- `file_ownership_note`
- `finding_in_progress`
- `verification_note`
- `handoff_lite_summary`
- `history_rewrite_notice`
- `undo_conflict_notice`
- `workspace_epoch_advanced`
- `reservation_invalidated`

Mailbox rows contain sender run/session/role, optional target session/run/role, title, body, related paths, priority, status, timestamps, acknowledgements, resolution metadata, and optional promoted record ID.

Agents interact with mailbox through the `agent_coordination` tool:

- List active agents, reservations, conflicts, and activity.
- Claim and release advisory file reservations.
- Publish, read, acknowledge, reply to, resolve, and promote mailbox items.

Promotion converts a temporary mailbox item into a project-record candidate with `visibility = memory_candidate`, tags such as `swarm-mailbox` and `memory-candidate`, and content JSON that records `temporaryMailbox: true` and `approvedMemoryAutomatically: false`.

This separation is good. Mailbox content should not automatically become long-term memory. It is short-lived coordination unless someone promotes it.

### Context Package And First-Turn Retrieval

Before a provider turn, Xero assembles a deterministic context package and persists a context manifest. The manifest records prompt fragments, messages, tool descriptors, policy decisions, retrieval, working set, coordination state, handoff/compaction metadata, redaction state, and contributors.

The durable context delivery model is intentionally tool-mediated:

- Raw durable memory is not bulk-injected.
- Retrieval is run for a small working set.
- The prompt receives a source-cited summary of top results.
- The agent is told to call `project_context_get` before relying on details.

First-turn retrieval uses `retrievalDefaults` from the agent definition when present. If both record kinds and memory kinds are requested, the search scope is hybrid. If only memory kinds are requested, the scope is approved memory. Otherwise it defaults to project records.

There is one important implementation tradeoff: provider-turn retrieval calls the no-freshness-refresh path for latency. It logs freshness diagnostics as skipped. This avoids slow preflight, but it can expose stale or superseded rows in a working-set summary until a manual retrieval or explicit freshness refresh happens.

### Agent-Facing Tools

The `project_context` tool supports:

- Search project records.
- Search approved memory.
- Get project record by ID.
- Get memory by ID.
- List recent handoffs.
- List active decisions and constraints.
- List open questions and blockers.
- Explain the current context package.
- Record durable context.
- Update durable context or create correction records.
- Propose project-record candidates.
- Refresh freshness.

Read actions are available broadly. Mutations are restricted:

- Ask cannot record or update context.
- Agent Create and Crawl cannot record context through `project_context`.
- Plan can record only accepted plan-pack records.
- Engineering-capable agents can record/update context subject to policy.

Tool descriptors correctly describe `project_context` as source-cited, redacted, app-data-backed durable context. The coordination descriptor correctly states that mailbox is TTL-scoped runtime state, not durable memory.

### Automatic Capture

After owned-agent runs, Xero captures project records and memory candidates on:

- Completion.
- Pause.
- Non-pausing failure.
- Handoff.

Project-record capture is deterministic and mostly rule-based. Memory extraction is provider-based:

- The runtime builds a redacted transcript with source item IDs.
- It includes code-history operation provenance.
- It sends the transcript to the provider's `extract_memory_candidates` method.
- The provider must return JSON candidates with scope, kind, text, confidence, and source item IDs.
- Low confidence candidates are skipped.
- Redacted/secret/prompt-injection-shaped candidates are rejected.
- Code-history guardrails reject or downgrade facts affected by undo/rollback.
- Duplicates are skipped by active text hash.

There are two extraction paths today:

- Support/session-history extraction creates `candidate` memories with `enabled = false`.
- Automatic owned-agent runtime extraction currently creates `approved` memories with `enabled = true`.

The target behavior is neither user approval nor unconditional auto-approval. Extracted memory should flow through a named automated promotion gate that can approve, reject, disable, correct, supersede, or keep a candidate for later automated handling.

### Freshness And Supersession

Freshness uses source fingerprints from related paths and source file changes. Rows can be:

- `current`
- `source_unknown`
- `stale`
- `source_missing`
- `superseded`
- `blocked`

Existing diagnostic queue eligibility treats approved/enabled/current or source-unknown rows as retrievable, and excludes superseded, invalidated, stale, source-missing, blocked, disabled, and pending/rejected memory.

Search scoring penalizes stale/superseded/source-missing rows and adds trust metadata, but it does not consistently hard-exclude all contradicted rows from every retrieval path. This means diagnostic retrievability is stricter than the search layer's ranking behavior.

Supersession uses fact keys and source fingerprint overlap. Corrections create a new approved memory and mark the old memory superseded/invalidated.

### Embeddings And Fallback

Embeddings are fixed-size, currently 768-dimensional. A configured OpenAI-compatible embedding provider can be used through environment settings. If unavailable, Xero falls back to a deterministic local hash embedding service.

Fallback is useful for offline behavior, but it is not true semantic retrieval. Retrieval logs surface degradation and keyword fallback, which is good. Memory usefulness will be much higher when a real embedding provider is configured and backfill jobs keep rows current.

## What Gets Saved And When

Saved before provider calls:

- Context manifest with contributors, exclusions, policy decision, retrieval summary, coordination summary, prompt fragments, message previews, tools, and preflight metadata.
- Retrieval query/result logs for the first-turn working-set retrieval.

Saved during tool use:

- `project_context` searches log retrieval queries and results.
- `project_context` get actions log manual retrieval evidence.
- `project_context` record/update actions insert project records into LanceDB and cross-store outbox rows.
- `project_context` freshness refresh updates LanceDB freshness fields.
- `agent_coordination` actions update presence, reservations, mailbox items, acknowledgements, resolution, or mailbox promotion state.

Saved after runs:

- Project records for terminal summaries, final answers, continuity, plans, decisions, verification, diagnostics, debug findings, and crawl-derived context.
- Provider-extracted memory candidates or, in the automatic path today, approved/enabled memories.
- Validation events and diagnostics for extraction success/failure/skips.

Saved during handoff:

- Handoff lineage in SQLite.
- Handoff bundle as a project record in LanceDB.
- Target run with a developer seed message containing the bundle.
- Source run marked handed off.
- Memory extraction from the source run.

Saved during memory governance:

- Review state and enabled updates from automated gates, agent-directed memory tools, support commands, or migrations.
- Corrections as new approved memory records that supersede the original.
- Deletions in LanceDB.

Saved during maintenance:

- Embedding backfill jobs and outcomes.
- LanceDB health and optimization information through project-state diagnostics.
- Outbox status for cross-store replay/reconciliation.

## Key Strengths

1. App-data storage is the right default.

New durable state is not written under legacy `.xero/`. That matches the repo rule and prevents project working trees from accumulating runtime state.

2. Memory is lower-priority cited context, not policy.

Tool descriptors, docs, and prompt summaries all describe memory as evidence. This is important for prompt-injection resilience.

3. Mailbox is correctly separate from memory.

Temporary agent coordination has TTL, acknowledgement, targeting, priority, and promotion. It does not silently pollute long-term context.

4. Source citations exist almost everywhere.

Memory, project records, retrieval results, handoff bundles, mailbox promotions, and manifests all carry IDs that can be traced.

5. Freshness and supersession are first-class fields.

The data model can represent stale, missing, superseded, invalidated, blocked, and source-unknown context. That is the right foundation for reliable memory.

6. Code-history guardrails are thoughtful.

Undo and rollback are append-only history operations. Memory extraction is explicitly told not to promote reverted implementation details without provenance.

7. Governance commands and an existing diagnostic command path exist.

The Settings memory section can load a queue, approve, reject, disable, delete, and correct memory. This plan does not expand or redesign that interface, and it must not make that interface part of normal runtime memory operation. It treats existing commands as support/diagnostic overrides over an autonomous backend pipeline.

8. Retrieval observability is strong.

Queries and results are logged. Diagnostics include degraded/fallback mode, result metadata, freshness summaries, and source citations.

9. Cross-store recovery exists.

The outbox can detect and replay missing LanceDB rows from serialized payloads.

## Findings And Risks

### P0: Automatic owned-agent memory promotion lacks an explicit gate

The runtime `capture_memory_candidates_for_run` path prepares extracted memories as `review_state = Approved` and `enabled = true`. Support/session-history extraction prepares candidates as `Candidate` and `enabled = false`. The problem is not missing user approval; the problem is that the automatic path does not route them through a named, testable programmatic promotion gate.

Why this matters:

- It contradicts the intended memory-policy contract.
- It trusts provider summarization too early.
- It increases the chance that future agents retrieve false, vague, duplicated, stale, or overbroad memory.
- It hides the decision logic that should explain why a memory became active.

Target behavior:

- Provider-extracted memory should be captured, evaluated, and either promoted to approved/enabled, rejected, corrected, superseded, or kept inactive by automated runtime policy.
- The promotion gate should be deterministic enough to test and observable enough to explain.
- Agents may request memory saves or corrections during runtime, but the runtime policy gate owns final activation.
- No normal runtime flow should wait for user approval.

### P0: Retrieval eligibility is not unified

The existing diagnostic review queue has a strict eligibility model. Retrieval scoring has trust penalties, but can still rank stale/superseded/source-missing rows if they pass earlier filters.

Why this matters:

- Agents may see contradicted memory in search results, especially when semantic/keyword scores are high.
- Diagnostic state can say "not retrievable" while search behavior still considers a row.
- Trust metadata helps, but models can still over-rely on stale snippets.

Target behavior:

- Default retrieval hard-excludes rejected, disabled, blocked, superseded, invalidated, stale, and source-missing memory.
- Project records should follow the same default for superseded/invalidated/blocked rows.
- Explicit diagnostic or historical searches can opt into stale/superseded rows with a named flag.

### P0: Current worktree appears syntactically unstable in memory modules

Several inspected Rust files contain duplicated fields, duplicate match arms, stray braces, and repeated lines in memory, retrieval, project-record, continuity, outbox, and migration modules.

Why this matters:

- The audit plan should not be implemented on top of an unknown broken baseline.
- Memory correctness depends on compile-time and migration integrity.

Target behavior:

- Before implementing memory changes, get a scoped Rust check green for the affected modules, or intentionally separate the current in-progress work from the memory improvement branch/work.

### P1: Handoff bundles do not actually carry relevant memory

The handoff bundle structure includes `approvedMemories` and `relevantProjectRecords`, but the current bundle fills them with empty arrays and tells the target run to use `project_context`.

Why this matters:

- Continuity depends on the next agent knowing when and how to retrieve.
- High-value context can be missed on the first target turn.
- The handoff bundle claims to preserve relevant memory but does not yet do the work.

Target behavior:

- Handoff assembly should run a focused retrieval for the source goal and include a small cited subset of approved memory and project records.
- The target run should still use `project_context` for exact content, but the bundle should carry enough anchors to make retrieval obvious and auditable.

### P1: Provider-turn retrieval skips freshness refresh

The first-turn context package uses `search_agent_context_without_freshness_refresh` for latency. This can include rows whose source files changed since last refresh.

Why this matters:

- The first model turn is often where task framing happens.
- A stale working-set summary can bias the agent before it calls tools.

Target behavior:

- Keep the fast path, but add a cheap prefilter that excludes rows already marked stale, source-missing, superseded, blocked, or invalidated.
- Queue asynchronous freshness refresh for any selected rows with source fingerprints older than a small threshold.
- Optionally run synchronous targeted refresh for the top N rows only.

### P1: Source item IDs are not strong enough as provenance

Provider candidates can return source item IDs. If empty, the runtime falls back to the first eight transcript source IDs. The guardrails validate content safety, but provenance quality is still loose.

Why this matters:

- Memory can look cited while the source IDs are broad or irrelevant.
- Future agents, support diagnostics, and automated gates need to know exactly why a memory exists.

Target behavior:

- Validate provider-supplied source IDs against the extraction transcript.
- Require at least one source ID with text overlap or a structured extraction reason.
- Store source spans or quoted evidence snippets where feasible.
- Mark fallback source IDs as low-provenance in diagnostics.

### P1: Project-record capture can be noisy

Automatic capture writes terminal summaries, final answers, continuity notes, plans, decisions, diagnostics, verification, and debug findings. This is useful but can create many overlapping rows.

Why this matters:

- Retrieval precision drops when many records say similar broad things.
- Agents may retrieve meta-summaries instead of the actual decision, plan, or file evidence.

Target behavior:

- Add type-specific capture quality gates.
- Prefer structured records for decisions, constraints, verification, and findings.
- Merge or supersede repetitive continuity records.
- Make final-answer/project-summary records less likely to outrank specific facts.

### P1: Memory policies are defined but not consistently enforced end to end

Built-in and custom agents describe memory candidate kinds and memory-policy requirements. The runtime tool policy restricts context writes by agent type, but automatic extraction does not appear to honor those requirements as an automated gate.

Why this matters:

- Agent authors may believe memory policy is mandatory when automatic extraction bypasses it.
- Custom agents need predictable memory boundaries.

Target behavior:

- Treat `memoryCandidatePolicy` as authoritative, but implement it as automated runtime governance rather than user approval.
- Filter extraction candidates by allowed memory kinds.
- Persist policy snapshot/decision with every memory candidate.

### P2: Mailbox promotion is agent-directed but broad

Mailbox promotion creates project-record candidates, not memory, which is correct. However, high-value temporary coordination can disappear when TTL expires if no agent or runtime heuristic promotes it.

Why this matters:

- Blockers, file ownership notes, and verification notes can be valuable later.
- Agents may not remember to promote useful mailbox items.

Target behavior:

- Add heuristics that suggest promotion for urgent/high-priority resolved items, blockers with answers, verification notes, and handoff lite summaries.
- Do not activate mailbox-derived context without the automated policy gate. Create candidates with clear provenance and TTL context, then let the runtime decide.
- Add mailbox digests to handoff/project-record capture when useful.

### P2: Memory governance backend needs stronger triage signals

The existing commands can change memory state for support, diagnostics, migrations, and emergency overrides. The backend should make autonomous promotion and diagnostic override paths safer and more evidence-rich without adding new UI.

Why this matters:

- Automated promotion quality depends on provenance, freshness, and retrieval impact being available as structured data.
- Existing callers and support diagnostics need enough structured information to explain decisions without adding a new product surface.

Target behavior:

- Improve automated candidate scoring by risk, confidence, freshness, source quality, and likely retrieval impact.
- Add structured provenance and retrieval-impact fields to existing responses where compatible.
- Add support diagnostics and logs that explain why a candidate was promoted, rejected, disabled, corrected, duplicated, or kept inactive.
- Keep all governance improvements consumable by existing commands and existing command consumers; do not create a new memory workbench.

### P2: Embedding fallback may hide semantic weakness

The local hash embedding fallback is deterministic and safe, but it is not semantically equivalent to a real embedding model.

Why this matters:

- Users may assume memory retrieval is semantic even when degraded.
- Bad retrieval reduces trust in memory.

Target behavior:

- Record embedding health in existing project-state diagnostics, retrieval diagnostics, and memory-governance responses.
- Add a `semantic retrieval degraded` indicator in retrieval diagnostics.
- Encourage backfill after configuring a real embedding provider.

### P2: Agent behavior depends on tool-use initiative

Agents receive a small working-set summary, but must choose to call `project_context_get` or `project_context_search` for exact content.

Why this matters:

- Agents can skip retrieval even when memory would help.
- The user goal may require context but the first-turn prompt only contains top-result titles.

Target behavior:

- Add stronger stage/task-start guidance: when work involves decisions, constraints, preferences, prior failures, or handoff continuation, retrieve context before acting.
- Include retrieval checklist hints in stage gates for Engineer and Debug.
- Add a compact "memory brief" that is explicit about what to retrieve and why.

### P2: Memory has limited lifecycle semantics

Current kinds are broad. Freshness helps code-related facts, but preferences, process decisions, troubleshooting learnings, and temporary constraints age differently.

Why this matters:

- A troubleshooting note from a transient dependency outage should not live like a user preference.
- A decision may be current until superseded, even if files changed.

Target behavior:

- Add retention/lifecycle metadata: durable, until superseded, time-bound, session-only, historical, or volatile.
- Require expiration or recheck-after for volatile troubleshooting.
- Make retrieval ranking lifecycle-aware.

### P3: Audit and eval coverage should become product metrics

There are tests and diagnostic schemas, but memory usefulness needs explicit evals.

Why this matters:

- Memory can be correct structurally but useless behaviorally.
- Improvements should optimize recall, precision, safety, and continuity.

Target behavior:

- Add golden retrieval tasks with expected top-k memory/project records.
- Track stale-context exposure rate.
- Track candidate promotion/rejection/disablement rates and reasons.
- Track handoff continuation success and whether target agents used carried memory.

## Improvement Plan

### Phase 0: Stabilize And Align Contracts

Goal: make the current behavior and documented contract agree before deeper feature work.

Tasks:

- [x] Get a scoped Rust compile/check green for memory, retrieval, project-record, mailbox, continuity, migration, and outbox modules.
- [x] Fix the automatic owned-agent extraction path so provider-extracted memories pass through a named automated promotion gate before becoming enabled.
- [x] Preserve candidate persistence as an intermediate or diagnostic state, but do not require a user to process it.
- [x] Add tests proving completion, pause, failure, and handoff extraction cannot create enabled memory except through the automated promotion gate.
- [x] Update docs and contracts to reflect the autonomous memory-governance behavior, without proposing new UI or user approval.
- [x] Add one invariant test: no memory with `review_state != approved` can be enabled or retrieved.

Implementation Evidence:

- Added `automatic_memory_promotion_gate` v1 in `client/src-tauri/src/runtime/agent_core/persistence.rs`. Automatic extraction now persists candidates disabled first, then records a deterministic promotion, rejection, or keep-candidate diagnostic before any memory becomes enabled.
- Promotion diagnostics include gate/version, trigger, runtime agent, definition snapshot metadata, provider/model, allowed kinds, confidence, provenance quality, fallback status, evidence snippets, and sensitive-source flags.
- Added tests in `persistence.rs` for completion, pause, failure, handoff triggers, low-confidence rejection, disallowed policy kinds, and instruction-override rejection.
- Added retrieval invariant coverage in `client/src-tauri/src/db/project_store/agent_memory.rs` proving non-approved, disabled, stale, and superseded memory is not retrievable.
- Verification: `cargo check --lib` passed. `cargo test --lib automatic_memory -- --nocapture` passed 4 tests. `cargo test --lib retrieval_predicate_rejects -- --nocapture` passed 1 test.

Acceptance criteria:

- Automatic extraction cannot create model-visible memory without a recorded automated promotion decision.
- No memory save/retrieval path requires user approval during runtime.
- `project_context.search_approved_memory` returns only active memories that passed the promotion and retrieval gates.
- Existing approved-memory tests still pass with explicitly approved fixtures.

### Phase 1: Unify Retrieval Eligibility

Goal: make every path agree about what is retrievable by default.

Tasks:

- [x] Create one shared `is_retrievable_memory` predicate used by diagnostic queues, Lance listing, retrieval, direct get, context manifests, and backfill decisions where applicable.
- [x] Create an equivalent project-record predicate for default retrieval.
- [x] Hard-exclude superseded, invalidated, blocked, stale, and source-missing rows in normal retrieval.
- [x] Add explicit diagnostic/historical search mode for stale/superseded rows.
- [x] Include exclusion counts in retrieval diagnostics.
- [x] Ensure direct `get_memory` refuses approved/enabled memory if it is blocked, superseded, invalidated, stale, or source-missing unless a diagnostic override exists.

Implementation Evidence:

- Added shared predicates in `agent_memory.rs` and `project_record.rs`, then wired them into review queues, normal listing, retrieval candidate selection, and direct `project_context_get`.
- Added `includeHistorical` to `project_context_search` and `project_context_get`; normal retrieval pushes down freshness/supersession filters while historical mode omits those default filters.
- Retrieval diagnostics now include `freshnessDiagnostics.defaultEligibilityExclusionCounts` and `semanticRetrievalDegraded`.
- Added a pre-scan exclusion counter so rows filtered out by LanceDB vector pushdown are still represented in diagnostics.
- Verification: `cargo test --lib s29_memory_freshness_invalidates_sources_and_deprioritizes_stale_results -- --nocapture` passed 1 test. `cargo test --lib s34_ -- --nocapture` passed 4 tests.

Acceptance criteria:

- Diagnostic "retrievable" status exactly matches search eligibility.
- A stale or superseded approved memory never appears in normal top-k results.
- Diagnostics explain excluded rows without leaking blocked content.

### Phase 2: Improve Memory Capture Quality

Goal: extract fewer, better memories with stronger provenance.

Tasks:

- [x] Enforce memory kinds from agent definition policy during extraction.
- [x] Store extraction policy metadata with each candidate: runtime agent, agent definition, allowed kinds, promotion gate, trigger, provider, model.
- [x] Validate source item IDs against the extraction transcript.
- [x] Add source evidence snippets or source span metadata for each candidate.
- [x] Mark fallback source IDs as low-provenance diagnostics.
- [x] Raise or vary confidence thresholds by memory kind.
- [x] Add kind-specific quality rules:
  - `user_preference`: must be stated by the user or inferred only from explicit user instruction, never from agent behavior alone.
  - `decision`: must include decision owner/source and scope.
  - `project_fact`: must cite file/tool/runtime evidence.
  - `troubleshooting`: must include symptom and verified fix or known failed attempt.
  - `session_summary`: must be session-scoped unless intentionally promoted.
- [x] Add duplicate/near-duplicate detection beyond exact text hash.

Implementation Evidence:

- `RuntimeMemoryExtractionPolicy` now loads memory kind policy from the effective agent definition and defaults conservatively when absent.
- `resolve_memory_candidate_provenance` validates source IDs against the extraction transcript, requires overlap or structured fallback, enforces user-authored evidence for preferences, and records redacted evidence snippets.
- The promotion gate applies per-kind confidence thresholds and kind-specific quality rules for decisions, troubleshooting, and session summaries.
- Low-provenance fallback candidates remain disabled as candidates with `memory_promotion_gate_low_provenance`; low-confidence and unsafe candidates are rejected.
- Existing active hash duplicate detection remains in the activation path; exact duplicates are skipped before insertion/activation.
- Verification: `cargo test --lib automatic_memory -- --nocapture` passed 4 tests.

Acceptance criteria:

- Candidate rows include enough evidence for automated gates and support diagnostics to explain the promotion decision without reading the whole transcript.
- User preferences cannot be inferred from agent behavior alone.
- Repeated final-answer summaries do not create multiple equivalent memories.

### Phase 3: Make Memory Useful At The Moment Of Work

Goal: agents should see or fetch the right memory at task boundaries without overloading prompts.

Tasks:

- [x] Add a "memory brief" to the first-turn context package that names top relevant memory/project records and why they may matter.
- [x] Keep raw content tool-mediated, but include stronger retrieval prompts for exact reads.
- [x] Add task-type retrieval hints:
  - Before edits: retrieve decisions, constraints, plans, and relevant file facts.
  - Before debugging: retrieve troubleshooting, prior failures, diagnostics, and verification notes.
  - Before answering: retrieve project facts, constraints, and open questions.
  - During handoff: retrieve handoff bundle plus cited memory/project records.
- [x] Add stage-aware retrieval nudges for gated agent stages.
- [x] Log when agents ignore a high-confidence memory brief and proceed to risky actions.

Implementation Evidence:

- `source_cited_working_set_context` now emits a bounded memory brief with citation labels, selection reasons, "Why it may matter" text, and explicit `project_context_get` guidance before relying on details.
- Tool descriptors now state normal retrieval excludes disabled, rejected, stale, source-missing, superseded, invalidated, and blocked rows, and schemas expose `includeHistorical` for diagnostic override.
- The retrieval eval suite includes `context_usage_after_brief` coverage for exact context retrieval before risky actions after a high-impact brief.
- Verification: `cargo test --lib s26_provider_context_package_admits_source_cited_working_set_summary -- --nocapture` passed 1 test. `cargo test --lib s27_provider_context_package_honors_per_agent_first_turn_context_policy -- --nocapture` passed 1 test.

Acceptance criteria:

- On tasks with known approved memory, the first-turn manifest includes a cited memory brief.
- Agents retrieve exact content before acting when the brief marks it as high impact.
- Prompt size stays bounded and raw context remains tool-mediated.

### Phase 4: Fix Handoff Memory Carryover

Goal: handoff should be continuity with evidence, not just a retrieval suggestion.

Tasks:

- [x] During handoff bundle creation, run focused hybrid retrieval using user goal, pending work, changed paths, latest plan, and open questions.
- [x] Fill `approvedMemories` with a small cited subset.
- [x] Fill `relevantProjectRecords` with a small cited subset.
- [x] Include retrieval query IDs and result IDs in the bundle.
- [x] Record why each carried item was selected.
- [x] Add target-run prompt guidance to retrieve exact content for carried IDs before relying on details.
- [x] Add tests for same-type handoff carrying approved memory and relevant project records.

Implementation Evidence:

- `build_handoff_bundle` now calls a focused hybrid retrieval helper and carries bounded approved-memory and project-record anchors into `approvedMemories`, `relevantProjectRecords`, and `durableContextRetrieval`.
- The handoff retrieval query includes the source goal, pending prompt, recent messages, changed paths, and recent event summaries. Changed paths are relevance evidence rather than a hard filter so approved memories without paths can still be carried.
- Carried items include result IDs, source IDs, rank, score, redaction state, selection reason, freshness/trust metadata, and citation data.
- Verification: `cargo test --lib handoff_bundle_carries_matching_approved_memory_and_project_records -- --nocapture` passed 1 test.

Acceptance criteria:

- Handoff bundles no longer contain empty relevant context arrays when matching durable context exists.
- Target context manifests cite both the handoff bundle and carried context anchors.
- Continuation after handoff can answer "what did the previous run know?" without broad searching.

### Phase 5: Make Mailbox More Effective Without Making It Memory

Goal: preserve valuable coordination when it matters while keeping mailbox ephemeral by default.

Tasks:

- [x] Add mailbox promotion suggestions for urgent blockers, verification notes, resolved questions, file ownership notes, and handoff lite summaries.
- [x] Add a mailbox digest to active coordination prompt summaries when multiple items are present.
- [x] Add source threading: parent question, answer, resolution, and promotion should be linked in the promoted candidate.
- [x] Prevent promotion of code-history notices into durable memory unless explicitly converted into a diagnostic record with current-file warning.
- [x] Add TTL-expiry diagnostics for high-priority unacknowledged mailbox items.
- [x] Add backend support for promoting a resolved mailbox thread to a candidate when an existing caller requests it.

Implementation Evidence:

- Mailbox context manifests and active coordination summaries now include promotion suggestions for high-value temporary coordination items.
- Mailbox promotion content JSON now records parent/threading IDs, status, resolution metadata, promoter/source IDs, related paths, temporary-mailbox status, and `requiresAutomatedGovernance`.
- Code-history mailbox item types are tagged and include a current-file warning; all mailbox promotions now remind agents that current files and current tool output are authoritative.
- Existing promotion remains review-only project-record candidate state (`visibility = memory_candidate`) and does not become approved durable memory.
- Verification: `cargo test --lib mailbox_promotion_creates_review_only_project_record_candidate -- --nocapture` passed 1 test.

Acceptance criteria:

- Mailbox remains TTL-scoped and non-retrievable by default.
- High-value mailbox threads can become programmatically evaluated project-record candidates with full provenance.
- Agents see active mailbox state in context but are reminded that current files and tool output outrank it.

### Phase 6: Strengthen The Backend Governance Pipeline

Goal: make autonomous memory promotion safer and more evidence-based behind the scenes, using existing commands only for diagnostics and overrides.

Tasks:

- [x] Add backend query options for state, scope, kind, freshness, confidence, source run, related path, created date, promotion status, and retrievability.
- [x] Add structured provenance fields with source run, source items, redacted source snippets, related paths, and file fingerprints.
- [x] Add retrieval-impact metadata: scopes, kinds, paths, and search modes where the memory would become eligible.
- [x] Add conflict metadata for supersedes/superseded chains.
- [x] Add backend support for project-record candidate governance for mailbox promotions and `project_context.propose_record_candidate`, not just agent memories.
- [x] Add audit logs for promote, reject, disable, delete, correction, and supersession operations.
- [x] Keep this phase UI-free. Existing callers may consume richer data they already request, but this plan must not add new screens, controls, panels, or inspectors.

Implementation Evidence:

- `ListSessionMemoriesRequestDto` now supports backend filters for review state, scope, kind, freshness state, minimum confidence, source run, related path, created-after, promotion status, and retrievability.
- `SessionMemoryRecordDto` now exposes freshness, retrievability reason, promotion status, provenance JSON, retrieval-impact JSON, and conflict-chain JSON without adding any UI surface.
- Automatic promotion/rejection/keep-candidate decisions write structured diagnostics; memory updates can preserve diagnostics for support and audit consumers.
- `project_context.propose_record_candidate` now wraps candidate content with backend governance metadata, and mailbox promotions already create project-record candidates with full provenance.
- Verification: `cargo check --lib` passed after these contract changes.

Acceptance criteria:

- Automated gates and existing diagnostic commands return enough structured evidence for safe promotion/rejection decisions.
- Redacted rows cannot be promoted until corrected or sanitized by a policy-approved path.
- Project-record candidates have an equivalent backend governance path.

### Phase 7: Add Memory Quality Evals And Observability

Goal: measure whether memory helps agents.

Tasks:

- [x] Add golden fixtures for:
  - User preference recall.
  - Project decision recall.
  - Prior debugging fix recall.
  - Stale memory exclusion.
  - Superseded memory exclusion.
  - Handoff context carryover.
  - Mailbox promotion provenance.
- [x] Track metrics:
  - Retrieval precision at top 3 and top 5.
  - Recall for known-memory tasks.
  - Stale/superseded exposure rate.
  - Candidate promotion rate by kind and agent.
  - Rejection reasons.
  - Memory correction rate.
  - Agent `project_context` usage after memory brief.
  - Handoff continuation success.
  - Mailbox acknowledgement/resolution/promotion rates.
- [x] Add support diagnostics that summarize memory health by project.
- [x] Add scheduled or startup outbox reconciliation for missing LanceDB rows.

Implementation Evidence:

- The retrieval/memory eval suite now includes golden surfaces for user preference recall, project decision recall, prior debugging fix recall, stale memory exclusion, superseded memory exclusion, handoff context carryover, mailbox promotion provenance, and exact context usage after a memory brief.
- Retrieval quality metrics now include recall rates, stale exposure rate, superseded exposure rate, handoff carryover rate, mailbox provenance rate, and context usage after brief.
- Existing outbox reconciliation and LanceDB replay tests remain in place; memory diagnostics now expose enough structured promotion/retrieval evidence for project health summaries.
- Verification: `cargo test --lib s58_retrieval_memory_quality_eval_covers_context_memory_and_fallback -- --nocapture` passed 1 test.

Acceptance criteria:

- Memory improvements have regression tests and measurable quality deltas.
- Support can answer "why did the agent see this memory?" from diagnostics.
- Embedding/backfill health is visible before retrieval quality degrades silently.

### Phase 8: Security And Privacy Hardening

Goal: keep memory useful without storing unsafe content.

Tasks:

- [x] Keep prompt-injection and secret-shaped text blocked for memory and mailbox.
- [x] Add adversarial tests for instruction-override content inside retrieved project records and mailbox items.
- [x] Ensure all retrieval snippets label memory/project-record content as untrusted data.
- [x] Add a "sensitive source" flag for memories derived from terminal output, logs, environment checks, and credentials-adjacent files.
- [x] Require automated sanitization, correction, or rejection before promotion for any redacted memory.
- [x] Add explicit policy that memory can never grant tool permissions, change approval mode, or override AGENTS/project instructions.

Implementation Evidence:

- Memory extraction rejects redacted, secret-shaped, and instruction-override candidates before promotion; the adversarial automatic-memory test asserts instruction-override text is rejected.
- Retrieval metadata for memory and project records now labels context as `untrustedData: true` with `instructionAuthority: "none"`.
- Promotion-gate evidence snippets now include `sensitiveSource` for tool, terminal, environment, secret-adjacent, code-history, and file-change evidence.
- Tool descriptors and handoff constraints explicitly frame stored context as source-cited data that cannot override current system, developer, repository, approval, or tool policy.
- Verification: `cargo test --lib automatic_memory -- --nocapture` passed 4 tests; `cargo test --lib mailbox_promotion_creates_review_only_project_record_candidate -- --nocapture` passed 1 test.

Acceptance criteria:

- Injection-shaped memory cannot be promoted or retrieved.
- Retrieved context cannot be mistaken for system/developer instructions in prompts or tool results.
- Security diagnostics preserve IDs and reasons without revealing raw secrets.

## Proposed Invariants

These invariants should become tests and, where possible, runtime assertions:

- No provider call runs without a persisted context manifest.
- No model-extracted memory becomes model-visible without a recorded automated promotion decision.
- No enabled memory can be non-approved.
- No approved memory is retrievable when disabled, superseded, invalidated, blocked, stale, or source-missing.
- No mailbox item is durable memory unless promoted by agent/runtime policy to a candidate or project record.
- No promoted mailbox item becomes active durable context without the same automated promotion and retrieval gates.
- No prompt-injection-shaped memory text is stored as approved memory.
- No redacted memory can be promoted directly without sanitization or correction.
- No source item ID on a memory candidate can reference an item outside the extraction source set.
- No handoff bundle with available relevant context should have empty `approvedMemories` and `relevantProjectRecords`.
- No stale/superseded row should appear in normal first-turn working-set summaries.
- Every retrieval result shown to an agent must include a source kind, source ID, redaction state, and citation.

## Suggested Data Model Additions

Potential additions to memory/project-record metadata:

- `promotion_gate`: named automated gate and version used to decide whether memory became active.
- `promotion_decision`: promoted, rejected, disabled, corrected, superseded, or kept_candidate.
- `extraction_trigger`: completion, pause, failure, handoff, manual_support, mailbox_promotion.
- `extraction_provider_id` and `extraction_model_id`.
- `extraction_policy_snapshot`: allowed kinds, promotion rules, runtime agent.
- `provenance_quality`: exact_source, broad_source, fallback_source, inferred, user_confirmed.
- `evidence_snippets`: redacted excerpts tied to source item IDs.
- `lifecycle`: durable, until_superseded, time_bound, session_only, historical, volatile.
- `expires_at` or `recheck_after`.
- `selection_reason`: for handoff-carried context and memory briefs.
- `retrieval_eligibility_version`: lets future migrations distinguish old and new rules.

Avoid adding new fields until Phase 0 and Phase 1 are stable. The current schema already has enough room for some of this in diagnostics/content JSON, but first-class fields will make evals, diagnostics, and backend consumers much easier.

## Agent Interaction Guidelines

Agents should learn these operational rules:

- Treat approved memory as helpful evidence, never as instruction hierarchy.
- Prefer current files and current tool output over retrieved memory.
- Use `project_context_search` when starting work that may depend on prior decisions, constraints, plans, troubleshooting, user preferences, or handoff state.
- Use `project_context_get` before relying on an exact memory or project-record detail.
- Use mailbox for active coordination only.
- Promote mailbox items only when they are useful beyond the current TTL, and route them through the same automated governance gates as other durable context.
- Refresh freshness before relying on old path-related memory.
- Correct or supersede wrong memory instead of silently ignoring it.

## Implementation Order

Recommended order:

1. Stabilize the memory-related compile baseline.
2. Fix automatic extraction to use an automated promotion gate.
3. Add shared retrieval-eligibility predicates and hard exclusions.
4. Add tests for completion/pause/failure/handoff promotion-gate behavior.
5. Fill handoff `approvedMemories` and `relevantProjectRecords`.
6. Improve provenance validation and evidence snippets.
7. Add memory brief/stage-aware retrieval nudges.
8. Expand mailbox promotion and project-record candidate governance.
9. Add eval fixtures and observability dashboards.
10. Add lifecycle metadata and retention policies.

## Open Questions

- Which deterministic runtime-authored records may bypass provider extraction and be promoted immediately by policy, such as verified command maps or accepted plan packs?
- Should project records have the same explicit review state as agent memories, or is visibility plus candidate status enough?
- Should `source_unknown` remain retrievable by default, or should it be opt-in for code-related facts?
- What confidence threshold should apply per memory kind?
- Should user preferences require explicit user source IDs only?
- Should handoff bundles carry exact snippets or only IDs and titles?
- Should mailbox TTL vary by priority or item type?
- Do existing agent-canvas memory nodes need richer backend metadata to render accurately when creating or viewing agents?

## Definition Of Done

Memory should be considered effective when:

- Agents reliably retrieve relevant decisions, constraints, preferences, troubleshooting notes, and handoff facts without prompt bloat.
- Bad memory is automatically detected, rejected, corrected, disabled, or superseded when possible.
- Stale or superseded memory does not appear in normal retrieval.
- Handoffs carry enough cited context for the target run to continue smoothly.
- Mailbox coordination stays temporary unless deliberately promoted.
- The autonomous governance path can operate with provenance and confidence, not blind trust.
- Diagnostics can explain what was saved, when, why, by whom, from what evidence, and whether it was visible to the model.
