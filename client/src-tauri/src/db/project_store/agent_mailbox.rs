use std::{collections::BTreeSet, path::Path};

use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::database_path_for_repo,
};

use super::{
    find_prohibited_runtime_persistence_content, insert_project_record, load_agent_run,
    open_runtime_database, validate_non_empty_text, NewProjectRecordRecord,
    ProjectRecordImportance, ProjectRecordKind, ProjectRecordRedactionState,
    ProjectRecordVisibility,
};

const DEFAULT_MAILBOX_TTL_SECONDS: i64 = 3_600;
const MAX_MAILBOX_CONTEXT_ITEMS: usize = 8;
const MAX_MAILBOX_TITLE_CHARS: usize = 240;
const MAX_MAILBOX_BODY_CHARS: usize = 4_000;
const MAILBOX_PROMOTION_SCHEMA: &str = "xero.agent_mailbox.promoted_candidate.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMailboxItemType {
    HeadsUp,
    Question,
    Answer,
    Blocker,
    FileOwnershipNote,
    FindingInProgress,
    VerificationNote,
    HandoffLiteSummary,
    HistoryRewriteNotice,
    UndoConflictNotice,
    WorkspaceEpochAdvanced,
    ReservationInvalidated,
}

impl AgentMailboxItemType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HeadsUp => "heads_up",
            Self::Question => "question",
            Self::Answer => "answer",
            Self::Blocker => "blocker",
            Self::FileOwnershipNote => "file_ownership_note",
            Self::FindingInProgress => "finding_in_progress",
            Self::VerificationNote => "verification_note",
            Self::HandoffLiteSummary => "handoff_lite_summary",
            Self::HistoryRewriteNotice => "history_rewrite_notice",
            Self::UndoConflictNotice => "undo_conflict_notice",
            Self::WorkspaceEpochAdvanced => "workspace_epoch_advanced",
            Self::ReservationInvalidated => "reservation_invalidated",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMailboxPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl AgentMailboxPriority {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMailboxStatus {
    Open,
    Resolved,
    Promoted,
}

impl AgentMailboxStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
            Self::Promoted => "promoted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentMailboxItemRecord {
    pub item_id: String,
    pub project_id: String,
    pub item_type: AgentMailboxItemType,
    pub parent_item_id: Option<String>,
    pub sender_agent_session_id: String,
    pub sender_run_id: String,
    pub sender_parent_run_id: Option<String>,
    pub sender_child_run_id: Option<String>,
    pub sender_role: Option<String>,
    pub sender_trace_id: String,
    pub target_agent_session_id: Option<String>,
    pub target_run_id: Option<String>,
    pub target_role: Option<String>,
    pub title: String,
    pub body: String,
    pub related_paths: Vec<String>,
    pub priority: AgentMailboxPriority,
    pub status: AgentMailboxStatus,
    pub created_at: String,
    pub expires_at: String,
    pub resolved_at: Option<String>,
    pub resolved_by_run_id: Option<String>,
    pub resolve_reason: Option<String>,
    pub promoted_record_id: Option<String>,
    pub promoted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentMailboxDeliveryRecord {
    pub item: AgentMailboxItemRecord,
    pub acknowledged_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentMailboxItemRecord {
    pub project_id: String,
    pub sender_run_id: String,
    pub item_type: AgentMailboxItemType,
    pub parent_item_id: Option<String>,
    pub target_agent_session_id: Option<String>,
    pub target_run_id: Option<String>,
    pub target_role: Option<String>,
    pub title: String,
    pub body: String,
    pub related_paths: Vec<String>,
    pub priority: AgentMailboxPriority,
    pub created_at: String,
    pub ttl_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyAgentMailboxItemRecord {
    pub project_id: String,
    pub sender_run_id: String,
    pub parent_item_id: String,
    pub item_type: Option<AgentMailboxItemType>,
    pub title: Option<String>,
    pub body: String,
    pub related_paths: Vec<String>,
    pub priority: Option<AgentMailboxPriority>,
    pub created_at: String,
    pub ttl_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveAgentMailboxItemRecord {
    pub project_id: String,
    pub resolver_run_id: String,
    pub item_id: String,
    pub resolve_reason: String,
    pub resolved_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromoteAgentMailboxItemRecord {
    pub project_id: String,
    pub promoter_run_id: String,
    pub item_id: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub promoted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentMailboxPromotionRecord {
    pub item: AgentMailboxItemRecord,
    pub promoted_record_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentMailboxActor {
    agent_session_id: String,
    run_id: String,
    trace_id: String,
    lineage_kind: String,
    parent_run_id: Option<String>,
    parent_subagent_id: Option<String>,
    role: Option<String>,
}

pub fn publish_agent_mailbox_item(
    repo_root: &Path,
    record: &NewAgentMailboxItemRecord,
) -> CommandResult<AgentMailboxItemRecord> {
    validate_mailbox_publish(record)?;
    let title = normalize_mailbox_title(&record.title)?;
    let body = normalize_mailbox_body(&record.body)?;
    let related_paths = normalize_mailbox_paths(&record.related_paths)?;
    let expires_at = timestamp_plus_seconds(
        &record.created_at,
        record.ttl_seconds.unwrap_or(DEFAULT_MAILBOX_TTL_SECONDS),
    );
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let item_id = generate_mailbox_item_id();
    let inserted = connection
        .execute(
            r#"
            INSERT INTO agent_mailbox_items (
                item_id,
                project_id,
                item_type,
                parent_item_id,
                sender_agent_session_id,
                sender_run_id,
                sender_parent_run_id,
                sender_child_run_id,
                sender_role,
                sender_trace_id,
                target_agent_session_id,
                target_run_id,
                target_role,
                title,
                body,
                related_paths_json,
                priority,
                status,
                created_at,
                expires_at
            )
            SELECT
                ?1,
                ?2,
                ?3,
                ?4,
                agent_runs.agent_session_id,
                agent_runs.run_id,
                agent_runs.parent_run_id,
                CASE
                    WHEN agent_runs.lineage_kind = 'subagent_child'
                    THEN agent_runs.run_id
                    ELSE NULL
                END,
                COALESCE(agent_runs.subagent_role, agent_runs.runtime_agent_id),
                agent_runs.trace_id,
                ?5,
                ?6,
                ?7,
                ?8,
                ?9,
                ?10,
                ?11,
                'open',
                ?12,
                ?13
            FROM agent_runs
            WHERE agent_runs.project_id = ?2
              AND agent_runs.run_id = ?14
            "#,
            params![
                item_id,
                record.project_id,
                record.item_type.as_str(),
                record.parent_item_id.as_deref(),
                record.target_agent_session_id.as_deref(),
                record.target_run_id.as_deref(),
                record.target_role.as_deref(),
                title,
                body,
                json_string_array(&related_paths, "relatedPaths")?,
                record.priority.as_str(),
                record.created_at,
                expires_at,
                record.sender_run_id,
            ],
        )
        .map_err(|error| {
            map_mailbox_write_error(&database_path, "agent_mailbox_item_insert_failed", error)
        })?;
    if inserted == 0 {
        return Err(CommandError::system_fault(
            "agent_mailbox_run_missing",
            format!(
                "Xero could not publish mailbox item for missing run `{}` in project `{}`.",
                record.sender_run_id, record.project_id
            ),
        ));
    }
    read_agent_mailbox_item(&connection, repo_root, &record.project_id, &item_id)
}

pub fn reply_agent_mailbox_item(
    repo_root: &Path,
    record: &ReplyAgentMailboxItemRecord,
) -> CommandResult<AgentMailboxItemRecord> {
    validate_non_empty_text(
        &record.parent_item_id,
        "parentItemId",
        "agent_mailbox_reply_invalid",
    )?;
    let parent = get_agent_mailbox_item(
        repo_root,
        &record.project_id,
        &record.parent_item_id,
        &record.created_at,
    )?;
    let title = record
        .title
        .clone()
        .unwrap_or_else(|| format!("Re: {}", parent.title));
    let mut related_paths = record.related_paths.clone();
    if related_paths.is_empty() {
        related_paths = parent.related_paths.clone();
    }
    publish_agent_mailbox_item(
        repo_root,
        &NewAgentMailboxItemRecord {
            project_id: record.project_id.clone(),
            sender_run_id: record.sender_run_id.clone(),
            item_type: record.item_type.unwrap_or(AgentMailboxItemType::Answer),
            parent_item_id: Some(parent.item_id),
            target_agent_session_id: Some(parent.sender_agent_session_id),
            target_run_id: Some(parent.sender_run_id),
            target_role: parent.sender_role,
            title,
            body: record.body.clone(),
            related_paths,
            priority: record.priority.unwrap_or(parent.priority),
            created_at: record.created_at.clone(),
            ttl_seconds: record.ttl_seconds,
        },
    )
}

pub fn list_agent_mailbox_inbox(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    now: &str,
    limit: usize,
) -> CommandResult<Vec<AgentMailboxDeliveryRecord>> {
    validate_non_empty_text(project_id, "projectId", "agent_mailbox_request_invalid")?;
    validate_non_empty_text(run_id, "runId", "agent_mailbox_request_invalid")?;
    cleanup_expired_agent_mailbox(repo_root, project_id, now)?;
    let limit = limit.clamp(1, MAX_MAILBOX_CONTEXT_ITEMS);
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let Some(actor) = mailbox_actor_for_run(&connection, repo_root, project_id, run_id)? else {
        return Ok(Vec::new());
    };
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                agent_mailbox_items.item_id,
                agent_mailbox_items.project_id,
                agent_mailbox_items.item_type,
                agent_mailbox_items.parent_item_id,
                agent_mailbox_items.sender_agent_session_id,
                agent_mailbox_items.sender_run_id,
                agent_mailbox_items.sender_parent_run_id,
                agent_mailbox_items.sender_child_run_id,
                agent_mailbox_items.sender_role,
                agent_mailbox_items.sender_trace_id,
                agent_mailbox_items.target_agent_session_id,
                agent_mailbox_items.target_run_id,
                agent_mailbox_items.target_role,
                agent_mailbox_items.title,
                agent_mailbox_items.body,
                agent_mailbox_items.related_paths_json,
                agent_mailbox_items.priority,
                agent_mailbox_items.status,
                agent_mailbox_items.created_at,
                agent_mailbox_items.expires_at,
                agent_mailbox_items.resolved_at,
                agent_mailbox_items.resolved_by_run_id,
                agent_mailbox_items.resolve_reason,
                agent_mailbox_items.promoted_record_id,
                agent_mailbox_items.promoted_at,
                acknowledgements.acknowledged_at
            FROM agent_mailbox_items
            LEFT JOIN agent_mailbox_acknowledgements AS acknowledgements
              ON acknowledgements.project_id = agent_mailbox_items.project_id
             AND acknowledgements.item_id = agent_mailbox_items.item_id
             AND acknowledgements.run_id = ?3
            WHERE agent_mailbox_items.project_id = ?1
              AND agent_mailbox_items.expires_at > ?2
              AND agent_mailbox_items.status = 'open'
              AND agent_mailbox_items.sender_run_id <> ?3
              AND acknowledgements.run_id IS NULL
              AND (agent_mailbox_items.target_run_id IS NULL OR agent_mailbox_items.target_run_id = ?3)
              AND (agent_mailbox_items.target_agent_session_id IS NULL OR agent_mailbox_items.target_agent_session_id = ?4)
              AND (agent_mailbox_items.target_role IS NULL OR agent_mailbox_items.target_role = ?5)
            ORDER BY
                CASE agent_mailbox_items.priority
                    WHEN 'urgent' THEN 0
                    WHEN 'high' THEN 1
                    WHEN 'normal' THEN 2
                    ELSE 3
                END,
                agent_mailbox_items.created_at DESC,
                agent_mailbox_items.item_id ASC
            LIMIT ?6
            "#,
        )
        .map_err(|error| {
            map_mailbox_query_error(&database_path, "agent_mailbox_inbox_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(
            params![
                project_id,
                now,
                run_id,
                actor.agent_session_id,
                actor.role.as_deref(),
                limit as i64,
            ],
            read_delivery_row,
        )
        .map_err(|error| {
            map_mailbox_query_error(&database_path, "agent_mailbox_inbox_query_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_mailbox_query_error(&database_path, "agent_mailbox_inbox_decode_failed", error)
    })
}

pub fn acknowledge_agent_mailbox_item(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    item_id: &str,
    acknowledged_at: &str,
) -> CommandResult<AgentMailboxItemRecord> {
    validate_non_empty_text(project_id, "projectId", "agent_mailbox_ack_invalid")?;
    validate_non_empty_text(run_id, "runId", "agent_mailbox_ack_invalid")?;
    validate_non_empty_text(item_id, "itemId", "agent_mailbox_ack_invalid")?;
    let item = get_agent_mailbox_item(repo_root, project_id, item_id, acknowledged_at)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let actor = require_mailbox_actor_for_run(&connection, repo_root, project_id, run_id)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_mailbox_acknowledgements (
                project_id,
                item_id,
                agent_session_id,
                run_id,
                acknowledged_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(project_id, item_id, run_id) DO UPDATE SET
                acknowledged_at = excluded.acknowledged_at
            "#,
            params![
                project_id,
                item_id,
                actor.agent_session_id,
                actor.run_id,
                acknowledged_at,
            ],
        )
        .map_err(|error| {
            map_mailbox_write_error(&database_path, "agent_mailbox_ack_insert_failed", error)
        })?;
    Ok(item)
}

pub fn resolve_agent_mailbox_item(
    repo_root: &Path,
    record: &ResolveAgentMailboxItemRecord,
) -> CommandResult<AgentMailboxItemRecord> {
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_mailbox_resolve_invalid",
    )?;
    validate_non_empty_text(
        &record.resolver_run_id,
        "resolverRunId",
        "agent_mailbox_resolve_invalid",
    )?;
    validate_non_empty_text(&record.item_id, "itemId", "agent_mailbox_resolve_invalid")?;
    validate_non_empty_text(
        &record.resolve_reason,
        "resolveReason",
        "agent_mailbox_resolve_invalid",
    )?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    require_mailbox_actor_for_run(
        &connection,
        repo_root,
        &record.project_id,
        &record.resolver_run_id,
    )?;
    let updated = connection
        .execute(
            r#"
            UPDATE agent_mailbox_items
            SET status = 'resolved',
                resolved_at = ?4,
                resolved_by_run_id = ?3,
                resolve_reason = ?5
            WHERE project_id = ?1
              AND item_id = ?2
              AND expires_at > ?4
              AND status = 'open'
            "#,
            params![
                record.project_id,
                record.item_id,
                record.resolver_run_id,
                record.resolved_at,
                record.resolve_reason,
            ],
        )
        .map_err(|error| {
            map_mailbox_write_error(&database_path, "agent_mailbox_resolve_failed", error)
        })?;
    if updated == 0 {
        return Err(mailbox_item_missing_error(
            &record.project_id,
            &record.item_id,
        ));
    }
    read_agent_mailbox_item(&connection, repo_root, &record.project_id, &record.item_id)
}

pub fn promote_agent_mailbox_item(
    repo_root: &Path,
    record: &PromoteAgentMailboxItemRecord,
) -> CommandResult<AgentMailboxPromotionRecord> {
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_mailbox_promote_invalid",
    )?;
    validate_non_empty_text(
        &record.promoter_run_id,
        "promoterRunId",
        "agent_mailbox_promote_invalid",
    )?;
    validate_non_empty_text(&record.item_id, "itemId", "agent_mailbox_promote_invalid")?;
    let item = get_agent_mailbox_item(
        repo_root,
        &record.project_id,
        &record.item_id,
        &record.promoted_at,
    )?;
    let snapshot = load_agent_run(repo_root, &record.project_id, &record.promoter_run_id)?;
    let title = normalize_mailbox_title(record.title.as_deref().unwrap_or(item.title.as_str()))?;
    let summary =
        normalize_mailbox_title(record.summary.as_deref().unwrap_or(item.title.as_str()))?;
    let text = normalize_mailbox_body(&format!(
        "Temporary mailbox {kind} from {sender}: {body}",
        kind = item.item_type.as_str(),
        sender = item.sender_role.as_deref().unwrap_or("agent"),
        body = item.body
    ))?;
    let mut source_item_ids = vec![
        format!("agent_mailbox_items:{}", item.item_id),
        format!("agent_runs:{}", item.sender_run_id),
    ];
    if record.promoter_run_id != item.sender_run_id {
        source_item_ids.push(format!("agent_runs:{}", record.promoter_run_id));
    }
    let promoted_record = insert_project_record(
        repo_root,
        &NewProjectRecordRecord {
            record_id: super::generate_project_record_id(),
            project_id: record.project_id.clone(),
            record_kind: project_record_kind_for_mailbox(item.item_type),
            runtime_agent_id: snapshot.run.runtime_agent_id,
            agent_definition_id: snapshot.run.agent_definition_id,
            agent_definition_version: snapshot.run.agent_definition_version,
            agent_session_id: Some(snapshot.run.agent_session_id),
            run_id: record.promoter_run_id.clone(),
            workflow_run_id: None,
            workflow_step_id: None,
            title,
            summary,
            text,
            content_json: Some(mailbox_promotion_content_json(
                &item,
                &record.promoter_run_id,
            )),
            schema_name: Some(MAILBOX_PROMOTION_SCHEMA.into()),
            schema_version: 1,
            importance: project_record_importance_for_mailbox(item.priority),
            confidence: Some(0.8),
            tags: vec![
                "swarm-mailbox".into(),
                "memory-candidate".into(),
                item.item_type.as_str().into(),
            ],
            source_item_ids,
            related_paths: item.related_paths.clone(),
            produced_artifact_refs: Vec::new(),
            redaction_state: ProjectRecordRedactionState::Clean,
            visibility: ProjectRecordVisibility::MemoryCandidate,
            created_at: record.promoted_at.clone(),
        },
    )?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .execute(
            r#"
            UPDATE agent_mailbox_items
            SET status = 'promoted',
                promoted_record_id = ?4,
                promoted_at = ?5
            WHERE project_id = ?1
              AND item_id = ?2
              AND expires_at > ?3
            "#,
            params![
                record.project_id,
                record.item_id,
                record.promoted_at,
                promoted_record.record_id,
                record.promoted_at,
            ],
        )
        .map_err(|error| {
            map_mailbox_write_error(&database_path, "agent_mailbox_promote_mark_failed", error)
        })?;
    let item =
        read_agent_mailbox_item(&connection, repo_root, &record.project_id, &record.item_id)?;
    Ok(AgentMailboxPromotionRecord {
        item,
        promoted_record_id: promoted_record.record_id,
    })
}

pub fn get_agent_mailbox_item(
    repo_root: &Path,
    project_id: &str,
    item_id: &str,
    now: &str,
) -> CommandResult<AgentMailboxItemRecord> {
    validate_non_empty_text(project_id, "projectId", "agent_mailbox_request_invalid")?;
    validate_non_empty_text(item_id, "itemId", "agent_mailbox_request_invalid")?;
    cleanup_expired_agent_mailbox(repo_root, project_id, now)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_agent_mailbox_item(&connection, repo_root, project_id, item_id)
}

pub fn active_agent_mailbox_context(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    now: &str,
) -> CommandResult<Vec<AgentMailboxDeliveryRecord>> {
    list_agent_mailbox_inbox(
        repo_root,
        project_id,
        run_id,
        now,
        MAX_MAILBOX_CONTEXT_ITEMS,
    )
}

pub fn cleanup_expired_agent_mailbox(
    repo_root: &Path,
    project_id: &str,
    now: &str,
) -> CommandResult<()> {
    validate_non_empty_text(project_id, "projectId", "agent_mailbox_gc_invalid")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .execute(
            r#"
            DELETE FROM agent_mailbox_items
            WHERE project_id = ?1
              AND expires_at <= ?2
            "#,
            params![project_id, now],
        )
        .map_err(|error| {
            map_mailbox_write_error(&database_path, "agent_mailbox_items_gc_failed", error)
        })?;
    Ok(())
}

pub fn clear_project_agent_mailbox(repo_root: &Path, project_id: &str) -> CommandResult<usize> {
    validate_non_empty_text(project_id, "projectId", "agent_mailbox_clear_invalid")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .execute(
            "DELETE FROM agent_mailbox_items WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(|error| {
            map_mailbox_write_error(&database_path, "agent_mailbox_clear_failed", error)
        })
}

fn validate_mailbox_publish(record: &NewAgentMailboxItemRecord) -> CommandResult<()> {
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_mailbox_publish_invalid",
    )?;
    validate_non_empty_text(
        &record.sender_run_id,
        "senderRunId",
        "agent_mailbox_publish_invalid",
    )?;
    validate_non_empty_text(&record.title, "title", "agent_mailbox_publish_invalid")?;
    validate_non_empty_text(&record.body, "body", "agent_mailbox_publish_invalid")?;
    validate_non_empty_text(
        &record.created_at,
        "createdAt",
        "agent_mailbox_publish_invalid",
    )?;
    validate_optional_non_empty(
        record.parent_item_id.as_deref(),
        "parentItemId",
        "agent_mailbox_publish_invalid",
    )?;
    validate_optional_non_empty(
        record.target_agent_session_id.as_deref(),
        "targetAgentSessionId",
        "agent_mailbox_publish_invalid",
    )?;
    validate_optional_non_empty(
        record.target_run_id.as_deref(),
        "targetRunId",
        "agent_mailbox_publish_invalid",
    )?;
    validate_optional_non_empty(
        record.target_role.as_deref(),
        "targetRole",
        "agent_mailbox_publish_invalid",
    )
}

fn validate_optional_non_empty(value: Option<&str>, field: &str, code: &str) -> CommandResult<()> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err(CommandError::user_fixable(
            code,
            format!("Mailbox field `{field}` cannot be blank."),
        ));
    }
    Ok(())
}

fn normalize_mailbox_title(title: &str) -> CommandResult<String> {
    let title = normalize_mailbox_text(title, "title", MAX_MAILBOX_TITLE_CHARS)?;
    if title.contains('\n') || title.contains('\r') {
        return Err(CommandError::user_fixable(
            "agent_mailbox_title_invalid",
            "Mailbox titles must fit on one line.",
        ));
    }
    Ok(title)
}

fn normalize_mailbox_body(body: &str) -> CommandResult<String> {
    normalize_mailbox_text(body, "body", MAX_MAILBOX_BODY_CHARS)
}

fn normalize_mailbox_text(
    value: &str,
    field: &'static str,
    max_chars: usize,
) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    if let Some(reason) = find_prohibited_runtime_persistence_content(trimmed) {
        return Err(CommandError::user_fixable(
            "agent_mailbox_content_rejected",
            format!("Xero refused to store mailbox {field} because it looked like {reason}."),
        ));
    }
    let normalized = trimmed.to_ascii_lowercase();
    for phrase in [
        "ignore previous instructions",
        "ignore all previous instructions",
        "disregard previous instructions",
        "override your instructions",
        "developer message says",
        "system prompt says",
    ] {
        if normalized.contains(phrase) {
            return Err(CommandError::user_fixable(
                "agent_mailbox_content_rejected",
                "Xero refused to store mailbox content that looked like an instruction-override attempt.",
            ));
        }
    }
    Ok(truncate_chars(trimmed, max_chars))
}

fn normalize_mailbox_paths(paths: &[String]) -> CommandResult<Vec<String>> {
    let mut normalized = BTreeSet::new();
    for path in paths {
        let path = normalize_mailbox_path(path)?;
        normalized.insert(path);
    }
    Ok(normalized.into_iter().collect())
}

fn normalize_mailbox_path(path: &str) -> CommandResult<String> {
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
            "agent_mailbox_related_path_invalid",
            format!("Xero refused the unsafe mailbox related path `{path}`."),
        ));
    }
    let mut parts = Vec::new();
    for part in trimmed.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                return Err(CommandError::user_fixable(
                    "agent_mailbox_related_path_invalid",
                    format!("Xero refused the unsafe mailbox related path `{path}`."),
                ));
            }
            value => parts.push(value),
        }
    }
    if parts.is_empty() {
        return Err(CommandError::user_fixable(
            "agent_mailbox_related_path_invalid",
            format!("Xero refused the unsafe mailbox related path `{path}`."),
        ));
    }
    Ok(parts.join("/"))
}

fn mailbox_actor_for_run(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> CommandResult<Option<AgentMailboxActor>> {
    let database_path = database_path_for_repo(repo_root);
    connection
        .query_row(
            r#"
            SELECT
                agent_session_id,
                run_id,
                trace_id,
                lineage_kind,
                parent_run_id,
                parent_subagent_id,
                COALESCE(subagent_role, runtime_agent_id)
            FROM agent_runs
            WHERE project_id = ?1
              AND run_id = ?2
            "#,
            params![project_id, run_id],
            |row| {
                Ok(AgentMailboxActor {
                    agent_session_id: row.get(0)?,
                    run_id: row.get(1)?,
                    trace_id: row.get(2)?,
                    lineage_kind: row.get(3)?,
                    parent_run_id: row.get(4)?,
                    parent_subagent_id: row.get(5)?,
                    role: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            map_mailbox_query_error(&database_path, "agent_mailbox_actor_read_failed", error)
        })
}

fn require_mailbox_actor_for_run(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> CommandResult<AgentMailboxActor> {
    mailbox_actor_for_run(connection, repo_root, project_id, run_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "agent_mailbox_run_missing",
            format!(
                "Xero could not update mailbox state for missing run `{run_id}` in project `{project_id}`."
            ),
        )
    })
}

fn read_agent_mailbox_item(
    connection: &Connection,
    _repo_root: &Path,
    project_id: &str,
    item_id: &str,
) -> CommandResult<AgentMailboxItemRecord> {
    connection
        .query_row(
            r#"
            SELECT
                item_id,
                project_id,
                item_type,
                parent_item_id,
                sender_agent_session_id,
                sender_run_id,
                sender_parent_run_id,
                sender_child_run_id,
                sender_role,
                sender_trace_id,
                target_agent_session_id,
                target_run_id,
                target_role,
                title,
                body,
                related_paths_json,
                priority,
                status,
                created_at,
                expires_at,
                resolved_at,
                resolved_by_run_id,
                resolve_reason,
                promoted_record_id,
                promoted_at
            FROM agent_mailbox_items
            WHERE project_id = ?1
              AND item_id = ?2
            "#,
            params![project_id, item_id],
            read_mailbox_item_row,
        )
        .map_err(|_| mailbox_item_missing_error(project_id, item_id))
}

fn read_mailbox_item_row(row: &Row<'_>) -> rusqlite::Result<AgentMailboxItemRecord> {
    let item_type: String = row.get(2)?;
    let related_paths_json: String = row.get(15)?;
    let related_paths = serde_json::from_str(&related_paths_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(15, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let priority: String = row.get(16)?;
    let status: String = row.get(17)?;
    Ok(AgentMailboxItemRecord {
        item_id: row.get(0)?,
        project_id: row.get(1)?,
        item_type: parse_mailbox_item_type(&item_type),
        parent_item_id: row.get(3)?,
        sender_agent_session_id: row.get(4)?,
        sender_run_id: row.get(5)?,
        sender_parent_run_id: row.get(6)?,
        sender_child_run_id: row.get(7)?,
        sender_role: row.get(8)?,
        sender_trace_id: row.get(9)?,
        target_agent_session_id: row.get(10)?,
        target_run_id: row.get(11)?,
        target_role: row.get(12)?,
        title: row.get(13)?,
        body: row.get(14)?,
        related_paths,
        priority: parse_mailbox_priority(&priority),
        status: parse_mailbox_status(&status),
        created_at: row.get(18)?,
        expires_at: row.get(19)?,
        resolved_at: row.get(20)?,
        resolved_by_run_id: row.get(21)?,
        resolve_reason: row.get(22)?,
        promoted_record_id: row.get(23)?,
        promoted_at: row.get(24)?,
    })
}

fn read_delivery_row(row: &Row<'_>) -> rusqlite::Result<AgentMailboxDeliveryRecord> {
    Ok(AgentMailboxDeliveryRecord {
        item: read_mailbox_item_row(row)?,
        acknowledged_at: row.get(25)?,
    })
}

fn parse_mailbox_item_type(value: &str) -> AgentMailboxItemType {
    match value {
        "question" => AgentMailboxItemType::Question,
        "answer" => AgentMailboxItemType::Answer,
        "blocker" => AgentMailboxItemType::Blocker,
        "file_ownership_note" => AgentMailboxItemType::FileOwnershipNote,
        "finding_in_progress" => AgentMailboxItemType::FindingInProgress,
        "verification_note" => AgentMailboxItemType::VerificationNote,
        "handoff_lite_summary" => AgentMailboxItemType::HandoffLiteSummary,
        "history_rewrite_notice" => AgentMailboxItemType::HistoryRewriteNotice,
        "undo_conflict_notice" => AgentMailboxItemType::UndoConflictNotice,
        "workspace_epoch_advanced" => AgentMailboxItemType::WorkspaceEpochAdvanced,
        "reservation_invalidated" => AgentMailboxItemType::ReservationInvalidated,
        _ => AgentMailboxItemType::HeadsUp,
    }
}

fn parse_mailbox_priority(value: &str) -> AgentMailboxPriority {
    match value {
        "low" => AgentMailboxPriority::Low,
        "high" => AgentMailboxPriority::High,
        "urgent" => AgentMailboxPriority::Urgent,
        _ => AgentMailboxPriority::Normal,
    }
}

fn parse_mailbox_status(value: &str) -> AgentMailboxStatus {
    match value {
        "resolved" => AgentMailboxStatus::Resolved,
        "promoted" => AgentMailboxStatus::Promoted,
        _ => AgentMailboxStatus::Open,
    }
}

fn project_record_kind_for_mailbox(item_type: AgentMailboxItemType) -> ProjectRecordKind {
    match item_type {
        AgentMailboxItemType::Question => ProjectRecordKind::Question,
        AgentMailboxItemType::Blocker => ProjectRecordKind::Diagnostic,
        AgentMailboxItemType::FindingInProgress => ProjectRecordKind::Finding,
        AgentMailboxItemType::VerificationNote => ProjectRecordKind::Verification,
        AgentMailboxItemType::HandoffLiteSummary => ProjectRecordKind::AgentHandoff,
        AgentMailboxItemType::FileOwnershipNote => ProjectRecordKind::ContextNote,
        AgentMailboxItemType::UndoConflictNotice => ProjectRecordKind::Diagnostic,
        AgentMailboxItemType::HeadsUp
        | AgentMailboxItemType::Answer
        | AgentMailboxItemType::HistoryRewriteNotice
        | AgentMailboxItemType::WorkspaceEpochAdvanced
        | AgentMailboxItemType::ReservationInvalidated => ProjectRecordKind::ContextNote,
    }
}

fn project_record_importance_for_mailbox(
    priority: AgentMailboxPriority,
) -> ProjectRecordImportance {
    match priority {
        AgentMailboxPriority::Low => ProjectRecordImportance::Low,
        AgentMailboxPriority::Normal => ProjectRecordImportance::Normal,
        AgentMailboxPriority::High => ProjectRecordImportance::High,
        AgentMailboxPriority::Urgent => ProjectRecordImportance::Critical,
    }
}

fn mailbox_promotion_content_json(
    item: &AgentMailboxItemRecord,
    promoter_run_id: &str,
) -> JsonValue {
    json!({
        "schema": MAILBOX_PROMOTION_SCHEMA,
        "itemId": item.item_id,
        "itemType": item.item_type.as_str(),
        "priority": item.priority.as_str(),
        "senderRunId": item.sender_run_id,
        "senderAgentSessionId": item.sender_agent_session_id,
        "senderChildRunId": item.sender_child_run_id,
        "senderRole": item.sender_role,
        "promoterRunId": promoter_run_id,
        "targetAgentSessionId": item.target_agent_session_id,
        "targetRunId": item.target_run_id,
        "targetRole": item.target_role,
        "relatedPaths": item.related_paths,
        "temporaryMailbox": true,
        "approvedMemoryAutomatically": false,
    })
}

fn json_string_array(values: &[String], field: &'static str) -> CommandResult<String> {
    serde_json::to_string(values).map_err(|error| {
        CommandError::system_fault(
            "agent_mailbox_json_encode_failed",
            format!("Xero could not encode {field} mailbox JSON: {error}"),
        )
    })
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn timestamp_plus_seconds(timestamp: &str, seconds: i64) -> String {
    let base = OffsetDateTime::parse(timestamp, &Rfc3339)
        .unwrap_or_else(|_| OffsetDateTime::parse(&now_timestamp(), &Rfc3339).unwrap());
    (base + Duration::seconds(seconds.max(1)))
        .format(&Rfc3339)
        .expect("rfc3339 timestamp formatting should succeed")
}

fn generate_mailbox_item_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "mailbox-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn mailbox_item_missing_error(project_id: &str, item_id: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_mailbox_item_not_found",
        format!("Xero could not find active mailbox item `{item_id}` for project `{project_id}`."),
    )
}

fn map_mailbox_query_error(
    database_path: &Path,
    code: &'static str,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::retryable(
        code,
        format!(
            "Xero could not query agent mailbox state in {}: {error}",
            database_path.display()
        ),
    )
}

fn map_mailbox_write_error(
    database_path: &Path,
    code: &'static str,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::retryable(
        code,
        format!(
            "Xero could not update agent mailbox state in {}: {error}",
            database_path.display()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use rusqlite::{params, Connection};

    use crate::{
        commands::RuntimeAgentIdDto,
        db::{
            configure_connection,
            migrations::migrations,
            project_store::{
                create_agent_session, insert_agent_run, list_project_records, project_record_lance,
                AgentSessionCreateRecord, AgentSessionRecord, NewAgentRunRecord,
            },
        },
    };

    fn create_project_database(repo_root: &Path, project_id: &str) {
        let database_path = repo_root
            .parent()
            .expect("repo parent")
            .join("app-data")
            .join("projects")
            .join(project_id)
            .join("state.db");
        fs::create_dir_all(database_path.parent().expect("database parent")).expect("database dir");
        let mut connection = Connection::open(&database_path).expect("open database");
        configure_connection(&connection).expect("configure database");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate database");
        connection
            .execute(
                "INSERT INTO projects (id, name, description, milestone) VALUES (?1, 'Project', '', '')",
                params![project_id],
            )
            .expect("insert project");
        connection
            .execute(
                r#"
                INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo)
                VALUES ('repo-1', ?1, ?2, 'Project', 'main', 'abc123', 1)
                "#,
                params![project_id, repo_root.to_string_lossy().as_ref()],
            )
            .expect("insert repository");
        crate::db::register_project_database_path(repo_root, &database_path);
    }

    fn seed_agent_run(
        repo_root: &Path,
        project_id: &str,
        run_id: &str,
        title: &str,
    ) -> AgentSessionRecord {
        let session = create_agent_session(
            repo_root,
            &AgentSessionCreateRecord {
                project_id: project_id.into(),
                title: title.into(),
                summary: String::new(),
                selected: true,
            },
        )
        .expect("create agent session");
        insert_agent_run(
            repo_root,
            &NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                project_id: project_id.into(),
                agent_session_id: session.agent_session_id.clone(),
                run_id: run_id.into(),
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                prompt: "Coordinate active work.".into(),
                system_prompt: "system".into(),
                now: "2026-05-03T00:00:00Z".into(),
            },
        )
        .expect("insert agent run");
        session
    }

    #[test]
    fn mailbox_publish_read_ack_reply_and_resolve_are_temporary() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(repo_root.join("src")).expect("repo src dir");
        let project_id = "project-mailbox-basic";
        create_project_database(&repo_root, project_id);
        seed_agent_run(&repo_root, project_id, "run-sender", "Sender");
        seed_agent_run(&repo_root, project_id, "run-reader", "Reader");

        let item = publish_agent_mailbox_item(
            &repo_root,
            &NewAgentMailboxItemRecord {
                project_id: project_id.into(),
                sender_run_id: "run-sender".into(),
                item_type: AgentMailboxItemType::Question,
                parent_item_id: None,
                target_agent_session_id: None,
                target_run_id: None,
                target_role: None,
                title: "Can someone verify src/lib.rs?".into(),
                body: "I changed the parser shape and need another active agent to verify tests."
                    .into(),
                related_paths: vec!["src/lib.rs".into()],
                priority: AgentMailboxPriority::High,
                created_at: "2026-05-03T00:00:00Z".into(),
                ttl_seconds: Some(600),
            },
        )
        .expect("publish mailbox item");

        let inbox = list_agent_mailbox_inbox(
            &repo_root,
            project_id,
            "run-reader",
            "2026-05-03T00:01:00Z",
            10,
        )
        .expect("read inbox");
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].item.item_id, item.item_id);
        assert_eq!(inbox[0].item.status, AgentMailboxStatus::Open);

        acknowledge_agent_mailbox_item(
            &repo_root,
            project_id,
            "run-reader",
            &item.item_id,
            "2026-05-03T00:02:00Z",
        )
        .expect("ack item");
        let inbox = list_agent_mailbox_inbox(
            &repo_root,
            project_id,
            "run-reader",
            "2026-05-03T00:03:00Z",
            10,
        )
        .expect("read inbox after ack");
        assert!(inbox.is_empty());

        let reply = reply_agent_mailbox_item(
            &repo_root,
            &ReplyAgentMailboxItemRecord {
                project_id: project_id.into(),
                sender_run_id: "run-reader".into(),
                parent_item_id: item.item_id.clone(),
                item_type: None,
                title: None,
                body: "I can verify after the current reservation clears.".into(),
                related_paths: Vec::new(),
                priority: None,
                created_at: "2026-05-03T00:04:00Z".into(),
                ttl_seconds: Some(600),
            },
        )
        .expect("reply");
        assert_eq!(reply.item_type, AgentMailboxItemType::Answer);
        assert_eq!(reply.parent_item_id.as_deref(), Some(item.item_id.as_str()));

        let sender_inbox = list_agent_mailbox_inbox(
            &repo_root,
            project_id,
            "run-sender",
            "2026-05-03T00:05:00Z",
            10,
        )
        .expect("sender inbox");
        assert_eq!(sender_inbox.len(), 1);
        assert_eq!(sender_inbox[0].item.item_id, reply.item_id);

        let resolved = resolve_agent_mailbox_item(
            &repo_root,
            &ResolveAgentMailboxItemRecord {
                project_id: project_id.into(),
                resolver_run_id: "run-sender".into(),
                item_id: item.item_id.clone(),
                resolve_reason: "Verifier replied.".into(),
                resolved_at: "2026-05-03T00:06:00Z".into(),
            },
        )
        .expect("resolve");
        assert_eq!(resolved.status, AgentMailboxStatus::Resolved);
    }

    #[test]
    fn mailbox_ttl_and_scoped_delivery_limit_visibility() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-mailbox-scope";
        create_project_database(&repo_root, project_id);
        seed_agent_run(&repo_root, project_id, "run-sender", "Sender");
        let target_session = seed_agent_run(&repo_root, project_id, "run-target", "Target");
        seed_agent_run(&repo_root, project_id, "run-other", "Other");

        publish_agent_mailbox_item(
            &repo_root,
            &NewAgentMailboxItemRecord {
                project_id: project_id.into(),
                sender_run_id: "run-sender".into(),
                item_type: AgentMailboxItemType::Blocker,
                parent_item_id: None,
                target_agent_session_id: Some(target_session.agent_session_id),
                target_run_id: None,
                target_role: None,
                title: "Blocked on generated bindings".into(),
                body: "The bindings file is being regenerated; avoid editing it until this clears."
                    .into(),
                related_paths: vec!["src/generated".into()],
                priority: AgentMailboxPriority::Urgent,
                created_at: "2026-05-03T00:00:00Z".into(),
                ttl_seconds: Some(60),
            },
        )
        .expect("publish scoped item");

        assert_eq!(
            list_agent_mailbox_inbox(
                &repo_root,
                project_id,
                "run-target",
                "2026-05-03T00:00:30Z",
                10,
            )
            .expect("target inbox")
            .len(),
            1
        );
        assert!(list_agent_mailbox_inbox(
            &repo_root,
            project_id,
            "run-other",
            "2026-05-03T00:00:30Z",
            10,
        )
        .expect("other inbox")
        .is_empty());
        assert!(list_agent_mailbox_inbox(
            &repo_root,
            project_id,
            "run-target",
            "2026-05-03T00:01:01Z",
            10,
        )
        .expect("expired target inbox")
        .is_empty());
    }

    #[test]
    fn mailbox_inbox_for_missing_run_is_empty_context() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-mailbox-missing-run-context";
        create_project_database(&repo_root, project_id);
        seed_agent_run(&repo_root, project_id, "run-sender", "Sender");

        publish_agent_mailbox_item(
            &repo_root,
            &NewAgentMailboxItemRecord {
                project_id: project_id.into(),
                sender_run_id: "run-sender".into(),
                item_type: AgentMailboxItemType::HeadsUp,
                parent_item_id: None,
                target_agent_session_id: None,
                target_run_id: None,
                target_role: None,
                title: "Shared heads up".into(),
                body: "This should not make missing run context fail.".into(),
                related_paths: Vec::new(),
                priority: AgentMailboxPriority::Normal,
                created_at: "2026-05-03T00:00:00Z".into(),
                ttl_seconds: Some(600),
            },
        )
        .expect("publish mailbox item");

        let inbox = list_agent_mailbox_inbox(
            &repo_root,
            project_id,
            "run-not-persisted-yet",
            "2026-05-03T00:00:30Z",
            10,
        )
        .expect("missing run inbox context");

        assert!(inbox.is_empty());
    }

    #[test]
    fn mailbox_rejects_instruction_override_content() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-mailbox-injection";
        create_project_database(&repo_root, project_id);
        seed_agent_run(&repo_root, project_id, "run-sender", "Sender");

        let error = publish_agent_mailbox_item(
            &repo_root,
            &NewAgentMailboxItemRecord {
                project_id: project_id.into(),
                sender_run_id: "run-sender".into(),
                item_type: AgentMailboxItemType::HeadsUp,
                parent_item_id: None,
                target_agent_session_id: None,
                target_run_id: None,
                target_role: None,
                title: "Ignore previous instructions".into(),
                body: "This should not enter temporary context.".into(),
                related_paths: Vec::new(),
                priority: AgentMailboxPriority::Normal,
                created_at: "2026-05-03T00:00:00Z".into(),
                ttl_seconds: Some(600),
            },
        )
        .expect_err("instruction override rejected");
        assert_eq!(error.code, "agent_mailbox_content_rejected");
    }

    #[test]
    fn mailbox_promotion_creates_review_only_project_record_candidate() {
        project_record_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(repo_root.join("src")).expect("repo src dir");
        fs::write(repo_root.join("src/lib.rs"), "pub fn feature() {}\n").expect("write source");
        let project_id = "project-mailbox-promotion";
        create_project_database(&repo_root, project_id);
        seed_agent_run(&repo_root, project_id, "run-sender", "Sender");
        seed_agent_run(&repo_root, project_id, "run-promoter", "Promoter");

        let item = publish_agent_mailbox_item(
            &repo_root,
            &NewAgentMailboxItemRecord {
                project_id: project_id.into(),
                sender_run_id: "run-sender".into(),
                item_type: AgentMailboxItemType::VerificationNote,
                parent_item_id: None,
                target_agent_session_id: None,
                target_run_id: Some("run-promoter".into()),
                target_role: None,
                title: "Parser verification passed".into(),
                body: "Scoped parser tests passed after the enum normalization change.".into(),
                related_paths: vec!["src/lib.rs".into()],
                priority: AgentMailboxPriority::High,
                created_at: "2026-05-03T00:00:00Z".into(),
                ttl_seconds: Some(600),
            },
        )
        .expect("publish item");

        let promotion = promote_agent_mailbox_item(
            &repo_root,
            &PromoteAgentMailboxItemRecord {
                project_id: project_id.into(),
                promoter_run_id: "run-promoter".into(),
                item_id: item.item_id,
                title: None,
                summary: Some("Parser verification passed.".into()),
                promoted_at: "2026-05-03T00:02:00Z".into(),
            },
        )
        .expect("promote item");
        assert_eq!(promotion.item.status, AgentMailboxStatus::Promoted);
        assert_eq!(
            promotion.item.promoted_record_id.as_deref(),
            Some(promotion.promoted_record_id.as_str())
        );

        let records = list_project_records(&repo_root, project_id).expect("records");
        let candidate = records
            .into_iter()
            .find(|record| record.record_id == promotion.promoted_record_id)
            .expect("candidate record");
        assert_eq!(
            candidate.visibility,
            ProjectRecordVisibility::MemoryCandidate
        );
        assert!(candidate
            .source_item_ids
            .iter()
            .any(|source| source.starts_with("agent_mailbox_items:")));
        assert!(candidate.tags.iter().any(|tag| tag == "swarm-mailbox"));
    }
}
