//! Generic per-provider OAuth callback completion command introduced in
//! Phase 2.2 of the provider-layer refactor. Mirrors `start_oauth_login` —
//! today this is a thin wrapper over the existing OpenAI Codex callback.

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        submit_openai_callback::submit_openai_callback, validate_non_empty,
        CommandError, CommandResult, CompleteOAuthCallbackRequestDto, RuntimeSessionDto,
        SubmitOpenAiCallbackRequestDto,
    },
    runtime::OPENAI_CODEX_PROVIDER_ID,
    state::DesktopState,
};

#[tauri::command]
pub fn complete_oauth_callback<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CompleteOAuthCallbackRequestDto,
) -> CommandResult<RuntimeSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;
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

    let profile_id = format!("{}-default", request.provider_id);
    submit_openai_callback(
        app,
        state,
        SubmitOpenAiCallbackRequestDto {
            project_id: request.project_id,
            profile_id,
            flow_id: request.flow_id,
            manual_input: request.manual_input,
        },
    )
}
