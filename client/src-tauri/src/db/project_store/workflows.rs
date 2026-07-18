use std::{collections::BTreeSet, path::Path};

use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};

use crate::{
    auth::now_timestamp,
    commands::{
        contracts::{
            workflow_agents::AgentRefDto,
            workflows::{
                WorkflowArtifactRecordDto, WorkflowDefinitionDto, WorkflowDefinitionSummaryDto,
                WorkflowEventDto, WorkflowGateDecisionDto, WorkflowHumanCheckpointTypeDto,
                WorkflowLoopAttemptDto, WorkflowNodeDto, WorkflowNodeRunStatusDto, WorkflowRunDto,
                WorkflowRunEdgeDecisionDto, WorkflowRunNodeDto, WorkflowRunStatusDto,
                WorkflowStateQueryDto, WorkflowStateWriteOperationDto, WorkflowTerminalStatusDto,
            },
        },
        CommandError,
    },
    db::database_path_for_repo,
};

use super::{
    delivery_state::{query_delivery_state_with_connection, write_delivery_state_with_connection},
    open_runtime_database, read_project_row, validate_non_empty_text, DeliveryStateWriteContext,
};

pub fn generate_workflow_id(prefix: &str) -> String {
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

pub fn create_workflow_definition(
    repo_root: &Path,
    definition: &WorkflowDefinitionDto,
) -> Result<WorkflowDefinitionDto, CommandError> {
    validate_definition_identity(definition)?;
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(
        &connection,
        &database_path,
        repo_root,
        &definition.project_id,
    )?;

    let now = now_timestamp();
    let version_id = generate_workflow_id("workflow-version");
    let mut stored_definition = definition.clone();
    stored_definition.version = 1;
    stored_definition.created_at = Some(now.clone());
    stored_definition.updated_at = Some(now.clone());
    let definition_json = serialize_json(&stored_definition, "workflow_definition_encode_failed")?;

    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_definition_transaction_failed",
                &database_path,
                error,
            )
        })?;
    tx.execute(
        r#"
        INSERT INTO workflow_definitions (
            id,
            project_id,
            name,
            description,
            active_version_id,
            created_at,
            updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            stored_definition.id.as_str(),
            stored_definition.project_id.as_str(),
            stored_definition.name.as_str(),
            stored_definition.description.as_str(),
            version_id.as_str(),
            now.as_str(),
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_definition_insert_failed", &database_path, error)
    })?;
    insert_definition_version(
        &tx,
        &database_path,
        &version_id,
        &stored_definition.project_id,
        &stored_definition.id,
        stored_definition.version,
        &definition_json,
        &now,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_definition_commit_failed", &database_path, error)
    })?;

    Ok(stored_definition)
}

pub fn update_workflow_definition(
    repo_root: &Path,
    workflow_id: &str,
    expected_version: u32,
    definition: &WorkflowDefinitionDto,
) -> Result<WorkflowDefinitionDto, CommandError> {
    validate_definition_identity(definition)?;
    validate_non_empty_text(
        workflow_id,
        "workflowId",
        "workflow_definition_request_invalid",
    )?;
    if workflow_id != definition.id {
        return Err(CommandError::user_fixable(
            "workflow_definition_id_mismatch",
            "Xero refused to update a Workflow because the request id and definition id differ.",
        ));
    }
    if expected_version == 0 || definition.version != expected_version {
        return Err(workflow_definition_version_conflict(workflow_id));
    }

    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(
        &connection,
        &database_path,
        repo_root,
        &definition.project_id,
    )?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_definition_transaction_failed",
                &database_path,
                error,
            )
        })?;

    let current =
        read_definition_summary_row(&tx, &database_path, &definition.project_id, workflow_id)?
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "workflow_definition_not_found",
                    format!("Xero could not find Workflow `{workflow_id}`."),
                )
            })?;
    if current.active_version_number != expected_version {
        return Err(workflow_definition_version_conflict(workflow_id));
    }
    let version_number = expected_version.checked_add(1).ok_or_else(|| {
        CommandError::system_fault(
            "workflow_definition_version_exhausted",
            format!("Workflow `{workflow_id}` cannot create another immutable version."),
        )
    })?;
    let now = now_timestamp();
    let version_id = generate_workflow_id("workflow-version");
    let mut stored_definition = definition.clone();
    stored_definition.version = version_number;
    stored_definition.created_at = Some(current.created_at);
    stored_definition.updated_at = Some(now.clone());
    let definition_json = serialize_json(&stored_definition, "workflow_definition_encode_failed")?;

    insert_definition_version(
        &tx,
        &database_path,
        &version_id,
        &definition.project_id,
        workflow_id,
        version_number,
        &definition_json,
        &now,
    )?;
    let changed = tx
        .execute(
            r#"
        UPDATE workflow_definitions
        SET name = ?3,
            description = ?4,
            active_version_id = ?5,
            updated_at = ?6
        WHERE project_id = ?1
          AND id = ?2
          AND active_version_id = ?7
        "#,
            params![
                definition.project_id.as_str(),
                workflow_id,
                stored_definition.name.as_str(),
                stored_definition.description.as_str(),
                version_id.as_str(),
                now.as_str(),
                current.active_version_id.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_definition_update_failed", &database_path, error)
        })?;
    if changed != 1 {
        return Err(workflow_definition_version_conflict(workflow_id));
    }
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_definition_commit_failed", &database_path, error)
    })?;

    Ok(stored_definition)
}

fn workflow_definition_version_conflict(workflow_id: &str) -> CommandError {
    CommandError::user_fixable(
        "workflow_definition_version_conflict",
        format!(
            "Workflow `{workflow_id}` changed since it was loaded. Reload it before saving again."
        ),
    )
}

pub fn list_workflow_definitions(
    repo_root: &Path,
    project_id: &str,
) -> Result<Vec<WorkflowDefinitionSummaryDto>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "workflow_definition_request_invalid",
    )?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let mut statement = connection
        .prepare(
            r#"
            SELECT
                definitions.id,
                definitions.project_id,
                definitions.name,
                definitions.description,
                definitions.active_version_id,
                versions.version_number,
                definitions.created_at,
                definitions.updated_at
            FROM workflow_definitions definitions
            JOIN workflow_definition_versions versions
              ON versions.project_id = definitions.project_id
             AND versions.id = definitions.active_version_id
            WHERE definitions.project_id = ?1
            ORDER BY definitions.updated_at DESC, definitions.name ASC
            "#,
        )
        .map_err(|error| {
            map_workflow_query_error("workflow_definitions_prepare_failed", &database_path, error)
        })?;
    let rows = statement
        .query_map(params![project_id], read_definition_summary_from_row)
        .map_err(|error| {
            map_workflow_query_error("workflow_definitions_query_failed", &database_path, error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_workflow_query_error("workflow_definitions_decode_failed", &database_path, error)
    })
}

pub fn get_workflow_definition(
    repo_root: &Path,
    project_id: &str,
    workflow_id: &str,
) -> Result<Option<WorkflowDefinitionDto>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "workflow_definition_request_invalid",
    )?;
    validate_non_empty_text(
        workflow_id,
        "workflowId",
        "workflow_definition_request_invalid",
    )?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    read_active_definition_json(&connection, &database_path, project_id, workflow_id)?
        .map(|json| decode_definition(&database_path, &json))
        .transpose()
}

pub fn create_workflow_run(
    repo_root: &Path,
    project_id: &str,
    workflow_id: &str,
    initial_input: Option<JsonValue>,
) -> Result<WorkflowRunDto, CommandError> {
    validate_non_empty_text(project_id, "projectId", "workflow_run_request_invalid")?;
    validate_non_empty_text(workflow_id, "workflowId", "workflow_run_request_invalid")?;
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let initial_input_json = initial_input
        .as_ref()
        .map(|value| serialize_json(value, "workflow_run_input_encode_failed"))
        .transpose()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error("workflow_run_transaction_failed", &database_path, error)
        })?;
    let version = required_active_definition_version(&tx, &database_path, project_id, workflow_id)?;
    validate_new_run_agent_refs_active(&tx, &database_path, &version)?;
    let now = now_timestamp();
    let run_id = insert_new_workflow_run(
        &tx,
        &database_path,
        project_id,
        workflow_id,
        &version,
        initial_input_json.as_deref(),
        &now,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_run_commit_failed", &database_path, error)
    })?;
    read_created_workflow_run(&connection, &database_path, project_id, &run_id)
}

pub fn create_workflow_run_idempotently(
    repo_root: &Path,
    project_id: &str,
    workflow_id: &str,
    idempotency_key: &str,
    expected_workflow_version: u32,
    initial_input: Option<JsonValue>,
) -> Result<WorkflowRunDto, CommandError> {
    validate_non_empty_text(project_id, "projectId", "workflow_run_request_invalid")?;
    validate_non_empty_text(workflow_id, "workflowId", "workflow_run_request_invalid")?;
    validate_workflow_start_idempotency_key(idempotency_key)?;

    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let initial_input_json = initial_input
        .as_ref()
        .map(|value| serialize_json(value, "workflow_run_input_encode_failed"))
        .transpose()?;
    let payload_hash =
        workflow_start_payload_hash(project_id, workflow_id, initial_input.as_ref())?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error("workflow_run_transaction_failed", &database_path, error)
        })?;

    let existing = tx
        .query_row(
            r#"
            SELECT payload_hash, workflow_run_id
            FROM workflow_run_start_requests
            WHERE project_id = ?1
              AND idempotency_key = ?2
            "#,
            params![project_id, idempotency_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_run_start_request_query_failed",
                &database_path,
                error,
            )
        })?;
    let run_id = if let Some((stored_payload_hash, run_id)) = existing {
        if stored_payload_hash != payload_hash {
            return Err(CommandError::user_fixable(
                "workflow_run_idempotency_conflict",
                "This Workflow start key was already used with a different request.",
            ));
        }
        run_id
    } else {
        let version =
            required_active_definition_version(&tx, &database_path, project_id, workflow_id)?;
        if version.version_number != expected_workflow_version {
            return Err(CommandError::retryable(
                "workflow_definition_changed_during_start",
                "The Workflow changed while Xero was starting it. Xero will retry against the latest version.",
            ));
        }
        validate_new_run_agent_refs_active(&tx, &database_path, &version)?;
        let now = now_timestamp();
        let run_id = insert_new_workflow_run(
            &tx,
            &database_path,
            project_id,
            workflow_id,
            &version,
            initial_input_json.as_deref(),
            &now,
        )?;
        tx.execute(
            r#"
            INSERT INTO workflow_run_start_requests (
                project_id,
                idempotency_key,
                payload_hash,
                workflow_run_id,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                project_id,
                idempotency_key,
                payload_hash.as_str(),
                run_id.as_str(),
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_run_start_request_insert_failed",
                &database_path,
                error,
            )
        })?;
        run_id
    };
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_run_commit_failed", &database_path, error)
    })?;
    read_created_workflow_run(&connection, &database_path, project_id, &run_id)
}

#[derive(Debug, Clone)]
pub struct WorkflowResumePhaseStartRecord {
    pub workflow_id: String,
    pub expected_workflow_version: u32,
    pub source_run_id: String,
    pub initial_input: JsonValue,
    pub loop_node_id: String,
    pub phase_id: String,
    pub phase_key: String,
    pub input_path: String,
}

pub fn create_workflow_resume_phase_run_idempotently(
    repo_root: &Path,
    project_id: &str,
    idempotency_key: &str,
    resume: &WorkflowResumePhaseStartRecord,
) -> Result<WorkflowRunDto, CommandError> {
    validate_non_empty_text(project_id, "projectId", "workflow_run_request_invalid")?;
    validate_non_empty_text(
        &resume.workflow_id,
        "workflowId",
        "workflow_run_request_invalid",
    )?;
    validate_non_empty_text(
        &resume.source_run_id,
        "sourceRunId",
        "workflow_run_request_invalid",
    )?;
    validate_workflow_start_idempotency_key(idempotency_key)?;
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let initial_input_json =
        serialize_json(&resume.initial_input, "workflow_run_input_encode_failed")?;
    let payload_hash = workflow_resume_phase_payload_hash(project_id, resume)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error("workflow_run_transaction_failed", &database_path, error)
        })?;
    let existing = tx
        .query_row(
            r#"
            SELECT payload_hash, workflow_run_id
            FROM workflow_run_start_requests
            WHERE project_id = ?1 AND idempotency_key = ?2
            "#,
            params![project_id, idempotency_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_run_start_request_query_failed",
                &database_path,
                error,
            )
        })?;
    let run_id = if let Some((stored_payload_hash, run_id)) = existing {
        if stored_payload_hash != payload_hash {
            return Err(CommandError::user_fixable(
                "workflow_run_idempotency_conflict",
                "This Workflow resume key was already used with a different phase selection.",
            ));
        }
        run_id
    } else {
        let version = required_active_definition_version(
            &tx,
            &database_path,
            project_id,
            &resume.workflow_id,
        )?;
        if version.version_number != resume.expected_workflow_version {
            return Err(CommandError::retryable(
                "workflow_definition_changed_during_start",
                "The Workflow changed while Xero was resuming its next phase.",
            ));
        }
        validate_new_run_agent_refs_active(&tx, &database_path, &version)?;
        let now = now_timestamp();
        let run_id = insert_new_workflow_run(
            &tx,
            &database_path,
            project_id,
            &resume.workflow_id,
            &version,
            Some(&initial_input_json),
            &now,
        )?;
        tx.execute(
            r#"
            INSERT INTO workflow_run_start_requests (
                project_id, idempotency_key, payload_hash, workflow_run_id, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                project_id,
                idempotency_key,
                payload_hash.as_str(),
                run_id.as_str(),
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_run_start_request_insert_failed",
                &database_path,
                error,
            )
        })?;
        insert_deterministic_workflow_event(
            &tx,
            project_id,
            &run_id,
            None,
            &format!("{run_id}:resume-next-incomplete-phase"),
            "workflow_resume_next_incomplete_phase",
            &json!({
                "sourceRunId": resume.source_run_id,
                "loopNodeId": resume.loop_node_id,
                "phaseId": resume.phase_id,
                "phaseKey": resume.phase_key,
                "inputPath": resume.input_path,
            }),
            &now,
            &database_path,
        )?;
        run_id
    };
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_run_commit_failed", &database_path, error)
    })?;
    read_created_workflow_run(&connection, &database_path, project_id, &run_id)
}

pub fn get_workflow_resume_phase_replay(
    repo_root: &Path,
    project_id: &str,
    source_run_id: &str,
    idempotency_key: &str,
) -> Result<Option<WorkflowRunDto>, CommandError> {
    validate_non_empty_text(project_id, "projectId", "workflow_run_request_invalid")?;
    validate_non_empty_text(source_run_id, "sourceRunId", "workflow_run_request_invalid")?;
    validate_workflow_start_idempotency_key(idempotency_key)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let existing = connection
        .query_row(
            r#"
            SELECT request.workflow_run_id, event.event_json
            FROM workflow_run_start_requests AS request
            LEFT JOIN workflow_events AS event
              ON event.project_id = request.project_id
             AND event.workflow_run_id = request.workflow_run_id
             AND event.id = request.workflow_run_id || ':resume-next-incomplete-phase'
             AND event.event_type = 'workflow_resume_next_incomplete_phase'
            WHERE request.project_id = ?1
              AND request.idempotency_key = ?2
            "#,
            params![project_id, idempotency_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_run_start_request_query_failed",
                &database_path,
                error,
            )
        })?;
    let Some((run_id, event_json)) = existing else {
        return Ok(None);
    };
    let event = event_json
        .as_deref()
        .map(|event| decode_json(&database_path, event, "workflow_event_decode_failed"))
        .transpose()?;
    if event.as_ref().and_then(|event| event.get("sourceRunId"))
        != Some(&JsonValue::String(source_run_id.to_owned()))
    {
        return Err(CommandError::user_fixable(
            "workflow_run_idempotency_conflict",
            "This Workflow resume key was already used for a different operation.",
        ));
    }
    read_workflow_run(&connection, &database_path, project_id, &run_id)?.map_or_else(
        || {
            Err(CommandError::system_fault(
                "workflow_run_start_replay_missing",
                "Xero found a Workflow resume request whose run is missing.",
            ))
        },
        |run| Ok(Some(run)),
    )
}

pub fn get_workflow_run_start_replay(
    repo_root: &Path,
    project_id: &str,
    workflow_id: &str,
    idempotency_key: &str,
    initial_input: Option<&JsonValue>,
) -> Result<Option<WorkflowRunDto>, CommandError> {
    validate_non_empty_text(project_id, "projectId", "workflow_run_request_invalid")?;
    validate_non_empty_text(workflow_id, "workflowId", "workflow_run_request_invalid")?;
    validate_workflow_start_idempotency_key(idempotency_key)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let payload_hash = workflow_start_payload_hash(project_id, workflow_id, initial_input)?;
    let existing = connection
        .query_row(
            r#"
            SELECT payload_hash, workflow_run_id
            FROM workflow_run_start_requests
            WHERE project_id = ?1
              AND idempotency_key = ?2
            "#,
            params![project_id, idempotency_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_run_start_request_query_failed",
                &database_path,
                error,
            )
        })?;
    let Some((stored_payload_hash, run_id)) = existing else {
        return Ok(None);
    };
    if stored_payload_hash != payload_hash {
        return Err(CommandError::user_fixable(
            "workflow_run_idempotency_conflict",
            "This Workflow start key was already used with a different request.",
        ));
    }
    read_workflow_run(&connection, &database_path, project_id, &run_id)?.map_or_else(
        || {
            Err(CommandError::system_fault(
                "workflow_run_start_replay_missing",
                "Xero found a Workflow start request whose run is missing.",
            ))
        },
        |run| Ok(Some(run)),
    )
}

fn validate_workflow_start_idempotency_key(idempotency_key: &str) -> Result<(), CommandError> {
    if idempotency_key.trim().is_empty()
        || idempotency_key.trim() != idempotency_key
        || idempotency_key.len() > 200
    {
        return Err(CommandError::user_fixable(
            "workflow_run_idempotency_key_invalid",
            "Workflow start idempotencyKey must be a non-empty string of at most 200 characters without surrounding whitespace.",
        ));
    }
    Ok(())
}

fn workflow_start_payload_hash(
    project_id: &str,
    workflow_id: &str,
    initial_input: Option<&JsonValue>,
) -> Result<String, CommandError> {
    let payload = json!({
        "projectId": project_id,
        "workflowId": workflow_id,
        "initialInput": initial_input,
    });
    let bytes = serde_json::to_vec(&payload).map_err(|error| {
        CommandError::system_fault(
            "workflow_run_start_payload_encode_failed",
            format!("Xero could not serialize the Workflow start identity: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn workflow_resume_phase_payload_hash(
    project_id: &str,
    resume: &WorkflowResumePhaseStartRecord,
) -> Result<String, CommandError> {
    // The request identity is deliberately limited to caller-owned inputs.
    // Phase selection, Workflow version, and initial input are derived by the
    // server and may change while two copies of the same request race. Once a
    // key wins, every equivalent retry must replay that winner rather than
    // conflict with a later selection snapshot.
    let payload = json!({
        "operation": "resume_next_incomplete_phase",
        "projectId": project_id,
        "sourceRunId": resume.source_run_id,
    });
    let bytes = serde_json::to_vec(&payload).map_err(|error| {
        CommandError::system_fault(
            "workflow_run_start_payload_encode_failed",
            format!("Xero could not serialize the Workflow phase resume identity: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn required_active_definition_version(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    workflow_id: &str,
) -> Result<DefinitionVersionRow, CommandError> {
    read_active_definition_version_row(connection, database_path, project_id, workflow_id)?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_definition_not_found",
                format!("Xero could not find Workflow `{workflow_id}`."),
            )
        })
}

fn validate_new_run_agent_refs_active(
    connection: &Connection,
    database_path: &Path,
    version: &DefinitionVersionRow,
) -> Result<(), CommandError> {
    let definition = decode_definition(database_path, &version.definition_json)?;
    let agent_refs = definition
        .nodes
        .iter()
        .chain(
            definition
                .subgraphs
                .iter()
                .flat_map(|subgraph| subgraph.nodes.iter()),
        )
        .filter_map(|node| match node {
            WorkflowNodeDto::Agent { agent_ref, .. } => Some(agent_ref),
            _ => None,
        });

    for agent_ref in agent_refs {
        let AgentRefDto::Custom {
            definition_id,
            version,
        } = agent_ref
        else {
            continue;
        };
        let state = connection
            .query_row(
                r#"
                SELECT definition.lifecycle_state,
                       EXISTS (
                           SELECT 1
                           FROM agent_definition_versions AS pinned
                           WHERE pinned.definition_id = definition.definition_id
                             AND pinned.version = ?2
                       )
                FROM agent_definitions AS definition
                WHERE definition.definition_id = ?1
                "#,
                params![definition_id, version],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
            )
            .optional()
            .map_err(|error| {
                map_workflow_query_error(
                    "workflow_agent_ref_validation_failed",
                    database_path,
                    error,
                )
            })?;
        match state {
            None => {
                return Err(CommandError::user_fixable(
                    "workflow_agent_ref_missing",
                    format!("Workflow agent `{definition_id}` no longer exists."),
                ));
            }
            Some((lifecycle_state, _)) if lifecycle_state != "active" => {
                return Err(CommandError::user_fixable(
                    "workflow_agent_ref_inactive",
                    format!(
                        "Workflow agent `{definition_id}` is `{lifecycle_state}` and cannot be used to start a new run."
                    ),
                ));
            }
            Some((_, false)) => {
                return Err(CommandError::user_fixable(
                    "workflow_agent_ref_version_missing",
                    format!("Workflow agent `{definition_id}` version {version} no longer exists."),
                ));
            }
            Some((_, true)) => {}
        }
    }
    Ok(())
}

fn insert_new_workflow_run(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    workflow_id: &str,
    version: &DefinitionVersionRow,
    initial_input_json: Option<&str>,
    now: &str,
) -> Result<String, CommandError> {
    let run_id = generate_workflow_id("workflow-run");
    connection
        .execute(
            r#"
            INSERT INTO workflow_runs (
                id,
                project_id,
                workflow_id,
                workflow_version_id,
                workflow_version_number,
                status,
                definition_json,
                initial_input_json,
                started_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, 'queued', ?6, ?7, ?8, ?9)
            "#,
            params![
                run_id.as_str(),
                project_id,
                workflow_id,
                version.id.as_str(),
                version.version_number,
                version.definition_json.as_str(),
                initial_input_json,
                now,
                now,
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_run_insert_failed", database_path, error)
        })?;
    insert_workflow_event_with_connection(
        connection,
        project_id,
        &run_id,
        None,
        "workflow_started",
        &json!({
            "workflowId": workflow_id,
            "workflowVersionId": version.id,
            "workflowVersionNumber": version.version_number
        }),
        now,
    )?;
    Ok(run_id)
}

fn read_created_workflow_run(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<WorkflowRunDto, CommandError> {
    read_workflow_run(connection, database_path, project_id, run_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "workflow_run_missing_after_insert",
            "Xero created a Workflow run but could not read it back.",
        )
    })
}

pub fn get_workflow_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Option<WorkflowRunDto>, CommandError> {
    validate_non_empty_text(project_id, "projectId", "workflow_run_request_invalid")?;
    validate_non_empty_text(run_id, "runId", "workflow_run_request_invalid")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    read_workflow_run(&connection, &database_path, project_id, run_id)
}

pub fn list_workflow_runs(
    repo_root: &Path,
    project_id: &str,
    workflow_id: Option<&str>,
) -> Result<Vec<WorkflowRunDto>, CommandError> {
    validate_non_empty_text(project_id, "projectId", "workflow_run_request_invalid")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT id
            FROM workflow_runs
            WHERE project_id = ?1
              AND (?2 IS NULL OR workflow_id = ?2)
            ORDER BY updated_at DESC, started_at DESC
            "#,
        )
        .map_err(|error| {
            map_workflow_query_error("workflow_runs_prepare_failed", &database_path, error)
        })?;
    let ids = statement
        .query_map(params![project_id, workflow_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| {
            map_workflow_query_error("workflow_runs_query_failed", &database_path, error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            map_workflow_query_error("workflow_runs_decode_failed", &database_path, error)
        })?;
    ids.into_iter()
        .map(|run_id| {
            read_workflow_run(&connection, &database_path, project_id, &run_id)?.ok_or_else(|| {
                CommandError::system_fault(
                    "workflow_run_missing_during_list",
                    format!("Xero listed Workflow run `{run_id}` but could not read it."),
                )
            })
        })
        .collect()
}

pub fn update_workflow_run_status(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    status: WorkflowRunStatusDto,
    terminal_status: Option<WorkflowTerminalStatusDto>,
    cancellation_reason: Option<&str>,
) -> Result<(), CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let completed_at = matches!(
        status,
        WorkflowRunStatusDto::Completed
            | WorkflowRunStatusDto::Failed
            | WorkflowRunStatusDto::Cancelled
    )
    .then_some(now.as_str());
    connection
        .execute(
            r#"
            UPDATE workflow_runs
            SET status = ?3,
                terminal_status = ?4,
                cancellation_reason = COALESCE(?5, cancellation_reason),
                completed_at = CASE
                    WHEN ?6 IS NOT NULL THEN ?6
                    WHEN ?3 IN ('queued', 'running', 'paused', 'cancelling') THEN NULL
                    ELSE completed_at
                END,
                updated_at = ?7
            WHERE project_id = ?1
              AND id = ?2
              AND (
                    status IN ('queued', 'running', 'paused')
                    OR status = ?3
                  )
            "#,
            params![
                project_id,
                run_id,
                status.as_str(),
                terminal_status.map(WorkflowTerminalStatusDto::as_str),
                cancellation_reason,
                completed_at,
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_run_status_update_failed", &database_path, error)
        })?;
    Ok(())
}

pub fn start_workflow_run_atomically(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    start_node_id: &str,
    start_node_type: &str,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!("{run_id}:execution-started");
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_run_start_transaction_failed",
                &database_path,
                error,
            )
        })?;
    if workflow_marker_exists(&tx, project_id, run_id, &marker_id, &database_path)? {
        tx.commit().map_err(|error| {
            map_workflow_write_error("workflow_run_start_commit_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    if workflow_marker_exists(
        &tx,
        project_id,
        run_id,
        &format!("{run_id}:cancellation-requested"),
        &database_path,
    )? {
        tx.rollback().map_err(|error| {
            map_workflow_write_error("workflow_run_start_rollback_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    let status = tx
        .query_row(
            "SELECT status FROM workflow_runs WHERE project_id = ?1 AND id = ?2",
            params![project_id, run_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error("workflow_run_query_failed", &database_path, error)
        })?;
    if !matches!(status.as_deref(), Some("queued" | "running")) {
        tx.rollback().map_err(|error| {
            map_workflow_write_error("workflow_run_start_rollback_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    tx.execute(
        r#"
        UPDATE workflow_runs
        SET status = 'running', terminal_status = NULL, completed_at = NULL, updated_at = ?3
        WHERE project_id = ?1 AND id = ?2 AND status = 'queued'
        "#,
        params![project_id, run_id, now.as_str()],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_run_start_update_failed", &database_path, error)
    })?;
    let node_run_id = format!("{run_id}:node:{start_node_id}:attempt:0");
    let idempotency_key = format!("{run_id}:{start_node_id}:0");
    tx.execute(
        r#"
        INSERT INTO workflow_run_nodes (
            id, project_id, workflow_run_id, node_id, node_type, status,
            attempt_number, updated_at, idempotency_key
        )
        VALUES (?1, ?2, ?3, ?4, ?5, 'eligible', 0, ?6, ?7)
        ON CONFLICT(project_id, workflow_run_id, node_id, attempt_number) DO NOTHING
        "#,
        params![
            node_run_id,
            project_id,
            run_id,
            start_node_id,
            start_node_type,
            now.as_str(),
            idempotency_key,
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_run_start_node_failed", &database_path, error)
    })?;
    let start_node_exists = tx
        .query_row(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM workflow_run_nodes
                WHERE project_id = ?1 AND workflow_run_id = ?2
                  AND node_id = ?3 AND attempt_number = 0
            )
            "#,
            params![project_id, run_id, start_node_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_run_start_node_query_failed",
                &database_path,
                error,
            )
        })?;
    if !start_node_exists {
        return Err(CommandError::system_fault(
            "workflow_run_start_node_missing",
            "The Workflow start node was not present after its atomic start transaction.",
        ));
    }
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        run_id,
        None,
        &marker_id,
        "workflow_execution_started",
        &json!({ "startNodeId": start_node_id }),
        &now,
        &database_path,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_run_start_commit_failed", &database_path, error)
    })?;
    Ok(true)
}

#[cfg(test)]
pub fn complete_workflow_run_if_active(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    status: WorkflowRunStatusDto,
    terminal_status: WorkflowTerminalStatusDto,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let event_id = generate_workflow_id("workflow-event");
    let event_json = serialize_json(
        &json!({ "terminalStatus": terminal_status.as_str() }),
        "workflow_event_encode_failed",
    )?;
    let completed_at = matches!(
        status,
        WorkflowRunStatusDto::Completed
            | WorkflowRunStatusDto::Failed
            | WorkflowRunStatusDto::Cancelled
    )
    .then_some(now.as_str());
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_completion_transaction_failed",
                &database_path,
                error,
            )
        })?;
    let changed = tx
        .execute(
            r#"
            UPDATE workflow_runs
            SET status = ?3,
                terminal_status = ?4,
                cancellation_reason = COALESCE(?5, cancellation_reason),
                completed_at = CASE
                    WHEN ?6 IS NOT NULL THEN ?6
                    WHEN ?3 IN ('queued', 'running', 'paused') THEN NULL
                    ELSE completed_at
                END,
                updated_at = ?7
            WHERE project_id = ?1
              AND id = ?2
              AND status IN ('queued', 'running', 'paused')
            "#,
            params![
                project_id,
                run_id,
                status.as_str(),
                terminal_status.as_str(),
                Option::<&str>::None,
                completed_at,
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_run_status_update_failed", &database_path, error)
        })?;
    if changed == 0 {
        tx.commit().map_err(|error| {
            map_workflow_write_error("workflow_completion_commit_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    tx.execute(
        r#"
        INSERT INTO workflow_events (
            id,
            project_id,
            workflow_run_id,
            node_run_id,
            event_type,
            event_json,
            created_at
        )
        VALUES (?1, ?2, ?3, NULL, 'workflow_completed', ?4, ?5)
        ON CONFLICT(project_id, id) DO NOTHING
        "#,
        params![event_id, project_id, run_id, event_json, now.as_str()],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_event_insert_failed", &database_path, error)
    })?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_completion_commit_failed", &database_path, error)
    })?;
    Ok(true)
}

pub fn complete_workflow_terminal_atomically(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    terminal_node_run_id: &str,
    run_status: WorkflowRunStatusDto,
    terminal_status: WorkflowTerminalStatusDto,
) -> Result<bool, CommandError> {
    if !matches!(
        run_status,
        WorkflowRunStatusDto::Completed
            | WorkflowRunStatusDto::Failed
            | WorkflowRunStatusDto::Cancelled
            | WorkflowRunStatusDto::Paused
    ) {
        return Err(CommandError::system_fault(
            "workflow_terminal_run_status_invalid",
            "A terminal node must complete, fail, cancel, or pause its Workflow run.",
        ));
    }
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!("{run_id}:terminal:{terminal_node_run_id}");
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_terminal_transaction_failed",
                &database_path,
                error,
            )
        })?;
    if workflow_marker_exists(&tx, project_id, run_id, &marker_id, &database_path)? {
        tx.commit().map_err(|error| {
            map_workflow_write_error("workflow_terminal_commit_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    if workflow_marker_exists(
        &tx,
        project_id,
        run_id,
        &format!("{run_id}:cancellation-requested"),
        &database_path,
    )? {
        tx.rollback().map_err(|error| {
            map_workflow_write_error("workflow_terminal_rollback_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    let live_execution_count =
        workflow_live_execution_count(&tx, project_id, run_id, None, &database_path)?;
    if live_execution_count != 0 {
        tx.rollback().map_err(|error| {
            map_workflow_write_error("workflow_terminal_rollback_failed", &database_path, error)
        })?;
        return Err(CommandError::retryable(
            "workflow_terminal_execution_still_active",
            "The Workflow cannot enter its terminal state until every sibling execution is terminal.",
        ));
    }
    let terminal_changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'succeeded',
                completed_at = COALESCE(completed_at, ?4),
                updated_at = ?4
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND id = ?3
              AND status IN ('eligible', 'succeeded')
            "#,
            params![project_id, run_id, terminal_node_run_id, now.as_str()],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_terminal_node_update_failed",
                &database_path,
                error,
            )
        })?;
    if terminal_changed != 1 {
        tx.rollback().map_err(|error| {
            map_workflow_write_error("workflow_terminal_rollback_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    tx.execute(
        r#"
        UPDATE workflow_run_nodes
        SET status = 'cancelled',
            failure_class = 'workflow_terminal_short_circuit',
            completed_at = ?4,
            updated_at = ?4
        WHERE project_id = ?1
          AND workflow_run_id = ?2
          AND id <> ?3
          AND status IN ('pending', 'eligible', 'starting', 'running', 'waiting_on_gate')
        "#,
        params![project_id, run_id, terminal_node_run_id, now.as_str()],
    )
    .map_err(|error| {
        map_workflow_write_error(
            "workflow_terminal_sibling_update_failed",
            &database_path,
            error,
        )
    })?;
    let completed_at = (run_status != WorkflowRunStatusDto::Paused).then_some(now.as_str());
    let run_changed = tx
        .execute(
            r#"
            UPDATE workflow_runs
            SET status = ?3,
                terminal_status = ?4,
                completed_at = ?5,
                updated_at = ?6
            WHERE project_id = ?1
              AND id = ?2
              AND status IN ('queued', 'running', 'paused')
            "#,
            params![
                project_id,
                run_id,
                run_status.as_str(),
                terminal_status.as_str(),
                completed_at,
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_terminal_run_update_failed", &database_path, error)
        })?;
    if run_changed != 1 {
        tx.rollback().map_err(|error| {
            map_workflow_write_error("workflow_terminal_rollback_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        run_id,
        Some(terminal_node_run_id),
        &marker_id,
        "workflow_completed",
        &json!({ "terminalStatus": terminal_status.as_str() }),
        &now,
        &database_path,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_terminal_commit_failed", &database_path, error)
    })?;
    Ok(true)
}

#[derive(Debug, Clone)]
pub struct WorkflowNodeRetryRecord {
    pub run_id: String,
    pub source_node_run_id: String,
    pub node_id: String,
    pub node_type: String,
}

pub fn retry_workflow_node_atomically(
    repo_root: &Path,
    project_id: &str,
    retry: &WorkflowNodeRetryRecord,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!("{}:retry:{}", retry.run_id, retry.source_node_run_id);
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error("workflow_retry_transaction_failed", &database_path, error)
        })?;
    if workflow_marker_exists(&tx, project_id, &retry.run_id, &marker_id, &database_path)? {
        tx.commit().map_err(|error| {
            map_workflow_write_error("workflow_retry_commit_failed", &database_path, error)
        })?;
        return Ok(false);
    }

    let source = tx
        .query_row(
            r#"
            WITH RECURSIVE source_agent_runs(run_id) AS (
                SELECT agent.run_id
                FROM workflow_run_nodes AS source_seed
                JOIN agent_runs AS agent
                  ON agent.project_id = source_seed.project_id
                 AND (agent.run_id = source_seed.runtime_run_id
                      OR agent.run_id = source_seed.idempotency_key)
                WHERE source_seed.project_id = ?1
                  AND source_seed.workflow_run_id = ?2
                  AND source_seed.id = ?3
                  AND source_seed.node_id = ?4
                  AND source_seed.node_type = 'agent'
                UNION
                SELECT lineage.target_run_id
                FROM source_agent_runs AS owned
                JOIN agent_handoff_lineage AS lineage
                  ON lineage.project_id = ?1
                 AND lineage.source_run_id = owned.run_id
                WHERE lineage.target_run_id IS NOT NULL
            )
            SELECT source.status,
                   EXISTS (
                       SELECT 1
                       FROM source_agent_runs AS owned
                       JOIN agent_runs AS agent
                         ON agent.project_id = source.project_id
                        AND agent.run_id = owned.run_id
                       WHERE agent.status IN ('starting', 'running', 'paused', 'cancelling')
                   ) OR EXISTS (
                       SELECT 1
                       FROM workflow_command_leases AS lease
                       WHERE lease.project_id = source.project_id
                         AND lease.node_run_id = source.id
                   )
            FROM workflow_run_nodes AS source
            WHERE source.project_id = ?1
              AND source.workflow_run_id = ?2
              AND source.id = ?3
              AND source.node_id = ?4
            "#,
            params![
                project_id,
                retry.run_id.as_str(),
                retry.source_node_run_id.as_str(),
                retry.node_id.as_str(),
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error("workflow_retry_source_query_failed", &database_path, error)
        })?;
    let Some((source_status, execution_is_live)) = source else {
        return Err(CommandError::user_fixable(
            "workflow_node_run_not_retryable",
            format!(
                "Workflow node run `{}` changed before its retry could be committed.",
                retry.source_node_run_id
            ),
        ));
    };
    if execution_is_live {
        return Err(CommandError::user_fixable(
            "workflow_node_execution_still_active",
            format!(
                "Workflow node run `{}` still owns a live execution and cannot be retried yet.",
                retry.source_node_run_id
            ),
        ));
    }
    if workflow_live_execution_count(&tx, project_id, &retry.run_id, None, &database_path)? != 0 {
        return Err(CommandError::user_fixable(
            "workflow_retry_execution_still_active",
            "A Workflow retry cannot rewind routing while another owned execution is still active.",
        ));
    }
    if !matches!(
        source_status.as_str(),
        "failed" | "stalled" | "skipped" | "cancelled"
    ) {
        return Err(CommandError::user_fixable(
            "workflow_node_run_not_retryable",
            format!(
                "Workflow node run `{}` changed before its retry could be committed.",
                retry.source_node_run_id
            ),
        ));
    }

    let descendant_node_ids = workflow_retry_descendant_node_ids(
        &tx,
        &database_path,
        project_id,
        &retry.run_id,
        &retry.node_id,
    )?;
    let invalidated_node_run_ids = workflow_retry_invalidated_node_run_ids(
        &tx,
        &database_path,
        project_id,
        &retry.run_id,
        &retry.source_node_run_id,
        &descendant_node_ids,
    )?;
    rewind_workflow_retry_evidence(
        &tx,
        &database_path,
        project_id,
        retry,
        &descendant_node_ids,
        &invalidated_node_run_ids,
        &now,
    )?;

    let reopened = tx
        .execute(
            r#"
            UPDATE workflow_runs
            SET status = 'running',
                terminal_status = NULL,
                completed_at = NULL,
                updated_at = ?3
            WHERE project_id = ?1
              AND id = ?2
              AND status IN ('running', 'paused', 'failed')
            "#,
            params![project_id, retry.run_id.as_str(), now.as_str()],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_run_retry_reopen_failed", &database_path, error)
        })?;
    if reopened != 1 {
        return Err(CommandError::user_fixable(
            "workflow_run_not_retryable",
            "The Workflow run changed state before the retry could be committed.",
        ));
    }

    let attempt = next_workflow_node_attempt(
        &tx,
        project_id,
        &retry.run_id,
        &retry.node_id,
        &database_path,
    )?;
    let retry_node_run_id = format!("{}:node:{}:attempt:{attempt}", retry.run_id, retry.node_id);
    let idempotency_key = format!("{}:{}:{attempt}", retry.run_id, retry.node_id);
    tx.execute(
        r#"
        INSERT INTO workflow_run_nodes (
            id, project_id, workflow_run_id, node_id, node_type, status,
            attempt_number, updated_at, idempotency_key
        )
        VALUES (?1, ?2, ?3, ?4, ?5, 'eligible', ?6, ?7, ?8)
        "#,
        params![
            retry_node_run_id.as_str(),
            project_id,
            retry.run_id.as_str(),
            retry.node_id.as_str(),
            retry.node_type.as_str(),
            attempt,
            now.as_str(),
            idempotency_key,
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_retry_node_insert_failed", &database_path, error)
    })?;
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        &retry.run_id,
        Some(&retry.source_node_run_id),
        &marker_id,
        "workflow_node_retry_requested",
        &json!({
            "nodeId": retry.node_id,
            "previousStatus": source_status,
            "retryNodeRunId": retry_node_run_id,
            "attemptNumber": attempt,
            "rewoundNodeRunIds": invalidated_node_run_ids
                .iter()
                .filter(|node_run_id| *node_run_id != &retry.source_node_run_id)
                .collect::<Vec<_>>(),
        }),
        &now,
        &database_path,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_retry_commit_failed", &database_path, error)
    })?;
    Ok(true)
}

fn workflow_retry_descendant_node_ids(
    tx: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
    source_node_id: &str,
) -> Result<BTreeSet<String>, CommandError> {
    let mut statement = tx
        .prepare(
            r#"
            WITH RECURSIVE descendants(node_id) AS (
                SELECT to_node_id
                FROM workflow_run_edges
                WHERE project_id = ?1
                  AND workflow_run_id = ?2
                  AND from_node_id = ?3
                UNION
                SELECT edge.to_node_id
                FROM workflow_run_edges AS edge
                JOIN descendants
                  ON descendants.node_id = edge.from_node_id
                WHERE edge.project_id = ?1
                  AND edge.workflow_run_id = ?2
            )
            SELECT node_id FROM descendants
            "#,
        )
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_retry_descendants_prepare_failed",
                database_path,
                error,
            )
        })?;
    let rows = statement
        .query_map(params![project_id, run_id, source_node_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_retry_descendants_query_failed",
                database_path,
                error,
            )
        })?;
    rows.collect::<Result<BTreeSet<_>, _>>().map_err(|error| {
        map_workflow_query_error(
            "workflow_retry_descendants_decode_failed",
            database_path,
            error,
        )
    })
}

fn workflow_retry_invalidated_node_run_ids(
    tx: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
    source_node_run_id: &str,
    descendant_node_ids: &BTreeSet<String>,
) -> Result<BTreeSet<String>, CommandError> {
    let mut ids = BTreeSet::from([source_node_run_id.to_owned()]);
    for descendant_node_id in descendant_node_ids {
        let mut statement = tx
            .prepare(
                r#"
                SELECT id
                FROM workflow_run_nodes
                WHERE project_id = ?1
                  AND workflow_run_id = ?2
                  AND node_id = ?3
                  AND id <> ?4
                "#,
            )
            .map_err(|error| {
                map_workflow_query_error(
                    "workflow_retry_node_runs_prepare_failed",
                    database_path,
                    error,
                )
            })?;
        let rows = statement
            .query_map(
                params![project_id, run_id, descendant_node_id, source_node_run_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| {
                map_workflow_query_error(
                    "workflow_retry_node_runs_query_failed",
                    database_path,
                    error,
                )
            })?;
        ids.extend(rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
            map_workflow_query_error(
                "workflow_retry_node_runs_decode_failed",
                database_path,
                error,
            )
        })?);
    }
    Ok(ids)
}

#[allow(clippy::too_many_arguments)]
fn rewind_workflow_retry_evidence(
    tx: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    retry: &WorkflowNodeRetryRecord,
    descendant_node_ids: &BTreeSet<String>,
    invalidated_node_run_ids: &BTreeSet<String>,
    now: &str,
) -> Result<(), CommandError> {
    let affected_node_ids = descendant_node_ids
        .iter()
        .cloned()
        .chain(std::iter::once(retry.node_id.clone()))
        .collect::<BTreeSet<_>>();
    let affected_loop_keys = workflow_retry_loop_keys(
        tx,
        database_path,
        project_id,
        &retry.run_id,
        &affected_node_ids,
    )?;
    for node_run_id in invalidated_node_run_ids {
        tx.execute(
            "DELETE FROM workflow_artifacts WHERE project_id = ?1 AND workflow_run_id = ?2 AND producer_node_run_id = ?3",
            params![project_id, retry.run_id.as_str(), node_run_id],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_retry_artifact_rewind_failed", database_path, error)
        })?;
        tx.execute(
            "DELETE FROM workflow_gate_decisions WHERE project_id = ?1 AND workflow_run_id = ?2 AND node_run_id = ?3",
            params![project_id, retry.run_id.as_str(), node_run_id],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_retry_gate_rewind_failed", database_path, error)
        })?;
        if node_run_id != &retry.source_node_run_id {
            tx.execute(
                r#"
                UPDATE workflow_run_nodes
                SET status = 'cancelled',
                    failure_class = 'workflow_retry_rewind',
                    completed_at = ?4,
                    updated_at = ?4
                WHERE project_id = ?1
                  AND workflow_run_id = ?2
                  AND id = ?3
                "#,
                params![project_id, retry.run_id.as_str(), node_run_id, now],
            )
            .map_err(|error| {
                map_workflow_write_error("workflow_retry_node_rewind_failed", database_path, error)
            })?;
        }
    }
    for node_id in descendant_node_ids
        .iter()
        .chain(std::iter::once(&retry.node_id))
    {
        tx.execute(
            "DELETE FROM workflow_run_edges WHERE project_id = ?1 AND workflow_run_id = ?2 AND from_node_id = ?3",
            params![project_id, retry.run_id.as_str(), node_id],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_retry_edge_rewind_failed", database_path, error)
        })?;
    }
    for loop_key in affected_loop_keys {
        tx.execute(
            "DELETE FROM workflow_loop_attempts WHERE project_id = ?1 AND workflow_run_id = ?2 AND loop_key = ?3",
            params![project_id, retry.run_id.as_str(), loop_key],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_retry_loop_rewind_failed", database_path, error)
        })?;
    }
    Ok(())
}

fn workflow_retry_loop_keys(
    tx: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
    affected_node_ids: &BTreeSet<String>,
) -> Result<BTreeSet<String>, CommandError> {
    const SUBGRAPH_NODE_SEPARATOR: &str = "::";

    let definition_json = tx
        .query_row(
            "SELECT definition_json FROM workflow_runs WHERE project_id = ?1 AND id = ?2",
            params![project_id, run_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_retry_definition_query_failed",
                database_path,
                error,
            )
        })?;
    let definition = decode_definition(database_path, &definition_json)?;
    let mut loop_keys = definition
        .edges
        .iter()
        .filter(|edge| affected_node_ids.contains(&edge.from_node_id))
        .filter_map(|edge| {
            edge.loop_policy
                .as_ref()
                .map(|policy| policy.loop_key.clone())
        })
        .collect::<BTreeSet<_>>();

    // Subgraph node runs and their loop keys are namespaced by every invocation
    // in the runtime. Reconstruct those concrete identities from the immutable
    // definition snapshot so a retry only resets the loops that can actually be
    // reached from its runtime node id.
    let mut invocations = definition
        .nodes
        .iter()
        .filter_map(|node| match node {
            WorkflowNodeDto::Subgraph {
                id, subgraph_id, ..
            } => {
                let mut ancestry = BTreeSet::new();
                ancestry.insert(subgraph_id.clone());
                Some((id.clone(), subgraph_id.clone(), ancestry))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    while let Some((parent_node_id, subgraph_id, ancestry)) = invocations.pop() {
        let Some(subgraph) = definition
            .subgraphs
            .iter()
            .find(|subgraph| subgraph.id == subgraph_id)
        else {
            continue;
        };
        for edge in &subgraph.edges {
            let runtime_source = format!(
                "{parent_node_id}{SUBGRAPH_NODE_SEPARATOR}{}",
                edge.from_node_id
            );
            if !affected_node_ids.contains(&runtime_source) {
                continue;
            }
            if let Some(policy) = &edge.loop_policy {
                loop_keys.insert(if policy.loop_key.contains(SUBGRAPH_NODE_SEPARATOR) {
                    policy.loop_key.clone()
                } else {
                    format!(
                        "{parent_node_id}{SUBGRAPH_NODE_SEPARATOR}{}",
                        policy.loop_key
                    )
                });
            }
        }
        for node in &subgraph.nodes {
            let WorkflowNodeDto::Subgraph {
                id, subgraph_id, ..
            } = node
            else {
                continue;
            };
            let mut nested_ancestry = ancestry.clone();
            if nested_ancestry.insert(subgraph_id.clone()) {
                invocations.push((
                    format!("{parent_node_id}{SUBGRAPH_NODE_SEPARATOR}{id}"),
                    subgraph_id.clone(),
                    nested_ancestry,
                ));
            }
        }
    }

    Ok(loop_keys)
}

#[derive(Debug, Clone)]
pub struct WorkflowBranchSkipRecord {
    pub run_id: String,
    pub node_run_id: String,
    pub node_id: String,
    pub previous_status: WorkflowNodeRunStatusDto,
    pub reason: Option<String>,
    pub merge_targets: Vec<(String, String)>,
}

pub fn skip_workflow_branch_atomically(
    repo_root: &Path,
    project_id: &str,
    skip: &WorkflowBranchSkipRecord,
) -> Result<bool, CommandError> {
    if skip.merge_targets.is_empty() {
        return Err(CommandError::user_fixable(
            "workflow_branch_skip_requires_merge_target",
            format!(
                "Workflow node `{}` has no direct Merge target, so its branch cannot be skipped safely.",
                skip.node_id
            ),
        ));
    }
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!("{}:skip:{}", skip.run_id, skip.node_run_id);
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error("workflow_skip_transaction_failed", &database_path, error)
        })?;
    if workflow_marker_exists(&tx, project_id, &skip.run_id, &marker_id, &database_path)? {
        tx.commit().map_err(|error| {
            map_workflow_write_error("workflow_skip_commit_failed", &database_path, error)
        })?;
        return Ok(false);
    }

    let changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'skipped',
                failure_class = 'skipped_by_user',
                completed_at = ?4,
                updated_at = ?4
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND id = ?3
              AND status IN ('pending', 'eligible', 'starting', 'running', 'waiting_on_gate')
              AND NOT EXISTS (
                    SELECT 1
                    FROM agent_runs AS agent
                    WHERE agent.project_id = workflow_run_nodes.project_id
                      AND (agent.run_id = workflow_run_nodes.runtime_run_id
                           OR agent.run_id = workflow_run_nodes.idempotency_key)
                      AND agent.status IN ('starting', 'running', 'paused', 'cancelling')
                  )
              AND NOT EXISTS (
                    SELECT 1
                    FROM workflow_command_leases AS lease
                    WHERE lease.project_id = workflow_run_nodes.project_id
                      AND lease.node_run_id = workflow_run_nodes.id
                  )
            "#,
            params![
                project_id,
                skip.run_id.as_str(),
                skip.node_run_id.as_str(),
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_skip_node_update_failed", &database_path, error)
        })?;
    if changed != 1 {
        return Err(CommandError::user_fixable(
            "workflow_node_run_not_skippable",
            format!(
                "Workflow node run `{}` changed before its skip could be committed.",
                skip.node_run_id
            ),
        ));
    }

    let mut merge_target_node_ids = Vec::with_capacity(skip.merge_targets.len());
    for (node_id, node_type) in &skip.merge_targets {
        let existing_attempt = tx
            .query_row(
                r#"
                SELECT attempt_number
                FROM workflow_run_nodes
                WHERE project_id = ?1
                  AND workflow_run_id = ?2
                  AND node_id = ?3
                  AND status IN ('pending', 'eligible', 'starting', 'running', 'waiting_on_gate')
                ORDER BY attempt_number DESC
                LIMIT 1
                "#,
                params![project_id, skip.run_id.as_str(), node_id.as_str()],
                |row| row.get::<_, u32>(0),
            )
            .optional()
            .map_err(|error| {
                map_workflow_query_error("workflow_skip_merge_query_failed", &database_path, error)
            })?;
        if existing_attempt.is_none() {
            let attempt =
                next_workflow_node_attempt(&tx, project_id, &skip.run_id, node_id, &database_path)?;
            let node_run_id = format!("{}:node:{node_id}:attempt:{attempt}", skip.run_id);
            let idempotency_key = format!("{}:{node_id}:{attempt}", skip.run_id);
            tx.execute(
                r#"
                INSERT INTO workflow_run_nodes (
                    id, project_id, workflow_run_id, node_id, node_type, status,
                    attempt_number, updated_at, idempotency_key
                )
                VALUES (?1, ?2, ?3, ?4, ?5, 'eligible', ?6, ?7, ?8)
                "#,
                params![
                    node_run_id,
                    project_id,
                    skip.run_id.as_str(),
                    node_id.as_str(),
                    node_type.as_str(),
                    attempt,
                    now.as_str(),
                    idempotency_key,
                ],
            )
            .map_err(|error| {
                map_workflow_write_error("workflow_skip_merge_insert_failed", &database_path, error)
            })?;
        }
        merge_target_node_ids.push(node_id.clone());
    }

    let reopened = tx
        .execute(
            r#"
            UPDATE workflow_runs
            SET status = 'running',
                terminal_status = NULL,
                completed_at = NULL,
                updated_at = ?3
            WHERE project_id = ?1
              AND id = ?2
              AND status IN ('queued', 'running', 'paused')
            "#,
            params![project_id, skip.run_id.as_str(), now.as_str()],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_skip_run_reopen_failed", &database_path, error)
        })?;
    if reopened != 1 {
        return Err(CommandError::retryable(
            "workflow_skip_run_reopen_conflict",
            "The Workflow run changed before its branch skip could be committed.",
        ));
    }
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        &skip.run_id,
        Some(&skip.node_run_id),
        &marker_id,
        "workflow_branch_skipped",
        &json!({
            "nodeId": skip.node_id,
            "previousStatus": skip.previous_status.as_str(),
            "reason": skip.reason,
            "mergeTargetNodeIds": merge_target_node_ids,
        }),
        &now,
        &database_path,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_skip_commit_failed", &database_path, error)
    })?;
    Ok(true)
}

pub fn stall_workflow_agent_for_activity_timeout(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    event: &JsonValue,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!("{run_id}:activity-timeout:{node_run_id}");
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_activity_timeout_transaction_failed",
                &database_path,
                error,
            )
        })?;
    if workflow_marker_exists(&tx, project_id, run_id, &marker_id, &database_path)? {
        tx.commit().map_err(|error| {
            map_workflow_write_error(
                "workflow_activity_timeout_commit_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    let changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'stalled',
                failure_class = 'runtime_activity_timeout',
                completed_at = ?4,
                updated_at = ?4
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND id = ?3
              AND status = 'running'
              AND NOT EXISTS (
                    SELECT 1
                    FROM agent_runs AS agent
                    WHERE agent.project_id = workflow_run_nodes.project_id
                      AND (agent.run_id = workflow_run_nodes.runtime_run_id
                           OR agent.run_id = workflow_run_nodes.idempotency_key)
                      AND agent.status IN ('starting', 'running', 'paused', 'cancelling')
                  )
            "#,
            params![project_id, run_id, node_run_id, now.as_str()],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_activity_timeout_node_update_failed",
                &database_path,
                error,
            )
        })?;
    if changed == 0 {
        tx.rollback().map_err(|error| {
            map_workflow_write_error(
                "workflow_activity_timeout_rollback_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        run_id,
        Some(node_run_id),
        &marker_id,
        "workflow_node_stalled",
        event,
        &now,
        &database_path,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error(
            "workflow_activity_timeout_commit_failed",
            &database_path,
            error,
        )
    })?;
    Ok(true)
}

pub fn fail_workflow_node_with_event_atomically(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    node_id: &str,
    event_type: &str,
    failure_class: &str,
    message: &str,
) -> Result<bool, CommandError> {
    validate_non_empty_text(event_type, "eventType", "workflow_event_type_invalid")?;
    validate_non_empty_text(
        failure_class,
        "failureClass",
        "workflow_failure_class_invalid",
    )?;
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!("{run_id}:node-failure:{node_run_id}:{event_type}");
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_node_failure_transaction_failed",
                &database_path,
                error,
            )
        })?;
    if workflow_marker_exists(&tx, project_id, run_id, &marker_id, &database_path)? {
        tx.commit().map_err(|error| {
            map_workflow_write_error("workflow_node_failure_commit_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    let changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'failed',
                failure_class = ?5,
                completed_at = ?6,
                updated_at = ?6
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND id = ?3
              AND node_id = ?4
              AND status IN ('pending', 'eligible', 'starting', 'running', 'waiting_on_gate')
              AND EXISTS (
                    SELECT 1
                    FROM workflow_runs AS run
                    WHERE run.project_id = workflow_run_nodes.project_id
                      AND run.id = workflow_run_nodes.workflow_run_id
                      AND run.status = 'running'
                  )
            "#,
            params![
                project_id,
                run_id,
                node_run_id,
                node_id,
                failure_class,
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_node_failure_update_failed", &database_path, error)
        })?;
    if changed != 1 {
        tx.rollback().map_err(|error| {
            map_workflow_write_error(
                "workflow_node_failure_rollback_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        run_id,
        Some(node_run_id),
        &marker_id,
        event_type,
        &json!({
            "nodeId": node_id,
            "failureClass": failure_class,
            "message": message,
        }),
        &now,
        &database_path,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_node_failure_commit_failed", &database_path, error)
    })?;
    Ok(true)
}

pub fn pause_workflow_checkpoint_atomically(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    node_id: &str,
    reason: &str,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!("{run_id}:checkpoint-pause:{node_run_id}");
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_checkpoint_pause_transaction_failed",
                &database_path,
                error,
            )
        })?;
    if workflow_marker_exists(&tx, project_id, run_id, &marker_id, &database_path)? {
        tx.commit().map_err(|error| {
            map_workflow_write_error(
                "workflow_checkpoint_pause_commit_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    let node_changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'waiting_on_gate', updated_at = ?4
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND id = ?3
              AND status = 'eligible'
              AND EXISTS (
                    SELECT 1
                    FROM workflow_runs
                    WHERE workflow_runs.project_id = workflow_run_nodes.project_id
                      AND workflow_runs.id = workflow_run_nodes.workflow_run_id
                      AND workflow_runs.status = 'running'
                  )
            "#,
            params![project_id, run_id, node_run_id, now.as_str()],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_checkpoint_pause_node_failed",
                &database_path,
                error,
            )
        })?;
    let run_changed = tx
        .execute(
            r#"
            UPDATE workflow_runs
            SET status = 'paused',
                terminal_status = 'needs_human',
                completed_at = NULL,
                updated_at = ?3
            WHERE project_id = ?1
              AND id = ?2
              AND status = 'running'
            "#,
            params![project_id, run_id, now.as_str()],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_checkpoint_pause_run_failed",
                &database_path,
                error,
            )
        })?;
    if node_changed != 1 || run_changed != 1 {
        return Err(CommandError::retryable(
            "workflow_checkpoint_pause_conflict",
            "The Workflow changed before its checkpoint pause could be committed.",
        ));
    }
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        run_id,
        Some(node_run_id),
        &marker_id,
        "workflow_paused",
        &json!({ "reason": reason, "nodeId": node_id }),
        &now,
        &database_path,
    )?;
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        run_id,
        Some(node_run_id),
        &format!("{marker_id}:metric"),
        "workflow_metric_recorded",
        &json!({
            "metric": "checkpoint_pause",
            "reason": reason,
            "nodeId": node_id,
        }),
        &now,
        &database_path,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error(
            "workflow_checkpoint_pause_commit_failed",
            &database_path,
            error,
        )
    })?;
    Ok(true)
}

pub fn complete_workflow_state_query_node_atomically(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    node_id: &str,
    query: &WorkflowStateQueryDto,
    output_artifact_type: &str,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!("{run_id}:state-query:{node_run_id}");
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_state_query_transaction_failed",
                &database_path,
                error,
            )
        })?;
    if workflow_marker_exists(&tx, project_id, run_id, &marker_id, &database_path)? {
        tx.commit().map_err(|error| {
            map_workflow_write_error("workflow_state_query_commit_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    if !claim_eligible_workflow_node(&tx, project_id, run_id, node_run_id, &now, &database_path)? {
        tx.rollback().map_err(|error| {
            map_workflow_write_error(
                "workflow_state_query_rollback_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    let payload = query_delivery_state_with_connection(&tx, project_id, query)?;
    let record_count = payload
        .get("count")
        .and_then(JsonValue::as_u64)
        .unwrap_or(0);
    insert_deterministic_workflow_artifact(
        &tx,
        project_id,
        run_id,
        node_run_id,
        &format!("{run_id}:state-query-artifact:{node_run_id}"),
        output_artifact_type,
        1,
        &payload,
        Some(&format!(
            "{} {record_count} record(s)",
            query.entity_type.as_str()
        )),
        &now,
        &database_path,
    )?;
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        run_id,
        Some(node_run_id),
        &marker_id,
        "workflow_state_read",
        &json!({
            "nodeId": node_id,
            "entityType": query.entity_type.as_str(),
            "recordCount": record_count,
        }),
        &now,
        &database_path,
    )?;
    finish_claimed_workflow_node(&tx, project_id, node_run_id, &now, &database_path)?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_state_query_commit_failed", &database_path, error)
    })?;
    Ok(true)
}

pub fn complete_workflow_state_write_node_atomically(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    node_id: &str,
    operation: &WorkflowStateWriteOperationDto,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!("{run_id}:state-write:{node_run_id}");
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_state_write_transaction_failed",
                &database_path,
                error,
            )
        })?;
    if workflow_marker_exists(&tx, project_id, run_id, &marker_id, &database_path)? {
        tx.commit().map_err(|error| {
            map_workflow_write_error("workflow_state_write_commit_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    if !claim_eligible_workflow_node(&tx, project_id, run_id, node_run_id, &now, &database_path)? {
        tx.rollback().map_err(|error| {
            map_workflow_write_error(
                "workflow_state_write_rollback_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    let operation =
        workflow_state_write_operation_with_stable_identity(run_id, node_id, None, operation)?;
    let operation_hash = workflow_state_write_operation_hash(&operation)?;
    let payload = match replay_workflow_state_write(
        &tx,
        &database_path,
        project_id,
        run_id,
        &operation,
        &operation_hash,
    )? {
        Some(payload) => payload,
        None => write_delivery_state_with_connection(
            &tx,
            project_id,
            DeliveryStateWriteContext {
                workflow_run_id: Some(run_id),
                node_run_id: Some(node_run_id),
            },
            &operation,
        )?,
    };
    let render_text = payload
        .get("record")
        .and_then(|record| record.get("title"))
        .and_then(JsonValue::as_str)
        .or_else(|| payload.get("id").and_then(JsonValue::as_str));
    insert_deterministic_workflow_artifact(
        &tx,
        project_id,
        run_id,
        node_run_id,
        &format!("{run_id}:state-write-artifact:{node_run_id}"),
        &operation.output_artifact_type,
        1,
        &payload,
        render_text,
        &now,
        &database_path,
    )?;
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        run_id,
        Some(node_run_id),
        &marker_id,
        "workflow_state_written",
        &json!({
            "nodeId": node_id,
            "entityType": operation.entity_type.as_str(),
            "action": operation.action.as_str(),
            "entityId": payload.get("id"),
            "idempotencyKey": operation.idempotency_key,
            "operationHash": operation_hash,
            "result": payload,
        }),
        &now,
        &database_path,
    )?;
    finish_claimed_workflow_node(&tx, project_id, node_run_id, &now, &database_path)?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_state_write_commit_failed", &database_path, error)
    })?;
    Ok(true)
}

fn workflow_state_write_operation_with_stable_identity(
    run_id: &str,
    node_id: &str,
    operation_index: Option<usize>,
    operation: &WorkflowStateWriteOperationDto,
) -> Result<WorkflowStateWriteOperationDto, CommandError> {
    let mut operation = operation.clone();
    if operation.idempotency_key.is_some() {
        return Ok(operation);
    }
    let identity = json!({
        "workflowRunId": run_id,
        "nodeId": node_id,
        "operationIndex": operation_index,
        "entityType": operation.entity_type.as_str(),
        "action": operation.action.as_str(),
        "targetId": operation.target_id.as_deref(),
        "payload": &operation.payload,
    });
    let bytes = serde_json::to_vec(&identity).map_err(|error| {
        CommandError::system_fault(
            "workflow_state_write_identity_encode_failed",
            format!("Xero could not encode a Workflow state-write identity: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    operation.idempotency_key = Some(format!("workflow-state-{:x}", hasher.finalize()));
    Ok(operation)
}

fn workflow_state_write_operation_hash(
    operation: &WorkflowStateWriteOperationDto,
) -> Result<String, CommandError> {
    let bytes = serde_json::to_vec(operation).map_err(|error| {
        CommandError::system_fault(
            "workflow_state_write_operation_encode_failed",
            format!("Xero could not encode a Workflow state-write operation: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn replay_workflow_state_write(
    tx: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
    operation: &WorkflowStateWriteOperationDto,
    operation_hash: &str,
) -> Result<Option<JsonValue>, CommandError> {
    let idempotency_key = operation.idempotency_key.as_deref().ok_or_else(|| {
        CommandError::system_fault(
            "workflow_state_write_identity_missing",
            "Workflow state writes require a durable logical identity.",
        )
    })?;
    let prior_event_json = tx
        .query_row(
            r#"
            SELECT event_json
            FROM workflow_events
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND event_type IN ('workflow_state_written', 'workflow_checkpoint_state_written')
              AND json_extract(event_json, '$.idempotencyKey') = ?3
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            "#,
            params![project_id, run_id, idempotency_key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_state_write_replay_query_failed",
                database_path,
                error,
            )
        })?;
    let Some(prior_event_json) = prior_event_json else {
        return Ok(None);
    };
    let prior_event = decode_json(
        database_path,
        &prior_event_json,
        "workflow_state_write_replay_decode_failed",
    )?;
    if prior_event.get("operationHash").and_then(JsonValue::as_str) != Some(operation_hash) {
        return Err(CommandError::user_fixable(
            "workflow_state_write_idempotency_conflict",
            "A Workflow state-write identity was already used for a different resolved operation.",
        ));
    }
    prior_event.get("result").cloned().map(Some).ok_or_else(|| {
        CommandError::system_fault(
            "workflow_state_write_replay_result_missing",
            "Xero found a committed Workflow state write without its replay result.",
        )
    })
}

#[derive(Debug, Clone)]
pub struct WorkflowPreparedStateNodeCompletionRecord {
    pub run_id: String,
    pub node_run_id: String,
    pub artifact_type: String,
    pub payload: JsonValue,
    pub render_text: Option<String>,
    pub event_type: String,
    pub event: JsonValue,
}

pub fn complete_prepared_workflow_state_node_atomically(
    repo_root: &Path,
    project_id: &str,
    completion: &WorkflowPreparedStateNodeCompletionRecord,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!(
        "{}:state-node:{}",
        completion.run_id, completion.node_run_id
    );
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_state_node_transaction_failed",
                &database_path,
                error,
            )
        })?;
    if workflow_marker_exists(
        &tx,
        project_id,
        &completion.run_id,
        &marker_id,
        &database_path,
    )? {
        tx.commit().map_err(|error| {
            map_workflow_write_error("workflow_state_node_commit_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    if !claim_eligible_workflow_node(
        &tx,
        project_id,
        &completion.run_id,
        &completion.node_run_id,
        &now,
        &database_path,
    )? {
        tx.rollback().map_err(|error| {
            map_workflow_write_error("workflow_state_node_rollback_failed", &database_path, error)
        })?;
        return Ok(false);
    }
    insert_deterministic_workflow_artifact(
        &tx,
        project_id,
        &completion.run_id,
        &completion.node_run_id,
        &format!(
            "{}:state-artifact:{}",
            completion.run_id, completion.node_run_id
        ),
        &completion.artifact_type,
        1,
        &completion.payload,
        completion.render_text.as_deref(),
        &now,
        &database_path,
    )?;
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        &completion.run_id,
        Some(&completion.node_run_id),
        &marker_id,
        &completion.event_type,
        &completion.event,
        &now,
        &database_path,
    )?;
    finish_claimed_workflow_node(
        &tx,
        project_id,
        &completion.node_run_id,
        &now,
        &database_path,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_state_node_commit_failed", &database_path, error)
    })?;
    Ok(true)
}

#[derive(Debug, Clone)]
pub struct WorkflowSubgraphStartRecord {
    pub run_id: String,
    pub parent_node_run_id: String,
    pub parent_node_id: String,
    pub subgraph_id: String,
    pub input_artifact_type: String,
    pub input_payload: JsonValue,
    pub output_artifact_type: String,
    pub child_node_id: String,
    pub child_node_type: String,
}

pub fn start_workflow_subgraph_atomically(
    repo_root: &Path,
    project_id: &str,
    start: &WorkflowSubgraphStartRecord,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!(
        "{}:subgraph-start:{}",
        start.run_id, start.parent_node_run_id
    );
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_subgraph_start_transaction_failed",
                &database_path,
                error,
            )
        })?;
    if workflow_marker_exists(&tx, project_id, &start.run_id, &marker_id, &database_path)? {
        tx.commit().map_err(|error| {
            map_workflow_write_error(
                "workflow_subgraph_start_commit_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    let parent_changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'running',
                started_at = COALESCE(started_at, ?4),
                updated_at = ?4
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND id = ?3
              AND status = 'eligible'
              AND EXISTS (
                    SELECT 1
                    FROM workflow_runs
                    WHERE workflow_runs.project_id = workflow_run_nodes.project_id
                      AND workflow_runs.id = workflow_run_nodes.workflow_run_id
                      AND workflow_runs.status = 'running'
                  )
            "#,
            params![
                project_id,
                start.run_id.as_str(),
                start.parent_node_run_id.as_str(),
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_subgraph_parent_start_failed",
                &database_path,
                error,
            )
        })?;
    if parent_changed != 1 {
        tx.rollback().map_err(|error| {
            map_workflow_write_error(
                "workflow_subgraph_start_rollback_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    insert_deterministic_workflow_artifact(
        &tx,
        project_id,
        &start.run_id,
        &start.parent_node_run_id,
        &format!(
            "{}:subgraph-input:{}",
            start.run_id, start.parent_node_run_id
        ),
        &start.input_artifact_type,
        1,
        &start.input_payload,
        Some("Subgraph input"),
        &now,
        &database_path,
    )?;
    let child_attempt = next_workflow_node_attempt(
        &tx,
        project_id,
        &start.run_id,
        &start.child_node_id,
        &database_path,
    )?;
    let child_node_run_id = format!(
        "{}:node:{}:attempt:{child_attempt}",
        start.run_id, start.child_node_id
    );
    let child_idempotency_key = format!("{}:{}:{child_attempt}", start.run_id, start.child_node_id);
    tx.execute(
        r#"
        INSERT INTO workflow_run_nodes (
            id, project_id, workflow_run_id, node_id, node_type, status,
            attempt_number, updated_at, idempotency_key
        )
        VALUES (?1, ?2, ?3, ?4, ?5, 'eligible', ?6, ?7, ?8)
        "#,
        params![
            child_node_run_id.as_str(),
            project_id,
            start.run_id.as_str(),
            start.child_node_id.as_str(),
            start.child_node_type.as_str(),
            child_attempt,
            now.as_str(),
            child_idempotency_key,
        ],
    )
    .map_err(|error| {
        map_workflow_write_error(
            "workflow_subgraph_child_insert_failed",
            &database_path,
            error,
        )
    })?;
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        &start.run_id,
        Some(&start.parent_node_run_id),
        &marker_id,
        "workflow_subgraph_started",
        &json!({
            "nodeId": start.parent_node_id,
            "subgraphId": start.subgraph_id,
            "childNodeId": start.child_node_id,
            "childNodeRunId": child_node_run_id,
            "outputArtifactType": start.output_artifact_type,
        }),
        &now,
        &database_path,
    )?;
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        &start.run_id,
        Some(&start.parent_node_run_id),
        &format!("{marker_id}:child"),
        "workflow_subgraph_child_scheduled",
        &json!({
            "nodeId": start.parent_node_id,
            "subgraphId": start.subgraph_id,
            "childNodeId": start.child_node_id,
            "childNodeRunId": child_node_run_id,
        }),
        &now,
        &database_path,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error(
            "workflow_subgraph_start_commit_failed",
            &database_path,
            error,
        )
    })?;
    Ok(true)
}

#[derive(Debug, Clone)]
pub struct WorkflowSubgraphCompletionRecord {
    pub run_id: String,
    pub terminal_node_run_id: String,
    pub terminal_node_id: String,
    pub parent_node_run_id: String,
    pub parent_node_id: String,
    pub parent_status: WorkflowNodeRunStatusDto,
    pub parent_failure_class: Option<String>,
    pub pause_run: bool,
    pub artifact_type: String,
    pub schema_version: u32,
    pub payload: JsonValue,
    pub render_text: Option<String>,
    pub edge_evidence: JsonValue,
    pub terminal_event: JsonValue,
    pub parent_event: JsonValue,
}

pub fn complete_workflow_subgraph_atomically(
    repo_root: &Path,
    project_id: &str,
    completion: &WorkflowSubgraphCompletionRecord,
) -> Result<bool, CommandError> {
    if !matches!(
        completion.parent_status,
        WorkflowNodeRunStatusDto::Succeeded
            | WorkflowNodeRunStatusDto::Failed
            | WorkflowNodeRunStatusDto::Cancelled
            | WorkflowNodeRunStatusDto::WaitingOnGate
    ) {
        return Err(CommandError::system_fault(
            "workflow_subgraph_completion_status_invalid",
            "A subgraph parent may only complete, cancel, fail, or wait for a human.",
        ));
    }
    if completion.pause_run != (completion.parent_status == WorkflowNodeRunStatusDto::WaitingOnGate)
    {
        return Err(CommandError::system_fault(
            "workflow_subgraph_pause_status_invalid",
            "A subgraph may pause its run only while its parent waits on a gate.",
        ));
    }
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!(
        "{}:subgraph-complete:{}",
        completion.run_id, completion.terminal_node_run_id
    );
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_subgraph_completion_transaction_failed",
                &database_path,
                error,
            )
        })?;
    if workflow_marker_exists(
        &tx,
        project_id,
        &completion.run_id,
        &marker_id,
        &database_path,
    )? {
        tx.commit().map_err(|error| {
            map_workflow_write_error(
                "workflow_subgraph_completion_commit_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    let terminal_changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'succeeded', completed_at = ?4, updated_at = ?4
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND id = ?3
              AND status = 'eligible'
            "#,
            params![
                project_id,
                completion.run_id.as_str(),
                completion.terminal_node_run_id.as_str(),
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_subgraph_terminal_update_failed",
                &database_path,
                error,
            )
        })?;
    let parent_completed_at = (!completion.pause_run).then_some(now.as_str());
    let parent_changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = ?4,
                failure_class = ?5,
                completed_at = ?6,
                updated_at = ?7
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND id = ?3
              AND status = 'running'
              AND EXISTS (
                    SELECT 1
                    FROM workflow_runs
                    WHERE workflow_runs.project_id = workflow_run_nodes.project_id
                      AND workflow_runs.id = workflow_run_nodes.workflow_run_id
                      AND workflow_runs.status = 'running'
                  )
            "#,
            params![
                project_id,
                completion.run_id.as_str(),
                completion.parent_node_run_id.as_str(),
                completion.parent_status.as_str(),
                completion.parent_failure_class.as_deref(),
                parent_completed_at,
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_subgraph_parent_update_failed",
                &database_path,
                error,
            )
        })?;
    if terminal_changed != 1 || parent_changed != 1 {
        tx.rollback().map_err(|error| {
            map_workflow_write_error(
                "workflow_subgraph_completion_rollback_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    insert_deterministic_workflow_artifact(
        &tx,
        project_id,
        &completion.run_id,
        &completion.parent_node_run_id,
        &format!(
            "{}:subgraph-artifact:{}",
            completion.run_id, completion.parent_node_run_id
        ),
        &completion.artifact_type,
        completion.schema_version,
        &completion.payload,
        completion.render_text.as_deref(),
        &now,
        &database_path,
    )?;
    let condition_json = serialize_json(
        &json!({ "kind": "always" }),
        "workflow_edge_condition_encode_failed",
    )?;
    let evidence_json = serialize_json(
        &completion.edge_evidence,
        "workflow_edge_evidence_encode_failed",
    )?;
    tx.execute(
        r#"
        INSERT INTO workflow_run_edges (
            id, project_id, workflow_run_id, from_node_id, to_node_id,
            edge_id, matched, condition_json, evidence_json, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, '__subgraph_terminal__', 1, ?6, ?7, ?8)
        ON CONFLICT(project_id, id) DO NOTHING
        "#,
        params![
            format!(
                "{}:subgraph-edge:{}",
                completion.run_id, completion.terminal_node_run_id
            ),
            project_id,
            completion.run_id.as_str(),
            completion.terminal_node_id.as_str(),
            completion.parent_node_id.as_str(),
            condition_json,
            evidence_json,
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error(
            "workflow_subgraph_edge_insert_failed",
            &database_path,
            error,
        )
    })?;
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        &completion.run_id,
        Some(&completion.terminal_node_run_id),
        &marker_id,
        "workflow_subgraph_terminal_completed",
        &completion.terminal_event,
        &now,
        &database_path,
    )?;
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        &completion.run_id,
        Some(&completion.parent_node_run_id),
        &format!("{marker_id}:parent"),
        "workflow_subgraph_completed",
        &completion.parent_event,
        &now,
        &database_path,
    )?;
    if completion.pause_run {
        let paused = tx
            .execute(
                r#"
                UPDATE workflow_runs
                SET status = 'paused',
                    terminal_status = 'needs_human',
                    completed_at = NULL,
                    updated_at = ?3
                WHERE project_id = ?1
                  AND id = ?2
                  AND status = 'running'
                "#,
                params![project_id, completion.run_id.as_str(), now.as_str()],
            )
            .map_err(|error| {
                map_workflow_write_error("workflow_subgraph_pause_failed", &database_path, error)
            })?;
        if paused != 1 {
            return Err(CommandError::retryable(
                "workflow_subgraph_pause_conflict",
                "The Workflow changed before its subgraph pause could be committed.",
            ));
        }
    }
    tx.commit().map_err(|error| {
        map_workflow_write_error(
            "workflow_subgraph_completion_commit_failed",
            &database_path,
            error,
        )
    })?;
    Ok(true)
}

fn workflow_marker_exists(
    tx: &Transaction<'_>,
    project_id: &str,
    run_id: &str,
    marker_id: &str,
    database_path: &Path,
) -> Result<bool, CommandError> {
    tx.query_row(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM workflow_events
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND id = ?3
        )
        "#,
        params![project_id, run_id, marker_id],
        |row| row.get(0),
    )
    .map_err(|error| map_workflow_query_error("workflow_marker_query_failed", database_path, error))
}

fn workflow_live_execution_count(
    tx: &Transaction<'_>,
    project_id: &str,
    run_id: &str,
    excluded_node_run_id: Option<&str>,
    database_path: &Path,
) -> Result<i64, CommandError> {
    tx.query_row(
        r#"
        WITH RECURSIVE workflow_agent_runs(node_run_id, run_id) AS (
            SELECT node.id, agent.run_id
            FROM workflow_run_nodes AS node
            JOIN agent_runs AS agent
              ON agent.project_id = node.project_id
             AND (agent.run_id = node.runtime_run_id
                  OR agent.run_id = node.idempotency_key)
            WHERE node.project_id = ?1
              AND node.workflow_run_id = ?2
              AND node.node_type = 'agent'
              AND (?3 IS NULL OR node.id <> ?3)
            UNION
            SELECT owned.node_run_id, lineage.target_run_id
            FROM workflow_agent_runs AS owned
            JOIN agent_handoff_lineage AS lineage
              ON lineage.project_id = ?1
             AND lineage.source_run_id = owned.run_id
            WHERE lineage.target_run_id IS NOT NULL
        )
        SELECT COUNT(*)
        FROM workflow_run_nodes AS node
        WHERE node.project_id = ?1
          AND node.workflow_run_id = ?2
          AND (?3 IS NULL OR node.id <> ?3)
          AND (
                EXISTS (
                    SELECT 1
                    FROM workflow_agent_runs AS owned
                    JOIN agent_runs AS agent
                      ON agent.project_id = node.project_id
                     AND agent.run_id = owned.run_id
                    WHERE owned.node_run_id = node.id
                      AND agent.status IN ('starting', 'running', 'paused', 'cancelling')
                )
                OR EXISTS (
                    SELECT 1
                    FROM workflow_command_leases AS lease
                    WHERE lease.project_id = node.project_id
                      AND lease.node_run_id = node.id
                )
              )
        "#,
        params![project_id, run_id, excluded_node_run_id],
        |row| row.get(0),
    )
    .map_err(|error| {
        map_workflow_query_error("workflow_live_execution_query_failed", database_path, error)
    })
}

fn next_workflow_node_attempt(
    tx: &Transaction<'_>,
    project_id: &str,
    run_id: &str,
    node_id: &str,
    database_path: &Path,
) -> Result<u32, CommandError> {
    let next = tx
        .query_row(
            r#"
            SELECT COALESCE(MAX(attempt_number) + 1, 0)
            FROM workflow_run_nodes
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND node_id = ?3
            "#,
            params![project_id, run_id, node_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| {
            map_workflow_query_error("workflow_node_attempt_query_failed", database_path, error)
        })?;
    u32::try_from(next).map_err(|_| {
        CommandError::system_fault(
            "workflow_node_attempt_overflow",
            format!("Workflow node `{node_id}` exhausted its attempt number range."),
        )
    })
}

fn claim_eligible_workflow_node(
    tx: &Transaction<'_>,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    now: &str,
    database_path: &Path,
) -> Result<bool, CommandError> {
    tx.execute(
        r#"
        UPDATE workflow_run_nodes
        SET status = 'starting',
            started_at = COALESCE(started_at, ?4),
            updated_at = ?4
        WHERE project_id = ?1
          AND workflow_run_id = ?2
          AND id = ?3
          AND status = 'eligible'
          AND EXISTS (
                SELECT 1
                FROM workflow_runs
                WHERE workflow_runs.project_id = workflow_run_nodes.project_id
                  AND workflow_runs.id = workflow_run_nodes.workflow_run_id
                  AND workflow_runs.status = 'running'
              )
        "#,
        params![project_id, run_id, node_run_id, now],
    )
    .map(|changed| changed == 1)
    .map_err(|error| {
        map_workflow_write_error("workflow_state_node_claim_failed", database_path, error)
    })
}

fn finish_claimed_workflow_node(
    tx: &Transaction<'_>,
    project_id: &str,
    node_run_id: &str,
    now: &str,
    database_path: &Path,
) -> Result<(), CommandError> {
    let changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'succeeded', completed_at = ?3, updated_at = ?3
            WHERE project_id = ?1
              AND id = ?2
              AND status = 'starting'
            "#,
            params![project_id, node_run_id, now],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_state_node_finish_failed", database_path, error)
        })?;
    if changed == 1 {
        Ok(())
    } else {
        Err(CommandError::retryable(
            "workflow_state_node_finish_conflict",
            format!("Workflow state node `{node_run_id}` changed before completion could commit."),
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_deterministic_workflow_artifact(
    tx: &Transaction<'_>,
    project_id: &str,
    run_id: &str,
    producer_node_run_id: &str,
    artifact_id: &str,
    artifact_type: &str,
    schema_version: u32,
    payload: &JsonValue,
    render_text: Option<&str>,
    now: &str,
    database_path: &Path,
) -> Result<(), CommandError> {
    let payload_json = serialize_json(payload, "workflow_artifact_encode_failed")?;
    tx.execute(
        r#"
        INSERT INTO workflow_artifacts (
            id, project_id, workflow_run_id, producer_node_run_id, artifact_type,
            schema_version, payload_json, render_text, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(project_id, id) DO NOTHING
        "#,
        params![
            artifact_id,
            project_id,
            run_id,
            producer_node_run_id,
            artifact_type,
            schema_version,
            payload_json,
            render_text,
            now,
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_artifact_insert_failed", database_path, error)
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_deterministic_workflow_event(
    tx: &Transaction<'_>,
    project_id: &str,
    run_id: &str,
    node_run_id: Option<&str>,
    event_id: &str,
    event_type: &str,
    event: &JsonValue,
    now: &str,
    database_path: &Path,
) -> Result<(), CommandError> {
    let event_json = serialize_json(event, "workflow_event_encode_failed")?;
    tx.execute(
        r#"
        INSERT INTO workflow_events (
            id, project_id, workflow_run_id, node_run_id, event_type, event_json, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(project_id, id) DO NOTHING
        "#,
        params![
            event_id,
            project_id,
            run_id,
            node_run_id,
            event_type,
            event_json,
            now,
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_event_insert_failed", database_path, error)
    })?;
    Ok(())
}

pub fn request_workflow_run_cancellation(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    reason: Option<&str>,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_id = format!("{run_id}:cancellation-requested");
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_cancel_request_transaction_failed",
                &database_path,
                error,
            )
        })?;
    if workflow_marker_exists(&tx, project_id, run_id, &marker_id, &database_path)? {
        tx.commit().map_err(|error| {
            map_workflow_write_error(
                "workflow_cancel_request_commit_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    let changed = tx
        .execute(
            r#"
            UPDATE workflow_runs
            SET status = 'cancelling',
                terminal_status = NULL,
                cancellation_reason = COALESCE(cancellation_reason, ?3),
                completed_at = NULL,
                updated_at = ?4
            WHERE project_id = ?1
              AND id = ?2
              AND status IN ('queued', 'running', 'paused')
            "#,
            params![project_id, run_id, reason, now.as_str()],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_cancel_request_update_failed",
                &database_path,
                error,
            )
        })?;
    if changed != 1 {
        let status = tx
            .query_row(
                "SELECT status FROM workflow_runs WHERE project_id = ?1 AND id = ?2",
                params![project_id, run_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| {
                map_workflow_query_error("workflow_run_query_failed", &database_path, error)
            })?;
        if status.as_deref() == Some("cancelled") {
            tx.commit().map_err(|error| {
                map_workflow_write_error(
                    "workflow_cancel_request_commit_failed",
                    &database_path,
                    error,
                )
            })?;
            return Ok(false);
        }
        return Err(CommandError::user_fixable(
            "workflow_run_not_cancellable",
            status.map_or_else(
                || format!("Xero could not find Workflow run `{run_id}`."),
                |status| {
                    format!("Workflow run `{run_id}` cannot be cancelled while it is `{status}`.")
                },
            ),
        ));
    }
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        run_id,
        None,
        &marker_id,
        "workflow_cancellation_requested",
        &json!({ "reason": reason }),
        &now,
        &database_path,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error(
            "workflow_cancel_request_commit_failed",
            &database_path,
            error,
        )
    })?;
    Ok(true)
}

pub fn workflow_run_cancellation_pending(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .query_row(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM workflow_events AS requested
                WHERE requested.project_id = ?1
                  AND requested.workflow_run_id = ?2
                  AND requested.id = ?2 || ':cancellation-requested'
                  AND requested.event_type = 'workflow_cancellation_requested'
                  AND NOT EXISTS (
                      SELECT 1
                      FROM workflow_events AS completed
                      WHERE completed.project_id = requested.project_id
                        AND completed.workflow_run_id = requested.workflow_run_id
                        AND completed.id = ?2 || ':cancelled'
                        AND completed.event_type = 'workflow_cancelled'
                  )
            )
            "#,
            params![project_id, run_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_cancel_request_query_failed",
                &database_path,
                error,
            )
        })
}

pub fn cancel_workflow_run_execution(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    reason: Option<&str>,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let event_id = format!("{run_id}:cancelled");
    let event_json = serialize_json(&json!({ "reason": reason }), "workflow_event_encode_failed")?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error("workflow_cancel_transaction_failed", &database_path, error)
        })?;
    if !workflow_marker_exists(
        &tx,
        project_id,
        run_id,
        &format!("{run_id}:cancellation-requested"),
        &database_path,
    )? {
        tx.rollback().map_err(|error| {
            map_workflow_write_error("workflow_cancel_rollback_failed", &database_path, error)
        })?;
        return Err(CommandError::retryable(
            "workflow_cancellation_intent_missing",
            "Workflow cancellation must persist its intent before execution shutdown begins.",
        ));
    }
    if workflow_live_execution_count(&tx, project_id, run_id, None, &database_path)? != 0 {
        tx.rollback().map_err(|error| {
            map_workflow_write_error("workflow_cancel_rollback_failed", &database_path, error)
        })?;
        return Err(CommandError::retryable(
            "workflow_cancellation_execution_still_active",
            "The Workflow cannot finish cancellation until every owned execution is terminal.",
        ));
    }
    let changed = tx
        .execute(
            r#"
            UPDATE workflow_runs
            SET status = 'cancelled',
                terminal_status = 'cancelled',
                cancellation_reason = COALESCE(?3, cancellation_reason),
                completed_at = ?4,
                updated_at = ?4
            WHERE project_id = ?1
              AND id = ?2
              AND status = 'cancelling'
            "#,
            params![project_id, run_id, reason, now.as_str()],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_run_cancel_failed", &database_path, error)
        })?;
    if changed == 0 {
        let current_status = tx
            .query_row(
                "SELECT status FROM workflow_runs WHERE project_id = ?1 AND id = ?2",
                params![project_id, run_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| {
                map_workflow_query_error("workflow_run_query_failed", &database_path, error)
            })?;
        if current_status.as_deref() == Some("cancelled") {
            tx.commit().map_err(|error| {
                map_workflow_write_error("workflow_cancel_commit_failed", &database_path, error)
            })?;
            return Ok(false);
        }
        return Err(CommandError::user_fixable(
            "workflow_run_not_cancellable",
            match current_status {
                Some(status) => {
                    format!("Workflow run `{run_id}` cannot be cancelled while it is `{status}`.")
                }
                None => format!("Xero could not find Workflow run `{run_id}`."),
            },
        ));
    }

    tx.execute(
        r#"
        UPDATE workflow_run_nodes
        SET status = 'cancelled',
            failure_class = 'cancelled',
            completed_at = ?3,
            updated_at = ?3
        WHERE project_id = ?1
          AND workflow_run_id = ?2
          AND status IN ('pending', 'eligible', 'starting', 'running', 'waiting_on_gate')
        "#,
        params![project_id, run_id, now.as_str()],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_run_nodes_cancel_failed", &database_path, error)
    })?;
    tx.execute(
        r#"
        INSERT INTO workflow_events (
            id,
            project_id,
            workflow_run_id,
            node_run_id,
            event_type,
            event_json,
            created_at
        )
        VALUES (?1, ?2, ?3, NULL, 'workflow_cancelled', ?4, ?5)
        ON CONFLICT(project_id, id) DO NOTHING
        "#,
        params![event_id, project_id, run_id, event_json, now.as_str()],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_event_insert_failed", &database_path, error)
    })?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_cancel_commit_failed", &database_path, error)
    })?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
pub fn insert_workflow_run_node(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    node_id: &str,
    node_type: &str,
    attempt_number: u32,
    status: WorkflowNodeRunStatusDto,
    idempotency_key: &str,
) -> Result<WorkflowRunNodeDto, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let id = format!("{run_id}:node:{node_id}:attempt:{attempt_number}");
    connection
        .execute(
            r#"
            INSERT INTO workflow_run_nodes (
                id,
                project_id,
                workflow_run_id,
                node_id,
                node_type,
                status,
                attempt_number,
                updated_at,
                idempotency_key
            )
            SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9
            WHERE EXISTS (
                SELECT 1
                FROM workflow_runs
                WHERE project_id = ?2
                  AND id = ?3
                  AND status IN ('queued', 'running', 'paused')
            )
            ON CONFLICT(project_id, workflow_run_id, node_id, attempt_number) DO NOTHING
            "#,
            params![
                id.as_str(),
                project_id,
                run_id,
                node_id,
                node_type,
                status.as_str(),
                attempt_number,
                now.as_str(),
                idempotency_key,
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_run_node_insert_failed", &database_path, error)
        })?;
    read_workflow_run_node_by_key(
        &connection,
        &database_path,
        project_id,
        run_id,
        node_id,
        attempt_number,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "workflow_run_node_missing_after_insert",
            format!("Xero could not read Workflow node `{node_id}` after creating it."),
        )
    })
}

pub fn update_workflow_run_node(
    repo_root: &Path,
    project_id: &str,
    node_run_id: &str,
    status: WorkflowNodeRunStatusDto,
    runtime_run_id: Option<&str>,
    agent_session_id: Option<&str>,
    failure_class: Option<&str>,
) -> Result<(), CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_run_node_transaction_failed",
                &database_path,
                error,
            )
        })?;
    let now = now_timestamp();
    let started_at = matches!(
        status,
        WorkflowNodeRunStatusDto::Starting | WorkflowNodeRunStatusDto::Running
    )
    .then_some(now.as_str());
    let completed_at = matches!(
        status,
        WorkflowNodeRunStatusDto::Succeeded
            | WorkflowNodeRunStatusDto::Failed
            | WorkflowNodeRunStatusDto::Stalled
            | WorkflowNodeRunStatusDto::Skipped
            | WorkflowNodeRunStatusDto::Cancelled
    )
    .then_some(now.as_str());
    tx.execute(
        r#"
            UPDATE workflow_run_nodes
            SET status = ?3,
                runtime_run_id = COALESCE(?4, runtime_run_id),
                agent_session_id = COALESCE(?5, agent_session_id),
                failure_class = COALESCE(?6, failure_class),
                started_at = COALESCE(started_at, ?7),
                completed_at = CASE WHEN ?8 IS NULL THEN completed_at ELSE ?8 END,
                updated_at = ?9
            WHERE project_id = ?1
              AND id = ?2
              AND status NOT IN ('succeeded', 'failed', 'stalled', 'skipped', 'cancelled')
              AND EXISTS (
                    SELECT 1
                    FROM workflow_runs
                    WHERE workflow_runs.project_id = workflow_run_nodes.project_id
                      AND workflow_runs.id = workflow_run_nodes.workflow_run_id
                      AND workflow_runs.status IN ('queued', 'running', 'paused')
                  )
            "#,
        params![
            project_id,
            node_run_id,
            status.as_str(),
            runtime_run_id,
            agent_session_id,
            failure_class,
            started_at,
            completed_at,
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_run_node_update_failed", &database_path, error)
    })?;
    if completed_at.is_some() {
        tx.execute(
            "DELETE FROM workflow_command_leases WHERE project_id = ?1 AND node_run_id = ?2",
            params![project_id, node_run_id],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_command_lease_release_failed",
                &database_path,
                error,
            )
        })?;
    }
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_run_node_commit_failed", &database_path, error)
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn compare_and_set_workflow_run_node(
    repo_root: &Path,
    project_id: &str,
    node_run_id: &str,
    expected_statuses: &[WorkflowNodeRunStatusDto],
    status: WorkflowNodeRunStatusDto,
    runtime_run_id: Option<&str>,
    agent_session_id: Option<&str>,
    failure_class: Option<&str>,
) -> Result<bool, CommandError> {
    if expected_statuses.is_empty() {
        return Err(CommandError::system_fault(
            "workflow_node_transition_invalid",
            "A Workflow node transition must declare at least one expected status.",
        ));
    }
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_run_node_transaction_failed",
                &database_path,
                error,
            )
        })?;
    let now = now_timestamp();
    let started_at = matches!(
        status,
        WorkflowNodeRunStatusDto::Starting | WorkflowNodeRunStatusDto::Running
    )
    .then_some(now.as_str());
    let completed_at = matches!(
        status,
        WorkflowNodeRunStatusDto::Succeeded
            | WorkflowNodeRunStatusDto::Failed
            | WorkflowNodeRunStatusDto::Stalled
            | WorkflowNodeRunStatusDto::Skipped
            | WorkflowNodeRunStatusDto::Cancelled
    )
    .then_some(now.as_str());
    let expected_statuses = expected_statuses
        .iter()
        .map(|expected| expected.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = ?3,
                runtime_run_id = COALESCE(?4, runtime_run_id),
                agent_session_id = COALESCE(?5, agent_session_id),
                failure_class = COALESCE(?6, failure_class),
                started_at = COALESCE(started_at, ?7),
                completed_at = CASE WHEN ?8 IS NULL THEN completed_at ELSE ?8 END,
                updated_at = ?9
            WHERE project_id = ?1
              AND id = ?2
              AND instr(',' || ?10 || ',', ',' || status || ',') > 0
              AND EXISTS (
                    SELECT 1
                    FROM workflow_runs
                    WHERE workflow_runs.project_id = workflow_run_nodes.project_id
                      AND workflow_runs.id = workflow_run_nodes.workflow_run_id
                      AND workflow_runs.status IN ('queued', 'running', 'paused')
                  )
            "#,
            params![
                project_id,
                node_run_id,
                status.as_str(),
                runtime_run_id,
                agent_session_id,
                failure_class,
                started_at,
                completed_at,
                now.as_str(),
                expected_statuses,
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_run_node_transition_failed", &database_path, error)
        })?;
    if changed == 1 && completed_at.is_some() {
        tx.execute(
            "DELETE FROM workflow_command_leases WHERE project_id = ?1 AND node_run_id = ?2",
            params![project_id, node_run_id],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_command_lease_release_failed",
                &database_path,
                error,
            )
        })?;
    }
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_run_node_commit_failed", &database_path, error)
    })?;
    Ok(changed == 1)
}

pub fn claim_workflow_run_node_starting(
    repo_root: &Path,
    project_id: &str,
    node_run_id: &str,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let changed = connection
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'starting',
                started_at = COALESCE(started_at, ?3),
                updated_at = ?3
            WHERE project_id = ?1
              AND id = ?2
              AND status = 'eligible'
              AND EXISTS (
                    SELECT 1
                    FROM workflow_runs
                    WHERE workflow_runs.project_id = workflow_run_nodes.project_id
                      AND workflow_runs.id = workflow_run_nodes.workflow_run_id
                      AND workflow_runs.status = 'running'
                  )
            "#,
            params![project_id, node_run_id, now.as_str()],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_run_node_claim_failed", &database_path, error)
        })?;
    Ok(changed == 1)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowCommandLeaseRecord {
    pub project_id: String,
    pub workflow_run_id: String,
    pub node_run_id: String,
    pub owner_instance_id: String,
    pub owner_process_id: u32,
    pub owner_process_birth_identity: String,
    pub lease_token: String,
    pub command_process_id: Option<u32>,
    pub command_process_birth_identity: Option<String>,
    pub acquired_at: String,
    pub heartbeat_at: String,
}

#[allow(clippy::too_many_arguments)]
pub fn claim_workflow_command_node_starting(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    owner_instance_id: &str,
    owner_process_id: u32,
    owner_process_birth_identity: &str,
    lease_token: &str,
    now: &str,
) -> Result<bool, CommandError> {
    validate_workflow_command_lease_identity(
        owner_instance_id,
        owner_process_id,
        owner_process_birth_identity,
        lease_token,
    )?;
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_command_lease_transaction_failed",
                &database_path,
                error,
            )
        })?;
    let changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'starting',
                started_at = COALESCE(started_at, ?4),
                updated_at = ?4
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND id = ?3
              AND node_type = 'command'
              AND status = 'eligible'
              AND EXISTS (
                    SELECT 1
                    FROM workflow_runs
                    WHERE workflow_runs.project_id = workflow_run_nodes.project_id
                      AND workflow_runs.id = workflow_run_nodes.workflow_run_id
                      AND workflow_runs.status = 'running'
                  )
            "#,
            params![project_id, run_id, node_run_id, now],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_run_node_claim_failed", &database_path, error)
        })?;
    if changed == 0 {
        tx.commit().map_err(|error| {
            map_workflow_write_error(
                "workflow_command_lease_commit_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    tx.execute(
        r#"
        INSERT INTO workflow_command_leases (
            project_id,
            workflow_run_id,
            node_run_id,
            owner_instance_id,
            owner_process_id,
            owner_process_birth_identity,
            lease_token,
            command_process_id,
            acquired_at,
            heartbeat_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?8)
        "#,
        params![
            project_id,
            run_id,
            node_run_id,
            owner_instance_id,
            i64::from(owner_process_id),
            owner_process_birth_identity,
            lease_token,
            now,
        ],
    )
    .map_err(|error| {
        map_workflow_write_error(
            "workflow_command_lease_insert_failed",
            &database_path,
            error,
        )
    })?;
    tx.commit().map_err(|error| {
        map_workflow_write_error(
            "workflow_command_lease_commit_failed",
            &database_path,
            error,
        )
    })?;
    Ok(true)
}

pub fn load_workflow_command_lease(
    repo_root: &Path,
    project_id: &str,
    node_run_id: &str,
) -> Result<Option<WorkflowCommandLeaseRecord>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .query_row(
            r#"
            SELECT
                project_id,
                workflow_run_id,
                node_run_id,
                owner_instance_id,
                owner_process_id,
                owner_process_birth_identity,
                lease_token,
                command_process_id,
                command_process_birth_identity,
                acquired_at,
                heartbeat_at
            FROM workflow_command_leases
            WHERE project_id = ?1 AND node_run_id = ?2
            "#,
            params![project_id, node_run_id],
            read_workflow_command_lease_row,
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error("workflow_command_lease_query_failed", &database_path, error)
        })
}

pub fn attach_workflow_command_process(
    repo_root: &Path,
    project_id: &str,
    node_run_id: &str,
    owner_instance_id: &str,
    lease_token: &str,
    command_process_id: u32,
    command_process_birth_identity: Option<&str>,
    heartbeat_at: &str,
) -> Result<bool, CommandError> {
    if command_process_id == 0 {
        return Err(CommandError::system_fault(
            "workflow_command_process_id_invalid",
            "A Workflow command process id must be positive.",
        ));
    }
    if command_process_birth_identity.is_some_and(|identity| identity.trim().is_empty()) {
        return Err(CommandError::system_fault(
            "workflow_command_process_birth_identity_invalid",
            "A Workflow command process birth identity cannot be empty.",
        ));
    }
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let changed = connection
        .execute(
            r#"
            UPDATE workflow_command_leases
            SET command_process_id = ?5,
                command_process_birth_identity = ?6,
                heartbeat_at = ?7
            WHERE project_id = ?1
              AND node_run_id = ?2
              AND owner_instance_id = ?3
              AND lease_token = ?4
              AND command_process_id IS NULL
            "#,
            params![
                project_id,
                node_run_id,
                owner_instance_id,
                lease_token,
                i64::from(command_process_id),
                command_process_birth_identity,
                heartbeat_at,
            ],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_command_process_attach_failed",
                &database_path,
                error,
            )
        })?;
    Ok(changed == 1)
}

pub fn renew_workflow_command_lease(
    repo_root: &Path,
    project_id: &str,
    node_run_id: &str,
    owner_instance_id: &str,
    lease_token: &str,
    heartbeat_at: &str,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let changed = connection
        .execute(
            r#"
            UPDATE workflow_command_leases
            SET heartbeat_at = ?5
            WHERE project_id = ?1
              AND node_run_id = ?2
              AND owner_instance_id = ?3
              AND lease_token = ?4
            "#,
            params![
                project_id,
                node_run_id,
                owner_instance_id,
                lease_token,
                heartbeat_at,
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_command_lease_renew_failed", &database_path, error)
        })?;
    Ok(changed == 1)
}

pub fn release_workflow_command_lease(
    repo_root: &Path,
    project_id: &str,
    node_run_id: &str,
    owner_instance_id: &str,
    lease_token: &str,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let changed = connection
        .execute(
            r#"
            DELETE FROM workflow_command_leases
            WHERE project_id = ?1
              AND node_run_id = ?2
              AND owner_instance_id = ?3
              AND lease_token = ?4
            "#,
            params![project_id, node_run_id, owner_instance_id, lease_token],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_command_lease_release_failed",
                &database_path,
                error,
            )
        })?;
    Ok(changed == 1)
}

pub fn claim_interrupted_workflow_command(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    lease: &WorkflowCommandLeaseRecord,
    failure_class: &str,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_command_recovery_transaction_failed",
                &database_path,
                error,
            )
        })?;
    let deleted = tx
        .execute(
            r#"
            DELETE FROM workflow_command_leases
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND node_run_id = ?3
              AND owner_instance_id = ?4
              AND owner_process_id = ?5
              AND lease_token = ?6
              AND heartbeat_at = ?7
            "#,
            params![
                project_id,
                run_id,
                lease.node_run_id.as_str(),
                lease.owner_instance_id.as_str(),
                i64::from(lease.owner_process_id),
                lease.lease_token.as_str(),
                lease.heartbeat_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_command_lease_recovery_failed",
                &database_path,
                error,
            )
        })?;
    if deleted == 0 {
        tx.commit().map_err(|error| {
            map_workflow_write_error(
                "workflow_command_recovery_commit_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    let changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'stalled',
                failure_class = ?4,
                completed_at = ?5,
                updated_at = ?5
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND id = ?3
              AND status IN ('starting', 'running')
            "#,
            params![
                project_id,
                run_id,
                lease.node_run_id.as_str(),
                failure_class,
                now
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_command_recovery_failed", &database_path, error)
        })?;
    if changed != 1 {
        return Err(CommandError::retryable(
            "workflow_command_recovery_conflict",
            format!(
                "Workflow command node `{}` changed while its expired lease was being recovered.",
                lease.node_run_id
            ),
        ));
    }
    tx.commit().map_err(|error| {
        map_workflow_write_error(
            "workflow_command_recovery_commit_failed",
            &database_path,
            error,
        )
    })?;
    Ok(true)
}

fn validate_workflow_command_lease_identity(
    owner_instance_id: &str,
    owner_process_id: u32,
    owner_process_birth_identity: &str,
    lease_token: &str,
) -> Result<(), CommandError> {
    if owner_instance_id.trim().is_empty()
        || owner_process_birth_identity.trim().is_empty()
        || lease_token.trim().is_empty()
        || owner_process_id == 0
    {
        return Err(CommandError::system_fault(
            "workflow_command_lease_identity_invalid",
            "Workflow command ownership requires a non-empty instance id, positive process id, and non-empty lease token.",
        ));
    }
    Ok(())
}

fn read_workflow_command_lease_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<WorkflowCommandLeaseRecord> {
    let owner_process_id = row.get::<_, i64>(4)?;
    let command_process_id = row.get::<_, Option<i64>>(7)?;
    Ok(WorkflowCommandLeaseRecord {
        project_id: row.get(0)?,
        workflow_run_id: row.get(1)?,
        node_run_id: row.get(2)?,
        owner_instance_id: row.get(3)?,
        owner_process_id: u32::try_from(owner_process_id).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        owner_process_birth_identity: row.get(5)?,
        lease_token: row.get(6)?,
        command_process_id: command_process_id
            .map(u32::try_from)
            .transpose()
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Integer,
                    Box::new(error),
                )
            })?,
        command_process_birth_identity: row.get(8)?,
        acquired_at: row.get(9)?,
        heartbeat_at: row.get(10)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowDriverLeaseRecord {
    pub project_id: String,
    pub workflow_run_id: String,
    pub owner_instance_id: String,
    pub owner_process_id: u32,
    pub owner_process_birth_identity: String,
    pub lease_token: String,
    pub acquired_at: String,
    pub heartbeat_at: String,
}

#[allow(clippy::too_many_arguments)]
pub fn claim_workflow_driver_lease(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    owner_instance_id: &str,
    owner_process_id: u32,
    owner_process_birth_identity: &str,
    lease_token: &str,
    expected_existing: Option<&WorkflowDriverLeaseRecord>,
    heartbeat_at: &str,
) -> Result<bool, CommandError> {
    validate_workflow_driver_lease_identity(
        owner_instance_id,
        owner_process_id,
        owner_process_birth_identity,
        lease_token,
    )?;
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_driver_lease_transaction_failed",
                &database_path,
                error,
            )
        })?;
    if let Some(expected) = expected_existing {
        let removed = tx
            .execute(
                r#"
                DELETE FROM workflow_driver_leases
                WHERE project_id = ?1
                  AND workflow_run_id = ?2
                  AND owner_instance_id = ?3
                  AND owner_process_id = ?4
                  AND owner_process_birth_identity = ?5
                  AND lease_token = ?6
                  AND heartbeat_at = ?7
                "#,
                params![
                    project_id,
                    run_id,
                    expected.owner_instance_id.as_str(),
                    i64::from(expected.owner_process_id),
                    expected.owner_process_birth_identity.as_str(),
                    expected.lease_token.as_str(),
                    expected.heartbeat_at.as_str(),
                ],
            )
            .map_err(|error| {
                map_workflow_write_error(
                    "workflow_driver_lease_takeover_failed",
                    &database_path,
                    error,
                )
            })?;
        if removed != 1 {
            tx.rollback().map_err(|error| {
                map_workflow_write_error(
                    "workflow_driver_lease_rollback_failed",
                    &database_path,
                    error,
                )
            })?;
            return Ok(false);
        }
    }
    let inserted = tx
        .execute(
            r#"
            INSERT INTO workflow_driver_leases (
                project_id, workflow_run_id, owner_instance_id, owner_process_id,
                owner_process_birth_identity, lease_token, acquired_at, heartbeat_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
            ON CONFLICT(project_id, workflow_run_id) DO NOTHING
            "#,
            params![
                project_id,
                run_id,
                owner_instance_id,
                i64::from(owner_process_id),
                owner_process_birth_identity,
                lease_token,
                heartbeat_at,
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_driver_lease_insert_failed", &database_path, error)
        })?;
    if inserted != 1 {
        tx.rollback().map_err(|error| {
            map_workflow_write_error(
                "workflow_driver_lease_rollback_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_driver_lease_commit_failed", &database_path, error)
    })?;
    Ok(true)
}

pub fn load_workflow_driver_lease(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Option<WorkflowDriverLeaseRecord>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .query_row(
            r#"
            SELECT project_id, workflow_run_id, owner_instance_id, owner_process_id,
                   owner_process_birth_identity, lease_token, acquired_at, heartbeat_at
            FROM workflow_driver_leases
            WHERE project_id = ?1 AND workflow_run_id = ?2
            "#,
            params![project_id, run_id],
            |row| {
                let owner_process_id = row.get::<_, i64>(3)?;
                Ok(WorkflowDriverLeaseRecord {
                    project_id: row.get(0)?,
                    workflow_run_id: row.get(1)?,
                    owner_instance_id: row.get(2)?,
                    owner_process_id: u32::try_from(owner_process_id).map_err(|_| {
                        rusqlite::Error::IntegralValueOutOfRange(3, owner_process_id)
                    })?,
                    owner_process_birth_identity: row.get(4)?,
                    lease_token: row.get(5)?,
                    acquired_at: row.get(6)?,
                    heartbeat_at: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error("workflow_driver_lease_query_failed", &database_path, error)
        })
}

pub fn renew_workflow_driver_lease(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    owner_instance_id: &str,
    lease_token: &str,
    heartbeat_at: &str,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .execute(
            r#"
            UPDATE workflow_driver_leases
            SET heartbeat_at = ?6
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND owner_instance_id = ?3
              AND owner_process_id = ?4
              AND lease_token = ?5
            "#,
            params![
                project_id,
                run_id,
                owner_instance_id,
                i64::from(std::process::id()),
                lease_token,
                heartbeat_at,
            ],
        )
        .map(|changed| changed == 1)
        .map_err(|error| {
            map_workflow_write_error("workflow_driver_lease_renew_failed", &database_path, error)
        })
}

pub fn release_workflow_driver_lease(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    owner_instance_id: &str,
    lease_token: &str,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .execute(
            r#"
            DELETE FROM workflow_driver_leases
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND owner_instance_id = ?3
              AND lease_token = ?4
            "#,
            params![project_id, run_id, owner_instance_id, lease_token],
        )
        .map(|changed| changed == 1)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_driver_lease_release_failed",
                &database_path,
                error,
            )
        })
}

fn validate_workflow_driver_lease_identity(
    owner_instance_id: &str,
    owner_process_id: u32,
    owner_process_birth_identity: &str,
    lease_token: &str,
) -> Result<(), CommandError> {
    if owner_instance_id.trim().is_empty()
        || owner_process_birth_identity.trim().is_empty()
        || lease_token.trim().is_empty()
        || owner_process_id == 0
    {
        return Err(CommandError::system_fault(
            "workflow_driver_lease_identity_invalid",
            "Workflow driver ownership requires a non-empty instance id, positive process id, and non-empty lease token.",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct WorkflowDriverFailureRecord {
    pub run_id: String,
    pub incident_id: String,
    pub failure_class: String,
    pub event: JsonValue,
    pub owner_instance_id: String,
    pub lease_token: String,
}

pub fn fail_workflow_run_from_driver_atomically(
    repo_root: &Path,
    project_id: &str,
    failure: &WorkflowDriverFailureRecord,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_driver_failure_transaction_failed",
                &database_path,
                error,
            )
        })?;
    let owns_lease = tx
        .query_row(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM workflow_driver_leases
                WHERE project_id = ?1
                  AND workflow_run_id = ?2
                  AND owner_instance_id = ?3
                  AND lease_token = ?4
            )
            "#,
            params![
                project_id,
                failure.run_id.as_str(),
                failure.owner_instance_id.as_str(),
                failure.lease_token.as_str(),
            ],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_driver_failure_lease_query_failed",
                &database_path,
                error,
            )
        })?;
    if !owns_lease {
        tx.rollback().map_err(|error| {
            map_workflow_write_error(
                "workflow_driver_failure_rollback_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    let live_execution_count = tx
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM workflow_run_nodes AS node
            WHERE node.project_id = ?1
              AND node.workflow_run_id = ?2
              AND node.status IN ('eligible', 'starting', 'running', 'waiting_on_gate')
              AND (
                    EXISTS (
                        SELECT 1
                        FROM agent_runs AS agent
                        WHERE agent.project_id = node.project_id
                          AND (agent.run_id = node.runtime_run_id
                               OR agent.run_id = node.idempotency_key)
                          AND agent.status IN ('starting', 'running', 'paused', 'cancelling')
                    )
                    OR EXISTS (
                        SELECT 1
                        FROM workflow_command_leases AS lease
                        WHERE lease.project_id = node.project_id
                          AND lease.node_run_id = node.id
                    )
                  )
            "#,
            params![project_id, failure.run_id.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_driver_failure_execution_query_failed",
                &database_path,
                error,
            )
        })?;
    if live_execution_count != 0 {
        tx.rollback().map_err(|error| {
            map_workflow_write_error(
                "workflow_driver_failure_rollback_failed",
                &database_path,
                error,
            )
        })?;
        return Err(CommandError::retryable(
            "workflow_driver_execution_still_active",
            "The Workflow driver cannot fail the run until every owned execution is terminal.",
        ));
    }
    let run_changed = tx
        .execute(
            r#"
            UPDATE workflow_runs
            SET status = 'failed',
                terminal_status = 'failure',
                completed_at = ?3,
                updated_at = ?3
            WHERE project_id = ?1
              AND id = ?2
              AND status IN ('queued', 'running')
            "#,
            params![project_id, failure.run_id.as_str(), now.as_str()],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_driver_failure_run_update_failed",
                &database_path,
                error,
            )
        })?;
    if run_changed == 0 {
        tx.rollback().map_err(|error| {
            map_workflow_write_error(
                "workflow_driver_failure_rollback_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }
    tx.execute(
        r#"
        UPDATE workflow_run_nodes
        SET status = 'stalled',
            failure_class = ?3,
            completed_at = ?4,
            updated_at = ?4
        WHERE project_id = ?1
          AND workflow_run_id = ?2
          AND status IN ('eligible', 'starting', 'running', 'waiting_on_gate')
        "#,
        params![
            project_id,
            failure.run_id.as_str(),
            failure.failure_class.as_str(),
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error(
            "workflow_driver_failure_nodes_update_failed",
            &database_path,
            error,
        )
    })?;
    insert_deterministic_workflow_event(
        &tx,
        project_id,
        &failure.run_id,
        None,
        &failure.incident_id,
        "workflow_driver_failed",
        &failure.event,
        &now,
        &database_path,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error(
            "workflow_driver_failure_commit_failed",
            &database_path,
            error,
        )
    })?;
    Ok(true)
}

#[derive(Debug, Clone)]
pub struct WorkflowCommandCompletionRecord {
    pub run_id: String,
    pub node_run_id: String,
    pub artifact_type: String,
    pub schema_version: u32,
    pub payload: JsonValue,
    pub render_text: Option<String>,
    pub event: JsonValue,
    pub status: WorkflowNodeRunStatusDto,
    pub failure_class: Option<String>,
    pub owner_instance_id: String,
    pub lease_token: String,
}

pub fn complete_workflow_command_node(
    repo_root: &Path,
    project_id: &str,
    completion: &WorkflowCommandCompletionRecord,
) -> Result<(), CommandError> {
    if !matches!(
        completion.status,
        WorkflowNodeRunStatusDto::Succeeded | WorkflowNodeRunStatusDto::Failed
    ) {
        return Err(CommandError::system_fault(
            "workflow_command_completion_status_invalid",
            "Command nodes may only complete as succeeded or failed.",
        ));
    }

    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let payload_json = serialize_json(&completion.payload, "workflow_artifact_encode_failed")?;
    let event_json = serialize_json(&completion.event, "workflow_event_encode_failed")?;
    let now = now_timestamp();
    let artifact_id = format!(
        "{}:command-artifact:{}",
        completion.run_id, completion.node_run_id
    );
    let event_id = format!(
        "{}:command-event:{}",
        completion.run_id, completion.node_run_id
    );
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error("workflow_command_transaction_failed", &database_path, error)
        })?;

    let current_status = tx
        .query_row(
            r#"
            SELECT status
            FROM workflow_run_nodes
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND id = ?3
            "#,
            params![
                project_id,
                completion.run_id.as_str(),
                completion.node_run_id.as_str(),
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error("workflow_run_node_query_failed", &database_path, error)
        })?;
    if current_status.as_deref() == Some(completion.status.as_str()) {
        tx.commit().map_err(|error| {
            map_workflow_write_error("workflow_command_commit_failed", &database_path, error)
        })?;
        return Ok(());
    }
    let owns_lease = tx
        .query_row(
            r#"
            SELECT 1
            FROM workflow_command_leases
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND node_run_id = ?3
              AND owner_instance_id = ?4
              AND lease_token = ?5
            "#,
            params![
                project_id,
                completion.run_id.as_str(),
                completion.node_run_id.as_str(),
                completion.owner_instance_id.as_str(),
                completion.lease_token.as_str(),
            ],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error("workflow_command_lease_query_failed", &database_path, error)
        })?
        .is_some();
    if !owns_lease {
        return Err(CommandError::retryable(
            "workflow_command_completion_conflict",
            format!(
                "Workflow command node `{}` is no longer owned by this command process.",
                completion.node_run_id
            ),
        ));
    }

    tx.execute(
        r#"
        INSERT INTO workflow_artifacts (
            id,
            project_id,
            workflow_run_id,
            producer_node_run_id,
            artifact_type,
            schema_version,
            payload_json,
            render_text,
            created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(project_id, id) DO NOTHING
        "#,
        params![
            artifact_id,
            project_id,
            completion.run_id.as_str(),
            completion.node_run_id.as_str(),
            completion.artifact_type.as_str(),
            completion.schema_version,
            payload_json,
            completion.render_text.as_deref(),
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_artifact_insert_failed", &database_path, error)
    })?;
    tx.execute(
        r#"
        INSERT INTO workflow_events (
            id,
            project_id,
            workflow_run_id,
            node_run_id,
            event_type,
            event_json,
            created_at
        )
        VALUES (?1, ?2, ?3, ?4, 'workflow_command_completed', ?5, ?6)
        ON CONFLICT(project_id, id) DO NOTHING
        "#,
        params![
            event_id,
            project_id,
            completion.run_id.as_str(),
            completion.node_run_id.as_str(),
            event_json,
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_event_insert_failed", &database_path, error)
    })?;
    let changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = ?3,
                failure_class = ?4,
                completed_at = ?5,
                updated_at = ?5
            WHERE project_id = ?1
              AND id = ?2
              AND status IN ('starting', 'running')
              AND EXISTS (
                    SELECT 1
                    FROM workflow_runs
                    WHERE workflow_runs.project_id = workflow_run_nodes.project_id
                      AND workflow_runs.id = workflow_run_nodes.workflow_run_id
                      AND workflow_runs.status IN ('queued', 'running', 'paused')
                  )
            "#,
            params![
                project_id,
                completion.node_run_id.as_str(),
                completion.status.as_str(),
                completion.failure_class.as_deref(),
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_run_node_update_failed", &database_path, error)
        })?;
    if changed == 0 {
        let current_status = tx
            .query_row(
                "SELECT status FROM workflow_run_nodes WHERE project_id = ?1 AND id = ?2",
                params![project_id, completion.node_run_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| {
                map_workflow_query_error("workflow_run_node_query_failed", &database_path, error)
            })?;
        if current_status.as_deref() != Some(completion.status.as_str()) {
            return Err(CommandError::retryable(
                "workflow_command_completion_conflict",
                format!(
                    "Workflow command node `{}` was no longer active when completion was persisted.",
                    completion.node_run_id
                ),
            ));
        }
    }

    let released = tx
        .execute(
            r#"
            DELETE FROM workflow_command_leases
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND node_run_id = ?3
              AND owner_instance_id = ?4
              AND lease_token = ?5
            "#,
            params![
                project_id,
                completion.run_id.as_str(),
                completion.node_run_id.as_str(),
                completion.owner_instance_id.as_str(),
                completion.lease_token.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_command_lease_release_failed",
                &database_path,
                error,
            )
        })?;
    if released != 1 {
        return Err(CommandError::retryable(
            "workflow_command_completion_conflict",
            format!(
                "Workflow command node `{}` lost ownership before completion could commit.",
                completion.node_run_id
            ),
        ));
    }

    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_command_commit_failed", &database_path, error)
    })
}

#[derive(Debug, Clone)]
pub struct WorkflowAgentArtifactCompletionRecord {
    pub run_id: String,
    pub node_run_id: String,
    pub artifact_type: String,
    pub schema_version: u32,
    pub payload: JsonValue,
    pub render_text: Option<String>,
    pub event: JsonValue,
}

pub fn complete_workflow_agent_node_with_artifact(
    repo_root: &Path,
    project_id: &str,
    completion: &WorkflowAgentArtifactCompletionRecord,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let payload_json = serialize_json(&completion.payload, "workflow_artifact_encode_failed")?;
    let event_json = serialize_json(&completion.event, "workflow_event_encode_failed")?;
    let now = now_timestamp();
    let artifact_id = format!(
        "{}:agent-artifact:{}",
        completion.run_id, completion.node_run_id
    );
    let event_id = format!(
        "{}:agent-artifact-event:{}",
        completion.run_id, completion.node_run_id
    );
    let tx = connection.unchecked_transaction().map_err(|error| {
        map_workflow_write_error(
            "workflow_agent_completion_transaction_failed",
            &database_path,
            error,
        )
    })?;
    let changed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'succeeded',
                completed_at = ?3,
                updated_at = ?3
            WHERE project_id = ?1
              AND id = ?2
              AND status IN ('running', 'waiting_on_gate')
              AND EXISTS (
                    SELECT 1
                    FROM workflow_runs
                    WHERE workflow_runs.project_id = workflow_run_nodes.project_id
                      AND workflow_runs.id = workflow_run_nodes.workflow_run_id
                      AND workflow_runs.status IN ('queued', 'running', 'paused')
                  )
            "#,
            params![project_id, completion.node_run_id.as_str(), now.as_str()],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_run_node_update_failed", &database_path, error)
        })?;
    if changed == 0 {
        tx.rollback().map_err(|error| {
            map_workflow_write_error(
                "workflow_agent_completion_rollback_failed",
                &database_path,
                error,
            )
        })?;
        return Ok(false);
    }

    tx.execute(
        r#"
        INSERT INTO workflow_artifacts (
            id,
            project_id,
            workflow_run_id,
            producer_node_run_id,
            artifact_type,
            schema_version,
            payload_json,
            render_text,
            created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
        params![
            artifact_id,
            project_id,
            completion.run_id.as_str(),
            completion.node_run_id.as_str(),
            completion.artifact_type.as_str(),
            completion.schema_version,
            payload_json,
            completion.render_text.as_deref(),
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_artifact_insert_failed", &database_path, error)
    })?;
    tx.execute(
        r#"
        INSERT INTO workflow_events (
            id,
            project_id,
            workflow_run_id,
            node_run_id,
            event_type,
            event_json,
            created_at
        )
        VALUES (?1, ?2, ?3, ?4, 'workflow_artifact_extracted', ?5, ?6)
        "#,
        params![
            event_id,
            project_id,
            completion.run_id.as_str(),
            completion.node_run_id.as_str(),
            event_json,
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_event_insert_failed", &database_path, error)
    })?;
    tx.commit().map_err(|error| {
        map_workflow_write_error(
            "workflow_agent_completion_commit_failed",
            &database_path,
            error,
        )
    })?;
    Ok(true)
}

#[derive(Debug, Clone)]
pub struct WorkflowRouteDecisionRecord {
    pub source_node_run_id: String,
    pub source_status: WorkflowNodeRunStatusDto,
    pub from_node_id: String,
    pub to_node_id: String,
    pub edge_id: String,
    pub condition: JsonValue,
    pub evidence: JsonValue,
    pub target_node_type: String,
    pub target_attempt_number: u32,
    pub target_idempotency_key: String,
}

pub fn commit_workflow_route(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    decisions: &[WorkflowRouteDecisionRecord],
) -> Result<bool, CommandError> {
    let Some(source_node_run_id) = decisions
        .first()
        .map(|decision| decision.source_node_run_id.as_str())
    else {
        return Ok(false);
    };
    if decisions.iter().any(|decision| {
        decision.source_node_run_id != source_node_run_id
            || decision.source_status != decisions[0].source_status
    }) {
        return Err(CommandError::system_fault(
            "workflow_route_source_mismatch",
            "A Workflow route transaction cannot contain multiple source node attempts.",
        ));
    }

    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let marker_event = json!({
        "sourceNodeRunId": source_node_run_id,
        "edgeIds": decisions.iter().map(|decision| decision.edge_id.as_str()).collect::<Vec<_>>(),
        "targetNodeIds": decisions.iter().map(|decision| decision.to_node_id.as_str()).collect::<Vec<_>>(),
    });
    let marker_json = serialize_json(&marker_event, "workflow_event_encode_failed")?;
    let marker_id = format!("{run_id}:route:{source_node_run_id}:completed");
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error("workflow_route_transaction_failed", &database_path, error)
        })?;

    let already_committed = tx
        .query_row(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM workflow_events
                WHERE project_id = ?1
                  AND id = ?2
                  AND workflow_run_id = ?3
                  AND node_run_id = ?4
                  AND event_type = 'workflow_node_routed'
            )
            "#,
            params![project_id, marker_id.as_str(), run_id, source_node_run_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| {
            map_workflow_query_error("workflow_route_marker_query_failed", &database_path, error)
        })?;
    if already_committed {
        tx.commit().map_err(|error| {
            map_workflow_write_error("workflow_route_commit_failed", &database_path, error)
        })?;
        return Ok(true);
    }

    let source_is_current = tx
        .query_row(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM workflow_run_nodes AS source
                JOIN workflow_runs AS run
                  ON run.project_id = source.project_id
                 AND run.id = source.workflow_run_id
                WHERE source.project_id = ?1
                  AND source.id = ?2
                  AND source.workflow_run_id = ?3
                  AND source.status = ?4
                  AND run.status = 'running'
                  AND NOT EXISTS (
                        SELECT 1
                        FROM agent_runs AS agent
                        WHERE agent.project_id = source.project_id
                          AND (agent.run_id = source.runtime_run_id
                               OR agent.run_id = source.idempotency_key)
                          AND agent.status IN ('starting', 'running', 'paused', 'cancelling')
                      )
                  AND NOT EXISTS (
                        SELECT 1
                        FROM workflow_command_leases AS lease
                        WHERE lease.project_id = source.project_id
                          AND lease.node_run_id = source.id
                      )
            )
            "#,
            params![
                project_id,
                source_node_run_id,
                run_id,
                decisions[0].source_status.as_str(),
            ],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| {
            map_workflow_query_error("workflow_route_source_query_failed", &database_path, error)
        })?;
    if !source_is_current {
        tx.rollback().map_err(|error| {
            map_workflow_write_error("workflow_route_rollback_failed", &database_path, error)
        })?;
        return Ok(false);
    }

    for decision in decisions {
        let target_run_id = format!(
            "{run_id}:node:{}:attempt:{}",
            decision.to_node_id, decision.target_attempt_number
        );
        tx.execute(
            r#"
            INSERT INTO workflow_run_nodes (
                id,
                project_id,
                workflow_run_id,
                node_id,
                node_type,
                status,
                attempt_number,
                updated_at,
                idempotency_key
            )
            VALUES (?1, ?2, ?3, ?4, ?5, 'eligible', ?6, ?7, ?8)
            ON CONFLICT(project_id, workflow_run_id, node_id, attempt_number) DO NOTHING
            "#,
            params![
                target_run_id,
                project_id,
                run_id,
                decision.to_node_id.as_str(),
                decision.target_node_type.as_str(),
                decision.target_attempt_number,
                now.as_str(),
                decision.target_idempotency_key.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_run_node_insert_failed", &database_path, error)
        })?;

        let edge_decision_id = format!(
            "{run_id}:route:{}:{}:{}",
            decision.source_node_run_id, decision.edge_id, decision.target_attempt_number
        );
        tx.execute(
            r#"
            INSERT INTO workflow_run_edges (
                id,
                project_id,
                workflow_run_id,
                from_node_id,
                to_node_id,
                edge_id,
                matched,
                condition_json,
                evidence_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9)
            ON CONFLICT(project_id, id) DO NOTHING
            "#,
            params![
                edge_decision_id,
                project_id,
                run_id,
                decision.from_node_id.as_str(),
                decision.to_node_id.as_str(),
                decision.edge_id.as_str(),
                serialize_json(&decision.condition, "workflow_edge_condition_encode_failed")?,
                serialize_json(&decision.evidence, "workflow_edge_evidence_encode_failed")?,
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_edge_decision_insert_failed",
                &database_path,
                error,
            )
        })?;
    }

    tx.execute(
        r#"
        INSERT INTO workflow_events (
            id,
            project_id,
            workflow_run_id,
            node_run_id,
            event_type,
            event_json,
            created_at
        )
        VALUES (?1, ?2, ?3, ?4, 'workflow_node_routed', ?5, ?6)
        ON CONFLICT(project_id, id) DO NOTHING
        "#,
        params![
            marker_id,
            project_id,
            run_id,
            source_node_run_id,
            marker_json,
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_event_insert_failed", &database_path, error)
    })?;

    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_route_commit_failed", &database_path, error)
    })?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
pub fn insert_workflow_artifact(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    producer_node_run_id: &str,
    artifact_type: &str,
    schema_version: u32,
    payload: &JsonValue,
    render_text: Option<&str>,
) -> Result<WorkflowArtifactRecordDto, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let id = generate_workflow_id("workflow-artifact");
    let now = now_timestamp();
    let payload_json = serialize_json(payload, "workflow_artifact_encode_failed")?;
    connection
        .execute(
            r#"
            INSERT INTO workflow_artifacts (
                id,
                project_id,
                workflow_run_id,
                producer_node_run_id,
                artifact_type,
                schema_version,
                payload_json,
                render_text,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                id.as_str(),
                project_id,
                run_id,
                producer_node_run_id,
                artifact_type,
                schema_version,
                payload_json.as_str(),
                render_text,
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_artifact_insert_failed", &database_path, error)
        })?;
    Ok(WorkflowArtifactRecordDto {
        id,
        workflow_run_id: run_id.into(),
        producer_node_run_id: producer_node_run_id.into(),
        artifact_type: artifact_type.into(),
        schema_version,
        payload: payload.clone(),
        render_text: render_text.map(ToOwned::to_owned),
        created_at: now,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn insert_workflow_edge_decision(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    from_node_id: &str,
    to_node_id: &str,
    edge_id: &str,
    condition: &JsonValue,
    evidence: &JsonValue,
) -> Result<(), CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    connection
        .execute(
            r#"
            INSERT INTO workflow_run_edges (
                id,
                project_id,
                workflow_run_id,
                from_node_id,
                to_node_id,
                edge_id,
                matched,
                condition_json,
                evidence_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9)
            "#,
            params![
                generate_workflow_id("workflow-edge-decision"),
                project_id,
                run_id,
                from_node_id,
                to_node_id,
                edge_id,
                serialize_json(condition, "workflow_edge_condition_encode_failed")?,
                serialize_json(evidence, "workflow_edge_evidence_encode_failed")?,
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_edge_decision_insert_failed",
                &database_path,
                error,
            )
        })?;
    Ok(())
}

pub fn increment_workflow_loop_attempt(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    loop_key: &str,
    node_run_id: &str,
    exhausted: bool,
) -> Result<WorkflowLoopAttemptDto, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let id = generate_workflow_id("workflow-loop");
    connection
        .execute(
            r#"
            INSERT INTO workflow_loop_attempts (
                id,
                project_id,
                workflow_run_id,
                loop_key,
                attempt_count,
                last_node_run_id,
                exhausted,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7)
            ON CONFLICT(project_id, workflow_run_id, loop_key) DO UPDATE SET
                attempt_count = CASE
                    WHEN workflow_loop_attempts.last_node_run_id = excluded.last_node_run_id
                        THEN workflow_loop_attempts.attempt_count
                    ELSE workflow_loop_attempts.attempt_count + 1
                END,
                last_node_run_id = excluded.last_node_run_id,
                exhausted = excluded.exhausted,
                updated_at = excluded.updated_at
            "#,
            params![
                id.as_str(),
                project_id,
                run_id,
                loop_key,
                node_run_id,
                i64::from(exhausted),
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_loop_attempt_update_failed", &database_path, error)
        })?;
    read_loop_attempt(&connection, &database_path, project_id, run_id, loop_key)?.ok_or_else(|| {
        CommandError::system_fault(
            "workflow_loop_attempt_missing_after_update",
            format!("Xero updated loop `{loop_key}` but could not read it back."),
        )
    })
}

#[derive(Debug, Clone)]
pub struct WorkflowCheckpointResumeRecord {
    pub run_id: String,
    pub node_run_id: String,
    pub checkpoint_type: WorkflowHumanCheckpointTypeDto,
    pub decision: String,
    pub payload: Option<JsonValue>,
    pub state_updates: Vec<WorkflowStateWriteOperationDto>,
}

pub fn resume_workflow_checkpoint_atomically(
    repo_root: &Path,
    project_id: &str,
    resume: &WorkflowCheckpointResumeRecord,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_runtime_database(repo_root, &database_path)?;
    let payload_json = resume
        .payload
        .as_ref()
        .map(|payload| serialize_json(payload, "workflow_gate_decision_payload_encode_failed"))
        .transpose()?;
    let artifact_payload = json!({
        "decision": resume.decision,
        "payload": resume.payload,
    });
    let artifact_payload_json =
        serialize_json(&artifact_payload, "workflow_artifact_encode_failed")?;
    let now = now_timestamp();
    let decision_id = format!(
        "{}:checkpoint-decision:{}",
        resume.run_id, resume.node_run_id
    );
    let artifact_id = format!(
        "{}:checkpoint-artifact:{}",
        resume.run_id, resume.node_run_id
    );
    let resumed_event_id = format!(
        "{}:checkpoint-resumed:{}",
        resume.run_id, resume.node_run_id
    );
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_checkpoint_resume_transaction_failed",
                &database_path,
                error,
            )
        })?;

    let existing = tx
        .query_row(
            r#"
            SELECT checkpoint_type, decision, decision_payload_json
            FROM workflow_gate_decisions
            WHERE project_id = ?1
              AND id = ?2
              AND workflow_run_id = ?3
              AND node_run_id = ?4
            "#,
            params![
                project_id,
                decision_id.as_str(),
                resume.run_id.as_str(),
                resume.node_run_id.as_str(),
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_checkpoint_decision_query_failed",
                &database_path,
                error,
            )
        })?;
    if let Some((checkpoint_type, decision, existing_payload_json)) = existing {
        if checkpoint_type == resume.checkpoint_type.as_str()
            && decision == resume.decision
            && existing_payload_json == payload_json
        {
            tx.commit().map_err(|error| {
                map_workflow_write_error(
                    "workflow_checkpoint_resume_commit_failed",
                    &database_path,
                    error,
                )
            })?;
            return Ok(false);
        }
        return Err(CommandError::user_fixable(
            "workflow_checkpoint_already_resumed",
            format!(
                "Workflow checkpoint `{}` was already resumed with a different decision or payload.",
                resume.node_run_id
            ),
        ));
    }

    let claimed = tx
        .execute(
            r#"
            UPDATE workflow_run_nodes
            SET status = 'succeeded',
                completed_at = ?4,
                updated_at = ?4
            WHERE project_id = ?1
              AND id = ?2
              AND workflow_run_id = ?3
              AND node_type = 'human_checkpoint'
              AND status = 'waiting_on_gate'
              AND EXISTS (
                    SELECT 1
                    FROM workflow_runs
                    WHERE workflow_runs.project_id = workflow_run_nodes.project_id
                      AND workflow_runs.id = workflow_run_nodes.workflow_run_id
                      AND workflow_runs.id = ?3
                      AND workflow_runs.status = 'paused'
                      AND workflow_runs.terminal_status = 'needs_human'
                  )
            "#,
            params![
                project_id,
                resume.node_run_id.as_str(),
                resume.run_id.as_str(),
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_checkpoint_claim_failed", &database_path, error)
        })?;
    if claimed == 0 {
        return Err(CommandError::user_fixable(
            "workflow_checkpoint_not_waiting",
            format!(
                "Workflow checkpoint `{}` is not waiting in a paused Workflow run.",
                resume.node_run_id
            ),
        ));
    }
    let checkpoint_node_id = tx
        .query_row(
            "SELECT node_id FROM workflow_run_nodes WHERE project_id = ?1 AND workflow_run_id = ?2 AND id = ?3",
            params![project_id, resume.run_id.as_str(), resume.node_run_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_checkpoint_node_query_failed",
                &database_path,
                error,
            )
        })?;

    tx.execute(
        r#"
        INSERT INTO workflow_gate_decisions (
            id,
            project_id,
            workflow_run_id,
            node_run_id,
            checkpoint_type,
            decision,
            decision_payload_json,
            decided_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            decision_id.as_str(),
            project_id,
            resume.run_id.as_str(),
            resume.node_run_id.as_str(),
            resume.checkpoint_type.as_str(),
            resume.decision.as_str(),
            payload_json.as_deref(),
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error(
            "workflow_gate_decision_insert_failed",
            &database_path,
            error,
        )
    })?;
    tx.execute(
        r#"
        INSERT INTO workflow_artifacts (
            id,
            project_id,
            workflow_run_id,
            producer_node_run_id,
            artifact_type,
            schema_version,
            payload_json,
            render_text,
            created_at
        )
        VALUES (?1, ?2, ?3, ?4, 'human_decision', 1, ?5, ?6, ?7)
        "#,
        params![
            artifact_id.as_str(),
            project_id,
            resume.run_id.as_str(),
            resume.node_run_id.as_str(),
            artifact_payload_json,
            resume.decision.as_str(),
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_artifact_insert_failed", &database_path, error)
    })?;

    for (index, operation) in resume.state_updates.iter().enumerate() {
        let operation = workflow_state_write_operation_with_stable_identity(
            &resume.run_id,
            &checkpoint_node_id,
            Some(index),
            operation,
        )?;
        let operation_hash = workflow_state_write_operation_hash(&operation)?;
        let result = match replay_workflow_state_write(
            &tx,
            &database_path,
            project_id,
            &resume.run_id,
            &operation,
            &operation_hash,
        )? {
            Some(result) => result,
            None => write_delivery_state_with_connection(
                &tx,
                project_id,
                DeliveryStateWriteContext {
                    workflow_run_id: Some(&resume.run_id),
                    node_run_id: Some(&resume.node_run_id),
                },
                &operation,
            )?,
        };
        let event = json!({
            "nodeRunId": resume.node_run_id,
            "entityType": operation.entity_type.as_str(),
            "action": operation.action.as_str(),
            "entityId": result.get("id"),
            "decision": resume.decision,
            "idempotencyKey": operation.idempotency_key,
            "operationHash": operation_hash,
            "result": result,
        });
        let event_json = serialize_json(&event, "workflow_event_encode_failed")?;
        let event_id = format!(
            "{}:checkpoint-state:{}:{}",
            resume.run_id, resume.node_run_id, index
        );
        tx.execute(
            r#"
            INSERT INTO workflow_events (
                id,
                project_id,
                workflow_run_id,
                node_run_id,
                event_type,
                event_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, 'workflow_checkpoint_state_written', ?5, ?6)
            "#,
            params![
                event_id,
                project_id,
                resume.run_id.as_str(),
                resume.node_run_id.as_str(),
                event_json,
                now.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_write_error("workflow_event_insert_failed", &database_path, error)
        })?;
    }

    let resumed_event = json!({
        "nodeRunId": resume.node_run_id,
        "checkpointType": resume.checkpoint_type.as_str(),
        "decision": resume.decision,
    });
    let resumed_event_json = serialize_json(&resumed_event, "workflow_event_encode_failed")?;
    tx.execute(
        r#"
        INSERT INTO workflow_events (
            id,
            project_id,
            workflow_run_id,
            node_run_id,
            event_type,
            event_json,
            created_at
        )
        VALUES (?1, ?2, ?3, ?4, 'workflow_checkpoint_resumed', ?5, ?6)
        "#,
        params![
            resumed_event_id,
            project_id,
            resume.run_id.as_str(),
            resume.node_run_id.as_str(),
            resumed_event_json,
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_event_insert_failed", &database_path, error)
    })?;
    let reopened = tx
        .execute(
            r#"
            UPDATE workflow_runs
            SET status = 'running',
                terminal_status = NULL,
                completed_at = NULL,
                updated_at = ?3
            WHERE project_id = ?1
              AND id = ?2
              AND status = 'paused'
              AND terminal_status = 'needs_human'
            "#,
            params![project_id, resume.run_id.as_str(), now.as_str()],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_checkpoint_run_reopen_failed",
                &database_path,
                error,
            )
        })?;
    if reopened != 1 {
        return Err(CommandError::retryable(
            "workflow_checkpoint_run_reopen_conflict",
            format!(
                "Workflow run `{}` changed while checkpoint `{}` was being resumed.",
                resume.run_id, resume.node_run_id
            ),
        ));
    }

    tx.commit().map_err(|error| {
        map_workflow_write_error(
            "workflow_checkpoint_resume_commit_failed",
            &database_path,
            error,
        )
    })?;
    Ok(true)
}

pub fn insert_workflow_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    node_run_id: Option<&str>,
    event_type: &str,
    event: &JsonValue,
) -> Result<(), CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    insert_workflow_event_with_connection(
        &connection,
        project_id,
        run_id,
        node_run_id,
        event_type,
        event,
        &now_timestamp(),
    )
}

fn insert_workflow_event_with_connection(
    connection: &Connection,
    project_id: &str,
    run_id: &str,
    node_run_id: Option<&str>,
    event_type: &str,
    event: &JsonValue,
    created_at: &str,
) -> Result<(), CommandError> {
    connection
        .execute(
            r#"
            INSERT INTO workflow_events (
                id,
                project_id,
                workflow_run_id,
                node_run_id,
                event_type,
                event_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                generate_workflow_id("workflow-event"),
                project_id,
                run_id,
                node_run_id,
                event_type,
                serialize_json(event, "workflow_event_encode_failed")?,
                created_at,
            ],
        )
        .map_err(|error| {
            CommandError::retryable("workflow_event_insert_failed", error.to_string())
        })?;
    Ok(())
}

fn read_workflow_run(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Option<WorkflowRunDto>, CommandError> {
    let base = connection
        .query_row(
            r#"
            SELECT
                id,
                project_id,
                workflow_version_id,
                workflow_id,
                workflow_version_number,
                status,
                terminal_status,
                definition_json,
                initial_input_json,
                started_at,
                updated_at,
                completed_at,
                cancellation_reason
            FROM workflow_runs
            WHERE project_id = ?1
              AND id = ?2
            "#,
            params![project_id, run_id],
            |row| {
                let definition_json: String = row.get(7)?;
                let initial_input_json: Option<String> = row.get(8)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    definition_json,
                    initial_input_json,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<String>>(12)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error("workflow_run_query_failed", database_path, error)
        })?;
    let Some((
        id,
        project_id,
        workflow_version_id,
        workflow_id,
        workflow_version_number,
        status,
        terminal_status,
        definition_json,
        initial_input_json,
        started_at,
        updated_at,
        completed_at,
        cancellation_reason,
    )) = base
    else {
        return Ok(None);
    };
    Ok(Some(WorkflowRunDto {
        id: id.clone(),
        project_id: project_id.clone(),
        workflow_version_id,
        workflow_id,
        workflow_version_number,
        status: parse_run_status(&status, database_path)?,
        terminal_status: terminal_status
            .as_deref()
            .map(|value| parse_terminal_status(value, database_path))
            .transpose()?,
        definition_snapshot: decode_definition(database_path, &definition_json)?,
        initial_input: initial_input_json
            .as_deref()
            .map(|value| decode_json(database_path, value, "initialInput"))
            .transpose()?,
        started_at,
        updated_at,
        completed_at,
        cancellation_reason,
        nodes: read_run_nodes(connection, database_path, &project_id, &id)?,
        edge_decisions: read_edge_decisions(connection, database_path, &project_id, &id)?,
        artifacts: read_artifacts(connection, database_path, &project_id, &id)?,
        gate_decisions: read_gate_decisions(connection, database_path, &project_id, &id)?,
        loop_attempts: read_loop_attempts(connection, database_path, &project_id, &id)?,
        events: read_events(connection, database_path, &project_id, &id)?,
    }))
}

struct DefinitionVersionRow {
    id: String,
    version_number: u32,
    definition_json: String,
}

fn read_active_definition_version_row(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    workflow_id: &str,
) -> Result<Option<DefinitionVersionRow>, CommandError> {
    connection
        .query_row(
            r#"
            SELECT versions.id, versions.version_number, versions.definition_json
            FROM workflow_definitions definitions
            JOIN workflow_definition_versions versions
              ON versions.project_id = definitions.project_id
             AND versions.id = definitions.active_version_id
            WHERE definitions.project_id = ?1
              AND definitions.id = ?2
            "#,
            params![project_id, workflow_id],
            |row| {
                Ok(DefinitionVersionRow {
                    id: row.get(0)?,
                    version_number: row.get(1)?,
                    definition_json: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_definition_version_query_failed",
                database_path,
                error,
            )
        })
}

fn read_active_definition_json(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    workflow_id: &str,
) -> Result<Option<String>, CommandError> {
    read_active_definition_version_row(connection, database_path, project_id, workflow_id)
        .map(|row| row.map(|row| row.definition_json))
}

fn read_definition_summary_row(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    workflow_id: &str,
) -> Result<Option<WorkflowDefinitionSummaryDto>, CommandError> {
    connection
        .query_row(
            r#"
            SELECT
                definitions.id,
                definitions.project_id,
                definitions.name,
                definitions.description,
                definitions.active_version_id,
                versions.version_number,
                definitions.created_at,
                definitions.updated_at
            FROM workflow_definitions definitions
            JOIN workflow_definition_versions versions
              ON versions.project_id = definitions.project_id
             AND versions.id = definitions.active_version_id
            WHERE definitions.project_id = ?1
              AND definitions.id = ?2
            "#,
            params![project_id, workflow_id],
            read_definition_summary_from_row,
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error("workflow_definition_query_failed", database_path, error)
        })
}

fn read_definition_summary_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<WorkflowDefinitionSummaryDto> {
    Ok(WorkflowDefinitionSummaryDto {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        active_version_id: row.get(4)?,
        active_version_number: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

#[allow(clippy::too_many_arguments)]
fn insert_definition_version(
    connection: &Connection,
    database_path: &Path,
    version_id: &str,
    project_id: &str,
    workflow_id: &str,
    version_number: u32,
    definition_json: &str,
    created_at: &str,
) -> Result<(), CommandError> {
    connection
        .execute(
            r#"
            INSERT INTO workflow_definition_versions (
                id,
                project_id,
                workflow_id,
                version_number,
                definition_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                version_id,
                project_id,
                workflow_id,
                version_number,
                definition_json,
                created_at,
            ],
        )
        .map_err(|error| {
            map_workflow_write_error(
                "workflow_definition_version_insert_failed",
                database_path,
                error,
            )
        })?;
    Ok(())
}

fn read_workflow_run_node_by_key(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
    node_id: &str,
    attempt_number: u32,
) -> Result<Option<WorkflowRunNodeDto>, CommandError> {
    connection
        .query_row(
            run_node_select_sql_with_where(
                "WHERE project_id = ?1 AND workflow_run_id = ?2 AND node_id = ?3 AND attempt_number = ?4",
            )
            .as_str(),
            params![project_id, run_id, node_id, attempt_number],
            read_run_node_from_row,
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error("workflow_run_node_query_failed", database_path, error)
        })
}

fn read_run_nodes(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<WorkflowRunNodeDto>, CommandError> {
    let sql = run_node_select_sql_with_where(
        "WHERE project_id = ?1 AND workflow_run_id = ?2 ORDER BY updated_at ASC, id ASC",
    );
    let mut statement = connection.prepare(&sql).map_err(|error| {
        map_workflow_query_error("workflow_run_nodes_prepare_failed", database_path, error)
    })?;
    let rows = statement
        .query_map(params![project_id, run_id], read_run_node_from_row)
        .map_err(|error| {
            map_workflow_query_error("workflow_run_nodes_query_failed", database_path, error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_workflow_query_error("workflow_run_nodes_decode_failed", database_path, error)
    })
}

fn run_node_select_sql_with_where(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            id,
            workflow_run_id,
            node_id,
            node_type,
            status,
            attempt_number,
            runtime_run_id,
            agent_session_id,
            failure_class,
            started_at,
            updated_at,
            completed_at,
            idempotency_key
        FROM workflow_run_nodes
        {where_clause}
        "#
    )
}

fn read_run_node_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowRunNodeDto> {
    let status: String = row.get(4)?;
    Ok(WorkflowRunNodeDto {
        id: row.get(0)?,
        workflow_run_id: row.get(1)?,
        node_id: row.get(2)?,
        node_type: row.get(3)?,
        status: parse_node_status_lossy(&status),
        attempt_number: row.get(5)?,
        runtime_run_id: row.get(6)?,
        agent_session_id: row.get(7)?,
        failure_class: row.get(8)?,
        started_at: row.get(9)?,
        updated_at: row.get(10)?,
        completed_at: row.get(11)?,
        idempotency_key: row.get(12)?,
    })
}

fn read_artifacts(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<WorkflowArtifactRecordDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                workflow_run_id,
                producer_node_run_id,
                artifact_type,
                schema_version,
                payload_json,
                render_text,
                created_at
            FROM workflow_artifacts
            WHERE project_id = ?1
              AND workflow_run_id = ?2
            ORDER BY created_at ASC, id ASC
            "#,
        )
        .map_err(|error| {
            map_workflow_query_error("workflow_artifacts_prepare_failed", database_path, error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            let payload_json: String = row.get(5)?;
            let payload = serde_json::from_str(&payload_json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(WorkflowArtifactRecordDto {
                id: row.get(0)?,
                workflow_run_id: row.get(1)?,
                producer_node_run_id: row.get(2)?,
                artifact_type: row.get(3)?,
                schema_version: row.get(4)?,
                payload,
                render_text: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|error| {
            map_workflow_query_error("workflow_artifacts_query_failed", database_path, error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_workflow_query_error("workflow_artifacts_decode_failed", database_path, error)
    })
}

fn read_edge_decisions(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<WorkflowRunEdgeDecisionDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                workflow_run_id,
                from_node_id,
                to_node_id,
                edge_id,
                matched,
                condition_json,
                evidence_json,
                created_at
            FROM workflow_run_edges
            WHERE project_id = ?1
              AND workflow_run_id = ?2
            ORDER BY created_at ASC, id ASC
            "#,
        )
        .map_err(|error| {
            map_workflow_query_error("workflow_edges_prepare_failed", database_path, error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            let condition_json: String = row.get(6)?;
            let evidence_json: String = row.get(7)?;
            Ok(WorkflowRunEdgeDecisionDto {
                id: row.get(0)?,
                workflow_run_id: row.get(1)?,
                from_node_id: row.get(2)?,
                to_node_id: row.get(3)?,
                edge_id: row.get(4)?,
                matched: row.get::<_, i64>(5)? != 0,
                condition: serde_json::from_str(&condition_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                evidence: serde_json::from_str(&evidence_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        7,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                created_at: row.get(8)?,
            })
        })
        .map_err(|error| {
            map_workflow_query_error("workflow_edges_query_failed", database_path, error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_workflow_query_error("workflow_edges_decode_failed", database_path, error)
    })
}

fn read_gate_decisions(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<WorkflowGateDecisionDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                workflow_run_id,
                node_run_id,
                checkpoint_type,
                decision,
                decision_payload_json,
                decided_at
            FROM workflow_gate_decisions
            WHERE project_id = ?1
              AND workflow_run_id = ?2
            ORDER BY decided_at ASC, id ASC
            "#,
        )
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_gate_decisions_prepare_failed",
                database_path,
                error,
            )
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            let checkpoint_type: String = row.get(3)?;
            let payload_json: Option<String> = row.get(5)?;
            Ok(WorkflowGateDecisionDto {
                id: row.get(0)?,
                workflow_run_id: row.get(1)?,
                node_run_id: row.get(2)?,
                checkpoint_type: parse_checkpoint_type_lossy(&checkpoint_type),
                decision: row.get(4)?,
                decision_payload: payload_json
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                decided_at: row.get(6)?,
            })
        })
        .map_err(|error| {
            map_workflow_query_error("workflow_gate_decisions_query_failed", database_path, error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_workflow_query_error(
            "workflow_gate_decisions_decode_failed",
            database_path,
            error,
        )
    })
}

fn read_loop_attempts(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<WorkflowLoopAttemptDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                workflow_run_id,
                loop_key,
                attempt_count,
                last_node_run_id,
                exhausted,
                updated_at
            FROM workflow_loop_attempts
            WHERE project_id = ?1
              AND workflow_run_id = ?2
            ORDER BY updated_at ASC, id ASC
            "#,
        )
        .map_err(|error| {
            map_workflow_query_error(
                "workflow_loop_attempts_prepare_failed",
                database_path,
                error,
            )
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], read_loop_attempt_from_row)
        .map_err(|error| {
            map_workflow_query_error("workflow_loop_attempts_query_failed", database_path, error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_workflow_query_error("workflow_loop_attempts_decode_failed", database_path, error)
    })
}

fn read_loop_attempt(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
    loop_key: &str,
) -> Result<Option<WorkflowLoopAttemptDto>, CommandError> {
    connection
        .query_row(
            r#"
            SELECT
                id,
                workflow_run_id,
                loop_key,
                attempt_count,
                last_node_run_id,
                exhausted,
                updated_at
            FROM workflow_loop_attempts
            WHERE project_id = ?1
              AND workflow_run_id = ?2
              AND loop_key = ?3
            "#,
            params![project_id, run_id, loop_key],
            read_loop_attempt_from_row,
        )
        .optional()
        .map_err(|error| {
            map_workflow_query_error("workflow_loop_attempt_query_failed", database_path, error)
        })
}

fn read_loop_attempt_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkflowLoopAttemptDto> {
    Ok(WorkflowLoopAttemptDto {
        id: row.get(0)?,
        workflow_run_id: row.get(1)?,
        loop_key: row.get(2)?,
        attempt_count: row.get(3)?,
        last_node_run_id: row.get(4)?,
        exhausted: row.get::<_, i64>(5)? != 0,
        updated_at: row.get(6)?,
    })
}

fn read_events(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<WorkflowEventDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                workflow_run_id,
                node_run_id,
                event_type,
                event_json,
                created_at
            FROM workflow_events
            WHERE project_id = ?1
              AND workflow_run_id = ?2
            ORDER BY created_at ASC, id ASC
            "#,
        )
        .map_err(|error| {
            map_workflow_query_error("workflow_events_prepare_failed", database_path, error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            let event_json: String = row.get(4)?;
            Ok(WorkflowEventDto {
                id: row.get(0)?,
                workflow_run_id: row.get(1)?,
                node_run_id: row.get(2)?,
                event_type: row.get(3)?,
                event: serde_json::from_str(&event_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|error| {
            map_workflow_query_error("workflow_events_query_failed", database_path, error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_workflow_query_error("workflow_events_decode_failed", database_path, error)
    })
}

fn validate_definition_identity(definition: &WorkflowDefinitionDto) -> Result<(), CommandError> {
    validate_non_empty_text(&definition.id, "id", "workflow_definition_request_invalid")?;
    validate_non_empty_text(
        &definition.project_id,
        "projectId",
        "workflow_definition_request_invalid",
    )?;
    validate_non_empty_text(
        &definition.name,
        "name",
        "workflow_definition_request_invalid",
    )
}

fn serialize_json<T: serde::Serialize>(
    value: &T,
    code: &'static str,
) -> Result<String, CommandError> {
    serde_json::to_string(value).map_err(|error| {
        CommandError::system_fault(
            code,
            format!("Xero could not encode Workflow JSON: {error}"),
        )
    })
}

fn decode_definition(
    database_path: &Path,
    json: &str,
) -> Result<WorkflowDefinitionDto, CommandError> {
    serde_json::from_str(json).map_err(|error| {
        CommandError::system_fault(
            "workflow_definition_decode_failed",
            format!(
                "Xero could not decode a Workflow definition from {}: {error}",
                database_path.display()
            ),
        )
    })
}

fn decode_json(database_path: &Path, json: &str, field: &str) -> Result<JsonValue, CommandError> {
    serde_json::from_str(json).map_err(|error| {
        CommandError::system_fault(
            "workflow_json_decode_failed",
            format!(
                "Xero could not decode Workflow field `{field}` from {}: {error}",
                database_path.display()
            ),
        )
    })
}

fn parse_run_status(
    value: &str,
    database_path: &Path,
) -> Result<WorkflowRunStatusDto, CommandError> {
    match value {
        "queued" => Ok(WorkflowRunStatusDto::Queued),
        "running" => Ok(WorkflowRunStatusDto::Running),
        "paused" => Ok(WorkflowRunStatusDto::Paused),
        "cancelling" => Ok(WorkflowRunStatusDto::Cancelling),
        "completed" => Ok(WorkflowRunStatusDto::Completed),
        "failed" => Ok(WorkflowRunStatusDto::Failed),
        "cancelled" => Ok(WorkflowRunStatusDto::Cancelled),
        _ => Err(CommandError::system_fault(
            "workflow_run_status_decode_failed",
            format!(
                "Xero read unknown Workflow run status `{value}` from {}.",
                database_path.display()
            ),
        )),
    }
}

fn parse_terminal_status(
    value: &str,
    database_path: &Path,
) -> Result<WorkflowTerminalStatusDto, CommandError> {
    match value {
        "success" => Ok(WorkflowTerminalStatusDto::Success),
        "failure" => Ok(WorkflowTerminalStatusDto::Failure),
        "cancelled" => Ok(WorkflowTerminalStatusDto::Cancelled),
        "needs_human" => Ok(WorkflowTerminalStatusDto::NeedsHuman),
        _ => Err(CommandError::system_fault(
            "workflow_terminal_status_decode_failed",
            format!(
                "Xero read unknown Workflow terminal status `{value}` from {}.",
                database_path.display()
            ),
        )),
    }
}

fn parse_node_status_lossy(value: &str) -> WorkflowNodeRunStatusDto {
    match value {
        "pending" => WorkflowNodeRunStatusDto::Pending,
        "eligible" => WorkflowNodeRunStatusDto::Eligible,
        "starting" => WorkflowNodeRunStatusDto::Starting,
        "running" => WorkflowNodeRunStatusDto::Running,
        "waiting_on_gate" => WorkflowNodeRunStatusDto::WaitingOnGate,
        "succeeded" => WorkflowNodeRunStatusDto::Succeeded,
        "failed" => WorkflowNodeRunStatusDto::Failed,
        "stalled" => WorkflowNodeRunStatusDto::Stalled,
        "skipped" => WorkflowNodeRunStatusDto::Skipped,
        "cancelled" => WorkflowNodeRunStatusDto::Cancelled,
        _ => WorkflowNodeRunStatusDto::Failed,
    }
}

fn parse_checkpoint_type_lossy(value: &str) -> WorkflowHumanCheckpointTypeDto {
    match value {
        "human_verify" => WorkflowHumanCheckpointTypeDto::HumanVerify,
        "human_action" => WorkflowHumanCheckpointTypeDto::HumanAction,
        _ => WorkflowHumanCheckpointTypeDto::Decision,
    }
}

fn map_workflow_write_error(
    code: &'static str,
    database_path: &Path,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::retryable(
        code,
        format!(
            "Xero could not write Workflow state in {}: {error}",
            database_path.display()
        ),
    )
}

fn map_workflow_query_error(
    code: &'static str,
    database_path: &Path,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::retryable(
        code,
        format!(
            "Xero could not read Workflow state from {}: {error}",
            database_path.display()
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use super::*;
    use crate::{
        commands::contracts::{
            runtime::RuntimeAgentIdDto,
            workflow_agents::AgentRefDto,
            workflows::{
                WorkflowDeliveryStateEntityTypeDto, WorkflowEdgeDto, WorkflowEdgeTypeDto,
                WorkflowLoopPolicyDto, WorkflowNodeDto, WorkflowOutputContractDto,
                WorkflowRunPolicyDto, WorkflowStateWriteActionDto, WorkflowSubgraphDto,
                WorkflowTerminalStatusDto,
            },
        },
        db::{
            configure_connection, migrations::migrations, register_project_database_path_for_tests,
        },
    };
    use tempfile::TempDir;

    fn repo_with_database() -> (TempDir, WorkflowDefinitionDto) {
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

        let definition = WorkflowDefinitionDto {
            schema: "xero.workflow_definition.v1".into(),
            id: "workflow-1".into(),
            project_id: "project-1".into(),
            name: "Workflow".into(),
            description: String::new(),
            version: 1,
            start_node_id: "agent-a".into(),
            nodes: vec![
                WorkflowNodeDto::Agent {
                    id: "agent-a".into(),
                    title: "Agent A".into(),
                    description: String::new(),
                    position: Default::default(),
                    agent_ref: AgentRefDto::BuiltIn {
                        runtime_agent_id: RuntimeAgentIdDto::Engineer,
                        version: 2,
                    },
                    display_label: None,
                    input_bindings: Vec::new(),
                    output_contract: WorkflowOutputContractDto::default(),
                    run_overrides: None,
                    resource_scopes: Vec::new(),
                    failure_policy: Default::default(),
                },
                WorkflowNodeDto::Terminal {
                    id: "done".into(),
                    title: "Done".into(),
                    description: String::new(),
                    position: Default::default(),
                    terminal_status: WorkflowTerminalStatusDto::Success,
                },
            ],
            edges: vec![WorkflowEdgeDto {
                id: "edge-done".into(),
                from_node_id: "agent-a".into(),
                to_node_id: "done".into(),
                r#type: WorkflowEdgeTypeDto::Success,
                label: String::new(),
                priority: 10,
                condition: Default::default(),
                loop_policy: None,
            }],
            subgraphs: Vec::new(),
            artifact_contracts: Vec::new(),
            run_policy: WorkflowRunPolicyDto::default(),
            created_at: None,
            updated_at: None,
        };
        (temp, definition)
    }

    fn persist_test_command_lease(
        repo_root: &Path,
        run_id: &str,
        node_run_id: &str,
        owner_instance_id: &str,
        lease_token: &str,
    ) {
        let connection =
            Connection::open(repo_root.join("state.db")).expect("open project database");
        connection
            .execute(
                r#"
                INSERT INTO workflow_command_leases (
                    project_id,
                    workflow_run_id,
                    node_run_id,
                    owner_instance_id,
                    owner_process_id,
                    owner_process_birth_identity,
                    lease_token,
                    acquired_at,
                    heartbeat_at
                )
                VALUES ('project-1', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
                "#,
                params![
                    run_id,
                    node_run_id,
                    owner_instance_id,
                    i64::from(std::process::id()),
                    crate::runtime::process_tree::process_birth_identity(std::process::id())
                        .expect("read current process birth identity"),
                    lease_token,
                    now_timestamp(),
                ],
            )
            .expect("insert command lease");
    }

    fn persist_test_agent_handoff_chain(
        repo_root: &Path,
        source_run_id: &str,
        target_run_id: &str,
        target_status: &str,
    ) {
        let connection =
            Connection::open(repo_root.join("state.db")).expect("open project database");
        let source_session_id = format!("session-{source_run_id}");
        let target_session_id = format!("session-{target_run_id}");
        for session_id in [&source_session_id, &target_session_id] {
            connection
                .execute(
                    "INSERT INTO agent_sessions (project_id, agent_session_id, title, status, selected) VALUES ('project-1', ?1, 'Workflow Agent', 'active', 0)",
                    params![session_id],
                )
                .expect("insert agent session");
        }
        for (session_id, run_id, trace_id, status) in [
            (
                source_session_id.as_str(),
                source_run_id,
                "11111111111111111111111111111111",
                "handed_off",
            ),
            (
                target_session_id.as_str(),
                target_run_id,
                "22222222222222222222222222222222",
                target_status,
            ),
        ] {
            connection
                .execute(
                    r#"
                    INSERT INTO agent_runs (
                        runtime_agent_id, agent_definition_id, agent_definition_version,
                        project_id, agent_session_id, run_id, trace_id, provider_id,
                        model_id, status, prompt, system_prompt, started_at, updated_at
                    )
                    VALUES (
                        'engineer', 'engineer', 2, 'project-1', ?1, ?2, ?3,
                        'provider-1', 'model-1', ?4, 'Do the work', 'System prompt',
                        '2026-07-15T12:00:00Z', '2026-07-15T12:00:00Z'
                    )
                    "#,
                    params![session_id, run_id, trace_id, status],
                )
                .expect("insert agent run");
        }
        connection
            .execute(
                r#"
                INSERT INTO agent_handoff_lineage (
                    handoff_id, project_id, source_agent_session_id, source_run_id,
                    source_runtime_agent_id, source_agent_definition_id,
                    source_agent_definition_version, target_agent_session_id, target_run_id,
                    target_runtime_agent_id, target_agent_definition_id,
                    target_agent_definition_version, provider_id, model_id,
                    source_context_hash, status, idempotency_key, bundle_json,
                    created_at, updated_at, completed_at
                )
                VALUES (
                    ?1, 'project-1', ?2, ?3, 'engineer', 'engineer', 2, ?4, ?5,
                    'engineer', 'engineer', 2, 'provider-1', 'model-1', ?6,
                    'completed', ?7, '{}', '2026-07-15T12:00:00Z',
                    '2026-07-15T12:00:00Z', '2026-07-15T12:00:00Z'
                )
                "#,
                params![
                    format!("handoff-{source_run_id}"),
                    source_session_id,
                    source_run_id,
                    target_session_id,
                    target_run_id,
                    "0".repeat(64),
                    format!("handoff-key-{source_run_id}"),
                ],
            )
            .expect("insert agent handoff lineage");
    }

    #[test]
    fn definitions_round_trip_with_immutable_versions() {
        let (temp, mut definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        definition.name = "Workflow v2".into();
        let updated =
            update_workflow_definition(temp.path(), &created.id, created.version, &definition)
                .expect("update workflow");

        let summaries =
            list_workflow_definitions(temp.path(), "project-1").expect("list workflows");

        assert_eq!(updated.version, 2);
        assert_eq!(summaries[0].active_version_number, 2);
    }

    #[test]
    fn stale_definition_update_is_rejected_with_a_stable_conflict() {
        let (temp, mut definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        definition.name = "First update".into();
        update_workflow_definition(temp.path(), &created.id, created.version, &definition)
            .expect("first update");

        definition.name = "Stale update".into();
        let error =
            update_workflow_definition(temp.path(), &created.id, created.version, &definition)
                .expect_err("stale update must lose");

        assert_eq!(error.code, "workflow_definition_version_conflict");
        let loaded = get_workflow_definition(temp.path(), "project-1", &created.id)
            .expect("load workflow")
            .expect("workflow exists");
        assert_eq!(loaded.name, "First update");
        assert_eq!(loaded.version, 2);
    }

    #[test]
    fn concurrent_definition_updates_allow_exactly_one_version_cas_winner() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let repo_root = temp.path().to_path_buf();
        let workflow_id = created.id.clone();
        let barrier = Arc::new(Barrier::new(2));
        let handles = ["Winner A", "Winner B"].map(|name| {
            let repo_root = repo_root.clone();
            let workflow_id = workflow_id.clone();
            let barrier = Arc::clone(&barrier);
            let mut candidate = created.clone();
            candidate.name = name.into();
            std::thread::spawn(move || {
                barrier.wait();
                update_workflow_definition(&repo_root, &workflow_id, candidate.version, &candidate)
            })
        });
        let results = handles.map(|handle| handle.join().expect("join update"));

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .filter(|error| error.code == "workflow_definition_version_conflict")
                .count(),
            1,
        );
        let summaries =
            list_workflow_definitions(temp.path(), "project-1").expect("list workflows");
        assert_eq!(summaries[0].active_version_number, 2);
    }

    #[test]
    fn workflow_start_key_replays_one_run_and_rejects_a_conflicting_payload() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let first = create_workflow_run_idempotently(
            temp.path(),
            "project-1",
            &created.id,
            "start-request-1",
            created.version,
            Some(json!({ "goal": "ship" })),
        )
        .expect("create idempotent run");
        let replay = create_workflow_run_idempotently(
            temp.path(),
            "project-1",
            &created.id,
            "start-request-1",
            created.version,
            Some(json!({ "goal": "ship" })),
        )
        .expect("replay idempotent run");

        assert_eq!(replay.id, first.id);
        assert_eq!(
            list_workflow_runs(temp.path(), "project-1", Some(&created.id))
                .expect("list runs")
                .len(),
            1,
        );
        let error = create_workflow_run_idempotently(
            temp.path(),
            "project-1",
            &created.id,
            "start-request-1",
            created.version,
            Some(json!({ "goal": "different" })),
        )
        .expect_err("same key cannot identify another payload");
        assert_eq!(error.code, "workflow_run_idempotency_conflict");
    }

    #[test]
    fn concurrent_workflow_start_replays_create_exactly_one_run() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let repo_root = temp.path().to_path_buf();
        let barrier = Arc::new(Barrier::new(2));
        let handles = [(), ()].map(|()| {
            let repo_root = repo_root.clone();
            let workflow_id = created.id.clone();
            let version = created.version;
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                create_workflow_run_idempotently(
                    &repo_root,
                    "project-1",
                    &workflow_id,
                    "concurrent-start-request",
                    version,
                    Some(json!({ "goal": "ship" })),
                )
            })
        });
        let runs = handles.map(|handle| {
            handle
                .join()
                .expect("join start")
                .expect("start or replay run")
        });

        assert_eq!(runs[0].id, runs[1].id);
        assert_eq!(
            list_workflow_runs(temp.path(), "project-1", Some(&created.id))
                .expect("list runs")
                .len(),
            1,
        );
    }

    #[test]
    fn resume_phase_identity_excludes_server_derived_selection_state() {
        let first = WorkflowResumePhaseStartRecord {
            workflow_id: "workflow-1".into(),
            expected_workflow_version: 1,
            source_run_id: "source-run".into(),
            initial_input: json!({ "phase": "first" }),
            loop_node_id: "loop-a".into(),
            phase_id: "phase-a".into(),
            phase_key: "first".into(),
            input_path: "$.first".into(),
        };
        let mut changed_selection = first.clone();
        changed_selection.workflow_id = "workflow-2".into();
        changed_selection.expected_workflow_version = 9;
        changed_selection.initial_input = json!({ "phase": "later" });
        changed_selection.loop_node_id = "loop-b".into();
        changed_selection.phase_id = "phase-b".into();
        changed_selection.phase_key = "later".into();
        changed_selection.input_path = "$.later".into();

        assert_eq!(
            workflow_resume_phase_payload_hash("project-1", &first).expect("hash first"),
            workflow_resume_phase_payload_hash("project-1", &changed_selection)
                .expect("hash changed selection"),
        );
        changed_selection.source_run_id = "another-source".into();
        assert_ne!(
            workflow_resume_phase_payload_hash("project-1", &first).expect("hash first"),
            workflow_resume_phase_payload_hash("project-1", &changed_selection)
                .expect("hash another source"),
        );
    }

    #[test]
    fn runs_round_trip_with_artifacts_and_loop_attempts() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run = create_workflow_run(
            temp.path(),
            "project-1",
            &created.id,
            Some(json!({ "goal": "ship" })),
        )
        .expect("create run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Succeeded,
            "idempotency-key",
        )
        .expect("insert node");
        insert_workflow_artifact(
            temp.path(),
            "project-1",
            &run.id,
            &node.id,
            "text_output",
            1,
            &json!({ "text": "done" }),
            Some("done"),
        )
        .expect("insert artifact");
        increment_workflow_loop_attempt(
            temp.path(),
            "project-1",
            &run.id,
            "retry",
            &node.id,
            false,
        )
        .expect("increment loop");

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");

        assert_eq!(loaded.artifacts.len(), 1);
        assert_eq!(loaded.loop_attempts[0].attempt_count, 1);
    }

    #[test]
    fn checkpoint_resume_commits_once_with_state_updates_and_reopens_run() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Paused,
            Some(WorkflowTerminalStatusDto::NeedsHuman),
            None,
        )
        .expect("pause run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "approval",
            "human_checkpoint",
            0,
            WorkflowNodeRunStatusDto::WaitingOnGate,
            "approval-attempt",
        )
        .expect("insert checkpoint");
        let resume = WorkflowCheckpointResumeRecord {
            run_id: run.id.clone(),
            node_run_id: node.id.clone(),
            checkpoint_type: WorkflowHumanCheckpointTypeDto::Decision,
            decision: "approve".into(),
            payload: Some(json!({ "note": "looks good" })),
            state_updates: vec![WorkflowStateWriteOperationDto {
                entity_type: WorkflowDeliveryStateEntityTypeDto::DeliveryProject,
                action: WorkflowStateWriteActionDto::Create,
                idempotency_key: Some("checkpoint-project".into()),
                target_id: Some("delivery-project-1".into()),
                payload: serde_json::from_value(json!({ "title": "Approved project" }))
                    .expect("object payload"),
                output_artifact_type: "state_write_result".into(),
            }],
        };

        assert!(
            resume_workflow_checkpoint_atomically(temp.path(), "project-1", &resume)
                .expect("resume checkpoint")
        );
        assert!(
            !resume_workflow_checkpoint_atomically(temp.path(), "project-1", &resume)
                .expect("repeat resume after ambiguous acknowledgement")
        );
        let mut conflicting_resume = resume.clone();
        conflicting_resume.decision = "reject".into();
        let conflict =
            resume_workflow_checkpoint_atomically(temp.path(), "project-1", &conflicting_resume)
                .expect_err("a different replay must not overwrite the committed decision");

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let persisted = loaded
            .nodes
            .iter()
            .find(|candidate| candidate.id == node.id)
            .expect("checkpoint exists");
        assert_eq!(loaded.status, WorkflowRunStatusDto::Running);
        assert_eq!(conflict.code, "workflow_checkpoint_already_resumed");
        assert_eq!(loaded.terminal_status, None);
        assert_eq!(persisted.status, WorkflowNodeRunStatusDto::Succeeded);
        assert_eq!(loaded.gate_decisions.len(), 1);
        assert_eq!(loaded.artifacts.len(), 1);
        assert_eq!(
            loaded
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_checkpoint_state_written")
                .count(),
            1,
        );
        assert_eq!(
            loaded
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_checkpoint_resumed")
                .count(),
            1,
        );
        let connection = Connection::open(temp.path().join("state.db")).expect("open database");
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM delivery_projects WHERE project_id = 'project-1'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("count delivery projects"),
            1,
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM delivery_state_events WHERE project_id = 'project-1' AND node_run_id = ?1",
                    params![node.id],
                    |row| row.get::<_, i64>(0),
                )
                .expect("count state events"),
            1,
        );
    }

    #[test]
    fn retried_checkpoint_replays_state_updates_without_mutating_state_twice() {
        let (temp, mut definition) = repo_with_database();
        let state_update = WorkflowStateWriteOperationDto {
            entity_type: WorkflowDeliveryStateEntityTypeDto::DeliveryProject,
            action: WorkflowStateWriteActionDto::Create,
            idempotency_key: None,
            target_id: Some("checkpoint-retry-project".into()),
            payload: serde_json::from_value(json!({ "title": "Checkpoint retry" }))
                .expect("object payload"),
            output_artifact_type: "state_write_result".into(),
        };
        definition.nodes.push(WorkflowNodeDto::HumanCheckpoint {
            id: "approval".into(),
            title: "Approval".into(),
            description: String::new(),
            position: Default::default(),
            checkpoint_type: WorkflowHumanCheckpointTypeDto::Decision,
            prompt: "Approve?".into(),
            decision_options: vec!["approve".into()],
            resume_payload_schema: None,
            state_updates: vec![state_update.clone()],
        });
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Failed,
            "checkpoint-retry-source",
        )
        .expect("insert failed source");
        assert!(commit_workflow_route(
            temp.path(),
            "project-1",
            &run.id,
            &[WorkflowRouteDecisionRecord {
                source_node_run_id: source.id.clone(),
                source_status: WorkflowNodeRunStatusDto::Failed,
                from_node_id: "agent-a".into(),
                to_node_id: "approval".into(),
                edge_id: "failure-to-approval".into(),
                condition: json!({ "kind": "always" }),
                evidence: json!({ "matched": true }),
                target_node_type: "human_checkpoint".into(),
                target_attempt_number: 0,
                target_idempotency_key: "approval-attempt-0".into(),
            }],
        )
        .expect("route to checkpoint"));
        let first_checkpoint = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load routed run")
            .expect("run exists")
            .nodes
            .into_iter()
            .find(|node| node.node_id == "approval")
            .expect("checkpoint exists");
        assert!(compare_and_set_workflow_run_node(
            temp.path(),
            "project-1",
            &first_checkpoint.id,
            &[WorkflowNodeRunStatusDto::Eligible],
            WorkflowNodeRunStatusDto::WaitingOnGate,
            None,
            None,
            None,
        )
        .expect("wait on checkpoint"));
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Paused,
            Some(WorkflowTerminalStatusDto::NeedsHuman),
            None,
        )
        .expect("pause for checkpoint");
        let first_resume = WorkflowCheckpointResumeRecord {
            run_id: run.id.clone(),
            node_run_id: first_checkpoint.id.clone(),
            checkpoint_type: WorkflowHumanCheckpointTypeDto::Decision,
            decision: "approve".into(),
            payload: None,
            state_updates: vec![state_update.clone()],
        };
        assert!(
            resume_workflow_checkpoint_atomically(temp.path(), "project-1", &first_resume)
                .expect("resume first checkpoint")
        );

        assert!(retry_workflow_node_atomically(
            temp.path(),
            "project-1",
            &WorkflowNodeRetryRecord {
                run_id: run.id.clone(),
                source_node_run_id: source.id,
                node_id: source.node_id,
                node_type: source.node_type,
            },
        )
        .expect("retry upstream source"));
        let second_checkpoint = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "approval",
            "human_checkpoint",
            1,
            WorkflowNodeRunStatusDto::WaitingOnGate,
            "approval-attempt-1",
        )
        .expect("insert retried checkpoint");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Paused,
            Some(WorkflowTerminalStatusDto::NeedsHuman),
            None,
        )
        .expect("pause for retried checkpoint");
        let second_resume = WorkflowCheckpointResumeRecord {
            node_run_id: second_checkpoint.id.clone(),
            ..first_resume
        };
        assert!(
            resume_workflow_checkpoint_atomically(temp.path(), "project-1", &second_resume)
                .expect("resume retried checkpoint")
        );

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            loaded
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_checkpoint_state_written")
                .count(),
            2,
        );
        let connection = Connection::open(temp.path().join("state.db")).expect("open database");
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM delivery_projects WHERE project_id = 'project-1' AND id = 'checkpoint-retry-project'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("count delivery project"),
            1,
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM delivery_state_events WHERE project_id = 'project-1' AND workflow_run_id = ?1",
                    params![run.id],
                    |row| row.get::<_, i64>(0),
                )
                .expect("count durable state mutations"),
            1,
        );
    }

    #[test]
    fn checkpoint_resume_rolls_back_every_write_when_a_state_update_fails() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Paused,
            Some(WorkflowTerminalStatusDto::NeedsHuman),
            None,
        )
        .expect("pause run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "approval",
            "human_checkpoint",
            0,
            WorkflowNodeRunStatusDto::WaitingOnGate,
            "approval-attempt",
        )
        .expect("insert checkpoint");
        let error = resume_workflow_checkpoint_atomically(
            temp.path(),
            "project-1",
            &WorkflowCheckpointResumeRecord {
                run_id: run.id.clone(),
                node_run_id: node.id.clone(),
                checkpoint_type: WorkflowHumanCheckpointTypeDto::Decision,
                decision: "approve".into(),
                payload: None,
                state_updates: vec![
                    WorkflowStateWriteOperationDto {
                        entity_type: WorkflowDeliveryStateEntityTypeDto::DeliveryProject,
                        action: WorkflowStateWriteActionDto::Create,
                        idempotency_key: Some("checkpoint-project".into()),
                        target_id: Some("delivery-project-1".into()),
                        payload: serde_json::from_value(json!({ "title": "Must roll back" }))
                            .expect("object payload"),
                        output_artifact_type: "state_write_result".into(),
                    },
                    WorkflowStateWriteOperationDto {
                        entity_type: WorkflowDeliveryStateEntityTypeDto::Requirement,
                        action: WorkflowStateWriteActionDto::Create,
                        idempotency_key: Some("invalid-requirement".into()),
                        target_id: Some("requirement-1".into()),
                        payload: serde_json::from_value(json!({ "title": "Missing milestone" }))
                            .expect("object payload"),
                        output_artifact_type: "state_write_result".into(),
                    },
                ],
            },
        )
        .expect_err("invalid state update must abort the checkpoint resume");

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let persisted = loaded
            .nodes
            .iter()
            .find(|candidate| candidate.id == node.id)
            .expect("checkpoint exists");
        assert_eq!(error.code, "delivery_state_payload_invalid");
        assert_eq!(loaded.status, WorkflowRunStatusDto::Paused);
        assert_eq!(
            loaded.terminal_status,
            Some(WorkflowTerminalStatusDto::NeedsHuman)
        );
        assert_eq!(persisted.status, WorkflowNodeRunStatusDto::WaitingOnGate);
        assert!(loaded.gate_decisions.is_empty());
        assert!(loaded.artifacts.is_empty());
        assert!(!loaded.events.iter().any(|event| {
            event.event_type == "workflow_checkpoint_state_written"
                || event.event_type == "workflow_checkpoint_resumed"
        }));
        let connection = Connection::open(temp.path().join("state.db")).expect("open database");
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM delivery_projects WHERE project_id = 'project-1'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("count delivery projects"),
            0,
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM delivery_state_events WHERE project_id = 'project-1'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("count state events"),
            0,
        );
    }

    #[test]
    fn checkpoint_resume_rejects_a_node_that_is_not_waiting_in_a_paused_run() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "approval",
            "human_checkpoint",
            0,
            WorkflowNodeRunStatusDto::WaitingOnGate,
            "approval-attempt",
        )
        .expect("insert checkpoint");

        let error = resume_workflow_checkpoint_atomically(
            temp.path(),
            "project-1",
            &WorkflowCheckpointResumeRecord {
                run_id: run.id.clone(),
                node_run_id: node.id,
                checkpoint_type: WorkflowHumanCheckpointTypeDto::Decision,
                decision: "approve".into(),
                payload: None,
                state_updates: Vec::new(),
            },
        )
        .expect_err("queued run cannot resume a checkpoint");

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        assert_eq!(error.code, "workflow_checkpoint_not_waiting");
        assert_eq!(loaded.status, WorkflowRunStatusDto::Queued);
        assert!(loaded.gate_decisions.is_empty());
        assert!(loaded.artifacts.is_empty());
    }

    #[test]
    fn terminal_status_and_completion_event_roll_back_together() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        Connection::open(temp.path().join("state.db"))
            .expect("open database")
            .execute_batch(
                r#"
                CREATE TRIGGER reject_workflow_completion_event
                BEFORE INSERT ON workflow_events
                WHEN NEW.event_type = 'workflow_completed'
                BEGIN
                    SELECT RAISE(ABORT, 'simulated completion event failure');
                END;
                "#,
            )
            .expect("install failure trigger");

        complete_workflow_run_if_active(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Completed,
            WorkflowTerminalStatusDto::Success,
        )
        .expect_err("event failure must roll back terminal status");

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        assert_eq!(loaded.status, WorkflowRunStatusDto::Running);
        assert_eq!(loaded.terminal_status, None);
        assert!(!loaded
            .events
            .iter()
            .any(|event| event.event_type == "workflow_completed"));
    }

    #[test]
    fn retry_records_a_fresh_terminal_completion_event() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Failed,
            "retry-history-source",
        )
        .expect("insert failed node");

        assert!(complete_workflow_run_if_active(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Failed,
            WorkflowTerminalStatusDto::Failure,
        )
        .expect("record failed completion"));
        assert!(retry_workflow_node_atomically(
            temp.path(),
            "project-1",
            &WorkflowNodeRetryRecord {
                run_id: run.id.clone(),
                source_node_run_id: source.id,
                node_id: source.node_id,
                node_type: source.node_type,
            },
        )
        .expect("retry failed node"));
        assert!(complete_workflow_run_if_active(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Completed,
            WorkflowTerminalStatusDto::Success,
        )
        .expect("record successful retry"));

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let terminal_history = loaded
            .events
            .iter()
            .filter(|event| event.event_type == "workflow_completed")
            .map(|event| {
                event
                    .event
                    .get("terminalStatus")
                    .and_then(JsonValue::as_str)
                    .expect("terminal event payload")
            })
            .collect::<Vec<_>>();
        assert_eq!(terminal_history, ["failure", "success"]);
    }

    #[test]
    fn retry_rewinds_terminal_descendants_and_stale_route_evidence() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Failed,
            "retry-rewind-source",
        )
        .expect("insert failed source");
        assert!(commit_workflow_route(
            temp.path(),
            "project-1",
            &run.id,
            &[WorkflowRouteDecisionRecord {
                source_node_run_id: source.id.clone(),
                source_status: WorkflowNodeRunStatusDto::Failed,
                from_node_id: "agent-a".into(),
                to_node_id: "done".into(),
                edge_id: "failure-to-done".into(),
                condition: json!({ "kind": "always" }),
                evidence: json!({ "old": true }),
                target_node_type: "terminal".into(),
                target_attempt_number: 0,
                target_idempotency_key: format!("{}:done:0", run.id),
            }],
        )
        .expect("route failure to terminal"));
        let routed = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load routed run")
            .expect("run exists");
        let terminal = routed
            .nodes
            .iter()
            .find(|node| node.node_id == "done")
            .expect("terminal exists")
            .clone();
        insert_workflow_artifact(
            temp.path(),
            "project-1",
            &run.id,
            &terminal.id,
            "stale_terminal_output",
            1,
            &json!({ "stale": true }),
            None,
        )
        .expect("insert stale artifact");
        assert!(complete_workflow_terminal_atomically(
            temp.path(),
            "project-1",
            &run.id,
            &terminal.id,
            WorkflowRunStatusDto::Failed,
            WorkflowTerminalStatusDto::Failure,
        )
        .expect("complete failure terminal"));

        assert!(retry_workflow_node_atomically(
            temp.path(),
            "project-1",
            &WorkflowNodeRetryRecord {
                run_id: run.id.clone(),
                source_node_run_id: source.id,
                node_id: source.node_id,
                node_type: source.node_type,
            },
        )
        .expect("retry source"));

        let retried = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load retried run")
            .expect("run exists");
        assert_eq!(retried.status, WorkflowRunStatusDto::Running);
        assert_eq!(
            retried
                .nodes
                .iter()
                .find(|node| node.id == terminal.id)
                .expect("old terminal remains as history")
                .status,
            WorkflowNodeRunStatusDto::Cancelled,
        );
        assert!(retried.artifacts.is_empty());
        assert!(retried.edge_decisions.is_empty());
        assert_eq!(
            retried
                .nodes
                .iter()
                .find(|node| node.node_id == "agent-a" && node.attempt_number == 1)
                .expect("retry attempt exists")
                .status,
            WorkflowNodeRunStatusDto::Eligible,
        );
    }

    #[test]
    fn retry_rejects_a_live_agent_handoff_leaf_before_rewinding_other_nodes() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let failed = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Failed,
            "retry-handoff-failed",
        )
        .expect("insert failed node");
        let handed_off = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-handoff",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Running,
            "retry-handoff-source",
        )
        .expect("insert handed-off owner");
        persist_test_agent_handoff_chain(
            temp.path(),
            "retry-handoff-source",
            "retry-handoff-target",
            "running",
        );
        assert!(compare_and_set_workflow_run_node(
            temp.path(),
            "project-1",
            &handed_off.id,
            &[WorkflowNodeRunStatusDto::Running],
            WorkflowNodeRunStatusDto::Running,
            Some("retry-handoff-source"),
            Some("session-retry-handoff-source"),
            None,
        )
        .expect("attach source run to Workflow node"));

        let error = retry_workflow_node_atomically(
            temp.path(),
            "project-1",
            &WorkflowNodeRetryRecord {
                run_id: run.id.clone(),
                source_node_run_id: failed.id.clone(),
                node_id: failed.node_id.clone(),
                node_type: failed.node_type.clone(),
            },
        )
        .expect_err("live handoff target must block retry rewind");

        assert_eq!(error.code, "workflow_retry_execution_still_active");
        let unchanged = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load unchanged run")
            .expect("run exists");
        assert_eq!(
            unchanged
                .nodes
                .iter()
                .filter(|node| node.node_id == "agent-a")
                .count(),
            1,
        );
        assert!(!unchanged.events.iter().any(|event| {
            event.event_type == "workflow_node_retry_requested"
                && event.node_run_id.as_deref() == Some(failed.id.as_str())
        }));
    }

    #[test]
    fn retry_preserves_unrelated_loop_attempt_budgets() {
        let (temp, mut definition) = repo_with_database();
        definition.nodes.push(WorkflowNodeDto::Terminal {
            id: "other".into(),
            title: "Other branch".into(),
            description: String::new(),
            position: Default::default(),
            terminal_status: WorkflowTerminalStatusDto::Success,
        });
        let loop_policy = |loop_key: &str| WorkflowLoopPolicyDto {
            loop_key: loop_key.into(),
            max_attempts: 3,
            attempt_scope: Default::default(),
            carryover_policy: Default::default(),
            selected_artifact_refs: Vec::new(),
            reset_policy: Default::default(),
            stall_detector: None,
            on_exhausted: "done".into(),
        };
        definition.edges.extend([
            WorkflowEdgeDto {
                id: "retry-loop".into(),
                from_node_id: "agent-a".into(),
                to_node_id: "done".into(),
                r#type: WorkflowEdgeTypeDto::Loop,
                label: String::new(),
                priority: 20,
                condition: Default::default(),
                loop_policy: Some(loop_policy("retry-loop")),
            },
            WorkflowEdgeDto {
                id: "unrelated-loop".into(),
                from_node_id: "other".into(),
                to_node_id: "done".into(),
                r#type: WorkflowEdgeTypeDto::Loop,
                label: String::new(),
                priority: 20,
                condition: Default::default(),
                loop_policy: Some(loop_policy("unrelated-loop")),
            },
        ]);
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Failed,
            "retry-loop-source",
        )
        .expect("insert failed source");
        let unrelated = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "other",
            "terminal",
            0,
            WorkflowNodeRunStatusDto::Succeeded,
            "unrelated-loop-source",
        )
        .expect("insert unrelated node");
        increment_workflow_loop_attempt(
            temp.path(),
            "project-1",
            &run.id,
            "retry-loop",
            &source.id,
            true,
        )
        .expect("record retry loop attempt");
        increment_workflow_loop_attempt(
            temp.path(),
            "project-1",
            &run.id,
            "unrelated-loop",
            &unrelated.id,
            true,
        )
        .expect("record unrelated loop attempt");

        assert!(retry_workflow_node_atomically(
            temp.path(),
            "project-1",
            &WorkflowNodeRetryRecord {
                run_id: run.id.clone(),
                source_node_run_id: source.id,
                node_id: source.node_id,
                node_type: source.node_type,
            },
        )
        .expect("retry source"));

        let retried = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load retried run")
            .expect("run exists");
        assert_eq!(
            retried
                .loop_attempts
                .iter()
                .map(|attempt| attempt.loop_key.as_str())
                .collect::<Vec<_>>(),
            ["unrelated-loop"],
        );
        assert!(retried.loop_attempts[0].exhausted);
    }

    #[test]
    fn retry_resets_namespaced_subgraph_loop_without_touching_other_loop_budgets() {
        let (temp, mut definition) = repo_with_database();
        let subgraph_loop_policy = WorkflowLoopPolicyDto {
            loop_key: "review-retry".into(),
            max_attempts: 3,
            attempt_scope: Default::default(),
            carryover_policy: Default::default(),
            selected_artifact_refs: Vec::new(),
            reset_policy: Default::default(),
            stall_detector: None,
            on_exhausted: "local-done".into(),
        };
        definition.nodes.push(WorkflowNodeDto::Subgraph {
            id: "review-phase".into(),
            title: "Review phase".into(),
            description: String::new(),
            position: Default::default(),
            subgraph_id: "review-subgraph".into(),
            input_bindings: Vec::new(),
            output_contract: WorkflowOutputContractDto::default(),
        });
        definition.subgraphs.push(WorkflowSubgraphDto {
            id: "review-subgraph".into(),
            title: "Review".into(),
            description: String::new(),
            start_node_id: "router".into(),
            nodes: vec![
                WorkflowNodeDto::Router {
                    id: "router".into(),
                    title: "Route review".into(),
                    description: String::new(),
                    position: Default::default(),
                },
                WorkflowNodeDto::Terminal {
                    id: "local-done".into(),
                    title: "Done".into(),
                    description: String::new(),
                    position: Default::default(),
                    terminal_status: WorkflowTerminalStatusDto::Success,
                },
            ],
            edges: vec![WorkflowEdgeDto {
                id: "review-loop".into(),
                from_node_id: "router".into(),
                to_node_id: "local-done".into(),
                r#type: WorkflowEdgeTypeDto::Loop,
                label: String::new(),
                priority: 20,
                condition: Default::default(),
                loop_policy: Some(subgraph_loop_policy),
            }],
            input_bindings: Vec::new(),
            output_contract: WorkflowOutputContractDto::default(),
        });
        definition.edges.push(WorkflowEdgeDto {
            id: "unrelated-main-loop".into(),
            from_node_id: "agent-a".into(),
            to_node_id: "done".into(),
            r#type: WorkflowEdgeTypeDto::Loop,
            label: String::new(),
            priority: 20,
            condition: Default::default(),
            loop_policy: Some(WorkflowLoopPolicyDto {
                loop_key: "main-retry".into(),
                max_attempts: 3,
                attempt_scope: Default::default(),
                carryover_policy: Default::default(),
                selected_artifact_refs: Vec::new(),
                reset_policy: Default::default(),
                stall_detector: None,
                on_exhausted: "done".into(),
            }),
        });
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "review-phase::router",
            "router",
            0,
            WorkflowNodeRunStatusDto::Failed,
            "subgraph-loop-source",
        )
        .expect("insert failed subgraph node");
        let unrelated = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Succeeded,
            "main-loop-source",
        )
        .expect("insert unrelated main node");
        increment_workflow_loop_attempt(
            temp.path(),
            "project-1",
            &run.id,
            "review-phase::review-retry",
            &source.id,
            true,
        )
        .expect("record subgraph loop attempt");
        increment_workflow_loop_attempt(
            temp.path(),
            "project-1",
            &run.id,
            "main-retry",
            &unrelated.id,
            true,
        )
        .expect("record unrelated loop attempt");

        assert!(retry_workflow_node_atomically(
            temp.path(),
            "project-1",
            &WorkflowNodeRetryRecord {
                run_id: run.id.clone(),
                source_node_run_id: source.id,
                node_id: source.node_id,
                node_type: source.node_type,
            },
        )
        .expect("retry subgraph source"));

        let retried = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load retried run")
            .expect("run exists");
        assert_eq!(
            retried
                .loop_attempts
                .iter()
                .map(|attempt| attempt.loop_key.as_str())
                .collect::<Vec<_>>(),
            ["main-retry"],
        );
        assert!(retried.loop_attempts[0].exhausted);
    }

    #[test]
    fn terminal_completion_and_cancellation_linearize_without_contradictory_events() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");

        let barrier = Arc::new(Barrier::new(2));
        let completion = {
            let barrier = Arc::clone(&barrier);
            let repo_root = temp.path().to_path_buf();
            let run_id = run.id.clone();
            std::thread::spawn(move || {
                barrier.wait();
                complete_workflow_run_if_active(
                    &repo_root,
                    "project-1",
                    &run_id,
                    WorkflowRunStatusDto::Completed,
                    WorkflowTerminalStatusDto::Success,
                )
            })
        };
        let cancellation = {
            let barrier = Arc::clone(&barrier);
            let repo_root = temp.path().to_path_buf();
            let run_id = run.id.clone();
            std::thread::spawn(move || {
                barrier.wait();
                request_workflow_run_cancellation(&repo_root, "project-1", &run_id, Some("stop"))?;
                cancel_workflow_run_execution(&repo_root, "project-1", &run_id, Some("stop"))
            })
        };
        let completion = completion.join().expect("join completion");
        let cancellation = cancellation.join().expect("join cancellation");

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let completion_events = loaded
            .events
            .iter()
            .filter(|event| event.event_type == "workflow_completed")
            .count();
        match loaded.status {
            WorkflowRunStatusDto::Completed => {
                assert_eq!(completion.expect("completion wins"), true);
                assert_eq!(
                    cancellation
                        .expect_err("completed run is not cancellable")
                        .code,
                    "workflow_run_not_cancellable"
                );
                assert_eq!(completion_events, 1);
            }
            WorkflowRunStatusDto::Cancelled => {
                assert_eq!(completion.expect("completion observes cancellation"), false);
                assert_eq!(cancellation.expect("cancellation wins"), true);
                assert_eq!(completion_events, 0);
            }
            status => panic!("unexpected terminal race result: {status:?}"),
        }
    }

    #[test]
    fn command_completion_rolls_back_when_artifact_is_invalid() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "command",
            0,
            WorkflowNodeRunStatusDto::Starting,
            "command-attempt",
        )
        .expect("insert command node");
        persist_test_command_lease(temp.path(), &run.id, &node.id, "test-owner", "test-lease");

        let error = complete_workflow_command_node(
            temp.path(),
            "project-1",
            &WorkflowCommandCompletionRecord {
                run_id: run.id.clone(),
                node_run_id: node.id.clone(),
                artifact_type: "command_output".into(),
                schema_version: 0,
                payload: json!({ "status": "passed" }),
                render_text: Some("done".into()),
                event: json!({ "status": "passed" }),
                status: WorkflowNodeRunStatusDto::Succeeded,
                failure_class: None,
                owner_instance_id: "test-owner".into(),
                lease_token: "test-lease".into(),
            },
        )
        .expect_err("invalid artifact must abort completion");

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let persisted = loaded
            .nodes
            .iter()
            .find(|candidate| candidate.id == node.id)
            .expect("command node exists");
        assert_eq!(error.code, "workflow_artifact_insert_failed");
        assert_eq!(
            (
                persisted.status,
                loaded.artifacts.len(),
                loaded.events.len()
            ),
            (WorkflowNodeRunStatusDto::Starting, 0, 1),
            "the pre-existing workflow_started event is the only event that may remain",
        );
    }

    #[test]
    fn command_node_and_durable_lease_have_exactly_one_cross_connection_owner() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "command-a",
            "command",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "command-lease-attempt",
        )
        .expect("insert command node");
        let barrier = Arc::new(Barrier::new(2));
        let handles = [("owner-a", "lease-a"), ("owner-b", "lease-b")].map(|(owner, token)| {
            let barrier = Arc::clone(&barrier);
            let repo_root = temp.path().to_path_buf();
            let run_id = run.id.clone();
            let node_run_id = node.id.clone();
            std::thread::spawn(move || {
                barrier.wait();
                claim_workflow_command_node_starting(
                    &repo_root,
                    "project-1",
                    &run_id,
                    &node_run_id,
                    owner,
                    std::process::id(),
                    &crate::runtime::process_tree::process_birth_identity(std::process::id())
                        .expect("current process identity"),
                    token,
                    "2026-07-15T12:00:00Z",
                )
                .map(|claimed| (owner, token, claimed))
            })
        });
        let results = handles.map(|handle| handle.join().expect("join claim").expect("claim"));

        assert_eq!(results.iter().filter(|(_, _, claimed)| *claimed).count(), 1);
        let (owner, token, _) = results
            .iter()
            .find(|(_, _, claimed)| *claimed)
            .expect("one claim wins");
        let lease = load_workflow_command_lease(temp.path(), "project-1", &node.id)
            .expect("load lease")
            .expect("lease exists");
        assert_eq!(lease.owner_instance_id, *owner);
        assert_eq!(lease.lease_token, *token);
        assert!(attach_workflow_command_process(
            temp.path(),
            "project-1",
            &node.id,
            owner,
            token,
            42_000,
            None,
            "2026-07-15T12:00:01Z",
        )
        .expect("attach process"));
        assert!(!renew_workflow_command_lease(
            temp.path(),
            "project-1",
            &node.id,
            "different-owner",
            token,
            "2026-07-15T12:00:02Z",
        )
        .expect("reject foreign heartbeat"));
        let error = complete_workflow_command_node(
            temp.path(),
            "project-1",
            &WorkflowCommandCompletionRecord {
                run_id: run.id.clone(),
                node_run_id: node.id.clone(),
                artifact_type: "command_output".into(),
                schema_version: 1,
                payload: json!({ "status": "passed" }),
                render_text: Some("foreign".into()),
                event: json!({ "status": "passed" }),
                status: WorkflowNodeRunStatusDto::Succeeded,
                failure_class: None,
                owner_instance_id: "different-owner".into(),
                lease_token: token.to_string(),
            },
        )
        .expect_err("foreign completion must not commit");
        assert_eq!(error.code, "workflow_command_completion_conflict");
        assert!(
            load_workflow_command_lease(temp.path(), "project-1", &node.id)
                .expect("load lease after foreign completion")
                .is_some()
        );
    }

    #[test]
    fn stale_recovery_cannot_take_a_lease_that_was_renewed_after_it_was_read() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "command-a",
            "command",
            0,
            WorkflowNodeRunStatusDto::Running,
            "command-recovery-attempt",
        )
        .expect("insert command node");
        persist_test_command_lease(temp.path(), &run.id, &node.id, "owner-a", "lease-a");
        let stale = load_workflow_command_lease(temp.path(), "project-1", &node.id)
            .expect("load stale lease")
            .expect("lease exists");
        assert!(renew_workflow_command_lease(
            temp.path(),
            "project-1",
            &node.id,
            "owner-a",
            "lease-a",
            "2026-07-15T12:00:05Z",
        )
        .expect("renew lease"));

        assert!(!claim_interrupted_workflow_command(
            temp.path(),
            "project-1",
            &run.id,
            &stale,
            "workflow_command_interrupted",
        )
        .expect("stale recovery loses"));
        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            loaded
                .nodes
                .iter()
                .find(|candidate| candidate.id == node.id)
                .expect("node exists")
                .status,
            WorkflowNodeRunStatusDto::Running,
        );
        assert_eq!(
            load_workflow_command_lease(temp.path(), "project-1", &node.id)
                .expect("load current lease")
                .expect("lease remains")
                .heartbeat_at,
            "2026-07-15T12:00:05Z",
        );
    }

    #[test]
    fn command_completion_is_idempotent_after_success() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "command",
            0,
            WorkflowNodeRunStatusDto::Starting,
            "command-attempt",
        )
        .expect("insert command node");
        persist_test_command_lease(temp.path(), &run.id, &node.id, "test-owner", "test-lease");
        let completion = WorkflowCommandCompletionRecord {
            run_id: run.id.clone(),
            node_run_id: node.id.clone(),
            artifact_type: "command_output".into(),
            schema_version: 1,
            payload: json!({ "status": "passed" }),
            render_text: Some("done".into()),
            event: json!({ "status": "passed" }),
            status: WorkflowNodeRunStatusDto::Succeeded,
            failure_class: None,
            owner_instance_id: "test-owner".into(),
            lease_token: "test-lease".into(),
        };

        complete_workflow_command_node(temp.path(), "project-1", &completion)
            .expect("complete command");
        complete_workflow_command_node(temp.path(), "project-1", &completion)
            .expect("repeat completion after ambiguous acknowledgement");

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let persisted = loaded
            .nodes
            .iter()
            .find(|candidate| candidate.id == node.id)
            .expect("command node exists");
        assert_eq!(
            (
                persisted.status,
                loaded.artifacts.len(),
                loaded
                    .events
                    .iter()
                    .filter(|event| event.event_type == "workflow_command_completed")
                    .count(),
            ),
            (WorkflowNodeRunStatusDto::Succeeded, 1, 1),
        );
    }

    #[test]
    fn agent_completion_rolls_back_status_when_artifact_is_invalid() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Running,
            "agent-attempt",
        )
        .expect("insert running agent node");

        complete_workflow_agent_node_with_artifact(
            temp.path(),
            "project-1",
            &WorkflowAgentArtifactCompletionRecord {
                run_id: run.id.clone(),
                node_run_id: node.id.clone(),
                artifact_type: "agent_output".into(),
                schema_version: 0,
                payload: json!({ "status": "done" }),
                render_text: Some("done".into()),
                event: json!({ "status": "done" }),
            },
        )
        .expect_err("invalid artifact must roll back agent completion");

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let persisted = loaded
            .nodes
            .iter()
            .find(|candidate| candidate.id == node.id)
            .expect("agent node exists");
        assert_eq!(persisted.status, WorkflowNodeRunStatusDto::Running);
        assert!(loaded.artifacts.is_empty());
        assert!(!loaded
            .events
            .iter()
            .any(|event| event.event_type == "workflow_artifact_extracted"));
    }

    #[test]
    fn agent_completion_accepts_a_child_that_completed_while_parent_waited_on_its_gate() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::WaitingOnGate,
            "agent-waiting-attempt",
        )
        .expect("insert waiting agent node");

        assert!(complete_workflow_agent_node_with_artifact(
            temp.path(),
            "project-1",
            &WorkflowAgentArtifactCompletionRecord {
                run_id: run.id.clone(),
                node_run_id: node.id.clone(),
                artifact_type: "agent_output".into(),
                schema_version: 1,
                payload: json!({ "status": "done" }),
                render_text: Some("done".into()),
                event: json!({ "status": "done" }),
            },
        )
        .expect("complete waiting agent node"));

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let persisted = loaded
            .nodes
            .iter()
            .find(|candidate| candidate.id == node.id)
            .expect("agent node exists");
        assert_eq!(persisted.status, WorkflowNodeRunStatusDto::Succeeded);
        assert_eq!(loaded.artifacts.len(), 1);
    }

    #[test]
    fn workflow_route_rolls_back_every_target_when_one_target_is_invalid() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Succeeded,
            "source-attempt",
        )
        .expect("insert source node");
        let decisions = vec![
            WorkflowRouteDecisionRecord {
                source_node_run_id: source.id.clone(),
                source_status: WorkflowNodeRunStatusDto::Succeeded,
                from_node_id: "agent-a".into(),
                to_node_id: "done".into(),
                edge_id: "edge-done".into(),
                condition: json!({ "kind": "always" }),
                evidence: json!({ "matched": true }),
                target_node_type: "terminal".into(),
                target_attempt_number: 0,
                target_idempotency_key: "done-attempt".into(),
            },
            WorkflowRouteDecisionRecord {
                source_node_run_id: source.id,
                source_status: WorkflowNodeRunStatusDto::Succeeded,
                from_node_id: "agent-a".into(),
                to_node_id: "invalid".into(),
                edge_id: "edge-invalid".into(),
                condition: json!({ "kind": "always" }),
                evidence: json!({ "matched": true }),
                target_node_type: "not-a-node-type".into(),
                target_attempt_number: 0,
                target_idempotency_key: "invalid-attempt".into(),
            },
        ];

        commit_workflow_route(temp.path(), "project-1", &run.id, &decisions)
            .expect_err("invalid second target must roll back the route");

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            (
                loaded
                    .nodes
                    .iter()
                    .filter(|node| node.node_id == "done")
                    .count(),
                loaded.edge_decisions.len(),
                loaded
                    .events
                    .iter()
                    .filter(|event| event.event_type == "workflow_node_routed")
                    .count(),
            ),
            (0, 0, 0),
        );
    }

    #[test]
    fn workflow_route_commit_is_idempotent_after_success() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Succeeded,
            "source-attempt",
        )
        .expect("insert source node");
        let decisions = vec![WorkflowRouteDecisionRecord {
            source_node_run_id: source.id,
            source_status: WorkflowNodeRunStatusDto::Succeeded,
            from_node_id: "agent-a".into(),
            to_node_id: "done".into(),
            edge_id: "edge-done".into(),
            condition: json!({ "kind": "always" }),
            evidence: json!({ "matched": true }),
            target_node_type: "terminal".into(),
            target_attempt_number: 0,
            target_idempotency_key: "done-attempt".into(),
        }];

        commit_workflow_route(temp.path(), "project-1", &run.id, &decisions).expect("commit route");
        commit_workflow_route(temp.path(), "project-1", &run.id, &decisions)
            .expect("repeat route after ambiguous acknowledgement");

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            (
                loaded
                    .nodes
                    .iter()
                    .filter(|node| node.node_id == "done")
                    .count(),
                loaded.edge_decisions.len(),
                loaded
                    .events
                    .iter()
                    .filter(|event| event.event_type == "workflow_node_routed")
                    .count(),
            ),
            (1, 1, 1),
        );
    }

    #[test]
    fn late_command_completion_after_cancel_is_rejected_without_side_effects() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "command",
            0,
            WorkflowNodeRunStatusDto::Starting,
            "command-attempt",
        )
        .expect("insert command node");
        persist_test_command_lease(temp.path(), &run.id, &node.id, "test-owner", "test-lease");

        assert!(
            request_workflow_run_cancellation(temp.path(), "project-1", &run.id, Some("stop"),)
                .expect("request cancellation")
        );
        let active_error =
            cancel_workflow_run_execution(temp.path(), "project-1", &run.id, Some("stop"))
                .expect_err("live command lease blocks cancellation finalization");
        assert_eq!(
            active_error.code,
            "workflow_cancellation_execution_still_active"
        );
        assert!(release_workflow_command_lease(
            temp.path(),
            "project-1",
            &node.id,
            "test-owner",
            "test-lease",
        )
        .expect("release terminated command lease"));
        assert!(
            cancel_workflow_run_execution(temp.path(), "project-1", &run.id, Some("stop"),)
                .expect("finalize cancellation")
        );
        assert!(
            load_workflow_command_lease(temp.path(), "project-1", &node.id)
                .expect("load cancelled command lease")
                .is_none()
        );
        let error = complete_workflow_command_node(
            temp.path(),
            "project-1",
            &WorkflowCommandCompletionRecord {
                run_id: run.id.clone(),
                node_run_id: node.id.clone(),
                artifact_type: "command_output".into(),
                schema_version: 1,
                payload: json!({ "status": "passed" }),
                render_text: Some("late".into()),
                event: json!({ "status": "passed" }),
                status: WorkflowNodeRunStatusDto::Succeeded,
                failure_class: None,
                owner_instance_id: "test-owner".into(),
                lease_token: "test-lease".into(),
            },
        )
        .expect_err("late completion must lose to cancellation");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("stale reconcile status update is ignored");
        let stale_route = vec![WorkflowRouteDecisionRecord {
            source_node_run_id: node.id.clone(),
            source_status: WorkflowNodeRunStatusDto::Succeeded,
            from_node_id: "agent-a".into(),
            to_node_id: "done".into(),
            edge_id: "edge-done".into(),
            condition: json!({ "kind": "always" }),
            evidence: json!({ "matched": true }),
            target_node_type: "terminal".into(),
            target_attempt_number: 0,
            target_idempotency_key: "done-attempt".into(),
        }];
        assert!(
            !commit_workflow_route(temp.path(), "project-1", &run.id, &stale_route)
                .expect("cancelled run rejects stale routing")
        );

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let persisted = loaded
            .nodes
            .iter()
            .find(|candidate| candidate.id == node.id)
            .expect("command node exists");
        assert_eq!(error.code, "workflow_command_completion_conflict");
        assert_eq!(loaded.status, WorkflowRunStatusDto::Cancelled);
        assert_eq!(persisted.status, WorkflowNodeRunStatusDto::Cancelled);
        assert!(loaded.artifacts.is_empty());
        assert!(!loaded.nodes.iter().any(|node| node.node_id == "done"));
        assert!(!loaded
            .events
            .iter()
            .any(|event| event.event_type == "workflow_command_completed"));
    }

    fn assert_late_success_is_blocked_by_control_status(control_status: WorkflowNodeRunStatusDto) {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Running,
            "source-attempt",
        )
        .expect("insert running source");
        assert!(compare_and_set_workflow_run_node(
            temp.path(),
            "project-1",
            &source.id,
            &[WorkflowNodeRunStatusDto::Running],
            control_status,
            None,
            None,
            Some(control_status.as_str()),
        )
        .expect("persist control status"));
        assert!(!complete_workflow_agent_node_with_artifact(
            temp.path(),
            "project-1",
            &WorkflowAgentArtifactCompletionRecord {
                run_id: run.id.clone(),
                node_run_id: source.id.clone(),
                artifact_type: "agent_output".into(),
                schema_version: 1,
                payload: json!({ "status": "late" }),
                render_text: Some("late".into()),
                event: json!({ "status": "late" }),
            },
        )
        .expect("reject late agent completion"));
        let decisions = vec![WorkflowRouteDecisionRecord {
            source_node_run_id: source.id.clone(),
            source_status: WorkflowNodeRunStatusDto::Succeeded,
            from_node_id: "agent-a".into(),
            to_node_id: "done".into(),
            edge_id: "edge-done".into(),
            condition: json!({ "kind": "always" }),
            evidence: json!({ "matched": true }),
            target_node_type: "terminal".into(),
            target_attempt_number: 0,
            target_idempotency_key: "done-attempt".into(),
        }];
        assert!(
            !commit_workflow_route(temp.path(), "project-1", &run.id, &decisions)
                .expect("reject stale success route")
        );

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let persisted = loaded
            .nodes
            .iter()
            .find(|candidate| candidate.id == source.id)
            .expect("source exists");
        assert_eq!(persisted.status, control_status);
        assert!(!loaded.nodes.iter().any(|node| node.node_id == "done"));
        assert!(loaded.artifacts.is_empty());
        assert!(loaded.edge_decisions.is_empty());
        assert!(!loaded
            .events
            .iter()
            .any(|event| event.event_type == "workflow_node_routed"));
    }

    #[test]
    fn late_completion_cannot_overwrite_skip_or_emit_success_route() {
        assert_late_success_is_blocked_by_control_status(WorkflowNodeRunStatusDto::Skipped);
    }

    #[test]
    fn late_completion_cannot_overwrite_stall_or_emit_success_route() {
        assert_late_success_is_blocked_by_control_status(WorkflowNodeRunStatusDto::Stalled);
    }

    #[test]
    fn concurrent_retry_commits_exactly_one_next_attempt_and_marker() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Failed,
            "retry-source",
        )
        .expect("insert failed node");
        assert!(complete_workflow_run_if_active(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Failed,
            WorkflowTerminalStatusDto::Failure,
        )
        .expect("fail run"));

        let barrier = Arc::new(Barrier::new(2));
        let handles = [(), ()].map(|()| {
            let barrier = Arc::clone(&barrier);
            let repo_root = temp.path().to_path_buf();
            let retry = WorkflowNodeRetryRecord {
                run_id: run.id.clone(),
                source_node_run_id: source.id.clone(),
                node_id: source.node_id.clone(),
                node_type: source.node_type.clone(),
            };
            std::thread::spawn(move || {
                barrier.wait();
                retry_workflow_node_atomically(&repo_root, "project-1", &retry)
            })
        });
        let results = handles.map(|handle| handle.join().expect("join retry").expect("retry"));

        assert_eq!(results.iter().filter(|committed| **committed).count(), 1);
        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let attempts = loaded
            .nodes
            .iter()
            .filter(|node| node.node_id == "agent-a")
            .collect::<Vec<_>>();
        assert_eq!(loaded.status, WorkflowRunStatusDto::Running);
        assert_eq!(loaded.terminal_status, None);
        assert_eq!(attempts.len(), 2);
        assert_eq!(
            attempts
                .iter()
                .find(|node| node.attempt_number == 1)
                .expect("retry attempt")
                .status,
            WorkflowNodeRunStatusDto::Eligible,
        );
        assert_eq!(
            loaded
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_node_retry_requested")
                .count(),
            1,
        );
    }

    #[test]
    fn branch_skip_replays_once_and_requires_an_explicit_merge_target() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Running,
            "skip-source",
        )
        .expect("insert source");
        let no_merge = WorkflowBranchSkipRecord {
            run_id: run.id.clone(),
            node_run_id: source.id.clone(),
            node_id: source.node_id.clone(),
            previous_status: source.status,
            reason: Some("not needed".into()),
            merge_targets: Vec::new(),
        };
        let error = skip_workflow_branch_atomically(temp.path(), "project-1", &no_merge)
            .expect_err("skip without merge is unsafe");
        assert_eq!(error.code, "workflow_branch_skip_requires_merge_target");

        let skip = WorkflowBranchSkipRecord {
            merge_targets: vec![("merge-a".into(), "merge".into())],
            ..no_merge
        };
        assert!(
            skip_workflow_branch_atomically(temp.path(), "project-1", &skip).expect("commit skip")
        );
        assert!(
            !skip_workflow_branch_atomically(temp.path(), "project-1", &skip).expect("replay skip")
        );

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let persisted_source = loaded
            .nodes
            .iter()
            .find(|node| node.id == source.id)
            .expect("source exists");
        assert_eq!(persisted_source.status, WorkflowNodeRunStatusDto::Skipped);
        assert_eq!(
            persisted_source.failure_class.as_deref(),
            Some("skipped_by_user")
        );
        assert_eq!(
            loaded
                .nodes
                .iter()
                .filter(|node| node.node_id == "merge-a")
                .count(),
            1,
        );
        assert_eq!(
            loaded
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_branch_skipped")
                .count(),
            1,
        );
    }

    #[test]
    fn state_write_and_query_are_exactly_once_under_concurrent_reconcile() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let write_node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "write-state",
            "state_write",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "write-state-attempt",
        )
        .expect("insert state write");
        let operation = WorkflowStateWriteOperationDto {
            entity_type: WorkflowDeliveryStateEntityTypeDto::DeliveryProject,
            action: WorkflowStateWriteActionDto::Create,
            idempotency_key: None,
            target_id: Some("delivery-project-1".into()),
            payload: serde_json::from_value(json!({ "title": "Exactly once" }))
                .expect("object payload"),
            output_artifact_type: "state_write_result".into(),
        };
        let barrier = Arc::new(Barrier::new(2));
        let handles = [(), ()].map(|()| {
            let barrier = Arc::clone(&barrier);
            let repo_root = temp.path().to_path_buf();
            let run_id = run.id.clone();
            let node_run_id = write_node.id.clone();
            let operation = operation.clone();
            std::thread::spawn(move || {
                barrier.wait();
                complete_workflow_state_write_node_atomically(
                    &repo_root,
                    "project-1",
                    &run_id,
                    &node_run_id,
                    "write-state",
                    &operation,
                )
            })
        });
        let writes = handles.map(|handle| handle.join().expect("join write").expect("write"));
        assert_eq!(writes.iter().filter(|committed| **committed).count(), 1);

        let query_node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "query-state",
            "state_query",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "query-state-attempt",
        )
        .expect("insert state query");
        let query = WorkflowStateQueryDto {
            entity_type: WorkflowDeliveryStateEntityTypeDto::DeliveryProject,
            filters: Vec::new(),
            order_by: None,
            limit: None,
            include_archived: false,
        };
        let barrier = Arc::new(Barrier::new(2));
        let handles = [(), ()].map(|()| {
            let barrier = Arc::clone(&barrier);
            let repo_root = temp.path().to_path_buf();
            let run_id = run.id.clone();
            let node_run_id = query_node.id.clone();
            let query = query.clone();
            std::thread::spawn(move || {
                barrier.wait();
                complete_workflow_state_query_node_atomically(
                    &repo_root,
                    "project-1",
                    &run_id,
                    &node_run_id,
                    "query-state",
                    &query,
                    "state_query_result",
                )
            })
        });
        let queries = handles.map(|handle| handle.join().expect("join query").expect("query"));
        assert_eq!(queries.iter().filter(|committed| **committed).count(), 1);

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            loaded
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_state_written")
                .count(),
            1,
        );
        assert_eq!(
            loaded
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_state_read")
                .count(),
            1,
        );
        assert_eq!(
            loaded
                .artifacts
                .iter()
                .filter(|artifact| artifact.artifact_type == "state_write_result")
                .count(),
            1,
        );
        assert_eq!(
            loaded
                .artifacts
                .iter()
                .filter(|artifact| artifact.artifact_type == "state_query_result")
                .count(),
            1,
        );
        let connection = Connection::open(temp.path().join("state.db")).expect("open database");
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM delivery_projects WHERE project_id = 'project-1'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("count delivery projects"),
            1,
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM delivery_state_events WHERE project_id = 'project-1' AND node_run_id = ?1",
                    params![write_node.id],
                    |row| row.get::<_, i64>(0),
                )
                .expect("count state events"),
            1,
        );
    }

    #[test]
    fn retried_state_write_replays_its_committed_result_without_mutating_state_twice() {
        let (temp, mut definition) = repo_with_database();
        let operation = WorkflowStateWriteOperationDto {
            entity_type: WorkflowDeliveryStateEntityTypeDto::DeliveryProject,
            action: WorkflowStateWriteActionDto::Create,
            idempotency_key: None,
            target_id: Some("delivery-project-retry".into()),
            payload: serde_json::from_value(json!({ "title": "Retry-safe project" }))
                .expect("object payload"),
            output_artifact_type: "state_write_result".into(),
        };
        definition.nodes.push(WorkflowNodeDto::StateWrite {
            id: "write-state".into(),
            title: "Write state".into(),
            description: String::new(),
            position: Default::default(),
            input_bindings: Vec::new(),
            operation: operation.clone(),
        });
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Failed,
            "state-write-retry-source",
        )
        .expect("insert failed source");
        assert!(commit_workflow_route(
            temp.path(),
            "project-1",
            &run.id,
            &[WorkflowRouteDecisionRecord {
                source_node_run_id: source.id.clone(),
                source_status: WorkflowNodeRunStatusDto::Failed,
                from_node_id: "agent-a".into(),
                to_node_id: "write-state".into(),
                edge_id: "failure-to-write-state".into(),
                condition: json!({ "kind": "always" }),
                evidence: json!({ "matched": true }),
                target_node_type: "state_write".into(),
                target_attempt_number: 0,
                target_idempotency_key: "write-state-attempt-0".into(),
            }],
        )
        .expect("route to state write"));
        let first_write = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load routed run")
            .expect("run exists")
            .nodes
            .into_iter()
            .find(|node| node.node_id == "write-state")
            .expect("state write exists");
        assert!(complete_workflow_state_write_node_atomically(
            temp.path(),
            "project-1",
            &run.id,
            &first_write.id,
            "write-state",
            &operation,
        )
        .expect("complete first state write"));

        assert!(retry_workflow_node_atomically(
            temp.path(),
            "project-1",
            &WorkflowNodeRetryRecord {
                run_id: run.id.clone(),
                source_node_run_id: source.id,
                node_id: source.node_id,
                node_type: source.node_type,
            },
        )
        .expect("retry upstream source"));
        let second_write = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "write-state",
            "state_write",
            1,
            WorkflowNodeRunStatusDto::Eligible,
            "write-state-attempt-1",
        )
        .expect("insert retried state write");
        assert!(complete_workflow_state_write_node_atomically(
            temp.path(),
            "project-1",
            &run.id,
            &second_write.id,
            "write-state",
            &operation,
        )
        .expect("replay retried state write"));

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            loaded
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_state_written")
                .count(),
            2,
        );
        assert_eq!(
            loaded
                .artifacts
                .iter()
                .filter(|artifact| artifact.producer_node_run_id == second_write.id)
                .count(),
            1,
        );
        let connection = Connection::open(temp.path().join("state.db")).expect("open database");
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM delivery_projects WHERE project_id = 'project-1' AND id = 'delivery-project-retry'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("count delivery project"),
            1,
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM delivery_state_events WHERE project_id = 'project-1' AND workflow_run_id = ?1",
                    params![run.id],
                    |row| row.get::<_, i64>(0),
                )
                .expect("count durable state mutations"),
            1,
        );
    }

    #[test]
    fn prepared_collection_completion_is_exactly_once_under_concurrency() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "collect-state",
            "collection_loop",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "collect-state-attempt",
        )
        .expect("insert collection");
        let completion = WorkflowPreparedStateNodeCompletionRecord {
            run_id: run.id.clone(),
            node_run_id: node.id.clone(),
            artifact_type: "state_collection_result".into(),
            payload: json!({ "items": [{ "id": "delivery-project-1" }] }),
            render_text: Some("1 item".into()),
            event_type: "workflow_state_collection_completed".into(),
            event: json!({ "count": 1 }),
        };
        let barrier = Arc::new(Barrier::new(2));
        let handles = [(), ()].map(|()| {
            let barrier = Arc::clone(&barrier);
            let repo_root = temp.path().to_path_buf();
            let completion = completion.clone();
            std::thread::spawn(move || {
                barrier.wait();
                complete_prepared_workflow_state_node_atomically(
                    &repo_root,
                    "project-1",
                    &completion,
                )
            })
        });
        let results = handles.map(|handle| {
            handle
                .join()
                .expect("join collection")
                .expect("complete collection")
        });
        assert_eq!(results.iter().filter(|committed| **committed).count(), 1);

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            loaded
                .nodes
                .iter()
                .find(|candidate| candidate.id == node.id)
                .expect("collection exists")
                .status,
            WorkflowNodeRunStatusDto::Succeeded,
        );
        assert_eq!(
            loaded
                .artifacts
                .iter()
                .filter(|artifact| artifact.artifact_type == "state_collection_result")
                .count(),
            1,
        );
        assert_eq!(
            loaded
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_state_collection_completed")
                .count(),
            1,
        );
    }

    #[test]
    fn checkpoint_pause_rolls_back_together_then_replays_once() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "approval",
            "human_checkpoint",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "pause-checkpoint",
        )
        .expect("insert checkpoint");
        let connection = Connection::open(temp.path().join("state.db")).expect("open database");
        connection
            .execute_batch(
                r#"
                CREATE TRIGGER reject_checkpoint_pause_event
                BEFORE INSERT ON workflow_events
                WHEN NEW.event_type = 'workflow_paused'
                BEGIN
                    SELECT RAISE(ABORT, 'simulated pause event failure');
                END;
                "#,
            )
            .expect("install failure trigger");

        pause_workflow_checkpoint_atomically(
            temp.path(),
            "project-1",
            &run.id,
            &node.id,
            &node.node_id,
            "approval",
        )
        .expect_err("event failure aborts checkpoint pause");
        let rolled_back = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load rolled back run")
            .expect("run exists");
        assert_eq!(rolled_back.status, WorkflowRunStatusDto::Running);
        assert_eq!(
            rolled_back
                .nodes
                .iter()
                .find(|candidate| candidate.id == node.id)
                .expect("checkpoint exists")
                .status,
            WorkflowNodeRunStatusDto::Eligible,
        );

        connection
            .execute_batch("DROP TRIGGER reject_checkpoint_pause_event")
            .expect("remove failure trigger");
        assert!(pause_workflow_checkpoint_atomically(
            temp.path(),
            "project-1",
            &run.id,
            &node.id,
            &node.node_id,
            "approval",
        )
        .expect("pause checkpoint"));
        assert!(!pause_workflow_checkpoint_atomically(
            temp.path(),
            "project-1",
            &run.id,
            &node.id,
            &node.node_id,
            "approval",
        )
        .expect("replay checkpoint pause"));
        let paused = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load paused run")
            .expect("run exists");
        assert_eq!(paused.status, WorkflowRunStatusDto::Paused);
        assert_eq!(
            paused
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_paused")
                .count(),
            1,
        );
        assert_eq!(
            paused
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_metric_recorded")
                .count(),
            1,
        );
    }

    #[test]
    fn subgraph_start_and_terminal_completion_are_atomic_and_replayable() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let parent = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "subgraph-a",
            "subgraph",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "subgraph-parent",
        )
        .expect("insert subgraph parent");
        let start = WorkflowSubgraphStartRecord {
            run_id: run.id.clone(),
            parent_node_run_id: parent.id.clone(),
            parent_node_id: parent.node_id.clone(),
            subgraph_id: "nested".into(),
            input_artifact_type: "subgraph_input".into(),
            input_payload: json!({ "goal": "ship" }),
            output_artifact_type: "subgraph_output".into(),
            child_node_id: "subgraph-a::done".into(),
            child_node_type: "terminal".into(),
        };
        assert!(
            start_workflow_subgraph_atomically(temp.path(), "project-1", &start)
                .expect("start subgraph")
        );
        assert!(
            !start_workflow_subgraph_atomically(temp.path(), "project-1", &start)
                .expect("replay subgraph start")
        );
        let started = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load started run")
            .expect("run exists");
        let child = started
            .nodes
            .iter()
            .find(|node| node.node_id == "subgraph-a::done")
            .expect("child exists")
            .clone();
        assert_eq!(
            started
                .nodes
                .iter()
                .filter(|node| node.node_id == child.node_id)
                .count(),
            1,
        );

        let completion = WorkflowSubgraphCompletionRecord {
            run_id: run.id.clone(),
            terminal_node_run_id: child.id.clone(),
            terminal_node_id: child.node_id.clone(),
            parent_node_run_id: parent.id.clone(),
            parent_node_id: parent.node_id.clone(),
            parent_status: WorkflowNodeRunStatusDto::Succeeded,
            parent_failure_class: None,
            pause_run: false,
            artifact_type: "subgraph_output".into(),
            schema_version: 1,
            payload: json!({ "status": "success" }),
            render_text: Some("success".into()),
            edge_evidence: json!({ "terminalStatus": "success" }),
            terminal_event: json!({ "terminalStatus": "success" }),
            parent_event: json!({ "status": "succeeded" }),
        };
        let connection = Connection::open(temp.path().join("state.db")).expect("open database");
        connection
            .execute_batch(
                r#"
                CREATE TRIGGER reject_subgraph_parent_event
                BEFORE INSERT ON workflow_events
                WHEN NEW.event_type = 'workflow_subgraph_completed'
                BEGIN
                    SELECT RAISE(ABORT, 'simulated subgraph event failure');
                END;
                "#,
            )
            .expect("install failure trigger");
        complete_workflow_subgraph_atomically(temp.path(), "project-1", &completion)
            .expect_err("subgraph event failure aborts completion");
        let rolled_back = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load rolled back subgraph")
            .expect("run exists");
        assert_eq!(
            rolled_back
                .nodes
                .iter()
                .find(|node| node.id == parent.id)
                .expect("parent exists")
                .status,
            WorkflowNodeRunStatusDto::Running,
        );
        assert_eq!(
            rolled_back
                .nodes
                .iter()
                .find(|node| node.id == child.id)
                .expect("child exists")
                .status,
            WorkflowNodeRunStatusDto::Eligible,
        );
        assert!(!rolled_back
            .artifacts
            .iter()
            .any(|artifact| artifact.artifact_type == "subgraph_output"));
        assert!(rolled_back.edge_decisions.is_empty());

        connection
            .execute_batch("DROP TRIGGER reject_subgraph_parent_event")
            .expect("remove failure trigger");
        assert!(
            complete_workflow_subgraph_atomically(temp.path(), "project-1", &completion)
                .expect("complete subgraph")
        );
        assert!(
            !complete_workflow_subgraph_atomically(temp.path(), "project-1", &completion)
                .expect("replay subgraph completion")
        );
        let completed = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load completed subgraph")
            .expect("run exists");
        assert_eq!(
            completed
                .nodes
                .iter()
                .find(|node| node.id == parent.id)
                .expect("parent exists")
                .status,
            WorkflowNodeRunStatusDto::Succeeded,
        );
        assert_eq!(
            completed
                .artifacts
                .iter()
                .filter(|artifact| artifact.artifact_type == "subgraph_output")
                .count(),
            1,
        );
        assert_eq!(completed.edge_decisions.len(), 1);
        assert_eq!(
            completed
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_subgraph_completed")
                .count(),
            1,
        );
    }

    #[test]
    fn live_command_lease_blocks_retry_skip_and_routing() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");

        let retry_source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "retry-command",
            "command",
            0,
            WorkflowNodeRunStatusDto::Failed,
            "retry-command-attempt",
        )
        .expect("insert retry source");
        persist_test_command_lease(
            temp.path(),
            &run.id,
            &retry_source.id,
            "foreign-retry-owner",
            "foreign-retry-lease",
        );
        let retry_error = retry_workflow_node_atomically(
            temp.path(),
            "project-1",
            &WorkflowNodeRetryRecord {
                run_id: run.id.clone(),
                source_node_run_id: retry_source.id.clone(),
                node_id: retry_source.node_id.clone(),
                node_type: retry_source.node_type.clone(),
            },
        )
        .expect_err("live command cannot be retried");
        assert_eq!(retry_error.code, "workflow_node_execution_still_active");

        let skip_source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "skip-command",
            "command",
            0,
            WorkflowNodeRunStatusDto::Running,
            "skip-command-attempt",
        )
        .expect("insert skip source");
        persist_test_command_lease(
            temp.path(),
            &run.id,
            &skip_source.id,
            "foreign-skip-owner",
            "foreign-skip-lease",
        );
        let skip_error = skip_workflow_branch_atomically(
            temp.path(),
            "project-1",
            &WorkflowBranchSkipRecord {
                run_id: run.id.clone(),
                node_run_id: skip_source.id.clone(),
                node_id: skip_source.node_id.clone(),
                previous_status: skip_source.status,
                reason: None,
                merge_targets: vec![("merge-command".into(), "merge".into())],
            },
        )
        .expect_err("live command cannot be skipped");
        assert_eq!(skip_error.code, "workflow_node_run_not_skippable");

        let route_source = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "route-command",
            "command",
            0,
            WorkflowNodeRunStatusDto::Succeeded,
            "route-command-attempt",
        )
        .expect("insert route source");
        persist_test_command_lease(
            temp.path(),
            &run.id,
            &route_source.id,
            "foreign-route-owner",
            "foreign-route-lease",
        );
        assert!(!commit_workflow_route(
            temp.path(),
            "project-1",
            &run.id,
            &[WorkflowRouteDecisionRecord {
                source_node_run_id: route_source.id.clone(),
                source_status: WorkflowNodeRunStatusDto::Succeeded,
                from_node_id: route_source.node_id.clone(),
                to_node_id: "route-target".into(),
                edge_id: "route-edge".into(),
                condition: json!({ "kind": "always" }),
                evidence: json!({ "matched": true }),
                target_node_type: "terminal".into(),
                target_attempt_number: 0,
                target_idempotency_key: "route-target-attempt".into(),
            }],
        )
        .expect("live command rejects route"));

        let loaded = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load guarded run")
            .expect("run exists");
        assert_eq!(
            loaded
                .nodes
                .iter()
                .filter(|node| node.node_id == retry_source.node_id)
                .count(),
            1,
        );
        assert_eq!(
            loaded
                .nodes
                .iter()
                .find(|node| node.id == skip_source.id)
                .expect("skip source exists")
                .status,
            WorkflowNodeRunStatusDto::Running,
        );
        assert!(!loaded
            .nodes
            .iter()
            .any(|node| node.node_id == "route-target"));
    }

    #[test]
    fn durable_driver_lease_has_one_owner_and_failure_is_atomic() {
        let (temp, definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        let run =
            create_workflow_run(temp.path(), "project-1", &created.id, None).expect("create run");
        let node = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "agent-a",
            "agent",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "driver-failure-node",
        )
        .expect("insert active node");
        let barrier = Arc::new(Barrier::new(2));
        let handles = [("driver-a", "lease-a"), ("driver-b", "lease-b")].map(|(owner, token)| {
            let barrier = Arc::clone(&barrier);
            let repo_root = temp.path().to_path_buf();
            let run_id = run.id.clone();
            std::thread::spawn(move || {
                barrier.wait();
                claim_workflow_driver_lease(
                    &repo_root,
                    "project-1",
                    &run_id,
                    owner,
                    std::process::id(),
                    &crate::runtime::process_tree::process_birth_identity(std::process::id())
                        .expect("current process identity"),
                    token,
                    None,
                    "2026-07-15T12:00:00Z",
                )
                .map(|claimed| (owner.to_owned(), token.to_owned(), claimed))
            })
        });
        let results = handles.map(|handle| handle.join().expect("join claim").expect("claim"));
        assert_eq!(results.iter().filter(|(_, _, claimed)| *claimed).count(), 1);
        let (owner, token, _) = results
            .iter()
            .find(|(_, _, claimed)| *claimed)
            .expect("one owner wins");
        let (foreign_owner, foreign_token, _) = results
            .iter()
            .find(|(_, _, claimed)| !*claimed)
            .expect("one owner loses");
        assert!(!fail_workflow_run_from_driver_atomically(
            temp.path(),
            "project-1",
            &WorkflowDriverFailureRecord {
                run_id: run.id.clone(),
                incident_id: "foreign-incident".into(),
                failure_class: "workflow_driver_reconcile_failed".into(),
                event: json!({ "incidentId": "foreign-incident" }),
                owner_instance_id: foreign_owner.to_string(),
                lease_token: foreign_token.to_string(),
            },
        )
        .expect("foreign owner cannot fail run"));

        let connection = Connection::open(temp.path().join("state.db")).expect("open database");
        connection
            .execute_batch(
                r#"
                CREATE TRIGGER reject_driver_failure_event
                BEFORE INSERT ON workflow_events
                WHEN NEW.event_type = 'workflow_driver_failed'
                BEGIN
                    SELECT RAISE(ABORT, 'simulated driver failure event error');
                END;
                "#,
            )
            .expect("install failure trigger");
        let failure = WorkflowDriverFailureRecord {
            run_id: run.id.clone(),
            incident_id: "driver-incident".into(),
            failure_class: "workflow_driver_reconcile_failed".into(),
            event: json!({ "incidentId": "driver-incident" }),
            owner_instance_id: owner.to_string(),
            lease_token: token.to_string(),
        };
        let live_command = insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "live-command",
            "command",
            0,
            WorkflowNodeRunStatusDto::Running,
            "live-command-attempt",
        )
        .expect("insert live command");
        persist_test_command_lease(
            temp.path(),
            &run.id,
            &live_command.id,
            "live-command-owner",
            "live-command-lease",
        );
        let active_error =
            fail_workflow_run_from_driver_atomically(temp.path(), "project-1", &failure)
                .expect_err("driver cannot fail while an execution lease remains");
        assert_eq!(active_error.code, "workflow_driver_execution_still_active");
        assert!(release_workflow_command_lease(
            temp.path(),
            "project-1",
            &live_command.id,
            "live-command-owner",
            "live-command-lease",
        )
        .expect("release command lease"));
        fail_workflow_run_from_driver_atomically(temp.path(), "project-1", &failure)
            .expect_err("event failure rolls back run and nodes");
        let rolled_back = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load rolled back run")
            .expect("run exists");
        assert_eq!(rolled_back.status, WorkflowRunStatusDto::Queued);
        assert_eq!(
            rolled_back
                .nodes
                .iter()
                .find(|candidate| candidate.id == node.id)
                .expect("node exists")
                .status,
            WorkflowNodeRunStatusDto::Eligible,
        );

        connection
            .execute_batch("DROP TRIGGER reject_driver_failure_event")
            .expect("remove failure trigger");
        assert!(
            fail_workflow_run_from_driver_atomically(temp.path(), "project-1", &failure,)
                .expect("persist driver failure")
        );
        assert!(
            !fail_workflow_run_from_driver_atomically(temp.path(), "project-1", &failure,)
                .expect("replay driver failure")
        );
        let failed = get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load failed run")
            .expect("run exists");
        assert_eq!(failed.status, WorkflowRunStatusDto::Failed);
        assert_eq!(
            failed
                .nodes
                .iter()
                .find(|candidate| candidate.id == node.id)
                .expect("node exists")
                .status,
            WorkflowNodeRunStatusDto::Stalled,
        );
        assert_eq!(
            failed
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_driver_failed")
                .count(),
            1,
        );
    }
}
