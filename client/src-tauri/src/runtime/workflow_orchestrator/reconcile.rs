use std::{
    collections::{BTreeMap, BTreeSet},
    process::{Command, Stdio},
    time::{Duration as StdDuration, Instant},
};

use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Runtime};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::{
    commands::{
        agent_task::start_agent_task_blocking,
        contracts::{
            runtime::{RuntimeRunApprovalModeDto, RuntimeRunControlInputDto},
            workflow_agents::AgentRefDto,
            workflows::{
                WorkflowCollectionLoopControlsDto, WorkflowConditionDto, WorkflowDefinitionDto,
                WorkflowEdgeDto, WorkflowEdgeTypeDto, WorkflowHumanCheckpointTypeDto,
                WorkflowInputBindingDto, WorkflowMergeWaitPolicyDto, WorkflowNodeDto,
                WorkflowNodeRunStatusDto, WorkflowOutputContractDto, WorkflowOutputExtractionDto,
                WorkflowResourceConflictModeDto, WorkflowRunDto, WorkflowRunNodeDto,
                WorkflowRunOverrideDto, WorkflowRunStatusDto, WorkflowStallDetectorDto,
                WorkflowStateQueryDto, WorkflowStateWriteOperationDto, WorkflowSubgraphDto,
                WorkflowTerminalStatusDto,
            },
        },
        default_runtime_agent_approval_mode, CommandError, CommandResult, StartAgentTaskRequestDto,
    },
    db::project_store::{
        self, AgentRunRecord, AgentRunSnapshotRecord, AgentRunStatus, AgentSessionCreateRecord,
        DeliveryStateWriteContext,
    },
    runtime::DesktopAgentCoreRuntime,
    state::DesktopState,
};

use super::{
    artifacts::{
        build_agent_node_prompt, extract_workflow_artifact_payload, final_assistant_text,
        validate_workflow_artifact_payload,
    },
    condition_eval::{evaluate_workflow_condition, json_path_lookup, WorkflowConditionContext},
};

const MAX_RECONCILE_STEPS: usize = 32;
const RUNTIME_ACTIVITY_TIMEOUT_FAILURE_CLASS: &str = "runtime_activity_timeout";
const USER_SKIPPED_FAILURE_CLASS: &str = "skipped_by_user";
const SUBGRAPH_NODE_SEPARATOR: &str = "::";
const SUBGRAPH_INPUT_ARTIFACT_TYPE: &str = "subgraph_input";

pub fn reconcile_workflow_run<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
) -> CommandResult<WorkflowRunDto> {
    let repo_root = crate::commands::runtime_support::resolve_project_root(app, state, project_id)?;
    for _ in 0..MAX_RECONCILE_STEPS {
        let run =
            project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
                CommandError::user_fixable(
                    "workflow_run_not_found",
                    format!("Xero could not find Workflow run `{run_id}`."),
                )
            })?;
        if is_terminal_run(run.status) || run.status == WorkflowRunStatusDto::Paused {
            return Ok(run);
        }

        if run.status == WorkflowRunStatusDto::Queued {
            project_store::update_workflow_run_status(
                &repo_root,
                project_id,
                run_id,
                WorkflowRunStatusDto::Running,
                None,
                None,
            )?;
            ensure_node_run(&repo_root, &run, &run.definition_snapshot.start_node_id, 0)?;
            continue;
        }

        if reconcile_running_agent_nodes(&repo_root, project_id, &run)? {
            continue;
        }

        if route_completed_nodes(&repo_root, project_id, &run)? {
            continue;
        }

        if start_eligible_nodes(app, state, &repo_root, project_id, &run)? {
            continue;
        }

        return project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
            CommandError::system_fault(
                "workflow_run_missing_after_reconcile",
                format!("Workflow run `{run_id}` disappeared during reconcile."),
            )
        });
    }

    project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "workflow_run_missing_after_reconcile",
            format!("Workflow run `{run_id}` disappeared during reconcile."),
        )
    })
}

pub fn resume_workflow_checkpoint<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    decision: &str,
    payload: Option<JsonValue>,
) -> CommandResult<WorkflowRunDto> {
    let repo_root = crate::commands::runtime_support::resolve_project_root(app, state, project_id)?;
    let run =
        project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_run_not_found",
                format!("Xero could not find Workflow run `{run_id}`."),
            )
        })?;
    let node_run = run
        .nodes
        .iter()
        .find(|node| node.id == node_run_id)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_checkpoint_not_found",
                format!("Xero could not find Workflow checkpoint node run `{node_run_id}`."),
            )
        })?;
    let checkpoint_node = find_node(&run.definition_snapshot, &node_run.node_id);
    let checkpoint_type = checkpoint_type_for_node(&run.definition_snapshot, &node_run.node_id)
        .unwrap_or(WorkflowHumanCheckpointTypeDto::Decision);
    if let Some(WorkflowNodeDto::HumanCheckpoint {
        decision_options,
        resume_payload_schema,
        ..
    }) = checkpoint_node
    {
        if !decision_options.is_empty() && !decision_options.iter().any(|option| option == decision)
        {
            return Err(CommandError::user_fixable(
                "workflow_checkpoint_decision_invalid",
                format!(
                    "Decision `{decision}` is not allowed for Workflow checkpoint `{}`.",
                    node_run.node_id
                ),
            ));
        }
        if let Some(schema) = resume_payload_schema.as_ref() {
            let payload_value = payload.as_ref().unwrap_or(&JsonValue::Null);
            validate_workflow_artifact_payload(
                &WorkflowOutputContractDto {
                    artifact_type: "human_decision".into(),
                    schema_version: 1,
                    extraction: WorkflowOutputExtractionDto::JsonObject,
                    required: true,
                    render_text_path: None,
                },
                Some(schema),
                payload_value,
            )
            .map_err(|error| {
                CommandError::user_fixable(
                    "workflow_checkpoint_payload_invalid",
                    format!(
                        "Xero rejected the resume payload for checkpoint `{}`: {}",
                        node_run.node_id, error.message
                    ),
                )
            })?;
        }
    }
    project_store::insert_workflow_gate_decision(
        &repo_root,
        project_id,
        run_id,
        node_run_id,
        checkpoint_type,
        decision,
        payload.as_ref(),
    )?;
    project_store::insert_workflow_artifact(
        &repo_root,
        project_id,
        run_id,
        node_run_id,
        "human_decision",
        1,
        &json!({ "decision": decision, "payload": payload.clone() }),
        Some(decision),
    )?;
    if let Some(WorkflowNodeDto::HumanCheckpoint { state_updates, .. }) = checkpoint_node {
        let decision_context = json!({ "decision": decision, "payload": payload.clone() });
        for operation in state_updates {
            let operation = runtime_state_write_operation_for_node(
                &run.definition_snapshot,
                &node_run.node_id,
                operation,
            );
            let resolved_operation =
                resolve_state_write_operation(&operation, &run, Some(&decision_context))?;
            let result = project_store::write_delivery_state(
                &repo_root,
                project_id,
                DeliveryStateWriteContext {
                    workflow_run_id: Some(&run.id),
                    node_run_id: Some(&node_run.id),
                },
                &resolved_operation,
            )?;
            project_store::insert_workflow_event(
                &repo_root,
                project_id,
                run_id,
                Some(&node_run.id),
                "workflow_checkpoint_state_written",
                &json!({
                    "nodeId": node_run.node_id,
                    "entityType": resolved_operation.entity_type.as_str(),
                    "action": resolved_operation.action.as_str(),
                    "entityId": result.get("id"),
                    "decision": decision,
                }),
            )?;
        }
    }
    project_store::update_workflow_run_node(
        &repo_root,
        project_id,
        node_run_id,
        WorkflowNodeRunStatusDto::Succeeded,
        None,
        None,
        None,
    )?;
    project_store::update_workflow_run_status(
        &repo_root,
        project_id,
        run_id,
        WorkflowRunStatusDto::Running,
        None,
        None,
    )?;
    reconcile_workflow_run(app, state, project_id, run_id)
}

pub fn retry_workflow_node_run<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
) -> CommandResult<WorkflowRunDto> {
    let repo_root = crate::commands::runtime_support::resolve_project_root(app, state, project_id)?;
    let run =
        project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_run_not_found",
                format!("Xero could not find Workflow run `{run_id}`."),
            )
        })?;
    if matches!(
        run.status,
        WorkflowRunStatusDto::Completed | WorkflowRunStatusDto::Cancelled
    ) {
        return Err(CommandError::user_fixable(
            "workflow_run_not_retryable",
            "Completed or cancelled Workflow runs cannot be retried from a node.",
        ));
    }
    let node_run = run
        .nodes
        .iter()
        .find(|node| node.id == node_run_id)
        .cloned()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_node_run_not_found",
                format!("Xero could not find Workflow node run `{node_run_id}`."),
            )
        })?;
    if !is_retryable_node_status(node_run.status) {
        return Err(CommandError::user_fixable(
            "workflow_node_run_not_retryable",
            format!(
                "Workflow node run `{node_run_id}` cannot be retried while it is `{}`.",
                node_run.status.as_str()
            ),
        ));
    }
    if find_node(&run.definition_snapshot, &node_run.node_id).is_none() {
        return Err(CommandError::system_fault(
            "workflow_retry_node_missing",
            format!(
                "Workflow node `{}` was missing from run `{run_id}`.",
                node_run.node_id
            ),
        ));
    }
    if has_control_event_after_completion(&run, &node_run, "workflow_node_retry_requested") {
        return reconcile_workflow_run(app, state, project_id, run_id);
    }

    let attempt = next_attempt_for_node(&run, &node_run.node_id);
    let retry_node = ensure_node_run(&repo_root, &run, &node_run.node_id, attempt)?;
    project_store::insert_workflow_event(
        &repo_root,
        project_id,
        run_id,
        Some(&node_run.id),
        "workflow_node_retry_requested",
        &json!({
            "nodeId": node_run.node_id,
            "previousStatus": node_run.status.as_str(),
            "retryNodeRunId": retry_node.id,
            "attemptNumber": retry_node.attempt_number,
        }),
    )?;
    project_store::update_workflow_run_status(
        &repo_root,
        project_id,
        run_id,
        WorkflowRunStatusDto::Running,
        None,
        None,
    )?;
    reconcile_workflow_run(app, state, project_id, run_id)
}

pub fn skip_workflow_branch<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    reason: Option<&str>,
) -> CommandResult<WorkflowRunDto> {
    let repo_root = crate::commands::runtime_support::resolve_project_root(app, state, project_id)?;
    let run =
        project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_run_not_found",
                format!("Xero could not find Workflow run `{run_id}`."),
            )
        })?;
    if is_terminal_run(run.status) {
        return Err(CommandError::user_fixable(
            "workflow_run_not_skippable",
            "Completed, failed, or cancelled Workflow runs cannot skip branches.",
        ));
    }
    let node_run = run
        .nodes
        .iter()
        .find(|node| node.id == node_run_id)
        .cloned()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_node_run_not_found",
                format!("Xero could not find Workflow node run `{node_run_id}`."),
            )
        })?;
    if !is_skippable_node_status(node_run.status) {
        return Err(CommandError::user_fixable(
            "workflow_node_run_not_skippable",
            format!(
                "Workflow node run `{node_run_id}` cannot be skipped while it is `{}`.",
                node_run.status.as_str()
            ),
        ));
    }

    if let Some(runtime_run_id) = node_run.runtime_run_id.as_ref() {
        let runtime = DesktopAgentCoreRuntime::new(state.agent_run_supervisor().clone());
        let _ = runtime.cancel_run(
            repo_root.clone(),
            project_id.to_owned(),
            runtime_run_id.to_owned(),
        );
    }
    project_store::update_workflow_run_node(
        &repo_root,
        project_id,
        &node_run.id,
        WorkflowNodeRunStatusDto::Skipped,
        None,
        None,
        Some(USER_SKIPPED_FAILURE_CLASS),
    )?;
    let merge_target_node_ids = ensure_direct_merge_targets_for_skipped_branch(
        &repo_root,
        project_id,
        &run,
        &node_run.node_id,
    )?;
    project_store::insert_workflow_event(
        &repo_root,
        project_id,
        run_id,
        Some(&node_run.id),
        "workflow_branch_skipped",
        &json!({
            "nodeId": node_run.node_id,
            "previousStatus": node_run.status.as_str(),
            "reason": reason,
            "mergeTargetNodeIds": merge_target_node_ids,
        }),
    )?;
    project_store::update_workflow_run_status(
        &repo_root,
        project_id,
        run_id,
        WorkflowRunStatusDto::Running,
        None,
        None,
    )?;
    reconcile_workflow_run(app, state, project_id, run_id)
}

fn reconcile_running_agent_nodes(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
) -> CommandResult<bool> {
    let mut changed = false;
    let now = OffsetDateTime::now_utc();
    for node_run in run
        .nodes
        .iter()
        .filter(|node| node.status == WorkflowNodeRunStatusDto::Running)
    {
        let Some(runtime_run_id) = node_run.runtime_run_id.as_deref() else {
            continue;
        };
        let snapshot = project_store::load_agent_run(repo_root, project_id, runtime_run_id)?;
        match snapshot.run.status {
            AgentRunStatus::Starting | AgentRunStatus::Running => {
                if let Some(timeout_seconds) =
                    activity_timeout_seconds_for_node(&run.definition_snapshot, &node_run.node_id)
                {
                    if let Some(last_activity_at) =
                        stale_agent_activity_at(&snapshot.run, timeout_seconds, now)
                    {
                        project_store::update_workflow_run_node(
                            repo_root,
                            project_id,
                            &node_run.id,
                            WorkflowNodeRunStatusDto::Stalled,
                            None,
                            None,
                            Some(RUNTIME_ACTIVITY_TIMEOUT_FAILURE_CLASS),
                        )?;
                        project_store::insert_workflow_event(
                            repo_root,
                            project_id,
                            &run.id,
                            Some(&node_run.id),
                            "workflow_node_stalled",
                            &json!({
                                "nodeId": node_run.node_id,
                                "runtimeRunId": runtime_run_id,
                                "failureClass": RUNTIME_ACTIVITY_TIMEOUT_FAILURE_CLASS,
                                "timeoutSeconds": timeout_seconds,
                                "lastActivityAt": last_activity_at,
                            }),
                        )?;
                        changed = true;
                    }
                }
            }
            AgentRunStatus::Completed => {
                if let Some(contract) =
                    output_contract_for_node(&run.definition_snapshot, &node_run.node_id)
                {
                    let final_text = final_assistant_text(&snapshot).unwrap_or_default();
                    let json_schema =
                        artifact_schema_for_output(&run.definition_snapshot, contract);
                    let (payload, render_text, diagnostics) =
                        match extract_workflow_artifact_payload(contract, json_schema, &final_text)
                        {
                            Ok(artifact) => artifact,
                            Err(error)
                                if error.code == "workflow_artifact_extraction_failed"
                                    || error.code == "workflow_artifact_schema_invalid" =>
                            {
                                fail_node_with_recoverable_error(
                                    repo_root,
                                    project_id,
                                    run,
                                    node_run,
                                    &error.code,
                                    &error.code,
                                    &error.message,
                                )?;
                                changed = true;
                                continue;
                            }
                            Err(error) => return Err(error),
                        };
                    project_store::insert_workflow_artifact(
                        repo_root,
                        project_id,
                        &run.id,
                        &node_run.id,
                        &contract.artifact_type,
                        contract.schema_version,
                        &payload,
                        render_text.as_deref(),
                    )?;
                    project_store::insert_workflow_event(
                        repo_root,
                        project_id,
                        &run.id,
                        Some(&node_run.id),
                        "workflow_artifact_extracted",
                        &json!({
                            "nodeId": node_run.node_id,
                            "artifactType": contract.artifact_type,
                            "schemaVersion": contract.schema_version,
                            "validationStatus": "valid",
                            "diagnostics": diagnostics.iter().map(|diagnostic| {
                                json!({
                                    "code": diagnostic.code,
                                    "path": diagnostic.path,
                                    "message": diagnostic.message,
                                })
                            }).collect::<Vec<_>>(),
                            "renderTextPath": contract.render_text_path,
                        }),
                    )?;
                }
                project_store::update_workflow_run_node(
                    repo_root,
                    project_id,
                    &node_run.id,
                    WorkflowNodeRunStatusDto::Succeeded,
                    None,
                    None,
                    None,
                )?;
                changed = true;
            }
            AgentRunStatus::Failed => {
                let failure_class = snapshot
                    .run
                    .last_error
                    .as_ref()
                    .map(|error| error.code.as_str())
                    .unwrap_or("agent_failed");
                project_store::update_workflow_run_node(
                    repo_root,
                    project_id,
                    &node_run.id,
                    WorkflowNodeRunStatusDto::Failed,
                    None,
                    None,
                    Some(failure_class),
                )?;
                changed = true;
            }
            AgentRunStatus::Cancelled => {
                project_store::update_workflow_run_node(
                    repo_root,
                    project_id,
                    &node_run.id,
                    WorkflowNodeRunStatusDto::Cancelled,
                    None,
                    None,
                    Some("cancelled"),
                )?;
                changed = true;
            }
            _ => {}
        }
    }
    Ok(changed)
}

fn activity_timeout_seconds_for_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
) -> Option<u32> {
    match find_node(definition, node_id) {
        Some(WorkflowNodeDto::Agent { failure_policy, .. }) => failure_policy
            .runtime_activity_timeout_seconds
            .or(definition.run_policy.node_timeout_seconds),
        _ => None,
    }
}

fn stale_agent_activity_at(
    agent_run: &AgentRunRecord,
    timeout_seconds: u32,
    now: OffsetDateTime,
) -> Option<&str> {
    let timeout = Duration::seconds(timeout_seconds.into());
    let latest_activity = [
        agent_run.last_heartbeat_at.as_deref(),
        Some(agent_run.updated_at.as_str()),
        Some(agent_run.started_at.as_str()),
    ]
    .into_iter()
    .flatten()
    .filter_map(|timestamp| {
        OffsetDateTime::parse(timestamp, &Rfc3339)
            .ok()
            .map(|parsed| (timestamp, parsed))
    })
    .max_by_key(|(_, timestamp)| timestamp.unix_timestamp_nanos())?;

    (now - latest_activity.1 >= timeout).then_some(latest_activity.0)
}

fn route_completed_nodes(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
) -> CommandResult<bool> {
    for node_run in run.nodes.iter().filter(|node| {
        matches!(
            node.status,
            WorkflowNodeRunStatusDto::Succeeded
                | WorkflowNodeRunStatusDto::Failed
                | WorkflowNodeRunStatusDto::Stalled
                | WorkflowNodeRunStatusDto::Cancelled
        ) && !has_routed_node_run(run, node)
    }) {
        let Some(node) = find_node(&run.definition_snapshot, &node_run.node_id) else {
            continue;
        };
        if matches!(node, WorkflowNodeDto::Terminal { .. })
            && subgraph_context_for_node_id(&run.definition_snapshot, &node_run.node_id).is_some()
        {
            continue;
        }
        if let WorkflowNodeDto::Terminal {
            terminal_status, ..
        } = node
        {
            complete_for_terminal(repo_root, project_id, run, *terminal_status)?;
            return Ok(true);
        }

        let context = condition_context(run);
        let mut outgoing = runtime_edges_from_node(&run.definition_snapshot, &node_run.node_id)
            .into_iter()
            .filter(|edge| {
                edge.from_node_id == node_run.node_id
                    && edge_applies_to_node_status(edge.r#type, node_run.status)
            })
            .collect::<Vec<_>>();
        outgoing.sort_by_key(|edge| edge.priority);

        let mut matched_edges = Vec::new();
        let mut default_edge = None;
        for edge in outgoing {
            let evaluation = evaluate_workflow_condition(&edge.condition, &context);
            let condition_json = encode_workflow_condition(&edge.condition)?;
            project_store::insert_workflow_event(
                repo_root,
                project_id,
                &run.id,
                Some(&node_run.id),
                "workflow_edge_evaluated",
                &json!({
                    "edgeId": edge.id,
                    "fromNodeId": edge.from_node_id,
                    "toNodeId": edge.to_node_id,
                    "matched": evaluation.matched,
                    "condition": condition_json,
                    "evidence": evaluation.evidence.clone(),
                }),
            )?;
            if !evaluation.matched {
                continue;
            }
            if matches!(
                edge.condition,
                crate::commands::contracts::workflows::WorkflowConditionDto::Always
            ) {
                default_edge = Some((edge, condition_json, evaluation.evidence));
                continue;
            }
            matched_edges.push((edge, condition_json, evaluation.evidence));
            if routes_single_match(node) {
                break;
            }
        }
        if matched_edges.is_empty() {
            if let Some((edge, condition_json, evidence)) = default_edge {
                matched_edges.push((edge, condition_json, evidence));
            }
        }

        if !matched_edges.is_empty() {
            let mut created = false;
            if node_run.status == WorkflowNodeRunStatusDto::Succeeded
                && had_prior_unsuccessful_attempt(run, node_run)
                && !has_metric_event_for_node(run, node_run, "recovery_success")
            {
                insert_workflow_metric_event(
                    repo_root,
                    project_id,
                    &run.id,
                    Some(&node_run.id),
                    "recovery_success",
                    &json!({
                        "nodeId": node_run.node_id,
                        "attemptNumber": node_run.attempt_number,
                    }),
                )?;
            }
            for (edge, condition_json, evidence) in matched_edges {
                let target_node_id =
                    loop_target_for_edge(repo_root, project_id, run, node_run, &edge)?;
                project_store::insert_workflow_edge_decision(
                    repo_root,
                    project_id,
                    &run.id,
                    &edge.from_node_id,
                    &target_node_id,
                    &edge.id,
                    &condition_json,
                    &evidence,
                )?;
                let attempt = next_attempt_for_node(run, &target_node_id);
                ensure_node_run(repo_root, run, &target_node_id, attempt)?;
                created = true;
            }
            return Ok(created);
        }

        project_store::insert_workflow_event(
            repo_root,
            project_id,
            &run.id,
            Some(&node_run.id),
            "workflow_route_missing",
            &json!({ "nodeId": node_run.node_id }),
        )?;
        project_store::update_workflow_run_status(
            repo_root,
            project_id,
            &run.id,
            WorkflowRunStatusDto::Paused,
            Some(WorkflowTerminalStatusDto::NeedsHuman),
            None,
        )?;
        return Ok(true);
    }
    Ok(false)
}

fn start_eligible_nodes<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
) -> CommandResult<bool> {
    let concurrency_limit = run.definition_snapshot.run_policy.concurrency_limit.max(1) as usize;
    let running_agent_count = run
        .nodes
        .iter()
        .filter(|node| {
            node.status == WorkflowNodeRunStatusDto::Running
                && matches!(
                    find_node(&run.definition_snapshot, &node.node_id),
                    Some(WorkflowNodeDto::Agent { .. })
                )
        })
        .count();
    for node_run in run
        .nodes
        .iter()
        .filter(|node| node.status == WorkflowNodeRunStatusDto::Eligible)
    {
        let Some(node) = find_node(&run.definition_snapshot, &node_run.node_id) else {
            continue;
        };
        match node {
            WorkflowNodeDto::Agent {
                title,
                agent_ref,
                input_bindings,
                run_overrides,
                ..
            } => {
                if running_agent_count >= concurrency_limit {
                    continue;
                }
                if let Some(conflict) = resource_conflict_for_node(run, node_run, node) {
                    if !has_node_event(run, node_run, "workflow_resource_conflict_wait") {
                        project_store::insert_workflow_event(
                            repo_root,
                            project_id,
                            &run.id,
                            Some(&node_run.id),
                            "workflow_resource_conflict_wait",
                            &json!({
                                "nodeId": node_run.node_id,
                                "blockedByNodeRunId": conflict.node_run_id,
                                "blockedByNodeId": conflict.node_id,
                                "scopes": conflict.scopes,
                            }),
                        )?;
                        return Ok(true);
                    }
                    continue;
                }
                let input_bindings = runtime_input_bindings_for_node(
                    &run.definition_snapshot,
                    &node_run.node_id,
                    input_bindings,
                );
                start_agent_node(
                    app,
                    state,
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    title,
                    agent_ref,
                    &input_bindings,
                    run_overrides.as_ref(),
                )?;
                return Ok(true);
            }
            WorkflowNodeDto::Router { .. } => {
                project_store::update_workflow_run_node(
                    repo_root,
                    project_id,
                    &node_run.id,
                    WorkflowNodeRunStatusDto::Succeeded,
                    None,
                    None,
                    None,
                )?;
                return Ok(true);
            }
            WorkflowNodeDto::Merge {
                wait_policy,
                quorum,
                fail_fast,
                ..
            } => match evaluate_merge_node(run, node_run, *wait_policy, *quorum, *fail_fast) {
                MergeEvaluation::Waiting => {}
                MergeEvaluation::Succeeded => {
                    project_store::update_workflow_run_node(
                        repo_root,
                        project_id,
                        &node_run.id,
                        WorkflowNodeRunStatusDto::Succeeded,
                        None,
                        None,
                        None,
                    )?;
                    return Ok(true);
                }
                MergeEvaluation::Failed(failure_class) => {
                    project_store::update_workflow_run_node(
                        repo_root,
                        project_id,
                        &node_run.id,
                        WorkflowNodeRunStatusDto::Failed,
                        None,
                        None,
                        Some(failure_class),
                    )?;
                    return Ok(true);
                }
            },
            WorkflowNodeDto::Gate {
                required_checks,
                on_blocked,
                ..
            }
            | WorkflowNodeDto::StateCheckpoint {
                required_checks,
                on_blocked,
                ..
            } => {
                let context = condition_context(run);
                let passed = required_checks
                    .iter()
                    .all(|condition| evaluate_workflow_condition(condition, &context).matched);
                if passed {
                    project_store::update_workflow_run_node(
                        repo_root,
                        project_id,
                        &node_run.id,
                        WorkflowNodeRunStatusDto::Succeeded,
                        None,
                        None,
                        None,
                    )?;
                } else if on_blocked == "fail" {
                    project_store::update_workflow_run_node(
                        repo_root,
                        project_id,
                        &node_run.id,
                        WorkflowNodeRunStatusDto::Failed,
                        None,
                        None,
                        Some("gate_failed"),
                    )?;
                } else {
                    pause_at_checkpoint(repo_root, project_id, run, node_run, "gate_waiting")?;
                }
                return Ok(true);
            }
            WorkflowNodeDto::HumanCheckpoint { .. } => {
                pause_at_checkpoint(repo_root, project_id, run, node_run, "human_checkpoint")?;
                return Ok(true);
            }
            WorkflowNodeDto::StateRead {
                query,
                output_artifact_type,
                ..
            }
            | WorkflowNodeDto::StateQuery {
                query,
                output_artifact_type,
                ..
            } => {
                run_state_query_node(
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    query,
                    output_artifact_type,
                )?;
                return Ok(true);
            }
            WorkflowNodeDto::StateWrite { operation, .. }
            | WorkflowNodeDto::StatePatch { operation, .. } => {
                let operation = runtime_state_write_operation_for_node(
                    &run.definition_snapshot,
                    &node_run.node_id,
                    operation,
                );
                run_state_write_node(repo_root, project_id, run, node_run, &operation)?;
                return Ok(true);
            }
            WorkflowNodeDto::CollectionLoop {
                collection,
                item_artifact_type,
                sort_key,
                max_item_count,
                controls,
                ..
            } => {
                run_collection_loop_node(
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    collection,
                    item_artifact_type,
                    sort_key.as_deref(),
                    *max_item_count,
                    controls,
                )?;
                return Ok(true);
            }
            WorkflowNodeDto::Command {
                command,
                args,
                allowed_commands,
                working_directory,
                timeout_seconds,
                success_exit_codes,
                output_contract,
                parser,
                ..
            } => {
                let args = runtime_template_strings_for_node(
                    &run.definition_snapshot,
                    &node_run.node_id,
                    args,
                );
                let working_directory = working_directory.as_ref().map(|value| {
                    runtime_template_string_for_node(
                        &run.definition_snapshot,
                        &node_run.node_id,
                        value,
                    )
                });
                run_command_node(
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    command,
                    &args,
                    allowed_commands,
                    working_directory.as_deref(),
                    *timeout_seconds,
                    success_exit_codes,
                    output_contract,
                    parser.extraction,
                    parser.render_text_path.as_deref(),
                )?;
                return Ok(true);
            }
            WorkflowNodeDto::Subgraph {
                subgraph_id,
                input_bindings,
                output_contract,
                ..
            } => {
                run_subgraph_node(
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    subgraph_id,
                    input_bindings,
                    output_contract,
                )?;
                return Ok(true);
            }
            WorkflowNodeDto::Terminal {
                terminal_status, ..
            } => {
                if let Some(context) =
                    subgraph_context_for_node_id(&run.definition_snapshot, &node_run.node_id)
                {
                    complete_subgraph_terminal(
                        repo_root,
                        project_id,
                        run,
                        node_run,
                        *terminal_status,
                        &context,
                    )?;
                } else {
                    project_store::update_workflow_run_node(
                        repo_root,
                        project_id,
                        &node_run.id,
                        WorkflowNodeRunStatusDto::Succeeded,
                        None,
                        None,
                        None,
                    )?;
                    complete_for_terminal(repo_root, project_id, run, *terminal_status)?;
                }
                return Ok(true);
            }
        }
    }
    Ok(false)
}

struct ResourceConflict {
    node_run_id: String,
    node_id: String,
    scopes: Vec<String>,
}

fn resource_conflict_for_node(
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    node: &WorkflowNodeDto,
) -> Option<ResourceConflict> {
    if run
        .definition_snapshot
        .run_policy
        .resource_conflict_policy
        .mode
        == WorkflowResourceConflictModeDto::AllowConflicts
    {
        return None;
    }
    let candidate_scopes = resource_scopes_for_node(&run.definition_snapshot, node);
    if candidate_scopes.is_empty() {
        return None;
    }

    for running in run.nodes.iter().filter(|running| {
        running.id != node_run.id
            && matches!(
                running.status,
                WorkflowNodeRunStatusDto::Starting | WorkflowNodeRunStatusDto::Running
            )
    }) {
        let Some(running_node) = find_node(&run.definition_snapshot, &running.node_id) else {
            continue;
        };
        let running_scopes = resource_scopes_for_node(&run.definition_snapshot, running_node);
        let overlap = overlapping_resource_scopes(&candidate_scopes, &running_scopes);
        if !overlap.is_empty() {
            return Some(ResourceConflict {
                node_run_id: running.id.clone(),
                node_id: running.node_id.clone(),
                scopes: overlap,
            });
        }
    }
    None
}

fn resource_scopes_for_node(
    definition: &WorkflowDefinitionDto,
    node: &WorkflowNodeDto,
) -> Vec<String> {
    match node {
        WorkflowNodeDto::Agent {
            resource_scopes, ..
        } if !resource_scopes.is_empty() => normalize_resource_scopes(resource_scopes),
        WorkflowNodeDto::Agent { .. } => normalize_resource_scopes(
            &definition
                .run_policy
                .resource_conflict_policy
                .default_scopes,
        ),
        _ => Vec::new(),
    }
}

fn normalize_resource_scopes(scopes: &[String]) -> Vec<String> {
    let mut normalized = scopes
        .iter()
        .map(|scope| scope.trim())
        .filter(|scope| !scope.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn overlapping_resource_scopes(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .filter(|scope| right.iter().any(|candidate| candidate == *scope))
        .cloned()
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn start_agent_node<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    title: &str,
    agent_ref: &AgentRefDto,
    input_bindings: &[WorkflowInputBindingDto],
    run_overrides: Option<&WorkflowRunOverrideDto>,
) -> CommandResult<()> {
    if let Some(snapshot) = load_existing_agent_run_for_node(repo_root, project_id, node_run)? {
        project_store::update_workflow_run_node(
            repo_root,
            project_id,
            &node_run.id,
            WorkflowNodeRunStatusDto::Running,
            Some(&snapshot.run.run_id),
            Some(&snapshot.run.agent_session_id),
            None,
        )?;
        project_store::insert_workflow_event(
            repo_root,
            project_id,
            &run.id,
            Some(&node_run.id),
            "workflow_agent_reconnected",
            &json!({
                "nodeId": node_run.node_id,
                "runtimeRunId": snapshot.run.run_id,
                "agentSessionId": snapshot.run.agent_session_id
            }),
        )?;
        return Ok(());
    }

    let default_output_contract = WorkflowOutputContractDto::default();
    let output_contract = output_contract_for_node(&run.definition_snapshot, &node_run.node_id)
        .unwrap_or(&default_output_contract);
    let output_json_schema = artifact_schema_for_output(&run.definition_snapshot, output_contract);
    let prompt = match build_agent_node_prompt(
        &run.definition_snapshot.name,
        title,
        run_overrides.map(|overrides| overrides.prompt_preface.as_str()),
        output_contract,
        output_json_schema,
        run.initial_input.as_ref(),
        input_bindings,
        &run.artifacts,
    ) {
        Ok(prompt) => prompt,
        Err(error) if error.code == "workflow_required_input_missing" => {
            fail_node_with_recoverable_error(
                repo_root,
                project_id,
                run,
                node_run,
                "workflow_required_input_missing",
                &error.code,
                &error.message,
            )?;
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    let controls = controls_for_agent_ref(
        repo_root,
        &run.definition_snapshot,
        agent_ref,
        run_overrides,
    )?;
    let session = project_store::create_agent_session(
        repo_root,
        &AgentSessionCreateRecord {
            project_id: project_id.into(),
            title: format!("Workflow: {title}"),
            summary: format!(
                "Node `{}` in Workflow `{}`.",
                node_run.node_id, run.definition_snapshot.name
            ),
            selected: false,
            session_kind: crate::db::project_store::AgentSessionKind::Standard,
        },
    )?;
    let resource_scopes = find_node(&run.definition_snapshot, &node_run.node_id)
        .map(|node| resource_scopes_for_node(&run.definition_snapshot, node))
        .unwrap_or_default();
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        "workflow_agent_start_requested",
        &json!({
            "nodeId": node_run.node_id,
            "agentRef": agent_ref,
            "agentSessionId": session.agent_session_id.clone(),
            "resourceScopes": resource_scopes,
        }),
    )?;
    let agent_run = start_agent_task_blocking(
        app,
        state,
        StartAgentTaskRequestDto {
            project_id: project_id.into(),
            agent_session_id: session.agent_session_id.clone(),
            run_id: Some(node_run.idempotency_key.clone()),
            prompt,
            controls: Some(controls),
            attachments: Vec::new(),
        },
    )?;
    project_store::update_workflow_run_node(
        repo_root,
        project_id,
        &node_run.id,
        WorkflowNodeRunStatusDto::Running,
        Some(&agent_run.run_id),
        Some(&session.agent_session_id),
        None,
    )?;
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        "workflow_agent_started",
        &json!({
            "runtimeRunId": agent_run.run_id,
            "agentSessionId": session.agent_session_id
        }),
    )?;
    Ok(())
}

fn load_existing_agent_run_for_node(
    repo_root: &std::path::Path,
    project_id: &str,
    node_run: &WorkflowRunNodeDto,
) -> CommandResult<Option<AgentRunSnapshotRecord>> {
    match project_store::load_agent_run(repo_root, project_id, &node_run.idempotency_key) {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(error) if error.code == "agent_run_not_found" => Ok(None),
        Err(error) => Err(error),
    }
}

fn run_state_query_node(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    query: &WorkflowStateQueryDto,
    output_artifact_type: &str,
) -> CommandResult<()> {
    let payload = project_store::query_delivery_state(repo_root, project_id, query)?;
    project_store::insert_workflow_artifact(
        repo_root,
        project_id,
        &run.id,
        &node_run.id,
        output_artifact_type,
        1,
        &payload,
        Some(&format!(
            "{} {} record(s)",
            query.entity_type.as_str(),
            payload
                .get("count")
                .and_then(JsonValue::as_u64)
                .unwrap_or(0)
        )),
    )?;
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        "workflow_state_read",
        &json!({
            "nodeId": node_run.node_id,
            "entityType": query.entity_type.as_str(),
            "recordCount": payload.get("count"),
        }),
    )?;
    project_store::update_workflow_run_node(
        repo_root,
        project_id,
        &node_run.id,
        WorkflowNodeRunStatusDto::Succeeded,
        None,
        None,
        None,
    )
}

fn run_state_write_node(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    operation: &WorkflowStateWriteOperationDto,
) -> CommandResult<()> {
    let resolved_operation = resolve_state_write_operation(operation, run, None)?;
    let payload = project_store::write_delivery_state(
        repo_root,
        project_id,
        DeliveryStateWriteContext {
            workflow_run_id: Some(&run.id),
            node_run_id: Some(&node_run.id),
        },
        &resolved_operation,
    )?;
    project_store::insert_workflow_artifact(
        repo_root,
        project_id,
        &run.id,
        &node_run.id,
        &operation.output_artifact_type,
        1,
        &payload,
        payload
            .get("record")
            .and_then(|record| record.get("title"))
            .and_then(JsonValue::as_str)
            .or_else(|| payload.get("id").and_then(JsonValue::as_str)),
    )?;
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        "workflow_state_written",
        &json!({
            "nodeId": node_run.node_id,
            "entityType": operation.entity_type.as_str(),
            "action": operation.action.as_str(),
            "entityId": payload.get("id"),
        }),
    )?;
    project_store::update_workflow_run_node(
        repo_root,
        project_id,
        &node_run.id,
        WorkflowNodeRunStatusDto::Succeeded,
        None,
        None,
        None,
    )
}

fn resolve_state_write_operation(
    operation: &WorkflowStateWriteOperationDto,
    run: &WorkflowRunDto,
    context: Option<&JsonValue>,
) -> CommandResult<WorkflowStateWriteOperationDto> {
    let resolved_idempotency_key = operation
        .idempotency_key
        .as_deref()
        .map(|value| {
            resolve_template_string_with_context(value, run, context)
                .map(|value| template_value_to_string(&value))
        })
        .transpose()?;
    let resolved_target_id = operation
        .target_id
        .as_deref()
        .map(|value| {
            resolve_template_string_with_context(value, run, context)
                .map(|value| template_value_to_string(&value))
        })
        .transpose()?;
    Ok(WorkflowStateWriteOperationDto {
        idempotency_key: resolved_idempotency_key,
        target_id: resolved_target_id,
        payload: resolve_template_object_with_context(
            &JsonValue::Object(operation.payload.clone()),
            run,
            context,
        )?
        .as_object()
        .cloned()
        .unwrap_or_default(),
        ..operation.clone()
    })
}

fn resolve_input_bindings_payload(
    run: &WorkflowRunDto,
    input_bindings: &[WorkflowInputBindingDto],
) -> CommandResult<JsonValue> {
    if input_bindings.is_empty() {
        return Ok(run.initial_input.clone().unwrap_or_else(|| json!({})));
    }
    let artifact_index = artifact_payloads_by_ref(run);
    let mut payload = serde_json::Map::new();
    for binding in input_bindings {
        let (name, required, value) = match binding {
            WorkflowInputBindingDto::RunInput {
                name,
                required,
                path,
                ..
            } => {
                let value = match (run.initial_input.as_ref(), path.as_deref()) {
                    (Some(input), Some(path)) => json_path_lookup(input, path).cloned(),
                    (Some(input), None) => Some(input.clone()),
                    (None, _) => None,
                };
                (name, *required, value)
            }
            WorkflowInputBindingDto::Artifact {
                name,
                required,
                artifact_ref,
                path,
                ..
            }
            | WorkflowInputBindingDto::State {
                name,
                required,
                state_ref: artifact_ref,
                path,
                ..
            } => {
                let value = artifact_index.get(artifact_ref).and_then(|artifact| {
                    path.as_deref()
                        .and_then(|path| json_path_lookup(artifact, path).cloned())
                        .or_else(|| Some((*artifact).clone()))
                });
                (name, *required, value)
            }
        };
        if let Some(value) = value {
            payload.insert(name.clone(), value);
        } else if required {
            return Err(CommandError::user_fixable(
                "workflow_required_input_missing",
                format!("Workflow subgraph input `{name}` is required but missing."),
            ));
        }
    }
    Ok(JsonValue::Object(payload))
}

fn runtime_input_bindings_for_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
    input_bindings: &[WorkflowInputBindingDto],
) -> Vec<WorkflowInputBindingDto> {
    let Some(context) = subgraph_context_for_node_id(definition, node_id) else {
        return input_bindings.to_vec();
    };
    let mut bindings = context
        .subgraph
        .input_bindings
        .iter()
        .map(|binding| subgraph_input_binding_for_parent(&context.parent_node_id, binding))
        .collect::<Vec<_>>();
    bindings.extend(input_bindings.iter().map(|binding| {
        namespace_input_binding(binding, &context.parent_node_id, context.subgraph)
    }));
    bindings
}

fn subgraph_input_binding_for_parent(
    parent_node_id: &str,
    binding: &WorkflowInputBindingDto,
) -> WorkflowInputBindingDto {
    let (name, required, prompt_label) = match binding {
        WorkflowInputBindingDto::RunInput {
            name,
            required,
            prompt_label,
            ..
        }
        | WorkflowInputBindingDto::Artifact {
            name,
            required,
            prompt_label,
            ..
        }
        | WorkflowInputBindingDto::State {
            name,
            required,
            prompt_label,
            ..
        } => (name.clone(), *required, prompt_label.clone()),
    };
    WorkflowInputBindingDto::Artifact {
        name: name.clone(),
        required,
        artifact_ref: format!("{parent_node_id}.{SUBGRAPH_INPUT_ARTIFACT_TYPE}"),
        path: Some(format!("$.{name}")),
        prompt_label,
    }
}

fn namespace_input_binding(
    binding: &WorkflowInputBindingDto,
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
) -> WorkflowInputBindingDto {
    match binding {
        WorkflowInputBindingDto::Artifact {
            name,
            required,
            artifact_ref,
            path,
            prompt_label,
        } => WorkflowInputBindingDto::Artifact {
            name: name.clone(),
            required: *required,
            artifact_ref: namespace_artifact_ref(parent_node_id, subgraph, artifact_ref),
            path: path.clone(),
            prompt_label: prompt_label.clone(),
        },
        WorkflowInputBindingDto::State {
            name,
            required,
            state_ref,
            path,
            prompt_label,
        } => WorkflowInputBindingDto::State {
            name: name.clone(),
            required: *required,
            state_ref: namespace_artifact_ref(parent_node_id, subgraph, state_ref),
            path: path.clone(),
            prompt_label: prompt_label.clone(),
        },
        WorkflowInputBindingDto::RunInput { .. } => binding.clone(),
    }
}

fn runtime_state_write_operation_for_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
    operation: &WorkflowStateWriteOperationDto,
) -> WorkflowStateWriteOperationDto {
    let Some(context) = subgraph_context_for_node_id(definition, node_id) else {
        return operation.clone();
    };
    let mut operation = operation.clone();
    operation.idempotency_key = operation
        .idempotency_key
        .as_deref()
        .map(|value| namespace_template_string(value, &context.parent_node_id, context.subgraph));
    operation.target_id = operation
        .target_id
        .as_deref()
        .map(|value| namespace_template_string(value, &context.parent_node_id, context.subgraph));
    operation.payload = operation
        .payload
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                namespace_template_value(value, &context.parent_node_id, context.subgraph),
            )
        })
        .collect();
    operation
}

fn runtime_template_strings_for_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
    values: &[String],
) -> Vec<String> {
    values
        .iter()
        .map(|value| runtime_template_string_for_node(definition, node_id, value))
        .collect()
}

fn runtime_template_string_for_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
    value: &str,
) -> String {
    let Some(context) = subgraph_context_for_node_id(definition, node_id) else {
        return value.to_string();
    };
    namespace_template_string(value, &context.parent_node_id, context.subgraph)
}

fn resolve_template_object_with_context(
    value: &JsonValue,
    run: &WorkflowRunDto,
    context: Option<&JsonValue>,
) -> CommandResult<JsonValue> {
    match value {
        JsonValue::Object(map) => {
            let mut resolved = serde_json::Map::new();
            for (key, value) in map {
                resolved.insert(
                    key.clone(),
                    resolve_template_object_with_context(value, run, context)?,
                );
            }
            Ok(JsonValue::Object(resolved))
        }
        JsonValue::Array(values) => values
            .iter()
            .map(|value| resolve_template_object_with_context(value, run, context))
            .collect::<Result<Vec<_>, _>>()
            .map(JsonValue::Array),
        JsonValue::String(text) => resolve_template_string_with_context(text, run, context),
        value => Ok(value.clone()),
    }
}

fn resolve_template_string(text: &str, run: &WorkflowRunDto) -> CommandResult<JsonValue> {
    resolve_template_string_with_context(text, run, None)
}

fn resolve_template_string_with_context(
    text: &str,
    run: &WorkflowRunDto,
    context: Option<&JsonValue>,
) -> CommandResult<JsonValue> {
    let trimmed = text.trim();
    if let Some(expression) = trimmed
        .strip_prefix("{{")
        .and_then(|value| value.strip_suffix("}}"))
    {
        return resolve_template_expression_with_context(expression.trim(), run, context);
    }

    let mut rendered = String::new();
    let mut remainder = text;
    loop {
        let Some(start) = remainder.find("{{") else {
            rendered.push_str(remainder);
            break;
        };
        rendered.push_str(&remainder[..start]);
        let after_start = &remainder[start + 2..];
        let Some(end) = after_start.find("}}") else {
            rendered.push_str(&remainder[start..]);
            break;
        };
        let expression = after_start[..end].trim();
        let value = resolve_template_expression_with_context(expression, run, context)?;
        rendered.push_str(&template_value_to_string(&value));
        remainder = &after_start[end + 2..];
    }
    Ok(JsonValue::String(rendered))
}

fn resolve_template_expression_with_context(
    expression: &str,
    run: &WorkflowRunDto,
    context: Option<&JsonValue>,
) -> CommandResult<JsonValue> {
    if expression == "decision" {
        return context.cloned().ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_template_context_missing",
                "Workflow template references `decision`, but no checkpoint decision context is available.",
            )
        });
    }
    if let Some(path) = expression.strip_prefix("decision.") {
        return lookup_template_context(context, &format!("$.{path}"));
    }
    if let Some(path) = expression.strip_prefix("decision:") {
        return lookup_template_context(context, path.trim());
    }
    if expression == "input" {
        return run.initial_input.clone().ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_template_input_missing",
                "Workflow state write template references `input`, but the run has no initial input.",
            )
        });
    }
    if let Some(path) = expression.strip_prefix("input.") {
        return lookup_initial_input(run, &format!("$.{path}"));
    }
    if let Some(path) = expression.strip_prefix("input:") {
        return lookup_initial_input(run, path.trim());
    }
    if expression == "run.id" {
        return Ok(JsonValue::String(run.id.clone()));
    }
    if expression == "workflow.id" {
        return Ok(JsonValue::String(run.workflow_id.clone()));
    }
    if let Some(rest) = expression
        .strip_prefix("artifact.")
        .or_else(|| expression.strip_prefix("state."))
    {
        return lookup_artifact_template_ref(run, rest);
    }
    if let Some(rest) = expression
        .strip_prefix("artifact:")
        .or_else(|| expression.strip_prefix("state:"))
    {
        return lookup_artifact_template_ref(run, rest.trim());
    }

    Err(CommandError::user_fixable(
        "workflow_template_expression_unknown",
        format!("Workflow template expression `{expression}` is not supported."),
    ))
}

fn lookup_template_context(context: Option<&JsonValue>, path: &str) -> CommandResult<JsonValue> {
    let context = context.ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_template_context_missing",
            format!(
                "Workflow template references `{path}`, but no checkpoint decision context is available."
            ),
        )
    })?;
    json_path_lookup(context, path).cloned().ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_template_context_missing",
            format!("Workflow template could not resolve checkpoint decision path `{path}`."),
        )
    })
}

fn lookup_initial_input(run: &WorkflowRunDto, path: &str) -> CommandResult<JsonValue> {
    let input = run.initial_input.as_ref().ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_template_input_missing",
            format!("Workflow template references `{path}`, but the run has no initial input."),
        )
    })?;
    json_path_lookup(input, path).cloned().ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_template_input_missing",
            format!("Workflow template could not resolve initial input path `{path}`."),
        )
    })
}

fn lookup_artifact_template_ref(
    run: &WorkflowRunDto,
    expression: &str,
) -> CommandResult<JsonValue> {
    let (artifact_ref, path) = split_artifact_template_ref(expression);
    let index = artifact_payloads_by_ref(run);
    let payload = index.get(&artifact_ref).ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_template_artifact_missing",
            format!(
                "Workflow template references `{artifact_ref}`, but no matching artifact exists."
            ),
        )
    })?;
    if let Some(path) = path {
        return json_path_lookup(payload, &path).cloned().ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_template_artifact_path_missing",
                format!("Workflow template could not resolve `{artifact_ref}{path}`."),
            )
        });
    }
    Ok((*payload).to_owned())
}

fn split_artifact_template_ref(expression: &str) -> (String, Option<String>) {
    if let Some((artifact_ref, path)) = expression.split_once(' ') {
        let trimmed_path = path.trim();
        return (
            artifact_ref.trim().to_string(),
            (!trimmed_path.is_empty()).then(|| trimmed_path.to_string()),
        );
    }

    let parts = expression.split('.').collect::<Vec<_>>();
    if parts.len() >= 3 {
        (
            format!("{}.{}", parts[0], parts[1]),
            Some(format!("$.{}", parts[2..].join("."))),
        )
    } else {
        (expression.trim().to_string(), None)
    }
}

fn namespace_template_value(
    value: &JsonValue,
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
) -> JsonValue {
    match value {
        JsonValue::String(text) => {
            JsonValue::String(namespace_template_string(text, parent_node_id, subgraph))
        }
        JsonValue::Array(values) => JsonValue::Array(
            values
                .iter()
                .map(|value| namespace_template_value(value, parent_node_id, subgraph))
                .collect(),
        ),
        JsonValue::Object(map) => JsonValue::Object(
            map.iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        namespace_template_value(value, parent_node_id, subgraph),
                    )
                })
                .collect(),
        ),
        value => value.clone(),
    }
}

fn namespace_template_string(
    text: &str,
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
) -> String {
    let mut rendered = String::new();
    let mut remainder = text;
    loop {
        let Some(start) = remainder.find("{{") else {
            rendered.push_str(remainder);
            break;
        };
        rendered.push_str(&remainder[..start]);
        let after_start = &remainder[start + 2..];
        let Some(end) = after_start.find("}}") else {
            rendered.push_str(&remainder[start..]);
            break;
        };
        let expression = after_start[..end].trim();
        rendered.push_str("{{");
        rendered.push_str(&namespace_template_expression(
            expression,
            parent_node_id,
            subgraph,
        ));
        rendered.push_str("}}");
        remainder = &after_start[end + 2..];
    }
    rendered
}

fn namespace_template_expression(
    expression: &str,
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
) -> String {
    for prefix in ["artifact:", "state:"] {
        if let Some(rest) = expression.strip_prefix(prefix) {
            let (artifact_ref, path) = split_artifact_template_ref(rest.trim());
            let namespaced_ref = namespace_artifact_ref(parent_node_id, subgraph, &artifact_ref);
            return match path {
                Some(path) => format!("{prefix}{namespaced_ref} {path}"),
                None => format!("{prefix}{namespaced_ref}"),
            };
        }
    }
    for prefix in ["artifact.", "state."] {
        if let Some(rest) = expression.strip_prefix(prefix) {
            let (artifact_ref, path) = split_artifact_template_ref(rest.trim());
            let namespaced_ref = namespace_artifact_ref(parent_node_id, subgraph, &artifact_ref);
            let colon_prefix = if prefix == "artifact." {
                "artifact:"
            } else {
                "state:"
            };
            return match path {
                Some(path) => format!("{colon_prefix}{namespaced_ref} {path}"),
                None => format!("{colon_prefix}{namespaced_ref}"),
            };
        }
    }
    expression.to_string()
}

fn namespace_artifact_ref(
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
    artifact_ref: &str,
) -> String {
    let Some((node_ref, artifact_type)) = artifact_ref.split_once('.') else {
        return artifact_ref.to_string();
    };
    if !subgraph.nodes.iter().any(|node| node.id() == node_ref) {
        return artifact_ref.to_string();
    }
    format!(
        "{}.{}",
        namespaced_subgraph_node_id(parent_node_id, node_ref),
        artifact_type
    )
}

fn namespace_node_ref(
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
    node_id: &str,
) -> String {
    if subgraph.nodes.iter().any(|node| node.id() == node_id) {
        namespaced_subgraph_node_id(parent_node_id, node_id)
    } else {
        node_id.to_string()
    }
}

fn namespace_loop_key(parent_node_id: &str, loop_key: &str) -> String {
    if loop_key.contains(SUBGRAPH_NODE_SEPARATOR) {
        loop_key.to_string()
    } else {
        format!("{parent_node_id}{SUBGRAPH_NODE_SEPARATOR}{loop_key}")
    }
}

fn artifact_payloads_by_ref(run: &WorkflowRunDto) -> BTreeMap<String, &JsonValue> {
    let node_id_by_run_id = run
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.node_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut index = BTreeMap::new();
    for artifact in &run.artifacts {
        if let Some(node_id) = node_id_by_run_id.get(artifact.producer_node_run_id.as_str()) {
            index.insert(
                format!("{node_id}.{}", artifact.artifact_type),
                &artifact.payload,
            );
        }
    }
    index
}

fn template_value_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::String(text) => text.clone(),
        JsonValue::Null => String::new(),
        value => serde_json::to_string(value).unwrap_or_else(|_| value.to_string()),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_collection_loop_node(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    collection: &WorkflowStateQueryDto,
    item_artifact_type: &str,
    sort_key: Option<&str>,
    max_item_count: u32,
    controls: &WorkflowCollectionLoopControlsDto,
) -> CommandResult<()> {
    let mut payload = project_store::query_delivery_state(repo_root, project_id, collection)?;
    let mut records = payload
        .get_mut("records")
        .and_then(JsonValue::as_array_mut)
        .map(std::mem::take)
        .unwrap_or_default();
    if let Some(sort_key) = sort_key {
        records.sort_by(|left, right| {
            compare_json_values_for_runtime(
                json_path_lookup(left, sort_key),
                json_path_lookup(right, sort_key),
            )
        });
    }
    let control_selection = collection_control_selection(run.initial_input.as_ref(), controls);
    records = apply_collection_controls(records, run.initial_input.as_ref(), controls);

    let processed = processed_collection_item_ids(run, &node_run.node_id);
    let processed_count = processed.len() as u32;
    let next = if processed_count >= max_item_count {
        None
    } else {
        records
            .iter()
            .find(|record| {
                record
                    .get("id")
                    .and_then(JsonValue::as_str)
                    .map(|id| !processed.contains(id))
                    .unwrap_or(false)
            })
            .cloned()
    };
    let has_item = next.is_some();
    let item_id = next
        .as_ref()
        .and_then(|record| record.get("id"))
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned);
    let artifact_payload = json!({
        "hasItem": has_item,
        "item": next,
        "itemId": item_id,
        "processedCount": processed_count,
        "remainingCount": records.len().saturating_sub(processed.len()),
        "maxItemCount": max_item_count,
        "partialSelection": control_selection.has_selection,
        "controls": {
            "only": control_selection.only_values,
            "from": control_selection.from_value,
            "to": control_selection.to_value,
        },
    });
    project_store::insert_workflow_artifact(
        repo_root,
        project_id,
        &run.id,
        &node_run.id,
        item_artifact_type,
        1,
        &artifact_payload,
        item_id.as_deref().or(Some(if has_item {
            "Collection item selected"
        } else {
            "Collection complete"
        })),
    )?;
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        if has_item {
            "workflow_collection_item_started"
        } else {
            "workflow_collection_completed"
        },
        &json!({
            "loopNodeId": node_run.node_id,
            "itemId": item_id,
            "processedCount": processed_count,
            "remainingCount": records.len().saturating_sub(processed.len()),
        }),
    )?;
    project_store::update_workflow_run_node(
        repo_root,
        project_id,
        &node_run.id,
        WorkflowNodeRunStatusDto::Succeeded,
        None,
        None,
        None,
    )
}

fn apply_collection_controls(
    records: Vec<JsonValue>,
    initial_input: Option<&JsonValue>,
    controls: &WorkflowCollectionLoopControlsDto,
) -> Vec<JsonValue> {
    let only_values = controls
        .only_input_path
        .as_deref()
        .and_then(|path| initial_input.and_then(|input| json_path_lookup(input, path)))
        .map(control_values);
    let from_value = controls
        .from_input_path
        .as_deref()
        .and_then(|path| initial_input.and_then(|input| json_path_lookup(input, path)))
        .and_then(control_scalar);
    let to_value = controls
        .to_input_path
        .as_deref()
        .and_then(|path| initial_input.and_then(|input| json_path_lookup(input, path)))
        .and_then(control_scalar);

    records
        .into_iter()
        .filter(|record| {
            let key = collection_record_key(record);
            if let Some(only_values) = &only_values {
                let Some(key) = key.as_ref() else {
                    return false;
                };
                if !only_values
                    .iter()
                    .any(|value| control_values_equal(value, key))
                {
                    return false;
                }
            }
            if let (Some(from), Some(key)) = (from_value.as_ref(), key.as_ref()) {
                if compare_control_values(key, from) == std::cmp::Ordering::Less {
                    return false;
                }
            }
            if let (Some(to), Some(key)) = (to_value.as_ref(), key.as_ref()) {
                if compare_control_values(key, to) == std::cmp::Ordering::Greater {
                    return false;
                }
            }
            true
        })
        .collect()
}

#[derive(Debug, Clone)]
struct CollectionControlSelection {
    only_values: Option<Vec<JsonValue>>,
    from_value: Option<JsonValue>,
    to_value: Option<JsonValue>,
    has_selection: bool,
}

fn collection_control_selection(
    initial_input: Option<&JsonValue>,
    controls: &WorkflowCollectionLoopControlsDto,
) -> CollectionControlSelection {
    let only_values = controls
        .only_input_path
        .as_deref()
        .and_then(|path| initial_input.and_then(|input| json_path_lookup(input, path)))
        .map(control_values)
        .filter(|values| !values.is_empty());
    let from_value = controls
        .from_input_path
        .as_deref()
        .and_then(|path| initial_input.and_then(|input| json_path_lookup(input, path)))
        .and_then(control_scalar);
    let to_value = controls
        .to_input_path
        .as_deref()
        .and_then(|path| initial_input.and_then(|input| json_path_lookup(input, path)))
        .and_then(control_scalar);
    let has_selection = only_values.is_some() || from_value.is_some() || to_value.is_some();
    CollectionControlSelection {
        only_values,
        from_value,
        to_value,
        has_selection,
    }
}

fn processed_collection_item_ids(run: &WorkflowRunDto, loop_node_id: &str) -> BTreeSet<String> {
    let loop_node_run_ids = run
        .nodes
        .iter()
        .filter(|node| {
            node.node_id == loop_node_id && node.status == WorkflowNodeRunStatusDto::Succeeded
        })
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    run.artifacts
        .iter()
        .filter(|artifact| loop_node_run_ids.contains(artifact.producer_node_run_id.as_str()))
        .filter_map(|artifact| {
            artifact
                .payload
                .get("itemId")
                .and_then(JsonValue::as_str)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .collect()
}

fn collection_record_key(record: &JsonValue) -> Option<JsonValue> {
    record
        .get("phaseKey")
        .or_else(|| record.get("phase_key"))
        .or_else(|| record.get("id"))
        .or_else(|| record.get("sortOrder"))
        .or_else(|| record.get("sort_order"))
        .cloned()
}

fn control_values(value: &JsonValue) -> Vec<JsonValue> {
    match value {
        JsonValue::Array(values) => values.clone(),
        JsonValue::String(text) => text
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| JsonValue::String(value.to_string()))
            .collect(),
        value => vec![value.clone()],
    }
}

fn control_scalar(value: &JsonValue) -> Option<JsonValue> {
    match value {
        JsonValue::Array(values) => values.first().cloned(),
        JsonValue::Null => None,
        value => Some(value.clone()),
    }
}

fn control_values_equal(left: &JsonValue, right: &JsonValue) -> bool {
    if left == right {
        return true;
    }
    left.as_str().map(str::trim) == right.as_str().map(str::trim)
        || left
            .as_f64()
            .zip(right.as_f64())
            .is_some_and(|(left, right)| (left - right).abs() < f64::EPSILON)
}

fn compare_control_values(left: &JsonValue, right: &JsonValue) -> std::cmp::Ordering {
    match (left.as_f64(), right.as_f64()) {
        (Some(left), Some(right)) => left
            .partial_cmp(&right)
            .unwrap_or(std::cmp::Ordering::Equal),
        _ => template_value_to_string(left).cmp(&template_value_to_string(right)),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_command_node(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    command: &str,
    args: &[String],
    allowed_commands: &[String],
    working_directory: Option<&str>,
    timeout_seconds: u32,
    success_exit_codes: &[i32],
    output_contract: &WorkflowOutputContractDto,
    parser_extraction: WorkflowOutputExtractionDto,
    parser_render_text_path: Option<&str>,
) -> CommandResult<()> {
    if !allowed_commands.is_empty() && !allowed_commands.iter().any(|allowed| allowed == command) {
        return fail_node_with_recoverable_error(
            repo_root,
            project_id,
            run,
            node_run,
            "workflow_command_not_allowed",
            "workflow_command_not_allowed",
            &format!(
                "Command node `{}` is not in its allowlist.",
                node_run.node_id
            ),
        );
    }
    let resolved_args = args
        .iter()
        .map(|arg| resolve_template_string(arg, run).map(|value| template_value_to_string(&value)))
        .collect::<CommandResult<Vec<_>>>()?;
    let cwd = working_directory
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| repo_root.to_path_buf());
    let started = Instant::now();
    let mut child = Command::new(command)
        .args(&resolved_args)
        .current_dir(&cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            CommandError::retryable(
                "workflow_command_spawn_failed",
                format!(
                    "Xero could not start command node `{}`: {error}",
                    node_run.node_id
                ),
            )
        })?;
    let timeout = StdDuration::from_secs(timeout_seconds.into());
    let timed_out;
    loop {
        if child
            .try_wait()
            .map_err(|error| {
                CommandError::retryable(
                    "workflow_command_wait_failed",
                    format!(
                        "Xero could not poll command node `{}`: {error}",
                        node_run.node_id
                    ),
                )
            })?
            .is_some()
        {
            timed_out = false;
            break;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            timed_out = true;
            break;
        }
        std::thread::sleep(StdDuration::from_millis(25));
    }
    let output = child.wait_with_output().map_err(|error| {
        CommandError::retryable(
            "workflow_command_output_failed",
            format!(
                "Xero could not collect command node `{}` output: {error}",
                node_run.node_id
            ),
        )
    })?;
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let (parsed, parse_error) = match parser_extraction {
        WorkflowOutputExtractionDto::GenericText => (JsonValue::String(stdout.clone()), None),
        WorkflowOutputExtractionDto::JsonObject | WorkflowOutputExtractionDto::JsonArray => {
            match serde_json::from_str::<JsonValue>(&stdout) {
                Ok(value) => (value, None),
                Err(error) => (JsonValue::Null, Some(error.to_string())),
            }
        }
    };
    let parser_shape_valid = match parser_extraction {
        WorkflowOutputExtractionDto::GenericText => true,
        WorkflowOutputExtractionDto::JsonObject => parsed.is_object(),
        WorkflowOutputExtractionDto::JsonArray => parsed.is_array(),
    };
    let ok = !timed_out
        && success_exit_codes.contains(&exit_code)
        && parse_error.is_none()
        && parser_shape_valid;
    let payload = json!({
        "status": if ok { "passed" } else { "failed" },
        "command": command,
        "args": resolved_args,
        "workingDirectory": cwd,
        "exitCode": exit_code,
        "timedOut": timed_out,
        "stdout": stdout,
        "stderr": stderr,
        "parsed": parsed,
        "parseError": parse_error,
    });
    let json_schema = artifact_schema_for_output(&run.definition_snapshot, output_contract);
    let (validated_render_text, diagnostics) =
        match validate_workflow_artifact_payload(output_contract, json_schema, &payload) {
            Ok(result) => result,
            Err(error) => {
                fail_node_with_recoverable_error(
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    "workflow_command_artifact_invalid",
                    &error.code,
                    &error.message,
                )?;
                return Ok(());
            }
        };
    let render_text = parser_render_text_path
        .and_then(|path| json_path_lookup(&payload, path))
        .and_then(JsonValue::as_str)
        .or(validated_render_text.as_deref())
        .or_else(|| payload.get("stdout").and_then(JsonValue::as_str));
    project_store::insert_workflow_artifact(
        repo_root,
        project_id,
        &run.id,
        &node_run.id,
        &output_contract.artifact_type,
        output_contract.schema_version,
        &payload,
        render_text,
    )?;
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        "workflow_command_completed",
        &json!({
            "nodeId": node_run.node_id,
            "command": command,
            "exitCode": exit_code,
            "timedOut": timed_out,
            "parseError": payload.get("parseError"),
            "status": if ok { "passed" } else { "failed" },
            "validationStatus": "valid",
            "diagnostics": diagnostics.iter().map(|diagnostic| {
                json!({
                    "code": diagnostic.code,
                    "path": diagnostic.path,
                    "message": diagnostic.message,
                })
            }).collect::<Vec<_>>(),
        }),
    )?;
    project_store::update_workflow_run_node(
        repo_root,
        project_id,
        &node_run.id,
        if ok {
            WorkflowNodeRunStatusDto::Succeeded
        } else {
            WorkflowNodeRunStatusDto::Failed
        },
        None,
        None,
        (!ok).then_some(if timed_out {
            "workflow_command_timeout"
        } else {
            "workflow_command_failed"
        }),
    )
}

fn run_subgraph_node(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    subgraph_id: &str,
    input_bindings: &[WorkflowInputBindingDto],
    output_contract: &WorkflowOutputContractDto,
) -> CommandResult<()> {
    let Some(subgraph) = run
        .definition_snapshot
        .subgraphs
        .iter()
        .find(|subgraph| subgraph.id == subgraph_id)
    else {
        return fail_node_with_recoverable_error(
            repo_root,
            project_id,
            run,
            node_run,
            "workflow_subgraph_missing",
            "workflow_subgraph_missing",
            &format!("Subgraph `{subgraph_id}` is missing from the Workflow snapshot."),
        );
    };
    let input_payload = resolve_input_bindings_payload(run, input_bindings)?;
    project_store::insert_workflow_artifact(
        repo_root,
        project_id,
        &run.id,
        &node_run.id,
        SUBGRAPH_INPUT_ARTIFACT_TYPE,
        1,
        &input_payload,
        Some("Subgraph input"),
    )?;
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        "workflow_subgraph_started",
        &json!({
            "nodeId": node_run.node_id,
            "subgraphId": subgraph_id,
            "startNodeId": subgraph.start_node_id,
            "outputArtifactType": output_contract.artifact_type,
        }),
    )?;
    project_store::update_workflow_run_node(
        repo_root,
        project_id,
        &node_run.id,
        WorkflowNodeRunStatusDto::Running,
        None,
        None,
        None,
    )?;
    let child_node_id = namespaced_subgraph_node_id(&node_run.node_id, &subgraph.start_node_id);
    let attempt = next_attempt_for_node(run, &child_node_id);
    let child_run = ensure_node_run(repo_root, run, &child_node_id, attempt)?;
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        "workflow_subgraph_child_scheduled",
        &json!({
            "nodeId": node_run.node_id,
            "subgraphId": subgraph_id,
            "childNodeId": child_node_id,
            "childNodeRunId": child_run.id,
        }),
    )
}

fn complete_subgraph_terminal(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    terminal_node_run: &WorkflowRunNodeDto,
    terminal_status: WorkflowTerminalStatusDto,
    context: &SubgraphNodeContext<'_>,
) -> CommandResult<()> {
    let parent_node_run = run
        .nodes
        .iter()
        .filter(|node| {
            node.node_id == context.parent_node_id.as_str()
                && node.status == WorkflowNodeRunStatusDto::Running
        })
        .max_by_key(|node| node.attempt_number)
        .ok_or_else(|| {
            CommandError::system_fault(
                "workflow_subgraph_parent_missing",
                format!(
                    "Subgraph child `{}` completed but parent `{}` is not running.",
                    terminal_node_run.node_id, context.parent_node_id
                ),
            )
        })?;
    let WorkflowNodeDto::Subgraph {
        subgraph_id,
        output_contract,
        ..
    } = context.parent_node
    else {
        return Err(CommandError::system_fault(
            "workflow_subgraph_parent_invalid",
            format!(
                "Subgraph child `{}` is attached to non-subgraph parent `{}`.",
                terminal_node_run.node_id, context.parent_node_id
            ),
        ));
    };
    let child_prefix = format!("{}{}", context.parent_node_id, SUBGRAPH_NODE_SEPARATOR);
    let child_node_run_ids = run
        .nodes
        .iter()
        .filter(|node| node.node_id.starts_with(&child_prefix))
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();
    let status = subgraph_status_for_terminal(terminal_status);
    let summary = format!(
        "Subgraph `{}` completed with `{}`.",
        subgraph_id,
        terminal_status.as_str()
    );
    let payload = if output_contract.extraction == WorkflowOutputExtractionDto::GenericText {
        json!({ "text": summary.clone() })
    } else {
        json!({
            "status": status,
            "subgraphId": subgraph_id,
            "summary": summary.clone(),
            "terminalNodeId": terminal_node_run.node_id,
            "terminalStatus": terminal_status.as_str(),
            "childNodeRunIds": child_node_run_ids,
        })
    };
    let json_schema = artifact_schema_for_output(&run.definition_snapshot, output_contract);
    let (render_text, diagnostics) =
        match validate_workflow_artifact_payload(output_contract, json_schema, &payload) {
            Ok(result) => result,
            Err(error) => {
                fail_node_with_recoverable_error(
                    repo_root,
                    project_id,
                    run,
                    parent_node_run,
                    "workflow_subgraph_artifact_invalid",
                    &error.code,
                    &error.message,
                )?;
                return Ok(());
            }
        };

    project_store::update_workflow_run_node(
        repo_root,
        project_id,
        &terminal_node_run.id,
        WorkflowNodeRunStatusDto::Succeeded,
        None,
        None,
        None,
    )?;
    project_store::insert_workflow_artifact(
        repo_root,
        project_id,
        &run.id,
        &parent_node_run.id,
        &output_contract.artifact_type,
        output_contract.schema_version,
        &payload,
        render_text
            .as_deref()
            .or_else(|| payload.get("summary").and_then(JsonValue::as_str))
            .or(Some(summary.as_str())),
    )?;
    project_store::insert_workflow_edge_decision(
        repo_root,
        project_id,
        &run.id,
        &terminal_node_run.node_id,
        &context.parent_node_id,
        "__subgraph_terminal__",
        &encode_workflow_condition(&WorkflowConditionDto::Always)?,
        &json!({
            "terminalStatus": terminal_status.as_str(),
            "subgraphId": subgraph_id,
        }),
    )?;
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&terminal_node_run.id),
        "workflow_subgraph_terminal_completed",
        &json!({
            "nodeId": terminal_node_run.node_id,
            "parentNodeId": context.parent_node_id.clone(),
            "subgraphId": subgraph_id,
            "terminalStatus": terminal_status.as_str(),
        }),
    )?;
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&parent_node_run.id),
        "workflow_subgraph_completed",
        &json!({
            "nodeId": context.parent_node_id.clone(),
            "subgraphId": subgraph_id,
            "terminalNodeId": terminal_node_run.node_id,
            "terminalStatus": terminal_status.as_str(),
            "status": status,
            "validationStatus": "valid",
            "diagnostics": diagnostics.iter().map(|diagnostic| {
                json!({
                    "code": diagnostic.code,
                    "path": diagnostic.path,
                    "message": diagnostic.message,
                })
            }).collect::<Vec<_>>(),
        }),
    )?;

    match terminal_status {
        WorkflowTerminalStatusDto::Success => project_store::update_workflow_run_node(
            repo_root,
            project_id,
            &parent_node_run.id,
            WorkflowNodeRunStatusDto::Succeeded,
            None,
            None,
            None,
        ),
        WorkflowTerminalStatusDto::Failure => project_store::update_workflow_run_node(
            repo_root,
            project_id,
            &parent_node_run.id,
            WorkflowNodeRunStatusDto::Failed,
            None,
            None,
            Some("workflow_subgraph_failed"),
        ),
        WorkflowTerminalStatusDto::Cancelled => project_store::update_workflow_run_node(
            repo_root,
            project_id,
            &parent_node_run.id,
            WorkflowNodeRunStatusDto::Cancelled,
            None,
            None,
            Some("workflow_subgraph_cancelled"),
        ),
        WorkflowTerminalStatusDto::NeedsHuman => {
            project_store::update_workflow_run_node(
                repo_root,
                project_id,
                &parent_node_run.id,
                WorkflowNodeRunStatusDto::WaitingOnGate,
                None,
                None,
                None,
            )?;
            project_store::update_workflow_run_status(
                repo_root,
                project_id,
                &run.id,
                WorkflowRunStatusDto::Paused,
                Some(WorkflowTerminalStatusDto::NeedsHuman),
                None,
            )
        }
    }
}

fn subgraph_status_for_terminal(terminal_status: WorkflowTerminalStatusDto) -> &'static str {
    match terminal_status {
        WorkflowTerminalStatusDto::Success => "succeeded",
        WorkflowTerminalStatusDto::Failure => "failed",
        WorkflowTerminalStatusDto::Cancelled => "cancelled",
        WorkflowTerminalStatusDto::NeedsHuman => "needs_human",
    }
}

fn fail_node_with_recoverable_error(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    event_type: &str,
    failure_class: &str,
    message: &str,
) -> CommandResult<()> {
    project_store::update_workflow_run_node(
        repo_root,
        project_id,
        &node_run.id,
        WorkflowNodeRunStatusDto::Failed,
        None,
        None,
        Some(failure_class),
    )?;
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        event_type,
        &json!({
            "nodeId": node_run.node_id,
            "failureClass": failure_class,
            "message": message,
        }),
    )
}

fn compare_json_values_for_runtime(
    left: Option<&JsonValue>,
    right: Option<&JsonValue>,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => match (left.as_f64(), right.as_f64()) {
            (Some(left), Some(right)) => left
                .partial_cmp(&right)
                .unwrap_or(std::cmp::Ordering::Equal),
            _ => template_value_to_string(left).cmp(&template_value_to_string(right)),
        },
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

fn controls_for_agent_ref(
    repo_root: &std::path::Path,
    definition: &WorkflowDefinitionDto,
    agent_ref: &AgentRefDto,
    run_overrides: Option<&WorkflowRunOverrideDto>,
) -> CommandResult<RuntimeRunControlInputDto> {
    let (runtime_agent_id, agent_definition_id, agent_definition_version) = match agent_ref {
        AgentRefDto::BuiltIn {
            runtime_agent_id,
            version,
        } => (*runtime_agent_id, None, Some(*version)),
        AgentRefDto::Custom {
            definition_id,
            version,
        } => {
            let selection = project_store::resolve_agent_definition_version_for_run(
                repo_root,
                Some(definition_id),
                Some(*version),
                crate::commands::default_runtime_agent_id(),
            )?;
            (
                selection.runtime_agent_id,
                Some(selection.definition_id),
                Some(selection.version),
            )
        }
    };
    let approval_mode: RuntimeRunApprovalModeDto = run_overrides
        .and_then(|overrides| overrides.approval_mode.clone())
        .or_else(|| definition.run_policy.approval_mode.clone())
        .unwrap_or_else(|| default_runtime_agent_approval_mode(&runtime_agent_id));
    Ok(RuntimeRunControlInputDto {
        runtime_agent_id,
        agent_definition_id,
        agent_definition_version,
        provider_profile_id: run_overrides
            .and_then(|overrides| overrides.provider_profile_id.clone())
            .or_else(|| definition.run_policy.default_provider_profile_id.clone()),
        model_id: run_overrides
            .and_then(|overrides| overrides.model_id.clone())
            .or_else(|| definition.run_policy.default_model_id.clone())
            .unwrap_or_default(),
        thinking_effort: None,
        approval_mode,
        plan_mode_required: run_overrides
            .map(|overrides| overrides.plan_mode_required)
            .unwrap_or(false),
        auto_compact_enabled: run_overrides
            .map(|overrides| overrides.auto_compact_enabled)
            .unwrap_or(true),
    })
}

fn ensure_node_run(
    repo_root: &std::path::Path,
    run: &WorkflowRunDto,
    node_id: &str,
    attempt: u32,
) -> CommandResult<WorkflowRunNodeDto> {
    let node = find_node(&run.definition_snapshot, node_id).ok_or_else(|| {
        CommandError::system_fault(
            "workflow_target_node_missing",
            format!("Workflow target node `{node_id}` was missing from its snapshot."),
        )
    })?;
    let idempotency_key = format!("{}:{}:{attempt}", run.id, node_id);
    project_store::insert_workflow_run_node(
        repo_root,
        &run.project_id,
        &run.id,
        node_id,
        node.node_type().as_str(),
        attempt,
        WorkflowNodeRunStatusDto::Eligible,
        &idempotency_key,
    )
}

fn ensure_direct_merge_targets_for_skipped_branch(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    skipped_node_id: &str,
) -> CommandResult<Vec<String>> {
    let mut targets = Vec::new();
    for edge in runtime_edges_from_node(&run.definition_snapshot, skipped_node_id) {
        let Some(WorkflowNodeDto::Merge { .. }) =
            find_node(&run.definition_snapshot, &edge.to_node_id)
        else {
            continue;
        };
        let attempt = next_attempt_for_node(run, &edge.to_node_id);
        ensure_node_run(repo_root, run, &edge.to_node_id, attempt)?;
        targets.push(edge.to_node_id.clone());
    }
    if targets.is_empty() {
        project_store::insert_workflow_event(
            repo_root,
            project_id,
            &run.id,
            None,
            "workflow_branch_skip_no_merge_target",
            &json!({ "nodeId": skipped_node_id }),
        )?;
    }
    Ok(targets)
}

fn loop_target_for_edge(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    edge: &WorkflowEdgeDto,
) -> CommandResult<String> {
    let Some(policy) = edge.loop_policy.as_ref() else {
        return Ok(edge.to_node_id.clone());
    };
    if let Some(detector) = policy.stall_detector {
        if let Some(failure_class) = stall_failure_class_for_detector(run, node_run, detector) {
            project_store::update_workflow_run_node(
                repo_root,
                project_id,
                &node_run.id,
                WorkflowNodeRunStatusDto::Stalled,
                None,
                None,
                Some(failure_class),
            )?;
            project_store::increment_workflow_loop_attempt(
                repo_root,
                project_id,
                &run.id,
                &policy.loop_key,
                &node_run.id,
                true,
            )?;
            project_store::insert_workflow_event(
                repo_root,
                project_id,
                &run.id,
                Some(&node_run.id),
                "workflow_node_stalled",
                &json!({
                    "nodeId": node_run.node_id,
                    "failureClass": failure_class,
                    "stallDetector": detector.as_str(),
                    "loopKey": policy.loop_key.as_str(),
                }),
            )?;
            insert_workflow_metric_event(
                repo_root,
                project_id,
                &run.id,
                Some(&node_run.id),
                "loop_exhaustion",
                &json!({
                    "loopKey": policy.loop_key.as_str(),
                    "stallDetector": detector.as_str(),
                    "failureClass": failure_class,
                    "onExhausted": policy.on_exhausted.as_str(),
                }),
            )?;
            return Ok(policy.on_exhausted.clone());
        }
    }
    let current_attempts = run
        .loop_attempts
        .iter()
        .find(|attempt| attempt.loop_key == policy.loop_key)
        .map(|attempt| attempt.attempt_count)
        .unwrap_or(0);
    if current_attempts >= policy.max_attempts {
        project_store::increment_workflow_loop_attempt(
            repo_root,
            project_id,
            &run.id,
            &policy.loop_key,
            &node_run.id,
            true,
        )?;
        insert_workflow_metric_event(
            repo_root,
            project_id,
            &run.id,
            Some(&node_run.id),
            "loop_exhaustion",
            &json!({
                "loopKey": policy.loop_key.as_str(),
                "attemptCount": current_attempts.saturating_add(1),
                "maxAttempts": policy.max_attempts,
                "onExhausted": policy.on_exhausted.as_str(),
            }),
        )?;
        return Ok(policy.on_exhausted.clone());
    }
    project_store::increment_workflow_loop_attempt(
        repo_root,
        project_id,
        &run.id,
        &policy.loop_key,
        &node_run.id,
        false,
    )?;
    Ok(edge.to_node_id.clone())
}

fn stall_failure_class_for_detector(
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    detector: WorkflowStallDetectorDto,
) -> Option<&'static str> {
    match detector {
        WorkflowStallDetectorDto::FindingCountNotDecreasing => {
            finding_count_not_decreasing(run, node_run).then_some("finding_count_not_decreasing")
        }
        WorkflowStallDetectorDto::SameFailureClassRepeated => {
            same_failure_class_repeated(run, node_run).then_some("same_failure_class_repeated")
        }
        WorkflowStallDetectorDto::NoArtifactProgress => {
            no_artifact_progress(run, node_run).then_some("no_artifact_progress")
        }
        WorkflowStallDetectorDto::RuntimeActivityTimeout => (node_run.failure_class.as_deref()
            == Some(RUNTIME_ACTIVITY_TIMEOUT_FAILURE_CLASS))
        .then_some(RUNTIME_ACTIVITY_TIMEOUT_FAILURE_CLASS),
        WorkflowStallDetectorDto::RetryLimitExceeded => (node_run.failure_class.as_deref()
            == Some("retry_limit_exceeded"))
        .then_some("retry_limit_exceeded"),
    }
}

fn same_failure_class_repeated(run: &WorkflowRunDto, node_run: &WorkflowRunNodeDto) -> bool {
    let Some(current_failure) = node_run.failure_class.as_deref() else {
        return false;
    };
    run.nodes
        .iter()
        .filter(|candidate| {
            candidate.node_id == node_run.node_id
                && candidate.attempt_number < node_run.attempt_number
        })
        .max_by_key(|candidate| candidate.attempt_number)
        .and_then(|candidate| candidate.failure_class.as_deref())
        == Some(current_failure)
}

fn no_artifact_progress(run: &WorkflowRunDto, node_run: &WorkflowRunNodeDto) -> bool {
    let Some(contract) = output_contract_for_node(&run.definition_snapshot, &node_run.node_id)
    else {
        return false;
    };
    contract.required && artifacts_for_node_run(run, &node_run.id).is_empty()
}

fn finding_count_not_decreasing(run: &WorkflowRunDto, node_run: &WorkflowRunNodeDto) -> bool {
    let Some(current_count) = latest_finding_count_for_node_run(run, &node_run.id) else {
        return false;
    };
    let Some(previous_count) = run
        .nodes
        .iter()
        .filter(|candidate| {
            candidate.node_id == node_run.node_id
                && candidate.attempt_number < node_run.attempt_number
        })
        .max_by_key(|candidate| candidate.attempt_number)
        .and_then(|candidate| latest_finding_count_for_node_run(run, &candidate.id))
    else {
        return false;
    };
    current_count >= previous_count
}

fn latest_finding_count_for_node_run(run: &WorkflowRunDto, node_run_id: &str) -> Option<f64> {
    artifacts_for_node_run(run, node_run_id)
        .into_iter()
        .rev()
        .find_map(|artifact| finding_count_in_value(&artifact.payload))
}

fn artifacts_for_node_run<'a>(
    run: &'a WorkflowRunDto,
    node_run_id: &str,
) -> Vec<&'a crate::commands::contracts::workflows::WorkflowArtifactRecordDto> {
    run.artifacts
        .iter()
        .filter(|artifact| artifact.producer_node_run_id == node_run_id)
        .collect()
}

fn finding_count_in_value(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Object(map) => {
            for key in [
                "high_count",
                "highCount",
                "finding_count",
                "findingCount",
                "findings_count",
                "findingsCount",
                "gap_count",
                "gapCount",
                "gaps_count",
                "gapsCount",
            ] {
                if let Some(count) = map.get(key).and_then(JsonValue::as_f64) {
                    return Some(count);
                }
            }
            map.values().find_map(finding_count_in_value)
        }
        JsonValue::Array(items) => items.iter().find_map(finding_count_in_value),
        _ => None,
    }
}

fn condition_context(run: &WorkflowRunDto) -> WorkflowConditionContext {
    let mut context = WorkflowConditionContext::default();
    let mut node_id_by_run_id = BTreeMap::new();
    for node in &run.nodes {
        context
            .node_statuses
            .insert(node.node_id.clone(), node.status);
        if let Some(failure_class) = node.failure_class.as_ref() {
            context
                .failure_classes
                .insert(node.node_id.clone(), failure_class.clone());
            context.latest_failure_class = Some(failure_class.clone());
        }
        node_id_by_run_id.insert(node.id.clone(), node.node_id.clone());
    }
    for artifact in &run.artifacts {
        if let Some(node_id) = node_id_by_run_id.get(&artifact.producer_node_run_id) {
            let reference = format!("{node_id}.{}", artifact.artifact_type);
            context
                .artifacts
                .insert(reference.clone(), artifact.payload.clone());
            if artifact.artifact_type.starts_with("state_")
                || artifact.artifact_type == "collection_item"
            {
                context
                    .state_values
                    .insert(reference, artifact.payload.clone());
            }
        }
    }
    for attempt in &run.loop_attempts {
        context
            .loop_attempts
            .insert(attempt.loop_key.clone(), attempt.attempt_count);
    }
    for decision in &run.gate_decisions {
        if let Some(node_id) = node_id_by_run_id.get(&decision.node_run_id) {
            context
                .human_decisions
                .insert(node_id.clone(), decision.decision.clone());
        }
    }
    context
}

fn encode_workflow_condition(
    condition: &crate::commands::contracts::workflows::WorkflowConditionDto,
) -> CommandResult<JsonValue> {
    serde_json::to_value(condition).map_err(|error| {
        CommandError::system_fault(
            "workflow_condition_encode_failed",
            format!("Xero could not encode Workflow condition: {error}"),
        )
    })
}

fn had_prior_unsuccessful_attempt(run: &WorkflowRunDto, node_run: &WorkflowRunNodeDto) -> bool {
    run.nodes.iter().any(|candidate| {
        candidate.node_id == node_run.node_id
            && candidate.attempt_number < node_run.attempt_number
            && (is_failed_status(candidate.status)
                || candidate.status == WorkflowNodeRunStatusDto::Skipped)
    })
}

fn insert_workflow_metric_event(
    repo_root: &std::path::Path,
    project_id: &str,
    run_id: &str,
    node_run_id: Option<&str>,
    metric: &str,
    fields: &JsonValue,
) -> CommandResult<()> {
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        run_id,
        node_run_id,
        "workflow_metric_recorded",
        &json!({
            "metric": metric,
            "fields": fields,
        }),
    )
}

fn output_contract_for_node<'a>(
    definition: &'a WorkflowDefinitionDto,
    node_id: &str,
) -> Option<&'a WorkflowOutputContractDto> {
    find_node(definition, node_id).and_then(WorkflowNodeDto::output_contract)
}

fn artifact_schema_for_output<'a>(
    definition: &'a WorkflowDefinitionDto,
    output_contract: &WorkflowOutputContractDto,
) -> Option<&'a JsonValue> {
    definition
        .artifact_contracts
        .iter()
        .find(|contract| {
            contract.artifact_type == output_contract.artifact_type
                && contract.schema_version == output_contract.schema_version
        })
        .and_then(|contract| contract.json_schema.as_ref())
}

#[derive(Debug, Clone)]
struct SubgraphNodeContext<'a> {
    parent_node_id: String,
    local_node_id: String,
    parent_node: &'a WorkflowNodeDto,
    subgraph: &'a WorkflowSubgraphDto,
}

fn find_node<'a>(
    definition: &'a WorkflowDefinitionDto,
    node_id: &str,
) -> Option<&'a WorkflowNodeDto> {
    if let Some(node) = definition.nodes.iter().find(|node| node.id() == node_id) {
        return Some(node);
    }
    let context = subgraph_context_for_node_id(definition, node_id)?;
    context
        .subgraph
        .nodes
        .iter()
        .find(|node| node.id() == context.local_node_id.as_str())
}

fn find_subgraph<'a>(
    definition: &'a WorkflowDefinitionDto,
    subgraph_id: &str,
) -> Option<&'a WorkflowSubgraphDto> {
    definition
        .subgraphs
        .iter()
        .find(|subgraph| subgraph.id == subgraph_id)
}

fn subgraph_context_for_node_id<'a>(
    definition: &'a WorkflowDefinitionDto,
    node_id: &str,
) -> Option<SubgraphNodeContext<'a>> {
    let (parent_node_id, local_node_id) = node_id.rsplit_once(SUBGRAPH_NODE_SEPARATOR)?;
    let parent_node = find_node(definition, parent_node_id)?;
    let WorkflowNodeDto::Subgraph { subgraph_id, .. } = parent_node else {
        return None;
    };
    let subgraph = find_subgraph(definition, subgraph_id)?;
    if !subgraph.nodes.iter().any(|node| node.id() == local_node_id) {
        return None;
    }
    Some(SubgraphNodeContext {
        parent_node_id: parent_node_id.to_string(),
        local_node_id: local_node_id.to_string(),
        parent_node,
        subgraph,
    })
}

fn namespaced_subgraph_node_id(parent_node_id: &str, local_node_id: &str) -> String {
    format!("{parent_node_id}{SUBGRAPH_NODE_SEPARATOR}{local_node_id}")
}

fn runtime_edges_from_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
) -> Vec<WorkflowEdgeDto> {
    let Some(context) = subgraph_context_for_node_id(definition, node_id) else {
        return definition
            .edges
            .iter()
            .filter(|edge| edge.from_node_id == node_id)
            .cloned()
            .collect();
    };
    context
        .subgraph
        .edges
        .iter()
        .filter(|edge| edge.from_node_id == context.local_node_id.as_str())
        .map(|edge| namespace_subgraph_edge(edge, &context))
        .collect()
}

fn runtime_incoming_source_ids(definition: &WorkflowDefinitionDto, node_id: &str) -> Vec<String> {
    let Some(context) = subgraph_context_for_node_id(definition, node_id) else {
        return definition
            .edges
            .iter()
            .filter(|edge| edge.to_node_id == node_id)
            .map(|edge| edge.from_node_id.clone())
            .collect();
    };
    context
        .subgraph
        .edges
        .iter()
        .filter(|edge| edge.to_node_id == context.local_node_id.as_str())
        .map(|edge| namespaced_subgraph_node_id(&context.parent_node_id, &edge.from_node_id))
        .collect()
}

fn namespace_subgraph_edge(
    edge: &WorkflowEdgeDto,
    context: &SubgraphNodeContext<'_>,
) -> WorkflowEdgeDto {
    let mut edge = edge.clone();
    edge.id = format!("{}{}", context.parent_node_id, SUBGRAPH_NODE_SEPARATOR) + &edge.id;
    edge.from_node_id = namespaced_subgraph_node_id(&context.parent_node_id, &edge.from_node_id);
    edge.to_node_id =
        namespace_node_ref(&context.parent_node_id, context.subgraph, &edge.to_node_id);
    edge.condition =
        namespace_condition(&edge.condition, &context.parent_node_id, context.subgraph);
    if let Some(policy) = edge.loop_policy.as_mut() {
        policy.loop_key = namespace_loop_key(&context.parent_node_id, &policy.loop_key);
        policy.on_exhausted = namespace_node_ref(
            &context.parent_node_id,
            context.subgraph,
            &policy.on_exhausted,
        );
        policy.selected_artifact_refs = policy
            .selected_artifact_refs
            .iter()
            .map(|artifact_ref| {
                namespace_artifact_ref(&context.parent_node_id, context.subgraph, artifact_ref)
            })
            .collect();
    }
    edge
}

fn namespace_condition(
    condition: &WorkflowConditionDto,
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
) -> WorkflowConditionDto {
    match condition {
        WorkflowConditionDto::Always => WorkflowConditionDto::Always,
        WorkflowConditionDto::All { conditions } => WorkflowConditionDto::All {
            conditions: conditions
                .iter()
                .map(|condition| namespace_condition(condition, parent_node_id, subgraph))
                .collect(),
        },
        WorkflowConditionDto::Any { conditions } => WorkflowConditionDto::Any {
            conditions: conditions
                .iter()
                .map(|condition| namespace_condition(condition, parent_node_id, subgraph))
                .collect(),
        },
        WorkflowConditionDto::Not { condition } => WorkflowConditionDto::Not {
            condition: Box::new(namespace_condition(condition, parent_node_id, subgraph)),
        },
        WorkflowConditionDto::NodeStatus { node_id, status } => WorkflowConditionDto::NodeStatus {
            node_id: namespace_node_ref(parent_node_id, subgraph, node_id),
            status: *status,
        },
        WorkflowConditionDto::ArtifactExists { artifact_ref } => {
            WorkflowConditionDto::ArtifactExists {
                artifact_ref: namespace_artifact_ref(parent_node_id, subgraph, artifact_ref),
            }
        }
        WorkflowConditionDto::ArtifactFieldEquals {
            artifact_ref,
            path,
            value,
        } => WorkflowConditionDto::ArtifactFieldEquals {
            artifact_ref: namespace_artifact_ref(parent_node_id, subgraph, artifact_ref),
            path: path.clone(),
            value: value.clone(),
        },
        WorkflowConditionDto::ArtifactFieldIn {
            artifact_ref,
            path,
            values,
        } => WorkflowConditionDto::ArtifactFieldIn {
            artifact_ref: namespace_artifact_ref(parent_node_id, subgraph, artifact_ref),
            path: path.clone(),
            values: values.clone(),
        },
        WorkflowConditionDto::ArtifactFieldNumberCompare {
            artifact_ref,
            path,
            operator,
            value,
        } => WorkflowConditionDto::ArtifactFieldNumberCompare {
            artifact_ref: namespace_artifact_ref(parent_node_id, subgraph, artifact_ref),
            path: path.clone(),
            operator: *operator,
            value: *value,
        },
        WorkflowConditionDto::FailureClassIs {
            node_id,
            failure_class,
        } => WorkflowConditionDto::FailureClassIs {
            node_id: node_id
                .as_deref()
                .map(|node_id| namespace_node_ref(parent_node_id, subgraph, node_id)),
            failure_class: failure_class.clone(),
        },
        WorkflowConditionDto::LoopAttemptLt { loop_key, value } => {
            WorkflowConditionDto::LoopAttemptLt {
                loop_key: namespace_loop_key(parent_node_id, loop_key),
                value: *value,
            }
        }
        WorkflowConditionDto::LoopAttemptGte { loop_key, value } => {
            WorkflowConditionDto::LoopAttemptGte {
                loop_key: namespace_loop_key(parent_node_id, loop_key),
                value: *value,
            }
        }
        WorkflowConditionDto::HumanDecisionIs {
            checkpoint_node_id,
            decision,
        } => WorkflowConditionDto::HumanDecisionIs {
            checkpoint_node_id: namespace_node_ref(parent_node_id, subgraph, checkpoint_node_id),
            decision: decision.clone(),
        },
        WorkflowConditionDto::StateFieldEquals {
            state_ref,
            path,
            value,
        } => WorkflowConditionDto::StateFieldEquals {
            state_ref: namespace_artifact_ref(parent_node_id, subgraph, state_ref),
            path: path.clone(),
            value: value.clone(),
        },
        WorkflowConditionDto::StateCollectionCountCompare {
            state_ref,
            operator,
            value,
        } => WorkflowConditionDto::StateCollectionCountCompare {
            state_ref: namespace_artifact_ref(parent_node_id, subgraph, state_ref),
            operator: *operator,
            value: *value,
        },
    }
}

fn next_attempt_for_node(run: &WorkflowRunDto, node_id: &str) -> u32 {
    run.nodes
        .iter()
        .filter(|node| node.node_id == node_id)
        .map(|node| node.attempt_number)
        .max()
        .map(|attempt| attempt.saturating_add(1))
        .unwrap_or(0)
}

fn checkpoint_type_for_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
) -> Option<WorkflowHumanCheckpointTypeDto> {
    match find_node(definition, node_id)? {
        WorkflowNodeDto::HumanCheckpoint {
            checkpoint_type, ..
        } => Some(*checkpoint_type),
        _ => None,
    }
}

fn pause_at_checkpoint(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    reason: &str,
) -> CommandResult<()> {
    project_store::update_workflow_run_node(
        repo_root,
        project_id,
        &node_run.id,
        WorkflowNodeRunStatusDto::WaitingOnGate,
        None,
        None,
        None,
    )?;
    project_store::update_workflow_run_status(
        repo_root,
        project_id,
        &run.id,
        WorkflowRunStatusDto::Paused,
        Some(WorkflowTerminalStatusDto::NeedsHuman),
        None,
    )?;
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        "workflow_paused",
        &json!({ "reason": reason, "nodeId": node_run.node_id }),
    )?;
    insert_workflow_metric_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        "checkpoint_pause",
        &json!({
            "reason": reason,
            "nodeId": node_run.node_id,
        }),
    )
}

fn complete_for_terminal(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    terminal_status: WorkflowTerminalStatusDto,
) -> CommandResult<()> {
    let run_status = match terminal_status {
        WorkflowTerminalStatusDto::Success => WorkflowRunStatusDto::Completed,
        WorkflowTerminalStatusDto::Failure => WorkflowRunStatusDto::Failed,
        WorkflowTerminalStatusDto::Cancelled => WorkflowRunStatusDto::Cancelled,
        WorkflowTerminalStatusDto::NeedsHuman => WorkflowRunStatusDto::Paused,
    };
    project_store::update_workflow_run_status(
        repo_root,
        project_id,
        &run.id,
        run_status,
        Some(terminal_status),
        None,
    )?;
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        None,
        "workflow_completed",
        &json!({ "terminalStatus": terminal_status.as_str() }),
    )
}

fn is_terminal_run(status: WorkflowRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowRunStatusDto::Completed
            | WorkflowRunStatusDto::Failed
            | WorkflowRunStatusDto::Cancelled
    )
}

fn edge_applies_to_node_status(
    edge_type: WorkflowEdgeTypeDto,
    node_status: WorkflowNodeRunStatusDto,
) -> bool {
    match edge_type {
        WorkflowEdgeTypeDto::Success => node_status == WorkflowNodeRunStatusDto::Succeeded,
        WorkflowEdgeTypeDto::Failure => matches!(
            node_status,
            WorkflowNodeRunStatusDto::Failed
                | WorkflowNodeRunStatusDto::Stalled
                | WorkflowNodeRunStatusDto::Cancelled
        ),
        WorkflowEdgeTypeDto::Recovery => matches!(
            node_status,
            WorkflowNodeRunStatusDto::Failed | WorkflowNodeRunStatusDto::Stalled
        ),
        WorkflowEdgeTypeDto::Conditional
        | WorkflowEdgeTypeDto::Loop
        | WorkflowEdgeTypeDto::ManualOverride => true,
    }
}

fn routes_single_match(node: &WorkflowNodeDto) -> bool {
    matches!(
        node,
        WorkflowNodeDto::Router { .. }
            | WorkflowNodeDto::Gate { .. }
            | WorkflowNodeDto::HumanCheckpoint { .. }
    )
}

fn has_routed_node_run(run: &WorkflowRunDto, node_run: &WorkflowRunNodeDto) -> bool {
    if has_control_event_after_completion(run, node_run, "workflow_node_retry_requested") {
        return true;
    }
    run.edge_decisions.iter().any(|decision| {
        if decision.from_node_id != node_run.node_id {
            return false;
        }
        node_run
            .completed_at
            .as_ref()
            .map(|completed_at| decision.created_at >= *completed_at)
            .unwrap_or(true)
    })
}

fn has_node_event(run: &WorkflowRunDto, node_run: &WorkflowRunNodeDto, event_type: &str) -> bool {
    run.events.iter().any(|event| {
        event.node_run_id.as_deref() == Some(node_run.id.as_str()) && event.event_type == event_type
    })
}

fn has_metric_event_for_node(
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    metric: &str,
) -> bool {
    run.events.iter().any(|event| {
        event.node_run_id.as_deref() == Some(node_run.id.as_str())
            && event.event_type == "workflow_metric_recorded"
            && event.event.get("metric").and_then(JsonValue::as_str) == Some(metric)
    })
}

fn has_control_event_after_completion(
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    event_type: &str,
) -> bool {
    run.events.iter().any(|event| {
        event.node_run_id.as_deref() == Some(node_run.id.as_str())
            && event.event_type == event_type
            && node_run
                .completed_at
                .as_ref()
                .map(|completed_at| event.created_at >= *completed_at)
                .unwrap_or(true)
    })
}

fn is_retryable_node_status(status: WorkflowNodeRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowNodeRunStatusDto::Failed
            | WorkflowNodeRunStatusDto::Stalled
            | WorkflowNodeRunStatusDto::Skipped
            | WorkflowNodeRunStatusDto::Cancelled
    )
}

fn is_skippable_node_status(status: WorkflowNodeRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowNodeRunStatusDto::Pending
            | WorkflowNodeRunStatusDto::Eligible
            | WorkflowNodeRunStatusDto::Starting
            | WorkflowNodeRunStatusDto::Running
            | WorkflowNodeRunStatusDto::WaitingOnGate
    )
}

#[cfg(test)]
#[derive(Debug, Default, PartialEq, Eq)]
struct WorkflowEventReplaySummary {
    edge_evaluations: usize,
    node_start_requests: usize,
    resource_conflict_waits: usize,
    loop_exhaustions: usize,
    checkpoint_pauses: usize,
    recovery_successes: usize,
}

#[cfg(test)]
fn replay_workflow_events(run: &WorkflowRunDto) -> WorkflowEventReplaySummary {
    let mut summary = WorkflowEventReplaySummary::default();
    for event in &run.events {
        match event.event_type.as_str() {
            "workflow_edge_evaluated" => summary.edge_evaluations += 1,
            "workflow_agent_start_requested" => summary.node_start_requests += 1,
            "workflow_resource_conflict_wait" => summary.resource_conflict_waits += 1,
            "workflow_metric_recorded" => {
                match event.event.get("metric").and_then(JsonValue::as_str) {
                    Some("loop_exhaustion") => summary.loop_exhaustions += 1,
                    Some("checkpoint_pause") => summary.checkpoint_pauses += 1,
                    Some("recovery_success") => summary.recovery_successes += 1,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    summary
}

#[derive(Debug, PartialEq, Eq)]
enum MergeEvaluation {
    Waiting,
    Succeeded,
    Failed(&'static str),
}

fn evaluate_merge_node(
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    wait_policy: WorkflowMergeWaitPolicyDto,
    quorum: Option<u32>,
    fail_fast: bool,
) -> MergeEvaluation {
    let incoming_sources = runtime_incoming_source_ids(&run.definition_snapshot, &node_run.node_id)
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    if incoming_sources.is_empty() {
        return MergeEvaluation::Succeeded;
    }

    let statuses = incoming_sources
        .iter()
        .filter_map(|node_id| latest_status_for_node(run, node_id))
        .collect::<Vec<_>>();
    let finished_count = statuses
        .iter()
        .filter(|status| is_finished_status(**status))
        .count();
    let succeeded_count = statuses
        .iter()
        .filter(|status| **status == WorkflowNodeRunStatusDto::Succeeded)
        .count();
    let skipped_count = statuses
        .iter()
        .filter(|status| **status == WorkflowNodeRunStatusDto::Skipped)
        .count();
    let failed_count = statuses
        .iter()
        .filter(|status| is_failed_status(**status))
        .count();
    let resolved_without_failure_count = succeeded_count + skipped_count;
    let expected_count = incoming_sources.len();

    if fail_fast && failed_count > 0 {
        return MergeEvaluation::Failed("merge_branch_failed");
    }

    match wait_policy {
        WorkflowMergeWaitPolicyDto::Any => {
            if succeeded_count > 0 {
                MergeEvaluation::Succeeded
            } else if finished_count == expected_count {
                MergeEvaluation::Failed("merge_no_successful_branch")
            } else {
                MergeEvaluation::Waiting
            }
        }
        WorkflowMergeWaitPolicyDto::Quorum => {
            let required = quorum.unwrap_or(expected_count as u32).max(1) as usize;
            if succeeded_count >= required {
                MergeEvaluation::Succeeded
            } else if finished_count == expected_count {
                MergeEvaluation::Failed("merge_quorum_not_met")
            } else {
                MergeEvaluation::Waiting
            }
        }
        WorkflowMergeWaitPolicyDto::FailFast => {
            if failed_count > 0 {
                MergeEvaluation::Failed("merge_branch_failed")
            } else if resolved_without_failure_count == expected_count && succeeded_count > 0 {
                MergeEvaluation::Succeeded
            } else if finished_count == expected_count {
                MergeEvaluation::Failed("merge_no_successful_branch")
            } else {
                MergeEvaluation::Waiting
            }
        }
        WorkflowMergeWaitPolicyDto::All => {
            if failed_count > 0 && finished_count == expected_count {
                MergeEvaluation::Failed("merge_branch_failed")
            } else if resolved_without_failure_count == expected_count && succeeded_count > 0 {
                MergeEvaluation::Succeeded
            } else if finished_count == expected_count {
                MergeEvaluation::Failed("merge_no_successful_branch")
            } else {
                MergeEvaluation::Waiting
            }
        }
    }
}

fn latest_status_for_node(run: &WorkflowRunDto, node_id: &str) -> Option<WorkflowNodeRunStatusDto> {
    run.nodes
        .iter()
        .filter(|node| node.node_id == node_id)
        .max_by_key(|node| node.attempt_number)
        .map(|node| node.status)
}

fn is_finished_status(status: WorkflowNodeRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowNodeRunStatusDto::Succeeded
            | WorkflowNodeRunStatusDto::Failed
            | WorkflowNodeRunStatusDto::Stalled
            | WorkflowNodeRunStatusDto::Skipped
            | WorkflowNodeRunStatusDto::Cancelled
    )
}

fn is_failed_status(status: WorkflowNodeRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowNodeRunStatusDto::Failed
            | WorkflowNodeRunStatusDto::Stalled
            | WorkflowNodeRunStatusDto::Cancelled
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commands::contracts::{
            runtime::RuntimeAgentIdDto,
            workflows::{
                WorkflowArtifactRecordDto, WorkflowCollectionLoopControlsDto, WorkflowConditionDto,
                WorkflowEventDto, WorkflowFailureClassificationPolicyDto, WorkflowLoopPolicyDto,
                WorkflowResourceConflictModeDto, WorkflowResourceConflictPolicyDto,
                WorkflowRunPolicyDto, WorkflowStallDetectorDto,
            },
        },
        db::{
            configure_connection, migrations::migrations, project_store,
            register_project_database_path_for_tests,
        },
    };
    use rusqlite::Connection;
    use tempfile::TempDir;

    const NOW: &str = "2026-01-01T00:00:00Z";

    fn terminal_node(id: &str) -> WorkflowNodeDto {
        WorkflowNodeDto::Terminal {
            id: id.into(),
            title: id.into(),
            description: String::new(),
            position: Default::default(),
            terminal_status: WorkflowTerminalStatusDto::Success,
        }
    }

    fn agent_node(id: &str, resource_scopes: Vec<String>) -> WorkflowNodeDto {
        WorkflowNodeDto::Agent {
            id: id.into(),
            title: id.into(),
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
            resource_scopes,
            failure_policy: Default::default(),
        }
    }

    fn merge_node() -> WorkflowNodeDto {
        WorkflowNodeDto::Merge {
            id: "merge".into(),
            title: "Merge".into(),
            description: String::new(),
            position: Default::default(),
            wait_policy: WorkflowMergeWaitPolicyDto::All,
            quorum: None,
            fail_fast: false,
        }
    }

    fn edge(
        id: &str,
        from_node_id: &str,
        to_node_id: &str,
        edge_type: WorkflowEdgeTypeDto,
    ) -> WorkflowEdgeDto {
        WorkflowEdgeDto {
            id: id.into(),
            from_node_id: from_node_id.into(),
            to_node_id: to_node_id.into(),
            r#type: edge_type,
            label: String::new(),
            priority: 10,
            condition: WorkflowConditionDto::Always,
            loop_policy: None,
        }
    }

    fn definition_with_edges(edges: Vec<WorkflowEdgeDto>) -> WorkflowDefinitionDto {
        WorkflowDefinitionDto {
            schema: "xero.workflow_definition.v1".into(),
            id: "workflow-1".into(),
            project_id: "project-1".into(),
            name: "Workflow".into(),
            description: String::new(),
            version: 1,
            start_node_id: "source-a".into(),
            nodes: vec![
                terminal_node("source-a"),
                terminal_node("source-b"),
                terminal_node("source-c"),
                merge_node(),
                terminal_node("done"),
            ],
            edges,
            subgraphs: Vec::new(),
            artifact_contracts: Vec::new(),
            run_policy: WorkflowRunPolicyDto::default(),
            created_at: None,
            updated_at: None,
        }
    }

    fn node_run(
        node_id: &str,
        status: WorkflowNodeRunStatusDto,
        attempt_number: u32,
    ) -> WorkflowRunNodeDto {
        WorkflowRunNodeDto {
            id: format!("run-1:node:{node_id}:attempt:{attempt_number}"),
            workflow_run_id: "run-1".into(),
            node_id: node_id.into(),
            node_type: if node_id == "merge" {
                "merge".into()
            } else {
                "terminal".into()
            },
            status,
            attempt_number,
            runtime_run_id: None,
            agent_session_id: None,
            failure_class: None,
            started_at: None,
            updated_at: NOW.into(),
            completed_at: is_finished_status(status).then(|| NOW.into()),
            idempotency_key: format!("run-1:{node_id}:{attempt_number}"),
        }
    }

    fn artifact_for_node_run(node_run_id: &str, payload: JsonValue) -> WorkflowArtifactRecordDto {
        WorkflowArtifactRecordDto {
            id: format!("artifact-{node_run_id}"),
            workflow_run_id: "run-1".into(),
            producer_node_run_id: node_run_id.into(),
            artifact_type: "review_findings".into(),
            schema_version: 1,
            payload,
            render_text: None,
            created_at: NOW.into(),
        }
    }

    fn workflow_event(event_type: &str, event: JsonValue) -> WorkflowEventDto {
        WorkflowEventDto {
            id: format!("event-{event_type}"),
            workflow_run_id: "run-1".into(),
            node_run_id: None,
            event_type: event_type.into(),
            event,
            created_at: NOW.into(),
        }
    }

    fn run_with_nodes(
        edges: Vec<WorkflowEdgeDto>,
        nodes: Vec<WorkflowRunNodeDto>,
    ) -> WorkflowRunDto {
        run_with_definition(definition_with_edges(edges), nodes)
    }

    fn run_with_definition(
        definition: WorkflowDefinitionDto,
        nodes: Vec<WorkflowRunNodeDto>,
    ) -> WorkflowRunDto {
        WorkflowRunDto {
            id: "run-1".into(),
            project_id: "project-1".into(),
            workflow_version_id: "workflow-version-1".into(),
            workflow_id: definition.id.clone(),
            workflow_version_number: 1,
            status: WorkflowRunStatusDto::Running,
            terminal_status: None,
            definition_snapshot: definition,
            initial_input: None,
            started_at: NOW.into(),
            updated_at: NOW.into(),
            completed_at: None,
            cancellation_reason: None,
            nodes,
            edge_decisions: Vec::new(),
            artifacts: Vec::new(),
            gate_decisions: Vec::new(),
            loop_attempts: Vec::new(),
            events: Vec::new(),
        }
    }

    fn run_for_merge(
        sources: &[&str],
        node_statuses: Vec<(&str, WorkflowNodeRunStatusDto)>,
    ) -> (WorkflowRunDto, WorkflowRunNodeDto) {
        let merge_run = node_run("merge", WorkflowNodeRunStatusDto::Eligible, 0);
        let mut nodes = node_statuses
            .into_iter()
            .map(|(node_id, status)| node_run(node_id, status, 0))
            .collect::<Vec<_>>();
        nodes.push(merge_run.clone());

        let edges = sources
            .iter()
            .map(|source| {
                edge(
                    &format!("edge-{source}-merge"),
                    source,
                    "merge",
                    WorkflowEdgeTypeDto::Success,
                )
            })
            .collect::<Vec<_>>();

        (run_with_nodes(edges, nodes), merge_run)
    }

    #[test]
    fn merge_all_waits_until_every_incoming_source_finishes_successfully() {
        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Running),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Waiting
        );

        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Succeeded),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Succeeded
        );

        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Failed),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Failed("merge_branch_failed")
        );
    }

    #[test]
    fn merge_any_succeeds_on_first_successful_branch() {
        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Running),
            ],
        );

        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::Any,
                None,
                false,
            ),
            MergeEvaluation::Succeeded
        );
    }

    #[test]
    fn merge_all_treats_skipped_branches_as_resolved_not_successful() {
        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Skipped),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Succeeded
        );

        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Skipped),
                ("source-b", WorkflowNodeRunStatusDto::Skipped),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Failed("merge_no_successful_branch")
        );
    }

    #[test]
    fn resource_conflict_policy_serializes_declared_scopes() {
        let mut definition = definition_with_edges(Vec::new());
        definition.nodes = vec![
            agent_node("agent-a", vec!["repo".into(), "src/lib.rs".into()]),
            agent_node("agent-b", vec!["src/lib.rs".into()]),
        ];
        definition.run_policy.concurrency_limit = 2;
        definition.run_policy.resource_conflict_policy = WorkflowResourceConflictPolicyDto {
            mode: WorkflowResourceConflictModeDto::SerializeConflicts,
            default_scopes: Vec::new(),
        };
        let eligible = node_run("agent-b", WorkflowNodeRunStatusDto::Eligible, 0);
        let run = run_with_definition(
            definition.clone(),
            vec![
                node_run("agent-a", WorkflowNodeRunStatusDto::Running, 0),
                eligible.clone(),
            ],
        );
        let conflict = resource_conflict_for_node(
            &run,
            &eligible,
            find_node(&definition, "agent-b").expect("agent-b exists"),
        )
        .expect("conflict exists");

        assert_eq!(conflict.node_id, "agent-a");
        assert_eq!(conflict.scopes, vec!["src/lib.rs".to_string()]);

        let mut allowed_definition = definition;
        allowed_definition.run_policy.resource_conflict_policy.mode =
            WorkflowResourceConflictModeDto::AllowConflicts;
        let allowed_run = run_with_definition(
            allowed_definition.clone(),
            vec![
                node_run("agent-a", WorkflowNodeRunStatusDto::Running, 0),
                eligible.clone(),
            ],
        );
        assert!(resource_conflict_for_node(
            &allowed_run,
            &eligible,
            find_node(&allowed_definition, "agent-b").expect("agent-b exists"),
        )
        .is_none());
    }

    #[test]
    fn merge_quorum_requires_configured_success_count() {
        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b", "source-c"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Succeeded),
                ("source-c", WorkflowNodeRunStatusDto::Running),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::Quorum,
                Some(2),
                false,
            ),
            MergeEvaluation::Succeeded
        );

        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b", "source-c"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Failed),
                ("source-c", WorkflowNodeRunStatusDto::Cancelled),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::Quorum,
                Some(2),
                false,
            ),
            MergeEvaluation::Failed("merge_quorum_not_met")
        );
    }

    #[test]
    fn merge_fail_fast_fails_before_all_sources_finish() {
        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Failed),
                ("source-b", WorkflowNodeRunStatusDto::Running),
            ],
        );

        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                true,
            ),
            MergeEvaluation::Failed("merge_branch_failed")
        );
    }

    #[test]
    fn edge_status_routing_matches_terminal_status_semantics() {
        assert!(edge_applies_to_node_status(
            WorkflowEdgeTypeDto::Success,
            WorkflowNodeRunStatusDto::Succeeded
        ));
        assert!(!edge_applies_to_node_status(
            WorkflowEdgeTypeDto::Success,
            WorkflowNodeRunStatusDto::Failed
        ));

        for status in [
            WorkflowNodeRunStatusDto::Failed,
            WorkflowNodeRunStatusDto::Stalled,
            WorkflowNodeRunStatusDto::Cancelled,
        ] {
            assert!(edge_applies_to_node_status(
                WorkflowEdgeTypeDto::Failure,
                status
            ));
        }
        assert!(!edge_applies_to_node_status(
            WorkflowEdgeTypeDto::Failure,
            WorkflowNodeRunStatusDto::Succeeded
        ));

        assert!(edge_applies_to_node_status(
            WorkflowEdgeTypeDto::Recovery,
            WorkflowNodeRunStatusDto::Failed
        ));
        assert!(edge_applies_to_node_status(
            WorkflowEdgeTypeDto::Recovery,
            WorkflowNodeRunStatusDto::Stalled
        ));
        assert!(!edge_applies_to_node_status(
            WorkflowEdgeTypeDto::Recovery,
            WorkflowNodeRunStatusDto::Cancelled
        ));

        for edge_type in [
            WorkflowEdgeTypeDto::Conditional,
            WorkflowEdgeTypeDto::Loop,
            WorkflowEdgeTypeDto::ManualOverride,
        ] {
            assert!(edge_applies_to_node_status(
                edge_type,
                WorkflowNodeRunStatusDto::Pending
            ));
        }
    }

    #[test]
    fn activity_timeout_prefers_agent_policy_over_run_policy() {
        let mut definition = definition_with_edges(Vec::new());
        definition.run_policy.node_timeout_seconds = Some(60);
        definition.nodes.push(WorkflowNodeDto::Agent {
            id: "agent".into(),
            title: "Agent".into(),
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
            failure_policy: WorkflowFailureClassificationPolicyDto {
                runtime_activity_timeout_seconds: Some(5),
                ..WorkflowFailureClassificationPolicyDto::default()
            },
        });

        assert_eq!(
            activity_timeout_seconds_for_node(&definition, "agent"),
            Some(5)
        );
        assert_eq!(
            activity_timeout_seconds_for_node(&definition, "source-a"),
            None
        );
    }

    #[test]
    fn stale_agent_activity_uses_latest_runtime_activity_timestamp() {
        let now = OffsetDateTime::parse("2026-01-01T00:10:00Z", &Rfc3339).expect("parse now");
        let recent_heartbeat = agent_run_record(
            "2026-01-01T00:00:00Z",
            Some("2026-01-01T00:09:00Z"),
            "2026-01-01T00:01:00Z",
        );
        assert_eq!(stale_agent_activity_at(&recent_heartbeat, 120, now), None);

        let stale_heartbeat = agent_run_record(
            "2026-01-01T00:00:00Z",
            Some("2026-01-01T00:07:00Z"),
            "2026-01-01T00:01:00Z",
        );
        assert_eq!(
            stale_agent_activity_at(&stale_heartbeat, 120, now),
            Some("2026-01-01T00:07:00Z")
        );
    }

    #[test]
    fn stall_detectors_classify_repeated_failures_missing_artifacts_and_flat_findings() {
        let first_failed = WorkflowRunNodeDto {
            failure_class: Some("tool_retry_limit".into()),
            ..node_run("source-a", WorkflowNodeRunStatusDto::Failed, 0)
        };
        let repeated_failed = WorkflowRunNodeDto {
            failure_class: Some("tool_retry_limit".into()),
            ..node_run("source-a", WorkflowNodeRunStatusDto::Failed, 1)
        };
        let run = run_with_nodes(Vec::new(), vec![first_failed, repeated_failed.clone()]);
        assert_eq!(
            stall_failure_class_for_detector(
                &run,
                &repeated_failed,
                WorkflowStallDetectorDto::SameFailureClassRepeated,
            ),
            Some("same_failure_class_repeated")
        );

        let missing_artifact = node_run("agent", WorkflowNodeRunStatusDto::Succeeded, 0);
        let mut definition = definition_with_edges(Vec::new());
        definition.nodes.push(agent_node("agent", Vec::new()));
        let run = run_with_definition(definition, vec![missing_artifact.clone()]);
        assert_eq!(
            stall_failure_class_for_detector(
                &run,
                &missing_artifact,
                WorkflowStallDetectorDto::NoArtifactProgress,
            ),
            Some("no_artifact_progress")
        );

        let previous_review = node_run("source-a", WorkflowNodeRunStatusDto::Succeeded, 0);
        let current_review = node_run("source-a", WorkflowNodeRunStatusDto::Succeeded, 1);
        let mut run = run_with_nodes(
            Vec::new(),
            vec![previous_review.clone(), current_review.clone()],
        );
        run.artifacts = vec![
            artifact_for_node_run(
                &previous_review.id,
                json!({ "findings": { "high_count": 2 } }),
            ),
            artifact_for_node_run(
                &current_review.id,
                json!({ "findings": { "high_count": 2 } }),
            ),
        ];
        assert_eq!(
            stall_failure_class_for_detector(
                &run,
                &current_review,
                WorkflowStallDetectorDto::FindingCountNotDecreasing,
            ),
            Some("finding_count_not_decreasing")
        );
    }

    fn agent_run_record(
        started_at: &str,
        last_heartbeat_at: Option<&str>,
        updated_at: &str,
    ) -> AgentRunRecord {
        AgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: "agent-definition".into(),
            agent_definition_version: 1,
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "runtime-run-1".into(),
            trace_id: "trace-1".into(),
            lineage_kind: "root".into(),
            parent_run_id: None,
            parent_trace_id: None,
            parent_subagent_id: None,
            subagent_role: None,
            provider_id: "provider".into(),
            model_id: "model".into(),
            status: AgentRunStatus::Running,
            prompt: "prompt".into(),
            system_prompt: "system".into(),
            started_at: started_at.into(),
            last_heartbeat_at: last_heartbeat_at.map(ToOwned::to_owned),
            completed_at: None,
            cancelled_at: None,
            last_error: None,
            updated_at: updated_at.into(),
        }
    }

    #[test]
    fn latest_status_for_node_uses_highest_attempt() {
        let merge_run = node_run("merge", WorkflowNodeRunStatusDto::Eligible, 0);
        let run = run_with_nodes(
            vec![edge(
                "edge-source-a-merge",
                "source-a",
                "merge",
                WorkflowEdgeTypeDto::Success,
            )],
            vec![
                node_run("source-a", WorkflowNodeRunStatusDto::Failed, 0),
                node_run("source-a", WorkflowNodeRunStatusDto::Succeeded, 1),
                merge_run,
            ],
        );

        assert_eq!(
            latest_status_for_node(&run, "source-a"),
            Some(WorkflowNodeRunStatusDto::Succeeded)
        );
    }

    #[test]
    fn event_replay_reconstructs_workflow_observability_counts() {
        let mut run = run_with_nodes(Vec::new(), Vec::new());
        run.events = vec![
            workflow_event("workflow_edge_evaluated", json!({ "matched": true })),
            workflow_event(
                "workflow_agent_start_requested",
                json!({ "nodeId": "agent-a" }),
            ),
            workflow_event(
                "workflow_resource_conflict_wait",
                json!({ "nodeId": "agent-b" }),
            ),
            workflow_event(
                "workflow_metric_recorded",
                json!({ "metric": "loop_exhaustion" }),
            ),
            workflow_event(
                "workflow_metric_recorded",
                json!({ "metric": "checkpoint_pause" }),
            ),
            workflow_event(
                "workflow_metric_recorded",
                json!({ "metric": "recovery_success" }),
            ),
        ];

        assert_eq!(
            replay_workflow_events(&run),
            WorkflowEventReplaySummary {
                edge_evaluations: 1,
                node_start_requests: 1,
                resource_conflict_waits: 1,
                loop_exhaustions: 1,
                checkpoint_pauses: 1,
                recovery_successes: 1,
            }
        );
    }

    fn subgraph_output_contract() -> WorkflowOutputContractDto {
        WorkflowOutputContractDto {
            artifact_type: "subgraph_result".into(),
            schema_version: 1,
            extraction: WorkflowOutputExtractionDto::JsonObject,
            required: true,
            render_text_path: Some("$.summary".into()),
        }
    }

    fn subgraph_invocation_node(id: &str) -> WorkflowNodeDto {
        WorkflowNodeDto::Subgraph {
            id: id.into(),
            title: "Invoke subgraph".into(),
            description: String::new(),
            position: Default::default(),
            subgraph_id: "phase_flow".into(),
            input_bindings: vec![WorkflowInputBindingDto::RunInput {
                name: "goal".into(),
                required: true,
                path: Some("$.goal".into()),
                prompt_label: Some("Goal".into()),
            }],
            output_contract: subgraph_output_contract(),
        }
    }

    fn definition_with_subgraph(subgraph_edges: Vec<WorkflowEdgeDto>) -> WorkflowDefinitionDto {
        WorkflowDefinitionDto {
            schema: "xero.workflow_definition.v1".into(),
            id: "workflow-subgraph".into(),
            project_id: "project-1".into(),
            name: "Subgraph Workflow".into(),
            description: String::new(),
            version: 1,
            start_node_id: "invoke".into(),
            nodes: vec![subgraph_invocation_node("invoke"), terminal_node("done")],
            edges: vec![edge(
                "invoke-to-done",
                "invoke",
                "done",
                WorkflowEdgeTypeDto::Success,
            )],
            subgraphs: vec![WorkflowSubgraphDto {
                id: "phase_flow".into(),
                title: "Phase flow".into(),
                description: String::new(),
                start_node_id: "local_done".into(),
                nodes: vec![
                    terminal_node("producer"),
                    WorkflowNodeDto::Router {
                        id: "router".into(),
                        title: "Router".into(),
                        description: String::new(),
                        position: Default::default(),
                    },
                    terminal_node("local_done"),
                ],
                edges: subgraph_edges,
                input_bindings: vec![WorkflowInputBindingDto::RunInput {
                    name: "goal".into(),
                    required: true,
                    path: Some("$.goal".into()),
                    prompt_label: Some("Goal".into()),
                }],
                output_contract: subgraph_output_contract(),
            }],
            artifact_contracts: Vec::new(),
            run_policy: WorkflowRunPolicyDto::default(),
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn subgraph_node_schedules_child_run_and_completes_parent_from_local_terminal() {
        let temp = repo_with_database();
        let definition = definition_with_subgraph(Vec::new());
        let created = project_store::create_workflow_definition(temp.path(), &definition)
            .expect("create workflow");
        let run = project_store::create_workflow_run(
            temp.path(),
            "project-1",
            &created.id,
            Some(json!({ "goal": "ship subgraphs" })),
        )
        .expect("create run");
        let parent_run = project_store::insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "invoke",
            "subgraph",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "run-1:invoke:0",
        )
        .expect("insert parent node run");
        let loaded_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let WorkflowNodeDto::Subgraph {
            subgraph_id,
            input_bindings,
            output_contract,
            ..
        } = find_node(&loaded_run.definition_snapshot, "invoke").expect("invoke node")
        else {
            panic!("expected subgraph node");
        };

        run_subgraph_node(
            temp.path(),
            "project-1",
            &loaded_run,
            &parent_run,
            subgraph_id,
            input_bindings,
            output_contract,
        )
        .expect("start subgraph");

        let running_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("reload running run")
            .expect("run exists");
        let parent = running_run
            .nodes
            .iter()
            .find(|node| node.node_id == "invoke")
            .expect("parent exists");
        assert_eq!(parent.status, WorkflowNodeRunStatusDto::Running);
        assert!(running_run.artifacts.iter().any(|artifact| {
            artifact.producer_node_run_id == parent.id
                && artifact.artifact_type == SUBGRAPH_INPUT_ARTIFACT_TYPE
                && artifact.payload.get("goal").and_then(JsonValue::as_str)
                    == Some("ship subgraphs")
        }));

        let child = running_run
            .nodes
            .iter()
            .find(|node| node.node_id == "invoke::local_done")
            .cloned()
            .expect("child local terminal was scheduled");
        assert_eq!(child.status, WorkflowNodeRunStatusDto::Eligible);
        let context =
            subgraph_context_for_node_id(&running_run.definition_snapshot, &child.node_id)
                .expect("subgraph context");
        complete_subgraph_terminal(
            temp.path(),
            "project-1",
            &running_run,
            &child,
            WorkflowTerminalStatusDto::Success,
            &context,
        )
        .expect("complete subgraph");

        let finished_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("reload finished run")
            .expect("run exists");
        let parent = finished_run
            .nodes
            .iter()
            .find(|node| node.node_id == "invoke")
            .expect("parent exists");
        let child = finished_run
            .nodes
            .iter()
            .find(|node| node.node_id == "invoke::local_done")
            .expect("child exists");
        assert_eq!(parent.status, WorkflowNodeRunStatusDto::Succeeded);
        assert_eq!(child.status, WorkflowNodeRunStatusDto::Succeeded);
        let subgraph_artifact = finished_run
            .artifacts
            .iter()
            .find(|artifact| {
                artifact.producer_node_run_id == parent.id
                    && artifact.artifact_type == "subgraph_result"
            })
            .expect("subgraph result artifact");
        assert_eq!(
            subgraph_artifact
                .payload
                .get("status")
                .and_then(JsonValue::as_str),
            Some("succeeded")
        );
        assert_eq!(
            subgraph_artifact
                .payload
                .get("terminalNodeId")
                .and_then(JsonValue::as_str),
            Some("invoke::local_done")
        );
        assert!(finished_run.edge_decisions.iter().any(|decision| {
            decision.from_node_id == "invoke::local_done"
                && decision.to_node_id == "invoke"
                && decision.edge_id == "__subgraph_terminal__"
        }));
    }

    #[test]
    fn subgraph_edges_namespace_local_routes_conditions_and_loop_policy() {
        let mut pass_edge = edge(
            "local-pass",
            "router",
            "local_done",
            WorkflowEdgeTypeDto::Conditional,
        );
        pass_edge.condition = WorkflowConditionDto::ArtifactFieldEquals {
            artifact_ref: "producer.review_findings".into(),
            path: "$.status".into(),
            value: json!("passed"),
        };
        let mut retry_edge = edge(
            "local-retry",
            "router",
            "producer",
            WorkflowEdgeTypeDto::Loop,
        );
        retry_edge.loop_policy = Some(WorkflowLoopPolicyDto {
            loop_key: "review_retry".into(),
            max_attempts: 2,
            attempt_scope: Default::default(),
            carryover_policy: Default::default(),
            selected_artifact_refs: vec!["producer.review_findings".into()],
            reset_policy: Default::default(),
            stall_detector: None,
            on_exhausted: "local_done".into(),
        });
        let definition = definition_with_subgraph(vec![pass_edge, retry_edge]);

        let runtime_edges = runtime_edges_from_node(&definition, "invoke::router");
        let pass_edge = runtime_edges
            .iter()
            .find(|edge| edge.id == "invoke::local-pass")
            .expect("pass edge exists");
        assert_eq!(pass_edge.from_node_id, "invoke::router");
        assert_eq!(pass_edge.to_node_id, "invoke::local_done");
        assert_eq!(
            pass_edge.condition,
            WorkflowConditionDto::ArtifactFieldEquals {
                artifact_ref: "invoke::producer.review_findings".into(),
                path: "$.status".into(),
                value: json!("passed"),
            }
        );

        let retry_edge = runtime_edges
            .iter()
            .find(|edge| edge.id == "invoke::local-retry")
            .expect("retry edge exists");
        let policy = retry_edge.loop_policy.as_ref().expect("loop policy");
        assert_eq!(retry_edge.to_node_id, "invoke::producer");
        assert_eq!(policy.loop_key, "invoke::review_retry");
        assert_eq!(policy.on_exhausted, "invoke::local_done");
        assert_eq!(
            policy.selected_artifact_refs,
            vec!["invoke::producer.review_findings".to_string()]
        );
        assert_eq!(
            runtime_incoming_source_ids(&definition, "invoke::local_done"),
            vec!["invoke::router".to_string()]
        );
    }

    #[test]
    fn collection_control_selection_marks_partial_phase_runs() {
        let controls = WorkflowCollectionLoopControlsDto {
            from_input_path: Some("$.from".into()),
            to_input_path: Some("$.to".into()),
            only_input_path: Some("$.only".into()),
        };

        let selected = collection_control_selection(
            Some(&json!({
                "only": "2,2.1",
                "from": "2",
                "to": "3",
            })),
            &controls,
        );

        assert!(selected.has_selection);
        assert_eq!(selected.only_values, Some(vec![json!("2"), json!("2.1")]));
        assert_eq!(selected.from_value, Some(json!("2")));
        assert_eq!(selected.to_value, Some(json!("3")));

        let unselected = collection_control_selection(Some(&json!({ "goal": "ship" })), &controls);
        assert!(!unselected.has_selection);
    }

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

    #[test]
    fn exhausted_loop_routes_to_fallback_and_records_exhaustion() {
        let temp = repo_with_database();
        let mut retry_edge = edge(
            "edge-retry",
            "source-a",
            "source-b",
            WorkflowEdgeTypeDto::Loop,
        );
        retry_edge.loop_policy = Some(WorkflowLoopPolicyDto {
            loop_key: "retry".into(),
            max_attempts: 1,
            attempt_scope: Default::default(),
            carryover_policy: Default::default(),
            selected_artifact_refs: Vec::new(),
            reset_policy: Default::default(),
            stall_detector: None,
            on_exhausted: "done".into(),
        });

        let created = project_store::create_workflow_definition(
            temp.path(),
            &definition_with_edges(vec![retry_edge.clone()]),
        )
        .expect("create workflow");
        let run = project_store::create_workflow_run(temp.path(), "project-1", &created.id, None)
            .expect("create run");
        let source_node_run = project_store::insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "source-a",
            "terminal",
            0,
            WorkflowNodeRunStatusDto::Succeeded,
            "run-1:source-a:0",
        )
        .expect("insert source node run");
        project_store::increment_workflow_loop_attempt(
            temp.path(),
            "project-1",
            &run.id,
            "retry",
            &source_node_run.id,
            false,
        )
        .expect("seed first loop attempt");
        let loaded_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");

        let target_node_id = loop_target_for_edge(
            temp.path(),
            "project-1",
            &loaded_run,
            &source_node_run,
            &retry_edge,
        )
        .expect("resolve exhausted loop target");

        assert_eq!(target_node_id, "done");
        let reloaded_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("reload run")
            .expect("run exists");
        let retry_attempt = reloaded_run
            .loop_attempts
            .iter()
            .find(|attempt| attempt.loop_key == "retry")
            .expect("retry attempt exists");
        assert_eq!(retry_attempt.attempt_count, 2);
        assert!(retry_attempt.exhausted);
        let replay = replay_workflow_events(&reloaded_run);
        assert_eq!(replay.loop_exhaustions, 1);
    }
}
