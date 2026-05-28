use std::{
    collections::BTreeMap,
    io::{self, BufRead, Cursor, Write},
    process,
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use base64::Engine as _;
use image::{ImageFormat, Rgba, RgbaImage};
use serde_json::json;
use time::format_description::well_known::Rfc3339;
use webrtc::{
    api::{
        media_engine::{MediaEngine, MIME_TYPE_H264},
        APIBuilder,
    },
    ice_transport::{ice_candidate::RTCIceCandidateInit, ice_server::RTCIceServer},
    media::Sample,
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
use xero_desktop_control_ipc::{
    validate_sidecar_handshake, validate_sidecar_request, DesktopSidecarAccessibilityElement,
    DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarAccessibilitySnapshotRequest,
    DesktopSidecarAccessibilitySnapshotRow, DesktopSidecarAccessibilitySnapshotTarget,
    DesktopSidecarApp, DesktopSidecarAppListPayload, DesktopSidecarCapabilities,
    DesktopSidecarControlRequest, DesktopSidecarCursorStatePayload, DesktopSidecarDisplay,
    DesktopSidecarDisplayListPayload, DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody,
    DesktopSidecarForegroundStatePayload, DesktopSidecarHandshake, DesktopSidecarLease,
    DesktopSidecarOcrSnapshotPayload, DesktopSidecarOcrSnapshotRequest, DesktopSidecarOcrTextBlock,
    DesktopSidecarOperation, DesktopSidecarPermissionGrant, DesktopSidecarPermissionStatus,
    DesktopSidecarPermissionsPayload, DesktopSidecarPointRequest, DesktopSidecarRequest,
    DesktopSidecarResponse, DesktopSidecarScreenshotPayload, DesktopSidecarScreenshotRequest,
    DesktopSidecarSessionDescription, DesktopSidecarStreamCapabilitiesPayload,
    DesktopSidecarStreamMetrics, DesktopSidecarStreamPayload, DesktopSidecarStreamQuality,
    DesktopSidecarStreamRequest, DesktopSidecarStreamStatus, DesktopSidecarStreamTransport,
    DesktopSidecarWindow, DesktopSidecarWindowListPayload,
};

const WEBRTC_MAX_WIDTH: u32 = 1920;
const WEBRTC_MAX_FRAME_RATE: u32 = 30;
const WEBRTC_ICE_GATHER_TIMEOUT: Duration = Duration::from_secs(5);
const H264_ANNEX_B_START_CODE: &[u8; 4] = b"\x00\x00\x00\x01";

#[derive(Clone)]
struct WebRtcStreamConfig {
    stream_id: String,
    display_id: Option<String>,
    max_width: u32,
    max_frame_rate: u32,
    include_cursor: bool,
    quality: DesktopSidecarStreamQuality,
    redaction: Option<xero_desktop_control_ipc::DesktopSidecarRedactionRequest>,
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
        DesktopSidecarOperation::WindowList => match sidecar_windows() {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
        DesktopSidecarOperation::AppList => match sidecar_apps() {
            Ok(payload) => json_response(request.request_id, request.operation, payload),
            Err(error) => sidecar_error_response(request.request_id, request.operation, error),
        },
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
        DesktopSidecarOperation::MouseMove
        | DesktopSidecarOperation::MouseClick
        | DesktopSidecarOperation::MouseDoubleClick
        | DesktopSidecarOperation::MouseRightClick
        | DesktopSidecarOperation::MouseDrag
        | DesktopSidecarOperation::Scroll
        | DesktopSidecarOperation::KeyPress
        | DesktopSidecarOperation::Hotkey
        | DesktopSidecarOperation::TypeText
        | DesktopSidecarOperation::PasteText
        | DesktopSidecarOperation::AxPress
        | DesktopSidecarOperation::AxSetValue
        | DesktopSidecarOperation::AxFocus
        | DesktopSidecarOperation::MenuSelect
        | DesktopSidecarOperation::CancelCurrentAction => {
            match sidecar_control(request.operation, request.payload) {
                Ok(payload) => json_response(request.request_id, request.operation, payload),
                Err(error) => sidecar_error_response(request.request_id, request.operation, error),
            }
        }
        _ => DesktopSidecarResponse::error(
            request.request_id,
            request.operation,
            DesktopSidecarErrorBody::new(
                "sidecar_operation_unimplemented",
                "This platform sidecar operation is not implemented by the scaffold backend.",
                false,
                false,
            ),
        ),
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
        foreground_state: true,
        cursor_state: cfg!(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        )),
        accessibility_snapshot: cfg!(target_os = "macos"),
        ocr_snapshot: cfg!(target_os = "macos"),
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
        accessibility_actions: cfg!(target_os = "macos"),
        menu_select: cfg!(target_os = "macos"),
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

#[cfg(target_os = "macos")]
fn desktop_screen_recording_permission_status() -> DesktopSidecarPermissionGrant {
    permission_grant_from_bool(unsafe { CGPreflightScreenCaptureAccess() })
}

#[cfg(not(target_os = "macos"))]
fn desktop_screen_recording_permission_status() -> DesktopSidecarPermissionGrant {
    if cfg!(any(target_os = "windows", target_os = "linux")) {
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
    DesktopSidecarPermissionGrant::Unsupported
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

fn sidecar_apps() -> Result<DesktopSidecarAppListPayload, DesktopSidecarErrorBody> {
    sidecar_window_rows().map(|windows| DesktopSidecarAppListPayload {
        apps: apps_from_windows(&windows),
    })
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

#[cfg(target_os = "macos")]
fn platform_accessibility_snapshot(
    request: DesktopSidecarAccessibilitySnapshotRequest,
) -> Result<DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarErrorBody> {
    macos_accessibility::snapshot(request)
}

#[cfg(not(target_os = "macos"))]
fn platform_element_at_point(
    _request: DesktopSidecarPointRequest,
) -> Result<DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody> {
    Err(unimplemented_operation())
}

#[cfg(not(target_os = "macos"))]
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

#[cfg(target_os = "macos")]
fn platform_ocr_snapshot(
    request: DesktopSidecarOcrSnapshotRequest,
) -> Result<DesktopSidecarOcrSnapshotPayload, DesktopSidecarErrorBody> {
    let capture_request = DesktopSidecarScreenshotRequest {
        display_id: request.display_id,
        region: request.region,
        redaction: request.redaction,
    };
    let capture = capture_desktop_image(&capture_request)?;
    let png_bytes = encode_png(
        &capture.image,
        "desktop_ocr_image_encode_failed",
        "Desktop sidecar could not encode OCR capture PNG",
    )?;
    macos_ocr::recognize_png(&capture, png_bytes, request.limit.unwrap_or(200))
}

#[cfg(not(target_os = "macos"))]
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
        redactions_applied: capture.redactions_applied,
    })
}

fn sidecar_stream_capabilities() -> DesktopSidecarStreamCapabilitiesPayload {
    DesktopSidecarStreamCapabilitiesPayload {
        webrtc_stream: native_webrtc_stream_available(),
        screenshot_fallback_stream: true,
        native_video_track: native_webrtc_stream_available(),
        preferred_codec: Some(MIME_TYPE_H264.into()),
        capture_backends: native_capture_backends(),
        encoder_backends: native_encoder_backends(),
        hardware_encoding: native_webrtc_stream_available(),
        supported_qualities: vec![
            DesktopSidecarStreamQuality::Low,
            DesktopSidecarStreamQuality::Balanced,
            DesktopSidecarStreamQuality::High,
        ],
        max_width: WEBRTC_MAX_WIDTH,
        max_frame_rate: WEBRTC_MAX_FRAME_RATE,
        message: "Native WebRTC desktop streaming publishes an H.264 video track with screenshot fallback available only for degraded mode.".into(),
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
            redaction: None,
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
        redaction: request.redaction.clone(),
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
    if request.redaction.is_some() {
        config.redaction = request.redaction.clone();
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
    #[cfg(not(target_os = "macos"))]
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
    if native_webrtc_stream_available() {
        vec!["screencapturekit".into()]
    } else {
        Vec::new()
    }
}

fn native_encoder_backends() -> Vec<String> {
    if native_webrtc_stream_available() {
        vec!["videotoolbox_h264".into()]
    } else {
        Vec::new()
    }
}

fn native_webrtc_stream_available() -> bool {
    cfg!(target_os = "macos")
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

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeVideoTarget {
    display_id: Option<String>,
    max_width: u32,
    fps: u32,
    include_cursor: bool,
    quality: DesktopSidecarStreamQuality,
}

#[cfg(target_os = "macos")]
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
    receiver: tokio::sync::mpsc::Receiver<MacosScreenFrame>,
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
        let (sender, receiver) = tokio::sync::mpsc::channel(3);
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
                if sender.try_send(MacosScreenFrame { surface }).is_err() {
                    metrics_for_handler
                        .capture_dropped_frames
                        .fetch_add(1, Ordering::Relaxed);
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

#[cfg(target_os = "macos")]
fn scaled_even_dimensions(source_width: u32, source_height: u32, max_width: u32) -> (u32, u32) {
    let width = source_width.min(max_width).max(2);
    let height = ((source_height as u64 * width as u64) / source_width.max(1) as u64)
        .max(2)
        .min(u32::MAX as u64) as u32;
    (make_even(width), make_even(height))
}

#[cfg(target_os = "macos")]
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
    bitrate: i32,
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
            bitrate,
            parameter_sets: Vec::new(),
            nal_length_size: 4,
        })
    }

    fn matches(&self, width: u32, height: u32, fps: u32, bitrate: i32) -> bool {
        self.width == width && self.height == height && self.fps == fps && self.bitrate == bitrate
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
        let include_parameter_sets = force_keyframe || frame_index == 0;
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
            keyframe: force_keyframe || contains_idr,
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
        if force_keyframe
            || encoder.as_ref().is_none_or(|encoder| {
                !encoder.matches(
                    capture_lease.width,
                    capture_lease.height,
                    capture_lease.target.fps,
                    bitrate,
                )
            })
        {
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
    redactions_applied: usize,
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
    let (origin_x, origin_y, mut image) = if let Some(region) = &request.region {
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
    let redactions_applied =
        apply_private_region_redactions(&mut image, request.redaction.as_ref());
    Ok(CapturedDesktopImage {
        image,
        scale_factor,
        captured_at: now_timestamp(),
        redactions_applied,
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
    platform_control(operation, request)?;
    Ok(json!({
        "status": "executed",
        "message": format!("Desktop sidecar executed `{operation:?}`."),
    }))
}

#[cfg(target_os = "macos")]
fn platform_control(
    operation: DesktopSidecarOperation,
    request: DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    match operation {
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
            macos_input::type_text(text)
        }
        DesktopSidecarOperation::PasteText => {
            let text = request
                .text
                .as_deref()
                .filter(|text| !text.is_empty())
                .ok_or_else(|| schema_error("text"))?;
            macos_clipboard::paste_text(text)
        }
        DesktopSidecarOperation::AxPress => macos_accessibility::press(&request),
        DesktopSidecarOperation::AxSetValue => macos_accessibility::set_value(&request),
        DesktopSidecarOperation::AxFocus => macos_accessibility::focus(&request),
        DesktopSidecarOperation::MenuSelect => macos_accessibility::menu_select(&request),
        _ => Err(unimplemented_operation()),
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn platform_control(
    operation: DesktopSidecarOperation,
    request: DesktopSidecarControlRequest,
) -> Result<(), DesktopSidecarErrorBody> {
    match operation {
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
            cross_platform_input::type_text(text)
        }
        DesktopSidecarOperation::PasteText => {
            let text = request
                .text
                .as_deref()
                .filter(|text| !text.is_empty())
                .ok_or_else(|| schema_error("text"))?;
            cross_platform_input::paste_text(text)
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
        "This platform sidecar operation is not implemented by the active backend.",
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

fn apply_private_region_redactions(
    image: &mut RgbaImage,
    redaction: Option<&xero_desktop_control_ipc::DesktopSidecarRedactionRequest>,
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
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

#[cfg(target_os = "macos")]
mod macos_accessibility {
    use std::{ffi::c_void, ptr, thread, time::Duration};

    use core_foundation::{
        array::CFArray,
        base::{CFType, CFTypeID, CFTypeRef, TCFType},
        boolean::CFBoolean,
        number::CFNumber,
        string::{CFString, CFStringRef},
    };
    use core_graphics::geometry::{CGPoint, CGSize};

    use super::{
        redact_sensitive_label, schema_error, DesktopSidecarAccessibilityElement,
        DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarAccessibilitySnapshotRequest,
        DesktopSidecarAccessibilitySnapshotRow, DesktopSidecarAccessibilitySnapshotTarget,
        DesktopSidecarControlRequest, DesktopSidecarElementAtPointPayload, DesktopSidecarErrorBody,
        DesktopSidecarPointRequest,
    };

    type AXError = i32;
    type AXUIElementRef = *const c_void;
    type AXValueRef = *const c_void;

    const AX_ERROR_SUCCESS: AXError = 0;
    const AX_VALUE_CGPOINT_TYPE: i32 = 1;
    const AX_VALUE_CGSIZE_TYPE: i32 = 2;

    pub(super) fn snapshot(
        request: DesktopSidecarAccessibilitySnapshotRequest,
    ) -> Result<DesktopSidecarAccessibilitySnapshotPayload, DesktopSidecarErrorBody> {
        if !accessibility_permission_granted() {
            return Ok(DesktopSidecarAccessibilitySnapshotPayload {
                performed: false,
                target: None,
                rows: Vec::new(),
                truncated: false,
                redacted: false,
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
            redacted: false,
        };

        let app_row = snapshot_row(
            "macos_accessibility_app",
            &target.app,
            0,
            None,
            &mut context.redacted,
        );
        context.push(app_row);

        let windows = target.windows();
        if windows.is_empty() {
            return Ok(DesktopSidecarAccessibilitySnapshotPayload {
                performed: true,
                target: Some(target.snapshot_target()),
                rows: context.rows,
                truncated: context.truncated,
                redacted: context.redacted,
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
            let row = snapshot_row(
                "macos_accessibility_window",
                &window,
                0,
                Some(index),
                &mut context.redacted,
            );
            context.push(row);
            snapshot_children(&mut context, &window, 1);
        }

        Ok(DesktopSidecarAccessibilitySnapshotPayload {
            performed: true,
            target: Some(target.snapshot_target()),
            rows: context.rows,
            truncated: context.truncated,
            redacted: context.redacted,
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
            element: Some(describe_element(&element, request.x, request.y)),
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
            .filter(|value| !value.is_empty())
            .ok_or_else(|| schema_error("value"))?;
        let element = control_element(request)?;
        set_string_attribute(&element, "AXValue", value)
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
        redacted: bool,
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
        let mut redacted = false;
        Ok(SnapshotTarget {
            pid: element_pid(&app),
            app_name: redacted_attribute(&app, "AXTitle", &mut redacted),
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
            .map(|title| redact_sensitive_label(&title));
        let app_name = focused_window
            .app_name()
            .ok()
            .map(|name| redact_sensitive_label(&name));
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
                .map(|title| redact_sensitive_label(&title));
            let app_name = window
                .app_name()
                .ok()
                .map(|name| redact_sensitive_label(&name));
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

    fn snapshot_children(context: &mut SnapshotContext, element: &AxElement, depth: usize) {
        if !context.include_children || depth > context.max_depth || context.is_full() {
            return;
        }
        let children = ax_element_array_attribute(element, "AXChildren");
        for (index, child) in children.into_iter().enumerate() {
            if context.is_full() {
                context.truncated = true;
                break;
            }
            let row = snapshot_row(
                "macos_accessibility_element",
                &child,
                depth,
                Some(index),
                &mut context.redacted,
            );
            context.push(row);
            snapshot_children(context, &child, depth + 1);
        }
    }

    fn snapshot_row(
        row_type: &str,
        element: &AxElement,
        depth: usize,
        child_index: Option<usize>,
        redacted: &mut bool,
    ) -> DesktopSidecarAccessibilitySnapshotRow {
        let element = describe_element_with_redaction(element, 0, 0, redacted);
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

    fn control_element(
        request: &DesktopSidecarControlRequest,
    ) -> Result<AxElement, DesktopSidecarErrorBody> {
        let (x, y) = control_target_point(request)?;
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
        element_at_position(&system_wide, x, y)
    }

    fn control_target_point(
        request: &DesktopSidecarControlRequest,
    ) -> Result<(i32, i32), DesktopSidecarErrorBody> {
        if let (Some(x), Some(y)) = (request.x, request.y) {
            if x >= 0 && y >= 0 {
                return Ok((x, y));
            }
        }
        let Some(element_id) = request.element_id.as_deref() else {
            return Err(schema_error("elementId or x/y"));
        };
        parse_element_id_point(element_id).ok_or_else(|| schema_error("elementId"))
    }

    fn parse_element_id_point(element_id: &str) -> Option<(i32, i32)> {
        let mut parts = element_id.rsplit(':');
        let y = parts.next()?.parse::<i32>().ok()?;
        let x = parts.next()?.parse::<i32>().ok()?;
        (element_id.starts_with("macos_ax:") && x >= 0 && y >= 0).then_some((x, y))
    }

    fn describe_element(
        element: &AxElement,
        hit_x: i32,
        hit_y: i32,
    ) -> DesktopSidecarAccessibilityElement {
        let mut redacted = false;
        describe_element_with_redaction(element, hit_x, hit_y, &mut redacted)
    }

    fn describe_element_with_redaction(
        element: &AxElement,
        hit_x: i32,
        hit_y: i32,
        redacted: &mut bool,
    ) -> DesktopSidecarAccessibilityElement {
        let role = ax_string_attribute(element, "AXRole");
        let title = redacted_attribute(element, "AXTitle", redacted);
        let value = redacted_attribute(element, "AXValue", redacted);
        let description = redacted_attribute(element, "AXDescription", redacted);
        let enabled = ax_bool_attribute(element, "AXEnabled");
        let focused = ax_bool_attribute(element, "AXFocused");
        let position = ax_point_attribute(element, "AXPosition");
        let size = ax_size_attribute(element, "AXSize");
        let x = position.map(|point| point.x.round() as i32);
        let y = position.map(|point| point.y.round() as i32);
        let width = size.map(|size| size.width.max(0.0).round() as u32);
        let height = size.map(|size| size.height.max(0.0).round() as u32);
        let pid = element_pid(element);
        let geometry = AxElementGeometry {
            x,
            y,
            width,
            height,
        };
        DesktopSidecarAccessibilityElement {
            element_id: element_id(pid, role.as_deref(), geometry, hit_x, hit_y),
            pid,
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

    struct AxElementGeometry {
        x: Option<i32>,
        y: Option<i32>,
        width: Option<u32>,
        height: Option<u32>,
    }

    fn element_id(
        pid: Option<u32>,
        role: Option<&str>,
        geometry: AxElementGeometry,
        hit_x: i32,
        hit_y: i32,
    ) -> String {
        format!(
            "macos_ax:{}:{}:{}:{}:{}:{}:{}:{}",
            pid.unwrap_or_default(),
            role.unwrap_or("element"),
            geometry.x.unwrap_or(hit_x),
            geometry.y.unwrap_or(hit_y),
            geometry.width.unwrap_or_default(),
            geometry.height.unwrap_or_default(),
            hit_x,
            hit_y
        )
    }

    fn redacted_attribute(
        element: &AxElement,
        attribute: &str,
        redacted: &mut bool,
    ) -> Option<String> {
        ax_string_attribute(element, attribute).map(|value| {
            let redacted_value = redact_sensitive_label(&value);
            *redacted |= redacted_value != value && redacted_value.contains("[redacted");
            redacted_value
        })
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
            Err(ax_action_error("set Accessibility focus", status))
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

    fn ax_action_error(action: &str, status: AXError) -> DesktopSidecarErrorBody {
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
        fn AXValueGetTypeID() -> CFTypeID;
        fn AXValueGetType(value: AXValueRef) -> i32;
        fn AXValueGetValue(value: AXValueRef, value_type: i32, value: *mut c_void) -> bool;
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
        let mut clipboard = arboard::Clipboard::new().map_err(|error| {
            DesktopSidecarErrorBody::new(
                "permission_clipboard_denied",
                format!("Could not open the system clipboard for paste: {error}"),
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
        })?;
        hotkey(&["control".into(), "v".into()])
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

#[cfg(target_os = "macos")]
mod macos_ocr {
    use objc2::{rc::autoreleasepool, runtime::AnyObject, AnyThread};
    use objc2_foundation::{NSArray, NSData, NSDictionary, NSError};
    use objc2_vision::{
        VNImageOption, VNImageRequestHandler, VNRecognizeTextRequest, VNRequest,
        VNRequestTextRecognitionLevel,
    };

    use super::{
        redact_sensitive_label, CapturedDesktopImage, DesktopSidecarErrorBody,
        DesktopSidecarOcrSnapshotPayload, DesktopSidecarOcrTextBlock,
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
        let mut redacted = false;

        for observation in observations.into_iter().take(limit) {
            let candidates = observation.topCandidates(1);
            let Some(candidate) = candidates.firstObject() else {
                continue;
            };
            let raw_text = candidate.string().to_string();
            if raw_text.trim().is_empty() {
                continue;
            }
            let text = redact_sensitive_label(&raw_text);
            redacted |= text != raw_text;
            let bbox = unsafe { observation.boundingBox() };
            text_blocks.push(text_block_from_bbox(
                text,
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
            redactions_applied: capture.redactions_applied,
            text_blocks,
            full_text,
            truncated,
            redacted,
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

#[cfg(target_os = "macos")]
mod macos_clipboard {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;

    use super::{macos_input, DesktopSidecarErrorBody};

    pub(super) fn paste_text(text: &str) -> Result<(), DesktopSidecarErrorBody> {
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
        macos_input::hotkey(&["command".into(), "v".into()])
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
    fn redact_sensitive_label_removes_secret_window_titles() {
        assert_eq!(
            redact_sensitive_label("Password reset token"),
            "[redacted sensitive desktop label]"
        );
    }

    #[test]
    fn sidecar_capabilities_match_implemented_operations() {
        let capabilities = sidecar_capabilities();

        assert!(capabilities.display_list);
        assert!(capabilities.window_list);
        assert!(capabilities.app_list);
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
            cfg!(target_os = "macos")
        );
        assert_eq!(capabilities.ocr_snapshot, cfg!(target_os = "macos"));
        assert!(capabilities.screenshot_fallback_stream);
        assert_eq!(capabilities.mouse_input, native_input_platform);
        assert_eq!(capabilities.keyboard_input, native_input_platform);
        assert_eq!(capabilities.clipboard, native_input_platform);
        assert_eq!(capabilities.manual_cloud_control, native_input_platform);
        assert_eq!(
            capabilities.accessibility_actions,
            cfg!(target_os = "macos")
        );
        assert_eq!(capabilities.menu_select, cfg!(target_os = "macos"));
        assert_eq!(capabilities.webrtc_stream, cfg!(target_os = "macos"));
    }

    #[test]
    fn sidecar_permissions_include_platform_requirement_rows() {
        let permissions = sidecar_permissions().permissions;
        let names = permissions
            .iter()
            .map(|permission| permission.name.as_str())
            .collect::<Vec<_>>();

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

        assert_eq!(capabilities.webrtc_stream, cfg!(target_os = "macos"));
        assert!(capabilities.screenshot_fallback_stream);
        assert_eq!(capabilities.native_video_track, cfg!(target_os = "macos"));
        assert_eq!(
            capabilities.preferred_codec.as_deref(),
            Some(MIME_TYPE_H264)
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
                .encoder_backends
                .iter()
                .any(|backend| backend == "videotoolbox_h264"),
            cfg!(target_os = "macos")
        );
        assert_eq!(capabilities.hardware_encoding, cfg!(target_os = "macos"));
        assert_eq!(capabilities.max_width, WEBRTC_MAX_WIDTH);
        assert_eq!(capabilities.max_frame_rate, WEBRTC_MAX_FRAME_RATE);
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

    #[cfg(not(target_os = "macos"))]
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
                redaction: None,
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

            video_track
                .write_sample(&Sample {
                    data: vec![0, 0, 0, 1, 0x65, 0x88, 0x84].into(),
                    duration: Duration::from_millis(42),
                    ..Default::default()
                })
                .await
                .expect("write h264 sample");

            let mime = tokio::time::timeout(Duration::from_secs(5), track_rx.recv())
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
    fn screenshot_redaction_blacks_requested_private_regions() {
        let mut image = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]));
        let redaction = xero_desktop_control_ipc::DesktopSidecarRedactionRequest {
            mode: xero_desktop_control_ipc::DesktopSidecarRedactionMode::Balanced,
            private_regions: vec![xero_desktop_control_ipc::DesktopSidecarRegion {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            }],
        };

        let applied = apply_private_region_redactions(&mut image, Some(&redaction));

        assert_eq!(applied, 1);
        assert_eq!(*image.get_pixel(1, 1), Rgba([0, 0, 0, 255]));
        assert_eq!(*image.get_pixel(2, 2), Rgba([0, 0, 0, 255]));
        assert_eq!(*image.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
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
    fn paste_text_requires_text() {
        let error = sidecar_control(
            DesktopSidecarOperation::PasteText,
            serde_json::json!({ "text": "" }),
        )
        .expect_err("missing paste text");

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
        assert!(macos_input::key_code_for("definitely-not-a-key").is_none());
    }
}
