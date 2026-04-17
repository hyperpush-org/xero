use std::{path::Path, time::SystemTime};

use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::load_openai_codex_session,
    commands::{
        validate_non_empty, CommandResult, ProjectIdRequestDto, RuntimeAuthPhase,
        RuntimeDiagnosticDto, RuntimeSessionDto,
    },
    state::DesktopState,
};

use super::runtime_support::{
    emit_runtime_updated, load_runtime_session_status, persist_runtime_session,
    resolve_project_root,
};

#[tauri::command]
pub fn get_runtime_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<RuntimeSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let runtime = load_runtime_session_status(state.inner(), &repo_root, &request.project_id)?;
    reconcile_runtime_session(&app, state.inner(), &repo_root, runtime)
}

pub(crate) fn reconcile_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    runtime: RuntimeSessionDto,
) -> CommandResult<RuntimeSessionDto> {
    if is_transient_phase(&runtime.phase)
        && runtime.flow_id.is_some()
        && runtime.authorization_url.is_none()
        && runtime.redirect_uri.is_none()
    {
        let updated = RuntimeSessionDto {
            flow_id: None,
            phase: RuntimeAuthPhase::Failed,
            callback_bound: None,
            authorization_url: None,
            redirect_uri: None,
            last_error_code: Some("auth_flow_unavailable".into()),
            last_error: Some(RuntimeDiagnosticDto {
                code: "auth_flow_unavailable".into(),
                message:
                    "Cadence no longer has the in-memory OpenAI login flow for this project. Start login again."
                        .into(),
                retryable: false,
            }),
            updated_at: crate::auth::now_timestamp(),
            ..runtime
        };
        let persisted = persist_runtime_session(repo_root, &updated)?;
        emit_runtime_updated(app, &persisted)?;
        return Ok(persisted);
    }

    if runtime.phase != RuntimeAuthPhase::Authenticated {
        return Ok(runtime);
    }

    let Some(account_id) = runtime.account_id.clone() else {
        let updated = signed_out_runtime(
            runtime,
            "runtime_account_missing",
            "Cadence could not reconcile the runtime session because the stored account id was missing.",
            false,
        );
        let persisted = persist_runtime_session(repo_root, &updated)?;
        emit_runtime_updated(app, &persisted)?;
        return Ok(persisted);
    };

    let auth_store_path = match state.auth_store_file(app) {
        Ok(path) => path,
        Err(error) => {
            let updated = signed_out_runtime(runtime, &error.code, &error.message, error.retryable);
            let persisted = persist_runtime_session(repo_root, &updated)?;
            emit_runtime_updated(app, &persisted)?;
            return Ok(persisted);
        }
    };

    let stored_auth = match load_openai_codex_session(&auth_store_path, &account_id) {
        Ok(session) => session,
        Err(error) => {
            let updated = signed_out_runtime(runtime, &error.code, &error.message, error.retryable);
            let persisted = persist_runtime_session(repo_root, &updated)?;
            emit_runtime_updated(app, &persisted)?;
            return Ok(persisted);
        }
    };

    let Some(stored_auth) = stored_auth else {
        let updated = signed_out_runtime(
            runtime,
            "auth_session_not_found",
            &format!(
                "Cadence does not have an app-local OpenAI auth session for account `{account_id}`."
            ),
            false,
        );
        let persisted = persist_runtime_session(repo_root, &updated)?;
        emit_runtime_updated(app, &persisted)?;
        return Ok(persisted);
    };

    if stored_auth.expires_at <= current_unix_timestamp() {
        let updated = signed_out_runtime(
            runtime,
            "auth_session_expired",
            &format!(
                "The app-local OpenAI auth session for account `{account_id}` has expired. Sign in again or refresh the runtime session."
            ),
            false,
        );
        let persisted = persist_runtime_session(repo_root, &updated)?;
        emit_runtime_updated(app, &persisted)?;
        return Ok(persisted);
    }

    Ok(runtime)
}

fn signed_out_runtime(
    runtime: RuntimeSessionDto,
    code: &str,
    message: &str,
    retryable: bool,
) -> RuntimeSessionDto {
    RuntimeSessionDto {
        flow_id: None,
        session_id: None,
        phase: RuntimeAuthPhase::Idle,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: Some(code.into()),
        last_error: Some(RuntimeDiagnosticDto {
            code: code.into(),
            message: message.into(),
            retryable,
        }),
        updated_at: crate::auth::now_timestamp(),
        ..runtime
    }
}

fn is_transient_phase(phase: &RuntimeAuthPhase) -> bool {
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
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}
