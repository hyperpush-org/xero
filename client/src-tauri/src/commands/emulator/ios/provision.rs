//! First-run iOS Simulator auto-provisioning.
//!
//! Xcode can be installed while CoreSimulator has no installed runtimes or
//! simulator devices, especially after disk cleanup. This module performs the
//! setup Xcode would normally drive from Settings:
//!
//! 1. Run Xcode first-launch tasks.
//! 2. Download/install the iOS Simulator runtime when none is available.
//! 3. Ask CoreSimulator to scan and mount runtime disk images.
//! 4. Create a default iPhone simulator when no devices exist.
//!
//! Progress is streamed to the frontend on [`EMULATOR_IOS_PROVISION_EVENT`].

use std::ffi::CString;
use std::mem::MaybeUninit;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};

use crate::commands::emulator::events::EMULATOR_SDK_STATUS_CHANGED_EVENT;
use crate::commands::{CommandError, CommandResult};

#[cfg(target_os = "macos")]
use super::xcrun;

pub const EMULATOR_IOS_PROVISION_EVENT: &str = "emulator:ios_provision";

static PROVISIONING: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
const IOS_RUNTIME_DOWNLOAD_MIN_FREE_BYTES: u64 = 9_000_000_000;
#[cfg(target_os = "macos")]
const IOS_RUNTIME_STORAGE_PATH: &str = "/Library/Developer/CoreSimulator";
#[cfg(target_os = "macos")]
const MAX_COMMAND_DETAIL_CHARS: usize = 900;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IosProvisionPhase {
    Starting,
    RunningFirstLaunch,
    DownloadingRuntime,
    MountingRuntime,
    CreatingDevice,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IosProvisionEvent {
    pub phase: IosProvisionPhase,
    pub message: Option<String>,
    pub progress: Option<f32>,
    pub error: Option<String>,
}

impl IosProvisionEvent {
    fn phase(phase: IosProvisionPhase) -> Self {
        Self {
            phase,
            message: None,
            progress: None,
            error: None,
        }
    }

    fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = Some(msg.into());
        self
    }

    fn with_progress(mut self, value: f32) -> Self {
        self.progress = Some(value.clamp(0.0, 1.0));
        self
    }

    fn with_error(mut self, err: impl Into<String>) -> Self {
        self.error = Some(err.into());
        self
    }
}

#[tauri::command]
pub async fn emulator_ios_provision<R: Runtime + 'static>(app: AppHandle<R>) -> CommandResult<()> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        return Err(CommandError::user_fixable(
            "ios_unsupported",
            "iOS Simulator provisioning is only available on macOS.",
        ));
    }

    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn_blocking(move || run_provision_command(app))
            .await
            .map_err(|error| {
                CommandError::system_fault(
                    "ios_provision_task_failed",
                    format!("Xero could not finish iOS Simulator setup in the background: {error}"),
                )
            })?
    }
}

#[cfg(target_os = "macos")]
fn run_provision_command<R: Runtime>(app: AppHandle<R>) -> CommandResult<()> {
    if PROVISIONING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(CommandError::user_fixable(
            "ios_provision_already_running",
            "iOS Simulator setup is already in progress.",
        ));
    }
    let _guard = ProvisionGuard;

    match run_provision(&app) {
        Ok(()) => {
            emit_provision(&app, IosProvisionEvent::phase(IosProvisionPhase::Completed));
            let _ = app.emit(EMULATOR_SDK_STATUS_CHANGED_EVENT, ());
            Ok(())
        }
        Err(err) => {
            emit_provision(
                &app,
                IosProvisionEvent::phase(IosProvisionPhase::Failed).with_error(err.to_string()),
            );
            Err(err)
        }
    }
}

#[cfg(target_os = "macos")]
struct ProvisionGuard;

#[cfg(target_os = "macos")]
impl Drop for ProvisionGuard {
    fn drop(&mut self) {
        PROVISIONING.store(false, Ordering::Release);
    }
}

fn emit_provision<R: Runtime>(app: &AppHandle<R>, event: IosProvisionEvent) {
    let _ = app.emit(EMULATOR_IOS_PROVISION_EVENT, event);
}

#[cfg(target_os = "macos")]
fn run_provision<R: Runtime>(app: &AppHandle<R>) -> CommandResult<()> {
    emit_provision(
        app,
        IosProvisionEvent::phase(IosProvisionPhase::Starting)
            .with_message("Checking Xcode and CoreSimulator setup.")
            .with_progress(0.05),
    );
    ensure_xcode_cli_available()?;

    emit_provision(
        app,
        IosProvisionEvent::phase(IosProvisionPhase::RunningFirstLaunch)
            .with_message("Running Xcode first-launch tasks.")
            .with_progress(0.15),
    );
    run_command(
        "xcodebuild",
        &["-runFirstLaunch"],
        "ios_xcode_first_launch_failed",
    )?;

    if available_runtime_count()? == 0 {
        let arch = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "universal"
        };
        ensure_runtime_download_disk_space()?;
        emit_provision(
            app,
            IosProvisionEvent::phase(IosProvisionPhase::DownloadingRuntime)
                .with_message("Downloading and installing the iOS Simulator runtime.")
                .with_progress(0.35),
        );
        run_command(
            "xcodebuild",
            &["-downloadPlatform", "iOS", "-architectureVariant", arch],
            "ios_runtime_download_failed",
        )
        .or_else(|first_error| {
            if first_error.code == "ios_runtime_disk_space_low" {
                return Err(first_error);
            }
            // Some Xcode releases are pickier about architecture variants;
            // retry once with Xcode's default resolver before giving up.
            run_command(
                "xcodebuild",
                &["-downloadPlatform", "iOS"],
                "ios_runtime_download_failed",
            )
            .map_err(|_| first_error)
        })?;
    }

    emit_provision(
        app,
        IosProvisionEvent::phase(IosProvisionPhase::MountingRuntime)
            .with_message("Registering Simulator runtimes.")
            .with_progress(0.75),
    );
    run_command(
        "xcrun",
        &["simctl", "runtime", "scan-and-mount"],
        "ios_runtime_mount_failed",
    )?;

    if available_runtime_count()? == 0 {
        return Err(CommandError::user_fixable(
            "ios_runtime_missing",
            "Xcode is installed, but no iOS Simulator runtime is available after setup. Install the iOS Simulator runtime in Xcode Settings > Components, then refresh.",
        ));
    }

    if xcrun::list_devices()
        .map(|devices| devices.is_empty())
        .unwrap_or(true)
    {
        emit_provision(
            app,
            IosProvisionEvent::phase(IosProvisionPhase::CreatingDevice)
                .with_message("Creating a default iPhone simulator.")
                .with_progress(0.9),
        );
        create_default_device()?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn ensure_xcode_cli_available() -> CommandResult<()> {
    run_command("xcrun", &["--find", "simctl"], "ios_simctl_missing").map(|_| ())
}

#[cfg(target_os = "macos")]
fn ensure_runtime_download_disk_space() -> CommandResult<()> {
    let required = RequiredSpace {
        label: "about 9 GB".to_string(),
        bytes: IOS_RUNTIME_DOWNLOAD_MIN_FREE_BYTES,
    };
    let Some((path, available)) = runtime_storage_available_bytes() else {
        return Ok(());
    };
    if available < required.bytes {
        return Err(CommandError::user_fixable(
            "ios_runtime_disk_space_low",
            disk_space_error_message(Some(required), Some((path, available))),
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn available_runtime_count() -> CommandResult<usize> {
    xcrun::list_runtimes()
        .map(|runtimes| {
            runtimes
                .into_iter()
                .filter(|runtime| runtime.available && runtime.is_ios())
                .count()
        })
        .map_err(|err| {
            CommandError::system_fault(
                "ios_runtime_probe_failed",
                format!("Could not inspect iOS Simulator runtimes: {err}"),
            )
        })
}

#[cfg(target_os = "macos")]
fn create_default_device() -> CommandResult<()> {
    let candidates = [
        "iPhone 17 Pro",
        "iPhone 16 Pro",
        "iPhone 15 Pro",
        "iPhone 14",
        "iPhone 13",
        "iPhone 12",
        "iPhone 11",
        "iPhone SE (3rd generation)",
    ];

    let mut last_error: Option<CommandError> = None;
    for device_type in candidates {
        match run_command(
            "xcrun",
            &["simctl", "create", "Xero iPhone", device_type],
            "ios_simulator_create_failed",
        ) {
            Ok(_) => return Ok(()),
            Err(err) => last_error = Some(err),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        CommandError::user_fixable(
            "ios_simulator_create_failed",
            "Xero could not find a compatible iPhone simulator profile to create.",
        )
    }))
}

#[cfg(target_os = "macos")]
fn run_command(binary: &str, args: &[&str], code: &'static str) -> CommandResult<String> {
    let output = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| {
            CommandError::system_fault(code, format!("Could not run `{binary}`: {err}"))
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() {
        return Ok(stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let detail = if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{stderr}\n{stdout}")
    };
    Err(command_error(binary, args, code, &detail))
}

#[cfg(target_os = "macos")]
fn command_error(binary: &str, args: &[&str], code: &'static str, detail: &str) -> CommandError {
    if detail.to_ascii_lowercase().contains("insufficient space") {
        return CommandError::user_fixable(
            "ios_runtime_disk_space_low",
            disk_space_error_message(parse_required_space(detail), runtime_storage_available_bytes()),
        );
    }

    let detail = compact_command_detail(detail);
    CommandError::user_fixable(
        code,
        format!(
            "`{binary} {}` failed{}",
            args.join(" "),
            if detail.is_empty() {
                ".".to_string()
            } else {
                format!(": {detail}")
            },
        ),
    )
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct RequiredSpace {
    label: String,
    bytes: u64,
}

#[cfg(target_os = "macos")]
fn parse_required_space(detail: &str) -> Option<RequiredSpace> {
    let marker = "Requires ";
    let start = detail.find(marker)? + marker.len();
    let mut parts = detail[start..].split_whitespace();
    let raw_amount = parts.next()?;
    let raw_unit = parts.next()?;
    let amount = raw_amount
        .trim_matches(|c: char| !c.is_ascii_digit() && c != '.')
        .parse::<f64>()
        .ok()?;
    let unit = raw_unit.trim_matches(|c: char| !c.is_ascii_alphabetic());
    let multiplier = match unit {
        "GB" => 1_000_000_000.0,
        "GiB" => 1_073_741_824.0,
        "MB" => 1_000_000.0,
        "MiB" => 1_048_576.0,
        _ => return None,
    };
    Some(RequiredSpace {
        label: format!("{raw_amount} {unit}"),
        bytes: (amount * multiplier).ceil() as u64,
    })
}

#[cfg(target_os = "macos")]
fn disk_space_error_message(
    required: Option<RequiredSpace>,
    available: Option<(String, u64)>,
) -> String {
    let required = required.unwrap_or_else(|| RequiredSpace {
        label: "additional disk space".to_string(),
        bytes: IOS_RUNTIME_DOWNLOAD_MIN_FREE_BYTES,
    });
    let Some((path, available_bytes)) = available else {
        return format!(
            "Xcode does not have enough disk space to install the iOS Simulator runtime. It requires {}. Free disk space, then try again.",
            required.label
        );
    };

    let shortfall = required.bytes.saturating_sub(available_bytes);
    let shortfall = if shortfall > 0 {
        format!(" Free at least {} more, then try again.", format_bytes(shortfall))
    } else {
        " Free more disk space, then try again.".to_string()
    };
    format!(
        "Xcode does not have enough disk space to install the iOS Simulator runtime. It requires {}, but {} only has {} available.{}",
        required.label,
        path,
        format_bytes(available_bytes),
        shortfall
    )
}

#[cfg(target_os = "macos")]
fn runtime_storage_available_bytes() -> Option<(String, u64)> {
    let path = if Path::new(IOS_RUNTIME_STORAGE_PATH).exists() {
        IOS_RUNTIME_STORAGE_PATH
    } else {
        "/"
    };
    available_bytes(path).ok().map(|bytes| (path.to_string(), bytes))
}

#[cfg(target_os = "macos")]
fn available_bytes(path: &str) -> std::io::Result<u64> {
    let c_path = CString::new(path.as_bytes()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains nul byte")
    })?;
    let mut stat = MaybeUninit::<libc::statfs>::uninit();
    let rc = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let stat = unsafe { stat.assume_init() };
    Ok((stat.f_bavail as u64).saturating_mul(stat.f_bsize as u64))
}

#[cfg(target_os = "macos")]
fn compact_command_detail(detail: &str) -> String {
    let compact = detail.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= MAX_COMMAND_DETAIL_CHARS {
        return compact;
    }

    let mut truncated = compact
        .chars()
        .take(MAX_COMMAND_DETAIL_CHARS)
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

#[cfg(target_os = "macos")]
fn format_bytes(bytes: u64) -> String {
    let gb = bytes as f64 / 1_000_000_000.0;
    if gb >= 1.0 {
        return format!("{gb:.1} GB");
    }
    let mb = bytes as f64 / 1_000_000.0;
    format!("{mb:.0} MB")
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn parses_xcode_required_space_error() {
        let parsed = parse_required_space("Insufficient space available. Requires 8.39 GB Finding content.")
            .expect("required space");

        assert_eq!(parsed.label, "8.39 GB");
        assert_eq!(parsed.bytes, 8_390_000_000);
    }

    #[test]
    fn builds_compact_low_disk_message() {
        let message = disk_space_error_message(
            Some(RequiredSpace {
                label: "8.39 GB".to_string(),
                bytes: 8_390_000_000,
            }),
            Some((
                "/Library/Developer/CoreSimulator".to_string(),
                3_700_000_000,
            )),
        );

        assert!(message.contains("requires 8.39 GB"));
        assert!(message.contains("3.7 GB available"));
        assert!(message.contains("4.7 GB more"));
    }

    #[test]
    fn truncates_repeated_command_output() {
        let detail = "Downloading iOS Simulator: Preparing to download...".repeat(100);
        let compact = compact_command_detail(&detail);

        assert!(compact.len() < detail.len());
        assert!(compact.ends_with("..."));
    }
}
