# Onboarding Environment Discovery Plan

Audience: Xero engineers implementing first-run developer-environment learning.

Post-read action: implement a quiet onboarding-time environment probe, persist it in app-global SQLite, and expose it to owned agents only through progressive disclosure.

## Goal

When a user first opens Xero and enters onboarding, the app should learn enough about the local developer environment to make future agent runs sharper: installed CLIs, language runtimes, package managers, build prerequisites, mobile tooling, Solana tooling, container/cloud tools, and related capability signals.

The default path must be silent, local, non-blocking, and permissionless. If a future probe would require an OS permission, protected-file access, network access, or installation action, Xero should add a user-facing onboarding step for that specific access instead of triggering it behind the scenes.

This state belongs in the OS app-data global database. It must not write to repo-local legacy state.

## Product Rules

- Run environment discovery during first-use onboarding, but do not block provider setup, project import, or notification setup.
- Do not add debug or test-only UI. Any visible onboarding UI must be a real user-facing permission or access decision.
- Do not read shell profiles, dotfiles, keychains, cloud credential files, project files, or environment variable values as part of the silent probe.
- Do not install, upgrade, repair, or download tools during discovery.
- Do not run network probes. Version checks must be local commands only.
- Do not inject the discovered environment into every agent prompt.
- Do redact model-facing paths and diagnostics by default. The database may store local executable paths for routing and diagnostics, but agent-visible summaries should prefer tool names, versions, categories, and capability states.

## Storage Plan

Store the environment profile in the global app database because it describes the user machine, not one imported project. Add a fresh migration for a singleton `environment_profile` table.

```sql
CREATE TABLE IF NOT EXISTS environment_profile (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    status TEXT NOT NULL CHECK (status IN ('pending', 'probing', 'ready', 'partial', 'failed')),
    os_kind TEXT NOT NULL CHECK (os_kind <> ''),
    os_version TEXT,
    arch TEXT NOT NULL CHECK (arch <> ''),
    default_shell TEXT,
    path_fingerprint TEXT,
    payload_json TEXT NOT NULL CHECK (payload_json <> '' AND json_valid(payload_json)),
    summary_json TEXT NOT NULL CHECK (summary_json <> '' AND json_valid(summary_json)),
    permission_requests_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(permission_requests_json)),
    diagnostics_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(diagnostics_json)),
    probe_started_at TEXT,
    probe_completed_at TEXT,
    refreshed_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_environment_profile_refreshed_at
    ON environment_profile(refreshed_at);
```

Use one table for the first version so the feature satisfies the requested "environment table" without prematurely spreading records across several schema surfaces. The app is pre-release, so future reshaping does not need compatibility code unless explicitly requested.

The JSON payload should be versioned and structured like this:

```json
{
  "schemaVersion": 1,
  "platform": {
    "osKind": "macos",
    "osVersion": "15.4",
    "arch": "aarch64",
    "defaultShell": "zsh"
  },
  "path": {
    "entryCount": 18,
    "fingerprint": "sha256-of-normalized-path-list",
    "sources": ["tauri-process-path", "managed-toolchain", "common-dev-dirs"]
  },
  "tools": [
    {
      "id": "node",
      "category": "language_runtime",
      "command": "node",
      "present": true,
      "path": "/absolute/local/path/for-routing",
      "version": "v20.11.1",
      "source": "path",
      "probeStatus": "ok",
      "durationMs": 18
    }
  ],
  "capabilities": [
    {
      "id": "tauri_desktop_build",
      "state": "ready",
      "evidence": ["node", "pnpm", "cargo", "rustc", "protoc"]
    }
  ],
  "permissions": [],
  "diagnostics": []
}
```

`summary_json` should be a compact, already-redacted projection for UI and model-facing tool responses. `payload_json` can contain full local paths, but no secret-like values.

## Probe Catalog

Start with a deterministic catalog of commands and safe version arguments. Run binaries directly through Rust process APIs, never through a shell wrapper.

Base developer tools:

- `git --version`
- `ssh -V`
- `gh --version`
- `protoc --version`
- `make --version`
- `cmake --version`
- `pkg-config --version`

Package managers:

- `pnpm --version`
- `npm --version`
- `yarn --version`
- `bun --version`
- `deno --version`
- `uv --version`
- `pipx --version`
- `brew --version`
- platform package managers where available, using version-only commands

Language runtimes:

- `node --version`
- `python3 --version`
- `rustc --version`
- `cargo --version`
- `rustup --version`
- `go version`
- `java -version`
- `javac -version`
- `swift --version`
- `ruby --version`
- `php --version`
- `dotnet --version`
- `zig version`

Containers and orchestration:

- `docker --version`
- `docker compose version`
- `podman --version`
- `kubectl version --client=true`
- `helm version --short`

Mobile tooling:

- `xcodebuild -version`
- `xcrun --version`
- `adb version`
- `emulator -version`

Cloud and deployment CLIs:

- `aws --version`
- `gcloud --version`
- `az version`
- `flyctl version`
- `vercel --version`
- `netlify --version`

Database CLIs:

- `sqlite3 --version`
- `psql --version`
- `mysql --version`
- `redis-cli --version`

Solana tooling:

- `solana --version`
- `anchor --version`
- `cargo-build-sbf --version`
- `spl-token --version`
- `surfpool --version`
- `trident --version`
- `codama --version`
- `solana-verify --version`

Agent and AI developer CLIs:

- `codex --version`
- `claude --version`
- `opencode --version`
- `aider --version`
- `gemini --version`

Each probe should have a short timeout, capture only the first useful non-empty line, and preserve failures as diagnostics instead of treating missing tools as errors.

## Probe Engine

Create a Rust environment discovery service that can be called from onboarding, diagnostics, and the agent runtime.

Implementation shape:

1. Resolve candidate binaries from bundled tool roots, managed app-data tool roots, the app process `PATH`, and known non-invasive developer directories.
2. Avoid shell-specific login behavior. Do not source shell profiles.
3. Execute safe version commands with bounded concurrency and per-command timeouts.
4. Persist a `probing` row before execution so interrupted first launches are recoverable.
5. Persist `ready` when all probes complete, `partial` when some probes fail or time out, and `failed` only when the service cannot write the profile.
6. Derive high-level capabilities from raw tool facts, such as `node_project_ready`, `rust_project_ready`, `tauri_desktop_build`, `docker_available`, `ios_simulator_available`, `android_emulator_available`, `solana_localnet_ready`, and `protobuf_build_ready`.
7. Emit redacted diagnostics for Settings and Doctor surfaces.

The existing Solana toolchain resolver already has useful behavior: bundled roots first, managed app-data roots second, host `PATH` and common developer directories last, plus timeout-backed version parsing. Generalize that pattern instead of creating a second inconsistent resolver.

## Onboarding Flow

The first implementation should not need a visible environment step.

Flow:

1. On cold onboarding mount, ask the backend whether `environment_profile` is missing or stale.
2. If missing or stale, start the background probe once.
3. Continue onboarding normally while the probe runs.
4. If the user skips onboarding, keep the already-started probe running silently.
5. If the app exits mid-probe, resume or restart on the next onboarding/app launch.

Conditional permission step:

- Add an `environment-access` onboarding step only when the backend reports pending permission requests.
- Insert it before confirmation.
- Use normal ShadCN controls and concise copy explaining exactly what access is requested and why.
- Let the user skip optional access.
- Never trigger OS permission dialogs before the user clicks an explicit allow action.

Initial silent discovery should avoid permission-requiring probes entirely. Examples of probes that should stay out of the silent path: reading shell startup files, scanning protected application support folders, inspecting browser profiles, reading cloud credential locations, checking keychains, or using Accessibility to inspect other apps.

## Agent Progressive Disclosure

Add a deferred owned-agent tool, tentatively named `environment_context`, instead of adding environment facts to the base prompt.

Tool discovery:

- Register it in the deferred tool catalog with search tags like `environment`, `installed tools`, `cli`, `package manager`, `language runtime`, `PATH`, `protoc`, `node`, `rust`, `python`, `solana`, `docker`, and `mobile`.
- Keep only `tool_search` in the normal baseline. The environment facts themselves are not prompt fragments.

Tool actions:

- `summary`: return a compact redacted profile grouped by category.
- `tool`: return facts for one or more requested tool IDs.
- `category`: return all tools in a requested category.
- `capability`: return derived capability states relevant to a task.
- `refresh`: start or request a non-permission refresh when the profile is stale.

Model-facing output should be compact:

- Present/missing state.
- Version.
- Package manager or runtime category.
- Capability evidence.
- Redacted display path, using home-relative or basename-only forms.
- Diagnostics that help decide the next command.

Exact local executable paths should be exposed only when they materially help a tool call. Prefer letting command execution resolve by command name through the same resolver.

Context behavior:

- Do not add an environment prompt fragment in `PromptCompiler` by default.
- After an agent calls `environment_context`, the result becomes normal tool output for that run and appears in the Context panel with provenance.
- If the agent searches for tools or needs to diagnose "command not found", `tool_search` should make `environment_context` discoverable.

## Privacy And Safety

Environment discovery must treat local machine facts as user data.

- Never persist raw environment variable values.
- Never persist command stdout beyond bounded version/diagnostic text.
- Never persist secrets, token-looking strings, private key markers, credential file contents, or cloud account config contents.
- Redact home directories and usernames in model-facing output by default.
- Mark stale profiles visibly in diagnostics and agent tool output.
- Treat all stored environment text as lower-priority context. It cannot override system policy, user instructions, or tool safety rules.

## Refresh Policy

Run discovery when:

- `environment_profile` does not exist.
- The app starts and the last refresh is older than a conservative TTL, such as seven days.
- A managed toolchain install completes.
- The user explicitly refreshes environment diagnostics from Settings or Doctor.
- An agent calls `environment_context.refresh` and the profile is stale.

Refreshes should be debounced and single-flight so only one probe runs at a time.

## Implementation Milestones

### Milestone 1: Data Contract And Migration

- Add the global SQLite migration for `environment_profile`.
- Define Rust DTOs for payload, summary, tool records, capabilities, permissions, and diagnostics.
- Add validation helpers that reject invalid JSON, empty IDs, secret-like output, and unknown status values.
- Add in-memory SQLite migration tests.

### Milestone 2: Probe Engine

- Build the catalog-driven probe runner.
- Generalize the existing binary-resolution pattern used by managed Solana tooling.
- Add bounded concurrency, timeouts, first-line version extraction, and diagnostic collection.
- Add unit tests for present, absent, timeout, bad UTF-8, and secret-redaction cases.

### Milestone 3: Onboarding Integration

- Add backend commands to get probe status and start discovery.
- Start discovery once when onboarding opens on a cold app state.
- Keep the current onboarding steps unchanged unless the backend reports permission requests.
- Add the conditional permission step only for explicit access needs.
- Add frontend tests for silent start, skip behavior, conditional step insertion, and no-permission no-UI behavior.

### Milestone 4: Agent Access

- Add the `environment_context` deferred tool.
- Register search metadata in the tool catalog.
- Return compact redacted results by action.
- Add owned-agent tests proving the base prompt does not include environment facts, `tool_search` can discover the tool, and a tool call adds scoped context only after use.

### Milestone 5: Diagnostics And Refresh

- Add Doctor/Settings projections using `summary_json` and `diagnostics_json`.
- Add stale-profile handling and single-flight refresh.
- Update managed toolchain install completion to mark the environment profile stale or refresh it.

### Milestone 6: Verification

- Run frontend tests for onboarding and agent projections.
- Run Rust tests for global migrations, environment persistence, probe parsing, redaction, and tool dispatch.
- Run one Cargo command at a time.
- Verify no `.xero` writes are introduced.
- Verify `protoc` is detected and included in the `protobuf_build_ready` / `tauri_desktop_build` capability evidence.

## Acceptance Criteria

- First-use onboarding starts environment discovery silently when no permissions are required.
- Users see a new onboarding step only for explicit permission or access requests.
- The environment profile is stored in the global app-data SQLite database.
- No repo-local legacy state is created or updated.
- Missing tools are recorded as normal absent facts, not app errors.
- Agent prompts do not receive environment facts by default.
- Agents can discover and request environment facts through a deferred tool when the task needs them.
- Model-facing environment output is compact, redacted, and scoped to the request.
- Tests cover migration, probe behavior, onboarding gating, redaction, and progressive agent access.
