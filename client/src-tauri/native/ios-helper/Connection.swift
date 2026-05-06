// Connection.swift — UDS server + binary framing protocol.
//
// Framing:
//   [1 byte type][4 bytes payload length BE][payload bytes]
//
// Types:
//   0x01 — JSON control message (UTF-8)
//   0x02 — Frame: payload = [4B width BE][4B height BE][JPEG bytes]

import Foundation

// MARK: - Error types

enum HelperError: Error, CustomStringConvertible {
    case invalidParams(String)
    case unknownMethod(String)
    case captureError(String)
    case hidError(String)
    case socketError(String)
    case timeout

    var description: String {
        switch self {
        case .invalidParams(let m): return "invalid_params: \(m)"
        case .unknownMethod(let m): return "unknown_method: \(m)"
        case .captureError(let m): return "capture_error: \(m)"
        case .hidError(let m): return "hid_error: \(m)"
        case .socketError(let m): return "socket_error: \(m)"
        case .timeout: return "timeout"
        }
    }

    var code: String {
        switch self {
        case .invalidParams: return "invalid_params"
        case .unknownMethod: return "unknown_method"
        case .captureError: return "capture_error"
        case .hidError: return "hid_error"
        case .socketError: return "socket_error"
        case .timeout: return "timeout"
        }
    }
}

// MARK: - Request/Response types

struct HelperRequest {
    let id: Int
    let method: String
    let params: [String: Any]?
}

typealias ResponseCallback = (Result<[String: Any], HelperError>) -> Void

// MARK: - Message types

private let msgTypeJSON: UInt8 = 0x01
private let msgTypeFrame: UInt8 = 0x02

// MARK: - Connection

class Connection {
    let socketPath: String
    var onRequest: ((HelperRequest, @escaping ResponseCallback) -> Void)?

    private var serverFd: Int32 = -1
    private var clientFd: Int32 = -1
    private let writeLock = NSLock()
    private var readThread: Thread?
    private var alive = true

    init(socketPath: String) {
        self.socketPath = socketPath
    }

    // MARK: - Server lifecycle

    /// Bind and listen on the UDS, then accept a client on a background
    /// thread. This does NOT block the main thread — RunLoop.main must
    /// be running for ScreenCaptureKit async callbacks to fire.
    func acceptAndServe() {
        // Remove stale socket file.
        unlink(socketPath)

        serverFd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard serverFd >= 0 else {
            fputs("failed to create socket: \(errno)\n", stderr)
            Foundation.exit(1)
        }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = socketPath.utf8CString
        guard pathBytes.count <= MemoryLayout.size(ofValue: addr.sun_path) else {
            fputs("socket path too long\n", stderr)
            Foundation.exit(1)
        }
        _ = withUnsafeMutablePointer(to: &addr.sun_path) { sunPath in
            pathBytes.withUnsafeBufferPointer { buf in
                memcpy(sunPath, buf.baseAddress!, buf.count)
            }
        }

        let bindResult = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockPtr in
                bind(serverFd, sockPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard bindResult == 0 else {
            fputs("failed to bind \(socketPath): \(errno)\n", stderr)
            Foundation.exit(1)
        }

        guard listen(serverFd, 1) == 0 else {
            fputs("failed to listen: \(errno)\n", stderr)
            Foundation.exit(1)
        }

        // Accept on a BACKGROUND thread so the main RunLoop stays free
        // for ScreenCaptureKit / GCD callbacks.
        let acceptThread = Thread {
            let fd = accept(self.serverFd, nil, nil)
            guard fd >= 0 else {
                fputs("accept failed: \(errno)\n", stderr)
                return
            }
            self.clientFd = fd

            // Start reader thread for incoming messages.
            self.readThread = Thread {
                self.readLoop()
            }
            self.readThread?.name = "xero-ios-helper-reader"
            self.readThread?.start()
        }
        acceptThread.name = "xero-ios-helper-accept"
        acceptThread.start()
    }

    func close() {
        alive = false
        if clientFd >= 0 { Darwin.close(clientFd); clientFd = -1 }
        if serverFd >= 0 { Darwin.close(serverFd); serverFd = -1 }
        unlink(socketPath)
    }

    // MARK: - Read loop

    private func readLoop() {
        while alive && clientFd >= 0 {
            guard let (msgType, payload) = readMessage() else {
                // Connection closed or error.
                alive = false
                break
            }

            if msgType == msgTypeJSON {
                handleJSONMessage(payload)
            }
            // Type 0x02 (frame) is outbound-only; ignore if received.
        }
    }

    private func handleJSONMessage(_ data: Data) {
        guard let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let id = json["id"] as? Int,
              let method = json["method"] as? String else {
            return
        }

        let params = json["params"] as? [String: Any]
        let request = HelperRequest(id: id, method: method, params: params)

        onRequest?(request) { [weak self] result in
            var response: [String: Any] = ["id": id]
            switch result {
            case .success(let payload):
                response.merge(payload) { _, new in new }
            case .failure(let error):
                response["error"] = ["code": error.code, "message": error.description]
            }
            self?.sendJSON(response)
        }
    }

    // MARK: - Write methods

    func sendFrame(jpeg: Data, width: Int, height: Int) {
        // Frame payload: [4B width BE][4B height BE][JPEG bytes]
        var payload = Data(capacity: 8 + jpeg.count)
        payload.appendBE(UInt32(width))
        payload.appendBE(UInt32(height))
        payload.append(jpeg)
        writeMessage(type: msgTypeFrame, payload: payload)
    }

    func sendJSON(_ object: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: object) else { return }
        writeMessage(type: msgTypeJSON, payload: data)
    }

    func sendEvent(code: String, message: String) {
        sendJSON(["event": "error", "code": code, "message": message])
    }

    // MARK: - Framing primitives

    private func writeMessage(type: UInt8, payload: Data) {
        writeLock.lock()
        defer { writeLock.unlock() }

        guard clientFd >= 0 else { return }

        // Header: [1B type][4B length BE]
        var header = Data(capacity: 5)
        header.append(type)
        header.appendBE(UInt32(payload.count))

        let headerOk = header.withUnsafeBytes { ptr in
            writeAll(fd: clientFd, ptr.baseAddress!, ptr.count)
        }
        guard headerOk else { return }

        payload.withUnsafeBytes { ptr in
            _ = writeAll(fd: clientFd, ptr.baseAddress!, ptr.count)
        }
    }

    private func readMessage() -> (UInt8, Data)? {
        // Read header: 1 byte type + 4 bytes length.
        guard let header = readExact(count: 5) else { return nil }
        let msgType = header[0]
        let length = header.readBE(at: 1) as UInt32

        guard length <= 50_000_000 else {
            // Sanity limit: 50MB max message.
            fputs("message too large: \(length)\n", stderr)
            return nil
        }

        guard let payload = readExact(count: Int(length)) else { return nil }
        return (msgType, payload)
    }

    private func readExact(count: Int) -> Data? {
        var buf = Data(count: count)
        var offset = 0
        while offset < count {
            let n = buf.withUnsafeMutableBytes { ptr in
                read(clientFd, ptr.baseAddress!.advanced(by: offset), count - offset)
            }
            if n <= 0 { return nil }
            offset += n
        }
        return buf
    }

    @discardableResult
    private func writeAll(fd: Int32, _ ptr: UnsafeRawPointer, _ count: Int) -> Bool {
        var offset = 0
        while offset < count {
            let n = write(fd, ptr.advanced(by: offset), count - offset)
            if n <= 0 {
                if errno == EAGAIN || errno == EWOULDBLOCK { continue }
                return false
            }
            offset += n
        }
        return true
    }
}

// MARK: - Data extensions for big-endian encoding

private extension Data {
    mutating func appendBE(_ value: UInt32) {
        var be = value.bigEndian
        Swift.withUnsafeBytes(of: &be) { ptr in
            self.append(contentsOf: ptr)
        }
    }

    func readBE(at offset: Int) -> UInt32 {
        guard self.count >= offset + 4 else { return 0 }
        let b0 = self[self.startIndex + offset]
        let b1 = self[self.startIndex + offset + 1]
        let b2 = self[self.startIndex + offset + 2]
        let b3 = self[self.startIndex + offset + 3]
        return (UInt32(b0) << 24) | (UInt32(b1) << 16) | (UInt32(b2) << 8) | UInt32(b3)
    }
}
