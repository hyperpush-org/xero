# Cloud Session Shell Architecture

## Reader And Goal

This proposal is for the engineer fixing cloud session navigation at the architecture level. After reading it, they should be able to refactor the cloud app so creating or switching sessions swaps only the active conversation surface, while the account chrome, sidebar, project groups, and session directory stay mounted.

The goal is not to hide reload flashes with timing. The goal is to remove the ownership boundary that makes the app vulnerable to shell teardown during normal session navigation.

## Problem

The cloud session UI currently treats the sessions home view and the active session view as separate owners of the same shell. Each path assembles its own sidebar, drawer, top bar, account footer, remote session directory hook, and navigation handlers.

That structure creates three failure modes:

1. Route transitions can briefly replace the whole app with the global pending component while cached auth or loader work resolves.
2. The sidebar and drawer are rebuilt when moving between the sessions index and a concrete session, even though the user perceives them as persistent app chrome.
3. Creating a session waits for a remote directory update and then navigates into a route whose parent shell is not stable by design.

The user-visible result is a flash: the entire app, including the sidebar, disappears and returns when a new session is created or a different session is selected.

## Architectural Decision

Make the authenticated sessions area a persistent shell route.

The shell route should own:

- Cloud auth/session context.
- Account-level remote session and project directory subscriptions.
- Desktop online/presence state.
- Sidebar and drawer composition.
- Top-bar composition and active title derivation.
- Session/project selection handlers.
- Sign-out handling.

Child routes should own only the content that genuinely changes:

- Sessions index empty/selection state.
- Active session transcript, composer, context meter, attachment staging, and live channel.
- Any future active-session subviews.

This creates a simple invariant:

> Navigating between cloud sessions must never unmount the sessions shell. It may only update shell props and replace the active content outlet.

## Target Route Shape

Use the existing TanStack Router stack, but separate layout ownership from child content ownership.

```text
Root document
  Signed-out landing route
  Authenticated sessions shell route
    Sessions index child
    Active session child
```

The sessions shell route renders the app frame once and places an outlet inside the main content region. The active session child renders inside that outlet.

The sessions index child should no longer render a second copy of the shell. It should render only the empty/selection content that belongs inside the already-mounted shell.

## Shell Data Model

Introduce a single cloud session directory view model for the shell. It should return:

- Authenticated cloud account.
- Visible session summaries.
- Remote project summaries.
- Online desktop state.
- Active route target, if present.
- Active session summary, if present.
- Active project label.
- Whether the active target is still valid.
- Commands for selecting a session, starting a session, changing remote visibility, archiving, and signing out.

This view model should be the only place that subscribes to the account-level remote session directory. Active session children should not independently create account-level directory subscriptions.

The active session child can still join the specific live transcript channel for the selected session. That lifecycle is content-specific and should unmount when the selected session changes.

## Navigation Model

Session row clicks should perform client-side route navigation only. No internal cloud navigation should use plain anchors.

Starting a new session should be modeled as a shell-level command with explicit pending state:

1. The shell records a pending start request with project id and the set of known session ids.
2. The shell sends the remote `start_session` command.
3. While the desktop acknowledges the request, the shell stays mounted and the new-session control can show a pending state.
4. When the directory reports a new session for that project, the shell clears the pending request and navigates the outlet to that session.
5. If the command fails or times out, the shell clears the pending state and leaves the current content intact.

The current route should not be replaced by a full-screen loader during this flow. Loading belongs inside the outlet or the specific control that initiated the action.

## Pending And Loading Policy

Global pending UI should be reserved for app boot, hard auth transitions, and first paint where there is no shell yet.

Once the sessions shell is mounted:

- Auth refresh should keep showing the current shell until it succeeds or redirects.
- Session directory refresh should update the sidebar in place.
- Active transcript load should show a content-local loading state in the conversation pane.
- New-session creation should show a control-local pending state, not a full-app pending screen.
- Route pending for child session content should render inside the outlet.

After this refactor, any router delay used to mask fast cached navigations can be removed.

## Component Boundaries

The target component boundary should look like this:

```text
SessionsShell
  SessionSidebar
  SessionTopBar
  SessionDrawer
  SessionsContentOutlet

SessionsIndexContent
  Empty/selection state only

ActiveSessionContent
  Conversation viewport
  Empty transcript state
  Transcript rendering
  Composer dock
```

The shell should pass stable callbacks and a small context object through the outlet context. Child routes should not reconstruct shell props.

## State Ownership

Keep state close to its lifetime:

| State | Owner |
| --- | --- |
| Authenticated cloud account | Sessions shell route context |
| Account remote session/project directory | Sessions shell view model |
| Sidebar group collapse state | Sidebar/list component |
| Pending new-session request | Sessions shell view model |
| Active transcript/channel | Active session child |
| Draft prompt and composer controls | Active session child |
| Context meter request state | Active session child |
| Attachment staging | Active session child |

This split prevents a session switch from resetting sidebar state, account subscriptions, or shell layout.

## Redirect Rules

Redirect logic should also follow ownership:

- The shell redirects to signed-out landing only when auth is missing or invalid.
- The shell may redirect from the sessions index to the first visible session after the session directory is reconciled.
- The active session child reports invalid active targets to the shell view model, but the shell decides whether to return to the index.
- The active session child should not own account-level offline cleanup.

This avoids competing effects where a child route tears down while the parent is still resolving whether the target is valid.

## Implementation Plan

1. Create the persistent sessions shell.
   - Move common sidebar, drawer, top bar, sign-out, session selection, and project selection wiring into the shell.
   - Render an outlet where the active conversation or sessions index content appears.

2. Move account-level remote directory subscriptions into the shell.
   - The shell subscribes once per authenticated account.
   - Active session content receives visible session data from shell context when needed.

3. Thin the sessions index child.
   - Remove duplicate shell composition.
   - Render only empty/selection content inside the shell outlet.

4. Thin the active session child.
   - Remove duplicate sidebar/drawer/top-bar composition.
   - Keep only active transcript, composer, context meter, remote attachments, and session channel logic.

5. Move new-session pending state to the shell.
   - Starting a session should not require the active session child to remain mounted.
   - The shell reconciles the desktop-created session and navigates the outlet.

6. Scope pending UI.
   - Replace full-app pending during sessions child transitions with outlet-local pending.
   - Keep full-app pending only before the shell exists.

7. Remove the timing workaround.
   - Once the shell is persistent and pending is scoped, remove any router pending delay that was added only to mask shell teardown.

## Test Plan

Add tests that assert lifecycle, not pixels:

- Switching from one session to another does not unmount the session sidebar.
- Navigating from the sessions index to an active session does not unmount the session sidebar.
- Creating a new session keeps the existing shell visible while the directory update is pending.
- Active transcript loading renders inside the content area, not as a full-app loading screen.
- Internal session navigation uses client-side router links/actions.
- The account-level remote session directory hook is mounted once per sessions shell, not once per child route.

Use component tests and hook tests. Do not add temporary debug UI.

## Acceptance Criteria

- The sessions shell remains mounted across session switches, new-session navigation, and index-to-session navigation.
- Sidebar collapse/group state survives session switches.
- Account-level remote subscriptions are not recreated during child route changes.
- Active transcript subscriptions are recreated only when the active session target changes.
- No full-screen loading screen appears after the sessions shell has mounted.
- The timing-based pending delay can be removed without reintroducing flash.

## Reader Test

A fresh engineer should be able to implement the refactor in vertical slices:

1. Create the shell route and move shared chrome into it.
2. Move the index content into the shell outlet.
3. Move the active session content into the shell outlet.
4. Move new-session pending state to the shell.
5. Scope loading and remove the delay.

Each slice has observable behavior and focused tests. The document intentionally avoids a visual redesign, new UI surfaces, and compatibility glue.
