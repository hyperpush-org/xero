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
        AutonomousProcessOutputChunk, AutonomousProcessStatus,
    },
    state::DesktopState,
};

const PROCESS_WAKEUP_OUTPUT_WINDOW_CHARS: usize = 64 * 1024;

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
    let status = match tool_runtime
        .process_manager(process_wakeup_request(
            AutonomousProcessManagerAction::Status,
            process_id,
            None,
        ))
        .and_then(process_manager_output)
    {
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
            let output = match tool_runtime
                .process_manager(process_wakeup_request(
                    AutonomousProcessManagerAction::Output,
                    process_id,
                    after_cursor,
                ))
                .and_then(process_manager_output)
            {
                Ok(output) => output,
                Err(error) if error.code == "autonomous_tool_process_manager_not_found" => {
                    return Ok(missing_process_resume(record, payload));
                }
                Err(error) => return Err(error),
            };
            let previous_window = payload
                .get("outputWindow")
                .and_then(JsonValue::as_str)
                .unwrap_or_default();
            let combined = merge_process_output_window(previous_window, &output.chunks);
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
                if let Some(object) = payload.as_object_mut() {
                    object.insert("outputWindow".into(), json!(combined));
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

fn merge_process_output_window(
    previous_window: &str,
    chunks: &[AutonomousProcessOutputChunk],
) -> String {
    let appended_chars = chunks
        .iter()
        .filter_map(|chunk| chunk.text.as_deref())
        .map(str::chars)
        .map(Iterator::count)
        .sum::<usize>();
    let mut combined = String::with_capacity(previous_window.len() + appended_chars);
    combined.push_str(previous_window);
    for text in chunks.iter().filter_map(|chunk| chunk.text.as_deref()) {
        combined.push_str(text);
    }
    if combined.chars().count() <= PROCESS_WAKEUP_OUTPUT_WINDOW_CHARS {
        return combined;
    }

    combined
        .chars()
        .rev()
        .take(PROCESS_WAKEUP_OUTPUT_WINDOW_CHARS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, git::repository::CanonicalRepository, runtime::AutonomousProcessOutputStream};

    fn wakeup(
        kind: project_store::AgentRunWakeupKind,
        payload: JsonValue,
    ) -> project_store::AgentRunWakeupRecord {
        project_store::AgentRunWakeupRecord {
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-1".into(),
            wake_id: "wake-1".into(),
            kind,
            due_at: "2026-07-18T12:00:00Z".into(),
            deadline_at: None,
            poll_interval_ms: Some(2_500),
            payload_json: payload.to_string(),
            status: project_store::AgentRunWakeupStatus::Pending,
            attempt_count: 0,
            last_error: None,
            fired_at: None,
            created_at: "2026-07-18T11:59:00Z".into(),
            updated_at: "2026-07-18T11:59:00Z".into(),
        }
    }

    fn timestamp(value: &str) -> OffsetDateTime {
        OffsetDateTime::parse(value, &Rfc3339).expect("valid fixture timestamp")
    }

    fn output_chunk(cursor: u64, text: &str) -> AutonomousProcessOutputChunk {
        AutonomousProcessOutputChunk {
            cursor,
            stream: AutonomousProcessOutputStream::Stdout,
            text: Some(text.into()),
            truncated: false,
            redacted: false,
            captured_at: None,
        }
    }

    #[test]
    fn process_output_window_preserves_matches_split_across_poll_boundaries() {
        let first = merge_process_output_window("", &[output_chunk(1, "Server rea")]);
        assert_eq!(first, "Server rea");

        let second = merge_process_output_window(&first, &[output_chunk(2, "dy on :3000\n")]);

        assert!(regex::Regex::new("Server ready on :3000")
            .expect("valid regex")
            .is_match(&second));
    }

    #[test]
    fn process_output_window_is_unicode_safe_and_memory_bounded() {
        let oversized = "🦀".repeat(PROCESS_WAKEUP_OUTPUT_WINDOW_CHARS + 2);

        let window = merge_process_output_window(&oversized, &[output_chunk(2, "done")]);

        assert_eq!(window.chars().count(), PROCESS_WAKEUP_OUTPUT_WINDOW_CHARS);
        assert!(window.ends_with("done"));
    }

    #[test]
    fn sleep_and_deadline_wakeups_produce_complete_resume_observations() {
        let now = timestamp("2026-07-18T12:00:01Z");
        let sleep = wakeup(
            project_store::AgentRunWakeupKind::Sleep,
            json!({"reason": "brief pause"}),
        );
        let WakeupEvaluation::Resume(sleep_resume) =
            evaluate_wakeup(&sleep, None, now).expect("evaluate sleep wakeup")
        else {
            panic!("elapsed sleep wakeup must resume")
        };
        assert_eq!(
            sleep_resume.status,
            project_store::AgentRunWakeupStatus::Fired
        );
        assert_eq!(sleep_resume.outcome, "timer_elapsed");
        assert_eq!(sleep_resume.observation["dueAt"], sleep.due_at);
        assert!(sleep_resume.diagnostic.is_none());

        let mut expired = sleep;
        expired.deadline_at = Some("2026-07-18T12:00:00Z".into());
        let WakeupEvaluation::Resume(expired_resume) =
            evaluate_wakeup(&expired, None, now).expect("evaluate expired wakeup")
        else {
            panic!("expired wakeup must resume with a diagnostic")
        };
        assert_eq!(
            expired_resume.status,
            project_store::AgentRunWakeupStatus::Expired
        );
        assert_eq!(expired_resume.outcome, "expired");
        assert_eq!(
            expired_resume.diagnostic.expect("deadline diagnostic").code,
            "agent_run_wakeup_deadline_expired"
        );
        assert_eq!(
            expired_resume.observation["deadlineAt"],
            "2026-07-18T12:00:00Z"
        );
    }

    #[test]
    fn process_wakeups_fail_closed_when_the_in_memory_runtime_is_gone() {
        for kind in [
            project_store::AgentRunWakeupKind::ProcessExit,
            project_store::AgentRunWakeupKind::ProcessReady,
            project_store::AgentRunWakeupKind::ProcessOutput,
        ] {
            let record = wakeup(
                kind,
                json!({
                    "processId": "process-1",
                    "outputPattern": "ready",
                }),
            );
            let WakeupEvaluation::Resume(resume) =
                evaluate_wakeup(&record, None, timestamp("2026-07-18T12:00:01Z"))
                    .expect("evaluate missing process runtime")
            else {
                panic!("missing process state must terminate the wakeup")
            };
            assert_eq!(resume.status, project_store::AgentRunWakeupStatus::Failed);
            assert_eq!(resume.outcome, "process_state_missing");
            assert_eq!(
                resume.diagnostic.expect("missing process diagnostic").code,
                "agent_run_wakeup_process_missing"
            );
        }
    }

    #[test]
    fn pending_process_wakeup_advances_due_time_and_preserves_payload() {
        let record = wakeup(
            project_store::AgentRunWakeupKind::ProcessOutput,
            json!({"processId": "process-1", "afterCursor": 12}),
        );

        let WakeupEvaluation::Pending {
            due_at,
            payload_json,
        } = pending_process_wakeup(
            &record,
            record.payload().expect("fixture payload"),
            timestamp("2026-07-18T12:00:00Z"),
        )
        .expect("reschedule pending wakeup")
        else {
            panic!("unmatched process wakeup must remain pending")
        };

        assert_eq!(due_at, "2026-07-18T12:00:02.5Z");
        assert_eq!(
            serde_json::from_str::<JsonValue>(&payload_json).expect("serialized payload")
                ["afterCursor"],
            12
        );
    }

    #[test]
    fn scheduler_helpers_validate_time_requests_and_internal_prompt_shape() {
        let record = wakeup(
            project_store::AgentRunWakeupKind::Sleep,
            json!({"reason": "continue checks"}),
        );
        assert_eq!(wakeup_key(&record), "project-1:run-1:wake-1");

        let request = process_wakeup_request(
            AutonomousProcessManagerAction::Output,
            " process-1 ",
            Some(7),
        );
        assert_eq!(request.action, AutonomousProcessManagerAction::Output);
        assert_eq!(request.process_id.as_deref(), Some(" process-1 "));
        assert_eq!(request.after_cursor, Some(7));
        assert_eq!(request.max_bytes, Some(64 * 1024));

        let prompt = render_scheduled_wakeup_prompt(&json!({
            "wakeId": "wake-1",
            "outcome": "timer_elapsed",
        }))
        .expect("render internal resume prompt");
        assert!(prompt.contains("not a new user request"));
        assert!(prompt.contains("\"wakeId\": \"wake-1\""));

        assert_eq!(
            format_wakeup_timestamp(timestamp("2026-07-18T12:00:00Z")).expect("format timestamp"),
            "2026-07-18T12:00:00Z"
        );
        assert_eq!(
            parse_wakeup_timestamp("not-a-timestamp")
                .expect_err("reject malformed timestamp")
                .code,
            "agent_run_wakeup_timestamp_parse_failed"
        );
        assert_eq!(
            add_wakeup_ms(timestamp("2026-07-18T12:00:00Z"), u64::MAX)
                .expect_err("reject intervals outside i64")
                .code,
            "invalid_request"
        );
        assert_eq!(
            add_wakeup_ms(
                time::Date::MAX
                    .with_hms_milli(23, 59, 59, 999)
                    .expect("maximum fixture time")
                    .assume_utc(),
                1,
            )
            .expect_err("reject timestamps outside supported range")
            .code,
            "agent_run_wakeup_timestamp_out_of_range"
        );
    }

    #[test]
    fn scheduler_deduplicates_workers_and_cancels_terminal_run_wakeups() {
        let fixture = tempfile::tempdir().expect("create scheduler fixture");
        let repo_root = seed_project(&fixture);
        let record = seed_pending_wakeup(&repo_root);
        let state = DesktopState::default();
        let app = crate::configure_builder_with_state(tauri::test::mock_builder(), state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock application");
        let scheduler = AgentRunWakeupScheduler::default();

        assert!(scheduler
            .schedule_record(
                app.handle().clone(),
                repo_root.clone(),
                record.clone(),
                None,
            )
            .expect("schedule first worker"));
        assert!(!scheduler
            .schedule_record(app.handle().clone(), repo_root.clone(), record, None)
            .expect("deduplicate active worker"));

        project_store::update_agent_run_status(
            &repo_root,
            "project-scheduler",
            "run-scheduler",
            AgentRunStatus::Cancelled,
            None,
            &now_timestamp(),
        )
        .expect("cancel fixture run");

        let started = std::time::Instant::now();
        loop {
            let wakeup = project_store::load_agent_run_wakeup(
                &repo_root,
                "project-scheduler",
                "run-scheduler",
                "wake-scheduler",
            )
            .expect("load scheduled wakeup");
            if wakeup.status == project_store::AgentRunWakeupStatus::Cancelled {
                break;
            }
            assert!(
                started.elapsed() < StdDuration::from_secs(3),
                "scheduled worker did not cancel the terminal run wakeup"
            );
            thread::sleep(StdDuration::from_millis(20));
        }

        assert_eq!(
            scheduler
                .schedule_pending_for_project(app.handle().clone(), repo_root)
                .expect("scan terminal project's pending wakeups"),
            0
        );
    }

    #[test]
    fn scheduled_wakeup_failure_fixture_persists_terminal_diagnostics_and_non_fired_statuses() {
        let fixture = tempfile::tempdir().expect("create wakeup failure fixture");
        let repo_root = seed_project(&fixture);
        let record = seed_pending_wakeup(&repo_root);
        project_store::update_agent_run_status(
            &repo_root,
            "project-scheduler",
            "run-scheduler",
            AgentRunStatus::Paused,
            None,
            &now_timestamp(),
        )
        .expect("pause wakeup fixture run");

        let expired_diagnostic = project_store::AgentRunDiagnosticRecord {
            code: "agent_run_wakeup_deadline_expired".into(),
            message: "The fixture wakeup reached its deadline.".into(),
        };
        mark_scheduled_wakeup_resumed(
            &repo_root,
            &record,
            project_store::AgentRunWakeupStatus::Expired,
            Some(expired_diagnostic.clone()),
        )
        .expect("persist non-fired wakeup result");
        let expired = project_store::load_agent_run_wakeup(
            &repo_root,
            "project-scheduler",
            "run-scheduler",
            "wake-scheduler",
        )
        .expect("load expired wakeup");
        assert_eq!(
            expired.status,
            project_store::AgentRunWakeupStatus::Expired
        );
        assert_eq!(expired.last_error, Some(expired_diagnostic));

        let resume_error = CommandError::retryable(
            "fixture_provider_unavailable",
            "The fixture provider was unavailable.",
        );
        persist_scheduled_wakeup_resume_failure(&repo_root, &record, &resume_error);

        let failed_wakeup = project_store::load_agent_run_wakeup(
            &repo_root,
            "project-scheduler",
            "run-scheduler",
            "wake-scheduler",
        )
        .expect("load failed wakeup");
        assert_eq!(
            failed_wakeup.status,
            project_store::AgentRunWakeupStatus::Failed
        );
        assert_eq!(
            failed_wakeup
                .last_error
                .as_ref()
                .map(|diagnostic| diagnostic.code.as_str()),
            Some("agent_run_wakeup_resume_failed")
        );

        let snapshot = project_store::load_agent_run(
            &repo_root,
            "project-scheduler",
            "run-scheduler",
        )
        .expect("load failed run");
        assert_eq!(snapshot.run.status, AgentRunStatus::Failed);
        let failure = snapshot
            .events
            .iter()
            .find(|event| event.event_kind == AgentRunEventKind::RunFailed)
            .expect("persisted wakeup failure event");
        let payload: JsonValue =
            serde_json::from_str(&failure.payload_json).expect("decode wakeup failure event");
        assert_eq!(payload["resumeError"]["code"], json!(resume_error.code));
        assert_eq!(payload["wakeId"], json!(record.wake_id));
    }

    #[test]
    fn scheduled_wakeup_resume_fixture_deduplicates_active_drive_then_continues_fake_provider() {
        let fixture = tempfile::tempdir().expect("create wakeup resume fixture");
        let repo_root = seed_project(&fixture);
        let database_path = db::database_path_for_repo(&repo_root);
        let record = seed_pending_wakeup(&repo_root);
        project_store::update_agent_run_status(
            &repo_root,
            "project-scheduler",
            "run-scheduler",
            AgentRunStatus::Paused,
            None,
            &now_timestamp(),
        )
        .expect("pause wakeup fixture run");
        let controls = project_store::build_runtime_run_control_state_with_profile(
            RuntimeAgentIdDto::Engineer,
            Some("engineer"),
            Some(project_store::BUILTIN_AGENT_DEFINITION_VERSION),
            Some(OPENAI_CODEX_PROVIDER_ID),
            "fake-model",
            None,
            RuntimeRunApprovalModeDto::Yolo,
            false,
            "2026-07-18T12:00:00Z",
            None,
        )
        .expect("build wakeup runtime controls");
        project_store::upsert_runtime_run(
            &repo_root,
            &project_store::RuntimeRunUpsertRecord {
                run: project_store::RuntimeRunRecord {
                    project_id: "project-scheduler".into(),
                    agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
                    run_id: "run-scheduler".into(),
                    runtime_kind: OPENAI_CODEX_PROVIDER_ID.into(),
                    provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
                    supervisor_kind: OWNED_AGENT_SUPERVISOR_KIND.into(),
                    status: project_store::RuntimeRunStatus::Running,
                    transport: project_store::RuntimeRunTransportRecord {
                        kind: "internal".into(),
                        endpoint: "xero://owned-agent".into(),
                        liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                    },
                    started_at: "2026-07-18T12:00:00Z".into(),
                    last_heartbeat_at: Some("2026-07-18T12:00:00Z".into()),
                    stopped_at: None,
                    last_error: None,
                    updated_at: "2026-07-18T12:00:00Z".into(),
                },
                checkpoint: None,
                control_state: Some(controls),
                expected_snapshot: None,
            },
        )
        .expect("persist wakeup runtime run");

        let registry_path = fixture.path().join("app-data/xero.db");
        let state = DesktopState::default()
            .with_global_db_path_override(registry_path)
            .with_owned_agent_provider_config_override(AgentProviderConfig::Fake);
        let app = crate::configure_builder_with_state(tauri::test::mock_builder(), state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build wakeup resume app");
        db::register_project_database_path_for_tests(&repo_root, database_path);
        let resume = WakeupResume {
            status: project_store::AgentRunWakeupStatus::Fired,
            outcome: "timer_elapsed".into(),
            diagnostic: None,
            observation: json!({"elapsed": true}),
        };
        let mut tool_runtime = Some(
            AutonomousToolRuntime::new(&repo_root).expect("wakeup continuation tool runtime"),
        );
        let lease = app
            .state::<DesktopState>()
            .agent_run_supervisor()
            .begin_persisted(
                &repo_root,
                "project-scheduler",
                project_store::DEFAULT_AGENT_SESSION_ID,
                "run-scheduler",
            )
            .expect("acquire active wakeup lease");
        assert!(!resume_scheduled_wakeup(
            app.handle(),
            &repo_root,
            &record,
            resume.clone(),
            &mut tool_runtime,
        )
        .expect("active wakeup resume is deduplicated"));
        assert!(tool_runtime.is_some(), "deduplication must not consume runtime");
        drop(lease);

        assert!(resume_scheduled_wakeup(
            app.handle(),
            &repo_root,
            &record,
            resume,
            &mut tool_runtime,
        )
        .expect("resume scheduled wakeup"));
        assert!(tool_runtime.is_none());
        let fired = project_store::load_agent_run_wakeup(
            &repo_root,
            "project-scheduler",
            "run-scheduler",
            "wake-scheduler",
        )
        .expect("load fired wakeup");
        assert_eq!(fired.status, project_store::AgentRunWakeupStatus::Fired);

        let deadline = std::time::Instant::now() + StdDuration::from_secs(5);
        loop {
            let snapshot = project_store::load_agent_run(
                &repo_root,
                "project-scheduler",
                "run-scheduler",
            )
            .expect("load resumed wakeup run");
            if snapshot.run.status == AgentRunStatus::Completed {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "fake-provider wakeup continuation did not complete: {:?}",
                snapshot.run.status
            );
            thread::sleep(StdDuration::from_millis(20));
        }
    }

    #[cfg(unix)]
    #[test]
    fn process_wakeup_evaluation_covers_ready_output_pending_and_exit_states() {
        let fixture = tempfile::tempdir().expect("create process fixture");
        let runtime = AutonomousToolRuntime::new(fixture.path())
            .expect("create tool runtime")
            .with_runtime_run_controls(RuntimeRunControlStateDto {
                active: RuntimeRunActiveControlSnapshotDto {
                    runtime_agent_id: RuntimeAgentIdDto::Engineer,
                    agent_definition_id: None,
                    agent_definition_version: None,
                    provider_profile_id: None,
                    model_id: "fixture-model".into(),
                    thinking_effort: None,
                    approval_mode: RuntimeRunApprovalModeDto::Yolo,
                    plan_mode_required: false,
                    auto_compact_enabled: true,
                    revision: 1,
                    applied_at: now_timestamp(),
                },
                pending: None,
            });
        let mut start = base_process_request(AutonomousProcessManagerAction::Start);
        start.argv = vec![
            "sh".into(),
            "-c".into(),
            "printf 'scheduler-ready'; sleep 5".into(),
        ];
        let started = process_manager_output(
            runtime
                .process_manager_with_operator_approval(start)
                .expect("start fixture process"),
        )
        .expect("decode start output");
        let process_id = started.process_id.expect("fixture process id");

        let mut wait_ready = base_process_request(AutonomousProcessManagerAction::WaitForReady);
        wait_ready.process_id = Some(process_id.clone());
        wait_ready.wait_pattern = Some("scheduler-ready".into());
        wait_ready.timeout_ms = Some(2_000);
        process_manager_output(
            runtime
                .process_manager(wait_ready)
                .expect("wait for fixture readiness"),
        )
        .expect("decode readiness output");

        let ready_record = wakeup(
            project_store::AgentRunWakeupKind::ProcessReady,
            json!({"processId": process_id}),
        );
        let WakeupEvaluation::Resume(ready) = evaluate_wakeup(
            &ready_record,
            Some(&runtime),
            timestamp("2026-07-18T12:00:01Z"),
        )
        .expect("evaluate ready process") else {
            panic!("ready process must fire")
        };
        assert_eq!(ready.outcome, "process_ready");

        let output_record = wakeup(
            project_store::AgentRunWakeupKind::ProcessOutput,
            json!({
                "processId": ready_record.payload().expect("ready payload")["processId"],
                "outputPattern": "scheduler-ready",
            }),
        );
        let WakeupEvaluation::Resume(output) = evaluate_wakeup(
            &output_record,
            Some(&runtime),
            timestamp("2026-07-18T12:00:01Z"),
        )
        .expect("evaluate matching process output") else {
            panic!("matching output must fire")
        };
        assert_eq!(output.outcome, "process_output_matched");

        let mut unmatched_record = output_record.clone();
        unmatched_record.payload_json = json!({
            "processId": ready_record.payload().expect("ready payload")["processId"],
            "outputPattern": "never-present",
        })
        .to_string();
        assert!(matches!(
            evaluate_wakeup(
                &unmatched_record,
                Some(&runtime),
                timestamp("2026-07-18T12:00:01Z"),
            )
            .expect("evaluate unmatched output"),
            WakeupEvaluation::Pending { .. }
        ));

        let mut invalid_pattern = unmatched_record.clone();
        invalid_pattern.payload_json = json!({
            "processId": ready_record.payload().expect("ready payload")["processId"],
            "outputPattern": "[",
        })
        .to_string();
        assert_eq!(
            evaluate_wakeup(
                &invalid_pattern,
                Some(&runtime),
                timestamp("2026-07-18T12:00:01Z"),
            )
            .expect_err("reject invalid output regex")
            .code,
            "agent_run_wakeup_output_pattern_invalid"
        );

        let exit_record = wakeup(
            project_store::AgentRunWakeupKind::ProcessExit,
            json!({
                "processId": ready_record.payload().expect("ready payload")["processId"],
            }),
        );
        assert!(matches!(
            evaluate_wakeup(
                &exit_record,
                Some(&runtime),
                timestamp("2026-07-18T12:00:01Z"),
            )
            .expect("evaluate running process"),
            WakeupEvaluation::Pending { .. }
        ));

        let process_id = exit_record.payload().expect("exit payload")["processId"]
            .as_str()
            .expect("process id string")
            .to_owned();
        let mut kill = base_process_request(AutonomousProcessManagerAction::Kill);
        kill.process_id = Some(process_id);
        process_manager_output(runtime.process_manager(kill).expect("kill fixture process"))
            .expect("decode kill output");

        let WakeupEvaluation::Resume(missing_after_cleanup) = evaluate_wakeup(
            &exit_record,
            Some(&runtime),
            timestamp("2026-07-18T12:00:02Z"),
        )
        .expect("evaluate cleaned-up process") else {
            panic!("cleaned-up process must terminate the wakeup")
        };
        assert_eq!(missing_after_cleanup.outcome, "process_state_missing");

        let mut short_start = base_process_request(AutonomousProcessManagerAction::Start);
        short_start.argv = vec!["sh".into(), "-c".into(), "sleep 0.1; exit 7".into()];
        let short_process_id = process_manager_output(
            runtime
                .process_manager_with_operator_approval(short_start)
                .expect("start short fixture process"),
        )
        .expect("decode short process output")
        .process_id
        .expect("short fixture process id");
        let short_exit_record = wakeup(
            project_store::AgentRunWakeupKind::ProcessExit,
            json!({"processId": short_process_id}),
        );
        let started_waiting = std::time::Instant::now();
        loop {
            match evaluate_wakeup(
                &short_exit_record,
                Some(&runtime),
                timestamp("2026-07-18T12:00:03Z"),
            )
            .expect("evaluate naturally exiting process")
            {
                WakeupEvaluation::Resume(exited) => {
                    assert_eq!(exited.outcome, "process_exited");
                    break;
                }
                WakeupEvaluation::Pending { .. } => {
                    assert!(
                        started_waiting.elapsed() < StdDuration::from_secs(2),
                        "short fixture process did not exit"
                    );
                    thread::sleep(StdDuration::from_millis(20));
                }
            }
        }
    }

    fn seed_project(root: &tempfile::TempDir) -> PathBuf {
        let repo_root = root.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("create fixture repository");
        let canonical_root = std::fs::canonicalize(&repo_root).expect("canonical fixture root");
        let repository = CanonicalRepository {
            project_id: "project-scheduler".into(),
            repository_id: "repository-scheduler".into(),
            root_path: canonical_root.clone(),
            root_path_string: canonical_root.to_string_lossy().into_owned(),
            common_git_dir: canonical_root.join(".git"),
            display_name: "Scheduler fixture".into(),
            branch_name: Some("main".into()),
            head_sha: Some("abc123".into()),
            branch: None,
            last_commit: None,
            status_entries: Vec::new(),
            has_staged_changes: false,
            has_unstaged_changes: false,
            has_untracked_changes: false,
            additions: 0,
            deletions: 0,
        };
        db::configure_project_database_paths(&root.path().join("app-data").join("xero.db"));
        db::import_project(&repository, DesktopState::default().import_failpoints())
            .expect("import scheduler fixture project");
        canonical_root
    }

    fn base_process_request(
        action: AutonomousProcessManagerAction,
    ) -> AutonomousProcessManagerRequest {
        AutonomousProcessManagerRequest {
            action,
            process_id: None,
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
            after_cursor: None,
            since_last_read: false,
            max_bytes: None,
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

    fn seed_pending_wakeup(repo_root: &Path) -> project_store::AgentRunWakeupRecord {
        project_store::insert_agent_run(
            repo_root,
            &project_store::NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: Some("engineer".into()),
                agent_definition_version: Some(1),
                project_id: "project-scheduler".into(),
                agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
                run_id: "run-scheduler".into(),
                provider_id: "fixture-provider".into(),
                model_id: "fixture-model".into(),
                prompt: "Wait, then continue.".into(),
                system_prompt: "scheduler-fixture".into(),
                now: "2026-07-18T12:00:00Z".into(),
            },
        )
        .expect("insert scheduler fixture run");
        project_store::insert_agent_run_wakeup(
            repo_root,
            &project_store::NewAgentRunWakeupRecord {
                project_id: "project-scheduler".into(),
                agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
                run_id: "run-scheduler".into(),
                wake_id: "wake-scheduler".into(),
                kind: project_store::AgentRunWakeupKind::Sleep,
                due_at: "2026-07-18T12:00:01Z".into(),
                deadline_at: None,
                poll_interval_ms: None,
                payload_json: json!({"reason": "scheduler fixture"}).to_string(),
                created_at: "2026-07-18T12:00:00Z".into(),
            },
        )
        .expect("insert scheduler fixture wakeup")
    }
}
