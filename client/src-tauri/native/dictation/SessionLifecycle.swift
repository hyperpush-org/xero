import Darwin
import Dispatch
import Foundation

public typealias XeroDictationEventCallback = @convention(c) (
    UnsafeMutableRawPointer?,
    UnsafePointer<CChar>?
) -> Void

private struct XeroDictationSessionRequest: Decodable {
    let sessionId: String
    let engine: String
    let locale: String
    let privacyMode: String?
    let contextualPhrases: [String]?
}

struct XeroDictationOperationResponse: Encodable {
    let ok: Bool
    let sessionId: String?
    let engine: String?
    let locale: String?
    let code: String?
    let message: String?
    let retryable: Bool?

    static func success(sessionId: String? = nil, engine: String? = nil, locale: String? = nil) -> Self {
        Self(
            ok: true,
            sessionId: sessionId,
            engine: engine,
            locale: locale,
            code: nil,
            message: nil,
            retryable: nil
        )
    }

    static func failure(code: String, message: String, retryable: Bool = false) -> Self {
        Self(
            ok: false,
            sessionId: nil,
            engine: nil,
            locale: nil,
            code: code,
            message: message,
            retryable: retryable
        )
    }
}

private final class XeroDictationSession {
    private enum LifecycleState {
        case created
        case started
        case stopped
    }

    private let sessionId: String
    private let engine: String
    private let locale: String
    private let privacyMode: String
    private let contextualPhrases: [String]
    private let callback: XeroDictationEventCallback?
    private let context: UnsafeMutableRawPointer?
    private let queue: DispatchQueue
    private let callbackQueue: DispatchQueue
    private var state: LifecycleState = .created
    private var modernEngine: XeroModernDictationEngine?
    private var legacyEngine: XeroLegacyDictationEngine?

    init(request: XeroDictationSessionRequest, callback: XeroDictationEventCallback?, context: UnsafeMutableRawPointer?) {
        self.sessionId = request.sessionId
        self.engine = request.engine
        self.locale = request.locale
        self.privacyMode = request.privacyMode ?? "on_device_preferred"
        self.contextualPhrases = request.contextualPhrases ?? []
        self.callback = callback
        self.context = context
        self.queue = DispatchQueue(label: "dev.xero.dictation.session.\(request.sessionId)")
        self.callbackQueue = DispatchQueue(label: "dev.xero.dictation.session.\(request.sessionId).events")
    }

    deinit {
        if let modernEngine {
            _ = modernEngine.cancel()
        }
        if let legacyEngine {
            _ = legacyEngine.cancel()
        }
    }

    func start() -> XeroDictationOperationResponse {
        queue.sync {
            switch state {
            case .created:
                if engine == "modern" {
                    if #available(macOS 26.0, *) {
                        let modernEngine = XeroModernDictationEngine(
                            sessionId: sessionId,
                            localeIdentifier: locale,
                            privacyMode: privacyMode,
                            contextualPhrases: contextualPhrases,
                            emit: { [weak self] payload in
                                self?.emit(payload)
                            }
                        )
                        self.modernEngine = modernEngine
                        let response = modernEngine.start()
                        if response.ok {
                            state = .started
                        } else {
                            state = .stopped
                            self.modernEngine = nil
                        }
                        return response
                    }

                    state = .stopped
                    return .failure(
                        code: "dictation_modern_runtime_unavailable",
                        message: "Xero could not start modern dictation because macOS 26 SpeechAnalyzer is unavailable.",
                        retryable: false
                    )
                } else {
                    let legacyEngine = XeroLegacyDictationEngine(
                        sessionId: sessionId,
                        localeIdentifier: locale,
                        privacyMode: privacyMode,
                        contextualPhrases: contextualPhrases,
                        emit: { [weak self] payload in
                            self?.emit(payload)
                        }
                    )
                    self.legacyEngine = legacyEngine
                    let response = legacyEngine.start()
                    if response.ok {
                        state = .started
                    } else {
                        state = .stopped
                        self.legacyEngine = nil
                    }
                    return response
                }
            case .started:
                break
            case .stopped:
                return .failure(
                    code: "dictation_session_stopped",
                    message: "Xero cannot start a dictation session after it has already stopped."
                )
            }

            return .success(sessionId: sessionId, engine: engine, locale: locale)
        }
    }

    func stop() -> XeroDictationOperationResponse {
        if engine == "modern" {
            return endModern(reason: "user")
        }

        return endLegacy(reason: "user")
    }

    func cancel() -> XeroDictationOperationResponse {
        if engine == "modern" {
            return endModern(reason: "cancelled")
        }

        return endLegacy(reason: "cancelled")
    }

    private func endModern(reason: String) -> XeroDictationOperationResponse {
        queue.sync {
            switch state {
            case .created, .started:
                state = .stopped
                let response: XeroDictationOperationResponse
                if reason == "cancelled" {
                    response = modernEngine?.cancel() ?? .success()
                } else {
                    response = modernEngine?.stop(reason: reason) ?? .success()
                }
                modernEngine = nil
                return response
            case .stopped:
                return .success()
            }
        }
    }

    private func endLegacy(reason: String) -> XeroDictationOperationResponse {
        queue.sync {
            switch state {
            case .created, .started:
                state = .stopped
                let response: XeroDictationOperationResponse
                if reason == "cancelled" {
                    response = legacyEngine?.cancel() ?? .success()
                } else {
                    response = legacyEngine?.stop(reason: reason) ?? .success()
                }
                legacyEngine = nil
                return response
            case .stopped:
                return .success()
            }
        }
    }

    private func emit(_ payload: [String: Any]) {
        callbackQueue.sync { [callback, context] in
            guard let callback else {
                return
            }
            guard JSONSerialization.isValidJSONObject(payload),
                  let data = try? JSONSerialization.data(withJSONObject: payload),
                  let json = String(data: data, encoding: .utf8) else {
                let fallback = #"{"kind":"error","sessionId":null,"code":"native_event_encoding_failed","message":"Xero could not encode a native dictation event.","retryable":false}"#
                fallback.withCString { pointer in
                    callback(context, pointer)
                }
                return
            }

            json.withCString { pointer in
                callback(context, pointer)
            }
        }
    }
}

@_cdecl("xero_dictation_create_session")
public func xeroDictationCreateSession(
    _ requestJson: UnsafePointer<CChar>?,
    _ callback: XeroDictationEventCallback?,
    _ context: UnsafeMutableRawPointer?
) -> UnsafeMutableRawPointer? {
    guard let requestJson else {
        return nil
    }

    let json = String(cString: requestJson)
    guard let data = json.data(using: .utf8),
          let request = try? JSONDecoder().decode(XeroDictationSessionRequest.self, from: data) else {
        return nil
    }

    let session = XeroDictationSession(request: request, callback: callback, context: context)
    return Unmanaged.passRetained(session).toOpaque()
}

@_cdecl("xero_dictation_start_session")
public func xeroDictationStartSession(_ session: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? {
    guard let session else {
        return operationResponseString(.failure(
            code: "native_session_missing",
            message: "Xero could not start dictation because the native session was unavailable."
        ))
    }

    let instance = Unmanaged<XeroDictationSession>.fromOpaque(session).takeUnretainedValue()
    return operationResponseString(instance.start())
}

@_cdecl("xero_dictation_stop_session")
public func xeroDictationStopSession(_ session: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? {
    guard let session else {
        return operationResponseString(.success())
    }

    let instance = Unmanaged<XeroDictationSession>.fromOpaque(session).takeUnretainedValue()
    return operationResponseString(instance.stop())
}

@_cdecl("xero_dictation_cancel_session")
public func xeroDictationCancelSession(_ session: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? {
    guard let session else {
        return operationResponseString(.success())
    }

    let instance = Unmanaged<XeroDictationSession>.fromOpaque(session).takeUnretainedValue()
    return operationResponseString(instance.cancel())
}

@_cdecl("xero_dictation_release_session")
public func xeroDictationReleaseSession(_ session: UnsafeMutableRawPointer?) {
    guard let session else {
        return
    }

    Unmanaged<XeroDictationSession>.fromOpaque(session).release()
}

private func operationResponseString(_ response: XeroDictationOperationResponse) -> UnsafeMutablePointer<CChar>? {
    guard let data = try? JSONEncoder().encode(response),
          let json = String(data: data, encoding: .utf8) else {
        return duplicateCString(#"{"ok":false,"code":"native_response_encoding_failed","message":"Xero could not encode a native dictation response.","retryable":false}"#)
    }

    return duplicateCString(json)
}
