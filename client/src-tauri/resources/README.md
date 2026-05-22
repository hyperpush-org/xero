# Bundled sidecars

This directory holds sidecar assets that Xero can embed into desktop
bundles or use during local builds. Bundled assets are resolved at runtime
via `app.path().resolve(..., Resource)`.

## scrcpy-server-v2.7.jar

Required for the Android emulator sidebar to stream frames.

**Auto-fetched by `build.rs`**: the first `cargo build` downloads the jar
from the pinned Genymobile release and verifies the SHA-256. The pinned
version and digest live in `build.rs::SCRCPY_VERSION` / `SCRCPY_SHA256`;
keep them in lockstep with `src/commands/emulator/android/scrcpy.rs::SCRCPY_VERSION`.

To skip the fetch in a CI environment that handles sidecar caching
itself (or to iterate offline once the jar is present), set
`XERO_SKIP_SIDECAR_FETCH=1` in the build environment.

Manual override: drop `scrcpy-server-v2.7.jar` into this directory and
the verification step will pick it up without fetching. SHA mismatch
triggers a re-fetch.

Apache 2.0 licensed (Genymobile) — a matching `NOTICE` entry ships in
Xero's About dialog as required.

## idb-companion.universal/ (macOS-only)

Required for the iOS simulator sidebar to stream frames.

**Auto-fetched by `build.rs` on macOS**: the first `cargo build` downloads
the pinned `idb-companion.universal.tar.gz` from the `facebook/idb` GitHub
release, verifies the SHA-256, and extracts into
`resources/idb-companion.universal/` preserving the upstream layout:

```
idb-companion.universal/
├── bin/idb_companion
└── Frameworks/        # XCTestBootstrap, FBSimulatorControl, FBControlCore, …
```

The binary's `LC_RPATH` is `@executable_path/../Frameworks`, so the tree
has to stay intact — shipping only `bin/idb_companion` yields
`Library not loaded: @rpath/...` at spawn time.

The base `tauri.conf.json` intentionally excludes this tree so Windows
and Linux bundles do not depend on macOS-only resources. The signed macOS
release overlay also excludes it: Apple's notarization service rejects the
upstream universal framework bundle after Tauri resource copying. Release
builds therefore rely on the runtime Homebrew / `PATH` fallback for
`idb_companion`.

The pinned version and digest live in `build.rs::IDB_COMPANION_VERSION` /
`IDB_COMPANION_SHA256`. A `.xero-version` sentinel inside the
extracted directory lets incremental builds skip the refetch. Bumping
the pin means:

1. Update `IDB_COMPANION_VERSION` + `IDB_COMPANION_SHA256` in `build.rs`.
2. `rm -rf resources/idb-companion.universal resources/*.tar.gz` once so
   the sentinel mismatches and the fetcher re-runs on the next build.

Set `XERO_SKIP_IDB_COMPANION_FETCH=1` to bypass only this fetcher. Signed
macOS CI sets this because the tree is not bundled into notarized apps.
`XERO_SKIP_SIDECAR_FETCH=1` bypasses every resource fetcher and should only
be used by builds that also remove all bundled resources.

Manual override: drop a compatible `idb-companion.universal/` tree into
this directory (for example, the one Homebrew installs at
`/opt/homebrew/opt/idb-companion`) and the fetcher will see the version
sentinel is already in place.

### Runtime resolution order

The iOS pipeline picks up `idb_companion` in this order:

1. Tauri resource directory — a manually bundled or local
   `idb-companion.universal/bin/idb_companion`.
2. `which idb_companion` on `PATH`.
3. `/opt/homebrew/bin/idb_companion`, `/usr/local/bin/idb_companion`
   (Homebrew fallbacks for dev builds that skipped the fetcher).

MIT-licensed (Meta / facebook/idb).

### Signing / notarization

Release builds do not bundle `idb-companion.universal`, so the normal app
signing check is enough:

```
codesign --verify --deep --strict \
  target/release/bundle/macos/Xero.app
```

and check that `spctl --assess --type execute --verbose` returns
"accepted".

### Required host state

Because release builds do not bundle `idb_companion`, users need an
install discoverable via Homebrew or `PATH`. They also need Xcode itself:
`idb_companion` links against Apple's private
`CoreSimulator.framework`, which only ships inside the Xcode.app
install. The titlebar discovery UI surfaces an "Install Xcode" CTA when
`xcrun` is missing.

## solana-toolchain/

Optional Solana workbench resource root. Xero checks this directory before
the first-run managed install location and before host PATH, so release builds
can ship pre-hydrated Solana tools without relying on a user's shell setup.

The bundled/managed layout is documented in `solana-toolchain/README.md`.
Development builds can leave it empty; the sidebar exposes a managed download
for Agave `v3.1.13` and Anchor CLI `v1.0.0`.
