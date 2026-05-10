use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

use crate::{
    commands::CommandError,
    db::{
        configure_connection, is_database_too_far_ahead, migrations::migrations,
        rebuild_incompatible_project_database,
    },
};
use rusqlite::Connection;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct VerifiedStateDatabaseKey {
    path: PathBuf,
}

static VERIFIED_STATE_DATABASES: LazyLock<Mutex<HashSet<VerifiedStateDatabaseKey>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static MIGRATED_STATE_DATABASES: LazyLock<Mutex<HashSet<VerifiedStateDatabaseKey>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

fn open_state_database(
    repo_root: &Path,
    database_path: &Path,
) -> Result<(Connection, VerifiedStateDatabaseKey), CommandError> {
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
    let verified_key = verify_state_database_once(&connection, database_path)?;
    Ok((connection, verified_key))
}

fn verify_state_database_once(
    connection: &Connection,
    database_path: &Path,
) -> Result<VerifiedStateDatabaseKey, CommandError> {
    let key = verified_state_database_key(database_path)?;
    let mut verified_databases = VERIFIED_STATE_DATABASES.lock().map_err(|_| {
        CommandError::system_fault(
            "project_state_verification_cache_failed",
            "Xero could not check the project-state verification cache.",
        )
    })?;
    if verified_databases.contains(&key) {
        return Ok(key);
    }

    verify_state_database_integrity(connection, database_path)?;
    verify_state_database_checkpoint(connection, database_path)?;
    verified_databases.insert(key.clone());
    Ok(key)
}

fn verified_state_database_key(
    database_path: &Path,
) -> Result<VerifiedStateDatabaseKey, CommandError> {
    let path = fs::canonicalize(database_path).unwrap_or_else(|_| database_path.to_path_buf());

    Ok(VerifiedStateDatabaseKey { path })
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
    open_migrated_state_database(
        repo_root,
        database_path,
        "project_state_migration_failed",
        "Xero could not initialize selected-project state",
    )
}

pub(crate) fn open_runtime_database(
    repo_root: &Path,
    database_path: &Path,
) -> Result<Connection, CommandError> {
    open_migrated_state_database(
        repo_root,
        database_path,
        "runtime_session_migration_failed",
        "Xero could not initialize runtime-session tables",
    )
}

fn open_migrated_state_database(
    repo_root: &Path,
    database_path: &Path,
    migration_error_code: &'static str,
    migration_error_message: &'static str,
) -> Result<Connection, CommandError> {
    let (mut connection, key) = open_state_database(repo_root, database_path)?;
    let mut migrated_databases = MIGRATED_STATE_DATABASES.lock().map_err(|_| {
        CommandError::system_fault(
            "project_state_migration_cache_failed",
            "Xero could not check the project-state migration cache.",
        )
    })?;
    if migrated_databases.contains(&key) {
        return Ok(connection);
    }

    match migrations().to_latest(&mut connection) {
        Ok(()) => {}
        Err(error) if is_database_too_far_ahead(&error) => {
            connection =
                rebuild_incompatible_project_database(repo_root, database_path, connection)?;
        }
        Err(error) => {
            return Err(CommandError::retryable(
                migration_error_code,
                format!(
                    "{migration_error_message} at {}. The local project state may need to be reset: {error}",
                    database_path.display()
                ),
            ));
        }
    }
    migrated_databases.insert(key);
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
