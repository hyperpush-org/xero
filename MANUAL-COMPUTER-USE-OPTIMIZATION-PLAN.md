# Manual Computer Use Optimization Plan

## Reader And Goal

This plan is for engineers improving Xero's manual computer use path. After reading it, an engineer should be able to implement the reliability work in a safe order without needing the original audit conversation.

The product goal is a consistent manual control experience with no silent interruptions, even when the network is degraded, relay connections reconnect, WebRTC fails, or the desktop bridge restarts.

## Target Experience

- Manual control requests either become active or produce a visible denial. They never appear active before the desktop grants control.
- Clicks, key presses, text input, drag gestures, release requests, and heartbeats are either executed once or visibly rejected. They are never silently lost.
- Pointer movement and stream status can be coalesced under pressure, but critical input and signaling cannot be dropped.
- WebRTC remains the preferred stream path, but the system automatically moves through fallback transports when needed.
- Recovery is automatic after temporary network degradation, token expiry, relay reconnects, sidecar failure, and desktop bridge reconnects.
- The user receives a stable visual state during recovery instead of abrupt pauses, stale frames, or misleading "active" control.

## Current Strengths

- WebRTC is already the primary desktop stream path.
- Screenshot fallback, keyframe refresh, stale-frame recovery, and adaptive quality hooks already exist.
- Manual control is protected by desktop-side leases and recurring heartbeats.
- Stream tokens and run identifiers already scope remote commands.
- Drag gesture support exists in the current manual control surface.
- Desktop-side execution already audits manual control acquisition, refresh, input, and release.

## Main Reliability Gaps

### Fire-And-Forget Web Commands

The web relay client sends many commands without waiting for command-level success, failure, or timeout. This affects manual input, manual control requests, heartbeats, WebRTC signaling, stream status, and keyframe requests.

Impact:

- Rate-limit failures can be invisible.
- Relay timeouts can look like accepted input.
- Manual control may enter an active-looking state before the desktop grants the lease.
- The UI cannot distinguish "still sending" from "executed" from "rejected."

### Coarse Server Rate Limiting

The server currently treats many web-originated command families as one relay event class. Manual input, pointer movement, screenshot fallback polling, keyframes, heartbeats, and signaling can compete for the same rate budget.

Impact:

- Degraded streaming can starve manual input.
- Pointer movement can starve lease heartbeats or critical input.
- WebRTC recovery traffic can collide with manual control traffic.
- Network degradation amplifies command loss because retries create more traffic.

### Optimistic Manual State

The UI can move into manual mode immediately after requesting control, before the desktop confirms the matching lease.

Impact:

- The user can interact while control is not actually held.
- Denials and stale leases can feel like broken input.
- Recovery paths are harder to reason about because "manual" means both requested and active.

### Weak Delivery Guarantees In The Desktop Bridge

The desktop bridge has reconnect and replay foundations, but outbound frames can be consumed before a successful relay send. Inbound broadcast lag can also drop commands without a command-aware policy.

Impact:

- A reconnect at the wrong moment can lose events.
- Critical commands and best-effort updates are not treated differently.
- Relay recovery depends too heavily on session cursor behavior.

### Fallback Stream Uses The Control Plane

Screenshot fallback sends encoded image data through the same broad relay command path used by control messages.

Impact:

- Fallback frames are large and expensive.
- Degraded streaming can interfere with manual input.
- The fallback path is especially vulnerable to rate limits and websocket instability.

### Limited Degraded-Network Test Coverage

The current test surface covers many local behaviors, but the manual control path needs explicit loss, delay, reconnect, token expiry, rate-limit, and sidecar-failure scenarios.

Impact:

- Regressions can ship in the exact conditions this feature must tolerate.
- Optimizations may improve happy-path latency while making recovery less predictable.

## Implementation Phases

### Phase 1: Protocol Acknowledgements And Command Envelopes

Goal: make every important command observable from the UI to the desktop and back.

Tasks:

- Add a command envelope with `clientCommandId`, `clientSeq`, `kind`, `priority`, `sentAt`, `dedupeKey`, and optional `expiresAt`.
- Change the web relay client so command pushes return an acknowledgement result instead of fire-and-forget completion.
- Define command outcomes: `accepted`, `executed`, `rejected`, `rate_limited`, `timed_out`, `stale`, and `duplicate`.
- Emit command outcome events back to the sender so the UI can recover even if a push response is missed.
- Add desktop-side dedupe for critical commands by command id.
- Preserve best-effort behavior only for command kinds that are explicitly marked coalescible.

Acceptance criteria:

- Manual acquire, release, heartbeat, click, key, text, drag, WebRTC answer, and ICE commands all have visible ack or rejection paths.
- A server-side rejection is surfaced to the requesting UI.
- Duplicate critical commands are not executed twice.

### Phase 2: Priority Queues And Backpressure

Goal: protect critical control traffic when the network or relay is degraded.

Tasks:

- Introduce command classes:
  - critical reliable: acquire, release, heartbeat, click, key, text, drag, signaling.
  - reliable idempotent: stream start, stream stop, status request, keyframe request.
  - coalesced best-effort: pointer move, quality update, cursor position, repeated status refresh.
- Add a client-side scheduler that prioritizes critical reliable commands.
- Coalesce pointer moves and repeated stream status updates to the newest pending value.
- Add bounded queues with explicit drop policies.
- Add timeout and retry policies by command class.
- Avoid automatic replay of unsafe manual input unless desktop dedupe can prove the command was not executed twice.

Acceptance criteria:

- Pointer movement cannot starve clicks, keys, heartbeats, or release.
- Queue drops are explicit, logged, and limited to coalescible commands.
- Critical commands either complete, retry within policy, or visibly fail.

### Phase 3: Per-Kind Server Rate Limits

Goal: stop unrelated traffic from starving manual control.

Tasks:

- Replace the single broad web command bucket with per-kind token buckets.
- Give critical manual commands a separate burst budget.
- Give pointer movement a higher but coalesced budget.
- Give fallback frame requests, keyframes, status polling, and signaling separate budgets.
- Include structured rate-limit metadata in command rejections.
- Add telemetry for accepted, rejected, delayed, and coalesced command counts by kind.

Acceptance criteria:

- Degraded screenshot polling cannot rate-limit manual click/key/drag commands.
- Pointer movement cannot rate-limit heartbeats.
- WebRTC recovery traffic cannot rate-limit manual release.

### Phase 4: Manual Control State Machine

Goal: make manual control state honest and recoverable.

Tasks:

- Replace the single optimistic manual state with explicit states:
  - `manual_idle`
  - `manual_requesting`
  - `manual_active`
  - `manual_reconnecting`
  - `manual_denied`
  - `manual_releasing`
  - `manual_released`
- Only enter `manual_active` after the desktop confirms the matching manual control id.
- Disable manual input while requesting, reconnecting, denied, or releasing.
- Add heartbeat acknowledgements.
- On heartbeat failure, enter `manual_reconnecting`, attempt lease refresh or reacquire, and show a stable recovery state.
- Release control explicitly when the user exits manual control or the session closes.

Acceptance criteria:

- The UI never reports active manual control before desktop grant.
- Missed heartbeat recovery does not silently drop into a stale active state.
- Manual control denial is visible and actionable.

### Phase 5: Desktop Bridge Delivery Hardening

Goal: keep the bridge reliable across reconnects and local backpressure.

Tasks:

- Requeue outbound frames when relay send fails before acknowledgement.
- Keep critical pending frames until acknowledged or expired.
- Persist critical pending state under OS app-data when required for restart recovery.
- Replace lag-prone broadcast handling with a command-aware queue.
- Never drop critical manual or signaling commands due to local lag.
- Coalesce safe high-frequency events before they enter the bridge.
- Add bridge telemetry for reconnects, replay counts, dropped coalescible events, and command ack latency.

Acceptance criteria:

- Simulated relay disconnect during click/key/drag delivery does not silently lose the command.
- Bridge restart during an active session results in visible recovery or rejection.
- Local queue pressure cannot drop release or heartbeat commands.

### Phase 6: Degraded Streaming Transport Ladder

Goal: keep a useful picture on screen while protecting input reliability.

Tasks:

- Define the transport ladder:
  - WebRTC video stream.
  - WebRTC data-channel still frames.
  - Dedicated fallback image channel or chunked media path.
  - Slow relay stills as last resort.
- Move high-frequency fallback images away from the manual control command budget.
- Add adaptive fallback interval and quality based on ack latency, frame age, payload size, and network errors.
- Keep trying WebRTC recovery with cooldown and ICE restart while fallback is active.
- Queue ICE candidates until the peer connection is ready.
- Add stream token refresh or rejoin before long sessions hit token expiry.

Acceptance criteria:

- WebRTC failure automatically falls back without breaking manual control.
- Fallback streaming cannot starve critical manual commands.
- Long manual sessions recover or refresh before stream authorization expires.

### Phase 7: Native Input Refinement

Goal: make manual input feel consistent once delivery is reliable.

Tasks:

- Preserve click, scroll, keyboard, paste, and drag behavior behind the reliable command path.
- Add interpolation or duration support for native drag execution where platform APIs need smoother motion.
- Add input ack feedback for click, drag, key, and text actions.
- Keep local visual feedback optimistic but reconcile it with command outcome.
- Avoid replaying unsafe input after uncertain transport unless command dedupe confirms safety.

Acceptance criteria:

- Drag works consistently across WebRTC and fallback stream modes.
- Failed input produces visible recovery or rejection.
- Repeated text input respects backpressure and acknowledgement.

### Phase 8: Observability

Goal: make interruptions measurable before and after optimization.

Tasks:

- Track command ack latency by kind.
- Track command timeout, retry, duplicate, and rejection rates.
- Track manual lease acquisition time, heartbeat latency, missed heartbeat count, and reacquire success.
- Track stream mode transitions, frame age, fallback interval, WebRTC disconnect duration, ICE restart count, and recovery time.
- Track bridge reconnect count, replay count, queue depth, and dropped coalescible events.
- Add dashboard thresholds for manual control interruption, stale frame duration, and critical command loss.

Acceptance criteria:

- Engineers can identify whether an interruption came from UI scheduling, server rate limit, relay disconnect, bridge reconnect, desktop execution, WebRTC failure, or token expiry.
- Manual control reliability can be compared before and after rollout.

### Phase 9: Test Matrix

Goal: prove the feature works in the conditions it is designed for.

Tasks:

- Add web tests for command ack handling, timeout handling, manual state transitions, denial handling, and no-input-before-grant.
- Add server channel tests for per-kind rate limits, structured rejections, sender outcome events, and async authorization behavior.
- Add bridge tests for requeue-on-send-failure, reconnect replay, critical queue preservation, and coalescible drops.
- Add integration tests for:
  - websocket disconnect during click.
  - websocket disconnect during drag.
  - delayed manual grant.
  - denied manual grant.
  - missed heartbeat and reacquire.
  - WebRTC failure with fallback stream.
  - fallback stream under low bandwidth.
  - token expiry during long manual session.
  - sidecar failure during active manual control.
- Add a degraded-network harness that can simulate latency, jitter, packet loss, reconnects, and relay rate limits.

Acceptance criteria:

- Critical manual commands are executed once or visibly rejected under simulated degradation.
- Manual control stays recoverable during stream fallback.
- Tests fail if rate limits or reconnects silently drop critical input.

## Rollout Plan

1. Add observability behind existing behavior.
2. Add command envelopes and acknowledgements behind a feature flag.
3. Enable per-kind rate limits in shadow mode and compare with existing behavior.
4. Enable the manual state machine for internal testing.
5. Enable bridge delivery hardening.
6. Move degraded fallback streaming off the manual control command budget.
7. Run the degraded-network test matrix.
8. Roll out to all users once interruption metrics improve and critical command loss is zero in test scenarios.

## Success Metrics

- Critical manual command loss: zero in automated degraded-network tests.
- Duplicate critical manual execution: zero in automated degraded-network tests.
- Manual acquire success or visible denial: 100 percent of attempts.
- Manual heartbeat recovery: successful reacquire or visible release after simulated transient disconnect.
- Stale frame duration: bounded and visible during degraded streaming.
- WebRTC recovery: automatic fallback and retry without breaking manual control.
- User-facing interruption: measurable decrease after rollout.

## Non-Goals

- Backwards compatibility with legacy repo-local state.
- Adding temporary debug UI.
- Replacing the whole remote desktop stack before command reliability is fixed.
- Optimizing visual frame rate at the expense of manual input reliability.
- Hiding degraded network state from the user when control is not truly active.

## Recommended First Slice

Start with command acknowledgements and manual state correctness.

Deliverables:

- Command envelope type shared by web, server, and desktop bridge.
- Web relay client method that returns structured ack results.
- Server sender outcome event for rejected or rate-limited commands.
- Manual request flow that enters `manual_requesting` first and only enters `manual_active` after desktop grant.
- Tests for grant, denial, timeout, and no-input-before-grant.

This slice gives the team a reliable signal for every later optimization. Once command outcomes are visible, rate limits, bridge reconnects, fallback stream pressure, and WebRTC recovery can be optimized without guessing where interruptions come from.
