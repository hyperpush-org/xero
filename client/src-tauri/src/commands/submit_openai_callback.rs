//! OpenAI-specific compatibility command. It completes the app-scoped provider
//! OAuth flow and never reads or writes project runtime-session state.

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        CommandResult, CompleteOAuthCallbackRequestDto, ProviderAuthSessionDto,
        SubmitOpenAiCallbackRequestDto,
    },
    runtime::OPENAI_CODEX_PROVIDER_ID,
    state::DesktopState,
};

use super::complete_oauth_callback::complete_oauth_callback;

#[tauri::command]
pub fn submit_openai_callback<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SubmitOpenAiCallbackRequestDto,
) -> CommandResult<ProviderAuthSessionDto> {
    complete_oauth_callback(
        app,
        state,
        CompleteOAuthCallbackRequestDto {
            provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
            flow_id: request.flow_id,
            manual_input: request.manual_input,
        },
    )
}
