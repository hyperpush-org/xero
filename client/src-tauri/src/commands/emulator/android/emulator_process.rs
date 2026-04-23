//! Spawns the Android emulator binary for a named AVD, with flags tuned for
//! headless streaming.
//!
//! The emulator produces its own window on every platform by default. We pass
//! `-no-window` to suppress that — scrcpy streams the framebuffer to us over
//! the control-plane socket rather than us capturing the OS-level window.

use std::io::Result;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::commands::emulator::process::ChildGuard;

use super::adb::Adb;

/// Configuration knobs for boot. The defaults mirror the plan's spawn line:
/// `-no-window -no-audio -no-snapshot-save -no-boot-anim` — plus an explicit
/// `-port` so we get a predictable serial.
#[derive(Debug, Clone)]
pub struct EmulatorLaunch {
    pub avd_name: String,
    /// Which emulator console port to bind (emulator uses this as `5554 + idx`
    /// in practice). Must be even and 5554–5682.
    pub console_port: u16,
    pub extra_args: Vec<String>,
}

impl EmulatorLaunch {
    pub fn new(avd_name: impl Into<String>) -> Self {
        Self {
            avd_name: avd_name.into(),
            console_port: pick_console_port(),
            extra_args: Vec::new(),
        }
    }

    pub fn serial(&self) -> String {
        format!("emulator-{}", self.console_port)
    }
}

/// Boot an emulator and return a guarded handle to the process plus the serial
/// ADB should use. Blocks for up to `boot_timeout` waiting for
/// `sys.boot_completed=1`.
pub fn spawn(
    emulator_bin: &Path,
    adb_bin: &Path,
    launch: &EmulatorLaunch,
    boot_timeout: Duration,
) -> Result<(ChildGuard, Adb)> {
    let serial = launch.serial();

    // Ensure the ADB server is live before the emulator phones home — avoids
    // a startup race where adb forks its own server mid-boot.
    super::adb::start_server(adb_bin)?;

    let mut cmd = Command::new(emulator_bin);
    cmd.arg(format!("@{}", launch.avd_name))
        .args([
            "-no-window",
            "-no-audio",
            "-no-snapshot-save",
            "-no-boot-anim",
        ])
        .args(["-port", &launch.console_port.to_string()])
        .args(&launch.extra_args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let child = cmd.spawn()?;
    let mut guard = ChildGuard::new("android-emulator", child);

    let adb = Adb::new(adb_bin.to_path_buf(), serial.clone());

    // Race: wait for the emulator to become visible to adb. If the child
    // process dies early (e.g. AVD lock held), surface that promptly instead
    // of blocking the full `boot_timeout`.
    let deadline = Instant::now() + boot_timeout;
    loop {
        match guard.try_wait() {
            Ok(Some(status)) => {
                let tail = guard.stderr_tail();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("emulator exited before boot (status={status}). stderr tail: {tail}"),
                ));
            }
            Ok(None) if Instant::now() >= deadline => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "emulator {} did not boot within {:?}",
                        launch.avd_name, boot_timeout
                    ),
                ));
            }
            Ok(None) => {}
            Err(err) => return Err(err),
        }

        if adb.wait_for_device(Duration::from_secs(2)).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "emulator reached adb but ran out of time before boot_completed",
        ));
    }
    adb.wait_for_boot(remaining)?;

    Ok((guard, adb))
}

/// Pick a fresh emulator console port. We don't coordinate with other
/// emulator instances so this is a best-effort: try 5554 first, then walk up
/// by 2. In practice Cadence runs one emulator at a time, so the default will
/// usually work.
fn pick_console_port() -> u16 {
    use std::net::{SocketAddrV4, TcpListener};
    for port in (5554..=5682).step_by(2) {
        if TcpListener::bind(SocketAddrV4::new([127, 0, 0, 1].into(), port)).is_ok() {
            return port;
        }
    }
    5554
}
