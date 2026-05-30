# Computer Use System Control Audit And Expansion Plan

## Reader And Goal

Reader: an internal engineer expanding Xero Computer Use agent control.

Post-read action: implement the missing agent-visible tools and platform capabilities needed for the user to control their own macOS or Windows machine by prompting, with explicit local consent, visible control state, auditability, and no attempts to bypass OS security boundaries.

## Audit Conclusion

Computer Use already has a real agent-control foundation. It can use ordinary agent tools for project inspection, file changes, commands, browser control, diagnostics, skills, subagents, durable context, and external capabilities. It also has a native desktop-control foundation: observe the desktop, send pointer and keyboard input, launch or target apps, manage a desktop stream, and audit control actions.

This plan is not mainly about manual WebRTC control. Manual streaming and human viewport control are one client of the desktop broker. The main target is LLM-driven Computer Use: the agent should have the right prompt-visible tools, schemas, permissions, policy gates, and platform backends to operate the user's computer as completely as the user explicitly allows.

macOS is substantially deeper than Windows because it has Accessibility, Vision OCR, ScreenCaptureKit WebRTC streaming, and app/window automation fallbacks. Windows currently has useful pointer, keyboard, clipboard, screenshot, window, and app basics, but it lacks structured UI Automation, OCR, and native WebRTC video publishing.

The current surface is not "absolute full system control" yet. Prompted desktop control is still bounded by:

- OS permissions such as Screen Recording, Accessibility, Input Monitoring, Windows session access, and UAC.
- Xero policy gates that deny secret text, credential managers, payments, financial flows, identity verification, MFA/recovery flows, and system privacy/security settings.
- Repo-scoped command tools rather than a general host-administration shell.
- Agent-visible schemas that omit a few lower-level actions already implemented in the runtime.
- Tool activation and effective-policy gaps where the Computer Use agent can theoretically use a capability, but it is not present, clear, or available in the current turn.

The right expansion path is to add an explicit owner-approved power mode, close the prompt-visible schema gaps, bring Windows to parity with macOS structured control, then add audited host-administration tools where the local user grants the privilege.

## Scope: Agent-Driven Control

The product goal is not only "let a remote human click the streamed desktop." It is "let the Computer Use agent complete computer tasks by choosing the best tool."

That means the audit must cover four layers:

- Agent policy: whether Computer Use is allowed to use a tool at all.
- Tool activation: whether the tool is included in the current turn or discoverable through tool access.
- Tool schema: whether the LLM can see every action and field it needs.
- Platform backend: whether macOS and Windows can actually execute the requested action.

Manual WebRTC control belongs mostly to the platform backend and streaming layers. LLM-driven control also needs browser tools, file tools, command tools, process tools, system diagnostics, MCP tools, skills, subagents, project context, and future host-administration tools.

## Current Computer Use Agent Tool Surface

Computer Use is defined as a general-purpose runtime agent. Its base prompt says it may combine computer interaction, project inspection, file changes, commands, browser and desktop automation, diagnostics, external-capability tools, skills, subagents, and durable context when those tools are available and appropriate.

The runtime agent gate broadly allows Computer Use to use the available autonomous tool catalog. The practical availability still depends on per-run tool policy, tool-access activation, feature rollout flags, provider schema limits, and platform support.

### Agent Tools Already Covered

- Repository and workspace observation: read, read-many, stat, search, find, list, tree listing, directory digest, file hash, git status, git diff, workspace index, code intelligence, and LSP.
- Repository mutation: edit, write, patch, copy, structured file transactions, JSON/TOML/YAML edit, delete, rename, mkdir, and notebook edit where available.
- Command and process control: command probe, command verify, command run, command session, and process manager.
- Browser/web control: browser observe, browser control, web fetch, and web search where enabled.
- Desktop control: macOS automation, desktop observe, desktop control, and desktop stream.
- System diagnostics: observe diagnostics and approval-gated privileged diagnostics.
- External extension surfaces: MCP list/read/get/call, dynamic MCP tools, skills, and subagents.
- Coordination and memory: todo, project context search/get/record/update/refresh, environment context, and agent coordination.
- Domain tools: emulator and Solana tools when the selected app/tool profile exposes them.

### Agent Tool Gaps

- There is no explicit capability matrix that says which Computer Use tools are active by default, available through tool-access request, blocked by policy, blocked by rollout, or blocked by platform.
- The current desktop-control audit covers native desktop tools more thoroughly than non-desktop agent tools.
- Repo-scoped command tools are powerful for development tasks, but they are not a complete host-administration surface.
- PowerShell appears as a tool constant/category, but the plan must verify whether it is fully agent-visible, cross-platform, policy-gated correctly, and sufficient for Windows administration.
- Browser control, desktop control, command tools, and host administration need clearer routing rules so the agent chooses the most precise safe tool instead of falling back to pixel control.
- Dynamic MCP, skills, and subagents can extend control, but there is no required "Computer Use workstation pack" that guarantees the expected macOS and Windows capabilities are installed.

## Current Desktop Tool Surface

### Generic Desktop Observation

The `desktop_observe` tool is agent-visible and exposes:

- `permissions_status`
- `display_list`
- `window_list`
- `app_list`
- `foreground_state`
- `screenshot`
- `cursor_state`
- `accessibility_snapshot`
- `ocr_snapshot`
- `element_at_point`
- `health`

It supports display targeting, window targeting, screenshot regions, and coordinates for element lookup.

### Generic Desktop Control

The `desktop_control` tool is agent-visible and exposes:

- Pointer: `mouse_move`, `mouse_click`, `mouse_double_click`, `mouse_right_click`, `mouse_drag`, `scroll`
- Keyboard and text: `key_press`, `hotkey`, `type_text`, `paste_text`
- App/window: `focus_window`, `activate_app`, `launch_app`, `quit_app`
- Structured UI: `ax_press`, `ax_set_value`, `ax_focus`, `menu_select`
- Safety/control: `cancel_current_action`

The runtime also supports `mouse_down`, `mouse_drag_move`, and `mouse_up`, but those are not in the current agent-visible schema. They appear intended for lower-level/manual-control paths.

### Desktop Streaming

The `desktop_stream` tool is agent-visible and exposes:

- `stream_capabilities`
- `stream_start`
- `stream_stop`
- `stream_status`
- `stream_set_quality`
- `stream_request_keyframe`

The runtime and sidecar also support WebRTC offer, answer, and ICE candidate operations, but those signaling operations are not currently in the agent-visible schema.

### macOS-Specific Automation

The `macos_automation` tool exposes:

- `mac_permissions`
- `mac_app_list`
- `mac_app_launch`
- `mac_app_activate`
- `mac_app_quit`
- `mac_window_list`
- `mac_window_focus`
- `mac_screenshot`

It is macOS-only. App quit requires operator approval. On macOS, generic desktop app/window actions can fall back through this path when the sidecar does not implement those operations directly.

## macOS Capability Matrix

### macOS: Present

- Permission status for Screen Recording, Accessibility, and Input Monitoring.
- Display, window, app, and foreground-state observation.
- Full-display and region screenshots.
- Cursor position.
- Accessibility snapshot and element-at-point through macOS Accessibility.
- OCR snapshot through macOS Vision.
- Mouse move, down, up, click, double-click, right-click, drag, drag-move, and scroll in the runtime/sidecar.
- Key press, hotkey, and typed text.
- Clipboard-backed paste through the sidecar.
- Accessibility actions: press, set value, focus.
- Menu path selection through Accessibility.
- App launch, activation, quit, and window focus through macOS app automation.
- Native WebRTC desktop stream using ScreenCaptureKit and VideoToolbox H.264.
- Screenshot fallback stream state.
- Controller lock, local-user takeover detection, audit log, sidecar token auth, and sidecar integrity checks.

### macOS: Missing Or Weak

- Agent-visible schema does not expose `mouse_down`, `mouse_up`, `mouse_drag_move`, `sourceWidth`, or `sourceHeight`.
- Agent-visible stream schema does not expose WebRTC signaling actions, even though the runtime supports them.
- Accessibility control is narrow: press, set value, focus, and menu select only. It lacks common AX actions such as select, confirm, cancel, increment/decrement, expand/collapse, scroll-to-visible, table/list row selection, checkbox/radio state changes, and robust text-field editing helpers.
- Element identity is not yet a durable targeting contract. A prompt can use snapshots, but stable element references across app refreshes need stronger IDs and re-resolution.
- Keyboard input depends on a fixed key map. International layouts, dead keys, IME text, media keys, function layers, and secure input mode need explicit handling.
- Clipboard is write/paste oriented. There is no general read/write clipboard API for text, images, files, or rich formats.
- No first-class Dock, menu bar extra, Mission Control, Spaces, notification, file dialog, open/save panel, or drag-and-drop file automation.
- No general system-administration bridge for host-level operations outside the repo sandbox.
- No privileged control for settings, network, display, audio, services, package managers, login items, or app install/uninstall beyond what can be clicked manually.
- No supported path to bypass TCC, SIP, secure input, password prompts, or other OS-protected boundaries. Those must remain user-mediated.

## Windows Capability Matrix

### Windows: Present

- Display, window, app, and foreground-state observation through `xcap`.
- Full-display and region screenshots through `xcap`.
- Cursor state through the sidecar input backend.
- Mouse move, down, up, click, double-click, right-click, drag, drag-move, and scroll through Enigo.
- Key press, hotkey, and typed text through Enigo.
- Clipboard-backed paste through `arboard` plus Ctrl+V.
- Window focus and app activation through Win32 calls wrapped in PowerShell.
- App launch through PowerShell and the Windows AppsFolder shell namespace.
- App quit through `taskkill.exe`.
- Static permissions model that treats screen capture and desktop input as granted in the active user session.
- Screenshot fallback stream state.
- Controller lock and audit flow shared with macOS.

### Windows: Missing Or Weak

- No Windows UI Automation tree, element-at-point implementation, or structured element actions.
- No OCR snapshot implementation.
- No menu selection implementation.
- No native WebRTC desktop video publisher. The current native publisher is macOS-only; Windows falls back to screenshot-based degraded mode.
- Window management is basic: focus/activate/launch/quit only. There is no maximize, minimize, restore, move, resize, close-window message, virtual desktop, monitor move, z-order control, or owner-drawn app targeting contract.
- App launching is heuristic. It does not yet model Store apps, protocol handlers, file associations, shell verbs, admin launches, or installed-app inventory robustly.
- UAC and secure desktop are not controllable. Elevated actions require explicit user approval through the OS.
- No Registry, Services, Task Scheduler, Event Log, Windows Settings, firewall, network adapter, winget, MSI, process privilege, or local user/group management tool.
- No RDP/session switching, lock screen, credential provider, session 0, or secure-input support.
- No deep browser/Office/Explorer-specific structured automation beyond generic screen and input.

## Cross-Platform Policy And Safety Boundaries

Current desktop-control policy intentionally blocks:

- Secret text typing or pasting.
- Password managers, Keychain, credentials, passkeys, and saved-password contexts.
- Purchases, checkout, payment confirmation, money transfer, and card entry.
- Banking, brokerage, tax, payroll, insurance, crypto, and wallet contexts.
- Identity verification, KYC, passport, driver license, SSN, and account ownership flows.
- MFA, TOTP, OTP, recovery codes, account recovery, password reset, and security settings.
- System privacy and security settings.

These boundaries conflict with literal "absolute full control," but they are the right default for a shipped product. Expansion should add an explicit local owner/admin mode rather than silently weakening default Computer Use policy.

## Expansion Plan

### Phase 0: Agent Tool Access Manifest

Goal: make Computer Use's LLM-accessible tool surface explicit before expanding any backend.

Tasks:

- Generate or maintain a Computer Use capability manifest that lists every tool, action, schema field, risk class, default availability, tool-access availability, rollout gate, platform gate, and approval requirement.
- Add tests that fail when the runtime action enum, agent-visible schema, tool catalog metadata, TypeScript DTOs, and provider schema projection drift apart.
- Verify that Computer Use can activate the expected non-desktop tool families: repository read/write, command/session/process, browser observe/control, web fetch/search, diagnostics, MCP, skill, subagent, todo, project context, environment context, code intelligence, and domain tools.
- Add a "best tool selection" guide to the Computer Use prompt or tool descriptions: prefer structured browser tools for browser tasks, command/process tools for shellable tasks, native desktop structured actions for app UI, and pixel input only when no more precise tool exists.
- Add a visible status/debug surface for developers that explains why a tool is missing in a run: policy, provider limit, rollout flag, platform unsupported, permission denied, not activated, or not installed.
- Define the required "workstation control pack" for macOS and Windows: desktop sidecar, browser control, host command/admin tools, clipboard/file-drop tools, OCR/UI tree support, and diagnostics.

Acceptance criteria:

- An engineer can answer "what can the Computer Use agent do on this machine right now?" from one manifest.
- A prompt-visible Computer Use tool cannot silently disappear without a failing test or visible availability reason.
- Manual WebRTC controls and LLM-driven tools are documented as separate surfaces sharing some desktop broker backends.

### Phase 1: Make Implemented Actions Prompt-Visible

Goal: expose the actions that already exist in the runtime where doing so improves prompted control.

Tasks:

- Add `mouse_down`, `mouse_up`, and `mouse_drag_move` to the `desktop_control` agent schema, tool catalog metadata, TypeScript DTOs, tests, and provider schema tests.
- Add `sourceWidth` and `sourceHeight` to the `desktop_control` agent schema so prompted actions can target screenshots or streams rendered at non-native sizes.
- Decide whether stream signaling remains internal or becomes prompt-visible. If prompt-visible, expose `stream_offer`, `stream_answer`, and `stream_ice_candidate` with strict SDP/ICE validation.
- Add focused runtime tests proving the schema and action enum stay aligned.
- Update the manual-control drag plan if the frontend still lacks the stateful gesture path.

Acceptance criteria:

- Every runtime desktop action intentionally available to agents is present in the tool schema.
- Hidden/internal actions are documented as internal, not accidentally omitted.
- Pointer press-hold-release can be driven by prompt when the local policy allows it.

### Phase 2: Windows Structured UI Parity

Goal: make Windows controllable by elements, not only pixels.

Tasks:

- Add a Windows UI Automation backend in the sidecar.
- Implement `accessibility_snapshot` from UIA trees with role, name, value, state, bounds, enabled/focused, and provider diagnostics.
- Implement `element_at_point` through UIA hit testing.
- Implement `ax_press`, `ax_set_value`, and `ax_focus` through UIA patterns such as Invoke, Value, Text, SelectionItem, Toggle, ExpandCollapse, and ScrollItem.
- Implement `menu_select` through UIA menu traversal where possible, with a fallback to Alt-key menu navigation.
- Add stable element handles that can be re-resolved from snapshot rows without trusting stale raw handles.
- Add Windows-specific tests behind target cfg gates and sidecar contract tests with mocked UIA payloads.

Acceptance criteria:

- A prompt can inspect a Windows app's UI tree, select a visible element, and invoke or edit it without coordinate clicking when UIA supports it.
- Unsupported controls return actionable diagnostics instead of falling back silently.
- The existing pointer/keyboard path remains available when UIA is blocked or absent.

### Phase 3: Windows OCR And Native Streaming

Goal: make Windows observation comparable to macOS for visual and remote-control tasks.

Tasks:

- Implement OCR with Windows.Media.Ocr where available, with a fallback strategy for unsupported Windows versions.
- Return OCR blocks with text, bounds, confidence, full text, truncation, and diagnostics matching the current sidecar contract.
- Implement native WebRTC publishing on Windows with Windows Graphics Capture plus Media Foundation or a proven H.264 encoder path.
- Preserve screenshot fallback as degraded mode, but avoid sharing a rate budget with critical input commands.
- Add GPU/driver failure diagnostics and software fallback messaging.

Acceptance criteria:

- Windows can return OCR snapshots from display/region captures.
- Windows can publish a native WebRTC desktop video track with cursor inclusion where supported.
- Fallback behavior is explicit and does not mask input failures.

### Phase 4: macOS Structured Control Completion

Goal: expand macOS Accessibility from basic actions to a practical app-control API.

Tasks:

- Add AX action support for select, confirm/cancel, increment/decrement, expand/collapse, scroll-to-visible, list/table row selection, checkbox/radio toggles, and text range editing.
- Add menu bar, Dock, status item, open/save panel, and file dialog helpers.
- Add element re-resolution by app, window, role, title, bounds, and ancestry path.
- Add keyboard-layout-aware key translation and a paste-first text path for long or non-US text.
- Add clipboard read/write support for text, images, files, and common rich payloads with redaction controls.

Acceptance criteria:

- A prompt can operate ordinary macOS forms, menus, dialogs, lists, tables, and toggles without relying only on screen coordinates.
- Text input works across keyboard layouts and long text.
- Sensitive clipboard payloads are never persisted in audit logs.

### Phase 5: Owner-Approved Host Administration Mode

Goal: provide as much whole-system control as possible for an explicitly consenting local owner without making default Computer Use unsafe.

Tasks:

- Add a "Power User" or "Owner Admin" mode in desktop-control settings. It must be local-only, time-bound, visibly active, and revocable with emergency stop.
- Add policy profiles: default safe, developer workstation, and owner admin.
- Add an audited host command tool separate from repo-scoped commands. It should require explicit mode enablement and support shell, PowerShell, AppleScript/JXA, Shortcuts, `brew`, `winget`, service management, registry edits, package install/uninstall, app launch arguments, and host file operations.
- Require per-command preview for destructive, privileged, network/security, startup-item, credential-adjacent, or privacy-sensitive operations.
- Use OS-native elevation prompts when needed. Do not attempt to bypass UAC, TCC, SIP, secure desktop, or credential prompts.
- Add rollback metadata where practical: changed files, registry keys, service state, package operations, and settings snapshots.

Acceptance criteria:

- A local owner can prompt Xero to perform broad workstation administration after enabling owner-admin mode.
- The user can see, stop, and audit all high-impact actions.
- Xero never claims it can bypass OS protections; it asks the user to approve OS prompts when required.

### Phase 6: Desktop Resource Surfaces

Goal: cover common real desktop work that is not just clicking.

Tasks:

- Add clipboard read/write actions for text, images, files, and common rich formats.
- Add file drag/drop synthesis for apps that expect dropped files.
- Add notification observation where platform APIs permit it.
- Add audio volume, media keys, display arrangement/readout, and window layout helpers.
- Add app inventory and launch-target discovery on both platforms.
- Add browser and terminal bridge affordances that hand off to existing browser/control/command tools when they are more precise than desktop pixels.

Acceptance criteria:

- Computer Use can complete routine app workflows involving files, clipboard, dialogs, notifications, and window layout.
- The agent prefers structured/native actions over coordinate input when available.

### Phase 7: Verification Matrix

Goal: make expanded control dependable across real machines.

Tasks:

- Add a capability-report fixture per platform that records sidecar capabilities, permission states, and known unavailable surfaces.
- Test macOS with Screen Recording denied/granted, Accessibility denied/granted, Input Monitoring denied/granted, multiple displays, Retina scaling, and secure input.
- Test Windows with standard user, administrator, UAC prompt, multiple DPI scales, multiple monitors, RDP, Store app, classic Win32 app, Electron app, browser, Explorer, and Office-style apps.
- Add failure-mode tests for sidecar unavailable, sidecar operation unimplemented, screenshot capture denied, UIA unavailable, OCR unavailable, stream start failure, and local-user takeover.
- Keep Cargo tests scoped and run one Cargo command at a time.

Acceptance criteria:

- The app can report exactly what a machine supports before the agent acts.
- Platform-specific failures produce user-actionable messages.
- High-risk actions are covered by approval, audit, and emergency-stop tests.

## Final Target State

The practical target is not stealthy or permissionless control. The target is consented, visible, owner-controlled workstation automation:

- Observe the screen, UI tree, OCR text, windows, apps, cursor, permissions, and stream state.
- Choose among browser, command, file, process, diagnostics, MCP, skill, subagent, and desktop tools based on the task.
- Control pointer, keyboard, text, clipboard, menus, dialogs, windows, apps, and structured UI elements.
- Stream the desktop reliably on macOS and Windows.
- Administer the host through an explicit owner-approved tool surface.
- Preserve audit logs, leases, emergency stop, and policy profiles.
- Respect OS security prompts and refuse to bypass protected boundaries.
