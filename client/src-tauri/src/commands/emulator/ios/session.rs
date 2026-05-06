//! iOS Simulator session.
//!
//! Orchestrates:
//!   - simctl-driven simulator boot + shutdown.
//!   - `idb_companion` sidecar lifecycle (spawned at start, reaped on drop).
//!   - H.264 video stream via `idb_client::start_video_stream` (returns
//!     `ios_idb_proto_missing` until the proto is vendored — the pipeline
//!     falls back to a simctl-screenshot bridge so the sidebar still
//!     renders frames).
//!   - HID input dispatch via `idb_client::send_hid`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};

use crate::commands::emulator::codec::encode_jpeg_rgba;
use crate::commands::emulator::decoder::{new_default_decoder, DecodeError};
use crate::commands::emulator::events::{StatusPayload, StatusPhase, EMULATOR_STATUS_EVENT};
use crate::commands::emulator::frame_bus::{publish_and_emit, FrameBus};
use crate::commands::emulator::{EmulatorInputRequest, InputKind, Orientation};
use crate::commands::CommandError;

use super::cg_input;
use super::helper;
use super::helper_client::HelperClient;
use super::idb_client::{IdbClient, VideoStreamHandle};
use super::idb_companion::{self, Companion};
use super::input::{self, HidEvent, TouchPhase};
use super::xcrun;

type FramePumpStart = (u32, u32, Option<VideoStreamHandle>, Option<JoinHandle<()>>);

const BOOT_TIMEOUT: Duration = Duration::from_secs(90);
const COMPANION_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulatorDescriptor {
    pub udid: String,
    pub display_name: String,
    pub is_tablet: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub scale: Option<f32>,
}

pub struct SpawnArgs<R: Runtime> {
    pub app: AppHandle<R>,
    pub frame_bus: Arc<FrameBus>,
    pub device_id: String,
}

/// Owns the simulator + companion for the lifetime of a session.
pub struct IosSession {
    device_id: String,
    /// Bare device name ("iPhone 17 Pro"), used to locate the matching
    /// Simulator.app window by title when dispatching CGEvent input.
    device_name: String,
    width: u32,
    height: u32,
    /// Present when the Swift helper (`xero-ios-helper`) was found and
    /// spawned. Preferred over idb_companion for both frame capture
    /// (ScreenCaptureKit) and HID input (IndigoHID).
    helper: Option<helper::Helper>,
    helper_client: Option<Arc<HelperClient>>,
    /// Present only when `idb_companion` was found on disk and spawned
    /// successfully. Preferred for HID input and required for automation
    /// commands that need idb data (UI dump, log stream).
    client: Option<Arc<IdbClient>>,
    video: Option<VideoStreamHandle>,
    shutdown_flag: Arc<AtomicBool>,
    fallback_thread: Option<JoinHandle<()>>,
    companion: Option<Companion>,
    booted_by_us: bool,
}

impl IosSession {
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Dispatch an input event from the unified `emulator_input` command.
    /// Prefer idb's HID surface when `idb_companion` is available. The
    /// screenshot frame pump can run without idb, so Core Graphics remains a
    /// fallback for dev builds or machines where the sidecar failed to start.
    pub fn dispatch(&self, request: &EmulatorInputRequest) -> Result<(), CommandError> {
        match request.kind {
            InputKind::TouchDown => self.send_touch(
                TouchPhase::Began,
                request.x.unwrap_or(0.0),
                request.y.unwrap_or(0.0),
            ),
            InputKind::TouchMove => self.send_touch(
                TouchPhase::Moved,
                request.x.unwrap_or(0.0),
                request.y.unwrap_or(0.0),
            ),
            InputKind::TouchUp => self.send_touch(
                TouchPhase::Ended,
                request.x.unwrap_or(0.0),
                request.y.unwrap_or(0.0),
            ),
            InputKind::Scroll => {
                let ax = request.x.unwrap_or(0.5);
                let ay = request.y.unwrap_or(0.5);
                let dx = request.dx.unwrap_or(0.0);
                let dy = request.dy.unwrap_or(0.0);
                self.send_swipe(
                    ax,
                    ay,
                    (ax + dx).clamp(0.0, 1.0),
                    (ay + dy).clamp(0.0, 1.0),
                    200,
                )
            }
            InputKind::Key | InputKind::HwButton => {
                let name = request
                    .button
                    .as_deref()
                    .or(request.key.as_deref())
                    .unwrap_or("");
                self.press_hardware_key(name)
            }
            InputKind::Text => self.send_text(request.text.as_deref().unwrap_or("")),
        }
    }

    fn send_touch(&self, phase: TouchPhase, nx: f32, ny: f32) -> Result<(), CommandError> {
        let (px, py) = input::denormalize(nx, ny, self.width.max(1), self.height.max(1));

        match phase {
            TouchPhase::Began => {
                // Taps: CG/AppleScript click is the proven path on macOS 26.
                // idb_companion 1.1.8 HID is broken on Xcode 26 (returns Ok
                // but silently drops the event). Try CG first, then helper/idb.
                let cg = cg_input::send_touch(&self.device_name, phase, nx, ny);
                if cg.is_ok() {
                    return cg;
                }
                // CG failed (AX denied, window not found) — try helper, then idb.
                if let Some(hc) = self.helper_client.as_ref() {
                    if hc.send_touch(phase, px, py).is_ok() {
                        return Ok(());
                    }
                }
                if let Some(client) = self.client.as_ref() {
                    if client.send_hid(HidEvent::Touch { phase, x: px, y: py }).is_ok() {
                        return Ok(());
                    }
                }
                cg
            }
            TouchPhase::Moved | TouchPhase::Ended | TouchPhase::Cancelled => {
                // Drags/swipes: CG is a no-op for these phases.
                // Helper (IndigoHID) and idb are the only paths that work.
                if let Some(hc) = self.helper_client.as_ref() {
                    if hc.send_touch(phase, px, py).is_ok() {
                        return Ok(());
                    }
                }
                if let Some(client) = self.client.as_ref() {
                    return client.send_hid(HidEvent::Touch { phase, x: px, y: py });
                }
                // No helper or idb available — silent no-op (same as before).
                Ok(())
            }
        }
    }

    fn send_swipe(
        &self,
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        duration_ms: u32,
    ) -> Result<(), CommandError> {
        let width = self.width.max(1);
        let height = self.height.max(1);
        let (fx, fy) = input::denormalize(from_x, from_y, width, height);
        let (tx, ty) = input::denormalize(to_x, to_y, width, height);

        // 1. Swift helper (IndigoHID) — most reliable.
        if let Some(hc) = self.helper_client.as_ref() {
            if hc.send_swipe(fx, fy, tx, ty, duration_ms).is_ok() {
                return Ok(());
            }
        }

        // 2. CG/AppleScript drag — proven working path, try before idb
        //    because idb HID is broken on Xcode 26.
        let cg = cg_input::send_swipe(&self.device_name, from_x, from_y, to_x, to_y, duration_ms);
        if cg.is_ok() {
            return cg;
        }

        // 3. idb gRPC HID — last resort.
        if let Some(client) = self.client.as_ref() {
            return client.send_hid(HidEvent::Swipe {
                from_x: fx,
                from_y: fy,
                to_x: tx,
                to_y: ty,
                duration_ms,
            });
        }

        cg
    }

    fn send_hid_or_cg(
        &self,
        event: HidEvent,
        fallback: impl FnOnce(&str) -> Result<(), CommandError>,
    ) -> Result<(), CommandError> {
        // Prefer Swift helper for button/text events.
        if let Some(hc) = self.helper_client.as_ref() {
            let helper_result = match &event {
                HidEvent::Button { button } => hc.send_button(*button),
                HidEvent::Text { text } => hc.send_text(text),
                HidEvent::Home => hc.send_button(input::HardwareButton::Home),
                _ => Err(CommandError::system_fault("unsupported_helper_event", "event type not supported by helper")),
            };
            if helper_result.is_ok() {
                return helper_result;
            }
            // Fall through to idb/CG on helper failure.
        }

        if let Some(client) = self.client.as_ref() {
            let result = client.send_hid(event);
            if result.is_ok() || !should_try_cg_fallback(result.as_ref().unwrap_err()) {
                return result;
            }
        }
        fallback(&self.device_name)
    }

    /// Semantic hardware-key press shared with the automation surface.
    pub fn press_hardware_key(&self, name: &str) -> Result<(), CommandError> {
        if name == "home" {
            return self.send_hid_or_cg(HidEvent::Home, cg_input::send_home);
        }
        let button = super::input::parse_hardware_button(name).ok_or_else(|| {
            CommandError::user_fixable(
                "emulator_unknown_key",
                format!("Unknown iOS hardware key: {name}"),
            )
        })?;
        self.send_hid_or_cg(HidEvent::Button { button }, |device_name| {
            cg_input::send_hardware_button(device_name, button)
        })
    }

    /// Swipe helper used by the automation `emulator_swipe` command so it
    /// shares the exact code path the sidebar uses.
    pub fn swipe(
        &self,
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        duration_ms: u32,
    ) -> Result<(), CommandError> {
        self.send_swipe(from_x, from_y, to_x, to_y, duration_ms)
    }

    /// Type helper used by the automation `emulator_type` command.
    pub fn send_text(&self, text: &str) -> Result<(), CommandError> {
        self.send_hid_or_cg(
            HidEvent::Text {
                text: text.to_string(),
            },
            |device_name| cg_input::send_text(device_name, text),
        )
    }

    /// Single-point tap helper (down + up) used by the automation
    /// `emulator_tap` command; matches the sidebar's two-event gesture so
    /// selectors behave identically to the user's click.
    pub fn tap(&self, nx: f32, ny: f32) -> Result<(), CommandError> {
        self.send_touch(TouchPhase::Began, nx, ny)?;
        self.send_touch(TouchPhase::Ended, nx, ny)
    }

    pub fn set_orientation(&self, orientation: Orientation) -> Result<(), CommandError> {
        let value = match orientation {
            Orientation::Portrait => "portrait",
            Orientation::Landscape => "landscapeLeft",
        };
        xcrun::set_orientation(&self.device_id, value).map_err(|err| {
            CommandError::user_fixable(
                "ios_set_orientation_failed",
                format!("iOS Simulator rotation failed: {err}"),
            )
        })
    }

    /// Expose the underlying gRPC client so automation commands (UI dump,
    /// log streaming) can issue their own calls. Returns `None` when
    /// `idb_companion` wasn't available at session start — callers should
    /// surface a typed error in that case.
    pub fn client(&self) -> Option<Arc<IdbClient>> {
        self.client.as_ref().map(Arc::clone)
    }

    /// Expose the Swift helper client for accessibility tree queries
    /// (Phase 3 AXUIElement inspection).
    pub fn helper_client(&self) -> Option<Arc<HelperClient>> {
        self.helper_client.as_ref().map(Arc::clone)
    }

    pub fn shutdown(&mut self) {
        self.shutdown_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.video.take() {
            handle.shutdown(Duration::from_millis(300));
        }
        if let Some(handle) = self.fallback_thread.take() {
            let _ = handle.join();
        }
        // Shut down Swift helper before idb_companion.
        if let Some(ref hc) = self.helper_client {
            let _ = hc.stop_capture();
        }
        self.helper_client = None;
        if let Some(mut h) = self.helper.take() {
            let _ = h.guard.shutdown(Duration::from_millis(500));
            let _ = std::fs::remove_file(&h.socket_path);
        }
        if let Some(mut companion) = self.companion.take() {
            let _ = companion.guard.shutdown(Duration::from_millis(500));
        }
        if self.booted_by_us {
            let _ = xcrun::shutdown(&self.device_id);
        }
        cg_input::invalidate_cache();
    }
}

fn should_try_cg_fallback(err: &CommandError) -> bool {
    matches!(
        err.code.as_str(),
        "ios_input_unsupported" | "ios_idb_proto_missing"
    )
}

impl Drop for IosSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn list_devices() -> Vec<SimulatorDescriptor> {
    xcrun::list_devices().unwrap_or_default()
}

pub fn spawn<R: Runtime + 'static>(args: SpawnArgs<R>) -> Result<IosSession, CommandError> {
    let SpawnArgs {
        app,
        frame_bus,
        device_id,
    } = args;

    eprintln!("[ios-session] spawn starting for {device_id}");

    emit_status(
        &app,
        StatusPhase::Booting,
        &device_id,
        Some(format!("booting simulator {device_id}")),
    );

    eprintln!("[ios-session] calling xcrun boot...");
    xcrun::boot(&device_id, BOOT_TIMEOUT).map_err(|err| {
        CommandError::system_fault(
            "ios_boot_failed",
            format!("simctl boot {device_id} failed: {err}"),
        )
    })?;

    // Look up the bare device name now so the CGEvent input path can match
    // the Simulator.app window by title even after the window is reopened
    // or repositioned. Fall back to the UDID only if simctl refuses to
    // answer — a poor key for title matching, but a valid session handle.
    let device_name = xcrun::device_name(&device_id).unwrap_or_else(|_| device_id.clone());

    // Make sure Simulator.app is running so its window exists for CGEvent
    // dispatch. `open -g` keeps Xero frontmost on most recent macOS
    // releases; brief sleep gives the window server time to register the
    // new window before we hand control back to the frontend.
    eprintln!("[ios-session] boot complete, focusing simulator...");
    let _ = xcrun::focus_simulator(&device_id);
    std::thread::sleep(Duration::from_millis(400));
    eprintln!("[ios-session] focus done, attaching pipeline...");

    emit_status(
        &app,
        StatusPhase::Connecting,
        &device_id,
        Some("attaching input pipeline".to_string()),
    );

    // Spawn the Swift helper for AX inspection (Phase 3). Frame capture
    // is NOT routed through the helper (ScreenCaptureKit crashes as a
    // sidecar without window-server connection). The helper is used for:
    //   - accessibility_tree / accessibility_element_at queries
    //   - HID input via IndigoHID (fallback after CG)
    let (ios_helper, ios_helper_client) = match helper::resolve_helper_binary(&app) {
        Some(binary) => {
            match helper::HelperLaunch::new(binary, device_id.clone())
                .and_then(|launch| helper::spawn(launch, Duration::from_secs(5)))
            {
                Ok(h) => {
                    match super::helper_client::HelperClient::connect(
                        &h.socket_path,
                        Duration::from_secs(3),
                    ) {
                        Ok(c) => (Some(h), Some(Arc::new(c))),
                        Err(e) => {
                            eprintln!("xero: helper connect failed: {e}");
                            (None, None)
                        }
                    }
                }
                Err(e) => {
                    eprintln!("xero: helper spawn failed: {e}");
                    (None, None)
                }
            }
        }
        None => (None, None),
    };
    eprintln!("[ios-session] helper result: helper={}, client={}", ios_helper.is_some(), ios_helper_client.is_some());

    eprintln!("[ios-session] helper result: helper={}, client={}", ios_helper.is_some(), ios_helper_client.is_some());

    // `idb_companion` is best-effort: when it starts, HID input uses idb's
    // real simulator surface; when it does not, the session can still render
    // via screenshots and attempt Core Graphics input.
    let companion = match xcrun::resolve_idb_companion(&app) {
        Some(path) => {
            let _ = idb_companion::ensure_executable(&path);
            match idb_companion::Launch::new(path, device_id.clone())
                .and_then(|launch| idb_companion::spawn(launch, COMPANION_TIMEOUT))
            {
                Ok(companion) => Some(companion),
                Err(err) => {
                    // Surface as informational status rather than an error —
                    // the sidebar still works without it.
                    emit_status(
                        &app,
                        StatusPhase::Connecting,
                        &device_id,
                        Some(format!(
                            "idb_companion unavailable (automation commands disabled): {err}"
                        )),
                    );
                    None
                }
            }
        }
        None => None,
    };

    let client = companion
        .as_ref()
        .map(|c| Arc::new(IdbClient::new(c.grpc_port, device_id.clone())));

    eprintln!("[ios-session] idb companion={}, client={}", companion.is_some(), client.is_some());
    eprintln!("[ios-session] starting frame pump...");

    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let (width, height, video_handle, fallback_thread) = start_frame_pump(
        &app,
        ios_helper_client.as_ref(),
        client.as_ref(),
        &frame_bus,
        &device_id,
        Arc::clone(&shutdown_flag),
    )?;

    eprintln!("[ios-session] frame pump started: {width}x{height}");

    emit_status(
        &app,
        StatusPhase::Streaming,
        &device_id,
        Some(format!("streaming at {width}x{height}")),
    );

    eprintln!("[ios-session] spawn complete, returning session");
    Ok(IosSession {
        device_id,
        device_name,
        width,
        height,
        helper: ios_helper,
        helper_client: ios_helper_client,
        client,
        video: video_handle,
        shutdown_flag,
        fallback_thread,
        companion,
        booted_by_us: true,
    })
}

fn start_frame_pump<R: Runtime + 'static>(
    app: &AppHandle<R>,
    helper_client: Option<&Arc<HelperClient>>,
    idb_client: Option<&Arc<IdbClient>>,
    bus: &Arc<FrameBus>,
    device_id: &str,
    shutdown: Arc<AtomicBool>,
) -> Result<FramePumpStart, CommandError> {
    eprintln!("[frame-pump] helper={}, idb={}", helper_client.is_some(), idb_client.is_some());

    // NOTE: ScreenCaptureKit requires a window-server connection that
    // sidecar binaries don't have (CGS_REQUIRE_INIT crash). The helper
    // is still used for HID input (IndigoHID) and AX inspection, but
    // frame capture always uses the screenshot fallback or idb H.264.

    // Fallback: H.264 stream via idb_companion or screenshot polling.
    let client = idb_client;
    let app_clone = app.clone();
    let bus_clone = Arc::clone(bus);
    let device_for_stream = device_id.to_string();

    let decoder = Arc::new(Mutex::new(new_default_decoder()));
    let width_state = Arc::new(Mutex::new((0_u32, 0_u32)));
    let width_state_clone = Arc::clone(&width_state);
    let decoder_clone = Arc::clone(&decoder);

    // Skip the H.264 path whenever either the decoder isn't linked
    // (no `emulator-live` feature) or idb_companion didn't start (so
    // there's nobody to pull frames from). The screenshot poll gives
    // the user a functional — if lower-FPS — viewport in both cases.
    let decoder_live = decoder
        .lock()
        .ok()
        .map(|d| d.name() != "unavailable")
        .unwrap_or(false);
    let Some(client) = client else {
        let (w, h, thread_handle) = spawn_screenshot_fallback(
            app.clone(),
            Arc::clone(bus),
            device_id.to_string(),
            shutdown,
        )?;
        return Ok((w, h, None, Some(thread_handle)));
    };
    if !decoder_live {
        let (w, h, thread_handle) = spawn_screenshot_fallback(
            app.clone(),
            Arc::clone(bus),
            device_id.to_string(),
            shutdown,
        )?;
        return Ok((w, h, None, Some(thread_handle)));
    }

    let video_cb = Box::new(move |nal: &[u8]| {
        let mut decoder_guard = decoder_clone.lock().expect("ios decoder mutex");
        match decoder_guard.decode(nal) {
            Ok(Some(frame)) => {
                *width_state_clone.lock().expect("ios width mutex") = (frame.width, frame.height);
                match encode_jpeg_rgba(&frame.rgba, frame.width, frame.height) {
                    Ok(jpeg) => {
                        publish_and_emit(&app_clone, &bus_clone, frame.width, frame.height, jpeg);
                    }
                    Err(err) => {
                        emit_error(
                            &app_clone,
                            &device_for_stream,
                            format!("jpeg encode: {err}"),
                        );
                    }
                }
            }
            Ok(None) => {}
            Err(DecodeError::Unavailable) => {
                emit_error(
                    &app_clone,
                    &device_for_stream,
                    "H.264 decoder unavailable: rebuild with --features emulator-live".to_string(),
                );
            }
            Err(err) => {
                emit_error(
                    &app_clone,
                    &device_for_stream,
                    format!("h264 decode: {err}"),
                );
            }
        }
    });

    let stream_result = client.start_video_stream(30, video_cb);

    match stream_result {
        Ok(handle) => {
            std::thread::sleep(Duration::from_millis(500));
            let (w, h) = *width_state.lock().unwrap();
            let (width, height) = if w > 0 && h > 0 { (w, h) } else { (1179, 2556) };
            Ok((width, height, Some(handle), None))
        }
        Err(_) => {
            let (w, h, thread_handle) = spawn_screenshot_fallback(
                app.clone(),
                Arc::clone(bus),
                device_id.to_string(),
                shutdown,
            )?;
            Ok((w, h, None, Some(thread_handle)))
        }
    }
}

fn spawn_screenshot_fallback<R: Runtime + 'static>(
    app: AppHandle<R>,
    bus: Arc<FrameBus>,
    device_id: String,
    shutdown: Arc<AtomicBool>,
) -> Result<(u32, u32, JoinHandle<()>), CommandError> {
    // Retry the initial screenshot — iOS 26's display pipeline can take
    // several seconds to attach after boot, causing ScreenshotError code=2.
    let mut png = None;
    for attempt in 0..10 {
        match xcrun::screenshot(&device_id) {
            Ok(data) => {
                png = Some(data);
                break;
            }
            Err(e) => {
                eprintln!("[frame-pump] initial screenshot attempt {attempt} failed: {e}");
                if attempt < 9 {
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }
    let png = png.ok_or_else(|| {
        CommandError::system_fault(
            "ios_screenshot_failed",
            "initial simctl screenshot failed after 10 retries (display not ready)".to_string(),
        )
    })?;
    let initial =
        image::load_from_memory_with_format(&png, image::ImageFormat::Png).map_err(|e| {
            CommandError::system_fault(
                "ios_screenshot_decode_failed",
                format!("failed to decode simctl PNG: {e}"),
            )
        })?;
    let (width, height) = (initial.width(), initial.height());
    publish_png(&app, &bus, png, width, height);

    let handle = thread::spawn(move || {
        eprintln!("[screenshot-poll] thread started for {device_id}");
        let mut frame_count = 0u64;
        loop {
            if shutdown.load(Ordering::Relaxed) {
                eprintln!("[screenshot-poll] shutdown requested, exiting");
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
            if shutdown.load(Ordering::Relaxed) {
                eprintln!("[screenshot-poll] shutdown requested, exiting");
                break;
            }
            match xcrun::screenshot(&device_id) {
                Ok(png) => {
                    frame_count += 1;
                    if frame_count <= 3 || frame_count % 20 == 0 {
                        eprintln!("[screenshot-poll] frame #{frame_count}, {} bytes", png.len());
                    }
                    publish_png(&app, &bus, png, width, height);
                }
                Err(e) => {
                    eprintln!("[screenshot-poll] error: {e}");
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    });

    Ok((width, height, handle))
}

fn publish_png<R: Runtime>(
    app: &AppHandle<R>,
    bus: &Arc<FrameBus>,
    png_bytes: Vec<u8>,
    _initial_width: u32,
    _initial_height: u32,
) {
    match png_to_jpeg(&png_bytes) {
        Ok((width, height, jpeg)) => {
            publish_and_emit(app, bus, width, height, jpeg);
        }
        Err(err) => {
            // Surface the specific failure — an earlier version of this
            // function silently dropped frames when the `image` crate's
            // JPEG encoder rejected an RGBA buffer, which looked like a
            // frozen stream from the frontend. Route through stderr so
            // the next diagnosis doesn't have to re-derive this.
            eprintln!("[emulator] ios publish_png: {err}");
        }
    }
}

/// Decode a PNG, strip its alpha channel, and JPEG-encode it. Pure so
/// it's testable without a Tauri runtime — the regression test for the
/// "first frame never arrives" bug lives in `tests::` below.
fn png_to_jpeg(png_bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    let img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)
        .map_err(|err| format!("PNG decode failed: {err}"))?;
    // Trust the PNG's actual dimensions — rotation can change them
    // mid-session and we don't want to feed the encoder the
    // initial-boot dimensions against a rotated buffer.
    let width = img.width();
    let height = img.height();
    let rgba = img.to_rgba8();
    let jpeg = encode_jpeg_rgba(rgba.as_raw(), width, height)
        .map_err(|err| format!("JPEG encode failed: {err}"))?;
    Ok((width, height, jpeg))
}

fn emit_status<R: Runtime>(
    app: &AppHandle<R>,
    phase: StatusPhase,
    device_id: &str,
    message: Option<String>,
) {
    let mut payload = StatusPayload::new(phase)
        .with_platform("ios")
        .with_device(device_id.to_string());
    if let Some(msg) = message {
        payload = payload.with_message(msg);
    }
    let _ = app.emit(EMULATOR_STATUS_EVENT, payload);
}

fn emit_error<R: Runtime>(app: &AppHandle<R>, device_id: &str, message: String) {
    let _ = app.emit(
        EMULATOR_STATUS_EVENT,
        StatusPayload::new(StatusPhase::Error)
            .with_platform("ios")
            .with_device(device_id.to_string())
            .with_message(message),
    );
}

#[cfg(test)]
mod tests {
    use super::png_to_jpeg;
    use image::codecs::png::PngEncoder;
    use image::{ColorType, ImageEncoder};

    /// Regression: `image` 0.25's `JpegEncoder` rejects `Rgba8` buffers
    /// with `UnsupportedError`. An earlier revision of `publish_png`
    /// fed the RGBA buffer straight to the encoder and silently
    /// returned on the error, leaving the screenshot fallback stalled
    /// on "Waiting for first frame…" even though `simctl io
    /// screenshot` was succeeding every 600 ms. Route through
    /// `encode_jpeg_rgba` (strips alpha first) and verify end-to-end
    /// that a PNG with an alpha channel becomes a valid JPEG.
    #[test]
    fn png_with_alpha_round_trips_to_jpeg() {
        let width = 16u32;
        let height = 8u32;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                rgba.push((x * 16) as u8);
                rgba.push((y * 32) as u8);
                rgba.push(((x + y) * 8) as u8);
                rgba.push(200); // non-opaque alpha — the bug path
            }
        }

        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&rgba, width, height, ColorType::Rgba8.into())
            .expect("png encode");

        let (decoded_w, decoded_h, jpeg) = png_to_jpeg(&png).expect("publish path must not fail");
        assert_eq!(decoded_w, width);
        assert_eq!(decoded_h, height);
        // JPEG magic: 0xFF 0xD8 0xFF.
        assert_eq!(&jpeg[..3], &[0xFF, 0xD8, 0xFF]);
        assert!(jpeg.len() > 128, "jpeg output suspiciously small");

        let decoded = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg)
            .expect("jpeg decode");
        assert_eq!(decoded.width(), width);
        assert_eq!(decoded.height(), height);
    }

    #[test]
    fn invalid_png_bytes_surface_a_typed_error() {
        let err = png_to_jpeg(&[0, 1, 2, 3]).unwrap_err();
        assert!(err.contains("PNG decode failed"), "got {err}");
    }
}
