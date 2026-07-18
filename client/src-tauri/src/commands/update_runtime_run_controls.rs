use std::{
    collections::HashSet,
    path::Path,
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandError, CommandResult, RuntimeRunDto,
        UpdateRuntimeRunControlsRequestDto,
    },
    db::project_store::{self, RuntimeRunSnapshotRecord, RuntimeRunStatus},
    runtime::{
        create_owned_agent_run, register_existing_initial_agent_continuation,
        AgentAutoCompactPreference, AutonomousToolRuntime, ContinueOwnedAgentRunRequest,
        DesktopAgentCoreRuntime, DesktopRunDriveMode, OwnedAgentRunRequest,
    },
    state::DesktopState,
};

use super::agent_task::auto_compact_preference;
use super::agent_tooling_settings::resolve_agent_tool_application_style;
use super::runtime_support::{
    agent_provider_config_identity, apply_owned_runtime_run_pending_controls_with_status,
    bind_owned_runtime_run_to_agent_handoff, drive_cursor_runtime_prompt,
    emit_runtime_run_updated_if_changed, ensure_owned_runtime_provider_turn_capabilities,
    fail_owned_runtime_run, is_cursor_runtime_provider, launch_or_reconnect_runtime_run,
    load_persisted_runtime_run, resolve_owned_agent_provider_config,
    resolve_owned_runtime_profile_selection, resolve_project_root, runtime_run_dto_from_snapshot,
    update_owned_runtime_run_controls,
};

const QUEUED_PROMPT_DRIVE_POLL_INTERVAL: Duration = Duration::from_millis(250);
static OWNED_RUNTIME_PROMPT_DRIVE_WORKERS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

struct OwnedRuntimePromptDriveWorkerRegistration {
    key: String,
}

impl OwnedRuntimePromptDriveWorkerRegistration {
    fn try_register(project_id: &str, run_id: &str) -> Option<Self> {
        let key = format!("{project_id}\0{run_id}");
        let workers = OWNED_RUNTIME_PROMPT_DRIVE_WORKERS.get_or_init(|| Mutex::new(HashSet::new()));
        let mut workers = workers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !workers.insert(key.clone()) {
            return None;
        }
        drop(workers);
        Some(Self { key })
    }
}

impl Drop for OwnedRuntimePromptDriveWorkerRegistration {
    fn drop(&mut self) {
        let workers = OWNED_RUNTIME_PROMPT_DRIVE_WORKERS.get_or_init(|| Mutex::new(HashSet::new()));
        workers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.key);
    }
}

#[tauri::command]
pub async fn update_runtime_run_controls<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateRuntimeRunControlsRequestDto,
) -> CommandResult<RuntimeRunDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.continuation_request_id, "continuationRequestId")?;

    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        update_runtime_run_controls_blocking(app, state, request)
    })
    .await
    .map_err(|error| {
        CommandError::system_fault(
            "runtime_run_update_controls_task_failed",
            format!("Xero could not finish background runtime-run control work: {error}"),
        )
    })?
}

pub(crate) fn update_runtime_run_controls_blocking<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    request: UpdateRuntimeRunControlsRequestDto,
) -> CommandResult<RuntimeRunDto> {
    let repo_root = resolve_project_root(&app, &state, &request.project_id)?;
    let mut before =
        load_persisted_runtime_run(&repo_root, &request.project_id, &request.agent_session_id)?;
    let Some(existing) = before.as_ref() else {
        return Err(CommandError::retryable(
            "runtime_run_missing",
            format!(
                "Xero cannot queue runtime-run controls because project `{}` has no durable runtime run.",
                request.project_id
            ),
        ));
    };

    if existing.run.run_id != request.run_id {
        return Err(CommandError::user_fixable(
            "runtime_run_mismatch",
            format!(
                "Xero refused to queue controls for run `{}` because project `{}` is currently bound to durable run `{}`.",
                request.run_id, request.project_id, existing.run.run_id
            ),
        ));
    }

    if existing.run.supervisor_kind != crate::runtime::OWNED_AGENT_SUPERVISOR_KIND {
        let outcome = launch_or_reconnect_runtime_run(
            &app,
            &state,
            &request.project_id,
            &request.agent_session_id,
            request.controls.clone(),
            request.prompt.clone(),
            request.attachments.clone(),
            request.linked_paths.clone(),
        )?;
        emit_runtime_run_updated_if_changed(
            &app,
            &request.project_id,
            &request.agent_session_id,
            &before,
            &Some(outcome.snapshot.clone()),
        )?;
        return Ok(runtime_run_dto_from_snapshot(&outcome.snapshot));
    }

    const MAX_CONTROL_UPDATE_ATTEMPTS: usize = 5;
    let mut last_conflict = None;
    for attempt in 0..MAX_CONTROL_UPDATE_ATTEMPTS {
        if attempt > 0 {
            before = load_persisted_runtime_run(
                &repo_root,
                &request.project_id,
                &request.agent_session_id,
            )?;
        }
        let Some(existing) = before.as_ref() else {
            return Err(CommandError::retryable(
                "runtime_run_missing",
                format!(
                    "Xero cannot queue runtime-run controls because project `{}` has no durable runtime run.",
                    request.project_id
                ),
            ));
        };
        if existing.run.run_id != request.run_id {
            return Err(CommandError::user_fixable(
                "runtime_run_mismatch",
                format!(
                    "Xero refused to queue controls for run `{}` because project `{}` is currently bound to durable run `{}`.",
                    request.run_id, request.project_id, existing.run.run_id
                ),
            ));
        }
        if existing.run.supervisor_kind != crate::runtime::OWNED_AGENT_SUPERVISOR_KIND {
            return Err(CommandError::retryable(
                "runtime_run_supervisor_changed",
                format!(
                    "Xero could not queue owned-agent controls because runtime run `{}` changed supervisors. Refresh and retry.",
                    request.run_id
                ),
            ));
        }

        let next_provider_id = match request.controls.as_ref() {
            Some(controls) => {
                resolve_owned_runtime_profile_selection(&app, &state, Some(controls))?.provider_id
            }
            None => existing.run.provider_id.clone(),
        };
        match update_owned_runtime_run_controls(
            &repo_root,
            existing,
            &next_provider_id,
            request.controls.clone(),
            &request.continuation_request_id,
            request.prompt.clone(),
            &request.attachments,
            &request.linked_paths,
        ) {
            Ok(after) => {
                emit_runtime_run_updated_if_changed(
                    &app,
                    &request.project_id,
                    &request.agent_session_id,
                    &before,
                    &Some(after.clone()),
                )?;
                if normalized_prompt(request.prompt.as_deref()).is_some() {
                    spawn_owned_runtime_prompt_drive_when_idle(
                        app.clone(),
                        state.clone(),
                        repo_root.clone(),
                        after.clone(),
                    );
                }
                return Ok(runtime_run_dto_from_snapshot(&after));
            }
            Err(error) if error.code == "runtime_run_write_conflict" => {
                last_conflict = Some(error);
                thread::yield_now();
            }
            Err(error) => return Err(error),
        }
    }

    Err(last_conflict.expect("a control-update retry only continues after a write conflict"))
}

fn queued_attachments_as_staged(
    attachments: &[project_store::RuntimeRunQueuedAttachmentRecord],
) -> Vec<crate::commands::StagedAgentAttachmentDto> {
    attachments
        .iter()
        .map(|attachment| crate::commands::StagedAgentAttachmentDto {
            kind: attachment.kind.clone(),
            absolute_path: attachment.absolute_path.clone(),
            media_type: attachment.media_type.clone(),
            original_name: attachment.original_name.clone(),
            size_bytes: attachment.size_bytes,
            width: attachment.width,
            height: attachment.height,
        })
        .collect()
}

fn queued_linked_paths_as_dto(
    linked_paths: &[project_store::RuntimeRunQueuedLinkedPathRecord],
) -> Vec<crate::commands::RuntimeLinkedPathDto> {
    linked_paths
        .iter()
        .map(|linked_path| crate::commands::RuntimeLinkedPathDto {
            kind: linked_path.kind.clone(),
            absolute_path: linked_path.absolute_path.clone(),
        })
        .collect()
}

fn spawn_owned_runtime_prompt_drive_when_idle<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    repo_root: std::path::PathBuf,
    snapshot: RuntimeRunSnapshotRecord,
) {
    let Some(worker_registration) = OwnedRuntimePromptDriveWorkerRegistration::try_register(
        &snapshot.run.project_id,
        &snapshot.run.run_id,
    ) else {
        return;
    };
    let _ = thread::Builder::new()
        .name("xero-runtime-prompt-recovery".into())
        .spawn(move || {
            let _worker_registration = worker_registration;
            let agent_core = DesktopAgentCoreRuntime::new(state.agent_run_supervisor().clone());
            loop {
                if matches!(agent_core.is_active(&snapshot.run.run_id), Ok(true)) {
                    thread::sleep(QUEUED_PROMPT_DRIVE_POLL_INTERVAL);
                    continue;
                }
                let latest = match load_persisted_runtime_run(
                    &repo_root,
                    &snapshot.run.project_id,
                    &snapshot.run.agent_session_id,
                ) {
                    Ok(Some(latest)) if latest.run.run_id == snapshot.run.run_id => latest,
                    _ => return,
                };
                let Some(pending) = latest.controls.pending.as_ref() else {
                    return;
                };
                let Some(prompt) = normalized_prompt(pending.queued_prompt.as_deref()) else {
                    return;
                };
                let Some(continuation_request_id) = pending
                    .queued_prompt_continuation_request_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|request_id| !request_id.is_empty())
                    .map(ToOwned::to_owned)
                else {
                    return;
                };
                let attachments = queued_attachments_as_staged(&pending.queued_attachments);
                let linked_paths = queued_linked_paths_as_dto(&pending.queued_linked_paths);
                let auto_compact = match derive_auto_compact_preference(&latest) {
                    Ok(auto_compact) => auto_compact,
                    Err(error) => {
                        let _ = fail_owned_runtime_run(
                            &app,
                            &repo_root,
                            &latest,
                            &error,
                            "Owned agent task failed.",
                        );
                        return;
                    }
                };
                let before = Some(latest.clone());
                match drive_owned_runtime_prompt(
                    &app,
                    &state,
                    &repo_root,
                    &latest,
                    continuation_request_id.clone(),
                    prompt,
                    attachments,
                    linked_paths,
                    auto_compact,
                ) {
                    Ok(Some(rebound)) => {
                        let _ = emit_runtime_run_updated_if_changed(
                            &app,
                            &latest.run.project_id,
                            &latest.run.agent_session_id,
                            &before,
                            &Some(rebound),
                        );
                        return;
                    }
                    Ok(None) => {
                        match project_store::load_agent_continuation_request_by_id(
                            &repo_root,
                            &latest.run.project_id,
                            &continuation_request_id,
                        ) {
                            Ok(Some(request))
                                if request.state
                                    == project_store::AgentContinuationRequestState::Prepared =>
                            {
                                if let Ok(agent_snapshot) = project_store::load_agent_run(
                                    &repo_root,
                                    &latest.run.project_id,
                                    &latest.run.run_id,
                                ) {
                                    if agent_snapshot.run.status
                                        == project_store::AgentRunStatus::Failed
                                    {
                                        let diagnostic = agent_snapshot.run.last_error.unwrap_or(
                                            project_store::AgentRunDiagnosticRecord {
                                                code: "agent_continuation_pre_dispatch_failed".into(),
                                                message: "The queued prompt failed before provider dispatch and was not retried automatically.".into(),
                                            },
                                        );
                                        let _ = fail_owned_runtime_run(
                                            &app,
                                            &repo_root,
                                            &latest,
                                            &CommandError::user_fixable(
                                                diagnostic.code,
                                                diagnostic.message,
                                            ),
                                            "Owned agent task failed before provider dispatch.",
                                        );
                                        return;
                                    }
                                }
                                thread::sleep(QUEUED_PROMPT_DRIVE_POLL_INTERVAL);
                                continue;
                            }
                            Ok(Some(request))
                                if request.state
                                    == project_store::AgentContinuationRequestState::Driving =>
                            {
                                // The provider turn is in flight. Keep this worker alive so it can
                                // reconcile the durable request and apply pending controls once the
                                // supervisor becomes idle; the continuation id prevents replay.
                                thread::sleep(QUEUED_PROMPT_DRIVE_POLL_INTERVAL);
                                continue;
                            }
                            Err(error) => {
                                let _ = fail_owned_runtime_run(
                                    &app,
                                    &repo_root,
                                    &latest,
                                    &error,
                                    "Owned agent task failed.",
                                );
                                return;
                            }
                            _ => {}
                        }
                        let current = match load_persisted_runtime_run(
                            &repo_root,
                            &latest.run.project_id,
                            &latest.run.agent_session_id,
                        ) {
                            Ok(Some(current)) if current.run.run_id == latest.run.run_id => current,
                            _ => return,
                        };
                        let still_current =
                            current.controls.pending.as_ref().is_some_and(|pending| {
                                pending.queued_prompt_continuation_request_id.as_deref()
                                    == Some(continuation_request_id.as_str())
                            });
                        if !still_current {
                            continue;
                        }
                        match apply_owned_runtime_run_pending_controls_with_status(
                            &repo_root,
                            &current,
                            RuntimeRunStatus::Running,
                            "Owned agent runtime accepted the queued prompt.",
                        ) {
                            Ok(applied) => {
                                let _ = emit_runtime_run_updated_if_changed(
                                    &app,
                                    &current.run.project_id,
                                    &current.run.agent_session_id,
                                    &Some(current.clone()),
                                    &Some(applied),
                                );
                                return;
                            }
                            Err(error) if error.code == "runtime_run_write_conflict" => continue,
                            Err(error) => {
                                let _ = fail_owned_runtime_run(
                                    &app,
                                    &repo_root,
                                    &current,
                                    &error,
                                    "Owned agent task failed.",
                                );
                                return;
                            }
                        }
                    }
                    Err(error) if queued_prompt_drive_race_is_benign(&error) => {
                        thread::sleep(QUEUED_PROMPT_DRIVE_POLL_INTERVAL);
                    }
                    Err(error) => {
                        let _ = fail_owned_runtime_run(
                            &app,
                            &repo_root,
                            &latest,
                            &error,
                            "Owned agent task failed.",
                        );
                        return;
                    }
                }
            }
        });
}

pub(crate) fn recover_pending_runtime_prompts_for_project<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    repo_root: std::path::PathBuf,
    project_id: &str,
) -> CommandResult<()> {
    for snapshot in project_store::list_runtime_runs_for_project(&repo_root, project_id)? {
        recover_pending_runtime_prompt_snapshot(
            app.clone(),
            state.clone(),
            repo_root.clone(),
            snapshot,
        );
    }
    Ok(())
}

pub(crate) fn recover_pending_runtime_prompt_snapshot<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    repo_root: std::path::PathBuf,
    snapshot: RuntimeRunSnapshotRecord,
) {
    let recoverable = snapshot.controls.pending.as_ref().is_some_and(|pending| {
        normalized_prompt(pending.queued_prompt.as_deref()).is_some()
            && pending
                .queued_prompt_continuation_request_id
                .as_deref()
                .is_some_and(|request_id| !request_id.trim().is_empty())
    });
    if recoverable {
        spawn_owned_runtime_prompt_drive_when_idle(app, state, repo_root, snapshot);
    }
}

fn queued_prompt_drive_race_is_benign(error: &CommandError) -> bool {
    matches!(
        error.code.as_str(),
        "agent_run_already_active" | "runtime_run_write_conflict"
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "runtime prompt dispatch requires the complete persisted prompt context"
)]
fn drive_owned_runtime_prompt<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    continuation_request_id: String,
    prompt: String,
    attachments: Vec<crate::commands::StagedAgentAttachmentDto>,
    linked_paths: Vec<crate::commands::RuntimeLinkedPathDto>,
    auto_compact: Option<AgentAutoCompactPreference>,
) -> CommandResult<Option<RuntimeRunSnapshotRecord>> {
    let agent_core = DesktopAgentCoreRuntime::new(state.agent_run_supervisor().clone());
    if agent_core.is_active(&snapshot.run.run_id)? {
        return Err(CommandError::user_fixable(
            "agent_run_already_active",
            format!(
                "Xero is already driving owned-agent run `{}`. Wait for it to finish or cancel it before sending another message.",
                snapshot.run.run_id
            ),
        ));
    }

    if is_cursor_runtime_provider(&snapshot.run.provider_id) {
        drive_cursor_runtime_prompt(
            app,
            state,
            repo_root,
            snapshot,
            &continuation_request_id,
            prompt,
            attachments,
        )?;
        return load_persisted_runtime_run(
            repo_root,
            &snapshot.run.project_id,
            &snapshot.run.agent_session_id,
        );
    }

    let controls = Some(runtime_run_controls_as_input(snapshot));
    let provider_config = resolve_owned_agent_provider_config(app, state, controls.as_ref())?;
    let (provider_id, model_id) = agent_provider_config_identity(&provider_config);
    let profile_id = controls
        .as_ref()
        .and_then(|controls| controls.provider_profile_id.as_deref())
        .map(str::trim)
        .filter(|profile_id| !profile_id.is_empty())
        .unwrap_or(provider_id.as_str())
        .to_string();
    let provider_preflight = ensure_owned_runtime_provider_turn_capabilities(
        app,
        state,
        state.owned_agent_provider_config_override().is_none(),
        &profile_id,
        &provider_id,
        &model_id,
        &attachments,
    )?;
    let tool_application_policy =
        resolve_agent_tool_application_style(app, state, &provider_id, &model_id)?;
    let tool_runtime = AutonomousToolRuntime::for_project_with_provider_config(
        app,
        state,
        &snapshot.run.project_id,
        Some(&provider_config),
    )?
    .with_tool_application_policy(tool_application_policy);
    match project_store::load_agent_run(repo_root, &snapshot.run.project_id, &snapshot.run.run_id) {
        Ok(agent_snapshot) => {
            let answer_pending_actions = agent_snapshot
                .action_requests
                .iter()
                .any(|action| action.status == "pending");
            let continuation = ContinueOwnedAgentRunRequest {
                repo_root: repo_root.to_path_buf(),
                project_id: snapshot.run.project_id.clone(),
                run_id: snapshot.run.run_id.clone(),
                continuation_request_id: continuation_request_id.clone(),
                prompt,
                attachments: attachments
                    .iter()
                    .map(super::runtime_support::staged_attachment_dto_to_message_attachment)
                    .collect(),
                linked_paths: linked_paths.clone(),
                controls,
                tool_runtime,
                provider_config,
                provider_preflight: Some(provider_preflight.clone()),
                answer_pending_actions,
                answer_pending_action_id: None,
                auto_compact,
                internal_resume: None,
            };
            if continuation_request_id.starts_with("runtime-start:") {
                register_existing_initial_agent_continuation(&continuation, &agent_snapshot)?;
            }
            let mut prepared =
                agent_core.continue_run(continuation, DesktopRunDriveMode::CreateOnly)?;
            let target_run_id = prepared.drive_request.run_id.clone();
            let rebound = if target_run_id != snapshot.run.run_id {
                Some(bind_owned_runtime_run_to_agent_handoff(
                    repo_root,
                    snapshot,
                    &prepared.snapshot,
                )?)
            } else {
                None
            };
            if prepared.drive_required {
                let drive_lease = prepared.drive_lease.take();
                agent_core.spawn_owned_agent_continuation(
                    prepared.snapshot.run.agent_session_id.clone(),
                    prepared.drive_request,
                    drive_lease,
                )?;
            }
            Ok(rebound)
        }
        Err(error) if error.code == "agent_run_not_found" => {
            let request = OwnedAgentRunRequest {
                repo_root: repo_root.to_path_buf(),
                project_id: snapshot.run.project_id.clone(),
                agent_session_id: snapshot.run.agent_session_id.clone(),
                run_id: snapshot.run.run_id.clone(),
                prompt: prompt.clone(),
                attachments: attachments
                    .iter()
                    .map(super::runtime_support::staged_attachment_dto_to_message_attachment)
                    .collect(),
                linked_paths: linked_paths.clone(),
                controls: controls.clone(),
                tool_runtime: tool_runtime.clone(),
                provider_config: provider_config.clone(),
                provider_preflight: Some(provider_preflight.clone()),
            };
            let created = create_owned_agent_run(&request)?;
            let continuation = ContinueOwnedAgentRunRequest {
                repo_root: repo_root.to_path_buf(),
                project_id: snapshot.run.project_id.clone(),
                run_id: snapshot.run.run_id.clone(),
                continuation_request_id,
                prompt,
                attachments: request.attachments,
                linked_paths,
                controls,
                tool_runtime,
                provider_config,
                provider_preflight: Some(provider_preflight),
                answer_pending_actions: false,
                answer_pending_action_id: None,
                auto_compact,
                internal_resume: None,
            };
            register_existing_initial_agent_continuation(&continuation, &created)?;
            let mut prepared =
                agent_core.continue_run(continuation, DesktopRunDriveMode::CreateOnly)?;
            if prepared.drive_required {
                let drive_lease = prepared.drive_lease.take();
                agent_core.spawn_owned_agent_continuation(
                    prepared.snapshot.run.agent_session_id.clone(),
                    prepared.drive_request,
                    drive_lease,
                )?;
            }
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

const AUTO_COMPACT_DEFAULT_THRESHOLD_PERCENT: u8 = 85;
const AUTO_COMPACT_DEFAULT_RAW_TAIL_MESSAGE_COUNT: u32 = 8;

fn derive_auto_compact_preference(
    snapshot: &RuntimeRunSnapshotRecord,
) -> CommandResult<Option<AgentAutoCompactPreference>> {
    let enabled = snapshot
        .controls
        .pending
        .as_ref()
        .map(|pending| pending.auto_compact_enabled)
        .unwrap_or(snapshot.controls.active.auto_compact_enabled);
    if !enabled {
        return Ok(None);
    }
    auto_compact_preference(Some(crate::commands::AgentAutoCompactPreferenceDto {
        enabled: true,
        threshold_percent: Some(AUTO_COMPACT_DEFAULT_THRESHOLD_PERCENT),
        raw_tail_message_count: Some(AUTO_COMPACT_DEFAULT_RAW_TAIL_MESSAGE_COUNT),
    }))
}

fn runtime_run_controls_as_input(
    snapshot: &RuntimeRunSnapshotRecord,
) -> crate::commands::RuntimeRunControlInputDto {
    if let Some(pending) = snapshot.controls.pending.as_ref() {
        return crate::commands::RuntimeRunControlInputDto {
            runtime_agent_id: pending.runtime_agent_id,
            agent_definition_id: pending.agent_definition_id.clone(),
            agent_definition_version: pending.agent_definition_version,
            provider_profile_id: pending.provider_profile_id.clone(),
            model_id: pending.model_id.clone(),
            thinking_effort: pending.thinking_effort.clone(),
            approval_mode: pending.approval_mode.clone(),
            plan_mode_required: pending.plan_mode_required,
            auto_compact_enabled: pending.auto_compact_enabled,
        };
    }

    crate::commands::RuntimeRunControlInputDto {
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

fn normalized_prompt(prompt: Option<&str>) -> Option<String> {
    prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod worker_registration_tests {
    use super::*;

    #[test]
    fn prompt_drive_worker_registration_deduplicates_and_cleans_up() {
        let first = OwnedRuntimePromptDriveWorkerRegistration::try_register(
            "worker-registry-project",
            "worker-registry-run",
        )
        .expect("first worker registration");
        assert!(OwnedRuntimePromptDriveWorkerRegistration::try_register(
            "worker-registry-project",
            "worker-registry-run",
        )
        .is_none());

        drop(first);

        assert!(OwnedRuntimePromptDriveWorkerRegistration::try_register(
            "worker-registry-project",
            "worker-registry-run",
        )
        .is_some());
    }
}
