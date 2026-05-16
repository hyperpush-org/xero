# Xero Agent TUI Parity Matrix

The Xero Agent TUI is the terminal-native client for owned-agent development workflows. The `tool-harness` binary remains a developer/debug harness for individual backend tool calls and fixture sequences; it is not the product TUI.

`pnpm run dev:tui` launches the desktop-backed `xero-tui`, which shares the headless owned-agent runtime, OS app-data state, and renderer-independent desktop services with `xero agent exec` and the desktop backend.

## Classifications

- `required`: must be present for release readiness.
- `terminal-native`: same backend operation with a terminal interaction model.
- `deferred`: real backend support exists or is emerging, but the first TUI release carries an explicit later rationale.
- `desktop-only`: impossible or inappropriate without a graphical or OS-window surface.

## Matrix

| Command group | Classification | Terminal surface | Backend contract | Rationale |
| --- | --- | --- | --- | --- |
| Project lifecycle | required | Projects screen, `xero project` | OS app-data global registry and project `state.db` | Import/create/remove/select and snapshots are renderer-independent. |
| Agent run loop | required | Agent screen composer | `xero agent exec` owned-agent runtime | Prompt sends use the shared headless owned-agent runtime; fake provider requires explicit TUI toggle. |
| Session browser | required | Sessions screen, `xero session` | `agent_sessions` in app-data project state | Create/resume/rename/auto-name/archive/restore/delete/select are terminal-native database operations. |
| Runtime events and approvals | required | Events / Tool Calls inspector, `xero notification routes|upsert-route|remove-route|dispatches|replies` | `agent_events`, runtime trace export, notification route/dispatch/reply tables | Transcript messages, tool calls, approvals, command events, notification routes, notification dispatches, and notification replies are managed without desktop UI. |
| Files and search | terminal-native | Files screen command group, `xero file`, `xero workspace`, `dev:tui` command palette `code-history selective-undo` | Owned-agent Tool Registry V2 plus workspace index/query plus shared code-history rollback service | File list/read/write/patch/delete/move/replace dispatch through the shared headless production tool runtime; workspace index status/query/explain/reset stay on the existing workspace surface; desktop-backed `code-history selective-undo --apply` calls existing `project_store::apply_code_*_undo` functions instead of maintaining a TUI rollback implementation. |
| Diffs and git | terminal-native | Git screen, `xero git` | Git CLI plus repository status/diff contracts | Git status and diff preview render in the TUI via `xero git status|diff`; stage, commit, sync, generated messages, and guarded discard are terminal-native commands. |
| Command output and PTY | terminal-native | Processes screen, `xero process`, `dev:tui` command palette `terminal open\|list\|read\|write\|resize\|close` | Project `start_targets`, process session rows in project `state.db`, replayed command/tool events, shared desktop project-runner PTY registry | Configured start targets are editable, runnable, tail-able, and stoppable from terminal app-data state; interactive PTYs use the existing `project_runner` portable-pty registry with a retained output buffer for terminal-native reads instead of a second terminal tool. |
| Todos, context, records, memory, Stages | terminal-native | Context screen | Context manifests, plan/todo snapshots, memory candidates, desktop-backed project_store/Lance project records and memory review queue, Stage gate events | Context manifests, plan/todo items, memory candidates, project records, memory review counts/items, and progress/Stage events render without a TUI-owned store; user-facing text keeps per-run terminology as Stage/Stages. |
| Memory review queue automation | terminal-native | Context screen desktop adapter | Shared project_store/Lance memory review queue and review mutations | `dev:tui` supplies a desktop-backed adapter for `project-record` and `memory` operations, so moderation uses existing project_store functions instead of duplicated CLI persistence. |
| Provider, model, MCP, skills/plugins | required | Providers screen, `xero provider`, `xero mcp`, `xero skills`, `xero plugins`, `xero environment`, `xero settings`, `dev:tui` command palette `skill-sources`, `plugin-sources`, and `environment` service commands | Provider profiles, preflight, MCP config, installed skill/plugin records, shared skill/plugin source settings/discovery, environment profile/user-tool rows, shared environment discovery service, agent-tooling/browser-control/soul settings, tool-pack health | Providers screen reads provider, MCP, skill, plugin, environment, and behavior-setting records through terminal adapters; installed skill/plugin enable/disable/remove operates on shared app-data records; source/root configuration and reload use the shared skill/plugin settings and discovery functions; active environment probing, permission resolution, and verified user-tool save/remove call the desktop environment service. |
| Agent definition authoring | terminal-native | Agent Definitions screen, `xero agent-definition list\|show\|versions\|diff\|archive`, `dev:tui` command palette `agent-definition draft\|validate\|preview\|save\|update\|clone\|attachable-skills` | Agent definition/version/Stage contracts plus shared autonomous definition service | Saved definitions, current snapshots, immutable version history, version diffs, and archive browsing are terminal-native through the shared app-data contract; desktop-backed `dev:tui` palette commands call `AutonomousToolRuntime` for JSON-file authoring, validation, effective-runtime preview, explicit write approval, clone/update/save/archive, and attachable-skill catalog output without duplicating the definition validator. |
| Session recovery | terminal-native | Recovery screen, conversation commands, `dev:tui` command palette `conversation rewind` / `code-history session-rollback` | Conversation continue/answer/approve/deny/cancel/resume/search/export/compact/retry/clone/branch/dump/support-bundle plus shared session-lineage and code-history rollback services | The Recovery screen surfaces the existing conversation command surface for selected runs; `branch` reuses the same facade fork operation as `clone`; desktop-backed rewind and rollback palette commands call existing `project_store::create_agent_session_branch` and `project_store::apply_code_session_rollback`. |
| Solana workbench | terminal-native | Workbenches screen, `xero tool-pack doctor solana`, `dev:tui` command palette `solana catalog\|cluster-list\|scenario-list\|persona-roles\|pda-scan\|pda-derive\|secrets-scan\|doc-catalog\|doc-snippets\|wallet-scaffold-list\|token-extension-matrix` | Shared Solana domain tool-pack manifest, health checks, and desktop Solana command module | The TUI renders catalog/doctor availability and routes non-graphical Solana catalog, persona, scenario, PDA, secrets, docs, wallet, and token-matrix operations through the existing desktop Solana module. |
| Maintenance and diagnostics | terminal-native | Maintenance screen, `xero usage summary`, `xero project-state`, `xero wipe` | Provider doctor/preflight, tool-pack doctor, support bundles, usage rows, OS app-data project-state backups/repair, strong-confirmation wipe gates | Existing diagnostics, support bundles, usage/cost summaries, project-state backup/list/restore/repair, and gated app-data wipes are surfaced without touching runtime tool implementations. |
| Built-in browser viewport | desktop-only | Unavailable notice | Embedded webview/tab chrome | A terminal cannot provide the graphical browser viewport, DOM click/type surface, or cookie-import UI. |
| Emulator/simulator live panes | desktop-only | Unavailable notice | Frame streaming and touch/rotation panels | Live device pixels and direct touch panels are graphical OS-window surfaces. |
| Graphical canvas gestures | desktop-only | Agent Definitions terminal editor | Definition DTOs remain shared | Drag/drop layout is desktop-only; Stage authoring remains terminal-native. |
| Voice dictation and window chrome | desktop-only | Unavailable notice | Microphone/window manager APIs | Microphone capture and dock/window chrome are OS-window-bound. |

## Verification

Scoped commands for this surface:

```bash
cargo check --manifest-path client/src-tauri/Cargo.toml -p xero-cli --bins --tests
cargo check --manifest-path client/src-tauri/Cargo.toml -p xero-desktop --bin xero-tui
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-desktop --lib project_runner
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-cli --lib
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-cli --lib tui
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-cli --lib file_cli
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-cli --lib process_cli
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-cli --lib agent_definition_cli
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-cli --lib notification_cli
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-cli --lib usage_cli
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-cli --lib skills_cli
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-cli --lib settings_cli
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-cli --lib project_state_cli
cargo test --manifest-path client/src-tauri/Cargo.toml -p xero-agent-core headless
pnpm run dev:tui -- --smoke
pnpm run dev:tui -- --smoke-run
```

The smoke command renders the ratatui shell on a headless test backend and asserts that all parity groups are classified. The smoke-run command starts an explicit fake-provider run through the same TUI prompt request path and renders the resulting transcript snapshot.
