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
| Multi-agent | Existing subagent/tooling | Strong primitives and limits | Parent/sub conversations | Subagents and roles | Role registry, budgets, lineage, wait/follow-up, pane UI |
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
- Add mailbox-style summarization between agents.
- Integrate with the existing multi-pane workspace plan:
  - Up to six panes.
  - Independent sessions.
  - Focused pane command routing.
  - Shared project context.
  - Clear lineage display.

### Acceptance Criteria

- Parent and child runs have explicit lineage and trace IDs.
- Delegated runs cannot escalate tools beyond their assigned policy.
- The UI can show which agent changed which files.
- Multi-agent scenario tests cover research plus implementation, debug plus verification, and planner plus engineer workflows.

### Inspiration

- Codex multi-agent primitives.
- ForgeCode implementation/planning/research roles.
- Xero's existing multi-pane workspace plan.

## Phase 10: Product Workflow Integrations

### Build

- Add issue and PR resolver workflows:
  - GitHub.
  - GitLab.
  - Bitbucket.
  - Azure DevOps.
  - Forgejo.
- Add planning and task workflow integrations:
  - Jira.
  - Linear.
  - Slack.
- Add triggers:
  - Manual app trigger.
  - CLI trigger.
  - MCP trigger.
  - Scheduled automation.
  - PR comment.
  - Issue assignment.
  - Slack command.
- Add guarded output actions:
  - Draft PR description.
  - Commit message.
  - Branch summary.
  - Review summary.
  - Release note draft.
- Keep all credentials in OS app-data/keychain-backed storage.

### Acceptance Criteria

- Integrations use least-privilege credentials.
- A resolver run has a visible source trigger and external artifact links.
- The user approves external writes unless a trusted automation policy exists.
- Integration failures do not corrupt local sessions.

### Inspiration

- OpenHands app integrations.
- ForgeCode commit and command helpers.

## Phase 11: Provider Breadth And Diagnostics

### Build

- Add a provider catalog with:
  - Capabilities.
  - Auth method.
  - Streaming support.
  - Tool-call support.
  - Reasoning support.
  - Vision support.
  - Context window.
  - Known limitations.
  - Cost hints when available.
- Expand adapters carefully:
  - Additional OpenAI-compatible vendors.
  - Local runtimes.
  - Bedrock variants.
  - Vertex variants.
  - Subscription-backed external CLIs through ACP, not by pretending they are normal APIs.
- Add provider diagnostics:
  - Auth valid.
  - Model available.
  - Streaming works.
  - Tool call schema accepted.
  - Context limit detected.
  - Rate-limit behavior.
  - Redacted request preview.
- Cache model/provider lists with a visible TTL.

### Acceptance Criteria

- Provider setup can be diagnosed without starting an agent run.
- The app and CLI show the same provider catalog.
- Tool-call incompatibility is detected before long-running tasks.
- Diagnostics redact secrets by default.

### Inspiration

- ForgeCode provider list and config.
- Xero provider setup docs.

## Phase 12: Domain Tool Packs As Xero Differentiators

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

## Phase 13: Observability, Replay, And Quality Gates

### Build

- Add a trace viewer for:
  - Provider turns.
  - Context manifests.
  - Retrieved records.
  - Tool registry snapshots.
  - Tool calls.
  - Approvals.
  - Sandbox decisions.
  - File changes.
  - Verification gates.
  - Memory captures.
- Add export formats:
  - JSON trace.
  - Markdown summary.
  - Redacted support bundle.
- Add quality gates:
  - Prompt injection regression pass.
  - Sandbox policy pass.
  - Provider fake-adapter pass.
  - Tool schema validation pass.
  - Context manifest determinism pass.
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

### Acceptance Criteria

- A failed quality gate points to a specific trace and regression category.
- A maintainer can inspect a run timeline without replaying the task manually.
- Support bundles are redacted by default.

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
10. Add trace viewer, support bundles, and quality gates for prompt, tool, and sandbox regressions.

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
- Multi-pane lineage integration.
- MCP server.
- ACP external agent adapter.

Exit criteria:

- Parent/child runs have clear lineage.
- External agent sessions are labeled and sandboxed.
- Multi-agent scenario tests pass with budget enforcement.

## Milestone 7: Integrations And Domain Packs

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
6. Multi-agent roles and panes.
7. Integrations and domain tool packs.
8. Traceability and quality gates.

If Xero executes that sequence, it can become the best blend of the three competitors rather than a partial clone of any one of them: safer and more reusable than OpenHands, more product-complete than Codex, more transparent and desktop-native than ForgeCode, and more continuity-aware than all three.
