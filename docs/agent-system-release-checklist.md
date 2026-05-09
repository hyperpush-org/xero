# Agent System Release Checklist

This checklist gates release of the improved agent system. It distinguishes shipped backend/runtime contracts from deferred product UI so support, docs, and implementation work do not drift.

## Current UI Constraint

No new user-facing UI is part of the current implementation pass. Slices that require visible controls, previews, panels, dialogs, notices, or inspectors remain deferred until UI work is explicitly allowed. When UI work resumes, use existing ShadCN patterns and do not add temporary debug/test surfaces.

## Completion Audit Gate

Before declaring this plan complete, compare release evidence against the prompt-to-artifact completion audit and Good Enough Coverage Audit in `AGENT_SYSTEM_IMPROVEMENT_PLAN.md`. Those audits are the current source of truth for unchecked slices, accepted backend-only evidence, deferred UI surfaces, S70 dogfood status, final production-readiness criteria, and final verification gaps. Do not treat backend contracts, green shared-model tests, or this checklist as completion evidence unless they cover the visible UI, user-facing tests, dogfood runs, and final verification named in those audits.

## Reusable Verification Commands

Run these scoped checks before updating release status. Keep Cargo commands serialized.

```bash
pnpm run agent-system:verify-plan
pnpm --dir ./client exec vitest run src/lib/xero-model/agent.test.ts src/lib/xero-model/agent-definition.test.ts src/lib/xero-model/workflow-agents.test.ts src/lib/xero-model/agent-extensions.test.ts src/lib/xero-model/agent-reports.test.ts src/lib/xero-model/session-context.test.ts src/lib/xero-model/project-state.test.ts src/lib/xero-model/project-records.test.ts src/lib/xero-model/runtime-protocol.test.ts
cargo fmt --manifest-path client/src-tauri/Cargo.toml --check
cargo test --manifest-path client/src-tauri/Cargo.toml --lib
git diff --check -- AGENT_SYSTEM_IMPROVEMENT_PLAN.md docs/agent-runtime-continuity.md docs/agent-system-release-checklist.md docs/agent-system-dogfood-notes.md client/src-tauri/src/runtime/agent_core/harness_contract.rs client/src/lib/xero-model/agent.ts client/src/lib/xero-model/agent.test.ts
rg -n '[[:blank:]]$' AGENT_SYSTEM_IMPROVEMENT_PLAN.md docs/agent-runtime-continuity.md docs/agent-system-release-checklist.md docs/agent-system-dogfood-notes.md client/src-tauri/src/runtime/agent_core/harness_contract.rs client/src/lib/xero-model/agent.ts client/src/lib/xero-model/agent.test.ts
```

Also verify that every checked slice in `AGENT_SYSTEM_IMPROVEMENT_PLAN.md` includes completed behavior, runtime/storage contract, verification, and rollout consequence evidence; every unchecked slice has an explicit deferral or remaining-work reason; every unchecked slice id appears in this checklist and in `docs/agent-system-dogfood-notes.md`; and every Good Enough criterion has a coverage-audit row.

## Custom Agent Gates

- Visual builder round-trip preserves the canonical custom-agent contract without dropping or inflating graph fields.
- Detail hydration reads saved custom definitions before runtime defaults.
- Granular tool policy, output contracts, database touchpoints, consumed artifacts, memory policy, retrieval policy, and handoff policy are persisted and runtime-effective.
- Unsupported or partial custom-agent definitions fail closed or are explicitly reset. There is no silent backwards-compatibility upgrade path unless one is deliberately added.
- Blocked, revoked, invalid, archived, or draft custom agents cannot start new production runs.

## Runtime Fidelity Gates

- Provider prompts and final-response checks honor the pinned custom-agent definition version.
- Activation preflight rejects invalid runtime combinations before provider submission.
- Consumed artifact preflight blocks missing required artifacts with a user-fixable diagnostic.
- Prompt hierarchy tests prove custom text, memory, retrieved records, and tool output cannot override Xero policy, tool gates, approvals, or redaction.

## Context And Handoff Gates

- First-turn context includes source-cited working-set material without preloading bulk durable records as higher-priority prompt text.
- Handoff bundles include the completeness contract for every runtime type.
- Handoff lineage reconciliation recovers from partial bundle write, lineage update, target-run creation, and source-run marking.
- Target manifests prove handoff bundle, working-set summary, source-cited continuity records, and pending prompt were available before the first provider call.
- Handoff comparison diagnostics flag runtime, provider, model, definition, tool-policy, and context-policy drift.

## Storage And Retrieval Gates

- SQLite project stores enforce connection pragmas, startup integrity checks, migrations, backup, restore, repair, and app-data-only storage.
- Cross-store writes use an outbox and reconciliation sweep for SQLite/LanceDB coordination.
- LanceDB schema drift preserves data through quarantine instead of destructive reset.
- Retrieval diagnostics identify normal hybrid, hybrid-degraded, and keyword-fallback modes.
- Trust scoring, contradiction state, freshness, provenance, and redaction influence retrieval results.
- Performance budgets exist for project open, agent selection, custom detail load, retrieval, memory-review queries, handoff preparation, and startup diagnostics.

## Security And Audit Gates

- Risky capabilities have backend permission explanations.
- Capability revocation can disable custom agents, tool packs, external integrations, browser-control grants, and destructive-write grants without deleting historical audit records.
- Runtime audit export reconstructs the run/session identity, effective prompt sections, tool policy, memory policy, retrieval policy, output contract, handoff policy, pinned definition version, context manifests, and risky capability approvals.
- Support diagnostics bundles are redacted before return, include only bounded runtime audit-event summaries, and do not expose raw transcripts, secrets, raw tool payloads, or hidden prompts.

## Backend Evidence Gates While UI Is Deferred

These backend contracts are release evidence while product surfaces are intentionally deferred. They do not make the related UI slices complete.

- Run-start explanation proves the selected definition version, run session, provider/model, approval mode, tool policy, memory policy, retrieval policy, output contract, handoff policy, and risky capability explanations before a run starts.
- Knowledge inspection proves which project records, continuity records, approved memory, and handoff records can influence an agent without exposing blocked records or raw payloads. Run-scoped inspection must resolve approved memory through the run's agent session, scope handoff lineage to the effective run/session, report the retrieval-policy source plus applied record-kind and memory-kind filters, and return records that match those filters.
- Effective-runtime preview, saved-version diff, graph validation summary, and graph repair hints prove custom-agent authoring contracts before visible preview, diff, and repair UI exist.
- Database-touchpoint explanation proves saved read, write, and encouraged touchpoints can be inspected from definition data without relying on raw runtime manifests.
- Memory review queue, memory update, memory correction, and memory delete contracts prove review, approval, rejection, disablement, correction provenance, supersession, and retrieval exclusion behavior before the permanent memory UI exists.
- Project-record delete and supersede contracts prove stale project facts can be removed or linked to corrected facts before the permanent correction UI exists.
- Handoff summary proves carried context, omitted context, redaction, lineage, provider, and safety rationale can be inspected before a visible handoff notice exists.
- Tool-pack catalog and extension-manifest validation prove capability boundaries and manifest safety before visible builder pickers or fixture controls exist.
- Support diagnostics bundle proves failure-area classification and redaction before a permanent support-report surface exists.
- Project-state backup, restore, and repair contracts prove app-data-only recovery behavior before visible backup/restore controls exist.

## Deferred UI Exit Gates

Before claiming product readiness, revisit every unchecked UI-deferred slice and decide one of two outcomes: implement the visible surface with user-facing tests, or explicitly accept a backend-only release with the missing surface listed as a product limitation.

- Visual authoring must eventually show profile-aware availability, validation, diffs, templates, generated editable graphs, graph repair hints, and granular policy controls for S04, S07, S08, S09, S10, S11, S12, S13, S20, S21, S25, S62, and S63.
- Runtime transparency must eventually show effective-runtime preview, run-start explanation, capability permission explanations, database-touchpoint explanations, knowledge inspection, and handoff summaries for S15, S46, S52, S61, S64, and S66.
- User control must eventually show memory review/correction, project-fact correction, backup/restore/repair, and support diagnostics surfaces for S28, S43, S61, and S65.
- Dogfood must record whether each deferred surface blocked, slowed, or did not affect engineering, debugging, planning, repository reconnaissance, support triage, and long-running handoff workflows for S70.

## Unchecked Slice Coverage

These slice ids are intentionally still release-blocking unless their visible surface is implemented with user-facing tests or a backend-only product limitation is explicitly accepted: S04, S07, S08, S09, S10, S11, S12, S13, S15, S20, S21, S25, S28, S43, S46, S52, S61, S62, S63, S64, S65, S66, and S70.

## Backend-Only Acceptance Decision

No backend-only acceptance decision is recorded as of the current audit. To accept a backend-only release, record the decision owner, date, explicit product limitations, unchecked slice ids being waived, replacement dogfood scope, and follow-up owner here and in `docs/agent-system-dogfood-notes.md`. Without that record, every unchecked slice above remains release-blocking.

## Documentation Gates

- README and docs describe shipped behavior, degraded modes, and deferred UI accurately.
- Memory review, effective-runtime preview, handoff visibility, support diagnostics UI, and product-finish surfaces are not described as shipped until they exist.
- Release notes call out any reset/fail-closed behavior for custom-agent definitions.
- Dogfood notes in `docs/agent-system-dogfood-notes.md` cover engineering, debugging, planning, repository reconnaissance, support triage, and long-running handoff workflows.
