use std::{path::Path, thread};

use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        validate_non_empty, CommandError, CommandResult, RuntimeRunDto,
        UpdateRuntimeRunControlsRequestDto,
    },
    db::project_store::{self, RuntimeRunSnapshotRecord, RuntimeRunStatus},
    runtime::{
        AgentAutoCompactPreference, AutonomousToolRuntime, ContinueOwnedAgentRunRequest,
        DesktopAgentCoreRuntime, DesktopRunDriveMode, OwnedAgentRunRequest,
    },
    state::DesktopState,
};

use super::agent_task::auto_compact_preference;
use super::agent_tooling_settings::resolve_agent_tool_application_style;
use super::runtime_support::{
    agent_provider_config_identity, apply_owned_runtime_run_pending_controls_with_status,
    bind_owned_runtime_run_to_agent_handoff, emit_runtime_run_updated_if_changed,
    ensure_owned_runtime_provider_turn_capabilities, fail_owned_runtime_run,
    launch_or_reconnect_runtime_run, load_persisted_runtime_run,
    resolve_owned_agent_provider_config, resolve_project_root, runtime_run_dto_from_snapshot,
    update_owned_runtime_run_controls,
};

#[tauri::command]
pub async fn update_runtime_run_controls<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateRuntimeRunControlsRequestDto,
) -> CommandResult<RuntimeRunDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    validate_non_empty(&request.run_id, "runId")?;

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

fn update_runtime_run_controls_blocking<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    request: UpdateRuntimeRunControlsRequestDto,
) -> CommandResult<RuntimeRunDto> {
    let repo_root = resolve_project_root(&app, &state, &request.project_id)?;
    let before =
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

    reject_provider_profile_change(existing, request.controls.as_ref())?;

    let auto_compact = auto_compact_preference(request.auto_compact.clone())?;
    let after = update_owned_runtime_run_controls(
        &repo_root,
        existing,
        request.controls.clone(),
        request.prompt.clone(),
        &request.attachments,
    )?;
    emit_runtime_run_updated_if_changed(
        &app,
        &request.project_id,
        &request.agent_session_id,
        &before,
        &Some(after.clone()),
    )?;
    if let Some(prompt) = normalized_prompt(request.prompt.as_deref()) {
        let agent_core = DesktopAgentCoreRuntime::new(state.agent_run_supervisor().clone());
        if agent_core.is_active(&after.run.run_id)? {
            return Err(CommandError::user_fixable(
                "agent_run_already_active",
                format!(
                    "Xero is already driving owned-agent run `{}`. Wait for it to finish or cancel it before sending another message.",
                    after.run.run_id
                ),
            ));
        }
        mark_existing_agent_run_accepted(&repo_root, &after)?;
        spawn_owned_runtime_prompt_drive(
            app.clone(),
            state.clone(),
            repo_root.clone(),
            after.clone(),
            prompt,
            request.attachments.clone(),
            auto_compact,
        );
    }

    Ok(runtime_run_dto_from_snapshot(&after))
}

fn mark_existing_agent_run_accepted(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
) -> CommandResult<()> {
    let run = match project_store::load_agent_run_record(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.run_id,
    ) {
        Ok(run) => run,
        Err(error) if error.code == "agent_run_not_found" => return Ok(()),
        Err(error) => return Err(error),
    };
    if matches!(
        run.status,
        project_store::AgentRunStatus::Starting | project_store::AgentRunStatus::Running
    ) {
        return Ok(());
    }
    project_store::update_agent_run_status(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.run_id,
        project_store::AgentRunStatus::Running,
        None,
        &now_timestamp(),
    )?;
    Ok(())
}

fn spawn_owned_runtime_prompt_drive<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    repo_root: std::path::PathBuf,
    snapshot: RuntimeRunSnapshotRecord,
    prompt: String,
    attachments: Vec<crate::commands::StagedAgentAttachmentDto>,
    auto_compact: Option<AgentAutoCompactPreference>,
) {
    thread::spawn(move || {
        let before = Some(snapshot.clone());
        match drive_owned_runtime_prompt(
            &app,
            &state,
            &repo_root,
            &snapshot,
            prompt,
            attachments,
            auto_compact,
        ) {
            Ok(Some(rebound)) => {
                let _ = emit_runtime_run_updated_if_changed(
                    &app,
                    &snapshot.run.project_id,
                    &snapshot.run.agent_session_id,
                    &before,
                    &Some(rebound),
                );
            }
            Ok(None) => {
                match apply_owned_runtime_run_pending_controls_with_status(
                    &repo_root,
                    &snapshot,
                    RuntimeRunStatus::Running,
                    "Owned agent runtime accepted the queued prompt.",
                ) {
                    Ok(applied) => {
                        let _ = emit_runtime_run_updated_if_changed(
                            &app,
                            &snapshot.run.project_id,
                            &snapshot.run.agent_session_id,
                            &before,
                            &Some(applied),
                        );
                    }
                    Err(error) => {
                        let _ = fail_owned_runtime_run(
                            &app,
                            &repo_root,
                            &snapshot,
                            &error,
                            "Owned agent task failed.",
                        );
                    }
                }
            }
            Err(error) => {
                let _ = fail_owned_runtime_run(
                    &app,
                    &repo_root,
                    &snapshot,
                    &error,
                    "Owned agent task failed.",
                );
            }
        }
    });
}

fn drive_owned_runtime_prompt<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    prompt: String,
    attachments: Vec<crate::commands::StagedAgentAttachmentDto>,
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
    let tool_runtime = AutonomousToolRuntime::for_project(app, state, &snapshot.run.project_id)?
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
                prompt,
                attachments: attachments
                    .iter()
                    .map(super::runtime_support::staged_attachment_dto_to_message_attachment)
                    .collect(),
                controls,
                tool_runtime,
                provider_config,
                provider_preflight: Some(provider_preflight.clone()),
                answer_pending_actions,
                auto_compact,
            };
            let prepared =
                agent_core.continue_run(continuation, DesktopRunDriveMode::CreateOnly)?;
            let target_run_id = prepared.drive_request.run_id.clone();
            if target_run_id != snapshot.run.run_id && agent_core.is_active(&target_run_id)? {
                return Err(CommandError::user_fixable(
                    "agent_run_already_active",
                    format!(
                        "Xero is already driving owned-agent run `{target_run_id}`. Wait for it to finish or cancel it before sending another message."
                    ),
                ));
            }
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
                agent_core.spawn_owned_agent_continuation(
                    prepared.snapshot.run.agent_session_id.clone(),
                    prepared.drive_request,
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
                prompt,
                attachments: attachments
                    .iter()
                    .map(super::runtime_support::staged_attachment_dto_to_message_attachment)
                    .collect(),
                controls,
                tool_runtime,
                provider_config,
                provider_preflight: Some(provider_preflight),
            };
            agent_core.start_run(request, DesktopRunDriveMode::Background)?;
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn runtime_run_controls_as_input(
    snapshot: &RuntimeRunSnapshotRecord,
) -> crate::commands::RuntimeRunControlInputDto {
    if let Some(pending) = snapshot.controls.pending.as_ref() {
        return crate::commands::RuntimeRunControlInputDto {
            runtime_agent_id: pending.runtime_agent_id,
            agent_definition_id: pending.agent_definition_id.clone(),
            provider_profile_id: pending.provider_profile_id.clone(),
            model_id: pending.model_id.clone(),
            thinking_effort: pending.thinking_effort.clone(),
            approval_mode: pending.approval_mode.clone(),
            plan_mode_required: pending.plan_mode_required,
        };
    }

    crate::commands::RuntimeRunControlInputDto {
        runtime_agent_id: snapshot.controls.active.runtime_agent_id,
        agent_definition_id: snapshot.controls.active.agent_definition_id.clone(),
        provider_profile_id: snapshot.controls.active.provider_profile_id.clone(),
        model_id: snapshot.controls.active.model_id.clone(),
        thinking_effort: snapshot.controls.active.thinking_effort.clone(),
        approval_mode: snapshot.controls.active.approval_mode.clone(),
        plan_mode_required: snapshot.controls.active.plan_mode_required,
    }
}

fn reject_provider_profile_change(
    existing: &RuntimeRunSnapshotRecord,
    controls: Option<&crate::commands::RuntimeRunControlInputDto>,
) -> CommandResult<()> {
    let requested_profile_id = controls
        .and_then(|controls| controls.provider_profile_id.as_deref())
        .map(str::trim)
        .filter(|profile_id| !profile_id.is_empty());
    let active_profile_id = existing
        .controls
        .active
        .provider_profile_id
        .as_deref()
        .map(str::trim)
        .filter(|profile_id| !profile_id.is_empty());

    if let (Some(requested), Some(active)) = (requested_profile_id, active_profile_id) {
        if requested != active {
            return Err(CommandError::user_fixable(
                "runtime_run_provider_profile_switch_blocked",
                format!(
                    "Xero cannot switch active runtime run `{}` from provider profile `{active}` to `{requested}`. Stop the current run before changing providers.",
                    existing.run.run_id
                ),
            ));
        }
    }

    Ok(())
}

fn normalized_prompt(prompt: Option<&str>) -> Option<String> {
    prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .map(ToOwned::to_owned)
}
