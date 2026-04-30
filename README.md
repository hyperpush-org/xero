# Xero (Tauri Desktop)

Xero is a **Tauri desktop app** for agentic development workflows, with a React/Vite frontend and a Rust backend command surface.

It combines:

- Project/repository import + file editing/search operations
- Runtime/session orchestration for AI providers
- Autonomous run + operator approval loop support
- In-app browser automation
- iOS/Android emulator sidebars + automation hooks
- Solana workbench tooling (clusters, personas, tx pipeline, deploy helpers)
- MCP server registry management
- Notification routing (Telegram, Discord)

> Important: this is a **desktop-first Tauri app**. For end-to-end behavior, run via Tauri (`tauri dev`), not as a plain browser app.

---

## Repository Layout

```text
.
├─ client/                 # Main desktop app (React + Vite + Tauri + Rust)
│  ├─ src/                 # App entry + feature hooks
│  ├─ components/xero/  # Main UI shell + sidebars + views
│  ├─ src-tauri/           # Rust backend, commands, state, tests
│  └─ package.json
├─ landing/                # Separate Next.js marketing site
├─ EMULATOR_SIDEBAR_PLAN.md
├─ SOLANA_WORKBENCH_PLAN.md
└─ package.json            # Root convenience scripts
```

### Key top-level projects

- `client/`: production desktop app (`productName: Xero`, `identifier: dev.sn0w.xero`)
- `landing/`: separate website, run on port `3001` in root dev workflow

### Non-runtime/reference content

- `.tmp-gsd2-ref/`: reference snapshot directory (ignored by build workflows)

---

## Tech Stack

### Desktop app (`client/`)

- **Frontend:** React 19, TypeScript, Vite, Vitest, ShadCN/Radix UI, Tailwind CSS
- **Desktop host:** Tauri v2
- **Backend:** Rust (command surface + orchestration + persistence)
- **Storage:** SQLite under the OS app-data directory

### Landing site (`landing/`)

- Next.js 16, TypeScript, Tailwind CSS

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
- Tauri OS prerequisites for your platform (WebView/runtime dependencies)

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

---

## Setup

This repo is **not** a pnpm workspace; install dependencies per package.

```bash
# root scripts
pnpm install

# desktop app
pnpm --dir client install

# landing site
pnpm --dir landing install
```

---

## Command Reference

### Root commands

```bash
pnpm run dev         # Runs Tauri desktop + landing site together
pnpm run dev:tauri   # Runs desktop app only (Tauri dev)
pnpm run dev:landing # Runs landing site on port 3001
```

`dev` uses `concurrently` to start:

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

---

## Tauri Command Surface (High-Level)

Backend commands are registered in `client/src-tauri/src/lib.rs` and grouped under modules in `client/src-tauri/src/commands/`.

Major groups:

- **Project/repo:** import/list/remove projects, snapshot, git status/diff, file operations, search/replace
- **Runtime:** auth/session, start/stop runtime runs, stream subscription, operator action resolution
- **MCP:** list/upsert/remove/import MCP servers, refresh connection status
- **Notifications:** route management, credentials, dispatch records, reply submission
- **Browser:** tabbed in-app browser automation (navigate/click/type/query/cookies/storage/screenshot)
- **Emulator:** SDK status, device lifecycle/input, screenshots, UI tree/find/tap/swipe/type, app lifecycle helpers
- **Solana:** cluster lifecycle, snapshots, personas, scenario runs, tx build/sim/send/explain, ALT/IDL/PDA/program deploy flows

---

## Sidecars, Build-Time Behavior, and Feature Flags

### Rust/Cargo features (`client/src-tauri/Cargo.toml`)

- `default = ["ios-grpc"]`
- `emulator-live` (enables `openh264` decoding for live emulator frame decode)
- `emulator-synthetic` (synthetic frame generator/testing path)
- `ios-grpc` (compiles vendored `idb.proto` gRPC client)

If `emulator-live` is not enabled, H.264 decode path reports decoder unavailable.

### `build.rs` behavior (important)

On build, Xero can:

1. Build and stage `xero-cookie-importer` helper binary
2. Fetch and verify checksum for `scrcpy-server-v2.7.jar`
3. On macOS, fetch and verify `idb-companion` universal bundle
4. Compile `proto/idb.proto` when `ios-grpc` is enabled

### Build-time env var

```bash
XERO_SKIP_SIDECAR_FETCH=1
```

Skips sidecar download steps (useful for CI/offline/pre-cached environments).

### Optional runtime env vars

These are optional and only needed for specific runtime integrations:

```bash
# Custom web-search provider used by autonomous web tools
XERO_AUTONOMOUS_WEB_SEARCH_URL=https://...
XERO_AUTONOMOUS_WEB_SEARCH_BEARER_TOKEN=...
```

---

## Persistence Model

### SQLite state

Xero stores application and project state under the OS app-data directory:

- `xero.db` for global state
- `projects/<project-id>/state.db` for per-project state

New imports do not create `<repo>/.xero/`. That directory is legacy.
Project skill artifacts also live in app data, under `projects/<project-id>/skills` and `projects/<project-id>/dynamic-skills`.

### App-level JSON state

Xero also stores UI/runtime-adjacent JSON files like:

- `window-state.json`

Solana stores also use OS data dirs under `xero/solana/...` for personas/snapshots.

---

## Browser + Cookie Import Notes

The in-app browser supports tabbed automation and storage/cookie operations.

Cookie import helper supports detection/import from common browsers, including:

- Chrome/Chromium/Brave/Edge
- Opera/Opera GX/Vivaldi/Arc
- Firefox/LibreWolf/Zen
- Safari (macOS)

---

## Troubleshooting

### Tauri app fails to start in dev

- Ensure `pnpm --dir client install` completed
- Ensure Tauri OS prerequisites are installed
- Ensure port `3000` is free (Vite dev server is strict on this port)

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

For deep subsystem planning context:

- `EMULATOR_SIDEBAR_PLAN.md`
- `SOLANA_WORKBENCH_PLAN.md`

---

## Current Status Summary

This repository is actively structured around a Tauri desktop runtime with broad command surfaces for runtime orchestration, browser automation, mobile emulator control, and Solana workflows. The `landing/` app is a separate Next.js site used alongside (not instead of) the desktop host.
