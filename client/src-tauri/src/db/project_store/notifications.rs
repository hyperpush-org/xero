use std::path::Path;

use rusqlite::{params, Connection, Transaction};
use sha2::{Digest, Sha256};

use crate::{
    commands::{CommandError, OperatorApprovalStatus},
    db::database_path_for_repo,
    notifications::{
        route_target::parse_notification_route_target_for_kind, NotificationRouteKind,
    },
};

use super::{
    decode_optional_non_empty_text, find_prohibited_runtime_persistence_content,
    is_unique_constraint_violation, map_operator_loop_commit_error,
    map_operator_loop_transaction_error, map_operator_loop_write_error, map_snapshot_decode_error,
    open_project_database, operator_approval_status_label, read_operator_approval_by_action_id,
    read_project_row, require_non_empty_owned, validate_non_empty_text,
    NotificationDispatchEnqueueRecord, NotificationDispatchOutcomeUpdateRecord,
    NotificationDispatchRecord, NotificationDispatchStatus, NotificationReplyClaimRecord,
    NotificationReplyClaimRequestRecord, NotificationReplyClaimResultRecord,
    NotificationReplyClaimStatus, NotificationRouteRecord, NotificationRouteUpsertRecord,
};

const MAX_NOTIFICATION_ROUTE_ROWS: i64 = 128;
const MAX_NOTIFICATION_DISPATCH_ROWS: i64 = 256;
const MAX_NOTIFICATION_PENDING_DISPATCH_BATCH_ROWS: i64 = 64;
const MAX_NOTIFICATION_REPLY_CLAIM_ROWS: i64 = 512;
const NOTIFICATION_CORRELATION_KEY_PREFIX: &str = "nfy";
const NOTIFICATION_CORRELATION_KEY_HEX_LEN: usize = 32;

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
            "Xero could not start the notification-route transaction.",
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
                "Xero could not persist notification-route metadata.",
            )
        })?;

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "notification_route_commit_failed",
            &database_path,
            error,
            "Xero could not commit the notification-route transaction.",
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
                "Xero persisted notification route `{}` in {} but could not read it back.",
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
            "Xero could not start the notification-dispatch enqueue transaction.",
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
                "Xero could not enqueue notification dispatches because operator action `{}` was not found for project `{}`.",
                enqueue.action_id, enqueue.project_id
            ),
        )
    })?;

    if approval.status != OperatorApprovalStatus::Pending {
        return Err(CommandError::user_fixable(
            "notification_dispatch_action_not_pending",
            format!(
                "Xero can only enqueue notification dispatches for pending operator actions. Action `{}` is currently {}.",
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
                    "Xero could not persist notification dispatch fan-out rows.",
                )
            })?;
    }

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "notification_dispatch_commit_failed",
            database_path,
            error,
            "Xero could not commit notification dispatch fan-out rows.",
        )
    })?;

    read_notification_dispatches(
        connection,
        database_path,
        &enqueue.project_id,
        Some(&enqueue.action_id),
    )
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
            "Xero could not start the notification-dispatch outcome transaction.",
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
                "Xero could not record dispatch outcome because `{}`/`{}`/`{}` was not found.",
                outcome.project_id, outcome.action_id, outcome.route_id
            ),
        )
    })?;

    if existing.status == NotificationDispatchStatus::Claimed {
        return Err(CommandError::user_fixable(
            "notification_dispatch_already_claimed",
            format!(
                "Xero refused to overwrite dispatch outcome for route `{}` because action `{}` has already been claimed for reply correlation.",
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
                "Xero could not persist notification dispatch outcome metadata.",
            )
        })?;

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "notification_dispatch_commit_failed",
            &database_path,
            error,
            "Xero could not commit notification dispatch outcome metadata.",
        )
    })?;

    read_notification_dispatch_by_id(&connection, &database_path, existing.id)?.ok_or_else(|| {
        CommandError::system_fault(
            "notification_dispatch_missing_after_persist",
            format!(
                "Xero persisted notification dispatch outcome in {} but could not read row {} back.",
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
            "Xero could not start the notification-reply claim transaction.",
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
            "Xero rejected the notification reply because action `{}` is not pending for project `{}`.",
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
                "Xero could not commit the rejected notification reply claim.",
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
            "Xero rejected the notification reply because action `{}` is already {}.",
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
                "Xero could not commit the rejected notification reply claim.",
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
            "Xero rejected the notification reply because route `{}` is not linked to action `{}` for project `{}`.",
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
                "Xero could not commit the rejected notification reply claim.",
            )
        })?;

        return Err(CommandError::user_fixable(
            "notification_reply_correlation_invalid",
            message,
        ));
    };

    if dispatch.correlation_key != request.correlation_key {
        let message = format!(
            "Xero rejected the notification reply because correlation key `{}` does not match route `{}` for action `{}`.",
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
                "Xero could not commit the rejected notification reply claim.",
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
            "Xero rejected the notification reply because action `{}` was already claimed by route `{}` at {}.",
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
                "Xero could not commit the rejected notification reply claim.",
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
                "Xero rejected the notification reply because action `{}` was already claimed by another route.",
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
                    "Xero could not commit the rejected notification reply claim.",
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
            "Xero could not persist the accepted notification reply claim.",
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
                "Xero could not persist notification-dispatch claim metadata.",
            )
        })?;

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "notification_reply_commit_failed",
            &database_path,
            error,
            "Xero could not commit the notification reply claim transaction.",
        )
    })?;

    let claim = read_notification_reply_claim_by_id(&connection, &database_path, accepted_claim_id)?
        .ok_or_else(|| {
            CommandError::system_fault(
                "notification_reply_missing_after_persist",
                format!(
                    "Xero persisted accepted notification reply claim `{accepted_claim_id}` in {} but could not read it back.",
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
                        "Xero persisted notification dispatch claim metadata in {} but could not read row {} back.",
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
                    "Xero could not prepare notification-route rows from {}: {error}",
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
                    "Xero could not query notification-route rows from {}: {error}",
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
                            "Xero could not decode notification-route rows from {}: {error}",
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
                    "Xero could not prepare notification-route lookup from {}: {error}",
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
                    "Xero could not query notification-route lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_route_query_failed",
            format!(
                "Xero could not read notification-route lookup rows from {}: {error}",
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
                        "Xero could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Xero could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_kind: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Xero could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_target: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Xero could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            enabled: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Xero could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            metadata_json: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Xero could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Xero could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            updated_at: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Xero could not decode notification-route lookup rows from {}: {error}",
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
                        "Xero could not prepare notification-dispatch rows from {}: {error}",
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
                        "Xero could not prepare notification-dispatch rows from {}: {error}",
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
                        "Xero could not query notification-dispatch rows from {}: {error}",
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
                                "Xero could not decode notification-dispatch rows from {}: {error}",
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
                        "Xero could not query notification-dispatch rows from {}: {error}",
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
                                "Xero could not decode notification-dispatch rows from {}: {error}",
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
                    "Xero could not prepare pending notification-dispatch query from {}: {error}",
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
                    "Xero could not query pending notification-dispatch rows from {}: {error}",
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
                            "Xero could not decode pending notification-dispatch rows from {}: {error}",
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
                    "Xero could not prepare notification-dispatch lookup from {}: {error}",
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
                    "Xero could not query notification-dispatch lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_dispatch_query_failed",
            format!(
                "Xero could not read notification-dispatch lookup rows from {}: {error}",
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
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            correlation_key: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            attempt_count: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_attempt_at: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            delivered_at: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            claimed_at: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_error_code: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_error_message: row.get(11).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(12).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            updated_at: row.get(13).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch lookup rows from {}: {error}",
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
                    "Xero could not prepare notification-dispatch id lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement.query(params![id]).map_err(|error| {
        CommandError::system_fault(
            "notification_dispatch_query_failed",
            format!(
                "Xero could not query notification-dispatch id lookup from {}: {error}",
                database_path.display()
            ),
        )
    })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_dispatch_query_failed",
            format!(
                "Xero could not read notification-dispatch id lookup rows from {}: {error}",
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
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            correlation_key: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            attempt_count: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_attempt_at: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            delivered_at: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            claimed_at: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_error_code: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_error_message: row.get(11).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(12).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            updated_at: row.get(13).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Xero could not decode notification-dispatch id lookup rows from {}: {error}",
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
                        "Xero could not prepare notification-reply claim rows from {}: {error}",
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
                        "Xero could not prepare notification-reply claim rows from {}: {error}",
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
                        "Xero could not query notification-reply claim rows from {}: {error}",
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
                                "Xero could not decode notification-reply claim rows from {}: {error}",
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
                        "Xero could not query notification-reply claim rows from {}: {error}",
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
                                "Xero could not decode notification-reply claim rows from {}: {error}",
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
                    "Xero could not prepare notification-reply claim id lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement.query(params![id]).map_err(|error| {
        CommandError::system_fault(
            "notification_reply_query_failed",
            format!(
                "Xero could not query notification-reply claim id lookup from {}: {error}",
                database_path.display()
            ),
        )
    })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_reply_query_failed",
            format!(
                "Xero could not read notification-reply claim id lookup rows from {}: {error}",
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
                        "Xero could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            correlation_key: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            responder_id: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            reply_text: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            rejection_code: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            rejection_message: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode notification-reply claim id lookup rows from {}: {error}",
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
                    "Xero could not prepare winning notification-reply lookup from {}: {error}",
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
                    "Xero could not query winning notification-reply lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_reply_query_failed",
            format!(
                "Xero could not read winning notification-reply lookup rows from {}: {error}",
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
                        "Xero could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            correlation_key: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            responder_id: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            reply_text: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            rejection_code: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            rejection_message: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Xero could not decode winning notification-reply lookup rows from {}: {error}",
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
                "Xero could not persist the rejected notification reply claim.",
            )
        })?;

    Ok(transaction.last_insert_rowid())
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
            format!("Xero could not canonicalize notification route metadata JSON: {error}"),
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
