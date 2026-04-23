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

#![cfg(target_os = "macos")]

use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use image::{codecs::jpeg::JpegEncoder, ColorType, ImageEncoder};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};

use crate::commands::emulator::codec::encode_jpeg_rgba;
use crate::commands::emulator::decoder::{new_default_decoder, DecodeError};
use crate::commands::emulator::events::{StatusPayload, StatusPhase, EMULATOR_STATUS_EVENT};
use crate::commands::emulator::frame_bus::{publish_and_emit, FrameBus};
use crate::commands::emulator::{EmulatorInputRequest, InputKind, Orientation};
use crate::commands::CommandError;

use super::idb_client::{IdbClient, VideoStreamHandle};
use super::idb_companion::{self, Companion};
use super::input::{self, HidEvent, TouchPhase};
use super::xcrun;

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
    width: u32,
    height: u32,
    client: Arc<IdbClient>,
    video: Option<VideoStreamHandle>,
    shutdown_flag: Arc<AtomicBool>,
    fallback_thread: Option<JoinHandle<()>>,
    companion: Companion,
    booted_by_us: bool,
}

impl IosSession {
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Dispatch an input event from the unified `emulator_input` command.
    pub fn dispatch(&self, request: &EmulatorInputRequest) -> Result<(), CommandError> {
        let (width, height) = (self.width.max(1), self.height.max(1));
        match request.kind {
            InputKind::TouchDown => {
                let (x, y) = input::denormalize(
                    request.x.unwrap_or(0.0),
                    request.y.unwrap_or(0.0),
                    width,
                    height,
                );
                self.client.send_hid(HidEvent::Touch {
                    phase: TouchPhase::Began,
                    x,
                    y,
                })
            }
            InputKind::TouchMove => {
                let (x, y) = input::denormalize(
                    request.x.unwrap_or(0.0),
                    request.y.unwrap_or(0.0),
                    width,
                    height,
                );
                self.client.send_hid(HidEvent::Touch {
                    phase: TouchPhase::Moved,
                    x,
                    y,
                })
            }
            InputKind::TouchUp => {
                let (x, y) = input::denormalize(
                    request.x.unwrap_or(0.0),
                    request.y.unwrap_or(0.0),
                    width,
                    height,
                );
                self.client.send_hid(HidEvent::Touch {
                    phase: TouchPhase::Ended,
                    x,
                    y,
                })
            }
            InputKind::Scroll => {
                let ax = request.x.unwrap_or(0.5);
                let ay = request.y.unwrap_or(0.5);
                let dx = request.dx.unwrap_or(0.0);
                let dy = request.dy.unwrap_or(0.0);
                let (from_x, from_y) = input::denormalize(ax, ay, width, height);
                let (to_x, to_y) = input::denormalize(
                    (ax + dx).clamp(0.0, 1.0),
                    (ay + dy).clamp(0.0, 1.0),
                    width,
                    height,
                );
                self.client.send_hid(HidEvent::Swipe {
                    from_x,
                    from_y,
                    to_x,
                    to_y,
                    duration_ms: 200,
                })
            }
            InputKind::Key | InputKind::HwButton => {
                let name = request
                    .button
                    .as_deref()
                    .or(request.key.as_deref())
                    .unwrap_or("");
                if name == "home" {
                    return self.client.send_hid(HidEvent::Home);
                }
                let button = input::parse_hardware_button(name).ok_or_else(|| {
                    CommandError::user_fixable(
                        "emulator_unknown_key",
                        format!("Unknown iOS hardware key: {name}"),
                    )
                })?;
                self.client.send_hid(HidEvent::Button { button })
            }
            InputKind::Text => {
                let text = request.text.as_deref().unwrap_or("");
                self.client.send_hid(HidEvent::Text {
                    text: text.to_string(),
                })
            }
        }
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

    /// Expose the underlying gRPC client so automation commands can issue
    /// their own calls against it.
    pub fn client(&self) -> Arc<IdbClient> {
        Arc::clone(&self.client)
    }

    pub fn shutdown(&mut self) {
        self.shutdown_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = self.video.take() {
            handle.shutdown(Duration::from_millis(300));
        }
        if let Some(handle) = self.fallback_thread.take() {
            let _ = handle.join();
        }
        let _ = self.companion.guard.shutdown(Duration::from_millis(500));
        if self.booted_by_us {
            let _ = xcrun::shutdown(&self.device_id);
        }
    }
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

    emit_status(
        &app,
        StatusPhase::Booting,
        &device_id,
        Some(format!("booting simulator {device_id}")),
    );

    xcrun::boot(&device_id, BOOT_TIMEOUT).map_err(|err| {
        CommandError::system_fault(
            "ios_boot_failed",
            format!("simctl boot {device_id} failed: {err}"),
        )
    })?;

    emit_status(
        &app,
        StatusPhase::Connecting,
        &device_id,
        Some("launching idb_companion".to_string()),
    );

    let companion_path = xcrun::resolve_idb_companion(&app).ok_or_else(|| {
        CommandError::user_fixable(
            "ios_idb_companion_missing",
            "idb_companion is not bundled with Cadence and could not be found on PATH. Install \
             it via `brew install facebook/fb/idb-companion` or drop the binary into \
             client/src-tauri/resources/binaries/.",
        )
    })?;
    let _ = idb_companion::ensure_executable(&companion_path);

    let launch = idb_companion::Launch::new(companion_path, device_id.clone()).map_err(|err| {
        CommandError::system_fault(
            "ios_idb_launch_setup_failed",
            format!("failed to build idb_companion launch: {err}"),
        )
    })?;
    let companion = idb_companion::spawn(launch, COMPANION_TIMEOUT).map_err(|err| {
        CommandError::system_fault(
            "ios_idb_companion_spawn_failed",
            format!("failed to spawn idb_companion: {err}"),
        )
    })?;

    let client = Arc::new(IdbClient::new(companion.grpc_port, device_id.clone()));
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let (width, height, video_handle, fallback_thread) = start_frame_pump(
        &app,
        &client,
        &frame_bus,
        &device_id,
        Arc::clone(&shutdown_flag),
    )?;

    emit_status(
        &app,
        StatusPhase::Streaming,
        &device_id,
        Some(format!("streaming at {width}x{height}")),
    );

    Ok(IosSession {
        device_id,
        width,
        height,
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
    client: &Arc<IdbClient>,
    bus: &Arc<FrameBus>,
    device_id: &str,
    shutdown: Arc<AtomicBool>,
) -> Result<(u32, u32, Option<VideoStreamHandle>, Option<JoinHandle<()>>), CommandError> {
    let app_clone = app.clone();
    let bus_clone = Arc::clone(bus);
    let device_for_stream = device_id.to_string();

    let decoder = Arc::new(Mutex::new(new_default_decoder()));
    let width_state = Arc::new(Mutex::new((0_u32, 0_u32)));
    let width_state_clone = Arc::clone(&width_state);
    let decoder_clone = Arc::clone(&decoder);

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
    let png = xcrun::screenshot(&device_id).map_err(|err| {
        CommandError::system_fault(
            "ios_screenshot_failed",
            format!("initial simctl screenshot failed: {err}"),
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
        // Tolerate transient screenshot failures — a single simctl hiccup
        // shouldn't kill the whole frame pump. Exit only after the shutdown
        // flag is set or the failure streak clearly means the simulator is
        // gone.
        const MAX_CONSECUTIVE_FAILURES: u32 = 10;
        let mut consecutive_failures = 0u32;
        loop {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(600));
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            match xcrun::screenshot(&device_id) {
                Ok(png) => {
                    consecutive_failures = 0;
                    publish_png(&app, &bus, png, width, height);
                }
                Err(err) => {
                    consecutive_failures += 1;
                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        emit_error(
                            &app,
                            &device_id,
                            format!(
                                "simctl screenshot failed {consecutive_failures} times in a row: {err}"
                            ),
                        );
                        break;
                    }
                    // Back off a little so we don't spin on a transient
                    // issue (e.g. simctl contending during a boot step).
                    std::thread::sleep(Duration::from_millis(400));
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
    let Ok(img) = image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png) else {
        return;
    };
    // Always trust the PNG's actual dimensions — rotation can change
    // them mid-session and we don't want to feed the encoder the
    // initial-boot dimensions against a rotated buffer.
    let width = img.width();
    let height = img.height();
    let rgba = img.to_rgba8();
    let mut out = Vec::with_capacity(rgba.len() / 4);
    let encoder = JpegEncoder::new_with_quality(Cursor::new(&mut out), 80);
    if encoder
        .write_image(rgba.as_raw(), width, height, ColorType::Rgba8.into())
        .is_err()
    {
        return;
    }
    publish_and_emit(app, bus, width, height, out);
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
