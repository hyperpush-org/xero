use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        runtime_support::{emit_project_updated, resolve_project_root},
        validate_non_empty, CommandError, CommandResult, ProjectUpdateReason,
        UpsertWorkflowGraphRequestDto, UpsertWorkflowGraphResponseDto, WorkflowGateMetadataDto,
        WorkflowGateStateDto, WorkflowGraphEdgeDto, WorkflowGraphGateRequestDto,
        WorkflowGraphNodeDto,
    },
    db::project_store::{self, WorkflowGraphUpsertRecord},
    state::DesktopState,
};

const MAX_GRAPH_NODE_COUNT: usize = 256;
const MAX_GRAPH_EDGE_COUNT: usize = 1024;
const MAX_GRAPH_GATE_COUNT: usize = 1024;

#[tauri::command]
pub fn upsert_workflow_graph<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertWorkflowGraphRequestDto,
) -> CommandResult<UpsertWorkflowGraphResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_graph_size(&request)?;
    let graph = map_upsert_request(&request)?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;

    let persisted = project_store::upsert_workflow_graph(&repo_root, &request.project_id, &graph)?;
    let phases = project_store::load_project_snapshot(&repo_root, &request.project_id)?
        .snapshot
        .phases;

    emit_project_updated(
        &app,
        &repo_root,
        &request.project_id,
        ProjectUpdateReason::MetadataChanged,
    )?;

    Ok(UpsertWorkflowGraphResponseDto {
        nodes: persisted.nodes.into_iter().map(map_node_record).collect(),
        edges: persisted.edges.into_iter().map(map_edge_record).collect(),
        gates: persisted.gates.into_iter().map(map_gate_record).collect(),
        phases,
    })
}

fn validate_graph_size(request: &UpsertWorkflowGraphRequestDto) -> CommandResult<()> {
    if request.nodes.len() > MAX_GRAPH_NODE_COUNT {
        return Err(CommandError::user_fixable(
            "workflow_graph_request_too_large",
            format!(
                "Cadence only accepts up to {MAX_GRAPH_NODE_COUNT} workflow nodes per upsert request."
            ),
        ));
    }

    if request.edges.len() > MAX_GRAPH_EDGE_COUNT {
        return Err(CommandError::user_fixable(
            "workflow_graph_request_too_large",
            format!(
                "Cadence only accepts up to {MAX_GRAPH_EDGE_COUNT} workflow edges per upsert request."
            ),
        ));
    }

    if request.gates.len() > MAX_GRAPH_GATE_COUNT {
        return Err(CommandError::user_fixable(
            "workflow_graph_request_too_large",
            format!(
                "Cadence only accepts up to {MAX_GRAPH_GATE_COUNT} workflow gates per upsert request."
            ),
        ));
    }

    Ok(())
}

fn map_upsert_request(
    request: &UpsertWorkflowGraphRequestDto,
) -> CommandResult<WorkflowGraphUpsertRecord> {
    Ok(WorkflowGraphUpsertRecord {
        nodes: request.nodes.iter().map(map_node_dto).collect(),
        edges: request.edges.iter().map(map_edge_dto).collect(),
        gates: request
            .gates
            .iter()
            .map(map_gate_request)
            .collect::<CommandResult<Vec<_>>>()?,
    })
}

fn map_node_dto(dto: &WorkflowGraphNodeDto) -> project_store::WorkflowGraphNodeRecord {
    project_store::WorkflowGraphNodeRecord {
        node_id: dto.node_id.clone(),
        phase_id: dto.phase_id,
        sort_order: dto.sort_order,
        name: dto.name.clone(),
        description: dto.description.clone(),
        status: dto.status.clone(),
        current_step: dto.current_step.clone(),
        task_count: dto.task_count,
        completed_tasks: dto.completed_tasks,
        summary: dto.summary.clone(),
    }
}

fn map_edge_dto(dto: &WorkflowGraphEdgeDto) -> project_store::WorkflowGraphEdgeRecord {
    project_store::WorkflowGraphEdgeRecord {
        from_node_id: dto.from_node_id.clone(),
        to_node_id: dto.to_node_id.clone(),
        transition_kind: dto.transition_kind.clone(),
        gate_requirement: dto.gate_requirement.clone(),
    }
}

fn map_gate_request(
    dto: &WorkflowGraphGateRequestDto,
) -> CommandResult<project_store::WorkflowGateMetadataRecord> {
    Ok(project_store::WorkflowGateMetadataRecord {
        node_id: dto.node_id.clone(),
        gate_key: dto.gate_key.clone(),
        gate_state: parse_workflow_gate_state(&dto.gate_state, "workflow_graph_request_invalid")?,
        action_type: normalize_optional_non_empty(dto.action_type.clone(), "actionType")?,
        title: normalize_optional_non_empty(dto.title.clone(), "title")?,
        detail: normalize_optional_non_empty(dto.detail.clone(), "detail")?,
        decision_context: normalize_optional_non_empty(
            dto.decision_context.clone(),
            "decisionContext",
        )?,
    })
}

fn map_node_record(record: project_store::WorkflowGraphNodeRecord) -> WorkflowGraphNodeDto {
    WorkflowGraphNodeDto {
        node_id: record.node_id,
        phase_id: record.phase_id,
        sort_order: record.sort_order,
        name: record.name,
        description: record.description,
        status: record.status,
        current_step: record.current_step,
        task_count: record.task_count,
        completed_tasks: record.completed_tasks,
        summary: record.summary,
    }
}

fn map_edge_record(record: project_store::WorkflowGraphEdgeRecord) -> WorkflowGraphEdgeDto {
    WorkflowGraphEdgeDto {
        from_node_id: record.from_node_id,
        to_node_id: record.to_node_id,
        transition_kind: record.transition_kind,
        gate_requirement: record.gate_requirement,
    }
}

fn map_gate_record(record: project_store::WorkflowGateMetadataRecord) -> WorkflowGateMetadataDto {
    WorkflowGateMetadataDto {
        node_id: record.node_id,
        gate_key: record.gate_key,
        gate_state: map_workflow_gate_state(record.gate_state),
        action_type: record.action_type,
        title: record.title,
        detail: record.detail,
        decision_context: record.decision_context,
    }
}

fn parse_workflow_gate_state(
    value: &str,
    code: &'static str,
) -> CommandResult<project_store::WorkflowGateState> {
    match value.trim() {
        "pending" => Ok(project_store::WorkflowGateState::Pending),
        "satisfied" => Ok(project_store::WorkflowGateState::Satisfied),
        "blocked" => Ok(project_store::WorkflowGateState::Blocked),
        "skipped" => Ok(project_store::WorkflowGateState::Skipped),
        other => Err(CommandError::user_fixable(
            code,
            format!(
                "Cadence does not support workflow gate_state `{other}`. Allowed states: pending, satisfied, blocked, skipped."
            ),
        )),
    }
}

fn map_workflow_gate_state(value: project_store::WorkflowGateState) -> WorkflowGateStateDto {
    match value {
        project_store::WorkflowGateState::Pending => WorkflowGateStateDto::Pending,
        project_store::WorkflowGateState::Satisfied => WorkflowGateStateDto::Satisfied,
        project_store::WorkflowGateState::Blocked => WorkflowGateStateDto::Blocked,
        project_store::WorkflowGateState::Skipped => WorkflowGateStateDto::Skipped,
    }
}

fn normalize_optional_non_empty(
    value: Option<String>,
    field: &'static str,
) -> CommandResult<Option<String>> {
    match value {
        Some(value) if value.trim().is_empty() => Err(CommandError::invalid_request(field)),
        Some(value) => Ok(Some(value.trim().to_string())),
        None => Ok(None),
    }
}
