use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::remove_openai_codex_session,
    commands::{
        validate_non_empty, CommandResult, ProjectIdRequestDto, RuntimeAuthPhase, RuntimeSessionDto,
    },
    state::DesktopState,
};

use super::runtime_support::{
    emit_runtime_updated, load_runtime_session_status, persist_runtime_session,
    resolve_project_root, OPENAI_RUNTIME_KIND,
};

#[tauri::command]
pub fn logout_runtime_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<RuntimeSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let current = load_runtime_session_status(state.inner(), &repo_root, &request.project_id)?;

    if let Some(account_id) = current.account_id.as_deref() {
        let auth_store_path = match state.auth_store_file(&app) {
            Ok(path) => path,
            Err(error) => {
                return Err(crate::commands::CommandError::new(
                    error.code,
                    if error.retryable {
                        crate::commands::CommandErrorClass::Retryable
                    } else {
                        crate::commands::CommandErrorClass::UserFixable
                    },
                    error.message,
                    error.retryable,
                ))
            }
        };

        if let Err(error) = remove_openai_codex_session(&auth_store_path, account_id) {
            return Err(crate::commands::CommandError::new(
                error.code,
                if error.retryable {
                    crate::commands::CommandErrorClass::Retryable
                } else {
                    crate::commands::CommandErrorClass::UserFixable
                },
                error.message,
                error.retryable,
            ));
        }
    }

    let signed_out = RuntimeSessionDto {
        project_id: request.project_id,
        runtime_kind: OPENAI_RUNTIME_KIND.into(),
        provider_id: crate::auth::OPENAI_CODEX_PROVIDER_ID.into(),
        flow_id: None,
        session_id: None,
        account_id: current.account_id,
        phase: RuntimeAuthPhase::Idle,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: None,
        last_error: None,
        updated_at: crate::auth::now_timestamp(),
    };

    let persisted = persist_runtime_session(&repo_root, &signed_out)?;
    emit_runtime_updated(&app, &persisted)?;
    Ok(persisted)
}
