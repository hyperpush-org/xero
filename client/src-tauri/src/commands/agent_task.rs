use std::{path::Path, path::PathBuf, str::FromStr, thread, time::Duration};

use tauri::{
    ipc::{Channel, JavaScriptChannelId},
    AppHandle, Runtime, State, Webview,
};

use crate::{
    commands::{
        agent_event_dto, agent_run_dto, agent_run_summary_dto, validate_non_empty, AgentRunDto,
        AgentRunEventDto, CancelAgentRunRequestDto, CommandError, CommandResult,
        GetAgentRunRequestDto, ListAgentRunsRequestDto, ListAgentRunsResponseDto,
        ResumeAgentRunRequestDto, SendAgentMessageRequestDto, StartAgentTaskRequestDto,
        SubscribeAgentStreamRequestDto, SubscribeAgentStreamResponseDto,
    },
    db::project_store,
    registry::read_registry,
    runtime::{
        cancel_owned_agent_run, create_owned_agent_run, drive_owned_agent_continuation,
        drive_owned_agent_run, prepare_owned_agent_continuation_for_drive, subscribe_agent_events,
        AgentAutoCompactPreference, AgentEventSubscription, AgentRunSupervisor,
        AutonomousToolRuntime, ContinueOwnedAgentRunRequest, OwnedAgentRunRequest,
    },
    state::DesktopState,
};

use super::runtime_support::{
    generate_runtime_run_id, resolve_owned_agent_provider_config, resolve_project_root,
};

#[tauri::command]
pub fn start_agent_task<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StartAgentTaskRequestDto,
) -> CommandResult<AgentRunDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    validate_non_empty(&request.prompt, "prompt")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let tool_runtime =
        AutonomousToolRuntime::for_project(&app, state.inner(), &request.project_id)?;
    let provider_config =
        resolve_owned_agent_provider_config(&app, state.inner(), request.controls.as_ref())?;
    let owned_request = OwnedAgentRunRequest {
        repo_root,
        project_id: request.project_id,
        agent_session_id: request.agent_session_id,
        run_id: generate_runtime_run_id(),
        prompt: request.prompt,
        attachments: Vec::new(),
        controls: request.controls,
        tool_runtime,
        provider_config,
    };
    let snapshot = create_owned_agent_run(&owned_request)?;
    spawn_owned_agent_run(state.inner().agent_run_supervisor().clone(), owned_request)?;

    Ok(agent_run_dto(snapshot))
}

#[tauri::command]
pub fn send_agent_message<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SendAgentMessageRequestDto,
) -> CommandResult<AgentRunDto> {
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.prompt, "prompt")?;
    let LocatedAgentRun {
        repo_root,
        project_id,
        ..
    } = locate_agent_run(&app, state.inner(), &request.run_id)?;
    ensure_agent_run_not_active(state.inner(), &request.run_id)?;
    let tool_runtime = AutonomousToolRuntime::for_project(&app, state.inner(), &project_id)?;
    let provider_config = resolve_owned_agent_provider_config(&app, state.inner(), None)?;
    let continuation = ContinueOwnedAgentRunRequest {
        repo_root,
        project_id: project_id.clone(),
        run_id: request.run_id,
        prompt: request.prompt,
        attachments: Vec::new(),
        controls: None,
        tool_runtime,
        provider_config,
        answer_pending_actions: false,
        auto_compact: auto_compact_preference(request.auto_compact)?,
    };
    let prepared = prepare_owned_agent_continuation_for_drive(&continuation)?;
    let snapshot = prepared.snapshot.clone();
    if prepared.drive_required {
        spawn_owned_agent_continuation(
            state.inner().agent_run_supervisor().clone(),
            snapshot.run.agent_session_id.clone(),
            prepared.drive_request,
        )?;
    }
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
    let _ = state
        .inner()
        .agent_run_supervisor()
        .cancel(&request.run_id)?;
    Ok(agent_run_dto(cancel_owned_agent_run(
        &repo_root,
        &project_id,
        &request.run_id,
    )?))
}

#[tauri::command]
pub fn resume_agent_run<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ResumeAgentRunRequestDto,
) -> CommandResult<AgentRunDto> {
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.response, "response")?;
    let LocatedAgentRun {
        repo_root,
        project_id,
        ..
    } = locate_agent_run(&app, state.inner(), &request.run_id)?;
    ensure_agent_run_not_active(state.inner(), &request.run_id)?;
    let tool_runtime = AutonomousToolRuntime::for_project(&app, state.inner(), &project_id)?;
    let provider_config = resolve_owned_agent_provider_config(&app, state.inner(), None)?;
    let continuation = ContinueOwnedAgentRunRequest {
        repo_root,
        project_id: project_id.clone(),
        run_id: request.run_id,
        prompt: request.response,
        attachments: Vec::new(),
        controls: None,
        tool_runtime,
        provider_config,
        answer_pending_actions: true,
        auto_compact: auto_compact_preference(request.auto_compact)?,
    };
    let prepared = prepare_owned_agent_continuation_for_drive(&continuation)?;
    let snapshot = prepared.snapshot.clone();
    if prepared.drive_required {
        spawn_owned_agent_continuation(
            state.inner().agent_run_supervisor().clone(),
            snapshot.run.agent_session_id.clone(),
            prepared.drive_request,
        )?;
    }
    Ok(agent_run_dto(snapshot))
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
    let terminal = matches!(
        dto.status,
        crate::commands::AgentRunStatusDto::Paused
            | crate::commands::AgentRunStatusDto::Cancelled
            | crate::commands::AgentRunStatusDto::HandedOff
            | crate::commands::AgentRunStatusDto::Completed
            | crate::commands::AgentRunStatusDto::Failed
    );
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

fn spawn_owned_agent_run(
    supervisor: AgentRunSupervisor,
    request: OwnedAgentRunRequest,
) -> CommandResult<()> {
    let lease = supervisor.begin(
        &request.project_id,
        &request.agent_session_id,
        &request.run_id,
    )?;
    thread::spawn(move || {
        let token = lease.token();
        let _ = drive_owned_agent_run(request, token);
        drop(lease);
    });
    Ok(())
}

fn spawn_owned_agent_continuation(
    supervisor: AgentRunSupervisor,
    agent_session_id: String,
    request: ContinueOwnedAgentRunRequest,
) -> CommandResult<()> {
    let lease = supervisor.begin(&request.project_id, &agent_session_id, &request.run_id)?;
    thread::spawn(move || {
        let token = lease.token();
        let _ = drive_owned_agent_continuation(request, token);
        drop(lease);
    });
    Ok(())
}

fn ensure_agent_run_not_active(state: &DesktopState, run_id: &str) -> CommandResult<()> {
    if state.agent_run_supervisor().is_active(run_id)? {
        return Err(CommandError::user_fixable(
            "agent_run_already_active",
            format!(
                "Xero is already driving owned-agent run `{run_id}`. Wait for it to finish or cancel it before sending another message."
            ),
        ));
    }
    Ok(())
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
        let terminal = matches!(
            event.event_kind,
            project_store::AgentRunEventKind::RunPaused
                | project_store::AgentRunEventKind::RunCompleted
                | project_store::AgentRunEventKind::RunFailed
        );
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
            terminal = matches!(
                event.event_kind,
                project_store::AgentRunEventKind::RunPaused
                    | project_store::AgentRunEventKind::RunCompleted
                    | project_store::AgentRunEventKind::RunFailed
            );
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
    use std::sync::mpsc::sync_channel;

    use super::*;
    use crate::db::project_store::{AgentRunEventKind, NewAgentEventRecord, NewAgentRunRecord};

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

        let (tx, rx) = sync_channel(800);
        let channel = Channel::<AgentRunEventDto>::new(move |body| {
            tx.send(
                body.deserialize::<AgentRunEventDto>()
                    .expect("deserialize agent event"),
            )
            .expect("send agent event");
            Ok(())
        });

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
    }
}
