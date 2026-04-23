# Bundled emulator sidecars

This directory holds binaries we embed into the Cadence desktop bundle at
build time. They are resolved at runtime via `app.path().resolve(..., Resource)`.

## scrcpy-server-v2.7.jar

Required for the Android emulator sidebar to stream frames.

**Auto-fetched by `build.rs`**: the first `cargo build` downloads the jar
from the pinned Genymobile release and verifies the SHA-256. The pinned
version and digest live in `build.rs::SCRCPY_VERSION` / `SCRCPY_SHA256`;
keep them in lockstep with `src/commands/emulator/android/scrcpy.rs::SCRCPY_VERSION`.

To skip the fetch in a CI environment that handles sidecar caching
itself (or to iterate offline once the jar is present), set
`CADENCE_SKIP_SIDECAR_FETCH=1` in the build environment.

Manual override: drop `scrcpy-server-v2.7.jar` into this directory and
the verification step will pick it up without fetching. SHA mismatch
triggers a re-fetch.

Apache 2.0 licensed (Genymobile) — a matching `NOTICE` entry ships in
Cadence's About dialog as required.

## idb_companion (macOS-only)

Required for the iOS simulator sidebar to stream frames.

**Not auto-fetched** — the upstream macOS universal binary is ~50 MB and
not consistently published as a release asset. Either:

1. Install via Homebrew: `brew install facebook/fb/idb-companion`. The
   SDK probe picks it up from `/opt/homebrew/bin` or `/usr/local/bin`.
2. Drop a universal binary into `resources/binaries/idb_companion` and
   reference it from `tauri.conf.json` as an `externalBin`.

The iOS pipeline resolves `idb_companion` in this order:

1. Tauri resource directory (this folder).
2. `which idb_companion` on `PATH`.
3. `/opt/homebrew/bin/idb_companion`, `/usr/local/bin/idb_companion`.

MIT-licensed (Meta / facebook/idb).
