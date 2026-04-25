# OpenClaude Capability Gap Report

Date: 2026-04-25
Last updated: 2026-04-25

## Reader And Action

This report is for an engineer planning the next Cadence work. After reading it, they should be able to choose which OpenClaude capabilities still need to be brought into this project, which ones are already covered by Cadence in a different form, and which gaps are low priority because Cadence intentionally has a different product shape.

## Scope

The comparison covers the local OpenClaude repository and the current Cadence repository state. OpenClaude is a terminal-first coding-agent CLI with optional service and editor integrations. Cadence is a Tauri desktop app with a React interface, Rust command surface, app-local persistence, and desktop-native sidebars.

Some OpenClaude modules are feature-gated or internal-only. I counted a capability as OpenClaude-supported when it appears in the public README, package scripts, command registry, tool registry, provider/profile utilities, service modules, or extension docs. Feature-gated/internal capabilities are noted as such when they matter.

## Executive Summary

Cadence already covers the broad desktop shell, project import, file editing, provider profiles, OpenAI Codex OAuth, runtime supervision, operator approval flows, notifications, MCP registry management, in-app browser automation, emulator automation, and a large Solana workbench. In several areas Cadence is beyond OpenClaude, especially mobile device automation and Solana-specific tooling.

The largest missing OpenClaude-equivalent area is not another sidebar. It is the mature agent operating system around the model loop: terminal CLI/REPL, slash commands, plugins, skills, memory/compaction, diagnostics, provider launch profiles, and editor/remote integrations.

Cadence has a native owned-agent runtime and autonomous tool runtime, but it is not yet equivalent to OpenClaude's CLI agent runtime. The Priority 1 native tool-surface gap is now closed: owned agents have MCP invocation, subagents, todo/task state, notebook editing, PowerShell, LSP/code intelligence, tool search, and command-session support. Remaining gaps are now concentrated in plugin/skill command loading, default web search provider breadth, memory/context management, diagnostics, session operations, and external integration surfaces.

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
| Slash commands | Desktop settings/actions and runtime controls | No command registry equivalent for provider, doctor, compact, memory, plugin, skills, mcp, review, security-review, permissions, etc. |
| Provider launch profiles | App-local provider profile store | No `.openclaude-profile.json` compatibility, profile launcher scripts, provider recommendation, or CLI profile bootstrap |
| Provider breadth | Strong built-in presets for common cloud/local providers | Missing direct Mistral, Atomic Chat, NVIDIA NIM, MiniMax, Foundry, LiteLLM-oriented docs/profile flow, and GitHub device onboarding |
| Per-agent routing | Active profile/model controls | No settings-level agent model routing by agent name/type |
| Core tool parity | Native autonomous tools for files/git/commands/web/browser/emulator/Solana plus Priority 1 agent tools | Priority 1 is complete; still missing cron/monitor and first-class AskUserQuestion equivalents |
| Subagents | Native owned-agent subagent tool with built-in Explore/Plan/general/verification types and model routing | Still missing custom agent definitions and team/swarm tools |
| MCP runtime use | Registry, probes, projection to detached runtime, and native owned-agent MCP tool/resource/prompt invocation | Still missing broader MCP auth/server approval UX and marketplace-style discovery |
| Plugins | None equivalent in UI/runtime | No plugin marketplace, plugin command loader, plugin skills, trust warning, or reload flow |
| Skills | Autonomous skill discovery/install/invoke from a GitHub source | Not equivalent to OpenClaude's skill directories, bundled skills, dynamic skills, plugin skills, MCP skills, and SlashCommand/SkillTool integration |
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

### Priority 2: Bring Over Skills And Plugins In A Cadence-Native Way

Cadence already has an autonomous skill runtime, but it should be connected to the user and model experience:

- Local skill directories.
- Bundled skills.
- Project skills.
- Dynamic skills discovered during work.
- MCP-provided skills.
- Plugin-provided skills.
- A model-visible SkillTool equivalent.
- A settings UI for installed skills/plugins, source trust, and reload.

### Priority 3: Build Runtime Diagnostics And Provider Setup Parity

Cadence's provider UI is good, but OpenClaude has stronger startup and troubleshooting loops:

- Provider reachability diagnostics.
- Runtime doctor report with human and JSON modes.
- Saved-profile validation and repair suggestions.
- Provider recommendation for local models.
- LiteLLM/OpenAI-compatible setup guidance.
- GitHub Models device onboarding.
- Mistral, Atomic Chat, NVIDIA NIM, MiniMax, and Foundry presets or documented OpenAI-compatible recipes.

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
2. [ ] Skills and plugins: make skills first-class in the UI and model tool list, then add plugin source/trust/reload mechanics.
3. [ ] Provider and diagnostics: add doctor reports, provider repair, profile recommendation, and missing provider presets.
4. [ ] Memory and sessions: add compact/export/search/resume/rename/branch-style user flows.
5. [ ] External surfaces: add headless API and editor/remote integrations if Cadence is meant to be used outside the desktop app.

## Cold-Read Check

A fresh engineer can use this report to identify the biggest missing pieces without needing the original code-inspection context. The report distinguishes OpenClaude parity gaps from capabilities Cadence already covers differently, and it prioritizes the missing work around the native agent runtime rather than around cosmetic UI parity.
