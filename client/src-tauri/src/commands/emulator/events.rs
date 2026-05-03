use serde::{Deserialize, Serialize};

pub const EMULATOR_FRAME_EVENT: &str = "emulator:frame";
pub const EMULATOR_STATUS_EVENT: &str = "emulator:status";
pub const EMULATOR_SDK_STATUS_CHANGED_EVENT: &str = "emulator:sdk_status_changed";

/// Emitted when the latest frame should be presented. The payload deliberately
/// omits the bytes — the frontend swaps `<img src="emulator://frame?t={seq}">`
/// and the URI scheme handler returns the JPEG bytes from the [`FrameBus`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FramePayload {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusPhase {
    Idle,
    Booting,
    Connecting,
    Streaming,
    Stopping,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub phase: StatusPhase,
    pub platform: Option<String>,
    pub device_id: Option<String>,
    pub message: Option<String>,
}

impl StatusPayload {
    pub fn new(phase: StatusPhase) -> Self {
        Self {
            phase,
            platform: None,
            device_id: None,
            message: None,
        }
    }

    pub fn with_platform(mut self, platform: impl Into<String>) -> Self {
        self.platform = Some(platform.into());
        self
    }

    pub fn with_device(mut self, id: impl Into<String>) -> Self {
        self.device_id = Some(id.into());
        self
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}
