use serde::{Deserialize, Serialize};

use crate::db::project_store;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    Complete,
    Active,
    Pending,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStep {
    Discuss,
    Plan,
    Execute,
    Verify,
    Ship,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowGateStateDto {
    Pending,
    Satisfied,
    Blocked,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTransitionGateDecisionDto {
    Approved,
    Rejected,
    Blocked,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveOperatorActionRequestDto {
    pub project_id: String,
    pub action_id: String,
    pub decision: String,
    pub user_answer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeOperatorRunRequestDto {
    pub project_id: String,
    pub action_id: String,
    pub user_answer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowGraphNodeDto {
    pub node_id: String,
    pub phase_id: u32,
    pub sort_order: u32,
    pub name: String,
    pub description: String,
    pub status: PhaseStatus,
    pub current_step: Option<PhaseStep>,
    pub task_count: u32,
    pub completed_tasks: u32,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowGraphEdgeDto {
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub gate_requirement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowGraphGateRequestDto {
    pub node_id: String,
    pub gate_key: String,
    pub gate_state: String,
    pub action_type: Option<String>,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub decision_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowGateMetadataDto {
    pub node_id: String,
    pub gate_key: String,
    pub gate_state: WorkflowGateStateDto,
    pub action_type: Option<String>,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub decision_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertWorkflowGraphRequestDto {
    pub project_id: String,
    pub nodes: Vec<WorkflowGraphNodeDto>,
    pub edges: Vec<WorkflowGraphEdgeDto>,
    pub gates: Vec<WorkflowGraphGateRequestDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertWorkflowGraphResponseDto {
    pub nodes: Vec<WorkflowGraphNodeDto>,
    pub edges: Vec<WorkflowGraphEdgeDto>,
    pub gates: Vec<WorkflowGateMetadataDto>,
    pub phases: Vec<PhaseSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowTransitionGateUpdateRequestDto {
    pub gate_key: String,
    pub gate_state: String,
    pub decision_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyWorkflowTransitionRequestDto {
    pub project_id: String,
    pub transition_id: String,
    pub causal_transition_id: Option<String>,
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub gate_decision: String,
    pub gate_decision_context: Option<String>,
    pub gate_updates: Vec<WorkflowTransitionGateUpdateRequestDto>,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowTransitionEventDto {
    pub id: i64,
    pub transition_id: String,
    pub causal_transition_id: Option<String>,
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub gate_decision: WorkflowTransitionGateDecisionDto,
    pub gate_decision_context: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowHandoffPackageDto {
    pub id: i64,
    pub project_id: String,
    pub handoff_transition_id: String,
    pub causal_transition_id: Option<String>,
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub package_payload: String,
    pub package_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowAutomaticDispatchStatusDto {
    NoContinuation,
    Applied,
    Replayed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowAutomaticDispatchPackageStatusDto {
    Persisted,
    Replayed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAutomaticDispatchPackageOutcomeDto {
    pub status: WorkflowAutomaticDispatchPackageStatusDto,
    pub package: Option<WorkflowHandoffPackageDto>,
    pub code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAutomaticDispatchOutcomeDto {
    pub status: WorkflowAutomaticDispatchStatusDto,
    pub transition_event: Option<WorkflowTransitionEventDto>,
    pub handoff_package: Option<WorkflowAutomaticDispatchPackageOutcomeDto>,
    pub code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyWorkflowTransitionResponseDto {
    pub transition_event: WorkflowTransitionEventDto,
    pub automatic_dispatch: WorkflowAutomaticDispatchOutcomeDto,
    pub phases: Vec<PhaseSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhaseSummaryDto {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub status: PhaseStatus,
    pub current_step: Option<PhaseStep>,
    pub task_count: u32,
    pub completed_tasks: u32,
    pub summary: Option<String>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanningLifecycleStageKindDto {
    Discussion,
    Research,
    Requirements,
    Roadmap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningLifecycleStageDto {
    pub stage: PlanningLifecycleStageKindDto,
    pub node_id: String,
    pub status: PhaseStatus,
    pub action_required: bool,
    pub unblock_reason: Option<String>,
    pub unblock_gate_key: Option<String>,
    pub unblock_action_id: Option<String>,
    pub last_transition_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningLifecycleProjectionDto {
    pub stages: Vec<PlanningLifecycleStageDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperatorApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationRecordStatus {
    Pending,
    Passed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResumeHistoryStatus {
    Started,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperatorApprovalDto {
    pub action_id: String,
    pub session_id: Option<String>,
    pub flow_id: Option<String>,
    pub action_type: String,
    pub title: String,
    pub detail: String,
    pub gate_node_id: Option<String>,
    pub gate_key: Option<String>,
    pub transition_from_node_id: Option<String>,
    pub transition_to_node_id: Option<String>,
    pub transition_kind: Option<String>,
    pub user_answer: Option<String>,
    pub status: OperatorApprovalStatus,
    pub decision_note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationRecordDto {
    pub id: u32,
    pub source_action_id: Option<String>,
    pub status: VerificationRecordStatus,
    pub summary: String,
    pub detail: Option<String>,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeHistoryEntryDto {
    pub id: u32,
    pub source_action_id: Option<String>,
    pub session_id: Option<String>,
    pub status: ResumeHistoryStatus,
    pub summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveOperatorActionResponseDto {
    pub approval_request: OperatorApprovalDto,
    pub verification_record: VerificationRecordDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeOperatorRunResponseDto {
    pub approval_request: OperatorApprovalDto,
    pub resume_entry: ResumeHistoryEntryDto,
    pub automatic_dispatch: Option<WorkflowAutomaticDispatchOutcomeDto>,
}

pub(crate) fn map_workflow_transition_event_record(
    event: project_store::WorkflowTransitionEventRecord,
) -> WorkflowTransitionEventDto {
    WorkflowTransitionEventDto {
        id: event.id,
        transition_id: event.transition_id,
        causal_transition_id: event.causal_transition_id,
        from_node_id: event.from_node_id,
        to_node_id: event.to_node_id,
        transition_kind: event.transition_kind,
        gate_decision: map_transition_gate_decision(event.gate_decision),
        gate_decision_context: event.gate_decision_context,
        created_at: event.created_at,
    }
}

pub(crate) fn map_workflow_handoff_package_record(
    package: project_store::WorkflowHandoffPackageRecord,
) -> WorkflowHandoffPackageDto {
    WorkflowHandoffPackageDto {
        id: package.id,
        project_id: package.project_id,
        handoff_transition_id: package.handoff_transition_id,
        causal_transition_id: package.causal_transition_id,
        from_node_id: package.from_node_id,
        to_node_id: package.to_node_id,
        transition_kind: package.transition_kind,
        package_payload: package.package_payload,
        package_hash: package.package_hash,
        created_at: package.created_at,
    }
}

pub(crate) fn map_workflow_automatic_dispatch_outcome(
    outcome: project_store::WorkflowAutomaticDispatchOutcome,
) -> WorkflowAutomaticDispatchOutcomeDto {
    match outcome {
        project_store::WorkflowAutomaticDispatchOutcome::NoContinuation => {
            WorkflowAutomaticDispatchOutcomeDto {
                status: WorkflowAutomaticDispatchStatusDto::NoContinuation,
                transition_event: None,
                handoff_package: None,
                code: None,
                message: None,
            }
        }
        project_store::WorkflowAutomaticDispatchOutcome::Applied {
            transition_event,
            handoff_package,
        } => WorkflowAutomaticDispatchOutcomeDto {
            status: WorkflowAutomaticDispatchStatusDto::Applied,
            transition_event: Some(map_workflow_transition_event_record(transition_event)),
            handoff_package: Some(map_workflow_automatic_dispatch_package_outcome(
                handoff_package,
            )),
            code: None,
            message: None,
        },
        project_store::WorkflowAutomaticDispatchOutcome::Replayed {
            transition_event,
            handoff_package,
        } => WorkflowAutomaticDispatchOutcomeDto {
            status: WorkflowAutomaticDispatchStatusDto::Replayed,
            transition_event: Some(map_workflow_transition_event_record(transition_event)),
            handoff_package: Some(map_workflow_automatic_dispatch_package_outcome(
                handoff_package,
            )),
            code: None,
            message: None,
        },
        project_store::WorkflowAutomaticDispatchOutcome::Skipped { code, message } => {
            WorkflowAutomaticDispatchOutcomeDto {
                status: WorkflowAutomaticDispatchStatusDto::Skipped,
                transition_event: None,
                handoff_package: None,
                code: Some(code),
                message: Some(message),
            }
        }
    }
}

fn map_workflow_automatic_dispatch_package_outcome(
    outcome: project_store::WorkflowAutomaticDispatchPackageOutcome,
) -> WorkflowAutomaticDispatchPackageOutcomeDto {
    match outcome {
        project_store::WorkflowAutomaticDispatchPackageOutcome::Persisted { package } => {
            WorkflowAutomaticDispatchPackageOutcomeDto {
                status: WorkflowAutomaticDispatchPackageStatusDto::Persisted,
                package: Some(map_workflow_handoff_package_record(package)),
                code: None,
                message: None,
            }
        }
        project_store::WorkflowAutomaticDispatchPackageOutcome::Replayed { package } => {
            WorkflowAutomaticDispatchPackageOutcomeDto {
                status: WorkflowAutomaticDispatchPackageStatusDto::Replayed,
                package: Some(map_workflow_handoff_package_record(package)),
                code: None,
                message: None,
            }
        }
        project_store::WorkflowAutomaticDispatchPackageOutcome::Skipped { code, message } => {
            WorkflowAutomaticDispatchPackageOutcomeDto {
                status: WorkflowAutomaticDispatchPackageStatusDto::Skipped,
                package: None,
                code: Some(code),
                message: Some(message),
            }
        }
    }
}

fn map_transition_gate_decision(
    value: project_store::WorkflowTransitionGateDecision,
) -> WorkflowTransitionGateDecisionDto {
    match value {
        project_store::WorkflowTransitionGateDecision::Approved => {
            WorkflowTransitionGateDecisionDto::Approved
        }
        project_store::WorkflowTransitionGateDecision::Rejected => {
            WorkflowTransitionGateDecisionDto::Rejected
        }
        project_store::WorkflowTransitionGateDecision::Blocked => {
            WorkflowTransitionGateDecisionDto::Blocked
        }
        project_store::WorkflowTransitionGateDecision::NotApplicable => {
            WorkflowTransitionGateDecisionDto::NotApplicable
        }
    }
}
