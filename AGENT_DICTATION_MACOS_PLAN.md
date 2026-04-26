# Agent Dictation on macOS — Phased Implementation Plan

Production-grade speech dictation for the Cadence agent view. The feature lets a user dictate a prompt into the agent composer, review or edit the recognized text, and send it with the existing send flow.

Reader: an engineer implementing this feature in Cadence.

Post-read action: implement native macOS dictation in phases, preferring the macOS 26 SpeechAnalyzer stack and falling back to SFSpeechRecognizer with AVAudioEngine when the modern stack is unavailable.

---

## 1. Goal & Constraints

### Goal
Add a microphone control to the agent composer so users can dictate text instead of typing. Dictation should feel native to macOS, stream partial results into the existing composer, and never auto-send text without the user pressing Send.

### Hard Constraints
1. macOS only for the first release.
2. Use native Apple speech APIs, not a cloud transcription vendor.
3. Prefer the macOS 26 path:
   - `SpeechAnalyzer`
   - `DictationTranscriber`
   - `AssetInventory`
   - `AVAudioEngine`
4. Fall back when the macOS 26 APIs are not available:
   - `SFSpeechRecognizer`
   - `SFSpeechAudioBufferRecognitionRequest`
   - `AVAudioEngine`
5. Use ShadCN UI where possible.
6. Do not add temporary debug or test UI. Any UI must be user-facing.
7. This is a Tauri app; do not plan browser-only validation.
8. Keep one active dictation session per app process.

### Non-Goals
1. No voice commands in this milestone. The user dictates prompt text only.
2. No cross-platform dictation yet.
3. No automatic prompt submission.
4. No persistent audio recording.
5. No third-party speech service.

---

## 2. Architecture Decision

Implement a single Tauri command surface backed by two native macOS engines.

```
React agent composer
  -> useSpeechDictation hook
  -> Cadence desktop adapter
  -> Tauri commands + Channel
  -> Rust dictation state
  -> Swift macOS dictation shim
     -> Modern engine: SpeechAnalyzer + DictationTranscriber
     -> Legacy engine: SFSpeechRecognizer + AVAudioEngine
```

### Why a Swift Shim
The preferred macOS 26 APIs are Swift-first and use Swift concurrency. A small Swift shim can own both the modern and legacy implementations behind the same interface, while Rust owns Tauri IPC, lifecycle state, and event delivery to the frontend.

Use an in-process native shim instead of a helper sidecar so microphone and speech permissions belong to Cadence, not to a separate helper executable.

### Engine Selection
The backend should expose an engine preference, but the default behavior should be automatic:

1. Try the modern engine when:
   - the app is running on macOS 26 or newer,
   - the app was built with an SDK that contains `SpeechAnalyzer` and `DictationTranscriber`,
   - the requested locale is supported or equivalent,
   - required speech assets are installed or can be installed.
2. Fall back to the legacy engine when:
   - runtime macOS is older than 26,
   - the build SDK lacks the modern APIs,
   - modern assets are unavailable or installation fails,
   - the modern engine reports unsupported hardware or locale.
3. Surface a user-facing error only when both engines are unavailable or permission is denied.

---

## 3. Shared Dictation Contract

Create a stable DTO contract before either engine is implemented. Both engines must produce the same events.

### Commands

```ts
speechDictationStatus(): Promise<DictationStatus>
speechDictationStart(request: DictationStartRequest): Promise<DictationStartResponse>
speechDictationStop(): Promise<void>
speechDictationCancel(): Promise<void>
```

### Start Request

```ts
type DictationStartRequest = {
  locale?: string | null
  enginePreference?: 'automatic' | 'modern' | 'legacy'
  privacyMode?: 'on_device_preferred' | 'on_device_required' | 'allow_network'
  contextualPhrases?: string[]
  channel: Channel<DictationEvent>
}
```

### Events

```ts
type DictationEvent =
  | { kind: 'permission'; microphone: PermissionState; speech: PermissionState }
  | { kind: 'started'; sessionId: string; engine: 'modern' | 'legacy'; locale: string }
  | { kind: 'asset_installing'; progress: number | null }
  | { kind: 'partial'; sessionId: string; text: string; sequence: number }
  | { kind: 'final'; sessionId: string; text: string; sequence: number }
  | { kind: 'stopped'; sessionId: string; reason: StopReason }
  | { kind: 'error'; sessionId: string | null; code: string; message: string; retryable: boolean }
```

### Text Semantics
Each `partial` event replaces the active dictated segment. It does not append blindly.

Frontend flow:
1. User starts dictation.
2. Composer captures the current draft as `baseDraft`.
3. Partial text renders as `baseDraft + dictatedText`.
4. Final text commits to the draft.
5. User can edit text before pressing Send.

This prevents duplicated words when native engines revise partial transcripts.

---

## 4. Phase 0 — Native Capability Probe

Purpose: make capability detection truthful before wiring UI.

### Backend Work
1. Add a dictation module under the Tauri command layer.
2. Add managed dictation state with:
   - active session id,
   - active engine,
   - cancellation handle,
   - one-session-at-a-time guard.
3. Add `speech_dictation_status`.
4. Add build-time SDK detection in the Tauri build script:
   - detect the macOS SDK version,
   - compile the modern Swift source only when the SDK exposes the macOS 26 Speech APIs,
   - compile a modern-engine stub otherwise.
5. Add app bundle privacy strings:
   - `NSSpeechRecognitionUsageDescription`
   - `NSMicrophoneUsageDescription`
6. Link native frameworks needed by the shim:
   - Speech
   - AVFoundation or AVFAudio
   - Foundation

### Frontend Work
1. Add zod schemas for status and events.
2. Add desktop adapter methods, but do not show a mic button yet.
3. Add tests that mocked status responses normalize correctly.

### Acceptance Criteria
1. The app can report whether modern dictation, legacy dictation, microphone permission, and speech permission are available.
2. Older macOS SDK builds still compile with the modern engine disabled.
3. No user-facing composer changes yet.

---

## 5. Phase 1 — Swift Shim Skeleton

Purpose: establish the Rust-to-Swift lifecycle with fake-free native plumbing, not a throwaway debug UI.

### Swift Shim Responsibilities
1. Expose C ABI functions for Rust:
   - create session,
   - start,
   - stop,
   - cancel,
   - query capabilities.
2. Accept a callback pointer and opaque context from Rust.
3. Emit JSON event payloads back through the Rust callback.
4. Guarantee callbacks are serialized per session.
5. Ensure all AppKit or AVFoundation work runs on the appropriate queue.

### Rust Responsibilities
1. Wrap the C ABI in a safe Rust facade.
2. Convert native events into `DictationEvent` DTOs.
3. Send events over the Tauri `Channel`.
4. Prevent overlapping sessions.
5. Clean up the native session when:
   - the frontend calls stop,
   - the frontend channel closes,
   - the app window closes,
   - a native error ends recognition.

### Acceptance Criteria
1. Starting a session returns a session id and selected engine.
2. Stopping and cancelling are idempotent.
3. A second start request while one session is active returns a typed user-fixable error.
4. Unit tests cover Rust session state transitions.

---

## 6. Phase 2 — Modern Engine: macOS 26 SpeechAnalyzer

Purpose: implement the preferred dictation engine.

### Engine
Use:
1. `AVAudioEngine` to capture microphone buffers.
2. `DictationTranscriber` for dictation-oriented text.
3. `SpeechAnalyzer` to analyze the live audio sequence.
4. `AssetInventory` to detect and install required locale assets.

### Behavior
1. Select the best locale equivalent to the requested locale or the system locale.
2. Prefer progressive dictation results for responsive partial text.
3. Request punctuation where supported.
4. Use contextual phrases for project-specific terms where the API supports analysis context.
5. Install missing assets only after a user action starts dictation.
6. Emit an `asset_installing` event while models are being installed.
7. If the modern engine cannot start, return a structured fallback reason to Rust.

### Privacy
The modern engine should be treated as the preferred privacy path because it uses on-device dictation assets. If asset installation requires network access, the UI should describe that as installing Apple speech assets, not as streaming the user's dictated audio to a third-party service.

### Fallback Triggers
Fallback to the legacy engine if:
1. macOS is older than 26.
2. Modern symbols are not compiled in.
3. The locale is unsupported.
4. Asset installation fails or is declined.
5. The analyzer cannot obtain a compatible audio format.
6. The engine throws during startup before audio capture begins.

Do not fallback silently after partial text has already been produced. In that case, stop the session and show an error so the user can decide whether to retry.

### Acceptance Criteria
1. On macOS 26 with compatible assets, dictation starts with `engine: 'modern'`.
2. Partial text updates the composer without duplicated fragments.
3. Final text remains editable and is not sent automatically.
4. Modern-engine startup failure can fall back to legacy in automatic mode.

---

## 7. Phase 3 — Legacy Engine: SFSpeechRecognizer + AVAudioEngine

Purpose: support older macOS versions and modern-engine failure cases.

### Engine
Use:
1. `SFSpeechRecognizer`.
2. `SFSpeechAudioBufferRecognitionRequest`.
3. `AVAudioEngine` microphone input.
4. Recognition task result handler for partial and final results.

### Behavior
1. Request speech authorization on first real use.
2. Request microphone authorization on first real use.
3. Set `shouldReportPartialResults = true`.
4. Set `taskHint` to dictation if available.
5. Set contextual strings for short project and app vocabulary.
6. Use `addsPunctuation` on macOS versions that support it.
7. Respect Apple's recognition-duration limit by stopping cleanly before the limit and restarting only when there is active speech input and no final result yet.

### Privacy Modes
1. `on_device_required`: set `requiresOnDeviceRecognition = true`; fail if unsupported.
2. `on_device_preferred`: try on-device first; if unsupported, ask the UI layer to decide whether network recognition is allowed.
3. `allow_network`: allow Apple server-backed recognition when local recognition is unavailable.

The legacy engine should never silently switch from on-device recognition to network recognition.

### Acceptance Criteria
1. On macOS versions older than 26 but new enough for Speech framework support, dictation starts with `engine: 'legacy'`.
2. Permission denial produces a user-fixable error.
3. Network-backed recognition is never used unless explicitly allowed by the request or a user setting.
4. Long-running sessions stop or roll over without crashing the composer.

---

## 8. Phase 4 — Agent Composer UI

Purpose: expose dictation in the actual agent view.

### UI
Add a microphone icon button beside the existing send button in the agent composer.

States:
1. Idle: `Mic` icon, tooltip "Start dictation".
2. Requesting permission: spinner or pulsing state, disabled.
3. Listening: active mic state, tooltip "Stop dictation".
4. Stopping: disabled short-lived state.
5. Error: subtle inline error in the existing composer error area.

Use ShadCN primitives already present in the app:
1. `Button`
2. `Tooltip`
3. Existing `Textarea`
4. Existing composer alert area for actionable failures

### Interaction Rules
1. The mic button is disabled when the prompt input is disabled.
2. Dictation can run while a prompt is being drafted.
3. Sending stops active dictation before submitting the draft.
4. Switching projects or agent sessions cancels dictation.
5. Manual typing during dictation is allowed, but the hook must preserve the user's edits. If text diverges from the dictation base, new partials should append after the user's current draft rather than replacing the whole textarea.

### Accessibility
1. Use explicit aria labels for start and stop.
2. Reflect listening state with `aria-pressed`.
3. Keep keyboard focus in the textarea after dictation starts.
4. Do not rely on color alone to indicate recording.

### Acceptance Criteria
1. The mic control is visible only in the real agent composer.
2. No debug or test-only UI is added.
3. Dictated text appears in the composer and remains editable.
4. Existing typed-send behavior is unchanged.

---

## 9. Phase 5 — Settings, Diagnostics, and Failure Recovery

Purpose: make the feature supportable without cluttering the composer.

### Settings
Add a macOS-only dictation setting in the appropriate settings area:
1. Engine preference:
   - Automatic
   - Prefer macOS 26 Dictation
   - Legacy only
2. Privacy mode:
   - On-device preferred
   - On-device required
   - Allow Apple server recognition
3. Locale:
   - System default
   - Supported locale list from backend

### Diagnostics
Add doctor checks for:
1. macOS version.
2. build SDK modern-engine support.
3. speech permission status.
4. microphone permission status.
5. selected locale support.
6. installed modern speech assets when available.

### Recovery
User-fixable errors should tell the user what to do:
1. open System Settings for microphone permission,
2. open System Settings for speech recognition permission,
3. choose a supported locale,
4. permit network recognition if on-device recognition is unavailable.

### Acceptance Criteria
1. Users can understand why dictation is unavailable.
2. Diagnostics distinguish "modern unavailable, legacy available" from "dictation unavailable".
3. The composer stays compact; detailed troubleshooting lives in settings or diagnostics.

---

## 10. Phase 6 — Verification

Purpose: prove the feature works without relying on browser workflows.

### Unit Tests
1. DTO parsing and event normalization.
2. Engine-selection decision matrix.
3. Rust session state:
   - start,
   - stop,
   - cancel,
   - double-start rejection,
   - channel-close cleanup.
4. Composer hook:
   - partial replacement,
   - final commit,
   - typed text preserved,
   - send stops dictation.

### Component Tests
1. Mic button renders only when dictation is supported.
2. Button labels and disabled states are correct.
3. Composer draft changes from mocked partial and final events.
4. Existing Enter-to-send and send button behavior remain unchanged.

### Native Manual Tests
Run on real macOS, not a browser:
1. macOS 26 with modern engine available.
2. macOS 26 with modern assets missing, then install flow.
3. macOS 26 with modern engine disabled, legacy fallback.
4. macOS older than 26, legacy engine.
5. Microphone permission denied.
6. Speech permission denied.
7. On-device-required mode with unsupported locale.
8. Long dictation beyond the legacy engine's normal recognition window.

### Build Checks
Run one Cargo command at a time:
1. frontend tests,
2. Rust tests,
3. Tauri build or check on macOS.

### Acceptance Criteria
1. The modern path works on supported systems.
2. The fallback path works when the modern path is unavailable.
3. Permission and privacy failures are understandable.
4. No temporary UI remains.

---

## 11. Suggested Slice Order

1. Shared contract, status command, and build-time SDK detection.
2. Swift shim skeleton with Rust session management.
3. Modern engine implementation.
4. Legacy engine implementation.
5. Desktop adapter and `useSpeechDictation` hook.
6. Composer mic UI.
7. Settings and diagnostics.
8. Verification pass and cleanup.

This order keeps the risky native integration isolated before touching the agent UI, then adds the visible experience only after the backend can truthfully report capability and stream transcript events.

---

## 12. Open Decisions

1. Default privacy mode: recommended default is `on_device_preferred`.
2. Whether to expose `allow_network` in initial settings or defer it until a user hits an unsupported on-device case.
3. Whether modern speech asset downloads should be automatic after the user starts dictation or require an explicit confirmation dialog.
4. Whether dictation should preserve one space between the existing draft and dictated text automatically.
5. Whether contextual phrases should include project filenames and dependency names, or only app-level terms like Cadence, Tauri, ShadCN, and provider names.
