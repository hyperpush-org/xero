use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::load_openai_codex_session_for_profile_link,
    commands::{
        provider_credentials::load_provider_credentials_view,
        provider_model_catalog::map_provider_model_catalog, CheckProviderProfileRequestDto,
        CommandError, CommandResult, ProviderProfileDiagnosticsDto,
    },
    provider_credentials::ProviderCredentialProfile,
    provider_models::load_provider_model_catalog,
    runtime::{
        provider_capability_diagnostics, provider_model_catalog_diagnostic,
        provider_validation_diagnostics, XeroDiagnosticCheck, XeroDiagnosticCheckInput,
        XeroDiagnosticSeverity, XeroDiagnosticStatus, XeroDiagnosticSubject,
        OPENAI_CODEX_PROVIDER_ID,
    },
    state::DesktopState,
};

#[tauri::command]
pub fn check_provider_profile<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CheckProviderProfileRequestDto,
) -> CommandResult<ProviderProfileDiagnosticsDto> {
    let profile_id = request.profile_id.trim();
    if profile_id.is_empty() {
        return Err(CommandError::invalid_request("profileId"));
    }

    let snapshot = load_provider_credentials_view(&app, state.inner())?;
    let profile = snapshot
        .profile(profile_id)
        .or_else(|| {
            snapshot
                .profiles()
                .iter()
                .find(|profile| profile.provider_id == profile_id)
        })
        .cloned()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "provider_not_found",
                format!("Xero could not find provider `{profile_id}`."),
            )
        })?;

    let mut validation_checks = provider_validation_diagnostics(&snapshot, &profile)?;
    if let Some(check) = openai_codex_session_check(&app, state.inner(), &profile)? {
        validation_checks.push(check);
    }

    let mut reachability_checks = Vec::new();
    let mut capability_checks = Vec::new();
    let mut model_catalog = None;
    if request.include_network {
        match load_provider_model_catalog(&app, state.inner(), profile_id, true) {
            Ok(catalog) => {
                reachability_checks.push(provider_model_catalog_diagnostic(&catalog)?);
                capability_checks.extend(provider_capability_diagnostics(
                    &catalog,
                    request.model_id.as_deref(),
                )?);
                model_catalog = Some(map_provider_model_catalog(catalog));
            }
            Err(error) => {
                reachability_checks.push(command_error_to_model_catalog_check(&profile, error)?);
            }
        }
    }

    Ok(ProviderProfileDiagnosticsDto {
        checked_at: crate::auth::now_timestamp(),
        profile_id: profile.profile_id,
        provider_id: profile.provider_id,
        validation_checks,
        reachability_checks,
        capability_checks,
        model_catalog,
    })
}

fn openai_codex_session_check<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile: &ProviderCredentialProfile,
) -> CommandResult<Option<XeroDiagnosticCheck>> {
    if profile.provider_id != OPENAI_CODEX_PROVIDER_ID {
        return Ok(None);
    }

    let Some(link) = profile.credential_link.as_ref() else {
        return Ok(None);
    };

    let auth_store_path = state.global_db_path(app)?;

    match load_openai_codex_session_for_profile_link(&auth_store_path, link) {
        Ok(Some(session)) => XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::ProviderCredential,
            status: XeroDiagnosticStatus::Passed,
            severity: XeroDiagnosticSeverity::Info,
            retryable: false,
            code: "provider_openai_session_ready".into(),
            message: format!(
                "OpenAI Codex has a matching app-local auth session for account `{}`.",
                session.account_id
            ),
            affected_profile_id: Some(profile.profile_id.clone()),
            affected_provider_id: Some(profile.provider_id.clone()),
            endpoint: None,
            remediation: None,
        })
        .map(Some),
        Ok(None) => XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::ProviderCredential,
            status: XeroDiagnosticStatus::Failed,
            severity: XeroDiagnosticSeverity::Error,
            retryable: false,
            code: "provider_openai_session_missing".into(),
            message: "OpenAI Codex points at an auth session that is no longer present.".into(),
            affected_profile_id: Some(profile.profile_id.clone()),
            affected_provider_id: Some(profile.provider_id.clone()),
            endpoint: None,
            remediation: Some(
                "Sign in to OpenAI Codex again from Providers settings to repair this provider."
                    .into(),
            ),
        })
        .map(Some),
        Err(error) => XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
            subject: XeroDiagnosticSubject::ProviderCredential,
            status: XeroDiagnosticStatus::Failed,
            severity: XeroDiagnosticSeverity::Error,
            retryable: error.retryable,
            code: error.code,
            message: error.message,
            affected_profile_id: Some(profile.profile_id.clone()),
            affected_provider_id: Some(profile.provider_id.clone()),
            endpoint: None,
            remediation: Some(
                "Reconnect or resave OpenAI Codex so Xero can rebuild the app-local auth link."
                    .into(),
            ),
        })
        .map(Some),
    }
}

fn command_error_to_model_catalog_check(
    profile: &ProviderCredentialProfile,
    error: CommandError,
) -> CommandResult<XeroDiagnosticCheck> {
    XeroDiagnosticCheck::new(XeroDiagnosticCheckInput {
        subject: XeroDiagnosticSubject::ModelCatalog,
        status: XeroDiagnosticStatus::Failed,
        severity: XeroDiagnosticSeverity::Error,
        retryable: error.retryable,
        code: error.code,
        message: error.message,
        affected_profile_id: Some(profile.profile_id.clone()),
        affected_provider_id: Some(profile.provider_id.clone()),
        endpoint: None,
        remediation: Some(
            "Repair the provider credentials or endpoint metadata before checking the connection again."
                .into(),
        ),
    })
}
