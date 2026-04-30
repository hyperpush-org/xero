use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    thread,
    time::Duration,
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    global_db::{
        environment_profile::{
            parse_environment_profile_status, validate_environment_profile_row,
            EnvironmentDiagnostic, EnvironmentDiagnosticSeverity, EnvironmentPathProfile,
            EnvironmentPermissionRequest, EnvironmentPlatform, EnvironmentProfilePayload,
            EnvironmentProfileRow, EnvironmentProfileStatus, EnvironmentProfileSummary,
            ENVIRONMENT_PROFILE_SCHEMA_VERSION,
        },
        open_global_database,
    },
};

use super::probe::{probe_environment_profile, EnvironmentProbeReport};

const PROFILE_STALE_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);

static ACTIVE_DISCOVERIES: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentDiscoveryStatus {
    pub has_profile: bool,
    pub status: EnvironmentProfileStatus,
    pub stale: bool,
    pub should_start: bool,
    pub refreshed_at: Option<String>,
    pub probe_started_at: Option<String>,
    pub probe_completed_at: Option<String>,
    pub permission_requests: Vec<EnvironmentPermissionRequest>,
    pub diagnostics: Vec<EnvironmentDiagnostic>,
}

pub fn environment_discovery_status(
    database_path: &Path,
) -> CommandResult<EnvironmentDiscoveryStatus> {
    let connection = open_global_database(database_path)?;
    let row = load_environment_profile_row(&connection)?;
    Ok(status_from_row(
        row.as_ref(),
        discovery_is_active(database_path),
    ))
}

pub fn start_environment_discovery(
    database_path: PathBuf,
) -> CommandResult<EnvironmentDiscoveryStatus> {
    if !mark_discovery_active(&database_path) {
        return environment_discovery_status(&database_path);
    }

    let mut connection = open_global_database(&database_path)?;
    let current = load_environment_profile_row(&connection)?;
    if !status_from_row(current.as_ref(), false).should_start {
        unmark_discovery_active(&database_path);
        return Ok(status_from_row(current.as_ref(), false));
    }

    persist_marker_profile(&mut connection, EnvironmentProfileStatus::Probing)?;
    let started_status = status_from_row(load_environment_profile_row(&connection)?.as_ref(), true);
    let worker_database_path = database_path.clone();
    thread::spawn(move || {
        let report = probe_environment_profile();
        match open_global_database(&worker_database_path) {
            Ok(mut connection) => {
                let result = match report {
                    Ok(report) => persist_probe_report(&mut connection, &report),
                    Err(error) => persist_failed_profile(
                        &mut connection,
                        "environment_probe_failed",
                        format!("Environment discovery could not build a valid profile: {error}"),
                    ),
                };
                if let Err(error) = result {
                    eprintln!("[environment] discovery persistence failed: {error}");
                }
            }
            Err(error) => {
                eprintln!("[environment] discovery could not open global database: {error}");
            }
        }
        unmark_discovery_active(&worker_database_path);
    });

    Ok(started_status)
}

fn status_from_row(
    row: Option<&EnvironmentProfileRow>,
    active: bool,
) -> EnvironmentDiscoveryStatus {
    let Some(row) = row else {
        return EnvironmentDiscoveryStatus {
            has_profile: false,
            status: EnvironmentProfileStatus::Pending,
            stale: true,
            should_start: !active,
            refreshed_at: None,
            probe_started_at: None,
            probe_completed_at: None,
            permission_requests: vec![],
            diagnostics: vec![],
        };
    };

    let stale = match row.status {
        EnvironmentProfileStatus::Ready | EnvironmentProfileStatus::Partial => {
            timestamp_is_stale(&row.refreshed_at)
        }
        EnvironmentProfileStatus::Pending
        | EnvironmentProfileStatus::Probing
        | EnvironmentProfileStatus::Failed => true,
    };

    EnvironmentDiscoveryStatus {
        has_profile: true,
        status: row.status,
        stale,
        should_start: !active && stale,
        refreshed_at: Some(row.refreshed_at.clone()),
        probe_started_at: row.probe_started_at.clone(),
        probe_completed_at: row.probe_completed_at.clone(),
        permission_requests: serde_json::from_str(&row.permission_requests_json)
            .unwrap_or_default(),
        diagnostics: serde_json::from_str(&row.diagnostics_json).unwrap_or_default(),
    }
}

fn timestamp_is_stale(timestamp: &str) -> bool {
    let Ok(parsed) =
        time::OffsetDateTime::parse(timestamp, &time::format_description::well_known::Rfc3339)
    else {
        return true;
    };
    let age = time::OffsetDateTime::now_utc() - parsed;
    age.whole_seconds() < 0 || age.whole_seconds() as u64 > PROFILE_STALE_AFTER.as_secs()
}

fn discovery_is_active(database_path: &Path) -> bool {
    ACTIVE_DISCOVERIES
        .lock()
        .map(|active| active.contains(database_path))
        .unwrap_or(false)
}

fn mark_discovery_active(database_path: &Path) -> bool {
    ACTIVE_DISCOVERIES
        .lock()
        .map(|mut active| active.insert(database_path.to_path_buf()))
        .unwrap_or(false)
}

fn unmark_discovery_active(database_path: &Path) {
    if let Ok(mut active) = ACTIVE_DISCOVERIES.lock() {
        active.remove(database_path);
    }
}

fn load_environment_profile_row(
    connection: &Connection,
) -> CommandResult<Option<EnvironmentProfileRow>> {
    let row = connection
        .query_row(
            "SELECT schema_version, status, os_kind, os_version, arch, default_shell,
                    path_fingerprint, payload_json, summary_json, permission_requests_json,
                    diagnostics_json, probe_started_at, probe_completed_at, refreshed_at
             FROM environment_profile
             WHERE id = 1",
            [],
            |row| {
                let status: String = row.get(1)?;
                Ok((status, row_to_environment_profile(row)?))
            },
        )
        .optional()
        .map_err(|error| {
            CommandError::system_fault(
                "environment_profile_load_failed",
                format!("Xero could not load the environment profile: {error}"),
            )
        })?;

    let Some((status, mut row)) = row else {
        return Ok(None);
    };
    row.status = parse_environment_profile_status(&status).map_err(validation_error)?;
    validate_environment_profile_row(&row).map_err(validation_error)?;
    Ok(Some(row))
}

fn row_to_environment_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<EnvironmentProfileRow> {
    Ok(EnvironmentProfileRow {
        schema_version: row.get(0)?,
        status: EnvironmentProfileStatus::Pending,
        os_kind: row.get(2)?,
        os_version: row.get(3)?,
        arch: row.get(4)?,
        default_shell: row.get(5)?,
        path_fingerprint: row.get(6)?,
        payload_json: row.get(7)?,
        summary_json: row.get(8)?,
        permission_requests_json: row.get(9)?,
        diagnostics_json: row.get(10)?,
        probe_started_at: row.get(11)?,
        probe_completed_at: row.get(12)?,
        refreshed_at: row.get(13)?,
    })
}

fn persist_marker_profile(
    connection: &mut Connection,
    status: EnvironmentProfileStatus,
) -> CommandResult<()> {
    let timestamp = now_timestamp();
    let platform = current_platform();
    let path = EnvironmentPathProfile {
        entry_count: 0,
        fingerprint: None,
        sources: vec![],
    };
    let payload = EnvironmentProfilePayload {
        schema_version: ENVIRONMENT_PROFILE_SCHEMA_VERSION,
        platform: platform.clone(),
        path,
        tools: vec![],
        capabilities: vec![],
        permissions: vec![],
        diagnostics: vec![],
    };
    let summary = EnvironmentProfileSummary {
        schema_version: ENVIRONMENT_PROFILE_SCHEMA_VERSION,
        status,
        platform: platform.clone(),
        refreshed_at: Some(timestamp.clone()),
        tools: vec![],
        capabilities: vec![],
        permission_requests: vec![],
        diagnostics: vec![],
    };

    let payload_json = serialize_profile_json(&payload)?;
    let summary_json = serialize_profile_json(&summary)?;
    upsert_environment_profile(
        connection,
        status,
        &platform,
        None,
        &payload_json,
        &summary_json,
        "[]",
        "[]",
        if status == EnvironmentProfileStatus::Probing {
            Some(timestamp.as_str())
        } else {
            None
        },
        None,
        &timestamp,
    )
}

fn persist_probe_report(
    connection: &mut Connection,
    report: &EnvironmentProbeReport,
) -> CommandResult<()> {
    let payload_json = serialize_profile_json(&report.payload)?;
    let summary_json = serialize_profile_json(&report.summary)?;
    let permission_requests_json = serialize_profile_json(&report.payload.permissions)?;
    let diagnostics_json = serialize_profile_json(&report.payload.diagnostics)?;
    upsert_environment_profile(
        connection,
        report.status,
        &report.payload.platform,
        report.payload.path.fingerprint.as_deref(),
        &payload_json,
        &summary_json,
        &permission_requests_json,
        &diagnostics_json,
        Some(&report.started_at),
        Some(&report.completed_at),
        &report.completed_at,
    )
}

fn persist_failed_profile(
    connection: &mut Connection,
    code: &str,
    message: String,
) -> CommandResult<()> {
    let timestamp = now_timestamp();
    let platform = current_platform();
    let diagnostics = vec![EnvironmentDiagnostic {
        code: code.into(),
        severity: EnvironmentDiagnosticSeverity::Error,
        message,
        retryable: true,
        tool_id: None,
    }];
    let payload = EnvironmentProfilePayload {
        schema_version: ENVIRONMENT_PROFILE_SCHEMA_VERSION,
        platform: platform.clone(),
        path: EnvironmentPathProfile {
            entry_count: 0,
            fingerprint: None,
            sources: vec![],
        },
        tools: vec![],
        capabilities: vec![],
        permissions: vec![],
        diagnostics: diagnostics.clone(),
    };
    let summary = EnvironmentProfileSummary {
        schema_version: ENVIRONMENT_PROFILE_SCHEMA_VERSION,
        status: EnvironmentProfileStatus::Failed,
        platform: platform.clone(),
        refreshed_at: Some(timestamp.clone()),
        tools: vec![],
        capabilities: vec![],
        permission_requests: vec![],
        diagnostics,
    };
    let payload_json = serialize_profile_json(&payload)?;
    let summary_json = serialize_profile_json(&summary)?;
    let diagnostics_json = serialize_profile_json(&payload.diagnostics)?;
    upsert_environment_profile(
        connection,
        EnvironmentProfileStatus::Failed,
        &platform,
        None,
        &payload_json,
        &summary_json,
        "[]",
        &diagnostics_json,
        None,
        Some(&timestamp),
        &timestamp,
    )
}

#[allow(clippy::too_many_arguments)]
fn upsert_environment_profile(
    connection: &mut Connection,
    status: EnvironmentProfileStatus,
    platform: &EnvironmentPlatform,
    path_fingerprint: Option<&str>,
    payload_json: &str,
    summary_json: &str,
    permission_requests_json: &str,
    diagnostics_json: &str,
    probe_started_at: Option<&str>,
    probe_completed_at: Option<&str>,
    refreshed_at: &str,
) -> CommandResult<()> {
    connection
        .execute(
            "INSERT INTO environment_profile (
                id, schema_version, status, os_kind, os_version, arch, default_shell,
                path_fingerprint, payload_json, summary_json, permission_requests_json,
                diagnostics_json, probe_started_at, probe_completed_at, refreshed_at, updated_at
            ) VALUES (
                1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14
            )
            ON CONFLICT(id) DO UPDATE SET
                schema_version = excluded.schema_version,
                status = excluded.status,
                os_kind = excluded.os_kind,
                os_version = excluded.os_version,
                arch = excluded.arch,
                default_shell = excluded.default_shell,
                path_fingerprint = excluded.path_fingerprint,
                payload_json = excluded.payload_json,
                summary_json = excluded.summary_json,
                permission_requests_json = excluded.permission_requests_json,
                diagnostics_json = excluded.diagnostics_json,
                probe_started_at = excluded.probe_started_at,
                probe_completed_at = excluded.probe_completed_at,
                refreshed_at = excluded.refreshed_at,
                updated_at = excluded.updated_at",
            params![
                ENVIRONMENT_PROFILE_SCHEMA_VERSION,
                status.as_str(),
                &platform.os_kind,
                platform.os_version.as_deref(),
                &platform.arch,
                platform.default_shell.as_deref(),
                path_fingerprint,
                payload_json,
                summary_json,
                permission_requests_json,
                diagnostics_json,
                probe_started_at,
                probe_completed_at,
                refreshed_at,
            ],
        )
        .map(|_| ())
        .map_err(|error| {
            CommandError::system_fault(
                "environment_profile_save_failed",
                format!("Xero could not save the environment profile: {error}"),
            )
        })
}

fn current_platform() -> EnvironmentPlatform {
    EnvironmentPlatform {
        os_kind: env::consts::OS.to_string(),
        os_version: None,
        arch: env::consts::ARCH.to_string(),
        default_shell: None,
    }
}

fn serialize_profile_json<T: Serialize>(value: &T) -> CommandResult<String> {
    serde_json::to_string(value).map_err(|error| {
        CommandError::system_fault(
            "environment_profile_encode_failed",
            format!("Xero could not encode the environment profile: {error}"),
        )
    })
}

fn validation_error(
    error: crate::global_db::environment_profile::EnvironmentProfileValidationError,
) -> CommandError {
    CommandError::system_fault(
        "environment_profile_invalid",
        format!("Xero found an invalid environment profile: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global_db::{configure_connection, migrations};

    fn connection() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open db");
        configure_connection(&connection).expect("configure db");
        migrations::migrations()
            .to_latest(&mut connection)
            .expect("migrate db");
        connection
    }

    #[test]
    fn missing_profile_requests_silent_start() {
        let status = status_from_row(None, false);

        assert!(!status.has_profile);
        assert_eq!(status.status, EnvironmentProfileStatus::Pending);
        assert!(status.should_start);
        assert!(status.permission_requests.is_empty());
    }

    #[test]
    fn fresh_ready_profile_does_not_restart() {
        let mut connection = connection();
        persist_marker_profile(&mut connection, EnvironmentProfileStatus::Ready)
            .expect("persist profile");
        let row = load_environment_profile_row(&connection)
            .expect("load row")
            .expect("row");
        let status = status_from_row(Some(&row), false);

        assert!(status.has_profile);
        assert_eq!(status.status, EnvironmentProfileStatus::Ready);
        assert!(!status.stale);
        assert!(!status.should_start);
    }

    #[test]
    fn probing_profile_restarts_when_no_worker_is_active() {
        let mut connection = connection();
        persist_marker_profile(&mut connection, EnvironmentProfileStatus::Probing)
            .expect("persist profile");
        let row = load_environment_profile_row(&connection)
            .expect("load row")
            .expect("row");

        assert!(status_from_row(Some(&row), false).should_start);
        assert!(!status_from_row(Some(&row), true).should_start);
    }
}
