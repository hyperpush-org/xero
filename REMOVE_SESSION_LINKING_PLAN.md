# Remove Session Link/Unlink Plan

## Reader And Outcome

This plan is for the engineer removing per-session cloud linking from Xero.
After reading it, they should be able to make every non-archived desktop session available in the cloud app, remove the link/unlink controls, and preserve the current low-load streaming model where only the active cloud session joins a live transcript channel.

## Decision

Remove per-session link/unlink as a product feature.

The feature was introduced as a performance guard, but the current relay design already avoids the expensive case: the cloud app keeps account-level directory subscriptions mounted and joins the active session stream only when a user opens that session. Making all non-archived sessions visible does not require subscribing to all transcript streams.

After this change:

- All non-archived desktop sessions are visible in the cloud session directory.
- The cloud app can open any listed session without first sending a visibility command.
- The desktop still streams transcript/runtime frames only for sessions with an active cloud join or an outbound event path.
- Archiving remains the way to remove a session from the cloud list.
- Device sign-in/revocation remains the account-level security boundary.

## Non-Goals

- Do not add a replacement per-session sharing mode.
- Do not preserve backwards compatibility for hidden sessions.
- Do not add migration glue for old local visibility state.
- Do not make the cloud app subscribe to every transcript channel.
- Do not change account/device linking.
- Do not redesign the session list.

## Current Behavior To Remove

Desktop stores a per-session remote visibility flag and a bridge-local visibility list. The desktop UI exposes a cloud/share toggle for the selected session. The cloud session list exposes link/unlink behavior: opening an unlinked session first sends a visibility command, while linked sessions can be unlinked from the row action.

The relay also uses this visibility state as an authorization gate for joining concrete session topics. Special control topics for session directory, project directory, and new-session creation are already always allowed.

## Target Behavior

The account link controls remote access at the device level. Once a desktop is signed into the relay, the cloud app lists all non-archived sessions from that desktop.

Cloud navigation should be simple:

1. Account/session shell subscribes to desktop presence, session directory, project directory, and new-session control topics.
2. The sidebar shows all non-archived sessions.
3. Selecting a session navigates directly to it.
4. The active session route joins only that selected session channel.
5. Switching sessions leaves the previous active channel and joins the next one.
6. Archiving removes the session from both desktop and cloud lists.

This keeps idle sessions cheap. Ten visible sessions with one active run should behave close to one visible session with one active run because the extra nine sessions are directory metadata, not live transcript subscribers.

## Implementation Slices

### 1. Simplify The Data Model

Make remote session visibility derived from session archival status instead of a stored per-session user choice.

- Stop writing the per-session remote visibility flag from user commands.
- Stop reading the bridge-local visibility list as the source of allowed sessions.
- Treat all non-archived sessions as cloud-visible when building the remote session directory.
- Return `remoteVisible: true` for every non-archived session DTO while the UI contract is being simplified.
- Remove stale local visibility state from the active runtime path rather than migrating it.

Because this is a new app and backwards compatibility is prohibited, do not preserve old hidden-session semantics. If incompatible app-data state causes runtime issues during development, wipe the affected app-data state.

### 2. Remove Desktop Share Controls

Delete the desktop UI affordance that toggles session sharing.

- Remove the selected-session cloud/share button from the agent runtime header.
- Remove props and callbacks that only exist to toggle session remote visibility.
- Keep the passive "available in cloud" indicator only if it still has clear user value. Prefer removing it if every session is always available, because a universal indicator becomes visual noise.
- Remove tests that assert the toggle exists, and replace them with tests that session switching/archiving still work.

No temporary debug UI should be added during this work.

### 3. Remove Cloud Link/Unlink Controls

Delete cloud-side link/unlink behavior from the session list.

- Remove unlink row actions.
- Remove the branch that opens an unlinked session by first sending a visibility command.
- Remove pending visibility state from session list components.
- Make all selectable rows behave the same way.
- Keep archive behavior and its confirmation flow.
- Update accessible labels and titles so they no longer mention linking or unlinking.

The cloud UI should not expose hidden/unshared session states after this change.

### 4. Simplify Relay Commands

Retire the visibility command from cloud-to-desktop command handling.

- Remove the cloud command builder for setting session visibility.
- Remove inbound command routing for visibility changes.
- Remove command result handling that only exists for visibility toggles.
- Keep session listing, project listing, new-session creation, session snapshot, send-message, context snapshot, attachment, run-control, cancellation, and archive commands.

If the command enum or protocol type is shared with generated/client contracts, remove the command in the same slice and update tests together. Do not leave a no-op compatibility command unless explicitly requested.

### 5. Relax Concrete Session Join Authorization

Update session channel authorization so a web device can join any non-archived session that belongs to its linked desktop/account.

The desktop should still reject:

- Unknown sessions.
- Archived sessions.
- Sessions from another account or desktop.
- Commands from revoked or unknown web devices.

The server should continue enforcing account/desktop ownership before asking the desktop to authorize a concrete session join.

### 6. Preserve The Active-Only Streaming Model

Keep the current performance invariant explicit in code and tests:

- The session shell subscribes to account-level directory/control channels once.
- The active session route joins one concrete session channel at a time.
- Session switches leave the old concrete channel.
- The cloud app does not join concrete channels for every row in the sidebar.

This is the real optimization boundary. Removing link/unlink should not change it.

### 7. Clean Up Persistence

Remove obsolete remote visibility persistence where practical.

- Drop bridge-local visibility state usage.
- Stop creating or updating visibility state files.
- Remove database reads/writes that only serve per-session visibility.
- If the column remains temporarily to keep a slice small, make it inert and schedule its removal in the same milestone.

Because backwards compatibility is prohibited, prefer deleting stale state and schemas over carrying compatibility branches.

## Test Plan

Use scoped tests and commands.

- Cloud session store/projection tests: all listed non-archived sessions are selectable and no hidden/unlinked state is represented.
- Cloud session list tests: no unlink action, opening a row navigates directly, archive still works.
- Cloud stream hook tests: account directory channels mount once and active transcript channel behavior is unchanged.
- Server channel tests: web can join a valid non-archived desktop session without per-session visibility, cannot join another account desktop, cannot join archived/unknown sessions.
- Desktop remote bridge tests: session directory includes all non-archived sessions, archived sessions are omitted, inbound commands still validate linked web devices.
- Desktop UI tests: no share/unshare toggle is rendered, archive/session selection flows still work.

Suggested scoped verification:

```sh
pnpm --dir cloud test src/lib/relay/use-session-stream.test.ts src/components/session-drawer.test.tsx
pnpm --dir cloud test src/components/session-list-panel.test.tsx src/components/session-list-row.test.tsx
mix test server/test/xero_web/channels/remote_channel_test.exs
pnpm --dir client test components/xero/agent-runtime.test.tsx components/xero/agent-sessions-sidebar.test.tsx
```

Adjust exact test file names to the final touched files. For Rust work, run only the affected Cargo tests and only one Cargo command at a time.

## Rollout Checklist

- All non-archived desktop sessions appear in cloud after account sign-in.
- Opening any listed session requires no link step.
- No user-facing copy says link, unlink, share to web, unshared, or hidden for sessions.
- Server channel count remains tied to active session views, not sidebar row count.
- Active session events still stream live.
- Idle sessions do not emit transcript/runtime frames solely because they are listed.
- Archiving still removes a session from cloud.
- Revoking a web device still prevents remote commands.
- Old app-data visibility state does not affect behavior.

## Reader Test

A fresh engineer should be able to implement this in vertical slices:

1. Make session directory visibility derived from non-archived sessions.
2. Remove desktop and cloud visibility controls.
3. Remove visibility commands and persistence.
4. Update join authorization to use session existence/archive/account checks.
5. Verify the active-only streaming invariant.

The key design rule is simple: account linking controls access; active session routing controls streaming load.
