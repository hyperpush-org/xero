use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub const DESKTOP_SIDECAR_SCHEMA_VERSION: u32 = 1;
pub const DESKTOP_SIDECAR_PROTOCOL: &str = "xero.desktop_sidecar.ipc.v1";
pub const DESKTOP_SIDECAR_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarHandshake {
    pub schema_version: u32,
    pub protocol: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub token_sha256: String,
    pub allowed_operations: BTreeSet<DesktopSidecarOperation>,
    pub expires_at: String,
}

impl DesktopSidecarHandshake {
    pub fn into_lease(self) -> DesktopSidecarLease {
        DesktopSidecarLease {
            session_id: self.session_id,
            run_id: self.run_id,
            token_sha256: self.token_sha256,
            allowed_operations: self.allowed_operations,
            expires_at: self.expires_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarLease {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub token_sha256: String,
    pub allowed_operations: BTreeSet<DesktopSidecarOperation>,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarRequest {
    pub schema_version: u32,
    pub protocol: String,
    pub request_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub actor: DesktopSidecarActor,
    pub operation: DesktopSidecarOperation,
    #[serde(default)]
    pub payload: JsonValue,
    pub policy_decision_id: String,
    pub auth: DesktopSidecarAuth,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarAuth {
    pub scheme: DesktopSidecarAuthScheme,
    pub token: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSidecarAuthScheme {
    BearerSessionToken,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSidecarActor {
    Agent,
    LocalUser,
    CloudManualControl,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSidecarOperation {
    Health,
    Capabilities,
    PermissionsStatus,
    DisplayList,
    WindowList,
    AppList,
    ForegroundState,
    Screenshot,
    CursorState,
    AccessibilitySnapshot,
    OcrSnapshot,
    ElementAtPoint,
    MouseDown,
    MouseMove,
    MouseClick,
    MouseDoubleClick,
    MouseRightClick,
    MouseDrag,
    MouseDragMove,
    MouseUp,
    Scroll,
    KeyPress,
    Hotkey,
    TypeText,
    PasteText,
    FocusWindow,
    ActivateApp,
    LaunchApp,
    QuitApp,
    AxPress,
    AxSetValue,
    AxFocus,
    MenuSelect,
    CancelCurrentAction,
    StreamCapabilities,
    StreamStart,
    StreamOffer,
    StreamAnswer,
    StreamIceCandidate,
    StreamStop,
    StreamStatus,
    StreamSetQuality,
    StreamRequestKeyframe,
}

impl DesktopSidecarOperation {
    pub fn all_contract_operations() -> BTreeSet<Self> {
        use DesktopSidecarOperation::*;
        [
            Health,
            Capabilities,
            PermissionsStatus,
            DisplayList,
            WindowList,
            AppList,
            ForegroundState,
            Screenshot,
            CursorState,
            AccessibilitySnapshot,
            OcrSnapshot,
            ElementAtPoint,
            MouseDown,
            MouseMove,
            MouseClick,
            MouseDoubleClick,
            MouseRightClick,
            MouseDrag,
            MouseDragMove,
            MouseUp,
            Scroll,
            KeyPress,
            Hotkey,
            TypeText,
            PasteText,
            FocusWindow,
            ActivateApp,
            LaunchApp,
            QuitApp,
            AxPress,
            AxSetValue,
            AxFocus,
            MenuSelect,
            CancelCurrentAction,
            StreamCapabilities,
            StreamStart,
            StreamOffer,
            StreamAnswer,
            StreamIceCandidate,
            StreamStop,
            StreamStatus,
            StreamSetQuality,
            StreamRequestKeyframe,
        ]
        .into_iter()
        .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarCapabilities {
    pub platform: String,
    pub schema_version: u32,
    pub display_list: bool,
    pub screenshot: bool,
    pub window_list: bool,
    pub app_list: bool,
    pub foreground_state: bool,
    pub cursor_state: bool,
    pub accessibility_snapshot: bool,
    pub ocr_snapshot: bool,
    pub mouse_input: bool,
    pub keyboard_input: bool,
    pub clipboard: bool,
    #[serde(default)]
    pub window_focus: bool,
    #[serde(default)]
    pub app_control: bool,
    pub accessibility_actions: bool,
    pub menu_select: bool,
    pub webrtc_stream: bool,
    pub screenshot_fallback_stream: bool,
    pub manual_cloud_control: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarPermissionsPayload {
    pub permissions: Vec<DesktopSidecarPermissionStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarPermissionStatus {
    pub name: String,
    pub status: DesktopSidecarPermissionGrant,
    pub required_for: Vec<String>,
    pub remediation: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSidecarPermissionGrant {
    Granted,
    Denied,
    Unknown,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarDisplay {
    pub display_id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub rotation: f32,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarDisplayListPayload {
    pub displays: Vec<DesktopSidecarDisplay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarWindow {
    pub window_id: String,
    pub app_name: String,
    pub title: String,
    pub pid: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub z: i32,
    pub focused: bool,
    pub minimized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarWindowListPayload {
    pub windows: Vec<DesktopSidecarWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarApp {
    pub app_name: String,
    pub pid: u32,
    pub window_count: usize,
    pub focused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarAppListPayload {
    pub apps: Vec<DesktopSidecarApp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarForegroundStatePayload {
    pub foreground: Option<DesktopSidecarWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarCursorStatePayload {
    pub x: i32,
    pub y: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_id: Option<String>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarPointRequest {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarAccessibilityElement {
    pub element_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarElementAtPointPayload {
    pub x: i32,
    pub y: i32,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<DesktopSidecarAccessibilityElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarAccessibilitySnapshotRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_id: Option<String>,
    #[serde(default)]
    pub focused_only: bool,
    #[serde(default = "default_accessibility_include_children")]
    pub include_children: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

fn default_accessibility_include_children() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarAccessibilitySnapshotTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarAccessibilitySnapshotRow {
    pub row_type: String,
    pub depth: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub element: DesktopSidecarAccessibilityElement,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarAccessibilitySnapshotPayload {
    pub performed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<DesktopSidecarAccessibilitySnapshotTarget>,
    pub rows: Vec<DesktopSidecarAccessibilitySnapshotRow>,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarScreenshotRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<DesktopSidecarRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarScreenshotPayload {
    pub media_type: String,
    pub bytes_base64: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub captured_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarOcrSnapshotRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<DesktopSidecarRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarOcrTextBlock {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarOcrSnapshotPayload {
    pub performed: bool,
    pub captured_at: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub text_blocks: Vec<DesktopSidecarOcrTextBlock>,
    pub full_text: String,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSidecarStreamQuality {
    Low,
    Balanced,
    High,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSidecarStreamStatus {
    Idle,
    Starting,
    Live,
    Degraded,
    Paused,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSidecarStreamTransport {
    WebRtc,
    ScreenshotFallback,
    Unavailable,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarStreamRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_frame_rate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_cursor: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<DesktopSidecarStreamQuality>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ice_servers: Vec<DesktopSidecarIceServer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_description: Option<DesktopSidecarSessionDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ice_candidate: Option<DesktopSidecarIceCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarIceServer {
    pub urls: DesktopSidecarIceServerUrls,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum DesktopSidecarIceServerUrls {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarSessionDescription {
    #[serde(rename = "type")]
    pub sdp_type: String,
    pub sdp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarIceCandidate {
    pub candidate: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdp_mid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdp_m_line_index: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username_fragment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarStreamPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_id: Option<String>,
    pub status: DesktopSidecarStreamStatus,
    pub transport: DesktopSidecarStreamTransport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signaling_channel: Option<String>,
    pub quality: DesktopSidecarStreamQuality,
    pub max_width: u32,
    pub max_frame_rate: u32,
    pub include_cursor: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_description: Option<DesktopSidecarSessionDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ice_candidate: Option<DesktopSidecarIceCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<DesktopSidecarStreamMetrics>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarStreamMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder_backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder_hardware: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_codec: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_frame_rate: Option<u32>,
    #[serde(default)]
    pub capture_dropped_frames: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode_frame_rate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode_latency_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outbound_bitrate_bps: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_outgoing_bitrate_bps: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packets_sent: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_sent: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packet_loss: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round_trip_time_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retransmits: Option<u64>,
    #[serde(default)]
    pub keyframes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarStreamCapabilitiesPayload {
    pub webrtc_stream: bool,
    pub screenshot_fallback_stream: bool,
    pub native_video_track: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_codec: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capture_backends: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub encoder_backends: Vec<String>,
    pub hardware_encoding: bool,
    pub supported_qualities: Vec<DesktopSidecarStreamQuality>,
    pub max_width: u32,
    pub max_frame_rate: u32,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopSidecarMouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarControlRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_y: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_y: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<DesktopSidecarMouseButton>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clicks: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menu_path: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarResponse {
    pub schema_version: u32,
    pub protocol: String,
    pub request_id: String,
    pub operation: DesktopSidecarOperation,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<DesktopSidecarErrorBody>,
}

impl DesktopSidecarResponse {
    pub fn ok(
        request_id: impl Into<String>,
        operation: DesktopSidecarOperation,
        result: JsonValue,
    ) -> Self {
        Self {
            schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
            protocol: DESKTOP_SIDECAR_PROTOCOL.into(),
            request_id: request_id.into(),
            operation,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(
        request_id: impl Into<String>,
        operation: DesktopSidecarOperation,
        error: DesktopSidecarErrorBody,
    ) -> Self {
        Self {
            schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
            protocol: DESKTOP_SIDECAR_PROTOCOL.into(),
            request_id: request_id.into(),
            operation,
            ok: false,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopSidecarErrorBody {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub user_action_required: bool,
}

impl DesktopSidecarErrorBody {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        user_action_required: bool,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            user_action_required,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DesktopSidecarContractError {
    #[error("sidecar schema version {actual} does not match expected {expected}")]
    SchemaVersionMismatch { expected: u32, actual: u32 },
    #[error("sidecar protocol `{actual}` does not match expected `{expected}`")]
    ProtocolMismatch { expected: String, actual: String },
    #[error("sidecar request field `{field}` is empty")]
    EmptyField { field: &'static str },
    #[error("sidecar session does not match the active lease")]
    SessionMismatch,
    #[error("sidecar run does not match the active lease")]
    RunMismatch,
    #[error("sidecar request token is missing or invalid")]
    InvalidToken,
    #[error("sidecar session lease has expired")]
    LeaseExpired,
    #[error("sidecar request has expired")]
    RequestExpired,
    #[error("sidecar request expires after the active lease")]
    RequestOutlivesLease,
    #[error("sidecar operation `{operation:?}` is not allowed by the active lease")]
    OperationNotAllowed { operation: DesktopSidecarOperation },
    #[error("sidecar request payload is too large")]
    PayloadTooLarge,
    #[error("sidecar request payload contains forbidden key `{key}`")]
    ForbiddenPayloadKey { key: String },
    #[error("sidecar timestamp `{field}` is invalid")]
    InvalidTimestamp { field: &'static str },
    #[error("sidecar response request `{actual}` does not match expected `{expected}`")]
    ResponseRequestMismatch { expected: String, actual: String },
    #[error("sidecar response operation `{actual:?}` does not match expected `{expected:?}`")]
    ResponseOperationMismatch {
        expected: DesktopSidecarOperation,
        actual: DesktopSidecarOperation,
    },
    #[error("successful sidecar response is missing a result body")]
    ResponseMissingResult,
    #[error("failed sidecar response is missing an error body")]
    ResponseMissingError,
    #[error("sidecar response has both result and error bodies")]
    ResponseConflictingBody,
}

impl DesktopSidecarContractError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::SchemaVersionMismatch { .. } => "sidecar_version_mismatch",
            Self::ProtocolMismatch { .. } => "sidecar_protocol_mismatch",
            Self::EmptyField { .. } => "sidecar_schema_invalid",
            Self::SessionMismatch | Self::RunMismatch => "sidecar_session_mismatch",
            Self::InvalidToken => "sidecar_auth_failed",
            Self::LeaseExpired => "sidecar_lease_expired",
            Self::RequestExpired => "sidecar_request_expired",
            Self::RequestOutlivesLease => "sidecar_request_outlives_lease",
            Self::OperationNotAllowed { .. } => "sidecar_operation_not_allowed",
            Self::PayloadTooLarge => "sidecar_payload_too_large",
            Self::ForbiddenPayloadKey { .. } => "sidecar_forbidden_payload",
            Self::InvalidTimestamp { .. } => "sidecar_timestamp_invalid",
            Self::ResponseRequestMismatch { .. }
            | Self::ResponseOperationMismatch { .. }
            | Self::ResponseMissingResult
            | Self::ResponseMissingError
            | Self::ResponseConflictingBody => "sidecar_response_invalid",
        }
    }

    pub fn to_error_body(&self) -> DesktopSidecarErrorBody {
        DesktopSidecarErrorBody::new(self.code(), self.to_string(), false, false)
    }
}

pub fn validate_sidecar_handshake(
    handshake: &DesktopSidecarHandshake,
    now: OffsetDateTime,
) -> Result<(), DesktopSidecarContractError> {
    validate_schema(handshake.schema_version)?;
    validate_protocol(&handshake.protocol)?;
    validate_non_empty(&handshake.session_id, "sessionId")?;
    validate_non_empty(&handshake.token_sha256, "tokenSha256")?;
    let expires_at = parse_timestamp(&handshake.expires_at, "expiresAt")?;
    if now >= expires_at {
        return Err(DesktopSidecarContractError::LeaseExpired);
    }
    if handshake.allowed_operations.is_empty() {
        return Err(DesktopSidecarContractError::OperationNotAllowed {
            operation: DesktopSidecarOperation::Health,
        });
    }
    Ok(())
}

pub fn validate_sidecar_request(
    request: &DesktopSidecarRequest,
    lease: &DesktopSidecarLease,
    now: OffsetDateTime,
) -> Result<(), DesktopSidecarContractError> {
    validate_schema(request.schema_version)?;
    validate_protocol(&request.protocol)?;
    validate_non_empty(&request.request_id, "requestId")?;
    validate_non_empty(&request.session_id, "sessionId")?;
    validate_non_empty(&request.policy_decision_id, "policyDecisionId")?;
    if request.session_id != lease.session_id {
        return Err(DesktopSidecarContractError::SessionMismatch);
    }
    if let (Some(request_run_id), Some(lease_run_id)) = (&request.run_id, &lease.run_id) {
        if request_run_id != lease_run_id {
            return Err(DesktopSidecarContractError::RunMismatch);
        }
    }
    if request.auth.scheme != DesktopSidecarAuthScheme::BearerSessionToken
        || hash_session_token(&request.auth.token) != lease.token_sha256
    {
        return Err(DesktopSidecarContractError::InvalidToken);
    }
    let lease_expires_at = parse_timestamp(&lease.expires_at, "lease.expiresAt")?;
    if now >= lease_expires_at {
        return Err(DesktopSidecarContractError::LeaseExpired);
    }
    let request_expires_at = parse_timestamp(&request.expires_at, "expiresAt")?;
    if now >= request_expires_at {
        return Err(DesktopSidecarContractError::RequestExpired);
    }
    if request_expires_at > lease_expires_at {
        return Err(DesktopSidecarContractError::RequestOutlivesLease);
    }
    if !lease.allowed_operations.contains(&request.operation) {
        return Err(DesktopSidecarContractError::OperationNotAllowed {
            operation: request.operation,
        });
    }
    validate_payload(&request.payload)?;
    Ok(())
}

pub fn validate_sidecar_response(
    response: &DesktopSidecarResponse,
    expected_request_id: &str,
    expected_operation: DesktopSidecarOperation,
) -> Result<(), DesktopSidecarContractError> {
    validate_schema(response.schema_version)?;
    validate_protocol(&response.protocol)?;
    validate_non_empty(&response.request_id, "requestId")?;
    if response.request_id != expected_request_id {
        return Err(DesktopSidecarContractError::ResponseRequestMismatch {
            expected: expected_request_id.into(),
            actual: response.request_id.clone(),
        });
    }
    if response.operation != expected_operation {
        return Err(DesktopSidecarContractError::ResponseOperationMismatch {
            expected: expected_operation,
            actual: response.operation,
        });
    }
    match (
        response.ok,
        response.result.is_some(),
        response.error.is_some(),
    ) {
        (true, true, false) | (false, false, true) => Ok(()),
        (true, false, false) => Err(DesktopSidecarContractError::ResponseMissingResult),
        (false, false, false) => Err(DesktopSidecarContractError::ResponseMissingError),
        _ => Err(DesktopSidecarContractError::ResponseConflictingBody),
    }
}

pub fn hash_session_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn validate_schema(actual: u32) -> Result<(), DesktopSidecarContractError> {
    if actual == DESKTOP_SIDECAR_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(DesktopSidecarContractError::SchemaVersionMismatch {
            expected: DESKTOP_SIDECAR_SCHEMA_VERSION,
            actual,
        })
    }
}

fn validate_protocol(actual: &str) -> Result<(), DesktopSidecarContractError> {
    if actual == DESKTOP_SIDECAR_PROTOCOL {
        Ok(())
    } else {
        Err(DesktopSidecarContractError::ProtocolMismatch {
            expected: DESKTOP_SIDECAR_PROTOCOL.into(),
            actual: actual.into(),
        })
    }
}

fn validate_non_empty(value: &str, field: &'static str) -> Result<(), DesktopSidecarContractError> {
    if value.trim().is_empty() {
        Err(DesktopSidecarContractError::EmptyField { field })
    } else {
        Ok(())
    }
}

fn parse_timestamp(
    value: &str,
    field: &'static str,
) -> Result<OffsetDateTime, DesktopSidecarContractError> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|_| DesktopSidecarContractError::InvalidTimestamp { field })
}

fn validate_payload(payload: &JsonValue) -> Result<(), DesktopSidecarContractError> {
    let bytes = serde_json::to_vec(payload).unwrap_or_default();
    if bytes.len() > DESKTOP_SIDECAR_MAX_PAYLOAD_BYTES {
        return Err(DesktopSidecarContractError::PayloadTooLarge);
    }
    if let Some(key) = forbidden_payload_key(payload) {
        return Err(DesktopSidecarContractError::ForbiddenPayloadKey { key });
    }
    Ok(())
}

fn forbidden_payload_key(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::Object(map) => {
            for (key, value) in map {
                if is_forbidden_payload_key(key) {
                    return Some(key.clone());
                }
                if let Some(found) = forbidden_payload_key(value) {
                    return Some(found);
                }
            }
            None
        }
        JsonValue::Array(values) => values.iter().find_map(forbidden_payload_key),
        _ => None,
    }
}

fn is_forbidden_payload_key(key: &str) -> bool {
    let normalized: String = key
        .chars()
        .filter(|ch| *ch != '_' && *ch != '-')
        .flat_map(char::to_lowercase)
        .collect();
    matches!(
        normalized.as_str(),
        "shell"
            | "script"
            | "command"
            | "commandline"
            | "argv"
            | "stdin"
            | "stdout"
            | "executable"
            | "executablepath"
            | "plugin"
            | "pluginpath"
            | "file"
            | "filepath"
            | "readpath"
            | "writepath"
            | "sourcepath"
            | "targetpath"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn future(seconds: i64) -> String {
        (OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(seconds))
            .format(&Rfc3339)
            .expect("format")
    }

    fn lease() -> DesktopSidecarLease {
        DesktopSidecarLease {
            session_id: "session_1".into(),
            run_id: Some("run_1".into()),
            token_sha256: hash_session_token("token"),
            allowed_operations: [DesktopSidecarOperation::Health].into_iter().collect(),
            expires_at: future(120),
        }
    }

    fn request() -> DesktopSidecarRequest {
        DesktopSidecarRequest {
            schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
            protocol: DESKTOP_SIDECAR_PROTOCOL.into(),
            request_id: "req_1".into(),
            session_id: "session_1".into(),
            run_id: Some("run_1".into()),
            actor: DesktopSidecarActor::Agent,
            operation: DesktopSidecarOperation::Health,
            payload: json!({}),
            policy_decision_id: "policy_1".into(),
            auth: DesktopSidecarAuth {
                scheme: DesktopSidecarAuthScheme::BearerSessionToken,
                token: "token".into(),
            },
            expires_at: future(60),
        }
    }

    #[test]
    fn validates_token_and_lease() {
        validate_sidecar_request(&request(), &lease(), OffsetDateTime::UNIX_EPOCH)
            .expect("valid request");
    }

    #[test]
    fn rejects_bad_token() {
        let mut request = request();
        request.auth.token = "bad".into();
        let error = validate_sidecar_request(&request, &lease(), OffsetDateTime::UNIX_EPOCH)
            .expect_err("bad token");
        assert_eq!(error.code(), "sidecar_auth_failed");
    }

    #[test]
    fn rejects_expired_request() {
        let error = validate_sidecar_request(
            &request(),
            &lease(),
            OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(90),
        )
        .expect_err("expired request");
        assert_eq!(error, DesktopSidecarContractError::RequestExpired);
    }

    #[test]
    fn rejects_disallowed_operation() {
        let mut request = request();
        request.operation = DesktopSidecarOperation::MouseClick;
        let error = validate_sidecar_request(&request, &lease(), OffsetDateTime::UNIX_EPOCH)
            .expect_err("operation denied");
        assert_eq!(error.code(), "sidecar_operation_not_allowed");
    }

    #[test]
    fn rejects_shell_like_payloads() {
        let mut request = request();
        request.payload = json!({ "command": "rm -rf ~" });
        let error = validate_sidecar_request(&request, &lease(), OffsetDateTime::UNIX_EPOCH)
            .expect_err("forbidden key");
        assert_eq!(error.code(), "sidecar_forbidden_payload");
    }

    #[test]
    fn rejects_unknown_request_fields() {
        let raw = json!({
            "schemaVersion": DESKTOP_SIDECAR_SCHEMA_VERSION,
            "protocol": DESKTOP_SIDECAR_PROTOCOL,
            "requestId": "req_1",
            "sessionId": "session_1",
            "actor": "agent",
            "operation": "health",
            "payload": {},
            "policyDecisionId": "policy_1",
            "auth": { "scheme": "bearer_session_token", "token": "token" },
            "expiresAt": future(60),
            "shell": "nope"
        });
        assert!(serde_json::from_value::<DesktopSidecarRequest>(raw).is_err());
    }

    #[test]
    fn validates_response_shape() {
        let response =
            DesktopSidecarResponse::ok("req_1", DesktopSidecarOperation::Health, json!({}));
        validate_sidecar_response(&response, "req_1", DesktopSidecarOperation::Health)
            .expect("valid response");
    }

    #[test]
    fn rejects_response_request_mismatch() {
        let response =
            DesktopSidecarResponse::ok("req_2", DesktopSidecarOperation::Health, json!({}));
        let error = validate_sidecar_response(&response, "req_1", DesktopSidecarOperation::Health)
            .expect_err("request mismatch");
        assert_eq!(error.code(), "sidecar_response_invalid");
    }

    #[test]
    fn rejects_success_response_without_result() {
        let response = DesktopSidecarResponse {
            schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
            protocol: DESKTOP_SIDECAR_PROTOCOL.into(),
            request_id: "req_1".into(),
            operation: DesktopSidecarOperation::Health,
            ok: true,
            result: None,
            error: None,
        };
        let error = validate_sidecar_response(&response, "req_1", DesktopSidecarOperation::Health)
            .expect_err("missing result");
        assert_eq!(error, DesktopSidecarContractError::ResponseMissingResult);
    }

    #[test]
    fn parses_screenshot_request_contract() {
        let request = serde_json::from_value::<DesktopSidecarScreenshotRequest>(json!({
            "displayId": "main",
            "region": { "x": 1, "y": 2, "width": 3, "height": 4 }
        }))
        .expect("screenshot request");

        assert_eq!(request.display_id.as_deref(), Some("main"));
        assert_eq!(request.region.as_ref().map(|region| region.width), Some(3));
    }

    #[test]
    fn parses_control_request_contract() {
        let request = serde_json::from_value::<DesktopSidecarControlRequest>(json!({
            "x": 10,
            "y": 20,
            "button": "right",
            "clicks": 2,
            "elementId": "macos_ax:1:AXButton:10:20:30:40:10:20",
            "keys": ["command", "a"],
            "text": "hello",
            "value": "updated",
            "menuPath": ["File", "New"]
        }))
        .expect("control request");

        assert_eq!(
            request.element_id.as_deref(),
            Some("macos_ax:1:AXButton:10:20:30:40:10:20")
        );
        assert_eq!(request.x, Some(10));
        assert_eq!(request.button, Some(DesktopSidecarMouseButton::Right));
        assert_eq!(request.keys, vec!["command".to_string(), "a".to_string()]);
        assert_eq!(request.text.as_deref(), Some("hello"));
        assert_eq!(request.value.as_deref(), Some("updated"));
        assert_eq!(
            request.menu_path,
            vec!["File".to_string(), "New".to_string()]
        );
    }

    #[test]
    fn parses_cursor_state_contract() {
        let payload = serde_json::from_value::<DesktopSidecarCursorStatePayload>(json!({
            "x": 12,
            "y": 34,
            "displayId": "main",
            "available": true
        }))
        .expect("cursor state payload");

        assert_eq!(payload.x, 12);
        assert_eq!(payload.y, 34);
        assert_eq!(payload.display_id.as_deref(), Some("main"));
        assert!(payload.available);
    }

    #[test]
    fn parses_element_at_point_contract() {
        let request = serde_json::from_value::<DesktopSidecarPointRequest>(json!({
            "x": 10,
            "y": 20
        }))
        .expect("point request");
        assert_eq!(request.x, 10);
        assert_eq!(request.y, 20);

        let payload = serde_json::from_value::<DesktopSidecarElementAtPointPayload>(json!({
            "x": 10,
            "y": 20,
            "available": true,
            "element": {
                "elementId": "macos_ax:1:button:10:20:30:40",
                "pid": 1,
                "role": "AXButton",
                "title": "Continue",
                "enabled": true,
                "focused": false,
                "x": 10,
                "y": 20,
                "width": 30,
                "height": 40
            }
        }))
        .expect("element payload");

        assert!(payload.available);
        assert_eq!(
            payload
                .element
                .as_ref()
                .map(|element| element.role.as_deref()),
            Some(Some("AXButton"))
        );
    }

    #[test]
    fn parses_accessibility_snapshot_contract() {
        let request = serde_json::from_value::<DesktopSidecarAccessibilitySnapshotRequest>(json!({
            "windowId": "window-1",
            "focusedOnly": false,
            "includeChildren": true,
            "maxDepth": 3,
            "limit": 50
        }))
        .expect("snapshot request");
        assert_eq!(request.window_id.as_deref(), Some("window-1"));
        assert_eq!(request.max_depth, Some(3));

        let payload = serde_json::from_value::<DesktopSidecarAccessibilitySnapshotPayload>(json!({
            "performed": true,
            "target": { "pid": 1, "windowId": "window-1", "appName": "Notes" },
            "rows": [{
                "rowType": "macos_accessibility_window",
                "depth": 0,
                "state": "visible",
                "element": {
                    "elementId": "macos_ax:1:AXWindow:0:0:800:600:0:0",
                    "pid": 1,
                    "role": "AXWindow",
                    "title": "Untitled",
                    "enabled": true
                }
            }],
            "truncated": false,
            "diagnostics": []
        }))
        .expect("snapshot payload");

        assert!(payload.performed);
        assert_eq!(payload.rows.len(), 1);
        assert_eq!(payload.rows[0].element.role.as_deref(), Some("AXWindow"));
    }

    #[test]
    fn parses_ocr_snapshot_contract() {
        let request = serde_json::from_value::<DesktopSidecarOcrSnapshotRequest>(json!({
            "displayId": "main",
            "region": { "x": 1, "y": 2, "width": 3, "height": 4 },
            "limit": 25
        }))
        .expect("ocr request");

        assert_eq!(request.display_id.as_deref(), Some("main"));
        assert_eq!(request.region.as_ref().map(|region| region.height), Some(4));
        assert_eq!(request.limit, Some(25));

        let payload = serde_json::from_value::<DesktopSidecarOcrSnapshotPayload>(json!({
            "performed": true,
            "capturedAt": "2026-05-26T12:00:00Z",
            "width": 120,
            "height": 80,
            "scaleFactor": 2.0,
            "textBlocks": [{
                "text": "Continue",
                "x": 10,
                "y": 20,
                "width": 30,
                "height": 12,
                "confidence": 0.98
            }],
            "fullText": "Continue",
            "truncated": false
        }))
        .expect("ocr payload");

        assert!(payload.performed);
        assert_eq!(payload.text_blocks[0].text, "Continue");
    }

    #[test]
    fn parses_stream_request_contract() {
        let request = serde_json::from_value::<DesktopSidecarStreamRequest>(json!({
            "sessionId": "session-1",
            "runId": "run-1",
            "displayId": "main",
            "streamId": "stream-1",
            "maxWidth": 1280,
            "maxFrameRate": 24,
            "includeCursor": true,
            "quality": "balanced",
            "iceServers": [
                {
                    "urls": ["turn:turn.example.test:3478"],
                    "username": "user",
                    "credential": "pass",
                    "credentialType": "password"
                }
            ],
            "sessionDescription": {
                "type": "answer",
                "sdp": "v=0"
            },
            "iceCandidate": {
                "candidate": "candidate:1",
                "sdpMid": "0",
                "sdpMLineIndex": 0,
                "usernameFragment": "ufrag"
            }
        }))
        .expect("stream request");

        assert_eq!(request.session_id.as_deref(), Some("session-1"));
        assert_eq!(request.stream_id.as_deref(), Some("stream-1"));
        assert_eq!(request.max_frame_rate, Some(24));
        assert_eq!(request.quality, Some(DesktopSidecarStreamQuality::Balanced));
        assert_eq!(request.ice_servers.len(), 1);
        assert_eq!(
            request
                .session_description
                .as_ref()
                .map(|value| value.sdp.as_str()),
            Some("v=0")
        );
        assert_eq!(
            request
                .ice_candidate
                .as_ref()
                .map(|value| value.candidate.as_str()),
            Some("candidate:1")
        );
    }

    #[test]
    fn parses_stream_payload_contract() {
        let payload = serde_json::from_value::<DesktopSidecarStreamPayload>(json!({
            "streamId": "stream-1",
            "status": "starting",
            "transport": "web_rtc",
            "signalingChannel": "computer_use_stream",
            "quality": "high",
            "maxWidth": 1920,
            "maxFrameRate": 30,
            "includeCursor": true,
            "metrics": {
                "captureBackend": "screencapturekit",
                "encoderBackend": "videotoolbox_h264",
                "encoderHardware": true,
                "preferredCodec": "video/H264",
                "captureFrameRate": 24,
                "captureDroppedFrames": 1,
                "encodeFrameRate": 24,
                "encodeLatencyMs": 4,
                "outboundBitrateBps": 3500000,
                "availableOutgoingBitrateBps": 5000000,
                "packetsSent": 100,
                "bytesSent": 120000,
                "packetLoss": 0,
                "roundTripTimeMs": 12,
                "retransmits": 2,
                "keyframes": 1
            },
            "message": "Native stream is starting."
        }))
        .expect("stream payload");

        assert_eq!(payload.stream_id.as_deref(), Some("stream-1"));
        assert_eq!(payload.transport, DesktopSidecarStreamTransport::WebRtc);
        assert_eq!(payload.status, DesktopSidecarStreamStatus::Starting);
        assert_eq!(payload.quality, DesktopSidecarStreamQuality::High);
        let metrics = payload.metrics.expect("stream metrics");
        assert_eq!(
            metrics.encoder_backend.as_deref(),
            Some("videotoolbox_h264")
        );
        assert_eq!(metrics.available_outgoing_bitrate_bps, Some(5_000_000));
        assert_eq!(metrics.retransmits, Some(2));
    }

    #[test]
    fn parses_stream_capabilities_contract() {
        let payload = serde_json::from_value::<DesktopSidecarStreamCapabilitiesPayload>(json!({
            "webrtcStream": true,
            "screenshotFallbackStream": true,
            "nativeVideoTrack": true,
            "preferredCodec": "video/H264",
            "captureBackends": ["screencapturekit"],
            "encoderBackends": ["videotoolbox_h264"],
            "hardwareEncoding": true,
            "supportedQualities": ["low", "balanced", "high"],
            "maxWidth": 1920,
            "maxFrameRate": 30,
            "message": "WebRTC streaming is available."
        }))
        .expect("stream capabilities");

        assert!(payload.webrtc_stream);
        assert_eq!(
            payload.supported_qualities,
            vec![
                DesktopSidecarStreamQuality::Low,
                DesktopSidecarStreamQuality::Balanced,
                DesktopSidecarStreamQuality::High
            ]
        );
    }
}
