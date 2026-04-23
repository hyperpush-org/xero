//! iOS Simulator SDK / idb_companion discovery.
//!
//! On non-macOS hosts the struct is returned empty — the iOS pipeline is
//! gated by `cfg(target_os = "macos")` everywhere else.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

/// A very thin device descriptor used when the iOS pipeline is compiled out.
/// The frontend receives an empty list in that case.
#[derive(Debug, Clone, Serialize)]
pub struct IosDeviceStub {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Default)]
pub struct IosSdk {
    pub xcrun: Option<PathBuf>,
    pub simctl: Option<PathBuf>,
    pub idb_companion: Option<PathBuf>,
}

impl IosSdk {
    pub fn is_usable(&self) -> bool {
        self.xcrun.is_some() && self.simctl.is_some()
    }
}

#[cfg(target_os = "macos")]
pub fn probe() -> IosSdk {
    let xcrun = which_binary("xcrun");
    let simctl = xcrun.as_ref().and_then(|_| locate_simctl());
    let idb_companion = find_idb_companion();
    IosSdk {
        xcrun,
        simctl,
        idb_companion,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn probe() -> IosSdk {
    IosSdk::default()
}

/// Like [`probe`] but also consults the Tauri resource directory for the
/// bundled `idb_companion` tree. Used by the SDK-status command so packaged
/// builds report `idb_companion_present = true` without the user having to
/// install anything.
#[cfg(target_os = "macos")]
pub fn probe_with_app<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> IosSdk {
    let mut sdk = probe();
    if sdk.idb_companion.is_none() {
        sdk.idb_companion = super::xcrun::resolve_bundled_idb_companion(app);
    }
    sdk
}

#[cfg(not(target_os = "macos"))]
pub fn probe_with_app<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) -> IosSdk {
    IosSdk::default()
}

fn which_binary(name: &str) -> Option<PathBuf> {
    let locator = if cfg!(windows) { "where" } else { "which" };
    if let Ok(out) = Command::new(locator).arg(name).output() {
        if out.status.success() {
            let first = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !first.is_empty() {
                let pb = PathBuf::from(first);
                if pb.is_file() {
                    return Some(pb);
                }
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn locate_simctl() -> Option<PathBuf> {
    // `xcrun --find simctl` returns the absolute path; falls back to the
    // default location inside Xcode.app if the developer directory isn't set.
    if let Ok(out) = Command::new("xcrun").args(["--find", "simctl"]).output() {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                let pb = PathBuf::from(path);
                if pb.is_file() {
                    return Some(pb);
                }
            }
        }
    }
    let default = Path::new("/usr/bin/simctl");
    if default.is_file() {
        return Some(default.to_path_buf());
    }
    None
}

fn find_idb_companion() -> Option<PathBuf> {
    // Check standard install locations from `brew install facebook/fb/idb-companion`
    // as well as any locally-bundled sidecar path. The Tauri app will set the
    // bundled path via `app.path().resolve()` at runtime; that takes precedence.
    which_binary("idb_companion").or_else(|| {
        for candidate in [
            "/opt/homebrew/bin/idb_companion",
            "/usr/local/bin/idb_companion",
        ] {
            let p = PathBuf::from(candidate);
            if p.is_file() {
                return Some(p);
            }
        }
        None
    })
}
