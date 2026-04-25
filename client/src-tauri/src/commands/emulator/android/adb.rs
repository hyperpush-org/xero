//! Typed wrappers around `adb`. We shell out rather than speak the ADB
//! protocol directly because (a) the binary is already on disk and (b) the
//! number of commands we need is small enough that a few `Command::output()`
//! calls stay readable.
//!
//! Every call is scoped to a specific serial (`emulator-5554` style) so that
//! if the user has multiple devices connected we never fire commands at the
//! wrong one.

use std::io::{Error, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Handle to an `adb` binary plus the device serial to target.
#[derive(Debug, Clone)]
pub struct Adb {
    binary: PathBuf,
    serial: String,
}

impl Adb {
    pub fn new(binary: impl Into<PathBuf>, serial: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            serial: serial.into(),
        }
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    fn base_command(&self) -> Command {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-s").arg(&self.serial);
        cmd
    }

    /// Run `adb wait-for-device`, bounded by `timeout`.
    pub fn wait_for_device(&self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            let child = self
                .base_command()
                .arg("wait-for-device")
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()?;
            let output = child.wait_with_output()?;
            if output.status.success() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io_other(format!(
                    "adb wait-for-device failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    /// Wait until `getprop sys.boot_completed` returns `1`. The emulator
    /// reaches `device` state well before userspace is usable; polling this
    /// property is the conventional way to wait out the rest of boot.
    pub fn wait_for_boot(&self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.getprop("sys.boot_completed") {
                Ok(value) if value.trim() == "1" => return Ok(()),
                Ok(_) | Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(750));
                }
                Ok(value) => {
                    return Err(io_other(format!(
                        "device never finished booting (sys.boot_completed={value})"
                    )));
                }
                Err(err) => return Err(err),
            }
        }
    }

    /// Fetch a system property via `adb shell getprop <name>`.
    pub fn getprop(&self, name: &str) -> Result<String> {
        let output = self
            .base_command()
            .args(["shell", "getprop", name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
        if !output.status.success() {
            return Err(io_other(format!(
                "adb getprop {name} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Push a local file to the device. Used to land `scrcpy-server.jar` in
    /// `/data/local/tmp` before spawning the server.
    pub fn push(&self, local: &Path, remote: &str) -> Result<()> {
        let output = self
            .base_command()
            .arg("push")
            .arg(local)
            .arg(remote)
            .output()?;
        if !output.status.success() {
            return Err(io_other(format!(
                "adb push {} → {remote} failed: {}",
                local.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    /// `adb reverse localabstract:<name> tcp:<port>` — lets the device
    /// connect back to us at `127.0.0.1:<port>`.
    pub fn reverse(&self, remote: &str, local: &str) -> Result<()> {
        let output = self
            .base_command()
            .args(["reverse", remote, local])
            .output()?;
        if !output.status.success() {
            return Err(io_other(format!(
                "adb reverse {remote} -> {local} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    /// Remove a previously-installed reverse tunnel. Best-effort — errors
    /// are swallowed because teardown runs during cleanup paths.
    pub fn reverse_remove(&self, remote: &str) {
        let _ = self
            .base_command()
            .args(["reverse", "--remove", remote])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    /// Run `adb shell <args…>`; returns stdout on success.
    pub fn shell<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut cmd = self.base_command();
        cmd.arg("shell");
        for arg in args {
            cmd.arg(arg.as_ref());
        }
        let output = cmd.output()?;
        if !output.status.success() {
            return Err(io_other(format!(
                "adb shell failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Spawn a long-running `adb shell` child. Used for scrcpy's `app_process`
    /// launch and for `logcat`.
    pub fn shell_spawn<I, S>(&self, args: I) -> Result<std::process::Child>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut cmd = self.base_command();
        cmd.arg("shell");
        for arg in args {
            cmd.arg(arg.as_ref());
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
    }

    /// `adb install` — returns a generic error if the device reports
    /// `Failure`.
    pub fn install(&self, apk: &Path) -> Result<()> {
        let output = self
            .base_command()
            .args(["install", "-r", "-d"])
            .arg(apk)
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() || stdout.contains("Failure") {
            return Err(io_other(format!("adb install failed: {}", stdout.trim())));
        }
        Ok(())
    }

    /// `adb uninstall <package>`.
    pub fn uninstall(&self, package: &str) -> Result<()> {
        let output = self.base_command().args(["uninstall", package]).output()?;
        if !output.status.success() {
            return Err(io_other(format!(
                "adb uninstall failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }
}

fn io_other(msg: String) -> Error {
    Error::other(msg)
}

/// Run `adb start-server` without targeting a specific device; used once at
/// pipeline boot to make sure the server is up before parallel commands race.
pub fn start_server(binary: &Path) -> Result<()> {
    let output = Command::new(binary)
        .arg("start-server")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        return Err(io_other(format!(
            "adb start-server failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}
