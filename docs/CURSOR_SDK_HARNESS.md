# Cursor SDK Harness

Xero runs Cursor as an external-agent adapter through the Cursor SDK, not as an OpenAI-compatible model provider. Routine provider surfaces should label it as **Cursor**. Trace, support, and audit surfaces use **Cursor SDK via Xero MCP harness**.

## Setup

1. Install Node.js 20 or newer.
2. Install dependencies from the repository root:

```bash
pnpm --config.ignore-scripts=false install
```

The root `package.json` allows the native packages required by the Cursor SDK bridge, including `sqlite3`, to build.

3. Create a Cursor API key or service account key in the Cursor dashboard.
4. Export the key:

```bash
export CURSOR_API_KEY=...
```

Verify the local bridge and MCP tool surface:

```bash
node client/scripts/cursor-sdk-bridge.mjs --self-test
cargo run -p xero-cli --bin xero-core -- --json mcp serve-tools --self-test --repo . --mode workspace-write
```

Run a local Cursor smoke test with an installed `xero` CLI:

```bash
xero agent cursor "Inspect this repository and summarize the entry points." \
  --repo . \
  --model composer-latest \
  --allow-subprocess
```

The command creates a Xero run before Cursor starts, launches the Node bridge, exposes Xero Tool Registry V2 through MCP, records Cursor JSONL bridge events, and exports through the normal `xero conversation dump RUN_ID --json` path.

## Safety Modes

`--mode observe-only` exposes `read` and `list`.

`--mode workspace-write` also exposes `write`, `patch`, `delete`, `move`, and `replace`.

`--mode command-enabled` exposes workspace writes plus `command`.

`--native-tool-policy recover` records Cursor-native tool observations and reconciles safe direct workspace changes.

`--native-tool-policy warn` records degraded audit detail without failing solely for native tool observations.

`--native-tool-policy fail_on_unrecoverable_native_mutation` fails only when reconciliation cannot make the workspace and trace safe.

`--native-tool-policy fail_on_any_native_tool` fails on any observed Cursor-native tool call.

## Trace Fields

Cursor-backed runs use provider id `external_cursor_sdk`, default model `composer-latest`, and adapter label `Cursor`.

MCP calls are persisted as Tool Registry V2 events: `tool_registry_snapshot`, `tool_started`, `tool_completed`, `file_changed`, `command_output`, and `policy_decision`.

Cursor-native tool observations are persisted as policy events with `reasonCode: cursor_native_tool_bypass`. Recovered direct edits are recorded with `recovered_cursor_direct_edit`; contained Tool Registry changes promoted back to the real workspace use `contained_cursor_change_promotion`.

## Troubleshooting

If `node client/scripts/cursor-sdk-bridge.mjs --self-test` fails with a missing `sqlite3` binding, lifecycle scripts were likely disabled during install. Re-run `pnpm --config.ignore-scripts=false install` from the repository root, then rerun the self-test.

If `xero agent cursor` fails with `cursor_auth_missing`, check that `CURSOR_API_KEY` is exported in the environment that launches Xero.

If MCP startup fails, run the `mcp serve-tools --self-test` command above with the same `--repo` and `--mode` values as the failing run.

## Notes

Local Cursor SDK execution is the production MVP. Cursor cloud agents are not enabled for local stdio MCP because that would require packaging Xero inside the Cursor VM or exposing an authenticated remote MCP bridge.

The adapter does not claim strict native Xero owned-agent semantics until Cursor-native bypass detection and workspace reconciliation are active for the run.
