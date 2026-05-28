# Desktop Platform Verification Matrix

This matrix is the runnable release gate for Xero desktop platform support.
It intentionally references test targets that exist in the current tree.

## Per-Host Gate

Run these commands on every desktop host. Keep Cargo commands serialized so
the Cargo lock is never contended.

```bash
pnpm --dir client test
cargo test --manifest-path client/src-tauri/Cargo.toml
cargo check --manifest-path client/src-tauri/Cargo.toml
pnpm --dir client exec tauri build --debug
```

## Focused Smoke Set

For fast platform triage before the full gate, run:

```bash
cargo test --manifest-path client/src-tauri/Cargo.toml --test platform_adapters
cargo test --manifest-path client/src-tauri/Cargo.toml --test autonomous_tool_runtime
cargo test --manifest-path client/src-tauri/Cargo.toml --test provider_diagnostics_contract
cargo test --manifest-path client/src-tauri/Cargo.toml --test solana_workbench
pnpm --dir client test components/xero/shell.test.tsx src/lib/xero-model/diagnostics.test.ts src/lib/xero-model/session-context.test.ts
```

## Host Targets

| Host | Required result | Notes |
| --- | --- | --- |
| macOS arm64 | Per-host gate passes | Verifies macOS dictation/iOS code paths plus shared desktop support. |
| macOS x64 | Per-host gate passes where hardware/CI is available | Same commands; no test-name substitutions. |
| Windows x64 | Per-host gate passes | Verifies Windows shell selection, native SDK dictation, process/port parsers, `taskkill` signaling, and `.cmd`/`.bat` behavior. |
| Linux x64 | Per-host gate passes | Verifies `/proc` process/port inspection and Linux packaging. |

## Platform-Only Features

- iOS Simulator remains macOS-only and must return typed unavailable results
  outside macOS.
- Native desktop dictation is supported on macOS and Windows. Windows must use
  the Windows SDK engine through the shared composer contract, emit audio-level
  events when microphone metering is available, and keep `modern`/`legacy`
  preferences rejected because those names are macOS-specific.
- Linux native dictation must report no native engine and keep the composer mic
  hidden outside browser-backed cloud dictation.
- macOS automation remains macOS-only and must return typed unavailable
  results outside macOS.
- Android emulator support is expected on macOS, Windows, and Linux when
  SDK/JDK/hypervisor prerequisites are present.

## Windows Dictation Manual Smoke

On Windows 10 or Windows 11:

1. Connect a microphone.
2. Enable microphone access for desktop apps.
3. Enable Windows online speech recognition if the SDK path requires it.
4. Start Xero, focus the agent composer, and press `Ctrl+Shift+D`.
5. Speak a short phrase and confirm the waveform moves and text reaches the composer.
6. Press `Ctrl+Shift+D` again and confirm microphone activity stops.
7. Run diagnostics and confirm Windows dictation reports the SDK engine status, microphone state, speech privacy state, and locale support.
