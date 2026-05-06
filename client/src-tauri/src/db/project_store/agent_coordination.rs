use std::{collections::BTreeSet, path::Path};

use rand::RngCore;
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::database_path_for_repo,
};

use super::{open_runtime_database, validate_non_empty_text};

const DEFAULT_PRESENCE_LEASE_SECONDS: i64 = 120;
const DEFAULT_RESERVATION_LEASE_SECONDS: i64 = 300;
const DEFAULT_EVENT_LEASE_SECONDS: i64 = 3_600;
const MAX_COORDINATION_CONTEXT_PRESENCE: usize = 6;
const MAX_COORDINATION_CONTEXT_RESERVATIONS: usize = 12;
const MAX_COORDINATION_CONTEXT_EVENTS: usize = 8;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentCoordinationReservationOperation {
    Observing,
    Editing,
    Refactoring,
    Testing,
    Verifying,
    Writing,
}

impl AgentCoordinationReservationOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observing => "observing",
            Self::Editing => "editing",
            Self::Refactoring => "refactoring",
            Self::Testing => "testing",
            Self::Verifying => "verifying",
            Self::Writing => "writing",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentCoordinationPresenceRecord {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub trace_id: String,
    pub lineage_kind: String,
    pub parent_run_id: Option<String>,
    pub parent_subagent_id: Option<String>,
    pub role: Option<String>,
    pub pane_id: Option<String>,
    pub status: String,
    pub current_phase: String,
    pub activity_summary: String,
    pub last_event_id: Option<i64>,
    pub last_event_kind: Option<String>,
    pub started_at: String,
    pub last_heartbeat_at: String,
    pub updated_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentCoordinationEventRecord {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub trace_id: String,
    pub event_kind: String,
    pub summary: String,
    pub payload: JsonValue,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentFileReservationRecord {
    pub reservation_id: String,
    pub project_id: String,
    pub path: String,
    pub path_kind: String,
    pub operation: AgentCoordinationReservationOperation,
    pub owner_agent_session_id: String,
    pub owner_run_id: String,
    pub owner_child_run_id: Option<String>,
    pub owner_role: Option<String>,
    pub owner_pane_id: Option<String>,
    pub owner_trace_id: String,
    pub note: Option<String>,
    pub override_reason: Option<String>,
    pub claimed_at: String,
    pub last_heartbeat_at: String,
    pub expires_at: String,
    pub released_at: Option<String>,
    pub release_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentFileReservationConflictRecord {
    pub requested_path: String,
    pub reservation: AgentFileReservationRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentFileReservationClaimResult {
    pub claimed: Vec<AgentFileReservationRecord>,
    pub conflicts: Vec<AgentFileReservationConflictRecord>,
    pub override_recorded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpsertAgentCoordinationPresenceRecord {
    pub project_id: String,
    pub run_id: String,
    pub pane_id: Option<String>,
    pub status: String,
    pub current_phase: String,
    pub activity_summary: String,
    pub last_event_id: Option<i64>,
    pub last_event_kind: Option<String>,
    pub updated_at: String,
    pub lease_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAgentCoordinationEventRecord {
    pub project_id: String,
    pub run_id: String,
    pub event_kind: String,
    pub summary: String,
    pub payload: JsonValue,
    pub created_at: String,
    pub lease_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimAgentFileReservationRequest {
    pub project_id: String,
    pub owner_run_id: String,
    pub paths: Vec<String>,
    pub operation: AgentCoordinationReservationOperation,
    pub note: Option<String>,
    pub override_reason: Option<String>,
    pub claimed_at: String,
    pub lease_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAgentFileReservationRequest {
    pub project_id: String,
    pub owner_run_id: String,
    pub reservation_id: Option<String>,
    pub paths: Vec<String>,
    pub release_reason: String,
    pub released_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentCoordinationContext {
    pub presence: Vec<AgentCoordinationPresenceRecord>,
    pub reservations: Vec<AgentFileReservationRecord>,
    pub events: Vec<AgentCoordinationEventRecord>,
    pub mailbox: Vec<super::AgentMailboxDeliveryRecord>,
}

pub fn upsert_agent_coordination_presence(
    repo_root: &Path,
    record: &UpsertAgentCoordinationPresenceRecord,
) -> CommandResult<AgentCoordinationPresenceRecord> {
    validate_presence_record(record)?;
    let expires_at = timestamp_plus_seconds(
        &record.updated_at,
        record
            .lease_seconds
            .unwrap_or(DEFAULT_PRESENCE_LEASE_SECONDS),
    );
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let inserted = connection
        .execute(
            r#"
            INSERT INTO agent_coordination_presence (
                project_id,
                agent_session_id,
                run_id,
                trace_id,
                lineage_kind,
                parent_run_id,
                parent_subagent_id,
                role,
                pane_id,
                status,
                current_phase,
                activity_summary,
                last_event_id,
                last_event_kind,
                started_at,
                last_heartbeat_at,
                updated_at,
                expires_at
            )
            SELECT
                ?1,
                agent_runs.agent_session_id,
                agent_runs.run_id,
                agent_runs.trace_id,
                agent_runs.lineage_kind,
                agent_runs.parent_run_id,
                agent_runs.parent_subagent_id,
                COALESCE(agent_runs.subagent_role, agent_runs.runtime_agent_id),
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8,
                COALESCE(
                    (SELECT started_at
                     FROM agent_coordination_presence
                     WHERE project_id = ?1 AND run_id = ?2),
                    agent_runs.started_at
                ),
                ?9,
                ?9,
                ?10
            FROM agent_runs
            WHERE agent_runs.project_id = ?1
              AND agent_runs.run_id = ?2
            ON CONFLICT(project_id, run_id) DO UPDATE SET
                agent_session_id = excluded.agent_session_id,
                trace_id = excluded.trace_id,
                lineage_kind = excluded.lineage_kind,
                parent_run_id = excluded.parent_run_id,
                parent_subagent_id = excluded.parent_subagent_id,
                role = excluded.role,
                pane_id = COALESCE(excluded.pane_id, agent_coordination_presence.pane_id),
                status = excluded.status,
                current_phase = excluded.current_phase,
                activity_summary = excluded.activity_summary,
                last_event_id = excluded.last_event_id,
                last_event_kind = excluded.last_event_kind,
                last_heartbeat_at = excluded.last_heartbeat_at,
                updated_at = excluded.updated_at,
                expires_at = excluded.expires_at
            "#,
            params![
                record.project_id,
                record.run_id,
                record.pane_id,
                record.status,
                record.current_phase,
                record.activity_summary,
                record.last_event_id,
                record.last_event_kind,
                record.updated_at,
                expires_at,
            ],
        )
        .map_err(|error| {
            map_coordination_write_error(
                &database_path,
                "agent_coordination_presence_upsert_failed",
                error,
            )
        })?;
    if inserted == 0 {
        return Err(CommandError::system_fault(
            "agent_coordination_run_missing",
            format!(
                "Xero could not publish coordination presence for run `{}` in project `{}` because the run was not found.",
                record.run_id, record.project_id
            ),
        ));
    }
    read_presence(&connection, repo_root, &record.project_id, &record.run_id)
}

pub fn append_agent_coordination_event(
    repo_root: &Path,
    record: &NewAgentCoordinationEventRecord,
) -> CommandResult<AgentCoordinationEventRecord> {
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_coordination_event_invalid",
    )?;
    validate_non_empty_text(&record.run_id, "runId", "agent_coordination_event_invalid")?;
    validate_non_empty_text(
        &record.event_kind,
        "eventKind",
        "agent_coordination_event_invalid",
    )?;
    validate_non_empty_text(
        &record.summary,
        "summary",
        "agent_coordination_event_invalid",
    )?;
    let payload_json = json_string(&record.payload, "payload")?;
    let expires_at = timestamp_plus_seconds(
        &record.created_at,
        record.lease_seconds.unwrap_or(DEFAULT_EVENT_LEASE_SECONDS),
    );
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let inserted = connection
        .execute(
            r#"
            INSERT INTO agent_coordination_events (
                project_id,
                run_id,
                trace_id,
                event_kind,
                summary,
                payload_json,
                created_at,
                expires_at
            )
            SELECT
                ?1,
                ?2,
                agent_runs.trace_id,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7
            FROM agent_runs
            WHERE agent_runs.project_id = ?1
              AND agent_runs.run_id = ?2
            "#,
            params![
                record.project_id,
                record.run_id,
                record.event_kind,
                record.summary,
                payload_json,
                record.created_at,
                expires_at,
            ],
        )
        .map_err(|error| {
            map_coordination_write_error(
                &database_path,
                "agent_coordination_event_insert_failed",
                error,
            )
        })?;
    if inserted == 0 {
        return Err(CommandError::system_fault(
            "agent_coordination_run_missing",
            format!(
                "Xero could not record coordination event `{}` for missing run `{}` in project `{}`.",
                record.event_kind, record.run_id, record.project_id
            ),
        ));
    }
    let id = connection.last_insert_rowid();
    read_coordination_event(&connection, repo_root, id)
}

pub fn list_active_agent_coordination_presence(
    repo_root: &Path,
    project_id: &str,
    current_run_id: Option<&str>,
    now: &str,
    limit: usize,
) -> CommandResult<Vec<AgentCoordinationPresenceRecord>> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_coordination_request_invalid",
    )?;
    cleanup_expired_agent_coordination(repo_root, project_id, now)?;
    let limit = bounded_limit(limit, MAX_COORDINATION_CONTEXT_PRESENCE);
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                agent_session_id,
                run_id,
                trace_id,
                lineage_kind,
                parent_run_id,
                parent_subagent_id,
                role,
                pane_id,
                status,
                current_phase,
                activity_summary,
                last_event_id,
                last_event_kind,
                started_at,
                last_heartbeat_at,
                updated_at,
                expires_at
            FROM agent_coordination_presence
            WHERE project_id = ?1
              AND expires_at > ?2
              AND (?3 IS NULL OR run_id <> ?3)
            ORDER BY updated_at DESC, run_id ASC
            LIMIT ?4
            "#,
        )
        .map_err(|error| {
            map_coordination_query_error(
                &database_path,
                "agent_coordination_presence_prepare_failed",
                error,
            )
        })?;
    let rows = statement
        .query_map(
            params![project_id, now, current_run_id, limit as i64],
            read_presence_row,
        )
        .map_err(|error| {
            map_coordination_query_error(
                &database_path,
                "agent_coordination_presence_query_failed",
                error,
            )
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_coordination_query_error(
            &database_path,
            "agent_coordination_presence_decode_failed",
            error,
        )
    })
}

pub fn list_active_agent_file_reservations(
    repo_root: &Path,
    project_id: &str,
    current_run_id: Option<&str>,
    now: &str,
    limit: usize,
) -> CommandResult<Vec<AgentFileReservationRecord>> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_coordination_request_invalid",
    )?;
    cleanup_expired_agent_coordination(repo_root, project_id, now)?;
    let limit = bounded_limit(limit, MAX_COORDINATION_CONTEXT_RESERVATIONS);
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                reservation_id,
                project_id,
                path,
                path_kind,
                operation,
                owner_agent_session_id,
                owner_run_id,
                owner_child_run_id,
                owner_role,
                owner_pane_id,
                owner_trace_id,
                note,
                override_reason,
                claimed_at,
                last_heartbeat_at,
                expires_at,
                released_at,
                release_reason
            FROM agent_file_reservations
            WHERE project_id = ?1
              AND released_at IS NULL
              AND expires_at > ?2
              AND (?3 IS NULL OR owner_run_id <> ?3)
            ORDER BY claimed_at DESC, reservation_id ASC
            LIMIT ?4
            "#,
        )
        .map_err(|error| {
            map_coordination_query_error(
                &database_path,
                "agent_file_reservations_prepare_failed",
                error,
            )
        })?;
    let rows = statement
        .query_map(
            params![project_id, now, current_run_id, limit as i64],
            read_reservation_row,
        )
        .map_err(|error| {
            map_coordination_query_error(
                &database_path,
                "agent_file_reservations_query_failed",
                error,
            )
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_coordination_query_error(
            &database_path,
            "agent_file_reservations_decode_failed",
            error,
        )
    })
}

pub fn list_recent_agent_coordination_events(
    repo_root: &Path,
    project_id: &str,
    current_run_id: Option<&str>,
    now: &str,
    limit: usize,
) -> CommandResult<Vec<AgentCoordinationEventRecord>> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_coordination_request_invalid",
    )?;
    cleanup_expired_agent_coordination(repo_root, project_id, now)?;
    let limit = bounded_limit(limit, MAX_COORDINATION_CONTEXT_EVENTS);
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, project_id, run_id, trace_id, event_kind, summary, payload_json, created_at, expires_at
            FROM agent_coordination_events
            WHERE project_id = ?1
              AND expires_at > ?2
              AND (?3 IS NULL OR run_id <> ?3)
            ORDER BY created_at DESC, id DESC
            LIMIT ?4
            "#,
        )
        .map_err(|error| map_coordination_query_error(&database_path, "agent_coordination_events_prepare_failed", error))?;
    let rows = statement
        .query_map(
            params![project_id, now, current_run_id, limit as i64],
            read_event_row,
        )
        .map_err(|error| {
            map_coordination_query_error(
                &database_path,
                "agent_coordination_events_query_failed",
                error,
            )
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_coordination_query_error(
            &database_path,
            "agent_coordination_events_decode_failed",
            error,
        )
    })
}

pub fn active_agent_coordination_context(
    repo_root: &Path,
    project_id: &str,
    current_run_id: &str,
    now: &str,
) -> CommandResult<AgentCoordinationContext> {
    Ok(AgentCoordinationContext {
        presence: list_active_agent_coordination_presence(
            repo_root,
            project_id,
            Some(current_run_id),
            now,
            MAX_COORDINATION_CONTEXT_PRESENCE,
        )?,
        reservations: list_active_agent_file_reservations(
            repo_root,
            project_id,
            Some(current_run_id),
            now,
            MAX_COORDINATION_CONTEXT_RESERVATIONS,
        )?,
        events: list_recent_agent_coordination_events(
            repo_root,
            project_id,
            Some(current_run_id),
            now,
            MAX_COORDINATION_CONTEXT_EVENTS,
        )?,
        mailbox: super::active_agent_mailbox_context(repo_root, project_id, current_run_id, now)?,
    })
}

pub fn check_agent_file_reservation_conflicts(
    repo_root: &Path,
    project_id: &str,
    owner_run_id: &str,
    paths: &[String],
    now: &str,
) -> CommandResult<Vec<AgentFileReservationConflictRecord>> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_coordination_request_invalid",
    )?;
    validate_non_empty_text(
        owner_run_id,
        "ownerRunId",
        "agent_coordination_request_invalid",
    )?;
    let normalized_paths = normalize_reservation_paths(paths)?;
    cleanup_expired_agent_coordination(repo_root, project_id, now)?;
    let active = list_active_agent_file_reservations(repo_root, project_id, None, now, 1_000)?;
    let mut conflicts = Vec::new();
    for requested_path in normalized_paths {
        for reservation in &active {
            if reservation.owner_run_id == owner_run_id
                || reservation.owner_child_run_id.as_deref() == Some(owner_run_id)
            {
                continue;
            }
            if coordination_paths_overlap(&requested_path, &reservation.path) {
                conflicts.push(AgentFileReservationConflictRecord {
                    requested_path: requested_path.clone(),
                    reservation: reservation.clone(),
                });
            }
        }
    }
    Ok(conflicts)
}

pub fn has_active_agent_file_reservation_for_paths(
    repo_root: &Path,
    project_id: &str,
    owner_run_id: &str,
    paths: &[String],
    now: &str,
) -> CommandResult<bool> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_coordination_request_invalid",
    )?;
    validate_non_empty_text(
        owner_run_id,
        "ownerRunId",
        "agent_coordination_request_invalid",
    )?;
    let paths = normalize_reservation_paths(paths)?;
    if paths.is_empty() {
        return Ok(true);
    }
    cleanup_expired_agent_coordination(repo_root, project_id, now)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let reservations = reservations_for_owner(
        &connection,
        repo_root,
        project_id,
        owner_run_id,
        None,
        &paths,
    )?;
    Ok(paths.iter().all(|path| {
        reservations
            .iter()
            .any(|reservation| coordination_paths_overlap(path, &reservation.path))
    }))
}

pub fn claim_agent_file_reservations(
    repo_root: &Path,
    request: &ClaimAgentFileReservationRequest,
) -> CommandResult<AgentFileReservationClaimResult> {
    validate_claim_request(request)?;
    let paths = normalize_reservation_paths(&request.paths)?;
    let conflicts = check_agent_file_reservation_conflicts(
        repo_root,
        &request.project_id,
        &request.owner_run_id,
        &paths,
        &request.claimed_at,
    )?;
    if !conflicts.is_empty() && request.override_reason.is_none() {
        return Ok(AgentFileReservationClaimResult {
            claimed: Vec::new(),
            conflicts,
            override_recorded: false,
        });
    }

    let expires_at = timestamp_plus_seconds(
        &request.claimed_at,
        request
            .lease_seconds
            .unwrap_or(DEFAULT_RESERVATION_LEASE_SECONDS),
    );
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let mut claimed = Vec::with_capacity(paths.len());
    for path in paths {
        let reservation_id = generate_reservation_id();
        let inserted = connection
            .execute(
                r#"
                INSERT INTO agent_file_reservations (
                    reservation_id,
                    project_id,
                    path,
                    path_kind,
                    operation,
                    owner_agent_session_id,
                    owner_run_id,
                    owner_child_run_id,
                    owner_role,
                    owner_pane_id,
                    owner_trace_id,
                    note,
                    override_reason,
                    claimed_at,
                    last_heartbeat_at,
                    expires_at
                )
                SELECT
                    ?1,
                    ?2,
                    ?3,
                    ?4,
                    ?5,
                    agent_runs.agent_session_id,
                    COALESCE(agent_runs.parent_run_id, agent_runs.run_id),
                    CASE
                        WHEN agent_runs.lineage_kind = 'subagent_child'
                        THEN agent_runs.run_id
                        ELSE NULL
                    END,
                    COALESCE(agent_runs.subagent_role, agent_runs.runtime_agent_id),
                    NULL,
                    agent_runs.trace_id,
                    ?6,
                    ?7,
                    ?8,
                    ?8,
                    ?9
                FROM agent_runs
                WHERE agent_runs.project_id = ?2
                  AND agent_runs.run_id = ?10
                "#,
                params![
                    reservation_id,
                    request.project_id,
                    path,
                    "path",
                    request.operation.as_str(),
                    request.note.as_deref(),
                    request.override_reason.as_deref(),
                    request.claimed_at,
                    expires_at,
                    request.owner_run_id,
                ],
            )
            .map_err(|error| {
                map_coordination_write_error(
                    &database_path,
                    "agent_file_reservation_insert_failed",
                    error,
                )
            })?;
        if inserted == 0 {
            return Err(CommandError::system_fault(
                "agent_coordination_run_missing",
                format!(
                    "Xero could not claim a file reservation for missing run `{}` in project `{}`.",
                    request.owner_run_id, request.project_id
                ),
            ));
        }
        claimed.push(read_reservation(
            &connection,
            repo_root,
            &request.project_id,
            &reservation_id,
        )?);
    }

    if !conflicts.is_empty() && request.override_reason.is_some() {
        append_agent_coordination_event(
            repo_root,
            &NewAgentCoordinationEventRecord {
                project_id: request.project_id.clone(),
                run_id: request.owner_run_id.clone(),
                event_kind: "reservation_override".into(),
                summary: "File reservation conflict override recorded.".into(),
                payload: json!({
                    "overrideReason": request.override_reason.as_deref(),
                    "claimedReservationIds": claimed.iter().map(|reservation| reservation.reservation_id.as_str()).collect::<Vec<_>>(),
                    "conflicts": conflicts.clone(),
                }),
                created_at: request.claimed_at.clone(),
                lease_seconds: Some(DEFAULT_EVENT_LEASE_SECONDS),
            },
        )?;
    }

    Ok(AgentFileReservationClaimResult {
        claimed,
        conflicts,
        override_recorded: request.override_reason.is_some(),
    })
}

pub fn release_agent_file_reservations(
    repo_root: &Path,
    request: &ReleaseAgentFileReservationRequest,
) -> CommandResult<Vec<AgentFileReservationRecord>> {
    validate_non_empty_text(
        &request.project_id,
        "projectId",
        "agent_coordination_release_invalid",
    )?;
    validate_non_empty_text(
        &request.owner_run_id,
        "ownerRunId",
        "agent_coordination_release_invalid",
    )?;
    validate_non_empty_text(
        &request.release_reason,
        "releaseReason",
        "agent_coordination_release_invalid",
    )?;
    let paths = normalize_reservation_paths(&request.paths)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let before = reservations_for_owner(
        &connection,
        repo_root,
        &request.project_id,
        &request.owner_run_id,
        request.reservation_id.as_deref(),
        &paths,
    )?;
    if before.is_empty() {
        return Ok(Vec::new());
    }
    for reservation in &before {
        connection
            .execute(
                r#"
                UPDATE agent_file_reservations
                SET released_at = ?3,
                    release_reason = ?4
                WHERE project_id = ?1
                  AND reservation_id = ?2
                  AND released_at IS NULL
                "#,
                params![
                    request.project_id,
                    reservation.reservation_id,
                    request.released_at,
                    request.release_reason,
                ],
            )
            .map_err(|error| {
                map_coordination_write_error(
                    &database_path,
                    "agent_file_reservation_release_failed",
                    error,
                )
            })?;
    }
    before
        .iter()
        .map(|reservation| {
            read_reservation(
                &connection,
                repo_root,
                &request.project_id,
                &reservation.reservation_id,
            )
        })
        .collect()
}

pub fn heartbeat_agent_coordination(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    timestamp: &str,
) -> CommandResult<()> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_coordination_request_invalid",
    )?;
    validate_non_empty_text(run_id, "runId", "agent_coordination_request_invalid")?;
    let presence_expires_at = timestamp_plus_seconds(timestamp, DEFAULT_PRESENCE_LEASE_SECONDS);
    let reservation_expires_at =
        timestamp_plus_seconds(timestamp, DEFAULT_RESERVATION_LEASE_SECONDS);
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .execute(
            r#"
            UPDATE agent_coordination_presence
            SET last_heartbeat_at = ?3,
                updated_at = ?3,
                expires_at = ?4
            WHERE project_id = ?1
              AND run_id = ?2
            "#,
            params![project_id, run_id, timestamp, presence_expires_at],
        )
        .map_err(|error| {
            map_coordination_write_error(
                &database_path,
                "agent_coordination_presence_heartbeat_failed",
                error,
            )
        })?;
    connection
        .execute(
            r#"
            UPDATE agent_file_reservations
            SET last_heartbeat_at = ?3,
                expires_at = ?4
            WHERE project_id = ?1
              AND (owner_run_id = ?2 OR owner_child_run_id = ?2)
              AND released_at IS NULL
            "#,
            params![project_id, run_id, timestamp, reservation_expires_at],
        )
        .map_err(|error| {
            map_coordination_write_error(
                &database_path,
                "agent_file_reservation_heartbeat_failed",
                error,
            )
        })?;
    Ok(())
}

pub fn cleanup_agent_coordination_for_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    reason: &str,
    timestamp: &str,
) -> CommandResult<()> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_coordination_cleanup_invalid",
    )?;
    validate_non_empty_text(run_id, "runId", "agent_coordination_cleanup_invalid")?;
    validate_non_empty_text(reason, "reason", "agent_coordination_cleanup_invalid")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .execute(
            r#"
            UPDATE agent_file_reservations
            SET released_at = ?4,
                release_reason = ?3
            WHERE project_id = ?1
              AND (owner_run_id = ?2 OR owner_child_run_id = ?2)
              AND released_at IS NULL
            "#,
            params![project_id, run_id, reason, timestamp],
        )
        .map_err(|error| {
            map_coordination_write_error(
                &database_path,
                "agent_file_reservation_cleanup_failed",
                error,
            )
        })?;
    connection
        .execute(
            r#"
            DELETE FROM agent_coordination_presence
            WHERE project_id = ?1
              AND (run_id = ?2 OR parent_run_id = ?2)
            "#,
            params![project_id, run_id],
        )
        .map_err(|error| {
            map_coordination_write_error(
                &database_path,
                "agent_coordination_presence_cleanup_failed",
                error,
            )
        })?;
    Ok(())
}

pub fn cleanup_expired_agent_coordination(
    repo_root: &Path,
    project_id: &str,
    now: &str,
) -> CommandResult<()> {
    validate_non_empty_text(project_id, "projectId", "agent_coordination_gc_invalid")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .execute(
            r#"
            DELETE FROM agent_coordination_presence
            WHERE project_id = ?1
              AND expires_at <= ?2
            "#,
            params![project_id, now],
        )
        .map_err(|error| {
            map_coordination_write_error(
                &database_path,
                "agent_coordination_presence_gc_failed",
                error,
            )
        })?;
    connection
        .execute(
            r#"
            DELETE FROM agent_coordination_events
            WHERE project_id = ?1
              AND expires_at <= ?2
            "#,
            params![project_id, now],
        )
        .map_err(|error| {
            map_coordination_write_error(
                &database_path,
                "agent_coordination_events_gc_failed",
                error,
            )
        })?;
    connection
        .execute(
            r#"
            UPDATE agent_file_reservations
            SET released_at = expires_at,
                release_reason = 'expired'
            WHERE project_id = ?1
              AND released_at IS NULL
              AND expires_at <= ?2
            "#,
            params![project_id, now],
        )
        .map_err(|error| {
            map_coordination_write_error(&database_path, "agent_file_reservations_gc_failed", error)
        })?;
    super::cleanup_expired_agent_mailbox(repo_root, project_id, now)?;
    Ok(())
}

pub fn coordination_paths_overlap(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|rest| rest.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn validate_presence_record(record: &UpsertAgentCoordinationPresenceRecord) -> CommandResult<()> {
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_coordination_presence_invalid",
    )?;
    validate_non_empty_text(
        &record.run_id,
        "runId",
        "agent_coordination_presence_invalid",
    )?;
    validate_non_empty_text(
        &record.status,
        "status",
        "agent_coordination_presence_invalid",
    )?;
    validate_non_empty_text(
        &record.current_phase,
        "currentPhase",
        "agent_coordination_presence_invalid",
    )?;
    validate_non_empty_text(
        &record.activity_summary,
        "activitySummary",
        "agent_coordination_presence_invalid",
    )?;
    validate_non_empty_text(
        &record.updated_at,
        "updatedAt",
        "agent_coordination_presence_invalid",
    )
}

fn validate_claim_request(request: &ClaimAgentFileReservationRequest) -> CommandResult<()> {
    validate_non_empty_text(
        &request.project_id,
        "projectId",
        "agent_file_reservation_claim_invalid",
    )?;
    validate_non_empty_text(
        &request.owner_run_id,
        "ownerRunId",
        "agent_file_reservation_claim_invalid",
    )?;
    validate_non_empty_text(
        &request.claimed_at,
        "claimedAt",
        "agent_file_reservation_claim_invalid",
    )?;
    if request.paths.is_empty() {
        return Err(CommandError::user_fixable(
            "agent_file_reservation_paths_required",
            "Xero requires at least one repo-relative path for a file reservation.",
        ));
    }
    Ok(())
}

fn normalize_reservation_paths(paths: &[String]) -> CommandResult<Vec<String>> {
    let mut normalized = BTreeSet::new();
    for path in paths {
        let path = normalize_reservation_path(path)?;
        normalized.insert(path);
    }
    Ok(normalized.into_iter().collect())
}

fn normalize_reservation_path(path: &str) -> CommandResult<String> {
    let trimmed = path.trim().trim_matches('/');
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed.starts_with("../")
        || trimmed.contains("/../")
        || trimmed.contains('\0')
        || trimmed.starts_with('~')
        || Path::new(trimmed).is_absolute()
    {
        return Err(CommandError::user_fixable(
            "agent_file_reservation_path_invalid",
            format!("Xero refused the unsafe file reservation path `{path}`."),
        ));
    }
    let mut parts = Vec::new();
    for part in trimmed.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                return Err(CommandError::user_fixable(
                    "agent_file_reservation_path_invalid",
                    format!("Xero refused the unsafe file reservation path `{path}`."),
                ));
            }
            value => parts.push(value),
        }
    }
    if parts.is_empty() {
        return Err(CommandError::user_fixable(
            "agent_file_reservation_path_invalid",
            format!("Xero refused the unsafe file reservation path `{path}`."),
        ));
    }
    Ok(parts.join("/"))
}

fn reservations_for_owner(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    owner_run_id: &str,
    reservation_id: Option<&str>,
    paths: &[String],
) -> CommandResult<Vec<AgentFileReservationRecord>> {
    let database_path = database_path_for_repo(repo_root);
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                reservation_id,
                project_id,
                path,
                path_kind,
                operation,
                owner_agent_session_id,
                owner_run_id,
                owner_child_run_id,
                owner_role,
                owner_pane_id,
                owner_trace_id,
                note,
                override_reason,
                claimed_at,
                last_heartbeat_at,
                expires_at,
                released_at,
                release_reason
            FROM agent_file_reservations
            WHERE project_id = ?1
              AND released_at IS NULL
              AND (?2 IS NULL OR reservation_id = ?2)
              AND (owner_run_id = ?3 OR owner_child_run_id = ?3)
            ORDER BY claimed_at DESC, reservation_id ASC
            "#,
        )
        .map_err(|error| {
            map_coordination_query_error(
                &database_path,
                "agent_file_reservation_owner_prepare_failed",
                error,
            )
        })?;
    let rows = statement
        .query_map(
            params![project_id, reservation_id, owner_run_id],
            read_reservation_row,
        )
        .map_err(|error| {
            map_coordination_query_error(
                &database_path,
                "agent_file_reservation_owner_query_failed",
                error,
            )
        })?;
    let mut reservations = rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_coordination_query_error(
            &database_path,
            "agent_file_reservation_owner_decode_failed",
            error,
        )
    })?;
    if !paths.is_empty() {
        reservations.retain(|reservation| {
            paths
                .iter()
                .any(|path| coordination_paths_overlap(path, &reservation.path))
        });
    }
    Ok(reservations)
}

fn read_presence(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> CommandResult<AgentCoordinationPresenceRecord> {
    let database_path = database_path_for_repo(repo_root);
    connection
        .query_row(
            r#"
            SELECT
                project_id,
                agent_session_id,
                run_id,
                trace_id,
                lineage_kind,
                parent_run_id,
                parent_subagent_id,
                role,
                pane_id,
                status,
                current_phase,
                activity_summary,
                last_event_id,
                last_event_kind,
                started_at,
                last_heartbeat_at,
                updated_at,
                expires_at
            FROM agent_coordination_presence
            WHERE project_id = ?1
              AND run_id = ?2
            "#,
            params![project_id, run_id],
            read_presence_row,
        )
        .map_err(|error| {
            map_coordination_query_error(
                &database_path,
                "agent_coordination_presence_read_failed",
                error,
            )
        })
}

fn read_coordination_event(
    connection: &Connection,
    repo_root: &Path,
    id: i64,
) -> CommandResult<AgentCoordinationEventRecord> {
    let database_path = database_path_for_repo(repo_root);
    connection
        .query_row(
            r#"
            SELECT id, project_id, run_id, trace_id, event_kind, summary, payload_json, created_at, expires_at
            FROM agent_coordination_events
            WHERE id = ?1
            "#,
            params![id],
            read_event_row,
        )
        .map_err(|error| map_coordination_query_error(&database_path, "agent_coordination_event_read_failed", error))
}

fn read_reservation(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    reservation_id: &str,
) -> CommandResult<AgentFileReservationRecord> {
    let database_path = database_path_for_repo(repo_root);
    connection
        .query_row(
            r#"
            SELECT
                reservation_id,
                project_id,
                path,
                path_kind,
                operation,
                owner_agent_session_id,
                owner_run_id,
                owner_child_run_id,
                owner_role,
                owner_pane_id,
                owner_trace_id,
                note,
                override_reason,
                claimed_at,
                last_heartbeat_at,
                expires_at,
                released_at,
                release_reason
            FROM agent_file_reservations
            WHERE project_id = ?1
              AND reservation_id = ?2
            "#,
            params![project_id, reservation_id],
            read_reservation_row,
        )
        .map_err(|error| {
            map_coordination_query_error(
                &database_path,
                "agent_file_reservation_read_failed",
                error,
            )
        })
}

fn read_presence_row(row: &Row<'_>) -> rusqlite::Result<AgentCoordinationPresenceRecord> {
    Ok(AgentCoordinationPresenceRecord {
        project_id: row.get(0)?,
        agent_session_id: row.get(1)?,
        run_id: row.get(2)?,
        trace_id: row.get(3)?,
        lineage_kind: row.get(4)?,
        parent_run_id: row.get(5)?,
        parent_subagent_id: row.get(6)?,
        role: row.get(7)?,
        pane_id: row.get(8)?,
        status: row.get(9)?,
        current_phase: row.get(10)?,
        activity_summary: row.get(11)?,
        last_event_id: row.get(12)?,
        last_event_kind: row.get(13)?,
        started_at: row.get(14)?,
        last_heartbeat_at: row.get(15)?,
        updated_at: row.get(16)?,
        expires_at: row.get(17)?,
    })
}

fn read_event_row(row: &Row<'_>) -> rusqlite::Result<AgentCoordinationEventRecord> {
    let payload_json: String = row.get(6)?;
    let payload = serde_json::from_str(&payload_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(AgentCoordinationEventRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        run_id: row.get(2)?,
        trace_id: row.get(3)?,
        event_kind: row.get(4)?,
        summary: row.get(5)?,
        payload,
        created_at: row.get(7)?,
        expires_at: row.get(8)?,
    })
}

fn read_reservation_row(row: &Row<'_>) -> rusqlite::Result<AgentFileReservationRecord> {
    let operation: String = row.get(4)?;
    Ok(AgentFileReservationRecord {
        reservation_id: row.get(0)?,
        project_id: row.get(1)?,
        path: row.get(2)?,
        path_kind: row.get(3)?,
        operation: parse_reservation_operation(&operation),
        owner_agent_session_id: row.get(5)?,
        owner_run_id: row.get(6)?,
        owner_child_run_id: row.get(7)?,
        owner_role: row.get(8)?,
        owner_pane_id: row.get(9)?,
        owner_trace_id: row.get(10)?,
        note: row.get(11)?,
        override_reason: row.get(12)?,
        claimed_at: row.get(13)?,
        last_heartbeat_at: row.get(14)?,
        expires_at: row.get(15)?,
        released_at: row.get(16)?,
        release_reason: row.get(17)?,
    })
}

fn parse_reservation_operation(value: &str) -> AgentCoordinationReservationOperation {
    match value {
        "observing" => AgentCoordinationReservationOperation::Observing,
        "refactoring" => AgentCoordinationReservationOperation::Refactoring,
        "testing" => AgentCoordinationReservationOperation::Testing,
        "verifying" => AgentCoordinationReservationOperation::Verifying,
        "writing" => AgentCoordinationReservationOperation::Writing,
        _ => AgentCoordinationReservationOperation::Editing,
    }
}

fn bounded_limit(limit: usize, max_limit: usize) -> usize {
    limit.clamp(1, max_limit)
}

fn timestamp_plus_seconds(timestamp: &str, seconds: i64) -> String {
    let base = OffsetDateTime::parse(timestamp, &Rfc3339)
        .unwrap_or_else(|_| OffsetDateTime::parse(&now_timestamp(), &Rfc3339).unwrap());
    (base + Duration::seconds(seconds.max(1)))
        .format(&Rfc3339)
        .expect("rfc3339 timestamp formatting should succeed")
}

fn generate_reservation_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "file-reservation-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn json_string(value: &JsonValue, field: &'static str) -> CommandResult<String> {
    serde_json::to_string(value).map_err(|error| {
        CommandError::system_fault(
            "agent_coordination_json_encode_failed",
            format!("Xero could not encode {field} coordination JSON: {error}"),
        )
    })
}

fn map_coordination_query_error(
    database_path: &Path,
    code: &'static str,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::retryable(
        code,
        format!(
            "Xero could not query agent coordination state in {}: {error}",
            database_path.display()
        ),
    )
}

fn map_coordination_write_error(
    database_path: &Path,
    code: &'static str,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::retryable(
        code,
        format!(
            "Xero could not update agent coordination state in {}: {error}",
            database_path.display()
        ),
    )
}
