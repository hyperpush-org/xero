use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::{start_openai_codex_flow, OpenAiCodexAuthConfig},
    commands::{
        validate_non_empty, CommandResult, RuntimeDiagnosticDto, RuntimeSessionDto,
        StartOpenAiLoginRequestDto,
    },
    state::DesktopState,
};

use super::runtime_support::{
    command_error_from_auth, default_runtime_session, emit_runtime_updated,
    load_runtime_session_status, persist_runtime_session, resolve_project_root,
    OPENAI_RUNTIME_KIND,
};

#[tauri::command]
pub fn start_openai_login<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StartOpenAiLoginRequestDto,
) -> CommandResult<RuntimeSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let config = OpenAiCodexAuthConfig::for_platform();

    let started =
        match start_openai_codex_flow(state.inner(), config, request.originator.as_deref()) {
            Ok(started) => started,
            Err(error) => {
                let failed = RuntimeSessionDto {
                    phase: error.phase.clone(),
                    last_error_code: Some(error.code.clone()),
                    last_error: Some(RuntimeDiagnosticDto {
                        code: error.code.clone(),
                        message: error.message.clone(),
                        retryable: error.retryable,
                    }),
                    updated_at: crate::auth::now_timestamp(),
                    ..default_runtime_session(&request.project_id)
                };
                let persisted = persist_runtime_session(&repo_root, &failed)?;
                emit_runtime_updated(&app, &persisted)?;
                return Err(command_error_from_auth(error));
            }
        };

    let initial = RuntimeSessionDto {
        project_id: request.project_id.clone(),
        runtime_kind: OPENAI_RUNTIME_KIND.into(),
        provider_id: crate::auth::OPENAI_CODEX_PROVIDER_ID.into(),
        flow_id: Some(started.flow_id.clone()),
        session_id: None,
        account_id: None,
        phase: started.phase,
        callback_bound: Some(started.callback_bound),
        authorization_url: Some(started.authorization_url),
        redirect_uri: Some(started.redirect_uri),
        last_error_code: None,
        last_error: None,
        updated_at: started.updated_at,
    };

    persist_runtime_session(&repo_root, &initial)?;
    let runtime = load_runtime_session_status(state.inner(), &repo_root, &request.project_id)?;
    emit_runtime_updated(&app, &runtime)?;
    Ok(runtime)
}
