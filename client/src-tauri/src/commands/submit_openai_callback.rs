use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::complete_provider_auth_flow,
    commands::{
        validate_non_empty, CommandResult, RuntimeDiagnosticDto, RuntimeSessionDto,
        SubmitOpenAiCallbackRequestDto,
    },
    runtime::openai_codex_provider,
    state::DesktopState,
};

use super::runtime_support::{
    command_error_from_auth, default_runtime_session, emit_runtime_updated,
    persist_runtime_session, resolve_project_root, runtime_diagnostic_from_auth,
};

#[tauri::command]
pub fn submit_openai_callback<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SubmitOpenAiCallbackRequestDto,
) -> CommandResult<RuntimeSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.profile_id, "profileId")?;
    validate_non_empty(&request.flow_id, "flowId")?;

    let provider = openai_codex_provider();
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;

    match complete_provider_auth_flow(
        &app,
        state.inner(),
        provider.provider,
        &request.project_id,
        &request.flow_id,
        &request.profile_id,
        request.manual_input.as_deref(),
    ) {
        Ok(session) => {
            let runtime = RuntimeSessionDto {
                project_id: request.project_id,
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
            };
            let persisted = persist_runtime_session(&repo_root, &runtime)?;
            emit_runtime_updated(&app, &persisted)?;
            Ok(persisted)
        }
        Err(error) => {
            if request.manual_input.is_none() && error.code == "authorization_code_pending" {
                return Err(command_error_from_auth(error));
            }

            let snapshot = state.inner().active_auth_flows().snapshot(&request.flow_id);
            let failed = if let Some(snapshot) = snapshot {
                let last_error = snapshot
                    .last_error
                    .map(runtime_diagnostic_from_auth)
                    .or_else(|| {
                        Some(RuntimeDiagnosticDto {
                            code: error.code.clone(),
                            message: error.message.clone(),
                            retryable: error.retryable,
                        })
                    });
                RuntimeSessionDto {
                    project_id: request.project_id.clone(),
                    runtime_kind: provider.runtime_kind.into(),
                    provider_id: snapshot.provider_id,
                    flow_id: Some(snapshot.flow_id),
                    session_id: snapshot.session_id,
                    account_id: snapshot.account_id,
                    phase: snapshot.phase,
                    callback_bound: Some(snapshot.callback_bound),
                    authorization_url: Some(snapshot.authorization_url),
                    redirect_uri: Some(snapshot.redirect_uri),
                    last_error_code: last_error
                        .as_ref()
                        .map(|diagnostic| diagnostic.code.clone()),
                    last_error,
                    updated_at: snapshot.updated_at,
                }
            } else {
                RuntimeSessionDto {
                    phase: error.phase.clone(),
                    flow_id: Some(request.flow_id.clone()),
                    last_error_code: Some(error.code.clone()),
                    last_error: Some(RuntimeDiagnosticDto {
                        code: error.code.clone(),
                        message: error.message.clone(),
                        retryable: error.retryable,
                    }),
                    updated_at: crate::auth::now_timestamp(),
                    ..default_runtime_session(&request.project_id)
                }
            };
            let persisted = persist_runtime_session(&repo_root, &failed)?;
            emit_runtime_updated(&app, &persisted)?;
            Err(command_error_from_auth(error))
        }
    }
}
