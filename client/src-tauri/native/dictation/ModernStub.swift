func cadenceDictationModernCompiled() -> Bool {
    false
}

func cadenceDictationModernRuntimeSupported() -> Bool {
    false
}

final class CadenceModernDictationEngine {
    init(
        sessionId: String,
        localeIdentifier: String,
        privacyMode: String,
        contextualPhrases: [String],
        emit: @escaping ([String: Any]) -> Void
    ) {
        _ = sessionId
        _ = localeIdentifier
        _ = privacyMode
        _ = contextualPhrases
        _ = emit
    }

    func start() -> CadenceDictationOperationResponse {
        .failure(
            code: "dictation_modern_sdk_unavailable",
            message: "Cadence was built with a macOS SDK that does not include SpeechAnalyzer.",
            retryable: false
        )
    }

    func stop(reason: String) -> CadenceDictationOperationResponse {
        _ = reason
        return .success()
    }

    func cancel() -> CadenceDictationOperationResponse {
        .success()
    }
}
