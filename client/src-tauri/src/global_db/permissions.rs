//! Phase 6 file-mode hardening.
//!
//! At app start, set restrictive permissions on the app-data directory and on every
//! Cadence database file (`cadence.db` plus its WAL/SHM sidecars, and any per-project
//! `state.db`). The directory is `0700` so peers on a multi-user box cannot enumerate
//! credentials; database files are `0600` so peers cannot read or replace them.
//!
//! Hardening is **best-effort**: callers log failures rather than aborting boot. SQLite
//! creates the WAL/SHM sidecars on first connection, so the helper accepts them as
//! optional and only chmods the ones that exist. Re-running the hardening pass is a
//! no-op when the modes already match.
//!
//! Windows has no equivalent to POSIX modes, so the public helpers compile to a no-op
//! on non-Unix targets and the unit tests are gated to Unix.

use std::path::Path;

#[cfg(unix)]
const APP_DATA_DIRECTORY_MODE: u32 = 0o700;
#[cfg(unix)]
const DATABASE_FILE_MODE: u32 = 0o600;

#[cfg(unix)]
const DATABASE_SIDECAR_SUFFIXES: &[&str] = &["-wal", "-shm"];

/// Errors produced by the hardening helpers. Always `Display`-able so callers can log
/// without further plumbing.
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("could not stat `{path}`: {source}")]
    Stat {
        path: String,
        source: std::io::Error,
    },
    #[error("could not chmod `{path}` to {mode:#o}: {source}")]
    Chmod {
        path: String,
        mode: u32,
        source: std::io::Error,
    },
}

/// Hardens the app-data directory and the global database file (plus WAL/SHM sidecars).
///
/// `app_data_dir` is chmod'd to `0700`; `global_db_path` and any companion sidecars are
/// chmod'd to `0600`. Missing files are skipped — they may not exist before SQLite opens
/// the database for the first time.
pub fn harden_global_paths(
    app_data_dir: &Path,
    global_db_path: &Path,
) -> Result<(), PermissionError> {
    chmod_directory(app_data_dir, APP_DATA_DIRECTORY_MODE)?;
    chmod_database_with_sidecars(global_db_path)
}

/// Hardens a per-project SQLite database file. Use after the file has been created or
/// migrated to its app-data location. Like the global helper, missing sidecars are
/// silently skipped.
pub fn harden_project_database(database_path: &Path) -> Result<(), PermissionError> {
    chmod_database_with_sidecars(database_path)
}

#[cfg(unix)]
fn chmod_database_with_sidecars(database_path: &Path) -> Result<(), PermissionError> {
    chmod_file_if_present(database_path, DATABASE_FILE_MODE)?;
    if let Some(file_name) = database_path.file_name().and_then(|name| name.to_str()) {
        if let Some(parent) = database_path.parent() {
            for suffix in DATABASE_SIDECAR_SUFFIXES {
                let sidecar_name = format!("{file_name}{suffix}");
                let sidecar_path = parent.join(sidecar_name);
                chmod_file_if_present(&sidecar_path, DATABASE_FILE_MODE)?;
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn chmod_database_with_sidecars(_database_path: &Path) -> Result<(), PermissionError> {
    Ok(())
}

#[cfg(unix)]
fn chmod_directory(path: &Path, mode: u32) -> Result<(), PermissionError> {
    use std::{
        fs::{self, Permissions},
        os::unix::fs::PermissionsExt,
    };

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(PermissionError::Stat {
                path: path.display().to_string(),
                source,
            });
        }
    };

    if !metadata.is_dir() {
        return Ok(());
    }

    let current = metadata.permissions().mode() & 0o777;
    if current == mode {
        return Ok(());
    }

    fs::set_permissions(path, Permissions::from_mode(mode)).map_err(|source| {
        PermissionError::Chmod {
            path: path.display().to_string(),
            mode,
            source,
        }
    })
}

#[cfg(not(unix))]
fn chmod_directory(_path: &Path, _mode: u32) -> Result<(), PermissionError> {
    Ok(())
}

#[cfg(unix)]
fn chmod_file_if_present(path: &Path, mode: u32) -> Result<(), PermissionError> {
    use std::{
        fs::{self, Permissions},
        os::unix::fs::PermissionsExt,
    };

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(PermissionError::Stat {
                path: path.display().to_string(),
                source,
            });
        }
    };

    // Skip symlinks: hardening is meant for the actual file Cadence owns. Following a
    // symlink could relax permissions on a target the user did not intend to modify.
    if metadata.file_type().is_symlink() {
        return Ok(());
    }

    if !metadata.is_file() {
        return Ok(());
    }

    let current = metadata.permissions().mode() & 0o777;
    if current == mode {
        return Ok(());
    }

    fs::set_permissions(path, Permissions::from_mode(mode)).map_err(|source| {
        PermissionError::Chmod {
            path: path.display().to_string(),
            mode,
            source,
        }
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    use std::{
        fs::{self, File, Permissions},
        os::unix::fs::PermissionsExt,
    };
    use tempfile::tempdir;

    fn mode_of(path: &Path) -> u32 {
        fs::symlink_metadata(path)
            .expect("stat path")
            .permissions()
            .mode()
            & 0o777
    }

    #[test]
    fn hardens_directory_to_0700() {
        let dir = tempdir().expect("tempdir");
        let app_data = dir.path().join("app");
        fs::create_dir(&app_data).expect("mkdir");
        fs::set_permissions(&app_data, Permissions::from_mode(0o755)).expect("seed mode");

        let database = app_data.join("cadence.db");
        File::create(&database).expect("create db");

        harden_global_paths(&app_data, &database).expect("harden");
        assert_eq!(mode_of(&app_data), 0o700);
        assert_eq!(mode_of(&database), 0o600);
    }

    #[test]
    fn hardens_wal_and_shm_sidecars() {
        let dir = tempdir().expect("tempdir");
        let database = dir.path().join("cadence.db");
        let wal = dir.path().join("cadence.db-wal");
        let shm = dir.path().join("cadence.db-shm");
        File::create(&database).expect("db");
        File::create(&wal).expect("wal");
        File::create(&shm).expect("shm");
        fs::set_permissions(&database, Permissions::from_mode(0o644)).expect("seed db mode");
        fs::set_permissions(&wal, Permissions::from_mode(0o644)).expect("seed wal mode");
        fs::set_permissions(&shm, Permissions::from_mode(0o644)).expect("seed shm mode");

        harden_global_paths(dir.path(), &database).expect("harden");
        assert_eq!(mode_of(&database), 0o600);
        assert_eq!(mode_of(&wal), 0o600);
        assert_eq!(mode_of(&shm), 0o600);
    }

    #[test]
    fn missing_sidecars_are_skipped() {
        let dir = tempdir().expect("tempdir");
        let database = dir.path().join("cadence.db");
        File::create(&database).expect("db");

        harden_global_paths(dir.path(), &database).expect("harden");
        assert_eq!(mode_of(&database), 0o600);
        assert!(!dir.path().join("cadence.db-wal").exists());
        assert!(!dir.path().join("cadence.db-shm").exists());
    }

    #[test]
    fn missing_database_is_no_op() {
        let dir = tempdir().expect("tempdir");
        let database = dir.path().join("cadence.db");

        harden_global_paths(dir.path(), &database).expect("harden");
        assert!(!database.exists());
        assert_eq!(mode_of(dir.path()), 0o700);
    }

    #[test]
    fn idempotent_when_already_hardened() {
        let dir = tempdir().expect("tempdir");
        let database = dir.path().join("cadence.db");
        File::create(&database).expect("db");
        fs::set_permissions(&database, Permissions::from_mode(0o600)).expect("seed");
        fs::set_permissions(dir.path(), Permissions::from_mode(0o700)).expect("seed dir");

        harden_global_paths(dir.path(), &database).expect("first run");
        harden_global_paths(dir.path(), &database).expect("second run");
        assert_eq!(mode_of(&database), 0o600);
        assert_eq!(mode_of(dir.path()), 0o700);
    }

    #[test]
    fn project_database_hardens_main_and_sidecars() {
        let dir = tempdir().expect("tempdir");
        let project_dir = dir.path().join("projects").join("p-1");
        fs::create_dir_all(&project_dir).expect("mkdir");
        let database = project_dir.join("state.db");
        let wal = project_dir.join("state.db-wal");
        File::create(&database).expect("db");
        File::create(&wal).expect("wal");
        fs::set_permissions(&database, Permissions::from_mode(0o644)).expect("seed");
        fs::set_permissions(&wal, Permissions::from_mode(0o644)).expect("seed");

        harden_project_database(&database).expect("harden");
        assert_eq!(mode_of(&database), 0o600);
        assert_eq!(mode_of(&wal), 0o600);
    }

    #[test]
    fn directory_chmod_is_skipped_for_files() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("not-a-dir");
        File::create(&file).expect("file");
        fs::set_permissions(&file, Permissions::from_mode(0o644)).expect("seed");

        // chmod_directory is private; exercise it by passing a file path to harden_global_paths
        // as the first argument. The file should be left alone (not chmod'd to 0o700).
        let database = dir.path().join("cadence.db");
        File::create(&database).expect("db");
        harden_global_paths(&file, &database).expect("harden");
        assert_eq!(mode_of(&file), 0o644);
        assert_eq!(mode_of(&database), 0o600);
    }

    #[test]
    fn symlink_target_is_not_chmodded() {
        let dir = tempdir().expect("tempdir");
        let real_target = dir.path().join("real.db");
        File::create(&real_target).expect("create real target");
        fs::set_permissions(&real_target, Permissions::from_mode(0o644)).expect("seed");

        let database_link = dir.path().join("cadence.db");
        std::os::unix::fs::symlink(&real_target, &database_link).expect("symlink");

        harden_global_paths(dir.path(), &database_link).expect("harden");

        // The symlink target keeps its original mode (we refuse to follow).
        assert_eq!(mode_of(&real_target), 0o644);
    }
}
