# OpenClaude Capability Gap Report

Date: 2026-04-25
Last updated: 2026-04-26

## Reader And Action

This report is for an engineer planning the next Cadence work. After reading it, they should be able to choose which OpenClaude capabilities still need to be brought into this project, which ones are already covered by Cadence in a different form, and which gaps are low priority because Cadence intentionally has a different product shape.

## Scope

The comparison covers the local OpenClaude repository and the current Cadence repository state. OpenClaude is a terminal-first coding-agent CLI with optional service and editor integrations. Cadence is a Tauri desktop app with a React interface, Rust command surface, app-local persistence, and desktop-native sidebars.

Some OpenClaude modules are feature-gated or internal-only. I counted a capability as OpenClaude-supported when it appears in the public README, package scripts, command registry, tool registry, provider/profile utilities, service modules, or extension docs. Feature-gated/internal capabilities are noted as such when they matter.

## Executive Summary

Cadence already covers the broad desktop shell, project import, file editing, provider profiles, OpenAI Codex OAuth, runtime supervision, operator approval flows, notifications, MCP registry management, in-app browser automation, emulator automation, and a large Solana workbench. In several areas Cadence is beyond OpenClaude, especially mobile device automation and Solana-specific tooling.

The largest remaining OpenClaude-equivalent area is not another sidebar. It is the mature agent operating system around the model loop: terminal CLI/REPL, broad command operations, memory/compaction, diagnostics, provider launch/profile repair workflows, web search breadth, and editor/remote integrations.

Cadence has a native owned-agent runtime and autonomous tool runtime, but it is not yet equivalent to OpenClaude's CLI agent runtime. The Priority 1 native tool-surface gap is now closed: owned agents have MCP invocation, subagents, todo/task state, notebook editing, PowerShell, LSP/code intelligence, tool search, and command-session support. Priority 2 is also closed: skills and plugins now have durable source registries, model-visible invocation, settings surfaces, trust controls, MCP/plugin/dynamic source coverage, and reload diagnostics. Remaining gaps are now concentrated in provider setup parity, runtime diagnostics, memory/context management, session operations, web search provider breadth, and external integration surfaces.

## OpenClaude Capability Inventory

### Product Form And Entrypoints

- npm-distributed `openclaude` CLI.
- Bun source-build and development workflow.
- Interactive terminal UI built around an Ink/React-style REPL.
- Headless gRPC server with a streaming chat protocol for tokens, tool calls, tool results, action-required prompts, final responses, and errors.
- Lightweight gRPC CLI client for testing the headless service.
- VS Code extension with a Control Center, project-aware terminal launch, profile awareness, command shortcuts, and theme support.
- Source-build, setup, quick-start, advanced, Android, LiteLLM, and local-agent playbook docs.

### Provider And Model Support

- OpenAI-compatible providers via `/v1`, including OpenAI, OpenRouter, DeepSeek, Groq, LM Studio, Together, Azure OpenAI-style endpoints, LiteLLM, and other compatible gateways.
- OpenAI Codex/Codex OAuth path with Codex model aliases such as `codexplan` and `codexspark`.
- Gemini through API key, access token, or ADC-style auth.
- GitHub Models and GitHub Copilot/Models onboarding through a device-flow style setup.
- Ollama local models with `ollama launch openclaude` support.
- Atomic Chat local Apple Silicon backend.
- Mistral direct provider mode.
- NVIDIA NIM and MiniMax profile support in provider profile utilities.
- Bedrock, Vertex, and Foundry environment modes.
- Provider profile persistence in `.openclaude-profile.json`.
- Provider recommendation and auto-selection workflows for local/openai profiles.
- Per-agent model routing through settings that map agent names/types to provider configs.
- Provider validation, diagnostics, reachability checks, and hardening scripts.

### Core Agent Loop

- Streaming model responses with tool-use progress.
- Multi-step tool execution with model follow-up turns.
- Tool-call history compression and output management.
- Provider-specific request shaping for chat/completions, responses, Anthropic-style APIs, and compatibility shims.
- Image inputs through URL/base64 support where providers support vision.
- Cost, usage, token, and context tracking.
- Session storage, resume, rename, export, branch, rewind, transcript search, and history search.
- Context visualization, context suggestions, auto-compact, micro-compact, session memory, and memory consolidation.

### Tool Surface

- Bash tool with permission checks, read-only validation, destructive command warnings, sandbox decisioning, path validation, and command semantics.
- PowerShell tool with Windows-specific command policy, constrained-language awareness, git safety, and destructive command warnings.
- File read, write, edit, grep, glob, and notebook edit tools.
- Web search and web fetch tools.
- Web search providers include DuckDuckGo fallback plus Firecrawl, Bing, Exa, Jina, Linkup, Mojeek, Tavily, You.com, and a custom provider path.
- MCP tools, resource listing, resource reading, and MCP auth support.
- Agent/subagent tool with built-in Explore, Plan, general-purpose, verification, and Claude Code guide agents.
- Team/swarm tools and send-message tools when the relevant feature flags are enabled.
- Todo/task tools for background work management.
- Skill tool and tool search.
- LSP tool when enabled.
- Cron/scheduled task tools.
- Plan mode and worktree tools.
- Ask-user-question, brief/upload, monitor, remote trigger, workflow, and browser/computer-use tools where enabled.

### Slash Commands And Workflows

OpenClaude exposes a large slash-command surface. Important user-visible groups include:

- Setup and status: provider, onboard-github, doctor, status, config, model, effort, output-style, theme, keybindings, terminal setup, upgrade, usage, rate-limit options.
- Session management: clear, compact, resume, rename, branch, rewind, export, copy, files, context, cost, stats.
- Development workflows: review, security-review, commit, commit-push-pr, auto-fix, init, init-verifiers, diff, pr-comments, release-notes.
- Agent operation: agents, tasks, plan, permissions, sandbox, skills, memory, hooks, plugin, reload-plugins, mcp.
- Integrations: ide, desktop handoff, chrome, remote-control/bridge, remote-env, mobile QR, install GitHub app, install Slack app.
- Knowledge and analysis: wiki, insights/project areas, dream/memory consolidation, thinkback.
- Feature-gated/internal commands also exist for bridge, assistant, voice, workflows, teleport, and other internal diagnostics.

### MCP, Plugins, And Skills

- MCP connection manager, stdio/http/SSE transports, server approval flows, auth helpers, official registry support, XAA/IDP helpers, and VS Code SDK MCP bridge.
- MCP tools, resources, prompts, and commands can be surfaced to the model.
- Plugin marketplace and plugin command loading.
- Skill loading from skill directories, bundled skills, plugin skills, built-in plugin skills, dynamic skills, and MCP-provided skills.
- Skill indexing and model-invocable skill filtering.

### Memory, Knowledge, And Context

- CLAUDE/project instruction handling and memory file detection.
- Memdir/team memory utilities.
- Session memory extraction and consolidation.
- Wiki initialization and ingestion.
- Auto-dream/memory consolidation.
- Context collapse, compaction, transcript search, and cross-project resume.

### Integrations

- VS Code companion extension.
- IDE status and integration hooks.
- Chrome/native host and Claude-in-Chrome paths where enabled.
- Remote control/bridge/direct connect/session URL.
- GitHub app and Slack app installation flows.
- Voice mode where enabled.
- Mobile QR/download helper.

### Safety, Privacy, And Diagnostics

- Allow/deny permission rules.
- Tool approval flows and classifier approval hooks.
- Sandbox toggles and violation display.
- Security review command.
- URL redaction, schema sanitization, provider error redaction, and secret handling.
- Runtime doctor commands and JSON reports.
- Privacy verification script for no-phone-home checks.
- Smoke, typecheck, hardening, and provider-specific tests.

## Cadence Capability Inventory

### Desktop Shell And Project Workflow

- Tauri desktop host with React, Vite, ShadCN/Radix UI, and Tailwind.
- Project import, project registry, project rail, and active-project loading.
- Repository status and diff.
- File tree, code editor, file create/rename/delete, read/write, search, and replace.
- Workflow, Agent, and Editor views.
- Onboarding for provider setup, project import, and notification routing.
- Settings dialog for providers, MCP, notifications, browser/development options, and themes.
- App-level and project-level persistence, including repo-local SQLite state.

### Provider Profiles And Runtime Sessions

- Provider profile UI and app-local store.
- OpenAI Codex OAuth flow with app-local OpenAI Codex session store.
- OpenRouter, Anthropic, GitHub Models, OpenAI API/custom OpenAI-compatible, Ollama, Azure OpenAI, Gemini AI Studio, Bedrock, and Vertex provider presets.
- Model catalog discovery for supported profiles.
- Credential readiness states for OAuth, stored secret, local, and ambient profiles.
- Active provider profile selection.
- Runtime session reconciliation and logout.

### Runtime And Operator Loop

- Detached runtime supervisor over a PTY with internal TCP control.
- Start, stop, reconnect, and probe runtime runs.
- Runtime stream subscription and normalized live events.
- Runtime run controls for model, thinking effort, approval mode, plan-mode requirement, and prompt updates.
- Operator action resolution and resume flow.
- Notification dispatch and reply handling.
- MCP registry projection into detached runtime launch environment.

### Native Owned-Agent Runtime

- Agent sessions and agent runs.
- Start task, send message, cancel run, resume run, get/list runs, and subscribe to agent stream.
- Provider adapters for OpenAI Responses, OpenAI-compatible chat, Anthropic, Bedrock, and Vertex.
- Tool registry with file, git, command, web, browser, emulator, Solana, MCP, subagent, todo, notebook edit, code intelligence, LSP, PowerShell, and tool-search descriptors.
- Safety decisions for allow, require approval, and deny.
- Persistent agent messages, tool calls, checkpoints, file changes, action requests, and run status.

### Autonomous Tool Runtime

- Filesystem tools: read, search, find, edit, write, patch, delete, rename, mkdir, list, and file hash.
- Git tools: status and diff.
- Process tools: one-shot command plus persistent command session start/read/stop.
- Web tools: fetch and search through a configured search provider.
- Browser automation tool wrapping the in-app browser.
- Emulator automation tool wrapping iOS/Android device commands.
- Solana tool group with clusters, logs, transactions, simulation, explanation, ALTs, IDL, Codama, PDA, program build/deploy/upgrade checks, Squads, verified builds, audits, replay, indexer, secrets, drift, costs, and docs.
- Priority agent tools: MCP tool/resource/prompt invocation, subagents, todo/task state, notebook cell edits, code intelligence, LSP server discovery/symbols/diagnostics with install suggestions, PowerShell wrapping, and tool search.
- Tool-access discovery/request behavior for expanding tool groups.

### MCP Registry

- App-local MCP registry.
- Stdio, HTTP, and SSE transport records.
- Environment-variable references without storing raw secret values.
- Import, upsert, remove, list, and refresh connection status.
- Connection truth probing and stale/failed/blocked/misconfigured states.
- Runtime MCP projection for connected servers.

### Browser, Emulator, And Solana Surfaces

- In-app tabbed browser sidebar with navigation, DOM actions, text extraction, query, waits, screenshots, cookies, storage, and cookie import.
- Browser cookie import source detection for common Chromium, Firefox-family, and Safari browsers.
- iOS and Android emulator sidebars with SDK status, device lifecycle, input, screenshots, UI tree, element find/tap/swipe/type/key, app lifecycle, location, push notification, and logs.
- Android SDK provisioning and bundled sidecar fetch/build behavior.
- Solana workbench sidebar with local/fork cluster controls, personas, transactions, deploys, IDL/Codama/PDA, audits, indexer, token, wallet, safety, docs, logs, and cost tooling.

### Notifications

- Telegram and Discord route configuration.
- Credential store and readiness validation.
- Dispatch records and reply submission.
- Notification health projection in runtime views.

## Gap Matrix

| Area | Cadence status | Still missing or not equivalent |
| --- | --- | --- |
| Terminal CLI/REPL | Desktop-only Tauri app with detached PTY runtime | No `openclaude`-style standalone CLI, terminal UI, npm binary, or direct terminal REPL surface |
| Headless external API | Internal Tauri IPC and supervisor TCP control | No public gRPC/HTTP streaming service equivalent to OpenClaude's AgentService |
| Slash commands | Desktop settings/actions, plugin-contributed command projection, and runtime controls | No broad command-operations equivalent for provider doctor, compact, memory, mcp, review, security-review, permissions, usage, and related workflows |
| Provider launch profiles | App-local provider profile store | No `.openclaude-profile.json` compatibility, profile launcher scripts, provider recommendation, or CLI profile bootstrap |
| Provider breadth | Strong built-in presets for common cloud/local providers | Missing direct Mistral, Atomic Chat, NVIDIA NIM, MiniMax, Foundry, LiteLLM-oriented docs/profile flow, and GitHub device onboarding |
| Per-agent routing | Active profile/model controls | No settings-level agent model routing by agent name/type |
| Core tool parity | Native autonomous tools for files/git/commands/web/browser/emulator/Solana plus Priority 1 agent tools | Priority 1 is complete; still missing cron/monitor and first-class AskUserQuestion equivalents |
| Subagents | Native owned-agent subagent tool with built-in Explore/Plan/general/verification types and model routing | Still missing custom agent definitions and team/swarm tools |
| MCP runtime use | Registry, probes, projection to detached runtime, and native owned-agent MCP tool/resource/prompt invocation | Still missing broader MCP auth/server approval UX and marketplace-style discovery |
| Plugins | Cadence-native plugin roots, manifests, trust state, settings UI, contributed skills/commands, reload, and diagnostics | Still missing a public marketplace/distribution story and broader command-palette integration; no need to clone OpenClaude slash-command UI exactly |
| Skills | Durable installed skills across local/project/bundled/GitHub/dynamic/plugin/MCP sources, SkillTool discovery/invoke, settings UI, trust controls, reload, and diagnostics | Mostly covered for native parity; remaining work is polish around default bundled catalog breadth and future command/agent workflow integrations |
| Memory/context | Runtime stream and project DB state | No OpenClaude-equivalent memory files, memdir/team memory, auto-dream, session memory extraction, context visualization, auto-compact, or transcript search |
| Session operations | Agent sessions/runs and runtime sessions | Missing user-facing resume/rename/export/branch/rewind/compact/copy/history search equivalents |
| Web search | Configurable search provider endpoint | No built-in free DuckDuckGo fallback or first-class Firecrawl/Bing/Exa/Jina/Linkup/Mojeek/Tavily/You providers |
| Web fetch | Basic HTTP plus HTML/plain extraction | No Firecrawl scrape path for JS-rendered or blocked pages |
| Attachments/media | Code editor, browser screenshots, and notebook cell editing | No OpenClaude-style image input pipeline, base64/url images as model attachments, or PDF/image processing |
| IDE/editor integration | Desktop editor and file operations | No VS Code extension, IDE status/selection hooks, or terminal launch integration |
| GitHub/Slack integrations | Git status/diff and Solana GitHub usage in specific tools | No GitHub app install flow, PR comments command, Slack app install flow, or GitHub Models OAuth/device onboarding |
| Remote workflows | Detached local runtime supervisor | No remote control/bridge, direct-connect, teleport, mobile QR/session URL equivalent |
| Voice/Chrome | Browser sidebar only | No OpenClaude voice mode or Chrome/native-host integration |
| Diagnostics/hardening | Tests and runtime errors exist | No user-facing doctor/runtime diagnostic report, hardening scripts, smoke command, provider-env validator CLI, or privacy/no-phone-home verifier |
| Safety UX | Approval modes and operator actions | Missing OpenClaude's mature permission-rule UI/commands, sandbox command, destructive command warning surface per shell tool, and broader policy controls |
| Cost/usage | Solana cost tooling and runtime metadata | No general LLM cost/usage/stats screens equivalent to OpenClaude's cost, stats, usage, and rate-limit commands |
| Documentation/setup | Strong project README and feature plans | Missing OpenClaude-style beginner quick starts, advanced provider setup guides, LiteLLM guide, local-agent playbook, VS Code extension docs, and runtime doctor docs |

## Highest-Value Missing Work

### Priority 0: Decide The Product Boundary

Cadence needs an explicit decision on whether it should absorb OpenClaude as an embedded engine, expose OpenClaude-compatible workflows, or remain a desktop orchestrator with only selected OpenClaude ideas. Without that decision, some "missing" items are only missing if Cadence is meant to replace OpenClaude.

Recommended decision record:

- Cadence remains desktop-first.
- The owned-agent runtime becomes the in-app native agent engine.
- OpenClaude parity is pursued where it improves agent autonomy, not where it duplicates terminal UI affordances.
- A compatibility layer may still be useful for provider profiles, skills, MCP, and diagnostics.

### Priority 1: Mature The Native Agent Tool Surface - Completed

Status: completed on 2026-04-25. The owned-agent runtime now covers the most important OpenClaude tool categories from this priority. Verification evidence: Rust formatting, focused autonomous-tool runtime tests, owned-agent runtime tests, library tests, full `cargo test`, and diff whitespace checks all passed after the final implementation.

- [x] MCP tool/resource/prompt invocation as native tools.
- [x] Subagent spawning with built-in agent types and per-agent model routing.
- [x] Todo/task tools for model-visible planning state.
- [x] Notebook edit support.
- [x] LSP/code-intelligence tool for symbol lookup and diagnostics, including safe install suggestions when a server is missing.
- [x] PowerShell-specific tool behavior for Windows parity.
- [x] Better tool search/deferred tool loading for large tool surfaces.

### Priority 2: Bring Over Skills And Plugins In A Cadence-Native Way - Completed

Status: completed on 2026-04-26. Cadence now has durable installed skills, local/project/bundled/GitHub/dynamic/plugin/MCP skill sources, model-visible SkillTool discovery and invocation, settings management for skills and plugins, plugin manifests and command loading, explicit trust controls, reload/stale-state hardening, and workflow documentation. Verification evidence is recorded under Phase 6.

Cadence already has an autonomous skill runtime, but it should be connected to the user and model experience:

- Local skill directories.
- Bundled skills.
- Project skills.
- Dynamic skills discovered during work.
- MCP-provided skills.
- Plugin-provided skills.
- A model-visible SkillTool equivalent.
- A settings UI for installed skills/plugins, source trust, and reload.

#### Priority 2 Implementation Plan

Reader and action for this plan: a future implementation agent should be able to claim one slice, complete it without needing extra product context, and leave tests proving the slice works. Each slice should be small enough for one agent turn or one narrow pull request. UI slices must use ShadCN/Radix patterns already present in Cadence, and agents should use unit or e2e tests rather than temporary debug UI.

##### Phase 0: Freeze The Contract Before Adding Surfaces

Outcome: Cadence has one shared vocabulary for skills, skill sources, plugin sources, trust state, install state, and reload behavior.

- [x] Slice 2.0.1: Define the Cadence skill-source taxonomy.
  - Scope: write the internal contract for `bundled`, `local`, `project`, `github`, `dynamic`, `mcp`, and `plugin` sources; define which fields are required for each source type; decide whether source identity is global or project-scoped.
  - Acceptance: the contract distinguishes discoverable, installed, enabled, disabled, stale, failed, and blocked states; it names the trust states Cadence will expose to users; it keeps GitHub-backed autonomous skills compatible with the existing runtime.
  - Verification: focused Rust or TypeScript contract tests validate source ids, duplicate handling, and unsupported state transitions.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_source_contract` passed 5 tests and `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_runtime` passed 7 tests.

- [x] Slice 2.0.2: Define SkillTool semantics.
  - Scope: specify model-visible operations for listing, resolving, installing, invoking, and reloading skills; define what skill markdown and supporting assets are allowed to enter model context; define lifecycle events for success and failure.
  - Acceptance: the contract makes it clear when a model may discover a skill, when user approval is required, and how failures are returned without leaking secrets or raw local paths unnecessarily.
  - Verification: schema tests cover valid and invalid tool inputs, redacted error payloads, and lifecycle event projection.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_tool_contract` passed 6 tests.

##### Phase 1: Make Installed Skills Durable

Outcome: Cadence can remember installed skills from multiple source types, not just cache a GitHub skill for one invocation.

- [x] Slice 2.1.1: Add a durable installed-skill registry.
  - Scope: persist installed skill records with source identity, resolved metadata, cache key or local location, enabled state, trust state, version/hash, timestamps, and last diagnostic.
  - Acceptance: installed skill records survive app restart and can be listed by project and by global scope; duplicate source records converge instead of creating parallel entries.
  - Verification: project-store tests cover create, update, list, disable, re-enable, remove, and corrupt-record rejection.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_durable_registry` passed 4 tests, and full `cargo test --manifest-path client/src-tauri/Cargo.toml` passed.

- [x] Slice 2.1.2: Register GitHub autonomous skills after install/invoke.
  - Scope: connect the existing GitHub autonomous skill runtime to the installed-skill registry so successful install or invoke operations leave a durable record.
  - Acceptance: installing a GitHub-backed skill updates registry state and preserves existing cache/lifecycle behavior.
  - Verification: autonomous skill runtime tests assert registry updates for cache hit, cache miss, refresh, and failed install paths.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_durable_registry` passed cache miss, hit, refresh, invoke, and failed-install registry assertions; `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_runtime` passed 7 tests.

- [x] Slice 2.1.3: Add local and project skill directory scanning.
  - Scope: scan configured local skill directories and project skill directories for valid `SKILL.md` documents and supported assets using the same validation limits as cached autonomous skills.
  - Acceptance: local/project skills appear as discoverable candidates, invalid skills produce typed diagnostics, and scanning never follows paths outside the declared source root.
  - Verification: filesystem fixture tests cover valid skill discovery, missing frontmatter, duplicate ids, oversized files, unsupported assets, and path traversal attempts.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_durable_registry` passed filesystem fixture coverage for valid local/project discovery, missing frontmatter, duplicate ids, oversized files, unsupported assets, and symlink/path escape rejection.

- [x] Slice 2.1.4: Add bundled skill discovery.
  - Scope: define Cadence-owned bundled skill roots and expose bundled skills through the same registry/discovery path as local and GitHub skills.
  - Acceptance: bundled skills can be discovered and invoked without network access; bundled skill metadata includes a Cadence-controlled version/hash.
  - Verification: unit tests run discovery against fixture bundled skills and assert deterministic ordering.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_durable_registry` passed bundled discovery ordering and offline invocation-context loading assertions.

##### Phase 2: Expose Skills To The Model Loop

Outcome: owned agents can see, choose, install, and invoke skills through a Cadence-native SkillTool.

- [x] Slice 2.2.1: Add a model-visible SkillTool descriptor.
  - Scope: add a tool descriptor for skill discovery and resolution with strict input schemas and concise descriptions suitable for model planning.
  - Acceptance: the tool appears in owned-agent tool discovery only when skill support is enabled and reports unavailable states clearly when no sources are configured.
  - Verification: owned-agent tool registry tests cover enabled/disabled availability and tool-search discoverability.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_model_tool` passed SkillTool descriptor/tool-search gating assertions and `cargo test --manifest-path client/src-tauri/Cargo.toml --test agent_core_runtime` passed owned-agent tool registry coverage.

- [x] Slice 2.2.2: Implement SkillTool discover and resolve.
  - Scope: route model requests through the durable registry plus source scanners; return ranked candidates with source type, trust state, enabled state, and short descriptions.
  - Acceptance: discovery merges GitHub, bundled, local, and project skills; disabled or blocked skills are visible only when the request asks for them or when needed for diagnostics.
  - Verification: backend tests cover merged source ordering, query matching, trust filtering, and malformed request rejection.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_model_tool` passed merged GitHub/bundled/local/project discovery, query filtering, trust filtering, disabled-source diagnostics, and stale-source detection; `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_tool_contract` passed malformed request rejection.

- [x] Slice 2.2.3: Implement SkillTool install and invoke.
  - Scope: install missing skills when allowed, load validated skill markdown/assets, and return a model-consumable invocation payload while recording lifecycle events.
  - Acceptance: invocation works for cached GitHub skills, bundled skills, local skills, and project skills; approval is required for untrusted sources before first use; failures leave durable diagnostics.
  - Verification: owned-agent runtime tests cover trusted invocation, approval-required invocation, rejected invocation, stale source refresh, and asset loading.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_model_tool` passed trusted bundled/GitHub invocation, approval-required local invocation, stale bundled refresh, dynamic rejection, and asset-loading assertions; `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_tool_runtime` passed adjacent runtime dispatch coverage.

- [x] Slice 2.2.4: Add dynamic skill discovery during work.
  - Scope: let the model create a discoverable dynamic skill candidate from an approved source or completed run artifact without automatically trusting it.
  - Acceptance: dynamic skills start disabled or untrusted, can be reviewed later in settings, and never become model-invocable until explicitly enabled or trusted by policy.
  - Verification: tests cover dynamic candidate creation, duplicate merging, disabled-by-default behavior, and lifecycle telemetry.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_model_tool` passed dynamic candidate creation, duplicate merge, disabled/untrusted defaults, lifecycle output, and non-invocable behavior.

##### Phase 3: Add User-Facing Skill Management - Completed

Outcome: users can inspect and control skills/plugins from Cadence settings without using hidden commands.

- [x] Slice 2.3.1: Add a Skills settings tab.
  - Scope: build a ShadCN-based settings surface showing installed/discoverable skills, source type, trust state, enabled state, version/hash, last used time, and last diagnostic.
  - Acceptance: users can filter/search skills, enable/disable a skill, remove an installed skill, and inspect the non-secret source metadata for a skill.
  - Verification: React tests cover empty state, populated state, search/filter, enable/disable, remove confirmation, and diagnostic rendering.
  - Completed: 2026-04-25. Verification evidence: `pnpm --dir client test -- settings-dialog.test.tsx agent-runtime.test.tsx use-cadence-desktop-state.test.tsx` passed coverage for Skills registry metadata, search, enable toggles, remove actions, source metadata, and diagnostic rendering.

- [x] Slice 2.3.2: Add source management for local/project/GitHub skill roots.
  - Scope: add controls for adding/removing local skill directories, enabling project skill discovery, and configuring GitHub source repo/ref values.
  - Acceptance: unsafe paths are rejected with clear errors; changing a source triggers a reload request; source settings persist at the correct global or project scope.
  - Verification: state-management tests cover add/remove/update, invalid paths, project-scope persistence, and reload triggering.
  - Completed: 2026-04-25. Verification evidence: `pnpm --dir client test -- settings-dialog.test.tsx agent-runtime.test.tsx use-cadence-desktop-state.test.tsx` passed coverage for invalid local paths, local root add/disable/remove, project skill discovery toggling, GitHub repo/ref/root saves, and reload-triggering state mutations.

- [x] Slice 2.3.3: Surface skill lifecycle in the agent run view.
  - Scope: connect existing skill lifecycle events to a compact user-facing lane that shows discovery, install, invoke, cache status, and diagnostics.
  - Acceptance: successful skill use is visible without overwhelming the transcript, and failed skill use exposes the actionable diagnostic already stored in the runtime state.
  - Verification: React tests cover successful and failed lifecycle rows, replayed events after reconnect, and malformed event handling.
  - Completed: 2026-04-25. Verification evidence: `pnpm --dir client test -- settings-dialog.test.tsx agent-runtime.test.tsx use-cadence-desktop-state.test.tsx` passed coverage for the agent runtime Skill lane, source/cache details, failed invocation diagnostics, empty skill activity state, and stream replay/projection behavior.

##### Phase 4: Add Plugin Sources Without Making Plugins A Second Runtime - Completed

Outcome: plugins can contribute skills and commands through Cadence-controlled manifests, trust checks, and reload mechanics.

- [x] Slice 2.4.1: Define and validate a plugin manifest.
  - Scope: define the minimal plugin manifest for id, name, version, description, trust declaration, contributed skills, contributed commands, and entry locations.
  - Acceptance: manifests are schema-validated, plugin ids are stable, unsupported fields fail closed, and contributed paths must stay inside the plugin root.
  - Verification: parser tests cover valid manifests, missing required fields, duplicate ids, bad versions, unknown capability types, and path traversal.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test plugin_sources` passed manifest validation coverage for valid manifests, missing required fields, duplicate ids, bad versions, unknown fields, and path traversal.

- [x] Slice 2.4.2: Add plugin discovery and installed-plugin registry state.
  - Scope: scan configured plugin roots, persist installed plugin metadata, and track enabled/disabled/trusted/blocked state independently from contributed skills.
  - Acceptance: disabling a plugin disables its contributed skills and commands without deleting their records; removing a plugin marks contributions unavailable instead of leaving dangling invocations.
  - Verification: registry tests cover plugin install, disable, enable, remove, contribution projection, and stale contribution cleanup.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test plugin_sources` passed registry coverage for install, disable, enable, remove, command projection, and stale contribution cleanup; `cargo test --manifest-path client/src-tauri/Cargo.toml --test skill_source_settings` passed plugin root persistence and validation.

- [x] Slice 2.4.3: Project plugin-contributed skills into SkillTool.
  - Scope: expose plugin skills through the same SkillTool discovery/resolve/invoke path as other skills, while preserving plugin provenance and trust state.
  - Acceptance: plugin skills cannot bypass skill validation, approval, asset limits, or disabled-plugin state.
  - Verification: SkillTool tests cover trusted plugin skills, untrusted plugin approval, disabled plugin behavior, and invalid plugin skill assets.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test plugin_sources` passed plugin SkillTool coverage for trusted invocation, untrusted approval, disabled plugin behavior, and invalid asset diagnostics; `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_model_tool` passed adjacent SkillTool regression coverage.

- [x] Slice 2.4.4: Add plugin command loading and reload.
  - Scope: load plugin-contributed commands into the Cadence command/action registry without duplicating slash-command UI from OpenClaude; add explicit reload behavior and diagnostics.
  - Acceptance: commands have stable ids, labels, descriptions, availability rules, and trust provenance; reload updates added/removed commands without restarting the app.
  - Verification: command-registry tests cover load, conflict resolution, disabled plugins, reload success, reload failure, and stale command removal.
  - Completed: 2026-04-25. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test plugin_sources` passed stable command id, duplicate plugin conflict, disabled plugin, remove, and stale command projection coverage; `npm test -- settings-dialog.test.tsx use-cadence-desktop-state.test.tsx` passed explicit plugin reload and state mutation coverage.

- [x] Slice 2.4.5: Add a Plugins settings tab.
  - Scope: build a ShadCN-based settings surface for installed plugins, source roots, trust state, enabled state, contributed skills/commands, reload, and diagnostics.
  - Acceptance: users can enable/disable plugins, reload plugins, inspect contributions, and see why a plugin is blocked.
  - Verification: React tests cover list rendering, details view, enable/disable, reload, blocked state, and contribution counts.
  - Completed: 2026-04-25. Verification evidence: `npm test -- settings-dialog.test.tsx use-cadence-desktop-state.test.tsx` passed 59 tests covering plugin list rendering, metadata/details, contribution counts, enable/disable, remove, source root validation, reload, and blocked diagnostics; `npm run build` passed with the existing Vite large-chunk warning.

##### Phase 5: Add MCP-Provided Skills

Outcome: MCP servers can contribute model-visible skills without weakening the existing MCP registry approval model.

- [x] Slice 2.5.1: Extend MCP projection with skill metadata.
  - Scope: map MCP-provided skill resources/prompts, where available, into Cadence skill candidate records with server provenance and approval state.
  - Acceptance: MCP skills are visible only for approved/connected servers and include enough provenance for users to identify the contributing server.
  - Verification: MCP projection tests cover connected, blocked, stale, and misconfigured servers.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_model_tool` passed 7 tests, including MCP resource/prompt projection from connected servers, blocked/stale/misconfigured server filtering, and server provenance assertions.

- [x] Slice 2.5.2: Invoke MCP-provided skills through SkillTool.
  - Scope: route MCP skill invocation through existing MCP tool/resource/prompt invocation mechanics instead of creating a parallel transport.
  - Acceptance: server approval, authentication failures, and transport failures surface as typed skill diagnostics and do not corrupt installed skill state.
  - Verification: integration-style tests cover successful invocation, auth-required failure, disconnected server failure, and lifecycle event persistence.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_model_tool` passed successful MCP resource and prompt SkillTool invocation, auth/env failure, disconnected-server failure, transport failure, lifecycle event assertions, and installed-skill registry non-mutation assertions; `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_tool_runtime` passed 21 tests covering the shared MCP transport regression surface.

##### Phase 6: Hardening, Docs, And Completion Criteria

Outcome: Priority 2 is safe enough to call complete and hand to normal users.

- [x] Slice 2.6.1: Add source trust and policy hardening.
  - Scope: enforce explicit trust boundaries for local, project, GitHub, MCP, dynamic, and plugin-provided skills; redact local secrets and absolute paths from model-facing outputs where they are not required for execution.
  - Acceptance: untrusted sources cannot become model-invocable silently, blocked sources fail closed, and diagnostics give users enough information to fix configuration safely.
  - Verification: security-focused tests cover trust escalation attempts, disabled source use, secret redaction, and path redaction.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_model_tool` passed 9 tests including candidate/diagnostic redaction, untrusted dynamic non-invocation, and disabled-source visibility; `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_durable_registry` passed 5 tests including blocked source re-enable rejection.

- [x] Slice 2.6.2: Add reload and stale-state hardening.
  - Scope: make reload idempotent across skill and plugin sources; mark stale records when source content changes or disappears; preserve last-known diagnostics for troubleshooting.
  - Acceptance: repeated reloads do not create duplicates, removed sources become unavailable, and changed hashes/versions are reflected in registry state.
  - Verification: registry tests cover repeated reload, content change, deleted source, partial failure, and recovery after failure.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_model_tool` passed reload coverage for changed and deleted filesystem skills with stale diagnostics and no duplicate records; `cargo test --manifest-path client/src-tauri/Cargo.toml --test plugin_sources` passed 4 tests covering plugin reload, stale contribution cleanup, disabled plugins, and invalid plugin skill assets.

- [x] Slice 2.6.3: Document the user and agent workflows.
  - Scope: document how users add skills/plugins, how agents discover and invoke skills, what trust states mean, and how to troubleshoot blocked or failed skills.
  - Acceptance: a fresh engineer can implement a new skill source against the documented contract, and a user can understand why a skill is unavailable.
  - Verification: docs review plus focused tests for any examples or fixtures included with the docs.
  - Completed: 2026-04-26. Verification evidence: added `docs/skills-and-plugins.md` covering user workflows, agent SkillTool operations, trust states, source contracts, plugin contracts, and troubleshooting; the doc uses descriptive contracts only and does not add untested executable examples.

- [x] Slice 2.6.4: Declare Priority 2 complete.
  - Scope: run the focused Rust tests for skill/plugin registry and SkillTool behavior, focused React tests for settings and run-view surfaces, and the existing autonomous skill runtime tests.
  - Acceptance: the report can mark Priority 2 complete only after local/project/bundled/GitHub skills, plugin-provided skills, MCP-provided skills, model-visible invocation, settings management, trust controls, reload, and diagnostics all have passing verification.
  - Verification: record the exact commands and passing results in the completion note, using one Cargo command at a time.
  - Completed: 2026-04-26. Verification evidence:
    - `cargo fmt --manifest-path client/src-tauri/Cargo.toml` passed.
    - `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_model_tool` passed 9 tests.
    - `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_durable_registry` passed 5 tests.
    - `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_tool_contract` passed 6 tests.
    - `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_source_contract` passed 5 tests.
    - `cargo test --manifest-path client/src-tauri/Cargo.toml --test plugin_sources` passed 4 tests.
    - `cargo test --manifest-path client/src-tauri/Cargo.toml --test skill_source_settings` passed 4 tests.
    - `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_skill_runtime` passed 7 tests.
    - `cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_tool_runtime` passed 21 tests.
    - `pnpm --dir client test -- settings-dialog.test.tsx agent-runtime.test.tsx use-cadence-desktop-state.test.tsx` passed 23 test files and 297 tests.
    - `pnpm --dir client build` passed with the existing Vite large-chunk warning.

### Priority 3: Build Runtime Diagnostics And Provider Setup Parity

Cadence already has provider profiles, readiness projections, provider-model catalog refresh, cached/stale/manual catalog states, provider-specific runtime binding, and typed runtime/session diagnostics. Priority 3 should turn those primitives into an OpenClaude-grade troubleshooting loop: a single report that can explain why a provider or runtime is not ready, guide users through repair, and broaden provider setup without weakening Cadence's desktop-first product boundary.

Cadence's provider UI is good, but OpenClaude has stronger startup and troubleshooting loops:

- Provider reachability diagnostics.
- Runtime doctor report with human and JSON modes.
- Saved-profile validation and repair suggestions.
- Provider recommendation for local models.
- LiteLLM/OpenAI-compatible setup guidance.
- GitHub Models device onboarding.
- Mistral, Atomic Chat, NVIDIA NIM, MiniMax, and Foundry presets or documented OpenAI-compatible recipes.

#### Priority 3 Implementation Plan

Reader and action for this plan: a future implementation agent should be able to claim one slice, complete it without needing extra product context, and leave tests proving the slice works. Each slice should build on Cadence's existing provider profiles, provider-model catalogs, runtime session diagnostics, and ShadCN settings surfaces. UI slices must stay user-facing only, and verification should use Rust/TypeScript/unit/e2e tests rather than temporary debug UI.

##### Phase 0: Define One Diagnostics Contract

Outcome: Cadence has one shared vocabulary for provider readiness, reachability, profile repair, runtime health, report severity, redaction, and machine-readable doctor output.

- [x] Slice 3.0.1: Define the provider diagnostic contract.
  - Scope: add typed records for provider checks, severity, retryability, affected profile id, affected provider id, endpoint metadata, remediation text, and redaction class; map existing provider-profile readiness states and provider-model catalog errors into the new contract.
  - Acceptance: missing credentials, malformed profile links, unsupported provider ids, invalid base URLs, stale runtime bindings, catalog transport failures, and ambient-auth failures all normalize to one diagnostic shape without leaking API keys, OAuth session ids, or secret-bearing paths.
  - Verification: Rust and TypeScript schema tests cover valid diagnostics, invalid severity/state combinations, retryable vs non-retryable mapping, and secret redaction.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test provider_diagnostics_contract` passed 3 contract tests; `pnpm --dir client vitest run src/lib/cadence-model/diagnostics.test.ts` passed 3 TypeScript/Zod contract tests; `cargo test --manifest-path client/src-tauri/Cargo.toml` passed the full Rust suite; `pnpm --dir client build` passed with the existing Vite large-chunk warning.

- [x] Slice 3.0.2: Define the doctor report contract.
  - Scope: define a report DTO with report id, generated timestamp, app/runtime version fields, profile checks, model catalog checks, runtime supervisor checks, MCP/settings dependency checks, summary counts, and output modes for compact human text and JSON.
  - Acceptance: a doctor report can be generated without network access, can mark checks as skipped when a dependency is unavailable, and has stable JSON suitable for copying into support or future CLI/headless surfaces.
  - Verification: contract tests cover human/JSON serialization, deterministic ordering, skipped checks, and no secret leakage in nested diagnostics.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test provider_diagnostics_contract` passed stable JSON/human rendering, deterministic counts, skipped checks, and redaction coverage; `pnpm --dir client vitest run src/lib/cadence-model/diagnostics.test.ts` passed frontend report parsing, rendering, sorting, and redaction coverage; `cargo fmt --manifest-path client/src-tauri/Cargo.toml`, `cargo test --manifest-path client/src-tauri/Cargo.toml`, and `pnpm --dir client build` all passed.

##### Phase 1: Build Provider Reachability Checks

Outcome: Cadence can actively test configured provider profiles and explain whether failure is credentials, endpoint shape, network, catalog parsing, local service readiness, or ambient auth.

- [x] Slice 3.1.1: Add a provider profile validation engine.
  - Scope: inspect saved provider-profile metadata and credentials before any network probe; validate active profile, profile/provider/runtime-kind alignment, required base URL/API version/region/project fields, credential-link freshness, local readiness proofs, and ambient readiness proofs.
  - Acceptance: malformed or partially migrated profiles return actionable repair suggestions and do not require model catalog refresh to reveal the problem.
  - Verification: Rust tests cover ready/missing/malformed profiles for OpenAI Codex, OpenRouter, Anthropic, GitHub Models, OpenAI-compatible, Ollama, Azure OpenAI, Gemini AI Studio, Bedrock, and Vertex.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test provider_diagnostics_contract` passed 5 tests, including ready/missing/malformed validation and supported metadata shapes for OpenAI Codex, OpenRouter, Anthropic, GitHub Models, OpenAI-compatible, Ollama, Azure OpenAI, Gemini AI Studio, Bedrock, and Vertex.

- [x] Slice 3.1.2: Add active provider reachability probes.
  - Scope: reuse existing provider-model catalog and auth clients to run explicit reachability probes for the active profile, including OpenAI-compatible `/models`, GitHub Models catalog, Ollama local endpoint, OpenRouter, Anthropic-family providers, Bedrock, and Vertex.
  - Acceptance: probes classify DNS/connect timeout, 401/403, 404 endpoint-shape errors, 429/rate-limit, bad JSON, missing local service, and unsupported catalog strategies with provider-specific recovery text.
  - Verification: backend tests use mocked HTTP/auth clients and local fixture responses for success, timeout, auth failure, rate limit, bad JSON, stale cache fallback, and manual-catalog providers.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test provider_model_catalog_bridge` passed 25 tests, including the new `check_provider_profile` live OpenRouter probe, OpenRouter auth failure, stale-cache rate-limit warning, unreachable Ollama local service, and manual Azure catalog cases.

- [x] Slice 3.1.3: Surface profile repair suggestions in Settings.
  - Scope: extend the existing Providers settings surface with compact diagnostic rows, repair calls to action, and a "Check connection" action that runs validation/probe for one profile.
  - Acceptance: users can see whether the issue is key, endpoint, model catalog, local service, or ambient auth; existing model catalog choices remain visible when Cadence has a stale usable cache.
  - Verification: React tests cover ready, missing key, malformed credential link, invalid base URL, unreachable local Ollama, stale cache with warning, and successful recheck.
  - Completed: 2026-04-26. Verification evidence: `pnpm --dir client exec vitest run src/lib/cadence-model/diagnostics.test.ts components/cadence/settings-dialog.test.tsx` passed 2 test files and 29 tests, including missing-key repair copy, malformed credential-link copy, invalid base URL copy, unreachable Ollama copy, stale-cache warning copy, and successful recheck. Additional verification: `pnpm --dir client exec vitest run src/features/cadence/use-cadence-desktop-state.test.tsx src/App.test.tsx` passed 2 test files and 76 tests; `pnpm --dir client exec tsc --noEmit` passed; targeted `pnpm --dir client exec eslint ...` passed; `cargo fmt --manifest-path client/src-tauri/Cargo.toml -- --check` passed.

##### Phase 2: Add Runtime Doctor Reports

Outcome: Cadence can produce an OpenClaude-style doctor report from the desktop app, with both readable and JSON forms.

- [x] Slice 3.2.1: Implement the backend doctor report command.
  - Scope: add a Tauri command that gathers provider profile validation, provider reachability when requested, runtime session reconciliation, detached supervisor state, provider-model catalog state, MCP registry health, notification route readiness, and important app paths.
  - Acceptance: the command supports a quick local mode and an extended network mode; it reports partial failures without aborting the whole report; JSON output is stable and redacted.
  - Verification: Rust integration tests cover quick mode, extended mode, partial failure aggregation, unavailable app-data files, stale runtime session, and JSON redaction.
  - Completed: 2026-04-26. Implementation: added the `run_doctor_report` Tauri command, stable/redacted request and response contracts, command-surface registration, quick local checks, extended catalog probes, partial-failure aggregation, MCP/runtime/notification/app-path checks, and runtime-session failure projection into report checks. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test doctor_report_command` passed 2 tests covering quick local output, redacted dependencies, and runtime session failure aggregation; `cargo test --manifest-path client/src-tauri/Cargo.toml --test provider_diagnostics_contract` passed 5 tests for shared provider diagnostic contracts; `cargo check --manifest-path client/src-tauri/Cargo.toml` passed; `cargo fmt --manifest-path client/src-tauri/Cargo.toml` passed.

- [x] Slice 3.2.2: Add a diagnostics settings surface.
  - Scope: add a ShadCN-based Diagnostics section or tab that displays doctor summary counts, grouped checks, last run timestamp, copyable JSON, and per-check remediation.
  - Acceptance: users can run quick or extended diagnostics, inspect failures without scrolling through raw logs, and copy the redacted report for support.
  - Verification: React tests cover empty state, running state, passed/warning/failed/skipped groups, JSON copy action, and malformed report handling.
  - Completed: 2026-04-26. Implementation: added the ShadCN-based Diagnostics settings section, quick and extended run actions, grouped report rendering, summary counts, last-run timestamp, status/remediation rows, malformed/error/empty/running states, and redacted JSON copy support through the desktop adapter and Cadence state hook. Verification evidence: `pnpm --dir client exec vitest run src/lib/cadence-model/diagnostics.test.ts components/cadence/settings-dialog.test.tsx components/cadence/agent-runtime.test.tsx` passed 3 test files and 63 tests; `pnpm --dir client exec tsc --noEmit` passed; `pnpm --dir client build` passed.

- [x] Slice 3.2.3: Thread doctor diagnostics into runtime startup failures.
  - Scope: connect runtime start/session failures to the same diagnostic vocabulary so a failed provider bind can offer "run diagnostics" and show the relevant provider/profile check inline.
  - Acceptance: runtime failures for stale binding, missing credentials, provider mismatch, unavailable local endpoint, and ambient auth missing all link back to the same remediation text users see in Settings.
  - Verification: state/view-builder tests cover runtime session failure projection, run-start failure projection, doctor suggestion visibility, and secret-free persisted diagnostics.
  - Completed: 2026-04-26. Implementation: wired runtime startup failures to a Settings Diagnostics entry point, exposed doctor report state through the app shell, and added backend/runtime checks so provider and session failures use the same redacted diagnostic vocabulary as Settings. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test doctor_report_command` passed 2 tests, including runtime session failure projection; `pnpm --dir client exec vitest run src/lib/cadence-model/diagnostics.test.ts components/cadence/settings-dialog.test.tsx components/cadence/agent-runtime.test.tsx` passed 3 test files and 63 tests, including the runtime failure Diagnostics action; `pnpm --dir client exec tsc --noEmit` passed.

##### Phase 3: Add Provider Recommendation And Setup Guides

Outcome: Cadence can recommend a usable profile path for common local/cloud setups and help users configure OpenAI-compatible endpoints without guessing.

- [x] Slice 3.3.1: Add provider recommendation logic.
  - Scope: inspect saved profiles, credential readiness, local Ollama reachability, configured OpenAI-compatible endpoints, and ambient Bedrock/Vertex readiness to recommend a default profile path.
  - Acceptance: recommendations distinguish fastest-ready profile, best local profile, missing-key cloud profile, and unsupported/incomplete profile; recommendations never activate or mutate profiles without user action.
  - Verification: pure tests cover no profiles, OpenAI Codex ready, OpenRouter ready, Ollama reachable, local OpenAI-compatible endpoint, Bedrock/Vertex ambient ready, and multiple competing ready profiles.
  - Completed: 2026-04-26. Implementation: added typed provider recommendation contracts and pure ranking logic for ready cloud profiles, local profiles, missing-key cloud setup, and incomplete repair paths. Verification evidence: `pnpm --dir client exec vitest run src/lib/cadence-model/provider-setup.test.ts components/cadence/settings-dialog.test.tsx` passed 2 test files and 35 tests, including no-profile, OpenAI Codex, OpenRouter, Ollama, local OpenAI-compatible, Bedrock/Vertex, and competing-ready-profile recommendation cases.

- [x] Slice 3.3.2: Add OpenAI-compatible and LiteLLM setup recipes.
  - Scope: add structured recipe metadata for common OpenAI-compatible setups, including LiteLLM, LM Studio, Groq, Together, DeepSeek, and custom `/v1` gateways; map each recipe to provider profile fields, auth mode, model catalog expectations, and repair suggestions.
  - Acceptance: users can choose a recipe that pre-fills label/base URL/API-key expectations while still saving through the existing provider-profile contract.
  - Verification: TypeScript tests cover recipe validation, generated upsert requests, required-field prompts, local endpoint no-key behavior, and catalog/manual-model expectations.
  - Completed: 2026-04-26. Implementation: added structured recipes for LiteLLM, LM Studio, Groq, Together AI, DeepSeek, and custom `/v1` gateways with auth/key mode, model-catalog expectations, profile defaults, guidance, repair copy, required-field prompts, and generated upsert requests. Verification evidence: `pnpm --dir client exec vitest run src/lib/cadence-model/provider-setup.test.ts components/cadence/settings-dialog.test.tsx` passed recipe validation, generated request, missing-field, optional key, and local no-key assertions.

- [x] Slice 3.3.3: Present setup guidance in Providers Settings.
  - Scope: add a ShadCN recipe picker and compact guidance inside the existing Providers section, without replacing the current provider cards.
  - Acceptance: recipes make the setup path clearer for LiteLLM/OpenAI-compatible/local gateways, and they feed into the same save/check connection flow as first-class presets.
  - Verification: React tests cover recipe selection, prefilled fields, required key/base URL messaging, save flow, check connection handoff, and no temporary debug controls.
  - Completed: 2026-04-26. Implementation: added a ShadCN/Radix recipe picker, a compact recommendation panel, recipe guidance inside the existing provider editor, hosted API-key validation, and local OpenAI-compatible no-key save behavior that still uses the existing provider-profile save and connection-check path. Verification evidence: `pnpm --dir client exec vitest run src/lib/cadence-model/provider-setup.test.ts components/cadence/settings-dialog.test.tsx` passed 35 tests; `pnpm --dir client exec tsc --noEmit` passed; `pnpm --dir client exec eslint src/lib/cadence-model/provider-setup.ts src/lib/cadence-model/provider-setup.test.ts components/cadence/provider-profiles/provider-profile-form.tsx components/cadence/settings-dialog.test.tsx` passed; `pnpm --dir client build` passed with the existing Vite large-chunk warning.

##### Phase 4: Fill Missing Provider Preset Parity

Outcome: Cadence covers OpenClaude's missing provider setup surface either as first-class presets or as documented OpenAI-compatible recipes.

- [x] Slice 3.4.1: Add direct Mistral provider support or a first-class Mistral recipe.
  - Scope: decide whether Mistral should be a dedicated provider id or a locked OpenAI-compatible recipe; implement the selected path through provider presets, runtime provider identity, endpoint resolution, model catalog behavior, and settings UI.
  - Acceptance: users can save a Mistral-backed profile, validate it, refresh or manually specify models according to the chosen transport, and launch a runtime without secret leakage.
  - Verification: Rust and React tests cover profile save, endpoint resolution, catalog behavior, runtime binding, stale binding, settings rendering, and diagnostics.
  - Completed: 2026-04-26. Implementation: added a first-class Mistral OpenAI-compatible recipe using the existing `openai_api` provider profile path, preserving provider-profile validation, model catalog probing/manual fallback, runtime launch binding, stale binding checks, and redacted diagnostics through the shared OpenAI-compatible runtime. Verification evidence: `pnpm --dir client exec vitest run src/lib/cadence-model/provider-setup.test.ts components/cadence/settings-dialog.test.tsx` passed 2 files and 39 tests; `cargo test --manifest-path client/src-tauri/Cargo.toml --test provider_diagnostics_contract` passed 5 tests; `cargo test --manifest-path client/src-tauri/Cargo.toml --test runtime_session_bridge` passed 41 tests.

- [x] Slice 3.4.2: Add NVIDIA NIM, MiniMax, and Foundry recipes.
  - Scope: add recipe metadata and validation for NVIDIA NIM, MiniMax, and Foundry-compatible endpoints using the existing OpenAI-compatible profile path unless a direct provider is required.
  - Acceptance: each recipe has clear required fields, default endpoint guidance, catalog/manual-model behavior, and provider-specific repair text.
  - Verification: TypeScript recipe tests and provider-profile form tests cover generated requests, invalid/missing fields, catalog expectations, and repair text.
  - Completed: 2026-04-26. Implementation: added NVIDIA NIM, MiniMax, and Azure AI Foundry setup recipes with provider-specific labels, default endpoint guidance, required fields, catalog expectations, and repair copy. Azure AI Foundry stays on the OpenAI-compatible endpoint route and explicitly points deployment-level `api-version` users to the existing Azure OpenAI preset. Verification evidence: `pnpm --dir client exec vitest run src/lib/cadence-model/provider-setup.test.ts components/cadence/settings-dialog.test.tsx` passed recipe validation, generated request, required-field, settings rendering, and repair-copy coverage; `pnpm --dir client exec eslint src/lib/cadence-model/provider-setup.ts src/lib/cadence-model/provider-setup.test.ts components/cadence/provider-profiles/provider-profile-form.tsx components/cadence/settings-dialog.test.tsx` passed.

- [x] Slice 3.4.3: Add Atomic Chat local setup support.
  - Scope: add a local recipe or provider preset for Atomic Chat with no fake key requirement, local endpoint reachability, and manual-model fallback when catalog discovery is unavailable.
  - Acceptance: users can configure Atomic Chat as a local model backend, see local readiness state, and run connection checks without storing placeholder secrets.
  - Verification: Rust tests cover local readiness and endpoint resolution; React tests cover setup, missing local service, manual model fallback, and runtime launch handoff.
  - Completed: 2026-04-26. Implementation: added an Atomic Chat local recipe that saves through `openai_api` with local auth, no placeholder key, editable local endpoint, manual model entry, and shared connection checks. The provider form now enforces recipe-level required base URL/model fields before saving. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test provider_model_catalog_bridge` passed 26 tests including Atomic Chat local no-key reachability; `pnpm --dir client exec vitest run src/lib/cadence-model/provider-setup.test.ts components/cadence/settings-dialog.test.tsx` passed Atomic Chat local setup/no-secret assertions.

- [x] Slice 3.4.4: Add GitHub Models device onboarding only if it fits Cadence auth.
  - Scope: evaluate and implement a GitHub Models device-flow path only if it can share Cadence's existing provider-profile credential store and redaction rules; otherwise document API-token setup as the supported path and keep device onboarding out of scope.
  - Acceptance: the chosen path is explicit in the report/docs; if implemented, device onboarding saves a redacted app-local token link and reuses profile readiness/catalog diagnostics.
  - Verification: auth-flow tests cover successful device flow or documented non-support, cancellation, stale flow rejection, token redaction, profile readiness, and catalog discovery.
  - Completed: 2026-04-26. Decision: GitHub Models device onboarding stays out of scope for this phase because it needs a dedicated auth flow, cancellation/stale-flow handling, app-local token-link storage, and redaction coverage. Cadence supports GitHub Models through saved app-local tokens, existing profile readiness, catalog diagnostics, runtime binding, and stale-binding checks. Documentation now records the token-based path and non-support decision. Verification evidence: `docs/provider-setup-and-diagnostics.md` documents the supported path; `cargo test --manifest-path client/src-tauri/Cargo.toml --test runtime_session_bridge` passed GitHub Models token binding and stale-token coverage; `pnpm --dir client build` passed.

##### Phase 5: Documentation And Completion Criteria

Outcome: Priority 3 is safe enough to call complete and useful for both users and implementation agents.

- [x] Slice 3.5.1: Document provider setup and diagnostics workflows.
  - Scope: document how users configure each supported provider path, how recipe-based OpenAI-compatible setup works, what each diagnostic state means, and when to use quick vs extended doctor reports.
  - Acceptance: a fresh user can set up a common cloud provider, a local provider, and a custom OpenAI-compatible endpoint without reading source code; a support engineer can interpret a redacted doctor JSON report.
  - Verification: docs review plus tests for any executable examples or fixture-backed recipes included in the docs.
  - Completed: 2026-04-26. Implementation: expanded `docs/provider-setup-and-diagnostics.md` with direct provider setup paths, OpenAI-compatible recipe behavior, GitHub Models token onboarding, quick vs extended diagnostics, diagnostic state meanings, support triage order, and the doctor JSON privacy contract. Verification evidence: docs review confirmed the file uses descriptive workflow guidance only and adds no executable examples or fixtures requiring separate test coverage.

- [x] Slice 3.5.2: Add privacy and no-secret hardening for diagnostics.
  - Scope: audit doctor output, provider diagnostics, copied JSON, runtime failure text, and model-facing diagnostics for leaked API keys, OAuth tokens, local secret file contents, and unnecessary absolute paths.
  - Acceptance: diagnostics include enough non-secret metadata to repair the issue while excluding raw secrets and secret-bearing paths from persisted, copied, and model-visible surfaces.
  - Verification: Rust and TypeScript tests cover redaction of API keys, bearer headers, OAuth/session ids, ADC/AWS path content, local endpoint credentials, and nested diagnostic payloads.
  - Completed: 2026-04-26. Implementation: hardened the shared Rust and TypeScript diagnostic redaction paths for opaque bearer headers, compact authorization headers, cloud credential path assignments, AWS/API-key/session-token names, local endpoint URL credentials, and nested doctor-report checks constructed outside the normal diagnostic factory before copied JSON or human output is rendered. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test provider_diagnostics_contract` passed 6 tests including the new redaction coverage; `pnpm --dir client exec vitest run src/lib/cadence-model/diagnostics.test.ts` passed 1 test file and 5 tests including copied JSON redaction.

- [x] Slice 3.5.3: Declare Priority 3 complete.
  - Scope: run focused Rust tests for provider profiles, provider model catalogs, runtime session/supervisor diagnostics, and doctor report generation; run focused React tests for Providers and Diagnostics settings surfaces; run build/type checks.
  - Acceptance: the report can mark Priority 3 complete only after provider reachability, doctor reports, profile repair, recommendations, setup recipes/presets, docs, and privacy hardening all have passing verification.
  - Verification: record the exact commands and passing results in the completion note, using one Cargo command at a time.
  - Completed: 2026-04-26. Verification evidence:
    - `cargo fmt --manifest-path client/src-tauri/Cargo.toml` passed.
    - `cargo test --manifest-path client/src-tauri/Cargo.toml --test provider_diagnostics_contract` passed 6 tests.
    - `cargo test --manifest-path client/src-tauri/Cargo.toml --test provider_model_catalog_bridge` passed 26 tests.
    - `cargo test --manifest-path client/src-tauri/Cargo.toml --test doctor_report_command` passed 2 tests.
    - `cargo test --manifest-path client/src-tauri/Cargo.toml --test runtime_session_bridge` passed 41 tests.
    - `pnpm --dir client exec vitest run src/lib/cadence-model/diagnostics.test.ts src/lib/cadence-model/provider-setup.test.ts components/cadence/settings-dialog.test.tsx components/cadence/agent-runtime.test.tsx` passed 4 test files and 77 tests covering provider, diagnostics, settings, and runtime UI behavior.
    - `pnpm --dir client exec eslint src/lib/cadence-model/diagnostics.ts src/lib/cadence-model/diagnostics.test.ts components/cadence/settings-dialog/diagnostics-section.tsx components/cadence/provider-profiles/provider-profile-form.tsx` passed.
    - `pnpm --dir client exec tsc --noEmit` passed.
    - `pnpm --dir client build` passed with the existing Vite large-chunk warning.

### Priority 4: Add Session Memory And Context Management

Cadence persists runs well, but OpenClaude has richer working-memory features:

- Context visualization.
- Auto-compact and manual compact.
- Session export and transcript search.
- Memory extraction/consolidation.
- Project instruction/memory file management.
- Cross-session resume search and rename/branch/rewind equivalents.

Current code footing: Cadence already has durable `agent_sessions`, `agent_runs`, `agent_messages`, `agent_events`, `agent_tool_calls`, `agent_file_changes`, `agent_checkpoints`, `agent_action_requests`, and `agent_usage` records. The owned-agent loop rebuilds provider replay state from the full persisted message history, while the system prompt currently reads only `AGENTS.md` plus tool descriptors. The desktop UI already exposes active/archived sessions and a run-scoped feed, but search, export, compaction, memory review, and branch/rewind workflows are not yet first-class.

#### Priority 4 Implementation Plan

Reader and action for this plan: a future implementation agent should be able to claim one slice, complete it without needing extra product context, and leave tests proving the slice works. Each slice should stay aligned with the existing Tauri command contracts, repo-local SQLite project store, owned-agent provider loop, ShadCN/Radix UI patterns, and the no-temporary-debug-UI rule.

##### Phase 0: Define The Session Context Contract

Outcome: Cadence has one vocabulary for transcripts, context budgets, compaction records, memory candidates, exports, and branch/rewind lineage before storage or UI surfaces diverge.

- [x] Slice 4.0.1: Define transcript and context snapshot contracts.
  - Scope: specify stable DTOs for run transcript items, session transcript items, context snapshots, context contributors, usage totals, export payloads, and search result snippets across owned-agent runs and supervised runtime stream items.
  - Acceptance: the contract maps existing `agent_messages`, `agent_events`, `agent_tool_calls`, `agent_file_changes`, `agent_checkpoints`, `agent_usage`, and runtime-stream transcript/tool/activity records without losing ordering, run id, session id, provider, model, or redaction metadata.
  - Verification: Rust and TypeScript schema tests cover valid payloads, malformed payload rejection, stable ordering, empty sessions, archived sessions, and secret-free serialization.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_context_contract` passed 7 tests covering owned-agent transcript projection, runtime-stream transcript projection, stable ordering, malformed payload rejection, archived empty sessions, context snapshots, search/export-adjacent DTOs, usage totals, and secret-free serialization; `pnpm --dir client exec vitest run src/lib/cadence-model/session-context.test.ts` passed 4 TypeScript schema tests.

- [x] Slice 4.0.2: Define compaction and memory policy semantics.
  - Scope: decide how manual compact, auto-compact, memory extraction, approved memory injection, disabled memory, and project instruction files interact with the provider replay path.
  - Acceptance: compaction is non-destructive, raw transcript rows remain searchable/exportable, approved memory is injected deterministically, unapproved memory is never model-visible, and policy decisions can be explained in the UI.
  - Verification: pure Rust tests cover threshold decisions, manual-vs-auto policy results, disabled/unapproved memory filtering, deterministic contributor ordering, and redaction of memory diagnostics.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_context_contract` passed 7 tests including manual-vs-auto compaction policy, threshold decisions, disabled auto-compact handling, approved-only memory contributor filtering, deterministic memory ordering, model-visible contributor integrity, and redaction of secret-bearing memory text.

##### Phase 1: Make Session History Searchable And Exportable

Outcome: users can find, inspect, and export prior work without reopening raw database state or relying on the live feed.

- [x] Slice 4.1.1: Add a redacted transcript projection command.
  - Scope: add backend commands that project one run or one agent session into a chronological transcript containing user/assistant messages, reasoning summaries, tool summaries, file changes, checkpoints, action requests, and usage totals.
  - Acceptance: projections work for active and archived sessions, preserve event/message order, summarize large tool payloads safely, and never expose secrets beyond what the current transcript already permits.
  - Verification: Rust project-store tests cover multi-run sessions, tool-call ordering, checkpoint/file-change ordering, archived-session access, malformed JSON recovery, large payload summarization, and redaction.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_history_commands` passed 2 tests covering active/archived/deleted sessions, run-scoped mismatch rejection, redaction, large tool payload summaries, ordering, usage totals, and search cleanup; `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_context_contract` passed 7 contract tests.

- [x] Slice 4.1.2: Add transcript export in Markdown and JSON.
  - Scope: expose export commands and adapter methods for selected run/session exports in readable Markdown and structured JSON, including enough metadata for support/debugging but excluding raw secret-bearing values.
  - Acceptance: users can export the selected session or a specific run from the agent UI; exported JSON round-trips through the schema; Markdown is readable and includes run boundaries, prompts, assistant responses, tool summaries, checkpoints, and file changes.
  - Verification: Rust serialization tests and React tests cover run export, session export, archived-session export, copy/save action states, failed export diagnostics, and redacted payloads.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_history_commands` passed Markdown, JSON, save, run-scope, archived-session, and redacted-payload assertions; `pnpm --dir client exec vitest run src/lib/cadence-model/session-context.test.ts components/cadence/agent-sessions-sidebar.test.tsx components/cadence/agent-runtime.test.tsx` passed 43 tests including schema round-trip and copy/save UI actions.

- [x] Slice 4.1.3: Add session and transcript search.
  - Scope: add SQLite-backed search over session titles/summaries, prompts, assistant messages, tool summaries, file changes, and checkpoints, with project/session/run scopes and an archived-session toggle.
  - Acceptance: users can search across sessions, jump to the matching run/session, see safe snippets, and distinguish active vs archived results.
  - Verification: migration/store tests cover FTS or equivalent indexing, ranking, snippets, archived filtering, deleted-session cleanup, and redaction; React tests cover query input, empty state, result navigation, loading, and error states.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_history_commands` passed SQLite FTS-backed search, fallback-safe snippets, archived filtering, deleted-session cleanup, scope, and redaction assertions; the focused Vitest command above passed sidebar search input, loading/debounce, result navigation, and schema tests.

- [x] Slice 4.1.4: Surface rename and run navigation.
  - Scope: connect the existing rename mutation to ShadCN session actions and add a compact run history view for the selected session.
  - Acceptance: users can rename a session, inspect its prior runs, reopen a historical run transcript, and start a follow-up from the correct selected session.
  - Verification: React tests cover rename validation, rename failure recovery, run list ordering, selected-run navigation, and no temporary debug controls.
  - Completed: 2026-04-26. Verification evidence: the focused Vitest command above passed rename validation, rename failure recovery, search-result run navigation, selected-run export, and history run switching tests; `pnpm --dir client exec tsc --noEmit` exited 0; `pnpm --dir client build` completed successfully with only the existing Vite large-chunk warnings.

##### Phase 2: Add Context Visualization And Budget Awareness

Outcome: users can see what Cadence will send to the model and when a conversation is approaching context pressure.

- [x] Slice 4.2.1: Compute context contributors and usage rollups.
  - Scope: derive context contributors from the active system prompt, `AGENTS.md`, selected tool descriptors, approved memory, conversation tail, compacted summaries, tool results, and provider usage records.
  - Acceptance: Cadence can report per-run and per-session token/character estimates, known provider usage totals, largest contributors, and whether estimates came from provider data or local approximation.
  - Verification: Rust tests cover empty runs, long transcripts, tool-heavy runs, existing `agent_usage` records, missing usage records, and deterministic contributor ordering.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_history_commands` passed 3 tests covering run-scoped context snapshots, no-run session snapshots, usage rollups, tool descriptors, tool results, instruction files, provider-budget classification, redaction, and session mismatch rejection; `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_context_contract` passed 8 tests including provider budget family coverage and context snapshot invariants. Approved-memory and compaction-summary contributor kinds are contract/UI-ready; their persisted stores are still later Priority 4 phases.

- [x] Slice 4.2.2: Add a context visualization panel.
  - Scope: build a ShadCN-based panel in the agent experience that shows context contributors, approximate budget pressure, compacted vs raw history, approved memory, instruction-file sources, and the next-turn replay shape.
  - Acceptance: users can understand what will enter the next provider call without reading logs, and the panel remains useful for sessions with no runs, short runs, and long runs.
  - Verification: React tests cover no-run state, normal context, over-budget warning, compacted summary display, memory contributors, instruction-file contributors, and responsive layout.
  - Completed: 2026-04-26. Verification evidence: `pnpm --dir client exec vitest run src/lib/cadence-model/session-context.test.ts components/cadence/agent-runtime.test.tsx` passed 41 tests covering the context snapshot schema, adapter request shape, context panel load/rendering, budget pressure badges, instruction-file/tool contributors, usage display, and pending-prompt refresh; `pnpm --dir client exec tsc --noEmit` exited 0; targeted ESLint over touched frontend files exited 0.

- [x] Slice 4.2.3: Warn before over-budget continuations.
  - Scope: integrate context pressure checks into `send_agent_message`/owned-agent continuation preparation and surface actionable warnings in the agent UI before a continuation is likely to exceed the provider context budget.
  - Acceptance: warnings do not block short sessions, do not mutate history, point users to manual compact when available, and classify unknown-provider-budget cases separately from known over-budget cases.
  - Verification: Rust continuation tests cover below-threshold, near-threshold, over-threshold, unknown-budget, and already-compacted sessions; React tests cover warning rendering and continuation retry after compaction.
  - Completed: 2026-04-26. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test agent_core_runtime` passed 28 tests, including an over-budget continuation test that verifies the guard returns `agent_context_budget_exceeded` before appending a message or changing run status; the focused Vitest command above passed over-budget warning rendering and pending-prompt request assertions. Unknown provider budgets remain non-blocking and render separately in the context panel.

##### Phase 3: Add Manual Compact And Compaction-Aware Replay

Outcome: long sessions can continue with a compacted context while raw history stays durable, searchable, and exportable.

- [x] Slice 4.3.1: Persist compaction records.
  - Scope: add project-store records for compaction summaries, covered message/event ranges, source hashes, provider/model metadata, token estimates, policy reason, active/inactive state, and diagnostics.
  - Acceptance: compaction records are append-only by default, can be superseded without deleting raw rows, and can be loaded by project/session/run scope.
  - Verification: migration/store tests cover insert, load active, supersede, range validation, source-hash mismatch, archived sessions, and invalid JSON rejection.
  - Completed: 2026-04-26. Implementation: added repo-local `agent_compactions` storage with active/superseded records, coverage ranges, provider/model metadata, source hashes, token estimates, policy reason, trigger kind, diagnostics, and project-store load/list/supersede helpers. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_history_commands` passed 4 tests including manual compact persistence, supersession, active-record loading, redaction, raw transcript preservation, and context-summary projection.

- [x] Slice 4.3.2: Implement manual compact generation.
  - Scope: add a backend command that compacts selected session history through the active provider adapter or a fake test adapter, records the summary, and preserves a recent raw tail for replay.
  - Acceptance: manual compact handles multi-turn tool conversations, does not summarize pending action requests as completed work, emits typed diagnostics on provider failure, and leaves raw transcript export unchanged.
  - Verification: owned-agent runtime tests cover successful compact, provider failure, cancellation, pending action requests, tool-call-heavy transcripts, redaction, and raw transcript preservation.
  - Completed: 2026-04-26. Implementation: added the `compact_session_history` Tauri command, provider adapter compaction hook, fake-provider deterministic summaries, raw-tail preservation, pending-action wording, summary redaction rejection, and secret-bearing file path redaction in compaction transcripts. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_history_commands` passed 4 tests; `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_context_contract` passed 8 tests.

- [x] Slice 4.3.3: Make provider replay compaction-aware.
  - Scope: update `provider_messages_from_snapshot` and continuation assembly so the next provider turn uses active compaction summaries plus a recent raw tail instead of the full message history when compaction is active.
  - Acceptance: replay never splits assistant/tool-call pairs incorrectly, tool result supersession still works, plan-mode behavior remains intact, and users can continue a compacted run successfully.
  - Verification: agent-core tests cover compacted replay, assistant/tool-call pairing, superseded tool messages, follow-up user messages, plan-mode action requests, and provider mismatch errors.
  - Completed: 2026-04-26. Implementation: made owned-agent provider replay prepend active compaction summaries, skip covered raw messages, preserve raw tail/tool-result pairing, reject provider/model mismatches, and recompute a covered-source hash before continuation. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test agent_core_runtime` passed 30 tests including compacted replay continuation and covered-source mismatch rejection.

- [x] Slice 4.3.4: Add manual compact UI.
  - Scope: add a user-facing compact action to the context panel or session actions with progress, success, failure, and compacted-history display states.
  - Acceptance: users can manually compact the selected session, see what range was compacted, continue after compaction, and inspect any diagnostic without temporary debug UI.
  - Verification: React tests cover action availability, confirmation/progress, success rendering, provider-failure diagnostics, disabled states for no-run sessions, and continuation after compact.
  - Completed: 2026-04-26. Implementation: added a ShadCN compact action to the Context panel with loading, disabled, success, failure, compacted-range copy, and compacted replay display states wired through the desktop adapter. Verification evidence: `pnpm --dir client exec vitest run src/lib/cadence-model/session-context.test.ts components/cadence/agent-runtime.test.tsx` passed 2 files and 45 tests; `pnpm --dir client exec tsc --noEmit` passed; targeted `pnpm --dir client exec eslint ...` passed; `pnpm --dir client build` passed with the existing Vite large-chunk warning.

##### Phase 4: Add Memory Extraction And Project Instruction Management

Outcome: useful durable context can survive individual runs, but users remain in control of what becomes model-visible memory.

- [x] Slice 4.4.1: Add a reviewed memory store.
  - Scope: persist memory candidates and approved memories with project/session scope, kind, text, source run/message references, confidence, review state, enabled state, timestamps, and diagnostics.
  - Acceptance: unreviewed candidates are not injected into model context, approved memories are queryable by scope, disabled memories remain visible to users but unavailable to replay, and deleted source sessions do not leave broken references.
  - Verification: migration/store tests cover candidate creation, approve, disable, re-enable, delete, source-reference cleanup, project vs session scope, and redacted diagnostics.
  - Completed: 2026-04-26. Implementation: added the `agent_memories` store with project/session scoping, review/enabled gating, source provenance, diagnostics, duplicate text hashing, and source-run/session cleanup triggers. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_history_commands` passed 5 tests, including candidate create/approve/disable/re-enable/delete and deleted-source cleanup.

- [x] Slice 4.4.2: Extract memory candidates from completed runs.
  - Scope: add a command or post-run job that proposes memories from completed transcripts, file changes, user preferences, project decisions, and durable troubleshooting facts without auto-approving them.
  - Acceptance: candidates cite their source run/session, avoid duplicating existing approved memory, and distinguish project facts from session summaries and user preferences.
  - Verification: fake-provider tests cover candidate extraction, duplicate merging, low-confidence rejection, source citation, no-auto-approval behavior, and secret redaction.
  - Completed: 2026-04-26. Implementation: added `extract_session_memory_candidates` with provider-backed extraction, deterministic fake-provider fixtures, confidence/secret rejection, duplicate skipping, source item citation, and candidate-only persistence. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_history_commands` passed 5 tests; `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_context_contract` passed 8 tests.

- [x] Slice 4.4.3: Add memory review UI.
  - Scope: build a ShadCN-based Memory surface in Settings or the agent context panel for reviewing, approving, disabling, deleting, filtering, and inspecting memory provenance.
  - Acceptance: users can understand and control every model-visible memory item, and memory diagnostics are actionable without exposing secret-bearing paths or raw credentials.
  - Verification: React tests cover empty state, candidate list, approve/disable/delete, filtering by scope/kind, provenance display, failed action diagnostics, and responsive layout.
  - Completed: 2026-04-26. Implementation: added a ShadCN Memory section to the agent Context panel with load/refresh/extract, approve/reject, enable/disable, delete, scope/kind filters, provenance, confidence, diagnostics, and context refresh after model-visible changes. Verification evidence: `pnpm --dir client exec vitest run src/lib/cadence-model/session-context.test.ts components/cadence/agent-runtime.test.tsx` passed 2 files and 46 tests; targeted `pnpm --dir client exec eslint ...` passed.

- [x] Slice 4.4.4: Inject approved memory and instruction files into the system prompt.
  - Scope: extend `assemble_system_prompt` to include deterministic, redacted approved memory plus supported project instruction files such as `AGENTS.md` and any Cadence-supported memory file contract.
  - Acceptance: injected memory appears in the context visualization panel, ordering is stable, disabled/unreviewed memory is excluded, and missing or malformed instruction files produce diagnostics rather than provider-call failure.
  - Verification: Rust tests cover approved memory injection, disabled candidate exclusion, project vs session scope ordering, instruction-file detection, malformed file diagnostics, and unchanged behavior when no memory exists.
  - Completed: 2026-04-26. Implementation: assembled owned-agent prompts now include supported project instructions (`AGENTS.md`) and deterministically ordered, redacted approved memory only; context snapshots show approved-memory contributors and explain inject/exclude policy decisions while excluding disabled or unreviewed memory. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test agent_core_runtime` passed 30 tests, including redacted approved-memory system prompt coverage; `pnpm --dir client exec tsc --noEmit` passed.

##### Phase 5: Add Branch, Rewind, And Auto-Compact Workflows

Outcome: users can recover and explore from prior conversation points without destructive transcript edits, and long-running sessions can compact themselves when policy allows.

- [x] Slice 4.5.1: Branch a session from a historical run.
  - Scope: add branch lineage records and commands that create a new active session from a selected source session/run, carrying the relevant compacted context and transcript references without mutating the source session.
  - Acceptance: users can branch from a search result or run history row, the new session is selected, source lineage is visible, and continuing the branch does not append to the original session.
  - Verification: store tests cover branch creation, lineage loading, archived source sessions, duplicate titles, and source deletion behavior; React tests cover branch action, selected-session switch, and lineage display.
  - Completed: 2026-04-26. Implementation: added durable branch lineage records, branch commands, branch replay runs, source-deletion diagnostics, App-level branch selection, run-history branch controls, and lineage display in the Agent history panel. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_history_commands` passed 7 tests; `pnpm --dir client exec vitest run src/lib/cadence-model/runtime.test.ts src/lib/cadence-model/agent.test.ts src/lib/cadence-model/session-context.test.ts components/cadence/agent-runtime.test.tsx components/cadence/agent-sessions-sidebar.test.tsx` passed 5 files and 62 tests.

- [x] Slice 4.5.2: Rewind by branching from a checkpoint or message boundary.
  - Scope: add a safe rewind workflow that creates a branch from a selected checkpoint/message boundary and replays only the selected context prefix plus any active compaction summary.
  - Acceptance: rewind never deletes the original run, clearly identifies file rollback limitations, and preserves enough checkpoint metadata to explain what changed before the branch point.
  - Verification: Rust tests cover message-boundary rewind, checkpoint-boundary rewind, tool-call pair boundaries, file-change checkpoint metadata, invalid boundary rejection, and branch continuation.
  - Completed: 2026-04-26. Implementation: added message/checkpoint rewind commands, prefix replay copying for messages/events/tool calls/file changes/checkpoints/action requests, file rollback limitation summaries, App-level rewind selection, and transcript-row rewind controls. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_history_commands` passed 7 tests, including message/checkpoint rewind and invalid boundary rejection; targeted React/schema Vitest passed 62 tests.

- [x] Slice 4.5.3: Add auto-compact policy after manual compact is stable.
  - Scope: add opt-in auto-compact behavior that triggers when context pressure crosses configured thresholds before a continuation, reusing the manual compact pipeline and producing the same diagnostics.
  - Acceptance: auto-compact can be disabled, never runs without a provider/profile capable of summarization, preserves raw history, and reports whether compaction happened before the next provider turn.
  - Verification: policy tests cover disabled, enabled below-threshold, enabled above-threshold, provider unavailable, provider failure, cancellation, and successful continuation after auto-compact.
  - Completed: 2026-04-26. Implementation: added opt-in auto-compact preferences to owned-agent continuations and runtime-run controls, reused the manual compaction pipeline before provider continuation, emitted durable auto-compact diagnostics, preserved raw transcript history, and exposed a ShadCN composer switch. Verification evidence: `cargo test --manifest-path client/src-tauri/Cargo.toml --test agent_core_runtime` passed 32 tests, including successful auto-compact-before-continuation and provider-failure no-mutation coverage; `cargo test --manifest-path client/src-tauri/Cargo.toml --test session_context_contract` passed 8 tests covering policy disabled/below-threshold/above-threshold/provider-unavailable branches; `pnpm --dir client exec tsc --noEmit`, targeted ESLint, and `pnpm --dir client build` passed.

##### Phase 6: Hardening, Docs, And Completion Criteria

Outcome: Priority 4 is safe enough to call complete and useful for both users and future implementation agents.

- [ ] Slice 4.6.1: Add privacy and integrity hardening for session context.
  - Scope: audit transcript projections, exports, search snippets, context visualization, compaction summaries, memory candidates, approved memories, and branch/rewind metadata for secret leakage and source-integrity issues.
  - Acceptance: copied/exported/model-visible surfaces redact API keys, OAuth tokens, bearer headers, credential paths, and secret-bearing tool results while preserving enough context to be useful.
  - Verification: Rust and TypeScript tests cover nested JSON redaction, large tool results, prompt-injection-shaped memory text, secret-bearing paths, exported Markdown, exported JSON, search snippets, and compaction summaries.

- [ ] Slice 4.6.2: Document session memory and context workflows.
  - Scope: document search, export, context visualization, manual compact, auto-compact, memory review, instruction files, branch, rewind, and privacy guarantees.
  - Acceptance: a fresh user can understand when to compact, what memory becomes model-visible, how to export a session, and how branch/rewind differ from destructive history edits.
  - Verification: docs review confirms the workflow is user-facing, accurate to the implemented commands/UI, and contains no executable examples requiring separate test coverage.

- [ ] Slice 4.6.3: Declare Priority 4 complete.
  - Scope: run focused Rust tests for session projection/search/export, context policy, compaction, memory store/extraction, branch/rewind, and provider replay; run focused React tests for agent/session UI; run typecheck/build.
  - Acceptance: the report can mark Priority 4 complete only after search, export, context visualization, manual compact, reviewed memory, branch/rewind, auto-compact policy, docs, and privacy hardening all have passing verification.
  - Verification: record the exact commands and passing results in the completion note, using one Cargo command at a time.

### Priority 5: Add External Integration Surfaces Only If Needed

These are valuable, but only after the agent engine and tooling are solid:

- Public gRPC/HTTP headless service.
- VS Code extension.
- Remote control/bridge/direct-connect.
- GitHub app and Slack app flows.
- Chrome/native-host integration.
- Voice mode.

## Capabilities Cadence Already Exceeds OpenClaude On

Cadence should not blindly copy OpenClaude in these areas because it already has a stronger or more domain-specific path:

- Desktop-native project/workflow shell.
- In-app browser sidebar with tab/storage/cookie management.
- Browser cookie import UX.
- iOS and Android emulator automation.
- Solana workbench, including clusters, personas, IDL, transactions, deploys, audits, replay, indexers, wallets, token flows, logs, and cost tooling.
- App-local provider profile UI.
- Operator approval loop with notification routing.
- Runtime stream projection into a desktop UI.

## Suggested Milestone Order

1. [x] Agent parity foundation: MCP invocation, subagents, todo/task tools, tool search, LSP, notebook editing, and PowerShell are now in the owned-agent runtime.
2. [x] Skills and plugins: make skills first-class in the UI and model tool list, then add plugin source/trust/reload mechanics.
3. [ ] Provider and diagnostics: add doctor reports, provider repair, profile recommendation, and missing provider presets.
4. [ ] Memory and sessions: add compact/export/search/resume/rename/branch-style user flows.
5. [ ] External surfaces: add headless API and editor/remote integrations if Cadence is meant to be used outside the desktop app.

## Cold-Read Check

A fresh engineer can use this report to identify the biggest missing pieces without needing the original code-inspection context. The report distinguishes OpenClaude parity gaps from capabilities Cadence already covers differently, and it prioritizes the missing work around the native agent runtime rather than around cosmetic UI parity.
