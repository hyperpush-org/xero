use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::{ensure_openai_profile_target, start_provider_auth_flow, AuthFlowError},
    commands::{
        validate_non_empty, CommandResult, RuntimeDiagnosticDto, RuntimeSessionDto,
        StartOpenAiLoginRequestDto,
    },
    runtime::openai_codex_provider,
    state::DesktopState,
};

use super::{
    get_runtime_session::reconcile_runtime_session,
    runtime_support::{
        command_error_from_auth, default_runtime_session, emit_runtime_updated,
        load_runtime_session_status, persist_runtime_session, resolve_project_root,
    },
};

#[tauri::command]
pub fn start_openai_login<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StartOpenAiLoginRequestDto,
) -> CommandResult<RuntimeSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.profile_id, "profileId")?;

    let provider = openai_codex_provider();
    ensure_openai_profile_target(
        &app,
        state.inner(),
        &request.profile_id,
        crate::commands::RuntimeAuthPhase::Starting,
        "start OpenAI login",
    )
    .map_err(command_error_from_auth)?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let current = load_runtime_session_status(state.inner(), &repo_root, &request.project_id)?;
    let current = reconcile_runtime_session(&app, state.inner(), &repo_root, current)?;

    if is_login_in_progress(&current) {
        if current.provider_id != provider.provider_id {
            return Err(command_error_from_auth(AuthFlowError::terminal(
                "auth_flow_provider_mismatch",
                current.phase.clone(),
                format!(
                    "Cadence already has an in-flight `{}` login for this project. Finish or clear it before starting `{}`.",
                    current.provider_id, provider.provider_id
                ),
            )));
        }

        let in_flight_profile_id = current
            .flow_id
            .as_deref()
            .and_then(|flow_id| state.inner().active_auth_flows().snapshot(flow_id))
            .map(|snapshot| snapshot.profile_id);
        if let Some(in_flight_profile_id) = in_flight_profile_id {
            if in_flight_profile_id != request.profile_id {
                let error = AuthFlowError::terminal(
                    "auth_flow_profile_mismatch",
                    current.phase.clone(),
                    format!(
                        "Cadence already has an in-flight `{}` login for provider profile `{}` on this project. Finish or cancel it before starting login for `{}`.",
                        provider.provider_id, in_flight_profile_id, request.profile_id
                    ),
                );
                let failed = runtime_session_with_auth_error(&current, &error);
                let persisted = persist_runtime_session(&repo_root, &failed)?;
                emit_runtime_updated(&app, &persisted)?;
                return Err(command_error_from_auth(error));
            }
        }

        return Ok(current);
    }

    let started = match start_provider_auth_flow(
        state.inner(),
        provider.provider,
        &request.project_id,
        &request.profile_id,
        request.originator.as_deref(),
    ) {
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
        runtime_kind: provider.runtime_kind.into(),
        provider_id: started.provider_id,
        flow_id: Some(started.flow_id.clone()),
        session_id: None,
        account_id: None,
        phase: started.phase,
        callback_bound: Some(started.callback_bound),
        authorization_url: Some(started.authorization_url),
        redirect_uri: Some(started.redirect_uri),
        last_error_code: started.last_error_code,
        last_error: None,
        updated_at: started.updated_at,
    };

    persist_runtime_session(&repo_root, &initial)?;
    let runtime = load_runtime_session_status(state.inner(), &repo_root, &request.project_id)?;
    emit_runtime_updated(&app, &runtime)?;
    Ok(runtime)
}

fn runtime_session_with_auth_error(
    runtime: &RuntimeSessionDto,
    error: &AuthFlowError,
) -> RuntimeSessionDto {
    RuntimeSessionDto {
        last_error_code: Some(error.code.clone()),
        last_error: Some(RuntimeDiagnosticDto {
            code: error.code.clone(),
            message: error.message.clone(),
            retryable: error.retryable,
        }),
        updated_at: crate::auth::now_timestamp(),
        ..runtime.clone()
    }
}

fn is_login_in_progress(runtime: &RuntimeSessionDto) -> bool {
    runtime.flow_id.is_some()
        && matches!(
            runtime.phase,
            crate::commands::RuntimeAuthPhase::Starting
                | crate::commands::RuntimeAuthPhase::AwaitingBrowserCallback
                | crate::commands::RuntimeAuthPhase::AwaitingManualInput
                | crate::commands::RuntimeAuthPhase::ExchangingCode
                | crate::commands::RuntimeAuthPhase::Refreshing
        )
}
