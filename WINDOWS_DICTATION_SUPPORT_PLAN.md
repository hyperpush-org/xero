# Windows Dictation Support Plan

## Reader And Outcome

This plan is for an internal engineer adding native Windows dictation to the desktop app. After reading it, they should be able to implement the first Windows engine without changing the composer contract or adding paid third-party transcription dependencies.

## Goal

Add Windows desktop dictation in the most free and native way available:

- Use Windows built-in speech APIs first.
- Keep the current dictation command, settings, event, and composer contracts intact.
- Preserve the animated composer waveform by continuing to emit normalized audio level events.
- Avoid any app-owned paid speech service, API key, subscription, or third-party hosted transcription dependency.
- Make the implementation a platform extension, not a second dictation feature.

## Current State

The app already has a cross-platform dictation surface:

- The composer consumes a small dictation controller contract.
- The desktop adapter starts one dictation session and receives streamed events over a Tauri channel.
- Native macOS dictation emits started, partial, final, stopped, error, permission, asset-installing, and audio-level events.
- Windows and Linux are currently identified as their own platforms, but no native desktop engine exists there yet.
- The browser/cloud composer can use Web Speech where the browser supports it; that is separate from desktop native dictation.

The Windows work should fit into the existing native dictation session shape instead of creating new UI behavior.

## Recommended API Strategy

### Phase 1: Windows SDK Speech Recognition

Use `Windows.Media.SpeechRecognition` through Rust's Windows bindings.

Why this is the right first implementation:

- It is the native Windows speech API.
- It is free to call from a Windows app.
- Microsoft documents it as available on Windows 10 and later.
- It supports continuous recognition sessions for dictation-style input.
- It avoids bundling large models or requiring a separate runtime install.

Expected constraints:

- The predefined dictation grammar may use Microsoft's online speech service and requires the user's Windows speech privacy settings to allow it.
- The app must handle microphone denial, missing microphone devices, disabled online speech recognition, unsupported language, and recognition quality degradation as first-class statuses.
- The API returns recognition text, but it does not replace the need for a separate audio-level meter if the composer waveform should remain accurate.

### Phase 2: Optional Foundry Local Whisper Engine

After the native Windows SDK engine is stable, consider an optional local Whisper engine through Microsoft Foundry Local.

Why it should not be Phase 1:

- Foundry Local is still preview.
- It may require a separate install, first-run downloads, and device-specific acceleration packages.
- It is better positioned as an opt-in "local/offline transcription" engine than the default Windows support path.

Why it is still worth tracking:

- It can keep speech on-device after model download.
- It is free for local use and does not require an Azure subscription for local-only scenarios.
- It could become the best privacy mode if the Windows SDK dictation grammar requires online speech recognition.

## Architecture

Add a Windows native dictation backend behind the same session abstraction used by macOS:

1. Capability probe

   Report Windows as supported only when the Windows speech runtime can be initialized and the system has a usable input path. Status should distinguish:

   - Platform supported, engine unavailable.
   - Microphone missing.
   - Microphone permission denied.
   - Speech privacy disabled.
   - Locale unsupported.
   - Engine ready.

2. Session creation

   Create one Windows native session per desktop dictation start request. The session owns:

   - Session id.
   - Locale.
   - Windows recognizer instance.
   - Continuous recognition handlers.
   - Audio meter handle.
   - Cancellation state.
   - Tauri channel sender.

3. Recognition flow

   The Windows engine should emit:

   - `started` when recognition has started.
   - `partial` from hypothesis fragments.
   - `final` from completed recognition results.
   - `audio_level` from the parallel microphone meter.
   - `stopped` when the continuous session ends normally.
   - `error` for permissions, privacy settings, unsupported language, recognizer failures, and channel failures.

4. Stop and cancel

   Preserve the existing semantics:

   - Stop ends recognition and flushes pending final results if Windows provides them.
   - Cancel ends recognition and discards pending results where possible.
   - Window close and channel failure release the native session and stop microphone metering.

## Audio Metering

The Windows speech recognizer should not be treated as the waveform source. Build a small parallel meter so the composer can use the same `audio_level` event already used by macOS.

Preferred first implementation:

- Use a free Rust audio input crate that uses WASAPI on Windows.
- Capture shared-mode microphone samples while speech recognition is running.
- Compute RMS over small buffers.
- Convert RMS to a clamped 0.0-1.0 level with the same decibel-style normalization used by macOS.
- Emit `audio_level` at animation-friendly cadence, throttled enough to avoid IPC spam.

Fallback if shared capture conflicts with the recognizer:

- Keep dictation functional.
- Emit `audio_level` as 0.0 while recording.
- Surface a diagnostic note that Windows recognition is active but metering is unavailable.

Do not block Windows dictation on perfect waveform support if recognition is otherwise working.

## Locale And Settings

Reuse the existing dictation settings shape.

Windows locale behavior should be:

- If the user has selected a locale, attempt to create the recognizer for that locale.
- If no locale is selected, use the system speech language.
- If the selected locale is unsupported, report a user-fixable error and leave the session idle.
- Do not add migrations or compatibility glue unless the settings schema genuinely changes.

Engine preferences should map as:

- `automatic`: choose the Windows SDK engine.
- `legacy`: reject on Windows unless a Windows-specific meaning is intentionally added later.
- `modern`: reject on Windows until Foundry Local or another Windows engine is implemented and explicitly mapped.

This avoids pretending macOS engine names have Windows semantics.

## Implementation Slices

### Slice 1: Contract And Capability Probe

- Add a Windows SDK engine status to the internal capability probe.
- Keep the public engine enum stable unless adding a real Windows engine enum is unavoidable.
- Update diagnostics so Windows says what is missing instead of saying dictation is macOS-only.
- Add Rust unit tests for Windows platform status using cfg-gated assertions or pure helper functions.

Acceptance:

- Windows status can report "engine ready" or a precise unavailable reason.
- Non-Windows behavior is unchanged.

### Slice 2: Windows Recognizer Spike

- Add a small Windows-only recognizer module.
- Instantiate `Windows.Media.SpeechRecognition.SpeechRecognizer`.
- Compile default dictation constraints.
- Start a continuous recognition session.
- Wire hypothesis and final-result events into the existing native event format.
- Build this behind `cfg(target_os = "windows")`.

Acceptance:

- A manual Windows run can start dictation and receive at least final text.
- Unsupported privacy or microphone states return user-fixable errors.

### Slice 3: Session Lifecycle Integration

- Store the Windows session handle in the existing active-session guard.
- Implement stop, cancel, release, and channel-failure cleanup.
- Ensure only one dictation session can run at a time.
- Ensure session id mismatches are rejected before reaching the UI.

Acceptance:

- Start, stop, cancel, and close-window paths are idempotent.
- The existing desktop adapter and composer do not need Windows-specific branches.

### Slice 4: Audio Level Meter

- Add a Windows audio meter running only while dictation is active.
- Emit normalized audio-level events through the same channel as speech events.
- Throttle metering to a reasonable UI cadence.
- Stop all microphone tracks/streams when dictation stops or errors.

Acceptance:

- The composer waveform moves with live microphone input during Windows dictation.
- Dictation still works if the meter fails.

### Slice 5: Settings, Diagnostics, And Tests

- Update settings labels only if the UI currently implies macOS-only behavior.
- Update diagnostics copy to describe Windows-specific permission/privacy remediation.
- Add adapter contract tests for Windows status and audio-level events.
- Add native unit tests for event conversion and session cleanup.
- Add platform-matrix instructions for the Windows manual smoke.

Acceptance:

- The Windows support path is discoverable from diagnostics.
- The focused dictation test set passes on macOS and Windows.

## Manual Windows Smoke Test

Run this on a Windows 10 or Windows 11 machine:

1. Confirm a microphone is connected.
2. Confirm microphone permission is enabled for desktop apps.
3. Confirm Windows speech recognition privacy settings allow dictation if the SDK path requires it.
4. Start the desktop app.
5. Open the agent composer.
6. Press `Ctrl+Shift+D`.
7. Speak a short phrase.
8. Confirm the composer waveform moves while speaking.
9. Confirm partial or final text appears in the composer.
10. Press `Ctrl+Shift+D` again.
11. Confirm dictation stops and no microphone activity continues.
12. Run diagnostics and confirm Windows dictation reports an accurate status.

## Risks And Decisions

- Online speech recognition may be required for the Windows SDK dictation grammar. This is still free, but it is not fully local. The UI and diagnostics should be honest about that.
- Foundry Local may eventually be a better privacy engine, but its preview status and install/model requirements make it a poor first default.
- Audio metering can fail independently of recognition. The implementation should degrade gracefully.
- Windows language availability depends on installed system speech languages and Microsoft service support.
- Do not rename current macOS "modern" and "legacy" engines as part of this work. Add Windows-specific engine meaning only when there are two real Windows engines.

## Definition Of Done

- Windows can start and stop native desktop dictation from the shared composer.
- `Ctrl+Shift+D` works without a Windows-specific composer component.
- Dictated text streams through the same event contract used by macOS.
- The waveform uses real microphone levels while dictation runs, or diagnostics clearly explain why metering is unavailable.
- Settings and diagnostics no longer describe native dictation as macOS-only on Windows.
- No app-owned paid service, API key, subscription, or third-party hosted model is required.
- Existing macOS dictation behavior remains unchanged.

## References

- Microsoft documents Windows speech recognition options as Windows SDK Speech Recognition and Whisper through Foundry Local: https://learn.microsoft.com/en-us/windows/ai/apis/speech-recognition
- Windows SDK speech recognition uses `Windows.Media.SpeechRecognition` and supports predefined dictation grammars: https://learn.microsoft.com/en-us/windows/uwp/ui-input/speech-recognition
- Continuous recognition sessions are available through `SpeechContinuousRecognitionSession`: https://learn.microsoft.com/en-us/uwp/api/windows.media.speechrecognition.speechcontinuousrecognitionsession
- `SpeechRecognizer` exposes continuous sessions, hypotheses, recognition quality events, and supported languages: https://learn.microsoft.com/en-us/uwp/api/windows.media.speechrecognition.speechrecognizer
- Foundry Local can run audio transcription locally after setup/model download, but is preview: https://learn.microsoft.com/en-us/azure/ai-foundry/foundry-local/get-started
