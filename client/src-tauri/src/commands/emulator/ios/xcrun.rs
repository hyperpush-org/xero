//! `xcrun simctl` wrapper.
//!
//! We drive the iOS Simulator through Apple's `simctl` CLI for anything that
//! doesn't require the real-time streaming path. This covers: listing
//! devices, booting, shutting down, taking a one-shot screenshot, installing
//! apps, launching apps, setting the simulated location. The streaming path
//! (live H.264 video) is handled by `idb_companion` in sibling modules.

#![cfg(target_os = "macos")]

use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::session::SimulatorDescriptor;

/// Enumerate all *available* simulators across all installed runtimes, keeping
/// only the ones that can actually boot (skipping the "unavailable" entries
/// simctl lists when a runtime is missing).
pub fn list_devices() -> Result<Vec<SimulatorDescriptor>> {
    let output = Command::new("xcrun")
        .args(["simctl", "list", "devices", "available", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(io_other(format!(
            "xcrun simctl list failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let dump: SimctlListDevicesDump = serde_json::from_slice(&output.stdout)
        .map_err(|e| io_other(format!("failed to parse simctl JSON: {e}")))?;

    let mut out = Vec::new();
    for (runtime, devices) in dump.devices {
        for device in devices {
            if !device.is_available.unwrap_or(true) {
                continue;
            }
            let (width, height, scale, is_tablet) =
                dimensions_for_device_type(&device.device_type_identifier);
            out.push(SimulatorDescriptor {
                udid: device.udid,
                display_name: format!("{} ({})", device.name, humanize_runtime(&runtime)),
                is_tablet,
                width,
                height,
                scale,
            });
        }
    }
    Ok(out)
}

/// Boot a simulator, waiting up to `timeout` for it to reach `Booted` state.
/// Idempotent — if the device is already booted we return immediately.
pub fn boot(udid: &str, timeout: Duration) -> Result<()> {
    let state = device_state(udid)?;
    if state == "Booted" {
        return Ok(());
    }

    let output = Command::new("xcrun")
        .args(["simctl", "boot", udid])
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .output()?;
    if !output.status.success() {
        let msg = String::from_utf8_lossy(&output.stderr).to_lowercase();
        // `simctl boot` on an already-booted device returns a non-zero status
        // with "is already booted" on stderr. Treat that as success.
        if !msg.contains("already booted") {
            return Err(io_other(format!(
                "xcrun simctl boot failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
    }

    let deadline = Instant::now() + timeout;
    loop {
        match device_state(udid) {
            Ok(state) if state == "Booted" => return Ok(()),
            Ok(_) | Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(500));
            }
            Ok(state) => {
                return Err(io_other(format!(
                    "simulator {udid} never reached Booted (current state: {state})"
                )));
            }
            Err(err) => return Err(err),
        }
    }
}

/// Shut down a simulator by UDID. Best-effort — if the device wasn't running
/// we swallow the error.
pub fn shutdown(udid: &str) -> Result<()> {
    let output = Command::new("xcrun")
        .args(["simctl", "shutdown", udid])
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if stderr.contains("unable to shutdown device in current state: shutdown")
            || stderr.contains("no such device")
        {
            return Ok(());
        }
        return Err(io_other(format!(
            "xcrun simctl shutdown failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

/// Take a single PNG screenshot of the booted simulator. Returns the bytes.
pub fn screenshot(udid: &str) -> Result<Vec<u8>> {
    let output = Command::new("xcrun")
        .args(["simctl", "io", udid, "screenshot", "--type=png", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(io_other(format!(
            "xcrun simctl io screenshot failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

/// `simctl install <udid> <path-to-.app-or-.ipa>`.
pub fn install(udid: &str, bundle: &Path) -> Result<()> {
    let output = Command::new("xcrun")
        .arg("simctl")
        .arg("install")
        .arg(udid)
        .arg(bundle)
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(io_other(format!(
            "simctl install failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

pub fn uninstall(udid: &str, bundle_id: &str) -> Result<()> {
    let output = Command::new("xcrun")
        .args(["simctl", "uninstall", udid, bundle_id])
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(io_other(format!(
            "simctl uninstall failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

pub fn launch(udid: &str, bundle_id: &str, args: &[String]) -> Result<()> {
    let mut cmd = Command::new("xcrun");
    cmd.args(["simctl", "launch", udid, bundle_id]);
    for arg in args {
        cmd.arg(arg);
    }
    let output = cmd.stderr(Stdio::piped()).output()?;
    if !output.status.success() {
        return Err(io_other(format!(
            "simctl launch failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

pub fn terminate(udid: &str, bundle_id: &str) -> Result<()> {
    let output = Command::new("xcrun")
        .args(["simctl", "terminate", udid, bundle_id])
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(io_other(format!(
            "simctl terminate failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

pub fn list_apps(udid: &str) -> Result<String> {
    let output = Command::new("xcrun")
        .args(["simctl", "listapps", udid])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(io_other(format!(
            "simctl listapps failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn set_location(udid: &str, lat: f64, lon: f64) -> Result<()> {
    let arg = format!("{lat},{lon}");
    let output = Command::new("xcrun")
        .args(["simctl", "location", udid, "set", &arg])
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(io_other(format!(
            "simctl location failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

/// Push a silent APNS payload to a running app.
pub fn push_notification(udid: &str, bundle_id: &str, payload: &str) -> Result<()> {
    let mut child = Command::new("xcrun")
        .args(["simctl", "push", udid, bundle_id, "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(payload.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(io_other(format!(
            "simctl push failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

/// Set the simulator's UI orientation.
pub fn set_orientation(udid: &str, value: &str) -> Result<()> {
    let output = Command::new("xcrun")
        .args(["simctl", "ui", udid, "orientation", value])
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(io_other(format!(
            "simctl ui orientation failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

fn device_state(udid: &str) -> Result<String> {
    let output = Command::new("xcrun")
        .args(["simctl", "list", "devices", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(io_other(format!(
            "xcrun simctl list (for state) failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let dump: SimctlListDevicesDump = serde_json::from_slice(&output.stdout)
        .map_err(|e| io_other(format!("failed to parse simctl state JSON: {e}")))?;

    for devices in dump.devices.values() {
        for d in devices {
            if d.udid == udid {
                return Ok(d.state.clone());
            }
        }
    }
    Err(io_other(format!("udid {udid} not found in simctl list")))
}

fn io_other(msg: String) -> Error {
    Error::new(ErrorKind::Other, msg)
}

#[derive(Debug, Deserialize)]
struct SimctlListDevicesDump {
    devices: std::collections::BTreeMap<String, Vec<SimctlDevice>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SimctlDevice {
    udid: String,
    name: String,
    state: String,
    device_type_identifier: String,
    #[serde(default)]
    is_available: Option<bool>,
}

fn dimensions_for_device_type(id: &str) -> (Option<u32>, Option<u32>, Option<f32>, bool) {
    // id is like `com.apple.CoreSimulator.SimDeviceType.iPhone-15-Pro`.
    let lower = id.to_ascii_lowercase();
    if lower.contains("ipad") {
        return (Some(1668), Some(2388), Some(2.0), true);
    }
    if lower.contains("iphone-se") {
        return (Some(750), Some(1334), Some(2.0), false);
    }
    if lower.contains("iphone-15-pro-max") || lower.contains("iphone-16-pro-max") {
        return (Some(1290), Some(2796), Some(3.0), false);
    }
    if lower.contains("iphone") {
        return (Some(1179), Some(2556), Some(3.0), false);
    }
    (None, None, None, false)
}

fn humanize_runtime(id: &str) -> String {
    // id is like `com.apple.CoreSimulator.SimRuntime.iOS-17-2`.
    id.rsplit('.').next().unwrap_or(id).replace('-', " ")
}

/// Resolve the bundled `idb_companion` binary path. Prefers the Tauri
/// resource directory for distributed builds and falls back to common
/// Homebrew locations for local development.
pub fn resolve_idb_companion<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    use tauri::{path::BaseDirectory, Manager};

    for rel in ["binaries/idb_companion", "idb_companion"] {
        if let Ok(path) = app.path().resolve(rel, BaseDirectory::Resource) {
            if path.is_file() {
                return Some(path);
            }
        }
    }
    super::sdk::probe().idb_companion
}
