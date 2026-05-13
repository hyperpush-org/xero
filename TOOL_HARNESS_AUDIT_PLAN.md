# Tool Harness Audit And Backend-Only Improvement Plan

Audience: engineers working on the agent harness, tool registry, runtime dispatch, and model-visible tool output.

Post-read action: pick the first implementation slice, add the contract tests that freeze the current surface, then ship improvements in small backend-only increments.

## Guardrails

- Do not add new UI.
- Do not add temporary debug UI or test UI.
- Treat UI work as out of scope unless a backend change becomes unusable without a tiny existing-surface text adjustment.
- Keep new project state out of `.xero/`; use OS app-data for persisted artifacts, traces, caches, and large raw tool results.
- This is a new application, so do not build compatibility layers unless explicitly requested.
- Prefer scoped Rust tests and formatting for touched modules.
- Only run one Cargo command at a time.

## Executive Summary

The harness already has a broad and well-structured tool surface: repo-aware filesystem reads, text mutation, command wrappers, process control, web and browser tooling, MCP, skills, project context, code intelligence, Solana tools, and model-visible result compaction.

The largest improvement opportunity is not adding more high-level product features. It is tightening the core contracts that every agent turn depends on:

- Make filesystem observation cheaper and more precise so agents stop reaching for shell commands to answer simple file questions.
- Make writes safer by adding consistent guards, preview behavior, and transaction semantics across all mutating filesystem tools.
- Make model-visible outputs purpose-built rather than generic JSON truncations.
- Make the registry, policy metadata, descriptors, and tests impossible to drift.
- Add pagination and artifact references so the model sees the useful part of large outputs without losing access to the rest.
- Add evals that measure whether tool improvements reduce retries, output bloat, stale writes, and unnecessary command use.

No new UI is required for this plan.

## Current Tool Surface Snapshot

This inventory is based on the current backend tool registry and autonomous runtime modules.

| Area | Current tools |
| --- | --- |
| Core repository observation | `read`, `search`, `find`, `list`, `file_hash`, `git_status`, `git_diff`, `workspace_index` |
| Filesystem mutation | `edit`, `write`, `patch`, `delete`, `rename`, `mkdir`, `notebook_edit` |
| Tool discovery and access | `tool_access`, `tool_search` |
| Agent state and coordination | `todo`, `agent_coordination`, `agent_definition`, project context read/write tools |
| Code intelligence | `code_intel`, `lsp`, project context search/get |
| Command execution | `command_probe`, `command_verify`, `command_run`, `command_session`, legacy `command` |
| Process and system tools | `process_manager`, system diagnostics observe/privileged tools, macOS automation, Windows PowerShell |
| External information | `web_search`, `web_fetch` |
| Browser and device control | browser observe/control tools, emulator tools |
| Extensibility | MCP list/read/prompt/call tools, dynamic `mcp__...` routes, skills, subagents |
| Domain tooling | Solana cluster, logs, transaction, simulation, IDL, deploy, audit, replay, indexer, cost, docs, and related tools |
| Reserved or legacy surface | `harness_runner`, aggregate/internal tool names, and older compatibility descriptors |

## What Is Working Well

- The runtime already distinguishes observe, write, destructive write, command, process control, browser control, device control, external service, skill runtime, and agent delegation effects.
- `read` has useful safety affordances: repo-relative default behavior, explicit system path mode, line ranges, line hashes, image previews, binary metadata, and size caps.
- `edit` and `patch` already support strong stale-write protection through exact text, file hashes, and line hashes.
- Command wrappers separate cheap read-only probes, verification commands, broad command runs, and long-running sessions.
- The dispatch layer already centralizes policy checks, sandbox decisions, write preflight, code change capture, checkpoints, and rollback hooks.
- Tool descriptors are registered through a single registry and can be filtered by task mode, explicit tool requests, runtime availability, and policy.
- Model-visible result compaction already exists and has specialized handling for reads, diffs, patches, commands, web fetches, workspace index, and project context.

## Main Audit Findings

### 1. The Filesystem Surface Is Good But Too Thin

Agents still need shell fallbacks for common filesystem questions because the native filesystem tools lack a few core primitives.

Examples:

- Check file existence, kind, size, modified time, permissions, and symlink target without reading content.
- Read several small files in one bounded call.
- Get a compact directory tree with omitted counts and ignore reasons.
- Hash a whole directory or a matched file set.
- Copy files or directories with the same policy and stale guards as other mutations.
- Apply a mixed create/update/delete/rename/copy plan atomically enough that failures do not leave partial edits.

### 2. Mutation Guards Are Inconsistent

`edit`, `patch`, `delete`, and `rename` have stronger guard behavior than `write` and `mkdir`.

High-value fixes:

- Add `expectedHash` to `write`.
- Add `createOnly` and `overwrite` modes to `write`.
- Add consistent `preview` behavior to all mutating filesystem tools.
- Add directory digest guards for recursive deletes and future directory-level operations.
- Return current hash and compact conflict context when stale-write checks fail.

### 3. Catalog Metadata Can Drift

The active tool catalog, descriptors, effect classes, risk labels, group definitions, and model-visible descriptions are hand-maintained in several places.

Observed risk:

- Reserved or legacy tool names can remain visible in descriptors or group text after the runtime no longer allows them.
- Risk label vocabulary and effect-class vocabulary can diverge.
- Dynamic MCP tools are correctly supported, but their generated descriptors and result projections need the same contract guarantees as built-ins.

### 4. Model-Visible Outputs Need A Stronger Contract

The current compactor is useful, but it still depends on per-tool special cases plus generic JSON pruning. Large results can lose navigability, while some low-value fields can still consume context.

Needed improvements:

- Every tool result should declare a model projection shape.
- Every truncation should include omitted counts and a continuation path.
- Large raw values should persist to app-data artifacts instead of being inlined.
- Model-visible outputs should include stable provenance fields, such as path, hash, range, command id, or artifact id.
- The model should see suggested next actions for recoverable errors.

### 5. Search, List, And Command Outputs Need Pagination

The runtime currently caps many outputs to fixed item and character limits. That prevents runaway context usage, but it also encourages repeated broad calls because the model cannot ask for page 2 or a narrower continuation from the original result.

Needed improvements:

- Add continuation tokens for large search, find, list, tree, command, process, MCP, browser, and domain-tool outputs.
- Store full raw results in app-data.
- Add a generic `result_page` or equivalent result-continuation tool that retrieves additional slices by artifact id.

### 6. Command Tools Overlap With Native Tools

The command wrappers are useful and should remain. The gap is that native tools should be good enough for simple file inspection so the model does not use `cat`, `ls`, `find`, or `rg` reflexively inside `command_probe`.

Desired direction:

- Native tools are canonical for filesystem observation and mutation.
- Commands are canonical for build systems, test runners, package managers, project scripts, and user-requested shell behavior.
- Long-running processes flow through process/session tools, not ad hoc command invocations.

### 7. Evals Are Needed Before And After Tool Changes

The harness needs tests that catch regressions in tool usefulness, not only tests that validate individual tool behavior.

Key metrics:

- Model-visible bytes per successful task.
- Tool-call retry rate.
- Stale-write failure recovery rate.
- Shell fallback rate for filesystem tasks.
- Search/list follow-up rate.
- Output truncation rate by tool.
- Mutation rollback success rate.
- Tool descriptor drift count.

## Proposed New Core Filesystem Tools

### P0: `stat`

Purpose: answer file metadata questions without reading content or shelling out.

Inputs:

- `path`
- `followSymlinks`
- `includeGitStatus`
- `includeHash` for files below a bounded size

Output:

- normalized repo-relative path
- kind: file, directory, symlink, missing, other
- size
- modified time
- permissions summary
- symlink target when applicable
- file hash when requested and allowed
- git status when requested

Acceptance criteria:

- Does not return file content.
- Handles missing paths as a successful observation with `kind: "missing"` unless the caller requests strict mode.
- Uses the same repo boundary policy as `read`.

### P0: `read_many`

Purpose: read a bounded set of small files in one call.

Inputs:

- `paths`
- optional per-file or global line range
- `maxBytesPerFile`
- `maxTotalBytes`
- `includeLineHashes`

Output:

- one result per path, each shaped like a compact `read` result
- per-file errors without failing the whole batch unless the input is invalid
- omitted byte and omitted file counts

Acceptance criteria:

- Applies the same text/binary/image rules as `read`.
- Preserves path order.
- Never inlines image previews or binary excerpts into model-visible output unless explicitly allowed by the existing read policy.

### P0: `list_tree`

Purpose: return a compact, readable directory tree rather than a flat entry list.

Inputs:

- `path`
- `maxDepth`
- `maxEntries`
- include/exclude globs
- `includeGitStatus`
- `showOmitted`

Output:

- tree-shaped directory structure
- file and directory counts
- omitted counts by reason: depth, entry cap, ignored directory, binary/generated, permission
- optional git status summary

Acceptance criteria:

- No decorative or UI-specific output.
- Model projection should be text-first and compact.
- Result should be stable enough for tests without depending on filesystem traversal accidents.

### P0: `directory_digest`

Purpose: guard recursive operations and quickly answer whether a subtree changed.

Inputs:

- `path`
- include/exclude globs
- `maxFiles`
- `hashMode`: metadata-only, content-hash, git-index-aware

Output:

- deterministic digest
- file count
- directory count
- total bytes
- omitted count and reasons
- optional manifest artifact id for full details

Acceptance criteria:

- Uses deterministic ordering.
- Has clear behavior for ignored and generated directories.
- Can be used as an expected guard by `delete`, future `copy`, and future transaction tools.

### P1: `copy`

Purpose: copy files or directories under the same policy system as other filesystem mutations.

Inputs:

- `from`
- `to`
- `recursive`
- `expectedSourceHash` for files
- `expectedSourceDigest` for directories
- `overwrite`
- `expectedTargetHash` when overwriting a file
- `preview`

Output:

- planned operations
- copied bytes
- created directories
- skipped/omitted entries
- compact summary

Acceptance criteria:

- Refuses implicit overwrite.
- Preserves basic file contents and permissions where reasonable.
- Does not follow symlink targets by default.

### P1: `fs_transaction`

Purpose: apply a mixed filesystem plan with validation before mutation and rollback on failure.

Supported operations:

- create file
- replace file
- edit file by exact range or search/replace
- delete file
- delete directory with digest guard
- rename
- copy
- mkdir

Inputs:

- `operations`
- `preview`
- `stopOnFirstError`
- per-operation expected hash or digest guards

Output:

- validation summary
- planned operation list
- compact diffs for text changes
- rollback status
- per-operation result

Acceptance criteria:

- All validation runs before any write unless explicitly impossible.
- Rollback attempts are recorded when a partial failure occurs.
- Model-visible output summarizes changed paths, not full file content.

### P2: Structured Edit Tools

Purpose: avoid fragile string editing for structured config files.

Candidate tools:

- `json_edit`
- `toml_edit`
- `yaml_edit`
- package manifest helper for common dependency/script edits

Inputs:

- `path`
- `expectedHash`
- typed operations such as set, delete, insert, append unique, sort keys
- formatting preservation mode when parser support allows it
- `preview`

Acceptance criteria:

- Uses real parsers rather than regex.
- Returns semantic diffs plus compact text diff.
- Refuses unsupported syntax instead of silently reformatting large files.

## Improvements To Existing Tools

### `read`

- Add a result cursor for long files so follow-up ranges can use stable continuation ids.
- Add optional `aroundPattern` for small targeted reads around a match.
- Add generated/minified detection and make the model-visible output say why content was omitted.
- Return stable file metadata in every result: kind, size, modified time, hash when available, line count when text.

### `search`

- Add pagination with continuation ids.
- Group matches by file in the model projection.
- Return a matched-file summary before individual matches.
- Add optional `filesOnly` mode.
- Include ignore/omission summaries so the model understands what was not searched.
- Add stronger schema bounds for context lines, max results, and preview characters.

### `find`

- Add explicit modes for glob, name, extension, and path-prefix matching.
- Add pagination.
- Return directory and file counts separately.
- Include ignored/omitted directory summaries.

### `list`

- Keep `list` as the flat listing tool.
- Add pagination and stable sorting controls.
- Include aggregate child counts and omitted reasons.
- Encourage `list_tree` for tree-shaped summaries.

### `file_hash`

- Add optional multi-file mode or supersede with `stat` plus `directory_digest`.
- Return algorithm metadata and byte count.
- Support artifact-backed manifests for large matched file sets.

### `edit`

- Keep exact expected text and line-hash guards.
- Add optional preview mode for consistency with other mutators.
- Improve conflict errors with current nearby lines and current line hashes.

### `write`

- Add `expectedHash`.
- Add `createOnly`.
- Add `overwrite` with explicit true/false behavior.
- Add `preview`.
- Return compact diff for replacements and content summary for creates.

### `patch`

- Keep search/replace and multi-operation support.
- Make result output include per-file guard status, per-file changed ranges, and rollback status.
- Support continuation artifact for very large diffs.

### `delete`

- Add `expectedDigest` for recursive directory deletes.
- Add `preview`.
- Return deleted count and byte estimate before/after.
- Refuse recursive deletes without either preview confirmation semantics or a digest guard.

### `rename`

- Add optional guarded replace mode:
  - `overwrite: true`
  - `expectedTargetHash`
- Return source and target metadata.
- Keep default behavior as refusal when target exists.

### `mkdir`

- Add `parents` and `existOk` as explicit flags if not already encoded.
- Add preview.
- Return created path list rather than only final status.

### `tool_access`

- Remove stale reserved tool references from model-visible group text.
- Show effect class, risk class, runtime availability, and activation group for each listed tool.
- Add invariant tests that every named group points to real tools and every real tool has a group or an explicit internal-only marker.

### `tool_search`

- Rank by current task mode and likely next action.
- Hide reserved/internal-only tools.
- Include short examples only when they save a tool call.
- Add a compact "why this matched" field.

### Command And Process Tools

- Keep `command_probe` narrow and read-only.
- Keep `command_verify` scoped to known verification commands.
- Make `process_manager` the preferred path for long-running processes, ports, and background jobs.
- Add output continuation artifacts for large stdout/stderr.
- Make command result projections emphasize exit status, command intent, changed files, and next useful verification steps.

### MCP And Skill Tools

- Treat external descriptors, schemas, and results as untrusted input.
- Cap schema text and descriptor text before they enter the model-visible registry.
- Add per-dynamic-tool projection metadata.
- Store large external results as artifacts and show concise summaries with continuation ids.
- Add tests that dynamic `mcp__...` descriptors cannot shadow built-in tools.

## Model-Visible Output Contract

Introduce an explicit projection contract for every tool result.

Each result should have:

- `schemaVersion`
- `toolName`
- `status`
- `summary`
- `forModel`
- `artifacts`
- `omissions`
- `continuation`
- `provenance`
- `suggestedNextActions`

### Projection Rules

- Full raw output is persisted under app-data when it exceeds model-safe limits.
- Model output gets only the smallest useful representation.
- Base64 blobs, large schemas, command traces, registry dumps, previews, and raw manifests are excluded from model-visible output by default.
- Every truncation includes what was omitted and how to retrieve more.
- Errors are compact, but actionable.
- Recoverable errors include suggested next actions.

### Tool-Specific Projection Tests

Add golden or snapshot-style tests for:

- text `read`
- binary `read`
- image `read`
- large `search`
- large `list`
- `git_diff`
- `patch`
- `edit`
- `write`
- command output with huge stdout
- command output with JSON streams
- dynamic MCP result
- skill result
- Solana result

## Registry And Policy Cleanup

### Required Invariants

Add tests that fail when:

- a built-in descriptor has no catalog metadata
- a catalog tool has no descriptor unless explicitly internal
- a tool group references an unknown tool
- a descriptor mentions a reserved tool as available
- a dynamic MCP route shadows a built-in tool
- a tool has an unknown effect class
- risk labels use vocabulary outside the approved set
- an agent mode exposes a mutation tool without policy coverage
- a model-visible projection falls back to generic JSON for a tool that has a specialized contract

### Reserved And Legacy Tool Names

Clean up the reserved Test-agent runner surface.

Desired behavior:

- Reserved tools are not returned by `tool_search`.
- Reserved tools are not suggested by `tool_access`.
- Reserved tools are not visible in group descriptions.
- Internal compatibility names are documented in backend tests, not taught to the model as first-class options.

### Schema Quality

Improve descriptor schemas:

- Add `minimum`, `maximum`, `minItems`, `maxItems`, and enum bounds everywhere runtime caps already exist.
- Add SHA-256 string patterns for expected hash fields.
- Make path descriptions consistently say repo-relative by default.
- Add schema examples only when they reduce ambiguity.
- Keep schemas short enough to avoid descriptor bloat.

## Backend Implementation Plan

### Phase 0: Freeze The Current Surface

Goal: create tests that describe the existing tool surface before changing it.

Work:

- Add an inventory test for all built-in tools.
- Add group-to-tool invariant tests.
- Add effect/risk vocabulary tests.
- Add descriptor schema validity tests.
- Add a model-visible projection smoke test for each major tool family.

Done when:

- The tests expose current drift without changing runtime behavior.
- Reserved/internal tools are explicitly classified.

### Phase 1: Catalog, Descriptor, And Projection Hygiene

Goal: remove drift and make the registry trustworthy.

Work:

- Remove stale reserved-tool mentions from model-visible descriptions.
- Normalize risk/effect vocabulary.
- Add missing schema caps and path/hash descriptions.
- Add a backend-only projection contract type.
- Convert generic compact JSON fallbacks into explicit projections for high-volume tools.

Done when:

- Tool discovery cannot recommend unavailable tools.
- Tool descriptors are smaller, stricter, and more accurate.

### Phase 2: Filesystem Observation Tools

Goal: reduce shell fallbacks for routine file inspection.

Work:

- Implement `stat`.
- Implement `read_many`.
- Implement `list_tree`.
- Implement `directory_digest`.
- Add model-visible projections and tests for each.

Done when:

- Common tasks like "show me these files", "what is in this folder", and "did this subtree change" can be completed without command tools.

### Phase 3: Safer Filesystem Mutation

Goal: make all filesystem writes consistently guarded and previewable.

Work:

- Add hash and mode guards to `write`.
- Add preview to `edit`, `write`, `delete`, `rename`, and `mkdir`.
- Add directory digest guards to recursive delete.
- Implement `copy`.
- Implement `fs_transaction` if repeated multi-file mutation patterns justify it after the smaller changes.

Done when:

- Every mutating filesystem tool can explain what it will change before changing it.
- Stale write failures give the model enough context to recover safely.

### Phase 4: Pagination And Artifacts

Goal: stop losing useful data to hard truncation while keeping model context lean.

Work:

- Persist large raw tool outputs to app-data artifacts.
- Add continuation ids to search, find, list, list_tree, command, process, MCP, skill, browser, emulator, and Solana results where relevant.
- Add a generic result-continuation read path.
- Add omitted count and continuation metadata to all truncated projections.

Done when:

- Large outputs no longer require repeating the original expensive tool call.
- The model can intentionally request more data rather than guessing.

### Phase 5: Structured Edits And Dynamic Tool Safety

Goal: make high-risk text edits and dynamic external tools more reliable.

Work:

- Add structured edit tools for JSON, TOML, and YAML if the existing patch/edit flow remains error-prone in dogfood.
- Add dynamic MCP descriptor caps and shadowing tests.
- Add projection contracts for dynamic MCP and skill outputs.

Done when:

- Config edits are parser-backed where practical.
- External tool surfaces cannot flood or confuse the built-in registry.

### Phase 6: Evals And Dogfood Metrics

Goal: prove the new tool surface helps agents finish tasks with less context and fewer retries.

Work:

- Create a small eval suite for filesystem observation, safe mutation, command verification, large output pagination, and dynamic tool handling.
- Record model-visible byte counts per task.
- Record tool retries and shell fallback rate.
- Track stale-write conflicts and recovery success.

Done when:

- There is a repeatable before/after report for each shipped phase.
- Tool changes can be judged by task success and context efficiency, not only unit tests.

## Suggested First Slice

Start with the smallest backend-only slice that unlocks safer future work.

1. Add catalog invariant tests.
2. Remove stale reserved-tool mentions from tool discovery/access text.
3. Add schema caps for the highest-use filesystem descriptors.
4. Implement `stat`.
5. Add projection tests for `stat`, `read`, `search`, and a large command result.

This slice has low product risk, requires no UI, and gives immediate feedback on descriptor drift and model-visible output quality.

## Acceptance Criteria For The Whole Plan

- No new UI is added.
- New state and large artifacts use OS app-data, not `.xero/`.
- Built-in tools, descriptors, groups, effect classes, risk labels, and projections are covered by invariant tests.
- Reserved/internal tools are not suggested to the model.
- The model can answer common filesystem questions without shelling out.
- All mutating filesystem tools support clear guard behavior and useful preview/error output.
- Large outputs provide continuation ids and omitted counts.
- Dynamic MCP and skill tools are capped, namespaced, and projected safely.
- Scoped tests cover the touched backend modules.

## Non-Goals

- No new visual tool browser.
- No canvas changes.
- No settings panel changes.
- No compatibility layer for old tool names unless explicitly requested.
- No broad rewrite of the command runner.
- No replacement of MCP, skills, browser, emulator, or Solana tools.
- No repo-wide formatting pass.
