use std::str::FromStr;

use tauri::{
    ipc::{Channel, JavaScriptChannelId},
    AppHandle, Runtime, State, Webview,
};

use crate::{
    commands::{
        validate_non_empty, CommandError, CommandResult, RuntimeAuthPhase, RuntimeStreamItemDto,
        RuntimeStreamItemKind, SubscribeRuntimeStreamRequestDto, SubscribeRuntimeStreamResponseDto,
    },
    db::project_store::{RuntimeRunSnapshotRecord, RuntimeRunStatus, RuntimeRunTransportLiveness},
    runtime::{start_runtime_stream, RuntimeStreamRequest},
    state::DesktopState,
};

use super::{
    get_runtime_session::reconcile_runtime_session,
    runtime_support::{
        load_persisted_runtime_run, load_runtime_session_status, resolve_project_root,
    },
};

#[tauri::command]
pub fn subscribe_runtime_stream<R: Runtime>(
    app: AppHandle<R>,
    webview: Webview<R>,
    state: State<'_, DesktopState>,
    request: SubscribeRuntimeStreamRequestDto,
) -> CommandResult<SubscribeRuntimeStreamResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let item_kinds = parse_requested_item_kinds(&request.item_kinds)?;
    let channel = resolve_channel(&webview, request.channel.as_deref())?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let runtime = load_runtime_session_status(state.inner(), &repo_root, &request.project_id)?;
    let runtime = reconcile_runtime_session(&app, state.inner(), &repo_root, runtime)?;

    if runtime.phase != RuntimeAuthPhase::Authenticated {
        return Err(runtime_stream_precondition_error(
            &runtime.phase,
            runtime.last_error.as_ref(),
        ));
    }

    let Some(session_id) = runtime.session_id else {
        return Err(CommandError::system_fault(
            "runtime_stream_session_missing",
            "Cadence could not start a runtime stream because the authenticated runtime session did not include a session id.",
        ));
    };

    let runtime_run = load_persisted_runtime_run(&repo_root, &request.project_id)?
        .ok_or_else(|| {
            CommandError::retryable(
                "runtime_stream_run_unavailable",
                "Cadence cannot start a live runtime stream until the selected project has an attachable durable run.",
            )
        })?;
    ensure_attachable_runtime_run(&runtime_run)?;

    start_runtime_stream(
        app,
        state.inner().clone(),
        RuntimeStreamRequest {
            project_id: runtime.project_id.clone(),
            repo_root,
            session_id: session_id.clone(),
            flow_id: runtime.flow_id.clone(),
            runtime_kind: runtime.runtime_kind.clone(),
            run_id: runtime_run.run.run_id.clone(),
            requested_item_kinds: item_kinds.clone(),
        },
        channel,
    );

    Ok(SubscribeRuntimeStreamResponseDto {
        project_id: runtime.project_id,
        runtime_kind: runtime.runtime_kind,
        run_id: runtime_run.run.run_id,
        session_id,
        flow_id: runtime.flow_id,
        subscribed_item_kinds: item_kinds,
    })
}

fn parse_requested_item_kinds(item_kinds: &[String]) -> CommandResult<Vec<RuntimeStreamItemKind>> {
    if item_kinds.is_empty() {
        return Err(CommandError::user_fixable(
            "invalid_request",
            "Field `itemKinds` must contain at least one allowed runtime stream item kind.",
        ));
    }

    let mut parsed = Vec::with_capacity(item_kinds.len());
    for kind in item_kinds {
        let kind = parse_runtime_stream_item_kind(kind)?;
        if !parsed.contains(&kind) {
            parsed.push(kind);
        }
    }

    Ok(parsed)
}

fn parse_runtime_stream_item_kind(value: &str) -> CommandResult<RuntimeStreamItemKind> {
    match value {
        "transcript" => Ok(RuntimeStreamItemKind::Transcript),
        "tool" => Ok(RuntimeStreamItemKind::Tool),
        "activity" => Ok(RuntimeStreamItemKind::Activity),
        "action_required" => Ok(RuntimeStreamItemKind::ActionRequired),
        "complete" => Ok(RuntimeStreamItemKind::Complete),
        "failure" => Ok(RuntimeStreamItemKind::Failure),
        other => Err(CommandError::user_fixable(
            "runtime_stream_item_kind_unsupported",
            format!(
                "Cadence does not support runtime stream item kind `{other}`. Allowed kinds: {}.",
                RuntimeStreamItemDto::allowed_kind_names().join(", ")
            ),
        )),
    }
}

fn ensure_attachable_runtime_run(snapshot: &RuntimeRunSnapshotRecord) -> CommandResult<()> {
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
                    "Cadence cannot start a live runtime stream because run `{}` is already terminal.",
                    snapshot.run.run_id
                )
            }),
        )),
        RuntimeRunStatus::Starting | RuntimeRunStatus::Running | RuntimeRunStatus::Stale => {
            Err(CommandError::retryable(
                "runtime_stream_run_stale",
                last_error_message.unwrap_or_else(|| {
                    format!(
                        "Cadence cannot attach to run `{}` because the detached supervisor is not currently reachable.",
                        snapshot.run.run_id
                    )
                }),
            ))
        }
    }
}

fn resolve_channel<R: Runtime>(
    webview: &Webview<R>,
    raw_channel: Option<&str>,
) -> CommandResult<Channel<RuntimeStreamItemDto>> {
    let Some(raw_channel) = raw_channel else {
        return Err(CommandError::user_fixable(
            "runtime_stream_channel_missing",
            "Cadence requires a runtime stream channel before it can start streaming selected-project runtime items.",
        ));
    };

    let channel_id = JavaScriptChannelId::from_str(raw_channel).map_err(|_| {
        CommandError::user_fixable(
            "runtime_stream_channel_invalid",
            "Cadence received an invalid runtime stream channel handle from the desktop shell.",
        )
    })?;

    Ok(channel_id.channel_on(webview.clone()))
}

fn runtime_stream_precondition_error(
    phase: &RuntimeAuthPhase,
    diagnostic: Option<&crate::commands::RuntimeDiagnosticDto>,
) -> CommandError {
    let default_message = match phase {
        RuntimeAuthPhase::Idle => {
            "Cadence cannot start a runtime stream until the selected project has an authenticated runtime session."
        }
        RuntimeAuthPhase::Starting
        | RuntimeAuthPhase::AwaitingBrowserCallback
        | RuntimeAuthPhase::AwaitingManualInput
        | RuntimeAuthPhase::ExchangingCode
        | RuntimeAuthPhase::Refreshing => {
            "Cadence cannot start a runtime stream until the selected project's runtime session finishes its auth transition."
        }
        RuntimeAuthPhase::Cancelled | RuntimeAuthPhase::Failed => {
            "Cadence cannot start a runtime stream because the selected project's runtime session is unavailable."
        }
        RuntimeAuthPhase::Authenticated => {
            "Cadence cannot start a runtime stream because the selected project's runtime session is incomplete."
        }
    };

    let code = match phase {
        RuntimeAuthPhase::Idle => "runtime_stream_auth_required",
        RuntimeAuthPhase::Starting
        | RuntimeAuthPhase::AwaitingBrowserCallback
        | RuntimeAuthPhase::AwaitingManualInput
        | RuntimeAuthPhase::ExchangingCode
        | RuntimeAuthPhase::Refreshing => "runtime_stream_not_ready",
        RuntimeAuthPhase::Cancelled | RuntimeAuthPhase::Failed => "runtime_stream_unavailable",
        RuntimeAuthPhase::Authenticated => "runtime_stream_session_missing",
    };

    let retryable = matches!(
        phase,
        RuntimeAuthPhase::Starting
            | RuntimeAuthPhase::AwaitingBrowserCallback
            | RuntimeAuthPhase::AwaitingManualInput
            | RuntimeAuthPhase::ExchangingCode
            | RuntimeAuthPhase::Refreshing
    );

    let message = diagnostic
        .map(|diagnostic| diagnostic.message.clone())
        .unwrap_or_else(|| default_message.into());

    if retryable {
        CommandError::retryable(code, message)
    } else {
        CommandError::user_fixable(code, message)
    }
}
