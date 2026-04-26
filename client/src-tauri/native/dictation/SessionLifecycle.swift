import Darwin
import Dispatch
import Foundation

public typealias CadenceDictationEventCallback = @convention(c) (
    UnsafeMutableRawPointer?,
    UnsafePointer<CChar>?
) -> Void

private struct CadenceDictationSessionRequest: Decodable {
    let sessionId: String
    let engine: String
    let locale: String
    let privacyMode: String?
    let contextualPhrases: [String]?
}

struct CadenceDictationOperationResponse: Encodable {
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

private final class CadenceDictationSession {
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
    private let callback: CadenceDictationEventCallback?
    private let context: UnsafeMutableRawPointer?
    private let queue: DispatchQueue
    private let callbackQueue: DispatchQueue
    private var state: LifecycleState = .created
    private var modernEngine: CadenceModernDictationEngine?

    init(request: CadenceDictationSessionRequest, callback: CadenceDictationEventCallback?, context: UnsafeMutableRawPointer?) {
        self.sessionId = request.sessionId
        self.engine = request.engine
        self.locale = request.locale
        self.privacyMode = request.privacyMode ?? "on_device_preferred"
        self.contextualPhrases = request.contextualPhrases ?? []
        self.callback = callback
        self.context = context
        self.queue = DispatchQueue(label: "dev.cadence.dictation.session.\(request.sessionId)")
        self.callbackQueue = DispatchQueue(label: "dev.cadence.dictation.session.\(request.sessionId).events")
    }

    deinit {
        if let modernEngine {
            _ = modernEngine.cancel()
        }
    }

    func start() -> CadenceDictationOperationResponse {
        queue.sync {
            switch state {
            case .created:
                if engine == "modern" {
                    if #available(macOS 26.0, *) {
                        let modernEngine = CadenceModernDictationEngine(
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
                        message: "Cadence could not start modern dictation because macOS 26 SpeechAnalyzer is unavailable.",
                        retryable: false
                    )
                } else {
                    state = .started
                    emit([
                        "kind": "permission",
                        "microphone": microphonePermissionState(),
                        "speech": speechPermissionState(),
                    ])
                    emit([
                        "kind": "started",
                        "sessionId": sessionId,
                        "engine": engine,
                        "locale": locale,
                    ])
                }
            case .started:
                break
            case .stopped:
                return .failure(
                    code: "dictation_session_stopped",
                    message: "Cadence cannot start a dictation session after it has already stopped."
                )
            }

            return .success(sessionId: sessionId, engine: engine, locale: locale)
        }
    }

    func stop() -> CadenceDictationOperationResponse {
        if engine == "modern" {
            return endModern(reason: "user")
        }

        return end(reason: "user")
    }

    func cancel() -> CadenceDictationOperationResponse {
        if engine == "modern" {
            return endModern(reason: "cancelled")
        }

        return end(reason: "cancelled")
    }

    private func endModern(reason: String) -> CadenceDictationOperationResponse {
        queue.sync {
            switch state {
            case .created, .started:
                state = .stopped
                let response: CadenceDictationOperationResponse
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

    private func end(reason: String) -> CadenceDictationOperationResponse {
        queue.sync {
            switch state {
            case .created, .started:
                state = .stopped
                emit([
                    "kind": "stopped",
                    "sessionId": sessionId,
                    "reason": reason,
                ])
            case .stopped:
                break
            }

            return .success()
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
                let fallback = #"{"kind":"error","sessionId":null,"code":"native_event_encoding_failed","message":"Cadence could not encode a native dictation event.","retryable":false}"#
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

@_cdecl("cadence_dictation_create_session")
public func cadenceDictationCreateSession(
    _ requestJson: UnsafePointer<CChar>?,
    _ callback: CadenceDictationEventCallback?,
    _ context: UnsafeMutableRawPointer?
) -> UnsafeMutableRawPointer? {
    guard let requestJson else {
        return nil
    }

    let json = String(cString: requestJson)
    guard let data = json.data(using: .utf8),
          let request = try? JSONDecoder().decode(CadenceDictationSessionRequest.self, from: data) else {
        return nil
    }

    let session = CadenceDictationSession(request: request, callback: callback, context: context)
    return Unmanaged.passRetained(session).toOpaque()
}

@_cdecl("cadence_dictation_start_session")
public func cadenceDictationStartSession(_ session: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? {
    guard let session else {
        return operationResponseString(.failure(
            code: "native_session_missing",
            message: "Cadence could not start dictation because the native session was unavailable."
        ))
    }

    let instance = Unmanaged<CadenceDictationSession>.fromOpaque(session).takeUnretainedValue()
    return operationResponseString(instance.start())
}

@_cdecl("cadence_dictation_stop_session")
public func cadenceDictationStopSession(_ session: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? {
    guard let session else {
        return operationResponseString(.success())
    }

    let instance = Unmanaged<CadenceDictationSession>.fromOpaque(session).takeUnretainedValue()
    return operationResponseString(instance.stop())
}

@_cdecl("cadence_dictation_cancel_session")
public func cadenceDictationCancelSession(_ session: UnsafeMutableRawPointer?) -> UnsafeMutablePointer<CChar>? {
    guard let session else {
        return operationResponseString(.success())
    }

    let instance = Unmanaged<CadenceDictationSession>.fromOpaque(session).takeUnretainedValue()
    return operationResponseString(instance.cancel())
}

@_cdecl("cadence_dictation_release_session")
public func cadenceDictationReleaseSession(_ session: UnsafeMutableRawPointer?) {
    guard let session else {
        return
    }

    Unmanaged<CadenceDictationSession>.fromOpaque(session).release()
}

private func operationResponseString(_ response: CadenceDictationOperationResponse) -> UnsafeMutablePointer<CChar>? {
    guard let data = try? JSONEncoder().encode(response),
          let json = String(data: data, encoding: .utf8) else {
        return duplicateCString(#"{"ok":false,"code":"native_response_encoding_failed","message":"Cadence could not encode a native dictation response.","retryable":false}"#)
    }

    return duplicateCString(json)
}
