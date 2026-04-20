use std::{
    collections::HashSet,
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use tauri::{ipc::Channel, AppHandle, Runtime};

use crate::{
    commands::{
        get_runtime_session::reconcile_runtime_session,
        runtime_support::{
            load_persisted_runtime_run, load_runtime_run_status, load_runtime_session_status,
            DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
        },
        CommandError, OperatorApprovalStatus, RuntimeAuthPhase, RuntimeStreamItemDto,
        RuntimeStreamItemKind, RuntimeToolCallState,
    },
    db::project_store::{
        self, RuntimeRunSnapshotRecord, RuntimeRunStatus, RuntimeRunTransportLiveness,
    },
    runtime::protocol::{
        SupervisorControlRequest, SupervisorControlResponse, SupervisorLiveEventPayload,
        SupervisorToolCallState, SUPERVISOR_PROTOCOL_VERSION,
    },
    state::DesktopState,
};

const ATTACH_FRAME_POLL_INTERVAL: Duration = Duration::from_millis(200);
const ATTACH_RETRY_INTERVAL: Duration = Duration::from_millis(120);
const ATTACH_RETRY_ATTEMPTS: u32 = 4;
const TERMINAL_SNAPSHOT_RETRY_INTERVAL: Duration = Duration::from_millis(120);
const TERMINAL_SNAPSHOT_RETRY_ATTEMPTS: u32 = 6;
const PROTOCOL_LINE_LIMIT: usize = 16 * 1024;

#[derive(Debug, Clone, Default)]
pub struct RuntimeStreamController {
    inner: Arc<Mutex<RuntimeStreamRegistry>>,
}

#[derive(Debug, Default)]
struct RuntimeStreamRegistry {
    next_generation: u64,
    active: Option<ActiveRuntimeStream>,
}

#[derive(Debug, Clone)]
struct ActiveRuntimeStream {
    project_id: String,
    session_id: String,
    run_id: String,
    generation: u64,
    cancelled: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
struct RuntimeStreamLease {
    project_id: String,
    session_id: String,
    run_id: String,
    generation: u64,
    cancelled: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct RuntimeStreamRequest {
    pub project_id: String,
    pub repo_root: PathBuf,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub runtime_kind: String,
    pub run_id: String,
    pub requested_item_kinds: Vec<RuntimeStreamItemKind>,
}

#[derive(Debug, Clone)]
struct PendingActionRequired {
    action_id: String,
    boundary_id: Option<String>,
    action_type: String,
    title: String,
    detail: String,
    created_at: String,
}

#[derive(Debug)]
struct AttachAck {
    replayed_count: u32,
}

#[derive(Debug, Default)]
struct AttachForwardState {
    last_sequence: u64,
    action_required_ids: HashSet<String>,
}

#[derive(Debug)]
struct StreamFailure {
    error: CommandError,
    last_sequence: u64,
}

enum StreamExit {
    Cancelled,
    Failed(StreamFailure),
}

type StreamResult<T = u64> = Result<T, StreamExit>;

impl RuntimeStreamController {
    fn begin_stream(&self, project_id: &str, session_id: &str, run_id: &str) -> RuntimeStreamLease {
        let mut registry = self.inner.lock().expect("runtime stream registry poisoned");

        if let Some(active) = registry.active.take() {
            active.cancelled.store(true, Ordering::SeqCst);
        }

        registry.next_generation = registry.next_generation.saturating_add(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let generation = registry.next_generation;

        registry.active = Some(ActiveRuntimeStream {
            project_id: project_id.into(),
            session_id: session_id.into(),
            run_id: run_id.into(),
            generation,
            cancelled: cancelled.clone(),
        });

        RuntimeStreamLease {
            project_id: project_id.into(),
            session_id: session_id.into(),
            run_id: run_id.into(),
            generation,
            cancelled,
        }
    }

    fn finish_stream(&self, lease: &RuntimeStreamLease) {
        let mut registry = self.inner.lock().expect("runtime stream registry poisoned");
        let should_clear = registry.active.as_ref().is_some_and(|active| {
            active.project_id == lease.project_id
                && active.session_id == lease.session_id
                && active.run_id == lease.run_id
                && active.generation == lease.generation
        });

        if should_clear {
            registry.active = None;
        }
    }
}

impl RuntimeStreamLease {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

pub fn start_runtime_stream<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    request: RuntimeStreamRequest,
    channel: Channel<RuntimeStreamItemDto>,
) {
    let lease = state.runtime_stream_controller().begin_stream(
        &request.project_id,
        &request.session_id,
        &request.run_id,
    );
    let controller = state.runtime_stream_controller().clone();

    std::thread::spawn(move || {
        let outcome = emit_runtime_stream(&app, &state, &request, &lease, &channel);

        if let Err(StreamExit::Failed(failure)) = outcome {
            let _ = emit_failure_item(
                &channel,
                &request,
                failure.last_sequence.saturating_add(1),
                failure.error,
            );
        }

        controller.finish_stream(&lease);
    });
}

fn emit_runtime_stream<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    request: &RuntimeStreamRequest,
    lease: &RuntimeStreamLease,
    channel: &Channel<RuntimeStreamItemDto>,
) -> StreamResult {
    ensure_stream_active(lease)?;
    ensure_stream_identity(app, state, request, 0)?;

    let runtime_run = load_streamable_runtime_run(request, 0)?;
    let pending_action_required = load_pending_action_required(request, 0)?;
    let mut attach_state =
        attach_and_forward_supervisor_stream(request, lease, channel, &runtime_run)?;

    ensure_stream_active(lease)?;
    let terminal_snapshot =
        load_terminal_runtime_snapshot(state, request, attach_state.last_sequence)?;

    for approval in pending_action_required {
        if !attach_state
            .action_required_ids
            .insert(approval.action_id.clone())
        {
            continue;
        }

        attach_state.last_sequence = attach_state.last_sequence.saturating_add(1);
        emit_item_if_requested(
            channel,
            request,
            lease,
            action_required_item(request, attach_state.last_sequence, approval),
        )?;
    }

    emit_terminal_item(
        channel,
        request,
        lease,
        &terminal_snapshot,
        attach_state.last_sequence,
    )
}

fn ensure_stream_identity<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    request: &RuntimeStreamRequest,
    last_sequence: u64,
) -> StreamResult<()> {
    let runtime = load_runtime_session_status(state, &request.repo_root, &request.project_id)
        .map_err(|error| {
            StreamExit::Failed(StreamFailure {
                error,
                last_sequence,
            })
        })?;
    let runtime =
        reconcile_runtime_session(app, state, &request.repo_root, runtime).map_err(|error| {
            StreamExit::Failed(StreamFailure {
                error,
                last_sequence,
            })
        })?;

    if runtime.phase != RuntimeAuthPhase::Authenticated {
        return Err(StreamExit::Failed(StreamFailure {
            error: runtime_auth_lost_error(&runtime.phase),
            last_sequence,
        }));
    }

    let latest_session_id = runtime.session_id.ok_or_else(|| {
        StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_stream_session_missing",
                "Cadence could not keep the live runtime stream attached because the authenticated runtime session became incomplete.",
            ),
            last_sequence,
        })
    })?;

    if latest_session_id != request.session_id {
        return Err(StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_stream_session_stale",
                "Cadence discarded a stale runtime stream because the selected project's authenticated session changed.",
            ),
            last_sequence,
        }));
    }

    Ok(())
}

fn load_terminal_runtime_snapshot(
    state: &DesktopState,
    request: &RuntimeStreamRequest,
    last_sequence: u64,
) -> StreamResult<RuntimeRunSnapshotRecord> {
    let mut latest_snapshot: Option<RuntimeRunSnapshotRecord> = None;

    for attempt in 0..TERMINAL_SNAPSHOT_RETRY_ATTEMPTS {
        let snapshot = load_runtime_run_status(state, &request.repo_root, &request.project_id)
            .map_err(|error| {
                StreamExit::Failed(StreamFailure {
                    error,
                    last_sequence,
                })
            })?
            .ok_or_else(|| {
                StreamExit::Failed(StreamFailure {
                    error: CommandError::retryable(
                        "runtime_stream_run_unavailable",
                        "Cadence lost the durable runtime-run row before the live stream could finish cleanly.",
                    ),
                    last_sequence,
                })
            })?;

        if snapshot.run.run_id != request.run_id {
            return Err(StreamExit::Failed(StreamFailure {
                error: CommandError::retryable(
                    "runtime_stream_run_replaced",
                    format!(
                        "Cadence discarded the live runtime stream because run `{}` replaced `{}` before the bridge finished.",
                        snapshot.run.run_id, request.run_id
                    ),
                ),
                last_sequence,
            }));
        }

        let is_terminal = matches!(
            snapshot.run.status,
            RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
        );
        latest_snapshot = Some(snapshot);
        if is_terminal {
            break;
        }

        if attempt + 1 < TERMINAL_SNAPSHOT_RETRY_ATTEMPTS {
            std::thread::sleep(TERMINAL_SNAPSHOT_RETRY_INTERVAL);
        }
    }

    latest_snapshot.ok_or_else(|| {
        StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_stream_run_unavailable",
                "Cadence lost the durable runtime-run row before the live stream could finish cleanly.",
            ),
            last_sequence,
        })
    })
}

fn load_streamable_runtime_run(
    request: &RuntimeStreamRequest,
    last_sequence: u64,
) -> StreamResult<RuntimeRunSnapshotRecord> {
    let snapshot = load_persisted_runtime_run(&request.repo_root, &request.project_id)
        .map_err(|error| StreamExit::Failed(StreamFailure { error, last_sequence }))?
        .ok_or_else(|| {
            StreamExit::Failed(StreamFailure {
                error: CommandError::retryable(
                    "runtime_stream_run_unavailable",
                    "Cadence cannot attach a live runtime stream because the selected project has no durable run to bridge.",
                ),
                last_sequence,
            })
        })?;

    if snapshot.run.run_id != request.run_id {
        return Err(StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_stream_run_replaced",
                format!(
                    "Cadence discarded the live runtime stream because run `{}` replaced `{}` before attach started.",
                    snapshot.run.run_id, request.run_id
                ),
            ),
            last_sequence,
        }));
    }

    ensure_attachable_runtime_run(&snapshot).map_err(|error| {
        StreamExit::Failed(StreamFailure {
            error,
            last_sequence,
        })
    })?;

    Ok(snapshot)
}

fn load_pending_action_required(
    request: &RuntimeStreamRequest,
    last_sequence: u64,
) -> StreamResult<Vec<PendingActionRequired>> {
    let snapshot = project_store::load_project_snapshot(&request.repo_root, &request.project_id)
        .map_err(|error| {
            StreamExit::Failed(StreamFailure {
                error,
                last_sequence,
            })
        })?
        .snapshot;

    let mut pending = Vec::new();

    for approval in snapshot
        .approval_requests
        .into_iter()
        .filter(|approval| approval.status == OperatorApprovalStatus::Pending)
    {
        if let Some(session_id) = approval.session_id.as_deref() {
            if session_id != request.session_id {
                return Err(StreamExit::Failed(StreamFailure {
                    error: CommandError::system_fault(
                        "runtime_stream_session_mismatch",
                        format!(
                            "Cadence refused to project pending approval `{}` because it belongs to session `{session_id}` while `{}` is active.",
                            approval.action_id, request.session_id
                        ),
                    ),
                    last_sequence,
                }));
            }
        }

        if let (Some(expected_flow_id), Some(flow_id)) =
            (request.flow_id.as_deref(), approval.flow_id.as_deref())
        {
            if flow_id != expected_flow_id {
                return Err(StreamExit::Failed(StreamFailure {
                    error: CommandError::system_fault(
                        "runtime_stream_flow_mismatch",
                        format!(
                            "Cadence refused to project pending approval `{}` because it belongs to flow `{flow_id}` while `{expected_flow_id}` is active.",
                            approval.action_id
                        ),
                    ),
                    last_sequence,
                }));
            }
        }

        require_non_empty(
            Some(approval.action_id.as_str()),
            "actionId",
            "runtime action-required item",
        )
        .map_err(|error| {
            StreamExit::Failed(StreamFailure {
                error,
                last_sequence,
            })
        })?;
        require_non_empty(
            Some(approval.action_type.as_str()),
            "actionType",
            "runtime action-required item",
        )
        .map_err(|error| {
            StreamExit::Failed(StreamFailure {
                error,
                last_sequence,
            })
        })?;
        require_non_empty(
            Some(approval.title.as_str()),
            "title",
            "runtime action-required item",
        )
        .map_err(|error| {
            StreamExit::Failed(StreamFailure {
                error,
                last_sequence,
            })
        })?;
        require_non_empty(
            Some(approval.detail.as_str()),
            "detail",
            "runtime action-required item",
        )
        .map_err(|error| {
            StreamExit::Failed(StreamFailure {
                error,
                last_sequence,
            })
        })?;

        let boundary_id = parse_runtime_boundary_id_for_run(&approval.action_id, &request.run_id)
            .map_err(|error| {
            StreamExit::Failed(StreamFailure {
                error,
                last_sequence,
            })
        })?;

        let Some(boundary_id) = boundary_id else {
            continue;
        };

        pending.push(PendingActionRequired {
            action_id: approval.action_id,
            boundary_id: Some(boundary_id),
            action_type: approval.action_type,
            title: approval.title,
            detail: approval.detail,
            created_at: approval.created_at,
        });
    }

    pending.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.action_id.cmp(&right.action_id))
    });

    Ok(pending)
}

fn attach_and_forward_supervisor_stream(
    request: &RuntimeStreamRequest,
    lease: &RuntimeStreamLease,
    channel: &Channel<RuntimeStreamItemDto>,
    runtime_run: &RuntimeRunSnapshotRecord,
) -> StreamResult<AttachForwardState> {
    let (mut reader, attach_ack) =
        open_attach_reader_with_ack(request, lease, &runtime_run.run.transport.endpoint)?;
    let mut attach_state = AttachForwardState::default();

    for _ in 0..attach_ack.replayed_count {
        ensure_stream_active(lease)?;
        attach_state = read_and_forward_event(&mut reader, request, lease, channel, attach_state)?;
    }

    loop {
        ensure_stream_active(lease)?;
        match read_supervisor_response(&mut reader) {
            Ok(Some(response)) => {
                attach_state =
                    forward_supervisor_response(response, request, lease, channel, attach_state)?;
            }
            Ok(None) => return Ok(attach_state),
            Err(ReadSupervisorResponseError::Timeout) => continue,
            Err(ReadSupervisorResponseError::Io(error)) => {
                return Err(StreamExit::Failed(StreamFailure {
                    error: CommandError::retryable(
                        "runtime_stream_attach_io_failed",
                        format!(
                            "Cadence lost the detached supervisor attach stream while bridging live runtime events: {error}"
                        ),
                    ),
                    last_sequence: attach_state.last_sequence,
                }));
            }
            Err(ReadSupervisorResponseError::Decode(error)) => {
                return Err(StreamExit::Failed(StreamFailure {
                    error: CommandError::system_fault(
                        "runtime_stream_contract_invalid",
                        format!(
                            "Cadence could not decode a detached supervisor attach frame while bridging the live runtime stream: {error}"
                        ),
                    ),
                    last_sequence: attach_state.last_sequence,
                }));
            }
        }
    }
}

fn open_attach_reader_with_ack(
    request: &RuntimeStreamRequest,
    lease: &RuntimeStreamLease,
    endpoint: &str,
) -> StreamResult<(BufReader<TcpStream>, AttachAck)> {
    let mut last_failure: Option<StreamFailure> = None;

    for attempt in 0..ATTACH_RETRY_ATTEMPTS {
        ensure_stream_active(lease)?;

        let mut reader = match open_attach_reader(request, endpoint, 0) {
            Ok(reader) => reader,
            Err(StreamExit::Failed(failure)) => {
                last_failure = Some(failure);
                if attempt + 1 < ATTACH_RETRY_ATTEMPTS {
                    std::thread::sleep(ATTACH_RETRY_INTERVAL);
                    continue;
                }

                return Err(StreamExit::Failed(
                    last_failure.expect("attach failure captured"),
                ));
            }
            Err(other) => return Err(other),
        };

        match read_attach_ack(&mut reader, request, 0) {
            Ok(attach_ack) => return Ok((reader, attach_ack)),
            Err(StreamExit::Failed(failure)) => {
                last_failure = Some(failure);
                if attempt + 1 < ATTACH_RETRY_ATTEMPTS {
                    std::thread::sleep(ATTACH_RETRY_INTERVAL);
                    continue;
                }

                return Err(StreamExit::Failed(
                    last_failure.expect("attach failure captured"),
                ));
            }
            Err(other) => return Err(other),
        }
    }

    Err(StreamExit::Failed(last_failure.unwrap_or(StreamFailure {
        error: CommandError::retryable(
            "runtime_stream_attach_connect_failed",
            format!(
                "Cadence could not connect the live runtime stream to detached run `{}`.",
                request.run_id
            ),
        ),
        last_sequence: 0,
    })))
}

fn open_attach_reader(
    request: &RuntimeStreamRequest,
    endpoint: &str,
    last_sequence: u64,
) -> StreamResult<BufReader<TcpStream>> {
    let address = endpoint.parse::<SocketAddr>().map_err(|_| {
        StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_supervisor_endpoint_invalid",
                "Cadence could not parse the detached supervisor control endpoint for the live runtime stream.",
            ),
            last_sequence,
        })
    })?;

    let mut stream = TcpStream::connect_timeout(&address, DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT)
        .map_err(|_| {
            StreamExit::Failed(StreamFailure {
                error: CommandError::retryable(
                    "runtime_stream_attach_connect_failed",
                    format!(
                        "Cadence could not connect the live runtime stream to detached run `{}`.",
                        request.run_id
                    ),
                ),
                last_sequence,
            })
        })?;

    stream
        .set_write_timeout(Some(DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT))
        .map_err(|_| {
            StreamExit::Failed(StreamFailure {
                error: CommandError::retryable(
                    "runtime_stream_attach_timeout_config_failed",
                    "Cadence could not configure the live runtime stream attach write timeout.",
                ),
                last_sequence,
            })
        })?;
    stream
        .set_read_timeout(Some(DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT))
        .map_err(|_| {
            StreamExit::Failed(StreamFailure {
                error: CommandError::retryable(
                    "runtime_stream_attach_timeout_config_failed",
                    "Cadence could not configure the live runtime stream attach read timeout.",
                ),
                last_sequence,
            })
        })?;

    write_json_line(
        &mut stream,
        &SupervisorControlRequest::attach(&request.project_id, &request.run_id, None),
    )
    .map_err(|error| {
        StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_stream_attach_write_failed",
                format!(
                    "Cadence could not send the detached supervisor attach request for run `{}`: {error}",
                    request.run_id
                ),
            ),
            last_sequence,
        })
    })?;

    stream
        .set_read_timeout(Some(ATTACH_FRAME_POLL_INTERVAL))
        .map_err(|_| {
            StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_stream_attach_timeout_config_failed",
                "Cadence could not switch the live runtime stream attach socket into polling mode.",
            ),
            last_sequence,
        })
        })?;

    Ok(BufReader::new(stream))
}

fn read_attach_ack(
    reader: &mut BufReader<TcpStream>,
    request: &RuntimeStreamRequest,
    last_sequence: u64,
) -> StreamResult<AttachAck> {
    match read_supervisor_response(reader) {
        Ok(Some(SupervisorControlResponse::Attached {
            protocol_version,
            project_id,
            run_id,
            replayed_count,
            ..
        })) => {
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                return Err(StreamExit::Failed(StreamFailure {
                    error: CommandError::retryable(
                        "runtime_stream_contract_invalid",
                        "Cadence rejected the detached supervisor attach acknowledgement because its protocol version was unsupported.",
                    ),
                    last_sequence,
                }));
            }

            if project_id != request.project_id || run_id != request.run_id {
                return Err(StreamExit::Failed(StreamFailure {
                    error: CommandError::retryable(
                        "runtime_stream_run_replaced",
                        "Cadence rejected the detached supervisor attach acknowledgement because it no longer matched the active project or run.",
                    ),
                    last_sequence,
                }));
            }

            Ok(AttachAck { replayed_count })
        }
        Ok(Some(SupervisorControlResponse::Error {
            code,
            message,
            retryable,
            ..
        })) => Err(StreamExit::Failed(StreamFailure {
            error: if retryable {
                CommandError::retryable(code, message)
            } else {
                CommandError::user_fixable(code, message)
            },
            last_sequence,
        })),
        Ok(Some(other)) => Err(StreamExit::Failed(StreamFailure {
            error: CommandError::system_fault(
                "runtime_stream_contract_invalid",
                format!(
                    "Cadence expected a detached supervisor attach acknowledgement but received `{other:?}` instead."
                ),
            ),
            last_sequence,
        })),
        Ok(None) => Err(StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_stream_attach_closed",
                "Cadence lost the detached supervisor attach stream before the acknowledgement arrived.",
            ),
            last_sequence,
        })),
        Err(ReadSupervisorResponseError::Timeout) => Err(StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_stream_attach_timeout",
                format!(
                    "Cadence timed out while waiting for detached supervisor run `{}` to acknowledge the live stream attach.",
                    request.run_id
                ),
            ),
            last_sequence,
        })),
        Err(ReadSupervisorResponseError::Io(error)) => Err(StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_stream_attach_io_failed",
                format!(
                    "Cadence lost the detached supervisor attach stream before the acknowledgement completed: {error}"
                ),
            ),
            last_sequence,
        })),
        Err(ReadSupervisorResponseError::Decode(error)) => Err(StreamExit::Failed(StreamFailure {
            error: CommandError::system_fault(
                "runtime_stream_contract_invalid",
                format!(
                    "Cadence could not decode the detached supervisor attach acknowledgement: {error}"
                ),
            ),
            last_sequence,
        })),
    }
}

fn read_and_forward_event(
    reader: &mut BufReader<TcpStream>,
    request: &RuntimeStreamRequest,
    lease: &RuntimeStreamLease,
    channel: &Channel<RuntimeStreamItemDto>,
    attach_state: AttachForwardState,
) -> StreamResult<AttachForwardState> {
    match read_supervisor_response(reader) {
        Ok(Some(response)) => {
            forward_supervisor_response(response, request, lease, channel, attach_state)
        }
        Ok(None) => Err(StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_stream_attach_closed",
                "Cadence lost the detached supervisor attach stream while replaying buffered runtime events.",
            ),
            last_sequence: attach_state.last_sequence,
        })),
        Err(ReadSupervisorResponseError::Timeout) => Err(StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_stream_attach_timeout",
                "Cadence timed out while replaying buffered runtime events from the detached supervisor.",
            ),
            last_sequence: attach_state.last_sequence,
        })),
        Err(ReadSupervisorResponseError::Io(error)) => Err(StreamExit::Failed(StreamFailure {
            error: CommandError::retryable(
                "runtime_stream_attach_io_failed",
                format!(
                    "Cadence lost the detached supervisor attach stream while replaying buffered runtime events: {error}"
                ),
            ),
            last_sequence: attach_state.last_sequence,
        })),
        Err(ReadSupervisorResponseError::Decode(error)) => Err(StreamExit::Failed(StreamFailure {
            error: CommandError::system_fault(
                "runtime_stream_contract_invalid",
                format!(
                    "Cadence could not decode a replayed detached supervisor event frame: {error}"
                ),
            ),
            last_sequence: attach_state.last_sequence,
        })),
    }
}

fn forward_supervisor_response(
    response: SupervisorControlResponse,
    request: &RuntimeStreamRequest,
    lease: &RuntimeStreamLease,
    channel: &Channel<RuntimeStreamItemDto>,
    mut attach_state: AttachForwardState,
) -> StreamResult<AttachForwardState> {
    match response {
        SupervisorControlResponse::Event {
            protocol_version,
            project_id,
            run_id,
            sequence,
            created_at,
            item,
            ..
        } => {
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                return Err(StreamExit::Failed(StreamFailure {
                    error: CommandError::system_fault(
                        "runtime_stream_contract_invalid",
                        "Cadence rejected a detached supervisor event frame because its protocol version was unsupported.",
                    ),
                    last_sequence: attach_state.last_sequence,
                }));
            }

            if project_id != request.project_id || run_id != request.run_id {
                return Err(StreamExit::Failed(StreamFailure {
                    error: CommandError::retryable(
                        "runtime_stream_run_replaced",
                        "Cadence rejected a detached supervisor event frame because it no longer matched the active project or run.",
                    ),
                    last_sequence: attach_state.last_sequence,
                }));
            }

            if sequence == 0 || sequence <= attach_state.last_sequence {
                return Err(StreamExit::Failed(StreamFailure {
                    error: CommandError::system_fault(
                        "runtime_stream_sequence_invalid",
                        format!(
                            "Cadence rejected detached supervisor event sequence {sequence} because the prior bridged sequence was {}.",
                            attach_state.last_sequence
                        ),
                    ),
                    last_sequence: attach_state.last_sequence,
                }));
            }

            let item = map_supervisor_event_to_stream_item(request, sequence, created_at, item)
                .map_err(|error| StreamExit::Failed(StreamFailure {
                    error,
                    last_sequence: attach_state.last_sequence,
                }))?;
            if let Some(action_id) = item.action_id.as_ref() {
                attach_state.action_required_ids.insert(action_id.clone());
            }
            emit_item_if_requested(channel, request, lease, item)?;
            attach_state.last_sequence = sequence;
            Ok(attach_state)
        }
        SupervisorControlResponse::Error {
            code,
            message,
            retryable,
            ..
        } => Err(StreamExit::Failed(StreamFailure {
            error: if retryable {
                CommandError::retryable(code, message)
            } else {
                CommandError::user_fixable(code, message)
            },
            last_sequence: attach_state.last_sequence,
        })),
        other => Err(StreamExit::Failed(StreamFailure {
            error: CommandError::system_fault(
                "runtime_stream_contract_invalid",
                format!(
                    "Cadence expected a detached supervisor event frame but received `{other:?}` instead."
                ),
            ),
            last_sequence: attach_state.last_sequence,
        })),
    }
}

fn map_supervisor_event_to_stream_item(
    request: &RuntimeStreamRequest,
    sequence: u64,
    created_at: String,
    item: SupervisorLiveEventPayload,
) -> Result<RuntimeStreamItemDto, CommandError> {
    let item = match item {
        SupervisorLiveEventPayload::Transcript { text } => RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Transcript,
            run_id: request.run_id.clone(),
            sequence,
            session_id: Some(request.session_id.clone()),
            flow_id: request.flow_id.clone(),
            text: Some(text),
            tool_call_id: None,
            tool_name: None,
            tool_state: None,
            tool_summary: None,
            action_id: None,
            boundary_id: None,
            action_type: None,
            title: None,
            detail: None,
            code: None,
            message: None,
            retryable: None,
            created_at,
        },
        SupervisorLiveEventPayload::Tool {
            tool_call_id,
            tool_name,
            tool_state,
            detail,
            tool_summary,
        } => RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Tool,
            run_id: request.run_id.clone(),
            sequence,
            session_id: Some(request.session_id.clone()),
            flow_id: request.flow_id.clone(),
            text: None,
            tool_call_id: Some(tool_call_id),
            tool_name: Some(tool_name),
            tool_state: Some(map_supervisor_tool_state(tool_state)),
            tool_summary,
            action_id: None,
            boundary_id: None,
            action_type: None,
            title: None,
            detail,
            code: None,
            message: None,
            retryable: None,
            created_at,
        },
        SupervisorLiveEventPayload::Activity {
            code,
            title,
            detail,
        } => RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Activity,
            run_id: request.run_id.clone(),
            sequence,
            session_id: Some(request.session_id.clone()),
            flow_id: request.flow_id.clone(),
            text: None,
            tool_call_id: None,
            tool_name: None,
            tool_state: None,
            tool_summary: None,
            action_id: None,
            boundary_id: None,
            action_type: None,
            title: Some(title),
            detail,
            code: Some(code),
            message: None,
            retryable: None,
            created_at,
        },
        SupervisorLiveEventPayload::ActionRequired {
            action_id,
            boundary_id,
            action_type,
            title,
            detail,
        } => RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::ActionRequired,
            run_id: request.run_id.clone(),
            sequence,
            session_id: Some(request.session_id.clone()),
            flow_id: request.flow_id.clone(),
            text: None,
            tool_call_id: None,
            tool_name: None,
            tool_state: None,
            tool_summary: None,
            action_id: Some(action_id),
            boundary_id: Some(boundary_id),
            action_type: Some(action_type),
            title: Some(title),
            detail: Some(detail),
            code: None,
            message: None,
            retryable: None,
            created_at,
        },
    };

    validate_stream_item(&item)?;
    Ok(item)
}

fn map_supervisor_tool_state(state: SupervisorToolCallState) -> RuntimeToolCallState {
    match state {
        SupervisorToolCallState::Pending => RuntimeToolCallState::Pending,
        SupervisorToolCallState::Running => RuntimeToolCallState::Running,
        SupervisorToolCallState::Succeeded => RuntimeToolCallState::Succeeded,
        SupervisorToolCallState::Failed => RuntimeToolCallState::Failed,
    }
}

fn emit_terminal_item(
    channel: &Channel<RuntimeStreamItemDto>,
    request: &RuntimeStreamRequest,
    lease: &RuntimeStreamLease,
    snapshot: &RuntimeRunSnapshotRecord,
    last_sequence: u64,
) -> StreamResult<u64> {
    let next_sequence = last_sequence.saturating_add(1);

    match snapshot.run.status {
        RuntimeRunStatus::Stopped => {
            emit_item_if_requested(
                channel,
                request,
                lease,
                complete_item(
                    request,
                    next_sequence,
                    format!(
                        "Detached runtime run `{}` finished and closed the live stream.",
                        snapshot.run.run_id
                    ),
                ),
            )?;
            Ok(next_sequence)
        }
        RuntimeRunStatus::Failed => {
            let error = snapshot.run.last_error.as_ref().map_or_else(
                || {
                    CommandError::user_fixable(
                        "runtime_stream_run_failed",
                        format!(
                            "Cadence marked detached runtime run `{}` as failed after the live stream closed.",
                            snapshot.run.run_id
                        ),
                    )
                },
                |diagnostic| CommandError::user_fixable(&diagnostic.code, &diagnostic.message),
            );
            emit_failure_item(channel, request, next_sequence, error).map_err(|error| {
                StreamExit::Failed(StreamFailure {
                    error,
                    last_sequence,
                })
            })?;
            Ok(next_sequence)
        }
        RuntimeRunStatus::Stale | RuntimeRunStatus::Starting | RuntimeRunStatus::Running => {
            let error = snapshot.run.last_error.as_ref().map_or_else(
                || {
                    CommandError::retryable(
                        "runtime_stream_run_stale",
                        format!(
                            "Cadence lost the detached supervisor attach stream for run `{}` before it reached a terminal state.",
                            snapshot.run.run_id
                        ),
                    )
                },
                |diagnostic| CommandError::retryable(&diagnostic.code, &diagnostic.message),
            );
            emit_failure_item(channel, request, next_sequence, error).map_err(|error| {
                StreamExit::Failed(StreamFailure {
                    error,
                    last_sequence,
                })
            })?;
            Ok(next_sequence)
        }
    }
}

fn ensure_attachable_runtime_run(snapshot: &RuntimeRunSnapshotRecord) -> Result<(), CommandError> {
    let reachable = snapshot.run.transport.liveness == RuntimeRunTransportLiveness::Reachable;
    let active = matches!(
        snapshot.run.status,
        RuntimeRunStatus::Starting | RuntimeRunStatus::Running
    );

    if active && reachable {
        return Ok(());
    }

    let last_error_message = snapshot
        .run
        .last_error
        .as_ref()
        .map(|error| error.message.clone());

    match snapshot.run.status {
        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed => Err(CommandError::user_fixable(
            "runtime_stream_run_unavailable",
            last_error_message.unwrap_or_else(|| {
                format!(
                    "Cadence cannot attach a live runtime stream because run `{}` is already terminal.",
                    snapshot.run.run_id
                )
            }),
        )),
        RuntimeRunStatus::Starting | RuntimeRunStatus::Running | RuntimeRunStatus::Stale => {
            Err(CommandError::retryable(
                "runtime_stream_run_stale",
                last_error_message.unwrap_or_else(|| {
                    format!(
                        "Cadence cannot attach a live runtime stream because detached run `{}` is not currently reachable.",
                        snapshot.run.run_id
                    )
                }),
            ))
        }
    }
}

fn should_emit(
    requested_item_kinds: &[RuntimeStreamItemKind],
    kind: &RuntimeStreamItemKind,
) -> bool {
    if *kind == RuntimeStreamItemKind::Failure {
        return true;
    }

    requested_item_kinds
        .iter()
        .any(|requested| requested == kind)
}

fn emit_item_if_requested(
    channel: &Channel<RuntimeStreamItemDto>,
    request: &RuntimeStreamRequest,
    lease: &RuntimeStreamLease,
    item: RuntimeStreamItemDto,
) -> StreamResult<()> {
    ensure_stream_active(lease)?;

    if !should_emit(&request.requested_item_kinds, &item.kind) {
        return Ok(());
    }

    validate_stream_item(&item).map_err(|error| {
        StreamExit::Failed(StreamFailure {
            last_sequence: item.sequence.saturating_sub(1),
            error,
        })
    })?;

    let sequence = item.sequence;
    channel.send(item).map_err(|error| {
        StreamExit::Failed(StreamFailure {
            last_sequence: sequence,
            error: CommandError::retryable(
                "runtime_stream_channel_closed",
                format!(
                    "Cadence could not deliver the runtime stream item because the desktop channel closed: {error}"
                ),
            ),
        })
    })
}

fn ensure_stream_active(lease: &RuntimeStreamLease) -> StreamResult<()> {
    if lease.is_cancelled() {
        Err(StreamExit::Cancelled)
    } else {
        Ok(())
    }
}

fn validate_stream_item(item: &RuntimeStreamItemDto) -> Result<(), CommandError> {
    require_non_empty(Some(item.run_id.as_str()), "runId", "runtime stream item")?;

    if item.sequence == 0 {
        return Err(CommandError::system_fault(
            "runtime_stream_item_invalid",
            "Cadence produced a runtime stream item without a positive `sequence` value.",
        ));
    }

    match item.kind {
        RuntimeStreamItemKind::Transcript => {
            require_non_empty(item.text.as_deref(), "text", "runtime transcript item")?
        }
        RuntimeStreamItemKind::Tool => {
            require_non_empty(
                item.tool_call_id.as_deref(),
                "toolCallId",
                "runtime tool item",
            )?;
            require_non_empty(item.tool_name.as_deref(), "toolName", "runtime tool item")?;
            if item.tool_state.is_none() {
                return Err(CommandError::system_fault(
                    "runtime_stream_item_invalid",
                    "Cadence produced a runtime tool item without a tool state.",
                ));
            }
        }
        RuntimeStreamItemKind::Activity => {
            require_non_empty(item.code.as_deref(), "code", "runtime activity item")?;
            require_non_empty(item.title.as_deref(), "title", "runtime activity item")?;
        }
        RuntimeStreamItemKind::ActionRequired => {
            require_non_empty(
                item.action_id.as_deref(),
                "actionId",
                "runtime action-required item",
            )?;
            require_non_empty(
                item.action_type.as_deref(),
                "actionType",
                "runtime action-required item",
            )?;
            require_non_empty(
                item.title.as_deref(),
                "title",
                "runtime action-required item",
            )?;
            require_non_empty(
                item.detail.as_deref(),
                "detail",
                "runtime action-required item",
            )?;
        }
        RuntimeStreamItemKind::Complete => {
            require_non_empty(item.detail.as_deref(), "detail", "runtime completion item")?;
        }
        RuntimeStreamItemKind::Failure => {
            require_non_empty(item.code.as_deref(), "code", "runtime failure item")?;
            require_non_empty(item.message.as_deref(), "message", "runtime failure item")?;
            if item.retryable.is_none() {
                return Err(CommandError::system_fault(
                    "runtime_stream_item_invalid",
                    "Cadence produced a runtime failure item without a retryable flag.",
                ));
            }
        }
    }

    Ok(())
}

fn require_non_empty(value: Option<&str>, field: &str, kind: &str) -> Result<(), CommandError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(_) => Ok(()),
        None => Err(CommandError::system_fault(
            "runtime_stream_item_invalid",
            format!("Cadence produced a {kind} without a non-empty `{field}` field."),
        )),
    }
}

fn parse_runtime_boundary_id_for_run(
    action_id: &str,
    run_id: &str,
) -> Result<Option<String>, CommandError> {
    if !action_id.contains(":run:") || !action_id.contains(":boundary:") {
        return Ok(None);
    }

    let run_marker = format!(":run:{}:boundary:", run_id.trim());
    if !action_id.contains(&run_marker) {
        return Ok(None);
    }

    let Some(boundary_start) = action_id.find(&run_marker) else {
        return Err(CommandError::system_fault(
            "runtime_stream_item_invalid",
            format!(
                "Cadence could not parse runtime action-required id `{action_id}` for run `{run_id}`."
            ),
        ));
    };

    let boundary_and_action = &action_id[boundary_start + run_marker.len()..];
    let Some((boundary_id, _action_type)) = boundary_and_action.split_once(':') else {
        return Err(CommandError::system_fault(
            "runtime_stream_item_invalid",
            format!(
                "Cadence could not parse runtime boundary id from action-required id `{action_id}`."
            ),
        ));
    };

    let boundary_id = boundary_id.trim();
    if boundary_id.is_empty() {
        return Err(CommandError::system_fault(
            "runtime_stream_item_invalid",
            format!(
                "Cadence could not parse a non-empty runtime boundary id from action-required id `{action_id}`."
            ),
        ));
    }

    Ok(Some(boundary_id.to_string()))
}

fn runtime_auth_lost_error(phase: &RuntimeAuthPhase) -> CommandError {
    let (code, retryable, message) = match phase {
        RuntimeAuthPhase::Starting
        | RuntimeAuthPhase::AwaitingBrowserCallback
        | RuntimeAuthPhase::AwaitingManualInput
        | RuntimeAuthPhase::ExchangingCode
        | RuntimeAuthPhase::Refreshing => (
            "runtime_stream_auth_transition",
            true,
            "Cadence paused the runtime stream because the selected project's auth session is transitioning.",
        ),
        RuntimeAuthPhase::Idle => (
            "runtime_stream_auth_required",
            false,
            "Cadence closed the runtime stream because the selected project is signed out.",
        ),
        RuntimeAuthPhase::Cancelled | RuntimeAuthPhase::Failed => (
            "runtime_stream_unavailable",
            false,
            "Cadence closed the runtime stream because the selected project's runtime session is unavailable.",
        ),
        RuntimeAuthPhase::Authenticated => (
            "runtime_stream_session_missing",
            true,
            "Cadence could not keep the runtime stream attached because the authenticated session became incomplete.",
        ),
    };

    if retryable {
        CommandError::retryable(code, message)
    } else {
        CommandError::user_fixable(code, message)
    }
}

fn action_required_item(
    request: &RuntimeStreamRequest,
    sequence: u64,
    action_required: PendingActionRequired,
) -> RuntimeStreamItemDto {
    RuntimeStreamItemDto {
        kind: RuntimeStreamItemKind::ActionRequired,
        run_id: request.run_id.clone(),
        sequence,
        session_id: Some(request.session_id.clone()),
        flow_id: request.flow_id.clone(),
        text: None,
        tool_call_id: None,
        tool_name: None,
        tool_state: None,
        tool_summary: None,
        action_id: Some(action_required.action_id),
        boundary_id: action_required.boundary_id,
        action_type: Some(action_required.action_type),
        title: Some(action_required.title),
        detail: Some(action_required.detail),
        code: None,
        message: None,
        retryable: None,
        created_at: action_required.created_at,
    }
}

fn complete_item(
    request: &RuntimeStreamRequest,
    sequence: u64,
    detail: String,
) -> RuntimeStreamItemDto {
    RuntimeStreamItemDto {
        kind: RuntimeStreamItemKind::Complete,
        run_id: request.run_id.clone(),
        sequence,
        session_id: Some(request.session_id.clone()),
        flow_id: request.flow_id.clone(),
        text: None,
        tool_call_id: None,
        tool_name: None,
        tool_state: None,
        tool_summary: None,
        action_id: None,
        boundary_id: None,
        action_type: None,
        title: None,
        detail: Some(detail),
        code: None,
        message: None,
        retryable: None,
        created_at: crate::auth::now_timestamp(),
    }
}

fn failure_item(
    request: &RuntimeStreamRequest,
    sequence: u64,
    error: CommandError,
) -> RuntimeStreamItemDto {
    RuntimeStreamItemDto {
        kind: RuntimeStreamItemKind::Failure,
        run_id: request.run_id.clone(),
        sequence,
        session_id: Some(request.session_id.clone()),
        flow_id: request.flow_id.clone(),
        text: None,
        tool_call_id: None,
        tool_name: None,
        tool_state: None,
        tool_summary: None,
        action_id: None,
        boundary_id: None,
        action_type: None,
        title: Some("Runtime stream failed".into()),
        detail: None,
        code: Some(error.code),
        message: Some(error.message),
        retryable: Some(error.retryable),
        created_at: crate::auth::now_timestamp(),
    }
}

fn emit_failure_item(
    channel: &Channel<RuntimeStreamItemDto>,
    request: &RuntimeStreamRequest,
    sequence: u64,
    error: CommandError,
) -> Result<(), CommandError> {
    let item = failure_item(request, sequence, error);
    validate_stream_item(&item)?;
    channel.send(item).map_err(|send_error| {
        CommandError::retryable(
            "runtime_stream_channel_closed",
            format!(
                "Cadence could not deliver the runtime failure item because the desktop channel closed: {send_error}"
            ),
        )
    })
}

enum ReadSupervisorResponseError {
    Timeout,
    Io(std::io::Error),
    Decode(String),
}

fn read_supervisor_response(
    reader: &mut BufReader<TcpStream>,
) -> Result<Option<SupervisorControlResponse>, ReadSupervisorResponseError> {
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => Ok(None),
        Ok(_) => {
            if line.len() > PROTOCOL_LINE_LIMIT {
                return Err(ReadSupervisorResponseError::Decode(
                    "line exceeded protocol limit".into(),
                ));
            }

            serde_json::from_str(line.trim())
                .map(Some)
                .map_err(|error| ReadSupervisorResponseError::Decode(error.to_string()))
        }
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) =>
        {
            Err(ReadSupervisorResponseError::Timeout)
        }
        Err(error) => Err(ReadSupervisorResponseError::Io(error)),
    }
}

fn write_json_line<W: Write>(
    writer: &mut W,
    value: &SupervisorControlRequest,
) -> Result<(), std::io::Error> {
    serde_json::to_writer(&mut *writer, value).map_err(std::io::Error::other)?;
    writer.write_all(b"\n")?;
    writer.flush()
}
