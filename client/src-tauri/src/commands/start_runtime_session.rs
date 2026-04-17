use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::{
        load_latest_openai_codex_session, load_openai_codex_session, refresh_openai_codex_session,
        OPENAI_CODEX_PROVIDER_ID,
    },
    commands::{
        validate_non_empty, CommandResult, ProjectIdRequestDto, RuntimeAuthPhase,
        RuntimeDiagnosticDto, RuntimeSessionDto,
    },
    state::DesktopState,
};

use super::{
    get_runtime_session::reconcile_runtime_session,
    runtime_support::{
        emit_runtime_updated, load_runtime_session_status, persist_runtime_session,
        resolve_project_root, OPENAI_RUNTIME_KIND,
    },
};

#[tauri::command]
pub fn start_runtime_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<RuntimeSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let current = load_runtime_session_status(state.inner(), &repo_root, &request.project_id)?;
    let current = reconcile_runtime_session(&app, state.inner(), &repo_root, current)?;

    if current.phase == RuntimeAuthPhase::Authenticated || is_login_in_progress(&current.phase) {
        return Ok(current);
    }

    let auth_store_path = match state.auth_store_file(&app) {
        Ok(path) => path,
        Err(error) => {
            let updated = runtime_with_phase(
                current,
                RuntimeAuthPhase::Idle,
                Some(RuntimeDiagnosticDto {
                    code: error.code,
                    message: error.message,
                    retryable: error.retryable,
                }),
            );
            let persisted = persist_runtime_session(&repo_root, &updated)?;
            emit_runtime_updated(&app, &persisted)?;
            return Ok(persisted);
        }
    };

    let stored_auth = if let Some(account_id) = current.account_id.as_deref() {
        match load_openai_codex_session(&auth_store_path, account_id) {
            Ok(session) => session,
            Err(error) => {
                let updated = runtime_with_phase(
                    current,
                    RuntimeAuthPhase::Idle,
                    Some(RuntimeDiagnosticDto {
                        code: error.code,
                        message: error.message,
                        retryable: error.retryable,
                    }),
                );
                let persisted = persist_runtime_session(&repo_root, &updated)?;
                emit_runtime_updated(&app, &persisted)?;
                return Ok(persisted);
            }
        }
    } else {
        match load_latest_openai_codex_session(&auth_store_path) {
            Ok(session) => session,
            Err(error) => {
                let updated = runtime_with_phase(
                    current,
                    RuntimeAuthPhase::Idle,
                    Some(RuntimeDiagnosticDto {
                        code: error.code,
                        message: error.message,
                        retryable: error.retryable,
                    }),
                );
                let persisted = persist_runtime_session(&repo_root, &updated)?;
                emit_runtime_updated(&app, &persisted)?;
                return Ok(persisted);
            }
        }
    };

    let Some(stored_auth) = stored_auth else {
        let updated = runtime_with_phase(
            current,
            RuntimeAuthPhase::Idle,
            Some(RuntimeDiagnosticDto {
                code: "auth_session_not_found".into(),
                message: "Cadence does not have an app-local OpenAI auth session to bind to this project yet.".into(),
                retryable: false,
            }),
        );
        let persisted = persist_runtime_session(&repo_root, &updated)?;
        emit_runtime_updated(&app, &persisted)?;
        return Ok(persisted);
    };

    if stored_auth.expires_at <= current_unix_timestamp() {
        let refreshing = RuntimeSessionDto {
            account_id: Some(stored_auth.account_id.clone()),
            phase: RuntimeAuthPhase::Refreshing,
            last_error_code: None,
            last_error: None,
            updated_at: crate::auth::now_timestamp(),
            ..current.clone()
        };
        let refreshing = persist_runtime_session(&repo_root, &refreshing)?;
        emit_runtime_updated(&app, &refreshing)?;

        let config = crate::auth::OpenAiCodexAuthConfig::for_platform();
        match refresh_openai_codex_session(&app, state.inner(), &stored_auth.account_id, &config) {
            Ok(session) => {
                let authenticated = RuntimeSessionDto {
                    project_id: request.project_id,
                    runtime_kind: OPENAI_RUNTIME_KIND.into(),
                    provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
                    flow_id: None,
                    session_id: Some(session.session_id),
                    account_id: Some(session.account_id),
                    phase: RuntimeAuthPhase::Authenticated,
                    callback_bound: None,
                    authorization_url: None,
                    redirect_uri: None,
                    last_error_code: None,
                    last_error: None,
                    updated_at: session.updated_at,
                };
                let persisted = persist_runtime_session(&repo_root, &authenticated)?;
                emit_runtime_updated(&app, &persisted)?;
                return Ok(persisted);
            }
            Err(error) => {
                let failed = RuntimeSessionDto {
                    account_id: Some(stored_auth.account_id),
                    session_id: None,
                    phase: RuntimeAuthPhase::Failed,
                    last_error_code: Some(error.code.clone()),
                    last_error: Some(RuntimeDiagnosticDto {
                        code: error.code,
                        message: error.message,
                        retryable: error.retryable,
                    }),
                    updated_at: crate::auth::now_timestamp(),
                    ..current
                };
                let persisted = persist_runtime_session(&repo_root, &failed)?;
                emit_runtime_updated(&app, &persisted)?;
                return Ok(persisted);
            }
        }
    }

    let authenticated = RuntimeSessionDto {
        project_id: request.project_id,
        runtime_kind: OPENAI_RUNTIME_KIND.into(),
        provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
        flow_id: None,
        session_id: Some(stored_auth.session_id),
        account_id: Some(stored_auth.account_id),
        phase: RuntimeAuthPhase::Authenticated,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: None,
        last_error: None,
        updated_at: crate::auth::now_timestamp(),
    };
    let persisted = persist_runtime_session(&repo_root, &authenticated)?;
    emit_runtime_updated(&app, &persisted)?;
    Ok(persisted)
}

fn runtime_with_phase(
    runtime: RuntimeSessionDto,
    phase: RuntimeAuthPhase,
    diagnostic: Option<RuntimeDiagnosticDto>,
) -> RuntimeSessionDto {
    RuntimeSessionDto {
        flow_id: None,
        session_id: if phase == RuntimeAuthPhase::Authenticated {
            runtime.session_id.clone()
        } else {
            None
        },
        phase,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: diagnostic.as_ref().map(|item| item.code.clone()),
        last_error: diagnostic,
        updated_at: crate::auth::now_timestamp(),
        ..runtime
    }
}

fn is_login_in_progress(phase: &RuntimeAuthPhase) -> bool {
    matches!(
        phase,
        RuntimeAuthPhase::Starting
            | RuntimeAuthPhase::AwaitingBrowserCallback
            | RuntimeAuthPhase::AwaitingManualInput
            | RuntimeAuthPhase::ExchangingCode
            | RuntimeAuthPhase::Refreshing
    )
}

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}
