//! Emulator sidebar backend — iOS Simulator and Android Emulator bring-up.
//!
//! Phase 2 scaffolded the frame pipeline (FrameBus + `emulator://` URI
//! scheme) and a synthetic frame driver. Phase 3 wires in the real Android
//! pipeline (emulator process + scrcpy). Phase 4 adds the iOS pipeline.

pub mod android;
pub mod automation;
pub mod codec;
pub mod decoder;
pub mod events;
pub mod frame_bus;
pub mod ios;
pub mod process;
pub mod sdk;
pub mod shutdown;
#[cfg(feature = "emulator-synthetic")]
pub mod synthetic;
pub mod uri_scheme;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::commands::{CommandError, CommandResult};

pub use android::provision::{
    emulator_android_provision, emulator_android_provision_status, EMULATOR_PROVISION_EVENT,
};
pub use events::{
    FramePayload, StatusPayload, StatusPhase, EMULATOR_FRAME_EVENT,
    EMULATOR_SDK_STATUS_CHANGED_EVENT, EMULATOR_STATUS_EVENT,
};
pub use frame_bus::{Frame, FrameBus};
pub use sdk::{probe_sdks, AndroidSdkStatus, IosSdkStatus, SdkStatus};
pub use uri_scheme::{handle as handle_uri_scheme, URI_SCHEME};

use automation::{
    AppDescriptor, BundleIdRequest, HardwareKeyRequest, InstallAppRequest, LaunchAppRequest,
    LocationRequest, LogSubscribeRequest, PushNotificationRequest, ScreenshotResponse, Selector,
    SubscriptionToken, SwipeRequest, TapTarget, TypeRequest, UiTree,
};
use automation::metro_inspector::{
    ElementInfo, MetroInspector, MetroStatus, METRO_PORT_RANGE,
};

/// Process-wide emulator state. Holds the FrameBus (shared with the URI
/// scheme handler) and the single active device session, if any.
pub struct EmulatorState {
    frame_bus: Arc<FrameBus>,
    active: Mutex<Option<ActiveDevice>>,
    log_collector: automation::logs::LogCollector,
    log_stream: Mutex<Option<LogStreamHandle>>,
    /// Metro inspector bridge (React Native / Expo).
    metro_inspector: Mutex<Option<MetroInspector>>,
}

enum LogStreamHandle {
    // Variant is held only for its Drop impl (kills the logcat child).
    Android(#[allow(dead_code)] automation::logs::AndroidLogStream),
}

impl Default for EmulatorState {
    fn default() -> Self {
        Self {
            frame_bus: Arc::new(FrameBus::new()),
            active: Mutex::new(None),
            log_collector: automation::logs::LogCollector::new(),
            log_stream: Mutex::new(None),
            metro_inspector: Mutex::new(None),
        }
    }
}

impl EmulatorState {
    pub fn frame_bus(&self) -> Arc<FrameBus> {
        Arc::clone(&self.frame_bus)
    }
}

/// Platform tag shared between the frontend and backend.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EmulatorPlatform {
    Ios,
    Android,
}

impl EmulatorPlatform {
    pub fn as_str(self) -> &'static str {
        match self {
            EmulatorPlatform::Ios => "ios",
            EmulatorPlatform::Android => "android",
        }
    }
}

/// The backing session for the currently-running device.
enum ActiveDevice {
    Android {
        device_id: String,
        session: android::AndroidSession,
    },
    #[cfg(target_os = "macos")]
    Ios {
        device_id: String,
        session: ios::IosSession,
    },
    #[cfg(feature = "emulator-synthetic")]
    Synthetic {
        platform: EmulatorPlatform,
        device_id: String,
        // Dropping this joins the producer thread.
        #[allow(dead_code)]
        session: synthetic::SyntheticSession,
    },
}

impl ActiveDevice {
    fn platform(&self) -> EmulatorPlatform {
        match self {
            ActiveDevice::Android { .. } => EmulatorPlatform::Android,
            #[cfg(target_os = "macos")]
            ActiveDevice::Ios { .. } => EmulatorPlatform::Ios,
            #[cfg(feature = "emulator-synthetic")]
            ActiveDevice::Synthetic { platform, .. } => *platform,
        }
    }

    fn device_id(&self) -> &str {
        match self {
            ActiveDevice::Android { device_id, .. } => device_id,
            #[cfg(target_os = "macos")]
            ActiveDevice::Ios { device_id, .. } => device_id,
            #[cfg(feature = "emulator-synthetic")]
            ActiveDevice::Synthetic { device_id, .. } => device_id,
        }
    }
}

// ---------- Request/response shapes ----------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmulatorStartRequest {
    pub platform: EmulatorPlatform,
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmulatorStartResponse {
    pub platform: EmulatorPlatform,
    pub device_id: String,
    pub width: u32,
    pub height: u32,
    pub device_pixel_ratio: f32,
    pub frame_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmulatorInputRequest {
    pub kind: InputKind,
    /// Normalized 0..1 against the device resolution.
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub button: Option<String>,
    #[serde(default)]
    pub dx: Option<f32>,
    #[serde(default)]
    pub dy: Option<f32>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    TouchDown,
    TouchMove,
    TouchUp,
    Scroll,
    Key,
    Text,
    HwButton,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmulatorRotateRequest {
    pub orientation: Orientation,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmulatorListDevicesRequest {
    pub platform: EmulatorPlatform,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDescriptor {
    pub id: String,
    pub display_name: String,
    pub kind: DeviceKind,
    pub width: u32,
    pub height: u32,
    pub device_pixel_ratio: f32,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Phone,
    Tablet,
}

// ---------- Tauri commands --------------------------------------------------

#[tauri::command]
pub fn emulator_sdk_status<R: Runtime>(app: AppHandle<R>) -> CommandResult<SdkStatus> {
    Ok(probe_sdks(&app))
}

/// macOS only — trigger the system Accessibility-permission prompt.
/// Returns the current permission state after the call. On non-macOS
/// hosts always returns `false` (iOS isn't reachable there anyway).
#[tauri::command]
pub fn emulator_ios_request_ax_permission<R: Runtime>(app: AppHandle<R>) -> CommandResult<bool> {
    #[cfg(target_os = "macos")]
    {
        let granted = ios::cg_input::request_ax_permission();
        // Fire the sdk-status-changed event so the frontend re-probes and
        // hides its banner the moment the user flips the Accessibility
        // toggle — without this, the panel would sit stale until the next
        // manual probe.
        let _ = app.emit(EMULATOR_SDK_STATUS_CHANGED_EVENT, ());
        Ok(granted)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(false)
    }
}

/// Open the Privacy & Security → Accessibility pane in System Settings so
/// the user can enable Xero. macOS-only; on other hosts this is a
/// no-op.
#[tauri::command]
pub fn emulator_ios_open_accessibility_settings() -> CommandResult<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        // `x-apple.systempreferences:` is the documented URL scheme for
        // deep-linking to a specific settings pane. The pane anchor is
        // the preference bundle id + a `Privacy_*` query parameter.
        Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .status()
            .map_err(|e| {
                CommandError::system_fault(
                    "ios_open_accessibility_failed",
                    format!("could not launch System Settings: {e}"),
                )
            })?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

/// macOS only — trigger the system Screen Recording permission prompt.
/// Required by ScreenCaptureKit for the Swift helper's frame capture.
/// Returns the current permission state after the call.
#[tauri::command]
pub fn emulator_ios_request_screen_recording_permission<R: Runtime>(
    app: AppHandle<R>,
) -> CommandResult<bool> {
    #[cfg(target_os = "macos")]
    {
        let granted = ios::cg_input::request_screen_recording_permission();
        let _ = app.emit(EMULATOR_SDK_STATUS_CHANGED_EVENT, ());
        Ok(granted)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(false)
    }
}

/// Open the Privacy & Security → Screen Recording pane in System Settings
/// so the user can enable Xero. macOS-only; on other hosts this is a no-op.
#[tauri::command]
pub fn emulator_ios_open_screen_recording_settings() -> CommandResult<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .status()
            .map_err(|e| {
                CommandError::system_fault(
                    "ios_open_screen_recording_failed",
                    format!("could not launch System Settings: {e}"),
                )
            })?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

#[tauri::command]
pub fn emulator_list_devices<R: Runtime>(
    app: AppHandle<R>,
    request: EmulatorListDevicesRequest,
) -> CommandResult<Vec<DeviceDescriptor>> {
    match request.platform {
        EmulatorPlatform::Android => {
            #[allow(unused_mut)]
            let mut out: Vec<DeviceDescriptor> = android::list_devices(&app)
                .into_iter()
                .map(|avd| DeviceDescriptor {
                    id: avd.name,
                    display_name: avd.display_name,
                    kind: match avd.kind {
                        android::avd::AvdKind::Phone => DeviceKind::Phone,
                        android::avd::AvdKind::Tablet => DeviceKind::Tablet,
                    },
                    width: avd.width.unwrap_or(0),
                    height: avd.height.unwrap_or(0),
                    device_pixel_ratio: avd.density.map(|d| d as f32 / 160.0).unwrap_or(2.0),
                })
                .collect();

            #[cfg(feature = "emulator-synthetic")]
            if out.is_empty() {
                out.push(DeviceDescriptor {
                    id: "synthetic-pixel".to_string(),
                    display_name: "Synthetic Pixel".to_string(),
                    kind: DeviceKind::Phone,
                    width: synthetic::synthetic_width(),
                    height: synthetic::synthetic_height(),
                    device_pixel_ratio: 2.0,
                });
            }

            Ok(out)
        }
        EmulatorPlatform::Ios => {
            #[cfg(target_os = "macos")]
            {
                #[allow(unused_mut)]
                let mut out: Vec<DeviceDescriptor> = ios::list_devices()
                    .into_iter()
                    .map(|sim| DeviceDescriptor {
                        id: sim.udid,
                        display_name: sim.display_name,
                        kind: if sim.is_tablet {
                            DeviceKind::Tablet
                        } else {
                            DeviceKind::Phone
                        },
                        width: sim.width.unwrap_or(0),
                        height: sim.height.unwrap_or(0),
                        device_pixel_ratio: sim.scale.unwrap_or(3.0),
                    })
                    .collect();

                #[cfg(feature = "emulator-synthetic")]
                if out.is_empty() {
                    out.push(DeviceDescriptor {
                        id: "synthetic-iphone".to_string(),
                        display_name: "Synthetic iPhone".to_string(),
                        kind: DeviceKind::Phone,
                        width: synthetic::synthetic_width(),
                        height: synthetic::synthetic_height(),
                        device_pixel_ratio: 3.0,
                    });
                }

                Ok(out)
            }
            #[cfg(not(target_os = "macos"))]
            {
                Ok(Vec::new())
            }
        }
    }
}

#[tauri::command]
pub fn emulator_start<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, EmulatorState>,
    request: EmulatorStartRequest,
) -> CommandResult<EmulatorStartResponse> {
    if request.device_id.trim().is_empty() {
        return Err(CommandError::invalid_request("deviceId"));
    }

    // Single-active-device invariant — shut down any previous session first.
    stop_active(&app, &state)?;

    #[cfg(feature = "emulator-synthetic")]
    if request.device_id.starts_with("synthetic-") {
        return start_synthetic(&app, &state, request);
    }

    match request.platform {
        EmulatorPlatform::Android => start_android(&app, &state, request),
        EmulatorPlatform::Ios => start_ios(&app, &state, request),
    }
}

fn start_android<R: Runtime>(
    app: &AppHandle<R>,
    state: &State<'_, EmulatorState>,
    request: EmulatorStartRequest,
) -> CommandResult<EmulatorStartResponse> {
    let scrcpy_jar = android::scrcpy::bundled_jar_path(app).map_err(|err| {
        CommandError::user_fixable(
            "scrcpy_jar_missing",
            format!(
                "scrcpy-server.jar is not bundled with this Xero build: {err}. Drop the jar \
                 into client/src-tauri/resources/ and rebuild."
            ),
        )
    })?;

    let session = android::spawn(android::SpawnArgs {
        app: app.clone(),
        frame_bus: state.frame_bus(),
        device_id: request.device_id.clone(),
        scrcpy_jar,
    })?;

    let width = session.width();
    let height = session.height();

    let mut active = state.active.lock().expect("emulator active mutex poisoned");
    *active = Some(ActiveDevice::Android {
        device_id: request.device_id.clone(),
        session,
    });

    Ok(EmulatorStartResponse {
        platform: request.platform,
        device_id: request.device_id,
        width,
        height,
        device_pixel_ratio: 2.0,
        frame_url: "emulator://localhost/frame".to_string(),
    })
}

#[cfg(target_os = "macos")]
fn start_ios<R: Runtime>(
    app: &AppHandle<R>,
    state: &State<'_, EmulatorState>,
    request: EmulatorStartRequest,
) -> CommandResult<EmulatorStartResponse> {
    let session = ios::spawn(ios::SpawnArgs {
        app: app.clone(),
        frame_bus: state.frame_bus(),
        device_id: request.device_id.clone(),
    })?;

    let width = session.width();
    let height = session.height();

    let mut active = state.active.lock().expect("emulator active mutex poisoned");
    *active = Some(ActiveDevice::Ios {
        device_id: request.device_id.clone(),
        session,
    });

    Ok(EmulatorStartResponse {
        platform: request.platform,
        device_id: request.device_id,
        width,
        height,
        device_pixel_ratio: 3.0,
        frame_url: "emulator://localhost/frame".to_string(),
    })
}

#[cfg(not(target_os = "macos"))]
fn start_ios<R: Runtime>(
    _app: &AppHandle<R>,
    _state: &State<'_, EmulatorState>,
    _request: EmulatorStartRequest,
) -> CommandResult<EmulatorStartResponse> {
    Err(CommandError::user_fixable(
        "ios_unsupported",
        "iOS Simulator is only available on macOS.",
    ))
}

#[cfg(feature = "emulator-synthetic")]
fn start_synthetic<R: Runtime>(
    app: &AppHandle<R>,
    state: &State<'_, EmulatorState>,
    request: EmulatorStartRequest,
) -> CommandResult<EmulatorStartResponse> {
    let _ = app.emit(
        EMULATOR_STATUS_EVENT,
        StatusPayload::new(StatusPhase::Booting)
            .with_platform(request.platform.as_str())
            .with_device(&request.device_id)
            .with_message("starting synthetic frame source"),
    );

    let session = synthetic::SyntheticSession::spawn(
        app.clone(),
        state.frame_bus(),
        request.platform.as_str().to_string(),
        request.device_id.clone(),
    );

    let mut active = state.active.lock().expect("emulator active mutex poisoned");
    *active = Some(ActiveDevice::Synthetic {
        platform: request.platform,
        device_id: request.device_id.clone(),
        session,
    });

    Ok(EmulatorStartResponse {
        platform: request.platform,
        device_id: request.device_id,
        width: synthetic::synthetic_width(),
        height: synthetic::synthetic_height(),
        device_pixel_ratio: 2.0,
        frame_url: "emulator://localhost/frame".to_string(),
    })
}

#[tauri::command]
pub fn emulator_stop<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, EmulatorState>,
) -> CommandResult<()> {
    stop_active(&app, &state)
}

#[tauri::command]
pub fn emulator_input(
    state: State<'_, EmulatorState>,
    request: EmulatorInputRequest,
) -> CommandResult<()> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    let active = active.as_ref().ok_or_else(|| {
        CommandError::user_fixable(
            "emulator_not_running",
            "No emulator device is currently running.",
        )
    })?;

    match active {
        ActiveDevice::Android { session, .. } => dispatch_android_input(session, &request),
        #[cfg(target_os = "macos")]
        ActiveDevice::Ios { session, .. } => dispatch_ios_input(session, &request),
        #[cfg(feature = "emulator-synthetic")]
        ActiveDevice::Synthetic { .. } => Ok(()),
    }
}

fn dispatch_android_input(
    session: &android::AndroidSession,
    request: &EmulatorInputRequest,
) -> CommandResult<()> {
    use android::input::MotionAction;

    match request.kind {
        InputKind::TouchDown => {
            let x = request.x.unwrap_or(0.0);
            let y = request.y.unwrap_or(0.0);
            session.send_touch(MotionAction::Down, x, y)
        }
        InputKind::TouchMove => {
            let x = request.x.unwrap_or(0.0);
            let y = request.y.unwrap_or(0.0);
            session.send_touch(MotionAction::Move, x, y)
        }
        InputKind::TouchUp => {
            let x = request.x.unwrap_or(0.0);
            let y = request.y.unwrap_or(0.0);
            session.send_touch(MotionAction::Up, x, y)
        }
        InputKind::Scroll => {
            let x = request.x.unwrap_or(0.5);
            let y = request.y.unwrap_or(0.5);
            let dx = (request.dx.unwrap_or(0.0) * 32.0) as i16;
            let dy = (request.dy.unwrap_or(0.0) * 32.0) as i16;
            session.send_scroll(x, y, dx, dy)
        }
        InputKind::Key | InputKind::HwButton => {
            let name = request
                .button
                .as_deref()
                .or(request.key.as_deref())
                .unwrap_or("");
            let keycode = map_hardware_key(name).ok_or_else(|| {
                CommandError::user_fixable(
                    "emulator_unknown_key",
                    format!("Unknown hardware key: {name}"),
                )
            })?;
            session.send_key(keycode)
        }
        InputKind::Text => {
            let text = request.text.as_deref().unwrap_or("");
            session.send_text(text)
        }
    }
}

#[cfg(target_os = "macos")]
fn dispatch_ios_input(
    session: &ios::IosSession,
    request: &EmulatorInputRequest,
) -> CommandResult<()> {
    session.dispatch(request)
}

fn map_hardware_key(name: &str) -> Option<android::input::Keycode> {
    use android::input::Keycode;
    match name {
        "home" => Some(Keycode::Home),
        "back" => Some(Keycode::Back),
        "recents" | "app_switch" | "menu" => Some(Keycode::AppSwitch),
        "vol_up" | "volume_up" => Some(Keycode::VolumeUp),
        "vol_down" | "volume_down" => Some(Keycode::VolumeDown),
        "power" | "lock" => Some(Keycode::Power),
        "enter" => Some(Keycode::Enter),
        "backspace" | "delete" | "del" => Some(Keycode::Del),
        "tab" => Some(Keycode::Tab),
        "escape" => Some(Keycode::Escape),
        "search" => Some(Keycode::Search),
        "dpad_left" | "left" => Some(Keycode::DpadLeft),
        "dpad_right" | "right" => Some(Keycode::DpadRight),
        "dpad_up" | "up" => Some(Keycode::DpadUp),
        "dpad_down" | "down" => Some(Keycode::DpadDown),
        _ => None,
    }
}

#[tauri::command]
pub fn emulator_rotate(
    state: State<'_, EmulatorState>,
    request: EmulatorRotateRequest,
) -> CommandResult<()> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    let active = active.as_ref().ok_or_else(|| {
        CommandError::user_fixable(
            "emulator_not_running",
            "No emulator device is currently running.",
        )
    })?;

    match active {
        ActiveDevice::Android { session, .. } => {
            let rotation = match request.orientation {
                Orientation::Portrait => 0,
                Orientation::Landscape => 1,
            };
            session.send_rotate(rotation)
        }
        #[cfg(target_os = "macos")]
        ActiveDevice::Ios { session, .. } => session.set_orientation(request.orientation),
        #[cfg(feature = "emulator-synthetic")]
        ActiveDevice::Synthetic { .. } => Ok(()),
    }
}

#[tauri::command]
pub fn emulator_subscribe_ready<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, EmulatorState>,
) -> CommandResult<SubscribeReadyResponse> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    let status = match active.as_ref() {
        Some(device) => StatusPayload::new(StatusPhase::Streaming)
            .with_platform(device.platform().as_str())
            .with_device(device.device_id().to_string()),
        None => StatusPayload::new(StatusPhase::Stopped),
    };
    // Hand the frontend the current frame seq (if any) alongside the
    // status. This closes a race on session startup: the backend can
    // publish its first frame before the frontend's `listen` finishes
    // registering, which would otherwise leave the UI stuck on
    // "Waiting for first frame" even though FrameBus has something
    // ready to serve.
    let frame = state.frame_bus().latest().map(|f| FramePayload {
        seq: f.seq,
        width: f.width,
        height: f.height,
    });

    let _ = app.emit(EMULATOR_STATUS_EVENT, status.clone());
    Ok(SubscribeReadyResponse { status, frame })
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeReadyResponse {
    pub status: StatusPayload,
    pub frame: Option<FramePayload>,
}

// ---------- Helpers ---------------------------------------------------------

fn stop_active<R: Runtime>(
    app: &AppHandle<R>,
    state: &State<'_, EmulatorState>,
) -> CommandResult<()> {
    // Stop any active log stream first so its child process doesn't outlive
    // the emulator session.
    {
        let mut stream = state.log_stream.lock().expect("log stream mutex poisoned");
        *stream = None;
    }

    let mut active = state.active.lock().expect("emulator active mutex poisoned");
    let taken = active.take();
    drop(active);

    let (platform, device_id) = match &taken {
        Some(device) => (
            Some(device.platform().as_str().to_string()),
            Some(device.device_id().to_string()),
        ),
        None => (None, None),
    };

    if taken.is_some() {
        let _ = app.emit(
            EMULATOR_STATUS_EVENT,
            StatusPayload {
                phase: StatusPhase::Stopping,
                platform: platform.clone(),
                device_id: device_id.clone(),
                message: None,
            },
        );
    }

    drop(taken); // Joins the decoder thread + kills child processes.

    state.frame_bus().clear();
    state.log_collector.clear();

    let _ = app.emit(
        EMULATOR_STATUS_EVENT,
        StatusPayload {
            phase: StatusPhase::Stopped,
            platform,
            device_id,
            message: None,
        },
    );
    Ok(())
}

// ---------- Automation commands --------------------------------------------

#[tauri::command]
pub fn emulator_screenshot(state: State<'_, EmulatorState>) -> CommandResult<ScreenshotResponse> {
    let frame = state.frame_bus().latest().ok_or_else(|| {
        CommandError::user_fixable(
            "emulator_no_frame",
            "No frame has been captured yet. Wait for the stream to start.",
        )
    })?;

    // `FrameBus` stores JPEG; re-encode once as PNG so agents get a
    // lossless copy they can pipe into a vision model without worrying
    // about JPEG artifacts.
    let img = image::load_from_memory_with_format(&frame.bytes, image::ImageFormat::Jpeg).map_err(
        |e| {
            CommandError::system_fault(
                "emulator_frame_decode_failed",
                format!("could not decode live frame: {e}"),
            )
        },
    )?;

    let mut png_bytes: Vec<u8> = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .map_err(|e| {
        CommandError::system_fault(
            "emulator_frame_encode_failed",
            format!("png encode failed: {e}"),
        )
    })?;

    Ok(ScreenshotResponse {
        png_base64: base64::engine::general_purpose::STANDARD.encode(&png_bytes),
        width: frame.width,
        height: frame.height,
        device_pixel_ratio: 1.0,
    })
}

#[tauri::command]
pub fn emulator_ui_dump(state: State<'_, EmulatorState>) -> CommandResult<UiTree> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    match active.as_ref() {
        Some(ActiveDevice::Android { session, .. }) => automation::android_ui::dump(session.adb())
            .map_err(|err| {
                CommandError::system_fault(
                    "android_ui_dump_failed",
                    format!("uiautomator dump failed: {err}"),
                )
            }),
        #[cfg(target_os = "macos")]
        Some(ActiveDevice::Ios { session, .. }) => {
            let helper = session.helper_client();
            let idb = session.client();
            automation::ios_ui::dump(
                helper.as_deref(),
                idb.as_deref(),
            )
        }
        #[cfg(feature = "emulator-synthetic")]
        Some(ActiveDevice::Synthetic { .. }) => Err(CommandError::user_fixable(
            "synthetic_ui_dump_unsupported",
            "UI dumps are not available for the synthetic device. Start a real AVD or simulator.",
        )),
        None => Err(no_active_device()),
    }
}

#[tauri::command]
pub fn emulator_find(
    state: State<'_, EmulatorState>,
    selector: Selector,
) -> CommandResult<Vec<automation::UiNode>> {
    let tree = emulator_ui_dump(state)?;
    Ok(automation::selector::find(&tree, &selector))
}

#[tauri::command]
pub fn emulator_tap(state: State<'_, EmulatorState>, target: TapTarget) -> CommandResult<()> {
    match target {
        TapTarget::Point { x, y } => tap_point(&state, x, y),
        TapTarget::Element { selector } => {
            let tree = emulator_ui_dump(state.clone())?;
            let hits = automation::selector::find(&tree, &selector);
            match hits.len() {
                0 => Err(CommandError::user_fixable(
                    "emulator_selector_no_match",
                    "Selector matched no elements.",
                )),
                1 => {
                    let (cx, cy) = hits[0].bounds.center();
                    // Resolve the tap at the exact pixel bounds found by the
                    // dump. We must normalize back to 0..1 for the session's
                    // dispatch which expects normalized coords.
                    tap_element(&state, cx, cy)
                }
                n => Err(CommandError::user_fixable(
                    "emulator_selector_ambiguous",
                    format!("Selector matched {n} elements. Narrow it with an id or label."),
                )),
            }
        }
    }
}

fn tap_point(state: &State<'_, EmulatorState>, x: f32, y: f32) -> CommandResult<()> {
    synthetic_down_up(state, x, y)
}

fn tap_element(state: &State<'_, EmulatorState>, px: i32, py: i32) -> CommandResult<()> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    let device = active.as_ref().ok_or_else(no_active_device)?;
    let (w, h) = device.viewport();
    if w == 0 || h == 0 {
        return Err(CommandError::system_fault(
            "emulator_viewport_unknown",
            "Device viewport dimensions are unknown; cannot resolve selector bounds.",
        ));
    }
    let nx = (px as f32 / w as f32).clamp(0.0, 1.0);
    let ny = (py as f32 / h as f32).clamp(0.0, 1.0);
    drop(active);
    synthetic_down_up(state, nx, ny)
}

fn synthetic_down_up(state: &State<'_, EmulatorState>, x: f32, y: f32) -> CommandResult<()> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    let device = active.as_ref().ok_or_else(no_active_device)?;
    match device {
        ActiveDevice::Android { session, .. } => {
            use android::input::MotionAction;
            session.send_touch(MotionAction::Down, x, y)?;
            session.send_touch(MotionAction::Up, x, y)?;
            Ok(())
        }
        #[cfg(target_os = "macos")]
        ActiveDevice::Ios { session, .. } => session.tap(x, y),
        #[cfg(feature = "emulator-synthetic")]
        ActiveDevice::Synthetic { .. } => Ok(()),
    }
}

#[tauri::command]
pub fn emulator_swipe(state: State<'_, EmulatorState>, request: SwipeRequest) -> CommandResult<()> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    let device = active.as_ref().ok_or_else(no_active_device)?;
    match device {
        ActiveDevice::Android { session, .. } => {
            use android::input::MotionAction;
            session.send_touch(MotionAction::Down, request.from_x, request.from_y)?;
            // Issue a few interpolated Move events so the gesture reads as a
            // real swipe rather than a teleport.
            let steps = 6;
            for i in 1..=steps {
                let t = i as f32 / steps as f32;
                let x = request.from_x + (request.to_x - request.from_x) * t;
                let y = request.from_y + (request.to_y - request.from_y) * t;
                session.send_touch(MotionAction::Move, x, y)?;
            }
            session.send_touch(MotionAction::Up, request.to_x, request.to_y)?;
            Ok(())
        }
        #[cfg(target_os = "macos")]
        ActiveDevice::Ios { session, .. } => session.swipe(
            request.from_x,
            request.from_y,
            request.to_x,
            request.to_y,
            request.duration_ms.unwrap_or(200),
        ),
        #[cfg(feature = "emulator-synthetic")]
        ActiveDevice::Synthetic { .. } => Ok(()),
    }
}

#[tauri::command]
pub fn emulator_type(state: State<'_, EmulatorState>, request: TypeRequest) -> CommandResult<()> {
    if let Some(selector) = request.into.as_ref() {
        let tree = emulator_ui_dump(state.clone())?;
        let hits = automation::selector::find(&tree, selector);
        match hits.len() {
            0 => {
                return Err(CommandError::user_fixable(
                    "emulator_selector_no_match",
                    "Selector for `into` matched no elements.",
                ))
            }
            1 => {
                let (cx, cy) = hits[0].bounds.center();
                tap_element(&state, cx, cy)?;
            }
            n => {
                return Err(CommandError::user_fixable(
                    "emulator_selector_ambiguous",
                    format!("Selector for `into` matched {n} elements."),
                ))
            }
        }
    }

    let active = state.active.lock().expect("emulator active mutex poisoned");
    let device = active.as_ref().ok_or_else(no_active_device)?;
    match device {
        ActiveDevice::Android { session, .. } => session.send_text(&request.text),
        #[cfg(target_os = "macos")]
        ActiveDevice::Ios { session, .. } => session.send_text(&request.text),
        #[cfg(feature = "emulator-synthetic")]
        ActiveDevice::Synthetic { .. } => Ok(()),
    }
}

#[tauri::command]
pub fn emulator_press_key(
    state: State<'_, EmulatorState>,
    request: HardwareKeyRequest,
) -> CommandResult<()> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    let device = active.as_ref().ok_or_else(no_active_device)?;
    match device {
        ActiveDevice::Android { session, .. } => {
            let keycode = map_hardware_key(&request.key).ok_or_else(|| {
                CommandError::user_fixable(
                    "emulator_unknown_key",
                    format!("Unknown hardware key: {}", request.key),
                )
            })?;
            session.send_key(keycode)
        }
        #[cfg(target_os = "macos")]
        ActiveDevice::Ios { session, .. } => session.press_hardware_key(&request.key),
        #[cfg(feature = "emulator-synthetic")]
        ActiveDevice::Synthetic { .. } => Ok(()),
    }
}

// ---- App lifecycle commands ------------------------------------------------

#[tauri::command]
pub fn emulator_list_apps(state: State<'_, EmulatorState>) -> CommandResult<Vec<AppDescriptor>> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    match active.as_ref() {
        Some(ActiveDevice::Android { session, .. }) => {
            automation::apps::android_list(session.adb())
        }
        #[cfg(target_os = "macos")]
        Some(ActiveDevice::Ios { session, .. }) => automation::apps::ios_list(session.device_id()),
        #[cfg(feature = "emulator-synthetic")]
        Some(ActiveDevice::Synthetic { .. }) => Ok(Vec::new()),
        None => Err(no_active_device()),
    }
}

#[tauri::command]
pub fn emulator_install_app(
    state: State<'_, EmulatorState>,
    request: InstallAppRequest,
) -> CommandResult<AppDescriptor> {
    let path = PathBuf::from(request.source_path);
    if !path.exists() {
        return Err(CommandError::user_fixable(
            "emulator_install_source_missing",
            format!("{} does not exist.", path.display()),
        ));
    }
    let active = state.active.lock().expect("emulator active mutex poisoned");
    match active.as_ref() {
        Some(ActiveDevice::Android { session, .. }) => {
            automation::apps::android_install(session.adb(), &path)
        }
        #[cfg(target_os = "macos")]
        Some(ActiveDevice::Ios { session, .. }) => {
            automation::apps::ios_install(session.device_id(), &path)
        }
        #[cfg(feature = "emulator-synthetic")]
        Some(ActiveDevice::Synthetic { .. }) => Err(CommandError::user_fixable(
            "synthetic_app_unsupported",
            "App install is not available for the synthetic device.",
        )),
        None => Err(no_active_device()),
    }
}

#[tauri::command]
pub fn emulator_uninstall_app(
    state: State<'_, EmulatorState>,
    request: BundleIdRequest,
) -> CommandResult<()> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    match active.as_ref() {
        Some(ActiveDevice::Android { session, .. }) => {
            automation::apps::android_uninstall(session.adb(), &request.bundle_id)
        }
        #[cfg(target_os = "macos")]
        Some(ActiveDevice::Ios { session, .. }) => {
            automation::apps::ios_uninstall(session.device_id(), &request.bundle_id)
        }
        #[cfg(feature = "emulator-synthetic")]
        Some(ActiveDevice::Synthetic { .. }) => Ok(()),
        None => Err(no_active_device()),
    }
}

#[tauri::command]
pub fn emulator_launch_app(
    state: State<'_, EmulatorState>,
    request: LaunchAppRequest,
) -> CommandResult<()> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    match active.as_ref() {
        Some(ActiveDevice::Android { session, .. }) => {
            automation::apps::android_launch(session.adb(), &request.bundle_id, &request.args)
        }
        #[cfg(target_os = "macos")]
        Some(ActiveDevice::Ios { session, .. }) => {
            automation::apps::ios_launch(session.device_id(), &request.bundle_id, &request.args)
        }
        #[cfg(feature = "emulator-synthetic")]
        Some(ActiveDevice::Synthetic { .. }) => Ok(()),
        None => Err(no_active_device()),
    }
}

#[tauri::command]
pub fn emulator_terminate_app(
    state: State<'_, EmulatorState>,
    request: BundleIdRequest,
) -> CommandResult<()> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    match active.as_ref() {
        Some(ActiveDevice::Android { session, .. }) => {
            automation::apps::android_terminate(session.adb(), &request.bundle_id)
        }
        #[cfg(target_os = "macos")]
        Some(ActiveDevice::Ios { session, .. }) => {
            automation::apps::ios_terminate(session.device_id(), &request.bundle_id)
        }
        #[cfg(feature = "emulator-synthetic")]
        Some(ActiveDevice::Synthetic { .. }) => Ok(()),
        None => Err(no_active_device()),
    }
}

#[tauri::command]
pub fn emulator_set_location(
    state: State<'_, EmulatorState>,
    request: LocationRequest,
) -> CommandResult<()> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    match active.as_ref() {
        Some(ActiveDevice::Android { session, .. }) => {
            // Android emulator supports `geo fix <lon> <lat>` via the console
            // but that requires a telnet connection. We route through `adb`
            // emu geo instead, which the platform-tools binary supports.
            session
                .adb()
                .shell([
                    "geo".to_string(),
                    "fix".to_string(),
                    request.lon.to_string(),
                    request.lat.to_string(),
                ])
                .map(|_| ())
                .map_err(|e| {
                    CommandError::system_fault(
                        "android_set_location_failed",
                        format!("emu geo fix failed: {e}"),
                    )
                })
        }
        #[cfg(target_os = "macos")]
        Some(ActiveDevice::Ios { session, .. }) => {
            ios::xcrun::set_location(session.device_id(), request.lat, request.lon).map_err(|e| {
                CommandError::system_fault(
                    "ios_set_location_failed",
                    format!("simctl location set failed: {e}"),
                )
            })
        }
        #[cfg(feature = "emulator-synthetic")]
        Some(ActiveDevice::Synthetic { .. }) => Ok(()),
        None => Err(no_active_device()),
    }
}

#[tauri::command]
pub fn emulator_push_notification(
    state: State<'_, EmulatorState>,
    request: PushNotificationRequest,
) -> CommandResult<()> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    match active.as_ref() {
        Some(ActiveDevice::Android { .. }) => Err(CommandError::user_fixable(
            "push_unsupported_on_android",
            "Android AVDs have no APNS equivalent; push notifications are iOS-only.",
        )),
        #[cfg(target_os = "macos")]
        Some(ActiveDevice::Ios { session, .. }) => {
            ios::xcrun::push_notification(session.device_id(), &request.bundle_id, &request.payload)
                .map_err(|e| {
                    CommandError::system_fault(
                        "ios_push_failed",
                        format!("simctl push failed: {e}"),
                    )
                })
        }
        #[cfg(feature = "emulator-synthetic")]
        Some(ActiveDevice::Synthetic { .. }) => Ok(()),
        None => Err(no_active_device()),
    }
}

// ---- Log streaming ---------------------------------------------------------

#[tauri::command]
pub fn emulator_logs_subscribe<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, EmulatorState>,
    _request: LogSubscribeRequest,
) -> CommandResult<SubscriptionToken> {
    let active = state.active.lock().expect("emulator active mutex poisoned");
    let device = active.as_ref().ok_or_else(no_active_device)?;

    let id = format!("log-{}", uuid_like());

    match device {
        ActiveDevice::Android { session, .. } => {
            let stream = automation::logs::AndroidLogStream::spawn(
                app,
                session.adb(),
                state.log_collector.clone(),
            )
            .map_err(|e| {
                CommandError::system_fault(
                    "android_logcat_spawn_failed",
                    format!("logcat spawn failed: {e}"),
                )
            })?;
            let mut slot = state.log_stream.lock().expect("log stream mutex poisoned");
            *slot = Some(LogStreamHandle::Android(stream));
            Ok(SubscriptionToken { id })
        }
        #[cfg(target_os = "macos")]
        ActiveDevice::Ios { .. } => Err(CommandError::system_fault(
            "ios_log_stream_not_implemented",
            "iOS log streaming requires the idb proto to be vendored.",
        )),
        #[cfg(feature = "emulator-synthetic")]
        ActiveDevice::Synthetic { .. } => Ok(SubscriptionToken { id }),
    }
}

#[tauri::command]
pub fn emulator_logs_unsubscribe(
    state: State<'_, EmulatorState>,
    _token: SubscriptionToken,
) -> CommandResult<()> {
    let mut slot = state.log_stream.lock().expect("log stream mutex poisoned");
    *slot = None;
    Ok(())
}

#[tauri::command]
pub fn emulator_logs_get_recent(
    state: State<'_, EmulatorState>,
    limit: Option<usize>,
) -> CommandResult<Vec<automation::LogEntry>> {
    let limit = limit.unwrap_or(500).min(10_000);
    Ok(state.log_collector.recent(limit))
}

// ---- Helpers --------------------------------------------------------------

fn no_active_device() -> CommandError {
    CommandError::user_fixable(
        "emulator_not_running",
        "No emulator device is currently running.",
    )
}

impl ActiveDevice {
    fn viewport(&self) -> (u32, u32) {
        match self {
            ActiveDevice::Android { session, .. } => (session.width(), session.height()),
            #[cfg(target_os = "macos")]
            ActiveDevice::Ios { session, .. } => (session.width(), session.height()),
            #[cfg(feature = "emulator-synthetic")]
            ActiveDevice::Synthetic { .. } => {
                (synthetic::synthetic_width(), synthetic::synthetic_height())
            }
        }
    }
}

// ---------- Metro Inspector commands (Phase 2) ----------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectorConnectRequest {
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InspectorElementAtRequest {
    pub x: f32,
    pub y: f32,
}

/// Connect to the Metro inspector. Auto-discovers Metro on common ports
/// unless an explicit port is provided.
#[tauri::command]
pub fn emulator_inspector_connect(
    state: State<'_, EmulatorState>,
    request: InspectorConnectRequest,
) -> CommandResult<MetroStatus> {
    // Disconnect any existing connection first.
    let _ = state.metro_inspector.lock().unwrap().take();

    let (inspector, status) = if let Some(port) = request.port {
        MetroInspector::connect(port)?
    } else {
        MetroInspector::connect_auto(METRO_PORT_RANGE)?
    };

    *state.metro_inspector.lock().unwrap() = Some(inspector);
    Ok(status)
}

/// Disconnect from the Metro inspector.
#[tauri::command]
pub fn emulator_inspector_disconnect(
    state: State<'_, EmulatorState>,
) -> CommandResult<()> {
    let _ = state.metro_inspector.lock().unwrap().take();
    Ok(())
}

/// Query the element at a device-pixel coordinate. Tries Metro inspector
/// first (React Native), falls back to the Swift helper's AXUIElement
/// bridge (native Swift/UIKit apps).
#[tauri::command]
pub fn emulator_inspector_element_at(
    state: State<'_, EmulatorState>,
    request: InspectorElementAtRequest,
) -> CommandResult<ElementInfo> {
    // 1. Try Metro inspector (React Native / Expo).
    {
        let mut guard = state.metro_inspector.lock().unwrap();
        if let Some(inspector) = guard.as_mut() {
            if let Ok(info) = inspector.element_at_point(request.x, request.y) {
                return Ok(info);
            }
        }
    }

    // 2. Fall back to AX inspection via the Swift helper (native apps).
    #[cfg(target_os = "macos")]
    {
        let active = state.active.lock().expect("emulator active mutex poisoned");
        if let Some(ActiveDevice::Ios { session, .. }) = active.as_ref() {
            if let Some(hc) = session.helper_client() {
                let resp = hc.send_request_raw("accessibility_element_at", serde_json::json!({
                    "x": request.x,
                    "y": request.y,
                }))?;
                if let Some(element) = resp.get("element") {
                    let bounds_obj = element.get("frame").or(element.get("bounds"));
                    return Ok(ElementInfo {
                        component_name: element["label"].as_str()
                            .filter(|s| !s.is_empty())
                            .or_else(|| element["type"].as_str())
                            .map(|s| s.to_string()),
                        native_type: element["type"].as_str().map(|s| s.to_string()),
                        bounds: automation::Bounds {
                            x: bounds_obj.and_then(|b| b["x"].as_f64()).unwrap_or(0.0) as i32,
                            y: bounds_obj.and_then(|b| b["y"].as_f64()).unwrap_or(0.0) as i32,
                            w: bounds_obj.and_then(|b| b["w"].as_f64().or(b["width"].as_f64())).unwrap_or(0.0) as i32,
                            h: bounds_obj.and_then(|b| b["h"].as_f64().or(b["height"].as_f64())).unwrap_or(0.0) as i32,
                        },
                        props: serde_json::Value::Object(Default::default()),
                        source: None, // AX doesn't provide source location.
                    });
                }
            }
        }
    }

    Err(CommandError::user_fixable(
        "inspector_no_source",
        "No inspection source available. For React Native apps, ensure Metro is running. \
         For native apps, the Swift helper must be active.".to_string(),
    ))
}

/// Get the full React component tree via the Metro inspector.
#[tauri::command]
pub fn emulator_inspector_component_tree(
    state: State<'_, EmulatorState>,
) -> CommandResult<serde_json::Value> {
    let mut guard = state.metro_inspector.lock().unwrap();
    let inspector = guard.as_mut().ok_or_else(|| {
        CommandError::user_fixable(
            "metro_not_connected",
            "Metro inspector is not connected.".to_string(),
        )
    })?;
    inspector.component_tree()
}

fn uuid_like() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(1);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}-{seq:x}")
}
