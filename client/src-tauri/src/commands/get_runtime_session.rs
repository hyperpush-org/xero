use std::path::Path;

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        get_runtime_settings::{
            runtime_settings_snapshot_from_provider_profiles, RuntimeSettingsSnapshot,
        },
        provider_profiles::load_provider_profiles_snapshot,
        validate_non_empty, CommandError, CommandResult, ProjectIdRequestDto, RuntimeAuthPhase,
        RuntimeDiagnosticDto, RuntimeSessionDto,
    },
    provider_profiles::ProviderProfilesSnapshot,
    runtime::{
        reconcile_provider_runtime_session, resolve_runtime_provider_identity,
        ResolvedRuntimeProvider, RuntimeProviderReconcileOutcome,
    },
    state::DesktopState,
};

use super::runtime_support::{
    emit_runtime_updated, load_runtime_session_status, persist_runtime_session,
    resolve_project_root,
};

#[derive(Debug, Clone)]
pub(crate) struct RuntimeProviderSelection {
    pub provider: ResolvedRuntimeProvider,
    pub settings: RuntimeSettingsSnapshot,
    pub provider_profiles: ProviderProfilesSnapshot,
}

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
    let original = runtime.clone();
    let (runtime, selection) =
        match prepare_runtime_session_for_selected_provider(app, state, runtime) {
            Ok(prepared) => prepared,
            Err(updated) => {
                let persisted = persist_runtime_session(repo_root, &updated)?;
                emit_runtime_updated(app, &persisted)?;
                return Ok(persisted);
            }
        };

    reconcile_prepared_runtime_session(app, state, repo_root, original, runtime, selection)
}

#[allow(clippy::result_large_err)]
pub(crate) fn prepare_runtime_session_for_selected_provider<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    runtime: RuntimeSessionDto,
) -> Result<(RuntimeSessionDto, RuntimeProviderSelection), RuntimeSessionDto> {
    let provider_profiles = match load_provider_profiles_snapshot(app, state) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let error = normalize_runtime_provider_selection_error(error);
            return Err(signed_out_runtime(
                runtime,
                &error.code,
                &error.message,
                error.retryable,
            ));
        }
    };

    let settings = match runtime_settings_snapshot_from_provider_profiles(&provider_profiles) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return Err(signed_out_runtime(
                runtime,
                &error.code,
                &error.message,
                error.retryable,
            ));
        }
    };

    let provider = match resolve_runtime_provider_identity(
        Some(settings.settings.provider_id.as_str()),
        Some(settings.settings.provider_id.as_str()),
    ) {
        Ok(provider) => provider,
        Err(diagnostic) => {
            return Err(signed_out_runtime(
                runtime,
                &diagnostic.code,
                &diagnostic.message,
                diagnostic.retryable,
            ));
        }
    };

    Ok((
        runtime_with_selected_provider(runtime, provider),
        RuntimeProviderSelection {
            provider,
            settings,
            provider_profiles,
        },
    ))
}

pub(crate) fn reconcile_prepared_runtime_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    original: RuntimeSessionDto,
    runtime: RuntimeSessionDto,
    selection: RuntimeProviderSelection,
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
                message: format!(
                    "Cadence no longer has the in-memory {} login flow for this project. Start login again.",
                    runtime_provider_label(&runtime)
                ),
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
        if runtime != original {
            let persisted = persist_runtime_session(repo_root, &runtime)?;
            emit_runtime_updated(app, &persisted)?;
            return Ok(persisted);
        }
        return Ok(runtime);
    }

    match reconcile_provider_runtime_session(
        app,
        state,
        selection.provider,
        runtime.account_id.as_deref(),
        runtime.session_id.as_deref(),
        Some(&selection.settings),
        Some(&selection.provider_profiles),
    ) {
        Ok(RuntimeProviderReconcileOutcome::Authenticated(_binding)) => {
            if runtime != original {
                let persisted = persist_runtime_session(repo_root, &runtime)?;
                emit_runtime_updated(app, &persisted)?;
                return Ok(persisted);
            }
            Ok(runtime)
        }
        Ok(RuntimeProviderReconcileOutcome::SignedOut(diagnostic)) => {
            let updated = signed_out_runtime(
                runtime,
                &diagnostic.code,
                &diagnostic.message,
                diagnostic.retryable,
            );
            let persisted = persist_runtime_session(repo_root, &updated)?;
            emit_runtime_updated(app, &persisted)?;
            Ok(persisted)
        }
        Err(error) => {
            let updated = signed_out_runtime(runtime, &error.code, &error.message, error.retryable);
            let persisted = persist_runtime_session(repo_root, &updated)?;
            emit_runtime_updated(app, &persisted)?;
            Ok(persisted)
        }
    }
}

fn normalize_runtime_provider_selection_error(error: CommandError) -> CommandError {
    const MIGRATION_PREFIX: &str = "provider_profiles_migration_";
    const AUTH_STORE_PREFIX: &str = "auth_store_";

    match error.code.strip_prefix(MIGRATION_PREFIX) {
        Some(stripped) if stripped.starts_with(AUTH_STORE_PREFIX) => CommandError::new(
            stripped.to_owned(),
            error.class,
            error.message,
            error.retryable,
        ),
        _ => error,
    }
}

fn runtime_with_selected_provider(
    runtime: RuntimeSessionDto,
    provider: ResolvedRuntimeProvider,
) -> RuntimeSessionDto {
    if runtime.provider_id == provider.provider_id && runtime.runtime_kind == provider.runtime_kind
    {
        return runtime;
    }

    RuntimeSessionDto {
        runtime_kind: provider.runtime_kind.into(),
        provider_id: provider.provider_id.into(),
        flow_id: None,
        session_id: None,
        account_id: None,
        phase: RuntimeAuthPhase::Idle,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: None,
        last_error: None,
        updated_at: crate::auth::now_timestamp(),
        ..runtime
    }
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

fn runtime_provider_label(runtime: &RuntimeSessionDto) -> String {
    resolve_runtime_provider_identity(
        Some(runtime.provider_id.as_str()),
        Some(runtime.runtime_kind.as_str()),
    )
    .map(|provider| provider.provider_id.into())
    .unwrap_or_else(|_| {
        let provider_id = runtime.provider_id.trim();
        if provider_id.is_empty() {
            let runtime_kind = runtime.runtime_kind.trim();
            if runtime_kind.is_empty() {
                "runtime".into()
            } else {
                runtime_kind.into()
            }
        } else {
            provider_id.into()
        }
    })
}
