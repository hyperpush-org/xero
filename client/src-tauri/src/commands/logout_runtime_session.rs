use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandResult, ProjectIdRequestDto, RuntimeAuthPhase, RuntimeSessionDto,
    },
    runtime::logout_provider_runtime_session,
    state::DesktopState,
};

use super::{
    get_runtime_session::prepare_runtime_session_for_selected_provider,
    runtime_support::{
        command_error_from_auth, emit_runtime_updated, load_runtime_session_status,
        persist_runtime_session, resolve_project_root,
    },
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
    let (current, selection) =
        match prepare_runtime_session_for_selected_provider(&app, state.inner(), current, None) {
            Ok(prepared) => prepared,
            Err(updated) => {
                let persisted = persist_runtime_session(&repo_root, &updated)?;
                emit_runtime_updated(&app, &persisted)?;
                return Ok(persisted);
            }
        };

    let account_id = current
        .account_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("");
    if !account_id.is_empty()
        || selection.provider.provider == crate::runtime::RuntimeProvider::OpenAiCodex
    {
        if let Err(error) =
            logout_provider_runtime_session(&app, state.inner(), selection.provider, account_id)
        {
            return Err(command_error_from_auth(error));
        }
    }

    let signed_out = RuntimeSessionDto {
        project_id: request.project_id,
        runtime_kind: selection.provider.runtime_kind.into(),
        provider_id: selection.provider.provider_id.into(),
        flow_id: None,
        session_id: None,
        account_id: current
            .account_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned),
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
