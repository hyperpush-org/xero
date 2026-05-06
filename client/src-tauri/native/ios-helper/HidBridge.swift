// HidBridge.swift — IndigoHID mach IPC for Simulator input injection.
//
// Communicates with the Simulator's Indigo HID mach service to inject
// touch, swipe, text, and button events. This is the same approach used
// by FBSimulatorControl (Facebook's open-source Simulator control library).
//
// The mach service name follows the pattern:
//   com.apple.CoreSimulator.SimDevice.<UDID>.IndigoHID
//
// When the mach port is unavailable (older Xcode, SIP restrictions),
// the caller falls back to AppleScript-based input via cg_input.rs.

import Foundation
import Darwin.Mach

// kNullPort is Int32 on macOS 26 but mach_port_t is UInt32.
private let kNullPort: mach_port_t = 0

// bootstrap_look_up is in bootstrap.h but not always bridged to Swift.
@_silgen_name("bootstrap_look_up")
private func _bootstrap_look_up(
    _ bp: mach_port_t,
    _ serviceName: UnsafePointer<CChar>,
    _ sp: UnsafeMutablePointer<mach_port_t>
) -> kern_return_t

class HidBridge {

    enum TouchPhase {
        case began, moved, ended, cancelled

        static func from(_ string: String) -> TouchPhase {
            switch string.lowercased() {
            case "began": return .began
            case "moved": return .moved
            case "ended": return .ended
            case "cancelled": return .cancelled
            default: return .began
            }
        }
    }

    let udid: String
    private var indigoPort: mach_port_t = kNullPort
    private var portLookupAttempted = false
    private let portLock = NSLock()

    init(udid: String) {
        self.udid = udid
    }

    // MARK: - Mach port bootstrap

    private func ensurePort() -> mach_port_t {
        portLock.lock()
        defer { portLock.unlock() }

        if indigoPort != kNullPort { return indigoPort }
        if portLookupAttempted { return kNullPort }

        portLookupAttempted = true
        let serviceName = "com.apple.CoreSimulator.SimDevice.\(udid).IndigoHID"

        var port: mach_port_t = kNullPort
        let kr = _bootstrap_look_up(bootstrap_port, serviceName, &port)

        if kr == KERN_SUCCESS {
            indigoPort = port
            fputs("IndigoHID mach port acquired for \(udid)\n", stderr)
        } else {
            fputs("IndigoHID mach port lookup failed (kr=\(kr)) for \(serviceName)\n", stderr)
        }

        return indigoPort
    }

    // MARK: - Touch injection

    func sendTouch(phase: TouchPhase, x: Int, y: Int, completion: @escaping (Result<Void, HelperError>) -> Void) {
        let port = ensurePort()
        guard port != kNullPort else {
            completion(.failure(.hidError("indigo_unavailable")))
            return
        }

        // Build IndigoHID touch event message.
        //
        // The IndigoHID mach message format (reverse-engineered from
        // FBSimulatorControl's FBSimulatorIndigoHID):
        //
        //   mach_msg_header_t (standard mach header)
        //   IndigoHIDEventHeader
        //     eventType: UInt32 = 2 (touch)
        //     pathAction: UInt32 (1=began, 2=moved, 3=ended, 4=cancelled)
        //     x: Float64
        //     y: Float64
        //     pathIndex: UInt32 = 1 (single finger)
        //
        // Rather than reproduce the full binary protocol (which varies
        // across Xcode versions), we use simctl's sendkey/input as a
        // reliable fallback and directly construct messages when the
        // mach port is available.
        //
        // For production use, this should be replaced with a proper
        // IndigoHID message builder that handles the specific Xcode
        // version's message layout.

        let result = sendIndigoTouchEvent(
            port: port,
            action: indigoAction(for: phase),
            x: Double(x),
            y: Double(y)
        )

        if result == KERN_SUCCESS {
            completion(.success(()))
        } else {
            completion(.failure(.hidError("indigo touch failed (kr=\(result))")))
        }
    }

    // MARK: - Swipe injection

    func sendSwipe(
        fromX: Int, fromY: Int,
        toX: Int, toY: Int,
        durationMs: Int,
        completion: @escaping (Result<Void, HelperError>) -> Void
    ) {
        let port = ensurePort()
        guard port != kNullPort else {
            completion(.failure(.hidError("indigo_unavailable")))
            return
        }

        // Interpolate touch events across the swipe duration.
        let steps = max(6, durationMs / 16) // ~60Hz
        let stepDelay = TimeInterval(durationMs) / 1000.0 / TimeInterval(steps)

        DispatchQueue.global(qos: .userInteractive).async { [weak self] in
            guard let self = self else { return }

            // Touch down at start.
            var kr = self.sendIndigoTouchEvent(port: port, action: 1, x: Double(fromX), y: Double(fromY))
            guard kr == KERN_SUCCESS else {
                completion(.failure(.hidError("swipe down failed (kr=\(kr))")))
                return
            }

            // Intermediate moves.
            for i in 1...steps {
                let t = Double(i) / Double(steps)
                let cx = Double(fromX) + t * Double(toX - fromX)
                let cy = Double(fromY) + t * Double(toY - fromY)
                Thread.sleep(forTimeInterval: stepDelay)
                kr = self.sendIndigoTouchEvent(port: port, action: 2, x: cx, y: cy)
                if kr != KERN_SUCCESS { break }
            }

            // Touch up at end.
            kr = self.sendIndigoTouchEvent(port: port, action: 3, x: Double(toX), y: Double(toY))
            if kr == KERN_SUCCESS {
                completion(.success(()))
            } else {
                completion(.failure(.hidError("swipe up failed (kr=\(kr))")))
            }
        }
    }

    // MARK: - Text injection

    func sendText(_ text: String, completion: @escaping (Result<Void, HelperError>) -> Void) {
        // Text input via simctl is more reliable than IndigoHID keyboard
        // events, which require keycode mapping per locale.
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/usr/bin/xcrun")
        proc.arguments = ["simctl", "io", udid, "type", text]
        proc.standardOutput = FileHandle.nullDevice
        proc.standardError = FileHandle.nullDevice

        do {
            try proc.run()
            proc.waitUntilExit()
            if proc.terminationStatus == 0 {
                completion(.success(()))
            } else {
                completion(.failure(.hidError("simctl type exit=\(proc.terminationStatus)")))
            }
        } catch {
            completion(.failure(.hidError("simctl type: \(error)")))
        }
    }

    // MARK: - Button injection

    func sendButton(_ button: String, completion: @escaping (Result<Void, HelperError>) -> Void) {
        let port = ensurePort()
        guard port != kNullPort else {
            // Fall back to simctl for buttons when IndigoHID unavailable.
            sendButtonViaSimctl(button, completion: completion)
            return
        }

        // IndigoHID button codes (from FBSimulatorControl):
        //   home=1, lock/side=2, volumeUp=3, volumeDown=4, siri=5
        let code: UInt32
        switch button.lowercased() {
        case "home": code = 1
        case "lock", "side_button", "power": code = 2
        case "volume_up", "vol_up": code = 3
        case "volume_down", "vol_down": code = 4
        case "siri": code = 5
        default:
            completion(.failure(.hidError("unknown button: \(button)")))
            return
        }

        let kr = sendIndigoButtonEvent(port: port, buttonCode: code)
        if kr == KERN_SUCCESS {
            completion(.success(()))
        } else {
            // Fall back to simctl on IndigoHID failure.
            sendButtonViaSimctl(button, completion: completion)
        }
    }

    // MARK: - Simctl fallback for buttons

    private func sendButtonViaSimctl(_ button: String, completion: @escaping (Result<Void, HelperError>) -> Void) {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/usr/bin/xcrun")

        switch button.lowercased() {
        case "home":
            proc.arguments = ["simctl", "io", udid, "sendkey", "home"]
        case "lock", "side_button", "power":
            proc.arguments = ["simctl", "io", udid, "sendkey", "lock"]
        case "volume_up", "vol_up":
            proc.arguments = ["simctl", "io", udid, "sendkey", "volumeUp"]
        case "volume_down", "vol_down":
            proc.arguments = ["simctl", "io", udid, "sendkey", "volumeDown"]
        case "siri":
            proc.arguments = ["simctl", "io", udid, "sendkey", "siri"]
        default:
            completion(.failure(.hidError("unknown button: \(button)")))
            return
        }

        proc.standardOutput = FileHandle.nullDevice
        proc.standardError = FileHandle.nullDevice

        do {
            try proc.run()
            proc.waitUntilExit()
            if proc.terminationStatus == 0 {
                completion(.success(()))
            } else {
                completion(.failure(.hidError("simctl sendkey exit=\(proc.terminationStatus)")))
            }
        } catch {
            completion(.failure(.hidError("simctl sendkey: \(error)")))
        }
    }

    // MARK: - IndigoHID mach message primitives

    /// Construct and send an IndigoHID touch event via mach IPC.
    /// Returns the mach kernel return code.
    ///
    /// action: 1=began, 2=moved, 3=ended, 4=cancelled
    private func sendIndigoTouchEvent(port: mach_port_t, action: UInt32, x: Double, y: Double) -> kern_return_t {
        // IndigoHID touch message layout (simplified, 64-byte body):
        //   [4B event_type=2][4B action][8B x][8B y][4B path_index=1][4B pressure=1.0]
        //   [32B padding/reserved]
        var body = Data(count: 64)
        body.withUnsafeMutableBytes { ptr in
            let buf = ptr.bindMemory(to: UInt8.self)
            // event_type = 2 (touch)
            var eventType: UInt32 = 2
            memcpy(buf.baseAddress!, &eventType, 4)
            // action
            var act = action
            memcpy(buf.baseAddress! + 4, &act, 4)
            // x coordinate
            var xVal = x
            memcpy(buf.baseAddress! + 8, &xVal, 8)
            // y coordinate
            var yVal = y
            memcpy(buf.baseAddress! + 16, &yVal, 8)
            // path index = 1
            var pathIndex: UInt32 = 1
            memcpy(buf.baseAddress! + 24, &pathIndex, 4)
            // pressure = 1.0
            var pressure: Float = 1.0
            memcpy(buf.baseAddress! + 28, &pressure, 4)
        }

        return sendMachMessage(port: port, body: body)
    }

    /// Construct and send an IndigoHID button event via mach IPC.
    private func sendIndigoButtonEvent(port: mach_port_t, buttonCode: UInt32) -> kern_return_t {
        // IndigoHID button message layout (simplified):
        //   [4B event_type=1][4B button_code]
        //   [56B padding/reserved]
        var body = Data(count: 64)
        body.withUnsafeMutableBytes { ptr in
            let buf = ptr.bindMemory(to: UInt8.self)
            // event_type = 1 (button)
            var eventType: UInt32 = 1
            memcpy(buf.baseAddress!, &eventType, 4)
            // button code
            var code = buttonCode
            memcpy(buf.baseAddress! + 4, &code, 4)
        }

        return sendMachMessage(port: port, body: body)
    }

    /// Send a raw mach message to the IndigoHID port.
    private func sendMachMessage(port: mach_port_t, body: Data) -> kern_return_t {
        // Total message: mach_msg_header_t (24 bytes) + body
        let headerSize = MemoryLayout<mach_msg_header_t>.size
        let totalSize = headerSize + body.count

        var msgBuf = Data(count: totalSize)
        return msgBuf.withUnsafeMutableBytes { ptr in
            let headerPtr = ptr.baseAddress!.assumingMemoryBound(to: mach_msg_header_t.self)
            headerPtr.pointee.msgh_bits = UInt32(MACH_MSG_TYPE_COPY_SEND)
            headerPtr.pointee.msgh_size = mach_msg_size_t(totalSize)
            headerPtr.pointee.msgh_remote_port = port
            headerPtr.pointee.msgh_local_port = kNullPort
            headerPtr.pointee.msgh_id = 0

            // Copy body after header.
            _ = body.withUnsafeBytes { bodyPtr in
                memcpy(ptr.baseAddress! + headerSize, bodyPtr.baseAddress!, body.count)
            }

            return mach_msg(
                headerPtr,
                MACH_SEND_MSG | MACH_SEND_TIMEOUT,
                mach_msg_size_t(totalSize),
                0,
                kNullPort,
                500,  // 500ms timeout
                kNullPort
            )
        }
    }

    // MARK: - Helpers

    private func indigoAction(for phase: TouchPhase) -> UInt32 {
        switch phase {
        case .began: return 1
        case .moved: return 2
        case .ended: return 3
        case .cancelled: return 4
        }
    }
}
