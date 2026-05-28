use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        global_computer_use::ensure_global_computer_use_session_record, validate_non_empty,
        CommandError, CommandResult,
    },
    db::project_app_data_dir_for_repo,
    runtime::{
        AutonomousDesktopCapabilities, AutonomousDesktopControlStatusSnapshot,
        AutonomousDesktopControllerLock, AutonomousDesktopPermissionGrant,
        AutonomousDesktopPermissionStatus, AutonomousDesktopRedactionMode,
        AutonomousDesktopRedactionRequest, AutonomousDesktopRegion, AutonomousDesktopSidecarStatus,
        AutonomousDesktopStreamState, AutonomousToolRuntime,
    },
    state::DesktopState,
};

const DESKTOP_CONTROL_DIR: &str = "desktop-control";
const DESKTOP_CONTROL_SETTINGS_FILE: &str = "settings.json";
const DESKTOP_CONTROL_AUDIT_FILE: &str = "desktop-control/audit.jsonl";
const MAX_PRIVATE_REGIONS: usize = 16;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopControlSettingsDto {
    pub cloud_streaming_enabled: bool,
    pub manual_cloud_control_enabled: bool,
    pub redaction_mode: AutonomousDesktopRedactionMode,
    pub private_regions: Vec<AutonomousDesktopRegion>,
    pub updated_at: Option<String>,
}

impl Default for DesktopControlSettingsDto {
    fn default() -> Self {
        Self {
            cloud_streaming_enabled: false,
            manual_cloud_control_enabled: false,
            redaction_mode: AutonomousDesktopRedactionMode::Balanced,
            private_regions: Vec::new(),
            updated_at: None,
        }
    }
}

impl DesktopControlSettingsDto {
    pub(crate) fn redaction_request(&self) -> Option<AutonomousDesktopRedactionRequest> {
        if self.private_regions.is_empty()
            && self.redaction_mode == AutonomousDesktopRedactionMode::Off
        {
            return None;
        }
        Some(AutonomousDesktopRedactionRequest {
            mode: self.redaction_mode.clone(),
            private_regions: self.private_regions.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertDesktopControlSettingsRequestDto {
    pub cloud_streaming_enabled: bool,
    pub manual_cloud_control_enabled: bool,
    pub redaction_mode: AutonomousDesktopRedactionMode,
    pub private_regions: Vec<AutonomousDesktopRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopControlStatusRequestDto {
    #[serde(default)]
    pub refresh_permission_status: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopControlPermissionActionKindDto {
    OpenMacosPrivacyPane,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopControlPermissionActionDto {
    pub kind: DesktopControlPermissionActionKindDto,
    pub target: String,
    pub label: String,
    pub post_action_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopControlPermissionStatusDto {
    pub name: String,
    pub status: AutonomousDesktopPermissionGrant,
    pub required_for: Vec<String>,
    pub remediation: String,
    pub action: Option<DesktopControlPermissionActionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopControlOpenPermissionSettingsRequestDto {
    pub kind: DesktopControlPermissionActionKindDto,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopControlStatusDto {
    pub schema: String,
    pub platform: String,
    pub sidecar: AutonomousDesktopSidecarStatus,
    pub capabilities: AutonomousDesktopCapabilities,
    pub permissions: Vec<DesktopControlPermissionStatusDto>,
    pub controller_lock: Option<AutonomousDesktopControllerLock>,
    pub stream: AutonomousDesktopStreamState,
    pub settings: DesktopControlSettingsDto,
    pub audit_log_path: String,
    pub updated_at: String,
}

#[tauri::command]
pub fn desktop_control_status<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: Option<DesktopControlStatusRequestDto>,
) -> CommandResult<DesktopControlStatusDto> {
    let runtime = global_computer_use_desktop_runtime(&app, state.inner(), "status")?;
    let snapshot = runtime
        .desktop_control_status_snapshot(request.unwrap_or_default().refresh_permission_status)?;
    let settings = load_desktop_control_settings(&app, state.inner())?;
    let audit_log_path = global_computer_use_audit_log_path(&app, state.inner())?;
    Ok(desktop_status_dto(snapshot, settings, audit_log_path))
}

#[tauri::command]
pub fn desktop_control_update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertDesktopControlSettingsRequestDto,
) -> CommandResult<DesktopControlStatusDto> {
    let settings = DesktopControlSettingsDto {
        cloud_streaming_enabled: request.cloud_streaming_enabled,
        manual_cloud_control_enabled: request.manual_cloud_control_enabled,
        redaction_mode: request.redaction_mode,
        private_regions: normalize_private_regions(request.private_regions)?,
        updated_at: Some(crate::auth::now_timestamp()),
    };
    write_desktop_control_settings(&app, state.inner(), &settings)?;
    let runtime = global_computer_use_desktop_runtime(&app, state.inner(), "settings")?;
    let snapshot = runtime.desktop_control_status_snapshot(false)?;
    let audit_log_path = global_computer_use_audit_log_path(&app, state.inner())?;
    Ok(desktop_status_dto(snapshot, settings, audit_log_path))
}

#[tauri::command]
pub fn desktop_control_stop<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<DesktopControlStatusDto> {
    let runtime = global_computer_use_desktop_runtime(&app, state.inner(), "emergency-stop")?;
    let snapshot = runtime.desktop_emergency_stop("local_desktop_control_stop")?;
    let settings = load_desktop_control_settings(&app, state.inner())?;
    let audit_log_path = global_computer_use_audit_log_path(&app, state.inner())?;
    Ok(desktop_status_dto(snapshot, settings, audit_log_path))
}

#[tauri::command]
pub fn desktop_control_open_permission_settings(
    request: DesktopControlOpenPermissionSettingsRequestDto,
) -> CommandResult<()> {
    let url = desktop_control_permission_settings_url(&request)?;

    if !cfg!(target_os = "macos") {
        return Err(CommandError::user_fixable(
            "desktop_control_permission_settings_unsupported",
            "Xero can only open this desktop permission pane on macOS.",
        ));
    }

    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|error| {
        CommandError::system_fault(
            "desktop_control_permission_settings_open_failed",
            format!("Xero could not open desktop permission settings: {error}"),
        )
    })
}

pub(crate) fn load_desktop_control_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<DesktopControlSettingsDto> {
    let path = desktop_control_settings_path(app, state)?;
    if !path.exists() {
        return Ok(DesktopControlSettingsDto::default());
    }
    let bytes = fs::read(&path).map_err(|error| {
        CommandError::system_fault(
            "desktop_control_settings_read_failed",
            format!("Xero could not read desktop-control settings: {error}"),
        )
    })?;
    match serde_json::from_slice::<DesktopControlSettingsDto>(&bytes) {
        Ok(settings) => Ok(settings),
        Err(_) => {
            let _ = fs::remove_file(&path);
            Ok(DesktopControlSettingsDto::default())
        }
    }
}

pub(crate) fn global_computer_use_desktop_runtime<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    reason: &str,
) -> CommandResult<AutonomousToolRuntime> {
    validate_non_empty(reason, "reason")?;
    let global = ensure_global_computer_use_session_record(app, state)?;
    AutonomousToolRuntime::new(&global.repo_root).map(|runtime| {
        runtime.with_agent_run_context(
            global.project_id,
            global.session.agent_session_id,
            format!("desktop-control-{reason}"),
        )
    })
}

fn write_desktop_control_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    settings: &DesktopControlSettingsDto,
) -> CommandResult<()> {
    let path = desktop_control_settings_path(app, state)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::system_fault(
                "desktop_control_settings_dir_failed",
                format!("Xero could not create desktop-control settings storage: {error}"),
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(settings).map_err(|error| {
        CommandError::system_fault(
            "desktop_control_settings_encode_failed",
            format!("Xero could not encode desktop-control settings: {error}"),
        )
    })?;
    fs::write(path, bytes).map_err(|error| {
        CommandError::system_fault(
            "desktop_control_settings_write_failed",
            format!("Xero could not write desktop-control settings: {error}"),
        )
    })
}

fn normalize_private_regions(
    regions: Vec<AutonomousDesktopRegion>,
) -> CommandResult<Vec<AutonomousDesktopRegion>> {
    if regions.len() > MAX_PRIVATE_REGIONS {
        return Err(CommandError::invalid_request("privateRegions"));
    }
    for region in &regions {
        if region.width == 0 || region.height == 0 {
            return Err(CommandError::invalid_request("privateRegions"));
        }
    }
    Ok(regions)
}

fn desktop_control_settings_path<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<PathBuf> {
    Ok(state
        .app_data_dir(app)?
        .join(DESKTOP_CONTROL_DIR)
        .join(DESKTOP_CONTROL_SETTINGS_FILE))
}

fn global_computer_use_audit_log_path<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<PathBuf> {
    let global = ensure_global_computer_use_session_record(app, state)?;
    Ok(project_app_data_dir_for_repo(&global.repo_root).join(DESKTOP_CONTROL_AUDIT_FILE))
}

fn desktop_status_dto(
    snapshot: AutonomousDesktopControlStatusSnapshot,
    settings: DesktopControlSettingsDto,
    audit_log_path: PathBuf,
) -> DesktopControlStatusDto {
    let platform = snapshot.platform;
    let permissions = desktop_permission_status_dtos(&platform, snapshot.permissions);
    DesktopControlStatusDto {
        schema: snapshot.schema,
        platform,
        sidecar: snapshot.sidecar,
        capabilities: snapshot.capabilities,
        permissions,
        controller_lock: snapshot.controller_lock,
        stream: snapshot.stream,
        settings,
        audit_log_path: audit_log_path.to_string_lossy().into_owned(),
        updated_at: snapshot.updated_at,
    }
}

fn desktop_permission_status_dtos(
    platform: &str,
    permissions: Vec<AutonomousDesktopPermissionStatus>,
) -> Vec<DesktopControlPermissionStatusDto> {
    permissions
        .into_iter()
        .map(|permission| {
            let action = desktop_permission_action(platform, &permission);
            DesktopControlPermissionStatusDto {
                name: permission.name,
                status: permission.status,
                required_for: permission.required_for,
                remediation: permission.remediation,
                action,
            }
        })
        .collect()
}

fn desktop_permission_action(
    platform: &str,
    permission: &AutonomousDesktopPermissionStatus,
) -> Option<DesktopControlPermissionActionDto> {
    if platform != "macos" || permission.status == AutonomousDesktopPermissionGrant::Unsupported {
        return None;
    }

    let (target, label, post_action_hint) = match permission.name.as_str() {
        "Screen Recording" => (
            "Privacy_ScreenCapture",
            "Open Screen Recording",
            "After changing Screen Recording, macOS may ask you to quit and reopen Xero. Return here and refresh status after Xero is running again.",
        ),
        "Accessibility" => (
            "Privacy_Accessibility",
            "Open Accessibility",
            "After changing Accessibility, return here and refresh status. If macOS keeps reporting denied, quit and reopen Xero.",
        ),
        "Input Monitoring" => (
            "Privacy_ListenEvent",
            "Open Input Monitoring",
            "After changing Input Monitoring, return here and refresh status. Some keyboard backends require quitting and reopening Xero.",
        ),
        _ => return None,
    };

    Some(DesktopControlPermissionActionDto {
        kind: DesktopControlPermissionActionKindDto::OpenMacosPrivacyPane,
        target: target.into(),
        label: label.into(),
        post_action_hint: post_action_hint.into(),
    })
}

fn desktop_control_permission_settings_url(
    request: &DesktopControlOpenPermissionSettingsRequestDto,
) -> CommandResult<String> {
    validate_non_empty(&request.target, "target")?;
    match request.kind {
        DesktopControlPermissionActionKindDto::OpenMacosPrivacyPane => {
            if !allowed_macos_privacy_pane(&request.target) {
                return Err(CommandError::user_fixable(
                    "desktop_control_permission_settings_target_invalid",
                    "Xero refused to open an unknown desktop permission pane.",
                ));
            }
            Ok(format!(
                "x-apple.systempreferences:com.apple.preference.security?{}",
                request.target
            ))
        }
    }
}

fn allowed_macos_privacy_pane(target: &str) -> bool {
    matches!(
        target,
        "Privacy_ScreenCapture" | "Privacy_Accessibility" | "Privacy_ListenEvent"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_keep_cloud_control_off() {
        let settings = DesktopControlSettingsDto::default();
        assert!(!settings.cloud_streaming_enabled);
        assert!(!settings.manual_cloud_control_enabled);
        assert_eq!(
            settings.redaction_mode,
            AutonomousDesktopRedactionMode::Balanced
        );
        assert!(settings.private_regions.is_empty());
    }

    #[test]
    fn settings_reject_too_many_private_regions() {
        let regions = (0..=MAX_PRIVATE_REGIONS)
            .map(|x| AutonomousDesktopRegion {
                x: x as u32,
                y: 0,
                width: 10,
                height: 10,
            })
            .collect();

        let error = normalize_private_regions(regions).expect_err("too many regions");

        assert_eq!(error.code, "invalid_request");
    }

    #[test]
    fn redaction_request_preserves_user_marked_private_regions_when_auto_off() {
        let settings = DesktopControlSettingsDto {
            redaction_mode: AutonomousDesktopRedactionMode::Off,
            private_regions: vec![AutonomousDesktopRegion {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            }],
            ..DesktopControlSettingsDto::default()
        };

        let redaction = settings.redaction_request().expect("redaction request");

        assert_eq!(redaction.mode, AutonomousDesktopRedactionMode::Off);
        assert_eq!(redaction.private_regions.len(), 1);
    }

    #[test]
    fn macos_permission_status_includes_vetted_settings_actions() {
        let rows = desktop_permission_status_dtos(
            "macos",
            vec![AutonomousDesktopPermissionStatus {
                name: "Screen Recording".into(),
                status: AutonomousDesktopPermissionGrant::Denied,
                required_for: vec!["screenshot".into(), "stream".into()],
                remediation: "Grant screen capture permission.".into(),
            }],
        );

        let action = rows[0].action.as_ref().expect("permission action");

        assert_eq!(
            action.kind,
            DesktopControlPermissionActionKindDto::OpenMacosPrivacyPane
        );
        assert_eq!(action.target, "Privacy_ScreenCapture");
        assert_eq!(action.label, "Open Screen Recording");
        assert!(action.post_action_hint.contains("refresh status"));
    }

    #[test]
    fn non_macos_permission_status_omits_macos_settings_actions() {
        let rows = desktop_permission_status_dtos(
            "linux",
            vec![AutonomousDesktopPermissionStatus {
                name: "Screen Recording".into(),
                status: AutonomousDesktopPermissionGrant::Unknown,
                required_for: vec!["stream".into()],
                remediation: "Approve the local portal prompt.".into(),
            }],
        );

        assert!(rows[0].action.is_none());
    }

    #[test]
    fn permission_settings_url_allows_only_known_macos_privacy_panes() {
        let request = DesktopControlOpenPermissionSettingsRequestDto {
            kind: DesktopControlPermissionActionKindDto::OpenMacosPrivacyPane,
            target: "Privacy_Accessibility".into(),
        };

        let url = desktop_control_permission_settings_url(&request).expect("settings url");

        assert_eq!(
            url,
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        );

        let rejected = desktop_control_permission_settings_url(
            &DesktopControlOpenPermissionSettingsRequestDto {
                kind: DesktopControlPermissionActionKindDto::OpenMacosPrivacyPane,
                target: "Privacy_AllFiles".into(),
            },
        )
        .expect_err("unknown pane rejected");

        assert_eq!(
            rejected.code,
            "desktop_control_permission_settings_target_invalid"
        );
    }
}
