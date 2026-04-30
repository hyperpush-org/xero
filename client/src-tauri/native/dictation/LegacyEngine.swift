import AVFoundation
import Dispatch
import Foundation
import Speech

private struct XeroLegacyRecognitionFailure {
    let domain: String
    let code: Int
    let message: String
}

private struct XeroLegacyRecognitionUpdate {
    let text: String?
    let isFinal: Bool
    let failure: XeroLegacyRecognitionFailure?
}

private struct XeroLegacyDictationError: Error {
    let code: String
    let message: String
    let retryable: Bool
}

final class XeroLegacyDictationEngine {
    private enum LifecycleState {
        case created
        case started
        case stopping
        case stopped
    }

    private static let recognitionWindow: DispatchTimeInterval = .seconds(55)
    private static let rolloverActivityGraceNanoseconds: UInt64 = 5_000_000_000

    private let sessionId: String
    private let localeIdentifier: String
    private let privacyMode: String
    private let contextualPhrases: [String]
    private let emitPayload: ([String: Any]) -> Void
    private let controlQueue: DispatchQueue
    private let controlQueueKey = DispatchSpecificKey<Void>()
    private let resultQueue = OperationQueue()
    private let audioEngine = AVAudioEngine()

    private var recognizer: SFSpeechRecognizer?
    private var recognitionRequest: SFSpeechAudioBufferRecognitionRequest?
    private var recognitionTask: SFSpeechRecognitionTask?
    private var durationTimer: DispatchSourceTimer?
    private var state: LifecycleState = .created
    private var generation: UInt64 = 0
    private var sequence: UInt64 = 0
    private var audioTapInstalled = false
    private var requiresOnDeviceRecognition = false
    private var finalizedTranscript = ""
    private var lastTaskTranscript = ""
    private var hasDetectedSpeech = false
    private var hasFinalResult = false
    private var lastSpeechActivityUptime: UInt64?
    private var emittedText = false
    private var terminalEventEmitted = false

    init(
        sessionId: String,
        localeIdentifier: String,
        privacyMode: String,
        contextualPhrases: [String],
        emit: @escaping ([String: Any]) -> Void
    ) {
        self.sessionId = sessionId
        self.localeIdentifier = localeIdentifier
        self.privacyMode = privacyMode
        self.contextualPhrases = contextualPhrases
        self.emitPayload = emit
        self.controlQueue = DispatchQueue(label: "dev.xero.dictation.legacy.\(sessionId)")
        self.controlQueue.setSpecific(key: controlQueueKey, value: ())
        self.resultQueue.name = "dev.xero.dictation.legacy.\(sessionId).results"
        self.resultQueue.maxConcurrentOperationCount = 1
        self.resultQueue.qualityOfService = .userInitiated
    }

    deinit {
        forceCleanup()
    }

    func start() -> XeroDictationOperationResponse {
        performSync {
            startOnQueue()
        }
    }

    func stop(reason: String) -> XeroDictationOperationResponse {
        performSync {
            stopOnQueue(cancelled: false, reason: reason)
        }
    }

    func cancel() -> XeroDictationOperationResponse {
        performSync {
            stopOnQueue(cancelled: true, reason: "cancelled")
        }
    }

    private func performSync<T>(_ body: () -> T) -> T {
        if DispatchQueue.getSpecific(key: controlQueueKey) != nil {
            return body()
        }

        return controlQueue.sync(execute: body)
    }

    private func startOnQueue() -> XeroDictationOperationResponse {
        guard #available(macOS 10.15, *) else {
            return .failure(
                code: "dictation_legacy_runtime_unavailable",
                message: "Xero legacy dictation requires macOS 10.15 or newer.",
                retryable: false
            )
        }

        switch state {
        case .created:
            break
        case .started, .stopping:
            return .success(sessionId: sessionId, engine: "legacy", locale: localeIdentifier)
        case .stopped:
            return .failure(
                code: "dictation_session_stopped",
                message: "Xero cannot start a dictation session after it has already stopped.",
                retryable: false
            )
        }

        do {
            let permissions = try requestPermissions()
            emit([
                "kind": "permission",
                "microphone": permissions.microphone,
                "speech": permissions.speech,
            ])

            try validatePermissions(permissions)

            let requestedLocale = Locale(identifier: localeIdentifier)
            guard let recognizer = SFSpeechRecognizer(locale: requestedLocale) else {
                throw XeroLegacyDictationError(
                    code: "dictation_legacy_locale_unsupported",
                    message: "Xero could not create a legacy speech recognizer for \(localeIdentifier).",
                    retryable: false
                )
            }

            guard recognizer.isAvailable else {
                throw XeroLegacyDictationError(
                    code: "dictation_legacy_recognizer_unavailable",
                    message: "Legacy Apple speech recognition is not currently available for \(recognizer.locale.identifier).",
                    retryable: true
                )
            }

            try configurePrivacy(for: recognizer)
            recognizer.defaultTaskHint = .dictation
            recognizer.queue = resultQueue
            self.recognizer = recognizer
            self.state = .started

            do {
                try startRecognitionTaskOnQueue()
            } catch {
                cleanupRecognitionOnQueue(cancelTask: true)
                self.state = .stopped
                throw error
            }

            emit([
                "kind": "started",
                "sessionId": sessionId,
                "engine": "legacy",
                "locale": recognizer.locale.identifier,
            ])

            return .success(sessionId: sessionId, engine: "legacy", locale: recognizer.locale.identifier)
        } catch let error as XeroLegacyDictationError {
            cleanupRecognitionOnQueue(cancelTask: true)
            state = .stopped
            return .failure(code: error.code, message: error.message, retryable: error.retryable)
        } catch {
            cleanupRecognitionOnQueue(cancelTask: true)
            state = .stopped
            return .failure(
                code: "dictation_legacy_start_failed",
                message: "Xero could not start legacy dictation: \(error.localizedDescription)",
                retryable: true
            )
        }
    }

    private func stopOnQueue(cancelled: Bool, reason: String) -> XeroDictationOperationResponse {
        switch state {
        case .created:
            state = .stopped
            return .success()
        case .started, .stopping:
            state = .stopping
        case .stopped:
            return .success()
        }

        if !cancelled {
            commitLastPartialAsFinalOnQueue()
        }

        cleanupRecognitionOnQueue(cancelTask: cancelled)
        state = .stopped
        emitTerminalStoppedOnQueue(reason: reason)
        return .success()
    }

    private func requestPermissions() throws -> (microphone: String, speech: String) {
        if AVCaptureDevice.authorizationStatus(for: .audio) == .notDetermined {
            try ensurePrivacyPromptCanRun(
                key: "NSMicrophoneUsageDescription",
                code: "dictation_microphone_permission_prompt_unavailable",
                label: "microphone"
            )
            let semaphore = DispatchSemaphore(value: 0)
            AVCaptureDevice.requestAccess(for: .audio) { _ in
                semaphore.signal()
            }
            semaphore.wait()
        }

        if SFSpeechRecognizer.authorizationStatus() == .notDetermined {
            try ensurePrivacyPromptCanRun(
                key: "NSSpeechRecognitionUsageDescription",
                code: "dictation_speech_permission_prompt_unavailable",
                label: "speech recognition"
            )
            let semaphore = DispatchSemaphore(value: 0)
            SFSpeechRecognizer.requestAuthorization { _ in
                semaphore.signal()
            }
            semaphore.wait()
        }

        return (microphonePermissionState(), speechPermissionState())
    }

    private func ensurePrivacyPromptCanRun(key: String, code: String, label: String) throws {
        guard privacyUsageDescriptionVisibleToTcc(key) else {
            throw XeroLegacyDictationError(
                code: code,
                message: "Xero cannot request \(label) permission because macOS cannot see the app privacy usage string. Restart with pnpm run dev:tauri so the dev runner signs the Tauri binary, or use a bundled Xero build.",
                retryable: false
            )
        }
    }

    private func validatePermissions(_ permissions: (microphone: String, speech: String)) throws {
        guard permissions.microphone == "authorized" else {
            throw XeroLegacyDictationError(
                code: "dictation_microphone_permission_denied",
                message: "Xero needs microphone permission before it can start dictation.",
                retryable: false
            )
        }

        guard permissions.speech == "authorized" else {
            throw XeroLegacyDictationError(
                code: "dictation_speech_permission_denied",
                message: "Xero needs speech recognition permission before it can start dictation.",
                retryable: false
            )
        }
    }

    private func configurePrivacy(for recognizer: SFSpeechRecognizer) throws {
        switch privacyMode {
        case "allow_network":
            requiresOnDeviceRecognition = false
        case "on_device_required":
            guard recognizer.supportsOnDeviceRecognition else {
                throw XeroLegacyDictationError(
                    code: "dictation_legacy_on_device_unavailable",
                    message: "Xero could not start legacy dictation because on-device recognition is unavailable for \(recognizer.locale.identifier).",
                    retryable: false
                )
            }
            requiresOnDeviceRecognition = true
        default:
            guard recognizer.supportsOnDeviceRecognition else {
                throw XeroLegacyDictationError(
                    code: "dictation_legacy_network_recognition_required",
                    message: "On-device legacy dictation is unavailable for \(recognizer.locale.identifier). Allow Apple server recognition to use this locale.",
                    retryable: false
                )
            }
            requiresOnDeviceRecognition = true
        }
    }

    private func startRecognitionTaskOnQueue() throws {
        guard let recognizer else {
            throw XeroLegacyDictationError(
                code: "dictation_legacy_recognizer_missing",
                message: "Xero could not start legacy dictation because the speech recognizer was unavailable.",
                retryable: true
            )
        }

        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.taskHint = .dictation
        request.contextualStrings = normalizedContextualPhrases()
        request.requiresOnDeviceRecognition = requiresOnDeviceRecognition
        if #available(macOS 13.0, *) {
            request.addsPunctuation = true
        }

        generation += 1
        let taskGeneration = generation
        lastTaskTranscript = ""
        hasDetectedSpeech = false
        hasFinalResult = false
        lastSpeechActivityUptime = nil
        recognitionRequest = request

        recognitionTask = recognizer.recognitionTask(with: request) { [weak self] result, error in
            let update = XeroLegacyRecognitionUpdate(
                text: result?.bestTranscription.formattedString.trimmingCharacters(in: .whitespacesAndNewlines),
                isFinal: result?.isFinal ?? false,
                failure: error.map { error in
                    let nsError = error as NSError
                    return XeroLegacyRecognitionFailure(
                        domain: nsError.domain,
                        code: nsError.code,
                        message: nsError.localizedDescription
                    )
                }
            )

            self?.handleRecognitionCallback(update, generation: taskGeneration)
        }

        do {
            try startAudioCaptureOnQueue(request: request)
        } catch {
            recognitionTask?.cancel()
            recognitionRequest?.endAudio()
            recognitionTask = nil
            recognitionRequest = nil
            throw error
        }

        scheduleDurationLimitOnQueue(generation: taskGeneration)
    }

    private func startAudioCaptureOnQueue(request: SFSpeechAudioBufferRecognitionRequest) throws {
        stopAudioCaptureOnQueue()

        let inputNode = audioEngine.inputNode
        let inputFormat = inputNode.outputFormat(forBus: 0)
        guard inputFormat.sampleRate > 0, inputFormat.channelCount > 0 else {
            throw XeroLegacyDictationError(
                code: "dictation_legacy_audio_format_unavailable",
                message: "Xero could not find a microphone audio format for legacy dictation.",
                retryable: true
            )
        }

        inputNode.installTap(onBus: 0, bufferSize: 4096, format: inputFormat) { buffer, _ in
            request.append(buffer)
        }
        audioTapInstalled = true

        audioEngine.prepare()
        do {
            try audioEngine.start()
        } catch {
            stopAudioCaptureOnQueue()
            throw XeroLegacyDictationError(
                code: "dictation_legacy_audio_capture_failed",
                message: "Xero could not start microphone capture for legacy dictation: \(error.localizedDescription)",
                retryable: true
            )
        }
    }

    private func handleRecognitionCallback(_ update: XeroLegacyRecognitionUpdate, generation taskGeneration: UInt64) {
        controlQueue.async { [weak self] in
            self?.handleRecognitionOnQueue(update, generation: taskGeneration)
        }
    }

    private func handleRecognitionOnQueue(_ update: XeroLegacyRecognitionUpdate, generation taskGeneration: UInt64) {
        guard taskGeneration == generation else {
            return
        }
        guard state == .started || state == .stopping else {
            return
        }

        if let text = update.text, !text.isEmpty {
            hasDetectedSpeech = true
            lastSpeechActivityUptime = DispatchTime.now().uptimeNanoseconds
            emittedText = true
            sequence += 1

            if update.isFinal {
                hasFinalResult = true
                finalizedTranscript = appendTranscriptSegment(text, to: finalizedTranscript)
                lastTaskTranscript = ""
                emit([
                    "kind": "final",
                    "sessionId": sessionId,
                    "text": finalizedTranscript,
                    "sequence": sequence,
                ])
            } else {
                lastTaskTranscript = text
                emit([
                    "kind": "partial",
                    "sessionId": sessionId,
                    "text": appendTranscriptSegment(text, to: finalizedTranscript),
                    "sequence": sequence,
                ])
            }
        }

        if let failure = update.failure, state == .started {
            let mapped = mapRecognitionFailure(failure)
            emitTerminalErrorOnQueue(
                code: mapped.code,
                message: mapped.message,
                retryable: mapped.retryable
            )
        }
    }

    private func scheduleDurationLimitOnQueue(generation taskGeneration: UInt64) {
        durationTimer?.cancel()
        let timer = DispatchSource.makeTimerSource(queue: controlQueue)
        timer.schedule(deadline: .now() + Self.recognitionWindow)
        timer.setEventHandler { [weak self] in
            self?.handleDurationLimitOnQueue(generation: taskGeneration)
        }
        durationTimer = timer
        timer.resume()
    }

    private func handleDurationLimitOnQueue(generation taskGeneration: UInt64) {
        guard taskGeneration == generation, state == .started else {
            return
        }

        if hasDetectedSpeech && !hasFinalResult && hasRecentSpeechActivityOnQueue() {
            commitLastPartialAsFinalOnQueue()
            stopCurrentTaskForRolloverOnQueue()
            do {
                try startRecognitionTaskOnQueue()
            } catch let error as XeroLegacyDictationError {
                emitTerminalErrorOnQueue(code: error.code, message: error.message, retryable: error.retryable)
            } catch {
                emitTerminalErrorOnQueue(
                    code: "dictation_legacy_rollover_failed",
                    message: "Xero could not continue legacy dictation: \(error.localizedDescription)",
                    retryable: true
                )
            }
            return
        }

        _ = stopOnQueue(cancelled: false, reason: "user")
    }

    private func hasRecentSpeechActivityOnQueue() -> Bool {
        guard let lastSpeechActivityUptime else {
            return false
        }

        let now = DispatchTime.now().uptimeNanoseconds
        return now >= lastSpeechActivityUptime
            && now - lastSpeechActivityUptime <= Self.rolloverActivityGraceNanoseconds
    }

    private func stopCurrentTaskForRolloverOnQueue() {
        generation += 1
        durationTimer?.cancel()
        durationTimer = nil
        stopAudioCaptureOnQueue()
        recognitionRequest?.endAudio()
        recognitionTask?.cancel()
        recognitionRequest = nil
        recognitionTask = nil
    }

    private func cleanupRecognitionOnQueue(cancelTask: Bool) {
        generation += 1
        durationTimer?.cancel()
        durationTimer = nil
        stopAudioCaptureOnQueue()
        recognitionRequest?.endAudio()
        if cancelTask {
            recognitionTask?.cancel()
        } else {
            recognitionTask?.finish()
        }
        recognitionRequest = nil
        recognitionTask = nil
        recognizer = nil
        resultQueue.cancelAllOperations()
    }

    private func stopAudioCaptureOnQueue() {
        if audioEngine.isRunning {
            audioEngine.stop()
        }

        if audioTapInstalled {
            audioEngine.inputNode.removeTap(onBus: 0)
            audioTapInstalled = false
        }
    }

    private func commitLastPartialAsFinalOnQueue() {
        let text = lastTaskTranscript.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else {
            return
        }

        finalizedTranscript = appendTranscriptSegment(text, to: finalizedTranscript)
        lastTaskTranscript = ""
        hasFinalResult = true
        emittedText = true
        sequence += 1
        emit([
            "kind": "final",
            "sessionId": sessionId,
            "text": finalizedTranscript,
            "sequence": sequence,
        ])
    }

    private func emitTerminalStoppedOnQueue(reason: String) {
        guard !terminalEventEmitted else {
            return
        }

        terminalEventEmitted = true
        emit([
            "kind": "stopped",
            "sessionId": sessionId,
            "reason": reason,
        ])
    }

    private func emitTerminalErrorOnQueue(code: String, message: String, retryable: Bool) {
        guard !terminalEventEmitted else {
            return
        }

        cleanupRecognitionOnQueue(cancelTask: true)
        state = .stopped
        terminalEventEmitted = true
        emit([
            "kind": "error",
            "sessionId": sessionId,
            "code": emittedText ? code : "dictation_legacy_startup_failed",
            "message": message,
            "retryable": retryable,
        ])
    }

    private func mapRecognitionFailure(_ failure: XeroLegacyRecognitionFailure) -> XeroLegacyDictationError {
        if failure.code == 1700 {
            return XeroLegacyDictationError(
                code: "dictation_speech_permission_denied",
                message: "Xero needs speech recognition permission before it can continue dictation.",
                retryable: false
            )
        }

        if failure.domain == "kLSRErrorDomain", failure.code == 201 {
            return XeroLegacyDictationError(
                code: "dictation_legacy_dictation_disabled",
                message: "Apple dictation appears to be disabled in System Settings.",
                retryable: false
            )
        }

        if failure.domain == "kLSRErrorDomain", failure.code == 102 {
            return XeroLegacyDictationError(
                code: "dictation_legacy_assets_missing",
                message: "Apple speech assets are missing for legacy on-device dictation.",
                retryable: true
            )
        }

        let retryableCodes: Set<Int> = [203, 300, 1100, 1101, 1107, 1110]
        return XeroLegacyDictationError(
            code: "dictation_legacy_recognition_failed",
            message: "Legacy dictation stopped unexpectedly: \(failure.message)",
            retryable: retryableCodes.contains(failure.code)
        )
    }

    private func normalizedContextualPhrases() -> [String] {
        let appPhrases = [
            "Xero",
            "Tauri",
            "ShadCN",
            "OpenAI",
            "OpenRouter",
            "Anthropic",
            "Claude",
            "Codex",
            "Rust",
            "Swift",
            "TypeScript",
            "pnpm",
            "Cargo",
        ]
        var seen = Set<String>()
        var phrases: [String] = []

        for phrase in contextualPhrases + appPhrases {
            let trimmed = phrase.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else {
                continue
            }

            let key = trimmed.lowercased()
            if seen.insert(key).inserted {
                phrases.append(trimmed)
            }

            if phrases.count >= 100 {
                break
            }
        }

        return phrases
    }

    private func appendTranscriptSegment(_ segment: String, to transcript: String) -> String {
        guard !segment.isEmpty else {
            return transcript
        }
        guard !transcript.isEmpty else {
            return segment
        }
        if transcript.hasSuffix(" ") || segment.hasPrefix(" ") {
            return transcript + segment
        }
        return transcript + " " + segment
    }

    private func emit(_ payload: [String: Any]) {
        emitPayload(payload)
    }

    private func forceCleanup() {
        durationTimer?.cancel()
        durationTimer = nil
        if audioEngine.isRunning {
            audioEngine.stop()
        }
        if audioTapInstalled {
            audioEngine.inputNode.removeTap(onBus: 0)
            audioTapInstalled = false
        }
        recognitionRequest?.endAudio()
        recognitionTask?.cancel()
        recognitionRequest = nil
        recognitionTask = nil
        recognizer = nil
        resultQueue.cancelAllOperations()
    }
}
