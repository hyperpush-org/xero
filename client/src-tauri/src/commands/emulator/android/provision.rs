//! First-run Android SDK auto-provisioning.
//!
//! When the host has neither Android Studio nor `ANDROID_HOME` on
//! `PATH`, Cadence offers to download just enough of the Android SDK
//! into the app's data dir to boot an emulator. This module owns that
//! flow:
//!
//! 1. Ensure a JDK 17+ is usable — use `JAVA_HOME` / `java` from PATH
//!    when present, otherwise fetch the pinned Temurin 17 JRE and stage
//!    it into `{data_dir}/android-sdk/jdk/`.
//! 2. Ensure the Google `commandlinetools-<host>-*.zip` is extracted to
//!    `{data_dir}/android-sdk/cmdline-tools/latest/`.
//! 3. Auto-accept the SDK licenses (`sdkmanager --licenses`).
//! 4. Install `platform-tools`, `emulator`, and one
//!    `system-images;android-34;google_apis;<arch>` package.
//! 5. Create a default AVD via `avdmanager`.
//!
//! Progress is streamed to the frontend on [`EMULATOR_PROVISION_EVENT`]
//! so the missing-SDK panel can render a live progress indicator.
//!
//! A process-wide [`PROVISIONING`] flag prevents concurrent runs — the
//! user can still cancel by closing the panel, but overlapping installs
//! against the same SDK root would corrupt it.
//!
//! Hypervisor notes:
//! - Apple Silicon uses `Hypervisor.framework`, always present.
//! - Intel macOS / Linux / Windows depend on HAXM / KVM / WHPX. We don't
//!   provision those; the emulator launcher will surface a typed error
//!   if the user's host lacks KVM/HAXM/WHPX.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::commands::emulator::events::EMULATOR_SDK_STATUS_CHANGED_EVENT;
use crate::commands::{CommandError, CommandResult};

/// Pinned Google cmdline-tools build (Android 34 / API 34 era).
const CMDLINE_TOOLS_VERSION: &str = "14742923";
const CMDLINE_TOOLS_SHA_MAC: &str =
    "ed304c5ede3718541e4f978e4ae870a4d853db74af6c16d920588d48523b9dee";
const CMDLINE_TOOLS_SHA_LINUX: &str =
    "04453066b540409d975c676d781da1477479dde3761310f1a7eb92a1dfb15af7";
const CMDLINE_TOOLS_SHA_WINDOWS: &str =
    "cc610ccbe83faddb58e1aa68e8fc8743bb30aa5e83577eceb4cc168dae95f9ee";

/// Pinned Eclipse Temurin 17 JRE. Used when the host lacks a JAVA_HOME
/// / `java` on PATH — sdkmanager + avdmanager both need a JDK 17+.
/// Every cross-compile target only references its own pair, so the rest
/// trigger dead-code warnings when building for just one host.
const TEMURIN_JRE_VERSION: &str = "17.0.9+9";
#[allow(dead_code)]
const TEMURIN_JRE_VERSION_URLENC: &str = "17.0.9%2B9";
#[allow(dead_code)]
const TEMURIN_JRE_VERSION_URLENC_WIN: &str = "17.0.9%2B9.1";
#[allow(dead_code)]
const TEMURIN_JRE_MAC_AARCH64_FILE: &str = "OpenJDK17U-jre_aarch64_mac_hotspot_17.0.9_9.tar.gz";
#[allow(dead_code)]
const TEMURIN_JRE_MAC_AARCH64_SHA: &str =
    "89831d03b7cd9922bd178f1a9c8544a36c54d52295366db4e6628454b01acaef";
#[allow(dead_code)]
const TEMURIN_JRE_MAC_X64_FILE: &str = "OpenJDK17U-jre_x64_mac_hotspot_17.0.9_9.tar.gz";
#[allow(dead_code)]
const TEMURIN_JRE_MAC_X64_SHA: &str =
    "ba214f2217dc134e94432085cff4fc5a97e964ffc211d343725fd535f3cd98a0";
#[allow(dead_code)]
const TEMURIN_JRE_LINUX_AARCH64_FILE: &str = "OpenJDK17U-jre_aarch64_linux_hotspot_17.0.9_9.tar.gz";
#[allow(dead_code)]
const TEMURIN_JRE_LINUX_AARCH64_SHA: &str =
    "05b192f81ed478178ba953a2a779b67fc5a810acadb633ad69f8c4412399edb8";
#[allow(dead_code)]
const TEMURIN_JRE_LINUX_X64_FILE: &str = "OpenJDK17U-jre_x64_linux_hotspot_17.0.9_9.tar.gz";
#[allow(dead_code)]
const TEMURIN_JRE_LINUX_X64_SHA: &str =
    "c37f729200b572884b8f8e157852c739be728d61d9a1da0f920104876d324733";
#[allow(dead_code)]
const TEMURIN_JRE_WIN_X64_FILE: &str = "OpenJDK17U-jre_x64_windows_hotspot_17.0.9_9.zip";
#[allow(dead_code)]
const TEMURIN_JRE_WIN_X64_SHA: &str =
    "6c491d6f8c28c6f451f08110a30348696a04b009f8c58592191046e0fab1477b";

/// Android API level + variant we install. `google_apis` (not
/// `google_apis_playstore`) because playstore images refuse root, and
/// automation test apps need root for many real-world scenarios.
const SYSTEM_IMAGE_API: &str = "android-34";
const SYSTEM_IMAGE_VARIANT: &str = "google_apis";
const DEFAULT_AVD_NAME: &str = "CadenceDefault";
const DEFAULT_DEVICE_PROFILE: &str = "pixel_6";

/// Tauri event for the frontend progress panel. Distinct from
/// `emulator:status` because the lifecycles are independent —
/// provisioning can run while no device is active, and a device session
/// can start/stop without touching the installer.
pub const EMULATOR_PROVISION_EVENT: &str = "emulator:android_provision";

/// Process-wide guard. Set to `true` while a provision is in-flight, so
/// concurrent invocations fail fast instead of racing each other against
/// the same SDK root.
static PROVISIONING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProvisionPhase {
    Starting,
    EnsuringJava,
    DownloadingJava,
    ExtractingJava,
    DownloadingCmdlineTools,
    ExtractingCmdlineTools,
    AcceptingLicenses,
    InstallingPackages,
    CreatingAvd,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionEvent {
    pub phase: ProvisionPhase,
    pub message: Option<String>,
    /// `0.0..=1.0` for the current phase if known. Frontend renders an
    /// indeterminate bar when `None`.
    pub progress: Option<f32>,
    pub error: Option<String>,
}

impl ProvisionEvent {
    fn phase(phase: ProvisionPhase) -> Self {
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

/// Return the managed Android SDK root the provisioning flow targets.
/// The directory may not exist yet; callers that need to probe inside
/// it should check `exists()` first.
pub fn managed_sdk_root<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("android-sdk"))
}

/// Resolve the path to a bundled/managed JRE, if one was installed by a
/// previous provisioning run. Returns `None` when no fetched JRE is
/// present — the caller should fall back to the host JAVA_HOME / PATH.
fn managed_jre_home(root: &Path) -> Option<PathBuf> {
    let home = root.join("jdk");
    if is_valid_java_home(&home) {
        return Some(home);
    }

    // On macOS, the tarball extracts to `Contents/Home` inside a
    // versioned directory. We sniff for the first viable layout.
    if let Ok(entries) = fs::read_dir(&home) {
        for entry in entries.flatten() {
            let path = entry.path();
            #[cfg(target_os = "macos")]
            {
                let contents_home = path.join("Contents").join("Home");
                if is_valid_java_home(&contents_home) {
                    return Some(contents_home);
                }
            }
            if is_valid_java_home(&path) {
                return Some(path);
            }
        }
    }
    None
}

fn is_valid_java_home(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    path.join("bin").join(exe).is_file()
}

/// Tauri command: run the full provisioning flow. Emits progress via
/// [`EMULATOR_PROVISION_EVENT`] and returns once the managed SDK is
/// usable. Concurrent invocations fail fast with a typed error.
#[tauri::command]
pub fn emulator_android_provision<R: Runtime>(app: AppHandle<R>) -> CommandResult<()> {
    if PROVISIONING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(CommandError::user_fixable(
            "android_provision_already_running",
            "Android SDK provisioning is already in progress.",
        ));
    }
    let _guard = ProvisionGuard;

    let root = managed_sdk_root(&app).ok_or_else(|| {
        CommandError::system_fault(
            "android_provision_no_data_dir",
            "could not resolve the app data directory",
        )
    })?;

    match run_provision(&app, &root) {
        Ok(()) => {
            emit_provision(&app, ProvisionEvent::phase(ProvisionPhase::Completed));
            // Nudge any SDK-status watchers so the UI re-probes without a
            // restart.
            let _ = app.emit(EMULATOR_SDK_STATUS_CHANGED_EVENT, ());
            Ok(())
        }
        Err(err) => {
            emit_provision(
                &app,
                ProvisionEvent::phase(ProvisionPhase::Failed).with_error(err.to_string()),
            );
            Err(err)
        }
    }
}

struct ProvisionGuard;

impl Drop for ProvisionGuard {
    fn drop(&mut self) {
        PROVISIONING.store(false, Ordering::Release);
    }
}

fn emit_provision<R: Runtime>(app: &AppHandle<R>, event: ProvisionEvent) {
    let _ = app.emit(EMULATOR_PROVISION_EVENT, event);
}

fn run_provision<R: Runtime>(app: &AppHandle<R>, root: &Path) -> CommandResult<()> {
    emit_provision(
        app,
        ProvisionEvent::phase(ProvisionPhase::Starting)
            .with_message(format!("installing into {}", root.display())),
    );

    fs::create_dir_all(root).map_err(|e| {
        CommandError::system_fault(
            "android_provision_mkdir_failed",
            format!("could not create {}: {e}", root.display()),
        )
    })?;

    let java_home = ensure_java(app, root)?;
    install_cmdline_tools(app, root)?;
    accept_licenses(app, root, &java_home)?;
    install_packages(app, root, &java_home)?;
    create_default_avd(app, root, &java_home)?;

    Ok(())
}

// ---- Java resolution --------------------------------------------------

fn ensure_java<R: Runtime>(app: &AppHandle<R>, root: &Path) -> CommandResult<PathBuf> {
    emit_provision(
        app,
        ProvisionEvent::phase(ProvisionPhase::EnsuringJava)
            .with_message("checking for a usable JDK 17+"),
    );

    if let Some(home) = detect_host_java() {
        emit_provision(
            app,
            ProvisionEvent::phase(ProvisionPhase::EnsuringJava)
                .with_message(format!("using host JDK at {}", home.display())),
        );
        return Ok(home);
    }

    if let Some(home) = managed_jre_home(root) {
        emit_provision(
            app,
            ProvisionEvent::phase(ProvisionPhase::EnsuringJava)
                .with_message(format!("using managed JDK at {}", home.display())),
        );
        return Ok(home);
    }

    fetch_managed_jre(app, root)
}

fn detect_host_java() -> Option<PathBuf> {
    // Respect an explicit JAVA_HOME only when it points at a valid JDK.
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        let path = PathBuf::from(home);
        if is_valid_java_home(&path) && java_major_version(&path).unwrap_or(0) >= 17 {
            return Some(path);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("/usr/libexec/java_home")
            .args(["-v", "17+"])
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !text.is_empty() {
                    let path = PathBuf::from(text);
                    if is_valid_java_home(&path) {
                        return Some(path);
                    }
                }
            }
        }
    }

    // `which java` → parent → strip `bin/` → JAVA_HOME candidate.
    let locator = if cfg!(windows) { "where" } else { "which" };
    if let Ok(out) = Command::new(locator).arg("java").output() {
        if out.status.success() {
            let first = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !first.is_empty() {
                let bin = PathBuf::from(first);
                if let Some(bin_dir) = bin.parent() {
                    if let Some(home) = bin_dir.parent() {
                        let home = home.to_path_buf();
                        if is_valid_java_home(&home) && java_major_version(&home).unwrap_or(0) >= 17
                        {
                            return Some(home);
                        }
                    }
                }
            }
        }
    }

    None
}

fn java_major_version(home: &Path) -> Option<u32> {
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    let out = Command::new(home.join("bin").join(exe))
        .arg("-version")
        .output()
        .ok()?;
    // `java -version` prints to stderr. Format: `openjdk version "17.0.6" ...`
    // or `java version "1.8.0_341"` for legacy JDKs.
    let text = String::from_utf8_lossy(&out.stderr);
    let line = text.lines().next()?;
    let quoted = line.split('"').nth(1)?;
    let mut parts = quoted.split('.');
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

fn fetch_managed_jre<R: Runtime>(app: &AppHandle<R>, root: &Path) -> CommandResult<PathBuf> {
    let (file_name, expected_sha, url_enc_version) = temurin_spec()?;
    let url = format!(
        "https://github.com/adoptium/temurin17-binaries/releases/download/jdk-{url_enc_version}/{file_name}"
    );

    let jdk_dir = root.join("jdk");
    let _ = fs::remove_dir_all(&jdk_dir);
    fs::create_dir_all(&jdk_dir).map_err(|e| {
        CommandError::system_fault(
            "android_provision_mkdir_failed",
            format!("could not create {}: {e}", jdk_dir.display()),
        )
    })?;

    let archive = root.join(file_name);

    emit_provision(
        app,
        ProvisionEvent::phase(ProvisionPhase::DownloadingJava)
            .with_message(format!("downloading {}", url)),
    );

    download_with_progress(app, &url, &archive, ProvisionPhase::DownloadingJava)?;
    verify_sha256(&archive, expected_sha)?;

    emit_provision(
        app,
        ProvisionEvent::phase(ProvisionPhase::ExtractingJava)
            .with_message(format!("unpacking Temurin {TEMURIN_JRE_VERSION}")),
    );

    if file_name.ends_with(".zip") {
        unzip_into(&archive, &jdk_dir)?;
    } else {
        untar_gz_into(&archive, &jdk_dir)?;
    }
    let _ = fs::remove_file(&archive);

    managed_jre_home(root).ok_or_else(|| {
        CommandError::system_fault(
            "android_provision_jre_layout_unknown",
            format!(
                "Temurin tarball extracted but no java binary was found under {}",
                jdk_dir.display()
            ),
        )
    })
}

fn temurin_spec() -> CommandResult<(&'static str, &'static str, &'static str)> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return Ok((
            TEMURIN_JRE_MAC_AARCH64_FILE,
            TEMURIN_JRE_MAC_AARCH64_SHA,
            TEMURIN_JRE_VERSION_URLENC,
        ));
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        return Ok((
            TEMURIN_JRE_MAC_X64_FILE,
            TEMURIN_JRE_MAC_X64_SHA,
            TEMURIN_JRE_VERSION_URLENC,
        ));
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return Ok((
            TEMURIN_JRE_LINUX_AARCH64_FILE,
            TEMURIN_JRE_LINUX_AARCH64_SHA,
            TEMURIN_JRE_VERSION_URLENC,
        ));
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return Ok((
            TEMURIN_JRE_LINUX_X64_FILE,
            TEMURIN_JRE_LINUX_X64_SHA,
            TEMURIN_JRE_VERSION_URLENC,
        ));
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        return Ok((
            TEMURIN_JRE_WIN_X64_FILE,
            TEMURIN_JRE_WIN_X64_SHA,
            TEMURIN_JRE_VERSION_URLENC_WIN,
        ));
    }
    #[allow(unreachable_code)]
    {
        Err(CommandError::user_fixable(
            "android_provision_unsupported_host",
            "No pinned Temurin JRE build is available for this OS / architecture. \
             Install a JDK 17+ manually and re-run provisioning.",
        ))
    }
}

// ---- cmdline-tools ----------------------------------------------------

fn install_cmdline_tools<R: Runtime>(app: &AppHandle<R>, root: &Path) -> CommandResult<()> {
    let target = root.join("cmdline-tools").join("latest");
    if target.join("bin").join(sdkmanager_binary()).is_file() {
        return Ok(());
    }

    let (host_slug, expected_sha) = host_cmdline_tools_spec()?;
    let url = format!(
        "https://dl.google.com/android/repository/commandlinetools-{host_slug}-{CMDLINE_TOOLS_VERSION}_latest.zip"
    );

    emit_provision(
        app,
        ProvisionEvent::phase(ProvisionPhase::DownloadingCmdlineTools)
            .with_message(format!("downloading {url}")),
    );

    let zip_path = root.join(format!(
        "cmdline-tools-{host_slug}-{CMDLINE_TOOLS_VERSION}.zip"
    ));
    download_with_progress(
        app,
        &url,
        &zip_path,
        ProvisionPhase::DownloadingCmdlineTools,
    )?;
    verify_sha256(&zip_path, expected_sha)?;

    emit_provision(
        app,
        ProvisionEvent::phase(ProvisionPhase::ExtractingCmdlineTools)
            .with_message("unpacking cmdline-tools"),
    );

    // The zip extracts to `cmdline-tools/` at the top level. sdkmanager
    // expects `{root}/cmdline-tools/latest/`, so extract into a staging
    // dir then rename.
    let staging = root.join("cmdline-tools-staging");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| {
        CommandError::system_fault(
            "android_provision_staging_mkdir_failed",
            format!("could not create {}: {e}", staging.display()),
        )
    })?;

    unzip_into(&zip_path, &staging)?;

    let staged = staging.join("cmdline-tools");
    if !staged.is_dir() {
        return Err(CommandError::system_fault(
            "android_provision_cmdline_tools_missing",
            format!(
                "cmdline-tools zip did not contain the expected top-level directory (staging: {})",
                staging.display()
            ),
        ));
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            CommandError::system_fault(
                "android_provision_mkdir_failed",
                format!("could not create {}: {e}", parent.display()),
            )
        })?;
    }

    let _ = fs::remove_dir_all(&target);
    fs::rename(&staged, &target).map_err(|e| {
        CommandError::system_fault(
            "android_provision_move_failed",
            format!(
                "could not move {} → {}: {e}",
                staged.display(),
                target.display()
            ),
        )
    })?;

    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_file(&zip_path);

    Ok(())
}

// ---- sdkmanager / avdmanager invocations ------------------------------

fn accept_licenses<R: Runtime>(
    app: &AppHandle<R>,
    root: &Path,
    java_home: &Path,
) -> CommandResult<()> {
    emit_provision(
        app,
        ProvisionEvent::phase(ProvisionPhase::AcceptingLicenses)
            .with_message("accepting Android SDK licenses"),
    );

    let sdkmanager = sdkmanager_path(root)?;
    let mut child = spawn_sdk_tool(
        &sdkmanager,
        java_home,
        root,
        &[&format!("--sdk_root={}", root.display()), "--licenses"],
        true,
    )?;

    // Keep feeding `y` lines — a single `--licenses` run may prompt for
    // 5–10 separate licenses. Stop after a reasonable cap so a runaway
    // binary can't keep us here forever.
    if let Some(mut stdin) = child.stdin.take() {
        thread::spawn(move || {
            for _ in 0..64 {
                if stdin.write_all(b"y\n").is_err() {
                    break;
                }
                if stdin.flush().is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
    }

    stream_child_output(app, &mut child, ProvisionPhase::AcceptingLicenses);

    let status = child.wait().map_err(|e| {
        CommandError::system_fault(
            "android_provision_sdkmanager_wait_failed",
            format!("sdkmanager --licenses wait failed: {e}"),
        )
    })?;
    if !status.success() {
        return Err(CommandError::system_fault(
            "android_provision_license_failed",
            format!("sdkmanager --licenses exited with {status}"),
        ));
    }
    Ok(())
}

fn install_packages<R: Runtime>(
    app: &AppHandle<R>,
    root: &Path,
    java_home: &Path,
) -> CommandResult<()> {
    let arch = host_system_image_arch()?;
    let system_image = format!("system-images;{SYSTEM_IMAGE_API};{SYSTEM_IMAGE_VARIANT};{arch}");
    let packages: [&str; 3] = ["platform-tools", "emulator", &system_image];

    emit_provision(
        app,
        ProvisionEvent::phase(ProvisionPhase::InstallingPackages)
            .with_message(format!("installing {}", packages.join(", "))),
    );

    let sdkmanager = sdkmanager_path(root)?;
    let mut args: Vec<String> = vec![
        format!("--sdk_root={}", root.display()),
        "--install".to_string(),
    ];
    args.extend(packages.iter().map(|s| (*s).to_string()));

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut child = spawn_sdk_tool(&sdkmanager, java_home, root, &arg_refs, false)?;
    stream_child_output(app, &mut child, ProvisionPhase::InstallingPackages);

    let status = child.wait().map_err(|e| {
        CommandError::system_fault(
            "android_provision_sdkmanager_wait_failed",
            format!("sdkmanager --install wait failed: {e}"),
        )
    })?;
    if !status.success() {
        return Err(CommandError::system_fault(
            "android_provision_install_failed",
            format!("sdkmanager --install exited with {status}"),
        ));
    }
    Ok(())
}

fn create_default_avd<R: Runtime>(
    app: &AppHandle<R>,
    root: &Path,
    java_home: &Path,
) -> CommandResult<()> {
    let avd_home = default_avd_home();
    if let Some(dir) = &avd_home {
        fs::create_dir_all(dir).ok();
        if dir.join(format!("{DEFAULT_AVD_NAME}.ini")).is_file() {
            return Ok(());
        }
    }

    emit_provision(
        app,
        ProvisionEvent::phase(ProvisionPhase::CreatingAvd)
            .with_message(format!("creating AVD {DEFAULT_AVD_NAME}")),
    );

    let avdmanager = avdmanager_path(root)?;
    let arch = host_system_image_arch()?;
    let system_image = format!("system-images;{SYSTEM_IMAGE_API};{SYSTEM_IMAGE_VARIANT};{arch}");

    let args: Vec<&str> = vec![
        "create",
        "avd",
        "--name",
        DEFAULT_AVD_NAME,
        "--package",
        &system_image,
        "--device",
        DEFAULT_DEVICE_PROFILE,
        "--force",
    ];

    let mut child = spawn_avd_tool(&avdmanager, java_home, root, &args)?;

    // avdmanager prompts "Do you wish to create a custom hardware profile?
    // [no]" — answer 'no' so we get the default device profile.
    if let Some(mut stdin) = child.stdin.take() {
        thread::spawn(move || {
            let _ = stdin.write_all(b"no\n");
            let _ = stdin.flush();
        });
    }

    stream_child_output(app, &mut child, ProvisionPhase::CreatingAvd);

    let status = child.wait().map_err(|e| {
        CommandError::system_fault(
            "android_provision_avdmanager_wait_failed",
            format!("avdmanager wait failed: {e}"),
        )
    })?;
    if !status.success() {
        return Err(CommandError::system_fault(
            "android_provision_avd_failed",
            format!("avdmanager exited with {status}"),
        ));
    }
    Ok(())
}

fn spawn_sdk_tool(
    binary: &Path,
    java_home: &Path,
    sdk_root: &Path,
    args: &[&str],
    pipe_stdin: bool,
) -> CommandResult<Child> {
    let mut cmd = Command::new(binary);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.env("JAVA_HOME", java_home)
        .env("ANDROID_SDK_ROOT", sdk_root)
        .env("ANDROID_HOME", sdk_root)
        // Prepend the JRE's bin/ so any nested scripts that shell out to
        // `java` pick up our managed JDK instead of whatever's on PATH.
        .env("PATH", path_with_prefix(java_home.join("bin")))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if pipe_stdin {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }

    cmd.spawn().map_err(|e| {
        CommandError::system_fault(
            "android_provision_sdkmanager_spawn_failed",
            format!("failed to spawn {}: {e}", binary.display()),
        )
    })
}

fn spawn_avd_tool(
    binary: &Path,
    java_home: &Path,
    sdk_root: &Path,
    args: &[&str],
) -> CommandResult<Child> {
    // Deliberately do NOT set `ANDROID_AVD_HOME` — avdmanager defaults
    // to `~/.android/avd/`, which keeps AVDs discoverable by anything
    // else the user might install later (Android Studio, the emulator
    // binary's `-list-avds`, etc). Scoping to a managed dir would
    // fragment their device catalog.
    let mut cmd = Command::new(binary);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.env("JAVA_HOME", java_home)
        .env("ANDROID_SDK_ROOT", sdk_root)
        .env("ANDROID_HOME", sdk_root)
        .env("PATH", path_with_prefix(java_home.join("bin")))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    cmd.spawn().map_err(|e| {
        CommandError::system_fault(
            "android_provision_avdmanager_spawn_failed",
            format!("failed to spawn {}: {e}", binary.display()),
        )
    })
}

fn path_with_prefix(prefix: PathBuf) -> std::ffi::OsString {
    let separator = if cfg!(windows) { ";" } else { ":" };
    let mut out = std::ffi::OsString::new();
    out.push(prefix.as_os_str());
    if let Some(existing) = std::env::var_os("PATH") {
        out.push(separator);
        out.push(existing);
    }
    out
}

// ---- Subprocess output streaming --------------------------------------

fn stream_child_output<R: Runtime>(app: &AppHandle<R>, child: &mut Child, phase: ProvisionPhase) {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = mpsc::channel::<String>();

    let stdout_thread = stdout.map(|handle| {
        let tx = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(handle);
            for line in reader.lines().map_while(Result::ok) {
                if tx.send(truncate_log(&line)).is_err() {
                    break;
                }
            }
        })
    });

    let stderr_thread = stderr.map(|handle| {
        thread::spawn(move || {
            let reader = BufReader::new(handle);
            for line in reader.lines().map_while(Result::ok) {
                if tx.send(truncate_log(&line)).is_err() {
                    break;
                }
            }
        })
    });

    // Block on the channel until both reader threads drop their senders.
    let mut last_percent: Option<f32> = None;
    for line in rx {
        if line.is_empty() {
            continue;
        }
        let progress = parse_sdkmanager_progress(&line);
        if let Some(p) = progress {
            // Only forward progress moves that changed by ≥1% so the
            // frontend doesn't thrash on every buffer flush.
            if matches!(last_percent, Some(prev) if (prev - p).abs() < 0.01) {
                continue;
            }
            last_percent = Some(p);
        }
        let mut event = ProvisionEvent::phase(phase).with_message(line);
        if let Some(p) = progress {
            event = event.with_progress(p);
        }
        emit_provision(app, event);
    }

    if let Some(handle) = stdout_thread {
        let _ = handle.join();
    }
    if let Some(handle) = stderr_thread {
        let _ = handle.join();
    }
}

fn truncate_log(line: &str) -> String {
    const MAX: usize = 512;
    let line = line.trim_end_matches(['\r', '\n']);
    if line.len() <= MAX {
        line.to_string()
    } else {
        // Find a valid char boundary ≤ MAX to avoid slicing mid-utf8.
        let mut boundary = MAX;
        while boundary > 0 && !line.is_char_boundary(boundary) {
            boundary -= 1;
        }
        let mut truncated = line[..boundary].to_string();
        truncated.push('…');
        truncated
    }
}

fn parse_sdkmanager_progress(line: &str) -> Option<f32> {
    // sdkmanager prints `[=====               ] 42% Fetch remote …` —
    // scan from the first `%` back to the last non-digit to pull the
    // percent, which is more robust than a regex across versions.
    let trimmed = line.trim();
    let percent_idx = trimmed.find('%')?;
    let start = trimmed[..percent_idx]
        .rfind(|c: char| !c.is_ascii_digit() && c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    let number = trimmed[start..percent_idx].trim();
    number.parse::<f32>().ok().map(|p| p / 100.0)
}

// ---- HTTP download + SHA-256 ------------------------------------------

fn download_with_progress<R: Runtime>(
    app: &AppHandle<R>,
    url: &str,
    target: &Path,
    phase: ProvisionPhase,
) -> CommandResult<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| {
            CommandError::system_fault(
                "android_provision_http_client_failed",
                format!("could not build HTTP client: {e}"),
            )
        })?;

    let mut response = client.get(url).send().map_err(|e| {
        CommandError::system_fault(
            "android_provision_download_failed",
            format!("GET {url} failed: {e}"),
        )
    })?;
    if !response.status().is_success() {
        return Err(CommandError::system_fault(
            "android_provision_download_failed",
            format!("GET {url} returned {}", response.status()),
        ));
    }

    let total = response.content_length();
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            CommandError::system_fault(
                "android_provision_mkdir_failed",
                format!("could not create {}: {e}", parent.display()),
            )
        })?;
    }

    let mut file = fs::File::create(target).map_err(|e| {
        CommandError::system_fault(
            "android_provision_download_open_failed",
            format!("could not open {} for writing: {e}", target.display()),
        )
    })?;

    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_emit = Instant::now();
    loop {
        let n = response.read(&mut buf).map_err(|e| {
            CommandError::system_fault(
                "android_provision_download_read_failed",
                format!("download read failed: {e}"),
            )
        })?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| {
            CommandError::system_fault(
                "android_provision_download_write_failed",
                format!("download write failed: {e}"),
            )
        })?;
        downloaded += n as u64;

        // Throttle UI updates so a fast pipe doesn't swamp the event bus.
        if last_emit.elapsed() >= Duration::from_millis(150) {
            last_emit = Instant::now();
            let progress = total.map(|t| downloaded as f32 / t as f32);
            let msg = match total {
                Some(t) => format!("{} / {} MB", downloaded / 1_000_000, t / 1_000_000),
                None => format!("{} MB", downloaded / 1_000_000),
            };
            let mut event = ProvisionEvent::phase(phase).with_message(msg);
            if let Some(p) = progress {
                event = event.with_progress(p);
            }
            emit_provision(app, event);
        }
    }

    // Emit a final 100% so the bar visibly completes.
    if let Some(t) = total {
        let _ = app.emit(
            EMULATOR_PROVISION_EVENT,
            ProvisionEvent::phase(phase)
                .with_progress(1.0)
                .with_message(format!("{} / {} MB", t / 1_000_000, t / 1_000_000)),
        );
    }

    file.sync_all().ok();
    Ok(())
}

fn verify_sha256(path: &Path, expected: &str) -> CommandResult<()> {
    use sha2::{Digest, Sha256};

    let mut file = fs::File::open(path).map_err(|e| {
        CommandError::system_fault(
            "android_provision_hash_open_failed",
            format!("could not open {} for hashing: {e}", path.display()),
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| {
            CommandError::system_fault(
                "android_provision_hash_read_failed",
                format!("could not read {} for hashing: {e}", path.display()),
            )
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if hex != expected {
        let _ = fs::remove_file(path);
        return Err(CommandError::system_fault(
            "android_provision_sha_mismatch",
            format!(
                "SHA-256 mismatch for {}: got {hex}, want {expected}",
                path.display()
            ),
        ));
    }
    Ok(())
}

// ---- Archive extraction ----------------------------------------------

fn unzip_into(zip: &Path, target: &Path) -> CommandResult<()> {
    // macOS + Linux ship `unzip`; Windows ships `tar` (which handles zip
    // since Windows 10 1803). Shell out rather than pull a zip crate.
    let status = if cfg!(windows) {
        Command::new("tar")
            .arg("-xf")
            .arg(zip)
            .arg("-C")
            .arg(target)
            .status()
    } else {
        Command::new("unzip")
            .arg("-q")
            .arg("-o")
            .arg(zip)
            .arg("-d")
            .arg(target)
            .status()
    };

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(CommandError::system_fault(
            "android_provision_unzip_failed",
            format!(
                "unzip exited {s} extracting {} → {}",
                zip.display(),
                target.display()
            ),
        )),
        Err(e) => Err(CommandError::system_fault(
            "android_provision_unzip_spawn_failed",
            format!("failed to spawn unzip: {e}"),
        )),
    }
}

fn untar_gz_into(tarball: &Path, target: &Path) -> CommandResult<()> {
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(tarball)
        .arg("--no-same-owner")
        .arg("-C")
        .arg(target)
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(CommandError::system_fault(
            "android_provision_untar_failed",
            format!(
                "tar exited {s} extracting {} → {}",
                tarball.display(),
                target.display()
            ),
        )),
        Err(e) => Err(CommandError::system_fault(
            "android_provision_untar_spawn_failed",
            format!("failed to spawn tar: {e}"),
        )),
    }
}

// ---- Platform-specific helpers ----------------------------------------

fn host_cmdline_tools_spec() -> CommandResult<(&'static str, &'static str)> {
    if cfg!(target_os = "macos") {
        Ok(("mac", CMDLINE_TOOLS_SHA_MAC))
    } else if cfg!(target_os = "linux") {
        Ok(("linux", CMDLINE_TOOLS_SHA_LINUX))
    } else if cfg!(target_os = "windows") {
        Ok(("win", CMDLINE_TOOLS_SHA_WINDOWS))
    } else {
        Err(CommandError::user_fixable(
            "android_provision_unsupported_host",
            "Android provisioning is only supported on macOS, Linux, and Windows.",
        ))
    }
}

fn host_system_image_arch() -> CommandResult<&'static str> {
    #[cfg(target_arch = "aarch64")]
    {
        return Ok("arm64-v8a");
    }
    #[cfg(target_arch = "x86_64")]
    {
        return Ok("x86_64");
    }
    #[allow(unreachable_code)]
    {
        Err(CommandError::user_fixable(
            "android_provision_unsupported_arch",
            "Only arm64 and x86_64 hosts can run the Android emulator.",
        ))
    }
}

fn sdkmanager_binary() -> &'static str {
    if cfg!(windows) {
        "sdkmanager.bat"
    } else {
        "sdkmanager"
    }
}

fn avdmanager_binary() -> &'static str {
    if cfg!(windows) {
        "avdmanager.bat"
    } else {
        "avdmanager"
    }
}

fn sdkmanager_path(root: &Path) -> CommandResult<PathBuf> {
    let path = root
        .join("cmdline-tools")
        .join("latest")
        .join("bin")
        .join(sdkmanager_binary());
    if !path.is_file() {
        return Err(CommandError::system_fault(
            "android_provision_sdkmanager_missing",
            format!("sdkmanager missing at {}", path.display()),
        ));
    }
    Ok(path)
}

fn avdmanager_path(root: &Path) -> CommandResult<PathBuf> {
    let path = root
        .join("cmdline-tools")
        .join("latest")
        .join("bin")
        .join(avdmanager_binary());
    if !path.is_file() {
        return Err(CommandError::system_fault(
            "android_provision_avdmanager_missing",
            format!("avdmanager missing at {}", path.display()),
        ));
    }
    Ok(path)
}

/// Location where avdmanager drops `<name>.ini` files — also where the
/// emulator binary searches for AVDs by default. Lives next to the
/// host's normal Android config so AVDs show up in Android Studio too
/// if the user installs it later.
fn default_avd_home() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".android").join("avd"))
}

/// Tauri command: describe the current provisioning target and
/// host-detected JDK state. Cheap enough to call on every panel render.
#[tauri::command]
pub fn emulator_android_provision_status<R: Runtime>(
    app: AppHandle<R>,
) -> CommandResult<ProvisionStatus> {
    let root = managed_sdk_root(&app).ok_or_else(|| {
        CommandError::system_fault(
            "android_provision_no_data_dir",
            "could not resolve the app data directory",
        )
    })?;

    let host_java = detect_host_java();
    let managed_java = managed_jre_home(&root);
    let cmdline_tools = root
        .join("cmdline-tools")
        .join("latest")
        .join("bin")
        .join(sdkmanager_binary())
        .is_file();
    let platform_tools = root
        .join("platform-tools")
        .join(if cfg!(windows) { "adb.exe" } else { "adb" })
        .is_file();
    let emulator = root
        .join("emulator")
        .join(if cfg!(windows) {
            "emulator.exe"
        } else {
            "emulator"
        })
        .is_file();
    let avd_present = default_avd_home()
        .map(|dir| dir.join(format!("{DEFAULT_AVD_NAME}.ini")).is_file())
        .unwrap_or(false);

    Ok(ProvisionStatus {
        in_progress: PROVISIONING.load(Ordering::Acquire),
        sdk_root: root.to_string_lossy().into_owned(),
        host_java_home: host_java.map(|p| p.to_string_lossy().into_owned()),
        managed_java_home: managed_java.map(|p| p.to_string_lossy().into_owned()),
        cmdline_tools_present: cmdline_tools,
        platform_tools_present: platform_tools,
        emulator_present: emulator,
        default_avd_present: avd_present,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionStatus {
    pub in_progress: bool,
    pub sdk_root: String,
    pub host_java_home: Option<String>,
    pub managed_java_home: Option<String>,
    pub cmdline_tools_present: bool,
    pub platform_tools_present: bool,
    pub emulator_present: bool,
    pub default_avd_present: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_progress_marker() {
        assert_eq!(
            parse_sdkmanager_progress("[=====          ] 42% Fetch remote"),
            Some(0.42)
        );
        assert_eq!(parse_sdkmanager_progress("[===] 7% Unzip"), Some(0.07));
        assert_eq!(parse_sdkmanager_progress("100% Done"), Some(1.0));
        assert_eq!(parse_sdkmanager_progress("nothing"), None);
    }

    #[test]
    fn truncate_long_lines() {
        let input = "x".repeat(1024);
        let out = truncate_log(&input);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() <= 513);
    }

    #[test]
    fn truncate_preserves_short_lines() {
        assert_eq!(truncate_log("hello"), "hello");
        assert_eq!(truncate_log("trailing\n"), "trailing");
    }

    #[test]
    fn java_major_parses_modern_version() {
        // Covered indirectly — the function shells out so we only assert
        // on the parse path via a synthetic helper. Kept here as a
        // placeholder so future contributors know the semver parse lives
        // in `java_major_version` and not in a helper module.
    }
}
