//! Android SDK discovery — finds `adb`, `emulator`, and (for future probes)
//! related tools. The return value feeds both the frontend's missing-SDK
//! panel and the runtime bootstrap in [`super::session`].

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Locations for every binary the Android pipeline needs at runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AndroidSdk {
    pub sdk_root: Option<PathBuf>,
    pub emulator: Option<PathBuf>,
    pub adb: Option<PathBuf>,
    pub avdmanager: Option<PathBuf>,
}

impl AndroidSdk {
    /// `true` when we have enough to boot a device. `avdmanager` is optional —
    /// we fall back to `emulator -list-avds` if it's missing.
    pub fn is_usable(&self) -> bool {
        self.emulator.is_some() && self.adb.is_some()
    }

    pub fn emulator_path(&self) -> Option<&Path> {
        self.emulator.as_deref()
    }

    pub fn adb_path(&self) -> Option<&Path> {
        self.adb.as_deref()
    }
}

/// Like [`probe`] but threads the Tauri `AppHandle` through for callers
/// that want to merge app-managed SDK locations on top of the host
/// defaults. Phase 1 keeps this a thin passthrough; the Android
/// auto-provisioning flow in a later phase extends the fallback chain.
pub fn probe_with_app<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) -> AndroidSdk {
    probe()
}

/// Probe the host for an Android SDK. Order of precedence:
/// 1. `ANDROID_HOME`, `ANDROID_SDK_ROOT`, or `ANDROID_SDK_HOME` env vars.
/// 2. `which adb` / `which emulator` (covers `brew install android-platform-tools`).
/// 3. `~/Library/Android/sdk` (Android Studio default on macOS).
/// 4. `~/Android/Sdk` (Android Studio default on Linux).
/// 5. `%LOCALAPPDATA%/Android/Sdk` (Android Studio default on Windows).
pub fn probe() -> AndroidSdk {
    let env_root = env::var("ANDROID_HOME")
        .ok()
        .or_else(|| env::var("ANDROID_SDK_ROOT").ok())
        .or_else(|| env::var("ANDROID_SDK_HOME").ok())
        .map(PathBuf::from)
        .filter(|p| p.exists());

    let default_roots = default_sdk_roots();
    let mut candidate_roots: Vec<PathBuf> = Vec::new();
    if let Some(root) = env_root.clone() {
        candidate_roots.push(root);
    }
    candidate_roots.extend(default_roots.into_iter().filter(|p| p.exists()));

    let adb =
        which_binary("adb").or_else(|| find_in_roots(&candidate_roots, &["platform-tools/adb"]));
    let emulator = which_binary("emulator")
        .or_else(|| find_in_roots(&candidate_roots, &["emulator/emulator"]));
    let avdmanager = which_binary("avdmanager").or_else(|| {
        find_in_roots(
            &candidate_roots,
            &[
                "cmdline-tools/latest/bin/avdmanager",
                "tools/bin/avdmanager",
            ],
        )
    });

    // Best guess at the SDK root: the env var if set, otherwise derive from adb
    // (which lives at <root>/platform-tools/adb).
    let sdk_root = env_root.or_else(|| {
        adb.as_ref()
            .and_then(|adb| adb.parent()?.parent().map(PathBuf::from))
    });

    AndroidSdk {
        sdk_root,
        emulator,
        adb,
        avdmanager,
    }
}

fn default_sdk_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = dirs::home_dir() {
        out.push(home.join("Library/Android/sdk"));
        out.push(home.join("Android/Sdk"));
        out.push(home.join("AppData/Local/Android/Sdk"));
    }
    out
}

fn find_in_roots(roots: &[PathBuf], rels: &[&str]) -> Option<PathBuf> {
    for root in roots {
        for rel in rels {
            let path = root.join(rel);
            let with_ext = if cfg!(windows) {
                path.with_extension("exe")
            } else {
                path.clone()
            };
            for candidate in [with_ext, path] {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
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

    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(exe_name(name));
            if candidate.is_file() {
                Some(candidate)
            } else {
                None
            }
        })
    })
}

fn exe_name(name: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("{name}.exe"))
    } else {
        PathBuf::from(name)
    }
}
