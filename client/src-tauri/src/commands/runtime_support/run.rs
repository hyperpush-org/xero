use std::{path::Path, path::PathBuf, time::Duration};

use rand::RngCore;
use tauri::{AppHandle, Emitter, Runtime};

use crate::{
    auth::openai_compatible::{
        missing_openai_compatible_api_key_error, resolve_openai_compatible_endpoint_for_profile,
        resolve_openai_compatible_launch_env,
    },
    commands::{
        get_runtime_session::reconcile_runtime_session,
        get_runtime_settings::runtime_settings_file_from_request,
        provider_profiles::load_provider_profiles_snapshot, CommandError, CommandResult,
        RuntimeAuthPhase, RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto,
        RuntimeRunCheckpointDto, RuntimeRunCheckpointKindDto, RuntimeRunControlInputDto,
        RuntimeRunControlStateDto, RuntimeRunDiagnosticDto, RuntimeRunDto,
        RuntimeRunPendingControlSnapshotDto, RuntimeRunStatusDto, RuntimeRunTransportDto,
        RuntimeRunTransportLivenessDto, RuntimeRunUpdatedPayloadDto, RUNTIME_RUN_UPDATED_EVENT,
    },
    db::project_store::{
        self, build_runtime_run_control_state, RuntimeRunCheckpointKind,
        RuntimeRunControlStateRecord, RuntimeRunDiagnosticRecord, RuntimeRunSnapshotRecord,
        RuntimeRunStatus, RuntimeRunTransportLiveness,
    },
    provider_models::{
        load_provider_model_catalog, ProviderModelCatalog, ProviderModelCatalogSource,
        ProviderModelRecord, ProviderModelThinkingEffort,
    },
    provider_profiles::ProviderProfileReadinessStatus,
    runtime::{
        launch_detached_runtime_supervisor, probe_runtime_run, resolve_runtime_shell_selection,
        RuntimeSupervisorLaunchContext, RuntimeSupervisorLaunchEnv, RuntimeSupervisorLaunchRequest,
        RuntimeSupervisorProbeRequest, ANTHROPIC_PROVIDER_ID, AZURE_OPENAI_PROVIDER_ID,
        GEMINI_AI_STUDIO_PROVIDER_ID, GITHUB_MODELS_PROVIDER_ID, OPENAI_API_PROVIDER_ID,
    },
    state::DesktopState,
};

use super::{
    project::resolve_project_root,
    session::{command_error_from_auth, load_runtime_session_status},
};

pub(crate) const DEFAULT_RUNTIME_RUN_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT: Duration = Duration::from_millis(750);
pub(crate) const DEFAULT_RUNTIME_RUN_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(4);

pub(crate) struct RuntimeRunLaunchOutcome {
    pub repo_root: PathBuf,
    pub snapshot: RuntimeRunSnapshotRecord,
    pub reconnected: bool,
}

struct PreparedRuntimeSupervisorLaunch {
    launch_context: RuntimeSupervisorLaunchContext,
    launch_env: RuntimeSupervisorLaunchEnv,
}

pub(crate) fn load_persisted_runtime_run(
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<Option<RuntimeRunSnapshotRecord>> {
    project_store::load_runtime_run(repo_root, project_id)
}

pub(crate) fn load_runtime_run_status(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<Option<RuntimeRunSnapshotRecord>> {
    probe_runtime_run(
        state,
        RuntimeSupervisorProbeRequest {
            project_id: project_id.into(),
            repo_root: repo_root.to_path_buf(),
            control_timeout: DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
        },
    )
}

pub(crate) fn runtime_run_dto_from_snapshot(snapshot: &RuntimeRunSnapshotRecord) -> RuntimeRunDto {
    RuntimeRunDto {
        project_id: snapshot.run.project_id.clone(),
        run_id: snapshot.run.run_id.clone(),
        runtime_kind: snapshot.run.runtime_kind.clone(),
        provider_id: snapshot.run.provider_id.clone(),
        supervisor_kind: snapshot.run.supervisor_kind.clone(),
        status: runtime_run_status_dto(snapshot.run.status.clone()),
        transport: RuntimeRunTransportDto {
            kind: snapshot.run.transport.kind.clone(),
            endpoint: snapshot.run.transport.endpoint.clone(),
            liveness: runtime_run_transport_liveness_dto(snapshot.run.transport.liveness.clone()),
        },
        controls: runtime_run_control_state_dto(&snapshot.controls),
        started_at: snapshot.run.started_at.clone(),
        last_heartbeat_at: snapshot.run.last_heartbeat_at.clone(),
        last_checkpoint_sequence: snapshot.last_checkpoint_sequence,
        last_checkpoint_at: snapshot.last_checkpoint_at.clone(),
        stopped_at: snapshot.run.stopped_at.clone(),
        last_error_code: snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.clone()),
        last_error: snapshot
            .run
            .last_error
            .as_ref()
            .map(runtime_run_diagnostic_dto),
        updated_at: snapshot.run.updated_at.clone(),
        checkpoints: snapshot
            .checkpoints
            .iter()
            .map(|checkpoint| RuntimeRunCheckpointDto {
                sequence: checkpoint.sequence,
                kind: runtime_run_checkpoint_kind_dto(checkpoint.kind.clone()),
                summary: checkpoint.summary.clone(),
                created_at: checkpoint.created_at.clone(),
            })
            .collect(),
    }
}

pub(crate) fn emit_runtime_run_updated<R: Runtime>(
    app: &AppHandle<R>,
    runtime_run: Option<&RuntimeRunDto>,
) -> CommandResult<()> {
    let project_id = runtime_run
        .map(|runtime_run| runtime_run.project_id.clone())
        .unwrap_or_default();

    app.emit(
        RUNTIME_RUN_UPDATED_EVENT,
        RuntimeRunUpdatedPayloadDto {
            project_id,
            run: runtime_run.cloned(),
        },
    )
    .map_err(|error| {
        CommandError::retryable(
            "runtime_run_updated_emit_failed",
            format!(
                "Cadence updated durable runtime-run metadata but could not emit the runtime-run update event: {error}"
            ),
        )
    })
}

pub(crate) fn emit_runtime_run_updated_if_changed<R: Runtime>(
    app: &AppHandle<R>,
    project_id: &str,
    before: &Option<RuntimeRunSnapshotRecord>,
    after: &Option<RuntimeRunSnapshotRecord>,
) -> CommandResult<()> {
    if before == after {
        return Ok(());
    }

    let runtime_run = after.as_ref().map(runtime_run_dto_from_snapshot);
    if let Some(runtime_run) = runtime_run.as_ref() {
        return emit_runtime_run_updated(app, Some(runtime_run));
    }

    app.emit(
        RUNTIME_RUN_UPDATED_EVENT,
        RuntimeRunUpdatedPayloadDto {
            project_id: project_id.into(),
            run: None,
        },
    )
    .map_err(|error| {
        CommandError::retryable(
            "runtime_run_updated_emit_failed",
            format!(
                "Cadence updated durable runtime-run metadata but could not emit the runtime-run update event: {error}"
            ),
        )
    })
}

pub(crate) fn launch_or_reconnect_runtime_run<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    requested_controls: Option<RuntimeRunControlInputDto>,
    initial_prompt: Option<String>,
) -> CommandResult<RuntimeRunLaunchOutcome> {
    let repo_root = resolve_project_root(app, state, project_id)?;
    let before = load_persisted_runtime_run(&repo_root, project_id)?;
    let current = load_runtime_run_status(state, &repo_root, project_id)?;
    emit_runtime_run_updated_if_changed(app, project_id, &before, &current)?;

    if let Some(existing) = current
        .as_ref()
        .filter(|snapshot| is_reconnectable_runtime_run(snapshot))
    {
        return Ok(RuntimeRunLaunchOutcome {
            repo_root,
            snapshot: existing.clone(),
            reconnected: true,
        });
    }

    let runtime = load_runtime_session_status(state, &repo_root, project_id)?;
    let runtime = reconcile_runtime_session(app, state, &repo_root, runtime)?;
    ensure_runtime_run_auth_ready(&runtime.phase)?;
    let session_id = runtime.session_id.clone().ok_or_else(|| {
        CommandError::retryable(
            "runtime_run_session_missing",
            "Cadence cannot start a runtime run until the selected project's authenticated runtime session exposes a stable session id.",
        )
    })?;

    let shell = resolve_runtime_shell_selection();
    let run_controls = resolve_initial_runtime_run_control_state(
        app,
        state,
        requested_controls.as_ref(),
        initial_prompt.as_deref(),
    )?;
    let prepared_launch =
        prepare_runtime_supervisor_launch(app, state, &runtime, &session_id, &run_controls)?;

    let launched = launch_detached_runtime_supervisor(
        state,
        RuntimeSupervisorLaunchRequest {
            project_id: project_id.into(),
            repo_root: repo_root.clone(),
            runtime_kind: runtime.runtime_kind.clone(),
            run_id: generate_runtime_run_id(),
            session_id,
            flow_id: runtime.flow_id.clone(),
            launch_context: prepared_launch.launch_context,
            launch_env: prepared_launch.launch_env,
            program: shell.program,
            args: shell.args,
            startup_timeout: DEFAULT_RUNTIME_RUN_STARTUP_TIMEOUT,
            control_timeout: DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
            supervisor_binary: state.runtime_supervisor_binary_override().cloned(),
            run_controls,
        },
    )?;

    let runtime_run = runtime_run_dto_from_snapshot(&launched);
    emit_runtime_run_updated(app, Some(&runtime_run))?;

    Ok(RuntimeRunLaunchOutcome {
        repo_root,
        snapshot: launched,
        reconnected: false,
    })
}

pub(crate) fn normalize_requested_runtime_run_controls<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    requested_controls: &RuntimeRunControlInputDto,
) -> CommandResult<RuntimeRunControlInputDto> {
    let control_state =
        resolve_initial_runtime_run_control_state(app, state, Some(requested_controls), None)?;
    Ok(RuntimeRunControlInputDto {
        model_id: control_state.active.model_id,
        thinking_effort: control_state.active.thinking_effort,
        approval_mode: control_state.active.approval_mode,
    })
}

pub(crate) fn generate_runtime_run_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "run-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

pub(crate) fn is_reconnectable_runtime_run(
    snapshot: &crate::db::project_store::RuntimeRunSnapshotRecord,
) -> bool {
    matches!(
        snapshot.run.status,
        crate::db::project_store::RuntimeRunStatus::Starting
            | crate::db::project_store::RuntimeRunStatus::Running
    ) && snapshot.run.transport.liveness
        == crate::db::project_store::RuntimeRunTransportLiveness::Reachable
}

pub(crate) fn ensure_runtime_run_auth_ready(phase: &RuntimeAuthPhase) -> CommandResult<()> {
    match phase {
        RuntimeAuthPhase::Authenticated => Ok(()),
        RuntimeAuthPhase::Starting
        | RuntimeAuthPhase::AwaitingBrowserCallback
        | RuntimeAuthPhase::AwaitingManualInput
        | RuntimeAuthPhase::ExchangingCode
        | RuntimeAuthPhase::Refreshing => Err(CommandError::retryable(
            "runtime_run_auth_in_progress",
            "Cadence cannot start a runtime run until the selected project's authenticated runtime session finishes its auth transition.",
        )),
        RuntimeAuthPhase::Idle | RuntimeAuthPhase::Cancelled | RuntimeAuthPhase::Failed => {
            Err(CommandError::user_fixable(
                "runtime_run_auth_required",
                "Cadence cannot start a runtime run until the selected project has an authenticated runtime session.",
            ))
        }
    }
}

pub(super) fn runtime_run_diagnostic_dto(
    reason: &RuntimeRunDiagnosticRecord,
) -> RuntimeRunDiagnosticDto {
    RuntimeRunDiagnosticDto {
        code: reason.code.clone(),
        message: reason.message.clone(),
    }
}

pub(super) fn runtime_reason_dto(
    reason: &RuntimeRunDiagnosticRecord,
) -> crate::commands::AutonomousLifecycleReasonDto {
    crate::commands::AutonomousLifecycleReasonDto {
        code: reason.code.clone(),
        message: reason.message.clone(),
    }
}

fn resolve_initial_runtime_run_control_state<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    requested_controls: Option<&RuntimeRunControlInputDto>,
    initial_prompt: Option<&str>,
) -> CommandResult<RuntimeRunControlStateRecord> {
    let provider_profiles = load_provider_profiles_snapshot(app, state)?;
    let active_profile = provider_profiles.active_profile().ok_or_else(|| {
        CommandError::user_fixable(
            "provider_profiles_invalid",
            "Cadence could not derive runtime-run controls because the active provider profile is missing.",
        )
    })?;
    let catalog = load_provider_model_catalog(app, state, &active_profile.profile_id, false)?;
    let model_id = resolve_initial_runtime_run_model_id(
        active_profile.provider_id.as_str(),
        active_profile.model_id.as_str(),
        requested_controls,
    )?;
    let model = resolve_initial_runtime_run_model(&catalog, &model_id)?;
    let thinking_effort = resolve_initial_runtime_run_thinking_effort(model, requested_controls)?;
    let approval_mode = requested_controls
        .map(|controls| controls.approval_mode.clone())
        .unwrap_or(RuntimeRunApprovalModeDto::Suggest);
    let timestamp = crate::auth::now_timestamp();

    build_runtime_run_control_state(
        &model_id,
        thinking_effort,
        approval_mode,
        &timestamp,
        initial_prompt,
    )
}

fn prepare_runtime_supervisor_launch<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    runtime: &crate::commands::RuntimeSessionDto,
    session_id: &str,
    run_controls: &RuntimeRunControlStateRecord,
) -> CommandResult<PreparedRuntimeSupervisorLaunch> {
    let provider_profiles = load_provider_profiles_snapshot(app, state)?;
    let active_profile = provider_profiles.active_profile().ok_or_else(|| {
        CommandError::user_fixable(
            "provider_profiles_invalid",
            "Cadence could not launch a runtime run because the active provider profile is missing.",
        )
    })?;

    if active_profile.provider_id != runtime.provider_id {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_provider_mismatch",
            format!(
                "Cadence cannot launch runtime run `{}` because the active provider profile targets `{}` while the authenticated runtime session is bound to `{}`.",
                run_controls.active.model_id, active_profile.provider_id, runtime.provider_id
            ),
        ));
    }

    let mut launch_env = RuntimeSupervisorLaunchEnv::default();
    if runtime.provider_id == ANTHROPIC_PROVIDER_ID {
        let readiness = active_profile.readiness(&provider_profiles.credentials);
        match readiness.status {
            ProviderProfileReadinessStatus::Ready => {
                let secret = provider_profiles
                    .anthropic_credential(&active_profile.profile_id)
                    .ok_or_else(|| {
                        CommandError::user_fixable(
                            "provider_profile_credentials_unavailable",
                            format!(
                                "Cadence cannot launch the detached Anthropic runtime because provider profile `{}` no longer matches the saved app-local secret state.",
                                active_profile.profile_id
                            ),
                        )
                    })?;
                launch_env.insert("ANTHROPIC_API_KEY", secret.api_key.clone());
            }
            ProviderProfileReadinessStatus::Missing => {
                return Err(CommandError::user_fixable(
                    "anthropic_api_key_missing",
                    format!(
                        "Cadence cannot launch the detached Anthropic runtime because provider profile `{}` has no app-local API key configured.",
                        active_profile.profile_id
                    ),
                ));
            }
            ProviderProfileReadinessStatus::Malformed => {
                return Err(CommandError::user_fixable(
                    "provider_profile_credentials_unavailable",
                    format!(
                        "Cadence cannot launch the detached Anthropic runtime because provider profile `{}` no longer matches the saved app-local secret state.",
                        active_profile.profile_id
                    ),
                ));
            }
        }
    } else if matches!(
        runtime.provider_id.as_str(),
        OPENAI_API_PROVIDER_ID
            | AZURE_OPENAI_PROVIDER_ID
            | GITHUB_MODELS_PROVIDER_ID
            | GEMINI_AI_STUDIO_PROVIDER_ID
    ) {
        let readiness = active_profile.readiness(&provider_profiles.credentials);
        let api_key = match readiness.status {
            ProviderProfileReadinessStatus::Ready => provider_profiles
                .matched_api_key_credential_for_profile(&active_profile.profile_id)
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "provider_profile_credentials_unavailable",
                        format!(
                            "Cadence cannot launch the detached {} runtime because provider profile `{}` no longer matches the saved app-local secret state.",
                            runtime.provider_id, active_profile.profile_id
                        ),
                    )
                })?
                .api_key
                .clone(),
            ProviderProfileReadinessStatus::Missing => {
                return Err(command_error_from_auth(
                    missing_openai_compatible_api_key_error(runtime.provider_id.as_str(), "launch"),
                ));
            }
            ProviderProfileReadinessStatus::Malformed => {
                return Err(CommandError::user_fixable(
                    "provider_profile_credentials_unavailable",
                    format!(
                        "Cadence cannot launch the detached {} runtime because provider profile `{}` no longer matches the saved app-local secret state.",
                        runtime.provider_id, active_profile.profile_id
                    ),
                ));
            }
        };

        let endpoint = resolve_openai_compatible_endpoint_for_profile(
            active_profile,
            &state.openai_compatible_auth_config(),
        )
        .map_err(command_error_from_auth)?;
        let env = resolve_openai_compatible_launch_env(&api_key, &endpoint)
            .map_err(command_error_from_auth)?;
        launch_env.insert("OPENAI_API_KEY", env.api_key);
        launch_env.insert("OPENAI_BASE_URL", env.base_url);
        if let Some(api_version) = env.api_version {
            launch_env.insert("OPENAI_API_VERSION", api_version);
        }
    }

    Ok(PreparedRuntimeSupervisorLaunch {
        launch_context: RuntimeSupervisorLaunchContext {
            provider_id: runtime.provider_id.clone(),
            session_id: session_id.to_owned(),
            flow_id: runtime.flow_id.clone(),
            model_id: run_controls.active.model_id.clone(),
            thinking_effort: run_controls.active.thinking_effort.clone(),
        },
        launch_env,
    })
}

fn resolve_initial_runtime_run_model_id(
    provider_id: &str,
    configured_model_id: &str,
    requested_controls: Option<&RuntimeRunControlInputDto>,
) -> CommandResult<String> {
    match requested_controls {
        Some(requested) => {
            runtime_settings_file_from_request(provider_id, &requested.model_id, false)
                .map(|settings| settings.model_id)
        }
        None => Ok(configured_model_id.trim().to_owned()),
    }
}

fn resolve_initial_runtime_run_model<'a>(
    catalog: &'a ProviderModelCatalog,
    model_id: &str,
) -> CommandResult<&'a ProviderModelRecord> {
    if matches!(catalog.source, ProviderModelCatalogSource::Unavailable)
        || catalog.models.is_empty()
    {
        return Err(CommandError::user_fixable(
            "runtime_run_initial_controls_unavailable",
            format!(
                "Cadence could not derive runtime-run controls because the active provider-model catalog for profile `{}` is unavailable.",
                catalog.profile_id
            ),
        ));
    }

    catalog.models.iter().find(|model| model.model_id == model_id).ok_or_else(|| {
        CommandError::user_fixable(
            "runtime_run_initial_controls_invalid",
            format!(
                "Cadence could not seed runtime-run controls because model `{model_id}` is not present in the active provider-model catalog for profile `{}`.",
                catalog.profile_id
            ),
        )
    })
}

fn resolve_initial_runtime_run_thinking_effort(
    model: &ProviderModelRecord,
    requested_controls: Option<&RuntimeRunControlInputDto>,
) -> CommandResult<Option<crate::commands::ProviderModelThinkingEffortDto>> {
    let requested_effort = requested_controls.and_then(|controls| controls.thinking_effort.clone());
    if !model.thinking.supported {
        if requested_effort.is_some() {
            return Err(CommandError::user_fixable(
                "runtime_run_initial_controls_invalid",
                format!(
                    "Cadence could not seed runtime-run controls because model `{}` does not support configurable thinking.",
                    model.model_id
                ),
            ));
        }
        return Ok(None);
    }

    if let Some(requested_effort) = requested_effort {
        let mapped_requested_effort = provider_model_thinking_effort_from_dto(&requested_effort);
        if model
            .thinking
            .effort_options
            .contains(&mapped_requested_effort)
        {
            return Ok(Some(requested_effort));
        }
        return Err(CommandError::user_fixable(
            "runtime_run_initial_controls_invalid",
            format!(
                "Cadence could not seed runtime-run controls because thinking effort is unsupported for model `{}`.",
                model.model_id
            ),
        ));
    }

    Ok(model
        .thinking
        .default_effort
        .or_else(|| model.thinking.effort_options.first().copied())
        .map(provider_model_thinking_effort_dto))
}

fn provider_model_thinking_effort_from_dto(
    effort: &crate::commands::ProviderModelThinkingEffortDto,
) -> ProviderModelThinkingEffort {
    match effort {
        crate::commands::ProviderModelThinkingEffortDto::Minimal => {
            ProviderModelThinkingEffort::Minimal
        }
        crate::commands::ProviderModelThinkingEffortDto::Low => ProviderModelThinkingEffort::Low,
        crate::commands::ProviderModelThinkingEffortDto::Medium => {
            ProviderModelThinkingEffort::Medium
        }
        crate::commands::ProviderModelThinkingEffortDto::High => ProviderModelThinkingEffort::High,
        crate::commands::ProviderModelThinkingEffortDto::XHigh => {
            ProviderModelThinkingEffort::XHigh
        }
    }
}

fn provider_model_thinking_effort_dto(
    effort: ProviderModelThinkingEffort,
) -> crate::commands::ProviderModelThinkingEffortDto {
    match effort {
        ProviderModelThinkingEffort::Minimal => {
            crate::commands::ProviderModelThinkingEffortDto::Minimal
        }
        ProviderModelThinkingEffort::Low => crate::commands::ProviderModelThinkingEffortDto::Low,
        ProviderModelThinkingEffort::Medium => {
            crate::commands::ProviderModelThinkingEffortDto::Medium
        }
        ProviderModelThinkingEffort::High => crate::commands::ProviderModelThinkingEffortDto::High,
        ProviderModelThinkingEffort::XHigh => {
            crate::commands::ProviderModelThinkingEffortDto::XHigh
        }
    }
}

fn runtime_run_control_state_dto(
    controls: &RuntimeRunControlStateRecord,
) -> RuntimeRunControlStateDto {
    RuntimeRunControlStateDto {
        active: RuntimeRunActiveControlSnapshotDto {
            model_id: controls.active.model_id.clone(),
            thinking_effort: controls.active.thinking_effort.clone(),
            approval_mode: controls.active.approval_mode.clone(),
            revision: controls.active.revision,
            applied_at: controls.active.applied_at.clone(),
        },
        pending: controls
            .pending
            .as_ref()
            .map(|pending| RuntimeRunPendingControlSnapshotDto {
                model_id: pending.model_id.clone(),
                thinking_effort: pending.thinking_effort.clone(),
                approval_mode: pending.approval_mode.clone(),
                revision: pending.revision,
                queued_at: pending.queued_at.clone(),
                queued_prompt: pending.queued_prompt.clone(),
                queued_prompt_at: pending.queued_prompt_at.clone(),
            }),
    }
}

fn runtime_run_status_dto(status: RuntimeRunStatus) -> RuntimeRunStatusDto {
    match status {
        RuntimeRunStatus::Starting => RuntimeRunStatusDto::Starting,
        RuntimeRunStatus::Running => RuntimeRunStatusDto::Running,
        RuntimeRunStatus::Stale => RuntimeRunStatusDto::Stale,
        RuntimeRunStatus::Stopped => RuntimeRunStatusDto::Stopped,
        RuntimeRunStatus::Failed => RuntimeRunStatusDto::Failed,
    }
}

fn runtime_run_transport_liveness_dto(
    liveness: RuntimeRunTransportLiveness,
) -> RuntimeRunTransportLivenessDto {
    match liveness {
        RuntimeRunTransportLiveness::Unknown => RuntimeRunTransportLivenessDto::Unknown,
        RuntimeRunTransportLiveness::Reachable => RuntimeRunTransportLivenessDto::Reachable,
        RuntimeRunTransportLiveness::Unreachable => RuntimeRunTransportLivenessDto::Unreachable,
    }
}

fn runtime_run_checkpoint_kind_dto(kind: RuntimeRunCheckpointKind) -> RuntimeRunCheckpointKindDto {
    match kind {
        RuntimeRunCheckpointKind::Bootstrap => RuntimeRunCheckpointKindDto::Bootstrap,
        RuntimeRunCheckpointKind::State => RuntimeRunCheckpointKindDto::State,
        RuntimeRunCheckpointKind::Tool => RuntimeRunCheckpointKindDto::Tool,
        RuntimeRunCheckpointKind::ActionRequired => RuntimeRunCheckpointKindDto::ActionRequired,
        RuntimeRunCheckpointKind::Diagnostic => RuntimeRunCheckpointKindDto::Diagnostic,
    }
}
