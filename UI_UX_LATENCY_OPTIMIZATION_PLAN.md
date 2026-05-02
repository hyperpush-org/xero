# UI/UX Latency Optimization Plan

Reader: an internal Xero engineer or agent responsible for making the desktop UI feel immediate under normal editor, agent, repository, and sidebar workflows.

Post-read action: implement the optimization work in safe phases, with measurable before/after evidence, without adding temporary debug UI or changing product behavior for its own sake.

Status: draft.

## Goal

Xero currently feels like many actions carry small but noticeable latency. The audit indicates this is not a single obvious blocking operation. It is the compound effect of broad React re-renders, high-frequency runtime events, eager editor code, hidden-but-mounted UI surfaces, layout polling, and unstable callback identities.

The goal is to reduce sub-frame and low-millisecond friction across the UI layer while preserving the current product shape:

- Agent streaming should remain smooth during tool-heavy runs.
- Editor typing should stay local to the editor hot path.
- Opening and closing sidebars should not disturb heavy panes.
- Repository status updates should not trigger unnecessary diff reloads.
- Inactive views should not participate in active-view updates.
- Startup and first interaction should not pay for editor/language functionality before it is needed.

## Product Constraints

- This is a Tauri desktop app. Do not validate the app by opening it in a normal browser.
- Use ShadCN/Radix UI components where possible for any user-facing UI changes.
- Do not add temporary debug or test UI. Profiling and measurement must live in tests, scripts, traces, or developer tooling, not in the product surface.
- Run scoped tests and format checks. Avoid repo-wide Rust commands unless a phase explicitly needs them.
- Run only one Cargo command at a time.
- Do not create branches or stashes unless explicitly asked.
- New durable state belongs under OS app-data, not the legacy repo-local `.xero/` state.
- Backwards compatibility is prohibited unless explicitly requested; prefer clean new contracts over compatibility shims.

## Research Principles To Apply

VS Code keeps user-facing operations responsive by isolating extension work from the renderer and lazily activating extension behavior. Xero should apply the same principle to runtime streams, repository work, and feature sidebars: data production and validation should not force the entire UI shell to render.

VS Code's editor performance work also shows that real hot paths can be surprising. Measure the actual slow calls before and after each phase; do not assume a data-structure or native boundary change is an automatic win.

Zed optimizes around frame budgets and foreground/background separation. Treat the UI thread as a scarce foreground executor. Anything that is not immediately visible or required for the current input should be deferred, coalesced, or moved out of the hot path.

CodeMirror already virtualizes the editor viewport and separates DOM write/measure phases. Xero should avoid wrapping it in a React-controlled pattern that forces whole-document strings and parent state updates on every keystroke.

## Baseline Hypotheses

The audit found these likely contributors:

1. Centralized desktop state causes broad App-level re-renders.
2. Runtime and repository events update several top-level state slices per event.
3. Runtime stream item validation and delivery happen on the hot path before React updates.
4. Agent, workflow, execution, and sidebar surfaces remain mounted while hidden.
5. CodeMirror language packages are eagerly imported.
6. CodeMirror changes are mirrored through React by calling full-document `toString()` on every edit.
7. Browser sidebar resize handling reads layout every animation frame while open.
8. VCS diff loading depends on unstable array and callback identities.
9. Some rail/sidebar animations still use layout-affecting properties in frequently changing surfaces.
10. The shell/titlebar re-renders for state changes it does not visually depend on.

## Follow-up Code Review Notes (2026-05-02)

Static review after the initial draft found that several first-wave optimizations already exist in the codebase:

- `client/src/features/xero/use-xero-desktop-state/high-churn-store.ts` has selector-based high-churn subscriptions.
- `client/src/features/xero/use-xero-desktop-state/runtime-stream.ts` buffers runtime stream events and repository status updates.
- `client/components/xero/code-editor.tsx` lazy-loads language extensions, debounces full snapshots, and coalesces cursor reports.
- `client/components/xero/browser-resize-scheduler.ts` and `client/components/xero/browser-sidebar.tsx` have moved browser viewport sync to scheduled observer-driven work.
- `client/components/xero/vcs-sidebar.tsx` has stable revision keys and a small diff patch cache.
- `client/src/performance/performance-smoke.test.tsx` and `client/scripts/performance-smoke.mjs` provide a browser-free smoke path.

The next optimization work should therefore bias toward second-order hotspots:

- Rich text surfaces: `conversation-markdown.tsx` reparses markdown on render and asks Shiki to tokenize code blocks by content/theme.
- Diff surfaces: `vcs-sidebar.tsx` renders every diff line and tokenizes all non-header lines for the selected patch.
- File surfaces: `project_files.rs` builds the full project tree recursively, and `file-tree.tsx` renders the visible tree recursively without windowing.
- Search/replace: `search_project.rs` walks the full tree and returns complete result payloads up to large caps.
- Native boundaries: repository diff/status, project files, browser events, emulator frames, and runtime streams need explicit payload and frame budgets.
- Optional heavy surfaces: Solana, emulator, games, provider/model registries, diagnostics, and settings subpanels should keep listeners, timers, and polling inactive unless visible or explicitly preloaded.

Treat these as investigation leads, not conclusions. Each phase still needs before/after evidence from the same workflow.

## Success Metrics

Use these as targets, not as dogma. If a target is unrealistic after profiling, update the plan with evidence.

- Agent stream burst: one visible React commit per frame or fewer during high-frequency stream events.
- Runtime event handling: no repeated full-shell renders for stream-only updates.
- Editor typing: keystrokes should not cause App shell, project rail, or inactive sidebars to re-render.
- Sidebar open/close: no continuous layout polling after the sidebar reaches steady state.
- VCS panel: selected-file diff is not refetched unless project, selected path, selected scope, or repository revision changes.
- Bundle shape: editor language code is not part of the initial app chunk unless the first view actually needs an editor.
- Long tasks: normal agent streaming, editor typing, and sidebar toggles should avoid main-thread tasks over one frame budget in production profiling.

## Phase 0: Baseline And Measurement

Purpose: establish proof before changing architecture.

Work:

- Add or use a production-mode profiling harness for the Tauri frontend. The harness must not add visible product UI.
- Capture baseline React commit counts for these workflows:
  - Open a project and switch between Workflow, Agent, and Editor.
  - Run or replay a high-volume runtime stream burst.
  - Type in the code editor for several seconds.
  - Open, resize, and close browser/VCS/usage sidebars.
  - Receive repeated repository status events while VCS is closed and while it is open.
- Capture bundle composition from the Vite production build.
- Identify the top render causes in App shell, project rail, agent runtime, execution workspace, and sidebars.
- Add small test fixtures or replay utilities for stream bursts and repository status updates if none exist.

Acceptance criteria:

- A baseline note exists in this plan or a companion section with commit counts, bundle sizes, and the slowest user-visible workflows.
- The measurement path can be re-run after each phase.
- No temporary debug UI was introduced.

Verification:

- Run the scoped frontend build.
- Run only the relevant frontend tests for any harness helpers.
- If a Rust replay/helper is added, run only the scoped Rust test for that helper.

## Phase 1: Split UI State By Ownership

Purpose: stop small state changes from waking the whole application shell.

Work:

- Inventory every value currently returned by the main desktop state hook and assign an owner:
  - App/session shell state.
  - Project list and active project metadata.
  - Repository status and diffs.
  - Runtime sessions, runs, streams, and actions.
  - Provider, MCP, skill, doctor, and account settings.
  - Sidebar open/closed UI state.
  - Editor workspace state.
- Introduce selector-based subscriptions for high-churn stores. Prefer a small `useSyncExternalStore`-based store or an already-established local pattern over a new dependency.
- Keep low-churn setup and settings state in normal React state where that is simpler.
- Move runtime stream data out of the top-level App render path.
- Move repository status into a store that lets the shell subscribe only to counts/branch label while VCS subscribes to entries/diffs.
- Split titlebar/shell into memoized leaves so a stream item cannot re-render menus and static controls.
- Stabilize callbacks passed from App into sidebars and panes with `useCallback` where identity currently causes downstream effects.

Acceptance criteria:

- Stream-only updates do not re-render inactive panes or closed sidebars.
- Repository count updates can update the shell badge without forcing VCS diff effects.
- The shell/titlebar only re-renders when shell-visible props change.
- The implementation has no compatibility layer for old state contracts unless a direct caller still needs it during the phase.

Verification:

- Run scoped TypeScript tests for the changed state/store helpers.
- Re-run Phase 0 profiler workflows for agent stream burst and repository status events.
- Run the scoped frontend build.

## Phase 2: Coalesce Runtime And Repository Events

Purpose: turn high-frequency backend events into predictable UI commits.

Work:

- Add an event buffer for runtime stream items outside React state.
- Flush non-urgent stream updates once per animation frame or in a 4-8ms batch window.
- Preserve immediate delivery for critical state transitions:
  - user approval required
  - failure
  - cancellation
  - run completed
  - authentication/session invalid
- Coalesce repeated repository status events by project and revision.
- Avoid mapping the full project list for every runtime update when the changed field is not visible.
- Keep validation safety, but avoid full Zod parsing on every hot stream item if Rust already owns the event contract. Options:
  - Validate command responses and control events normally.
  - Validate a sampled or batched subset of stream items in development/test.
  - Keep a cheap runtime guard for item kind, sequence, run id, and project id in production.
- Add sequence-gap handling that reports a durable error without spamming React.

Acceptance criteria:

- A burst of stream items produces bounded UI commits.
- Critical events still appear without user-visible delay.
- Stream ordering and deduplication remain correct.
- Repository updates no longer reset diffs or project metadata when the effective status did not change.

Verification:

- Add tests for stream buffering, urgent bypass, deduplication, and sequence handling.
- Add tests for repository status coalescing.
- Re-run the agent stream burst profile.
- Run the scoped frontend build.

## Phase 3: De-Control The Code Editor Hot Path

Purpose: let CodeMirror own typing while React observes only the metadata it needs.

Work:

- Lazy-load the CodeEditor surface and language extensions. Initial app startup should not import every supported CodeMirror language.
- Replace eager first-party language imports with async language resolvers and compartments.
- Keep the editor document in CodeMirror state during typing.
- Replace per-keystroke full-document `toString()` with one of:
  - change-set/delta forwarding,
  - debounced full snapshot persistence,
  - dirty flag plus explicit snapshot on save/tab switch,
  - or a hybrid where tiny files snapshot eagerly and larger files debounce.
- Throttle cursor position updates to animation frames.
- Avoid whole-document replacement when the external value changes because of the local editor's own edit.
- When loading a new file, create a fresh editor state or transaction according to CodeMirror guidance so undo history and scroll state are correct.
- Keep read-only/theme/language reconfiguration through compartments.

Acceptance criteria:

- Typing in the editor does not re-render the App shell, project rail, or unrelated sidebars.
- Large-file typing avoids full-document string creation on every keystroke.
- Language code is loaded on demand.
- Save, dirty-state, cursor display, theme switching, read-only mode, and file switching still work.

Verification:

- Add focused tests for editor change handling, dirty state, save behavior, file switching, and cursor throttling helpers.
- Re-run the editor typing profile.
- Compare build chunks before and after.
- Run the scoped frontend build.

## Phase 4: Freeze Or Unmount Inactive Surfaces

Purpose: keep inactive views from participating in active-view work.

Work:

- Classify each hidden surface:
  - Must preserve live state while hidden.
  - Can unmount and restore from durable/store state.
  - Can lazy mount on first open.
- Convert heavy sidebars to lazy-mounted bodies. Keep only a tiny stable shell mounted if layout requires it.
- Use the existing VCS pattern as the default: closed panels should not render heavy body content.
- For Agent, Workflow, and Execution panes, prefer one mounted active pane plus cached data stores over three always-active React subtrees.
- Preload likely-next heavy panes on idle, hover, or first project load if first-open latency becomes visible.
- Preserve focus and accessibility semantics when surfaces mount/unmount.
- Keep all newly visible controls user-facing; do not add development-only toggles.

Acceptance criteria:

- Closed sidebars do not re-render on unrelated App state changes.
- Inactive main panes do not process stream, editor, or repository updates unless they own the visible result.
- Opening a previously visited surface restores expected user state.
- First-open latency is measured and acceptable, or preloading is added.

Verification:

- Add focused component tests for open/close state preservation where practical.
- Re-run render-count profiles for view switching and sidebar toggles.
- Run the scoped frontend build.

## Phase 5: Replace Layout Polling With Observers

Purpose: remove continuous layout work from steady-state UI.

Work:

- Replace browser sidebar `requestAnimationFrame` resize polling with `ResizeObserver`.
- Trigger native browser resize on:
  - sidebar open
  - active tab change
  - observed viewport size/position change
  - transition end after width animation
  - explicit user resize drag frames
- During active drag, throttle resize IPC to one call per animation frame.
- When not dragging and no size changes occur, do not read layout every frame.
- Audit other components for steady-state `getBoundingClientRect`, layout reads in loops, or repeated measure/write cycles.

Acceptance criteria:

- Browser sidebar has no perpetual rAF loop in steady state.
- Native child webview remains correctly positioned across open, close, tab switch, tool overlay, and resize.
- IPC calls are bounded during active resizing.

Verification:

- Add tests for resize scheduling helpers if extracted.
- Manually validate through Tauri, not a normal browser.
- Re-run sidebar open/resize profile.

## Phase 6: Stabilize VCS And Repository Workflows

Purpose: make source-control updates feel quiet and deterministic.

Work:

- Memoize VCS callbacks from the App layer.
- Add a repository status revision or stable hash to the repository store.
- Make selected diff loading depend on project id, selected path, selected scope, and repository revision.
- Do not depend on a freshly mapped full entries array for diff loading.
- Cache diff results by project, revision, scope, and path.
- Avoid resetting selected diffs when repository status is effectively unchanged.
- Keep explicit refresh behavior user-facing and predictable.

Acceptance criteria:

- Selected diff does not reload during unrelated App renders.
- Repository badge/count updates do not disrupt the selected VCS file.
- VCS remains accurate after staging, unstaging, discard, commit, and refresh.

Verification:

- Add component/helper tests for selected scope derivation and diff cache invalidation.
- Run scoped tests for VCS helpers/components.
- Re-run repository event and VCS open profiles.

## Phase 7: Reduce Layout-Affecting Animation

Purpose: keep polish without paying avoidable layout cost.

Work:

- Keep the existing CSS-driven sidebar width animation and containment model.
- Audit remaining Motion usage for layout-affecting properties such as `width`, `max-width`, `height`, and layout springs in hot areas.
- Prefer transform/opacity for content reveal.
- For rail labels and dense toolbars, use stable grid/flex tracks and clip/fade content instead of animating intrinsic sizes.
- Disable nonessential transitions during tab/view switches using the existing layout-shifting guard pattern.
- Ensure reduced-motion behavior remains correct.

Acceptance criteria:

- Frequently toggled UI surfaces do not animate intrinsic layout properties unless the animation is isolated and measured.
- Tab/view switches do not trigger sidebar width transitions.
- Motion still feels polished but no longer causes repeated reflow through heavy children.

Verification:

- Re-run sidebar and view-switch profiles.
- Run scoped component tests if helper behavior changes.
- Run the scoped frontend build.

## Phase 8: Bundle And Startup Cleanup

Purpose: reduce initial parse/compile and first-interaction cost.

Work:

- Confirm CodeMirror and language chunks are not statically imported by the initial app entry.
- Lazy-load heavy optional surfaces:
  - editor workspace
  - browser tools
  - games sidebar
  - emulator sidebars
  - Solana workbench
  - workflow builder if not first screen
  - settings subpanels with expensive provider/model registries
- Keep common ShadCN primitives shared; avoid splitting tiny primitives into excessive chunks.
- Audit Shiki language/theme chunks and load only when the rendering path actually needs them.
- Add preload hints on user intent where needed, such as hovering the Editor tab or opening a project that last used the Editor view.

Acceptance criteria:

- Initial bundle no longer includes editor languages unless required by initial route/view.
- Heavy optional surfaces load on first use or preload by intent.
- Startup and first project open profiles improve or stay neutral.

Verification:

- Compare Vite build output before and after.
- Run the scoped frontend build.
- Run startup/first-interaction profile in Tauri.

## Phase 9: Regression Tests And Performance Gates

Purpose: keep the latency work from decaying.

Work:

- Add replay tests for:
  - high-volume runtime streams
  - repository status churn
  - editor typing/change handling
  - sidebar resize scheduling
  - VCS diff cache invalidation
- Add a lightweight performance smoke script that can run locally without opening a browser.
- Make the script report:
  - render/commit counts for fixtures where feasible
  - stream flush count
  - bundle chunk sizes
  - slowest measured tasks in the replay
- Do not make flaky wall-clock thresholds block normal development. Prefer structural assertions, counts, and generous budgets.
- Document the profiling procedure in this plan or a short companion doc if it becomes too long.

Acceptance criteria:

- The highest-risk regressions have tests or repeatable profiling checks.
- The performance smoke path is documented and can be run by a future agent.
- No product UI exists only for measurement.

Verification:

- Run the new smoke script.
- Run the scoped frontend test suite for changed helpers/components.
- Run any scoped Rust tests for changed Tauri commands or event projection.
- Run the scoped frontend build.

Implementation note (2026-05-02):

- Browser-free smoke path: `pnpm run performance:smoke`.
- Replay fixture: `client/src/performance/performance-smoke.test.tsx`.
- Companion procedure: `docs/ui-ux-performance-smoke.md`.
- The smoke path reports runtime stream flush counts, repository shell commit counts, editor cursor coalescing counts, sidebar resize scheduling counts, VCS diff cache invalidation counts, slowest replay task timings, and production bundle chunk sizes.

## Phase 10: Virtualize Long Lists And Dense Text Surfaces

Purpose: keep large repositories, long diffs, long sessions, and diagnostics feeds from turning into large DOM workloads.

Work:

- Inventory all user-facing scroll containers and classify which can exceed a few hundred rows:
  - file tree
  - VCS staged/unstaged file groups
  - unified diff lines
  - agent conversation turns if full history becomes visible
  - archived sessions
  - settings lists for providers, MCP, skills, plugins, diagnostics, notification dispatches
  - usage rows
  - Solana logs, audits, scenarios, IDLs, personas, wallets, and indexer results
- Build or adopt one small virtual list helper for fixed-height rows, with an escape hatch for measured variable-height rows.
- Flatten recursive trees into row models before rendering. Keep expansion state as data, not nested mounted components.
- Keep keyboard navigation, selection, drag/drop, aria labels, and context menus correct under windowing.
- Add `content-visibility: auto` only for coarse, below-fold rich sections where it is measurably helpful and does not break focus restoration.
- Keep small lists simple. Do not virtualize lists that cannot grow enough to matter.

Acceptance criteria:

- A project with thousands of files can open the explorer without mounting thousands of row components.
- A VCS status with hundreds or thousands of changed files keeps scrolling responsive.
- Large diffs first paint quickly and do not render every line outside the viewport.
- Keyboard navigation and screen-reader labels remain correct for windowed lists.

Verification:

- Add component/helper tests for visible range calculation, overscan, selection retention, and tree flattening.
- Add a performance smoke replay for large file tree and large VCS/diff datasets.
- Run scoped tests for the affected list/tree helpers and components.

## Phase 11: Bound Markdown, Diff Parsing, And Syntax Highlighting

Purpose: make rich text feel instant by showing plain content first and spending highlighting work only where it is visible and reusable.

Work:

- Add an LRU tokenization cache around `tokenizeCode`, keyed by language, Shiki theme, and a content hash.
- Add byte and entry budgets for the Shiki token cache so a large diff or transcript cannot retain unbounded token arrays.
- For VCS diffs, parse patch metadata once per patch key and tokenize only visible or near-visible lines.
- Replace all-line `Promise.all` tokenization for diffs with bounded batches scheduled by frame or idle time.
- For conversation markdown, memoize fenced segment parsing by message id/text revision and avoid re-tokenizing unchanged code blocks during assistant streaming.
- Render plain code immediately, then hydrate syntax highlighting when ready.
- Skip or partially highlight very large code blocks and very long diff lines, with a normal user-facing truncated/plain rendering state.
- Keep theme changes correct by invalidating only theme-dependent token entries.

Acceptance criteria:

- Streaming assistant text does not reparse or rehighlight older completed turns.
- Large diffs do not launch one Shiki task per line before first paint.
- Switching theme updates visible highlighting without freezing the UI.
- Plain fallback rendering remains readable when highlighting is skipped.

Verification:

- Add tests for token cache keys, eviction, theme invalidation, and large-block fallback.
- Add smoke metrics for markdown parse count and diff tokenization batch count.
- Compare large-diff open time before and after.

## Phase 12: Define IPC And Event Payload Budgets

Purpose: prevent the Tauri boundary from delivering payloads that are technically correct but too large or too frequent for smooth UI.

Work:

- Document per-command and per-event payload budgets:
  - runtime stream item
  - repository status
  - repository diff
  - project tree/listing
  - project search results
  - browser tab/events/console
  - emulator frame/status events
  - provider/model and settings registries
- Add lightweight payload-size instrumentation to the performance harness. Keep it out of visible UI.
- Split large commands into paged or path-scoped variants where needed:
  - load one file diff instead of a whole scope diff when the UI selects one file
  - page search results
  - page notification dispatches and diagnostics tables
  - lazily load tree children
- Coalesce noisy browser events such as load/title/url/console updates by tab and frame.
- Keep production runtime stream validation cheap, but keep full schema validation in tests and command responses.
- Add a durable dropped/truncated/backpressure diagnostic when payload budgets are exceeded.

Acceptance criteria:

- The UI never needs to parse/render a multi-megabyte command response for a normal interaction.
- Repeated native events are coalesced before React sees them.
- Payload truncation is explicit, user-facing where needed, and test-covered.

Verification:

- Add payload budget tests for representative command DTOs.
- Add smoke output for largest replay payloads and event rates.
- Run scoped TypeScript tests for adapter parsing and scoped Rust tests for changed command DTOs.

## Phase 13: Isolate And Cancel Backend Work

Purpose: keep filesystem, git, database, model catalog, and discovery work from competing with foreground UI responsiveness.

Work:

- Audit sync Tauri commands and classify them by expected duration and blocking behavior:
  - git status/diff/stage/unstage/discard/commit/fetch/pull/push
  - project file read/write/tree/search/replace
  - environment discovery and doctor report
  - provider model catalog refresh
  - MCP/skill/plugin discovery
  - Solana audit/log/indexer commands
  - emulator startup, frame capture, and SDK probes
- For long or filesystem-heavy commands, use a cancellable job contract and move blocking work to the appropriate backend worker path.
- Introduce per-project operation lanes where correctness needs ordering, such as git mutations and file mutations.
- Cancel stale work when the active project, selected file, selected diff, search query, or visible sidebar changes.
- Deduplicate identical in-flight commands by stable request key.
- Avoid registry/database reads on hot paths when an app-data-backed cached projection can be kept current.
- Emit progress only at bounded intervals for long jobs.

Acceptance criteria:

- Stale search, tree, diff, and provider refresh requests cannot apply over newer UI state.
- Git mutations remain ordered per project without blocking unrelated projects.
- Long background jobs surface progress without event spam.
- Closing a heavy sidebar cancels or detaches work that only served that surface.

Verification:

- Add scoped Rust tests for cancellation, deduplication, ordering, and stale-result rejection where helpers are added.
- Add frontend tests for "latest request wins" behavior.
- Profile Tauri while a long search or git operation runs and confirm typing/sidebar interaction remains responsive.

## Phase 14: Make Project Trees And Search Incremental

Purpose: avoid paying full-repository costs for explorer open, folder expand, quick search, and replace flows.

Work:

- Replace full recursive `list_project_files` with lazy root/folder listing.
- Keep expansion state and loaded children in a normalized frontend tree store.
- Apply gitignore/ignore filtering consistently in tree listing, search, and replace.
- Add a total node cap and a clear user-facing "too many entries" continuation state for enormous folders.
- Maintain a lightweight app-data-backed project file index if profiling shows repeated tree walks dominate.
- Debounce and cancel project search as the user types.
- Stream or page search results by file, with stable ordering and a truncation marker.
- For replace, operate on explicit reviewed result sets by default, not a freshly rescanned unbounded tree unless the user requests it.

Acceptance criteria:

- Opening the editor/explorer reads only the root and expanded folders needed for the current view.
- Expanding a large folder does not block the whole editor.
- Search results appear incrementally and stale queries cannot overwrite newer ones.
- Replace remains predictable and scoped to the user's visible/reviewed selection.

Verification:

- Add Rust tests for folder listing, ignore handling, caps, and virtual-path safety.
- Add frontend tests for incremental tree hydration and search cancellation.
- Add smoke fixtures for large synthetic repository trees.

## Phase 15: Govern Native Frames, Pointer Streams, And Hidden Loops

Purpose: ensure every frame-producing or pointer-heavy surface obeys foreground/background rules.

Work:

- Audit active `requestAnimationFrame`, pointermove, scroll, resize, observer, polling, and native frame event sources.
- Confirm games run their animation loops only while their game and sidebar are visible.
- Confirm emulator frame streams drop or coalesce frames when the sidebar is hidden, covered by another pane, or the UI is already behind.
- Keep browser tool overlay geometry reads scheduled and cached during pointer drawing/inspection.
- Throttle pointer-driven width updates in sidebars to one React/CSS update per animation frame.
- Persist resized widths only on drag end or idle, not during every pointermove.
- Add reduced-motion and battery-sensitive behavior where continuous animation is nonessential.

Acceptance criteria:

- Hidden games, emulator sidebars, browser tools, and optional panels do not keep active rAF loops or high-rate event listeners alive.
- Pointer drags do not produce more than one layout-affecting update per frame.
- Native frame streams have explicit drop/coalesce behavior under pressure.

Verification:

- Add tests for frame-loop lifecycle helpers where extracted.
- Add smoke metrics for active loop count and pointer-move coalescing.
- Manually validate emulator and browser behavior through Tauri, not a normal browser.

## Phase 16: Prioritize Input And Defer Derived Work

Purpose: keep typing, clicking, dragging, and scrolling ahead of derived calculations.

Work:

- Use `startTransition` or deferred values for non-urgent filters, search previews, model lists, diagnostics grouping, and settings table derivations.
- Keep text inputs locally controlled; commit expensive derived state after debounce, blur, submit, or transition.
- Move expensive per-render derivations into memoized selectors or precomputed store projections.
- Replace repeated `map/filter/sort` chains in hot render paths with indexed projections when lists are large or update often.
- Use stable event callbacks for props passed into memoized child components.
- Avoid reading and writing `localStorage` during active input loops; hydrate once and persist after the interaction settles.

Acceptance criteria:

- Typing in composer, search, file search, settings filters, commit message, and editor-related inputs never waits for heavy list derivation.
- Large settings/diagnostics lists can update filters without waking unrelated shell and agent surfaces.
- Resize and drag interactions persist state after the drag without synchronous storage writes in the move path.

Verification:

- Add focused tests for debounced/deferred commit semantics.
- Add Profiler smoke probes around large filterable lists.
- Re-run editor typing, sidebar drag, and settings filter profiles.

## Phase 17: Reduce Allocation And GC Pressure

Purpose: prevent low-millisecond churn from accumulating into pauses during stream-heavy and diff-heavy workflows.

Work:

- Add heap/retained-size snapshots to the performance procedure for stream burst, large diff, large tree, and settings open.
- Give caches byte budgets, not only entry budgets:
  - diff patches
  - Shiki tokens
  - project trees
  - provider/model catalogs
  - runtime streams by session
  - browser console/event logs
  - emulator frames
- Prefer normalized maps plus stable id arrays for high-churn data that changes one item at a time.
- Avoid repeated large string concatenation for growing assistant transcript text if profiling shows it dominates. Consider chunk storage plus joined display snapshots.
- Avoid creating short-lived arrays in hot selectors where a stable projection can be updated incrementally.
- Clear session/project-scoped caches when the project closes or a run/session is no longer reachable.

Acceptance criteria:

- Stream bursts and large diffs do not produce visible GC pauses in release profiling.
- Cache memory has explicit limits and eviction behavior.
- Project/session changes release large stale objects.

Verification:

- Add cache eviction tests for each bounded cache.
- Capture before/after heap snapshots for the targeted flows.
- Add smoke reporting for cache entry counts and approximate bytes where practical.

## Phase 18: Production Tauri Trace Loop

Purpose: make smoothness measurable in the real desktop runtime, not only in jsdom or a normal web environment.

Work:

- Define the repeatable release-build Tauri profiling procedure for macOS first, then other supported platforms.
- Capture UI-thread frame timing, long tasks, memory, IPC/event rates, and backend job timings for the core workflows.
- Add `performance.mark`/`performance.measure` or equivalent trace points for:
  - project load
  - agent stream flush
  - editor file open/save
  - VCS status/diff load
  - sidebar open/close/resize
  - search start/first result/complete
  - emulator frame received/rendered
- Keep trace points internal to tooling. Do not add visible debug UI.
- Store profiling notes and trace filenames in this plan or a companion doc after each optimization slice.
- Establish non-flaky budgets based on structural counts and generous duration bands, then tighten only with evidence.

Acceptance criteria:

- A future engineer can reproduce the same release Tauri profiles without using a normal browser.
- Every major optimization phase records the same before/after workflow evidence.
- Regressions can be detected by smoke counts first and release traces second.

Verification:

- Run the browser-free smoke script.
- Run the documented Tauri profiling workflow on the current platform.
- Attach or reference the resulting trace/profiling note.

## Suggested Implementation Order

1. Phase 0: baseline measurement.
2. Phase 2: stream coalescing, because agent runs likely produce the most frequent updates.
3. Phase 1: state ownership split for runtime and repository stores.
4. Phase 3: editor hot-path cleanup.
5. Phase 5: browser sidebar observer replacement.
6. Phase 6: VCS identity and cache stabilization.
7. Phase 4: inactive surface freeze/unmount.
8. Phase 7: animation cleanup.
9. Phase 8: bundle/startup cleanup.
10. Phase 9: performance gates.
11. Phase 11: markdown, diff parsing, and syntax highlighting, because it targets visible rich-text stalls after the first-wave render fixes.
12. Phase 10: virtualization for VCS, diffs, file tree, and any large diagnostics/settings lists.
13. Phase 12: IPC/event payload budgets, before changing command contracts.
14. Phase 14: incremental project tree and search, because it depends on the payload-budget decisions.
15. Phase 13: backend job isolation and cancellation for filesystem, git, search, and discovery work.
16. Phase 15: native frame, pointer stream, and hidden-loop governance.
17. Phase 16: input priority and deferred derived work across settings, filters, composer, and sidebars.
18. Phase 17: allocation/GC budget cleanup after the major data-flow shapes are stable.
19. Phase 18: production Tauri trace loop, then repeat after each later slice.

Phases 1 and 2 may overlap, but keep their commits/slices separate: first make event delivery bounded, then shrink the number of components that observe the delivered state.

## Slice Breakdown

### Slice A: Runtime Stream Buffer

Deliverable: buffered stream store with urgent bypass and tests.

Risk: missed or delayed critical agent events.

Verification focus: ordering, deduplication, urgent events, render commits under replay.

### Slice B: Runtime Store Selectors

Deliverable: App no longer owns high-frequency runtime stream state directly.

Risk: stale active run/session projection.

Verification focus: agent session switching, run start/stop, stream display, approval prompts.

### Slice C: Repository Store Selectors

Deliverable: shell badge, project metadata, VCS entries, and diffs subscribe to separate slices.

Risk: repository status desync after git operations.

Verification focus: branch label, counts, selected file, diff invalidation.

### Slice D: CodeEditor Hot Path

Deliverable: lazy languages plus debounced/delta editor change propagation.

Risk: dirty state or save semantics regress.

Verification focus: edit/save/file switch/read-only/theme/language changes.

### Slice E: Sidebar Lifecycle

Deliverable: heavy sidebar bodies lazy mount or unmount when closed.

Risk: losing user state on close.

Verification focus: close/reopen state, accessibility, first-open profile.

### Slice F: Browser Resize Scheduler

Deliverable: observer-based native webview resize.

Risk: native webview drift or stale position.

Verification focus: Tauri manual check plus resize scheduling tests.

### Slice G: Motion Cleanup

Deliverable: hot interactions avoid layout-affecting Motion patterns.

Risk: visual regressions or abrupt-feeling controls.

Verification focus: view switch, rail collapse, sidebar toggle, reduced motion.

### Slice H: Bundle Cleanup

Deliverable: initial chunks shed editor/language/optional-surface weight.

Risk: first-use loading delay.

Verification focus: build output, first-use preload behavior, Tauri startup.

### Slice I: Rich Text Work Budget

Deliverable: cached/batched markdown, Shiki, and diff tokenization.

Risk: stale highlighting after theme/language changes.

Verification focus: token cache invalidation, large code block fallback, diff first paint.

### Slice J: Virtualized Dense Surfaces

Deliverable: shared virtual list helper plus VCS/file-tree/diff adoption where measured.

Risk: broken keyboard navigation, focus, drag/drop, or screen-reader semantics.

Verification focus: visible range, overscan, selection retention, accessibility labels.

### Slice K: IPC Payload Budgets

Deliverable: documented payload limits plus command/event instrumentation.

Risk: truncating information users need.

Verification focus: explicit truncation states, payload-size smoke metrics, command DTO tests.

### Slice L: Incremental File Tree And Search

Deliverable: lazy folder listing, normalized tree store, cancellable/paged search results.

Risk: stale or incomplete project tree/search state.

Verification focus: ignore handling, virtual-path safety, latest-query-wins behavior.

### Slice M: Backend Job Isolation

Deliverable: cancellable/deduplicated worker path for long filesystem, git, discovery, and catalog operations.

Risk: operation ordering regressions, especially for git and file mutations.

Verification focus: per-project ordering, stale-result rejection, cancellation cleanup.

### Slice N: Native Frame And Pointer Governance

Deliverable: lifecycle gates and frame/pointer coalescing for browser tools, emulator, games, and resizable sidebars.

Risk: dropped frames or delayed pointer feedback in visible surfaces.

Verification focus: active-loop counts, frame drop/coalesce behavior, Tauri manual checks.

### Slice O: Input Priority

Deliverable: deferred large derivations and local-first input state for search, filters, composer, and settings.

Risk: delayed derived data appearing stale.

Verification focus: immediate keystroke feedback, debounced commit behavior, render counts.

### Slice P: Memory And GC Budget

Deliverable: byte-bounded caches, heap snapshot procedure, and cleanup on project/session disposal.

Risk: over-eviction causing repeated recomputation.

Verification focus: cache eviction tests, heap snapshots, repeated navigation profiles.

## Detailed Acceptance Checklist

- [ ] Baseline profile captured before optimization work.
- [ ] Runtime stream updates are buffered outside React.
- [ ] Critical runtime events bypass normal buffering.
- [ ] Runtime store subscriptions are selector-based.
- [ ] Repository status and diff state are selector-based.
- [ ] App shell does not subscribe to full runtime streams.
- [ ] Shell/titlebar is memoized or split enough to avoid irrelevant renders.
- [ ] CodeEditor does not call full-document `toString()` on every normal edit.
- [ ] CodeMirror languages are loaded on demand.
- [ ] Closed heavy sidebars do not render their bodies.
- [ ] Inactive main panes are frozen, unmounted, or subscribed only to stable external stores.
- [ ] Browser sidebar has no steady-state rAF layout polling.
- [ ] VCS diff loading depends on stable keys.
- [ ] Hot animations avoid uncontained intrinsic layout properties.
- [ ] Build output shows improved chunking.
- [x] Performance smoke checks exist for the highest-risk flows.
- [ ] All verification avoids normal-browser app execution.
- [ ] Large VCS file lists and diffs mount only visible rows plus overscan.
- [ ] File tree loading is incremental or otherwise proven cheap for large repositories.
- [ ] Search results are cancellable and paged/streamed for large repositories.
- [ ] Markdown and Shiki tokenization are cached, bounded, and scheduled off the first-paint path.
- [ ] IPC command/event payloads have documented size/frequency budgets.
- [ ] Long backend jobs can be cancelled or ignored safely when UI state changes.
- [ ] Hidden frame-producing surfaces have no active rAF/native-frame loops.
- [ ] Pointer-driven resize work is coalesced and storage persistence happens after drag/idle.
- [ ] Hot input filters use local-first state plus deferred heavy derivation.
- [ ] Caches have byte or entry limits and project/session cleanup paths.
- [ ] Release Tauri profiling notes exist for the optimized workflows.

## Rollout Notes

Do not try to optimize the whole UI in one patch. Latency work is easy to make worse by moving cost around invisibly. Each slice should:

1. Capture a baseline for the specific flow.
2. Make one architectural change.
3. Verify correctness.
4. Re-run the same profile.
5. Record the result in this file or a small companion note.

If a phase does not improve the measured flow, either revert that phase or document why the change is still valuable for a later phase.

## Open Questions

- Should Xero adopt a small store library, or keep selector stores implemented directly with `useSyncExternalStore`?
- What are the target machines for performance budgets: Apple Silicon only, Intel macOS, Windows, Linux, or all supported Tauri platforms?
- Should runtime stream validation be reduced in production once Rust-side contracts are covered by tests?
- Which view should be treated as the startup-critical first view after project load?
- Should editor persistence use deltas, debounced snapshots, or explicit save snapshots as the primary contract?
- What is the maximum project size Xero should optimize for: files, directories, changed files, search matches, and diff bytes?
- Should VCS expose a path-scoped diff command instead of loading and slicing scope-level patches in the frontend?
- Should project search results be pushed over a Tauri channel, returned by pages, or stored as a backend query session?
- Which caches should persist under OS app-data across app launches, and which should stay memory-only?
- What are acceptable memory ceilings for Shiki tokens, diff patches, runtime streams, and project tree caches?
- Which optional surfaces are allowed to preload on idle, and which must stay cold until explicit user intent?
- Which Tauri profiling tools should be canonical on macOS, Windows, and Linux?

## Final Definition Of Done

The optimization effort is done when a production Tauri build shows fewer unnecessary renders and smoother interaction profiles for agent streaming, editor typing, sidebar toggles, and VCS updates; all changed behavior is covered by scoped tests or replay checks; and no temporary debug UI remains.
