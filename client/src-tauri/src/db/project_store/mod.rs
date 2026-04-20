use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, Error as SqlError, ErrorCode, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};

use crate::{
    commands::{
        CommandError, OperatorApprovalDto, OperatorApprovalStatus, PhaseStatus, PhaseStep,
        ProjectSnapshotResponseDto, ResumeHistoryEntryDto, ResumeHistoryStatus,
        VerificationRecordDto, VerificationRecordStatus,
    },
    db::database_path_for_repo,
    notifications::{
        route_target::parse_notification_route_target_for_kind, NotificationRouteKind,
    },
};

mod autonomous;
mod connection;
mod project_snapshot;
mod runtime;
pub(crate) mod workflow;

pub use autonomous::*;
pub(crate) use connection::{open_project_database, open_runtime_database};
pub use project_snapshot::{load_project_snapshot, load_project_summary};
pub(crate) use project_snapshot::{
    planning_lifecycle_stage_label, read_phase_summaries, read_planning_lifecycle_projection,
};
pub use runtime::*;
pub(crate) use runtime::{
    find_prohibited_runtime_persistence_content, find_prohibited_transition_diagnostic_content,
    map_runtime_run_write_error, normalize_runtime_checkpoint_summary, read_runtime_run_row,
    read_runtime_run_snapshot, read_runtime_session_row, runtime_run_checkpoint_kind_sql_value,
    validate_runtime_action_required_payload,
};
pub use workflow::*;
pub(crate) use workflow::{
    apply_workflow_transition_mutation, attempt_automatic_dispatch_after_transition,
    compute_workflow_handoff_package_hash, decode_operator_resume_transition_context,
    derive_resume_transition_id, read_latest_transition_id, read_transition_event_by_transition_id,
    read_workflow_handoff_package_by_transition_id, resolve_operator_approval_gate_link,
    validate_workflow_handoff_package_hash, validate_workflow_handoff_package_transition_linkage,
    OperatorApprovalGateLink, WorkflowTransitionGateMutationRecord,
    WorkflowTransitionMutationApplyOutcome, WorkflowTransitionMutationRecord,
    OPERATOR_RESUME_MUTATION_ERROR_PROFILE,
};

const MAX_APPROVAL_REQUEST_ROWS: i64 = 50;
const MAX_VERIFICATION_RECORD_ROWS: i64 = 100;
const MAX_RESUME_HISTORY_ROWS: i64 = 100;
const MAX_NOTIFICATION_ROUTE_ROWS: i64 = 128;
const MAX_NOTIFICATION_DISPATCH_ROWS: i64 = 256;
const MAX_NOTIFICATION_PENDING_DISPATCH_BATCH_ROWS: i64 = 64;
const MAX_NOTIFICATION_REPLY_CLAIM_ROWS: i64 = 512;
const NOTIFICATION_CORRELATION_KEY_PREFIX: &str = "nfy";
const NOTIFICATION_CORRELATION_KEY_HEX_LEN: usize = 32;

#[derive(Debug, Clone)]
pub struct ProjectSnapshotRecord {
    pub snapshot: ProjectSnapshotResponseDto,
    pub database_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorApprovalDecision {
    Approved,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveOperatorActionRecord {
    pub approval_request: OperatorApprovalDto,
    pub verification_record: VerificationRecordDto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeOperatorRunRecord {
    pub approval_request: OperatorApprovalDto,
    pub resume_entry: ResumeHistoryEntryDto,
    pub automatic_dispatch: Option<WorkflowAutomaticDispatchOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRuntimeOperatorResume {
    pub project_id: String,
    pub approval_request: OperatorApprovalDto,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub run_id: String,
    pub boundary_id: String,
    pub user_answer: String,
}

#[derive(Debug)]
struct ProjectSummaryRow {
    id: String,
    name: String,
    description: String,
    milestone: String,
    branch: Option<String>,
    runtime: Option<String>,
}

#[derive(Debug)]
struct RawNotificationRouteRow {
    project_id: String,
    route_id: String,
    route_kind: String,
    route_target: String,
    enabled: i64,
    metadata_json: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug)]
struct RawNotificationDispatchRow {
    id: i64,
    project_id: String,
    action_id: String,
    route_id: String,
    correlation_key: String,
    status: String,
    attempt_count: i64,
    last_attempt_at: Option<String>,
    delivered_at: Option<String>,
    claimed_at: Option<String>,
    last_error_code: Option<String>,
    last_error_message: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug)]
struct RawNotificationReplyClaimRow {
    id: i64,
    project_id: String,
    action_id: String,
    route_id: String,
    correlation_key: String,
    responder_id: Option<String>,
    reply_text: String,
    status: String,
    rejection_code: Option<String>,
    rejection_message: Option<String>,
    created_at: String,
}

#[derive(Debug)]
struct RawOperatorApprovalRow {
    action_id: String,
    session_id: Option<String>,
    flow_id: Option<String>,
    action_type: String,
    title: String,
    detail: String,
    gate_node_id: Option<String>,
    gate_key: Option<String>,
    transition_from_node_id: Option<String>,
    transition_to_node_id: Option<String>,
    transition_kind: Option<String>,
    user_answer: Option<String>,
    status: String,
    decision_note: Option<String>,
    created_at: String,
    updated_at: String,
    resolved_at: Option<String>,
}

#[derive(Debug)]
struct RawVerificationRecordRow {
    id: i64,
    source_action_id: Option<String>,
    status: String,
    summary: String,
    detail: Option<String>,
    recorded_at: String,
}

#[derive(Debug)]
struct RawResumeHistoryRow {
    id: i64,
    source_action_id: Option<String>,
    session_id: Option<String>,
    status: String,
    summary: String,
    created_at: String,
}

pub fn upsert_runtime_action_required(
    repo_root: &Path,
    payload: &RuntimeActionRequiredUpsertRecord,
) -> Result<RuntimeActionRequiredPersistedRecord, CommandError> {
    validate_runtime_action_required_payload(payload)?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &payload.project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "runtime_action_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the runtime action-required transaction.",
        )
    })?;

    let runtime_row = read_runtime_run_row(&transaction, &database_path, &payload.project_id)?
        .ok_or_else(|| {
            CommandError::retryable(
                "runtime_action_request_invalid",
                format!(
                    "Cadence could not persist action-required runtime state in {} because the selected project has no durable run row.",
                    database_path.display()
                ),
            )
        })?;

    if runtime_row.run_id != payload.run_id {
        return Err(CommandError::retryable(
            "runtime_action_request_invalid",
            format!(
                "Cadence refused to persist runtime action-required state for run `{}` because the durable run row currently points at `{}`.",
                payload.run_id, runtime_row.run_id
            ),
        ));
    }

    let action_id = derive_runtime_action_id(
        &payload.session_id,
        payload.flow_id.as_deref(),
        &payload.run_id,
        &payload.boundary_id,
        &payload.action_type,
    )?;

    let existing = read_operator_approval_by_action_id(
        &transaction,
        &database_path,
        &payload.project_id,
        &action_id,
    )?;
    match existing {
        None => {
            transaction
                .execute(
                    r#"
                    INSERT INTO operator_approvals (
                        project_id,
                        action_id,
                        session_id,
                        flow_id,
                        action_type,
                        title,
                        detail,
                        gate_node_id,
                        gate_key,
                        transition_from_node_id,
                        transition_to_node_id,
                        transition_kind,
                        user_answer,
                        status,
                        decision_note,
                        created_at,
                        updated_at,
                        resolved_at
                    )
                    VALUES (
                        ?1,
                        ?2,
                        ?3,
                        ?4,
                        ?5,
                        ?6,
                        ?7,
                        NULL,
                        NULL,
                        NULL,
                        NULL,
                        NULL,
                        NULL,
                        'pending',
                        NULL,
                        ?8,
                        ?8,
                        NULL
                    )
                    "#,
                    params![
                        payload.project_id.as_str(),
                        action_id.as_str(),
                        payload.session_id.as_str(),
                        payload.flow_id.as_deref(),
                        payload.action_type.as_str(),
                        payload.title.as_str(),
                        payload.detail.as_str(),
                        payload.created_at.as_str(),
                    ],
                )
                .map_err(|error| {
                    map_operator_loop_write_error(
                        "runtime_action_persist_failed",
                        &database_path,
                        error,
                        "Cadence could not persist the runtime action-required approval row.",
                    )
                })?;
        }
        Some(approval) => match approval.status {
            OperatorApprovalStatus::Pending => {
                transaction
                    .execute(
                        r#"
                        UPDATE operator_approvals
                        SET session_id = ?3,
                            flow_id = ?4,
                            title = ?5,
                            detail = ?6,
                            updated_at = ?7
                        WHERE project_id = ?1
                          AND action_id = ?2
                          AND status = 'pending'
                        "#,
                        params![
                            payload.project_id.as_str(),
                            action_id.as_str(),
                            payload.session_id.as_str(),
                            payload.flow_id.as_deref(),
                            payload.title.as_str(),
                            payload.detail.as_str(),
                            payload.created_at.as_str(),
                        ],
                    )
                    .map_err(|error| {
                        map_operator_loop_write_error(
                            "runtime_action_persist_failed",
                            &database_path,
                            error,
                            "Cadence could not refresh the runtime action-required approval row.",
                        )
                    })?;
            }
            OperatorApprovalStatus::Approved | OperatorApprovalStatus::Rejected => {
                return Err(CommandError::retryable(
                    "runtime_action_sync_conflict",
                    format!(
                        "Cadence received a retained runtime action for already-resolved operator request `{action_id}`. Refresh selected project state before retrying."
                    ),
                ));
            }
        },
    }

    let next_sequence = runtime_row.last_checkpoint_sequence.saturating_add(1);
    let (last_error_code, last_error_message) = payload
        .last_error
        .as_ref()
        .map(|error| (Some(error.code.as_str()), Some(error.message.as_str())))
        .unwrap_or((None, None));

    transaction
        .execute(
            r#"
            UPDATE runtime_runs
            SET runtime_kind = ?3,
                supervisor_kind = ?4,
                status = 'running',
                transport_kind = ?5,
                transport_endpoint = ?6,
                transport_liveness = 'reachable',
                last_checkpoint_sequence = ?7,
                started_at = ?8,
                last_heartbeat_at = ?9,
                last_checkpoint_at = ?10,
                stopped_at = NULL,
                last_error_code = ?11,
                last_error_message = ?12,
                updated_at = ?10
            WHERE project_id = ?1
              AND run_id = ?2
            "#,
            params![
                payload.project_id.as_str(),
                payload.run_id.as_str(),
                payload.runtime_kind.as_str(),
                "detached_pty",
                "tcp",
                payload.transport_endpoint.as_str(),
                i64::from(next_sequence),
                payload.started_at.as_str(),
                payload.last_heartbeat_at.as_deref(),
                payload.created_at.as_str(),
                last_error_code,
                last_error_message,
            ],
        )
        .map_err(|error| {
            map_runtime_run_write_error(
                "runtime_action_persist_failed",
                &database_path,
                error,
                "Cadence could not update the runtime run row while persisting action-required state.",
            )
        })?;

    transaction
        .execute(
            r#"
            INSERT INTO runtime_run_checkpoints (
                project_id,
                run_id,
                sequence,
                kind,
                summary,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                payload.project_id.as_str(),
                payload.run_id.as_str(),
                i64::from(next_sequence),
                runtime_run_checkpoint_kind_sql_value(&RuntimeRunCheckpointKind::ActionRequired),
                normalize_runtime_checkpoint_summary(&payload.checkpoint_summary),
                payload.created_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_runtime_run_write_error(
                "runtime_action_persist_failed",
                &database_path,
                error,
                "Cadence could not persist the runtime action-required checkpoint.",
            )
        })?;

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "runtime_action_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the runtime action-required transaction.",
        )
    })?;

    let approval_request =
        read_operator_approval_by_action_id(&connection, &database_path, &payload.project_id, &action_id)?
            .ok_or_else(|| {
                CommandError::system_fault(
                    "runtime_action_missing_after_persist",
                    format!(
                        "Cadence persisted runtime action-required state for `{action_id}` in {} but could not read the approval row back.",
                        database_path.display()
                    ),
                )
            })?;
    let runtime_run = read_runtime_run_snapshot(&connection, &database_path, &payload.project_id)?
        .ok_or_else(|| {
            CommandError::system_fault(
                "runtime_action_missing_after_persist",
                format!(
                    "Cadence persisted runtime action-required state in {} but could not read the durable runtime run back.",
                    database_path.display()
                ),
            )
        })?;
    let notification_dispatch_outcome = enqueue_notification_dispatches_best_effort_with_connection(
        &connection,
        &database_path,
        &NotificationDispatchEnqueueRecord {
            project_id: payload.project_id.clone(),
            action_id: action_id.clone(),
            enqueued_at: payload.created_at.clone(),
        },
    );

    Ok(RuntimeActionRequiredPersistedRecord {
        approval_request,
        runtime_run,
        notification_dispatch_outcome,
    })
}

pub fn upsert_pending_operator_approval(
    repo_root: &Path,
    project_id: &str,
    session_id: &str,
    flow_id: Option<&str>,
    action_type: &str,
    title: &str,
    detail: &str,
    created_at: &str,
) -> Result<OperatorApprovalDto, CommandError> {
    upsert_pending_operator_approval_with_gate_link(
        repo_root,
        project_id,
        session_id,
        flow_id,
        action_type,
        title,
        detail,
        created_at,
        None,
    )
}

pub fn upsert_pending_operator_approval_with_gate_link(
    repo_root: &Path,
    project_id: &str,
    session_id: &str,
    flow_id: Option<&str>,
    action_type: &str,
    title: &str,
    detail: &str,
    created_at: &str,
    gate_link: Option<&OperatorApprovalGateLinkInput>,
) -> Result<OperatorApprovalDto, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "operator_approval_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the operator-approval transaction.",
        )
    })?;

    let gate_link = match gate_link {
        Some(gate_link) => Some(validate_operator_approval_gate_link_input(
            gate_link,
            action_type,
        )?),
        None => resolve_operator_approval_gate_link(
            &transaction,
            &database_path,
            project_id,
            action_type,
            title,
            detail,
        )?,
    };
    let action_id =
        derive_operator_action_id(session_id, flow_id, action_type, gate_link.as_ref())?;

    let existing =
        read_operator_approval_by_action_id(&transaction, &database_path, project_id, &action_id)?;
    match existing {
        None => {
            transaction
                .execute(
                    r#"
                    INSERT INTO operator_approvals (
                        project_id,
                        action_id,
                        session_id,
                        flow_id,
                        action_type,
                        title,
                        detail,
                        gate_node_id,
                        gate_key,
                        transition_from_node_id,
                        transition_to_node_id,
                        transition_kind,
                        user_answer,
                        status,
                        decision_note,
                        created_at,
                        updated_at,
                        resolved_at
                    )
                    VALUES (
                        ?1,
                        ?2,
                        ?3,
                        ?4,
                        ?5,
                        ?6,
                        ?7,
                        ?8,
                        ?9,
                        ?10,
                        ?11,
                        ?12,
                        NULL,
                        'pending',
                        NULL,
                        ?13,
                        ?13,
                        NULL
                    )
                    "#,
                    params![
                        project_id,
                        action_id,
                        session_id,
                        flow_id,
                        action_type,
                        title,
                        detail,
                        gate_link.as_ref().map(|link| link.gate_node_id.as_str()),
                        gate_link.as_ref().map(|link| link.gate_key.as_str()),
                        gate_link
                            .as_ref()
                            .map(|link| link.transition_from_node_id.as_str()),
                        gate_link
                            .as_ref()
                            .map(|link| link.transition_to_node_id.as_str()),
                        gate_link.as_ref().map(|link| link.transition_kind.as_str()),
                        created_at,
                    ],
                )
                .map_err(|error| {
                    map_operator_loop_write_error(
                        "operator_approval_upsert_failed",
                        &database_path,
                        error,
                        "Cadence could not persist the pending operator approval.",
                    )
                })?;
        }
        Some(approval) => match approval.status {
            OperatorApprovalStatus::Pending => {
                transaction
                    .execute(
                        r#"
                        UPDATE operator_approvals
                        SET session_id = ?3,
                            flow_id = ?4,
                            title = ?5,
                            detail = ?6,
                            gate_node_id = ?7,
                            gate_key = ?8,
                            transition_from_node_id = ?9,
                            transition_to_node_id = ?10,
                            transition_kind = ?11,
                            updated_at = ?12
                        WHERE project_id = ?1
                          AND action_id = ?2
                          AND status = 'pending'
                        "#,
                        params![
                            project_id,
                            action_id,
                            session_id,
                            flow_id,
                            title,
                            detail,
                            gate_link.as_ref().map(|link| link.gate_node_id.as_str()),
                            gate_link.as_ref().map(|link| link.gate_key.as_str()),
                            gate_link
                                .as_ref()
                                .map(|link| link.transition_from_node_id.as_str()),
                            gate_link
                                .as_ref()
                                .map(|link| link.transition_to_node_id.as_str()),
                            gate_link.as_ref().map(|link| link.transition_kind.as_str()),
                            created_at,
                        ],
                    )
                    .map_err(|error| {
                        map_operator_loop_write_error(
                            "operator_approval_upsert_failed",
                            &database_path,
                            error,
                            "Cadence could not refresh the pending operator approval.",
                        )
                    })?;
            }
            OperatorApprovalStatus::Approved | OperatorApprovalStatus::Rejected => {
                return Err(CommandError::retryable(
                    "runtime_action_sync_conflict",
                    format!(
                        "Cadence received a retained runtime action for already-resolved operator request `{action_id}`. Reopen or refresh the selected project before retrying."
                    ),
                ));
            }
        },
    }

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "operator_approval_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the pending operator approval.",
        )
    })?;

    let _ = enqueue_notification_dispatches_best_effort_with_connection(
        &connection,
        &database_path,
        &NotificationDispatchEnqueueRecord {
            project_id: project_id.to_string(),
            action_id: action_id.clone(),
            enqueued_at: created_at.to_string(),
        },
    );

    read_operator_approval_by_action_id(&connection, &database_path, project_id, &action_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "operator_approval_missing_after_persist",
            format!(
                "Cadence persisted operator approval `{action_id}` in {} but could not read it back.",
                database_path.display()
            ),
        )
    })
}

pub fn upsert_notification_route(
    repo_root: &Path,
    route: &NotificationRouteUpsertRecord,
) -> Result<NotificationRouteRecord, CommandError> {
    let validated_route = validate_notification_route_upsert_payload(route)?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &route.project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "notification_route_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the notification-route transaction.",
        )
    })?;

    let metadata_json = normalize_optional_notification_metadata_json(
        route.metadata_json.as_deref(),
        "notification_route_request_invalid",
    )?;

    transaction
        .execute(
            r#"
            INSERT INTO notification_routes (
                project_id,
                route_id,
                route_kind,
                route_target,
                enabled,
                metadata_json,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
            ON CONFLICT(project_id, route_id) DO UPDATE SET
                route_kind = excluded.route_kind,
                route_target = excluded.route_target,
                enabled = excluded.enabled,
                metadata_json = excluded.metadata_json,
                updated_at = excluded.updated_at
            "#,
            params![
                route.project_id.as_str(),
                route.route_id.as_str(),
                validated_route.route_kind.as_str(),
                validated_route.canonical_route_target.as_str(),
                if route.enabled { 1_i64 } else { 0_i64 },
                metadata_json.as_deref(),
                route.updated_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "notification_route_upsert_failed",
                &database_path,
                error,
                "Cadence could not persist notification-route metadata.",
            )
        })?;

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "notification_route_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the notification-route transaction.",
        )
    })?;

    read_notification_route_by_id(
        &connection,
        &database_path,
        &route.project_id,
        &route.route_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "notification_route_missing_after_persist",
            format!(
                "Cadence persisted notification route `{}` in {} but could not read it back.",
                route.route_id,
                database_path.display()
            ),
        )
    })
}

pub fn load_notification_routes(
    repo_root: &Path,
    project_id: &str,
) -> Result<Vec<NotificationRouteRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "project_id",
        "notification_route_request_invalid",
    )?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    read_notification_routes(&connection, &database_path, project_id)
}

pub fn enqueue_notification_dispatches(
    repo_root: &Path,
    enqueue: &NotificationDispatchEnqueueRecord,
) -> Result<Vec<NotificationDispatchRecord>, CommandError> {
    validate_notification_dispatch_enqueue_payload(enqueue)?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &enqueue.project_id)?;

    enqueue_notification_dispatches_with_connection(&connection, &database_path, enqueue)
}

fn enqueue_notification_dispatches_with_connection(
    connection: &Connection,
    database_path: &Path,
    enqueue: &NotificationDispatchEnqueueRecord,
) -> Result<Vec<NotificationDispatchRecord>, CommandError> {
    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "notification_dispatch_transaction_failed",
            database_path,
            error,
            "Cadence could not start the notification-dispatch enqueue transaction.",
        )
    })?;

    let approval = read_operator_approval_by_action_id(
        &transaction,
        database_path,
        &enqueue.project_id,
        &enqueue.action_id,
    )?
    .ok_or_else(|| {
        CommandError::user_fixable(
            "notification_dispatch_action_not_found",
            format!(
                "Cadence could not enqueue notification dispatches because operator action `{}` was not found for project `{}`.",
                enqueue.action_id, enqueue.project_id
            ),
        )
    })?;

    if approval.status != OperatorApprovalStatus::Pending {
        return Err(CommandError::user_fixable(
            "notification_dispatch_action_not_pending",
            format!(
                "Cadence can only enqueue notification dispatches for pending operator actions. Action `{}` is currently {}.",
                enqueue.action_id,
                operator_approval_status_label(&approval.status)
            ),
        ));
    }

    let routes = read_notification_routes(&transaction, database_path, &enqueue.project_id)?;

    for route in routes.iter().filter(|route| route.enabled) {
        let correlation_key = derive_notification_correlation_key(
            &enqueue.project_id,
            &enqueue.action_id,
            &route.route_id,
        );

        transaction
            .execute(
                r#"
                INSERT INTO notification_dispatches (
                    project_id,
                    action_id,
                    route_id,
                    correlation_key,
                    status,
                    attempt_count,
                    last_attempt_at,
                    delivered_at,
                    claimed_at,
                    last_error_code,
                    last_error_message,
                    created_at,
                    updated_at
                )
                VALUES (
                    ?1,
                    ?2,
                    ?3,
                    ?4,
                    'pending',
                    0,
                    NULL,
                    NULL,
                    NULL,
                    NULL,
                    NULL,
                    ?5,
                    ?5
                )
                ON CONFLICT(project_id, action_id, route_id) DO NOTHING
                "#,
                params![
                    enqueue.project_id.as_str(),
                    enqueue.action_id.as_str(),
                    route.route_id.as_str(),
                    correlation_key.as_str(),
                    enqueue.enqueued_at.as_str(),
                ],
            )
            .map_err(|error| {
                map_operator_loop_write_error(
                    "notification_dispatch_enqueue_failed",
                    database_path,
                    error,
                    "Cadence could not persist notification dispatch fan-out rows.",
                )
            })?;
    }

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "notification_dispatch_commit_failed",
            database_path,
            error,
            "Cadence could not commit notification dispatch fan-out rows.",
        )
    })?;

    read_notification_dispatches(
        connection,
        database_path,
        &enqueue.project_id,
        Some(&enqueue.action_id),
    )
}

fn enqueue_notification_dispatches_best_effort_with_connection(
    connection: &Connection,
    database_path: &Path,
    enqueue: &NotificationDispatchEnqueueRecord,
) -> NotificationDispatchEnqueueOutcomeRecord {
    match enqueue_notification_dispatches_with_connection(connection, database_path, enqueue) {
        Ok(dispatches) if dispatches.is_empty() => NotificationDispatchEnqueueOutcomeRecord {
            status: NotificationDispatchEnqueueStatus::Skipped,
            dispatch_count: 0,
            code: Some("notification_dispatch_enqueue_skipped".into()),
            message: Some(format!(
                "Cadence skipped notification dispatch fan-out for operator action `{}` because no enabled routes are configured for project `{}`.",
                enqueue.action_id, enqueue.project_id
            )),
        },
        Ok(dispatches) => NotificationDispatchEnqueueOutcomeRecord {
            status: NotificationDispatchEnqueueStatus::Enqueued,
            dispatch_count: dispatches.len() as u32,
            code: Some("notification_dispatch_enqueued".into()),
            message: Some(format!(
                "Cadence enqueued {} notification dispatch route(s) for operator action `{}`.",
                dispatches.len(), enqueue.action_id
            )),
        },
        Err(error) => NotificationDispatchEnqueueOutcomeRecord {
            status: NotificationDispatchEnqueueStatus::Skipped,
            dispatch_count: 0,
            code: Some(error.code),
            message: Some(error.message),
        },
    }
}

fn format_notification_dispatch_enqueue_outcome(
    outcome: &NotificationDispatchEnqueueOutcomeRecord,
) -> String {
    let code = outcome
        .code
        .as_deref()
        .unwrap_or("notification_dispatch_enqueue_skipped");
    let message = outcome
        .message
        .as_deref()
        .unwrap_or("Cadence skipped notification dispatch fan-out.");

    match outcome.status {
        NotificationDispatchEnqueueStatus::Enqueued => format!(
            "{code}: {message} (dispatch_count={}).",
            outcome.dispatch_count
        ),
        NotificationDispatchEnqueueStatus::Skipped => format!("{code}: {message}"),
    }
}

pub fn load_notification_dispatches(
    repo_root: &Path,
    project_id: &str,
    action_id: Option<&str>,
) -> Result<Vec<NotificationDispatchRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "project_id",
        "notification_dispatch_request_invalid",
    )?;
    if let Some(action_id) = action_id {
        validate_non_empty_text(
            action_id,
            "action_id",
            "notification_dispatch_request_invalid",
        )?;
    }

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    read_notification_dispatches(&connection, &database_path, project_id, action_id)
}

pub fn load_pending_notification_dispatches(
    repo_root: &Path,
    project_id: &str,
    limit: Option<u32>,
) -> Result<Vec<NotificationDispatchRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "project_id",
        "notification_dispatch_request_invalid",
    )?;

    let limit = limit
        .map(i64::from)
        .unwrap_or(MAX_NOTIFICATION_PENDING_DISPATCH_BATCH_ROWS)
        .clamp(1, MAX_NOTIFICATION_PENDING_DISPATCH_BATCH_ROWS);

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    read_pending_notification_dispatches(&connection, &database_path, project_id, limit)
}

pub fn record_notification_dispatch_outcome(
    repo_root: &Path,
    outcome: &NotificationDispatchOutcomeUpdateRecord,
) -> Result<NotificationDispatchRecord, CommandError> {
    validate_notification_dispatch_outcome_update_payload(outcome)?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &outcome.project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "notification_dispatch_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the notification-dispatch outcome transaction.",
        )
    })?;

    let existing = read_notification_dispatch_by_route(
        &transaction,
        &database_path,
        &outcome.project_id,
        &outcome.action_id,
        &outcome.route_id,
    )?
    .ok_or_else(|| {
        CommandError::user_fixable(
            "notification_dispatch_not_found",
            format!(
                "Cadence could not record dispatch outcome because `{}`/`{}`/`{}` was not found.",
                outcome.project_id, outcome.action_id, outcome.route_id
            ),
        )
    })?;

    if existing.status == NotificationDispatchStatus::Claimed {
        return Err(CommandError::user_fixable(
            "notification_dispatch_already_claimed",
            format!(
                "Cadence refused to overwrite dispatch outcome for route `{}` because action `{}` has already been claimed for reply correlation.",
                existing.route_id, existing.action_id
            ),
        ));
    }

    let attempt_count = existing.attempt_count.saturating_add(1);
    let (last_error_code, last_error_message, delivered_at) = match outcome.status {
        NotificationDispatchStatus::Sent => (None, None, Some(outcome.attempted_at.as_str())),
        NotificationDispatchStatus::Failed => (
            outcome.error_code.as_deref(),
            outcome.error_message.as_deref(),
            existing.delivered_at.as_deref(),
        ),
        NotificationDispatchStatus::Pending | NotificationDispatchStatus::Claimed => {
            return Err(CommandError::user_fixable(
                "notification_dispatch_outcome_invalid",
                "Dispatch outcomes must use `sent` or `failed` status updates.",
            ))
        }
    };

    transaction
        .execute(
            r#"
            UPDATE notification_dispatches
            SET status = ?2,
                attempt_count = ?3,
                last_attempt_at = ?4,
                delivered_at = ?5,
                last_error_code = ?6,
                last_error_message = ?7,
                updated_at = ?4
            WHERE id = ?1
            "#,
            params![
                existing.id,
                notification_dispatch_status_sql_value(&outcome.status),
                i64::from(attempt_count),
                outcome.attempted_at.as_str(),
                delivered_at,
                last_error_code,
                last_error_message,
            ],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "notification_dispatch_update_failed",
                &database_path,
                error,
                "Cadence could not persist notification dispatch outcome metadata.",
            )
        })?;

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "notification_dispatch_commit_failed",
            &database_path,
            error,
            "Cadence could not commit notification dispatch outcome metadata.",
        )
    })?;

    read_notification_dispatch_by_id(&connection, &database_path, existing.id)?.ok_or_else(|| {
        CommandError::system_fault(
            "notification_dispatch_missing_after_persist",
            format!(
                "Cadence persisted notification dispatch outcome in {} but could not read row {} back.",
                database_path.display(),
                existing.id
            ),
        )
    })
}

pub fn claim_notification_reply(
    repo_root: &Path,
    request: &NotificationReplyClaimRequestRecord,
) -> Result<NotificationReplyClaimResultRecord, CommandError> {
    validate_notification_reply_claim_request_payload(request)?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &request.project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "notification_reply_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the notification-reply claim transaction.",
        )
    })?;

    let approval = read_operator_approval_by_action_id(
        &transaction,
        &database_path,
        &request.project_id,
        &request.action_id,
    )?;

    if approval.is_none() {
        let message = format!(
            "Cadence rejected the notification reply because action `{}` is not pending for project `{}`.",
            request.action_id, request.project_id
        );
        persist_notification_reply_rejection(
            &transaction,
            &database_path,
            request,
            "notification_reply_correlation_invalid",
            &message,
        )?;
        transaction.commit().map_err(|error| {
            map_operator_loop_commit_error(
                "notification_reply_commit_failed",
                &database_path,
                error,
                "Cadence could not commit the rejected notification reply claim.",
            )
        })?;

        return Err(CommandError::user_fixable(
            "notification_reply_correlation_invalid",
            message,
        ));
    }

    let approval = approval.expect("checked above");
    if approval.status != OperatorApprovalStatus::Pending {
        let message = format!(
            "Cadence rejected the notification reply because action `{}` is already {}.",
            request.action_id,
            operator_approval_status_label(&approval.status)
        );
        persist_notification_reply_rejection(
            &transaction,
            &database_path,
            request,
            "notification_reply_already_claimed",
            &message,
        )?;
        transaction.commit().map_err(|error| {
            map_operator_loop_commit_error(
                "notification_reply_commit_failed",
                &database_path,
                error,
                "Cadence could not commit the rejected notification reply claim.",
            )
        })?;

        return Err(CommandError::user_fixable(
            "notification_reply_already_claimed",
            message,
        ));
    }

    let Some(dispatch) = read_notification_dispatch_by_route(
        &transaction,
        &database_path,
        &request.project_id,
        &request.action_id,
        &request.route_id,
    )?
    else {
        let message = format!(
            "Cadence rejected the notification reply because route `{}` is not linked to action `{}` for project `{}`.",
            request.route_id, request.action_id, request.project_id
        );
        persist_notification_reply_rejection(
            &transaction,
            &database_path,
            request,
            "notification_reply_correlation_invalid",
            &message,
        )?;
        transaction.commit().map_err(|error| {
            map_operator_loop_commit_error(
                "notification_reply_commit_failed",
                &database_path,
                error,
                "Cadence could not commit the rejected notification reply claim.",
            )
        })?;

        return Err(CommandError::user_fixable(
            "notification_reply_correlation_invalid",
            message,
        ));
    };

    if dispatch.correlation_key != request.correlation_key {
        let message = format!(
            "Cadence rejected the notification reply because correlation key `{}` does not match route `{}` for action `{}`.",
            request.correlation_key, request.route_id, request.action_id
        );
        persist_notification_reply_rejection(
            &transaction,
            &database_path,
            request,
            "notification_reply_correlation_invalid",
            &message,
        )?;
        transaction.commit().map_err(|error| {
            map_operator_loop_commit_error(
                "notification_reply_commit_failed",
                &database_path,
                error,
                "Cadence could not commit the rejected notification reply claim.",
            )
        })?;

        return Err(CommandError::user_fixable(
            "notification_reply_correlation_invalid",
            message,
        ));
    }

    if let Some(existing_winner) = read_notification_winning_reply_claim(
        &transaction,
        &database_path,
        &request.project_id,
        &request.action_id,
    )? {
        let message = format!(
            "Cadence rejected the notification reply because action `{}` was already claimed by route `{}` at {}.",
            request.action_id, existing_winner.route_id, existing_winner.created_at
        );
        persist_notification_reply_rejection(
            &transaction,
            &database_path,
            request,
            "notification_reply_already_claimed",
            &message,
        )?;
        transaction.commit().map_err(|error| {
            map_operator_loop_commit_error(
                "notification_reply_commit_failed",
                &database_path,
                error,
                "Cadence could not commit the rejected notification reply claim.",
            )
        })?;

        return Err(CommandError::user_fixable(
            "notification_reply_already_claimed",
            message,
        ));
    }

    let accepted_insert = transaction.execute(
        r#"
        INSERT INTO notification_reply_claims (
            project_id,
            action_id,
            route_id,
            correlation_key,
            responder_id,
            reply_text,
            status,
            rejection_code,
            rejection_message,
            created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'accepted', NULL, NULL, ?7)
        "#,
        params![
            request.project_id.as_str(),
            request.action_id.as_str(),
            request.route_id.as_str(),
            request.correlation_key.as_str(),
            request.responder_id.as_deref(),
            request.reply_text.as_str(),
            request.received_at.as_str(),
        ],
    );

    if let Err(error) = accepted_insert {
        if is_unique_constraint_violation(&error) {
            let message = format!(
                "Cadence rejected the notification reply because action `{}` was already claimed by another route.",
                request.action_id
            );
            persist_notification_reply_rejection(
                &transaction,
                &database_path,
                request,
                "notification_reply_already_claimed",
                &message,
            )?;
            transaction.commit().map_err(|commit_error| {
                map_operator_loop_commit_error(
                    "notification_reply_commit_failed",
                    &database_path,
                    commit_error,
                    "Cadence could not commit the rejected notification reply claim.",
                )
            })?;

            return Err(CommandError::user_fixable(
                "notification_reply_already_claimed",
                message,
            ));
        }

        return Err(map_operator_loop_write_error(
            "notification_reply_claim_persist_failed",
            &database_path,
            error,
            "Cadence could not persist the accepted notification reply claim.",
        ));
    }

    let accepted_claim_id = transaction.last_insert_rowid();

    transaction
        .execute(
            r#"
            UPDATE notification_dispatches
            SET status = 'claimed',
                claimed_at = ?2,
                updated_at = ?2
            WHERE id = ?1
            "#,
            params![dispatch.id, request.received_at.as_str()],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "notification_reply_dispatch_update_failed",
                &database_path,
                error,
                "Cadence could not persist notification-dispatch claim metadata.",
            )
        })?;

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "notification_reply_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the notification reply claim transaction.",
        )
    })?;

    let claim = read_notification_reply_claim_by_id(&connection, &database_path, accepted_claim_id)?
        .ok_or_else(|| {
            CommandError::system_fault(
                "notification_reply_missing_after_persist",
                format!(
                    "Cadence persisted accepted notification reply claim `{accepted_claim_id}` in {} but could not read it back.",
                    database_path.display()
                ),
            )
        })?;
    let dispatch =
        read_notification_dispatch_by_id(&connection, &database_path, dispatch.id)?.ok_or_else(
            || {
                CommandError::system_fault(
                    "notification_dispatch_missing_after_persist",
                    format!(
                        "Cadence persisted notification dispatch claim metadata in {} but could not read row {} back.",
                        database_path.display(),
                        dispatch.id
                    ),
                )
            },
        )?;

    Ok(NotificationReplyClaimResultRecord { claim, dispatch })
}

pub fn load_notification_reply_claims(
    repo_root: &Path,
    project_id: &str,
    action_id: Option<&str>,
) -> Result<Vec<NotificationReplyClaimRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "project_id",
        "notification_reply_request_invalid",
    )?;
    if let Some(action_id) = action_id {
        validate_non_empty_text(action_id, "action_id", "notification_reply_request_invalid")?;
    }

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    read_notification_reply_claims(&connection, &database_path, project_id, action_id)
}

pub fn resolve_operator_action(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
    decision: OperatorApprovalDecision,
    decision_note: Option<&str>,
) -> Result<ResolveOperatorActionRecord, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "operator_action_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the operator-action transaction.",
        )
    })?;

    let existing =
        read_operator_approval_by_action_id(&transaction, &database_path, project_id, action_id)?
            .ok_or_else(|| {
            CommandError::user_fixable(
                "operator_action_not_found",
                format!(
                "Cadence could not find operator request `{action_id}` for the selected project."
            ),
            )
        })?;

    if existing.status != OperatorApprovalStatus::Pending {
        return Err(CommandError::user_fixable(
            "operator_action_already_resolved",
            format!(
                "Cadence cannot change operator request `{action_id}` because it is already {}.",
                operator_approval_status_label(&existing.status)
            ),
        ));
    }

    let decision_note = decision_note.map(str::trim).filter(|note| !note.is_empty());

    if let Some(secret_hint) = decision_note.and_then(find_prohibited_transition_diagnostic_content)
    {
        return Err(CommandError::user_fixable(
            "operator_action_decision_payload_invalid",
            format!(
                "Operator decision payload for `{action_id}` must not include {secret_hint}. Remove secret-bearing transcript/tool/auth material before retrying."
            ),
        ));
    }

    let answer_requirement = if matches!(decision, OperatorApprovalDecision::Approved) {
        classify_operator_answer_requirement(&existing)?
    } else {
        None
    };

    if answer_requirement.is_some() && decision_note.is_none() {
        return Err(CommandError::user_fixable(
            "operator_action_answer_required",
            format!(
                "Cadence requires a non-empty user answer before approving required-input operator request `{action_id}`."
            ),
        ));
    }

    let resolved_at = crate::auth::now_timestamp();
    let (approval_status, verification_status, verification_summary) = match decision {
        OperatorApprovalDecision::Approved => (
            "approved",
            "passed",
            format!("Approved operator action: {}.", existing.title),
        ),
        OperatorApprovalDecision::Rejected => (
            "rejected",
            "failed",
            format!("Rejected operator action: {}.", existing.title),
        ),
    };

    transaction
        .execute(
            r#"
            UPDATE operator_approvals
            SET status = ?3,
                decision_note = ?4,
                user_answer = ?5,
                updated_at = ?6,
                resolved_at = ?6
            WHERE project_id = ?1
              AND action_id = ?2
              AND status = 'pending'
            "#,
            params![
                project_id,
                action_id,
                approval_status,
                decision_note,
                decision_note,
                resolved_at,
            ],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "operator_action_resolve_failed",
                &database_path,
                error,
                "Cadence could not persist the operator decision.",
            )
        })?;

    let verification_id = transaction
        .execute(
            r#"
            INSERT INTO operator_verification_records (
                project_id,
                source_action_id,
                status,
                summary,
                detail,
                recorded_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                project_id,
                action_id,
                verification_status,
                verification_summary,
                decision_note,
                resolved_at,
            ],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "operator_verification_persist_failed",
                &database_path,
                error,
                "Cadence could not persist the operator verification record.",
            )
        })?;

    debug_assert_eq!(verification_id, 1);
    let verification_row_id = transaction.last_insert_rowid();

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "operator_action_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the operator decision.",
        )
    })?;

    let approval_request =
        read_operator_approval_by_action_id(&connection, &database_path, project_id, action_id)?
            .ok_or_else(|| {
                CommandError::system_fault(
                    "operator_action_missing_after_persist",
                    format!(
                "Cadence resolved operator request `{action_id}` in {} but could not read it back.",
                database_path.display()
            ),
                )
            })?;
    let verification_record = read_verification_record_by_id(
        &connection,
        &database_path,
        project_id,
        verification_row_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "operator_verification_missing_after_persist",
            format!(
                "Cadence persisted the operator verification record for `{action_id}` in {} but could not read it back.",
                database_path.display()
            ),
        )
    })?;

    Ok(ResolveOperatorActionRecord {
        approval_request,
        verification_record,
    })
}

fn classify_operator_answer_requirement(
    approval_request: &OperatorApprovalDto,
) -> Result<Option<ResolveOperatorAnswerRequirement>, CommandError> {
    if approval_request.gate_node_id.is_some() {
        return Ok(Some(ResolveOperatorAnswerRequirement::GateLinked));
    }

    match decode_runtime_operator_resume_target(approval_request) {
        Ok(Some(_)) => Ok(Some(ResolveOperatorAnswerRequirement::RuntimeResumable)),
        Ok(None) => Ok(None),
        Err(error) if error.code == "operator_resume_runtime_action_invalid" => {
            Err(CommandError::retryable(
                "operator_action_runtime_identity_invalid",
                format!(
                    "Cadence cannot resolve runtime-scoped operator request `{}` because its durable runtime action identity is malformed.",
                    approval_request.action_id
                ),
            ))
        }
        Err(error) => Err(error),
    }
}

pub fn prepare_runtime_operator_run_resume(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
    expected_user_answer: Option<&str>,
) -> Result<Option<PreparedRuntimeOperatorResume>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let approval_request =
        read_operator_approval_by_action_id(&connection, &database_path, project_id, action_id)?
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "operator_action_not_found",
                    format!(
                "Cadence could not find operator request `{action_id}` for the selected project."
            ),
                )
            })?;

    let Some(runtime_target) = decode_runtime_operator_resume_target(&approval_request)? else {
        return Ok(None);
    };

    if approval_request.status != OperatorApprovalStatus::Approved {
        return Err(CommandError::user_fixable(
            "operator_resume_requires_approved_action",
            format!(
                "Cadence can resume only after operator request `{action_id}` is approved. Current status: {}.",
                operator_approval_status_label(&approval_request.status)
            ),
        ));
    }

    let durable_user_answer = approval_request
        .user_answer
        .as_deref()
        .map(str::trim)
        .filter(|answer| !answer.is_empty());
    let durable_decision_note = approval_request
        .decision_note
        .as_deref()
        .map(str::trim)
        .filter(|note| !note.is_empty());

    if durable_decision_note.is_some() && durable_user_answer != durable_decision_note {
        return Err(CommandError::retryable(
            "operator_resume_answer_conflict",
            format!(
                "Cadence cannot resume operator request `{action_id}` because the durable approved answer metadata is inconsistent. Refresh project state and re-approve the pending runtime boundary before retrying."
            ),
        ));
    }

    let expected_user_answer = expected_user_answer.map(str::trim);
    if let Some(expected_user_answer) = expected_user_answer {
        if expected_user_answer.is_empty() {
            return Err(CommandError::invalid_request("userAnswer"));
        }

        if let Some(secret_hint) =
            find_prohibited_transition_diagnostic_content(expected_user_answer)
        {
            return Err(CommandError::user_fixable(
                "operator_resume_decision_payload_invalid",
                format!(
                    "Operator decision payload for `{action_id}` must not include {secret_hint}. Remove secret-bearing transcript/tool/auth material before retrying."
                ),
            ));
        }

        if durable_user_answer != Some(expected_user_answer) {
            return Err(CommandError::user_fixable(
                "operator_resume_answer_conflict",
                format!(
                    "Cadence cannot resume operator request `{action_id}` because the provided `userAnswer` does not match the durable gate decision. Refresh project state and retry with the stored answer."
                ),
            ));
        }
    }

    let session_id = approval_request.session_id.as_deref().ok_or_else(|| {
        CommandError::retryable(
            "operator_resume_session_missing",
            format!(
                "Cadence cannot record a resume event for `{action_id}` because the durable approval is missing its runtime session id."
            ),
        )
    })?;
    let user_answer = durable_user_answer.ok_or_else(|| {
        CommandError::user_fixable(
            "operator_resume_answer_missing",
            format!(
                "Cadence cannot resume operator request `{action_id}` because no approved operator answer was recorded for the pending terminal input."
            ),
        )
    })?;

    if let Some(secret_hint) = find_prohibited_transition_diagnostic_content(user_answer) {
        return Err(CommandError::user_fixable(
            "operator_resume_decision_payload_invalid",
            format!(
                "Operator decision payload for `{action_id}` must not include {secret_hint}. Remove secret-bearing transcript/tool/auth material before retrying."
            ),
        ));
    }

    let flow_id = approval_request.flow_id.clone();
    let session_id = session_id.to_string();
    let user_answer = user_answer.to_string();

    Ok(Some(PreparedRuntimeOperatorResume {
        project_id: project_id.to_string(),
        approval_request,
        session_id,
        flow_id,
        run_id: runtime_target.run_id,
        boundary_id: runtime_target.boundary_id,
        user_answer,
    }))
}

pub fn record_runtime_operator_resume_outcome(
    repo_root: &Path,
    resume: &PreparedRuntimeOperatorResume,
    status: ResumeHistoryStatus,
    summary: &str,
) -> Result<ResumeOperatorRunRecord, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &resume.project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "operator_resume_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the operator-resume transaction.",
        )
    })?;

    let created_at = crate::auth::now_timestamp();
    let fallback_summary = match status {
        ResumeHistoryStatus::Started => format!(
            "Operator resumed the selected project's runtime session after approving {}.",
            resume.approval_request.title
        ),
        ResumeHistoryStatus::Failed => format!(
            "Cadence could not resume the selected project's runtime session after approving {}.",
            resume.approval_request.title
        ),
    };
    let summary = normalize_runtime_resume_history_summary(summary, &fallback_summary);

    transaction
        .execute(
            r#"
            INSERT INTO operator_resume_history (
                project_id,
                source_action_id,
                session_id,
                status,
                summary,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                resume.project_id.as_str(),
                resume.approval_request.action_id.as_str(),
                resume.session_id.as_str(),
                resume_history_status_sql_value(&status),
                summary.as_str(),
                created_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "operator_resume_persist_failed",
                &database_path,
                error,
                "Cadence could not persist the operator resume event.",
            )
        })?;

    let resume_row_id = transaction.last_insert_rowid();

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "operator_resume_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the operator resume event.",
        )
    })?;

    let approval_request = read_operator_approval_by_action_id(
        &connection,
        &database_path,
        &resume.project_id,
        &resume.approval_request.action_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "operator_action_missing_after_resume",
            format!(
                "Cadence recorded a resume event for `{}` in {} but could not reload the approval row.",
                resume.approval_request.action_id,
                database_path.display()
            ),
        )
    })?;
    let resume_entry = read_resume_history_entry_by_id(
        &connection,
        &database_path,
        &resume.project_id,
        resume_row_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "operator_resume_missing_after_persist",
            format!(
                "Cadence persisted the operator resume entry for `{}` in {} but could not read it back.",
                resume.approval_request.action_id,
                database_path.display()
            ),
        )
    })?;

    Ok(ResumeOperatorRunRecord {
        approval_request,
        resume_entry,
        automatic_dispatch: None,
    })
}

pub fn resume_operator_run(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
) -> Result<ResumeOperatorRunRecord, CommandError> {
    resume_operator_run_with_user_answer(repo_root, project_id, action_id, None)
}

pub fn resume_operator_run_with_user_answer(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
    expected_user_answer: Option<&str>,
) -> Result<ResumeOperatorRunRecord, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "operator_resume_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the operator-resume transaction.",
        )
    })?;

    let approval_request =
        read_operator_approval_by_action_id(&transaction, &database_path, project_id, action_id)?
            .ok_or_else(|| {
            CommandError::user_fixable(
                "operator_action_not_found",
                format!(
                "Cadence could not find operator request `{action_id}` for the selected project."
            ),
            )
        })?;

    if approval_request.status != OperatorApprovalStatus::Approved {
        return Err(CommandError::user_fixable(
            "operator_resume_requires_approved_action",
            format!(
                "Cadence can resume only after operator request `{action_id}` is approved. Current status: {}.",
                operator_approval_status_label(&approval_request.status)
            ),
        ));
    }

    let expected_user_answer = expected_user_answer.map(str::trim);
    if let Some(expected_user_answer) = expected_user_answer {
        if expected_user_answer.is_empty() {
            return Err(CommandError::invalid_request("userAnswer"));
        }

        if let Some(secret_hint) =
            find_prohibited_transition_diagnostic_content(expected_user_answer)
        {
            return Err(CommandError::user_fixable(
                "operator_resume_decision_payload_invalid",
                format!(
                    "Operator decision payload for `{action_id}` must not include {secret_hint}. Remove secret-bearing transcript/tool/auth material before retrying."
                ),
            ));
        }

        let actual_user_answer = approval_request.user_answer.as_deref().map(str::trim);
        if actual_user_answer != Some(expected_user_answer) {
            return Err(CommandError::user_fixable(
                "operator_resume_answer_conflict",
                format!(
                    "Cadence cannot resume operator request `{action_id}` because the provided `userAnswer` does not match the durable gate decision. Refresh project state and retry with the stored answer."
                ),
            ));
        }
    }

    let session_id = approval_request.session_id.as_deref().ok_or_else(|| {
        CommandError::retryable(
            "operator_resume_session_missing",
            format!(
                "Cadence cannot record a resume event for `{action_id}` because the durable approval is missing its runtime session id."
            ),
        )
    })?;

    let transition_context =
        decode_operator_resume_transition_context(&approval_request, action_id)?;

    let created_at = crate::auth::now_timestamp();
    let mut summary = format!(
        "Operator resumed the selected project's runtime session after approving {}.",
        approval_request.title
    );
    let mut completion_transition_id: Option<String> = None;

    if let Some(context) = transition_context {
        let transition_id = derive_resume_transition_id(action_id, &context);
        completion_transition_id = Some(transition_id.clone());

        let mutation_outcome = if let Some(existing) = read_transition_event_by_transition_id(
            &transaction,
            &database_path,
            project_id,
            &transition_id,
        )? {
            WorkflowTransitionMutationApplyOutcome::Replayed(existing)
        } else {
            let causal_transition_id =
                read_latest_transition_id(&transaction, &database_path, project_id)?;

            let transition = WorkflowTransitionMutationRecord {
                transition_id,
                causal_transition_id,
                from_node_id: context.transition_from_node_id.clone(),
                to_node_id: context.transition_to_node_id.clone(),
                transition_kind: context.transition_kind.clone(),
                gate_decision: WorkflowTransitionGateDecision::Approved,
                gate_decision_context: Some(context.user_answer.clone()),
                gate_updates: vec![WorkflowTransitionGateMutationRecord {
                    node_id: context.gate_node_id.clone(),
                    gate_key: context.gate_key.clone(),
                    gate_state: WorkflowGateState::Satisfied,
                    decision_context: Some(context.user_answer.clone()),
                    require_pending_or_blocked: true,
                }],
                required_gate_requirement: Some(context.gate_key.clone()),
                occurred_at: created_at.clone(),
            };

            apply_workflow_transition_mutation(
                &transaction,
                &database_path,
                project_id,
                &transition,
                &OPERATOR_RESUME_MUTATION_ERROR_PROFILE,
                map_operator_loop_write_error,
            )?
        };

        summary = match mutation_outcome {
            WorkflowTransitionMutationApplyOutcome::Applied => format!(
                "Operator resumed the selected project's runtime session after approving {} and applied transition {} -> {} ({}).",
                approval_request.title,
                context.transition_from_node_id,
                context.transition_to_node_id,
                context.transition_kind,
            ),
            WorkflowTransitionMutationApplyOutcome::Replayed(_) => format!(
                "Operator resumed the selected project's runtime session after approving {} and reused existing transition {} -> {} ({}).",
                approval_request.title,
                context.transition_from_node_id,
                context.transition_to_node_id,
                context.transition_kind,
            ),
        };
    }

    transaction
        .execute(
            r#"
            INSERT INTO operator_resume_history (
                project_id,
                source_action_id,
                session_id,
                status,
                summary,
                created_at
            )
            VALUES (?1, ?2, ?3, 'started', ?4, ?5)
            "#,
            params![project_id, action_id, session_id, summary, created_at],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "operator_resume_persist_failed",
                &database_path,
                error,
                "Cadence could not persist the operator resume event.",
            )
        })?;

    let resume_row_id = transaction.last_insert_rowid();

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "operator_resume_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the operator resume event.",
        )
    })?;

    let approval_request =
        read_operator_approval_by_action_id(&connection, &database_path, project_id, action_id)?
            .ok_or_else(|| {
                CommandError::system_fault(
                    "operator_action_missing_after_resume",
                    format!(
                        "Cadence recorded a resume event for `{action_id}` in {} but could not reload the approval row.",
                        database_path.display()
                    ),
                )
            })?;
    let resume_entry =
        read_resume_history_entry_by_id(&connection, &database_path, project_id, resume_row_id)?
            .ok_or_else(|| {
                CommandError::system_fault(
                    "operator_resume_missing_after_persist",
                    format!(
                        "Cadence persisted the operator resume entry for `{action_id}` in {} but could not read it back.",
                        database_path.display()
                    ),
                )
            })?;

    let automatic_dispatch = if let Some(transition_id) = completion_transition_id {
        let transition_event = read_transition_event_by_transition_id(
            &connection,
            &database_path,
            project_id,
            &transition_id,
        )?
        .ok_or_else(|| {
            CommandError::system_fault(
                "workflow_transition_event_missing_after_persist",
                format!(
                    "Cadence persisted transition `{transition_id}` in {} but could not read it back.",
                    database_path.display()
                ),
            )
        })?;

        Some(attempt_automatic_dispatch_after_transition(
            &mut connection,
            &database_path,
            project_id,
            &transition_event,
        ))
    } else {
        None
    };

    Ok(ResumeOperatorRunRecord {
        approval_request,
        resume_entry,
        automatic_dispatch,
    })
}

fn read_notification_routes(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
) -> Result<Vec<NotificationRouteRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                route_id,
                route_kind,
                route_target,
                enabled,
                metadata_json,
                created_at,
                updated_at
            FROM notification_routes
            WHERE project_id = ?1
            ORDER BY enabled DESC, updated_at DESC, route_id ASC
            LIMIT ?2
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_route_query_failed",
                format!(
                    "Cadence could not prepare notification-route rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map(params![project_id, MAX_NOTIFICATION_ROUTE_ROWS], |row| {
            Ok(RawNotificationRouteRow {
                project_id: row.get(0)?,
                route_id: row.get(1)?,
                route_kind: row.get(2)?,
                route_target: row.get(3)?,
                enabled: row.get(4)?,
                metadata_json: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "notification_route_query_failed",
                format!(
                    "Cadence could not query notification-route rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "notification_route_decode_failed",
                        format!(
                            "Cadence could not decode notification-route rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_notification_route_row(raw_row, database_path))
        })
        .collect()
}

fn read_notification_route_by_id(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    route_id: &str,
) -> Result<Option<NotificationRouteRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                route_id,
                route_kind,
                route_target,
                enabled,
                metadata_json,
                created_at,
                updated_at
            FROM notification_routes
            WHERE project_id = ?1
              AND route_id = ?2
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_route_query_failed",
                format!(
                    "Cadence could not prepare notification-route lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement
        .query(params![project_id, route_id])
        .map_err(|error| {
            CommandError::system_fault(
                "notification_route_query_failed",
                format!(
                    "Cadence could not query notification-route lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_route_query_failed",
            format!(
                "Cadence could not read notification-route lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_notification_route_row(
        RawNotificationRouteRow {
            project_id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_kind: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_target: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            enabled: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            metadata_json: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            updated_at: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_notification_dispatches(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    action_id: Option<&str>,
) -> Result<Vec<NotificationDispatchRecord>, CommandError> {
    let mut statement = if action_id.is_some() {
        connection
            .prepare(
                r#"
                SELECT
                    id,
                    project_id,
                    action_id,
                    route_id,
                    correlation_key,
                    status,
                    attempt_count,
                    last_attempt_at,
                    delivered_at,
                    claimed_at,
                    last_error_code,
                    last_error_message,
                    created_at,
                    updated_at
                FROM notification_dispatches
                WHERE project_id = ?1
                  AND action_id = ?2
                ORDER BY created_at ASC, id ASC
                LIMIT ?3
                "#,
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_query_failed",
                    format!(
                        "Cadence could not prepare notification-dispatch rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?
    } else {
        connection
            .prepare(
                r#"
                SELECT
                    id,
                    project_id,
                    action_id,
                    route_id,
                    correlation_key,
                    status,
                    attempt_count,
                    last_attempt_at,
                    delivered_at,
                    claimed_at,
                    last_error_code,
                    last_error_message,
                    created_at,
                    updated_at
                FROM notification_dispatches
                WHERE project_id = ?1
                ORDER BY updated_at DESC, id DESC
                LIMIT ?2
                "#,
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_query_failed",
                    format!(
                        "Cadence could not prepare notification-dispatch rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?
    };

    if let Some(action_id) = action_id {
        let raw_rows = statement
            .query_map(
                params![project_id, action_id, MAX_NOTIFICATION_DISPATCH_ROWS],
                |row| {
                    Ok(RawNotificationDispatchRow {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        action_id: row.get(2)?,
                        route_id: row.get(3)?,
                        correlation_key: row.get(4)?,
                        status: row.get(5)?,
                        attempt_count: row.get(6)?,
                        last_attempt_at: row.get(7)?,
                        delivered_at: row.get(8)?,
                        claimed_at: row.get(9)?,
                        last_error_code: row.get(10)?,
                        last_error_message: row.get(11)?,
                        created_at: row.get(12)?,
                        updated_at: row.get(13)?,
                    })
                },
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_query_failed",
                    format!(
                        "Cadence could not query notification-dispatch rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?;

        raw_rows
            .map(|raw_row| {
                raw_row
                    .map_err(|error| {
                        CommandError::system_fault(
                            "notification_dispatch_decode_failed",
                            format!(
                                "Cadence could not decode notification-dispatch rows from {}: {error}",
                                database_path.display()
                            ),
                        )
                    })
                    .and_then(|raw_row| decode_notification_dispatch_row(raw_row, database_path))
            })
            .collect()
    } else {
        let raw_rows = statement
            .query_map(params![project_id, MAX_NOTIFICATION_DISPATCH_ROWS], |row| {
                Ok(RawNotificationDispatchRow {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    action_id: row.get(2)?,
                    route_id: row.get(3)?,
                    correlation_key: row.get(4)?,
                    status: row.get(5)?,
                    attempt_count: row.get(6)?,
                    last_attempt_at: row.get(7)?,
                    delivered_at: row.get(8)?,
                    claimed_at: row.get(9)?,
                    last_error_code: row.get(10)?,
                    last_error_message: row.get(11)?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            })
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_query_failed",
                    format!(
                        "Cadence could not query notification-dispatch rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?;

        raw_rows
            .map(|raw_row| {
                raw_row
                    .map_err(|error| {
                        CommandError::system_fault(
                            "notification_dispatch_decode_failed",
                            format!(
                                "Cadence could not decode notification-dispatch rows from {}: {error}",
                                database_path.display()
                            ),
                        )
                    })
                    .and_then(|raw_row| decode_notification_dispatch_row(raw_row, database_path))
            })
            .collect()
    }
}

fn read_pending_notification_dispatches(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    limit: i64,
) -> Result<Vec<NotificationDispatchRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                action_id,
                route_id,
                correlation_key,
                status,
                attempt_count,
                last_attempt_at,
                delivered_at,
                claimed_at,
                last_error_code,
                last_error_message,
                created_at,
                updated_at
            FROM notification_dispatches
            WHERE project_id = ?1
              AND status = 'pending'
            ORDER BY
                COALESCE(last_attempt_at, created_at) ASC,
                created_at ASC,
                id ASC
            LIMIT ?2
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_dispatch_query_failed",
                format!(
                    "Cadence could not prepare pending notification-dispatch query from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map(params![project_id, limit], |row| {
            Ok(RawNotificationDispatchRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                action_id: row.get(2)?,
                route_id: row.get(3)?,
                correlation_key: row.get(4)?,
                status: row.get(5)?,
                attempt_count: row.get(6)?,
                last_attempt_at: row.get(7)?,
                delivered_at: row.get(8)?,
                claimed_at: row.get(9)?,
                last_error_code: row.get(10)?,
                last_error_message: row.get(11)?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "notification_dispatch_query_failed",
                format!(
                    "Cadence could not query pending notification-dispatch rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "notification_dispatch_decode_failed",
                        format!(
                            "Cadence could not decode pending notification-dispatch rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_notification_dispatch_row(raw_row, database_path))
        })
        .collect()
}

fn read_notification_dispatch_by_route(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    action_id: &str,
    route_id: &str,
) -> Result<Option<NotificationDispatchRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                action_id,
                route_id,
                correlation_key,
                status,
                attempt_count,
                last_attempt_at,
                delivered_at,
                claimed_at,
                last_error_code,
                last_error_message,
                created_at,
                updated_at
            FROM notification_dispatches
            WHERE project_id = ?1
              AND action_id = ?2
              AND route_id = ?3
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_dispatch_query_failed",
                format!(
                    "Cadence could not prepare notification-dispatch lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement
        .query(params![project_id, action_id, route_id])
        .map_err(|error| {
            CommandError::system_fault(
                "notification_dispatch_query_failed",
                format!(
                    "Cadence could not query notification-dispatch lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_dispatch_query_failed",
            format!(
                "Cadence could not read notification-dispatch lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_notification_dispatch_row(
        RawNotificationDispatchRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            correlation_key: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            attempt_count: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_attempt_at: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            delivered_at: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            claimed_at: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_error_code: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_error_message: row.get(11).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(12).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            updated_at: row.get(13).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_notification_dispatch_by_id(
    connection: &Connection,
    database_path: &Path,
    id: i64,
) -> Result<Option<NotificationDispatchRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                action_id,
                route_id,
                correlation_key,
                status,
                attempt_count,
                last_attempt_at,
                delivered_at,
                claimed_at,
                last_error_code,
                last_error_message,
                created_at,
                updated_at
            FROM notification_dispatches
            WHERE id = ?1
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_dispatch_query_failed",
                format!(
                    "Cadence could not prepare notification-dispatch id lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement.query(params![id]).map_err(|error| {
        CommandError::system_fault(
            "notification_dispatch_query_failed",
            format!(
                "Cadence could not query notification-dispatch id lookup from {}: {error}",
                database_path.display()
            ),
        )
    })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_dispatch_query_failed",
            format!(
                "Cadence could not read notification-dispatch id lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_notification_dispatch_row(
        RawNotificationDispatchRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            correlation_key: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            attempt_count: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_attempt_at: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            delivered_at: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            claimed_at: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_error_code: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_error_message: row.get(11).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(12).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            updated_at: row.get(13).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_notification_reply_claims(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    action_id: Option<&str>,
) -> Result<Vec<NotificationReplyClaimRecord>, CommandError> {
    let mut statement = if action_id.is_some() {
        connection
            .prepare(
                r#"
                SELECT
                    id,
                    project_id,
                    action_id,
                    route_id,
                    correlation_key,
                    responder_id,
                    reply_text,
                    status,
                    rejection_code,
                    rejection_message,
                    created_at
                FROM notification_reply_claims
                WHERE project_id = ?1
                  AND action_id = ?2
                ORDER BY created_at DESC, id DESC
                LIMIT ?3
                "#,
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_query_failed",
                    format!(
                        "Cadence could not prepare notification-reply claim rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?
    } else {
        connection
            .prepare(
                r#"
                SELECT
                    id,
                    project_id,
                    action_id,
                    route_id,
                    correlation_key,
                    responder_id,
                    reply_text,
                    status,
                    rejection_code,
                    rejection_message,
                    created_at
                FROM notification_reply_claims
                WHERE project_id = ?1
                ORDER BY created_at DESC, id DESC
                LIMIT ?2
                "#,
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_query_failed",
                    format!(
                        "Cadence could not prepare notification-reply claim rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?
    };

    if let Some(action_id) = action_id {
        let raw_rows = statement
            .query_map(
                params![project_id, action_id, MAX_NOTIFICATION_REPLY_CLAIM_ROWS],
                |row| {
                    Ok(RawNotificationReplyClaimRow {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        action_id: row.get(2)?,
                        route_id: row.get(3)?,
                        correlation_key: row.get(4)?,
                        responder_id: row.get(5)?,
                        reply_text: row.get(6)?,
                        status: row.get(7)?,
                        rejection_code: row.get(8)?,
                        rejection_message: row.get(9)?,
                        created_at: row.get(10)?,
                    })
                },
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_query_failed",
                    format!(
                        "Cadence could not query notification-reply claim rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?;

        raw_rows
            .map(|raw_row| {
                raw_row
                    .map_err(|error| {
                        CommandError::system_fault(
                            "notification_reply_decode_failed",
                            format!(
                                "Cadence could not decode notification-reply claim rows from {}: {error}",
                                database_path.display()
                            ),
                        )
                    })
                    .and_then(|raw_row| decode_notification_reply_claim_row(raw_row, database_path))
            })
            .collect()
    } else {
        let raw_rows = statement
            .query_map(
                params![project_id, MAX_NOTIFICATION_REPLY_CLAIM_ROWS],
                |row| {
                    Ok(RawNotificationReplyClaimRow {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        action_id: row.get(2)?,
                        route_id: row.get(3)?,
                        correlation_key: row.get(4)?,
                        responder_id: row.get(5)?,
                        reply_text: row.get(6)?,
                        status: row.get(7)?,
                        rejection_code: row.get(8)?,
                        rejection_message: row.get(9)?,
                        created_at: row.get(10)?,
                    })
                },
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_query_failed",
                    format!(
                        "Cadence could not query notification-reply claim rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?;

        raw_rows
            .map(|raw_row| {
                raw_row
                    .map_err(|error| {
                        CommandError::system_fault(
                            "notification_reply_decode_failed",
                            format!(
                                "Cadence could not decode notification-reply claim rows from {}: {error}",
                                database_path.display()
                            ),
                        )
                    })
                    .and_then(|raw_row| decode_notification_reply_claim_row(raw_row, database_path))
            })
            .collect()
    }
}

fn read_notification_reply_claim_by_id(
    connection: &Connection,
    database_path: &Path,
    id: i64,
) -> Result<Option<NotificationReplyClaimRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                action_id,
                route_id,
                correlation_key,
                responder_id,
                reply_text,
                status,
                rejection_code,
                rejection_message,
                created_at
            FROM notification_reply_claims
            WHERE id = ?1
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_reply_query_failed",
                format!(
                    "Cadence could not prepare notification-reply claim id lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement.query(params![id]).map_err(|error| {
        CommandError::system_fault(
            "notification_reply_query_failed",
            format!(
                "Cadence could not query notification-reply claim id lookup from {}: {error}",
                database_path.display()
            ),
        )
    })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_reply_query_failed",
            format!(
                "Cadence could not read notification-reply claim id lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_notification_reply_claim_row(
        RawNotificationReplyClaimRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            correlation_key: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            responder_id: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            reply_text: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            rejection_code: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            rejection_message: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_notification_winning_reply_claim(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    action_id: &str,
) -> Result<Option<NotificationReplyClaimRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                action_id,
                route_id,
                correlation_key,
                responder_id,
                reply_text,
                status,
                rejection_code,
                rejection_message,
                created_at
            FROM notification_reply_claims
            WHERE project_id = ?1
              AND action_id = ?2
              AND status = 'accepted'
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_reply_query_failed",
                format!(
                    "Cadence could not prepare winning notification-reply lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement
        .query(params![project_id, action_id])
        .map_err(|error| {
            CommandError::system_fault(
                "notification_reply_query_failed",
                format!(
                    "Cadence could not query winning notification-reply lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_reply_query_failed",
            format!(
                "Cadence could not read winning notification-reply lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_notification_reply_claim_row(
        RawNotificationReplyClaimRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            correlation_key: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            responder_id: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            reply_text: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            rejection_code: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            rejection_message: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn persist_notification_reply_rejection(
    transaction: &Transaction<'_>,
    database_path: &Path,
    request: &NotificationReplyClaimRequestRecord,
    rejection_code: &str,
    rejection_message: &str,
) -> Result<i64, CommandError> {
    transaction
        .execute(
            r#"
            INSERT INTO notification_reply_claims (
                project_id,
                action_id,
                route_id,
                correlation_key,
                responder_id,
                reply_text,
                status,
                rejection_code,
                rejection_message,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'rejected', ?7, ?8, ?9)
            "#,
            params![
                request.project_id.as_str(),
                request.action_id.as_str(),
                request.route_id.as_str(),
                request.correlation_key.as_str(),
                request.responder_id.as_deref(),
                request.reply_text.as_str(),
                rejection_code,
                rejection_message,
                request.received_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "notification_reply_claim_persist_failed",
                database_path,
                error,
                "Cadence could not persist the rejected notification reply claim.",
            )
        })?;

    Ok(transaction.last_insert_rowid())
}

fn read_operator_approvals(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Vec<OperatorApprovalDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                action_id,
                session_id,
                flow_id,
                action_type,
                title,
                detail,
                gate_node_id,
                gate_key,
                transition_from_node_id,
                transition_to_node_id,
                transition_kind,
                user_answer,
                status,
                decision_note,
                created_at,
                updated_at,
                resolved_at
            FROM operator_approvals
            WHERE project_id = ?1
            ORDER BY
                CASE status WHEN 'pending' THEN 0 ELSE 1 END ASC,
                COALESCE(resolved_at, updated_at, created_at) DESC,
                action_id ASC
            LIMIT ?2
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "operator_approval_query_failed",
                format!(
                    "Cadence could not prepare operator-approval rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map(
            params![expected_project_id, MAX_APPROVAL_REQUEST_ROWS],
            |row| {
                Ok(RawOperatorApprovalRow {
                    action_id: row.get(0)?,
                    session_id: row.get(1)?,
                    flow_id: row.get(2)?,
                    action_type: row.get(3)?,
                    title: row.get(4)?,
                    detail: row.get(5)?,
                    gate_node_id: row.get(6)?,
                    gate_key: row.get(7)?,
                    transition_from_node_id: row.get(8)?,
                    transition_to_node_id: row.get(9)?,
                    transition_kind: row.get(10)?,
                    user_answer: row.get(11)?,
                    status: row.get(12)?,
                    decision_note: row.get(13)?,
                    created_at: row.get(14)?,
                    updated_at: row.get(15)?,
                    resolved_at: row.get(16)?,
                })
            },
        )
        .map_err(|error| {
            CommandError::system_fault(
                "operator_approval_query_failed",
                format!(
                    "Cadence could not query operator-approval rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "operator_approval_decode_failed",
                        format!(
                            "Cadence could not decode operator-approval rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_operator_approval_row(raw_row, database_path))
        })
        .collect()
}

fn read_verification_records(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Vec<VerificationRecordDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                source_action_id,
                status,
                summary,
                detail,
                recorded_at
            FROM operator_verification_records
            WHERE project_id = ?1
            ORDER BY recorded_at DESC, id DESC
            LIMIT ?2
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "verification_record_query_failed",
                format!(
                    "Cadence could not prepare verification rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map(
            params![expected_project_id, MAX_VERIFICATION_RECORD_ROWS],
            |row| {
                Ok(RawVerificationRecordRow {
                    id: row.get(0)?,
                    source_action_id: row.get(1)?,
                    status: row.get(2)?,
                    summary: row.get(3)?,
                    detail: row.get(4)?,
                    recorded_at: row.get(5)?,
                })
            },
        )
        .map_err(|error| {
            CommandError::system_fault(
                "verification_record_query_failed",
                format!(
                    "Cadence could not query verification rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "verification_record_decode_failed",
                        format!(
                            "Cadence could not decode verification rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_verification_record_row(raw_row, database_path))
        })
        .collect()
}

fn read_resume_history(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Vec<ResumeHistoryEntryDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                source_action_id,
                session_id,
                status,
                summary,
                created_at
            FROM operator_resume_history
            WHERE project_id = ?1
            ORDER BY created_at DESC, id DESC
            LIMIT ?2
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "resume_history_query_failed",
                format!(
                    "Cadence could not prepare resume-history rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map(
            params![expected_project_id, MAX_RESUME_HISTORY_ROWS],
            |row| {
                Ok(RawResumeHistoryRow {
                    id: row.get(0)?,
                    source_action_id: row.get(1)?,
                    session_id: row.get(2)?,
                    status: row.get(3)?,
                    summary: row.get(4)?,
                    created_at: row.get(5)?,
                })
            },
        )
        .map_err(|error| {
            CommandError::system_fault(
                "resume_history_query_failed",
                format!(
                    "Cadence could not query resume-history rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "resume_history_decode_failed",
                        format!(
                            "Cadence could not decode resume-history rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_resume_history_row(raw_row, database_path))
        })
        .collect()
}

fn read_operator_approval_by_action_id(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    action_id: &str,
) -> Result<Option<OperatorApprovalDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                action_id,
                session_id,
                flow_id,
                action_type,
                title,
                detail,
                gate_node_id,
                gate_key,
                transition_from_node_id,
                transition_to_node_id,
                transition_kind,
                user_answer,
                status,
                decision_note,
                created_at,
                updated_at,
                resolved_at
            FROM operator_approvals
            WHERE project_id = ?1
              AND action_id = ?2
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "operator_approval_query_failed",
                format!(
                    "Cadence could not prepare operator-approval lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement
        .query(params![project_id, action_id])
        .map_err(|error| {
            CommandError::system_fault(
                "operator_approval_query_failed",
                format!(
                    "Cadence could not query operator-approval lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "operator_approval_query_failed",
            format!(
                "Cadence could not read operator-approval lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_operator_approval_row(
        RawOperatorApprovalRow {
            action_id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            session_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            flow_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_type: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            title: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            detail: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            gate_node_id: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            gate_key: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            transition_from_node_id: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            transition_to_node_id: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            transition_kind: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            user_answer: row.get(11).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(12).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            decision_note: row.get(13).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(14).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            updated_at: row.get(15).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            resolved_at: row.get(16).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_verification_record_by_id(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    id: i64,
) -> Result<Option<VerificationRecordDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                source_action_id,
                status,
                summary,
                detail,
                recorded_at
            FROM operator_verification_records
            WHERE project_id = ?1
              AND id = ?2
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "verification_record_query_failed",
                format!(
                    "Cadence could not prepare verification-record lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement.query(params![project_id, id]).map_err(|error| {
        CommandError::system_fault(
            "verification_record_query_failed",
            format!(
                "Cadence could not query verification-record lookup from {}: {error}",
                database_path.display()
            ),
        )
    })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "verification_record_query_failed",
            format!(
                "Cadence could not read verification-record lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_verification_record_row(
        RawVerificationRecordRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "verification_record_decode_failed",
                    format!(
                        "Cadence could not decode verification-record lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            source_action_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "verification_record_decode_failed",
                    format!(
                        "Cadence could not decode verification-record lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "verification_record_decode_failed",
                    format!(
                        "Cadence could not decode verification-record lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            summary: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "verification_record_decode_failed",
                    format!(
                        "Cadence could not decode verification-record lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            detail: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "verification_record_decode_failed",
                    format!(
                        "Cadence could not decode verification-record lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            recorded_at: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "verification_record_decode_failed",
                    format!(
                        "Cadence could not decode verification-record lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_resume_history_entry_by_id(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    id: i64,
) -> Result<Option<ResumeHistoryEntryDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                source_action_id,
                session_id,
                status,
                summary,
                created_at
            FROM operator_resume_history
            WHERE project_id = ?1
              AND id = ?2
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "resume_history_query_failed",
                format!(
                    "Cadence could not prepare resume-history lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement.query(params![project_id, id]).map_err(|error| {
        CommandError::system_fault(
            "resume_history_query_failed",
            format!(
                "Cadence could not query resume-history lookup from {}: {error}",
                database_path.display()
            ),
        )
    })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "resume_history_query_failed",
            format!(
                "Cadence could not read resume-history lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_resume_history_row(
        RawResumeHistoryRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "resume_history_decode_failed",
                    format!(
                        "Cadence could not decode resume-history lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            source_action_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "resume_history_decode_failed",
                    format!(
                        "Cadence could not decode resume-history lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            session_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "resume_history_decode_failed",
                    format!(
                        "Cadence could not decode resume-history lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "resume_history_decode_failed",
                    format!(
                        "Cadence could not decode resume-history lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            summary: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "resume_history_decode_failed",
                    format!(
                        "Cadence could not decode resume-history lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "resume_history_decode_failed",
                    format!(
                        "Cadence could not decode resume-history lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn decode_notification_route_row(
    raw_row: RawNotificationRouteRow,
    database_path: &Path,
) -> Result<NotificationRouteRecord, CommandError> {
    let enabled = match raw_row.enabled {
        0 => false,
        1 => true,
        value => {
            return Err(map_snapshot_decode_error(
                "notification_route_decode_failed",
                database_path,
                format!("Field `enabled` must be 0 or 1, found {value}."),
            ))
        }
    };

    let metadata_json = decode_optional_non_empty_text(
        raw_row.metadata_json,
        "metadata_json",
        database_path,
        "notification_route_decode_failed",
    )?;
    if let Some(metadata_json) = metadata_json.as_deref() {
        serde_json::from_str::<serde_json::Value>(metadata_json).map_err(|error| {
            map_snapshot_decode_error(
                "notification_route_decode_failed",
                database_path,
                format!("Field `metadata_json` must be valid JSON text: {error}"),
            )
        })?;
    }

    Ok(NotificationRouteRecord {
        project_id: require_non_empty_owned(
            raw_row.project_id,
            "project_id",
            database_path,
            "notification_route_decode_failed",
        )?,
        route_id: require_non_empty_owned(
            raw_row.route_id,
            "route_id",
            database_path,
            "notification_route_decode_failed",
        )?,
        route_kind: require_non_empty_owned(
            raw_row.route_kind,
            "route_kind",
            database_path,
            "notification_route_decode_failed",
        )?,
        route_target: require_non_empty_owned(
            raw_row.route_target,
            "route_target",
            database_path,
            "notification_route_decode_failed",
        )?,
        enabled,
        metadata_json,
        created_at: require_non_empty_owned(
            raw_row.created_at,
            "created_at",
            database_path,
            "notification_route_decode_failed",
        )?,
        updated_at: require_non_empty_owned(
            raw_row.updated_at,
            "updated_at",
            database_path,
            "notification_route_decode_failed",
        )?,
    })
}

fn decode_notification_dispatch_row(
    raw_row: RawNotificationDispatchRow,
    database_path: &Path,
) -> Result<NotificationDispatchRecord, CommandError> {
    let correlation_key = require_non_empty_owned(
        raw_row.correlation_key,
        "correlation_key",
        database_path,
        "notification_dispatch_decode_failed",
    )?;
    validate_notification_correlation_key(
        &correlation_key,
        "correlation_key",
        "notification_dispatch_decode_failed",
    )?;

    let status = parse_notification_dispatch_status(&raw_row.status).map_err(|details| {
        map_snapshot_decode_error(
            "notification_dispatch_decode_failed",
            database_path,
            details,
        )
    })?;

    let attempt_count = u32::try_from(raw_row.attempt_count).map_err(|_| {
        map_snapshot_decode_error(
            "notification_dispatch_decode_failed",
            database_path,
            format!(
                "Field `attempt_count` must be a non-negative 32-bit integer, found {}.",
                raw_row.attempt_count
            ),
        )
    })?;

    let last_attempt_at = decode_optional_non_empty_text(
        raw_row.last_attempt_at,
        "last_attempt_at",
        database_path,
        "notification_dispatch_decode_failed",
    )?;
    let delivered_at = decode_optional_non_empty_text(
        raw_row.delivered_at,
        "delivered_at",
        database_path,
        "notification_dispatch_decode_failed",
    )?;
    let claimed_at = decode_optional_non_empty_text(
        raw_row.claimed_at,
        "claimed_at",
        database_path,
        "notification_dispatch_decode_failed",
    )?;
    let last_error_code = decode_optional_non_empty_text(
        raw_row.last_error_code,
        "last_error_code",
        database_path,
        "notification_dispatch_decode_failed",
    )?;
    let last_error_message = decode_optional_non_empty_text(
        raw_row.last_error_message,
        "last_error_message",
        database_path,
        "notification_dispatch_decode_failed",
    )?;

    if matches!(status, NotificationDispatchStatus::Sent) && delivered_at.is_none() {
        return Err(map_snapshot_decode_error(
            "notification_dispatch_decode_failed",
            database_path,
            "Sent notification dispatch rows must include delivered_at.".into(),
        ));
    }

    if matches!(status, NotificationDispatchStatus::Claimed) && claimed_at.is_none() {
        return Err(map_snapshot_decode_error(
            "notification_dispatch_decode_failed",
            database_path,
            "Claimed notification dispatch rows must include claimed_at.".into(),
        ));
    }

    if matches!(status, NotificationDispatchStatus::Failed)
        && (last_error_code.is_none() || last_error_message.is_none())
    {
        return Err(map_snapshot_decode_error(
            "notification_dispatch_decode_failed",
            database_path,
            "Failed notification dispatch rows must include last_error_code and last_error_message."
                .into(),
        ));
    }

    Ok(NotificationDispatchRecord {
        id: raw_row.id,
        project_id: require_non_empty_owned(
            raw_row.project_id,
            "project_id",
            database_path,
            "notification_dispatch_decode_failed",
        )?,
        action_id: require_non_empty_owned(
            raw_row.action_id,
            "action_id",
            database_path,
            "notification_dispatch_decode_failed",
        )?,
        route_id: require_non_empty_owned(
            raw_row.route_id,
            "route_id",
            database_path,
            "notification_dispatch_decode_failed",
        )?,
        correlation_key,
        status,
        attempt_count,
        last_attempt_at,
        delivered_at,
        claimed_at,
        last_error_code,
        last_error_message,
        created_at: require_non_empty_owned(
            raw_row.created_at,
            "created_at",
            database_path,
            "notification_dispatch_decode_failed",
        )?,
        updated_at: require_non_empty_owned(
            raw_row.updated_at,
            "updated_at",
            database_path,
            "notification_dispatch_decode_failed",
        )?,
    })
}

fn decode_notification_reply_claim_row(
    raw_row: RawNotificationReplyClaimRow,
    database_path: &Path,
) -> Result<NotificationReplyClaimRecord, CommandError> {
    let correlation_key = require_non_empty_owned(
        raw_row.correlation_key,
        "correlation_key",
        database_path,
        "notification_reply_decode_failed",
    )?;
    validate_notification_correlation_key(
        &correlation_key,
        "correlation_key",
        "notification_reply_decode_failed",
    )?;

    let reply_text = require_non_empty_owned(
        raw_row.reply_text,
        "reply_text",
        database_path,
        "notification_reply_decode_failed",
    )?;
    if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&reply_text) {
        return Err(map_snapshot_decode_error(
            "notification_reply_decode_failed",
            database_path,
            format!("Field `reply_text` must not include {secret_hint}."),
        ));
    }

    let status = parse_notification_reply_claim_status(&raw_row.status).map_err(|details| {
        map_snapshot_decode_error("notification_reply_decode_failed", database_path, details)
    })?;

    let responder_id = decode_optional_non_empty_text(
        raw_row.responder_id,
        "responder_id",
        database_path,
        "notification_reply_decode_failed",
    )?;
    let rejection_code = decode_optional_non_empty_text(
        raw_row.rejection_code,
        "rejection_code",
        database_path,
        "notification_reply_decode_failed",
    )?;
    let rejection_message = decode_optional_non_empty_text(
        raw_row.rejection_message,
        "rejection_message",
        database_path,
        "notification_reply_decode_failed",
    )?;

    match status {
        NotificationReplyClaimStatus::Accepted => {
            if rejection_code.is_some() || rejection_message.is_some() {
                return Err(map_snapshot_decode_error(
                    "notification_reply_decode_failed",
                    database_path,
                    "Accepted notification reply claims must not include rejection_code or rejection_message."
                        .into(),
                ));
            }
        }
        NotificationReplyClaimStatus::Rejected => {
            if rejection_code.is_none() || rejection_message.is_none() {
                return Err(map_snapshot_decode_error(
                    "notification_reply_decode_failed",
                    database_path,
                    "Rejected notification reply claims must include rejection_code and rejection_message."
                        .into(),
                ));
            }
        }
    }

    Ok(NotificationReplyClaimRecord {
        id: raw_row.id,
        project_id: require_non_empty_owned(
            raw_row.project_id,
            "project_id",
            database_path,
            "notification_reply_decode_failed",
        )?,
        action_id: require_non_empty_owned(
            raw_row.action_id,
            "action_id",
            database_path,
            "notification_reply_decode_failed",
        )?,
        route_id: require_non_empty_owned(
            raw_row.route_id,
            "route_id",
            database_path,
            "notification_reply_decode_failed",
        )?,
        correlation_key,
        responder_id,
        reply_text,
        status,
        rejection_code,
        rejection_message,
        created_at: require_non_empty_owned(
            raw_row.created_at,
            "created_at",
            database_path,
            "notification_reply_decode_failed",
        )?,
    })
}

fn decode_operator_approval_row(
    raw_row: RawOperatorApprovalRow,
    database_path: &Path,
) -> Result<OperatorApprovalDto, CommandError> {
    let action_id = require_non_empty_owned(
        raw_row.action_id,
        "action_id",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let session_id = decode_optional_non_empty_text(
        raw_row.session_id,
        "session_id",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let flow_id = decode_optional_non_empty_text(
        raw_row.flow_id,
        "flow_id",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let action_type = require_non_empty_owned(
        raw_row.action_type,
        "action_type",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let title = require_non_empty_owned(
        raw_row.title,
        "title",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let detail = require_non_empty_owned(
        raw_row.detail,
        "detail",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let gate_node_id = decode_optional_non_empty_text(
        raw_row.gate_node_id,
        "gate_node_id",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let gate_key = decode_optional_non_empty_text(
        raw_row.gate_key,
        "gate_key",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let transition_from_node_id = decode_optional_non_empty_text(
        raw_row.transition_from_node_id,
        "transition_from_node_id",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let transition_to_node_id = decode_optional_non_empty_text(
        raw_row.transition_to_node_id,
        "transition_to_node_id",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let transition_kind = decode_optional_non_empty_text(
        raw_row.transition_kind,
        "transition_kind",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let user_answer = decode_optional_non_empty_text(
        raw_row.user_answer,
        "user_answer",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let created_at = require_non_empty_owned(
        raw_row.created_at,
        "created_at",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let updated_at = require_non_empty_owned(
        raw_row.updated_at,
        "updated_at",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let decision_note = decode_optional_non_empty_text(
        raw_row.decision_note,
        "decision_note",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let resolved_at = decode_optional_non_empty_text(
        raw_row.resolved_at,
        "resolved_at",
        database_path,
        "operator_approval_decode_failed",
    )?;

    let status = parse_operator_approval_status(&raw_row.status).map_err(|details| {
        map_snapshot_decode_error("operator_approval_decode_failed", database_path, details)
    })?;

    let gate_fields_populated = gate_node_id.is_some() || gate_key.is_some();
    if gate_fields_populated && (gate_node_id.is_none() || gate_key.is_none()) {
        return Err(map_snapshot_decode_error(
            "operator_approval_decode_failed",
            database_path,
            "Gate-linked approval rows must include both `gate_node_id` and `gate_key`.".into(),
        ));
    }

    let continuation_fields_populated = transition_from_node_id.is_some()
        || transition_to_node_id.is_some()
        || transition_kind.is_some();
    if continuation_fields_populated
        && (transition_from_node_id.is_none()
            || transition_to_node_id.is_none()
            || transition_kind.is_none())
    {
        return Err(map_snapshot_decode_error(
            "operator_approval_decode_failed",
            database_path,
            "Gate-linked approval rows must include full transition continuation metadata (`transition_from_node_id`, `transition_to_node_id`, `transition_kind`).".into(),
        ));
    }

    if gate_fields_populated && !continuation_fields_populated {
        return Err(map_snapshot_decode_error(
            "operator_approval_decode_failed",
            database_path,
            "Gate-linked approval rows must include continuation metadata for deterministic resume.".into(),
        ));
    }

    if continuation_fields_populated && !gate_fields_populated {
        return Err(map_snapshot_decode_error(
            "operator_approval_decode_failed",
            database_path,
            "Transition continuation metadata requires matching gate identity fields.".into(),
        ));
    }

    if let (Some(gate_node_id), Some(transition_to_node_id)) =
        (gate_node_id.as_deref(), transition_to_node_id.as_deref())
    {
        if gate_node_id != transition_to_node_id {
            return Err(map_snapshot_decode_error(
                "operator_approval_decode_failed",
                database_path,
                "Gate-linked approval rows must target the same `transition_to_node_id` as `gate_node_id`.".into(),
            ));
        }
    }

    match status {
        OperatorApprovalStatus::Pending => {
            if decision_note.is_some() || resolved_at.is_some() || user_answer.is_some() {
                return Err(map_snapshot_decode_error(
                    "operator_approval_decode_failed",
                    database_path,
                    "Pending approval rows must not include decision_note, user_answer, or resolved_at."
                        .into(),
                ));
            }
        }
        OperatorApprovalStatus::Approved | OperatorApprovalStatus::Rejected => {
            if resolved_at.is_none() {
                return Err(map_snapshot_decode_error(
                    "operator_approval_decode_failed",
                    database_path,
                    "Resolved approval rows must include resolved_at.".into(),
                ));
            }
        }
    }

    Ok(OperatorApprovalDto {
        action_id,
        session_id,
        flow_id,
        action_type,
        title,
        detail,
        gate_node_id,
        gate_key,
        transition_from_node_id,
        transition_to_node_id,
        transition_kind,
        user_answer,
        status,
        decision_note,
        created_at,
        updated_at,
        resolved_at,
    })
}

fn decode_verification_record_row(
    raw_row: RawVerificationRecordRow,
    database_path: &Path,
) -> Result<VerificationRecordDto, CommandError> {
    let id = decode_snapshot_row_id(
        raw_row.id,
        "id",
        database_path,
        "verification_record_decode_failed",
    )?;
    let source_action_id = decode_optional_non_empty_text(
        raw_row.source_action_id,
        "source_action_id",
        database_path,
        "verification_record_decode_failed",
    )?;
    let status = parse_verification_record_status(&raw_row.status).map_err(|details| {
        map_snapshot_decode_error("verification_record_decode_failed", database_path, details)
    })?;
    let summary = require_non_empty_owned(
        raw_row.summary,
        "summary",
        database_path,
        "verification_record_decode_failed",
    )?;
    let detail = decode_optional_non_empty_text(
        raw_row.detail,
        "detail",
        database_path,
        "verification_record_decode_failed",
    )?;
    let recorded_at = require_non_empty_owned(
        raw_row.recorded_at,
        "recorded_at",
        database_path,
        "verification_record_decode_failed",
    )?;

    Ok(VerificationRecordDto {
        id,
        source_action_id,
        status,
        summary,
        detail,
        recorded_at,
    })
}

fn decode_resume_history_row(
    raw_row: RawResumeHistoryRow,
    database_path: &Path,
) -> Result<ResumeHistoryEntryDto, CommandError> {
    let id = decode_snapshot_row_id(
        raw_row.id,
        "id",
        database_path,
        "resume_history_decode_failed",
    )?;
    let source_action_id = decode_optional_non_empty_text(
        raw_row.source_action_id,
        "source_action_id",
        database_path,
        "resume_history_decode_failed",
    )?;
    let session_id = decode_optional_non_empty_text(
        raw_row.session_id,
        "session_id",
        database_path,
        "resume_history_decode_failed",
    )?;
    let status = parse_resume_history_status(&raw_row.status).map_err(|details| {
        map_snapshot_decode_error("resume_history_decode_failed", database_path, details)
    })?;
    let summary = require_non_empty_owned(
        raw_row.summary,
        "summary",
        database_path,
        "resume_history_decode_failed",
    )?;
    let created_at = require_non_empty_owned(
        raw_row.created_at,
        "created_at",
        database_path,
        "resume_history_decode_failed",
    )?;

    Ok(ResumeHistoryEntryDto {
        id,
        source_action_id,
        session_id,
        status,
        summary,
        created_at,
    })
}

struct ValidatedNotificationRouteUpsertPayload {
    route_kind: NotificationRouteKind,
    canonical_route_target: String,
}

fn validate_notification_route_upsert_payload(
    route: &NotificationRouteUpsertRecord,
) -> Result<ValidatedNotificationRouteUpsertPayload, CommandError> {
    validate_non_empty_text(
        &route.project_id,
        "project_id",
        "notification_route_request_invalid",
    )?;
    validate_non_empty_text(
        &route.route_id,
        "route_id",
        "notification_route_request_invalid",
    )?;
    validate_non_empty_text(
        &route.route_kind,
        "route_kind",
        "notification_route_request_invalid",
    )?;
    validate_non_empty_text(
        &route.route_target,
        "route_target",
        "notification_route_request_invalid",
    )?;
    validate_non_empty_text(
        &route.updated_at,
        "updated_at",
        "notification_route_request_invalid",
    )?;

    let route_kind = NotificationRouteKind::parse(&route.route_kind).map_err(|error| {
        CommandError::user_fixable("notification_route_request_invalid", error.message)
    })?;

    let canonical_route_target =
        parse_notification_route_target_for_kind(route_kind, &route.route_target)
            .map(|target| target.canonical())
            .map_err(|error| {
                CommandError::user_fixable("notification_route_request_invalid", error.message)
            })?;

    if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&canonical_route_target)
    {
        return Err(CommandError::user_fixable(
            "notification_route_request_invalid",
            format!(
                "Notification route targets must not include {secret_hint}. Persist non-secret route identifiers only."
            ),
        ));
    }

    if let Some(metadata_json) = route.metadata_json.as_deref() {
        let _ = normalize_optional_notification_metadata_json(
            Some(metadata_json),
            "notification_route_request_invalid",
        )?;
    }

    Ok(ValidatedNotificationRouteUpsertPayload {
        route_kind,
        canonical_route_target,
    })
}

fn normalize_optional_notification_metadata_json(
    metadata_json: Option<&str>,
    code: &str,
) -> Result<Option<String>, CommandError> {
    let Some(metadata_json) = metadata_json
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    if let Some(secret_hint) = find_prohibited_runtime_persistence_content(metadata_json) {
        return Err(CommandError::user_fixable(
            code,
            format!(
                "Notification route metadata must not include {secret_hint}. Persist redacted, non-secret metadata only."
            ),
        ));
    }

    let value: serde_json::Value = serde_json::from_str(metadata_json).map_err(|error| {
        CommandError::user_fixable(
            code,
            format!("Field `metadata_json` must be valid JSON text: {error}"),
        )
    })?;

    if !value.is_object() {
        return Err(CommandError::user_fixable(
            code,
            "Field `metadata_json` must be a JSON object.",
        ));
    }

    serde_json::to_string(&value).map(Some).map_err(|error| {
        CommandError::system_fault(
            code,
            format!("Cadence could not canonicalize notification route metadata JSON: {error}"),
        )
    })
}

fn validate_notification_dispatch_enqueue_payload(
    enqueue: &NotificationDispatchEnqueueRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &enqueue.project_id,
        "project_id",
        "notification_dispatch_request_invalid",
    )?;
    validate_non_empty_text(
        &enqueue.action_id,
        "action_id",
        "notification_dispatch_request_invalid",
    )?;
    validate_non_empty_text(
        &enqueue.enqueued_at,
        "enqueued_at",
        "notification_dispatch_request_invalid",
    )?;

    Ok(())
}

fn validate_notification_dispatch_outcome_update_payload(
    outcome: &NotificationDispatchOutcomeUpdateRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &outcome.project_id,
        "project_id",
        "notification_dispatch_request_invalid",
    )?;
    validate_non_empty_text(
        &outcome.action_id,
        "action_id",
        "notification_dispatch_request_invalid",
    )?;
    validate_non_empty_text(
        &outcome.route_id,
        "route_id",
        "notification_dispatch_request_invalid",
    )?;
    validate_non_empty_text(
        &outcome.attempted_at,
        "attempted_at",
        "notification_dispatch_request_invalid",
    )?;

    match outcome.status {
        NotificationDispatchStatus::Sent => {
            if outcome.error_code.is_some() || outcome.error_message.is_some() {
                return Err(CommandError::user_fixable(
                    "notification_dispatch_request_invalid",
                    "Sent notification-dispatch outcomes must not include error_code or error_message.",
                ));
            }
        }
        NotificationDispatchStatus::Failed => {
            let error_code = outcome
                .error_code
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "notification_dispatch_request_invalid",
                        "Failed notification-dispatch outcomes must include non-empty error_code.",
                    )
                })?;
            let error_message = outcome
                .error_message
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "notification_dispatch_request_invalid",
                        "Failed notification-dispatch outcomes must include non-empty error_message.",
                    )
                })?;

            if let Some(secret_hint) = find_prohibited_runtime_persistence_content(error_message) {
                return Err(CommandError::user_fixable(
                    "notification_dispatch_request_invalid",
                    format!(
                        "Notification-dispatch failure diagnostics must not include {secret_hint}."
                    ),
                ));
            }

            let _ = error_code;
        }
        NotificationDispatchStatus::Pending | NotificationDispatchStatus::Claimed => {
            return Err(CommandError::user_fixable(
                "notification_dispatch_request_invalid",
                "Dispatch outcomes must use `sent` or `failed` status values.",
            ));
        }
    }

    Ok(())
}

fn validate_notification_reply_claim_request_payload(
    request: &NotificationReplyClaimRequestRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &request.project_id,
        "project_id",
        "notification_reply_request_invalid",
    )?;
    validate_non_empty_text(
        &request.action_id,
        "action_id",
        "notification_reply_request_invalid",
    )?;
    validate_non_empty_text(
        &request.route_id,
        "route_id",
        "notification_reply_request_invalid",
    )?;
    validate_non_empty_text(
        &request.correlation_key,
        "correlation_key",
        "notification_reply_request_invalid",
    )?;
    validate_non_empty_text(
        &request.reply_text,
        "reply_text",
        "notification_reply_request_invalid",
    )?;
    validate_non_empty_text(
        &request.received_at,
        "received_at",
        "notification_reply_request_invalid",
    )?;
    if let Some(responder_id) = request.responder_id.as_deref() {
        validate_non_empty_text(
            responder_id,
            "responder_id",
            "notification_reply_request_invalid",
        )?;
    }

    validate_notification_correlation_key(
        &request.correlation_key,
        "correlation_key",
        "notification_reply_request_invalid",
    )?;

    if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&request.reply_text) {
        return Err(CommandError::user_fixable(
            "notification_reply_request_invalid",
            format!(
                "Notification reply payloads must not include {secret_hint}. Remove secret-bearing material before retrying."
            ),
        ));
    }

    Ok(())
}

fn derive_notification_correlation_key(
    project_id: &str,
    action_id: &str,
    route_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project_id.trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(action_id.trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(route_id.trim().as_bytes());

    let digest = hasher.finalize();
    let digest_hex: String = digest
        .iter()
        .take(NOTIFICATION_CORRELATION_KEY_HEX_LEN / 2)
        .map(|byte| format!("{byte:02x}"))
        .collect();

    format!("{NOTIFICATION_CORRELATION_KEY_PREFIX}:{digest_hex}")
}

fn validate_notification_correlation_key(
    value: &str,
    field: &str,
    code: &str,
) -> Result<(), CommandError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CommandError::user_fixable(
            code,
            format!("Field `{field}` must be a non-empty string."),
        ));
    }

    let prefix = format!("{NOTIFICATION_CORRELATION_KEY_PREFIX}:");
    let Some(suffix) = value.strip_prefix(prefix.as_str()) else {
        return Err(CommandError::user_fixable(
            code,
            format!("Field `{field}` must start with `{NOTIFICATION_CORRELATION_KEY_PREFIX}:`."),
        ));
    };

    if suffix.len() != NOTIFICATION_CORRELATION_KEY_HEX_LEN
        || !suffix
            .chars()
            .all(|character| character.is_ascii_hexdigit())
        || suffix
            .chars()
            .any(|character| character.is_ascii_uppercase())
    {
        return Err(CommandError::user_fixable(
            code,
            format!(
                "Field `{field}` must use `{NOTIFICATION_CORRELATION_KEY_PREFIX}:` plus {} lowercase hexadecimal characters.",
                NOTIFICATION_CORRELATION_KEY_HEX_LEN
            ),
        ));
    }

    Ok(())
}

fn parse_operator_approval_status(value: &str) -> Result<OperatorApprovalStatus, String> {
    match value {
        "pending" => Ok(OperatorApprovalStatus::Pending),
        "approved" => Ok(OperatorApprovalStatus::Approved),
        "rejected" => Ok(OperatorApprovalStatus::Rejected),
        other => Err(format!(
            "Field `status` must be a known approval status, found `{other}`."
        )),
    }
}

fn parse_verification_record_status(value: &str) -> Result<VerificationRecordStatus, String> {
    match value {
        "pending" => Ok(VerificationRecordStatus::Pending),
        "passed" => Ok(VerificationRecordStatus::Passed),
        "failed" => Ok(VerificationRecordStatus::Failed),
        other => Err(format!(
            "Field `status` must be a known verification status, found `{other}`."
        )),
    }
}

fn normalize_runtime_resume_history_summary(summary: &str, fallback: &str) -> String {
    let candidate = if summary.trim().is_empty() {
        fallback.trim()
    } else {
        summary.trim()
    };

    if find_prohibited_runtime_persistence_content(candidate).is_some() {
        return normalize_runtime_checkpoint_summary(fallback);
    }

    normalize_runtime_checkpoint_summary(candidate)
}

fn resume_history_status_sql_value(value: &ResumeHistoryStatus) -> &'static str {
    match value {
        ResumeHistoryStatus::Started => "started",
        ResumeHistoryStatus::Failed => "failed",
    }
}

fn parse_resume_history_status(value: &str) -> Result<ResumeHistoryStatus, String> {
    match value {
        "started" => Ok(ResumeHistoryStatus::Started),
        "failed" => Ok(ResumeHistoryStatus::Failed),
        other => Err(format!(
            "Field `status` must be a known resume-history status, found `{other}`."
        )),
    }
}

fn validate_operator_approval_gate_link_input(
    gate_link: &OperatorApprovalGateLinkInput,
    action_type: &str,
) -> Result<OperatorApprovalGateLink, CommandError> {
    let gate_node_id = normalize_operator_gate_link_field(
        gate_link.gate_node_id.as_str(),
        "gateNodeId",
        action_type,
    )?;
    let gate_key =
        normalize_operator_gate_link_field(gate_link.gate_key.as_str(), "gateKey", action_type)?;
    let transition_from_node_id = normalize_operator_gate_link_field(
        gate_link.transition_from_node_id.as_str(),
        "transitionFromNodeId",
        action_type,
    )?;
    let transition_to_node_id = normalize_operator_gate_link_field(
        gate_link.transition_to_node_id.as_str(),
        "transitionToNodeId",
        action_type,
    )?;
    let transition_kind = normalize_operator_gate_link_field(
        gate_link.transition_kind.as_str(),
        "transitionKind",
        action_type,
    )?;

    if gate_node_id != transition_to_node_id {
        return Err(CommandError::system_fault(
            "runtime_action_request_invalid",
            format!(
                "Cadence could not persist gate-linked runtime action `{action_type}` because gate node `{gate_node_id}` does not match transition target `{transition_to_node_id}`."
            ),
        ));
    }

    Ok(OperatorApprovalGateLink {
        gate_node_id,
        gate_key,
        transition_from_node_id,
        transition_to_node_id,
        transition_kind,
    })
}

fn normalize_operator_gate_link_field(
    value: &str,
    field: &str,
    action_type: &str,
) -> Result<String, CommandError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CommandError::system_fault(
            "runtime_action_request_invalid",
            format!(
                "Cadence could not persist gate-linked runtime action `{action_type}` because `{field}` was empty."
            ),
        ));
    }

    Ok(value.to_string())
}

fn decode_runtime_operator_resume_target(
    approval_request: &OperatorApprovalDto,
) -> Result<Option<RuntimeOperatorResumeTarget>, CommandError> {
    let action_id = approval_request.action_id.as_str();
    if !action_id.contains(":run:") || !action_id.contains(":boundary:") {
        return Ok(None);
    }

    let session_id = approval_request.session_id.as_deref().ok_or_else(|| {
        CommandError::retryable(
            "operator_resume_runtime_action_invalid",
            format!(
                "Cadence cannot resume runtime-scoped operator request `{action_id}` because the durable approval is missing its session identity."
            ),
        )
    })?;
    let scope = derive_operator_scope_prefix(session_id, approval_request.flow_id.as_deref())?;
    let prefix = format!("{scope}:run:");
    if !action_id.starts_with(&prefix) {
        return Err(CommandError::retryable(
            "operator_resume_runtime_action_invalid",
            format!(
                "Cadence cannot resume runtime-scoped operator request `{action_id}` because its action scope does not match the durable session identity."
            ),
        ));
    }

    let remainder = &action_id[prefix.len()..];
    let (run_id_raw, boundary_and_action) = remainder.split_once(":boundary:").ok_or_else(|| {
        CommandError::retryable(
            "operator_resume_runtime_action_invalid",
            format!(
                "Cadence cannot resume runtime-scoped operator request `{action_id}` because its durable action id is malformed."
            ),
        )
    })?;

    let run_id = run_id_raw.trim();
    validate_runtime_resume_identity_component(run_id, "run_id", action_id)?;

    let action_type = approval_request.action_type.trim();
    validate_non_empty_text(
        action_type,
        "action_type",
        "operator_resume_runtime_action_invalid",
    )?;
    if action_type.contains(':') || action_type.chars().any(char::is_whitespace) {
        return Err(CommandError::retryable(
            "operator_resume_runtime_action_invalid",
            format!(
                "Cadence cannot resume runtime-scoped operator request `{action_id}` because `action_type` contains unsupported delimiters or whitespace."
            ),
        ));
    }

    let action_suffix = format!(":{action_type}");
    let boundary_id_raw = boundary_and_action.strip_suffix(&action_suffix).ok_or_else(|| {
        CommandError::retryable(
            "operator_resume_runtime_action_invalid",
            format!(
                "Cadence cannot resume runtime-scoped operator request `{action_id}` because its durable action type does not match the stored action id."
            ),
        )
    })?;
    let boundary_id = boundary_id_raw.trim();
    validate_runtime_resume_identity_component(boundary_id, "boundary_id", action_id)?;

    let canonical_action_id = derive_runtime_action_id(
        session_id,
        approval_request.flow_id.as_deref(),
        run_id,
        boundary_id,
        action_type,
    )?;
    if canonical_action_id != action_id {
        return Err(CommandError::retryable(
            "operator_resume_runtime_action_invalid",
            format!(
                "Cadence cannot resume runtime-scoped operator request `{action_id}` because its durable action identity is not canonical."
            ),
        ));
    }

    Ok(Some(RuntimeOperatorResumeTarget {
        run_id: run_id.to_string(),
        boundary_id: boundary_id.to_string(),
    }))
}

fn validate_runtime_resume_identity_component(
    value: &str,
    field: &str,
    action_id: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(value, field, "operator_resume_runtime_action_invalid")?;

    if value.contains(':') || value.chars().any(char::is_whitespace) {
        return Err(CommandError::retryable(
            "operator_resume_runtime_action_invalid",
            format!(
                "Cadence cannot resume runtime-scoped operator request `{action_id}` because `{field}` contains unsupported delimiters or whitespace."
            ),
        ));
    }

    Ok(())
}

fn derive_operator_scope_prefix(
    session_id: &str,
    flow_id: Option<&str>,
) -> Result<String, CommandError> {
    flow_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("flow:{value}"))
        .or_else(|| {
            let session_id = session_id.trim();
            (!session_id.is_empty()).then(|| format!("session:{session_id}"))
        })
        .ok_or_else(|| {
            CommandError::system_fault(
                "runtime_action_request_invalid",
                "Cadence could not persist the runtime approval because the action-required item was missing both flow and session identifiers.",
            )
        })
}

fn derive_operator_action_id(
    session_id: &str,
    flow_id: Option<&str>,
    action_type: &str,
    gate_link: Option<&OperatorApprovalGateLink>,
) -> Result<String, CommandError> {
    let action_type = action_type.trim();
    if action_type.is_empty() {
        return Err(CommandError::system_fault(
            "runtime_action_request_invalid",
            "Cadence could not persist the runtime approval because the action-required item was missing a stable action type.",
        ));
    }

    let stable_scope = derive_operator_scope_prefix(session_id, flow_id)?;

    if let Some(gate_link) = gate_link {
        return Ok(format!(
            "{stable_scope}:gate:{}:{}:{action_type}",
            gate_link.gate_node_id, gate_link.gate_key
        ));
    }

    Ok(format!("{stable_scope}:{action_type}"))
}

fn derive_runtime_action_id(
    session_id: &str,
    flow_id: Option<&str>,
    run_id: &str,
    boundary_id: &str,
    action_type: &str,
) -> Result<String, CommandError> {
    validate_non_empty_text(run_id, "run_id", "runtime_action_request_invalid")?;
    validate_non_empty_text(boundary_id, "boundary_id", "runtime_action_request_invalid")?;

    let stable_scope = derive_operator_scope_prefix(session_id, flow_id)?;
    let action_type = action_type.trim();
    if action_type.is_empty() {
        return Err(CommandError::system_fault(
            "runtime_action_request_invalid",
            "Cadence could not persist the runtime approval because the action-required item was missing a stable action type.",
        ));
    }

    Ok(format!(
        "{stable_scope}:run:{}:boundary:{}:{action_type}",
        run_id.trim(),
        boundary_id.trim()
    ))
}

fn operator_approval_status_label(status: &OperatorApprovalStatus) -> &'static str {
    match status {
        OperatorApprovalStatus::Pending => "pending",
        OperatorApprovalStatus::Approved => "approved",
        OperatorApprovalStatus::Rejected => "rejected",
    }
}

fn map_operator_loop_transaction_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_operator_loop_write_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_operator_loop_commit_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn sqlite_path_suffix(database_path: &Path) -> String {
    format!("against {}.", database_path.display())
}

fn is_unique_constraint_violation(error: &SqlError) -> bool {
    match error {
        SqlError::SqliteFailure(inner, _) => {
            matches!(inner.code, ErrorCode::ConstraintViolation)
        }
        _ => false,
    }
}

fn is_retryable_sql_error(error: &SqlError) -> bool {
    match error {
        SqlError::SqliteFailure(inner, _) => {
            matches!(
                inner.code,
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked
            )
        }
        _ => false,
    }
}

fn decode_snapshot_row_id(
    value: i64,
    field: &str,
    database_path: &Path,
    code: &str,
) -> Result<u32, CommandError> {
    u32::try_from(value).map_err(|_| {
        map_snapshot_decode_error(
            code,
            database_path,
            format!("Field `{field}` must be a non-negative 32-bit integer, found {value}."),
        )
    })
}

fn require_non_empty_owned(
    value: String,
    field: &str,
    database_path: &Path,
    code: &str,
) -> Result<String, CommandError> {
    if value.trim().is_empty() {
        Err(map_snapshot_decode_error(
            code,
            database_path,
            format!("Field `{field}` must be a non-empty string."),
        ))
    } else {
        Ok(value)
    }
}

fn decode_optional_non_empty_text(
    value: Option<String>,
    field: &str,
    database_path: &Path,
    code: &str,
) -> Result<Option<String>, CommandError> {
    match value {
        Some(value) if value.trim().is_empty() => Err(map_snapshot_decode_error(
            code,
            database_path,
            format!("Field `{field}` must be null or a non-empty string."),
        )),
        other => Ok(other),
    }
}

fn parse_notification_dispatch_status(value: &str) -> Result<NotificationDispatchStatus, String> {
    match value {
        "pending" => Ok(NotificationDispatchStatus::Pending),
        "sent" => Ok(NotificationDispatchStatus::Sent),
        "failed" => Ok(NotificationDispatchStatus::Failed),
        "claimed" => Ok(NotificationDispatchStatus::Claimed),
        other => Err(format!(
            "Field `status` must be a known notification-dispatch status, found `{other}`."
        )),
    }
}

fn notification_dispatch_status_sql_value(value: &NotificationDispatchStatus) -> &'static str {
    match value {
        NotificationDispatchStatus::Pending => "pending",
        NotificationDispatchStatus::Sent => "sent",
        NotificationDispatchStatus::Failed => "failed",
        NotificationDispatchStatus::Claimed => "claimed",
    }
}

fn parse_notification_reply_claim_status(
    value: &str,
) -> Result<NotificationReplyClaimStatus, String> {
    match value {
        "accepted" => Ok(NotificationReplyClaimStatus::Accepted),
        "rejected" => Ok(NotificationReplyClaimStatus::Rejected),
        other => Err(format!(
            "Field `status` must be a known notification-reply claim status, found `{other}`."
        )),
    }
}

fn parse_phase_status(value: &str) -> Result<PhaseStatus, String> {
    match value {
        "complete" => Ok(PhaseStatus::Complete),
        "active" => Ok(PhaseStatus::Active),
        "pending" => Ok(PhaseStatus::Pending),
        "blocked" => Ok(PhaseStatus::Blocked),
        other => Err(format!("Unknown phase status `{other}`.")),
    }
}

fn parse_phase_step(value: &str) -> Result<PhaseStep, String> {
    match value {
        "discuss" => Ok(PhaseStep::Discuss),
        "plan" => Ok(PhaseStep::Plan),
        "execute" => Ok(PhaseStep::Execute),
        "verify" => Ok(PhaseStep::Verify),
        "ship" => Ok(PhaseStep::Ship),
        other => Err(format!("Unknown phase current_step `{other}`.")),
    }
}

fn map_snapshot_decode_error(code: &str, database_path: &Path, details: String) -> CommandError {
    CommandError::system_fault(
        code,
        format!(
            "Cadence could not decode selected-project operator-loop metadata from {}: {details}",
            database_path.display()
        ),
    )
}

fn map_project_query_error(
    error: SqlError,
    database_path: &Path,
    repo_root: &Path,
    expected_project_id: &str,
) -> CommandError {
    match error {
        SqlError::QueryReturnedNoRows => CommandError::system_fault(
            "project_registry_mismatch",
            format!(
                "Registry entry for {} expected project `{expected_project_id}`, but {} did not contain that project row.",
                repo_root.display(),
                database_path.display()
            ),
        ),
        other => CommandError::system_fault(
            "project_summary_query_failed",
            format!(
                "Cadence could not read the project summary from {}: {other}",
                database_path.display()
            ),
        ),
    }
}
