use std::path::Path;

use rusqlite::{params, Connection, Error as SqlError, ErrorCode};

use crate::{
    commands::{
        CommandError, OperatorApprovalDto, OperatorApprovalStatus, ResumeHistoryEntryDto,
        ResumeHistoryStatus, VerificationRecordDto, VerificationRecordStatus,
    },
    db::database_path_for_repo,
};

use super::{
    classify_operator_answer_requirement, find_prohibited_runtime_persistence_content,
    find_prohibited_transition_diagnostic_content, normalize_runtime_checkpoint_summary,
    open_project_database, read_project_row, PreparedRuntimeOperatorResume,
    ResumeOperatorRunRecord,
};

// The `operator_approvals`, `operator_verification_records`, and
// `operator_resume_history` tables are the live human-in-the-loop approval gate
// for the autonomous tool runtime (see `*_with_operator_approval` paths in
// `runtime/autonomous_tool_runtime/`). They were decoupled from the deprecated
// workflow state machine in Phase 5 of the storage refactor — the previous
// `gate_*` / `transition_*` columns and indexes were dropped, and the SQL/DTO
// surface no longer references workflow concepts despite the table names.

const MAX_APPROVAL_REQUEST_ROWS: i64 = 50;
const MAX_VERIFICATION_RECORD_ROWS: i64 = 100;
const MAX_RESUME_HISTORY_ROWS: i64 = 100;

#[derive(Debug)]
pub(crate) struct ProjectSummaryRow {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) milestone: String,
    pub(crate) branch: Option<String>,
    pub(crate) runtime: Option<String>,
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

#[derive(Debug)]
struct RawOperatorApprovalRow {
    action_id: String,
    session_id: Option<String>,
    flow_id: Option<String>,
    action_type: String,
    title: String,
    detail: String,
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

#[allow(clippy::too_many_arguments)]
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

    let action_id = derive_operator_action_id(session_id, flow_id, action_type)?;

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
                        'pending',
                        NULL,
                        ?8,
                        ?8,
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
                            updated_at = ?7
                        WHERE project_id = ?1
                          AND action_id = ?2
                          AND status = 'pending'
                        "#,
                        params![
                            project_id, action_id, session_id, flow_id, title, detail, created_at,
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

    let answer_required = matches!(decision, OperatorApprovalDecision::Approved)
        && classify_operator_answer_requirement(&existing)?;

    if answer_required && decision_note.is_none() {
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
    })
}

pub(crate) fn read_operator_approvals(
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
                    user_answer: row.get(6)?,
                    status: row.get(7)?,
                    decision_note: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                    resolved_at: row.get(11)?,
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

pub(crate) fn read_verification_records(
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

pub(crate) fn read_resume_history(
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

pub(crate) fn read_operator_approval_by_action_id(
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

    let decode_field = |index: usize| -> Result<String, CommandError> {
        row.get::<_, String>(index).map_err(|error| {
            CommandError::system_fault(
                "operator_approval_decode_failed",
                format!(
                    "Cadence could not decode operator-approval lookup rows from {}: {error}",
                    database_path.display()
                ),
            )
        })
    };
    let decode_optional_field = |index: usize| -> Result<Option<String>, CommandError> {
        row.get::<_, Option<String>>(index).map_err(|error| {
            CommandError::system_fault(
                "operator_approval_decode_failed",
                format!(
                    "Cadence could not decode operator-approval lookup rows from {}: {error}",
                    database_path.display()
                ),
            )
        })
    };

    decode_operator_approval_row(
        RawOperatorApprovalRow {
            action_id: decode_field(0)?,
            session_id: decode_optional_field(1)?,
            flow_id: decode_optional_field(2)?,
            action_type: decode_field(3)?,
            title: decode_field(4)?,
            detail: decode_field(5)?,
            user_answer: decode_optional_field(6)?,
            status: decode_field(7)?,
            decision_note: decode_optional_field(8)?,
            created_at: decode_field(9)?,
            updated_at: decode_field(10)?,
            resolved_at: decode_optional_field(11)?,
        },
        database_path,
    )
    .map(Some)
}

pub(crate) fn read_verification_record_by_id(
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

pub(crate) fn read_resume_history_entry_by_id(
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

pub(crate) fn derive_operator_scope_prefix(
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

pub(crate) fn derive_operator_action_id(
    session_id: &str,
    flow_id: Option<&str>,
    action_type: &str,
) -> Result<String, CommandError> {
    let action_type = action_type.trim();
    if action_type.is_empty() {
        return Err(CommandError::system_fault(
            "runtime_action_request_invalid",
            "Cadence could not persist the runtime approval because the action-required item was missing a stable action type.",
        ));
    }

    let stable_scope = derive_operator_scope_prefix(session_id, flow_id)?;

    Ok(format!("{stable_scope}:{action_type}"))
}

pub(crate) fn validate_non_empty_text(
    value: &str,
    field: &str,
    code: &str,
) -> Result<(), CommandError> {
    if value.trim().is_empty() {
        return Err(CommandError::user_fixable(
            code,
            format!("Field `{field}` must be a non-empty string."),
        ));
    }

    Ok(())
}

pub(crate) fn operator_approval_status_label(status: &OperatorApprovalStatus) -> &'static str {
    match status {
        OperatorApprovalStatus::Pending => "pending",
        OperatorApprovalStatus::Approved => "approved",
        OperatorApprovalStatus::Rejected => "rejected",
    }
}

pub(crate) fn map_operator_loop_transaction_error(
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

pub(crate) fn map_operator_loop_write_error(
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

pub(crate) fn map_operator_loop_commit_error(
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

pub(crate) fn sqlite_path_suffix(database_path: &Path) -> String {
    format!("against {}.", database_path.display())
}

pub(crate) fn is_unique_constraint_violation(error: &SqlError) -> bool {
    match error {
        SqlError::SqliteFailure(inner, _) => {
            matches!(inner.code, ErrorCode::ConstraintViolation)
        }
        _ => false,
    }
}

pub(crate) fn is_retryable_sql_error(error: &SqlError) -> bool {
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

pub(crate) fn decode_snapshot_row_id(
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

pub(crate) fn require_non_empty_owned(
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

pub(crate) fn decode_optional_non_empty_text(
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

pub(crate) fn map_snapshot_decode_error(
    code: &str,
    database_path: &Path,
    details: String,
) -> CommandError {
    CommandError::system_fault(
        code,
        format!(
            "Cadence could not decode selected-project operator-loop metadata from {}: {details}",
            database_path.display()
        ),
    )
}

pub(crate) fn map_project_query_error(
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
