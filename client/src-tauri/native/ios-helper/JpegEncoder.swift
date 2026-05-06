// JpegEncoder.swift — CVPixelBuffer → JPEG encoding via ImageIO.
//
// Uses CoreGraphics and ImageIO for hardware-accelerated JPEG encoding.
// Quality 0.8 matches the existing JPEG_QUALITY = 80 constant in
// the Rust codec.rs module.

import Foundation
import CoreGraphics
import CoreVideo
import ImageIO
import UniformTypeIdentifiers

class JpegEncoder {

    let quality: Float

    init(quality: Float = 0.8) {
        self.quality = quality
    }

    /// Encode a CVPixelBuffer to JPEG data.
    /// Returns nil if encoding fails.
    func encode(pixelBuffer: CVPixelBuffer) -> Data? {
        CVPixelBufferLockBaseAddress(pixelBuffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(pixelBuffer, .readOnly) }

        let width = CVPixelBufferGetWidth(pixelBuffer)
        let height = CVPixelBufferGetHeight(pixelBuffer)
        let bytesPerRow = CVPixelBufferGetBytesPerRow(pixelBuffer)

        guard let baseAddress = CVPixelBufferGetBaseAddress(pixelBuffer) else { return nil }

        // Create CGImage from the pixel buffer (BGRA format).
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        guard let context = CGContext(
            data: baseAddress,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: bytesPerRow,
            space: colorSpace,
            bitmapInfo: CGBitmapInfo.byteOrder32Little.rawValue | CGImageAlphaInfo.premultipliedFirst.rawValue
        ) else { return nil }

        guard let cgImage = context.makeImage() else { return nil }
        return encodeImage(cgImage)
    }

    /// Encode a CGImage to JPEG data.
    func encodeImage(_ image: CGImage) -> Data? {
        let data = NSMutableData()
        let typeIdentifier: CFString
        if #available(macOS 11.0, *) {
            typeIdentifier = UTType.jpeg.identifier as CFString
        } else {
            typeIdentifier = kUTTypeJPEG
        }

        guard let dest = CGImageDestinationCreateWithData(data, typeIdentifier, 1, nil) else {
            return nil
        }

        let options: [CFString: Any] = [
            kCGImageDestinationLossyCompressionQuality: quality,
        ]
        CGImageDestinationAddImage(dest, image, options as CFDictionary)

        guard CGImageDestinationFinalize(dest) else { return nil }
        return data as Data
    }
}
