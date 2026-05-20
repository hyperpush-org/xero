# Cloud App Refactor Plan

## Reader And Goal

This plan is for the engineer refactoring the Xero cloud app without changing product behavior. After reading it, they should be able to split the large mixed-responsibility files into feature modules, hooks, pure relay/domain logic, and focused ShadCN-backed UI components while keeping the current session experience intact.

The immediate goal is code health, not a redesign. Refactor in small slices, preserve user-facing behavior, and use unit/component tests for confidence. Do not add temporary debug UI.

## Current Diagnosis

The cloud app is already split into `routes`, `components`, and `lib`, but the current boundaries are mostly file-location boundaries, not responsibility boundaries. The largest files are large because they combine several concerns at once:

| Area | Current pressure | Why it hurts |
| --- | --- | --- |
| Active session route | `cloud/src/routes/sessions.$computerId.$sessionId.tsx` owns route params, store selection, relay channel lifecycle, redirect rules, context-meter requests, composer control resolution, command construction, new-session navigation, and layout rendering. | Any change to session behavior requires reading a route file that mixes data flow, effects, payload shape, and JSX. |
| Relay projection | `cloud/src/lib/relay/stream-projection.ts` handles runtime item projection, remote snapshot run replay, remote runtime events, turn merging, action grouping, tool detail formatting, and loose payload parsing. | Projection changes are risky because unrelated event families live in one 1200+ line module with many private helpers. |
| Relay stream hooks | `cloud/src/lib/relay/use-session-stream.ts` combines session channel subscription, account presence, session directory sync, project directory sync, command retry intervals, remote control parsing, and payload guards. | Hook side effects, command builders, and parsing logic are tangled, which makes testing and reuse awkward. |
| Session store | `cloud/src/lib/relay/session-store.ts` contains store types, visible session reducers, project reducers, presence reducers, transcript reducers, model option normalization, and equality/sort helpers. | Zustand updates are hard to reason about because one store module owns several independent state domains. |
| Session list UI | `cloud/src/components/session-list-panel.tsx` renders header, empty state, rows, pending actions, account footer, install action, and sign-out affordances. Sidebar and drawer wrap it differently. | The core list component has to know too much about layout variants and async row actions. |
| Login route | `cloud/src/routes/index.tsx` mixes route redirect logic, marketing copy, preview data, sign-in state, hero rail, mobile hero, and install affordance rendering. | The route is doing both route work and page composition work, so auth or visual changes collide. |

The plan below keeps the app's existing technologies: TanStack Router/Start, React, ShadCN-compatible `@xero/ui` components, Zustand, Phoenix channels, Vitest, Testing Library, and Biome.

## Target Shape

Move from broad folders to feature/domain boundaries:

```text
cloud/src/
  routes/
    index.tsx
    sessions.tsx
    sessions.$computerId.$sessionId.tsx
  features/
    login/
      login-screen.tsx
      login-hero.tsx
      login-sign-in-panel.tsx
      login-preview.tsx
    session-view/
      session-view-page.tsx
      session-layout.tsx
      conversation-pane.tsx
      composer-dock.tsx
      use-active-session.ts
      use-session-controls.ts
      use-session-context-meter.ts
      use-session-commands.ts
      session-command-builders.ts
    session-list/
      session-list-panel.tsx
      session-list-header.tsx
      session-list-empty-state.tsx
      session-account-footer.tsx
      session-list-row.tsx
      use-session-list-actions.ts
  lib/
    relay/
      channel/
        session-channel.ts
        account-presence.ts
        remote-session-directory.ts
        remote-project-directory.ts
        command-builders.ts
      projection/
        index.ts
        runtime-items.ts
        remote-snapshots.ts
        remote-runtime-events.ts
        turn-merge.ts
        action-groups.ts
        tool-details.ts
        payload-fields.ts
      store/
        index.ts
        types.ts
        visible-sessions.ts
        remote-projects.ts
        computer-presence.ts
        transcripts.ts
        model-options.ts
```

This is a target map, not a requirement to move everything in one pass. The route files should eventually become thin shells that load route context and render feature pages. Shared ShadCN UI should still come from `@xero/ui`; cloud-specific composition belongs under `features`.

## Refactor Principles

Keep every slice behavior-preserving. Move code first, then improve naming and boundaries once tests still pass.

Prefer pure modules for parsing, command building, projection, sorting, equality checks, and model option normalization. Hooks should orchestrate side effects, not contain large parsing or DTO-building blocks.

Avoid compatibility layers. This is a new app, so when a module moves, update imports in the same slice and delete the old entry point unless a temporary index is needed inside the slice.

Use ShadCN components wherever UI primitives exist. Do not introduce debug surfaces, temporary panels, or development-only controls.

Keep file budgets visible:

| File kind | Target |
| --- | --- |
| Route files | Under 120 lines, ideally only route config plus page render |
| Feature page components | Under 250 lines |
| Leaf UI components | Under 180 lines |
| Hooks | Under 220 lines |
| Pure relay/projection modules | Under 300 lines |
| Store slices | Under 220 lines |

Budgets are guidance, not a reason to create tiny files with no clear ownership.

## Phase 0 - Safety Net

Before moving code, make sure existing tests cover the behavior that is easiest to break.

Add or strengthen tests only where coverage is missing:

- Active session command payloads for `send_message`, `update_session_controls`, and `start_session`.
- Context meter request behavior: debounce, stale/loading/ready/error states, and no request while a session is live.
- Route guard behavior when a computer goes offline, a join is rejected, or the current session is no longer visible.
- Projection golden cases for remote snapshots, live runtime events, command output, file changes, action prompts, failure turns, assistant delta merging, and action grouping.
- Account directory behavior for presence sync, visible-session replacement, project list replacement, offline cleanup, archive command, and visibility command.

Acceptance:

- No temporary UI is added.
- Tests describe user-visible behavior and payload contracts, not implementation details.
- Existing cloud tests still pass before the first extraction.

Suggested scoped commands:

```bash
pnpm --dir cloud test
pnpm --dir cloud check
```

## Phase 1 - Thin The Active Session Route

Create a `session-view` feature and move behavior out of the dynamic session route in this order:

1. Extract `SessionViewPage`.
   - The route keeps `createFileRoute`, reads params/context, and renders the feature page.
   - No command building or JSX layout remains in the route file.

2. Extract `useActiveSession`.
   - Owns route params, session key, store selections, remote account session state, current transcript, visibility, online status, and redirect eligibility.
   - Returns a small view model instead of exposing raw store calls everywhere.

3. Extract `useSessionControls`.
   - Owns selected agent/model/thinking/auto-compact state.
   - Resolves effective agent, model, provider profile, raw model id, thinking options, and labels.
   - Keeps `formatThinkingEffortLabel` with control logic, not in the route.

4. Extract `useSessionContextMeter`.
   - Owns draft debounce, context request key construction, pending request state, context snapshot request dispatch, and indicator status.
   - Return `contextMeter` data and let `composer-dock` render the existing `WebComposerContextIndicator`.

5. Extract `useSessionCommands`.
   - Owns `send_message`, `update_session_controls`, and `start_session` command construction.
   - Use pure command-builder helpers so command payload tests do not need React.
   - Keep attachment reads and clearing at the hook boundary.

6. Extract layout components.
   - `session-layout` composes sidebar, top bar, drawer, and main content.
   - `conversation-pane` renders loading, empty state, and `ConversationSection`.
   - `composer-dock` renders `WebComposer` and wires control callbacks.

Acceptance:

- The route file only defines the TanStack route and delegates to the feature page.
- The feature page reads like orchestration, not a wall of effects and JSX.
- Command payload tests pass without rendering the whole route.
- Existing session drawer/sidebar/top-bar behavior is unchanged.

## Phase 2 - Split Relay Channel Orchestration

Break `use-session-stream.ts` into channel-level hooks and pure helpers:

1. `session-channel`
   - Owns joining/leaving a specific session channel.
   - Emits decoded snapshot/event frames to callbacks.
   - Handles join rejection as a channel state, not as store mutation directly.

2. `account-presence`
   - Owns `account:<accountId>` presence subscription and online desktop id extraction.
   - Exports a pure `hasDesktopPresence` helper with tests.

3. `remote-session-directory`
   - Owns `__sessions__` channels, visible-session list requests, retry until reconciled, replace/upsert/remove application, and remote visibility/archive command sending.

4. `remote-project-directory`
   - Owns `__projects__` channels, project list requests, retry until reconciled, and project replacement.

5. `command-builders`
   - Builds inbound command DTOs for list sessions, list projects, remote visibility, archive, send message, update controls, start session, context snapshot, attachment staging/discarding where applicable.
   - No React imports.

Acceptance:

- `useSessionStream` becomes a small composition hook over `session-channel` plus store application.
- `useAccountRemoteSessions` becomes a small composition hook over presence, session directory, and project directory.
- Retry interval setup/cleanup is localized and tested with fake timers.
- Payload parsing helpers are not duplicated between hooks.

## Phase 3 - Split Projection By Event Family

Move `stream-projection.ts` to `lib/relay/projection` without changing public exports at first:

1. `payload-fields`
   - `isRecord`, `recordField`, `recordArray`, `stringField`, `nonEmptyStringField`, `numberField`, `arrayStringField`, and `booleanField`.

2. `runtime-items`
   - Projection from current runtime stream items: transcript, tool, activity, action required, and failure.

3. `remote-snapshots`
   - Snapshot schema handling, persisted run replay, terminal run handling, message fallback behavior, and snapshot live-state calculation.

4. `remote-runtime-events`
   - Remote runtime event schema handling and event-kind dispatch.

5. `turn-merge`
   - `appendConversationTurn`, assistant/thinking delta merging, action merge by tool call id, cloning, and group expansion.

6. `action-groups`
   - Compacting terminal tool bursts, code-edit exceptions, group state, and group summaries.

7. `tool-details`
   - Tool title, detail fallback text, detail rows, command output detail, context event title/fallback, file change detail parsing, and truncation.

Acceptance:

- Existing imports can still use the projection index.
- Golden projection tests pass unchanged first, then can be split by event family.
- No event-family module imports React or hooks.
- Adding a new remote event kind only requires touching the event-family module and relevant tests.

## Phase 4 - Split Session Store Into Slices

Refactor `session-store.ts` after projection is stable, because the store depends on `appendConversationTurn`.

Suggested modules:

- `types`: visible session, remote project, transcript, context snapshot/error, model option, and thinking effort types.
- `visible-sessions`: sort/equality/pruning helpers and visible-session reducers.
- `remote-projects`: project sort/equality helpers and project reducers.
- `computer-presence`: online computer map/equality helpers and presence reducers.
- `transcripts`: snapshot replacement, append turn, controls update, context snapshot update, live flag update.
- `model-options`: `modelOptionId`, `normalizeModelOptions`, `parseThinkingEffort`, and fallback model option insertion.
- `index`: creates and exports the Zustand store.

Acceptance:

- Store update behavior remains referentially stable where current tests expect no-op updates.
- Selectors in feature hooks read focused state, not whole-store objects.
- Model normalization tests live with `model-options`.
- Visible session and project equality tests live with their slices.

## Phase 5 - Split Login Route Composition

Move the signed-out route UI into `features/login`:

1. Keep route redirect logic in `routes/index.tsx`.
2. Move sign-in state and GitHub login action to `login-screen`.
3. Move desktop hero rail to `login-hero`.
4. Move preview session list to `login-preview`.
5. Move mobile sign-in panel to `login-sign-in-panel`.
6. Keep install affordance as a real user-facing feature via `InstallAppAction`.

Acceptance:

- The route file is thin.
- Copy/data arrays are owned by login feature modules.
- The sign-in error and pending states are covered by component tests.
- No landing-page redesign is included in this refactor.

## Phase 6 - Split Session List Surface

Move session list components into `features/session-list` and reduce `SessionListPanel`:

1. `session-list-header`
   - Title, count, new-session picker, optional close slot.

2. `session-list-empty-state`
   - Current ShadCN `Empty` rendering and iconography.

3. `session-account-footer`
   - GitHub account link, install action, sign out button.

4. `use-session-list-actions`
   - Pending row action state, select hidden session behavior, visibility toggle, archive action, and after-select callback.

5. `session-list-panel`
   - Only composes header, list/empty state, rows, and footer.

Acceptance:

- Sidebar and drawer still share one panel implementation.
- Drawer-specific close behavior stays in the drawer wrapper.
- Footer actions remain accessible and use existing ShadCN primitives.
- Existing session list tests pass with minor import updates.

## Phase 7 - Cleanup And Guardrails

After the extraction slices land:

- Delete obsolete modules and temporary internal indexes.
- Run Biome format/check on touched cloud files.
- Add a small documented file-size check if the team wants an automated guard, but do not block the refactor on adding tooling.
- Review imports for cycles between `features` and `lib`.
- Keep `routes` depending on `features`, `features` depending on `lib` and `@xero/ui`, and `lib` depending on neither `routes` nor `features`.

Acceptance:

- Route files are thin.
- No production cloud file still exceeds the agreed budget without an explicit reason.
- Public behavior and tests remain stable.
- The cloud app still uses ShadCN-compatible primitives through `@xero/ui`.

## Suggested Slice Order

Use this order to reduce merge pain and keep each PR/review small:

1. Add projection golden tests and command-builder tests.
2. Extract session command builders from the active session route.
3. Extract `useSessionControls` and `useSessionContextMeter`.
4. Extract `SessionViewPage`, `ConversationPane`, and `ComposerDock`.
5. Split relay projection modules behind a stable index.
6. Split account/session/project channel orchestration.
7. Split session store modules.
8. Split login route components.
9. Split session list panel.
10. Cleanup imports, file budgets, and docs.

## Verification Matrix

Run focused checks after each slice:

```bash
pnpm --dir cloud test
pnpm --dir cloud check
```

For projection/store slices, run the relevant focused tests first:

```bash
pnpm --dir cloud test src/lib/relay/stream-projection.test.ts
pnpm --dir cloud test src/lib/relay/session-store.test.ts
pnpm --dir cloud test src/lib/relay/use-session-stream.test.ts
```

For component/route slices:

```bash
pnpm --dir cloud test src/components/session-drawer.test.tsx
pnpm --dir cloud test src/components/session-top-bar.test.tsx
pnpm --dir cloud test src/components/new-session-picker.test.tsx
pnpm --dir cloud test src/components/web-composer-dictation.test.tsx
```

If tests move with the feature modules, update the command paths in the same slice.

## Completion Criteria

The refactor is complete when:

- Active session route, login route, and sessions index route are thin route shells.
- Relay projection is split by event family and still covered by golden tests.
- Relay channel orchestration is separated from payload parsing and store mutation.
- Session store logic is split into typed slices with focused tests.
- Session list UI has focused header, empty state, list action, row, and footer modules.
- No temporary/debug UI has been introduced.
- All cloud tests and checks pass.
