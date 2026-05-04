# Xero (Tauri Desktop)

Xero is a **Tauri desktop app** for agentic development workflows, with a React/Vite frontend and a Rust backend command surface.

It combines:

- Project/repository import + file editing/search operations
- Runtime/session orchestration for AI providers
- Autonomous run + operator approval loop support
- In-app browser automation
- iOS/Android emulator sidebars + automation hooks
- Solana workbench tooling (clusters, personas, tx pipeline, deploy helpers)
- Skill/plugin discovery and execution support
- Session memory, transcript search, compaction, branch, and rewind workflows
- MCP server registry management
- Notification routing (Telegram, Discord)
- Phoenix/Postgres sidecar services for web callbacks and shared backend features

> Important: this is a **desktop-first Tauri app**. For end-to-end behavior, run via Tauri (`tauri dev`), not as a plain browser app.

---

## Repository Layout

```text
.
├─ client/                 # Main desktop app (React + Vite + Tauri + Rust)
│  ├─ src/                 # App entry + feature hooks
│  ├─ components/          # ShadCN UI + Xero shell/views
│  ├─ src-tauri/           # Rust backend, commands, state, tests
│  └─ package.json
├─ landing/                # Separate Next.js marketing site
├─ server/                 # Phoenix service + Postgres-backed features
├─ docs/                   # Provider, memory, skills/plugin docs
├─ STANDALONE_AGENTS_ASK_PLAN.md # Plan for Ask agent + future standalone agents
└─ package.json            # Root convenience scripts
```

### Key top-level projects

- `client/`: production desktop app (`productName: Xero`, `identifier: dev.sn0w.xero`)
- `landing/`: separate website, run on port `3001` in root dev workflow
- `server/`: Phoenix 1.8 service, local Postgres, GitHub auth callback/session support, game stats, Oban jobs

### Non-runtime/reference content

- `.tmp-gsd2-ref/`: reference snapshot directory (ignored by build workflows)
- `.xero/`: legacy repo-local state. New app/project state belongs under the OS app-data directory.

---

## Tech Stack

### Desktop app (`client/`)

- **Frontend:** React 19, TypeScript, Vite, Vitest, ShadCN/Radix UI, Tailwind CSS
- **Desktop host:** Tauri v2
- **Backend:** Rust (command surface + orchestration + persistence)
- **Storage:** SQLite and LanceDB-backed project stores under the OS app-data directory

### Landing site (`landing/`)

- Next.js 16, TypeScript, Tailwind CSS

### Server (`server/`)

- Phoenix 1.8, Elixir, LiveView, Ecto/PostgreSQL, Oban, Hammer rate limiting

---

## Core App Surfaces

Main shell views:

- **Workflow**
- **Agent**
- **Editor**

Sidebar tools:

- In-app browser
- Games sidebar
- iOS emulator sidebar (macOS only)
- Android emulator sidebar
- Solana workbench sidebar

Onboarding flow covers:

- Provider profile setup
- Project import
- Notification route setup

---

## Prerequisites

### 1) Base (required)

- Node.js (modern LTS recommended; Node 20+ is safest for current deps)
- `pnpm`
- Rust toolchain + Cargo
- `protoc` on PATH. LanceDB-backed memory pulls `lance-`* crates whose build scripts compile vendored `.proto` files. On macOS: `brew install protobuf`.
- Tauri OS prerequisites for your platform (WebView/runtime dependencies)
- Docker Desktop, Docker Engine, or a compatible Docker daemon, for the local Postgres service
- Docker Compose v2 (`docker compose`) or legacy `docker-compose`; root scripts use whichever is available
- Elixir/Mix for the Phoenix server

### 2) Emulator features

#### Android

- Android SDK tooling (`adb`, `emulator`) and at least one AVD
- If missing, Xero can provision a managed SDK in app data via backend provisioning flow

#### iOS (macOS only)

- Xcode + iOS Simulator tooling (`xcrun`, `simctl`)
- Accessibility permission (required for some simulator input paths)

### 3) Solana workbench features

Detected/used CLIs include:

- `solana` (minimum required for localnet workflows)
- `anchor`
- `cargo-build-sbf`
- `surfpool`
- `trident`
- `codama`
- `solana-verify`
- plus `node` and `pnpm`

If tools are missing, workbench surfaces degraded/missing-toolchain states rather than crashing.

### 4) Server features

- Postgres is provided by `server/docker-compose.yml` in local development.
- Server env defaults are documented in `server/.env.example`; local secrets belong in `server/.env`.

---

## Setup

This repo is **not** a pnpm workspace, but the root dev preflight installs each package in place.
After cloning and completing the prerequisite toolchain/env setup above, the happy path is:

```bash
pnpm run dev
```

The root `pnpm run dev` command runs an idempotent preflight that:

- installs root, `client/`, and `landing/` pnpm dependencies from their lockfiles
- verifies required local commands (`pnpm`, `git`, `mix`, `cargo`, `protoc`)
- installs Hex/Rebar if missing, fetches Mix deps, and installs Phoenix asset tools
- starts Docker/Postgres where the OS allows it, creates the database, and applies migrations

Manual setup commands still work if you need to isolate a failing step:

```bash
pnpm install
pnpm --dir client install
pnpm --dir landing install
cd server && mix setup
```

---

## Command Reference

### Root commands

```bash
pnpm run dev          # Preflight, Postgres logs, Phoenix server, Tauri desktop, and landing site
pnpm run dev:preflight
pnpm run db:up
pnpm run db:down
pnpm run db:reset
pnpm run server:setup
pnpm run dev:server   # Phoenix server on localhost:4000
pnpm run dev:tauri    # Desktop app only (Tauri dev)
pnpm run dev:landing  # Landing site on port 3001
```

`dev` uses `concurrently` to start:

- `pnpm run dev:db:logs`
- `pnpm run dev:server`
- `pnpm run dev:tauri`
- `pnpm run dev:landing`

### Desktop app (`client/`) commands

```bash
pnpm --dir client dev         # Vite dev server on :3000 (frontend only)
pnpm --dir client build       # Frontend production build
pnpm --dir client preview     # Preview built frontend on :3000
pnpm --dir client test        # Vitest run
pnpm --dir client test:watch  # Vitest watch mode
pnpm --dir client lint        # ESLint
```

### Tauri desktop commands

```bash
pnpm --dir client run tauri:dev
pnpm --dir client run tauri:dev:ios-grpc
pnpm --dir client exec tauri build
pnpm --dir client exec tauri build --debug

# enable live emulator H.264 decode support
pnpm --dir client run tauri:dev -- --features emulator-live
```

### Rust backend checks/tests

```bash
cargo check --manifest-path client/src-tauri/Cargo.toml
cargo test  --manifest-path client/src-tauri/Cargo.toml
```

Target a specific integration suite:

```bash
cargo test --manifest-path client/src-tauri/Cargo.toml --test runtime_supervisor
cargo test --manifest-path client/src-tauri/Cargo.toml --test solana_workbench
```

Prefer scoped Cargo checks/tests while iterating, and run only one Cargo command at a time so the target directory lock does not become the bottleneck.

Root helpers wrap the same policy:

```bash
pnpm run rust:test
pnpm run rust:target:prune:dry-run
pnpm run rust:target:prune
```

### Phoenix server (`server/`) commands

```bash
cd server
mix setup
mix phx.server
mix test
mix precommit
```

### Landing site (`landing/`) commands

```bash
pnpm --dir landing dev
pnpm --dir landing build
pnpm --dir landing start
pnpm --dir landing lint
```

---

## Runtime Provider Support (Desktop)

Xero supports provider profiles for:

- `openai_codex` (OAuth flow)
- `openrouter`
- `anthropic`
- `github_models`
- `openai_api` (OpenAI-compatible)
- `ollama` (local OpenAI-compatible)
- `azure_openai`
- `gemini_ai_studio`
- `bedrock` (ambient AWS creds)
- `vertex` (ambient GCP creds)

Credentials/config are managed via app state and provider profile stores (not via checked-in env files).
OpenAI-compatible setup recipes cover LiteLLM, LM Studio, Mistral, Groq, Together AI, DeepSeek, NVIDIA NIM, MiniMax, Azure AI Foundry, Atomic Chat local, and custom `/v1` gateways. See `docs/provider-setup-and-diagnostics.md` for the setup and diagnostics workflow, including the current GitHub Models token-based onboarding decision.

---

## Session Memory And Context

Xero supports session transcript search, Markdown/JSON export, context visualization, manual compact, opt-in auto-compact, reviewed memory, branch, and rewind workflows. See `docs/session-memory-and-context.md` for the user workflow, privacy guarantees, and support triage guidance.

## Agent Harness Benchmarking

Xero's owned-agent harness should be compared with fixed-model, sandboxed benchmark runs rather than informal leaderboard screenshots. See `docs/agent-harness-benchmarking.md` for the research summary, benchmark choices, and implementation plan.

## Skills And Plugins

Xero discovers static and dynamic project skills/plugins and stores trusted project artifacts in app data, not inside the imported repository. See `docs/skills-and-plugins.md` for authoring, trust, and runtime notes.

---

## Tauri Command Surface (High-Level)

Backend commands are registered in `client/src-tauri/src/lib.rs` and grouped under modules in `client/src-tauri/src/commands/`.

Major groups:

- **Project/repo:** import/list/remove projects, snapshot, git status/diff, file operations, search/replace
- **Runtime:** auth/session, start/stop runtime runs, stream subscription, operator action resolution
- **MCP:** list/upsert/remove/import MCP servers, refresh connection status
- **Notifications:** route management, credentials, dispatch records, reply submission
- **Browser:** tabbed in-app browser automation (navigate/click/type/query/cookies/storage/screenshot/diagnostics/state), plus configurable native browser fallback for owned agents
- **Emulator:** SDK status, device lifecycle/input, screenshots, UI tree/find/tap/swipe/type, app lifecycle helpers
- **Solana:** cluster lifecycle, snapshots, personas, scenario runs, tx build/sim/send/explain, ALT/IDL/PDA/program deploy flows

---

## Sidecars, Build-Time Behavior, and Feature Flags

### Rust/Cargo features (`client/src-tauri/Cargo.toml`)

- `default = []`
- `emulator-live` (enables `openh264` decoding for live emulator frame decode)
- `emulator-synthetic` (synthetic frame generator/testing path)
- `ios-grpc` (compiles vendored `idb.proto` gRPC client)

Default dev builds keep emulator/iOS gRPC dependencies out of the hot path. If `emulator-live` is not enabled, H.264 decode path reports decoder unavailable.

### `build.rs` behavior (important)

On build, Xero can:

1. Build and stage `xero-cookie-importer` helper binary
2. Fetch and verify checksum for `scrcpy-server-v2.7.jar`
3. On macOS, fetch and verify `idb-companion` universal bundle
4. Compile `proto/idb.proto` when `ios-grpc` is enabled
5. Compile the macOS dictation Swift shim when the host SDK supports it

### Build-time env var

```bash
XERO_SKIP_SIDECAR_FETCH=1
XERO_BUILD_COOKIE_IMPORTER=1
XERO_SKIP_COOKIE_IMPORTER=1
XERO_SKIP_DICTATION_SHIM=1
```

- `XERO_SKIP_SIDECAR_FETCH=1` skips scrcpy/idb download steps.
- `XERO_BUILD_COOKIE_IMPORTER=1` builds the cookie helper from its separate crate.
- `XERO_SKIP_COOKIE_IMPORTER=1` skips helper staging.
- `XERO_SKIP_DICTATION_SHIM=1` skips the macOS native dictation shim.

### Optional runtime env vars

These are optional and only needed for specific runtime integrations:

```bash
# Custom web-search provider used by autonomous web tools
XERO_AUTONOMOUS_WEB_SEARCH_URL=https://...
XERO_AUTONOMOUS_WEB_SEARCH_BEARER_TOKEN=...

# Solana workbench resource overrides
XERO_SOLANA_RESOURCE_ROOT=/path/to/resources
XERO_SOLANA_TOOLCHAIN_ROOT=/path/to/toolchain
```

---

## Persistence Model

### SQLite state

Xero stores application and project state under the OS app-data directory:

- `xero.db` for global state
- `projects/<project-id>/state.db` for per-project state

New imports do not create `<repo>/.xero/`. That directory is legacy.
Project skill artifacts also live in app data, under `projects/<project-id>/skills` and `projects/<project-id>/dynamic-skills`.

Agent memory also uses the OS app-data project store. The LanceDB-backed record/memory store is part of the Rust backend, which is why `protoc` is a build prerequisite.

### App-level JSON state

Xero also stores UI/runtime-adjacent JSON files like:

- `window-state.json`

Solana stores also use OS data dirs under `xero/solana/...` for personas/snapshots.

### Server state

The Phoenix service uses the local Postgres database from `server/docker-compose.yml` (`xero_dev` by default). Current migrations cover Oban jobs, GitHub auth sessions, and arcade game stats.

---

## Browser + Cookie Import Notes

The in-app browser supports tabbed automation, storage/cookie operations, screenshots, console/network diagnostics, accessibility snapshots, and state save/restore. Owned agents default to the in-app browser first and can fall back to native device-browser control when the browser-control preference allows it.

Cookie import helper supports detection/import from common browsers, including:

- Chrome/Chromium/Brave/Edge
- Opera/Opera GX/Vivaldi/Arc
- Firefox/LibreWolf/Zen
- Safari (macOS)

---

## Troubleshooting

### Tauri app fails to start in dev

- Ensure `pnpm --dir client install` completed
- Ensure `protoc` is installed and visible on PATH
- Ensure Tauri OS prerequisites are installed
- Ensure port `3000` is free (Vite dev server is strict on this port)

### Root `pnpm run dev` fails before Tauri starts

- Ensure Docker Desktop, Docker Engine, or a compatible Docker daemon is installed and running, or let the preflight start it where your OS allows
- Ensure Elixir/Mix is installed for the Phoenix server
- Inspect Postgres with `docker logs xero-postgres`
- Run `pnpm run dev:preflight` to isolate setup failures

### Emulator frames/status issues

- For iOS, confirm Xcode/simctl are installed (macOS only)
- For Android, confirm `adb`/`emulator` availability or run provisioning flow
- If decoder unavailable errors appear, build/run with `emulator-live` feature

### Sidecar download issues

- Use `XERO_SKIP_SIDECAR_FETCH=1` with pre-populated resources
- Or allow network access during first build so `build.rs` can fetch pinned artifacts

### Solana local cluster start failures

- Verify `solana` (and `surfpool` for fork mode) are on PATH
- Check toolchain status from Solana sidebar; missing tools are surfaced explicitly

---

## Where to Start in Code

If you’re new to this repo, start here:

1. `client/src/App.tsx` (top-level app orchestration)
2. `client/components/xero/shell.tsx` (window shell + sidebar buttons)
3. `client/src/features/xero/use-xero-desktop-state.ts` (state orchestration)
4. `client/src/lib/xero-desktop.ts` (frontend adapter + invoke/event contract)
5. `client/src-tauri/src/lib.rs` (backend command registration)
6. `client/src-tauri/src/commands/` (backend command namespaces)
7. `server/lib/xero_web/router.ex` (Phoenix routes)
8. `server/lib/xero_web/controllers/` (server callback/API controllers)

---

## Current Status Summary

This repository is actively structured around a Tauri desktop runtime with broad command surfaces for runtime orchestration, browser automation, mobile emulator control, Solana workflows, skills/plugins, and session memory. The `server/` Phoenix app and local Postgres service support web callback/shared backend features. The `landing/` app is a separate Next.js site used alongside (not instead of) the desktop host.
