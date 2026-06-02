# Dev Tool Error Log Plan

## Reader And Outcome

This plan is for the engineer implementing dev-only tool failure logging. After reading it, they should be able to add a dedicated SQLite log database under OS app-data, capture every agent tool-call failure during development runs, and expose the failures in the Settings -> Developer -> Development section.

## Goals

- Log every failed tool call while the app is running in development.
- Store the log in a new dev-only SQLite database, separate from the production global database.
- Capture enough context to diagnose the failure without leaking secrets.
- Add a user-facing Developer settings view for browsing, filtering, inspecting, and clearing failures.
- Keep production builds clean: no production writes, no production UI surface, and no repo-local `.xero/` state.

## Non-Goals

- Do not add backwards-compatible migrations for this dev database. If the dev DB schema is stale or incompatible, wipe and recreate it.
- Do not use the generic Developer Storage table browser as the primary UI. This feature needs a dedicated, purpose-built error viewer.
- Do not add temporary debug UI. The only new UI is the requested user-facing developer error log.
- Do not rename existing stage/workflow DTOs or user-facing stage terminology.

## Current Repo Findings

- The Developer settings tab already renders a `DevelopmentSection` with platform preview controls and a `ToolHarness`.
- The central tool dispatch path is the Tool Registry V2 batch dispatch/persistence flow. It records starts, successes, policy/decoder failures, sandbox failures, handler failures, rollback failures, timeouts, and budget failures into agent run events.
- `AutonomousToolRuntime::execute` and `execute_approved` only see failures after decode/policy/sandbox gates. They are useful, but they are not broad enough as the only logging hook.
- The existing global SQLite helper opens `xero.db` under OS app-data with `foreign_keys`, WAL, and `synchronous=NORMAL`.
- Existing developer tooling already has Rust DTOs, TypeScript zod schemas, Tauri commands, and React settings components that can be mirrored for this feature.

## Architecture

### 1. Dev-Only Database

Create a dedicated SQLite database under the OS app-data directory:

```text
<app-data>/development/tool-call-errors.sqlite
```

The database is only opened when development logging is enabled. Use a helper such as `dev_tool_error_log_path(app_data_dir)` and keep it separate from `xero.db`.

Development logging should be enabled only when both of these are true:

- The Rust build is a debug build, using `cfg(debug_assertions)`.
- The launch mode is local source development, using `XERO_LAUNCH_MODE=local-source`.

Release builds should compile no-op logging and return a clear unavailable result from log viewer commands if invoked directly.

### 2. Schema

Use a single strict table with JSON validity checks and targeted indexes:

```sql
CREATE TABLE IF NOT EXISTS tool_call_error_log (
    id TEXT PRIMARY KEY CHECK (id <> ''),
    occurred_at TEXT NOT NULL CHECK (occurred_at <> ''),
    source TEXT NOT NULL CHECK (source <> ''),
    project_id TEXT,
    agent_session_id TEXT,
    run_id TEXT,
    turn_index INTEGER,
    tool_call_id TEXT NOT NULL CHECK (tool_call_id <> ''),
    tool_name TEXT NOT NULL CHECK (tool_name <> ''),
    input_sha256 TEXT NOT NULL CHECK (length(input_sha256) = 64),
    input_json TEXT NOT NULL CHECK (input_json <> '' AND json_valid(input_json)),
    input_redacted INTEGER NOT NULL CHECK (input_redacted IN (0, 1)),
    error_code TEXT NOT NULL CHECK (error_code <> ''),
    error_class TEXT NOT NULL CHECK (error_class <> ''),
    error_category TEXT,
    error_message TEXT NOT NULL CHECK (error_message <> ''),
    model_message TEXT,
    retryable INTEGER NOT NULL CHECK (retryable IN (0, 1)),
    dispatch_json TEXT NOT NULL CHECK (dispatch_json <> '' AND json_valid(dispatch_json)),
    context_json TEXT NOT NULL CHECK (context_json <> '' AND json_valid(context_json))
) STRICT;

CREATE INDEX IF NOT EXISTS idx_tool_call_error_log_occurred_at
    ON tool_call_error_log(occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_tool_call_error_log_tool_name
    ON tool_call_error_log(tool_name, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_tool_call_error_log_error_code
    ON tool_call_error_log(error_code, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_tool_call_error_log_project
    ON tool_call_error_log(project_id, occurred_at DESC);
```

Set `PRAGMA user_version = 1`. On any schema mismatch in development, close the connection, delete the DB plus WAL/SHM sidecars, and recreate it. This follows the project rule for stale/incompatible app-data state.

### 3. Stored Context

Capture these fields for every failed tool call:

- Identity: project id, agent session id, run id, turn index, tool call id, tool name.
- Input: redacted JSON input, whether it was redacted, and SHA-256 of the original JSON.
- Error: command error code/class/message/retryable and, when available, V2 tool error category/model message.
- Dispatch metadata: policy/sandbox result, group mode, elapsed time, timeout, budget, rollback payload/error, doom-loop signal, and telemetry.
- Runtime context: provider id, model id, runtime agent id, operator approval state, launch mode, host OS, app version if available.

Use the existing persistence redaction helper for input JSON before writing. Never store unredacted sensitive input, command secrets, sensitive-input tool values, API keys, OAuth tokens, wallet/keypair content, or hidden prompts.

### 4. Logging Hook

Primary hook:

- Extend the Tool Registry V2 dispatch persistence path so every `ToolDispatchOutcome::Failed` writes a dev log row.
- Also log the preflight path where a descriptor is missing or the legacy registry rejects the call before a V2 report is produced.

Implementation notes:

- Keep the logging best-effort. If the dev log DB cannot be opened or written, do not change the tool-call failure returned to the agent/user.
- Pass enough original call context into failure persistence so failures are logged with the real tool input, not `{}`.
- Include the agent session id and turn index by loading the existing agent run record once and passing the values through the failure persistence path.
- `AutonomousToolRuntime::execute` and `execute_approved` can optionally add supplemental telemetry, but they should not be the only hook because they miss decode, policy, sandbox, timeout, and budget failures.

### 5. Commands And DTOs

Add dev-only Tauri commands:

- `developer_tool_error_log_list`
- `developer_tool_error_log_clear`

List request:

- `limit`, default 100, max 500.
- `offset`, default 0.
- Optional `projectId`, `toolName`, `errorCode`, `query`.
- Sort by `occurredAt DESC`.

List response:

- `databasePath`
- `entries`
- `totalCount`
- `limit`
- `offset`

Entry DTO:

- All table columns, with JSON columns decoded as JSON values for the frontend.
- A short derived `messagePreview` for dense table display.

Use parameterized SQLite queries. Any dynamic sort/filter fields must be whitelisted.

### 6. Settings UI

Add a new `ToolErrorLog` component under the existing Development settings section.

Expected UI:

- Header row with a failure count badge, refresh button, and clear button.
- Filter controls for text query, project id, tool name, and error code.
- Dense ShadCN-style table/list showing time, tool, error code/category, project/run, retryability, and message preview.
- Row expansion or a details panel showing redacted input JSON, dispatch JSON, and context JSON.
- Empty state when there are no failures.
- Error state when the command is unavailable or the dev DB cannot be read.

Use ShadCN components where possible: `Button`, `Badge`, table/list primitives, `ScrollArea`, `Input`, `Select`, `Separator`, and dialog/alert components for destructive clear confirmation.

### 7. Frontend Model

Add TypeScript zod schemas and types beside the existing developer tool harness schemas. Keep backend and frontend field names camelCase over IPC.

The UI should invoke:

- `developer_tool_error_log_list` on mount and on refresh/filter change.
- `developer_tool_error_log_clear` only after confirmation, then reload the list.

### 8. Verification

Rust scoped tests:

- Dev DB initializes with the v1 schema, WAL pragmas, and indexes.
- Stale schema version wipes and recreates the dev DB.
- Insert uses redacted input and stores the original input hash.
- Query filters use parameters and return newest failures first.
- Unknown/invalid tool calls are logged.
- V2 policy/sandbox/handler failures are logged.
- Logging failures are best-effort and do not replace the original tool failure.

Frontend scoped tests:

- Development settings renders the new error log section.
- Empty, loading, error, populated, filtered, expanded/details, and clear-confirm states render correctly.
- IPC responses are validated by zod.

Suggested scoped commands:

```text
pnpm --dir client test -- developer-tool-error-log
cargo test -p xero-desktop-lib developer_tool_error_log
cargo test -p xero-desktop-lib tool_dispatch::tests::<specific failure log tests>
```

Run one Cargo command at a time.

## Rollout Steps

1. Add the dev DB module and schema lifecycle.
2. Add insert/list/clear services with tests.
3. Wire dispatch failure logging into the V2 dispatch persistence path.
4. Add Rust DTOs, Tauri commands, and TypeScript zod schemas.
5. Add the Development settings error-log UI.
6. Add scoped Rust and frontend tests.
7. Manually run a synthetic failing tool call in the Tauri app and confirm it appears in the new Developer settings section.

