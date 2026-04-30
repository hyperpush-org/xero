use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, Runtime};

use crate::{
    commands::{
        CommandError, CommandResult, ProjectUpdateReason, ProjectUpdatedPayloadDto,
        PROJECT_UPDATED_EVENT,
    },
    db::project_store,
    state::DesktopState,
};

pub(crate) fn resolve_project_root<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
) -> CommandResult<PathBuf> {
    crate::runtime::resolve_imported_repo_root(app, state, project_id)
}

pub(crate) fn emit_project_updated<R: Runtime>(
    app: &AppHandle<R>,
    repo_root: &Path,
    project_id: &str,
    reason: ProjectUpdateReason,
) -> CommandResult<()> {
    let project = project_store::load_project_summary(repo_root, project_id)?;

    app.emit(
        PROJECT_UPDATED_EVENT,
        ProjectUpdatedPayloadDto { project, reason },
    )
    .map_err(|error| {
        CommandError::retryable(
            "project_updated_emit_failed",
            format!(
                "Xero updated selected-project metadata but could not emit the project update event: {error}"
            ),
        )
    })
}
