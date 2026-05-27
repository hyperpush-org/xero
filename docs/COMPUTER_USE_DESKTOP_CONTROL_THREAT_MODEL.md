# Computer Use Desktop Control Threat Model

## Scope

This threat model covers Computer Use desktop observation, brokered native input, desktop streaming, cloud manual control, and the local sidecar IPC boundary.

Out of scope for this release:

- Generic shell, script, file, MCP, process-manager, or plugin execution by Computer Use.
- Locked, logged-out, sleeping, pre-login, or privileged daemon control.
- Stream recording or raw video persistence.

## Trust Boundaries

- Cloud browser to relay: authenticated paired web device, session-bound command frames, revocation checks.
- Relay to desktop: existing remote bridge device identity, Computer Use session binding, local opt-in checks.
- Desktop broker to sidecar: local stdio IPC, short-lived bearer session token, schema validation, operation allowlist, checksum-verified sidecar binary.
- Broker to native OS APIs: platform permissions, policy checks, controller lock, approval gates, audit records.

## Primary Assets

- Local screen contents and structured UI text.
- Native mouse/keyboard control.
- Clipboard contents and pasted text.
- Remote stream visibility.
- Computer Use session identity and run context.
- Audit logs and policy decisions.

## Threats And Mitigations

| Threat | Mitigation |
| --- | --- |
| Cloud client bypasses local policy | Remote bridge routes stream/manual commands through the desktop broker; WebRTC answer/ICE payloads are handed to the sidecar only through typed IPC after broker authorization; manual input is normalized into `desktop_control` requests; relay stream/manual command frames require a short-lived token bound to the joined desktop, session, web device, and current run when one is active. |
| Feature rolls out too broadly | Release builds fail closed behind Computer Use desktop rollout flags; per-tool overrides and deterministic rollout percentages gate both descriptor exposure and direct broker calls. |
| Sidecar used as a command runner | IPC schema has no shell/file/plugin operations and rejects shell/script/plugin/file-path payload keys. |
| Stale or stolen sidecar token | Token is minted per sidecar lease, hashed in the lease, expires, and is verified on every request. |
| Wrong sidecar binary launched | Runtime verifies a configured or bundled SHA-256 before treating the sidecar as authenticated. |
| Multiple controllers fight over desktop | Process-wide controller lock permits one active actor and records lease metadata. |
| Stale cloud manual-control session keeps sending input | Manual-control leases expire unless refreshed by the active cloud viewport; brokered input without an active lease is denied. |
| Local user loses control | macOS HID takeover detection makes brokered control back off with `local_user_takeover`; emergency stop releases lock and stream. |
| Sensitive screen/text exposure | Sensitive observation requires approval; high-risk targets are denied; labels/audit summaries are redacted. |
| User-marked private regions leak through fallback frames | Desktop-control settings persist private regions under OS app-data; fallback screenshots apply them before relay and report `redactionsApplied`. |
| Remote stream/manual control lacks local visibility | The desktop app renders a persistent safety banner for active streams and controller locks, with Settings access and emergency stop. |
| Paste action reads prior clipboard contents | The macOS sidecar paste path only writes the supplied text before sending paste input; it does not read or restore previous clipboard contents. |
| Remote user starts viewing/control silently | Cloud viewing and manual control are separate local opt-ins, both off by default. |
| Fallback frames become recordings | Degraded screenshot frames are bounded, relayed ephemerally, and local temporary screenshot files are deleted after read; stream session records store metadata only, not frame bytes. |
| Engineering capabilities leak into Computer Use | Desktop tools are Computer Use-only and do not grant shell, file mutation, MCP, skills, subagents, or process-manager tools. |
| TURN credentials leak beyond one session | Phoenix issues short-lived coturn REST credentials from server-side configuration; the shared TURN secret is never sent to clients. |

## Release Gates

- IPC contract tests pass.
- Desktop tool access tests confirm Computer Use-only exposure.
- Remote bridge tests cover stream/manual command parsing and broker routing.
- Phoenix telemetry reports forwarded/rejected Computer Use stream/manual command counts, bytes, and rejection reasons.
- Local settings keep cloud viewing/control off by default.
- CI builds and tests the sidecar scaffold plus IPC contract on macOS, Windows, and Linux.
- macOS signed builds verify the app bundle with nested executables.
- Security review explicitly accepts any remaining platform gaps before rollout.
- Release rollout flags are configured intentionally for each channel before enabling desktop tools.

## Residual Risks

- Native WebRTC publishing currently uses a sidecar data channel with chunked redacted JPEG frames; a media-track encoder path can be added later for lower bandwidth and browser-native video metrics.
- Windows and Linux sidecars currently share the contract and brokered native input/WebRTC frame backend, but still need Accessibility, OCR, deeper Wayland portal coverage, and permissioned platform integration tests.
- macOS local takeover detection is best-effort and should be expanded with event taps or sidecar-level input monitoring.
- TURN credential issuance is implemented, but coturn deployment, network policy, metrics, and media relay operations need deployment-specific review.
