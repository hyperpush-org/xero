use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime};
use xero_agent_core::{
    PermissionProfileSandbox, ProjectTrustState, SandboxApprovalSource, SandboxExecutionContext,
    SandboxExecutionMetadata, SandboxPlatform, SandboxedProcessRequest, SandboxedProcessRunner,
    ToolApprovalRequirement, ToolCallInput, ToolDescriptorV2, ToolEffectClass,
    ToolExecutionContext, ToolMutability, ToolSandbox, ToolSandboxRequirement,
};

use crate::{
    auth::now_timestamp,
    commands::{
        get_runtime_settings::runtime_settings_snapshot_for_provider_profile,
        provider_credentials::load_provider_credentials_view, AgentAttachmentKindDto, CommandError,
        CommandResult, RuntimeRunControlInputDto, StagedAgentAttachmentDto,
    },
    db::project_store::{
        self, AgentMessageAttachmentKind, AgentMessageRole, AgentRunDiagnosticRecord,
        AgentRunEventKind, AgentRunStatus, AgentToolCallFinishRecord, AgentToolCallStartRecord,
        AgentToolCallState, NewAgentEventRecord, NewAgentFileChangeRecord, NewAgentMessageRecord,
        NewAgentRunRecord, RuntimeRunSnapshotRecord, RuntimeRunStatus,
    },
    runtime::{
        agent_core::publish_committed_agent_event, CURSOR_PROVIDER_ID, OWNED_AGENT_RUNTIME_KIND,
    },
    state::DesktopState,
};

use super::run::{
    clear_owned_runtime_prompt_if_current, complete_owned_runtime_run, emit_owned_runtime_progress,
    emit_runtime_run_updated, ensure_owned_runtime_provider_turn_capabilities,
    fail_owned_runtime_run, runtime_control_input_from_active, runtime_run_dto_from_snapshot,
    OwnedRuntimePromptStart,
};

const CURSOR_SIDECAR_BINARY_NAME: &str = "xero-cursor-sidecar";
const CURSOR_SIDECAR_PATH_ENV: &str = "XERO_CURSOR_SIDECAR_PATH";
const CURSOR_BRIDGE_PATH_ENV: &str = "XERO_CURSOR_BRIDGE_PATH";
const CURSOR_DEFAULT_TIMEOUT_MS: u64 = 30 * 60 * 1000;

pub(crate) struct CursorRuntimeDriveError {
    pub(crate) repo_root: PathBuf,
    pub(crate) snapshot: RuntimeRunSnapshotRecord,
    pub(crate) error: CommandError,
}

pub(crate) fn is_cursor_runtime_provider(provider_id: &str) -> bool {
    provider_id == CURSOR_PROVIDER_ID
}

pub(crate) fn bootstrap_and_drive_cursor_runtime_prompt<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    task: OwnedRuntimePromptStart,
    runtime_snapshot: RuntimeRunSnapshotRecord,
) -> Result<(), Box<CursorRuntimeDriveError>> {
    let repo_root = task.repo_root.clone();
    let snapshot = runtime_snapshot.clone();
    match bootstrap_and_drive_cursor_runtime_prompt_inner(
        app,
        state,
        task,
        runtime_snapshot.clone(),
    ) {
        Ok(()) => Ok(()),
        Err(error) => Err(Box::new(CursorRuntimeDriveError {
            repo_root,
            snapshot,
            error,
        })),
    }
}

fn bootstrap_and_drive_cursor_runtime_prompt_inner<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    task: OwnedRuntimePromptStart,
    mut runtime_snapshot: RuntimeRunSnapshotRecord,
) -> CommandResult<()> {
    let controls = runtime_control_input_from_active(&task.run_controls.active);
    ensure_cursor_turn_capabilities(
        app,
        state,
        &task.provider_profile_id,
        &controls,
        &task.attachments,
    )?;
    runtime_snapshot = emit_owned_runtime_progress(
        app,
        &task.repo_root,
        &runtime_snapshot,
        RuntimeRunStatus::Starting,
        None,
        "Preparing Cursor sidecar.",
    )?;
    drive_cursor_sidecar_turn(
        app,
        state,
        &task.repo_root,
        &runtime_snapshot,
        task.run_controls
            .pending
            .as_ref()
            .and_then(|pending| pending.queued_prompt_continuation_request_id.as_deref())
            .unwrap_or("runtime-start"),
        task.prompt,
        task.attachments,
        controls,
    )
}

pub(crate) fn drive_cursor_runtime_prompt<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    continuation_request_id: &str,
    prompt: String,
    attachments: Vec<StagedAgentAttachmentDto>,
) -> CommandResult<()> {
    let controls = runtime_run_controls_as_input(snapshot);
    let profile_id = controls
        .provider_profile_id
        .as_deref()
        .map(str::trim)
        .filter(|profile_id| !profile_id.is_empty())
        .unwrap_or(CURSOR_PROVIDER_ID)
        .to_owned();
    ensure_cursor_turn_capabilities(app, state, &profile_id, &controls, &attachments)?;
    drive_cursor_sidecar_turn(
        app,
        state,
        repo_root,
        snapshot,
        continuation_request_id,
        prompt,
        attachments,
        controls,
    )
}

fn ensure_cursor_turn_capabilities<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile_id: &str,
    controls: &RuntimeRunControlInputDto,
    attachments: &[StagedAgentAttachmentDto],
) -> CommandResult<()> {
    for attachment in attachments {
        if attachment.kind != AgentAttachmentKindDto::Image {
            return Err(CommandError::user_fixable(
                "cursor_attachment_unsupported",
                "Cursor SDK currently accepts image attachments from Xero. Remove document/text file attachments or choose another provider with file-input support.",
            ));
        }
    }
    ensure_owned_runtime_provider_turn_capabilities(
        app,
        state,
        state.owned_agent_provider_config_override().is_none(),
        profile_id,
        CURSOR_PROVIDER_ID,
        &controls.model_id,
        attachments,
    )?;
    Ok(())
}

fn drive_cursor_sidecar_turn<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    continuation_request_id: &str,
    prompt: String,
    attachments: Vec<StagedAgentAttachmentDto>,
    controls: RuntimeRunControlInputDto,
) -> CommandResult<()> {
    let provider = cursor_provider_settings(app, state, &controls)?;
    let sidecar_path = resolve_cursor_sidecar_binary()?;
    let bridge_path = resolve_cursor_bridge_path()?;
    let app_data_dir = state.app_data_dir(app)?;
    let run_dir = app_data_dir
        .join("cursor-sdk")
        .join("runs")
        .join(safe_path_segment(&snapshot.run.run_id));
    fs::create_dir_all(&run_dir).map_err(|error| {
        CommandError::system_fault(
            "cursor_sidecar_state_prepare_failed",
            format!("Xero could not prepare Cursor sidecar state: {error}"),
        )
    })?;
    let prompt_file = write_private_file(&run_dir.join("prompt.txt"), prompt.as_bytes())?;
    let attachments_file = if attachments.is_empty() {
        None
    } else {
        let payload = serde_json::to_vec(&attachments).map_err(|error| {
            CommandError::system_fault(
                "cursor_attachment_payload_encode_failed",
                format!("Xero could not encode Cursor attachments: {error}"),
            )
        })?;
        Some(write_private_file(
            &run_dir.join("attachments.json"),
            &payload,
        )?)
    };
    let api_key_file =
        write_private_file(&run_dir.join("cursor-api-key"), provider.api_key.as_bytes())?;

    ensure_cursor_agent_run(repo_root, snapshot, &prompt, &controls, &provider.model_id)?;
    let lease = state.agent_run_supervisor().begin_persisted(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.agent_session_id,
        &snapshot.run.run_id,
    )?;
    let continuation = prepare_cursor_user_turn(
        repo_root,
        snapshot,
        continuation_request_id,
        &prompt,
        &attachments,
    )?;
    let continuation_request =
        if continuation.request.state == project_store::AgentContinuationRequestState::Driving {
            project_store::reconcile_completed_agent_continuation(
                repo_root,
                &snapshot.run.project_id,
                &snapshot.run.run_id,
                continuation_request_id,
                &now_timestamp(),
            )?
            .ok_or_else(|| {
                CommandError::system_fault(
                "agent_continuation_request_missing",
                "The prepared Cursor continuation disappeared during completion reconciliation.",
            )
            })?
        } else {
            continuation.request
        };
    let mut prepared;
    match continuation_request.state {
        project_store::AgentContinuationRequestState::Consumed => {
            let current = clear_owned_runtime_prompt_if_current(
                repo_root,
                snapshot,
                continuation_request_id,
                snapshot.run.status.clone(),
                "Cursor prompt already completed.",
            )?;
            let runtime_run = runtime_run_dto_from_snapshot(&current);
            emit_runtime_run_updated(app, Some(&runtime_run))?;
            drop(lease);
            return Ok(());
        }
        project_store::AgentContinuationRequestState::Driving => {
            drop(lease);
            return Err(CommandError::user_fixable(
                "agent_continuation_recovery_required",
                "Cursor may already have received this prompt. Xero kept it pending for explicit reconciliation instead of replaying it.",
            ));
        }
        project_store::AgentContinuationRequestState::Prepared => {
            prepared = continuation.snapshot;
            if matches!(
                prepared.run.status,
                AgentRunStatus::Paused | AgentRunStatus::Completed | AgentRunStatus::Failed
            ) {
                prepared = project_store::reopen_agent_run_for_continuation(
                    repo_root,
                    &snapshot.run.project_id,
                    &snapshot.run.run_id,
                    &now_timestamp(),
                )?;
            }
        }
    }
    let runtime_snapshot = emit_owned_runtime_progress(
        app,
        repo_root,
        snapshot,
        RuntimeRunStatus::Running,
        None,
        "Starting Cursor sidecar.",
    )?;
    let runtime_run = runtime_run_dto_from_snapshot(&runtime_snapshot);
    emit_runtime_run_updated(app, Some(&runtime_run))?;

    let cursor_agent_id = latest_cursor_agent_id(&prepared);
    let mcp_mode = cursor_mcp_mode(&controls);
    let sandbox_metadata =
        cursor_sidecar_sandbox_metadata(repo_root, &app_data_dir, &sidecar_path)?;
    let mut argv = vec![
        sidecar_path.display().to_string(),
        "run".into(),
        "--repo-root".into(),
        repo_root.display().to_string(),
        "--project-id".into(),
        snapshot.run.project_id.clone(),
        "--run-id".into(),
        snapshot.run.run_id.clone(),
        "--session-id".into(),
        snapshot.run.agent_session_id.clone(),
        "--runtime-agent-id".into(),
        controls.runtime_agent_id.as_str().into(),
        "--model".into(),
        provider.model_id.clone(),
        "--prompt-file".into(),
        prompt_file.display().to_string(),
        "--api-key-file".into(),
        api_key_file.display().to_string(),
        "--state-dir".into(),
        app_data_dir.display().to_string(),
        "--bridge-path".into(),
        bridge_path.display().to_string(),
        "--mcp-mode".into(),
        mcp_mode.into(),
        "--timeout-ms".into(),
        CURSOR_DEFAULT_TIMEOUT_MS.to_string(),
    ];
    if let Some(path) = attachments_file.as_ref() {
        argv.push("--attachments-json-file".into());
        argv.push(path.display().to_string());
    }
    if let Some(cursor_agent_id) = cursor_agent_id {
        argv.push("--cursor-agent-id".into());
        argv.push(cursor_agent_id);
    }

    match project_store::mark_agent_continuation_drive_started(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.run_id,
        continuation_request_id,
        &now_timestamp(),
    )? {
        project_store::AgentContinuationDriveStartResult::Started(_) => {}
        project_store::AgentContinuationDriveStartResult::AlreadyDriving(_) => {
            drop(lease);
            return Err(CommandError::user_fixable(
                "agent_continuation_outcome_unknown",
                "Cursor may already have received this prompt. Xero will not replay it without reconciliation evidence.",
            ));
        }
        project_store::AgentContinuationDriveStartResult::Consumed(_) => {
            drop(lease);
            return Ok(());
        }
        project_store::AgentContinuationDriveStartResult::Missing => {
            drop(lease);
            return Err(CommandError::system_fault(
                "agent_continuation_request_missing",
                "The prepared Cursor continuation disappeared before sidecar dispatch.",
            ));
        }
    }

    let output = SandboxedProcessRunner::new().run(
        SandboxedProcessRequest {
            argv,
            cwd: Some(repo_root.display().to_string()),
            timeout_ms: Some(CURSOR_DEFAULT_TIMEOUT_MS),
            stdout_limit_bytes: 8 * 1024 * 1024,
            stderr_limit_bytes: 1024 * 1024,
            metadata: sandbox_metadata,
        },
        || lease.token().is_cancelled(),
    );
    let _ = fs::remove_file(&api_key_file);

    let result = match output {
        Ok(output) => {
            let mut report = CursorSidecarReport::default();
            if let Some(stderr) = output
                .stderr
                .as_deref()
                .filter(|stderr| !stderr.trim().is_empty())
            {
                append_agent_event(
                    repo_root,
                    snapshot,
                    AgentRunEventKind::CommandOutput,
                    json!({
                        "stream": "cursor_sidecar_stderr",
                        "text": truncate_bytes(stderr, 64 * 1024),
                    }),
                )?;
            }
            for line in output
                .stdout
                .unwrap_or_default()
                .lines()
                .filter(|line| !line.trim().is_empty())
            {
                process_cursor_sidecar_line(repo_root, snapshot, &mut report, line)?;
            }
            if output.exit_code != Some(0) && report.failure.is_none() {
                report.failure = Some((
                    "cursor_sidecar_exit_failed".into(),
                    format!("Cursor sidecar exited with status {:?}.", output.exit_code),
                ));
            }
            finalize_cursor_agent_run(app, repo_root, snapshot, report)
        }
        Err(error) => {
            let code = if error.code == "sandboxed_process_cancelled" {
                "cursor_run_cancelled".to_string()
            } else {
                "cursor_sidecar_failed".to_string()
            };
            let message = error.message;
            let report = CursorSidecarReport {
                cancelled: code == "cursor_run_cancelled",
                failure: Some((code, message)),
                ..CursorSidecarReport::default()
            };
            finalize_cursor_agent_run(app, repo_root, snapshot, report)
        }
    };
    let finish = if result.is_ok() {
        project_store::finish_agent_continuation_drive(
            repo_root,
            &snapshot.run.project_id,
            &snapshot.run.run_id,
            continuation_request_id,
            true,
            &now_timestamp(),
        )
    } else {
        // The sidecar process may have observed the prompt. Keep Driving until durable
        // reconciliation proves whether replay is safe.
        Ok(None)
    };
    drop(lease);
    let outcome = match (result, finish) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(_)) => Ok(()),
    };
    if outcome.is_ok() {
        let latest = project_store::load_runtime_run(
            repo_root,
            &snapshot.run.project_id,
            &snapshot.run.agent_session_id,
        )?
        .unwrap_or(runtime_snapshot);
        let cleared = clear_owned_runtime_prompt_if_current(
            repo_root,
            &latest,
            continuation_request_id,
            latest.run.status.clone(),
            "Cursor accepted queued prompt.",
        )?;
        let runtime_run = runtime_run_dto_from_snapshot(&cleared);
        emit_runtime_run_updated(app, Some(&runtime_run))?;
    }
    outcome
}

#[derive(Clone)]
struct CursorProviderSettings {
    model_id: String,
    api_key: String,
}

fn cursor_provider_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    controls: &RuntimeRunControlInputDto,
) -> CommandResult<CursorProviderSettings> {
    let view = load_provider_credentials_view(app, state)?;
    let requested_profile_id = controls
        .provider_profile_id
        .as_deref()
        .map(str::trim)
        .filter(|profile_id| !profile_id.is_empty());
    let profile = match requested_profile_id {
        Some(profile_id) => view.profile(profile_id).ok_or_else(|| {
            CommandError::user_fixable(
                "provider_not_found",
                format!("Cursor provider profile `{profile_id}` is missing."),
            )
        })?,
        None => view.active_profile().ok_or_else(|| {
            CommandError::user_fixable(
                "provider_credentials_invalid",
                "Xero could not resolve the selected Cursor provider profile.",
            )
        })?,
    };
    if profile.provider_id != CURSOR_PROVIDER_ID {
        return Err(CommandError::user_fixable(
            "cursor_provider_profile_required",
            "Cursor runtime runs require a Cursor provider profile.",
        ));
    }
    let settings = runtime_settings_snapshot_for_provider_profile(&view, profile)?;
    let api_key = settings.provider_api_key.clone().ok_or_else(|| {
        CommandError::user_fixable(
            "cursor_auth_missing",
            "Xero cannot start Cursor because no Cursor API key is configured.",
        )
    })?;
    Ok(CursorProviderSettings {
        model_id: controls.model_id.clone(),
        api_key,
    })
}

fn ensure_cursor_agent_run(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    prompt: &str,
    controls: &RuntimeRunControlInputDto,
    model_id: &str,
) -> CommandResult<project_store::AgentRunSnapshotRecord> {
    let now = now_timestamp();
    match project_store::load_agent_run(repo_root, &snapshot.run.project_id, &snapshot.run.run_id) {
        Ok(existing) => {
            if existing.run.provider_id != CURSOR_PROVIDER_ID {
                return Err(CommandError::user_fixable(
                    "cursor_run_provider_mismatch",
                    "Xero cannot continue this run with Cursor because the persisted agent run belongs to a different provider.",
                ));
            }
            Ok(existing)
        }
        Err(error) if error.code == "agent_run_not_found" => {
            let system_prompt = "Cursor SDK via Xero Cursor sidecar. Cursor-specific mechanics are trace detail; routine user experience should remain provider-like.".to_string();
            project_store::insert_agent_run(
                repo_root,
                &NewAgentRunRecord {
                    runtime_agent_id: controls.runtime_agent_id,
                    agent_definition_id: controls.agent_definition_id.clone(),
                    agent_definition_version: controls.agent_definition_version,
                    project_id: snapshot.run.project_id.clone(),
                    agent_session_id: snapshot.run.agent_session_id.clone(),
                    run_id: snapshot.run.run_id.clone(),
                    provider_id: CURSOR_PROVIDER_ID.into(),
                    model_id: model_id.into(),
                    prompt: prompt.into(),
                    system_prompt: system_prompt.clone(),
                    now: now.clone(),
                },
            )?;
            project_store::append_agent_message(
                repo_root,
                &NewAgentMessageRecord {
                    project_id: snapshot.run.project_id.clone(),
                    run_id: snapshot.run.run_id.clone(),
                    role: AgentMessageRole::System,
                    content: system_prompt,
                    provider_metadata_json: None,
                    created_at: now.clone(),
                    attachments: Vec::new(),
                },
            )?;
            append_agent_event(
                repo_root,
                snapshot,
                AgentRunEventKind::RunStarted,
                json!({
                    "status": "starting",
                    "providerId": CURSOR_PROVIDER_ID,
                    "modelId": model_id,
                    "execution": "cursor_sidecar",
                    "runtimeKind": OWNED_AGENT_RUNTIME_KIND,
                }),
            )?;
            project_store::update_agent_run_status(
                repo_root,
                &snapshot.run.project_id,
                &snapshot.run.run_id,
                AgentRunStatus::Running,
                None,
                &now,
            )
        }
        Err(error) => Err(error),
    }
}

fn prepare_cursor_user_turn(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    continuation_request_id: &str,
    prompt: &str,
    attachments: &[StagedAgentAttachmentDto],
) -> CommandResult<project_store::AgentContinuationPreparationResult> {
    let attachment_inputs = attachments
        .iter()
        .map(|attachment| project_store::NewMessageAttachmentInput {
            kind: match attachment.kind {
                AgentAttachmentKindDto::Image => AgentMessageAttachmentKind::Image,
                AgentAttachmentKindDto::Document => AgentMessageAttachmentKind::Document,
                AgentAttachmentKindDto::Text => AgentMessageAttachmentKind::Text,
            },
            storage_path: attachment.absolute_path.clone(),
            media_type: attachment.media_type.clone(),
            original_name: attachment.original_name.clone(),
            size_bytes: attachment.size_bytes,
            width: attachment.width,
            height: attachment.height,
        })
        .collect::<Vec<_>>();
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    hasher.update([0]);
    hasher.update(serde_json::to_vec(attachments).map_err(|error| {
        CommandError::system_fault(
            "cursor_continuation_hash_failed",
            format!("Xero could not encode the Cursor continuation identity: {error}"),
        )
    })?);
    let payload_hash = format!("{:x}", hasher.finalize());
    let recovery_payload_json = serde_json::to_string(&json!({
        "schema": "xero.cursor_continuation_recovery.v1",
        "projectId": snapshot.run.project_id,
        "runId": snapshot.run.run_id,
        "continuationRequestId": continuation_request_id,
        "prompt": prompt,
        "attachments": attachments,
    }))
    .map_err(|error| {
        CommandError::system_fault(
            "cursor_continuation_recovery_encode_failed",
            format!("Xero could not encode the Cursor recovery payload: {error}"),
        )
    })?;
    let now = now_timestamp();
    project_store::prepare_agent_continuation(
        repo_root,
        &project_store::NewAgentContinuationPreparationRecord {
            project_id: snapshot.run.project_id.clone(),
            request_id: continuation_request_id.to_owned(),
            run_id: snapshot.run.run_id.clone(),
            payload_hash,
            recovery_payload_json,
            role: AgentMessageRole::User,
            content: prompt.to_owned(),
            attachments: attachment_inputs,
            linked_path_grant_payload_json: None,
            message_payload_json: serde_json::to_string(&json!({
                "role": "user",
                "text": prompt,
            }))
            .map_err(|error| {
                CommandError::system_fault(
                    "cursor_continuation_event_encode_failed",
                    format!("Xero could not encode the Cursor continuation event: {error}"),
                )
            })?,
            action_answer: None,
            prepared_at: now,
        },
    )
}

#[derive(Default)]
struct CursorSidecarReport {
    assistant_text: String,
    failure: Option<(String, String)>,
    cancelled: bool,
    cursor_agent_id: Option<String>,
    cursor_run_id: Option<String>,
    requested_model_route: Option<String>,
    requested_model_id: Option<String>,
    resolved_model: Option<String>,
}

fn process_cursor_sidecar_line(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    report: &mut CursorSidecarReport,
    line: &str,
) -> CommandResult<()> {
    let event = match serde_json::from_str::<JsonValue>(line) {
        Ok(event) => event,
        Err(error) => {
            report.failure.get_or_insert((
                "cursor_sidecar_invalid_jsonl".into(),
                format!("Cursor sidecar emitted invalid JSONL: {error}"),
            ));
            return Ok(());
        }
    };
    let event_type = event
        .get("type")
        .and_then(JsonValue::as_str)
        .unwrap_or("unknown");
    if let Some(agent_id) = event.get("cursorAgentId").and_then(JsonValue::as_str) {
        report.cursor_agent_id = Some(agent_id.to_owned());
    }
    if let Some(run_id) = event.get("cursorRunId").and_then(JsonValue::as_str) {
        report.cursor_run_id = Some(run_id.to_owned());
    }
    if let Some(route) = event.get("requestedModelRoute").and_then(JsonValue::as_str) {
        report.requested_model_route = Some(route.to_owned());
    }
    if let Some(model_id) = event.get("requestedModelId").and_then(JsonValue::as_str) {
        report.requested_model_id = Some(model_id.to_owned());
    }
    if let Some(model) = event.get("resolvedModel").and_then(JsonValue::as_str) {
        report.resolved_model = Some(model.to_owned());
    }

    match event_type {
        "delta" => {
            if let Some(text) = event.get("text").and_then(JsonValue::as_str) {
                if !text.is_empty() {
                    report.assistant_text.push_str(text);
                    append_agent_event(
                        repo_root,
                        snapshot,
                        AgentRunEventKind::MessageDelta,
                        json!({
                            "role": "assistant",
                            "text": text,
                            "provenance": cursor_provenance(),
                        }),
                    )?;
                }
            }
        }
        "failed" | "sidecar_failed" => {
            let code = event
                .get("code")
                .and_then(JsonValue::as_str)
                .unwrap_or("cursor_sidecar_failed")
                .to_owned();
            let message = event
                .get("message")
                .and_then(JsonValue::as_str)
                .unwrap_or("Cursor sidecar failed.")
                .to_owned();
            report.failure = Some((code, message));
        }
        "agent_event" => persist_sidecar_agent_event(repo_root, snapshot, &event)?,
        "sidecar_command_output" => {
            append_agent_event(
                repo_root,
                snapshot,
                AgentRunEventKind::CommandOutput,
                event.get("payload").cloned().unwrap_or_else(|| {
                    json!({
                        "stream": event.get("stream").cloned().unwrap_or_else(|| json!("cursor_sidecar")),
                        "text": event.get("text").cloned().unwrap_or_else(|| json!("")),
                    })
                }),
            )?;
        }
        _ => {}
    }

    append_agent_event(
        repo_root,
        snapshot,
        AgentRunEventKind::StateTransition,
        json!({
            "kind": "cursor_sidecar_event",
            "cursorEventKind": event_type,
            "event": truncate_json_event(event),
            "provenance": cursor_provenance(),
        }),
    )?;
    Ok(())
}

fn persist_sidecar_agent_event(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    event: &JsonValue,
) -> CommandResult<()> {
    let kind = event
        .get("eventKind")
        .and_then(JsonValue::as_str)
        .and_then(sidecar_event_kind)
        .unwrap_or(AgentRunEventKind::StateTransition);
    let payload = event.get("payload").cloned().unwrap_or_else(|| json!({}));
    if kind == AgentRunEventKind::ToolStarted {
        let tool_call_id = payload
            .get("toolCallId")
            .and_then(JsonValue::as_str)
            .unwrap_or("cursor-tool-call")
            .to_string();
        let tool_name = payload
            .get("toolName")
            .and_then(JsonValue::as_str)
            .unwrap_or("unknown")
            .to_string();
        let input_json = payload
            .get("input")
            .cloned()
            .unwrap_or_else(|| json!({}))
            .to_string();
        let _ = project_store::start_agent_tool_call(
            repo_root,
            &AgentToolCallStartRecord {
                project_id: snapshot.run.project_id.clone(),
                run_id: snapshot.run.run_id.clone(),
                tool_call_id,
                tool_name,
                input_json,
                started_at: now_timestamp(),
            },
        );
    } else if kind == AgentRunEventKind::ToolCompleted {
        if let Some(tool_call_id) = payload.get("toolCallId").and_then(JsonValue::as_str) {
            let ok = payload
                .get("ok")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false);
            let error = (!ok).then(|| AgentRunDiagnosticRecord {
                code: payload
                    .get("code")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("cursor_tool_failed")
                    .to_string(),
                message: payload
                    .get("message")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("Cursor MCP tool failed.")
                    .to_string(),
            });
            let _ = project_store::finish_agent_tool_call(
                repo_root,
                &AgentToolCallFinishRecord {
                    project_id: snapshot.run.project_id.clone(),
                    run_id: snapshot.run.run_id.clone(),
                    tool_call_id: tool_call_id.to_string(),
                    state: if ok {
                        AgentToolCallState::Succeeded
                    } else {
                        AgentToolCallState::Failed
                    },
                    result_json: ok.then(|| payload.to_string()),
                    error,
                    completed_at: now_timestamp(),
                },
            );
        }
    } else if kind == AgentRunEventKind::FileChanged {
        let path = payload
            .get("path")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                CommandError::system_fault(
                    "cursor_file_change_path_missing",
                    "Cursor sidecar emitted a file-changed event without a path.",
                )
            })?
            .to_string();
        let operation = payload
            .get("operation")
            .and_then(JsonValue::as_str)
            .unwrap_or("write")
            .to_string();
        let mut touched_paths = vec![path.clone()];
        if let Some(to_path) = payload.get("toPath").and_then(JsonValue::as_str) {
            touched_paths.push(to_path.to_string());
        }
        let recorded_at = now_timestamp();
        let (_, stored_event) = project_store::append_agent_file_change_with_event(
            repo_root,
            &NewAgentFileChangeRecord {
                project_id: snapshot.run.project_id.clone(),
                run_id: snapshot.run.run_id.clone(),
                change_group_id: None,
                path,
                operation,
                old_hash: None,
                new_hash: None,
                created_at: recorded_at.clone(),
            },
            |stored_change| {
                project_store::refresh_project_record_freshness_for_paths(
                    repo_root,
                    &snapshot.run.project_id,
                    &touched_paths,
                    &recorded_at,
                )?;
                project_store::refresh_agent_memory_freshness_for_paths(
                    repo_root,
                    &snapshot.run.project_id,
                    &touched_paths,
                    &recorded_at,
                )?;
                let mut event_payload = payload;
                if let Some(object) = event_payload.as_object_mut() {
                    object.insert("projectId".into(), json!(snapshot.run.project_id.clone()));
                    object.insert("traceId".into(), json!(stored_change.trace_id.clone()));
                    object.insert(
                        "topLevelRunId".into(),
                        json!(stored_change.top_level_run_id.clone()),
                    );
                    object.insert(
                        "subagentId".into(),
                        json!(stored_change.subagent_id.clone()),
                    );
                    object.insert(
                        "subagentRole".into(),
                        json!(stored_change.subagent_role.clone()),
                    );
                }
                Ok(event_payload)
            },
        )?;
        publish_committed_agent_event(repo_root, &stored_event);
        return Ok(());
    }
    append_agent_event(repo_root, snapshot, kind, payload)
}

fn sidecar_event_kind(kind: &str) -> Option<AgentRunEventKind> {
    match kind {
        "run_started" => Some(AgentRunEventKind::RunStarted),
        "message_delta" => Some(AgentRunEventKind::MessageDelta),
        "tool_started" => Some(AgentRunEventKind::ToolStarted),
        "tool_completed" => Some(AgentRunEventKind::ToolCompleted),
        "file_changed" => Some(AgentRunEventKind::FileChanged),
        "command_output" => Some(AgentRunEventKind::CommandOutput),
        "tool_registry_snapshot" => Some(AgentRunEventKind::ToolRegistrySnapshot),
        "policy_decision" => Some(AgentRunEventKind::PolicyDecision),
        "run_completed" => Some(AgentRunEventKind::RunCompleted),
        "run_failed" => Some(AgentRunEventKind::RunFailed),
        _ => None,
    }
}

fn finalize_cursor_agent_run<R: Runtime>(
    app: &AppHandle<R>,
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    report: CursorSidecarReport,
) -> CommandResult<()> {
    let now = now_timestamp();
    if !report.assistant_text.trim().is_empty() {
        project_store::append_agent_message(
            repo_root,
            &NewAgentMessageRecord {
                project_id: snapshot.run.project_id.clone(),
                run_id: snapshot.run.run_id.clone(),
                role: AgentMessageRole::Assistant,
                content: report.assistant_text.trim().to_string(),
                provider_metadata_json: None,
                created_at: now.clone(),
                attachments: Vec::new(),
            },
        )?;
    }
    if report.cancelled {
        project_store::update_agent_run_status(
            repo_root,
            &snapshot.run.project_id,
            &snapshot.run.run_id,
            AgentRunStatus::Cancelled,
            None,
            &now,
        )?;
        append_agent_event(
            repo_root,
            snapshot,
            AgentRunEventKind::RunFailed,
            json!({
                "code": "cursor_run_cancelled",
                "message": "Cursor sidecar run was cancelled.",
                "provenance": cursor_provenance(),
            }),
        )?;
        return Ok(());
    }
    if let Some((code, message)) = report.failure {
        project_store::update_agent_run_status(
            repo_root,
            &snapshot.run.project_id,
            &snapshot.run.run_id,
            AgentRunStatus::Failed,
            Some(AgentRunDiagnosticRecord {
                code: code.clone(),
                message: message.clone(),
            }),
            &now,
        )?;
        append_agent_event(
            repo_root,
            snapshot,
            AgentRunEventKind::RunFailed,
            json!({
                "code": code,
                "message": message,
                "provenance": cursor_provenance(),
                "cursorAgentId": report.cursor_agent_id,
                "cursorRunId": report.cursor_run_id,
                "requestedModelRoute": report.requested_model_route,
                "requestedModelId": report.requested_model_id,
                "resolvedModel": report.resolved_model,
            }),
        )?;
        fail_owned_runtime_run(
            app,
            repo_root,
            snapshot,
            &CommandError::user_fixable("cursor_sidecar_run_failed", "Cursor sidecar run failed."),
            "Cursor sidecar run failed.",
        )?;
        return Ok(());
    }
    project_store::update_agent_run_status(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.run_id,
        AgentRunStatus::Completed,
        None,
        &now,
    )?;
    append_agent_event(
        repo_root,
        snapshot,
        AgentRunEventKind::RunCompleted,
        json!({
            "summary": "Cursor SDK run completed through Xero Cursor sidecar.",
            "provenance": cursor_provenance(),
            "cursorAgentId": report.cursor_agent_id,
            "cursorRunId": report.cursor_run_id,
            "requestedModelRoute": report.requested_model_route,
            "requestedModelId": report.requested_model_id,
            "resolvedModel": report.resolved_model,
        }),
    )?;
    complete_owned_runtime_run(
        app,
        repo_root,
        snapshot,
        "Cursor sidecar runtime completed.",
    )?;
    Ok(())
}

fn append_agent_event(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    event_kind: AgentRunEventKind,
    payload: JsonValue,
) -> CommandResult<()> {
    project_store::append_agent_event(
        repo_root,
        &NewAgentEventRecord {
            project_id: snapshot.run.project_id.clone(),
            run_id: snapshot.run.run_id.clone(),
            event_kind,
            payload_json: payload.to_string(),
            created_at: now_timestamp(),
        },
    )?;
    Ok(())
}

fn cursor_sidecar_sandbox_metadata(
    repo_root: &Path,
    app_data_dir: &Path,
    sidecar_path: &Path,
) -> CommandResult<SandboxExecutionMetadata> {
    let descriptor = ToolDescriptorV2 {
        name: "cursor_sidecar".into(),
        description: "Launch the bundled Cursor SDK sidecar with Xero MCP tools.".into(),
        input_schema: json!({ "type": "object" }),
        capability_tags: vec!["cursor".into(), "subprocess".into(), "mcp".into()],
        application_metadata: Default::default(),
        effect_class: ToolEffectClass::CommandExecution,
        mutability: ToolMutability::Mutating,
        sandbox_requirement: ToolSandboxRequirement::FullLocal,
        approval_requirement: ToolApprovalRequirement::Always,
        telemetry_attributes: Default::default(),
        result_truncation: Default::default(),
    };
    let sandbox = PermissionProfileSandbox::new(SandboxExecutionContext {
        workspace_root: repo_root.display().to_string(),
        app_data_roots: vec![app_data_dir.display().to_string()],
        project_trust: ProjectTrustState::UserApproved,
        approval_source: SandboxApprovalSource::Policy,
        platform: SandboxPlatform::current(),
        explicit_git_mutation_allowed: false,
        legacy_xero_migration_allowed: false,
        preserved_environment_keys: vec![
            "PATH".into(),
            "HOME".into(),
            "USER".into(),
            "LOGNAME".into(),
            "SHELL".into(),
            "TMPDIR".into(),
            "TMP".into(),
            "TEMP".into(),
        ],
    });
    sandbox
        .evaluate(
            &descriptor,
            &ToolCallInput {
                tool_call_id: "cursor-sidecar".into(),
                tool_name: descriptor.name.clone(),
                input: json!({ "sidecarPath": sidecar_path, "repoRoot": repo_root }),
            },
            &ToolExecutionContext::default(),
        )
        .map_err(|denied| CommandError::user_fixable(denied.error.code, denied.error.message))
}

fn resolve_cursor_sidecar_binary() -> CommandResult<PathBuf> {
    if let Some(path) = std::env::var_os(CURSOR_SIDECAR_PATH_ENV).map(PathBuf::from) {
        return validate_executable(path, CURSOR_SIDECAR_PATH_ENV);
    }
    let binary_name = cursor_sidecar_binary_name();
    cursor_sidecar_binary_candidates(&binary_name)
        .into_iter()
        .find_map(|candidate| validate_executable(candidate, CURSOR_SIDECAR_PATH_ENV).ok())
        .ok_or_else(|| {
            CommandError::user_fixable(
                "cursor_sidecar_missing",
                format!(
                    "Bundled Cursor sidecar `{binary_name}` was not found. Build it with `cargo build --package xero-cursor-sidecar` or set {CURSOR_SIDECAR_PATH_ENV}."
                ),
            )
        })
}

fn resolve_cursor_bridge_path() -> CommandResult<PathBuf> {
    if let Some(path) = std::env::var_os(CURSOR_BRIDGE_PATH_ENV).map(PathBuf::from) {
        if path.is_file() {
            return Ok(path);
        }
        return Err(CommandError::user_fixable(
            "cursor_bridge_missing",
            format!(
                "Cursor bridge `{}` from {CURSOR_BRIDGE_PATH_ENV} was not found.",
                path.display()
            ),
        ));
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/cursor-sdk-bridge.mjs");
    if path.is_file() {
        Ok(path)
    } else {
        Err(CommandError::user_fixable(
            "cursor_bridge_missing",
            format!("Cursor bridge `{}` was not found.", path.display()),
        ))
    }
}

fn cursor_sidecar_binary_candidates(binary_name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(binary_name));
            let resources = dir.join("../Resources");
            candidates.push(resources.join(binary_name));
            candidates.push(resources.join("resources").join(binary_name));
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest_dir.join("resources").join(binary_name));
    if let Some(target_root) = manifest_dir.parent() {
        candidates.push(target_root.join("target/debug").join(binary_name));
        candidates.push(target_root.join("target/release").join(binary_name));
    }
    candidates
}

fn validate_executable(path: PathBuf, env_name: &str) -> CommandResult<PathBuf> {
    if path.is_file() {
        Ok(path)
    } else {
        Err(CommandError::user_fixable(
            "cursor_sidecar_missing",
            format!(
                "Cursor sidecar path `{}` from {env_name} does not exist.",
                path.display()
            ),
        ))
    }
}

fn cursor_sidecar_binary_name() -> String {
    if cfg!(windows) {
        format!("{CURSOR_SIDECAR_BINARY_NAME}.exe")
    } else {
        CURSOR_SIDECAR_BINARY_NAME.into()
    }
}

fn write_private_file(path: &Path, bytes: &[u8]) -> CommandResult<PathBuf> {
    fs::write(path, bytes).map_err(|error| {
        CommandError::system_fault(
            "cursor_sidecar_state_write_failed",
            format!("Xero could not write `{}`: {error}", path.display()),
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(path.to_path_buf())
}

fn cursor_mcp_mode(controls: &RuntimeRunControlInputDto) -> &'static str {
    if matches!(
        controls.runtime_agent_id.as_str(),
        "ask" | "plan" | "crawl" | "agent_create"
    ) || controls.plan_mode_required
    {
        "observe-only"
    } else {
        "workspace-write"
    }
}

fn latest_cursor_agent_id(snapshot: &project_store::AgentRunSnapshotRecord) -> Option<String> {
    snapshot.events.iter().rev().find_map(|event| {
        serde_json::from_str::<JsonValue>(&event.payload_json)
            .ok()
            .and_then(|payload| {
                payload
                    .get("cursorAgentId")
                    .and_then(JsonValue::as_str)
                    .map(ToOwned::to_owned)
            })
    })
}

fn runtime_run_controls_as_input(snapshot: &RuntimeRunSnapshotRecord) -> RuntimeRunControlInputDto {
    if let Some(pending) = snapshot.controls.pending.as_ref() {
        RuntimeRunControlInputDto {
            runtime_agent_id: pending.runtime_agent_id,
            agent_definition_id: pending.agent_definition_id.clone(),
            agent_definition_version: pending.agent_definition_version,
            provider_profile_id: pending.provider_profile_id.clone(),
            model_id: pending.model_id.clone(),
            thinking_effort: pending.thinking_effort.clone(),
            approval_mode: pending.approval_mode.clone(),
            plan_mode_required: pending.plan_mode_required,
            auto_compact_enabled: pending.auto_compact_enabled,
        }
    } else {
        RuntimeRunControlInputDto {
            runtime_agent_id: snapshot.controls.active.runtime_agent_id,
            agent_definition_id: snapshot.controls.active.agent_definition_id.clone(),
            agent_definition_version: snapshot.controls.active.agent_definition_version,
            provider_profile_id: snapshot.controls.active.provider_profile_id.clone(),
            model_id: snapshot.controls.active.model_id.clone(),
            thinking_effort: snapshot.controls.active.thinking_effort.clone(),
            approval_mode: snapshot.controls.active.approval_mode.clone(),
            plan_mode_required: snapshot.controls.active.plan_mode_required,
            auto_compact_enabled: snapshot.controls.active.auto_compact_enabled,
        }
    }
}

fn cursor_provenance() -> JsonValue {
    json!({
        "kind": "cursor_sidecar",
        "providerId": CURSOR_PROVIDER_ID,
    })
}

fn truncate_json_event(event: JsonValue) -> JsonValue {
    let text = event.to_string();
    if text.len() <= 16 * 1024 {
        event
    } else {
        json!({
            "truncated": true,
            "preview": truncate_bytes(&text, 16 * 1024),
        })
    }
}

fn truncate_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes.min(value.len());
    while !value.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    format!("{}...[truncated]", &value[..end])
}

fn safe_path_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches('_');
    if sanitized.is_empty() {
        "run".into()
    } else {
        sanitized.chars().take(120).collect()
    }
}
