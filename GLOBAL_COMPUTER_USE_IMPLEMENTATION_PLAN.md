# Global Computer Use Implementation Plan

## Reader And Post-Read Action

This plan is for the engineer replacing project-scoped Computer Use sessions with a global Computer Use surface. After reading it, the next useful action should be to implement the shared global Computer Use session contract and then update the desktop, TUI, and cloud entry points to use it.

The important product correction is that Computer Use is app functionality, not a project feature. A user should not decide which project Computer Use belongs to, should not create multiple Computer Use sessions across projects, and should not manage Computer Use context or rollover. When a desktop client or TUI is online, Computer Use should simply be available.

## North Star

Computer Use is a single global, always-on capability per running computer.

User-facing behavior:

- The desktop client has a dedicated Computer Use button in the app titlebar.
- That button opens a right sidebar rather than creating or selecting a project session.
- The cloud app shows Computer Use as an available capability for each online desktop or TUI, independent of projects.
- The backing session is hidden from normal project session lists.
- The backing session title updates from prompts like normal agent sessions.
- Context handoff or rollover happens behind the scenes before context pressure becomes a user concern.
- The runtime root is app-owned state, not a project repository.

Computer Use should feel like "use the visible computer" rather than "run an agent in this repo."

## Product Invariants

- There is at most one visible Computer Use surface per online computer.
- Normal project session creation never creates a Computer Use session.
- Computer Use does not appear in project session lists, project pickers, or project-scoped new-session actions.
- Computer Use is available even if the desktop or TUI has zero projects registered.
- Cloud availability comes from online device presence plus desktop/TUI capability, not from visible project sessions.
- The user can close the Computer Use sidebar without deleting or archiving the backing session.
- The user can stop an active Computer Use run from desktop, cloud, and TUI.
- Computer Use stays locked to the Computer Use agent and safety policy.

## Backing Session Model

Use a hidden global backing session for each desktop/TUI device.

Recommended shape:

- Stable global session id, derived from the local app instance rather than a project.
- App-data runtime root dedicated to Computer Use.
- Hidden session storage separated from project repositories.
- Transcript and run records still use the existing runtime/event machinery where possible.
- Session title starts as "Computer Use" and auto-renames after meaningful prompts using the existing title update behavior.
- The UI treats the hidden backing session as one continuous Computer Use conversation.

The implementation may use the existing agent handoff/lineage machinery or automatic rollover. The user-facing requirement is stronger than the internal mechanism: when context approaches exhaustion, create or switch to a continuation behind the scenes and keep the Computer Use surface continuous.

## Runtime Root

Computer Use should not start from a project root.

Use an app-owned workspace root such as a Computer Use directory under OS app data. This root should be stable, private to Xero, and safe for runtime artifacts, traces, temporary files, staged attachments, and context handoff records.

The runtime should still know that Computer Use may inspect and operate the visible desktop. Its filesystem and shell powers should remain restricted by Computer Use policy rather than inheriting project-agent capabilities.

## Desktop Client UX

Remove project-scoped entry points:

- Remove the Computer Use button from the Agent header and project session controls.
- Remove Computer Use from normal agent creation flows.
- Hide existing Computer Use backing sessions from regular project session sidebars.

Add a titlebar entry point:

- Add a dedicated Computer Use button in the client app titlebar.
- Use the existing icon language for computer/display control.
- The button opens a right sidebar.
- If an active Computer Use run exists, the button should make that state apparent without becoming noisy.

Right sidebar behavior:

- Reuse the existing agent sidebar shell where practical.
- Support a Computer Use mode in the shared sidebar component rather than duplicating the whole surface.
- Header contains only the current session title and a close button.
- No project breadcrumb.
- No new-session control.
- No archive control.
- No agent selector in the header.
- The main body uses the existing transcript and empty-state patterns, with Computer Use-specific copy and prompts.

Composer behavior in Computer Use mode:

- Hide the agent selector.
- Hide the compact/context control.
- Hide the voice button.
- Hide the context remaining indicator.
- Keep the prompt input and send button.
- Keep model/thinking controls only if they are valid for Computer Use and do not imply project-agent behavior.
- Keep stop/cancel visible while a run is active.

## Cloud UX

Cloud should show Computer Use as global desktop functionality.

Session directory behavior:

- Show Computer Use for every online desktop/TUI that advertises the capability.
- Do not nest Computer Use under a project.
- Do not require project selection to open or send a Computer Use prompt.
- Do not show Computer Use in project new-session menus.
- Keep normal project sessions grouped by project as they are today.

Cloud Computer Use view:

- Route to a device-scoped Computer Use surface rather than a project-scoped session route where possible.
- Attach to the hidden global backing session behind the scenes.
- Show a Computer Use title and badge/icon treatment.
- Keep the composer simplified in the same way as desktop Computer Use mode.
- Keep stop/cancel visible while a run is active.

Remote availability:

- Desktop/TUI publishes Computer Use capability on bridge startup and reconnect.
- Cloud reconciles capabilities periodically, the same way it reconciles sessions.
- A bad project registry entry must not affect Computer Use availability.
- A desktop with no projects should still make Computer Use available.

## TUI Behavior

The TUI should advertise Computer Use availability whenever it is signed in and connected to the relay.

Expected behavior:

- Computer Use is not tied to the active project.
- A palette command can open or focus the Computer Use surface.
- Remote cloud requests attach to the global backing session.
- The status/footer can indicate an active Computer Use run.
- Stop/cancel uses the same run cancellation path as other agents.

The TUI does not need to mimic the desktop sidebar. It only needs to share the same global backing session and remote capability contract.

## Relay And Capability Contract

Add an explicit Computer Use capability distinct from visible project sessions.

Recommended contract:

- Device capability payload includes Computer Use availability.
- Capability summary includes the device id, display name, online status, and current Computer Use title.
- Existing visible project session summaries exclude hidden Computer Use backing sessions.
- Existing project list summaries remain project-only.
- Computer Use attach/start/send commands do not require project id.
- Desktop/TUI resolves Computer Use commands to the global backing session.

Keep compatibility with the existing authenticated relay model. Do not create a second transport unless a concrete limitation appears.

## Context Rollover

The user should not manage Computer Use context.

Implementation options:

- Use hidden handoff when context pressure gets high, then continue in a fresh backing run/session.
- Or keep a stable visible global session that internally chains multiple runtime sessions.

Acceptance behavior:

- The UI continues to look like one Computer Use conversation.
- Relevant prior context remains available after rollover.
- The title remains user-facing and stable.
- Rollover emits durable events for audit/debugging, but does not surface as "new session created" to the user.
- Failed rollover becomes a recoverable error in the Computer Use transcript, not a silent spinner.

## Safety Model

Keep the conservative Computer Use policy from the first implementation and apply it to the global surface.

Still denied by default:

- Arbitrary shell commands.
- Package installs.
- Git operations.
- Broad filesystem writes or deletion.
- Credential, token, key, or secret extraction.
- External service mutation without explicit confirmation.
- Persistent background automation after the user closes/stops Computer Use.

Still require explicit confirmation for high-risk UI actions:

- Sending messages or emails.
- Purchasing, publishing, transferring money, or submitting important forms.
- Deleting files or records.
- Changing billing, security, privacy, or account settings.
- Granting permissions.
- Installing software.

The move to global Computer Use must not weaken policy just because the session is outside a project.

## Implementation Phases

### Phase 1: Global Contract

Goal: make Computer Use addressable without a project.

Tasks:

- Define the hidden global Computer Use session identity.
- Define the app-owned runtime root.
- Add a device capability payload for Computer Use availability.
- Ensure visible project session summaries exclude hidden Computer Use sessions.
- Ensure Computer Use commands do not require project id.

Acceptance criteria:

- A desktop/TUI with no projects can advertise Computer Use.
- A desktop/TUI with multiple projects advertises exactly one Computer Use capability.
- Existing project sessions still list normally.

### Phase 2: Desktop Sidebar UX

Goal: replace project-scoped Computer Use creation with a dedicated titlebar sidebar.

Tasks:

- Remove the Agent header Computer Use creation button.
- Add the titlebar Computer Use button.
- Refactor the agent sidebar shell to support normal agent mode and Computer Use mode.
- Add the simplified Computer Use header.
- Hide disallowed composer controls in Computer Use mode.
- Wire prompt sending to the hidden global backing session.

Acceptance criteria:

- Clicking the titlebar button opens the right sidebar.
- No project session is created by opening the sidebar.
- The composer omits agent select, compact/context control, voice button, and context remaining indicator.
- Prompting Computer Use starts or continues the hidden backing session.

### Phase 3: Cloud UX

Goal: make Computer Use visibly global in cloud.

Tasks:

- Render Computer Use from device capability state.
- Remove Computer Use from project new-session flows.
- Open Computer Use using a device-scoped route or equivalent device-scoped state.
- Attach/send through the global Computer Use command path.
- Match the simplified composer affordances.

Acceptance criteria:

- Cloud shows Computer Use when the desktop/TUI is online, even with no projects.
- Cloud does not show Computer Use under each project.
- Sending a prompt does not require project selection.
- The hidden backing session loads without an infinite spinner.

### Phase 4: TUI Support

Goal: keep TUI parity for remote Computer Use availability.

Tasks:

- Advertise Computer Use capability when the TUI relay bridge is online.
- Add or update command/palette behavior to focus the global Computer Use surface.
- Route remote Computer Use commands to the global backing session.
- Keep stop/cancel support.

Acceptance criteria:

- Cloud sees Computer Use when only the TUI is connected.
- The TUI does not require an active project for Computer Use.
- Active runs can be stopped.

### Phase 5: Context Rollover

Goal: hide context/session management from users.

Tasks:

- Detect context pressure for Computer Use runs.
- Create a hidden continuation before context exhaustion.
- Preserve relevant context through handoff or summary.
- Keep the visible Computer Use surface continuous.
- Add recovery behavior for rollover failures.

Acceptance criteria:

- Long Computer Use interactions continue without asking the user to start a new session.
- The UI does not expose continuation session ids.
- Failures render as actionable transcript errors.

### Phase 6: Cleanup And Migration

Goal: remove project-scoped Computer Use behavior from the product.

Tasks:

- Remove project-specific Computer Use rows from normal lists.
- Remove project-scoped Computer Use creation actions.
- Wipe or ignore stale development app-data state as needed.
- Update tests and docs that assume Computer Use is project-scoped.

Acceptance criteria:

- A new Computer Use session cannot be created per project from UI.
- Old project-scoped Computer Use entries do not appear in normal project session lists.
- Normal standard sessions are unaffected.

## Test Plan

Desktop/client tests:

- Titlebar Computer Use button opens the right sidebar.
- Agent header no longer has a Computer Use creation button.
- Computer Use sidebar header only shows title and close.
- Computer Use composer hides disallowed controls.
- Prompt sends to the global backing session.

Cloud tests:

- Online desktop/TUI produces a Computer Use capability.
- Computer Use appears with zero projects.
- Computer Use appears once for multiple projects.
- Project new-session flow cannot create Computer Use.
- Existing project sessions still render by project.

Relay/runtime tests:

- Global Computer Use attach/send works without project id.
- Hidden backing session uses app-owned runtime root.
- Visible session summaries exclude hidden Computer Use sessions.
- Bad project registry state does not affect Computer Use availability.
- Context rollover preserves a continuous user-facing transcript.

TUI tests:

- TUI advertises Computer Use capability when connected.
- Remote cloud attach/send works through the TUI bridge.
- Stop/cancel works for active Computer Use runs.

## Open Decisions

- Whether the hidden backing model is one stable session with hidden run rollover or a chain of hidden sessions presented as one surface.
- Exact app-data directory name for the Computer Use runtime root.
- Whether model selection should remain visible in the simplified Computer Use composer.
- Whether desktop and cloud should share one visual route pattern for device-scoped Computer Use or keep separate shell-level implementations.

## Non-Goals

- Adding project-specific Computer Use sessions.
- Adding yolo mode for Computer Use.
- Exposing context handoff controls to the user.
- Replacing the existing relay transport.
- Reworking normal project session UX beyond removing Computer Use from project-scoped flows.
