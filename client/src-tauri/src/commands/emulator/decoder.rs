//! H.264 → RGBA decoder abstraction.
//!
//! Both platform pipelines (scrcpy on Android, idb on iOS) receive H.264 NAL
//! units over the wire. They hand the bytes to a [`H264Decoder`], receive
//! RGBA, and then [`super::codec::encode_jpeg_rgba`] turns the result into
//! the JPEG frames the webview consumes through `emulator_frame`.
//!
//! The actual decoder implementation lives behind the `emulator-live` Cargo
//! feature so Xero's default build is independent of the `openh264-sys2`
//! C++ toolchain. Without that feature the decoder returns a clear error
//! that the sidecar pipelines surface as an `emulator:status` error — the
//! rest of the pipeline (process spawn, ADB push, socket plumbing, control
//! messages, logs) stays fully exercised.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("decoder unavailable: Xero was built without the `emulator-live` feature")]
    Unavailable,
    #[error("h264 decode failed: {0}")]
    Decode(String),
    #[error("decoder produced a frame with unexpected dimensions ({got_w}x{got_h})")]
    BadDimensions { got_w: u32, got_h: u32 },
}

/// A decoded frame ready for JPEG re-encoding.
#[derive(Debug)]
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    /// Tightly packed `RGBA8` — one byte per channel, row-major.
    pub rgba: Vec<u8>,
}

/// Object-safe decoder trait. Implementations are stateful (H.264 is a
/// temporal codec — keyframes set reference frames for subsequent deltas),
/// so the pipeline must feed NALs into the same instance for the whole
/// session.
pub trait H264Decoder: Send {
    /// Push one or more concatenated Annex-B NAL units. Returns the decoded
    /// frame if this NAL completes one; returns `Ok(None)` for SPS/PPS or
    /// partial access units.
    fn decode(&mut self, nal: &[u8]) -> Result<Option<DecodedFrame>, DecodeError>;

    /// Implementation name for diagnostics.
    fn name(&self) -> &'static str;
}

/// Construct the default decoder for this build. Will be the `openh264`-backed
/// decoder when `--features emulator-live` is on, or an always-erroring stub
/// otherwise.
pub fn new_default_decoder() -> Box<dyn H264Decoder> {
    #[cfg(feature = "emulator-live")]
    {
        match openh264_impl::OpenH264Decoder::new() {
            Ok(decoder) => Box::new(decoder),
            Err(err) => Box::new(UnavailableDecoder {
                reason: format!("openh264 init failed: {err}"),
            }),
        }
    }
    #[cfg(not(feature = "emulator-live"))]
    {
        Box::new(UnavailableDecoder {
            reason: "emulator-live feature disabled at build time".to_string(),
        })
    }
}

struct UnavailableDecoder {
    reason: String,
}

impl H264Decoder for UnavailableDecoder {
    fn decode(&mut self, _nal: &[u8]) -> Result<Option<DecodedFrame>, DecodeError> {
        let _ = &self.reason;
        Err(DecodeError::Unavailable)
    }

    fn name(&self) -> &'static str {
        "unavailable"
    }
}

#[cfg(feature = "emulator-live")]
mod openh264_impl {
    use super::{DecodeError, DecodedFrame, H264Decoder};
    use openh264::decoder::Decoder;
    use openh264::formats::YUVSource;

    pub struct OpenH264Decoder {
        inner: Decoder,
    }

    impl OpenH264Decoder {
        pub fn new() -> Result<Self, String> {
            let decoder = Decoder::new().map_err(|e| e.to_string())?;
            Ok(Self { inner: decoder })
        }
    }

    impl H264Decoder for OpenH264Decoder {
        fn decode(&mut self, nal: &[u8]) -> Result<Option<DecodedFrame>, DecodeError> {
            let decoded = self
                .inner
                .decode(nal)
                .map_err(|e| DecodeError::Decode(e.to_string()))?;
            let Some(yuv) = decoded else { return Ok(None) };

            let (width, height) = yuv.dimensions();
            let width_u32 = width as u32;
            let height_u32 = height as u32;
            let mut rgba = vec![0u8; width * height * 4];
            yuv.write_rgba8(&mut rgba);
            Ok(Some(DecodedFrame {
                width: width_u32,
                height: height_u32,
                rgba,
            }))
        }

        fn name(&self) -> &'static str {
            "openh264"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_decoder_without_feature_reports_unavailable() {
        #[cfg(not(feature = "emulator-live"))]
        {
            let mut decoder = new_default_decoder();
            assert_eq!(decoder.name(), "unavailable");
            match decoder.decode(&[0, 0, 0, 1, 0x67]) {
                Err(DecodeError::Unavailable) => {}
                other => panic!("expected Unavailable, got {other:?}"),
            }
        }
        #[cfg(feature = "emulator-live")]
        {
            let decoder = new_default_decoder();
            assert_eq!(decoder.name(), "openh264");
        }
    }
}
