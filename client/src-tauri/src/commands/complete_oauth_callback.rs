//! Generic app-scoped provider OAuth callback completion command.

use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::complete_provider_auth_flow,
    commands::{
        validate_non_empty, CommandError, CommandResult, CompleteOAuthCallbackRequestDto,
        ProviderAuthSessionDto,
    },
    provider_credentials::OPENAI_CODEX_DEFAULT_PROFILE_ID,
    runtime::{openai_codex_provider, OPENAI_CODEX_PROVIDER_ID},
    state::DesktopState,
};

use super::{
    runtime_support::command_error_from_auth, start_oauth_login::PROVIDER_CREDENTIAL_OAUTH_SCOPE_ID,
};

#[tauri::command]
pub fn complete_oauth_callback<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CompleteOAuthCallbackRequestDto,
) -> CommandResult<ProviderAuthSessionDto> {
    validate_non_empty(&request.provider_id, "providerId")?;
    validate_non_empty(&request.flow_id, "flowId")?;

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
    let session = complete_provider_auth_flow(
        &app,
        state.inner(),
        provider.provider,
        PROVIDER_CREDENTIAL_OAUTH_SCOPE_ID,
        &request.flow_id,
        OPENAI_CODEX_DEFAULT_PROFILE_ID,
        request.manual_input.as_deref(),
    )
    .map_err(command_error_from_auth)?;

    Ok(ProviderAuthSessionDto {
        runtime_kind: provider.runtime_kind.into(),
        provider_id: session.provider_id,
        flow_id: None,
        session_id: Some(session.session_id),
        account_id: Some(session.account_id),
        phase: crate::commands::RuntimeAuthPhase::Authenticated,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: None,
        last_error: None,
        updated_at: session.updated_at,
    })
}
