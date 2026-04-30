//! Cross-platform app lifecycle (install / launch / terminate / list).
//!
//! Android uses `adb install` / `adb shell am`; iOS uses `simctl`. Both
//! return the same [`AppDescriptor`] shape so agents don't have to branch.

use std::path::Path;

use crate::commands::emulator::android::adb::Adb;
use crate::commands::CommandError;

use super::AppDescriptor;

// ---------- Android ---------------------------------------------------------

pub fn android_install(adb: &Adb, apk: &Path) -> Result<AppDescriptor, CommandError> {
    adb.install(apk)
        .map_err(|err| install_err(err.to_string()))?;

    // `adb install` doesn't print the bundle id; we best-effort extract it
    // from the filename (com.example.apk → com.example) and fall back to
    // an empty string. The caller can always call `list` to enumerate.
    let display_name = apk
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());
    Ok(AppDescriptor {
        bundle_id: String::new(),
        display_name,
        version: None,
        installed_at: None,
    })
}

pub fn android_uninstall(adb: &Adb, bundle_id: &str) -> Result<(), CommandError> {
    adb.uninstall(bundle_id)
        .map_err(|err| CommandError::system_fault("app_uninstall_failed", err.to_string()))
}

pub fn android_launch(adb: &Adb, bundle_id: &str, args: &[String]) -> Result<(), CommandError> {
    // Resolve the launcher activity so we can use `am start -n <pkg>/<activity>`.
    // Fall back to `monkey` which auto-picks the launcher activity — less
    // precise but works when manifest parsing fails.
    let activity = resolve_launcher_activity(adb, bundle_id).unwrap_or_default();
    if !activity.is_empty() {
        let component = format!("{bundle_id}/{activity}");
        let mut cmd = vec![
            "am".to_string(),
            "start".to_string(),
            "-n".to_string(),
            component,
        ];
        for arg in args {
            cmd.push("--es".to_string());
            cmd.push("xero_arg".to_string());
            cmd.push(arg.clone());
        }
        adb.shell(cmd)
            .map_err(|e| CommandError::system_fault("app_launch_failed", e.to_string()))?;
        return Ok(());
    }

    adb.shell([
        "monkey".to_string(),
        "-p".to_string(),
        bundle_id.to_string(),
        "-c".to_string(),
        "android.intent.category.LAUNCHER".to_string(),
        "1".to_string(),
    ])
    .map_err(|e| CommandError::system_fault("app_launch_failed", e.to_string()))?;
    Ok(())
}

pub fn android_terminate(adb: &Adb, bundle_id: &str) -> Result<(), CommandError> {
    adb.shell(["am", "force-stop", bundle_id])
        .map_err(|e| CommandError::system_fault("app_terminate_failed", e.to_string()))?;
    Ok(())
}

pub fn android_list(adb: &Adb) -> Result<Vec<AppDescriptor>, CommandError> {
    let stdout = adb
        .shell(["pm", "list", "packages", "-3"])
        .map_err(|e| CommandError::system_fault("app_list_failed", e.to_string()))?;
    let apps = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("package:"))
        .map(|id| AppDescriptor {
            bundle_id: id.trim().to_string(),
            display_name: None,
            version: None,
            installed_at: None,
        })
        .collect();
    Ok(apps)
}

fn resolve_launcher_activity(adb: &Adb, bundle_id: &str) -> Option<String> {
    let stdout = adb
        .shell(["cmd", "package", "resolve-activity", "--brief", bundle_id])
        .ok()?;
    // Output is two lines: the priority (ignore) and `<pkg>/<activity>`.
    stdout
        .lines()
        .find(|line| line.contains('/'))
        .and_then(|line| line.split('/').nth(1).map(|s| s.trim().to_string()))
}

fn install_err(detail: String) -> CommandError {
    CommandError::system_fault("app_install_failed", detail)
}

// ---------- iOS -------------------------------------------------------------

#[cfg(target_os = "macos")]
pub fn ios_install(udid: &str, bundle: &Path) -> Result<AppDescriptor, CommandError> {
    use crate::commands::emulator::ios::xcrun;
    xcrun::install(udid, bundle)
        .map_err(|e| CommandError::system_fault("app_install_failed", e.to_string()))?;
    Ok(AppDescriptor {
        bundle_id: String::new(),
        display_name: bundle
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string()),
        version: None,
        installed_at: None,
    })
}

#[cfg(target_os = "macos")]
pub fn ios_uninstall(udid: &str, bundle_id: &str) -> Result<(), CommandError> {
    use crate::commands::emulator::ios::xcrun;
    xcrun::uninstall(udid, bundle_id)
        .map_err(|e| CommandError::system_fault("app_uninstall_failed", e.to_string()))
}

#[cfg(target_os = "macos")]
pub fn ios_launch(udid: &str, bundle_id: &str, args: &[String]) -> Result<(), CommandError> {
    use crate::commands::emulator::ios::xcrun;
    xcrun::launch(udid, bundle_id, args)
        .map_err(|e| CommandError::system_fault("app_launch_failed", e.to_string()))
}

#[cfg(target_os = "macos")]
pub fn ios_terminate(udid: &str, bundle_id: &str) -> Result<(), CommandError> {
    use crate::commands::emulator::ios::xcrun;
    xcrun::terminate(udid, bundle_id)
        .map_err(|e| CommandError::system_fault("app_terminate_failed", e.to_string()))
}

#[cfg(target_os = "macos")]
pub fn ios_list(udid: &str) -> Result<Vec<AppDescriptor>, CommandError> {
    use crate::commands::emulator::ios::xcrun;
    let raw = xcrun::list_apps(udid)
        .map_err(|e| CommandError::system_fault("app_list_failed", e.to_string()))?;
    Ok(parse_listapps_output(&raw))
}

/// `simctl listapps` returns plist-like output:
/// ```text
/// "com.apple.Preferences" =     {
///     ApplicationType = System;
///     CFBundleDisplayName = Settings;
///     CFBundleShortVersionString = "1.0";
/// };
/// ```
///
/// We parse the subset we need without pulling in a plist crate — the
/// format is stable and regular enough.
#[cfg(target_os = "macos")]
fn parse_listapps_output(raw: &str) -> Vec<AppDescriptor> {
    let mut out = Vec::new();
    let mut current: Option<AppDescriptor> = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with('{') && trimmed.contains('=') {
            if let Some(previous) = current.take() {
                out.push(previous);
            }
            if let Some(bundle) = trimmed.split('=').next() {
                let bundle = bundle.trim().trim_matches('"').to_string();
                if !bundle.is_empty() {
                    current = Some(AppDescriptor {
                        bundle_id: bundle,
                        display_name: None,
                        version: None,
                        installed_at: None,
                    });
                }
            }
        } else if let Some(app) = current.as_mut() {
            if let Some(rest) = trimmed.strip_prefix("CFBundleDisplayName = ") {
                app.display_name = Some(strip_plist_value(rest));
            } else if let Some(rest) = trimmed.strip_prefix("CFBundleShortVersionString = ") {
                app.version = Some(strip_plist_value(rest));
            }
        }
    }
    if let Some(tail) = current {
        out.push(tail);
    }
    out
}

#[cfg(target_os = "macos")]
fn strip_plist_value(raw: &str) -> String {
    raw.trim_end_matches(';')
        .trim()
        .trim_matches('"')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn parse_listapps_extracts_bundles() {
        let raw = r#"    "com.apple.Preferences" =     {
        ApplicationType = System;
        CFBundleDisplayName = Settings;
        CFBundleShortVersionString = "17.2";
    };
    "com.example.Foo" =     {
        CFBundleDisplayName = "Foo Bar";
    };
"#;
        let apps = parse_listapps_output(raw);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].bundle_id, "com.apple.Preferences");
        assert_eq!(apps[0].display_name.as_deref(), Some("Settings"));
        assert_eq!(apps[0].version.as_deref(), Some("17.2"));
        assert_eq!(apps[1].display_name.as_deref(), Some("Foo Bar"));
    }
}
