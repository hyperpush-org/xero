use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandResult, RuntimeAuthPhase, RuntimeDiagnosticDto,
        RuntimeSessionDto,
    },
    runtime::{bind_provider_runtime_session, RuntimeProviderBindOutcome},
    state::DesktopState,
};

use super::{
    get_runtime_session::{
        prepare_runtime_session_for_selected_provider, reconcile_prepared_runtime_session,
    },
    runtime_support::{
        emit_runtime_updated, load_runtime_session_status, persist_runtime_session,
        resolve_project_root, runtime_diagnostic_from_auth,
    },
};

#[tauri::command]
pub fn start_runtime_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: crate::commands::ProjectIdRequestDto,
) -> CommandResult<RuntimeSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let current = load_runtime_session_status(state.inner(), &repo_root, &request.project_id)?;
    let original = current.clone();
    let (current, selection) = match prepare_runtime_session_for_selected_provider(
        &app,
        state.inner(),
        current,
    ) {
        Ok(prepared) => prepared,
        Err(updated) => {
            let persisted = persist_runtime_session(&repo_root, &updated)?;
            emit_runtime_updated(&app, &persisted)?;
            return Ok(persisted);
        }
    };
    let current = reconcile_prepared_runtime_session(
        &app,
        state.inner(),
        &repo_root,
        original,
        current,
        selection.clone(),
    )?;

    if current.phase == RuntimeAuthPhase::Authenticated || is_login_in_progress(&current.phase) {
        return Ok(current);
    }

    match bind_provider_runtime_session(
        &app,
        state.inner(),
        selection.provider,
        current.account_id.as_deref(),
        Some(&selection.settings),
    ) {
        Ok(RuntimeProviderBindOutcome::Ready(binding)) => {
            let authenticated = runtime_from_provider(
                &request.project_id,
                binding.provider,
                Some(binding.account_id),
                Some(binding.session_id),
                RuntimeAuthPhase::Authenticated,
                None,
                binding.updated_at,
            );
            let persisted = persist_runtime_session(&repo_root, &authenticated)?;
            emit_runtime_updated(&app, &persisted)?;
            Ok(persisted)
        }
        Ok(RuntimeProviderBindOutcome::RefreshRequired(binding)) => {
            let refreshing = runtime_from_provider(
                &request.project_id,
                binding.provider,
                Some(binding.account_id.clone()),
                None,
                RuntimeAuthPhase::Refreshing,
                None,
                crate::auth::now_timestamp(),
            );
            let refreshing = persist_runtime_session(&repo_root, &refreshing)?;
            emit_runtime_updated(&app, &refreshing)?;

            match crate::runtime::refresh_provider_runtime_session(
                &app,
                state.inner(),
                selection.provider,
                &binding.account_id,
            ) {
                Ok(binding) => {
                    let authenticated = runtime_from_provider(
                        &request.project_id,
                        binding.provider,
                        Some(binding.account_id),
                        Some(binding.session_id),
                        RuntimeAuthPhase::Authenticated,
                        None,
                        binding.updated_at,
                    );
                    let persisted = persist_runtime_session(&repo_root, &authenticated)?;
                    emit_runtime_updated(&app, &persisted)?;
                    Ok(persisted)
                }
                Err(error) => {
                    let diagnostic = RuntimeDiagnosticDto {
                        code: error.code.clone(),
                        message: error.message.clone(),
                        retryable: error.retryable,
                    };
                    let failed = runtime_from_provider(
                        &request.project_id,
                        selection.provider,
                        Some(binding.account_id),
                        None,
                        if error.retryable {
                            RuntimeAuthPhase::Refreshing
                        } else {
                            RuntimeAuthPhase::Failed
                        },
                        Some(diagnostic),
                        crate::auth::now_timestamp(),
                    );
                    let persisted = persist_runtime_session(&repo_root, &failed)?;
                    emit_runtime_updated(&app, &persisted)?;
                    Ok(persisted)
                }
            }
        }
        Ok(RuntimeProviderBindOutcome::SignedOut(diagnostic)) => {
            let updated = runtime_with_phase(
                current,
                RuntimeAuthPhase::Idle,
                Some(runtime_diagnostic_from_auth(diagnostic)),
            );
            let persisted = persist_runtime_session(&repo_root, &updated)?;
            emit_runtime_updated(&app, &persisted)?;
            Ok(persisted)
        }
        Err(error) => {
            let updated = runtime_with_phase(
                current,
                RuntimeAuthPhase::Idle,
                Some(RuntimeDiagnosticDto {
                    code: error.code.clone(),
                    message: error.message,
                    retryable: error.retryable,
                }),
            );
            let persisted = persist_runtime_session(&repo_root, &updated)?;
            emit_runtime_updated(&app, &persisted)?;
            Ok(persisted)
        }
    }
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

fn runtime_from_provider(
    project_id: &str,
    provider: crate::runtime::ResolvedRuntimeProvider,
    account_id: Option<String>,
    session_id: Option<String>,
    phase: RuntimeAuthPhase,
    diagnostic: Option<RuntimeDiagnosticDto>,
    updated_at: String,
) -> RuntimeSessionDto {
    RuntimeSessionDto {
        project_id: project_id.into(),
        runtime_kind: provider.runtime_kind.into(),
        provider_id: provider.provider_id.into(),
        flow_id: None,
        session_id,
        account_id,
        phase,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: diagnostic.as_ref().map(|item| item.code.clone()),
        last_error: diagnostic,
        updated_at,
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
