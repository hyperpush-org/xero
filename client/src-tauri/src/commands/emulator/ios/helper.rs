//! Swift helper binary lifecycle management.
//!
//! The `xero-ios-helper` is a standalone macOS daemon that owns all
//! low-level Simulator interaction: ScreenCaptureKit frame capture and
//! IndigoHID input injection. This module handles spawning the helper,
//! health-checking via UDS connectivity, and resolving the binary path.
//!
//! Structurally mirrors `idb_companion.rs` — spawn + ChildGuard + health
//! check loop.

use std::io::Result;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, Runtime};

use crate::commands::emulator::process::ChildGuard;

/// Launch configuration for the Swift helper binary.
pub struct HelperLaunch {
    pub binary: PathBuf,
    pub udid: String,
    pub socket_path: PathBuf,
}

impl HelperLaunch {
    pub fn new(binary: impl Into<PathBuf>, udid: impl Into<String>) -> Result<Self> {
        let udid = udid.into();
        // macOS sockaddr_un.sun_path is 104 bytes. Use /tmp/ directly
        // (not $TMPDIR which is a long path under /var/folders/) and
        // truncate the UDID to keep the total under 104.
        let short_id = &udid[..udid.len().min(8)];
        let socket_path = PathBuf::from(format!("/tmp/xero-ih-{short_id}.sock"));
        // Remove stale socket from a previous crash.
        let _ = std::fs::remove_file(&socket_path);
        Ok(Self {
            binary: binary.into(),
            udid,
            socket_path,
        })
    }
}

/// Running helper instance. Drop terminates the helper process.
pub struct Helper {
    pub socket_path: PathBuf,
    pub guard: ChildGuard,
}

/// Spawn the Swift helper and wait for it to start accepting UDS connections.
pub fn spawn(launch: HelperLaunch, startup_timeout: Duration) -> Result<Helper> {
    let mut cmd = Command::new(&launch.binary);
    cmd.args([
        "--udid",
        &launch.udid,
        "--socket-path",
        launch.socket_path.to_str().unwrap_or_default(),
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::piped());

    let child = cmd.spawn()?;
    let mut guard = ChildGuard::new("xero-ios-helper", child);

    let deadline = Instant::now() + startup_timeout;
    loop {
        // Check if process exited early.
        match guard.try_wait() {
            Ok(Some(status)) => {
                let tail = guard.stderr_tail();
                return Err(std::io::Error::other(format!(
                    "xero-ios-helper exited before accepting connections (status={status}). \
                     stderr: {tail}"
                )));
            }
            Ok(None) if Instant::now() >= deadline => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "xero-ios-helper did not start listening on {} within {:?}",
                        launch.socket_path.display(),
                        startup_timeout
                    ),
                ));
            }
            Ok(None) => {}
            Err(err) => return Err(err),
        }

        // Attempt UDS connection.
        if UnixStream::connect(&launch.socket_path).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(150));
    }

    Ok(Helper {
        socket_path: launch.socket_path,
        guard,
    })
}

/// Locate the helper binary. Check (in order):
///   1. Tauri resource directory (bundled builds)
///   2. Adjacent to the running executable (dev builds)
///   3. $PATH
pub fn resolve_helper_binary<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    const BINARY_NAME: &str = "xero-ios-helper";

    // 1. Tauri resource directory.
    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidate: PathBuf = resource_dir.join(BINARY_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // 2. Next to the running executable (cargo build puts it in target/<profile>/).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(BINARY_NAME);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // 3. $PATH lookup.
    if let Ok(output) = Command::new("which")
        .arg(BINARY_NAME)
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let p = PathBuf::from(&path);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_launch_generates_valid_socket_path() {
        let launch =
            HelperLaunch::new("/usr/bin/true", "AAAA-BBBB-CCCC").expect("launch creation");
        assert!(launch
            .socket_path
            .to_str()
            .unwrap()
            .contains("xero-ios-helper-AAAA-BBBB-CCCC.sock"));
    }

    #[test]
    fn helper_launch_cleans_stale_socket() {
        let udid = "test-stale-socket-cleanup";
        let path = std::env::temp_dir().join(format!("xero-ios-helper-{udid}.sock"));
        // Create a fake stale file.
        std::fs::write(&path, b"stale").ok();
        assert!(path.exists());

        let _launch = HelperLaunch::new("/usr/bin/true", udid).expect("launch creation");
        assert!(!path.exists(), "stale socket should be removed");
    }
}
