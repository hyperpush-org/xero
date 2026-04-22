use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, RecvTimeoutError},
        Arc, Mutex,
    },
    thread,
};

use portable_pty::ChildKiller;

use crate::{
    auth::now_timestamp,
    commands::{CommandError, RuntimeRunControlInputDto},
    db::project_store::RuntimeRunPendingControlSnapshotRecord,
};

use super::live_events::append_live_event;
use super::persistence::{persist_runtime_row_from_shared, persist_sidecar_checkpoint};
use super::{
    read_json_line_from_reader, write_json_line, BufferedSupervisorEvent, ReplayRegistration,
    SharedPtyWriter, SidecarSharedState, SupervisorEventHub, ACTIVITY_OUTPUT_PREFIX,
    CONTROL_ACCEPT_POLL_INTERVAL, DEFAULT_CONTROL_TIMEOUT, LIVE_EVENT_SUBSCRIBER_BUFFER,
    MAX_CONTROL_INPUT_CHARS,
};
use crate::runtime::protocol::{
    SupervisorControlRequest, SupervisorControlResponse, SupervisorLiveEventPayload,
    SupervisorProcessStatus, SUPERVISOR_PROTOCOL_VERSION,
};

#[derive(Clone)]
pub(super) struct ControlListenerContext {
    pub(super) repo_root: PathBuf,
    pub(super) shared: Arc<Mutex<SidecarSharedState>>,
    pub(super) event_hub: Arc<Mutex<SupervisorEventHub>>,
    pub(super) persistence_lock: Arc<Mutex<()>>,
    pub(super) writer: SharedPtyWriter,
    pub(super) shutdown: Arc<AtomicBool>,
    pub(super) killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
}

struct QueueControlsRequestArgs {
    protocol_version: u8,
    project_id: String,
    run_id: String,
    session_id: String,
    flow_id: Option<String>,
    controls: Option<RuntimeRunControlInputDto>,
    prompt: Option<String>,
}

pub(super) fn spawn_control_listener(
    listener: TcpListener,
    context: ControlListenerContext,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !context.shutdown.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let connection_context = context.clone();
                    thread::spawn(move || {
                        let _ = handle_control_connection(stream, &connection_context);
                    });
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::Interrupted
                            | std::io::ErrorKind::ConnectionAborted
                    ) =>
                {
                    thread::sleep(CONTROL_ACCEPT_POLL_INTERVAL);
                }
                Err(_) => {
                    if context.shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(CONTROL_ACCEPT_POLL_INTERVAL);
                }
            }
        }
    })
}

fn handle_control_connection(
    mut stream: TcpStream,
    context: &ControlListenerContext,
) -> Result<(), CommandError> {
    let shared = &context.shared;
    let event_hub = &context.event_hub;
    let writer = &context.writer;
    let shutdown = &context.shutdown;
    let killer = &context.killer;

    stream.set_nonblocking(false).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_control_io_failed",
            "Cadence could not configure blocking detached supervisor control IO.",
        )
    })?;
    stream
        .set_read_timeout(Some(DEFAULT_CONTROL_TIMEOUT))
        .map_err(|_| {
            CommandError::retryable(
                "runtime_supervisor_control_io_failed",
                "Cadence could not configure the detached supervisor control read timeout.",
            )
        })?;
    stream
        .set_write_timeout(Some(DEFAULT_CONTROL_TIMEOUT))
        .map_err(|_| {
            CommandError::retryable(
                "runtime_supervisor_control_io_failed",
                "Cadence could not configure the detached supervisor control write timeout.",
            )
        })?;

    let request = read_json_line_from_reader::<_, SupervisorControlRequest>(
        stream.try_clone().map_err(|_| {
            CommandError::retryable(
                "runtime_supervisor_control_io_failed",
                "Cadence could not clone the detached supervisor control stream.",
            )
        })?,
    );

    match request {
        Ok(SupervisorControlRequest::Probe {
            protocol_version,
            project_id,
            run_id,
        }) => {
            let snapshot = shared.lock().expect("sidecar state lock poisoned").clone();
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                write_protocol_error(
                    &mut stream,
                    "runtime_supervisor_protocol_invalid",
                    "Detached supervisor protocol version mismatch.",
                    false,
                )?;
                return Ok(());
            }

            if project_id != snapshot.project_id || run_id != snapshot.run_id {
                write_protocol_error(
                    &mut stream,
                    "runtime_supervisor_identity_mismatch",
                    "Detached supervisor identity mismatch.",
                    false,
                )?;
                return Ok(());
            }

            write_json_line(
                &mut stream,
                &SupervisorControlResponse::ProbeResult {
                    protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                    project_id: snapshot.project_id,
                    run_id: snapshot.run_id,
                    status: snapshot.status,
                    last_heartbeat_at: snapshot.last_heartbeat_at,
                    last_checkpoint_sequence: snapshot.last_checkpoint_sequence,
                    last_checkpoint_at: snapshot.last_checkpoint_at,
                    last_error: snapshot.last_error,
                    child_pid: snapshot.child_pid,
                },
            )
            .map_err(|_| {
                CommandError::retryable(
                    "runtime_supervisor_control_io_failed",
                    "Cadence could not write the detached supervisor probe response.",
                )
            })
        }
        Ok(SupervisorControlRequest::Stop {
            protocol_version,
            project_id,
            run_id,
        }) => {
            let snapshot = shared.lock().expect("sidecar state lock poisoned").clone();
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                write_protocol_error(
                    &mut stream,
                    "runtime_supervisor_protocol_invalid",
                    "Detached supervisor protocol version mismatch.",
                    false,
                )?;
                return Ok(());
            }

            if project_id != snapshot.project_id || run_id != snapshot.run_id {
                write_protocol_error(
                    &mut stream,
                    "runtime_supervisor_identity_mismatch",
                    "Detached supervisor identity mismatch.",
                    false,
                )?;
                return Ok(());
            }

            {
                let mut snapshot = shared.lock().expect("sidecar state lock poisoned");
                snapshot.stop_requested = true;
            }
            killer
                .lock()
                .expect("detached supervisor killer lock poisoned")
                .kill()
                .map_err(|_| {
                    CommandError::retryable(
                        "runtime_supervisor_stop_failed",
                        "Cadence could not signal the detached PTY child to stop.",
                    )
                })?;
            write_json_line(
                &mut stream,
                &SupervisorControlResponse::StopAccepted {
                    protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                    project_id: snapshot.project_id,
                    run_id: snapshot.run_id,
                    child_pid: snapshot.child_pid,
                },
            )
            .map_err(|_| {
                CommandError::retryable(
                    "runtime_supervisor_control_io_failed",
                    "Cadence could not write the detached supervisor stop acknowledgement.",
                )
            })
        }
        Ok(SupervisorControlRequest::Attach {
            protocol_version,
            project_id,
            run_id,
            after_sequence,
        }) => handle_attach_request(
            &mut stream,
            shared,
            event_hub,
            shutdown,
            protocol_version,
            project_id,
            run_id,
            after_sequence,
        ),
        Ok(SupervisorControlRequest::SubmitInput {
            protocol_version,
            project_id,
            run_id,
            session_id,
            flow_id,
            action_id,
            boundary_id,
            input,
        }) => handle_submit_input_request(
            &mut stream,
            shared,
            event_hub,
            writer,
            protocol_version,
            project_id,
            run_id,
            session_id,
            flow_id,
            action_id,
            boundary_id,
            input,
        ),
        Ok(SupervisorControlRequest::QueueControls {
            protocol_version,
            project_id,
            run_id,
            session_id,
            flow_id,
            controls,
            prompt,
        }) => handle_queue_controls_request(
            &mut stream,
            context,
            QueueControlsRequestArgs {
                protocol_version,
                project_id,
                run_id,
                session_id,
                flow_id,
                controls,
                prompt,
            },
        ),
        Err(error) => write_protocol_error(
            &mut stream,
            "runtime_supervisor_request_invalid",
            &format!("Cadence rejected a malformed detached supervisor control request: {error}."),
            false,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_attach_request(
    stream: &mut TcpStream,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    shutdown: &Arc<AtomicBool>,
    protocol_version: u8,
    project_id: String,
    run_id: String,
    after_sequence: Option<u64>,
) -> Result<(), CommandError> {
    if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
        write_protocol_error(
            stream,
            "runtime_supervisor_protocol_invalid",
            "Detached supervisor protocol version mismatch.",
            false,
        )?;
        return Ok(());
    }

    let snapshot = shared.lock().expect("sidecar state lock poisoned").clone();
    if project_id != snapshot.project_id || run_id != snapshot.run_id {
        write_protocol_error(
            stream,
            "runtime_supervisor_identity_mismatch",
            "Detached supervisor identity mismatch.",
            false,
        )?;
        return Ok(());
    }

    if matches!(after_sequence, Some(0)) {
        write_protocol_error(
            stream,
            "runtime_supervisor_attach_cursor_invalid",
            "Detached supervisor attach cursors must be greater than zero when provided.",
            false,
        )?;
        return Ok(());
    }

    let terminal_snapshot = matches!(
        snapshot.status,
        SupervisorProcessStatus::Stopped | SupervisorProcessStatus::Failed
    );

    if terminal_snapshot {
        write_protocol_error(
            stream,
            "runtime_supervisor_attach_unavailable",
            "Cadence cannot attach to a detached supervisor after the run reached terminal state.",
            false,
        )?;
        return Ok(());
    }

    let (registration, receiver) = register_attach_replay(event_hub, &snapshot, after_sequence);
    write_json_line(stream, &registration.attach_response).map_err(|_| {
        remove_event_subscriber(event_hub, registration.subscriber_id);
        CommandError::retryable(
            "runtime_supervisor_control_io_failed",
            "Cadence could not write the detached supervisor attach acknowledgement.",
        )
    })?;

    for event in &registration.replay_events {
        let response = live_event_response(event, true);
        if write_json_line(stream, &response).is_err() {
            remove_event_subscriber(event_hub, registration.subscriber_id);
            return Ok(());
        }
    }

    while !shutdown.load(Ordering::SeqCst) {
        match receiver.recv_timeout(CONTROL_ACCEPT_POLL_INTERVAL) {
            Ok(event) => {
                let response = live_event_response(&event, false);
                if write_json_line(stream, &response).is_err() {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    remove_event_subscriber(event_hub, registration.subscriber_id);
    Ok(())
}

fn register_attach_replay(
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    snapshot: &SidecarSharedState,
    after_sequence: Option<u64>,
) -> (
    ReplayRegistration,
    std::sync::mpsc::Receiver<BufferedSupervisorEvent>,
) {
    let (sender, receiver) = sync_channel(LIVE_EVENT_SUBSCRIBER_BUFFER);
    let mut hub = event_hub.lock().expect("event hub lock poisoned");
    hub.next_subscriber_id = hub.next_subscriber_id.saturating_add(1);
    let subscriber_id = hub.next_subscriber_id;
    hub.subscribers.insert(subscriber_id, sender);

    let oldest_available_sequence = hub.ring.front().map(|event| event.sequence);
    let latest_sequence = hub.ring.back().map(|event| event.sequence);
    let replay_events = hub
        .ring
        .iter()
        .filter(|event| after_sequence.is_none_or(|cursor| event.sequence > cursor))
        .cloned()
        .collect::<Vec<_>>();
    let replay_truncated = after_sequence.map_or(
        oldest_available_sequence.is_some_and(|oldest| oldest > 1),
        |cursor| oldest_available_sequence.is_some_and(|oldest| cursor.saturating_add(1) < oldest),
    );

    (
        ReplayRegistration {
            subscriber_id,
            attach_response: SupervisorControlResponse::Attached {
                protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                project_id: snapshot.project_id.clone(),
                run_id: snapshot.run_id.clone(),
                after_sequence,
                replayed_count: replay_events.len() as u32,
                replay_truncated,
                oldest_available_sequence,
                latest_sequence,
            },
            replay_events,
        },
        receiver,
    )
}

fn live_event_response(event: &BufferedSupervisorEvent, replay: bool) -> SupervisorControlResponse {
    SupervisorControlResponse::Event {
        protocol_version: SUPERVISOR_PROTOCOL_VERSION,
        project_id: event.project_id.clone(),
        run_id: event.run_id.clone(),
        sequence: event.sequence,
        created_at: event.created_at.clone(),
        replay,
        item: event.item.clone(),
    }
}

fn remove_event_subscriber(event_hub: &Arc<Mutex<SupervisorEventHub>>, subscriber_id: u64) {
    event_hub
        .lock()
        .expect("event hub lock poisoned")
        .subscribers
        .remove(&subscriber_id);
}

#[allow(clippy::too_many_arguments)]
fn handle_submit_input_request(
    stream: &mut TcpStream,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    writer: &SharedPtyWriter,
    protocol_version: u8,
    project_id: String,
    run_id: String,
    session_id: String,
    flow_id: Option<String>,
    action_id: String,
    boundary_id: String,
    input: String,
) -> Result<(), CommandError> {
    if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
        write_protocol_error(
            stream,
            "runtime_supervisor_protocol_invalid",
            "Detached supervisor protocol version mismatch.",
            false,
        )?;
        return Ok(());
    }

    let snapshot = shared.lock().expect("sidecar state lock poisoned").clone();
    if project_id != snapshot.project_id || run_id != snapshot.run_id {
        write_protocol_error(
            stream,
            "runtime_supervisor_identity_mismatch",
            "Detached supervisor identity mismatch.",
            false,
        )?;
        return Ok(());
    }

    if session_id != snapshot.session_id || flow_id != snapshot.flow_id {
        write_protocol_error(
            stream,
            "runtime_supervisor_session_mismatch",
            "Detached supervisor session identity mismatch.",
            false,
        )?;
        return Ok(());
    }

    let Some(active_boundary) = snapshot.active_boundary.clone() else {
        write_protocol_error(
            stream,
            "runtime_supervisor_action_unavailable",
            "Cadence cannot resume the detached runtime because no operator boundary is currently pending.",
            false,
        )?;
        return Ok(());
    };

    if action_id != active_boundary.action_id || boundary_id != active_boundary.boundary_id {
        write_protocol_error(
            stream,
            "runtime_supervisor_action_mismatch",
            "Cadence rejected detached runtime resume for a stale or mismatched operator boundary.",
            false,
        )?;
        return Ok(());
    }

    let input = match normalize_control_input(&input) {
        Ok(input) => input,
        Err(error) => {
            write_protocol_error(stream, &error.code, &error.message, error.retryable)?;
            return Ok(());
        }
    };

    let delivered_title;
    let delivered_detail;
    let delivered_code;
    if active_boundary.action_type == "terminal_input_required" {
        let mut writer = writer
            .lock()
            .expect("runtime supervisor writer lock poisoned");
        if writer.write_all(input.as_bytes()).is_err()
            || writer
                .write_all(if input.ends_with('\n') { b"" } else { b"\n" })
                .is_err()
            || writer.flush().is_err()
        {
            write_protocol_error(
                stream,
                "runtime_supervisor_submit_input_failed",
                "Cadence could not write approved terminal input into the detached PTY.",
                true,
            )?;
            return Ok(());
        }
        drop(writer);

        delivered_code = "runtime_supervisor_input_delivered";
        delivered_title = "Terminal input delivered";
        delivered_detail = "Cadence wrote approved operator input into the active detached PTY.";
    } else {
        delivered_code = "runtime_supervisor_boundary_resumed";
        delivered_title = "Runtime boundary resumed";
        delivered_detail =
            "Cadence accepted operator approval for the active detached runtime boundary.";
    }

    {
        let mut state = shared.lock().expect("sidecar state lock poisoned");
        if state
            .active_boundary
            .as_ref()
            .is_some_and(|boundary| boundary.action_id == action_id)
        {
            state.active_boundary = None;
        }
    }

    let delivered_at = now_timestamp();
    append_live_event(
        shared,
        event_hub,
        &SupervisorLiveEventPayload::Activity {
            code: delivered_code.into(),
            title: delivered_title.into(),
            detail: Some(delivered_detail.into()),
        },
    );

    write_json_line(
        stream,
        &SupervisorControlResponse::SubmitInputAccepted {
            protocol_version: SUPERVISOR_PROTOCOL_VERSION,
            project_id: snapshot.project_id,
            run_id: snapshot.run_id,
            action_id,
            boundary_id,
            delivered_at,
        },
    )
    .map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_control_io_failed",
            "Cadence could not write the detached supervisor submit-input acknowledgement.",
        )
    })
}

fn handle_queue_controls_request(
    stream: &mut TcpStream,
    context: &ControlListenerContext,
    request: QueueControlsRequestArgs,
) -> Result<(), CommandError> {
    let QueueControlsRequestArgs {
        protocol_version,
        project_id,
        run_id,
        session_id,
        flow_id,
        controls,
        prompt,
    } = request;
    let repo_root = context.repo_root.as_path();
    let shared = &context.shared;
    let event_hub = &context.event_hub;
    let persistence_lock = &context.persistence_lock;

    if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
        write_protocol_error(
            stream,
            "runtime_supervisor_protocol_invalid",
            "Detached supervisor protocol version mismatch.",
            false,
        )?;
        return Ok(());
    }

    let snapshot = shared.lock().expect("sidecar state lock poisoned").clone();
    if project_id != snapshot.project_id || run_id != snapshot.run_id {
        write_protocol_error(
            stream,
            "runtime_supervisor_identity_mismatch",
            "Detached supervisor identity mismatch.",
            false,
        )?;
        return Ok(());
    }

    if session_id != snapshot.session_id || flow_id != snapshot.flow_id {
        write_protocol_error(
            stream,
            "runtime_supervisor_session_mismatch",
            "Detached supervisor session identity mismatch.",
            false,
        )?;
        return Ok(());
    }

    if controls.is_none() && prompt.is_none() {
        write_protocol_error(
            stream,
            "runtime_supervisor_queue_controls_invalid",
            "Cadence requires a prompt or control delta before it can queue runtime-run changes.",
            false,
        )?;
        return Ok(());
    }

    let normalized_prompt = match prompt.as_deref() {
        Some(prompt) => match normalize_queued_prompt(prompt) {
            Ok(prompt) => Some(prompt),
            Err(error) => {
                write_protocol_error(stream, &error.code, &error.message, error.retryable)?;
                return Ok(());
            }
        },
        None => None,
    };

    let queued_at = now_timestamp();
    let next_pending = match build_pending_control_snapshot(
        &snapshot.control_state,
        controls.as_ref(),
        normalized_prompt.as_deref(),
        &queued_at,
    ) {
        Ok(pending) => pending,
        Err(error) => {
            write_protocol_error(stream, &error.code, &error.message, error.retryable)?;
            return Ok(());
        }
    };

    {
        let mut state = shared.lock().expect("sidecar state lock poisoned");
        state.control_state.pending = Some(next_pending.clone());
        state.last_error = None;
    }

    if let Err(error) = persist_runtime_row_from_shared(repo_root, shared, persistence_lock) {
        let mut state = shared.lock().expect("sidecar state lock poisoned");
        state.control_state = snapshot.control_state.clone();
        state.last_error = snapshot.last_error.clone();
        drop(state);
        write_protocol_error(
            stream,
            "runtime_supervisor_queue_controls_failed",
            &format!(
                "Cadence could not persist queued runtime-run controls before acknowledging the control request: {}",
                error.message
            ),
            true,
        )?;
        return Ok(());
    }

    append_live_event(
        shared,
        event_hub,
        &SupervisorLiveEventPayload::Activity {
            code: "runtime_run_controls_queued".into(),
            title: "Queued runtime controls".into(),
            detail: Some(if next_pending.queued_prompt.is_some() {
                "Cadence queued pending runtime-run controls and one prompt for the next model-call boundary.".into()
            } else {
                "Cadence queued pending runtime-run controls for the next model-call boundary."
                    .into()
            }),
        },
    );
    let _ = persist_sidecar_checkpoint(
        repo_root,
        shared,
        persistence_lock,
        crate::db::project_store::RuntimeRunStatus::Running,
        crate::db::project_store::RuntimeRunCheckpointKind::State,
        format!("{ACTIVITY_OUTPUT_PREFIX} runtime_run_controls_queued: Queued runtime controls"),
    );

    write_json_line(
        stream,
        &SupervisorControlResponse::QueueControlsAccepted {
            protocol_version: SUPERVISOR_PROTOCOL_VERSION,
            project_id: snapshot.project_id,
            run_id: snapshot.run_id,
            session_id: snapshot.session_id,
            flow_id: snapshot.flow_id,
            pending_revision: next_pending.revision,
            queued_at,
            prompt_queued: next_pending.queued_prompt.is_some(),
        },
    )
    .map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_control_io_failed",
            "Cadence could not write the detached supervisor queued-controls acknowledgement.",
        )
    })
}

fn build_pending_control_snapshot(
    control_state: &crate::db::project_store::RuntimeRunControlStateRecord,
    controls: Option<&RuntimeRunControlInputDto>,
    prompt: Option<&str>,
    queued_at: &str,
) -> Result<RuntimeRunPendingControlSnapshotRecord, CommandError> {
    let base = control_state.pending.as_ref();
    let pending_revision = base
        .map_or(control_state.active.revision.saturating_add(1), |pending| {
            pending.revision.saturating_add(1)
        });

    let model_id = controls
        .map(|controls| controls.model_id.clone())
        .or_else(|| base.map(|pending| pending.model_id.clone()))
        .unwrap_or_else(|| control_state.active.model_id.clone());
    let thinking_effort = controls
        .map(|controls| controls.thinking_effort.clone())
        .or_else(|| base.map(|pending| pending.thinking_effort.clone()))
        .unwrap_or_else(|| control_state.active.thinking_effort.clone());
    let approval_mode = controls
        .map(|controls| controls.approval_mode.clone())
        .or_else(|| base.map(|pending| pending.approval_mode.clone()))
        .unwrap_or_else(|| control_state.active.approval_mode.clone());

    let existing_prompt = base.and_then(|pending| pending.queued_prompt.clone());
    let existing_prompt_at = base.and_then(|pending| pending.queued_prompt_at.clone());
    let (queued_prompt, queued_prompt_at) = match prompt {
        Some(prompt) => {
            if existing_prompt.is_some() {
                return Err(CommandError::user_fixable(
                    "runtime_supervisor_prompt_already_queued",
                    "Cadence already has one queued prompt for this run. Wait for it to apply before sending another prompt.",
                ));
            }
            (Some(prompt.to_owned()), Some(queued_at.to_owned()))
        }
        None => (existing_prompt, existing_prompt_at),
    };

    Ok(RuntimeRunPendingControlSnapshotRecord {
        model_id,
        thinking_effort,
        approval_mode,
        revision: pending_revision,
        queued_at: queued_at.to_owned(),
        queued_prompt,
        queued_prompt_at,
    })
}

fn normalize_queued_prompt(prompt: &str) -> Result<String, CommandError> {
    let normalized = prompt.trim_end_matches(['\r', '\n']);
    if normalized.trim().is_empty() {
        return Err(CommandError::invalid_request("prompt"));
    }

    if normalized.chars().count() > MAX_CONTROL_INPUT_CHARS {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_queue_controls_invalid",
            "Cadence refused an oversized queued prompt for the detached PTY.",
        ));
    }

    Ok(normalized.to_string())
}

fn normalize_control_input(input: &str) -> Result<String, CommandError> {
    let normalized = input.trim_end_matches(['\r', '\n']);
    if normalized.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_submit_input_invalid",
            "Cadence requires a non-empty operator decision payload before it can resume the detached runtime boundary.",
        ));
    }

    if normalized.chars().count() > MAX_CONTROL_INPUT_CHARS {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_submit_input_invalid",
            "Cadence refused oversized operator decision payload for the detached runtime boundary.",
        ));
    }

    Ok(normalized.to_string())
}

fn write_protocol_error(
    stream: &mut TcpStream,
    code: &str,
    message: &str,
    retryable: bool,
) -> Result<(), CommandError> {
    write_json_line(
        stream,
        &SupervisorControlResponse::Error {
            protocol_version: SUPERVISOR_PROTOCOL_VERSION,
            code: code.into(),
            message: message.into(),
            retryable,
        },
    )
    .map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_control_io_failed",
            "Cadence could not write the detached supervisor control error response.",
        )
    })
}
