use std::{path::Path, path::PathBuf, thread, time::Duration};

use rand::RngCore;
use tauri::{AppHandle, Emitter, Runtime};

use crate::{
    auth::{
        anthropic::{resolve_anthropic_family_launch_env, AnthropicFamilyProfileInput},
        load_latest_openai_codex_session, load_openai_codex_session_for_profile_link,
        now_timestamp,
        openai_compatible::{
            resolve_openai_compatible_endpoint_for_profile, resolve_openai_compatible_launch_env,
        },
    },
    commands::{
        get_runtime_session::reconcile_runtime_session_for_profile,
        get_runtime_settings::{
            runtime_settings_file_from_request, runtime_settings_snapshot_for_provider_profile,
        },
        provider_credentials::load_provider_credentials_view,
        CommandError, CommandResult, RuntimeAuthPhase, RuntimeRunActiveControlSnapshotDto,
        RuntimeRunApprovalModeDto, RuntimeRunCheckpointDto, RuntimeRunCheckpointKindDto,
        RuntimeRunControlInputDto, RuntimeRunControlStateDto, RuntimeRunDiagnosticDto,
        RuntimeRunDto, RuntimeRunPendingControlSnapshotDto, RuntimeRunStatusDto,
        RuntimeRunTransportDto, RuntimeRunTransportLivenessDto, RuntimeRunUpdatedPayloadDto,
        RUNTIME_RUN_UPDATED_EVENT,
    },
    db::project_store::{
        self, build_runtime_run_control_state_with_profile, RuntimeRunCheckpointKind,
        RuntimeRunCheckpointRecord, RuntimeRunControlStateRecord, RuntimeRunDiagnosticRecord,
        RuntimeRunRecord, RuntimeRunSnapshotRecord, RuntimeRunStatus, RuntimeRunTransportLiveness,
        RuntimeRunTransportRecord, RuntimeRunUpsertRecord,
    },
    mcp::{materialize_runtime_mcp_projection_for_run, RUNTIME_MCP_PROJECTION_DIRECTORY_NAME},
    provider_credentials::ProviderCredentialReadinessStatus,
    provider_models::{
        load_provider_model_catalog, ProviderModelCatalog, ProviderModelCatalogSource,
        ProviderModelRecord, ProviderModelThinkingEffort,
    },
    runtime::{
        create_owned_agent_run, drive_owned_agent_run, launch_detached_runtime_supervisor,
        normalize_openai_codex_model_id, openai_codex_provider, probe_runtime_run,
        resolve_runtime_shell_selection, AgentProviderConfig, AnthropicProviderConfig,
        AutonomousToolRuntime, BedrockProviderConfig, OpenAiCompatibleProviderConfig,
        OpenAiResponsesProviderConfig, OwnedAgentRunRequest, RuntimeSupervisorLaunchContext,
        RuntimeSupervisorLaunchEnv, RuntimeSupervisorLaunchRequest, RuntimeSupervisorProbeRequest,
        VertexProviderConfig, ANTHROPIC_PROVIDER_ID, AZURE_OPENAI_PROVIDER_ID, BEDROCK_PROVIDER_ID,
        GEMINI_AI_STUDIO_PROVIDER_ID, GITHUB_MODELS_PROVIDER_ID, OLLAMA_PROVIDER_ID,
        OPENAI_API_PROVIDER_ID, OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID,
        OWNED_AGENT_RUNTIME_KIND, OWNED_AGENT_SUPERVISOR_KIND, VERTEX_PROVIDER_ID,
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
const DEFAULT_OPENAI_RESPONSES_BASE_URL: &str = "https://api.openai.com/v1";
const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

pub(crate) struct RuntimeRunLaunchOutcome {
    pub repo_root: PathBuf,
    pub snapshot: RuntimeRunSnapshotRecord,
    pub reconnected: bool,
}

struct PreparedRuntimeSupervisorLaunch {
    launch_context: RuntimeSupervisorLaunchContext,
    launch_env: RuntimeSupervisorLaunchEnv,
}

struct ActiveProviderProfileSelection {
    profile_id: String,
    provider_id: String,
    model_id: String,
}

pub(crate) fn load_persisted_runtime_run(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> CommandResult<Option<RuntimeRunSnapshotRecord>> {
    project_store::load_runtime_run(repo_root, project_id, agent_session_id)
}

pub(crate) fn load_runtime_run_status(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> CommandResult<Option<RuntimeRunSnapshotRecord>> {
    if let Some(snapshot) = load_persisted_runtime_run(repo_root, project_id, agent_session_id)? {
        if snapshot.run.supervisor_kind == OWNED_AGENT_SUPERVISOR_KIND {
            return Ok(Some(snapshot));
        }
    }

    probe_runtime_run(
        state,
        RuntimeSupervisorProbeRequest {
            project_id: project_id.into(),
            agent_session_id: agent_session_id.into(),
            repo_root: repo_root.to_path_buf(),
            control_timeout: DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
        },
    )
}

pub(crate) fn runtime_run_dto_from_snapshot(snapshot: &RuntimeRunSnapshotRecord) -> RuntimeRunDto {
    RuntimeRunDto {
        project_id: snapshot.run.project_id.clone(),
        agent_session_id: snapshot.run.agent_session_id.clone(),
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
    let agent_session_id = runtime_run
        .map(|runtime_run| runtime_run.agent_session_id.clone())
        .unwrap_or_default();

    app.emit(
        RUNTIME_RUN_UPDATED_EVENT,
        RuntimeRunUpdatedPayloadDto {
            project_id,
            agent_session_id,
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
    agent_session_id: &str,
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
            agent_session_id: agent_session_id.into(),
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

pub(crate) fn launch_or_reconnect_runtime_run<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    agent_session_id: &str,
    requested_controls: Option<RuntimeRunControlInputDto>,
    initial_prompt: Option<String>,
) -> CommandResult<RuntimeRunLaunchOutcome> {
    if state.owned_agent_provider_config_override().is_some() {
        return launch_owned_runtime_run(
            app,
            state,
            project_id,
            agent_session_id,
            requested_controls,
            initial_prompt,
        );
    }

    launch_or_reconnect_detached_runtime_run(
        app,
        state,
        project_id,
        agent_session_id,
        requested_controls,
        initial_prompt,
    )
}

fn launch_owned_runtime_run<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    agent_session_id: &str,
    requested_controls: Option<RuntimeRunControlInputDto>,
    initial_prompt: Option<String>,
) -> CommandResult<RuntimeRunLaunchOutcome> {
    let repo_root = resolve_project_root(app, state, project_id)?;
    project_store::ensure_agent_session_active(&repo_root, project_id, agent_session_id)?;
    let before = load_persisted_runtime_run(&repo_root, project_id, agent_session_id)?;

    if let Some(existing) = before
        .as_ref()
        .filter(|snapshot| is_reconnectable_owned_runtime_run(snapshot))
    {
        reject_runtime_run_provider_profile_switch(existing, requested_controls.as_ref())?;
        return Ok(RuntimeRunLaunchOutcome {
            repo_root,
            snapshot: existing.clone(),
            reconnected: true,
        });
    }

    if let Some(existing) = before
        .as_ref()
        .filter(|snapshot| requires_matching_provider_for_new_launch(snapshot))
    {
        if existing.run.supervisor_kind != OWNED_AGENT_SUPERVISOR_KIND {
            return Err(CommandError::user_fixable(
                "runtime_supervisor_kind_mismatch",
                format!(
                    "Cadence cannot start the owned runtime because project `{project_id}` is still bound to {} runtime run `{}`. Stop that run before switching to the owned agent runtime.",
                    existing.run.supervisor_kind,
                    existing.run.run_id
                ),
            ));
        }
    }

    let active_profile = load_provider_profile_selection(
        app,
        state,
        requested_controls.as_ref(),
        state.owned_agent_provider_config_override().is_none(),
    )?;
    let run_controls = resolve_owned_runtime_run_control_state(
        &active_profile,
        requested_controls.as_ref(),
        initial_prompt.as_deref(),
    )?;
    let run_id = generate_runtime_run_id();
    let mut snapshot = persist_owned_runtime_run(
        &repo_root,
        project_id,
        agent_session_id,
        &run_id,
        &active_profile.provider_id,
        &run_controls,
        RuntimeRunStatus::Running,
        None,
        "Owned agent runtime started.",
        1,
        None,
    )?;

    let runtime_run = runtime_run_dto_from_snapshot(&snapshot);
    emit_runtime_run_updated(app, Some(&runtime_run))?;

    if let Some(prompt) = initial_prompt.and_then(|prompt| {
        let trimmed = prompt.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }) {
        let tool_runtime = AutonomousToolRuntime::for_project(app, state, project_id)?;
        let provider_config =
            resolve_owned_agent_provider_config(app, state, requested_controls.as_ref())?;
        let owned_request = OwnedAgentRunRequest {
            repo_root: repo_root.clone(),
            project_id: project_id.into(),
            agent_session_id: agent_session_id.into(),
            run_id: run_id.clone(),
            prompt,
            controls: requested_controls,
            tool_runtime,
            provider_config,
        };
        let lease = state
            .agent_run_supervisor()
            .begin(project_id, agent_session_id, &run_id)?;
        if let Err(error) = create_owned_agent_run(&owned_request) {
            drop(lease);
            let diagnostic = RuntimeRunDiagnosticRecord {
                code: error.code.clone(),
                message: error.message.clone(),
            };
            snapshot = persist_owned_runtime_run(
                &repo_root,
                project_id,
                agent_session_id,
                &run_id,
                &active_profile.provider_id,
                &run_controls,
                RuntimeRunStatus::Failed,
                Some(diagnostic),
                "Owned agent task failed.",
                2,
                Some(&snapshot),
            )?;
            let runtime_run = runtime_run_dto_from_snapshot(&snapshot);
            emit_runtime_run_updated(app, Some(&runtime_run))?;
            return Err(error);
        }
        let app_for_task = app.clone();
        let repo_root_for_task = repo_root.clone();
        let project_id_for_task = project_id.to_string();
        let agent_session_id_for_task = agent_session_id.to_string();
        let run_id_for_task = run_id.clone();
        let provider_id_for_task = active_profile.provider_id.clone();
        let run_controls_for_task = run_controls.clone();
        let runtime_snapshot_for_task = snapshot.clone();
        thread::spawn(move || {
            let token = lease.token();
            let outcome = drive_owned_agent_run(owned_request, token);
            let failure = match outcome {
                Ok(agent_snapshot)
                    if agent_snapshot.run.status == project_store::AgentRunStatus::Failed =>
                {
                    agent_snapshot
                        .run
                        .last_error
                        .map(|error| RuntimeRunDiagnosticRecord {
                            code: error.code,
                            message: error.message,
                        })
                }
                Err(error) => Some(RuntimeRunDiagnosticRecord {
                    code: error.code,
                    message: error.message,
                }),
                _ => None,
            };

            if let Some(diagnostic) = failure {
                if let Ok(snapshot) = persist_owned_runtime_run(
                    &repo_root_for_task,
                    &project_id_for_task,
                    &agent_session_id_for_task,
                    &run_id_for_task,
                    &provider_id_for_task,
                    &run_controls_for_task,
                    RuntimeRunStatus::Failed,
                    Some(diagnostic),
                    "Owned agent task failed.",
                    runtime_snapshot_for_task
                        .last_checkpoint_sequence
                        .saturating_add(1),
                    Some(&runtime_snapshot_for_task),
                ) {
                    let runtime_run = runtime_run_dto_from_snapshot(&snapshot);
                    let _ = emit_runtime_run_updated(&app_for_task, Some(&runtime_run));
                }
            }
            drop(lease);
        });
    }

    Ok(RuntimeRunLaunchOutcome {
        repo_root,
        snapshot,
        reconnected: false,
    })
}

pub(crate) fn launch_or_reconnect_detached_runtime_run<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    agent_session_id: &str,
    requested_controls: Option<RuntimeRunControlInputDto>,
    initial_prompt: Option<String>,
) -> CommandResult<RuntimeRunLaunchOutcome> {
    let repo_root = resolve_project_root(app, state, project_id)?;
    project_store::ensure_agent_session_active(&repo_root, project_id, agent_session_id)?;
    let before = load_persisted_runtime_run(&repo_root, project_id, agent_session_id)?;
    let current = if before
        .as_ref()
        .is_some_and(|snapshot| snapshot.run.supervisor_kind == OWNED_AGENT_SUPERVISOR_KIND)
    {
        before.clone()
    } else {
        probe_runtime_run(
            state,
            RuntimeSupervisorProbeRequest {
                project_id: project_id.into(),
                agent_session_id: agent_session_id.into(),
                repo_root: repo_root.clone(),
                control_timeout: DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
            },
        )?
    };
    emit_runtime_run_updated_if_changed(app, project_id, agent_session_id, &before, &current)?;

    if let Some(existing) = current.as_ref().filter(|snapshot| {
        snapshot.run.supervisor_kind != OWNED_AGENT_SUPERVISOR_KIND
            && is_reconnectable_runtime_run(snapshot)
    }) {
        reject_runtime_run_provider_profile_switch(existing, requested_controls.as_ref())?;
        return Ok(RuntimeRunLaunchOutcome {
            repo_root,
            snapshot: existing.clone(),
            reconnected: true,
        });
    }

    let active_profile =
        load_provider_profile_selection(app, state, requested_controls.as_ref(), true)?;
    if let Some(existing) = current
        .as_ref()
        .filter(|snapshot| requires_matching_provider_for_new_launch(snapshot))
    {
        if existing.run.supervisor_kind == OWNED_AGENT_SUPERVISOR_KIND {
            return Err(CommandError::user_fixable(
                "runtime_supervisor_kind_mismatch",
                format!(
                    "Cadence cannot start the detached runtime because project `{project_id}` is still bound to owned runtime run `{}`. Stop that run before switching to the detached terminal adapter.",
                    existing.run.run_id
                ),
            ));
        }
        if existing.run.provider_id != active_profile.provider_id {
            return Err(runtime_supervisor_existing_run_provider_mismatch(
                &active_profile,
                existing,
            ));
        }
    }

    let runtime = load_runtime_session_status(state, &repo_root, project_id)?;
    if runtime.phase == RuntimeAuthPhase::Authenticated
        && runtime.provider_id != active_profile.provider_id
    {
        return Err(runtime_supervisor_session_provider_mismatch(
            &active_profile,
            &runtime,
        ));
    }

    let runtime = reconcile_runtime_session_for_profile(
        app,
        state,
        &repo_root,
        runtime,
        Some(&active_profile.profile_id),
    )?;
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
        &active_profile,
        requested_controls.as_ref(),
        initial_prompt.as_deref(),
    )?;
    let run_id = generate_runtime_run_id();
    let prepared_launch = prepare_runtime_supervisor_launch(
        app,
        state,
        &active_profile,
        &runtime,
        &session_id,
        &run_controls,
        &run_id,
    )?;

    let launched = launch_detached_runtime_supervisor(
        state,
        RuntimeSupervisorLaunchRequest {
            project_id: project_id.into(),
            agent_session_id: agent_session_id.into(),
            repo_root: repo_root.clone(),
            runtime_kind: runtime.runtime_kind.clone(),
            run_id,
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
    let active_profile = load_provider_profile_selection(
        app,
        state,
        Some(requested_controls),
        state.owned_agent_provider_config_override().is_none(),
    )?;
    let control_state = resolve_initial_runtime_run_control_state(
        app,
        state,
        &active_profile,
        Some(requested_controls),
        None,
    )?;
    Ok(RuntimeRunControlInputDto {
        provider_profile_id: control_state.active.provider_profile_id,
        model_id: control_state.active.model_id,
        thinking_effort: control_state.active.thinking_effort,
        approval_mode: control_state.active.approval_mode,
        plan_mode_required: control_state.active.plan_mode_required,
    })
}

pub(crate) fn resolve_owned_agent_provider_config<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    requested_controls: Option<&RuntimeRunControlInputDto>,
) -> CommandResult<AgentProviderConfig> {
    if let Some(config) = state.owned_agent_provider_config_override() {
        return Ok(config);
    }

    let provider_profiles = load_provider_credentials_view(app, state)?;
    let requested_profile_id = requested_controls
        .and_then(|controls| controls.provider_profile_id.as_deref())
        .map(str::trim)
        .filter(|profile_id| !profile_id.is_empty());
    let active_profile = match requested_profile_id {
        Some(profile_id) => provider_profiles.profile(profile_id).ok_or_else(|| {
            CommandError::user_fixable(
                "provider_profile_not_found",
                format!(
                    "Cadence could not resolve the owned-agent provider because provider profile `{profile_id}` is missing.",
                ),
            )
        })?,
        None => provider_profiles.active_profile().ok_or_else(|| {
            CommandError::user_fixable(
                "provider_profiles_invalid",
                "Cadence could not resolve the owned-agent provider because the active provider profile is missing.",
            )
        })?,
    };
    let runtime_settings =
        runtime_settings_snapshot_for_provider_profile(&provider_profiles, active_profile)?;
    let model_id = requested_controls
        .map(|controls| controls.model_id.trim().to_owned())
        .filter(|model_id| !model_id.is_empty())
        .unwrap_or_else(|| active_profile.model_id.clone());

    match active_profile.provider_id.as_str() {
        OPENAI_CODEX_PROVIDER_ID => {
            let auth_store_path = state
                .auth_store_file_for_provider(app, openai_codex_provider())
                .map_err(command_error_from_auth)?;
            let session = match active_profile.credential_link.as_ref() {
                Some(link) => load_openai_codex_session_for_profile_link(&auth_store_path, link)
                    .map_err(command_error_from_auth)?,
                None => load_latest_openai_codex_session(&auth_store_path)
                    .map_err(command_error_from_auth)?,
            }
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "openai_codex_auth_missing",
                        format!(
                            "Cadence cannot start the owned OpenAI Codex adapter because no global app-local auth session is available for provider profile `{}`.",
                            active_profile.profile_id
                        ),
                    )
                })?;
            Ok(AgentProviderConfig::OpenAiResponses(
                OpenAiResponsesProviderConfig {
                    provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
                    model_id: normalize_openai_codex_model_id(model_id.as_str()),
                    base_url: DEFAULT_OPENAI_RESPONSES_BASE_URL.into(),
                    api_key: session.access_token,
                    timeout_ms: 0,
                },
            ))
        }
        OPENAI_API_PROVIDER_ID => {
            let endpoint = resolve_openai_compatible_endpoint_for_profile(
                active_profile,
                &state.openai_compatible_auth_config(),
            )
            .map_err(command_error_from_auth)?;
            let api_key = runtime_settings.provider_api_key.clone();
            if let (true, Some(api_key)) = (is_openai_responses_model(&model_id), api_key.as_ref())
            {
                Ok(AgentProviderConfig::OpenAiResponses(
                    OpenAiResponsesProviderConfig {
                        provider_id: OPENAI_API_PROVIDER_ID.into(),
                        model_id,
                        base_url: endpoint.effective_base_url,
                        api_key: api_key.clone(),
                        timeout_ms: 0,
                    },
                ))
            } else {
                let hosted_without_key =
                    api_key.is_none() && !is_local_provider_endpoint(&endpoint.effective_base_url);
                if hosted_without_key {
                    return Err(CommandError::user_fixable(
                        "openai_api_key_missing",
                        format!(
                            "Cadence cannot start the owned OpenAI-compatible adapter because provider profile `{}` targets hosted endpoint `{}` without an app-local API key.",
                            active_profile.profile_id, endpoint.effective_base_url
                        ),
                    ));
                }
                Ok(AgentProviderConfig::OpenAiCompatible(
                    OpenAiCompatibleProviderConfig {
                        provider_id: OPENAI_API_PROVIDER_ID.into(),
                        model_id,
                        base_url: endpoint.effective_base_url,
                        api_key,
                        api_version: endpoint.api_version,
                        timeout_ms: 0,
                    },
                ))
            }
        }
        OPENROUTER_PROVIDER_ID => {
            let api_key = runtime_settings
                .provider_api_key
                .clone()
                .or_else(|| runtime_settings.openrouter_api_key.clone())
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "openrouter_api_key_missing",
                        "Cadence cannot start the owned OpenRouter adapter because no OpenRouter API key is configured.",
                    )
                })?;
            Ok(AgentProviderConfig::OpenAiCompatible(
                OpenAiCompatibleProviderConfig {
                    provider_id: OPENROUTER_PROVIDER_ID.into(),
                    model_id,
                    base_url: OPENROUTER_BASE_URL.into(),
                    api_key: Some(api_key),
                    api_version: None,
                    timeout_ms: 0,
                },
            ))
        }
        ANTHROPIC_PROVIDER_ID => {
            let api_key = runtime_settings
                .provider_api_key
                .clone()
                .or_else(|| runtime_settings.anthropic_api_key.clone())
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "anthropic_api_key_missing",
                        format!(
                            "Cadence cannot start the owned Anthropic adapter because provider profile `{}` has no app-local API key.",
                            active_profile.profile_id
                        ),
                    )
                })?;
            Ok(AgentProviderConfig::Anthropic(AnthropicProviderConfig {
                provider_id: ANTHROPIC_PROVIDER_ID.into(),
                model_id,
                api_key,
                ..AnthropicProviderConfig::default()
            }))
        }
        GITHUB_MODELS_PROVIDER_ID | AZURE_OPENAI_PROVIDER_ID | GEMINI_AI_STUDIO_PROVIDER_ID => {
            let endpoint = resolve_openai_compatible_endpoint_for_profile(
                active_profile,
                &state.openai_compatible_auth_config(),
            )
            .map_err(command_error_from_auth)?;
            let api_key = runtime_settings.provider_api_key.clone().ok_or_else(|| {
                CommandError::user_fixable(
                    format!("{}_api_key_missing", active_profile.provider_id),
                    format!(
                        "Cadence cannot start the owned `{}` adapter because provider profile `{}` has no app-local API key.",
                        active_profile.provider_id, active_profile.profile_id
                    ),
                )
            })?;
            Ok(AgentProviderConfig::OpenAiCompatible(
                OpenAiCompatibleProviderConfig {
                    provider_id: active_profile.provider_id.clone(),
                    model_id,
                    base_url: endpoint.effective_base_url,
                    api_key: Some(api_key),
                    api_version: endpoint.api_version,
                    timeout_ms: 0,
                },
            ))
        }
        OLLAMA_PROVIDER_ID => {
            let endpoint = resolve_openai_compatible_endpoint_for_profile(
                active_profile,
                &state.openai_compatible_auth_config(),
            )
            .map_err(command_error_from_auth)?;
            Ok(AgentProviderConfig::OpenAiCompatible(
                OpenAiCompatibleProviderConfig {
                    provider_id: active_profile.provider_id.clone(),
                    model_id,
                    base_url: endpoint.effective_base_url,
                    api_key: runtime_settings.provider_api_key.clone(),
                    api_version: endpoint.api_version,
                    timeout_ms: 0,
                },
            ))
        }
        BEDROCK_PROVIDER_ID => {
            let region = runtime_settings.region.clone().ok_or_else(|| {
                CommandError::user_fixable(
                    "bedrock_region_missing",
                    "Cadence cannot start the owned Bedrock adapter because the active provider profile has no AWS region.",
                )
            })?;
            Ok(AgentProviderConfig::Bedrock(BedrockProviderConfig {
                model_id,
                region,
                timeout_ms: 0,
            }))
        }
        VERTEX_PROVIDER_ID => {
            let region = runtime_settings.region.clone().ok_or_else(|| {
                CommandError::user_fixable(
                    "vertex_region_missing",
                    "Cadence cannot start the owned Vertex AI adapter because the active provider profile has no Google Cloud region.",
                )
            })?;
            let project_id = runtime_settings.project_id.clone().ok_or_else(|| {
                CommandError::user_fixable(
                    "vertex_project_id_missing",
                    "Cadence cannot start the owned Vertex AI adapter because the active provider profile has no Google Cloud project id.",
                )
            })?;
            Ok(AgentProviderConfig::Vertex(VertexProviderConfig {
                model_id,
                region,
                project_id,
                timeout_ms: 0,
            }))
        }
        other => Err(CommandError::user_fixable(
            "owned_agent_provider_unsupported",
            format!("Cadence cannot start the owned agent with unsupported provider `{other}`."),
        )),
    }
}

fn is_openai_responses_model(model_id: &str) -> bool {
    let normalized = model_id.trim().to_ascii_lowercase();
    normalized.contains("codex") || normalized.starts_with("gpt-5")
}

fn is_local_provider_endpoint(base_url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(base_url) else {
        return false;
    };
    parsed
        .host_str()
        .map(|host| {
            matches!(
                host.to_ascii_lowercase().as_str(),
                "localhost" | "127.0.0.1" | "::1"
            )
        })
        .unwrap_or(false)
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

fn requires_matching_provider_for_new_launch(
    snapshot: &crate::db::project_store::RuntimeRunSnapshotRecord,
) -> bool {
    matches!(
        snapshot.run.status,
        crate::db::project_store::RuntimeRunStatus::Starting
            | crate::db::project_store::RuntimeRunStatus::Running
            | crate::db::project_store::RuntimeRunStatus::Stale
    )
}

fn is_reconnectable_owned_runtime_run(
    snapshot: &crate::db::project_store::RuntimeRunSnapshotRecord,
) -> bool {
    snapshot.run.supervisor_kind == OWNED_AGENT_SUPERVISOR_KIND
        && matches!(
            snapshot.run.status,
            crate::db::project_store::RuntimeRunStatus::Starting
                | crate::db::project_store::RuntimeRunStatus::Running
        )
}

fn resolve_owned_runtime_run_control_state(
    active_profile: &ActiveProviderProfileSelection,
    requested_controls: Option<&RuntimeRunControlInputDto>,
    initial_prompt: Option<&str>,
) -> CommandResult<RuntimeRunControlStateRecord> {
    let model_id = requested_controls
        .map(|controls| controls.model_id.clone())
        .unwrap_or_else(|| active_profile.model_id.clone());
    let thinking_effort = requested_controls.and_then(|controls| controls.thinking_effort.clone());
    let approval_mode = requested_controls
        .map(|controls| controls.approval_mode.clone())
        .unwrap_or(RuntimeRunApprovalModeDto::Yolo);
    let plan_mode_required = requested_controls
        .map(|controls| controls.plan_mode_required)
        .unwrap_or(false);

    build_runtime_run_control_state_with_profile(
        Some(&active_profile.profile_id),
        &model_id,
        thinking_effort,
        approval_mode,
        plan_mode_required,
        &now_timestamp(),
        initial_prompt,
    )
}

#[allow(clippy::too_many_arguments)]
fn persist_owned_runtime_run(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: &str,
    provider_id: &str,
    run_controls: &RuntimeRunControlStateRecord,
    status: RuntimeRunStatus,
    last_error: Option<RuntimeRunDiagnosticRecord>,
    checkpoint_summary: &str,
    checkpoint_sequence: u32,
    existing: Option<&RuntimeRunSnapshotRecord>,
) -> CommandResult<RuntimeRunSnapshotRecord> {
    let now = now_timestamp();
    let started_at = existing
        .map(|snapshot| snapshot.run.started_at.clone())
        .unwrap_or_else(|| now.clone());
    let stopped_at = matches!(status, RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed)
        .then_some(now.clone());

    project_store::upsert_runtime_run(
        repo_root,
        &RuntimeRunUpsertRecord {
            run: RuntimeRunRecord {
                project_id: project_id.into(),
                agent_session_id: agent_session_id.into(),
                run_id: run_id.into(),
                runtime_kind: OWNED_AGENT_RUNTIME_KIND.into(),
                provider_id: provider_id.into(),
                supervisor_kind: OWNED_AGENT_SUPERVISOR_KIND.into(),
                status,
                transport: RuntimeRunTransportRecord {
                    kind: "internal".into(),
                    endpoint: "cadence://owned-agent".into(),
                    liveness: RuntimeRunTransportLiveness::Reachable,
                },
                started_at,
                last_heartbeat_at: Some(now.clone()),
                stopped_at,
                last_error,
                updated_at: now.clone(),
            },
            checkpoint: Some(RuntimeRunCheckpointRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                sequence: checkpoint_sequence,
                kind: RuntimeRunCheckpointKind::Bootstrap,
                summary: checkpoint_summary.into(),
                created_at: now,
            }),
            control_state: Some(run_controls.clone()),
        },
    )
}

pub(crate) fn stop_owned_runtime_run(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
) -> CommandResult<RuntimeRunSnapshotRecord> {
    persist_owned_runtime_run(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.agent_session_id,
        &snapshot.run.run_id,
        &snapshot.run.provider_id,
        &snapshot.controls,
        RuntimeRunStatus::Stopped,
        None,
        "Owned agent runtime stopped.",
        snapshot.last_checkpoint_sequence.saturating_add(1),
        Some(snapshot),
    )
}

pub(crate) fn update_owned_runtime_run_controls(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    controls: Option<RuntimeRunControlInputDto>,
    prompt: Option<String>,
) -> CommandResult<RuntimeRunSnapshotRecord> {
    let active = &snapshot.controls.active;
    let model_id = controls
        .as_ref()
        .map(|controls| controls.model_id.clone())
        .unwrap_or_else(|| active.model_id.clone());
    let provider_profile_id = controls
        .as_ref()
        .and_then(|controls| controls.provider_profile_id.as_deref())
        .or(active.provider_profile_id.as_deref());
    let thinking_effort = controls
        .as_ref()
        .and_then(|controls| controls.thinking_effort.clone())
        .or_else(|| active.thinking_effort.clone());
    let approval_mode = controls
        .as_ref()
        .map(|controls| controls.approval_mode.clone())
        .unwrap_or_else(|| active.approval_mode.clone());
    let plan_mode_required = controls
        .as_ref()
        .map(|controls| controls.plan_mode_required)
        .unwrap_or(active.plan_mode_required);
    let run_controls = build_runtime_run_control_state_with_profile(
        provider_profile_id,
        &model_id,
        thinking_effort,
        approval_mode,
        plan_mode_required,
        &now_timestamp(),
        prompt.as_deref(),
    )?;

    persist_owned_runtime_run(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.agent_session_id,
        &snapshot.run.run_id,
        &snapshot.run.provider_id,
        &run_controls,
        snapshot.run.status.clone(),
        snapshot.run.last_error.clone(),
        "Owned agent runtime controls updated.",
        snapshot.last_checkpoint_sequence.saturating_add(1),
        Some(snapshot),
    )
}

fn load_provider_profile_selection<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    requested_controls: Option<&RuntimeRunControlInputDto>,
    require_ready_profile: bool,
) -> CommandResult<ActiveProviderProfileSelection> {
    let provider_profiles = load_provider_credentials_view(app, state)?;
    let requested_profile_id = requested_controls
        .and_then(|controls| controls.provider_profile_id.as_deref())
        .map(str::trim)
        .filter(|profile_id| !profile_id.is_empty());
    let active_profile = match requested_profile_id {
        Some(profile_id) => provider_profiles.profile(profile_id).ok_or_else(|| {
            CommandError::user_fixable(
                "provider_profile_not_found",
                format!(
                    "Cadence could not determine the selected provider profile `{profile_id}` before launching or reconnecting a runtime run.",
                ),
            )
        })?,
        None => provider_profiles.active_profile().ok_or_else(|| {
            CommandError::user_fixable(
                "provider_profiles_invalid",
                "Cadence could not determine the active provider profile before launching or reconnecting a runtime run.",
            )
        })?,
    };

    if !require_ready_profile {
        return Ok(ActiveProviderProfileSelection {
            profile_id: active_profile.profile_id.clone(),
            provider_id: active_profile.provider_id.clone(),
            model_id: active_profile.model_id.clone(),
        });
    }

    if active_profile.provider_id == OPENAI_CODEX_PROVIDER_ID {
        let auth_store_path = state
            .auth_store_file_for_provider(app, openai_codex_provider())
            .map_err(command_error_from_auth)?;
        let session = match active_profile.credential_link.as_ref() {
            Some(link) => load_openai_codex_session_for_profile_link(&auth_store_path, link)
                .map_err(command_error_from_auth)?,
            None => load_latest_openai_codex_session(&auth_store_path)
                .map_err(command_error_from_auth)?,
        };
        if session.is_none() {
            return Err(CommandError::user_fixable(
                "provider_profile_not_ready",
                format!(
                    "Cadence cannot launch a runtime run with provider profile `{}` because global OpenAI auth is not ready.",
                    active_profile.profile_id
                ),
            ));
        }
    } else if !active_profile.readiness().ready {
        return Err(CommandError::user_fixable(
            "provider_profile_not_ready",
            format!(
                "Cadence cannot launch a runtime run with provider profile `{}` because it is not ready.",
                active_profile.profile_id
            ),
        ));
    }

    Ok(ActiveProviderProfileSelection {
        profile_id: active_profile.profile_id.clone(),
        provider_id: active_profile.provider_id.clone(),
        model_id: active_profile.model_id.clone(),
    })
}

fn runtime_supervisor_existing_run_provider_mismatch(
    active_profile: &ActiveProviderProfileSelection,
    snapshot: &crate::db::project_store::RuntimeRunSnapshotRecord,
) -> CommandError {
    CommandError::user_fixable(
        "runtime_supervisor_provider_mismatch",
        format!(
            "Cadence cannot launch runtime run `{}` because active provider profile `{}` targets `{}` while durable runtime run `{}` is still attributable to `{}`. Reconnect or stop the existing run before switching providers.",
            active_profile.model_id,
            active_profile.profile_id,
            active_profile.provider_id,
            snapshot.run.run_id,
            snapshot.run.provider_id,
        ),
    )
}

fn runtime_supervisor_session_provider_mismatch(
    active_profile: &ActiveProviderProfileSelection,
    runtime: &crate::commands::RuntimeSessionDto,
) -> CommandError {
    CommandError::user_fixable(
        "runtime_supervisor_provider_mismatch",
        format!(
            "Cadence cannot launch runtime run `{}` because active provider profile `{}` targets `{}` while the authenticated runtime session is still bound to `{}`. Rebind the runtime session or switch back to the matching provider profile.",
            active_profile.model_id,
            active_profile.profile_id,
            active_profile.provider_id,
            runtime.provider_id,
        ),
    )
}

fn reject_runtime_run_provider_profile_switch(
    snapshot: &RuntimeRunSnapshotRecord,
    requested_controls: Option<&RuntimeRunControlInputDto>,
) -> CommandResult<()> {
    let requested_profile_id = requested_controls
        .and_then(|controls| controls.provider_profile_id.as_deref())
        .map(str::trim)
        .filter(|profile_id| !profile_id.is_empty());
    let active_profile_id = snapshot
        .controls
        .active
        .provider_profile_id
        .as_deref()
        .map(str::trim)
        .filter(|profile_id| !profile_id.is_empty());

    if let (Some(requested), Some(active)) = (requested_profile_id, active_profile_id) {
        if requested != active {
            return Err(CommandError::user_fixable(
                "runtime_run_provider_profile_switch_blocked",
                format!(
                    "Cadence cannot switch active runtime run `{}` from provider profile `{active}` to `{requested}`. Stop the current run before changing providers.",
                    snapshot.run.run_id
                ),
            ));
        }
    }

    Ok(())
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
    active_profile: &ActiveProviderProfileSelection,
    requested_controls: Option<&RuntimeRunControlInputDto>,
    initial_prompt: Option<&str>,
) -> CommandResult<RuntimeRunControlStateRecord> {
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
    let plan_mode_required = requested_controls
        .map(|controls| controls.plan_mode_required)
        .unwrap_or(false);
    let timestamp = crate::auth::now_timestamp();

    build_runtime_run_control_state_with_profile(
        Some(&active_profile.profile_id),
        &model_id,
        thinking_effort,
        approval_mode,
        plan_mode_required,
        &timestamp,
        initial_prompt,
    )
}

fn prepare_runtime_supervisor_launch<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    selected_profile: &ActiveProviderProfileSelection,
    runtime: &crate::commands::RuntimeSessionDto,
    session_id: &str,
    run_controls: &RuntimeRunControlStateRecord,
    run_id: &str,
) -> CommandResult<PreparedRuntimeSupervisorLaunch> {
    let provider_profiles = load_provider_credentials_view(app, state)?;
    let active_profile = provider_profiles
        .profile(&selected_profile.profile_id)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "provider_profile_not_found",
                format!(
                "Cadence could not launch a runtime run because provider profile `{}` is missing.",
                selected_profile.profile_id
            ),
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
    launch_env.insert(
        crate::runtime::supervisor::CADENCE_GLOBAL_DB_PATH_ENV,
        state.global_db_path(app)?.to_string_lossy().to_string(),
    );
    if matches!(
        runtime.provider_id.as_str(),
        ANTHROPIC_PROVIDER_ID | BEDROCK_PROVIDER_ID | VERTEX_PROVIDER_ID
    ) {
        if runtime.provider_id == ANTHROPIC_PROVIDER_ID {
            match active_profile.readiness().status {
                ProviderCredentialReadinessStatus::Ready => {}
                ProviderCredentialReadinessStatus::Missing => {
                    return Err(CommandError::user_fixable(
                        "anthropic_api_key_missing",
                        format!(
                            "Cadence cannot launch the detached Anthropic runtime because provider profile `{}` has no app-local API key configured.",
                            active_profile.profile_id
                        ),
                    ));
                }
                ProviderCredentialReadinessStatus::Malformed => {
                    return Err(CommandError::user_fixable(
                        "provider_profile_credentials_unavailable",
                        format!(
                            "Cadence cannot launch the detached Anthropic runtime because provider profile `{}` no longer matches the saved app-local secret state.",
                            active_profile.profile_id
                        ),
                    ));
                }
            }
        }

        let runtime_settings =
            runtime_settings_snapshot_for_provider_profile(&provider_profiles, active_profile)
                .map_err(|error| CommandError::user_fixable(error.code, error.message))?;
        let profile_input = AnthropicFamilyProfileInput::from(&runtime_settings);
        let launch_vars =
            resolve_anthropic_family_launch_env(&profile_input).map_err(command_error_from_auth)?;
        for (key, value) in launch_vars {
            launch_env.insert(key, value);
        }
    } else if matches!(
        runtime.provider_id.as_str(),
        OPENAI_API_PROVIDER_ID
            | OLLAMA_PROVIDER_ID
            | AZURE_OPENAI_PROVIDER_ID
            | GITHUB_MODELS_PROVIDER_ID
            | GEMINI_AI_STUDIO_PROVIDER_ID
    ) {
        let endpoint = resolve_openai_compatible_endpoint_for_profile(
            active_profile,
            &state.openai_compatible_auth_config(),
        )
        .map_err(command_error_from_auth)?;
        let readiness = active_profile.readiness();
        let api_key = match readiness.status {
            ProviderCredentialReadinessStatus::Ready => provider_profiles
                .matched_api_key_credential_for_profile(&active_profile.profile_id)
                .map(|secret| secret.api_key.as_str()),
            ProviderCredentialReadinessStatus::Missing => None,
            ProviderCredentialReadinessStatus::Malformed => {
                return Err(CommandError::user_fixable(
                    "provider_profile_credentials_unavailable",
                    format!(
                        "Cadence cannot launch the detached {} runtime because provider profile `{}` no longer matches the saved app-local secret state.",
                        runtime.provider_id, active_profile.profile_id
                    ),
                ));
            }
        };

        let env = resolve_openai_compatible_launch_env(api_key, &endpoint)
            .map_err(command_error_from_auth)?;
        if let Some(api_key) = env.api_key {
            launch_env.insert("OPENAI_API_KEY", api_key);
        }
        launch_env.insert("OPENAI_BASE_URL", env.base_url);
        if let Some(api_version) = env.api_version {
            launch_env.insert("OPENAI_API_VERSION", api_version);
        }
    }

    let mcp_registry_path = state.mcp_registry_file(app)?;
    let mcp_projection_root = state
        .app_data_dir(app)?
        .join(RUNTIME_MCP_PROJECTION_DIRECTORY_NAME);
    let mcp_projection = materialize_runtime_mcp_projection_for_run(
        &mcp_registry_path,
        &mcp_projection_root,
        run_id,
    )
    .map_err(|error| {
        let message = format!(
            "Cadence rejected detached runtime launch because MCP projection could not be built safely: {} ({})",
            error.message, error.code
        );
        if error.retryable {
            CommandError::retryable("runtime_mcp_projection_failed", message)
        } else {
            CommandError::user_fixable("runtime_mcp_projection_failed", message)
        }
    })?;

    launch_env.insert(
        crate::runtime::supervisor::CADENCE_RUNTIME_MCP_CONFIG_PATH_ENV,
        mcp_projection.projection_path.to_string_lossy().to_string(),
    );
    launch_env.insert(
        crate::runtime::supervisor::CADENCE_RUNTIME_MCP_CONTRACT_REQUIRED_ENV,
        "1",
    );

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
            provider_profile_id: controls.active.provider_profile_id.clone(),
            model_id: controls.active.model_id.clone(),
            thinking_effort: controls.active.thinking_effort.clone(),
            approval_mode: controls.active.approval_mode.clone(),
            plan_mode_required: controls.active.plan_mode_required,
            revision: controls.active.revision,
            applied_at: controls.active.applied_at.clone(),
        },
        pending: controls
            .pending
            .as_ref()
            .map(|pending| RuntimeRunPendingControlSnapshotDto {
                provider_profile_id: pending.provider_profile_id.clone(),
                model_id: pending.model_id.clone(),
                thinking_effort: pending.thinking_effort.clone(),
                approval_mode: pending.approval_mode.clone(),
                plan_mode_required: pending.plan_mode_required,
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
