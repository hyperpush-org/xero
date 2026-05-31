use std::{
    collections::BTreeMap,
    fs,
    io::{self, BufRead, Cursor, Write},
    path::Path,
    process,
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::process::Command;

use base64::Engine as _;
use image::{ImageFormat, RgbaImage};
use serde::Deserialize;
use serde_json::json;
use time::format_description::well_known::Rfc3339;
#[cfg(any(test, target_os = "macos", target_os = "windows"))]
use webrtc::media::Sample;
use webrtc::{
    api::{
        media_engine::{MediaEngine, MIME_TYPE_H264},
        APIBuilder,
    },
    ice_transport::{ice_candidate::RTCIceCandidateInit, ice_server::RTCIceServer},
    peer_connection::{
        configuration::RTCConfiguration, sdp::session_description::RTCSessionDescription,
        RTCPeerConnection,
    },
    rtcp::payload_feedbacks::{
        full_intra_request::FullIntraRequest, picture_loss_indication::PictureLossIndication,
    },
    rtp_transceiver::rtp_codec::RTCRtpCodecCapability,
    rtp_transceiver::rtp_sender::RTCRtpSender,
    stats::StatsReportType,
    track::track_local::{track_local_static_sample::TrackLocalStaticSample, TrackLocal},
};
use xcap::{Monitor, Window};
#[cfg(target_os = "windows")]
use xero_desktop_control_ipc::DesktopSidecarNotificationEntry;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use xero_desktop_control_ipc::DesktopSidecarOcrTextBlock;
use xero_desktop_control_ipc::{
    validate_sidecar_handshake, validate_sidecar_request,
    DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarAccessibilitySnapshotRequest,
    DesktopSidecarApp, DesktopSidecarAppInventoryEntry, DesktopSidecarAppInventoryPayload,
    DesktopSidecarAppListPayload, DesktopSidecarCapabilities, DesktopSidecarClipboardFilesPayload,
    DesktopSidecarClipboardHtmlPayload, DesktopSidecarClipboardImagePayload,
    DesktopSidecarClipboardRtfPayload, DesktopSidecarClipboardTextPayload,
    DesktopSidecarControlRequest, DesktopSidecarCursorStatePayload, DesktopSidecarDisplay,
    DesktopSidecarDisplayArrangementPayload, DesktopSidecarDisplayBounds,
    DesktopSidecarDisplayListPayload, DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody,
    DesktopSidecarForegroundStatePayload, DesktopSidecarHandshake, DesktopSidecarLease,
    DesktopSidecarNotificationSnapshotPayload, DesktopSidecarOcrSnapshotPayload,
    DesktopSidecarOcrSnapshotRequest, DesktopSidecarOperation, DesktopSidecarPermissionGrant,
    DesktopSidecarPermissionStatus, DesktopSidecarPermissionsPayload, DesktopSidecarPointRequest,
    DesktopSidecarRequest, DesktopSidecarResponse, DesktopSidecarScreenshotPayload,
    DesktopSidecarScreenshotRequest, DesktopSidecarSessionDescription,
    DesktopSidecarStreamCapabilitiesPayload, DesktopSidecarStreamMetrics,
    DesktopSidecarStreamPayload, DesktopSidecarStreamQuality, DesktopSidecarStreamRequest,
    DesktopSidecarStreamStatus, DesktopSidecarStreamTransport, DesktopSidecarWindow,
    DesktopSidecarWindowListPayload,
};
#[cfg(target_os = "macos")]
use xero_desktop_control_ipc::{
    DesktopSidecarAccessibilityElement, DesktopSidecarAccessibilitySnapshotRow,
    DesktopSidecarAccessibilitySnapshotTarget,
};

const WEBRTC_MAX_WIDTH: u32 = 1920;
const WEBRTC_MAX_FRAME_RATE: u32 = 30;
const WEBRTC_ICE_GATHER_TIMEOUT: Duration = Duration::from_secs(5);
const H264_ANNEX_B_START_CODE: &[u8; 4] = b"\x00\x00\x00\x01";
const CLIPBOARD_IMAGE_DEFAULT_MAX_BYTES: usize = 512 * 1024;
const CLIPBOARD_IMAGE_MAX_BYTES: usize = 768 * 1024;
const CLIPBOARD_HTML_DEFAULT_MAX_BYTES: usize = 256 * 1024;
const CLIPBOARD_HTML_MAX_BYTES: usize = 512 * 1024;
const CLIPBOARD_RTF_DEFAULT_MAX_BYTES: usize = 256 * 1024;
const CLIPBOARD_RTF_MAX_BYTES: usize = 512 * 1024;
const CLIPBOARD_MAX_FILE_PATHS: usize = 64;
const TYPE_TEXT_PASTE_FIRST_THRESHOLD_CHARS: usize = 160;

#[derive(Clone)]
struct WebRtcStreamConfig {
    stream_id: String,
    display_id: Option<String>,
    max_width: u32,
    max_frame_rate: u32,
    include_cursor: bool,
    quality: DesktopSidecarStreamQuality,
}

#[derive(Clone)]
struct ActiveWebRtcStream {
    peer_connection: Arc<RTCPeerConnection>,
    video_track: Arc<TrackLocalStaticSample>,
    rtp_sender: Arc<RTCRtpSender>,
    stop: Arc<AtomicBool>,
    media_started: Arc<AtomicBool>,
    keyframe_requested: Arc<AtomicBool>,
    config: Arc<Mutex<WebRtcStreamConfig>>,
    metrics: Arc<StreamTelemetry>,
}

#[derive(Default)]
struct StreamTelemetry {
    capture_backend: Mutex<Option<String>>,
    encoder_backend: Mutex<Option<String>>,
    encoder_hardware: AtomicBool,
    fallback_reason: Mutex<Option<String>>,
    capture_frames: AtomicU64,
    capture_dropped_frames: AtomicU64,
    encode_frames: AtomicU64,
    encode_latency_total_ms: AtomicU64,
    bytes_sent: AtomicU64,
    available_outgoing_bitrate_bps: AtomicU64,
    packets_sent: AtomicU64,
    packets_lost: AtomicI64,
    round_trip_time_ms: AtomicU64,
    retransmits: AtomicU64,
    keyframes: AtomicU64,
    started_at: OnceLock<Instant>,
}

struct EncodedVideoSample {
    bytes: Vec<u8>,
    duration: Duration,
    encode_latency_ms: u64,
    keyframe: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum LatestFrameSendResult {
    Stored,
    Replaced,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DesktopTextInputRoute {
    KeyEvents,
    ClipboardPaste,
}

struct LatestFrameSlot<T> {
    frame: Mutex<Option<T>>,
    notify: tokio::sync::Notify,
    closed: AtomicBool,
}

struct LatestFrameSender<T> {
    slot: Arc<LatestFrameSlot<T>>,
}

struct LatestFrameReceiver<T> {
    slot: Arc<LatestFrameSlot<T>>,
}

fn latest_frame_channel<T>() -> (LatestFrameSender<T>, LatestFrameReceiver<T>) {
    let slot = Arc::new(LatestFrameSlot {
        frame: Mutex::new(None),
        notify: tokio::sync::Notify::new(),
        closed: AtomicBool::new(false),
    });
    (
        LatestFrameSender {
            slot: Arc::clone(&slot),
        },
        LatestFrameReceiver { slot },
    )
}

impl<T> LatestFrameSender<T> {
    fn send_replace(&self, frame: T) -> LatestFrameSendResult {
        if self.slot.closed.load(Ordering::Acquire) {
            return LatestFrameSendResult::Closed;
        }
        let Ok(mut pending) = self.slot.frame.lock() else {
            return LatestFrameSendResult::Closed;
        };
        if self.slot.closed.load(Ordering::Acquire) {
            return LatestFrameSendResult::Closed;
        }
        let result = if pending.replace(frame).is_some() {
            LatestFrameSendResult::Replaced
        } else {
            LatestFrameSendResult::Stored
        };
        drop(pending);
        self.slot.notify.notify_one();
        result
    }
}

impl<T> Drop for LatestFrameSender<T> {
    fn drop(&mut self) {
        self.slot.closed.store(true, Ordering::Release);
        self.slot.notify.notify_waiters();
    }
}

impl<T> LatestFrameReceiver<T> {
    async fn recv(&mut self) -> Option<T> {
        loop {
            let notified = self.slot.notify.notified();
            let pending_frame = match self.slot.frame.lock() {
                Ok(mut pending) => pending.take(),
                Err(_) => return None,
            };
            if let Some(frame) = pending_frame {
                return Some(frame);
            }
            if self.slot.closed.load(Ordering::Acquire) {
                return None;
            }
            notified.await;
        }
    }
}

impl<T> Drop for LatestFrameReceiver<T> {
    fn drop(&mut self) {
        self.slot.closed.store(true, Ordering::Release);
        self.slot.notify.notify_waiters();
    }
}

fn main() {
    if let Err(error) = run() {
        let _ = writeln!(io::stderr(), "xero-desktop-sidecar: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let handshake_line = lines
        .next()
        .ok_or_else(|| "missing sidecar handshake".to_string())?
        .map_err(|error| format!("could not read sidecar handshake: {error}"))?;
    let handshake: DesktopSidecarHandshake = serde_json::from_str(&handshake_line)
        .map_err(|error| format!("sidecar handshake was malformed: {error}"))?;
    validate_sidecar_handshake(&handshake, time::OffsetDateTime::now_utc())
        .map_err(|error| error.to_string())?;
    let lease = handshake.into_lease();
    write_response(DesktopSidecarResponse::ok(
        "handshake",
        DesktopSidecarOperation::Health,
        health_payload("ready", "Authenticated sidecar handshake accepted."),
    ))?;

    for line in lines {
        let line = line.map_err(|error| format!("could not read sidecar request: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_request(&lease, &line);
        write_response(response)?;
    }
    Ok(())
}

fn handle_request(lease: &DesktopSidecarLease, line: &str) -> DesktopSidecarResponse {
    let request = match serde_json::from_str::<DesktopSidecarRequest>(line) {
        Ok(request) => request,
        Err(error) => {
            return DesktopSidecarResponse::error(
                "invalid",
                DesktopSidecarOperation::Health,
                DesktopSidecarErrorBody::new(
                    "sidecar_schema_invalid",
                    format!("Sidecar request was malformed: {error}"),
                    false,
                    false,
                ),
            );
        }
    };
    if let Err(error) = validate_sidecar_request(&request, lease, time::OffsetDateTime::now_utc()) {
        return DesktopSidecarResponse::error(
            request.request_id,
            request.operation,
            error.to_error_body(),
        );
    }

    match request.operation {
        DesktopSidecarOperation::Health => DesktopSidecarResponse::ok(
            request.request_id,
            request.operation,
            health_payload("ready", "Desktop sidecar is ready."),
        ),
        DesktopSidecarOperation::Capabilities => json_response(
            request.request_id,
            request.operation,
            sidecar_capabilities(),
        ),
        DesktopSidecarOperation::PermissionsStatus => {
            json_response(request.request_id, request.operation, sidecar_permissions())
        }
        DesktopSidecarOperation::DisplayList => match sidecar_displays() {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
        DesktopSidecarOperation::DisplayArrangement => match sidecar_display_arrangement() {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
        DesktopSidecarOperation::WindowList => match sidecar_windows() {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
        DesktopSidecarOperation::AppList => match sidecar_apps() {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
        DesktopSidecarOperation::AppInventory => match sidecar_app_inventory() {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
        DesktopSidecarOperation::NotificationSnapshot => json_response(
            request.request_id,
            request.operation,
            sidecar_notification_snapshot(),
        ),
        DesktopSidecarOperation::ForegroundState => match sidecar_foreground_state() {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
        DesktopSidecarOperation::CursorState => match sidecar_cursor_state() {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
        DesktopSidecarOperation::ElementAtPoint => {
            match sidecar_element_at_point(request.payload) {
                Ok(payload) => json_response(request.request_id, request.operation, payload),
                Err(error) => sidecar_error_response(request.request_id, request.operation, error),
            }
        }
        DesktopSidecarOperation::AccessibilitySnapshot => {
            match sidecar_accessibility_snapshot(request.payload) {
                Ok(payload) => json_response(request.request_id, request.operation, payload),
                Err(error) => sidecar_error_response(request.request_id, request.operation, error),
            }
        }
        DesktopSidecarOperation::OcrSnapshot => match sidecar_ocr_snapshot(request.payload) {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
        DesktopSidecarOperation::ClipboardReadText => match sidecar_clipboard_read_text() {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
        DesktopSidecarOperation::ClipboardReadHtml => {
            match sidecar_clipboard_read_html(request.payload) {
                Ok(payload) => json_response(request.request_id, request.operation, payload),
                Err(error) => sidecar_error_response(request.request_id, request.operation, error),
            }
        }
        DesktopSidecarOperation::ClipboardReadRtf => {
            match sidecar_clipboard_read_rtf(request.payload) {
                Ok(payload) => json_response(request.request_id, request.operation, payload),
                Err(error) => sidecar_error_response(request.request_id, request.operation, error),
            }
        }
        DesktopSidecarOperation::ClipboardReadImage => {
            match sidecar_clipboard_read_image(request.payload) {
                Ok(payload) => json_response(request.request_id, request.operation, payload),
                Err(error) => sidecar_error_response(request.request_id, request.operation, error),
            }
        }
        DesktopSidecarOperation::ClipboardReadFiles => match sidecar_clipboard_read_files() {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
        DesktopSidecarOperation::Screenshot => match sidecar_screenshot(request.payload) {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
        DesktopSidecarOperation::StreamCapabilities => json_response(
            request.request_id,
            request.operation,
            sidecar_stream_capabilities(),
        ),
        DesktopSidecarOperation::StreamStart
        | DesktopSidecarOperation::StreamOffer
        | DesktopSidecarOperation::StreamAnswer
        | DesktopSidecarOperation::StreamIceCandidate
        | DesktopSidecarOperation::StreamStop
        | DesktopSidecarOperation::StreamStatus
        | DesktopSidecarOperation::StreamSetQuality
        | DesktopSidecarOperation::StreamRequestKeyframe => {
            match sidecar_stream(request.operation, request.payload) {
                Ok(payload) => json_response(request.request_id, request.operation, payload),
                Err(error) => sidecar_error_response(request.request_id, request.operation, error),
            }
        }
        DesktopSidecarOperation::MouseDown
        | DesktopSidecarOperation::MouseMove
        | DesktopSidecarOperation::MouseClick
        | DesktopSidecarOperation::MouseDoubleClick
        | DesktopSidecarOperation::MouseRightClick
        | DesktopSidecarOperation::MouseDrag
        | DesktopSidecarOperation::MouseDragMove
        | DesktopSidecarOperation::MouseUp
        | DesktopSidecarOperation::Scroll
        | DesktopSidecarOperation::KeyPress
        | DesktopSidecarOperation::Hotkey
        | DesktopSidecarOperation::TypeText
        | DesktopSidecarOperation::PasteText
        | DesktopSidecarOperation::ClipboardWriteText
        | DesktopSidecarOperation::ClipboardWriteHtml
        | DesktopSidecarOperation::ClipboardWriteRtf
        | DesktopSidecarOperation::ClipboardWriteImage
        | DesktopSidecarOperation::ClipboardWriteFiles
        | DesktopSidecarOperation::FileDrop
        | DesktopSidecarOperation::FocusWindow
        | DesktopSidecarOperation::WindowMaximize
        | DesktopSidecarOperation::WindowMinimize
        | DesktopSidecarOperation::WindowRestore
        | DesktopSidecarOperation::WindowMoveResize
        | DesktopSidecarOperation::WindowClose
        | DesktopSidecarOperation::ActivateApp
        | DesktopSidecarOperation::LaunchApp
        | DesktopSidecarOperation::QuitApp
        | DesktopSidecarOperation::AxPress
        | DesktopSidecarOperation::AxSetValue
        | DesktopSidecarOperation::AxFocus
        | DesktopSidecarOperation::AxSelect
        | DesktopSidecarOperation::AxConfirm
        | DesktopSidecarOperation::AxCancel
        | DesktopSidecarOperation::AxIncrement
        | DesktopSidecarOperation::AxDecrement
        | DesktopSidecarOperation::AxExpand
        | DesktopSidecarOperation::AxCollapse
        | DesktopSidecarOperation::AxScrollToVisible
        | DesktopSidecarOperation::AxToggle
        | DesktopSidecarOperation::MenuSelect
        | DesktopSidecarOperation::DockItemPress
        | DesktopSidecarOperation::StatusItemPress
        | DesktopSidecarOperation::FileDialogSetPath
        | DesktopSidecarOperation::FileDialogConfirm
        | DesktopSidecarOperation::CancelCurrentAction => {
            match sidecar_control(request.operation, request.payload) {
                Ok(payload) => json_response(request.request_id, request.operation, payload),
                Err(error) => sidecar_error_response(request.request_id, request.operation, error),
            }
        }
    }
}

fn json_response<T: serde::Serialize>(
    request_id: String,
    operation: DesktopSidecarOperation,
    payload: T,
) -> DesktopSidecarResponse {
    match serde_json::to_value(payload) {
        Ok(value) => DesktopSidecarResponse::ok(request_id, operation, value),
        Err(error) => sidecar_error_response(
            request_id,
            operation,
            DesktopSidecarErrorBody::new(
                "sidecar_response_encode_failed",
                format!("Sidecar could not encode `{operation:?}` response: {error}"),
                false,
                false,
            ),
        ),
    }
}

fn sidecar_error_response(
    request_id: String,
    operation: DesktopSidecarOperation,
    error: DesktopSidecarErrorBody,
) -> DesktopSidecarResponse {
    DesktopSidecarResponse::error(request_id, operation, error)
}

fn sidecar_capabilities() -> DesktopSidecarCapabilities {
    DesktopSidecarCapabilities {
        schema_version: xero_desktop_control_ipc::DESKTOP_SIDECAR_SCHEMA_VERSION,
        platform: std::env::consts::OS.into(),
        display_list: true,
        screenshot: true,
        window_list: true,
        app_list: true,
        notification_observation: cfg!(target_os = "windows"),
        foreground_state: true,
        cursor_state: cfg!(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        )),
        accessibility_snapshot: cfg!(any(target_os = "macos", target_os = "windows")),
        ocr_snapshot: cfg!(any(target_os = "macos", target_os = "windows")),
        mouse_input: cfg!(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        )),
        keyboard_input: cfg!(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        )),
        clipboard: cfg!(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        )),
        window_focus: cfg!(target_os = "windows"),
        app_control: cfg!(target_os = "windows"),
        accessibility_actions: cfg!(any(target_os = "macos", target_os = "windows")),
        menu_select: cfg!(any(target_os = "macos", target_os = "windows")),
        webrtc_stream: native_webrtc_stream_available(),
        screenshot_fallback_stream: true,
        manual_cloud_control: cfg!(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        )),
    }
}

fn sidecar_permissions() -> DesktopSidecarPermissionsPayload {
    if cfg!(target_os = "windows") {
        return DesktopSidecarPermissionsPayload {
            permissions: windows_desktop_permissions(),
        };
    }

    DesktopSidecarPermissionsPayload {
        permissions: vec![
            permission(
                "Screen Recording",
                desktop_screen_recording_permission_status(),
                &["screenshot", "ocr_snapshot", "stream"],
                "Grant screen capture permission in the local desktop session, then retry.",
            ),
            permission(
                "Accessibility",
                desktop_accessibility_permission_status(),
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
                desktop_input_monitoring_permission_status(),
                &["keyboard", "hotkey"],
                "Grant Input Monitoring only if the selected keyboard backend requires it.",
            ),
            permission(
                "Notifications",
                DesktopSidecarPermissionGrant::Unsupported,
                &["notification_snapshot"],
                "macOS does not expose other apps' Notification Center history through a public sidecar API; use Accessibility or OCR when notification UI is visible.",
            ),
            permission(
                "Remote Desktop Portal",
                if cfg!(target_os = "linux") {
                    DesktopSidecarPermissionGrant::Unknown
                } else {
                    DesktopSidecarPermissionGrant::Unsupported
                },
                &["wayland_capture", "wayland_input"],
                "Approve the Wayland portal prompt in the local desktop session.",
            ),
        ],
    }
}

fn windows_desktop_permissions() -> Vec<DesktopSidecarPermissionStatus> {
    vec![
        permission(
            "Screen Capture",
            DesktopSidecarPermissionGrant::Granted,
            &["screenshot", "stream"],
            "Windows desktop capture is available in the active user session; no macOS-style privacy grant is required.",
        ),
        permission(
            "Desktop Input",
            DesktopSidecarPermissionGrant::Granted,
            &[
                "mouse",
                "keyboard",
                "clipboard",
                "window_focus",
                "app_control",
            ],
            "Windows desktop input is brokered through the active user session and the Computer Use controller lock.",
        ),
        permission(
            "UI Automation",
            DesktopSidecarPermissionGrant::Granted,
            &[
                "accessibility_snapshot",
                "accessibility_actions",
                "menu_select",
            ],
            "Windows UI Automation is available in the active user session for inspectable controls. Elevated or secure-desktop surfaces still require local user approval.",
        ),
        permission(
            "OCR",
            DesktopSidecarPermissionGrant::Granted,
            &["ocr_snapshot"],
            "Windows OCR uses Windows.Media.Ocr in the active user session. If the OCR engine or language pack is unavailable, the sidecar returns a performed=false diagnostic.",
        ),
        permission(
            "Notification Listener",
            DesktopSidecarPermissionGrant::Unknown,
            &["notification_snapshot"],
            "Windows notification observation uses UserNotificationListener and only returns notification text after the active user grants notification-listener access.",
        ),
    ]
}

#[cfg(target_os = "macos")]
fn desktop_screen_recording_permission_status() -> DesktopSidecarPermissionGrant {
    permission_grant_from_bool(unsafe { CGPreflightScreenCaptureAccess() })
}

#[cfg(not(target_os = "macos"))]
fn desktop_screen_recording_permission_status() -> DesktopSidecarPermissionGrant {
    if cfg!(target_os = "windows") {
        DesktopSidecarPermissionGrant::Granted
    } else if cfg!(target_os = "linux") {
        DesktopSidecarPermissionGrant::Unknown
    } else {
        DesktopSidecarPermissionGrant::Unsupported
    }
}

#[cfg(target_os = "macos")]
fn desktop_accessibility_permission_status() -> DesktopSidecarPermissionGrant {
    permission_grant_from_bool(unsafe { AXIsProcessTrusted() })
}

#[cfg(not(target_os = "macos"))]
fn desktop_accessibility_permission_status() -> DesktopSidecarPermissionGrant {
    DesktopSidecarPermissionGrant::Unsupported
}

#[cfg(target_os = "macos")]
fn desktop_input_monitoring_permission_status() -> DesktopSidecarPermissionGrant {
    permission_grant_from_bool(unsafe { CGPreflightListenEventAccess() })
}

#[cfg(not(target_os = "macos"))]
fn desktop_input_monitoring_permission_status() -> DesktopSidecarPermissionGrant {
    if cfg!(target_os = "windows") {
        DesktopSidecarPermissionGrant::Granted
    } else {
        DesktopSidecarPermissionGrant::Unsupported
    }
}

#[cfg(target_os = "macos")]
fn permission_grant_from_bool(granted: bool) -> DesktopSidecarPermissionGrant {
    if granted {
        DesktopSidecarPermissionGrant::Granted
    } else {
        DesktopSidecarPermissionGrant::Denied
    }
}

fn permission(
    name: &str,
    status: DesktopSidecarPermissionGrant,
    required_for: &[&str],
    remediation: &str,
) -> DesktopSidecarPermissionStatus {
    DesktopSidecarPermissionStatus {
        name: name.into(),
        status,
        required_for: required_for.iter().map(|value| (*value).into()).collect(),
        remediation: remediation.into(),
    }
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGPreflightScreenCaptureAccess() -> bool;
}

fn sidecar_displays() -> Result<DesktopSidecarDisplayListPayload, DesktopSidecarErrorBody> {
    let monitors = Monitor::all().map_err(|error| {
        DesktopSidecarErrorBody::new(
            "sidecar_display_list_failed",
            format!("Desktop sidecar could not enumerate displays: {error}"),
            true,
            false,
        )
    })?;
    let displays = monitors
        .iter()
        .map(|monitor| DesktopSidecarDisplay {
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
        .collect();
    Ok(DesktopSidecarDisplayListPayload { displays })
}

fn sidecar_display_arrangement(
) -> Result<DesktopSidecarDisplayArrangementPayload, DesktopSidecarErrorBody> {
    let displays = sidecar_displays()?.displays;
    Ok(display_arrangement_from_displays(displays))
}

fn display_arrangement_from_displays(
    displays: Vec<DesktopSidecarDisplay>,
) -> DesktopSidecarDisplayArrangementPayload {
    let display_count = displays.len();
    let virtual_bounds = display_virtual_bounds(&displays);
    let primary_display_id = displays
        .iter()
        .find(|display| display.primary)
        .or_else(|| displays.first())
        .map(|display| display.display_id.clone());
    let scale_factors = display_scale_factors(&displays);
    let has_overlaps = displays_have_overlaps(&displays);
    let has_gaps_in_virtual_bounds =
        displays_have_gaps_in_virtual_bounds(&displays, &virtual_bounds, has_overlaps);
    let mut diagnostics = Vec::new();
    if displays.is_empty() {
        diagnostics.push("display_arrangement_empty".into());
    }
    if displays
        .iter()
        .any(|display| display.width == 0 || display.height == 0)
    {
        diagnostics.push("display_arrangement_contains_zero_sized_display".into());
    }
    if primary_display_id.is_none() {
        diagnostics.push("display_arrangement_primary_unknown".into());
    }
    if scale_factors.len() > 1 {
        diagnostics.push("display_arrangement_multiple_scale_factors".into());
    }
    if has_overlaps {
        diagnostics.push("display_arrangement_overlapping_bounds".into());
    }
    if has_gaps_in_virtual_bounds {
        diagnostics.push("display_arrangement_virtual_bounds_include_gaps".into());
    }

    DesktopSidecarDisplayArrangementPayload {
        displays,
        display_count,
        virtual_bounds,
        primary_display_id,
        scale_factors,
        has_overlaps,
        has_gaps_in_virtual_bounds,
        diagnostics,
    }
}

fn display_virtual_bounds(displays: &[DesktopSidecarDisplay]) -> DesktopSidecarDisplayBounds {
    let Some(first) = displays.first() else {
        return DesktopSidecarDisplayBounds {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
    };
    let mut min_x = first.x as i64;
    let mut min_y = first.y as i64;
    let mut max_x = first.x as i64 + first.width as i64;
    let mut max_y = first.y as i64 + first.height as i64;
    for display in displays.iter().skip(1) {
        let left = display.x as i64;
        let top = display.y as i64;
        let right = left + display.width as i64;
        let bottom = top + display.height as i64;
        min_x = min_x.min(left);
        min_y = min_y.min(top);
        max_x = max_x.max(right);
        max_y = max_y.max(bottom);
    }
    DesktopSidecarDisplayBounds {
        x: min_x.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        y: min_y.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        width: (max_x - min_x).clamp(0, u32::MAX as i64) as u32,
        height: (max_y - min_y).clamp(0, u32::MAX as i64) as u32,
    }
}

fn display_scale_factors(displays: &[DesktopSidecarDisplay]) -> Vec<f32> {
    let mut scale_factors = displays
        .iter()
        .map(|display| display.scale_factor)
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .collect::<Vec<_>>();
    scale_factors.sort_by(|left, right| left.total_cmp(right));
    scale_factors.dedup_by(|left, right| (*left - *right).abs() < 0.001);
    scale_factors
}

fn displays_have_overlaps(displays: &[DesktopSidecarDisplay]) -> bool {
    displays.iter().enumerate().any(|(index, left)| {
        displays
            .iter()
            .skip(index + 1)
            .any(|right| display_overlap_area(left, right) > 0)
    })
}

fn displays_have_gaps_in_virtual_bounds(
    displays: &[DesktopSidecarDisplay],
    bounds: &DesktopSidecarDisplayBounds,
    has_overlaps: bool,
) -> bool {
    if displays.len() <= 1 || has_overlaps {
        return false;
    }
    let total_display_area = displays
        .iter()
        .map(|display| display.width as u64 * display.height as u64)
        .sum::<u64>();
    let virtual_area = bounds.width as u64 * bounds.height as u64;
    virtual_area > total_display_area
}

fn display_overlap_area(left: &DesktopSidecarDisplay, right: &DesktopSidecarDisplay) -> u64 {
    let left_right = left.x as i64 + left.width as i64;
    let left_bottom = left.y as i64 + left.height as i64;
    let right_right = right.x as i64 + right.width as i64;
    let right_bottom = right.y as i64 + right.height as i64;
    let width = left_right.min(right_right) - (left.x as i64).max(right.x as i64);
    let height = left_bottom.min(right_bottom) - (left.y as i64).max(right.y as i64);
    if width <= 0 || height <= 0 {
        0
    } else {
        width as u64 * height as u64
    }
}

fn sidecar_windows() -> Result<DesktopSidecarWindowListPayload, DesktopSidecarErrorBody> {
    sidecar_window_rows().map(|windows| DesktopSidecarWindowListPayload { windows })
}

fn sidecar_window_rows() -> Result<Vec<DesktopSidecarWindow>, DesktopSidecarErrorBody> {
    let windows = Window::all().map_err(|error| {
        DesktopSidecarErrorBody::new(
            "sidecar_window_list_failed",
            format!("Desktop sidecar could not enumerate windows: {error}"),
            true,
            false,
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
            Some(DesktopSidecarWindow {
                window_id: window.id().unwrap_or_default().to_string(),
                app_name: window.app_name().unwrap_or_else(|_| "Unknown".into()),
                title: desktop_label_preview(&window.title().unwrap_or_default()),
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

fn sidecar_apps() -> Result<DesktopSidecarAppListPayload, DesktopSidecarErrorBody> {
    sidecar_window_rows().map(|windows| DesktopSidecarAppListPayload {
        apps: apps_from_windows(&windows),
    })
}

fn sidecar_app_inventory() -> Result<DesktopSidecarAppInventoryPayload, DesktopSidecarErrorBody> {
    let mut diagnostics = Vec::new();
    let windows = match sidecar_window_rows() {
        Ok(windows) => windows,
        Err(error) => {
            diagnostics.push(format!("running_window_list_failed: {}", error.message));
            Vec::new()
        }
    };
    let mut apps = platform_app_inventory_entries(&mut diagnostics);
    merge_running_apps_into_inventory(&mut apps, apps_from_windows(&windows));
    apps.sort_by(|left, right| {
        left.app_name
            .to_ascii_lowercase()
            .cmp(&right.app_name.to_ascii_lowercase())
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.launch_target.cmp(&right.launch_target))
    });
    let mut sources = apps
        .iter()
        .map(|app| app.source.clone())
        .collect::<Vec<_>>();
    sources.sort();
    sources.dedup();
    let count = apps.len();
    Ok(DesktopSidecarAppInventoryPayload {
        apps,
        count,
        sources,
        diagnostics,
    })
}

fn sidecar_notification_snapshot() -> DesktopSidecarNotificationSnapshotPayload {
    platform_notification_snapshot()
}

#[cfg(target_os = "windows")]
fn platform_notification_snapshot() -> DesktopSidecarNotificationSnapshotPayload {
    match windows_notification_snapshot() {
        Ok(mut payload) => {
            payload.count = payload.notifications.len();
            payload
        }
        Err(error) => DesktopSidecarNotificationSnapshotPayload {
            available: false,
            permission_status: "unknown".into(),
            notifications: Vec::new(),
            count: 0,
            source: "windows_user_notification_listener".into(),
            diagnostics: vec![error],
        },
    }
}

#[cfg(target_os = "macos")]
fn platform_notification_snapshot() -> DesktopSidecarNotificationSnapshotPayload {
    DesktopSidecarNotificationSnapshotPayload {
        available: false,
        permission_status: "unsupported".into(),
        notifications: Vec::new(),
        count: 0,
        source: "macos_notification_center_platform_policy".into(),
        diagnostics: vec![
            "macos_notification_center_observation_not_available_to_sidecar".into(),
            "use_accessibility_snapshot_or_ocr_when_notification_center_is_visible".into(),
        ],
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_notification_snapshot() -> DesktopSidecarNotificationSnapshotPayload {
    DesktopSidecarNotificationSnapshotPayload {
        available: false,
        permission_status: "unsupported".into(),
        notifications: Vec::new(),
        count: 0,
        source: "unsupported_platform".into(),
        diagnostics: vec!["notification_observation_unsupported_on_platform".into()],
    }
}

#[cfg(target_os = "windows")]
fn windows_notification_snapshot() -> Result<DesktopSidecarNotificationSnapshotPayload, String> {
    let script = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Runtime.WindowsRuntime

function Convert-XeroWinRtAsyncOperation {
  param(
    [Parameter(Mandatory = $true)] $Operation,
    [Parameter(Mandatory = $true)] [Type] $ResultType
  )
  $asTaskMethod = [System.WindowsRuntimeSystemExtensions].GetMethods() |
    Where-Object {
      $_.Name -eq 'AsTask' -and
      $_.IsGenericMethodDefinition -and
      $_.GetParameters().Count -eq 1 -and
      $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
    } |
    Select-Object -First 1
  if ($null -eq $asTaskMethod) {
    throw 'Could not locate the WinRT AsTask adapter.'
  }
  $task = $asTaskMethod.MakeGenericMethod($ResultType).Invoke($null, @($Operation))
  $task.Wait()
  return $task.Result
}

$listener = [Windows.UI.Notifications.Management.UserNotificationListener, Windows.UI.Notifications, ContentType = WindowsRuntime]::Current
$accessStatus = $listener.GetAccessStatus().ToString()
if ($accessStatus -ne 'Allowed') {
  [PSCustomObject]@{
    available = $false
    permissionStatus = $accessStatus
    source = 'windows_user_notification_listener'
    notifications = @()
    diagnostics = @("windows_user_notification_listener_access_$($accessStatus.ToLowerInvariant())")
  } | ConvertTo-Json -Depth 8 -Compress
  return
}

$notificationType = [Windows.UI.Notifications.UserNotification, Windows.UI.Notifications, ContentType = WindowsRuntime]
$listType = [System.Collections.Generic.IReadOnlyList``1].MakeGenericType($notificationType)
$kind = [Windows.UI.Notifications.NotificationKinds, Windows.UI.Notifications, ContentType = WindowsRuntime]::Toast
$notifications = Convert-XeroWinRtAsyncOperation $listener.GetNotificationsAsync($kind) $listType
$rows = @()
foreach ($notification in @($notifications)) {
  $texts = @()
  foreach ($binding in @($notification.Notification.Visual.Bindings)) {
    foreach ($text in @($binding.GetTextElements())) {
      if (-not [string]::IsNullOrWhiteSpace($text.Text)) {
        $texts += [string]$text.Text
      }
    }
  }
  $appName = $null
  $appUserModelId = $null
  if ($null -ne $notification.AppInfo) {
    $appUserModelId = [string]$notification.AppInfo.AppUserModelId
    if ($null -ne $notification.AppInfo.DisplayInfo) {
      $appName = [string]$notification.AppInfo.DisplayInfo.DisplayName
    }
  }
  $deliveredAt = $null
  if ($null -ne $notification.CreationTime) {
    $deliveredAt = $notification.CreationTime.ToString('o')
  }
  $rows += [PSCustomObject]@{
    id = [string]$notification.Id
    appName = $appName
    title = if ($texts.Count -gt 0) { $texts[0] } else { $null }
    body = if ($texts.Count -gt 1) { ($texts | Select-Object -Skip 1) -join "`n" } else { $null }
    subtitle = $appUserModelId
    deliveredAt = $deliveredAt
    source = 'windows_user_notification_listener'
    diagnostics = @()
  }
}

[PSCustomObject]@{
  available = $true
  permissionStatus = $accessStatus
  source = 'windows_user_notification_listener'
  notifications = $rows
  diagnostics = @()
} | ConvertTo-Json -Depth 8 -Compress
"#;
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|error| format!("windows_notification_listener_failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "windows_notification_listener_failed: {}",
            sidecar_command_output_message(&output.stdout, &output.stderr)
        ));
    }
    windows_notification_snapshot_from_json(&output.stdout)
        .map_err(|error| format!("windows_notification_listener_decode_failed: {error}"))
}

#[cfg(target_os = "windows")]
fn windows_notification_snapshot_from_json(
    bytes: &[u8],
) -> Result<DesktopSidecarNotificationSnapshotPayload, serde_json::Error> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct WindowsNotificationSnapshot {
        available: bool,
        permission_status: Option<String>,
        source: Option<String>,
        #[serde(default)]
        notifications: Vec<WindowsNotificationRow>,
        #[serde(default)]
        diagnostics: Vec<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct WindowsNotificationRow {
        id: Option<String>,
        app_name: Option<String>,
        title: Option<String>,
        body: Option<String>,
        subtitle: Option<String>,
        delivered_at: Option<String>,
        source: Option<String>,
        #[serde(default)]
        diagnostics: Vec<String>,
    }

    let payload = serde_json::from_slice::<WindowsNotificationSnapshot>(bytes)?;
    let notifications = payload
        .notifications
        .into_iter()
        .map(|row| DesktopSidecarNotificationEntry {
            id: row.id.unwrap_or_else(|| "unknown".into()),
            app_name: row.app_name.filter(|value| !value.trim().is_empty()),
            title: row.title.filter(|value| !value.trim().is_empty()),
            body: row.body.filter(|value| !value.trim().is_empty()),
            subtitle: row.subtitle.filter(|value| !value.trim().is_empty()),
            delivered_at: row.delivered_at.filter(|value| !value.trim().is_empty()),
            source: row
                .source
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "windows_user_notification_listener".into()),
            diagnostics: row.diagnostics,
        })
        .collect::<Vec<_>>();
    let count = notifications.len();
    Ok(DesktopSidecarNotificationSnapshotPayload {
        available: payload.available,
        permission_status: payload
            .permission_status
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "unknown".into()),
        notifications,
        count,
        source: payload
            .source
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "windows_user_notification_listener".into()),
        diagnostics: payload.diagnostics,
    })
}

fn merge_running_apps_into_inventory(
    inventory: &mut Vec<DesktopSidecarAppInventoryEntry>,
    running_apps: Vec<DesktopSidecarApp>,
) {
    for running in running_apps {
        let matching_index =
            inventory.iter().position(|entry| {
                entry.bundle_id.as_deref().is_some_and(|bundle_id| {
                    app_inventory_names_match(bundle_id, &running.app_name)
                }) || app_inventory_names_match(&entry.app_name, &running.app_name)
            });
        if let Some(index) = matching_index {
            let entry = &mut inventory[index];
            entry.running = true;
            entry.pid = Some(running.pid);
            entry.window_count = running.window_count;
            entry.focused = running.focused;
        } else {
            inventory.push(DesktopSidecarAppInventoryEntry {
                app_name: running.app_name.clone(),
                bundle_id: None,
                executable_path: None,
                launch_target: Some(running.app_name),
                launch_kind: "app_name".into(),
                source: "running_windows".into(),
                installed: false,
                running: true,
                pid: Some(running.pid),
                window_count: running.window_count,
                focused: running.focused,
                diagnostics: vec!["not_matched_to_installed_app".into()],
            });
        }
    }
}

fn app_inventory_names_match(left: &str, right: &str) -> bool {
    let left = normalize_app_inventory_name(left);
    let right = normalize_app_inventory_name(right);
    !left.is_empty() && left == right
}

fn normalize_app_inventory_name(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".app")
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-' && *ch != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(target_os = "macos")]
fn platform_app_inventory_entries(
    diagnostics: &mut Vec<String>,
) -> Vec<DesktopSidecarAppInventoryEntry> {
    let mut entries = Vec::new();
    for directory in macos_application_directories() {
        let source = directory.to_string_lossy().into_owned();
        let Ok(read_dir) = fs::read_dir(&directory) else {
            continue;
        };
        for item in read_dir.flatten() {
            let path = item.path();
            if path.extension().and_then(|value| value.to_str()) != Some("app") {
                continue;
            }
            entries.push(macos_app_inventory_entry(&path, &source));
        }
    }
    dedupe_app_inventory_entries(&mut entries);
    if entries.is_empty() {
        diagnostics.push("macos_app_inventory_no_application_bundles_found".into());
    }
    entries
}

#[cfg(target_os = "macos")]
fn macos_application_directories() -> Vec<PathBuf> {
    let mut directories = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        directories.push(PathBuf::from(home).join("Applications"));
    }
    directories
}

#[cfg(target_os = "macos")]
fn macos_app_inventory_entry(path: &Path, source: &str) -> DesktopSidecarAppInventoryEntry {
    let info_plist = path.join("Contents/Info.plist");
    let bundle_id = read_plist_string_key(&info_plist, "CFBundleIdentifier");
    let app_name = read_plist_string_key(&info_plist, "CFBundleDisplayName")
        .or_else(|| read_plist_string_key(&info_plist, "CFBundleName"))
        .or_else(|| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "Unknown".into());
    let executable_path = read_plist_string_key(&info_plist, "CFBundleExecutable")
        .map(|executable| path.join("Contents/MacOS").join(executable))
        .filter(|path| path.exists())
        .map(|path| path.to_string_lossy().into_owned());
    let launch_target = bundle_id.clone().or_else(|| {
        path.file_stem()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
    });
    DesktopSidecarAppInventoryEntry {
        app_name,
        bundle_id,
        executable_path,
        launch_target,
        launch_kind: "bundle_id_or_app_name".into(),
        source: source.into(),
        installed: true,
        running: false,
        pid: None,
        window_count: 0,
        focused: false,
        diagnostics: if info_plist.exists() {
            Vec::new()
        } else {
            vec!["info_plist_missing".into()]
        },
    }
}

#[cfg(target_os = "windows")]
fn platform_app_inventory_entries(
    diagnostics: &mut Vec<String>,
) -> Vec<DesktopSidecarAppInventoryEntry> {
    match windows_start_app_inventory_entries() {
        Ok(mut entries) => {
            dedupe_app_inventory_entries(&mut entries);
            entries
        }
        Err(error) => {
            diagnostics.push(error);
            Vec::new()
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_start_app_inventory_entries() -> Result<Vec<DesktopSidecarAppInventoryEntry>, String> {
    let script = r#"
$ErrorActionPreference = 'Stop'
$shell = New-Object -ComObject Shell.Application
$folder = $shell.Namespace('shell:AppsFolder')
if ($null -eq $folder) { @() | ConvertTo-Json -Compress; return }
$items = @()
foreach ($item in $folder.Items()) {
  $items += [PSCustomObject]@{
    appName = $item.Name
    appId = $item.Path
  }
}
$items | ConvertTo-Json -Compress
"#;
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|error| format!("windows_app_inventory_powershell_failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "windows_app_inventory_failed: {}",
            sidecar_command_output_message(&output.stdout, &output.stderr)
        ));
    }
    windows_start_app_inventory_entries_from_json(&output.stdout)
        .map_err(|error| format!("windows_app_inventory_decode_failed: {error}"))
}

#[cfg(target_os = "windows")]
fn windows_start_app_inventory_entries_from_json(
    bytes: &[u8],
) -> Result<Vec<DesktopSidecarAppInventoryEntry>, serde_json::Error> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct WindowsStartApp {
        app_name: Option<String>,
        app_id: Option<String>,
    }

    let value = serde_json::from_slice::<serde_json::Value>(bytes)?;
    let rows = match value {
        serde_json::Value::Array(values) => values,
        serde_json::Value::Object(_) => vec![value],
        _ => Vec::new(),
    };
    rows.into_iter()
        .map(|value| {
            let row = serde_json::from_value::<WindowsStartApp>(value)?;
            let app_name = row
                .app_name
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "Unknown".into());
            let launch_target = row.app_id.filter(|value| !value.trim().is_empty());
            Ok(DesktopSidecarAppInventoryEntry {
                app_name,
                bundle_id: launch_target.clone(),
                executable_path: None,
                launch_target,
                launch_kind: "apps_folder_app_id".into(),
                source: "windows_apps_folder".into(),
                installed: true,
                running: false,
                pid: None,
                window_count: 0,
                focused: false,
                diagnostics: Vec::new(),
            })
        })
        .collect()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_app_inventory_entries(
    diagnostics: &mut Vec<String>,
) -> Vec<DesktopSidecarAppInventoryEntry> {
    diagnostics.push("installed_app_inventory_unsupported_on_platform".into());
    Vec::new()
}

fn dedupe_app_inventory_entries(entries: &mut Vec<DesktopSidecarAppInventoryEntry>) {
    let mut deduped = Vec::new();
    for entry in entries.drain(..) {
        let is_duplicate = deduped
            .iter()
            .any(|existing: &DesktopSidecarAppInventoryEntry| {
                match (
                    existing.bundle_id.as_deref(),
                    entry.bundle_id.as_deref(),
                    existing.launch_target.as_deref(),
                    entry.launch_target.as_deref(),
                ) {
                    (Some(left), Some(right), _, _) if left.eq_ignore_ascii_case(right) => true,
                    (_, _, Some(left), Some(right)) if left.eq_ignore_ascii_case(right) => true,
                    _ => {
                        app_inventory_names_match(&existing.app_name, &entry.app_name)
                            && existing.source == entry.source
                    }
                }
            });
        if !is_duplicate {
            deduped.push(entry);
        }
    }
    *entries = deduped;
}

fn read_plist_string_key(path: &Path, key: &str) -> Option<String> {
    if let Ok(text) = fs::read_to_string(path) {
        if let Some(value) = read_plist_string_key_from_xml(&text, key) {
            return Some(value);
        }
    }
    read_plist_string_key_with_plutil(path, key)
}

#[cfg(target_os = "macos")]
fn read_plist_string_key_with_plutil(path: &Path, key: &str) -> Option<String> {
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", key, "raw", "-o", "-"])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

#[cfg(not(target_os = "macos"))]
fn read_plist_string_key_with_plutil(_path: &Path, _key: &str) -> Option<String> {
    None
}

fn read_plist_string_key_from_xml(text: &str, key: &str) -> Option<String> {
    let key_marker = format!("<key>{key}</key>");
    let after_key = text.split_once(&key_marker)?.1;
    let after_open = after_key.split_once("<string>")?.1;
    let value = after_open.split_once("</string>")?.0.trim();
    if value.is_empty() {
        None
    } else {
        Some(unescape_plist_string(value))
    }
}

fn unescape_plist_string(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(target_os = "windows")]
fn sidecar_command_output_message(stdout: &[u8], stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = String::from_utf8_lossy(stdout).trim().to_string();
    if stdout.is_empty() {
        "command exited without output".into()
    } else {
        stdout
    }
}

fn sidecar_foreground_state(
) -> Result<DesktopSidecarForegroundStatePayload, DesktopSidecarErrorBody> {
    sidecar_window_rows().map(|windows| DesktopSidecarForegroundStatePayload {
        foreground: windows.into_iter().find(|window| window.focused),
    })
}

#[cfg(target_os = "macos")]
fn sidecar_cursor_state() -> Result<DesktopSidecarCursorStatePayload, DesktopSidecarErrorBody> {
    use core_graphics::{
        event::CGEvent,
        event_source::{CGEventSource, CGEventSourceStateID},
    };

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| cursor_state_error())?;
    let event = CGEvent::new(source).map_err(|_| cursor_state_error())?;
    let point = event.location();
    Ok(DesktopSidecarCursorStatePayload {
        x: point.x as i32,
        y: point.y as i32,
        display_id: Monitor::from_point(point.x as i32, point.y as i32)
            .ok()
            .and_then(|monitor| monitor.id().ok())
            .map(|id| id.to_string()),
        available: true,
    })
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn sidecar_cursor_state() -> Result<DesktopSidecarCursorStatePayload, DesktopSidecarErrorBody> {
    let enigo = cross_platform_input::new_enigo()?;
    let (x, y) = cross_platform_input::cursor_location(&enigo)?;
    Ok(DesktopSidecarCursorStatePayload {
        x,
        y,
        display_id: Monitor::from_point(x, y)
            .ok()
            .and_then(|monitor| monitor.id().ok())
            .map(|id| id.to_string()),
        available: true,
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn sidecar_cursor_state() -> Result<DesktopSidecarCursorStatePayload, DesktopSidecarErrorBody> {
    Err(unimplemented_operation())
}

fn sidecar_element_at_point(
    payload: serde_json::Value,
) -> Result<DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody> {
    let request =
        serde_json::from_value::<DesktopSidecarPointRequest>(payload).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "sidecar_schema_invalid",
                format!("Element-at-point request payload was malformed: {error}"),
                false,
                false,
            )
        })?;
    if request.x < 0 || request.y < 0 {
        return Err(DesktopSidecarErrorBody::new(
            "sidecar_schema_invalid",
            "Element-at-point coordinates must be non-negative.",
            false,
            false,
        ));
    }
    platform_element_at_point(request)
}

fn sidecar_accessibility_snapshot(
    payload: serde_json::Value,
) -> Result<DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarErrorBody> {
    let request = serde_json::from_value::<DesktopSidecarAccessibilitySnapshotRequest>(payload)
        .map_err(|error| {
            DesktopSidecarErrorBody::new(
                "sidecar_schema_invalid",
                format!("Accessibility snapshot request payload was malformed: {error}"),
                false,
                false,
            )
        })?;
    if request.limit.is_some_and(|limit| limit == 0 || limit > 500) {
        return Err(DesktopSidecarErrorBody::new(
            "sidecar_schema_invalid",
            "Accessibility snapshot limit must be between 1 and 500.",
            false,
            false,
        ));
    }
    if request.max_depth.is_some_and(|depth| depth > 8) {
        return Err(DesktopSidecarErrorBody::new(
            "sidecar_schema_invalid",
            "Accessibility snapshot maxDepth must be no greater than 8.",
            false,
            false,
        ));
    }
    platform_accessibility_snapshot(request)
}

#[cfg(target_os = "macos")]
fn platform_element_at_point(
    request: DesktopSidecarPointRequest,
) -> Result<DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody> {
    macos_accessibility::element_at_point(request)
}

#[cfg(target_os = "windows")]
fn platform_element_at_point(
    request: DesktopSidecarPointRequest,
) -> Result<DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody> {
    windows_ui_automation::element_at_point(request)
}

#[cfg(target_os = "macos")]
fn platform_accessibility_snapshot(
    request: DesktopSidecarAccessibilitySnapshotRequest,
) -> Result<DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarErrorBody> {
    macos_accessibility::snapshot(request)
}

#[cfg(target_os = "windows")]
fn platform_accessibility_snapshot(
    request: DesktopSidecarAccessibilitySnapshotRequest,
) -> Result<DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarErrorBody> {
    windows_ui_automation::snapshot(request)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_element_at_point(
    _request: DesktopSidecarPointRequest,
) -> Result<DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody> {
    Err(unimplemented_operation())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_accessibility_snapshot(
    _request: DesktopSidecarAccessibilitySnapshotRequest,
) -> Result<DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarErrorBody> {
    Err(unimplemented_operation())
}

fn sidecar_ocr_snapshot(
    payload: serde_json::Value,
) -> Result<DesktopSidecarOcrSnapshotPayload, DesktopSidecarErrorBody> {
    let request =
        serde_json::from_value::<DesktopSidecarOcrSnapshotRequest>(payload).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "sidecar_schema_invalid",
                format!("OCR snapshot request payload was malformed: {error}"),
                false,
                false,
            )
        })?;
    if request.limit.is_some_and(|limit| limit == 0 || limit > 500) {
        return Err(DesktopSidecarErrorBody::new(
            "sidecar_schema_invalid",
            "OCR snapshot limit must be between 1 and 500.",
            false,
            false,
        ));
    }
    platform_ocr_snapshot(request)
}

fn sidecar_clipboard_read_text(
) -> Result<DesktopSidecarClipboardTextPayload, DesktopSidecarErrorBody> {
    let text = platform_clipboard_read_text()?;
    let length = text.chars().count();
    Ok(DesktopSidecarClipboardTextPayload {
        available: true,
        text: Some(text),
        length,
    })
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClipboardReadImageRequest {
    #[serde(default)]
    include_data: bool,
    #[serde(default)]
    max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClipboardReadHtmlRequest {
    #[serde(default)]
    max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClipboardReadRtfRequest {
    #[serde(default)]
    max_bytes: Option<usize>,
}

fn sidecar_clipboard_read_html(
    payload: serde_json::Value,
) -> Result<DesktopSidecarClipboardHtmlPayload, DesktopSidecarErrorBody> {
    let request = serde_json::from_value::<ClipboardReadHtmlRequest>(payload).map_err(|error| {
        DesktopSidecarErrorBody::new(
            "sidecar_schema_invalid",
            format!("Desktop sidecar could not decode clipboard HTML request: {error}"),
            false,
            false,
        )
    })?;
    clipboard_resources::read_html(request)
}

fn sidecar_clipboard_read_rtf(
    payload: serde_json::Value,
) -> Result<DesktopSidecarClipboardRtfPayload, DesktopSidecarErrorBody> {
    let request = serde_json::from_value::<ClipboardReadRtfRequest>(payload).map_err(|error| {
        DesktopSidecarErrorBody::new(
            "sidecar_schema_invalid",
            format!("Desktop sidecar could not decode clipboard RTF request: {error}"),
            false,
            false,
        )
    })?;
    clipboard_resources::read_rtf(request)
}

fn sidecar_clipboard_read_image(
    payload: serde_json::Value,
) -> Result<DesktopSidecarClipboardImagePayload, DesktopSidecarErrorBody> {
    let request =
        serde_json::from_value::<ClipboardReadImageRequest>(payload).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "sidecar_schema_invalid",
                format!("Desktop sidecar could not decode clipboard image request: {error}"),
                false,
                false,
            )
        })?;
    clipboard_resources::read_image(request)
}

fn sidecar_clipboard_read_files(
) -> Result<DesktopSidecarClipboardFilesPayload, DesktopSidecarErrorBody> {
    clipboard_resources::read_files()
}

#[cfg(target_os = "macos")]
fn platform_clipboard_read_text() -> Result<String, DesktopSidecarErrorBody> {
    macos_clipboard::read_text()
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn platform_clipboard_read_text() -> Result<String, DesktopSidecarErrorBody> {
    cross_platform_input::read_clipboard_text()
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn platform_clipboard_read_text() -> Result<String, DesktopSidecarErrorBody> {
    Err(unimplemented_operation())
}

#[cfg(target_os = "macos")]
fn platform_ocr_snapshot(
    request: DesktopSidecarOcrSnapshotRequest,
) -> Result<DesktopSidecarOcrSnapshotPayload, DesktopSidecarErrorBody> {
    let capture_request = DesktopSidecarScreenshotRequest {
        display_id: request.display_id,
        region: request.region,
    };
    let capture = capture_desktop_image(&capture_request)?;
    let png_bytes = encode_png(
        &capture.image,
        "desktop_ocr_image_encode_failed",
        "Desktop sidecar could not encode OCR capture PNG",
    )?;
    macos_ocr::recognize_png(&capture, png_bytes, request.limit.unwrap_or(200))
}

#[cfg(target_os = "windows")]
fn platform_ocr_snapshot(
    request: DesktopSidecarOcrSnapshotRequest,
) -> Result<DesktopSidecarOcrSnapshotPayload, DesktopSidecarErrorBody> {
    let capture_request = DesktopSidecarScreenshotRequest {
        display_id: request.display_id,
        region: request.region,
    };
    let capture = capture_desktop_image(&capture_request)?;
    let png_bytes = encode_png(
        &capture.image,
        "desktop_ocr_image_encode_failed",
        "Desktop sidecar could not encode OCR capture PNG",
    )?;
    windows_ocr::recognize_png(&capture, png_bytes, request.limit.unwrap_or(200))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_ocr_snapshot(
    _request: DesktopSidecarOcrSnapshotRequest,
) -> Result<DesktopSidecarOcrSnapshotPayload, DesktopSidecarErrorBody> {
    Err(unimplemented_operation())
}

fn sidecar_screenshot(
    payload: serde_json::Value,
) -> Result<DesktopSidecarScreenshotPayload, DesktopSidecarErrorBody> {
    let request =
        serde_json::from_value::<DesktopSidecarScreenshotRequest>(payload).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "sidecar_schema_invalid",
                format!("Screenshot request payload was malformed: {error}"),
                false,
                false,
            )
        })?;
    let capture = capture_desktop_image(&request)?;
    let bytes = encode_png(
        &capture.image,
        "desktop_screenshot_encode_failed",
        "Desktop sidecar could not encode screenshot PNG",
    )?;
    let bytes_base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(DesktopSidecarScreenshotPayload {
        media_type: "image/png".into(),
        bytes_base64,
        width: capture.image.width(),
        height: capture.image.height(),
        scale_factor: capture.scale_factor,
        captured_at: capture.captured_at,
    })
}

fn sidecar_stream_capabilities() -> DesktopSidecarStreamCapabilitiesPayload {
    let native_available = native_webrtc_stream_available();
    DesktopSidecarStreamCapabilitiesPayload {
        webrtc_stream: native_available,
        screenshot_fallback_stream: true,
        native_video_track: native_available,
        preferred_codec: native_available.then(|| MIME_TYPE_H264.into()),
        capture_backends: native_capture_backends(),
        encoder_backends: native_encoder_backends(),
        hardware_encoding: native_hardware_encoding_available(),
        supported_qualities: vec![
            DesktopSidecarStreamQuality::Low,
            DesktopSidecarStreamQuality::Balanced,
            DesktopSidecarStreamQuality::High,
        ],
        max_width: WEBRTC_MAX_WIDTH,
        max_frame_rate: WEBRTC_MAX_FRAME_RATE,
        message: if native_available {
            native_webrtc_stream_message()
        } else {
            "Native WebRTC desktop publishing is not implemented for this platform yet; screenshot fallback stream state is available and input commands use an independent sidecar path.".into()
        },
    }
}

fn native_webrtc_stream_message() -> String {
    if cfg!(target_os = "windows") {
        "Native WebRTC desktop streaming publishes an H.264 video track from Windows DXGI desktop capture with OpenH264 software encoding and best-effort cursor overlay; screenshot fallback is available only for degraded mode.".into()
    } else {
        "Native WebRTC desktop streaming publishes an H.264 video track with screenshot fallback available only for degraded mode.".into()
    }
}

fn sidecar_stream(
    operation: DesktopSidecarOperation,
    payload: serde_json::Value,
) -> Result<serde_json::Value, DesktopSidecarErrorBody> {
    let request =
        serde_json::from_value::<DesktopSidecarStreamRequest>(payload).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "sidecar_schema_invalid",
                format!("Stream request payload was malformed: {error}"),
                false,
                false,
            )
        })?;
    validate_stream_request(operation, &request)?;
    let response = match operation {
        DesktopSidecarOperation::StreamStart => start_webrtc_stream(request)?,
        DesktopSidecarOperation::StreamAnswer => apply_webrtc_stream_answer(request)?,
        DesktopSidecarOperation::StreamIceCandidate => add_webrtc_stream_ice_candidate(request)?,
        DesktopSidecarOperation::StreamStop => stop_webrtc_stream(request)?,
        DesktopSidecarOperation::StreamStatus => webrtc_stream_status(request)?,
        DesktopSidecarOperation::StreamSetQuality => update_webrtc_stream_quality(request)?,
        DesktopSidecarOperation::StreamRequestKeyframe => request_webrtc_stream_keyframe(request)?,
        DesktopSidecarOperation::StreamOffer => return Err(DesktopSidecarErrorBody::new(
            "stream_offer_not_supported",
            "This sidecar publishes desktop streams and expects the browser to answer its offer.",
            false,
            false,
        )),
        _ => return Err(unimplemented_operation()),
    };
    serde_json::to_value(response).map_err(|error| {
        DesktopSidecarErrorBody::new(
            "sidecar_response_encode_failed",
            format!("Sidecar could not encode stream response: {error}"),
            false,
            false,
        )
    })
}

fn start_webrtc_stream(
    request: DesktopSidecarStreamRequest,
) -> Result<DesktopSidecarStreamPayload, DesktopSidecarErrorBody> {
    if !native_webrtc_stream_available() {
        return Err(native_webrtc_unavailable_error());
    }
    let stream_id = required_stream_id(&request)?;
    stop_webrtc_stream_by_id(&stream_id)?;
    let config = Arc::new(Mutex::new(webrtc_stream_config(&request, &stream_id)));
    let stop = Arc::new(AtomicBool::new(false));
    let media_started = Arc::new(AtomicBool::new(false));
    let keyframe_requested = Arc::new(AtomicBool::new(true));
    let metrics = Arc::new(StreamTelemetry::default());
    let runtime = webrtc_runtime()?;
    let (peer_connection, video_track, rtp_sender, session_description) =
        runtime.block_on(create_webrtc_offer(&request))?;

    active_webrtc_streams()
        .lock()
        .map_err(|_| stream_state_error())?
        .insert(
            stream_id.clone(),
            ActiveWebRtcStream {
                peer_connection,
                video_track,
                rtp_sender,
                stop,
                media_started,
                keyframe_requested,
                config: Arc::clone(&config),
                metrics: Arc::clone(&metrics),
            },
        );

    let config_snapshot = config.lock().map_err(|_| stream_state_error())?.clone();
    Ok(webrtc_stream_payload(
        &config_snapshot,
        DesktopSidecarStreamStatus::Starting,
        "Native WebRTC desktop stream offer is ready.",
        Some(session_description),
        None,
    ))
}

async fn create_webrtc_offer(
    request: &DesktopSidecarStreamRequest,
) -> Result<
    (
        Arc<RTCPeerConnection>,
        Arc<TrackLocalStaticSample>,
        Arc<RTCRtpSender>,
        DesktopSidecarSessionDescription,
    ),
    DesktopSidecarErrorBody,
> {
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs().map_err(|error| {
        stream_webrtc_error(
            "stream_webrtc_failed",
            "could not register WebRTC codecs",
            error,
        )
    })?;
    let api = APIBuilder::new().with_media_engine(media_engine).build();
    let peer_connection = Arc::new(
        api.new_peer_connection(RTCConfiguration {
            ice_servers: webrtc_ice_servers(&request.ice_servers),
            ..Default::default()
        })
        .await
        .map_err(|error| {
            stream_webrtc_error(
                "stream_webrtc_failed",
                "could not create peer connection",
                error,
            )
        })?,
    );
    let video_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.to_owned(),
            clock_rate: 90_000,
            sdp_fmtp_line: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f"
                .into(),
            ..Default::default()
        },
        "desktop-video".into(),
        "xero-desktop".into(),
    ));
    let rtp_sender = peer_connection
        .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await
        .map_err(|error| {
            stream_webrtc_error(
                "stream_webrtc_failed",
                "could not add WebRTC video track",
                error,
            )
        })?;

    let offer = peer_connection.create_offer(None).await.map_err(|error| {
        stream_webrtc_error(
            "stream_webrtc_failed",
            "could not create WebRTC offer",
            error,
        )
    })?;
    let mut gather_complete = peer_connection.gathering_complete_promise().await;
    peer_connection
        .set_local_description(offer)
        .await
        .map_err(|error| {
            stream_webrtc_error(
                "stream_webrtc_failed",
                "could not set local WebRTC description",
                error,
            )
        })?;
    let _ = tokio::time::timeout(WEBRTC_ICE_GATHER_TIMEOUT, gather_complete.recv()).await;
    let description = peer_connection.local_description().await.ok_or_else(|| {
        DesktopSidecarErrorBody::new(
            "stream_signaling_failed",
            "WebRTC offer was not available after ICE gathering.",
            true,
            false,
        )
    })?;

    Ok((
        peer_connection,
        video_track,
        rtp_sender,
        DesktopSidecarSessionDescription {
            sdp_type: "offer".into(),
            sdp: description.sdp,
        },
    ))
}

fn apply_webrtc_stream_answer(
    request: DesktopSidecarStreamRequest,
) -> Result<DesktopSidecarStreamPayload, DesktopSidecarErrorBody> {
    let stream_id = required_stream_id(&request)?;
    let description = request
        .session_description
        .as_ref()
        .ok_or_else(|| schema_error("sessionDescription"))?;
    let active = active_webrtc_stream(&stream_id)?;
    let rtc_description = match description.sdp_type.as_str() {
        "answer" => RTCSessionDescription::answer(description.sdp.clone()),
        "pranswer" => RTCSessionDescription::pranswer(description.sdp.clone()),
        _ => return Err(schema_error("sessionDescription")),
    }
    .map_err(|error| {
        stream_webrtc_error(
            "stream_signaling_failed",
            "browser WebRTC answer was invalid",
            error,
        )
    })?;
    webrtc_runtime()?.block_on(async {
        active
            .peer_connection
            .set_remote_description(rtc_description)
            .await
            .map_err(|error| {
                stream_webrtc_error(
                    "stream_signaling_failed",
                    "could not apply browser WebRTC answer",
                    error,
                )
            })
    })?;
    start_webrtc_media_publisher(&active)?;
    active_webrtc_stream_payload(&stream_id, "Browser WebRTC answer was applied.")
}

fn add_webrtc_stream_ice_candidate(
    request: DesktopSidecarStreamRequest,
) -> Result<DesktopSidecarStreamPayload, DesktopSidecarErrorBody> {
    let stream_id = required_stream_id(&request)?;
    let candidate = request
        .ice_candidate
        .as_ref()
        .ok_or_else(|| schema_error("iceCandidate"))?;
    let active = active_webrtc_stream(&stream_id)?;
    let rtc_candidate = RTCIceCandidateInit {
        candidate: candidate.candidate.clone(),
        sdp_mid: candidate.sdp_mid.clone(),
        sdp_mline_index: candidate.sdp_m_line_index,
        username_fragment: candidate.username_fragment.clone(),
    };
    webrtc_runtime()?.block_on(async {
        active
            .peer_connection
            .add_ice_candidate(rtc_candidate)
            .await
            .map_err(|error| {
                stream_webrtc_error(
                    "stream_signaling_failed",
                    "could not add browser ICE candidate",
                    error,
                )
            })
    })?;
    active_webrtc_stream_payload(&stream_id, "Browser ICE candidate was applied.")
}

fn stop_webrtc_stream(
    request: DesktopSidecarStreamRequest,
) -> Result<DesktopSidecarStreamPayload, DesktopSidecarErrorBody> {
    let stream_id = required_stream_id(&request)?;
    let config = stop_webrtc_stream_by_id(&stream_id)?;
    Ok(webrtc_stream_payload(
        &config,
        DesktopSidecarStreamStatus::Stopped,
        "Native WebRTC desktop stream stopped.",
        None,
        None,
    ))
}

fn webrtc_stream_status(
    request: DesktopSidecarStreamRequest,
) -> Result<DesktopSidecarStreamPayload, DesktopSidecarErrorBody> {
    let stream_id = required_stream_id(&request)?;
    active_webrtc_stream_payload(&stream_id, "Returned native WebRTC stream status.")
}

fn update_webrtc_stream_quality(
    request: DesktopSidecarStreamRequest,
) -> Result<DesktopSidecarStreamPayload, DesktopSidecarErrorBody> {
    let stream_id = required_stream_id(&request)?;
    let active = active_webrtc_stream(&stream_id)?;
    {
        let mut config = active.config.lock().map_err(|_| stream_state_error())?;
        apply_webrtc_stream_request_to_config(&mut config, &request);
    }
    active_webrtc_stream_payload(&stream_id, "Updated native WebRTC stream quality.")
}

fn request_webrtc_stream_keyframe(
    request: DesktopSidecarStreamRequest,
) -> Result<DesktopSidecarStreamPayload, DesktopSidecarErrorBody> {
    let stream_id = required_stream_id(&request)?;
    let active = active_webrtc_stream(&stream_id)?;
    active.keyframe_requested.store(true, Ordering::SeqCst);
    active_webrtc_stream_payload(&stream_id, "Requested native WebRTC stream keyframe.")
}

fn webrtc_runtime() -> Result<&'static tokio::runtime::Runtime, DesktopSidecarErrorBody> {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("xero-desktop-webrtc")
        .build()
        .map_err(|error| {
            DesktopSidecarErrorBody::new(
                "stream_webrtc_failed",
                format!("Could not start WebRTC runtime: {error}"),
                true,
                false,
            )
        })?;
    let _ = RUNTIME.set(runtime);
    RUNTIME.get().ok_or_else(|| {
        DesktopSidecarErrorBody::new(
            "stream_webrtc_failed",
            "Could not initialize WebRTC runtime.",
            true,
            false,
        )
    })
}

fn active_webrtc_streams() -> &'static Mutex<BTreeMap<String, ActiveWebRtcStream>> {
    static STREAMS: OnceLock<Mutex<BTreeMap<String, ActiveWebRtcStream>>> = OnceLock::new();
    STREAMS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn active_webrtc_stream(stream_id: &str) -> Result<ActiveWebRtcStream, DesktopSidecarErrorBody> {
    active_webrtc_streams()
        .lock()
        .map_err(|_| stream_state_error())?
        .get(stream_id)
        .cloned()
        .ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "stream_not_found",
                format!("Desktop sidecar does not have an active WebRTC stream `{stream_id}`."),
                true,
                false,
            )
        })
}

fn stop_webrtc_stream_by_id(
    stream_id: &str,
) -> Result<WebRtcStreamConfig, DesktopSidecarErrorBody> {
    let active = active_webrtc_streams()
        .lock()
        .map_err(|_| stream_state_error())?
        .remove(stream_id);
    let Some(active) = active else {
        return Ok(WebRtcStreamConfig {
            stream_id: stream_id.into(),
            display_id: None,
            max_width: 1280,
            max_frame_rate: 24,
            include_cursor: true,
            quality: DesktopSidecarStreamQuality::Balanced,
        });
    };
    active.stop.store(true, Ordering::SeqCst);
    active.media_started.store(false, Ordering::SeqCst);
    let config = active
        .config
        .lock()
        .map_err(|_| stream_state_error())?
        .clone();
    webrtc_runtime()?.block_on(async {
        active.peer_connection.close().await.map_err(|error| {
            stream_webrtc_error(
                "stream_webrtc_failed",
                "could not close WebRTC stream",
                error,
            )
        })
    })?;
    Ok(config)
}

fn active_webrtc_stream_payload(
    stream_id: &str,
    message: &'static str,
) -> Result<DesktopSidecarStreamPayload, DesktopSidecarErrorBody> {
    let active = active_webrtc_stream(stream_id)?;
    let config = active
        .config
        .lock()
        .map_err(|_| stream_state_error())?
        .clone();
    refresh_webrtc_transport_metrics(&active);
    let metrics = stream_metrics_payload(&active.metrics);
    let status = if metrics.fallback_reason.is_some() {
        DesktopSidecarStreamStatus::Failed
    } else if active.media_started.load(Ordering::SeqCst) {
        DesktopSidecarStreamStatus::Live
    } else {
        DesktopSidecarStreamStatus::Starting
    };
    Ok(webrtc_stream_payload_with_metrics(
        &config,
        status,
        message,
        None,
        None,
        Some(metrics),
    ))
}

fn webrtc_stream_payload(
    config: &WebRtcStreamConfig,
    status: DesktopSidecarStreamStatus,
    message: impl Into<String>,
    session_description: Option<DesktopSidecarSessionDescription>,
    ice_candidate: Option<xero_desktop_control_ipc::DesktopSidecarIceCandidate>,
) -> DesktopSidecarStreamPayload {
    webrtc_stream_payload_with_metrics(
        config,
        status,
        message,
        session_description,
        ice_candidate,
        None,
    )
}

fn webrtc_stream_payload_with_metrics(
    config: &WebRtcStreamConfig,
    status: DesktopSidecarStreamStatus,
    message: impl Into<String>,
    session_description: Option<DesktopSidecarSessionDescription>,
    ice_candidate: Option<xero_desktop_control_ipc::DesktopSidecarIceCandidate>,
    metrics: Option<DesktopSidecarStreamMetrics>,
) -> DesktopSidecarStreamPayload {
    DesktopSidecarStreamPayload {
        stream_id: Some(config.stream_id.clone()),
        display_id: config.display_id.clone(),
        status,
        transport: DesktopSidecarStreamTransport::WebRtc,
        signaling_channel: Some("computer_use_stream".into()),
        quality: config.quality,
        max_width: config.max_width,
        max_frame_rate: config.max_frame_rate,
        include_cursor: config.include_cursor,
        session_description,
        ice_candidate,
        metrics,
        message: message.into(),
    }
}

fn webrtc_stream_config(
    request: &DesktopSidecarStreamRequest,
    stream_id: &str,
) -> WebRtcStreamConfig {
    let quality = request
        .quality
        .unwrap_or(DesktopSidecarStreamQuality::Balanced);
    let (default_width, default_frame_rate) = sidecar_stream_quality_profile(quality);
    WebRtcStreamConfig {
        stream_id: stream_id.into(),
        display_id: request.display_id.clone(),
        max_width: request
            .max_width
            .unwrap_or(default_width)
            .clamp(640, WEBRTC_MAX_WIDTH),
        max_frame_rate: request
            .max_frame_rate
            .unwrap_or(default_frame_rate)
            .clamp(1, WEBRTC_MAX_FRAME_RATE),
        include_cursor: request.include_cursor.unwrap_or(true),
        quality,
    }
}

fn apply_webrtc_stream_request_to_config(
    config: &mut WebRtcStreamConfig,
    request: &DesktopSidecarStreamRequest,
) {
    if let Some(quality) = request.quality {
        let (default_width, default_frame_rate) = sidecar_stream_quality_profile(quality);
        config.quality = quality;
        config.max_width = request
            .max_width
            .unwrap_or(default_width)
            .clamp(640, WEBRTC_MAX_WIDTH);
        config.max_frame_rate = request
            .max_frame_rate
            .unwrap_or(default_frame_rate)
            .clamp(1, WEBRTC_MAX_FRAME_RATE);
    }
    if let Some(max_width) = request.max_width {
        config.max_width = max_width.clamp(640, WEBRTC_MAX_WIDTH);
    }
    if let Some(max_frame_rate) = request.max_frame_rate {
        config.max_frame_rate = max_frame_rate.clamp(1, WEBRTC_MAX_FRAME_RATE);
    }
    if let Some(include_cursor) = request.include_cursor {
        config.include_cursor = include_cursor;
    }
}

fn sidecar_stream_quality_profile(quality: DesktopSidecarStreamQuality) -> (u32, u32) {
    match quality {
        DesktopSidecarStreamQuality::Low => (960, 15),
        DesktopSidecarStreamQuality::Balanced => (1280, 24),
        DesktopSidecarStreamQuality::High => (WEBRTC_MAX_WIDTH, WEBRTC_MAX_FRAME_RATE),
    }
}

fn webrtc_ice_servers(
    servers: &[xero_desktop_control_ipc::DesktopSidecarIceServer],
) -> Vec<RTCIceServer> {
    servers
        .iter()
        .map(|server| RTCIceServer {
            urls: match &server.urls {
                xero_desktop_control_ipc::DesktopSidecarIceServerUrls::One(url) => {
                    vec![url.clone()]
                }
                xero_desktop_control_ipc::DesktopSidecarIceServerUrls::Many(urls) => urls.clone(),
            },
            username: server.username.clone().unwrap_or_default(),
            credential: server.credential.clone().unwrap_or_default(),
        })
        .collect()
}

fn start_webrtc_media_publisher(
    active: &ActiveWebRtcStream,
) -> Result<(), DesktopSidecarErrorBody> {
    if active.media_started.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let _ = active.metrics.started_at.set(Instant::now());
    let track = Arc::clone(&active.video_track);
    let rtp_sender = Arc::clone(&active.rtp_sender);
    let peer_connection = Arc::clone(&active.peer_connection);
    let config = Arc::clone(&active.config);
    let stop = Arc::clone(&active.stop);
    let keyframe_requested = Arc::clone(&active.keyframe_requested);
    let metrics = Arc::clone(&active.metrics);
    let media_started = Arc::clone(&active.media_started);
    let rtcp_stop = Arc::clone(&stop);
    let rtcp_keyframe_requested = Arc::clone(&keyframe_requested);
    webrtc_runtime()?.spawn(async move {
        drain_webrtc_rtcp(rtp_sender, rtcp_stop, rtcp_keyframe_requested).await;
    });
    webrtc_runtime()?.spawn(async move {
        if let Err(error) = run_webrtc_media_loop(
            track,
            peer_connection,
            config,
            stop,
            keyframe_requested,
            Arc::clone(&metrics),
        )
        .await
        {
            set_stream_metric_string(&metrics.fallback_reason, Some(error.message));
        }
        media_started.store(false, Ordering::SeqCst);
    });
    Ok(())
}

async fn drain_webrtc_rtcp(
    rtp_sender: Arc<RTCRtpSender>,
    stop: Arc<AtomicBool>,
    keyframe_requested: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::SeqCst) {
        match tokio::time::timeout(Duration::from_secs(1), rtp_sender.read_rtcp()).await {
            Ok(Ok((packets, _attributes))) => {
                if packets
                    .iter()
                    .any(|packet| rtcp_packet_requests_keyframe(packet.as_ref()))
                {
                    keyframe_requested.store(true, Ordering::SeqCst);
                }
            }
            Ok(Err(_)) => break,
            Err(_) => {}
        }
    }
}

fn rtcp_packet_requests_keyframe(
    packet: &(dyn webrtc::rtcp::packet::Packet + Send + Sync),
) -> bool {
    packet.as_any().is::<PictureLossIndication>() || packet.as_any().is::<FullIntraRequest>()
}

async fn run_webrtc_media_loop(
    track: Arc<TrackLocalStaticSample>,
    peer_connection: Arc<RTCPeerConnection>,
    config: Arc<Mutex<WebRtcStreamConfig>>,
    stop: Arc<AtomicBool>,
    keyframe_requested: Arc<AtomicBool>,
    metrics: Arc<StreamTelemetry>,
) -> Result<(), DesktopSidecarErrorBody> {
    #[cfg(target_os = "macos")]
    {
        run_macos_webrtc_media_loop(
            track,
            peer_connection,
            config,
            stop,
            keyframe_requested,
            metrics,
        )
        .await
    }
    #[cfg(target_os = "windows")]
    {
        run_windows_webrtc_media_loop(
            track,
            peer_connection,
            config,
            stop,
            keyframe_requested,
            metrics,
        )
        .await
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (track, peer_connection, config, stop, keyframe_requested);
        set_stream_metric_string(
            &metrics.capture_backend,
            Some("native_publisher_unavailable".into()),
        );
        Err(native_webrtc_unavailable_error())
    }
}

fn stream_metrics_payload(metrics: &StreamTelemetry) -> DesktopSidecarStreamMetrics {
    let started_at = metrics.started_at.get().copied();
    DesktopSidecarStreamMetrics {
        capture_backend: stream_metric_string(&metrics.capture_backend),
        encoder_backend: stream_metric_string(&metrics.encoder_backend),
        encoder_hardware: Some(metrics.encoder_hardware.load(Ordering::Relaxed)),
        preferred_codec: Some(MIME_TYPE_H264.into()),
        fallback_reason: stream_metric_string(&metrics.fallback_reason),
        capture_frame_rate: stream_metric_rate(
            metrics.capture_frames.load(Ordering::Relaxed),
            started_at,
        ),
        capture_dropped_frames: metrics.capture_dropped_frames.load(Ordering::Relaxed),
        encode_frame_rate: stream_metric_rate(
            metrics.encode_frames.load(Ordering::Relaxed),
            started_at,
        ),
        encode_latency_ms: stream_metric_average(
            metrics.encode_latency_total_ms.load(Ordering::Relaxed),
            metrics.encode_frames.load(Ordering::Relaxed),
        ),
        outbound_bitrate_bps: stream_metric_bitrate(
            metrics.bytes_sent.load(Ordering::Relaxed),
            started_at,
        ),
        available_outgoing_bitrate_bps: stream_metric_nonzero_u64(
            metrics
                .available_outgoing_bitrate_bps
                .load(Ordering::Relaxed),
        ),
        packets_sent: Some(metrics.packets_sent.load(Ordering::Relaxed)),
        bytes_sent: Some(metrics.bytes_sent.load(Ordering::Relaxed)),
        packet_loss: Some(metrics.packets_lost.load(Ordering::Relaxed)),
        round_trip_time_ms: Some(metrics.round_trip_time_ms.load(Ordering::Relaxed) as u32),
        retransmits: Some(metrics.retransmits.load(Ordering::Relaxed)),
        keyframes: metrics.keyframes.load(Ordering::Relaxed),
    }
}

fn stream_metric_string(value: &Mutex<Option<String>>) -> Option<String> {
    value.lock().ok().and_then(|value| value.clone())
}

fn set_stream_metric_string(value: &Mutex<Option<String>>, next: Option<String>) {
    if let Ok(mut value) = value.lock() {
        *value = next;
    }
}

fn stream_metric_rate(total: u64, started_at: Option<Instant>) -> Option<u32> {
    let started_at = started_at?;
    let elapsed = started_at.elapsed().as_secs_f64();
    if elapsed <= 0.0 {
        return None;
    }
    Some((total as f64 / elapsed).round().clamp(0.0, u32::MAX as f64) as u32)
}

fn stream_metric_bitrate(total_bytes: u64, started_at: Option<Instant>) -> Option<u64> {
    let started_at = started_at?;
    let elapsed = started_at.elapsed().as_secs_f64();
    if elapsed <= 0.0 {
        return None;
    }
    Some(((total_bytes as f64 * 8.0) / elapsed).round().max(0.0) as u64)
}

fn stream_metric_average(total: u64, count: u64) -> Option<u32> {
    if count == 0 {
        return None;
    }
    Some((total / count).min(u32::MAX as u64) as u32)
}

fn stream_metric_nonzero_u64(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}

fn refresh_webrtc_transport_metrics(active: &ActiveWebRtcStream) {
    let peer_connection = Arc::clone(&active.peer_connection);
    let metrics = Arc::clone(&active.metrics);
    let Ok(runtime) = webrtc_runtime() else {
        return;
    };
    runtime.block_on(async move {
        let stats = peer_connection.get_stats().await;
        for report in stats.reports.values() {
            match report {
                StatsReportType::OutboundRTP(outbound) if outbound.kind == "video" => {
                    metrics
                        .packets_sent
                        .store(outbound.packets_sent, Ordering::Relaxed);
                    metrics
                        .bytes_sent
                        .store(outbound.bytes_sent, Ordering::Relaxed);
                }
                StatsReportType::RemoteInboundRTP(remote) if remote.kind == "video" => {
                    metrics
                        .packets_lost
                        .store(remote.packets_lost, Ordering::Relaxed);
                    if let Some(rtt) = remote.round_trip_time {
                        metrics
                            .round_trip_time_ms
                            .store((rtt * 1_000.0).round().max(0.0) as u64, Ordering::Relaxed);
                    }
                }
                StatsReportType::CandidatePair(candidate_pair) => {
                    metrics
                        .retransmits
                        .store(candidate_pair.retransmissions_sent, Ordering::Relaxed);
                    if candidate_pair.available_outgoing_bitrate.is_finite()
                        && candidate_pair.available_outgoing_bitrate > 0.0
                    {
                        metrics.available_outgoing_bitrate_bps.store(
                            candidate_pair.available_outgoing_bitrate.round() as u64,
                            Ordering::Relaxed,
                        );
                    }
                }
                _ => {}
            }
        }
    });
}

fn native_capture_backends() -> Vec<String> {
    if cfg!(target_os = "macos") {
        vec!["screencapturekit".into()]
    } else if cfg!(target_os = "windows") {
        vec!["dxgi_output_duplication".into()]
    } else {
        Vec::new()
    }
}

fn native_encoder_backends() -> Vec<String> {
    if cfg!(target_os = "macos") {
        vec!["videotoolbox_h264".into()]
    } else if cfg!(target_os = "windows") {
        vec!["openh264_software".into()]
    } else {
        Vec::new()
    }
}

fn native_hardware_encoding_available() -> bool {
    cfg!(target_os = "macos")
}

fn native_webrtc_stream_available() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows"))
}

fn native_webrtc_unavailable_error() -> DesktopSidecarErrorBody {
    DesktopSidecarErrorBody::new(
        "stream_native_publisher_unavailable",
        "Native WebRTC media-track desktop publishing is not implemented on this platform yet.",
        true,
        false,
    )
}

fn stream_capture_bitrate(quality: DesktopSidecarStreamQuality) -> i32 {
    match quality {
        DesktopSidecarStreamQuality::Low => 1_200_000,
        DesktopSidecarStreamQuality::Balanced => 3_500_000,
        DesktopSidecarStreamQuality::High => 8_000_000,
    }
}

fn stream_target_bitrate(quality: DesktopSidecarStreamQuality, metrics: &StreamTelemetry) -> i32 {
    let base = stream_capture_bitrate(quality);
    let packets_lost = metrics.packets_lost.load(Ordering::Relaxed).max(0);
    let rtt_ms = metrics.round_trip_time_ms.load(Ordering::Relaxed);
    let congestion_scale = if rtt_ms >= 500 || packets_lost >= 50 {
        0.5
    } else if rtt_ms >= 250 || packets_lost > 0 {
        0.75
    } else {
        1.0
    };
    let available_ceiling = match metrics
        .available_outgoing_bitrate_bps
        .load(Ordering::Relaxed)
    {
        0 => base,
        available => ((available as f64) * 0.85)
            .round()
            .clamp(300_000.0, base as f64) as i32,
    };
    ((base as f64) * congestion_scale)
        .round()
        .clamp(300_000.0, available_ceiling as f64) as i32
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeVideoTarget {
    display_id: Option<String>,
    max_width: u32,
    fps: u32,
    include_cursor: bool,
    quality: DesktopSidecarStreamQuality,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl NativeVideoTarget {
    fn from_config(config: &WebRtcStreamConfig) -> Self {
        Self {
            display_id: config.display_id.clone(),
            max_width: config.max_width,
            fps: config.max_frame_rate.clamp(1, WEBRTC_MAX_FRAME_RATE),
            include_cursor: config.include_cursor,
            quality: config.quality,
        }
    }
}

#[cfg(target_os = "macos")]
struct MacosScreenFrame {
    surface: apple_cf::iosurface::IOSurface,
}

#[cfg(target_os = "macos")]
struct MacosCaptureLease {
    stream: screencapturekit::stream::SCStream,
    receiver: LatestFrameReceiver<MacosScreenFrame>,
    target: NativeVideoTarget,
    width: u32,
    height: u32,
}

#[cfg(target_os = "macos")]
impl MacosCaptureLease {
    fn start(
        target: NativeVideoTarget,
        metrics: Arc<StreamTelemetry>,
    ) -> Result<Self, DesktopSidecarErrorBody> {
        use screencapturekit::{
            cm::{CMSampleBufferExt, CMSampleBufferSCExt},
            prelude::*,
            stream::configuration::PixelFormat,
        };

        let content = SCShareableContent::get().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "permission_screen_recording_denied",
                format!("ScreenCaptureKit could not enumerate shareable displays: {error}"),
                true,
                true,
            )
        })?;
        let displays = content.displays();
        let display = select_screencapturekit_display(&displays, target.display_id.as_deref())?;
        let (width, height) =
            scaled_even_dimensions(display.width(), display.height(), target.max_width);
        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build();
        let config = SCStreamConfiguration::new()
            .with_width(width)
            .with_height(height)
            .with_pixel_format(PixelFormat::BGRA)
            .with_queue_depth(3)
            .with_fps(target.fps)
            .with_shows_cursor(target.include_cursor)
            .with_scales_to_fit(true);
        let (sender, receiver) = latest_frame_channel();
        let metrics_for_handler = Arc::clone(&metrics);
        let mut stream = SCStream::new(&filter, &config);
        stream.add_output_handler(
            move |sample: screencapturekit::cm::CMSampleBuffer,
                  output_type: screencapturekit::stream::output_type::SCStreamOutputType| {
                if output_type != SCStreamOutputType::Screen {
                    return;
                }
                if sample
                    .frame_status()
                    .is_some_and(|status| !status.has_content())
                {
                    metrics_for_handler
                        .capture_dropped_frames
                        .fetch_add(1, Ordering::Relaxed);
                    return;
                }
                let Some(image_buffer) = sample.image_buffer() else {
                    metrics_for_handler
                        .capture_dropped_frames
                        .fetch_add(1, Ordering::Relaxed);
                    return;
                };
                let Some(surface) = image_buffer.io_surface() else {
                    metrics_for_handler
                        .capture_dropped_frames
                        .fetch_add(1, Ordering::Relaxed);
                    return;
                };
                metrics_for_handler
                    .capture_frames
                    .fetch_add(1, Ordering::Relaxed);
                match sender.send_replace(MacosScreenFrame { surface }) {
                    LatestFrameSendResult::Stored => {}
                    LatestFrameSendResult::Replaced | LatestFrameSendResult::Closed => {
                        metrics_for_handler
                            .capture_dropped_frames
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
            },
            SCStreamOutputType::Screen,
        );
        stream.start_capture().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "stream_capture_failed",
                format!("ScreenCaptureKit capture could not start: {error}"),
                true,
                true,
            )
        })?;
        set_stream_metric_string(&metrics.capture_backend, Some("screencapturekit".into()));
        Ok(Self {
            stream,
            receiver,
            target,
            width,
            height,
        })
    }
}

#[cfg(target_os = "macos")]
impl Drop for MacosCaptureLease {
    fn drop(&mut self) {
        let _ = self.stream.stop_capture();
    }
}

#[cfg(target_os = "macos")]
fn select_screencapturekit_display(
    displays: &[screencapturekit::shareable_content::SCDisplay],
    display_id: Option<&str>,
) -> Result<screencapturekit::shareable_content::SCDisplay, DesktopSidecarErrorBody> {
    if let Some(display_id) = display_id {
        if let Some(display) = displays
            .iter()
            .find(|display| display.display_id().to_string() == display_id)
        {
            return Ok(display.clone());
        }
    }
    displays.first().cloned().ok_or_else(|| {
        DesktopSidecarErrorBody::new(
            "stream_capture_unavailable",
            "ScreenCaptureKit did not report any displays to capture.",
            true,
            true,
        )
    })
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn scaled_even_dimensions(source_width: u32, source_height: u32, max_width: u32) -> (u32, u32) {
    let width = source_width.min(max_width).max(2);
    let height = ((source_height as u64 * width as u64) / source_width.max(1) as u64)
        .max(2)
        .min(u32::MAX as u64) as u32;
    (make_even(width), make_even(height))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn make_even(value: u32) -> u32 {
    if value.is_multiple_of(2) {
        value
    } else {
        value.saturating_sub(1).max(2)
    }
}

#[cfg(target_os = "macos")]
struct MacosVideoToolboxEncoder {
    session: videotoolbox::CompressionSession,
    width: u32,
    height: u32,
    fps: u32,
    parameter_sets: Vec<Vec<u8>>,
    nal_length_size: usize,
}

#[cfg(target_os = "macos")]
impl MacosVideoToolboxEncoder {
    fn new(
        width: u32,
        height: u32,
        fps: u32,
        bitrate: i32,
    ) -> Result<Self, DesktopSidecarErrorBody> {
        let session = videotoolbox::CompressionSession::builder(
            width as i32,
            height as i32,
            videotoolbox::Codec::H264,
        )
        .with_real_time(true)
        .with_allow_frame_reordering(false)
        .with_average_bit_rate(bitrate)
        .with_expected_frame_rate(fps as f64)
        .with_max_keyframe_interval(fps as i32)
        .with_profile_level(videotoolbox::ProfileLevel::H264ConstrainedBaselineAutoLevel)
        .build()
        .map_err(|error| {
            DesktopSidecarErrorBody::new(
                "stream_encoder_failed",
                format!("VideoToolbox H.264 encoder could not start: {error}"),
                true,
                false,
            )
        })?;
        Ok(Self {
            session,
            width,
            height,
            fps,
            parameter_sets: Vec::new(),
            nal_length_size: 4,
        })
    }

    fn matches_stream_shape(&self, width: u32, height: u32, fps: u32) -> bool {
        self.width == width && self.height == height && self.fps == fps
    }

    fn encode(
        &mut self,
        frame: MacosScreenFrame,
        frame_index: i64,
        force_keyframe: bool,
    ) -> Result<EncodedVideoSample, DesktopSidecarErrorBody> {
        let started_at = Instant::now();
        let encoded = self
            .session
            .encode(&frame.surface, (frame_index, self.fps as i32))
            .map_err(|error| {
                DesktopSidecarErrorBody::new(
                    "stream_frame_encode_failed",
                    format!("VideoToolbox could not encode a desktop frame: {error}"),
                    true,
                    false,
                )
            })?;
        if encoded.data.is_empty() {
            return Err(DesktopSidecarErrorBody::new(
                "stream_frame_dropped",
                "VideoToolbox dropped a desktop frame before emitting H.264 bytes.",
                true,
                false,
            ));
        }
        if let Some(sample_buffer) = encoded.cm_sample_buffer() {
            if let Ok((parameter_sets, nal_length_size)) =
                h264_parameter_sets_from_sample_buffer(sample_buffer)
            {
                if !parameter_sets.is_empty() {
                    self.parameter_sets = parameter_sets;
                    self.nal_length_size = nal_length_size;
                }
            }
        }
        let contains_idr = h264_sample_contains_idr(&encoded.data, self.nal_length_size)?;
        let include_parameter_sets =
            h264_should_include_parameter_sets(frame_index, force_keyframe, contains_idr);
        let (bytes, contains_idr) = h264_sample_to_annex_b(
            &encoded.data,
            self.nal_length_size,
            if include_parameter_sets {
                &self.parameter_sets
            } else {
                &[]
            },
        )?;
        Ok(EncodedVideoSample {
            bytes,
            duration: Duration::from_micros(1_000_000 / self.fps.max(1) as u64),
            encode_latency_ms: started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            keyframe: contains_idr,
        })
    }
}

#[cfg(target_os = "macos")]
async fn run_macos_webrtc_media_loop(
    track: Arc<TrackLocalStaticSample>,
    peer_connection: Arc<RTCPeerConnection>,
    config: Arc<Mutex<WebRtcStreamConfig>>,
    stop: Arc<AtomicBool>,
    keyframe_requested: Arc<AtomicBool>,
    metrics: Arc<StreamTelemetry>,
) -> Result<(), DesktopSidecarErrorBody> {
    set_stream_metric_string(&metrics.encoder_backend, Some("videotoolbox_h264".into()));
    metrics.encoder_hardware.store(true, Ordering::Relaxed);
    let mut capture: Option<MacosCaptureLease> = None;
    let mut encoder: Option<MacosVideoToolboxEncoder> = None;
    let mut frame_index = 0_i64;
    while !stop.load(Ordering::SeqCst) {
        let frame_config = config.lock().map_err(|_| stream_state_error())?.clone();
        let target = NativeVideoTarget::from_config(&frame_config);
        if capture
            .as_ref()
            .is_none_or(|capture| capture.target != target)
        {
            drop(capture.take());
            encoder = None;
            capture = Some(MacosCaptureLease::start(target, Arc::clone(&metrics))?);
        }
        let Some(capture_lease) = capture.as_mut() else {
            continue;
        };
        let Some(frame) = capture_lease.receiver.recv().await else {
            return Err(DesktopSidecarErrorBody::new(
                "stream_capture_failed",
                "ScreenCaptureKit capture ended before the stream was stopped.",
                true,
                true,
            ));
        };
        let bitrate = stream_target_bitrate(frame_config.quality, &metrics);
        let force_keyframe = keyframe_requested.swap(false, Ordering::SeqCst);
        if encoder.as_ref().is_none_or(|encoder| {
            !encoder.matches_stream_shape(
                capture_lease.width,
                capture_lease.height,
                capture_lease.target.fps,
            )
        }) {
            encoder = Some(MacosVideoToolboxEncoder::new(
                capture_lease.width,
                capture_lease.height,
                capture_lease.target.fps,
                bitrate,
            )?);
        }
        let mut active_encoder = encoder.take().ok_or_else(stream_state_error)?;
        let encoded = tokio::task::spawn_blocking(move || {
            let result = active_encoder.encode(frame, frame_index, force_keyframe);
            (active_encoder, result)
        })
        .await
        .map_err(|error| {
            DesktopSidecarErrorBody::new(
                "stream_frame_encode_failed",
                format!("VideoToolbox encoder task failed: {error}"),
                true,
                false,
            )
        })?;
        encoder = Some(encoded.0);
        let sample = match encoded.1 {
            Ok(sample) => sample,
            Err(error) if error.code == "stream_frame_dropped" => {
                metrics
                    .capture_dropped_frames
                    .fetch_add(1, Ordering::Relaxed);
                continue;
            }
            Err(error) => return Err(error),
        };
        if sample.keyframe {
            metrics.keyframes.fetch_add(1, Ordering::Relaxed);
        }
        metrics.encode_frames.fetch_add(1, Ordering::Relaxed);
        metrics
            .encode_latency_total_ms
            .fetch_add(sample.encode_latency_ms, Ordering::Relaxed);
        metrics
            .bytes_sent
            .fetch_add(sample.bytes.len() as u64, Ordering::Relaxed);
        track
            .write_sample(&Sample {
                data: sample.bytes.into(),
                duration: sample.duration,
                ..Default::default()
            })
            .await
            .map_err(|error| {
                stream_webrtc_error(
                    "stream_webrtc_failed",
                    "could not write H.264 sample to WebRTC video track",
                    error,
                )
            })?;
        frame_index += 1;
        if frame_index % i64::from(capture_lease.target.fps.max(1)) == 0 {
            refresh_webrtc_transport_metrics_for_peer(&peer_connection, &metrics).await;
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
struct WindowsCaptureLease {
    recorder: xcap::VideoRecorder,
    receiver: Arc<Mutex<std::sync::mpsc::Receiver<xcap::Frame>>>,
    target: NativeVideoTarget,
    monitor_x: i32,
    monitor_y: i32,
}

#[cfg(target_os = "windows")]
impl WindowsCaptureLease {
    fn start(
        target: NativeVideoTarget,
        metrics: Arc<StreamTelemetry>,
    ) -> Result<Self, DesktopSidecarErrorBody> {
        let monitors = Monitor::all().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "stream_capture_unavailable",
                format!("Windows desktop capture could not enumerate displays: {error}"),
                true,
                true,
            )
        })?;
        let monitor = select_monitor(&monitors, target.display_id.as_deref())?;
        let monitor_x = monitor.x().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "stream_capture_unavailable",
                format!("Windows desktop capture could not resolve monitor origin: {error}"),
                true,
                true,
            )
        })?;
        let monitor_y = monitor.y().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "stream_capture_unavailable",
                format!("Windows desktop capture could not resolve monitor origin: {error}"),
                true,
                true,
            )
        })?;
        let (recorder, receiver) = monitor.video_recorder().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "stream_capture_unavailable",
                format!("Windows DXGI desktop capture could not create a recorder: {error}"),
                true,
                true,
            )
        })?;
        recorder.start().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "stream_capture_failed",
                format!("Windows DXGI desktop capture could not start: {error}"),
                true,
                true,
            )
        })?;
        set_stream_metric_string(
            &metrics.capture_backend,
            Some("dxgi_output_duplication".into()),
        );
        Ok(Self {
            recorder,
            receiver: Arc::new(Mutex::new(receiver)),
            target,
            monitor_x,
            monitor_y,
        })
    }

    async fn next_frame(&self) -> Result<xcap::Frame, DesktopSidecarErrorBody> {
        let receiver = Arc::clone(&self.receiver);
        tokio::task::spawn_blocking(move || {
            receiver
                .lock()
                .map_err(|_| stream_state_error())?
                .recv_timeout(Duration::from_millis(500))
                .map_err(|error| match error {
                    std::sync::mpsc::RecvTimeoutError::Timeout => DesktopSidecarErrorBody::new(
                        "stream_capture_timeout",
                        "Windows desktop capture did not produce a frame before timeout.",
                        true,
                        false,
                    ),
                    std::sync::mpsc::RecvTimeoutError::Disconnected => {
                        DesktopSidecarErrorBody::new(
                            "stream_capture_failed",
                            "Windows desktop capture stopped before the stream was stopped.",
                            true,
                            true,
                        )
                    }
                })
        })
        .await
        .map_err(|error| {
            DesktopSidecarErrorBody::new(
                "stream_capture_failed",
                format!("Windows desktop capture receiver task failed: {error}"),
                true,
                false,
            )
        })?
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsCaptureLease {
    fn drop(&mut self) {
        let _ = self.recorder.stop();
    }
}

#[cfg(target_os = "windows")]
struct WindowsOpenH264Encoder {
    encoder: openh264::encoder::Encoder,
    width: u32,
    height: u32,
    fps: u32,
}

#[cfg(target_os = "windows")]
impl WindowsOpenH264Encoder {
    fn new(
        width: u32,
        height: u32,
        fps: u32,
        bitrate: i32,
    ) -> Result<Self, DesktopSidecarErrorBody> {
        let config = openh264::encoder::EncoderConfig::new()
            .usage_type(openh264::encoder::UsageType::ScreenContentRealTime)
            .rate_control_mode(openh264::encoder::RateControlMode::Bitrate)
            .sps_pps_strategy(openh264::encoder::SpsPpsStrategy::ConstantId)
            .max_frame_rate(fps as f32)
            .set_bitrate_bps(bitrate.max(300_000) as u32)
            .enable_skip_frame(true);
        let encoder = openh264::encoder::Encoder::with_api_config(
            openh264::OpenH264API::from_source(),
            config,
        )
        .map_err(|error| {
            DesktopSidecarErrorBody::new(
                "stream_encoder_failed",
                format!("OpenH264 encoder could not start: {error}"),
                true,
                false,
            )
        })?;
        Ok(Self {
            encoder,
            width,
            height,
            fps,
        })
    }

    fn matches_stream_shape(&self, width: u32, height: u32, fps: u32) -> bool {
        self.width == width && self.height == height && self.fps == fps
    }

    fn encode(
        &mut self,
        rgba: Vec<u8>,
        frame_index: i64,
        force_keyframe: bool,
    ) -> Result<EncodedVideoSample, DesktopSidecarErrorBody> {
        let started_at = Instant::now();
        if force_keyframe {
            self.encoder.force_intra_frame();
        }
        let source =
            openh264::formats::RgbaSliceU8::new(&rgba, (self.width as usize, self.height as usize));
        let yuv = openh264::formats::YUVBuffer::from_rgb_source(source);
        let timestamp_ms =
            (u64::try_from(frame_index.max(0)).unwrap_or(0) * 1_000) / u64::from(self.fps.max(1));
        let bitstream = self
            .encoder
            .encode_at(&yuv, openh264::Timestamp::from_millis(timestamp_ms))
            .map_err(|error| {
                DesktopSidecarErrorBody::new(
                    "stream_frame_encode_failed",
                    format!("OpenH264 could not encode a desktop frame: {error}"),
                    true,
                    false,
                )
            })?;
        let keyframe = matches!(
            bitstream.frame_type(),
            openh264::encoder::FrameType::IDR | openh264::encoder::FrameType::I
        );
        let bytes = bitstream.to_vec();
        if bytes.is_empty() {
            return Err(DesktopSidecarErrorBody::new(
                "stream_frame_dropped",
                "OpenH264 skipped a desktop frame before emitting H.264 bytes.",
                true,
                false,
            ));
        }
        Ok(EncodedVideoSample {
            bytes,
            duration: Duration::from_micros(1_000_000 / u64::from(self.fps.max(1))),
            encode_latency_ms: started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            keyframe,
        })
    }
}

#[cfg(target_os = "windows")]
async fn run_windows_webrtc_media_loop(
    track: Arc<TrackLocalStaticSample>,
    peer_connection: Arc<RTCPeerConnection>,
    config: Arc<Mutex<WebRtcStreamConfig>>,
    stop: Arc<AtomicBool>,
    keyframe_requested: Arc<AtomicBool>,
    metrics: Arc<StreamTelemetry>,
) -> Result<(), DesktopSidecarErrorBody> {
    set_stream_metric_string(&metrics.encoder_backend, Some("openh264_software".into()));
    metrics.encoder_hardware.store(false, Ordering::Relaxed);
    let mut capture: Option<WindowsCaptureLease> = None;
    let mut encoder: Option<WindowsOpenH264Encoder> = None;
    let mut frame_index = 0_i64;
    let mut next_frame_at = Instant::now();
    let mut cursor_overlay_diagnostic_recorded = false;
    while !stop.load(Ordering::SeqCst) {
        let frame_config = config.lock().map_err(|_| stream_state_error())?.clone();
        let target = NativeVideoTarget::from_config(&frame_config);
        if capture
            .as_ref()
            .is_none_or(|capture| capture.target != target)
        {
            drop(capture.take());
            encoder = None;
            cursor_overlay_diagnostic_recorded = false;
            capture = Some(WindowsCaptureLease::start(target, Arc::clone(&metrics))?);
        }
        let Some(capture_lease) = capture.as_ref() else {
            continue;
        };
        let frame_interval =
            Duration::from_micros(1_000_000 / u64::from(capture_lease.target.fps.max(1)));
        let now = Instant::now();
        if now < next_frame_at {
            tokio::time::sleep(next_frame_at - now).await;
        }
        next_frame_at = Instant::now() + frame_interval;
        let mut frame = match capture_lease.next_frame().await {
            Ok(frame) => frame,
            Err(error) if error.code == "stream_capture_timeout" => {
                metrics
                    .capture_dropped_frames
                    .fetch_add(1, Ordering::Relaxed);
                continue;
            }
            Err(error) => return Err(error),
        };
        metrics.capture_frames.fetch_add(1, Ordering::Relaxed);
        if capture_lease.target.include_cursor {
            if let Err(error) =
                overlay_windows_cursor(&mut frame, capture_lease.monitor_x, capture_lease.monitor_y)
            {
                if !cursor_overlay_diagnostic_recorded {
                    set_stream_metric_string(&metrics.fallback_reason, Some(error.message));
                    cursor_overlay_diagnostic_recorded = true;
                }
            }
        }
        let (rgba, width, height) = windows_stream_frame_rgba(frame, &capture_lease.target)?;
        let bitrate = stream_target_bitrate(frame_config.quality, &metrics);
        let force_keyframe = keyframe_requested.swap(false, Ordering::SeqCst);
        if encoder.as_ref().is_none_or(|encoder| {
            !encoder.matches_stream_shape(width, height, capture_lease.target.fps)
        }) {
            encoder = Some(WindowsOpenH264Encoder::new(
                width,
                height,
                capture_lease.target.fps,
                bitrate,
            )?);
        }
        let mut active_encoder = encoder.take().ok_or_else(stream_state_error)?;
        let encoded = tokio::task::spawn_blocking(move || {
            let result = active_encoder.encode(rgba, frame_index, force_keyframe);
            (active_encoder, result)
        })
        .await
        .map_err(|error| {
            DesktopSidecarErrorBody::new(
                "stream_frame_encode_failed",
                format!("OpenH264 encoder task failed: {error}"),
                true,
                false,
            )
        })?;
        encoder = Some(encoded.0);
        let sample = match encoded.1 {
            Ok(sample) => sample,
            Err(error) if error.code == "stream_frame_dropped" => {
                metrics
                    .capture_dropped_frames
                    .fetch_add(1, Ordering::Relaxed);
                continue;
            }
            Err(error) => return Err(error),
        };
        if sample.keyframe {
            metrics.keyframes.fetch_add(1, Ordering::Relaxed);
        }
        metrics.encode_frames.fetch_add(1, Ordering::Relaxed);
        metrics
            .encode_latency_total_ms
            .fetch_add(sample.encode_latency_ms, Ordering::Relaxed);
        metrics
            .bytes_sent
            .fetch_add(sample.bytes.len() as u64, Ordering::Relaxed);
        track
            .write_sample(&Sample {
                data: sample.bytes.into(),
                duration: sample.duration,
                ..Default::default()
            })
            .await
            .map_err(|error| {
                stream_webrtc_error(
                    "stream_webrtc_failed",
                    "could not write OpenH264 sample to WebRTC video track",
                    error,
                )
            })?;
        frame_index += 1;
        if frame_index % i64::from(capture_lease.target.fps.max(1)) == 0 {
            refresh_webrtc_transport_metrics_for_peer(&peer_connection, &metrics).await;
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn overlay_windows_cursor(
    frame: &mut xcap::Frame,
    monitor_x: i32,
    monitor_y: i32,
) -> Result<(), DesktopSidecarErrorBody> {
    use std::{ffi::c_void, mem::size_of, ptr, slice};
    use windows::Win32::{
        Foundation::HANDLE,
        Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
            BI_RGB, DIB_RGB_COLORS, HBRUSH, HDC, RGBQUAD,
        },
        UI::WindowsAndMessaging::{
            DrawIconEx, GetCursorInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, DI_NORMAL, ICONINFO,
        },
    };

    let mut cursor = CURSORINFO {
        cbSize: size_of::<CURSORINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetCursorInfo(&mut cursor) }.map_err(|error| {
        DesktopSidecarErrorBody::new(
            "stream_cursor_overlay_unavailable",
            format!("Windows could not report the current cursor for stream overlay: {error}"),
            true,
            false,
        )
    })?;
    if cursor.hCursor.is_invalid() || (cursor.flags.0 & CURSOR_SHOWING.0) == 0 {
        return Ok(());
    }

    let mut icon_info = ICONINFO::default();
    unsafe { GetIconInfo(cursor.hCursor, &mut icon_info) }.map_err(|error| {
        DesktopSidecarErrorBody::new(
            "stream_cursor_overlay_unavailable",
            format!("Windows could not describe the current cursor for stream overlay: {error}"),
            true,
            false,
        )
    })?;
    let _icon_bitmaps = WindowsIconInfoBitmapGuard {
        color: icon_info.hbmColor,
        mask: icon_info.hbmMask,
    };

    let cursor_left = cursor.ptScreenPos.x - monitor_x - icon_info.xHotspot as i32;
    let cursor_top = cursor.ptScreenPos.y - monitor_y - icon_info.yHotspot as i32;
    if cursor_left >= frame.width as i32
        || cursor_top >= frame.height as i32
        || cursor_left < -(frame.width as i32)
        || cursor_top < -(frame.height as i32)
    {
        return Ok(());
    }

    let expected_len = frame
        .width
        .checked_mul(frame.height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "stream_capture_failed",
                "Windows cursor overlay could not prepare a frame because dimensions overflowed.",
                true,
                false,
            )
        })? as usize;
    if frame.raw.len() != expected_len {
        return Err(DesktopSidecarErrorBody::new(
            "stream_capture_failed",
            "Windows cursor overlay received a frame with invalid RGBA dimensions.",
            true,
            false,
        ));
    }

    let dc = unsafe { CreateCompatibleDC(HDC::default()) };
    if dc.is_invalid() {
        return Err(DesktopSidecarErrorBody::new(
            "stream_cursor_overlay_unavailable",
            "Windows could not create a memory device context for cursor overlay.",
            true,
            false,
        ));
    }
    let _dc_guard = WindowsMemoryDcGuard(dc);

    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: frame.width as i32,
            biHeight: -(frame.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: expected_len as u32,
            ..Default::default()
        },
        bmiColors: [RGBQUAD::default()],
    };
    let mut bits: *mut c_void = ptr::null_mut();
    let bitmap = unsafe {
        CreateDIBSection(
            dc,
            &bitmap_info,
            DIB_RGB_COLORS,
            &mut bits,
            HANDLE::default(),
            0,
        )
    }
    .map_err(|error| {
        DesktopSidecarErrorBody::new(
            "stream_cursor_overlay_unavailable",
            format!("Windows could not allocate cursor overlay pixels: {error}"),
            true,
            false,
        )
    })?;
    if bitmap.is_invalid() || bits.is_null() {
        return Err(DesktopSidecarErrorBody::new(
            "stream_cursor_overlay_unavailable",
            "Windows returned an empty cursor overlay bitmap.",
            true,
            false,
        ));
    }
    let _bitmap_guard = WindowsBitmapGuard(bitmap);
    unsafe { SelectObject(dc, bitmap) };

    let dib = unsafe { slice::from_raw_parts_mut(bits.cast::<u8>(), expected_len) };
    for (src, dst) in frame.raw.chunks_exact(4).zip(dib.chunks_exact_mut(4)) {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }
    unsafe {
        DrawIconEx(
            dc,
            cursor_left,
            cursor_top,
            cursor.hCursor,
            0,
            0,
            0,
            HBRUSH::default(),
            DI_NORMAL,
        )
    }
    .map_err(|error| {
        DesktopSidecarErrorBody::new(
            "stream_cursor_overlay_unavailable",
            format!("Windows could not draw the current cursor into the stream frame: {error}"),
            true,
            false,
        )
    })?;
    for (src, dst) in dib.chunks_exact(4).zip(frame.raw.chunks_exact_mut(4)) {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }
    Ok(())
}

#[cfg(target_os = "windows")]
struct WindowsIconInfoBitmapGuard {
    color: windows::Win32::Graphics::Gdi::HBITMAP,
    mask: windows::Win32::Graphics::Gdi::HBITMAP,
}

#[cfg(target_os = "windows")]
impl Drop for WindowsIconInfoBitmapGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.color.is_invalid() {
                let _ = windows::Win32::Graphics::Gdi::DeleteObject(self.color);
            }
            if !self.mask.is_invalid() {
                let _ = windows::Win32::Graphics::Gdi::DeleteObject(self.mask);
            }
        }
    }
}

#[cfg(target_os = "windows")]
struct WindowsMemoryDcGuard(windows::Win32::Graphics::Gdi::HDC);

#[cfg(target_os = "windows")]
impl Drop for WindowsMemoryDcGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::DeleteDC(self.0);
        }
    }
}

#[cfg(target_os = "windows")]
struct WindowsBitmapGuard(windows::Win32::Graphics::Gdi::HBITMAP);

#[cfg(target_os = "windows")]
impl Drop for WindowsBitmapGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::DeleteObject(self.0);
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_stream_frame_rgba(
    frame: xcap::Frame,
    target: &NativeVideoTarget,
) -> Result<(Vec<u8>, u32, u32), DesktopSidecarErrorBody> {
    let (width, height) = scaled_even_dimensions(frame.width, frame.height, target.max_width);
    if width == frame.width && height == frame.height {
        return Ok((frame.raw, width, height));
    }
    let image = RgbaImage::from_raw(frame.width, frame.height, frame.raw).ok_or_else(|| {
        DesktopSidecarErrorBody::new(
            "stream_capture_failed",
            "Windows desktop capture returned a frame with invalid RGBA dimensions.",
            true,
            false,
        )
    })?;
    let resized =
        image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle);
    Ok((resized.into_raw(), width, height))
}

async fn refresh_webrtc_transport_metrics_for_peer(
    peer_connection: &RTCPeerConnection,
    metrics: &StreamTelemetry,
) {
    let stats = peer_connection.get_stats().await;
    for report in stats.reports.values() {
        match report {
            StatsReportType::OutboundRTP(outbound) if outbound.kind == "video" => {
                metrics
                    .packets_sent
                    .store(outbound.packets_sent, Ordering::Relaxed);
                metrics
                    .bytes_sent
                    .store(outbound.bytes_sent, Ordering::Relaxed);
            }
            StatsReportType::RemoteInboundRTP(remote) if remote.kind == "video" => {
                metrics
                    .packets_lost
                    .store(remote.packets_lost, Ordering::Relaxed);
                if let Some(rtt) = remote.round_trip_time {
                    metrics
                        .round_trip_time_ms
                        .store((rtt * 1_000.0).round().max(0.0) as u64, Ordering::Relaxed);
                }
            }
            StatsReportType::CandidatePair(candidate_pair) => {
                metrics
                    .retransmits
                    .store(candidate_pair.retransmissions_sent, Ordering::Relaxed);
                if candidate_pair.available_outgoing_bitrate.is_finite()
                    && candidate_pair.available_outgoing_bitrate > 0.0
                {
                    metrics.available_outgoing_bitrate_bps.store(
                        candidate_pair.available_outgoing_bitrate.round() as u64,
                        Ordering::Relaxed,
                    );
                }
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "macos")]
fn h264_parameter_sets_from_sample_buffer(
    sample_buffer: &apple_cf::cm::CMSampleBuffer,
) -> Result<(Vec<Vec<u8>>, usize), DesktopSidecarErrorBody> {
    let description = sample_buffer.format_description().ok_or_else(|| {
        DesktopSidecarErrorBody::new(
            "stream_encoder_failed",
            "VideoToolbox H.264 sample did not include a format description.",
            true,
            false,
        )
    })?;
    let mut count = 0_usize;
    let mut nal_length_size = 4_i32;
    let mut sets = Vec::new();
    for index in 0..2 {
        let mut pointer: *const u8 = std::ptr::null();
        let mut size = 0_usize;
        let status = unsafe {
            apple_cf::raw::CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                description.as_ptr().cast(),
                index,
                &mut pointer,
                &mut size,
                &mut count,
                &mut nal_length_size,
            )
        };
        if status != 0 {
            if index == 0 {
                return Err(DesktopSidecarErrorBody::new(
                    "stream_encoder_failed",
                    format!("VideoToolbox H.264 parameter sets were unavailable: {status}"),
                    true,
                    false,
                ));
            }
            break;
        }
        if !pointer.is_null() && size > 0 {
            sets.push(unsafe { std::slice::from_raw_parts(pointer, size) }.to_vec());
        }
        if count <= index + 1 {
            break;
        }
    }
    Ok((
        sets,
        usize::try_from(nal_length_size).unwrap_or(4).clamp(1, 4),
    ))
}

fn h264_sample_to_annex_b(
    data: &[u8],
    nal_length_size: usize,
    parameter_sets: &[Vec<u8>],
) -> Result<(Vec<u8>, bool), DesktopSidecarErrorBody> {
    let mut output =
        Vec::with_capacity(data.len() + parameter_sets.iter().map(Vec::len).sum::<usize>() + 32);
    for set in parameter_sets {
        append_annex_b_nal(&mut output, set);
    }
    if h264_annex_b_starts(data) {
        output.extend_from_slice(data);
        return Ok((output, h264_annex_b_contains_idr(data)));
    }
    let mut offset = 0_usize;
    let mut contains_idr = false;
    while offset < data.len() {
        if offset + nal_length_size > data.len() {
            return Err(DesktopSidecarErrorBody::new(
                "stream_frame_encode_failed",
                "VideoToolbox emitted a truncated H.264 length-prefixed sample.",
                true,
                false,
            ));
        }
        let mut nal_size = 0_usize;
        for byte in &data[offset..offset + nal_length_size] {
            nal_size = (nal_size << 8) | usize::from(*byte);
        }
        offset += nal_length_size;
        if nal_size == 0 {
            continue;
        }
        if offset + nal_size > data.len() {
            return Err(DesktopSidecarErrorBody::new(
                "stream_frame_encode_failed",
                "VideoToolbox emitted an H.264 NAL unit larger than its sample buffer.",
                true,
                false,
            ));
        }
        let nal = &data[offset..offset + nal_size];
        if nal.first().is_some_and(|byte| byte & 0x1f == 5) {
            contains_idr = true;
        }
        append_annex_b_nal(&mut output, nal);
        offset += nal_size;
    }
    Ok((output, contains_idr))
}

fn h264_sample_contains_idr(
    data: &[u8],
    nal_length_size: usize,
) -> Result<bool, DesktopSidecarErrorBody> {
    if h264_annex_b_starts(data) {
        return Ok(h264_annex_b_contains_idr(data));
    }

    let mut offset = 0_usize;
    while offset < data.len() {
        if offset + nal_length_size > data.len() {
            return Err(DesktopSidecarErrorBody::new(
                "stream_frame_encode_failed",
                "VideoToolbox emitted a truncated H.264 length-prefixed sample.",
                true,
                false,
            ));
        }
        let mut nal_size = 0_usize;
        for byte in &data[offset..offset + nal_length_size] {
            nal_size = (nal_size << 8) | usize::from(*byte);
        }
        offset += nal_length_size;
        if nal_size == 0 {
            continue;
        }
        if offset + nal_size > data.len() {
            return Err(DesktopSidecarErrorBody::new(
                "stream_frame_encode_failed",
                "VideoToolbox emitted an H.264 NAL unit larger than its sample buffer.",
                true,
                false,
            ));
        }
        if data[offset] & 0x1f == 5 {
            return Ok(true);
        }
        offset += nal_size;
    }
    Ok(false)
}

fn h264_should_include_parameter_sets(
    frame_index: i64,
    force_keyframe: bool,
    contains_idr: bool,
) -> bool {
    frame_index == 0 || contains_idr || force_keyframe
}

fn append_annex_b_nal(output: &mut Vec<u8>, nal: &[u8]) {
    output.extend_from_slice(H264_ANNEX_B_START_CODE);
    output.extend_from_slice(nal);
}

fn h264_annex_b_starts(data: &[u8]) -> bool {
    data.starts_with(H264_ANNEX_B_START_CODE) || data.starts_with(b"\x00\x00\x01")
}

fn h264_annex_b_contains_idr(data: &[u8]) -> bool {
    let mut index = 0;
    while let Some((start, prefix_len)) = next_annex_b_start(data, index) {
        let nal_start = start + prefix_len;
        let next_start = next_annex_b_start(data, nal_start)
            .map(|(next, _)| next)
            .unwrap_or(data.len());
        if nal_start < next_start && data[nal_start] & 0x1f == 5 {
            return true;
        }
        index = next_start;
    }
    false
}

fn next_annex_b_start(data: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut index = from;
    while index + 3 <= data.len() {
        if index + 4 <= data.len() && &data[index..index + 4] == H264_ANNEX_B_START_CODE {
            return Some((index, 4));
        }
        if &data[index..index + 3] == b"\x00\x00\x01" {
            return Some((index, 3));
        }
        index += 1;
    }
    None
}

fn required_stream_id(
    request: &DesktopSidecarStreamRequest,
) -> Result<String, DesktopSidecarErrorBody> {
    request
        .stream_id
        .as_deref()
        .filter(|stream_id| !stream_id.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| schema_error("streamId"))
}

fn stream_state_error() -> DesktopSidecarErrorBody {
    DesktopSidecarErrorBody::new(
        "stream_state_unavailable",
        "Desktop sidecar could not access stream state.",
        true,
        false,
    )
}

fn stream_webrtc_error(
    code: &'static str,
    context: &'static str,
    error: impl std::fmt::Display,
) -> DesktopSidecarErrorBody {
    DesktopSidecarErrorBody::new(code, format!("{context}: {error}"), true, false)
}

fn validate_stream_request(
    operation: DesktopSidecarOperation,
    request: &DesktopSidecarStreamRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    validate_optional_stream_id(request.session_id.as_deref(), "sessionId")?;
    validate_optional_stream_id(request.run_id.as_deref(), "runId")?;
    validate_optional_stream_id(request.display_id.as_deref(), "displayId")?;
    validate_optional_stream_id(request.stream_id.as_deref(), "streamId")?;
    if request
        .max_width
        .is_some_and(|max_width| !(640..=7680).contains(&max_width))
    {
        return Err(schema_error("maxWidth"));
    }
    if request
        .max_frame_rate
        .is_some_and(|frame_rate| !(1..=120).contains(&frame_rate))
    {
        return Err(schema_error("maxFrameRate"));
    }
    for server in &request.ice_servers {
        match &server.urls {
            xero_desktop_control_ipc::DesktopSidecarIceServerUrls::One(url)
                if url.trim().is_empty() =>
            {
                return Err(schema_error("iceServers.urls"));
            }
            xero_desktop_control_ipc::DesktopSidecarIceServerUrls::Many(urls)
                if urls.is_empty() || urls.iter().any(|url| url.trim().is_empty()) =>
            {
                return Err(schema_error("iceServers.urls"));
            }
            _ => {}
        }
        if server
            .credential_type
            .as_deref()
            .is_some_and(|credential_type| !matches!(credential_type, "password" | "oauth"))
        {
            return Err(schema_error("iceServers.credentialType"));
        }
    }
    if matches!(
        operation,
        DesktopSidecarOperation::StreamOffer | DesktopSidecarOperation::StreamAnswer
    ) {
        let Some(description) = request.session_description.as_ref() else {
            return Err(schema_error("sessionDescription"));
        };
        if description.sdp.trim().is_empty()
            || !matches!(
                description.sdp_type.as_str(),
                "offer" | "answer" | "pranswer"
            )
        {
            return Err(schema_error("sessionDescription"));
        }
    }
    if matches!(operation, DesktopSidecarOperation::StreamIceCandidate) {
        let Some(candidate) = request.ice_candidate.as_ref() else {
            return Err(schema_error("iceCandidate"));
        };
        if candidate.candidate.trim().is_empty() {
            return Err(schema_error("iceCandidate.candidate"));
        }
    }
    Ok(())
}

fn validate_optional_stream_id(
    value: Option<&str>,
    field: &'static str,
) -> Result<(), DesktopSidecarErrorBody> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        Err(schema_error(field))
    } else {
        Ok(())
    }
}

struct CapturedDesktopImage {
    image: RgbaImage,
    scale_factor: f32,
    captured_at: String,
    origin_x: i32,
    origin_y: i32,
}

fn capture_desktop_image(
    request: &DesktopSidecarScreenshotRequest,
) -> Result<CapturedDesktopImage, DesktopSidecarErrorBody> {
    let monitors = Monitor::all().map_err(|error| {
        DesktopSidecarErrorBody::new(
            "permission_screen_recording_denied",
            format!("Desktop sidecar could not enumerate capture displays: {error}"),
            true,
            true,
        )
    })?;
    let monitor = select_monitor(&monitors, request.display_id.as_deref())?;
    let scale_factor = monitor.scale_factor().unwrap_or(1.0);
    let monitor_x = monitor.x().unwrap_or_default();
    let monitor_y = monitor.y().unwrap_or_default();
    let (origin_x, origin_y, image) = if let Some(region) = &request.region {
        (
            monitor_x.saturating_add(region.x.min(i32::MAX as u32) as i32),
            monitor_y.saturating_add(region.y.min(i32::MAX as u32) as i32),
            monitor
                .capture_region(region.x, region.y, region.width, region.height)
                .map_err(|error| {
                    DesktopSidecarErrorBody::new(
                        "coordinates_out_of_bounds",
                        format!("Desktop sidecar could not capture the requested region: {error}"),
                        false,
                        false,
                    )
                })?,
        )
    } else {
        (
            monitor_x,
            monitor_y,
            monitor.capture_image().map_err(|error| {
                DesktopSidecarErrorBody::new(
                    "permission_screen_recording_denied",
                    format!("Desktop sidecar could not capture a screenshot: {error}"),
                    true,
                    true,
                )
            })?,
        )
    };
    Ok(CapturedDesktopImage {
        image,
        scale_factor,
        captured_at: now_timestamp(),
        origin_x,
        origin_y,
    })
}

fn encode_png(
    image: &RgbaImage,
    code: &'static str,
    message: &'static str,
) -> Result<Vec<u8>, DesktopSidecarErrorBody> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|error| {
            DesktopSidecarErrorBody::new(code, format!("{message}: {error}"), false, false)
        })?;
    Ok(bytes)
}

fn sidecar_control(
    operation: DesktopSidecarOperation,
    payload: serde_json::Value,
) -> Result<serde_json::Value, DesktopSidecarErrorBody> {
    if operation == DesktopSidecarOperation::CancelCurrentAction {
        return Ok(json!({
            "status": "cancelled",
            "message": "No long-running sidecar action was active."
        }));
    }
    let request =
        serde_json::from_value::<DesktopSidecarControlRequest>(payload).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "sidecar_schema_invalid",
                format!("Control request payload was malformed: {error}"),
                false,
                false,
            )
        })?;
    validate_control_request(operation, &request)?;
    platform_control(operation, request)?;
    Ok(json!({
        "status": "executed",
        "message": format!("Desktop sidecar executed `{operation:?}`."),
    }))
}

fn validate_control_request(
    operation: DesktopSidecarOperation,
    request: &DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    match operation {
        DesktopSidecarOperation::MouseDown
        | DesktopSidecarOperation::MouseMove
        | DesktopSidecarOperation::MouseClick
        | DesktopSidecarOperation::MouseDoubleClick
        | DesktopSidecarOperation::MouseRightClick
        | DesktopSidecarOperation::MouseDragMove
        | DesktopSidecarOperation::MouseUp => required_point(request).map(|_| ()),
        DesktopSidecarOperation::MouseDrag => {
            required_point(request)?;
            required_target_point(request)?;
            Ok(())
        }
        DesktopSidecarOperation::Scroll => {
            let delta_x = request.delta_x.unwrap_or(0);
            let delta_y = request.delta_y.unwrap_or(0);
            if delta_x == 0 && delta_y == 0 {
                return Err(schema_error("deltaX/deltaY"));
            }
            Ok(())
        }
        DesktopSidecarOperation::KeyPress => request
            .key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
            .map(|_| ())
            .ok_or_else(|| schema_error("key")),
        DesktopSidecarOperation::Hotkey => {
            if request.keys.is_empty() {
                return Err(schema_error("keys"));
            }
            Ok(())
        }
        DesktopSidecarOperation::TypeText
        | DesktopSidecarOperation::PasteText
        | DesktopSidecarOperation::ClipboardWriteText => request
            .text
            .as_deref()
            .filter(|text| !text.is_empty())
            .map(|_| ())
            .ok_or_else(|| schema_error("text")),
        DesktopSidecarOperation::ClipboardWriteHtml => request
            .html
            .as_deref()
            .filter(|html| !html.trim().is_empty())
            .map(|_| ())
            .ok_or_else(|| schema_error("html")),
        DesktopSidecarOperation::ClipboardWriteRtf => request
            .rtf
            .as_deref()
            .filter(|rtf| !rtf.trim().is_empty() && rtf.len() <= CLIPBOARD_RTF_MAX_BYTES)
            .map(|_| ())
            .ok_or_else(|| schema_error("rtf")),
        DesktopSidecarOperation::ClipboardWriteImage => validate_clipboard_image_write(request),
        DesktopSidecarOperation::ClipboardWriteFiles | DesktopSidecarOperation::FileDrop => {
            validate_clipboard_file_paths(request)
        }
        DesktopSidecarOperation::FocusWindow
        | DesktopSidecarOperation::WindowMaximize
        | DesktopSidecarOperation::WindowMinimize
        | DesktopSidecarOperation::WindowRestore
        | DesktopSidecarOperation::WindowClose => validate_app_or_window_target(request),
        DesktopSidecarOperation::WindowMoveResize => {
            validate_app_or_window_target(request)?;
            validate_window_layout_bounds(request)
        }
        DesktopSidecarOperation::ActivateApp | DesktopSidecarOperation::QuitApp => {
            validate_app_or_window_target(request)
        }
        DesktopSidecarOperation::LaunchApp => {
            if has_non_empty_value(request.app_name.as_deref())
                || has_non_empty_value(request.bundle_id.as_deref())
            {
                Ok(())
            } else {
                Err(schema_error("appName or bundleId"))
            }
        }
        DesktopSidecarOperation::AxPress
        | DesktopSidecarOperation::AxFocus
        | DesktopSidecarOperation::AxSelect
        | DesktopSidecarOperation::AxConfirm
        | DesktopSidecarOperation::AxCancel
        | DesktopSidecarOperation::AxIncrement
        | DesktopSidecarOperation::AxDecrement
        | DesktopSidecarOperation::AxExpand
        | DesktopSidecarOperation::AxCollapse
        | DesktopSidecarOperation::AxScrollToVisible
        | DesktopSidecarOperation::AxToggle => validate_accessibility_control_target(request),
        DesktopSidecarOperation::AxSetValue => {
            validate_accessibility_control_target(request)?;
            validate_accessibility_value_request(request)
        }
        DesktopSidecarOperation::MenuSelect => {
            if request.menu_path.is_empty() {
                return Err(schema_error("menuPath"));
            }
            Ok(())
        }
        DesktopSidecarOperation::DockItemPress => {
            if has_non_empty_value(request.app_name.as_deref())
                || has_non_empty_value(request.target_label.as_deref())
            {
                Ok(())
            } else {
                Err(schema_error("appName or targetLabel"))
            }
        }
        DesktopSidecarOperation::StatusItemPress => {
            if has_non_empty_value(request.target_label.as_deref()) {
                Ok(())
            } else {
                validate_accessibility_control_target(request)
            }
        }
        DesktopSidecarOperation::FileDialogSetPath => validate_file_dialog_path(request),
        DesktopSidecarOperation::FileDialogConfirm => Ok(()),
        DesktopSidecarOperation::CancelCurrentAction => Ok(()),
        _ => Ok(()),
    }
}

fn validate_app_or_window_target(
    request: &DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    if has_non_empty_value(request.window_id.as_deref())
        || has_non_empty_value(request.app_name.as_deref())
        || has_non_empty_value(request.bundle_id.as_deref())
    {
        Ok(())
    } else {
        Err(schema_error("windowId, appName, or bundleId"))
    }
}

fn validate_window_layout_bounds(
    request: &DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    let has_position = matches!((request.x, request.y), (Some(x), Some(y)) if x >= 0 && y >= 0);
    let partial_position = matches!((request.x, request.y), (Some(_), None) | (None, Some(_)));
    if partial_position {
        return Err(schema_error("x/y"));
    }
    let has_size = matches!((request.width, request.height), (Some(width), Some(height)) if width > 0 && height > 0);
    let partial_size = matches!(
        (request.width, request.height),
        (Some(_), None) | (None, Some(_))
    );
    if partial_size || request.width == Some(0) || request.height == Some(0) {
        return Err(schema_error("width/height"));
    }
    if has_position || has_size {
        Ok(())
    } else {
        Err(schema_error("x/y or width/height"))
    }
}

fn validate_clipboard_image_write(
    request: &DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    let media_type = request
        .media_type
        .as_deref()
        .unwrap_or("image/png")
        .trim()
        .to_ascii_lowercase();
    if media_type != "image/png" {
        return Err(schema_error("mediaType"));
    }
    request
        .image_data_base64
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|_| ())
        .ok_or_else(|| schema_error("imageDataBase64"))
}

fn validate_clipboard_file_paths(
    request: &DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    if request.file_paths.is_empty() || request.file_paths.len() > CLIPBOARD_MAX_FILE_PATHS {
        return Err(schema_error("filePaths"));
    }
    for path in &request.file_paths {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(schema_error("filePaths"));
        }
        let path = std::path::Path::new(trimmed);
        if !path.is_absolute() {
            return Err(schema_error("filePaths"));
        }
        if !path.try_exists().unwrap_or(false) {
            return Err(DesktopSidecarErrorBody::new(
                "desktop_clipboard_file_not_found",
                format!(
                    "Desktop sidecar could not add `{}` to the clipboard because it does not exist.",
                    trimmed
                ),
                false,
                true,
            ));
        }
    }
    Ok(())
}

fn validate_file_dialog_path(
    request: &DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    if request.file_paths.len() != 1 {
        return Err(schema_error("filePaths"));
    }
    let trimmed = request.file_paths[0].trim();
    if trimmed.is_empty() {
        return Err(schema_error("filePaths"));
    }
    let path = std::path::Path::new(trimmed);
    if !path.is_absolute() {
        return Err(schema_error("filePaths"));
    }
    if path.try_exists().unwrap_or(false)
        || path
            .parent()
            .is_some_and(|parent| parent.try_exists().unwrap_or(false))
    {
        Ok(())
    } else {
        Err(DesktopSidecarErrorBody::new(
            "desktop_file_dialog_path_not_found",
            format!(
                "Desktop sidecar could not use `{trimmed}` in the file dialog because neither the path nor its parent exists."
            ),
            false,
            true,
        ))
    }
}

fn has_non_empty_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

fn validate_accessibility_control_target(
    request: &DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    if let (Some(x), Some(y)) = (request.x, request.y) {
        if x >= 0 && y >= 0 {
            return Ok(());
        }
    }
    if request
        .element_id
        .as_deref()
        .is_some_and(|element_id| !element_id.trim().is_empty())
    {
        return Ok(());
    }
    Err(schema_error("elementId or x/y"))
}

fn validate_accessibility_value_request(
    request: &DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    let has_value = request.value.as_ref().is_some();
    match (request.selection_start, request.selection_end) {
        (Some(start), Some(end)) if start <= end && has_value => Ok(()),
        (Some(_), Some(_)) => Err(schema_error("selectionStart/selectionEnd")),
        (None, None) => request
            .value
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(|_| ())
            .ok_or_else(|| schema_error("value")),
        _ => Err(schema_error("selectionStart/selectionEnd")),
    }
}

fn desktop_text_input_route(text: &str) -> DesktopTextInputRoute {
    if text.chars().count() > TYPE_TEXT_PASTE_FIRST_THRESHOLD_CHARS || !text.is_ascii() {
        DesktopTextInputRoute::ClipboardPaste
    } else {
        DesktopTextInputRoute::KeyEvents
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn windows_menu_alt_mnemonics(menu_path: &[String]) -> Vec<String> {
    let mut mnemonics = Vec::with_capacity(menu_path.len());
    for segment in menu_path {
        let Some(mnemonic) = windows_menu_mnemonic_for_segment(segment) else {
            return Vec::new();
        };
        mnemonics.push(mnemonic.to_string());
    }
    mnemonics
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn windows_menu_mnemonic_for_segment(segment: &str) -> Option<char> {
    let mut chars = segment.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '&' {
            continue;
        }
        match chars.peek().copied() {
            Some('&') => {
                chars.next();
            }
            Some(next) if next.is_ascii_alphanumeric() => {
                return Some(next.to_ascii_lowercase());
            }
            _ => {}
        }
    }

    segment
        .chars()
        .find(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
}

#[cfg(target_os = "macos")]
fn platform_control(
    operation: DesktopSidecarOperation,
    request: DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    match operation {
        DesktopSidecarOperation::MouseDown => macos_input::mouse_down(
            required_point(&request)?,
            request.button.unwrap_or_default(),
        ),
        DesktopSidecarOperation::MouseMove => macos_input::mouse_move(required_point(&request)?),
        DesktopSidecarOperation::MouseClick
        | DesktopSidecarOperation::MouseDoubleClick
        | DesktopSidecarOperation::MouseRightClick => {
            let point = required_point(&request)?;
            let button = if operation == DesktopSidecarOperation::MouseRightClick {
                xero_desktop_control_ipc::DesktopSidecarMouseButton::Right
            } else {
                request.button.unwrap_or_default()
            };
            let clicks = if operation == DesktopSidecarOperation::MouseDoubleClick {
                2
            } else {
                request.clicks.unwrap_or(1).max(1)
            };
            macos_input::mouse_click(point, button, clicks)
        }
        DesktopSidecarOperation::MouseDrag => {
            macos_input::mouse_drag(required_point(&request)?, required_target_point(&request)?)
        }
        DesktopSidecarOperation::MouseDragMove => {
            macos_input::mouse_drag_move(required_point(&request)?)
        }
        DesktopSidecarOperation::MouseUp => macos_input::mouse_up(
            required_point(&request)?,
            request.button.unwrap_or_default(),
        ),
        DesktopSidecarOperation::Scroll => {
            let delta_x = request.delta_x.unwrap_or(0);
            let delta_y = request.delta_y.unwrap_or(0);
            if delta_x == 0 && delta_y == 0 {
                return Err(schema_error("deltaX/deltaY"));
            }
            macos_input::scroll(delta_x, delta_y)
        }
        DesktopSidecarOperation::KeyPress => {
            let key = request
                .key
                .as_deref()
                .filter(|key| !key.trim().is_empty())
                .ok_or_else(|| schema_error("key"))?;
            macos_input::key_press(key)
        }
        DesktopSidecarOperation::Hotkey => {
            if request.keys.is_empty() {
                return Err(schema_error("keys"));
            }
            macos_input::hotkey(&request.keys)
        }
        DesktopSidecarOperation::TypeText => {
            let text = request
                .text
                .as_deref()
                .filter(|text| !text.is_empty())
                .ok_or_else(|| schema_error("text"))?;
            match desktop_text_input_route(text) {
                DesktopTextInputRoute::KeyEvents => macos_input::type_text(text),
                DesktopTextInputRoute::ClipboardPaste => macos_clipboard::paste_text(text),
            }
        }
        DesktopSidecarOperation::PasteText => {
            let text = request
                .text
                .as_deref()
                .filter(|text| !text.is_empty())
                .ok_or_else(|| schema_error("text"))?;
            macos_clipboard::paste_text(text)
        }
        DesktopSidecarOperation::ClipboardWriteText => {
            let text = request
                .text
                .as_deref()
                .filter(|text| !text.is_empty())
                .ok_or_else(|| schema_error("text"))?;
            macos_clipboard::write_text(text)
        }
        DesktopSidecarOperation::ClipboardWriteHtml => clipboard_resources::write_html(&request),
        DesktopSidecarOperation::ClipboardWriteRtf => clipboard_resources::write_rtf(&request),
        DesktopSidecarOperation::ClipboardWriteImage => clipboard_resources::write_image(&request),
        DesktopSidecarOperation::ClipboardWriteFiles => clipboard_resources::write_files(&request),
        DesktopSidecarOperation::FileDrop => clipboard_resources::file_drop(&request, || {
            macos_input::hotkey(&["command".into(), "v".into()])
        }),
        DesktopSidecarOperation::WindowMaximize => macos_accessibility::window_maximize(&request),
        DesktopSidecarOperation::WindowMinimize => macos_accessibility::window_minimize(&request),
        DesktopSidecarOperation::WindowRestore => macos_accessibility::window_restore(&request),
        DesktopSidecarOperation::WindowMoveResize => {
            macos_accessibility::window_move_resize(&request)
        }
        DesktopSidecarOperation::WindowClose => macos_accessibility::window_close(&request),
        DesktopSidecarOperation::AxPress => macos_accessibility::press(&request),
        DesktopSidecarOperation::AxSetValue => macos_accessibility::set_value(&request),
        DesktopSidecarOperation::AxFocus => macos_accessibility::focus(&request),
        DesktopSidecarOperation::AxSelect => macos_accessibility::select(&request),
        DesktopSidecarOperation::AxConfirm => macos_accessibility::confirm(&request),
        DesktopSidecarOperation::AxCancel => macos_accessibility::cancel(&request),
        DesktopSidecarOperation::AxIncrement => macos_accessibility::increment(&request),
        DesktopSidecarOperation::AxDecrement => macos_accessibility::decrement(&request),
        DesktopSidecarOperation::AxExpand => macos_accessibility::expand(&request),
        DesktopSidecarOperation::AxCollapse => macos_accessibility::collapse(&request),
        DesktopSidecarOperation::AxScrollToVisible => {
            macos_accessibility::scroll_to_visible(&request)
        }
        DesktopSidecarOperation::AxToggle => macos_accessibility::toggle(&request),
        DesktopSidecarOperation::MenuSelect => macos_accessibility::menu_select(&request),
        DesktopSidecarOperation::DockItemPress => macos_accessibility::dock_item_press(&request),
        DesktopSidecarOperation::StatusItemPress => {
            macos_accessibility::status_item_press(&request)
        }
        DesktopSidecarOperation::FileDialogSetPath => {
            macos_accessibility::file_dialog_set_path(&request)
        }
        DesktopSidecarOperation::FileDialogConfirm => {
            macos_accessibility::file_dialog_confirm(&request)
        }
        _ => Err(unimplemented_operation()),
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn platform_control(
    operation: DesktopSidecarOperation,
    request: DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    match operation {
        DesktopSidecarOperation::MouseDown => cross_platform_input::mouse_down(
            required_point(&request)?,
            request.button.unwrap_or_default(),
        ),
        DesktopSidecarOperation::MouseMove => {
            cross_platform_input::mouse_move(required_point(&request)?)
        }
        DesktopSidecarOperation::MouseClick
        | DesktopSidecarOperation::MouseDoubleClick
        | DesktopSidecarOperation::MouseRightClick => {
            let point = required_point(&request)?;
            let button = if operation == DesktopSidecarOperation::MouseRightClick {
                xero_desktop_control_ipc::DesktopSidecarMouseButton::Right
            } else {
                request.button.unwrap_or_default()
            };
            let clicks = if operation == DesktopSidecarOperation::MouseDoubleClick {
                2
            } else {
                request.clicks.unwrap_or(1).max(1)
            };
            cross_platform_input::mouse_click(point, button, clicks)
        }
        DesktopSidecarOperation::MouseDrag => cross_platform_input::mouse_drag(
            required_point(&request)?,
            required_target_point(&request)?,
        ),
        DesktopSidecarOperation::MouseDragMove => {
            cross_platform_input::mouse_drag_move(required_point(&request)?)
        }
        DesktopSidecarOperation::MouseUp => cross_platform_input::mouse_up(
            required_point(&request)?,
            request.button.unwrap_or_default(),
        ),
        DesktopSidecarOperation::Scroll => {
            let delta_x = request.delta_x.unwrap_or(0);
            let delta_y = request.delta_y.unwrap_or(0);
            if delta_x == 0 && delta_y == 0 {
                return Err(schema_error("deltaX/deltaY"));
            }
            cross_platform_input::scroll(delta_x, delta_y)
        }
        DesktopSidecarOperation::KeyPress => {
            let key = request
                .key
                .as_deref()
                .filter(|key| !key.trim().is_empty())
                .ok_or_else(|| schema_error("key"))?;
            cross_platform_input::key_press(key)
        }
        DesktopSidecarOperation::Hotkey => {
            if request.keys.is_empty() {
                return Err(schema_error("keys"));
            }
            cross_platform_input::hotkey(&request.keys)
        }
        DesktopSidecarOperation::TypeText => {
            let text = request
                .text
                .as_deref()
                .filter(|text| !text.is_empty())
                .ok_or_else(|| schema_error("text"))?;
            match desktop_text_input_route(text) {
                DesktopTextInputRoute::KeyEvents => cross_platform_input::type_text(text),
                DesktopTextInputRoute::ClipboardPaste => cross_platform_input::paste_text(text),
            }
        }
        DesktopSidecarOperation::PasteText => {
            let text = request
                .text
                .as_deref()
                .filter(|text| !text.is_empty())
                .ok_or_else(|| schema_error("text"))?;
            cross_platform_input::paste_text(text)
        }
        DesktopSidecarOperation::ClipboardWriteText => {
            let text = request
                .text
                .as_deref()
                .filter(|text| !text.is_empty())
                .ok_or_else(|| schema_error("text"))?;
            cross_platform_input::write_clipboard_text(text)
        }
        DesktopSidecarOperation::ClipboardWriteHtml => clipboard_resources::write_html(&request),
        DesktopSidecarOperation::ClipboardWriteRtf => clipboard_resources::write_rtf(&request),
        DesktopSidecarOperation::ClipboardWriteImage => clipboard_resources::write_image(&request),
        DesktopSidecarOperation::ClipboardWriteFiles => clipboard_resources::write_files(&request),
        DesktopSidecarOperation::FileDrop => clipboard_resources::file_drop(&request, || {
            cross_platform_input::hotkey(&["control".into(), "v".into()])
        }),
        DesktopSidecarOperation::FocusWindow => {
            #[cfg(target_os = "windows")]
            {
                windows_app_control::focus_window(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::WindowMaximize => {
            #[cfg(target_os = "windows")]
            {
                windows_app_control::window_maximize(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::WindowMinimize => {
            #[cfg(target_os = "windows")]
            {
                windows_app_control::window_minimize(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::WindowRestore => {
            #[cfg(target_os = "windows")]
            {
                windows_app_control::window_restore(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::WindowMoveResize => {
            #[cfg(target_os = "windows")]
            {
                windows_app_control::window_move_resize(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::WindowClose => {
            #[cfg(target_os = "windows")]
            {
                windows_app_control::window_close(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::ActivateApp => {
            #[cfg(target_os = "windows")]
            {
                windows_app_control::activate_app(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::LaunchApp => {
            #[cfg(target_os = "windows")]
            {
                windows_app_control::launch_app(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::QuitApp => {
            #[cfg(target_os = "windows")]
            {
                windows_app_control::quit_app(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::AxPress => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::press(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::AxSetValue => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::set_value(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::AxFocus => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::focus(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::AxSelect => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::select(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::AxConfirm => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::confirm(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::AxCancel => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::cancel(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::AxIncrement => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::increment(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::AxDecrement => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::decrement(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::AxExpand => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::expand(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::AxCollapse => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::collapse(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::AxScrollToVisible => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::scroll_to_visible(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::AxToggle => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::toggle(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        DesktopSidecarOperation::MenuSelect => {
            #[cfg(target_os = "windows")]
            {
                windows_ui_automation::menu_select(&request)
            }
            #[cfg(not(target_os = "windows"))]
            {
                Err(unimplemented_operation())
            }
        }
        _ => Err(unimplemented_operation()),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn platform_control(
    _operation: DesktopSidecarOperation,
    _request: DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    Err(unimplemented_operation())
}

fn required_point(
    request: &DesktopSidecarControlRequest,
) -> Result<(i32, i32), DesktopSidecarErrorBody> {
    match (request.x, request.y) {
        (Some(x), Some(y)) if x >= 0 && y >= 0 => Ok((x, y)),
        _ => Err(schema_error("x/y")),
    }
}

fn required_target_point(
    request: &DesktopSidecarControlRequest,
) -> Result<(i32, i32), DesktopSidecarErrorBody> {
    match (request.to_x, request.to_y) {
        (Some(x), Some(y)) if x >= 0 && y >= 0 => Ok((x, y)),
        _ => Err(schema_error("toX/toY")),
    }
}

fn schema_error(field: &'static str) -> DesktopSidecarErrorBody {
    DesktopSidecarErrorBody::new(
        "sidecar_schema_invalid",
        format!("Desktop sidecar request is missing or invalid `{field}`."),
        false,
        false,
    )
}

fn unimplemented_operation() -> DesktopSidecarErrorBody {
    DesktopSidecarErrorBody::new(
        "sidecar_operation_unimplemented",
        format!(
            "This sidecar operation is not implemented by the active {} backend. Check stream_capabilities, permissions_status, or accessibility_snapshot diagnostics before falling back to pointer/keyboard input.",
            std::env::consts::OS
        ),
        false,
        false,
    )
}

fn cursor_state_error() -> DesktopSidecarErrorBody {
    DesktopSidecarErrorBody::new(
        "desktop_cursor_state_unavailable",
        "Desktop sidecar could not read the current cursor location.",
        true,
        false,
    )
}

fn select_monitor<'a>(
    monitors: &'a [Monitor],
    display_id: Option<&str>,
) -> Result<&'a Monitor, DesktopSidecarErrorBody> {
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
        return Err(DesktopSidecarErrorBody::new(
            "display_not_found",
            format!("Desktop sidecar could not find display `{display_id}`."),
            false,
            true,
        ));
    }
    monitors
        .iter()
        .find(|monitor| monitor.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "display_not_found",
                "Desktop sidecar could not find a capture display.",
                false,
                true,
            )
        })
}

fn apps_from_windows(windows: &[DesktopSidecarWindow]) -> Vec<DesktopSidecarApp> {
    let mut apps: BTreeMap<(String, u32), DesktopSidecarApp> = BTreeMap::new();
    for window in windows {
        let key = (window.app_name.clone(), window.pid);
        let entry = apps.entry(key).or_insert_with(|| DesktopSidecarApp {
            app_name: window.app_name.clone(),
            pid: window.pid,
            window_count: 0,
            focused: false,
        });
        entry.window_count += 1;
        entry.focused |= window.focused;
    }
    apps.into_values().collect()
}

fn desktop_label_preview(value: &str) -> String {
    value.chars().take(240).collect()
}

fn now_timestamp() -> String {
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

#[cfg(target_os = "macos")]
mod macos_accessibility {
    use std::{ffi::c_void, path::Path, process::Command, ptr, thread, time::Duration};

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use core_foundation::{
        array::CFArray,
        base::{CFIndex, CFRange, CFType, CFTypeID, CFTypeRef, TCFType},
        boolean::CFBoolean,
        number::CFNumber,
        string::{CFString, CFStringRef},
    };
    use core_graphics::geometry::{CGPoint, CGSize};
    use serde::{Deserialize, Serialize};

    use super::{
        desktop_label_preview, macos_clipboard, macos_input, schema_error,
        DesktopSidecarAccessibilityElement, DesktopSidecarAccessibilitySnapshotPayload,
        DesktopSidecarAccessibilitySnapshotRequest, DesktopSidecarAccessibilitySnapshotRow,
        DesktopSidecarAccessibilitySnapshotTarget, DesktopSidecarControlRequest,
        DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody, DesktopSidecarPointRequest,
    };

    type AXError = i32;
    type AXUIElementRef = *const c_void;
    type AXValueRef = *const c_void;

    const AX_ERROR_SUCCESS: AXError = 0;
    const AX_VALUE_CGPOINT_TYPE: i32 = 1;
    const AX_VALUE_CGSIZE_TYPE: i32 = 2;
    const AX_VALUE_CFRANGE_TYPE: i32 = 4;

    pub(super) fn snapshot(
        request: DesktopSidecarAccessibilitySnapshotRequest,
    ) -> Result<DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarErrorBody> {
        if !accessibility_permission_granted() {
            return Ok(DesktopSidecarAccessibilitySnapshotPayload {
                performed: false,
                target: None,
                rows: Vec::new(),
                truncated: false,
                diagnostics: vec!["Grant Xero Accessibility permission in System Settings > Privacy & Security > Accessibility, then retry.".into()],
            });
        }

        let target = resolve_snapshot_target(&request)?;
        let mut context = SnapshotContext {
            rows: Vec::new(),
            limit: request.limit.unwrap_or(120),
            max_depth: request
                .max_depth
                .unwrap_or(if request.include_children { 5 } else { 0 }),
            include_children: request.include_children,
            truncated: false,
        };

        let app_identity = AxElementIdentityContext {
            app_name: target
                .app_name
                .clone()
                .or_else(|| ax_string_attribute(&target.app, "AXTitle")),
            ..AxElementIdentityContext::default()
        };
        let app_row = snapshot_row(
            "macos_accessibility_app",
            &target.app,
            0,
            None,
            &app_identity,
        );
        context.push(app_row);

        let windows = target.windows();
        if windows.is_empty() {
            return Ok(DesktopSidecarAccessibilitySnapshotPayload {
                performed: true,
                target: Some(target.snapshot_target()),
                rows: context.rows,
                truncated: context.truncated,
                diagnostics: vec![
                    "macOS Accessibility returned no window references for the selected app."
                        .into(),
                ],
            });
        }

        for (index, window) in windows.into_iter().enumerate() {
            if context.is_full() {
                context.truncated = true;
                break;
            }
            let window_identity = window_identity_context(&target, &window, index);
            let row = snapshot_row(
                "macos_accessibility_window",
                &window,
                0,
                Some(index),
                &window_identity,
            );
            context.push(row);
            snapshot_children(&mut context, &window, 1, &window_identity);
        }

        Ok(DesktopSidecarAccessibilitySnapshotPayload {
            performed: true,
            target: Some(target.snapshot_target()),
            rows: context.rows,
            truncated: context.truncated,
            diagnostics: Vec::new(),
        })
    }

    pub(super) fn element_at_point(
        request: DesktopSidecarPointRequest,
    ) -> Result<DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody> {
        if !accessibility_permission_granted() {
            return Err(DesktopSidecarErrorBody::new(
                "permission_accessibility_denied",
                "Grant Xero Accessibility permission in System Settings > Privacy & Security > Accessibility, then retry.",
                true,
                true,
            ));
        }

        let system_wide = AxElement::system_wide().ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_accessibility_backend_unavailable",
                "Desktop sidecar could not create the macOS Accessibility system reference.",
                true,
                false,
            )
        })?;
        let element = element_at_position(&system_wide, request.x, request.y)?;
        Ok(DesktopSidecarElementAtPointPayload {
            x: request.x,
            y: request.y,
            available: true,
            element: Some(describe_element(
                &element,
                request.x,
                request.y,
                &element_identity_context(&element),
            )),
        })
    }

    pub(super) fn press(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let element = control_element(request)?;
        perform_action(&element, "AXPress")
    }

    pub(super) fn focus(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let element = control_element(request)?;
        set_bool_attribute(&element, "AXFocused", true)
    }

    pub(super) fn set_value(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let value = request
            .value
            .as_deref()
            .ok_or_else(|| schema_error("value"))?;
        let element = control_element(request)?;
        if let (Some(start), Some(end)) = (request.selection_start, request.selection_end) {
            if start > end {
                return Err(schema_error("selectionStart/selectionEnd"));
            }
            set_text_range(&element, start, end)?;
            return set_string_attribute(&element, "AXSelectedText", value);
        }
        if request.selection_start.is_some() || request.selection_end.is_some() {
            return Err(schema_error("selectionStart/selectionEnd"));
        }
        if value.is_empty() {
            return Err(schema_error("value"));
        }
        set_string_attribute(&element, "AXValue", value)
    }

    pub(super) fn window_maximize(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let window = window_control_target(request)?;
        perform_first_action(&window, &["AXZoom", "AXRaise"])
    }

    pub(super) fn window_minimize(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let window = window_control_target(request)?;
        set_bool_attribute(&window, "AXMinimized", true)
    }

    pub(super) fn window_restore(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let window = window_control_target(request)?;
        let _ = set_bool_attribute(&window, "AXMinimized", false);
        perform_first_action(&window, &["AXRaise", "AXZoom"])
    }

    pub(super) fn window_move_resize(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let window = window_control_target(request)?;
        if let (Some(x), Some(y)) = (request.x, request.y) {
            if x < 0 || y < 0 {
                return Err(schema_error("x/y"));
            }
            set_point_attribute(&window, "AXPosition", CGPoint::new(x as f64, y as f64))?;
        }
        if let (Some(width), Some(height)) = (request.width, request.height) {
            if width == 0 || height == 0 {
                return Err(schema_error("width/height"));
            }
            set_size_attribute(&window, "AXSize", CGSize::new(width as f64, height as f64))?;
        }
        Ok(())
    }

    pub(super) fn window_close(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let window = window_control_target(request)?;
        if let Some(close_button) = ax_element_attribute(&window, "AXCloseButton") {
            return perform_action(&close_button, "AXPress");
        }
        perform_first_action(&window, &["AXClose", "AXCancel"])
    }

    pub(super) fn select(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let element = control_element(request)?;
        perform_first_action(&element, &["AXSelect", "AXPress"])
    }

    pub(super) fn confirm(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let element = control_element(request)?;
        perform_first_action(&element, &["AXConfirm", "AXPress"])
    }

    pub(super) fn cancel(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let element = control_element(request)?;
        match perform_action(&element, "AXCancel") {
            Ok(()) => Ok(()),
            Err(_) if element_suggests_cancel(&element) => perform_action(&element, "AXPress"),
            Err(error) => Err(error),
        }
    }

    pub(super) fn increment(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let element = control_element(request)?;
        perform_action(&element, "AXIncrement")
    }

    pub(super) fn decrement(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let element = control_element(request)?;
        perform_action(&element, "AXDecrement")
    }

    pub(super) fn expand(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        set_expanded(request, true)
    }

    pub(super) fn collapse(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        set_expanded(request, false)
    }

    pub(super) fn scroll_to_visible(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let element = control_element(request)?;
        perform_action(&element, "AXScrollToVisible")
    }

    pub(super) fn toggle(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let element = control_element(request)?;
        perform_action(&element, "AXPress")
    }

    pub(super) fn menu_select(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        if request.menu_path.is_empty() {
            return Err(schema_error("menuPath"));
        }
        if !accessibility_permission_granted() {
            return Err(DesktopSidecarErrorBody::new(
                "permission_accessibility_denied",
                "Grant Xero Accessibility permission in System Settings > Privacy & Security > Accessibility, then retry.",
                true,
                true,
            ));
        }
        let system_wide = AxElement::system_wide().ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_accessibility_backend_unavailable",
                "Desktop sidecar could not create the macOS Accessibility system reference.",
                true,
                false,
            )
        })?;
        let focused_app =
            ax_element_attribute(&system_wide, "AXFocusedApplication").ok_or_else(|| {
                DesktopSidecarErrorBody::new(
                    "desktop_menu_select_failed",
                    "macOS Accessibility did not return a focused application for menu selection.",
                    true,
                    false,
                )
            })?;
        let mut current = ax_element_attribute(&focused_app, "AXMenuBar").ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_menu_select_failed",
                "macOS Accessibility did not return a focused application menu bar.",
                true,
                false,
            )
        })?;

        for (index, segment) in request.menu_path.iter().enumerate() {
            let target = find_child_by_title(&current, segment).ok_or_else(|| {
                DesktopSidecarErrorBody::new(
                    "desktop_menu_select_failed",
                    format!("Could not find menu segment `{segment}` in the focused app."),
                    false,
                    true,
                )
            })?;
            if index == request.menu_path.len() - 1 {
                return perform_action(&target, "AXPress");
            }
            perform_action(&target, "AXPress")?;
            thread::sleep(Duration::from_millis(80));
            current = ax_element_attribute(&target, "AXMenu")
                .or_else(|| {
                    ax_element_array_attribute(&target, "AXChildren")
                        .into_iter()
                        .find(|child| {
                            ax_string_attribute(child, "AXRole").as_deref() == Some("AXMenu")
                        })
                })
                .ok_or_else(|| {
                    DesktopSidecarErrorBody::new(
                        "desktop_menu_select_failed",
                        format!("Menu segment `{segment}` did not expose a submenu."),
                        false,
                        true,
                    )
                })?;
        }
        Err(schema_error("menuPath"))
    }

    pub(super) fn dock_item_press(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let label = request
            .target_label
            .as_deref()
            .or(request.app_name.as_deref())
            .filter(|label| !label.trim().is_empty())
            .ok_or_else(|| schema_error("appName or targetLabel"))?;
        let dock = application_by_process_name("Dock")?;
        let item = find_descendant_by_label(&dock, label).ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_dock_item_not_found",
                format!("Could not find Dock item `{label}` through macOS Accessibility."),
                false,
                true,
            )
        })?;
        perform_first_action(&item, &["AXPress", "AXShowMenu"])
    }

    pub(super) fn status_item_press(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        if has_element_or_point_target(request) {
            let element = control_element(request)?;
            return perform_action(&element, "AXPress");
        }
        let label = request
            .target_label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
            .ok_or_else(|| schema_error("targetLabel or elementId/x/y"))?;
        let system_ui = application_by_process_name("SystemUIServer")?;
        let item = find_descendant_by_label(&system_ui, label).ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_status_item_not_found",
                format!(
                    "Could not find menu bar status item `{label}` through macOS Accessibility."
                ),
                false,
                true,
            )
        })?;
        perform_action(&item, "AXPress")
    }

    pub(super) fn file_dialog_set_path(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let path = request
            .file_paths
            .first()
            .map(|path| path.trim())
            .filter(|path| !path.is_empty())
            .ok_or_else(|| schema_error("filePaths"))?;
        if !Path::new(path).is_absolute() {
            return Err(schema_error("filePaths"));
        }
        macos_input::hotkey(&["command".into(), "shift".into(), "g".into()])?;
        thread::sleep(Duration::from_millis(120));
        macos_clipboard::paste_text(path)?;
        thread::sleep(Duration::from_millis(80));
        macos_input::key_press("return")
    }

    pub(super) fn file_dialog_confirm(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let labels = request
            .target_label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
            .map(|label| vec![label])
            .unwrap_or_else(|| vec!["Open", "Save", "Choose", "OK", "Done"]);
        let window = focused_window()?;
        for label in labels {
            if let Some(button) = find_descendant_by_label(&window, label) {
                return perform_action(&button, "AXPress");
            }
        }
        macos_input::key_press("return")
    }

    fn set_expanded(
        request: &DesktopSidecarControlRequest,
        expanded: bool,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let element = control_element(request)?;
        if ax_bool_attribute(&element, "AXExpanded") == Some(expanded) {
            return Ok(());
        }
        match set_bool_attribute(&element, "AXExpanded", expanded) {
            Ok(()) => Ok(()),
            Err(_) => perform_action(&element, "AXPress"),
        }
    }

    fn window_control_target(
        request: &DesktopSidecarControlRequest,
    ) -> Result<AxElement, DesktopSidecarErrorBody> {
        if let Some(window_id) = request
            .window_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            let target = resolve_window_snapshot_target(window_id)?;
            if let Some(window) = target.window {
                return Ok(window);
            }
        }
        if let (Some(x), Some(y)) = (request.x, request.y) {
            if x >= 0 && y >= 0 {
                return control_element(request);
            }
        }
        let system_wide = AxElement::system_wide().ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_accessibility_backend_unavailable",
                "Desktop sidecar could not create the macOS Accessibility system reference.",
                true,
                false,
            )
        })?;
        let app = ax_element_attribute(&system_wide, "AXFocusedApplication").ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_window_target_not_found",
                "macOS Accessibility did not return a focused application for window control. Provide windowId from window_list.",
                false,
                true,
            )
        })?;
        ax_element_attribute(&app, "AXFocusedWindow").ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_window_target_not_found",
                "macOS Accessibility did not return a focused window for window control. Provide windowId from window_list.",
                false,
                true,
            )
        })
    }

    fn focused_window() -> Result<AxElement, DesktopSidecarErrorBody> {
        if !accessibility_permission_granted() {
            return Err(DesktopSidecarErrorBody::new(
                "permission_accessibility_denied",
                "Grant Xero Accessibility permission in System Settings > Privacy & Security > Accessibility, then retry.",
                true,
                true,
            ));
        }
        let system_wide = AxElement::system_wide().ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_accessibility_backend_unavailable",
                "Desktop sidecar could not create the macOS Accessibility system reference.",
                true,
                false,
            )
        })?;
        let app = ax_element_attribute(&system_wide, "AXFocusedApplication").ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_window_target_not_found",
                "macOS Accessibility did not return a focused application.",
                false,
                true,
            )
        })?;
        ax_element_attribute(&app, "AXFocusedWindow").ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_window_target_not_found",
                "macOS Accessibility did not return a focused window.",
                false,
                true,
            )
        })
    }

    fn application_by_process_name(
        process_name: &str,
    ) -> Result<AxElement, DesktopSidecarErrorBody> {
        if !accessibility_permission_granted() {
            return Err(DesktopSidecarErrorBody::new(
                "permission_accessibility_denied",
                "Grant Xero Accessibility permission in System Settings > Privacy & Security > Accessibility, then retry.",
                true,
                true,
            ));
        }
        let output = Command::new("/usr/bin/pgrep")
            .arg("-x")
            .arg(process_name)
            .output()
            .map_err(|error| {
                DesktopSidecarErrorBody::new(
                    "desktop_process_lookup_failed",
                    format!("Could not locate macOS process `{process_name}`: {error}"),
                    true,
                    false,
                )
            })?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let pid = stdout
            .lines()
            .find_map(|line| line.trim().parse::<u32>().ok())
            .ok_or_else(|| {
                DesktopSidecarErrorBody::new(
                    "desktop_process_lookup_failed",
                    format!("macOS process `{process_name}` is not running."),
                    false,
                    true,
                )
            })?;
        AxElement::application(pid).ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_accessibility_app_unavailable",
                format!(
                    "Desktop sidecar could not create an Accessibility application reference for `{process_name}`."
                ),
                true,
                false,
            )
        })
    }

    fn has_element_or_point_target(request: &DesktopSidecarControlRequest) -> bool {
        request
            .element_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            || matches!((request.x, request.y), (Some(x), Some(y)) if x >= 0 && y >= 0)
    }

    #[derive(Clone)]
    struct AxElement(CFType);

    impl AxElement {
        fn system_wide() -> Option<Self> {
            unsafe {
                let raw = AXUIElementCreateSystemWide();
                (!raw.is_null()).then(|| Self(CFType::wrap_under_create_rule(raw as CFTypeRef)))
            }
        }

        fn application(pid: u32) -> Option<Self> {
            unsafe {
                let raw = AXUIElementCreateApplication(pid as libc::pid_t);
                (!raw.is_null()).then(|| Self(CFType::wrap_under_create_rule(raw as CFTypeRef)))
            }
        }

        fn from_cf(value: CFType) -> Option<Self> {
            (value.type_of() == ax_ui_element_type_id()).then_some(Self(value))
        }

        fn as_ref(&self) -> AXUIElementRef {
            self.0.as_CFTypeRef() as AXUIElementRef
        }
    }

    struct SnapshotTarget {
        app: AxElement,
        window: Option<AxElement>,
        pid: Option<u32>,
        window_id: Option<String>,
        app_name: Option<String>,
        window_title: Option<String>,
        focused_only: bool,
    }

    impl SnapshotTarget {
        fn windows(&self) -> Vec<AxElement> {
            if let Some(window) = &self.window {
                return vec![window.clone()];
            }
            if self.focused_only {
                if let Some(window) = ax_element_attribute(&self.app, "AXFocusedWindow") {
                    return vec![window];
                }
            }
            ax_element_array_attribute(&self.app, "AXWindows")
        }

        fn snapshot_target(&self) -> DesktopSidecarAccessibilitySnapshotTarget {
            DesktopSidecarAccessibilitySnapshotTarget {
                pid: self.pid,
                window_id: self.window_id.clone(),
                app_name: self.app_name.clone(),
                window_title: self.window_title.clone(),
            }
        }
    }

    struct SnapshotContext {
        rows: Vec<DesktopSidecarAccessibilitySnapshotRow>,
        limit: usize,
        max_depth: usize,
        include_children: bool,
        truncated: bool,
    }

    impl SnapshotContext {
        fn is_full(&self) -> bool {
            self.rows.len() >= self.limit
        }

        fn push(&mut self, row: DesktopSidecarAccessibilitySnapshotRow) {
            if self.is_full() {
                self.truncated = true;
            } else {
                self.rows.push(row);
            }
        }
    }

    #[derive(Clone, Default)]
    struct AxElementIdentityContext {
        app_name: Option<String>,
        window_title: Option<String>,
        window_bounds: Option<MacosElementBounds>,
        ancestry_path: Vec<usize>,
    }

    #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub(super) struct MacosElementBounds {
        pub(super) x: i32,
        pub(super) y: i32,
        pub(super) width: u32,
        pub(super) height: u32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub(super) struct MacosElementHandle {
        pub(super) version: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub(super) pid: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub(super) app_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub(super) window_title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub(super) window_bounds: Option<MacosElementBounds>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub(super) ancestry_path: Vec<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub(super) role: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub(super) title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub(super) description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub(super) bounds: Option<MacosElementBounds>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub(super) hit_x: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub(super) hit_y: Option<i32>,
    }

    fn resolve_snapshot_target(
        request: &DesktopSidecarAccessibilitySnapshotRequest,
    ) -> Result<SnapshotTarget, DesktopSidecarErrorBody> {
        if let Some(window_id) = request.window_id.as_deref() {
            return resolve_window_snapshot_target(window_id);
        }
        let system_wide = AxElement::system_wide().ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_accessibility_backend_unavailable",
                "Desktop sidecar could not create the macOS Accessibility system reference.",
                true,
                false,
            )
        })?;
        let Some(app) = ax_element_attribute(&system_wide, "AXFocusedApplication") else {
            return resolve_focused_window_snapshot_target();
        };
        Ok(SnapshotTarget {
            pid: element_pid(&app),
            app_name: ax_string_attribute(&app, "AXTitle"),
            window_title: None,
            window_id: None,
            focused_only: request.focused_only,
            app,
            window: None,
        })
    }

    fn resolve_focused_window_snapshot_target() -> Result<SnapshotTarget, DesktopSidecarErrorBody> {
        let windows = xcap::Window::all().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "desktop_accessibility_window_list_failed",
                format!(
                    "Desktop sidecar could not enumerate windows for Accessibility targeting: {error}"
                ),
                true,
                false,
            )
        })?;
        let focused_window = windows
            .into_iter()
            .find(|window| window.is_focused().unwrap_or(false))
            .ok_or_else(|| {
                DesktopSidecarErrorBody::new(
                    "desktop_accessibility_snapshot_target_not_found",
                    "Desktop sidecar could not resolve a focused application or focused window for Accessibility snapshot.",
                    true,
                    false,
                )
            })?;
        let pid = focused_window
            .pid()
            .ok()
            .filter(|pid| *pid > 0)
            .ok_or_else(|| {
                DesktopSidecarErrorBody::new(
                    "desktop_accessibility_window_pid_unavailable",
                    "Desktop sidecar found a focused window but could not resolve its process id.",
                    true,
                    false,
                )
            })?;
        let app = AxElement::application(pid).ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_accessibility_app_unavailable",
                format!(
                    "Desktop sidecar could not create an Accessibility application reference for PID {pid}."
                ),
                true,
                false,
            )
        })?;
        let window_title = focused_window
            .title()
            .ok()
            .map(|title| desktop_label_preview(&title));
        let app_name = focused_window
            .app_name()
            .ok()
            .map(|name| desktop_label_preview(&name));
        let target_window = find_matching_window(
            &app,
            window_title.as_deref(),
            window_bounds(&focused_window),
        );
        Ok(SnapshotTarget {
            app,
            window: target_window,
            pid: Some(pid),
            window_id: focused_window.id().ok().map(|id| id.to_string()),
            app_name,
            window_title,
            focused_only: true,
        })
    }

    fn resolve_window_snapshot_target(
        window_id: &str,
    ) -> Result<SnapshotTarget, DesktopSidecarErrorBody> {
        let requested_window_id = window_id.parse::<u32>().map_err(|_| {
            DesktopSidecarErrorBody::new(
                "sidecar_schema_invalid",
                "Accessibility snapshot windowId must be a native numeric window identifier.",
                false,
                false,
            )
        })?;
        let windows = xcap::Window::all().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "desktop_accessibility_window_list_failed",
                format!(
                    "Desktop sidecar could not enumerate windows for Accessibility targeting: {error}"
                ),
                true,
                false,
            )
        })?;
        for window in windows {
            if window.id().ok() != Some(requested_window_id) {
                continue;
            }
            let pid = window.pid().ok().filter(|pid| *pid > 0).ok_or_else(|| {
                DesktopSidecarErrorBody::new(
                    "desktop_accessibility_window_pid_unavailable",
                    format!(
                        "Desktop sidecar found window `{window_id}` but could not resolve its process id."
                    ),
                    true,
                    false,
                )
            })?;
            let app = AxElement::application(pid).ok_or_else(|| {
                DesktopSidecarErrorBody::new(
                    "desktop_accessibility_app_unavailable",
                    format!(
                        "Desktop sidecar could not create an Accessibility application reference for PID {pid}."
                    ),
                    true,
                    false,
                )
            })?;
            let window_title = window
                .title()
                .ok()
                .map(|title| desktop_label_preview(&title));
            let app_name = window
                .app_name()
                .ok()
                .map(|name| desktop_label_preview(&name));
            let target_window =
                find_matching_window(&app, window_title.as_deref(), window_bounds(&window));
            return Ok(SnapshotTarget {
                app,
                window: target_window,
                pid: Some(pid),
                window_id: Some(window_id.into()),
                app_name,
                window_title,
                focused_only: false,
            });
        }
        Err(DesktopSidecarErrorBody::new(
            "desktop_accessibility_window_not_found",
            format!(
                "Desktop sidecar could not find window `{window_id}` for Accessibility snapshot."
            ),
            false,
            true,
        ))
    }

    fn window_bounds(window: &xcap::Window) -> Option<(i32, i32, u32, u32)> {
        Some((
            window.x().ok()?,
            window.y().ok()?,
            window.width().ok()?,
            window.height().ok()?,
        ))
    }

    fn find_matching_window(
        app: &AxElement,
        title: Option<&str>,
        bounds: Option<(i32, i32, u32, u32)>,
    ) -> Option<AxElement> {
        let windows = ax_element_array_attribute(app, "AXWindows");
        windows.into_iter().find(|candidate| {
            if let Some(title) = title {
                if ax_string_attribute(candidate, "AXTitle").as_deref() == Some(title) {
                    return true;
                }
            }
            let Some((x, y, width, height)) = bounds else {
                return title.is_none();
            };
            let Some(point) = ax_point_attribute(candidate, "AXPosition") else {
                return false;
            };
            let Some(size) = ax_size_attribute(candidate, "AXSize") else {
                return false;
            };
            (point.x.round() as i32 - x).abs() <= 2
                && (point.y.round() as i32 - y).abs() <= 2
                && (size.width.max(0.0).round() as u32).abs_diff(width) <= 2
                && (size.height.max(0.0).round() as u32).abs_diff(height) <= 2
        })
    }

    fn window_identity_context(
        target: &SnapshotTarget,
        window: &AxElement,
        window_index: usize,
    ) -> AxElementIdentityContext {
        AxElementIdentityContext {
            app_name: target
                .app_name
                .clone()
                .or_else(|| ax_string_attribute(&target.app, "AXTitle")),
            window_title: ax_string_attribute(window, "AXTitle")
                .or_else(|| target.window_title.clone()),
            window_bounds: element_bounds(window),
            ancestry_path: vec![window_index],
        }
    }

    fn snapshot_children(
        context: &mut SnapshotContext,
        element: &AxElement,
        depth: usize,
        identity: &AxElementIdentityContext,
    ) {
        if !context.include_children || depth > context.max_depth || context.is_full() {
            return;
        }
        let children = ax_element_array_attribute(element, "AXChildren");
        for (index, child) in children.into_iter().enumerate() {
            if context.is_full() {
                context.truncated = true;
                break;
            }
            let mut child_identity = identity.clone();
            child_identity.ancestry_path.push(index);
            let row = snapshot_row(
                "macos_accessibility_element",
                &child,
                depth,
                Some(index),
                &child_identity,
            );
            context.push(row);
            snapshot_children(context, &child, depth + 1, &child_identity);
        }
    }

    fn snapshot_row(
        row_type: &str,
        element: &AxElement,
        depth: usize,
        child_index: Option<usize>,
        identity: &AxElementIdentityContext,
    ) -> DesktopSidecarAccessibilitySnapshotRow {
        let element = describe_element(element, 0, 0, identity);
        let state = if element.focused.unwrap_or(false) {
            Some("focused".into())
        } else {
            Some("visible".into())
        };
        DesktopSidecarAccessibilitySnapshotRow {
            row_type: row_type.into(),
            depth,
            child_index,
            state,
            element,
        }
    }

    fn element_at_position(
        system_wide: &AxElement,
        x: i32,
        y: i32,
    ) -> Result<AxElement, DesktopSidecarErrorBody> {
        let mut raw: AXUIElementRef = ptr::null();
        let status = unsafe {
            AXUIElementCopyElementAtPosition(system_wide.as_ref(), x as f64, y as f64, &mut raw)
        };
        if status != AX_ERROR_SUCCESS || raw.is_null() {
            return Err(DesktopSidecarErrorBody::new(
                "desktop_element_at_point_failed",
                format!(
                    "macOS Accessibility did not return an element at ({x}, {y}); AX status {status}."
                ),
                true,
                false,
            ));
        }
        let value = unsafe { CFType::wrap_under_create_rule(raw as CFTypeRef) };
        AxElement::from_cf(value).ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_element_at_point_failed",
                "macOS Accessibility returned an unexpected object for element-at-point.",
                true,
                false,
            )
        })
    }

    fn element_identity_context(element: &AxElement) -> AxElementIdentityContext {
        let pid = element_pid(element);
        let app_name = pid
            .and_then(AxElement::application)
            .and_then(|app| ax_string_attribute(&app, "AXTitle"));
        let window = ancestor_window(element);
        AxElementIdentityContext {
            app_name,
            window_title: window
                .as_ref()
                .and_then(|window| ax_string_attribute(window, "AXTitle")),
            window_bounds: window.as_ref().and_then(element_bounds),
            ancestry_path: Vec::new(),
        }
    }

    fn ancestor_window(element: &AxElement) -> Option<AxElement> {
        let mut current = element.clone();
        for _ in 0..16 {
            let parent = ax_element_attribute(&current, "AXParent")?;
            if ax_string_attribute(&parent, "AXRole").is_some_and(|role| {
                matches!(
                    role.as_str(),
                    "AXWindow" | "AXSheet" | "AXDialog" | "AXSystemDialog"
                )
            }) {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    fn control_element(
        request: &DesktopSidecarControlRequest,
    ) -> Result<AxElement, DesktopSidecarErrorBody> {
        if !accessibility_permission_granted() {
            return Err(DesktopSidecarErrorBody::new(
                "permission_accessibility_denied",
                "Grant Xero Accessibility permission in System Settings > Privacy & Security > Accessibility, then retry.",
                true,
                true,
            ));
        }
        let system_wide = AxElement::system_wide().ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_accessibility_backend_unavailable",
                "Desktop sidecar could not create the macOS Accessibility system reference.",
                true,
                false,
            )
        })?;
        if let (Some(x), Some(y)) = (request.x, request.y) {
            if x >= 0 && y >= 0 {
                return element_at_position(&system_wide, x, y);
            }
        }
        let Some(element_id) = request.element_id.as_deref() else {
            return Err(schema_error("elementId or x/y"));
        };
        if let Some(handle) = parse_macos_element_handle(element_id) {
            return resolve_macos_element_handle(&system_wide, &handle).or_else(|error| {
                if let (Some(x), Some(y)) = (handle.hit_x, handle.hit_y) {
                    if x >= 0 && y >= 0 {
                        return element_at_position(&system_wide, x, y);
                    }
                }
                Err(error)
            });
        }
        let (x, y) =
            parse_legacy_element_id_point(element_id).ok_or_else(|| schema_error("elementId"))?;
        element_at_position(&system_wide, x, y)
    }

    fn parse_legacy_element_id_point(element_id: &str) -> Option<(i32, i32)> {
        let mut parts = element_id.rsplit(':');
        let y = parts.next()?.parse::<i32>().ok()?;
        let x = parts.next()?.parse::<i32>().ok()?;
        (element_id.starts_with("macos_ax:") && x >= 0 && y >= 0).then_some((x, y))
    }

    pub(super) fn parse_macos_element_handle(element_id: &str) -> Option<MacosElementHandle> {
        let encoded = element_id.strip_prefix("macos_ax:v2:")?;
        let bytes = URL_SAFE_NO_PAD.decode(encoded.as_bytes()).ok()?;
        let handle = serde_json::from_slice::<MacosElementHandle>(&bytes).ok()?;
        (handle.version == 2).then_some(handle)
    }

    fn resolve_macos_element_handle(
        system_wide: &AxElement,
        handle: &MacosElementHandle,
    ) -> Result<AxElement, DesktopSidecarErrorBody> {
        let app = handle
            .pid
            .filter(|pid| *pid > 0)
            .and_then(AxElement::application)
            .or_else(|| ax_element_attribute(system_wide, "AXFocusedApplication"))
            .ok_or_else(|| {
                DesktopSidecarErrorBody::new(
                    "desktop_ax_element_not_found",
                    "macOS Accessibility could not resolve the app for the requested element. Refresh accessibility_snapshot and retry.",
                    false,
                    true,
                )
            })?;

        let windows = matching_windows_for_handle(&app, handle);
        if let Some(candidate) = resolve_handle_by_ancestry(&app, &windows, handle) {
            return Ok(candidate);
        }
        if let Some(candidate) = find_best_matching_element(&windows, handle) {
            return Ok(candidate);
        }
        if element_match_score(&app, handle) >= 5 {
            return Ok(app);
        }
        Err(DesktopSidecarErrorBody::new(
            "desktop_ax_element_not_found",
            "macOS Accessibility could not re-resolve the requested element by app, window, role, title, bounds, or ancestry path. Refresh accessibility_snapshot before retrying.",
            false,
            true,
        ))
    }

    fn matching_windows_for_handle(app: &AxElement, handle: &MacosElementHandle) -> Vec<AxElement> {
        let windows = ax_element_array_attribute(app, "AXWindows");
        if windows.is_empty() {
            return vec![app.clone()];
        }
        let mut matches = windows
            .iter()
            .filter(|window| window_matches_handle(window, handle))
            .cloned()
            .collect::<Vec<_>>();
        if matches.is_empty() {
            if let Some(index) = handle.ancestry_path.first().copied() {
                if let Some(window) = windows.get(index) {
                    matches.push(window.clone());
                }
            }
        }
        if matches.is_empty() {
            windows
        } else {
            matches
        }
    }

    fn window_matches_handle(window: &AxElement, handle: &MacosElementHandle) -> bool {
        let title_matches = handle
            .window_title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .is_some_and(|title| ax_string_attribute(window, "AXTitle").as_deref() == Some(title));
        let bounds_matches = handle.window_bounds.is_some_and(|bounds| {
            element_bounds(window).is_some_and(|actual| bounds_close(actual, bounds, 4))
        });
        title_matches || bounds_matches
    }

    fn resolve_handle_by_ancestry(
        app: &AxElement,
        windows: &[AxElement],
        handle: &MacosElementHandle,
    ) -> Option<AxElement> {
        if handle.ancestry_path.is_empty() {
            return (element_match_score(app, handle) >= 5).then(|| app.clone());
        }
        let window = windows.first().cloned().or_else(|| {
            ax_element_array_attribute(app, "AXWindows")
                .into_iter()
                .next()
        })?;
        if handle.ancestry_path.len() == 1 {
            return (element_match_score(&window, handle) >= 5).then_some(window);
        }
        let mut current = window;
        for index in handle.ancestry_path.iter().skip(1).copied() {
            let children = ax_element_array_attribute(&current, "AXChildren");
            current = children.get(index)?.clone();
        }
        (element_match_score(&current, handle) >= 5).then_some(current)
    }

    fn find_best_matching_element(
        roots: &[AxElement],
        handle: &MacosElementHandle,
    ) -> Option<AxElement> {
        let mut best: Option<(u8, AxElement)> = None;
        let mut visited = 0usize;
        for root in roots {
            search_best_matching_element(root, handle, 0, &mut visited, &mut best);
        }
        best.and_then(|(score, element)| (score >= 6).then_some(element))
    }

    fn search_best_matching_element(
        element: &AxElement,
        handle: &MacosElementHandle,
        depth: usize,
        visited: &mut usize,
        best: &mut Option<(u8, AxElement)>,
    ) {
        if depth > 14 || *visited >= 750 {
            return;
        }
        *visited += 1;
        let score = element_match_score(element, handle);
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            *best = Some((score, element.clone()));
        }
        for child in ax_element_array_attribute(element, "AXChildren") {
            search_best_matching_element(&child, handle, depth + 1, visited, best);
            if *visited >= 750 {
                break;
            }
        }
    }

    fn element_match_score(element: &AxElement, handle: &MacosElementHandle) -> u8 {
        if handle
            .pid
            .is_some_and(|pid| pid > 0 && element_pid(element).is_some_and(|actual| actual != pid))
        {
            return 0;
        }
        let mut score = 0u8;
        if handle
            .role
            .as_deref()
            .filter(|role| !role.trim().is_empty())
            .is_some_and(|role| ax_string_attribute(element, "AXRole").as_deref() == Some(role))
        {
            score += 3;
        }
        if handle
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .is_some_and(|title| ax_string_attribute(element, "AXTitle").as_deref() == Some(title))
        {
            score += 4;
        }
        if handle
            .description
            .as_deref()
            .filter(|description| !description.trim().is_empty())
            .is_some_and(|description| {
                ax_string_attribute(element, "AXDescription").as_deref() == Some(description)
            })
        {
            score += 2;
        }
        if handle.bounds.is_some_and(|bounds| {
            element_bounds(element).is_some_and(|actual| bounds_close(actual, bounds, 4))
        }) {
            score += 5;
        }
        score
    }

    fn bounds_close(
        actual: MacosElementBounds,
        expected: MacosElementBounds,
        tolerance: u32,
    ) -> bool {
        (actual.x - expected.x).unsigned_abs() <= tolerance
            && (actual.y - expected.y).unsigned_abs() <= tolerance
            && actual.width.abs_diff(expected.width) <= tolerance
            && actual.height.abs_diff(expected.height) <= tolerance
    }

    fn element_bounds(element: &AxElement) -> Option<MacosElementBounds> {
        let position = ax_point_attribute(element, "AXPosition")?;
        let size = ax_size_attribute(element, "AXSize")?;
        Some(MacosElementBounds {
            x: position.x.round() as i32,
            y: position.y.round() as i32,
            width: size.width.max(0.0).round() as u32,
            height: size.height.max(0.0).round() as u32,
        })
    }

    fn describe_element(
        element: &AxElement,
        hit_x: i32,
        hit_y: i32,
        identity: &AxElementIdentityContext,
    ) -> DesktopSidecarAccessibilityElement {
        let role = ax_string_attribute(element, "AXRole");
        let title = ax_string_attribute(element, "AXTitle");
        let value = ax_string_attribute(element, "AXValue");
        let description = ax_string_attribute(element, "AXDescription");
        let enabled = ax_bool_attribute(element, "AXEnabled");
        let focused = ax_bool_attribute(element, "AXFocused");
        let bounds = element_bounds(element);
        let x = bounds.map(|bounds| bounds.x);
        let y = bounds.map(|bounds| bounds.y);
        let width = bounds.map(|bounds| bounds.width);
        let height = bounds.map(|bounds| bounds.height);
        let pid = element_pid(element);
        let handle = MacosElementHandle {
            version: 2,
            pid,
            app_name: identity.app_name.clone(),
            window_title: identity.window_title.clone(),
            window_bounds: identity.window_bounds,
            ancestry_path: identity.ancestry_path.clone(),
            role: role.clone(),
            title: title.clone(),
            description: description.clone(),
            bounds,
            hit_x: Some(hit_x),
            hit_y: Some(hit_y),
        };
        DesktopSidecarAccessibilityElement {
            element_id: element_id(&handle),
            pid,
            app_name: identity.app_name.clone(),
            window_title: identity.window_title.clone(),
            ancestry_path: identity.ancestry_path.clone(),
            role,
            title,
            value,
            description,
            enabled,
            focused,
            x,
            y,
            width,
            height,
        }
    }

    pub(super) fn element_id(handle: &MacosElementHandle) -> String {
        match serde_json::to_vec(handle) {
            Ok(bytes) => format!("macos_ax:v2:{}", URL_SAFE_NO_PAD.encode(bytes)),
            Err(_) => "macos_ax:v2:invalid".into(),
        }
    }

    fn ax_string_attribute(element: &AxElement, attribute: &str) -> Option<String> {
        cf_value_summary(&ax_attribute(element, attribute)?)
    }

    fn ax_bool_attribute(element: &AxElement, attribute: &str) -> Option<bool> {
        let value = ax_attribute(element, attribute)?;
        value.downcast::<CFBoolean>().map(bool::from)
    }

    fn ax_attribute(element: &AxElement, attribute: &str) -> Option<CFType> {
        let attribute = CFString::new(attribute);
        let mut value: CFTypeRef = ptr::null();
        let status = unsafe {
            AXUIElementCopyAttributeValue(
                element.as_ref(),
                attribute.as_concrete_TypeRef(),
                &mut value,
            )
        };
        (status == AX_ERROR_SUCCESS && !value.is_null())
            .then(|| unsafe { CFType::wrap_under_create_rule(value) })
    }

    fn ax_element_attribute(element: &AxElement, attribute: &str) -> Option<AxElement> {
        AxElement::from_cf(ax_attribute(element, attribute)?)
    }

    fn ax_element_array_attribute(element: &AxElement, attribute: &str) -> Vec<AxElement> {
        let Some(value) = ax_attribute(element, attribute) else {
            return Vec::new();
        };
        let Some(array) = value.downcast::<CFArray>() else {
            return Vec::new();
        };
        array
            .get_all_values()
            .into_iter()
            .filter_map(|value| {
                if value.is_null() {
                    return None;
                }
                let cf_type = unsafe { CFType::wrap_under_get_rule(value as CFTypeRef) };
                AxElement::from_cf(cf_type)
            })
            .collect()
    }

    fn find_child_by_title(element: &AxElement, title: &str) -> Option<AxElement> {
        ax_element_array_attribute(element, "AXChildren")
            .into_iter()
            .find(|child| {
                ax_string_attribute(child, "AXTitle")
                    .is_some_and(|child_title| child_title.eq_ignore_ascii_case(title))
            })
    }

    fn find_descendant_by_label(element: &AxElement, label: &str) -> Option<AxElement> {
        let mut stack = vec![element.clone()];
        let mut visited = 0usize;
        while let Some(candidate) = stack.pop() {
            if visited >= 1_000 {
                break;
            }
            visited += 1;
            if element_label_matches(&candidate, label) {
                return Some(candidate);
            }
            stack.extend(accessibility_child_candidates(&candidate));
        }
        None
    }

    fn accessibility_child_candidates(element: &AxElement) -> Vec<AxElement> {
        let mut children = Vec::new();
        for attribute in ["AXChildren", "AXExtras"] {
            children.extend(ax_element_array_attribute(element, attribute));
        }
        for attribute in ["AXMenuBar", "AXMenu", "AXDefaultButton"] {
            if let Some(child) = ax_element_attribute(element, attribute) {
                children.push(child);
            }
        }
        children
    }

    fn element_label_matches(element: &AxElement, label: &str) -> bool {
        let expected = label.trim().to_ascii_lowercase();
        if expected.is_empty() {
            return false;
        }
        [
            "AXTitle",
            "AXDescription",
            "AXValue",
            "AXHelp",
            "AXIdentifier",
        ]
        .into_iter()
        .filter_map(|attribute| ax_string_attribute(element, attribute))
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .any(|value| value == expected || value.contains(&expected) || expected.contains(&value))
    }

    fn perform_action(element: &AxElement, action: &str) -> Result<(), DesktopSidecarErrorBody> {
        let action = CFString::new(action);
        let status =
            unsafe { AXUIElementPerformAction(element.as_ref(), action.as_concrete_TypeRef()) };
        if status == AX_ERROR_SUCCESS {
            Ok(())
        } else {
            Err(ax_action_error("perform Accessibility action", status))
        }
    }

    fn perform_first_action(
        element: &AxElement,
        actions: &[&str],
    ) -> Result<(), DesktopSidecarErrorBody> {
        let mut last_error = None;
        for action in actions {
            match perform_action(element, action) {
                Ok(()) => return Ok(()),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_ax_action_failed",
                "Desktop sidecar could not perform an Accessibility action because no action names were provided.",
                false,
                false,
            )
        }))
    }

    fn element_suggests_cancel(element: &AxElement) -> bool {
        ["AXTitle", "AXDescription", "AXValue"]
            .into_iter()
            .filter_map(|attribute| ax_string_attribute(element, attribute))
            .any(|value| {
                let value = value.trim().to_ascii_lowercase();
                matches!(
                    value.as_str(),
                    "cancel" | "close" | "dismiss" | "no" | "stop" | "abort"
                )
            })
    }

    fn set_bool_attribute(
        element: &AxElement,
        attribute: &str,
        value: bool,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let attribute = CFString::new(attribute);
        let value = CFBoolean::from(value);
        let status = unsafe {
            AXUIElementSetAttributeValue(
                element.as_ref(),
                attribute.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            )
        };
        if status == AX_ERROR_SUCCESS {
            Ok(())
        } else {
            Err(ax_action_error(
                format!("set Accessibility {attribute}"),
                status,
            ))
        }
    }

    fn set_string_attribute(
        element: &AxElement,
        attribute: &str,
        value: &str,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let attribute = CFString::new(attribute);
        let value = CFString::new(value);
        let status = unsafe {
            AXUIElementSetAttributeValue(
                element.as_ref(),
                attribute.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            )
        };
        if status == AX_ERROR_SUCCESS {
            Ok(())
        } else {
            Err(ax_action_error("set Accessibility value", status))
        }
    }

    fn set_point_attribute(
        element: &AxElement,
        attribute: &str,
        value: CGPoint,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let ax_value = ax_value_create(
            AX_VALUE_CGPOINT_TYPE,
            &value as *const CGPoint as *const c_void,
            "create Accessibility point value",
        )?;
        set_cf_attribute(element, attribute, &ax_value)
    }

    fn set_size_attribute(
        element: &AxElement,
        attribute: &str,
        value: CGSize,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let ax_value = ax_value_create(
            AX_VALUE_CGSIZE_TYPE,
            &value as *const CGSize as *const c_void,
            "create Accessibility size value",
        )?;
        set_cf_attribute(element, attribute, &ax_value)
    }

    fn set_text_range(
        element: &AxElement,
        start: usize,
        end: usize,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let location = CFIndex::try_from(start).map_err(|_| schema_error("selectionStart"))?;
        let length = CFIndex::try_from(end - start).map_err(|_| schema_error("selectionEnd"))?;
        let range = CFRange { location, length };
        let ax_value = ax_value_create(
            AX_VALUE_CFRANGE_TYPE,
            &range as *const CFRange as *const c_void,
            "create Accessibility text range value",
        )?;
        set_cf_attribute(element, "AXSelectedTextRange", &ax_value)
    }

    fn set_cf_attribute(
        element: &AxElement,
        attribute: &str,
        value: &CFType,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let attribute = CFString::new(attribute);
        let status = unsafe {
            AXUIElementSetAttributeValue(
                element.as_ref(),
                attribute.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
            )
        };
        if status == AX_ERROR_SUCCESS {
            Ok(())
        } else {
            Err(ax_action_error("set Accessibility attribute", status))
        }
    }

    fn ax_value_create(
        value_type: i32,
        value: *const c_void,
        action: &'static str,
    ) -> Result<CFType, DesktopSidecarErrorBody> {
        let raw = unsafe { AXValueCreate(value_type, value) };
        if raw.is_null() {
            return Err(DesktopSidecarErrorBody::new(
                "desktop_ax_action_failed",
                format!("Desktop sidecar could not {action}."),
                false,
                false,
            ));
        }
        Ok(unsafe { CFType::wrap_under_create_rule(raw as CFTypeRef) })
    }

    fn ax_action_error(action: impl std::fmt::Display, status: AXError) -> DesktopSidecarErrorBody {
        DesktopSidecarErrorBody::new(
            "desktop_ax_action_failed",
            format!("Desktop sidecar could not {action}; AX status {status}."),
            false,
            false,
        )
    }

    fn ax_point_attribute(element: &AxElement, attribute: &str) -> Option<CGPoint> {
        let value = ax_attribute(element, attribute)?;
        if value.type_of() != ax_value_type_id() {
            return None;
        }
        if unsafe { AXValueGetType(value.as_CFTypeRef() as AXValueRef) } != AX_VALUE_CGPOINT_TYPE {
            return None;
        }
        let mut point = CGPoint::default();
        let ok = unsafe {
            AXValueGetValue(
                value.as_CFTypeRef() as AXValueRef,
                AX_VALUE_CGPOINT_TYPE,
                &mut point as *mut CGPoint as *mut c_void,
            )
        };
        ok.then_some(point)
    }

    fn ax_size_attribute(element: &AxElement, attribute: &str) -> Option<CGSize> {
        let value = ax_attribute(element, attribute)?;
        if value.type_of() != ax_value_type_id() {
            return None;
        }
        if unsafe { AXValueGetType(value.as_CFTypeRef() as AXValueRef) } != AX_VALUE_CGSIZE_TYPE {
            return None;
        }
        let mut size = CGSize::default();
        let ok = unsafe {
            AXValueGetValue(
                value.as_CFTypeRef() as AXValueRef,
                AX_VALUE_CGSIZE_TYPE,
                &mut size as *mut CGSize as *mut c_void,
            )
        };
        ok.then_some(size)
    }

    fn cf_value_summary(value: &CFType) -> Option<String> {
        if let Some(value) = value.downcast::<CFString>() {
            return Some(value.to_string());
        }
        if let Some(value) = value.downcast::<CFBoolean>() {
            return Some(bool::from(value).to_string());
        }
        if let Some(value) = value.downcast::<CFNumber>() {
            if let Some(integer) = value.to_i64() {
                return Some(integer.to_string());
            }
            if let Some(float) = value.to_f64() {
                return Some(float.to_string());
            }
        }
        if value.type_of() == ax_ui_element_type_id() {
            return Some("AXUIElement".into());
        }
        if value.type_of() == ax_value_type_id() {
            return Some("AXValue".into());
        }
        None
    }

    fn element_pid(element: &AxElement) -> Option<u32> {
        let mut pid: libc::pid_t = 0;
        let status = unsafe { AXUIElementGetPid(element.as_ref(), &mut pid) };
        (status == AX_ERROR_SUCCESS && pid > 0).then_some(pid as u32)
    }

    fn accessibility_permission_granted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    fn ax_ui_element_type_id() -> CFTypeID {
        unsafe { AXUIElementGetTypeID() }
    }

    fn ax_value_type_id() -> CFTypeID {
        unsafe { AXValueGetTypeID() }
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCreateApplication(pid: libc::pid_t) -> AXUIElementRef;
        fn AXUIElementCopyElementAtPosition(
            application: AXUIElementRef,
            x: f64,
            y: f64,
            element: *mut AXUIElementRef,
        ) -> AXError;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementSetAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: CFTypeRef,
        ) -> AXError;
        fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> AXError;
        fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut libc::pid_t) -> AXError;
        fn AXUIElementGetTypeID() -> CFTypeID;
        fn AXValueCreate(value_type: i32, value: *const c_void) -> AXValueRef;
        fn AXValueGetTypeID() -> CFTypeID;
        fn AXValueGetType(value: AXValueRef) -> i32;
        fn AXValueGetValue(value: AXValueRef, value_type: i32, value: *mut c_void) -> bool;
    }
}

#[cfg(any(target_os = "windows", test))]
#[cfg_attr(all(test, not(target_os = "windows")), allow(dead_code))]
mod windows_ui_automation {
    use std::process::Command;

    use serde::de::DeserializeOwned;
    use xero_desktop_control_ipc::{
        DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarAccessibilitySnapshotRequest,
        DesktopSidecarControlRequest, DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody,
        DesktopSidecarPointRequest,
    };

    const UIA_COMMON_SCRIPT: &str = r####"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName WindowsBase

function Convert-XeroBool([string]$Value, [bool]$Default) {
  if ([string]::IsNullOrWhiteSpace($Value)) { return $Default }
  return @('1', 'true', 'yes', 'on') -contains $Value.Trim().ToLowerInvariant()
}

function Convert-XeroInt([string]$Value, [int]$Default, [int]$Minimum, [int]$Maximum) {
  if ([string]::IsNullOrWhiteSpace($Value)) { return $Default }
  $parsed = 0
  if (-not [int]::TryParse($Value, [ref]$parsed)) { return $Default }
  if ($parsed -lt $Minimum) { return $Minimum }
  if ($parsed -gt $Maximum) { return $Maximum }
  return $parsed
}

function Convert-XeroRectValue([double]$Value) {
  if ([double]::IsNaN($Value) -or [double]::IsInfinity($Value)) { return $null }
  return [int][Math]::Round($Value)
}

function Get-XeroRectParts($Element) {
  try {
    $rect = $Element.Current.BoundingRectangle
    if ($rect.IsEmpty) {
      return @{
        X = $null
        Y = $null
        Width = $null
        Height = $null
      }
    }
    return @{
      X = Convert-XeroRectValue $rect.Left
      Y = Convert-XeroRectValue $rect.Top
      Width = [Math]::Max(0, (Convert-XeroRectValue $rect.Width))
      Height = [Math]::Max(0, (Convert-XeroRectValue $rect.Height))
    }
  } catch {
    return @{
      X = $null
      Y = $null
      Width = $null
      Height = $null
    }
  }
}

function Get-XeroElementValue($Element) {
  try {
    $pattern = $null
    if ($Element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
      return $pattern.Current.Value
    }
  } catch {}
  return $null
}

function Get-XeroRole($Element) {
  try {
    $programmaticName = $Element.Current.ControlType.ProgrammaticName
    if ($programmaticName.StartsWith('ControlType.')) {
      return $programmaticName.Substring('ControlType.'.Length)
    }
    return $programmaticName
  } catch {
    return $null
  }
}

function New-XeroElementId($Element) {
  $rect = Get-XeroRectParts $Element
  $pid = 0
  $automationId = ''
  $name = ''
  $role = ''
  try { $pid = [int]$Element.Current.ProcessId } catch {}
  try { $automationId = [string]$Element.Current.AutomationId } catch {}
  try { $name = [string]$Element.Current.Name } catch {}
  try { $role = [string](Get-XeroRole $Element) } catch {}
  $seed = "$pid|$role|$automationId|$name|$($rect.X)|$($rect.Y)|$($rect.Width)|$($rect.Height)"
  $sha = [System.Security.Cryptography.SHA256]::Create()
  $hash = $sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($seed))
  $fingerprint = ([BitConverter]::ToString($hash)).Replace('-', '').Substring(0, 16).ToLowerInvariant()
  return "windows_uia:$fingerprint:$pid:$($rect.X):$($rect.Y):$($rect.Width):$($rect.Height)"
}

function Convert-XeroElement($Element) {
  if ($null -eq $Element) { return $null }
  $rect = Get-XeroRectParts $Element
  $pid = $null
  $title = $null
  $description = $null
  $enabled = $null
  $focused = $null
  try {
    $rawPid = [int]$Element.Current.ProcessId
    if ($rawPid -gt 0) { $pid = $rawPid }
  } catch {}
  try { $title = [string]$Element.Current.Name } catch {}
  try { $description = [string]$Element.Current.AutomationId } catch {}
  try { $enabled = [bool]$Element.Current.IsEnabled } catch {}
  try { $focused = [bool]$Element.Current.HasKeyboardFocus } catch {}

  return [ordered]@{
    elementId = New-XeroElementId $Element
    pid = $pid
    role = Get-XeroRole $Element
    title = $title
    value = Get-XeroElementValue $Element
    description = $description
    enabled = $enabled
    focused = $focused
    x = $rect.X
    y = $rect.Y
    width = $rect.Width
    height = $rect.Height
  }
}

function New-XeroSnapshotRow($Element, [int]$Depth, [Nullable[int]]$ChildIndex, [string]$State) {
  return [ordered]@{
    rowType = 'element'
    depth = $Depth
    childIndex = $ChildIndex
    state = $State
    element = Convert-XeroElement $Element
  }
}

function Resolve-XeroRoot([string]$WindowId, [bool]$FocusedOnly) {
  if (-not [string]::IsNullOrWhiteSpace($WindowId)) {
    $handleValue = [Int64]::Parse($WindowId, [System.Globalization.CultureInfo]::InvariantCulture)
    return [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$handleValue)
  }
  if ($FocusedOnly) {
    return [System.Windows.Automation.AutomationElement]::FocusedElement
  }
  return [System.Windows.Automation.AutomationElement]::RootElement
}

function Resolve-XeroPointElement([int]$X, [int]$Y) {
  $point = New-Object System.Windows.Point($X, $Y)
  return [System.Windows.Automation.AutomationElement]::FromPoint($point)
}

function Resolve-XeroElementFromId([string]$ElementId) {
  if ([string]::IsNullOrWhiteSpace($ElementId)) { return $null }
  if ($ElementId -match '^windows_uia:[^:]+:(?<pid>\d+):(?<x>-?\d+):(?<y>-?\d+):(?<w>\d+):(?<h>\d+)') {
    $x = [int]$Matches['x']
    $y = [int]$Matches['y']
    $w = [Math]::Max(1, [int]$Matches['w'])
    $h = [Math]::Max(1, [int]$Matches['h'])
    $candidate = Resolve-XeroPointElement ($x + [int]($w / 2)) ($y + [int]($h / 2))
    if ($null -ne $candidate -and (New-XeroElementId $candidate) -eq $ElementId) {
      return $candidate
    }
  }

  $root = [System.Windows.Automation.AutomationElement]::RootElement
  $queue = New-Object 'System.Collections.Generic.Queue[object]'
  $queue.Enqueue(@($root, 0))
  $visited = 0
  while ($queue.Count -gt 0 -and $visited -lt 2000) {
    $item = $queue.Dequeue()
    $element = $item[0]
    $depth = [int]$item[1]
    $visited += 1
    try {
      if ((New-XeroElementId $element) -eq $ElementId) { return $element }
    } catch {}
    if ($depth -ge 8) { continue }
    try {
      $children = $element.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition)
      for ($i = 0; $i -lt $children.Count; $i++) {
        $queue.Enqueue(@($children.Item($i), $depth + 1))
      }
    } catch {}
  }
  return $null
}

function Resolve-XeroTargetElement([string]$ElementId, [string]$XValue, [string]$YValue) {
  $element = Resolve-XeroElementFromId $ElementId
  if ($null -ne $element) { return $element }
  if (-not [string]::IsNullOrWhiteSpace($XValue) -and -not [string]::IsNullOrWhiteSpace($YValue)) {
    return Resolve-XeroPointElement ([int]$XValue) ([int]$YValue)
  }
  throw 'No UI Automation element target was provided. Supply elementId from accessibility_snapshot or x/y coordinates from element_at_point.'
}

function Invoke-XeroElement($Element) {
  $pattern = $null
  if ($Element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    $pattern.Invoke()
    return
  }
  if ($Element.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {
    $pattern.Select()
    return
  }
  if ($Element.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) {
    $pattern.Toggle()
    return
  }
  if ($Element.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pattern)) {
    if ($pattern.Current.ExpandCollapseState -eq [System.Windows.Automation.ExpandCollapseState]::Collapsed) {
      $pattern.Expand()
    } else {
      $pattern.Collapse()
    }
    return
  }
  throw 'Target element does not expose Invoke, SelectionItem, Toggle, or ExpandCollapse patterns.'
}

function Select-XeroElement($Element) {
  $pattern = $null
  if ($Element.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {
    $pattern.Select()
    return
  }
  if ($Element.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) {
    if ($pattern.Current.ToggleState -ne [System.Windows.Automation.ToggleState]::On) {
      $pattern.Toggle()
    }
    return
  }
  if ($Element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    $pattern.Invoke()
    return
  }
  throw 'Target element does not expose SelectionItem, Toggle, or Invoke patterns.'
}

function Confirm-XeroElement($Element) {
  $pattern = $null
  if ($Element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    $pattern.Invoke()
    return
  }
  throw 'Target element does not expose the Invoke pattern for confirm.'
}

function Test-XeroCancelLike($Element) {
  $name = ''
  $automationId = ''
  try { $name = [string]$Element.Current.Name } catch {}
  try { $automationId = [string]$Element.Current.AutomationId } catch {}
  $needle = ("$name $automationId").Trim().ToLowerInvariant()
  return $needle -match '(^|\s|-|_)(cancel|close|dismiss|no|stop|abort)(\s|-|_|$)'
}

function Cancel-XeroElement($Element) {
  $pattern = $null
  if (Test-XeroCancelLike $Element -and $Element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    $pattern.Invoke()
    return
  }
  if ($Element.TryGetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern, [ref]$pattern)) {
    $pattern.Close()
    return
  }
  throw 'Target element does not expose a safe cancel action. Target a Cancel/Close/Dismiss button or a closable window.'
}

function Step-XeroRangeValue($Element, [int]$Direction) {
  $pattern = $null
  if (-not $Element.TryGetCurrentPattern([System.Windows.Automation.RangeValuePattern]::Pattern, [ref]$pattern)) {
    throw 'Target element does not expose the RangeValue pattern.'
  }
  if ($pattern.Current.IsReadOnly) {
    throw 'Target element range value is read-only.'
  }
  $step = [double]$pattern.Current.SmallChange
  if ([double]::IsNaN($step) -or [double]::IsInfinity($step) -or $step -le 0) { $step = 1.0 }
  $next = [double]$pattern.Current.Value + ($step * $Direction)
  if ($next -lt $pattern.Current.Minimum) { $next = $pattern.Current.Minimum }
  if ($next -gt $pattern.Current.Maximum) { $next = $pattern.Current.Maximum }
  $pattern.SetValue($next)
}

function Expand-XeroElement($Element) {
  $pattern = $null
  if (-not $Element.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pattern)) {
    throw 'Target element does not expose the ExpandCollapse pattern.'
  }
  if ($pattern.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Expanded) {
    $pattern.Expand()
  }
}

function Collapse-XeroElement($Element) {
  $pattern = $null
  if (-not $Element.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$pattern)) {
    throw 'Target element does not expose the ExpandCollapse pattern.'
  }
  if ($pattern.Current.ExpandCollapseState -ne [System.Windows.Automation.ExpandCollapseState]::Collapsed) {
    $pattern.Collapse()
  }
}

function Scroll-XeroElementIntoView($Element) {
  $pattern = $null
  if ($Element.TryGetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern, [ref]$pattern)) {
    $pattern.ScrollIntoView()
    return
  }
  throw 'Target element does not expose the ScrollItem pattern.'
}

function Toggle-XeroElement($Element) {
  $pattern = $null
  if ($Element.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) {
    $pattern.Toggle()
    return
  }
  if ($Element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    $pattern.Invoke()
    return
  }
  throw 'Target element does not expose Toggle or Invoke patterns.'
}

function Set-XeroElementValue($Element, [string]$Value) {
  $pattern = $null
  if (-not $Element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
    throw 'Target element does not expose the Value pattern.'
  }
  if ($pattern.Current.IsReadOnly) {
    throw 'Target element value is read-only.'
  }
  $pattern.SetValue($Value)
}

function Get-XeroMenuCandidate($Root, [string]$Name) {
  $condition = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty,
    $Name
  )
  return $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
}
"####;

    const UIA_SNAPSHOT_SCRIPT: &str = r####"
$WindowId = $env:XERO_UIA_WINDOW_ID
$FocusedOnly = Convert-XeroBool $env:XERO_UIA_FOCUSED_ONLY $false
$IncludeChildren = Convert-XeroBool $env:XERO_UIA_INCLUDE_CHILDREN $true
$MaxDepth = Convert-XeroInt $env:XERO_UIA_MAX_DEPTH 4 0 16
$Limit = Convert-XeroInt $env:XERO_UIA_LIMIT 200 1 1000
$Root = Resolve-XeroRoot $WindowId $FocusedOnly
$Rows = New-Object 'System.Collections.Generic.List[object]'
$Diagnostics = New-Object 'System.Collections.Generic.List[string]'
$script:Truncated = $false

function Walk-XeroTree($Element, [int]$Depth, [Nullable[int]]$ChildIndex) {
  if ($Rows.Count -ge $Limit) {
    $script:Truncated = $true
    return
  }
  $Rows.Add((New-XeroSnapshotRow $Element $Depth $ChildIndex $null))
  if (-not $IncludeChildren -or $Depth -ge $MaxDepth) { return }
  try {
    $children = $Element.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition)
    for ($i = 0; $i -lt $children.Count; $i++) {
      Walk-XeroTree $children.Item($i) ($Depth + 1) $i
      if ($script:Truncated) { return }
    }
  } catch {
    $Diagnostics.Add("uia_children_unavailable: $($_.Exception.Message)")
  }
}

Walk-XeroTree $Root 0 $null
$TargetElement = Convert-XeroElement $Root
$Target = $null
if ($null -ne $TargetElement) {
  $Target = [ordered]@{
    pid = $TargetElement.pid
    windowId = $WindowId
    appName = $null
    windowTitle = $TargetElement.title
  }
}

([ordered]@{
  performed = $true
  target = $Target
  rows = @($Rows)
  truncated = [bool]$script:Truncated
  diagnostics = @($Diagnostics)
}) | ConvertTo-Json -Depth 16 -Compress
"####;

    const UIA_ELEMENT_AT_POINT_SCRIPT: &str = r####"
$X = [int]$env:XERO_UIA_X
$Y = [int]$env:XERO_UIA_Y
$Element = Resolve-XeroPointElement $X $Y
([ordered]@{
  x = $X
  y = $Y
  available = $null -ne $Element
  element = Convert-XeroElement $Element
}) | ConvertTo-Json -Depth 12 -Compress
"####;

    const UIA_PRESS_SCRIPT: &str = r####"
$Element = Resolve-XeroTargetElement $env:XERO_UIA_ELEMENT_ID $env:XERO_UIA_X $env:XERO_UIA_Y
Invoke-XeroElement $Element
"####;

    const UIA_SET_VALUE_SCRIPT: &str = r####"
$Element = Resolve-XeroTargetElement $env:XERO_UIA_ELEMENT_ID $env:XERO_UIA_X $env:XERO_UIA_Y
Set-XeroElementValue $Element ([string]$env:XERO_UIA_VALUE)
"####;

    const UIA_FOCUS_SCRIPT: &str = r####"
$Element = Resolve-XeroTargetElement $env:XERO_UIA_ELEMENT_ID $env:XERO_UIA_X $env:XERO_UIA_Y
$Element.SetFocus()
"####;

    const UIA_SELECT_SCRIPT: &str = r####"
$Element = Resolve-XeroTargetElement $env:XERO_UIA_ELEMENT_ID $env:XERO_UIA_X $env:XERO_UIA_Y
Select-XeroElement $Element
"####;

    const UIA_CONFIRM_SCRIPT: &str = r####"
$Element = Resolve-XeroTargetElement $env:XERO_UIA_ELEMENT_ID $env:XERO_UIA_X $env:XERO_UIA_Y
Confirm-XeroElement $Element
"####;

    const UIA_CANCEL_SCRIPT: &str = r####"
$Element = Resolve-XeroTargetElement $env:XERO_UIA_ELEMENT_ID $env:XERO_UIA_X $env:XERO_UIA_Y
Cancel-XeroElement $Element
"####;

    const UIA_INCREMENT_SCRIPT: &str = r####"
$Element = Resolve-XeroTargetElement $env:XERO_UIA_ELEMENT_ID $env:XERO_UIA_X $env:XERO_UIA_Y
Step-XeroRangeValue $Element 1
"####;

    const UIA_DECREMENT_SCRIPT: &str = r####"
$Element = Resolve-XeroTargetElement $env:XERO_UIA_ELEMENT_ID $env:XERO_UIA_X $env:XERO_UIA_Y
Step-XeroRangeValue $Element -1
"####;

    const UIA_EXPAND_SCRIPT: &str = r####"
$Element = Resolve-XeroTargetElement $env:XERO_UIA_ELEMENT_ID $env:XERO_UIA_X $env:XERO_UIA_Y
Expand-XeroElement $Element
"####;

    const UIA_COLLAPSE_SCRIPT: &str = r####"
$Element = Resolve-XeroTargetElement $env:XERO_UIA_ELEMENT_ID $env:XERO_UIA_X $env:XERO_UIA_Y
Collapse-XeroElement $Element
"####;

    const UIA_SCROLL_TO_VISIBLE_SCRIPT: &str = r####"
$Element = Resolve-XeroTargetElement $env:XERO_UIA_ELEMENT_ID $env:XERO_UIA_X $env:XERO_UIA_Y
Scroll-XeroElementIntoView $Element
"####;

    const UIA_TOGGLE_SCRIPT: &str = r####"
$Element = Resolve-XeroTargetElement $env:XERO_UIA_ELEMENT_ID $env:XERO_UIA_X $env:XERO_UIA_Y
Toggle-XeroElement $Element
"####;

    const UIA_MENU_SELECT_SCRIPT: &str = r####"
$MenuPath = @()
if (-not [string]::IsNullOrWhiteSpace($env:XERO_UIA_MENU_PATH_JSON)) {
  $MenuPath = @(ConvertFrom-Json $env:XERO_UIA_MENU_PATH_JSON)
}
if ($MenuPath.Count -eq 0) {
  throw 'menuPath is required for menu_select.'
}
$MenuMnemonics = @()
if (-not [string]::IsNullOrWhiteSpace($env:XERO_UIA_MENU_MNEMONICS_JSON)) {
  $MenuMnemonics = @(ConvertFrom-Json $env:XERO_UIA_MENU_MNEMONICS_JSON)
}

function Invoke-XeroMenuPathWithUia($Path, [string]$WindowId) {
  $Root = Resolve-XeroRoot $WindowId $false
  foreach ($Segment in $Path) {
    $Name = [string]$Segment
    $Candidate = Get-XeroMenuCandidate $Root $Name
    if ($null -eq $Candidate) {
      $Root = [System.Windows.Automation.AutomationElement]::RootElement
      $Candidate = Get-XeroMenuCandidate $Root $Name
    }
    if ($null -eq $Candidate) {
      throw "Could not find menu item '$Name'."
    }
    Invoke-XeroElement $Candidate
    Start-Sleep -Milliseconds 150
    $Root = [System.Windows.Automation.AutomationElement]::RootElement
  }
}

function Invoke-XeroMenuPathWithAltFallback($Mnemonics, [string]$WindowId, [string]$OriginalError) {
  if ($Mnemonics.Count -eq 0) {
    throw "Windows UI Automation menu traversal failed and no safe Alt-key mnemonic fallback is available: $OriginalError"
  }
  try {
    if (-not [string]::IsNullOrWhiteSpace($WindowId)) {
      $Target = Resolve-XeroRoot $WindowId $false
      $Target.SetFocus()
      Start-Sleep -Milliseconds 80
    }
  } catch {}
  $Shell = New-Object -ComObject WScript.Shell
  $Shell.SendKeys('%')
  Start-Sleep -Milliseconds 120
  foreach ($Mnemonic in $Mnemonics) {
    $Key = [string]$Mnemonic
    if ($Key.Length -ne 1 -or -not ($Key -match '^[A-Za-z0-9]$')) {
      throw "Windows UI Automation menu traversal failed and Alt-key fallback received an unsafe mnemonic '$Key'."
    }
    $Shell.SendKeys($Key)
    Start-Sleep -Milliseconds 120
  }
}

try {
  Invoke-XeroMenuPathWithUia $MenuPath $env:XERO_UIA_WINDOW_ID
} catch {
  Invoke-XeroMenuPathWithAltFallback $MenuMnemonics $env:XERO_UIA_WINDOW_ID $_.Exception.Message
}
"####;

    pub(super) fn snapshot(
        request: DesktopSidecarAccessibilitySnapshotRequest,
    ) -> Result<DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarErrorBody> {
        run_powershell_json(
            &format!("{UIA_COMMON_SCRIPT}\n{UIA_SNAPSHOT_SCRIPT}"),
            &[
                (
                    "XERO_UIA_WINDOW_ID",
                    request.window_id.as_deref().unwrap_or_default(),
                ),
                (
                    "XERO_UIA_FOCUSED_ONLY",
                    if request.focused_only {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "XERO_UIA_INCLUDE_CHILDREN",
                    if request.include_children {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "XERO_UIA_MAX_DEPTH",
                    &request.max_depth.unwrap_or(4).to_string(),
                ),
                ("XERO_UIA_LIMIT", &request.limit.unwrap_or(200).to_string()),
            ],
            "desktop_windows_uia_snapshot_failed",
            "Windows UI Automation snapshot failed",
        )
    }

    pub(super) fn element_at_point(
        request: DesktopSidecarPointRequest,
    ) -> Result<DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody> {
        run_powershell_json(
            &format!("{UIA_COMMON_SCRIPT}\n{UIA_ELEMENT_AT_POINT_SCRIPT}"),
            &[
                ("XERO_UIA_X", &request.x.to_string()),
                ("XERO_UIA_Y", &request.y.to_string()),
            ],
            "desktop_windows_uia_hit_test_failed",
            "Windows UI Automation hit testing failed",
        )
    }

    pub(super) fn press(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_control_script(
            UIA_PRESS_SCRIPT,
            request,
            "desktop_windows_uia_press_failed",
            "Windows UI Automation could not press the target element",
        )
    }

    pub(super) fn set_value(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_control_script(
            UIA_SET_VALUE_SCRIPT,
            request,
            "desktop_windows_uia_set_value_failed",
            "Windows UI Automation could not set the target value",
        )
    }

    pub(super) fn focus(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_control_script(
            UIA_FOCUS_SCRIPT,
            request,
            "desktop_windows_uia_focus_failed",
            "Windows UI Automation could not focus the target element",
        )
    }

    pub(super) fn select(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_control_script(
            UIA_SELECT_SCRIPT,
            request,
            "desktop_windows_uia_select_failed",
            "Windows UI Automation could not select the target element",
        )
    }

    pub(super) fn confirm(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_control_script(
            UIA_CONFIRM_SCRIPT,
            request,
            "desktop_windows_uia_confirm_failed",
            "Windows UI Automation could not confirm the target element",
        )
    }

    pub(super) fn cancel(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_control_script(
            UIA_CANCEL_SCRIPT,
            request,
            "desktop_windows_uia_cancel_failed",
            "Windows UI Automation could not cancel the target element",
        )
    }

    pub(super) fn increment(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_control_script(
            UIA_INCREMENT_SCRIPT,
            request,
            "desktop_windows_uia_increment_failed",
            "Windows UI Automation could not increment the target element",
        )
    }

    pub(super) fn decrement(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_control_script(
            UIA_DECREMENT_SCRIPT,
            request,
            "desktop_windows_uia_decrement_failed",
            "Windows UI Automation could not decrement the target element",
        )
    }

    pub(super) fn expand(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_control_script(
            UIA_EXPAND_SCRIPT,
            request,
            "desktop_windows_uia_expand_failed",
            "Windows UI Automation could not expand the target element",
        )
    }

    pub(super) fn collapse(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_control_script(
            UIA_COLLAPSE_SCRIPT,
            request,
            "desktop_windows_uia_collapse_failed",
            "Windows UI Automation could not collapse the target element",
        )
    }

    pub(super) fn scroll_to_visible(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_control_script(
            UIA_SCROLL_TO_VISIBLE_SCRIPT,
            request,
            "desktop_windows_uia_scroll_to_visible_failed",
            "Windows UI Automation could not scroll the target element into view",
        )
    }

    pub(super) fn toggle(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_control_script(
            UIA_TOGGLE_SCRIPT,
            request,
            "desktop_windows_uia_toggle_failed",
            "Windows UI Automation could not toggle the target element",
        )
    }

    pub(super) fn menu_select(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let menu_path_json = serde_json::to_string(&request.menu_path).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "desktop_windows_uia_menu_path_encode_failed",
                format!("Windows UI Automation could not encode menuPath: {error}"),
                false,
                false,
            )
        })?;
        let menu_mnemonics = crate::windows_menu_alt_mnemonics(&request.menu_path);
        let menu_mnemonics_json = serde_json::to_string(&menu_mnemonics).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "desktop_windows_uia_menu_path_encode_failed",
                format!("Windows UI Automation could not encode menuPath mnemonics: {error}"),
                false,
                false,
            )
        })?;
        run_powershell_unit(
            &format!("{UIA_COMMON_SCRIPT}\n{UIA_MENU_SELECT_SCRIPT}"),
            &[
                (
                    "XERO_UIA_WINDOW_ID",
                    request.window_id.as_deref().unwrap_or_default(),
                ),
                ("XERO_UIA_MENU_PATH_JSON", menu_path_json.as_str()),
                ("XERO_UIA_MENU_MNEMONICS_JSON", menu_mnemonics_json.as_str()),
            ],
            "desktop_windows_uia_menu_select_failed",
            "Windows UI Automation could not select the requested menu path",
        )
    }

    fn run_control_script(
        action_script: &str,
        request: &DesktopSidecarControlRequest,
        code: &'static str,
        context: &'static str,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_powershell_unit(
            &format!("{UIA_COMMON_SCRIPT}\n{action_script}"),
            &[
                (
                    "XERO_UIA_ELEMENT_ID",
                    request.element_id.as_deref().unwrap_or_default(),
                ),
                (
                    "XERO_UIA_X",
                    &request.x.map(|value| value.to_string()).unwrap_or_default(),
                ),
                (
                    "XERO_UIA_Y",
                    &request.y.map(|value| value.to_string()).unwrap_or_default(),
                ),
                (
                    "XERO_UIA_VALUE",
                    request.value.as_deref().unwrap_or_default(),
                ),
            ],
            code,
            context,
        )
    }

    fn run_powershell_json<T: DeserializeOwned>(
        script: &str,
        envs: &[(&str, &str)],
        code: &'static str,
        context: &'static str,
    ) -> Result<T, DesktopSidecarErrorBody> {
        let output = run_powershell(script, envs, code, context)?;
        serde_json::from_slice::<T>(&output).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "desktop_windows_uia_json_decode_failed",
                format!("{context}: could not decode UI Automation JSON: {error}"),
                true,
                false,
            )
        })
    }

    fn run_powershell_unit(
        script: &str,
        envs: &[(&str, &str)],
        code: &'static str,
        context: &'static str,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_powershell(script, envs, code, context).map(|_| ())
    }

    fn run_powershell(
        script: &str,
        envs: &[(&str, &str)],
        code: &'static str,
        context: &'static str,
    ) -> Result<Vec<u8>, DesktopSidecarErrorBody> {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ]);
        for (key, value) in envs {
            command.env(key, value);
        }
        let output = command.output().map_err(|error| {
            DesktopSidecarErrorBody::new(
                code,
                format!("{context}: PowerShell was unavailable: {error}"),
                true,
                true,
            )
        })?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(DesktopSidecarErrorBody::new(
                code,
                format!(
                    "{context}: {}",
                    command_output_message(&output.stdout, &output.stderr)
                ),
                true,
                true,
            ))
        }
    }

    fn command_output_message(stdout: &[u8], stderr: &[u8]) -> String {
        let stderr = String::from_utf8_lossy(stderr).trim().to_string();
        if !stderr.is_empty() {
            return stderr;
        }
        let stdout = String::from_utf8_lossy(stdout).trim().to_string();
        if stdout.is_empty() {
            "command exited unsuccessfully without diagnostics".into()
        } else {
            stdout
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_app_control {
    use std::{collections::BTreeSet, process::Command};

    use xero_desktop_control_ipc::{
        DesktopSidecarControlRequest, DesktopSidecarErrorBody, DesktopSidecarWindow,
    };

    use super::sidecar_window_rows;

    const FOCUS_WINDOW_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$WindowId = $env:XERO_DESKTOP_WINDOW_ID
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class XeroDesktopWin32 {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
}
'@
$handleValue = [Int64]::Parse($WindowId, [System.Globalization.CultureInfo]::InvariantCulture)
$hwnd = [IntPtr]$handleValue
if ([XeroDesktopWin32]::IsIconic($hwnd)) {
  [void][XeroDesktopWin32]::ShowWindowAsync($hwnd, 9)
}
[void][XeroDesktopWin32]::ShowWindowAsync($hwnd, 5)
if (-not [XeroDesktopWin32]::SetForegroundWindow($hwnd)) {
  throw 'SetForegroundWindow returned false.'
}
"#;

    const WINDOW_LAYOUT_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$WindowId = $env:XERO_DESKTOP_WINDOW_ID
$Action = $env:XERO_DESKTOP_WINDOW_ACTION
$XValue = $env:XERO_DESKTOP_WINDOW_X
$YValue = $env:XERO_DESKTOP_WINDOW_Y
$WidthValue = $env:XERO_DESKTOP_WINDOW_WIDTH
$HeightValue = $env:XERO_DESKTOP_WINDOW_HEIGHT
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class XeroDesktopWindowLayout {
  [StructLayout(LayoutKind.Sequential)]
  public struct RECT {
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
  }
  [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int nWidth, int nHeight, bool bRepaint);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, UInt32 Msg, IntPtr wParam, IntPtr lParam);
}
'@
function Convert-XeroOptionalInt([string]$Value) {
  if ([string]::IsNullOrWhiteSpace($Value)) { return $null }
  return [int]::Parse($Value, [System.Globalization.CultureInfo]::InvariantCulture)
}
$handleValue = [Int64]::Parse($WindowId, [System.Globalization.CultureInfo]::InvariantCulture)
$hwnd = [IntPtr]$handleValue
switch ($Action) {
  'maximize' {
    if (-not [XeroDesktopWindowLayout]::ShowWindowAsync($hwnd, 3)) { throw 'ShowWindowAsync(SW_MAXIMIZE) returned false.' }
    return
  }
  'minimize' {
    if (-not [XeroDesktopWindowLayout]::ShowWindowAsync($hwnd, 6)) { throw 'ShowWindowAsync(SW_MINIMIZE) returned false.' }
    return
  }
  'restore' {
    if (-not [XeroDesktopWindowLayout]::ShowWindowAsync($hwnd, 9)) { throw 'ShowWindowAsync(SW_RESTORE) returned false.' }
    return
  }
  'close' {
    if (-not [XeroDesktopWindowLayout]::PostMessageW($hwnd, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'PostMessage(WM_CLOSE) returned false.' }
    return
  }
  'move_resize' {
    $rect = New-Object XeroDesktopWindowLayout+RECT
    if (-not [XeroDesktopWindowLayout]::GetWindowRect($hwnd, [ref]$rect)) { throw 'GetWindowRect returned false.' }
    $x = Convert-XeroOptionalInt $XValue
    $y = Convert-XeroOptionalInt $YValue
    $width = Convert-XeroOptionalInt $WidthValue
    $height = Convert-XeroOptionalInt $HeightValue
    if ($null -eq $x) { $x = $rect.Left }
    if ($null -eq $y) { $y = $rect.Top }
    if ($null -eq $width) { $width = [Math]::Max(1, $rect.Right - $rect.Left) }
    if ($null -eq $height) { $height = [Math]::Max(1, $rect.Bottom - $rect.Top) }
    if ($width -le 0 -or $height -le 0) { throw 'Window width and height must be positive.' }
    if (-not [XeroDesktopWindowLayout]::MoveWindow($hwnd, $x, $y, $width, $height, $true)) { throw 'MoveWindow returned false.' }
    return
  }
}
throw "Unknown window layout action '$Action'."
"#;

    const LAUNCH_APP_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$AppName = $env:XERO_DESKTOP_APP_NAME
$BundleId = $env:XERO_DESKTOP_BUNDLE_ID
function Open-AppsFolderItem([string]$Needle) {
  if ([string]::IsNullOrWhiteSpace($Needle)) { return $false }
  $shell = New-Object -ComObject Shell.Application
  $folder = $shell.Namespace('shell:AppsFolder')
  if ($null -eq $folder) { return $false }
  foreach ($item in $folder.Items()) {
    if ($item.Name -eq $Needle -or $item.Path -eq $Needle) {
      $item.InvokeVerb('open')
      return $true
    }
  }
  foreach ($item in $folder.Items()) {
    if ($item.Name.IndexOf($Needle, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
      $item.InvokeVerb('open')
      return $true
    }
  }
  return $false
}
if (Open-AppsFolderItem $BundleId) { return }
if (Open-AppsFolderItem $AppName) { return }
if (-not [string]::IsNullOrWhiteSpace($BundleId)) {
  Start-Process -FilePath explorer.exe -ArgumentList ("shell:AppsFolder\" + $BundleId)
  return
}
if (-not [string]::IsNullOrWhiteSpace($AppName)) {
  Start-Process -FilePath $AppName
  return
}
throw 'No Windows app target was provided.'
"#;

    pub(super) fn focus_window(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let window = resolve_window_target(request)?;
        focus_window_id(&window.window_id)
    }

    pub(super) fn window_maximize(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_window_layout_action(request, "maximize")
    }

    pub(super) fn window_minimize(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_window_layout_action(request, "minimize")
    }

    pub(super) fn window_restore(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_window_layout_action(request, "restore")
    }

    pub(super) fn window_move_resize(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_window_layout_action(request, "move_resize")
    }

    pub(super) fn window_close(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        run_window_layout_action(request, "close")
    }

    pub(super) fn activate_app(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let window = resolve_window_target(request)?;
        focus_window_id(&window.window_id)
    }

    pub(super) fn launch_app(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        if let Ok(window) = resolve_window_target(request) {
            return focus_window_id(&window.window_id);
        }

        run_powershell(
            LAUNCH_APP_SCRIPT,
            &[
                (
                    "XERO_DESKTOP_APP_NAME",
                    request.app_name.as_deref().unwrap_or_default(),
                ),
                (
                    "XERO_DESKTOP_BUNDLE_ID",
                    request.bundle_id.as_deref().unwrap_or_default(),
                ),
            ],
            "desktop_app_launch_failed",
            "Windows refused to launch the requested app",
        )
    }

    pub(super) fn quit_app(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let pids = resolve_target_pids(request)?;
        let mut errors = Vec::new();
        for pid in pids {
            let pid_arg = pid.to_string();
            let output = Command::new("taskkill.exe")
                .args(["/PID", pid_arg.as_str()])
                .output()
                .map_err(|error| {
                    app_control_error(
                        "desktop_app_quit_failed",
                        format!("Windows could not invoke taskkill for PID {pid}: {error}"),
                        true,
                        true,
                    )
                })?;
            if !output.status.success() {
                errors.push(format!(
                    "PID {pid}: {}",
                    command_output_message(&output.stdout, &output.stderr)
                ));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(app_control_error(
                "desktop_app_quit_failed",
                format!(
                    "Windows refused to quit one or more target process(es): {}",
                    errors.join("; ")
                ),
                true,
                true,
            ))
        }
    }

    fn focus_window_id(window_id: &str) -> Result<(), DesktopSidecarErrorBody> {
        let window_id = parse_window_id(window_id)
            .ok_or_else(|| {
                app_control_error(
                    "desktop_window_target_invalid",
                    format!(
                        "Windows desktop window id `{}` is not numeric.",
                        window_id.trim()
                    ),
                    false,
                    true,
                )
            })?
            .to_string();
        run_powershell(
            FOCUS_WINDOW_SCRIPT,
            &[("XERO_DESKTOP_WINDOW_ID", window_id.as_str())],
            "desktop_window_focus_failed",
            "Windows refused to focus the requested window",
        )
    }

    fn run_window_layout_action(
        request: &DesktopSidecarControlRequest,
        action: &'static str,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let window = resolve_window_target(request)?;
        let window_id = parse_window_id(&window.window_id)
            .ok_or_else(|| {
                app_control_error(
                    "desktop_window_target_invalid",
                    format!(
                        "Windows desktop window id `{}` is not numeric.",
                        window.window_id.trim()
                    ),
                    false,
                    true,
                )
            })?
            .to_string();
        run_powershell(
            WINDOW_LAYOUT_SCRIPT,
            &[
                ("XERO_DESKTOP_WINDOW_ID", window_id.as_str()),
                ("XERO_DESKTOP_WINDOW_ACTION", action),
                (
                    "XERO_DESKTOP_WINDOW_X",
                    &request.x.map(|value| value.to_string()).unwrap_or_default(),
                ),
                (
                    "XERO_DESKTOP_WINDOW_Y",
                    &request.y.map(|value| value.to_string()).unwrap_or_default(),
                ),
                (
                    "XERO_DESKTOP_WINDOW_WIDTH",
                    &request
                        .width
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                ),
                (
                    "XERO_DESKTOP_WINDOW_HEIGHT",
                    &request
                        .height
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                ),
            ],
            "desktop_window_layout_failed",
            "Windows refused to update the requested window layout",
        )
    }

    fn resolve_target_pids(
        request: &DesktopSidecarControlRequest,
    ) -> Result<BTreeSet<u32>, DesktopSidecarErrorBody> {
        let windows = sidecar_window_rows().map_err(|error| {
            app_control_error(
                "desktop_app_target_resolution_failed",
                format!("Windows window enumeration failed: {}", error.message),
                true,
                error.user_action_required,
            )
        })?;
        let mut pids = BTreeSet::new();
        if request
            .window_id
            .as_deref()
            .is_some_and(|window_id| !window_id.trim().is_empty())
        {
            let window = find_window_by_id(&windows, request.window_id.as_deref().unwrap())?;
            if window.pid > 0 {
                pids.insert(window.pid);
            }
        } else {
            if let Some(app_name) = request.app_name.as_deref() {
                if !app_name.trim().is_empty() {
                    for window in windows
                        .iter()
                        .filter(|window| app_name_matches(window, app_name))
                    {
                        if window.pid > 0 {
                            pids.insert(window.pid);
                        }
                    }
                }
            }
            if pids.is_empty() {
                if let Some(bundle_id) = request.bundle_id.as_deref() {
                    if !bundle_id.trim().is_empty() {
                        for window in windows
                            .iter()
                            .filter(|window| app_name_matches(window, bundle_id))
                        {
                            if window.pid > 0 {
                                pids.insert(window.pid);
                            }
                        }
                    }
                }
            }
        }

        if pids.is_empty() {
            Err(app_control_error(
                "desktop_app_target_not_found",
                "Windows could not find a running app/window matching the requested target.",
                false,
                true,
            ))
        } else {
            Ok(pids)
        }
    }

    fn resolve_window_target(
        request: &DesktopSidecarControlRequest,
    ) -> Result<DesktopSidecarWindow, DesktopSidecarErrorBody> {
        let windows = sidecar_window_rows().map_err(|error| {
            app_control_error(
                "desktop_window_target_resolution_failed",
                format!("Windows window enumeration failed: {}", error.message),
                true,
                error.user_action_required,
            )
        })?;
        if let Some(window_id) = request.window_id.as_deref() {
            if !window_id.trim().is_empty() {
                return find_window_by_id(&windows, window_id);
            }
        }
        if let Some(app_name) = request.app_name.as_deref() {
            if let Some(window) = find_window_by_app_name(&windows, app_name) {
                return Ok(window.clone());
            }
        }
        if let Some(bundle_id) = request.bundle_id.as_deref() {
            if let Some(window) = find_window_by_app_name(&windows, bundle_id) {
                return Ok(window.clone());
            }
        }
        Err(app_control_error(
            "desktop_window_target_not_found",
            "Windows could not find a visible window matching the requested target.",
            false,
            true,
        ))
    }

    fn find_window_by_id(
        windows: &[DesktopSidecarWindow],
        window_id: &str,
    ) -> Result<DesktopSidecarWindow, DesktopSidecarErrorBody> {
        windows
            .iter()
            .find(|window| window_id_matches(&window.window_id, window_id))
            .cloned()
            .ok_or_else(|| {
                app_control_error(
                    "desktop_window_target_not_found",
                    format!(
                        "Windows could not find desktop window `{}`.",
                        window_id.trim()
                    ),
                    false,
                    true,
                )
            })
    }

    fn find_window_by_app_name<'a>(
        windows: &'a [DesktopSidecarWindow],
        app_name: &str,
    ) -> Option<&'a DesktopSidecarWindow> {
        windows
            .iter()
            .find(|window| app_name_matches_exact(window, app_name))
            .or_else(|| {
                windows
                    .iter()
                    .find(|window| app_name_matches(window, app_name))
            })
    }

    fn window_id_matches(actual: &str, requested: &str) -> bool {
        let actual = actual.trim();
        let requested = requested.trim();
        if actual.eq_ignore_ascii_case(requested) {
            return true;
        }
        parse_window_id(actual).is_some_and(|actual| {
            parse_window_id(requested).is_some_and(|requested| requested == actual)
        })
    }

    fn parse_window_id(value: &str) -> Option<u64> {
        let value = value.trim();
        if let Some(hex) = value
            .strip_prefix("0x")
            .or_else(|| value.strip_prefix("0X"))
        {
            u64::from_str_radix(hex, 16).ok()
        } else {
            value.parse::<u64>().ok()
        }
    }

    fn app_name_matches_exact(window: &DesktopSidecarWindow, requested: &str) -> bool {
        let requested = requested.trim();
        !requested.is_empty() && window.app_name.trim().eq_ignore_ascii_case(requested)
    }

    fn app_name_matches(window: &DesktopSidecarWindow, requested: &str) -> bool {
        let requested = requested.trim().to_ascii_lowercase();
        if requested.is_empty() {
            return false;
        }
        let app_name = window.app_name.trim().to_ascii_lowercase();
        app_name == requested || app_name.contains(&requested)
    }

    fn run_powershell(
        script: &str,
        envs: &[(&str, &str)],
        code: &'static str,
        context: &'static str,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ]);
        for (key, value) in envs {
            command.env(key, value);
        }
        let output = command.output().map_err(|error| {
            app_control_error(
                code,
                format!("{context}: PowerShell was unavailable: {error}"),
                true,
                true,
            )
        })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(app_control_error(
                code,
                format!(
                    "{context}: {}",
                    command_output_message(&output.stdout, &output.stderr)
                ),
                true,
                true,
            ))
        }
    }

    fn command_output_message(stdout: &[u8], stderr: &[u8]) -> String {
        let stderr = String::from_utf8_lossy(stderr).trim().to_string();
        if !stderr.is_empty() {
            return stderr;
        }
        let stdout = String::from_utf8_lossy(stdout).trim().to_string();
        if stdout.is_empty() {
            "command exited unsuccessfully without diagnostics".into()
        } else {
            stdout
        }
    }

    fn app_control_error(
        code: &'static str,
        message: impl Into<String>,
        retryable: bool,
        user_action_required: bool,
    ) -> DesktopSidecarErrorBody {
        DesktopSidecarErrorBody::new(code, message, retryable, user_action_required)
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
mod cross_platform_input {
    use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
    use xero_desktop_control_ipc::DesktopSidecarMouseButton;

    use super::DesktopSidecarErrorBody;

    pub(super) fn new_enigo() -> Result<Enigo, DesktopSidecarErrorBody> {
        Enigo::new(&Settings::default()).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "permission_accessibility_denied",
                format!("Could not initialize desktop input backend: {error}"),
                false,
                true,
            )
        })
    }

    pub(super) fn cursor_location(enigo: &Enigo) -> Result<(i32, i32), DesktopSidecarErrorBody> {
        enigo.location().map_err(input_error)
    }

    pub(super) fn mouse_move(point: (i32, i32)) -> Result<(), DesktopSidecarErrorBody> {
        let mut enigo = new_enigo()?;
        enigo
            .move_mouse(point.0, point.1, Coordinate::Abs)
            .map_err(input_error)
    }

    pub(super) fn mouse_down(
        point: (i32, i32),
        button: DesktopSidecarMouseButton,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let mut enigo = new_enigo()?;
        enigo
            .move_mouse(point.0, point.1, Coordinate::Abs)
            .map_err(input_error)?;
        enigo
            .button(mouse_button(button), Direction::Press)
            .map_err(input_error)
    }

    pub(super) fn mouse_click(
        point: (i32, i32),
        button: DesktopSidecarMouseButton,
        clicks: u8,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let mut enigo = new_enigo()?;
        enigo
            .move_mouse(point.0, point.1, Coordinate::Abs)
            .map_err(input_error)?;
        let button = mouse_button(button);
        for _ in 0..clicks {
            enigo
                .button(button, Direction::Click)
                .map_err(input_error)?;
        }
        Ok(())
    }

    pub(super) fn mouse_drag(
        from: (i32, i32),
        to: (i32, i32),
    ) -> Result<(), DesktopSidecarErrorBody> {
        let mut enigo = new_enigo()?;
        enigo
            .move_mouse(from.0, from.1, Coordinate::Abs)
            .map_err(input_error)?;
        enigo
            .button(Button::Left, Direction::Press)
            .map_err(input_error)?;
        enigo
            .move_mouse(to.0, to.1, Coordinate::Abs)
            .map_err(input_error)?;
        enigo
            .button(Button::Left, Direction::Release)
            .map_err(input_error)
    }

    pub(super) fn mouse_drag_move(point: (i32, i32)) -> Result<(), DesktopSidecarErrorBody> {
        let mut enigo = new_enigo()?;
        enigo
            .move_mouse(point.0, point.1, Coordinate::Abs)
            .map_err(input_error)
    }

    pub(super) fn mouse_up(
        point: (i32, i32),
        button: DesktopSidecarMouseButton,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let mut enigo = new_enigo()?;
        enigo
            .move_mouse(point.0, point.1, Coordinate::Abs)
            .map_err(input_error)?;
        enigo
            .button(mouse_button(button), Direction::Release)
            .map_err(input_error)
    }

    pub(super) fn scroll(delta_x: i32, delta_y: i32) -> Result<(), DesktopSidecarErrorBody> {
        let mut enigo = new_enigo()?;
        if delta_y != 0 {
            enigo
                .scroll(scroll_units(delta_y), Axis::Vertical)
                .map_err(input_error)?;
        }
        if delta_x != 0 {
            enigo
                .scroll(scroll_units(delta_x), Axis::Horizontal)
                .map_err(input_error)?;
        }
        Ok(())
    }

    pub(super) fn key_press(key: &str) -> Result<(), DesktopSidecarErrorBody> {
        let key = key_for(key)?;
        let mut enigo = new_enigo()?;
        enigo.key(key, Direction::Click).map_err(input_error)
    }

    pub(super) fn hotkey(keys: &[String]) -> Result<(), DesktopSidecarErrorBody> {
        let mut modifiers = Vec::new();
        let mut target = None;
        for key in keys {
            if let Some(modifier) = modifier_key(key) {
                modifiers.push(modifier);
            } else {
                target = Some(key_for(key)?);
            }
        }
        let mut enigo = new_enigo()?;
        for modifier in &modifiers {
            enigo
                .key(*modifier, Direction::Press)
                .map_err(input_error)?;
        }
        if let Some(target) = target {
            enigo.key(target, Direction::Click).map_err(input_error)?;
        }
        for modifier in modifiers.iter().rev() {
            enigo
                .key(*modifier, Direction::Release)
                .map_err(input_error)?;
        }
        Ok(())
    }

    pub(super) fn type_text(text: &str) -> Result<(), DesktopSidecarErrorBody> {
        let mut enigo = new_enigo()?;
        enigo.text(text).map_err(input_error)
    }

    pub(super) fn paste_text(text: &str) -> Result<(), DesktopSidecarErrorBody> {
        write_clipboard_text(text)?;
        hotkey(&["control".into(), "v".into()])
    }

    pub(super) fn read_clipboard_text() -> Result<String, DesktopSidecarErrorBody> {
        let mut clipboard = arboard::Clipboard::new().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "permission_clipboard_denied",
                format!("Could not open the system clipboard for read: {error}"),
                false,
                true,
            )
        })?;
        clipboard.get_text().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "sidecar_clipboard_read_failed",
                format!("Could not read text from the system clipboard: {error}"),
                true,
                false,
            )
        })
    }

    pub(super) fn write_clipboard_text(text: &str) -> Result<(), DesktopSidecarErrorBody> {
        let mut clipboard = arboard::Clipboard::new().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "permission_clipboard_denied",
                format!("Could not open the system clipboard for write: {error}"),
                false,
                true,
            )
        })?;
        clipboard.set_text(text.to_owned()).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "sidecar_clipboard_write_failed",
                format!("Could not write paste text to the system clipboard: {error}"),
                true,
                false,
            )
        })
    }

    fn mouse_button(button: DesktopSidecarMouseButton) -> Button {
        match button {
            DesktopSidecarMouseButton::Left => Button::Left,
            DesktopSidecarMouseButton::Middle => Button::Middle,
            DesktopSidecarMouseButton::Right => Button::Right,
        }
    }

    fn scroll_units(delta: i32) -> i32 {
        let units = delta / 120;
        if units == 0 {
            delta.signum()
        } else {
            units.clamp(-20, 20)
        }
    }

    fn modifier_key(key: &str) -> Option<Key> {
        match key.trim().to_ascii_lowercase().as_str() {
            "cmd" | "command" | "meta" | "super" | "windows" => Some(Key::Meta),
            "ctrl" | "control" => Some(Key::Control),
            "alt" | "option" => Some(Key::Alt),
            "shift" => Some(Key::Shift),
            _ => None,
        }
    }

    fn key_for(key: &str) -> Result<Key, DesktopSidecarErrorBody> {
        let normalized = key.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "return" | "enter" => Ok(Key::Return),
            "tab" => Ok(Key::Tab),
            "space" => Ok(Key::Space),
            "backspace" => Ok(Key::Backspace),
            "delete" | "forwarddelete" | "forward_delete" => Ok(Key::Delete),
            "escape" | "esc" => Ok(Key::Escape),
            "home" => Ok(Key::Home),
            "end" => Ok(Key::End),
            "pageup" | "page_up" => Ok(Key::PageUp),
            "pagedown" | "page_down" => Ok(Key::PageDown),
            "left" | "arrowleft" | "left_arrow" => Ok(Key::LeftArrow),
            "right" | "arrowright" | "right_arrow" => Ok(Key::RightArrow),
            "down" | "arrowdown" | "down_arrow" => Ok(Key::DownArrow),
            "up" | "arrowup" | "up_arrow" => Ok(Key::UpArrow),
            "f1" => Ok(Key::F1),
            "f2" => Ok(Key::F2),
            "f3" => Ok(Key::F3),
            "f4" => Ok(Key::F4),
            "f5" => Ok(Key::F5),
            "f6" => Ok(Key::F6),
            "f7" => Ok(Key::F7),
            "f8" => Ok(Key::F8),
            "f9" => Ok(Key::F9),
            "f10" => Ok(Key::F10),
            "f11" => Ok(Key::F11),
            "f12" => Ok(Key::F12),
            "volumeup" | "volume_up" | "audio_volume_up" => Ok(Key::VolumeUp),
            "volumedown" | "volume_down" | "audio_volume_down" => Ok(Key::VolumeDown),
            "volumemute" | "volume_mute" | "mute" | "audio_mute" => Ok(Key::VolumeMute),
            #[cfg(all(unix, not(target_os = "macos")))]
            "micmute" | "mic_mute" => Ok(Key::MicMute),
            "mediaplaypause" | "media_play_pause" | "playpause" | "play_pause" => {
                Ok(Key::MediaPlayPause)
            }
            "medianext" | "media_next" | "media_next_track" | "nexttrack" | "next_track" => {
                Ok(Key::MediaNextTrack)
            }
            "mediaprevious" | "media_previous" | "media_prev" | "media_prev_track"
            | "previous_track" | "prev_track" => Ok(Key::MediaPrevTrack),
            "mediastop" | "media_stop" => Ok(Key::MediaStop),
            "shift" => Ok(Key::Shift),
            "ctrl" | "control" => Ok(Key::Control),
            "alt" | "option" => Ok(Key::Alt),
            "cmd" | "command" | "meta" | "super" | "windows" => Ok(Key::Meta),
            value if value.chars().count() == 1 => {
                Ok(Key::Unicode(value.chars().next().expect("single char")))
            }
            _ => Err(DesktopSidecarErrorBody::new(
                "desktop_key_unsupported",
                format!("Desktop key `{key}` is not supported by the sidecar keyboard mapper."),
                false,
                true,
            )),
        }
    }

    fn input_error(error: enigo::InputError) -> DesktopSidecarErrorBody {
        DesktopSidecarErrorBody::new(
            "sidecar_input_event_failed",
            format!("Could not send desktop input event: {error}"),
            true,
            false,
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn key_mapper_accepts_common_browser_key_names() {
            assert_eq!(key_for("Enter").expect("enter"), Key::Return);
            assert_eq!(key_for("ArrowLeft").expect("arrow"), Key::LeftArrow);
            assert_eq!(key_for("Backspace").expect("backspace"), Key::Backspace);
            assert_eq!(key_for("Delete").expect("delete"), Key::Delete);
            assert_eq!(key_for("v").expect("v"), Key::Unicode('v'));
        }

        #[test]
        fn key_mapper_rejects_macos_only_keys() {
            for key in [
                "media_fast_forward",
                "media_rewind",
                "brightness_up",
                "brightness_down",
            ] {
                assert_eq!(
                    key_for(key).expect_err("macOS-only key").code,
                    "desktop_key_unsupported"
                );
            }
        }

        #[cfg(target_os = "windows")]
        #[test]
        fn key_mapper_rejects_linux_only_keys_on_windows() {
            assert_eq!(
                key_for("mic_mute").expect_err("Linux-only key").code,
                "desktop_key_unsupported"
            );
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        #[test]
        fn key_mapper_accepts_linux_mic_mute_key() {
            assert_eq!(key_for("mic_mute").expect("mic mute"), Key::MicMute);
        }

        #[test]
        fn wheel_pixels_convert_to_bounded_scroll_units() {
            assert_eq!(scroll_units(80), 1);
            assert_eq!(scroll_units(-80), -1);
            assert_eq!(scroll_units(5_000), 20);
        }
    }
}

#[cfg(target_os = "macos")]
mod macos_input {
    use super::DesktopSidecarErrorBody;
    use xero_desktop_control_ipc::DesktopSidecarMouseButton;

    pub(super) fn mouse_move(point: (i32, i32)) -> Result<(), DesktopSidecarErrorBody> {
        use core_graphics::{
            event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton},
            event_source::{CGEventSource, CGEventSourceStateID},
            geometry::CGPoint,
        };
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| input_source_error())?;
        let event = CGEvent::new_mouse_event(
            source,
            CGEventType::MouseMoved,
            CGPoint::new(point.0 as f64, point.1 as f64),
            CGMouseButton::Left,
        )
        .map_err(|_| event_error("mouse move"))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    pub(super) fn mouse_down(
        point: (i32, i32),
        button: DesktopSidecarMouseButton,
    ) -> Result<(), DesktopSidecarErrorBody> {
        use core_graphics::{
            event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton},
            event_source::{CGEventSource, CGEventSourceStateID},
            geometry::CGPoint,
        };
        let cg_button = match button {
            DesktopSidecarMouseButton::Left => CGMouseButton::Left,
            DesktopSidecarMouseButton::Right => CGMouseButton::Right,
            DesktopSidecarMouseButton::Middle => CGMouseButton::Center,
        };
        let event_type = match button {
            DesktopSidecarMouseButton::Right => CGEventType::RightMouseDown,
            _ => CGEventType::LeftMouseDown,
        };
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| input_source_error())?;
        let event = CGEvent::new_mouse_event(
            source,
            event_type,
            CGPoint::new(point.0 as f64, point.1 as f64),
            cg_button,
        )
        .map_err(|_| event_error("mouse down"))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    pub(super) fn mouse_click(
        point: (i32, i32),
        button: DesktopSidecarMouseButton,
        clicks: u8,
    ) -> Result<(), DesktopSidecarErrorBody> {
        use core_graphics::{
            event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton},
            event_source::{CGEventSource, CGEventSourceStateID},
            geometry::CGPoint,
        };
        let cg_button = match button {
            DesktopSidecarMouseButton::Left => CGMouseButton::Left,
            DesktopSidecarMouseButton::Right => CGMouseButton::Right,
            DesktopSidecarMouseButton::Middle => CGMouseButton::Center,
        };
        let (down, up) = match button {
            DesktopSidecarMouseButton::Right => {
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

    pub(super) fn mouse_drag_move(point: (i32, i32)) -> Result<(), DesktopSidecarErrorBody> {
        use core_graphics::{
            event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton},
            event_source::{CGEventSource, CGEventSourceStateID},
            geometry::CGPoint,
        };
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| input_source_error())?;
        let event = CGEvent::new_mouse_event(
            source,
            CGEventType::LeftMouseDragged,
            CGPoint::new(point.0 as f64, point.1 as f64),
            CGMouseButton::Left,
        )
        .map_err(|_| event_error("mouse drag move"))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    pub(super) fn mouse_up(
        point: (i32, i32),
        button: DesktopSidecarMouseButton,
    ) -> Result<(), DesktopSidecarErrorBody> {
        use core_graphics::{
            event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton},
            event_source::{CGEventSource, CGEventSourceStateID},
            geometry::CGPoint,
        };
        let cg_button = match button {
            DesktopSidecarMouseButton::Left => CGMouseButton::Left,
            DesktopSidecarMouseButton::Right => CGMouseButton::Right,
            DesktopSidecarMouseButton::Middle => CGMouseButton::Center,
        };
        let event_type = match button {
            DesktopSidecarMouseButton::Right => CGEventType::RightMouseUp,
            _ => CGEventType::LeftMouseUp,
        };
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| input_source_error())?;
        let event = CGEvent::new_mouse_event(
            source,
            event_type,
            CGPoint::new(point.0 as f64, point.1 as f64),
            cg_button,
        )
        .map_err(|_| event_error("mouse up"))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    pub(super) fn mouse_drag(
        from: (i32, i32),
        to: (i32, i32),
    ) -> Result<(), DesktopSidecarErrorBody> {
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

    pub(super) fn scroll(delta_x: i32, delta_y: i32) -> Result<(), DesktopSidecarErrorBody> {
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

    pub(super) fn key_press(key: &str) -> Result<(), DesktopSidecarErrorBody> {
        if let Some(key) = media_key_for(key) {
            return post_media_key(key);
        }
        let key_code = key_code_for(key).ok_or_else(|| {
            DesktopSidecarErrorBody::new(
                "desktop_key_unsupported",
                format!("Desktop key `{key}` is not supported by the sidecar keyboard mapper."),
                false,
                true,
            )
        })?;
        post_key_code(
            key_code,
            core_graphics::event::CGEventFlags::CGEventFlagNull,
        )
    }

    pub(super) fn hotkey(keys: &[String]) -> Result<(), DesktopSidecarErrorBody> {
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
                DesktopSidecarErrorBody::new(
                    "desktop_key_unsupported",
                    format!("Desktop hotkey target `{key}` is not supported by the sidecar keyboard mapper."),
                    false,
                    true,
                )
            })?,
            None if flags.contains(CGEventFlags::CGEventFlagCommand) => KeyCode::COMMAND,
            None if flags.contains(CGEventFlags::CGEventFlagControl) => KeyCode::CONTROL,
            None if flags.contains(CGEventFlags::CGEventFlagAlternate) => KeyCode::OPTION,
            None if flags.contains(CGEventFlags::CGEventFlagShift) => KeyCode::SHIFT,
            None => {
                return Err(DesktopSidecarErrorBody::new(
                    "sidecar_schema_invalid",
                    "Desktop sidecar hotkey request did not include a target key.",
                    false,
                    false,
                ))
            }
        };
        post_key_code(key_code, flags)
    }

    pub(super) fn type_text(text: &str) -> Result<(), DesktopSidecarErrorBody> {
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

    fn input_source_error() -> DesktopSidecarErrorBody {
        DesktopSidecarErrorBody::new(
            "permission_accessibility_denied",
            "Could not create desktop input source. Grant Accessibility permission to Xero.",
            false,
            true,
        )
    }

    fn event_error(kind: &str) -> DesktopSidecarErrorBody {
        DesktopSidecarErrorBody::new(
            "sidecar_input_event_failed",
            format!("Could not build desktop {kind} event."),
            true,
            false,
        )
    }

    pub(super) fn media_key_for(key: &str) -> Option<enigo::Key> {
        match key.trim().to_ascii_lowercase().as_str() {
            "volumeup" | "volume_up" | "audio_volume_up" => Some(enigo::Key::VolumeUp),
            "volumedown" | "volume_down" | "audio_volume_down" => Some(enigo::Key::VolumeDown),
            "volumemute" | "volume_mute" | "mute" | "audio_mute" => Some(enigo::Key::VolumeMute),
            "mediaplaypause" | "media_play_pause" | "playpause" | "play_pause" => {
                Some(enigo::Key::MediaPlayPause)
            }
            "medianext" | "media_next" | "media_next_track" | "nexttrack" | "next_track" => {
                Some(enigo::Key::MediaNextTrack)
            }
            "mediaprevious" | "media_previous" | "media_prev" | "media_prev_track"
            | "previous_track" | "prev_track" => Some(enigo::Key::MediaPrevTrack),
            "mediafast" | "media_fast" | "media_fast_forward" | "fast_forward" => {
                Some(enigo::Key::MediaFast)
            }
            "mediarewind" | "media_rewind" | "rewind" => Some(enigo::Key::MediaRewind),
            "brightnessup" | "brightness_up" => Some(enigo::Key::BrightnessUp),
            "brightnessdown" | "brightness_down" => Some(enigo::Key::BrightnessDown),
            _ => None,
        }
    }

    fn post_media_key(key: enigo::Key) -> Result<(), DesktopSidecarErrorBody> {
        use enigo::{Direction, Enigo, Keyboard, Settings};
        let mut enigo = Enigo::new(&Settings::default()).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "permission_accessibility_denied",
                format!("Could not initialize desktop media-key backend: {error}"),
                false,
                true,
            )
        })?;
        enigo.key(key, Direction::Click).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "sidecar_input_event_failed",
                format!("Could not send desktop media key: {error}"),
                true,
                false,
            )
        })
    }

    fn post_key_code(
        key_code: core_graphics::event::CGKeyCode,
        flags: core_graphics::event::CGEventFlags,
    ) -> Result<(), DesktopSidecarErrorBody> {
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

    pub(super) fn key_code_for(key: &str) -> Option<core_graphics::event::CGKeyCode> {
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
            "backspace" => Some(KeyCode::DELETE),
            "delete" | "forwarddelete" | "forward_delete" => Some(KeyCode::FORWARD_DELETE),
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

#[cfg(target_os = "windows")]
mod windows_ocr {
    use std::{fs, process::Command};

    use serde::Deserialize;

    use super::{
        CapturedDesktopImage, DesktopSidecarErrorBody, DesktopSidecarOcrSnapshotPayload,
        DesktopSidecarOcrTextBlock,
    };

    const WINDOWS_OCR_SCRIPT: &str = r####"
$ErrorActionPreference = 'Stop'

function New-XeroOcrFailure([string]$Message) {
  return [ordered]@{
    performed = $false
    textBlocks = @()
    fullText = ''
    truncated = $false
    diagnostics = @("windows_ocr_unavailable: $Message")
  }
}

try {
  Add-Type -AssemblyName System.Runtime.WindowsRuntime
  $null = [Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
  $null = [Windows.Storage.Streams.IRandomAccessStreamWithContentType, Windows.Storage.Streams, ContentType = WindowsRuntime]
  $null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Graphics.Imaging, ContentType = WindowsRuntime]
  $null = [Windows.Graphics.Imaging.SoftwareBitmap, Windows.Graphics.Imaging, ContentType = WindowsRuntime]
  $null = [Windows.Media.Ocr.OcrEngine, Windows.Media.Ocr, ContentType = WindowsRuntime]

  function Await-XeroWinRtOperation($AsyncOperation, [type]$ResultType) {
    $method = [System.WindowsRuntimeSystemExtensions].GetMethods() |
      Where-Object {
        $_.Name -eq 'AsTask' -and
        $_.IsGenericMethodDefinition -and
        $_.GetParameters().Count -eq 1
      } |
      Select-Object -First 1
    if ($null -eq $method) { throw 'Could not resolve WindowsRuntimeSystemExtensions.AsTask.' }
    $task = $method.MakeGenericMethod($ResultType).Invoke($null, @($AsyncOperation))
    $task.Wait()
    return $task.Result
  }

  $imagePath = [string]$env:XERO_OCR_IMAGE_PATH
  $limit = [Math]::Max(1, [Math]::Min(500, [int]$env:XERO_OCR_LIMIT))
  $originX = [int]$env:XERO_OCR_ORIGIN_X
  $originY = [int]$env:XERO_OCR_ORIGIN_Y

  $file = Await-XeroWinRtOperation ([Windows.Storage.StorageFile]::GetFileFromPathAsync($imagePath)) ([Windows.Storage.StorageFile])
  $stream = Await-XeroWinRtOperation ($file.OpenReadAsync()) ([Windows.Storage.Streams.IRandomAccessStreamWithContentType])
  $decoder = Await-XeroWinRtOperation ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
  $bitmap = Await-XeroWinRtOperation ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
  $engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
  if ($null -eq $engine) {
    (New-XeroOcrFailure 'Windows.Media.Ocr does not have an OCR engine for the current user profile languages.') | ConvertTo-Json -Depth 8 -Compress
    return
  }

  $ocr = Await-XeroWinRtOperation ($engine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])
  $blocks = New-Object 'System.Collections.Generic.List[object]'
  $diagnostics = New-Object 'System.Collections.Generic.List[string]'
  $diagnostics.Add('Windows.Media.Ocr does not expose per-word confidence; confidence is reported as 1.0 for recognized lines.')
  $truncated = $false

  foreach ($line in @($ocr.Lines)) {
    if ($blocks.Count -ge $limit) {
      $truncated = $true
      break
    }
    $words = @($line.Words)
    if ($words.Count -eq 0) { continue }
    $left = [double]::PositiveInfinity
    $top = [double]::PositiveInfinity
    $right = [double]::NegativeInfinity
    $bottom = [double]::NegativeInfinity
    foreach ($word in $words) {
      $rect = $word.BoundingRect
      $left = [Math]::Min($left, $rect.X)
      $top = [Math]::Min($top, $rect.Y)
      $right = [Math]::Max($right, $rect.X + $rect.Width)
      $bottom = [Math]::Max($bottom, $rect.Y + $rect.Height)
    }
    if ([double]::IsInfinity($left) -or [double]::IsInfinity($top)) { continue }
    $text = ([string]$line.Text).Trim()
    if ([string]::IsNullOrWhiteSpace($text)) { continue }
    $blocks.Add([ordered]@{
      text = $text
      x = $originX + [int][Math]::Round($left)
      y = $originY + [int][Math]::Round($top)
      width = [uint32][Math]::Max(0, [Math]::Round($right - $left))
      height = [uint32][Math]::Max(0, [Math]::Round($bottom - $top))
      confidence = 1.0
    })
  }

  $fullText = (@($blocks) | ForEach-Object { $_.text }) -join "`n"
  ([ordered]@{
    performed = $true
    textBlocks = @($blocks)
    fullText = $fullText
    truncated = [bool]$truncated
    diagnostics = @($diagnostics)
  }) | ConvertTo-Json -Depth 12 -Compress
} catch {
  (New-XeroOcrFailure $_.Exception.Message) | ConvertTo-Json -Depth 8 -Compress
}
"####;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct WindowsOcrOutput {
        performed: bool,
        #[serde(default)]
        text_blocks: Vec<DesktopSidecarOcrTextBlock>,
        #[serde(default)]
        full_text: String,
        #[serde(default)]
        truncated: bool,
        #[serde(default)]
        diagnostics: Vec<String>,
    }

    pub(super) fn recognize_png(
        capture: &CapturedDesktopImage,
        png_bytes: Vec<u8>,
        limit: usize,
    ) -> Result<DesktopSidecarOcrSnapshotPayload, DesktopSidecarErrorBody> {
        let temp_path = std::env::temp_dir().join(format!(
            "xero-windows-ocr-{}-{}.png",
            std::process::id(),
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        fs::write(&temp_path, png_bytes).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "desktop_windows_ocr_tempfile_failed",
                format!("Windows OCR could not create a temporary capture file: {error}"),
                true,
                false,
            )
        })?;

        let result = run_windows_ocr_script(&temp_path, capture, limit);
        let _ = fs::remove_file(&temp_path);
        let output = result?;
        Ok(DesktopSidecarOcrSnapshotPayload {
            performed: output.performed,
            captured_at: capture.captured_at.clone(),
            width: capture.image.width(),
            height: capture.image.height(),
            scale_factor: capture.scale_factor,
            text_blocks: output.text_blocks,
            full_text: output.full_text,
            truncated: output.truncated,
            diagnostics: output.diagnostics,
        })
    }

    fn run_windows_ocr_script(
        image_path: &std::path::Path,
        capture: &CapturedDesktopImage,
        limit: usize,
    ) -> Result<WindowsOcrOutput, DesktopSidecarErrorBody> {
        let output = Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                WINDOWS_OCR_SCRIPT,
            ])
            .env("XERO_OCR_IMAGE_PATH", image_path)
            .env("XERO_OCR_LIMIT", limit.to_string())
            .env("XERO_OCR_ORIGIN_X", capture.origin_x.to_string())
            .env("XERO_OCR_ORIGIN_Y", capture.origin_y.to_string())
            .output()
            .map_err(|error| {
                DesktopSidecarErrorBody::new(
                    "desktop_windows_ocr_unavailable",
                    format!("Windows OCR could not start PowerShell: {error}"),
                    true,
                    true,
                )
            })?;
        if !output.status.success() {
            return Err(DesktopSidecarErrorBody::new(
                "desktop_windows_ocr_failed",
                format!(
                    "Windows OCR command failed: {}",
                    command_output_message(&output.stdout, &output.stderr)
                ),
                true,
                true,
            ));
        }
        serde_json::from_slice::<WindowsOcrOutput>(&output.stdout).map_err(|error| {
            DesktopSidecarErrorBody::new(
                "desktop_windows_ocr_json_decode_failed",
                format!("Windows OCR returned malformed JSON: {error}"),
                true,
                false,
            )
        })
    }

    fn command_output_message(stdout: &[u8], stderr: &[u8]) -> String {
        let stderr = String::from_utf8_lossy(stderr).trim().to_string();
        if !stderr.is_empty() {
            return stderr;
        }
        let stdout = String::from_utf8_lossy(stdout).trim().to_string();
        if stdout.is_empty() {
            "command exited unsuccessfully without diagnostics".into()
        } else {
            stdout
        }
    }
}

#[cfg(target_os = "macos")]
mod macos_ocr {
    use objc2::{rc::autoreleasepool, runtime::AnyObject, AnyThread};
    use objc2_foundation::{NSArray, NSData, NSDictionary, NSError};
    use objc2_vision::{
        VNImageOption, VNImageRequestHandler, VNRecognizeTextRequest, VNRequest,
        VNRequestTextRecognitionLevel,
    };

    use super::{
        CapturedDesktopImage, DesktopSidecarErrorBody, DesktopSidecarOcrSnapshotPayload,
        DesktopSidecarOcrTextBlock,
    };

    pub(super) fn recognize_png(
        capture: &CapturedDesktopImage,
        png_bytes: Vec<u8>,
        limit: usize,
    ) -> Result<DesktopSidecarOcrSnapshotPayload, DesktopSidecarErrorBody> {
        autoreleasepool(|_| recognize_png_inner(capture, png_bytes, limit))
    }

    fn recognize_png_inner(
        capture: &CapturedDesktopImage,
        png_bytes: Vec<u8>,
        limit: usize,
    ) -> Result<DesktopSidecarOcrSnapshotPayload, DesktopSidecarErrorBody> {
        let image_data = NSData::with_bytes(&png_bytes);
        let option_keys: [&VNImageOption; 0] = [];
        let option_objects: [&AnyObject; 0] = [];
        let options =
            NSDictionary::<VNImageOption, AnyObject>::from_slices(&option_keys, &option_objects);
        let handler = VNImageRequestHandler::initWithData_options(
            VNImageRequestHandler::alloc(),
            &image_data,
            &options,
        );
        let request = VNRecognizeTextRequest::new();
        request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
        request.setUsesLanguageCorrection(true);
        request.setAutomaticallyDetectsLanguage(true);

        let request_ref: &VNRequest = request.as_ref();
        let requests = NSArray::from_slice(&[request_ref]);
        handler.performRequests_error(&requests).map_err(|error| {
            vision_error("desktop_ocr_failed", "macOS Vision OCR failed", &error)
        })?;

        let observations = request
            .results()
            .map(|results| results.to_vec())
            .unwrap_or_default();
        let truncated = observations.len() > limit;
        let mut text_blocks = Vec::new();

        for observation in observations.into_iter().take(limit) {
            let candidates = observation.topCandidates(1);
            let Some(candidate) = candidates.firstObject() else {
                continue;
            };
            let raw_text = candidate.string().to_string();
            if raw_text.trim().is_empty() {
                continue;
            }
            let bbox = unsafe { observation.boundingBox() };
            text_blocks.push(text_block_from_bbox(
                raw_text,
                candidate.confidence(),
                bbox.origin.x,
                bbox.origin.y,
                bbox.size.width,
                bbox.size.height,
                capture,
            ));
        }

        let full_text = text_blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(DesktopSidecarOcrSnapshotPayload {
            performed: true,
            captured_at: capture.captured_at.clone(),
            width: capture.image.width(),
            height: capture.image.height(),
            scale_factor: capture.scale_factor,
            text_blocks,
            full_text,
            truncated,
            diagnostics: Vec::new(),
        })
    }

    fn text_block_from_bbox(
        text: String,
        confidence: f32,
        origin_x: f64,
        origin_y: f64,
        width: f64,
        height: f64,
        capture: &CapturedDesktopImage,
    ) -> DesktopSidecarOcrTextBlock {
        let image_width = capture.image.width() as f64;
        let image_height = capture.image.height() as f64;
        let block_width = (width.clamp(0.0, 1.0) * image_width).round().max(0.0) as u32;
        let block_height = (height.clamp(0.0, 1.0) * image_height).round().max(0.0) as u32;
        let x = capture
            .origin_x
            .saturating_add((origin_x.clamp(0.0, 1.0) * image_width).round() as i32);
        let y_from_top =
            image_height - (origin_y.clamp(0.0, 1.0) * image_height) - block_height as f64;
        let y = capture
            .origin_y
            .saturating_add(y_from_top.round().max(0.0) as i32);
        DesktopSidecarOcrTextBlock {
            text,
            x,
            y,
            width: block_width,
            height: block_height,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }

    fn vision_error(
        code: &'static str,
        context: &'static str,
        error: &NSError,
    ) -> DesktopSidecarErrorBody {
        DesktopSidecarErrorBody::new(
            code,
            format!("{context}: {}", error.localizedDescription()),
            true,
            false,
        )
    }
}

mod clipboard_resources {
    use std::{borrow::Cow, path::PathBuf};

    use arboard::{Clipboard, Error as ClipboardError, ImageData};
    use base64::Engine as _;
    use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder, ImageFormat};

    use super::{
        schema_error, ClipboardReadHtmlRequest, ClipboardReadImageRequest, ClipboardReadRtfRequest,
        DesktopSidecarClipboardFilesPayload, DesktopSidecarClipboardHtmlPayload,
        DesktopSidecarClipboardImagePayload, DesktopSidecarClipboardRtfPayload,
        DesktopSidecarControlRequest, DesktopSidecarErrorBody, CLIPBOARD_HTML_DEFAULT_MAX_BYTES,
        CLIPBOARD_HTML_MAX_BYTES, CLIPBOARD_IMAGE_DEFAULT_MAX_BYTES, CLIPBOARD_IMAGE_MAX_BYTES,
        CLIPBOARD_MAX_FILE_PATHS, CLIPBOARD_RTF_DEFAULT_MAX_BYTES, CLIPBOARD_RTF_MAX_BYTES,
    };

    pub(super) fn read_image(
        request: ClipboardReadImageRequest,
    ) -> Result<DesktopSidecarClipboardImagePayload, DesktopSidecarErrorBody> {
        let mut clipboard = open_clipboard("read image")?;
        let image = match clipboard.get_image() {
            Ok(image) => image,
            Err(ClipboardError::ContentNotAvailable) => {
                return Ok(DesktopSidecarClipboardImagePayload {
                    available: false,
                    media_type: "image/png".into(),
                    width: 0,
                    height: 0,
                    byte_length: 0,
                    data_base64: None,
                    truncated: false,
                });
            }
            Err(error) => {
                return Err(clipboard_error(
                    "sidecar_clipboard_read_failed",
                    format!("Could not read image from the system clipboard: {error}"),
                    true,
                    false,
                ));
            }
        };
        let png_bytes = encode_png(&image)?;
        let max_bytes = request
            .max_bytes
            .unwrap_or(CLIPBOARD_IMAGE_DEFAULT_MAX_BYTES)
            .clamp(1, CLIPBOARD_IMAGE_MAX_BYTES);
        let include_data = request.include_data && png_bytes.len() <= max_bytes;
        Ok(DesktopSidecarClipboardImagePayload {
            available: true,
            media_type: "image/png".into(),
            width: image.width as u32,
            height: image.height as u32,
            byte_length: png_bytes.len(),
            data_base64: include_data
                .then(|| base64::engine::general_purpose::STANDARD.encode(&png_bytes)),
            truncated: request.include_data && png_bytes.len() > max_bytes,
        })
    }

    pub(super) fn write_image(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let media_type = request
            .media_type
            .as_deref()
            .unwrap_or("image/png")
            .trim()
            .to_ascii_lowercase();
        if media_type != "image/png" {
            return Err(schema_error("mediaType"));
        }
        let encoded = request
            .image_data_base64
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| schema_error("imageDataBase64"))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|error| {
                clipboard_error(
                    "desktop_clipboard_image_decode_failed",
                    format!("Clipboard image data was not valid base64: {error}"),
                    false,
                    false,
                )
            })?;
        let image = image::load_from_memory_with_format(&bytes, ImageFormat::Png)
            .map_err(|error| {
                clipboard_error(
                    "desktop_clipboard_image_decode_failed",
                    format!("Clipboard image data was not a valid PNG image: {error}"),
                    false,
                    false,
                )
            })?
            .to_rgba8();
        let (width, height) = image.dimensions();
        let image = ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Owned(image.into_raw()),
        };
        let mut clipboard = open_clipboard("write image")?;
        clipboard.set_image(image).map_err(|error| {
            clipboard_error(
                "sidecar_clipboard_write_failed",
                format!("Could not write image to the system clipboard: {error}"),
                true,
                false,
            )
        })
    }

    pub(super) fn read_html(
        request: ClipboardReadHtmlRequest,
    ) -> Result<DesktopSidecarClipboardHtmlPayload, DesktopSidecarErrorBody> {
        let mut clipboard = open_clipboard("read HTML")?;
        let html = match clipboard.get().html() {
            Ok(html) => html,
            Err(ClipboardError::ContentNotAvailable) => {
                return Ok(DesktopSidecarClipboardHtmlPayload {
                    available: false,
                    html: None,
                    byte_length: 0,
                    truncated: false,
                });
            }
            Err(error) => {
                return Err(clipboard_error(
                    "sidecar_clipboard_read_failed",
                    format!("Could not read HTML from the system clipboard: {error}"),
                    true,
                    false,
                ));
            }
        };
        let byte_length = html.len();
        let max_bytes = request
            .max_bytes
            .unwrap_or(CLIPBOARD_HTML_DEFAULT_MAX_BYTES)
            .clamp(1, CLIPBOARD_HTML_MAX_BYTES);
        let truncated = byte_length > max_bytes;
        let html = if truncated {
            Some(truncate_utf8(&html, max_bytes))
        } else {
            Some(html)
        };
        Ok(DesktopSidecarClipboardHtmlPayload {
            available: true,
            html,
            byte_length,
            truncated,
        })
    }

    pub(super) fn write_html(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let html = request
            .html
            .as_deref()
            .filter(|html| !html.trim().is_empty())
            .ok_or_else(|| schema_error("html"))?;
        let alt_text = request
            .alt_text
            .as_deref()
            .filter(|alt_text| !alt_text.trim().is_empty());
        let mut clipboard = open_clipboard("write HTML")?;
        clipboard
            .set()
            .html(Cow::Borrowed(html), alt_text.map(Cow::Borrowed))
            .map_err(|error| {
                clipboard_error(
                    "sidecar_clipboard_write_failed",
                    format!("Could not write HTML to the system clipboard: {error}"),
                    true,
                    false,
                )
            })
    }

    pub(super) fn read_rtf(
        request: ClipboardReadRtfRequest,
    ) -> Result<DesktopSidecarClipboardRtfPayload, DesktopSidecarErrorBody> {
        platform_read_rtf(request)
    }

    pub(super) fn write_rtf(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let rtf = request
            .rtf
            .as_deref()
            .filter(|rtf| !rtf.trim().is_empty() && rtf.len() <= CLIPBOARD_RTF_MAX_BYTES)
            .ok_or_else(|| schema_error("rtf"))?;
        platform_write_rtf(rtf)
    }

    #[cfg(target_os = "macos")]
    fn platform_read_rtf(
        request: ClipboardReadRtfRequest,
    ) -> Result<DesktopSidecarClipboardRtfPayload, DesktopSidecarErrorBody> {
        use objc2::rc::autoreleasepool;
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeRTF};

        let bytes = autoreleasepool(|_| {
            let pasteboard = NSPasteboard::generalPasteboard();
            pasteboard
                .dataForType(unsafe { NSPasteboardTypeRTF })
                .map(|data| nsdata_to_vec(&data))
        });
        let Some(bytes) = bytes else {
            return Ok(unavailable_rtf_payload("clipboard_rtf_not_available"));
        };
        rtf_payload_from_bytes(bytes, request.max_bytes, Vec::new())
    }

    #[cfg(target_os = "windows")]
    fn platform_read_rtf(
        request: ClipboardReadRtfRequest,
    ) -> Result<DesktopSidecarClipboardRtfPayload, DesktopSidecarErrorBody> {
        let script = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
if (-not [System.Windows.Forms.Clipboard]::ContainsText([System.Windows.Forms.TextDataFormat]::Rtf)) {
  return
}
$rtf = [System.Windows.Forms.Clipboard]::GetText([System.Windows.Forms.TextDataFormat]::Rtf)
[Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($rtf))
"#;
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-STA",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ])
            .output()
            .map_err(|error| {
                clipboard_error(
                    "sidecar_clipboard_read_failed",
                    format!("Could not read RTF from the Windows clipboard: {error}"),
                    true,
                    false,
                )
            })?;
        if !output.status.success() {
            return Err(clipboard_error(
                "sidecar_clipboard_read_failed",
                format!(
                    "Could not read RTF from the Windows clipboard: {}",
                    super::sidecar_command_output_message(&output.stdout, &output.stderr)
                ),
                true,
                false,
            ));
        }
        let encoded = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if encoded.is_empty() {
            return Ok(unavailable_rtf_payload("clipboard_rtf_not_available"));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.as_bytes())
            .map_err(|error| {
                clipboard_error(
                    "desktop_clipboard_rtf_decode_failed",
                    format!("Windows clipboard RTF data was not valid base64: {error}"),
                    false,
                    false,
                )
            })?;
        rtf_payload_from_bytes(bytes, request.max_bytes, Vec::new())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn platform_read_rtf(
        _request: ClipboardReadRtfRequest,
    ) -> Result<DesktopSidecarClipboardRtfPayload, DesktopSidecarErrorBody> {
        Ok(unavailable_rtf_payload(
            "clipboard_rtf_unsupported_on_platform",
        ))
    }

    #[cfg(target_os = "macos")]
    fn platform_write_rtf(rtf: &str) -> Result<(), DesktopSidecarErrorBody> {
        use objc2::rc::autoreleasepool;
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeRTF};
        use objc2_foundation::NSData;
        use std::ffi::c_void;

        let wrote = autoreleasepool(|_| {
            let pasteboard = NSPasteboard::generalPasteboard();
            pasteboard.clearContents();
            let data =
                unsafe { NSData::dataWithBytes_length(rtf.as_ptr().cast::<c_void>(), rtf.len()) };
            pasteboard.setData_forType(Some(&data), unsafe { NSPasteboardTypeRTF })
        });
        if wrote {
            Ok(())
        } else {
            Err(clipboard_error(
                "sidecar_clipboard_write_failed",
                "Could not write RTF to the macOS pasteboard.",
                true,
                false,
            ))
        }
    }

    #[cfg(target_os = "windows")]
    fn platform_write_rtf(rtf: &str) -> Result<(), DesktopSidecarErrorBody> {
        use std::io::Write as _;

        let script = r#"
$ErrorActionPreference = 'Stop'
[Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName System.Windows.Forms
$rtf = [Console]::In.ReadToEnd()
[System.Windows.Forms.Clipboard]::SetText($rtf, [System.Windows.Forms.TextDataFormat]::Rtf)
"#;
        let mut child = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-STA",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| {
                clipboard_error(
                    "sidecar_clipboard_write_failed",
                    format!("Could not start Windows clipboard RTF writer: {error}"),
                    true,
                    false,
                )
            })?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(rtf.as_bytes()).map_err(|error| {
                clipboard_error(
                    "sidecar_clipboard_write_failed",
                    format!("Could not send RTF to the Windows clipboard writer: {error}"),
                    true,
                    false,
                )
            })?;
        }
        let output = child.wait_with_output().map_err(|error| {
            clipboard_error(
                "sidecar_clipboard_write_failed",
                format!("Could not finish Windows clipboard RTF writer: {error}"),
                true,
                false,
            )
        })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(clipboard_error(
                "sidecar_clipboard_write_failed",
                format!(
                    "Could not write RTF to the Windows clipboard: {}",
                    super::sidecar_command_output_message(&output.stdout, &output.stderr)
                ),
                true,
                false,
            ))
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn platform_write_rtf(_rtf: &str) -> Result<(), DesktopSidecarErrorBody> {
        Err(clipboard_error(
            "desktop_clipboard_rtf_unsupported",
            "RTF clipboard payloads are not supported on this platform.",
            false,
            true,
        ))
    }

    #[cfg(target_os = "macos")]
    fn nsdata_to_vec(data: &objc2_foundation::NSData) -> Vec<u8> {
        use std::{ffi::c_void, ptr::NonNull};

        let len = data.length();
        let mut bytes = vec![0u8; len];
        if len > 0 {
            let ptr = NonNull::new(bytes.as_mut_ptr().cast::<c_void>())
                .expect("non-empty vec has pointer");
            unsafe { data.getBytes_length(ptr, len) };
        }
        bytes
    }

    fn rtf_payload_from_bytes(
        bytes: Vec<u8>,
        requested_max_bytes: Option<usize>,
        diagnostics: Vec<String>,
    ) -> Result<DesktopSidecarClipboardRtfPayload, DesktopSidecarErrorBody> {
        let byte_length = bytes.len();
        let max_bytes = requested_max_bytes
            .unwrap_or(CLIPBOARD_RTF_DEFAULT_MAX_BYTES)
            .clamp(1, CLIPBOARD_RTF_MAX_BYTES);
        let truncated = byte_length > max_bytes;
        let rtf = String::from_utf8(bytes).map_err(|error| {
            clipboard_error(
                "desktop_clipboard_rtf_decode_failed",
                format!("Clipboard RTF was not UTF-8 text: {error}"),
                false,
                false,
            )
        })?;
        let rtf = if truncated {
            truncate_utf8(&rtf, max_bytes)
        } else {
            rtf
        };
        Ok(DesktopSidecarClipboardRtfPayload {
            available: true,
            rtf: Some(rtf),
            byte_length,
            truncated,
            diagnostics,
        })
    }

    fn unavailable_rtf_payload(diagnostic: &'static str) -> DesktopSidecarClipboardRtfPayload {
        DesktopSidecarClipboardRtfPayload {
            available: false,
            rtf: None,
            byte_length: 0,
            truncated: false,
            diagnostics: vec![diagnostic.into()],
        }
    }

    pub(super) fn read_files(
    ) -> Result<DesktopSidecarClipboardFilesPayload, DesktopSidecarErrorBody> {
        let mut clipboard = open_clipboard("read file list")?;
        let files = match clipboard.get().file_list() {
            Ok(files) => files,
            Err(ClipboardError::ContentNotAvailable) => {
                return Ok(DesktopSidecarClipboardFilesPayload {
                    available: false,
                    files: Vec::new(),
                    count: 0,
                    truncated: false,
                });
            }
            Err(error) => {
                return Err(clipboard_error(
                    "sidecar_clipboard_read_failed",
                    format!("Could not read file paths from the system clipboard: {error}"),
                    true,
                    false,
                ));
            }
        };
        let count = files.len();
        let truncated = count > CLIPBOARD_MAX_FILE_PATHS;
        let files = files
            .into_iter()
            .take(CLIPBOARD_MAX_FILE_PATHS)
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        Ok(DesktopSidecarClipboardFilesPayload {
            available: true,
            files,
            count,
            truncated,
        })
    }

    pub(super) fn write_files(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(), DesktopSidecarErrorBody> {
        let paths = request
            .file_paths
            .iter()
            .map(|path| PathBuf::from(path.trim()))
            .collect::<Vec<_>>();
        let mut clipboard = open_clipboard("write file list")?;
        clipboard.set().file_list(&paths).map_err(|error| {
            clipboard_error(
                "sidecar_clipboard_write_failed",
                format!("Could not write file paths to the system clipboard: {error}"),
                true,
                false,
            )
        })
    }

    pub(super) fn file_drop(
        request: &DesktopSidecarControlRequest,
        paste: impl FnOnce() -> Result<(), DesktopSidecarErrorBody>,
    ) -> Result<(), DesktopSidecarErrorBody> {
        write_files(request)?;
        paste()
    }

    fn truncate_utf8(value: &str, max_bytes: usize) -> String {
        if value.len() <= max_bytes {
            return value.to_string();
        }
        let end = value
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= max_bytes)
            .last()
            .unwrap_or(0);
        value[..end].to_string()
    }

    fn open_clipboard(action: &'static str) -> Result<Clipboard, DesktopSidecarErrorBody> {
        Clipboard::new().map_err(|error| {
            clipboard_error(
                "permission_clipboard_denied",
                format!("Could not open the system clipboard to {action}: {error}"),
                false,
                true,
            )
        })
    }

    fn encode_png(image: &ImageData<'_>) -> Result<Vec<u8>, DesktopSidecarErrorBody> {
        let expected = image
            .width
            .checked_mul(image.height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                clipboard_error(
                    "desktop_clipboard_image_invalid",
                    "Clipboard image dimensions overflowed while preparing PNG data.",
                    false,
                    false,
                )
            })?;
        if image.bytes.len() != expected {
            return Err(clipboard_error(
                "desktop_clipboard_image_invalid",
                format!(
                    "Clipboard image byte length {} did not match {}x{} RGBA data.",
                    image.bytes.len(),
                    image.width,
                    image.height
                ),
                false,
                false,
            ));
        }
        let mut bytes = Vec::new();
        PngEncoder::new(&mut bytes)
            .write_image(
                image.bytes.as_ref(),
                image.width as u32,
                image.height as u32,
                ExtendedColorType::Rgba8,
            )
            .map_err(|error| {
                clipboard_error(
                    "desktop_clipboard_image_encode_failed",
                    format!("Could not encode clipboard image as PNG: {error}"),
                    true,
                    false,
                )
            })?;
        Ok(bytes)
    }

    fn clipboard_error(
        code: &'static str,
        message: impl Into<String>,
        retryable: bool,
        user_action_required: bool,
    ) -> DesktopSidecarErrorBody {
        DesktopSidecarErrorBody::new(code, message, retryable, user_action_required)
    }
}

#[cfg(target_os = "macos")]
mod macos_clipboard {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;

    use super::{macos_input, DesktopSidecarErrorBody};

    pub(super) fn paste_text(text: &str) -> Result<(), DesktopSidecarErrorBody> {
        write_text(text)?;
        macos_input::hotkey(&["command".into(), "v".into()])
    }

    pub(super) fn read_text() -> Result<String, DesktopSidecarErrorBody> {
        autoreleasepool(|_| {
            let pasteboard = NSPasteboard::generalPasteboard();
            let text = pasteboard.stringForType(unsafe { NSPasteboardTypeString });
            text.map(|value| value.to_string()).ok_or_else(|| {
                DesktopSidecarErrorBody::new(
                    "sidecar_clipboard_read_failed",
                    "The system pasteboard does not currently contain plain text.",
                    false,
                    true,
                )
            })
        })
    }

    pub(super) fn write_text(text: &str) -> Result<(), DesktopSidecarErrorBody> {
        let wrote = autoreleasepool(|_| {
            let pasteboard = NSPasteboard::generalPasteboard();
            pasteboard.clearContents();
            let text = NSString::from_str(text);
            pasteboard.setString_forType(&text, unsafe { NSPasteboardTypeString })
        });
        if !wrote {
            return Err(DesktopSidecarErrorBody::new(
                "desktop_clipboard_write_failed",
                "Desktop sidecar could not write supplied text to the system pasteboard.",
                true,
                false,
            ));
        }
        Ok(())
    }
}

fn health_payload(health: &str, message: &str) -> serde_json::Value {
    json!({
        "health": health,
        "message": message,
        "platform": std::env::consts::OS,
        "pid": process::id(),
        "checkedAt": now_timestamp(),
    })
}

fn write_response(response: DesktopSidecarResponse) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &response)
        .map_err(|error| format!("could not encode sidecar response: {error}"))?;
    stdout
        .write_all(b"\n")
        .map_err(|error| format!("could not write sidecar response: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("could not flush sidecar response: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(app_name: &str, pid: u32, focused: bool) -> DesktopSidecarWindow {
        DesktopSidecarWindow {
            window_id: format!("window-{app_name}-{pid}"),
            app_name: app_name.into(),
            title: "Window".into(),
            pid,
            x: 0,
            y: 0,
            width: 800,
            height: 600,
            z: 0,
            focused,
            minimized: false,
        }
    }

    fn display(
        display_id: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        scale_factor: f32,
        primary: bool,
    ) -> DesktopSidecarDisplay {
        DesktopSidecarDisplay {
            display_id: display_id.into(),
            name: format!("Display {display_id}"),
            x,
            y,
            width,
            height,
            scale_factor,
            rotation: 0.0,
            primary,
        }
    }

    #[test]
    fn apps_from_windows_groups_by_app_and_pid() {
        let apps = apps_from_windows(&[
            window("Notes", 10, false),
            window("Notes", 10, true),
            window("Safari", 20, false),
        ]);

        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].app_name, "Notes");
        assert_eq!(apps[0].window_count, 2);
        assert!(apps[0].focused);
        assert_eq!(apps[1].app_name, "Safari");
        assert_eq!(apps[1].window_count, 1);
    }

    #[test]
    fn app_inventory_merges_running_windows_into_installed_entries() {
        let mut inventory = vec![DesktopSidecarAppInventoryEntry {
            app_name: "Safari".into(),
            bundle_id: Some("com.apple.Safari".into()),
            executable_path: None,
            launch_target: Some("com.apple.Safari".into()),
            launch_kind: "bundle_id_or_app_name".into(),
            source: "/Applications".into(),
            installed: true,
            running: false,
            pid: None,
            window_count: 0,
            focused: false,
            diagnostics: Vec::new(),
        }];

        merge_running_apps_into_inventory(
            &mut inventory,
            vec![
                DesktopSidecarApp {
                    app_name: "Safari".into(),
                    pid: 42,
                    window_count: 2,
                    focused: true,
                },
                DesktopSidecarApp {
                    app_name: "Notes".into(),
                    pid: 7,
                    window_count: 1,
                    focused: false,
                },
            ],
        );

        assert_eq!(inventory.len(), 2);
        assert!(inventory[0].running);
        assert_eq!(inventory[0].pid, Some(42));
        assert_eq!(inventory[0].window_count, 2);
        assert_eq!(inventory[1].source, "running_windows");
        assert_eq!(
            inventory[1].diagnostics,
            vec!["not_matched_to_installed_app"]
        );
    }

    #[test]
    fn plist_string_reader_extracts_xml_values() {
        let plist = r#"
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.example.Xero</string>
  <key>CFBundleDisplayName</key>
  <string>Xero &amp; Friends</string>
</dict>
</plist>
"#;

        assert_eq!(
            read_plist_string_key_from_xml(plist, "CFBundleIdentifier").as_deref(),
            Some("com.example.Xero")
        );
        assert_eq!(
            read_plist_string_key_from_xml(plist, "CFBundleDisplayName").as_deref(),
            Some("Xero & Friends")
        );
    }

    #[test]
    fn desktop_label_preview_truncates_long_window_titles() {
        let long_title = "a".repeat(260);

        assert_eq!(desktop_label_preview(&long_title).len(), 240);
    }

    #[test]
    fn sidecar_capabilities_match_implemented_operations() {
        let capabilities = sidecar_capabilities();

        assert!(capabilities.display_list);
        assert!(capabilities.window_list);
        assert!(capabilities.app_list);
        assert_eq!(
            capabilities.notification_observation,
            cfg!(target_os = "windows")
        );
        assert!(capabilities.foreground_state);
        assert!(capabilities.screenshot);
        let native_input_platform = cfg!(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        ));
        assert_eq!(capabilities.cursor_state, native_input_platform);
        assert_eq!(
            capabilities.accessibility_snapshot,
            cfg!(any(target_os = "macos", target_os = "windows"))
        );
        assert_eq!(
            capabilities.ocr_snapshot,
            cfg!(any(target_os = "macos", target_os = "windows"))
        );
        assert!(capabilities.screenshot_fallback_stream);
        assert_eq!(capabilities.mouse_input, native_input_platform);
        assert_eq!(capabilities.keyboard_input, native_input_platform);
        assert_eq!(capabilities.clipboard, native_input_platform);
        assert_eq!(capabilities.manual_cloud_control, native_input_platform);
        assert_eq!(capabilities.window_focus, cfg!(target_os = "windows"));
        assert_eq!(capabilities.app_control, cfg!(target_os = "windows"));
        assert_eq!(
            capabilities.accessibility_actions,
            cfg!(any(target_os = "macos", target_os = "windows"))
        );
        assert_eq!(
            capabilities.menu_select,
            cfg!(any(target_os = "macos", target_os = "windows"))
        );
        assert_eq!(capabilities.webrtc_stream, native_webrtc_stream_available());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn notification_snapshot_reports_platform_availability() {
        let snapshot = sidecar_notification_snapshot();

        assert_eq!(snapshot.count, snapshot.notifications.len());
        assert!(!snapshot.available);
        assert_eq!(snapshot.permission_status, "unsupported");
        assert!(snapshot
            .diagnostics
            .contains(&"macos_notification_center_observation_not_available_to_sidecar".into()));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_notification_snapshot_decoder_maps_rows() {
        let snapshot = windows_notification_snapshot_from_json(
            br#"{
                "available": true,
                "permissionStatus": "Allowed",
                "source": "windows_user_notification_listener",
                "notifications": [{
                    "id": "7",
                    "appName": "Mail",
                    "title": "Hello",
                    "body": "World",
                    "source": "windows_user_notification_listener",
                    "diagnostics": []
                }],
                "diagnostics": []
            }"#,
        )
        .expect("snapshot");

        assert!(snapshot.available);
        assert_eq!(snapshot.count, 1);
        assert_eq!(snapshot.notifications[0].title.as_deref(), Some("Hello"));
    }

    #[test]
    fn display_arrangement_reports_virtual_bounds_primary_and_scale_factors() {
        let arrangement = display_arrangement_from_displays(vec![
            display("main", 0, 0, 1920, 1080, 2.0, true),
            display("side", 1920, 0, 1920, 1080, 1.0, false),
        ]);

        assert_eq!(arrangement.display_count, 2);
        assert_eq!(arrangement.primary_display_id.as_deref(), Some("main"));
        assert_eq!(
            arrangement.virtual_bounds,
            DesktopSidecarDisplayBounds {
                x: 0,
                y: 0,
                width: 3840,
                height: 1080,
            }
        );
        assert_eq!(arrangement.scale_factors, vec![1.0, 2.0]);
        assert!(!arrangement.has_overlaps);
        assert!(!arrangement.has_gaps_in_virtual_bounds);
        assert_eq!(
            arrangement.diagnostics,
            vec!["display_arrangement_multiple_scale_factors"]
        );
    }

    #[test]
    fn display_arrangement_flags_virtual_gaps_and_overlaps() {
        let offset = display_arrangement_from_displays(vec![
            display("main", 0, 0, 100, 100, 1.0, true),
            display("corner", 100, 100, 100, 100, 1.0, false),
        ]);
        assert!(offset.has_gaps_in_virtual_bounds);
        assert!(offset
            .diagnostics
            .contains(&"display_arrangement_virtual_bounds_include_gaps".into()));

        let overlapping = display_arrangement_from_displays(vec![
            display("main", 0, 0, 100, 100, 1.0, true),
            display("overlap", 50, 50, 100, 100, 1.0, false),
        ]);
        assert!(overlapping.has_overlaps);
        assert!(!overlapping.has_gaps_in_virtual_bounds);
        assert!(overlapping
            .diagnostics
            .contains(&"display_arrangement_overlapping_bounds".into()));
    }

    #[test]
    fn app_control_validation_rejects_blank_targets() {
        let mut launch = DesktopSidecarControlRequest {
            app_name: Some("   ".into()),
            ..Default::default()
        };
        assert!(validate_control_request(DesktopSidecarOperation::LaunchApp, &launch).is_err());

        launch.app_name = None;
        launch.bundle_id = Some("Microsoft.WindowsNotepad_8wekyb3d8bbwe!App".into());
        assert!(validate_control_request(DesktopSidecarOperation::LaunchApp, &launch).is_ok());

        let mut focus = DesktopSidecarControlRequest {
            window_id: Some("   ".into()),
            ..Default::default()
        };
        assert!(validate_control_request(DesktopSidecarOperation::FocusWindow, &focus).is_err());

        focus.window_id = None;
        focus.app_name = Some("Notepad".into());
        assert!(validate_control_request(DesktopSidecarOperation::FocusWindow, &focus).is_ok());
    }

    #[test]
    fn sidecar_permissions_include_platform_requirement_rows() {
        let permissions = sidecar_permissions().permissions;
        let names = permissions
            .iter()
            .map(|permission| permission.name.as_str())
            .collect::<Vec<_>>();

        if cfg!(target_os = "windows") {
            assert!(names.contains(&"Screen Capture"));
            assert!(names.contains(&"Desktop Input"));
            assert!(names.contains(&"UI Automation"));
            let input = permissions
                .iter()
                .find(|permission| permission.name == "Desktop Input")
                .expect("desktop input permission");
            assert_eq!(input.status, DesktopSidecarPermissionGrant::Granted);
            assert!(input.required_for.contains(&"window_focus".to_string()));
            assert!(input.required_for.contains(&"app_control".to_string()));
            return;
        }

        assert!(names.contains(&"Screen Recording"));
        assert!(names.contains(&"Accessibility"));
        assert!(names.contains(&"Input Monitoring"));
        assert!(names.contains(&"Remote Desktop Portal"));

        let portal = permissions
            .iter()
            .find(|permission| permission.name == "Remote Desktop Portal")
            .expect("portal permission");
        assert_eq!(
            portal.required_for,
            vec!["wayland_capture".to_string(), "wayland_input".to_string()]
        );
        assert_eq!(
            portal.status,
            if cfg!(target_os = "linux") {
                DesktopSidecarPermissionGrant::Unknown
            } else {
                DesktopSidecarPermissionGrant::Unsupported
            }
        );

        if cfg!(target_os = "macos") {
            for permission_name in ["Screen Recording", "Accessibility", "Input Monitoring"] {
                let permission = permissions
                    .iter()
                    .find(|permission| permission.name == permission_name)
                    .expect("macOS permission row");
                assert!(
                    matches!(
                        permission.status,
                        DesktopSidecarPermissionGrant::Granted
                            | DesktopSidecarPermissionGrant::Denied
                    ),
                    "{permission_name} should be resolved from macOS permission APIs"
                );
            }
        }
    }

    #[test]
    fn sidecar_stream_capabilities_report_native_webrtc_publisher() {
        let capabilities = sidecar_stream_capabilities();

        assert_eq!(capabilities.webrtc_stream, native_webrtc_stream_available());
        assert!(capabilities.screenshot_fallback_stream);
        assert_eq!(
            capabilities.native_video_track,
            native_webrtc_stream_available()
        );
        assert_eq!(
            capabilities.preferred_codec.as_deref(),
            native_webrtc_stream_available().then_some(MIME_TYPE_H264)
        );
        assert_eq!(
            capabilities
                .capture_backends
                .iter()
                .any(|backend| backend == "screencapturekit"),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            capabilities
                .capture_backends
                .iter()
                .any(|backend| backend == "dxgi_output_duplication"),
            cfg!(target_os = "windows")
        );
        assert_eq!(
            capabilities
                .encoder_backends
                .iter()
                .any(|backend| backend == "videotoolbox_h264"),
            cfg!(target_os = "macos")
        );
        assert_eq!(
            capabilities
                .encoder_backends
                .iter()
                .any(|backend| backend == "openh264_software"),
            cfg!(target_os = "windows")
        );
        assert_eq!(
            capabilities.hardware_encoding,
            native_hardware_encoding_available()
        );
        assert_eq!(capabilities.max_width, WEBRTC_MAX_WIDTH);
        assert_eq!(capabilities.max_frame_rate, WEBRTC_MAX_FRAME_RATE);
        if native_webrtc_stream_available() {
            assert!(capabilities.message.contains("H.264"));
        } else {
            assert!(capabilities.message.contains("not implemented"));
        }
    }

    #[test]
    fn stream_target_bitrate_clamps_when_transport_is_congested() {
        let metrics = StreamTelemetry::default();
        assert_eq!(
            stream_target_bitrate(DesktopSidecarStreamQuality::Balanced, &metrics),
            3_500_000
        );

        metrics.round_trip_time_ms.store(300, Ordering::Relaxed);
        assert_eq!(
            stream_target_bitrate(DesktopSidecarStreamQuality::Balanced, &metrics),
            2_625_000
        );

        metrics
            .available_outgoing_bitrate_bps
            .store(1_000_000, Ordering::Relaxed);
        assert_eq!(
            stream_target_bitrate(DesktopSidecarStreamQuality::Balanced, &metrics),
            850_000
        );
    }

    #[test]
    fn latest_frame_channel_replaces_stale_pending_frame() {
        webrtc_runtime()
            .expect("webrtc runtime")
            .block_on(async move {
                let (sender, mut receiver) = latest_frame_channel();

                assert_eq!(sender.send_replace(1), LatestFrameSendResult::Stored);
                assert_eq!(sender.send_replace(2), LatestFrameSendResult::Replaced);

                assert_eq!(receiver.recv().await, Some(2));
            });
    }

    #[test]
    fn latest_frame_channel_closes_after_sender_drops() {
        webrtc_runtime()
            .expect("webrtc runtime")
            .block_on(async move {
                let (sender, mut receiver) = latest_frame_channel::<u8>();

                drop(sender);

                assert_eq!(receiver.recv().await, None);
            });
    }

    #[test]
    fn h264_idr_detection_handles_length_prefixed_samples() {
        let non_idr = [0, 0, 0, 2, 0x41, 0x9a];
        let idr = [0, 0, 0, 3, 0x65, 0x88, 0x84];

        assert!(!h264_sample_contains_idr(&non_idr, 4).expect("non-idr sample"));
        assert!(h264_sample_contains_idr(&idr, 4).expect("idr sample"));
    }

    #[test]
    fn h264_parameter_sets_are_sent_for_actual_idr_frames() {
        assert!(h264_should_include_parameter_sets(42, false, true));
        assert!(h264_should_include_parameter_sets(42, true, false));
        assert!(h264_should_include_parameter_sets(0, false, false));
        assert!(!h264_should_include_parameter_sets(42, false, false));
    }

    #[test]
    fn sidecar_stream_rejects_invalid_payload_shape() {
        let error = sidecar_stream(
            DesktopSidecarOperation::StreamStart,
            json!({ "maxWidth": 10 }),
        )
        .expect_err("invalid stream request");

        assert_eq!(error.code, "sidecar_schema_invalid");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn sidecar_stream_starts_native_webrtc_offer() {
        let payload = sidecar_stream(
            DesktopSidecarOperation::StreamStart,
            json!({
                "sessionId": "session-1",
                "runId": "run-1",
                "streamId": "stream-1",
                "maxWidth": 1280,
                "maxFrameRate": 24,
                "includeCursor": true,
                "quality": "balanced"
            }),
        )
        .expect("stream starts with a native offer");
        let payload =
            serde_json::from_value::<DesktopSidecarStreamPayload>(payload).expect("stream payload");

        assert_eq!(payload.stream_id.as_deref(), Some("stream-1"));
        assert_eq!(payload.transport, DesktopSidecarStreamTransport::WebRtc);
        assert_eq!(payload.status, DesktopSidecarStreamStatus::Starting);
        assert_eq!(
            payload
                .session_description
                .as_ref()
                .map(|description| description.sdp_type.as_str()),
            Some("offer")
        );
        assert!(payload
            .session_description
            .as_ref()
            .is_some_and(|description| description.sdp.contains("m=video")));
        assert!(!payload
            .session_description
            .as_ref()
            .is_some_and(|description| description.sdp.contains("m=application")));

        let stopped = sidecar_stream(
            DesktopSidecarOperation::StreamStop,
            json!({
                "sessionId": "session-1",
                "runId": "run-1",
                "streamId": "stream-1"
            }),
        )
        .expect("stream stops");
        let stopped =
            serde_json::from_value::<DesktopSidecarStreamPayload>(stopped).expect("stream payload");
        assert_eq!(stopped.status, DesktopSidecarStreamStatus::Stopped);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[test]
    fn sidecar_stream_start_rejects_unsupported_native_publisher() {
        let error = sidecar_stream(
            DesktopSidecarOperation::StreamStart,
            json!({
                "sessionId": "session-1",
                "runId": "run-1",
                "streamId": "stream-1",
                "quality": "balanced"
            }),
        )
        .expect_err("unsupported native stream should fail");

        assert_eq!(error.code, "stream_native_publisher_unavailable");
    }

    #[test]
    fn native_webrtc_offer_negotiates_h264_video_track() {
        webrtc_runtime().expect("webrtc runtime").block_on(async {
            let request = DesktopSidecarStreamRequest {
                session_id: Some("session-negotiation".into()),
                run_id: Some("run-negotiation".into()),
                display_id: None,
                stream_id: Some("stream-negotiation".into()),
                max_width: Some(1280),
                max_frame_rate: Some(24),
                include_cursor: Some(true),
                quality: Some(DesktopSidecarStreamQuality::Balanced),
                ice_servers: Vec::new(),
                session_description: None,
                ice_candidate: None,
            };
            let (offerer, video_track, _rtp_sender, offer) =
                create_webrtc_offer(&request).await.expect("sidecar offer");

            let mut media_engine = MediaEngine::default();
            media_engine
                .register_default_codecs()
                .expect("answerer codecs");
            let answerer = Arc::new(
                APIBuilder::new()
                    .with_media_engine(media_engine)
                    .build()
                    .new_peer_connection(RTCConfiguration::default())
                    .await
                    .expect("answerer peer connection"),
            );
            let (track_tx, mut track_rx) = tokio::sync::mpsc::channel(1);
            answerer.on_track(Box::new(move |track, _, _| {
                let track_tx = track_tx.clone();
                Box::pin(async move {
                    let _ = track_tx
                        .send(track.codec().capability.mime_type.clone())
                        .await;
                })
            }));
            let (connected_tx, mut connected_rx) = tokio::sync::mpsc::channel(1);
            answerer.on_peer_connection_state_change(Box::new(move |state| {
                let connected_tx = connected_tx.clone();
                Box::pin(async move {
                    if state
                        == webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState::Connected
                    {
                        let _ = connected_tx.send(()).await;
                    }
                })
            }));

            answerer
                .set_remote_description(RTCSessionDescription::offer(offer.sdp).expect("offer sdp"))
                .await
                .expect("answerer remote description");
            let answer = answerer.create_answer(None).await.expect("answer");
            let mut gather_complete = answerer.gathering_complete_promise().await;
            answerer
                .set_local_description(answer)
                .await
                .expect("answerer local description");
            let _ = tokio::time::timeout(WEBRTC_ICE_GATHER_TIMEOUT, gather_complete.recv()).await;
            offerer
                .set_remote_description(
                    answerer
                        .local_description()
                        .await
                        .expect("answerer local description"),
                )
                .await
                .expect("offerer remote description");
            tokio::time::timeout(Duration::from_secs(5), connected_rx.recv())
                .await
                .expect("peer connection")
                .expect("connected state");

            let mime = tokio::time::timeout(Duration::from_secs(15), async {
                let mut tick = tokio::time::interval(Duration::from_millis(50));
                loop {
                    tokio::select! {
                        mime = track_rx.recv() => return mime,
                        _ = tick.tick() => {
                            video_track
                                .write_sample(&Sample {
                                    data: vec![0, 0, 0, 1, 0x65, 0x88, 0x84].into(),
                                    duration: Duration::from_millis(42),
                                    ..Default::default()
                                })
                                .await
                                .expect("write h264 sample");
                        }
                    }
                }
            })
                .await
                .expect("track callback")
                .expect("video track mime");
            assert_eq!(mime, MIME_TYPE_H264);
            offerer.close().await.expect("close offerer");
            answerer.close().await.expect("close answerer");
        });
    }

    #[test]
    fn sidecar_stream_validates_webrtc_signaling_payloads() {
        let missing_answer = sidecar_stream(
            DesktopSidecarOperation::StreamAnswer,
            json!({
                "sessionId": "session-1",
                "streamId": "stream-1"
            }),
        )
        .expect_err("answer operation requires a session description");

        assert_eq!(missing_answer.code, "sidecar_schema_invalid");

        let invalid_candidate = sidecar_stream(
            DesktopSidecarOperation::StreamIceCandidate,
            json!({
                "sessionId": "session-1",
                "streamId": "stream-1",
                "iceCandidate": { "candidate": "" }
            }),
        )
        .expect_err("candidate operation requires a non-empty candidate");

        assert_eq!(invalid_candidate.code, "sidecar_schema_invalid");

        let valid_answer_without_stream = sidecar_stream(
            DesktopSidecarOperation::StreamAnswer,
            json!({
                "sessionId": "session-1",
                "streamId": "stream-1",
                "sessionDescription": {
                    "type": "answer",
                    "sdp": "v=0"
                }
            }),
        )
        .expect_err("valid answer still requires an active stream");

        assert_eq!(valid_answer_without_stream.code, "stream_not_found");
    }

    #[test]
    fn control_request_requires_non_negative_point() {
        let request = DesktopSidecarControlRequest {
            x: Some(10),
            y: Some(20),
            ..Default::default()
        };

        assert_eq!(required_point(&request).expect("point"), (10, 20));

        let invalid = DesktopSidecarControlRequest {
            x: Some(-1),
            y: Some(20),
            ..Default::default()
        };
        assert_eq!(
            required_point(&invalid).expect_err("invalid point").code,
            "sidecar_schema_invalid"
        );
    }

    #[test]
    fn element_at_point_rejects_negative_coordinates() {
        let error = sidecar_element_at_point(serde_json::json!({
            "x": -1,
            "y": 20
        }))
        .expect_err("invalid point");

        assert_eq!(error.code, "sidecar_schema_invalid");
    }

    #[test]
    fn accessibility_snapshot_rejects_unbounded_requests() {
        let error = sidecar_accessibility_snapshot(serde_json::json!({
            "limit": 501,
            "maxDepth": 9
        }))
        .expect_err("unbounded snapshot");

        assert_eq!(error.code, "sidecar_schema_invalid");
    }

    #[test]
    fn ocr_snapshot_rejects_unbounded_requests() {
        let error = sidecar_ocr_snapshot(serde_json::json!({
            "limit": 501
        }))
        .expect_err("unbounded ocr");

        assert_eq!(error.code, "sidecar_schema_invalid");
    }

    #[test]
    fn ax_press_requires_element_target() {
        let error = sidecar_control(
            DesktopSidecarOperation::AxPress,
            serde_json::json!({ "elementId": "" }),
        )
        .expect_err("missing element target");

        assert_eq!(error.code, "sidecar_schema_invalid");
    }

    #[test]
    fn structured_ax_actions_require_element_target() {
        for operation in [
            DesktopSidecarOperation::AxSelect,
            DesktopSidecarOperation::AxConfirm,
            DesktopSidecarOperation::AxCancel,
            DesktopSidecarOperation::AxIncrement,
            DesktopSidecarOperation::AxDecrement,
            DesktopSidecarOperation::AxExpand,
            DesktopSidecarOperation::AxCollapse,
            DesktopSidecarOperation::AxScrollToVisible,
            DesktopSidecarOperation::AxToggle,
        ] {
            let error = sidecar_control(operation, serde_json::json!({ "elementId": "" }))
                .expect_err("missing element target");

            assert_eq!(error.code, "sidecar_schema_invalid");
        }
    }

    #[test]
    fn macos_helper_controls_validate_structured_targets() {
        let mut dock = DesktopSidecarControlRequest::default();
        assert_eq!(
            validate_control_request(DesktopSidecarOperation::DockItemPress, &dock)
                .expect_err("dock target")
                .code,
            "sidecar_schema_invalid"
        );
        dock.app_name = Some("Finder".into());
        assert!(validate_control_request(DesktopSidecarOperation::DockItemPress, &dock).is_ok());

        let mut status = DesktopSidecarControlRequest::default();
        assert_eq!(
            validate_control_request(DesktopSidecarOperation::StatusItemPress, &status)
                .expect_err("status target")
                .code,
            "sidecar_schema_invalid"
        );
        status.target_label = Some("Wi-Fi".into());
        assert!(
            validate_control_request(DesktopSidecarOperation::StatusItemPress, &status).is_ok()
        );

        let root =
            std::env::temp_dir().join(format!("xero-sidecar-dialog-test-{}", std::process::id()));
        fs::create_dir_all(&root).expect("dialog temp dir");
        let save_target = root.join("draft.txt");
        let mut dialog = DesktopSidecarControlRequest {
            file_paths: vec![save_target.to_string_lossy().into_owned()],
            ..Default::default()
        };
        assert!(
            validate_control_request(DesktopSidecarOperation::FileDialogSetPath, &dialog).is_ok()
        );
        dialog.file_paths = vec!["relative.txt".into()];
        assert_eq!(
            validate_control_request(DesktopSidecarOperation::FileDialogSetPath, &dialog)
                .expect_err("relative dialog path")
                .code,
            "sidecar_schema_invalid"
        );
        assert!(validate_control_request(
            DesktopSidecarOperation::FileDialogConfirm,
            &DesktopSidecarControlRequest::default()
        )
        .is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_element_id_round_trips_reresolution_metadata() {
        let handle = macos_accessibility::MacosElementHandle {
            version: 2,
            pid: Some(42),
            app_name: Some("Notes".into()),
            window_title: Some("Untitled".into()),
            window_bounds: Some(macos_accessibility::MacosElementBounds {
                x: 10,
                y: 20,
                width: 800,
                height: 600,
            }),
            ancestry_path: vec![0, 3, 2],
            role: Some("AXButton".into()),
            title: Some("Continue".into()),
            description: Some("Move to next step".into()),
            bounds: Some(macos_accessibility::MacosElementBounds {
                x: 24,
                y: 52,
                width: 110,
                height: 32,
            }),
            hit_x: Some(40),
            hit_y: Some(60),
        };

        let element_id = macos_accessibility::element_id(&handle);
        assert!(element_id.starts_with("macos_ax:v2:"));
        let parsed =
            macos_accessibility::parse_macos_element_handle(&element_id).expect("element handle");

        assert_eq!(parsed, handle);
    }

    #[test]
    fn paste_text_requires_text() {
        let error = sidecar_control(
            DesktopSidecarOperation::PasteText,
            serde_json::json!({ "text": "" }),
        )
        .expect_err("missing paste text");

        assert_eq!(error.code, "sidecar_schema_invalid");
    }

    #[test]
    fn clipboard_write_text_requires_text() {
        let error = sidecar_control(
            DesktopSidecarOperation::ClipboardWriteText,
            serde_json::json!({ "text": "" }),
        )
        .expect_err("missing clipboard text");

        assert_eq!(error.code, "sidecar_schema_invalid");
    }

    #[test]
    fn clipboard_write_html_requires_html() {
        let error = sidecar_control(
            DesktopSidecarOperation::ClipboardWriteHtml,
            serde_json::json!({ "html": "" }),
        )
        .expect_err("missing clipboard HTML");

        assert_eq!(error.code, "sidecar_schema_invalid");
    }

    #[test]
    fn clipboard_write_rtf_requires_rtf() {
        let error = sidecar_control(
            DesktopSidecarOperation::ClipboardWriteRtf,
            serde_json::json!({ "rtf": "" }),
        )
        .expect_err("missing clipboard RTF");

        assert_eq!(error.code, "sidecar_schema_invalid");
    }

    #[test]
    fn ax_set_value_accepts_text_range_replacement() {
        let valid_delete = sidecar_control(
            DesktopSidecarOperation::AxSetValue,
            serde_json::json!({
                "elementId": "macos_ax:1:AXTextField:10:20:120:24:10:20",
                "value": "",
                "selectionStart": 2,
                "selectionEnd": 5
            }),
        );
        assert!(
            !matches!(
                valid_delete.as_ref().map_err(|error| error.code.as_str()),
                Err("sidecar_schema_invalid")
            ),
            "range deletion payload should pass schema validation"
        );

        let invalid_range = sidecar_control(
            DesktopSidecarOperation::AxSetValue,
            serde_json::json!({
                "elementId": "macos_ax:1:AXTextField:10:20:120:24:10:20",
                "value": "x",
                "selectionStart": 5,
                "selectionEnd": 2
            }),
        )
        .expect_err("invalid range");

        assert_eq!(invalid_range.code, "sidecar_schema_invalid");
    }

    #[test]
    fn clipboard_write_image_requires_png_payload() {
        let missing = sidecar_control(
            DesktopSidecarOperation::ClipboardWriteImage,
            serde_json::json!({ "mediaType": "image/png" }),
        )
        .expect_err("missing image payload");
        assert_eq!(missing.code, "sidecar_schema_invalid");

        let unsupported = sidecar_control(
            DesktopSidecarOperation::ClipboardWriteImage,
            serde_json::json!({
                "mediaType": "image/jpeg",
                "imageDataBase64": "abcd"
            }),
        )
        .expect_err("unsupported media type");
        assert_eq!(unsupported.code, "sidecar_schema_invalid");
    }

    #[test]
    fn clipboard_file_actions_require_existing_absolute_paths() {
        let relative = sidecar_control(
            DesktopSidecarOperation::ClipboardWriteFiles,
            serde_json::json!({ "filePaths": ["relative.txt"] }),
        )
        .expect_err("relative file path");
        assert_eq!(relative.code, "sidecar_schema_invalid");

        let missing_path = if cfg!(target_os = "windows") {
            "C:\\definitely\\missing\\xero-file-drop.txt"
        } else {
            "/definitely/missing/xero-file-drop.txt"
        };
        let missing = sidecar_control(
            DesktopSidecarOperation::FileDrop,
            serde_json::json!({ "filePaths": [missing_path] }),
        )
        .expect_err("missing file path");
        assert_eq!(missing.code, "desktop_clipboard_file_not_found");
    }

    #[test]
    fn window_layout_actions_require_window_target() {
        for operation in [
            DesktopSidecarOperation::WindowMaximize,
            DesktopSidecarOperation::WindowMinimize,
            DesktopSidecarOperation::WindowRestore,
            DesktopSidecarOperation::WindowClose,
        ] {
            let error = sidecar_control(operation, serde_json::json!({ "windowId": "" }))
                .expect_err("missing window target");

            assert_eq!(error.code, "sidecar_schema_invalid");
        }
    }

    #[test]
    fn window_move_resize_requires_position_or_size() {
        let error = sidecar_control(
            DesktopSidecarOperation::WindowMoveResize,
            serde_json::json!({ "windowId": "42" }),
        )
        .expect_err("missing layout fields");
        assert_eq!(error.code, "sidecar_schema_invalid");

        let error = sidecar_control(
            DesktopSidecarOperation::WindowMoveResize,
            serde_json::json!({ "windowId": "42", "x": 10 }),
        )
        .expect_err("partial position");
        assert_eq!(error.code, "sidecar_schema_invalid");
    }

    #[test]
    fn menu_select_requires_path() {
        let error = sidecar_control(
            DesktopSidecarOperation::MenuSelect,
            serde_json::json!({ "menuPath": [] }),
        )
        .expect_err("missing menu path");

        assert_eq!(error.code, "sidecar_schema_invalid");
    }

    #[test]
    fn text_input_route_uses_key_events_for_short_ascii() {
        assert_eq!(
            desktop_text_input_route("short ASCII text"),
            DesktopTextInputRoute::KeyEvents
        );
    }

    #[test]
    fn text_input_route_uses_clipboard_paste_for_long_or_non_ascii_text() {
        let long_text = "a".repeat(TYPE_TEXT_PASTE_FIRST_THRESHOLD_CHARS + 1);

        assert_eq!(
            desktop_text_input_route(&long_text),
            DesktopTextInputRoute::ClipboardPaste
        );
        assert_eq!(
            desktop_text_input_route("cafe con leche"),
            DesktopTextInputRoute::KeyEvents
        );
        assert_eq!(
            desktop_text_input_route("café"),
            DesktopTextInputRoute::ClipboardPaste
        );
        assert_eq!(
            desktop_text_input_route("こんにちは"),
            DesktopTextInputRoute::ClipboardPaste
        );
    }

    #[test]
    fn windows_menu_alt_mnemonics_prefer_explicit_and_safe_keys() {
        let mnemonics =
            windows_menu_alt_mnemonics(&["&File".into(), "Save && Close".into(), "E&xit".into()]);

        assert_eq!(mnemonics, vec!["f", "s", "x"]);
    }

    #[test]
    fn windows_menu_alt_mnemonics_reject_non_ascii_paths() {
        let mnemonics = windows_menu_alt_mnemonics(&["ファイル".into()]);

        assert!(mnemonics.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_keyboard_mapper_accepts_common_key_aliases() {
        assert!(macos_input::key_code_for("enter").is_some());
        assert!(macos_input::key_code_for("arrowleft").is_some());
        assert_eq!(
            macos_input::key_code_for("backspace"),
            Some(core_graphics::event::KeyCode::DELETE)
        );
        assert_eq!(
            macos_input::key_code_for("delete"),
            Some(core_graphics::event::KeyCode::FORWARD_DELETE)
        );
        assert_eq!(
            macos_input::media_key_for("volume_up"),
            Some(enigo::Key::VolumeUp)
        );
        assert_eq!(
            macos_input::media_key_for("media_play_pause"),
            Some(enigo::Key::MediaPlayPause)
        );
        assert!(macos_input::key_code_for("definitely-not-a-key").is_none());
    }
}
