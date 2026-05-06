# Agents Core Harness Improvement Plan

Reader: internal Xero engineer working on the owned-agent runtime.

Post-read action: implement the next harness improvement slice with enough context to preserve the existing safety model while making tools, prompts, traces, and evaluations more reliable.

Last reviewed: 2026-05-05.

## Decision

Invest first in a single, typed harness contract for agent prompts, tool capabilities, action-level risk, provider preflight, and trace evidence. The current harness already has strong building blocks: built-in agent profiles, fragment-based prompt compilation, Tool Registry V2 dispatch, app-data project context, environment lifecycle checks, subagent coordination, and a deterministic Test agent. The largest weakness is that several surfaces still rely on coarse tool descriptors, keyword-based exposure, and prompt text to communicate boundaries that should be enforced by typed contracts.

The plan should bias toward breaking cleanup now because Xero is a new application and backwards compatibility is not required unless explicitly requested.

## Current Harness Map

The owned-agent harness is shaped around five built-in agents:

| Agent | Purpose | Prompt policy | Tool authority | Gates |
| --- | --- | --- | --- | --- |
| Ask | Answer project questions without repository or process mutation. | Observe-only answer contract with durable context retrieval. | Observe tools plus filtered tool discovery. | No plan gate, no verification gate. |
| Engineer | Implement repository changes. | Production coding workflow with inspect, plan when needed, edit, verify, summarize. | Engineering tools, subject to approval mode, agent policy, registry, sandbox, and tool runtime policy. | Plan gate for complex work, verification gate after file changes. |
| Debug | Investigate and fix failures. | Structured debugging workflow with evidence, hypotheses, reproduction, root cause, fix, verification, memory. | Same broad engineering surface, optimized for diagnostics. | Plan gate for debugging work, verification gate after file changes. |
| Agent Create | Interview and save custom agent definitions. | Least-privilege definition design and validation workflow. | Read-only project context and `agent_definition` registry state only. | No code execution gate. Registry mutations require approval. |
| Test | Run deterministic harness validation. | Canonical ordered manifest of tool groups, scratch-only mutation, exact final report shape. | Harness-safe engineering surface. | Harness order gate, no normal verification gate. |

The provider prompt is versioned as `xero-owned-agent-v1` and compiled from fragments:

- Selected Soul settings.
- Built-in agent system policy.
- Active tool policy and currently exposed tool names.
- Custom agent definition policy for non-built-in definitions.
- Repository instruction files.
- Generated project code map.
- Invoked skill context and supporting assets.
- Xero-owned process state.
- Durable project context guidance.
- Active agent coordination summary.

The provider context package persists a manifest containing prompt fragments, provider messages, tool descriptors, retrieval metadata, coordination metadata, provider preflight, context hash, token estimates, and context policy pressure. Durable project memory is intentionally tool-mediated instead of raw-preloaded.

Provider adapters currently target:

- OpenAI-compatible chat completions.
- OpenAI Responses.
- OpenAI Codex Responses.
- Anthropic Messages.
- Bedrock Anthropic through AWS CLI.
- Vertex Anthropic.
- Fake provider fixtures.

Tool execution currently flows through a legacy descriptor/decode layer into Tool Registry V2. V2 supplies descriptor validation, read-only parallel grouping, mutating sequential grouping, policy, sandbox evaluation, rollback hooks, group timeouts, repeated-call limits, failure categories, truncation metadata, and dispatch reports.

## Tool Inventory

Current built-in and dynamic tool surfaces:

| Surface | Tools |
| --- | --- |
| Core inspection and runtime state | `read`, `search`, `find`, `git_status`, `git_diff`, `list`, `file_hash`, `tool_search`, `tool_access`, `project_context`, `workspace_index`, `agent_coordination`, `todo` |
| File mutation | `edit`, `write`, `patch`, `delete`, `rename`, `mkdir`, `notebook_edit` |
| Commands and processes | `command`, `command_session_start`, `command_session_read`, `command_session_stop`, `process_manager`, `powershell` |
| Diagnostics and environment | `environment_context`, `system_diagnostics`, `macos_automation` |
| Web and browser | `web_search`, `web_fetch`, `browser` |
| MCP, skills, and agents | `mcp`, dynamic `mcp__...` tools, `skill`, `subagent`, `agent_definition` |
| Emulator | `emulator` |
| Solana | `solana_cluster`, `solana_logs`, `solana_tx`, `solana_simulate`, `solana_explain`, `solana_alt`, `solana_idl`, `solana_codama`, `solana_pda`, `solana_program`, `solana_deploy`, `solana_upgrade_check`, `solana_squads`, `solana_verified_build`, `solana_audit_static`, `solana_audit_external`, `solana_audit_fuzz`, `solana_audit_coverage`, `solana_replay`, `solana_indexer`, `solana_secrets`, `solana_cluster_drift`, `solana_cost`, `solana_docs` |

Tool access is currently hybrid:

- A prompt keyword selector starts runs with `core` plus likely-needed groups.
- The model can call `tool_search` for deferred capability discovery.
- The model can call `tool_access` to activate groups or exact tools for the next provider turn.
- Agent definitions can narrow tool access through tool policy labels, exact tools, groups, tool packs, denied tools, and risky-effect opt-ins.
- Runtime availability also filters tools, such as skills, browser, emulator, Solana, MCP, and environment-dependent packs.

## Main Findings

The prompt compiler has fragment priorities, hashes, provenance, and token estimates, but rendering still follows hardcoded insertion order rather than a budgeted priority strategy. The context policy records pressure, but prompt assembly does not yet make hard include, summarize, or exclude choices from that pressure.

Several descriptors are mixed-mutability surfaces. `project_context` can read, record, update, propose, and refresh context but is classified as observe. `browser_observe` and `browser_control` groups both expose the same `browser` schema. `mcp_list` and `mcp_invoke` both expose the same `mcp` schema. `command_readonly` still exposes a generic `command` tool that can mutate through arbitrary argv. This leaves too much safety boundary work to prompts and downstream handlers.

Tool capability data is duplicated across constants, tool access groups, deferred catalog entries, schemas, effect classes, V2 descriptors, tool-pack manifests, prompt text, and harness Test agent expectations. This makes drift likely.

Repository instruction handling loads all discovered `AGENTS.md` files into the prompt. The fragment says nested instructions apply only inside their directory, but every nested instruction is visible to the model before a concrete write path is known.

The generated project code map is useful for first orientation but can become prompt bloat and stale guidance. Workspace index and explicit file reads are better authorities for most code navigation.

Provider attachment handling is uneven. Anthropic serializes image, document, and text attachments; OpenAI-compatible chat and Responses paths currently ignore attachments in their request builders. Provider preflight can mark attachments as required, but the provider adapter needs to either implement them or fail closed.

The Test agent has a strong deterministic order gate, but its manifest is still model-driven: the model must call the right tool with safe input and produce a report. The harness should expose a machine-readable tool-test executor so CI can compare model-driven and deterministic fixture-driven outcomes.

## North Star

The harness should become a contract-driven runtime:

- One source of truth defines tools, actions, risk, schemas, policy, tool-pack membership, runtime availability, and documentation metadata.
- Prompt text describes the contract but never carries the only copy of an enforcement rule.
- Provider context packages are reproducible, budgeted, and diffable.
- Tool dispatch records exact action-level policy, sandbox, approval, timeout, rollback, and truncation metadata.
- Every production run can be exported as a canonical trace that passes quality gates.
- Every built-in and custom agent can be evaluated against prompt quality, least privilege, retrieval behavior, memory behavior, injection resistance, and version pinning.

## Phase 0: Freeze The Current Contract

Create a generated harness inventory from the current code before changing behavior.

Work:

- Add a machine-readable `harness_contract` export that lists built-in agents, prompt fragments, tool groups, tools, effect classes, schemas, tool packs, runtime availability, and agent access.
- Save golden snapshots for prompt compilation per built-in agent with and without custom agent policy, skill context, process state, and coordination state.
- Save golden snapshots for tool registry V2 descriptors per built-in agent and representative custom policies.
- Add a small contract check that fails when a tool exists in one registry surface but is missing from another.

Exit criteria:

- A scoped Rust test can prove every tool has a descriptor, effect class, access-group entry, V2 descriptor, and catalog entry when enabled.
- Prompt golden diffs identify intentional policy wording changes before they reach a provider.

## Phase 1: Replace Coarse Tools With Action-Level Risk

Create action-level tool descriptors and policies. This can be done with wrapper descriptors that route to existing handlers, so implementation can land incrementally.

Work:

- Split or virtualize mixed surfaces:
  - `project_context_search`, `project_context_get`, `project_context_record`, `project_context_update`, `project_context_refresh`.
  - `browser_observe` and `browser_control`, or finer browser action descriptors if the model benefits.
  - `mcp_list`, `mcp_read_resource`, `mcp_get_prompt`, `mcp_call_tool`.
  - `command_probe`, `command_verify`, `command_run`, `command_session`.
  - `system_diagnostics_observe` and privileged diagnostics actions.
- Treat action-level classification as the authority for mutability, approval, sandbox, and trace metadata.
- Keep the old runtime handlers internally where useful, but stop exposing coarse descriptors to the model once replacements pass tests.
- Make `project_context` writes app-state mutation in V2, not read-only observe, unless split descriptors remove the ambiguity.
- Make Solana tools typed. Replace generic object schemas with action schemas for cluster, logs, transaction, simulation, deploy, audit, docs, and secrets surfaces.

Exit criteria:

- Ask cannot call a write-capable durable-context action.
- Activating browser observe cannot type, click, navigate, write storage, or set cookies.
- Activating MCP list cannot invoke arbitrary external tools.
- Command verify/probe has a narrower policy than command run/session.
- Trace events show action-level effect class and sandbox requirement, not just tool-level metadata.

## Phase 2: Make Tool Exposure Capability-Driven

Replace prompt keyword selection as the primary exposure strategy with a typed capability planner.

Work:

- Keep a small startup surface: file read/search/status, tool search, project context read, workspace index status, and todo where the selected agent may use it.
- Add a deterministic capability planner that maps task classification, agent profile, custom policy, environment health, and provider support to initial tool groups.
- Use `tool_search` results and `tool_access` activation as explicit events in the trace, not hidden prompt behavior.
- Record why each tool was exposed: startup core, planner classification, user explicit tool marker, custom policy, tool access request, or verification gate.
- Make exposure explanations available through persisted trace metadata and a diagnostics command.

Exit criteria:

- A frontend prompt mentioning "latest docs" does not expose browser control by default; it exposes search/fetch first.
- A code implementation prompt exposes edit/patch and verification command tools only when the selected agent and approval mode allow them.
- Tool exposure is reproducible from the persisted run controls and task text.

## Phase 3: Refactor Prompt Compilation

Turn the prompt compiler into a budget-aware assembly pipeline.

Work:

- Sort and render fragments by explicit priority tiers while preserving deterministic tie-breakers.
- Add fragment budget policies: always include, include if relevant, summarize, tool-mediated only, and exclude with manifest reason.
- Replace the generated code map with a compact workspace manifest plus explicit guidance to use `workspace_index`, `search`, and `read` for authoritative details.
- Load nested repository instructions lazily based on relevant paths once a task plan or file write scope exists.
- Add prompt-injection hardening for repository instructions, skill context, MCP prompts, web fetches, mailbox content, and durable context with consistent boundary markers.
- Record prompt diffs between turns in the manifest when fragments change due to tool activation, process state, skills, coordination, or compaction.

Exit criteria:

- Over-budget prompts fail closed, compact, or hand off before provider submission.
- Prompt manifests can explain every included and excluded fragment.
- Nested repository instructions are applied by path, not globally over the whole run.

## Phase 4: Sharpen Built-In Agent Contracts

Keep the built-in agents distinct and make their prompts thinner by moving requirements into tooling.

Work:

- Ask: remove durable write affordances from the default Ask surface unless the user explicitly asks to save a note, and route that through a separate approved context-write action.
- Engineer: move "read or hash before edit" into file-write preconditions so the model does not need to remember it.
- Debug: add a first-class evidence ledger tool or structured todo mode for symptom, reproduction, hypotheses, experiments, root cause, fix, and verification.
- Agent Create: make generated definitions schema-first, with validation diagnostics that show exact denied tool/effect reasons.
- Test: add a deterministic harness runner tool that can execute the canonical tool manifest with safe fixture inputs and compare it to the model-driven report.

Exit criteria:

- Each built-in agent has a prompt golden and a tool-access golden.
- Custom definitions cannot expand beyond base profile, parent policy, or active runtime availability.
- Test agent CI can fail on out-of-order, missing, or unsafe steps without depending only on final Markdown.

## Phase 5: Provider Preflight And Adapter Parity

Make provider support explicit, live, and bound to each provider turn.

Work:

- Require live harmless selected-model preflight for production tool-using runs, with cached reuse only when provider, model, endpoint, account class, required features, and freshness match.
- Bind the admitted provider preflight hash into every context manifest and canonical trace.
- Implement attachment parity for OpenAI-compatible chat, OpenAI Responses, and Codex Responses, or deny attachment runs for those adapters until supported.
- Add provider-specific capability normalization for parallel tools, strict schemas, reasoning effort, max output tokens, stream usage, cache accounting, and cost.
- Add provider transcript replay tests that preserve assistant tool call ids and tool result ordering across resume, compaction, fork, retry, and handoff.

Exit criteria:

- A provider turn cannot silently drop user attachments.
- Production traces can prove the provider actually admitted the required tool and attachment features.
- Provider-specific failures are classified as auth, quota, unsupported capability, malformed stream, transport, retryable provider failure, or harness fault.

## Phase 6: Strengthen Tool Runtime Reliability

Improve execution semantics around cancellation, rollback, output limits, and side effects.

Work:

- Ensure every long-running or blocking handler observes cancellation through Tool Registry V2 control.
- Make rollback real for file mutations where possible: checkpoint old content before mutation and restore after handler failure when safe.
- Add per-action output truncation contracts that preserve JSON shape for structured tools.
- Add command policy profiles for read-only verification, generated-file mutation, dependency installation, external network use, and destructive operations.
- Move file reservations into the dispatch preflight for write actions so conflicts block before handler execution.
- Add sandbox metadata to every subprocess, external process, browser, emulator, Solana, MCP, and subagent execution event.

Exit criteria:

- A hung read-only tool times out without blocking the provider loop.
- A failed write records checkpoint metadata and rollback outcome.
- Tool results are never large enough to destabilize provider replay.

## First Slice To Build

Start with the smallest slice that reduces drift and unlocks later phases:

1. Add a `ToolCapabilitySpec` source of truth in the core runtime.
2. Generate or derive legacy descriptors, V2 descriptors, access groups, catalog entries, effect classes, and tool-pack membership from it.
3. Add action-level spec support for at least `project_context`, `browser`, `mcp`, and `command`.
4. Keep existing handlers but expose new action-level descriptors in one gated test profile.
5. Add golden tests for Ask, Engineer, Debug, Agent Create, and Test tool registries.

This slice is valuable even before replacing every model-facing tool name because it gives the rest of the roadmap a stable contract to converge on.

## Verification Plan

Use scoped checks, not repo-wide runs, while iterating:

- Prompt contract tests for built-in agents and representative custom definitions.
- Tool registry contract tests for descriptor generation and drift.
- Tool dispatch tests for action-level policy, sandbox denial, approval required, timeout, rollback, and truncation.
- Provider adapter request-shape tests for tools, attachments, reasoning effort, and replayed tool ids.
- Harness Test agent CI for canonical manifest ordering.
- Canonical trace quality tests for production gates.

## Non-Goals

- Do not build a browser-based workflow around this plan; Xero is a Tauri app.
- Do not write new repo-local state under `.xero/`.
- Do not keep compatibility aliases for old coarse tool names once replacement descriptors are validated, unless a user explicitly asks for migration compatibility.
- Do not add UI as part of this harness plan.