# Computer Use Manual Keyboard Passthrough Plan

## Reader And Outcome

This plan is for engineers expanding Computer Use manual control in the cloud app. After reading it, an engineer should be able to implement reliable keyboard passthrough so a remote viewer can click an input field in the streamed desktop, type on the viewer machine keyboard, and have the paired desktop receive that input through the existing brokered manual-control path.

## Current Contract Review

Manual cloud control already has the correct high-level safety shape:

- The cloud viewport has a Manual mode with a per-client manual-control lease.
- Manual-control requests, heartbeats, input, and release frames are sent through the same relay command family as desktop streaming.
- The desktop remote bridge accepts only paired web-device commands for Computer Use sessions, checks the local manual-control opt-in, and routes manual input through the desktop broker.
- The desktop broker requires an active manual-control lease, shares the same controller lock as agent input, applies policy and approval gates, writes audit records, and then dispatches normal `desktop_control` requests.
- The sidecar and broker contract already include `key_press`, `hotkey`, `type_text`, and `paste_text`, alongside pointer and scroll actions.

The current cloud implementation is therefore not missing a new backend command family. The gap is that keyboard passthrough needs to become an explicit, tested, user-facing capture model instead of relying on incidental focus behavior.

## Problem Statement

The desired interaction is:

1. The viewer opens a Computer Use desktop stream in the cloud app.
2. The viewer enables Manual control.
3. The viewer clicks an input area in the streamed host desktop.
4. The host desktop receives the mouse click and focuses the target app/control.
5. Subsequent physical keyboard input on the viewer machine is captured by the cloud viewport and forwarded to the focused host app.

The risky part is step 5. A browser only sends keyboard events to the focused web element, and the current pointer handler prevents default behavior while sending the remote click. That can make viewport focus fragile. The existing tests cover click coordinate mapping, but they do not prove that typing after a manual click is captured, normalized, relayed, authorized, and dispatched.

## Goals

- Make keyboard capture deterministic after a manual click inside the desktop viewport.
- Preserve the existing relay and broker safety model; no raw sidecar messages from the cloud app.
- Support ordinary text entry, editing/navigation keys, modifier shortcuts, and explicit paste events.
- Handle IME/composed text where the browser exposes it.
- Keep toolbar, composer, and dialog controls usable without leaking their keyboard input to the host desktop.
- Avoid persisting raw typed text in client state, relay logs, telemetry, or audit summaries.
- Add focused unit and contract tests instead of temporary development UI.

## Non-Goals

- Do not add a new remote-shell, script, file, MCP, or process-management capability.
- Do not support keys the browser or viewer operating system never exposes, such as OS-level app switching shortcuts.
- Do not make Escape a local release shortcut by default; host apps need Escape passthrough.
- Do not add compatibility glue for legacy repo-local state.
- Do not record streamed desktop video or typed text.

## Target Behavior

When Manual mode is active, a click on the desktop media surface should both send the remote mouse action and arm keyboard capture for that manual-control lease. While capture is armed:

- Printable text is sent as `type_text`.
- Enter, Tab, Backspace, Delete, arrow keys, Escape, Page Up/Down, Home/End, and function keys are sent as `key_press`.
- Modifier combinations are sent as `hotkey` with normalized modifier names.
- Browser paste events in the viewport are sent as `paste_text` only from the explicit paste event payload.
- Clicking toolbar controls, the composer, dialogs, or any non-viewport UI must not forward those keys to the host.
- Clicking outside the viewport disarms capture but does not release the manual-control lease.
- Releasing Manual mode or stopping the stream clears capture immediately.

Browser and OS limitations must be visible in tests and docs: some shortcuts may remain reserved by the viewer browser or operating system and cannot be passed through.

## Design

### 1. Cloud Capture Model

Replace incidental keyboard focus with an explicit keyboard capture owner inside the desktop viewport.

Implementation shape:

- Add a small internal capture state: inactive, armed, composing.
- On Manual request/grant, keep capture inactive until the viewer interacts with the desktop media surface.
- On pointer down inside the media content rect, send the existing pointer action and focus the viewport's keyboard sink with `preventScroll`.
- Use a functional hidden keyboard sink or the existing focusable desktop surface. It must be part of the real interaction model, not debug UI.
- Keep capture attached to the current `manualControlId`; if the lease changes, reset capture.
- Stop event propagation from toolbar controls so toolbar shortcuts remain local to the cloud UI.
- Do not steal focus from the prompt composer, select menus, dialogs, or settings controls.

The first implementation task should verify whether the current `preventDefault` on pointer down prevents browser focus. If so, focus the keyboard sink explicitly before returning from the pointer handler.

### 2. Text And Key Normalization

Use two lanes:

- Text lane: `beforeinput`, composition end, and paste events produce text payloads.
- Key lane: `keydown` produces navigation/editing keys and modifier shortcuts.

Rules:

- Prefer `beforeinput.data` or composition output for printable text so IME and dead-key input work when available.
- Use a short fallback path from `keydown` for printable characters only when no text event follows.
- Batch fast printable text into small `type_text` chunks with a short debounce, but flush before any non-text key.
- Normalize DOM keys to broker key names before sending.
- Normalize modifiers to `command`, `control`, `option`, and `shift`.
- Deduplicate modifier-only repeats and ignore auto-repeat for text when the text lane already covers it.
- Include a monotonically increasing input sequence in the cloud payload if ordering diagnostics are needed.

Candidate payloads:

```json
{
  "action": "type_text",
  "text": "hello"
}
```

```json
{
  "action": "key_press",
  "key": "Enter"
}
```

```json
{
  "action": "hotkey",
  "keys": ["command", "a"]
}
```

```json
{
  "action": "paste_text",
  "text": "pasted text"
}
```

The existing `computer_use_manual_control_input` frame can carry these actions. A new command kind is not required.

### 3. Host Platform Semantics

The first release should preserve literal passthrough semantics: send the key or modifier the browser reports, then let the host broker and sidecar map it to the target platform. That matches the current desktop-control contract and avoids hidden cross-platform surprises.

After the basic path is reliable, consider a user-facing shortcut compatibility option for common cross-OS shortcuts. For example, a macOS viewer pressing Command+C while controlling a Windows host may expect Control+C. That should be a deliberate product choice, not accidental remapping.

### 4. Relay And Broker Contract

Keep cloud input normalized before it enters the relay:

- Cloud sends only manual-control command frames.
- The relay forwards the payload with the existing stream token/run binding.
- The desktop remote bridge converts payloads to normal desktop-control requests.
- The desktop broker remains responsible for lease checks, local opt-in, controller lock, policy, approval, audit, and sidecar dispatch.

Add explicit contract coverage for keyboard payloads:

- `type_text` maps text into a desktop-control request.
- `key_press` maps key into a desktop-control request.
- `hotkey` maps key arrays into a desktop-control request.
- `paste_text` maps text into a desktop-control request and remains policy-gated.
- Unknown actions remain rejected.
- Keyboard input without an active matching manual-control lease remains rejected.

### 5. Safety And Privacy

Keyboard passthrough increases sensitivity because typed text may contain secrets.

Required safeguards:

- Manual cloud control remains a separate local opt-in.
- Input is accepted only for paired web devices on Computer Use sessions.
- Text payloads are not written to persistent cloud state.
- Audit records store action type, status, policy decision, target context when known, and redacted summaries only.
- Telemetry never includes raw text.
- Existing policy denial for password managers, payment flows, wallet/financial contexts, identity verification, MFA/recovery, and system security settings applies equally to remote keyboard input.
- Paste passthrough uses only the explicit browser paste event and must not poll the viewer clipboard.
- Size-limit text and paste payloads to prevent accidental large clipboard transfer.

### 6. User-Facing UX

Use existing ShadCN primitives where UI changes are needed.

Suggested user-facing additions:

- When Manual mode is active and capture is armed, show a concise "Keyboard captured" state in the desktop controls toolbar.
- When Manual mode is active but capture is not armed, the Manual button state can remain active and the viewport can use focus styling to indicate the desktop surface is ready.
- If a key cannot be sent because capture is inactive, do not show noisy toasts; the user can click the streamed desktop to arm capture.
- If the host rejects keyboard input for permission or policy reasons, surface the existing broker error in the desktop controls status path.

Do not add temporary debug controls or test-only UI.

## Implementation Phases

### Phase 1: Cloud Capture Reliability

- Add explicit keyboard capture state to the cloud desktop viewport.
- Focus the viewport keyboard sink after valid manual pointer input.
- Disarm capture on outside click, manual release, stream stop, session change, and lease change.
- Ensure toolbar, prompt composer, dialogs, and menus stop keyboard events before they reach the viewport.
- Add unit tests proving that typing after a manual click sends keyboard payloads.

### Phase 2: Text, Composition, And Paste

- Add `beforeinput`/composition handling for text.
- Keep `keydown` for special keys and hotkeys.
- Add a small text batching/flushing helper.
- Add paste-event support with payload size limits.
- Add tests for plain text, shifted characters, IME/composition output, Enter, Backspace, Tab, arrows, Escape, paste, and modifier shortcuts.

### Phase 3: Relay And Broker Parity

- Add relay-client tests for keyboard manual-control payloads with run ID and stream token.
- Add remote bridge tests mapping keyboard actions to desktop-control requests.
- Add broker tests proving keyboard input requires a matching manual-control lease.
- Add sidecar mapper tests for browser-normalized key names that are likely to arrive from the cloud UI.

### Phase 4: Host Dispatch Verification

- Verify macOS `key_press`, `hotkey`, `type_text`, and `paste_text` through the sidecar path and broker fallback where applicable.
- Verify Windows/Linux keyboard dispatch through the cross-platform input backend.
- Verify permission-denied and unsupported-key errors are returned as user-fixable where appropriate.
- Keep Cargo commands scoped and run only one Cargo command at a time.

### Phase 5: UX And Documentation

- Add the minimal user-facing capture state if tests show users need feedback.
- Update the Computer Use desktop control docs to describe keyboard passthrough behavior, limitations, and privacy handling.
- Keep terminology consistent: per-run gated phases are "Stages"; do not label them as workflow phases in user-facing copy.

## Test Plan

Cloud tests:

- Manual click focuses/arms keyboard capture and still sends the pointer payload.
- Plain key entry sends `type_text`.
- Enter, Tab, Backspace, Delete, Escape, arrows, Home/End, Page Up/Down, and function keys send `key_press`.
- Modifier combinations send `hotkey`.
- Composition output sends one text payload and does not double-send from `keydown`.
- Paste sends `paste_text` from the paste event and never reads clipboard outside that event.
- Toolbar/composer/dialog key events are not forwarded.
- Manual release and stream stop disarm capture.

Relay and desktop tests:

- Relay client preserves keyboard payload fields and stream security fields.
- Remote bridge maps keyboard payloads into desktop-control requests.
- Unknown actions remain rejected.
- Manual-control keyboard input requires the matching lease.
- Keyboard actions continue to use policy, approval, audit, and controller-lock paths.
- Sidecar key mappers accept the cloud-normalized key names.

Manual verification:

- In the cloud app, start a Computer Use stream, enable Manual, click a visible host text input, type text, edit with Backspace/arrows, press Enter, and use a common shortcut.
- Verify the host receives the input, the cloud composer does not receive it, and releasing Manual stops passthrough.
- Verify permission and policy denial states appear through the existing desktop-control status path.

## Acceptance Criteria

- A viewer can click a host input field in the streamed desktop and type normal text from the viewer keyboard into that field.
- The same session can send editing keys and modifier shortcuts through Manual mode.
- Keyboard input is ignored unless Manual mode has an active matching lease and viewport capture is armed.
- Cloud UI controls remain locally operable while Manual mode is active.
- Raw typed text is not persisted in audit, telemetry, or local/cloud UI state.
- Unit and contract tests cover mouse-plus-keyboard passthrough from cloud capture through desktop request mapping.
