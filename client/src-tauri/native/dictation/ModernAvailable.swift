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
