func xeroDictationModernCompiled() -> Bool {
    false
}

func xeroDictationModernRuntimeSupported() -> Bool {
    false
}

func xeroDictationModernAssetProbe(localeIdentifier: String) -> (status: String, localeIdentifier: String?, reason: String?) {
    _ = localeIdentifier
    return ("unavailable", nil, "modern_sdk_unavailable")
}

final class XeroModernDictationEngine {
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

    func start() -> XeroDictationOperationResponse {
        .failure(
            code: "dictation_modern_sdk_unavailable",
            message: "Xero was built with a macOS SDK that does not include SpeechAnalyzer.",
            retryable: false
        )
    }

    func stop(reason: String) -> XeroDictationOperationResponse {
        _ = reason
        return .success()
    }

    func cancel() -> XeroDictationOperationResponse {
        .success()
    }
}
