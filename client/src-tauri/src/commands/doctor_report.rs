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
    mcp::{McpConnectionStatus, McpRegistry},
    notifications::{FileNotificationCredentialStore, NotificationRouteKind},
    provider_models::load_provider_model_catalog,
    registry,
    runtime::{
        provider_model_catalog_diagnostic, provider_validation_diagnostics, CadenceDiagnosticCheck,
        CadenceDiagnosticCheckInput, CadenceDiagnosticSeverity, CadenceDiagnosticStatus,
        CadenceDiagnosticSubject, CadenceDoctorReport, CadenceDoctorReportInput,
        CadenceDoctorReportMode, CadenceDoctorVersionInfo,
    },
    state::DesktopState,
};

use super::runtime_support::{load_runtime_run_status, load_runtime_session_status};

#[tauri::command]
pub fn run_doctor_report<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RunDoctorReportRequestDto,
) -> CommandResult<CadenceDoctorReport> {
    let mode = request.mode.unwrap_or(CadenceDoctorReportMode::QuickLocal);
    let generated_at = now_timestamp();
    let mut checks = DoctorCheckBuckets::default();

    collect_app_path_checks(&app, state.inner(), &mut checks.settings_dependency_checks);
    collect_dictation_checks(&app, state.inner(), &mut checks.dictation_checks);
    collect_provider_checks(&app, state.inner(), mode, &mut checks);
    collect_mcp_checks(&app, state.inner(), &mut checks.mcp_dependency_checks);
    collect_project_runtime_checks(&app, state.inner(), &mut checks);

    CadenceDoctorReport::new(CadenceDoctorReportInput {
        report_id: doctor_report_id(&generated_at),
        generated_at,
        mode,
        versions: CadenceDoctorVersionInfo {
            app_version: env!("CARGO_PKG_VERSION").into(),
            runtime_supervisor_version: Some(env!("CARGO_PKG_VERSION").into()),
            runtime_protocol_version: Some(format!(
                "supervisor-v{}",
                crate::runtime::protocol::SUPERVISOR_PROTOCOL_VERSION
            )),
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
    profile_checks: Vec<CadenceDiagnosticCheck>,
    dictation_checks: Vec<CadenceDiagnosticCheck>,
    model_catalog_checks: Vec<CadenceDiagnosticCheck>,
    runtime_supervisor_checks: Vec<CadenceDiagnosticCheck>,
    mcp_dependency_checks: Vec<CadenceDiagnosticCheck>,
    settings_dependency_checks: Vec<CadenceDiagnosticCheck>,
}

fn collect_dictation_checks<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    checks: &mut Vec<CadenceDiagnosticCheck>,
) {
    let status = probe_dictation_status();
    let settings = match load_dictation_settings(app, state) {
        Ok(settings) => Some(settings),
        Err(error) => {
            push_check(
                checks,
                command_error_check(
                    CadenceDiagnosticSubject::Dictation,
                    "dictation_settings_unavailable",
                    "Cadence could not load dictation settings while generating diagnostics.",
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
            CadenceDiagnosticCheck::skipped(
                CadenceDiagnosticSubject::Dictation,
                "dictation_platform_unsupported",
                "Native dictation is only available on macOS in this release.",
                Some("Use Cadence on macOS to enable native dictation.".into()),
            ),
        );
        return;
    }

    push_check(
        checks,
        CadenceDiagnosticCheck::passed(
            CadenceDiagnosticSubject::Dictation,
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
            CadenceDiagnosticCheck::passed(
                CadenceDiagnosticSubject::Dictation,
                "dictation_modern_sdk_compiled",
                "Cadence was built with the macOS 26 dictation SDK.",
            ),
        );
    } else if status.legacy.available {
        push_check(
            checks,
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::Dictation,
                status: CadenceDiagnosticStatus::Warning,
                severity: CadenceDiagnosticSeverity::Warning,
                retryable: false,
                code: "dictation_modern_sdk_unavailable_legacy_available".into(),
                message: "Cadence was built without macOS 26 SpeechAnalyzer support, but legacy dictation is available.".into(),
                affected_profile_id: None,
                affected_provider_id: None,
                endpoint: None,
                remediation: Some(
                    "Use Automatic or Legacy only in Dictation settings, or rebuild Cadence with the macOS 26 SDK.".into(),
                ),
            }),
        );
    } else {
        push_check(
            checks,
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::Dictation,
                status: CadenceDiagnosticStatus::Failed,
                severity: CadenceDiagnosticSeverity::Error,
                retryable: false,
                code: "dictation_no_native_engine_available".into(),
                message: "Cadence could not find an available native macOS dictation engine.".into(),
                affected_profile_id: None,
                affected_provider_id: None,
                endpoint: None,
                remediation: Some(
                    "Update macOS, enable Apple Speech Recognition support, or rebuild Cadence with the macOS 26 SDK.".into(),
                ),
            }),
        );
    }

    push_permission_check(
        checks,
        "microphone",
        status.microphone_permission,
        "Open System Settings > Privacy & Security > Microphone and allow Cadence.",
    );
    push_permission_check(
        checks,
        "speech",
        status.speech_permission,
        "Open System Settings > Privacy & Security > Speech Recognition and allow Cadence.",
    );

    if let Some(settings) = settings {
        push_selected_engine_check(checks, settings.engine_preference, &status);
        push_selected_locale_check(checks, settings.locale.as_deref(), &status);
    }

    push_modern_asset_check(checks, &status);
}

fn push_selected_engine_check(
    checks: &mut Vec<CadenceDiagnosticCheck>,
    preference: DictationEnginePreferenceDto,
    status: &DictationStatusDto,
) {
    let (check_status, severity, retryable, code, message, remediation) = match preference {
        DictationEnginePreferenceDto::Modern if status.modern.available => (
            CadenceDiagnosticStatus::Passed,
            CadenceDiagnosticSeverity::Info,
            false,
            "dictation_selected_engine_available",
            "The selected modern dictation engine is available.".to_string(),
            None,
        ),
        DictationEnginePreferenceDto::Legacy if status.legacy.available => (
            CadenceDiagnosticStatus::Passed,
            CadenceDiagnosticSeverity::Info,
            false,
            "dictation_selected_engine_available",
            "The selected legacy dictation engine is available.".to_string(),
            None,
        ),
        DictationEnginePreferenceDto::Automatic if status.modern.available && status.legacy.available => (
            CadenceDiagnosticStatus::Passed,
            CadenceDiagnosticSeverity::Info,
            false,
            "dictation_automatic_modern_with_legacy_fallback",
            "Automatic dictation can use modern dictation with legacy fallback.".to_string(),
            None,
        ),
        DictationEnginePreferenceDto::Automatic if status.modern.available => (
            CadenceDiagnosticStatus::Passed,
            CadenceDiagnosticSeverity::Info,
            false,
            "dictation_automatic_modern_available",
            "Automatic dictation can use the modern dictation engine.".to_string(),
            None,
        ),
        DictationEnginePreferenceDto::Automatic if status.legacy.available => (
            CadenceDiagnosticStatus::Warning,
            CadenceDiagnosticSeverity::Warning,
            false,
            "dictation_modern_unavailable_legacy_available",
            "Automatic dictation will use legacy dictation because modern dictation is unavailable.".to_string(),
            Some(
                "This is usable. Rebuild with the macOS 26 SDK or update macOS to enable the modern engine.".to_string(),
            ),
        ),
        DictationEnginePreferenceDto::Modern => (
            CadenceDiagnosticStatus::Failed,
            CadenceDiagnosticSeverity::Error,
            false,
            "dictation_selected_modern_unavailable",
            "Dictation is set to prefer the modern engine, but modern dictation is unavailable.".to_string(),
            Some("Choose Automatic or Legacy only in Dictation settings.".to_string()),
        ),
        DictationEnginePreferenceDto::Legacy => (
            CadenceDiagnosticStatus::Failed,
            CadenceDiagnosticSeverity::Error,
            false,
            "dictation_selected_legacy_unavailable",
            "Dictation is set to Legacy only, but legacy dictation is unavailable.".to_string(),
            Some("Choose Automatic in Dictation settings or update macOS Speech Recognition support.".to_string()),
        ),
        DictationEnginePreferenceDto::Automatic => (
            CadenceDiagnosticStatus::Failed,
            CadenceDiagnosticSeverity::Error,
            true,
            "dictation_automatic_no_engine_available",
            "Automatic dictation could not find any available native dictation engine.".to_string(),
            Some("Update macOS or check Apple Speech Recognition availability, then run diagnostics again.".to_string()),
        ),
    };

    push_check(
        checks,
        CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
            subject: CadenceDiagnosticSubject::Dictation,
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
    checks: &mut Vec<CadenceDiagnosticCheck>,
    permission: &'static str,
    state: DictationPermissionStateDto,
    denied_remediation: &'static str,
) {
    let (status, severity, retryable, code, message, remediation) = match state {
        DictationPermissionStateDto::Authorized => (
            CadenceDiagnosticStatus::Passed,
            CadenceDiagnosticSeverity::Info,
            false,
            format!("dictation_{permission}_permission_authorized"),
            format!("Cadence has {permission} permission for dictation."),
            None,
        ),
        DictationPermissionStateDto::NotDetermined => (
            CadenceDiagnosticStatus::Warning,
            CadenceDiagnosticSeverity::Warning,
            true,
            format!("dictation_{permission}_permission_not_determined"),
            format!("Cadence has not requested {permission} permission yet."),
            Some("Start dictation once to trigger the macOS permission prompt.".to_string()),
        ),
        DictationPermissionStateDto::Denied | DictationPermissionStateDto::Restricted => (
            CadenceDiagnosticStatus::Failed,
            CadenceDiagnosticSeverity::Error,
            false,
            format!("dictation_{permission}_permission_denied"),
            format!("Cadence cannot use dictation because {permission} permission is {state:?}."),
            Some(denied_remediation.to_string()),
        ),
        DictationPermissionStateDto::Unsupported | DictationPermissionStateDto::Unknown => (
            CadenceDiagnosticStatus::Warning,
            CadenceDiagnosticSeverity::Warning,
            true,
            format!("dictation_{permission}_permission_unknown"),
            format!("Cadence could not determine {permission} permission state."),
            Some(
                "Open System Settings privacy permissions, then run diagnostics again.".to_string(),
            ),
        ),
    };

    push_check(
        checks,
        CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
            subject: CadenceDiagnosticSubject::Dictation,
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
    checks: &mut Vec<CadenceDiagnosticCheck>,
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
            CadenceDiagnosticCheck::skipped(
                CadenceDiagnosticSubject::Dictation,
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
            CadenceDiagnosticCheck::skipped(
                CadenceDiagnosticSubject::Dictation,
                "dictation_supported_locale_list_unavailable",
                format!("Cadence could not verify selected dictation locale `{locale}` because macOS did not report supported locales."),
                Some("Start dictation or run diagnostics again after Speech Recognition becomes available.".into()),
            ),
        );
        return;
    }

    if locale_supported(locale, &status.supported_locales) {
        push_check(
            checks,
            CadenceDiagnosticCheck::passed(
                CadenceDiagnosticSubject::Dictation,
                "dictation_selected_locale_supported",
                format!("Selected dictation locale `{locale}` is supported."),
            ),
        );
    } else {
        push_check(
            checks,
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::Dictation,
                status: CadenceDiagnosticStatus::Failed,
                severity: CadenceDiagnosticSeverity::Error,
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

fn push_modern_asset_check(checks: &mut Vec<CadenceDiagnosticCheck>, status: &DictationStatusDto) {
    if !status.modern.available {
        push_check(
            checks,
            CadenceDiagnosticCheck::skipped(
                CadenceDiagnosticSubject::Dictation,
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
            CadenceDiagnosticCheck::passed(
                CadenceDiagnosticSubject::Dictation,
                "dictation_modern_assets_installed",
                format!("Modern Apple speech assets are installed for `{locale}`."),
            ),
        ),
        DictationModernAssetStatusDto::NotInstalled => push_check(
            checks,
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::Dictation,
                status: CadenceDiagnosticStatus::Warning,
                severity: CadenceDiagnosticSeverity::Warning,
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
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::Dictation,
                status: CadenceDiagnosticStatus::Failed,
                severity: CadenceDiagnosticSeverity::Error,
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
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::Dictation,
                status: CadenceDiagnosticStatus::Warning,
                severity: CadenceDiagnosticSeverity::Warning,
                retryable: true,
                code: "dictation_modern_assets_unknown".into(),
                message: "Cadence could not determine whether modern Apple speech assets are installed.".into(),
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
    mode: CadenceDoctorReportMode,
    checks: &mut DoctorCheckBuckets,
) {
    let snapshot = match load_provider_credentials_view(app, state) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            push_check(
                &mut checks.settings_dependency_checks,
                command_error_check(
                    CadenceDiagnosticSubject::SettingsDependency,
                    "providers_unavailable",
                    "Cadence could not load providers while generating diagnostics.",
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
            CadenceDiagnosticCheck::skipped(
                CadenceDiagnosticSubject::ProviderCredential,
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
                    CadenceDiagnosticSubject::ProviderCredential,
                    "provider_validation_failed",
                    "Cadence could not validate a provider.",
                    error,
                    "Open Providers settings, resave the affected provider, then run diagnostics again.",
                ),
            ),
        }

        if mode == CadenceDoctorReportMode::ExtendedNetwork {
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
                        CadenceDiagnosticSubject::ModelCatalog,
                        "provider_model_catalog_probe_failed",
                        format!(
                            "Cadence could not refresh the model catalog for provider `{}`.",
                            profile.provider_id
                        ),
                        error,
                        "Repair the provider, credentials, or endpoint metadata before checking the connection again.",
                    ),
                ),
            }
        }
    }

    if mode == CadenceDoctorReportMode::QuickLocal {
        push_check(
            &mut checks.model_catalog_checks,
            CadenceDiagnosticCheck::skipped(
                CadenceDiagnosticSubject::ModelCatalog,
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
    checks: &mut Vec<CadenceDiagnosticCheck>,
) {
    let registry_path = match state.global_db_path(app) {
        Ok(path) => path,
        Err(error) => {
            push_check(
                checks,
                command_error_check(
                    CadenceDiagnosticSubject::McpRegistry,
                    "mcp_registry_path_unavailable",
                    "Cadence could not resolve the MCP registry path.",
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
                    CadenceDiagnosticSubject::McpRegistry,
                    "mcp_registry_unavailable",
                    "Cadence could not load the app-local MCP registry.",
                    error,
                    "Open MCP settings, repair or remove invalid server definitions, then run diagnostics again.",
                ),
            );
            return;
        }
    };

    push_mcp_registry_checks(&registry, checks);
}

fn push_mcp_registry_checks(registry: &McpRegistry, checks: &mut Vec<CadenceDiagnosticCheck>) {
    if registry.servers.is_empty() {
        push_check(
            checks,
            CadenceDiagnosticCheck::skipped(
                CadenceDiagnosticSubject::McpRegistry,
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
                    CadenceDiagnosticStatus::Passed,
                    CadenceDiagnosticSeverity::Info,
                    false,
                    "mcp_server_connected".to_string(),
                    format!("MCP server `{}` is connected.", server.id),
                    None,
                ),
                McpConnectionStatus::Stale => (
                    CadenceDiagnosticStatus::Warning,
                    CadenceDiagnosticSeverity::Warning,
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
                    CadenceDiagnosticStatus::Failed,
                    CadenceDiagnosticSeverity::Error,
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
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::McpRegistry,
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
                    CadenceDiagnosticSubject::SettingsDependency,
                    "project_registry_path_unavailable",
                    "Cadence could not resolve the project registry path.",
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
                    CadenceDiagnosticSubject::SettingsDependency,
                    "project_registry_unavailable",
                    "Cadence could not load the project registry.",
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
            CadenceDiagnosticCheck::skipped(
                CadenceDiagnosticSubject::RuntimeSupervisor,
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
                    CadenceDiagnosticSubject::SettingsDependency,
                    "notification_credentials_path_unavailable",
                    "Cadence could not resolve the notification credential store path.",
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
                CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                    subject: CadenceDiagnosticSubject::SettingsDependency,
                    status: CadenceDiagnosticStatus::Failed,
                    severity: CadenceDiagnosticSeverity::Error,
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
            CadenceDiagnosticCheck::skipped(
                CadenceDiagnosticSubject::SettingsDependency,
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
                    CadenceDiagnosticSubject::RuntimeBinding,
                    "runtime_session_load_failed",
                    format!(
                        "Cadence could not load runtime-session state for project `{project_id}`."
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
                    CadenceDiagnosticSubject::RuntimeBinding,
                    "runtime_session_reconcile_failed",
                    format!("Cadence could not reconcile runtime-session state for project `{project_id}`."),
                    error,
                    "Repair the selected provider, then restart the runtime session from the Agent tab.",
                ),
            );
            return;
        }
    };

    let subject = CadenceDiagnosticSubject::RuntimeBinding;
    if let Some(error) = runtime.last_error.as_ref() {
        push_check(
            &mut checks.runtime_supervisor_checks,
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject,
                status: CadenceDiagnosticStatus::Failed,
                severity: CadenceDiagnosticSeverity::Error,
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
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject,
                status: CadenceDiagnosticStatus::Passed,
                severity: CadenceDiagnosticSeverity::Info,
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
            CadenceDiagnosticCheck::skipped(
                subject,
                "runtime_session_not_bound",
                format!(
                    "Runtime session for project `{project_id}` is not currently bound."
                ),
                Some("Bind the selected provider from the Agent tab before starting a supervised run.".into()),
            ),
        ),
        RuntimeAuthPhase::Starting
        | RuntimeAuthPhase::AwaitingBrowserCallback
        | RuntimeAuthPhase::AwaitingManualInput
        | RuntimeAuthPhase::ExchangingCode
        | RuntimeAuthPhase::Refreshing => push_check(
            &mut checks.runtime_supervisor_checks,
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject,
                status: CadenceDiagnosticStatus::Warning,
                severity: CadenceDiagnosticSeverity::Warning,
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
            CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject,
                status: CadenceDiagnosticStatus::Failed,
                severity: CadenceDiagnosticSeverity::Error,
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
                        CadenceDiagnosticSubject::RuntimeSupervisor,
                        "agent_session_selection_unavailable",
                        format!("Cadence could not read the selected agent session for project `{project_id}`."),
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
            CadenceDiagnosticCheck::skipped(
                CadenceDiagnosticSubject::RuntimeSupervisor,
                "agent_session_not_selected",
                format!("Project `{project_id}` has no selected agent session."),
                Some(
                    "Select or create an agent session before checking detached supervisor state."
                        .into(),
                ),
            ),
        );
        return;
    };

    match load_runtime_run_status(state, repo_root, project_id, &agent_session.agent_session_id) {
        Ok(Some(snapshot)) => push_runtime_run_check(state, &snapshot.run, checks),
        Ok(None) => push_check(
            &mut checks.runtime_supervisor_checks,
            CadenceDiagnosticCheck::skipped(
                CadenceDiagnosticSubject::RuntimeSupervisor,
                "runtime_run_not_started",
                format!(
                    "Project `{project_id}` has no durable supervised runtime run for selected agent session `{}`.",
                    agent_session.agent_session_id
                ),
                Some("Start a supervised run from the Agent tab before checking detached supervisor state.".into()),
            ),
        ),
        Err(error) => push_check(
            &mut checks.runtime_supervisor_checks,
            command_error_check(
                CadenceDiagnosticSubject::RuntimeSupervisor,
                "runtime_run_probe_failed",
                format!("Cadence could not probe the runtime supervisor for project `{project_id}`."),
                error,
                "Stop or restart the supervised run from the Agent tab, then run diagnostics again.",
            ),
        ),
    }
}

fn push_runtime_run_check(
    state: &DesktopState,
    run: &project_store::RuntimeRunRecord,
    checks: &mut DoctorCheckBuckets,
) {
    let remembered = state
        .runtime_supervisor_controller()
        .snapshot(&run.project_id, &run.agent_session_id)
        .is_some_and(|active| active.run_id == run.run_id);

    let (status, severity, retryable, code, message, remediation) = match run.status {
        RuntimeRunStatus::Starting | RuntimeRunStatus::Running
            if run.transport.liveness == RuntimeRunTransportLiveness::Reachable =>
        {
            (
                CadenceDiagnosticStatus::Passed,
                CadenceDiagnosticSeverity::Info,
                false,
                "runtime_supervisor_reachable",
                format!(
                    "Runtime supervisor `{}` for project `{}` is reachable{}.",
                    run.run_id,
                    run.project_id,
                    if remembered {
                        " and tracked in memory"
                    } else {
                        ""
                    }
                ),
                None,
            )
        }
        RuntimeRunStatus::Starting | RuntimeRunStatus::Running | RuntimeRunStatus::Stale => (
            CadenceDiagnosticStatus::Warning,
            CadenceDiagnosticSeverity::Warning,
            true,
            "runtime_supervisor_unreachable",
            format!(
                "Runtime supervisor `{}` for project `{}` is not reachable.",
                run.run_id, run.project_id
            ),
            Some("Reconnect, stop, or restart the supervised run from the Agent tab.".into()),
        ),
        RuntimeRunStatus::Stopped => (
            CadenceDiagnosticStatus::Skipped,
            CadenceDiagnosticSeverity::Info,
            false,
            "runtime_supervisor_stopped",
            format!(
                "Runtime supervisor `{}` for project `{}` is stopped.",
                run.run_id, run.project_id
            ),
            Some("Start a new supervised run when live runtime diagnostics are needed.".into()),
        ),
        RuntimeRunStatus::Failed => (
            CadenceDiagnosticStatus::Failed,
            CadenceDiagnosticSeverity::Error,
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
                        "Runtime supervisor `{}` for project `{}` failed.",
                        run.run_id, run.project_id
                    )
                }),
            Some("Inspect the final runtime checkpoint, repair the provider or command environment, then start a new supervised run.".into()),
        ),
    };

    push_check(
        &mut checks.runtime_supervisor_checks,
        CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
            subject: CadenceDiagnosticSubject::RuntimeSupervisor,
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
    checks: &mut Vec<CadenceDiagnosticCheck>,
) -> usize {
    let routes = match project_store::load_notification_routes(repo_root, project_id) {
        Ok(routes) => routes,
        Err(error) => {
            push_check(
                checks,
                command_error_check(
                    CadenceDiagnosticSubject::SettingsDependency,
                    "notification_routes_load_failed",
                    format!(
                        "Cadence could not load notification routes for project `{project_id}`."
                    ),
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
                    CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                        subject: CadenceDiagnosticSubject::SettingsDependency,
                        status: CadenceDiagnosticStatus::Failed,
                        severity: CadenceDiagnosticSeverity::Error,
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
                CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                    subject: CadenceDiagnosticSubject::SettingsDependency,
                    status: CadenceDiagnosticStatus::Passed,
                    severity: CadenceDiagnosticSeverity::Info,
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
                CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                    subject: CadenceDiagnosticSubject::SettingsDependency,
                    status: CadenceDiagnosticStatus::Failed,
                    severity: CadenceDiagnosticSeverity::Error,
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
    checks: &mut Vec<CadenceDiagnosticCheck>,
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

#[derive(Debug, Copy, Clone)]
enum PathExpectation {
    DirectoryMayBeCreated,
    OptionalFile,
}

fn push_path_check(
    checks: &mut Vec<CadenceDiagnosticCheck>,
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
                    CadenceDiagnosticSubject::SettingsDependency,
                    format!("settings_path_{id}_unavailable"),
                    format!("Cadence could not resolve the {label} path."),
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
                    CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                        subject: CadenceDiagnosticSubject::SettingsDependency,
                        status: CadenceDiagnosticStatus::Passed,
                        severity: CadenceDiagnosticSeverity::Info,
                        retryable: false,
                        code: format!("settings_path_{id}_ready"),
                        message: format!(
                            "Cadence resolved the {label} path at {}.",
                            path.display()
                        ),
                        affected_profile_id: None,
                        affected_provider_id: None,
                        endpoint: None,
                        remediation: None,
                    }),
                );
            } else {
                push_check(
                    checks,
                    CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                        subject: CadenceDiagnosticSubject::SettingsDependency,
                        status: CadenceDiagnosticStatus::Failed,
                        severity: CadenceDiagnosticSeverity::Error,
                        retryable: false,
                        code: format!("settings_path_{id}_not_directory"),
                        message: format!(
                            "Cadence resolved the {label} path at {}, but it is not a directory.",
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
                    CadenceDiagnosticCheck::skipped(
                        CadenceDiagnosticSubject::SettingsDependency,
                        format!("settings_path_{id}_missing"),
                        format!("The {label} file does not exist yet at {}.", path.display()),
                        Some(
                            "Cadence will create this file when the related feature is configured."
                                .into(),
                        ),
                    ),
                );
                return;
            }

            if path.is_file() {
                push_check(
                    checks,
                    CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                        subject: CadenceDiagnosticSubject::SettingsDependency,
                        status: CadenceDiagnosticStatus::Passed,
                        severity: CadenceDiagnosticSeverity::Info,
                        retryable: false,
                        code: format!("settings_path_{id}_ready"),
                        message: format!("Cadence can see the {label} file at {}.", path.display()),
                        affected_profile_id: None,
                        affected_provider_id: None,
                        endpoint: None,
                        remediation: None,
                    }),
                );
            } else {
                push_check(
                    checks,
                    CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                        subject: CadenceDiagnosticSubject::SettingsDependency,
                        status: CadenceDiagnosticStatus::Failed,
                        severity: CadenceDiagnosticSeverity::Error,
                        retryable: false,
                        code: format!("settings_path_{id}_not_file"),
                        message: format!(
                            "Cadence expected the {label} path at {} to be a file.",
                            path.display()
                        ),
                        affected_profile_id: None,
                        affected_provider_id: None,
                        endpoint: None,
                        remediation: Some(
                            "Move the blocking directory or let Cadence recreate the settings file.".into(),
                        ),
                    }),
                );
            }
        }
    }
}

fn command_error_check(
    subject: CadenceDiagnosticSubject,
    code: impl Into<String>,
    message: impl Into<String>,
    error: CommandError,
    remediation: impl Into<String>,
) -> CommandResult<CadenceDiagnosticCheck> {
    CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
        subject,
        status: CadenceDiagnosticStatus::Failed,
        severity: CadenceDiagnosticSeverity::Error,
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

fn push_check(
    checks: &mut Vec<CadenceDiagnosticCheck>,
    result: CommandResult<CadenceDiagnosticCheck>,
) {
    match result {
        Ok(check) => checks.push(check),
        Err(error) => {
            let fallback = CadenceDiagnosticCheck::new(CadenceDiagnosticCheckInput {
                subject: CadenceDiagnosticSubject::SettingsDependency,
                status: CadenceDiagnosticStatus::Failed,
                severity: CadenceDiagnosticSeverity::Error,
                retryable: error.retryable,
                code: "doctor_report_check_projection_failed".into(),
                message: format!(
                    "Cadence could not project one diagnostic check: [{}] {}",
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
