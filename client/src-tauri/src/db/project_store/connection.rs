use std::path::Path;

use rusqlite::Connection;

use crate::{
    commands::CommandError,
    db::{configure_connection, migrations::migrations, project_store::drain_pending_into_lance},
};

fn project_id_from_meta(connection: &Connection) -> Option<String> {
    use rusqlite::OptionalExtension;
    connection
        .query_row::<String, _, _>(
            "SELECT project_id FROM meta WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten()
}

fn drain_lance_imports(connection: &Connection, database_path: &Path) {
    let project_id = project_id_from_meta(connection)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    if let Err(error) = drain_pending_into_lance(database_path, &project_id) {
        eprintln!(
            "[cadence] agent_memories lance drain failed at {}: {} ({})",
            database_path.display(),
            error.message,
            error.code
        );
    }
}

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
    migrations().to_latest(&mut connection).map_err(|error| {
        CommandError::retryable(
            "project_state_migration_failed",
            format!(
                "Cadence could not migrate selected-project state at {}: {error}",
                database_path.display()
            ),
        )
    })?;
    drain_lance_imports(&connection, database_path);
    Ok(connection)
}

pub(crate) fn open_runtime_database(
    repo_root: &Path,
    database_path: &Path,
) -> Result<Connection, CommandError> {
    let mut connection = open_state_database(repo_root, database_path)?;
    migrations().to_latest(&mut connection).map_err(|error| {
        CommandError::retryable(
            "runtime_session_migration_failed",
            format!(
                "Cadence could not migrate runtime-session tables at {}: {error}",
                database_path.display()
            ),
        )
    })?;
    drain_lance_imports(&connection, database_path);
    Ok(connection)
}
