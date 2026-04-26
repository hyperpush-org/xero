import AVFoundation
import Darwin
import Foundation
import Speech

struct CadenceDictationCapabilityStatus: Encodable {
    let platform: String
    let defaultLocale: String?
    let modernCompiled: Bool
    let modernRuntimeSupported: Bool
    let legacyRuntimeSupported: Bool
    let legacyRecognizerAvailable: Bool
    let microphonePermission: String
    let speechPermission: String
}

@_cdecl("cadence_dictation_capability_status_json")
public func cadenceDictationCapabilityStatusJson() -> UnsafeMutablePointer<CChar>? {
    let localeIdentifier = Locale.current.identifier
    let legacy = legacySpeechAvailability(localeIdentifier: localeIdentifier)
    let status = CadenceDictationCapabilityStatus(
        platform: "macos",
        defaultLocale: localeIdentifier,
        modernCompiled: cadenceDictationModernCompiled(),
        modernRuntimeSupported: cadenceDictationModernRuntimeSupported(),
        legacyRuntimeSupported: legacy.runtimeSupported,
        legacyRecognizerAvailable: legacy.recognizerAvailable,
        microphonePermission: microphonePermissionState(),
        speechPermission: speechPermissionState()
    )

    guard let data = try? JSONEncoder().encode(status),
          let json = String(data: data, encoding: .utf8) else {
        return duplicateCString(#"{"platform":"macos","modernCompiled":false,"modernRuntimeSupported":false,"legacyRuntimeSupported":false,"legacyRecognizerAvailable":false,"microphonePermission":"unknown","speechPermission":"unknown"}"#)
    }

    return duplicateCString(json)
}

@_cdecl("cadence_dictation_free_string")
public func cadenceDictationFreeString(_ value: UnsafeMutablePointer<CChar>?) {
    if let value = value {
        free(value)
    }
}

func duplicateCString(_ value: String) -> UnsafeMutablePointer<CChar>? {
    value.withCString { pointer in
        strdup(pointer)
    }
}

func microphonePermissionState() -> String {
    if #available(macOS 10.14, *) {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            return "authorized"
        case .denied:
            return "denied"
        case .restricted:
            return "restricted"
        case .notDetermined:
            return "not_determined"
        @unknown default:
            return "unknown"
        }
    }

    return "unsupported"
}

func speechPermissionState() -> String {
    if #available(macOS 10.15, *) {
        switch SFSpeechRecognizer.authorizationStatus() {
        case .authorized:
            return "authorized"
        case .denied:
            return "denied"
        case .restricted:
            return "restricted"
        case .notDetermined:
            return "not_determined"
        @unknown default:
            return "unknown"
        }
    }

    return "unsupported"
}

func legacySpeechAvailability(localeIdentifier: String) -> (runtimeSupported: Bool, recognizerAvailable: Bool) {
    if #available(macOS 10.15, *) {
        let recognizer = SFSpeechRecognizer(locale: Locale(identifier: localeIdentifier))
        return (true, recognizer?.isAvailable ?? false)
    }

    return (false, false)
}
