# Agent Harness Audit And Production Plan

Audience: Cadence engineers turning the owned-agent path into a production-grade coding harness.

Post-read action: use this as the implementation plan for replacing the current prototype-grade agent loop with a reliable harness comparable in ambition to OpenCode, Claude Code, and Codex.

## Executive Summary

Cadence already has the bones of a real coding-agent harness: a durable owned-agent run loop, provider adapters, tool-call streaming, repo-scoped filesystem tools, command policy, app-local browser/emulator/MCP/Solana bridges, session compaction, memory review, and action-request persistence.

The current harness is not yet production-grade. The largest weaknesses are prompt construction, tool disclosure, planning discipline, tool activation, state-machine rigor, and verification. The system prompt is assembled as one string, project instructions are injected without a clear priority boundary, tool disclosure is mostly keyword-selected, tool discovery is coarse, subagents are synchronous, and there is no harness-level contract that forces plan, edit, verify, and final-evidence behavior across models.

The right direction is not to add more ad hoc prompt text. Cadence needs a versioned prompt compiler, a progressive tool catalog, a stricter run state machine, a richer approval/sandbox model, first-class planning and verification loops, and an eval suite that catches regressions in tool use, safety, and task completion.

## How The Agent Works Today

### Runtime Shape

Cadence has two runtime paths:

- Detached runtime supervisor: launches an external runtime shell/sidecar path for existing provider CLIs.
- Owned agent runtime: Cadence drives the model loop itself through Rust provider adapters and executes tools through its own tool runtime.

This audit focuses on the owned agent runtime because that is the in-app harness Cadence controls.

An owned run starts from either the agent task commands or the runtime-run control path. Cadence resolves the project root, active provider profile, model, approval mode, optional thinking effort, and tool runtime. It creates a durable run row, stores the system and user messages, emits preflight events, and then starts the provider loop.

The provider loop runs up to 32 turns. Each turn:

1. Rebuilds the tool registry for the current transcript.
2. Reassembles the system prompt.
3. Sends the full provider turn with selected tool descriptors.
4. Streams message deltas, reasoning summaries, usage, and tool argument deltas into events.
5. If the provider returns tool calls, validates each call against the current registry.
6. Executes tools sequentially through the autonomous tool runtime.
7. Persists tool start, result, file-change, command-output, rollback checkpoint, and action-required events.
8. Appends tool results back into replay messages.
9. Expands the registry on the next turn when `tool_access` grants additional tools.
10. Completes when the provider returns a normal assistant message without tool calls.

### Current System Prompt

The current prompt version is:

```text
cadence-owned-agent-v1
```

The prompt body is assembled from:

- A fixed Cadence-owned-agent instruction string.
- The comma-separated names of currently selected tools.
- The root project instruction file, currently `AGENTS.md`.
- Approved memory for the project/session, after redaction.
- A per-turn owned-process lifecycle summary when Cadence-owned processes exist.

The current prompt template is effectively:

```text
cadence-owned-agent-v1

You are Xero's owned software-building agent. Work directly in the imported repository, use tools for filesystem and command work, record evidence, and stop only when the task is done or a configured safety boundary requires user input.

Operate like a production coding agent: inspect before editing, respect a dirty worktree, keep changes scoped, prefer `rg` for search, run focused verification when behavior changes, and summarize concrete evidence before completion. Before modifying an existing file, read or hash the target in the current run so Xero can detect stale writes safely.

Available tools: {tool_names}

If a relevant capability is not currently available, call `tool_access` to request the smallest needed tool group before proceeding. If the `lsp` tool reports an `installSuggestion`, ask the user before running any candidate install command; use the command tool only after consent and normal operator approval.

Repository instructions:
{root AGENTS.md contents or "(none)"}

Approved memory:
{approved redacted memory or "(none)"}
```

On turns where Cadence owns live processes, this is appended:

```text
Cadence-owned process state for this turn (read-only digest; call `process_manager` for fresh output or control):
{summary}
```

Important observations:

- The prompt says "Xero" while the product and codebase are Cadence. That is a brand/identity mismatch.
- Repository instructions are inserted into the system prompt string, but the prompt does not explicitly say they are lower-priority than Cadence safety policy.
- Only the root instruction file is used by the owned prompt path. The context docs talk about supported instruction files, but current owned prompt assembly only reads root `AGENTS.md`.
- Memory has a priority warning and secret redaction. Repository instructions do not get the same injection-boundary treatment.
- Tool names are listed in the prompt, but full tool schemas are disclosed separately through provider-native function/tool descriptors.

### Provider Requests

The same `ProviderTurnRequest` is adapted to each provider family:

- OpenAI Responses: system prompt goes into `instructions`; messages and tool results go into `input`; tools are function tools.
- OpenAI-compatible chat: system prompt becomes the first `system` message; tools are OpenAI chat function tools with `tool_choice: auto`.
- Anthropic, Bedrock, Vertex Anthropic: system prompt goes into `system`; tools use Anthropic `input_schema`; tool calls and tool results are mapped to `tool_use` and `tool_result` blocks.

The provider only receives the descriptors in the current `ToolRegistry`, not every built-in descriptor.

### Current Tool Registry

The registry starts with the `core` group on every prompt:

- `read`
- `search`
- `find`
- `git_status`
- `git_diff`
- `tool_access`
- `list`
- `file_hash`

Cadence then uses prompt keyword matching to add groups:

| Group | When selected | Tools |
| --- | --- | --- |
| `mutation` | Implement/change/write/create/refactor-style prompts | `edit`, `write`, `patch`, `delete`, `rename`, `mkdir` |
| `command` | Test/audit/run/debug/build/security-style prompts | `command`, `command_session_start`, `command_session_read`, `command_session_stop` |
| `process_manager` | Long-running/background/process prompts | `process_manager` |
| `macos` | macOS/system automation prompts | `macos_automation` |
| `web` | Browser/frontend/web/docs/latest/internet prompts | `web_search`, `web_fetch`, `browser` |
| `emulator` | Mobile/iOS/Android/device/app automation prompts | `emulator` |
| `solana` | Solana keywords or Solana-looking workspace | Solana cluster/logs/tx/simulate/explain/ALT/IDL/Codama/PDA/program/deploy/audit/replay/indexer/secrets/cost/docs tools |
| `agent_ops` | Subagent/todo/deferred-tool prompts | `subagent`, `todo`, `tool_search` |
| `mcp` | MCP/resource/prompt-template/invoke-tool prompts | `mcp` |
| `intelligence` | LSP/symbols/diagnostics/code intelligence prompts | `code_intel`, `lsp` |
| `notebook` | Notebook/Jupyter prompts | `notebook_edit` |
| `powershell` | PowerShell prompts | `powershell` |
| `skills` | Skill-related prompts, when skill runtime is enabled | `skill` |

The registry also preserves tools granted by successful `tool_access` calls from previous turns in the run.

### Is Tool Disclosure Progressive?

Partially, but not enough.

What is progressive today:

- The provider does not receive every built-in tool descriptor by default.
- Initial descriptors are selected by prompt heuristics.
- `tool_access` is always available through the core group.
- A model can call `tool_access` with a group or specific tool, and granted tools are exposed on the next provider turn.
- Tool grants are persisted and replayed for future continuation turns.

What is not progressive enough:

- Selection is keyword-based, not semantic, contextual, or budget-aware.
- The initial prompt receives only tool names, while the provider receives full schemas for all selected tools immediately.
- `tool_search` is not always available; the agent may need to request `agent_ops` before it can search deferred tools.
- `tool_search` is substring matching over a static catalog, not BM25/vector/ranked search over full tool metadata.
- MCP capabilities are hidden behind one generic `mcp` tool, not progressively exposed as typed, model-native function tools.
- Skills are behind one generic `skill` tool, not loaded into the prompt automatically based on task fit.
- Tool groups are coarse. Requesting `web` exposes search, fetch, and in-app browser together.
- The active registry is inferred from transcript/prompt plus grants, rather than stored as a first-class versioned run artifact.

### Tool Execution And Safety

Filesystem operations are repo-scoped. Existing-file writes are guarded by an observation hash: the agent must read or hash a file in the current run before modifying it, and stale files produce action requests. New files can be written without a prior read.

Commands are repo-scoped and policy-gated:

- Non-destructive commands can run automatically in `yolo` mode.
- Non-`yolo` approval modes escalate all shell commands.
- Ambiguous, network-capable, destructive, and package-manager mutation commands escalate.
- Command output is capped and redacted before durable persistence.

Browser, emulator, Solana, MCP, process-manager, macOS automation, skill, notebook, code-intel, and LSP tools route through dedicated runtime modules.

Subagents exist, but are synchronous child owned-agent runs. They block the parent tool call and are depth-limited by disabling further subagent spawning in the child runtime.

### Current Durability

Owned-agent runs persist:

- Run metadata, status, heartbeat, provider, model, prompt, and system prompt.
- Messages by role.
- Provider stream events.
- Tool call start/result/error state.
- File-change records.
- Command-output records.
- Rollback checkpoints for file changes.
- Action requests for approvals/safety boundaries.
- Usage records.
- Compaction and memory records.

Context replay can use active compaction summaries plus raw tails. Auto-compact can run before continuations when enabled and budget pressure crosses the configured threshold.

## Production Gaps

### Prompt And Instruction Hierarchy

The system prompt is a hand-built string instead of a prompt compiler. It lacks a strong instruction hierarchy, explicit tool-use contract, final response contract, security model, verification policy, and workspace/environment facts. Project instructions are included without delimiters that clearly mark them as lower-priority, project-owned guidance.

### Tool Disclosure

Tool disclosure is more progressive than "dump everything," but it is not a high-quality progressive disclosure system. The current heuristic can overexpose large tool groups or underexpose needed tools. It also makes discovery dependent on whether the right discovery tool was initially selected.

### Planning And Autonomy

Plan mode only pauses before first-turn tool calls. The harness does not require a structured plan for complex tasks, does not track plan evolution as a first-class artifact, and does not enforce verification before completion.

### Provider Independence

The harness maps multiple providers, but there is no model-capability matrix that changes prompt style, tool schema detail, parallel tool behavior, reasoning controls, output token limits, or retry policy by model capability.

### Tool Runtime

The tools are useful but uneven:

- There is no single canonical patch language equivalent to Codex's `apply_patch` contract.
- Long-running command sessions exist, but the core loop still executes tool calls sequentially.
- Browser and app automation are generic tools rather than progressive, page-aware surfaces.
- MCP tools are not projected as model-native tool descriptors.
- Skills require explicit model operation instead of automatic retrieval and invocation when clearly relevant.

### Safety And Approvals

The command policy is a good start, but production harnesses need a broader policy engine:

- Filesystem writes, command execution, network access, OS automation, external process signaling, credentials, and package installation should share one explainable policy model.
- Approval prompts should include precise diff/risk summaries.
- "Yolo" mode should still have non-negotiable deny rules for secrets, destructive escapes, and external exfiltration.

### Context And Memory

Compaction and memory review exist, but the prompt builder does not yet use a ranked, token-budget-aware context graph. Memory is inserted as a flat list. Tool results are replayed directly unless compacted. There is no codebase map, dependency map, or retrieval over prior runs beyond current context utilities.

### Observability And Evaluation

Events are persisted, but production readiness needs evals and metrics:

- Task success rate.
- Tool-call validity rate.
- Unnecessary tool exposure.
- Approval precision.
- Stale-write prevention.
- Verification coverage.
- Token/cost pressure.
- Provider error recovery.

## Production Plan

### Milestone 1: Prompt Compiler And Instruction Hierarchy

Replace string assembly with a versioned `PromptCompiler`.

Deliverables:

- Prompt fragments with stable IDs, priorities, hashes, token estimates, and provenance.
- Explicit hierarchy: Cadence system policy, developer/runtime policy, project instructions, selected skills, approved memory, task transcript, tool policy.
- Strong injection boundaries for project instructions, memory, MCP text, web text, skill text, and tool output.
- Root and nested instruction-file support with deterministic precedence.
- Brand cleanup: Cadence identity everywhere unless product branding intentionally changes.
- A model-facing final response contract: summary, files changed, verification run, blockers.
- Snapshot tests for prompt assembly across empty repo, dirty repo, nested instructions, memory, compaction, and selected tools.

Acceptance criteria:

- Prompt construction is deterministic and testable.
- Context visualization and provider replay use the same prompt compiler.
- Project-owned text cannot silently override Cadence safety policy.
- Every prompt fragment can be explained in the Context panel.

### Milestone 2: Progressive Tool Catalog

Make tool disclosure genuinely progressive.

Deliverables:

- Always-on minimal tools: `read`, `search`, `find`, `list`, `git_status`, `git_diff`, `file_hash`, `tool_search`, `tool_access`, `todo`.
- A deferred tool catalog indexed by name, group, tags, schema fields, examples, risk class, and runtime availability.
- Ranked search using lexical scoring first, with an embedding-backed index later if needed.
- Tool activation tokens: `tool_search` can return candidate descriptors, but the model must call `tool_access` to activate them.
- Fine-grained tool bundles, such as `web_search_only`, `browser_observe`, `browser_control`, `command_readonly`, `command_mutating`, `mcp_list`, `mcp_invoke`.
- Store the active registry as a durable run artifact rather than deriving it only from prompt text and prior grants.

Acceptance criteria:

- A broad task does not receive unrelated Solana/browser/emulator schemas unless needed.
- A task can discover and activate an obscure tool without user intervention.
- The active tool set is explainable, reproducible, and replayable.

### Milestone 3: Native MCP And Skill Projection

Move MCP and skills from generic adapters toward model-native, progressive capabilities.

Deliverables:

- MCP capability discovery that records server tools/resources/prompts as deferred catalog entries.
- Per-MCP-tool schema projection into provider-native tool descriptors after activation.
- Stable namespacing for MCP tools to avoid collisions.
- Skill retrieval that can recommend relevant skills before the main model overcommits to an approach.
- Skill invocation as a prompt-fragment source with hashes and provenance, not just a generic tool result.
- Trust and approval status shown directly in tool-search results.

Acceptance criteria:

- The model can discover "the Playwright skill" or an MCP server's exact tool without seeing every MCP schema upfront.
- Invoked skills become visible, bounded prompt fragments.
- Untrusted skill/MCP content cannot bypass prompt hierarchy.

### Milestone 4: Agent State Machine

Make the loop explicit instead of "provider turns until done."

Deliverables:

- States: intake, context gather, plan, approval wait, execute, verify, summarize, blocked, complete.
- Model actions constrained by state.
- Complex-task classifier that requires plan state for high-risk or multi-file work.
- Harness-owned plan artifact with updates, not just model prose.
- Completion gate that checks whether required verification evidence exists.
- Stop reasons that distinguish complete, blocked, waiting for approval, context over budget, provider failure, cancelled, and harness fault.

Acceptance criteria:

- The agent cannot finish a code-changing task without a declared verification result or an explicit "unable to verify" reason.
- Plan mode works beyond first-turn tool calls.
- UI can explain exactly why a run is paused or complete.

### Milestone 5: Tool Runtime Hardening

Standardize tools around production coding workflows.

Deliverables:

- Canonical patch tool with preview, expected-hash guards, multi-file patch support, and exact failure diagnostics.
- Unified file observation model shared by read, search, hash, edit, patch, delete, rename, and notebook edit.
- PTY-backed command sessions where needed, with durable cursors and cleanup.
- Separate command tools for short command, long-running process, and interactive session.
- Better command policy classification with repo-local allowlists and package-script introspection.
- First-class test/lint/build tool wrappers that know project scripts and reduce arbitrary shell use.
- Rollback support surfaced in UI for failed or user-rejected edits.

Acceptance criteria:

- Multi-file edits are atomic where possible or leave a clear rollback plan.
- Commands produce bounded, redacted, structured evidence.
- Approval requests include the exact planned command/diff and risk reason.

### Milestone 6: Safety Policy Engine

Replace scattered checks with a central policy engine.

Deliverables:

- Policy inputs: tool name, arguments, repo state, approval mode, project trust, network intent, credential sensitivity, OS target, prior observations.
- Policy outputs: allow, require approval, deny, with stable codes and user-facing explanations.
- Non-negotiable denies for path escape, secret exposure, destructive system operations, and unapproved external process control.
- Approval grants with scope, expiry, replay rules, and audit trail.
- Red-team tests for prompt injection through repo files, tool output, web pages, MCP resources, and skills.

Acceptance criteria:

- Every tool call has an auditable policy decision.
- Approval replay cannot accidentally approve a materially different action.
- Secret-like content is never persisted or sent back to the model unredacted.

### Milestone 7: Context Engine

Build a token-budget-aware context graph.

Deliverables:

- Context nodes for system fragments, instructions, memory, transcript messages, tool summaries, file observations, code symbols, dependency metadata, and run artifacts.
- Ranking by recency, relevance, authority, and task phase.
- Summaries for large tool outputs and older transcript segments.
- Project code map generation using file tree, package manifests, symbols, and recent edits.
- Context budget planner that decides what to include, summarize, defer, or retrieve.
- Context snapshot diffing between turns.

Acceptance criteria:

- The Context panel and provider request match.
- Long sessions remain useful without manual transcript pruning.
- The model receives relevant file/code context before editing.

### Milestone 8: Subagents And Parallel Work

Upgrade subagents from synchronous child runs into a managed parallel work system.

Deliverables:

- Async subagent tasks with independent status, cancellation, logs, result artifacts, and ownership boundaries.
- Agent roles: explorer, implementation worker, verifier, reviewer.
- Disjoint write-set enforcement for workers.
- Parent integration step that reads subagent outputs and decides what to apply.
- Model routing by subtask complexity and cost.

Acceptance criteria:

- Parent runs can continue useful non-overlapping work while subagents run.
- Subagent outputs are durable, summarized, and linked to parent decisions.
- Workers cannot silently overwrite each other's files.

### Milestone 9: Verification And Evals

Create a harness quality gate.

Deliverables:

- Golden transcript tests for prompt assembly, tool selection, tool activation, approvals, compaction, and continuations.
- Simulated provider tests for common coding tasks.
- Repository fixture tasks: one-file fix, multi-file refactor, frontend change, Rust backend change, failing test repair, prompt-injection file, stale-worktree conflict.
- Metrics: task completion, tool-call validity, unnecessary tool exposure, approval precision, verification rate, rollback correctness.
- Nightly eval command and CI reporting.

Acceptance criteria:

- Regressions in prompt/tool behavior fail tests before release.
- New tools require descriptor tests, policy tests, and at least one task eval.
- Production readiness is measured, not guessed.

### Milestone 10: Product Surface

Make the harness legible to users.

Deliverables:

- Live state timeline: planning, tool activation, approvals, edits, verification, completion.
- Tool registry view with active/deferred tools and why each was exposed.
- Prompt/context inspector backed by prompt fragment provenance.
- Approval cards with precise risk, command/diff preview, and grant scope.
- Verification summary cards.
- Rollback/retry controls for failed edits and stale writes.

Acceptance criteria:

- A user can understand what the agent is about to do before approving risk.
- A completed run shows exactly what changed and how it was verified.
- Tool overexposure and context bloat are visible during development.

## Recommended Build Order

1. Prompt compiler and exact context snapshot parity.
2. Always-on `tool_search` plus durable active registry.
3. Fine-grained progressive tool groups.
4. Central policy engine and approval grants.
5. State-machine loop with plan and verification gates.
6. Canonical patch/edit tool improvements.
7. Native MCP and skill projection.
8. Context graph and code map.
9. Async subagents.
10. Eval suite and UI inspector polish.

This order keeps the harness usable while reducing the riskiest architectural debt first. Prompt and registry determinism should land before adding more agent capability, because every later feature depends on knowing what the model saw, what tools it had, and why.

## First Implementation Slice

Start with a narrow but high-leverage slice:

1. Add `PromptCompiler` with fragments for base policy, repo instructions, approved memory, active tools, and process summary.
2. Make context visualization consume the same fragments.
3. Add `tool_search` to the always-on core tools.
4. Persist the active tool registry for each provider turn.
5. Add snapshot tests for the current prompt output so behavior changes are explicit.
6. Fix the Cadence/Xero identity mismatch.
7. Add an explicit lower-priority boundary around repository instructions.

This slice does not need to redesign every tool. It gives Cadence a stable foundation for the rest of the harness.
