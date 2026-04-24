# Bundled sidecars

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
`Library not loaded: @rpath/...` at spawn time. Tauri copies every file
under the directory into the bundle's `Resources/resources/`, preserving
that layout.

The pinned version and digest live in `build.rs::IDB_COMPANION_VERSION` /
`IDB_COMPANION_SHA256`. A `.cadence-version` sentinel inside the
extracted directory lets incremental builds skip the refetch. Bumping
the pin means:

1. Update `IDB_COMPANION_VERSION` + `IDB_COMPANION_SHA256` in `build.rs`.
2. `rm -rf resources/idb-companion.universal resources/*.tar.gz` once so
   the sentinel mismatches and the fetcher re-runs on the next build.

Set `CADENCE_SKIP_SIDECAR_FETCH=1` to bypass the fetcher entirely.

Manual override: drop a compatible `idb-companion.universal/` tree into
this directory (for example, the one Homebrew installs at
`/opt/homebrew/opt/idb-companion`) and the fetcher will see the version
sentinel is already in place.

### Runtime resolution order

The iOS pipeline picks up `idb_companion` in this order:

1. Tauri resource directory — the bundled `idb-companion.universal/bin/idb_companion`.
2. `which idb_companion` on `PATH`.
3. `/opt/homebrew/bin/idb_companion`, `/usr/local/bin/idb_companion`
   (Homebrew fallbacks for dev builds that skipped the fetcher).

MIT-licensed (Meta / facebook/idb).

### Signing / notarization

`tauri build` code-signs everything under `Resources/`, including every
framework inside `idb-companion.universal/Frameworks`. After a release
build, smoke-test with:

```
codesign --verify --deep --strict \
  target/release/bundle/macos/Cadence.app
```

and check that `spctl --assess --type execute --verbose` returns
"accepted". Both should pass without additional entitlements — the
frameworks are already signed by Meta, and Tauri's codesign pass
re-signs them under the Cadence identity.

### Required host state

Bundling `idb_companion` removes the "install via Homebrew" step from
the user's onboarding. It does **not** remove the need for Xcode itself —
`idb_companion` links against Apple's private
`CoreSimulator.framework`, which only ships inside the Xcode.app
install. The titlebar discovery UI surfaces an "Install Xcode" CTA when
`xcrun` is missing.

## solana-toolchain/

Optional Solana workbench resource root. Cadence checks this directory before
the first-run managed install location and before host PATH, so release builds
can ship pre-hydrated Solana tools without relying on a user's shell setup.

The bundled/managed layout is documented in `solana-toolchain/README.md`.
Development builds can leave it empty; the sidebar exposes a managed download
for Agave `v3.1.13` and Anchor CLI `v1.0.0`.
