import AVFoundation
import Dispatch
import Foundation
import Speech

func cadenceDictationModernCompiled() -> Bool {
    true
}

func cadenceDictationModernRuntimeSupported() -> Bool {
    if #available(macOS 26.0, *) {
        _ = SpeechAnalyzer.self
        _ = DictationTranscriber.self
        _ = AssetInventory.self
        return true
    }

    return false
}

@available(macOS 26.0, *)
final class CadenceModernDictationEngine {
    private let sessionId: String
    private let localeIdentifier: String
    private let privacyMode: String
    private let contextualPhrases: [String]
    private let emitPayload: ([String: Any]) -> Void
    private let audioEngine = AVAudioEngine()
    private let audioQueue = DispatchQueue(label: "dev.cadence.dictation.modern.audio")
    private let audioQueueKey = DispatchSpecificKey<Void>()
    private let lock = NSLock()

    private var analyzer: SpeechAnalyzer?
    private var transcriber: DictationTranscriber?
    private var inputBuilder: AsyncStream<AnalyzerInput>.Continuation?
    private var analyzerFormat: AVAudioFormat?
    private var resultTask: Task<Void, Never>?
    private var progressTask: Task<Void, Never>?
    private var isStopping = false
    private var isFinished = false
    private var audioTapInstalled = false
    private var finalizedTranscript = ""
    private var volatileTranscript = ""
    private var sequence: UInt64 = 0
    private var emittedText = false

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
        self.audioQueue.setSpecific(key: audioQueueKey, value: ())
    }

    func start() -> CadenceDictationOperationResponse {
        var response: CadenceDictationOperationResponse?
        let semaphore = DispatchSemaphore(value: 0)

        Task {
            do {
                let locale = try await startAsync()
                response = .success(sessionId: sessionId, engine: "modern", locale: locale.identifier)
            } catch let error as CadenceModernDictationError {
                await cleanupAfterFailedStart()
                response = .failure(code: error.code, message: error.message, retryable: error.retryable)
            } catch {
                await cleanupAfterFailedStart()
                response = .failure(
                    code: "dictation_modern_start_failed",
                    message: "Cadence could not start modern dictation: \(error.localizedDescription)",
                    retryable: true
                )
            }
            semaphore.signal()
        }

        semaphore.wait()
        return response ?? .failure(
            code: "dictation_modern_start_failed",
            message: "Cadence could not start modern dictation.",
            retryable: true
        )
    }

    func stop(reason: String) -> CadenceDictationOperationResponse {
        waitForStop(cancelled: false, reason: reason)
    }

    func cancel() -> CadenceDictationOperationResponse {
        waitForStop(cancelled: true, reason: "cancelled")
    }

    private func startAsync() async throws -> Locale {
        _ = privacyMode
        try Task.checkCancellation()

        let permissions = await requestPermissions()
        emit([
            "kind": "permission",
            "microphone": permissions.microphone,
            "speech": permissions.speech,
        ])
        try validatePermissions(permissions)

        let requestedLocale = Locale(identifier: localeIdentifier)
        guard let locale = await DictationTranscriber.supportedLocale(equivalentTo: requestedLocale) else {
            throw CadenceModernDictationError(
                code: "dictation_modern_locale_unsupported",
                message: "Cadence could not find a modern on-device dictation locale equivalent to \(localeIdentifier).",
                retryable: false
            )
        }

        let transcriber = DictationTranscriber(
            locale: locale,
            contentHints: [],
            transcriptionOptions: [.punctuation],
            reportingOptions: [.volatileResults, .frequentFinalization],
            attributeOptions: []
        )
        let modules: [any SpeechModule] = [transcriber]
        let analyzer = SpeechAnalyzer(modules: modules)

        if !contextualPhrases.isEmpty {
            let analysisContext = AnalysisContext()
            analysisContext.contextualStrings[.general] = contextualPhrases
            try await analyzer.setContext(analysisContext)
        }

        try await ensureAssetsInstalled(for: modules)

        let naturalFormat = audioEngine.inputNode.outputFormat(forBus: 0)
        let preferredFormat = await SpeechAnalyzer.bestAvailableAudioFormat(
            compatibleWith: modules,
            considering: naturalFormat
        )
        let fallbackFormat = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: modules)
        guard let analyzerFormat = preferredFormat ?? fallbackFormat else {
            throw CadenceModernDictationError(
                code: "dictation_modern_audio_format_unavailable",
                message: "Cadence could not find an audio format compatible with modern dictation.",
                retryable: true
            )
        }

        let (inputSequence, inputBuilder) = AsyncStream.makeStream(of: AnalyzerInput.self)
        let resultTask = makeResultTask(transcriber: transcriber)

        lock.withLock {
            self.analyzer = analyzer
            self.transcriber = transcriber
            self.inputBuilder = inputBuilder
            self.analyzerFormat = analyzerFormat
            self.resultTask = resultTask
        }

        try await analyzer.prepareToAnalyze(in: analyzerFormat)
        try await analyzer.start(inputSequence: inputSequence)
        try startAudioEngine(format: analyzerFormat)

        emit([
            "kind": "started",
            "sessionId": sessionId,
            "engine": "modern",
            "locale": locale.identifier,
        ])

        return locale
    }

    private func ensureAssetsInstalled(for modules: [any SpeechModule]) async throws {
        guard let installationRequest = try await AssetInventory.assetInstallationRequest(supporting: modules) else {
            return
        }

        emit([
            "kind": "asset_installing",
            "progress": 0.0,
        ])
        let task = monitorProgress(installationRequest.progress)
        lock.withLock {
            progressTask = task
        }
        defer {
            task.cancel()
            lock.withLock {
                progressTask = nil
            }
        }

        do {
            try await installationRequest.downloadAndInstall()
            emit([
                "kind": "asset_installing",
                "progress": 1.0,
            ])
        } catch {
            throw CadenceModernDictationError(
                code: "dictation_modern_asset_install_failed",
                message: "Cadence could not install Apple speech assets for modern dictation: \(error.localizedDescription)",
                retryable: true
            )
        }
    }

    private func monitorProgress(_ progress: Progress) -> Task<Void, Never> {
        Task { [weak self] in
            var lastProgress = -1.0
            while !Task.isCancelled && !progress.isFinished {
                let currentProgress = min(1.0, max(0.0, progress.fractionCompleted))
                if abs(currentProgress - lastProgress) >= 0.01 {
                    lastProgress = currentProgress
                    self?.emit([
                        "kind": "asset_installing",
                        "progress": currentProgress,
                    ])
                }
                try? await Task.sleep(nanoseconds: 150_000_000)
            }
        }
    }

    private func makeResultTask(transcriber: DictationTranscriber) -> Task<Void, Never> {
        Task { [weak self] in
            do {
                for try await result in transcriber.results {
                    self?.handle(result: result)
                }
            } catch is CancellationError {
                return
            } catch {
                self?.handleModernError(
                    code: "dictation_modern_result_stream_failed",
                    message: "Modern dictation stopped unexpectedly: \(error.localizedDescription)",
                    retryable: true
                )
            }
        }
    }

    private func startAudioEngine(format: AVAudioFormat) throws {
        let inputNode = audioEngine.inputNode
        let inputFormat = inputNode.outputFormat(forBus: 0)

        inputNode.installTap(onBus: 0, bufferSize: 4096, format: inputFormat) { [weak self] buffer, _ in
            self?.enqueue(buffer: buffer)
        }
        lock.withLock {
            audioTapInstalled = true
        }

        audioEngine.prepare()
        do {
            try audioEngine.start()
        } catch {
            inputNode.removeTap(onBus: 0)
            lock.withLock {
                audioTapInstalled = false
            }
            throw CadenceModernDictationError(
                code: "dictation_modern_audio_capture_failed",
                message: "Cadence could not start microphone capture for modern dictation: \(error.localizedDescription)",
                retryable: true
            )
        }

        lock.withLock {
            analyzerFormat = format
        }
    }

    private func enqueue(buffer: AVAudioPCMBuffer) {
        audioQueue.async { [weak self] in
            guard let self else {
                return
            }

            let snapshot = lock.withLock {
                (self.inputBuilder, self.analyzerFormat, self.isStopping || self.isFinished)
            }
            guard let inputBuilder = snapshot.0, let analyzerFormat = snapshot.1, !snapshot.2 else {
                return
            }

            do {
                let convertedBuffer = try convert(buffer: buffer, to: analyzerFormat)
                inputBuilder.yield(AnalyzerInput(buffer: convertedBuffer))
            } catch {
                handleModernError(
                    code: "dictation_modern_audio_conversion_failed",
                    message: "Cadence could not convert microphone audio for modern dictation: \(error.localizedDescription)",
                    retryable: true
                )
            }
        }
    }

    private func convert(buffer: AVAudioPCMBuffer, to outputFormat: AVAudioFormat) throws -> AVAudioPCMBuffer {
        if buffer.format == outputFormat {
            return buffer
        }

        guard let converter = AVAudioConverter(from: buffer.format, to: outputFormat) else {
            throw CadenceModernDictationError(
                code: "dictation_modern_audio_converter_unavailable",
                message: "Cadence could not create a microphone audio converter for modern dictation.",
                retryable: true
            )
        }

        let frameRatio = outputFormat.sampleRate / buffer.format.sampleRate
        let frameCapacity = AVAudioFrameCount((Double(buffer.frameLength) * frameRatio).rounded(.up)) + 1
        guard let convertedBuffer = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: frameCapacity) else {
            throw CadenceModernDictationError(
                code: "dictation_modern_audio_buffer_unavailable",
                message: "Cadence could not allocate a microphone audio buffer for modern dictation.",
                retryable: true
            )
        }

        var didProvideInput = false
        var conversionError: NSError?
        let status = converter.convert(to: convertedBuffer, error: &conversionError) { _, outStatus in
            if didProvideInput {
                outStatus.pointee = .noDataNow
                return nil
            }

            didProvideInput = true
            outStatus.pointee = .haveData
            return buffer
        }

        if status == .error {
            throw conversionError ?? CadenceModernDictationError(
                code: "dictation_modern_audio_conversion_failed",
                message: "Cadence could not convert microphone audio for modern dictation.",
                retryable: true
            )
        }

        return convertedBuffer
    }

    private func handle(result: DictationTranscriber.Result) {
        let text = String(result.text.characters).trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else {
            return
        }

        let payload: [String: Any] = lock.withLock {
            emittedText = true
            sequence += 1

            if result.isFinal {
                finalizedTranscript = appendTranscriptSegment(text, to: finalizedTranscript)
                volatileTranscript = ""
                return [
                    "kind": "final",
                    "sessionId": sessionId,
                    "text": finalizedTranscript,
                    "sequence": sequence,
                ]
            }

            volatileTranscript = text
            return [
                "kind": "partial",
                "sessionId": sessionId,
                "text": appendTranscriptSegment(volatileTranscript, to: finalizedTranscript),
                "sequence": sequence,
            ]
        }

        emit(payload)
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

    private func waitForStop(cancelled: Bool, reason: String) -> CadenceDictationOperationResponse {
        var response: CadenceDictationOperationResponse?
        let semaphore = DispatchSemaphore(value: 0)

        Task {
            response = await stopAsync(cancelled: cancelled, reason: reason)
            semaphore.signal()
        }

        semaphore.wait()
        return response ?? .success()
    }

    private func stopAsync(cancelled: Bool, reason: String) async -> CadenceDictationOperationResponse {
        let shouldStop = lock.withLock { () -> Bool in
            if isFinished {
                return false
            }
            isStopping = true
            return true
        }
        guard shouldStop else {
            return .success()
        }

        stopAudioCapture()
        lock.withLock {
            inputBuilder?.finish()
            progressTask?.cancel()
            progressTask = nil
        }

        let analyzer = lock.withLock { self.analyzer }
        if cancelled {
            await analyzer?.cancelAndFinishNow()
        } else {
            do {
                try await analyzer?.finalizeAndFinishThroughEndOfInput()
            } catch {
                markFinished()
                emit([
                    "kind": "error",
                    "sessionId": sessionId,
                    "code": "dictation_modern_finalize_failed",
                    "message": "Cadence could not finalize modern dictation: \(error.localizedDescription)",
                    "retryable": true,
                ])
                return .failure(
                    code: "dictation_modern_finalize_failed",
                    message: "Cadence could not finalize modern dictation: \(error.localizedDescription)",
                    retryable: true
                )
            }
        }

        let resultTask = lock.withLock { self.resultTask }
        if cancelled {
            resultTask?.cancel()
        }
        await resultTask?.value

        markFinished()
        emit([
            "kind": "stopped",
            "sessionId": sessionId,
            "reason": reason,
        ])
        return .success()
    }

    private func cleanupAfterFailedStart() async {
        stopAudioCapture()
        let analyzer = lock.withLock { () -> SpeechAnalyzer? in
            isStopping = true
            inputBuilder?.finish()
            progressTask?.cancel()
            resultTask?.cancel()
            return self.analyzer
        }
        await analyzer?.cancelAndFinishNow()
        markFinished()
    }

    private func stopAudioCapture() {
        if DispatchQueue.getSpecific(key: audioQueueKey) != nil {
            stopAudioCaptureOnAudioQueue()
        } else {
            audioQueue.sync {
                stopAudioCaptureOnAudioQueue()
            }
        }
    }

    private func stopAudioCaptureOnAudioQueue() {
        if audioEngine.isRunning {
            audioEngine.stop()
        }

        let shouldRemoveTap = lock.withLock { () -> Bool in
            let installed = audioTapInstalled
            audioTapInstalled = false
            return installed
        }
        if shouldRemoveTap {
            audioEngine.inputNode.removeTap(onBus: 0)
        }
    }

    private func handleModernError(code: String, message: String, retryable: Bool) {
        let shouldEmit = lock.withLock { () -> Bool in
            if isStopping || isFinished {
                return false
            }
            isFinished = true
            isStopping = true
            return true
        }
        guard shouldEmit else {
            return
        }

        stopAudioCapture()
        lock.withLock {
            inputBuilder?.finish()
            resultTask?.cancel()
            progressTask?.cancel()
            progressTask = nil
        }

        emit([
            "kind": "error",
            "sessionId": sessionId,
            "code": emittedText ? code : "dictation_modern_startup_failed",
            "message": message,
            "retryable": retryable,
        ])
    }

    private func markFinished() {
        lock.withLock {
            isFinished = true
            isStopping = true
            analyzer = nil
            transcriber = nil
            inputBuilder = nil
            analyzerFormat = nil
            resultTask = nil
            progressTask = nil
        }
    }

    private func requestPermissions() async -> (microphone: String, speech: String) {
        if AVCaptureDevice.authorizationStatus(for: .audio) == .notDetermined {
            _ = await AVCaptureDevice.requestAccess(for: .audio)
        }

        if SFSpeechRecognizer.authorizationStatus() == .notDetermined {
            _ = await withCheckedContinuation { continuation in
                SFSpeechRecognizer.requestAuthorization { status in
                    continuation.resume(returning: status)
                }
            }
        }

        return (microphonePermissionState(), speechPermissionState())
    }

    private func validatePermissions(_ permissions: (microphone: String, speech: String)) throws {
        guard permissions.microphone == "authorized" else {
            throw CadenceModernDictationError(
                code: "dictation_microphone_permission_denied",
                message: "Cadence needs microphone permission before it can start dictation.",
                retryable: false
            )
        }

        guard permissions.speech == "authorized" else {
            throw CadenceModernDictationError(
                code: "dictation_speech_permission_denied",
                message: "Cadence needs speech recognition permission before it can start dictation.",
                retryable: false
            )
        }
    }

    private func emit(_ payload: [String: Any]) {
        emitPayload(payload)
    }
}

private struct CadenceModernDictationError: Error {
    let code: String
    let message: String
    let retryable: Bool
}

private extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer {
            unlock()
        }
        return try body()
    }
}
