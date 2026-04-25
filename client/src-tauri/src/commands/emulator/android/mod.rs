//! Android pipeline: emulator process + scrcpy-driven video + input.

pub mod adb;
pub mod avd;
pub mod emulator_process;
pub mod input;
pub mod provision;
pub mod scrcpy;
pub mod sdk;

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Runtime};

use crate::commands::emulator::codec::encode_jpeg_rgba;
use crate::commands::emulator::decoder::{new_default_decoder, DecodeError, H264Decoder};
use crate::commands::emulator::events::{StatusPayload, StatusPhase, EMULATOR_STATUS_EVENT};
use crate::commands::emulator::frame_bus::{publish_and_emit, FrameBus};
use crate::commands::emulator::process::ChildGuard;
use crate::commands::CommandError;

use adb::Adb;
use avd::AndroidAvd;
use emulator_process::EmulatorLaunch;
use input::{Keycode, MotionAction};

const BOOT_TIMEOUT: Duration = Duration::from_secs(120);

/// Runtime-alive Android session: owns the emulator process, the scrcpy sockets,
/// and the decoder thread. Dropping the session (or calling [`Self::shutdown`])
/// tears everything down in order: decoder thread → control socket → video socket
/// → emulator child → frame bus clear.
pub struct AndroidSession {
    device_id: String,
    width: u32,
    height: u32,
    control: Arc<Mutex<TcpStream>>,
    shutdown_flag: Arc<AtomicBool>,
    decoder_thread: Option<JoinHandle<()>>,
    _emulator: ChildGuard,
    adb: Adb,
}

impl AndroidSession {
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

/// Attempt to enumerate AVDs on this host. Errors become an empty list in the
/// frontend — the missing-SDK panel surfaces the same information via `sdk_status`.
pub fn list_devices<R: Runtime>(app: &AppHandle<R>) -> Vec<AndroidAvd> {
    let sdk = sdk::probe_with_app(app);
    if !sdk.is_usable() {
        return Vec::new();
    }
    avd::list(&sdk).unwrap_or_default()
}

/// Arguments threaded into the session. Kept out of the public API so the
/// callsite in `emulator::mod` stays a one-liner.
pub struct SpawnArgs<R: Runtime> {
    pub app: AppHandle<R>,
    pub frame_bus: Arc<FrameBus>,
    pub device_id: String,
    pub scrcpy_jar: std::path::PathBuf,
}

pub fn spawn<R: Runtime + 'static>(args: SpawnArgs<R>) -> Result<AndroidSession, CommandError> {
    let SpawnArgs {
        app,
        frame_bus,
        device_id,
        scrcpy_jar,
    } = args;

    let sdk = sdk::probe_with_app(&app);
    let emulator_bin = sdk
        .emulator_path()
        .ok_or_else(|| missing_sdk("emulator binary not found on PATH or ANDROID_HOME"))?
        .to_path_buf();
    let adb_bin = sdk
        .adb_path()
        .ok_or_else(|| missing_sdk("adb binary not found on PATH or ANDROID_HOME"))?
        .to_path_buf();
    let sdk_root_env = sdk.sdk_root.clone();

    emit_status(
        &app,
        StatusPhase::Booting,
        &device_id,
        Some(format!("booting AVD {device_id}")),
    );

    let launch = EmulatorLaunch::new(device_id.clone());
    let (emulator, adb) = emulator_process::spawn(
        &emulator_bin,
        &adb_bin,
        &launch,
        BOOT_TIMEOUT,
        sdk_root_env.as_deref(),
    )
    .map_err(|err| {
        CommandError::system_fault(
            "android_emulator_boot_failed",
            format!("failed to boot {device_id}: {err}"),
        )
    })?;

    emit_status(
        &app,
        StatusPhase::Connecting,
        &device_id,
        Some("connecting scrcpy".to_string()),
    );

    let connection = scrcpy::start(&adb, &scrcpy_jar).map_err(|err| {
        CommandError::system_fault(
            "android_scrcpy_start_failed",
            format!("failed to start scrcpy: {err}"),
        )
    })?;

    let meta = connection.meta;
    let scrcpy::ScrcpyConnection { video, control, .. } = connection;

    let width = meta.width;
    let height = meta.height;
    let control = Arc::new(Mutex::new(control));
    let shutdown_flag = Arc::new(AtomicBool::new(false));

    let decoder_thread = spawn_decoder_thread(
        app.clone(),
        frame_bus,
        video,
        width,
        height,
        device_id.clone(),
        Arc::clone(&shutdown_flag),
    );

    emit_status(
        &app,
        StatusPhase::Streaming,
        &device_id,
        Some(format!("streaming at {width}x{height}")),
    );

    Ok(AndroidSession {
        device_id,
        width,
        height,
        control,
        shutdown_flag,
        decoder_thread: Some(decoder_thread),
        _emulator: emulator,
        adb,
    })
}

impl AndroidSession {
    /// Dispatch a normalized touch event onto the device's control socket.
    pub fn send_touch(&self, action: MotionAction, x: f32, y: f32) -> Result<(), CommandError> {
        let (px, py) = input::denormalize(x, y, self.width, self.height);
        let mut guard = self
            .control
            .lock()
            .map_err(|_| control_error("control socket mutex poisoned"))?;
        input::send_touch(
            &mut guard,
            action,
            px,
            py,
            self.width as u16,
            self.height as u16,
        )
        .map_err(|e| control_error(format!("write touch failed: {e}")))
    }

    pub fn send_scroll(&self, x: f32, y: f32, dx: i16, dy: i16) -> Result<(), CommandError> {
        let (px, py) = input::denormalize(x, y, self.width, self.height);
        let mut guard = self
            .control
            .lock()
            .map_err(|_| control_error("control socket mutex poisoned"))?;
        input::send_scroll(
            &mut guard,
            px,
            py,
            self.width as u16,
            self.height as u16,
            dx,
            dy,
        )
        .map_err(|e| control_error(format!("write scroll failed: {e}")))
    }

    pub fn send_key(&self, keycode: Keycode) -> Result<(), CommandError> {
        let mut guard = self
            .control
            .lock()
            .map_err(|_| control_error("control socket mutex poisoned"))?;
        input::send_key(&mut guard, input::KeyAction::Down, keycode, 0, 0)
            .map_err(|e| control_error(format!("write key down failed: {e}")))?;
        input::send_key(&mut guard, input::KeyAction::Up, keycode, 0, 0)
            .map_err(|e| control_error(format!("write key up failed: {e}")))
    }

    pub fn send_text(&self, text: &str) -> Result<(), CommandError> {
        // Split long strings — scrcpy caps per message at 300 bytes.
        let mut guard = self
            .control
            .lock()
            .map_err(|_| control_error("control socket mutex poisoned"))?;
        for chunk in chunk_text(text, 300) {
            input::send_text(&mut guard, chunk)
                .map_err(|e| control_error(format!("write text failed: {e}")))?;
        }
        Ok(())
    }

    pub fn send_rotate(&self, rotation: u8) -> Result<(), CommandError> {
        let mut guard = self
            .control
            .lock()
            .map_err(|_| control_error("control socket mutex poisoned"))?;
        input::send_rotate(&mut guard, rotation)
            .map_err(|e| control_error(format!("rotate failed: {e}")))
    }

    pub fn adb(&self) -> &Adb {
        &self.adb
    }

    pub fn shutdown(&mut self) {
        self.shutdown_flag.store(true, Ordering::Relaxed);
        if let Ok(guard) = self.control.lock() {
            // Shutdown the control socket so any blocked reader wakes up.
            let _ = guard.shutdown(std::net::Shutdown::Both);
        }
        if let Some(handle) = self.decoder_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AndroidSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn spawn_decoder_thread<R: Runtime + 'static>(
    app: AppHandle<R>,
    bus: Arc<FrameBus>,
    mut video: TcpStream,
    initial_width: u32,
    initial_height: u32,
    device_id: String,
    shutdown: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut decoder: Box<dyn H264Decoder> = new_default_decoder();
        let mut announced_decoder_error = false;
        let mut width = initial_width;
        let mut height = initial_height;

        loop {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }

            let packet = match scrcpy::read_video_packet(&mut video) {
                Ok(Some(pkt)) => pkt,
                Ok(None) => break,
                Err(err) => {
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                    let _ = app.emit(
                        EMULATOR_STATUS_EVENT,
                        StatusPayload::new(StatusPhase::Error)
                            .with_platform("android")
                            .with_device(device_id.clone())
                            .with_message(format!("scrcpy video read failed: {err}")),
                    );
                    break;
                }
            };

            match decoder.decode(&packet.payload) {
                Ok(Some(frame)) => {
                    if frame.width > 0 && frame.height > 0 {
                        width = frame.width;
                        height = frame.height;
                    }
                    match encode_jpeg_rgba(&frame.rgba, frame.width, frame.height) {
                        Ok(jpeg) => {
                            publish_and_emit(&app, &bus, frame.width, frame.height, jpeg);
                        }
                        Err(err) => {
                            let _ = app.emit(
                                EMULATOR_STATUS_EVENT,
                                StatusPayload::new(StatusPhase::Error)
                                    .with_platform("android")
                                    .with_device(device_id.clone())
                                    .with_message(format!("jpeg encode failed: {err}")),
                            );
                            break;
                        }
                    }
                }
                Ok(None) => {}
                Err(DecodeError::Unavailable) => {
                    if !announced_decoder_error {
                        let _ = app.emit(
                            EMULATOR_STATUS_EVENT,
                            StatusPayload::new(StatusPhase::Error)
                                .with_platform("android")
                                .with_device(device_id.clone())
                                .with_message(
                                    "H.264 decoder unavailable: rebuild Cadence with --features emulator-live to stream Android frames"
                                        .to_string(),
                                ),
                        );
                        announced_decoder_error = true;
                    }
                }
                Err(err) => {
                    let _ = app.emit(
                        EMULATOR_STATUS_EVENT,
                        StatusPayload::new(StatusPhase::Error)
                            .with_platform("android")
                            .with_device(device_id.clone())
                            .with_message(format!("h264 decode failed: {err}")),
                    );
                    break;
                }
            }
        }

        let _ = (width, height);
    })
}

fn emit_status<R: Runtime>(
    app: &AppHandle<R>,
    phase: StatusPhase,
    device_id: &str,
    message: Option<String>,
) {
    let mut payload = StatusPayload::new(phase)
        .with_platform("android")
        .with_device(device_id.to_string());
    if let Some(msg) = message {
        payload = payload.with_message(msg);
    }
    let _ = app.emit(EMULATOR_STATUS_EVENT, payload);
}

fn missing_sdk(msg: impl Into<String>) -> CommandError {
    CommandError::user_fixable("android_sdk_missing", msg.into())
}

fn control_error(msg: impl Into<String>) -> CommandError {
    CommandError::system_fault("android_control_write_failed", msg.into())
}

fn chunk_text(text: &str, max_bytes: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        let mut end = (start + max_bytes).min(bytes.len());
        // Back off to a UTF-8 boundary.
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            break;
        }
        out.push(&text[start..end]);
        start = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_text_respects_char_boundaries() {
        let s = "aaaaaaaaaa😀😀😀aaaaaa";
        let chunks = chunk_text(s, 12);
        let reassembled: String = chunks.into_iter().collect();
        assert_eq!(reassembled, s);
    }
}
