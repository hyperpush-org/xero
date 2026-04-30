use std::path::Path;

use tauri::{AppHandle, Emitter, Runtime};

use crate::{
    auth::{AuthDiagnostic, AuthFlowError},
    commands::{
        CommandError, CommandErrorClass, CommandResult, RuntimeDiagnosticDto, RuntimeSessionDto,
        RuntimeUpdatedPayloadDto, RUNTIME_UPDATED_EVENT,
    },
    db::project_store::{self, RuntimeSessionDiagnosticRecord, RuntimeSessionRecord},
    runtime::default_runtime_provider,
    state::DesktopState,
};

pub(crate) fn default_runtime_session(project_id: &str) -> RuntimeSessionDto {
    let provider = default_runtime_provider();

    RuntimeSessionDto {
        project_id: project_id.into(),
        runtime_kind: provider.runtime_kind.into(),
        provider_id: provider.provider_id.into(),
        flow_id: None,
        session_id: None,
        account_id: None,
        phase: crate::commands::RuntimeAuthPhase::Idle,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: None,
        last_error: None,
        updated_at: crate::auth::now_timestamp(),
    }
}

pub(crate) fn load_runtime_session_status(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<RuntimeSessionDto> {
    let stored = project_store::load_runtime_session(repo_root, project_id)?;
    Ok(runtime_session_from_record(
        state,
        project_id,
        stored.as_ref(),
    ))
}

pub(crate) fn persist_runtime_session(
    repo_root: &Path,
    runtime: &RuntimeSessionDto,
) -> CommandResult<RuntimeSessionDto> {
    let record = RuntimeSessionRecord {
        project_id: runtime.project_id.clone(),
        runtime_kind: runtime.runtime_kind.clone(),
        provider_id: runtime.provider_id.clone(),
        flow_id: runtime.flow_id.clone(),
        session_id: runtime.session_id.clone(),
        account_id: runtime.account_id.clone(),
        auth_phase: runtime.phase.clone(),
        last_error: runtime
            .last_error
            .as_ref()
            .map(|error| RuntimeSessionDiagnosticRecord {
                code: error.code.clone(),
                message: error.message.clone(),
                retryable: error.retryable,
            }),
        updated_at: runtime.updated_at.clone(),
    };
    let persisted = project_store::upsert_runtime_session(repo_root, &record)?;
    Ok(RuntimeSessionDto {
        project_id: persisted.project_id,
        runtime_kind: persisted.runtime_kind,
        provider_id: persisted.provider_id,
        flow_id: persisted.flow_id,
        session_id: persisted.session_id,
        account_id: persisted.account_id,
        phase: persisted.auth_phase,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: persisted
            .last_error
            .as_ref()
            .map(|error| error.code.clone()),
        last_error: persisted.last_error.map(runtime_diagnostic_from_record),
        updated_at: persisted.updated_at,
    })
}

pub(crate) fn runtime_session_from_record(
    state: &DesktopState,
    project_id: &str,
    stored: Option<&RuntimeSessionRecord>,
) -> RuntimeSessionDto {
    let Some(stored) = stored else {
        return default_runtime_session(project_id);
    };

    let active_flow = stored
        .flow_id
        .as_deref()
        .and_then(|flow_id| state.active_auth_flows().snapshot(flow_id));

    if let Some(flow) = active_flow {
        let flow_last_error = flow.last_error.map(runtime_diagnostic_from_auth);
        let last_error = flow_last_error.clone().or_else(|| {
            stored
                .last_error
                .clone()
                .map(runtime_diagnostic_from_record)
        });
        let updated_at = if flow_last_error.is_some() || last_error.is_none() {
            flow.updated_at.clone()
        } else {
            stored.updated_at.clone()
        };

        return RuntimeSessionDto {
            project_id: stored.project_id.clone(),
            runtime_kind: stored.runtime_kind.clone(),
            provider_id: stored.provider_id.clone(),
            flow_id: Some(flow.flow_id),
            session_id: flow.session_id.or_else(|| stored.session_id.clone()),
            account_id: flow.account_id.or_else(|| stored.account_id.clone()),
            phase: flow.phase,
            callback_bound: Some(flow.callback_bound),
            authorization_url: Some(flow.authorization_url),
            redirect_uri: Some(flow.redirect_uri),
            last_error_code: last_error.as_ref().map(|error| error.code.clone()),
            last_error,
            updated_at,
        };
    }

    RuntimeSessionDto {
        project_id: stored.project_id.clone(),
        runtime_kind: stored.runtime_kind.clone(),
        provider_id: stored.provider_id.clone(),
        flow_id: stored.flow_id.clone(),
        session_id: stored.session_id.clone(),
        account_id: stored.account_id.clone(),
        phase: stored.auth_phase.clone(),
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: stored.last_error.as_ref().map(|error| error.code.clone()),
        last_error: stored
            .last_error
            .clone()
            .map(runtime_diagnostic_from_record),
        updated_at: stored.updated_at.clone(),
    }
}

pub(crate) fn emit_runtime_updated<R: Runtime>(
    app: &AppHandle<R>,
    runtime: &RuntimeSessionDto,
) -> CommandResult<()> {
    app.emit(
        RUNTIME_UPDATED_EVENT,
        RuntimeUpdatedPayloadDto {
            project_id: runtime.project_id.clone(),
            runtime_kind: runtime.runtime_kind.clone(),
            provider_id: runtime.provider_id.clone(),
            flow_id: runtime.flow_id.clone(),
            session_id: runtime.session_id.clone(),
            account_id: runtime.account_id.clone(),
            auth_phase: runtime.phase.clone(),
            last_error_code: runtime.last_error_code.clone(),
            last_error: runtime.last_error.clone(),
            updated_at: runtime.updated_at.clone(),
        },
    )
    .map_err(|error| {
        CommandError::retryable(
            "runtime_updated_emit_failed",
            format!(
                "Xero updated runtime-session metadata but could not emit the runtime update event: {error}"
            ),
        )
    })
}

pub(crate) fn command_error_from_auth(error: AuthFlowError) -> CommandError {
    let class = if error.retryable {
        CommandErrorClass::Retryable
    } else {
        CommandErrorClass::UserFixable
    };

    CommandError::new(error.code, class, error.message, error.retryable)
}

pub(crate) fn runtime_diagnostic_from_auth(error: AuthDiagnostic) -> RuntimeDiagnosticDto {
    RuntimeDiagnosticDto {
        code: error.code,
        message: error.message,
        retryable: error.retryable,
    }
}

fn runtime_diagnostic_from_record(error: RuntimeSessionDiagnosticRecord) -> RuntimeDiagnosticDto {
    RuntimeDiagnosticDto {
        code: error.code,
        message: error.message,
        retryable: error.retryable,
    }
}
