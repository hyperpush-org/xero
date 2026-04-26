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

- [ ] Slice 3.1.1: Add a provider profile validation engine.
  - Scope: inspect saved provider-profile metadata and credentials before any network probe; validate active profile, profile/provider/runtime-kind alignment, required base URL/API version/region/project fields, credential-link freshness, local readiness proofs, and ambient readiness proofs.
  - Acceptance: malformed or partially migrated profiles return actionable repair suggestions and do not require model catalog refresh to reveal the problem.
  - Verification: Rust tests cover ready/missing/malformed profiles for OpenAI Codex, OpenRouter, Anthropic, GitHub Models, OpenAI-compatible, Ollama, Azure OpenAI, Gemini AI Studio, Bedrock, and Vertex.

- [ ] Slice 3.1.2: Add active provider reachability probes.
  - Scope: reuse existing provider-model catalog and auth clients to run explicit reachability probes for the active profile, including OpenAI-compatible `/models`, GitHub Models catalog, Ollama local endpoint, OpenRouter, Anthropic-family providers, Bedrock, and Vertex.
  - Acceptance: probes classify DNS/connect timeout, 401/403, 404 endpoint-shape errors, 429/rate-limit, bad JSON, missing local service, and unsupported catalog strategies with provider-specific recovery text.
  - Verification: backend tests use mocked HTTP/auth clients and local fixture responses for success, timeout, auth failure, rate limit, bad JSON, stale cache fallback, and manual-catalog providers.

- [ ] Slice 3.1.3: Surface profile repair suggestions in Settings.
  - Scope: extend the existing Providers settings surface with compact diagnostic rows, repair calls to action, and a "Check connection" action that runs validation/probe for one profile.
  - Acceptance: users can see whether the issue is key, endpoint, model catalog, local service, or ambient auth; existing model catalog choices remain visible when Cadence has a stale usable cache.
  - Verification: React tests cover ready, missing key, malformed credential link, invalid base URL, unreachable local Ollama, stale cache with warning, and successful recheck.

##### Phase 2: Add Runtime Doctor Reports

Outcome: Cadence can produce an OpenClaude-style doctor report from the desktop app, with both readable and JSON forms.

- [ ] Slice 3.2.1: Implement the backend doctor report command.
  - Scope: add a Tauri command that gathers provider profile validation, provider reachability when requested, runtime session reconciliation, detached supervisor state, provider-model catalog state, MCP registry health, notification route readiness, and important app paths.
  - Acceptance: the command supports a quick local mode and an extended network mode; it reports partial failures without aborting the whole report; JSON output is stable and redacted.
  - Verification: Rust integration tests cover quick mode, extended mode, partial failure aggregation, unavailable app-data files, stale runtime session, and JSON redaction.

- [ ] Slice 3.2.2: Add a diagnostics settings surface.
  - Scope: add a ShadCN-based Diagnostics section or tab that displays doctor summary counts, grouped checks, last run timestamp, copyable JSON, and per-check remediation.
  - Acceptance: users can run quick or extended diagnostics, inspect failures without scrolling through raw logs, and copy the redacted report for support.
  - Verification: React tests cover empty state, running state, passed/warning/failed/skipped groups, JSON copy action, and malformed report handling.

- [ ] Slice 3.2.3: Thread doctor diagnostics into runtime startup failures.
  - Scope: connect runtime start/session failures to the same diagnostic vocabulary so a failed provider bind can offer "run diagnostics" and show the relevant provider/profile check inline.
  - Acceptance: runtime failures for stale binding, missing credentials, provider mismatch, unavailable local endpoint, and ambient auth missing all link back to the same remediation text users see in Settings.
  - Verification: state/view-builder tests cover runtime session failure projection, run-start failure projection, doctor suggestion visibility, and secret-free persisted diagnostics.

##### Phase 3: Add Provider Recommendation And Setup Guides

Outcome: Cadence can recommend a usable profile path for common local/cloud setups and help users configure OpenAI-compatible endpoints without guessing.

- [ ] Slice 3.3.1: Add provider recommendation logic.
  - Scope: inspect saved profiles, credential readiness, local Ollama reachability, configured OpenAI-compatible endpoints, and ambient Bedrock/Vertex readiness to recommend a default profile path.
  - Acceptance: recommendations distinguish fastest-ready profile, best local profile, missing-key cloud profile, and unsupported/incomplete profile; recommendations never activate or mutate profiles without user action.
  - Verification: pure tests cover no profiles, OpenAI Codex ready, OpenRouter ready, Ollama reachable, local OpenAI-compatible endpoint, Bedrock/Vertex ambient ready, and multiple competing ready profiles.

- [ ] Slice 3.3.2: Add OpenAI-compatible and LiteLLM setup recipes.
  - Scope: add structured recipe metadata for common OpenAI-compatible setups, including LiteLLM, LM Studio, Groq, Together, DeepSeek, and custom `/v1` gateways; map each recipe to provider profile fields, auth mode, model catalog expectations, and repair suggestions.
  - Acceptance: users can choose a recipe that pre-fills label/base URL/API-key expectations while still saving through the existing provider-profile contract.
  - Verification: TypeScript tests cover recipe validation, generated upsert requests, required-field prompts, local endpoint no-key behavior, and catalog/manual-model expectations.

- [ ] Slice 3.3.3: Present setup guidance in Providers Settings.
  - Scope: add a ShadCN recipe picker and compact guidance inside the existing Providers section, without replacing the current provider cards.
  - Acceptance: recipes make the setup path clearer for LiteLLM/OpenAI-compatible/local gateways, and they feed into the same save/check connection flow as first-class presets.
  - Verification: React tests cover recipe selection, prefilled fields, required key/base URL messaging, save flow, check connection handoff, and no temporary debug controls.

##### Phase 4: Fill Missing Provider Preset Parity

Outcome: Cadence covers OpenClaude's missing provider setup surface either as first-class presets or as documented OpenAI-compatible recipes.

- [ ] Slice 3.4.1: Add direct Mistral provider support or a first-class Mistral recipe.
  - Scope: decide whether Mistral should be a dedicated provider id or a locked OpenAI-compatible recipe; implement the selected path through provider presets, runtime provider identity, endpoint resolution, model catalog behavior, and settings UI.
  - Acceptance: users can save a Mistral-backed profile, validate it, refresh or manually specify models according to the chosen transport, and launch a runtime without secret leakage.
  - Verification: Rust and React tests cover profile save, endpoint resolution, catalog behavior, runtime binding, stale binding, settings rendering, and diagnostics.

- [ ] Slice 3.4.2: Add NVIDIA NIM, MiniMax, and Foundry recipes.
  - Scope: add recipe metadata and validation for NVIDIA NIM, MiniMax, and Foundry-compatible endpoints using the existing OpenAI-compatible profile path unless a direct provider is required.
  - Acceptance: each recipe has clear required fields, default endpoint guidance, catalog/manual-model behavior, and provider-specific repair text.
  - Verification: TypeScript recipe tests and provider-profile form tests cover generated requests, invalid/missing fields, catalog expectations, and repair text.

- [ ] Slice 3.4.3: Add Atomic Chat local setup support.
  - Scope: add a local recipe or provider preset for Atomic Chat with no fake key requirement, local endpoint reachability, and manual-model fallback when catalog discovery is unavailable.
  - Acceptance: users can configure Atomic Chat as a local model backend, see local readiness state, and run connection checks without storing placeholder secrets.
  - Verification: Rust tests cover local readiness and endpoint resolution; React tests cover setup, missing local service, manual model fallback, and runtime launch handoff.

- [ ] Slice 3.4.4: Add GitHub Models device onboarding only if it fits Cadence auth.
  - Scope: evaluate and implement a GitHub Models device-flow path only if it can share Cadence's existing provider-profile credential store and redaction rules; otherwise document API-token setup as the supported path and keep device onboarding out of scope.
  - Acceptance: the chosen path is explicit in the report/docs; if implemented, device onboarding saves a redacted app-local token link and reuses profile readiness/catalog diagnostics.
  - Verification: auth-flow tests cover successful device flow or documented non-support, cancellation, stale flow rejection, token redaction, profile readiness, and catalog discovery.

##### Phase 5: Documentation And Completion Criteria

Outcome: Priority 3 is safe enough to call complete and useful for both users and implementation agents.

- [ ] Slice 3.5.1: Document provider setup and diagnostics workflows.
  - Scope: document how users configure each supported provider path, how recipe-based OpenAI-compatible setup works, what each diagnostic state means, and when to use quick vs extended doctor reports.
  - Acceptance: a fresh user can set up a common cloud provider, a local provider, and a custom OpenAI-compatible endpoint without reading source code; a support engineer can interpret a redacted doctor JSON report.
  - Verification: docs review plus tests for any executable examples or fixture-backed recipes included in the docs.

- [ ] Slice 3.5.2: Add privacy and no-secret hardening for diagnostics.
  - Scope: audit doctor output, provider diagnostics, copied JSON, runtime failure text, and model-facing diagnostics for leaked API keys, OAuth tokens, local secret file contents, and unnecessary absolute paths.
  - Acceptance: diagnostics include enough non-secret metadata to repair the issue while excluding raw secrets and secret-bearing paths from persisted, copied, and model-visible surfaces.
  - Verification: Rust and TypeScript tests cover redaction of API keys, bearer headers, OAuth/session ids, ADC/AWS path content, local endpoint credentials, and nested diagnostic payloads.

- [ ] Slice 3.5.3: Declare Priority 3 complete.
  - Scope: run focused Rust tests for provider profiles, provider model catalogs, runtime session/supervisor diagnostics, and doctor report generation; run focused React tests for Providers and Diagnostics settings surfaces; run build/type checks.
  - Acceptance: the report can mark Priority 3 complete only after provider reachability, doctor reports, profile repair, recommendations, setup recipes/presets, docs, and privacy hardening all have passing verification.
  - Verification: record the exact commands and passing results in the completion note, using one Cargo command at a time.

### Priority 4: Add Session Memory And Context Management

Cadence persists runs well, but OpenClaude has richer working-memory features:

- Context visualization.
- Auto-compact and manual compact.
- Session export and transcript search.
- Memory extraction/consolidation.
- Project instruction/memory file management.
- Cross-session resume search and rename/branch/rewind equivalents.

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
