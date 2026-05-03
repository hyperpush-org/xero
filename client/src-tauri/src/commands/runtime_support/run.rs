use std::{path::Path, path::PathBuf, thread};

use rand::RngCore;
use tauri::{AppHandle, Emitter, Runtime};

use crate::{
    auth::{
        load_latest_openai_codex_session, load_openai_codex_session,
        load_openai_codex_session_for_profile_link, now_timestamp,
        openai_compatible::resolve_openai_compatible_endpoint_for_profile,
        refresh_provider_auth_session, StoredOpenAiCodexSession,
    },
    commands::{
        default_runtime_agent_id,
        get_runtime_settings::runtime_settings_snapshot_for_provider_profile,
        provider_credentials::load_provider_credentials_view, CommandError, CommandResult,
        RuntimeRunActiveControlSnapshotDto, RuntimeRunCheckpointDto, RuntimeRunCheckpointKindDto,
        RuntimeRunControlInputDto, RuntimeRunControlStateDto, RuntimeRunDiagnosticDto,
        RuntimeRunDto, RuntimeRunPendingControlSnapshotDto, RuntimeRunStatusDto,
        RuntimeRunTransportDto, RuntimeRunTransportLivenessDto, RuntimeRunUpdatedPayloadDto,
        RUNTIME_RUN_UPDATED_EVENT,
    },
    db::project_store::{
        self, build_runtime_run_control_state_with_profile, RuntimeRunActiveControlSnapshotRecord,
        RuntimeRunCheckpointKind, RuntimeRunCheckpointRecord, RuntimeRunControlStateRecord,
        RuntimeRunDiagnosticRecord, RuntimeRunPendingControlSnapshotRecord, RuntimeRunRecord,
        RuntimeRunSnapshotRecord, RuntimeRunStatus, RuntimeRunTransportLiveness,
        RuntimeRunTransportRecord, RuntimeRunUpsertRecord,
    },
    runtime::{
        create_owned_agent_run, drive_owned_agent_run, normalize_openai_codex_model_id,
        AgentProviderConfig, AnthropicProviderConfig, AutonomousToolRuntime, BedrockProviderConfig,
        OpenAiCodexResponsesProviderConfig, OpenAiCompatibleProviderConfig,
        OpenAiResponsesProviderConfig, OwnedAgentRunRequest, RuntimeProvider, VertexProviderConfig,
        ANTHROPIC_PROVIDER_ID, AZURE_OPENAI_PROVIDER_ID, BEDROCK_PROVIDER_ID,
        GEMINI_AI_STUDIO_PROVIDER_ID, GITHUB_MODELS_PROVIDER_ID, OLLAMA_PROVIDER_ID,
        OPENAI_API_PROVIDER_ID, OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID,
        OWNED_AGENT_RUNTIME_KIND, OWNED_AGENT_SUPERVISOR_KIND, VERTEX_PROVIDER_ID,
    },
    state::DesktopState,
};

use super::{project::resolve_project_root, session::command_error_from_auth};

const DEFAULT_OPENAI_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api";
const OPENAI_CODEX_REFRESH_SKEW_SECONDS: i64 = 60;
const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

pub(crate) struct RuntimeRunLaunchOutcome {
    pub repo_root: PathBuf,
    pub snapshot: RuntimeRunSnapshotRecord,
    pub reconnected: bool,
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
    _state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> CommandResult<Option<RuntimeRunSnapshotRecord>> {
    Ok(
        load_persisted_runtime_run(repo_root, project_id, agent_session_id)?
            .filter(|snapshot| snapshot.run.supervisor_kind == OWNED_AGENT_SUPERVISOR_KIND),
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
                "Xero updated durable runtime-run metadata but could not emit the runtime-run update event: {error}"
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
                "Xero updated durable runtime-run metadata but could not emit the runtime-run update event: {error}"
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
    initial_attachments: Vec<crate::commands::StagedAgentAttachmentDto>,
) -> CommandResult<RuntimeRunLaunchOutcome> {
    launch_owned_runtime_run(
        app,
        state,
        project_id,
        agent_session_id,
        requested_controls,
        initial_prompt,
        initial_attachments,
    )
}

fn launch_owned_runtime_run<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    agent_session_id: &str,
    requested_controls: Option<RuntimeRunControlInputDto>,
    initial_prompt: Option<String>,
    initial_attachments: Vec<crate::commands::StagedAgentAttachmentDto>,
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

    let active_profile =
        resolve_owned_runtime_profile_selection(app, state, requested_controls.as_ref())?;
    let requested_agent_id = requested_controls
        .as_ref()
        .map(|controls| controls.runtime_agent_id)
        .unwrap_or_else(default_runtime_agent_id);
    let definition_selection = project_store::resolve_agent_definition_for_run(
        &repo_root,
        requested_controls
            .as_ref()
            .and_then(|controls| controls.agent_definition_id.as_deref()),
        requested_agent_id,
    )?;
    let run_controls = resolve_owned_runtime_run_control_state(
        &active_profile,
        &definition_selection,
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
            attachments: initial_attachments
                .iter()
                .map(staged_attachment_dto_to_message_attachment)
                .collect(),
            controls: Some(runtime_control_input_from_active(&run_controls.active)),
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
        snapshot = apply_owned_runtime_run_pending_controls(
            &repo_root,
            &snapshot,
            "Owned agent runtime accepted the initial prompt.",
        )?;
        let runtime_run = runtime_run_dto_from_snapshot(&snapshot);
        emit_runtime_run_updated(app, Some(&runtime_run))?;

        let run_controls_for_task = snapshot.controls.clone();
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
                "provider_not_found",
                format!(
                    "Xero could not resolve the owned-agent provider because provider `{profile_id}` is missing.",
                ),
            )
        })?,
        None => provider_profiles.active_profile().ok_or_else(|| {
            CommandError::user_fixable(
                "provider_credentials_invalid",
                "Xero could not resolve the owned-agent provider because the selected provider is missing.",
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
            let auth_store_path = state.global_db_path(app)?;
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
                            "Xero cannot start the owned OpenAI Codex adapter because no global app-local auth session is available for provider `{}`.",
                            active_profile.provider_id
                        ),
                    )
                })?;
            let session = refresh_openai_codex_session_before_run(app, state, session)?;
            Ok(openai_codex_provider_config_from_session(
                model_id.as_str(),
                session,
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
                            "Xero cannot start the owned OpenAI-compatible adapter because provider `{}` targets hosted endpoint `{}` without an app-local API key.",
                            active_profile.provider_id, endpoint.effective_base_url
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
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "openrouter_api_key_missing",
                        "Xero cannot start the owned OpenRouter adapter because no OpenRouter API key is configured.",
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
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "anthropic_api_key_missing",
                        format!(
                            "Xero cannot start the owned Anthropic adapter because provider `{}` has no app-local API key.",
                            active_profile.provider_id
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
                        "Xero cannot start the owned `{}` adapter because provider `{}` has no app-local API key.",
                        active_profile.provider_id, active_profile.provider_id
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
                    "Xero cannot start the owned Bedrock adapter because the selected provider has no AWS region.",
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
                    "Xero cannot start the owned Vertex AI adapter because the selected provider has no Google Cloud region.",
                )
            })?;
            let project_id = runtime_settings.project_id.clone().ok_or_else(|| {
                CommandError::user_fixable(
                    "vertex_project_id_missing",
                    "Xero cannot start the owned Vertex AI adapter because the selected provider has no Google Cloud project id.",
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
            format!("Xero cannot start the owned agent with unsupported provider `{other}`."),
        )),
    }
}

fn refresh_openai_codex_session_before_run<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    session: StoredOpenAiCodexSession,
) -> CommandResult<StoredOpenAiCodexSession> {
    if !openai_codex_session_needs_refresh(&session, current_unix_timestamp()) {
        return Ok(session);
    }

    let refreshed = refresh_provider_auth_session(
        app,
        state,
        RuntimeProvider::OpenAiCodex,
        session.account_id.as_str(),
    )
    .map_err(command_error_from_auth)?;
    let auth_store_path = state.global_db_path(app)?;
    load_openai_codex_session(&auth_store_path, refreshed.account_id.as_str())
        .map_err(command_error_from_auth)?
        .ok_or_else(|| {
            CommandError::retryable(
                "openai_codex_auth_refresh_missing",
                "Xero refreshed OpenAI Codex auth, but the refreshed session was not available in the app-local auth store.",
            )
        })
}

fn openai_codex_session_needs_refresh(session: &StoredOpenAiCodexSession, now: i64) -> bool {
    session.expires_at <= now.saturating_add(OPENAI_CODEX_REFRESH_SKEW_SECONDS)
}

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn openai_codex_provider_config_from_session(
    model_id: &str,
    session: StoredOpenAiCodexSession,
) -> AgentProviderConfig {
    AgentProviderConfig::OpenAiCodexResponses(OpenAiCodexResponsesProviderConfig {
        provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
        model_id: normalize_openai_codex_model_id(model_id),
        base_url: DEFAULT_OPENAI_CODEX_BASE_URL.into(),
        access_token: session.access_token,
        account_id: session.account_id,
        session_id: Some(session.session_id),
        timeout_ms: 0,
    })
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
    definition_selection: &project_store::AgentDefinitionRunSelection,
    requested_controls: Option<&RuntimeRunControlInputDto>,
    initial_prompt: Option<&str>,
) -> CommandResult<RuntimeRunControlStateRecord> {
    let model_id = requested_controls
        .map(|controls| controls.model_id.clone())
        .unwrap_or_else(|| active_profile.model_id.clone());
    let thinking_effort = requested_controls.and_then(|controls| controls.thinking_effort.clone());
    let runtime_agent_id = definition_selection.runtime_agent_id;
    let approval_mode = requested_controls
        .map(|controls| controls.approval_mode.clone())
        .filter(|mode| {
            definition_selection
                .allowed_approval_modes
                .iter()
                .any(|allowed| allowed == mode)
        })
        .unwrap_or_else(|| definition_selection.default_approval_mode.clone());
    let plan_mode_required = requested_controls
        .map(|controls| controls.plan_mode_required)
        .unwrap_or(false);

    build_runtime_run_control_state_with_profile(
        runtime_agent_id,
        Some(&definition_selection.definition_id),
        Some(definition_selection.version),
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
                    endpoint: "xero://owned-agent".into(),
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

fn runtime_control_input_from_active(
    active: &RuntimeRunActiveControlSnapshotRecord,
) -> RuntimeRunControlInputDto {
    RuntimeRunControlInputDto {
        runtime_agent_id: active.runtime_agent_id,
        agent_definition_id: active.agent_definition_id.clone(),
        provider_profile_id: active.provider_profile_id.clone(),
        model_id: active.model_id.clone(),
        thinking_effort: active.thinking_effort.clone(),
        approval_mode: active.approval_mode.clone(),
        plan_mode_required: active.plan_mode_required,
    }
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
    if let Some(requested_agent_id) = controls
        .as_ref()
        .map(|controls| controls.runtime_agent_id)
        .filter(|agent_id| agent_id != &active.runtime_agent_id)
    {
        return Err(CommandError::user_fixable(
            "runtime_agent_switch_blocked",
            format!(
                "Xero cannot switch active runtime run `{}` from {} to {}. Stop the current run before changing agents.",
                snapshot.run.run_id,
                active.runtime_agent_id.label(),
                requested_agent_id.label()
            ),
        ));
    }
    let base_pending = snapshot.controls.pending.as_ref();
    let model_id = controls
        .as_ref()
        .map(|controls| controls.model_id.clone())
        .or_else(|| base_pending.map(|pending| pending.model_id.clone()))
        .unwrap_or_else(|| active.model_id.clone());
    let provider_profile_id = controls
        .as_ref()
        .and_then(|controls| controls.provider_profile_id.as_deref())
        .or_else(|| base_pending.and_then(|pending| pending.provider_profile_id.as_deref()))
        .or(active.provider_profile_id.as_deref());
    let thinking_effort = controls
        .as_ref()
        .and_then(|controls| controls.thinking_effort.clone())
        .or_else(|| base_pending.and_then(|pending| pending.thinking_effort.clone()))
        .or_else(|| active.thinking_effort.clone());
    let approval_mode = controls
        .as_ref()
        .map(|controls| controls.approval_mode.clone())
        .or_else(|| base_pending.map(|pending| pending.approval_mode.clone()))
        .unwrap_or_else(|| active.approval_mode.clone());
    let runtime_agent_id = active.runtime_agent_id;
    let plan_mode_required = controls
        .as_ref()
        .map(|controls| controls.plan_mode_required)
        .or_else(|| base_pending.map(|pending| pending.plan_mode_required))
        .unwrap_or(active.plan_mode_required);
    let queued_prompt = prompt
        .as_deref()
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| base_pending.and_then(|pending| pending.queued_prompt.clone()));
    let now = now_timestamp();
    let queued_prompt_at = if queued_prompt.is_some() {
        prompt
            .as_deref()
            .map(str::trim)
            .filter(|prompt| !prompt.is_empty())
            .map(|_| now.clone())
            .or_else(|| base_pending.and_then(|pending| pending.queued_prompt_at.clone()))
            .or_else(|| Some(now.clone()))
    } else {
        None
    };
    let next_revision = base_pending
        .map(|pending| pending.revision)
        .unwrap_or(active.revision)
        .max(active.revision)
        .saturating_add(1);
    let run_controls = RuntimeRunControlStateRecord {
        active: active.clone(),
        pending: Some(RuntimeRunPendingControlSnapshotRecord {
            runtime_agent_id,
            agent_definition_id: active.agent_definition_id.clone(),
            agent_definition_version: active.agent_definition_version,
            provider_profile_id: provider_profile_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            model_id,
            thinking_effort,
            approval_mode,
            plan_mode_required,
            revision: next_revision,
            queued_at: now,
            queued_prompt,
            queued_prompt_at,
        }),
    };

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

pub(crate) fn apply_owned_runtime_run_pending_controls(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    checkpoint_summary: &str,
) -> CommandResult<RuntimeRunSnapshotRecord> {
    let Some(pending) = snapshot.controls.pending.as_ref() else {
        return Ok(snapshot.clone());
    };

    let run_controls = RuntimeRunControlStateRecord {
        active: RuntimeRunActiveControlSnapshotRecord {
            runtime_agent_id: pending.runtime_agent_id,
            agent_definition_id: pending.agent_definition_id.clone(),
            agent_definition_version: pending.agent_definition_version,
            provider_profile_id: pending.provider_profile_id.clone(),
            model_id: pending.model_id.clone(),
            thinking_effort: pending.thinking_effort.clone(),
            approval_mode: pending.approval_mode.clone(),
            plan_mode_required: pending.plan_mode_required,
            revision: pending.revision,
            applied_at: now_timestamp(),
        },
        pending: None,
    };

    persist_owned_runtime_run(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.agent_session_id,
        &snapshot.run.run_id,
        &snapshot.run.provider_id,
        &run_controls,
        snapshot.run.status.clone(),
        snapshot.run.last_error.clone(),
        checkpoint_summary,
        snapshot.last_checkpoint_sequence.saturating_add(1),
        Some(snapshot),
    )
}

pub(crate) fn bind_owned_runtime_run_to_agent_handoff(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    target: &project_store::AgentRunSnapshotRecord,
) -> CommandResult<RuntimeRunSnapshotRecord> {
    let next_active = match snapshot.controls.pending.as_ref() {
        Some(pending) => RuntimeRunActiveControlSnapshotRecord {
            runtime_agent_id: target.run.runtime_agent_id,
            agent_definition_id: Some(target.run.agent_definition_id.clone()),
            agent_definition_version: Some(target.run.agent_definition_version),
            provider_profile_id: pending.provider_profile_id.clone(),
            model_id: pending.model_id.clone(),
            thinking_effort: pending.thinking_effort.clone(),
            approval_mode: pending.approval_mode.clone(),
            plan_mode_required: target.run.runtime_agent_id.allows_plan_gate()
                && pending.plan_mode_required,
            revision: pending.revision,
            applied_at: now_timestamp(),
        },
        None => RuntimeRunActiveControlSnapshotRecord {
            runtime_agent_id: target.run.runtime_agent_id,
            agent_definition_id: Some(target.run.agent_definition_id.clone()),
            agent_definition_version: Some(target.run.agent_definition_version),
            provider_profile_id: snapshot.controls.active.provider_profile_id.clone(),
            model_id: snapshot.controls.active.model_id.clone(),
            thinking_effort: snapshot.controls.active.thinking_effort.clone(),
            approval_mode: snapshot.controls.active.approval_mode.clone(),
            plan_mode_required: target.run.runtime_agent_id.allows_plan_gate()
                && snapshot.controls.active.plan_mode_required,
            revision: snapshot.controls.active.revision.saturating_add(1),
            applied_at: now_timestamp(),
        },
    };
    let run_controls = RuntimeRunControlStateRecord {
        active: next_active,
        pending: None,
    };
    persist_owned_runtime_run(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.agent_session_id,
        &target.run.run_id,
        &snapshot.run.provider_id,
        &run_controls,
        snapshot.run.status.clone(),
        snapshot.run.last_error.clone(),
        "Owned agent runtime handed off to a same-type target run.",
        snapshot.last_checkpoint_sequence.saturating_add(1),
        Some(snapshot),
    )
}

fn resolve_owned_runtime_profile_selection<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    requested_controls: Option<&RuntimeRunControlInputDto>,
) -> CommandResult<ActiveProviderProfileSelection> {
    if let Some(config) = state.owned_agent_provider_config_override() {
        return Ok(active_profile_selection_from_override(
            config,
            requested_controls,
        ));
    }

    load_provider_profile_selection(app, state, requested_controls, true)
}

fn active_profile_selection_from_override(
    config: AgentProviderConfig,
    requested_controls: Option<&RuntimeRunControlInputDto>,
) -> ActiveProviderProfileSelection {
    let provider_id = match &config {
        AgentProviderConfig::Fake => OPENAI_CODEX_PROVIDER_ID.to_string(),
        AgentProviderConfig::OpenAiResponses(config) => config.provider_id.clone(),
        AgentProviderConfig::OpenAiCodexResponses(config) => config.provider_id.clone(),
        AgentProviderConfig::OpenAiCompatible(config) => config.provider_id.clone(),
        AgentProviderConfig::Anthropic(config) => config.provider_id.clone(),
        AgentProviderConfig::Bedrock(_) => BEDROCK_PROVIDER_ID.to_string(),
        AgentProviderConfig::Vertex(_) => VERTEX_PROVIDER_ID.to_string(),
    };
    let model_id = requested_controls
        .map(|controls| controls.model_id.trim().to_owned())
        .filter(|model_id| !model_id.is_empty())
        .or_else(|| match config {
            AgentProviderConfig::Fake => Some("test-model".to_string()),
            AgentProviderConfig::OpenAiResponses(config) => Some(config.model_id),
            AgentProviderConfig::OpenAiCodexResponses(config) => Some(config.model_id),
            AgentProviderConfig::OpenAiCompatible(config) => Some(config.model_id),
            AgentProviderConfig::Anthropic(config) => Some(config.model_id),
            AgentProviderConfig::Bedrock(config) => Some(config.model_id),
            AgentProviderConfig::Vertex(config) => Some(config.model_id),
        })
        .unwrap_or_else(|| "model-unavailable".to_string());
    let profile_id = requested_controls
        .and_then(|controls| controls.provider_profile_id.as_deref())
        .map(str::trim)
        .filter(|profile_id| !profile_id.is_empty())
        .unwrap_or("owned-agent-override")
        .to_string();

    ActiveProviderProfileSelection {
        profile_id,
        provider_id,
        model_id,
    }
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
                "provider_not_found",
                format!(
                    "Xero could not determine the selected provider `{profile_id}` before launching or reconnecting a runtime run.",
                ),
            )
        })?,
        None => provider_profiles.active_profile().ok_or_else(|| {
            CommandError::user_fixable(
                "provider_credentials_invalid",
                "Xero could not determine a provider before launching or reconnecting a runtime run.",
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
        let auth_store_path = state.global_db_path(app)?;
        let session = match active_profile.credential_link.as_ref() {
            Some(link) => load_openai_codex_session_for_profile_link(&auth_store_path, link)
                .map_err(command_error_from_auth)?,
            None => load_latest_openai_codex_session(&auth_store_path)
                .map_err(command_error_from_auth)?,
        };
        if session.is_none() {
            return Err(CommandError::user_fixable(
                "provider_not_ready",
                format!(
                    "Xero cannot launch a runtime run with provider `{}` because global OpenAI auth is not ready.",
                    active_profile.provider_id
                ),
            ));
        }
    } else if !active_profile.readiness().ready {
        return Err(CommandError::user_fixable(
            "provider_not_ready",
            format!(
                "Xero cannot launch a runtime run with provider `{}` because it is not ready.",
                active_profile.provider_id
            ),
        ));
    }

    Ok(ActiveProviderProfileSelection {
        profile_id: active_profile.profile_id.clone(),
        provider_id: active_profile.provider_id.clone(),
        model_id: active_profile.model_id.clone(),
    })
}

fn reject_runtime_run_provider_profile_switch(
    snapshot: &RuntimeRunSnapshotRecord,
    requested_controls: Option<&RuntimeRunControlInputDto>,
) -> CommandResult<()> {
    if let Some(requested_agent_id) = requested_controls
        .map(|controls| controls.runtime_agent_id)
        .filter(|agent_id| agent_id != &snapshot.controls.active.runtime_agent_id)
    {
        return Err(CommandError::user_fixable(
            "runtime_agent_switch_blocked",
            format!(
                "Xero cannot reconnect active runtime run `{}` as {} because it was started as {}. Stop the current run before changing agents.",
                snapshot.run.run_id,
                requested_agent_id.label(),
                snapshot.controls.active.runtime_agent_id.label()
            ),
        ));
    }

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
                "runtime_run_provider_switch_blocked",
                format!(
                    "Xero cannot switch active runtime run `{}` from provider `{active}` to `{requested}`. Stop the current run before changing providers.",
                    snapshot.run.run_id
                ),
            ));
        }
    }

    Ok(())
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

fn runtime_run_control_state_dto(
    controls: &RuntimeRunControlStateRecord,
) -> RuntimeRunControlStateDto {
    RuntimeRunControlStateDto {
        active: RuntimeRunActiveControlSnapshotDto {
            runtime_agent_id: controls.active.runtime_agent_id,
            agent_definition_id: controls.active.agent_definition_id.clone(),
            agent_definition_version: controls.active.agent_definition_version,
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
                runtime_agent_id: pending.runtime_agent_id,
                agent_definition_id: pending.agent_definition_id.clone(),
                agent_definition_version: pending.agent_definition_version,
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

pub(crate) fn staged_attachment_dto_to_message_attachment(
    dto: &crate::commands::StagedAgentAttachmentDto,
) -> crate::runtime::MessageAttachment {
    crate::runtime::MessageAttachment {
        kind: match dto.kind {
            crate::commands::AgentAttachmentKindDto::Image => {
                crate::runtime::MessageAttachmentKind::Image
            }
            crate::commands::AgentAttachmentKindDto::Document => {
                crate::runtime::MessageAttachmentKind::Document
            }
            crate::commands::AgentAttachmentKindDto::Text => {
                crate::runtime::MessageAttachmentKind::Text
            }
        },
        absolute_path: std::path::PathBuf::from(&dto.absolute_path),
        media_type: dto.media_type.clone(),
        original_name: dto.original_name.clone(),
        size_bytes: dto.size_bytes,
        width: dto.width,
        height: dto.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored_codex_session(expires_at: i64) -> StoredOpenAiCodexSession {
        StoredOpenAiCodexSession {
            provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
            session_id: "session-123".into(),
            account_id: "acct_123".into(),
            access_token: "oauth-access-token".into(),
            refresh_token: "oauth-refresh-token".into(),
            expires_at,
            updated_at: "2026-04-29T18:14:27Z".into(),
        }
    }

    #[test]
    fn openai_codex_provider_config_uses_chatgpt_backend_contract() {
        let config =
            openai_codex_provider_config_from_session("gpt-5.5", stored_codex_session(100));

        match config {
            AgentProviderConfig::OpenAiCodexResponses(config) => {
                assert_eq!(config.provider_id, OPENAI_CODEX_PROVIDER_ID);
                assert_eq!(config.model_id, "gpt-5.5");
                assert_eq!(config.base_url, DEFAULT_OPENAI_CODEX_BASE_URL);
                assert_eq!(config.access_token, "oauth-access-token");
                assert_eq!(config.account_id, "acct_123");
                assert_eq!(config.session_id.as_deref(), Some("session-123"));
            }
            other => panic!("expected OpenAiCodexResponses config, got {other:?}"),
        }
    }

    #[test]
    fn openai_codex_session_refresh_uses_expiry_skew() {
        let now = 1_000;
        assert!(openai_codex_session_needs_refresh(
            &stored_codex_session(now),
            now
        ));
        assert!(openai_codex_session_needs_refresh(
            &stored_codex_session(now + OPENAI_CODEX_REFRESH_SKEW_SECONDS),
            now
        ));
        assert!(!openai_codex_session_needs_refresh(
            &stored_codex_session(now + OPENAI_CODEX_REFRESH_SKEW_SECONDS + 1),
            now
        ));
    }
}
