use std::path::Path;

use rusqlite::params;

use crate::{
    commands::{CommandError, OperatorApprovalDto, OperatorApprovalStatus, ResumeHistoryEntryDto},
    db::database_path_for_repo,
};

use super::{
    apply_workflow_transition_mutation, attempt_automatic_dispatch_after_transition,
    decode_operator_resume_transition_context, derive_operator_scope_prefix,
    derive_resume_transition_id, enqueue_notification_dispatches_best_effort_with_connection,
    find_prohibited_transition_diagnostic_content, map_operator_loop_commit_error,
    map_operator_loop_transaction_error, map_operator_loop_write_error,
    map_runtime_run_write_error, normalize_runtime_checkpoint_summary, open_project_database,
    operator_approval_status_label, read_latest_transition_id, read_operator_approval_by_action_id,
    read_project_row, read_resume_history_entry_by_id, read_runtime_run_row,
    read_runtime_run_snapshot, read_transition_event_by_transition_id,
    runtime_run_checkpoint_kind_sql_value, validate_non_empty_text,
    validate_runtime_action_required_payload, NotificationDispatchEnqueueRecord,
    ResolveOperatorAnswerRequirement, RuntimeActionRequiredPersistedRecord,
    RuntimeActionRequiredUpsertRecord, RuntimeOperatorResumeTarget, RuntimeRunCheckpointKind,
    WorkflowAutomaticDispatchOutcome, WorkflowGateState, WorkflowTransitionGateDecision,
    WorkflowTransitionGateMutationRecord, WorkflowTransitionMutationApplyOutcome,
    WorkflowTransitionMutationRecord, OPERATOR_RESUME_MUTATION_ERROR_PROFILE,
};

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
    let replayed_existing_pending = match existing {
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
            false
        }
        Some(approval) => match approval.status {
            OperatorApprovalStatus::Pending => true,
            OperatorApprovalStatus::Approved | OperatorApprovalStatus::Rejected => {
                return Err(CommandError::retryable(
                    "runtime_action_sync_conflict",
                    format!(
                        "Cadence received a retained runtime action for already-resolved operator request `{action_id}`. Refresh selected project state before retrying."
                    ),
                ));
            }
        },
    };

    let next_sequence = runtime_row.last_checkpoint_sequence.saturating_add(1);
    let (last_error_code, last_error_message) = payload
        .last_error
        .as_ref()
        .map(|error| (Some(error.code.as_str()), Some(error.message.as_str())))
        .unwrap_or((None, None));

    if !replayed_existing_pending {
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
                    runtime_run_checkpoint_kind_sql_value(
                        &RuntimeRunCheckpointKind::ActionRequired
                    ),
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
    }

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

pub(crate) fn classify_operator_answer_requirement(
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

pub(crate) fn decode_runtime_operator_resume_target(
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

pub fn derive_runtime_action_id(
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
