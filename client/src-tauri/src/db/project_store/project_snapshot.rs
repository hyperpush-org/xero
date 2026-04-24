use std::path::{Path, PathBuf};

use rusqlite::{Connection, Error as SqlError};

use crate::{
    commands::{
        map_workflow_handoff_package_record, CommandError, OperatorApprovalDto,
        OperatorApprovalStatus, PhaseStatus, PhaseSummaryDto, PlanningLifecycleProjectionDto,
        PlanningLifecycleStageDto, PlanningLifecycleStageKindDto, ProjectSnapshotResponseDto,
        ProjectSummaryDto, RepositorySummaryDto,
    },
    db::database_path_for_repo,
};

use super::{
    map_snapshot_decode_error, open_project_database, parse_phase_status, parse_phase_step,
    read_operator_approvals, read_project_row, read_resume_history, read_verification_records,
    workflow::{
        read_transition_events, read_workflow_gate_metadata, read_workflow_graph_nodes,
        read_workflow_handoff_packages, WorkflowGateMetadataRecord, WorkflowGateState,
        WorkflowGraphNodeRecord, MAX_LIFECYCLE_TRANSITION_EVENT_ROWS,
    },
    ProjectSummaryRow,
};

#[derive(Debug, Clone)]
pub struct ProjectSnapshotRecord {
    pub snapshot: ProjectSnapshotResponseDto,
    pub database_path: PathBuf,
}

#[derive(Debug)]
struct ProjectProjection {
    project: ProjectSummaryDto,
    phases: Vec<PhaseSummaryDto>,
    lifecycle: PlanningLifecycleProjectionDto,
}

#[derive(Debug)]
struct RawPhaseRow {
    id: i64,
    name: String,
    description: String,
    status: String,
    current_step: Option<String>,
    task_count: i64,
    completed_tasks: i64,
    summary: Option<String>,
}

pub fn load_project_summary(
    repo_root: &Path,
    expected_project_id: &str,
) -> Result<ProjectSummaryDto, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;

    read_project_projection(&connection, &database_path, repo_root, expected_project_id)
        .map(|projection| projection.project)
}

pub fn load_project_snapshot(
    repo_root: &Path,
    expected_project_id: &str,
) -> Result<ProjectSnapshotRecord, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    let projection =
        read_project_projection(&connection, &database_path, repo_root, expected_project_id)?;
    let repository = read_repository_summary(&connection, &database_path, expected_project_id)?;
    let approval_requests =
        read_operator_approvals(&connection, &database_path, expected_project_id)?;
    let verification_records =
        read_verification_records(&connection, &database_path, expected_project_id)?;
    let resume_history = read_resume_history(&connection, &database_path, expected_project_id)?;
    let handoff_packages =
        read_workflow_handoff_packages(&connection, &database_path, expected_project_id, None)?
            .into_iter()
            .map(map_workflow_handoff_package_record)
            .collect();

    Ok(ProjectSnapshotRecord {
        snapshot: ProjectSnapshotResponseDto {
            project: projection.project,
            repository,
            phases: projection.phases,
            lifecycle: projection.lifecycle,
            approval_requests,
            verification_records,
            resume_history,
            handoff_packages,
            autonomous_run: None,
            autonomous_unit: None,
        },
        database_path,
    })
}

fn read_project_projection(
    connection: &Connection,
    database_path: &Path,
    repo_root: &Path,
    expected_project_id: &str,
) -> Result<ProjectProjection, CommandError> {
    let project_row = read_project_row(connection, database_path, repo_root, expected_project_id)?;
    let phases = read_phase_summaries(connection, database_path, expected_project_id)?;
    let lifecycle =
        read_planning_lifecycle_projection(connection, database_path, expected_project_id)?;

    Ok(ProjectProjection {
        project: derive_project_summary(project_row, &phases),
        phases,
        lifecycle,
    })
}

fn derive_project_summary(
    project_row: ProjectSummaryRow,
    phases: &[PhaseSummaryDto],
) -> ProjectSummaryDto {
    let total_phases = phases
        .iter()
        .fold(0_u32, |count, _| count.saturating_add(1));
    let completed_phases = phases.iter().fold(0_u32, |count, phase| {
        if phase.status == PhaseStatus::Complete {
            count.saturating_add(1)
        } else {
            count
        }
    });
    let active_phase = phases
        .iter()
        .find(|phase| phase.status == PhaseStatus::Active)
        .map_or(0, |phase| phase.id);

    ProjectSummaryDto {
        id: project_row.id,
        name: project_row.name,
        description: project_row.description,
        milestone: project_row.milestone,
        total_phases,
        completed_phases,
        active_phase,
        branch: project_row.branch,
        runtime: project_row.runtime,
    }
}

fn read_repository_summary(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Option<RepositorySummaryDto>, CommandError> {
    connection
        .query_row(
            r#"
            SELECT
                id,
                project_id,
                root_path,
                display_name,
                branch,
                head_sha,
                is_git_repo
            FROM repositories
            WHERE project_id = ?1
            ORDER BY updated_at DESC, created_at DESC
            LIMIT 1
            "#,
            [expected_project_id],
            |row| {
                Ok(RepositorySummaryDto {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    root_path: row.get(2)?,
                    display_name: row.get(3)?,
                    branch: row.get(4)?,
                    head_sha: row.get(5)?,
                    is_git_repo: row.get::<_, i64>(6)? == 1,
                })
            },
        )
        .map(Some)
        .or_else(|error| match error {
            SqlError::QueryReturnedNoRows => Ok(None),
            other => Err(CommandError::system_fault(
                "project_repository_query_failed",
                format!(
                    "Cadence could not read repository metadata from {}: {other}",
                    database_path.display()
                ),
            )),
        })
}

pub(crate) fn read_phase_summaries(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Vec<PhaseSummaryDto>, CommandError> {
    let graph_phases = read_graph_phase_summaries(connection, database_path, expected_project_id)?;
    if !graph_phases.is_empty() {
        return Ok(graph_phases);
    }

    read_legacy_phase_summaries(connection, database_path, expected_project_id)
}

pub(crate) fn read_planning_lifecycle_projection(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<PlanningLifecycleProjectionDto, CommandError> {
    let nodes = read_workflow_graph_nodes(connection, database_path, expected_project_id)?;
    if nodes.is_empty() {
        return Ok(PlanningLifecycleProjectionDto { stages: Vec::new() });
    }

    let gates = read_workflow_gate_metadata(connection, database_path, expected_project_id)?;
    let transitions = read_transition_events(
        connection,
        database_path,
        expected_project_id,
        Some(MAX_LIFECYCLE_TRANSITION_EVENT_ROWS),
    )?;

    let mut discussion_node: Option<&WorkflowGraphNodeRecord> = None;
    let mut research_node: Option<&WorkflowGraphNodeRecord> = None;
    let mut requirements_node: Option<&WorkflowGraphNodeRecord> = None;
    let mut roadmap_node: Option<&WorkflowGraphNodeRecord> = None;

    for node in &nodes {
        let Some(stage) = classify_planning_lifecycle_stage(&node.node_id) else {
            continue;
        };

        let slot = match stage {
            PlanningLifecycleStageKindDto::Discussion => &mut discussion_node,
            PlanningLifecycleStageKindDto::Research => &mut research_node,
            PlanningLifecycleStageKindDto::Requirements => &mut requirements_node,
            PlanningLifecycleStageKindDto::Roadmap => &mut roadmap_node,
        };

        if let Some(existing) = slot {
            return Err(map_snapshot_decode_error(
                "workflow_graph_decode_failed",
                database_path,
                format!(
                    "Planning lifecycle stage `{}` matched multiple workflow nodes (`{}` and `{}`).",
                    planning_lifecycle_stage_label(&stage),
                    existing.node_id,
                    node.node_id
                ),
            ));
        }

        *slot = Some(node);
    }

    let approvals = read_operator_approvals(connection, database_path, expected_project_id)?;

    let mut stages = Vec::new();
    for (stage, node) in [
        (PlanningLifecycleStageKindDto::Discussion, discussion_node),
        (PlanningLifecycleStageKindDto::Research, research_node),
        (
            PlanningLifecycleStageKindDto::Requirements,
            requirements_node,
        ),
        (PlanningLifecycleStageKindDto::Roadmap, roadmap_node),
    ] {
        let Some(node) = node else {
            continue;
        };

        let stage_blocking_gates: Vec<&WorkflowGateMetadataRecord> = gates
            .iter()
            .filter(|gate| {
                gate.node_id == node.node_id
                    && matches!(
                        gate.gate_state,
                        WorkflowGateState::Pending | WorkflowGateState::Blocked
                    )
            })
            .collect();
        let action_required = !stage_blocking_gates.is_empty();
        let (unblock_reason, unblock_gate_key, unblock_action_id) =
            derive_lifecycle_unblock_reason(stage, &node.node_id, &stage_blocking_gates, &approvals);

        stages.push(PlanningLifecycleStageDto {
            stage,
            node_id: node.node_id.clone(),
            status: node.status.clone(),
            action_required,
            unblock_reason,
            unblock_gate_key,
            unblock_action_id,
            last_transition_at: transitions
                .iter()
                .find(|event| {
                    event.from_node_id == node.node_id || event.to_node_id == node.node_id
                })
                .map(|event| event.created_at.clone()),
        });
    }

    Ok(PlanningLifecycleProjectionDto { stages })
}

fn derive_lifecycle_unblock_reason(
    stage: PlanningLifecycleStageKindDto,
    node_id: &str,
    stage_blocking_gates: &[&WorkflowGateMetadataRecord],
    approvals: &[OperatorApprovalDto],
) -> (Option<String>, Option<String>, Option<String>) {
    let Some(gate) = stage_blocking_gates.iter().copied().min_by(|left, right| {
        gate_priority(left)
            .cmp(&gate_priority(right))
            .then_with(|| left.gate_key.cmp(&right.gate_key))
    }) else {
        return (None, None, None);
    };

    let linked_approval = approvals.iter().find(|approval| {
        approval.gate_node_id.as_deref() == Some(node_id)
            && approval.gate_key.as_deref() == Some(gate.gate_key.as_str())
            && matches!(
                approval.status,
                OperatorApprovalStatus::Pending | OperatorApprovalStatus::Approved
            )
    });

    let stage_label = planning_lifecycle_stage_label(&stage);
    let reason = if gate.gate_key == super::workflow::PLAN_MODE_REQUIRED_GATE_KEY {
        if linked_approval.is_some_and(|approval| approval.status == OperatorApprovalStatus::Approved) {
            format!(
                "{stage_label} is waiting for an explicit resume after plan-mode approval."
            )
        } else if linked_approval.is_some() {
            format!(
                "{stage_label} requires explicit plan-mode approval before implementation can continue."
            )
        } else {
            format!(
                "{stage_label} remains blocked until the plan-mode continuation gate is resolved."
            )
        }
    } else if linked_approval.is_some_and(|approval| approval.status == OperatorApprovalStatus::Approved) {
        format!(
            "{stage_label} is waiting for resume after a gate approval was recorded."
        )
    } else if linked_approval.is_some() {
        format!(
            "{stage_label} is waiting on a linked operator gate approval."
        )
    } else if gate.gate_state == WorkflowGateState::Blocked {
        format!("{stage_label} is blocked by an unresolved workflow gate.")
    } else {
        format!("{stage_label} is waiting on a pending workflow gate.")
    };

    (
        Some(reason),
        Some(gate.gate_key.clone()),
        linked_approval.map(|approval| approval.action_id.clone()),
    )
}

fn gate_priority(gate: &WorkflowGateMetadataRecord) -> u8 {
    if gate.gate_key == super::workflow::PLAN_MODE_REQUIRED_GATE_KEY {
        return 0;
    }

    match gate.gate_state {
        WorkflowGateState::Blocked => 1,
        WorkflowGateState::Pending => 2,
        WorkflowGateState::Satisfied | WorkflowGateState::Skipped => 3,
    }
}

fn classify_planning_lifecycle_stage(node_id: &str) -> Option<PlanningLifecycleStageKindDto> {
    let normalized = node_id.trim().to_ascii_lowercase().replace('_', "-");

    match normalized.as_str() {
        "discussion"
        | "discuss"
        | "plan-discussion"
        | "planning-discussion"
        | "workflow-discussion"
        | "lifecycle-discussion" => Some(PlanningLifecycleStageKindDto::Discussion),
        "research" | "plan-research" | "planning-research" | "workflow-research"
        | "lifecycle-research" => Some(PlanningLifecycleStageKindDto::Research),
        "requirements"
        | "requirement"
        | "plan-requirements"
        | "planning-requirements"
        | "workflow-requirements"
        | "lifecycle-requirements" => Some(PlanningLifecycleStageKindDto::Requirements),
        "roadmap" | "plan-roadmap" | "planning-roadmap" | "workflow-roadmap"
        | "lifecycle-roadmap" => Some(PlanningLifecycleStageKindDto::Roadmap),
        _ => None,
    }
}

pub(crate) fn planning_lifecycle_stage_label(
    stage: &PlanningLifecycleStageKindDto,
) -> &'static str {
    match stage {
        PlanningLifecycleStageKindDto::Discussion => "discussion",
        PlanningLifecycleStageKindDto::Research => "research",
        PlanningLifecycleStageKindDto::Requirements => "requirements",
        PlanningLifecycleStageKindDto::Roadmap => "roadmap",
    }
}

fn read_graph_phase_summaries(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Vec<PhaseSummaryDto>, CommandError> {
    let nodes = read_workflow_graph_nodes(connection, database_path, expected_project_id)?;
    Ok(nodes
        .into_iter()
        .map(|node| PhaseSummaryDto {
            id: node.phase_id,
            name: node.name,
            description: node.description,
            status: node.status,
            current_step: node.current_step,
            task_count: node.task_count,
            completed_tasks: node.completed_tasks,
            summary: node.summary,
        })
        .collect())
}

fn read_legacy_phase_summaries(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Vec<PhaseSummaryDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                name,
                description,
                status,
                current_step,
                task_count,
                completed_tasks,
                summary
            FROM workflow_phases
            WHERE project_id = ?1
            ORDER BY id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "project_phase_query_failed",
                format!(
                    "Cadence could not prepare workflow phase rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map([expected_project_id], |row| {
            Ok(RawPhaseRow {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                status: row.get(3)?,
                current_step: row.get(4)?,
                task_count: row.get(5)?,
                completed_tasks: row.get(6)?,
                summary: row.get(7)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "project_phase_query_failed",
                format!(
                    "Cadence could not query workflow phase rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "project_phase_decode_failed",
                        format!(
                            "Cadence could not decode workflow phase rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_phase_row(raw_row, database_path))
        })
        .collect()
}

fn decode_phase_row(
    raw_row: RawPhaseRow,
    database_path: &Path,
) -> Result<PhaseSummaryDto, CommandError> {
    let phase_id = decode_phase_number(raw_row.id, "id", database_path, None)?;
    let task_count = decode_phase_number(
        raw_row.task_count,
        "task_count",
        database_path,
        Some(phase_id),
    )?;
    let completed_tasks = decode_phase_number(
        raw_row.completed_tasks,
        "completed_tasks",
        database_path,
        Some(phase_id),
    )?;

    if completed_tasks > task_count {
        return Err(map_phase_decode_error(
            database_path,
            Some(phase_id),
            format!(
                "Field `completed_tasks` cannot exceed `task_count` ({} > {}).",
                completed_tasks, task_count
            ),
        ));
    }

    let status = parse_phase_status(&raw_row.status)
        .map_err(|message| map_phase_decode_error(database_path, Some(phase_id), message))?;
    let current_step = raw_row
        .current_step
        .as_deref()
        .map(parse_phase_step)
        .transpose()
        .map_err(|message| map_phase_decode_error(database_path, Some(phase_id), message))?;

    Ok(PhaseSummaryDto {
        id: phase_id,
        name: raw_row.name,
        description: raw_row.description,
        status,
        current_step,
        task_count,
        completed_tasks,
        summary: raw_row.summary,
    })
}

fn decode_phase_number(
    value: i64,
    field: &str,
    database_path: &Path,
    phase_id: Option<u32>,
) -> Result<u32, CommandError> {
    u32::try_from(value).map_err(|_| {
        map_phase_decode_error(
            database_path,
            phase_id,
            format!("Field `{field}` must be a non-negative 32-bit integer, found {value}."),
        )
    })
}

fn map_phase_decode_error(
    database_path: &Path,
    phase_id: Option<u32>,
    details: String,
) -> CommandError {
    let phase_label = phase_id
        .map(|value| format!(" phase {}", value))
        .unwrap_or_default();

    CommandError::system_fault(
        "project_phase_decode_failed",
        format!(
            "Cadence could not decode workflow{phase_label} from {}: {details}",
            database_path.display()
        ),
    )
}
