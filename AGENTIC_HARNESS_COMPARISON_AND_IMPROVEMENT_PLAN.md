# Agentic Harness Competitive Comparison and Improvement Plan

## Reader And Outcome

This document is for Xero engineers deciding what to build next in the agent runtime. After reading it, a maintainer should be able to explain where Xero is already ahead, where Codex, OpenHands, and ForgeCode are ahead, and what sequence of harness work would make Xero the best combination of the three.

The intended outcome is not a union of every competitor feature. The target is a local-first desktop harness with:

- Codex-grade core architecture, protocol discipline, tool execution, and sandboxing.
- OpenHands-grade environment lifecycle, conversation operations, server surface, and external workflow integrations.
- ForgeCode-grade terminal ergonomics, provider breadth, conversation commands, semantic workspace search, and distribution polish.
- Xero-grade durable context, memory, desktop-native orchestration, browser/mobile/Solana tooling, and operator approval UX.

## Scope And Method

The comparison is based on local inspection of these repositories:

- Xero: current project at this repository root.
- Codex: local checkout named `codex`.
- OpenHands: local checkout named `OpenHands`.
- ForgeCode npm package: local package named `npm-forgecode`.

Important source caveats:

- The ForgeCode package contains an npm wrapper and prebuilt binaries, not the full core source. Its assessment uses package metadata, README, CLI help output, provider/config/tool lists, and visible binary strings. Treat it as product capability evidence, not implementation-level evidence.
- The OpenHands checkout contains the app/server/frontend product and depends on the separate OpenHands SDK for much of the core agent engine. Its assessment is strongest for product architecture, conversation lifecycle, sandboxes, and integrations.
- Codex exposes the richest inspected core source. Its assessment is strongest for reusable harness architecture, typed protocol, tool dispatch, sandboxing, thread management, and CLI/TUI ergonomics.

## Executive Assessment

Xero already has unusually strong local-first agent memory and desktop orchestration. The durable context manifest, hybrid project retrieval, session lineage, compaction/handoff policies, approval-aware tool dispatch, in-app browser control, emulator tooling, Solana workbench, and provider setup docs are a serious base. The current architecture is not a toy harness.

The largest gap is that Xero's strongest ideas are still too bound to the Tauri desktop runtime. Codex's core is more portable and protocol-shaped. OpenHands has a clearer operational lifecycle around conversations and sandboxes. ForgeCode has a better terminal and distribution story. Xero should keep its desktop-native identity while extracting a reusable core that can power the app, a CLI, an MCP server, a headless daemon, and future remote execution.

The most important move is to harden the core harness boundary before adding more UI extras. Once Xero has a reusable typed protocol, pluggable tool registry, sandboxed execution profiles, headless runner, and traceable event stream, the higher-level features from OpenHands and ForgeCode become much cheaper to add.

## Xero Baseline

### Current Strengths

Xero already has a serious owned-agent loop:

- Agent types: Ask, Engineer, Debug, and AgentCreate.
- Provider adapters: OpenAI Codex backend, OpenAI Responses and compatible APIs, Anthropic, OpenRouter, GitHub Models, Gemini, Ollama, Azure OpenAI, Bedrock, Vertex, and OpenAI-compatible recipes.
- Prompt compiler: combines agent personality, tool policy, custom agent definitions, repository instructions, code maps, skill contexts, process summaries, approved memory, and retrieved project records.
- Deterministic context package: a persisted manifest is assembled before provider calls, including prompt fragments, message selection, tools, retrieval, token estimates, pressure policy, compaction state, and redaction state.
- Durable runtime state: sessions, runs, messages, events, tool calls, snapshots, context manifests, project records, memory candidates, retrieval logs, and lineage are persisted.
- Retrieval: SQLite plus LanceDB-backed hybrid retrieval for handoffs, project facts, decisions, plans, findings, verification, memories, and candidates.
- Continuity: compaction thresholds, same-type handoff rules, context pressure policy, idempotent handoff records, and session lineage.
- Tool dispatch: central registry, approval policy, redaction, path checks, command risk classification, observation-before-write guard, rollback checkpoints, tool-call events, command output recording, and diagnostics.
- Tool breadth: filesystem, git, command sessions, process management, diagnostics, web search/fetch, browser automation, emulator control, Solana tools, MCP, skills, subagents, project context, tool search, todo planning, notebooks, code intelligence, LSP, and PowerShell.
- State machine: intake, context gathering, planning, approval wait, execution, verification, summarization, blocked, and complete states.
- Gates: plan gates for complex or high-risk tasks and verification gates for Engineer/Debug workflows.
- Quality coverage: internal surfaces for prompt assembly, tool selection, activation, approvals, compaction, continuations, and scenarios covering real edit workflows plus injection and stale worktree cases.
- Desktop product advantage: Tauri app, project import/search, runtime/session orchestration, operator approval loop, browser automation, iOS/Android emulator, Solana workbench, skill/plugin discovery, notifications, and sidecar services.

### Current Gaps

The main gaps are architectural and operational:

- The core harness is not yet clearly packaged as a reusable library or daemon boundary independent from Tauri commands and app state.
- There is no obvious Codex-style submission/event protocol that can be shared by desktop, CLI, MCP server, tests, and future cloud or remote surfaces.
- Tool execution is policy-rich, but the registry could be more extensible through handler traits, pre/post hooks, typed telemetry, explicit parallel-read scheduling, and dynamic tool provenance.
- Safety policy is strong at the application level, but Xero does not yet appear to have Codex-grade OS/container sandbox enforcement around command execution.
- Xero has project memory and retrieval, but not a ForgeCode-style workspace semantic code index exposed as a first-class user command and agent tool.
- Conversation management exists through sessions, branch/rewind, export, and compaction concepts, but ForgeCode's terminal commands for retry, clone, dump, compact, stats, and conversation selection are more complete.
- OpenHands has a clearer environment lifecycle: wait for sandbox, prepare repository, run setup scripts, install hooks, install skills, start conversation, and signal readiness.
- Provider breadth is strong but not ForgeCode-level, especially for hosting external agent CLIs or subscription-backed agents through ACP-like adapters.
- Multi-agent support exists, but Codex and ForgeCode expose more explicit subagent lifecycle, concurrency limits, wait/follow-up semantics, and role separation.
- Integrations are promising, but OpenHands has more mature issue/PR/Slack/Jira/Linear/cloud workflow surfaces.

## Competitor Profiles

## Codex

### What Codex Is Best At

Codex is the strongest inspected core harness. It is not merely an app; it is a reusable Rust agent runtime with a CLI, TUI, protocol layer, thread manager, sandboxing, MCP support, rollout traces, and typed client/server concepts.

The important architectural pattern is separation:

- `codex-core` owns the agent session, tools, model client, config, history, sandboxing, skills, plugins, MCP, hooks, rollout, and thread orchestration.
- Protocol crates define typed submissions, events, approvals, model/config changes, realtime events, and traces.
- CLI/TUI/app-server surfaces consume the same core instead of owning the core.
- Sandbox and execution policy are dedicated concerns, not incidental command wrappers.

### Capabilities Worth Copying

- A reusable core crate that can run without the desktop UI.
- A typed submission queue and event queue protocol with generated or shareable schema.
- W3C-style trace identifiers attached to turns, tool calls, approvals, and events.
- Thread manager abstraction that supports local, remote, and in-memory stores.
- Fork/resume/truncate behavior as a first-class runtime feature.
- Strong config model for providers, model presets, features, tools, MCP servers, permissions, environment policy, memories, and credentials.
- Tool registry with handler traits, mutating tool markers, pre/post hooks, provenance, telemetry tags, and per-tool policy metadata.
- Explicit parallelism controls for safe read-only work.
- Cross-platform sandboxing: macOS sandbox-exec, Linux Landlock/bubblewrap, Windows sandbox support, and network policy separation.
- Project trust and restricted workspace-write behavior that protects `.git` and internal agent state.
- Non-interactive `exec` mode for automation and CI.
- TUI distribution for terminal users.
- MCP client and experimental MCP server surfaces.
- Rollout traces and replayable task state.
- Rich hooks system for notifications, tool events, and integrations.
- Multi-agent primitives with concurrency and depth controls.

### Where Xero Is Already Ahead

- Xero has a stronger durable context-manifest design around every owned-agent provider request.
- Xero has deeper local project memory and retrieval concepts, including project records, memory candidates, compaction artifacts, and handoff records.
- Xero's desktop product can combine text, browser, emulator, Solana, project state, approvals, and diagnostics in one operator surface.
- Xero has domain tool packs Codex does not appear to emphasize, especially mobile emulator and Solana workflows.

### Where Codex Is Ahead

- The core/runtime boundary is cleaner and more reusable.
- Protocol and event typing are more central.
- Sandbox enforcement is more mature.
- CLI/TUI/headless usage is more productized.
- Tool registry abstractions and hooks are more general.
- Thread/session stores and fork semantics are more deeply built into the core.

### What Not To Copy

- Do not let terminal-first design dominate Xero's desktop-native strengths.
- Do not duplicate Codex's protocol shape exactly if Xero's context manifests need richer memory/retrieval metadata.
- Do not build sandboxing only as a compatibility veneer; it must integrate with Xero's approval modes, rollback, and state machine.

## OpenHands

### What OpenHands Is Best At

OpenHands is strongest as an operational product platform. It has a web app, FastAPI server, sandbox services, event services, conversation lifecycle services, integrations, and cloud/enterprise orientation.

The inspected code shows a mature conversation-start pipeline:

1. Create or find conversation metadata.
2. Wait for sandbox availability.
3. Prepare the repository.
4. Run setup scripts.
5. Install git hooks.
6. Install skills.
7. Start the conversation through the agent server.
8. Attach callbacks and processors.
9. Process pending messages.
10. Mark the conversation ready or failed with visible status.

That lifecycle is one of the clearest product advantages over Xero's current harness surface.

### Capabilities Worth Copying

- Conversation lifecycle as a visible state machine with user-readable progress phases.
- Sandbox service abstraction with start, resume, wait, pause, delete, search, and grouping policies.
- Docker, local, and remote sandbox options behind one service contract.
- Agent server separation for scalable or remote execution.
- REST API for conversations, events, sandbox state, settings, secrets, status, and exports.
- Event persistence/query/streaming with pagination, filters, and storage backends.
- Conversation search, count, lifecycle metadata, tags, public/private status, PR numbers, parent/sub conversations, and metrics.
- Setup scripts and repository preparation as first-class workflow steps.
- Git hooks and skill installation during conversation startup.
- Pending message handling while a sandbox or conversation is still starting.
- Event callbacks/processors, such as automatic title generation.
- Trajectory export as a zip or portable artifact.
- Issue/PR workflow integrations across GitHub, GitLab, Bitbucket, Azure DevOps, Forgejo, Jira, Linear, Slack, and automation triggers.
- ACP support for launching external agent CLIs such as Claude Code, Codex, Gemini CLI, and custom agents.
- User-scoped secrets and integration tokens with provider-specific handling.

### Where Xero Is Already Ahead

- Xero is local-first and desktop-native instead of primarily web/server-oriented.
- Xero has richer durable context and memory policy in the inspected code.
- Xero can coordinate local OS resources, app-data state, browser automation, mobile emulators, and domain tools in a single desktop experience.
- Xero's approval and tool policy are tightly coupled to operator UX.

### Where OpenHands Is Ahead

- Environment lifecycle is much more explicit.
- Remote and container sandbox management is more productized.
- Conversation CRUD/search/filter/pagination is more mature.
- Integrations are broader and more workflow-oriented.
- The agent server split is a useful scaling pattern.
- External agent CLI hosting through ACP is a pragmatic way to add capability quickly.

### What Not To Copy

- Do not turn Xero into a web-only control plane.
- Do not make remote sandboxing mandatory for local development.
- Do not make Xero's local state depend on `.xero/`; new project state must stay under OS app-data.
- Do not copy enterprise integration breadth before the core harness boundary is stable.

## ForgeCode

### What ForgeCode Is Best At

ForgeCode is strongest as a terminal product and distribution package. The npm package is easy to install, ships prebuilt binaries, detects platform/libc/architecture, and exposes a broad command surface.

The visible product surface includes:

- `forge` terminal command.
- Provider login/logout/list.
- Conversation list/new/dump/compact/retry/resume/show/info/stats/clone/delete/rename.
- MCP import/list/remove/show/reload/login/logout.
- Workspace semantic sync/list/query/info/delete/status/init.
- Natural-language command suggestion.
- Commit generation.
- VS Code integration.
- Zsh integration.
- Agent, provider, model, config, tool, MCP, conversation, command, skill, and file listing.
- `--sandbox` isolated git worktree mode.
- Broad provider matrix, including standard APIs, local providers, OpenRouter, Requesty, Bedrock, Vertex, OpenAI-compatible endpoints, Codex, Claude Code, OpenCode variants, and many hosted model vendors.

### Capabilities Worth Copying

- Very low-friction install and launch path.
- Prebuilt binary packaging for macOS, Linux glibc, Linux musl, Windows, and Android/Termux.
- Terminal-first command surface for every major agent operation.
- Provider catalog command that works even before a full session starts.
- File-based provider login flow with migration away from environment-only credentials.
- Conversation command set: retry, clone, compact, dump, stats, show, info, delete, and rename.
- Semantic workspace index commands as product features, not hidden internals.
- Natural-language shell command suggestion.
- Commit-message generation.
- Configurable max requests per turn, max tool failures, tool timeout, retry policy, compact thresholds, reasoning settings, and subagent flags.
- Doom-loop detection, pending-todo reminders, todo verification, and max-failure limits.
- Built-in agent role separation: implementation, planning, and research.
- Strong prompt discipline around todo writing, semantic search, specialized file tools, and subagent use.
- MCP OAuth/login/logout ergonomics.
- Skills and custom command support through local config.

### Where Xero Is Already Ahead

- Xero is auditable from source in the inspected project, while the local ForgeCode package hides most core behavior in binaries.
- Xero's durable context manifest and memory model are more explicit.
- Xero has richer desktop UI, operator approvals, and domain tool integrations.
- Xero is better positioned for local visual workflows such as browser, emulator, and app-specific tools.

### Where ForgeCode Is Ahead

- Install and distribution are simpler.
- Terminal command ergonomics are much more complete.
- Provider matrix and provider listing are broader.
- Conversation management commands are mature.
- Semantic workspace sync/search is productized.
- Runaway guardrails are highly visible in config.

### What Not To Copy

- Do not depend on a remote semantic search service for Xero's core experience.
- Do not hide critical harness behavior behind opaque binaries.
- Do not make terminal UX the only control surface; it should complement the desktop app.

## Competitive Matrix

| Dimension | Xero Today | Codex | OpenHands | ForgeCode | Target For Xero |
| --- | --- | --- | --- | --- | --- |
| Core harness | Strong but Tauri-coupled | Best reusable Rust core | SDK/server split, core external | Opaque binary core | Extract reusable `xero-agent-core` plus daemon/CLI bindings |
| Protocol | Durable event records and manifests | Strong typed SQ/EQ protocol | REST/event services | CLI command interface | Typed submission/event protocol shared by app, CLI, tests, MCP |
| Provider abstraction | Broad and documented | Strong model client/config | LiteLLM/SDK/provider config | Very broad catalog | Keep current adapters, add provider catalog, ACP, external CLI adapters |
| Tool registry | Broad and policy-rich | Strong handler traits/hooks | Tool selection via agent server | Rich hidden tool surface | Handler trait, hooks, provenance, budgets, typed errors, safe parallel reads |
| Safety policy | Approval, redaction, path checks, rollback | Strong OS sandbox profiles | Container/remote sandboxes | Worktree sandbox option | Combine central policy with OS/container enforcement |
| Execution sandbox | Command policy, no clear OS sandbox | Best inspected sandboxing | Best operational sandbox service | Worktree isolation | Permission profiles plus local OS sandbox plus optional Docker/remote sandbox |
| Context continuity | Best inspected manifests/memory | Strong history/thread store | Conversation/event persistence | Compaction commands | Preserve Xero lead, add CLI commands and trace exports |
| Retrieval | Project records and memory via SQLite/LanceDB | Skills/history/context | Not primary in inspected app server | Semantic workspace sync/search | Add local semantic code index alongside project memory |
| Multi-agent | Existing subagent/tooling | Strong primitives and limits | Parent/sub conversations | Subagents and roles | Role registry, budgets, lineage, wait/follow-up, pane-contained child activity |
| Active coordination | Per-run events and in-memory subagent write sets | Thread events and task state | Conversation events | Hidden if present | Temporary cross-session swarm state for active panes and child runs |
| Conversation lifecycle | Runs/sessions/lineage | Threads/forks/resume | Best start/status workflow | Best terminal commands | Observable lifecycle phases plus conversation CLI |
| UI | Desktop-native Tauri | TUI/IDE/desktop | Web app/cloud GUI | Terminal/VS Code/zsh | Desktop first, with CLI/TUI/headless companions |
| Integrations | Notifications, MCP, providers, domain tools | MCP/hooks/plugins | Best external workflow integrations | MCP/provider/VS Code | Add issue/PR/resolver integrations after core extraction |
| Observability | Strong internal plan | Rollout traces | Trajectory export | Unknown | Trace viewer, redacted support bundles, and quality gates |
| Distribution | Tauri app | CLI/TUI/installers | Docker/web/cloud | Best npm binary package | App plus `xero` CLI, npm/brew/install scripts |

## Definition Of "Best Combination"

Xero should not try to become a clone of any one competitor. The target should be:

1. A reusable harness core that can run from the desktop app, CLI, MCP server, headless daemon, or tests.
2. A permission and sandbox model that gives local users confidence equivalent to Codex while preserving Xero's approval UX.
3. A visible environment lifecycle equivalent to OpenHands for projects, sandboxes, setup, hooks, and readiness.
4. Terminal workflows as smooth as ForgeCode for conversation, provider, MCP, workspace, commit, and command-suggestion operations.
5. Durable memory, retrieval, and context continuity that remain a Xero differentiator.
6. Domain tool packs that are more ambitious than generic coding agents: browser, emulator, Solana, OS automation, and project-specific plugins.

## Improvement Plan: Core Harness Outward

## Phase 1: Extract A Reusable Core Harness

### Build

- Extract the owned-agent runtime into a reusable Rust crate, tentatively `xero-agent-core`.
- Keep Tauri commands as adapters over the core, not owners of the core.
- Define a stable runtime facade:
  - `start_run`
  - `continue_run`
  - `submit_user_input`
  - `approve_action`
  - `reject_action`
  - `cancel_run`
  - `resume_run`
  - `fork_session`
  - `compact_session`
  - `export_trace`
- Move shared state transitions, provider loop, context package assembly, tool registry, tool dispatch, and persistence contracts behind core interfaces.
- Make the storage layer pluggable enough for:
  - Desktop app-data SQLite/LanceDB.
  - Test in-memory store.
  - Future remote or server store.

### Acceptance Criteria

- The desktop app can still run owned-agent tasks through the same visible UX.
- A non-Tauri integration test can start a fake-provider run and receive the same events as the app.
- Core APIs do not depend on frontend state, window handles, or Tauri command types.
- Context manifest persistence remains mandatory before every provider turn.

### Inspiration

- Codex `codex-core` separation.
- OpenHands agent-server separation.

## Phase 2: Add A Typed Submission/Event Protocol

### Build

- Define a versioned protocol crate for all runtime inputs and outputs.
- Model submissions such as:
  - Start run.
  - Continue run.
  - User message.
  - Approval decision.
  - Tool permission grant.
  - Cancel.
  - Fork.
  - Compact.
  - Provider/model change.
  - Runtime settings change.
- Model events such as:
  - Run started/completed/failed.
  - State transition.
  - Message delta.
  - Reasoning summary.
  - Tool started/delta/completed.
  - Policy decision.
  - Approval required.
  - Plan updated.
  - Verification gate.
  - Context manifest recorded.
  - Retrieval performed.
  - Memory candidate captured.
  - Sandbox lifecycle update.
- Attach trace IDs to runs, provider turns, tool calls, approval decisions, and storage writes.
- Generate TypeScript types for the frontend from the Rust protocol or maintain a schema-tested mirror.

### Acceptance Criteria

- Desktop, CLI, and tests consume the same event model.
- Event schemas are snapshot-tested.
- A recorded protocol trace can be replayed enough to reconstruct a run timeline.
- Protocol version mismatch fails explicitly.

### Inspiration

- Codex protocol queue/event architecture.
- OpenHands event services.

## Phase 3: Tool Registry V2

### Build

- Introduce a tool handler trait with:
  - Descriptor generation.
  - Input schema validation.
  - Capability tags.
  - Effect class.
  - Mutating/read-only classification.
  - Sandbox requirement.
  - Approval requirement.
  - Pre-hook payload.
  - Post-hook payload.
  - Telemetry attributes.
  - Result truncation contract.
- Preserve the current policy-rich descriptors, but make handlers easier to register outside the monolithic runtime.
- Add safe parallel scheduling for read-only tools:
  - File reads.
  - Search.
  - Metadata queries.
  - Retrieval.
  - Provider-independent diagnostics.
- Keep mutating tools sequential by default.
- Add explicit tool budgets:
  - Max tool calls per turn.
  - Max tool failures per turn.
  - Max repeated equivalent calls.
  - Max command output bytes.
  - Max wall-clock time per tool group.
- Add doom-loop detection:
  - Same failing tool repeated.
  - Same file read repeated without new context.
  - Pending todos ignored after claimed completion.
  - Verification requested repeatedly without changed inputs.
- Add typed tool error categories:
  - Invalid input.
  - Policy denied.
  - Approval required.
  - Sandbox denied.
  - Timeout.
  - External dependency missing.
  - Tool unavailable.
  - Retryable provider/tool failure.

### Acceptance Criteria

- Existing tools migrate without losing approval, redaction, rollback, or event behavior.
- Read-only tool batches can run in parallel where safe.
- Tool results include structured truncation metadata.
- The model receives useful failure messages without exposing secrets or system internals.
- Scoped tests cover invalid descriptor input, policy denial, approval waiting, rollback, repeated failure, and read-only parallel dispatch.

### Inspiration

- Codex tool handler registry and hooks.
- ForgeCode max-failure and max-request guardrails.
- Xero's existing central policy and rollback system.

## Phase 4: Sandboxed Execution Profiles

### Build

- Define permission profiles:
  - Read-only.
  - Workspace write.
  - Workspace write with network denied.
  - Workspace write with network allowed.
  - Full local with approval.
  - Dangerous unrestricted mode.
- Enforce project trust before enabling write or command tools.
- Add OS-level sandbox support:
  - macOS sandbox-exec profile for file and network boundaries.
  - Linux bubblewrap or equivalent where available.
  - Windows restricted execution strategy.
- Protect internal state:
  - `.git` mutation requires explicit policy.
  - OS app-data state is never treated as ordinary project working files.
  - Legacy `.xero/` stays read-only or ignored unless explicitly migrated by a planned migration.
- Add command execution metadata:
  - Sandbox profile.
  - Network mode.
  - Writable paths.
  - Environment redaction summary.
  - Approval source.
  - Exit classification.
- Keep rollback checkpoints for workspace mutations.

### Acceptance Criteria

- A denied write outside the workspace fails at the sandbox layer even if policy validation misses it.
- A denied network command cannot reach the network under a network-denied profile.
- The UI can explain why a command was blocked and which profile applied.
- Scoped tests cover macOS locally first, with portable abstractions for other platforms.

### Inspiration

- Codex sandboxing.
- OpenHands container sandbox abstraction.
- Xero approval and rollback policy.

## Phase 5: Environment Lifecycle Service

### Build

- Introduce an environment lifecycle state machine:
  - Created.
  - Waiting for sandbox.
  - Preparing repository.
  - Loading project instructions.
  - Running setup scripts.
  - Setting up hooks.
  - Setting up skills/plugins.
  - Indexing workspace.
  - Starting conversation.
  - Ready.
  - Failed.
  - Paused.
  - Archived.
- Add optional setup scripts defined in trusted project/app config.
- Add git hook setup as an explicit approval-gated action.
- Add pending user messages while an environment is still starting.
- Add environment health checks:
  - Filesystem accessible.
  - Git state available.
  - Required binaries available.
  - Provider credentials valid.
  - Tool packs available.
  - Semantic index status.
- Add sandbox grouping policies for future remote/container execution:
  - None.
  - Reuse newest.
  - Reuse least busy.
  - Reuse by project.
  - Dedicated per session.

### Acceptance Criteria

- Starting a run emits progress events visible in the app and available to CLI/headless clients.
- A setup failure leaves an actionable diagnostic and does not start the agent loop blindly.
- Pending messages are queued and delivered once the environment is ready.
- Health checks are persisted and exportable with a trace.

### Inspiration

- OpenHands conversation start lifecycle.

## Phase 6: Local Semantic Workspace Index

### Build

- Add a local-first semantic code index separate from, but connected to, project memory.
- Index:
  - File summaries.
  - Symbols.
  - Imports.
  - Tests.
  - Routes/components/commands.
  - Recent diffs.
  - Build/test failure snippets.
- Expose commands:
  - `workspace index`
  - `workspace status`
  - `workspace query`
  - `workspace explain`
  - `workspace reset`
- Add agent tools:
  - Semantic code search.
  - Symbol-aware lookup.
  - Related test discovery.
  - Change-impact search.
- Store under OS app-data, not repo-local legacy state.
- Make indexing incremental and cancellable.
- Add retrieval diagnostics that show what was used without overwhelming the user.

### Acceptance Criteria

- The agent can find relevant files in medium-sized repos without broad shell searches.
- The UI can show index freshness and coverage.
- Index writes do not touch `.xero/`.
- Workspace queries are available from CLI and app.

### Inspiration

- ForgeCode workspace sync/query.
- Xero LanceDB-backed project records.

## Phase 7: Headless CLI And Daemon

### Build

- Add a `xero` CLI backed by `xero-agent-core`.
- Initial commands:
  - `xero agent exec`
  - `xero conversation list`
  - `xero conversation show`
  - `xero conversation dump`
  - `xero conversation compact`
  - `xero conversation retry`
  - `xero conversation clone`
  - `xero conversation stats`
  - `xero provider list`
  - `xero provider login`
  - `xero provider doctor`
  - `xero mcp list`
  - `xero mcp add`
  - `xero mcp login`
  - `xero workspace index`
  - `xero workspace query`
  - `xero commit-message`
  - `xero suggest-command`
- Add JSON output mode for automation.
- Add non-interactive CI mode with strict sandbox defaults.
- Add a local daemon mode only if it reduces startup cost or enables shared event streaming.
- Package binaries for macOS, Linux, and Windows. Consider npm and Homebrew distribution once the CLI is stable.

### Acceptance Criteria

- A user can complete a simple code edit from CLI without opening the Tauri app.
- CLI runs create the same durable sessions/events/manifests as desktop runs.
- CLI conversation commands can inspect desktop-created sessions.
- Provider diagnostics use the same backend as the app.

### Inspiration

- Codex `exec` and TUI.
- ForgeCode command surface.

## Phase 8: MCP Server And External Agent Hosting

### Build

- Add a Xero MCP server exposing controlled capabilities:
  - Start run.
  - Query conversation.
  - Query workspace index.
  - Fetch project memory.
  - Invoke approved tool packs.
  - Export traces.
- Add ACP-style external agent hosting:
  - Launch Codex CLI, Claude Code, Gemini CLI, or compatible agents as subprocess-backed sessions.
  - Capture their events into Xero conversation records where possible.
  - Apply Xero approval and sandbox policy around subprocess execution.
  - Clearly label external-agent provenance.
- Add provider catalog entries for external agent adapters separately from normal model providers.

### Acceptance Criteria

- Xero can serve as an MCP-accessible local harness without exposing dangerous tools by default.
- External agent sessions are auditable and isolated from owned-agent runs.
- Provider/model mismatch cannot accidentally drive the wrong run.

### Inspiration

- Codex MCP server.
- OpenHands ACP support.
- ForgeCode provider catalog entries for external agents.

## Phase 9: Multi-Agent And Role System

### Build

- Keep the UI model explicit:
  - Panes are top-level main-agent sessions.
  - Subagents are children inside the owning pane's main-agent runtime.
  - Spawning a subagent must not create a new pane.
  - Child-agent activity can appear as trace, attribution, or collapsible lineage inside the existing pane.
- Formalize a role registry:
  - Engineer.
  - Debugger.
  - Planner.
  - Researcher.
  - Reviewer.
  - Agent Builder.
  - Domain specialists such as Browser, Emulator, Solana, Database.
- Add role-specific tool policies and verification contracts.
- Expand subagent lifecycle:
  - Spawn.
  - Send input.
  - Wait.
  - Follow up.
  - Interrupt.
  - Close.
  - Export child trace.
- Add budgets:
  - Max child agents.
  - Max depth.
  - Max concurrent child runs.
  - Max delegated tool calls.
  - Max delegated token/cost budget.
- Integrate with the existing multi-pane workspace plan:
  - Up to six panes.
  - Independent sessions.
  - Focused pane command routing.
  - Pane-contained child-run lineage.
  - File-change attribution by top-level pane and child agent where applicable.

### Acceptance Criteria

- Parent and child runs have explicit lineage and trace IDs.
- Delegated runs cannot escalate tools beyond their assigned policy.
- The UI can show which agent changed which files.
- Subagents never appear as independent workspace panes.
- Multi-agent scenario tests cover research plus implementation, debug plus verification, and planner plus engineer workflows.

### Inspiration

- Codex multi-agent primitives.
- ForgeCode implementation/planning/research roles.
- Xero's existing multi-pane workspace plan.

## Phase 10: Active Agent Coordination Bus

### Current Code Baseline

- Durable project records and approved memories already exist under OS app-data, with LanceDB-backed storage, freshness, supersession, redaction, retrieval logs, and context manifests.
- The `project_context` tool already lets agents search, read, record, update, and refresh durable context.
- Automatic memory extraction already creates durable memory records from run transcripts, with duplicate and redaction checks.
- Provider turns already persist context manifests; durable project context is tool-mediated instead of raw memory being treated as prompt authority.
- Agent sessions, runs, events, messages, tool calls, and file changes are persisted per run.
- Multi-pane workspace state already maps panes to top-level agent sessions in the frontend.
- Subagent write-set conflict checks already exist, but only inside one parent runtime's in-memory subagent task store.

The missing piece is not another durable memory layer. The missing piece is short-lived, cross-session awareness for active work.

### Build

- Add an app-data-backed temporary coordination plane for active top-level sessions and their child runs.
- Store structured state in SQLite first:
  - Active run presence.
  - Pane/session/run identity.
  - Current phase or activity summary.
  - File reservations.
  - Recent coordination events.
  - Expiration timestamps.
- Do not overload durable `agent_memories` or `project_records` for this state.
- Do not write temporary coordination state under `.xero/`.
- Treat every coordination row as TTL-scoped and garbage-collectable.
- Publish presence from active runs:
  - Run started.
  - Provider turn started.
  - Tool call started/completed.
  - File observation.
  - File write intent.
  - File changed.
  - Verification started/completed.
  - Run completed/failed/cancelled.
- Add file reservations:
  - Repo-relative path or path prefix.
  - Operation such as observing, editing, refactoring, testing, or verifying.
  - Owning agent session, run, child run, role, and pane when known.
  - Optional note describing intent.
  - Lease duration and renewal heartbeat.
  - Release on completion, cancellation, integration, or timeout.
- Make reservations advisory, not filesystem locks:
  - Warn agents before overlapping work.
  - Require an explicit override reason to proceed through a conflict.
  - Record override events for audit.
- Feed active coordination into provider turns as low-priority context:
  - Include only active same-project sessions with recent activity.
  - Prefer concise summaries over raw event streams.
  - Include file reservations before tool use and write attempts.
  - Record coordination contributor IDs in context manifests.
- Add an `agent_coordination` read surface:
  - List active agents.
  - List file reservations.
  - Check conflicts for a path set.
  - Claim and release file reservations.
  - Explain recent active-agent activity.
- Keep UI changes minimal:
  - Existing panes remain the only top-level agent containers.
  - Show passive conflict/presence indicators inside existing pane chrome or activity surfaces.
  - Do not create panes for subagents or mailbox messages.

### Acceptance Criteria

- Two active top-level sessions in the same project can see each other's recent activity without using durable memory.
- If one active session reserves a file or directory, another active session gets a conflict warning before editing overlapping paths.
- Stale reservations expire automatically when a run stops heartbeating.
- Subagent worker write sets are visible to sibling top-level sessions as reservations while the child run is active.
- Provider context manifests identify which active-coordination records were included.
- Tests cover reservation overlap, lease expiry, explicit override, child-run publication, and completed-run cleanup.

### Inspiration

- Xero's existing run heartbeat and file-change records.
- Xero's existing frontend multi-pane layout.
- Xero's existing in-memory subagent worker write-set conflict guard.

## Phase 11: Swarm Mailbox And Temporary Memory

### Build

- Add a temporary agent mailbox on top of the coordination bus.
- Keep it separate from durable project memory:
  - No mailbox item becomes approved memory automatically.
  - Durable promotion must use the existing project-context or memory-review flow.
  - Mailbox records expire unless explicitly promoted.
- Define mailbox item types:
  - Heads-up.
  - Question.
  - Answer.
  - Blocker.
  - File-ownership note.
  - Finding-in-progress.
  - Verification note.
  - Handoff-lite summary.
- Scope each item:
  - Project.
  - Target session or all active sessions.
  - Parent run or child run.
  - Role.
  - Related paths.
  - Priority.
  - TTL.
- Add agent actions:
  - Publish message.
  - Read inbox.
  - Acknowledge.
  - Reply.
  - Mark resolved.
  - Promote to durable context candidate.
- Add swarm summaries for provider turns:
  - "What other active agents are doing."
  - "Files to avoid."
  - "Questions waiting for this agent."
  - "Recent blockers or verification results."
- Use SQLite for the first implementation.
- Add a separate TTL LanceDB dataset only if semantic mailbox search becomes necessary; do not mix temporary mailbox vectors into durable memory/project-record datasets.
- Keep the operator in control:
  - Show a compact activity trail.
  - Let users clear temporary swarm state for a project.
  - Never let temporary mailbox content override user instructions, tool policy, or current file evidence.

### Acceptance Criteria

- Active sessions can publish and read temporary mailbox items without creating durable memories.
- Agents receive concise "swarm awareness" before risky edits and provider turns.
- Mailbox items expire or resolve and do not pollute durable retrieval.
- A mailbox item can be promoted into the existing durable-context review path with provenance.
- Tests cover publish/read/ack/reply/resolve, TTL expiry, scoped delivery, prompt-injection filtering, and promotion.

### Inspiration

- Xero's durable context manifests and retrieval diagnostics.
- Xero's existing project-context promotion path.
- Swarm-style coordination without turning panes into subagent containers.

## Phase 13: Provider Breadth And Diagnostics

### What This Actually Delivers

This phase is still needed, but it should not be framed as "add provider setup." Xero already has first-class provider presets, app-local credentials, OpenAI-compatible recipes, model catalog refresh/cache behavior, quick and extended doctor reports, redacted diagnostic output, and per-profile connection checks.

Code review update: the app already has a shared provider capability contract in the frontend model layer and `xero-agent-core`, backend model-catalog cache/TTL plumbing, a Provider "Check" action, composer capability badges, doctor reports, CLI provider list/doctor commands, and runtime guards that block obvious owned-agent incompatibilities such as missing tool-call support. The remaining user value is narrower: prove the exact selected model path before the run starts, record the proof used for that run, and make inferred/manual/cached truth impossible to mistake for a live green light.

The user-facing gap is confidence before a run starts. A user should be able to open Providers or the future CLI, choose a provider/model, and know:

- Whether the credential, ambient auth, endpoint, model, stream path, and tool-call path work right now.
- Whether the selected model supports the agent features Xero will use: streaming, function/tool calls, reasoning controls, image/document input, context size, and usable output limits.
- Whether Xero is using live provider truth, cached truth, manual truth, or an unverified fallback.
- Whether a provider is a normal model API, a local runtime, an ambient-cloud runtime, or an external subscription-backed agent CLI.
- Whether a failure is local setup, auth, model availability, schema incompatibility, rate limit, provider outage, or an unsupported capability.

The phase should deliver fewer mysterious failed agent starts, safer model selection, clearer support reports, and one provider capability source shared by the app, CLI, and owned-agent runtime.

### Build

- Promote the existing preset and model-catalog work into a single provider capability catalog.
  - Keep provider identity, runtime family, auth method, credential proof, endpoint shape, model-list strategy, cache state, and default model in one shared contract.
  - Add capability fields per provider family and per model where known:
    - Streaming: supported, probed, unavailable, or not applicable.
    - Tool calls: supported, strictness behavior, schema dialect, parallel-call behavior, and known incompatibilities.
    - Reasoning controls: effort levels, summary support, provider-specific clamping, and unsupported-model fallback.
    - Vision/document input: supported attachment types and provider-family limits.
    - Context window and max output: live catalog, known static table, manual, or unknown confidence.
    - Transport mode: hosted API, OpenAI-compatible API, local API, ambient-cloud API, cloud CLI bridge, or external agent CLI.
    - Cost hints when the provider exposes usable metadata; never block on cost data.
    - Known limitations and remediation copy that can be shown directly in the UI.
  - Reuse the current provider presets and model catalog cache instead of creating a second catalog path.
  - Expose the same catalog contract to the app, owned-agent runtime, and CLI.
- Tighten model catalog behavior already present in the app.
  - Surface cache age and TTL visibly in the composer, Providers settings, and doctor output.
  - Preserve the difference between live, cache, manual, and unavailable sources.
  - Do not hide manual catalog fallbacks behind "available" language; make them explicit.
  - Carry context-window and max-output metadata through the frontend contract, not only backend normalization.
  - Store provider/model cache under OS app-data only; never revive repo-local `.xero/` state.
- Add a preflight provider probe that runs without starting an agent run.
  - Return separate declared, inferred, cached, and probed statuses so a static capability table never masquerades as a live probe.
  - Validate credential or ambient auth.
  - Verify the selected model exists or explain that the provider is manual/unverified.
  - Send a minimal streaming probe where supported.
  - Send a minimal tool-call schema probe using a harmless echo/no-op tool.
  - Verify reasoning-effort request shape only when the selected model exposes reasoning controls.
  - Verify attachment support with metadata-only or fixture-free checks where possible.
  - Detect context-limit source and confidence.
  - Classify rate-limit and provider-error responses as retryable or blocking.
  - Produce a redacted request preview that shows route, model, enabled features, tool schema names, and non-secret headers/metadata.
  - Cache the last preflight result in app-data with profile id, provider id, model id, catalog source, cache age, probe age, and the exact feature set checked.
  - Persist the provider capability and preflight snapshot used for every provider turn beside the context manifest.
- Tighten provider-specific probe behavior.
  - OpenAI Codex and OpenAI-compatible providers: verify Responses/chat route shape, streaming, and a minimal no-op tool schema against the selected model.
  - OpenRouter: distinguish model-list availability from tool-call compatibility for the selected routed model.
  - Anthropic: verify Messages route, tool-use schema, streaming, and thinking controls separately.
  - GitHub Models: keep token setup simple, but mark model catalog and tool-call support as unverified unless the selected endpoint proves them.
  - Ollama and local APIs: treat server reachability, model presence, and tool-call support as separate checks; "server is running" is not enough.
  - Bedrock and Vertex: check ambient credentials, region/project, model access, and streaming/tool-call limits through provider-specific paths.
  - External subscription-backed CLIs: verify executable, version, approval/sandbox posture, and provenance labels; keep them out of normal model-provider execution.
- Make diagnostics actionable in the UI.
  - Add a "Check" action beside each configured provider and selected composer model.
  - Show a compact capability matrix for the selected model: streaming, tools, reasoning, vision/documents, context, catalog freshness.
  - Link failed checks to the exact repair surface: reconnect OAuth, paste key, fix base URL, refresh ambient cloud auth, start local server, choose another model, or switch provider.
  - Include copied doctor JSON for provider capability results with the existing redaction contract.
- Expand adapters only where diagnostics can prove the path works.
  - Add named OpenAI-compatible recipes only when endpoint shape, auth header behavior, model listing, and tool-call behavior are declared.
  - Treat local runtimes as local API providers with explicit "server running" probes.
  - Treat Bedrock and Vertex as ambient-cloud providers with region/project checks and clear non-streaming or CLI-bridge limits where applicable.
  - Add subscription-backed external CLIs through ACP/external-agent adapters, not as normal model APIs.
  - Keep external-agent catalog entries separate from owned-model providers so users do not accidentally bind the wrong runtime.
- Add guardrails before provider turns.
  - Block or warn before a long-running task when the chosen provider/model lacks required tool-call support.
  - Warn when the selected model is only present through cached or manual truth.
  - Explain when Xero disables or clamps a control, such as reasoning effort or parallel tool calls.
  - Persist the provider capability snapshot used for each provider turn alongside the context manifest.
  - Reuse the most recent successful preflight only when it matches the selected profile, model, catalog source, and required features; otherwise show it as stale and ask for a fresh check.

### Acceptance Criteria

- Provider setup and selected-model capability can be diagnosed without starting an agent run.
- The app, CLI, and owned-agent runtime consume the same provider capability catalog.
- The composer can show whether its selected model is live, cached, manual, or unavailable, including visible cache age/TTL.
- Tool-call incompatibility is detected before long-running tasks and produces a direct remediation.
- Reasoning controls, attachment support, context limits, and streaming support are either verified, explicitly inferred, or marked unknown.
- Bedrock, Vertex, local runtime, and OpenAI-compatible providers report transport-specific limits instead of pretending to be identical APIs.
- External subscription-backed agent CLIs appear as external-agent adapters, not normal model providers.
- Diagnostics redact secrets, secret-bearing paths, auth headers, and sensitive endpoint components by default.
- Provider doctor JSON includes capability results, cache metadata, and redacted request previews that are safe to share.
- Tests cover catalog contract validation, live/cache/manual/unavailable state transitions, TTL display data, redaction, per-profile diagnostics, selected-model preflight, harmless tool-call schema rejection, streaming failure classification, rate-limit classification, app-data preflight cache invalidation, persisted provider-turn snapshots, and external-agent separation.

### Inspiration

- ForgeCode provider list and config.
- Xero provider setup docs.
- Xero's existing provider presets, model catalog cache, provider diagnostics, and redacted doctor report contracts.

## Phase 14: Domain Tool Packs As Xero Differentiators

### Build

- Turn browser, emulator, Solana, OS automation, and project-specific tools into explicit tool packs.
- Each pack should have:
  - Manifest.
  - Tool descriptors.
  - Policy profile.
  - Required binaries/services.
  - Health check.
  - Scenario checks.
  - UI affordances.
  - CLI commands where useful.
- Browser pack:
  - Observe/control split.
  - Screenshot capture.
  - Interaction trace.
  - DOM/snapshot tools.
- Emulator pack:
  - Device lifecycle.
  - App install/launch.
  - Frame capture.
  - Gesture/input.
  - Log capture.
- Solana pack:
  - Wallet safety boundaries.
  - Network selection.
  - Transaction simulation.
  - Program/test workflow.
  - Explicit user approval for signing or value movement.

### Acceptance Criteria

- Tool packs can be enabled/disabled per agent policy.
- Missing prerequisites produce health diagnostics, not mysterious tool failures.
- Domain workflow checks prove the tools work beyond generic file editing.

### Inspiration

- Xero's existing unique tool breadth.
- Codex-style tool policy.

## Phase 15: Observability, Replay, And Quality Gates

### What This Actually Delivers

This phase is still useful, but only if it becomes a user-support workflow over the trace primitives Xero already has. It should not create a second trace system, duplicate the typed protocol work, or repeat Phase 13's provider diagnostics.

The user-facing deliverable is simple: when a run fails, stalls, uses surprising context, or is blocked by policy, the user can open the run and see what happened without manually replaying the task. They should be able to answer:

- Which provider/model and capability snapshot did this run use?
- What context manifest, retrieved records, and active coordination records reached the provider?
- Which tool call, approval, sandbox decision, provider retry, storage write, or verification gate changed the outcome?
- What can be shared with support safely?

For maintainers, this phase delivers trace-linked regression gates: a failed quality check should point at the event, manifest, policy decision, or provider-preflight category that regressed. The current code already has run events, trace IDs, context manifests, CLI trace export/conversation dump, fake-provider harness tests, and internal quality evals. The remaining work is to make those artifacts inspectable, redacted, and tied to gates.

### Build

- Add a desktop run timeline and support view over the existing event store and `xero-agent-core` trace export for:
  - Provider turns.
  - Provider capability/preflight snapshots from Phase 13.
  - Context manifests.
  - Retrieved records.
  - Active coordination and mailbox records included in provider context.
  - Tool registry snapshots.
  - Tool calls.
  - Approvals.
  - Sandbox decisions.
  - File changes.
  - Verification gates.
  - Memory captures.
  - Storage writes and storage errors.
  - Provider retries, rate limits, and response-shape failures.
- Add export formats:
  - JSON trace.
  - Markdown summary.
  - Redacted support bundle.
  - Keep all exports generated from the same trace snapshot so support bundles, CLI dumps, and the UI timeline do not disagree.
  - Redact secrets, bearer headers, OAuth tokens, cloud credential paths, private-key paths, secret-bearing URLs, raw file contents, and unapproved memory text by default.
  - Include app/runtime versions, provider diagnostic summaries, environment health, and relevant doctor checks without copying raw credentials.
- Add replay as deterministic timeline reconstruction, not provider/tool re-execution.
  - Rebuild the ordered run timeline from persisted events, messages, context manifests, file-change records, and trace IDs.
  - Show missing/corrupt event ranges explicitly.
  - Preserve the current raw transcript and compaction boundaries; do not silently mutate replay state.
- Wire quality gates to trace categories:
  - Prompt-injection regression pass.
  - Sandbox policy pass.
  - Provider capability/preflight pass owned by Phase 13.
  - Tool schema validation pass.
  - Context manifest determinism pass.
  - Event/protocol schema snapshot pass.
  - Support-bundle redaction pass.
- Add diagnostic signals:
  - Context pressure.
  - Tool failures.
  - Tool denials.
  - Approval waits.
  - Sandbox denials.
  - Provider retries.
  - Retrieval usage.
  - Redaction events.
  - Storage errors.
  - Stale, manual, cached, or unprobed provider capability state.
  - Missing timeline segments or trace/export conversion failures.

### Acceptance Criteria

- A failed quality gate points to a specific trace and regression category.
- A user or maintainer can inspect a run timeline without replaying the task manually.
- A failed run shows the most likely failing layer: provider, context assembly, retrieval, tool schema, approval, sandbox, filesystem, verification, storage, or redaction.
- The UI, CLI dump, MCP trace export, and support bundle are generated from the same canonical trace snapshot.
- Support bundles are redacted by default.
- Support bundles include enough provider, environment, context-manifest, tool, and verification metadata to diagnose common failures without raw secrets or raw repository contents.
- Existing fake-provider/core tests and quality evals remain the fast gate; new gates add trace-linked failure output instead of only pass/fail summaries.
- Phase 15 does not add a parallel provider-diagnostics system; selected-model preflight and provider fake-adapter coverage live in Phase 13.

### Inspiration

- Codex rollout traces.
- OpenHands trajectory export.
- Xero trace and context-manifest design.

## Prioritized Top Ten

If the team can only do ten things, do these in order:

1. Extract `xero-agent-core` and make Tauri an adapter.
2. Define the typed submission/event protocol and trace IDs.
3. Add non-Tauri fake-provider harness tests around context manifests, tools, approvals, and verification.
4. Implement Tool Registry V2 with handler traits, hooks, budgets, typed errors, and safe read-only parallelism.
5. Add Codex-style execution profiles with OS sandbox enforcement, starting on macOS.
6. Add an OpenHands-style environment lifecycle service with visible setup phases and health checks.
7. Add local semantic workspace indexing and expose it to both app and agent tools.
8. Ship a `xero` CLI with provider, conversation, workspace, and `agent exec` commands.
9. Add MCP server and ACP-style external agent hosting.
10. Add active-agent coordination with temporary file reservations and swarm mailbox state.

## Risks And Mitigations

| Risk | Why It Matters | Mitigation |
| --- | --- | --- |
| Core extraction stalls product work | Runtime touches persistence, providers, tools, UI events, and approvals | Extract behind compatibility adapters and keep desktop behavior unchanged during migration |
| Sandbox support becomes platform-specific chaos | macOS, Linux, and Windows differ sharply | Start with profile abstraction, ship macOS first, mark other platforms explicit instead of pretending |
| Feature breadth dilutes the harness | Integrations and domain packs can sprawl | Gate all extras behind core protocol, tool registry, health checks, and scenario checks |
| Semantic indexing creates stale or expensive state | Repo indexes can drift and consume storage | Store in OS app-data, incremental updates, clear status, explicit reset, scoped indexing |
| External agent hosting weakens auditability | ACP/subprocess agents may not emit rich events | Label provenance, sandbox subprocesses, capture stdout/stderr/events, and store separate trace classes |
| Provider catalog becomes unmaintainable | Vendor behavior changes often | Use capability probes and cached diagnostics instead of static claims only |
| CLI and desktop diverge | Two product surfaces can develop different semantics | Make both consume the same core protocol and storage APIs |
| Memory becomes too magical | Bad retrieval can mislead the model | Keep memory review, retrieval diagnostics, redaction, and prompt-injection filtering central |
| Temporary swarm state becomes stale or bossy | Agents may avoid files because of dead reservations or treat peer notes as instructions | TTL every row, require heartbeats, expose clear/reset controls, and keep swarm context lower priority than user intent and current file evidence |

## Concrete Milestones

## Milestone 1: Harness Core Boundary

Deliverables:

- `xero-agent-core` crate.
- Tauri adapter over core APIs.
- Fake-provider core integration tests.
- Protocol draft for run submissions and events.
- Trace ID attached to runs, provider turns, and tool calls.

Exit criteria:

- Desktop behavior remains intact.
- A headless test can run one fake-provider task end to end.
- Context manifest persistence is verified outside Tauri.

## Milestone 2: Safer, More Extensible Tools

Deliverables:

- Tool handler trait.
- Tool budget config.
- Structured tool errors.
- Safe read-only parallel dispatch.
- Pre/post hook skeleton.
- Result truncation metadata.

Exit criteria:

- Existing tool tests pass.
- New tests cover repeated failure, policy denial, approval required, and parallel reads.
- Tool registry snapshots include provenance, effect class, and sandbox requirement.

## Milestone 3: Sandboxed Commands

Deliverables:

- Permission profile model.
- macOS sandbox profile implementation.
- Command execution metadata.
- UI/CLI denial explanations.
- Sandbox policy tests.

Exit criteria:

- Network-denied and write-denied commands are blocked by enforcement.
- Approval mode and sandbox profile are visible in traces.
- Rollback remains functional for allowed workspace mutations.

## Milestone 4: Observable Environment Startup

Deliverables:

- Environment lifecycle states.
- Setup script and health-check support.
- Pending message queue.
- Workspace indexing startup phase.
- Diagnostic bundle for failed startup.

Exit criteria:

- Starting an agent run shows environment progress before provider calls.
- Setup failures are actionable.
- Pending user input is delivered when ready.

## Milestone 5: CLI And Workspace Index

Deliverables:

- `xero agent exec`.
- `xero conversation` commands.
- `xero provider` commands.
- `xero workspace` commands.
- Local semantic code index.

Exit criteria:

- CLI and desktop share sessions and events.
- Workspace query can explain why it selected relevant files in representative projects.
- Provider diagnostics run without starting an agent task.

## Milestone 6: Multi-Agent And External Agents

Deliverables:

- Role registry.
- Subagent lifecycle hardening.
- Pane-contained child-run lineage integration.
- MCP server.
- ACP external agent adapter.

Exit criteria:

- Parent/child runs have clear lineage.
- Subagents stay inside their owning top-level pane.
- External agent sessions are labeled and sandboxed.
- Multi-agent scenario tests pass with budget enforcement.

## Milestone 7: Active Agent Coordination Bus

Deliverables:

- Active run presence records.
- File reservation records with TTL leases.
- Coordination event publication from run/tool/file-change lifecycle.
- Conflict check before overlapping write attempts.
- Context-manifest contributors for active coordination.

Exit criteria:

- Two active top-level sessions can see each other's active work.
- Overlapping file reservations warn before writes and support explicit override.
- Child worker write sets are visible as active reservations.
- Stale reservations expire when heartbeats stop.

## Milestone 8: Swarm Mailbox And Temporary Memory

Deliverables:

- Temporary mailbox records.
- Agent coordination tool actions for publish/read/ack/reply/resolve.
- Scoped swarm summaries in provider context.
- Promotion path into existing durable context review.
- Clear/reset controls for temporary swarm state.

Exit criteria:

- Agents can communicate temporary blockers, questions, and file-ownership notes across active sessions.
- Mailbox items expire or resolve without polluting durable memory.
- Prompt-injection filtering applies before mailbox content reaches provider context.
- Tests cover scoped delivery, TTL expiry, acknowledgement, reply, resolve, and promotion.

## Milestone 9: Integrations And Domain Packs

Deliverables:

- Issue/PR resolver skeleton.
- Slack/Jira/Linear trigger skeleton.
- Browser/emulator/Solana tool pack manifests.
- Domain health checks and scenario checks.

Exit criteria:

- External writes require approval or trusted automation policy.
- Domain pack failures are diagnosed before agent execution.
- Xero retains local-first operation when integrations are disabled.

## What Xero Should Preserve

- Durable context manifests before every provider request.
- Local-first OS app-data storage.
- Memory candidate review and redaction.
- Project record retrieval and continuity.
- Approval-aware tool dispatch.
- Observation-before-write behavior.
- Rollback checkpoints.
- Narrow user-facing memory/continuity controls.
- Desktop-native operator UX.
- Browser, emulator, Solana, and OS automation ambitions.

These are not secondary features. They are the places where Xero can be meaningfully better than a terminal-only or cloud-first harness.

## What Xero Should Avoid

- A web-only architecture that makes the desktop app a thin shell.
- Remote-first semantic search or memory that weakens local ownership.
- Opaque binary distribution for core logic.
- Integration sprawl before the harness protocol is stable.
- Provider breadth without diagnostics.
- Tool packs without health checks and policy profiles.
- Multi-agent features without lineage, budgets, and clear operator control.
- Backwards compatibility with legacy `.xero/` state unless explicitly requested.

## Suggested Architecture Target

```text
Desktop UI (Tauri/React)
        |
        | typed submissions/events
        v
Xero Agent Core
        |
        |-- Provider adapters
        |-- Tool registry V2
        |-- State machine
        |-- Context package builder
        |-- Memory/retrieval interfaces
        |-- Sandbox profiles
        |-- Trace/event recorder
        |
        +--> App-data SQLite/LanceDB store
        +--> Local semantic workspace index
        +--> CLI
        +--> MCP server
        +--> Headless daemon/tests
        +--> Optional remote/container sandbox service
        +--> External ACP agent adapters
```

The app should become one excellent client of the harness, not the place where harness semantics are trapped.

## Final Recommendation

Start with the core. Xero's best long-term path is to make its owned-agent runtime as reusable and traceable as Codex, then layer in OpenHands-style lifecycle operations and ForgeCode-style CLI ergonomics. The current memory/context system is a differentiator worth protecting, but it should be exposed through a cleaner protocol and backed by stronger sandbox enforcement.

The strategic sequence is:

1. Core extraction.
2. Typed protocol and traces.
3. Tool registry and sandbox hardening.
4. Environment lifecycle and workspace indexing.
5. CLI/MCP/external agent surfaces.
6. Multi-agent roles inside main-agent panes.
7. Active-agent coordination bus with file reservations.
8. Swarm mailbox and temporary memory.
9. Integrations and domain tool packs.
10. Traceability and quality gates.

If Xero executes that sequence, it can become the best blend of the three competitors rather than a partial clone of any one of them: safer and more reusable than OpenHands, more product-complete than Codex, more transparent and desktop-native than ForgeCode, and more continuity-aware than all three.
