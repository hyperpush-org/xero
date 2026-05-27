# Computer Use Desktop Video Streaming Plan

## Reader And Goal

This plan is for engineers replacing the current desktop frame stream with production-grade video. After reading it, an engineer should be able to implement a native WebRTC media-track publisher for Computer Use desktop viewing and know which interim approaches are intentionally not the target architecture.

Redaction is deliberately out of scope for this plan. The first production-grade streaming slice should optimize capture, encode, transport, and playback quality without private-region processing.

## Decision

Use a real WebRTC video media track from the desktop sidecar to the cloud client.

The current data-channel frame stream should become a compatibility fallback only. It is useful for bring-up and degraded environments, but it should not be the main path because it turns video into screenshot transport: repeated image capture, image compression, base64 expansion, JSON chunking, and browser image replacement. That design cannot reliably deliver smooth low-latency desktop video.

FFmpeg can be useful as a development probe or optional diagnostic backend, but it should not be the primary production runtime. A production desktop app needs predictable packaging, permission handling, display selection, lifecycle control, hardware acceleration, and telemetry across macOS, Windows, and Linux. Native capture and platform encoder APIs give tighter control over those requirements than a long-running FFmpeg process.

## Target Architecture

The cloud client remains the WebRTC answerer. It creates an answer, returns ICE candidates through the existing relay command path, and renders the incoming video with the browser's normal media pipeline.

The desktop sidecar becomes the WebRTC offerer and publisher. On stream start it creates a peer connection, adds one video track, captures frames from the selected display, encodes them with a hardware-backed codec when available, packetizes them into RTP, and lets WebRTC handle congestion control, pacing, retransmission, and jitter buffering.

The desktop broker remains the policy and lifecycle boundary. It validates stream commands, starts and stops the sidecar stream, stores stream state, and records lifecycle/audit metadata. It should not inspect or transform frame bytes in the media-track path.

## Platform Capture And Encode

macOS should use ScreenCaptureKit for display/window capture and VideoToolbox for hardware H.264. ScreenCaptureKit is the most appropriate capture API for modern macOS desktop streaming, and VideoToolbox avoids software JPEG-style CPU burn.

Windows should use Windows Graphics Capture or Desktop Duplication for capture and Media Foundation for hardware H.264. Windows Graphics Capture is the better user-facing API where available; Desktop Duplication can remain available for environments where it is more reliable.

Linux should use the xdg-desktop-portal/PipeWire path for Wayland-compatible capture and a GStreamer or FFmpeg library-backed encoder path only where native VAAPI integration is not yet practical. X11 capture can be a degraded backend, not the main Linux design point.

The sidecar should expose one stable stream contract while selecting platform publishers internally. The cloud client should not care which capture backend produced the media.

## Codec Strategy

Start with H.264 because browser decode support, hardware encoder support, and operational familiarity are strongest. Prefer constrained baseline or main profile settings that are broadly decodable. Add VP8 only if a target platform cannot provide a reliable H.264 path without unacceptable packaging cost.

Use dynamic bitrate and frame-rate targets by quality tier:

- Low: 960px wide, 15fps, low bitrate for constrained networks.
- Balanced: 1280px wide, 24fps, default for normal interactive use.
- High: 1920px wide, 30fps, higher bitrate with hardware encoding required.

The publisher should respond to WebRTC sender statistics and congestion feedback instead of treating the configured quality tier as a fixed promise. If the network cannot sustain the tier, lower bitrate before dropping the connection.

## WebRTC Integration

The sidecar should stop using the desktop stream data channel as the normal frame path. It should create a video track, attach a sender, and publish encoded samples through the WebRTC stack. Data channels can remain for control-plane messages such as keyframe requests, diagnostics, or temporary fallback frames during migration.

The cloud client should prefer `ontrack` media playback and keep the image fallback path only for explicit degraded status. The UI should surface stream state from WebRTC connection state and stream status messages, not from the arrival of image blobs.

ICE, TURN credentials, and stream tokens should stay on the existing relay path. Signaling shape does not need a product-facing redesign; the payloads need to carry media-track capabilities and backend diagnostics so the cloud can distinguish native video from fallback frames.

## Lifecycle

Starting a stream should allocate capture, encoder, WebRTC peer connection, and telemetry resources as one lease. Stopping a stream, closing the peer connection, losing the relay session, or receiving emergency stop should tear down all four.

Changing quality should update capture scaling, encoder bitrate, and sender parameters without requiring a full reconnect when possible. If a platform backend cannot update in place, it may restart the publisher behind the same stream state with an explicit transient reconnect status.

Requesting a keyframe should map to the platform encoder's keyframe mechanism and WebRTC sender behavior, not to screenshot refresh.

## Observability

The implementation should emit local and cloud-visible metrics for:

- capture frame rate and dropped capture frames
- encode frame rate, encode latency, and hardware/software encoder selection
- outbound bitrate, packet loss, RTT, retransmits, and keyframe count
- cloud playback dimensions, decoded frame rate, freezes, and connection state
- stream start time, stop reason, backend selection, and fallback reason

These metrics should be available before removing the data-channel frame path as the default. Smooth video without telemetry is too hard to debug in production.

## Migration Plan

1. Add media-track capability fields to the stream capabilities and status payloads while leaving the existing fallback contract intact.
2. Implement the macOS ScreenCaptureKit and VideoToolbox publisher first, because it gives the fastest path to proving the target architecture with hardware capture and encode.
3. Wire the publisher into the sidecar WebRTC peer connection as a real video track and make the cloud client prefer `ontrack` playback.
4. Add stream statistics collection on both sides and expose degraded reasons when native video cannot start.
5. Implement Windows capture and encoding using native APIs under the same sidecar contract.
6. Implement Linux PipeWire capture and the least fragile hardware/software encoder path available for supported distributions.
7. Keep the data-channel JPEG stream as an explicit fallback until all supported platforms have native video coverage and telemetry shows the fallback is no longer needed as the default.

## Acceptance Criteria

- Balanced quality delivers a stable 24fps target at 1280px width on a normal local network with browser-native video playback.
- High quality can deliver 30fps at 1920px width when hardware encoding is available.
- The cloud client receives a WebRTC media track for the normal stream path.
- Closing or stopping a stream releases capture, encoder, peer connection, and telemetry resources.
- Stream quality changes apply without a full reconnect on at least the first production platform.
- Fallback image streaming is visible as degraded mode, not labeled as the normal WebRTC video path.
- Metrics identify whether bottlenecks are capture, encode, network, or browser decode.

## Non-Goals

- Redaction or private-region processing.
- Recording streams.
- Browser `getDisplayMedia` capture, because the browser is not running on the controlled desktop.
- Making FFmpeg a required runtime dependency for the primary production path.
