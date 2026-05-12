//! Dev-only synthetic frame generator. Produces a color-cycling test pattern
//! so the pipeline (FrameBus → IPC frame command → webview `<img>`) can be verified
//! without a real Android or iOS device.
//!
//! Enabled by the `emulator-synthetic` cargo feature. When disabled this
//! module is compiled out entirely.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Runtime};

use super::codec::encode_jpeg_rgba;
use super::events::{StatusPayload, StatusPhase, EMULATOR_STATUS_EVENT};
use super::frame_bus::{publish_and_emit, FrameBus};
use tauri::Emitter;

const SYNTHETIC_WIDTH: u32 = 360;
const SYNTHETIC_HEIGHT: u32 = 640;
const FRAME_INTERVAL: Duration = Duration::from_millis(33); // ~30 FPS

pub struct SyntheticSession {
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl SyntheticSession {
    pub fn spawn<R: Runtime>(
        app: AppHandle<R>,
        bus: Arc<FrameBus>,
        platform: String,
        device_id: String,
    ) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let worker_bus = Arc::clone(&bus);

        let _ = app.emit(
            EMULATOR_STATUS_EVENT,
            StatusPayload::new(StatusPhase::Streaming)
                .with_platform(platform.clone())
                .with_device(device_id.clone())
                .with_message("synthetic frame source"),
        );

        let handle = thread::spawn(move || {
            let start = Instant::now();
            let mut frame_no: u32 = 0;
            while !worker_shutdown.load(Ordering::Relaxed) {
                let elapsed = start.elapsed().as_secs_f32();
                let rgba = render_pattern(frame_no, elapsed);
                match encode_jpeg_rgba(&rgba, SYNTHETIC_WIDTH, SYNTHETIC_HEIGHT) {
                    Ok(jpeg) => {
                        publish_and_emit(
                            &app,
                            &worker_bus,
                            SYNTHETIC_WIDTH,
                            SYNTHETIC_HEIGHT,
                            jpeg,
                        );
                    }
                    Err(error) => {
                        let _ = app.emit(
                            EMULATOR_STATUS_EVENT,
                            StatusPayload::new(StatusPhase::Error)
                                .with_platform(platform.clone())
                                .with_device(device_id.clone())
                                .with_message(format!("synthetic encode failed: {error}")),
                        );
                        break;
                    }
                }
                frame_no = frame_no.wrapping_add(1);
                thread::sleep(FRAME_INTERVAL);
            }
        });

        Self {
            shutdown,
            handle: Some(handle),
        }
    }

    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for SyntheticSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Simple HSV-cycling gradient with a moving vertical bar. Not pretty but
/// makes temporal updates (and stuck frames) obvious at a glance.
fn render_pattern(frame_no: u32, elapsed: f32) -> Vec<u8> {
    let width = SYNTHETIC_WIDTH as usize;
    let height = SYNTHETIC_HEIGHT as usize;
    let mut buf = Vec::with_capacity(width * height * 4);

    let hue = (elapsed * 36.0) % 360.0; // full rotation every 10s
    let bar_x = ((frame_no as f32 * 2.0) as usize) % width;

    for y in 0..height {
        for x in 0..width {
            let (r, g, b) = if x == bar_x {
                (255, 255, 255)
            } else {
                let local_hue =
                    (hue + (x as f32 / width as f32) * 120.0 + (y as f32 / height as f32) * 60.0)
                        % 360.0;
                hsv_to_rgb(local_hue, 0.7, 0.85)
            };
            buf.push(r);
            buf.push(g);
            buf.push(b);
            buf.push(255);
        }
    }

    buf
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

pub const fn synthetic_width() -> u32 {
    SYNTHETIC_WIDTH
}

pub const fn synthetic_height() -> u32 {
    SYNTHETIC_HEIGHT
}
