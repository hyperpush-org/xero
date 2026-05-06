//! Top-level SDK probe. Delegates to platform-specific modules so the
//! frontend can render a single combined status payload.

use serde::{Deserialize, Serialize};

use super::android::sdk as android_sdk;

/// Result of probing the host machine for each platform's SDK. Surfaced to
/// the frontend so the missing-SDK panel can render without blocking the
/// user on start.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SdkStatus {
    pub android: AndroidSdkStatus,
    pub ios: IosSdkStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AndroidSdkStatus {
    pub present: bool,
    pub sdk_root: Option<String>,
    pub emulator_path: Option<String>,
    pub adb_path: Option<String>,
    pub avdmanager_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IosSdkStatus {
    pub present: bool,
    pub xcrun_path: Option<String>,
    pub simctl_path: Option<String>,
    pub idb_companion_present: bool,
    /// Host OS supports iOS Simulator (only macOS does).
    pub supported: bool,
    /// Xero has been granted Accessibility permission (macOS) — required
    /// for `CGEventPostToPid` to deliver taps to Simulator.app. Always `false`
    /// on non-macOS hosts.
    pub ax_permission_granted: bool,
    /// Xero has been granted Screen Recording permission (macOS) — required
    /// by ScreenCaptureKit for the Swift helper's frame capture. Always
    /// `false` on non-macOS hosts.
    pub screen_recording_permission_granted: bool,
    /// The Swift helper binary (`xero-ios-helper`) was found on disk.
    pub helper_present: bool,
}

pub fn probe_sdks<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> SdkStatus {
    SdkStatus {
        android: probe_android_status(app),
        ios: probe_ios_status(app),
    }
}

fn probe_android_status<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> AndroidSdkStatus {
    let sdk = android_sdk::probe_with_app(app);
    AndroidSdkStatus {
        present: sdk.is_usable(),
        sdk_root: sdk.sdk_root.map(path_to_string),
        emulator_path: sdk.emulator.map(path_to_string),
        adb_path: sdk.adb.map(path_to_string),
        avdmanager_path: sdk.avdmanager.map(path_to_string),
    }
}

fn probe_ios_status<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> IosSdkStatus {
    #[cfg(target_os = "macos")]
    {
        let ios = super::ios::sdk::probe_with_app(app);
        IosSdkStatus {
            present: ios.is_usable(),
            xcrun_path: ios.xcrun.map(path_to_string),
            simctl_path: ios.simctl.map(path_to_string),
            idb_companion_present: ios.idb_companion.is_some(),
            supported: true,
            ax_permission_granted: super::ios::cg_input::ax_permission_granted(),
            screen_recording_permission_granted: super::ios::cg_input::screen_recording_permission_granted(),
            helper_present: super::ios::helper::resolve_helper_binary(app).is_some(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        IosSdkStatus {
            present: false,
            xcrun_path: None,
            simctl_path: None,
            idb_companion_present: false,
            supported: false,
            ax_permission_granted: false,
            screen_recording_permission_granted: false,
            helper_present: false,
        }
    }
}

fn path_to_string(p: std::path::PathBuf) -> String {
    p.to_string_lossy().into_owned()
}
