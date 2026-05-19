# Adrenaline Mode Implementation Plan

## Reader And Goal

This plan is for the engineer implementing Xero's first sleep-prevention setting. After reading it, they should be able to add a user-facing Settings toggle that keeps the Mac awake while Xero is running, understand the separate risk profile of lid-closed behavior, and verify the feature without adding temporary UI.

## Research Summary

The public `adrenaline` Python package implements macOS support by loading IOKit, creating a power-management assertion with `IOPMAssertionCreateWithName`, and releasing that assertion later with `IOPMAssertionRelease`. Its default assertion type is `NoIdleSleepAssertion`; when the caller asks to keep the display awake, it uses `NoDisplaySleepAssertion`. It verifies behavior by checking `pmset -g assertions`.

Apple's IOKit power assertion APIs are the correct app-level primitive for preventing idle sleep. The important boundary is that normal app assertions prevent idle sleep while the process holds the assertion; they do not promise to override every sleep reason. Apple's documentation explicitly distinguishes idle sleep prevention from lid close, manual sleep, low battery, and thermal cases.

Closed-lid behavior is different. Community tools and scripts that keep a MacBook awake with the lid closed usually wrap `pmset disablesleep`, which requires root and changes global power-management settings. It is not just another process-scoped assertion. The `pmset` manual also states that modifying power settings requires root and that settings are persisted at the system level, while IOKit assertions are dynamic process overrides.

Sources:

- Adrenaline macOS implementation: https://github.com/ntamas/adrenaline/blob/main/src/adrenaline/_impl/darwin.py
- Adrenaline package overview: https://pypi.org/project/adrenaline/
- Apple IOPM assertion types: https://developer.apple.com/documentation/iokit/iopmlib_h/iopmassertiontypes
- Apple idle sleep assertion notes: https://developer.apple.com/documentation/iokit/kiopmassertiontypepreventuseridlesystemsleep
- `pmset` manual mirror: https://keith.github.io/xcode-man-pages/pmset.1.html
- Example lid-close wrapper script: https://github.com/Moarram/wake

## Product Decision

Implement Adrenaline Mode in two layers:

1. **Default Adrenaline Mode**, shipped first: process-scoped macOS IOKit assertion owned by Xero. This keeps the system awake during idle while Xero is open and avoids requiring admin privileges.
2. **Extended Lid-Closed Mode**, designed but gated behind a separate implementation decision: privileged, explicit, reversible support for `pmset disablesleep` or a blessed helper. This should not be silently included in the basic toggle because it changes global system behavior.

The Settings UI should present the shipped toggle as "Adrenaline Mode" and describe the active guarantee as keeping Xero's Mac awake while the app is running. If extended lid-closed support is implemented later, it should be a separate control with clearer warning text and an admin authorization path.

## Scope For The First Implementation

Add a Settings section or card under system/workspace settings for Adrenaline Mode. The control should use ShadCN UI components where possible, especially a `Switch`, `Alert`, and existing section/card patterns.

Persist the preference in OS app-data backed global state, not `.xero/`. This repo already stores app-global settings in the global database, and that pattern fits better than project UI state because this is a machine/app-level preference.

When the user enables the toggle, the backend should create and hold one process-wide macOS power assertion. When the user disables it, the backend should release the assertion. When Xero starts, it should load the persisted preference and reacquire the assertion if enabled. When Xero exits or the main window closes, it should release the assertion best-effort.

The feature should be macOS-only at runtime. On unsupported platforms, return a clean unsupported status and keep the UI disabled or explanatory rather than failing.

## Backend Plan

Add a new command module for power management settings and runtime state.

Define DTOs:

- `AdrenalineModeSettingsDto`: enabled, display behavior, active status, platform support, updated timestamp, optional diagnostic message.
- `UpsertAdrenalineModeSettingsRequestDto`: enabled plus the selected assertion mode.
- `AdrenalineModeAssertionKindDto`: start with `prevent_idle_system_sleep`; optionally include `prevent_idle_display_sleep` if we want display-awake behavior in the first UI.

Persist settings using a global database table with a JSON payload and schema version. Match the fail-closed style used by other settings files: reject unknown schema versions instead of migrating, because this is a new app and backwards compatibility is prohibited unless requested.

Add process-wide state:

- Store the current assertion ID behind a `Mutex`.
- Treat enabling as idempotent: if an assertion is already active, return the current status.
- Treat disabling as idempotent: if no assertion exists, persist disabled and return inactive.
- Release on update from enabled to disabled, on shutdown, and before switching assertion kinds.

On macOS, implement a small IOKit wrapper:

- Link against IOKit with Rust FFI.
- Use `core-foundation` to build `CFStringRef` values.
- Prefer `IOPMAssertionCreateWithDescription` if practical, because Apple calls it the preferred creation API and it allows better names/details for `pmset -g assertions`.
- Use `kIOPMAssertionLevelOn`.
- Use a clear assertion name such as `Xero Adrenaline Mode`.
- Release with `IOPMAssertionRelease`.

On non-macOS, provide a stub implementation that reports unsupported without trying to call IOKit.

Register the commands in the Tauri command handler and add the new state with the existing builder-managed state. Add startup wiring in setup so persisted enabled settings are applied after the global database path is available.

## Frontend Plan

Add a typed model module for Adrenaline Mode settings and request validation with Zod.

Extend the desktop adapter with:

- `adrenalineModeSettings()`
- `adrenalineModeUpdateSettings(request)`

Add a settings UI entry. The most natural placement is a new "Power" section in the Workspace group, or a compact "Power" card inside Diagnostics if a new section feels too heavy. A dedicated "Power" section is cleaner because this is a durable user preference rather than a diagnostic.

Use user-facing copy that stays precise:

- Toggle label: `Adrenaline Mode`
- Short body: `Keep this Mac awake while Xero is running.`
- Status text when active: `Active`
- Status text on unsupported platforms: `macOS only`
- Warning text near any future lid-closed mode: `Closed-lid mode changes global macOS power settings and can increase heat and battery drain.`

Do not add temporary debug UI. Use tests and `pmset -g assertions` verification instead.

## Extended Lid-Closed Mode Design

Do not make the first toggle run `sudo pmset disablesleep 1`.

If the product requirement becomes "must stay awake when the lid is closed even without an external display," implement it as a separate "Allow lid-closed operation" mode with these safeguards:

- Explicit separate opt-in, not bundled into normal Adrenaline Mode.
- Admin authorization flow via a privileged helper or a clear OS prompt.
- Always restore `pmset disablesleep 0` when the mode is disabled.
- On startup, detect stale global state and offer repair rather than silently leaving the machine in a no-sleep state.
- Show a persistent warning while enabled.
- Verify current `pmset -g` state before and after changes.
- Prefer AC-power-only behavior if feasible.

This mode may be unreliable or constrained on newer Apple Silicon/macOS combinations. The UI must present that as a system limitation, not as a Xero error.

## Test Plan

Backend unit tests:

- Default settings load disabled when no row exists.
- Settings persist and round trip.
- Unknown schema versions fail closed.
- Enable is idempotent.
- Disable is idempotent.
- Switching assertion kinds releases the old assertion before acquiring the new one.
- Unsupported platforms return unsupported status without panicking.

Backend integration/manual verification on macOS:

- Enable from the command path.
- Run `pmset -g assertions` and confirm an assertion named `Xero Adrenaline Mode`.
- Disable and confirm the assertion disappears.
- Quit Xero while enabled and confirm the assertion is released.
- Restart with the persisted toggle enabled and confirm the assertion reacquires.

Frontend tests:

- Settings navigation exposes the new Power section/card.
- Toggle loads current state.
- Toggle calls the adapter with the expected request.
- Save failure rolls back the optimistic UI state and shows an error.
- Unsupported platform renders disabled state.
- No temporary/debug UI is introduced.

Run scoped checks only:

- Rust tests for the new command module and any touched settings modules.
- Frontend tests for the settings dialog and the new model/adapter tests.
- Targeted format for edited Rust and TypeScript files.

## Rollout Notes

Start with idle-sleep prevention because it is process-scoped, reversible, and does not need root. Keep the lid-closed feature as a follow-up unless the user explicitly accepts privileged global power-setting behavior.

Name the feature consistently as "Adrenaline Mode" in user-facing UI. Avoid "workflow" terminology entirely here; this is a Settings/system feature and should not touch agent Stages naming.
