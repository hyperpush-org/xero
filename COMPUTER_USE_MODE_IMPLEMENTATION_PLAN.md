# Computer Use Mode Implementation Plan

## Reader And Post-Read Action

This plan is for the engineer implementing Xero's Computer Use mode. After reading it, the next useful action should be to start Phase 1 by adding the shared `computer_use` agent/session contracts and focused tests, then wire the existing remote session creation flow through those contracts.

This is an integration-heavy feature. Most of the required primitives already exist: built-in runtime agents, runtime controls, cloud-to-desktop relay commands, desktop-created sessions, TUI remote bridging, session lists, composers, model selection, tool policy enforcement, and macOS/browser/device automation boundaries. The work is mostly to join those pieces into one intentional product mode with clear UX and stricter safety defaults.

## North Star

A user should be able to start a special Computer Use session from the client app, cloud app, or TUI. That session should use a dedicated Computer Use agent whose job is to follow direct user instructions by observing and controlling the local computer through approved, bounded tools.

The session must feel different from normal chat/build/debug sessions:

- It is created explicitly, not accidentally selected from the regular agent picker.
- It has a dedicated Computer Use agent selected from the first message.
- It is visually denoted in sidebars, headers, and empty states.
- It has stricter safety defaults than a normal engineering agent.
- It exposes an obvious stop/pause affordance anywhere the user can drive it remotely.
- It works from the desktop client, the cloud browser app, and the TUI.

## Product Shape

Computer Use is a mode and a session kind, not just another label in the normal session list.

User-facing behavior:

- Add a "Computer Use" action next to the normal "New session" action.
- Selecting it creates a new session immediately.
- The new session is titled "Computer Use" by default unless the user supplies a prompt-specific title.
- The session opens with the Computer Use agent selected.
- The first prompt may be empty; the user can create the session first and then send instructions.
- If a prompt is provided at creation time, it starts a run under the Computer Use agent.
- The session remains a Computer Use session even if the active run ends.
- Changing models is allowed where existing controls support it.
- Switching the session's agent away from Computer Use should be disabled unless we intentionally add an escape hatch later.

Do not call the per-run gated phases "Workflow phases" in this work. Use "Computer Use session", "Computer Use agent", and "run" in UI copy.

## Existing Pieces To Reuse

Use the existing relay flow:

- The cloud app already sends `start_session`, `send_message`, and control-update commands to the owning desktop.
- The desktop and TUI already create local agent sessions in response.
- Runtime controls already include an active runtime agent id, model id, approval mode, thinking effort, and auto-compact state.
- Session snapshots already carry available agents, available models, current agent id, and current model id back to cloud.
- The cloud composer already renders agent/model controls from those snapshots.
- The desktop client already supports pending initial runtime agent selection for newly created sessions.
- The TUI already has palette commands for new sessions and sessions.
- Tool policy and approval layers already classify observe, write, destructive write, command, process, browser, and device effects.

Prefer extending these contracts over adding a parallel Computer Use transport.

## Contract Changes

Add a built-in runtime agent id:

- `computer_use`
- Label: "Computer Use"
- Short label: "Computer"
- Prompt policy: direct instruction following for computer control
- Tool policy: new `computer_use` policy or a narrowly scoped custom policy derived from existing automation policies
- Output contract: `answer`
- Default approval mode: `suggest`
- Allowed approval modes: `suggest` only for the first release
- Plan gate: disabled
- Verification gate: disabled
- Auto-compact: enabled

Add a persisted session kind:

- `standard`
- `computer_use`

This should live in app-data backed session storage. Because this is a new app and backwards compatibility is prohibited unless requested, do not add compatibility glue for stale local state during development. If old app-data rows or schemas cause problems, wipe the affected app-data state.

Expose session kind in all DTOs and relay payloads that summarize sessions:

- session create response
- session list response
- session update response
- remote visible session summary
- session snapshot
- remote session started event
- remote session added event

Relay command additions:

- `start_session` payload accepts `sessionKind: "computer_use"` and `agent: "computer_use"`.
- `send_message` payload for a Computer Use session defaults to `agent: "computer_use"` when omitted.
- `update_session_controls` rejects non-Computer Use agents for Computer Use sessions.
- TUI remote start honors the same payload fields.

## Safety Model

The first release should be intentionally conservative. Computer Use should control the computer, but not gain broad engineering powers just because it can see the screen.

Allowed by default:

- observe screen or active app state through existing approved screenshot/accessibility/browser/device observation tools
- click, type, scroll, hotkey, and navigate through bounded automation tools
- read non-secret UI text and summarize what it sees
- ask the user for confirmation when an operation is ambiguous or risky
- stop immediately when the user cancels the run

Denied by default:

- arbitrary shell commands
- package installs
- git operations
- file deletion, broad filesystem writes, or destructive writes
- process kill/restart outside a specifically approved UI action
- credential, token, key, or secret extraction
- external service mutation unless the user confirms a specific action
- persistent background automation after the session is closed
- bypassing OS permission prompts or app authorization dialogs

High-risk UI actions require explicit user confirmation before execution:

- deleting files or records
- sending messages or emails
- purchasing, publishing, transferring funds, trading, or submitting forms
- changing account, security, billing, or privacy settings
- granting permissions
- installing software
- running destructive app actions

Implementation guardrails:

- Gate Computer Use through tool policy, not just prompt text.
- Add a final policy check before dispatching any browser/device/OS automation action.
- Record policy decisions in the run event stream.
- Keep "Stop Computer Use" available in desktop, cloud, and TUI.
- Do not expose yolo mode for Computer Use in the first release.

## UX Plan

### Desktop Client

Add a Computer Use entry point near normal session creation:

- Use ShadCN controls and lucide icons.
- The normal plus button keeps creating standard sessions.
- A distinct Computer Use button or menu item creates a Computer Use session.
- The new session opens selected and starts with `computer_use` as the pending initial runtime agent.

Sidebar treatment:

- Render Computer Use sessions separately from regular sessions or with a strong distinct row treatment.
- Use a monitor/cursor-style icon, "Computer Use" badge, and a subtle accent that is not confused with selected state.
- Do not bury the distinction only in row text.
- Keep stable row height and actions.

Session view treatment:

- Header shows "Computer Use" and current safety state.
- Empty state should invite direct computer instructions, not coding/chat suggestions.
- Composer should keep model controls if available, but hide or lock the normal agent picker.
- During an active run, show a clear stop/pause action and current automation status.

### Cloud App

Add a Computer Use start action for each available desktop/project:

- If there is one project, the action can be a direct button.
- If there are multiple projects, reuse the project picker pattern with a Computer Use-specific item.
- Send `sessionKind: "computer_use"` and `agent: "computer_use"` in the existing `start_session` command.

Cloud sidebar treatment:

- Computer Use sessions should not look like normal chat sessions.
- Group them in a "Computer Use" section or render them as special rows above normal sessions.
- Include an icon/badge and a stronger active/live treatment.
- Keep archive behavior, but require the same confirmation pattern as existing rows.

Cloud session view:

- Use the existing transcript and composer.
- Lock the agent selector to Computer Use for Computer Use sessions.
- Keep model selection and thinking controls if supported by the selected model.
- Add a compact Computer Use status strip above the composer or in the top bar.
- Keep a visible stop button while live.

### TUI

Add palette entries:

- `computer-use` or `cu`: create a new Computer Use session for the active project.
- `new`: keep creating normal sessions.
- `sessions`: mark Computer Use rows with a prefix/icon-safe text label such as `[Computer Use]`.

TUI behavior:

- Creating a Computer Use session sets the active session id.
- The next prompt uses `computer_use` as the runtime agent.
- Remote TUI bridge honors cloud `start_session` payloads with `sessionKind: "computer_use"`.
- The session list and footer/status line show when the active session is Computer Use.
- Stop/cancel commands work exactly like normal runs.

## Implementation Phases

### Phase 1: Shared Contracts And Agent Descriptor

Goal: make Computer Use a first-class runtime agent and session kind everywhere local code validates contracts.

Tasks:

- Add `computer_use` to shared TypeScript runtime agent ids.
- Add `computer_use` to Rust runtime agent ids.
- Add the Computer Use descriptor in shared UI and Rust command contracts.
- Add session kind to agent session records, DTOs, model schemas, and tests.
- Set default session kind to `standard`.
- Add create-session request support for optional session kind.
- Add validation that Computer Use sessions only default to the Computer Use agent.

Acceptance criteria:

- Shared runtime-agent tests accept `computer_use`.
- Rust contract tests serialize and deserialize `computer_use`.
- Session create/list tests include `sessionKind`.
- Existing standard sessions still create as `standard` in fresh app-data.

### Phase 2: Runtime Policy

Goal: make the Computer Use agent safe before any UI exposes it.

Tasks:

- Add a Computer Use prompt fragment that focuses on direct computer control and user confirmation.
- Add a Computer Use tool policy.
- Allow only observation and bounded browser/device/OS automation tools needed for visible computer control.
- Deny arbitrary command, destructive write, package, git, and broad filesystem tools.
- Add policy tests for allowed observe/control actions and denied destructive actions.
- Add event assertions for denied tool calls.

Acceptance criteria:

- Computer Use can observe and perform bounded UI automation in tests/harnesses.
- Computer Use cannot dispatch shell, file deletion, destructive write, or broad process-control tools.
- Approval mode is locked to `suggest`.

### Phase 3: Local Session Creation Plumbing

Goal: let desktop and TUI create Computer Use sessions through existing session creation paths.

Tasks:

- Add optional `sessionKind` and `runtimeAgentId` or `agent` fields to create-session commands where needed.
- When creating a Computer Use session, title it "Computer Use" by default.
- Store `sessionKind`.
- Select the new session.
- Seed pending initial runtime agent to `computer_use`.
- Ensure the first run starts with Computer Use even when the prompt is sent immediately.

Acceptance criteria:

- Desktop client can create a Computer Use session without a prompt.
- Desktop client can create a Computer Use session and immediately send a prompt.
- TUI can create and activate a Computer Use session.
- Normal session creation remains unchanged.

### Phase 4: Relay And Cloud Plumbing

Goal: make cloud-created Computer Use sessions land on the owning desktop or TUI with the right kind and agent.

Tasks:

- Extend cloud `start_session` requests to include `sessionKind`.
- Extend desktop remote command handling to read `sessionKind` and `agent`.
- Extend TUI remote command handling to read `sessionKind` and `agent`.
- Include `sessionKind` in remote session started, added, list, and snapshot payloads.
- Update cloud store equality/sorting to account for `sessionKind`.
- Default cloud sends for a Computer Use session to `agent: "computer_use"`.

Acceptance criteria:

- Cloud can create a Computer Use session on an online desktop.
- Cloud can create a Computer Use session through the TUI remote bridge.
- Cloud sidebars immediately show the created session as Computer Use.
- Sending the first message from cloud uses the Computer Use agent.

### Phase 5: Desktop UX

Goal: make the mode discoverable and visually distinct in the client app.

Tasks:

- Add a Computer Use create action beside normal session creation.
- Add special sidebar row styling for Computer Use sessions.
- Add a Computer Use session header/status strip.
- Lock or hide the agent picker in Computer Use sessions.
- Replace generic empty-state suggestions with direct computer-control prompts.
- Show stop/pause controls while live.

Acceptance criteria:

- A user can identify Computer Use sessions at a glance in the client sidebar.
- A user cannot accidentally switch a Computer Use session to Engineer/Agent through the composer.
- The mode uses only user-facing UI, no temporary debug/test UI.

### Phase 6: Cloud UX

Goal: give the browser app a clear "drive this computer" experience.

Tasks:

- Add Computer Use to the new-session picker or project group actions.
- Add a dedicated Computer Use section or distinct row component in the cloud sidebar.
- Add Computer Use badge/status in the session view.
- Lock the agent picker for Computer Use sessions.
- Show a prominent stop button while live.
- Add tests for mobile drawer and desktop sidebar variants.

Acceptance criteria:

- Cloud Computer Use sessions do not look like normal sessions.
- The user can start Computer Use from cloud without first creating a normal session.
- The UI remains usable on mobile and desktop.

### Phase 7: TUI UX

Goal: make Computer Use usable from terminal workflows without a graphical sidebar.

Tasks:

- Add `computer-use` palette entry.
- Mark Computer Use rows in the sessions palette.
- Show active Computer Use status in footer or session context.
- Ensure remote-created Computer Use sessions appear with the special marker.
- Add focused TUI tests for palette creation and session row labeling.

Acceptance criteria:

- TUI users can create, select, send prompts to, and cancel Computer Use sessions.
- Computer Use sessions are visibly distinct in terminal lists.

### Phase 8: End-To-End Verification

Goal: prove the feature works across client, cloud, and TUI without broad test runs.

Focused verification:

- Shared TypeScript model tests for `computer_use` and `sessionKind`.
- Rust contract and session-store tests for session kind persistence.
- Runtime policy tests for allowed and denied tools.
- Cloud relay/store tests for `sessionKind` propagation.
- Cloud component tests for special rows and creation actions.
- Desktop component tests for sidebar and composer locking.
- TUI palette tests for Computer Use creation and display.
- One manual local run in the Tauri app if available, but do not try to open it in a browser.

Because this is a Tauri app, do not use browser-based local app verification for the client. Use unit/e2e tests and Tauri-capable manual verification only.

## Open Decisions

- Exact Computer Use icon and accent color.
- Whether Computer Use sessions should be grouped above normal sessions or interleaved with a special row.
- Whether the cloud start action should be a separate button or a split menu under the plus action.
- Whether Computer Use can ever switch back to a standard session agent after creation.
- Which existing OS automation tools are safe enough for first-release enablement.
- Whether high-risk UI actions use the existing operator action system or a new Computer Use confirmation copy layer.

## Non-Goals For First Release

- A new cloud-hosted runtime.
- A new remote transport.
- A general-purpose RPA scheduler.
- Background automation after the user leaves the Computer Use session.
- Unrestricted shell or filesystem control.
- A new Workflow feature.
- Backwards-compatible handling for stale app-data schemas.

## Reader Test

A fresh engineer should be able to start with Phase 1, add the shared `computer_use` and `sessionKind` contracts, and then proceed vertically through runtime policy, session creation, relay propagation, and UX. The key implementation choice is to reuse the existing session/run/relay machinery and distinguish Computer Use through session kind, runtime agent, policy, and UI treatment rather than building a separate system.
