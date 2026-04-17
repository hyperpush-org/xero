use std::path::Path;

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        map_workflow_automatic_dispatch_outcome,
        runtime_support::{
            emit_project_updated, emit_runtime_run_updated, resolve_project_root,
            runtime_run_dto_from_snapshot, DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
        },
        validate_non_empty, CommandError, CommandResult, ProjectUpdateReason, ResumeHistoryStatus,
        ResumeOperatorRunRequestDto, ResumeOperatorRunResponseDto,
    },
    db::project_store::{self, PreparedRuntimeOperatorResume},
    runtime::{submit_runtime_run_input, RuntimeSupervisorSubmitInputRequest},
    state::DesktopState,
};

#[tauri::command]
pub fn resume_operator_run<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ResumeOperatorRunRequestDto,
) -> CommandResult<ResumeOperatorRunResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.action_id, "actionId")?;

    let user_answer = normalize_optional_non_empty(request.user_answer, "userAnswer")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;

    if let Some(runtime_resume) = project_store::prepare_runtime_operator_run_resume(
        &repo_root,
        &request.project_id,
        &request.action_id,
        user_answer.as_deref(),
    )? {
        return resume_runtime_operator_run(&app, state.inner(), &repo_root, runtime_resume);
    }

    let resumed = project_store::resume_operator_run_with_user_answer(
        &repo_root,
        &request.project_id,
        &request.action_id,
        user_answer.as_deref(),
    )?;

    emit_project_updated(
        &app,
        &repo_root,
        &request.project_id,
        ProjectUpdateReason::MetadataChanged,
    )?;

    Ok(ResumeOperatorRunResponseDto {
        approval_request: resumed.approval_request,
        resume_entry: resumed.resume_entry,
        automatic_dispatch: resumed
            .automatic_dispatch
            .map(map_workflow_automatic_dispatch_outcome),
    })
}

fn resume_runtime_operator_run<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    runtime_resume: PreparedRuntimeOperatorResume,
) -> CommandResult<ResumeOperatorRunResponseDto> {
    match submit_runtime_run_input(
        state,
        RuntimeSupervisorSubmitInputRequest {
            project_id: runtime_resume.project_id.clone(),
            repo_root: repo_root.to_path_buf(),
            run_id: runtime_resume.run_id.clone(),
            session_id: runtime_resume.session_id.clone(),
            flow_id: runtime_resume.flow_id.clone(),
            action_id: runtime_resume.approval_request.action_id.clone(),
            boundary_id: runtime_resume.boundary_id.clone(),
            input: runtime_resume.user_answer.clone(),
            control_timeout: DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
        },
    ) {
        Ok(_) => {
            let resumed = project_store::record_runtime_operator_resume_outcome(
                repo_root,
                &runtime_resume,
                ResumeHistoryStatus::Started,
                &runtime_resume_success_summary(&runtime_resume),
            )?;
            emit_runtime_resume_updates(app, repo_root, &runtime_resume.project_id)?;
            Ok(ResumeOperatorRunResponseDto {
                approval_request: resumed.approval_request,
                resume_entry: resumed.resume_entry,
                automatic_dispatch: resumed
                    .automatic_dispatch
                    .map(map_workflow_automatic_dispatch_outcome),
            })
        }
        Err(error) => {
            project_store::record_runtime_operator_resume_outcome(
                repo_root,
                &runtime_resume,
                ResumeHistoryStatus::Failed,
                &runtime_resume_failure_summary(&runtime_resume, &error),
            )?;
            emit_runtime_resume_updates(app, repo_root, &runtime_resume.project_id)?;
            Err(error)
        }
    }
}

fn emit_runtime_resume_updates<R: Runtime>(
    app: &AppHandle<R>,
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<()> {
    emit_project_updated(
        app,
        repo_root,
        project_id,
        ProjectUpdateReason::MetadataChanged,
    )?;

    let runtime_run = project_store::load_runtime_run(repo_root, project_id)?;
    let runtime_run = runtime_run.as_ref().map(runtime_run_dto_from_snapshot);
    emit_runtime_run_updated(app, runtime_run.as_ref())
}

fn runtime_resume_success_summary(runtime_resume: &PreparedRuntimeOperatorResume) -> String {
    format!(
        "Operator resumed detached runtime run `{}` for action `{}` at boundary `{}` after approving {}.",
        runtime_resume.run_id,
        runtime_resume.approval_request.action_id,
        runtime_resume.boundary_id,
        runtime_resume.approval_request.title,
    )
}

fn runtime_resume_failure_summary(
    runtime_resume: &PreparedRuntimeOperatorResume,
    error: &CommandError,
) -> String {
    format!(
        "Cadence could not resume detached runtime run `{}` for action `{}` at boundary `{}` after approving {}: [{}] {}.",
        runtime_resume.run_id,
        runtime_resume.approval_request.action_id,
        runtime_resume.boundary_id,
        runtime_resume.approval_request.title,
        error.code,
        error.message,
    )
}

fn normalize_optional_non_empty(
    value: Option<String>,
    field: &'static str,
) -> CommandResult<Option<String>> {
    match value {
        Some(value) if value.trim().is_empty() => Err(CommandError::invalid_request(field)),
        Some(value) => Ok(Some(value.trim().to_string())),
        None => Ok(None),
    }
}
