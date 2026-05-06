// FrameCapture.swift — ScreenCaptureKit frame capture.
//
// Captures the iOS Simulator window at the requested frame rate and
// delivers JPEG-encoded frames via the `onFrame` callback.

import Foundation
import ScreenCaptureKit
import CoreMedia
import CoreGraphics

@available(macOS 12.3, *)
class FrameCapture: NSObject, SCStreamOutput {

    let udid: String
    var onFrame: ((Data, Int, Int) -> Void)?
    var onError: ((String, String) -> Void)?

    private var stream: SCStream?
    private var isCapturing = false
    private let encoder = JpegEncoder()

    init(udid: String) {
        self.udid = udid
        super.init()
    }

    // MARK: - Start / Stop

    struct Dimensions {
        let width: Int
        let height: Int
    }

    func start(fps: Int, completion: @escaping (Result<Dimensions, HelperError>) -> Void) {
        fputs("[FrameCapture] start called, fps=\(fps)\n", stderr)
        guard !isCapturing else {
            fputs("[FrameCapture] already capturing\n", stderr)
            completion(.failure(.captureError("already capturing")))
            return
        }

        fputs("[FrameCapture] finding simulator window...\n", stderr)
        findSimulatorWindow { [weak self] result in
            guard let self = self else { return }
            switch result {
            case .failure(let err):
                completion(.failure(err))
            case .success(let window):
                self.startStream(window: window, fps: fps, completion: completion)
            }
        }
    }

    func stop() {
        guard isCapturing, let stream = stream else { return }
        isCapturing = false
        stream.stopCapture { error in
            if let error = error {
                fputs("stop capture error: \(error)\n", stderr)
            }
        }
        self.stream = nil
    }

    // MARK: - Window discovery

    private func findSimulatorWindow(
        completion: @escaping (Result<SCWindow, HelperError>) -> Void
    ) {
        fputs("[FrameCapture] calling SCShareableContent.get...\n", stderr)
        SCShareableContent.getExcludingDesktopWindows(true, onScreenWindowsOnly: false) { [weak self] content, error in
            fputs("[FrameCapture] SCShareableContent callback fired, error=\(error?.localizedDescription ?? "nil")\n", stderr)
            guard let self = self else { fputs("[FrameCapture] self is nil\n", stderr); return }

            if let error = error {
                let nsError = error as NSError
                // Code 1 typically means Screen Recording permission denied.
                if nsError.domain == "com.apple.ScreenCaptureKit.SCStreamErrorDomain" {
                    self.onError?("screen_recording_denied", "Screen Recording permission required")
                    completion(.failure(.captureError("screen_recording_denied")))
                    return
                }
                completion(.failure(.captureError(error.localizedDescription)))
                return
            }

            guard let content = content else {
                completion(.failure(.captureError("no shareable content")))
                return
            }

            // Find the Simulator app.
            let simApp = content.applications.first { app in
                app.bundleIdentifier == "com.apple.iphonesimulator"
            }

            guard let simApp = simApp else {
                completion(.failure(.captureError("Simulator.app not running")))
                return
            }

            // Find Simulator windows. Don't filter by isOnScreen — the
            // window may be minimized or hidden but still capturable.
            let simWindows = content.windows.filter { window in
                window.owningApplication?.bundleIdentifier == simApp.bundleIdentifier
            }

            fputs("[FrameCapture] found \(simWindows.count) Simulator window(s)\n", stderr)
            for (i, w) in simWindows.enumerated() {
                fputs("[FrameCapture]   [\(i)] \(w.frame.width)x\(w.frame.height) onScreen=\(w.isOnScreen) title=\(w.title ?? "nil")\n", stderr)
            }

            // Prefer the largest window (the device viewport).
            let window = simWindows.max(by: { a, b in
                (a.frame.width * a.frame.height) < (b.frame.width * b.frame.height)
            })

            guard let window = window else {
                completion(.failure(.captureError("no Simulator window found (\(simWindows.count) candidates, \(content.windows.count) total windows)")))
                return
            }
            fputs("[FrameCapture] selected window: \(window.frame.width)x\(window.frame.height)\n", stderr)

            completion(.success(window))
        }
    }

    // MARK: - Stream setup

    private func startStream(
        window: SCWindow,
        fps: Int,
        completion: @escaping (Result<Dimensions, HelperError>) -> Void
    ) {
        let filter = SCContentFilter(desktopIndependentWindow: window)
        let config = SCStreamConfiguration()
        config.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(fps))
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.showsCursor = false
        // Match the window's native resolution.
        config.width = Int(window.frame.width * 2)  // Retina
        config.height = Int(window.frame.height * 2)

        let newStream = SCStream(filter: filter, configuration: config, delegate: nil)
        do {
            try newStream.addStreamOutput(self, type: .screen, sampleHandlerQueue: .global(qos: .userInteractive))
        } catch {
            completion(.failure(.captureError("addStreamOutput: \(error)")))
            return
        }

        newStream.startCapture { [weak self] error in
            if let error = error {
                completion(.failure(.captureError("startCapture: \(error)")))
                return
            }
            self?.stream = newStream
            self?.isCapturing = true
            completion(.success(Dimensions(
                width: Int(window.frame.width * 2),
                height: Int(window.frame.height * 2)
            )))
        }
    }

    // MARK: - SCStreamOutput

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen, isCapturing else { return }
        guard let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }

        let width = CVPixelBufferGetWidth(pixelBuffer)
        let height = CVPixelBufferGetHeight(pixelBuffer)

        guard let jpeg = encoder.encode(pixelBuffer: pixelBuffer) else { return }
        onFrame?(jpeg, width, height)
    }
}

// Fallback for macOS < 12.3 — this file won't be reached at runtime
// because build.rs checks SDK version, but the compiler needs the type.
@available(macOS, obsoleted: 12.3, message: "ScreenCaptureKit unavailable")
class FrameCaptureFallback {
    init(udid: String) {}
    func start(fps: Int, completion: @escaping (Result<(width: Int, height: Int), HelperError>) -> Void) {
        completion(.failure(.captureError("ScreenCaptureKit requires macOS 12.3+")))
    }
    func stop() {}
}
