import AVFoundation
import Darwin
import Foundation
import Security
import Speech

struct XeroDictationCapabilityStatus: Encodable {
    let platform: String
    let osVersion: String
    let defaultLocale: String?
    let supportedLocales: [String]
    let modernCompiled: Bool
    let modernRuntimeSupported: Bool
    let modernAssetsStatus: String
    let modernAssetLocale: String?
    let modernAssetsReason: String?
    let legacyRuntimeSupported: Bool
    let legacyRecognizerAvailable: Bool
    let microphonePermission: String
    let speechPermission: String
}

@_cdecl("xero_dictation_capability_status_json")
public func xeroDictationCapabilityStatusJson() -> UnsafeMutablePointer<CChar>? {
    let localeIdentifier = Locale.current.identifier
    let legacy = legacySpeechAvailability(localeIdentifier: localeIdentifier)
    let modernAssets = xeroDictationModernAssetProbe(localeIdentifier: localeIdentifier)
    let status = XeroDictationCapabilityStatus(
        platform: "macos",
        osVersion: operatingSystemVersionString(),
        defaultLocale: localeIdentifier,
        supportedLocales: supportedSpeechLocaleIdentifiers(),
        modernCompiled: xeroDictationModernCompiled(),
        modernRuntimeSupported: xeroDictationModernRuntimeSupported(),
        modernAssetsStatus: modernAssets.status,
        modernAssetLocale: modernAssets.localeIdentifier,
        modernAssetsReason: modernAssets.reason,
        legacyRuntimeSupported: legacy.runtimeSupported,
        legacyRecognizerAvailable: legacy.recognizerAvailable,
        microphonePermission: microphonePermissionState(),
        speechPermission: speechPermissionState()
    )

    guard let data = try? JSONEncoder().encode(status),
          let json = String(data: data, encoding: .utf8) else {
        return duplicateCString(#"{"platform":"macos","modernCompiled":false,"modernRuntimeSupported":false,"modernAssetsStatus":"unknown","legacyRuntimeSupported":false,"legacyRecognizerAvailable":false,"microphonePermission":"unknown","speechPermission":"unknown"}"#)
    }

    return duplicateCString(json)
}

@_cdecl("xero_dictation_free_string")
public func xeroDictationFreeString(_ value: UnsafeMutablePointer<CChar>?) {
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

func operatingSystemVersionString() -> String {
    let version = ProcessInfo.processInfo.operatingSystemVersion
    return "\(version.majorVersion).\(version.minorVersion).\(version.patchVersion)"
}

func supportedSpeechLocaleIdentifiers() -> [String] {
    if #available(macOS 10.15, *) {
        return SFSpeechRecognizer.supportedLocales()
            .map(\.identifier)
            .sorted()
    }

    return []
}

func legacySpeechAvailability(localeIdentifier: String) -> (runtimeSupported: Bool, recognizerAvailable: Bool) {
    if #available(macOS 10.15, *) {
        let recognizer = SFSpeechRecognizer(locale: Locale(identifier: localeIdentifier))
        return (true, recognizer?.isAvailable ?? false)
    }

    return (false, false)
}

func privacyUsageDescriptionVisibleToTcc(_ key: String) -> Bool {
    if signedInfoPlistContainsNonEmptyString(key) {
        return true
    }

    guard Bundle.main.bundleURL.pathExtension == "app" else {
        return false
    }

    return bundleInfoPlistContainsNonEmptyString(key)
}

private func bundleInfoPlistContainsNonEmptyString(_ key: String) -> Bool {
    guard let value = Bundle.main.object(forInfoDictionaryKey: key) as? String else {
        return false
    }

    return !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
}

private func signedInfoPlistContainsNonEmptyString(_ key: String) -> Bool {
    var dynamicCode: SecCode?
    guard SecCodeCopySelf(SecCSFlags(), &dynamicCode) == errSecSuccess,
          let dynamicCode else {
        return false
    }

    var staticCode: SecStaticCode?
    guard SecCodeCopyStaticCode(dynamicCode, SecCSFlags(), &staticCode) == errSecSuccess,
          let staticCode else {
        return false
    }

    var rawInformation: CFDictionary?
    let flags = SecCSFlags(rawValue: kSecCSSigningInformation)
    guard SecCodeCopySigningInformation(staticCode, flags, &rawInformation) == errSecSuccess,
          let information = rawInformation as? [String: Any],
          let plist = information[kSecCodeInfoPList as String] as? [String: Any],
          let value = plist[key] as? String else {
        return false
    }

    return !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
}
