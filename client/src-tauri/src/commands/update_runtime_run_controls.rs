use std::{path::Path, thread};

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandError, CommandResult, RuntimeRunDto,
        UpdateRuntimeRunControlsRequestDto,
    },
    db::project_store::{self, RuntimeRunSnapshotRecord},
    runtime::{
        create_owned_agent_run, drive_owned_agent_continuation, drive_owned_agent_run,
        prepare_owned_agent_continuation,
        update_runtime_run_controls as update_supervised_runtime_run_controls, AgentRunSupervisor,
        AutonomousToolRuntime, ContinueOwnedAgentRunRequest, OwnedAgentRunRequest,
        RuntimeSupervisorUpdateControlsRequest,
    },
    state::DesktopState,
};

use super::runtime_support::{
    emit_runtime_run_updated_if_changed, load_persisted_runtime_run,
    normalize_requested_runtime_run_controls, resolve_owned_agent_provider_config,
    resolve_project_root, runtime_run_dto_from_snapshot, update_owned_runtime_run_controls,
    DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
};

#[tauri::command]
pub fn update_runtime_run_controls<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateRuntimeRunControlsRequestDto,
) -> CommandResult<RuntimeRunDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    validate_non_empty(&request.run_id, "runId")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let before =
        load_persisted_runtime_run(&repo_root, &request.project_id, &request.agent_session_id)?;
    let Some(existing) = before.as_ref() else {
        return Err(CommandError::retryable(
            "runtime_run_missing",
            format!(
                "Cadence cannot queue runtime-run controls because project `{}` has no durable runtime run.",
                request.project_id
            ),
        ));
    };

    if existing.run.run_id != request.run_id {
        return Err(CommandError::user_fixable(
            "runtime_run_mismatch",
            format!(
                "Cadence refused to queue controls for run `{}` because project `{}` is currently bound to durable run `{}`.",
                request.run_id, request.project_id, existing.run.run_id
            ),
        ));
    }

    if existing.run.supervisor_kind == crate::runtime::OWNED_AGENT_SUPERVISOR_KIND {
        let after = update_owned_runtime_run_controls(
            &repo_root,
            existing,
            request.controls.clone(),
            request.prompt.clone(),
        )?;
        emit_runtime_run_updated_if_changed(
            &app,
            &request.project_id,
            &request.agent_session_id,
            &before,
            &Some(after.clone()),
        )?;
        if let Some(prompt) = normalized_prompt(request.prompt.as_deref()) {
            drive_owned_runtime_prompt(&app, state.inner(), &repo_root, &after, prompt)?;
        }
        return Ok(runtime_run_dto_from_snapshot(&after));
    }

    let normalized_controls = request
        .controls
        .as_ref()
        .map(|controls| normalize_requested_runtime_run_controls(&app, state.inner(), controls))
        .transpose()?;

    let after = update_supervised_runtime_run_controls(
        state.inner(),
        RuntimeSupervisorUpdateControlsRequest {
            project_id: request.project_id.clone(),
            agent_session_id: request.agent_session_id.clone(),
            repo_root,
            run_id: request.run_id.clone(),
            controls: normalized_controls,
            prompt: request.prompt.clone(),
            control_timeout: DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
        },
    )?;
    emit_runtime_run_updated_if_changed(
        &app,
        &request.project_id,
        &request.agent_session_id,
        &before,
        &Some(after.clone()),
    )?;

    Ok(runtime_run_dto_from_snapshot(&after))
}

fn drive_owned_runtime_prompt<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    prompt: String,
) -> CommandResult<()> {
    if state
        .agent_run_supervisor()
        .is_active(&snapshot.run.run_id)?
    {
        return Err(CommandError::user_fixable(
            "agent_run_already_active",
            format!(
                "Cadence is already driving owned-agent run `{}`. Wait for it to finish or cancel it before sending another message.",
                snapshot.run.run_id
            ),
        ));
    }

    let controls = Some(runtime_run_controls_as_input(snapshot));
    let provider_config = resolve_owned_agent_provider_config(app, state, controls.as_ref())?;
    let tool_runtime = AutonomousToolRuntime::for_project(app, state, &snapshot.run.project_id)?;
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
                controls,
                tool_runtime,
                provider_config,
                answer_pending_actions,
            };
            prepare_owned_agent_continuation(&continuation)?;
            spawn_owned_agent_continuation(
                state.agent_run_supervisor().clone(),
                snapshot.run.agent_session_id.clone(),
                continuation,
            )
        }
        Err(error) if error.code == "agent_run_not_found" => {
            let request = OwnedAgentRunRequest {
                repo_root: repo_root.to_path_buf(),
                project_id: snapshot.run.project_id.clone(),
                agent_session_id: snapshot.run.agent_session_id.clone(),
                run_id: snapshot.run.run_id.clone(),
                prompt,
                controls,
                tool_runtime,
                provider_config,
            };
            create_owned_agent_run(&request)?;
            spawn_owned_agent_run(state.agent_run_supervisor().clone(), request)
        }
        Err(error) => Err(error),
    }
}

fn spawn_owned_agent_run(
    supervisor: AgentRunSupervisor,
    request: OwnedAgentRunRequest,
) -> CommandResult<()> {
    let lease = supervisor.begin(
        &request.project_id,
        &request.agent_session_id,
        &request.run_id,
    )?;
    thread::spawn(move || {
        let token = lease.token();
        let _ = drive_owned_agent_run(request, token);
        drop(lease);
    });
    Ok(())
}

fn spawn_owned_agent_continuation(
    supervisor: AgentRunSupervisor,
    agent_session_id: String,
    request: ContinueOwnedAgentRunRequest,
) -> CommandResult<()> {
    let lease = supervisor.begin(&request.project_id, &agent_session_id, &request.run_id)?;
    thread::spawn(move || {
        let token = lease.token();
        let _ = drive_owned_agent_continuation(request, token);
        drop(lease);
    });
    Ok(())
}

fn runtime_run_controls_as_input(
    snapshot: &RuntimeRunSnapshotRecord,
) -> crate::commands::RuntimeRunControlInputDto {
    crate::commands::RuntimeRunControlInputDto {
        model_id: snapshot.controls.active.model_id.clone(),
        thinking_effort: snapshot.controls.active.thinking_effort.clone(),
        approval_mode: snapshot.controls.active.approval_mode.clone(),
        plan_mode_required: snapshot.controls.active.plan_mode_required,
    }
}

fn normalized_prompt(prompt: Option<&str>) -> Option<String> {
    prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .map(ToOwned::to_owned)
}
