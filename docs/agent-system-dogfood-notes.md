# Agent System Dogfood Notes

This document is the durable S70 dogfood record for the improved agent system. It is intentionally separate from the release checklist so real workflow findings can be recorded without rewriting the gate itself.

## Status

Dogfood is not complete yet. The current implementation pass is backend-only because new user-facing UI is explicitly deferred. Do not mark S70 complete until either the deferred UI surfaces are implemented or product leadership explicitly accepts a backend-only dogfood pass with the missing surfaces called out as product gaps.

## Completion Audit Snapshot

Current objective: implement `AGENT_SYSTEM_IMPROVEMENT_PLAN.md`.

The backend-only implementation pass has durable evidence for the non-UI contracts listed in the backend evidence map below, but the overall objective is not complete. The plan still has unchecked UI-deferred slices in visual authoring, runtime transparency, user control, backup/repair controls, support diagnostics, and product-finish flows. The release checklist requires either visible surfaces with user-facing tests or an explicit backend-only product acceptance decision before product readiness can be claimed.

The prompt-to-artifact completion audit in `AGENT_SYSTEM_IMPROVEMENT_PLAN.md` is the source of truth for the current blockers. Update that audit, this dogfood record, and the release checklist together whenever a deferred UI surface is implemented, waived by product decision, or proven by representative workflow evidence.

Completion blockers as of this snapshot:

- S70 has no representative workflow runs recorded.
- Deferred UI surfaces are still marked `Not assessed` in the table below.
- No product decision accepting a backend-only release is recorded in this document or the release checklist.
- Unchecked slices in `AGENT_SYSTEM_IMPROVEMENT_PLAN.md` must remain unchecked unless their visible UI acceptance gates are implemented or explicitly waived.

## Backend-Only Acceptance Decision

No backend-only acceptance decision is recorded. If product accepts a backend-only release before the deferred UI surfaces exist, record the decision owner, date, accepted limitations, unchecked slice ids being waived, replacement dogfood scope, and follow-up owner here and in `docs/agent-system-release-checklist.md`. Until then, the required workflows below remain unrun and S70 remains incomplete.

## Required Workflows

Each workflow must use a real project, realistic context limits, and the ordinary Xero runtime path. Record commands, run ids, custom definition ids, context manifest ids, handoff ids, support diagnostic bundle ids, and any manual context the user had to provide.

| Workflow | Primary question | Required evidence | Result |
| --- | --- | --- | --- |
| Engineering | Can an Engineer agent continue from saved project context, make a scoped change, and verify it without re-describing prior work? | Run id, context manifest, retrieved records/memory, changed files, verification command output. | Not run |
| Debugging | Can a Debug agent recover symptom, reproduction, hypotheses, root cause, fix, and verification from durable context and handoff evidence? | Run id, debug context package, retrieval log, handoff bundle if triggered, final verification output. | Not run |
| Planning | Can a planning-oriented custom agent honor its saved prompt, output contract, retrieval policy, and handoff policy? | Custom definition id/version, activation preflight result, prompt/output contract evidence, produced plan artifact. | Not run |
| Repository reconnaissance | Can an Ask agent answer project-structure questions using source-cited working-set context without mutating project state? | Run id, context manifest, retrieval diagnostics, cited files or records, mutation audit absence. | Not run |
| Custom support triage | Can diagnostics explain a simulated provider/runtime/storage/retrieval problem without exposing secrets or hidden prompts? | Support diagnostics bundle, redaction evidence, failure area classification, audit export reference. | Not run |
| Long-running handoff | Can a context-exhausted run hand off to a same-type target and continue without the user restating the task? | Source run id, target run id, handoff lineage, bundle quality score, target first-turn context manifest. | Not run |

## Backend Evidence Collection Map

Use these backend contracts while the permanent UI surfaces are deferred. They are evidence sources, not replacements for the later user-facing product work.

| Evidence need | Backend source | Expected proof |
| --- | --- | --- |
| Run-start configuration | `get_agent_run_start_explanation` / `xero.agent_run_start_explanation.v1` | Definition id/version, run session, runtime agent, provider/model, approval, tool, memory, retrieval, output, and handoff policy match the run manifest. |
| Agent knowledge before a run | `get_agent_knowledge_inspection` / `xero.agent_knowledge_inspection.v1` | Retrieval-visible project records, continuity records, approved memory, and handoff records are current, redaction-safe, and filtered by the supplied run's `retrievalPolicy.recordKinds` and `retrievalPolicy.memoryKinds` when `runId` is provided. |
| Effective custom-agent preview | `preview_agent_definition` / `xero.agent_definition_preview_command.v1` | Compiled prompt, effective tools, denied capabilities, validation summary, and repair hints match what the runtime would load. |
| Saved definition diff | `get_agent_definition_version_diff` / `xero.agent_definition_version_diff.v1` | Prompt, policy, tool, memory, retrieval, handoff, output, database, consumed-artifact, workflow, and safety-limit changes are derived from saved versions. |
| Database-touchpoint explanation | `get_agent_database_touchpoint_explanation` / `xero.agent_database_touchpoint_explanation.v1` | Read/write/encouraged touchpoint counts and table/purpose/column/trigger metadata match the saved definition. |
| Memory review and correction | `get_session_memory_review_queue`, `update_session_memory`, `correct_session_memory`, `delete_session_memory` | Approved/enabled memory is retrievable, rejected/disabled/superseded memory is excluded, corrected memory cites provenance, and raw secret-like text stays hidden. |
| Project fact correction | `delete_project_context_record`, `supersede_project_context_record` | Deleted facts disappear from retrieval; superseded stale facts point to corrected records with an invalidation reason. |
| Handoff visibility | `get_agent_handoff_context_summary` / `xero.agent_handoff_context_summary.v1` | Carried, omitted, redacted, source, target, provider, and safety-rationale fields explain continuation readiness without raw payloads. |
| Tool-pack and extension safety | `get_agent_tool_pack_catalog`, `validate_agent_tool_extension_manifest` | Tool-pack manifests/health and extension-manifest validation expose policy boundaries without creating a builder UI. |
| Support triage | `get_agent_support_diagnostics_bundle` / `xero.agent_support_diagnostics_bundle.v1` | Failure areas classify storage, retrieval, memory, handoff, runtime-policy, and visual-builder issues with redacted diagnostics; run-scoped bundles include the runtime audit run/session reference and audit-event summaries when available. |
| Backup/repair readiness | `create_project_state_backup`, `restore_project_state_backup`, `repair_project_state` | App-data-only backups, pre-restore snapshots, repair health, Lance health, and outbox reconciliation are recorded without legacy `.xero/` state. |

## Questions To Answer

- Did the user need to re-describe the active task, constraints, or prior decisions?
- Did the selected or custom agent behave as authored, including prompt, tools, output contract, memory policy, retrieval policy, and handoff policy?
- Did retrieval use current, source-cited, non-redacted evidence instead of stale, contradicted, blocked, or reverted facts?
- Did diagnostics explain failures well enough for support without raw transcripts, secrets, raw tool payloads, or hidden prompts?
- Did deferred UI surfaces block the workflow, merely slow it down, or have no effect?

## Deferred UI Surface Check

During dogfood, record whether each missing surface was needed:

| Deferred surface | Related slices | Dogfood impact |
| --- | --- | --- |
| Granular policy editor | S04 | Not assessed |
| Profile-aware authoring catalog | S07 | Not assessed |
| Effective-runtime preview and database-touchpoint explanation | S08, S15, S17, S52, S66 | Not assessed |
| Memory/retrieval/handoff builder controls | S09 | Not assessed |
| Visual validation, diff, templates, generated editable graphs, and repair | S10, S11, S12, S13, S25, S62, S63 | Not assessed |
| Extension tools and user-configurable tool packs | S20, S21 | Not assessed |
| Memory review and correction UI | S28, S65 | Not assessed |
| Backup/restore/repair controls | S43 | Not assessed |
| Handoff/context inspection | S46, S64 | Not assessed |
| Support report surface | S61 | Not assessed |

## Run Log

No dogfood runs have been recorded yet.

## Backend-Only S64 Probe

Until the permanent "what this agent knows" UI exists, dogfood runs that evaluate pre-run knowledge should record the request payload used for `get_agent_knowledge_inspection`, including `projectId`, optional `agentSessionId`, optional `runId`, and `limit`. When `runId` is present, confirm `agentSessionId` resolves to that run's session when omitted, then record the response `retrievalPolicy.source`, `recordKindFilter`, `memoryKindFilter`, and `filtersApplied` fields. Confirm excluded record or memory kinds are absent, unrelated handoff lineage from other sessions/runs is absent, and redacted text plus raw handoff bundles remain hidden.
