use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use image::{ImageFormat, Rgba, RgbaImage};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use xcap::{Monitor, Window};
use xero_desktop_control_ipc::{
    hash_session_token, validate_sidecar_response, DesktopSidecarAccessibilitySnapshotPayload,
    DesktopSidecarAccessibilitySnapshotRequest, DesktopSidecarApp, DesktopSidecarAppListPayload,
    DesktopSidecarAuth, DesktopSidecarAuthScheme, DesktopSidecarCapabilities,
    DesktopSidecarControlRequest, DesktopSidecarCursorStatePayload, DesktopSidecarDisplay,
    DesktopSidecarDisplayListPayload, DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody,
    DesktopSidecarForegroundStatePayload, DesktopSidecarHandshake, DesktopSidecarIceCandidate,
    DesktopSidecarIceServer, DesktopSidecarIceServerUrls, DesktopSidecarMouseButton,
    DesktopSidecarOcrSnapshotPayload, DesktopSidecarOcrSnapshotRequest, DesktopSidecarOperation,
    DesktopSidecarPermissionGrant, DesktopSidecarPermissionStatus,
    DesktopSidecarPermissionsPayload, DesktopSidecarPointRequest, DesktopSidecarRedactionMode,
    DesktopSidecarRedactionRequest, DesktopSidecarRegion, DesktopSidecarRequest,
    DesktopSidecarResponse, DesktopSidecarScreenshotPayload, DesktopSidecarScreenshotRequest,
    DesktopSidecarSessionDescription, DesktopSidecarStreamPayload, DesktopSidecarStreamQuality,
    DesktopSidecarStreamRequest, DesktopSidecarStreamStatus, DesktopSidecarStreamTransport,
    DesktopSidecarWindow, DesktopSidecarWindowListPayload, DESKTOP_SIDECAR_PROTOCOL,
    DESKTOP_SIDECAR_SCHEMA_VERSION,
};

use super::{
    AutonomousMacosAutomationAction, AutonomousMacosAutomationRequest,
    AutonomousSystemDiagnosticsAction, AutonomousSystemDiagnosticsArtifactMode,
    AutonomousSystemDiagnosticsRequest, AutonomousToolOutput, AutonomousToolResult,
    AutonomousToolRuntime, AUTONOMOUS_TOOL_DESKTOP_CONTROL, AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
    AUTONOMOUS_TOOL_DESKTOP_STREAM,
};
use crate::{
    commands::{validate_non_empty, CommandError, CommandResult},
    db::project_app_data_dir_for_repo,
};

const DESKTOP_CONTROL_PHASE: &str = "phase_computer_use_desktop_control";
const DESKTOP_AUDIT_DIR: &str = "desktop-control";
const DESKTOP_AUDIT_FILE: &str = "desktop-control/audit.jsonl";
const DESKTOP_STREAM_SESSIONS_FILE: &str = "desktop-control/stream-sessions.jsonl";
#[cfg(unix)]
const DESKTOP_METADATA_DIR_MODE: u32 = 0o700;
#[cfg(unix)]
const DESKTOP_METADATA_FILE_MODE: u32 = 0o600;
const DEFAULT_LOCK_LEASE_MS: u64 = 30_000;
const DEFAULT_SIDECAR_LEASE_MS: u64 = 5 * 60 * 1_000;
const MAX_TYPE_TEXT_CHARS: usize = 8_000;
const MAX_MENU_PATH_SEGMENTS: usize = 8;
const MAX_PRIVATE_REGIONS: usize = 32;
const DESKTOP_STATUS_SCHEMA: &str = "xero.desktop_control_status.v1";
#[cfg(not(test))]
const DESKTOP_SIDECAR_BINARY_NAME: &str = "xero-desktop-sidecar";
const DESKTOP_SIDECAR_PATH_ENV: &str = "XERO_DESKTOP_SIDECAR_PATH";
const DESKTOP_SIDECAR_SHA256_ENV: &str = "XERO_DESKTOP_SIDECAR_SHA256";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopObserveAction {
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
    Health,
}

impl AutonomousDesktopObserveAction {
    fn as_str(&self) -> &'static str {
        match self {
            Self::PermissionsStatus => "permissions_status",
            Self::DisplayList => "display_list",
            Self::WindowList => "window_list",
            Self::AppList => "app_list",
            Self::ForegroundState => "foreground_state",
            Self::Screenshot => "screenshot",
            Self::CursorState => "cursor_state",
            Self::AccessibilitySnapshot => "accessibility_snapshot",
            Self::OcrSnapshot => "ocr_snapshot",
            Self::ElementAtPoint => "element_at_point",
            Self::Health => "health",
        }
    }

    fn sensitive(&self) -> bool {
        matches!(
            self,
            Self::Screenshot
                | Self::AccessibilitySnapshot
                | Self::OcrSnapshot
                | Self::ElementAtPoint
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopControlAction {
    MouseMove,
    MouseClick,
    MouseDoubleClick,
    MouseRightClick,
    MouseDrag,
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
}

impl AutonomousDesktopControlAction {
    fn as_str(&self) -> &'static str {
        match self {
            Self::MouseMove => "mouse_move",
            Self::MouseClick => "mouse_click",
            Self::MouseDoubleClick => "mouse_double_click",
            Self::MouseRightClick => "mouse_right_click",
            Self::MouseDrag => "mouse_drag",
            Self::Scroll => "scroll",
            Self::KeyPress => "key_press",
            Self::Hotkey => "hotkey",
            Self::TypeText => "type_text",
            Self::PasteText => "paste_text",
            Self::FocusWindow => "focus_window",
            Self::ActivateApp => "activate_app",
            Self::LaunchApp => "launch_app",
            Self::QuitApp => "quit_app",
            Self::AxPress => "ax_press",
            Self::AxSetValue => "ax_set_value",
            Self::AxFocus => "ax_focus",
            Self::MenuSelect => "menu_select",
            Self::CancelCurrentAction => "cancel_current_action",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopStreamAction {
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

impl AutonomousDesktopStreamAction {
    fn as_str(&self) -> &'static str {
        match self {
            Self::StreamCapabilities => "stream_capabilities",
            Self::StreamStart => "stream_start",
            Self::StreamOffer => "stream_offer",
            Self::StreamAnswer => "stream_answer",
            Self::StreamIceCandidate => "stream_ice_candidate",
            Self::StreamStop => "stream_stop",
            Self::StreamStatus => "stream_status",
            Self::StreamSetQuality => "stream_set_quality",
            Self::StreamRequestKeyframe => "stream_request_keyframe",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopObserveRequest {
    pub action: AutonomousDesktopObserveAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<AutonomousDesktopRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redaction: Option<AutonomousDesktopRedactionRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopControlRequest {
    pub action: AutonomousDesktopControlAction,
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
    pub button: Option<AutonomousDesktopMouseButton>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensitivity: Option<AutonomousDesktopTextSensitivity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopStreamRequest {
    pub action: AutonomousDesktopStreamAction,
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
    pub quality: Option<AutonomousDesktopStreamQuality>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redaction: Option<AutonomousDesktopRedactionRequest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ice_servers: Vec<AutonomousDesktopIceServer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_description: Option<AutonomousDesktopSessionDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ice_candidate: Option<AutonomousDesktopIceCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopIceServer {
    pub urls: AutonomousDesktopIceServerUrls,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum AutonomousDesktopIceServerUrls {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopSessionDescription {
    #[serde(rename = "type")]
    pub sdp_type: String,
    pub sdp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopIceCandidate {
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
pub struct AutonomousDesktopRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopRedactionRequest {
    #[serde(default)]
    pub mode: AutonomousDesktopRedactionMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub private_regions: Vec<AutonomousDesktopRegion>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopRedactionMode {
    Off,
    #[default]
    Balanced,
    Auto,
    Strict,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopMouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopTextSensitivity {
    Normal,
    Sensitive,
    Secret,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopStreamQuality {
    Low,
    Balanced,
    High,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopCapabilities {
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
    pub accessibility_actions: bool,
    pub menu_select: bool,
    pub webrtc_stream: bool,
    pub screenshot_fallback_stream: bool,
    pub manual_cloud_control: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopPermissionStatus {
    pub name: String,
    pub status: AutonomousDesktopPermissionGrant,
    pub required_for: Vec<String>,
    pub remediation: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopPermissionGrant {
    Granted,
    Denied,
    Unknown,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopDisplay {
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

impl Eq for AutonomousDesktopDisplay {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopWindow {
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
pub struct AutonomousDesktopApp {
    pub app_name: String,
    pub pid: u32,
    pub window_count: usize,
    pub focused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopScreenshot {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub captured_at: String,
    pub redactions_applied: usize,
}

impl Eq for AutonomousDesktopScreenshot {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopCursorState {
    pub x: i32,
    pub y: i32,
    pub display_id: Option<String>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopStreamState {
    pub stream_id: Option<String>,
    pub status: AutonomousDesktopStreamStatus,
    pub transport: AutonomousDesktopStreamTransport,
    pub signaling_channel: Option<String>,
    pub quality: AutonomousDesktopStreamQuality,
    pub max_width: u32,
    pub max_frame_rate: u32,
    pub include_cursor: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopStreamSignal {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_description: Option<AutonomousDesktopSessionDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ice_candidate: Option<AutonomousDesktopIceCandidate>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopStreamStatus {
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
pub enum AutonomousDesktopStreamTransport {
    WebRtc,
    ScreenshotFallback,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopControllerLock {
    pub actor: AutonomousDesktopActor,
    pub lease_id: Option<String>,
    pub session_id: String,
    pub run_id: Option<String>,
    pub acquired_at: String,
    pub expires_at: String,
    pub last_input_at: String,
    pub release_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopActor {
    Agent,
    LocalUser,
    CloudManualControl,
}

impl AutonomousDesktopActor {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::LocalUser => "local_user",
            Self::CloudManualControl => "cloud_manual_control",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopPolicyTrace {
    pub category: AutonomousDesktopPolicyCategory,
    pub decision: AutonomousDesktopPolicyDecision,
    pub decision_id: String,
    pub code: String,
    pub reason: String,
    pub approval_required: bool,
    pub user_action_required: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopPolicyCategory {
    ObserveSafe,
    ObserveSensitive,
    ControlSafe,
    ControlApprovalRequired,
    ControlDenied,
    StreamSafe,
    StreamApprovalRequired,
    StreamDenied,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopPolicyDecision {
    Allowed,
    ApprovalRequired,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopToolError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub user_action_required: bool,
    pub safe_next_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopToolOutput {
    pub tool: String,
    pub action: String,
    pub request_id: String,
    pub phase: String,
    pub status: AutonomousDesktopToolStatus,
    pub platform: String,
    pub sidecar: AutonomousDesktopSidecarStatus,
    pub capabilities: AutonomousDesktopCapabilities,
    pub permissions: Vec<AutonomousDesktopPermissionStatus>,
    pub policy: AutonomousDesktopPolicyTrace,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub displays: Vec<AutonomousDesktopDisplay>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub windows: Vec<AutonomousDesktopWindow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<AutonomousDesktopApp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground: Option<AutonomousDesktopWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<AutonomousDesktopCursorState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<AutonomousDesktopScreenshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<AutonomousDesktopStreamState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_signal: Option<AutonomousDesktopStreamSignal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_snapshot: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller_lock: Option<AutonomousDesktopControllerLock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<AutonomousDesktopToolError>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopControlStatusSnapshot {
    pub schema: String,
    pub platform: String,
    pub sidecar: AutonomousDesktopSidecarStatus,
    pub capabilities: AutonomousDesktopCapabilities,
    pub permissions: Vec<AutonomousDesktopPermissionStatus>,
    pub controller_lock: Option<AutonomousDesktopControllerLock>,
    pub stream: AutonomousDesktopStreamState,
    pub updated_at: String,
}

impl From<DesktopSidecarCapabilities> for AutonomousDesktopCapabilities {
    fn from(capabilities: DesktopSidecarCapabilities) -> Self {
        Self {
            platform: capabilities.platform,
            schema_version: capabilities.schema_version,
            display_list: capabilities.display_list,
            screenshot: capabilities.screenshot,
            window_list: capabilities.window_list,
            app_list: capabilities.app_list,
            foreground_state: capabilities.foreground_state,
            cursor_state: capabilities.cursor_state,
            accessibility_snapshot: capabilities.accessibility_snapshot,
            ocr_snapshot: capabilities.ocr_snapshot,
            mouse_input: capabilities.mouse_input,
            keyboard_input: capabilities.keyboard_input,
            clipboard: capabilities.clipboard,
            accessibility_actions: capabilities.accessibility_actions,
            menu_select: capabilities.menu_select,
            webrtc_stream: capabilities.webrtc_stream,
            screenshot_fallback_stream: capabilities.screenshot_fallback_stream,
            manual_cloud_control: capabilities.manual_cloud_control,
        }
    }
}

impl From<DesktopSidecarPermissionGrant> for AutonomousDesktopPermissionGrant {
    fn from(status: DesktopSidecarPermissionGrant) -> Self {
        match status {
            DesktopSidecarPermissionGrant::Granted => Self::Granted,
            DesktopSidecarPermissionGrant::Denied => Self::Denied,
            DesktopSidecarPermissionGrant::Unknown => Self::Unknown,
            DesktopSidecarPermissionGrant::Unsupported => Self::Unsupported,
        }
    }
}

impl From<DesktopSidecarPermissionStatus> for AutonomousDesktopPermissionStatus {
    fn from(permission: DesktopSidecarPermissionStatus) -> Self {
        Self {
            name: permission.name,
            status: permission.status.into(),
            required_for: permission.required_for,
            remediation: permission.remediation,
        }
    }
}

impl From<DesktopSidecarDisplay> for AutonomousDesktopDisplay {
    fn from(display: DesktopSidecarDisplay) -> Self {
        Self {
            display_id: display.display_id,
            name: display.name,
            x: display.x,
            y: display.y,
            width: display.width,
            height: display.height,
            scale_factor: display.scale_factor,
            rotation: display.rotation,
            primary: display.primary,
        }
    }
}

impl From<DesktopSidecarWindow> for AutonomousDesktopWindow {
    fn from(window: DesktopSidecarWindow) -> Self {
        Self {
            window_id: window.window_id,
            app_name: window.app_name,
            title: window.title,
            pid: window.pid,
            x: window.x,
            y: window.y,
            width: window.width,
            height: window.height,
            z: window.z,
            focused: window.focused,
            minimized: window.minimized,
        }
    }
}

impl From<DesktopSidecarApp> for AutonomousDesktopApp {
    fn from(app: DesktopSidecarApp) -> Self {
        Self {
            app_name: app.app_name,
            pid: app.pid,
            window_count: app.window_count,
            focused: app.focused,
        }
    }
}

impl From<DesktopSidecarCursorStatePayload> for AutonomousDesktopCursorState {
    fn from(cursor: DesktopSidecarCursorStatePayload) -> Self {
        Self {
            x: cursor.x,
            y: cursor.y,
            display_id: cursor.display_id,
            available: cursor.available,
        }
    }
}

impl From<DesktopSidecarStreamQuality> for AutonomousDesktopStreamQuality {
    fn from(quality: DesktopSidecarStreamQuality) -> Self {
        match quality {
            DesktopSidecarStreamQuality::Low => Self::Low,
            DesktopSidecarStreamQuality::Balanced => Self::Balanced,
            DesktopSidecarStreamQuality::High => Self::High,
        }
    }
}

impl From<DesktopSidecarStreamStatus> for AutonomousDesktopStreamStatus {
    fn from(status: DesktopSidecarStreamStatus) -> Self {
        match status {
            DesktopSidecarStreamStatus::Idle => Self::Idle,
            DesktopSidecarStreamStatus::Starting => Self::Starting,
            DesktopSidecarStreamStatus::Live => Self::Live,
            DesktopSidecarStreamStatus::Degraded => Self::Degraded,
            DesktopSidecarStreamStatus::Paused => Self::Paused,
            DesktopSidecarStreamStatus::Stopped => Self::Stopped,
            DesktopSidecarStreamStatus::Failed => Self::Failed,
        }
    }
}

impl From<DesktopSidecarStreamTransport> for AutonomousDesktopStreamTransport {
    fn from(transport: DesktopSidecarStreamTransport) -> Self {
        match transport {
            DesktopSidecarStreamTransport::WebRtc => Self::WebRtc,
            DesktopSidecarStreamTransport::ScreenshotFallback => Self::ScreenshotFallback,
            DesktopSidecarStreamTransport::Unavailable => Self::Unavailable,
        }
    }
}

impl From<DesktopSidecarStreamPayload> for AutonomousDesktopStreamState {
    fn from(payload: DesktopSidecarStreamPayload) -> Self {
        Self {
            stream_id: payload.stream_id,
            status: payload.status.into(),
            transport: payload.transport.into(),
            signaling_channel: payload.signaling_channel,
            quality: payload.quality.into(),
            max_width: payload.max_width,
            max_frame_rate: payload.max_frame_rate,
            include_cursor: payload.include_cursor,
            message: payload.message,
        }
    }
}

struct AutonomousDesktopStreamSidecarOutput {
    stream: AutonomousDesktopStreamState,
    signal: Option<AutonomousDesktopStreamSignal>,
}

impl From<DesktopSidecarStreamPayload> for AutonomousDesktopStreamSidecarOutput {
    fn from(payload: DesktopSidecarStreamPayload) -> Self {
        let stream = AutonomousDesktopStreamState {
            stream_id: payload.stream_id,
            status: payload.status.into(),
            transport: payload.transport.into(),
            signaling_channel: payload.signaling_channel,
            quality: payload.quality.into(),
            max_width: payload.max_width,
            max_frame_rate: payload.max_frame_rate,
            include_cursor: payload.include_cursor,
            message: payload.message,
        };
        let signal = if payload.session_description.is_some() || payload.ice_candidate.is_some() {
            Some(AutonomousDesktopStreamSignal {
                session_description: payload
                    .session_description
                    .map(autonomous_session_description),
                ice_candidate: payload.ice_candidate.map(autonomous_ice_candidate),
            })
        } else {
            None
        };
        Self { stream, signal }
    }
}

fn autonomous_session_description(
    description: DesktopSidecarSessionDescription,
) -> AutonomousDesktopSessionDescription {
    AutonomousDesktopSessionDescription {
        sdp_type: description.sdp_type,
        sdp: description.sdp,
    }
}

fn autonomous_ice_candidate(
    candidate: DesktopSidecarIceCandidate,
) -> AutonomousDesktopIceCandidate {
    AutonomousDesktopIceCandidate {
        candidate: candidate.candidate,
        sdp_mid: candidate.sdp_mid,
        sdp_m_line_index: candidate.sdp_m_line_index,
        username_fragment: candidate.username_fragment,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDesktopToolStatus {
    Executed,
    Starting,
    Stopped,
    ApprovalRequired,
    Denied,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDesktopSidecarStatus {
    pub schema_version: u32,
    pub platform: String,
    pub transport: String,
    pub authenticated: bool,
    pub health: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DesktopControlState {
    lock: Arc<Mutex<Option<AutonomousDesktopControllerLock>>>,
    stream: Arc<Mutex<AutonomousDesktopStreamState>>,
}

impl Default for DesktopControlState {
    fn default() -> Self {
        #[cfg(test)]
        {
            Self::new_local()
        }
        #[cfg(not(test))]
        {
            static SHARED: OnceLock<DesktopControlState> = OnceLock::new();
            SHARED.get_or_init(Self::new_local).clone()
        }
    }
}

impl DesktopControlState {
    fn new_local() -> Self {
        Self {
            lock: Arc::new(Mutex::new(None)),
            stream: Arc::new(Mutex::new(default_stream_state())),
        }
    }
}

impl AutonomousToolRuntime {
    pub fn desktop_observe(
        &self,
        request: AutonomousDesktopObserveRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.desktop_observe_with_approval(request, false)
    }

    pub fn desktop_observe_with_operator_approval(
        &self,
        request: AutonomousDesktopObserveRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.desktop_observe_with_approval(request, true)
    }

    fn desktop_observe_with_approval(
        &self,
        request: AutonomousDesktopObserveRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        validate_desktop_observe_request(&request)?;
        if !super::desktop_tool_available_by_rollout(AUTONOMOUS_TOOL_DESKTOP_OBSERVE) {
            let output = self.desktop_feature_disabled_output(
                AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
                request.action.as_str(),
                AutonomousDesktopPolicyCategory::ObserveSensitive,
            )?;
            return Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_OBSERVE, output));
        }
        let policy = desktop_observe_policy(&request, operator_approved);
        if policy.decision == AutonomousDesktopPolicyDecision::ApprovalRequired {
            let output = self.desktop_base_output(
                AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
                request.action.as_str(),
                policy,
                AutonomousDesktopToolStatus::ApprovalRequired,
                "Desktop observation requires operator approval before sensitive screen state is captured.",
            )?;
            return Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_OBSERVE, output));
        }
        if policy.decision == AutonomousDesktopPolicyDecision::Denied {
            let output = self.desktop_base_output(
                AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
                request.action.as_str(),
                policy,
                AutonomousDesktopToolStatus::Denied,
                "Desktop observation was denied by policy.",
            )?;
            return Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_OBSERVE, output));
        }

        let output = self.run_desktop_observe(request, policy)?;
        Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_OBSERVE, output))
    }

    pub fn desktop_control(
        &self,
        request: AutonomousDesktopControlRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.desktop_control_with_approval(request, false)
    }

    pub fn desktop_control_with_operator_approval(
        &self,
        request: AutonomousDesktopControlRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.desktop_control_with_approval(request, true)
    }

    fn desktop_control_with_approval(
        &self,
        request: AutonomousDesktopControlRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        self.desktop_control_with_actor_and_approval(
            request,
            AutonomousDesktopActor::Agent,
            operator_approved,
            None,
        )
    }

    pub fn desktop_control_as_actor_with_operator_approval(
        &self,
        request: AutonomousDesktopControlRequest,
        actor: AutonomousDesktopActor,
    ) -> CommandResult<AutonomousToolResult> {
        self.desktop_control_with_actor_and_approval(request, actor, true, None)
    }

    pub fn desktop_control_as_manual_control_with_operator_approval(
        &self,
        request: AutonomousDesktopControlRequest,
        manual_control_id: &str,
    ) -> CommandResult<AutonomousToolResult> {
        validate_non_empty(manual_control_id, "manualControlId")?;
        self.desktop_control_with_actor_and_approval(
            request,
            AutonomousDesktopActor::CloudManualControl,
            true,
            Some(manual_control_id),
        )
    }

    fn desktop_control_with_actor_and_approval(
        &self,
        request: AutonomousDesktopControlRequest,
        actor: AutonomousDesktopActor,
        operator_approved: bool,
        manual_control_lease_id: Option<&str>,
    ) -> CommandResult<AutonomousToolResult> {
        validate_desktop_control_request(&request)?;
        if !super::desktop_tool_available_by_rollout(AUTONOMOUS_TOOL_DESKTOP_CONTROL) {
            let output = self.desktop_feature_disabled_output(
                AUTONOMOUS_TOOL_DESKTOP_CONTROL,
                request.action.as_str(),
                AutonomousDesktopPolicyCategory::ControlDenied,
            )?;
            return Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_CONTROL, output));
        }
        let policy = desktop_control_policy(&request, operator_approved);
        let manual_control_lease_active = if actor == AutonomousDesktopActor::CloudManualControl {
            if let Some(lease_id) = manual_control_lease_id {
                desktop_lock_active_for_actor_and_lease(&self.desktop_control, actor, lease_id)?
            } else {
                desktop_lock_active_for_actor(&self.desktop_control, actor)?
            }
        } else {
            true
        };
        if actor == AutonomousDesktopActor::CloudManualControl && !manual_control_lease_active {
            let mut output = self.desktop_base_output(
                AUTONOMOUS_TOOL_DESKTOP_CONTROL,
                request.action.as_str(),
                desktop_policy(
                    AutonomousDesktopPolicyCategory::ControlDenied,
                    AutonomousDesktopPolicyDecision::Denied,
                    "desktop_policy_manual_control_lease_required",
                    "Cloud manual control input requires an active manual-control lease.",
                    false,
                    true,
                ),
                AutonomousDesktopToolStatus::Denied,
                "Cloud manual control input was denied because the controller lease is not active.",
            )?;
            output.error = Some(desktop_error(
                "manual_control_lease_required",
                "Cloud manual control input requires an active manual-control lease.",
                true,
                true,
                "Request manual control again from the cloud viewport, then retry.",
            ));
            output.audit_id = Some(self.write_desktop_audit(&output, request.reason.as_deref())?);
            return Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_CONTROL, output));
        }
        if policy.decision == AutonomousDesktopPolicyDecision::ApprovalRequired {
            let output = self.desktop_base_output(
                AUTONOMOUS_TOOL_DESKTOP_CONTROL,
                request.action.as_str(),
                policy,
                AutonomousDesktopToolStatus::ApprovalRequired,
                "Desktop control requires operator approval before native input is sent.",
            )?;
            return Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_CONTROL, output));
        }
        if policy.decision == AutonomousDesktopPolicyDecision::Denied {
            let output = self.desktop_base_output(
                AUTONOMOUS_TOOL_DESKTOP_CONTROL,
                request.action.as_str(),
                policy,
                AutonomousDesktopToolStatus::Denied,
                "Desktop control was denied by policy.",
            )?;
            return Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_CONTROL, output));
        }

        let output = self.run_desktop_control(request, actor, policy)?;
        Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_CONTROL, output))
    }

    pub fn desktop_control_status_snapshot(
        &self,
    ) -> CommandResult<AutonomousDesktopControlStatusSnapshot> {
        if !desktop_feature_any_surface_enabled() {
            return Ok(AutonomousDesktopControlStatusSnapshot {
                schema: DESKTOP_STATUS_SCHEMA.into(),
                platform: std::env::consts::OS.into(),
                sidecar: sidecar_unavailable_status(
                    "desktop_feature_disabled",
                    "Computer Use desktop control is disabled by rollout configuration.",
                ),
                capabilities: disabled_desktop_capabilities(),
                permissions: Vec::new(),
                controller_lock: current_desktop_lock(&self.desktop_control)?,
                stream: current_desktop_stream(&self.desktop_control)?,
                updated_at: now_timestamp(),
            });
        }
        Ok(AutonomousDesktopControlStatusSnapshot {
            schema: DESKTOP_STATUS_SCHEMA.into(),
            platform: std::env::consts::OS.into(),
            sidecar: sidecar_status(),
            capabilities: desktop_capabilities(),
            permissions: desktop_permissions(),
            controller_lock: current_desktop_lock(&self.desktop_control)?,
            stream: current_desktop_stream(&self.desktop_control)?,
            updated_at: now_timestamp(),
        })
    }

    pub fn desktop_acquire_manual_control(
        &self,
        manual_control_id: &str,
        reason: Option<&str>,
    ) -> CommandResult<AutonomousDesktopToolOutput> {
        validate_non_empty(manual_control_id, "manualControlId")?;
        if !super::desktop_tool_available_by_rollout(AUTONOMOUS_TOOL_DESKTOP_CONTROL) {
            return self.desktop_feature_disabled_output(
                AUTONOMOUS_TOOL_DESKTOP_CONTROL,
                "manual_control_request",
                AutonomousDesktopPolicyCategory::ControlDenied,
            );
        }
        let policy = desktop_policy(
            AutonomousDesktopPolicyCategory::ControlApprovalRequired,
            AutonomousDesktopPolicyDecision::Allowed,
            "desktop_policy_manual_control_allowed",
            "Cloud manual control was allowed after local opt-in and remote authorization.",
            false,
            false,
        );
        let lock = self.acquire_desktop_lock_for(
            AutonomousDesktopActor::CloudManualControl,
            Some(manual_control_id.to_owned()),
        )?;
        let mut output = self.desktop_base_output(
            AUTONOMOUS_TOOL_DESKTOP_CONTROL,
            "manual_control_request",
            policy,
            AutonomousDesktopToolStatus::Executed,
            "Cloud manual control acquired the desktop controller lock.",
        )?;
        output.controller_lock = Some(lock);
        output.audit_id = Some(self.write_desktop_audit(&output, reason)?);
        Ok(output)
    }

    pub fn desktop_refresh_manual_control(
        &self,
        manual_control_id: &str,
        reason: &str,
    ) -> CommandResult<AutonomousDesktopToolOutput> {
        validate_non_empty(manual_control_id, "manualControlId")?;
        if !super::desktop_tool_available_by_rollout(AUTONOMOUS_TOOL_DESKTOP_CONTROL) {
            return self.desktop_feature_disabled_output(
                AUTONOMOUS_TOOL_DESKTOP_CONTROL,
                "manual_control_heartbeat",
                AutonomousDesktopPolicyCategory::ControlDenied,
            );
        }
        let policy = desktop_policy(
            AutonomousDesktopPolicyCategory::ControlSafe,
            AutonomousDesktopPolicyDecision::Allowed,
            "desktop_policy_manual_control_heartbeat_allowed",
            "Manual-control heartbeats are allowed for an active cloud controller lease.",
            false,
            false,
        );
        let mut output = self.desktop_base_output(
            AUTONOMOUS_TOOL_DESKTOP_CONTROL,
            "manual_control_heartbeat",
            policy,
            AutonomousDesktopToolStatus::Executed,
            "Cloud manual control lease refreshed.",
        )?;
        match self.refresh_desktop_lock(
            AutonomousDesktopActor::CloudManualControl,
            Some(manual_control_id),
        ) {
            Ok(lock) => {
                if let Some(message) = local_user_takeover_message() {
                    let local_lock = self.mark_local_user_takeover()?;
                    output.status = AutonomousDesktopToolStatus::Failed;
                    output.controller_lock = Some(local_lock);
                    output.error = Some(desktop_error(
                        "local_user_takeover",
                        &message,
                        true,
                        true,
                        "Wait for the local user to finish, then request manual control again.",
                    ));
                    output.message = message;
                } else {
                    output.controller_lock = Some(lock);
                }
            }
            Err(error) => {
                output.status = AutonomousDesktopToolStatus::Denied;
                output.error = Some(desktop_error(
                    &error.code,
                    &error.message,
                    true,
                    true,
                    "Request manual control again from the cloud viewport, then retry.",
                ));
                output.message = error.message;
            }
        }
        output.audit_id = Some(self.write_desktop_audit(&output, Some(reason))?);
        Ok(output)
    }

    pub fn desktop_release_manual_control(
        &self,
        manual_control_id: Option<&str>,
        reason: &str,
    ) -> CommandResult<AutonomousDesktopToolOutput> {
        if let Some(manual_control_id) = manual_control_id {
            validate_non_empty(manual_control_id, "manualControlId")?;
        }
        self.release_desktop_lock(reason)?;
        let policy = desktop_policy(
            AutonomousDesktopPolicyCategory::ControlSafe,
            AutonomousDesktopPolicyDecision::Allowed,
            "desktop_policy_manual_control_release_allowed",
            "Releasing desktop control is always allowed.",
            false,
            false,
        );
        let mut output = self.desktop_base_output(
            AUTONOMOUS_TOOL_DESKTOP_CONTROL,
            "manual_control_release",
            policy,
            AutonomousDesktopToolStatus::Stopped,
            "Cloud manual control released the desktop controller lock.",
        )?;
        output.audit_id = Some(self.write_desktop_audit(&output, Some(reason))?);
        Ok(output)
    }

    pub fn desktop_emergency_stop(
        &self,
        reason: &str,
    ) -> CommandResult<AutonomousDesktopControlStatusSnapshot> {
        self.release_desktop_lock(reason)?;
        {
            let mut stream = self.desktop_control.stream.lock().map_err(|_| {
                CommandError::system_fault(
                    "desktop_stream_state_lock_failed",
                    "Xero could not lock desktop stream state.",
                )
            })?;
            stream.status = AutonomousDesktopStreamStatus::Stopped;
            stream.transport = AutonomousDesktopStreamTransport::Unavailable;
            stream.message = format!("Desktop stream stopped by {reason}.");
        }
        let policy = desktop_policy(
            AutonomousDesktopPolicyCategory::ControlSafe,
            AutonomousDesktopPolicyDecision::Allowed,
            "desktop_policy_emergency_stop_allowed",
            "Emergency stop is always allowed.",
            false,
            false,
        );
        let mut output = self.desktop_base_output(
            AUTONOMOUS_TOOL_DESKTOP_CONTROL,
            "emergency_stop",
            policy,
            AutonomousDesktopToolStatus::Stopped,
            "Desktop control emergency stop completed.",
        )?;
        output.audit_id = Some(self.write_desktop_audit(&output, Some(reason))?);
        self.desktop_control_status_snapshot()
    }

    pub fn desktop_stream(
        &self,
        request: AutonomousDesktopStreamRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.desktop_stream_with_approval(request, false)
    }

    pub fn desktop_stream_with_operator_approval(
        &self,
        request: AutonomousDesktopStreamRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.desktop_stream_with_approval(request, true)
    }

    fn desktop_stream_with_approval(
        &self,
        request: AutonomousDesktopStreamRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        validate_desktop_stream_request(&request)?;
        if !super::desktop_tool_available_by_rollout(AUTONOMOUS_TOOL_DESKTOP_STREAM) {
            let output = self.desktop_feature_disabled_output(
                AUTONOMOUS_TOOL_DESKTOP_STREAM,
                request.action.as_str(),
                AutonomousDesktopPolicyCategory::StreamDenied,
            )?;
            return Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_STREAM, output));
        }
        let policy = desktop_stream_policy(&request, operator_approved);
        if policy.decision == AutonomousDesktopPolicyDecision::ApprovalRequired {
            let output = self.desktop_base_output(
                AUTONOMOUS_TOOL_DESKTOP_STREAM,
                request.action.as_str(),
                policy,
                AutonomousDesktopToolStatus::ApprovalRequired,
                "Desktop streaming requires operator approval before screen media is exposed.",
            )?;
            return Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_STREAM, output));
        }
        if policy.decision == AutonomousDesktopPolicyDecision::Denied {
            let output = self.desktop_base_output(
                AUTONOMOUS_TOOL_DESKTOP_STREAM,
                request.action.as_str(),
                policy,
                AutonomousDesktopToolStatus::Denied,
                "Desktop streaming was denied by policy.",
            )?;
            return Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_STREAM, output));
        }

        let output = self.run_desktop_stream(request, policy)?;
        Ok(desktop_result(AUTONOMOUS_TOOL_DESKTOP_STREAM, output))
    }

    fn run_desktop_observe(
        &self,
        request: AutonomousDesktopObserveRequest,
        policy: AutonomousDesktopPolicyTrace,
    ) -> CommandResult<AutonomousDesktopToolOutput> {
        let mut output = self.desktop_base_output(
            AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
            request.action.as_str(),
            policy,
            AutonomousDesktopToolStatus::Executed,
            "Desktop observation completed.",
        )?;

        match request.action {
            AutonomousDesktopObserveAction::PermissionsStatus => {
                output.message = "Desktop permission status returned.".into();
            }
            AutonomousDesktopObserveAction::DisplayList => {
                output.displays = desktop_displays()?;
                output.message = format!("Returned {} desktop display(s).", output.displays.len());
            }
            AutonomousDesktopObserveAction::WindowList => {
                output.windows = desktop_windows()?;
                output.message = format!("Returned {} desktop window(s).", output.windows.len());
            }
            AutonomousDesktopObserveAction::AppList => {
                output.apps = desktop_apps()?;
                output.message = format!("Returned {} desktop app(s).", output.apps.len());
            }
            AutonomousDesktopObserveAction::ForegroundState => {
                output.foreground = foreground_window()?;
                output.message = if output.foreground.is_some() {
                    "Returned foreground desktop state.".into()
                } else {
                    "Foreground desktop state is unavailable on this host.".into()
                };
            }
            AutonomousDesktopObserveAction::Screenshot => {
                let screenshot = capture_desktop_screenshot(&self.repo_root, &request)?;
                output.message = format!(
                    "Captured desktop screenshot {}x{}.",
                    screenshot.width, screenshot.height
                );
                output.screenshot = Some(screenshot);
            }
            AutonomousDesktopObserveAction::CursorState => {
                output.cursor = Some(cursor_state());
                output.message = "Returned desktop cursor state.".into();
            }
            AutonomousDesktopObserveAction::AccessibilitySnapshot => {
                self.attach_accessibility_snapshot(&request, &mut output)?;
            }
            AutonomousDesktopObserveAction::OcrSnapshot => {
                self.attach_ocr_snapshot(&request, &mut output)?;
            }
            AutonomousDesktopObserveAction::ElementAtPoint => {
                match desktop_element_at_point(&request, &output.policy.decision_id) {
                    Ok(snapshot) => {
                        output.structured_snapshot = Some(snapshot);
                        output.message = "Returned desktop Accessibility element at point.".into();
                    }
                    Err(error) => {
                        output.status = if matches!(
                            error.code.as_str(),
                            "sidecar_unavailable" | "sidecar_operation_unimplemented"
                        ) {
                            AutonomousDesktopToolStatus::Unavailable
                        } else {
                            AutonomousDesktopToolStatus::Failed
                        };
                        output.error = Some(desktop_error(
                            &error.code,
                            &error.message,
                            error.retryable,
                            error.user_action_required,
                            "Use screenshot and window_list, then retry after Accessibility is available.",
                        ));
                        output.message = error.message;
                    }
                }
            }
            AutonomousDesktopObserveAction::Health => {
                output.message = "Desktop sidecar contract is healthy.".into();
            }
        }
        output.audit_id = Some(self.write_desktop_audit(&output, None)?);
        Ok(output)
    }

    fn attach_accessibility_snapshot(
        &self,
        request: &AutonomousDesktopObserveRequest,
        output: &mut AutonomousDesktopToolOutput,
    ) -> CommandResult<()> {
        match desktop_accessibility_snapshot(request, &output.policy.decision_id) {
            Ok(snapshot) => {
                let performed = snapshot.performed;
                let diagnostics = snapshot.diagnostics.clone();
                output.structured_snapshot = Some(json!({
                    "schema": "xero.desktop_accessibility_snapshot.v1",
                    "platform": std::env::consts::OS,
                    "performed": performed,
                    "target": snapshot.target,
                    "rows": snapshot.rows,
                    "truncated": snapshot.truncated,
                    "redacted": snapshot.redacted,
                    "diagnostics": diagnostics,
                    "source": "authenticated_sidecar",
                }));
                if performed {
                    output.status = AutonomousDesktopToolStatus::Executed;
                    output.message = "Returned desktop Accessibility snapshot from sidecar.".into();
                } else {
                    output.status = AutonomousDesktopToolStatus::Unavailable;
                    output.error = Some(desktop_error(
                        "permission_accessibility_denied",
                        "Accessibility tree snapshot could not run on this host.",
                        false,
                        true,
                        "Use permissions_status, grant Accessibility locally, then retry.",
                    ));
                    output.message = diagnostics.first().cloned().unwrap_or_else(|| {
                        "Accessibility snapshot is unavailable from this backend.".into()
                    });
                }
                return Ok(());
            }
            Err(error) if sidecar_control_error_allows_fallback(&error) => {}
            Err(error) => {
                output.status = if error.code == "sidecar_operation_unimplemented" {
                    AutonomousDesktopToolStatus::Unavailable
                } else {
                    AutonomousDesktopToolStatus::Failed
                };
                output.error = Some(desktop_error(
                    &error.code,
                    &error.message,
                    error.retryable,
                    error.user_action_required,
                    "Use permissions_status, grant Accessibility locally, then retry.",
                ));
                output.message = error.message;
                return Ok(());
            }
        }

        let diagnostics_request = AutonomousSystemDiagnosticsRequest {
            action: AutonomousSystemDiagnosticsAction::MacosAccessibilitySnapshot,
            preset: None,
            pid: None,
            process_name: None,
            bundle_id: None,
            app_name: None,
            window_id: request
                .window_id
                .as_deref()
                .and_then(|window_id| window_id.parse::<u32>().ok()),
            since: None,
            duration_ms: None,
            interval_ms: None,
            limit: Some(120),
            filter: None,
            include_children: true,
            artifact_mode: Some(AutonomousSystemDiagnosticsArtifactMode::None),
            fd_kinds: Vec::new(),
            include_sockets: false,
            include_files: false,
            include_deleted: false,
            sample_count: None,
            include_ports: false,
            include_threads_summary: false,
            include_wait_channel: false,
            include_stack_hints: false,
            max_artifact_bytes: None,
            last_ms: None,
            level: None,
            subsystem: None,
            category: None,
            message_contains: None,
            process_predicate: None,
            max_depth: Some(5),
            focused_only: request.window_id.is_none(),
            attributes: vec![
                "role".into(),
                "title".into(),
                "value".into(),
                "description".into(),
                "enabled".into(),
                "focused".into(),
                "frame".into(),
            ],
        };
        let diagnostics = self.system_diagnostics_with_operator_approval(diagnostics_request)?;
        let AutonomousToolOutput::SystemDiagnostics(diagnostics_output) = diagnostics.output else {
            return Err(CommandError::system_fault(
                "desktop_accessibility_snapshot_failed",
                "Xero could not decode the Accessibility snapshot backend output.",
            ));
        };
        output.structured_snapshot = Some(json!({
            "schema": "xero.desktop_accessibility_snapshot.v1",
            "platform": std::env::consts::OS,
            "performed": diagnostics_output.performed,
            "target": diagnostics_output.target,
            "rows": diagnostics_output.rows,
            "truncated": diagnostics_output.truncated,
            "redacted": diagnostics_output.redacted,
            "diagnostics": diagnostics_output.diagnostics,
        }));
        output.message = diagnostics_output.summary;
        if diagnostics_output.performed {
            output.status = AutonomousDesktopToolStatus::Executed;
        } else {
            output.status = AutonomousDesktopToolStatus::Unavailable;
            output.error = Some(desktop_error(
                "permission_accessibility_denied",
                "Accessibility tree snapshot could not run on this host.",
                false,
                true,
                "Use permissions_status, grant Accessibility locally, then retry.",
            ));
        }
        Ok(())
    }

    fn attach_ocr_snapshot(
        &self,
        request: &AutonomousDesktopObserveRequest,
        output: &mut AutonomousDesktopToolOutput,
    ) -> CommandResult<()> {
        match desktop_ocr_snapshot(request, &output.policy.decision_id) {
            Ok(snapshot) => {
                let performed = snapshot.performed;
                let diagnostics = snapshot.diagnostics.clone();
                let block_count = snapshot.text_blocks.len();
                output.structured_snapshot = Some(json!({
                    "schema": "xero.desktop_ocr_snapshot.v1",
                    "platform": std::env::consts::OS,
                    "performed": performed,
                    "capturedAt": snapshot.captured_at,
                    "width": snapshot.width,
                    "height": snapshot.height,
                    "scaleFactor": snapshot.scale_factor,
                    "redactionsApplied": snapshot.redactions_applied,
                    "textBlocks": snapshot.text_blocks,
                    "fullText": snapshot.full_text,
                    "truncated": snapshot.truncated,
                    "redacted": snapshot.redacted,
                    "diagnostics": diagnostics,
                    "source": "authenticated_sidecar",
                }));
                if performed {
                    output.status = AutonomousDesktopToolStatus::Executed;
                    output.message =
                        format!("Returned OCR snapshot with {block_count} text block(s).");
                } else {
                    output.status = AutonomousDesktopToolStatus::Unavailable;
                    output.error = Some(desktop_error(
                        "desktop_ocr_unavailable",
                        "OCR snapshot could not run on this host.",
                        false,
                        true,
                        "Use permissions_status, grant Screen Recording locally, then retry.",
                    ));
                    output.message = diagnostics
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "OCR snapshot is unavailable from this backend.".into());
                }
            }
            Err(error) => {
                output.status = if matches!(
                    error.code.as_str(),
                    "sidecar_unavailable" | "sidecar_operation_unimplemented"
                ) {
                    AutonomousDesktopToolStatus::Unavailable
                } else {
                    AutonomousDesktopToolStatus::Failed
                };
                output.error = Some(desktop_error(
                    &error.code,
                    &error.message,
                    error.retryable,
                    error.user_action_required,
                    "Use screenshot or accessibility_snapshot when OCR is unavailable.",
                ));
                output.message = error.message;
            }
        }
        Ok(())
    }

    fn run_desktop_control(
        &self,
        request: AutonomousDesktopControlRequest,
        actor: AutonomousDesktopActor,
        policy: AutonomousDesktopPolicyTrace,
    ) -> CommandResult<AutonomousDesktopToolOutput> {
        let continuing_control = desktop_lock_active_for_actor(&self.desktop_control, actor)?;
        let lock = self.acquire_desktop_lock(actor)?;
        let mut output = self.desktop_base_output(
            AUTONOMOUS_TOOL_DESKTOP_CONTROL,
            request.action.as_str(),
            policy,
            AutonomousDesktopToolStatus::Executed,
            "Desktop control action completed.",
        )?;
        output.controller_lock = Some(lock);

        if continuing_control && actor != AutonomousDesktopActor::LocalUser {
            if let Some(message) = local_user_takeover_message() {
                let local_lock = self.mark_local_user_takeover()?;
                output.controller_lock = Some(local_lock);
                output.status = AutonomousDesktopToolStatus::Failed;
                output.error = Some(desktop_error(
                    "local_user_takeover",
                    &message,
                    true,
                    true,
                    "Wait for the local user to finish, then ask before resuming desktop control.",
                ));
                output.message = message;
                output.audit_id =
                    Some(self.write_desktop_audit(&output, request.reason.as_deref())?);
                return Ok(output);
            }
        }

        let execution = match request.action {
            AutonomousDesktopControlAction::CancelCurrentAction => {
                self.release_desktop_lock("cancel_current_action")?;
                Ok("Cancelled current desktop action and released the controller lock.".into())
            }
            AutonomousDesktopControlAction::MouseMove => {
                if let Some(message) =
                    run_sidecar_desktop_control(&request, &output.policy.decision_id)?
                {
                    Ok(message)
                } else {
                    platform_input::mouse_move(required_point(&request)?)?;
                    Ok("Moved desktop pointer.".into())
                }
            }
            AutonomousDesktopControlAction::MouseClick
            | AutonomousDesktopControlAction::MouseDoubleClick
            | AutonomousDesktopControlAction::MouseRightClick => {
                if let Some(message) =
                    run_sidecar_desktop_control(&request, &output.policy.decision_id)?
                {
                    Ok(message)
                } else {
                    let point = required_point(&request)?;
                    let button = match request.action {
                        AutonomousDesktopControlAction::MouseRightClick => {
                            AutonomousDesktopMouseButton::Right
                        }
                        _ => request.button.unwrap_or_default(),
                    };
                    let clicks = match request.action {
                        AutonomousDesktopControlAction::MouseDoubleClick => 2,
                        _ => request.clicks.unwrap_or(1).max(1),
                    };
                    platform_input::mouse_click(point, button, clicks)?;
                    Ok("Clicked desktop pointer target.".into())
                }
            }
            AutonomousDesktopControlAction::MouseDrag => {
                if let Some(message) =
                    run_sidecar_desktop_control(&request, &output.policy.decision_id)?
                {
                    Ok(message)
                } else {
                    let from = required_point(&request)?;
                    let to = required_target_point(&request)?;
                    platform_input::mouse_drag(from, to)?;
                    Ok("Dragged desktop pointer target.".into())
                }
            }
            AutonomousDesktopControlAction::Scroll => {
                if let Some(message) =
                    run_sidecar_desktop_control(&request, &output.policy.decision_id)?
                {
                    Ok(message)
                } else {
                    platform_input::scroll(
                        request.delta_x.unwrap_or(0),
                        request.delta_y.unwrap_or(0),
                    )?;
                    Ok("Sent desktop scroll input.".into())
                }
            }
            AutonomousDesktopControlAction::KeyPress => {
                if let Some(message) =
                    run_sidecar_desktop_control(&request, &output.policy.decision_id)?
                {
                    Ok(message)
                } else {
                    let key = request
                        .key
                        .as_deref()
                        .ok_or_else(|| CommandError::invalid_request("key"))?;
                    platform_input::key_press(key)?;
                    Ok("Sent desktop key press.".into())
                }
            }
            AutonomousDesktopControlAction::Hotkey => {
                if let Some(message) =
                    run_sidecar_desktop_control(&request, &output.policy.decision_id)?
                {
                    Ok(message)
                } else {
                    platform_input::hotkey(&request.keys)?;
                    Ok("Sent desktop hotkey.".into())
                }
            }
            AutonomousDesktopControlAction::TypeText => {
                if let Some(message) =
                    run_sidecar_desktop_control(&request, &output.policy.decision_id)?
                {
                    Ok(message)
                } else {
                    let text = request
                        .text
                        .as_deref()
                        .ok_or_else(|| CommandError::invalid_request("text"))?;
                    platform_input::type_text(text)?;
                    Ok("Typed text through desktop input.".into())
                }
            }
            AutonomousDesktopControlAction::PasteText => {
                if let Some(message) =
                    run_sidecar_desktop_control(&request, &output.policy.decision_id)?
                {
                    Ok(message)
                } else {
                    Err(CommandError::user_fixable(
                        "sidecar_unavailable",
                        "Clipboard-mediated paste requires the clipboard sidecar backend for the active platform.",
                    ))
                }
            }
            AutonomousDesktopControlAction::FocusWindow
            | AutonomousDesktopControlAction::ActivateApp
            | AutonomousDesktopControlAction::LaunchApp
            | AutonomousDesktopControlAction::QuitApp => {
                let action = match request.action {
                    AutonomousDesktopControlAction::FocusWindow => {
                        AutonomousMacosAutomationAction::MacWindowFocus
                    }
                    AutonomousDesktopControlAction::ActivateApp => {
                        AutonomousMacosAutomationAction::MacAppActivate
                    }
                    AutonomousDesktopControlAction::LaunchApp => {
                        AutonomousMacosAutomationAction::MacAppLaunch
                    }
                    AutonomousDesktopControlAction::QuitApp => {
                        AutonomousMacosAutomationAction::MacAppQuit
                    }
                    _ => unreachable!("desktop app-control action already matched"),
                };
                self.run_desktop_app_automation(&request, action, &mut output)
            }
            AutonomousDesktopControlAction::AxPress
            | AutonomousDesktopControlAction::AxSetValue
            | AutonomousDesktopControlAction::AxFocus => {
                if let Some(message) =
                    run_sidecar_desktop_control(&request, &output.policy.decision_id)?
                {
                    Ok(message)
                } else {
                    Err(CommandError::user_fixable(
                        "sidecar_unavailable",
                        "This desktop action requires a platform Accessibility backend that is not available in the current sidecar.",
                    ))
                }
            }
            AutonomousDesktopControlAction::MenuSelect => {
                if let Some(message) =
                    run_sidecar_desktop_control(&request, &output.policy.decision_id)?
                {
                    Ok(message)
                } else {
                    Err(CommandError::user_fixable(
                        "sidecar_unavailable",
                        "This desktop action requires a platform app-menu backend that is not available in the current sidecar.",
                    ))
                }
            }
        };

        match execution {
            Ok(message) => output.message = message,
            Err(error) => {
                output.status = if error.code == "sidecar_unavailable" {
                    AutonomousDesktopToolStatus::Unavailable
                } else {
                    AutonomousDesktopToolStatus::Failed
                };
                output.error = Some(desktop_error(
                    &error.code,
                    &error.message,
                    false,
                    matches!(error.class, crate::commands::CommandErrorClass::UserFixable),
                    "Observe the current desktop state before retrying or ask the user to intervene.",
                ));
                output.message = error.message;
            }
        }
        output.audit_id = Some(self.write_desktop_audit(&output, request.reason.as_deref())?);
        Ok(output)
    }

    fn run_desktop_app_automation(
        &self,
        request: &AutonomousDesktopControlRequest,
        action: AutonomousMacosAutomationAction,
        output: &mut AutonomousDesktopToolOutput,
    ) -> CommandResult<String> {
        let macos_request = AutonomousMacosAutomationRequest {
            action,
            app_name: request.app_name.clone(),
            bundle_id: request.bundle_id.clone(),
            pid: None,
            window_id: request
                .window_id
                .as_deref()
                .and_then(|window_id| window_id.parse::<u32>().ok()),
            monitor_id: None,
            screenshot_target: None,
        };
        let result = self.macos_automation_with_operator_approval(macos_request)?;
        let AutonomousToolOutput::MacosAutomation(macos_output) = result.output else {
            return Err(CommandError::system_fault(
                "desktop_app_control_failed",
                "Xero could not decode the desktop app-control backend output.",
            ));
        };
        let performed = macos_output.performed;
        let platform_supported = macos_output.platform_supported;
        let message = macos_output.message.clone();
        output.structured_snapshot = Some(json!({
            "schema": "xero.desktop_app_control_result.v1",
            "platform": std::env::consts::OS,
            "performed": performed,
            "apps": macos_output.apps,
            "windows": macos_output.windows,
            "permissions": macos_output.permissions,
            "policy": macos_output.policy,
            "message": message,
        }));
        if performed {
            Ok(message)
        } else if platform_supported {
            Err(CommandError::user_fixable(
                "desktop_app_control_failed",
                message,
            ))
        } else {
            Err(CommandError::user_fixable(
                "sidecar_unavailable",
                "Desktop app launch, activation, quit, and window focus require the platform app-control backend.",
            ))
        }
    }

    fn run_desktop_stream(
        &self,
        request: AutonomousDesktopStreamRequest,
        policy: AutonomousDesktopPolicyTrace,
    ) -> CommandResult<AutonomousDesktopToolOutput> {
        let action = request.action.clone();
        let mut output = self.desktop_base_output(
            AUTONOMOUS_TOOL_DESKTOP_STREAM,
            request.action.as_str(),
            policy,
            AutonomousDesktopToolStatus::Executed,
            "Desktop stream action completed.",
        )?;
        let policy_decision_id = output.policy.decision_id.clone();

        match action {
            AutonomousDesktopStreamAction::StreamCapabilities => {
                output.stream = Some(current_desktop_stream(&self.desktop_control)?);
                output.message = "Returned desktop stream capabilities.".into();
            }
            AutonomousDesktopStreamAction::StreamStart => {
                let session_id = request
                    .session_id
                    .clone()
                    .or_else(|| {
                        self.agent_run_context
                            .as_ref()
                            .map(|context| context.agent_session_id.clone())
                    })
                    .ok_or_else(|| CommandError::invalid_request("sessionId"))?;
                validate_non_empty(&session_id, "sessionId")?;
                let stream_id = request.stream_id.clone().unwrap_or_else(|| {
                    format!(
                        "stream_{}",
                        short_hash(&format!("{}:{}", session_id, now_millis()))
                    )
                });
                let native_result = if output.capabilities.webrtc_stream {
                    Some(run_sidecar_desktop_stream(
                        DesktopSidecarOperation::StreamStart,
                        &request,
                        Some(&session_id),
                        Some(&stream_id),
                        None,
                        &policy_decision_id,
                    ))
                } else {
                    None
                };
                let (next_stream, native_error) = match native_result {
                    Some(Ok(native)) => {
                        output.stream_signal = native.signal;
                        (native.stream, None)
                    }
                    Some(Err(error)) if output.capabilities.screenshot_fallback_stream => (
                        degraded_stream_state(&request, &stream_id, Some(&error)),
                        Some(error),
                    ),
                    Some(Err(error)) => return Err(command_error_from_sidecar(error)),
                    None if output.capabilities.screenshot_fallback_stream => {
                        (degraded_stream_state(&request, &stream_id, None), None)
                    }
                    None => {
                        return Err(CommandError::system_fault(
                            "desktop_stream_unavailable",
                            "No desktop stream transport is available on this host.",
                        ))
                    }
                };
                let next_stream =
                    replace_current_desktop_stream(&self.desktop_control, next_stream)?;
                output.status = AutonomousDesktopToolStatus::Starting;
                output.stream = Some(next_stream.clone());
                output.message = match (next_stream.transport, native_error.as_ref()) {
                    (AutonomousDesktopStreamTransport::WebRtc, _) => {
                        "Started native WebRTC desktop stream.".into()
                    }
                    (_, Some(_)) => {
                        "Started degraded desktop stream state after native stream fallback.".into()
                    }
                    _ => "Started degraded desktop stream state.".into(),
                };
            }
            AutonomousDesktopStreamAction::StreamOffer
            | AutonomousDesktopStreamAction::StreamAnswer
            | AutonomousDesktopStreamAction::StreamIceCandidate => {
                let current = current_desktop_stream(&self.desktop_control)?;
                let operation = desktop_stream_sidecar_operation(&action);
                let native_result = if stream_should_use_sidecar(&current, &output.capabilities) {
                    Some(run_sidecar_desktop_stream(
                        operation,
                        &request,
                        request.session_id.as_deref(),
                        request
                            .stream_id
                            .as_deref()
                            .or(current.stream_id.as_deref()),
                        Some(&current),
                        &policy_decision_id,
                    ))
                } else {
                    None
                };
                let next_stream = match native_result {
                    Some(Ok(native)) => {
                        output.stream_signal = native.signal;
                        native.stream
                    }
                    Some(Err(error)) if output.capabilities.screenshot_fallback_stream => {
                        let mut stream = current;
                        stream.status = AutonomousDesktopStreamStatus::Degraded;
                        stream.transport = AutonomousDesktopStreamTransport::ScreenshotFallback;
                        stream.message = degraded_stream_message(Some(&error));
                        stream
                    }
                    Some(Err(error)) => return Err(command_error_from_sidecar(error)),
                    None => current,
                };
                output.stream = Some(replace_current_desktop_stream(
                    &self.desktop_control,
                    next_stream,
                )?);
                output.message = match action {
                    AutonomousDesktopStreamAction::StreamOffer => {
                        "Processed desktop stream offer signaling.".into()
                    }
                    AutonomousDesktopStreamAction::StreamAnswer => {
                        "Processed desktop stream answer signaling.".into()
                    }
                    AutonomousDesktopStreamAction::StreamIceCandidate => {
                        "Processed desktop stream ICE candidate signaling.".into()
                    }
                    _ => unreachable!("handled by outer match arm"),
                };
            }
            AutonomousDesktopStreamAction::StreamStop => {
                let current = current_desktop_stream(&self.desktop_control)?;
                let native_result = if stream_should_use_sidecar(&current, &output.capabilities) {
                    Some(run_sidecar_desktop_stream(
                        DesktopSidecarOperation::StreamStop,
                        &request,
                        request.session_id.as_deref(),
                        request
                            .stream_id
                            .as_deref()
                            .or(current.stream_id.as_deref()),
                        Some(&current),
                        &policy_decision_id,
                    ))
                } else {
                    None
                };
                let next_stream = match native_result {
                    Some(Ok(native)) => {
                        output.stream_signal = native.signal;
                        native.stream
                    }
                    Some(Err(error)) => stopped_stream_state(
                        current,
                        Some(format!(
                            "Desktop stream stopped locally after native stream stop failed: {}",
                            error.message
                        )),
                    ),
                    None => stopped_stream_state(current, None),
                };
                let next_stream =
                    replace_current_desktop_stream(&self.desktop_control, next_stream)?;
                output.status = AutonomousDesktopToolStatus::Stopped;
                output.stream = Some(next_stream);
                output.message = "Stopped desktop stream.".into();
            }
            AutonomousDesktopStreamAction::StreamStatus => {
                let current = current_desktop_stream(&self.desktop_control)?;
                let next_stream = refresh_native_stream_state(
                    &request,
                    &current,
                    &output.capabilities,
                    &policy_decision_id,
                )?;
                let next_stream = if next_stream != current {
                    replace_current_desktop_stream(&self.desktop_control, next_stream)?
                } else {
                    next_stream
                };
                output.stream = Some(next_stream);
                output.message = "Returned desktop stream status.".into();
            }
            AutonomousDesktopStreamAction::StreamSetQuality => {
                let current = current_desktop_stream(&self.desktop_control)?;
                let native_result = if stream_should_use_sidecar(&current, &output.capabilities) {
                    Some(run_sidecar_desktop_stream(
                        DesktopSidecarOperation::StreamSetQuality,
                        &request,
                        request.session_id.as_deref(),
                        request
                            .stream_id
                            .as_deref()
                            .or(current.stream_id.as_deref()),
                        Some(&current),
                        &policy_decision_id,
                    ))
                } else {
                    None
                };
                let next_stream = match native_result {
                    Some(Ok(native)) => {
                        output.stream_signal = native.signal;
                        native.stream
                    }
                    Some(Err(error)) if output.capabilities.screenshot_fallback_stream => {
                        let mut stream = apply_stream_quality_update(current, &request);
                        stream.status = AutonomousDesktopStreamStatus::Degraded;
                        stream.transport = AutonomousDesktopStreamTransport::ScreenshotFallback;
                        stream.message = degraded_stream_message(Some(&error));
                        stream
                    }
                    Some(Err(error)) => return Err(command_error_from_sidecar(error)),
                    None => apply_stream_quality_update(current, &request),
                };
                output.stream = Some(replace_current_desktop_stream(
                    &self.desktop_control,
                    next_stream,
                )?);
                output.message = "Updated desktop stream quality.".into();
            }
            AutonomousDesktopStreamAction::StreamRequestKeyframe => {
                let current = current_desktop_stream(&self.desktop_control)?;
                let native_result = if stream_should_use_sidecar(&current, &output.capabilities) {
                    Some(run_sidecar_desktop_stream(
                        DesktopSidecarOperation::StreamRequestKeyframe,
                        &request,
                        request.session_id.as_deref(),
                        request
                            .stream_id
                            .as_deref()
                            .or(current.stream_id.as_deref()),
                        Some(&current),
                        &policy_decision_id,
                    ))
                } else {
                    None
                };
                let next_stream = match native_result {
                    Some(Ok(native)) => {
                        output.stream_signal = native.signal;
                        native.stream
                    }
                    Some(Err(error)) if output.capabilities.screenshot_fallback_stream => {
                        let mut stream = current;
                        stream.status = AutonomousDesktopStreamStatus::Degraded;
                        stream.transport = AutonomousDesktopStreamTransport::ScreenshotFallback;
                        stream.message = degraded_stream_message(Some(&error));
                        stream
                    }
                    Some(Err(error)) => return Err(command_error_from_sidecar(error)),
                    None => current,
                };
                output.stream = Some(replace_current_desktop_stream(
                    &self.desktop_control,
                    next_stream,
                )?);
                output.message = "Requested desktop stream keyframe or fallback refresh.".into();
            }
        }
        output.audit_id = Some(self.write_desktop_audit(&output, None)?);
        self.write_desktop_stream_session_event(&output)?;
        Ok(output)
    }

    fn desktop_base_output(
        &self,
        tool: &str,
        action: &str,
        policy: AutonomousDesktopPolicyTrace,
        status: AutonomousDesktopToolStatus,
        message: impl Into<String>,
    ) -> CommandResult<AutonomousDesktopToolOutput> {
        Ok(AutonomousDesktopToolOutput {
            tool: tool.into(),
            action: action.into(),
            request_id: format!(
                "req_{}",
                short_hash(&format!("{tool}:{action}:{}", now_millis()))
            ),
            phase: DESKTOP_CONTROL_PHASE.into(),
            status,
            platform: std::env::consts::OS.into(),
            sidecar: sidecar_status(),
            capabilities: desktop_capabilities(),
            permissions: desktop_permissions(),
            policy,
            displays: Vec::new(),
            windows: Vec::new(),
            apps: Vec::new(),
            foreground: None,
            cursor: None,
            screenshot: None,
            stream: None,
            stream_signal: None,
            structured_snapshot: None,
            controller_lock: current_desktop_lock(&self.desktop_control)?,
            audit_id: None,
            error: None,
            message: message.into(),
        })
    }

    fn desktop_feature_disabled_output(
        &self,
        tool: &str,
        action: &str,
        category: AutonomousDesktopPolicyCategory,
    ) -> CommandResult<AutonomousDesktopToolOutput> {
        let message = "Computer Use desktop control is disabled by rollout configuration.";
        Ok(AutonomousDesktopToolOutput {
            tool: tool.into(),
            action: action.into(),
            request_id: format!(
                "req_{}",
                short_hash(&format!("{tool}:{action}:disabled:{}", now_millis()))
            ),
            phase: DESKTOP_CONTROL_PHASE.into(),
            status: AutonomousDesktopToolStatus::Unavailable,
            platform: std::env::consts::OS.into(),
            sidecar: sidecar_unavailable_status("desktop_feature_disabled", message),
            capabilities: disabled_desktop_capabilities(),
            permissions: Vec::new(),
            policy: desktop_policy(
                category,
                AutonomousDesktopPolicyDecision::Denied,
                "desktop_policy_feature_disabled",
                message,
                false,
                true,
            ),
            displays: Vec::new(),
            windows: Vec::new(),
            apps: Vec::new(),
            foreground: None,
            cursor: None,
            screenshot: None,
            stream: Some(current_desktop_stream(&self.desktop_control)?),
            stream_signal: None,
            structured_snapshot: Some(json!({
                "schema": "xero.desktop_control_feature_flag.v1",
                "enabled": false,
                "tool": tool,
                "action": action,
                "env": {
                    "master": super::DESKTOP_FEATURE_MASTER_ENV,
                    "observe": super::DESKTOP_FEATURE_OBSERVE_ENV,
                    "control": super::DESKTOP_FEATURE_CONTROL_ENV,
                    "stream": super::DESKTOP_FEATURE_STREAM_ENV,
                    "rolloutPercent": super::DESKTOP_FEATURE_ROLLOUT_PERCENT_ENV,
                    "rolloutId": super::DESKTOP_FEATURE_ROLLOUT_ID_ENV,
                }
            })),
            controller_lock: current_desktop_lock(&self.desktop_control)?,
            audit_id: None,
            error: Some(desktop_error(
                "desktop_feature_disabled",
                message,
                false,
                true,
                "Enable the Computer Use desktop rollout flag for this host, then retry.",
            )),
            message: message.into(),
        })
    }

    fn acquire_desktop_lock(
        &self,
        actor: AutonomousDesktopActor,
    ) -> CommandResult<AutonomousDesktopControllerLock> {
        self.acquire_desktop_lock_for(actor, None)
    }

    fn acquire_desktop_lock_for(
        &self,
        actor: AutonomousDesktopActor,
        lease_id: Option<String>,
    ) -> CommandResult<AutonomousDesktopControllerLock> {
        let now = now_timestamp();
        let expires_at = timestamp_after(Duration::from_millis(DEFAULT_LOCK_LEASE_MS));
        let session_id = self
            .agent_run_context
            .as_ref()
            .map(|context| context.agent_session_id.clone())
            .unwrap_or_else(|| "local-computer-use".into());
        let run_id = self
            .agent_run_context
            .as_ref()
            .map(|context| context.run_id.clone());
        let mut guard = self.desktop_control.lock.lock().map_err(|_| {
            CommandError::system_fault(
                "desktop_controller_lock_state_failed",
                "Xero could not lock desktop controller state.",
            )
        })?;
        if let Some(existing) = guard.as_ref() {
            if lock_is_active_at(existing, &now) && existing.actor != actor {
                return Err(CommandError::user_fixable(
                    "controller_lock_unavailable",
                    format!(
                        "Desktop control is currently held by {} until {}.",
                        existing.actor.as_str(),
                        existing.expires_at
                    ),
                ));
            }
            if !lock_is_active_at(existing, &now) {
                *guard = None;
            }
        }
        let lock = AutonomousDesktopControllerLock {
            actor,
            lease_id,
            session_id,
            run_id,
            acquired_at: now.clone(),
            expires_at,
            last_input_at: now,
            release_reason: None,
        };
        *guard = Some(lock.clone());
        Ok(lock)
    }

    fn refresh_desktop_lock(
        &self,
        actor: AutonomousDesktopActor,
        lease_id: Option<&str>,
    ) -> CommandResult<AutonomousDesktopControllerLock> {
        let now = now_timestamp();
        let expires_at = timestamp_after(Duration::from_millis(DEFAULT_LOCK_LEASE_MS));
        let mut guard = self.desktop_control.lock.lock().map_err(|_| {
            CommandError::system_fault(
                "desktop_controller_lock_state_failed",
                "Xero could not lock desktop controller state.",
            )
        })?;
        let Some(lock) = guard.as_mut() else {
            return Err(CommandError::user_fixable(
                "controller_lock_unavailable",
                "Desktop control is not currently held by a cloud manual-control user.",
            ));
        };
        if !lock_is_active_at(lock, &now) {
            *guard = None;
            return Err(CommandError::user_fixable(
                "controller_lock_unavailable",
                "The cloud manual-control lease expired.",
            ));
        }
        if lock.actor != actor {
            return Err(CommandError::user_fixable(
                "controller_lock_unavailable",
                format!(
                    "Desktop control is currently held by {}.",
                    lock.actor.as_str()
                ),
            ));
        }
        if let Some(lease_id) = lease_id {
            if lock.lease_id.as_deref() != Some(lease_id) {
                return Err(CommandError::user_fixable(
                    "manual_control_lease_mismatch",
                    "The cloud manual-control lease does not match the active controller lock.",
                ));
            }
        }
        lock.last_input_at = now;
        lock.expires_at = expires_at;
        lock.release_reason = None;
        Ok(lock.clone())
    }

    fn mark_local_user_takeover(&self) -> CommandResult<AutonomousDesktopControllerLock> {
        let now = now_timestamp();
        let session_id = self
            .agent_run_context
            .as_ref()
            .map(|context| context.agent_session_id.clone())
            .unwrap_or_else(|| "local-computer-use".into());
        let run_id = self
            .agent_run_context
            .as_ref()
            .map(|context| context.run_id.clone());
        let lock = AutonomousDesktopControllerLock {
            actor: AutonomousDesktopActor::LocalUser,
            lease_id: None,
            session_id,
            run_id,
            acquired_at: now.clone(),
            expires_at: timestamp_after(Duration::from_millis(DEFAULT_LOCK_LEASE_MS)),
            last_input_at: now,
            release_reason: Some("local_user_takeover".into()),
        };
        let mut guard = self.desktop_control.lock.lock().map_err(|_| {
            CommandError::system_fault(
                "desktop_controller_lock_state_failed",
                "Xero could not lock desktop controller state.",
            )
        })?;
        *guard = Some(lock.clone());
        Ok(lock)
    }

    fn release_desktop_lock(&self, reason: &str) -> CommandResult<()> {
        let mut guard = self.desktop_control.lock.lock().map_err(|_| {
            CommandError::system_fault(
                "desktop_controller_lock_state_failed",
                "Xero could not lock desktop controller state.",
            )
        })?;
        if let Some(lock) = guard.as_mut() {
            lock.release_reason = Some(reason.into());
        }
        *guard = None;
        Ok(())
    }

    fn write_desktop_audit(
        &self,
        output: &AutonomousDesktopToolOutput,
        reason: Option<&str>,
    ) -> CommandResult<String> {
        let audit_id = format!(
            "audit_{}",
            short_hash(&format!("{}:{}", output.request_id, now_millis()))
        );
        let app_data = project_app_data_dir_for_repo(&self.repo_root);
        let audit_path = app_data.join(DESKTOP_AUDIT_FILE);
        if let Some(parent) = audit_path.parent() {
            prepare_desktop_metadata_parent(parent).map_err(|error| {
                CommandError::system_fault(
                    "desktop_audit_dir_failed",
                    format!("Xero could not create desktop audit storage: {error}"),
                )
            })?;
        }
        let redacted_summary = redacted_desktop_summary(output, reason);
        let record = json!({
            "schema": "xero.desktop_control_audit.v1",
            "id": audit_id,
            "createdAt": now_timestamp(),
            "sessionId": self.agent_run_context.as_ref().map(|context| context.agent_session_id.as_str()),
            "runId": self.agent_run_context.as_ref().map(|context| context.run_id.as_str()),
            "actorType": output.controller_lock.as_ref().map(|lock| lock.actor.as_str()).unwrap_or("agent"),
            "tool": output.tool,
            "action": output.action,
            "targetApp": output.foreground.as_ref().map(|window| window.app_name.as_str()),
            "targetWindow": output.foreground.as_ref().map(|window| window.title.as_str()),
            "displayId": output.screenshot.as_ref().map(|_| "selected"),
            "policyResult": output.policy.decision,
            "policyDecisionId": output.policy.decision_id,
            "approvalId": if output.policy.approval_required { Some(output.policy.decision_id.as_str()) } else { None },
            "status": output.status,
            "errorCode": output.error.as_ref().map(|error| error.code.as_str()),
            "redactedSummary": redacted_summary,
        });
        let mut file = open_desktop_metadata_append_file(&audit_path).map_err(|error| {
            CommandError::system_fault(
                "desktop_audit_write_failed",
                format!("Xero could not open desktop audit log: {error}"),
            )
        })?;
        writeln!(file, "{record}").map_err(|error| {
            CommandError::system_fault(
                "desktop_audit_write_failed",
                format!("Xero could not write desktop audit log: {error}"),
            )
        })?;
        Ok(audit_id)
    }

    fn write_desktop_stream_session_event(
        &self,
        output: &AutonomousDesktopToolOutput,
    ) -> CommandResult<()> {
        let Some(stream) = output.stream.as_ref() else {
            return Ok(());
        };

        let app_data = project_app_data_dir_for_repo(&self.repo_root);
        let stream_path = app_data.join(DESKTOP_STREAM_SESSIONS_FILE);
        if let Some(parent) = stream_path.parent() {
            prepare_desktop_metadata_parent(parent).map_err(|error| {
                CommandError::system_fault(
                    "desktop_stream_session_dir_failed",
                    format!("Xero could not create desktop stream metadata storage: {error}"),
                )
            })?;
        }

        let event_id = format!(
            "stream_event_{}",
            short_hash(&format!(
                "{}:{}:{}",
                output.request_id,
                output.action,
                now_millis()
            ))
        );
        let record = json!({
            "schema": "xero.desktop_stream_session.v1",
            "id": event_id,
            "createdAt": now_timestamp(),
            "event": desktop_stream_event_name(&output.action),
            "sessionId": self.agent_run_context.as_ref().map(|context| context.agent_session_id.as_str()),
            "runId": self.agent_run_context.as_ref().map(|context| context.run_id.as_str()),
            "streamId": stream.stream_id.as_deref(),
            "transport": stream.transport,
            "status": stream.status,
            "quality": stream.quality,
            "maxWidth": stream.max_width,
            "maxFrameRate": stream.max_frame_rate,
            "includeCursor": stream.include_cursor,
            "action": output.action,
            "auditId": output.audit_id.as_deref(),
            "errorCode": output.error.as_ref().map(|error| error.code.as_str()),
        });
        let mut file = open_desktop_metadata_append_file(&stream_path).map_err(|error| {
            CommandError::system_fault(
                "desktop_stream_session_write_failed",
                format!("Xero could not open desktop stream metadata log: {error}"),
            )
        })?;
        writeln!(file, "{record}").map_err(|error| {
            CommandError::system_fault(
                "desktop_stream_session_write_failed",
                format!("Xero could not write desktop stream metadata log: {error}"),
            )
        })?;
        Ok(())
    }
}

fn prepare_desktop_metadata_parent(parent: &Path) -> std::io::Result<()> {
    fs::create_dir_all(parent)?;
    reject_desktop_metadata_symlink(parent)?;
    harden_desktop_metadata_directory(parent)
}

#[cfg(unix)]
fn reject_desktop_metadata_symlink(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "refusing to use symlinked desktop metadata path `{}`",
                path.display()
            ),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(not(unix))]
fn reject_desktop_metadata_symlink(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn harden_desktop_metadata_directory(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Ok(());
    }
    let current = metadata.permissions().mode() & 0o777;
    if current != DESKTOP_METADATA_DIR_MODE {
        fs::set_permissions(path, fs::Permissions::from_mode(DESKTOP_METADATA_DIR_MODE))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn harden_desktop_metadata_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn open_desktop_metadata_append_file(path: &Path) -> std::io::Result<File> {
    reject_desktop_metadata_symlink(path)?;
    let file = open_desktop_metadata_append_file_inner(path)?;
    harden_desktop_metadata_file(path)?;
    Ok(file)
}

#[cfg(unix)]
fn open_desktop_metadata_append_file_inner(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(DESKTOP_METADATA_FILE_MODE)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(not(unix))]
fn open_desktop_metadata_append_file_inner(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

#[cfg(unix)]
fn harden_desktop_metadata_file(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Ok(());
    }
    let current = metadata.permissions().mode() & 0o777;
    if current != DESKTOP_METADATA_FILE_MODE {
        fs::set_permissions(path, fs::Permissions::from_mode(DESKTOP_METADATA_FILE_MODE))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn harden_desktop_metadata_file(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn desktop_stream_event_name(action: &str) -> &'static str {
    match action {
        "stream_start" => "start",
        "stream_stop" => "stop",
        "stream_status" => "status",
        "stream_set_quality" => "quality",
        "stream_request_keyframe" => "keyframe",
        "stream_capabilities" => "capabilities",
        _ => "unknown",
    }
}

pub fn desktop_result(
    tool_name: &str,
    output: AutonomousDesktopToolOutput,
) -> AutonomousToolResult {
    AutonomousToolResult {
        tool_name: tool_name.into(),
        summary: output.message.clone(),
        command_result: None,
        output: match tool_name {
            AUTONOMOUS_TOOL_DESKTOP_CONTROL => AutonomousToolOutput::DesktopControl(output),
            AUTONOMOUS_TOOL_DESKTOP_STREAM => AutonomousToolOutput::DesktopStream(output),
            _ => AutonomousToolOutput::DesktopObserve(output),
        },
    }
}

pub(crate) fn desktop_action_approval_id(output: &AutonomousDesktopToolOutput) -> String {
    format!(
        "desktop:{}:{}:{}",
        output.tool, output.action, output.policy.decision_id
    )
}

fn validate_desktop_observe_request(
    request: &AutonomousDesktopObserveRequest,
) -> CommandResult<()> {
    validate_optional_id(request.display_id.as_deref(), "displayId")?;
    validate_optional_id(request.window_id.as_deref(), "windowId")?;
    if let Some(redaction) = &request.redaction {
        if redaction.private_regions.len() > MAX_PRIVATE_REGIONS {
            return Err(CommandError::invalid_request("redaction.privateRegions"));
        }
        for region in &redaction.private_regions {
            validate_region(region)?;
        }
    }
    if let Some(region) = &request.region {
        validate_region(region)?;
    }
    if matches!(
        request.action,
        AutonomousDesktopObserveAction::ElementAtPoint
    ) && (request.x.is_none() || request.y.is_none())
    {
        return Err(CommandError::invalid_request("x/y"));
    }
    Ok(())
}

fn validate_desktop_control_request(
    request: &AutonomousDesktopControlRequest,
) -> CommandResult<()> {
    validate_optional_id(request.display_id.as_deref(), "displayId")?;
    validate_optional_id(request.window_id.as_deref(), "windowId")?;
    validate_optional_id(request.element_id.as_deref(), "elementId")?;
    validate_optional_id(request.bundle_id.as_deref(), "bundleId")?;
    validate_optional_id(request.app_name.as_deref(), "appName")?;
    if let Some(reason) = request.reason.as_deref() {
        validate_non_empty(reason, "reason")?;
    }
    if let Some(text) = request.text.as_deref() {
        if text.chars().count() > MAX_TYPE_TEXT_CHARS {
            return Err(CommandError::invalid_request("text"));
        }
    }
    if request.menu_path.len() > MAX_MENU_PATH_SEGMENTS {
        return Err(CommandError::invalid_request("menuPath"));
    }
    for segment in &request.menu_path {
        validate_non_empty(segment, "menuPath")?;
    }

    match request.action {
        AutonomousDesktopControlAction::MouseMove
        | AutonomousDesktopControlAction::MouseClick
        | AutonomousDesktopControlAction::MouseDoubleClick
        | AutonomousDesktopControlAction::MouseRightClick => {
            let _ = required_point(request)?;
        }
        AutonomousDesktopControlAction::MouseDrag => {
            let _ = required_point(request)?;
            let _ = required_target_point(request)?;
        }
        AutonomousDesktopControlAction::Scroll => {
            if request.delta_x.unwrap_or(0) == 0 && request.delta_y.unwrap_or(0) == 0 {
                return Err(CommandError::invalid_request("deltaX/deltaY"));
            }
        }
        AutonomousDesktopControlAction::KeyPress => {
            validate_non_empty(request.key.as_deref().unwrap_or_default(), "key")?;
        }
        AutonomousDesktopControlAction::Hotkey => {
            if request.keys.is_empty() {
                return Err(CommandError::invalid_request("keys"));
            }
            for key in &request.keys {
                validate_non_empty(key, "keys")?;
            }
        }
        AutonomousDesktopControlAction::TypeText | AutonomousDesktopControlAction::PasteText => {
            validate_non_empty(request.text.as_deref().unwrap_or_default(), "text")?;
        }
        AutonomousDesktopControlAction::AxSetValue => {
            validate_non_empty(request.value.as_deref().unwrap_or_default(), "value")?;
            validate_ax_target(request)?;
        }
        AutonomousDesktopControlAction::AxPress | AutonomousDesktopControlAction::AxFocus => {
            validate_ax_target(request)?;
        }
        AutonomousDesktopControlAction::MenuSelect => {
            if request.menu_path.is_empty() {
                return Err(CommandError::invalid_request("menuPath"));
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_ax_target(request: &AutonomousDesktopControlRequest) -> CommandResult<()> {
    if request
        .element_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(());
    }
    if matches!((request.x, request.y), (Some(x), Some(y)) if x >= 0 && y >= 0) {
        return Ok(());
    }
    Err(CommandError::invalid_request("elementId or x/y"))
}

fn validate_desktop_stream_request(request: &AutonomousDesktopStreamRequest) -> CommandResult<()> {
    validate_optional_id(request.session_id.as_deref(), "sessionId")?;
    validate_optional_id(request.run_id.as_deref(), "runId")?;
    validate_optional_id(request.display_id.as_deref(), "displayId")?;
    validate_optional_id(request.stream_id.as_deref(), "streamId")?;
    if matches!(request.action, AutonomousDesktopStreamAction::StreamStart)
        && request
            .max_width
            .is_some_and(|width| !(320..=3840).contains(&width))
    {
        return Err(CommandError::invalid_request("maxWidth"));
    }
    if request
        .max_frame_rate
        .is_some_and(|frame_rate| !(1..=60).contains(&frame_rate))
    {
        return Err(CommandError::invalid_request("maxFrameRate"));
    }
    for server in &request.ice_servers {
        match &server.urls {
            AutonomousDesktopIceServerUrls::One(url) if url.trim().is_empty() => {
                return Err(CommandError::invalid_request("iceServers.urls"));
            }
            AutonomousDesktopIceServerUrls::Many(urls)
                if urls.is_empty() || urls.iter().any(|url| url.trim().is_empty()) =>
            {
                return Err(CommandError::invalid_request("iceServers.urls"));
            }
            _ => {}
        }
        if server
            .credential_type
            .as_deref()
            .is_some_and(|credential_type| !matches!(credential_type, "password" | "oauth"))
        {
            return Err(CommandError::invalid_request("iceServers.credentialType"));
        }
    }
    if matches!(
        request.action,
        AutonomousDesktopStreamAction::StreamOffer | AutonomousDesktopStreamAction::StreamAnswer
    ) {
        let Some(description) = request.session_description.as_ref() else {
            return Err(CommandError::invalid_request("sessionDescription"));
        };
        if description.sdp.trim().is_empty()
            || !matches!(
                description.sdp_type.as_str(),
                "offer" | "answer" | "pranswer"
            )
        {
            return Err(CommandError::invalid_request("sessionDescription"));
        }
    }
    if matches!(
        request.action,
        AutonomousDesktopStreamAction::StreamIceCandidate
    ) {
        let Some(candidate) = request.ice_candidate.as_ref() else {
            return Err(CommandError::invalid_request("iceCandidate"));
        };
        if candidate.candidate.trim().is_empty() {
            return Err(CommandError::invalid_request("iceCandidate.candidate"));
        }
    }
    Ok(())
}

fn validate_optional_id(value: Option<&str>, field: &'static str) -> CommandResult<()> {
    if let Some(value) = value {
        validate_non_empty(value, field)?;
    }
    Ok(())
}

fn validate_region(region: &AutonomousDesktopRegion) -> CommandResult<()> {
    if region.width == 0 || region.height == 0 {
        return Err(CommandError::invalid_request("region"));
    }
    Ok(())
}

fn desktop_observe_policy(
    request: &AutonomousDesktopObserveRequest,
    operator_approved: bool,
) -> AutonomousDesktopPolicyTrace {
    if request.action.sensitive() && !operator_approved {
        return desktop_policy(
            AutonomousDesktopPolicyCategory::ObserveSensitive,
            AutonomousDesktopPolicyDecision::ApprovalRequired,
            "desktop_policy_observe_sensitive_requires_approval",
            "Screenshots, OCR, Accessibility snapshots, and element lookup can expose private desktop content.",
            true,
            true,
        );
    }
    desktop_policy(
        if request.action.sensitive() {
            AutonomousDesktopPolicyCategory::ObserveSensitive
        } else {
            AutonomousDesktopPolicyCategory::ObserveSafe
        },
        AutonomousDesktopPolicyDecision::Allowed,
        "desktop_policy_observe_allowed",
        "Desktop observation is allowed under the active Computer Use policy.",
        false,
        false,
    )
}

fn desktop_control_policy(
    request: &AutonomousDesktopControlRequest,
    operator_approved: bool,
) -> AutonomousDesktopPolicyTrace {
    if let Some(target) = blocked_desktop_target(request) {
        return desktop_policy(
            AutonomousDesktopPolicyCategory::ControlDenied,
            AutonomousDesktopPolicyDecision::Denied,
            "desktop_policy_blocked_target_denied",
            target,
            false,
            true,
        );
    }
    if matches!(
        request.sensitivity,
        Some(AutonomousDesktopTextSensitivity::Secret)
    ) {
        return desktop_policy(
            AutonomousDesktopPolicyCategory::ControlDenied,
            AutonomousDesktopPolicyDecision::Denied,
            "desktop_policy_secret_text_denied",
            "Computer Use cannot type or paste secret text into the desktop.",
            false,
            true,
        );
    }
    if matches!(
        request.action,
        AutonomousDesktopControlAction::CancelCurrentAction
    ) {
        return desktop_policy(
            AutonomousDesktopPolicyCategory::ControlSafe,
            AutonomousDesktopPolicyDecision::Allowed,
            "desktop_policy_cancel_allowed",
            "Cancelling the current action is always allowed.",
            false,
            false,
        );
    }
    if !operator_approved {
        return desktop_policy(
            AutonomousDesktopPolicyCategory::ControlApprovalRequired,
            AutonomousDesktopPolicyDecision::ApprovalRequired,
            "desktop_policy_control_requires_approval",
            "Native desktop input, app control, clipboard use, and Accessibility actions require explicit operator approval.",
            true,
            true,
        );
    }
    desktop_policy(
        AutonomousDesktopPolicyCategory::ControlApprovalRequired,
        AutonomousDesktopPolicyDecision::Allowed,
        "desktop_policy_control_allowed_after_approval",
        "Desktop control was allowed after operator approval.",
        false,
        false,
    )
}

#[derive(Debug, Clone, Copy)]
struct BlockedDesktopTargetRule {
    terms: &'static [&'static str],
    reason: &'static str,
}

const BLOCKED_CREDENTIAL_TARGET_REASON: &str =
    "Desktop control is blocked in password manager, Keychain, credential, and browser-saved-password contexts.";
const BLOCKED_PAYMENT_TARGET_REASON: &str =
    "Desktop control is blocked in purchasing, ordering, payment confirmation, and money-transfer contexts.";
const BLOCKED_FINANCIAL_TARGET_REASON: &str =
    "Desktop control is blocked in banking, brokerage, tax, payroll, crypto, insurance, and wallet contexts.";
const BLOCKED_IDENTITY_TARGET_REASON: &str =
    "Desktop control is blocked in identity verification and account-ownership contexts.";
const BLOCKED_SECURITY_RECOVERY_TARGET_REASON: &str =
    "Desktop control is blocked in MFA, recovery-code, account-recovery, and security-setting contexts.";
const BLOCKED_SYSTEM_PRIVACY_TARGET_REASON: &str =
    "Desktop control is blocked in system privacy and security settings.";

const BLOCKED_DESKTOP_TARGET_RULES: &[BlockedDesktopTargetRule] = &[
    BlockedDesktopTargetRule {
        terms: &["password"],
        reason: BLOCKED_CREDENTIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["credential"],
        reason: BLOCKED_CREDENTIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["passkey"],
        reason: BLOCKED_CREDENTIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["keychain"],
        reason: BLOCKED_CREDENTIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["1password"],
        reason: BLOCKED_CREDENTIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["onepassword"],
        reason: BLOCKED_CREDENTIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["bitwarden"],
        reason: BLOCKED_CREDENTIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["lastpass"],
        reason: BLOCKED_CREDENTIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["dashlane"],
        reason: BLOCKED_CREDENTIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["browser saved password"],
        reason: BLOCKED_CREDENTIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["saved password"],
        reason: BLOCKED_CREDENTIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["payment"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["checkout"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["purchase"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["place order"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["confirm order"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["buy now"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["transfer funds"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["send money"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["credit card"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["card number"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["cvv"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["paypal"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["venmo"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["zelle"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["cash app"],
        reason: BLOCKED_PAYMENT_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["bank"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["banking"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["brokerage"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["broker"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["tax"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["payroll"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["insurance"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["crypto"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["wallet"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["coinbase"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["metamask"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["turbotax"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["h r block"],
        reason: BLOCKED_FINANCIAL_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["identity verification"],
        reason: BLOCKED_IDENTITY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["verify identity"],
        reason: BLOCKED_IDENTITY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["id verification"],
        reason: BLOCKED_IDENTITY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["kyc"],
        reason: BLOCKED_IDENTITY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["passport"],
        reason: BLOCKED_IDENTITY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["driver license"],
        reason: BLOCKED_IDENTITY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["drivers license"],
        reason: BLOCKED_IDENTITY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["ssn"],
        reason: BLOCKED_IDENTITY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["social security"],
        reason: BLOCKED_IDENTITY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["account ownership"],
        reason: BLOCKED_IDENTITY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["mfa"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["2fa"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["two factor"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["totp"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["otp"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["authenticator"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["security key"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["recovery code"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["backup code"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["account recovery"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["security recovery"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["change password"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["reset password"],
        reason: BLOCKED_SECURITY_RECOVERY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["system settings", "privacy"],
        reason: BLOCKED_SYSTEM_PRIVACY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["system settings", "security"],
        reason: BLOCKED_SYSTEM_PRIVACY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["privacy security"],
        reason: BLOCKED_SYSTEM_PRIVACY_TARGET_REASON,
    },
    BlockedDesktopTargetRule {
        terms: &["security privacy"],
        reason: BLOCKED_SYSTEM_PRIVACY_TARGET_REASON,
    },
];

fn blocked_desktop_target(request: &AutonomousDesktopControlRequest) -> Option<&'static str> {
    let haystack = desktop_target_haystack(request);
    BLOCKED_DESKTOP_TARGET_RULES
        .iter()
        .find(|rule| {
            rule.terms
                .iter()
                .all(|term| desktop_target_contains(&haystack, term))
        })
        .map(|rule| rule.reason)
}

fn desktop_target_haystack(request: &AutonomousDesktopControlRequest) -> String {
    let mut values = Vec::new();
    if let Some(value) = request.app_name.as_deref() {
        values.push(value);
    }
    if let Some(value) = request.bundle_id.as_deref() {
        values.push(value);
    }
    if let Some(value) = request.element_id.as_deref() {
        values.push(value);
    }
    for segment in &request.menu_path {
        values.push(segment);
    }
    if let Some(value) = request.reason.as_deref() {
        values.push(value);
    }
    let joined = values.join(" ");
    format!(" {} ", normalize_desktop_policy_text(&joined))
}

fn normalize_desktop_policy_text(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_was_space = true;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            normalized.push(character);
            previous_was_space = false;
        } else if !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }
    if normalized.ends_with(' ') {
        normalized.pop();
    }
    normalized
}

fn desktop_target_contains(haystack: &str, term: &str) -> bool {
    let term = normalize_desktop_policy_text(term);
    if term.is_empty() {
        return false;
    }
    if term.contains(' ') {
        return haystack.contains(&format!(" {term} "));
    }
    haystack.split_whitespace().any(|word| {
        word == term || word.starts_with(&term) || (term.len() > 4 && word.contains(&term))
    })
}

fn desktop_stream_policy(
    request: &AutonomousDesktopStreamRequest,
    operator_approved: bool,
) -> AutonomousDesktopPolicyTrace {
    match request.action {
        AutonomousDesktopStreamAction::StreamCapabilities
        | AutonomousDesktopStreamAction::StreamStatus => desktop_policy(
            AutonomousDesktopPolicyCategory::StreamSafe,
            AutonomousDesktopPolicyDecision::Allowed,
            "desktop_policy_stream_observe_allowed",
            "Reading desktop stream capability or status is safe.",
            false,
            false,
        ),
        AutonomousDesktopStreamAction::StreamStart if !operator_approved => desktop_policy(
            AutonomousDesktopPolicyCategory::StreamApprovalRequired,
            AutonomousDesktopPolicyDecision::ApprovalRequired,
            "desktop_policy_stream_start_requires_approval",
            "Starting a desktop stream exposes live screen media and requires operator approval.",
            true,
            true,
        ),
        _ => desktop_policy(
            AutonomousDesktopPolicyCategory::StreamApprovalRequired,
            AutonomousDesktopPolicyDecision::Allowed,
            "desktop_policy_stream_allowed",
            "Desktop stream action is allowed under the active policy.",
            false,
            false,
        ),
    }
}

fn desktop_policy(
    category: AutonomousDesktopPolicyCategory,
    decision: AutonomousDesktopPolicyDecision,
    code: &'static str,
    reason: &'static str,
    approval_required: bool,
    user_action_required: bool,
) -> AutonomousDesktopPolicyTrace {
    AutonomousDesktopPolicyTrace {
        category,
        decision,
        decision_id: format!("policy_{}", short_hash(&format!("{code}:{}", now_millis()))),
        code: code.into(),
        reason: reason.into(),
        approval_required,
        user_action_required,
    }
}

fn sidecar_status() -> AutonomousDesktopSidecarStatus {
    let manager = DESKTOP_SIDECAR_MANAGER.get_or_init(|| Mutex::new(DesktopSidecarManager::new()));
    match manager.lock() {
        Ok(mut manager) => manager.status(),
        Err(_) => sidecar_unavailable_status(
            "desktop_sidecar_state_lock_failed",
            "Xero could not lock desktop sidecar manager state.",
        ),
    }
}

pub fn shutdown_desktop_control_sidecar() {
    if let Some(manager) = DESKTOP_SIDECAR_MANAGER.get() {
        if let Ok(mut manager) = manager.lock() {
            manager.shutdown();
        }
    }
}

static DESKTOP_SIDECAR_MANAGER: OnceLock<Mutex<DesktopSidecarManager>> = OnceLock::new();

fn sidecar_json_result(
    operation: DesktopSidecarOperation,
    payload: serde_json::Value,
    policy_decision_id: &str,
) -> Result<serde_json::Value, String> {
    sidecar_json_result_with_error(operation, payload, policy_decision_id)
        .map_err(|error| format!("{}: {}", error.code, error.message))
}

fn sidecar_json_result_with_error(
    operation: DesktopSidecarOperation,
    payload: serde_json::Value,
    policy_decision_id: &str,
) -> Result<serde_json::Value, DesktopSidecarErrorBody> {
    let manager = DESKTOP_SIDECAR_MANAGER.get_or_init(|| Mutex::new(DesktopSidecarManager::new()));
    let mut manager = manager.lock().map_err(|_| {
        DesktopSidecarErrorBody::new(
            "sidecar_unavailable",
            "Xero could not lock desktop sidecar manager state.",
            true,
            false,
        )
    })?;
    let response = manager
        .request(operation, payload, policy_decision_id)
        .map_err(|error| {
            DesktopSidecarErrorBody::new(
                "sidecar_unavailable",
                format!("Desktop sidecar request failed: {error}"),
                true,
                false,
            )
        })?;
    if response.ok {
        response.result.ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "sidecar_response_invalid",
                "Desktop sidecar response did not include a result body.",
                true,
                false,
            )
        })
    } else {
        Err(response.error.unwrap_or_else(|| {
            DesktopSidecarErrorBody::new(
                "sidecar_response_invalid",
                "Desktop sidecar request failed without details.",
                true,
                false,
            )
        }))
    }
}

struct DesktopSidecarManager {
    process: Option<DesktopSidecarProcess>,
    last_error: Option<String>,
}

struct DesktopSidecarProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    token: String,
    session_id: String,
    lease_expires_at: String,
    binary_path: PathBuf,
    checksum_verified: bool,
}

impl DesktopSidecarManager {
    fn new() -> Self {
        Self {
            process: None,
            last_error: None,
        }
    }

    fn status(&mut self) -> AutonomousDesktopSidecarStatus {
        match self.ensure_started().and_then(|_| self.health_probe()) {
            Ok(()) => {
                let Some(process) = self.process.as_ref() else {
                    return sidecar_unavailable_status(
                        "sidecar_unavailable",
                        "Desktop sidecar process is not running after startup.",
                    );
                };
                AutonomousDesktopSidecarStatus {
                    schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
                    platform: std::env::consts::OS.into(),
                    transport: "stdio_authenticated_sidecar".into(),
                    authenticated: true,
                    health: "ready".into(),
                    message: format!(
                        "Desktop sidecar is running from {} with a lease expiring at {}{}.",
                        process.binary_path.display(),
                        process.lease_expires_at,
                        if process.checksum_verified {
                            " and a verified checksum"
                        } else {
                            " in development checksum mode"
                        }
                    ),
                }
            }
            Err(error) => {
                self.last_error = Some(error.clone());
                self.shutdown();
                sidecar_unavailable_status("sidecar_unavailable", &error)
            }
        }
    }

    fn ensure_started(&mut self) -> Result<(), String> {
        let should_start = match self.process.as_mut() {
            Some(process) => match process.child.try_wait() {
                Ok(Some(status)) => {
                    self.last_error = Some(format!("desktop sidecar exited with {status}"));
                    true
                }
                Ok(None) => timestamp_has_expired(&process.lease_expires_at),
                Err(error) => {
                    self.last_error = Some(format!("desktop sidecar health check failed: {error}"));
                    true
                }
            },
            None => true,
        };
        if should_start {
            self.shutdown();
            self.start()?;
        }
        Ok(())
    }

    fn start(&mut self) -> Result<(), String> {
        let binary_path = resolve_desktop_sidecar_binary()?;
        let checksum_verified = verify_desktop_sidecar_binary(&binary_path)?;
        let mut command = Command::new(&binary_path);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::runtime::process_tree::configure_process_tree_root(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("Xero could not start the desktop sidecar: {error}"))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Desktop sidecar stdin was unavailable.".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Desktop sidecar stdout was unavailable.".to_string())?;
        let mut stdout = BufReader::new(stdout);
        let token = mint_sidecar_token();
        let session_id = format!("desktop-sidecar-{}", short_hash(&token));
        let lease_expires_at = timestamp_after(Duration::from_millis(DEFAULT_SIDECAR_LEASE_MS));
        let handshake = DesktopSidecarHandshake {
            schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
            protocol: DESKTOP_SIDECAR_PROTOCOL.into(),
            session_id: session_id.clone(),
            run_id: None,
            token_sha256: hash_session_token(&token),
            allowed_operations: DesktopSidecarOperation::all_contract_operations(),
            expires_at: lease_expires_at.clone(),
        };
        write_sidecar_line(&mut stdin, &handshake)?;
        let response = read_sidecar_response(&mut stdout)?;
        validate_sidecar_response(&response, "handshake", DesktopSidecarOperation::Health)
            .map_err(|error| error.to_string())?;
        if !response.ok {
            let message = response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "Desktop sidecar rejected the authenticated handshake.".into());
            return Err(message);
        }
        self.process = Some(DesktopSidecarProcess {
            child,
            stdin,
            stdout,
            token,
            session_id,
            lease_expires_at,
            binary_path,
            checksum_verified,
        });
        self.health_probe()?;
        Ok(())
    }

    fn request(
        &mut self,
        operation: DesktopSidecarOperation,
        payload: serde_json::Value,
        policy_decision_id: &str,
    ) -> Result<DesktopSidecarResponse, String> {
        self.ensure_started()?;
        match self.request_once(operation, payload, policy_decision_id) {
            Ok(response) => Ok(response),
            Err(error) => {
                self.last_error = Some(error.clone());
                self.shutdown();
                Err(error)
            }
        }
    }

    fn request_once(
        &mut self,
        operation: DesktopSidecarOperation,
        payload: serde_json::Value,
        policy_decision_id: &str,
    ) -> Result<DesktopSidecarResponse, String> {
        let Some(process) = self.process.as_mut() else {
            return Err("Desktop sidecar process is not running.".into());
        };
        let request_id = format!(
            "req_{}",
            short_hash(&format!("{operation:?}:{}", now_millis()))
        );
        let request = DesktopSidecarRequest {
            schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
            protocol: DESKTOP_SIDECAR_PROTOCOL.into(),
            request_id: request_id.clone(),
            session_id: process.session_id.clone(),
            run_id: None,
            actor: xero_desktop_control_ipc::DesktopSidecarActor::Agent,
            operation,
            payload,
            policy_decision_id: policy_decision_id.into(),
            auth: DesktopSidecarAuth {
                scheme: DesktopSidecarAuthScheme::BearerSessionToken,
                token: process.token.clone(),
            },
            expires_at: timestamp_after(Duration::from_secs(10)),
        };
        write_sidecar_line(&mut process.stdin, &request)?;
        let response = read_sidecar_response(&mut process.stdout)?;
        if let Err(error) = validate_sidecar_response(&response, &request_id, operation) {
            return Err(error.to_string());
        }
        Ok(response)
    }

    fn health_probe(&mut self) -> Result<(), String> {
        let Some(process) = self.process.as_mut() else {
            return Err("Desktop sidecar process is not running.".into());
        };
        let request = DesktopSidecarRequest {
            schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
            protocol: DESKTOP_SIDECAR_PROTOCOL.into(),
            request_id: format!("req_{}", short_hash(&format!("health:{}", now_millis()))),
            session_id: process.session_id.clone(),
            run_id: None,
            actor: xero_desktop_control_ipc::DesktopSidecarActor::Agent,
            operation: DesktopSidecarOperation::Health,
            payload: json!({}),
            policy_decision_id: "desktop_sidecar_health_probe".into(),
            auth: DesktopSidecarAuth {
                scheme: DesktopSidecarAuthScheme::BearerSessionToken,
                token: process.token.clone(),
            },
            expires_at: timestamp_after(Duration::from_secs(10)),
        };
        write_sidecar_line(&mut process.stdin, &request)?;
        let response = read_sidecar_response(&mut process.stdout)?;
        validate_sidecar_response(
            &response,
            &request.request_id,
            DesktopSidecarOperation::Health,
        )
        .map_err(|error| error.to_string())?;
        if response.ok {
            Ok(())
        } else {
            Err(response
                .error
                .map(|error| error.message)
                .unwrap_or_else(|| "Desktop sidecar health probe failed.".into()))
        }
    }

    fn shutdown(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = crate::runtime::process_tree::terminate_process_tree(&mut process.child);
        }
    }
}

fn sidecar_unavailable_status(code: &str, message: &str) -> AutonomousDesktopSidecarStatus {
    AutonomousDesktopSidecarStatus {
        schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
        platform: std::env::consts::OS.into(),
        transport: "in_process_limited_fallback".into(),
        authenticated: false,
        health: "degraded".into(),
        message: format!(
            "{message} ({code}). Xero will use the limited in-process desktop broker until the authenticated sidecar is available."
        ),
    }
}

fn resolve_desktop_sidecar_binary() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(DESKTOP_SIDECAR_PATH_ENV).map(PathBuf::from) {
        return validate_sidecar_binary_path(path);
    }
    #[cfg(test)]
    {
        return Err(format!(
            "Desktop sidecar auto-discovery is disabled in tests; set {DESKTOP_SIDECAR_PATH_ENV} to exercise the authenticated sidecar."
        ));
    }

    #[cfg(not(test))]
    {
        let binary_name = desktop_sidecar_binary_name();
        let mut candidates = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join(&binary_name));
                candidates.push(dir.join("../Resources").join(&binary_name));
            }
        }
        if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
            let manifest_dir = PathBuf::from(manifest_dir);
            candidates.push(manifest_dir.join("resources").join(&binary_name));
            if let Some(target_dir) = manifest_dir.parent() {
                candidates.push(target_dir.join("target/debug").join(&binary_name));
                candidates.push(target_dir.join("target/release").join(&binary_name));
            }
        }

        candidates
            .into_iter()
            .find_map(|candidate| validate_sidecar_binary_path(candidate).ok())
            .ok_or_else(|| {
                format!(
                    "Bundled desktop sidecar `{}` was not found. Build it with `cargo build --package xero-desktop-sidecar` or set {DESKTOP_SIDECAR_PATH_ENV}.",
                    binary_name
                )
            })
    }
}

#[cfg(not(test))]
fn desktop_sidecar_binary_name() -> String {
    if cfg!(windows) {
        format!("{DESKTOP_SIDECAR_BINARY_NAME}.exe")
    } else {
        DESKTOP_SIDECAR_BINARY_NAME.into()
    }
}

fn validate_sidecar_binary_path(path: PathBuf) -> Result<PathBuf, String> {
    let metadata = fs::metadata(&path).map_err(|error| {
        format!(
            "Desktop sidecar `{}` is unavailable: {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "Desktop sidecar `{}` is not a regular file.",
            path.display()
        ));
    }
    Ok(path)
}

fn verify_desktop_sidecar_binary(path: &PathBuf) -> Result<bool, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "Xero could not read desktop sidecar `{}` for verification: {error}",
            path.display()
        )
    })?;
    let digest = Sha256::digest(&bytes);
    let actual = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let configured_expected = std::env::var(DESKTOP_SIDECAR_SHA256_ENV).ok().or_else(|| {
        option_env!("XERO_BUNDLED_DESKTOP_SIDECAR_SHA256")
            .map(str::to_owned)
            .filter(|value| !value.trim().is_empty())
    });
    match configured_expected {
        Some(expected) if expected.eq_ignore_ascii_case(&actual) => Ok(true),
        Some(expected) => Err(format!(
            "Desktop sidecar checksum mismatch for `{}`: expected {}, got {}.",
            path.display(),
            expected,
            actual
        )),
        None if cfg!(debug_assertions) => Ok(false),
        None => Err(format!(
            "Desktop sidecar checksum is required in release builds. Set {DESKTOP_SIDECAR_SHA256_ENV} for `{}`.",
            path.display()
        )),
    }
}

fn mint_sidecar_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_sidecar_line<T: Serialize>(stdin: &mut ChildStdin, value: &T) -> Result<(), String> {
    serde_json::to_writer(&mut *stdin, value)
        .map_err(|error| format!("Xero could not encode desktop sidecar IPC: {error}"))?;
    stdin
        .write_all(b"\n")
        .and_then(|_| stdin.flush())
        .map_err(|error| format!("Xero could not write desktop sidecar IPC: {error}"))
}

fn read_sidecar_response(
    stdout: &mut BufReader<std::process::ChildStdout>,
) -> Result<DesktopSidecarResponse, String> {
    let mut line = String::new();
    stdout
        .read_line(&mut line)
        .map_err(|error| format!("Xero could not read desktop sidecar IPC: {error}"))?;
    if line.trim().is_empty() {
        return Err("Desktop sidecar closed before sending a response.".into());
    }
    serde_json::from_str(&line)
        .map_err(|error| format!("Xero could not decode desktop sidecar response: {error}"))
}

fn desktop_capabilities() -> AutonomousDesktopCapabilities {
    if let Ok(payload) = sidecar_json_result(
        DesktopSidecarOperation::Capabilities,
        json!({}),
        "desktop_sidecar_capabilities",
    ) {
        if let Ok(capabilities) = serde_json::from_value::<DesktopSidecarCapabilities>(payload) {
            return merge_desktop_capabilities(
                capabilities.into(),
                in_process_desktop_capabilities(),
            );
        }
    }
    in_process_desktop_capabilities()
}

fn merge_desktop_capabilities(
    sidecar: AutonomousDesktopCapabilities,
    fallback: AutonomousDesktopCapabilities,
) -> AutonomousDesktopCapabilities {
    AutonomousDesktopCapabilities {
        platform: fallback.platform,
        schema_version: sidecar.schema_version,
        display_list: sidecar.display_list || fallback.display_list,
        screenshot: sidecar.screenshot || fallback.screenshot,
        window_list: sidecar.window_list || fallback.window_list,
        app_list: sidecar.app_list || fallback.app_list,
        foreground_state: sidecar.foreground_state || fallback.foreground_state,
        cursor_state: sidecar.cursor_state || fallback.cursor_state,
        accessibility_snapshot: sidecar.accessibility_snapshot || fallback.accessibility_snapshot,
        ocr_snapshot: sidecar.ocr_snapshot || fallback.ocr_snapshot,
        mouse_input: sidecar.mouse_input || fallback.mouse_input,
        keyboard_input: sidecar.keyboard_input || fallback.keyboard_input,
        clipboard: sidecar.clipboard || fallback.clipboard,
        accessibility_actions: sidecar.accessibility_actions || fallback.accessibility_actions,
        menu_select: sidecar.menu_select || fallback.menu_select,
        webrtc_stream: sidecar.webrtc_stream || fallback.webrtc_stream,
        screenshot_fallback_stream: sidecar.screenshot_fallback_stream
            || fallback.screenshot_fallback_stream,
        manual_cloud_control: sidecar.manual_cloud_control || fallback.manual_cloud_control,
    }
}

fn in_process_desktop_capabilities() -> AutonomousDesktopCapabilities {
    AutonomousDesktopCapabilities {
        platform: std::env::consts::OS.into(),
        schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
        display_list: true,
        screenshot: true,
        window_list: true,
        app_list: true,
        foreground_state: true,
        cursor_state: cfg!(target_os = "macos"),
        accessibility_snapshot: cfg!(target_os = "macos"),
        ocr_snapshot: false,
        mouse_input: cfg!(target_os = "macos"),
        keyboard_input: cfg!(target_os = "macos"),
        clipboard: false,
        accessibility_actions: false,
        menu_select: false,
        webrtc_stream: false,
        screenshot_fallback_stream: true,
        manual_cloud_control: cfg!(target_os = "macos"),
    }
}

fn disabled_desktop_capabilities() -> AutonomousDesktopCapabilities {
    AutonomousDesktopCapabilities {
        platform: std::env::consts::OS.into(),
        schema_version: DESKTOP_SIDECAR_SCHEMA_VERSION,
        display_list: false,
        screenshot: false,
        window_list: false,
        app_list: false,
        foreground_state: false,
        cursor_state: false,
        accessibility_snapshot: false,
        ocr_snapshot: false,
        mouse_input: false,
        keyboard_input: false,
        clipboard: false,
        accessibility_actions: false,
        menu_select: false,
        webrtc_stream: false,
        screenshot_fallback_stream: false,
        manual_cloud_control: false,
    }
}

fn desktop_feature_any_surface_enabled() -> bool {
    [
        AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
        AUTONOMOUS_TOOL_DESKTOP_CONTROL,
        AUTONOMOUS_TOOL_DESKTOP_STREAM,
    ]
    .into_iter()
    .any(super::desktop_tool_available_by_rollout)
}

fn desktop_permissions() -> Vec<AutonomousDesktopPermissionStatus> {
    if let Ok(payload) = sidecar_json_result(
        DesktopSidecarOperation::PermissionsStatus,
        json!({}),
        "desktop_sidecar_permissions_status",
    ) {
        if let Ok(payload) = serde_json::from_value::<DesktopSidecarPermissionsPayload>(payload) {
            let sidecar_permissions = payload
                .permissions
                .into_iter()
                .map(AutonomousDesktopPermissionStatus::from)
                .collect();
            return merge_desktop_permissions(
                sidecar_permissions,
                in_process_desktop_permissions(),
            );
        }
    }
    in_process_desktop_permissions()
}

fn merge_desktop_permissions(
    mut sidecar: Vec<AutonomousDesktopPermissionStatus>,
    fallback: Vec<AutonomousDesktopPermissionStatus>,
) -> Vec<AutonomousDesktopPermissionStatus> {
    for fallback_permission in fallback {
        if let Some(existing) = sidecar
            .iter_mut()
            .find(|permission| permission.name == fallback_permission.name)
        {
            for required_for in fallback_permission.required_for {
                if !existing.required_for.contains(&required_for) {
                    existing.required_for.push(required_for);
                }
            }
        } else {
            sidecar.push(fallback_permission);
        }
    }

    sidecar
}

fn in_process_desktop_permissions() -> Vec<AutonomousDesktopPermissionStatus> {
    vec![
        permission(
            "Screen Recording",
            if cfg!(any(
                target_os = "macos",
                target_os = "windows",
                target_os = "linux"
            )) {
                AutonomousDesktopPermissionGrant::Unknown
            } else {
                AutonomousDesktopPermissionGrant::Unsupported
            },
            &["screenshot", "stream"],
            "Grant screen capture permission in the local desktop session, then refresh status.",
        ),
        permission(
            "Accessibility",
            if cfg!(target_os = "macos") {
                AutonomousDesktopPermissionGrant::Unknown
            } else {
                AutonomousDesktopPermissionGrant::Unsupported
            },
            &[
                "mouse",
                "keyboard",
                "accessibility_snapshot",
                "accessibility_actions",
            ],
            "Grant Accessibility permission to Xero in local system privacy settings.",
        ),
        permission(
            "Input Monitoring",
            if cfg!(target_os = "macos") {
                AutonomousDesktopPermissionGrant::Unknown
            } else {
                AutonomousDesktopPermissionGrant::Unsupported
            },
            &["keyboard", "hotkey"],
            "Grant Input Monitoring only if the selected keyboard backend requires it.",
        ),
        permission(
            "Remote Desktop Portal",
            if cfg!(target_os = "linux") {
                AutonomousDesktopPermissionGrant::Unknown
            } else {
                AutonomousDesktopPermissionGrant::Unsupported
            },
            &["wayland_capture", "wayland_input"],
            "Approve the Wayland portal prompt in the local desktop session.",
        ),
    ]
}

fn permission(
    name: &str,
    status: AutonomousDesktopPermissionGrant,
    required_for: &[&str],
    remediation: &str,
) -> AutonomousDesktopPermissionStatus {
    AutonomousDesktopPermissionStatus {
        name: name.into(),
        status,
        required_for: required_for.iter().map(|value| (*value).into()).collect(),
        remediation: remediation.into(),
    }
}

fn desktop_displays() -> CommandResult<Vec<AutonomousDesktopDisplay>> {
    if let Ok(payload) = sidecar_json_result(
        DesktopSidecarOperation::DisplayList,
        json!({}),
        "desktop_sidecar_display_list",
    ) {
        if let Ok(payload) = serde_json::from_value::<DesktopSidecarDisplayListPayload>(payload) {
            return Ok(payload
                .displays
                .into_iter()
                .map(AutonomousDesktopDisplay::from)
                .collect());
        }
    }
    let monitors = Monitor::all().map_err(|error| {
        CommandError::system_fault(
            "sidecar_unavailable",
            format!("Xero could not enumerate desktop displays: {error}"),
        )
    })?;
    monitors
        .iter()
        .map(|monitor| {
            Ok(AutonomousDesktopDisplay {
                display_id: monitor.id().unwrap_or_default().to_string(),
                name: monitor.name().unwrap_or_else(|_| "Display".into()),
                x: monitor.x().unwrap_or_default(),
                y: monitor.y().unwrap_or_default(),
                width: monitor.width().unwrap_or_default(),
                height: monitor.height().unwrap_or_default(),
                scale_factor: monitor.scale_factor().unwrap_or(1.0),
                rotation: monitor.rotation().unwrap_or_default(),
                primary: monitor.is_primary().unwrap_or(false),
            })
        })
        .collect()
}

fn desktop_windows() -> CommandResult<Vec<AutonomousDesktopWindow>> {
    if let Ok(payload) = sidecar_json_result(
        DesktopSidecarOperation::WindowList,
        json!({}),
        "desktop_sidecar_window_list",
    ) {
        if let Ok(payload) = serde_json::from_value::<DesktopSidecarWindowListPayload>(payload) {
            return Ok(payload
                .windows
                .into_iter()
                .map(AutonomousDesktopWindow::from)
                .collect());
        }
    }
    let windows = Window::all().map_err(|error| {
        CommandError::system_fault(
            "sidecar_unavailable",
            format!("Xero could not enumerate desktop windows: {error}"),
        )
    })?;
    Ok(windows
        .iter()
        .filter_map(|window| {
            let width = window.width().ok()?;
            let height = window.height().ok()?;
            if width == 0 || height == 0 {
                return None;
            }
            Some(AutonomousDesktopWindow {
                window_id: window.id().unwrap_or_default().to_string(),
                app_name: window.app_name().unwrap_or_else(|_| "Unknown".into()),
                title: redact_sensitive_label(&window.title().unwrap_or_default()),
                pid: window.pid().unwrap_or_default(),
                x: window.x().unwrap_or_default(),
                y: window.y().unwrap_or_default(),
                width,
                height,
                z: window.z().unwrap_or_default(),
                focused: window.is_focused().unwrap_or(false),
                minimized: window.is_minimized().unwrap_or(false),
            })
        })
        .collect())
}

fn desktop_apps() -> CommandResult<Vec<AutonomousDesktopApp>> {
    if let Ok(payload) = sidecar_json_result(
        DesktopSidecarOperation::AppList,
        json!({}),
        "desktop_sidecar_app_list",
    ) {
        if let Ok(payload) = serde_json::from_value::<DesktopSidecarAppListPayload>(payload) {
            return Ok(payload
                .apps
                .into_iter()
                .map(AutonomousDesktopApp::from)
                .collect());
        }
    }
    let mut apps: BTreeMap<(String, u32), AutonomousDesktopApp> = BTreeMap::new();
    for window in desktop_windows()? {
        let key = (window.app_name.clone(), window.pid);
        let entry = apps.entry(key).or_insert_with(|| AutonomousDesktopApp {
            app_name: window.app_name.clone(),
            pid: window.pid,
            window_count: 0,
            focused: false,
        });
        entry.window_count += 1;
        entry.focused |= window.focused;
    }
    Ok(apps.into_values().collect())
}

fn foreground_window() -> CommandResult<Option<AutonomousDesktopWindow>> {
    if let Ok(payload) = sidecar_json_result(
        DesktopSidecarOperation::ForegroundState,
        json!({}),
        "desktop_sidecar_foreground_state",
    ) {
        if let Ok(payload) = serde_json::from_value::<DesktopSidecarForegroundStatePayload>(payload)
        {
            return Ok(payload.foreground.map(AutonomousDesktopWindow::from));
        }
    }
    Ok(desktop_windows()?.into_iter().find(|window| window.focused))
}

fn desktop_element_at_point(
    request: &AutonomousDesktopObserveRequest,
    policy_decision_id: &str,
) -> Result<serde_json::Value, DesktopSidecarErrorBody> {
    let payload =
        serde_json::to_value(sidecar_element_at_point_request(request)).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "desktop_element_at_point_request_encode_failed",
                format!(
                    "Xero could not encode the desktop element-at-point sidecar request: {error}"
                ),
                false,
                false,
            )
        })?;
    let payload = sidecar_json_result_with_error(
        DesktopSidecarOperation::ElementAtPoint,
        payload,
        policy_decision_id,
    )?;
    let decoded = serde_json::from_value::<DesktopSidecarElementAtPointPayload>(payload).map_err(
        |error| {
            DesktopSidecarErrorBody::new(
                "desktop_element_at_point_decode_failed",
                format!(
                    "Xero could not decode the desktop element-at-point sidecar response: {error}"
                ),
                true,
                false,
            )
        },
    )?;
    serde_json::to_value(json!({
        "schema": "xero.desktop_element_at_point.v1",
        "platform": std::env::consts::OS,
        "x": decoded.x,
        "y": decoded.y,
        "available": decoded.available,
        "element": decoded.element,
        "storage": "ephemeral"
    }))
    .map_err(|error| {
        DesktopSidecarErrorBody::new(
            "desktop_element_at_point_encode_failed",
            format!("Xero could not encode the desktop element-at-point output: {error}"),
            false,
            false,
        )
    })
}

fn desktop_accessibility_snapshot(
    request: &AutonomousDesktopObserveRequest,
    policy_decision_id: &str,
) -> Result<DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarErrorBody> {
    let payload =
        serde_json::to_value(sidecar_accessibility_snapshot_request(request)).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "desktop_accessibility_snapshot_request_encode_failed",
                format!(
                    "Xero could not encode the desktop Accessibility snapshot sidecar request: {error}"
                ),
                false,
                false,
            )
        })?;
    let payload = sidecar_json_result_with_error(
        DesktopSidecarOperation::AccessibilitySnapshot,
        payload,
        policy_decision_id,
    )?;
    serde_json::from_value::<DesktopSidecarAccessibilitySnapshotPayload>(payload).map_err(|error| {
        DesktopSidecarErrorBody::new(
            "desktop_accessibility_snapshot_decode_failed",
            format!("Xero could not decode the desktop Accessibility snapshot sidecar response: {error}"),
            true,
            false,
        )
        })
}

fn desktop_ocr_snapshot(
    request: &AutonomousDesktopObserveRequest,
    policy_decision_id: &str,
) -> Result<DesktopSidecarOcrSnapshotPayload, DesktopSidecarErrorBody> {
    let payload = sidecar_json_result_with_error(
        DesktopSidecarOperation::OcrSnapshot,
        serde_json::to_value(sidecar_ocr_snapshot_request(request)).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "desktop_ocr_snapshot_request_encode_failed",
                format!("Xero could not encode the desktop OCR sidecar request: {error}"),
                false,
                false,
            )
        })?,
        policy_decision_id,
    )?;
    serde_json::from_value::<DesktopSidecarOcrSnapshotPayload>(payload).map_err(|error| {
        DesktopSidecarErrorBody::new(
            "desktop_ocr_snapshot_decode_failed",
            format!("Xero could not decode the desktop OCR sidecar response: {error}"),
            false,
            false,
        )
    })
}

fn sidecar_accessibility_snapshot_request(
    request: &AutonomousDesktopObserveRequest,
) -> DesktopSidecarAccessibilitySnapshotRequest {
    DesktopSidecarAccessibilitySnapshotRequest {
        window_id: request.window_id.clone(),
        focused_only: request.window_id.is_none(),
        include_children: true,
        max_depth: Some(5),
        limit: Some(120),
    }
}

fn sidecar_ocr_snapshot_request(
    request: &AutonomousDesktopObserveRequest,
) -> DesktopSidecarOcrSnapshotRequest {
    DesktopSidecarOcrSnapshotRequest {
        display_id: request.display_id.clone(),
        region: request.region.as_ref().map(|region| DesktopSidecarRegion {
            x: region.x,
            y: region.y,
            width: region.width,
            height: region.height,
        }),
        redaction: request.redaction.as_ref().map(sidecar_redaction_request),
        limit: Some(200),
    }
}

fn sidecar_element_at_point_request(
    request: &AutonomousDesktopObserveRequest,
) -> DesktopSidecarPointRequest {
    DesktopSidecarPointRequest {
        x: request.x.unwrap_or_default(),
        y: request.y.unwrap_or_default(),
    }
}

fn capture_desktop_screenshot(
    repo_root: &PathBuf,
    request: &AutonomousDesktopObserveRequest,
) -> CommandResult<AutonomousDesktopScreenshot> {
    if let Ok(payload) = sidecar_json_result(
        DesktopSidecarOperation::Screenshot,
        serde_json::to_value(sidecar_screenshot_request(request)).map_err(|error| {
            CommandError::system_fault(
                "desktop_screenshot_request_encode_failed",
                format!("Xero could not encode the desktop screenshot sidecar request: {error}"),
            )
        })?,
        "desktop_sidecar_screenshot",
    ) {
        if let Ok(payload) = serde_json::from_value::<DesktopSidecarScreenshotPayload>(payload) {
            if let Ok(screenshot) = write_sidecar_screenshot_artifact(repo_root, request, payload) {
                return Ok(screenshot);
            }
        }
    }
    let displays = Monitor::all().map_err(|error| {
        CommandError::system_fault(
            "permission_screen_recording_denied",
            format!("Xero could not capture desktop displays: {error}"),
        )
    })?;
    let monitor = select_monitor(&displays, request.display_id.as_deref())?;
    let scale_factor = monitor.scale_factor().unwrap_or(1.0);
    let mut image = if let Some(region) = &request.region {
        monitor
            .capture_region(region.x, region.y, region.width, region.height)
            .map_err(|error| {
                CommandError::system_fault(
                    "coordinates_out_of_bounds",
                    format!("Xero could not capture the requested desktop region: {error}"),
                )
            })?
    } else {
        monitor.capture_image().map_err(|error| {
            CommandError::system_fault(
                "permission_screen_recording_denied",
                format!("Xero could not capture the desktop screenshot: {error}"),
            )
        })?
    };
    let redactions_applied =
        apply_private_region_redactions(&mut image, request.redaction.as_ref());
    let mut bytes = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|error| {
            CommandError::system_fault(
                "desktop_screenshot_encode_failed",
                format!("Xero could not encode the desktop screenshot: {error}"),
            )
        })?;
    let screenshot_dir = project_app_data_dir_for_repo(repo_root).join(DESKTOP_AUDIT_DIR);
    fs::create_dir_all(&screenshot_dir).map_err(|error| {
        CommandError::system_fault(
            "desktop_screenshot_dir_failed",
            format!("Xero could not create desktop screenshot storage: {error}"),
        )
    })?;
    let path = screenshot_dir.join(format!(
        "screenshot-{}-{}.png",
        monitor.id().unwrap_or_default(),
        now_millis()
    ));
    fs::write(&path, bytes).map_err(|error| {
        CommandError::system_fault(
            "desktop_screenshot_write_failed",
            format!("Xero could not write desktop screenshot: {error}"),
        )
    })?;
    Ok(AutonomousDesktopScreenshot {
        path: path.to_string_lossy().into_owned(),
        width: image.width(),
        height: image.height(),
        scale_factor,
        captured_at: now_timestamp(),
        redactions_applied,
    })
}

fn sidecar_screenshot_request(
    request: &AutonomousDesktopObserveRequest,
) -> DesktopSidecarScreenshotRequest {
    DesktopSidecarScreenshotRequest {
        display_id: request.display_id.clone(),
        region: request.region.as_ref().map(|region| DesktopSidecarRegion {
            x: region.x,
            y: region.y,
            width: region.width,
            height: region.height,
        }),
        redaction: request.redaction.as_ref().map(sidecar_redaction_request),
    }
}

fn sidecar_redaction_request(
    redaction: &AutonomousDesktopRedactionRequest,
) -> DesktopSidecarRedactionRequest {
    DesktopSidecarRedactionRequest {
        mode: match &redaction.mode {
            AutonomousDesktopRedactionMode::Off => DesktopSidecarRedactionMode::Off,
            AutonomousDesktopRedactionMode::Balanced => DesktopSidecarRedactionMode::Balanced,
            AutonomousDesktopRedactionMode::Auto => DesktopSidecarRedactionMode::Auto,
            AutonomousDesktopRedactionMode::Strict => DesktopSidecarRedactionMode::Strict,
        },
        private_regions: redaction
            .private_regions
            .iter()
            .map(|region| DesktopSidecarRegion {
                x: region.x,
                y: region.y,
                width: region.width,
                height: region.height,
            })
            .collect(),
    }
}

fn write_sidecar_screenshot_artifact(
    repo_root: &PathBuf,
    request: &AutonomousDesktopObserveRequest,
    payload: DesktopSidecarScreenshotPayload,
) -> CommandResult<AutonomousDesktopScreenshot> {
    if payload.media_type != "image/png" {
        return Err(CommandError::system_fault(
            "desktop_screenshot_media_type_invalid",
            format!(
                "Desktop sidecar returned unsupported screenshot media type `{}`.",
                payload.media_type
            ),
        ));
    }
    let bytes = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(payload.bytes_base64.as_bytes())
            .map_err(|error| {
                CommandError::system_fault(
                    "desktop_screenshot_decode_failed",
                    format!("Xero could not decode the desktop sidecar screenshot: {error}"),
                )
            })?
    };
    let screenshot_dir = project_app_data_dir_for_repo(repo_root).join(DESKTOP_AUDIT_DIR);
    fs::create_dir_all(&screenshot_dir).map_err(|error| {
        CommandError::system_fault(
            "desktop_screenshot_dir_failed",
            format!("Xero could not create desktop screenshot storage: {error}"),
        )
    })?;
    let display_id = request.display_id.as_deref().unwrap_or("selected");
    let path = screenshot_dir.join(format!(
        "screenshot-{}-{}.png",
        short_hash(display_id),
        now_millis()
    ));
    fs::write(&path, bytes).map_err(|error| {
        CommandError::system_fault(
            "desktop_screenshot_write_failed",
            format!("Xero could not write desktop sidecar screenshot: {error}"),
        )
    })?;
    Ok(AutonomousDesktopScreenshot {
        path: path.to_string_lossy().into_owned(),
        width: payload.width,
        height: payload.height,
        scale_factor: payload.scale_factor,
        captured_at: payload.captured_at,
        redactions_applied: payload.redactions_applied,
    })
}

fn apply_private_region_redactions(
    image: &mut RgbaImage,
    redaction: Option<&AutonomousDesktopRedactionRequest>,
) -> usize {
    let Some(redaction) = redaction else {
        return 0;
    };

    let width = image.width();
    let height = image.height();
    let mut applied = 0;
    for region in &redaction.private_regions {
        let x_start = region.x.min(width);
        let y_start = region.y.min(height);
        let x_end = region.x.saturating_add(region.width).min(width);
        let y_end = region.y.saturating_add(region.height).min(height);
        if x_start >= x_end || y_start >= y_end {
            continue;
        }
        for y in y_start..y_end {
            for x in x_start..x_end {
                image.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        applied += 1;
    }
    applied
}

fn select_monitor<'a>(
    monitors: &'a [Monitor],
    display_id: Option<&str>,
) -> CommandResult<&'a Monitor> {
    if let Some(display_id) = display_id {
        for monitor in monitors {
            if monitor
                .id()
                .map(|id| id.to_string() == display_id)
                .unwrap_or(false)
            {
                return Ok(monitor);
            }
        }
        return Err(CommandError::user_fixable(
            "display_not_found",
            format!("Xero could not find desktop display `{display_id}`."),
        ));
    }
    monitors
        .iter()
        .find(|monitor| monitor.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or_else(|| {
            CommandError::user_fixable(
                "display_not_found",
                "Xero could not find a desktop display.",
            )
        })
}

fn cursor_state() -> AutonomousDesktopCursorState {
    if let Ok(payload) = sidecar_json_result(
        DesktopSidecarOperation::CursorState,
        json!({}),
        "desktop_sidecar_cursor_state",
    ) {
        if let Ok(payload) = serde_json::from_value::<DesktopSidecarCursorStatePayload>(payload) {
            return payload.into();
        }
    }

    #[cfg(target_os = "macos")]
    {
        use core_graphics::event::CGEvent;
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        if let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) {
            if let Ok(event) = CGEvent::new(source) {
                let point = event.location();
                return AutonomousDesktopCursorState {
                    x: point.x as i32,
                    y: point.y as i32,
                    display_id: Monitor::from_point(point.x as i32, point.y as i32)
                        .ok()
                        .and_then(|monitor| monitor.id().ok())
                        .map(|id| id.to_string()),
                    available: true,
                };
            }
        }
    }
    AutonomousDesktopCursorState {
        x: 0,
        y: 0,
        display_id: None,
        available: false,
    }
}

fn current_desktop_lock(
    state: &DesktopControlState,
) -> CommandResult<Option<AutonomousDesktopControllerLock>> {
    let now = now_timestamp();
    let mut guard = state.lock.lock().map_err(|_| {
        CommandError::system_fault(
            "desktop_controller_lock_state_failed",
            "Xero could not lock desktop controller state.",
        )
    })?;
    if guard
        .as_ref()
        .is_some_and(|lock| !lock_is_active_at(lock, &now))
    {
        *guard = None;
    }
    Ok(guard.clone())
}

fn desktop_lock_active_for_actor(
    state: &DesktopControlState,
    actor: AutonomousDesktopActor,
) -> CommandResult<bool> {
    desktop_lock_active_for_actor_with_lease(state, actor, None)
}

fn desktop_lock_active_for_actor_and_lease(
    state: &DesktopControlState,
    actor: AutonomousDesktopActor,
    lease_id: &str,
) -> CommandResult<bool> {
    desktop_lock_active_for_actor_with_lease(state, actor, Some(lease_id))
}

fn desktop_lock_active_for_actor_with_lease(
    state: &DesktopControlState,
    actor: AutonomousDesktopActor,
    lease_id: Option<&str>,
) -> CommandResult<bool> {
    let now = now_timestamp();
    let mut guard = state.lock.lock().map_err(|_| {
        CommandError::system_fault(
            "desktop_controller_lock_state_failed",
            "Xero could not lock desktop controller state.",
        )
    })?;
    if guard
        .as_ref()
        .is_some_and(|lock| !lock_is_active_at(lock, &now))
    {
        *guard = None;
        return Ok(false);
    }
    Ok(guard.as_ref().is_some_and(|lock| {
        lock.actor == actor
            && lock_is_active_at(lock, &now)
            && lease_id.map_or(true, |lease_id| lock.lease_id.as_deref() == Some(lease_id))
    }))
}

fn lock_is_active_at(lock: &AutonomousDesktopControllerLock, now: &str) -> bool {
    lock.expires_at.as_str() > now
}

fn current_desktop_stream(
    state: &DesktopControlState,
) -> CommandResult<AutonomousDesktopStreamState> {
    state.stream.lock().map(|guard| guard.clone()).map_err(|_| {
        CommandError::system_fault(
            "desktop_stream_state_lock_failed",
            "Xero could not lock desktop stream state.",
        )
    })
}

fn local_user_takeover_message() -> Option<String> {
    local_user_recent_input(Duration::from_millis(750)).then(|| {
        "Local desktop input was detected, so Xero paused brokered desktop control.".into()
    })
}

#[cfg(target_os = "macos")]
fn local_user_recent_input(threshold: Duration) -> bool {
    use core_graphics::{event::CGEventType, event_source::CGEventSourceStateID};

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(
            state_id: CGEventSourceStateID,
            event_type: CGEventType,
        ) -> f64;
    }

    let threshold_secs = threshold.as_secs_f64();
    let event_types = [
        CGEventType::MouseMoved,
        CGEventType::LeftMouseDown,
        CGEventType::RightMouseDown,
        CGEventType::OtherMouseDown,
        CGEventType::ScrollWheel,
        CGEventType::KeyDown,
        CGEventType::FlagsChanged,
    ];
    event_types.into_iter().any(|event_type| {
        let seconds = unsafe {
            CGEventSourceSecondsSinceLastEventType(CGEventSourceStateID::HIDSystemState, event_type)
        };
        seconds.is_finite() && seconds >= 0.0 && seconds <= threshold_secs
    })
}

#[cfg(not(target_os = "macos"))]
fn local_user_recent_input(_threshold: Duration) -> bool {
    false
}

fn default_stream_state() -> AutonomousDesktopStreamState {
    AutonomousDesktopStreamState {
        stream_id: None,
        status: AutonomousDesktopStreamStatus::Idle,
        transport: AutonomousDesktopStreamTransport::Unavailable,
        signaling_channel: None,
        quality: AutonomousDesktopStreamQuality::Balanced,
        max_width: 1280,
        max_frame_rate: 2,
        include_cursor: true,
        message: "Desktop stream is idle.".into(),
    }
}

#[derive(Debug, Clone, Copy)]
struct StreamQualityProfile {
    max_width: u32,
    max_frame_rate: u32,
}

fn stream_quality_profile(quality: AutonomousDesktopStreamQuality) -> StreamQualityProfile {
    match quality {
        AutonomousDesktopStreamQuality::Low => StreamQualityProfile {
            max_width: 960,
            max_frame_rate: 1,
        },
        AutonomousDesktopStreamQuality::Balanced => StreamQualityProfile {
            max_width: 1280,
            max_frame_rate: 1,
        },
        AutonomousDesktopStreamQuality::High => StreamQualityProfile {
            max_width: 1920,
            max_frame_rate: 2,
        },
    }
}

fn webrtc_stream_quality_profile(quality: AutonomousDesktopStreamQuality) -> StreamQualityProfile {
    match quality {
        AutonomousDesktopStreamQuality::Low => StreamQualityProfile {
            max_width: 960,
            max_frame_rate: 15,
        },
        AutonomousDesktopStreamQuality::Balanced => StreamQualityProfile {
            max_width: 1280,
            max_frame_rate: 24,
        },
        AutonomousDesktopStreamQuality::High => StreamQualityProfile {
            max_width: 1920,
            max_frame_rate: 30,
        },
    }
}

fn run_sidecar_desktop_stream(
    operation: DesktopSidecarOperation,
    request: &AutonomousDesktopStreamRequest,
    session_id: Option<&str>,
    stream_id: Option<&str>,
    current: Option<&AutonomousDesktopStreamState>,
    policy_decision_id: &str,
) -> Result<AutonomousDesktopStreamSidecarOutput, DesktopSidecarErrorBody> {
    let payload = serde_json::to_value(sidecar_stream_request(
        request, session_id, stream_id, current,
    ))
    .map_err(|error| {
        DesktopSidecarErrorBody::new(
            "desktop_stream_request_encode_failed",
            format!("Xero could not encode the desktop stream sidecar request: {error}"),
            false,
            false,
        )
    })?;
    let payload = sidecar_json_result_with_error(operation, payload, policy_decision_id)?;
    let decoded =
        serde_json::from_value::<DesktopSidecarStreamPayload>(payload).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "desktop_stream_decode_failed",
                format!("Xero could not decode the desktop stream sidecar response: {error}"),
                true,
                false,
            )
        })?;
    Ok(decoded.into())
}

fn desktop_stream_sidecar_operation(
    action: &AutonomousDesktopStreamAction,
) -> DesktopSidecarOperation {
    match action {
        AutonomousDesktopStreamAction::StreamCapabilities => {
            DesktopSidecarOperation::StreamCapabilities
        }
        AutonomousDesktopStreamAction::StreamStart => DesktopSidecarOperation::StreamStart,
        AutonomousDesktopStreamAction::StreamOffer => DesktopSidecarOperation::StreamOffer,
        AutonomousDesktopStreamAction::StreamAnswer => DesktopSidecarOperation::StreamAnswer,
        AutonomousDesktopStreamAction::StreamIceCandidate => {
            DesktopSidecarOperation::StreamIceCandidate
        }
        AutonomousDesktopStreamAction::StreamStop => DesktopSidecarOperation::StreamStop,
        AutonomousDesktopStreamAction::StreamStatus => DesktopSidecarOperation::StreamStatus,
        AutonomousDesktopStreamAction::StreamSetQuality => {
            DesktopSidecarOperation::StreamSetQuality
        }
        AutonomousDesktopStreamAction::StreamRequestKeyframe => {
            DesktopSidecarOperation::StreamRequestKeyframe
        }
    }
}

fn sidecar_stream_request(
    request: &AutonomousDesktopStreamRequest,
    session_id: Option<&str>,
    stream_id: Option<&str>,
    current: Option<&AutonomousDesktopStreamState>,
) -> DesktopSidecarStreamRequest {
    let quality = request
        .quality
        .or_else(|| current.map(|stream| stream.quality))
        .unwrap_or(AutonomousDesktopStreamQuality::Balanced);
    let profile = webrtc_stream_quality_profile(quality);
    DesktopSidecarStreamRequest {
        session_id: session_id
            .map(ToOwned::to_owned)
            .or_else(|| request.session_id.clone()),
        run_id: request.run_id.clone(),
        display_id: request.display_id.clone(),
        stream_id: stream_id
            .map(ToOwned::to_owned)
            .or_else(|| request.stream_id.clone())
            .or_else(|| current.and_then(|stream| stream.stream_id.clone())),
        max_width: Some(
            request
                .max_width
                .or_else(|| current.map(|stream| stream.max_width))
                .unwrap_or(profile.max_width)
                .clamp(640, 7680),
        ),
        max_frame_rate: Some(
            request
                .max_frame_rate
                .or_else(|| current.map(|stream| stream.max_frame_rate))
                .unwrap_or(profile.max_frame_rate)
                .clamp(1, 120),
        ),
        include_cursor: Some(
            request
                .include_cursor
                .or_else(|| current.map(|stream| stream.include_cursor))
                .unwrap_or(true),
        ),
        quality: Some(sidecar_stream_quality(quality)),
        redaction: request.redaction.as_ref().map(sidecar_redaction_request),
        ice_servers: sidecar_ice_servers(&request.ice_servers),
        session_description: request
            .session_description
            .as_ref()
            .map(sidecar_session_description),
        ice_candidate: request.ice_candidate.as_ref().map(sidecar_ice_candidate),
    }
}

fn sidecar_stream_quality(quality: AutonomousDesktopStreamQuality) -> DesktopSidecarStreamQuality {
    match quality {
        AutonomousDesktopStreamQuality::Low => DesktopSidecarStreamQuality::Low,
        AutonomousDesktopStreamQuality::Balanced => DesktopSidecarStreamQuality::Balanced,
        AutonomousDesktopStreamQuality::High => DesktopSidecarStreamQuality::High,
    }
}

fn sidecar_ice_servers(servers: &[AutonomousDesktopIceServer]) -> Vec<DesktopSidecarIceServer> {
    servers
        .iter()
        .map(|server| DesktopSidecarIceServer {
            urls: match &server.urls {
                AutonomousDesktopIceServerUrls::One(url) => {
                    DesktopSidecarIceServerUrls::One(url.clone())
                }
                AutonomousDesktopIceServerUrls::Many(urls) => {
                    DesktopSidecarIceServerUrls::Many(urls.clone())
                }
            },
            username: server.username.clone(),
            credential: server.credential.clone(),
            credential_type: server.credential_type.clone(),
        })
        .collect()
}

fn sidecar_session_description(
    description: &AutonomousDesktopSessionDescription,
) -> DesktopSidecarSessionDescription {
    DesktopSidecarSessionDescription {
        sdp_type: description.sdp_type.clone(),
        sdp: description.sdp.clone(),
    }
}

fn sidecar_ice_candidate(candidate: &AutonomousDesktopIceCandidate) -> DesktopSidecarIceCandidate {
    DesktopSidecarIceCandidate {
        candidate: candidate.candidate.clone(),
        sdp_mid: candidate.sdp_mid.clone(),
        sdp_m_line_index: candidate.sdp_m_line_index,
        username_fragment: candidate.username_fragment.clone(),
    }
}

fn degraded_stream_state(
    request: &AutonomousDesktopStreamRequest,
    stream_id: &str,
    native_error: Option<&DesktopSidecarErrorBody>,
) -> AutonomousDesktopStreamState {
    let quality = request
        .quality
        .unwrap_or(AutonomousDesktopStreamQuality::Balanced);
    let profile = stream_quality_profile(quality);
    AutonomousDesktopStreamState {
        stream_id: Some(stream_id.into()),
        status: AutonomousDesktopStreamStatus::Degraded,
        transport: AutonomousDesktopStreamTransport::ScreenshotFallback,
        signaling_channel: Some("computer_use_stream".into()),
        quality,
        max_width: request
            .max_width
            .unwrap_or(profile.max_width)
            .clamp(640, 1920),
        max_frame_rate: request
            .max_frame_rate
            .unwrap_or(profile.max_frame_rate)
            .clamp(1, 30),
        include_cursor: request.include_cursor.unwrap_or(true),
        message: degraded_stream_message(native_error),
    }
}

fn degraded_stream_message(native_error: Option<&DesktopSidecarErrorBody>) -> String {
    let base = "Native WebRTC publisher is unavailable; screenshot fallback is available through desktop_observe.screenshot.";
    match native_error {
        Some(error) => format!("{base} Native stream error code: {}.", error.code),
        None => base.into(),
    }
}

fn stopped_stream_state(
    mut current: AutonomousDesktopStreamState,
    message: Option<String>,
) -> AutonomousDesktopStreamState {
    current.status = AutonomousDesktopStreamStatus::Stopped;
    current.message = message.unwrap_or_else(|| "Desktop stream stopped.".into());
    current
}

fn refresh_native_stream_state(
    request: &AutonomousDesktopStreamRequest,
    current: &AutonomousDesktopStreamState,
    capabilities: &AutonomousDesktopCapabilities,
    policy_decision_id: &str,
) -> CommandResult<AutonomousDesktopStreamState> {
    if !stream_should_use_sidecar(current, capabilities) {
        return Ok(current.clone());
    }
    match run_sidecar_desktop_stream(
        DesktopSidecarOperation::StreamStatus,
        request,
        request.session_id.as_deref(),
        request
            .stream_id
            .as_deref()
            .or(current.stream_id.as_deref()),
        Some(current),
        policy_decision_id,
    ) {
        Ok(native) => Ok(native.stream),
        Err(error) if capabilities.screenshot_fallback_stream => {
            let mut stream = current.clone();
            stream.status = AutonomousDesktopStreamStatus::Degraded;
            stream.transport = AutonomousDesktopStreamTransport::ScreenshotFallback;
            stream.message = degraded_stream_message(Some(&error));
            Ok(stream)
        }
        Err(error) => Err(command_error_from_sidecar(error)),
    }
}

fn replace_current_desktop_stream(
    state: &DesktopControlState,
    next: AutonomousDesktopStreamState,
) -> CommandResult<AutonomousDesktopStreamState> {
    let mut stream = state.stream.lock().map_err(|_| {
        CommandError::system_fault(
            "desktop_stream_state_lock_failed",
            "Xero could not lock desktop stream state.",
        )
    })?;
    *stream = next;
    Ok(stream.clone())
}

fn stream_should_use_sidecar(
    current: &AutonomousDesktopStreamState,
    capabilities: &AutonomousDesktopCapabilities,
) -> bool {
    capabilities.webrtc_stream
        && current.transport == AutonomousDesktopStreamTransport::WebRtc
        && !matches!(
            current.status,
            AutonomousDesktopStreamStatus::Idle | AutonomousDesktopStreamStatus::Stopped
        )
}

fn apply_stream_quality_update(
    mut stream: AutonomousDesktopStreamState,
    request: &AutonomousDesktopStreamRequest,
) -> AutonomousDesktopStreamState {
    if let Some(quality) = request.quality {
        stream.quality = quality;
        let profile = stream_quality_profile(quality);
        if request.max_width.is_none() {
            stream.max_width = profile.max_width;
        }
        if request.max_frame_rate.is_none() {
            stream.max_frame_rate = profile.max_frame_rate;
        }
    }
    if let Some(max_width) = request.max_width {
        stream.max_width = max_width.clamp(640, 1920);
    }
    if let Some(max_frame_rate) = request.max_frame_rate {
        stream.max_frame_rate = max_frame_rate.clamp(1, 30);
    }
    stream
}

fn run_sidecar_desktop_control(
    request: &AutonomousDesktopControlRequest,
    policy_decision_id: &str,
) -> CommandResult<Option<String>> {
    let Some(operation) = desktop_control_sidecar_operation(&request.action) else {
        return Ok(None);
    };
    let payload = serde_json::to_value(sidecar_control_request(request)).map_err(|error| {
        CommandError::system_fault(
            "desktop_control_request_encode_failed",
            format!("Xero could not encode the desktop control sidecar request: {error}"),
        )
    })?;
    match sidecar_json_result_with_error(operation, payload, policy_decision_id) {
        Ok(result) => Ok(Some(
            result
                .get("message")
                .and_then(|message| message.as_str())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| {
                    format!("Desktop sidecar executed {}.", request.action.as_str())
                }),
        )),
        Err(error) if sidecar_control_error_allows_fallback(&error) => Ok(None),
        Err(error) => Err(command_error_from_sidecar(error)),
    }
}

fn sidecar_control_error_allows_fallback(error: &DesktopSidecarErrorBody) -> bool {
    matches!(
        error.code.as_str(),
        "sidecar_unavailable" | "sidecar_operation_unimplemented"
    )
}

fn command_error_from_sidecar(error: DesktopSidecarErrorBody) -> CommandError {
    if error.user_action_required {
        CommandError::user_fixable(error.code, error.message)
    } else {
        CommandError::system_fault(error.code, error.message)
    }
}

fn desktop_control_sidecar_operation(
    action: &AutonomousDesktopControlAction,
) -> Option<DesktopSidecarOperation> {
    Some(match action {
        AutonomousDesktopControlAction::MouseMove => DesktopSidecarOperation::MouseMove,
        AutonomousDesktopControlAction::MouseClick => DesktopSidecarOperation::MouseClick,
        AutonomousDesktopControlAction::MouseDoubleClick => {
            DesktopSidecarOperation::MouseDoubleClick
        }
        AutonomousDesktopControlAction::MouseRightClick => DesktopSidecarOperation::MouseRightClick,
        AutonomousDesktopControlAction::MouseDrag => DesktopSidecarOperation::MouseDrag,
        AutonomousDesktopControlAction::Scroll => DesktopSidecarOperation::Scroll,
        AutonomousDesktopControlAction::KeyPress => DesktopSidecarOperation::KeyPress,
        AutonomousDesktopControlAction::Hotkey => DesktopSidecarOperation::Hotkey,
        AutonomousDesktopControlAction::TypeText => DesktopSidecarOperation::TypeText,
        AutonomousDesktopControlAction::PasteText => DesktopSidecarOperation::PasteText,
        AutonomousDesktopControlAction::AxPress => DesktopSidecarOperation::AxPress,
        AutonomousDesktopControlAction::AxSetValue => DesktopSidecarOperation::AxSetValue,
        AutonomousDesktopControlAction::AxFocus => DesktopSidecarOperation::AxFocus,
        AutonomousDesktopControlAction::MenuSelect => DesktopSidecarOperation::MenuSelect,
        AutonomousDesktopControlAction::CancelCurrentAction => {
            DesktopSidecarOperation::CancelCurrentAction
        }
        _ => return None,
    })
}

fn sidecar_control_request(
    request: &AutonomousDesktopControlRequest,
) -> DesktopSidecarControlRequest {
    DesktopSidecarControlRequest {
        element_id: request.element_id.clone(),
        x: request.x,
        y: request.y,
        to_x: request.to_x,
        to_y: request.to_y,
        delta_x: request.delta_x,
        delta_y: request.delta_y,
        button: request.button.map(|button| match button {
            AutonomousDesktopMouseButton::Left => DesktopSidecarMouseButton::Left,
            AutonomousDesktopMouseButton::Right => DesktopSidecarMouseButton::Right,
            AutonomousDesktopMouseButton::Middle => DesktopSidecarMouseButton::Middle,
        }),
        clicks: request.clicks,
        key: request.key.clone(),
        keys: request.keys.clone(),
        text: request.text.clone(),
        value: request.value.clone(),
        menu_path: request.menu_path.clone(),
    }
}

fn required_point(request: &AutonomousDesktopControlRequest) -> CommandResult<(i32, i32)> {
    match (request.x, request.y) {
        (Some(x), Some(y)) if x >= 0 && y >= 0 => Ok((x, y)),
        _ => Err(CommandError::invalid_request("x/y")),
    }
}

fn required_target_point(request: &AutonomousDesktopControlRequest) -> CommandResult<(i32, i32)> {
    match (request.to_x, request.to_y) {
        (Some(x), Some(y)) if x >= 0 && y >= 0 => Ok((x, y)),
        _ => Err(CommandError::invalid_request("toX/toY")),
    }
}

fn desktop_error(
    code: &str,
    message: &str,
    retryable: bool,
    user_action_required: bool,
    safe_next_action: &str,
) -> AutonomousDesktopToolError {
    AutonomousDesktopToolError {
        code: code.into(),
        message: message.into(),
        retryable,
        user_action_required,
        safe_next_action: safe_next_action.into(),
    }
}

fn redacted_desktop_summary(output: &AutonomousDesktopToolOutput, reason: Option<&str>) -> String {
    let reason = reason.map(redact_sensitive_label).unwrap_or_default();
    let status = serde_json::to_string(&output.status).unwrap_or_else(|_| "unknown".into());
    if reason.is_empty() {
        format!("{} {} {}", output.tool, output.action, status)
    } else {
        format!(
            "{} {} {} reason={}",
            output.tool, output.action, status, reason
        )
    }
}

fn redact_sensitive_label(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("password")
        || lower.contains("secret")
        || lower.contains("token")
        || lower.contains("recovery")
        || lower.contains("keychain")
        || lower.contains("wallet")
        || lower.contains("mfa")
    {
        "[redacted sensitive desktop label]".into()
    } else {
        value.chars().take(240).collect()
    }
}

fn now_timestamp() -> String {
    crate::auth::now_timestamp()
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn timestamp_after(duration: Duration) -> String {
    let millis = now_millis().saturating_add(duration.as_millis());
    let seconds = (millis / 1_000) as i64;
    let nanos = ((millis % 1_000) as i64) * 1_000_000;
    match time::OffsetDateTime::from_unix_timestamp(seconds)
        .and_then(|timestamp| timestamp.replace_nanosecond(nanos as u32))
    {
        Ok(timestamp) => timestamp
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| now_timestamp()),
        Err(_) => now_timestamp(),
    }
}

fn timestamp_has_expired(value: &str) -> bool {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map(|timestamp| time::OffsetDateTime::now_utc() >= timestamp)
        .unwrap_or(true)
}

fn short_hash(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(input.as_bytes());
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

mod platform_input {
    use super::{AutonomousDesktopMouseButton, CommandError, CommandResult};

    #[cfg(target_os = "macos")]
    pub(super) fn mouse_move(point: (i32, i32)) -> CommandResult<()> {
        use core_graphics::{
            event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton},
            event_source::{CGEventSource, CGEventSourceStateID},
            geometry::CGPoint,
        };
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            CommandError::system_fault(
                "permission_accessibility_denied",
                "Could not create desktop input source. Grant Accessibility permission to Xero.",
            )
        })?;
        let event = CGEvent::new_mouse_event(
            source,
            CGEventType::MouseMoved,
            CGPoint::new(point.0 as f64, point.1 as f64),
            CGMouseButton::Left,
        )
        .map_err(|_| {
            CommandError::system_fault(
                "sidecar_unavailable",
                "Could not build desktop mouse move event.",
            )
        })?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn mouse_move(_point: (i32, i32)) -> CommandResult<()> {
        unsupported_input()
    }

    #[cfg(target_os = "macos")]
    pub(super) fn mouse_click(
        point: (i32, i32),
        button: AutonomousDesktopMouseButton,
        clicks: u8,
    ) -> CommandResult<()> {
        use core_graphics::{
            event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton},
            event_source::{CGEventSource, CGEventSourceStateID},
            geometry::CGPoint,
        };
        let cg_button = match button {
            AutonomousDesktopMouseButton::Left => CGMouseButton::Left,
            AutonomousDesktopMouseButton::Right => CGMouseButton::Right,
            AutonomousDesktopMouseButton::Middle => CGMouseButton::Center,
        };
        let (down, up) = match button {
            AutonomousDesktopMouseButton::Right => {
                (CGEventType::RightMouseDown, CGEventType::RightMouseUp)
            }
            _ => (CGEventType::LeftMouseDown, CGEventType::LeftMouseUp),
        };
        for _ in 0..clicks {
            let source_down = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
                .map_err(|_| input_source_error())?;
            let source_up = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
                .map_err(|_| input_source_error())?;
            let location = CGPoint::new(point.0 as f64, point.1 as f64);
            let down_event = CGEvent::new_mouse_event(source_down, down, location, cg_button)
                .map_err(|_| event_error("mouse down"))?;
            let up_event = CGEvent::new_mouse_event(source_up, up, location, cg_button)
                .map_err(|_| event_error("mouse up"))?;
            down_event.post(CGEventTapLocation::HID);
            up_event.post(CGEventTapLocation::HID);
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn mouse_click(
        _point: (i32, i32),
        _button: AutonomousDesktopMouseButton,
        _clicks: u8,
    ) -> CommandResult<()> {
        unsupported_input()
    }

    #[cfg(target_os = "macos")]
    pub(super) fn mouse_drag(from: (i32, i32), to: (i32, i32)) -> CommandResult<()> {
        use core_graphics::{
            event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton},
            event_source::{CGEventSource, CGEventSourceStateID},
            geometry::CGPoint,
        };
        let points = [from, to];
        let event_types = [
            CGEventType::LeftMouseDown,
            CGEventType::LeftMouseDragged,
            CGEventType::LeftMouseUp,
        ];
        for (index, event_type) in event_types.into_iter().enumerate() {
            let point = if index == 0 { points[0] } else { points[1] };
            let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
                .map_err(|_| input_source_error())?;
            let event = CGEvent::new_mouse_event(
                source,
                event_type,
                CGPoint::new(point.0 as f64, point.1 as f64),
                CGMouseButton::Left,
            )
            .map_err(|_| event_error("mouse drag"))?;
            event.post(CGEventTapLocation::HID);
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn mouse_drag(_from: (i32, i32), _to: (i32, i32)) -> CommandResult<()> {
        unsupported_input()
    }

    #[cfg(target_os = "macos")]
    pub(super) fn scroll(delta_x: i32, delta_y: i32) -> CommandResult<()> {
        use core_graphics::{
            event::{CGEvent, CGEventTapLocation, ScrollEventUnit},
            event_source::{CGEventSource, CGEventSourceStateID},
        };
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| input_source_error())?;
        let wheel_count = if delta_x == 0 { 1 } else { 2 };
        let event = CGEvent::new_scroll_event(
            source,
            ScrollEventUnit::PIXEL,
            wheel_count,
            delta_y,
            delta_x,
            0,
        )
        .map_err(|_| event_error("scroll"))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn scroll(_delta_x: i32, _delta_y: i32) -> CommandResult<()> {
        unsupported_input()
    }

    #[cfg(target_os = "macos")]
    pub(super) fn key_press(key: &str) -> CommandResult<()> {
        let key_code = key_code_for(key).ok_or_else(|| {
            CommandError::user_fixable(
                "desktop_key_unsupported",
                format!("Desktop key `{key}` is not supported by the local keyboard mapper."),
            )
        })?;
        post_key_code(
            key_code,
            core_graphics::event::CGEventFlags::CGEventFlagNull,
        )
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn key_press(_key: &str) -> CommandResult<()> {
        unsupported_input()
    }

    #[cfg(target_os = "macos")]
    pub(super) fn hotkey(keys: &[String]) -> CommandResult<()> {
        use core_graphics::event::{CGEventFlags, KeyCode};
        let mut flags = CGEventFlags::CGEventFlagNull;
        let mut target: Option<&str> = None;
        for key in keys {
            let normalized = key.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "cmd" | "command" | "meta" | "super" => {
                    flags |= CGEventFlags::CGEventFlagCommand;
                }
                "ctrl" | "control" => {
                    flags |= CGEventFlags::CGEventFlagControl;
                }
                "alt" | "option" => {
                    flags |= CGEventFlags::CGEventFlagAlternate;
                }
                "shift" => {
                    flags |= CGEventFlags::CGEventFlagShift;
                }
                _ => target = Some(key.as_str()),
            }
        }
        let key_code = match target {
            Some(key) => key_code_for(key).ok_or_else(|| {
                CommandError::user_fixable(
                    "desktop_key_unsupported",
                    format!("Desktop hotkey target `{key}` is not supported by the local keyboard mapper."),
                )
            })?,
            None if flags.contains(CGEventFlags::CGEventFlagCommand) => KeyCode::COMMAND,
            None if flags.contains(CGEventFlags::CGEventFlagControl) => KeyCode::CONTROL,
            None if flags.contains(CGEventFlags::CGEventFlagAlternate) => KeyCode::OPTION,
            None if flags.contains(CGEventFlags::CGEventFlagShift) => KeyCode::SHIFT,
            None => return Err(CommandError::invalid_request("keys")),
        };
        post_key_code(key_code, flags)
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn hotkey(_keys: &[String]) -> CommandResult<()> {
        unsupported_input()
    }

    #[cfg(target_os = "macos")]
    pub(super) fn type_text(text: &str) -> CommandResult<()> {
        use core_graphics::{
            event::{CGEvent, CGEventTapLocation},
            event_source::{CGEventSource, CGEventSourceStateID},
        };
        for ch in text.chars() {
            let source_down = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
                .map_err(|_| input_source_error())?;
            let source_up = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
                .map_err(|_| input_source_error())?;
            let value = ch.to_string();
            let down = CGEvent::new_keyboard_event(source_down, 0, true)
                .map_err(|_| event_error("text key down"))?;
            down.set_string(&value);
            down.post(CGEventTapLocation::HID);
            let up = CGEvent::new_keyboard_event(source_up, 0, false)
                .map_err(|_| event_error("text key up"))?;
            up.set_string(&value);
            up.post(CGEventTapLocation::HID);
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn type_text(_text: &str) -> CommandResult<()> {
        unsupported_input()
    }

    #[cfg(not(target_os = "macos"))]
    fn unsupported_input() -> CommandResult<()> {
        Err(CommandError::user_fixable(
            "sidecar_unavailable",
            "Desktop input for this action requires the platform sidecar backend.",
        ))
    }

    #[cfg(target_os = "macos")]
    fn input_source_error() -> CommandError {
        CommandError::system_fault(
            "permission_accessibility_denied",
            "Could not create desktop input source. Grant Accessibility permission to Xero.",
        )
    }

    #[cfg(target_os = "macos")]
    fn event_error(kind: &str) -> CommandError {
        CommandError::system_fault(
            "sidecar_unavailable",
            format!("Could not build desktop {kind} event."),
        )
    }

    #[cfg(target_os = "macos")]
    fn post_key_code(
        key_code: core_graphics::event::CGKeyCode,
        flags: core_graphics::event::CGEventFlags,
    ) -> CommandResult<()> {
        use core_graphics::{
            event::{CGEvent, CGEventTapLocation},
            event_source::{CGEventSource, CGEventSourceStateID},
        };
        let source_down = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| input_source_error())?;
        let source_up = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| input_source_error())?;
        let down = CGEvent::new_keyboard_event(source_down, key_code, true)
            .map_err(|_| event_error("key down"))?;
        down.set_flags(flags);
        down.post(CGEventTapLocation::HID);
        let up = CGEvent::new_keyboard_event(source_up, key_code, false)
            .map_err(|_| event_error("key up"))?;
        up.set_flags(flags);
        up.post(CGEventTapLocation::HID);
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn key_code_for(key: &str) -> Option<core_graphics::event::CGKeyCode> {
        use core_graphics::event::KeyCode;
        match key.trim().to_ascii_lowercase().as_str() {
            "a" => Some(0x00),
            "s" => Some(0x01),
            "d" => Some(0x02),
            "f" => Some(0x03),
            "h" => Some(0x04),
            "g" => Some(0x05),
            "z" => Some(0x06),
            "x" => Some(0x07),
            "c" => Some(0x08),
            "v" => Some(0x09),
            "b" => Some(0x0B),
            "q" => Some(0x0C),
            "w" => Some(0x0D),
            "e" => Some(0x0E),
            "r" => Some(0x0F),
            "y" => Some(0x10),
            "t" => Some(0x11),
            "1" => Some(0x12),
            "2" => Some(0x13),
            "3" => Some(0x14),
            "4" => Some(0x15),
            "6" => Some(0x16),
            "5" => Some(0x17),
            "=" | "equal" => Some(0x18),
            "9" => Some(0x19),
            "7" => Some(0x1A),
            "-" | "minus" => Some(0x1B),
            "8" => Some(0x1C),
            "0" => Some(0x1D),
            "]" | "right_bracket" => Some(0x1E),
            "o" => Some(0x1F),
            "u" => Some(0x20),
            "[" | "left_bracket" => Some(0x21),
            "i" => Some(0x22),
            "p" => Some(0x23),
            "return" | "enter" => Some(KeyCode::RETURN),
            "l" => Some(0x25),
            "j" => Some(0x26),
            "'" | "quote" => Some(0x27),
            "k" => Some(0x28),
            ";" | "semicolon" => Some(0x29),
            "\\" | "backslash" => Some(0x2A),
            "," | "comma" => Some(0x2B),
            "/" | "slash" => Some(0x2C),
            "n" => Some(0x2D),
            "m" => Some(0x2E),
            "." | "period" => Some(0x2F),
            "tab" => Some(KeyCode::TAB),
            "space" => Some(KeyCode::SPACE),
            "delete" | "backspace" => Some(KeyCode::DELETE),
            "escape" | "esc" => Some(KeyCode::ESCAPE),
            "cmd" | "command" | "meta" | "super" => Some(KeyCode::COMMAND),
            "shift" => Some(KeyCode::SHIFT),
            "alt" | "option" => Some(KeyCode::OPTION),
            "ctrl" | "control" => Some(KeyCode::CONTROL),
            "home" => Some(KeyCode::HOME),
            "end" => Some(KeyCode::END),
            "pageup" | "page_up" => Some(KeyCode::PAGE_UP),
            "pagedown" | "page_down" => Some(KeyCode::PAGE_DOWN),
            "left" | "arrowleft" | "left_arrow" => Some(KeyCode::LEFT_ARROW),
            "right" | "arrowright" | "right_arrow" => Some(KeyCode::RIGHT_ARROW),
            "down" | "arrowdown" | "down_arrow" => Some(KeyCode::DOWN_ARROW),
            "up" | "arrowup" | "up_arrow" => Some(KeyCode::UP_ARROW),
            "f1" => Some(KeyCode::F1),
            "f2" => Some(KeyCode::F2),
            "f3" => Some(KeyCode::F3),
            "f4" => Some(KeyCode::F4),
            "f5" => Some(KeyCode::F5),
            "f6" => Some(KeyCode::F6),
            "f7" => Some(KeyCode::F7),
            "f8" => Some(KeyCode::F8),
            "f9" => Some(KeyCode::F9),
            "f10" => Some(KeyCode::F10),
            "f11" => Some(KeyCode::F11),
            "f12" => Some(KeyCode::F12),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn desktop_control_request(
        action: AutonomousDesktopControlAction,
    ) -> AutonomousDesktopControlRequest {
        AutonomousDesktopControlRequest {
            action,
            display_id: None,
            window_id: None,
            app_name: None,
            bundle_id: None,
            element_id: None,
            x: None,
            y: None,
            to_x: None,
            to_y: None,
            delta_x: None,
            delta_y: None,
            button: None,
            clicks: None,
            key: None,
            keys: Vec::new(),
            text: None,
            value: None,
            menu_path: Vec::new(),
            reason: None,
            sensitivity: None,
        }
    }

    #[test]
    fn observe_screenshot_requires_approval() {
        let request = AutonomousDesktopObserveRequest {
            action: AutonomousDesktopObserveAction::Screenshot,
            display_id: None,
            window_id: None,
            region: None,
            redaction: None,
            x: None,
            y: None,
        };
        let policy = desktop_observe_policy(&request, false);
        assert_eq!(
            policy.decision,
            AutonomousDesktopPolicyDecision::ApprovalRequired
        );
    }

    #[test]
    fn control_secret_text_is_denied() {
        let request = AutonomousDesktopControlRequest {
            action: AutonomousDesktopControlAction::TypeText,
            display_id: None,
            window_id: None,
            app_name: None,
            bundle_id: None,
            element_id: None,
            x: None,
            y: None,
            to_x: None,
            to_y: None,
            delta_x: None,
            delta_y: None,
            button: None,
            clicks: None,
            key: None,
            keys: Vec::new(),
            text: Some("hunter2".into()),
            value: None,
            menu_path: Vec::new(),
            reason: Some("test".into()),
            sensitivity: Some(AutonomousDesktopTextSensitivity::Secret),
        };
        let policy = desktop_control_policy(&request, true);
        assert_eq!(policy.decision, AutonomousDesktopPolicyDecision::Denied);
    }

    #[test]
    fn control_non_goal_targets_are_denied() {
        let mut cases = Vec::new();

        let mut password_manager =
            desktop_control_request(AutonomousDesktopControlAction::LaunchApp);
        password_manager.app_name = Some("1Password".into());
        password_manager.bundle_id = Some("com.agilebits.onepassword7".into());
        cases.push(("password manager", password_manager));

        let mut payment_confirmation =
            desktop_control_request(AutonomousDesktopControlAction::MenuSelect);
        payment_confirmation.app_name = Some("Safari".into());
        payment_confirmation.menu_path = vec!["Checkout".into(), "Confirm Payment".into()];
        cases.push(("payment confirmation", payment_confirmation));

        let mut identity_verification =
            desktop_control_request(AutonomousDesktopControlAction::AxSetValue);
        identity_verification.element_id = Some("passport-number-input".into());
        identity_verification.value = Some("123456789".into());
        identity_verification.reason = Some("Complete identity verification form".into());
        cases.push(("identity verification", identity_verification));

        let mut recovery_flow = desktop_control_request(AutonomousDesktopControlAction::TypeText);
        recovery_flow.text = Some("123456".into());
        recovery_flow.reason = Some("Enter account recovery code".into());
        cases.push(("account recovery", recovery_flow));

        let mut system_privacy = desktop_control_request(AutonomousDesktopControlAction::AxPress);
        system_privacy.app_name = Some("System Settings".into());
        system_privacy.element_id = Some("privacy-security-toggle".into());
        system_privacy.reason = Some("Change Privacy & Security permissions".into());
        cases.push(("system privacy", system_privacy));

        for (label, request) in cases {
            let policy = desktop_control_policy(&request, true);
            assert_eq!(
                policy.decision,
                AutonomousDesktopPolicyDecision::Denied,
                "{label}"
            );
            assert_eq!(
                policy.code, "desktop_policy_blocked_target_denied",
                "{label}"
            );
            assert_eq!(
                policy.category,
                AutonomousDesktopPolicyCategory::ControlDenied,
                "{label}"
            );
        }
    }

    #[test]
    fn control_developer_surfaces_are_not_blocked_by_category() {
        let mut request = desktop_control_request(AutonomousDesktopControlAction::Hotkey);
        request.app_name = Some("Visual Studio Code".into());
        request.bundle_id = Some("com.microsoft.VSCode".into());
        request.keys = vec!["meta".into(), "p".into()];
        request.reason = Some("Open a visible developer tool command palette".into());

        let policy = desktop_control_policy(&request, true);

        assert_eq!(policy.decision, AutonomousDesktopPolicyDecision::Allowed);
        assert_eq!(policy.code, "desktop_policy_control_allowed_after_approval");
    }

    #[test]
    fn controller_lock_rejects_different_active_actor() {
        let repo = tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        let _first = runtime
            .acquire_desktop_lock(AutonomousDesktopActor::Agent)
            .expect("first lock");
        let second = runtime.acquire_desktop_lock(AutonomousDesktopActor::CloudManualControl);
        assert!(second.is_err());
    }

    #[test]
    fn local_user_takeover_lock_blocks_remote_reacquire() {
        let repo = tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(repo.path())
            .expect("runtime")
            .with_agent_run_context("project-1", "session-1", "run-1");

        let takeover = runtime.mark_local_user_takeover().expect("takeover lock");

        assert_eq!(takeover.actor, AutonomousDesktopActor::LocalUser);
        assert_eq!(
            takeover.release_reason.as_deref(),
            Some("local_user_takeover")
        );

        let reacquire = runtime.desktop_acquire_manual_control("manual-1", Some("test"));

        assert!(reacquire
            .expect_err("local user lock should block manual control")
            .message
            .contains("local_user"));
    }

    #[test]
    fn current_lock_drops_expired_controller_lease() {
        let state = DesktopControlState::new_local();
        {
            let mut guard = state.lock.lock().expect("lock");
            *guard = Some(AutonomousDesktopControllerLock {
                actor: AutonomousDesktopActor::CloudManualControl,
                lease_id: Some("manual-expired".into()),
                session_id: "session-1".into(),
                run_id: None,
                acquired_at: "1970-01-01T00:00:00Z".into(),
                expires_at: "1970-01-01T00:00:01Z".into(),
                last_input_at: "1970-01-01T00:00:00Z".into(),
                release_reason: None,
            });
        }

        let current = current_desktop_lock(&state).expect("status lock");

        assert!(current.is_none());
        assert!(state.lock.lock().expect("lock").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn sidecar_manager_clears_process_after_ipc_response_failure() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("IFS= read -r _; exit 0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn fake sidecar");
        let stdin = child.stdin.take().expect("fake sidecar stdin");
        let stdout = child.stdout.take().expect("fake sidecar stdout");
        let mut manager = DesktopSidecarManager {
            process: Some(DesktopSidecarProcess {
                child,
                stdin,
                stdout: BufReader::new(stdout),
                token: "test-token".into(),
                session_id: "test-session".into(),
                lease_expires_at: timestamp_after(Duration::from_secs(30)),
                binary_path: PathBuf::from("fake-sidecar"),
                checksum_verified: false,
            }),
            last_error: None,
        };

        let error = manager
            .request(
                DesktopSidecarOperation::Health,
                json!({}),
                "desktop_sidecar_test_policy",
            )
            .expect_err("sidecar that exits before response should fail");

        assert!(error.contains("closed before sending a response"));
        assert!(manager.process.is_none());
        assert!(manager
            .last_error
            .as_deref()
            .is_some_and(|message| message.contains("closed before sending a response")));
    }

    #[test]
    fn cloud_manual_input_requires_active_controller_lease() {
        let repo = tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        let request = AutonomousDesktopControlRequest {
            action: AutonomousDesktopControlAction::MouseMove,
            display_id: None,
            window_id: None,
            app_name: None,
            bundle_id: None,
            element_id: None,
            x: Some(42),
            y: Some(64),
            to_x: None,
            to_y: None,
            delta_x: None,
            delta_y: None,
            button: None,
            clicks: None,
            key: None,
            keys: Vec::new(),
            text: None,
            value: None,
            menu_path: Vec::new(),
            reason: Some("cloud_manual_control_input".into()),
            sensitivity: None,
        };

        let result = runtime
            .desktop_control_as_manual_control_with_operator_approval(request, "manual-missing")
            .expect("manual input result");
        let AutonomousToolOutput::DesktopControl(output) = result.output else {
            panic!("expected desktop control output");
        };

        assert_eq!(output.status, AutonomousDesktopToolStatus::Denied);
        assert_eq!(
            output.error.as_ref().map(|error| error.code.as_str()),
            Some("manual_control_lease_required")
        );
    }

    #[test]
    fn cloud_manual_input_requires_matching_controller_lease() {
        let repo = tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        runtime
            .desktop_acquire_manual_control("manual-1", Some("test"))
            .expect("manual lock");
        let request = AutonomousDesktopControlRequest {
            action: AutonomousDesktopControlAction::MouseMove,
            display_id: None,
            window_id: None,
            app_name: None,
            bundle_id: None,
            element_id: None,
            x: Some(42),
            y: Some(64),
            to_x: None,
            to_y: None,
            delta_x: None,
            delta_y: None,
            button: None,
            clicks: None,
            key: None,
            keys: Vec::new(),
            text: None,
            value: None,
            menu_path: Vec::new(),
            reason: Some("cloud_manual_control_input".into()),
            sensitivity: None,
        };

        let result = runtime
            .desktop_control_as_manual_control_with_operator_approval(request, "manual-2")
            .expect("manual input result");
        let AutonomousToolOutput::DesktopControl(output) = result.output else {
            panic!("expected desktop control output");
        };

        assert_eq!(output.status, AutonomousDesktopToolStatus::Denied);
        assert_eq!(
            output.error.as_ref().map(|error| error.code.as_str()),
            Some("manual_control_lease_required")
        );
    }

    #[test]
    fn manual_control_heartbeat_refreshes_active_lease() {
        let repo = tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        runtime
            .desktop_acquire_manual_control("manual-1", Some("test"))
            .expect("manual lock");

        let output = runtime
            .desktop_refresh_manual_control("manual-1", "heartbeat")
            .expect("heartbeat output");

        assert_eq!(output.status, AutonomousDesktopToolStatus::Executed);
        assert_eq!(
            output.controller_lock.as_ref().map(|lock| lock.actor),
            Some(AutonomousDesktopActor::CloudManualControl)
        );
        assert_eq!(
            output
                .controller_lock
                .as_ref()
                .and_then(|lock| lock.lease_id.as_deref()),
            Some("manual-1")
        );
    }

    #[test]
    fn stream_quality_profiles_bound_fallback_frame_rate() {
        let low = stream_quality_profile(AutonomousDesktopStreamQuality::Low);
        let balanced = stream_quality_profile(AutonomousDesktopStreamQuality::Balanced);
        let high = stream_quality_profile(AutonomousDesktopStreamQuality::High);

        assert_eq!(low.max_width, 960);
        assert_eq!(low.max_frame_rate, 1);
        assert_eq!(balanced.max_width, 1280);
        assert_eq!(balanced.max_frame_rate, 1);
        assert_eq!(high.max_width, 1920);
        assert_eq!(high.max_frame_rate, 2);
    }

    #[test]
    fn sidecar_stream_request_uses_webrtc_quality_defaults() {
        let request = AutonomousDesktopStreamRequest {
            action: AutonomousDesktopStreamAction::StreamStart,
            session_id: Some("session-1".into()),
            run_id: Some("run-1".into()),
            display_id: Some("main".into()),
            stream_id: Some("stream-1".into()),
            max_width: None,
            max_frame_rate: None,
            include_cursor: None,
            quality: Some(AutonomousDesktopStreamQuality::Balanced),
            redaction: None,
            ice_servers: Vec::new(),
            session_description: None,
            ice_candidate: None,
        };

        let sidecar = sidecar_stream_request(&request, Some("session-1"), Some("stream-1"), None);

        assert_eq!(sidecar.max_width, Some(1280));
        assert_eq!(sidecar.max_frame_rate, Some(24));
        assert_eq!(sidecar.quality, Some(DesktopSidecarStreamQuality::Balanced));
    }

    #[test]
    fn sidecar_stream_request_carries_webrtc_signaling_payloads() {
        let request = AutonomousDesktopStreamRequest {
            action: AutonomousDesktopStreamAction::StreamAnswer,
            session_id: Some("session-1".into()),
            run_id: Some("run-1".into()),
            display_id: None,
            stream_id: Some("stream-1".into()),
            max_width: None,
            max_frame_rate: None,
            include_cursor: None,
            quality: None,
            redaction: None,
            ice_servers: vec![AutonomousDesktopIceServer {
                urls: AutonomousDesktopIceServerUrls::One("turn:turn.example.test:3478".into()),
                username: Some("user".into()),
                credential: Some("pass".into()),
                credential_type: Some("password".into()),
            }],
            session_description: Some(AutonomousDesktopSessionDescription {
                sdp_type: "answer".into(),
                sdp: "v=0".into(),
            }),
            ice_candidate: Some(AutonomousDesktopIceCandidate {
                candidate: "candidate:1".into(),
                sdp_mid: Some("0".into()),
                sdp_m_line_index: Some(0),
                username_fragment: Some("ufrag".into()),
            }),
        };

        let sidecar = sidecar_stream_request(&request, Some("session-1"), Some("stream-1"), None);

        assert_eq!(sidecar.ice_servers.len(), 1);
        assert_eq!(
            sidecar
                .session_description
                .as_ref()
                .map(|value| value.sdp.as_str()),
            Some("v=0")
        );
        assert_eq!(
            sidecar
                .ice_candidate
                .as_ref()
                .map(|value| value.candidate.as_str()),
            Some("candidate:1")
        );
    }

    #[test]
    fn sidecar_stream_payload_maps_to_runtime_state() {
        let state = AutonomousDesktopStreamState::from(DesktopSidecarStreamPayload {
            stream_id: Some("stream-1".into()),
            status: DesktopSidecarStreamStatus::Live,
            transport: DesktopSidecarStreamTransport::WebRtc,
            signaling_channel: Some("computer_use_stream".into()),
            quality: DesktopSidecarStreamQuality::High,
            max_width: 1920,
            max_frame_rate: 30,
            include_cursor: true,
            session_description: None,
            ice_candidate: None,
            message: "Native stream is live.".into(),
        });

        assert_eq!(state.transport, AutonomousDesktopStreamTransport::WebRtc);
        assert_eq!(state.status, AutonomousDesktopStreamStatus::Live);
        assert_eq!(state.quality, AutonomousDesktopStreamQuality::High);
    }

    #[test]
    fn degraded_stream_state_mentions_native_error_code_without_frame_bytes() {
        let request = AutonomousDesktopStreamRequest {
            action: AutonomousDesktopStreamAction::StreamStart,
            session_id: Some("session-1".into()),
            run_id: Some("run-1".into()),
            display_id: None,
            stream_id: None,
            max_width: None,
            max_frame_rate: None,
            include_cursor: Some(true),
            quality: Some(AutonomousDesktopStreamQuality::Low),
            redaction: None,
            ice_servers: Vec::new(),
            session_description: None,
            ice_candidate: None,
        };
        let error = DesktopSidecarErrorBody::new(
            "stream_webrtc_failed",
            "native stream setup failed",
            true,
            false,
        );

        let state = degraded_stream_state(&request, "stream-1", Some(&error));

        assert!(state.message.contains("stream_webrtc_failed"));
        assert_eq!(
            state.transport,
            AutonomousDesktopStreamTransport::ScreenshotFallback
        );
    }

    #[test]
    fn desktop_stream_writes_minimal_session_metadata_without_frame_bytes() {
        let repo = tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(repo.path())
            .expect("runtime")
            .with_agent_run_context("project-1", "session-1", "run-1");

        let start = runtime
            .desktop_stream_with_operator_approval(AutonomousDesktopStreamRequest {
                action: AutonomousDesktopStreamAction::StreamStart,
                session_id: Some("session-1".into()),
                run_id: Some("run-1".into()),
                display_id: None,
                stream_id: None,
                max_width: None,
                max_frame_rate: None,
                include_cursor: Some(true),
                quality: Some(AutonomousDesktopStreamQuality::Low),
                redaction: None,
                ice_servers: Vec::new(),
                session_description: None,
                ice_candidate: None,
            })
            .expect("stream start");
        let AutonomousToolOutput::DesktopStream(start_output) = start.output else {
            panic!("expected desktop stream output");
        };
        let stream_id = start_output
            .stream
            .as_ref()
            .and_then(|stream| stream.stream_id.clone())
            .expect("stream id");

        runtime
            .desktop_stream_with_operator_approval(AutonomousDesktopStreamRequest {
                action: AutonomousDesktopStreamAction::StreamStop,
                session_id: Some("session-1".into()),
                run_id: Some("run-1".into()),
                display_id: None,
                stream_id: Some(stream_id.clone()),
                max_width: None,
                max_frame_rate: None,
                include_cursor: None,
                quality: None,
                redaction: None,
                ice_servers: Vec::new(),
                session_description: None,
                ice_candidate: None,
            })
            .expect("stream stop");

        let metadata_path =
            project_app_data_dir_for_repo(repo.path()).join(DESKTOP_STREAM_SESSIONS_FILE);
        let records = std::fs::read_to_string(metadata_path).expect("stream session metadata");
        let records = records
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json record"))
            .collect::<Vec<_>>();

        assert_eq!(
            records
                .iter()
                .map(|record| record["event"].as_str())
                .collect::<Vec<_>>(),
            vec![Some("start"), Some("stop")]
        );
        assert_eq!(records[0]["streamId"], json!(stream_id));
        assert_eq!(records[0]["quality"], json!("low"));
        assert!(records
            .iter()
            .all(|record| record.get("bytesBase64").is_none()));
    }

    #[test]
    fn audit_redacts_sensitive_reason() {
        let output = AutonomousDesktopToolOutput {
            tool: AUTONOMOUS_TOOL_DESKTOP_CONTROL.into(),
            action: "type_text".into(),
            request_id: "req_1".into(),
            phase: DESKTOP_CONTROL_PHASE.into(),
            status: AutonomousDesktopToolStatus::Executed,
            platform: "macos".into(),
            sidecar: sidecar_status(),
            capabilities: desktop_capabilities(),
            permissions: Vec::new(),
            policy: desktop_policy(
                AutonomousDesktopPolicyCategory::ControlSafe,
                AutonomousDesktopPolicyDecision::Allowed,
                "ok",
                "ok",
                false,
                false,
            ),
            displays: Vec::new(),
            windows: Vec::new(),
            apps: Vec::new(),
            foreground: None,
            cursor: None,
            screenshot: None,
            stream: None,
            stream_signal: None,
            structured_snapshot: None,
            controller_lock: None,
            audit_id: None,
            error: None,
            message: "ok".into(),
        };
        let summary = redacted_desktop_summary(&output, Some("paste password"));
        assert!(summary.contains("[redacted sensitive desktop label]"));
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;

        fs::symlink_metadata(path)
            .expect("stat path")
            .permissions()
            .mode()
            & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn desktop_metadata_logs_are_owner_only_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempdir().expect("tempdir");
        let parent = root.path().join("desktop-control");
        let path = parent.join("audit.jsonl");
        fs::create_dir_all(&parent).expect("create parent");
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).expect("seed dir mode");
        fs::write(&path, b"").expect("seed log");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("seed file mode");

        prepare_desktop_metadata_parent(&parent).expect("prepare parent");
        {
            let mut file = open_desktop_metadata_append_file(&path).expect("open log");
            writeln!(file, "{{}}").expect("write log");
        }

        assert_eq!(mode_of(&parent), DESKTOP_METADATA_DIR_MODE);
        assert_eq!(mode_of(&path), DESKTOP_METADATA_FILE_MODE);
    }

    #[cfg(unix)]
    #[test]
    fn desktop_metadata_logs_reject_symlinked_files() {
        use std::os::unix::fs::symlink;

        let root = tempdir().expect("tempdir");
        let target = root.path().join("target.jsonl");
        let link = root.path().join("audit.jsonl");
        fs::write(&target, b"").expect("target");
        symlink(&target, &link).expect("symlink");

        let error = open_desktop_metadata_append_file(&link).expect_err("reject symlink");

        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn screenshot_redaction_blacks_requested_private_regions() {
        let mut image = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]));
        let redaction = AutonomousDesktopRedactionRequest {
            mode: AutonomousDesktopRedactionMode::Balanced,
            private_regions: vec![
                AutonomousDesktopRegion {
                    x: 1,
                    y: 1,
                    width: 2,
                    height: 2,
                },
                AutonomousDesktopRegion {
                    x: 10,
                    y: 10,
                    width: 2,
                    height: 2,
                },
            ],
        };

        let applied = apply_private_region_redactions(&mut image, Some(&redaction));

        assert_eq!(applied, 1);
        assert_eq!(*image.get_pixel(1, 1), Rgba([0, 0, 0, 255]));
        assert_eq!(*image.get_pixel(2, 2), Rgba([0, 0, 0, 255]));
        assert_eq!(*image.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn screenshot_redaction_keeps_user_marked_regions_when_auto_mode_is_off() {
        let mut image = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
        let redaction = AutonomousDesktopRedactionRequest {
            mode: AutonomousDesktopRedactionMode::Off,
            private_regions: vec![AutonomousDesktopRegion {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            }],
        };

        let applied = apply_private_region_redactions(&mut image, Some(&redaction));

        assert_eq!(applied, 1);
        assert_eq!(*image.get_pixel(0, 0), Rgba([0, 0, 0, 255]));
    }

    #[test]
    fn sidecar_screenshot_request_preserves_capture_bounds_and_redactions() {
        let request = AutonomousDesktopObserveRequest {
            action: AutonomousDesktopObserveAction::Screenshot,
            display_id: Some("main".into()),
            window_id: None,
            region: Some(AutonomousDesktopRegion {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            }),
            redaction: Some(AutonomousDesktopRedactionRequest {
                mode: AutonomousDesktopRedactionMode::Strict,
                private_regions: vec![AutonomousDesktopRegion {
                    x: 5,
                    y: 6,
                    width: 7,
                    height: 8,
                }],
            }),
            x: None,
            y: None,
        };

        let sidecar = sidecar_screenshot_request(&request);

        assert_eq!(sidecar.display_id.as_deref(), Some("main"));
        assert_eq!(sidecar.region.as_ref().map(|region| region.height), Some(4));
        assert_eq!(
            sidecar.redaction.as_ref().map(|redaction| &redaction.mode),
            Some(&DesktopSidecarRedactionMode::Strict)
        );
        assert_eq!(
            sidecar
                .redaction
                .as_ref()
                .map(|redaction| redaction.private_regions[0].width),
            Some(7)
        );
    }

    #[test]
    fn sidecar_control_request_maps_pointer_and_keyboard_fields() {
        let request = AutonomousDesktopControlRequest {
            action: AutonomousDesktopControlAction::MouseClick,
            display_id: None,
            window_id: None,
            app_name: None,
            bundle_id: None,
            element_id: Some("macos_ax:1:AXButton:10:20:30:40:10:20".into()),
            x: Some(10),
            y: Some(20),
            to_x: None,
            to_y: None,
            delta_x: None,
            delta_y: None,
            button: Some(AutonomousDesktopMouseButton::Right),
            clicks: Some(2),
            key: Some("a".into()),
            keys: vec!["command".into(), "a".into()],
            text: Some("hello".into()),
            value: Some("updated".into()),
            menu_path: vec!["File".into(), "New".into()],
            reason: None,
            sensitivity: None,
        };

        let sidecar = sidecar_control_request(&request);

        assert_eq!(
            sidecar.element_id.as_deref(),
            Some("macos_ax:1:AXButton:10:20:30:40:10:20")
        );
        assert_eq!(sidecar.x, Some(10));
        assert_eq!(sidecar.button, Some(DesktopSidecarMouseButton::Right));
        assert_eq!(sidecar.clicks, Some(2));
        assert_eq!(sidecar.keys, vec!["command".to_string(), "a".to_string()]);
        assert_eq!(sidecar.text.as_deref(), Some("hello"));
        assert_eq!(sidecar.value.as_deref(), Some("updated"));
        assert_eq!(
            sidecar.menu_path,
            vec!["File".to_string(), "New".to_string()]
        );
        assert_eq!(
            desktop_control_sidecar_operation(&request.action),
            Some(DesktopSidecarOperation::MouseClick)
        );
    }

    #[test]
    fn sidecar_control_operation_maps_ax_actions() {
        assert_eq!(
            desktop_control_sidecar_operation(&AutonomousDesktopControlAction::AxPress),
            Some(DesktopSidecarOperation::AxPress)
        );
        assert_eq!(
            desktop_control_sidecar_operation(&AutonomousDesktopControlAction::PasteText),
            Some(DesktopSidecarOperation::PasteText)
        );
        assert_eq!(
            desktop_control_sidecar_operation(&AutonomousDesktopControlAction::AxSetValue),
            Some(DesktopSidecarOperation::AxSetValue)
        );
        assert_eq!(
            desktop_control_sidecar_operation(&AutonomousDesktopControlAction::AxFocus),
            Some(DesktopSidecarOperation::AxFocus)
        );
        assert_eq!(
            desktop_control_sidecar_operation(&AutonomousDesktopControlAction::MenuSelect),
            Some(DesktopSidecarOperation::MenuSelect)
        );
    }

    #[test]
    fn sidecar_permission_payload_decodes_to_status_rows() {
        let payload = json!({
            "permissions": [
                {
                    "name": "Screen Recording",
                    "status": "unknown",
                    "requiredFor": ["screenshot", "stream"],
                    "remediation": "Grant permission locally."
                }
            ]
        });

        let decoded =
            serde_json::from_value::<DesktopSidecarPermissionsPayload>(payload).expect("decode");

        assert_eq!(decoded.permissions.len(), 1);
        assert_eq!(
            decoded.permissions[0].status,
            DesktopSidecarPermissionGrant::Unknown
        );
    }

    #[test]
    fn sidecar_permissions_merge_with_missing_platform_fallback_rows() {
        let merged = merge_desktop_permissions(
            vec![permission(
                "Screen Recording",
                AutonomousDesktopPermissionGrant::Denied,
                &["screenshot"],
                "Grant screen capture permission from the sidecar.",
            )],
            vec![
                permission(
                    "Screen Recording",
                    AutonomousDesktopPermissionGrant::Unknown,
                    &["stream"],
                    "Grant screen capture permission locally.",
                ),
                permission(
                    "Input Monitoring",
                    AutonomousDesktopPermissionGrant::Unknown,
                    &["keyboard", "hotkey"],
                    "Grant Input Monitoring locally.",
                ),
            ],
        );

        let screen_recording = merged
            .iter()
            .find(|permission| permission.name == "Screen Recording")
            .expect("screen recording permission");
        assert_eq!(
            screen_recording.status,
            AutonomousDesktopPermissionGrant::Denied
        );
        assert_eq!(
            screen_recording.required_for,
            vec!["screenshot".to_string(), "stream".to_string()]
        );
        assert!(merged
            .iter()
            .any(|permission| permission.name == "Input Monitoring"));
    }

    #[test]
    fn sidecar_cursor_payload_maps_to_runtime_state() {
        let cursor = AutonomousDesktopCursorState::from(DesktopSidecarCursorStatePayload {
            x: 42,
            y: 24,
            display_id: Some("display-1".into()),
            available: true,
        });

        assert_eq!(cursor.x, 42);
        assert_eq!(cursor.y, 24);
        assert_eq!(cursor.display_id.as_deref(), Some("display-1"));
        assert!(cursor.available);
    }

    #[test]
    fn sidecar_element_at_point_request_maps_coordinates() {
        let request = AutonomousDesktopObserveRequest {
            action: AutonomousDesktopObserveAction::ElementAtPoint,
            display_id: None,
            window_id: None,
            region: None,
            redaction: None,
            x: Some(10),
            y: Some(20),
        };

        let sidecar = sidecar_element_at_point_request(&request);

        assert_eq!(sidecar.x, 10);
        assert_eq!(sidecar.y, 20);
    }

    #[test]
    fn sidecar_accessibility_snapshot_request_sets_safe_limits() {
        let request = AutonomousDesktopObserveRequest {
            action: AutonomousDesktopObserveAction::AccessibilitySnapshot,
            display_id: None,
            window_id: Some("42".into()),
            region: None,
            redaction: None,
            x: None,
            y: None,
        };

        let sidecar = sidecar_accessibility_snapshot_request(&request);

        assert_eq!(sidecar.window_id.as_deref(), Some("42"));
        assert!(!sidecar.focused_only);
        assert!(sidecar.include_children);
        assert_eq!(sidecar.max_depth, Some(5));
        assert_eq!(sidecar.limit, Some(120));
    }

    #[test]
    fn sidecar_ocr_snapshot_request_preserves_capture_bounds_and_redactions() {
        let request = AutonomousDesktopObserveRequest {
            action: AutonomousDesktopObserveAction::OcrSnapshot,
            display_id: Some("main".into()),
            window_id: None,
            region: Some(AutonomousDesktopRegion {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            }),
            redaction: Some(AutonomousDesktopRedactionRequest {
                mode: AutonomousDesktopRedactionMode::Auto,
                private_regions: vec![AutonomousDesktopRegion {
                    x: 5,
                    y: 6,
                    width: 7,
                    height: 8,
                }],
            }),
            x: None,
            y: None,
        };

        let sidecar = sidecar_ocr_snapshot_request(&request);

        assert_eq!(sidecar.display_id.as_deref(), Some("main"));
        assert_eq!(sidecar.region.as_ref().map(|region| region.width), Some(3));
        assert_eq!(
            sidecar.redaction.as_ref().map(|redaction| &redaction.mode),
            Some(&DesktopSidecarRedactionMode::Auto)
        );
        assert_eq!(sidecar.limit, Some(200));
    }

    #[test]
    fn sidecar_capability_payload_decodes_to_contract_shape() {
        let payload = json!({
            "schemaVersion": DESKTOP_SIDECAR_SCHEMA_VERSION,
            "platform": "macos",
            "displayList": true,
            "screenshot": true,
            "windowList": true,
            "appList": true,
            "foregroundState": true,
            "cursorState": true,
            "accessibilitySnapshot": false,
            "ocrSnapshot": false,
            "mouseInput": true,
            "keyboardInput": true,
            "clipboard": false,
            "accessibilityActions": false,
            "menuSelect": false,
            "webrtcStream": false,
            "screenshotFallbackStream": true,
            "manualCloudControl": true
        });

        let decoded =
            serde_json::from_value::<AutonomousDesktopCapabilities>(payload).expect("decode");

        assert_eq!(decoded.schema_version, DESKTOP_SIDECAR_SCHEMA_VERSION);
        assert!(decoded.display_list);
        assert!(decoded.screenshot_fallback_stream);
    }
}
