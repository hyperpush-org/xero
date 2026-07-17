use std::path::Path;

use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map as JsonMap, Value as JsonValue};

use crate::{
    auth::now_timestamp,
    commands::{
        contracts::workflows::{
            WorkflowDeliveryStateEntityTypeDto, WorkflowStateQueryDto, WorkflowStateQueryFilterDto,
            WorkflowStateQueryFilterOperatorDto, WorkflowStateWriteActionDto,
            WorkflowStateWriteOperationDto,
        },
        CommandError,
    },
    db::database_path_for_repo,
};

use super::{open_runtime_database, read_project_row, validate_non_empty_text};

#[derive(Debug, Clone)]
pub struct DeliveryStateWriteContext<'a> {
    pub workflow_run_id: Option<&'a str>,
    pub node_run_id: Option<&'a str>,
}

pub fn query_delivery_state(
    repo_root: &Path,
    project_id: &str,
    query: &WorkflowStateQueryDto,
) -> Result<JsonValue, CommandError> {
    validate_non_empty_text(project_id, "projectId", "delivery_state_request_invalid")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    query_delivery_state_with_connection(&connection, project_id, query)
}

pub(super) fn query_delivery_state_with_connection(
    connection: &Connection,
    project_id: &str,
    query: &WorkflowStateQueryDto,
) -> Result<JsonValue, CommandError> {
    let mut records = read_records(connection, project_id, query.entity_type)?;
    if !query.include_archived {
        records.retain(|record| {
            json_path_lookup(record, "$.status")
                .and_then(JsonValue::as_str)
                .map(|status| status != "archived")
                .unwrap_or(true)
        });
    }
    records.retain(|record| {
        query
            .filters
            .iter()
            .all(|filter| filter_matches(record, filter))
    });
    if let Some(order_by) = query.order_by.as_deref() {
        records.sort_by(|left, right| {
            compare_json_values(
                json_path_lookup(left, order_by),
                json_path_lookup(right, order_by),
            )
        });
    }
    if let Some(limit) = query.limit {
        records.truncate(limit as usize);
    }

    Ok(json!({
        "entityType": query.entity_type.as_str(),
        "count": records.len(),
        "records": records,
    }))
}

pub fn write_delivery_state(
    repo_root: &Path,
    project_id: &str,
    context: DeliveryStateWriteContext<'_>,
    operation: &WorkflowStateWriteOperationDto,
) -> Result<JsonValue, CommandError> {
    validate_non_empty_text(project_id, "projectId", "delivery_state_request_invalid")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    write_delivery_state_with_connection(&connection, project_id, context, operation)
}

pub(super) fn write_delivery_state_with_connection(
    connection: &Connection,
    project_id: &str,
    context: DeliveryStateWriteContext<'_>,
    operation: &WorkflowStateWriteOperationDto,
) -> Result<JsonValue, CommandError> {
    let payload = JsonValue::Object(operation.payload.clone());
    let entity_id = resolve_entity_id(operation, &payload);
    let before = read_record_by_id(&connection, project_id, operation.entity_type, &entity_id)?;
    let now = now_timestamp();

    let after = match operation.action {
        WorkflowStateWriteActionDto::Create | WorkflowStateWriteActionDto::Upsert => upsert_record(
            &connection,
            project_id,
            operation.entity_type,
            &entity_id,
            &payload,
            &now,
        )?,
        WorkflowStateWriteActionDto::Update | WorkflowStateWriteActionDto::Patch => {
            let merged = merge_record_payload(before.as_ref(), &payload);
            upsert_record(
                &connection,
                project_id,
                operation.entity_type,
                &entity_id,
                &merged,
                &now,
            )?
        }
        WorkflowStateWriteActionDto::MarkComplete => {
            let status = completed_status_for_entity(operation.entity_type)?;
            update_record_status(
                &connection,
                project_id,
                operation.entity_type,
                &entity_id,
                status,
                &now,
            )?
        }
        WorkflowStateWriteActionDto::Archive => {
            let status = archived_status_for_entity(operation.entity_type)?;
            update_record_status(
                &connection,
                project_id,
                operation.entity_type,
                &entity_id,
                status,
                &now,
            )?
        }
    };

    insert_delivery_state_event(
        &connection,
        project_id,
        context,
        operation.entity_type,
        &entity_id,
        operation.action.as_str(),
        before.as_ref(),
        Some(&after),
    )?;

    Ok(json!({
        "entityType": operation.entity_type.as_str(),
        "action": operation.action.as_str(),
        "id": entity_id,
        "record": after,
    }))
}

pub fn export_delivery_state(
    repo_root: &Path,
    project_id: &str,
) -> Result<JsonValue, CommandError> {
    validate_non_empty_text(project_id, "projectId", "delivery_state_request_invalid")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let entities = [
        WorkflowDeliveryStateEntityTypeDto::DeliveryProject,
        WorkflowDeliveryStateEntityTypeDto::Milestone,
        WorkflowDeliveryStateEntityTypeDto::Requirement,
        WorkflowDeliveryStateEntityTypeDto::DeliveryPhase,
        WorkflowDeliveryStateEntityTypeDto::PhaseContext,
        WorkflowDeliveryStateEntityTypeDto::PhasePlan,
        WorkflowDeliveryStateEntityTypeDto::PhaseSummary,
        WorkflowDeliveryStateEntityTypeDto::VerificationEvidence,
        WorkflowDeliveryStateEntityTypeDto::DeferredItem,
        WorkflowDeliveryStateEntityTypeDto::MilestoneArchive,
    ];
    let mut export = JsonMap::new();
    for entity in entities {
        export.insert(
            entity.as_str().into(),
            JsonValue::Array(read_records(&connection, project_id, entity)?),
        );
    }
    Ok(JsonValue::Object(export))
}

pub fn wipe_delivery_state(repo_root: &Path, project_id: &str) -> Result<(), CommandError> {
    validate_non_empty_text(project_id, "projectId", "delivery_state_request_invalid")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    for table in [
        "delivery_state_events",
        "delivery_milestone_archives",
        "delivery_deferred_items",
        "delivery_verification_evidence",
        "delivery_phase_summaries",
        "delivery_phase_plans",
        "delivery_phase_context",
        "delivery_phases",
        "delivery_requirements",
        "delivery_milestones",
        "delivery_projects",
    ] {
        connection
            .execute(
                &format!("DELETE FROM {table} WHERE project_id = ?1"),
                params![project_id],
            )
            .map_err(|error| map_delivery_state_write_error("delivery_state_wipe_failed", error))?;
    }
    Ok(())
}

fn resolve_entity_id(operation: &WorkflowStateWriteOperationDto, payload: &JsonValue) -> String {
    operation
        .target_id
        .as_deref()
        .or_else(|| payload.get("id").and_then(JsonValue::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| operation.idempotency_key.clone())
        .unwrap_or_else(|| generate_delivery_id(operation.entity_type.as_str()))
}

fn upsert_record(
    connection: &Connection,
    project_id: &str,
    entity_type: WorkflowDeliveryStateEntityTypeDto,
    entity_id: &str,
    payload: &JsonValue,
    now: &str,
) -> Result<JsonValue, CommandError> {
    match entity_type {
        WorkflowDeliveryStateEntityTypeDto::DeliveryProject => {
            let title = text_field(payload, &["title", "name"]).unwrap_or("Delivery project");
            let summary = text_field(payload, &["summary"]).unwrap_or("");
            let status = text_field(payload, &["status"]).unwrap_or("active");
            execute_upsert(
                connection,
                r#"
                INSERT INTO delivery_projects (id, project_id, title, summary, status, metadata_json, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                ON CONFLICT(project_id, id) DO UPDATE SET
                    title = excluded.title,
                    summary = excluded.summary,
                    status = excluded.status,
                    metadata_json = excluded.metadata_json,
                    updated_at = excluded.updated_at
                "#,
                params![
                    entity_id,
                    project_id,
                    title,
                    summary,
                    status,
                    metadata_json(payload)?,
                    now
                ],
            )?;
        }
        WorkflowDeliveryStateEntityTypeDto::Milestone => {
            let title = text_field(payload, &["title", "name"]).unwrap_or("Milestone");
            let summary = text_field(payload, &["summary"]).unwrap_or("");
            let goal = text_field(payload, &["goal"]).unwrap_or(summary);
            let status = text_field(payload, &["status"]).unwrap_or("active");
            let delivery_project_id =
                text_field(payload, &["deliveryProjectId", "delivery_project_id"]);
            execute_upsert(
                connection,
                r#"
                INSERT INTO delivery_milestones (id, project_id, delivery_project_id, title, summary, goal, status, metadata_json, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                ON CONFLICT(project_id, id) DO UPDATE SET
                    delivery_project_id = excluded.delivery_project_id,
                    title = excluded.title,
                    summary = excluded.summary,
                    goal = excluded.goal,
                    status = excluded.status,
                    metadata_json = excluded.metadata_json,
                    updated_at = excluded.updated_at
                "#,
                params![
                    entity_id,
                    project_id,
                    delivery_project_id,
                    title,
                    summary,
                    goal,
                    status,
                    metadata_json(payload)?,
                    now
                ],
            )?;
        }
        WorkflowDeliveryStateEntityTypeDto::Requirement => {
            let milestone_id = required_text_field(
                payload,
                &["milestoneId", "milestone_id"],
                "requirement milestoneId",
            )?;
            let title = text_field(payload, &["title", "name"]).unwrap_or("Requirement");
            let description = text_field(payload, &["description", "summary"]).unwrap_or("");
            let status = text_field(payload, &["status"]).unwrap_or("open");
            let priority = payload
                .get("priority")
                .and_then(JsonValue::as_i64)
                .unwrap_or(0);
            execute_upsert(
                connection,
                r#"
                INSERT INTO delivery_requirements (id, project_id, milestone_id, title, description, status, priority, metadata_json, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                ON CONFLICT(project_id, id) DO UPDATE SET
                    milestone_id = excluded.milestone_id,
                    title = excluded.title,
                    description = excluded.description,
                    status = excluded.status,
                    priority = excluded.priority,
                    metadata_json = excluded.metadata_json,
                    updated_at = excluded.updated_at
                "#,
                params![
                    entity_id,
                    project_id,
                    milestone_id,
                    title,
                    description,
                    status,
                    priority,
                    metadata_json(payload)?,
                    now
                ],
            )?;
        }
        WorkflowDeliveryStateEntityTypeDto::DeliveryPhase => {
            let milestone_id = required_text_field(
                payload,
                &["milestoneId", "milestone_id"],
                "delivery phase milestoneId",
            )?;
            let phase_key =
                text_field(payload, &["phaseKey", "phase_key", "key"]).unwrap_or(entity_id);
            let title = text_field(payload, &["title", "name"]).unwrap_or(phase_key);
            let summary = text_field(payload, &["summary"]).unwrap_or("");
            let status = text_field(payload, &["status"]).unwrap_or("incomplete");
            let sort_order = payload
                .get("sortOrder")
                .or_else(|| payload.get("sort_order"))
                .and_then(JsonValue::as_f64)
                .unwrap_or_else(|| parse_phase_sort_order(phase_key));
            let inserted_after_phase_id = text_field(
                payload,
                &["insertedAfterPhaseId", "inserted_after_phase_id"],
            );
            execute_upsert(
                connection,
                r#"
                INSERT INTO delivery_phases (id, project_id, milestone_id, phase_key, title, summary, status, sort_order, inserted_after_phase_id, metadata_json, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
                ON CONFLICT(project_id, id) DO UPDATE SET
                    milestone_id = excluded.milestone_id,
                    phase_key = excluded.phase_key,
                    title = excluded.title,
                    summary = excluded.summary,
                    status = excluded.status,
                    sort_order = excluded.sort_order,
                    inserted_after_phase_id = excluded.inserted_after_phase_id,
                    metadata_json = excluded.metadata_json,
                    updated_at = excluded.updated_at
                "#,
                params![
                    entity_id,
                    project_id,
                    milestone_id,
                    phase_key,
                    title,
                    summary,
                    status,
                    sort_order,
                    inserted_after_phase_id,
                    metadata_json(payload)?,
                    now
                ],
            )?;
        }
        WorkflowDeliveryStateEntityTypeDto::PhaseContext => {
            insert_json_child(
                connection,
                "delivery_phase_context",
                "context_json",
                project_id,
                entity_id,
                "phase_id",
                required_text_field(payload, &["phaseId", "phase_id"], "phase context phaseId")?,
                payload,
                now,
            )?;
        }
        WorkflowDeliveryStateEntityTypeDto::PhasePlan => {
            insert_json_child(
                connection,
                "delivery_phase_plans",
                "plan_json",
                project_id,
                entity_id,
                "phase_id",
                required_text_field(payload, &["phaseId", "phase_id"], "phase plan phaseId")?,
                payload,
                now,
            )?;
        }
        WorkflowDeliveryStateEntityTypeDto::PhaseSummary => {
            insert_json_child(
                connection,
                "delivery_phase_summaries",
                "summary_json",
                project_id,
                entity_id,
                "phase_id",
                required_text_field(payload, &["phaseId", "phase_id"], "phase summary phaseId")?,
                payload,
                now,
            )?;
        }
        WorkflowDeliveryStateEntityTypeDto::VerificationEvidence => {
            let evidence_json = serde_json::to_string(payload).map_err(|error| {
                CommandError::system_fault("delivery_state_json_encode_failed", error.to_string())
            })?;
            let phase_id = text_field(payload, &["phaseId", "phase_id"]);
            let requirement_id = text_field(payload, &["requirementId", "requirement_id"]);
            let status = text_field(payload, &["status"]).unwrap_or("recorded");
            execute_upsert(
                connection,
                r#"
                INSERT INTO delivery_verification_evidence (id, project_id, phase_id, requirement_id, status, evidence_json, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                ON CONFLICT(project_id, id) DO UPDATE SET
                    phase_id = excluded.phase_id,
                    requirement_id = excluded.requirement_id,
                    status = excluded.status,
                    evidence_json = excluded.evidence_json,
                    updated_at = excluded.updated_at
                "#,
                params![
                    entity_id,
                    project_id,
                    phase_id,
                    requirement_id,
                    status,
                    evidence_json,
                    now
                ],
            )?;
        }
        WorkflowDeliveryStateEntityTypeDto::DeferredItem => {
            let title = text_field(payload, &["title", "summary"]).unwrap_or("Deferred item");
            let reason = text_field(payload, &["reason"]).unwrap_or("");
            let status = text_field(payload, &["status"]).unwrap_or("open");
            let milestone_id = text_field(payload, &["milestoneId", "milestone_id"]);
            let phase_id = text_field(payload, &["phaseId", "phase_id"]);
            execute_upsert(
                connection,
                r#"
                INSERT INTO delivery_deferred_items (id, project_id, milestone_id, phase_id, title, reason, status, metadata_json, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                ON CONFLICT(project_id, id) DO UPDATE SET
                    milestone_id = excluded.milestone_id,
                    phase_id = excluded.phase_id,
                    title = excluded.title,
                    reason = excluded.reason,
                    status = excluded.status,
                    metadata_json = excluded.metadata_json,
                    updated_at = excluded.updated_at
                "#,
                params![
                    entity_id,
                    project_id,
                    milestone_id,
                    phase_id,
                    title,
                    reason,
                    status,
                    metadata_json(payload)?,
                    now
                ],
            )?;
        }
        WorkflowDeliveryStateEntityTypeDto::MilestoneArchive => {
            let milestone_id = required_text_field(
                payload,
                &["milestoneId", "milestone_id"],
                "milestone archive milestoneId",
            )?;
            let archive_json = serde_json::to_string(payload).map_err(|error| {
                CommandError::system_fault("delivery_state_json_encode_failed", error.to_string())
            })?;
            execute_upsert(
                connection,
                r#"
                INSERT INTO delivery_milestone_archives (id, project_id, milestone_id, archive_json, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(project_id, id) DO UPDATE SET
                    milestone_id = excluded.milestone_id,
                    archive_json = excluded.archive_json
                "#,
                params![entity_id, project_id, milestone_id, archive_json, now],
            )?;
        }
    }
    read_record_by_id(connection, project_id, entity_type, entity_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "delivery_state_missing_after_write",
            format!(
                "Xero wrote {} `{entity_id}` but could not read it back.",
                entity_type.as_str()
            ),
        )
    })
}

fn update_record_status(
    connection: &Connection,
    project_id: &str,
    entity_type: WorkflowDeliveryStateEntityTypeDto,
    entity_id: &str,
    status: &str,
    now: &str,
) -> Result<JsonValue, CommandError> {
    let table = table_for_entity(entity_type);
    status_column_for_entity(entity_type)?;
    let timestamp_column = match (entity_type, status) {
        (WorkflowDeliveryStateEntityTypeDto::Milestone, "completed")
        | (WorkflowDeliveryStateEntityTypeDto::DeliveryPhase, "complete") => Some("completed_at"),
        (WorkflowDeliveryStateEntityTypeDto::Milestone, "archived") => Some("archived_at"),
        _ => None,
    };
    match timestamp_column {
        Some(column) => {
            connection
                .execute(
                    &format!(
                        "UPDATE {table} SET status = ?3, updated_at = ?4, {column} = COALESCE({column}, ?4) WHERE project_id = ?1 AND id = ?2"
                    ),
                    params![project_id, entity_id, status, now],
                )
                .map_err(|error| {
                    map_delivery_state_write_error("delivery_state_status_update_failed", error)
                })?;
        }
        None => {
            connection
                .execute(
                    &format!(
                        "UPDATE {table} SET status = ?3, updated_at = ?4 WHERE project_id = ?1 AND id = ?2"
                    ),
                    params![project_id, entity_id, status, now],
                )
                .map_err(|error| {
                    map_delivery_state_write_error("delivery_state_status_update_failed", error)
                })?;
        }
    }
    read_record_by_id(connection, project_id, entity_type, entity_id)?.ok_or_else(|| {
        CommandError::user_fixable(
            "delivery_state_record_not_found",
            format!(
                "Xero could not find {} `{entity_id}`.",
                entity_type.as_str()
            ),
        )
    })
}

fn completed_status_for_entity(
    entity_type: WorkflowDeliveryStateEntityTypeDto,
) -> Result<&'static str, CommandError> {
    match entity_type {
        WorkflowDeliveryStateEntityTypeDto::DeliveryProject => Ok("completed"),
        WorkflowDeliveryStateEntityTypeDto::Milestone => Ok("completed"),
        WorkflowDeliveryStateEntityTypeDto::Requirement => Ok("satisfied"),
        WorkflowDeliveryStateEntityTypeDto::DeliveryPhase => Ok("complete"),
        WorkflowDeliveryStateEntityTypeDto::VerificationEvidence => Ok("passed"),
        WorkflowDeliveryStateEntityTypeDto::DeferredItem => Ok("closed"),
        WorkflowDeliveryStateEntityTypeDto::PhaseContext
        | WorkflowDeliveryStateEntityTypeDto::PhasePlan
        | WorkflowDeliveryStateEntityTypeDto::PhaseSummary
        | WorkflowDeliveryStateEntityTypeDto::MilestoneArchive => Err(CommandError::user_fixable(
            "delivery_state_action_unsupported",
            format!(
                "Xero cannot mark {} records complete because that entity type has no status lifecycle.",
                entity_type.as_str()
            ),
        )),
    }
}

fn archived_status_for_entity(
    entity_type: WorkflowDeliveryStateEntityTypeDto,
) -> Result<&'static str, CommandError> {
    match entity_type {
        WorkflowDeliveryStateEntityTypeDto::DeliveryProject
        | WorkflowDeliveryStateEntityTypeDto::Milestone
        | WorkflowDeliveryStateEntityTypeDto::Requirement
        | WorkflowDeliveryStateEntityTypeDto::DeliveryPhase
        | WorkflowDeliveryStateEntityTypeDto::DeferredItem => Ok("archived"),
        WorkflowDeliveryStateEntityTypeDto::VerificationEvidence => Ok("superseded"),
        WorkflowDeliveryStateEntityTypeDto::PhaseContext
        | WorkflowDeliveryStateEntityTypeDto::PhasePlan
        | WorkflowDeliveryStateEntityTypeDto::PhaseSummary
        | WorkflowDeliveryStateEntityTypeDto::MilestoneArchive => Err(CommandError::user_fixable(
            "delivery_state_action_unsupported",
            format!(
                "Xero cannot archive {} records because that entity type has no status lifecycle.",
                entity_type.as_str()
            ),
        )),
    }
}

fn status_column_for_entity(
    entity_type: WorkflowDeliveryStateEntityTypeDto,
) -> Result<(), CommandError> {
    match entity_type {
        WorkflowDeliveryStateEntityTypeDto::DeliveryProject
        | WorkflowDeliveryStateEntityTypeDto::Milestone
        | WorkflowDeliveryStateEntityTypeDto::Requirement
        | WorkflowDeliveryStateEntityTypeDto::DeliveryPhase
        | WorkflowDeliveryStateEntityTypeDto::VerificationEvidence
        | WorkflowDeliveryStateEntityTypeDto::DeferredItem => Ok(()),
        WorkflowDeliveryStateEntityTypeDto::PhaseContext
        | WorkflowDeliveryStateEntityTypeDto::PhasePlan
        | WorkflowDeliveryStateEntityTypeDto::PhaseSummary
        | WorkflowDeliveryStateEntityTypeDto::MilestoneArchive => Err(CommandError::user_fixable(
            "delivery_state_action_unsupported",
            format!(
                "Xero cannot update {} status because that entity type has no status column.",
                entity_type.as_str()
            ),
        )),
    }
}

fn read_records(
    connection: &Connection,
    project_id: &str,
    entity_type: WorkflowDeliveryStateEntityTypeDto,
) -> Result<Vec<JsonValue>, CommandError> {
    let sql = select_sql_for_entity(entity_type, "WHERE project_id = ?1");
    let mut statement = connection.prepare(&sql).map_err(|error| {
        map_delivery_state_query_error("delivery_state_query_prepare_failed", error)
    })?;
    let rows = statement
        .query_map(params![project_id], |row| row_to_record(entity_type, row))
        .map_err(|error| map_delivery_state_query_error("delivery_state_query_failed", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| map_delivery_state_query_error("delivery_state_decode_failed", error))
}

fn read_record_by_id(
    connection: &Connection,
    project_id: &str,
    entity_type: WorkflowDeliveryStateEntityTypeDto,
    entity_id: &str,
) -> Result<Option<JsonValue>, CommandError> {
    let sql = select_sql_for_entity(entity_type, "WHERE project_id = ?1 AND id = ?2");
    connection
        .query_row(&sql, params![project_id, entity_id], |row| {
            row_to_record(entity_type, row)
        })
        .optional()
        .map_err(|error| {
            map_delivery_state_query_error("delivery_state_record_query_failed", error)
        })
}

fn select_sql_for_entity(
    entity_type: WorkflowDeliveryStateEntityTypeDto,
    where_clause: &str,
) -> String {
    match entity_type {
        WorkflowDeliveryStateEntityTypeDto::DeliveryProject => format!(
            "SELECT id, title, summary, status, metadata_json, created_at, updated_at FROM delivery_projects {where_clause} ORDER BY updated_at DESC"
        ),
        WorkflowDeliveryStateEntityTypeDto::Milestone => format!(
            "SELECT id, delivery_project_id, title, summary, goal, status, metadata_json, created_at, updated_at, completed_at, archived_at FROM delivery_milestones {where_clause} ORDER BY updated_at DESC"
        ),
        WorkflowDeliveryStateEntityTypeDto::Requirement => format!(
            "SELECT id, milestone_id, title, description, status, priority, metadata_json, created_at, updated_at FROM delivery_requirements {where_clause} ORDER BY priority DESC, created_at ASC"
        ),
        WorkflowDeliveryStateEntityTypeDto::DeliveryPhase => format!(
            "SELECT id, milestone_id, phase_key, title, summary, status, sort_order, inserted_after_phase_id, metadata_json, created_at, updated_at, completed_at FROM delivery_phases {where_clause} ORDER BY sort_order ASC, phase_key ASC"
        ),
        WorkflowDeliveryStateEntityTypeDto::PhaseContext => format!(
            "SELECT id, phase_id, context_json, created_at, updated_at FROM delivery_phase_context {where_clause} ORDER BY updated_at DESC"
        ),
        WorkflowDeliveryStateEntityTypeDto::PhasePlan => format!(
            "SELECT id, phase_id, plan_json, created_at, updated_at FROM delivery_phase_plans {where_clause} ORDER BY updated_at DESC"
        ),
        WorkflowDeliveryStateEntityTypeDto::PhaseSummary => format!(
            "SELECT id, phase_id, summary_json, created_at, updated_at FROM delivery_phase_summaries {where_clause} ORDER BY updated_at DESC"
        ),
        WorkflowDeliveryStateEntityTypeDto::VerificationEvidence => format!(
            "SELECT id, phase_id, requirement_id, status, evidence_json, created_at, updated_at FROM delivery_verification_evidence {where_clause} ORDER BY updated_at DESC"
        ),
        WorkflowDeliveryStateEntityTypeDto::DeferredItem => format!(
            "SELECT id, milestone_id, phase_id, title, reason, status, metadata_json, created_at, updated_at FROM delivery_deferred_items {where_clause} ORDER BY updated_at DESC"
        ),
        WorkflowDeliveryStateEntityTypeDto::MilestoneArchive => format!(
            "SELECT id, milestone_id, archive_json, created_at FROM delivery_milestone_archives {where_clause} ORDER BY created_at DESC"
        ),
    }
}

fn row_to_record(
    entity_type: WorkflowDeliveryStateEntityTypeDto,
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<JsonValue> {
    match entity_type {
        WorkflowDeliveryStateEntityTypeDto::DeliveryProject => Ok(json!({
            "entityType": entity_type.as_str(),
            "id": row.get::<_, String>(0)?,
            "title": row.get::<_, String>(1)?,
            "summary": row.get::<_, String>(2)?,
            "status": row.get::<_, String>(3)?,
            "metadata": decode_json_column(row, 4)?,
            "createdAt": row.get::<_, String>(5)?,
            "updatedAt": row.get::<_, String>(6)?,
        })),
        WorkflowDeliveryStateEntityTypeDto::Milestone => Ok(json!({
            "entityType": entity_type.as_str(),
            "id": row.get::<_, String>(0)?,
            "deliveryProjectId": row.get::<_, Option<String>>(1)?,
            "title": row.get::<_, String>(2)?,
            "summary": row.get::<_, String>(3)?,
            "goal": row.get::<_, String>(4)?,
            "status": row.get::<_, String>(5)?,
            "metadata": decode_json_column(row, 6)?,
            "createdAt": row.get::<_, String>(7)?,
            "updatedAt": row.get::<_, String>(8)?,
            "completedAt": row.get::<_, Option<String>>(9)?,
            "archivedAt": row.get::<_, Option<String>>(10)?,
        })),
        WorkflowDeliveryStateEntityTypeDto::Requirement => Ok(json!({
            "entityType": entity_type.as_str(),
            "id": row.get::<_, String>(0)?,
            "milestoneId": row.get::<_, String>(1)?,
            "title": row.get::<_, String>(2)?,
            "description": row.get::<_, String>(3)?,
            "status": row.get::<_, String>(4)?,
            "priority": row.get::<_, i64>(5)?,
            "metadata": decode_json_column(row, 6)?,
            "createdAt": row.get::<_, String>(7)?,
            "updatedAt": row.get::<_, String>(8)?,
        })),
        WorkflowDeliveryStateEntityTypeDto::DeliveryPhase => Ok(json!({
            "entityType": entity_type.as_str(),
            "id": row.get::<_, String>(0)?,
            "milestoneId": row.get::<_, String>(1)?,
            "phaseKey": row.get::<_, String>(2)?,
            "title": row.get::<_, String>(3)?,
            "summary": row.get::<_, String>(4)?,
            "status": row.get::<_, String>(5)?,
            "sortOrder": row.get::<_, f64>(6)?,
            "insertedAfterPhaseId": row.get::<_, Option<String>>(7)?,
            "metadata": decode_json_column(row, 8)?,
            "createdAt": row.get::<_, String>(9)?,
            "updatedAt": row.get::<_, String>(10)?,
            "completedAt": row.get::<_, Option<String>>(11)?,
        })),
        WorkflowDeliveryStateEntityTypeDto::PhaseContext => {
            json_child_record(entity_type, row, "phaseId", "context")
        }
        WorkflowDeliveryStateEntityTypeDto::PhasePlan => {
            json_child_record(entity_type, row, "phaseId", "plan")
        }
        WorkflowDeliveryStateEntityTypeDto::PhaseSummary => {
            json_child_record(entity_type, row, "phaseId", "summary")
        }
        WorkflowDeliveryStateEntityTypeDto::VerificationEvidence => Ok(json!({
            "entityType": entity_type.as_str(),
            "id": row.get::<_, String>(0)?,
            "phaseId": row.get::<_, Option<String>>(1)?,
            "requirementId": row.get::<_, Option<String>>(2)?,
            "status": row.get::<_, String>(3)?,
            "evidence": decode_json_column(row, 4)?,
            "createdAt": row.get::<_, String>(5)?,
            "updatedAt": row.get::<_, String>(6)?,
        })),
        WorkflowDeliveryStateEntityTypeDto::DeferredItem => Ok(json!({
            "entityType": entity_type.as_str(),
            "id": row.get::<_, String>(0)?,
            "milestoneId": row.get::<_, Option<String>>(1)?,
            "phaseId": row.get::<_, Option<String>>(2)?,
            "title": row.get::<_, String>(3)?,
            "reason": row.get::<_, String>(4)?,
            "status": row.get::<_, String>(5)?,
            "metadata": decode_json_column(row, 6)?,
            "createdAt": row.get::<_, String>(7)?,
            "updatedAt": row.get::<_, String>(8)?,
        })),
        WorkflowDeliveryStateEntityTypeDto::MilestoneArchive => Ok(json!({
            "entityType": entity_type.as_str(),
            "id": row.get::<_, String>(0)?,
            "milestoneId": row.get::<_, String>(1)?,
            "archive": decode_json_column(row, 2)?,
            "createdAt": row.get::<_, String>(3)?,
        })),
    }
}

fn json_child_record(
    entity_type: WorkflowDeliveryStateEntityTypeDto,
    row: &rusqlite::Row<'_>,
    parent_key: &str,
    payload_key: &str,
) -> rusqlite::Result<JsonValue> {
    Ok(json!({
        "entityType": entity_type.as_str(),
        "id": row.get::<_, String>(0)?,
        parent_key: row.get::<_, String>(1)?,
        payload_key: decode_json_column(row, 2)?,
        "createdAt": row.get::<_, String>(3)?,
        "updatedAt": row.get::<_, String>(4)?,
    }))
}

#[allow(clippy::too_many_arguments)]
fn insert_json_child(
    connection: &Connection,
    table: &str,
    json_column: &str,
    project_id: &str,
    entity_id: &str,
    parent_column: &str,
    parent_id: &str,
    payload: &JsonValue,
    now: &str,
) -> Result<(), CommandError> {
    let payload_json = serde_json::to_string(payload).map_err(|error| {
        CommandError::system_fault("delivery_state_json_encode_failed", error.to_string())
    })?;
    let sql = format!(
        "INSERT INTO {table} (id, project_id, {parent_column}, {json_column}, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(project_id, id) DO UPDATE SET
            {parent_column} = excluded.{parent_column},
            {json_column} = excluded.{json_column},
            updated_at = excluded.updated_at"
    );
    execute_upsert(
        connection,
        &sql,
        params![entity_id, project_id, parent_id, payload_json, now],
    )
}

#[allow(clippy::too_many_arguments)]
fn insert_delivery_state_event(
    connection: &Connection,
    project_id: &str,
    context: DeliveryStateWriteContext<'_>,
    entity_type: WorkflowDeliveryStateEntityTypeDto,
    entity_id: &str,
    event_type: &str,
    before: Option<&JsonValue>,
    after: Option<&JsonValue>,
) -> Result<(), CommandError> {
    let before_json = before
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            CommandError::system_fault("delivery_state_json_encode_failed", error.to_string())
        })?;
    let after_json = after
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            CommandError::system_fault("delivery_state_json_encode_failed", error.to_string())
        })?;
    connection
        .execute(
            r#"
            INSERT INTO delivery_state_events (
                id, project_id, workflow_run_id, node_run_id, entity_type, entity_id,
                event_type, before_json, after_json, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                generate_delivery_id("delivery-state-event"),
                project_id,
                context.workflow_run_id,
                context.node_run_id,
                entity_type.as_str(),
                entity_id,
                event_type,
                before_json,
                after_json,
                now_timestamp(),
            ],
        )
        .map_err(|error| {
            map_delivery_state_write_error("delivery_state_event_insert_failed", error)
        })?;
    Ok(())
}

fn execute_upsert<P: rusqlite::Params>(
    connection: &Connection,
    sql: &str,
    params: P,
) -> Result<(), CommandError> {
    connection
        .execute(sql, params)
        .map(|_| ())
        .map_err(|error| map_delivery_state_write_error("delivery_state_write_failed", error))
}

fn table_for_entity(entity_type: WorkflowDeliveryStateEntityTypeDto) -> &'static str {
    match entity_type {
        WorkflowDeliveryStateEntityTypeDto::DeliveryProject => "delivery_projects",
        WorkflowDeliveryStateEntityTypeDto::Milestone => "delivery_milestones",
        WorkflowDeliveryStateEntityTypeDto::Requirement => "delivery_requirements",
        WorkflowDeliveryStateEntityTypeDto::DeliveryPhase => "delivery_phases",
        WorkflowDeliveryStateEntityTypeDto::PhaseContext => "delivery_phase_context",
        WorkflowDeliveryStateEntityTypeDto::PhasePlan => "delivery_phase_plans",
        WorkflowDeliveryStateEntityTypeDto::PhaseSummary => "delivery_phase_summaries",
        WorkflowDeliveryStateEntityTypeDto::VerificationEvidence => {
            "delivery_verification_evidence"
        }
        WorkflowDeliveryStateEntityTypeDto::DeferredItem => "delivery_deferred_items",
        WorkflowDeliveryStateEntityTypeDto::MilestoneArchive => "delivery_milestone_archives",
    }
}

fn merge_record_payload(before: Option<&JsonValue>, patch: &JsonValue) -> JsonValue {
    let mut base = before.cloned().unwrap_or_else(|| json!({}));
    merge_json(&mut base, patch);
    base
}

fn merge_json(base: &mut JsonValue, patch: &JsonValue) {
    match (base, patch) {
        (JsonValue::Object(base), JsonValue::Object(patch)) => {
            for (key, value) in patch {
                merge_json(base.entry(key.clone()).or_insert(JsonValue::Null), value);
            }
        }
        (base, patch) => *base = patch.clone(),
    }
}

fn filter_matches(record: &JsonValue, filter: &WorkflowStateQueryFilterDto) -> bool {
    let actual = json_path_lookup(record, &filter.path);
    match filter.operator {
        WorkflowStateQueryFilterOperatorDto::Eq => actual == filter.value.as_ref(),
        WorkflowStateQueryFilterOperatorDto::Neq => actual != filter.value.as_ref(),
        WorkflowStateQueryFilterOperatorDto::In => actual
            .map(|actual| filter.values.iter().any(|value| value == actual))
            .unwrap_or(false),
        WorkflowStateQueryFilterOperatorDto::NotIn => actual
            .map(|actual| !filter.values.iter().any(|value| value == actual))
            .unwrap_or(true),
        WorkflowStateQueryFilterOperatorDto::Exists => actual.is_some(),
        WorkflowStateQueryFilterOperatorDto::Missing => actual.is_none(),
    }
}

fn compare_json_values(left: Option<&JsonValue>, right: Option<&JsonValue>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => match (left.as_f64(), right.as_f64()) {
            (Some(left), Some(right)) => left
                .partial_cmp(&right)
                .unwrap_or(std::cmp::Ordering::Equal),
            _ => value_sort_key(left).cmp(&value_sort_key(right)),
        },
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

fn value_sort_key(value: &JsonValue) -> String {
    match value {
        JsonValue::String(text) => text.clone(),
        value => serde_json::to_string(value).unwrap_or_else(|_| value.to_string()),
    }
}

fn text_field<'a>(value: &'a JsonValue, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(JsonValue::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn required_text_field<'a>(
    value: &'a JsonValue,
    keys: &[&str],
    label: &str,
) -> Result<&'a str, CommandError> {
    text_field(value, keys).ok_or_else(|| {
        CommandError::user_fixable(
            "delivery_state_payload_invalid",
            format!("Xero needs `{label}` to write delivery state."),
        )
    })
}

fn metadata_json(payload: &JsonValue) -> Result<String, CommandError> {
    serde_json::to_string(payload).map_err(|error| {
        CommandError::system_fault("delivery_state_json_encode_failed", error.to_string())
    })
}

fn decode_json_column(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<JsonValue> {
    let raw: String = row.get(index)?;
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn parse_phase_sort_order(phase_key: &str) -> f64 {
    phase_key.parse::<f64>().unwrap_or(0.0)
}

fn json_path_lookup<'a>(value: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    if path == "$" {
        return Some(value);
    }
    let mut cursor = value;
    let remainder = path.strip_prefix("$.")?;
    for segment in remainder.split('.') {
        let (field, indexes) = parse_path_segment(segment)?;
        cursor = cursor.get(field)?;
        for index in indexes {
            cursor = cursor.get(index)?;
        }
    }
    Some(cursor)
}

fn parse_path_segment(segment: &str) -> Option<(&str, Vec<usize>)> {
    let field_end = segment.find('[').unwrap_or(segment.len());
    let field = &segment[..field_end];
    if field.is_empty() {
        return None;
    }
    let mut indexes = Vec::new();
    let mut rest = &segment[field_end..];
    while !rest.is_empty() {
        let inner = rest.strip_prefix('[')?;
        let close = inner.find(']')?;
        indexes.push(inner[..close].parse::<usize>().ok()?);
        rest = &inner[close + 1..];
    }
    Some((field, indexes))
}

fn generate_delivery_id(prefix: &str) -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "{prefix}-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn map_delivery_state_write_error(code: &'static str, error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        code,
        format!("Xero could not write durable delivery state: {error}"),
    )
}

fn map_delivery_state_query_error(code: &'static str, error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        code,
        format!("Xero could not read durable delivery state: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        configure_connection, migrations::migrations, register_project_database_path_for_tests,
    };
    use rusqlite::Connection;
    use tempfile::TempDir;

    fn repo_with_database() -> TempDir {
        let temp = TempDir::new().expect("create temp repo");
        let database_path = temp.path().join("state.db");
        register_project_database_path_for_tests(temp.path(), database_path.clone());
        let mut connection = Connection::open(&database_path).expect("open project db");
        configure_connection(&connection).expect("configure project db");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate project db");
        connection
            .execute(
                r#"
                INSERT INTO projects (
                    id,
                    name,
                    description,
                    milestone,
                    total_phases,
                    completed_phases,
                    active_phase,
                    branch,
                    created_at,
                    updated_at
                )
                VALUES ('project-1', 'Project', '', '', 0, 0, 0, 'main', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')
                "#,
                [],
            )
            .expect("seed project");
        temp
    }

    fn payload(entries: &[(&str, JsonValue)]) -> serde_json::Map<String, JsonValue> {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    #[test]
    fn delivery_state_writes_queries_completes_exports_and_wipes_records() {
        let repo = repo_with_database();
        let context = DeliveryStateWriteContext {
            workflow_run_id: None,
            node_run_id: None,
        };
        let milestone = write_delivery_state(
            repo.path(),
            "project-1",
            context.clone(),
            &WorkflowStateWriteOperationDto {
                entity_type: WorkflowDeliveryStateEntityTypeDto::Milestone,
                action: WorkflowStateWriteActionDto::Upsert,
                idempotency_key: Some("milestone-1".into()),
                target_id: None,
                payload: payload(&[
                    ("title", json!("Milestone")),
                    ("summary", json!("Ship the thing")),
                    ("goal", json!("Ship the thing")),
                ]),
                output_artifact_type: "state_write_result".into(),
            },
        )
        .expect("write milestone");
        assert_eq!(milestone.get("id"), Some(&json!("milestone-1")));

        write_delivery_state(
            repo.path(),
            "project-1",
            context.clone(),
            &WorkflowStateWriteOperationDto {
                entity_type: WorkflowDeliveryStateEntityTypeDto::DeliveryPhase,
                action: WorkflowStateWriteActionDto::Upsert,
                idempotency_key: Some("phase-1".into()),
                target_id: None,
                payload: payload(&[
                    ("milestoneId", json!("milestone-1")),
                    ("phaseKey", json!("1")),
                    ("title", json!("Phase 1")),
                    ("status", json!("incomplete")),
                    ("sortOrder", json!(1)),
                ]),
                output_artifact_type: "state_write_result".into(),
            },
        )
        .expect("write phase");

        let incomplete = query_delivery_state(
            repo.path(),
            "project-1",
            &WorkflowStateQueryDto {
                entity_type: WorkflowDeliveryStateEntityTypeDto::DeliveryPhase,
                filters: vec![WorkflowStateQueryFilterDto {
                    path: "$.status".into(),
                    operator: WorkflowStateQueryFilterOperatorDto::Eq,
                    value: Some(json!("incomplete")),
                    values: Vec::new(),
                }],
                order_by: Some("$.sortOrder".into()),
                limit: None,
                include_archived: false,
            },
        )
        .expect("query incomplete phases");
        assert_eq!(incomplete.get("count"), Some(&json!(1)));

        let completed = write_delivery_state(
            repo.path(),
            "project-1",
            context,
            &WorkflowStateWriteOperationDto {
                entity_type: WorkflowDeliveryStateEntityTypeDto::DeliveryPhase,
                action: WorkflowStateWriteActionDto::MarkComplete,
                idempotency_key: None,
                target_id: Some("phase-1".into()),
                payload: serde_json::Map::new(),
                output_artifact_type: "state_write_result".into(),
            },
        )
        .expect("mark phase complete");
        assert_eq!(
            completed
                .get("record")
                .and_then(|record| record.get("status")),
            Some(&json!("complete"))
        );

        let export = export_delivery_state(repo.path(), "project-1").expect("export state");
        assert_eq!(
            export
                .get("delivery_phase")
                .and_then(JsonValue::as_array)
                .map(Vec::len),
            Some(1)
        );

        wipe_delivery_state(repo.path(), "project-1").expect("wipe state");
        let after_wipe = query_delivery_state(
            repo.path(),
            "project-1",
            &WorkflowStateQueryDto {
                entity_type: WorkflowDeliveryStateEntityTypeDto::DeliveryPhase,
                filters: Vec::new(),
                order_by: None,
                limit: None,
                include_archived: true,
            },
        )
        .expect("query after wipe");
        assert_eq!(after_wipe.get("count"), Some(&json!(0)));
    }
}
