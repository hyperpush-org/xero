# Computer Use Desktop Control

This document captures the production contract for Computer Use desktop observation, control, cloud viewing, and manual cloud control.

The companion threat model is in [COMPUTER_USE_DESKTOP_CONTROL_THREAT_MODEL.md](COMPUTER_USE_DESKTOP_CONTROL_THREAT_MODEL.md).

## Safety Boundary

Desktop control is UI-only. It does not grant Computer Use shell access, repository write access, MCP access, process management, skill execution, or arbitrary script execution. Every desktop action enters through the Tauri desktop broker, is classified by policy, uses the shared controller lock, and writes a local audit record under OS app-data-backed project state.

Cloud stream and manual-control commands are accepted only from an already paired web device for a Computer Use session. The desktop app keeps separate local opt-ins for cloud viewing and cloud manual control in Settings > Desktop Control. Manual input is normalized in the cloud UI and then converted into normal `desktop_control` requests; the cloud never sends raw sidecar messages.

## Tool Families

`desktop_observe` supports permission status, displays, windows, apps, foreground state, screenshots, cursor state, element lookup, health, macOS Accessibility snapshots, and OCR snapshots. The authenticated sidecar now serves display/window/app/foreground observation, cursor state, element-at-point hit testing, bounded AX tree snapshots, approved PNG screenshot capture, and macOS Vision OCR through the shared IPC contract; the Tauri broker keeps its limited in-process observation fallback for degraded hosts and remains responsible for writing approved screenshot artifacts under OS app-data. Windows and Linux OCR report `sidecar_operation_unimplemented` until their native OCR engines are attached.

`desktop_control` supports the platform-neutral request schema for pointer, keyboard, text, app, menu, clipboard, and Accessibility actions. On macOS the authenticated sidecar dispatches mouse move/click/drag, scroll, key press, hotkey, text input, clipboard-mediated paste, AX press/focus/set-value, and focused-app menu selection after the Tauri broker has enforced policy, approval, controller lock, and audit. On Windows and Linux the Rust sidecar dispatches brokered pointer, keyboard, text, scroll, drag, cursor-state, and clipboard-mediated paste through the cross-platform native input backend; Accessibility actions and menu selection still report `sidecar_operation_unimplemented` until their platform-specific backends are attached. The paste path writes only the supplied text to the system clipboard/pasteboard and sends paste input; it does not read or restore the user's previous clipboard contents. The broker keeps a narrow in-process macOS fallback only when the sidecar is unavailable or explicitly reports an unimplemented pointer/keyboard operation. App launch/activate/quit/window focus still route through the macOS automation backend.

`desktop_stream` supports capability/status/start/stop/quality/keyframe requests. WebRTC signaling messages are represented in the remote bridge contract, and the Phoenix session join supplies short-lived ICE server credentials plus a stream token bound to the joined desktop, session, web device, and current run when one is active. The cloud session viewport can answer desktop stream offers, return ICE candidates, render future media-track streams, and render the current sidecar's WebRTC data-channel frame stream. The relay rejects web-originated stream and manual-control command frames without a valid token, and run-bound tokens require the command payload to carry the matching `runId`. The broker prefers the authenticated sidecar for stream start/status/stop/quality/keyframe actions whenever the sidecar advertises `webrtcStream`; WebRTC offer/answer/ICE payloads are typed in the shared IPC contract and routed through the broker to the sidecar when a native stream is active. The cross-platform sidecar now advertises `webrtcStream: true`, creates a browser-answerable WebRTC offer, and publishes redacted JPEG desktop frames over an authenticated `xero-desktop-stream` data channel. Native sidecar failures degrade to the screenshot fallback when that fallback is available. The remote bridge captures bounded ephemeral fallback frames on stream start/status, deletes the local screenshot file after reading it, downscales and JPEG-compresses the redacted frame, and relays the compressed payload directly to the cloud viewport. Web-originated SDP and ICE payloads are not echoed back to the cloud viewport in relay acknowledgements.

## Controller Lock

The controller lock is process-wide and shared across agent runs, local status commands, and remote bridge commands. A lock contains:

- actor: `agent`, `local_user`, or `cloud_manual_control`
- session ID
- run ID
- acquisition time
- lease expiration
- last input time
- release reason

Only one actor can hold the lock while the lease is active. Emergency stop releases the lock and stops stream state.

On macOS, recent HID mouse, scroll, modifier, or key input is treated as local user takeover. Brokered agent/cloud input backs off with `local_user_takeover` so the user can finish before control resumes.

## Approval And Deny Policy

Sensitive observation, native input, app control, clipboard use, streaming start, and manual control are approval or opt-in gated. Secret text is denied. Password managers, Keychain-like contexts, browser-saved-password contexts, payment confirmation, purchasing, money transfer, identity verification, account ownership, wallet/financial/tax/payroll/insurance/MFA/recovery contexts, and system privacy/security settings are blocked by default. The broker evaluates app name, bundle ID, element ID, menu path, and the operator-visible reason when applying this deny policy.

The broker returns stable machine-readable errors such as `approval_required`, `policy_denied`, `controller_lock_unavailable`, `permission_screen_recording_denied`, `permission_accessibility_denied`, `sidecar_unavailable`, `display_not_found`, and `coordinates_out_of_bounds`.

## Local IPC / Sidecar Contract

The shared sidecar request contract is:

```json
{
  "schemaVersion": 1,
  "protocol": "xero.desktop_sidecar.ipc.v1",
  "requestId": "req_123",
  "sessionId": "agent-session-global-computer-use",
  "runId": "run_456",
  "actor": "agent",
  "operation": "mouse_click",
  "payload": {},
  "policyDecisionId": "policy_789",
  "auth": {
    "scheme": "bearer_session_token",
    "token": "short-lived-session-token"
  },
  "expiresAt": "2026-05-26T12:00:30Z"
}
```

The Tauri broker launches `xero-desktop-sidecar` as internal infrastructure, sends a short-lived handshake over stdio, verifies the bearer token hash on every request, and keeps the general process manager out of the path. Sidecars must reject unauthenticated callers, expired leases, invalid schemas, unsupported operations, and any operation outside the declared desktop contract. The schema validator also rejects shell/script/plugin/file-path payload keys. There is no shell, arbitrary file, plugin-loading, or background-control API.

Build and packaging notes:

- The shared IPC contract lives in `client/src-tauri/crates/xero-desktop-control-ipc`; stream requests and stream state payloads are typed there so platform publishers can be added without changing the broker/cloud contract.
- The cross-platform sidecar scaffold lives in `client/src-tauri/crates/xero-desktop-sidecar`.
- Build the sidecar with `cargo build --manifest-path client/src-tauri/Cargo.toml --package xero-desktop-sidecar`.
- CI runs the shared IPC contract tests and sidecar tests on macOS, Windows, and Linux through the `Desktop Sidecar Contract` matrix job.
- Release packaging builds the sidecar before Tauri packaging, bundles `resources/xero-desktop-sidecar*`, and embeds or sets `XERO_DESKTOP_SIDECAR_SHA256` so the broker refuses an unexpected sidecar binary.

## Rollout Flags

Desktop tools are exposed only on macOS, Windows, and Linux and are also gated by rollout configuration. Debug and test builds default to enabled for local development. Release builds fail closed unless one of these environment flags enables the surface:

- `XERO_COMPUTER_USE_DESKTOP_ENABLED`: enables or disables all Computer Use desktop tools.
- `XERO_COMPUTER_USE_DESKTOP_OBSERVE_ENABLED`: overrides `desktop_observe`.
- `XERO_COMPUTER_USE_DESKTOP_CONTROL_ENABLED`: overrides `desktop_control` and manual cloud input.
- `XERO_COMPUTER_USE_DESKTOP_STREAM_ENABLED`: overrides `desktop_stream`.
- `XERO_COMPUTER_USE_DESKTOP_ROLLOUT_PERCENT`: enables deterministic host/tool bucketing from `0` to `100` when explicit flags are absent.
- `XERO_COMPUTER_USE_DESKTOP_ROLLOUT_ID`: stable rollout ID used for bucketing; otherwise the app falls back to installation/host environment identifiers.

Boolean flags accept `1`, `true`, `yes`, `on`, or `enabled`; `0`, `false`, `no`, `off`, or `disabled`. Per-tool flags override the master flag. The runtime descriptor surface and direct remote-bridge broker paths both honor the rollout gate.

## Cloud Stream And Manual Control

Cloud stream commands:

- `computer_use_stream_request`
- `computer_use_stream_offer`
- `computer_use_stream_answer`
- `computer_use_stream_ice_candidate`
- `computer_use_stream_stop`
- `computer_use_stream_status`
- `computer_use_stream_set_quality`
- `computer_use_stream_request_keyframe`

Manual control commands:

- `computer_use_manual_control_request`
- `computer_use_manual_control_grant`
- `computer_use_manual_control_heartbeat`
- `computer_use_manual_control_input`
- `computer_use_manual_control_release`

Manual control requires local opt-in, a paired device, a Computer Use session, and the shared controller lock. The cloud viewport refreshes the manual-control lease while control is active; brokered input without an active lease is denied with `manual_control_lease_required`. The cloud viewport captures pointer, wheel, and keyboard events only while manual control is active. The viewport also exposes quality and refresh controls for the active stream, plus an emergency stop that releases manual control, stops the stream, and cancels the active run through the broker. In screenshot fallback mode the stream controls adjust bounded fallback frame cadence and request an immediate brokered frame. User-marked private regions configured in the desktop app are applied to fallback frames before the screenshot file is read for relay; the relayed frame is JPEG-compressed, bounded by the active stream max width, and includes `redactionsApplied`. The desktop app shows a persistent local safety banner whenever a stream or desktop controller lock is active, with Settings access and an emergency Stop action that routes through the broker.

## Data Handling

Raw video is not persisted and stream recording is not supported. Audit records store metadata, policy decisions, status, error code, and a redacted summary. Stream lifecycle metadata is written separately under OS app-data as JSONL records containing stream ID, session/run IDs, transport, status, quality, resolution cap, frame-rate cap, cursor inclusion, audit ID, and error code; frame bytes are never stored there. On Unix platforms these JSONL directories/files are hardened to owner-only permissions and symlinked log paths are rejected before append. Screenshots are written only as bounded runtime media artifacts for explicit observation/fallback use. Desktop-control settings live under OS app-data and include local opt-ins plus the private-region redaction list; `.xero/` is not used for this state.

## Operations Runbook

If permissions fail, open Settings > Desktop Control and use the permission row actions to open the local OS privacy pane. Refresh status after granting permissions.

If stream setup fails, confirm cloud viewing is enabled locally, request a new stream, and inspect the desktop-control audit log path shown in Settings. If the stream state is `degraded`, the sidecar returned a native-stream error or the host does not support the advertised stream backend and the broker switched to bounded screenshot frames.

If manual control behaves unexpectedly, use the local emergency stop. This releases the controller lock, stops stream state, and records an audit event.

For WebRTC deployments, configure `XERO_WEBRTC_STUN_URLS`, `XERO_TURN_URLS`, `XERO_TURN_SHARED_SECRET`, and `XERO_TURN_TTL_SECONDS` on the Phoenix server. The server issues coturn REST-style credentials in the session join response; the shared TURN secret stays server-side. Deploy coturn or an equivalent TURN service separately, and monitor relay usage and connection failure reasons before enabling production traffic.

Phoenix telemetry exposes Computer Use relay counters for `xero.remote.computer_use.command.forwarded.count`, `xero.remote.computer_use.command.forwarded.bytes`, and `xero.remote.computer_use.command.rejected.count`. Tags include command family, kind, direction, and rejection reason where applicable.
