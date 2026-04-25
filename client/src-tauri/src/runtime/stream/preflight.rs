use tauri::{AppHandle, Runtime};

use crate::{
    commands::{
        get_runtime_session::reconcile_runtime_session,
        runtime_support::{
            load_persisted_runtime_run, load_runtime_run_status, load_runtime_session_status,
        },
        CommandError, OperatorApprovalStatus, RuntimeAuthPhase,
    },
    db::project_store::{
        self, RuntimeRunSnapshotRecord, RuntimeRunStatus, RuntimeRunTransportLiveness,
    },
    state::DesktopState,
};

use super::{
    controller::RuntimeStreamRequest,
    items::{parse_runtime_boundary_id_for_run, require_non_empty, PendingActionRequired},
    StreamExit, StreamFailure, StreamResult, TERMINAL_SNAPSHOT_RETRY_ATTEMPTS,
    TERMINAL_SNAPSHOT_RETRY_INTERVAL,
};

pub(super) fn ensure_stream_identity<R: Runtime>(
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

pub(super) fn load_terminal_runtime_snapshot(
    state: &DesktopState,
    request: &RuntimeStreamRequest,
    last_sequence: u64,
) -> StreamResult<RuntimeRunSnapshotRecord> {
    let mut latest_snapshot: Option<RuntimeRunSnapshotRecord> = None;

    for attempt in 0..TERMINAL_SNAPSHOT_RETRY_ATTEMPTS {
        let snapshot = load_runtime_run_status(
            state,
            &request.repo_root,
            &request.project_id,
            &request.agent_session_id,
        )
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

pub(super) fn load_streamable_runtime_run(
    request: &RuntimeStreamRequest,
    last_sequence: u64,
) -> StreamResult<RuntimeRunSnapshotRecord> {
    let snapshot = load_persisted_runtime_run(
        &request.repo_root,
        &request.project_id,
        &request.agent_session_id,
    )
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

pub(super) fn load_pending_action_required(
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

pub(super) fn ensure_attachable_runtime_run(
    snapshot: &RuntimeRunSnapshotRecord,
) -> Result<(), CommandError> {
    let reachable = snapshot.run.transport.liveness == RuntimeRunTransportLiveness::Reachable;
    let active = matches!(
        snapshot.run.status,
        RuntimeRunStatus::Starting | RuntimeRunStatus::Running
    );

    if active && reachable {
        return Ok(());
    }

    if reachable
        && matches!(
            snapshot.run.status,
            RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
        )
    {
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
