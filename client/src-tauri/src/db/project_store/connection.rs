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
                "Xero could not open the project state database at {} for {}: {error}",
                database_path.display(),
                repo_root.display()
            ),
        )
    })?;

    configure_connection(&connection).map_err(|error| {
        CommandError::retryable(
            "project_state_integrity_check_failed",
            format!(
                "Xero could not configure SQLite project state at {} before integrity checks: {}",
                database_path.display(),
                error.message
            ),
        )
    })?;
    verify_state_database_integrity(&connection, database_path)?;
    verify_state_database_checkpoint(&connection, database_path)?;
    Ok(connection)
}

fn verify_state_database_integrity(
    connection: &Connection,
    database_path: &Path,
) -> Result<(), CommandError> {
    let quick_check = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get::<_, String>(0))
        .map_err(|error| {
            CommandError::retryable(
                "project_state_integrity_check_failed",
                format!(
                    "Xero could not run a SQLite quick_check for project state at {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    if quick_check.eq_ignore_ascii_case("ok") {
        return Ok(());
    }
    Err(CommandError::retryable(
        "project_state_integrity_check_failed",
        format!(
            "Xero could not safely open project state at {} because SQLite quick_check reported `{quick_check}`.",
            database_path.display()
        ),
    ))
}

fn verify_state_database_checkpoint(
    connection: &Connection,
    database_path: &Path,
) -> Result<(), CommandError> {
    let (busy, log_frames, checkpointed_frames): (i64, i64, i64) = connection
        .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|error| {
            CommandError::retryable(
                "project_state_checkpoint_failed",
                format!(
                    "Xero could not verify the SQLite WAL checkpoint state at {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    if busy == 0 {
        return Ok(());
    }
    Err(CommandError::retryable(
        "project_state_checkpoint_failed",
        format!(
            "Xero could not checkpoint project state at {} because {busy} database connection(s) were busy; {checkpointed_frames} of {log_frames} WAL frame(s) were checkpointed.",
            database_path.display()
        ),
    ))
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
                    "Xero could not initialize selected-project state at {}. The local project state may need to be reset: {error}",
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
                    "Xero could not initialize runtime-session tables at {}. The local project state may need to be reset: {error}",
                    database_path.display()
                ),
            ));
        }
    }
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s40_verify_state_database_integrity_accepts_clean_database() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let database_path = tempdir.path().join("state.db");
        let connection = Connection::open(&database_path).expect("open database");
        configure_connection(&connection).expect("configure connection");

        verify_state_database_integrity(&connection, &database_path)
            .expect("clean database passes quick_check");
        verify_state_database_checkpoint(&connection, &database_path)
            .expect("clean database passes checkpoint probe");
    }

    #[test]
    fn s40_open_runtime_database_reports_corrupt_state_as_integrity_failure() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("repo root");
        let database_path = tempdir.path().join("state.db");
        std::fs::write(&database_path, b"not a sqlite database").expect("write corrupt database");

        let error = open_runtime_database(&repo_root, &database_path)
            .expect_err("corrupt database should fail before runtime use");

        assert_eq!(error.code, "project_state_integrity_check_failed");
    }

    #[test]
    fn s40_open_runtime_database_reports_missing_root_and_state() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let missing_root = tempdir.path().join("missing-repo");
        let database_path = tempdir.path().join("state.db");

        let missing_root_error = open_runtime_database(&missing_root, &database_path)
            .expect_err("missing root should fail before runtime use");
        assert_eq!(missing_root_error.code, "project_root_unavailable");

        let repo_root = tempdir.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("repo root");
        let missing_state_error = open_runtime_database(&repo_root, &database_path)
            .expect_err("missing state should fail before runtime use");
        assert_eq!(missing_state_error.code, "project_state_unavailable");
    }
}
