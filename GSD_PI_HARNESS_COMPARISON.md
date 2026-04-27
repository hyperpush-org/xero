# GSD Pi Harness vs Cadence Harness

Date: 2026-04-27

This document compares the core GSD Pi harness in `/Users/sn0w/Documents/dev/gsd-2` with the Cadence/Joe app harness in this repository. The focus is the main execution harness: file reads and edits, commands, search, long-running processes, process visibility and control, system automation, safety policy, auditability, and extensibility.

## Executive Summary

Cadence already has a solid harness foundation. Its strengths are structured Rust tools, repo-scoped file access, approval policy, command redaction, rollback checkpoints, durable tool events, and a predictable Tauri-integrated runtime. It is generally safer and easier to audit than GSD Pi's base shell/file tools.

GSD Pi is ahead in raw harness capability. It can drive the system more naturally and persistently through real shell execution, background shell/process management, async jobs, process-tree helpers, image-aware reads, ripgrep/native search, hashline edits, browser tooling, and macOS app/window automation. The biggest gap is not simple command execution; Cadence has that. The biggest gap is a first-class process manager and interactive shell/session layer that lets the agent list, inspect, wait for, message, restart, and kill long-running work across turns.

The highest-priority target for Cadence should be a safe, approval-gated equivalent to GSD Pi's `bg_shell` plus better search and process discovery. Cadence should keep its current safety posture, but add richer system-control tools behind explicit policy boundaries.

## Source Map

### Cadence/Joe Harness

- `client/src-tauri/src/runtime/autonomous_tool_runtime/mod.rs`: built-in tool registry, dispatch, tool groups, limits.
- `client/src-tauri/src/runtime/autonomous_tool_runtime/filesystem.rs`: `read`, `search`, `find`, `edit`, `write`, `patch`, `delete`, `rename`, `mkdir`, `list`, `file_hash`.
- `client/src-tauri/src/runtime/autonomous_tool_runtime/repo_scope.rs`: repo-relative path normalization, skip rules, symlink avoidance.
- `client/src-tauri/src/runtime/autonomous_tool_runtime/process.rs`: one-shot commands and long-running command sessions.
- `client/src-tauri/src/runtime/process_tree.rs`: process-group/tree termination.
- `client/src-tauri/src/runtime/autonomous_tool_runtime/policy.rs`: command approval and classifier.
- `client/src-tauri/src/runtime/agent_core/tool_descriptors.rs`: prompt-visible tool descriptor selection.
- `client/src-tauri/src/runtime/agent_core/tool_dispatch.rs`: tool-call persistence, write guards, checkpoints, events.
- `client/src-tauri/src/runtime/agent_core/persistence.rs`: rollback checkpoints and durable command/file events.
- `client/src-tauri/src/runtime/redaction.rs`: sensitive data redaction.
- `client/src-tauri/src/runtime/autonomous_tool_runtime/browser.rs`: in-app browser/webview automation tools.

### GSD Pi Harness

- `/Users/sn0w/Documents/dev/gsd-2/packages/pi-coding-agent/src/core/tools/index.ts`: core coding tool sets.
- `/Users/sn0w/Documents/dev/gsd-2/packages/pi-agent-core/src/agent-loop.ts`: agent loop, tool hooks, tool streaming, parallel tool-call support.
- `/Users/sn0w/Documents/dev/gsd-2/packages/pi-coding-agent/src/core/tools/bash.ts`: shell command execution.
- `/Users/sn0w/Documents/dev/gsd-2/packages/pi-coding-agent/src/utils/shell.ts`: shell resolution, environment setup, process-tree kill.
- `/Users/sn0w/Documents/dev/gsd-2/packages/pi-coding-agent/src/core/tools/read.ts`: text/image reads.
- `/Users/sn0w/Documents/dev/gsd-2/packages/pi-coding-agent/src/core/tools/edit.ts`: exact/fuzzy edits.
- `/Users/sn0w/Documents/dev/gsd-2/packages/pi-coding-agent/src/core/tools/write.ts`: file writes.
- `/Users/sn0w/Documents/dev/gsd-2/packages/pi-coding-agent/src/core/tools/grep.ts`: ripgrep-backed search.
- `/Users/sn0w/Documents/dev/gsd-2/packages/pi-coding-agent/src/core/tools/find.ts`: native glob-backed file finding.
- `/Users/sn0w/Documents/dev/gsd-2/packages/pi-coding-agent/src/core/tools/path-utils.ts`: path expansion and platform normalization.
- `/Users/sn0w/Documents/dev/gsd-2/packages/pi-coding-agent/src/core/tools/hashline_*`: hash-anchored reads and edits.
- `/Users/sn0w/Documents/dev/gsd-2/native/crates/engine/src/ps.rs`: native process tree/list/kill support.
- `/Users/sn0w/Documents/dev/gsd-2/src/resources/extensions/bg-shell/*`: background process manager and persistent shell tools.
- `/Users/sn0w/Documents/dev/gsd-2/src/resources/extensions/async-jobs/*`: async command jobs.
- `/Users/sn0w/Documents/dev/gsd-2/src/resources/extensions/browser-tools/*`: Playwright browser harness.
- `/Users/sn0w/Documents/dev/gsd-2/src/resources/extensions/mac-tools/*`: macOS system/app/window automation.

## Capability Matrix

| Capability | GSD Pi | Cadence/Joe | Gap |
| --- | --- | --- | --- |
| Tool registry | Core tools plus extension-provided tools/hooks/renderers. | Fixed Rust built-ins grouped by prompt-selected descriptors, plus skills/MCP-style surfaces. | Cadence has less dynamic harness extensibility. |
| File read | Text plus image reads, offsets, limits, screenshot path variants. | Repo-scoped UTF-8 text reads, line ranges, size cap. | Cadence lacks image/binary previews and flexible path handling. |
| File write/edit | Write, exact/fuzzy edit, hashline edit, LSP notification. | Write, edit by line range with expected text, patch with expected hash, delete/rename/mkdir. | Cadence has better rollback/safety; Pi has better edit ergonomics and image/path support. |
| Search | Ripgrep JSON, regex/literal, glob, ignore-case, context, hidden files, `.gitignore`, native glob find. | Literal substring search and glob find over repo walker, default 100 result cap. | Cadence search is much weaker. |
| Command execution | Shell-native command string, streaming updates, timeout, output spill, abort kill tree. | Argv-based command, sanitized env, timeout, stdout/stderr capture, redaction, approval policy. | Cadence is safer; Pi is more natural and expressive. |
| Long-running sessions | `bg_shell` process manager and async jobs. | `command_session_start/read/stop`, max 8 sessions. | Cadence lacks stdin, readiness, digest, restart, groups, persistent shell state, and cross-turn context alerts. |
| Process discovery | Native process tree helpers, bg-shell list/group/status, mac app list. | Can kill only owned command/session trees. No general process list/kill tool. | Major system-control gap. |
| Output handling | Streaming updates, tails, digest/highlights, artifact spills, incremental output cursors. | Bounded excerpts and session chunks. | Cadence needs richer output retrieval and durable full-output artifacts. |
| Browser automation | Very broad Playwright extension: refs, console, network, trace, HAR, a11y, visual diffs, mocking, devices. | In-app browser/webview actions: navigate/click/type/query/screenshot/storage/cookies/history. | Cadence has useful basics, not Pi-level browser diagnostics. |
| macOS/system automation | Bundled mac-tools for permissions, app list/launch/activate/quit and broader app/window automation. | No equivalent general OS/app automation surface. | Major gap if "control the system" is a goal. |
| Safety and approval | More permissive filesystem/shell posture by default; extensions can be powerful. | Repo scope, approval classifier, sanitized env, rollback checkpoints, redaction, workspace guard. | Cadence is stronger here and should preserve it. |
| Rollback/audit | Tool events and outputs exist; less built-in rollback in inspected core file tools. | Durable start/end events, file changes, command output, rollback checkpoints. | Cadence advantage. |
| Parallel tool calls | Agent loop supports sequential or parallel tool calls. | Tool dispatch appears single-call oriented per request; concurrency comes from runtime/session model. | Consider explicit safe parallel read/search support. |

## Deep Comparison

### 1. Tool Registry and Agent Loop

GSD Pi exposes a small base coding tool set (`read`, `bash`, `edit`, `write`) plus read-only tools (`grep`, `find`, `ls`) and newer hashline tools. The larger harness surface comes from bundled extensions such as `bg-shell`, `async-jobs`, `browser-tools`, and `mac-tools`. Extensions can register tools, hooks, commands, renderers, and lifecycle behavior.

Cadence defines its built-ins in Rust. `mod.rs` groups tools into core, mutation, command, web, emulator, Solana, agent operations, MCP, intelligence, notebook, PowerShell, and skills. `tool_descriptors.rs` selects visible descriptors based on prompt keywords and hidden capability requests. This is disciplined and predictable, but it means Cadence's agent-callable harness surface is comparatively fixed.

Cadence's model is safer and easier to reason about. GSD Pi's model is more expandable and lets harness capabilities appear through extensions without modifying the core runtime. If Cadence needs to match Pi long-term, it needs a sanctioned way to add privileged local harness tools with policy, audit, and typed schemas, not only model-facing prose skills.

### 2. File Reads

GSD Pi's `read` tool can read normal text files and common image formats (`jpg`, `png`, `gif`, `webp`). Images are resized down to a max dimension and returned as media. Its path utilities expand `~`, tolerate absolute paths, normalize Windows/MSYS paths, strip `@` prefixes, and handle macOS screenshot filename variants such as Unicode normalization and AM/PM punctuation differences.

Cadence's `read` is intentionally narrower. It resolves paths through `RepoScope`, requires repo-relative normalized paths, rejects traversal, avoids symlinks, rejects non-UTF-8, and enforces a max text file size. It supports bounded line ranges with defaults and caps.

This is a safety win for Cadence but a capability gap for a full system harness. An agent sometimes needs to inspect screenshots, generated images, binary metadata, large logs, files outside the imported repo, or OS paths shown by command output. Today Cadence usually needs to go through `command` for those cases, which is less structured and less inspectable.

Recommended target:

- Keep repo-scoped `read` as the default.
- Add a separate, policy-gated `system_read` or expanded `read` mode for absolute paths, `~`, binary metadata, and image previews.
- Add image/media payload support for common image formats.
- Add offset/byte and line-range modes for large logs, with clear truncation metadata.
- Preserve UTF-8 text behavior and repo-scope defaults for normal code work.

### 3. File Mutation

GSD Pi provides `write`, `edit`, and hashline tools. The normal edit path supports exact and fuzzy replacement, preserves line endings, handles BOMs, checks uniqueness, returns diff details, and notifies LSP. Hashline tools let the model read and edit with content hashes anchored to lines, improving edit targeting in large files or stale contexts.

Cadence provides a broader set of structured mutations: `edit`, `write`, `patch`, `delete`, `rename`, and `mkdir`. It also supports `file_hash`, expected text guards, optional expected hashes, rollback checkpoints, file-change events, and workspace write-guard validation.

Cadence is stronger on safety, rollback, and audit. GSD Pi is stronger on authoring ergonomics. Cadence's `edit` by line range plus `expected` guard is safe, but line-number targeting can be brittle after adjacent edits. `patch` exact replace helps, but hashline-style anchored editing would reduce failures and accidental edits.

Recommended target:

- Add hashline read/edit equivalents or line-hash anchors in `read` output.
- Preserve line endings and BOMs explicitly in mutation tools.
- Return compact unified diffs for mutation results.
- Keep rollback checkpoints mandatory for file-destructive operations.
- Keep repo-scope default and explicit approval for wider system file writes.

### 4. Search and File Discovery

GSD Pi's search is substantially ahead. `grep` uses ripgrep JSON output, supports regex or literal matching, globs, ignore-case, context lines, hidden files, `.gitignore` behavior, max matches, and long-line truncation. `find` uses a native glob engine with caching and supports hidden/gitignored controls.

Cadence's `search` is a literal substring search over the repo walker. `find` uses `globset` over repo files. This is predictable and safe but limited. It lacks regex, context, ignore-case, richer globs, ripgrep performance, and scalable result handling.

This matters because search is part of the harness, not a convenience feature. A coding agent's ability to understand a repository depends heavily on fast, expressive, accurate search.

Recommended target:

- Implement ripgrep-backed `search` with structured JSON parsing.
- Support literal/regex, ignore-case, file globs, include/exclude, context lines, hidden files, and `.gitignore` controls.
- Return deterministic capped results with total-count/truncation metadata.
- Keep repo-scope enforcement and skip dangerous/generated directories by default.
- Consider a separate `symbols`/`code_search` layer for AST or LSP-aware searches.

### 5. Command Execution

GSD Pi's `bash` is shell-native. It runs commands through the user's configured shell, supports streaming updates, timeouts, abort handling, command interception, hook-based spawn customization, app-bin PATH injection, full shell-ish environment, and full-output spill to temp/artifacts when truncated. It rewrites unquoted trailing background commands to avoid pipe hangs.

Cadence's `command` is argv-based. It does not invoke a shell unless the model explicitly asks for one. It clears the environment and passes an allowlist of variables such as `PATH`, `HOME`, `USER`, `SHELL`, cache paths, Windows system variables, and a marker indicating the env is sanitized. It has timeout/cancellation handling and kills the process tree on timeout. It redacts output before durable persistence. Nonzero exits are returned as structured tool results rather than runtime errors.

Cadence's approach is safer and more deterministic. GSD Pi's is more convenient and closer to how developers actually run commands. The missing piece in Cadence is not that it needs unsafe shell-by-default behavior; it needs better support for shell workflows when explicitly requested and approved.

Recommended target:

- Keep argv execution as default.
- Add an explicit `shell` option or separate `shell_command` tool with stronger approval policy.
- Add output streaming updates to the agent loop/UI.
- Add artifact-backed full output storage for truncated commands.
- Expose tail/head/byte limits and cursors for command output retrieval.
- Add command hooks only if they can be audited and policy-checked.

### 6. Long-Running Processes and Interactive Shells

This is the largest gap.

GSD Pi's `bg_shell` is a real process manager. It supports:

- `start`: launch a process with label, type, cwd, group, ready pattern, ready port, timeout, and persistence options.
- `digest`: low-token summary of one or all running processes.
- `output`: incremental raw output since last check, with tail/filter controls.
- `highlights`: summarized errors, warnings, URLs, ports, and status changes.
- `wait_for_ready`: wait for a pattern or port.
- `send`: send stdin to a process.
- `send_and_wait`: send stdin and wait for matching output.
- `run`: execute commands in a persistent shell with persistent cwd/env.
- `env`: inspect live shell cwd/env.
- `signal`: send an OS signal.
- `list`: list managed processes.
- `kill`: terminate managed processes.
- `restart`: kill and relaunch with original config.
- `group_status`: inspect grouped processes.

It also has lifecycle hooks that clean up processes, persist manifests, detect surviving processes, inject alerts into future agent context, and show a live footer widget. This is exactly the kind of harness behavior that makes an agent feel competent around dev servers, watchers, REPLs, test runners, and interactive CLIs.

Cadence has `command_session_start`, `command_session_read`, and `command_session_stop`. This is a good start: sessions are registered, capped, chunked, killed on stop/drop, and protected by process-tree termination. But sessions start with `stdin(Stdio::null())`, so the agent cannot send input. There is no persistent shell env/cwd, readiness check, port detection, process grouping, restart, digest, highlights, lifecycle context reinjection, or general list beyond managed sessions.

Recommended P0 target:

- Build a first-class `process_manager` tool group.
- Actions should include `start`, `list`, `status`, `digest`, `output`, `wait_for_ready`, `send`, `send_and_wait`, `signal`, `kill`, `restart`, and `group_status`.
- Store process metadata: id, pid, pgid/job id, label, command, cwd, env summary, owner session/thread, status, start time, exit code, ports, URLs, recent errors/warnings, output cursor, restart count.
- Support stdin for opted-in interactive sessions.
- Add readiness detectors for regex output, port open, HTTP URL, and process exit.
- Add group ownership so a test server and watcher can be treated as one unit.
- Reinject concise process state into agent context across turns.
- Require approvals for risky commands, signals, restarts, and process persistence.

### 7. Process Visibility and Kill

GSD Pi has native process tree capabilities in Rust (`list_descendants`, `kill_tree`, `process_group_id`, `kill_process_group`) and exposes process management through bg-shell. Its mac-tools extension can list running macOS apps with names, bundle IDs, PIDs, and active status, and can launch, activate, and quit apps.

Cadence can kill process trees that it owns through one-shot commands and command sessions. It does not expose general process listing, descendant listing, listening port inspection, PID kill, process-group kill, or app list/quit/activate capabilities.

For a harness that can "control the system", this is a major gap. Cadence should add this carefully rather than making arbitrary shell commands the only escape hatch.

Recommended target:

- Add `process_list` for owned sessions first.
- Add policy-gated `system_process_list` with filters by name, cwd, parent, port, and owner.
- Add `process_tree` for a PID, using platform-native APIs where possible.
- Add `process_kill` with mode `pid`, `tree`, or `group`, requiring explicit approval unless the process is Cadence-owned.
- Add `port_list` or `listening_ports` to identify local servers.
- On macOS, add `app_list`, `app_activate`, `app_quit`, and permission checks if app-control is in scope.

### 8. Output Handling, Streaming, and Artifacts

GSD Pi's tools stream updates during command execution. Long output can spill to temp files or artifacts. Background processes provide incremental output, digest, and highlights so the agent can understand a server or watcher without rereading massive logs.

Cadence captures bounded stdout/stderr excerpts and session chunks. It stores durable command output events and redacts sensitive data. This is safer, but it is less capable for long-running workflows and large outputs.

Recommended target:

- Add full-output artifact storage with redaction.
- Add output cursors so the agent can ask for only new bytes/lines since the last read.
- Add structured highlights: URLs, ports, warnings, errors, stack traces, failed test names, build status.
- Add digest summaries for long-running sessions.
- Stream command progress to the UI/tool event channel, not just final excerpts.

### 9. Browser, Web, and UI Automation

GSD Pi's browser-tools extension is far beyond basic browser control. It includes navigation, click/type/drag/upload/scroll/hover/key/select/check/wait/screenshot, console and network inspection, dialogs, evaluate, accessibility tree, source extraction, tracing, HAR, timeline, session summaries, debug bundles, assertions, visual diffs, batching, refs, page/frame/form helpers, intent helpers, PDF, state storage, mocking, device emulation, extraction, zoom, test generation, and action caching.

Cadence has an in-app browser/webview automation tool with useful basics: open, tab operations, navigate, back/forward/reload/stop, click, type, scroll, key press, read text, query, wait for selector/load, URL/history state, screenshot, cookies, storage, and tab management.

The repo instruction says this app cannot be opened in a browser because it is a Tauri app. That does not eliminate browser automation needs for web targets, OAuth flows, docs, or embedded webviews. It does mean Cadence should not prioritize generic browser parity over Tauri-native app harness capabilities unless product requirements demand it.

Recommended target:

- Keep the current webview automation layer.
- Add console/network/trace/a11y only when needed by real workflows.
- For the Tauri app itself, prioritize native app/test harnesses over browser-only assumptions.
- If browser parity is desired, implement it as an optional tool group with clear policy and state isolation.

### 10. macOS and System Automation

GSD Pi's mac-tools extension gives it a foothold in OS-level control. It can check Accessibility/Automation/Screen Recording permissions, list running apps, launch apps, activate apps, quit apps, and appears designed for broader window/UI automation through Swift helpers.

Cadence has no comparable general OS automation harness. It can run shell commands, but there is no typed, discoverable tool for "what apps are running", "bring this app forward", "quit this app", "take a system screenshot", "list windows", or "inspect accessibility tree".

If Cadence's goal includes controlling the local development system, a typed OS automation layer is better than relying on arbitrary shell commands and AppleScript snippets.

Recommended target:

- Add permission introspection first.
- Add app list/launch/activate/quit for macOS.
- Add window list/focus/move/resize where useful.
- Add screenshot capture and optional image-read integration.
- Gate OS automation behind explicit user approval and visible UI affordances.

### 11. Safety, Policy, and Auditability

Cadence is ahead here.

Cadence has:

- Repo-relative path enforcement.
- Symlink avoidance.
- Skipped generated/heavy directories.
- A command approval classifier.
- Escalation for network, destructive, and ambiguous commands.
- Shell wrapper inspection.
- Sanitized command environment.
- Tool event persistence.
- Write-intent validation.
- Rollback checkpoints for mutating tools.
- Redaction of sensitive output and arguments.
- Process-tree cleanup for owned commands.

GSD Pi has powerful tools, but its base file and shell model is more permissive. It accepts absolute paths and `~`, uses a real shell by default, and extensions can add broad capability. That power is useful, but it increases risk.

Cadence should not copy Pi's permissiveness directly. The better goal is "Pi-level capability with Cadence-level policy." New system-control tools should be explicit, typed, auditable, and approval-gated.

### 12. Extensibility

GSD Pi's extension system is a major advantage. `bg-shell`, `async-jobs`, `browser-tools`, and `mac-tools` show that substantial harness capabilities can be shipped outside the base coding-agent package. Extensions can hook lifecycle events, tool execution, session compaction/switching, and agent start/end.

Cadence has MCP, skills, notebook editing, intelligence tools, and tool descriptor selection. Those are valuable, but the inspected autonomous runtime is still a fixed built-in tool set. There is not yet an equivalent pattern where a privileged local extension can register a typed tool, receive lifecycle hooks, persist state, and participate in policy.

Recommended target:

- Define a local harness extension API with typed schemas and policy metadata.
- Let extensions register tools, lifecycle hooks, state stores, and UI summaries.
- Require tool descriptors, risk labels, approval modes, and redaction rules.
- Keep privileged filesystem/process APIs centralized so extensions cannot silently bypass policy.

## Cadence Strengths to Preserve

1. Repo scope by default.
2. Structured argv command execution by default.
3. Sanitized command environment.
4. Approval classifier and explicit escalation.
5. Durable tool start/end/file/command events.
6. Redaction before persistence.
7. Rollback checkpoints for mutations.
8. Process-tree cleanup for owned work.
9. Typed Rust tool implementations.
10. Prompt-visible tool groups that avoid overexposing unnecessary tools.

These are not gaps. They are the foundation that should make Cadence's future system-control layer safer than GSD Pi's.

## Priority Gap Backlog

### P0: Process Manager / Background Shell Parity

Build a Cadence-native process manager comparable to `bg_shell`.

Minimum actions:

- `start`
- `list`
- `status`
- `output`
- `digest`
- `wait_for_ready`
- `send`
- `send_and_wait`
- `signal`
- `kill`
- `restart`
- `group_status`

Minimum metadata:

- Tool/session owner.
- PID and process group/job id.
- Command and argv/shell mode.
- Cwd and sanitized env summary.
- Label, type, and group.
- Start time, exit time, exit code.
- Output cursor and recent output ring buffer.
- Detected URLs, ports, errors, and warnings.
- Readiness state.
- Restart count.

This is the single highest-leverage improvement for making Cadence feel as capable as GSD Pi.

### P0: Interactive Session Input

Add stdin support to command sessions or the new process manager. Without `send`/`send_and_wait`, Cadence cannot handle REPLs, CLIs that prompt, test watchers, dev servers with interactive controls, or shell sessions with persistent state.

### P0: Process Visibility and Safe Kill

Add typed process inspection and killing:

- Owned session list.
- System process list with filters.
- Process tree for PID.
- Listening local ports.
- Kill owned process without extra approval.
- Kill external process/tree/group with explicit approval.

This should use platform-native process APIs where possible rather than shelling out blindly.

### P1: Ripgrep-Grade Search

Replace or supplement literal `search` with a ripgrep-backed structured search:

- Regex/literal mode.
- Ignore-case.
- Include/exclude globs.
- Context lines.
- Hidden and gitignore controls.
- Match/file caps with truncation metadata.
- High performance on large repos.

### P1: Output Artifacts and Incremental Logs

Add durable full-output artifacts for truncated commands and sessions. Provide cursors and tail controls so the agent can fetch new output without replaying everything. Add digest/highlight extraction for long-running output.

### P1: Hash-Anchored Editing

Add hashline-style read/edit or line-hash anchors to reduce stale-context edit failures. Keep the current expected-text and expected-hash protections.

### P1: System Read / Image Preview

Add image and binary-aware reading, behind repo-scope defaults and approval for absolute paths. This should cover screenshots, generated visual artifacts, and binary metadata.

### P2: macOS App/System Automation

If Cadence is intended to control the desktop environment, add a typed macOS tool group:

- Permission check.
- App list.
- Launch app.
- Activate app.
- Quit app.
- Window list/focus.
- Screenshot capture.

This should be clearly visible to the user and require approvals for intrusive actions.

### P2: Browser Diagnostics

Expand browser/webview automation only where needed:

- Console logs.
- Network logs.
- Accessibility tree.
- Trace/HAR.
- Visual diff.
- Device emulation.
- State save/restore.

This is valuable, but lower priority than process/session parity for a Tauri coding app.

### P2: Harness Extension API

Add a typed extension mechanism for privileged local tools:

- Tool registration.
- Risk metadata.
- Approval metadata.
- Redaction rules.
- Lifecycle hooks.
- State persistence.
- UI summary/render hook.

This would let Cadence add Pi-like capabilities without bloating the central runtime.

## Suggested Target Design

Cadence should aim for a two-ring harness model.

Ring 1: Safe Repo Harness

- Current repo-scoped file and command tools.
- Default read/edit/search/write behaviors.
- Strict path normalization.
- Rollback checkpoints.
- Sanitized env.
- Approval classifier.

Ring 2: Approved System Harness

- Process manager.
- Interactive shell/session support.
- System process list/kill.
- System read/image preview.
- macOS app/window automation.
- Optional browser diagnostics.

Ring 2 should be typed and explicit. It should not be "just run any shell command and hope." Each tool should declare its risk, approval requirement, persistence behavior, redaction behavior, and rollback/cleanup behavior.

## Phased Implementation Approach

The implementation should move from low-risk owned-process parity toward broader system control. The key principle is that each phase should make Cadence more capable without weakening the existing repo-scope, approval, rollback, and redaction model.

### Phase 0: Harness Contracts and Safety Invariants

Goal: define the shape of the new system harness before adding powerful tools.

This phase should produce design contracts, type definitions, test scaffolding, and policy decisions. It should not expose broad new control to the model yet.

Core work:

- Define a `process_manager` tool group and action schema.
- Define process ownership: thread id, session id, repo id, user id if available, and whether the process is Cadence-owned or external.
- Define risk levels for process actions: observe, run-owned, signal-owned, signal-external, persistent-background, system-read, and OS-automation.
- Define persistence rules for process metadata, output chunks, redaction, and cleanup.
- Define output limits: ring-buffer size, full-output artifact thresholds, excerpt limits, and cursor behavior.
- Define lifecycle behavior for app shutdown, thread switch, session compaction, and crash recovery.
- Decide whether the first implementation extends `command_session_*` or introduces a new `process_manager` module. Recommendation: introduce `process_manager`, then migrate command sessions onto it.

Expected files/modules:

- `client/src-tauri/src/runtime/autonomous_tool_runtime/process_manager.rs`
- `client/src-tauri/src/runtime/autonomous_tool_runtime/process.rs`
- `client/src-tauri/src/runtime/autonomous_tool_runtime/mod.rs`
- `client/src-tauri/src/runtime/autonomous_tool_runtime/policy.rs`
- `client/src-tauri/src/runtime/agent_core/tool_descriptors.rs`
- `client/src-tauri/src/runtime/agent_core/tool_dispatch.rs`
- `client/src-tauri/src/runtime/agent_core/persistence.rs`
- `client/src-tauri/src/runtime/redaction.rs`

Acceptance checks:

- New schemas are documented in tool descriptors.
- Existing `command`, `command_session_start`, `command_session_read`, and `command_session_stop` behavior remains compatible.
- Existing policy tests still pass.
- New process-manager unit tests can run without starting long-lived external services.

### Phase 1: Owned Process Manager MVP

Goal: reach a safe MVP for Cadence-owned long-running process control.

This phase should only manage processes that Cadence starts itself. It should not yet list or kill arbitrary system processes.

Tool actions:

- `start`: launch a Cadence-owned process.
- `list`: list Cadence-owned processes.
- `status`: return metadata for one Cadence-owned process.
- `output`: read bounded output from one Cadence-owned process.
- `kill`: terminate one Cadence-owned process tree.

Implementation details:

- Use existing process-tree termination from `process_tree.rs`.
- Store PID, process group/job id, command, cwd, start time, owner, status, exit code, and output cursor.
- Capture stdout and stderr separately, but allow a combined chronological output view if practical.
- Keep a bounded in-memory ring buffer for recent output.
- Persist command start/end and output events using the current persistence/redaction path.
- Keep stdin closed in this phase to reduce risk.
- Require the same command approval path as `command_session_start`.
- Enforce repo-scoped cwd and path-like argument checks exactly as current command policy does.

Acceptance checks:

- A long-running process can be started, listed, read from, and killed.
- Killing a process also kills its child process tree.
- Output is redacted before persistence.
- A timed-out or cancelled start cleans up its process tree.
- Registry limits prevent runaway process creation.
- Dropping the registry or shutting down the runtime cleans up non-persistent owned processes.

Why this comes first:

This gives Cadence a safe base comparable to the smallest useful subset of GSD Pi's `bg_shell`, while keeping the blast radius inside Cadence-owned processes.

### Phase 2: Interactive Sessions and Stateful Shells

Goal: make long-running sessions genuinely interactive.

This phase closes the biggest practical gap in `command_session_start/read/stop`: no stdin and no persistent shell state.

Tool actions:

- `send`: write stdin to a process.
- `send_and_wait`: write stdin, then wait for an output regex or timeout.
- `run`: execute a command inside a managed persistent shell session.
- `env`: inspect the managed shell's current cwd and selected environment details.

Implementation details:

- Start processes with piped stdin when the caller opts into interactivity.
- Track whether stdin is open, closed, or unavailable.
- Add a persistent shell mode that starts the user's shell or an explicit shell with approval.
- For persistent shell sessions, add prompt markers or command sentinels so `run` can identify command completion and exit status.
- Do not silently switch the normal `command` tool to shell mode.
- Apply stricter approval rules to shell sessions than argv commands.
- Ensure `send` and `send_and_wait` redact sensitive input before persistence.

Acceptance checks:

- The agent can answer a CLI prompt.
- The agent can run multiple commands in one persistent shell and observe cwd/env continuity.
- `send_and_wait` times out without killing the process unless requested.
- Closing or killing a session closes stdin and terminates descendants.
- Shell sessions cannot bypass command policy for obviously destructive commands.

Why this matters:

Many real harness tasks are interactive: REPLs, package managers, login flows, CLIs that ask for confirmation, test watchers, database consoles, and dev servers with keyboard controls. GSD Pi handles this through `bg_shell`; Cadence needs an equivalent.

### Phase 3: Readiness, Output Intelligence, and Cross-Turn Awareness

Goal: make long-running processes understandable and useful across turns.

This phase turns raw process control into an agent-friendly harness.

Tool actions:

- `wait_for_ready`: wait for output regex, open port, HTTP readiness, or process exit.
- `digest`: return a concise state summary for one or all owned processes.
- `highlights`: return detected URLs, ports, warnings, errors, stack traces, and status changes.
- `output`: extend with cursor, since-last-read, tail, stream name, and filter options.

Implementation details:

- Detect local URLs and ports from output.
- Optionally probe configured ports for readiness.
- Store a per-process output cursor per agent/thread so repeated reads can fetch only new output.
- Add full-output artifacts once ring buffers truncate.
- Add lifecycle summaries to the agent context at safe boundaries: before agent start, after session resume, after compaction, and after process state changes.
- Add UI-facing process summaries without exposing temporary debug UI.

Acceptance checks:

- A dev server can be started and `wait_for_ready` returns when the port or output pattern is ready.
- `digest` gives a low-token summary of active processes.
- `output` can return only new lines since the last read.
- Errors/warnings/URLs are surfaced without rereading the full log.
- Process state survives a thread resume where persistence is enabled.

Why this matters:

GSD Pi's advantage is not only that it starts background processes. It keeps them visible to the agent. Cadence needs the same cross-turn memory so the agent does not forget that a server, watcher, or REPL is already running.

### Phase 4: Restart, Groups, and Async Jobs

Goal: match the operational ergonomics of GSD Pi's background shell and async job extensions.

Tool actions:

- `restart`: kill and relaunch a managed process with its original configuration.
- `group_status`: summarize all processes in a group.
- `group_kill`: terminate all owned processes in a group.
- `async_start`: start a bounded async job.
- `async_await`: wait for a job or any job completion.
- `async_cancel`: cancel a job.

Implementation details:

- Add explicit process groups for related work such as `dev-server`, `watcher`, `test-run`, or user-provided labels.
- Track restart count and last restart reason.
- Make async jobs separate from persistent background sessions. Async jobs are for finite work that may outlive a single tool-call timeout.
- Keep default cleanup strict: async jobs should have timeouts and should not persist across sessions unless explicitly approved.
- Add artifact-backed result storage for async job outputs.

Acceptance checks:

- A test job can run asynchronously while the agent continues with file inspection.
- The agent can wait for any completed job.
- A process group can be summarized and killed.
- Restart preserves command/cwd/env config and increments restart metadata.
- Async jobs do not leak after cancellation, timeout, or runtime shutdown.

Why this matters:

This phase gives Cadence the practical benefits of GSD Pi's `async_bash`, `await_job`, `cancel_job`, and bg-shell grouping without making every command an unstructured background shell.

### Phase 5: System Process Visibility and External Kill

Goal: let Cadence inspect and control the broader system safely.

This phase is where Cadence moves beyond owned processes. It should be approval-gated and visibly different from normal repo work.

Tool actions:

- `system_process_list`: list processes with filters.
- `system_process_tree`: inspect descendants/ancestors for a PID.
- `system_port_list`: list local listening ports and owning processes where available.
- `system_signal`: send a signal to an external process.
- `system_kill_tree`: kill an external process tree with explicit approval.

Implementation details:

- Prefer native process APIs over shell parsing.
- On macOS, use `libproc` or a vetted crate where possible.
- On Linux, use `/proc` or a vetted crate.
- On Windows, use Toolhelp/job-object/taskkill equivalents.
- Mark Cadence-owned processes distinctly from external processes.
- Require explicit approval for signaling or killing anything external.
- Log enough metadata for audit: target PID, name, executable path if available, cwd if available, parent PID, signal, and reason.

Acceptance checks:

- The agent can identify a process occupying a local port.
- The agent can inspect a process tree before killing it.
- External kill requests require approval.
- Killing an external process tree does not accidentally target Cadence itself.
- Denied or failed kills are persisted as action-required/failure events.

Why this matters:

The user explicitly called out "being able to see and kill process." Cadence currently only controls processes it owns. This phase closes that system-control gap while keeping a safety boundary.

### Phase 6: Search, Read, and Edit Parity

Goal: bring core code-navigation and file-manipulation ergonomics closer to or beyond GSD Pi.

This phase can run in parallel with later process phases if owned by a separate implementation lane.

Search work:

- Replace or supplement literal `search` with ripgrep JSON.
- Add regex/literal mode.
- Add ignore-case.
- Add include/exclude globs.
- Add context lines.
- Add hidden and gitignore controls.
- Return total matches, truncation flags, and deterministic ordering.

Read work:

- Add image-aware reads for common image formats.
- Add binary metadata reads.
- Add byte offsets for large logs.
- Add optional absolute/system path reads behind approval.
- Preserve repo-scoped text read as the default.

Edit work:

- Add hashline-style read/edit anchors.
- Preserve BOM and line endings explicitly.
- Return compact diffs from mutation tools.
- Keep expected text/hash guards and rollback checkpoints.

Acceptance checks:

- Search can find regex matches with context across a large repo.
- Search respects `.gitignore` by default and can include hidden files when requested.
- The agent can inspect a screenshot/image artifact without shelling out.
- Hash-anchored edit fails safely if the file changed.
- Mutation results include enough diff context for audit.

Why this matters:

Even with a better process harness, the coding agent's day-to-day effectiveness depends on search and edit quality. GSD Pi is currently better here, especially because it uses ripgrep and hashline workflows.

### Phase 7: macOS App/System Automation

Goal: add typed local desktop control where it is genuinely useful for a Tauri app.

Tool actions:

- `mac_permissions`: check Accessibility, Screen Recording, Automation, and related permissions.
- `mac_app_list`: list running apps with bundle id, pid, and active status.
- `mac_app_launch`: launch an app by name or bundle id.
- `mac_app_activate`: bring an app forward.
- `mac_app_quit`: quit an app.
- `mac_window_list`: list windows for an app.
- `mac_window_focus`: focus a window.
- `mac_screenshot`: capture screen/window images.

Implementation details:

- Build a small Swift helper or native Rust/macOS bridge.
- Keep permissions explicit and user-visible.
- Require approval for activating, quitting, or controlling external apps.
- Feed screenshots into the image-aware read path from Phase 6.
- Avoid adding non-user-facing debug UI.

Acceptance checks:

- The agent can tell the user which required macOS permissions are missing.
- The agent can list running apps and identify the active app.
- App quit/activate actions require approval.
- Screenshots can be captured and inspected through the harness.

Why this matters:

GSD Pi has macOS automation as a bundled extension. If Cadence is expected to control the local system, shell commands alone are a poor substitute for typed OS tools.

### Phase 8: Browser Diagnostics and Optional Harness Extensions

Goal: close remaining Pi-level diagnostic and extensibility gaps.

Browser work:

- Add console log inspection.
- Add network request/response summaries.
- Add accessibility tree inspection.
- Add trace/HAR capture if needed.
- Add visual diff and screenshot comparison if needed.
- Add state save/restore for browser sessions.

Extension work:

- Define a privileged local harness extension API.
- Let extensions register typed tools with risk metadata.
- Let extensions register lifecycle hooks.
- Let extensions persist state through approved stores.
- Require redaction and approval rules for every extension tool.

Acceptance checks:

- A browser/webview issue can be debugged from console and network logs without external tooling.
- Extension tools cannot bypass policy.
- Tool descriptors clearly identify extension-provided tools and risk.
- Extension state is cleaned up or persisted according to declared policy.

Why this comes last:

Browser and extension parity are valuable, but they are not the core gap the user called out. Process/session/system control should land first.

### Phase 9: Production Hardening and Regression Closure

Goal: turn the completed harness phases into a production-ready surface by closing the final regressions, tightening edge-case semantics, and making the acceptance gates unambiguous.

This phase should not add broad new capability. It is a stabilization pass over the features already introduced by the earlier phases. The output should be a green verification suite and a harness whose behavior is predictable under reloads, empty states, output truncation, async completion, and browser state restore.

Core work:

- Restore imported-repo runtime session reconciliation so a valid stored provider session starts and reloads as authenticated instead of falling back to idle.
- Preserve the correct agent runtime empty-state priority: setup-required states and supervised-run no-run states must not be shadowed by global provider readiness.
- Tighten process output cursor semantics so "since last read" advances only through output actually returned to the caller.
- Clean up completed async jobs after await, not only after cancellation, and make completed jobs stop counting against process limits.
- Ensure async job artifacts represent full redacted output, not only the retained in-memory output ring after truncation.
- Validate and encode browser state restore cookies before writing them, and reject malformed cookie names or values that would inject attributes.
- Confirm browser state snapshot and restore outputs do not persist or expose secrets beyond the approved redaction boundary.
- Add regression tests for each fixed behavior before considering the phase closed.

Acceptance checks:

- Imported-repo bridge reload/start-once coverage passes with a seeded authenticated provider session.
- Agent runtime live-view tests pass for signed-out setup, authenticated no-run, and promptable empty-session states.
- Process output tests prove capped, filtered, and tail reads do not skip unread chunks on the next since-last-read request.
- Async job tests prove awaited, cancelled, timed-out, and runtime-shutdown jobs are removed or finalized according to policy.
- Browser state restore tests reject malformed cookies and preserve valid cookies without attribute injection.
- Full client and Tauri verification passes: lint, build, frontend tests, Rust formatting, and Rust tests.
- The production build may keep known bundle-size warnings only if they are documented as non-blocking and unrelated to the harness changes.

Why this phase is required:

The earlier phases prove capability parity. This phase proves operational quality. A harness that can control processes, browser state, and runtime sessions must be boring under reloads and edge cases; otherwise the new power becomes a reliability risk.

## Recommended First Milestone

The first milestone should be deliberately narrow:

1. Add `process_manager` with `start`, `list`, `status`, `output`, and `kill` for Cadence-owned processes only.
2. Route process start through the existing command approval policy.
3. Use existing process-tree kill logic.
4. Store process output in a bounded ring buffer and persist redacted command events.
5. Add tests for start/list/output/kill, timeout cleanup, child-process cleanup, and registry limits.

This milestone would not yet match GSD Pi, but it would create the right foundation. After that, `send`, `send_and_wait`, and `wait_for_ready` become natural incremental additions instead of a risky rewrite.

## Bottom Line

Cadence is already more disciplined than GSD Pi in safety, audit, rollback, and policy. GSD Pi is more capable at real-world system operation. To be "as good as Pi or better", Cadence should not loosen its safety model; it should add Pi's missing operational primitives as typed, auditable, approval-gated tools.

The must-have parity item is a `bg_shell`-class process manager. Without it, Cadence will continue to feel weaker whenever work involves dev servers, test watchers, REPLs, interactive CLIs, process cleanup, or cross-turn process awareness.
