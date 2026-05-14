use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{validate_non_empty, CommandError, CommandResult, ProjectIdRequestDto},
    db::project_app_data_dir_for_project,
    registry,
    state::DesktopState,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WipeProjectDataResponseDto {
    pub schema: String,
    pub project_id: String,
    pub directory_removed: bool,
    pub projects: Vec<crate::commands::ProjectSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WipeAllDataResponseDto {
    pub schema: String,
    pub directory_removed: bool,
}

#[tauri::command]
pub fn wipe_project_data<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<WipeProjectDataResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let registry_path = state.global_db_path(&app)?;
    let registry_snapshot = registry::read_registry(&registry_path)?;
    let remaining_projects = registry_snapshot
        .projects
        .into_iter()
        .filter(|record| record.project_id != request.project_id)
        .collect::<Vec<_>>();

    registry::replace_projects(&registry_path, remaining_projects)?;

    let project_dir = project_app_data_dir_for_project(&request.project_id);
    let directory_removed = remove_directory_if_present(&project_dir)?;

    let refreshed = super::list_projects::load_projects_from_registry(&registry_path)?;

    Ok(WipeProjectDataResponseDto {
        schema: "xero.wipe_project_data_command.v1".into(),
        project_id: request.project_id,
        directory_removed,
        projects: refreshed.projects,
    })
}

#[tauri::command]
pub fn wipe_all_xero_data<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<WipeAllDataResponseDto> {
    let registry_path = state.global_db_path(&app)?;
    let app_data_dir = state.app_data_dir(&app)?;

    let _ = registry::replace_projects(&registry_path, Vec::new());

    let directory_removed = remove_directory_contents_if_present(&app_data_dir)?;

    Ok(WipeAllDataResponseDto {
        schema: "xero.wipe_all_data_command.v1".into(),
        directory_removed,
    })
}

fn remove_directory_if_present(path: &Path) -> CommandResult<bool> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(CommandError::retryable(
            "wipe_data_remove_failed",
            format!(
                "Xero could not remove app-data directory {}: {error}",
                path.display()
            ),
        )),
    }
}

fn remove_directory_contents_if_present(path: &Path) -> CommandResult<bool> {
    let read_dir = match fs::read_dir(path) {
        Ok(read_dir) => read_dir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(CommandError::retryable(
                "wipe_data_read_failed",
                format!(
                    "Xero could not enumerate app-data directory {}: {error}",
                    path.display()
                ),
            ));
        }
    };

    for entry in read_dir {
        let entry = entry.map_err(|error| {
            CommandError::retryable(
                "wipe_data_read_failed",
                format!(
                    "Xero could not read entry in app-data directory {}: {error}",
                    path.display()
                ),
            )
        })?;
        let entry_path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            CommandError::retryable(
                "wipe_data_read_failed",
                format!("Xero could not stat {}: {error}", entry_path.display()),
            )
        })?;

        let result = if file_type.is_dir() {
            fs::remove_dir_all(&entry_path)
        } else {
            fs::remove_file(&entry_path)
        };

        result.map_err(|error| {
            CommandError::retryable(
                "wipe_data_remove_failed",
                format!("Xero could not remove {}: {error}", entry_path.display()),
            )
        })?;
    }

    Ok(true)
}
