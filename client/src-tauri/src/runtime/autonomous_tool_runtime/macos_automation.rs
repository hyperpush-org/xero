use std::{
    fs,
    io::Cursor,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use image::{ImageFormat, RgbaImage};

use super::{
    AutonomousMacosApp, AutonomousMacosAutomationAction, AutonomousMacosAutomationOutput,
    AutonomousMacosAutomationPolicyTrace, AutonomousMacosAutomationRequest,
    AutonomousMacosPermission, AutonomousMacosPermissionStatus, AutonomousMacosScreenshot,
    AutonomousMacosScreenshotTarget, AutonomousMacosWindow, AutonomousMacosWindowBounds,
    AutonomousProcessActionRiskLevel, AutonomousToolOutput, AutonomousToolResult,
    AutonomousToolRuntime, AUTONOMOUS_TOOL_MACOS_AUTOMATION,
};
use crate::commands::{validate_non_empty, CommandError, CommandResult};

const MACOS_AUTOMATION_PHASE: &str = "phase_7_macos_app_system_automation";
const SCREENSHOT_ARTIFACT_DIR: &str = "cadence-macos-screenshots";

impl AutonomousToolRuntime {
    pub fn macos_automation(
        &self,
        request: AutonomousMacosAutomationRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.macos_automation_with_approval(request, false)
    }

    pub fn macos_automation_with_operator_approval(
        &self,
        request: AutonomousMacosAutomationRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.macos_automation_with_approval(request, true)
    }

    fn macos_automation_with_approval(
        &self,
        request: AutonomousMacosAutomationRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        validate_macos_automation_request(&request)?;
        let policy = macos_automation_policy_trace(request.action);
        if policy.approval_required && !operator_approved {
            let message = format!(
                "Cadence paused `{}` because {}",
                macos_action_label(request.action),
                policy.reason
            );
            return Ok(macos_automation_result(AutonomousMacosAutomationOutput {
                action: request.action,
                phase: MACOS_AUTOMATION_PHASE.into(),
                platform_supported: cfg!(target_os = "macos"),
                performed: false,
                apps: Vec::new(),
                windows: Vec::new(),
                permissions: Vec::new(),
                screenshot: None,
                policy,
                message,
            }));
        }

        run_macos_automation(request, policy)
    }
}

fn macos_automation_result(output: AutonomousMacosAutomationOutput) -> AutonomousToolResult {
    let summary = output.message.clone();
    AutonomousToolResult {
        tool_name: AUTONOMOUS_TOOL_MACOS_AUTOMATION.into(),
        summary,
        command_result: None,
        output: AutonomousToolOutput::MacosAutomation(output),
    }
}

fn validate_macos_automation_request(
    request: &AutonomousMacosAutomationRequest,
) -> CommandResult<()> {
    if let Some(app_name) = request.app_name.as_deref() {
        validate_non_empty(app_name, "appName")?;
    }
    if let Some(bundle_id) = request.bundle_id.as_deref() {
        validate_non_empty(bundle_id, "bundleId")?;
    }

    let has_app_target =
        request.app_name.is_some() || request.bundle_id.is_some() || request.pid.is_some();
    match request.action {
        AutonomousMacosAutomationAction::MacPermissions
        | AutonomousMacosAutomationAction::MacAppList
        | AutonomousMacosAutomationAction::MacWindowList
        | AutonomousMacosAutomationAction::MacScreenshot => {}
        AutonomousMacosAutomationAction::MacAppLaunch => {
            if request.app_name.is_none() && request.bundle_id.is_none() {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_macos_app_target_required",
                    "Cadence needs `appName` or `bundleId` to launch a macOS app.",
                ));
            }
        }
        AutonomousMacosAutomationAction::MacAppActivate
        | AutonomousMacosAutomationAction::MacAppQuit => {
            if !has_app_target {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_macos_app_target_required",
                    "Cadence needs `appName`, `bundleId`, or `pid` to target a macOS app.",
                ));
            }
        }
        AutonomousMacosAutomationAction::MacWindowFocus => {
            if request.window_id.is_none() && !has_app_target {
                return Err(CommandError::user_fixable(
                    "autonomous_tool_macos_window_target_required",
                    "Cadence needs `windowId`, `appName`, `bundleId`, or `pid` to focus a macOS window.",
                ));
            }
        }
    }

    Ok(())
}

fn macos_automation_policy_trace(
    action: AutonomousMacosAutomationAction,
) -> AutonomousMacosAutomationPolicyTrace {
    let risk_level = match action {
        AutonomousMacosAutomationAction::MacPermissions
        | AutonomousMacosAutomationAction::MacAppList
        | AutonomousMacosAutomationAction::MacWindowList => {
            AutonomousProcessActionRiskLevel::Observe
        }
        AutonomousMacosAutomationAction::MacScreenshot => {
            AutonomousProcessActionRiskLevel::SystemRead
        }
        AutonomousMacosAutomationAction::MacAppLaunch
        | AutonomousMacosAutomationAction::MacAppActivate
        | AutonomousMacosAutomationAction::MacAppQuit
        | AutonomousMacosAutomationAction::MacWindowFocus => {
            AutonomousProcessActionRiskLevel::OsAutomation
        }
    };

    let (approval_required, code, reason) = match risk_level {
        AutonomousProcessActionRiskLevel::Observe => (
            false,
            "macos_policy_observe",
            "macOS permission, app, and window observation is read-only.",
        ),
        AutonomousProcessActionRiskLevel::SystemRead => (
            true,
            "macos_policy_screenshot_requires_approval",
            "Screen or window capture may expose private desktop contents and requires explicit operator approval.",
        ),
        AutonomousProcessActionRiskLevel::OsAutomation => (
            true,
            "macos_policy_os_automation_requires_approval",
            "Launching, activating, quitting, or focusing external apps requires explicit operator approval.",
        ),
        _ => (
            true,
            "macos_policy_automation_requires_approval",
            "macOS automation requires explicit operator approval.",
        ),
    };

    AutonomousMacosAutomationPolicyTrace {
        risk_level,
        approval_required,
        code: code.into(),
        reason: reason.into(),
    }
}

fn run_macos_automation(
    request: AutonomousMacosAutomationRequest,
    policy: AutonomousMacosAutomationPolicyTrace,
) -> CommandResult<AutonomousToolResult> {
    #[cfg(target_os = "macos")]
    {
        macos::run(request, policy)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let message = "Cadence macOS automation is only available on macOS hosts.".to_string();
        Ok(macos_automation_result(AutonomousMacosAutomationOutput {
            action: request.action,
            phase: MACOS_AUTOMATION_PHASE.into(),
            platform_supported: false,
            performed: false,
            apps: Vec::new(),
            windows: Vec::new(),
            permissions: vec![AutonomousMacosPermission {
                name: "macOS".into(),
                status: AutonomousMacosPermissionStatus::Unsupported,
                required_for: Vec::new(),
                detail: message.clone(),
            }],
            screenshot: None,
            policy,
            message,
        }))
    }
}

fn macos_action_label(action: AutonomousMacosAutomationAction) -> &'static str {
    match action {
        AutonomousMacosAutomationAction::MacPermissions => "mac_permissions",
        AutonomousMacosAutomationAction::MacAppList => "mac_app_list",
        AutonomousMacosAutomationAction::MacAppLaunch => "mac_app_launch",
        AutonomousMacosAutomationAction::MacAppActivate => "mac_app_activate",
        AutonomousMacosAutomationAction::MacAppQuit => "mac_app_quit",
        AutonomousMacosAutomationAction::MacWindowList => "mac_window_list",
        AutonomousMacosAutomationAction::MacWindowFocus => "mac_window_focus",
        AutonomousMacosAutomationAction::MacScreenshot => "mac_screenshot",
    }
}

fn screenshot_artifact_path(prefix: &str) -> CommandResult<PathBuf> {
    let root = std::env::temp_dir().join(SCREENSHOT_ARTIFACT_DIR);
    fs::create_dir_all(&root).map_err(|error| {
        CommandError::system_fault(
            "autonomous_tool_macos_screenshot_artifact_failed",
            format!("Cadence could not create the macOS screenshot artifact directory: {error}"),
        )
    })?;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_macos_screenshot_artifact_failed",
                format!("Cadence could not timestamp the macOS screenshot artifact: {error}"),
            )
        })?
        .as_millis();
    Ok(root.join(format!("{prefix}-{millis}.png")))
}

fn write_screenshot_artifact(
    prefix: &str,
    image: RgbaImage,
    target: AutonomousMacosScreenshotTarget,
    window_id: Option<u32>,
    monitor_id: Option<u32>,
) -> CommandResult<AutonomousMacosScreenshot> {
    let width = image.width();
    let height = image.height();
    let capacity = u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(4)
        .min(usize::MAX as u64) as usize;
    let mut buffer = Cursor::new(Vec::with_capacity(capacity));
    image
        .write_to(&mut buffer, ImageFormat::Png)
        .map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_macos_screenshot_encode_failed",
                format!("Cadence could not encode the macOS screenshot as PNG: {error}"),
            )
        })?;
    let bytes = buffer.into_inner();
    let path = screenshot_artifact_path(prefix)?;
    fs::write(&path, &bytes).map_err(|error| {
        CommandError::system_fault(
            "autonomous_tool_macos_screenshot_artifact_failed",
            format!("Cadence could not write the macOS screenshot artifact: {error}"),
        )
    })?;

    Ok(AutonomousMacosScreenshot {
        path: path.to_string_lossy().into_owned(),
        target,
        width,
        height,
        byte_count: bytes.len(),
        window_id,
        monitor_id,
    })
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use objc2::rc::{autoreleasepool, Retained};
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

    pub(super) fn run(
        request: AutonomousMacosAutomationRequest,
        policy: AutonomousMacosAutomationPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        let mut output = AutonomousMacosAutomationOutput {
            action: request.action,
            phase: MACOS_AUTOMATION_PHASE.into(),
            platform_supported: true,
            performed: false,
            apps: Vec::new(),
            windows: Vec::new(),
            permissions: Vec::new(),
            screenshot: None,
            policy,
            message: String::new(),
        };

        match request.action {
            AutonomousMacosAutomationAction::MacPermissions => {
                output.permissions = permissions();
                output.performed = true;
                output.message = "Checked macOS automation permissions.".into();
            }
            AutonomousMacosAutomationAction::MacAppList => {
                output.apps = list_apps(&request)?;
                output.performed = true;
                output.message = format!("Listed {} running macOS app(s).", output.apps.len());
            }
            AutonomousMacosAutomationAction::MacAppLaunch => {
                launch_app(&request)?;
                output.apps = list_apps(&request).unwrap_or_default();
                output.performed = true;
                output.message = "Requested macOS app launch.".into();
            }
            AutonomousMacosAutomationAction::MacAppActivate => {
                let app = find_target_app(&request)?;
                let target = app_summary(&app);
                let activated =
                    app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
                output.apps = vec![target];
                output.performed = activated;
                output.message = if activated {
                    "Requested macOS app activation.".into()
                } else {
                    "macOS refused to activate the target app.".into()
                };
            }
            AutonomousMacosAutomationAction::MacAppQuit => {
                let app = find_target_app(&request)?;
                let target = app_summary(&app);
                let terminated = app.terminate();
                output.apps = vec![target];
                output.performed = terminated;
                output.message = if terminated {
                    "Requested graceful macOS app quit.".into()
                } else {
                    "macOS refused to quit the target app.".into()
                };
            }
            AutonomousMacosAutomationAction::MacWindowList => {
                output.windows = list_windows(&request)?;
                output.performed = true;
                output.message = format!("Listed {} macOS window(s).", output.windows.len());
            }
            AutonomousMacosAutomationAction::MacWindowFocus => {
                let window = find_target_window(&request)?;
                let mut app_request = request.clone();
                app_request.pid = window.pid.or(app_request.pid);
                if app_request.pid.is_none()
                    && app_request.bundle_id.is_none()
                    && app_request.app_name.is_none()
                {
                    app_request.app_name = Some(window.app_name.clone());
                }
                let app = find_target_app(&app_request)?;
                let activated =
                    app.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
                output.windows = vec![window];
                output.apps = vec![app_summary(&app)];
                output.performed = activated;
                output.message = if activated {
                    "Requested focus for the target macOS window's app.".into()
                } else {
                    "macOS refused to focus the target window's app.".into()
                };
            }
            AutonomousMacosAutomationAction::MacScreenshot => {
                let screenshot = capture_screenshot(&request)?;
                output.performed = true;
                output.message = format!("Captured macOS screenshot to `{}`.", screenshot.path);
                output.screenshot = Some(screenshot);
            }
        }

        Ok(macos_automation_result(output))
    }

    fn permissions() -> Vec<AutonomousMacosPermission> {
        vec![
            AutonomousMacosPermission {
                name: "Accessibility".into(),
                status: if accessibility_permission_granted() {
                    AutonomousMacosPermissionStatus::Granted
                } else {
                    AutonomousMacosPermissionStatus::Denied
                },
                required_for: vec![
                    AutonomousMacosAutomationAction::MacAppActivate,
                    AutonomousMacosAutomationAction::MacAppQuit,
                    AutonomousMacosAutomationAction::MacWindowFocus,
                ],
                detail: "Required when macOS needs Cadence to drive or focus external app UI.".into(),
            },
            AutonomousMacosPermission {
                name: "Screen Recording".into(),
                status: if screen_recording_permission_granted() {
                    AutonomousMacosPermissionStatus::Granted
                } else {
                    AutonomousMacosPermissionStatus::Denied
                },
                required_for: vec![AutonomousMacosAutomationAction::MacScreenshot],
                detail: "Required for screen and window screenshots.".into(),
            },
            AutonomousMacosPermission {
                name: "Automation".into(),
                status: AutonomousMacosPermissionStatus::Unknown,
                required_for: vec![
                    AutonomousMacosAutomationAction::MacAppLaunch,
                    AutonomousMacosAutomationAction::MacAppActivate,
                    AutonomousMacosAutomationAction::MacAppQuit,
                ],
                detail: "macOS grants Automation per target app, so Cadence reports it as unknown until an operation is attempted.".into(),
            },
        ]
    }

    fn list_apps(
        request: &AutonomousMacosAutomationRequest,
    ) -> CommandResult<Vec<AutonomousMacosApp>> {
        Ok(autoreleasepool(|_| {
            let workspace = NSWorkspace::sharedWorkspace();
            workspace
                .runningApplications()
                .iter()
                .map(|app| app_summary(&app))
                .filter(|app| app_matches(app, request))
                .collect()
        }))
    }

    fn app_summary(app: &NSRunningApplication) -> AutonomousMacosApp {
        AutonomousMacosApp {
            name: app
                .localizedName()
                .map(|name| name.to_string())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "Unknown".into()),
            bundle_id: app
                .bundleIdentifier()
                .map(|bundle_id| bundle_id.to_string()),
            pid: normalize_pid(app.processIdentifier()),
            active: app.isActive(),
            hidden: app.isHidden(),
            terminated: app.isTerminated(),
            bundle_path: app
                .bundleURL()
                .and_then(|url| url.to_file_path())
                .map(|path| path.to_string_lossy().into_owned()),
            executable_path: app
                .executableURL()
                .and_then(|url| url.to_file_path())
                .map(|path| path.to_string_lossy().into_owned()),
        }
    }

    fn app_matches(app: &AutonomousMacosApp, request: &AutonomousMacosAutomationRequest) -> bool {
        if let Some(pid) = request.pid {
            if app.pid != Some(pid) {
                return false;
            }
        }
        if let Some(bundle_id) = request.bundle_id.as_deref() {
            if app.bundle_id.as_deref() != Some(bundle_id) {
                return false;
            }
        }
        if let Some(app_name) = request.app_name.as_deref() {
            if !app.name.eq_ignore_ascii_case(app_name) {
                return false;
            }
        }
        true
    }

    fn find_target_app(
        request: &AutonomousMacosAutomationRequest,
    ) -> CommandResult<Retained<NSRunningApplication>> {
        autoreleasepool(|_| {
            if let Some(pid) = request.pid {
                if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(
                    pid as libc::pid_t,
                ) {
                    return Ok(app);
                }
            }

            let workspace = NSWorkspace::sharedWorkspace();
            for app in workspace.runningApplications().iter() {
                let summary = app_summary(&app);
                if app_matches(&summary, request) {
                    return Ok(app);
                }
            }

            Err(CommandError::user_fixable(
                "autonomous_tool_macos_app_not_found",
                "Cadence could not find a running macOS app matching the requested target.",
            ))
        })
    }

    fn launch_app(request: &AutonomousMacosAutomationRequest) -> CommandResult<()> {
        let mut command = Command::new("open");
        if let Some(bundle_id) = request.bundle_id.as_deref() {
            command.arg("-b").arg(bundle_id);
        } else if let Some(app_name) = request.app_name.as_deref() {
            command.arg("-a").arg(app_name);
        }

        let output = command.output().map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_macos_app_launch_failed",
                format!("Cadence could not invoke macOS app launch: {error}"),
            )
        })?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(CommandError::user_fixable(
            "autonomous_tool_macos_app_launch_failed",
            format!(
                "macOS refused to launch the requested app: {}",
                stderr.trim()
            ),
        ))
    }

    fn list_windows(
        request: &AutonomousMacosAutomationRequest,
    ) -> CommandResult<Vec<AutonomousMacosWindow>> {
        let app_filter = if request.bundle_id.is_some() {
            Some(
                list_apps(request)?
                    .into_iter()
                    .filter_map(|app| app.pid)
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };

        let windows = xcap::Window::all().map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_macos_window_list_failed",
                format!("Cadence could not list macOS windows: {error}"),
            )
        })?;

        let mut output = Vec::new();
        for window in windows {
            let summary = window_summary(&window)?;
            if let Some(window_id) = request.window_id {
                if summary.window_id != window_id {
                    continue;
                }
            }
            if let Some(pid) = request.pid {
                if summary.pid != Some(pid) {
                    continue;
                }
            }
            if let Some(app_filter) = &app_filter {
                if summary.pid.is_none_or(|pid| !app_filter.contains(&pid)) {
                    continue;
                }
            }
            if let Some(app_name) = request.app_name.as_deref() {
                if !summary.app_name.eq_ignore_ascii_case(app_name) {
                    continue;
                }
            }
            output.push(summary);
        }

        Ok(output)
    }

    fn find_target_window(
        request: &AutonomousMacosAutomationRequest,
    ) -> CommandResult<AutonomousMacosWindow> {
        list_windows(request)?.into_iter().next().ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_macos_window_not_found",
                "Cadence could not find a macOS window matching the requested target.",
            )
        })
    }

    fn window_summary(window: &xcap::Window) -> CommandResult<AutonomousMacosWindow> {
        let window_id = window.id().map_err(map_window_error("read window id"))?;
        let pid = window.pid().ok();
        let app_name = window
            .app_name()
            .map_err(map_window_error("read window app name"))?;
        let title = window.title().unwrap_or_default();
        let x = window.x().map_err(map_window_error("read window x"))?;
        let y = window.y().map_err(map_window_error("read window y"))?;
        let width = window
            .width()
            .map_err(map_window_error("read window width"))?;
        let height = window
            .height()
            .map_err(map_window_error("read window height"))?;
        Ok(AutonomousMacosWindow {
            window_id,
            pid,
            app_name,
            title,
            active: window.is_focused().unwrap_or(false),
            minimized: window.is_minimized().unwrap_or(false),
            bounds: AutonomousMacosWindowBounds {
                x,
                y,
                width,
                height,
            },
        })
    }

    fn capture_screenshot(
        request: &AutonomousMacosAutomationRequest,
    ) -> CommandResult<AutonomousMacosScreenshot> {
        let target = request.screenshot_target.unwrap_or(
            if request.window_id.is_some()
                || request.pid.is_some()
                || request.app_name.is_some()
                || request.bundle_id.is_some()
            {
                AutonomousMacosScreenshotTarget::Window
            } else {
                AutonomousMacosScreenshotTarget::Screen
            },
        );

        match target {
            AutonomousMacosScreenshotTarget::Window => {
                let target_window = find_target_window(request)?;
                let windows = xcap::Window::all().map_err(|error| {
                    CommandError::system_fault(
                        "autonomous_tool_macos_screenshot_failed",
                        format!("Cadence could not enumerate windows for screenshot: {error}"),
                    )
                })?;
                for window in windows {
                    let summary = window_summary(&window)?;
                    if summary.window_id != target_window.window_id {
                        continue;
                    }
                    let image = window.capture_image().map_err(|error| {
                        CommandError::system_fault(
                            "autonomous_tool_macos_screenshot_failed",
                            format!("Cadence could not capture the target window: {error}"),
                        )
                    })?;
                    return write_screenshot_artifact(
                        "window",
                        image,
                        AutonomousMacosScreenshotTarget::Window,
                        Some(summary.window_id),
                        None,
                    );
                }
                Err(CommandError::user_fixable(
                    "autonomous_tool_macos_window_not_found",
                    "Cadence could not find a macOS window to screenshot.",
                ))
            }
            AutonomousMacosScreenshotTarget::Screen => {
                let monitors = xcap::Monitor::all().map_err(|error| {
                    CommandError::system_fault(
                        "autonomous_tool_macos_screenshot_failed",
                        format!("Cadence could not enumerate displays for screenshot: {error}"),
                    )
                })?;
                let monitor = if let Some(monitor_id) = request.monitor_id {
                    monitors
                        .into_iter()
                        .find(|monitor| monitor.id().ok() == Some(monitor_id))
                        .ok_or_else(|| {
                            CommandError::user_fixable(
                                "autonomous_tool_macos_monitor_not_found",
                                format!("Cadence could not find macOS monitor `{monitor_id}`."),
                            )
                        })?
                } else {
                    monitors
                        .into_iter()
                        .find(|monitor| monitor.is_primary().unwrap_or(false))
                        .or_else(|| {
                            xcap::Monitor::all()
                                .ok()
                                .and_then(|mut monitors| monitors.pop())
                        })
                        .ok_or_else(|| {
                            CommandError::system_fault(
                                "autonomous_tool_macos_monitor_not_found",
                                "Cadence could not find a macOS monitor to screenshot.",
                            )
                        })?
                };
                let monitor_id = monitor.id().ok();
                let image = monitor.capture_image().map_err(|error| {
                    CommandError::system_fault(
                        "autonomous_tool_macos_screenshot_failed",
                        format!("Cadence could not capture the display contents: {error}"),
                    )
                })?;
                write_screenshot_artifact(
                    "screen",
                    image,
                    AutonomousMacosScreenshotTarget::Screen,
                    None,
                    monitor_id,
                )
            }
        }
    }

    fn normalize_pid(pid: libc::pid_t) -> Option<u32> {
        (pid > 0).then_some(pid as u32)
    }

    fn map_window_error(operation: &'static str) -> impl FnOnce(xcap::XCapError) -> CommandError {
        move |error| {
            CommandError::system_fault(
                "autonomous_tool_macos_window_list_failed",
                format!("Cadence could not {operation}: {error}"),
            )
        }
    }

    fn accessibility_permission_granted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    fn screen_recording_permission_granted() -> bool {
        unsafe { CGPreflightScreenCaptureAccess() }
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }
}
