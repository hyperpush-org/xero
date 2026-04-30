use std::path::Path;

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        runtime_support::{
            emit_project_updated, emit_runtime_run_updated, resolve_project_root,
            runtime_run_dto_from_snapshot,
        },
        validate_non_empty, CommandError, CommandResult, ProjectUpdateReason, ResumeHistoryStatus,
        ResumeOperatorRunRequestDto, ResumeOperatorRunResponseDto,
    },
    db::project_store::{self, PreparedRuntimeOperatorResume},
    runtime::autonomous_orchestrator::validate_operator_resume_target,
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
    })
}

fn resume_runtime_operator_run<R: Runtime>(
    app: &AppHandle<R>,
    _state: &DesktopState,
    repo_root: &Path,
    runtime_resume: PreparedRuntimeOperatorResume,
) -> CommandResult<ResumeOperatorRunResponseDto> {
    if let Err(error) = validate_operator_resume_target(
        repo_root,
        &runtime_resume.project_id,
        &runtime_resume.agent_session_id,
        &runtime_resume.approval_request.action_id,
        &runtime_resume.boundary_id,
    ) {
        project_store::record_runtime_operator_resume_outcome(
            repo_root,
            &runtime_resume,
            ResumeHistoryStatus::Failed,
            &runtime_resume_failure_summary(&runtime_resume, &error),
        )?;
        emit_runtime_resume_updates(
            app,
            repo_root,
            &runtime_resume.project_id,
            &runtime_resume.agent_session_id,
        )?;
        return Err(error);
    }

    let error = CommandError::user_fixable(
        "runtime_operator_resume_unsupported",
        "Xero no longer resumes legacy runtime boundaries. Send the response through the Xero-owned agent prompt instead.",
    );
    project_store::record_runtime_operator_resume_outcome(
        repo_root,
        &runtime_resume,
        ResumeHistoryStatus::Failed,
        &runtime_resume_failure_summary(&runtime_resume, &error),
    )?;
    emit_runtime_resume_updates(
        app,
        repo_root,
        &runtime_resume.project_id,
        &runtime_resume.agent_session_id,
    )?;
    Err(error)
}

fn emit_runtime_resume_updates<R: Runtime>(
    app: &AppHandle<R>,
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> CommandResult<()> {
    emit_project_updated(
        app,
        repo_root,
        project_id,
        ProjectUpdateReason::MetadataChanged,
    )?;

    let runtime_run = project_store::load_runtime_run(repo_root, project_id, agent_session_id)?;
    let runtime_run = runtime_run.as_ref().map(runtime_run_dto_from_snapshot);
    emit_runtime_run_updated(app, runtime_run.as_ref())
}

fn runtime_resume_failure_summary(
    runtime_resume: &PreparedRuntimeOperatorResume,
    error: &CommandError,
) -> String {
    format!(
        "Xero could not resume removed runtime run `{}` for action `{}` at boundary `{}` after approving {}: [{}] {}.",
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
