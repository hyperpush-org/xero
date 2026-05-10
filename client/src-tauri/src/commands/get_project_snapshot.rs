use std::path::PathBuf;

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandResult, ProjectIdRequestDto, ProjectSnapshotResponseDto,
    },
    db::project_store,
    state::DesktopState,
};

#[tauri::command]
pub async fn get_project_snapshot<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<ProjectSnapshotResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let jobs = state.backend_jobs().clone();
    let state = state.inner().clone();
    let project_id = request.project_id;
    jobs.run_blocking_latest(
        "project-snapshot",
        "project snapshot",
        move |cancellation| {
            cancellation.check_cancelled("project snapshot")?;
            project_snapshot_for_project(&app, &state, &project_id)
        },
    )
    .await
}

pub(crate) struct ProjectSnapshotForProject {
    pub snapshot: ProjectSnapshotResponseDto,
    pub repo_root: PathBuf,
}

pub(crate) fn project_snapshot_for_project<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
) -> CommandResult<ProjectSnapshotResponseDto> {
    project_snapshot_record_for_project(app, state, project_id).map(|record| record.snapshot)
}

pub(crate) fn project_snapshot_record_for_project<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
) -> CommandResult<ProjectSnapshotForProject> {
    validate_non_empty(project_id, "projectId")?;

    let registry_path = state.global_db_path(app)?;
    let repo_root =
        crate::runtime::resolve_imported_repo_root_from_registry(&registry_path, project_id)?;

    let (record, agent_sessions) =
        project_store::load_project_snapshot_and_agent_sessions(&repo_root, project_id, false)?;
    let mut snapshot = record.snapshot;
    snapshot.agent_sessions = agent_sessions
        .iter()
        .map(super::agent_session::agent_session_dto)
        .collect();
    Ok(ProjectSnapshotForProject {
        snapshot,
        repo_root,
    })
}
