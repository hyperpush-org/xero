// Main.swift — xero-ios-helper entry point.
//
// A macOS daemon that captures the iOS Simulator window via
// ScreenCaptureKit and injects HID events via IndigoHID mach IPC.
// Communicates with the Xero Tauri backend over a Unix domain socket
// using a simple binary framing protocol.
//
// Usage:
//   xero-ios-helper --udid <UDID> --socket-path <PATH>

import Foundation

@main
struct XeroIosHelper {
    static func main() {
        let parsed = parseArgs()
        installSignalHandler()

        let connection = Connection(socketPath: parsed.socketPath)
        let frameCapture = FrameCapture(udid: parsed.udid)
        let hidBridge = HidBridge(udid: parsed.udid)
        let axBridge = AccessibilityBridge()

        // Wire frame capture output to the connection.
        frameCapture.onFrame = { [weak connection] jpeg, width, height in
            connection?.sendFrame(jpeg: jpeg, width: width, height: height)
        }

        frameCapture.onError = { [weak connection] code, message in
            connection?.sendEvent(code: code, message: message)
        }

        // Wire incoming requests to the appropriate handler.
        connection.onRequest = { request, respond in
            switch request.method {
            case "ping":
                respond(.success(["ok": true]))

            case "start_capture":
                let fps = (request.params?["fps"] as? Int) ?? 30
                fputs("[helper] start_capture request received, fps=\(fps)\n", stderr)
                frameCapture.start(fps: fps) { result in
                    fputs("[helper] start_capture completion: \(result)\n", stderr)
                    switch result {
                    case .success(let dims):
                        respond(.success([
                            "ok": true,
                            "width": dims.width,
                            "height": dims.height,
                        ]))
                    case .failure(let err):
                        respond(.failure(err))
                    }
                }

            case "stop_capture":
                frameCapture.stop()
                respond(.success(["ok": true]))

            case "hid_touch":
                guard let params = request.params,
                      let phaseStr = params["phase"] as? String,
                      let x = params["x"] as? Int,
                      let y = params["y"] as? Int else {
                    respond(.failure(HelperError.invalidParams("hid_touch requires phase, x, y")))
                    return
                }
                let phase = HidBridge.TouchPhase.from(phaseStr)
                hidBridge.sendTouch(phase: phase, x: x, y: y) { result in
                    respond(result.map { ["ok": true] as [String: Any] })
                }

            case "hid_swipe":
                guard let params = request.params,
                      let fromX = params["from_x"] as? Int,
                      let fromY = params["from_y"] as? Int,
                      let toX = params["to_x"] as? Int,
                      let toY = params["to_y"] as? Int else {
                    respond(.failure(HelperError.invalidParams("hid_swipe requires from_x/y, to_x/y")))
                    return
                }
                let durationMs = (params["duration_ms"] as? Int) ?? 300
                hidBridge.sendSwipe(fromX: fromX, fromY: fromY, toX: toX, toY: toY, durationMs: durationMs) { result in
                    respond(result.map { ["ok": true] as [String: Any] })
                }

            case "hid_text":
                guard let params = request.params,
                      let text = params["text"] as? String else {
                    respond(.failure(HelperError.invalidParams("hid_text requires text")))
                    return
                }
                hidBridge.sendText(text) { result in
                    respond(result.map { ["ok": true] as [String: Any] })
                }

            case "hid_button":
                guard let params = request.params,
                      let button = params["button"] as? String else {
                    respond(.failure(HelperError.invalidParams("hid_button requires button")))
                    return
                }
                hidBridge.sendButton(button) { result in
                    respond(result.map { ["ok": true] as [String: Any] })
                }

            case "accessibility_tree":
                let result = axBridge.dumpTree()
                switch result {
                case .success(let tree):
                    respond(.success(["ok": true, "tree": tree]))
                case .failure(let err):
                    respond(.failure(err))
                }

            case "accessibility_element_at":
                guard let params = request.params,
                      let x = (params["x"] as? NSNumber)?.floatValue,
                      let y = (params["y"] as? NSNumber)?.floatValue else {
                    respond(.failure(HelperError.invalidParams("accessibility_element_at requires x, y")))
                    return
                }
                let result = axBridge.elementAtPoint(x: x, y: y)
                switch result {
                case .success(let element):
                    respond(.success(["ok": true, "element": element]))
                case .failure(let err):
                    respond(.failure(err))
                }

            default:
                respond(.failure(HelperError.unknownMethod(request.method)))
            }
        }

        // Start accepting connections on a background thread.
        // acceptAndServe() is non-blocking — it binds/listens synchronously
        // then spawns a thread for accept(). This way RunLoop.main starts
        // immediately, which is required for ScreenCaptureKit async callbacks.
        connection.acceptAndServe()

        // Run the main RunLoop — required for ScreenCaptureKit, GCD, and
        // any other framework that dispatches to the main queue.
        RunLoop.main.run()

        // Cleanup on exit.
        frameCapture.stop()
        connection.close()
        Foundation.exit(0)
    }
}

// MARK: - Argument parsing

private func parseArgs() -> (udid: String, socketPath: String) {
    let args = CommandLine.arguments
    var udid: String?
    var socketPath: String?

    var i = 1
    while i < args.count {
        switch args[i] {
        case "--udid" where i + 1 < args.count:
            i += 1
            udid = args[i]
        case "--socket-path" where i + 1 < args.count:
            i += 1
            socketPath = args[i]
        default:
            break
        }
        i += 1
    }

    guard let u = udid, let s = socketPath else {
        fputs("usage: xero-ios-helper --udid <UDID> --socket-path <PATH>\n", stderr)
        Foundation.exit(1)
    }
    return (u, s)
}

// MARK: - Signal handling

private var shutdownRequested = false

private func installSignalHandler() {
    signal(SIGTERM) { _ in
        shutdownRequested = true
        CFRunLoopStop(CFRunLoopGetMain())
    }
    signal(SIGINT) { _ in
        shutdownRequested = true
        CFRunLoopStop(CFRunLoopGetMain())
    }
}
