use std::path::Path;

use crate::{
    commands::CommandError,
    db::{
        configure_connection, is_database_too_far_ahead, migrations::migrations,
        rebuild_incompatible_project_database,
    },
};
use rusqlite::Connection;

fn open_state_database(repo_root: &Path, database_path: &Path) -> Result<Connection, CommandError> {
    if !repo_root.is_dir() {
        return Err(CommandError::user_fixable(
            "project_root_unavailable",
            format!(
                "Imported project root {} is no longer available.",
                repo_root.display()
            ),
        ));
    }

    if !database_path.exists() {
        return Err(CommandError::retryable(
            "project_state_unavailable",
            format!(
                "Imported project at {} is missing project state at {}.",
                repo_root.display(),
                database_path.display()
            ),
        ));
    }

    let connection = Connection::open(database_path).map_err(|error| {
        CommandError::retryable(
            "project_state_open_failed",
            format!(
                "Cadence could not open the project state database at {} for {}: {error}",
                database_path.display(),
                repo_root.display()
            ),
        )
    })?;

    configure_connection(&connection)?;
    Ok(connection)
}

pub(crate) fn open_project_database(
    repo_root: &Path,
    database_path: &Path,
) -> Result<Connection, CommandError> {
    let mut connection = open_state_database(repo_root, database_path)?;
    match migrations().to_latest(&mut connection) {
        Ok(()) => {}
        Err(error) if is_database_too_far_ahead(&error) => {
            connection =
                rebuild_incompatible_project_database(repo_root, database_path, connection)?;
        }
        Err(error) => {
            return Err(CommandError::retryable(
                "project_state_migration_failed",
                format!(
                    "Cadence could not initialize selected-project state at {}. The local project state may need to be reset: {error}",
                    database_path.display()
                ),
            ));
        }
    }
    Ok(connection)
}

pub(crate) fn open_runtime_database(
    repo_root: &Path,
    database_path: &Path,
) -> Result<Connection, CommandError> {
    let mut connection = open_state_database(repo_root, database_path)?;
    match migrations().to_latest(&mut connection) {
        Ok(()) => {}
        Err(error) if is_database_too_far_ahead(&error) => {
            connection =
                rebuild_incompatible_project_database(repo_root, database_path, connection)?;
        }
        Err(error) => {
            return Err(CommandError::retryable(
                "runtime_session_migration_failed",
                format!(
                    "Cadence could not initialize runtime-session tables at {}. The local project state may need to be reset: {error}",
                    database_path.display()
                ),
            ));
        }
    }
    Ok(connection)
}
