use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::Duration as StdDuration,
};

use tauri::{AppHandle, Manager, Runtime};
use time::{format_description::well_known::Rfc3339, Duration as TimeDuration, OffsetDateTime};

use super::*;
use crate::{
    commands::runtime_support::{
        agent_provider_config_identity, ensure_owned_runtime_provider_turn_capabilities,
        resolve_owned_agent_provider_config, runtime_control_input_from_active,
    },
    runtime::{
        AutonomousProcessManagerOutput, AutonomousProcessManagerRequest, AutonomousProcessMetadata,
        AutonomousProcessStatus,
    },
    state::DesktopState,
};

type WakeupInsertedHandler =
    Arc<dyn Fn(PathBuf, project_store::AgentRunWakeupRecord, AutonomousToolRuntime) + Send + Sync>;

static WAKEUP_INSERTED_HANDLER: OnceLock<Mutex<Option<WakeupInsertedHandler>>> = OnceLock::new();

#[derive(Debug, Clone, Default)]
pub struct AgentRunWakeupScheduler {
    active: Arc<Mutex<BTreeSet<String>>>,
}

#[derive(Debug, Clone)]
struct WakeupResume {
    status: project_store::AgentRunWakeupStatus,
    outcome: String,
    diagnostic: Option<project_store::AgentRunDiagnosticRecord>,
    observation: JsonValue,
}

#[derive(Debug, Clone)]
enum WakeupEvaluation {
    Pending {
        due_at: String,
        payload_json: String,
    },
    Resume(WakeupResume),
}

pub fn set_agent_run_wakeup_inserted_handler<F>(handler: F)
where
    F: Fn(PathBuf, project_store::AgentRunWakeupRecord, AutonomousToolRuntime)
        + Send
        + Sync
        + 'static,
{
    let slot = WAKEUP_INSERTED_HANDLER.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(Arc::new(handler));
    }
}

pub fn notify_agent_run_wakeup_inserted(
    repo_root: &Path,
    record: &project_store::AgentRunWakeupRecord,
    tool_runtime: AutonomousToolRuntime,
) {
    let Some(slot) = WAKEUP_INSERTED_HANDLER.get() else {
        return;
    };
    let Ok(guard) = slot.lock() else {
        return;
    };
    let Some(handler) = guard.as_ref().cloned() else {
        return;
    };
    handler(repo_root.to_path_buf(), record.clone(), tool_runtime);
}

impl AgentRunWakeupScheduler {
    pub fn schedule_record<R: Runtime + 'static>(
        &self,
        app: AppHandle<R>,
        repo_root: PathBuf,
        record: project_store::AgentRunWakeupRecord,
        tool_runtime: Option<AutonomousToolRuntime>,
    ) -> CommandResult<bool> {
        let key = wakeup_key(&record);
        {
            let mut active = self.active.lock().map_err(|_| {
                CommandError::system_fault(
                    "agent_run_wakeup_scheduler_lock_failed",
                    "Xero could not lock the scheduled wakeup registry.",
                )
            })?;
            if !active.insert(key.clone()) {
                return Ok(false);
            }
        }

        let scheduler = self.clone();
        thread::spawn(move || {
            let result = drive_scheduled_wakeup(app, repo_root, record, tool_runtime);
            if let Err(error) = result {
                eprintln!(
                    "[agent-wakeup] scheduled wakeup worker stopped with {}: {}",
                    error.code, error.message
                );
            }
            scheduler.finish(&key);
        });
        Ok(true)
    }

    pub fn schedule_pending_for_project<R: Runtime + 'static>(
        &self,
        app: AppHandle<R>,
        repo_root: PathBuf,
    ) -> CommandResult<usize> {
        let wakeups = project_store::list_pending_agent_run_wakeups(&repo_root)?;
        let mut scheduled = 0_usize;
        for wakeup in wakeups {
            if self.schedule_record(app.clone(), repo_root.clone(), wakeup, None)? {
                scheduled += 1;
            }
        }
        Ok(scheduled)
    }

    fn finish(&self, key: &str) {
        let Ok(mut active) = self.active.lock() else {
            return;
        };
        active.remove(key);
    }
}

fn drive_scheduled_wakeup<R: Runtime + 'static>(
    app: AppHandle<R>,
    repo_root: PathBuf,
    initial: project_store::AgentRunWakeupRecord,
    tool_runtime: Option<AutonomousToolRuntime>,
) -> CommandResult<()> {
    let mut tool_runtime = tool_runtime;
    loop {
        let Some(record) = project_store::maybe_load_pending_agent_run_wakeup(
            &repo_root,
            &initial.project_id,
            &initial.run_id,
            &initial.wake_id,
        )?
        else {
            return Ok(());
        };
        let snapshot =
            project_store::load_agent_run(&repo_root, &record.project_id, &record.run_id)?;
        match snapshot.run.status {
            AgentRunStatus::Paused => {}
            AgentRunStatus::Cancelled
            | AgentRunStatus::HandedOff
            | AgentRunStatus::Completed
            | AgentRunStatus::Failed => {
                project_store::mark_agent_run_wakeup_status(
                    &repo_root,
                    &record.project_id,
                    &record.run_id,
                    &record.wake_id,
                    project_store::AgentRunWakeupStatus::Cancelled,
                    None,
                    &now_timestamp(),
                )?;
                return Ok(());
            }
            AgentRunStatus::Starting | AgentRunStatus::Running | AgentRunStatus::Cancelling => {
                sleep_for_ms(1_000);
                continue;
            }
        }

        let now = OffsetDateTime::now_utc();
        let due_at = parse_wakeup_timestamp(&record.due_at)?;
        if now < due_at {
            sleep_until(due_at);
            continue;
        }

        let evaluation = evaluate_wakeup(&record, tool_runtime.as_ref(), now)?;
        match evaluation {
            WakeupEvaluation::Pending {
                due_at,
                payload_json,
            } => {
                project_store::reschedule_agent_run_wakeup(
                    &repo_root,
                    &record.project_id,
                    &record.run_id,
                    &record.wake_id,
                    &due_at,
                    &payload_json,
                    &now_timestamp(),
                )?;
                continue;
            }
            WakeupEvaluation::Resume(resume) => {
                match resume_scheduled_wakeup(&app, &repo_root, &record, resume, &mut tool_runtime)
                {
                    Ok(true) => return Ok(()),
                    Ok(false) => {}
                    Err(error) => {
                        persist_scheduled_wakeup_resume_failure(&repo_root, &record, &error);
                        return Err(error);
                    }
                }
                sleep_for_ms(1_000);
                continue;
            }
        }
    }
}

fn evaluate_wakeup(
    record: &project_store::AgentRunWakeupRecord,
    tool_runtime: Option<&AutonomousToolRuntime>,
    now: OffsetDateTime,
) -> CommandResult<WakeupEvaluation> {
    let payload = record.payload()?;
    if let Some(deadline_at) = record.deadline_at.as_deref() {
        let deadline = parse_wakeup_timestamp(deadline_at)?;
        if now >= deadline {
            return Ok(WakeupEvaluation::Resume(WakeupResume {
                status: project_store::AgentRunWakeupStatus::Expired,
                outcome: "expired".into(),
                diagnostic: Some(project_store::AgentRunDiagnosticRecord {
                    code: "agent_run_wakeup_deadline_expired".into(),
                    message: format!(
                        "Scheduled wakeup `{}` reached its deadline at {deadline_at}.",
                        record.wake_id
                    ),
                }),
                observation: json!({
                    "deadlineAt": deadline_at,
                    "payload": payload,
                }),
            }));
        }
    }

    match record.kind {
        project_store::AgentRunWakeupKind::Sleep => Ok(WakeupEvaluation::Resume(WakeupResume {
            status: project_store::AgentRunWakeupStatus::Fired,
            outcome: "timer_elapsed".into(),
            diagnostic: None,
            observation: json!({
                "dueAt": record.due_at,
                "payload": payload,
            }),
        })),
        project_store::AgentRunWakeupKind::ProcessExit
        | project_store::AgentRunWakeupKind::ProcessReady
        | project_store::AgentRunWakeupKind::ProcessOutput => {
            evaluate_process_wakeup(record, payload, tool_runtime, now)
        }
    }
}

fn evaluate_process_wakeup(
    record: &project_store::AgentRunWakeupRecord,
    mut payload: JsonValue,
    tool_runtime: Option<&AutonomousToolRuntime>,
    now: OffsetDateTime,
) -> CommandResult<WakeupEvaluation> {
    let Some(tool_runtime) = tool_runtime else {
        return Ok(missing_process_resume(record, payload));
    };
    let process_id = payload
        .get("processId")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CommandError::invalid_request("processId"))?;
    let status = match process_manager_output(tool_runtime.process_manager(
        process_wakeup_request(AutonomousProcessManagerAction::Status, process_id, None),
    )?) {
        Ok(output) => output,
        Err(error) if error.code == "autonomous_tool_process_manager_not_found" => {
            return Ok(missing_process_resume(record, payload));
        }
        Err(error) => return Err(error),
    };
    let metadata = status.processes.first().cloned();

    match record.kind {
        project_store::AgentRunWakeupKind::ProcessExit => {
            if metadata.as_ref().is_some_and(process_metadata_is_terminal) {
                Ok(WakeupEvaluation::Resume(WakeupResume {
                    status: project_store::AgentRunWakeupStatus::Fired,
                    outcome: "process_exited".into(),
                    diagnostic: None,
                    observation: json!({
                        "process": metadata,
                    }),
                }))
            } else {
                pending_process_wakeup(record, payload, now)
            }
        }
        project_store::AgentRunWakeupKind::ProcessReady => {
            if metadata.as_ref().is_some_and(process_metadata_is_ready) {
                Ok(WakeupEvaluation::Resume(WakeupResume {
                    status: project_store::AgentRunWakeupStatus::Fired,
                    outcome: "process_ready".into(),
                    diagnostic: None,
                    observation: json!({
                        "process": metadata,
                    }),
                }))
            } else {
                pending_process_wakeup(record, payload, now)
            }
        }
        project_store::AgentRunWakeupKind::ProcessOutput => {
            let pattern = payload
                .get("outputPattern")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| CommandError::invalid_request("outputPattern"))?;
            let regex = regex::Regex::new(pattern).map_err(|error| {
                CommandError::user_fixable(
                    "agent_run_wakeup_output_pattern_invalid",
                    format!(
                        "Scheduled wakeup `{}` has an invalid output regex: {error}",
                        record.wake_id
                    ),
                )
            })?;
            let after_cursor = payload.get("afterCursor").and_then(JsonValue::as_u64);
            let output = match process_manager_output(tool_runtime.process_manager(
                process_wakeup_request(
                    AutonomousProcessManagerAction::Output,
                    process_id,
                    after_cursor,
                ),
            )?) {
                Ok(output) => output,
                Err(error) if error.code == "autonomous_tool_process_manager_not_found" => {
                    return Ok(missing_process_resume(record, payload));
                }
                Err(error) => return Err(error),
            };
            let combined = output
                .chunks
                .iter()
                .filter_map(|chunk| chunk.text.as_deref())
                .collect::<Vec<_>>()
                .join("\n");
            if regex.is_match(&combined) {
                Ok(WakeupEvaluation::Resume(WakeupResume {
                    status: project_store::AgentRunWakeupStatus::Fired,
                    outcome: "process_output_matched".into(),
                    diagnostic: None,
                    observation: json!({
                        "processes": output.processes,
                        "chunks": output.chunks,
                        "nextCursor": output.next_cursor,
                    }),
                }))
            } else {
                if let Some(next_cursor) = output.next_cursor {
                    if let Some(object) = payload.as_object_mut() {
                        object.insert("afterCursor".into(), json!(next_cursor));
                    }
                }
                pending_process_wakeup(record, payload, now)
            }
        }
        project_store::AgentRunWakeupKind::Sleep => unreachable!("handled by caller"),
    }
}

fn pending_process_wakeup(
    record: &project_store::AgentRunWakeupRecord,
    payload: JsonValue,
    now: OffsetDateTime,
) -> CommandResult<WakeupEvaluation> {
    let poll_interval_ms = record.poll_interval_ms.unwrap_or(10_000);
    let due_at = format_wakeup_timestamp(add_wakeup_ms(now, poll_interval_ms)?)?;
    let payload_json = serde_json::to_string(&payload).map_err(|error| {
        CommandError::system_fault(
            "agent_run_wakeup_payload_serialize_failed",
            format!(
                "Xero could not serialize scheduled wakeup `{}` payload: {error}",
                record.wake_id
            ),
        )
    })?;
    Ok(WakeupEvaluation::Pending {
        due_at,
        payload_json,
    })
}

fn missing_process_resume(
    record: &project_store::AgentRunWakeupRecord,
    payload: JsonValue,
) -> WakeupEvaluation {
    WakeupEvaluation::Resume(WakeupResume {
        status: project_store::AgentRunWakeupStatus::Failed,
        outcome: "process_state_missing".into(),
        diagnostic: Some(project_store::AgentRunDiagnosticRecord {
            code: "agent_run_wakeup_process_missing".into(),
            message: format!(
                "Scheduled wakeup `{}` references an in-memory Xero-owned process that is no longer registered. This can happen after app restart or process cleanup.",
                record.wake_id
            ),
        }),
        observation: json!({
            "payload": payload,
            "diagnostic": "process_state_missing",
        }),
    })
}

fn resume_scheduled_wakeup<R: Runtime + 'static>(
    app: &AppHandle<R>,
    repo_root: &Path,
    record: &project_store::AgentRunWakeupRecord,
    resume: WakeupResume,
    tool_runtime: &mut Option<AutonomousToolRuntime>,
) -> CommandResult<bool> {
    let state = app.state::<DesktopState>();
    let runtime = DesktopAgentCoreRuntime::new(state.agent_run_supervisor().clone());
    if runtime.is_active(&record.run_id)? {
        return Ok(false);
    }

    let agent_run = project_store::load_agent_run(repo_root, &record.project_id, &record.run_id)?;
    let runtime_run = project_store::load_runtime_run(
        repo_root,
        &record.project_id,
        &agent_run.run.agent_session_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "agent_run_wakeup_runtime_run_missing",
            format!(
                "Xero could not resume scheduled wakeup `{}` because the durable runtime run for session `{}` is missing.",
                record.wake_id, agent_run.run.agent_session_id
            ),
        )
    })?;
    if runtime_run.run.run_id != record.run_id {
        return Err(CommandError::system_fault(
            "agent_run_wakeup_runtime_run_mismatch",
            format!(
                "Xero could not resume scheduled wakeup `{}` because the durable runtime run `{}` does not match paused agent run `{}`.",
                record.wake_id, runtime_run.run.run_id, record.run_id
            ),
        ));
    }

    let controls = runtime_control_input_from_active(&runtime_run.controls.active);
    let provider_config = resolve_owned_agent_provider_config(app, state.inner(), Some(&controls))?;
    let (provider_id, model_id) = agent_provider_config_identity(&provider_config);
    let requested_profile_id = controls
        .provider_profile_id
        .as_deref()
        .unwrap_or(provider_id.as_str());
    let requested_profile_id = requested_profile_id.trim();
    let requested_profile_id = if requested_profile_id.is_empty() {
        provider_id.as_str()
    } else {
        requested_profile_id
    };
    let provider_preflight = ensure_owned_runtime_provider_turn_capabilities(
        app,
        state.inner(),
        state.owned_agent_provider_config_override().is_none(),
        requested_profile_id,
        &provider_id,
        &model_id,
        &[],
    )?;
    let tool_runtime = match tool_runtime.take() {
        Some(runtime) => runtime,
        None => {
            scheduled_wakeup_tool_runtime(app, state.inner(), &record.project_id, &provider_config)?
        }
    };
    let resume_status = resume.status;
    let resume_diagnostic = resume.diagnostic.clone();
    let resume_payload = json!({
        "schema": "xero.agent_run_wakeup.resume.v1",
        "wakeId": record.wake_id,
        "kind": project_store::agent_run_wakeup_kind_sql_value(record.kind),
        "outcome": resume.outcome,
        "reason": record.payload().ok().and_then(|payload| payload.get("reason").cloned()),
        "dueAt": record.due_at,
        "deadlineAt": record.deadline_at,
        "diagnostic": resume_diagnostic.clone(),
        "observation": resume.observation,
    });
    let prompt = render_scheduled_wakeup_prompt(&resume_payload)?;
    let continuation = ContinueOwnedAgentRunRequest {
        repo_root: repo_root.to_path_buf(),
        project_id: record.project_id.clone(),
        run_id: record.run_id.clone(),
        continuation_request_id: format!("scheduled-wakeup:{}", record.wake_id),
        prompt,
        attachments: Vec::new(),
        linked_paths: Vec::new(),
        controls: None,
        tool_runtime,
        provider_config,
        provider_preflight: Some(provider_preflight),
        answer_pending_actions: false,
        answer_pending_action_id: None,
        auto_compact: None,
        internal_resume: Some(AgentRunInternalResume {
            wake_id: record.wake_id.clone(),
            reason: "scheduled_wakeup".into(),
            payload: resume_payload,
        }),
    };
    runtime.continue_run(continuation, DesktopRunDriveMode::Background)?;
    if let Err(error) =
        mark_scheduled_wakeup_resumed(repo_root, record, resume_status, resume_diagnostic)
    {
        eprintln!(
            "[agent-wakeup] could not mark scheduled wakeup `{}` as resumed: {}",
            record.wake_id, error.message
        );
    }
    Ok(true)
}

fn mark_scheduled_wakeup_resumed(
    repo_root: &Path,
    record: &project_store::AgentRunWakeupRecord,
    status: project_store::AgentRunWakeupStatus,
    diagnostic: Option<project_store::AgentRunDiagnosticRecord>,
) -> CommandResult<()> {
    match status {
        project_store::AgentRunWakeupStatus::Fired => {
            project_store::mark_agent_run_wakeup_fired(
                repo_root,
                &record.project_id,
                &record.run_id,
                &record.wake_id,
                &now_timestamp(),
            )?;
        }
        status => {
            project_store::mark_agent_run_wakeup_status(
                repo_root,
                &record.project_id,
                &record.run_id,
                &record.wake_id,
                status,
                diagnostic,
                &now_timestamp(),
            )?;
        }
    }
    Ok(())
}

fn persist_scheduled_wakeup_resume_failure(
    repo_root: &Path,
    record: &project_store::AgentRunWakeupRecord,
    error: &CommandError,
) {
    let diagnostic = project_store::AgentRunDiagnosticRecord {
        code: "agent_run_wakeup_resume_failed".into(),
        message: format!(
            "Scheduled wakeup `{}` reached its wake condition, but Xero could not resume the run: {}",
            record.wake_id, error.message
        ),
    };
    let now = now_timestamp();
    let _ = project_store::mark_agent_run_wakeup_status(
        repo_root,
        &record.project_id,
        &record.run_id,
        &record.wake_id,
        project_store::AgentRunWakeupStatus::Failed,
        Some(diagnostic.clone()),
        &now,
    );
    let stop_reason = stop_reason_for_error(error);
    let _ = record_state_transition(
        repo_root,
        &record.project_id,
        &record.run_id,
        AgentStateTransition {
            from: Some(AgentRunState::ScheduledWait),
            to: AgentRunState::Blocked,
            reason: "Scheduled wakeup could not resume the owned-agent run.",
            stop_reason: Some(stop_reason),
            extra: Some(json!({
                "wakeId": record.wake_id,
                "code": error.code,
                "message": error.message,
                "retryable": error.retryable,
            })),
        },
    );
    let _ = append_event(
        repo_root,
        &record.project_id,
        &record.run_id,
        AgentRunEventKind::RunFailed,
        json!({
            "code": diagnostic.code,
            "message": diagnostic.message,
            "retryable": error.retryable,
            "state": AgentRunState::Blocked.as_str(),
            "stopReason": stop_reason.as_str(),
            "wakeId": record.wake_id,
            "resumeError": {
                "code": error.code,
                "message": error.message,
            },
        }),
    );
    let _ = project_store::update_agent_run_status(
        repo_root,
        &record.project_id,
        &record.run_id,
        AgentRunStatus::Failed,
        Some(diagnostic),
        &now,
    );
}

fn scheduled_wakeup_tool_runtime<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    provider_config: &AgentProviderConfig,
) -> CommandResult<AutonomousToolRuntime> {
    let (provider_id, model_id) = agent_provider_config_identity(provider_config);
    let policy = crate::commands::agent_tooling_settings::resolve_agent_tool_application_style(
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

fn render_scheduled_wakeup_prompt(payload: &JsonValue) -> CommandResult<String> {
    let serialized = serde_json::to_string_pretty(payload).map_err(|error| {
        CommandError::system_fault(
            "agent_run_wakeup_resume_payload_serialize_failed",
            format!("Xero could not serialize scheduled wakeup resume context: {error}"),
        )
    })?;
    Ok(format!(
        "Internal Xero scheduled wakeup: the previous runtime_wait has now elapsed or reached its wake condition. This is runtime/developer context, not a new user request. Continue the prior task using this wakeup observation, respect all existing user instructions and tool policy, and do not quote, summarize, or expose this wakeup payload to the user.\n\n```json\n{serialized}\n```"
    ))
}

fn process_wakeup_request(
    action: AutonomousProcessManagerAction,
    process_id: &str,
    after_cursor: Option<u64>,
) -> AutonomousProcessManagerRequest {
    AutonomousProcessManagerRequest {
        action,
        process_id: Some(process_id.to_string()),
        pid: None,
        parent_pid: None,
        port: None,
        group: None,
        label: None,
        process_type: None,
        argv: Vec::new(),
        cwd: None,
        shell_mode: false,
        interactive: false,
        target_ownership: None,
        persistent: false,
        timeout_ms: None,
        after_cursor,
        since_last_read: false,
        max_bytes: Some(64 * 1024),
        tail_lines: None,
        stream: None,
        filter: None,
        input: None,
        wait_pattern: None,
        wait_port: None,
        wait_url: None,
        signal: None,
    }
}

fn process_manager_output(
    result: AutonomousToolResult,
) -> CommandResult<AutonomousProcessManagerOutput> {
    match result.output {
        AutonomousToolOutput::ProcessManager(output) => Ok(output),
        _ => Err(CommandError::system_fault(
            "agent_run_wakeup_process_output_invalid",
            "Process-manager wakeup evaluation received a non-process-manager tool result.",
        )),
    }
}

fn process_metadata_is_terminal(metadata: &AutonomousProcessMetadata) -> bool {
    matches!(
        metadata.status,
        AutonomousProcessStatus::Exited
            | AutonomousProcessStatus::Failed
            | AutonomousProcessStatus::Killed
    ) || metadata.exit_code.is_some()
}

fn process_metadata_is_ready(metadata: &AutonomousProcessMetadata) -> bool {
    metadata.readiness.ready || metadata.status == AutonomousProcessStatus::Ready
}

fn wakeup_key(record: &project_store::AgentRunWakeupRecord) -> String {
    format!("{}:{}:{}", record.project_id, record.run_id, record.wake_id)
}

fn sleep_until(due_at: OffsetDateTime) {
    let now = OffsetDateTime::now_utc();
    if due_at <= now {
        return;
    }
    let millis = (due_at - now).whole_milliseconds().clamp(100, 60_000) as u64;
    sleep_for_ms(millis);
}

fn sleep_for_ms(millis: u64) {
    thread::sleep(StdDuration::from_millis(millis));
}

fn parse_wakeup_timestamp(value: &str) -> CommandResult<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| {
        CommandError::retryable(
            "agent_run_wakeup_timestamp_parse_failed",
            format!("Xero could not parse scheduled wakeup timestamp `{value}`: {error}"),
        )
    })
}

fn add_wakeup_ms(timestamp: OffsetDateTime, millis: u64) -> CommandResult<OffsetDateTime> {
    let millis =
        i64::try_from(millis).map_err(|_| CommandError::invalid_request("pollIntervalMs"))?;
    timestamp
        .checked_add(TimeDuration::milliseconds(millis))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_run_wakeup_timestamp_out_of_range",
                "Scheduled wakeup timestamp is outside the supported range.",
            )
        })
}

fn format_wakeup_timestamp(timestamp: OffsetDateTime) -> CommandResult<String> {
    timestamp.format(&Rfc3339).map_err(|error| {
        CommandError::system_fault(
            "agent_run_wakeup_timestamp_format_failed",
            format!("Xero could not format scheduled wakeup timestamp: {error}"),
        )
    })
}
