# React-to-Rust Performance Refactor Plan

Date: 2026-05-09

## Goal

Move work that is currently happening in the React/WebView layer into Rust when Rust already owns the data source, when the work scales with repository/file/event size, or when the result is project/app state that should live under OS app-data instead of WebView storage.

React should keep rendering, DOM measurement, ShadCN UI composition, small form parsing, and transient interaction state. Rust should own filesystem scanning, git diff projection, runtime stream projection, catalog validation, durable app/project state, and large payload budgeting.

## Audit Method

- Scanned all non-test TypeScript and TSX under `client/` for large files, loops/sorts/reduces, parsing, JSON serialization, localStorage, Shiki/Markdown/Dagre/ReactFlow, and IPC use.
- Cross-checked candidate React work against existing Tauri commands and Rust services in `client/src-tauri/src`.
- Used the `tauri-v2` and `react-best-practices` guidance: keep IPC typed, avoid blocking the UI thread, keep heavy data projection near the backend source, and keep React rendering predictable.

## Decision Rule

Move a React-side path to Rust when at least one of these is true:

- It iterates over repository files, full file contents, large diffs, runtime event streams, or large catalogs.
- Rust already produced the raw data, but React reparses, re-sorts, revalidates, or re-slices it.
- The output is a stable project/app projection that should be cached or persisted in app-data.
- React is using JSON stringify/parse as a generic algorithm for request keys, state persistence, budget measurement, or deep comparison.

Do not move:

- DOM measurement, viewport virtualization, hover/selection/editing state, ShadCN composition, CodeMirror editing, and small UI-only inputs.
- One-off sidebar widths or collapsed/expanded flags unless they become project-affecting state.

## Priority Overview

| Priority | Refactor | Main React files | Rust foundation |
| --- | --- | --- | --- |
| P0 | Runtime stream view projection | `client/src/lib/xero-desktop.ts`, `client/src/lib/xero-model/runtime-stream.ts`, `client/src/features/xero/use-xero-desktop-state/runtime-stream.ts` | `client/src-tauri/src/commands/subscribe_runtime_stream.rs`, project store agent events |
| P0 | Structured repository diff projection | `client/components/xero/vcs-sidebar.tsx` | `client/src-tauri/src/git/diff.rs`, `client/src-tauri/src/commands/get_repository_diff.rs` |
| P0 | Project file tree projection/pruning | `client/src/lib/file-system-tree.ts`, `client/components/xero/execution-view/use-execution-workspace-controller.ts` | `client/src-tauri/src/commands/project_files.rs` |
| P1 | File preview parsing/tokenization | `client/components/xero/execution-view/file-renderers.tsx`, `client/src/lib/shiki.ts` | `client/src-tauri/src/commands/project_files.rs` |
| P1 | Workflow agent graph projection/layout | `client/components/xero/workflow-canvas/build-agent-graph.ts`, `client/components/xero/workflow-canvas/layout.ts`, `client/components/xero/workflow-canvas/agent-visualization.tsx` | `client/src-tauri/src/commands/workflow_agents.rs` |
| P1 | Project load bundle projection | `client/src/lib/xero-model.ts`, `client/src/features/xero/use-xero-desktop-state/project-loaders.ts` | project snapshot/runtime/repository commands |
| P1 | Catalog/schema validation projection | `client/src/lib/xero-model/workflow-agents.ts`, `client/src/lib/xero-model/skills.ts`, `client/src/lib/xero-model/provider-models.ts` | `commands/workflow_agents.rs`, `commands/skills.rs`, autonomous skill runtime |
| P2 | IPC payload budgeting metadata | `client/src/lib/ipc-payload-budget.ts`, `client/src/lib/xero-desktop.ts` | existing Rust payload budget helpers |
| P2 | Solana log feed projection | `client/src/features/solana/use-solana-workbench.ts`, `client/components/xero/solana-log-feed.tsx` | `commands/solana/logs/*`, `commands/solana/mod.rs` |
| P2 | Developer storage rows formatting | `client/components/xero/settings-dialog/development-section.tsx` | `client/src-tauri/src/commands/development_storage.rs` |
| P2 | Durable app/project state | localStorage users listed below | OS app-data state commands |
| P2 | Request key/coalescing | `client/src/lib/backend-request-coordinator.ts` | Rust backend jobs/coalescing |

## Refactor Details

### P0. Runtime Stream View Projection

Current React work:

- `client/src/lib/xero-desktop.ts` parses every channel payload, records payload samples, batches delivery in JS, and enforces run/sequence checks.
- `client/src/lib/xero-model/runtime-stream.ts` normalizes every item, merges transcript chunks, merges tool/action/plan items, filters/sorts timeline arrays, caps arrays, and validates nested tool summaries.
- `client/src/features/xero/use-xero-desktop-state/runtime-stream.ts` buffers batches and repeatedly applies `mergeRuntimeStreamEvent`.

Why Rust should own it:

- Rust owns persisted agent events and live subscription order in `subscribe_runtime_stream.rs`.
- React currently rebuilds a view model from individual low-level events. That is exactly the kind of stream projection Rust can do once, with stable sequence cursors and bounded output.

Refactor target:

- Add a Rust `RuntimeStreamViewSnapshotDto` and `RuntimeStreamPatchDto`.
- Have `subscribe_runtime_stream` emit view patches instead of raw item DTOs for UI subscribers.
- Keep a raw-event command only for diagnostics/tests.
- Move transcript chunk coalescing, reasoning chunk coalescing, tool-call replacement, action-required replacement, plan replacement, status/failure/completion selection, and recent-array caps into Rust.
- Keep a cheap envelope assertion in React and reserve full Zod parse for tests/dev contract checks.

Acceptance checks:

- Runtime stream UI receives the same visible transcript/tool/activity/action/plan output.
- A replay of persisted events produces byte-for-byte stable view snapshots.
- High-volume assistant streaming no longer grows React merge time with historical item count.
- Existing runtime stream unit tests are ported or mirrored against Rust projection tests.

### P0. Structured Repository Diff Projection

Current React work:

- `client/components/xero/vcs-sidebar.tsx` hashes full patches, estimates diff-line cache bytes, parses unified diff text into line records, splits multi-file patches to extract file patches, and tokenizes visible diff lines with Shiki.
- `extractFilePatch` repeatedly scans/splits the full patch per file.

Why Rust should own it:

- Rust already builds the diff in `client/src-tauri/src/git/diff.rs`, then sends a flat patch string.
- Git/libgit2 exposes files, hunks, and lines before the patch is flattened, so Rust can return the structured representation without React reparsing text.

Refactor target:

- Replace or supplement `RepositoryDiffResponseDto.patch` with `files: RepositoryDiffFileDto[]`.
- Include per-file old/new paths, status, hunks, rows, old/new line numbers, line kind, truncation metadata, and stable cache keys.
- Keep raw patch text only for copy/export/fallback.
- Move per-file patch extraction into Rust.
- Consider Rust-side syntax tokenization later, but first remove unified diff parsing/slicing from React.

Acceptance checks:

- File-level diff view renders from Rust rows without calling `parseDiffLines`.
- Truncated diff behavior is preserved and visible per file/hunk.
- React never scans the full multi-file patch to render one selected file.

### P0. Project File Tree Projection And Pruning

Current React work:

- `client/src/lib/file-system-tree.ts` maps recursive DTOs, stores nodes by path, recursively materializes trees, estimates memory, recursively counts descendants, sorts candidate folders, prunes folders, and recursively searches nodes.
- `client/components/xero/execution-view/use-execution-workspace-controller.ts` applies listings, trims, materializes, and finds nodes after folder loads.

Why Rust should own it:

- `client/src-tauri/src/commands/project_files.rs` already scans the filesystem with `ignore::WalkBuilder`, has node and payload budgets, and emits budget diagnostics.
- React should not need to re-budget a tree that Rust already bounded.

Refactor target:

- Add a Rust `ProjectFileTreeViewDto` or `ProjectFileTreePatchDto` with flat `nodesByPath`, `childPathsByPath`, root path, loaded paths, selected/open path hints, total/truncated/omitted stats, and a server-side generation id.
- Move memory-budget pruning and protected-path pruning to Rust.
- Keep React expansion state and render materialization small, ideally only for visible/expanded paths.
- Persist larger tree/index state under OS app-data when needed, not `.xero/` and not WebView localStorage.

Acceptance checks:

- Large repositories do not trigger React-side recursive tree pruning.
- Folder expansion applies a Rust patch and renders only visible rows.
- Existing project tree truncation/budget tests cover Rust output.

### P1. File Preview Parsing And Tokenization

Current React work:

- `MarkdownPreview` renders full markdown text with `ReactMarkdown` and `remarkGfm`.
- `CsvPreview` parses every character of CSV/TSV content in `parseDelimitedText`.
- Markdown code blocks and diff/code views tokenize with Shiki in the WebView.
- Relative markdown asset paths are normalized/sanitized in React.

Why Rust should own more of it:

- `read_project_file` already reads, hashes, limits, classifies renderer kind, and issues media preview URLs.
- For saved files, Rust can parse and return bounded preview DTOs. React can keep a frontend fallback for unsaved editor text.

Refactor target:

- Extend `ReadProjectFileResponseDto` with optional `preview` variants:
  - `csv`: headers, rows, total row/column counts, truncation flags.
  - `markdown`: resolved safe asset refs, heading/table/code-block summaries, optional pre-chunked render model.
  - `code`: language, byte count, line count, optional Rust-generated token spans later.
- Use Rust `csv` crate for CSV/TSV preview.
- Use Rust path APIs for markdown relative asset normalization.
- Consider `syntect` or tree-sitter for Rust-side token spans if Shiki remains a measurable WebView cost.

Acceptance checks:

- Opening large CSV/TSV files does not run a WebView character parser.
- Saved markdown asset resolution is consistent with Rust project path validation.
- Unsaved editor preview still works through a small React fallback.

### P1. Workflow Agent Graph Projection And Layout

Current React work:

- `build-agent-graph.ts` partitions tools, deduplicates DB touchpoints, computes barycenter ordering, emits graph nodes/edges, and builds visual categories.
- `layout.ts` groups nodes, measures lanes, greedily packs tool frames, wraps DBs, lays sections/consumes, and can call Dagre.
- `agent-visualization.tsx` persists node positions in localStorage and recomputes layout from measured sizes.

Why Rust should own more of it:

- `commands/workflow_agents.rs` already assembles agent detail, authoring catalog, tool categories, DB tables, output contracts, and tool-pack manifests.
- Graph classification and deterministic ordering are domain projection, not UI rendering.

Refactor target:

- Add `get_workflow_agent_graph_projection` returning graph nodes, edges, category/group metadata, DB/tool/output ordering, and default layout coordinates.
- Move tool grouping, DB touchpoint ordering, edge ID construction, and barycenter ordering to Rust.
- Keep ReactFlow rendering, DOM size measurement, drag/selection, and final measured-size nudges in React.
- Persist user drag overrides through Rust app-data commands keyed by agent ref, not localStorage.

Acceptance checks:

- React no longer imports `build-agent-graph.ts` for production graph construction.
- Same graph renders for built-in and custom agents from a Rust snapshot.
- User position overrides survive restart through app-data.

### P1. Project Load Bundle Projection

Current React work:

- `mapProjectSnapshot` maps summary, phases, repository, approvals, verification, resume history, sessions, selected session, notification broker, pending approval count, decision outcome, and autonomous run.
- `project-loaders.ts` fetches snapshot, repository status, broker, routes, runtime, runtime run, autonomous run, then repeatedly applies view-model transforms and dispatches multiple state updates.

Why Rust should own it:

- Rust already owns project records, runtime records, notification dispatches, repository status, and persisted autonomous state.
- The React loader is doing backend view composition and fallback logic that can be returned as one project-load bundle.

Refactor target:

- Add `get_project_load_bundle(projectId)` returning:
  - `ProjectDetailViewDto`
  - selected agent session id
  - repository status/diff summary
  - runtime session/run/autonomous summary
  - notification broker/routes summary
  - load diagnostics per section
- Keep React responsible for stale-response guards and UI transitions.

Acceptance checks:

- Initial project selection uses one bundle command for the common path.
- Pending approval counts and selected session match current React projection.
- Existing UI fallback behavior is represented as structured diagnostics.

### P1. Catalog And Schema Validation Projection

Current React work:

- `workflow-agents.ts` has extensive Zod validation for tool-pack manifests, health reports, authoring catalogs, template references, creation flows, availability, and constraint explanations.
- `skills.ts` and provider-model schemas also perform large contract validation in the WebView.
- `xero-desktop.ts` runs `schema.parse(response)` for every command.

Why Rust should own more of it:

- Rust already discovers plugins/skills and builds workflow-agent catalogs.
- Runtime/catalog validation is backend/domain contract work. React should receive already-normalized DTOs and only defend the IPC boundary lightly in production.

Refactor target:

- Move catalog uniqueness/reference checks into Rust builders/tests.
- Emit a `contractVersion` plus `diagnostics` for catalog problems instead of requiring React to rediscover them.
- Gate full Zod response parsing behind test/dev flags for large catalogs and streams.
- Keep small request-schema validation in React for form/input safety.

Acceptance checks:

- Invalid tool-pack/authoring catalog fixtures fail Rust tests.
- Production catalog load does not traverse large catalog schemas with Zod on every open.
- Dev/test can still enable full Zod validation for contract drift detection.

### P2. IPC Payload Budgeting Metadata

Current React work:

- `ipc-payload-budget.ts` estimates payload size with `JSON.stringify(payload).length * 2` for most command/event samples.
- Runtime stream items use an approximate JS traversal.

Why Rust should own it:

- Rust can measure serialized payload size before IPC and already has payload-budget helpers for several commands.
- React-side stringification of large responses is itself a performance cost.

Refactor target:

- Add or standardize `payloadBudget` metadata across all large command responses and channel/event payloads.
- Make React consume Rust-provided observed/truncated/max values.
- Disable WebView JSON.stringify payload sampling outside focused diagnostics/tests.

Acceptance checks:

- No large command response is stringified in React just to estimate IPC size.
- Payload budget warnings still appear in diagnostics.

### P2. Solana Log Feed Projection

Current React work:

- `use-solana-workbench.ts` dedupes, maps, sorts, and caps log entries in `mergeLogEntries`.
- `solana-log-feed.tsx` sorts and filters entries and recomputes error/event counts.

Why Rust should own more of it:

- `commands/solana/logs/*` already has `LogBus`, filters, bounded rings, decoded entries, and `recent`.
- React should ask for a filtered/counted page rather than reshaping the whole feed every render.

Refactor target:

- Add a `solana_logs_view` command returning entries ordered newest-first or chronological by request, plus counts by filter.
- Add filter options for `errors`, `events`, cluster, program ids, and limit/cursor.
- Keep React input parsing and display state.

Acceptance checks:

- Feed tabs no longer sort/filter the full in-memory list in React.
- Counts come from Rust for the same filtered window.

### P2. Developer Storage Rows Formatting

Current React work:

- `development-section.tsx` parses storage overview/table responses with Zod.
- Row rendering calls `formatStorageValue`, including `JSON.stringify(value)`, for every visible cell.

Why Rust should own more of it:

- `developer_storage_read_table` already applies limit/offset and knows source column metadata.
- Rust can return display strings and raw redacted values once.

Refactor target:

- Extend `DeveloperStorageTableRowsDto` rows with `displayValues` per column.
- Apply redaction, JSON formatting, max cell length, and type labels in Rust.
- Keep React table rendering and pagination controls.

Acceptance checks:

- React table render does not stringify arbitrary cell values.
- Sensitive/redacted values are consistently formatted at the command boundary.

### P2. Durable App/Project State Currently In localStorage

Current React work:

- Project-affecting or durable UI state is stored in WebView localStorage in several places:
  - agent workspace layout in `use-xero-desktop-state.ts`
  - pinned sessions in `agent-sessions-sidebar.tsx`
  - workflow graph positions and snap-to-grid in `agent-visualization.tsx`
  - selected workflow agent ref in `use-workflow-agent-inspector.ts`
  - custom themes and shortcut bindings in feature providers

Why Rust should own selected cases:

- Project/app state belongs under OS app-data in this app. `.xero/` is legacy and WebView localStorage is not a good durable project-state store.
- JSON parse/stringify here is smaller than the other hot paths, so this is mainly correctness and startup consistency, with secondary performance benefits.

Refactor target:

- Add app-data preference/state commands for project-scoped layout, pinned sessions, selected agent refs, workflow graph positions, custom themes, and shortcut bindings.
- Keep pure window chrome widths/collapsed sidebar flags in localStorage unless they need cross-window or project semantics.

Acceptance checks:

- Project-scoped layout and pinned sessions survive WebView storage resets.
- No new project state is written under `.xero/`.

### P2. Backend Request Key And Coalescing

Current React work:

- `backend-request-coordinator.ts` recursively stable-stringifies arbitrary command args to build dedupe keys.
- `invokeTypedDeduped` applies that generic stringify to all deduped commands.

Why Rust or explicit keying should own more of it:

- For heavy search/tree/workspace operations, request identity is domain-specific and Rust already has latest-job cancellation lanes.
- Generic recursive stringify becomes costly if callers pass large path/filter arrays or nested objects.

Refactor target:

- Replace generic `stableBackendRequestKey([command, args])` with explicit key builders per command.
- For long-running commands, prefer Rust backend job keys/cancellation over WebView-side generic dedupe.

Acceptance checks:

- No large request object is recursively stringified for dedupe.
- Existing latest/deduped behavior is preserved for project tree, search, workspace index, repository status, and provider catalog.

## Checked Areas That Mostly Already Belong In Rust

- Workspace index/query/explain are already Rust-backed in `commands/workspace_index.rs`.
- Project search/replace are already Rust-backed in `commands/search_project.rs`.
- Repository status is Rust-backed; the main missing piece is structured diff rows.
- Solana core operations are mostly Rust-backed; the remaining React-side issue is feed/window projection, not chain/RPC logic.
- Media/PDF/image preview URLs are Rust-backed through project asset grants; keep the actual media element rendering in React.

## Suggested Implementation Order

1. Runtime stream projection: highest impact because it affects live agent runs and repeated event updates.
2. Structured repository diffs: removes repeated full-patch parsing and opens better truncation/caching.
3. Project file tree projection: removes recursive WebView tree budgeting on large repos.
4. File preview DTOs for CSV and markdown assets: removes full-text parsing from common file-open paths.
5. Workflow agent graph projection and app-data position persistence.
6. Project load bundle projection.
7. Catalog/schema validation migration and Zod production gating.
8. Remaining P2 cleanup: IPC budget metadata, Solana feed view, developer storage display strings, explicit request keys.

## Test Strategy

- Add Rust unit tests for each moved projection using existing DTO fixtures or small fixtures.
- Keep focused frontend tests that assert React renders DTOs correctly and no longer calls removed parser paths.
- For Tauri commands, run scoped Cargo tests only for touched Rust modules, one Cargo command at a time.
- For React changes, run scoped Vitest suites for the affected component/model files.
- Do not add temporary debug UI. All verification should be unit/e2e or command-level tests.
