use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};

use crate::{auth::now_timestamp, commands::CommandError, db::database_path_for_repo};

use super::{
    agent_context::{load_active_agent_compaction, AgentCompactionRecord, AgentCompactionTrigger},
    agent_core::{
        agent_event_kind_sql_value, agent_message_role_sql_value, agent_run_status_sql_value,
        agent_tool_call_state_sql_value, load_agent_run, load_agent_usage, AgentMessageRecord,
        AgentMessageRole, AgentRunDiagnosticRecord, AgentRunEventKind, AgentRunSnapshotRecord,
        AgentRunStatus,
    },
    agent_session::{
        clear_selected_agent_session, generate_agent_session_id, read_agent_session_row,
        AgentSessionRecord,
    },
    open_runtime_database, read_project_row, validate_non_empty_text,
};

const OWNED_AGENT_LINEAGE_RUNTIME_KIND: &str = "owned_agent";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSessionLineageBoundaryKind {
    Run,
    Message,
    Checkpoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionLineageDiagnosticRecord {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionLineageRecord {
    pub lineage_id: String,
    pub project_id: String,
    pub child_agent_session_id: String,
    pub source_agent_session_id: Option<String>,
    pub source_run_id: Option<String>,
    pub source_boundary_kind: AgentSessionLineageBoundaryKind,
    pub source_message_id: Option<i64>,
    pub source_checkpoint_id: Option<i64>,
    pub source_compaction_id: Option<String>,
    pub source_title: String,
    pub branch_title: String,
    pub replay_run_id: String,
    pub file_change_summary: String,
    pub diagnostic: Option<AgentSessionLineageDiagnosticRecord>,
    pub created_at: String,
    pub source_deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSessionBranchBoundary {
    Run,
    Message { message_id: i64 },
    Checkpoint { checkpoint_id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionBranchCreateRecord {
    pub project_id: String,
    pub source_agent_session_id: String,
    pub source_run_id: String,
    pub title: Option<String>,
    pub selected: bool,
    pub boundary: AgentSessionBranchBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionBranchRecord {
    pub session: AgentSessionRecord,
    pub lineage: AgentSessionLineageRecord,
    pub replay_run: AgentRunSnapshotRecord,
}

#[derive(Debug, Clone)]
struct ResolvedBranchBoundary {
    kind: AgentSessionLineageBoundaryKind,
    source_message_id: Option<i64>,
    source_checkpoint_id: Option<i64>,
    cutoff_message_id: Option<i64>,
    cutoff_created_at: Option<String>,
}

#[derive(Debug, Clone)]
struct CopiedMessageRecord {
    old_id: i64,
    new_id: i64,
    role: AgentMessageRole,
    content: String,
}

#[derive(Debug, Clone)]
struct CopiedEventRecord {
    old_id: i64,
    new_id: i64,
    run_id: String,
    event_kind: AgentRunEventKind,
    payload_json: String,
}

pub fn create_agent_session_branch(
    repo_root: &Path,
    request: &AgentSessionBranchCreateRecord,
) -> Result<AgentSessionBranchRecord, CommandError> {
    validate_non_empty_text(
        &request.project_id,
        "projectId",
        "agent_session_branch_request_invalid",
    )?;
    validate_non_empty_text(
        &request.source_agent_session_id,
        "sourceAgentSessionId",
        "agent_session_branch_request_invalid",
    )?;
    validate_non_empty_text(
        &request.source_run_id,
        "sourceRunId",
        "agent_session_branch_request_invalid",
    )?;
    if let Some(title) = request.title.as_deref() {
        validate_non_empty_text(title, "title", "agent_session_branch_request_invalid")?;
    }

    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &request.project_id)?;

    let source_session = read_agent_session_row(
        &connection,
        &database_path,
        &request.project_id,
        &request.source_agent_session_id,
    )?
    .ok_or_else(|| {
        CommandError::user_fixable(
            "agent_session_not_found",
            format!(
                "Xero could not find source agent session `{}` for project `{}`.",
                request.source_agent_session_id, request.project_id
            ),
        )
    })?;
    let source_snapshot = load_agent_run(repo_root, &request.project_id, &request.source_run_id)?;
    ensure_source_run_belongs_to_session(&source_snapshot, request)?;
    let boundary = resolve_branch_boundary(&source_snapshot, &request.boundary)?;
    let active_compaction = load_active_agent_compaction(
        repo_root,
        &request.project_id,
        &request.source_agent_session_id,
    )?
    .filter(|compaction| {
        compaction.covered_run_ids.len() == 1 && compaction.covers_run(&request.source_run_id)
    });
    let source_usage = load_agent_usage(repo_root, &request.project_id, &request.source_run_id)?;

    let child_agent_session_id = generate_agent_session_id();
    let replay_run_id = generate_branch_replay_run_id(&request.source_run_id);
    let lineage_id = generate_agent_session_lineage_id();
    let now = now_timestamp();
    let branch_title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_branch_title(&source_session.title, &boundary));
    let source_title = source_session.title.clone();
    let branch_summary = branch_summary(&source_session.title, &boundary);
    let file_change_summary = file_change_summary(&source_snapshot, &boundary);

    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "agent_session_branch_transaction_failed",
            format!(
                "Xero could not start the agent-session branch transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    if request.selected {
        clear_selected_agent_session(&transaction, &database_path, &request.project_id)?;
    }

    transaction
        .execute(
            r#"
            INSERT INTO agent_sessions (
                project_id,
                agent_session_id,
                title,
                summary,
                status,
                selected,
                created_at,
                updated_at,
                last_run_id,
                last_runtime_kind,
                last_provider_id
            )
            VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?6, ?7, ?8, ?9)
            "#,
            params![
                request.project_id.as_str(),
                child_agent_session_id.as_str(),
                branch_title.as_str(),
                branch_summary.as_str(),
                if request.selected { 1 } else { 0 },
                now.as_str(),
                replay_run_id.as_str(),
                OWNED_AGENT_LINEAGE_RUNTIME_KIND,
                source_snapshot.run.provider_id.as_str(),
            ],
        )
        .map_err(|error| {
            map_lineage_write_error(&database_path, "agent_session_branch_insert_failed", error)
        })?;

    insert_replay_run(
        &transaction,
        &source_snapshot,
        &request.project_id,
        &child_agent_session_id,
        &replay_run_id,
        &now,
    )?;
    let copied_messages =
        copy_replay_messages(&transaction, &source_snapshot, &boundary, &replay_run_id)?;
    let copied_events =
        copy_replay_events(&transaction, &source_snapshot, &boundary, &replay_run_id)?;
    copy_replay_tool_calls(
        &transaction,
        &source_snapshot,
        &boundary,
        &replay_run_id,
        copied_messages.as_slice(),
    )?;
    copy_replay_file_changes(&transaction, &source_snapshot, &boundary, &replay_run_id)?;
    copy_replay_checkpoints(&transaction, &source_snapshot, &boundary, &replay_run_id)?;
    copy_replay_action_requests(&transaction, &source_snapshot, &boundary, &replay_run_id)?;
    if matches!(boundary.kind, AgentSessionLineageBoundaryKind::Run) {
        if let Some(usage) = source_usage.as_ref() {
            transaction
                .execute(
                    r#"
                    INSERT INTO agent_usage (
                        project_id,
                        run_id,
                        agent_definition_id,
                        agent_definition_version,
                        provider_id,
                        model_id,
                        input_tokens,
                        output_tokens,
                        total_tokens,
                        estimated_cost_micros,
                        updated_at
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                    "#,
                    params![
                        usage.project_id.as_str(),
                        replay_run_id.as_str(),
                        usage.agent_definition_id.as_str(),
                        usage.agent_definition_version,
                        usage.provider_id.as_str(),
                        usage.model_id.as_str(),
                        usage.input_tokens,
                        usage.output_tokens,
                        usage.total_tokens,
                        usage.estimated_cost_micros,
                        now.as_str(),
                    ],
                )
                .map_err(|error| {
                    map_lineage_write_error(
                        &database_path,
                        "agent_session_branch_usage_copy_failed",
                        error,
                    )
                })?;
        }
    }

    let carried_compaction_id = copy_replay_compaction_if_available(
        &transaction,
        active_compaction.as_ref(),
        &source_snapshot,
        &boundary,
        &copied_messages,
        &copied_events,
        &child_agent_session_id,
        &replay_run_id,
        &now,
    )?;

    transaction
        .execute(
            r#"
            INSERT INTO agent_session_lineage (
                lineage_id,
                project_id,
                child_agent_session_id,
                source_agent_session_id,
                source_run_id,
                source_boundary_kind,
                source_message_id,
                source_checkpoint_id,
                source_compaction_id,
                source_title,
                branch_title,
                replay_run_id,
                file_change_summary,
                diagnostic_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, NULL, ?14)
            "#,
            params![
                lineage_id.as_str(),
                request.project_id.as_str(),
                child_agent_session_id.as_str(),
                request.source_agent_session_id.as_str(),
                request.source_run_id.as_str(),
                lineage_boundary_kind_sql_value(&boundary.kind),
                boundary.source_message_id,
                boundary.source_checkpoint_id,
                carried_compaction_id.as_deref(),
                source_title.as_str(),
                branch_title.as_str(),
                replay_run_id.as_str(),
                file_change_summary.as_str(),
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_lineage_write_error(&database_path, "agent_session_lineage_insert_failed", error)
        })?;

    transaction.commit().map_err(|error| {
        CommandError::system_fault(
            "agent_session_branch_commit_failed",
            format!(
                "Xero could not commit the agent-session branch transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    let session = read_agent_session_row(
        &connection,
        &database_path,
        &request.project_id,
        &child_agent_session_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "agent_session_branch_missing_after_persist",
            format!(
                "Xero created branch session `{child_agent_session_id}` in {} but could not read it back.",
                database_path.display()
            ),
        )
    })?;
    let lineage = read_agent_session_lineage_for_child(
        &connection,
        &database_path,
        &request.project_id,
        &child_agent_session_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "agent_session_lineage_missing_after_persist",
            format!(
                "Xero created branch lineage `{lineage_id}` in {} but could not read it back.",
                database_path.display()
            ),
        )
    })?;
    let replay_run = load_agent_run(repo_root, &request.project_id, &replay_run_id)?;

    Ok(AgentSessionBranchRecord {
        session,
        lineage,
        replay_run,
    })
}

pub(crate) fn read_agent_session_lineage_for_child(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    child_agent_session_id: &str,
) -> Result<Option<AgentSessionLineageRecord>, CommandError> {
    connection
        .query_row(
            r#"
            SELECT
                lineage_id,
                project_id,
                child_agent_session_id,
                source_agent_session_id,
                source_run_id,
                source_boundary_kind,
                source_message_id,
                source_checkpoint_id,
                source_compaction_id,
                source_title,
                branch_title,
                replay_run_id,
                file_change_summary,
                diagnostic_json,
                created_at,
                source_deleted_at
            FROM agent_session_lineage
            WHERE project_id = ?1
              AND child_agent_session_id = ?2
            "#,
            params![project_id, child_agent_session_id],
            read_agent_session_lineage_row,
        )
        .optional()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_lineage_query_failed",
                format!(
                    "Xero could not read agent-session lineage from {}: {error}",
                    database_path.display()
                ),
            )
        })?
        .transpose()
}

fn ensure_source_run_belongs_to_session(
    snapshot: &AgentRunSnapshotRecord,
    request: &AgentSessionBranchCreateRecord,
) -> Result<(), CommandError> {
    if snapshot.run.project_id != request.project_id
        || snapshot.run.agent_session_id != request.source_agent_session_id
    {
        return Err(CommandError::user_fixable(
            "agent_run_session_mismatch",
            format!(
                "Owned-agent run `{}` belongs to session `{}` for project `{}`, not session `{}` for project `{}`.",
                snapshot.run.run_id,
                snapshot.run.agent_session_id,
                snapshot.run.project_id,
                request.source_agent_session_id,
                request.project_id
            ),
        ));
    }
    Ok(())
}

fn resolve_branch_boundary(
    snapshot: &AgentRunSnapshotRecord,
    boundary: &AgentSessionBranchBoundary,
) -> Result<ResolvedBranchBoundary, CommandError> {
    match boundary {
        AgentSessionBranchBoundary::Run => Ok(ResolvedBranchBoundary {
            kind: AgentSessionLineageBoundaryKind::Run,
            source_message_id: None,
            source_checkpoint_id: None,
            cutoff_message_id: None,
            cutoff_created_at: None,
        }),
        AgentSessionBranchBoundary::Message { message_id } => {
            if *message_id <= 0 {
                return Err(CommandError::invalid_request("messageId"));
            }
            let message = snapshot
                .messages
                .iter()
                .find(|message| message.id == *message_id)
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "agent_rewind_boundary_not_found",
                        format!(
                            "Xero could not find message boundary `{message_id}` in owned-agent run `{}`.",
                            snapshot.run.run_id
                        ),
                    )
                })?;
            if message.role == AgentMessageRole::System {
                return Err(CommandError::user_fixable(
                    "agent_rewind_boundary_invalid",
                    "Xero cannot rewind to a system-prompt boundary. Choose a user, assistant, or tool message instead.",
                ));
            }
            Ok(ResolvedBranchBoundary {
                kind: AgentSessionLineageBoundaryKind::Message,
                source_message_id: Some(*message_id),
                source_checkpoint_id: None,
                cutoff_message_id: Some(*message_id),
                cutoff_created_at: Some(message.created_at.clone()),
            })
        }
        AgentSessionBranchBoundary::Checkpoint { checkpoint_id } => {
            if *checkpoint_id <= 0 {
                return Err(CommandError::invalid_request("checkpointId"));
            }
            let checkpoint = snapshot
                .checkpoints
                .iter()
                .find(|checkpoint| checkpoint.id == *checkpoint_id)
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "agent_rewind_boundary_not_found",
                        format!(
                            "Xero could not find checkpoint boundary `{checkpoint_id}` in owned-agent run `{}`.",
                            snapshot.run.run_id
                        ),
                    )
                })?;
            Ok(ResolvedBranchBoundary {
                kind: AgentSessionLineageBoundaryKind::Checkpoint,
                source_message_id: None,
                source_checkpoint_id: Some(*checkpoint_id),
                cutoff_message_id: None,
                cutoff_created_at: Some(checkpoint.created_at.clone()),
            })
        }
    }
}

fn insert_replay_run(
    transaction: &Transaction<'_>,
    source_snapshot: &AgentRunSnapshotRecord,
    project_id: &str,
    child_agent_session_id: &str,
    replay_run_id: &str,
    now: &str,
) -> Result<(), CommandError> {
    transaction
        .execute(
            r#"
            INSERT INTO agent_runs (
                runtime_agent_id,
                agent_definition_id,
                agent_definition_version,
                project_id,
                agent_session_id,
                run_id,
                trace_id,
                provider_id,
                model_id,
                status,
                prompt,
                system_prompt,
                started_at,
                last_heartbeat_at,
                completed_at,
                cancelled_at,
                last_error_code,
                last_error_message,
                updated_at,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13, ?13, NULL, NULL, NULL, ?13, ?13)
            "#,
            params![
                source_snapshot.run.runtime_agent_id.as_str(),
                source_snapshot.run.agent_definition_id.as_str(),
                source_snapshot.run.agent_definition_version,
                project_id,
                child_agent_session_id,
                replay_run_id,
                xero_agent_core::runtime_trace_id_for_run(project_id, replay_run_id),
                source_snapshot.run.provider_id.as_str(),
                source_snapshot.run.model_id.as_str(),
                agent_run_status_sql_value(&AgentRunStatus::Completed),
                source_snapshot.run.prompt.as_str(),
                source_snapshot.run.system_prompt.as_str(),
                now,
            ],
        )
        .map(|_| ())
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_branch_run_insert_failed",
                format!("Xero could not create the branch replay run: {error}"),
            )
        })
}

fn copy_replay_messages(
    transaction: &Transaction<'_>,
    source_snapshot: &AgentRunSnapshotRecord,
    boundary: &ResolvedBranchBoundary,
    replay_run_id: &str,
) -> Result<Vec<CopiedMessageRecord>, CommandError> {
    let mut copied = Vec::new();
    for message in source_snapshot
        .messages
        .iter()
        .filter(|message| includes_message(message, boundary))
    {
        transaction
            .execute(
                r#"
                INSERT INTO agent_messages (project_id, run_id, role, content, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                params![
                    message.project_id.as_str(),
                    replay_run_id,
                    agent_message_role_sql_value(&message.role),
                    message.content.as_str(),
                    message.created_at.as_str(),
                ],
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "agent_session_branch_message_copy_failed",
                    format!("Xero could not copy a branch replay message: {error}"),
                )
            })?;
        copied.push(CopiedMessageRecord {
            old_id: message.id,
            new_id: transaction.last_insert_rowid(),
            role: message.role.clone(),
            content: message.content.clone(),
        });
    }
    Ok(copied)
}

fn copy_replay_events(
    transaction: &Transaction<'_>,
    source_snapshot: &AgentRunSnapshotRecord,
    boundary: &ResolvedBranchBoundary,
    replay_run_id: &str,
) -> Result<Vec<CopiedEventRecord>, CommandError> {
    let mut copied = Vec::new();
    for event in source_snapshot
        .events
        .iter()
        .filter(|event| includes_created_at(event.created_at.as_str(), boundary))
    {
        transaction
            .execute(
                r#"
                INSERT INTO agent_events (project_id, run_id, event_kind, payload_json, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                params![
                    event.project_id.as_str(),
                    replay_run_id,
                    agent_event_kind_sql_value(&event.event_kind),
                    event.payload_json.as_str(),
                    event.created_at.as_str(),
                ],
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "agent_session_branch_event_copy_failed",
                    format!("Xero could not copy a branch replay event: {error}"),
                )
            })?;
        copied.push(CopiedEventRecord {
            old_id: event.id,
            new_id: transaction.last_insert_rowid(),
            run_id: replay_run_id.into(),
            event_kind: event.event_kind.clone(),
            payload_json: event.payload_json.clone(),
        });
    }
    Ok(copied)
}

fn copy_replay_tool_calls(
    transaction: &Transaction<'_>,
    source_snapshot: &AgentRunSnapshotRecord,
    boundary: &ResolvedBranchBoundary,
    replay_run_id: &str,
    copied_messages: &[CopiedMessageRecord],
) -> Result<(), CommandError> {
    let tool_call_ids_from_messages = copied_tool_result_ids(copied_messages)?;
    for tool_call in &source_snapshot.tool_calls {
        if !matches!(boundary.kind, AgentSessionLineageBoundaryKind::Run)
            && !tool_call_ids_from_messages.contains(&tool_call.tool_call_id)
            && !includes_tool_call_by_time(
                tool_call.started_at.as_str(),
                tool_call.completed_at.as_deref(),
                boundary,
            )
        {
            continue;
        }
        transaction
            .execute(
                r#"
                INSERT INTO agent_tool_calls (
                    project_id,
                    run_id,
                    tool_call_id,
                    tool_name,
                    input_json,
                    state,
                    result_json,
                    error_code,
                    error_message,
                    started_at,
                    completed_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
                params![
                    tool_call.project_id.as_str(),
                    replay_run_id,
                    tool_call.tool_call_id.as_str(),
                    tool_call.tool_name.as_str(),
                    tool_call.input_json.as_str(),
                    agent_tool_call_state_sql_value(&tool_call.state),
                    tool_call.result_json.as_deref(),
                    tool_call.error.as_ref().map(|error| error.code.as_str()),
                    tool_call.error.as_ref().map(|error| error.message.as_str()),
                    tool_call.started_at.as_str(),
                    tool_call.completed_at.as_deref(),
                ],
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "agent_session_branch_tool_copy_failed",
                    format!("Xero could not copy a branch replay tool call: {error}"),
                )
            })?;
    }
    Ok(())
}

fn copy_replay_file_changes(
    transaction: &Transaction<'_>,
    source_snapshot: &AgentRunSnapshotRecord,
    boundary: &ResolvedBranchBoundary,
    replay_run_id: &str,
) -> Result<(), CommandError> {
    for change in source_snapshot
        .file_changes
        .iter()
        .filter(|change| includes_created_at(change.created_at.as_str(), boundary))
    {
        transaction
            .execute(
                r#"
                INSERT INTO agent_file_changes (
                    project_id,
                    run_id,
                    trace_id,
                    top_level_run_id,
                    subagent_id,
                    subagent_role,
                    path,
                    operation,
                    old_hash,
                    new_hash,
                    created_at
                )
                VALUES (?1, ?2, ?3, ?2, NULL, NULL, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    change.project_id.as_str(),
                    replay_run_id,
                    xero_agent_core::runtime_trace_id_for_run(
                        change.project_id.as_str(),
                        replay_run_id,
                    ),
                    change.path.as_str(),
                    change.operation.as_str(),
                    change.old_hash.as_deref(),
                    change.new_hash.as_deref(),
                    change.created_at.as_str(),
                ],
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "agent_session_branch_file_change_copy_failed",
                    format!("Xero could not copy branch replay file-change metadata: {error}"),
                )
            })?;
    }
    Ok(())
}

fn copy_replay_checkpoints(
    transaction: &Transaction<'_>,
    source_snapshot: &AgentRunSnapshotRecord,
    boundary: &ResolvedBranchBoundary,
    replay_run_id: &str,
) -> Result<(), CommandError> {
    for checkpoint in source_snapshot.checkpoints.iter().filter(|checkpoint| {
        includes_checkpoint(checkpoint.id, checkpoint.created_at.as_str(), boundary)
    }) {
        transaction
            .execute(
                r#"
                INSERT INTO agent_checkpoints (
                    project_id,
                    run_id,
                    checkpoint_kind,
                    summary,
                    payload_json,
                    created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    checkpoint.project_id.as_str(),
                    replay_run_id,
                    checkpoint.checkpoint_kind.as_str(),
                    checkpoint.summary.as_str(),
                    checkpoint.payload_json.as_deref(),
                    checkpoint.created_at.as_str(),
                ],
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "agent_session_branch_checkpoint_copy_failed",
                    format!("Xero could not copy branch replay checkpoint metadata: {error}"),
                )
            })?;
    }
    Ok(())
}

fn copy_replay_action_requests(
    transaction: &Transaction<'_>,
    source_snapshot: &AgentRunSnapshotRecord,
    boundary: &ResolvedBranchBoundary,
    replay_run_id: &str,
) -> Result<(), CommandError> {
    for action in source_snapshot
        .action_requests
        .iter()
        .filter(|action| includes_created_at(action.created_at.as_str(), boundary))
    {
        let branch_action_id = format!("branch-{replay_run_id}-{}", action.action_id);
        transaction
            .execute(
                r#"
                INSERT INTO agent_action_requests (
                    project_id,
                    run_id,
                    action_id,
                    action_type,
                    title,
                    detail,
                    status,
                    created_at,
                    resolved_at,
                    response
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
                params![
                    action.project_id.as_str(),
                    replay_run_id,
                    branch_action_id.as_str(),
                    action.action_type.as_str(),
                    action.title.as_str(),
                    action.detail.as_str(),
                    action.status.as_str(),
                    action.created_at.as_str(),
                    action.resolved_at.as_deref(),
                    action.response.as_deref(),
                ],
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "agent_session_branch_action_copy_failed",
                    format!("Xero could not copy branch replay action-request metadata: {error}"),
                )
            })?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn copy_replay_compaction_if_available(
    transaction: &Transaction<'_>,
    active_compaction: Option<&AgentCompactionRecord>,
    source_snapshot: &AgentRunSnapshotRecord,
    boundary: &ResolvedBranchBoundary,
    copied_messages: &[CopiedMessageRecord],
    copied_events: &[CopiedEventRecord],
    child_agent_session_id: &str,
    replay_run_id: &str,
    now: &str,
) -> Result<Option<String>, CommandError> {
    let Some(compaction) = active_compaction else {
        return Ok(None);
    };
    if !boundary_covers_compaction(compaction, boundary) {
        return Ok(None);
    }
    let copied_messages_by_old_id = copied_messages
        .iter()
        .map(|message| (message.old_id, message))
        .collect::<BTreeMap<_, _>>();
    let copied_events_by_old_id = copied_events
        .iter()
        .map(|event| (event.old_id, event))
        .collect::<BTreeMap<_, _>>();
    let covered_messages = source_snapshot
        .messages
        .iter()
        .filter(|message| compaction.covers_message_id(message.id))
        .filter_map(|message| copied_messages_by_old_id.get(&message.id).copied())
        .collect::<Vec<_>>();
    if compaction.covered_message_start_id.is_some() && covered_messages.is_empty() {
        return Ok(None);
    }
    let covered_message_start_id = covered_messages.iter().map(|message| message.new_id).min();
    let covered_message_end_id = covered_messages.iter().map(|message| message.new_id).max();
    let covered_events = match (
        compaction.covered_event_start_id,
        compaction.covered_event_end_id,
    ) {
        (Some(start), Some(end)) => source_snapshot
            .events
            .iter()
            .filter(|event| event.id >= start && event.id <= end)
            .filter_map(|event| copied_events_by_old_id.get(&event.id).copied())
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let covered_event_start_id = covered_events.iter().map(|event| event.new_id).min();
    let covered_event_end_id = covered_events.iter().map(|event| event.new_id).max();
    let source_hash = replay_source_hash(
        replay_run_id,
        &source_snapshot.run.provider_id,
        &source_snapshot.run.model_id,
        &source_snapshot.run.prompt,
        &covered_messages,
        &covered_events,
    );
    let replay_compaction_id = format!(
        "session-compact:{}:{}:{}:{}",
        replay_run_id,
        now,
        source_hash.chars().take(12).collect::<String>(),
        random_hex_suffix(),
    );
    transaction
        .execute(
            r#"
            INSERT INTO agent_compactions (
                compaction_id,
                project_id,
                agent_session_id,
                source_run_id,
                provider_id,
                model_id,
                summary,
                covered_run_ids_json,
                covered_message_start_id,
                covered_message_end_id,
                covered_event_start_id,
                covered_event_end_id,
                source_hash,
                input_tokens,
                summary_tokens,
                raw_tail_message_count,
                policy_reason,
                trigger_kind,
                active,
                diagnostic_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, 1, ?19, ?20)
            "#,
            params![
                replay_compaction_id.as_str(),
                compaction.project_id.as_str(),
                child_agent_session_id,
                replay_run_id,
                compaction.provider_id.as_str(),
                compaction.model_id.as_str(),
                compaction.summary.as_str(),
                serde_json::to_string(&vec![replay_run_id.to_string()]).map_err(|error| {
                    CommandError::system_fault(
                        "agent_session_branch_compaction_serialize_failed",
                        format!("Xero could not serialize branch compaction coverage: {error}"),
                    )
                })?,
                covered_message_start_id,
                covered_message_end_id,
                covered_event_start_id,
                covered_event_end_id,
                source_hash.as_str(),
                compaction.input_tokens,
                compaction.summary_tokens,
                compaction.raw_tail_message_count,
                compaction.policy_reason.as_str(),
                compaction_trigger_sql_value(&compaction.trigger),
                diagnostic_json(compaction.diagnostic.as_ref())?,
                now,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_branch_compaction_copy_failed",
                format!("Xero could not copy active compaction summary into branch replay: {error}"),
            )
        })?;
    Ok(Some(compaction.compaction_id.clone()))
}

fn includes_message(message: &AgentMessageRecord, boundary: &ResolvedBranchBoundary) -> bool {
    match boundary.kind {
        AgentSessionLineageBoundaryKind::Run => true,
        AgentSessionLineageBoundaryKind::Message => boundary
            .cutoff_message_id
            .is_some_and(|cutoff| message.id <= cutoff),
        AgentSessionLineageBoundaryKind::Checkpoint => {
            includes_created_at(&message.created_at, boundary)
        }
    }
}

fn includes_created_at(created_at: &str, boundary: &ResolvedBranchBoundary) -> bool {
    match boundary.kind {
        AgentSessionLineageBoundaryKind::Run => true,
        AgentSessionLineageBoundaryKind::Message | AgentSessionLineageBoundaryKind::Checkpoint => {
            boundary
                .cutoff_created_at
                .as_deref()
                .is_some_and(|cutoff| created_at <= cutoff)
        }
    }
}

fn includes_checkpoint(id: i64, created_at: &str, boundary: &ResolvedBranchBoundary) -> bool {
    match boundary.kind {
        AgentSessionLineageBoundaryKind::Run => true,
        AgentSessionLineageBoundaryKind::Message => includes_created_at(created_at, boundary),
        AgentSessionLineageBoundaryKind::Checkpoint => boundary
            .source_checkpoint_id
            .is_some_and(|cutoff_id| id <= cutoff_id),
    }
}

fn includes_tool_call_by_time(
    started_at: &str,
    completed_at: Option<&str>,
    boundary: &ResolvedBranchBoundary,
) -> bool {
    match boundary.kind {
        AgentSessionLineageBoundaryKind::Run => true,
        AgentSessionLineageBoundaryKind::Message | AgentSessionLineageBoundaryKind::Checkpoint => {
            let Some(cutoff) = boundary.cutoff_created_at.as_deref() else {
                return false;
            };
            completed_at
                .map(|completed_at| completed_at <= cutoff)
                .unwrap_or(started_at <= cutoff)
        }
    }
}

fn boundary_covers_compaction(
    compaction: &AgentCompactionRecord,
    boundary: &ResolvedBranchBoundary,
) -> bool {
    match boundary.kind {
        AgentSessionLineageBoundaryKind::Run => true,
        AgentSessionLineageBoundaryKind::Message => {
            match (
                boundary.cutoff_message_id,
                compaction.covered_message_end_id,
            ) {
                (Some(cutoff), Some(end)) => cutoff >= end,
                (_, None) => true,
                _ => false,
            }
        }
        AgentSessionLineageBoundaryKind::Checkpoint => false,
    }
}

fn copied_tool_result_ids(
    copied_messages: &[CopiedMessageRecord],
) -> Result<BTreeSet<String>, CommandError> {
    let mut ids = BTreeSet::new();
    for message in copied_messages
        .iter()
        .filter(|message| message.role == AgentMessageRole::Tool)
    {
        let value = serde_json::from_str::<JsonValue>(&message.content).map_err(|error| {
            CommandError::system_fault(
                "agent_session_branch_tool_result_decode_failed",
                format!(
                    "Xero could not decode a copied tool-result message before branching: {error}"
                ),
            )
        })?;
        if let Some(tool_call_id) = value
            .get("toolCallId")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            ids.insert(tool_call_id.to_string());
        }
    }
    Ok(ids)
}

fn replay_source_hash(
    replay_run_id: &str,
    provider_id: &str,
    model_id: &str,
    prompt: &str,
    covered_messages: &[&CopiedMessageRecord],
    covered_events: &[&CopiedEventRecord],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(replay_run_id.as_bytes());
    hasher.update(provider_id.as_bytes());
    hasher.update(model_id.as_bytes());
    hasher.update(prompt.as_bytes());
    for message in covered_messages {
        hasher.update(message.new_id.to_string().as_bytes());
        hasher.update(format!("{:?}", message.role).as_bytes());
        hasher.update(message.content.as_bytes());
    }
    for event in covered_events {
        hasher.update(event.new_id.to_string().as_bytes());
        hasher.update(event.run_id.as_bytes());
        hasher.update(format!("{:?}", event.event_kind).as_bytes());
        hasher.update(event.payload_json.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn read_agent_session_lineage_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<AgentSessionLineageRecord, CommandError>> {
    let diagnostic_json: Option<String> = row.get(13)?;
    Ok(decode_agent_session_lineage_row(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get::<_, String>(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
        diagnostic_json,
        row.get(14)?,
        row.get(15)?,
    ))
}

#[allow(clippy::too_many_arguments)]
fn decode_agent_session_lineage_row(
    lineage_id: String,
    project_id: String,
    child_agent_session_id: String,
    source_agent_session_id: Option<String>,
    source_run_id: Option<String>,
    boundary_kind: String,
    source_message_id: Option<i64>,
    source_checkpoint_id: Option<i64>,
    source_compaction_id: Option<String>,
    source_title: String,
    branch_title: String,
    replay_run_id: String,
    file_change_summary: String,
    diagnostic_json: Option<String>,
    created_at: String,
    source_deleted_at: Option<String>,
) -> Result<AgentSessionLineageRecord, CommandError> {
    let diagnostic = diagnostic_json
        .as_deref()
        .map(|value| {
            serde_json::from_str::<AgentSessionLineageDiagnosticRecordWire>(value).map(|wire| {
                AgentSessionLineageDiagnosticRecord {
                    code: wire.code,
                    message: wire.message,
                }
            })
        })
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_lineage_diagnostic_decode_failed",
                format!("Xero could not decode branch lineage diagnostics: {error}"),
            )
        })?;
    Ok(AgentSessionLineageRecord {
        lineage_id,
        project_id,
        child_agent_session_id,
        source_agent_session_id,
        source_run_id,
        source_boundary_kind: parse_lineage_boundary_kind(&boundary_kind),
        source_message_id,
        source_checkpoint_id,
        source_compaction_id,
        source_title,
        branch_title,
        replay_run_id,
        file_change_summary,
        diagnostic,
        created_at,
        source_deleted_at,
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentSessionLineageDiagnosticRecordWire {
    code: String,
    message: String,
}

fn parse_lineage_boundary_kind(value: &str) -> AgentSessionLineageBoundaryKind {
    match value {
        "message" => AgentSessionLineageBoundaryKind::Message,
        "checkpoint" => AgentSessionLineageBoundaryKind::Checkpoint,
        _ => AgentSessionLineageBoundaryKind::Run,
    }
}

fn lineage_boundary_kind_sql_value(kind: &AgentSessionLineageBoundaryKind) -> &'static str {
    match kind {
        AgentSessionLineageBoundaryKind::Run => "run",
        AgentSessionLineageBoundaryKind::Message => "message",
        AgentSessionLineageBoundaryKind::Checkpoint => "checkpoint",
    }
}

fn compaction_trigger_sql_value(trigger: &AgentCompactionTrigger) -> &'static str {
    match trigger {
        AgentCompactionTrigger::Manual => "manual",
        AgentCompactionTrigger::Auto => "auto",
    }
}

fn diagnostic_json(
    diagnostic: Option<&AgentRunDiagnosticRecord>,
) -> Result<Option<String>, CommandError> {
    diagnostic
        .map(|diagnostic| {
            serde_json::to_string(&json!({
                "code": diagnostic.code,
                "message": diagnostic.message,
            }))
            .map_err(|error| {
                CommandError::system_fault(
                    "agent_session_branch_diagnostic_serialize_failed",
                    format!("Xero could not serialize branch diagnostic metadata: {error}"),
                )
            })
        })
        .transpose()
}

fn file_change_summary(
    source_snapshot: &AgentRunSnapshotRecord,
    boundary: &ResolvedBranchBoundary,
) -> String {
    let changes = source_snapshot
        .file_changes
        .iter()
        .filter(|change| includes_created_at(change.created_at.as_str(), boundary))
        .count();
    let checkpoints = source_snapshot
        .checkpoints
        .iter()
        .filter(|checkpoint| {
            includes_checkpoint(checkpoint.id, checkpoint.created_at.as_str(), boundary)
        })
        .count();
    if changes == 0 && checkpoints == 0 {
        return "No file-change or checkpoint metadata was before the branch point.".into();
    }
    format!(
        "{changes} file-change record(s) and {checkpoints} checkpoint(s) were before the branch point. Branching does not roll files back automatically."
    )
}

fn default_branch_title(source_title: &str, boundary: &ResolvedBranchBoundary) -> String {
    match boundary.kind {
        AgentSessionLineageBoundaryKind::Run => format!("Branch of {source_title}"),
        AgentSessionLineageBoundaryKind::Message | AgentSessionLineageBoundaryKind::Checkpoint => {
            format!("Rewind of {source_title}")
        }
    }
}

fn branch_summary(source_title: &str, boundary: &ResolvedBranchBoundary) -> String {
    match boundary.kind {
        AgentSessionLineageBoundaryKind::Run => {
            format!("Branched from `{source_title}` without mutating the original session.")
        }
        AgentSessionLineageBoundaryKind::Message => format!(
            "Rewound from `{source_title}` at message boundary {}. File rollback remains manual.",
            boundary.source_message_id.unwrap_or_default()
        ),
        AgentSessionLineageBoundaryKind::Checkpoint => format!(
            "Rewound from `{source_title}` at checkpoint boundary {}. File rollback remains manual.",
            boundary.source_checkpoint_id.unwrap_or_default()
        ),
    }
}

fn generate_agent_session_lineage_id() -> String {
    format!("agent-lineage-{}", random_hex_suffix())
}

fn generate_branch_replay_run_id(source_run_id: &str) -> String {
    let sanitized = source_run_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("branch-{}-{}", sanitized, random_hex_suffix())
}

fn random_hex_suffix() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn map_lineage_write_error(
    database_path: &Path,
    code: &'static str,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::system_fault(
        code,
        format!(
            "Xero could not persist agent-session branch state in {}: {error}",
            database_path.display()
        ),
    )
}
