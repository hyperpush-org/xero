# Project switch inventory

This records the project rail switch path as of the current implementation. The
user-visible latency target is the shell/titlebar update: it must happen on the
rail pointerdown path, before project hydration, git status, runtime state, or
agent surfaces can run.

## Immediate input path

1. `ProjectRailItem.onPointerDown`
   - Left-click only.
   - Updates the rail-local optimistic project id.
   - Emits `onPreviewProject(projectId)`.
   - `XeroApp.handlePreviewProject` looks up the project name from the already
     loaded project list and writes it into `project-selection-preview`.
   - `XeroShell` subscribes to that external store with `useSyncExternalStore`,
     so the titlebar can repaint without rerendering `XeroApp` or the heavy body
     surfaces.

2. `ProjectRailItem.onClick`
   - Calls the same preview handler for keyboard/click activation.
   - Calls `XeroApp.handleSelectProject`.
   - `handleSelectProject` calls `useXeroDesktopState.selectProject(projectId)`.

## Selection state path

1. `selectProject(projectId)`
   - Ignores a same-project click when there is no current load error.
   - Increments `projectSelectionRequestRef`.
   - Sets `pendingProjectSelectionId`.
   - Looks for a cached `ProjectDetailView` or a list summary.
   - Builds a lightweight `ProjectDetailView` shell if the full project is not
     cached.
   - Updates `activeProjectIdRef` and `activeProjectRef`.
   - Applies the React project preview state before hydration:
     - clears repository status
     - sets `activeProjectId`
     - sets `activeProject`
     - resets repository diffs
   - Waits for a paint using `waitForProjectSelectionPaint`.
   - Calls `loadProject(projectId, 'selection', { applyCachedProject: false })`.
   - Clears `pendingProjectSelectionId` when this selection is still current.

2. `loadProject(projectId, 'selection')`
   - Calls `loadProjectState`.
   - Selection loads intentionally skip `getProjectLoadBundle`; the bundle can
     include expensive secondary work and should not be in the rail-click path.

## Hydration path

`loadProjectState` starts these jobs:

- `adapter.getProjectSnapshot(projectId)`
- `adapter.getRepositoryStatus(projectId)`
- `adapter.getRuntimeSession(projectId)`
- `adapter.listNotificationDispatches(projectId)`
- `loadNotificationRoutes(projectId, { force: true })`

After the snapshot resolves:

- `mapProjectSnapshot` creates the project detail view.
- The selected agent session id is resolved.
- `adapter.getRuntimeRun(projectId, agentSessionId)` starts.
- `adapter.getAutonomousRun(projectId, agentSessionId)` starts unless the
  snapshot already includes an autonomous projection.
- Cached repository/runtime/run/autonomous data is applied to the first
  snapshot-backed project view.
- The active project and project list are updated from the snapshot.
- `isProjectLoading` is cleared early for selection loads.

After secondary jobs resolve:

- repository status is applied
- notification broker state is applied
- runtime session, runtime run, autonomous run, and their load errors are
  applied in a transition
- repository diffs are reset for the final status
- combined load errors are surfaced

## Backend commands touched by a switch

Common frontend adapter calls during a switch:

- `listProjects`: startup/bootstrap only, not a normal rail click
- `getProjectSnapshot`: project database snapshot and selected agent sessions
- `getRepositoryStatus`: git status and line count summary
- `getRuntimeSession`: runtime supervisor metadata
- `getRuntimeRun`: selected session run metadata
- `getAutonomousRun`: autonomous run inspection when not already projected
- `listNotificationDispatches`: project notification broker state
- `listNotificationRoutes`: notification route configuration
- `getProjectUsageSummary`: separate effect when `activeProjectId` changes
- `readProjectUiState`: surface-specific project UI state readers

Additional effects that can run after `activeProjectId` changes:

- usage summary refresh for the footer
- repository polling/focus refresh registration
- workflow agent inspector project scoping
- agent workspace layout project UI state hydration/persist
- runtime stream subscription changes for the selected session
- surface prewarm and lazy mounted sidebars

## Render path

`XeroApp` derives:

- project list and active project state
- workflow view
- agent view projection
- agent workspace panes/layout
- execution view
- repository status footer
- notification and usage footer state

`XeroShell` renders:

- titlebar/logo/project name
- nav/tools
- project rail
- active body surface
- lazy sidebars
- status footer

The optimized preview path deliberately bypasses the `XeroApp` render path. The
titlebar consumes only the preview store snapshot until the normal active
project state catches up.

## Benchmark

Run:

```sh
npx vitest run src/performance/project-switch-benchmark.test.tsx --reporter verbose
```

The benchmark asserts:

- rail pointerdown updates the shell project name before click selection
- selection is not called on pointerdown
- the synthetic heavy surface does not rerender
- event-to-shell update stays under 50 ms
- React update work stays under a 16 ms frame budget
