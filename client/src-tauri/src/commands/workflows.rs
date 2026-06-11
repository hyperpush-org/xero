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
            WorkflowRunResponseDto, WorkflowRunStatusDto, WorkflowStateQueryDto,
            WorkflowTerminalStatusDto, WorkflowValidationReportDto,
            WriteWorkflowDeliveryStateRequestDto,
        },
        runtime_support::resolve_project_root,
        validate_non_empty, CommandError, CommandResult,
    },
    db::project_store,
    runtime::{workflow_orchestrator, DesktopAgentCoreRuntime},
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
    let initial_input = request.initial_input;
    validate_workflow_initial_input(&definition, initial_input.as_ref())?;
    let run = project_store::create_workflow_run(
        &repo_root,
        &request.project_id,
        &request.workflow_id,
        initial_input,
    )?;
    let run = workflow_orchestrator::reconcile::reconcile_workflow_run(
        &app,
        state.inner(),
        &request.project_id,
        &run.id,
    )?;
    Ok(WorkflowRunResponseDto { run })
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
    let run = workflow_orchestrator::reconcile::reconcile_workflow_run(
        &app,
        state.inner(),
        &request.project_id,
        &request.run_id,
    )?;
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
    let run = workflow_orchestrator::reconcile::reconcile_workflow_run(
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
    let run = workflow_orchestrator::reconcile::reconcile_workflow_run(
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
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let source_run = workflow_orchestrator::reconcile::reconcile_workflow_run(
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
    let run = project_store::create_workflow_run(
        &repo_root,
        &request.project_id,
        &definition.id,
        Some(selection.initial_input.clone()),
    )?;
    project_store::insert_workflow_event(
        &repo_root,
        &request.project_id,
        &run.id,
        None,
        "workflow_resume_next_incomplete_phase",
        &serde_json::json!({
            "sourceRunId": source_run.id,
            "loopNodeId": selection.loop_node_id,
            "phaseId": selection.phase_id,
            "phaseKey": selection.phase_key,
            "inputPath": selection.input_path,
        }),
    )?;
    let run = workflow_orchestrator::reconcile::reconcile_workflow_run(
        &app,
        state.inner(),
        &request.project_id,
        &run.id,
    )?;
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
    record
        .get("phaseKey")
        .or_else(|| record.get("phase_key"))
        .or_else(|| record.get("id"))
        .or_else(|| record.get("sortOrder"))
        .or_else(|| record.get("sort_order"))
        .and_then(json_value_to_resume_key)
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
    if segments.is_empty() || segments.iter().any(|segment| segment.trim().is_empty()) {
        return Err(CommandError::user_fixable(
            "workflow_resume_input_path_invalid",
            "Workflow resume input paths must contain non-empty field segments.",
        ));
    }

    let mut cursor = root;
    for segment in &segments[..segments.len().saturating_sub(1)] {
        if segment.contains('[') || segment.contains(']') {
            return Err(CommandError::user_fixable(
                "workflow_resume_input_path_invalid",
                "Workflow resume input paths support object fields, not array indexes.",
            ));
        }
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
    if last.contains('[') || last.contains(']') {
        return Err(CommandError::user_fixable(
            "workflow_resume_input_path_invalid",
            "Workflow resume input paths support object fields, not array indexes.",
        ));
    }
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
pub fn list_workflow_runs<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListWorkflowRunsRequestDto,
) -> CommandResult<ListWorkflowRunsResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    Ok(ListWorkflowRunsResponseDto {
        runs: project_store::list_workflow_runs(
            &repo_root,
            &request.project_id,
            request.workflow_id.as_deref(),
        )?,
    })
}

#[tauri::command]
pub fn cancel_workflow_run<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CancelWorkflowRunRequestDto,
) -> CommandResult<WorkflowRunResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let run = project_store::get_workflow_run(&repo_root, &request.project_id, &request.run_id)?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_run_not_found",
                format!("Xero could not find Workflow run `{}`.", request.run_id),
            )
        })?;

    let runtime = DesktopAgentCoreRuntime::new(state.inner().agent_run_supervisor().clone());
    for node in run.nodes.iter().filter(|node| {
        node.status == crate::commands::contracts::workflows::WorkflowNodeRunStatusDto::Running
    }) {
        if let Some(runtime_run_id) = node.runtime_run_id.as_ref() {
            let _ = runtime.cancel_run(
                repo_root.clone(),
                request.project_id.clone(),
                runtime_run_id.clone(),
            );
        }
        project_store::update_workflow_run_node(
            &repo_root,
            &request.project_id,
            &node.id,
            crate::commands::contracts::workflows::WorkflowNodeRunStatusDto::Cancelled,
            None,
            None,
            Some("cancelled"),
        )?;
    }
    project_store::update_workflow_run_status(
        &repo_root,
        &request.project_id,
        &request.run_id,
        WorkflowRunStatusDto::Cancelled,
        Some(WorkflowTerminalStatusDto::Cancelled),
        request.reason.as_deref(),
    )?;
    let run = project_store::get_workflow_run(&repo_root, &request.project_id, &request.run_id)?
        .ok_or_else(|| {
            CommandError::system_fault(
                "workflow_run_missing_after_cancel",
                format!(
                    "Workflow run `{}` disappeared during cancellation.",
                    request.run_id
                ),
            )
        })?;
    Ok(WorkflowRunResponseDto { run })
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
    let run = workflow_orchestrator::reconcile::retry_workflow_node_run(
        &app,
        state.inner(),
        &request.project_id,
        &request.run_id,
        &request.node_run_id,
    )?;
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
    let run = workflow_orchestrator::reconcile::skip_workflow_branch(
        &app,
        state.inner(),
        &request.project_id,
        &request.run_id,
        &request.node_run_id,
        request.reason.as_deref(),
    )?;
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
    let run = workflow_orchestrator::reconcile::resume_workflow_checkpoint(
        &app,
        state.inner(),
        &request.project_id,
        &request.run_id,
        &request.node_run_id,
        &request.decision,
        request.payload,
    )?;
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
    use super::*;
    use crate::commands::contracts::workflows::WorkflowRunPolicyDto;
    use serde_json::json;

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
}
