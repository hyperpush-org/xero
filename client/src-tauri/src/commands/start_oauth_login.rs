//! Generic per-provider OAuth login entry point introduced in Phase 2.2 of the
//! provider-layer refactor. Today only `openai_codex` actually browses out for
//! credentials; this command wraps the existing OpenAI Codex flow but accepts
//! a `provider_id` directly so callers no longer think in terms of provider
//! profiles.

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        start_openai_login::start_openai_login, validate_non_empty, CommandError, CommandResult,
        RuntimeSessionDto, StartOAuthLoginRequestDto, StartOpenAiLoginRequestDto,
    },
    runtime::OPENAI_CODEX_PROVIDER_ID,
    state::DesktopState,
};

#[tauri::command]
pub fn start_oauth_login<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StartOAuthLoginRequestDto,
) -> CommandResult<RuntimeSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;
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

    let profile_id = format!("{}-default", request.provider_id);
    start_openai_login(
        app,
        state,
        StartOpenAiLoginRequestDto {
            project_id: request.project_id,
            profile_id,
            originator: request.originator,
        },
    )
}
