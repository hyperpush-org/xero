//! Generic app-scoped provider OAuth login entry point. Provider credentials are
//! app-local, so this command intentionally does not require or mutate a project.

use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::{ensure_openai_profile_target, start_provider_auth_flow},
    commands::{
        validate_non_empty, CommandError, CommandResult, ProviderAuthSessionDto,
        StartOAuthLoginRequestDto,
    },
    provider_credentials::OPENAI_CODEX_DEFAULT_PROFILE_ID,
    runtime::{openai_codex_provider, OPENAI_CODEX_PROVIDER_ID},
    state::DesktopState,
};

use super::runtime_support::command_error_from_auth;

pub(crate) const PROVIDER_CREDENTIAL_OAUTH_SCOPE_ID: &str = "app-provider-credentials";

#[tauri::command]
pub fn start_oauth_login<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StartOAuthLoginRequestDto,
) -> CommandResult<ProviderAuthSessionDto> {
    validate_non_empty(&request.provider_id, "providerId")?;

    if request.provider_id != OPENAI_CODEX_PROVIDER_ID {
        return Err(CommandError::user_fixable(
            "oauth_login_provider_unsupported",
            format!(
                "Cadence does not support browser-based OAuth for provider `{}`. Only `{}` is wired today.",
                request.provider_id, OPENAI_CODEX_PROVIDER_ID
            ),
        ));
    }

    let provider = openai_codex_provider();
    ensure_openai_profile_target(
        &app,
        state.inner(),
        OPENAI_CODEX_DEFAULT_PROFILE_ID,
        crate::commands::RuntimeAuthPhase::Starting,
        "start OpenAI login",
    )
    .map_err(command_error_from_auth)?;

    let started = start_provider_auth_flow(
        state.inner(),
        provider.provider,
        PROVIDER_CREDENTIAL_OAUTH_SCOPE_ID,
        OPENAI_CODEX_DEFAULT_PROFILE_ID,
        request.originator.as_deref(),
    )
    .map_err(command_error_from_auth)?;

    Ok(ProviderAuthSessionDto {
        runtime_kind: provider.runtime_kind.into(),
        provider_id: started.provider_id,
        flow_id: Some(started.flow_id),
        session_id: None,
        account_id: None,
        phase: started.phase,
        callback_bound: Some(started.callback_bound),
        authorization_url: Some(started.authorization_url),
        redirect_uri: Some(started.redirect_uri),
        last_error_code: started.last_error_code.clone(),
        last_error: None,
        updated_at: started.updated_at,
    })
}
