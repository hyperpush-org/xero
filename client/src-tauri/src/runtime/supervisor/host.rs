use std::{
    collections::HashMap,
    io::Read,
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        mpsc::{sync_channel, RecvTimeoutError},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use crate::{
    auth::now_timestamp,
    commands::{validate_non_empty, CommandError, RuntimeRunControlInputDto},
    db::project_store::{
        self, RuntimeRunControlStateRecord, RuntimeRunDiagnosticRecord, RuntimeRunRecord,
        RuntimeRunSnapshotRecord, RuntimeRunStatus, RuntimeRunTransportLiveness,
        RuntimeRunTransportRecord, RuntimeRunUpsertRecord,
    },
    runtime::{
        platform_adapter::resolve_runtime_supervisor_binary,
        protocol::{
            RuntimeSupervisorLaunchContext, SupervisorControlRequest, SupervisorControlResponse,
            SupervisorProcessStatus, SupervisorStartupMessage, SUPERVISOR_KIND_DETACHED_PTY,
            SUPERVISOR_PROTOCOL_VERSION, SUPERVISOR_TRANSPORT_KIND_TCP,
        },
    },
    state::DesktopState,
};

use super::persistence::protocol_diagnostic_into_record;
use super::{
    read_json_line_from_reader, runtime_supervisor_thinking_effort_env_value,
    validate_runtime_supervisor_launch_context, write_json_line, RuntimeSupervisorLaunchEnv,
    ANTHROPIC_API_KEY_ENV, CADENCE_AGENT_SESSION_ID_ENV, CADENCE_GLOBAL_DB_PATH_ENV,
    CADENCE_RUNTIME_FLOW_ID_ENV, CADENCE_RUNTIME_MCP_CONFIG_PATH_ENV,
    CADENCE_RUNTIME_MCP_CONTRACT_REQUIRED_ENV, CADENCE_RUNTIME_MODEL_ID_ENV,
    CADENCE_RUNTIME_PROVIDER_ID_ENV, CADENCE_RUNTIME_SESSION_ID_ENV,
    CADENCE_RUNTIME_THINKING_EFFORT_ENV, DEFAULT_CONTROL_TIMEOUT, DEFAULT_STARTUP_TIMEOUT,
    DEFAULT_STOP_TIMEOUT, OPENAI_API_KEY_ENV, OPENAI_API_VERSION_ENV, OPENAI_BASE_URL_ENV,
};

#[derive(Debug, Clone, Default)]
pub struct RuntimeSupervisorController {
    inner: Arc<Mutex<RuntimeSupervisorRegistry>>,
}

#[derive(Debug, Default)]
struct RuntimeSupervisorRegistry {
    active: HashMap<String, ActiveRuntimeSupervisor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveRuntimeSupervisor {
    project_id: String,
    agent_session_id: String,
    run_id: String,
    endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRuntimeSupervisorSnapshot {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub endpoint: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorLaunchRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub repo_root: PathBuf,
    pub runtime_kind: String,
    pub run_id: String,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub launch_context: RuntimeSupervisorLaunchContext,
    pub launch_env: RuntimeSupervisorLaunchEnv,
    pub program: String,
    pub args: Vec<String>,
    pub startup_timeout: Duration,
    pub control_timeout: Duration,
    pub supervisor_binary: Option<PathBuf>,
    pub run_controls: RuntimeRunControlStateRecord,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorProbeRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub repo_root: PathBuf,
    pub control_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorStopRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub repo_root: PathBuf,
    pub control_timeout: Duration,
    pub shutdown_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorSubmitInputRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub repo_root: PathBuf,
    pub run_id: String,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub action_id: String,
    pub boundary_id: String,
    pub input: String,
    pub control_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorUpdateControlsRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub repo_root: PathBuf,
    pub run_id: String,
    pub controls: Option<RuntimeRunControlInputDto>,
    pub prompt: Option<String>,
    pub control_timeout: Duration,
}

impl Default for RuntimeSupervisorUpdateControlsRequest {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            agent_session_id: String::new(),
            repo_root: PathBuf::new(),
            run_id: String::new(),
            controls: None,
            prompt: None,
            control_timeout: DEFAULT_CONTROL_TIMEOUT,
        }
    }
}

impl Default for RuntimeSupervisorLaunchRequest {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            agent_session_id: String::new(),
            repo_root: PathBuf::new(),
            runtime_kind: "openai_codex".into(),
            run_id: String::new(),
            session_id: String::new(),
            flow_id: None,
            launch_context: RuntimeSupervisorLaunchContext {
                provider_id: "openai_codex".into(),
                session_id: String::new(),
                flow_id: None,
                model_id: "openai_codex".into(),
                thinking_effort: None,
            },
            launch_env: RuntimeSupervisorLaunchEnv::default(),
            program: String::new(),
            args: Vec::new(),
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
            control_timeout: DEFAULT_CONTROL_TIMEOUT,
            supervisor_binary: None,
            run_controls: RuntimeRunControlStateRecord {
                active: project_store::RuntimeRunActiveControlSnapshotRecord {
                    provider_profile_id: None,
                    model_id: "openai_codex".into(),
                    thinking_effort: None,
                    approval_mode: crate::commands::RuntimeRunApprovalModeDto::Suggest,
                    plan_mode_required: false,
                    revision: 1,
                    applied_at: crate::auth::now_timestamp(),
                },
                pending: None,
            },
        }
    }
}

impl Default for RuntimeSupervisorProbeRequest {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            agent_session_id: String::new(),
            repo_root: PathBuf::new(),
            control_timeout: DEFAULT_CONTROL_TIMEOUT,
        }
    }
}

impl Default for RuntimeSupervisorStopRequest {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            agent_session_id: String::new(),
            repo_root: PathBuf::new(),
            control_timeout: DEFAULT_CONTROL_TIMEOUT,
            shutdown_timeout: DEFAULT_STOP_TIMEOUT,
        }
    }
}

impl RuntimeSupervisorController {
    pub fn remember(&self, project_id: &str, agent_session_id: &str, run_id: &str, endpoint: &str) {
        self.inner
            .lock()
            .expect("runtime supervisor registry poisoned")
            .active
            .insert(
                supervisor_registry_key(project_id, agent_session_id),
                ActiveRuntimeSupervisor {
                    project_id: project_id.into(),
                    agent_session_id: agent_session_id.into(),
                    run_id: run_id.into(),
                    endpoint: endpoint.into(),
                },
            );
    }

    pub fn forget(&self, project_id: &str, agent_session_id: &str) {
        self.inner
            .lock()
            .expect("runtime supervisor registry poisoned")
            .active
            .remove(&supervisor_registry_key(project_id, agent_session_id));
    }

    pub fn snapshot(
        &self,
        project_id: &str,
        agent_session_id: &str,
    ) -> Option<ActiveRuntimeSupervisorSnapshot> {
        self.inner
            .lock()
            .expect("runtime supervisor registry poisoned")
            .active
            .get(&supervisor_registry_key(project_id, agent_session_id))
            .cloned()
            .map(|entry| ActiveRuntimeSupervisorSnapshot {
                project_id: entry.project_id,
                agent_session_id: entry.agent_session_id,
                run_id: entry.run_id,
                endpoint: entry.endpoint,
            })
    }
}

fn supervisor_registry_key(project_id: &str, agent_session_id: &str) -> String {
    format!("{project_id}\u{1f}{agent_session_id}")
}

pub fn launch_detached_runtime_supervisor(
    state: &DesktopState,
    request: RuntimeSupervisorLaunchRequest,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    validate_launch_request(&request)?;

    if let Some(existing) = probe_runtime_run_with_timeout(
        state,
        &request.repo_root,
        &request.project_id,
        &request.agent_session_id,
        request.control_timeout,
    )? {
        if matches!(
            existing.run.status,
            RuntimeRunStatus::Starting | RuntimeRunStatus::Running
        ) && existing.run.transport.liveness == RuntimeRunTransportLiveness::Reachable
        {
            return Err(CommandError::user_fixable(
                "runtime_run_already_active",
                format!(
                    "Cadence already has a detached runtime supervisor for project `{}`. Stop or reconnect to run `{}` before starting another one.",
                    request.project_id, existing.run.run_id
                ),
            ));
        }
    }

    let supervisor_binary_resolution =
        resolve_runtime_supervisor_binary(request.supervisor_binary.as_deref())?;
    let supervisor_binary = supervisor_binary_resolution.binary_path;

    let mut sidecar = Command::new(&supervisor_binary);
    sidecar
        .arg("--project-id")
        .arg(&request.project_id)
        .arg("--agent-session-id")
        .arg(&request.agent_session_id)
        .arg("--repo-root")
        .arg(&request.repo_root)
        .arg("--runtime-kind")
        .arg(&request.runtime_kind)
        .arg("--run-id")
        .arg(&request.run_id)
        .arg("--session-id")
        .arg(&request.session_id)
        .arg("--program")
        .arg(&request.program)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    if let Some(flow_id) = request.flow_id.as_deref() {
        sidecar.arg("--flow-id").arg(flow_id);
    }

    let launch_context_json = serde_json::to_string(&request.launch_context).map_err(|error| {
        CommandError::system_fault(
            "runtime_supervisor_request_invalid",
            format!(
                "Cadence could not serialize the detached runtime supervisor launch context: {error}"
            ),
        )
    })?;
    sidecar
        .arg("--launch-context-json")
        .arg(launch_context_json);

    let control_state_json = serde_json::to_string(&request.run_controls).map_err(|error| {
        CommandError::system_fault(
            "runtime_supervisor_request_invalid",
            format!(
                "Cadence could not serialize the detached runtime supervisor control seed: {error}"
            ),
        )
    })?;
    sidecar.arg("--control-state-json").arg(control_state_json);

    apply_sidecar_launch_environment(
        &mut sidecar,
        &request.agent_session_id,
        &request.launch_context,
        &request.launch_env,
    );

    for arg in &request.args {
        sidecar.arg("--command-arg").arg(arg);
    }

    let mut child = sidecar.spawn().map_err(|error| {
        let _ = persist_failed_launch(
            &request.repo_root,
            &request.project_id,
            &request.agent_session_id,
            &request.run_id,
            &request.runtime_kind,
            request.launch_context.provider_id.as_str(),
            &request.run_controls,
            "runtime_supervisor_spawn_failed",
            "Cadence could not launch the detached PTY supervisor process.",
        );
        CommandError::retryable(
            "runtime_supervisor_spawn_failed",
            format!(
                "Cadence could not launch the detached PTY supervisor process from `{}`: {error}",
                supervisor_binary.display()
            ),
        )
    })?;

    let stdout = child.stdout.take().ok_or_else(|| {
        let _ = child.kill();
        let _ = persist_failed_launch(
            &request.repo_root,
            &request.project_id,
            &request.agent_session_id,
            &request.run_id,
            &request.runtime_kind,
            request.launch_context.provider_id.as_str(),
            &request.run_controls,
            "runtime_supervisor_stdout_missing",
            "Cadence could not read the detached PTY supervisor startup handshake.",
        );
        CommandError::system_fault(
            "runtime_supervisor_stdout_missing",
            "Cadence launched the detached PTY supervisor but could not read its startup handshake.",
        )
    })?;

    let startup_message = match read_startup_message(stdout, request.startup_timeout) {
        Ok(message) => message,
        Err(error) => {
            let _ = child.kill();
            let _ = persist_failed_launch(
                &request.repo_root,
                &request.project_id,
                &request.agent_session_id,
                &request.run_id,
                &request.runtime_kind,
                request.launch_context.provider_id.as_str(),
                &request.run_controls,
                &error.code,
                &error.message,
            );
            return Err(error);
        }
    };

    match startup_message {
        SupervisorStartupMessage::Ready {
            protocol_version,
            project_id,
            run_id,
            transport_kind,
            endpoint,
            status,
            launch_context,
            ..
        } => {
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                let _ = child.kill();
                let _ = persist_failed_launch(
                    &request.repo_root,
                    &request.project_id,
                    &request.agent_session_id,
                    &request.run_id,
                    &request.runtime_kind,
                    request.launch_context.provider_id.as_str(),
                    &request.run_controls,
                    "runtime_supervisor_protocol_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because its protocol version was unsupported.",
                );
                return Err(CommandError::retryable(
                    "runtime_supervisor_protocol_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because its protocol version was unsupported.",
                ));
            }

            if project_id != request.project_id || run_id != request.run_id {
                let _ = child.kill();
                let _ = persist_failed_launch(
                    &request.repo_root,
                    &request.project_id,
                    &request.agent_session_id,
                    &request.run_id,
                    &request.runtime_kind,
                    request.launch_context.provider_id.as_str(),
                    &request.run_controls,
                    "runtime_supervisor_handshake_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because it did not match the requested project or run id.",
                );
                return Err(CommandError::retryable(
                    "runtime_supervisor_handshake_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because it did not match the requested project or run id.",
                ));
            }

            if transport_kind != SUPERVISOR_TRANSPORT_KIND_TCP || endpoint.trim().is_empty() {
                let _ = child.kill();
                let _ = persist_failed_launch(
                    &request.repo_root,
                    &request.project_id,
                    &request.agent_session_id,
                    &request.run_id,
                    &request.runtime_kind,
                    request.launch_context.provider_id.as_str(),
                    &request.run_controls,
                    "runtime_supervisor_handshake_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because it omitted a valid control endpoint.",
                );
                return Err(CommandError::retryable(
                    "runtime_supervisor_handshake_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because it omitted a valid control endpoint.",
                ));
            }

            if launch_context != request.launch_context {
                let _ = child.kill();
                let _ = persist_failed_launch(
                    &request.repo_root,
                    &request.project_id,
                    &request.agent_session_id,
                    &request.run_id,
                    &request.runtime_kind,
                    request.launch_context.provider_id.as_str(),
                    &request.run_controls,
                    "runtime_supervisor_launch_context_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because provider/session/model launch context did not match the approved request.",
                );
                return Err(CommandError::retryable(
                    "runtime_supervisor_launch_context_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because provider/session/model launch context did not match the approved request.",
                ));
            }

            let expected_status = match status {
                SupervisorProcessStatus::Starting => RuntimeRunStatus::Starting,
                SupervisorProcessStatus::Running => RuntimeRunStatus::Running,
                SupervisorProcessStatus::Stopped => RuntimeRunStatus::Stopped,
                SupervisorProcessStatus::Failed => RuntimeRunStatus::Failed,
            };

            let snapshot = project_store::load_runtime_run(
                &request.repo_root,
                &request.project_id,
                &request.agent_session_id,
            )?
            .filter(|snapshot| snapshot.run.run_id == request.run_id)
            .map(Ok)
            .unwrap_or_else(|| {
                project_store::upsert_runtime_run(
                    &request.repo_root,
                    &RuntimeRunUpsertRecord {
                        run: RuntimeRunRecord {
                            project_id: request.project_id.clone(),
                            agent_session_id: request.agent_session_id.clone(),
                            run_id: request.run_id.clone(),
                            runtime_kind: request.runtime_kind.clone(),
                            provider_id: request.launch_context.provider_id.clone(),
                            supervisor_kind: SUPERVISOR_KIND_DETACHED_PTY.into(),
                            status: expected_status,
                            transport: RuntimeRunTransportRecord {
                                kind: SUPERVISOR_TRANSPORT_KIND_TCP.into(),
                                endpoint: endpoint.clone(),
                                liveness: RuntimeRunTransportLiveness::Reachable,
                            },
                            started_at: now_timestamp(),
                            last_heartbeat_at: Some(now_timestamp()),
                            stopped_at: None,
                            last_error: None,
                            updated_at: now_timestamp(),
                        },
                        checkpoint: None,
                        control_state: Some(request.run_controls.clone()),
                    },
                )
            })?;

            state.runtime_supervisor_controller().remember(
                &request.project_id,
                &request.agent_session_id,
                &request.run_id,
                &endpoint,
            );
            Ok(snapshot)
        }
        SupervisorStartupMessage::Error {
            code,
            message,
            retryable,
            ..
        } => {
            let _ = persist_failed_launch(
                &request.repo_root,
                &request.project_id,
                &request.agent_session_id,
                &request.run_id,
                &request.runtime_kind,
                request.launch_context.provider_id.as_str(),
                &request.run_controls,
                &code,
                &message,
            );
            if retryable {
                Err(CommandError::retryable(code, message))
            } else {
                Err(CommandError::user_fixable(code, message))
            }
        }
    }
}

pub fn probe_runtime_run(
    state: &DesktopState,
    request: RuntimeSupervisorProbeRequest,
) -> Result<Option<RuntimeRunSnapshotRecord>, CommandError> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    probe_runtime_run_with_timeout(
        state,
        &request.repo_root,
        &request.project_id,
        &request.agent_session_id,
        request.control_timeout,
    )
}

pub fn stop_runtime_run(
    state: &DesktopState,
    request: RuntimeSupervisorStopRequest,
) -> Result<Option<RuntimeRunSnapshotRecord>, CommandError> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;

    let Some(snapshot) = project_store::load_runtime_run(
        &request.repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?
    else {
        state
            .runtime_supervisor_controller()
            .forget(&request.project_id, &request.agent_session_id);
        return Ok(None);
    };

    if matches!(
        snapshot.run.status,
        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
    ) {
        state
            .runtime_supervisor_controller()
            .forget(&request.project_id, &request.agent_session_id);
        return Ok(Some(snapshot));
    }

    let response = send_control_request(
        &snapshot.run.transport.endpoint,
        request.control_timeout,
        &SupervisorControlRequest::stop(&snapshot.run.project_id, &snapshot.run.run_id),
    );

    match response {
        Ok(SupervisorControlResponse::StopAccepted {
            project_id, run_id, ..
        }) if project_id == snapshot.run.project_id && run_id == snapshot.run.run_id => {}
        Ok(_) => {
            return mark_runtime_run_after_probe_failure(
                state,
                &request.repo_root,
                snapshot,
                "supervisor_stop_invalid",
                "The detached supervisor returned a mismatched stop acknowledgement.",
            )
            .map(Some)
        }
        Err(_) => {
            return mark_runtime_run_after_probe_failure(
                state,
                &request.repo_root,
                snapshot,
                "supervisor_stop_failed",
                "The detached supervisor did not accept the stop request.",
            )
            .map(Some)
        }
    }

    let deadline = std::time::Instant::now() + request.shutdown_timeout;
    loop {
        let latest = project_store::load_runtime_run(
            &request.repo_root,
            &request.project_id,
            &request.agent_session_id,
        )?;
        if let Some(snapshot) = latest {
            if matches!(
                snapshot.run.status,
                RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
            ) {
                state
                    .runtime_supervisor_controller()
                    .forget(&request.project_id, &request.agent_session_id);
                return Ok(Some(snapshot));
            }
        }

        if std::time::Instant::now() >= deadline {
            let Some(latest) = project_store::load_runtime_run(
                &request.repo_root,
                &request.project_id,
                &request.agent_session_id,
            )?
            else {
                state
                    .runtime_supervisor_controller()
                    .forget(&request.project_id, &request.agent_session_id);
                return Ok(None);
            };
            let latest = mark_runtime_run_after_probe_failure(
                state,
                &request.repo_root,
                latest,
                "supervisor_stop_timeout",
                "The detached supervisor did not stop before the shutdown timeout elapsed.",
            )?;
            return Ok(Some(latest));
        }

        let _ = send_control_request(
            &snapshot.run.transport.endpoint,
            request.control_timeout,
            &SupervisorControlRequest::probe(&snapshot.run.project_id, &snapshot.run.run_id),
        );

        thread::sleep(Duration::from_millis(100));
    }
}

pub fn submit_runtime_run_input(
    state: &DesktopState,
    request: RuntimeSupervisorSubmitInputRequest,
) -> Result<String, CommandError> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.session_id, "sessionId")?;
    validate_non_empty(&request.action_id, "actionId")?;
    validate_non_empty(&request.boundary_id, "boundaryId")?;
    validate_non_empty(&request.input, "input")?;
    if let Some(flow_id) = request.flow_id.as_deref() {
        validate_non_empty(flow_id, "flowId")?;
    }
    if request.control_timeout.is_zero() {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_request_invalid",
            "Cadence requires a non-zero detached supervisor control timeout.",
        ));
    }

    let Some(snapshot) = project_store::load_runtime_run(
        &request.repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?
    else {
        state
            .runtime_supervisor_controller()
            .forget(&request.project_id, &request.agent_session_id);
        return Err(CommandError::retryable(
            "runtime_run_missing",
            format!(
                "Cadence cannot deliver approved terminal input for action `{}` because project `{}` has no durable runtime run.",
                request.action_id, request.project_id
            ),
        ));
    };

    if snapshot.run.run_id != request.run_id {
        return Err(CommandError::retryable(
            "runtime_run_mismatch",
            format!(
                "Cadence refused to deliver approved terminal input for action `{}` because project `{}` is currently bound to durable run `{}` instead of `{}`.",
                request.action_id, request.project_id, snapshot.run.run_id, request.run_id
            ),
        ));
    }

    if let Some(active) = state
        .runtime_supervisor_controller()
        .snapshot(&request.project_id, &request.agent_session_id)
        .filter(|active| active.run_id != request.run_id)
    {
        return Err(CommandError::retryable(
            "runtime_run_mismatch",
            format!(
                "Cadence refused to deliver approved terminal input for action `{}` because project `{}` is currently attached to run `{}` instead of `{}`.",
                request.action_id, request.project_id, active.run_id, request.run_id
            ),
        ));
    }

    if matches!(
        snapshot.run.status,
        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
    ) {
        return Err(CommandError::retryable(
            "runtime_run_unavailable",
            format!(
                "Cadence cannot deliver approved terminal input for action `{}` because run `{}` is already terminal ({}). Refresh runtime state before retrying.",
                request.action_id,
                request.run_id,
                runtime_run_status_label(&snapshot.run.status)
            ),
        ));
    }

    let response = send_control_request(
        &snapshot.run.transport.endpoint,
        request.control_timeout,
        &SupervisorControlRequest::submit_input(
            &request.project_id,
            &request.run_id,
            &request.session_id,
            request.flow_id.clone(),
            &request.action_id,
            &request.boundary_id,
            &request.input,
        ),
    );

    let persist_control_error = |code: &str, message: &str| -> Result<(), CommandError> {
        refresh_runtime_run_after_control_response(
            &request.repo_root,
            &snapshot,
            Some(RuntimeRunDiagnosticRecord {
                code: code.to_string(),
                message: message.to_string(),
            }),
        )?;
        Ok(())
    };

    match response {
        Ok(SupervisorControlResponse::SubmitInputAccepted {
            protocol_version,
            project_id,
            run_id,
            action_id,
            boundary_id,
            delivered_at,
        }) => {
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                let code = "runtime_supervisor_protocol_invalid";
                let message =
                    "Cadence rejected a detached supervisor submit-input acknowledgement with an unexpected protocol version.";
                persist_control_error(code, message)?;
                return Err(CommandError::user_fixable(code, message));
            }

            if project_id != snapshot.run.project_id
                || run_id != snapshot.run.run_id
                || action_id != request.action_id
                || boundary_id != request.boundary_id
            {
                let code = "runtime_supervisor_submit_ack_mismatch";
                let message =
                    "Cadence rejected detached supervisor submit-input acknowledgement because its runtime identity did not match the approved action boundary.";
                persist_control_error(code, message)?;
                return Err(CommandError::retryable(code, message));
            }

            let refreshed =
                refresh_runtime_run_after_control_response(&request.repo_root, &snapshot, None)?;
            state.runtime_supervisor_controller().remember(
                &refreshed.run.project_id,
                &refreshed.run.agent_session_id,
                &refreshed.run.run_id,
                &refreshed.run.transport.endpoint,
            );
            Ok(delivered_at)
        }
        Ok(SupervisorControlResponse::Error {
            protocol_version,
            code,
            message,
            retryable,
        }) => {
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                let code = "runtime_supervisor_protocol_invalid";
                let message =
                    "Cadence rejected a detached supervisor control error frame with an unexpected protocol version.";
                persist_control_error(code, message)?;
                return Err(CommandError::user_fixable(code, message));
            }

            persist_control_error(&code, &message)?;
            Err(if retryable {
                CommandError::retryable(code, message)
            } else {
                CommandError::user_fixable(code, message)
            })
        }
        Ok(_) => {
            let code = "runtime_supervisor_control_invalid";
            let message =
                "Cadence rejected an unsupported detached supervisor submit-input acknowledgement.";
            persist_control_error(code, message)?;
            Err(CommandError::retryable(code, message))
        }
        Err(error) => {
            if error.code == "runtime_supervisor_control_invalid" {
                persist_control_error(&error.code, &error.message)?;
                return Err(error);
            }

            upsert_runtime_run_projection(
                &request.repo_root,
                &snapshot,
                RuntimeRunStatus::Stale,
                RuntimeRunTransportLiveness::Unreachable,
                Some(RuntimeRunDiagnosticRecord {
                    code: error.code.clone(),
                    message: error.message.clone(),
                }),
                snapshot.run.last_heartbeat_at.clone(),
                snapshot.run.stopped_at.clone(),
            )?;
            state
                .runtime_supervisor_controller()
                .forget(&request.project_id, &request.agent_session_id);
            Err(error)
        }
    }
}

pub fn update_runtime_run_controls(
    state: &DesktopState,
    request: RuntimeSupervisorUpdateControlsRequest,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    validate_non_empty(&request.run_id, "runId")?;
    if request.controls.is_none() && request.prompt.is_none() {
        return Err(CommandError::user_fixable(
            "runtime_run_control_invalid",
            "Cadence requires a prompt or control delta before it can queue runtime-run changes.",
        ));
    }
    if request.control_timeout.is_zero() {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_request_invalid",
            "Cadence requires a non-zero detached supervisor control timeout.",
        ));
    }

    let Some(snapshot) = project_store::load_runtime_run(
        &request.repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?
    else {
        state
            .runtime_supervisor_controller()
            .forget(&request.project_id, &request.agent_session_id);
        return Err(CommandError::retryable(
            "runtime_run_missing",
            format!(
                "Cadence cannot queue runtime-run controls because project `{}` has no durable runtime run.",
                request.project_id
            ),
        ));
    };

    if snapshot.run.run_id != request.run_id {
        return Err(CommandError::retryable(
            "runtime_run_mismatch",
            format!(
                "Cadence refused to queue controls for run `{}` because project `{}` is currently bound to durable run `{}`.",
                request.run_id, request.project_id, snapshot.run.run_id
            ),
        ));
    }

    if let Some(active) = state
        .runtime_supervisor_controller()
        .snapshot(&request.project_id, &request.agent_session_id)
        .filter(|active| active.run_id != request.run_id)
    {
        return Err(CommandError::retryable(
            "runtime_run_mismatch",
            format!(
                "Cadence refused to queue controls for run `{}` because project `{}` is currently attached to run `{}` instead.",
                request.run_id, request.project_id, active.run_id
            ),
        ));
    }

    if matches!(
        snapshot.run.status,
        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
    ) {
        return Err(CommandError::retryable(
            "runtime_run_unavailable",
            format!(
                "Cadence cannot queue controls because run `{}` is already terminal ({}). Refresh runtime state before retrying.",
                request.run_id,
                runtime_run_status_label(&snapshot.run.status)
            ),
        ));
    }

    let runtime_session = project_store::load_runtime_session(&request.repo_root, &request.project_id)?
        .ok_or_else(|| {
            CommandError::retryable(
                "runtime_run_session_missing",
                format!(
                    "Cadence cannot queue runtime-run controls for project `{}` because the durable runtime session is missing.",
                    request.project_id
                ),
            )
        })?;
    let session_id = runtime_session.session_id.clone().ok_or_else(|| {
        CommandError::retryable(
            "runtime_run_session_missing",
            format!(
                "Cadence cannot queue runtime-run controls for project `{}` until the durable runtime session exposes a stable session id.",
                request.project_id
            ),
        )
    })?;

    let response = send_control_request(
        &snapshot.run.transport.endpoint,
        request.control_timeout,
        &SupervisorControlRequest::queue_controls(
            &request.project_id,
            &request.run_id,
            &session_id,
            runtime_session.flow_id.clone(),
            request.controls.clone(),
            request.prompt.clone(),
        ),
    );

    let persist_control_error = |code: &str, message: &str| -> Result<(), CommandError> {
        refresh_runtime_run_after_control_response(
            &request.repo_root,
            &snapshot,
            Some(RuntimeRunDiagnosticRecord {
                code: code.to_string(),
                message: message.to_string(),
            }),
        )?;
        Ok(())
    };

    match response {
        Ok(SupervisorControlResponse::QueueControlsAccepted {
            protocol_version,
            project_id,
            run_id,
            session_id: ack_session_id,
            flow_id: ack_flow_id,
            ..
        }) => {
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                let code = "runtime_supervisor_protocol_invalid";
                let message =
                    "Cadence rejected a detached supervisor queued-controls acknowledgement with an unexpected protocol version.";
                persist_control_error(code, message)?;
                return Err(CommandError::user_fixable(code, message));
            }

            if project_id != snapshot.run.project_id
                || run_id != snapshot.run.run_id
                || ack_session_id != session_id
                || ack_flow_id != runtime_session.flow_id
            {
                let code = "runtime_supervisor_control_ack_mismatch";
                let message =
                    "Cadence rejected detached supervisor queued-controls acknowledgement because its runtime identity did not match the active run session.";
                persist_control_error(code, message)?;
                return Err(CommandError::retryable(code, message));
            }

            let refreshed =
                refresh_runtime_run_after_control_response(&request.repo_root, &snapshot, None)?;
            state.runtime_supervisor_controller().remember(
                &refreshed.run.project_id,
                &refreshed.run.agent_session_id,
                &refreshed.run.run_id,
                &refreshed.run.transport.endpoint,
            );
            Ok(refreshed)
        }
        Ok(SupervisorControlResponse::Error {
            protocol_version,
            code,
            message,
            retryable,
        }) => {
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                let code = "runtime_supervisor_protocol_invalid";
                let message =
                    "Cadence rejected a detached supervisor control error frame with an unexpected protocol version.";
                persist_control_error(code, message)?;
                return Err(CommandError::user_fixable(code, message));
            }

            persist_control_error(&code, &message)?;
            Err(if retryable {
                CommandError::retryable(code, message)
            } else {
                CommandError::user_fixable(code, message)
            })
        }
        Ok(_) => {
            let code = "runtime_supervisor_control_invalid";
            let message =
                "Cadence rejected an unsupported detached supervisor queued-controls acknowledgement.";
            persist_control_error(code, message)?;
            Err(CommandError::retryable(code, message))
        }
        Err(error) => {
            if error.code == "runtime_supervisor_control_invalid" {
                persist_control_error(&error.code, &error.message)?;
                return Err(error);
            }

            upsert_runtime_run_projection(
                &request.repo_root,
                &snapshot,
                RuntimeRunStatus::Stale,
                RuntimeRunTransportLiveness::Unreachable,
                Some(RuntimeRunDiagnosticRecord {
                    code: error.code.clone(),
                    message: error.message.clone(),
                }),
                snapshot.run.last_heartbeat_at.clone(),
                snapshot.run.stopped_at.clone(),
            )?;
            state
                .runtime_supervisor_controller()
                .forget(&request.project_id, &request.agent_session_id);
            Err(error)
        }
    }
}

fn apply_sidecar_launch_environment(
    sidecar: &mut Command,
    agent_session_id: &str,
    launch_context: &RuntimeSupervisorLaunchContext,
    launch_env: &RuntimeSupervisorLaunchEnv,
) {
    sidecar
        .env_remove(CADENCE_AGENT_SESSION_ID_ENV)
        .env_remove(CADENCE_RUNTIME_PROVIDER_ID_ENV)
        .env_remove(CADENCE_RUNTIME_SESSION_ID_ENV)
        .env_remove(CADENCE_RUNTIME_FLOW_ID_ENV)
        .env_remove(CADENCE_RUNTIME_MODEL_ID_ENV)
        .env_remove(CADENCE_RUNTIME_THINKING_EFFORT_ENV)
        .env_remove(CADENCE_GLOBAL_DB_PATH_ENV)
        .env_remove(CADENCE_RUNTIME_MCP_CONFIG_PATH_ENV)
        .env_remove(CADENCE_RUNTIME_MCP_CONTRACT_REQUIRED_ENV)
        .env_remove(ANTHROPIC_API_KEY_ENV)
        .env_remove(OPENAI_API_KEY_ENV)
        .env_remove(OPENAI_BASE_URL_ENV)
        .env_remove(OPENAI_API_VERSION_ENV);

    sidecar.env(CADENCE_AGENT_SESSION_ID_ENV, agent_session_id);
    sidecar.env(CADENCE_RUNTIME_PROVIDER_ID_ENV, &launch_context.provider_id);
    sidecar.env(CADENCE_RUNTIME_SESSION_ID_ENV, &launch_context.session_id);
    sidecar.env(CADENCE_RUNTIME_MODEL_ID_ENV, &launch_context.model_id);

    if let Some(flow_id) = launch_context.flow_id.as_deref() {
        sidecar.env(CADENCE_RUNTIME_FLOW_ID_ENV, flow_id);
    }

    if let Some(thinking_effort) = launch_context.thinking_effort.as_ref() {
        sidecar.env(
            CADENCE_RUNTIME_THINKING_EFFORT_ENV,
            runtime_supervisor_thinking_effort_env_value(thinking_effort),
        );
    }

    if let Ok(global_db_path) = std::env::var(CADENCE_GLOBAL_DB_PATH_ENV) {
        if !global_db_path.trim().is_empty() {
            sidecar.env(CADENCE_GLOBAL_DB_PATH_ENV, global_db_path);
        }
    }

    for (key, value) in launch_env.iter() {
        sidecar.env(key, value);
    }
}

fn validate_launch_request(request: &RuntimeSupervisorLaunchRequest) -> Result<(), CommandError> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    validate_non_empty(&request.runtime_kind, "runtimeKind")?;
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.session_id, "sessionId")?;
    if let Some(flow_id) = request.flow_id.as_deref() {
        validate_non_empty(flow_id, "flowId")?;
    }
    validate_non_empty(&request.program, "program")?;

    if request.startup_timeout.is_zero() {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_request_invalid",
            "Cadence requires a non-zero detached supervisor startup timeout.",
        ));
    }

    if request.control_timeout.is_zero() {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_request_invalid",
            "Cadence requires a non-zero detached supervisor control timeout.",
        ));
    }

    validate_runtime_supervisor_launch_context(
        &request.runtime_kind,
        &request.session_id,
        request.flow_id.as_deref(),
        &request.run_controls,
        &request.launch_context,
    )?;

    Ok(())
}

fn probe_runtime_run_with_timeout(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    control_timeout: Duration,
) -> Result<Option<RuntimeRunSnapshotRecord>, CommandError> {
    let Some(snapshot) = project_store::load_runtime_run(repo_root, project_id, agent_session_id)?
    else {
        state
            .runtime_supervisor_controller()
            .forget(project_id, agent_session_id);
        return Ok(None);
    };

    if matches!(
        snapshot.run.status,
        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
    ) {
        state
            .runtime_supervisor_controller()
            .forget(project_id, agent_session_id);
        return Ok(Some(snapshot));
    }

    match send_control_request(
        &snapshot.run.transport.endpoint,
        control_timeout,
        &SupervisorControlRequest::probe(&snapshot.run.project_id, &snapshot.run.run_id),
    ) {
        Ok(SupervisorControlResponse::ProbeResult {
            protocol_version,
            project_id,
            run_id,
            status,
            last_heartbeat_at,
            last_error,
            ..
        }) => {
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION
                || project_id != snapshot.run.project_id
                || run_id != snapshot.run.run_id
            {
                let updated = mark_runtime_run_after_probe_failure(
                    state,
                    repo_root,
                    snapshot,
                    "supervisor_probe_invalid",
                    "The detached supervisor returned a mismatched probe response.",
                )?;
                return Ok(Some(updated));
            }

            let Some(latest) =
                project_store::load_runtime_run(repo_root, project_id.as_str(), agent_session_id)?
            else {
                let updated = mark_runtime_run_after_probe_failure(
                    state,
                    repo_root,
                    snapshot,
                    "supervisor_probe_missing",
                    "Cadence could not reload the durable runtime-run row after a successful probe.",
                )?;
                return Ok(Some(updated));
            };

            let mapped_status = match status {
                SupervisorProcessStatus::Starting => RuntimeRunStatus::Starting,
                SupervisorProcessStatus::Running => RuntimeRunStatus::Running,
                SupervisorProcessStatus::Stopped => RuntimeRunStatus::Stopped,
                SupervisorProcessStatus::Failed => RuntimeRunStatus::Failed,
            };
            let response_error = last_error.map(protocol_diagnostic_into_record);
            let persisted_error = response_error
                .clone()
                .or_else(|| latest.run.last_error.clone());

            if latest.run.transport.liveness == RuntimeRunTransportLiveness::Reachable
                && latest.run.status == mapped_status
                && latest.run.last_error == persisted_error
            {
                if matches!(
                    latest.run.status,
                    RuntimeRunStatus::Starting | RuntimeRunStatus::Running
                ) {
                    state.runtime_supervisor_controller().remember(
                        &latest.run.project_id,
                        &latest.run.agent_session_id,
                        &latest.run.run_id,
                        &latest.run.transport.endpoint,
                    );
                } else {
                    state
                        .runtime_supervisor_controller()
                        .forget(&latest.run.project_id, &latest.run.agent_session_id);
                }
                return Ok(Some(latest));
            }

            let updated = upsert_runtime_run_projection(
                repo_root,
                &latest,
                mapped_status,
                RuntimeRunTransportLiveness::Reachable,
                persisted_error,
                last_heartbeat_at,
                latest.run.stopped_at.clone(),
            )?;

            if matches!(
                updated.run.status,
                RuntimeRunStatus::Starting | RuntimeRunStatus::Running
            ) {
                state.runtime_supervisor_controller().remember(
                    &updated.run.project_id,
                    &updated.run.agent_session_id,
                    &updated.run.run_id,
                    &updated.run.transport.endpoint,
                );
            } else {
                state
                    .runtime_supervisor_controller()
                    .forget(&updated.run.project_id, &updated.run.agent_session_id);
            }

            Ok(Some(updated))
        }
        Ok(SupervisorControlResponse::Error { code, message, .. }) => {
            if let Some(latest) =
                project_store::load_runtime_run(repo_root, project_id, agent_session_id)?
            {
                if latest.run.run_id == snapshot.run.run_id
                    && matches!(
                        latest.run.status,
                        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
                    )
                {
                    state
                        .runtime_supervisor_controller()
                        .forget(&latest.run.project_id, &latest.run.agent_session_id);
                    return Ok(Some(latest));
                }
            }

            let updated =
                mark_runtime_run_after_probe_failure(state, repo_root, snapshot, &code, &message)?;
            Ok(Some(updated))
        }
        Ok(_) => {
            if let Some(latest) =
                project_store::load_runtime_run(repo_root, project_id, agent_session_id)?
            {
                if latest.run.run_id == snapshot.run.run_id
                    && matches!(
                        latest.run.status,
                        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
                    )
                {
                    state
                        .runtime_supervisor_controller()
                        .forget(&latest.run.project_id, &latest.run.agent_session_id);
                    return Ok(Some(latest));
                }
            }

            let updated = mark_runtime_run_after_probe_failure(
                state,
                repo_root,
                snapshot,
                "supervisor_probe_invalid",
                "Cadence rejected the detached supervisor probe response because it did not match the probe contract.",
            )?;
            Ok(Some(updated))
        }
        Err(error) => {
            if let Some(latest) =
                project_store::load_runtime_run(repo_root, project_id, agent_session_id)?
            {
                if latest.run.run_id == snapshot.run.run_id
                    && matches!(
                        latest.run.status,
                        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
                    )
                {
                    state
                        .runtime_supervisor_controller()
                        .forget(&latest.run.project_id, &latest.run.agent_session_id);
                    return Ok(Some(latest));
                }
            }

            let updated = mark_runtime_run_after_probe_failure(
                state,
                repo_root,
                snapshot,
                &error.code,
                &error.message,
            )?;
            Ok(Some(updated))
        }
    }
}

fn mark_runtime_run_after_probe_failure(
    state: &DesktopState,
    repo_root: &Path,
    snapshot: RuntimeRunSnapshotRecord,
    code: &str,
    message: &str,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    let updated = upsert_runtime_run_projection(
        repo_root,
        &snapshot,
        RuntimeRunStatus::Stale,
        RuntimeRunTransportLiveness::Unreachable,
        Some(RuntimeRunDiagnosticRecord {
            code: code.into(),
            message: message.into(),
        }),
        snapshot.run.last_heartbeat_at.clone(),
        snapshot.run.stopped_at.clone(),
    )?;
    state
        .runtime_supervisor_controller()
        .forget(&updated.run.project_id, &updated.run.agent_session_id);
    Ok(updated)
}

fn refresh_runtime_run_after_control_response(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    last_error: Option<RuntimeRunDiagnosticRecord>,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    let latest = project_store::load_runtime_run(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.agent_session_id,
    )?
    .filter(|latest| latest.run.run_id == snapshot.run.run_id)
    .unwrap_or_else(|| snapshot.clone());

    upsert_runtime_run_projection(
        repo_root,
        &latest,
        RuntimeRunStatus::Running,
        RuntimeRunTransportLiveness::Reachable,
        last_error,
        latest.run.last_heartbeat_at.clone(),
        None,
    )
}

fn upsert_runtime_run_projection(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    status: RuntimeRunStatus,
    liveness: RuntimeRunTransportLiveness,
    last_error: Option<RuntimeRunDiagnosticRecord>,
    last_heartbeat_at: Option<String>,
    stopped_at: Option<String>,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    if snapshot.run.status == status
        && snapshot.run.transport.liveness == liveness
        && snapshot.run.last_error.as_ref() == last_error.as_ref()
        && snapshot.run.last_heartbeat_at.as_deref() == last_heartbeat_at.as_deref()
        && snapshot.run.stopped_at.as_deref() == stopped_at.as_deref()
    {
        return Ok(snapshot.clone());
    }

    project_store::upsert_runtime_run(
        repo_root,
        &RuntimeRunUpsertRecord {
            run: RuntimeRunRecord {
                project_id: snapshot.run.project_id.clone(),
                agent_session_id: snapshot.run.agent_session_id.clone(),
                run_id: snapshot.run.run_id.clone(),
                runtime_kind: snapshot.run.runtime_kind.clone(),
                provider_id: snapshot.run.provider_id.clone(),
                supervisor_kind: snapshot.run.supervisor_kind.clone(),
                status,
                transport: RuntimeRunTransportRecord {
                    kind: snapshot.run.transport.kind.clone(),
                    endpoint: snapshot.run.transport.endpoint.clone(),
                    liveness,
                },
                started_at: snapshot.run.started_at.clone(),
                last_heartbeat_at,
                stopped_at,
                last_error,
                updated_at: now_timestamp(),
            },
            checkpoint: None,
            control_state: Some(snapshot.controls.clone()),
        },
    )
}

fn runtime_run_status_label(status: &RuntimeRunStatus) -> &'static str {
    match status {
        RuntimeRunStatus::Starting => "starting",
        RuntimeRunStatus::Running => "running",
        RuntimeRunStatus::Stale => "stale",
        RuntimeRunStatus::Stopped => "stopped",
        RuntimeRunStatus::Failed => "failed",
    }
}

#[allow(clippy::too_many_arguments)]
fn persist_failed_launch(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: &str,
    runtime_kind: &str,
    provider_id: &str,
    run_controls: &RuntimeRunControlStateRecord,
    code: &str,
    message: &str,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    project_store::upsert_runtime_run(
        repo_root,
        &RuntimeRunUpsertRecord {
            run: RuntimeRunRecord {
                project_id: project_id.into(),
                agent_session_id: agent_session_id.into(),
                run_id: run_id.into(),
                runtime_kind: runtime_kind.into(),
                provider_id: provider_id.into(),
                supervisor_kind: SUPERVISOR_KIND_DETACHED_PTY.into(),
                status: RuntimeRunStatus::Failed,
                transport: RuntimeRunTransportRecord {
                    kind: SUPERVISOR_TRANSPORT_KIND_TCP.into(),
                    endpoint: "launch-pending".into(),
                    liveness: RuntimeRunTransportLiveness::Unknown,
                },
                started_at: now_timestamp(),
                last_heartbeat_at: None,
                stopped_at: Some(now_timestamp()),
                last_error: Some(RuntimeRunDiagnosticRecord {
                    code: code.into(),
                    message: message.into(),
                }),
                updated_at: now_timestamp(),
            },
            checkpoint: None,
            control_state: Some(run_controls.clone()),
        },
    )
}

fn read_startup_message(
    stdout: impl Read + Send + 'static,
    timeout: Duration,
) -> Result<SupervisorStartupMessage, CommandError> {
    let (sender, receiver) = sync_channel(1);
    thread::spawn(move || {
        let result = read_json_line_from_reader::<_, SupervisorStartupMessage>(stdout).map_err(
            |error| {
                CommandError::retryable(
                    "runtime_supervisor_handshake_invalid",
                    format!(
                        "Cadence could not decode the detached PTY supervisor startup handshake: {error}"
                    ),
                )
            },
        );
        let _ = sender.send(result);
    });

    match receiver.recv_timeout(timeout) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(CommandError::retryable(
            "runtime_supervisor_start_timeout",
            "Cadence timed out while waiting for the detached PTY supervisor startup handshake.",
        )),
        Err(RecvTimeoutError::Disconnected) => Err(CommandError::retryable(
            "runtime_supervisor_handshake_invalid",
            "Cadence lost the detached PTY supervisor startup handshake before it completed.",
        )),
    }
}

fn send_control_request(
    endpoint: &str,
    timeout: Duration,
    request: &SupervisorControlRequest,
) -> Result<SupervisorControlResponse, CommandError> {
    let address = endpoint.parse::<SocketAddr>().map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_endpoint_invalid",
            "Cadence could not parse the detached supervisor control endpoint.",
        )
    })?;

    let mut stream = TcpStream::connect_timeout(&address, timeout).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_connect_failed",
            "Cadence could not connect to the detached supervisor control endpoint.",
        )
    })?;
    stream.set_read_timeout(Some(timeout)).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_timeout_config_failed",
            "Cadence could not configure the detached supervisor control read timeout.",
        )
    })?;
    stream.set_write_timeout(Some(timeout)).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_timeout_config_failed",
            "Cadence could not configure the detached supervisor control write timeout.",
        )
    })?;

    write_json_line(&mut stream, request).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_write_failed",
            "Cadence could not write the detached supervisor control request.",
        )
    })?;

    read_json_line_from_reader::<_, SupervisorControlResponse>(stream).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_control_invalid",
            "Cadence could not decode the detached supervisor control response.",
        )
    })
}
