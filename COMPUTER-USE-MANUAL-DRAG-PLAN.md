# Computer Use Manual Drag Support Plan

## Reader and Goal

Reader: an internal engineer implementing Computer Use manual-control input.

Post-read action: add and verify click-and-drag support for manual desktop control, so a user can drag windows, select file ranges, and perform ordinary drag gestures from the streamed desktop viewport.

## Audit Conclusion

Manual Computer Use does not currently support true click-and-drag from the human-controlled viewport.

The lower layers already have partial drag capability:

- The desktop control action model includes `mouse_drag`.
- The remote manual-control bridge accepts manual-control input actions and maps `x`, `y`, `toX`, `toY`, `sourceWidth`, and `sourceHeight` into the desktop control request.
- The desktop runtime validates drag source and target points, normalizes active stream coordinates to display coordinates, and maps the action to sidecar drag control when available.
- The native and sidecar input paths can emit a left-button drag sequence.

The manual-control UI path is the gap:

- Pointer down sends `mouse_click` or `mouse_right_click` immediately.
- Pointer move while a button is held sends throttled `mouse_move`.
- Pointer up on non-mobile does not send any button-release or drag action.
- Mobile touch gestures are reserved for tap, pan, and pinch behavior; they do not map touch movement into desktop drag.
- The relay client input type does not currently advertise `toX` or `toY`, even though the payload path can forward them.

Because the remote desktop never receives a held mouse button from the manual viewport, the current behavior cannot drag windows or rubber-band select files. It can only click, move the pointer, scroll, and send keyboard/text input.

## Implementation Strategy

Use the existing `mouse_drag` control action first. Do not introduce stateful `mouse_down` / `mouse_up` protocol actions unless one-shot drag proves unreliable during manual QA.

Implement desktop pointer drag as a gesture recognizer in the manual viewport:

1. On primary-button pointer down in manual mode, capture the pointer and store a pending gesture with pointer id, button, click detail, screen start position, and mapped desktop start point.
2. Do not send `mouse_click` immediately. Wait until pointer up so the gesture can be classified as click or drag.
3. On pointer move for the captured pointer, update the latest mapped point. Mark the gesture as dragging once movement exceeds the existing tap/click slop threshold.
4. On pointer up:
   - If movement stayed within slop, send the existing click or double-click payload.
   - If movement exceeded slop and the button is left, send one `mouse_drag` payload with `x`, `y`, `toX`, `toY`, `sourceWidth`, and `sourceHeight`.
   - If movement exceeded slop for right or middle button, keep the initial implementation conservative and do not synthesize unsupported button-drag behavior.
5. On pointer cancel, lost capture, manual-control release, stream change, or unmount, clear the pending gesture without sending a click.
6. Keep click ripples for click gestures only. Do not add temporary debug UI.

This preserves the backend approval, lease, stream-token, and coordinate-normalization paths already used by manual control.

## Type and Contract Updates

Update the relay client manual-input type so drag is first-class:

- Add `toX` and `toY`.
- Prefer a small string union for known manual actions if it fits local style; otherwise keep `action: string` and extend the shape only.
- Add a relay client test proving `mouse_drag` forwards start and target coordinates plus stream security fields.

Add bridge/runtime coverage:

- Add a bridge unit test for manual `mouse_drag` payload mapping, including `toX` and `toY`.
- Add or extend runtime/sidecar mapping tests so drag target coordinates are preserved into the sidecar request.
- If manual QA shows instant two-point drags are flaky, extend the runtime later with interpolated drag duration or a stateful press/drag/release protocol guarded by the same manual-control lease.

## Frontend Tests

Add focused tests around the manual viewport:

- A simple pointer down/up still sends one `mouse_click`.
- A small move within slop still sends a click.
- A left-button move beyond slop sends one `mouse_drag` on pointer up and does not send the old immediate `mouse_click`.
- The drag payload uses mapped desktop stream coordinates for both source and target, including object-contain letterboxing.
- Pointer cancel sends no click or drag.
- Existing mobile pinch, pan, tap, keyboard capture, scroll, and right-click behavior remain covered.

## Verification

Run scoped checks only:

- `pnpm --dir ./cloud test -- src/routes/-desktop-click-ripple.test.tsx src/lib/relay/relay-client.test.ts`
- `cargo test -p xero-desktop manual_control_drag --lib`
- `cargo test -p xero-desktop-sidecar mouse_drag --tests`

Run Cargo commands one at a time.

Manual QA must be performed in the Tauri app, not by opening the app in a browser:

- Start a Computer Use desktop stream.
- Enter manual control.
- Drag a window by its title bar and confirm it moves.
- Drag across files/icons and confirm multi-select works.
- Confirm normal click, double-click, right-click, scroll, and keyboard passthrough still behave normally.

## Acceptance Criteria

- Manual left-button drag works from the streamed desktop viewport.
- Clicks are not accidentally converted into drags.
- Drag gestures do not emit a premature click at the start point.
- Manual-control lease and approval gates remain unchanged.
- No temporary or test-only UI is added.
- Scoped frontend and Rust tests pass.
