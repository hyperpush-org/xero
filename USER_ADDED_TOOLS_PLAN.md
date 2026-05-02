# User-Added Developer Tools Plan

Reader: an internal Xero engineer who will implement the user-added-tools feature in the Diagnostics → Developer environment panel.

Post-read action: ship a UI affordance that lets a user register a custom CLI (one that the built-in catalog does not yet detect), have the desktop runtime *actually invoke it* to verify it exists and capture a version line, persist the entry, and merge it into every subsequent environment probe so it shows up in the panel like a built-in tool.

Status: draft.

## Goal

The current Developer environment panel reflects a hard-coded probe catalog (`environment_probe_catalog()` in `client/src-tauri/src/environment/probe.rs`). The catalog is broad but cannot enumerate every CLI a developer uses. Users should be able to teach Xero about their own tools — without rebuilding the binary — and have those entries treated as first-class detected tools.

Three properties must hold:

1. **Verified, not asserted.** When a user adds a tool, the runtime must run it once with the user-supplied args, confirm the binary resolves, capture a single version line, and surface success/failure before persistence. We never trust the user's claim that a tool exists; we re-use the same probe machinery the built-in catalog uses.
2. **Persistent across runs.** A user-added tool survives app restart and re-probe. Built-in catalog refreshes do not erase it.
3. **Indistinguishable in the UI from built-ins.** Once verified, a user-added tool flows through the same `EnvironmentToolSummaryDto` pipeline. It appears in the headliner row if priority-eligible, otherwise inside the appropriate category group. A subtle "custom" badge is acceptable but not required for a v1.

## Non-Goals

- No editing of built-in catalog entries through this UI. If a built-in entry is wrong, the fix is a code change, not user override.
- No live capability authoring (capabilities like `node_project_ready` stay code-defined).
- No discovery automation — we are not scanning `$PATH` for "interesting" binaries on the user's behalf. The user names the tool.
- No multi-arg "command profiles" (e.g., probing both `docker --version` and `docker compose version` as one tool). The built-in `docker_compose` entry shows that two-step probes are possible, but for user-added tools we limit to a single command + args list to keep the UX simple.

## Product Shape

In the Developer environment panel (Settings → Diagnostics):

- Below the "Show all detected tools" expander, render a small footer row: **"+ Add tool"**.
- Clicking opens a popover/dialog with three fields:
  - **Tool name** — a short identifier shown as the chip label (e.g., `terraform`, `kotlinc`). Required, lowercased, slug-safe (`a-z0-9_-`), 1–32 chars.
  - **Command** — the executable to resolve on `$PATH` (e.g., `terraform`). Required. Plain executable name OR an absolute path.
  - **Version arguments** — defaults to `--version`. Free-text comma- or space-separated list, parsed into `args[]`.
  - **Category** — dropdown of all `EnvironmentToolCategory` variants, default `BaseDeveloperTool`.
- A **"Verify"** primary button runs a server-side dry probe (see Tauri command below). The result is rendered inline:
  - Success: green pill with the captured version string and resolved path. **"Save"** button becomes enabled.
  - Missing: red pill "Could not find `<command>` on PATH." Save disabled.
  - Failed/timeout/redacted: amber pill with the diagnostic. Save disabled, but the user can adjust the args and retry.
- Once saved, the panel re-fetches the profile and the new chip appears in its category section. A small **×** button on each user-added chip removes it after a confirm. Removal does not cascade to the built-in catalog.

The popover must explicitly note: **"Xero will run `<command> <args>` to verify the tool. The first non-empty line of output (with secrets redacted) is stored as the version."** This is the consent surface for executing user-supplied commands.

## Architecture

### Storage — global SQLite

Add a new table in the global SQLite migration set (`client/src-tauri/src/global_db/`):

```sql
CREATE TABLE user_added_environment_tools (
  id              TEXT PRIMARY KEY,           -- the user-provided slug
  category        TEXT NOT NULL,              -- snake_case EnvironmentToolCategory
  command         TEXT NOT NULL,
  args_json       TEXT NOT NULL,              -- JSON array of strings
  created_at      TEXT NOT NULL,              -- RFC3339
  updated_at      TEXT NOT NULL
) STRICT;
```

Rationale: small, append-only, lives in the same DB as `environment_profile_snapshot`. No project scoping — the user's machine has one toolchain.

The `id` column is the primary key, so duplicate names are rejected at the DB level and round-trip cleanly. We collide-protect against built-in IDs in the Rust insert path (return `ConflictWithBuiltin` error when `id` is already in `environment_probe_catalog()`). The user must pick a different name.

### Catalog merge

Refactor `environment_probe_catalog()` so the existing `Vec<EnvironmentProbeCatalogEntry>` becomes the *built-in* catalog. Introduce:

```rust
pub fn merged_environment_probe_catalog(
    user_entries: Vec<UserAddedToolRow>,
) -> Vec<EnvironmentProbeCatalogEntry>
```

This new function takes the existing built-in list and appends a synthesized `EnvironmentProbeCatalogEntry` for each user row. The `EnvironmentProbeCatalogEntry` struct currently uses `&'static str` fields — it must be relaxed to `Cow<'static, str>` (or `String`) so dynamic entries can co-exist with static ones. This is a small but mandatory refactor; it touches:

- `EnvironmentProbeCatalogEntry { id, category, command, args }`
- The `entry()` helper.
- All call sites (`probe.rs` + tests).

`probe_environment_profile()` becomes a thin wrapper that loads user entries from SQLite, merges them, and runs the same `probe_environment_profile_with()` core. The probe behaviour (timeout, sanitization, sensitive-data redaction) is identical for built-in and user entries — that's the whole point.

### Verify Tauri command

Add `client/src-tauri/src/commands/environment_user_tools.rs` exposing three commands:

```rust
#[tauri::command]
pub async fn environment_verify_user_tool(request: VerifyUserToolRequest)
    -> Result<VerifyUserToolResponse, OperatorActionError>;

#[tauri::command]
pub async fn environment_save_user_tool(request: SaveUserToolRequest)
    -> Result<EnvironmentProbeReport, OperatorActionError>;

#[tauri::command]
pub async fn environment_remove_user_tool(id: String)
    -> Result<EnvironmentProbeReport, OperatorActionError>;
```

The verify command:
1. Validates the request (id slug regex, command non-empty, args are strings, category enum).
2. Builds a one-element catalog containing only the proposed entry.
3. Runs `probe_environment_profile_with(...)` against that catalog using the production `SystemEnvironmentBinaryResolver` and `SystemEnvironmentCommandExecutor`.
4. Returns the resulting `EnvironmentToolRecord` (present, version, path, probe_status, duration_ms) plus any diagnostics.

The save command:
1. Re-runs verification (defense in depth — the client could lie about prior verify success).
2. Rejects if `probe_status != Ok` so we never persist a tool that did not verify.
3. Inserts the row inside a transaction, then triggers a full re-probe and returns the new merged `EnvironmentProbeReport`. Returning the full report keeps the UI single-source-of-truth.

The remove command deletes the row by id and re-probes. No verification needed.

All three commands route through the existing operator-action error contract so the UI can surface structured failures (`OperatorActionErrorView`).

### Schema (TS / Zod)

Add to `client/src/lib/xero-model/environment.ts`:

```ts
export const verifyUserToolRequestSchema = z
  .object({
    id: z.string().trim().min(1).max(32).regex(/^[a-z0-9][a-z0-9_-]*$/),
    category: environmentToolCategorySchema,
    command: z.string().trim().min(1).max(256),
    args: z.array(z.string().trim().min(1)).max(8).default([]),
  })
  .strict()

export const verifyUserToolResponseSchema = z
  .object({
    record: environmentToolSummarySchema,
    diagnostics: z.array(environmentDiagnosticSchema).default([]),
  })
  .strict()

export const saveUserToolRequestSchema = verifyUserToolRequestSchema
export type VerifyUserToolRequestDto = z.infer<typeof verifyUserToolRequestSchema>
export type VerifyUserToolResponseDto = z.infer<typeof verifyUserToolResponseSchema>
```

The DB row + ID is enough to identify the tool to remove; no separate request schema needed beyond a `z.string()` id.

### Frontend wiring

The current data flow already plumbs `EnvironmentProfileSummaryDto` through `useXeroDesktopState` and `onRefreshEnvironmentDiscovery`. Add three new operator actions in the same hook:

- `verifyUserEnvironmentTool(request)`
- `saveUserEnvironmentTool(request)` — on success it replaces the cached summary with the returned report, mirroring the existing refresh behaviour.
- `removeUserEnvironmentTool(id)` — same pattern.

`DiagnosticsSection` (or a sibling component) consumes them and renders the popover.

### UI components

Most of what's needed already exists:
- `Popover` / `Dialog` — pick whichever fits the visual density best (Popover preferred for in-flow use).
- `Input`, `Select`, `Button`, `Badge`, `Alert` — existing.
- The `ToolChip` component added in the display fix is reused.

A new local component `AddToolForm` lives next to `diagnostics-section.tsx`. Keep it co-located until a second consumer appears.

A custom-tool chip should render an `×` overlay on hover only, to keep the read-only chips of built-ins clean. Use the same chip styling otherwise — the goal is parity, not visual othering.

## Validation Gates

Before this ships:

1. **The verify path actually executes the binary.** Cover with a test that drops a fixture script in a temp dir, points the resolver at it, and asserts the captured version. Reuse the existing `FakeResolver` / `FakeExecutor` test scaffolding in `probe.rs`.
2. **Slug collisions are blocked.** Test that adding a user tool with id `"git"` (which exists in the built-in catalog) returns a `ConflictWithBuiltin` error and never inserts.
3. **Sensitive output is dropped.** Reuse `find_prohibited_persistence_content` so a verify call that returns a token fails with `Failed`, not `Ok`. The existing `sensitive_version_output_is_not_persisted` test pattern applies.
4. **Persistence round-trips.** Insert via the save command, drop the in-memory state, reload, re-probe, and confirm the user tool appears in the merged report.
5. **Remove is idempotent.** Removing a non-existent id is a no-op (returns the current report).
6. **Schema validation.** Rejects ids with spaces, args > 8, command containing `;`, `&&`, backticks, or shell metacharacters. We invoke the binary directly (no shell), but reject metacharacters anyway to prevent users from creating tools whose name is an injection vector for someone else's regex.
7. **Frontend test:** the popover surfaces the verify error string, leaves Save disabled until verify succeeds, and clears the form on save.

## Security Considerations

- Commands run with the **same privilege as the Tauri host process**. There is no isolation. The popover must communicate this clearly.
- Args are passed as a `Vec<String>` to `Command::args()` — never to `bash -c` or `sh -c`. No shell expansion happens. This is already how the built-in probe works.
- The 3-second `DEFAULT_PROBE_TIMEOUT` and stdout/stderr redaction (`sanitize_version_line`, `find_prohibited_persistence_content`) inherit automatically because we reuse the same probe core. Do not bypass them for user-added tools.
- Reject `command` strings that resolve to anything outside the user's home, system bins, or the resolver's known dirs? **No** — that would cripple legitimate use. The resolver already only walks `$PATH` + bundled/managed/common dirs; that is the boundary.
- Storing `args` lets a malicious DB write set surprising arguments. Acceptable risk: the global DB is already trusted local state; if it's compromised, the attacker has worse options than seeding probe args.

## Migration & Rollout

- The new SQLite table is additive; no migration risk for existing installs.
- `merged_environment_probe_catalog()` returning the built-in list when the user table is empty makes this fully backwards-compatible. Existing probe behaviour is byte-identical for users who never add a tool.
- Refactoring `EnvironmentProbeCatalogEntry` from `&'static str` to `Cow<'static, str>` is the only invasive change. Bench it once to confirm no probe-time regression — it shouldn't, since the per-entry cost is dominated by the spawn.

## Open Questions

- **Per-project vs global**: should user-added tools be machine-global (this plan) or project-scoped? Current vote: machine-global, because the panel itself is machine-level. Revisit if a user asks for repo-level overrides.
- **Sync to settings file**: do user-added tools deserve a JSON export so they survive a profile reset? Probably yes, in a follow-up. Out of scope here.
- **Built-in suggestions**: when a user types a tool name we already detect, offer "this is already detected as `<id>` — refresh instead?" Polish item for v1.1.

## Estimated Work

| Layer | Files | Effort |
| --- | --- | --- |
| SQLite migration + repo | `global_db/`, new `user_added_tools.rs` | half day |
| `EnvironmentProbeCatalogEntry` refactor + merge fn | `environment/probe.rs` + tests | half day |
| Three Tauri commands + DTOs + Zod | `commands/environment_user_tools.rs`, `xero-model/environment.ts` | half day |
| `useXeroDesktopState` plumbing | `src/features/xero/use-xero-desktop-state` | quarter day |
| `AddToolForm` UI + chip remove control | `components/xero/settings-dialog/diagnostics-section.tsx` (or sibling) | half day |
| Tests (Rust probe + frontend popover + e2e roundtrip) | spread across above | half day |

Total: roughly 2.5 engineer-days, with most of the risk concentrated in the catalog refactor and the verification flow's edge cases.
