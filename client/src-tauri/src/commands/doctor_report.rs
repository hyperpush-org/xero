use std::path::{Path, PathBuf};

use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        dictation::{load_dictation_settings, probe_dictation_status},
        provider_credentials::load_provider_credentials_view,
        CommandError, CommandResult, DictationEnginePreferenceDto, DictationModernAssetStatusDto,
        DictationPermissionStateDto, DictationPlatformDto, DictationStatusDto,
        RunDoctorReportRequestDto, RuntimeAuthPhase,
    },
    db::project_store::{self, RuntimeRunStatus, RuntimeRunTransportLiveness},
    environment::service as environment_service,
    global_db::environment_profile::{
        EnvironmentCapabilityState, EnvironmentDiagnostic, EnvironmentDiagnosticSeverity,
        EnvironmentProfileStatus, EnvironmentProfileSummary,
    },
    mcp::{McpConnectionStatus, McpRegistry},
    notifications::{FileNotificationCredentialStore, NotificationRouteKind},
    provider_models::load_provider_model_catalog,
    registry,
    runtime::{
        provider_model_catalog_diagnostic, provider_validation_diagnostics, XeroDiagnosticCheck,
        XeroDiagnosticCheckInput, XeroDiagnosticSeverity, XeroDiagnosticStatus,
        XeroDiagnosticSubject, XeroDoctorReport, XeroDoctorReportInput, XeroDoctorReportMode,
        XeroDoctorVersionInfo,
    },
    state::DesktopState,
};

use super::runtime_support::{load_runtime_run_status, load_runtime_session_status};

#[tauri::command]
pub fn run_doctor_report<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RunDoctorReportRequestDto,
) -> CommandResult<XeroDoctorReport> {
    let mode = request.mode.unwrap_or(XeroDoctorReportMode::QuickLocal);
    let generated_at = now_timestamp();
    let mut checks = DoctorCheckBuckets::default();

    collect_app_path_checks(&app, state.inner(), &mut checks.settings_dependency_checks);
    collect_environment_profile_checks(&app, state.inner(), &mut checks.settings_dependency_checks);
    collect_dictation_checks(&app, state.inner(), &mut checks.dictation_checks);
    collect_provider_checks(&app, state.inner(), mode, &mut checks);
    collect_mcp_checks(&app, state.inner(), &mut checks.mcp_dependency_checks);
    collect_project_runtime_checks(&app, state.inner(), &mut checks);

    XeroDoctorReport::new(XeroDoctorReportInput {
        report_id: doctor_report_id(&generated_at),
        generated_at,
        mode,
        versions: XeroDoctorVersionInfo {
            app_version: env!("CARGO_PKG_VERSION").into(),
            runtime_supervisor_version: None,
            runtime_protocol_version: None,
        },
        dictation_checks: checks.dictation_checks,
        profile_checks: checks.profile_checks,
        model_catalog_checks: checks.model_catalog_checks,
        runtime_supervisor_checks: checks.runtime_supervisor_checks,
        mcp_dependency_checks: checks.mcp_dependency_checks,
        settings_dependency_checks: checks.settings_dependency_checks,
    })
}

#[derive(Default)]
struct DoctorCheckBuckets {
    profile_checks: Vec<XeroDiagnosticCheck>,
    dictation_checks: Vec<XeroDiagnosticCheck>,
    model_catalog_checks: Vec<XeroDiagnosticCheck>,
    runtime_supervisor_checks: Vec<XeroDiagnosticCheck>,
    mcp_dependency_checks: Vec<XeroDiagnosticCheck>,
    settings_dependency_checks: Vec<XeroDiagnosticCheck>,
}

fn collect_dictation_checks<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    checks: &mut Vec<XeroDiagnosticCheck>,
) {
    let status = probe_dictation_status();
    let settings = match load_dictation_settings(app, state) {
        Ok(settings) => Some(settings),
        Err(error) => {
            push_check(
                checks,
                command_error_check(
                    XeroDiagnosticSubject::Dictation,
                    "dictation_settings_unavailable",
                    "Xero could not load dictation settings while generating diagnostics.",
                    error,
                    "Open Dictation settings, resave the preferences, then run diagnostics again.",
                ),
            );
            None
        }
    };

    if status.platform != DictationPlatformDto::Macos {
        push_check(
            checks,
            XeroDiagnosticCheck::skipped(
                XeroDiagnosticSubject::Dictation,
                "dictation_platform_unsupported",
                "Native dictation is only available on macOS in this release.",
                Some("Use Xero on macOS to enable native dictation.".into()),
            ),
        );
        return;
    }

    push_check(
        checks,
        XeroDiagnosticCheck::passed(
            XeroDiagnosticSubject::Dictation,
            "dictation_macos_runtime_detected",
            format!(
                "macOS {} is available for native dictation.",
                status.os_version.as_deref().unwrap_or("unknown")
            ),
        ),
    );

    if status.modern.compiled {
        push_check(
            checks,
            XeroDiagnosticCheck::passed(
                XeroDiagnosticSubject::Dictation,
                "dictation_modern_sdk_compiled",
                "Xero was built with the macOS 26 dictation SDK.",
            ),
        );
    } else if status.legacy.available {
        push_check(
            checks,
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::Dictation,
                status: XeroDiagnosticStatus::Warning,
                severity: XeroDiagnosticSeverity::Warning,
                retryable: false,
                code: "dictation_modern_sdk_unavailable_legacy_available".into(),
                message: "Xero was built without macOS 26 SpeechAnalyzer support, but legacy dictation is available.".into(),
                affected_profile_id: None,
                affected_provider_id: None,
                endpoint: None,
                remediation: Some(
                    "Use Automatic or Legacy only in Dictation settings, or rebuild Xero with the macOS 26 SDK.".into(),
                ),
            }),
        );
    } else {
        push_check(
            checks,
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::Dictation,
                status: XeroDiagnosticStatus::Failed,
                severity: XeroDiagnosticSeverity::Error,
                retryable: false,
                code: "dictation_no_native_engine_available".into(),
                message: "Xero could not find an available native macOS dictation engine.".into(),
                affected_profile_id: None,
                affected_provider_id: None,
                endpoint: None,
                remediation: Some(
                    "Update macOS, enable Apple Speech Recognition support, or rebuild Xero with the macOS 26 SDK.".into(),
                ),
            }),
        );
    }

    push_permission_check(
        checks,
        "microphone",
        status.microphone_permission,
        "Open System Settings > Privacy & Security > Microphone and allow Xero.",
    );
    push_permission_check(
        checks,
        "speech",
        status.speech_permission,
        "Open System Settings > Privacy & Security > Speech Recognition and allow Xero.",
    );

    if let Some(settings) = settings {
        push_selected_engine_check(checks, settings.engine_preference, &status);
        push_selected_locale_check(checks, settings.locale.as_deref(), &status);
    }

    push_modern_asset_check(checks, &status);
}

fn push_selected_engine_check(
    checks: &mut Vec<XeroDiagnosticCheck>,
    preference: DictationEnginePreferenceDto,
    status: &DictationStatusDto,
) {
    let (check_status, severity, retryable, code, message, remediation) = match preference {
        DictationEnginePreferenceDto::Modern if status.modern.available => (
            XeroDiagnosticStatus::Passed,
            XeroDiagnosticSeverity::Info,
            false,
            "dictation_selected_engine_available",
            "The selected modern dictation engine is available.".to_string(),
            None,
        ),
        DictationEnginePreferenceDto::Legacy if status.legacy.available => (
            XeroDiagnosticStatus::Passed,
            XeroDiagnosticSeverity::Info,
            false,
            "dictation_selected_engine_available",
            "The selected legacy dictation engine is available.".to_string(),
            None,
        ),
        DictationEnginePreferenceDto::Automatic if status.modern.available && status.legacy.available => (
            XeroDiagnosticStatus::Passed,
            XeroDiagnosticSeverity::Info,
            false,
            "dictation_automatic_modern_with_legacy_fallback",
            "Automatic dictation can use modern dictation with legacy fallback.".to_string(),
            None,
        ),
        DictationEnginePreferenceDto::Automatic if status.modern.available => (
            XeroDiagnosticStatus::Passed,
            XeroDiagnosticSeverity::Info,
            false,
            "dictation_automatic_modern_available",
            "Automatic dictation can use the modern dictation engine.".to_string(),
            None,
        ),
        DictationEnginePreferenceDto::Automatic if status.legacy.available => (
            XeroDiagnosticStatus::Warning,
            XeroDiagnosticSeverity::Warning,
            false,
            "dictation_modern_unavailable_legacy_available",
            "Automatic dictation will use legacy dictation because modern dictation is unavailable.".to_string(),
            Some(
                "This is usable. Rebuild with the macOS 26 SDK or update macOS to enable the modern engine.".to_string(),
            ),
        ),
        DictationEnginePreferenceDto::Modern => (
            XeroDiagnosticStatus::Failed,
            XeroDiagnosticSeverity::Error,
            false,
            "dictation_selected_modern_unavailable",
            "Dictation is set to prefer the modern engine, but modern dictation is unavailable.".to_string(),
            Some("Choose Automatic or Legacy only in Dictation settings.".to_string()),
        ),
        DictationEnginePreferenceDto::Legacy => (
            XeroDiagnosticStatus::Failed,
            XeroDiagnosticSeverity::Error,
            false,
            "dictation_selected_legacy_unavailable",
            "Dictation is set to Legacy only, but legacy dictation is unavailable.".to_string(),
            Some("Choose Automatic in Dictation settings or update macOS Speech Recognition support.".to_string()),
        ),
        DictationEnginePreferenceDto::Automatic => (
            XeroDiagnosticStatus::Failed,
            XeroDiagnosticSeverity::Error,
            true,
            "dictation_automatic_no_engine_available",
            "Automatic dictation could not find any available native dictation engine.".to_string(),
            Some("Update macOS or check Apple Speech Recognition availability, then run diagnostics again.".to_string()),
        ),
    };

    push_check(
        checks,
        XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::Dictation,
            status: check_status,
            severity,
            retryable,
            code: code.into(),
            message,
            affected_profile_id: None,
            affected_provider_id: None,
            endpoint: None,
            remediation,
        }),
    );
}

fn push_permission_check(
    checks: &mut Vec<XeroDiagnosticCheck>,
    permission: &'static str,
    state: DictationPermissionStateDto,
    denied_remediation: &'static str,
) {
    let (status, severity, retryable, code, message, remediation) = match state {
        DictationPermissionStateDto::Authorized => (
            XeroDiagnosticStatus::Passed,
            XeroDiagnosticSeverity::Info,
            false,
            format!("dictation_{permission}_permission_authorized"),
            format!("Xero has {permission} permission for dictation."),
            None,
        ),
        DictationPermissionStateDto::NotDetermined => (
            XeroDiagnosticStatus::Warning,
            XeroDiagnosticSeverity::Warning,
            true,
            format!("dictation_{permission}_permission_not_determined"),
            format!("Xero has not requested {permission} permission yet."),
            Some("Start dictation once to trigger the macOS permission prompt.".to_string()),
        ),
        DictationPermissionStateDto::Denied | DictationPermissionStateDto::Restricted => (
            XeroDiagnosticStatus::Failed,
            XeroDiagnosticSeverity::Error,
            false,
            format!("dictation_{permission}_permission_denied"),
            format!("Xero cannot use dictation because {permission} permission is {state:?}."),
            Some(denied_remediation.to_string()),
        ),
        DictationPermissionStateDto::Unsupported | DictationPermissionStateDto::Unknown => (
            XeroDiagnosticStatus::Warning,
            XeroDiagnosticSeverity::Warning,
            true,
            format!("dictation_{permission}_permission_unknown"),
            format!("Xero could not determine {permission} permission state."),
            Some(
                "Open System Settings privacy permissions, then run diagnostics again.".to_string(),
            ),
        ),
    };

    push_check(
        checks,
        XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::Dictation,
            status,
            severity,
            retryable,
            code,
            message,
            affected_profile_id: None,
            affected_provider_id: None,
            endpoint: None,
            remediation,
        }),
    );
}

fn push_selected_locale_check(
    checks: &mut Vec<XeroDiagnosticCheck>,
    selected_locale: Option<&str>,
    status: &DictationStatusDto,
) {
    let locale = selected_locale
        .or(status.default_locale.as_deref())
        .map(str::trim)
        .filter(|locale| !locale.is_empty());

    let Some(locale) = locale else {
        push_check(
            checks,
            XeroDiagnosticCheck::skipped(
                XeroDiagnosticSubject::Dictation,
                "dictation_locale_unselected",
                "No dictation locale is selected and macOS did not report a system default locale.",
                Some("Choose a locale in Dictation settings.".into()),
            ),
        );
        return;
    };

    if status.supported_locales.is_empty() {
        push_check(
            checks,
            XeroDiagnosticCheck::skipped(
                XeroDiagnosticSubject::Dictation,
                "dictation_supported_locale_list_unavailable",
                format!("Xero could not verify selected dictation locale `{locale}` because macOS did not report supported locales."),
                Some("Start dictation or run diagnostics again after Speech Recognition becomes available.".into()),
            ),
        );
        return;
    }

    if locale_supported(locale, &status.supported_locales) {
        push_check(
            checks,
            XeroDiagnosticCheck::passed(
                XeroDiagnosticSubject::Dictation,
                "dictation_selected_locale_supported",
                format!("Selected dictation locale `{locale}` is supported."),
            ),
        );
    } else {
        push_check(
            checks,
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::Dictation,
                status: XeroDiagnosticStatus::Failed,
                severity: XeroDiagnosticSeverity::Error,
                retryable: false,
                code: "dictation_selected_locale_unsupported".into(),
                message: format!("Selected dictation locale `{locale}` is not in the backend-supported locale list."),
                affected_profile_id: None,
                affected_provider_id: None,
                endpoint: None,
                remediation: Some("Choose a supported locale in Dictation settings or use System default.".into()),
            }),
        );
    }
}

fn push_modern_asset_check(checks: &mut Vec<XeroDiagnosticCheck>, status: &DictationStatusDto) {
    if !status.modern.available {
        push_check(
            checks,
            XeroDiagnosticCheck::skipped(
                XeroDiagnosticSubject::Dictation,
                "dictation_modern_assets_not_applicable",
                "Modern dictation assets were not checked because the modern engine is unavailable.",
                Some("Use Automatic or Legacy only, or enable modern dictation support before checking assets.".into()),
            ),
        );
        return;
    }

    let locale = status
        .modern_assets
        .locale
        .as_deref()
        .or(status.default_locale.as_deref())
        .unwrap_or("system locale");

    match status.modern_assets.status {
        DictationModernAssetStatusDto::Installed => push_check(
            checks,
            XeroDiagnosticCheck::passed(
                XeroDiagnosticSubject::Dictation,
                "dictation_modern_assets_installed",
                format!("Modern Apple speech assets are installed for `{locale}`."),
            ),
        ),
        DictationModernAssetStatusDto::NotInstalled => push_check(
            checks,
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::Dictation,
                status: XeroDiagnosticStatus::Warning,
                severity: XeroDiagnosticSeverity::Warning,
                retryable: true,
                code: "dictation_modern_assets_not_installed".into(),
                message: format!("Modern Apple speech assets are not installed for `{locale}`."),
                affected_profile_id: None,
                affected_provider_id: None,
                endpoint: None,
                remediation: Some("Start dictation for this locale to let macOS install Apple speech assets.".into()),
            }),
        ),
        DictationModernAssetStatusDto::UnsupportedLocale => push_check(
            checks,
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::Dictation,
                status: XeroDiagnosticStatus::Failed,
                severity: XeroDiagnosticSeverity::Error,
                retryable: false,
                code: "dictation_modern_assets_locale_unsupported".into(),
                message: format!("Modern dictation does not support assets for `{locale}`."),
                affected_profile_id: None,
                affected_provider_id: None,
                endpoint: None,
                remediation: Some("Choose a supported locale in Dictation settings or switch to Legacy only.".into()),
            }),
        ),
        DictationModernAssetStatusDto::Unavailable | DictationModernAssetStatusDto::Unknown => push_check(
            checks,
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::Dictation,
                status: XeroDiagnosticStatus::Warning,
                severity: XeroDiagnosticSeverity::Warning,
                retryable: true,
                code: "dictation_modern_assets_unknown".into(),
                message: "Xero could not determine whether modern Apple speech assets are installed.".into(),
                affected_profile_id: None,
                affected_provider_id: None,
                endpoint: None,
                remediation: Some("Start dictation once or run diagnostics again after Apple Speech assets finish installing.".into()),
            }),
        ),
    }
}

fn locale_supported(locale: &str, supported_locales: &[String]) -> bool {
    let normalized = normalize_locale_for_compare(locale);
    supported_locales
        .iter()
        .any(|candidate| normalize_locale_for_compare(candidate) == normalized)
}

fn normalize_locale_for_compare(locale: &str) -> String {
    locale.trim().replace('-', "_").to_ascii_lowercase()
}

fn collect_provider_checks<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    mode: XeroDoctorReportMode,
    checks: &mut DoctorCheckBuckets,
) {
    let snapshot = match load_provider_credentials_view(app, state) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            push_check(
                &mut checks.settings_dependency_checks,
                command_error_check(
                    XeroDiagnosticSubject::SettingsDependency,
                    "providers_unavailable",
                    "Xero could not load providers while generating diagnostics.",
                    error,
                    "Open Providers settings, repair app-local provider storage, then run diagnostics again.",
                ),
            );
            return;
        }
    };

    if snapshot.profiles().is_empty() {
        push_check(
            &mut checks.profile_checks,
            XeroDiagnosticCheck::skipped(
                XeroDiagnosticSubject::ProviderCredential,
                "providers_not_connected",
                "No providers are connected.",
                Some("Connect a provider in Providers settings.".into()),
            ),
        );
        return;
    }

    for profile in snapshot.profiles() {
        match provider_validation_diagnostics(&snapshot, profile) {
            Ok(profile_checks) => checks.profile_checks.extend(profile_checks),
            Err(error) => push_check(
                &mut checks.profile_checks,
                command_error_check(
                    XeroDiagnosticSubject::ProviderCredential,
                    "provider_validation_failed",
                    "Xero could not validate a provider.",
                    error,
                    "Open Providers settings, resave the affected provider, then run diagnostics again.",
                ),
            ),
        }

        if mode == XeroDoctorReportMode::ExtendedNetwork {
            match load_provider_model_catalog(app, state, &profile.profile_id, true) {
                Ok(catalog) => {
                    push_check(
                        &mut checks.model_catalog_checks,
                        provider_model_catalog_diagnostic(&catalog),
                    );
                }
                Err(error) => push_check(
                    &mut checks.model_catalog_checks,
                    command_error_check(
                        XeroDiagnosticSubject::ModelCatalog,
                        "provider_model_catalog_probe_failed",
                        format!(
                            "Xero could not refresh the model catalog for provider `{}`.",
                            profile.provider_id
                        ),
                        error,
                        "Repair the provider, credentials, or endpoint metadata before checking the connection again.",
                    ),
                ),
            }
        }
    }

    if mode == XeroDoctorReportMode::QuickLocal {
        push_check(
            &mut checks.model_catalog_checks,
            XeroDiagnosticCheck::skipped(
                XeroDiagnosticSubject::ModelCatalog,
                "provider_model_catalog_network_skipped",
                "Quick diagnostics skipped live provider model-catalog probes.",
                Some(
                    "Run extended diagnostics to probe provider reachability and model catalogs."
                        .into(),
                ),
            ),
        );
    }
}

fn collect_mcp_checks<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    checks: &mut Vec<XeroDiagnosticCheck>,
) {
    let registry_path = match state.global_db_path(app) {
        Ok(path) => path,
        Err(error) => {
            push_check(
                checks,
                command_error_check(
                    XeroDiagnosticSubject::McpRegistry,
                    "mcp_registry_path_unavailable",
                    "Xero could not resolve the MCP registry path.",
                    error,
                    "Repair app-data directory permissions, then run diagnostics again.",
                ),
            );
            return;
        }
    };

    let registry = match crate::mcp::load_mcp_registry_from_path(&registry_path) {
        Ok(registry) => registry,
        Err(error) => {
            push_check(
                checks,
                command_error_check(
                    XeroDiagnosticSubject::McpRegistry,
                    "mcp_registry_unavailable",
                    "Xero could not load the app-local MCP registry.",
                    error,
                    "Open MCP settings, repair or remove invalid server definitions, then run diagnostics again.",
                ),
            );
            return;
        }
    };

    push_mcp_registry_checks(&registry, checks);
}

fn push_mcp_registry_checks(registry: &McpRegistry, checks: &mut Vec<XeroDiagnosticCheck>) {
    if registry.servers.is_empty() {
        push_check(
            checks,
            XeroDiagnosticCheck::skipped(
                XeroDiagnosticSubject::McpRegistry,
                "mcp_registry_not_configured",
                "No MCP servers are configured.",
                Some("Add an MCP server before running dependency checks.".into()),
            ),
        );
        return;
    }

    for server in &registry.servers {
        let (status, severity, retryable, code, message, remediation) =
            match server.connection.status {
                McpConnectionStatus::Connected => (
                    XeroDiagnosticStatus::Passed,
                    XeroDiagnosticSeverity::Info,
                    false,
                    "mcp_server_connected".to_string(),
                    format!("MCP server `{}` is connected.", server.id),
                    None,
                ),
                McpConnectionStatus::Stale => (
                    XeroDiagnosticStatus::Warning,
                    XeroDiagnosticSeverity::Warning,
                    true,
                    server
                        .connection
                        .diagnostic
                        .as_ref()
                        .map(|diagnostic| diagnostic.code.clone())
                        .unwrap_or_else(|| "mcp_server_stale".into()),
                    server
                        .connection
                        .diagnostic
                        .as_ref()
                        .map(|diagnostic| diagnostic.message.clone())
                        .unwrap_or_else(|| {
                            format!("MCP server `{}` has stale connection status.", server.id)
                        }),
                    Some("Refresh MCP server statuses from Settings.".to_string()),
                ),
                McpConnectionStatus::Failed
                | McpConnectionStatus::Blocked
                | McpConnectionStatus::Misconfigured => (
                    XeroDiagnosticStatus::Failed,
                    XeroDiagnosticSeverity::Error,
                    server
                        .connection
                        .diagnostic
                        .as_ref()
                        .is_some_and(|diagnostic| diagnostic.retryable),
                    server
                        .connection
                        .diagnostic
                        .as_ref()
                        .map(|diagnostic| diagnostic.code.clone())
                        .unwrap_or_else(|| "mcp_server_unavailable".into()),
                    server
                        .connection
                        .diagnostic
                        .as_ref()
                        .map(|diagnostic| diagnostic.message.clone())
                        .unwrap_or_else(|| {
                            format!("MCP server `{}` is not currently usable.", server.id)
                        }),
                    Some(
                        "Open MCP settings, repair the server definition, then refresh status."
                            .to_string(),
                    ),
                ),
            };

        push_check(
            checks,
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::McpRegistry,
                status,
                severity,
                retryable,
                code,
                message,
                affected_profile_id: None,
                affected_provider_id: None,
                endpoint: None,
                remediation,
            }),
        );
    }
}

fn collect_project_runtime_checks<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    checks: &mut DoctorCheckBuckets,
) {
    let registry_path = match state.global_db_path(app) {
        Ok(path) => path,
        Err(error) => {
            push_check(
                &mut checks.settings_dependency_checks,
                command_error_check(
                    XeroDiagnosticSubject::SettingsDependency,
                    "project_registry_path_unavailable",
                    "Xero could not resolve the project registry path.",
                    error,
                    "Repair app-data directory permissions, then run diagnostics again.",
                ),
            );
            return;
        }
    };

    crate::db::configure_project_database_paths(&registry_path);
    let registry = match registry::read_registry(&registry_path) {
        Ok(registry) => registry,
        Err(error) => {
            push_check(
                &mut checks.settings_dependency_checks,
                command_error_check(
                    XeroDiagnosticSubject::SettingsDependency,
                    "project_registry_unavailable",
                    "Xero could not load the project registry.",
                    error,
                    "Repair or recreate the desktop project registry, then run diagnostics again.",
                ),
            );
            return;
        }
    };

    if registry.projects.is_empty() {
        push_check(
            &mut checks.runtime_supervisor_checks,
            XeroDiagnosticCheck::skipped(
                XeroDiagnosticSubject::RuntimeSupervisor,
                "runtime_projects_not_configured",
                "No imported projects are available for runtime diagnostics.",
                Some(
                    "Import a project before checking runtime session and supervisor health."
                        .into(),
                ),
            ),
        );
        return;
    }

    let credential_store_path = match state.global_db_path(app) {
        Ok(path) => Some(path),
        Err(error) => {
            push_check(
                &mut checks.settings_dependency_checks,
                command_error_check(
                    XeroDiagnosticSubject::SettingsDependency,
                    "notification_credentials_path_unavailable",
                    "Xero could not resolve the notification credential store path.",
                    error,
                    "Repair app-data directory permissions, then run diagnostics again.",
                ),
            );
            None
        }
    };
    let readiness_projector = credential_store_path
        .map(FileNotificationCredentialStore::new)
        .map(|store| store.load_readiness_projector());
    let mut notification_route_count = 0usize;

    for project in registry.projects {
        let repo_root = PathBuf::from(project.root_path.clone());
        if !repo_root.is_dir() {
            push_check(
                &mut checks.settings_dependency_checks,
                XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                    subject: XeroDiagnosticSubject::SettingsDependency,
                    status: XeroDiagnosticStatus::Failed,
                    severity: XeroDiagnosticSeverity::Error,
                    retryable: false,
                    code: "project_root_missing".into(),
                    message: format!(
                        "Imported project `{}` points at a missing repository root {}.",
                        project.project_id,
                        repo_root.display()
                    ),
                    affected_profile_id: None,
                    affected_provider_id: None,
                    endpoint: None,
                    remediation: Some(
                        "Remove the stale project entry or restore the repository path.".into(),
                    ),
                }),
            );
            continue;
        }

        collect_runtime_session_check(app, state, &repo_root, &project.project_id, checks);
        collect_runtime_supervisor_check(state, &repo_root, &project.project_id, checks);

        if let Some(projector) = readiness_projector.as_ref() {
            notification_route_count += collect_notification_checks(
                &repo_root,
                &project.project_id,
                projector,
                &mut checks.settings_dependency_checks,
            );
        }
    }

    if notification_route_count == 0 && readiness_projector.is_some() {
        push_check(
            &mut checks.settings_dependency_checks,
            XeroDiagnosticCheck::skipped(
                XeroDiagnosticSubject::SettingsDependency,
                "notification_routes_not_configured",
                "No notification routes are configured for imported projects.",
                Some(
                    "Add a notification route before checking notification credential readiness."
                        .into(),
                ),
            ),
        );
    }
}

fn collect_runtime_session_check<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    checks: &mut DoctorCheckBuckets,
) {
    let runtime = match load_runtime_session_status(state, repo_root, project_id) {
        Ok(runtime) => runtime,
        Err(error) => {
            push_check(
                &mut checks.runtime_supervisor_checks,
                command_error_check(
                    XeroDiagnosticSubject::RuntimeBinding,
                    "runtime_session_load_failed",
                    format!(
                        "Xero could not load runtime-session state for project `{project_id}`."
                    ),
                    error,
                    "Refresh the project or restart the runtime session from the Agent tab.",
                ),
            );
            return;
        }
    };

    let runtime = match crate::commands::get_runtime_session::reconcile_runtime_session(
        app, state, repo_root, runtime,
    ) {
        Ok(runtime) => runtime,
        Err(error) => {
            push_check(
                &mut checks.runtime_supervisor_checks,
                command_error_check(
                    XeroDiagnosticSubject::RuntimeBinding,
                    "runtime_session_reconcile_failed",
                    format!("Xero could not reconcile runtime-session state for project `{project_id}`."),
                    error,
                    "Repair the selected provider, then restart the runtime session from the Agent tab.",
                ),
            );
            return;
        }
    };

    let subject = XeroDiagnosticSubject::RuntimeBinding;
    if let Some(error) = runtime.last_error.as_ref() {
        push_check(
            &mut checks.runtime_supervisor_checks,
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject,
                status: XeroDiagnosticStatus::Failed,
                severity: XeroDiagnosticSeverity::Error,
                retryable: error.retryable,
                code: error.code.clone(),
                message: error.message.clone(),
                affected_profile_id: None,
                affected_provider_id: Some(runtime.provider_id.clone()),
                endpoint: None,
                remediation: Some(runtime_binding_remediation(&error.code)),
            }),
        );
        return;
    }

    match runtime.phase {
        RuntimeAuthPhase::Authenticated => push_check(
            &mut checks.runtime_supervisor_checks,
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject,
                status: XeroDiagnosticStatus::Passed,
                severity: XeroDiagnosticSeverity::Info,
                retryable: false,
                code: "runtime_session_authenticated".into(),
                message: format!(
                    "Runtime session for project `{project_id}` is authenticated for provider `{}`.",
                    runtime.provider_id
                ),
                affected_profile_id: None,
                affected_provider_id: Some(runtime.provider_id),
                endpoint: None,
                remediation: None,
            }),
        ),
        RuntimeAuthPhase::Idle | RuntimeAuthPhase::Cancelled => push_check(
            &mut checks.runtime_supervisor_checks,
            XeroDiagnosticCheck::skipped(
                subject,
                "runtime_session_not_bound",
                format!(
                    "Runtime session for project `{project_id}` is not currently bound."
                ),
                Some("Bind the selected provider from the Agent tab before starting an agent run.".into()),
            ),
        ),
        RuntimeAuthPhase::Starting
        | RuntimeAuthPhase::AwaitingBrowserCallback
        | RuntimeAuthPhase::AwaitingManualInput
        | RuntimeAuthPhase::ExchangingCode
        | RuntimeAuthPhase::Refreshing => push_check(
            &mut checks.runtime_supervisor_checks,
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject,
                status: XeroDiagnosticStatus::Warning,
                severity: XeroDiagnosticSeverity::Warning,
                retryable: true,
                code: "runtime_session_auth_in_progress".into(),
                message: format!(
                    "Runtime session for project `{project_id}` is in auth phase `{:?}`.",
                    runtime.phase
                ),
                affected_profile_id: None,
                affected_provider_id: Some(runtime.provider_id),
                endpoint: None,
                remediation: Some(
                    "Finish or restart the provider bind flow from the Agent tab.".into(),
                ),
            }),
        ),
        RuntimeAuthPhase::Failed => push_check(
            &mut checks.runtime_supervisor_checks,
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject,
                status: XeroDiagnosticStatus::Failed,
                severity: XeroDiagnosticSeverity::Error,
                retryable: true,
                code: "runtime_session_failed".into(),
                message: format!("Runtime session for project `{project_id}` is failed."),
                affected_profile_id: None,
                affected_provider_id: Some(runtime.provider_id),
                endpoint: None,
                remediation: Some(
                    "Repair the selected provider, then restart the runtime session from the Agent tab.".into(),
                ),
            }),
        ),
    }
}

fn collect_runtime_supervisor_check(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    checks: &mut DoctorCheckBuckets,
) {
    let selected_agent_session = match project_store::read_selected_agent_session(
        repo_root, project_id,
    ) {
        Ok(session) => session,
        Err(error) => {
            push_check(
                    &mut checks.runtime_supervisor_checks,
                    command_error_check(
                        XeroDiagnosticSubject::RuntimeSupervisor,
                        "agent_session_selection_unavailable",
                        format!("Xero could not read the selected agent session for project `{project_id}`."),
                        error,
                        "Refresh the project or select an agent session before checking supervisor state.",
                    ),
                );
            return;
        }
    };

    let Some(agent_session) = selected_agent_session else {
        push_check(
            &mut checks.runtime_supervisor_checks,
            XeroDiagnosticCheck::skipped(
                XeroDiagnosticSubject::RuntimeSupervisor,
                "agent_session_not_selected",
                format!("Project `{project_id}` has no selected agent session."),
                Some(
                    "Select or create an agent session before checking owned-agent runtime state."
                        .into(),
                ),
            ),
        );
        return;
    };

    match load_runtime_run_status(state, repo_root, project_id, &agent_session.agent_session_id) {
        Ok(Some(snapshot)) => push_runtime_run_check(&snapshot.run, checks),
        Ok(None) => push_check(
            &mut checks.runtime_supervisor_checks,
            XeroDiagnosticCheck::skipped(
                XeroDiagnosticSubject::RuntimeSupervisor,
                "runtime_run_not_started",
                format!(
                    "Project `{project_id}` has no durable Xero-owned agent run for selected agent session `{}`.",
                    agent_session.agent_session_id
                ),
                Some("Start a Xero-owned agent run from the Agent tab before checking runtime state.".into()),
            ),
        ),
        Err(error) => push_check(
            &mut checks.runtime_supervisor_checks,
            command_error_check(
                XeroDiagnosticSubject::RuntimeSupervisor,
                "runtime_run_probe_failed",
                format!("Xero could not inspect the owned-agent runtime state for project `{project_id}`."),
                error,
                "Stop or restart the agent run from the Agent tab, then run diagnostics again.",
            ),
        ),
    }
}

fn push_runtime_run_check(run: &project_store::RuntimeRunRecord, checks: &mut DoctorCheckBuckets) {
    let (status, severity, retryable, code, message, remediation) = match run.status {
        RuntimeRunStatus::Starting | RuntimeRunStatus::Running
            if run.transport.liveness == RuntimeRunTransportLiveness::Reachable =>
        {
            (
                XeroDiagnosticStatus::Passed,
                XeroDiagnosticSeverity::Info,
                false,
                "runtime_supervisor_reachable",
                format!(
                    "Owned-agent runtime `{}` for project `{}` is reachable.",
                    run.run_id, run.project_id,
                ),
                None,
            )
        }
        RuntimeRunStatus::Starting | RuntimeRunStatus::Running | RuntimeRunStatus::Stale => (
            XeroDiagnosticStatus::Warning,
            XeroDiagnosticSeverity::Warning,
            true,
            "runtime_supervisor_unreachable",
            format!(
                "Owned-agent runtime `{}` for project `{}` is not reachable.",
                run.run_id, run.project_id
            ),
            Some("Reconnect, stop, or restart the agent run from the Agent tab.".into()),
        ),
        RuntimeRunStatus::Stopped => (
            XeroDiagnosticStatus::Skipped,
            XeroDiagnosticSeverity::Info,
            false,
            "runtime_supervisor_stopped",
            format!(
                "Owned-agent runtime `{}` for project `{}` is stopped.",
                run.run_id, run.project_id
            ),
            Some("Start a new agent run when live runtime diagnostics are needed.".into()),
        ),
        RuntimeRunStatus::Failed => (
            XeroDiagnosticStatus::Failed,
            XeroDiagnosticSeverity::Error,
            false,
            run.last_error
                .as_ref()
                .map(|error| error.code.as_str())
                .unwrap_or("runtime_supervisor_failed"),
            run.last_error
                .as_ref()
                .map(|error| error.message.clone())
                .unwrap_or_else(|| {
                    format!(
                        "Owned-agent runtime `{}` for project `{}` failed.",
                        run.run_id, run.project_id
                    )
                }),
            Some("Inspect the final runtime checkpoint, repair the provider or command environment, then start a new agent run.".into()),
        ),
    };

    push_check(
        &mut checks.runtime_supervisor_checks,
        XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::RuntimeSupervisor,
            status,
            severity,
            retryable,
            code: code.into(),
            message,
            affected_profile_id: None,
            affected_provider_id: Some(run.provider_id.clone()),
            endpoint: None,
            remediation,
        }),
    );
}

fn collect_notification_checks(
    repo_root: &Path,
    project_id: &str,
    readiness_projector: &crate::notifications::NotificationCredentialReadinessProjector,
    checks: &mut Vec<XeroDiagnosticCheck>,
) -> usize {
    let routes = match project_store::load_notification_routes(repo_root, project_id) {
        Ok(routes) => routes,
        Err(error) => {
            push_check(
                checks,
                command_error_check(
                    XeroDiagnosticSubject::SettingsDependency,
                    "notification_routes_load_failed",
                    format!("Xero could not load notification routes for project `{project_id}`."),
                    error,
                    "Refresh the project or repair app-data notification route state.",
                ),
            );
            return 0;
        }
    };

    for route in &routes {
        let route_kind = match NotificationRouteKind::parse(&route.route_kind) {
            Ok(route_kind) => route_kind,
            Err(error) => {
                push_check(
                    checks,
                    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                        subject: XeroDiagnosticSubject::SettingsDependency,
                        status: XeroDiagnosticStatus::Failed,
                        severity: XeroDiagnosticSeverity::Error,
                        retryable: error.retryable,
                        code: error.code,
                        message: error.message,
                        affected_profile_id: None,
                        affected_provider_id: None,
                        endpoint: None,
                        remediation: Some(
                            "Remove or recreate the unsupported notification route.".into(),
                        ),
                    }),
                );
                continue;
            }
        };
        let readiness = readiness_projector.project_route(project_id, &route.route_id, route_kind);

        if readiness.ready {
            push_check(
                checks,
                XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                    subject: XeroDiagnosticSubject::SettingsDependency,
                    status: XeroDiagnosticStatus::Passed,
                    severity: XeroDiagnosticSeverity::Info,
                    retryable: false,
                    code: "notification_route_credentials_ready".into(),
                    message: format!(
                        "Notification route `{}` for project `{project_id}` has ready credentials.",
                        route.route_id
                    ),
                    affected_profile_id: None,
                    affected_provider_id: None,
                    endpoint: None,
                    remediation: None,
                }),
            );
        } else if let Some(diagnostic) = readiness.diagnostic {
            push_check(
                checks,
                XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                    subject: XeroDiagnosticSubject::SettingsDependency,
                    status: XeroDiagnosticStatus::Failed,
                    severity: XeroDiagnosticSeverity::Error,
                    retryable: diagnostic.retryable,
                    code: diagnostic.code,
                    message: diagnostic.message,
                    affected_profile_id: None,
                    affected_provider_id: None,
                    endpoint: None,
                    remediation: Some(
                        "Open Notifications settings and repair the route credentials.".into(),
                    ),
                }),
            );
        }
    }

    routes.len()
}

fn collect_app_path_checks<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    checks: &mut Vec<XeroDiagnosticCheck>,
) {
    push_path_check(
        checks,
        "app_data_dir",
        "app-data directory",
        state.app_data_dir(app),
        PathExpectation::DirectoryMayBeCreated,
    );
    push_path_check(
        checks,
        "project_registry",
        "project registry",
        state.global_db_path(app),
        PathExpectation::OptionalFile,
    );
    push_path_check(
        checks,
        "runtime_settings",
        "runtime settings",
        state.global_db_path(app),
        PathExpectation::OptionalFile,
    );
    push_path_check(
        checks,
        "mcp_registry",
        "MCP registry",
        state.global_db_path(app),
        PathExpectation::OptionalFile,
    );
    push_path_check(
        checks,
        "notification_credentials",
        "notification credential store",
        state.global_db_path(app),
        PathExpectation::OptionalFile,
    );
    push_path_check(
        checks,
        "provider_model_catalog_cache",
        "provider model catalog cache",
        state.global_db_path(app),
        PathExpectation::OptionalFile,
    );
}

fn collect_environment_profile_checks<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    checks: &mut Vec<XeroDiagnosticCheck>,
) {
    let database_path = match state.global_db_path(app) {
        Ok(path) => path,
        Err(error) => {
            push_check(
                checks,
                command_error_check(
                    XeroDiagnosticSubject::SettingsDependency,
                    "environment_profile_path_unavailable",
                    "Xero could not resolve the global environment profile path.",
                    error,
                    "Repair app-data directory permissions, then run diagnostics again.",
                ),
            );
            return;
        }
    };

    let status = match environment_service::environment_discovery_status(&database_path) {
        Ok(status) => status,
        Err(error) => {
            push_check(
                checks,
                command_error_check(
                    XeroDiagnosticSubject::SettingsDependency,
                    "environment_profile_status_unavailable",
                    "Xero could not load environment profile status.",
                    error,
                    "Refresh environment discovery from Settings, then run diagnostics again.",
                ),
            );
            return;
        }
    };

    let summary = match environment_service::environment_profile_summary(&database_path) {
        Ok(summary) => summary,
        Err(error) => {
            push_check(
                checks,
                command_error_check(
                    XeroDiagnosticSubject::SettingsDependency,
                    "environment_profile_summary_unavailable",
                    "Xero could not decode the saved environment profile summary.",
                    error,
                    "Refresh environment discovery from Settings, then run diagnostics again.",
                ),
            );
            return;
        }
    };

    let Some(summary) = summary else {
        push_check(
            checks,
            XeroDiagnosticCheck::skipped(
                XeroDiagnosticSubject::SettingsDependency,
                "environment_profile_missing",
                "No developer environment profile has been recorded yet.",
                Some("Open onboarding or refresh environment discovery from Settings.".into()),
            ),
        );
        return;
    };

    push_environment_summary_check(checks, &summary, status.stale);
    if status.stale {
        push_check(
            checks,
            XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::SettingsDependency,
                status: XeroDiagnosticStatus::Warning,
                severity: XeroDiagnosticSeverity::Warning,
                retryable: true,
                code: "environment_profile_stale".into(),
                message: "The developer environment profile is stale.".into(),
                affected_profile_id: None,
                affected_provider_id: None,
                endpoint: None,
                remediation: Some("Refresh environment discovery from Settings.".into()),
            }),
        );
    }

    for diagnostic in &status.diagnostics {
        push_environment_diagnostic_check(checks, diagnostic);
    }
}

fn push_environment_summary_check(
    checks: &mut Vec<XeroDiagnosticCheck>,
    summary: &EnvironmentProfileSummary,
    stale: bool,
) {
    let present_tools = summary.tools.iter().filter(|tool| tool.present).count();
    let ready_capabilities = summary
        .capabilities
        .iter()
        .filter(|capability| capability.state == EnvironmentCapabilityState::Ready)
        .count();
    let important_missing = summary
        .capabilities
        .iter()
        .filter(|capability| {
            matches!(
                capability.state,
                EnvironmentCapabilityState::Missing
                    | EnvironmentCapabilityState::Partial
                    | EnvironmentCapabilityState::Blocked
            )
        })
        .take(3)
        .map(|capability| capability.id.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let (status, severity, retryable, code, remediation) = match summary.status {
        EnvironmentProfileStatus::Ready if !stale => (
            XeroDiagnosticStatus::Passed,
            XeroDiagnosticSeverity::Info,
            false,
            "environment_profile_ready",
            None,
        ),
        EnvironmentProfileStatus::Ready | EnvironmentProfileStatus::Partial => (
            XeroDiagnosticStatus::Warning,
            XeroDiagnosticSeverity::Warning,
            true,
            "environment_profile_partial_or_stale",
            Some("Refresh environment discovery from Settings.".to_string()),
        ),
        EnvironmentProfileStatus::Pending | EnvironmentProfileStatus::Probing => (
            XeroDiagnosticStatus::Skipped,
            XeroDiagnosticSeverity::Info,
            false,
            "environment_profile_probe_in_progress",
            Some("Run diagnostics again after environment discovery completes.".to_string()),
        ),
        EnvironmentProfileStatus::Failed => (
            XeroDiagnosticStatus::Failed,
            XeroDiagnosticSeverity::Error,
            true,
            "environment_profile_failed",
            Some("Refresh environment discovery from Settings.".to_string()),
        ),
    };

    let mut message = format!(
        "Developer environment profile `{}` includes {present_tools} present tool(s) and {ready_capabilities} ready capability fact(s).",
        summary.status.as_str()
    );
    if !important_missing.is_empty() {
        message.push_str(&format!(" Attention needed: {important_missing}."));
    }

    push_check(
        checks,
        XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::SettingsDependency,
            status,
            severity,
            retryable,
            code: code.into(),
            message,
            affected_profile_id: None,
            affected_provider_id: None,
            endpoint: None,
            remediation,
        }),
    );
}

fn push_environment_diagnostic_check(
    checks: &mut Vec<XeroDiagnosticCheck>,
    diagnostic: &EnvironmentDiagnostic,
) {
    let (status, severity, remediation) = match diagnostic.severity {
        EnvironmentDiagnosticSeverity::Info => (
            XeroDiagnosticStatus::Skipped,
            XeroDiagnosticSeverity::Info,
            None,
        ),
        EnvironmentDiagnosticSeverity::Warning => (
            XeroDiagnosticStatus::Warning,
            XeroDiagnosticSeverity::Warning,
            Some("Refresh environment discovery after changing local developer tools.".to_string()),
        ),
        EnvironmentDiagnosticSeverity::Error => (
            XeroDiagnosticStatus::Failed,
            XeroDiagnosticSeverity::Error,
            Some(
                "Repair the local tool or app-data state, then refresh environment discovery."
                    .to_string(),
            ),
        ),
    };

    push_check(
        checks,
        XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::SettingsDependency,
            status,
            severity,
            retryable: diagnostic.retryable,
            code: format!("environment_{}", diagnostic.code),
            message: diagnostic.message.clone(),
            affected_profile_id: None,
            affected_provider_id: None,
            endpoint: None,
            remediation,
        }),
    );
}

#[derive(Debug, Copy, Clone)]
enum PathExpectation {
    DirectoryMayBeCreated,
    OptionalFile,
}

fn push_path_check(
    checks: &mut Vec<XeroDiagnosticCheck>,
    id: &str,
    label: &str,
    path: Result<PathBuf, CommandError>,
    expectation: PathExpectation,
) {
    let path = match path {
        Ok(path) => path,
        Err(error) => {
            push_check(
                checks,
                command_error_check(
                    XeroDiagnosticSubject::SettingsDependency,
                    format!("settings_path_{id}_unavailable"),
                    format!("Xero could not resolve the {label} path."),
                    error,
                    "Repair app-data directory permissions, then run diagnostics again.",
                ),
            );
            return;
        }
    };

    match expectation {
        PathExpectation::DirectoryMayBeCreated => {
            let exists = path.exists();
            if !exists || path.is_dir() {
                push_check(
                    checks,
                    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                        subject: XeroDiagnosticSubject::SettingsDependency,
                        status: XeroDiagnosticStatus::Passed,
                        severity: XeroDiagnosticSeverity::Info,
                        retryable: false,
                        code: format!("settings_path_{id}_ready"),
                        message: format!("Xero resolved the {label} path at {}.", path.display()),
                        affected_profile_id: None,
                        affected_provider_id: None,
                        endpoint: None,
                        remediation: None,
                    }),
                );
            } else {
                push_check(
                    checks,
                    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                        subject: XeroDiagnosticSubject::SettingsDependency,
                        status: XeroDiagnosticStatus::Failed,
                        severity: XeroDiagnosticSeverity::Error,
                        retryable: false,
                        code: format!("settings_path_{id}_not_directory"),
                        message: format!(
                            "Xero resolved the {label} path at {}, but it is not a directory.",
                            path.display()
                        ),
                        affected_profile_id: None,
                        affected_provider_id: None,
                        endpoint: None,
                        remediation: Some(
                            "Move the blocking file or choose a usable app-data directory.".into(),
                        ),
                    }),
                );
            }
        }
        PathExpectation::OptionalFile => {
            if !path.exists() {
                push_check(
                    checks,
                    XeroDiagnosticCheck::skipped(
                        XeroDiagnosticSubject::SettingsDependency,
                        format!("settings_path_{id}_missing"),
                        format!("The {label} file does not exist yet at {}.", path.display()),
                        Some(
                            "Xero will create this file when the related feature is configured."
                                .into(),
                        ),
                    ),
                );
                return;
            }

            if path.is_file() {
                push_check(
                    checks,
                    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                        subject: XeroDiagnosticSubject::SettingsDependency,
                        status: XeroDiagnosticStatus::Passed,
                        severity: XeroDiagnosticSeverity::Info,
                        retryable: false,
                        code: format!("settings_path_{id}_ready"),
                        message: format!("Xero can see the {label} file at {}.", path.display()),
                        affected_profile_id: None,
                        affected_provider_id: None,
                        endpoint: None,
                        remediation: None,
                    }),
                );
            } else {
                push_check(
                    checks,
                    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                        subject: XeroDiagnosticSubject::SettingsDependency,
                        status: XeroDiagnosticStatus::Failed,
                        severity: XeroDiagnosticSeverity::Error,
                        retryable: false,
                        code: format!("settings_path_{id}_not_file"),
                        message: format!(
                            "Xero expected the {label} path at {} to be a file.",
                            path.display()
                        ),
                        affected_profile_id: None,
                        affected_provider_id: None,
                        endpoint: None,
                        remediation: Some(
                            "Move the blocking directory or let Xero recreate the settings file."
                                .into(),
                        ),
                    }),
                );
            }
        }
    }
}

fn command_error_check(
    subject: XeroDiagnosticSubject,
    code: impl Into<String>,
    message: impl Into<String>,
    error: CommandError,
    remediation: impl Into<String>,
) -> CommandResult<XeroDiagnosticCheck> {
    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
        subject,
        status: XeroDiagnosticStatus::Failed,
        severity: XeroDiagnosticSeverity::Error,
        retryable: error.retryable,
        code: code.into(),
        message: format!("{} [{}] {}", message.into(), error.code, error.message),
        affected_profile_id: None,
        affected_provider_id: None,
        endpoint: None,
        remediation: Some(remediation.into()),
    })
}

fn runtime_binding_remediation(code: &str) -> String {
    match code {
        "provider_credentials_missing" | "provider_missing" => {
            "Add credentials or choose another ready provider in Providers settings.".into()
        }
        "provider_credentials_malformed"
        | "providers_invalid"
        | "provider_provider_mismatch"
        | "provider_runtime_kind_invalid" => {
            "Reconnect or resave the selected provider in Providers settings.".into()
        }
        "provider_ambient_auth_failed" | "bedrock_ambient_auth_missing" | "vertex_ambient_auth_missing" => {
            "Refresh the ambient provider login in your shell or cloud SDK, then run diagnostics again.".into()
        }
        "runtime_binding_stale" => {
            "Restart the runtime session after resaving or reselecting the provider.".into()
        }
        _ => "Open Diagnostics in Settings, repair the selected provider, then restart the runtime session.".into(),
    }
}

fn push_check(checks: &mut Vec<XeroDiagnosticCheck>, result: CommandResult<XeroDiagnosticCheck>) {
    match result {
        Ok(check) => checks.push(check),
        Err(error) => {
            let fallback = XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
                subject: XeroDiagnosticSubject::SettingsDependency,
                status: XeroDiagnosticStatus::Failed,
                severity: XeroDiagnosticSeverity::Error,
                retryable: error.retryable,
                code: "doctor_report_check_projection_failed".into(),
                message: format!(
                    "Xero could not project one diagnostic check: [{}] {}",
                    error.code, error.message
                ),
                affected_profile_id: None,
                affected_provider_id: None,
                endpoint: None,
                remediation: Some(
                    "Retry diagnostics after repairing the reported settings state.".into(),
                ),
            });
            if let Ok(fallback) = fallback {
                checks.push(fallback);
            }
        }
    }
}

fn doctor_report_id(generated_at: &str) -> String {
    let suffix = generated_at
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    format!("doctor-{suffix}")
}
