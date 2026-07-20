use std::{
    path::{Path, PathBuf},
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

use tauri::{
    ipc::{Channel, JavaScriptChannelId},
    AppHandle, Runtime, State, Webview,
};

use crate::{
    commands::{
        agent_event_dto, agent_run_dto, agent_run_summary_dto, validate_non_empty, AgentRunDto,
        AgentRunEventDto, AgentRunEventKindDto, AgentRunStatusDto, AgentTraceExportDto,
        CancelAgentRunRequestDto, CommandError, CommandResult, ExportAgentTraceRequestDto,
        GetAgentRunRequestDto, ListAgentRunsRequestDto, ListAgentRunsResponseDto,
        RejectAgentActionRequestDto, ResumeAgentRunRequestDto, SendAgentMessageRequestDto,
        StartAgentTaskRequestDto, SubscribeAgentStreamRequestDto, SubscribeAgentStreamResponseDto,
    },
    db::project_store,
    registry::read_registry,
    runtime::{
        subscribe_agent_events, AgentAutoCompactPreference, AgentEventSubscription,
        AgentProviderConfig, AutonomousToolRuntime, ContinueOwnedAgentRunRequest,
        DesktopAgentCoreRuntime, DesktopRunDriveMode, OwnedAgentRunRequest,
    },
    state::DesktopState,
};
use xero_agent_core::ApprovalDecisionRequest;

use super::runtime_support::{
    agent_provider_config_identity, emit_owned_runtime_progress,
    ensure_owned_runtime_provider_turn_capabilities, generate_runtime_run_id,
    load_persisted_runtime_run, resolve_owned_agent_provider_config,
    resolve_persisted_owned_agent_provider_config, resolve_project_root,
    staged_attachment_dto_to_message_attachment,
};

const ACTION_PROMPT_ACTIVE_RUN_HANDOFF_TIMEOUT: Duration = Duration::from_millis(1_500);
const ACTION_PROMPT_ACTIVE_RUN_HANDOFF_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[tauri::command]
pub fn start_agent_task<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StartAgentTaskRequestDto,
) -> CommandResult<AgentRunDto> {
    start_agent_task_blocking(&app, state.inner(), request)
}

pub fn start_agent_task_blocking<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    request: StartAgentTaskRequestDto,
) -> CommandResult<AgentRunDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    validate_non_empty(&request.prompt, "prompt")?;
    let run_id = match request.run_id.clone() {
        Some(run_id) => {
            validate_non_empty(&run_id, "runId")?;
            run_id
        }
        None => generate_runtime_run_id(),
    };

    let repo_root = resolve_project_root(app, state, &request.project_id)?;
    let provider_config =
        resolve_owned_agent_provider_config(app, state, request.controls.as_ref())?;
    let (provider_id, model_id) = agent_provider_config_identity(&provider_config);
    let profile_id = provider_profile_id_for_controls(request.controls.as_ref(), &provider_id);
    let provider_preflight = ensure_owned_runtime_provider_turn_capabilities(
        app,
        state,
        state.owned_agent_provider_config_override().is_none(),
        &profile_id,
        &provider_id,
        &model_id,
        &request.attachments,
    )?;
    let tool_runtime =
        tool_runtime_for_provider(app, state, &request.project_id, &provider_config)?;
    let owned_request = OwnedAgentRunRequest {
        repo_root,
        project_id: request.project_id,
        agent_session_id: request.agent_session_id,
        run_id,
        prompt: request.prompt,
        attachments: request
            .attachments
            .iter()
            .map(staged_attachment_dto_to_message_attachment)
            .collect(),
        linked_paths: Vec::new(),
        controls: request.controls,
        tool_runtime,
        provider_config,
        provider_preflight: Some(provider_preflight),
    };
    let runtime = DesktopAgentCoreRuntime::new(state.agent_run_supervisor().clone());
    let snapshot = runtime.start_run(owned_request, DesktopRunDriveMode::Background)?;

    Ok(agent_run_dto(snapshot))
}

#[tauri::command]
pub fn send_agent_message<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SendAgentMessageRequestDto,
) -> CommandResult<AgentRunDto> {
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.continuation_request_id, "continuationRequestId")?;
    validate_non_empty(&request.prompt, "prompt")?;
    let LocatedAgentRun {
        repo_root,
        project_id,
        ..
    } = locate_agent_run(&app, state.inner(), &request.run_id)?;
    ensure_agent_run_not_active(state.inner(), &request.run_id)?;
    let provider_config = resolve_owned_agent_provider_config(&app, state.inner(), None)?;
    let (provider_id, model_id) = agent_provider_config_identity(&provider_config);
    let profile_id = provider_profile_id_for_controls(None, &provider_id);
    let provider_preflight = ensure_owned_runtime_provider_turn_capabilities(
        &app,
        state.inner(),
        state
            .inner()
            .owned_agent_provider_config_override()
            .is_none(),
        &profile_id,
        &provider_id,
        &model_id,
        &request.attachments,
    )?;
    let tool_runtime =
        tool_runtime_for_provider(&app, state.inner(), &project_id, &provider_config)?;
    let continuation = ContinueOwnedAgentRunRequest {
        repo_root,
        project_id: project_id.clone(),
        run_id: request.run_id,
        continuation_request_id: request.continuation_request_id,
        prompt: request.prompt,
        attachments: request
            .attachments
            .iter()
            .map(staged_attachment_dto_to_message_attachment)
            .collect(),
        linked_paths: Vec::new(),
        controls: None,
        tool_runtime,
        provider_config,
        provider_preflight: Some(provider_preflight),
        answer_pending_actions: false,
        answer_pending_action_id: None,
        auto_compact: auto_compact_preference(request.auto_compact)?,
        internal_resume: None,
    };
    let runtime = DesktopAgentCoreRuntime::new(state.inner().agent_run_supervisor().clone());
    let prepared = runtime.continue_run(continuation, DesktopRunDriveMode::Background)?;
    let snapshot = prepared.snapshot.clone();
    Ok(agent_run_dto(snapshot))
}

#[tauri::command]
pub fn cancel_agent_run<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CancelAgentRunRequestDto,
) -> CommandResult<AgentRunDto> {
    validate_non_empty(&request.run_id, "runId")?;
    let LocatedAgentRun {
        repo_root,
        project_id,
        ..
    } = locate_agent_run(&app, state.inner(), &request.run_id)?;
    let runtime = DesktopAgentCoreRuntime::new(state.inner().agent_run_supervisor().clone());
    Ok(agent_run_dto(runtime.cancel_run(
        repo_root,
        project_id,
        request.run_id,
    )?))
}

#[tauri::command]
pub fn reject_agent_action<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RejectAgentActionRequestDto,
) -> CommandResult<AgentRunDto> {
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.action_id, "actionId")?;
    let response = request
        .response
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let LocatedAgentRun {
        repo_root,
        project_id,
        ..
    } = locate_agent_run(&app, state.inner(), &request.run_id)?;
    wait_for_agent_run_to_release_action_prompt_lease(state.inner(), &request.run_id)?;
    let runtime = DesktopAgentCoreRuntime::new(state.inner().agent_run_supervisor().clone());
    Ok(agent_run_dto(runtime.reject_action(
        repo_root,
        ApprovalDecisionRequest {
            project_id,
            run_id: request.run_id,
            action_id: request.action_id,
            response,
        },
    )?))
}

#[tauri::command]
pub fn resume_agent_run<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ResumeAgentRunRequestDto,
) -> CommandResult<AgentRunDto> {
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.continuation_request_id, "continuationRequestId")?;
    validate_non_empty(&request.response, "response")?;
    if let Some(action_id) = request.action_id.as_deref() {
        validate_non_empty(action_id, "actionId")?;
    }
    let LocatedAgentRun {
        repo_root,
        project_id,
        snapshot: located_snapshot,
        ..
    } = locate_agent_run(&app, state.inner(), &request.run_id)?;
    if request.action_id.is_some() {
        wait_for_agent_run_to_release_action_prompt_lease(state.inner(), &request.run_id)?;
    } else {
        ensure_agent_run_not_active(state.inner(), &request.run_id)?;
    }
    let provider_config = resolve_owned_agent_provider_config(&app, state.inner(), None)?;
    let tool_runtime =
        tool_runtime_for_provider(&app, state.inner(), &project_id, &provider_config)?;
    let resume_run_id = request.run_id.clone();
    let resume_action_id = request.action_id.clone();
    let continuation = ContinueOwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        run_id: resume_run_id.clone(),
        continuation_request_id: request.continuation_request_id,
        prompt: request.response,
        attachments: Vec::new(),
        linked_paths: Vec::new(),
        controls: None,
        tool_runtime,
        provider_config,
        provider_preflight: None,
        answer_pending_actions: resume_action_id.is_none(),
        answer_pending_action_id: resume_action_id.clone(),
        auto_compact: auto_compact_preference(request.auto_compact)?,
        internal_resume: None,
    };
    let runtime = DesktopAgentCoreRuntime::new(state.inner().agent_run_supervisor().clone());
    let drive_mode = if resume_action_id.is_some() {
        DesktopRunDriveMode::CreateOnly
    } else {
        DesktopRunDriveMode::Background
    };
    let mut prepared = runtime.continue_run(continuation, drive_mode)?;
    if resume_action_id.is_some() && prepared.drive_required {
        let progress_result = emit_runtime_action_resume_progress(
            &app,
            &repo_root,
            &project_id,
            &located_snapshot.run.agent_session_id,
            &resume_run_id,
        );
        let spawn_result = runtime.spawn_owned_agent_continuation(
            prepared.snapshot.run.agent_session_id.clone(),
            prepared.drive_request.clone(),
            prepared.drive_lease.take(),
        );
        progress_result?;
        spawn_result?;
    }
    let snapshot = prepared.snapshot.clone();
    Ok(agent_run_dto(snapshot))
}

fn emit_runtime_action_resume_progress<R: Runtime>(
    app: &AppHandle<R>,
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: &str,
) -> CommandResult<()> {
    let Some(runtime_snapshot) =
        load_persisted_runtime_run(repo_root, project_id, agent_session_id)?
    else {
        return Ok(());
    };

    if runtime_snapshot.run.run_id != run_id {
        return Ok(());
    }

    if matches!(
        runtime_snapshot.run.status,
        project_store::RuntimeRunStatus::Stopped | project_store::RuntimeRunStatus::Failed
    ) {
        return Ok(());
    }

    emit_owned_runtime_progress(
        app,
        repo_root,
        &runtime_snapshot,
        project_store::RuntimeRunStatus::Running,
        None,
        "Owned agent runtime resumed after an action response.",
    )?;
    Ok(())
}

pub(crate) fn tool_runtime_for_provider<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    provider_config: &AgentProviderConfig,
) -> CommandResult<AutonomousToolRuntime> {
    let (provider_id, model_id) = agent_provider_config_identity(provider_config);
    let policy = super::agent_tooling_settings::resolve_agent_tool_application_style(
        app,
        state,
        &provider_id,
        &model_id,
    )?;
    Ok(AutonomousToolRuntime::for_project_with_provider_config(
        app,
        state,
        project_id,
        Some(provider_config),
    )?
    .with_tool_application_policy(policy))
}

fn provider_profile_id_for_controls(
    controls: Option<&crate::commands::RuntimeRunControlInputDto>,
    provider_id: &str,
) -> String {
    controls
        .and_then(|controls| controls.provider_profile_id.as_deref())
        .map(str::trim)
        .filter(|profile_id| !profile_id.is_empty())
        .unwrap_or(provider_id)
        .to_string()
}

pub(crate) fn recover_prepared_agent_runs_for_project<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<usize> {
    let runtime = DesktopAgentCoreRuntime::new(state.agent_run_supervisor().clone());
    let mut first_error: Option<CommandError> = None;

    // A crash can land after the Ready start marker but before the initial continuation marker.
    // Bind the already-persisted user turn before scanning Prepared requests.
    for start in project_store::list_ready_agent_run_starts(repo_root, project_id)? {
        let existing = project_store::list_agent_continuation_requests_for_run(
            repo_root,
            project_id,
            &start.run_id,
        )?;
        if !existing.is_empty() {
            continue;
        }
        let recovered = match crate::runtime::decode_owned_agent_start_recovery_payload(&start) {
            Ok(Some(recovered)) => recovered,
            Ok(None) => continue,
            Err(error) => {
                first_error.get_or_insert(error);
                continue;
            }
        };
        let snapshot = project_store::load_agent_run(repo_root, project_id, &start.run_id)?;
        let provider_config = match resolve_persisted_owned_agent_provider_config(
            app,
            state,
            &snapshot.run.provider_id,
            &snapshot.run.model_id,
            &recovered.runtime_controls,
        ) {
            Ok(config) => config,
            Err(error) => {
                first_error.get_or_insert(error);
                continue;
            }
        };
        let (resolved_provider_id, resolved_model_id) =
            agent_provider_config_identity(&provider_config);
        if resolved_provider_id != snapshot.run.provider_id
            || resolved_model_id != snapshot.run.model_id
        {
            first_error.get_or_insert_with(|| {
                CommandError::user_fixable(
                    "agent_recovery_provider_mismatch",
                    format!(
                        "Xero cannot recover run `{}` because `{resolved_provider_id}/{resolved_model_id}` does not match its persisted provider `{}/{}`.",
                        snapshot.run.run_id, snapshot.run.provider_id, snapshot.run.model_id
                    ),
                )
            });
            continue;
        }
        let tool_runtime = match tool_runtime_for_provider(app, state, project_id, &provider_config)
        {
            Ok(runtime) => runtime,
            Err(error) => {
                first_error.get_or_insert(error);
                continue;
            }
        };
        let owned = OwnedAgentRunRequest {
            repo_root: repo_root.to_path_buf(),
            project_id: recovered.project_id,
            agent_session_id: recovered.agent_session_id,
            run_id: recovered.run_id,
            prompt: recovered.prompt,
            attachments: recovered.attachments,
            linked_paths: recovered.linked_paths,
            controls: recovered.controls,
            tool_runtime,
            provider_config,
            provider_preflight: None,
        };
        let continuation = crate::runtime::initial_owned_agent_continuation_request(&owned);
        if let Err(error) =
            crate::runtime::register_existing_initial_agent_continuation(&continuation, &snapshot)
        {
            first_error.get_or_insert(error);
        }
    }

    let mut recovered_count = 0;
    for durable in project_store::list_prepared_agent_continuation_requests(repo_root, project_id)?
    {
        let snapshot = match project_store::load_agent_run(repo_root, project_id, &durable.run_id) {
            Ok(snapshot) if snapshot.run.status == project_store::AgentRunStatus::Running => {
                snapshot
            }
            Ok(_) => continue,
            Err(error) => {
                first_error.get_or_insert(error);
                continue;
            }
        };
        let start_ready =
            project_store::load_agent_run_start_request(repo_root, project_id, &durable.run_id)?
                .is_some_and(|start| {
                    start.state == project_store::AgentRunStartRequestState::Ready
                });
        if !start_ready {
            continue;
        }
        let recovered =
            match crate::runtime::decode_owned_agent_continuation_recovery_payload(&durable) {
                Ok(recovered) => recovered,
                Err(error) => {
                    first_error.get_or_insert(error);
                    continue;
                }
            };
        let provider_config = match resolve_persisted_owned_agent_provider_config(
            app,
            state,
            &snapshot.run.provider_id,
            &snapshot.run.model_id,
            &recovered.runtime_controls,
        ) {
            Ok(config) => config,
            Err(error) => {
                first_error.get_or_insert(error);
                continue;
            }
        };
        let (resolved_provider_id, resolved_model_id) =
            agent_provider_config_identity(&provider_config);
        if resolved_provider_id != snapshot.run.provider_id
            || resolved_model_id != snapshot.run.model_id
        {
            first_error.get_or_insert_with(|| {
                CommandError::user_fixable(
                    "agent_recovery_provider_mismatch",
                    format!(
                        "Xero cannot recover run `{}` because `{resolved_provider_id}/{resolved_model_id}` does not match its persisted provider `{}/{}`.",
                        snapshot.run.run_id, snapshot.run.provider_id, snapshot.run.model_id
                    ),
                )
            });
            continue;
        }
        let tool_runtime = match tool_runtime_for_provider(app, state, project_id, &provider_config)
        {
            Ok(runtime) => runtime,
            Err(error) => {
                first_error.get_or_insert(error);
                continue;
            }
        };
        let continuation = ContinueOwnedAgentRunRequest {
            repo_root: repo_root.to_path_buf(),
            project_id: recovered.project_id,
            run_id: recovered.run_id,
            continuation_request_id: durable.request_id.clone(),
            prompt: recovered.prompt,
            attachments: recovered.attachments,
            linked_paths: recovered.linked_paths,
            controls: recovered.controls,
            tool_runtime,
            provider_config,
            provider_preflight: None,
            answer_pending_actions: recovered.answer_pending_actions,
            answer_pending_action_id: recovered.answer_pending_action_id,
            auto_compact: recovered.auto_compact,
            internal_resume: recovered.internal_resume,
        };
        match runtime.recover_prepared_continuation(continuation) {
            Ok(true) => recovered_count += 1,
            Ok(false) => {}
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    if let Some(error) = first_error {
        return Err(error);
    }
    Ok(recovered_count)
}

#[tauri::command]
pub fn get_agent_run<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetAgentRunRequestDto,
) -> CommandResult<AgentRunDto> {
    validate_non_empty(&request.run_id, "runId")?;
    let located = locate_agent_run(&app, state.inner(), &request.run_id)?;
    Ok(agent_run_dto(located.snapshot))
}

#[tauri::command]
pub fn export_agent_trace<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ExportAgentTraceRequestDto,
) -> CommandResult<AgentTraceExportDto> {
    validate_non_empty(&request.run_id, "runId")?;
    let located = locate_agent_run(&app, state.inner(), &request.run_id)?;
    let runtime = DesktopAgentCoreRuntime::new(state.inner().agent_run_supervisor().clone());
    let trace = runtime.export_trace(
        located.repo_root,
        located.project_id,
        located.snapshot.run.run_id.clone(),
    )?;
    let canonical = trace.canonical_snapshot().map_err(|error| {
        CommandError::system_fault(
            error.code,
            format!(
                "Xero could not build the canonical owned-agent trace snapshot: {}",
                error.message
            ),
        )
    })?;
    let markdown_summary = trace.to_markdown_summary().map_err(|error| {
        CommandError::system_fault(
            error.code,
            format!(
                "Xero could not render the owned-agent trace Markdown summary: {}",
                error.message
            ),
        )
    })?;
    let support_bundle = if request.include_support_bundle {
        Some(trace_to_json_value(
            trace.redacted_support_bundle().map_err(|error| {
                CommandError::system_fault(
                    error.code,
                    format!(
                        "Xero could not build the redacted owned-agent support bundle: {}",
                        error.message
                    ),
                )
            })?,
        )?)
    } else {
        None
    };
    Ok(AgentTraceExportDto {
        trace: trace_to_json_value(&canonical.trace)?,
        timeline: trace_to_json_value(&canonical.timeline)?,
        diagnostics: trace_to_json_value(&canonical.diagnostics)?,
        quality_gates: trace_to_json_value(&canonical.quality_gates)?,
        production_readiness: trace_to_json_value(&canonical.production_readiness)?,
        markdown_summary,
        support_bundle,
        canonical_trace: trace_to_json_value(&canonical)?,
    })
}

#[tauri::command]
pub fn list_agent_runs<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListAgentRunsRequestDto,
) -> CommandResult<ListAgentRunsResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let runs =
        project_store::list_agent_runs(&repo_root, &request.project_id, &request.agent_session_id)?;
    Ok(ListAgentRunsResponseDto {
        runs: runs.into_iter().map(agent_run_summary_dto).collect(),
    })
}

#[tauri::command]
pub fn subscribe_agent_stream<R: Runtime>(
    app: AppHandle<R>,
    webview: Webview<R>,
    state: State<'_, DesktopState>,
    request: SubscribeAgentStreamRequestDto,
) -> CommandResult<SubscribeAgentStreamResponseDto> {
    validate_non_empty(&request.run_id, "runId")?;
    let channel = resolve_agent_channel(&webview, request.channel.as_deref())?;
    let located = locate_agent_run(&app, state.inner(), &request.run_id)?;
    let repo_root = located.repo_root.clone();
    let project_id = located.project_id.clone();
    let subscription = subscribe_agent_events(&project_id, &request.run_id);
    let dto = agent_run_dto(project_store::load_agent_run(
        &repo_root,
        &project_id,
        &request.run_id,
    )?);
    let replayed_event_count = dto.events.len();
    let last_event_id = dto.events.iter().map(|event| event.id).max().unwrap_or(0);
    let terminal = agent_run_status_ends_agent_stream(&dto.status, &dto.events);
    for event in dto.events {
        channel.send(event).map_err(|error| {
            CommandError::retryable(
                "agent_stream_channel_closed",
                format!("Xero could not deliver the owned-agent stream event because the desktop channel closed: {error}"),
            )
        })?;
    }
    if !terminal {
        let run_id = request.run_id.clone();
        thread::spawn(move || {
            stream_live_agent_events(
                subscription,
                channel,
                repo_root,
                project_id,
                run_id,
                last_event_id,
            );
        });
    }
    Ok(SubscribeAgentStreamResponseDto {
        run_id: request.run_id,
        replayed_event_count,
    })
}

fn trace_to_json_value<T: serde::Serialize>(value: T) -> CommandResult<serde_json::Value> {
    serde_json::to_value(value).map_err(|error| {
        CommandError::system_fault(
            "agent_trace_export_encode_failed",
            format!("Xero could not encode owned-agent trace export data: {error}"),
        )
    })
}

fn ensure_agent_run_not_active(state: &DesktopState, run_id: &str) -> CommandResult<()> {
    let runtime = DesktopAgentCoreRuntime::new(state.agent_run_supervisor().clone());
    if runtime.is_active(run_id)? {
        return Err(agent_run_already_active_error(run_id));
    }
    Ok(())
}

fn wait_for_agent_run_to_release_action_prompt_lease(
    state: &DesktopState,
    run_id: &str,
) -> CommandResult<()> {
    let runtime = DesktopAgentCoreRuntime::new(state.agent_run_supervisor().clone());
    let deadline = Instant::now() + ACTION_PROMPT_ACTIVE_RUN_HANDOFF_TIMEOUT;

    loop {
        if !runtime.is_active(run_id)? {
            return Ok(());
        }

        let now = Instant::now();
        if now >= deadline {
            return Err(agent_run_already_active_error(run_id));
        }

        thread::sleep(
            ACTION_PROMPT_ACTIVE_RUN_HANDOFF_POLL_INTERVAL
                .min(deadline.saturating_duration_since(now)),
        );
    }
}

fn agent_run_already_active_error(run_id: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_run_already_active",
        format!(
            "Xero is already driving owned-agent run `{run_id}`. Wait for it to finish or cancel it before sending another message."
        ),
    )
}

pub(crate) fn auto_compact_preference(
    preference: Option<crate::commands::AgentAutoCompactPreferenceDto>,
) -> CommandResult<Option<AgentAutoCompactPreference>> {
    let Some(preference) = preference else {
        return Ok(None);
    };
    if let Some(threshold_percent) = preference.threshold_percent {
        if !(1..=100).contains(&threshold_percent) {
            return Err(CommandError::invalid_request(
                "autoCompact.thresholdPercent",
            ));
        }
    }
    if let Some(raw_tail_message_count) = preference.raw_tail_message_count {
        if !(2..=24).contains(&raw_tail_message_count) {
            return Err(CommandError::invalid_request(
                "autoCompact.rawTailMessageCount",
            ));
        }
    }
    Ok(Some(AgentAutoCompactPreference {
        enabled: preference.enabled,
        threshold_percent: preference.threshold_percent,
        raw_tail_message_count: preference.raw_tail_message_count,
    }))
}

fn stream_live_agent_events(
    subscription: AgentEventSubscription,
    channel: Channel<AgentRunEventDto>,
    repo_root: PathBuf,
    project_id: String,
    run_id: String,
    mut last_event_id: i64,
) {
    const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
    loop {
        let event = match subscription.recv_timeout(IDLE_TIMEOUT) {
            Ok(event) => event,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                match stream_persisted_agent_events_after(
                    &repo_root,
                    &project_id,
                    &run_id,
                    &channel,
                    last_event_id,
                    None,
                ) {
                    Ok(StreamCatchupOutcome::Delivered { last_id, terminal }) => {
                        last_event_id = last_id;
                        if terminal {
                            break;
                        }
                    }
                    Ok(StreamCatchupOutcome::NoEvents) => {}
                    Err(_) => break,
                }
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if event.id <= last_event_id {
            continue;
        }
        if event.id > last_event_id.saturating_add(1) {
            match stream_persisted_agent_events_after(
                &repo_root,
                &project_id,
                &run_id,
                &channel,
                last_event_id,
                Some(event.id),
            ) {
                Ok(StreamCatchupOutcome::Delivered { last_id, terminal }) => {
                    last_event_id = last_id;
                    if terminal {
                        break;
                    }
                    if event.id <= last_event_id {
                        continue;
                    }
                }
                Ok(StreamCatchupOutcome::NoEvents) => {}
                Err(_) => break,
            }
        }
        let terminal = agent_event_record_ends_agent_stream(&event);
        last_event_id = event.id;
        if channel.send(agent_event_dto(event)).is_err() {
            return;
        }
        if terminal {
            break;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamCatchupOutcome {
    NoEvents,
    Delivered { last_id: i64, terminal: bool },
}

fn stream_persisted_agent_events_after(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    channel: &Channel<AgentRunEventDto>,
    after_event_id: i64,
    before_event_id: Option<i64>,
) -> CommandResult<StreamCatchupOutcome> {
    const CATCHUP_BATCH_LIMIT: usize = 500;

    let mut last_id = after_event_id;
    let mut delivered_any = false;
    let mut terminal = false;
    loop {
        let events = project_store::read_agent_events_after(
            repo_root,
            project_id,
            run_id,
            last_id,
            CATCHUP_BATCH_LIMIT,
        )?;
        if events.is_empty() {
            break;
        }

        let batch_len = events.len();
        let mut reached_before_event = false;
        for event in events {
            if before_event_id.is_some_and(|before| event.id >= before) {
                reached_before_event = true;
                break;
            }
            terminal = agent_event_record_ends_agent_stream(&event);
            last_id = event.id;
            delivered_any = true;
            channel.send(agent_event_dto(event)).map_err(|error| {
                CommandError::retryable(
                    "agent_stream_channel_closed",
                    format!("Xero could not deliver persisted owned-agent stream event because the desktop channel closed: {error}"),
                )
            })?;
            if terminal {
                break;
            }
        }

        if terminal || reached_before_event || batch_len < CATCHUP_BATCH_LIMIT {
            break;
        }
    }

    if delivered_any {
        Ok(StreamCatchupOutcome::Delivered { last_id, terminal })
    } else {
        Ok(StreamCatchupOutcome::NoEvents)
    }
}

fn agent_run_status_ends_agent_stream(
    status: &AgentRunStatusDto,
    events: &[AgentRunEventDto],
) -> bool {
    match status {
        AgentRunStatusDto::Paused => {
            !latest_run_pause_dto_is_scheduled_wait(events).unwrap_or(false)
        }
        AgentRunStatusDto::Cancelled
        | AgentRunStatusDto::HandedOff
        | AgentRunStatusDto::Completed
        | AgentRunStatusDto::Failed => true,
        AgentRunStatusDto::Starting
        | AgentRunStatusDto::Running
        | AgentRunStatusDto::Cancelling => false,
    }
}

fn latest_run_pause_dto_is_scheduled_wait(events: &[AgentRunEventDto]) -> Option<bool> {
    events
        .iter()
        .rev()
        .find(|event| event.event_kind == AgentRunEventKindDto::RunPaused)
        .map(|event| payload_is_scheduled_wait(&event.payload))
}

fn agent_event_record_ends_agent_stream(event: &project_store::AgentEventRecord) -> bool {
    match event.event_kind {
        project_store::AgentRunEventKind::RunCompleted
        | project_store::AgentRunEventKind::RunFailed => true,
        project_store::AgentRunEventKind::RunPaused => !agent_event_record_is_scheduled_wait(event),
        _ => false,
    }
}

fn agent_event_record_is_scheduled_wait(event: &project_store::AgentEventRecord) -> bool {
    serde_json::from_str::<serde_json::Value>(&event.payload_json)
        .map(|payload| payload_is_scheduled_wait(&payload))
        .unwrap_or(false)
}

fn payload_is_scheduled_wait(payload: &serde_json::Value) -> bool {
    let state = payload_text(payload, "state");
    let stop_reason = payload_text(payload, "stopReason");
    match (state.as_deref(), stop_reason.as_deref()) {
        (Some(state), Some(stop_reason)) => {
            state == "scheduled_wait" && stop_reason == "scheduled_wait"
        }
        (Some(state), None) => state == "scheduled_wait",
        (None, Some(stop_reason)) => stop_reason == "scheduled_wait",
        (None, None) => false,
    }
}

fn payload_text(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

struct LocatedAgentRun {
    repo_root: PathBuf,
    project_id: String,
    snapshot: project_store::AgentRunSnapshotRecord,
}

fn locate_agent_run<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    run_id: &str,
) -> CommandResult<LocatedAgentRun> {
    let registry_path = state.global_db_path(app)?;
    crate::db::configure_project_database_paths(&registry_path);
    let registry = read_registry(&registry_path)?;
    for project in registry.projects {
        let repo_root = PathBuf::from(&project.root_path);
        match project_store::load_agent_run(&repo_root, &project.project_id, run_id) {
            Ok(snapshot) => {
                return Ok(LocatedAgentRun {
                    repo_root,
                    project_id: project.project_id,
                    snapshot,
                });
            }
            Err(error) if error.code == "agent_run_not_found" => continue,
            Err(error) => return Err(error),
        }
    }

    Err(CommandError::user_fixable(
        "agent_run_not_found",
        format!("Xero could not find owned agent run `{run_id}` in the imported projects."),
    ))
}

fn resolve_agent_channel<R: Runtime>(
    webview: &Webview<R>,
    raw_channel: Option<&str>,
) -> CommandResult<Channel<AgentRunEventDto>> {
    let Some(raw_channel) = raw_channel else {
        return Err(CommandError::user_fixable(
            "agent_stream_channel_missing",
            "Xero requires an agent stream channel before it can replay owned-agent events.",
        ));
    };

    let channel_id = JavaScriptChannelId::from_str(raw_channel).map_err(|_| {
        CommandError::user_fixable(
            "agent_stream_channel_invalid",
            "Xero received an invalid owned-agent stream channel handle from the desktop shell.",
        )
    })?;

    Ok(channel_id.channel_on(webview.clone()))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::mpsc::sync_channel,
        time::{Duration as StdDuration, Instant as StdInstant},
    };

    use serde_json::json;
    use tauri::Manager;

    use super::*;
    use crate::{
        db::{self, project_store::{AgentRunEventKind, NewAgentEventRecord, NewAgentRunRecord}},
        git::repository::CanonicalRepository,
        runtime::agent_core::publish_agent_event,
    };

    fn capturing_event_channel(
        capacity: usize,
    ) -> (
        Channel<AgentRunEventDto>,
        std::sync::mpsc::Receiver<AgentRunEventDto>,
    ) {
        let (tx, rx) = sync_channel(capacity);
        let channel = Channel::<AgentRunEventDto>::new(move |body| {
            tx.send(
                body.deserialize::<AgentRunEventDto>()
                    .expect("deserialize agent event"),
            )
            .expect("capture agent event");
            Ok(())
        });
        (channel, rx)
    }

    fn event_record(
        event_kind: AgentRunEventKind,
        payload_json: &str,
    ) -> project_store::AgentEventRecord {
        project_store::AgentEventRecord {
            id: 1,
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            event_kind,
            payload_json: payload_json.into(),
            created_at: "2026-04-24T00:00:00Z".into(),
        }
    }

    fn wait_for_fixture_run_status(
        repo_root: &Path,
        project_id: &str,
        run_id: &str,
        expected: project_store::AgentRunStatus,
    ) -> project_store::AgentRunSnapshotRecord {
        let deadline = StdInstant::now() + StdDuration::from_secs(10);
        loop {
            let snapshot = project_store::load_agent_run(repo_root, project_id, run_id)
                .expect("load recovery fixture run");
            if snapshot.run.status == expected {
                return snapshot;
            }
            assert!(
                StdInstant::now() < deadline,
                "run {run_id} did not reach {expected:?}; last status was {:?}",
                snapshot.run.status
            );
            thread::sleep(StdDuration::from_millis(20));
        }
    }

    #[test]
    fn prepared_start_recovery_fixture_registers_and_drives_the_exact_initial_turn_once() {
        let fixture = tempfile::tempdir().expect("create recovery fixture");
        let repo_root = fixture.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create recovery repository");
        let repo_root = fs::canonicalize(repo_root).expect("canonical recovery repository");
        let project_id = "project-agent-recovery";
        let repository = CanonicalRepository {
            project_id: project_id.into(),
            repository_id: "repository-agent-recovery".into(),
            root_path: repo_root.clone(),
            root_path_string: repo_root.to_string_lossy().into_owned(),
            common_git_dir: repo_root.join(".git"),
            display_name: "Agent recovery fixture".into(),
            branch_name: None,
            head_sha: None,
            branch: None,
            last_commit: None,
            status_entries: Vec::new(),
            has_staged_changes: false,
            has_unstaged_changes: false,
            has_untracked_changes: false,
            additions: 0,
            deletions: 0,
        };
        let registry_path = fixture.path().join("app-data").join("xero.db");
        let state = DesktopState::default()
            .with_global_db_path_override(registry_path.clone())
            .with_owned_agent_provider_config_override(AgentProviderConfig::Fake);
        let app = crate::configure_builder_with_state(tauri::test::mock_builder(), state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build recovery fixture app");
        db::configure_project_database_paths(&registry_path);
        db::import_project(
            &repository,
            app.state::<DesktopState>().import_failpoints(),
        )
        .expect("import recovery fixture project");
        crate::registry::replace_projects(
            &registry_path,
            vec![crate::registry::RegistryProjectRecord {
                project_id: project_id.into(),
                repository_id: repository.repository_id.clone(),
                root_path: repo_root.to_string_lossy().into_owned(),
                is_git_repo: false,
            }],
        )
        .expect("seed recovery fixture registry");
        let run_id = "run-agent-recovery";
        let owned = OwnedAgentRunRequest {
            repo_root: repo_root.clone(),
            project_id: project_id.into(),
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: run_id.into(),
            prompt: "Recover this exact initial turn after the crash boundary.".into(),
            attachments: Vec::new(),
            linked_paths: Vec::new(),
            controls: None,
            tool_runtime: AutonomousToolRuntime::new(&repo_root).expect("recovery tool runtime"),
            provider_config: AgentProviderConfig::Fake,
            provider_preflight: None,
        };
        let runtime = DesktopAgentCoreRuntime::new(
            app.state::<DesktopState>().agent_run_supervisor().clone(),
        );
        let created = runtime
            .start_run(owned, DesktopRunDriveMode::CreateOnly)
            .expect("persist run at pre-dispatch crash boundary");
        assert_eq!(created.run.status, project_store::AgentRunStatus::Running);
        assert!(project_store::list_agent_continuation_requests_for_run(
            &repo_root,
            project_id,
            run_id,
        )
        .expect("list pre-recovery continuations")
        .is_empty());

        assert_eq!(
            recover_prepared_agent_runs_for_project(
                app.handle(),
                app.state::<DesktopState>().inner(),
                &repo_root,
                project_id,
            )
            .expect("recover prepared initial continuation"),
            1
        );
        let completed = wait_for_fixture_run_status(
            &repo_root,
            project_id,
            run_id,
            project_store::AgentRunStatus::Completed,
        );
        assert_eq!(
            completed
                .messages
                .iter()
                .filter(|message| message.role == project_store::AgentMessageRole::User)
                .count(),
            1,
            "startup recovery must bind rather than duplicate the initial user turn"
        );
        let deadline = StdInstant::now() + StdDuration::from_secs(5);
        let continuations = loop {
            let continuations = project_store::list_agent_continuation_requests_for_run(
                &repo_root,
                project_id,
                run_id,
            )
            .expect("list recovered continuations");
            if continuations.first().is_some_and(|request| {
                request.state == project_store::AgentContinuationRequestState::Consumed
            }) {
                break continuations;
            }
            assert!(
                StdInstant::now() < deadline,
                "recovered continuation did not finish its durable drive marker: {continuations:?}"
            );
            thread::sleep(StdDuration::from_millis(20));
        };
        assert_eq!(continuations.len(), 1);
        assert_eq!(
            continuations[0].state,
            project_store::AgentContinuationRequestState::Consumed
        );
        assert_eq!(
            recover_prepared_agent_runs_for_project(
                app.handle(),
                app.state::<DesktopState>().inner(),
                &repo_root,
                project_id,
            )
            .expect("replay recovery scan"),
            0
        );
    }

    #[test]
    fn live_stream_fixture_skips_replays_delivers_terminal_and_stops_on_closed_channel() {
        let project_id = "project-live-stream";
        let run_id = "run-live-stream";
        let subscription = subscribe_agent_events(project_id, run_id);
        let (channel, rx) = capturing_event_channel(3);
        let stream = thread::spawn(move || {
            stream_live_agent_events(
                subscription,
                channel,
                PathBuf::from("unused-for-sequential-events"),
                project_id.into(),
                run_id.into(),
                1,
            );
        });

        let mut replay = event_record(AgentRunEventKind::MessageDelta, r#"{"text":"old"}"#);
        replay.project_id = project_id.into();
        replay.run_id = run_id.into();
        replay.id = 1;
        publish_agent_event(replay);
        let mut delta = event_record(AgentRunEventKind::MessageDelta, r#"{"text":"new"}"#);
        delta.project_id = project_id.into();
        delta.run_id = run_id.into();
        delta.id = 2;
        publish_agent_event(delta);
        let mut completed = event_record(AgentRunEventKind::RunCompleted, r#"{"ok":true}"#);
        completed.project_id = project_id.into();
        completed.run_id = run_id.into();
        completed.id = 3;
        publish_agent_event(completed);

        assert_eq!(
            rx.recv_timeout(StdDuration::from_secs(1))
                .expect("receive live delta")
                .id,
            2
        );
        assert_eq!(
            rx.recv_timeout(StdDuration::from_secs(1))
                .expect("receive terminal event")
                .id,
            3
        );
        stream.join().expect("join terminal stream");

        let closed_project = "project-closed-stream";
        let closed_run = "run-closed-stream";
        let closed_subscription = subscribe_agent_events(closed_project, closed_run);
        let closed_channel = Channel::<AgentRunEventDto>::new(move |_body| {
            Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "fixture closed").into())
        });
        let closed_stream = thread::spawn(move || {
            stream_live_agent_events(
                closed_subscription,
                closed_channel,
                PathBuf::from("unused-for-closed-channel"),
                closed_project.into(),
                closed_run.into(),
                0,
            );
        });
        let mut event = event_record(AgentRunEventKind::MessageDelta, r#"{"text":"drop"}"#);
        event.project_id = closed_project.into();
        event.run_id = closed_run.into();
        event.id = 1;
        publish_agent_event(event);
        closed_stream.join().expect("closed channel stops stream");
    }

    #[test]
    fn scheduled_wait_pause_keeps_agent_stream_live() {
        let scheduled_wait = event_record(
            AgentRunEventKind::RunPaused,
            r#"{"state":"scheduled_wait","stopReason":"scheduled_wait"}"#,
        );
        let manual_pause = event_record(
            AgentRunEventKind::RunPaused,
            r#"{"state":"paused","stopReason":"waiting_for_approval"}"#,
        );
        let completed = event_record(
            AgentRunEventKind::RunCompleted,
            r#"{"summary":"Owned agent run completed."}"#,
        );

        assert!(!agent_event_record_ends_agent_stream(&scheduled_wait));
        assert!(agent_event_record_ends_agent_stream(&manual_pause));
        assert!(agent_event_record_ends_agent_stream(&completed));
        assert!(!agent_run_status_ends_agent_stream(
            &AgentRunStatusDto::Paused,
            &[agent_event_dto(scheduled_wait)]
        ));
        assert!(agent_run_status_ends_agent_stream(
            &AgentRunStatusDto::Paused,
            &[agent_event_dto(manual_pause)]
        ));
    }

    #[test]
    fn contradictory_pause_markers_fail_closed_and_end_the_stream() {
        for payload in [
            r#"{"state":"scheduled_wait","stopReason":"waiting_for_approval"}"#,
            r#"{"state":"paused","stopReason":"scheduled_wait"}"#,
        ] {
            let event = event_record(AgentRunEventKind::RunPaused, payload);
            assert!(
                agent_event_record_ends_agent_stream(&event),
                "conflicting pause markers must not keep a stream alive: {payload}"
            );
        }
    }

    #[test]
    fn stream_terminal_and_configuration_helpers_cover_every_boundary() {
        for status in [
            AgentRunStatusDto::Cancelled,
            AgentRunStatusDto::HandedOff,
            AgentRunStatusDto::Completed,
            AgentRunStatusDto::Failed,
        ] {
            assert!(agent_run_status_ends_agent_stream(&status, &[]));
        }
        for status in [
            AgentRunStatusDto::Starting,
            AgentRunStatusDto::Running,
            AgentRunStatusDto::Cancelling,
        ] {
            assert!(!agent_run_status_ends_agent_stream(&status, &[]));
        }
        assert!(agent_run_status_ends_agent_stream(
            &AgentRunStatusDto::Paused,
            &[]
        ));

        let manual = agent_event_dto(event_record(
            AgentRunEventKind::RunPaused,
            r#"{"state":"paused"}"#,
        ));
        let scheduled = agent_event_dto(event_record(
            AgentRunEventKind::RunPaused,
            r#"{"stopReason":" scheduled_wait "}"#,
        ));
        assert!(!agent_run_status_ends_agent_stream(
            &AgentRunStatusDto::Paused,
            &[manual.clone(), scheduled.clone()]
        ));
        assert!(agent_run_status_ends_agent_stream(
            &AgentRunStatusDto::Paused,
            &[scheduled, manual]
        ));
        assert!(agent_event_record_ends_agent_stream(&event_record(
            AgentRunEventKind::RunFailed,
            "{}"
        )));
        assert!(agent_event_record_ends_agent_stream(&event_record(
            AgentRunEventKind::RunPaused,
            "malformed"
        )));
        assert!(!agent_event_record_ends_agent_stream(&event_record(
            AgentRunEventKind::MessageDelta,
            "{}"
        )));

        assert_eq!(
            payload_text(&json!({"value": " trimmed "}), "value"),
            Some("trimmed".into())
        );
        assert_eq!(payload_text(&json!({"value": " "}), "value"), None);
        assert_eq!(payload_text(&json!({"value": 7}), "value"), None);

        assert!(auto_compact_preference(None).unwrap().is_none());
        let preference =
            auto_compact_preference(Some(crate::commands::AgentAutoCompactPreferenceDto {
                enabled: true,
                threshold_percent: Some(100),
                raw_tail_message_count: Some(24),
            }))
            .expect("valid auto-compact boundary")
            .expect("auto-compact preference");
        assert!(preference.enabled);
        assert_eq!(preference.threshold_percent, Some(100));
        assert_eq!(preference.raw_tail_message_count, Some(24));
        for threshold_percent in [0, 101] {
            assert!(auto_compact_preference(Some(
                crate::commands::AgentAutoCompactPreferenceDto {
                    enabled: true,
                    threshold_percent: Some(threshold_percent),
                    raw_tail_message_count: None,
                },
            ))
            .is_err());
        }
        for raw_tail_message_count in [1, 25] {
            assert!(auto_compact_preference(Some(
                crate::commands::AgentAutoCompactPreferenceDto {
                    enabled: false,
                    threshold_percent: None,
                    raw_tail_message_count: Some(raw_tail_message_count),
                },
            ))
            .is_err());
        }

        let mut controls = crate::commands::RuntimeRunControlInputDto {
            runtime_agent_id: crate::commands::RuntimeAgentIdDto::Engineer,
            agent_definition_id: Some("engineer".into()),
            agent_definition_version: None,
            provider_profile_id: Some(" profile-1 ".into()),
            model_id: "model-1".into(),
            thinking_effort: None,
            approval_mode: crate::commands::RuntimeRunApprovalModeDto::Suggest,
            plan_mode_required: false,
            auto_compact_enabled: true,
        };
        assert_eq!(
            provider_profile_id_for_controls(Some(&controls), "provider-1"),
            "profile-1"
        );
        controls.provider_profile_id = Some(" ".into());
        assert_eq!(
            provider_profile_id_for_controls(Some(&controls), "provider-1"),
            "provider-1"
        );
        assert_eq!(
            provider_profile_id_for_controls(None, "provider-2"),
            "provider-2"
        );

        let active = agent_run_already_active_error("run-1");
        assert_eq!(active.code, "agent_run_already_active");
        assert!(active.message.contains("run-1"));
    }

    #[test]
    fn trace_json_conversion_reports_serialization_failures() {
        struct SerializationFailure;

        impl serde::Serialize for SerializationFailure {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(serde::ser::Error::custom("fixture serialization failure"))
            }
        }

        assert_eq!(
            trace_to_json_value(json!({"ok": true})).unwrap()["ok"],
            true
        );
        let error = trace_to_json_value(SerializationFailure)
            .expect_err("serialization failure must be typed");
        assert_eq!(error.code, "agent_trace_export_encode_failed");
        assert!(error.message.contains("fixture serialization failure"));
    }

    #[test]
    fn stream_persisted_agent_events_replays_more_than_one_batch() {
        let root = tempfile::tempdir().expect("temp dir");
        let repo_root = root.path();
        let project_id = "project-1";
        let run_id = "run-1";
        let root_path = repo_root.to_string_lossy().into_owned();
        let registry_path = root.path().join("app-data").join("xero.db");
        crate::db::configure_project_database_paths(&registry_path);
        crate::registry::replace_projects(
            &registry_path,
            vec![crate::registry::RegistryProjectRecord {
                project_id: project_id.into(),
                repository_id: "repo-1".into(),
                root_path: root_path.clone(),
                is_git_repo: true,
            }],
        )
        .expect("seed registry");
        let database_path = crate::db::database_path_for_repo(repo_root);
        std::fs::create_dir_all(database_path.parent().expect("database parent"))
            .expect("create xero state dir");
        let mut connection =
            rusqlite::Connection::open(&database_path).expect("create state database");
        crate::db::configure_connection(&connection).expect("configure state database");
        crate::db::migrations::migrations()
            .to_latest(&mut connection)
            .expect("migrate state database");
        connection
            .execute(
                "INSERT INTO projects (id, name) VALUES (?1, 'Project')",
                [project_id],
            )
            .expect("insert project");
        connection
            .execute(
                "INSERT INTO repositories (id, project_id, root_path, display_name) VALUES ('repo-1', ?1, ?2, 'repo')",
                (project_id, root_path.as_str()),
            )
            .expect("insert repository");
        connection
            .execute(
                "INSERT INTO agent_sessions (project_id, agent_session_id, title, status, selected) VALUES (?1, ?2, 'Default', 'active', 1)",
                (project_id, project_store::DEFAULT_AGENT_SESSION_ID),
            )
            .expect("insert agent session");
        drop(connection);

        project_store::insert_agent_run(
            repo_root,
            &NewAgentRunRecord {
                runtime_agent_id: crate::commands::RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                project_id: project_id.into(),
                agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
                run_id: run_id.into(),
                provider_id: "fake".into(),
                model_id: "test".into(),
                prompt: "prompt".into(),
                system_prompt: "system".into(),
                now: "2026-04-25T00:00:00Z".into(),
            },
        )
        .expect("insert agent run");

        for index in 0..750 {
            project_store::append_agent_event(
                repo_root,
                &NewAgentEventRecord {
                    project_id: project_id.into(),
                    run_id: run_id.into(),
                    event_kind: AgentRunEventKind::MessageDelta,
                    payload_json: format!(r#"{{"index":{index}}}"#),
                    created_at: "2026-04-25T00:00:00Z".into(),
                },
            )
            .expect("append agent event");
        }

        let (channel, rx) = capturing_event_channel(800);

        let outcome =
            stream_persisted_agent_events_after(repo_root, project_id, run_id, &channel, 0, None)
                .expect("stream persisted events");

        assert_eq!(
            outcome,
            StreamCatchupOutcome::Delivered {
                last_id: 750,
                terminal: false,
            }
        );
        assert_eq!(rx.try_iter().count(), 750);

        assert_eq!(
            stream_persisted_agent_events_after(
                repo_root,
                project_id,
                run_id,
                &channel,
                750,
                None,
            )
            .expect("empty catchup"),
            StreamCatchupOutcome::NoEvents
        );

        for (event_kind, payload_json) in [
            (AgentRunEventKind::MessageDelta, r#"{"index":750}"#),
            (AgentRunEventKind::RunFailed, r#"{"code":"fixture"}"#),
            (AgentRunEventKind::MessageDelta, r#"{"index":752}"#),
        ] {
            project_store::append_agent_event(
                repo_root,
                &NewAgentEventRecord {
                    project_id: project_id.into(),
                    run_id: run_id.into(),
                    event_kind,
                    payload_json: payload_json.into(),
                    created_at: "2026-04-25T00:00:01Z".into(),
                },
            )
            .expect("append catchup boundary event");
        }

        let (terminal_channel, terminal_rx) = capturing_event_channel(4);
        assert_eq!(
            stream_persisted_agent_events_after(
                repo_root,
                project_id,
                run_id,
                &terminal_channel,
                750,
                None,
            )
            .expect("terminal catchup"),
            StreamCatchupOutcome::Delivered {
                last_id: 752,
                terminal: true,
            }
        );
        assert_eq!(
            terminal_rx
                .try_iter()
                .map(|event| event.id)
                .collect::<Vec<_>>(),
            vec![751, 752]
        );

        let (bounded_channel, bounded_rx) = capturing_event_channel(4);
        assert_eq!(
            stream_persisted_agent_events_after(
                repo_root,
                project_id,
                run_id,
                &bounded_channel,
                750,
                Some(752),
            )
            .expect("bounded catchup"),
            StreamCatchupOutcome::Delivered {
                last_id: 751,
                terminal: false,
            }
        );
        assert_eq!(bounded_rx.try_iter().count(), 1);

        let (empty_channel, _empty_rx) = capturing_event_channel(1);
        assert_eq!(
            stream_persisted_agent_events_after(
                repo_root,
                project_id,
                run_id,
                &empty_channel,
                750,
                Some(751),
            )
            .expect("empty bounded catchup"),
            StreamCatchupOutcome::NoEvents
        );

        let failed_channel = Channel::<AgentRunEventDto>::new(move |_body| {
            Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "channel dropped").into())
        });
        assert_eq!(
            stream_persisted_agent_events_after(
                repo_root,
                project_id,
                run_id,
                &failed_channel,
                750,
                None,
            )
            .expect_err("closed catchup channel must fail")
            .code,
            "agent_stream_channel_closed"
        );
    }
}
