use std::path::Path;

use serde_json::Value as JsonValue;
use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        contracts::workflows::{
            CancelWorkflowRunRequestDto, CreateWorkflowDefinitionRequestDto,
            ExplainWorkflowRunBlockerRequestDto, ExportWorkflowDeliveryStateRequestDto,
            ExportWorkflowRunBundleRequestDto, GetWorkflowDefinitionRequestDto,
            GetWorkflowRunRequestDto, ListWorkflowDefinitionsRequestDto,
            ListWorkflowDefinitionsResponseDto, ListWorkflowRunsRequestDto,
            ListWorkflowRunsResponseDto, ReadWorkflowDeliveryStateRequestDto,
            ResumeWorkflowCheckpointRequestDto, ResumeWorkflowNextIncompletePhaseRequestDto,
            RetryWorkflowNodeRunRequestDto, SkipWorkflowBranchRequestDto,
            StartWorkflowRunRequestDto, UpdateWorkflowDefinitionRequestDto,
            WipeWorkflowDeliveryStateRequestDto, WorkflowCollectionLoopControlsDto,
            WorkflowDefinitionDto, WorkflowDefinitionResponseDto,
            WorkflowDeliveryStateEntityTypeDto, WorkflowDeliveryStateResponseDto,
            WorkflowInputBindingDto, WorkflowNodeDto, WorkflowNodeRunStatusDto,
            WorkflowRunBlockerResponseDto, WorkflowRunBundleResponseDto, WorkflowRunDto,
            WorkflowRunResponseDto, WorkflowStateQueryDto, WorkflowValidationReportDto,
            WriteWorkflowDeliveryStateRequestDto,
        },
        runtime_support::resolve_project_root,
        validate_non_empty, CommandError, CommandResult,
    },
    db::project_store,
    runtime::workflow_orchestrator,
    state::DesktopState,
};

#[tauri::command]
pub fn validate_workflow_definition<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CreateWorkflowDefinitionRequestDto,
) -> CommandResult<WorkflowValidationReportDto> {
    let repo_root = resolve_project_root(&app, state.inner(), &request.definition.project_id)?;
    Ok(
        workflow_orchestrator::validate_workflow_definition_with_registry(
            &repo_root,
            &request.definition,
        ),
    )
}

#[tauri::command]
pub fn create_workflow_definition<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CreateWorkflowDefinitionRequestDto,
) -> CommandResult<WorkflowDefinitionResponseDto> {
    let repo_root = resolve_project_root(&app, state.inner(), &request.definition.project_id)?;
    let report = workflow_orchestrator::validate_workflow_definition_with_registry(
        &repo_root,
        &request.definition,
    );
    if matches!(
        report.status,
        crate::commands::contracts::workflows::WorkflowValidationStatusDto::Invalid
    ) {
        return Err(CommandError::user_fixable(
            "workflow_definition_invalid",
            "Xero refused to save the Workflow because the graph has validation errors.",
        ));
    }
    let definition = project_store::create_workflow_definition(&repo_root, &request.definition)?;
    Ok(WorkflowDefinitionResponseDto { definition })
}

#[tauri::command]
pub fn update_workflow_definition<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateWorkflowDefinitionRequestDto,
) -> CommandResult<WorkflowDefinitionResponseDto> {
    validate_non_empty(&request.workflow_id, "workflowId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.definition.project_id)?;
    let report = workflow_orchestrator::validate_workflow_definition_with_registry(
        &repo_root,
        &request.definition,
    );
    if matches!(
        report.status,
        crate::commands::contracts::workflows::WorkflowValidationStatusDto::Invalid
    ) {
        return Err(CommandError::user_fixable(
            "workflow_definition_invalid",
            "Xero refused to save the Workflow because the graph has validation errors.",
        ));
    }
    let definition = project_store::update_workflow_definition(
        &repo_root,
        &request.workflow_id,
        request.expected_version,
        &request.definition,
    )?;
    Ok(WorkflowDefinitionResponseDto { definition })
}

#[tauri::command]
pub fn list_workflow_definitions<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListWorkflowDefinitionsRequestDto,
) -> CommandResult<ListWorkflowDefinitionsResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    Ok(ListWorkflowDefinitionsResponseDto {
        definitions: project_store::list_workflow_definitions(&repo_root, &request.project_id)?,
    })
}

#[tauri::command]
pub fn get_workflow_definition<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetWorkflowDefinitionRequestDto,
) -> CommandResult<WorkflowDefinitionResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.workflow_id, "workflowId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let definition = project_store::get_workflow_definition(
        &repo_root,
        &request.project_id,
        &request.workflow_id,
    )?
    .ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_definition_not_found",
            format!("Xero could not find Workflow `{}`.", request.workflow_id),
        )
    })?;
    Ok(WorkflowDefinitionResponseDto { definition })
}

#[tauri::command]
pub fn start_workflow_run<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StartWorkflowRunRequestDto,
) -> CommandResult<WorkflowRunResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.workflow_id, "workflowId")?;
    validate_non_empty(&request.idempotency_key, "idempotencyKey")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let run = persist_workflow_start_request(&repo_root, &request)?;
    let run = workflow_orchestrator::driver::reconcile_workflow_run(
        &app,
        state.inner(),
        &request.project_id,
        &run.id,
    )?;
    workflow_orchestrator::driver::ensure_workflow_run_driver_if_active(&app, &run);
    Ok(WorkflowRunResponseDto { run })
}

fn persist_workflow_start_request(
    repo_root: &Path,
    request: &StartWorkflowRunRequestDto,
) -> CommandResult<WorkflowRunDto> {
    if let Some(run) = project_store::get_workflow_run_start_replay(
        repo_root,
        &request.project_id,
        &request.workflow_id,
        &request.idempotency_key,
        request.initial_input.as_ref(),
    )? {
        return Ok(run);
    }

    for _ in 0..3 {
        let definition = project_store::get_workflow_definition(
            repo_root,
            &request.project_id,
            &request.workflow_id,
        )?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_definition_not_found",
                format!("Xero could not find Workflow `{}`.", request.workflow_id),
            )
        })?;
        validate_workflow_initial_input(&definition, request.initial_input.as_ref())?;
        match project_store::create_workflow_run_idempotently(
            repo_root,
            &request.project_id,
            &request.workflow_id,
            &request.idempotency_key,
            definition.version,
            request.initial_input.clone(),
        ) {
            Err(error) if error.code == "workflow_definition_changed_during_start" => continue,
            result => return result,
        }
    }

    Err(CommandError::retryable(
        "workflow_definition_changed_during_start",
        "The Workflow kept changing while Xero tried to start it. Try again after the current edit is saved.",
    ))
}

fn validate_workflow_initial_input(
    definition: &WorkflowDefinitionDto,
    initial_input: Option<&JsonValue>,
) -> CommandResult<()> {
    let mut missing = Vec::new();
    for node in &definition.nodes {
        collect_missing_run_inputs_for_node(node, initial_input, &mut missing);
    }
    for subgraph in &definition.subgraphs {
        collect_missing_run_inputs(&subgraph.input_bindings, initial_input, &mut missing);
        for node in &subgraph.nodes {
            collect_missing_run_inputs_for_node(node, initial_input, &mut missing);
        }
    }
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
        return Ok(());
    }
    Err(CommandError::user_fixable(
        "workflow_required_input_missing",
        format!(
            "Xero could not start `{}` because required input is missing: {}.",
            definition.name,
            missing.join(", ")
        ),
    ))
}

fn collect_missing_run_inputs_for_node(
    node: &WorkflowNodeDto,
    initial_input: Option<&JsonValue>,
    missing: &mut Vec<String>,
) {
    match node {
        WorkflowNodeDto::Agent { input_bindings, .. }
        | WorkflowNodeDto::StateWrite { input_bindings, .. }
        | WorkflowNodeDto::StatePatch { input_bindings, .. }
        | WorkflowNodeDto::Subgraph { input_bindings, .. } => {
            collect_missing_run_inputs(input_bindings, initial_input, missing);
        }
        _ => {}
    }
}

fn collect_missing_run_inputs(
    input_bindings: &[WorkflowInputBindingDto],
    initial_input: Option<&JsonValue>,
    missing: &mut Vec<String>,
) {
    for binding in input_bindings {
        let WorkflowInputBindingDto::RunInput {
            name,
            required,
            path,
            prompt_label,
        } = binding
        else {
            continue;
        };
        if !*required {
            continue;
        }
        let path = path.clone().unwrap_or_else(|| format!("$.{name}"));
        let value = initial_input.and_then(|input| {
            workflow_orchestrator::condition_eval::json_path_lookup(input, &path)
        });
        if !value.is_some_and(workflow_input_value_present) {
            missing.push(prompt_label.clone().unwrap_or_else(|| name.clone()));
        }
    }
}

fn workflow_input_value_present(value: &JsonValue) -> bool {
    match value {
        JsonValue::Null => false,
        JsonValue::String(text) => !text.trim().is_empty(),
        _ => true,
    }
}

#[tauri::command]
pub fn get_workflow_run<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetWorkflowRunRequestDto,
) -> CommandResult<WorkflowRunResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;
    let run = workflow_orchestrator::driver::reconcile_workflow_run(
        &app,
        state.inner(),
        &request.project_id,
        &request.run_id,
    )?;
    workflow_orchestrator::driver::ensure_workflow_run_driver_if_active(&app, &run);
    Ok(WorkflowRunResponseDto { run })
}

#[tauri::command]
pub fn explain_workflow_run_blocker<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ExplainWorkflowRunBlockerRequestDto,
) -> CommandResult<WorkflowRunBlockerResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;
    let run = workflow_orchestrator::driver::reconcile_workflow_run(
        &app,
        state.inner(),
        &request.project_id,
        &request.run_id,
    )?;
    Ok(workflow_run_blocker(&run))
}

#[tauri::command]
pub fn export_workflow_run_bundle<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ExportWorkflowRunBundleRequestDto,
) -> CommandResult<WorkflowRunBundleResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let run = workflow_orchestrator::driver::reconcile_workflow_run(
        &app,
        state.inner(),
        &request.project_id,
        &request.run_id,
    )?;
    let blocker = workflow_run_blocker(&run);
    let delivery_state = project_store::export_delivery_state(&repo_root, &request.project_id)?;
    let run_json = serde_json::to_value(&run).map_err(|error| {
        CommandError::system_fault(
            "workflow_run_bundle_encode_failed",
            format!("Xero could not encode Workflow run `{}`: {error}", run.id),
        )
    })?;
    let blocker_json = serde_json::to_value(&blocker).map_err(|error| {
        CommandError::system_fault(
            "workflow_run_bundle_encode_failed",
            format!(
                "Xero could not encode Workflow blocker for run `{}`: {error}",
                run.id
            ),
        )
    })?;
    Ok(WorkflowRunBundleResponseDto {
        bundle: serde_json::json!({
            "schema": "xero.workflow_run_bundle.v1",
            "projectId": request.project_id,
            "runId": request.run_id,
            "run": run_json,
            "blocker": blocker_json,
            "deliveryState": delivery_state,
        }),
    })
}

#[tauri::command]
pub fn resume_workflow_next_incomplete_phase<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ResumeWorkflowNextIncompletePhaseRequestDto,
) -> CommandResult<WorkflowRunResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.idempotency_key, "idempotencyKey")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    if let Some(replayed) = project_store::get_workflow_resume_phase_replay(
        &repo_root,
        &request.project_id,
        &request.run_id,
        &request.idempotency_key,
    )? {
        let run = workflow_orchestrator::driver::reconcile_workflow_run(
            &app,
            state.inner(),
            &request.project_id,
            &replayed.id,
        )?;
        workflow_orchestrator::driver::ensure_workflow_run_driver_if_active(&app, &run);
        return Ok(WorkflowRunResponseDto { run });
    }
    let source_run = workflow_orchestrator::driver::reconcile_workflow_run(
        &app,
        state.inner(),
        &request.project_id,
        &request.run_id,
    )?;
    let definition = project_store::get_workflow_definition(
        &repo_root,
        &request.project_id,
        &source_run.workflow_id,
    )?
    .ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_definition_not_found",
            format!(
                "Xero could not find Workflow `{}` to resume the next delivery phase.",
                source_run.workflow_id
            ),
        )
    })?;
    let collection_query = delivery_phase_collection_loop(&definition)?.query.clone();
    let collection =
        project_store::query_delivery_state(&repo_root, &request.project_id, &collection_query)?;
    let records = collection
        .get("records")
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default();
    let selection = next_incomplete_phase_resume_selection(
        &definition,
        source_run.initial_input.as_ref(),
        records,
    )?;
    validate_workflow_initial_input(&definition, Some(&selection.initial_input))?;
    let run = project_store::create_workflow_resume_phase_run_idempotently(
        &repo_root,
        &request.project_id,
        &request.idempotency_key,
        &project_store::WorkflowResumePhaseStartRecord {
            workflow_id: definition.id.clone(),
            expected_workflow_version: definition.version,
            source_run_id: source_run.id.clone(),
            initial_input: selection.initial_input.clone(),
            loop_node_id: selection.loop_node_id,
            phase_id: selection.phase_id,
            phase_key: selection.phase_key,
            input_path: selection.input_path,
        },
    )?;
    let run = workflow_orchestrator::driver::reconcile_workflow_run(
        &app,
        state.inner(),
        &request.project_id,
        &run.id,
    )?;
    workflow_orchestrator::driver::ensure_workflow_run_driver_if_active(&app, &run);
    Ok(WorkflowRunResponseDto { run })
}

#[derive(Debug, Clone)]
struct DeliveryPhaseCollectionLoop<'a> {
    node_id: &'a str,
    query: &'a WorkflowStateQueryDto,
    controls: &'a WorkflowCollectionLoopControlsDto,
    sort_key: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
struct NextIncompletePhaseResumeSelection {
    initial_input: JsonValue,
    loop_node_id: String,
    phase_id: String,
    phase_key: String,
    input_path: String,
}

fn delivery_phase_collection_loop(
    definition: &WorkflowDefinitionDto,
) -> CommandResult<DeliveryPhaseCollectionLoop<'_>> {
    definition
        .nodes
        .iter()
        .find_map(|node| {
            if let WorkflowNodeDto::CollectionLoop {
                id,
                collection,
                controls,
                sort_key,
                ..
            } = node
            {
                (collection.entity_type == WorkflowDeliveryStateEntityTypeDto::DeliveryPhase)
                    .then_some(DeliveryPhaseCollectionLoop {
                        node_id: id,
                        query: collection,
                        controls,
                        sort_key: sort_key.as_deref(),
                    })
            } else {
                None
            }
        })
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_delivery_phase_loop_missing",
                format!(
                    "Workflow `{}` does not have a delivery-phase collection loop to resume.",
                    definition.name
                ),
            )
        })
}

fn next_incomplete_phase_resume_selection(
    definition: &WorkflowDefinitionDto,
    current_input: Option<&JsonValue>,
    mut records: Vec<JsonValue>,
) -> CommandResult<NextIncompletePhaseResumeSelection> {
    let collection_loop = delivery_phase_collection_loop(definition)?;
    records.retain(incomplete_delivery_phase_record);
    if let Some(sort_key) = collection_loop.sort_key {
        records.sort_by(|left, right| {
            compare_json_values_for_resume(
                workflow_orchestrator::condition_eval::json_path_lookup(left, sort_key),
                workflow_orchestrator::condition_eval::json_path_lookup(right, sort_key),
            )
        });
    }
    let next = records.first().ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_no_incomplete_delivery_phase",
            format!(
                "Workflow `{}` has no incomplete delivery phases to resume.",
                definition.name
            ),
        )
    })?;
    let phase_id = next
        .get("id")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_delivery_phase_id_missing",
                "Xero found an incomplete delivery phase without an id.",
            )
        })?;
    let phase_key = delivery_phase_resume_key(next).ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_delivery_phase_key_missing",
            format!("Xero found delivery phase `{phase_id}` without a phase key or id."),
        )
    })?;
    let input_path = collection_loop
        .controls
        .from_input_path
        .as_deref()
        .or(collection_loop.controls.only_input_path.as_deref())
        .unwrap_or("$.from")
        .to_string();
    let mut initial_input = current_input
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    set_json_path_value(
        &mut initial_input,
        &input_path,
        JsonValue::String(phase_key.clone()),
    )?;

    Ok(NextIncompletePhaseResumeSelection {
        initial_input,
        loop_node_id: collection_loop.node_id.to_string(),
        phase_id: phase_id.to_string(),
        phase_key,
        input_path,
    })
}

fn incomplete_delivery_phase_record(record: &JsonValue) -> bool {
    !matches!(
        record
            .get("status")
            .and_then(JsonValue::as_str)
            .map(|status| status.trim().to_ascii_lowercase()),
        Some(status) if matches!(status.as_str(), "complete" | "completed" | "archived")
    )
}

fn delivery_phase_resume_key(record: &JsonValue) -> Option<String> {
    ["phaseKey", "phase_key", "id", "sortOrder", "sort_order"]
        .into_iter()
        .find_map(|field| record.get(field).and_then(json_value_to_resume_key))
}

fn json_value_to_resume_key(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        JsonValue::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn set_json_path_value(root: &mut JsonValue, path: &str, value: JsonValue) -> CommandResult<()> {
    let path = path.trim();
    if path == "$" {
        *root = value;
        return Ok(());
    }
    let path = path.strip_prefix("$.").ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_resume_input_path_invalid",
            format!("Workflow resume input path `{path}` must start with `$.`."),
        )
    })?;
    let segments = path.split('.').collect::<Vec<_>>();
    if segments.is_empty()
        || segments.iter().any(|segment| {
            segment.is_empty()
                || segment
                    .chars()
                    .any(|character| character.is_whitespace() || matches!(character, '[' | ']'))
        })
    {
        return Err(CommandError::user_fixable(
            "workflow_resume_input_path_invalid",
            "Workflow resume input paths must contain non-empty object field segments without whitespace or array indexes.",
        ));
    }

    let mut cursor = root;
    for segment in &segments[..segments.len().saturating_sub(1)] {
        if !cursor.is_object() {
            *cursor = serde_json::json!({});
        }
        let object = cursor.as_object_mut().ok_or_else(|| {
            CommandError::system_fault(
                "workflow_resume_input_path_failed",
                "Xero could not prepare the Workflow resume input object.",
            )
        })?;
        cursor = object
            .entry((*segment).to_string())
            .or_insert_with(|| serde_json::json!({}));
    }

    let last = segments.last().expect("validated non-empty path segments");
    if !cursor.is_object() {
        *cursor = serde_json::json!({});
    }
    let object = cursor.as_object_mut().ok_or_else(|| {
        CommandError::system_fault(
            "workflow_resume_input_path_failed",
            "Xero could not write the Workflow resume input object.",
        )
    })?;
    object.insert((*last).to_string(), value);
    Ok(())
}

fn compare_json_values_for_resume(
    left: Option<&JsonValue>,
    right: Option<&JsonValue>,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => match (left.as_f64(), right.as_f64()) {
            (Some(left), Some(right)) => left
                .partial_cmp(&right)
                .unwrap_or(std::cmp::Ordering::Equal),
            _ => json_sort_key(left).cmp(&json_sort_key(right)),
        },
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

fn json_sort_key(value: &JsonValue) -> String {
    match value {
        JsonValue::String(text) => text.clone(),
        value => serde_json::to_string(value).unwrap_or_else(|_| value.to_string()),
    }
}

fn workflow_run_blocker(run: &WorkflowRunDto) -> WorkflowRunBlockerResponseDto {
    if let Some(node) = run
        .nodes
        .iter()
        .filter(|node| node.status == WorkflowNodeRunStatusDto::WaitingOnGate)
        .max_by_key(|node| node.updated_at.as_str())
    {
        return WorkflowRunBlockerResponseDto {
            status: "waiting_on_human".into(),
            summary: format!("Workflow is waiting at `{}`.", node.node_id),
            node_id: Some(node.node_id.clone()),
            node_run_id: Some(node.id.clone()),
            failure_class: None,
            event: latest_event_for_node(run, &node.id),
        };
    }
    if let Some(node) = run
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.status,
                WorkflowNodeRunStatusDto::Failed
                    | WorkflowNodeRunStatusDto::Stalled
                    | WorkflowNodeRunStatusDto::Cancelled
            )
        })
        .max_by_key(|node| node.updated_at.as_str())
    {
        return WorkflowRunBlockerResponseDto {
            status: node.status.as_str().into(),
            summary: format!(
                "Workflow is blocked at `{}`{}.",
                node.node_id,
                node.failure_class
                    .as_ref()
                    .map(|failure| format!(" by `{failure}`"))
                    .unwrap_or_default()
            ),
            node_id: Some(node.node_id.clone()),
            node_run_id: Some(node.id.clone()),
            failure_class: node.failure_class.clone(),
            event: latest_event_for_node(run, &node.id),
        };
    }
    if let Some(event) = run
        .events
        .iter()
        .rev()
        .find(|event| event.event_type == "workflow_route_missing")
    {
        return WorkflowRunBlockerResponseDto {
            status: "route_missing".into(),
            summary: "Workflow paused because no outgoing route matched.".into(),
            node_id: event
                .event
                .get("nodeId")
                .and_then(JsonValue::as_str)
                .map(ToOwned::to_owned),
            node_run_id: event.node_run_id.clone(),
            failure_class: None,
            event: Some(event.event.clone()),
        };
    }
    WorkflowRunBlockerResponseDto {
        status: run.status.as_str().into(),
        summary: format!("Workflow run is `{}`.", run.status.as_str()),
        node_id: None,
        node_run_id: None,
        failure_class: None,
        event: run.events.last().map(|event| event.event.clone()),
    }
}

fn latest_event_for_node(run: &WorkflowRunDto, node_run_id: &str) -> Option<JsonValue> {
    run.events
        .iter()
        .rev()
        .find(|event| event.node_run_id.as_deref() == Some(node_run_id))
        .map(|event| event.event.clone())
}

#[tauri::command]
pub fn list_workflow_runs<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListWorkflowRunsRequestDto,
) -> CommandResult<ListWorkflowRunsResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let runs = project_store::list_workflow_runs(
        &repo_root,
        &request.project_id,
        request.workflow_id.as_deref(),
    )?;
    // Re-arm drivers for runs that were still in flight when the app last
    // closed so they resume advancing as soon as the project is opened.
    for run in &runs {
        workflow_orchestrator::driver::ensure_workflow_run_driver_if_active(&app, run);
    }
    Ok(ListWorkflowRunsResponseDto { runs })
}

#[tauri::command]
pub fn cancel_workflow_run<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CancelWorkflowRunRequestDto,
) -> CommandResult<WorkflowRunResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    project_store::request_workflow_run_cancellation(
        &repo_root,
        &request.project_id,
        &request.run_id,
        request.reason.as_deref(),
    )?;
    let reconciliation = workflow_orchestrator::driver::reconcile_workflow_run(
        &app,
        state.inner(),
        &request.project_id,
        &request.run_id,
    );
    let run = accepted_workflow_cancellation_run(
        &repo_root,
        &request.project_id,
        &request.run_id,
        reconciliation,
    )?;
    workflow_orchestrator::driver::ensure_workflow_run_driver_if_active(&app, &run);
    workflow_orchestrator::driver::emit_workflow_run_updated(&app, &run);
    Ok(WorkflowRunResponseDto { run })
}

fn accepted_workflow_cancellation_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    reconciliation: CommandResult<WorkflowRunDto>,
) -> CommandResult<WorkflowRunDto> {
    match reconciliation {
        Ok(run) => Ok(run),
        Err(reconciliation_error) => {
            let persisted = project_store::get_workflow_run(repo_root, project_id, run_id)?;
            match persisted {
                Some(run)
                    if reconciliation_error.retryable
                        && matches!(
                        run.status,
                        crate::commands::contracts::workflows::WorkflowRunStatusDto::Cancelling
                            | crate::commands::contracts::workflows::WorkflowRunStatusDto::Cancelled
                    ) =>
                {
                    // Cancellation intent is already durable. A transient drain
                    // conflict is background work, not a rejected user command.
                    Ok(run)
                }
                _ => Err(reconciliation_error),
            }
        }
    }
}

#[tauri::command]
pub fn retry_workflow_node_run<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RetryWorkflowNodeRunRequestDto,
) -> CommandResult<WorkflowRunResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.node_run_id, "nodeRunId")?;
    let run = workflow_orchestrator::driver::retry_workflow_node_run(
        &app,
        state.inner(),
        &request.project_id,
        &request.run_id,
        &request.node_run_id,
    )?;
    workflow_orchestrator::driver::ensure_workflow_run_driver_if_active(&app, &run);
    Ok(WorkflowRunResponseDto { run })
}

#[tauri::command]
pub fn skip_workflow_branch<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SkipWorkflowBranchRequestDto,
) -> CommandResult<WorkflowRunResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.node_run_id, "nodeRunId")?;
    let run = workflow_orchestrator::driver::skip_workflow_branch(
        &app,
        state.inner(),
        &request.project_id,
        &request.run_id,
        &request.node_run_id,
        request.reason.as_deref(),
    )?;
    workflow_orchestrator::driver::ensure_workflow_run_driver_if_active(&app, &run);
    Ok(WorkflowRunResponseDto { run })
}

#[tauri::command]
pub fn resume_workflow_checkpoint<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ResumeWorkflowCheckpointRequestDto,
) -> CommandResult<WorkflowRunResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.node_run_id, "nodeRunId")?;
    validate_non_empty(&request.decision, "decision")?;
    let run = workflow_orchestrator::driver::resume_workflow_checkpoint(
        &app,
        state.inner(),
        &request.project_id,
        &request.run_id,
        &request.node_run_id,
        &request.decision,
        request.payload,
    )?;
    workflow_orchestrator::driver::ensure_workflow_run_driver_if_active(&app, &run);
    Ok(WorkflowRunResponseDto { run })
}

#[tauri::command]
pub fn read_workflow_delivery_state<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ReadWorkflowDeliveryStateRequestDto,
) -> CommandResult<WorkflowDeliveryStateResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    Ok(WorkflowDeliveryStateResponseDto {
        state: project_store::query_delivery_state(
            &repo_root,
            &request.project_id,
            &request.query,
        )?,
    })
}

#[tauri::command]
pub fn write_workflow_delivery_state<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: WriteWorkflowDeliveryStateRequestDto,
) -> CommandResult<WorkflowDeliveryStateResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    Ok(WorkflowDeliveryStateResponseDto {
        state: project_store::write_delivery_state(
            &repo_root,
            &request.project_id,
            project_store::DeliveryStateWriteContext {
                workflow_run_id: None,
                node_run_id: None,
            },
            &request.operation,
        )?,
    })
}

#[tauri::command]
pub fn export_workflow_delivery_state<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ExportWorkflowDeliveryStateRequestDto,
) -> CommandResult<WorkflowDeliveryStateResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    Ok(WorkflowDeliveryStateResponseDto {
        state: project_store::export_delivery_state(&repo_root, &request.project_id)?,
    })
}

#[tauri::command]
pub fn wipe_workflow_delivery_state<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: WipeWorkflowDeliveryStateRequestDto,
) -> CommandResult<WorkflowDeliveryStateResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    project_store::wipe_delivery_state(&repo_root, &request.project_id)?;
    Ok(WorkflowDeliveryStateResponseDto {
        state: serde_json::json!({ "wiped": true }),
    })
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::sync::{Arc, Barrier};

    use super::*;
    use crate::{
        commands::contracts::workflows::{
            WorkflowEventDto, WorkflowRunNodeDto, WorkflowRunPolicyDto, WorkflowRunStatusDto,
        },
        db::{
            configure_connection, migrations::migrations, register_project_database_path_for_tests,
        },
    };
    use rusqlite::Connection;
    use serde_json::json;
    use tempfile::TempDir;

    fn definition_with_delivery_phase_loop(
        controls: WorkflowCollectionLoopControlsDto,
    ) -> WorkflowDefinitionDto {
        WorkflowDefinitionDto {
            schema: "xero.workflow_definition.v1".into(),
            id: "workflow-1".into(),
            project_id: "project-1".into(),
            name: "Resume workflow".into(),
            description: String::new(),
            version: 1,
            start_node_id: "next_phase".into(),
            nodes: vec![WorkflowNodeDto::CollectionLoop {
                id: "next_phase".into(),
                title: "Next phase".into(),
                description: String::new(),
                position: Default::default(),
                collection: WorkflowStateQueryDto {
                    entity_type: WorkflowDeliveryStateEntityTypeDto::DeliveryPhase,
                    filters: Vec::new(),
                    order_by: Some("$.sortOrder".into()),
                    limit: None,
                    include_archived: false,
                },
                item_artifact_type: "collection_item".into(),
                item_variable_name: "item".into(),
                sort_key: Some("$.sortOrder".into()),
                after_item_requery: true,
                max_item_count: 100,
                max_runtime_seconds: None,
                controls,
            }],
            edges: Vec::new(),
            subgraphs: Vec::new(),
            artifact_contracts: Vec::new(),
            run_policy: WorkflowRunPolicyDto::default(),
            created_at: None,
            updated_at: None,
        }
    }

    fn repo_with_workflow() -> (TempDir, WorkflowDefinitionDto) {
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
        let definition =
            definition_with_delivery_phase_loop(WorkflowCollectionLoopControlsDto::default());
        let created = project_store::create_workflow_definition(temp.path(), &definition)
            .expect("create workflow");
        (temp, created)
    }

    fn run_node(
        id: &str,
        node_id: &str,
        status: WorkflowNodeRunStatusDto,
        updated_at: &str,
        failure_class: Option<&str>,
    ) -> WorkflowRunNodeDto {
        WorkflowRunNodeDto {
            id: id.into(),
            workflow_run_id: "run-1".into(),
            node_id: node_id.into(),
            node_type: "agent".into(),
            status,
            attempt_number: 1,
            runtime_run_id: None,
            agent_session_id: None,
            failure_class: failure_class.map(str::to_string),
            started_at: None,
            updated_at: updated_at.into(),
            completed_at: None,
            idempotency_key: format!("{id}-attempt-1"),
        }
    }

    fn run_event(
        id: &str,
        node_run_id: Option<&str>,
        event_type: &str,
        event: JsonValue,
    ) -> WorkflowEventDto {
        WorkflowEventDto {
            id: id.into(),
            workflow_run_id: "run-1".into(),
            node_run_id: node_run_id.map(str::to_string),
            event_type: event_type.into(),
            event,
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn workflow_run_fixture(
        status: WorkflowRunStatusDto,
        nodes: Vec<WorkflowRunNodeDto>,
        events: Vec<WorkflowEventDto>,
    ) -> WorkflowRunDto {
        WorkflowRunDto {
            id: "run-1".into(),
            project_id: "project-1".into(),
            workflow_version_id: "workflow-version-1".into(),
            workflow_id: "workflow-1".into(),
            workflow_version_number: 1,
            status,
            terminal_status: None,
            definition_snapshot: definition_with_delivery_phase_loop(
                WorkflowCollectionLoopControlsDto::default(),
            ),
            initial_input: None,
            started_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            completed_at: None,
            cancellation_reason: None,
            nodes,
            edge_decisions: Vec::new(),
            artifacts: Vec::new(),
            gate_decisions: Vec::new(),
            loop_attempts: Vec::new(),
            events,
        }
    }

    #[test]
    fn workflow_start_command_replays_the_same_request_and_rejects_key_reuse() {
        let (temp, definition) = repo_with_workflow();
        let request = StartWorkflowRunRequestDto {
            project_id: "project-1".into(),
            workflow_id: definition.id.clone(),
            idempotency_key: "command-start-1".into(),
            initial_input: Some(json!({ "goal": "ship" })),
        };
        let first = persist_workflow_start_request(temp.path(), &request).expect("start run");
        let replay =
            persist_workflow_start_request(temp.path(), &request).expect("replay start run");
        assert_eq!(first.id, replay.id);

        let mut conflicting = request;
        conflicting.initial_input = Some(json!({ "goal": "different" }));
        let error = persist_workflow_start_request(temp.path(), &conflicting)
            .expect_err("same key with another payload must fail");
        assert_eq!(error.code, "workflow_run_idempotency_conflict");
    }

    #[test]
    fn concurrent_workflow_start_commands_return_one_run() {
        let (temp, definition) = repo_with_workflow();
        let repo_root = temp.path().to_path_buf();
        let barrier = Arc::new(Barrier::new(2));
        let handles = [(), ()].map(|()| {
            let repo_root = repo_root.clone();
            let barrier = Arc::clone(&barrier);
            let request = StartWorkflowRunRequestDto {
                project_id: "project-1".into(),
                workflow_id: definition.id.clone(),
                idempotency_key: "concurrent-command-start".into(),
                initial_input: Some(json!({ "goal": "ship" })),
            };
            std::thread::spawn(move || {
                barrier.wait();
                persist_workflow_start_request(&repo_root, &request)
            })
        });
        let runs = handles.map(|handle| {
            handle
                .join()
                .expect("join command")
                .expect("start or replay run")
        });

        assert_eq!(runs[0].id, runs[1].id);
        assert_eq!(
            project_store::list_workflow_runs(temp.path(), "project-1", Some(&definition.id),)
                .expect("list runs")
                .len(),
            1,
        );
    }

    #[test]
    fn accepted_cancellation_survives_an_immediate_reconcile_failure() {
        let (temp, definition) = repo_with_workflow();
        let run =
            project_store::create_workflow_run(temp.path(), "project-1", &definition.id, None)
                .expect("create run");
        project_store::update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            crate::commands::contracts::workflows::WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        assert!(project_store::request_workflow_run_cancellation(
            temp.path(),
            "project-1",
            &run.id,
            Some("stop"),
        )
        .expect("persist cancellation"));

        let accepted = accepted_workflow_cancellation_run(
            temp.path(),
            "project-1",
            &run.id,
            Err(CommandError::retryable(
                "workflow_cancellation_execution_still_active",
                "A foreign execution owner is still draining.",
            )),
        )
        .expect("return accepted cancellation");

        assert_eq!(
            accepted.status,
            crate::commands::contracts::workflows::WorkflowRunStatusDto::Cancelling,
        );
        assert!(accepted
            .events
            .iter()
            .any(|event| event.event_type == "workflow_cancellation_requested"));

        let system_error = CommandError::system_fault(
            "workflow_snapshot_corrupt",
            "The Workflow snapshot could not be decoded.",
        );
        assert_eq!(
            accepted_workflow_cancellation_run(
                temp.path(),
                "project-1",
                &run.id,
                Err(system_error.clone()),
            )
            .expect_err("non-retryable reconciliation failures must remain visible"),
            system_error,
        );
    }

    #[test]
    fn next_incomplete_phase_resume_selection_sets_from_control_to_first_incomplete_phase() {
        let definition = definition_with_delivery_phase_loop(WorkflowCollectionLoopControlsDto {
            from_input_path: Some("$.phase.from".into()),
            to_input_path: None,
            only_input_path: Some("$.only".into()),
        });

        let selection = next_incomplete_phase_resume_selection(
            &definition,
            Some(&json!({
                "goal": "ship",
                "phase": { "from": "1" },
            })),
            vec![
                json!({
                    "id": "phase-3",
                    "phaseKey": "3",
                    "status": "incomplete",
                    "sortOrder": 3,
                }),
                json!({
                    "id": "phase-1",
                    "phaseKey": "1",
                    "status": "complete",
                    "sortOrder": 1,
                }),
                json!({
                    "id": "phase-2",
                    "phaseKey": "2",
                    "status": "incomplete",
                    "sortOrder": 2,
                }),
            ],
        )
        .expect("resume selection");

        assert_eq!(selection.loop_node_id, "next_phase");
        assert_eq!(selection.phase_id, "phase-2");
        assert_eq!(selection.phase_key, "2");
        assert_eq!(selection.input_path, "$.phase.from");
        assert_eq!(selection.initial_input["goal"], json!("ship"));
        assert_eq!(selection.initial_input["phase"]["from"], json!("2"));
    }

    #[test]
    fn next_incomplete_phase_resume_selection_reports_when_all_phases_are_complete() {
        let definition = definition_with_delivery_phase_loop(WorkflowCollectionLoopControlsDto {
            from_input_path: Some("$.from".into()),
            to_input_path: None,
            only_input_path: None,
        });

        let error = next_incomplete_phase_resume_selection(
            &definition,
            Some(&json!({ "goal": "ship" })),
            vec![json!({
                "id": "phase-1",
                "phaseKey": "1",
                "status": "complete",
                "sortOrder": 1,
            })],
        )
        .expect_err("complete phases should not produce a resume input");

        assert_eq!(error.code, "workflow_no_incomplete_delivery_phase");
    }

    #[test]
    fn required_run_input_validation_reports_labels_once_and_accepts_non_strings() {
        let mut definition =
            definition_with_delivery_phase_loop(WorkflowCollectionLoopControlsDto::default());
        definition.nodes = vec![serde_json::from_value(json!({
            "type": "agent",
            "id": "agent-1",
            "title": "Agent",
            "agentRef": {
                "kind": "built_in",
                "runtimeAgentId": "ask",
                "version": 1
            },
            "inputBindings": [
                {
                    "source": "run_input",
                    "name": "goal",
                    "required": true,
                    "path": "$.payload.goal",
                    "promptLabel": "Goal"
                },
                {
                    "source": "run_input",
                    "name": "duplicate_goal",
                    "required": true,
                    "path": "$.payload.duplicateGoal",
                    "promptLabel": "Goal"
                },
                {
                    "source": "run_input",
                    "name": "approved",
                    "required": true
                },
                {
                    "source": "run_input",
                    "name": "optional",
                    "required": false
                },
                {
                    "source": "artifact",
                    "name": "prior",
                    "artifactRef": "prior.output"
                }
            ]
        }))
        .expect("agent node fixture")];
        definition.start_node_id = "agent-1".into();

        let error = validate_workflow_initial_input(
            &definition,
            Some(&json!({
                "payload": { "goal": "  ", "duplicateGoal": null },
                "approved": false
            })),
        )
        .expect_err("blank and null required inputs must be missing");
        assert_eq!(error.code, "workflow_required_input_missing");
        assert!(error.message.ends_with("required input is missing: Goal."));

        validate_workflow_initial_input(
            &definition,
            Some(&json!({
                "payload": { "goal": { "value": "ship" }, "duplicateGoal": 0 },
                "approved": false
            })),
        )
        .expect("objects, numbers, and booleans are present input values");

        assert!(!workflow_input_value_present(&JsonValue::Null));
        assert!(!workflow_input_value_present(&json!(" \n ")));
        assert!(workflow_input_value_present(&json!([])));
    }

    #[test]
    fn resume_input_path_writer_supports_root_and_nested_object_fields() {
        let mut input = json!({ "phase": "stale", "keep": true });
        set_json_path_value(&mut input, "  $.phase.from  ", json!("2")).expect("write nested path");
        assert_eq!(input, json!({ "phase": { "from": "2" }, "keep": true }));

        set_json_path_value(&mut input, "$", json!({ "replaced": true }))
            .expect("replace root input");
        assert_eq!(input, json!({ "replaced": true }));

        let mut scalar = json!(false);
        set_json_path_value(&mut scalar, "$.phase.only", json!(3))
            .expect("replace scalar parents with objects");
        assert_eq!(scalar, json!({ "phase": { "only": 3 } }));

        for invalid_path in ["from", "$.", "$.phase..from", "$.items[0]", "$.phase from"] {
            let error = set_json_path_value(&mut json!({}), invalid_path, json!(1))
                .expect_err("unsupported resume path must fail");
            assert_eq!(error.code, "workflow_resume_input_path_invalid");
        }
    }

    #[test]
    fn delivery_phase_helpers_cover_status_key_and_sort_variants() {
        for status in ["complete", " COMPLETED ", "Archived"] {
            assert!(!incomplete_delivery_phase_record(
                &json!({ "status": status })
            ));
        }
        assert!(incomplete_delivery_phase_record(
            &json!({ "status": "running" })
        ));
        assert!(incomplete_delivery_phase_record(&json!({})));

        assert_eq!(
            delivery_phase_resume_key(&json!({ "phaseKey": " 3 " })),
            Some("3".into())
        );
        assert_eq!(
            delivery_phase_resume_key(&json!({ "phase_key": 4 })),
            Some("4".into())
        );
        assert_eq!(
            delivery_phase_resume_key(&json!({ "id": "phase-5" })),
            Some("phase-5".into())
        );
        assert_eq!(
            delivery_phase_resume_key(&json!({ "sortOrder": 6 })),
            Some("6".into())
        );
        assert_eq!(
            delivery_phase_resume_key(&json!({ "sort_order": 7 })),
            Some("7".into())
        );
        assert_eq!(
            delivery_phase_resume_key(&json!({ "phaseKey": "  " })),
            None
        );
        assert_eq!(
            delivery_phase_resume_key(&json!({ "phaseKey": null, "id": "phase-8" })),
            Some("phase-8".into())
        );
        assert_eq!(
            delivery_phase_resume_key(&json!({ "phaseKey": "  ", "id": "phase-9" })),
            Some("phase-9".into())
        );
        assert_eq!(json_value_to_resume_key(&json!(true)), None);

        assert_eq!(
            compare_json_values_for_resume(Some(&json!(2)), Some(&json!(10))),
            Ordering::Less,
        );
        assert_eq!(
            compare_json_values_for_resume(Some(&json!("beta")), Some(&json!("alpha"))),
            Ordering::Greater,
        );
        assert_eq!(
            compare_json_values_for_resume(Some(&json!(1)), None),
            Ordering::Less
        );
        assert_eq!(
            compare_json_values_for_resume(None, Some(&json!(1))),
            Ordering::Greater
        );
        assert_eq!(compare_json_values_for_resume(None, None), Ordering::Equal);
        assert_eq!(json_sort_key(&json!({ "b": 2 })), "{\"b\":2}");
    }

    #[test]
    fn next_incomplete_phase_selection_covers_fallback_and_invalid_records() {
        let definition = definition_with_delivery_phase_loop(WorkflowCollectionLoopControlsDto {
            from_input_path: None,
            to_input_path: Some("$.to".into()),
            only_input_path: Some("$.phase.only".into()),
        });
        let selection = next_incomplete_phase_resume_selection(
            &definition,
            None,
            vec![json!({ "id": "phase-1", "status": "running" })],
        )
        .expect("only input path fallback");
        assert_eq!(selection.input_path, "$.phase.only");
        assert_eq!(selection.phase_key, "phase-1");
        assert_eq!(
            selection.initial_input,
            json!({ "phase": { "only": "phase-1" } })
        );

        let error = next_incomplete_phase_resume_selection(
            &definition,
            None,
            vec![json!({ "phaseKey": "1", "status": "running" })],
        )
        .expect_err("incomplete phase id is required");
        assert_eq!(error.code, "workflow_delivery_phase_id_missing");

        let mut missing_loop = definition;
        missing_loop.nodes.clear();
        let error = delivery_phase_collection_loop(&missing_loop)
            .expect_err("delivery phase loop is required");
        assert_eq!(error.code, "workflow_delivery_phase_loop_missing");
    }

    #[test]
    fn workflow_blocker_prioritizes_human_failures_routes_and_fallback_status() {
        let mut run = workflow_run_fixture(
            WorkflowRunStatusDto::Running,
            vec![
                run_node(
                    "failed-run",
                    "failed-node",
                    WorkflowNodeRunStatusDto::Failed,
                    "2026-01-01T00:00:03Z",
                    Some("provider_error"),
                ),
                run_node(
                    "gate-old",
                    "approval-old",
                    WorkflowNodeRunStatusDto::WaitingOnGate,
                    "2026-01-01T00:00:01Z",
                    None,
                ),
                run_node(
                    "gate-new",
                    "approval-new",
                    WorkflowNodeRunStatusDto::WaitingOnGate,
                    "2026-01-01T00:00:02Z",
                    None,
                ),
            ],
            vec![
                run_event(
                    "event-1",
                    Some("gate-new"),
                    "checkpoint",
                    json!({ "sequence": 1 }),
                ),
                run_event(
                    "event-2",
                    Some("gate-new"),
                    "checkpoint",
                    json!({ "sequence": 2 }),
                ),
            ],
        );
        let blocker = workflow_run_blocker(&run);
        assert_eq!(blocker.status, "waiting_on_human");
        assert_eq!(blocker.node_id.as_deref(), Some("approval-new"));
        assert_eq!(blocker.event, Some(json!({ "sequence": 2 })));

        run.nodes
            .retain(|node| node.status != WorkflowNodeRunStatusDto::WaitingOnGate);
        let blocker = workflow_run_blocker(&run);
        assert_eq!(blocker.status, "failed");
        assert_eq!(blocker.failure_class.as_deref(), Some("provider_error"));

        run.nodes.clear();
        run.events.push(run_event(
            "event-3",
            Some("router-run"),
            "workflow_route_missing",
            json!({ "nodeId": "router" }),
        ));
        let blocker = workflow_run_blocker(&run);
        assert_eq!(blocker.status, "route_missing");
        assert_eq!(blocker.node_id.as_deref(), Some("router"));
        assert_eq!(blocker.node_run_id.as_deref(), Some("router-run"));

        run.events.clear();
        run.status = WorkflowRunStatusDto::Paused;
        let blocker = workflow_run_blocker(&run);
        assert_eq!(blocker.status, "paused");
        assert_eq!(blocker.summary, "Workflow run is `paused`.");
        assert_eq!(blocker.event, None);
    }
}
