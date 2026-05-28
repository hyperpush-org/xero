use std::path::Path;

use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value as JsonValue};

use crate::{
    auth::now_timestamp,
    commands::{
        contracts::workflows::{
            WorkflowArtifactRecordDto, WorkflowDefinitionDto, WorkflowDefinitionSummaryDto,
            WorkflowEventDto, WorkflowGateDecisionDto, WorkflowHumanCheckpointTypeDto,
            WorkflowLoopAttemptDto, WorkflowNodeRunStatusDto, WorkflowRunDto,
            WorkflowRunEdgeDecisionDto, WorkflowRunNodeDto, WorkflowRunStatusDto,
            WorkflowTerminalStatusDto,
        },
        CommandError,
    },
    db::database_path_for_repo,
};

use super::{open_runtime_database, read_project_row, validate_non_empty_text};

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
    let connection = open_runtime_database(repo_root, &database_path)?;
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

    let tx = connection.unchecked_transaction().map_err(|error| {
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

    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(
        &connection,
        &database_path,
        repo_root,
        &definition.project_id,
    )?;
    let tx = connection.unchecked_transaction().map_err(|error| {
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
    let version_number = current.active_version_number.saturating_add(1);
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
    tx.execute(
        r#"
        UPDATE workflow_definitions
        SET name = ?3,
            description = ?4,
            active_version_id = ?5,
            updated_at = ?6
        WHERE project_id = ?1
          AND id = ?2
        "#,
        params![
            definition.project_id.as_str(),
            workflow_id,
            stored_definition.name.as_str(),
            stored_definition.description.as_str(),
            version_id.as_str(),
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_definition_update_failed", &database_path, error)
    })?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_definition_commit_failed", &database_path, error)
    })?;

    Ok(stored_definition)
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
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let version =
        read_active_definition_version_row(&connection, &database_path, project_id, workflow_id)?
            .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_definition_not_found",
                format!("Xero could not find Workflow `{workflow_id}`."),
            )
        })?;
    let definition = decode_definition(&database_path, &version.definition_json)?;
    let run_id = generate_workflow_id("workflow-run");
    let now = now_timestamp();
    let initial_input_json = initial_input
        .as_ref()
        .map(|value| serialize_json(value, "workflow_run_input_encode_failed"))
        .transpose()?;
    let tx = connection.unchecked_transaction().map_err(|error| {
        map_workflow_write_error("workflow_run_transaction_failed", &database_path, error)
    })?;
    tx.execute(
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
            initial_input_json.as_deref(),
            now.as_str(),
            now.as_str(),
        ],
    )
    .map_err(|error| {
        map_workflow_write_error("workflow_run_insert_failed", &database_path, error)
    })?;
    insert_workflow_event_with_connection(
        &tx,
        project_id,
        &run_id,
        None,
        "workflow_started",
        &json!({
            "workflowId": workflow_id,
            "workflowVersionId": version.id,
            "workflowVersionNumber": version.version_number
        }),
        &now,
    )?;
    tx.commit().map_err(|error| {
        map_workflow_write_error("workflow_run_commit_failed", &database_path, error)
    })?;
    read_workflow_run(&connection, &database_path, project_id, &run_id)?
        .ok_or_else(|| {
            CommandError::system_fault(
                "workflow_run_missing_after_insert",
                "Xero created a Workflow run but could not read it back.",
            )
        })
        .map(|mut run| {
            run.definition_snapshot = definition;
            run
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
                    WHEN ?3 IN ('queued', 'running', 'paused') THEN NULL
                    ELSE completed_at
                END,
                updated_at = ?7
            WHERE project_id = ?1
              AND id = ?2
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
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
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
    let connection = open_runtime_database(repo_root, &database_path)?;
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
    connection
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
    Ok(())
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
                attempt_count = workflow_loop_attempts.attempt_count + 1,
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

pub fn insert_workflow_gate_decision(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    checkpoint_type: WorkflowHumanCheckpointTypeDto,
    decision: &str,
    payload: Option<&JsonValue>,
) -> Result<(), CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let now = now_timestamp();
    let payload_json = payload
        .map(|payload| serialize_json(payload, "workflow_gate_decision_payload_encode_failed"))
        .transpose()?;
    connection
        .execute(
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
                generate_workflow_id("workflow-gate-decision"),
                project_id,
                run_id,
                node_run_id,
                checkpoint_type.as_str(),
                decision,
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
    Ok(())
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
    use super::*;
    use crate::{
        commands::contracts::{
            runtime::RuntimeAgentIdDto,
            workflow_agents::AgentRefDto,
            workflows::{
                WorkflowEdgeDto, WorkflowEdgeTypeDto, WorkflowNodeDto, WorkflowOutputContractDto,
                WorkflowRunPolicyDto, WorkflowTerminalStatusDto,
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

    #[test]
    fn definitions_round_trip_with_immutable_versions() {
        let (temp, mut definition) = repo_with_database();
        let created =
            create_workflow_definition(temp.path(), &definition).expect("create workflow");
        definition.name = "Workflow v2".into();
        let updated = update_workflow_definition(temp.path(), &created.id, &definition)
            .expect("update workflow");

        let summaries =
            list_workflow_definitions(temp.path(), "project-1").expect("list workflows");

        assert_eq!(updated.version, 2);
        assert_eq!(summaries[0].active_version_number, 2);
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
}
