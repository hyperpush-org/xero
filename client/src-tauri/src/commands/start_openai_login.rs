//! OpenAI-specific compatibility command. It is app-scoped like
//! `start_oauth_login`; OpenAI provider credentials are never project-scoped.

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        CommandResult, ProviderAuthSessionDto, StartOAuthLoginRequestDto,
        StartOpenAiLoginRequestDto,
    },
    runtime::OPENAI_CODEX_PROVIDER_ID,
    state::DesktopState,
};

use super::start_oauth_login::start_oauth_login;

#[tauri::command]
pub fn start_openai_login<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StartOpenAiLoginRequestDto,
) -> CommandResult<ProviderAuthSessionDto> {
    start_oauth_login(
        app,
        state,
        StartOAuthLoginRequestDto {
            provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
            originator: request.originator,
        },
    )
}
