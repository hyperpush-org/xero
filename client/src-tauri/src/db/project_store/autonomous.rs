use std::{collections::HashMap, path::Path};

use rusqlite::{params, Connection, Error as SqlError, Transaction};
use serde::{Deserialize, Serialize};

use crate::{
    commands::CommandError,
    db::database_path_for_repo,
    runtime::protocol::{
        BrowserComputerUseActionStatus, BrowserComputerUseSurface, GitToolResultScope,
        McpCapabilityKind, ToolResultSummary,
    },
};

use super::runtime::{
    decode_runtime_run_bool, decode_runtime_run_checkpoint_sequence,
    decode_runtime_run_optional_non_empty_text, decode_runtime_run_reason,
    find_prohibited_runtime_persistence_content, map_runtime_run_commit_error,
    map_runtime_run_decode_error, map_runtime_run_transaction_error, map_runtime_run_write_error,
    read_runtime_run_row, require_runtime_run_non_empty_owned, RuntimeRunDiagnosticRecord,
};
use super::{
    compute_workflow_handoff_package_hash, open_runtime_database, read_project_row,
    read_transition_event_by_transition_id, read_workflow_handoff_package_by_transition_id,
    validate_non_empty_text, validate_workflow_handoff_package_hash,
    validate_workflow_handoff_package_transition_linkage,
};

const MAX_AUTONOMOUS_HISTORY_UNIT_ROWS: i64 = 16;
const MAX_AUTONOMOUS_HISTORY_ATTEMPT_ROWS: i64 = 32;
const MAX_AUTONOMOUS_HISTORY_ARTIFACT_ROWS: i64 = 64;
const AUTONOMOUS_ARTIFACT_KIND_TOOL_RESULT: &str = "tool_result";
const AUTONOMOUS_ARTIFACT_KIND_VERIFICATION_EVIDENCE: &str = "verification_evidence";
const AUTONOMOUS_ARTIFACT_KIND_POLICY_DENIED: &str = "policy_denied";
const AUTONOMOUS_ARTIFACT_KIND_SKILL_LIFECYCLE: &str = "skill_lifecycle";
const MAX_BROWSER_COMPUTER_USE_SUMMARY_TEXT_CHARS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousRunStatus {
    Starting,
    Running,
    Paused,
    Cancelling,
    Cancelled,
    Stale,
    Failed,
    Stopped,
    Crashed,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousUnitKind {
    Researcher,
    Planner,
    Executor,
    Verifier,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousUnitStatus {
    Pending,
    Active,
    Blocked,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousUnitArtifactStatus {
    Pending,
    Recorded,
    Rejected,
    Redacted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousToolCallStateRecord {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousVerificationOutcomeRecord {
    Passed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousArtifactCommandResultRecord {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolResultPayloadRecord {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub tool_state: AutonomousToolCallStateRecord,
    pub command_result: Option<AutonomousArtifactCommandResultRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_summary: Option<ToolResultSummary>,
    pub action_id: Option<String>,
    pub boundary_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousVerificationEvidencePayloadRecord {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub evidence_kind: String,
    pub label: String,
    pub outcome: AutonomousVerificationOutcomeRecord,
    pub command_result: Option<AutonomousArtifactCommandResultRecord>,
    pub action_id: Option<String>,
    pub boundary_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousPolicyDeniedPayloadRecord {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub diagnostic_code: String,
    pub message: String,
    pub tool_name: Option<String>,
    pub action_id: Option<String>,
    pub boundary_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSkillLifecycleStageRecord {
    Discovery,
    Install,
    Invoke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSkillLifecycleResultRecord {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSkillCacheStatusRecord {
    Miss,
    Hit,
    Refreshed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillLifecycleSourceRecord {
    pub repo: String,
    pub path: String,
    pub reference: String,
    pub tree_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillLifecycleCacheRecord {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AutonomousSkillCacheStatusRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillLifecycleDiagnosticRecord {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillLifecyclePayloadRecord {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub stage: AutonomousSkillLifecycleStageRecord,
    pub result: AutonomousSkillLifecycleResultRecord,
    pub skill_id: String,
    pub source: AutonomousSkillLifecycleSourceRecord,
    pub cache: AutonomousSkillLifecycleCacheRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<AutonomousSkillLifecycleDiagnosticRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutonomousArtifactPayloadRecord {
    ToolResult(AutonomousToolResultPayloadRecord),
    VerificationEvidence(AutonomousVerificationEvidencePayloadRecord),
    PolicyDenied(AutonomousPolicyDeniedPayloadRecord),
    SkillLifecycle(AutonomousSkillLifecyclePayloadRecord),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousRunRecord {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub runtime_kind: String,
    pub provider_id: String,
    pub supervisor_kind: String,
    pub status: AutonomousRunStatus,
    pub active_unit_sequence: Option<u32>,
    pub duplicate_start_detected: bool,
    pub duplicate_start_run_id: Option<String>,
    pub duplicate_start_reason: Option<String>,
    pub started_at: String,
    pub last_heartbeat_at: Option<String>,
    pub last_checkpoint_at: Option<String>,
    pub paused_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub completed_at: Option<String>,
    pub crashed_at: Option<String>,
    pub stopped_at: Option<String>,
    pub pause_reason: Option<RuntimeRunDiagnosticRecord>,
    pub cancel_reason: Option<RuntimeRunDiagnosticRecord>,
    pub crash_reason: Option<RuntimeRunDiagnosticRecord>,
    pub last_error: Option<RuntimeRunDiagnosticRecord>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousWorkflowLinkageRecord {
    pub workflow_node_id: String,
    pub transition_id: String,
    pub causal_transition_id: Option<String>,
    pub handoff_transition_id: String,
    pub handoff_package_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousUnitRecord {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub sequence: u32,
    pub kind: AutonomousUnitKind,
    pub status: AutonomousUnitStatus,
    pub summary: String,
    pub boundary_id: Option<String>,
    pub workflow_linkage: Option<AutonomousWorkflowLinkageRecord>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub updated_at: String,
    pub last_error: Option<RuntimeRunDiagnosticRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousUnitAttemptRecord {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub attempt_number: u32,
    pub child_session_id: String,
    pub status: AutonomousUnitStatus,
    pub boundary_id: Option<String>,
    pub workflow_linkage: Option<AutonomousWorkflowLinkageRecord>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub updated_at: String,
    pub last_error: Option<RuntimeRunDiagnosticRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousUnitArtifactRecord {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub artifact_kind: String,
    pub status: AutonomousUnitArtifactStatus,
    pub summary: String,
    pub content_hash: Option<String>,
    pub payload: Option<AutonomousArtifactPayloadRecord>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousUnitHistoryRecord {
    pub unit: AutonomousUnitRecord,
    pub latest_attempt: Option<AutonomousUnitAttemptRecord>,
    pub artifacts: Vec<AutonomousUnitArtifactRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousRunUpsertRecord {
    pub run: AutonomousRunRecord,
    pub unit: Option<AutonomousUnitRecord>,
    pub attempt: Option<AutonomousUnitAttemptRecord>,
    pub artifacts: Vec<AutonomousUnitArtifactRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousRunSnapshotRecord {
    pub run: AutonomousRunRecord,
    pub unit: Option<AutonomousUnitRecord>,
    pub attempt: Option<AutonomousUnitAttemptRecord>,
    pub history: Vec<AutonomousUnitHistoryRecord>,
}

#[derive(Debug)]
struct RawAutonomousRunRow {
    project_id: String,
    agent_session_id: String,
    run_id: String,
    runtime_kind: String,
    provider_id: String,
    supervisor_kind: String,
    status: String,
    active_unit_sequence: Option<i64>,
    duplicate_start_detected: i64,
    duplicate_start_run_id: Option<String>,
    duplicate_start_reason: Option<String>,
    started_at: String,
    last_heartbeat_at: Option<String>,
    last_checkpoint_at: Option<String>,
    paused_at: Option<String>,
    cancelled_at: Option<String>,
    completed_at: Option<String>,
    crashed_at: Option<String>,
    stopped_at: Option<String>,
    pause_reason_code: Option<String>,
    pause_reason_message: Option<String>,
    cancel_reason_code: Option<String>,
    cancel_reason_message: Option<String>,
    crash_reason_code: Option<String>,
    crash_reason_message: Option<String>,
    last_error_code: Option<String>,
    last_error_message: Option<String>,
    updated_at: String,
}

#[derive(Debug)]
struct RawAutonomousUnitRow {
    project_id: String,
    run_id: String,
    unit_id: String,
    sequence: i64,
    kind: String,
    status: String,
    summary: String,
    boundary_id: Option<String>,
    workflow_node_id: Option<String>,
    workflow_transition_id: Option<String>,
    workflow_causal_transition_id: Option<String>,
    workflow_handoff_transition_id: Option<String>,
    workflow_handoff_package_hash: Option<String>,
    started_at: String,
    finished_at: Option<String>,
    last_error_code: Option<String>,
    last_error_message: Option<String>,
    updated_at: String,
}

#[derive(Debug)]
struct RawAutonomousUnitAttemptRow {
    project_id: String,
    run_id: String,
    unit_id: String,
    attempt_id: String,
    attempt_number: i64,
    child_session_id: String,
    status: String,
    boundary_id: Option<String>,
    workflow_node_id: Option<String>,
    workflow_transition_id: Option<String>,
    workflow_causal_transition_id: Option<String>,
    workflow_handoff_transition_id: Option<String>,
    workflow_handoff_package_hash: Option<String>,
    started_at: String,
    finished_at: Option<String>,
    last_error_code: Option<String>,
    last_error_message: Option<String>,
    updated_at: String,
}

#[derive(Debug)]
struct RawAutonomousUnitArtifactRow {
    project_id: String,
    run_id: String,
    unit_id: String,
    attempt_id: String,
    artifact_id: String,
    artifact_kind: String,
    status: String,
    summary: String,
    content_hash: Option<String>,
    payload_json: Option<String>,
    created_at: String,
    updated_at: String,
}

pub fn load_autonomous_run(
    repo_root: &Path,
    expected_project_id: &str,
    expected_agent_session_id: &str,
) -> Result<Option<AutonomousRunSnapshotRecord>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, expected_project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_runtime_run_transaction_error(
            "autonomous_run_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the durable autonomous-run read transaction.",
        )
    })?;

    let snapshot = read_autonomous_run_snapshot(
        &transaction,
        &database_path,
        expected_project_id,
        expected_agent_session_id,
    )?;
    transaction.rollback().map_err(|error| {
        map_runtime_run_commit_error(
            "autonomous_run_commit_failed",
            &database_path,
            error,
            "Cadence could not close the durable autonomous-run read transaction.",
        )
    })?;

    Ok(snapshot)
}

pub fn upsert_autonomous_run(
    repo_root: &Path,
    payload: &AutonomousRunUpsertRecord,
) -> Result<AutonomousRunSnapshotRecord, CommandError> {
    let payload = normalize_autonomous_run_upsert_payload(payload)?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(
        &connection,
        &database_path,
        repo_root,
        &payload.run.project_id,
    )?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_runtime_run_transaction_error(
            "autonomous_run_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the durable autonomous-run transaction.",
        )
    })?;

    let runtime_row = read_runtime_run_row(
        &transaction,
        &database_path,
        &payload.run.project_id,
        &payload.run.agent_session_id,
    )?
    .ok_or_else(|| {
            CommandError::retryable(
                "autonomous_run_missing_runtime_row",
                format!(
                    "Cadence could not persist autonomous-run metadata in {} because the selected project has no durable runtime-run row.",
                    database_path.display()
                ),
            )
        })?;

    if runtime_row.run_id != payload.run.run_id {
        return Err(CommandError::retryable(
            "autonomous_run_mismatch",
            format!(
                "Cadence refused to persist autonomous-run metadata for run `{}` because the durable runtime-run row currently points at `{}`.",
                payload.run.run_id, runtime_row.run_id
            ),
        ));
    }

    if runtime_row.runtime_kind != payload.run.runtime_kind
        || runtime_row.provider_id != payload.run.provider_id
    {
        return Err(CommandError::retryable(
            "autonomous_run_mismatch",
            format!(
                "Cadence refused to persist autonomous-run metadata for run `{}` because the durable runtime-run identity is `{}`/`{}` instead of `{}`/`{}`.",
                payload.run.run_id,
                runtime_row.provider_id,
                runtime_row.runtime_kind,
                payload.run.provider_id,
                payload.run.runtime_kind
            ),
        ));
    }

    let active_unit_sequence = payload.run.active_unit_sequence.map(i64::from);
    let duplicate_start_detected = if payload.run.duplicate_start_detected {
        1
    } else {
        0
    };
    let pause_reason_code = payload
        .run
        .pause_reason
        .as_ref()
        .map(|reason| reason.code.as_str());
    let pause_reason_message = payload
        .run
        .pause_reason
        .as_ref()
        .map(|reason| reason.message.as_str());
    let cancel_reason_code = payload
        .run
        .cancel_reason
        .as_ref()
        .map(|reason| reason.code.as_str());
    let cancel_reason_message = payload
        .run
        .cancel_reason
        .as_ref()
        .map(|reason| reason.message.as_str());
    let crash_reason_code = payload
        .run
        .crash_reason
        .as_ref()
        .map(|reason| reason.code.as_str());
    let crash_reason_message = payload
        .run
        .crash_reason
        .as_ref()
        .map(|reason| reason.message.as_str());
    let last_error_code = payload
        .run
        .last_error
        .as_ref()
        .map(|reason| reason.code.as_str());
    let last_error_message = payload
        .run
        .last_error
        .as_ref()
        .map(|reason| reason.message.as_str());

    transaction
        .execute(
            r#"
            INSERT INTO autonomous_runs (
                project_id,
                agent_session_id,
                run_id,
                runtime_kind,
                provider_id,
                supervisor_kind,
                status,
                active_unit_sequence,
                duplicate_start_detected,
                duplicate_start_run_id,
                duplicate_start_reason,
                started_at,
                last_heartbeat_at,
                last_checkpoint_at,
                paused_at,
                cancelled_at,
                completed_at,
                crashed_at,
                stopped_at,
                pause_reason_code,
                pause_reason_message,
                cancel_reason_code,
                cancel_reason_message,
                crash_reason_code,
                crash_reason_message,
                last_error_code,
                last_error_message,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)
            ON CONFLICT(project_id, agent_session_id) DO UPDATE SET
                run_id = excluded.run_id,
                runtime_kind = excluded.runtime_kind,
                provider_id = excluded.provider_id,
                supervisor_kind = excluded.supervisor_kind,
                status = excluded.status,
                active_unit_sequence = excluded.active_unit_sequence,
                duplicate_start_detected = excluded.duplicate_start_detected,
                duplicate_start_run_id = excluded.duplicate_start_run_id,
                duplicate_start_reason = excluded.duplicate_start_reason,
                started_at = excluded.started_at,
                last_heartbeat_at = excluded.last_heartbeat_at,
                last_checkpoint_at = excluded.last_checkpoint_at,
                paused_at = excluded.paused_at,
                cancelled_at = excluded.cancelled_at,
                completed_at = excluded.completed_at,
                crashed_at = excluded.crashed_at,
                stopped_at = excluded.stopped_at,
                pause_reason_code = excluded.pause_reason_code,
                pause_reason_message = excluded.pause_reason_message,
                cancel_reason_code = excluded.cancel_reason_code,
                cancel_reason_message = excluded.cancel_reason_message,
                crash_reason_code = excluded.crash_reason_code,
                crash_reason_message = excluded.crash_reason_message,
                last_error_code = excluded.last_error_code,
                last_error_message = excluded.last_error_message,
                updated_at = excluded.updated_at
            "#,
            params![
                payload.run.project_id.as_str(),
                payload.run.agent_session_id.as_str(),
                payload.run.run_id.as_str(),
                payload.run.runtime_kind.as_str(),
                payload.run.provider_id.as_str(),
                payload.run.supervisor_kind.as_str(),
                autonomous_run_status_sql_value(&payload.run.status),
                active_unit_sequence,
                duplicate_start_detected,
                payload.run.duplicate_start_run_id.as_deref(),
                payload.run.duplicate_start_reason.as_deref(),
                payload.run.started_at.as_str(),
                payload.run.last_heartbeat_at.as_deref(),
                payload.run.last_checkpoint_at.as_deref(),
                payload.run.paused_at.as_deref(),
                payload.run.cancelled_at.as_deref(),
                payload.run.completed_at.as_deref(),
                payload.run.crashed_at.as_deref(),
                payload.run.stopped_at.as_deref(),
                pause_reason_code,
                pause_reason_message,
                cancel_reason_code,
                cancel_reason_message,
                crash_reason_code,
                crash_reason_message,
                last_error_code,
                last_error_message,
                payload.run.updated_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_runtime_run_write_error(
                "autonomous_run_persist_failed",
                &database_path,
                error,
                "Cadence could not persist durable autonomous-run metadata.",
            )
        })?;

    let open_unit = read_open_autonomous_unit(
        &transaction,
        &database_path,
        &payload.run.project_id,
        &payload.run.run_id,
    )?;
    let open_attempt = read_open_autonomous_unit_attempt(
        &transaction,
        &database_path,
        &payload.run.project_id,
        &payload.run.run_id,
    )?;
    let rollover_timestamp = payload
        .attempt
        .as_ref()
        .map(|attempt| attempt.started_at.as_str())
        .or_else(|| payload.unit.as_ref().map(|unit| unit.started_at.as_str()))
        .unwrap_or(payload.run.updated_at.as_str());

    if let Some(unit) = payload.unit.as_ref() {
        close_superseded_autonomous_unit_attempt(
            &transaction,
            &database_path,
            open_attempt.as_ref(),
            payload.attempt.as_ref(),
            &payload.run.status,
            rollover_timestamp,
        )?;
        close_superseded_autonomous_unit(
            &transaction,
            &database_path,
            open_unit.as_ref(),
            unit,
            &payload.run.status,
            rollover_timestamp,
        )?;

        persist_autonomous_unit(&transaction, &database_path, unit)?;
        if let Some(linkage) = unit.workflow_linkage.as_ref() {
            validate_autonomous_workflow_linkage_record(
                &transaction,
                &database_path,
                &payload.run.project_id,
                linkage,
                "unit",
                &unit.unit_id,
                "autonomous_run_request_invalid",
            )?;
        }
    }

    if let Some(attempt) = payload.attempt.as_ref() {
        persist_autonomous_unit_attempt(&transaction, &database_path, attempt)?;
        if let Some(linkage) = attempt.workflow_linkage.as_ref() {
            validate_autonomous_workflow_linkage_record(
                &transaction,
                &database_path,
                &payload.run.project_id,
                linkage,
                "attempt",
                &attempt.attempt_id,
                "autonomous_run_request_invalid",
            )?;
        }
    }

    for artifact in &payload.artifacts {
        persist_autonomous_unit_artifact(&transaction, &database_path, artifact)?;
    }

    transaction.commit().map_err(|error| {
        map_runtime_run_commit_error(
            "autonomous_run_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the durable autonomous-run transaction.",
        )
    })?;

    read_autonomous_run_snapshot(
        &connection,
        &database_path,
        &payload.run.project_id,
        &payload.run.agent_session_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "autonomous_run_missing_after_persist",
            format!(
                "Cadence persisted durable autonomous-run metadata in {} but could not read it back.",
                database_path.display()
            ),
        )
    })
}

fn read_open_autonomous_unit(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Option<AutonomousUnitRecord>, CommandError> {
    let mut open_units = read_autonomous_units(connection, database_path, project_id, run_id)?
        .into_iter()
        .filter(|unit| autonomous_unit_status_is_open(&unit.status))
        .collect::<Vec<_>>();

    if open_units.len() > 1 {
        return Err(CommandError::system_fault(
            "autonomous_unit_conflict",
            format!(
                "Cadence refused to persist autonomous unit rollover because run `{run_id}` already has {} open durable unit rows in {}.",
                open_units.len(),
                database_path.display()
            ),
        ));
    }

    Ok(open_units.pop())
}

fn read_open_autonomous_unit_attempt(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Option<AutonomousUnitAttemptRecord>, CommandError> {
    let mut open_attempts =
        read_autonomous_unit_attempts(connection, database_path, project_id, run_id)?
            .into_iter()
            .filter(|attempt| autonomous_unit_status_is_open(&attempt.status))
            .collect::<Vec<_>>();

    if open_attempts.len() > 1 {
        return Err(CommandError::system_fault(
            "autonomous_unit_attempt_conflict",
            format!(
                "Cadence refused to persist autonomous attempt rollover because run `{run_id}` already has {} open durable attempt rows in {}.",
                open_attempts.len(),
                database_path.display()
            ),
        ));
    }

    Ok(open_attempts.pop())
}

fn close_superseded_autonomous_unit(
    transaction: &Transaction<'_>,
    database_path: &Path,
    existing: Option<&AutonomousUnitRecord>,
    incoming: &AutonomousUnitRecord,
    run_status: &AutonomousRunStatus,
    closed_at: &str,
) -> Result<(), CommandError> {
    let Some(existing) = existing else {
        return Ok(());
    };
    if existing.unit_id == incoming.unit_id {
        return Ok(());
    }
    if existing.boundary_id.is_some() {
        return Err(CommandError::user_fixable(
            "autonomous_unit_boundary_drift",
            format!(
                "Cadence refused to roll durable autonomous unit `{}` to `{}` because the existing unit is still attached to boundary `{}`.",
                existing.unit_id,
                incoming.unit_id,
                existing.boundary_id.as_deref().unwrap_or_default()
            ),
        ));
    }

    transaction
        .execute(
            r#"
            UPDATE autonomous_units
            SET status = ?1,
                finished_at = COALESCE(finished_at, ?2),
                updated_at = ?3
            WHERE project_id = ?4
              AND run_id = ?5
              AND unit_id = ?6
            "#,
            params![
                autonomous_unit_status_sql_value(&rollover_autonomous_unit_status(run_status)),
                closed_at,
                closed_at,
                existing.project_id.as_str(),
                existing.run_id.as_str(),
                existing.unit_id.as_str(),
            ],
        )
        .map_err(|error| {
            map_runtime_run_write_error(
                "autonomous_unit_persist_failed",
                database_path,
                error,
                "Cadence could not close the superseded durable autonomous-unit row.",
            )
        })?;

    Ok(())
}

fn close_superseded_autonomous_unit_attempt(
    transaction: &Transaction<'_>,
    database_path: &Path,
    existing: Option<&AutonomousUnitAttemptRecord>,
    incoming: Option<&AutonomousUnitAttemptRecord>,
    run_status: &AutonomousRunStatus,
    closed_at: &str,
) -> Result<(), CommandError> {
    let Some(existing) = existing else {
        return Ok(());
    };
    let Some(incoming) = incoming else {
        return Ok(());
    };
    if existing.attempt_id == incoming.attempt_id {
        return Ok(());
    }
    if existing.boundary_id.is_some() {
        return Err(CommandError::user_fixable(
            "autonomous_unit_attempt_boundary_drift",
            format!(
                "Cadence refused to roll durable autonomous attempt `{}` to `{}` because the existing attempt is still attached to boundary `{}`.",
                existing.attempt_id,
                incoming.attempt_id,
                existing.boundary_id.as_deref().unwrap_or_default()
            ),
        ));
    }

    transaction
        .execute(
            r#"
            UPDATE autonomous_unit_attempts
            SET status = ?1,
                finished_at = COALESCE(finished_at, ?2),
                updated_at = ?3
            WHERE project_id = ?4
              AND run_id = ?5
              AND attempt_id = ?6
            "#,
            params![
                autonomous_unit_status_sql_value(&rollover_autonomous_unit_status(run_status)),
                closed_at,
                closed_at,
                existing.project_id.as_str(),
                existing.run_id.as_str(),
                existing.attempt_id.as_str(),
            ],
        )
        .map_err(|error| {
            map_runtime_run_write_error(
                "autonomous_unit_attempt_persist_failed",
                database_path,
                error,
                "Cadence could not close the superseded durable autonomous-attempt row.",
            )
        })?;

    Ok(())
}

fn autonomous_unit_status_is_open(status: &AutonomousUnitStatus) -> bool {
    matches!(
        status,
        AutonomousUnitStatus::Pending
            | AutonomousUnitStatus::Active
            | AutonomousUnitStatus::Blocked
            | AutonomousUnitStatus::Paused
    )
}

fn rollover_autonomous_unit_status(run_status: &AutonomousRunStatus) -> AutonomousUnitStatus {
    match run_status {
        AutonomousRunStatus::Cancelled => AutonomousUnitStatus::Cancelled,
        AutonomousRunStatus::Failed | AutonomousRunStatus::Crashed => AutonomousUnitStatus::Failed,
        _ => AutonomousUnitStatus::Completed,
    }
}

fn persist_autonomous_unit(
    transaction: &Transaction<'_>,
    database_path: &Path,
    unit: &AutonomousUnitRecord,
) -> Result<(), CommandError> {
    let (last_error_code, last_error_message) = unit
        .last_error
        .as_ref()
        .map(|error| (Some(error.code.as_str()), Some(error.message.as_str())))
        .unwrap_or((None, None));

    let (
        workflow_node_id,
        workflow_transition_id,
        workflow_causal_transition_id,
        workflow_handoff_transition_id,
        workflow_handoff_package_hash,
    ) = unit
        .workflow_linkage
        .as_ref()
        .map(|linkage| {
            (
                Some(linkage.workflow_node_id.as_str()),
                Some(linkage.transition_id.as_str()),
                linkage.causal_transition_id.as_deref(),
                Some(linkage.handoff_transition_id.as_str()),
                Some(linkage.handoff_package_hash.as_str()),
            )
        })
        .unwrap_or((None, None, None, None, None));

    transaction
        .execute(
            r#"
            INSERT INTO autonomous_units (
                unit_id,
                project_id,
                run_id,
                sequence,
                kind,
                status,
                summary,
                boundary_id,
                workflow_node_id,
                workflow_transition_id,
                workflow_causal_transition_id,
                workflow_handoff_transition_id,
                workflow_handoff_package_hash,
                started_at,
                finished_at,
                last_error_code,
                last_error_message,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
            ON CONFLICT(unit_id) DO UPDATE SET
                sequence = excluded.sequence,
                kind = excluded.kind,
                status = excluded.status,
                summary = excluded.summary,
                boundary_id = excluded.boundary_id,
                workflow_node_id = excluded.workflow_node_id,
                workflow_transition_id = excluded.workflow_transition_id,
                workflow_causal_transition_id = excluded.workflow_causal_transition_id,
                workflow_handoff_transition_id = excluded.workflow_handoff_transition_id,
                workflow_handoff_package_hash = excluded.workflow_handoff_package_hash,
                started_at = excluded.started_at,
                finished_at = excluded.finished_at,
                last_error_code = excluded.last_error_code,
                last_error_message = excluded.last_error_message,
                updated_at = excluded.updated_at
            "#,
            params![
                unit.unit_id.as_str(),
                unit.project_id.as_str(),
                unit.run_id.as_str(),
                i64::from(unit.sequence),
                autonomous_unit_kind_sql_value(&unit.kind),
                autonomous_unit_status_sql_value(&unit.status),
                unit.summary.as_str(),
                unit.boundary_id.as_deref(),
                workflow_node_id,
                workflow_transition_id,
                workflow_causal_transition_id,
                workflow_handoff_transition_id,
                workflow_handoff_package_hash,
                unit.started_at.as_str(),
                unit.finished_at.as_deref(),
                last_error_code,
                last_error_message,
                unit.updated_at.as_str(),
            ],
        )
        .map_err(|error| {
            if matches!(error, SqlError::SqliteFailure(_, _)) {
                return CommandError::system_fault(
                    "autonomous_unit_conflict",
                    format!(
                        "Cadence refused to persist autonomous unit `{}` because it would violate the one-active-unit invariant in {}: {error}",
                        unit.unit_id,
                        database_path.display()
                    ),
                );
            }

            map_runtime_run_write_error(
                "autonomous_unit_persist_failed",
                database_path,
                error,
                "Cadence could not persist the durable autonomous-unit row.",
            )
        })?;

    Ok(())
}

fn persist_autonomous_unit_attempt(
    transaction: &Transaction<'_>,
    database_path: &Path,
    attempt: &AutonomousUnitAttemptRecord,
) -> Result<(), CommandError> {
    let existing = read_autonomous_unit_attempt_by_id(
        transaction,
        database_path,
        &attempt.project_id,
        &attempt.run_id,
        &attempt.attempt_id,
    )?;
    if let Some(existing) = existing.as_ref() {
        if existing == attempt {
            return Ok(());
        }

        if matches!(
            existing.status,
            AutonomousUnitStatus::Completed
                | AutonomousUnitStatus::Cancelled
                | AutonomousUnitStatus::Failed
        ) {
            return Err(CommandError::system_fault(
                "autonomous_unit_attempt_immutable",
                format!(
                    "Cadence refused to mutate completed autonomous attempt `{}` in {}.",
                    attempt.attempt_id,
                    database_path.display()
                ),
            ));
        }
    }

    let (last_error_code, last_error_message) = attempt
        .last_error
        .as_ref()
        .map(|error| (Some(error.code.as_str()), Some(error.message.as_str())))
        .unwrap_or((None, None));

    let (
        workflow_node_id,
        workflow_transition_id,
        workflow_causal_transition_id,
        workflow_handoff_transition_id,
        workflow_handoff_package_hash,
    ) = attempt
        .workflow_linkage
        .as_ref()
        .map(|linkage| {
            (
                Some(linkage.workflow_node_id.as_str()),
                Some(linkage.transition_id.as_str()),
                linkage.causal_transition_id.as_deref(),
                Some(linkage.handoff_transition_id.as_str()),
                Some(linkage.handoff_package_hash.as_str()),
            )
        })
        .unwrap_or((None, None, None, None, None));

    transaction
        .execute(
            r#"
            INSERT INTO autonomous_unit_attempts (
                attempt_id,
                project_id,
                run_id,
                unit_id,
                attempt_number,
                child_session_id,
                status,
                boundary_id,
                workflow_node_id,
                workflow_transition_id,
                workflow_causal_transition_id,
                workflow_handoff_transition_id,
                workflow_handoff_package_hash,
                started_at,
                finished_at,
                last_error_code,
                last_error_message,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
            ON CONFLICT(attempt_id) DO UPDATE SET
                attempt_number = excluded.attempt_number,
                child_session_id = excluded.child_session_id,
                status = excluded.status,
                boundary_id = excluded.boundary_id,
                workflow_node_id = excluded.workflow_node_id,
                workflow_transition_id = excluded.workflow_transition_id,
                workflow_causal_transition_id = excluded.workflow_causal_transition_id,
                workflow_handoff_transition_id = excluded.workflow_handoff_transition_id,
                workflow_handoff_package_hash = excluded.workflow_handoff_package_hash,
                started_at = excluded.started_at,
                finished_at = excluded.finished_at,
                last_error_code = excluded.last_error_code,
                last_error_message = excluded.last_error_message,
                updated_at = excluded.updated_at
            "#,
            params![
                attempt.attempt_id.as_str(),
                attempt.project_id.as_str(),
                attempt.run_id.as_str(),
                attempt.unit_id.as_str(),
                i64::from(attempt.attempt_number),
                attempt.child_session_id.as_str(),
                autonomous_unit_status_sql_value(&attempt.status),
                attempt.boundary_id.as_deref(),
                workflow_node_id,
                workflow_transition_id,
                workflow_causal_transition_id,
                workflow_handoff_transition_id,
                workflow_handoff_package_hash,
                attempt.started_at.as_str(),
                attempt.finished_at.as_deref(),
                last_error_code,
                last_error_message,
                attempt.updated_at.as_str(),
            ],
        )
        .map_err(|error| {
            if matches!(error, SqlError::SqliteFailure(_, _)) {
                return CommandError::system_fault(
                    "autonomous_unit_attempt_conflict",
                    format!(
                        "Cadence refused to persist autonomous attempt `{}` because it would violate the active-attempt or parent-link invariants in {}: {error}",
                        attempt.attempt_id,
                        database_path.display()
                    ),
                );
            }

            map_runtime_run_write_error(
                "autonomous_unit_attempt_persist_failed",
                database_path,
                error,
                "Cadence could not persist the durable autonomous attempt row.",
            )
        })?;

    Ok(())
}

fn persist_autonomous_unit_artifact(
    transaction: &Transaction<'_>,
    database_path: &Path,
    artifact: &AutonomousUnitArtifactRecord,
) -> Result<(), CommandError> {
    let payload_json = artifact
        .payload
        .as_ref()
        .map(canonicalize_autonomous_artifact_payload_json)
        .transpose()?;

    transaction
        .execute(
            r#"
            INSERT INTO autonomous_unit_artifacts (
                artifact_id,
                project_id,
                run_id,
                unit_id,
                attempt_id,
                artifact_kind,
                status,
                summary,
                content_hash,
                payload_json,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(artifact_id) DO UPDATE SET
                artifact_kind = excluded.artifact_kind,
                status = excluded.status,
                summary = excluded.summary,
                content_hash = excluded.content_hash,
                payload_json = excluded.payload_json,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at
            "#,
            params![
                artifact.artifact_id.as_str(),
                artifact.project_id.as_str(),
                artifact.run_id.as_str(),
                artifact.unit_id.as_str(),
                artifact.attempt_id.as_str(),
                artifact.artifact_kind.as_str(),
                autonomous_unit_artifact_status_sql_value(&artifact.status),
                artifact.summary.as_str(),
                artifact.content_hash.as_deref(),
                payload_json.as_deref(),
                artifact.created_at.as_str(),
                artifact.updated_at.as_str(),
            ],
        )
        .map_err(|error| {
            if matches!(error, SqlError::SqliteFailure(_, _)) {
                return CommandError::system_fault(
                    "autonomous_unit_artifact_conflict",
                    format!(
                        "Cadence refused to persist autonomous artifact `{}` because its parent linkage is invalid in {}: {error}",
                        artifact.artifact_id,
                        database_path.display()
                    ),
                );
            }

            map_runtime_run_write_error(
                "autonomous_unit_artifact_persist_failed",
                database_path,
                error,
                "Cadence could not persist the durable autonomous artifact row.",
            )
        })?;

    Ok(())
}

fn read_autonomous_unit_attempt_by_id(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
    attempt_id: &str,
) -> Result<Option<AutonomousUnitAttemptRecord>, CommandError> {
    let row = connection.query_row(
        r#"
        SELECT
            project_id,
            run_id,
            unit_id,
            attempt_id,
            attempt_number,
            child_session_id,
            status,
            boundary_id,
            workflow_node_id,
            workflow_transition_id,
            workflow_causal_transition_id,
            workflow_handoff_transition_id,
            workflow_handoff_package_hash,
            started_at,
            finished_at,
            last_error_code,
            last_error_message,
            updated_at
        FROM autonomous_unit_attempts
        WHERE project_id = ?1
          AND run_id = ?2
          AND attempt_id = ?3
        "#,
        params![project_id, run_id, attempt_id],
        |row| {
            Ok(RawAutonomousUnitAttemptRow {
                project_id: row.get(0)?,
                run_id: row.get(1)?,
                unit_id: row.get(2)?,
                attempt_id: row.get(3)?,
                attempt_number: row.get(4)?,
                child_session_id: row.get(5)?,
                status: row.get(6)?,
                boundary_id: row.get(7)?,
                workflow_node_id: row.get(8)?,
                workflow_transition_id: row.get(9)?,
                workflow_causal_transition_id: row.get(10)?,
                workflow_handoff_transition_id: row.get(11)?,
                workflow_handoff_package_hash: row.get(12)?,
                started_at: row.get(13)?,
                finished_at: row.get(14)?,
                last_error_code: row.get(15)?,
                last_error_message: row.get(16)?,
                updated_at: row.get(17)?,
            })
        },
    );

    match row {
        Ok(row) => Ok(Some(decode_autonomous_unit_attempt_row(
            row,
            database_path,
        )?)),
        Err(SqlError::QueryReturnedNoRows) => Ok(None),
        Err(other) => Err(CommandError::system_fault(
            "autonomous_unit_attempt_query_failed",
            format!(
                "Cadence could not read autonomous attempt `{attempt_id}` from {}: {other}",
                database_path.display()
            ),
        )),
    }
}

fn read_autonomous_run_snapshot(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
    expected_agent_session_id: &str,
) -> Result<Option<AutonomousRunSnapshotRecord>, CommandError> {
    let row = connection.query_row(
        r#"
            SELECT
                project_id,
                agent_session_id,
                run_id,
                runtime_kind,
                provider_id,
                supervisor_kind,
                status,
                active_unit_sequence,
                duplicate_start_detected,
                duplicate_start_run_id,
                duplicate_start_reason,
                started_at,
                last_heartbeat_at,
                last_checkpoint_at,
                paused_at,
                cancelled_at,
                completed_at,
                crashed_at,
                stopped_at,
                pause_reason_code,
                pause_reason_message,
                cancel_reason_code,
                cancel_reason_message,
                crash_reason_code,
                crash_reason_message,
                last_error_code,
                last_error_message,
                updated_at
            FROM autonomous_runs
            WHERE project_id = ?1
              AND agent_session_id = ?2
            "#,
        params![expected_project_id, expected_agent_session_id],
        |row| {
            Ok(RawAutonomousRunRow {
                project_id: row.get(0)?,
                agent_session_id: row.get(1)?,
                run_id: row.get(2)?,
                runtime_kind: row.get(3)?,
                provider_id: row.get(4)?,
                supervisor_kind: row.get(5)?,
                status: row.get(6)?,
                active_unit_sequence: row.get(7)?,
                duplicate_start_detected: row.get(8)?,
                duplicate_start_run_id: row.get(9)?,
                duplicate_start_reason: row.get(10)?,
                started_at: row.get(11)?,
                last_heartbeat_at: row.get(12)?,
                last_checkpoint_at: row.get(13)?,
                paused_at: row.get(14)?,
                cancelled_at: row.get(15)?,
                completed_at: row.get(16)?,
                crashed_at: row.get(17)?,
                stopped_at: row.get(18)?,
                pause_reason_code: row.get(19)?,
                pause_reason_message: row.get(20)?,
                cancel_reason_code: row.get(21)?,
                cancel_reason_message: row.get(22)?,
                crash_reason_code: row.get(23)?,
                crash_reason_message: row.get(24)?,
                last_error_code: row.get(25)?,
                last_error_message: row.get(26)?,
                updated_at: row.get(27)?,
            })
        },
    );

    let raw_row = match row {
        Ok(row) => row,
        Err(SqlError::QueryReturnedNoRows) => return Ok(None),
        Err(other) => {
            return Err(CommandError::system_fault(
                "autonomous_run_query_failed",
                format!(
                    "Cadence could not read durable autonomous-run metadata from {}: {other}",
                    database_path.display()
                ),
            ))
        }
    };

    let run = decode_autonomous_run_row(raw_row, database_path)?;
    let units = read_autonomous_units(connection, database_path, expected_project_id, &run.run_id)?;
    let attempts =
        read_autonomous_unit_attempts(connection, database_path, expected_project_id, &run.run_id)?;
    let artifacts = read_autonomous_unit_artifacts(
        connection,
        database_path,
        expected_project_id,
        &run.run_id,
    )?;
    let history = build_autonomous_unit_history(database_path, &run, units, attempts, artifacts)?;

    let unit = history
        .iter()
        .find(|entry| {
            matches!(
                entry.unit.status,
                AutonomousUnitStatus::Active
                    | AutonomousUnitStatus::Blocked
                    | AutonomousUnitStatus::Paused
            )
        })
        .or_else(|| history.first())
        .map(|entry| entry.unit.clone());
    let attempt = unit.as_ref().and_then(|unit| {
        history
            .iter()
            .find(|entry| entry.unit.unit_id == unit.unit_id)
            .and_then(|entry| entry.latest_attempt.clone())
    });

    if let (Some(active_unit_sequence), Some(unit)) = (run.active_unit_sequence, unit.as_ref()) {
        if active_unit_sequence != unit.sequence {
            return Err(map_runtime_run_decode_error(
                database_path,
                format!(
                    "Autonomous run active_unit_sequence {} does not match durable unit `{}` sequence {}.",
                    active_unit_sequence, unit.unit_id, unit.sequence
                ),
            ));
        }
    }

    Ok(Some(AutonomousRunSnapshotRecord {
        run,
        unit,
        attempt,
        history,
    }))
}

fn read_autonomous_units(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<AutonomousUnitRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                run_id,
                unit_id,
                sequence,
                kind,
                status,
                summary,
                boundary_id,
                workflow_node_id,
                workflow_transition_id,
                workflow_causal_transition_id,
                workflow_handoff_transition_id,
                workflow_handoff_package_hash,
                started_at,
                finished_at,
                last_error_code,
                last_error_message,
                updated_at
            FROM autonomous_units
            WHERE project_id = ?1
              AND run_id = ?2
            ORDER BY sequence DESC, updated_at DESC, unit_id ASC
            LIMIT ?3
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "autonomous_unit_query_failed",
                format!(
                    "Cadence could not prepare the durable autonomous-unit query against {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let rows = statement
        .query_map(
            params![project_id, run_id, MAX_AUTONOMOUS_HISTORY_UNIT_ROWS],
            |row| {
                Ok(RawAutonomousUnitRow {
                    project_id: row.get(0)?,
                    run_id: row.get(1)?,
                    unit_id: row.get(2)?,
                    sequence: row.get(3)?,
                    kind: row.get(4)?,
                    status: row.get(5)?,
                    summary: row.get(6)?,
                    boundary_id: row.get(7)?,
                    workflow_node_id: row.get(8)?,
                    workflow_transition_id: row.get(9)?,
                    workflow_causal_transition_id: row.get(10)?,
                    workflow_handoff_transition_id: row.get(11)?,
                    workflow_handoff_package_hash: row.get(12)?,
                    started_at: row.get(13)?,
                    finished_at: row.get(14)?,
                    last_error_code: row.get(15)?,
                    last_error_message: row.get(16)?,
                    updated_at: row.get(17)?,
                })
            },
        )
        .map_err(|error| {
            CommandError::system_fault(
                "autonomous_unit_query_failed",
                format!(
                    "Cadence could not query durable autonomous-unit rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut units = Vec::new();
    let mut last_sequence = u32::MAX;
    for row in rows {
        let unit = decode_autonomous_unit_row(
            row.map_err(|error| {
                CommandError::system_fault(
                    "autonomous_unit_query_failed",
                    format!(
                        "Cadence could not read a durable autonomous-unit row from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            database_path,
        )?;

        if let Some(linkage) = unit.workflow_linkage.as_ref() {
            validate_autonomous_workflow_linkage_record(
                connection,
                database_path,
                project_id,
                linkage,
                "unit",
                &unit.unit_id,
                "runtime_run_decode_failed",
            )?;
        }

        if !units.is_empty() && unit.sequence >= last_sequence {
            return Err(map_runtime_run_decode_error(
                database_path,
                format!(
                    "Autonomous unit sequences must decrease strictly in bounded history order, but sequence {} followed {}.",
                    unit.sequence, last_sequence
                ),
            ));
        }

        last_sequence = unit.sequence;
        units.push(unit);
    }

    Ok(units)
}

fn read_autonomous_unit_attempts(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<AutonomousUnitAttemptRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                run_id,
                unit_id,
                attempt_id,
                attempt_number,
                child_session_id,
                status,
                boundary_id,
                workflow_node_id,
                workflow_transition_id,
                workflow_causal_transition_id,
                workflow_handoff_transition_id,
                workflow_handoff_package_hash,
                started_at,
                finished_at,
                last_error_code,
                last_error_message,
                updated_at
            FROM autonomous_unit_attempts
            WHERE project_id = ?1
              AND run_id = ?2
            ORDER BY attempt_number DESC, updated_at DESC, attempt_id ASC
            LIMIT ?3
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "autonomous_unit_attempt_query_failed",
                format!(
                    "Cadence could not prepare the durable autonomous attempt query against {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let rows = statement
        .query_map(
            params![project_id, run_id, MAX_AUTONOMOUS_HISTORY_ATTEMPT_ROWS],
            |row| {
                Ok(RawAutonomousUnitAttemptRow {
                    project_id: row.get(0)?,
                    run_id: row.get(1)?,
                    unit_id: row.get(2)?,
                    attempt_id: row.get(3)?,
                    attempt_number: row.get(4)?,
                    child_session_id: row.get(5)?,
                    status: row.get(6)?,
                    boundary_id: row.get(7)?,
                    workflow_node_id: row.get(8)?,
                    workflow_transition_id: row.get(9)?,
                    workflow_causal_transition_id: row.get(10)?,
                    workflow_handoff_transition_id: row.get(11)?,
                    workflow_handoff_package_hash: row.get(12)?,
                    started_at: row.get(13)?,
                    finished_at: row.get(14)?,
                    last_error_code: row.get(15)?,
                    last_error_message: row.get(16)?,
                    updated_at: row.get(17)?,
                })
            },
        )
        .map_err(|error| {
            CommandError::system_fault(
                "autonomous_unit_attempt_query_failed",
                format!(
                    "Cadence could not query durable autonomous attempts from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut attempts = Vec::new();
    for row in rows {
        let attempt = decode_autonomous_unit_attempt_row(
            row.map_err(|error| {
                CommandError::system_fault(
                    "autonomous_unit_attempt_query_failed",
                    format!(
                        "Cadence could not read a durable autonomous-attempt row from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            database_path,
        )?;

        if let Some(linkage) = attempt.workflow_linkage.as_ref() {
            validate_autonomous_workflow_linkage_record(
                connection,
                database_path,
                project_id,
                linkage,
                "attempt",
                &attempt.attempt_id,
                "runtime_run_decode_failed",
            )?;
        }

        attempts.push(attempt);
    }

    Ok(attempts)
}

fn read_autonomous_unit_artifacts(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<AutonomousUnitArtifactRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                run_id,
                unit_id,
                attempt_id,
                artifact_id,
                artifact_kind,
                status,
                summary,
                content_hash,
                payload_json,
                created_at,
                updated_at
            FROM autonomous_unit_artifacts
            WHERE project_id = ?1
              AND run_id = ?2
            ORDER BY created_at DESC, artifact_id ASC
            LIMIT ?3
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "autonomous_unit_artifact_query_failed",
                format!(
                    "Cadence could not prepare the durable autonomous artifact query against {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let rows = statement
        .query_map(
            params![project_id, run_id, MAX_AUTONOMOUS_HISTORY_ARTIFACT_ROWS],
            |row| {
                Ok(RawAutonomousUnitArtifactRow {
                    project_id: row.get(0)?,
                    run_id: row.get(1)?,
                    unit_id: row.get(2)?,
                    attempt_id: row.get(3)?,
                    artifact_id: row.get(4)?,
                    artifact_kind: row.get(5)?,
                    status: row.get(6)?,
                    summary: row.get(7)?,
                    content_hash: row.get(8)?,
                    payload_json: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            },
        )
        .map_err(|error| {
            CommandError::system_fault(
                "autonomous_unit_artifact_query_failed",
                format!(
                    "Cadence could not query durable autonomous artifacts from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut artifacts = Vec::new();
    for row in rows {
        artifacts.push(decode_autonomous_unit_artifact_row(
            row.map_err(|error| {
                CommandError::system_fault(
                    "autonomous_unit_artifact_query_failed",
                    format!(
                        "Cadence could not read a durable autonomous-artifact row from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            database_path,
        )?);
    }

    Ok(artifacts)
}

fn build_autonomous_unit_history(
    database_path: &Path,
    run: &AutonomousRunRecord,
    units: Vec<AutonomousUnitRecord>,
    attempts: Vec<AutonomousUnitAttemptRecord>,
    artifacts: Vec<AutonomousUnitArtifactRecord>,
) -> Result<Vec<AutonomousUnitHistoryRecord>, CommandError> {
    if units.is_empty() {
        return Err(map_runtime_run_decode_error(
            database_path,
            format!(
                "Autonomous run `{}` has no durable unit ledger rows.",
                run.run_id
            ),
        ));
    }

    let active_unit_count = units
        .iter()
        .filter(|unit| unit.status == AutonomousUnitStatus::Active)
        .count();
    if active_unit_count > 1 {
        return Err(map_runtime_run_decode_error(
            database_path,
            format!(
                "Autonomous run `{}` has {} active unit rows; expected at most one.",
                run.run_id, active_unit_count
            ),
        ));
    }

    let open_unit_count = units
        .iter()
        .filter(|unit| autonomous_unit_status_is_open(&unit.status))
        .count();
    if open_unit_count > 1 {
        return Err(map_runtime_run_decode_error(
            database_path,
            format!(
                "Autonomous run `{}` has {} open unit rows; expected at most one active, blocked, paused, or pending row.",
                run.run_id, open_unit_count
            ),
        ));
    }

    let active_attempt_count = attempts
        .iter()
        .filter(|attempt| attempt.status == AutonomousUnitStatus::Active)
        .count();
    if active_attempt_count > 1 {
        return Err(map_runtime_run_decode_error(
            database_path,
            format!(
                "Autonomous run `{}` has {} active attempt rows; expected at most one.",
                run.run_id, active_attempt_count
            ),
        ));
    }

    let open_attempt_count = attempts
        .iter()
        .filter(|attempt| autonomous_unit_status_is_open(&attempt.status))
        .count();
    if open_attempt_count > 1 {
        return Err(map_runtime_run_decode_error(
            database_path,
            format!(
                "Autonomous run `{}` has {} open attempt rows; expected at most one active, blocked, paused, or pending row.",
                run.run_id, open_attempt_count
            ),
        ));
    }

    let mut attempts_by_unit: HashMap<String, Vec<AutonomousUnitAttemptRecord>> = HashMap::new();
    for attempt in attempts {
        if !units.iter().any(|unit| unit.unit_id == attempt.unit_id) {
            return Err(map_runtime_run_decode_error(
                database_path,
                format!(
                    "Autonomous attempt `{}` points at missing durable unit `{}` for run `{}`.",
                    attempt.attempt_id, attempt.unit_id, run.run_id
                ),
            ));
        }
        attempts_by_unit
            .entry(attempt.unit_id.clone())
            .or_default()
            .push(attempt);
    }

    let mut artifacts_by_attempt: HashMap<String, Vec<AutonomousUnitArtifactRecord>> =
        HashMap::new();
    for artifact in artifacts {
        if !units.iter().any(|unit| unit.unit_id == artifact.unit_id) {
            return Err(map_runtime_run_decode_error(
                database_path,
                format!(
                    "Autonomous artifact `{}` points at missing durable unit `{}` for run `{}`.",
                    artifact.artifact_id, artifact.unit_id, run.run_id
                ),
            ));
        }

        let attempt_known = attempts_by_unit
            .get(&artifact.unit_id)
            .map(|attempts| {
                attempts
                    .iter()
                    .any(|attempt| attempt.attempt_id == artifact.attempt_id)
            })
            .unwrap_or(false);
        if !attempt_known {
            return Err(map_runtime_run_decode_error(
                database_path,
                format!(
                    "Autonomous artifact `{}` points at missing durable attempt `{}` for unit `{}`.",
                    artifact.artifact_id, artifact.attempt_id, artifact.unit_id
                ),
            ));
        }

        artifacts_by_attempt
            .entry(artifact.attempt_id.clone())
            .or_default()
            .push(artifact);
    }

    let mut history = Vec::new();
    for unit in units {
        let latest_attempt =
            attempts_by_unit
                .remove(&unit.unit_id)
                .and_then(|mut unit_attempts| {
                    unit_attempts.sort_by(|left, right| {
                        right
                            .attempt_number
                            .cmp(&left.attempt_number)
                            .then_with(|| right.updated_at.cmp(&left.updated_at))
                            .then_with(|| right.attempt_id.cmp(&left.attempt_id))
                    });
                    unit_attempts.into_iter().next()
                });

        if let Some(attempt) = latest_attempt.as_ref() {
            match (&unit.workflow_linkage, &attempt.workflow_linkage) {
                (None, None) => {}
                (Some(_), Some(_)) if unit.workflow_linkage == attempt.workflow_linkage => {}
                (None, Some(_)) => {
                    return Err(map_runtime_run_decode_error(
                        database_path,
                        format!(
                            "Autonomous attempt `{}` retained workflow linkage while parent unit `{}` did not.",
                            attempt.attempt_id, unit.unit_id
                        ),
                    ));
                }
                (Some(_), None) => {
                    return Err(map_runtime_run_decode_error(
                        database_path,
                        format!(
                            "Autonomous attempt `{}` is missing workflow linkage while parent unit `{}` retained durable linkage.",
                            attempt.attempt_id, unit.unit_id
                        ),
                    ));
                }
                (Some(_), Some(_)) => {
                    return Err(map_runtime_run_decode_error(
                        database_path,
                        format!(
                            "Autonomous attempt `{}` workflow linkage does not match parent unit `{}` linkage.",
                            attempt.attempt_id, unit.unit_id
                        ),
                    ));
                }
            }
        }

        let unit_artifacts = latest_attempt
            .as_ref()
            .and_then(|attempt| artifacts_by_attempt.remove(&attempt.attempt_id))
            .unwrap_or_default();

        history.push(AutonomousUnitHistoryRecord {
            unit,
            latest_attempt,
            artifacts: unit_artifacts,
        });
    }

    Ok(history)
}

fn decode_autonomous_run_row(
    raw_row: RawAutonomousRunRow,
    database_path: &Path,
) -> Result<AutonomousRunRecord, CommandError> {
    let project_id =
        require_runtime_run_non_empty_owned(raw_row.project_id, "project_id", database_path)?;
    let agent_session_id = require_runtime_run_non_empty_owned(
        raw_row.agent_session_id,
        "agent_session_id",
        database_path,
    )?;
    let run_id = require_runtime_run_non_empty_owned(raw_row.run_id, "run_id", database_path)?;
    let runtime_kind =
        require_runtime_run_non_empty_owned(raw_row.runtime_kind, "runtime_kind", database_path)?;
    let provider_id =
        require_runtime_run_non_empty_owned(raw_row.provider_id, "provider_id", database_path)?;
    crate::runtime::resolve_runtime_provider_identity(
        Some(provider_id.as_str()),
        Some(runtime_kind.as_str()),
    )
    .map_err(|diagnostic| {
        map_runtime_run_decode_error(
            database_path,
            format!(
                "Autonomous run provider identity is invalid because {}",
                diagnostic.message
            ),
        )
    })?;
    let supervisor_kind = require_runtime_run_non_empty_owned(
        raw_row.supervisor_kind,
        "supervisor_kind",
        database_path,
    )?;
    let status = parse_autonomous_run_status(&raw_row.status).map_err(|details| {
        map_runtime_run_decode_error(database_path, format!("Field `status` {details}"))
    })?;
    let active_unit_sequence = raw_row
        .active_unit_sequence
        .map(|value| {
            decode_runtime_run_checkpoint_sequence(value, "active_unit_sequence", database_path)
        })
        .transpose()?;
    let duplicate_start_detected = decode_runtime_run_bool(
        raw_row.duplicate_start_detected,
        "duplicate_start_detected",
        database_path,
    )?;
    let duplicate_start_run_id = decode_runtime_run_optional_non_empty_text(
        raw_row.duplicate_start_run_id,
        "duplicate_start_run_id",
        database_path,
    )?;
    let duplicate_start_reason = decode_runtime_run_optional_non_empty_text(
        raw_row.duplicate_start_reason,
        "duplicate_start_reason",
        database_path,
    )?;
    let started_at =
        require_runtime_run_non_empty_owned(raw_row.started_at, "started_at", database_path)?;
    let last_heartbeat_at = decode_runtime_run_optional_non_empty_text(
        raw_row.last_heartbeat_at,
        "last_heartbeat_at",
        database_path,
    )?;
    let last_checkpoint_at = decode_runtime_run_optional_non_empty_text(
        raw_row.last_checkpoint_at,
        "last_checkpoint_at",
        database_path,
    )?;
    let paused_at =
        decode_runtime_run_optional_non_empty_text(raw_row.paused_at, "paused_at", database_path)?;
    let cancelled_at = decode_runtime_run_optional_non_empty_text(
        raw_row.cancelled_at,
        "cancelled_at",
        database_path,
    )?;
    let completed_at = decode_runtime_run_optional_non_empty_text(
        raw_row.completed_at,
        "completed_at",
        database_path,
    )?;
    let crashed_at = decode_runtime_run_optional_non_empty_text(
        raw_row.crashed_at,
        "crashed_at",
        database_path,
    )?;
    let stopped_at = decode_runtime_run_optional_non_empty_text(
        raw_row.stopped_at,
        "stopped_at",
        database_path,
    )?;
    let pause_reason = decode_runtime_run_reason(
        raw_row.pause_reason_code,
        raw_row.pause_reason_message,
        "pause_reason",
        database_path,
    )?;
    let cancel_reason = decode_runtime_run_reason(
        raw_row.cancel_reason_code,
        raw_row.cancel_reason_message,
        "cancel_reason",
        database_path,
    )?;
    let crash_reason = decode_runtime_run_reason(
        raw_row.crash_reason_code,
        raw_row.crash_reason_message,
        "crash_reason",
        database_path,
    )?;
    let last_error = decode_runtime_run_reason(
        raw_row.last_error_code,
        raw_row.last_error_message,
        "last_error",
        database_path,
    )?;
    let updated_at =
        require_runtime_run_non_empty_owned(raw_row.updated_at, "updated_at", database_path)?;

    if duplicate_start_detected
        && (duplicate_start_run_id.is_none() || duplicate_start_reason.is_none())
    {
        return Err(map_runtime_run_decode_error(
            database_path,
            "Autonomous run duplicate-start fields must be fully populated when duplicate_start_detected is true.".into(),
        ));
    }

    if !duplicate_start_detected
        && (duplicate_start_run_id.is_some() || duplicate_start_reason.is_some())
    {
        return Err(map_runtime_run_decode_error(
            database_path,
            "Autonomous run duplicate-start fields must be null when duplicate_start_detected is false.".into(),
        ));
    }

    Ok(AutonomousRunRecord {
        project_id,
        agent_session_id,
        run_id,
        runtime_kind,
        provider_id,
        supervisor_kind,
        status,
        active_unit_sequence,
        duplicate_start_detected,
        duplicate_start_run_id,
        duplicate_start_reason,
        started_at,
        last_heartbeat_at,
        last_checkpoint_at,
        paused_at,
        cancelled_at,
        completed_at,
        crashed_at,
        stopped_at,
        pause_reason,
        cancel_reason,
        crash_reason,
        last_error,
        updated_at,
    })
}

fn decode_autonomous_workflow_linkage_row(
    workflow_node_id: Option<String>,
    transition_id: Option<String>,
    causal_transition_id: Option<String>,
    handoff_transition_id: Option<String>,
    handoff_package_hash: Option<String>,
    database_path: &Path,
) -> Result<Option<AutonomousWorkflowLinkageRecord>, CommandError> {
    let populated_fields = [
        workflow_node_id.is_some(),
        transition_id.is_some(),
        handoff_transition_id.is_some(),
        handoff_package_hash.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();

    if populated_fields == 0 && causal_transition_id.is_none() {
        return Ok(None);
    }

    if populated_fields != 4 {
        return Err(map_runtime_run_decode_error(
            database_path,
            "Autonomous workflow linkage rows must either omit all linkage fields or persist non-empty `workflow_node_id`, `transition_id`, `handoff_transition_id`, and `handoff_package_hash` values."
                .into(),
        ));
    }

    let handoff_package_hash = require_runtime_run_non_empty_owned(
        handoff_package_hash.ok_or_else(|| {
            map_runtime_run_decode_error(
                database_path,
                "Field `workflow_handoff_package_hash` must be a non-empty string when workflow linkage is present."
                    .into(),
            )
        })?,
        "workflow_handoff_package_hash",
        database_path,
    )?;
    validate_workflow_handoff_package_hash(
        &handoff_package_hash,
        "workflow_handoff_package_hash",
        database_path,
        "runtime_run_decode_failed",
    )?;

    Ok(Some(AutonomousWorkflowLinkageRecord {
        workflow_node_id: require_runtime_run_non_empty_owned(
            workflow_node_id.ok_or_else(|| {
                map_runtime_run_decode_error(
                    database_path,
                    "Field `workflow_node_id` must be a non-empty string when workflow linkage is present."
                        .into(),
                )
            })?,
            "workflow_node_id",
            database_path,
        )?,
        transition_id: require_runtime_run_non_empty_owned(
            transition_id.ok_or_else(|| {
                map_runtime_run_decode_error(
                    database_path,
                    "Field `workflow_transition_id` must be a non-empty string when workflow linkage is present."
                        .into(),
                )
            })?,
            "workflow_transition_id",
            database_path,
        )?,
        causal_transition_id: decode_runtime_run_optional_non_empty_text(
            causal_transition_id,
            "workflow_causal_transition_id",
            database_path,
        )?,
        handoff_transition_id: require_runtime_run_non_empty_owned(
            handoff_transition_id.ok_or_else(|| {
                map_runtime_run_decode_error(
                    database_path,
                    "Field `workflow_handoff_transition_id` must be a non-empty string when workflow linkage is present."
                        .into(),
                )
            })?,
            "workflow_handoff_transition_id",
            database_path,
        )?,
        handoff_package_hash,
    }))
}

fn decode_autonomous_unit_row(
    raw_row: RawAutonomousUnitRow,
    database_path: &Path,
) -> Result<AutonomousUnitRecord, CommandError> {
    Ok(AutonomousUnitRecord {
        project_id: require_runtime_run_non_empty_owned(
            raw_row.project_id,
            "project_id",
            database_path,
        )?,
        run_id: require_runtime_run_non_empty_owned(raw_row.run_id, "run_id", database_path)?,
        unit_id: require_runtime_run_non_empty_owned(raw_row.unit_id, "unit_id", database_path)?,
        sequence: decode_runtime_run_checkpoint_sequence(
            raw_row.sequence,
            "sequence",
            database_path,
        )?,
        kind: parse_autonomous_unit_kind(&raw_row.kind).map_err(|details| {
            map_runtime_run_decode_error(database_path, format!("Field `kind` {details}"))
        })?,
        status: parse_autonomous_unit_status(&raw_row.status).map_err(|details| {
            map_runtime_run_decode_error(database_path, format!("Field `status` {details}"))
        })?,
        summary: require_runtime_run_non_empty_owned(raw_row.summary, "summary", database_path)?,
        boundary_id: decode_runtime_run_optional_non_empty_text(
            raw_row.boundary_id,
            "boundary_id",
            database_path,
        )?,
        workflow_linkage: decode_autonomous_workflow_linkage_row(
            raw_row.workflow_node_id,
            raw_row.workflow_transition_id,
            raw_row.workflow_causal_transition_id,
            raw_row.workflow_handoff_transition_id,
            raw_row.workflow_handoff_package_hash,
            database_path,
        )?,
        started_at: require_runtime_run_non_empty_owned(
            raw_row.started_at,
            "started_at",
            database_path,
        )?,
        finished_at: decode_runtime_run_optional_non_empty_text(
            raw_row.finished_at,
            "finished_at",
            database_path,
        )?,
        updated_at: require_runtime_run_non_empty_owned(
            raw_row.updated_at,
            "updated_at",
            database_path,
        )?,
        last_error: decode_runtime_run_reason(
            raw_row.last_error_code,
            raw_row.last_error_message,
            "last_error",
            database_path,
        )?,
    })
}

fn decode_autonomous_unit_attempt_row(
    raw_row: RawAutonomousUnitAttemptRow,
    database_path: &Path,
) -> Result<AutonomousUnitAttemptRecord, CommandError> {
    Ok(AutonomousUnitAttemptRecord {
        project_id: require_runtime_run_non_empty_owned(
            raw_row.project_id,
            "project_id",
            database_path,
        )?,
        run_id: require_runtime_run_non_empty_owned(raw_row.run_id, "run_id", database_path)?,
        unit_id: require_runtime_run_non_empty_owned(raw_row.unit_id, "unit_id", database_path)?,
        attempt_id: require_runtime_run_non_empty_owned(
            raw_row.attempt_id,
            "attempt_id",
            database_path,
        )?,
        attempt_number: decode_runtime_run_checkpoint_sequence(
            raw_row.attempt_number,
            "attempt_number",
            database_path,
        )?,
        child_session_id: require_runtime_run_non_empty_owned(
            raw_row.child_session_id,
            "child_session_id",
            database_path,
        )?,
        status: parse_autonomous_unit_status(&raw_row.status).map_err(|details| {
            map_runtime_run_decode_error(database_path, format!("Field `status` {details}"))
        })?,
        boundary_id: decode_runtime_run_optional_non_empty_text(
            raw_row.boundary_id,
            "boundary_id",
            database_path,
        )?,
        workflow_linkage: decode_autonomous_workflow_linkage_row(
            raw_row.workflow_node_id,
            raw_row.workflow_transition_id,
            raw_row.workflow_causal_transition_id,
            raw_row.workflow_handoff_transition_id,
            raw_row.workflow_handoff_package_hash,
            database_path,
        )?,
        started_at: require_runtime_run_non_empty_owned(
            raw_row.started_at,
            "started_at",
            database_path,
        )?,
        finished_at: decode_runtime_run_optional_non_empty_text(
            raw_row.finished_at,
            "finished_at",
            database_path,
        )?,
        updated_at: require_runtime_run_non_empty_owned(
            raw_row.updated_at,
            "updated_at",
            database_path,
        )?,
        last_error: decode_runtime_run_reason(
            raw_row.last_error_code,
            raw_row.last_error_message,
            "last_error",
            database_path,
        )?,
    })
}

fn decode_autonomous_unit_artifact_row(
    raw_row: RawAutonomousUnitArtifactRow,
    database_path: &Path,
) -> Result<AutonomousUnitArtifactRecord, CommandError> {
    let project_id =
        require_runtime_run_non_empty_owned(raw_row.project_id, "project_id", database_path)?;
    let run_id = require_runtime_run_non_empty_owned(raw_row.run_id, "run_id", database_path)?;
    let unit_id = require_runtime_run_non_empty_owned(raw_row.unit_id, "unit_id", database_path)?;
    let attempt_id =
        require_runtime_run_non_empty_owned(raw_row.attempt_id, "attempt_id", database_path)?;
    let artifact_id =
        require_runtime_run_non_empty_owned(raw_row.artifact_id, "artifact_id", database_path)?;
    let artifact_kind =
        require_runtime_run_non_empty_owned(raw_row.artifact_kind, "artifact_kind", database_path)?;
    let summary = require_runtime_run_non_empty_owned(raw_row.summary, "summary", database_path)?;
    let content_hash = decode_runtime_run_optional_non_empty_text(
        raw_row.content_hash,
        "content_hash",
        database_path,
    )?;
    if let Some(content_hash) = content_hash.as_deref() {
        validate_workflow_handoff_package_hash(
            content_hash,
            "content_hash",
            database_path,
            "runtime_run_decode_failed",
        )?;
    }

    let payload = raw_row
        .payload_json
        .map(|payload_json| {
            decode_autonomous_artifact_payload_json(
                &payload_json,
                &project_id,
                &run_id,
                &unit_id,
                &attempt_id,
                &artifact_id,
                &artifact_kind,
                database_path,
            )
        })
        .transpose()?;

    if payload.is_some() && content_hash.is_none() {
        return Err(map_runtime_run_decode_error(
            database_path,
            format!(
                "Autonomous artifact `{artifact_id}` stored structured payload JSON without a matching content_hash."
            ),
        ));
    }

    if let (Some(payload), Some(content_hash)) = (payload.as_ref(), content_hash.as_deref()) {
        let canonical_payload = canonicalize_autonomous_artifact_payload_json(payload)?;
        let expected_hash = compute_workflow_handoff_package_hash(&canonical_payload);
        if content_hash != expected_hash {
            return Err(map_runtime_run_decode_error(
                database_path,
                format!(
                    "Autonomous artifact `{artifact_id}` stored content_hash `{content_hash}` but canonical payload hash is `{expected_hash}`."
                ),
            ));
        }
    }

    if payload.is_none() && autonomous_artifact_kind_requires_payload(&artifact_kind) {
        return Err(map_runtime_run_decode_error(
            database_path,
            format!(
                "Autonomous artifact `{artifact_id}` of kind `{artifact_kind}` must persist a structured payload JSON value."
            ),
        ));
    }

    Ok(AutonomousUnitArtifactRecord {
        project_id,
        run_id,
        unit_id,
        attempt_id,
        artifact_id,
        artifact_kind,
        status: parse_autonomous_unit_artifact_status(&raw_row.status).map_err(|details| {
            map_runtime_run_decode_error(database_path, format!("Field `status` {details}"))
        })?,
        summary,
        content_hash,
        payload,
        created_at: require_runtime_run_non_empty_owned(
            raw_row.created_at,
            "created_at",
            database_path,
        )?,
        updated_at: require_runtime_run_non_empty_owned(
            raw_row.updated_at,
            "updated_at",
            database_path,
        )?,
    })
}

fn validate_autonomous_run_payload(payload: &AutonomousRunRecord) -> Result<(), CommandError> {
    validate_non_empty_text(
        &payload.project_id,
        "project_id",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.agent_session_id,
        "agent_session_id",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(&payload.run_id, "run_id", "autonomous_run_request_invalid")?;
    validate_non_empty_text(
        &payload.runtime_kind,
        "runtime_kind",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.provider_id,
        "provider_id",
        "autonomous_run_request_invalid",
    )?;
    crate::runtime::resolve_runtime_provider_identity(
        Some(payload.provider_id.as_str()),
        Some(payload.runtime_kind.as_str()),
    )
    .map_err(|diagnostic| {
        CommandError::user_fixable(
            "autonomous_run_request_invalid",
            format!(
                "Cadence rejected the durable autonomous-run identity because {}",
                diagnostic.message
            ),
        )
    })?;
    validate_non_empty_text(
        &payload.supervisor_kind,
        "supervisor_kind",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.started_at,
        "started_at",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.updated_at,
        "updated_at",
        "autonomous_run_request_invalid",
    )?;

    if let Some(active_unit_sequence) = payload.active_unit_sequence {
        if active_unit_sequence == 0 {
            return Err(CommandError::system_fault(
                "autonomous_run_request_invalid",
                "Cadence requires autonomous active-unit sequences to start at 1.",
            ));
        }
    }

    for (value, field) in [
        (payload.last_heartbeat_at.as_deref(), "last_heartbeat_at"),
        (payload.last_checkpoint_at.as_deref(), "last_checkpoint_at"),
        (payload.paused_at.as_deref(), "paused_at"),
        (payload.cancelled_at.as_deref(), "cancelled_at"),
        (payload.completed_at.as_deref(), "completed_at"),
        (payload.crashed_at.as_deref(), "crashed_at"),
        (payload.stopped_at.as_deref(), "stopped_at"),
        (
            payload.duplicate_start_run_id.as_deref(),
            "duplicate_start_run_id",
        ),
        (
            payload.duplicate_start_reason.as_deref(),
            "duplicate_start_reason",
        ),
    ] {
        if let Some(value) = value {
            validate_non_empty_text(value, field, "autonomous_run_request_invalid")?;
        }
    }

    for (reason, label) in [
        (payload.pause_reason.as_ref(), "pause_reason"),
        (payload.cancel_reason.as_ref(), "cancel_reason"),
        (payload.crash_reason.as_ref(), "crash_reason"),
        (payload.last_error.as_ref(), "last_error"),
    ] {
        if let Some(reason) = reason {
            validate_non_empty_text(
                &reason.code,
                &format!("{label}_code"),
                "autonomous_run_request_invalid",
            )?;
            validate_non_empty_text(
                &reason.message,
                &format!("{label}_message"),
                "autonomous_run_request_invalid",
            )?;
            if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&reason.message)
            {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    format!(
                        "Autonomous run {label} must not include {secret_hint}. Remove secret-bearing content before retrying."
                    ),
                ));
            }
        }
    }

    Ok(())
}

fn normalize_autonomous_run_upsert_payload(
    payload: &AutonomousRunUpsertRecord,
) -> Result<AutonomousRunUpsertRecord, CommandError> {
    validate_autonomous_run_payload(&payload.run)?;

    let Some(unit) = payload.unit.as_ref() else {
        if payload.attempt.is_some() || !payload.artifacts.is_empty() {
            return Err(CommandError::system_fault(
                "autonomous_run_request_invalid",
                "Cadence requires a durable autonomous unit row before attempts or artifacts can be persisted.",
            ));
        }
        return Ok(payload.clone());
    };

    validate_non_empty_text(&unit.unit_id, "unit_id", "autonomous_run_request_invalid")?;
    validate_non_empty_text(&unit.summary, "summary", "autonomous_run_request_invalid")?;
    validate_non_empty_text(
        &unit.started_at,
        "started_at",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &unit.updated_at,
        "updated_at",
        "autonomous_run_request_invalid",
    )?;
    if unit.sequence == 0 {
        return Err(CommandError::system_fault(
            "autonomous_run_request_invalid",
            "Cadence requires autonomous unit sequences to start at 1.",
        ));
    }
    if unit.project_id != payload.run.project_id || unit.run_id != payload.run.run_id {
        return Err(CommandError::system_fault(
            "autonomous_run_request_invalid",
            "Cadence requires autonomous unit rows to share the parent run project_id and run_id.",
        ));
    }
    if let Some(boundary_id) = unit.boundary_id.as_deref() {
        validate_non_empty_text(boundary_id, "boundary_id", "autonomous_run_request_invalid")?;
    }
    if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&unit.summary) {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            format!(
                "Autonomous unit summaries must not include {secret_hint}. Remove secret-bearing content before retrying."
            ),
        ));
    }

    let normalized_unit_workflow_linkage = normalize_autonomous_workflow_linkage_payload(
        unit.workflow_linkage.as_ref(),
        "unit_workflow_linkage",
    )?;

    let normalized_attempt = if let Some(attempt) = payload.attempt.as_ref() {
        validate_non_empty_text(
            &attempt.attempt_id,
            "attempt_id",
            "autonomous_run_request_invalid",
        )?;
        validate_non_empty_text(
            &attempt.child_session_id,
            "child_session_id",
            "autonomous_run_request_invalid",
        )?;
        validate_non_empty_text(
            &attempt.started_at,
            "attempt_started_at",
            "autonomous_run_request_invalid",
        )?;
        validate_non_empty_text(
            &attempt.updated_at,
            "attempt_updated_at",
            "autonomous_run_request_invalid",
        )?;
        if attempt.attempt_number == 0 {
            return Err(CommandError::system_fault(
                "autonomous_run_request_invalid",
                "Cadence requires autonomous attempt numbers to start at 1.",
            ));
        }
        if attempt.project_id != payload.run.project_id
            || attempt.run_id != payload.run.run_id
            || attempt.unit_id != unit.unit_id
        {
            return Err(CommandError::system_fault(
                "autonomous_run_request_invalid",
                "Cadence requires autonomous attempts to share the parent run and unit linkage.",
            ));
        }
        if let Some(boundary_id) = attempt.boundary_id.as_deref() {
            validate_non_empty_text(
                boundary_id,
                "attempt_boundary_id",
                "autonomous_run_request_invalid",
            )?;
        }
        if let Some(reason) = attempt.last_error.as_ref() {
            validate_non_empty_text(
                &reason.code,
                "attempt_last_error_code",
                "autonomous_run_request_invalid",
            )?;
            validate_non_empty_text(
                &reason.message,
                "attempt_last_error_message",
                "autonomous_run_request_invalid",
            )?;
        }

        let normalized_attempt_workflow_linkage = normalize_autonomous_workflow_linkage_payload(
            attempt.workflow_linkage.as_ref(),
            "attempt_workflow_linkage",
        )?;
        validate_matching_autonomous_workflow_linkage_payloads(
            normalized_unit_workflow_linkage.as_ref(),
            normalized_attempt_workflow_linkage.as_ref(),
        )?;

        Some(AutonomousUnitAttemptRecord {
            workflow_linkage: normalized_attempt_workflow_linkage,
            ..attempt.clone()
        })
    } else {
        None
    };

    let normalized_artifacts = payload
        .artifacts
        .iter()
        .map(|artifact| {
            normalize_autonomous_unit_artifact_record(
                artifact,
                &payload.run,
                unit,
                payload.attempt.as_ref(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(AutonomousRunUpsertRecord {
        run: payload.run.clone(),
        unit: Some(AutonomousUnitRecord {
            workflow_linkage: normalized_unit_workflow_linkage,
            ..unit.clone()
        }),
        attempt: normalized_attempt,
        artifacts: normalized_artifacts,
    })
}

fn normalize_autonomous_workflow_linkage_payload(
    linkage: Option<&AutonomousWorkflowLinkageRecord>,
    field_prefix: &str,
) -> Result<Option<AutonomousWorkflowLinkageRecord>, CommandError> {
    let Some(linkage) = linkage else {
        return Ok(None);
    };

    validate_non_empty_text(
        &linkage.workflow_node_id,
        &format!("{field_prefix}_workflow_node_id"),
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &linkage.transition_id,
        &format!("{field_prefix}_transition_id"),
        "autonomous_run_request_invalid",
    )?;
    if let Some(causal_transition_id) = linkage.causal_transition_id.as_deref() {
        validate_non_empty_text(
            causal_transition_id,
            &format!("{field_prefix}_causal_transition_id"),
            "autonomous_run_request_invalid",
        )?;
    }
    validate_non_empty_text(
        &linkage.handoff_transition_id,
        &format!("{field_prefix}_handoff_transition_id"),
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &linkage.handoff_package_hash,
        &format!("{field_prefix}_handoff_package_hash"),
        "autonomous_run_request_invalid",
    )?;

    if linkage.handoff_package_hash.len() != 64
        || linkage
            .handoff_package_hash
            .chars()
            .any(|ch| !ch.is_ascii_hexdigit() || ch.is_ascii_uppercase())
    {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            format!(
                "Cadence requires {field_prefix} handoff package hashes to be lowercase 64-character hex digests."
            ),
        ));
    }

    Ok(Some(linkage.clone()))
}

fn validate_matching_autonomous_workflow_linkage_payloads(
    unit_linkage: Option<&AutonomousWorkflowLinkageRecord>,
    attempt_linkage: Option<&AutonomousWorkflowLinkageRecord>,
) -> Result<(), CommandError> {
    match (unit_linkage, attempt_linkage) {
        (None, None) | (Some(_), None) => Ok(()),
        (None, Some(_)) => Err(CommandError::system_fault(
            "autonomous_run_request_invalid",
            "Cadence requires autonomous attempts to omit workflow linkage until the parent unit carries durable workflow linkage.",
        )),
        (Some(unit_linkage), Some(attempt_linkage)) if unit_linkage == attempt_linkage => Ok(()),
        (Some(_), Some(_)) => Err(CommandError::system_fault(
            "autonomous_run_request_invalid",
            "Cadence requires autonomous attempt workflow linkage to match the owning unit linkage exactly.",
        )),
    }
}

fn validate_autonomous_workflow_linkage_record(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    linkage: &AutonomousWorkflowLinkageRecord,
    owner_kind: &str,
    owner_id: &str,
    error_code: &'static str,
) -> Result<(), CommandError> {
    let transition_event = read_transition_event_by_transition_id(
        connection,
        database_path,
        project_id,
        &linkage.transition_id,
    )?
    .ok_or_else(|| {
        autonomous_workflow_linkage_error(
            error_code,
            database_path,
            format!(
                "Autonomous {owner_kind} `{owner_id}` references workflow transition `{}` that is missing for project `{project_id}`.",
                linkage.transition_id
            ),
        )
    })?;

    if transition_event.to_node_id != linkage.workflow_node_id {
        return Err(autonomous_workflow_linkage_error(
            error_code,
            database_path,
            format!(
                "Autonomous {owner_kind} `{owner_id}` workflow node `{}` does not match transition `{}` destination node `{}`.",
                linkage.workflow_node_id, linkage.transition_id, transition_event.to_node_id
            ),
        ));
    }

    if transition_event.causal_transition_id != linkage.causal_transition_id {
        return Err(autonomous_workflow_linkage_error(
            error_code,
            database_path,
            format!(
                "Autonomous {owner_kind} `{owner_id}` causal transition linkage {:?} does not match durable transition `{}` causal linkage {:?}.",
                linkage.causal_transition_id,
                linkage.transition_id,
                transition_event.causal_transition_id
            ),
        ));
    }

    let handoff_package = read_workflow_handoff_package_by_transition_id(
        connection,
        database_path,
        project_id,
        &linkage.handoff_transition_id,
    )?
    .ok_or_else(|| {
        autonomous_workflow_linkage_error(
            error_code,
            database_path,
            format!(
                "Autonomous {owner_kind} `{owner_id}` references workflow handoff `{}` that is missing for project `{project_id}`.",
                linkage.handoff_transition_id
            ),
        )
    })?;

    validate_workflow_handoff_package_transition_linkage(&handoff_package, &transition_event)
        .map_err(|error| {
            autonomous_workflow_linkage_error(error_code, database_path, error.message)
        })?;

    if handoff_package.package_hash != linkage.handoff_package_hash {
        return Err(autonomous_workflow_linkage_error(
            error_code,
            database_path,
            format!(
                "Autonomous {owner_kind} `{owner_id}` handoff package hash `{}` does not match durable package hash `{}` for transition `{}`.",
                linkage.handoff_package_hash,
                handoff_package.package_hash,
                linkage.handoff_transition_id
            ),
        ));
    }

    Ok(())
}

fn autonomous_workflow_linkage_error(
    error_code: &'static str,
    database_path: &Path,
    message: String,
) -> CommandError {
    if error_code == "runtime_run_decode_failed" {
        return map_runtime_run_decode_error(database_path, message);
    }

    CommandError::system_fault(error_code, message)
}

fn normalize_autonomous_unit_artifact_record(
    artifact: &AutonomousUnitArtifactRecord,
    run: &AutonomousRunRecord,
    unit: &AutonomousUnitRecord,
    attempt: Option<&AutonomousUnitAttemptRecord>,
) -> Result<AutonomousUnitArtifactRecord, CommandError> {
    validate_non_empty_text(
        &artifact.artifact_id,
        "artifact_id",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &artifact.artifact_kind,
        "artifact_kind",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &artifact.summary,
        "artifact_summary",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &artifact.created_at,
        "artifact_created_at",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &artifact.updated_at,
        "artifact_updated_at",
        "autonomous_run_request_invalid",
    )?;

    if artifact.project_id != run.project_id
        || artifact.run_id != run.run_id
        || artifact.unit_id != unit.unit_id
    {
        return Err(CommandError::system_fault(
            "autonomous_run_request_invalid",
            "Cadence requires autonomous artifacts to share the parent run and unit linkage.",
        ));
    }
    if attempt.is_some_and(|attempt| artifact.attempt_id != attempt.attempt_id) {
        return Err(CommandError::system_fault(
            "autonomous_run_request_invalid",
            "Cadence requires autonomous artifacts to link to the persisted attempt id.",
        ));
    }
    if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&artifact.summary) {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            format!(
                "Autonomous artifact summaries must not include {secret_hint}. Remove secret-bearing content before retrying."
            ),
        ));
    }

    let canonical_payload = artifact
        .payload
        .as_ref()
        .map(|payload| {
            validate_autonomous_artifact_payload(
                payload,
                &artifact.project_id,
                &artifact.run_id,
                &artifact.unit_id,
                &artifact.attempt_id,
                &artifact.artifact_id,
                &artifact.artifact_kind,
            )?;
            canonicalize_autonomous_artifact_payload_json(payload)
        })
        .transpose()?;

    if artifact.payload.is_none()
        && autonomous_artifact_kind_requires_payload(&artifact.artifact_kind)
    {
        let message = format!(
            "Cadence requires `{}` autonomous artifacts to persist a structured payload.",
            artifact.artifact_kind
        );
        return if artifact.artifact_kind == AUTONOMOUS_ARTIFACT_KIND_POLICY_DENIED {
            Err(CommandError::policy_denied(message))
        } else {
            Err(CommandError::user_fixable(
                "autonomous_run_request_invalid",
                message,
            ))
        };
    }

    let normalized_hash = match canonical_payload.as_deref() {
        Some(payload_json) => {
            let expected_hash = compute_workflow_handoff_package_hash(payload_json);
            if let Some(content_hash) = artifact.content_hash.as_deref() {
                validate_non_empty_text(
                    content_hash,
                    "artifact_content_hash",
                    "autonomous_run_request_invalid",
                )?;
                if content_hash.len() != 64
                    || content_hash
                        .chars()
                        .any(|ch| !ch.is_ascii_hexdigit() || ch.is_ascii_uppercase())
                {
                    return Err(CommandError::user_fixable(
                        "autonomous_run_request_invalid",
                        "Cadence requires autonomous artifact content hashes to be lowercase 64-character hex digests.",
                    ));
                }
                if content_hash != expected_hash {
                    return Err(CommandError::user_fixable(
                        "autonomous_run_request_invalid",
                        "Cadence requires autonomous artifact content_hash values to match the canonical structured payload.",
                    ));
                }
            }
            Some(expected_hash)
        }
        None => {
            if let Some(content_hash) = artifact.content_hash.as_deref() {
                validate_non_empty_text(
                    content_hash,
                    "artifact_content_hash",
                    "autonomous_run_request_invalid",
                )?;
                if content_hash.len() != 64
                    || content_hash
                        .chars()
                        .any(|ch| !ch.is_ascii_hexdigit() || ch.is_ascii_uppercase())
                {
                    return Err(CommandError::user_fixable(
                        "autonomous_run_request_invalid",
                        "Cadence requires autonomous artifact content hashes to be lowercase 64-character hex digests.",
                    ));
                }
            }
            artifact.content_hash.clone()
        }
    };

    Ok(AutonomousUnitArtifactRecord {
        content_hash: normalized_hash,
        ..artifact.clone()
    })
}

#[allow(clippy::too_many_arguments)]
fn decode_autonomous_artifact_payload_json(
    payload_json: &str,
    project_id: &str,
    run_id: &str,
    unit_id: &str,
    attempt_id: &str,
    artifact_id: &str,
    artifact_kind: &str,
    database_path: &Path,
) -> Result<AutonomousArtifactPayloadRecord, CommandError> {
    let parsed =
        serde_json::from_str::<AutonomousArtifactPayloadRecord>(payload_json).map_err(|error| {
            map_runtime_run_decode_error(
                database_path,
                format!(
                    "Autonomous artifact `{artifact_id}` stored malformed payload_json: {error}"
                ),
            )
        })?;

    validate_autonomous_artifact_payload(
        &parsed,
        project_id,
        run_id,
        unit_id,
        attempt_id,
        artifact_id,
        artifact_kind,
    )
    .map_err(|error| map_runtime_run_decode_error(database_path, error.message))?;

    Ok(parsed)
}

fn canonicalize_autonomous_artifact_payload_json(
    payload: &AutonomousArtifactPayloadRecord,
) -> Result<String, CommandError> {
    let value = serde_json::to_value(payload).map_err(|error| {
        CommandError::system_fault(
            "autonomous_run_request_invalid",
            format!(
                "Cadence could not serialize the autonomous artifact payload to canonical JSON: {error}"
            ),
        )
    })?;

    let canonical = canonicalize_json_value(value);
    serde_json::to_string(&canonical).map_err(|error| {
        CommandError::system_fault(
            "autonomous_run_request_invalid",
            format!("Cadence could not canonicalize the autonomous artifact payload JSON: {error}"),
        )
    })
}

fn validate_autonomous_artifact_payload(
    payload: &AutonomousArtifactPayloadRecord,
    project_id: &str,
    run_id: &str,
    unit_id: &str,
    attempt_id: &str,
    artifact_id: &str,
    artifact_kind: &str,
) -> Result<(), CommandError> {
    let expected_kind = autonomous_artifact_payload_kind(payload);
    if artifact_kind != expected_kind {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            format!(
                "Cadence requires autonomous artifact kind `{artifact_kind}` to match payload kind `{expected_kind}`."
            ),
        ));
    }

    match payload {
        AutonomousArtifactPayloadRecord::ToolResult(tool) => {
            validate_autonomous_artifact_payload_linkage(
                &tool.project_id,
                &tool.run_id,
                &tool.unit_id,
                &tool.attempt_id,
                &tool.artifact_id,
                project_id,
                run_id,
                unit_id,
                attempt_id,
                artifact_id,
            )?;
            validate_non_empty_text(
                &tool.tool_call_id,
                "tool_call_id",
                "autonomous_run_request_invalid",
            )?;
            validate_non_empty_text(
                &tool.tool_name,
                "tool_name",
                "autonomous_run_request_invalid",
            )?;
            validate_autonomous_artifact_text(&tool.tool_name, "tool_name")?;
            validate_autonomous_artifact_action_boundary_linkage(
                tool.action_id.as_deref(),
                tool.boundary_id.as_deref(),
            )?;
            if let Some(command_result) = tool.command_result.as_ref() {
                validate_autonomous_artifact_command_result(command_result)?;
            }
            validate_autonomous_tool_result_summary(
                &tool.tool_state,
                tool.command_result.as_ref(),
                tool.tool_summary.as_ref(),
            )?;
        }
        AutonomousArtifactPayloadRecord::VerificationEvidence(evidence) => {
            validate_autonomous_artifact_payload_linkage(
                &evidence.project_id,
                &evidence.run_id,
                &evidence.unit_id,
                &evidence.attempt_id,
                &evidence.artifact_id,
                project_id,
                run_id,
                unit_id,
                attempt_id,
                artifact_id,
            )?;
            validate_non_empty_text(
                &evidence.evidence_kind,
                "evidence_kind",
                "autonomous_run_request_invalid",
            )?;
            validate_non_empty_text(
                &evidence.label,
                "evidence_label",
                "autonomous_run_request_invalid",
            )?;
            validate_autonomous_artifact_text(&evidence.evidence_kind, "evidence_kind")?;
            validate_autonomous_artifact_text(&evidence.label, "evidence_label")?;
            validate_autonomous_artifact_action_boundary_linkage(
                evidence.action_id.as_deref(),
                evidence.boundary_id.as_deref(),
            )?;
            if let Some(command_result) = evidence.command_result.as_ref() {
                validate_autonomous_artifact_command_result(command_result)?;
            }
        }
        AutonomousArtifactPayloadRecord::PolicyDenied(policy) => {
            validate_autonomous_artifact_payload_linkage(
                &policy.project_id,
                &policy.run_id,
                &policy.unit_id,
                &policy.attempt_id,
                &policy.artifact_id,
                project_id,
                run_id,
                unit_id,
                attempt_id,
                artifact_id,
            )?;
            if policy.diagnostic_code.trim().is_empty() {
                return Err(CommandError::policy_denied(
                    "Cadence requires policy_denied artifacts to include a stable diagnostic_code.",
                ));
            }
            validate_non_empty_text(
                &policy.diagnostic_code,
                "policy_denied_code",
                "autonomous_run_request_invalid",
            )?;
            validate_non_empty_text(
                &policy.message,
                "policy_denied_message",
                "autonomous_run_request_invalid",
            )?;
            validate_autonomous_artifact_text(&policy.message, "policy_denied_message")?;
            if let Some(tool_name) = policy.tool_name.as_deref() {
                validate_non_empty_text(
                    tool_name,
                    "policy_denied_tool_name",
                    "autonomous_run_request_invalid",
                )?;
                validate_autonomous_artifact_text(tool_name, "policy_denied_tool_name")?;
            }
            validate_autonomous_artifact_action_boundary_linkage(
                policy.action_id.as_deref(),
                policy.boundary_id.as_deref(),
            )?;
        }
        AutonomousArtifactPayloadRecord::SkillLifecycle(skill) => {
            validate_autonomous_artifact_payload_linkage(
                &skill.project_id,
                &skill.run_id,
                &skill.unit_id,
                &skill.attempt_id,
                &skill.artifact_id,
                project_id,
                run_id,
                unit_id,
                attempt_id,
                artifact_id,
            )?;
            validate_autonomous_skill_lifecycle_payload(skill)?;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_autonomous_artifact_payload_linkage(
    payload_project_id: &str,
    payload_run_id: &str,
    payload_unit_id: &str,
    payload_attempt_id: &str,
    payload_artifact_id: &str,
    project_id: &str,
    run_id: &str,
    unit_id: &str,
    attempt_id: &str,
    artifact_id: &str,
) -> Result<(), CommandError> {
    for (value, field) in [
        (payload_project_id, "payload_project_id"),
        (payload_run_id, "payload_run_id"),
        (payload_unit_id, "payload_unit_id"),
        (payload_attempt_id, "payload_attempt_id"),
        (payload_artifact_id, "payload_artifact_id"),
    ] {
        validate_non_empty_text(value, field, "autonomous_run_request_invalid")?;
    }

    if payload_project_id != project_id
        || payload_run_id != run_id
        || payload_unit_id != unit_id
        || payload_attempt_id != attempt_id
        || payload_artifact_id != artifact_id
    {
        return Err(CommandError::system_fault(
            "autonomous_run_request_invalid",
            "Cadence requires autonomous artifact payload linkage to match the owning project/run/unit/attempt/artifact row.",
        ));
    }

    Ok(())
}

fn validate_autonomous_artifact_action_boundary_linkage(
    action_id: Option<&str>,
    boundary_id: Option<&str>,
) -> Result<(), CommandError> {
    match (action_id, boundary_id) {
        (Some(action_id), Some(boundary_id)) => {
            validate_non_empty_text(
                action_id,
                "artifact_action_id",
                "autonomous_run_request_invalid",
            )?;
            validate_non_empty_text(
                boundary_id,
                "artifact_boundary_id",
                "autonomous_run_request_invalid",
            )?;
            validate_runtime_action_boundary_identity(action_id.trim(), boundary_id.trim())
        }
        (None, None) => Ok(()),
        _ => Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            "Cadence requires autonomous artifact action_id and boundary_id to be provided together.",
        )),
    }
}

fn validate_runtime_action_boundary_identity(
    action_id: &str,
    boundary_id: &str,
) -> Result<(), CommandError> {
    if action_id.chars().any(char::is_whitespace) {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            "Cadence requires boundary-linked autonomous artifacts to persist canonical action_id values without whitespace.",
        ));
    }

    if boundary_id.contains(':') || boundary_id.chars().any(char::is_whitespace) {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            "Cadence requires boundary-linked autonomous artifacts to persist canonical boundary_id values.",
        ));
    }

    let run_marker = ":run:";
    let Some(run_start) = action_id.find(run_marker) else {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            "Cadence requires boundary-linked autonomous artifacts to use runtime-scoped canonical action_id values.",
        ));
    };
    if run_start == 0 {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            "Cadence requires boundary-linked autonomous artifacts to include a stable action scope prefix.",
        ));
    }

    let boundary_marker = format!(":boundary:{boundary_id}:");
    let Some(boundary_start) = action_id.find(boundary_marker.as_str()) else {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            "Cadence requires boundary-linked autonomous artifacts to keep action_id and boundary_id in canonical agreement.",
        ));
    };

    let run_id = &action_id[(run_start + run_marker.len())..boundary_start];
    if run_id.is_empty() || run_id.contains(':') || run_id.chars().any(char::is_whitespace) {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            "Cadence requires boundary-linked autonomous artifacts to persist a canonical run-scoped action_id.",
        ));
    }

    let action_type = &action_id[(boundary_start + boundary_marker.len())..];
    if action_type.is_empty() || action_type.contains(':') || action_type.chars().any(char::is_whitespace)
    {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            "Cadence requires boundary-linked autonomous artifacts to persist an action_id with a canonical action type suffix.",
        ));
    }

    Ok(())
}

fn validate_autonomous_artifact_command_result(
    command_result: &AutonomousArtifactCommandResultRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &command_result.summary,
        "artifact_command_summary",
        "autonomous_run_request_invalid",
    )?;
    validate_autonomous_artifact_text(&command_result.summary, "artifact_command_summary")
}

fn validate_autonomous_tool_result_summary(
    tool_state: &AutonomousToolCallStateRecord,
    command_result: Option<&AutonomousArtifactCommandResultRecord>,
    tool_summary: Option<&ToolResultSummary>,
) -> Result<(), CommandError> {
    if let Some(command_result) = command_result {
        if matches!(
            tool_state,
            AutonomousToolCallStateRecord::Pending | AutonomousToolCallStateRecord::Running
        ) {
            return Err(CommandError::user_fixable(
                "autonomous_run_request_invalid",
                "Cadence only persists command_result metadata after a tool reaches a terminal state.",
            ));
        }
        if matches!(tool_state, AutonomousToolCallStateRecord::Failed)
            && command_result.exit_code == Some(0)
            && !command_result.timed_out
        {
            return Err(CommandError::user_fixable(
                "autonomous_run_request_invalid",
                "Cadence rejected a failed tool_result payload whose command_result reported a successful exit code.",
            ));
        }
    }

    let Some(tool_summary) = tool_summary else {
        return Ok(());
    };

    match tool_summary {
        ToolResultSummary::Command(summary) => {
            if matches!(
                tool_state,
                AutonomousToolCallStateRecord::Pending | AutonomousToolCallStateRecord::Running
            ) {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence only persists command tool_summary metadata after a tool reaches a terminal state.",
                ));
            }
            let Some(command_result) = command_result else {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence requires command tool_summary metadata to include the paired command_result payload.",
                ));
            };
            if summary.exit_code != command_result.exit_code
                || summary.timed_out != command_result.timed_out
            {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence requires command tool_summary exit metadata to match the paired command_result payload.",
                ));
            }
            if matches!(tool_state, AutonomousToolCallStateRecord::Failed)
                && summary.exit_code == Some(0)
                && !summary.timed_out
            {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence rejected a failed tool_result payload whose command tool_summary reported a successful exit code.",
                ));
            }
        }
        ToolResultSummary::File(summary) => {
            if matches!(tool_state, AutonomousToolCallStateRecord::Failed) {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence does not persist file tool_summary metadata for failed tool results.",
                ));
            }
            if command_result.is_some() {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence requires file tool_summary metadata to omit command_result payloads.",
                ));
            }
            if summary.path.is_none() && summary.scope.is_none() {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence requires file tool_summary metadata to include a bounded path or scope.",
                ));
            }
            if let Some(path) = summary.path.as_deref() {
                validate_non_empty_text(
                    path,
                    "tool_summary_file_path",
                    "autonomous_run_request_invalid",
                )?;
                validate_autonomous_artifact_text(path, "tool_summary_file_path")?;
            }
            if let Some(scope) = summary.scope.as_deref() {
                validate_non_empty_text(
                    scope,
                    "tool_summary_file_scope",
                    "autonomous_run_request_invalid",
                )?;
                validate_autonomous_artifact_text(scope, "tool_summary_file_scope")?;
            }
        }
        ToolResultSummary::Git(summary) => {
            if matches!(tool_state, AutonomousToolCallStateRecord::Failed) {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence does not persist git tool_summary metadata for failed tool results.",
                ));
            }
            if command_result.is_some() {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence requires git tool_summary metadata to omit command_result payloads.",
                ));
            }
            if let Some(base_revision) = summary.base_revision.as_deref() {
                validate_non_empty_text(
                    base_revision,
                    "tool_summary_git_base_revision",
                    "autonomous_run_request_invalid",
                )?;
                validate_autonomous_artifact_text(base_revision, "tool_summary_git_base_revision")?;
            }
            if let Some(scope) = summary.scope.as_ref() {
                match scope {
                    GitToolResultScope::Staged
                    | GitToolResultScope::Unstaged
                    | GitToolResultScope::Worktree => {}
                }
            }
        }
        ToolResultSummary::Web(summary) => {
            if matches!(tool_state, AutonomousToolCallStateRecord::Failed) {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence does not persist web tool_summary metadata for failed tool results.",
                ));
            }
            if command_result.is_some() {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence requires web tool_summary metadata to omit command_result payloads.",
                ));
            }
            validate_non_empty_text(
                &summary.target,
                "tool_summary_web_target",
                "autonomous_run_request_invalid",
            )?;
            validate_autonomous_artifact_text(&summary.target, "tool_summary_web_target")?;
            if let Some(final_url) = summary.final_url.as_deref() {
                validate_non_empty_text(
                    final_url,
                    "tool_summary_web_final_url",
                    "autonomous_run_request_invalid",
                )?;
                validate_autonomous_artifact_text(final_url, "tool_summary_web_final_url")?;
            }
            if let Some(content_type) = summary.content_type.as_deref() {
                validate_non_empty_text(
                    content_type,
                    "tool_summary_web_content_type",
                    "autonomous_run_request_invalid",
                )?;
                validate_autonomous_artifact_text(content_type, "tool_summary_web_content_type")?;
            }
        }
        ToolResultSummary::BrowserComputerUse(summary) => {
            if command_result.is_some() {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence requires browser/computer-use tool_summary metadata to omit command_result payloads.",
                ));
            }

            validate_non_empty_text(
                &summary.action,
                "tool_summary_browser_computer_use_action",
                "autonomous_run_request_invalid",
            )?;
            validate_bounded_autonomous_artifact_text(
                &summary.action,
                "tool_summary_browser_computer_use_action",
                MAX_BROWSER_COMPUTER_USE_SUMMARY_TEXT_CHARS,
            )?;

            if let Some(target) = summary.target.as_deref() {
                validate_non_empty_text(
                    target,
                    "tool_summary_browser_computer_use_target",
                    "autonomous_run_request_invalid",
                )?;
                validate_bounded_autonomous_artifact_text(
                    target,
                    "tool_summary_browser_computer_use_target",
                    MAX_BROWSER_COMPUTER_USE_SUMMARY_TEXT_CHARS,
                )?;
            }

            if let Some(outcome) = summary.outcome.as_deref() {
                validate_non_empty_text(
                    outcome,
                    "tool_summary_browser_computer_use_outcome",
                    "autonomous_run_request_invalid",
                )?;
                validate_bounded_autonomous_artifact_text(
                    outcome,
                    "tool_summary_browser_computer_use_outcome",
                    MAX_BROWSER_COMPUTER_USE_SUMMARY_TEXT_CHARS,
                )?;
            }

            match summary.surface {
                BrowserComputerUseSurface::Browser | BrowserComputerUseSurface::ComputerUse => {}
            }

            match summary.status {
                BrowserComputerUseActionStatus::Pending
                | BrowserComputerUseActionStatus::Running
                | BrowserComputerUseActionStatus::Succeeded
                | BrowserComputerUseActionStatus::Failed
                | BrowserComputerUseActionStatus::Blocked => {}
            }

            validate_browser_computer_use_status_for_tool_state(tool_state, &summary.status)?;
        }
        ToolResultSummary::McpCapability(summary) => {
            if command_result.is_some() {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence requires MCP capability tool_summary metadata to omit command_result payloads.",
                ));
            }
            validate_non_empty_text(
                &summary.server_id,
                "tool_summary_mcp_server_id",
                "autonomous_run_request_invalid",
            )?;
            validate_non_empty_text(
                &summary.capability_id,
                "tool_summary_mcp_capability_id",
                "autonomous_run_request_invalid",
            )?;
            validate_autonomous_artifact_text(&summary.server_id, "tool_summary_mcp_server_id")?;
            validate_autonomous_artifact_text(
                &summary.capability_id,
                "tool_summary_mcp_capability_id",
            )?;
            if let Some(capability_name) = summary.capability_name.as_deref() {
                validate_non_empty_text(
                    capability_name,
                    "tool_summary_mcp_capability_name",
                    "autonomous_run_request_invalid",
                )?;
                validate_autonomous_artifact_text(
                    capability_name,
                    "tool_summary_mcp_capability_name",
                )?;
            }
            match summary.capability_kind {
                McpCapabilityKind::Tool
                | McpCapabilityKind::Resource
                | McpCapabilityKind::Prompt
                | McpCapabilityKind::Command => {}
            }
        }
    }

    Ok(())
}

fn validate_autonomous_skill_lifecycle_payload(
    skill: &AutonomousSkillLifecyclePayloadRecord,
) -> Result<(), CommandError> {
    validate_autonomous_skill_lifecycle_skill_id(&skill.skill_id)?;
    validate_autonomous_skill_lifecycle_source(&skill.source)?;

    validate_non_empty_text(
        &skill.cache.key,
        "skill_lifecycle_cache_key",
        "autonomous_run_request_invalid",
    )?;
    validate_autonomous_artifact_text(&skill.cache.key, "skill_lifecycle_cache_key")?;

    match skill.stage {
        AutonomousSkillLifecycleStageRecord::Discovery => {
            if skill.cache.status.is_some() {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence discovery skill_lifecycle payloads must omit cache status because no install or invoke step has completed yet.",
                ));
            }
        }
        AutonomousSkillLifecycleStageRecord::Install
        | AutonomousSkillLifecycleStageRecord::Invoke => {
            if matches!(
                skill.result,
                AutonomousSkillLifecycleResultRecord::Succeeded
            ) && skill.cache.status.is_none()
            {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    "Cadence successful install/invoke skill_lifecycle payloads must include cache status.",
                ));
            }
        }
    }

    match (&skill.result, skill.diagnostic.as_ref()) {
        (AutonomousSkillLifecycleResultRecord::Succeeded, Some(_)) => {
            return Err(CommandError::user_fixable(
                "autonomous_run_request_invalid",
                "Cadence rejected a successful skill_lifecycle payload that also reported failure diagnostics.",
            ));
        }
        (AutonomousSkillLifecycleResultRecord::Failed, None) => {
            return Err(CommandError::user_fixable(
                "autonomous_run_request_invalid",
                "Cadence failed skill_lifecycle payloads require typed diagnostics.",
            ));
        }
        (AutonomousSkillLifecycleResultRecord::Failed, Some(diagnostic)) => {
            validate_non_empty_text(
                &diagnostic.code,
                "skill_lifecycle_diagnostic_code",
                "autonomous_run_request_invalid",
            )?;
            validate_non_empty_text(
                &diagnostic.message,
                "skill_lifecycle_diagnostic_message",
                "autonomous_run_request_invalid",
            )?;
            validate_autonomous_artifact_text(
                &diagnostic.message,
                "skill_lifecycle_diagnostic_message",
            )?;
        }
        (AutonomousSkillLifecycleResultRecord::Succeeded, None) => {}
    }

    Ok(())
}

fn validate_autonomous_skill_lifecycle_skill_id(skill_id: &str) -> Result<(), CommandError> {
    validate_non_empty_text(
        skill_id,
        "skill_lifecycle_skill_id",
        "autonomous_run_request_invalid",
    )?;
    validate_autonomous_artifact_text(skill_id, "skill_lifecycle_skill_id")?;
    if !skill_id.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    }) {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            "Cadence requires skill_lifecycle skill ids to stay lowercase kebab-case values.",
        ));
    }

    Ok(())
}

fn validate_autonomous_skill_lifecycle_source(
    source: &AutonomousSkillLifecycleSourceRecord,
) -> Result<(), CommandError> {
    for (value, field) in [
        (&source.repo, "skill_lifecycle_source_repo"),
        (&source.path, "skill_lifecycle_source_path"),
        (&source.reference, "skill_lifecycle_source_reference"),
        (&source.tree_hash, "skill_lifecycle_source_tree_hash"),
    ] {
        validate_non_empty_text(value, field, "autonomous_run_request_invalid")?;
        validate_autonomous_artifact_text(value, field)?;
    }

    if source.tree_hash.len() != 40
        || source
            .tree_hash
            .chars()
            .any(|ch| !ch.is_ascii_hexdigit() || ch.is_ascii_uppercase())
    {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            "Cadence requires skill_lifecycle source tree_hash values to be lowercase 40-character hexadecimal Git tree hashes.",
        ));
    }

    Ok(())
}

fn validate_autonomous_artifact_text(value: &str, field: &str) -> Result<(), CommandError> {
    if let Some(secret_hint) = find_prohibited_runtime_persistence_content(value) {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            format!(
                "Autonomous artifact field `{field}` must not include {secret_hint}. Remove secret-bearing content before retrying."
            ),
        ));
    }

    Ok(())
}

fn validate_bounded_autonomous_artifact_text(
    value: &str,
    field: &str,
    max_chars: usize,
) -> Result<(), CommandError> {
    validate_autonomous_artifact_text(value, field)?;
    if value.chars().count() > max_chars {
        return Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            format!(
                "Autonomous artifact field `{field}` must be <= {max_chars} characters after sanitization."
            ),
        ));
    }
    Ok(())
}

fn validate_browser_computer_use_status_for_tool_state(
    tool_state: &AutonomousToolCallStateRecord,
    status: &BrowserComputerUseActionStatus,
) -> Result<(), CommandError> {
    let allowed = match tool_state {
        AutonomousToolCallStateRecord::Pending => {
            matches!(status, BrowserComputerUseActionStatus::Pending)
        }
        AutonomousToolCallStateRecord::Running => matches!(
            status,
            BrowserComputerUseActionStatus::Pending | BrowserComputerUseActionStatus::Running
        ),
        AutonomousToolCallStateRecord::Succeeded => {
            matches!(status, BrowserComputerUseActionStatus::Succeeded)
        }
        AutonomousToolCallStateRecord::Failed => matches!(
            status,
            BrowserComputerUseActionStatus::Failed | BrowserComputerUseActionStatus::Blocked
        ),
    };

    if allowed {
        Ok(())
    } else {
        Err(CommandError::user_fixable(
            "autonomous_run_request_invalid",
            "Cadence rejected browser/computer-use tool_summary metadata whose status does not match the tool_state lifecycle.",
        ))
    }
}

fn autonomous_artifact_payload_kind(payload: &AutonomousArtifactPayloadRecord) -> &'static str {
    match payload {
        AutonomousArtifactPayloadRecord::ToolResult(_) => AUTONOMOUS_ARTIFACT_KIND_TOOL_RESULT,
        AutonomousArtifactPayloadRecord::VerificationEvidence(_) => {
            AUTONOMOUS_ARTIFACT_KIND_VERIFICATION_EVIDENCE
        }
        AutonomousArtifactPayloadRecord::PolicyDenied(_) => AUTONOMOUS_ARTIFACT_KIND_POLICY_DENIED,
        AutonomousArtifactPayloadRecord::SkillLifecycle(_) => {
            AUTONOMOUS_ARTIFACT_KIND_SKILL_LIFECYCLE
        }
    }
}

fn autonomous_artifact_kind_requires_payload(kind: &str) -> bool {
    matches!(
        kind,
        AUTONOMOUS_ARTIFACT_KIND_TOOL_RESULT
            | AUTONOMOUS_ARTIFACT_KIND_VERIFICATION_EVIDENCE
            | AUTONOMOUS_ARTIFACT_KIND_POLICY_DENIED
            | AUTONOMOUS_ARTIFACT_KIND_SKILL_LIFECYCLE
    )
}

fn canonicalize_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted = std::collections::BTreeMap::new();
            for (key, nested) in map {
                sorted.insert(key, canonicalize_json_value(nested));
            }

            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonicalize_json_value).collect())
        }
        other => other,
    }
}

fn parse_autonomous_run_status(value: &str) -> Result<AutonomousRunStatus, String> {
    match value {
        "starting" => Ok(AutonomousRunStatus::Starting),
        "running" => Ok(AutonomousRunStatus::Running),
        "paused" => Ok(AutonomousRunStatus::Paused),
        "cancelling" => Ok(AutonomousRunStatus::Cancelling),
        "cancelled" => Ok(AutonomousRunStatus::Cancelled),
        "stale" => Ok(AutonomousRunStatus::Stale),
        "failed" => Ok(AutonomousRunStatus::Failed),
        "stopped" => Ok(AutonomousRunStatus::Stopped),
        "crashed" => Ok(AutonomousRunStatus::Crashed),
        "completed" => Ok(AutonomousRunStatus::Completed),
        other => Err(format!(
            "must be a known autonomous-run status, found `{other}`."
        )),
    }
}

fn autonomous_run_status_sql_value(value: &AutonomousRunStatus) -> &'static str {
    match value {
        AutonomousRunStatus::Starting => "starting",
        AutonomousRunStatus::Running => "running",
        AutonomousRunStatus::Paused => "paused",
        AutonomousRunStatus::Cancelling => "cancelling",
        AutonomousRunStatus::Cancelled => "cancelled",
        AutonomousRunStatus::Stale => "stale",
        AutonomousRunStatus::Failed => "failed",
        AutonomousRunStatus::Stopped => "stopped",
        AutonomousRunStatus::Crashed => "crashed",
        AutonomousRunStatus::Completed => "completed",
    }
}

fn parse_autonomous_unit_kind(value: &str) -> Result<AutonomousUnitKind, String> {
    match value {
        "researcher" => Ok(AutonomousUnitKind::Researcher),
        "planner" => Ok(AutonomousUnitKind::Planner),
        "executor" => Ok(AutonomousUnitKind::Executor),
        "verifier" => Ok(AutonomousUnitKind::Verifier),
        other => Err(format!(
            "must be a known autonomous-unit kind, found `{other}`."
        )),
    }
}

fn autonomous_unit_kind_sql_value(value: &AutonomousUnitKind) -> &'static str {
    match value {
        AutonomousUnitKind::Researcher => "researcher",
        AutonomousUnitKind::Planner => "planner",
        AutonomousUnitKind::Executor => "executor",
        AutonomousUnitKind::Verifier => "verifier",
    }
}

fn parse_autonomous_unit_status(value: &str) -> Result<AutonomousUnitStatus, String> {
    match value {
        "pending" => Ok(AutonomousUnitStatus::Pending),
        "active" => Ok(AutonomousUnitStatus::Active),
        "blocked" => Ok(AutonomousUnitStatus::Blocked),
        "paused" => Ok(AutonomousUnitStatus::Paused),
        "completed" => Ok(AutonomousUnitStatus::Completed),
        "cancelled" => Ok(AutonomousUnitStatus::Cancelled),
        "failed" => Ok(AutonomousUnitStatus::Failed),
        other => Err(format!(
            "must be a known autonomous-unit status, found `{other}`."
        )),
    }
}

fn autonomous_unit_status_sql_value(value: &AutonomousUnitStatus) -> &'static str {
    match value {
        AutonomousUnitStatus::Pending => "pending",
        AutonomousUnitStatus::Active => "active",
        AutonomousUnitStatus::Blocked => "blocked",
        AutonomousUnitStatus::Paused => "paused",
        AutonomousUnitStatus::Completed => "completed",
        AutonomousUnitStatus::Cancelled => "cancelled",
        AutonomousUnitStatus::Failed => "failed",
    }
}

fn parse_autonomous_unit_artifact_status(
    value: &str,
) -> Result<AutonomousUnitArtifactStatus, String> {
    match value {
        "pending" => Ok(AutonomousUnitArtifactStatus::Pending),
        "recorded" => Ok(AutonomousUnitArtifactStatus::Recorded),
        "rejected" => Ok(AutonomousUnitArtifactStatus::Rejected),
        "redacted" => Ok(AutonomousUnitArtifactStatus::Redacted),
        other => Err(format!(
            "must be a known autonomous-artifact status, found `{other}`."
        )),
    }
}

fn autonomous_unit_artifact_status_sql_value(value: &AutonomousUnitArtifactStatus) -> &'static str {
    match value {
        AutonomousUnitArtifactStatus::Pending => "pending",
        AutonomousUnitArtifactStatus::Recorded => "recorded",
        AutonomousUnitArtifactStatus::Rejected => "rejected",
        AutonomousUnitArtifactStatus::Redacted => "redacted",
    }
}
