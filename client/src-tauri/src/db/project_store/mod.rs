use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection, Error as SqlError, ErrorCode, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    commands::{
        CommandError, CommandErrorClass, OperatorApprovalDto, OperatorApprovalStatus, PhaseStatus,
        PhaseStep, PhaseSummaryDto, PlanningLifecycleProjectionDto, PlanningLifecycleStageDto,
        PlanningLifecycleStageKindDto, ProjectSnapshotResponseDto, ProjectSummaryDto,
        RepositorySummaryDto, ResumeHistoryEntryDto, ResumeHistoryStatus, RuntimeAuthPhase,
        VerificationRecordDto, VerificationRecordStatus, WorkflowHandoffPackageDto,
    },
    db::database_path_for_repo,
    notifications::{
        route_target::parse_notification_route_target_for_kind, NotificationRouteKind,
    },
    runtime::protocol::{GitToolResultScope, ToolResultSummary},
};

mod autonomous;
mod connection;
mod project_snapshot;
mod runtime;

pub use autonomous::*;
pub(crate) use connection::{open_project_database, open_runtime_database};
pub use project_snapshot::{load_project_snapshot, load_project_summary};
pub(crate) use project_snapshot::{
    planning_lifecycle_stage_label, read_phase_summaries, read_planning_lifecycle_projection,
};
pub use runtime::*;
pub(crate) use runtime::{
    find_prohibited_runtime_persistence_content, normalize_runtime_checkpoint_summary,
    read_runtime_run_row, read_runtime_run_snapshot, read_runtime_session_row,
    runtime_run_checkpoint_kind_sql_value, validate_runtime_action_required_payload,
};

const MAX_APPROVAL_REQUEST_ROWS: i64 = 50;
const MAX_VERIFICATION_RECORD_ROWS: i64 = 100;
const MAX_RESUME_HISTORY_ROWS: i64 = 100;
const MAX_RUNTIME_RUN_CHECKPOINT_ROWS: i64 = 32;
const MAX_RUNTIME_RUN_CHECKPOINT_SUMMARY_CHARS: usize = 280;
const MAX_AUTONOMOUS_HISTORY_UNIT_ROWS: i64 = 16;
const MAX_AUTONOMOUS_HISTORY_ATTEMPT_ROWS: i64 = 32;
const MAX_AUTONOMOUS_HISTORY_ARTIFACT_ROWS: i64 = 64;
const AUTONOMOUS_ARTIFACT_KIND_TOOL_RESULT: &str = "tool_result";
const AUTONOMOUS_ARTIFACT_KIND_VERIFICATION_EVIDENCE: &str = "verification_evidence";
const AUTONOMOUS_ARTIFACT_KIND_POLICY_DENIED: &str = "policy_denied";
const AUTONOMOUS_ARTIFACT_KIND_SKILL_LIFECYCLE: &str = "skill_lifecycle";
const MAX_WORKFLOW_TRANSITION_EVENT_ROWS: i64 = 200;
const MAX_WORKFLOW_HANDOFF_PACKAGE_ROWS: i64 = 200;
const MAX_LIFECYCLE_TRANSITION_EVENT_ROWS: i64 = 64;
const MAX_NOTIFICATION_ROUTE_ROWS: i64 = 128;
const MAX_NOTIFICATION_DISPATCH_ROWS: i64 = 256;
const MAX_NOTIFICATION_PENDING_DISPATCH_BATCH_ROWS: i64 = 64;
const MAX_NOTIFICATION_REPLY_CLAIM_ROWS: i64 = 512;
const WORKFLOW_HANDOFF_PACKAGE_SCHEMA_VERSION: u32 = 1;
const RUNTIME_RUN_STALE_AFTER_SECONDS: i64 = 45;
const NOTIFICATION_CORRELATION_KEY_PREFIX: &str = "nfy";
const NOTIFICATION_CORRELATION_KEY_HEX_LEN: usize = 32;

#[derive(Debug, Clone)]
pub struct ProjectSnapshotRecord {
    pub snapshot: ProjectSnapshotResponseDto,
    pub database_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorApprovalDecision {
    Approved,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveOperatorActionRecord {
    pub approval_request: OperatorApprovalDto,
    pub verification_record: VerificationRecordDto,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowGateState {
    Pending,
    Satisfied,
    Blocked,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowTransitionGateDecision {
    Approved,
    Rejected,
    Blocked,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowGraphNodeRecord {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowGraphEdgeRecord {
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub gate_requirement: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowGateMetadataRecord {
    pub node_id: String,
    pub gate_key: String,
    pub gate_state: WorkflowGateState,
    pub action_type: Option<String>,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub decision_context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowTransitionEventRecord {
    pub id: i64,
    pub transition_id: String,
    pub causal_transition_id: Option<String>,
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub gate_decision: WorkflowTransitionGateDecision,
    pub gate_decision_context: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowHandoffPackageRecord {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowHandoffPackageUpsertRecord {
    pub project_id: String,
    pub handoff_transition_id: String,
    pub causal_transition_id: Option<String>,
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub package_payload: String,
    pub created_at: String,
}

fn map_workflow_handoff_package_record(
    record: WorkflowHandoffPackageRecord,
) -> WorkflowHandoffPackageDto {
    WorkflowHandoffPackageDto {
        id: record.id,
        project_id: record.project_id,
        handoff_transition_id: record.handoff_transition_id,
        causal_transition_id: record.causal_transition_id,
        from_node_id: record.from_node_id,
        to_node_id: record.to_node_id,
        transition_kind: record.transition_kind,
        package_payload: record.package_payload,
        package_hash: record.package_hash,
        created_at: record.created_at,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowHandoffPackagePayload {
    pub schema_version: u32,
    pub trigger_transition: WorkflowHandoffTriggerTransitionPayload,
    pub destination_state: WorkflowHandoffDestinationStatePayload,
    pub lifecycle_projection: WorkflowHandoffLifecycleProjectionPayload,
    pub operator_continuity: WorkflowHandoffOperatorContinuityPayload,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowHandoffTriggerTransitionPayload {
    pub transition_id: String,
    pub causal_transition_id: Option<String>,
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub gate_decision: String,
    pub gate_decision_context_present: bool,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowHandoffDestinationStatePayload {
    pub node_id: String,
    pub phase_id: u32,
    pub sort_order: u32,
    pub name: String,
    pub description: String,
    pub status: PhaseStatus,
    pub current_step: Option<PhaseStep>,
    pub task_count: u32,
    pub completed_tasks: u32,
    pub pending_gate_count: u32,
    pub gates: Vec<WorkflowHandoffDestinationGatePayload>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowHandoffDestinationGatePayload {
    pub gate_key: String,
    pub gate_state: String,
    pub action_type: Option<String>,
    pub detail_present: bool,
    pub decision_context_present: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowHandoffLifecycleProjectionPayload {
    pub stages: Vec<PlanningLifecycleStageDto>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowHandoffOperatorContinuityPayload {
    pub pending_gate_action_count: u32,
    pub pending_gate_actions: Vec<WorkflowHandoffPendingGateActionPayload>,
    pub latest_resume: Option<WorkflowHandoffLatestResumePayload>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowHandoffPendingGateActionPayload {
    pub action_id: String,
    pub action_type: String,
    pub gate_node_id: String,
    pub gate_key: String,
    pub transition_from_node_id: String,
    pub transition_to_node_id: String,
    pub transition_kind: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowHandoffLatestResumePayload {
    pub source_action_id: Option<String>,
    pub status: ResumeHistoryStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowAutomaticDispatchPackageOutcome {
    Persisted {
        package: WorkflowHandoffPackageRecord,
    },
    Replayed {
        package: WorkflowHandoffPackageRecord,
    },
    Skipped {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowAutomaticDispatchOutcome {
    NoContinuation,
    Applied {
        transition_event: WorkflowTransitionEventRecord,
        handoff_package: WorkflowAutomaticDispatchPackageOutcome,
    },
    Replayed {
        transition_event: WorkflowTransitionEventRecord,
        handoff_package: WorkflowAutomaticDispatchPackageOutcome,
    },
    Skipped {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowGraphRecord {
    pub nodes: Vec<WorkflowGraphNodeRecord>,
    pub edges: Vec<WorkflowGraphEdgeRecord>,
    pub gates: Vec<WorkflowGateMetadataRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowGraphUpsertRecord {
    pub nodes: Vec<WorkflowGraphNodeRecord>,
    pub edges: Vec<WorkflowGraphEdgeRecord>,
    pub gates: Vec<WorkflowGateMetadataRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowGateDecisionUpdate {
    pub gate_key: String,
    pub gate_state: WorkflowGateState,
    pub decision_context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyWorkflowTransitionRecord {
    pub transition_id: String,
    pub causal_transition_id: Option<String>,
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub gate_decision: WorkflowTransitionGateDecision,
    pub gate_decision_context: Option<String>,
    pub gate_updates: Vec<WorkflowGateDecisionUpdate>,
    pub occurred_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyWorkflowTransitionResult {
    pub transition_event: WorkflowTransitionEventRecord,
    pub automatic_dispatch: WorkflowAutomaticDispatchOutcome,
    pub phases: Vec<PhaseSummaryDto>,
}

#[derive(Debug)]
struct ProjectSummaryRow {
    id: String,
    name: String,
    description: String,
    milestone: String,
    branch: Option<String>,
    runtime: Option<String>,
}

#[derive(Debug)]
struct RawGraphNodeRow {
    node_id: String,
    phase_id: i64,
    sort_order: i64,
    name: String,
    description: String,
    status: String,
    current_step: Option<String>,
    task_count: i64,
    completed_tasks: i64,
    summary: Option<String>,
}

#[derive(Debug)]
struct RawGraphEdgeRow {
    from_node_id: String,
    to_node_id: String,
    transition_kind: String,
    gate_requirement: Option<String>,
}

#[derive(Debug)]
struct RawGateMetadataRow {
    node_id: String,
    gate_key: String,
    gate_state: String,
    action_type: Option<String>,
    title: Option<String>,
    detail: Option<String>,
    decision_context: Option<String>,
}

#[derive(Debug)]
struct RawTransitionEventRow {
    id: i64,
    transition_id: String,
    causal_transition_id: Option<String>,
    from_node_id: String,
    to_node_id: String,
    transition_kind: String,
    gate_decision: String,
    gate_decision_context: Option<String>,
    created_at: String,
}

#[derive(Debug)]
struct RawWorkflowHandoffPackageRow {
    id: i64,
    project_id: String,
    handoff_transition_id: String,
    causal_transition_id: Option<String>,
    from_node_id: String,
    to_node_id: String,
    transition_kind: String,
    package_payload: String,
    package_hash: String,
    created_at: String,
}

#[derive(Debug)]
struct RawNotificationRouteRow {
    project_id: String,
    route_id: String,
    route_kind: String,
    route_target: String,
    enabled: i64,
    metadata_json: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug)]
struct RawNotificationDispatchRow {
    id: i64,
    project_id: String,
    action_id: String,
    route_id: String,
    correlation_key: String,
    status: String,
    attempt_count: i64,
    last_attempt_at: Option<String>,
    delivered_at: Option<String>,
    claimed_at: Option<String>,
    last_error_code: Option<String>,
    last_error_message: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug)]
struct RawNotificationReplyClaimRow {
    id: i64,
    project_id: String,
    action_id: String,
    route_id: String,
    correlation_key: String,
    responder_id: Option<String>,
    reply_text: String,
    status: String,
    rejection_code: Option<String>,
    rejection_message: Option<String>,
    created_at: String,
}

#[derive(Debug)]
struct RawOperatorApprovalRow {
    action_id: String,
    session_id: Option<String>,
    flow_id: Option<String>,
    action_type: String,
    title: String,
    detail: String,
    gate_node_id: Option<String>,
    gate_key: Option<String>,
    transition_from_node_id: Option<String>,
    transition_to_node_id: Option<String>,
    transition_kind: Option<String>,
    user_answer: Option<String>,
    status: String,
    decision_note: Option<String>,
    created_at: String,
    updated_at: String,
    resolved_at: Option<String>,
}

#[derive(Debug)]
struct RawVerificationRecordRow {
    id: i64,
    source_action_id: Option<String>,
    status: String,
    summary: String,
    detail: Option<String>,
    recorded_at: String,
}

#[derive(Debug)]
struct RawResumeHistoryRow {
    id: i64,
    source_action_id: Option<String>,
    session_id: Option<String>,
    status: String,
    summary: String,
    created_at: String,
}

#[derive(Debug, Clone)]
struct OperatorApprovalGateCandidate {
    node_id: String,
    gate_key: String,
    title: String,
    detail: String,
}

#[derive(Debug, Clone)]
struct OperatorApprovalGateLink {
    gate_node_id: String,
    gate_key: String,
    transition_from_node_id: String,
    transition_to_node_id: String,
    transition_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorApprovalGateLinkInput {
    pub gate_node_id: String,
    pub gate_key: String,
    pub transition_from_node_id: String,
    pub transition_to_node_id: String,
    pub transition_kind: String,
}

#[derive(Debug, Clone)]
struct OperatorResumeTransitionContext {
    gate_node_id: String,
    gate_key: String,
    transition_from_node_id: String,
    transition_to_node_id: String,
    transition_kind: String,
    user_answer: String,
}

#[derive(Debug, Clone)]
struct RuntimeOperatorResumeTarget {
    run_id: String,
    boundary_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolveOperatorAnswerRequirement {
    GateLinked,
    RuntimeResumable,
}

type WorkflowTransitionSqlErrorMapper = fn(&str, &Path, SqlError, &str) -> CommandError;

#[derive(Debug, Clone)]
struct WorkflowTransitionGateMutationRecord {
    node_id: String,
    gate_key: String,
    gate_state: WorkflowGateState,
    decision_context: Option<String>,
    require_pending_or_blocked: bool,
}

#[derive(Debug, Clone)]
struct WorkflowTransitionMutationRecord {
    transition_id: String,
    causal_transition_id: Option<String>,
    from_node_id: String,
    to_node_id: String,
    transition_kind: String,
    gate_decision: WorkflowTransitionGateDecision,
    gate_decision_context: Option<String>,
    gate_updates: Vec<WorkflowTransitionGateMutationRecord>,
    required_gate_requirement: Option<String>,
    occurred_at: String,
}

#[derive(Debug, Clone)]
struct WorkflowAutomaticDispatchCandidate {
    from_node_id: String,
    to_node_id: String,
    transition_kind: String,
    gate_requirement: Option<String>,
}

#[derive(Debug, Clone)]
struct WorkflowAutomaticDispatchUnresolvedGateCandidate {
    gate_node_id: String,
    gate_key: String,
    gate_state: WorkflowGateState,
    action_type: Option<String>,
    title: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, Clone)]
struct WorkflowAutomaticDispatchUnresolvedContinuationCandidate {
    from_node_id: String,
    to_node_id: String,
    transition_kind: String,
    gate_requirement: Option<String>,
    unresolved_gates: Vec<WorkflowAutomaticDispatchUnresolvedGateCandidate>,
}

#[derive(Debug, Clone)]
enum WorkflowAutomaticDispatchCandidateResolution {
    NoContinuation,
    Candidate(WorkflowAutomaticDispatchCandidate),
    Unresolved {
        completed_node_id: String,
        blocked_candidates: Vec<WorkflowAutomaticDispatchUnresolvedContinuationCandidate>,
    },
}

#[derive(Debug, Clone)]
enum WorkflowTransitionMutationApplyOutcome {
    Applied,
    Replayed(WorkflowTransitionEventRecord),
}

#[derive(Debug, Clone, Copy)]
struct WorkflowTransitionMutationErrorProfile {
    edge_check_failed_code: &'static str,
    edge_check_failed_message: &'static str,
    gate_update_failed_code: &'static str,
    gate_update_failed_message: &'static str,
    gate_check_failed_code: &'static str,
    gate_check_failed_message: &'static str,
    source_update_failed_code: &'static str,
    source_update_failed_message: &'static str,
    target_update_failed_code: &'static str,
    target_update_failed_message: &'static str,
    event_persist_failed_code: &'static str,
    event_persist_failed_message: &'static str,
}

const WORKFLOW_TRANSITION_COMMAND_MUTATION_ERROR_PROFILE: WorkflowTransitionMutationErrorProfile =
    WorkflowTransitionMutationErrorProfile {
        edge_check_failed_code: "workflow_transition_edge_check_failed",
        edge_check_failed_message: "Cadence could not verify workflow transition edge legality.",
        gate_update_failed_code: "workflow_transition_gate_update_failed",
        gate_update_failed_message: "Cadence could not persist workflow gate decisions.",
        gate_check_failed_code: "workflow_transition_gate_check_failed",
        gate_check_failed_message:
            "Cadence could not verify workflow gate state before transition.",
        source_update_failed_code: "workflow_transition_source_update_failed",
        source_update_failed_message: "Cadence could not update workflow source-node state.",
        target_update_failed_code: "workflow_transition_target_update_failed",
        target_update_failed_message: "Cadence could not update workflow target-node state.",
        event_persist_failed_code: "workflow_transition_event_persist_failed",
        event_persist_failed_message: "Cadence could not persist the workflow transition event.",
    };

const OPERATOR_RESUME_MUTATION_ERROR_PROFILE: WorkflowTransitionMutationErrorProfile =
    WorkflowTransitionMutationErrorProfile {
        edge_check_failed_code: "operator_resume_transition_edge_check_failed",
        edge_check_failed_message:
            "Cadence could not verify gate-linked resume transition legality.",
        gate_update_failed_code: "operator_resume_gate_update_failed",
        gate_update_failed_message:
            "Cadence could not persist the approved gate decision during resume.",
        gate_check_failed_code: "operator_resume_gate_check_failed",
        gate_check_failed_message:
            "Cadence could not verify workflow gate state before resume transition.",
        source_update_failed_code: "operator_resume_source_update_failed",
        source_update_failed_message:
            "Cadence could not update workflow source-node state during resume.",
        target_update_failed_code: "operator_resume_target_update_failed",
        target_update_failed_message:
            "Cadence could not update workflow target-node state during resume.",
        event_persist_failed_code: "operator_resume_transition_event_persist_failed",
        event_persist_failed_message:
            "Cadence could not persist the resume-caused workflow transition event.",
    };

const WORKFLOW_AUTOMATIC_DISPATCH_MUTATION_ERROR_PROFILE: WorkflowTransitionMutationErrorProfile =
    WorkflowTransitionMutationErrorProfile {
        edge_check_failed_code: "workflow_transition_auto_dispatch_edge_check_failed",
        edge_check_failed_message:
            "Cadence could not verify automatic workflow dispatch edge legality.",
        gate_update_failed_code: "workflow_transition_auto_dispatch_gate_update_failed",
        gate_update_failed_message: "Cadence could not persist automatic workflow gate updates.",
        gate_check_failed_code: "workflow_transition_auto_dispatch_gate_check_failed",
        gate_check_failed_message:
            "Cadence could not verify workflow gate state before automatic dispatch.",
        source_update_failed_code: "workflow_transition_auto_dispatch_source_update_failed",
        source_update_failed_message:
            "Cadence could not update automatic-dispatch source node state.",
        target_update_failed_code: "workflow_transition_auto_dispatch_target_update_failed",
        target_update_failed_message:
            "Cadence could not update automatic-dispatch target node state.",
        event_persist_failed_code: "workflow_transition_auto_dispatch_event_persist_failed",
        event_persist_failed_message:
            "Cadence could not persist the automatic workflow transition event.",
    };

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
    match existing {
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
        }
        Some(approval) => match approval.status {
            OperatorApprovalStatus::Pending => {
                transaction
                    .execute(
                        r#"
                        UPDATE operator_approvals
                        SET session_id = ?3,
                            flow_id = ?4,
                            title = ?5,
                            detail = ?6,
                            updated_at = ?7
                        WHERE project_id = ?1
                          AND action_id = ?2
                          AND status = 'pending'
                        "#,
                        params![
                            payload.project_id.as_str(),
                            action_id.as_str(),
                            payload.session_id.as_str(),
                            payload.flow_id.as_deref(),
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
                            "Cadence could not refresh the runtime action-required approval row.",
                        )
                    })?;
            }
            OperatorApprovalStatus::Approved | OperatorApprovalStatus::Rejected => {
                return Err(CommandError::retryable(
                    "runtime_action_sync_conflict",
                    format!(
                        "Cadence received a retained runtime action for already-resolved operator request `{action_id}`. Refresh selected project state before retrying."
                    ),
                ));
            }
        },
    }

    let next_sequence = runtime_row.last_checkpoint_sequence.saturating_add(1);
    let (last_error_code, last_error_message) = payload
        .last_error
        .as_ref()
        .map(|error| (Some(error.code.as_str()), Some(error.message.as_str())))
        .unwrap_or((None, None));

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
                runtime_run_checkpoint_kind_sql_value(&RuntimeRunCheckpointKind::ActionRequired),
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

pub fn upsert_pending_operator_approval(
    repo_root: &Path,
    project_id: &str,
    session_id: &str,
    flow_id: Option<&str>,
    action_type: &str,
    title: &str,
    detail: &str,
    created_at: &str,
) -> Result<OperatorApprovalDto, CommandError> {
    upsert_pending_operator_approval_with_gate_link(
        repo_root,
        project_id,
        session_id,
        flow_id,
        action_type,
        title,
        detail,
        created_at,
        None,
    )
}

pub fn upsert_pending_operator_approval_with_gate_link(
    repo_root: &Path,
    project_id: &str,
    session_id: &str,
    flow_id: Option<&str>,
    action_type: &str,
    title: &str,
    detail: &str,
    created_at: &str,
    gate_link: Option<&OperatorApprovalGateLinkInput>,
) -> Result<OperatorApprovalDto, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "operator_approval_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the operator-approval transaction.",
        )
    })?;

    let gate_link = match gate_link {
        Some(gate_link) => Some(validate_operator_approval_gate_link_input(
            gate_link,
            action_type,
        )?),
        None => resolve_operator_approval_gate_link(
            &transaction,
            &database_path,
            project_id,
            action_type,
            title,
            detail,
        )?,
    };
    let action_id =
        derive_operator_action_id(session_id, flow_id, action_type, gate_link.as_ref())?;

    let existing =
        read_operator_approval_by_action_id(&transaction, &database_path, project_id, &action_id)?;
    match existing {
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
                        ?8,
                        ?9,
                        ?10,
                        ?11,
                        ?12,
                        NULL,
                        'pending',
                        NULL,
                        ?13,
                        ?13,
                        NULL
                    )
                    "#,
                    params![
                        project_id,
                        action_id,
                        session_id,
                        flow_id,
                        action_type,
                        title,
                        detail,
                        gate_link.as_ref().map(|link| link.gate_node_id.as_str()),
                        gate_link.as_ref().map(|link| link.gate_key.as_str()),
                        gate_link
                            .as_ref()
                            .map(|link| link.transition_from_node_id.as_str()),
                        gate_link
                            .as_ref()
                            .map(|link| link.transition_to_node_id.as_str()),
                        gate_link.as_ref().map(|link| link.transition_kind.as_str()),
                        created_at,
                    ],
                )
                .map_err(|error| {
                    map_operator_loop_write_error(
                        "operator_approval_upsert_failed",
                        &database_path,
                        error,
                        "Cadence could not persist the pending operator approval.",
                    )
                })?;
        }
        Some(approval) => match approval.status {
            OperatorApprovalStatus::Pending => {
                transaction
                    .execute(
                        r#"
                        UPDATE operator_approvals
                        SET session_id = ?3,
                            flow_id = ?4,
                            title = ?5,
                            detail = ?6,
                            gate_node_id = ?7,
                            gate_key = ?8,
                            transition_from_node_id = ?9,
                            transition_to_node_id = ?10,
                            transition_kind = ?11,
                            updated_at = ?12
                        WHERE project_id = ?1
                          AND action_id = ?2
                          AND status = 'pending'
                        "#,
                        params![
                            project_id,
                            action_id,
                            session_id,
                            flow_id,
                            title,
                            detail,
                            gate_link.as_ref().map(|link| link.gate_node_id.as_str()),
                            gate_link.as_ref().map(|link| link.gate_key.as_str()),
                            gate_link
                                .as_ref()
                                .map(|link| link.transition_from_node_id.as_str()),
                            gate_link
                                .as_ref()
                                .map(|link| link.transition_to_node_id.as_str()),
                            gate_link.as_ref().map(|link| link.transition_kind.as_str()),
                            created_at,
                        ],
                    )
                    .map_err(|error| {
                        map_operator_loop_write_error(
                            "operator_approval_upsert_failed",
                            &database_path,
                            error,
                            "Cadence could not refresh the pending operator approval.",
                        )
                    })?;
            }
            OperatorApprovalStatus::Approved | OperatorApprovalStatus::Rejected => {
                return Err(CommandError::retryable(
                    "runtime_action_sync_conflict",
                    format!(
                        "Cadence received a retained runtime action for already-resolved operator request `{action_id}`. Reopen or refresh the selected project before retrying."
                    ),
                ));
            }
        },
    }

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "operator_approval_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the pending operator approval.",
        )
    })?;

    let _ = enqueue_notification_dispatches_best_effort_with_connection(
        &connection,
        &database_path,
        &NotificationDispatchEnqueueRecord {
            project_id: project_id.to_string(),
            action_id: action_id.clone(),
            enqueued_at: created_at.to_string(),
        },
    );

    read_operator_approval_by_action_id(&connection, &database_path, project_id, &action_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "operator_approval_missing_after_persist",
            format!(
                "Cadence persisted operator approval `{action_id}` in {} but could not read it back.",
                database_path.display()
            ),
        )
    })
}

pub fn upsert_notification_route(
    repo_root: &Path,
    route: &NotificationRouteUpsertRecord,
) -> Result<NotificationRouteRecord, CommandError> {
    let validated_route = validate_notification_route_upsert_payload(route)?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &route.project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "notification_route_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the notification-route transaction.",
        )
    })?;

    let metadata_json = normalize_optional_notification_metadata_json(
        route.metadata_json.as_deref(),
        "notification_route_request_invalid",
    )?;

    transaction
        .execute(
            r#"
            INSERT INTO notification_routes (
                project_id,
                route_id,
                route_kind,
                route_target,
                enabled,
                metadata_json,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
            ON CONFLICT(project_id, route_id) DO UPDATE SET
                route_kind = excluded.route_kind,
                route_target = excluded.route_target,
                enabled = excluded.enabled,
                metadata_json = excluded.metadata_json,
                updated_at = excluded.updated_at
            "#,
            params![
                route.project_id.as_str(),
                route.route_id.as_str(),
                validated_route.route_kind.as_str(),
                validated_route.canonical_route_target.as_str(),
                if route.enabled { 1_i64 } else { 0_i64 },
                metadata_json.as_deref(),
                route.updated_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "notification_route_upsert_failed",
                &database_path,
                error,
                "Cadence could not persist notification-route metadata.",
            )
        })?;

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "notification_route_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the notification-route transaction.",
        )
    })?;

    read_notification_route_by_id(
        &connection,
        &database_path,
        &route.project_id,
        &route.route_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "notification_route_missing_after_persist",
            format!(
                "Cadence persisted notification route `{}` in {} but could not read it back.",
                route.route_id,
                database_path.display()
            ),
        )
    })
}

pub fn load_notification_routes(
    repo_root: &Path,
    project_id: &str,
) -> Result<Vec<NotificationRouteRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "project_id",
        "notification_route_request_invalid",
    )?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    read_notification_routes(&connection, &database_path, project_id)
}

pub fn enqueue_notification_dispatches(
    repo_root: &Path,
    enqueue: &NotificationDispatchEnqueueRecord,
) -> Result<Vec<NotificationDispatchRecord>, CommandError> {
    validate_notification_dispatch_enqueue_payload(enqueue)?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &enqueue.project_id)?;

    enqueue_notification_dispatches_with_connection(&connection, &database_path, enqueue)
}

fn enqueue_notification_dispatches_with_connection(
    connection: &Connection,
    database_path: &Path,
    enqueue: &NotificationDispatchEnqueueRecord,
) -> Result<Vec<NotificationDispatchRecord>, CommandError> {
    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "notification_dispatch_transaction_failed",
            database_path,
            error,
            "Cadence could not start the notification-dispatch enqueue transaction.",
        )
    })?;

    let approval = read_operator_approval_by_action_id(
        &transaction,
        database_path,
        &enqueue.project_id,
        &enqueue.action_id,
    )?
    .ok_or_else(|| {
        CommandError::user_fixable(
            "notification_dispatch_action_not_found",
            format!(
                "Cadence could not enqueue notification dispatches because operator action `{}` was not found for project `{}`.",
                enqueue.action_id, enqueue.project_id
            ),
        )
    })?;

    if approval.status != OperatorApprovalStatus::Pending {
        return Err(CommandError::user_fixable(
            "notification_dispatch_action_not_pending",
            format!(
                "Cadence can only enqueue notification dispatches for pending operator actions. Action `{}` is currently {}.",
                enqueue.action_id,
                operator_approval_status_label(&approval.status)
            ),
        ));
    }

    let routes = read_notification_routes(&transaction, database_path, &enqueue.project_id)?;

    for route in routes.iter().filter(|route| route.enabled) {
        let correlation_key = derive_notification_correlation_key(
            &enqueue.project_id,
            &enqueue.action_id,
            &route.route_id,
        );

        transaction
            .execute(
                r#"
                INSERT INTO notification_dispatches (
                    project_id,
                    action_id,
                    route_id,
                    correlation_key,
                    status,
                    attempt_count,
                    last_attempt_at,
                    delivered_at,
                    claimed_at,
                    last_error_code,
                    last_error_message,
                    created_at,
                    updated_at
                )
                VALUES (
                    ?1,
                    ?2,
                    ?3,
                    ?4,
                    'pending',
                    0,
                    NULL,
                    NULL,
                    NULL,
                    NULL,
                    NULL,
                    ?5,
                    ?5
                )
                ON CONFLICT(project_id, action_id, route_id) DO NOTHING
                "#,
                params![
                    enqueue.project_id.as_str(),
                    enqueue.action_id.as_str(),
                    route.route_id.as_str(),
                    correlation_key.as_str(),
                    enqueue.enqueued_at.as_str(),
                ],
            )
            .map_err(|error| {
                map_operator_loop_write_error(
                    "notification_dispatch_enqueue_failed",
                    database_path,
                    error,
                    "Cadence could not persist notification dispatch fan-out rows.",
                )
            })?;
    }

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "notification_dispatch_commit_failed",
            database_path,
            error,
            "Cadence could not commit notification dispatch fan-out rows.",
        )
    })?;

    read_notification_dispatches(
        connection,
        database_path,
        &enqueue.project_id,
        Some(&enqueue.action_id),
    )
}

fn enqueue_notification_dispatches_best_effort_with_connection(
    connection: &Connection,
    database_path: &Path,
    enqueue: &NotificationDispatchEnqueueRecord,
) -> NotificationDispatchEnqueueOutcomeRecord {
    match enqueue_notification_dispatches_with_connection(connection, database_path, enqueue) {
        Ok(dispatches) if dispatches.is_empty() => NotificationDispatchEnqueueOutcomeRecord {
            status: NotificationDispatchEnqueueStatus::Skipped,
            dispatch_count: 0,
            code: Some("notification_dispatch_enqueue_skipped".into()),
            message: Some(format!(
                "Cadence skipped notification dispatch fan-out for operator action `{}` because no enabled routes are configured for project `{}`.",
                enqueue.action_id, enqueue.project_id
            )),
        },
        Ok(dispatches) => NotificationDispatchEnqueueOutcomeRecord {
            status: NotificationDispatchEnqueueStatus::Enqueued,
            dispatch_count: dispatches.len() as u32,
            code: Some("notification_dispatch_enqueued".into()),
            message: Some(format!(
                "Cadence enqueued {} notification dispatch route(s) for operator action `{}`.",
                dispatches.len(), enqueue.action_id
            )),
        },
        Err(error) => NotificationDispatchEnqueueOutcomeRecord {
            status: NotificationDispatchEnqueueStatus::Skipped,
            dispatch_count: 0,
            code: Some(error.code),
            message: Some(error.message),
        },
    }
}

fn format_notification_dispatch_enqueue_outcome(
    outcome: &NotificationDispatchEnqueueOutcomeRecord,
) -> String {
    let code = outcome
        .code
        .as_deref()
        .unwrap_or("notification_dispatch_enqueue_skipped");
    let message = outcome
        .message
        .as_deref()
        .unwrap_or("Cadence skipped notification dispatch fan-out.");

    match outcome.status {
        NotificationDispatchEnqueueStatus::Enqueued => format!(
            "{code}: {message} (dispatch_count={}).",
            outcome.dispatch_count
        ),
        NotificationDispatchEnqueueStatus::Skipped => format!("{code}: {message}"),
    }
}

pub fn load_notification_dispatches(
    repo_root: &Path,
    project_id: &str,
    action_id: Option<&str>,
) -> Result<Vec<NotificationDispatchRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "project_id",
        "notification_dispatch_request_invalid",
    )?;
    if let Some(action_id) = action_id {
        validate_non_empty_text(
            action_id,
            "action_id",
            "notification_dispatch_request_invalid",
        )?;
    }

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    read_notification_dispatches(&connection, &database_path, project_id, action_id)
}

pub fn load_pending_notification_dispatches(
    repo_root: &Path,
    project_id: &str,
    limit: Option<u32>,
) -> Result<Vec<NotificationDispatchRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "project_id",
        "notification_dispatch_request_invalid",
    )?;

    let limit = limit
        .map(i64::from)
        .unwrap_or(MAX_NOTIFICATION_PENDING_DISPATCH_BATCH_ROWS)
        .clamp(1, MAX_NOTIFICATION_PENDING_DISPATCH_BATCH_ROWS);

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    read_pending_notification_dispatches(&connection, &database_path, project_id, limit)
}

pub fn record_notification_dispatch_outcome(
    repo_root: &Path,
    outcome: &NotificationDispatchOutcomeUpdateRecord,
) -> Result<NotificationDispatchRecord, CommandError> {
    validate_notification_dispatch_outcome_update_payload(outcome)?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &outcome.project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "notification_dispatch_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the notification-dispatch outcome transaction.",
        )
    })?;

    let existing = read_notification_dispatch_by_route(
        &transaction,
        &database_path,
        &outcome.project_id,
        &outcome.action_id,
        &outcome.route_id,
    )?
    .ok_or_else(|| {
        CommandError::user_fixable(
            "notification_dispatch_not_found",
            format!(
                "Cadence could not record dispatch outcome because `{}`/`{}`/`{}` was not found.",
                outcome.project_id, outcome.action_id, outcome.route_id
            ),
        )
    })?;

    if existing.status == NotificationDispatchStatus::Claimed {
        return Err(CommandError::user_fixable(
            "notification_dispatch_already_claimed",
            format!(
                "Cadence refused to overwrite dispatch outcome for route `{}` because action `{}` has already been claimed for reply correlation.",
                existing.route_id, existing.action_id
            ),
        ));
    }

    let attempt_count = existing.attempt_count.saturating_add(1);
    let (last_error_code, last_error_message, delivered_at) = match outcome.status {
        NotificationDispatchStatus::Sent => (None, None, Some(outcome.attempted_at.as_str())),
        NotificationDispatchStatus::Failed => (
            outcome.error_code.as_deref(),
            outcome.error_message.as_deref(),
            existing.delivered_at.as_deref(),
        ),
        NotificationDispatchStatus::Pending | NotificationDispatchStatus::Claimed => {
            return Err(CommandError::user_fixable(
                "notification_dispatch_outcome_invalid",
                "Dispatch outcomes must use `sent` or `failed` status updates.",
            ))
        }
    };

    transaction
        .execute(
            r#"
            UPDATE notification_dispatches
            SET status = ?2,
                attempt_count = ?3,
                last_attempt_at = ?4,
                delivered_at = ?5,
                last_error_code = ?6,
                last_error_message = ?7,
                updated_at = ?4
            WHERE id = ?1
            "#,
            params![
                existing.id,
                notification_dispatch_status_sql_value(&outcome.status),
                i64::from(attempt_count),
                outcome.attempted_at.as_str(),
                delivered_at,
                last_error_code,
                last_error_message,
            ],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "notification_dispatch_update_failed",
                &database_path,
                error,
                "Cadence could not persist notification dispatch outcome metadata.",
            )
        })?;

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "notification_dispatch_commit_failed",
            &database_path,
            error,
            "Cadence could not commit notification dispatch outcome metadata.",
        )
    })?;

    read_notification_dispatch_by_id(&connection, &database_path, existing.id)?.ok_or_else(|| {
        CommandError::system_fault(
            "notification_dispatch_missing_after_persist",
            format!(
                "Cadence persisted notification dispatch outcome in {} but could not read row {} back.",
                database_path.display(),
                existing.id
            ),
        )
    })
}

pub fn claim_notification_reply(
    repo_root: &Path,
    request: &NotificationReplyClaimRequestRecord,
) -> Result<NotificationReplyClaimResultRecord, CommandError> {
    validate_notification_reply_claim_request_payload(request)?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &request.project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "notification_reply_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the notification-reply claim transaction.",
        )
    })?;

    let approval = read_operator_approval_by_action_id(
        &transaction,
        &database_path,
        &request.project_id,
        &request.action_id,
    )?;

    if approval.is_none() {
        let message = format!(
            "Cadence rejected the notification reply because action `{}` is not pending for project `{}`.",
            request.action_id, request.project_id
        );
        persist_notification_reply_rejection(
            &transaction,
            &database_path,
            request,
            "notification_reply_correlation_invalid",
            &message,
        )?;
        transaction.commit().map_err(|error| {
            map_operator_loop_commit_error(
                "notification_reply_commit_failed",
                &database_path,
                error,
                "Cadence could not commit the rejected notification reply claim.",
            )
        })?;

        return Err(CommandError::user_fixable(
            "notification_reply_correlation_invalid",
            message,
        ));
    }

    let approval = approval.expect("checked above");
    if approval.status != OperatorApprovalStatus::Pending {
        let message = format!(
            "Cadence rejected the notification reply because action `{}` is already {}.",
            request.action_id,
            operator_approval_status_label(&approval.status)
        );
        persist_notification_reply_rejection(
            &transaction,
            &database_path,
            request,
            "notification_reply_already_claimed",
            &message,
        )?;
        transaction.commit().map_err(|error| {
            map_operator_loop_commit_error(
                "notification_reply_commit_failed",
                &database_path,
                error,
                "Cadence could not commit the rejected notification reply claim.",
            )
        })?;

        return Err(CommandError::user_fixable(
            "notification_reply_already_claimed",
            message,
        ));
    }

    let Some(dispatch) = read_notification_dispatch_by_route(
        &transaction,
        &database_path,
        &request.project_id,
        &request.action_id,
        &request.route_id,
    )?
    else {
        let message = format!(
            "Cadence rejected the notification reply because route `{}` is not linked to action `{}` for project `{}`.",
            request.route_id, request.action_id, request.project_id
        );
        persist_notification_reply_rejection(
            &transaction,
            &database_path,
            request,
            "notification_reply_correlation_invalid",
            &message,
        )?;
        transaction.commit().map_err(|error| {
            map_operator_loop_commit_error(
                "notification_reply_commit_failed",
                &database_path,
                error,
                "Cadence could not commit the rejected notification reply claim.",
            )
        })?;

        return Err(CommandError::user_fixable(
            "notification_reply_correlation_invalid",
            message,
        ));
    };

    if dispatch.correlation_key != request.correlation_key {
        let message = format!(
            "Cadence rejected the notification reply because correlation key `{}` does not match route `{}` for action `{}`.",
            request.correlation_key, request.route_id, request.action_id
        );
        persist_notification_reply_rejection(
            &transaction,
            &database_path,
            request,
            "notification_reply_correlation_invalid",
            &message,
        )?;
        transaction.commit().map_err(|error| {
            map_operator_loop_commit_error(
                "notification_reply_commit_failed",
                &database_path,
                error,
                "Cadence could not commit the rejected notification reply claim.",
            )
        })?;

        return Err(CommandError::user_fixable(
            "notification_reply_correlation_invalid",
            message,
        ));
    }

    if let Some(existing_winner) = read_notification_winning_reply_claim(
        &transaction,
        &database_path,
        &request.project_id,
        &request.action_id,
    )? {
        let message = format!(
            "Cadence rejected the notification reply because action `{}` was already claimed by route `{}` at {}.",
            request.action_id, existing_winner.route_id, existing_winner.created_at
        );
        persist_notification_reply_rejection(
            &transaction,
            &database_path,
            request,
            "notification_reply_already_claimed",
            &message,
        )?;
        transaction.commit().map_err(|error| {
            map_operator_loop_commit_error(
                "notification_reply_commit_failed",
                &database_path,
                error,
                "Cadence could not commit the rejected notification reply claim.",
            )
        })?;

        return Err(CommandError::user_fixable(
            "notification_reply_already_claimed",
            message,
        ));
    }

    let accepted_insert = transaction.execute(
        r#"
        INSERT INTO notification_reply_claims (
            project_id,
            action_id,
            route_id,
            correlation_key,
            responder_id,
            reply_text,
            status,
            rejection_code,
            rejection_message,
            created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'accepted', NULL, NULL, ?7)
        "#,
        params![
            request.project_id.as_str(),
            request.action_id.as_str(),
            request.route_id.as_str(),
            request.correlation_key.as_str(),
            request.responder_id.as_deref(),
            request.reply_text.as_str(),
            request.received_at.as_str(),
        ],
    );

    if let Err(error) = accepted_insert {
        if is_unique_constraint_violation(&error) {
            let message = format!(
                "Cadence rejected the notification reply because action `{}` was already claimed by another route.",
                request.action_id
            );
            persist_notification_reply_rejection(
                &transaction,
                &database_path,
                request,
                "notification_reply_already_claimed",
                &message,
            )?;
            transaction.commit().map_err(|commit_error| {
                map_operator_loop_commit_error(
                    "notification_reply_commit_failed",
                    &database_path,
                    commit_error,
                    "Cadence could not commit the rejected notification reply claim.",
                )
            })?;

            return Err(CommandError::user_fixable(
                "notification_reply_already_claimed",
                message,
            ));
        }

        return Err(map_operator_loop_write_error(
            "notification_reply_claim_persist_failed",
            &database_path,
            error,
            "Cadence could not persist the accepted notification reply claim.",
        ));
    }

    let accepted_claim_id = transaction.last_insert_rowid();

    transaction
        .execute(
            r#"
            UPDATE notification_dispatches
            SET status = 'claimed',
                claimed_at = ?2,
                updated_at = ?2
            WHERE id = ?1
            "#,
            params![dispatch.id, request.received_at.as_str()],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "notification_reply_dispatch_update_failed",
                &database_path,
                error,
                "Cadence could not persist notification-dispatch claim metadata.",
            )
        })?;

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "notification_reply_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the notification reply claim transaction.",
        )
    })?;

    let claim = read_notification_reply_claim_by_id(&connection, &database_path, accepted_claim_id)?
        .ok_or_else(|| {
            CommandError::system_fault(
                "notification_reply_missing_after_persist",
                format!(
                    "Cadence persisted accepted notification reply claim `{accepted_claim_id}` in {} but could not read it back.",
                    database_path.display()
                ),
            )
        })?;
    let dispatch =
        read_notification_dispatch_by_id(&connection, &database_path, dispatch.id)?.ok_or_else(
            || {
                CommandError::system_fault(
                    "notification_dispatch_missing_after_persist",
                    format!(
                        "Cadence persisted notification dispatch claim metadata in {} but could not read row {} back.",
                        database_path.display(),
                        dispatch.id
                    ),
                )
            },
        )?;

    Ok(NotificationReplyClaimResultRecord { claim, dispatch })
}

pub fn load_notification_reply_claims(
    repo_root: &Path,
    project_id: &str,
    action_id: Option<&str>,
) -> Result<Vec<NotificationReplyClaimRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "project_id",
        "notification_reply_request_invalid",
    )?;
    if let Some(action_id) = action_id {
        validate_non_empty_text(action_id, "action_id", "notification_reply_request_invalid")?;
    }

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    read_notification_reply_claims(&connection, &database_path, project_id, action_id)
}

pub fn resolve_operator_action(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
    decision: OperatorApprovalDecision,
    decision_note: Option<&str>,
) -> Result<ResolveOperatorActionRecord, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "operator_action_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the operator-action transaction.",
        )
    })?;

    let existing =
        read_operator_approval_by_action_id(&transaction, &database_path, project_id, action_id)?
            .ok_or_else(|| {
            CommandError::user_fixable(
                "operator_action_not_found",
                format!(
                "Cadence could not find operator request `{action_id}` for the selected project."
            ),
            )
        })?;

    if existing.status != OperatorApprovalStatus::Pending {
        return Err(CommandError::user_fixable(
            "operator_action_already_resolved",
            format!(
                "Cadence cannot change operator request `{action_id}` because it is already {}.",
                operator_approval_status_label(&existing.status)
            ),
        ));
    }

    let decision_note = decision_note.map(str::trim).filter(|note| !note.is_empty());

    if let Some(secret_hint) = decision_note.and_then(find_prohibited_transition_diagnostic_content)
    {
        return Err(CommandError::user_fixable(
            "operator_action_decision_payload_invalid",
            format!(
                "Operator decision payload for `{action_id}` must not include {secret_hint}. Remove secret-bearing transcript/tool/auth material before retrying."
            ),
        ));
    }

    let answer_requirement = if matches!(decision, OperatorApprovalDecision::Approved) {
        classify_operator_answer_requirement(&existing)?
    } else {
        None
    };

    if answer_requirement.is_some() && decision_note.is_none() {
        return Err(CommandError::user_fixable(
            "operator_action_answer_required",
            format!(
                "Cadence requires a non-empty user answer before approving required-input operator request `{action_id}`."
            ),
        ));
    }

    let resolved_at = crate::auth::now_timestamp();
    let (approval_status, verification_status, verification_summary) = match decision {
        OperatorApprovalDecision::Approved => (
            "approved",
            "passed",
            format!("Approved operator action: {}.", existing.title),
        ),
        OperatorApprovalDecision::Rejected => (
            "rejected",
            "failed",
            format!("Rejected operator action: {}.", existing.title),
        ),
    };

    transaction
        .execute(
            r#"
            UPDATE operator_approvals
            SET status = ?3,
                decision_note = ?4,
                user_answer = ?5,
                updated_at = ?6,
                resolved_at = ?6
            WHERE project_id = ?1
              AND action_id = ?2
              AND status = 'pending'
            "#,
            params![
                project_id,
                action_id,
                approval_status,
                decision_note,
                decision_note,
                resolved_at,
            ],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "operator_action_resolve_failed",
                &database_path,
                error,
                "Cadence could not persist the operator decision.",
            )
        })?;

    let verification_id = transaction
        .execute(
            r#"
            INSERT INTO operator_verification_records (
                project_id,
                source_action_id,
                status,
                summary,
                detail,
                recorded_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                project_id,
                action_id,
                verification_status,
                verification_summary,
                decision_note,
                resolved_at,
            ],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "operator_verification_persist_failed",
                &database_path,
                error,
                "Cadence could not persist the operator verification record.",
            )
        })?;

    debug_assert_eq!(verification_id, 1);
    let verification_row_id = transaction.last_insert_rowid();

    transaction.commit().map_err(|error| {
        map_operator_loop_commit_error(
            "operator_action_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the operator decision.",
        )
    })?;

    let approval_request =
        read_operator_approval_by_action_id(&connection, &database_path, project_id, action_id)?
            .ok_or_else(|| {
                CommandError::system_fault(
                    "operator_action_missing_after_persist",
                    format!(
                "Cadence resolved operator request `{action_id}` in {} but could not read it back.",
                database_path.display()
            ),
                )
            })?;
    let verification_record = read_verification_record_by_id(
        &connection,
        &database_path,
        project_id,
        verification_row_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "operator_verification_missing_after_persist",
            format!(
                "Cadence persisted the operator verification record for `{action_id}` in {} but could not read it back.",
                database_path.display()
            ),
        )
    })?;

    Ok(ResolveOperatorActionRecord {
        approval_request,
        verification_record,
    })
}

fn classify_operator_answer_requirement(
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

pub fn record_runtime_operator_resume_outcome(
    repo_root: &Path,
    resume: &PreparedRuntimeOperatorResume,
    status: ResumeHistoryStatus,
    summary: &str,
) -> Result<ResumeOperatorRunRecord, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &resume.project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_operator_loop_transaction_error(
            "operator_resume_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the operator-resume transaction.",
        )
    })?;

    let created_at = crate::auth::now_timestamp();
    let fallback_summary = match status {
        ResumeHistoryStatus::Started => format!(
            "Operator resumed the selected project's runtime session after approving {}.",
            resume.approval_request.title
        ),
        ResumeHistoryStatus::Failed => format!(
            "Cadence could not resume the selected project's runtime session after approving {}.",
            resume.approval_request.title
        ),
    };
    let summary = normalize_runtime_resume_history_summary(summary, &fallback_summary);

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
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                resume.project_id.as_str(),
                resume.approval_request.action_id.as_str(),
                resume.session_id.as_str(),
                resume_history_status_sql_value(&status),
                summary.as_str(),
                created_at.as_str(),
            ],
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

    let approval_request = read_operator_approval_by_action_id(
        &connection,
        &database_path,
        &resume.project_id,
        &resume.approval_request.action_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "operator_action_missing_after_resume",
            format!(
                "Cadence recorded a resume event for `{}` in {} but could not reload the approval row.",
                resume.approval_request.action_id,
                database_path.display()
            ),
        )
    })?;
    let resume_entry = read_resume_history_entry_by_id(
        &connection,
        &database_path,
        &resume.project_id,
        resume_row_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "operator_resume_missing_after_persist",
            format!(
                "Cadence persisted the operator resume entry for `{}` in {} but could not read it back.",
                resume.approval_request.action_id,
                database_path.display()
            ),
        )
    })?;

    Ok(ResumeOperatorRunRecord {
        approval_request,
        resume_entry,
        automatic_dispatch: None,
    })
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

pub fn load_workflow_graph(
    repo_root: &Path,
    expected_project_id: &str,
) -> Result<WorkflowGraphRecord, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, expected_project_id)?;

    let nodes = read_workflow_graph_nodes(&connection, &database_path, expected_project_id)?;
    let edges = read_workflow_graph_edges(&connection, &database_path, expected_project_id)?;
    let gates = read_workflow_gate_metadata(&connection, &database_path, expected_project_id)?;

    Ok(WorkflowGraphRecord {
        nodes,
        edges,
        gates,
    })
}

pub fn upsert_workflow_graph(
    repo_root: &Path,
    expected_project_id: &str,
    graph: &WorkflowGraphUpsertRecord,
) -> Result<WorkflowGraphRecord, CommandError> {
    validate_workflow_graph_upsert_payload(graph)?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, expected_project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_workflow_graph_transaction_error(
            "workflow_graph_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the workflow-graph upsert transaction.",
        )
    })?;

    transaction
        .execute(
            "DELETE FROM workflow_graph_edges WHERE project_id = ?1",
            params![expected_project_id],
        )
        .map_err(|error| {
            map_workflow_graph_write_error(
                "workflow_graph_clear_failed",
                &database_path,
                error,
                "Cadence could not clear previous workflow edges.",
            )
        })?;

    transaction
        .execute(
            "DELETE FROM workflow_gate_metadata WHERE project_id = ?1",
            params![expected_project_id],
        )
        .map_err(|error| {
            map_workflow_graph_write_error(
                "workflow_graph_clear_failed",
                &database_path,
                error,
                "Cadence could not clear previous workflow gate metadata.",
            )
        })?;

    transaction
        .execute(
            "DELETE FROM workflow_graph_nodes WHERE project_id = ?1",
            params![expected_project_id],
        )
        .map_err(|error| {
            map_workflow_graph_write_error(
                "workflow_graph_clear_failed",
                &database_path,
                error,
                "Cadence could not clear previous workflow graph nodes.",
            )
        })?;

    for node in &graph.nodes {
        transaction
            .execute(
                r#"
                INSERT INTO workflow_graph_nodes (
                    project_id,
                    node_id,
                    phase_id,
                    sort_order,
                    name,
                    description,
                    status,
                    current_step,
                    task_count,
                    completed_tasks,
                    summary,
                    updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                "#,
                params![
                    expected_project_id,
                    node.node_id.as_str(),
                    i64::from(node.phase_id),
                    i64::from(node.sort_order),
                    node.name.as_str(),
                    node.description.as_str(),
                    phase_status_sql_value(&node.status),
                    node.current_step.as_ref().map(phase_step_sql_value),
                    i64::from(node.task_count),
                    i64::from(node.completed_tasks),
                    node.summary.as_deref(),
                ],
            )
            .map_err(|error| {
                map_workflow_graph_write_error(
                    "workflow_graph_node_upsert_failed",
                    &database_path,
                    error,
                    "Cadence could not persist a workflow graph node.",
                )
            })?;
    }

    for edge in &graph.edges {
        transaction
            .execute(
                r#"
                INSERT INTO workflow_graph_edges (
                    project_id,
                    from_node_id,
                    to_node_id,
                    transition_kind,
                    gate_requirement,
                    updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                "#,
                params![
                    expected_project_id,
                    edge.from_node_id.as_str(),
                    edge.to_node_id.as_str(),
                    edge.transition_kind.as_str(),
                    edge.gate_requirement.as_deref(),
                ],
            )
            .map_err(|error| {
                map_workflow_graph_write_error(
                    "workflow_graph_edge_upsert_failed",
                    &database_path,
                    error,
                    "Cadence could not persist a workflow graph edge.",
                )
            })?;
    }

    for gate in &graph.gates {
        transaction
            .execute(
                r#"
                INSERT INTO workflow_gate_metadata (
                    project_id,
                    node_id,
                    gate_key,
                    gate_state,
                    action_type,
                    title,
                    detail,
                    decision_context,
                    updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                "#,
                params![
                    expected_project_id,
                    gate.node_id.as_str(),
                    gate.gate_key.as_str(),
                    workflow_gate_state_sql_value(&gate.gate_state),
                    gate.action_type.as_deref(),
                    gate.title.as_deref(),
                    gate.detail.as_deref(),
                    gate.decision_context.as_deref(),
                ],
            )
            .map_err(|error| {
                map_workflow_graph_write_error(
                    "workflow_gate_upsert_failed",
                    &database_path,
                    error,
                    "Cadence could not persist workflow gate metadata.",
                )
            })?;
    }

    transaction.commit().map_err(|error| {
        map_workflow_graph_commit_error(
            "workflow_graph_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the workflow-graph upsert transaction.",
        )
    })?;

    load_workflow_graph(repo_root, expected_project_id)
}

pub fn apply_workflow_transition(
    repo_root: &Path,
    expected_project_id: &str,
    transition: &ApplyWorkflowTransitionRecord,
) -> Result<ApplyWorkflowTransitionResult, CommandError> {
    validate_workflow_transition_payload(transition)?;

    let database_path = database_path_for_repo(repo_root);
    let mut connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, expected_project_id)?;

    let transition_event = if let Some(existing) = read_transition_event_by_transition_id(
        &connection,
        &database_path,
        expected_project_id,
        &transition.transition_id,
    )? {
        existing
    } else {
        let transaction = connection.unchecked_transaction().map_err(|error| {
            map_workflow_graph_transaction_error(
                "workflow_transition_transaction_failed",
                &database_path,
                error,
                "Cadence could not start the workflow-transition transaction.",
            )
        })?;

        let transition_mutation = build_transition_mutation_record(transition);
        let mutation_outcome = apply_workflow_transition_mutation(
            &transaction,
            &database_path,
            expected_project_id,
            &transition_mutation,
            &WORKFLOW_TRANSITION_COMMAND_MUTATION_ERROR_PROFILE,
            map_workflow_graph_write_error,
        )?;

        match mutation_outcome {
            WorkflowTransitionMutationApplyOutcome::Replayed(transition_event) => transition_event,
            WorkflowTransitionMutationApplyOutcome::Applied => {
                transaction.commit().map_err(|error| {
                    map_workflow_graph_commit_error(
                        "workflow_transition_commit_failed",
                        &database_path,
                        error,
                        "Cadence could not commit the workflow transition transaction.",
                    )
                })?;

                read_transition_event_by_transition_id(
                    &connection,
                    &database_path,
                    expected_project_id,
                    &transition.transition_id,
                )?
                .ok_or_else(|| {
                    CommandError::system_fault(
                        "workflow_transition_event_missing_after_persist",
                        format!(
                            "Cadence persisted transition `{}` in {} but could not read it back.",
                            transition.transition_id,
                            database_path.display()
                        ),
                    )
                })?
            }
        }
    };

    let automatic_dispatch = attempt_automatic_dispatch_after_transition(
        &mut connection,
        &database_path,
        expected_project_id,
        &transition_event,
    );

    let phases = read_phase_summaries(&connection, &database_path, expected_project_id)?;

    Ok(ApplyWorkflowTransitionResult {
        transition_event,
        automatic_dispatch,
        phases,
    })
}

pub fn load_workflow_transition_event(
    repo_root: &Path,
    expected_project_id: &str,
    transition_id: &str,
) -> Result<Option<WorkflowTransitionEventRecord>, CommandError> {
    validate_non_empty_text(
        transition_id,
        "transition_id",
        "workflow_transition_request_invalid",
    )?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, expected_project_id)?;

    read_transition_event_by_transition_id(
        &connection,
        &database_path,
        expected_project_id,
        transition_id,
    )
}

pub fn load_recent_workflow_transition_events(
    repo_root: &Path,
    expected_project_id: &str,
    limit: Option<u32>,
) -> Result<Vec<WorkflowTransitionEventRecord>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, expected_project_id)?;

    read_transition_events(
        &connection,
        &database_path,
        expected_project_id,
        limit.map(i64::from),
    )
}

pub fn assemble_workflow_handoff_package(
    repo_root: &Path,
    project_id: &str,
    handoff_transition_id: &str,
) -> Result<WorkflowHandoffPackageUpsertRecord, CommandError> {
    validate_non_empty_text(project_id, "project_id", "workflow_handoff_request_invalid")?;
    validate_non_empty_text(
        handoff_transition_id,
        "handoff_transition_id",
        "workflow_handoff_request_invalid",
    )?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let trigger_transition = read_transition_event_by_transition_id(
        &connection,
        &database_path,
        project_id,
        handoff_transition_id,
    )?
    .ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_handoff_build_transition_missing",
            format!(
                "Cadence could not assemble a workflow handoff package because transition `{handoff_transition_id}` is not present for project `{project_id}`."
            ),
        )
    })?;

    assemble_workflow_handoff_package_upsert_record(
        &connection,
        &database_path,
        project_id,
        &trigger_transition,
    )
}

pub fn assemble_and_persist_workflow_handoff_package(
    repo_root: &Path,
    project_id: &str,
    handoff_transition_id: &str,
) -> Result<WorkflowHandoffPackageRecord, CommandError> {
    let payload = assemble_workflow_handoff_package(repo_root, project_id, handoff_transition_id)?;
    upsert_workflow_handoff_package(repo_root, &payload)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkflowHandoffPackagePersistDisposition {
    Persisted,
    Replayed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkflowHandoffPackagePersistResult {
    package: WorkflowHandoffPackageRecord,
    disposition: WorkflowHandoffPackagePersistDisposition,
}

pub fn upsert_workflow_handoff_package(
    repo_root: &Path,
    payload: &WorkflowHandoffPackageUpsertRecord,
) -> Result<WorkflowHandoffPackageRecord, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &payload.project_id)?;

    let persisted =
        persist_workflow_handoff_package_with_connection(&connection, &database_path, payload)?;

    Ok(persisted.package)
}

fn persist_workflow_handoff_package_with_connection(
    connection: &Connection,
    database_path: &Path,
    payload: &WorkflowHandoffPackageUpsertRecord,
) -> Result<WorkflowHandoffPackagePersistResult, CommandError> {
    validate_workflow_handoff_package_payload(payload)?;

    let canonical_payload = canonicalize_workflow_handoff_package_payload(
        &payload.package_payload,
        Some(database_path),
        "workflow_handoff_request_invalid",
    )?;
    let package_hash = compute_workflow_handoff_package_hash(&canonical_payload);

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_workflow_handoff_transaction_error(
            "workflow_handoff_transaction_failed",
            database_path,
            error,
            "Cadence could not start the workflow handoff-package transaction.",
        )
    })?;

    let transition_event = read_transition_event_by_transition_id(
        &transaction,
        database_path,
        &payload.project_id,
        &payload.handoff_transition_id,
    )?
    .ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_handoff_transition_missing",
            format!(
                "Cadence cannot persist a workflow handoff package for transition `{}` because no matching workflow transition event exists.",
                payload.handoff_transition_id
            ),
        )
    })?;

    validate_workflow_handoff_transition_metadata(payload, &transition_event)?;

    let inserted_rows = transaction
        .execute(
            r#"
            INSERT INTO workflow_handoff_packages (
                project_id,
                handoff_transition_id,
                causal_transition_id,
                from_node_id,
                to_node_id,
                transition_kind,
                package_payload,
                package_hash,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(project_id, handoff_transition_id) DO NOTHING
            "#,
            params![
                payload.project_id.as_str(),
                payload.handoff_transition_id.as_str(),
                transition_event.causal_transition_id.as_deref(),
                payload.from_node_id.as_str(),
                payload.to_node_id.as_str(),
                payload.transition_kind.as_str(),
                canonical_payload.as_str(),
                package_hash.as_str(),
                payload.created_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_workflow_handoff_insert_error(
                database_path,
                error,
                &payload.project_id,
                &payload.handoff_transition_id,
            )
        })?;

    if inserted_rows == 0 {
        let existing = read_workflow_handoff_package_by_transition_id(
            &transaction,
            database_path,
            &payload.project_id,
            &payload.handoff_transition_id,
        )?
        .ok_or_else(|| {
            CommandError::system_fault(
                "workflow_handoff_missing_after_replay",
                format!(
                    "Cadence replayed workflow handoff package write for transition `{}` in {} but could not read the stored package row.",
                    payload.handoff_transition_id,
                    database_path.display()
                ),
            )
        })?;

        if existing.package_hash != package_hash {
            return Err(CommandError::system_fault(
                "workflow_handoff_hash_conflict",
                format!(
                    "Cadence refused to overwrite replayed workflow handoff package for transition `{}` because stored hash `{}` did not match derived hash `{}` in {}.",
                    payload.handoff_transition_id,
                    existing.package_hash,
                    package_hash,
                    database_path.display()
                ),
            ));
        }

        transaction.rollback().map_err(|error| {
            map_workflow_handoff_commit_error(
                "workflow_handoff_commit_failed",
                database_path,
                error,
                "Cadence could not close the workflow handoff-package replay transaction.",
            )
        })?;

        return Ok(WorkflowHandoffPackagePersistResult {
            package: existing,
            disposition: WorkflowHandoffPackagePersistDisposition::Replayed,
        });
    }

    transaction.commit().map_err(|error| {
        map_workflow_handoff_commit_error(
            "workflow_handoff_commit_failed",
            database_path,
            error,
            "Cadence could not commit the workflow handoff-package transaction.",
        )
    })?;

    let package = read_workflow_handoff_package_by_transition_id(
        connection,
        database_path,
        &payload.project_id,
        &payload.handoff_transition_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "workflow_handoff_missing_after_persist",
            format!(
                "Cadence persisted workflow handoff package transition `{}` in {} but could not read it back.",
                payload.handoff_transition_id,
                database_path.display()
            ),
        )
    })?;

    Ok(WorkflowHandoffPackagePersistResult {
        package,
        disposition: WorkflowHandoffPackagePersistDisposition::Persisted,
    })
}

pub fn load_workflow_handoff_package(
    repo_root: &Path,
    expected_project_id: &str,
    handoff_transition_id: &str,
) -> Result<Option<WorkflowHandoffPackageRecord>, CommandError> {
    validate_non_empty_text(
        handoff_transition_id,
        "handoff_transition_id",
        "workflow_handoff_request_invalid",
    )?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, expected_project_id)?;

    read_workflow_handoff_package_by_transition_id(
        &connection,
        &database_path,
        expected_project_id,
        handoff_transition_id,
    )
}

pub fn load_recent_workflow_handoff_packages(
    repo_root: &Path,
    expected_project_id: &str,
    limit: Option<u32>,
) -> Result<Vec<WorkflowHandoffPackageRecord>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, expected_project_id)?;

    read_workflow_handoff_packages(
        &connection,
        &database_path,
        expected_project_id,
        limit.map(i64::from),
    )
}

fn assemble_workflow_handoff_package_upsert_record(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    trigger_transition: &WorkflowTransitionEventRecord,
) -> Result<WorkflowHandoffPackageUpsertRecord, CommandError> {
    if trigger_transition.transition_id.starts_with("auto:")
        && trigger_transition.causal_transition_id.is_none()
    {
        return Err(CommandError::system_fault(
            "workflow_handoff_build_causal_missing",
            format!(
                "Cadence cannot assemble workflow handoff package `{}` because automatic transitions must retain causal transition linkage.",
                trigger_transition.transition_id
            ),
        ));
    }

    ensure_workflow_handoff_safe_text(
        &trigger_transition.transition_id,
        "triggerTransition.transitionId",
    )?;
    ensure_workflow_handoff_optional_text(
        trigger_transition.causal_transition_id.as_deref(),
        "triggerTransition.causalTransitionId",
    )?;
    ensure_workflow_handoff_safe_text(
        &trigger_transition.from_node_id,
        "triggerTransition.fromNodeId",
    )?;
    ensure_workflow_handoff_safe_text(
        &trigger_transition.to_node_id,
        "triggerTransition.toNodeId",
    )?;
    ensure_workflow_handoff_safe_text(
        &trigger_transition.transition_kind,
        "triggerTransition.transitionKind",
    )?;

    let nodes =
        read_workflow_graph_nodes(connection, database_path, project_id).map_err(|error| {
            map_workflow_handoff_build_dependency_error(
                "workflow_handoff_build_node_state_invalid",
                "workflow node state",
                error,
            )
        })?;

    let destination_node = nodes
        .into_iter()
        .find(|node| node.node_id == trigger_transition.to_node_id)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_handoff_build_target_missing",
                format!(
                    "Cadence cannot assemble workflow handoff package `{}` because destination node `{}` metadata is missing.",
                    trigger_transition.transition_id, trigger_transition.to_node_id
                ),
            )
        })?;

    ensure_workflow_handoff_safe_text(&destination_node.node_id, "destinationState.nodeId")?;
    ensure_workflow_handoff_safe_text(&destination_node.name, "destinationState.name")?;
    ensure_workflow_handoff_safe_text(
        &destination_node.description,
        "destinationState.description",
    )?;

    let mut destination_gates = read_workflow_gate_metadata(connection, database_path, project_id)
        .map_err(|error| {
            map_workflow_handoff_build_dependency_error(
                "workflow_handoff_build_gate_state_invalid",
                "destination gate state",
                error,
            )
        })?
        .into_iter()
        .filter(|gate| gate.node_id == destination_node.node_id)
        .map(|gate| {
            ensure_workflow_handoff_safe_text(&gate.gate_key, "destinationState.gates[].gateKey")?;
            ensure_workflow_handoff_optional_text(
                gate.action_type.as_deref(),
                "destinationState.gates[].actionType",
            )?;

            Ok(WorkflowHandoffDestinationGatePayload {
                gate_key: gate.gate_key,
                gate_state: workflow_gate_state_sql_value(&gate.gate_state).to_string(),
                action_type: gate.action_type,
                detail_present: gate.detail.is_some(),
                decision_context_present: gate.decision_context.is_some(),
            })
        })
        .collect::<Result<Vec<_>, CommandError>>()?;

    destination_gates.sort_by(|left, right| {
        left.gate_key
            .cmp(&right.gate_key)
            .then_with(|| left.gate_state.cmp(&right.gate_state))
            .then_with(|| left.action_type.cmp(&right.action_type))
    });

    let pending_gate_count = destination_gates
        .iter()
        .filter(|gate| matches!(gate.gate_state.as_str(), "pending" | "blocked"))
        .count() as u32;

    let lifecycle_projection =
        read_planning_lifecycle_projection(connection, database_path, project_id).map_err(
            |error| {
                map_workflow_handoff_build_dependency_error(
                    "workflow_handoff_build_lifecycle_invalid",
                    "lifecycle projection",
                    error,
                )
            },
        )?;

    validate_workflow_handoff_lifecycle_projection(
        &lifecycle_projection,
        &trigger_transition.transition_id,
    )?;

    let lifecycle_stages = lifecycle_projection
        .stages
        .into_iter()
        .map(|stage| {
            ensure_workflow_handoff_safe_text(
                &stage.node_id,
                "lifecycleProjection.stages[].nodeId",
            )?;
            Ok(stage)
        })
        .collect::<Result<Vec<_>, CommandError>>()?;

    let operator_approvals = read_operator_approvals(connection, database_path, project_id)
        .map_err(|error| {
            map_workflow_handoff_build_dependency_error(
                "workflow_handoff_build_operator_state_invalid",
                "operator approvals",
                error,
            )
        })?;

    let mut pending_gate_actions = operator_approvals
        .into_iter()
        .filter(|approval| approval.status == OperatorApprovalStatus::Pending)
        .filter_map(|approval| {
            let OperatorApprovalDto {
                action_id,
                action_type,
                gate_node_id,
                gate_key,
                transition_from_node_id,
                transition_to_node_id,
                transition_kind,
                created_at,
                updated_at,
                ..
            } = approval;

            let (
                Some(gate_node_id),
                Some(gate_key),
                Some(transition_from_node_id),
                Some(transition_to_node_id),
                Some(transition_kind),
            ) = (
                gate_node_id,
                gate_key,
                transition_from_node_id,
                transition_to_node_id,
                transition_kind,
            )
            else {
                return None;
            };

            if transition_to_node_id != trigger_transition.to_node_id {
                return None;
            }

            Some((
                action_id,
                action_type,
                gate_node_id,
                gate_key,
                transition_from_node_id,
                transition_to_node_id,
                transition_kind,
                created_at,
                updated_at,
            ))
        })
        .map(
            |(
                action_id,
                action_type,
                gate_node_id,
                gate_key,
                transition_from_node_id,
                transition_to_node_id,
                transition_kind,
                created_at,
                updated_at,
            )| {
                ensure_workflow_handoff_safe_text(
                    &action_id,
                    "operatorContinuity.pendingGateActions[].actionId",
                )?;
                ensure_workflow_handoff_safe_text(
                    &action_type,
                    "operatorContinuity.pendingGateActions[].actionType",
                )?;
                ensure_workflow_handoff_safe_text(
                    &gate_node_id,
                    "operatorContinuity.pendingGateActions[].gateNodeId",
                )?;
                ensure_workflow_handoff_safe_text(
                    &gate_key,
                    "operatorContinuity.pendingGateActions[].gateKey",
                )?;
                ensure_workflow_handoff_safe_text(
                    &transition_from_node_id,
                    "operatorContinuity.pendingGateActions[].transitionFromNodeId",
                )?;
                ensure_workflow_handoff_safe_text(
                    &transition_to_node_id,
                    "operatorContinuity.pendingGateActions[].transitionToNodeId",
                )?;
                ensure_workflow_handoff_safe_text(
                    &transition_kind,
                    "operatorContinuity.pendingGateActions[].transitionKind",
                )?;

                Ok(WorkflowHandoffPendingGateActionPayload {
                    action_id,
                    action_type,
                    gate_node_id,
                    gate_key,
                    transition_from_node_id,
                    transition_to_node_id,
                    transition_kind,
                    created_at,
                    updated_at,
                })
            },
        )
        .collect::<Result<Vec<_>, CommandError>>()?;

    pending_gate_actions.sort_by(|left, right| {
        left.action_id
            .cmp(&right.action_id)
            .then_with(|| {
                left.transition_from_node_id
                    .cmp(&right.transition_from_node_id)
            })
            .then_with(|| left.transition_to_node_id.cmp(&right.transition_to_node_id))
            .then_with(|| left.transition_kind.cmp(&right.transition_kind))
    });

    let pending_action_ids = pending_gate_actions
        .iter()
        .map(|action| action.action_id.as_str())
        .collect::<std::collections::HashSet<_>>();

    let resume_history =
        read_resume_history(connection, database_path, project_id).map_err(|error| {
            map_workflow_handoff_build_dependency_error(
                "workflow_handoff_build_operator_state_invalid",
                "operator resume history",
                error,
            )
        })?;

    let latest_resume_row = if pending_action_ids.is_empty() {
        resume_history.into_iter().next()
    } else {
        resume_history.into_iter().find(|entry| {
            entry
                .source_action_id
                .as_deref()
                .is_some_and(|source_action_id| pending_action_ids.contains(source_action_id))
        })
    };

    let latest_resume = latest_resume_row
        .map(|entry| {
            ensure_workflow_handoff_optional_text(
                entry.source_action_id.as_deref(),
                "operatorContinuity.latestResume.sourceActionId",
            )?;

            Ok(WorkflowHandoffLatestResumePayload {
                source_action_id: entry.source_action_id,
                status: entry.status,
                created_at: entry.created_at,
            })
        })
        .transpose()?;

    let payload = WorkflowHandoffPackagePayload {
        schema_version: WORKFLOW_HANDOFF_PACKAGE_SCHEMA_VERSION,
        trigger_transition: WorkflowHandoffTriggerTransitionPayload {
            transition_id: trigger_transition.transition_id.clone(),
            causal_transition_id: trigger_transition.causal_transition_id.clone(),
            from_node_id: trigger_transition.from_node_id.clone(),
            to_node_id: trigger_transition.to_node_id.clone(),
            transition_kind: trigger_transition.transition_kind.clone(),
            gate_decision: workflow_transition_gate_decision_sql_value(
                &trigger_transition.gate_decision,
            )
            .to_string(),
            gate_decision_context_present: trigger_transition.gate_decision_context.is_some(),
            occurred_at: trigger_transition.created_at.clone(),
        },
        destination_state: WorkflowHandoffDestinationStatePayload {
            node_id: destination_node.node_id,
            phase_id: destination_node.phase_id,
            sort_order: destination_node.sort_order,
            name: destination_node.name,
            description: destination_node.description,
            status: destination_node.status,
            current_step: destination_node.current_step,
            task_count: destination_node.task_count,
            completed_tasks: destination_node.completed_tasks,
            pending_gate_count,
            gates: destination_gates,
        },
        lifecycle_projection: WorkflowHandoffLifecycleProjectionPayload {
            stages: lifecycle_stages,
        },
        operator_continuity: WorkflowHandoffOperatorContinuityPayload {
            pending_gate_action_count: pending_gate_actions.len() as u32,
            pending_gate_actions,
            latest_resume,
        },
    };

    let package_payload = serialize_workflow_handoff_package_payload(&payload, database_path)?;

    Ok(WorkflowHandoffPackageUpsertRecord {
        project_id: project_id.to_string(),
        handoff_transition_id: trigger_transition.transition_id.clone(),
        causal_transition_id: trigger_transition.causal_transition_id.clone(),
        from_node_id: trigger_transition.from_node_id.clone(),
        to_node_id: trigger_transition.to_node_id.clone(),
        transition_kind: trigger_transition.transition_kind.clone(),
        package_payload,
        created_at: trigger_transition.created_at.clone(),
    })
}

fn validate_workflow_handoff_lifecycle_projection(
    lifecycle_projection: &PlanningLifecycleProjectionDto,
    transition_id: &str,
) -> Result<(), CommandError> {
    let mut previous_index: Option<usize> = None;
    let mut seen_stage_indexes = [false; 4];

    for stage in &lifecycle_projection.stages {
        let stage_index = workflow_handoff_lifecycle_stage_index(stage.stage);

        if seen_stage_indexes[stage_index] {
            return Err(CommandError::user_fixable(
                "workflow_handoff_build_lifecycle_invalid",
                format!(
                    "Cadence cannot assemble workflow handoff package `{transition_id}` because lifecycle stage `{}` appears more than once.",
                    planning_lifecycle_stage_label(&stage.stage)
                ),
            ));
        }

        if let Some(previous_index) = previous_index {
            if stage_index < previous_index {
                return Err(CommandError::user_fixable(
                    "workflow_handoff_build_lifecycle_invalid",
                    format!(
                        "Cadence cannot assemble workflow handoff package `{transition_id}` because lifecycle stages are not in canonical order."
                    ),
                ));
            }
        }

        seen_stage_indexes[stage_index] = true;
        previous_index = Some(stage_index);
    }

    Ok(())
}

fn workflow_handoff_lifecycle_stage_index(stage: PlanningLifecycleStageKindDto) -> usize {
    match stage {
        PlanningLifecycleStageKindDto::Discussion => 0,
        PlanningLifecycleStageKindDto::Research => 1,
        PlanningLifecycleStageKindDto::Requirements => 2,
        PlanningLifecycleStageKindDto::Roadmap => 3,
    }
}

fn ensure_workflow_handoff_optional_text(
    value: Option<&str>,
    field: &'static str,
) -> Result<(), CommandError> {
    if let Some(value) = value {
        ensure_workflow_handoff_safe_text(value, field)?;
    }

    Ok(())
}

fn ensure_workflow_handoff_safe_text(value: &str, field: &'static str) -> Result<(), CommandError> {
    if let Some(secret_hint) = find_prohibited_workflow_handoff_content(value) {
        return Err(CommandError::user_fixable(
            "workflow_handoff_redaction_failed",
            format!(
                "Cadence refused to assemble workflow handoff package because `{field}` contained {secret_hint}. Remove secret-bearing transcript/tool/auth content before retrying."
            ),
        ));
    }

    Ok(())
}

fn find_prohibited_workflow_handoff_content(value: &str) -> Option<&'static str> {
    find_prohibited_runtime_persistence_content(value)
}

fn serialize_workflow_handoff_package_payload(
    payload: &WorkflowHandoffPackagePayload,
    database_path: &Path,
) -> Result<String, CommandError> {
    let raw_payload = serde_json::to_value(payload).map_err(|error| {
        CommandError::system_fault(
            "workflow_handoff_serialize_failed",
            format!(
                "Cadence could not serialize workflow handoff package payload in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    let canonical_payload = canonicalize_workflow_handoff_json_value(raw_payload);
    let serialized_payload = serde_json::to_string(&canonical_payload).map_err(|error| {
        CommandError::system_fault(
            "workflow_handoff_serialize_failed",
            format!(
                "Cadence could not canonicalize workflow handoff package payload in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    if let Some(secret_hint) = find_prohibited_workflow_handoff_content(&serialized_payload) {
        return Err(CommandError::user_fixable(
            "workflow_handoff_redaction_failed",
            format!(
                "Cadence refused to assemble workflow handoff package because serialized payload contained {secret_hint}. Remove secret-bearing transcript/tool/auth content before retrying."
            ),
        ));
    }

    Ok(serialized_payload)
}

fn map_workflow_handoff_build_dependency_error(
    code: &str,
    dependency: &str,
    error: CommandError,
) -> CommandError {
    let message = format!(
        "Cadence could not assemble workflow handoff package because {dependency} could not be loaded: {}",
        error.message
    );

    match error.class {
        CommandErrorClass::UserFixable | CommandErrorClass::PolicyDenied => {
            CommandError::user_fixable(code, message)
        }
        CommandErrorClass::Retryable => CommandError::retryable(code, message),
        CommandErrorClass::SystemFault => CommandError::system_fault(code, message),
    }
}

fn read_project_row(
    connection: &Connection,
    database_path: &Path,
    repo_root: &Path,
    expected_project_id: &str,
) -> Result<ProjectSummaryRow, CommandError> {
    connection
        .query_row(
            r#"
            SELECT
                id,
                name,
                description,
                milestone,
                branch,
                runtime
            FROM projects
            WHERE id = ?1
            "#,
            [expected_project_id],
            |row| {
                Ok(ProjectSummaryRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    milestone: row.get(3)?,
                    branch: row.get(4)?,
                    runtime: row.get(5)?,
                })
            },
        )
        .map_err(|error| {
            map_project_query_error(error, database_path, repo_root, expected_project_id)
        })
}

fn read_workflow_graph_nodes(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Vec<WorkflowGraphNodeRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                node_id,
                phase_id,
                sort_order,
                name,
                description,
                status,
                current_step,
                task_count,
                completed_tasks,
                summary
            FROM workflow_graph_nodes
            WHERE project_id = ?1
            ORDER BY sort_order ASC, phase_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_graph_query_failed",
                format!(
                    "Cadence could not prepare workflow-graph node rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map([expected_project_id], |row| {
            Ok(RawGraphNodeRow {
                node_id: row.get(0)?,
                phase_id: row.get(1)?,
                sort_order: row.get(2)?,
                name: row.get(3)?,
                description: row.get(4)?,
                status: row.get(5)?,
                current_step: row.get(6)?,
                task_count: row.get(7)?,
                completed_tasks: row.get(8)?,
                summary: row.get(9)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_graph_query_failed",
                format!(
                    "Cadence could not query workflow-graph node rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "workflow_graph_decode_failed",
                        format!(
                            "Cadence could not decode workflow-graph node rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_workflow_graph_node_row(raw_row, database_path))
        })
        .collect()
}

fn read_workflow_graph_edges(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Vec<WorkflowGraphEdgeRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                from_node_id,
                to_node_id,
                transition_kind,
                gate_requirement
            FROM workflow_graph_edges
            WHERE project_id = ?1
            ORDER BY from_node_id ASC, to_node_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_graph_query_failed",
                format!(
                    "Cadence could not prepare workflow-graph edge rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map([expected_project_id], |row| {
            Ok(RawGraphEdgeRow {
                from_node_id: row.get(0)?,
                to_node_id: row.get(1)?,
                transition_kind: row.get(2)?,
                gate_requirement: row.get(3)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_graph_query_failed",
                format!(
                    "Cadence could not query workflow-graph edge rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "workflow_graph_decode_failed",
                        format!(
                            "Cadence could not decode workflow-graph edge rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_workflow_graph_edge_row(raw_row, database_path))
        })
        .collect()
}

fn read_workflow_gate_metadata(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Vec<WorkflowGateMetadataRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                node_id,
                gate_key,
                gate_state,
                action_type,
                title,
                detail,
                decision_context
            FROM workflow_gate_metadata
            WHERE project_id = ?1
            ORDER BY node_id ASC, gate_key ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_graph_query_failed",
                format!(
                    "Cadence could not prepare workflow gate rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map([expected_project_id], |row| {
            Ok(RawGateMetadataRow {
                node_id: row.get(0)?,
                gate_key: row.get(1)?,
                gate_state: row.get(2)?,
                action_type: row.get(3)?,
                title: row.get(4)?,
                detail: row.get(5)?,
                decision_context: row.get(6)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_graph_query_failed",
                format!(
                    "Cadence could not query workflow gate rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "workflow_graph_decode_failed",
                        format!(
                            "Cadence could not decode workflow gate rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_workflow_gate_metadata_row(raw_row, database_path))
        })
        .collect()
}

fn read_transition_events(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
    limit_override: Option<i64>,
) -> Result<Vec<WorkflowTransitionEventRecord>, CommandError> {
    let limit = limit_override
        .unwrap_or(MAX_WORKFLOW_TRANSITION_EVENT_ROWS)
        .max(1);

    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                transition_id,
                causal_transition_id,
                from_node_id,
                to_node_id,
                transition_kind,
                gate_decision,
                gate_decision_context,
                created_at
            FROM workflow_transition_events
            WHERE project_id = ?1
            ORDER BY created_at DESC, id DESC
            LIMIT ?2
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_transition_query_failed",
                format!(
                    "Cadence could not prepare workflow transition-event rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map(params![expected_project_id, limit], |row| {
            Ok(RawTransitionEventRow {
                id: row.get(0)?,
                transition_id: row.get(1)?,
                causal_transition_id: row.get(2)?,
                from_node_id: row.get(3)?,
                to_node_id: row.get(4)?,
                transition_kind: row.get(5)?,
                gate_decision: row.get(6)?,
                gate_decision_context: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_transition_query_failed",
                format!(
                    "Cadence could not query workflow transition-event rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "workflow_transition_decode_failed",
                        format!(
                            "Cadence could not decode workflow transition-event rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_workflow_transition_event_row(raw_row, database_path))
        })
        .collect()
}

fn read_transition_event_by_transition_id(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    transition_id: &str,
) -> Result<Option<WorkflowTransitionEventRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                transition_id,
                causal_transition_id,
                from_node_id,
                to_node_id,
                transition_kind,
                gate_decision,
                gate_decision_context,
                created_at
            FROM workflow_transition_events
            WHERE project_id = ?1
              AND transition_id = ?2
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_transition_query_failed",
                format!(
                    "Cadence could not prepare transition-event lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement
        .query(params![project_id, transition_id])
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_transition_query_failed",
                format!(
                    "Cadence could not query transition-event lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "workflow_transition_query_failed",
            format!(
                "Cadence could not read transition-event lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_workflow_transition_event_row(
        RawTransitionEventRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "workflow_transition_decode_failed",
                    format!(
                        "Cadence could not decode transition-event lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            transition_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "workflow_transition_decode_failed",
                    format!(
                        "Cadence could not decode transition-event lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            causal_transition_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "workflow_transition_decode_failed",
                    format!(
                        "Cadence could not decode transition-event lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            from_node_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "workflow_transition_decode_failed",
                    format!(
                        "Cadence could not decode transition-event lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            to_node_id: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "workflow_transition_decode_failed",
                    format!(
                        "Cadence could not decode transition-event lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            transition_kind: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "workflow_transition_decode_failed",
                    format!(
                        "Cadence could not decode transition-event lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            gate_decision: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "workflow_transition_decode_failed",
                    format!(
                        "Cadence could not decode transition-event lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            gate_decision_context: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "workflow_transition_decode_failed",
                    format!(
                        "Cadence could not decode transition-event lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "workflow_transition_decode_failed",
                    format!(
                        "Cadence could not decode transition-event lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_workflow_handoff_packages(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
    limit_override: Option<i64>,
) -> Result<Vec<WorkflowHandoffPackageRecord>, CommandError> {
    let limit = limit_override
        .unwrap_or(MAX_WORKFLOW_HANDOFF_PACKAGE_ROWS)
        .max(1);

    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                handoff_transition_id,
                causal_transition_id,
                from_node_id,
                to_node_id,
                transition_kind,
                package_payload,
                package_hash,
                created_at
            FROM workflow_handoff_packages
            WHERE project_id = ?1
            ORDER BY created_at DESC, id DESC
            LIMIT ?2
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_handoff_query_failed",
                format!(
                    "Cadence could not prepare workflow handoff-package rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map(params![expected_project_id, limit], |row| {
            Ok(RawWorkflowHandoffPackageRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                handoff_transition_id: row.get(2)?,
                causal_transition_id: row.get(3)?,
                from_node_id: row.get(4)?,
                to_node_id: row.get(5)?,
                transition_kind: row.get(6)?,
                package_payload: row.get(7)?,
                package_hash: row.get(8)?,
                created_at: row.get(9)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_handoff_query_failed",
                format!(
                    "Cadence could not query workflow handoff-package rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "workflow_handoff_decode_failed",
                        format!(
                            "Cadence could not decode workflow handoff-package rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_workflow_handoff_package_row(raw_row, database_path))
        })
        .collect()
}

fn read_workflow_handoff_package_by_transition_id(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    handoff_transition_id: &str,
) -> Result<Option<WorkflowHandoffPackageRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                handoff_transition_id,
                causal_transition_id,
                from_node_id,
                to_node_id,
                transition_kind,
                package_payload,
                package_hash,
                created_at
            FROM workflow_handoff_packages
            WHERE project_id = ?1
              AND handoff_transition_id = ?2
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_handoff_query_failed",
                format!(
                    "Cadence could not prepare workflow handoff-package lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement
        .query(params![project_id, handoff_transition_id])
        .map_err(|error| {
            CommandError::system_fault(
                "workflow_handoff_query_failed",
                format!(
                    "Cadence could not query workflow handoff-package lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "workflow_handoff_query_failed",
            format!(
                "Cadence could not read workflow handoff-package lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_workflow_handoff_package_row(
        RawWorkflowHandoffPackageRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "workflow_handoff_decode_failed",
                    format!(
                        "Cadence could not decode workflow handoff-package lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "workflow_handoff_decode_failed",
                    format!(
                        "Cadence could not decode workflow handoff-package lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            handoff_transition_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "workflow_handoff_decode_failed",
                    format!(
                        "Cadence could not decode workflow handoff-package lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            causal_transition_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "workflow_handoff_decode_failed",
                    format!(
                        "Cadence could not decode workflow handoff-package lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            from_node_id: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "workflow_handoff_decode_failed",
                    format!(
                        "Cadence could not decode workflow handoff-package lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            to_node_id: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "workflow_handoff_decode_failed",
                    format!(
                        "Cadence could not decode workflow handoff-package lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            transition_kind: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "workflow_handoff_decode_failed",
                    format!(
                        "Cadence could not decode workflow handoff-package lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            package_payload: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "workflow_handoff_decode_failed",
                    format!(
                        "Cadence could not decode workflow handoff-package lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            package_hash: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "workflow_handoff_decode_failed",
                    format!(
                        "Cadence could not decode workflow handoff-package lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "workflow_handoff_decode_failed",
                    format!(
                        "Cadence could not decode workflow handoff-package lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_notification_routes(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
) -> Result<Vec<NotificationRouteRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                route_id,
                route_kind,
                route_target,
                enabled,
                metadata_json,
                created_at,
                updated_at
            FROM notification_routes
            WHERE project_id = ?1
            ORDER BY enabled DESC, updated_at DESC, route_id ASC
            LIMIT ?2
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_route_query_failed",
                format!(
                    "Cadence could not prepare notification-route rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map(params![project_id, MAX_NOTIFICATION_ROUTE_ROWS], |row| {
            Ok(RawNotificationRouteRow {
                project_id: row.get(0)?,
                route_id: row.get(1)?,
                route_kind: row.get(2)?,
                route_target: row.get(3)?,
                enabled: row.get(4)?,
                metadata_json: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "notification_route_query_failed",
                format!(
                    "Cadence could not query notification-route rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "notification_route_decode_failed",
                        format!(
                            "Cadence could not decode notification-route rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_notification_route_row(raw_row, database_path))
        })
        .collect()
}

fn read_notification_route_by_id(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    route_id: &str,
) -> Result<Option<NotificationRouteRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                route_id,
                route_kind,
                route_target,
                enabled,
                metadata_json,
                created_at,
                updated_at
            FROM notification_routes
            WHERE project_id = ?1
              AND route_id = ?2
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_route_query_failed",
                format!(
                    "Cadence could not prepare notification-route lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement
        .query(params![project_id, route_id])
        .map_err(|error| {
            CommandError::system_fault(
                "notification_route_query_failed",
                format!(
                    "Cadence could not query notification-route lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_route_query_failed",
            format!(
                "Cadence could not read notification-route lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_notification_route_row(
        RawNotificationRouteRow {
            project_id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_kind: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_target: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            enabled: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            metadata_json: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            updated_at: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_route_decode_failed",
                    format!(
                        "Cadence could not decode notification-route lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_notification_dispatches(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    action_id: Option<&str>,
) -> Result<Vec<NotificationDispatchRecord>, CommandError> {
    let mut statement = if action_id.is_some() {
        connection
            .prepare(
                r#"
                SELECT
                    id,
                    project_id,
                    action_id,
                    route_id,
                    correlation_key,
                    status,
                    attempt_count,
                    last_attempt_at,
                    delivered_at,
                    claimed_at,
                    last_error_code,
                    last_error_message,
                    created_at,
                    updated_at
                FROM notification_dispatches
                WHERE project_id = ?1
                  AND action_id = ?2
                ORDER BY created_at ASC, id ASC
                LIMIT ?3
                "#,
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_query_failed",
                    format!(
                        "Cadence could not prepare notification-dispatch rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?
    } else {
        connection
            .prepare(
                r#"
                SELECT
                    id,
                    project_id,
                    action_id,
                    route_id,
                    correlation_key,
                    status,
                    attempt_count,
                    last_attempt_at,
                    delivered_at,
                    claimed_at,
                    last_error_code,
                    last_error_message,
                    created_at,
                    updated_at
                FROM notification_dispatches
                WHERE project_id = ?1
                ORDER BY updated_at DESC, id DESC
                LIMIT ?2
                "#,
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_query_failed",
                    format!(
                        "Cadence could not prepare notification-dispatch rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?
    };

    if let Some(action_id) = action_id {
        let raw_rows = statement
            .query_map(
                params![project_id, action_id, MAX_NOTIFICATION_DISPATCH_ROWS],
                |row| {
                    Ok(RawNotificationDispatchRow {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        action_id: row.get(2)?,
                        route_id: row.get(3)?,
                        correlation_key: row.get(4)?,
                        status: row.get(5)?,
                        attempt_count: row.get(6)?,
                        last_attempt_at: row.get(7)?,
                        delivered_at: row.get(8)?,
                        claimed_at: row.get(9)?,
                        last_error_code: row.get(10)?,
                        last_error_message: row.get(11)?,
                        created_at: row.get(12)?,
                        updated_at: row.get(13)?,
                    })
                },
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_query_failed",
                    format!(
                        "Cadence could not query notification-dispatch rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?;

        raw_rows
            .map(|raw_row| {
                raw_row
                    .map_err(|error| {
                        CommandError::system_fault(
                            "notification_dispatch_decode_failed",
                            format!(
                                "Cadence could not decode notification-dispatch rows from {}: {error}",
                                database_path.display()
                            ),
                        )
                    })
                    .and_then(|raw_row| decode_notification_dispatch_row(raw_row, database_path))
            })
            .collect()
    } else {
        let raw_rows = statement
            .query_map(params![project_id, MAX_NOTIFICATION_DISPATCH_ROWS], |row| {
                Ok(RawNotificationDispatchRow {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    action_id: row.get(2)?,
                    route_id: row.get(3)?,
                    correlation_key: row.get(4)?,
                    status: row.get(5)?,
                    attempt_count: row.get(6)?,
                    last_attempt_at: row.get(7)?,
                    delivered_at: row.get(8)?,
                    claimed_at: row.get(9)?,
                    last_error_code: row.get(10)?,
                    last_error_message: row.get(11)?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            })
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_query_failed",
                    format!(
                        "Cadence could not query notification-dispatch rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?;

        raw_rows
            .map(|raw_row| {
                raw_row
                    .map_err(|error| {
                        CommandError::system_fault(
                            "notification_dispatch_decode_failed",
                            format!(
                                "Cadence could not decode notification-dispatch rows from {}: {error}",
                                database_path.display()
                            ),
                        )
                    })
                    .and_then(|raw_row| decode_notification_dispatch_row(raw_row, database_path))
            })
            .collect()
    }
}

fn read_pending_notification_dispatches(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    limit: i64,
) -> Result<Vec<NotificationDispatchRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                action_id,
                route_id,
                correlation_key,
                status,
                attempt_count,
                last_attempt_at,
                delivered_at,
                claimed_at,
                last_error_code,
                last_error_message,
                created_at,
                updated_at
            FROM notification_dispatches
            WHERE project_id = ?1
              AND status = 'pending'
            ORDER BY
                COALESCE(last_attempt_at, created_at) ASC,
                created_at ASC,
                id ASC
            LIMIT ?2
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_dispatch_query_failed",
                format!(
                    "Cadence could not prepare pending notification-dispatch query from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map(params![project_id, limit], |row| {
            Ok(RawNotificationDispatchRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                action_id: row.get(2)?,
                route_id: row.get(3)?,
                correlation_key: row.get(4)?,
                status: row.get(5)?,
                attempt_count: row.get(6)?,
                last_attempt_at: row.get(7)?,
                delivered_at: row.get(8)?,
                claimed_at: row.get(9)?,
                last_error_code: row.get(10)?,
                last_error_message: row.get(11)?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "notification_dispatch_query_failed",
                format!(
                    "Cadence could not query pending notification-dispatch rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "notification_dispatch_decode_failed",
                        format!(
                            "Cadence could not decode pending notification-dispatch rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_notification_dispatch_row(raw_row, database_path))
        })
        .collect()
}

fn read_notification_dispatch_by_route(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    action_id: &str,
    route_id: &str,
) -> Result<Option<NotificationDispatchRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                action_id,
                route_id,
                correlation_key,
                status,
                attempt_count,
                last_attempt_at,
                delivered_at,
                claimed_at,
                last_error_code,
                last_error_message,
                created_at,
                updated_at
            FROM notification_dispatches
            WHERE project_id = ?1
              AND action_id = ?2
              AND route_id = ?3
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_dispatch_query_failed",
                format!(
                    "Cadence could not prepare notification-dispatch lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement
        .query(params![project_id, action_id, route_id])
        .map_err(|error| {
            CommandError::system_fault(
                "notification_dispatch_query_failed",
                format!(
                    "Cadence could not query notification-dispatch lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_dispatch_query_failed",
            format!(
                "Cadence could not read notification-dispatch lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_notification_dispatch_row(
        RawNotificationDispatchRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            correlation_key: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            attempt_count: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_attempt_at: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            delivered_at: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            claimed_at: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_error_code: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_error_message: row.get(11).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(12).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            updated_at: row.get(13).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_notification_dispatch_by_id(
    connection: &Connection,
    database_path: &Path,
    id: i64,
) -> Result<Option<NotificationDispatchRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                action_id,
                route_id,
                correlation_key,
                status,
                attempt_count,
                last_attempt_at,
                delivered_at,
                claimed_at,
                last_error_code,
                last_error_message,
                created_at,
                updated_at
            FROM notification_dispatches
            WHERE id = ?1
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_dispatch_query_failed",
                format!(
                    "Cadence could not prepare notification-dispatch id lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement.query(params![id]).map_err(|error| {
        CommandError::system_fault(
            "notification_dispatch_query_failed",
            format!(
                "Cadence could not query notification-dispatch id lookup from {}: {error}",
                database_path.display()
            ),
        )
    })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_dispatch_query_failed",
            format!(
                "Cadence could not read notification-dispatch id lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_notification_dispatch_row(
        RawNotificationDispatchRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            correlation_key: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            attempt_count: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_attempt_at: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            delivered_at: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            claimed_at: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_error_code: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            last_error_message: row.get(11).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(12).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            updated_at: row.get(13).map_err(|error| {
                CommandError::system_fault(
                    "notification_dispatch_decode_failed",
                    format!(
                        "Cadence could not decode notification-dispatch id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_notification_reply_claims(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    action_id: Option<&str>,
) -> Result<Vec<NotificationReplyClaimRecord>, CommandError> {
    let mut statement = if action_id.is_some() {
        connection
            .prepare(
                r#"
                SELECT
                    id,
                    project_id,
                    action_id,
                    route_id,
                    correlation_key,
                    responder_id,
                    reply_text,
                    status,
                    rejection_code,
                    rejection_message,
                    created_at
                FROM notification_reply_claims
                WHERE project_id = ?1
                  AND action_id = ?2
                ORDER BY created_at DESC, id DESC
                LIMIT ?3
                "#,
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_query_failed",
                    format!(
                        "Cadence could not prepare notification-reply claim rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?
    } else {
        connection
            .prepare(
                r#"
                SELECT
                    id,
                    project_id,
                    action_id,
                    route_id,
                    correlation_key,
                    responder_id,
                    reply_text,
                    status,
                    rejection_code,
                    rejection_message,
                    created_at
                FROM notification_reply_claims
                WHERE project_id = ?1
                ORDER BY created_at DESC, id DESC
                LIMIT ?2
                "#,
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_query_failed",
                    format!(
                        "Cadence could not prepare notification-reply claim rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?
    };

    if let Some(action_id) = action_id {
        let raw_rows = statement
            .query_map(
                params![project_id, action_id, MAX_NOTIFICATION_REPLY_CLAIM_ROWS],
                |row| {
                    Ok(RawNotificationReplyClaimRow {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        action_id: row.get(2)?,
                        route_id: row.get(3)?,
                        correlation_key: row.get(4)?,
                        responder_id: row.get(5)?,
                        reply_text: row.get(6)?,
                        status: row.get(7)?,
                        rejection_code: row.get(8)?,
                        rejection_message: row.get(9)?,
                        created_at: row.get(10)?,
                    })
                },
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_query_failed",
                    format!(
                        "Cadence could not query notification-reply claim rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?;

        raw_rows
            .map(|raw_row| {
                raw_row
                    .map_err(|error| {
                        CommandError::system_fault(
                            "notification_reply_decode_failed",
                            format!(
                                "Cadence could not decode notification-reply claim rows from {}: {error}",
                                database_path.display()
                            ),
                        )
                    })
                    .and_then(|raw_row| decode_notification_reply_claim_row(raw_row, database_path))
            })
            .collect()
    } else {
        let raw_rows = statement
            .query_map(
                params![project_id, MAX_NOTIFICATION_REPLY_CLAIM_ROWS],
                |row| {
                    Ok(RawNotificationReplyClaimRow {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        action_id: row.get(2)?,
                        route_id: row.get(3)?,
                        correlation_key: row.get(4)?,
                        responder_id: row.get(5)?,
                        reply_text: row.get(6)?,
                        status: row.get(7)?,
                        rejection_code: row.get(8)?,
                        rejection_message: row.get(9)?,
                        created_at: row.get(10)?,
                    })
                },
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_query_failed",
                    format!(
                        "Cadence could not query notification-reply claim rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?;

        raw_rows
            .map(|raw_row| {
                raw_row
                    .map_err(|error| {
                        CommandError::system_fault(
                            "notification_reply_decode_failed",
                            format!(
                                "Cadence could not decode notification-reply claim rows from {}: {error}",
                                database_path.display()
                            ),
                        )
                    })
                    .and_then(|raw_row| decode_notification_reply_claim_row(raw_row, database_path))
            })
            .collect()
    }
}

fn read_notification_reply_claim_by_id(
    connection: &Connection,
    database_path: &Path,
    id: i64,
) -> Result<Option<NotificationReplyClaimRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                action_id,
                route_id,
                correlation_key,
                responder_id,
                reply_text,
                status,
                rejection_code,
                rejection_message,
                created_at
            FROM notification_reply_claims
            WHERE id = ?1
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_reply_query_failed",
                format!(
                    "Cadence could not prepare notification-reply claim id lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement.query(params![id]).map_err(|error| {
        CommandError::system_fault(
            "notification_reply_query_failed",
            format!(
                "Cadence could not query notification-reply claim id lookup from {}: {error}",
                database_path.display()
            ),
        )
    })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_reply_query_failed",
            format!(
                "Cadence could not read notification-reply claim id lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_notification_reply_claim_row(
        RawNotificationReplyClaimRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            correlation_key: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            responder_id: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            reply_text: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            rejection_code: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            rejection_message: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode notification-reply claim id lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_notification_winning_reply_claim(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    action_id: &str,
) -> Result<Option<NotificationReplyClaimRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                action_id,
                route_id,
                correlation_key,
                responder_id,
                reply_text,
                status,
                rejection_code,
                rejection_message,
                created_at
            FROM notification_reply_claims
            WHERE project_id = ?1
              AND action_id = ?2
              AND status = 'accepted'
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "notification_reply_query_failed",
                format!(
                    "Cadence could not prepare winning notification-reply lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement
        .query(params![project_id, action_id])
        .map_err(|error| {
            CommandError::system_fault(
                "notification_reply_query_failed",
                format!(
                    "Cadence could not query winning notification-reply lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "notification_reply_query_failed",
            format!(
                "Cadence could not read winning notification-reply lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_notification_reply_claim_row(
        RawNotificationReplyClaimRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            project_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            route_id: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            correlation_key: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            responder_id: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            reply_text: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            rejection_code: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            rejection_message: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "notification_reply_decode_failed",
                    format!(
                        "Cadence could not decode winning notification-reply lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn persist_notification_reply_rejection(
    transaction: &Transaction<'_>,
    database_path: &Path,
    request: &NotificationReplyClaimRequestRecord,
    rejection_code: &str,
    rejection_message: &str,
) -> Result<i64, CommandError> {
    transaction
        .execute(
            r#"
            INSERT INTO notification_reply_claims (
                project_id,
                action_id,
                route_id,
                correlation_key,
                responder_id,
                reply_text,
                status,
                rejection_code,
                rejection_message,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'rejected', ?7, ?8, ?9)
            "#,
            params![
                request.project_id.as_str(),
                request.action_id.as_str(),
                request.route_id.as_str(),
                request.correlation_key.as_str(),
                request.responder_id.as_deref(),
                request.reply_text.as_str(),
                rejection_code,
                rejection_message,
                request.received_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "notification_reply_claim_persist_failed",
                database_path,
                error,
                "Cadence could not persist the rejected notification reply claim.",
            )
        })?;

    Ok(transaction.last_insert_rowid())
}

fn read_operator_approvals(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Vec<OperatorApprovalDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
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
            FROM operator_approvals
            WHERE project_id = ?1
            ORDER BY
                CASE status WHEN 'pending' THEN 0 ELSE 1 END ASC,
                COALESCE(resolved_at, updated_at, created_at) DESC,
                action_id ASC
            LIMIT ?2
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "operator_approval_query_failed",
                format!(
                    "Cadence could not prepare operator-approval rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map(
            params![expected_project_id, MAX_APPROVAL_REQUEST_ROWS],
            |row| {
                Ok(RawOperatorApprovalRow {
                    action_id: row.get(0)?,
                    session_id: row.get(1)?,
                    flow_id: row.get(2)?,
                    action_type: row.get(3)?,
                    title: row.get(4)?,
                    detail: row.get(5)?,
                    gate_node_id: row.get(6)?,
                    gate_key: row.get(7)?,
                    transition_from_node_id: row.get(8)?,
                    transition_to_node_id: row.get(9)?,
                    transition_kind: row.get(10)?,
                    user_answer: row.get(11)?,
                    status: row.get(12)?,
                    decision_note: row.get(13)?,
                    created_at: row.get(14)?,
                    updated_at: row.get(15)?,
                    resolved_at: row.get(16)?,
                })
            },
        )
        .map_err(|error| {
            CommandError::system_fault(
                "operator_approval_query_failed",
                format!(
                    "Cadence could not query operator-approval rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "operator_approval_decode_failed",
                        format!(
                            "Cadence could not decode operator-approval rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_operator_approval_row(raw_row, database_path))
        })
        .collect()
}

fn read_verification_records(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Vec<VerificationRecordDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                source_action_id,
                status,
                summary,
                detail,
                recorded_at
            FROM operator_verification_records
            WHERE project_id = ?1
            ORDER BY recorded_at DESC, id DESC
            LIMIT ?2
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "verification_record_query_failed",
                format!(
                    "Cadence could not prepare verification rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map(
            params![expected_project_id, MAX_VERIFICATION_RECORD_ROWS],
            |row| {
                Ok(RawVerificationRecordRow {
                    id: row.get(0)?,
                    source_action_id: row.get(1)?,
                    status: row.get(2)?,
                    summary: row.get(3)?,
                    detail: row.get(4)?,
                    recorded_at: row.get(5)?,
                })
            },
        )
        .map_err(|error| {
            CommandError::system_fault(
                "verification_record_query_failed",
                format!(
                    "Cadence could not query verification rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "verification_record_decode_failed",
                        format!(
                            "Cadence could not decode verification rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_verification_record_row(raw_row, database_path))
        })
        .collect()
}

fn read_resume_history(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Vec<ResumeHistoryEntryDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                source_action_id,
                session_id,
                status,
                summary,
                created_at
            FROM operator_resume_history
            WHERE project_id = ?1
            ORDER BY created_at DESC, id DESC
            LIMIT ?2
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "resume_history_query_failed",
                format!(
                    "Cadence could not prepare resume-history rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let raw_rows = statement
        .query_map(
            params![expected_project_id, MAX_RESUME_HISTORY_ROWS],
            |row| {
                Ok(RawResumeHistoryRow {
                    id: row.get(0)?,
                    source_action_id: row.get(1)?,
                    session_id: row.get(2)?,
                    status: row.get(3)?,
                    summary: row.get(4)?,
                    created_at: row.get(5)?,
                })
            },
        )
        .map_err(|error| {
            CommandError::system_fault(
                "resume_history_query_failed",
                format!(
                    "Cadence could not query resume-history rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    raw_rows
        .map(|raw_row| {
            raw_row
                .map_err(|error| {
                    CommandError::system_fault(
                        "resume_history_decode_failed",
                        format!(
                            "Cadence could not decode resume-history rows from {}: {error}",
                            database_path.display()
                        ),
                    )
                })
                .and_then(|raw_row| decode_resume_history_row(raw_row, database_path))
        })
        .collect()
}

fn read_operator_approval_by_action_id(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    action_id: &str,
) -> Result<Option<OperatorApprovalDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
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
            FROM operator_approvals
            WHERE project_id = ?1
              AND action_id = ?2
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "operator_approval_query_failed",
                format!(
                    "Cadence could not prepare operator-approval lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement
        .query(params![project_id, action_id])
        .map_err(|error| {
            CommandError::system_fault(
                "operator_approval_query_failed",
                format!(
                    "Cadence could not query operator-approval lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "operator_approval_query_failed",
            format!(
                "Cadence could not read operator-approval lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_operator_approval_row(
        RawOperatorApprovalRow {
            action_id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            session_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            flow_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            action_type: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            title: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            detail: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            gate_node_id: row.get(6).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            gate_key: row.get(7).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            transition_from_node_id: row.get(8).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            transition_to_node_id: row.get(9).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            transition_kind: row.get(10).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            user_answer: row.get(11).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(12).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            decision_note: row.get(13).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(14).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            updated_at: row.get(15).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            resolved_at: row.get(16).map_err(|error| {
                CommandError::system_fault(
                    "operator_approval_decode_failed",
                    format!(
                        "Cadence could not decode operator-approval lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_verification_record_by_id(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    id: i64,
) -> Result<Option<VerificationRecordDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                source_action_id,
                status,
                summary,
                detail,
                recorded_at
            FROM operator_verification_records
            WHERE project_id = ?1
              AND id = ?2
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "verification_record_query_failed",
                format!(
                    "Cadence could not prepare verification-record lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement.query(params![project_id, id]).map_err(|error| {
        CommandError::system_fault(
            "verification_record_query_failed",
            format!(
                "Cadence could not query verification-record lookup from {}: {error}",
                database_path.display()
            ),
        )
    })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "verification_record_query_failed",
            format!(
                "Cadence could not read verification-record lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_verification_record_row(
        RawVerificationRecordRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "verification_record_decode_failed",
                    format!(
                        "Cadence could not decode verification-record lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            source_action_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "verification_record_decode_failed",
                    format!(
                        "Cadence could not decode verification-record lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "verification_record_decode_failed",
                    format!(
                        "Cadence could not decode verification-record lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            summary: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "verification_record_decode_failed",
                    format!(
                        "Cadence could not decode verification-record lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            detail: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "verification_record_decode_failed",
                    format!(
                        "Cadence could not decode verification-record lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            recorded_at: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "verification_record_decode_failed",
                    format!(
                        "Cadence could not decode verification-record lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn read_resume_history_entry_by_id(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    id: i64,
) -> Result<Option<ResumeHistoryEntryDto>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                source_action_id,
                session_id,
                status,
                summary,
                created_at
            FROM operator_resume_history
            WHERE project_id = ?1
              AND id = ?2
            LIMIT 1
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "resume_history_query_failed",
                format!(
                    "Cadence could not prepare resume-history lookup from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut rows = statement.query(params![project_id, id]).map_err(|error| {
        CommandError::system_fault(
            "resume_history_query_failed",
            format!(
                "Cadence could not query resume-history lookup from {}: {error}",
                database_path.display()
            ),
        )
    })?;

    let Some(row) = rows.next().map_err(|error| {
        CommandError::system_fault(
            "resume_history_query_failed",
            format!(
                "Cadence could not read resume-history lookup rows from {}: {error}",
                database_path.display()
            ),
        )
    })?
    else {
        return Ok(None);
    };

    decode_resume_history_row(
        RawResumeHistoryRow {
            id: row.get(0).map_err(|error| {
                CommandError::system_fault(
                    "resume_history_decode_failed",
                    format!(
                        "Cadence could not decode resume-history lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            source_action_id: row.get(1).map_err(|error| {
                CommandError::system_fault(
                    "resume_history_decode_failed",
                    format!(
                        "Cadence could not decode resume-history lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            session_id: row.get(2).map_err(|error| {
                CommandError::system_fault(
                    "resume_history_decode_failed",
                    format!(
                        "Cadence could not decode resume-history lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            status: row.get(3).map_err(|error| {
                CommandError::system_fault(
                    "resume_history_decode_failed",
                    format!(
                        "Cadence could not decode resume-history lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            summary: row.get(4).map_err(|error| {
                CommandError::system_fault(
                    "resume_history_decode_failed",
                    format!(
                        "Cadence could not decode resume-history lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            created_at: row.get(5).map_err(|error| {
                CommandError::system_fault(
                    "resume_history_decode_failed",
                    format!(
                        "Cadence could not decode resume-history lookup rows from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
        },
        database_path,
    )
    .map(Some)
}

fn decode_notification_route_row(
    raw_row: RawNotificationRouteRow,
    database_path: &Path,
) -> Result<NotificationRouteRecord, CommandError> {
    let enabled = match raw_row.enabled {
        0 => false,
        1 => true,
        value => {
            return Err(map_snapshot_decode_error(
                "notification_route_decode_failed",
                database_path,
                format!("Field `enabled` must be 0 or 1, found {value}."),
            ))
        }
    };

    let metadata_json = decode_optional_non_empty_text(
        raw_row.metadata_json,
        "metadata_json",
        database_path,
        "notification_route_decode_failed",
    )?;
    if let Some(metadata_json) = metadata_json.as_deref() {
        serde_json::from_str::<serde_json::Value>(metadata_json).map_err(|error| {
            map_snapshot_decode_error(
                "notification_route_decode_failed",
                database_path,
                format!("Field `metadata_json` must be valid JSON text: {error}"),
            )
        })?;
    }

    Ok(NotificationRouteRecord {
        project_id: require_non_empty_owned(
            raw_row.project_id,
            "project_id",
            database_path,
            "notification_route_decode_failed",
        )?,
        route_id: require_non_empty_owned(
            raw_row.route_id,
            "route_id",
            database_path,
            "notification_route_decode_failed",
        )?,
        route_kind: require_non_empty_owned(
            raw_row.route_kind,
            "route_kind",
            database_path,
            "notification_route_decode_failed",
        )?,
        route_target: require_non_empty_owned(
            raw_row.route_target,
            "route_target",
            database_path,
            "notification_route_decode_failed",
        )?,
        enabled,
        metadata_json,
        created_at: require_non_empty_owned(
            raw_row.created_at,
            "created_at",
            database_path,
            "notification_route_decode_failed",
        )?,
        updated_at: require_non_empty_owned(
            raw_row.updated_at,
            "updated_at",
            database_path,
            "notification_route_decode_failed",
        )?,
    })
}

fn decode_notification_dispatch_row(
    raw_row: RawNotificationDispatchRow,
    database_path: &Path,
) -> Result<NotificationDispatchRecord, CommandError> {
    let correlation_key = require_non_empty_owned(
        raw_row.correlation_key,
        "correlation_key",
        database_path,
        "notification_dispatch_decode_failed",
    )?;
    validate_notification_correlation_key(
        &correlation_key,
        "correlation_key",
        "notification_dispatch_decode_failed",
    )?;

    let status = parse_notification_dispatch_status(&raw_row.status).map_err(|details| {
        map_snapshot_decode_error(
            "notification_dispatch_decode_failed",
            database_path,
            details,
        )
    })?;

    let attempt_count = u32::try_from(raw_row.attempt_count).map_err(|_| {
        map_snapshot_decode_error(
            "notification_dispatch_decode_failed",
            database_path,
            format!(
                "Field `attempt_count` must be a non-negative 32-bit integer, found {}.",
                raw_row.attempt_count
            ),
        )
    })?;

    let last_attempt_at = decode_optional_non_empty_text(
        raw_row.last_attempt_at,
        "last_attempt_at",
        database_path,
        "notification_dispatch_decode_failed",
    )?;
    let delivered_at = decode_optional_non_empty_text(
        raw_row.delivered_at,
        "delivered_at",
        database_path,
        "notification_dispatch_decode_failed",
    )?;
    let claimed_at = decode_optional_non_empty_text(
        raw_row.claimed_at,
        "claimed_at",
        database_path,
        "notification_dispatch_decode_failed",
    )?;
    let last_error_code = decode_optional_non_empty_text(
        raw_row.last_error_code,
        "last_error_code",
        database_path,
        "notification_dispatch_decode_failed",
    )?;
    let last_error_message = decode_optional_non_empty_text(
        raw_row.last_error_message,
        "last_error_message",
        database_path,
        "notification_dispatch_decode_failed",
    )?;

    if matches!(status, NotificationDispatchStatus::Sent) && delivered_at.is_none() {
        return Err(map_snapshot_decode_error(
            "notification_dispatch_decode_failed",
            database_path,
            "Sent notification dispatch rows must include delivered_at.".into(),
        ));
    }

    if matches!(status, NotificationDispatchStatus::Claimed) && claimed_at.is_none() {
        return Err(map_snapshot_decode_error(
            "notification_dispatch_decode_failed",
            database_path,
            "Claimed notification dispatch rows must include claimed_at.".into(),
        ));
    }

    if matches!(status, NotificationDispatchStatus::Failed)
        && (last_error_code.is_none() || last_error_message.is_none())
    {
        return Err(map_snapshot_decode_error(
            "notification_dispatch_decode_failed",
            database_path,
            "Failed notification dispatch rows must include last_error_code and last_error_message."
                .into(),
        ));
    }

    Ok(NotificationDispatchRecord {
        id: raw_row.id,
        project_id: require_non_empty_owned(
            raw_row.project_id,
            "project_id",
            database_path,
            "notification_dispatch_decode_failed",
        )?,
        action_id: require_non_empty_owned(
            raw_row.action_id,
            "action_id",
            database_path,
            "notification_dispatch_decode_failed",
        )?,
        route_id: require_non_empty_owned(
            raw_row.route_id,
            "route_id",
            database_path,
            "notification_dispatch_decode_failed",
        )?,
        correlation_key,
        status,
        attempt_count,
        last_attempt_at,
        delivered_at,
        claimed_at,
        last_error_code,
        last_error_message,
        created_at: require_non_empty_owned(
            raw_row.created_at,
            "created_at",
            database_path,
            "notification_dispatch_decode_failed",
        )?,
        updated_at: require_non_empty_owned(
            raw_row.updated_at,
            "updated_at",
            database_path,
            "notification_dispatch_decode_failed",
        )?,
    })
}

fn decode_notification_reply_claim_row(
    raw_row: RawNotificationReplyClaimRow,
    database_path: &Path,
) -> Result<NotificationReplyClaimRecord, CommandError> {
    let correlation_key = require_non_empty_owned(
        raw_row.correlation_key,
        "correlation_key",
        database_path,
        "notification_reply_decode_failed",
    )?;
    validate_notification_correlation_key(
        &correlation_key,
        "correlation_key",
        "notification_reply_decode_failed",
    )?;

    let reply_text = require_non_empty_owned(
        raw_row.reply_text,
        "reply_text",
        database_path,
        "notification_reply_decode_failed",
    )?;
    if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&reply_text) {
        return Err(map_snapshot_decode_error(
            "notification_reply_decode_failed",
            database_path,
            format!("Field `reply_text` must not include {secret_hint}."),
        ));
    }

    let status = parse_notification_reply_claim_status(&raw_row.status).map_err(|details| {
        map_snapshot_decode_error("notification_reply_decode_failed", database_path, details)
    })?;

    let responder_id = decode_optional_non_empty_text(
        raw_row.responder_id,
        "responder_id",
        database_path,
        "notification_reply_decode_failed",
    )?;
    let rejection_code = decode_optional_non_empty_text(
        raw_row.rejection_code,
        "rejection_code",
        database_path,
        "notification_reply_decode_failed",
    )?;
    let rejection_message = decode_optional_non_empty_text(
        raw_row.rejection_message,
        "rejection_message",
        database_path,
        "notification_reply_decode_failed",
    )?;

    match status {
        NotificationReplyClaimStatus::Accepted => {
            if rejection_code.is_some() || rejection_message.is_some() {
                return Err(map_snapshot_decode_error(
                    "notification_reply_decode_failed",
                    database_path,
                    "Accepted notification reply claims must not include rejection_code or rejection_message."
                        .into(),
                ));
            }
        }
        NotificationReplyClaimStatus::Rejected => {
            if rejection_code.is_none() || rejection_message.is_none() {
                return Err(map_snapshot_decode_error(
                    "notification_reply_decode_failed",
                    database_path,
                    "Rejected notification reply claims must include rejection_code and rejection_message."
                        .into(),
                ));
            }
        }
    }

    Ok(NotificationReplyClaimRecord {
        id: raw_row.id,
        project_id: require_non_empty_owned(
            raw_row.project_id,
            "project_id",
            database_path,
            "notification_reply_decode_failed",
        )?,
        action_id: require_non_empty_owned(
            raw_row.action_id,
            "action_id",
            database_path,
            "notification_reply_decode_failed",
        )?,
        route_id: require_non_empty_owned(
            raw_row.route_id,
            "route_id",
            database_path,
            "notification_reply_decode_failed",
        )?,
        correlation_key,
        responder_id,
        reply_text,
        status,
        rejection_code,
        rejection_message,
        created_at: require_non_empty_owned(
            raw_row.created_at,
            "created_at",
            database_path,
            "notification_reply_decode_failed",
        )?,
    })
}

fn decode_operator_approval_row(
    raw_row: RawOperatorApprovalRow,
    database_path: &Path,
) -> Result<OperatorApprovalDto, CommandError> {
    let action_id = require_non_empty_owned(
        raw_row.action_id,
        "action_id",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let session_id = decode_optional_non_empty_text(
        raw_row.session_id,
        "session_id",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let flow_id = decode_optional_non_empty_text(
        raw_row.flow_id,
        "flow_id",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let action_type = require_non_empty_owned(
        raw_row.action_type,
        "action_type",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let title = require_non_empty_owned(
        raw_row.title,
        "title",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let detail = require_non_empty_owned(
        raw_row.detail,
        "detail",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let gate_node_id = decode_optional_non_empty_text(
        raw_row.gate_node_id,
        "gate_node_id",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let gate_key = decode_optional_non_empty_text(
        raw_row.gate_key,
        "gate_key",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let transition_from_node_id = decode_optional_non_empty_text(
        raw_row.transition_from_node_id,
        "transition_from_node_id",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let transition_to_node_id = decode_optional_non_empty_text(
        raw_row.transition_to_node_id,
        "transition_to_node_id",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let transition_kind = decode_optional_non_empty_text(
        raw_row.transition_kind,
        "transition_kind",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let user_answer = decode_optional_non_empty_text(
        raw_row.user_answer,
        "user_answer",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let created_at = require_non_empty_owned(
        raw_row.created_at,
        "created_at",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let updated_at = require_non_empty_owned(
        raw_row.updated_at,
        "updated_at",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let decision_note = decode_optional_non_empty_text(
        raw_row.decision_note,
        "decision_note",
        database_path,
        "operator_approval_decode_failed",
    )?;
    let resolved_at = decode_optional_non_empty_text(
        raw_row.resolved_at,
        "resolved_at",
        database_path,
        "operator_approval_decode_failed",
    )?;

    let status = parse_operator_approval_status(&raw_row.status).map_err(|details| {
        map_snapshot_decode_error("operator_approval_decode_failed", database_path, details)
    })?;

    let gate_fields_populated = gate_node_id.is_some() || gate_key.is_some();
    if gate_fields_populated && (gate_node_id.is_none() || gate_key.is_none()) {
        return Err(map_snapshot_decode_error(
            "operator_approval_decode_failed",
            database_path,
            "Gate-linked approval rows must include both `gate_node_id` and `gate_key`.".into(),
        ));
    }

    let continuation_fields_populated = transition_from_node_id.is_some()
        || transition_to_node_id.is_some()
        || transition_kind.is_some();
    if continuation_fields_populated
        && (transition_from_node_id.is_none()
            || transition_to_node_id.is_none()
            || transition_kind.is_none())
    {
        return Err(map_snapshot_decode_error(
            "operator_approval_decode_failed",
            database_path,
            "Gate-linked approval rows must include full transition continuation metadata (`transition_from_node_id`, `transition_to_node_id`, `transition_kind`).".into(),
        ));
    }

    if gate_fields_populated && !continuation_fields_populated {
        return Err(map_snapshot_decode_error(
            "operator_approval_decode_failed",
            database_path,
            "Gate-linked approval rows must include continuation metadata for deterministic resume.".into(),
        ));
    }

    if continuation_fields_populated && !gate_fields_populated {
        return Err(map_snapshot_decode_error(
            "operator_approval_decode_failed",
            database_path,
            "Transition continuation metadata requires matching gate identity fields.".into(),
        ));
    }

    if let (Some(gate_node_id), Some(transition_to_node_id)) =
        (gate_node_id.as_deref(), transition_to_node_id.as_deref())
    {
        if gate_node_id != transition_to_node_id {
            return Err(map_snapshot_decode_error(
                "operator_approval_decode_failed",
                database_path,
                "Gate-linked approval rows must target the same `transition_to_node_id` as `gate_node_id`.".into(),
            ));
        }
    }

    match status {
        OperatorApprovalStatus::Pending => {
            if decision_note.is_some() || resolved_at.is_some() || user_answer.is_some() {
                return Err(map_snapshot_decode_error(
                    "operator_approval_decode_failed",
                    database_path,
                    "Pending approval rows must not include decision_note, user_answer, or resolved_at."
                        .into(),
                ));
            }
        }
        OperatorApprovalStatus::Approved | OperatorApprovalStatus::Rejected => {
            if resolved_at.is_none() {
                return Err(map_snapshot_decode_error(
                    "operator_approval_decode_failed",
                    database_path,
                    "Resolved approval rows must include resolved_at.".into(),
                ));
            }
        }
    }

    Ok(OperatorApprovalDto {
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
        resolved_at,
    })
}

fn decode_verification_record_row(
    raw_row: RawVerificationRecordRow,
    database_path: &Path,
) -> Result<VerificationRecordDto, CommandError> {
    let id = decode_snapshot_row_id(
        raw_row.id,
        "id",
        database_path,
        "verification_record_decode_failed",
    )?;
    let source_action_id = decode_optional_non_empty_text(
        raw_row.source_action_id,
        "source_action_id",
        database_path,
        "verification_record_decode_failed",
    )?;
    let status = parse_verification_record_status(&raw_row.status).map_err(|details| {
        map_snapshot_decode_error("verification_record_decode_failed", database_path, details)
    })?;
    let summary = require_non_empty_owned(
        raw_row.summary,
        "summary",
        database_path,
        "verification_record_decode_failed",
    )?;
    let detail = decode_optional_non_empty_text(
        raw_row.detail,
        "detail",
        database_path,
        "verification_record_decode_failed",
    )?;
    let recorded_at = require_non_empty_owned(
        raw_row.recorded_at,
        "recorded_at",
        database_path,
        "verification_record_decode_failed",
    )?;

    Ok(VerificationRecordDto {
        id,
        source_action_id,
        status,
        summary,
        detail,
        recorded_at,
    })
}

fn decode_resume_history_row(
    raw_row: RawResumeHistoryRow,
    database_path: &Path,
) -> Result<ResumeHistoryEntryDto, CommandError> {
    let id = decode_snapshot_row_id(
        raw_row.id,
        "id",
        database_path,
        "resume_history_decode_failed",
    )?;
    let source_action_id = decode_optional_non_empty_text(
        raw_row.source_action_id,
        "source_action_id",
        database_path,
        "resume_history_decode_failed",
    )?;
    let session_id = decode_optional_non_empty_text(
        raw_row.session_id,
        "session_id",
        database_path,
        "resume_history_decode_failed",
    )?;
    let status = parse_resume_history_status(&raw_row.status).map_err(|details| {
        map_snapshot_decode_error("resume_history_decode_failed", database_path, details)
    })?;
    let summary = require_non_empty_owned(
        raw_row.summary,
        "summary",
        database_path,
        "resume_history_decode_failed",
    )?;
    let created_at = require_non_empty_owned(
        raw_row.created_at,
        "created_at",
        database_path,
        "resume_history_decode_failed",
    )?;

    Ok(ResumeHistoryEntryDto {
        id,
        source_action_id,
        session_id,
        status,
        summary,
        created_at,
    })
}

fn decode_workflow_graph_node_row(
    raw_row: RawGraphNodeRow,
    database_path: &Path,
) -> Result<WorkflowGraphNodeRecord, CommandError> {
    let phase_id = decode_snapshot_row_id(
        raw_row.phase_id,
        "phase_id",
        database_path,
        "workflow_graph_decode_failed",
    )?;
    let sort_order = decode_snapshot_row_id(
        raw_row.sort_order,
        "sort_order",
        database_path,
        "workflow_graph_decode_failed",
    )?;
    let task_count = decode_snapshot_row_id(
        raw_row.task_count,
        "task_count",
        database_path,
        "workflow_graph_decode_failed",
    )?;
    let completed_tasks = decode_snapshot_row_id(
        raw_row.completed_tasks,
        "completed_tasks",
        database_path,
        "workflow_graph_decode_failed",
    )?;

    if completed_tasks > task_count {
        return Err(map_snapshot_decode_error(
            "workflow_graph_decode_failed",
            database_path,
            format!(
                "Field `completed_tasks` cannot exceed `task_count` ({} > {}).",
                completed_tasks, task_count
            ),
        ));
    }

    Ok(WorkflowGraphNodeRecord {
        node_id: require_non_empty_owned(
            raw_row.node_id,
            "node_id",
            database_path,
            "workflow_graph_decode_failed",
        )?,
        phase_id,
        sort_order,
        name: require_non_empty_owned(
            raw_row.name,
            "name",
            database_path,
            "workflow_graph_decode_failed",
        )?,
        description: raw_row.description,
        status: parse_phase_status(&raw_row.status).map_err(|details| {
            map_snapshot_decode_error("workflow_graph_decode_failed", database_path, details)
        })?,
        current_step: raw_row
            .current_step
            .as_deref()
            .map(parse_phase_step)
            .transpose()
            .map_err(|details| {
                map_snapshot_decode_error("workflow_graph_decode_failed", database_path, details)
            })?,
        task_count,
        completed_tasks,
        summary: raw_row.summary,
    })
}

fn decode_workflow_graph_edge_row(
    raw_row: RawGraphEdgeRow,
    database_path: &Path,
) -> Result<WorkflowGraphEdgeRecord, CommandError> {
    Ok(WorkflowGraphEdgeRecord {
        from_node_id: require_non_empty_owned(
            raw_row.from_node_id,
            "from_node_id",
            database_path,
            "workflow_graph_decode_failed",
        )?,
        to_node_id: require_non_empty_owned(
            raw_row.to_node_id,
            "to_node_id",
            database_path,
            "workflow_graph_decode_failed",
        )?,
        transition_kind: require_non_empty_owned(
            raw_row.transition_kind,
            "transition_kind",
            database_path,
            "workflow_graph_decode_failed",
        )?,
        gate_requirement: decode_optional_non_empty_text(
            raw_row.gate_requirement,
            "gate_requirement",
            database_path,
            "workflow_graph_decode_failed",
        )?,
    })
}

fn decode_workflow_gate_metadata_row(
    raw_row: RawGateMetadataRow,
    database_path: &Path,
) -> Result<WorkflowGateMetadataRecord, CommandError> {
    let gate_state = parse_workflow_gate_state(&raw_row.gate_state).map_err(|details| {
        map_snapshot_decode_error("workflow_graph_decode_failed", database_path, details)
    })?;

    let action_type = decode_optional_non_empty_text(
        raw_row.action_type,
        "action_type",
        database_path,
        "workflow_graph_decode_failed",
    )?;
    let title = decode_optional_non_empty_text(
        raw_row.title,
        "title",
        database_path,
        "workflow_graph_decode_failed",
    )?;
    let detail = decode_optional_non_empty_text(
        raw_row.detail,
        "detail",
        database_path,
        "workflow_graph_decode_failed",
    )?;

    if matches!(
        gate_state,
        WorkflowGateState::Pending | WorkflowGateState::Blocked
    ) && (action_type.is_none() || title.is_none() || detail.is_none())
    {
        return Err(map_snapshot_decode_error(
            "workflow_graph_decode_failed",
            database_path,
            "Pending or blocked workflow gates must include action_type, title, and detail.".into(),
        ));
    }

    Ok(WorkflowGateMetadataRecord {
        node_id: require_non_empty_owned(
            raw_row.node_id,
            "node_id",
            database_path,
            "workflow_graph_decode_failed",
        )?,
        gate_key: require_non_empty_owned(
            raw_row.gate_key,
            "gate_key",
            database_path,
            "workflow_graph_decode_failed",
        )?,
        gate_state,
        action_type,
        title,
        detail,
        decision_context: decode_optional_non_empty_text(
            raw_row.decision_context,
            "decision_context",
            database_path,
            "workflow_graph_decode_failed",
        )?,
    })
}

fn decode_workflow_transition_event_row(
    raw_row: RawTransitionEventRow,
    database_path: &Path,
) -> Result<WorkflowTransitionEventRecord, CommandError> {
    Ok(WorkflowTransitionEventRecord {
        id: raw_row.id,
        transition_id: require_non_empty_owned(
            raw_row.transition_id,
            "transition_id",
            database_path,
            "workflow_transition_decode_failed",
        )?,
        causal_transition_id: decode_optional_non_empty_text(
            raw_row.causal_transition_id,
            "causal_transition_id",
            database_path,
            "workflow_transition_decode_failed",
        )?,
        from_node_id: require_non_empty_owned(
            raw_row.from_node_id,
            "from_node_id",
            database_path,
            "workflow_transition_decode_failed",
        )?,
        to_node_id: require_non_empty_owned(
            raw_row.to_node_id,
            "to_node_id",
            database_path,
            "workflow_transition_decode_failed",
        )?,
        transition_kind: require_non_empty_owned(
            raw_row.transition_kind,
            "transition_kind",
            database_path,
            "workflow_transition_decode_failed",
        )?,
        gate_decision: parse_workflow_transition_gate_decision(&raw_row.gate_decision).map_err(
            |details| {
                map_snapshot_decode_error(
                    "workflow_transition_decode_failed",
                    database_path,
                    details,
                )
            },
        )?,
        gate_decision_context: decode_optional_non_empty_text(
            raw_row.gate_decision_context,
            "gate_decision_context",
            database_path,
            "workflow_transition_decode_failed",
        )?,
        created_at: require_non_empty_owned(
            raw_row.created_at,
            "created_at",
            database_path,
            "workflow_transition_decode_failed",
        )?,
    })
}

fn decode_workflow_handoff_package_row(
    raw_row: RawWorkflowHandoffPackageRow,
    database_path: &Path,
) -> Result<WorkflowHandoffPackageRecord, CommandError> {
    let package_payload = require_non_empty_owned(
        raw_row.package_payload,
        "package_payload",
        database_path,
        "workflow_handoff_decode_failed",
    )?;
    let canonical_payload = canonicalize_workflow_handoff_package_payload(
        &package_payload,
        Some(database_path),
        "workflow_handoff_decode_failed",
    )?;
    if canonical_payload != package_payload {
        return Err(map_snapshot_decode_error(
            "workflow_handoff_decode_failed",
            database_path,
            "Field `package_payload` must use canonical JSON key ordering for deterministic hashing."
                .into(),
        ));
    }

    if let Some(secret_hint) = find_prohibited_workflow_handoff_content(&package_payload) {
        return Err(map_snapshot_decode_error(
            "workflow_handoff_decode_failed",
            database_path,
            format!(
                "Field `package_payload` must not include {secret_hint}; persisted handoff packages are redacted-only."
            ),
        ));
    }

    let package_hash = require_non_empty_owned(
        raw_row.package_hash,
        "package_hash",
        database_path,
        "workflow_handoff_decode_failed",
    )?;
    validate_workflow_handoff_package_hash(
        &package_hash,
        "package_hash",
        database_path,
        "workflow_handoff_decode_failed",
    )?;

    let expected_hash = compute_workflow_handoff_package_hash(&canonical_payload);
    if package_hash != expected_hash {
        return Err(map_snapshot_decode_error(
            "workflow_handoff_decode_failed",
            database_path,
            format!(
                "Field `package_hash` must match the deterministic hash of `package_payload` (expected `{expected_hash}`, found `{package_hash}`)."
            ),
        ));
    }

    let created_at = require_non_empty_owned(
        raw_row.created_at,
        "created_at",
        database_path,
        "workflow_handoff_decode_failed",
    )?;
    validate_rfc3339_timestamp(
        &created_at,
        "created_at",
        Some(database_path),
        "workflow_handoff_decode_failed",
    )?;

    Ok(WorkflowHandoffPackageRecord {
        id: raw_row.id,
        project_id: require_non_empty_owned(
            raw_row.project_id,
            "project_id",
            database_path,
            "workflow_handoff_decode_failed",
        )?,
        handoff_transition_id: require_non_empty_owned(
            raw_row.handoff_transition_id,
            "handoff_transition_id",
            database_path,
            "workflow_handoff_decode_failed",
        )?,
        causal_transition_id: decode_optional_non_empty_text(
            raw_row.causal_transition_id,
            "causal_transition_id",
            database_path,
            "workflow_handoff_decode_failed",
        )?,
        from_node_id: require_non_empty_owned(
            raw_row.from_node_id,
            "from_node_id",
            database_path,
            "workflow_handoff_decode_failed",
        )?,
        to_node_id: require_non_empty_owned(
            raw_row.to_node_id,
            "to_node_id",
            database_path,
            "workflow_handoff_decode_failed",
        )?,
        transition_kind: require_non_empty_owned(
            raw_row.transition_kind,
            "transition_kind",
            database_path,
            "workflow_handoff_decode_failed",
        )?,
        package_payload,
        package_hash,
        created_at,
    })
}

fn validate_workflow_graph_upsert_payload(
    graph: &WorkflowGraphUpsertRecord,
) -> Result<(), CommandError> {
    use std::collections::BTreeSet;

    let mut node_ids = BTreeSet::new();
    let mut phase_ids = BTreeSet::new();
    let mut sort_orders = BTreeSet::new();

    for node in &graph.nodes {
        validate_non_empty_text(&node.node_id, "node_id", "workflow_graph_request_invalid")?;
        validate_non_empty_text(&node.name, "name", "workflow_graph_request_invalid")?;

        if node.completed_tasks > node.task_count {
            return Err(CommandError::user_fixable(
                "workflow_graph_request_invalid",
                format!(
                    "Workflow node `{}` has completed_tasks ({}) greater than task_count ({}).",
                    node.node_id, node.completed_tasks, node.task_count
                ),
            ));
        }

        if !node_ids.insert(node.node_id.as_str()) {
            return Err(CommandError::user_fixable(
                "workflow_graph_request_invalid",
                format!(
                    "Workflow graph contains duplicate node id `{}`.",
                    node.node_id
                ),
            ));
        }

        if !phase_ids.insert(node.phase_id) {
            return Err(CommandError::user_fixable(
                "workflow_graph_request_invalid",
                format!(
                    "Workflow graph contains duplicate phase id `{}`.",
                    node.phase_id
                ),
            ));
        }

        if !sort_orders.insert(node.sort_order) {
            return Err(CommandError::user_fixable(
                "workflow_graph_request_invalid",
                format!(
                    "Workflow graph contains duplicate sort order `{}`.",
                    node.sort_order
                ),
            ));
        }
    }

    for edge in &graph.edges {
        validate_non_empty_text(
            &edge.from_node_id,
            "from_node_id",
            "workflow_graph_request_invalid",
        )?;
        validate_non_empty_text(
            &edge.to_node_id,
            "to_node_id",
            "workflow_graph_request_invalid",
        )?;
        validate_non_empty_text(
            &edge.transition_kind,
            "transition_kind",
            "workflow_graph_request_invalid",
        )?;

        if !node_ids.contains(edge.from_node_id.as_str())
            || !node_ids.contains(edge.to_node_id.as_str())
        {
            return Err(CommandError::user_fixable(
                "workflow_graph_request_invalid",
                format!(
                    "Workflow edge `{}` -> `{}` references unknown node ids.",
                    edge.from_node_id, edge.to_node_id
                ),
            ));
        }
    }

    for gate in &graph.gates {
        validate_non_empty_text(&gate.node_id, "node_id", "workflow_graph_request_invalid")?;
        validate_non_empty_text(&gate.gate_key, "gate_key", "workflow_graph_request_invalid")?;

        if !node_ids.contains(gate.node_id.as_str()) {
            return Err(CommandError::user_fixable(
                "workflow_graph_request_invalid",
                format!(
                    "Workflow gate `{}` references unknown node `{}`.",
                    gate.gate_key, gate.node_id
                ),
            ));
        }

        if matches!(
            gate.gate_state,
            WorkflowGateState::Pending | WorkflowGateState::Blocked
        ) && (gate.action_type.is_none() || gate.title.is_none() || gate.detail.is_none())
        {
            return Err(CommandError::user_fixable(
                "workflow_graph_request_invalid",
                format!(
                    "Workflow gate `{}` for node `{}` requires action_type/title/detail when pending or blocked.",
                    gate.gate_key, gate.node_id
                ),
            ));
        }
    }

    Ok(())
}

fn validate_workflow_transition_payload(
    transition: &ApplyWorkflowTransitionRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &transition.transition_id,
        "transition_id",
        "workflow_transition_request_invalid",
    )?;
    validate_non_empty_text(
        &transition.from_node_id,
        "from_node_id",
        "workflow_transition_request_invalid",
    )?;
    validate_non_empty_text(
        &transition.to_node_id,
        "to_node_id",
        "workflow_transition_request_invalid",
    )?;
    validate_non_empty_text(
        &transition.transition_kind,
        "transition_kind",
        "workflow_transition_request_invalid",
    )?;
    validate_non_empty_text(
        &transition.occurred_at,
        "occurred_at",
        "workflow_transition_request_invalid",
    )?;

    if transition.from_node_id == transition.to_node_id {
        return Err(CommandError::user_fixable(
            "workflow_transition_request_invalid",
            "Workflow transitions must change node ids.",
        ));
    }

    for gate_update in &transition.gate_updates {
        validate_non_empty_text(
            &gate_update.gate_key,
            "gate_key",
            "workflow_transition_request_invalid",
        )?;
    }

    if let Some(secret_hint) = transition
        .gate_decision_context
        .as_deref()
        .and_then(find_prohibited_transition_diagnostic_content)
    {
        return Err(CommandError::user_fixable(
            "workflow_transition_request_invalid",
            format!(
                "Workflow transition diagnostics must not include {secret_hint}. Remove secret-bearing transcript/tool/auth payload content before retrying."
            ),
        ));
    }

    for gate_update in &transition.gate_updates {
        if let Some(secret_hint) = gate_update
            .decision_context
            .as_deref()
            .and_then(find_prohibited_transition_diagnostic_content)
        {
            return Err(CommandError::user_fixable(
                "workflow_transition_request_invalid",
                format!(
                    "Workflow gate diagnostics for `{}` must not include {secret_hint}. Remove secret-bearing transcript/tool/auth payload content before retrying.",
                    gate_update.gate_key
                ),
            ));
        }
    }

    Ok(())
}

fn validate_workflow_handoff_package_payload(
    payload: &WorkflowHandoffPackageUpsertRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &payload.project_id,
        "project_id",
        "workflow_handoff_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.handoff_transition_id,
        "handoff_transition_id",
        "workflow_handoff_request_invalid",
    )?;
    if let Some(causal_transition_id) = payload.causal_transition_id.as_deref() {
        validate_non_empty_text(
            causal_transition_id,
            "causal_transition_id",
            "workflow_handoff_request_invalid",
        )?;
    }
    validate_non_empty_text(
        &payload.from_node_id,
        "from_node_id",
        "workflow_handoff_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.to_node_id,
        "to_node_id",
        "workflow_handoff_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.transition_kind,
        "transition_kind",
        "workflow_handoff_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.package_payload,
        "package_payload",
        "workflow_handoff_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.created_at,
        "created_at",
        "workflow_handoff_request_invalid",
    )?;
    validate_rfc3339_timestamp(
        &payload.created_at,
        "created_at",
        None,
        "workflow_handoff_request_invalid",
    )?;

    if let Some(secret_hint) = find_prohibited_workflow_handoff_content(&payload.package_payload) {
        return Err(CommandError::user_fixable(
            "workflow_handoff_request_invalid",
            format!(
                "Workflow handoff packages must not include {secret_hint}. Remove secret-bearing transcript/tool/auth payload content before retrying."
            ),
        ));
    }

    canonicalize_workflow_handoff_package_payload(
        &payload.package_payload,
        None,
        "workflow_handoff_request_invalid",
    )?;

    Ok(())
}

fn validate_workflow_handoff_transition_metadata(
    payload: &WorkflowHandoffPackageUpsertRecord,
    transition_event: &WorkflowTransitionEventRecord,
) -> Result<(), CommandError> {
    if payload.from_node_id != transition_event.from_node_id {
        return Err(CommandError::user_fixable(
            "workflow_handoff_request_invalid",
            format!(
                "Workflow handoff package source node `{}` does not match transition `{}` source node `{}`.",
                payload.from_node_id,
                payload.handoff_transition_id,
                transition_event.from_node_id
            ),
        ));
    }

    if payload.to_node_id != transition_event.to_node_id {
        return Err(CommandError::user_fixable(
            "workflow_handoff_request_invalid",
            format!(
                "Workflow handoff package target node `{}` does not match transition `{}` target node `{}`.",
                payload.to_node_id,
                payload.handoff_transition_id,
                transition_event.to_node_id
            ),
        ));
    }

    if payload.transition_kind != transition_event.transition_kind {
        return Err(CommandError::user_fixable(
            "workflow_handoff_request_invalid",
            format!(
                "Workflow handoff package transition kind `{}` does not match transition `{}` kind `{}`.",
                payload.transition_kind,
                payload.handoff_transition_id,
                transition_event.transition_kind
            ),
        ));
    }

    if let Some(causal_transition_id) = payload.causal_transition_id.as_deref() {
        if transition_event.causal_transition_id.as_deref() != Some(causal_transition_id) {
            return Err(CommandError::user_fixable(
                "workflow_handoff_request_invalid",
                format!(
                    "Workflow handoff package causal transition `{}` does not match transition `{}` causal linkage `{:?}`.",
                    causal_transition_id,
                    payload.handoff_transition_id,
                    transition_event.causal_transition_id
                ),
            ));
        }
    }

    Ok(())
}

fn canonicalize_workflow_handoff_package_payload(
    value: &str,
    database_path: Option<&Path>,
    code: &str,
) -> Result<String, CommandError> {
    let parsed: serde_json::Value = serde_json::from_str(value).map_err(|error| {
        map_workflow_handoff_payload_error(
            code,
            database_path,
            format!("Field `package_payload` must be valid JSON text: {error}"),
        )
    })?;

    if !parsed.is_object() {
        return Err(map_workflow_handoff_payload_error(
            code,
            database_path,
            "Field `package_payload` must be a JSON object with redacted context metadata.".into(),
        ));
    }

    let canonical = canonicalize_workflow_handoff_json_value(parsed);
    serde_json::to_string(&canonical).map_err(|error| {
        map_workflow_handoff_payload_error(
            code,
            database_path,
            format!("Field `package_payload` could not be canonicalized: {error}"),
        )
    })
}

fn canonicalize_workflow_handoff_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted = std::collections::BTreeMap::new();
            for (key, nested) in map {
                sorted.insert(key, canonicalize_workflow_handoff_json_value(nested));
            }

            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(canonicalize_workflow_handoff_json_value)
                .collect(),
        ),
        other => other,
    }
}

fn compute_workflow_handoff_package_hash(canonical_payload: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_payload.as_bytes());
    let digest = hasher.finalize();

    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn validate_workflow_handoff_package_hash(
    value: &str,
    field: &str,
    database_path: &Path,
    code: &str,
) -> Result<(), CommandError> {
    if value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(map_snapshot_decode_error(
            code,
            database_path,
            format!("Field `{field}` must be a 64-character hexadecimal hash."),
        ));
    }

    if value
        .chars()
        .any(|character| character.is_ascii_uppercase())
    {
        return Err(map_snapshot_decode_error(
            code,
            database_path,
            format!("Field `{field}` must use lowercase hexadecimal characters."),
        ));
    }

    Ok(())
}

fn validate_rfc3339_timestamp(
    value: &str,
    field: &str,
    database_path: Option<&Path>,
    code: &str,
) -> Result<(), CommandError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| {
        map_workflow_handoff_payload_error(
            code,
            database_path,
            format!("Field `{field}` must be RFC3339 text: {error}"),
        )
    })?;

    Ok(())
}

fn map_workflow_handoff_payload_error(
    code: &str,
    database_path: Option<&Path>,
    details: String,
) -> CommandError {
    match database_path {
        Some(database_path) => map_snapshot_decode_error(code, database_path, details),
        None => CommandError::user_fixable(code, details),
    }
}

struct ValidatedNotificationRouteUpsertPayload {
    route_kind: NotificationRouteKind,
    canonical_route_target: String,
}

fn validate_notification_route_upsert_payload(
    route: &NotificationRouteUpsertRecord,
) -> Result<ValidatedNotificationRouteUpsertPayload, CommandError> {
    validate_non_empty_text(
        &route.project_id,
        "project_id",
        "notification_route_request_invalid",
    )?;
    validate_non_empty_text(
        &route.route_id,
        "route_id",
        "notification_route_request_invalid",
    )?;
    validate_non_empty_text(
        &route.route_kind,
        "route_kind",
        "notification_route_request_invalid",
    )?;
    validate_non_empty_text(
        &route.route_target,
        "route_target",
        "notification_route_request_invalid",
    )?;
    validate_non_empty_text(
        &route.updated_at,
        "updated_at",
        "notification_route_request_invalid",
    )?;

    let route_kind = NotificationRouteKind::parse(&route.route_kind).map_err(|error| {
        CommandError::user_fixable("notification_route_request_invalid", error.message)
    })?;

    let canonical_route_target =
        parse_notification_route_target_for_kind(route_kind, &route.route_target)
            .map(|target| target.canonical())
            .map_err(|error| {
                CommandError::user_fixable("notification_route_request_invalid", error.message)
            })?;

    if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&canonical_route_target)
    {
        return Err(CommandError::user_fixable(
            "notification_route_request_invalid",
            format!(
                "Notification route targets must not include {secret_hint}. Persist non-secret route identifiers only."
            ),
        ));
    }

    if let Some(metadata_json) = route.metadata_json.as_deref() {
        let _ = normalize_optional_notification_metadata_json(
            Some(metadata_json),
            "notification_route_request_invalid",
        )?;
    }

    Ok(ValidatedNotificationRouteUpsertPayload {
        route_kind,
        canonical_route_target,
    })
}

fn normalize_optional_notification_metadata_json(
    metadata_json: Option<&str>,
    code: &str,
) -> Result<Option<String>, CommandError> {
    let Some(metadata_json) = metadata_json
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    if let Some(secret_hint) = find_prohibited_runtime_persistence_content(metadata_json) {
        return Err(CommandError::user_fixable(
            code,
            format!(
                "Notification route metadata must not include {secret_hint}. Persist redacted, non-secret metadata only."
            ),
        ));
    }

    let value: serde_json::Value = serde_json::from_str(metadata_json).map_err(|error| {
        CommandError::user_fixable(
            code,
            format!("Field `metadata_json` must be valid JSON text: {error}"),
        )
    })?;

    if !value.is_object() {
        return Err(CommandError::user_fixable(
            code,
            "Field `metadata_json` must be a JSON object.",
        ));
    }

    serde_json::to_string(&value).map(Some).map_err(|error| {
        CommandError::system_fault(
            code,
            format!("Cadence could not canonicalize notification route metadata JSON: {error}"),
        )
    })
}

fn validate_notification_dispatch_enqueue_payload(
    enqueue: &NotificationDispatchEnqueueRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &enqueue.project_id,
        "project_id",
        "notification_dispatch_request_invalid",
    )?;
    validate_non_empty_text(
        &enqueue.action_id,
        "action_id",
        "notification_dispatch_request_invalid",
    )?;
    validate_non_empty_text(
        &enqueue.enqueued_at,
        "enqueued_at",
        "notification_dispatch_request_invalid",
    )?;

    Ok(())
}

fn validate_notification_dispatch_outcome_update_payload(
    outcome: &NotificationDispatchOutcomeUpdateRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &outcome.project_id,
        "project_id",
        "notification_dispatch_request_invalid",
    )?;
    validate_non_empty_text(
        &outcome.action_id,
        "action_id",
        "notification_dispatch_request_invalid",
    )?;
    validate_non_empty_text(
        &outcome.route_id,
        "route_id",
        "notification_dispatch_request_invalid",
    )?;
    validate_non_empty_text(
        &outcome.attempted_at,
        "attempted_at",
        "notification_dispatch_request_invalid",
    )?;

    match outcome.status {
        NotificationDispatchStatus::Sent => {
            if outcome.error_code.is_some() || outcome.error_message.is_some() {
                return Err(CommandError::user_fixable(
                    "notification_dispatch_request_invalid",
                    "Sent notification-dispatch outcomes must not include error_code or error_message.",
                ));
            }
        }
        NotificationDispatchStatus::Failed => {
            let error_code = outcome
                .error_code
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "notification_dispatch_request_invalid",
                        "Failed notification-dispatch outcomes must include non-empty error_code.",
                    )
                })?;
            let error_message = outcome
                .error_message
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "notification_dispatch_request_invalid",
                        "Failed notification-dispatch outcomes must include non-empty error_message.",
                    )
                })?;

            if let Some(secret_hint) = find_prohibited_runtime_persistence_content(error_message) {
                return Err(CommandError::user_fixable(
                    "notification_dispatch_request_invalid",
                    format!(
                        "Notification-dispatch failure diagnostics must not include {secret_hint}."
                    ),
                ));
            }

            let _ = error_code;
        }
        NotificationDispatchStatus::Pending | NotificationDispatchStatus::Claimed => {
            return Err(CommandError::user_fixable(
                "notification_dispatch_request_invalid",
                "Dispatch outcomes must use `sent` or `failed` status values.",
            ));
        }
    }

    Ok(())
}

fn validate_notification_reply_claim_request_payload(
    request: &NotificationReplyClaimRequestRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &request.project_id,
        "project_id",
        "notification_reply_request_invalid",
    )?;
    validate_non_empty_text(
        &request.action_id,
        "action_id",
        "notification_reply_request_invalid",
    )?;
    validate_non_empty_text(
        &request.route_id,
        "route_id",
        "notification_reply_request_invalid",
    )?;
    validate_non_empty_text(
        &request.correlation_key,
        "correlation_key",
        "notification_reply_request_invalid",
    )?;
    validate_non_empty_text(
        &request.reply_text,
        "reply_text",
        "notification_reply_request_invalid",
    )?;
    validate_non_empty_text(
        &request.received_at,
        "received_at",
        "notification_reply_request_invalid",
    )?;
    if let Some(responder_id) = request.responder_id.as_deref() {
        validate_non_empty_text(
            responder_id,
            "responder_id",
            "notification_reply_request_invalid",
        )?;
    }

    validate_notification_correlation_key(
        &request.correlation_key,
        "correlation_key",
        "notification_reply_request_invalid",
    )?;

    if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&request.reply_text) {
        return Err(CommandError::user_fixable(
            "notification_reply_request_invalid",
            format!(
                "Notification reply payloads must not include {secret_hint}. Remove secret-bearing material before retrying."
            ),
        ));
    }

    Ok(())
}

fn derive_notification_correlation_key(
    project_id: &str,
    action_id: &str,
    route_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project_id.trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(action_id.trim().as_bytes());
    hasher.update(b"\n");
    hasher.update(route_id.trim().as_bytes());

    let digest = hasher.finalize();
    let digest_hex: String = digest
        .iter()
        .take(NOTIFICATION_CORRELATION_KEY_HEX_LEN / 2)
        .map(|byte| format!("{byte:02x}"))
        .collect();

    format!("{NOTIFICATION_CORRELATION_KEY_PREFIX}:{digest_hex}")
}

fn validate_notification_correlation_key(
    value: &str,
    field: &str,
    code: &str,
) -> Result<(), CommandError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CommandError::user_fixable(
            code,
            format!("Field `{field}` must be a non-empty string."),
        ));
    }

    let prefix = format!("{NOTIFICATION_CORRELATION_KEY_PREFIX}:");
    let Some(suffix) = value.strip_prefix(prefix.as_str()) else {
        return Err(CommandError::user_fixable(
            code,
            format!("Field `{field}` must start with `{NOTIFICATION_CORRELATION_KEY_PREFIX}:`."),
        ));
    };

    if suffix.len() != NOTIFICATION_CORRELATION_KEY_HEX_LEN
        || !suffix
            .chars()
            .all(|character| character.is_ascii_hexdigit())
        || suffix
            .chars()
            .any(|character| character.is_ascii_uppercase())
    {
        return Err(CommandError::user_fixable(
            code,
            format!(
                "Field `{field}` must use `{NOTIFICATION_CORRELATION_KEY_PREFIX}:` plus {} lowercase hexadecimal characters.",
                NOTIFICATION_CORRELATION_KEY_HEX_LEN
            ),
        ));
    }

    Ok(())
}

fn build_transition_mutation_record(
    transition: &ApplyWorkflowTransitionRecord,
) -> WorkflowTransitionMutationRecord {
    WorkflowTransitionMutationRecord {
        transition_id: transition.transition_id.clone(),
        causal_transition_id: transition.causal_transition_id.clone(),
        from_node_id: transition.from_node_id.clone(),
        to_node_id: transition.to_node_id.clone(),
        transition_kind: transition.transition_kind.clone(),
        gate_decision: transition.gate_decision.clone(),
        gate_decision_context: transition.gate_decision_context.clone(),
        gate_updates: transition
            .gate_updates
            .iter()
            .map(|gate_update| WorkflowTransitionGateMutationRecord {
                node_id: transition.to_node_id.clone(),
                gate_key: gate_update.gate_key.clone(),
                gate_state: gate_update.gate_state.clone(),
                decision_context: gate_update.decision_context.clone(),
                require_pending_or_blocked: false,
            })
            .collect(),
        required_gate_requirement: None,
        occurred_at: transition.occurred_at.clone(),
    }
}

fn derive_resume_transition_id(
    action_id: &str,
    context: &OperatorResumeTransitionContext,
) -> String {
    let suffix = stable_transition_id_suffix(&[
        "resume",
        action_id.trim(),
        context.transition_from_node_id.as_str(),
        context.transition_to_node_id.as_str(),
        context.transition_kind.as_str(),
        context.gate_key.as_str(),
    ]);

    format!("resume:{}:{suffix}", action_id.trim())
}

fn derive_automatic_transition_id(
    causal_transition_id: &str,
    candidate: &WorkflowAutomaticDispatchCandidate,
) -> String {
    let suffix = stable_transition_id_suffix(&[
        "auto",
        causal_transition_id,
        candidate.from_node_id.as_str(),
        candidate.to_node_id.as_str(),
        candidate.transition_kind.as_str(),
        candidate.gate_requirement.as_deref().unwrap_or("no_gate"),
    ]);

    format!("auto:{causal_transition_id}:{suffix}")
}

fn stable_transition_id_suffix(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\n");
    }

    let digest = hasher.finalize();
    digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn apply_workflow_transition_mutation(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    transition: &WorkflowTransitionMutationRecord,
    error_profile: &WorkflowTransitionMutationErrorProfile,
    map_sql_error: WorkflowTransitionSqlErrorMapper,
) -> Result<WorkflowTransitionMutationApplyOutcome, CommandError> {
    if let Some(existing) = read_transition_event_by_transition_id(
        transaction,
        database_path,
        project_id,
        &transition.transition_id,
    )? {
        return Ok(WorkflowTransitionMutationApplyOutcome::Replayed(existing));
    }

    assert_transition_edge_exists(
        transaction,
        database_path,
        project_id,
        &transition.from_node_id,
        &transition.to_node_id,
        &transition.transition_kind,
        transition.required_gate_requirement.as_deref(),
        error_profile,
        map_sql_error,
    )?;

    for gate_update in &transition.gate_updates {
        validate_non_empty_text(
            &gate_update.gate_key,
            "gate_key",
            "workflow_transition_request_invalid",
        )?;

        let update_statement = if gate_update.require_pending_or_blocked {
            r#"
            UPDATE workflow_gate_metadata
            SET gate_state = ?4,
                decision_context = ?5,
                updated_at = ?6
            WHERE project_id = ?1
              AND node_id = ?2
              AND gate_key = ?3
              AND gate_state IN ('pending', 'blocked')
            "#
        } else {
            r#"
            UPDATE workflow_gate_metadata
            SET gate_state = ?4,
                decision_context = ?5,
                updated_at = ?6
            WHERE project_id = ?1
              AND node_id = ?2
              AND gate_key = ?3
            "#
        };

        let updated = transaction
            .execute(
                update_statement,
                params![
                    project_id,
                    gate_update.node_id.as_str(),
                    gate_update.gate_key.as_str(),
                    workflow_gate_state_sql_value(&gate_update.gate_state),
                    gate_update.decision_context.as_deref(),
                    transition.occurred_at.as_str(),
                ],
            )
            .map_err(|error| {
                map_sql_error(
                    error_profile.gate_update_failed_code,
                    database_path,
                    error,
                    error_profile.gate_update_failed_message,
                )
            })?;

        if updated == 0 {
            let gate_missing_detail = if gate_update.require_pending_or_blocked {
                format!(
                    "gate `{}` is not defined for workflow node `{}` in a pending or blocked state",
                    gate_update.gate_key, gate_update.node_id
                )
            } else {
                format!(
                    "gate `{}` is not defined for workflow node `{}`",
                    gate_update.gate_key, gate_update.node_id
                )
            };

            return Err(CommandError::user_fixable(
                "workflow_transition_gate_not_found",
                format!(
                    "Cadence could not apply transition `{}` because {gate_missing_detail}.",
                    transition.transition_id
                ),
            ));
        }
    }

    let mut gate_state_statement = transaction
        .prepare(
            r#"
            SELECT gate_state
            FROM workflow_gate_metadata
            WHERE project_id = ?1
              AND node_id = ?2
            "#,
        )
        .map_err(|error| {
            map_sql_error(
                error_profile.gate_check_failed_code,
                database_path,
                error,
                error_profile.gate_check_failed_message,
            )
        })?;

    let gate_states = gate_state_statement
        .query_map(params![project_id, transition.to_node_id.as_str()], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| {
            map_sql_error(
                error_profile.gate_check_failed_code,
                database_path,
                error,
                error_profile.gate_check_failed_message,
            )
        })?;

    let mut unresolved_gate_count = 0_i64;
    for gate_state_row in gate_states {
        let raw_gate_state = gate_state_row.map_err(|error| {
            map_sql_error(
                error_profile.gate_check_failed_code,
                database_path,
                error,
                error_profile.gate_check_failed_message,
            )
        })?;

        let parsed_gate_state = parse_workflow_gate_state(raw_gate_state.trim()).map_err(|reason| {
            CommandError::system_fault(
                error_profile.gate_check_failed_code,
                format!(
                    "Cadence found malformed workflow gate metadata while applying transition `{}`: {reason}",
                    transition.transition_id
                ),
            )
        })?;

        if matches!(
            parsed_gate_state,
            WorkflowGateState::Pending | WorkflowGateState::Blocked
        ) {
            unresolved_gate_count += 1;
        }
    }

    if unresolved_gate_count > 0 {
        return Err(CommandError::user_fixable(
            "workflow_transition_gate_unmet",
            format!(
                "Cadence cannot transition from `{}` to `{}` because {unresolved_gate_count} required gate(s) are still pending or blocked.",
                transition.from_node_id, transition.to_node_id
            ),
        ));
    }

    let source_updated = transaction
        .execute(
            r#"
            UPDATE workflow_graph_nodes
            SET status = 'complete',
                updated_at = ?3
            WHERE project_id = ?1
              AND node_id = ?2
            "#,
            params![
                project_id,
                transition.from_node_id.as_str(),
                transition.occurred_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_sql_error(
                error_profile.source_update_failed_code,
                database_path,
                error,
                error_profile.source_update_failed_message,
            )
        })?;

    if source_updated == 0 {
        return Err(CommandError::user_fixable(
            "workflow_transition_source_missing",
            format!(
                "Cadence cannot apply transition `{}` because source node `{}` does not exist.",
                transition.transition_id, transition.from_node_id
            ),
        ));
    }

    let target_updated = transaction
        .execute(
            r#"
            UPDATE workflow_graph_nodes
            SET status = 'active',
                updated_at = ?3
            WHERE project_id = ?1
              AND node_id = ?2
            "#,
            params![
                project_id,
                transition.to_node_id.as_str(),
                transition.occurred_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_sql_error(
                error_profile.target_update_failed_code,
                database_path,
                error,
                error_profile.target_update_failed_message,
            )
        })?;

    if target_updated == 0 {
        return Err(CommandError::user_fixable(
            "workflow_transition_target_missing",
            format!(
                "Cadence cannot apply transition `{}` because target node `{}` does not exist.",
                transition.transition_id, transition.to_node_id
            ),
        ));
    }

    let event_insert_result = transaction.execute(
        r#"
            INSERT INTO workflow_transition_events (
                project_id,
                transition_id,
                causal_transition_id,
                from_node_id,
                to_node_id,
                transition_kind,
                gate_decision,
                gate_decision_context,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
        params![
            project_id,
            transition.transition_id.as_str(),
            transition.causal_transition_id.as_deref(),
            transition.from_node_id.as_str(),
            transition.to_node_id.as_str(),
            transition.transition_kind.as_str(),
            workflow_transition_gate_decision_sql_value(&transition.gate_decision),
            transition.gate_decision_context.as_deref(),
            transition.occurred_at.as_str(),
        ],
    );

    match event_insert_result {
        Ok(_) => Ok(WorkflowTransitionMutationApplyOutcome::Applied),
        Err(error) if is_unique_constraint_violation(&error) => {
            let existing = read_transition_event_by_transition_id(
                transaction,
                database_path,
                project_id,
                &transition.transition_id,
            )?
            .ok_or_else(|| {
                map_sql_error(
                    error_profile.event_persist_failed_code,
                    database_path,
                    error,
                    error_profile.event_persist_failed_message,
                )
            })?;

            Ok(WorkflowTransitionMutationApplyOutcome::Replayed(existing))
        }
        Err(error) => Err(map_sql_error(
            error_profile.event_persist_failed_code,
            database_path,
            error,
            error_profile.event_persist_failed_message,
        )),
    }
}

fn attempt_automatic_dispatch_after_transition(
    connection: &mut Connection,
    database_path: &Path,
    project_id: &str,
    trigger_transition: &WorkflowTransitionEventRecord,
) -> WorkflowAutomaticDispatchOutcome {
    let transaction = match connection.unchecked_transaction() {
        Ok(transaction) => transaction,
        Err(error) => {
            return automatic_dispatch_outcome_from_error(map_workflow_graph_transaction_error(
                "workflow_transition_auto_dispatch_transaction_failed",
                database_path,
                error,
                "Cadence could not start an automatic-dispatch transaction.",
            ));
        }
    };

    let candidate = match resolve_automatic_dispatch_candidate(
        &transaction,
        database_path,
        project_id,
        &trigger_transition.to_node_id,
    ) {
        Ok(WorkflowAutomaticDispatchCandidateResolution::NoContinuation) => {
            return WorkflowAutomaticDispatchOutcome::NoContinuation;
        }
        Ok(WorkflowAutomaticDispatchCandidateResolution::Candidate(candidate)) => candidate,
        Ok(WorkflowAutomaticDispatchCandidateResolution::Unresolved {
            completed_node_id,
            blocked_candidates,
        }) => {
            let blocked_summary =
                format_unresolved_dispatch_candidate_summary(blocked_candidates.as_slice());

            let persisted = match persist_pending_approval_for_unresolved_auto_dispatch(
                &transaction,
                database_path,
                project_id,
                trigger_transition,
                blocked_candidates.as_slice(),
            ) {
                Ok(persisted) => persisted,
                Err(error) => return automatic_dispatch_outcome_from_error(error),
            };

            if let Err(error) = transaction.commit() {
                return automatic_dispatch_outcome_from_error(map_workflow_graph_commit_error(
                    "workflow_transition_auto_dispatch_commit_failed",
                    database_path,
                    error,
                    "Cadence could not commit gate-unmet automatic-dispatch state.",
                ));
            }

            let enqueue_outcome = enqueue_notification_dispatches_best_effort_with_connection(
                connection,
                database_path,
                &NotificationDispatchEnqueueRecord {
                    project_id: project_id.to_string(),
                    action_id: persisted.action_id.clone(),
                    enqueued_at: persisted.updated_at.clone(),
                },
            );

            return WorkflowAutomaticDispatchOutcome::Skipped {
                code: "workflow_transition_gate_unmet".into(),
                message: format!(
                    "Cadence skipped automatic dispatch from `{completed_node_id}` because continuation edges are still blocked by unresolved gates: {blocked_summary}. Persisted pending operator approval `{}` for deterministic replay. {}",
                    persisted.action_id,
                    format_notification_dispatch_enqueue_outcome(&enqueue_outcome)
                ),
            };
        }
        Err(error) => return automatic_dispatch_outcome_from_error(error),
    };

    let transition_id =
        derive_automatic_transition_id(&trigger_transition.transition_id, &candidate);
    let mutation = WorkflowTransitionMutationRecord {
        transition_id: transition_id.clone(),
        causal_transition_id: Some(trigger_transition.transition_id.clone()),
        from_node_id: candidate.from_node_id,
        to_node_id: candidate.to_node_id,
        transition_kind: candidate.transition_kind,
        gate_decision: WorkflowTransitionGateDecision::NotApplicable,
        gate_decision_context: None,
        gate_updates: Vec::new(),
        required_gate_requirement: candidate.gate_requirement,
        occurred_at: crate::auth::now_timestamp(),
    };

    let mutation_outcome = match apply_workflow_transition_mutation(
        &transaction,
        database_path,
        project_id,
        &mutation,
        &WORKFLOW_AUTOMATIC_DISPATCH_MUTATION_ERROR_PROFILE,
        map_workflow_graph_write_error,
    ) {
        Ok(mutation_outcome) => mutation_outcome,
        Err(error) => return automatic_dispatch_outcome_from_error(error),
    };

    match mutation_outcome {
        WorkflowTransitionMutationApplyOutcome::Replayed(transition_event) => {
            let handoff_package = load_replayed_handoff_package_for_automatic_dispatch(
                &transaction,
                database_path,
                project_id,
                &transition_event,
            );

            WorkflowAutomaticDispatchOutcome::Replayed {
                transition_event,
                handoff_package,
            }
        }
        WorkflowTransitionMutationApplyOutcome::Applied => {
            if let Err(error) = transaction.commit() {
                return automatic_dispatch_outcome_from_error(map_workflow_graph_commit_error(
                    "workflow_transition_auto_dispatch_commit_failed",
                    database_path,
                    error,
                    "Cadence could not commit automatic workflow dispatch.",
                ));
            }

            match read_transition_event_by_transition_id(
                connection,
                database_path,
                project_id,
                &transition_id,
            ) {
                Ok(Some(transition_event)) => {
                    let handoff_package = persist_handoff_package_for_automatic_dispatch(
                        connection,
                        database_path,
                        project_id,
                        &transition_event,
                    );

                    WorkflowAutomaticDispatchOutcome::Applied {
                        transition_event,
                        handoff_package,
                    }
                }
                Ok(None) => WorkflowAutomaticDispatchOutcome::Skipped {
                    code: "workflow_transition_auto_dispatch_event_missing_after_persist".into(),
                    message: format!(
                        "Cadence persisted automatic transition `{transition_id}` in {} but could not read it back.",
                        database_path.display()
                    ),
                },
                Err(error) => automatic_dispatch_outcome_from_error(error),
            }
        }
    }
}

fn persist_handoff_package_for_automatic_dispatch(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    transition_event: &WorkflowTransitionEventRecord,
) -> WorkflowAutomaticDispatchPackageOutcome {
    let package_payload = match assemble_workflow_handoff_package_upsert_record(
        connection,
        database_path,
        project_id,
        transition_event,
    ) {
        Ok(payload) => payload,
        Err(error) => return automatic_dispatch_package_outcome_from_error(error),
    };

    let persisted = match persist_workflow_handoff_package_with_connection(
        connection,
        database_path,
        &package_payload,
    ) {
        Ok(persisted) => persisted,
        Err(error) => return automatic_dispatch_package_outcome_from_error(error),
    };

    if let Err(error) =
        validate_workflow_handoff_package_transition_linkage(&persisted.package, transition_event)
    {
        return automatic_dispatch_package_outcome_from_error(error);
    }

    match persisted.disposition {
        WorkflowHandoffPackagePersistDisposition::Persisted => {
            WorkflowAutomaticDispatchPackageOutcome::Persisted {
                package: persisted.package,
            }
        }
        WorkflowHandoffPackagePersistDisposition::Replayed => {
            WorkflowAutomaticDispatchPackageOutcome::Replayed {
                package: persisted.package,
            }
        }
    }
}

fn load_replayed_handoff_package_for_automatic_dispatch(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    transition_event: &WorkflowTransitionEventRecord,
) -> WorkflowAutomaticDispatchPackageOutcome {
    let package = match read_workflow_handoff_package_by_transition_id(
        connection,
        database_path,
        project_id,
        &transition_event.transition_id,
    ) {
        Ok(Some(package)) => package,
        Ok(None) => {
            return WorkflowAutomaticDispatchPackageOutcome::Skipped {
                code: "workflow_handoff_replay_not_found".into(),
                message: format!(
                    "Cadence replayed automatic transition `{}` in {} but no workflow handoff package row exists for that transition.",
                    transition_event.transition_id,
                    database_path.display()
                ),
            };
        }
        Err(error) => return automatic_dispatch_package_outcome_from_error(error),
    };

    if let Err(error) =
        validate_workflow_handoff_package_transition_linkage(&package, transition_event)
    {
        return automatic_dispatch_package_outcome_from_error(error);
    }

    WorkflowAutomaticDispatchPackageOutcome::Replayed { package }
}

fn validate_workflow_handoff_package_transition_linkage(
    package: &WorkflowHandoffPackageRecord,
    transition_event: &WorkflowTransitionEventRecord,
) -> Result<(), CommandError> {
    if package.handoff_transition_id != transition_event.transition_id {
        return Err(CommandError::system_fault(
            "workflow_handoff_linkage_mismatch",
            format!(
                "Cadence loaded workflow handoff package `{}` for transition `{}` but transition linkage did not match.",
                package.handoff_transition_id, transition_event.transition_id
            ),
        ));
    }

    if package.from_node_id != transition_event.from_node_id
        || package.to_node_id != transition_event.to_node_id
        || package.transition_kind != transition_event.transition_kind
        || package.causal_transition_id != transition_event.causal_transition_id
    {
        return Err(CommandError::system_fault(
            "workflow_handoff_linkage_mismatch",
            format!(
                "Cadence found inconsistent workflow handoff linkage for transition `{}` (expected {} -> {} [{}], found {} -> {} [{}]).",
                transition_event.transition_id,
                transition_event.from_node_id,
                transition_event.to_node_id,
                transition_event.transition_kind,
                package.from_node_id,
                package.to_node_id,
                package.transition_kind,
            ),
        ));
    }

    Ok(())
}

fn automatic_dispatch_package_outcome_from_error(
    error: CommandError,
) -> WorkflowAutomaticDispatchPackageOutcome {
    WorkflowAutomaticDispatchPackageOutcome::Skipped {
        code: error.code,
        message: error.message,
    }
}

fn format_unresolved_dispatch_candidate_summary(
    blocked_candidates: &[WorkflowAutomaticDispatchUnresolvedContinuationCandidate],
) -> String {
    blocked_candidates
        .iter()
        .map(|candidate| {
            let gate_summary = candidate
                .unresolved_gates
                .iter()
                .map(|gate| {
                    format!(
                        "{}:{}:{}",
                        gate.gate_node_id,
                        gate.gate_key,
                        workflow_gate_state_sql_value(&gate.gate_state)
                    )
                })
                .collect::<Vec<_>>()
                .join("|");

            let gate_requirement_suffix = candidate
                .gate_requirement
                .as_deref()
                .map(|required_gate| format!(" gate={required_gate}"))
                .unwrap_or_default();

            format!(
                "{}->{}:{}{} [{}]",
                candidate.from_node_id,
                candidate.to_node_id,
                candidate.transition_kind,
                gate_requirement_suffix,
                gate_summary,
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn derive_auto_dispatch_operator_scope(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    trigger_transition: &WorkflowTransitionEventRecord,
) -> Result<(String, Option<String>), CommandError> {
    let runtime_session = read_runtime_session_row(transaction, database_path, project_id)?;

    let runtime_flow_id = runtime_session
        .as_ref()
        .and_then(|session| session.flow_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let runtime_session_id = runtime_session
        .as_ref()
        .and_then(|session| session.session_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let (session_id, flow_id) = if runtime_flow_id.is_some() || runtime_session_id.is_some() {
        (
            runtime_session_id.unwrap_or_else(|| format!("workflow-auto-dispatch:{project_id}")),
            runtime_flow_id,
        )
    } else {
        (
            format!("workflow-auto-dispatch:{project_id}"),
            Some(format!(
                "workflow-auto-dispatch:{project_id}:{}",
                trigger_transition.transition_id
            )),
        )
    };

    validate_non_empty_text(&session_id, "session_id", "runtime_action_request_invalid")?;
    if let Some(flow_id) = flow_id.as_deref() {
        validate_non_empty_text(flow_id, "flow_id", "runtime_action_request_invalid")?;
    }

    Ok((session_id, flow_id))
}

fn upsert_pending_operator_approval_row(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    session_id: &str,
    flow_id: Option<&str>,
    action_type: &str,
    title: &str,
    detail: &str,
    created_at: &str,
    gate_link: Option<&OperatorApprovalGateLink>,
) -> Result<String, CommandError> {
    let normalized_session_id = session_id.trim();
    let normalized_flow_id = flow_id.map(str::trim).filter(|value| !value.is_empty());

    if normalized_session_id.is_empty() && normalized_flow_id.is_none() {
        return Err(CommandError::system_fault(
            "runtime_action_request_invalid",
            "Cadence could not persist gate-unmet auto-dispatch approval because both session and flow scopes were empty.",
        ));
    }

    validate_non_empty_text(action_type, "action_type", "runtime_action_request_invalid")?;
    validate_non_empty_text(title, "title", "runtime_action_request_invalid")?;
    validate_non_empty_text(detail, "detail", "runtime_action_request_invalid")?;
    validate_non_empty_text(created_at, "created_at", "runtime_action_request_invalid")?;

    let action_id = derive_operator_action_id(
        normalized_session_id,
        normalized_flow_id,
        action_type,
        gate_link,
    )?;

    let existing =
        read_operator_approval_by_action_id(transaction, database_path, project_id, &action_id)?;
    match existing {
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
                        ?8,
                        ?9,
                        ?10,
                        ?11,
                        ?12,
                        NULL,
                        'pending',
                        NULL,
                        ?13,
                        ?13,
                        NULL
                    )
                    "#,
                    params![
                        project_id,
                        action_id,
                        if normalized_session_id.is_empty() {
                            None
                        } else {
                            Some(normalized_session_id)
                        },
                        normalized_flow_id,
                        action_type,
                        title,
                        detail,
                        gate_link.as_ref().map(|link| link.gate_node_id.as_str()),
                        gate_link.as_ref().map(|link| link.gate_key.as_str()),
                        gate_link
                            .as_ref()
                            .map(|link| link.transition_from_node_id.as_str()),
                        gate_link
                            .as_ref()
                            .map(|link| link.transition_to_node_id.as_str()),
                        gate_link.as_ref().map(|link| link.transition_kind.as_str()),
                        created_at,
                    ],
                )
                .map_err(|error| {
                    map_operator_loop_write_error(
                        "operator_approval_upsert_failed",
                        database_path,
                        error,
                        "Cadence could not persist the pending operator approval.",
                    )
                })?;
        }
        Some(approval) => match approval.status {
            OperatorApprovalStatus::Pending => {
                transaction
                    .execute(
                        r#"
                        UPDATE operator_approvals
                        SET session_id = ?3,
                            flow_id = ?4,
                            title = ?5,
                            detail = ?6,
                            gate_node_id = ?7,
                            gate_key = ?8,
                            transition_from_node_id = ?9,
                            transition_to_node_id = ?10,
                            transition_kind = ?11,
                            updated_at = ?12
                        WHERE project_id = ?1
                          AND action_id = ?2
                          AND status = 'pending'
                        "#,
                        params![
                            project_id,
                            action_id,
                            if normalized_session_id.is_empty() {
                                None
                            } else {
                                Some(normalized_session_id)
                            },
                            normalized_flow_id,
                            title,
                            detail,
                            gate_link.as_ref().map(|link| link.gate_node_id.as_str()),
                            gate_link.as_ref().map(|link| link.gate_key.as_str()),
                            gate_link
                                .as_ref()
                                .map(|link| link.transition_from_node_id.as_str()),
                            gate_link
                                .as_ref()
                                .map(|link| link.transition_to_node_id.as_str()),
                            gate_link.as_ref().map(|link| link.transition_kind.as_str()),
                            created_at,
                        ],
                    )
                    .map_err(|error| {
                        map_operator_loop_write_error(
                            "operator_approval_upsert_failed",
                            database_path,
                            error,
                            "Cadence could not refresh the pending operator approval.",
                        )
                    })?;
            }
            OperatorApprovalStatus::Approved | OperatorApprovalStatus::Rejected => {
                return Err(CommandError::retryable(
                    "runtime_action_sync_conflict",
                    format!(
                        "Cadence received a retained runtime action for already-resolved operator request `{action_id}`. Reopen or refresh the selected project before retrying."
                    ),
                ));
            }
        },
    }

    Ok(action_id)
}

fn persist_pending_approval_for_unresolved_auto_dispatch(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    trigger_transition: &WorkflowTransitionEventRecord,
    blocked_candidates: &[WorkflowAutomaticDispatchUnresolvedContinuationCandidate],
) -> Result<OperatorApprovalDto, CommandError> {
    let candidate = match blocked_candidates {
        [candidate] => candidate,
        [] => {
            return Err(CommandError::user_fixable(
                "workflow_transition_gate_unmet",
                "Cadence skipped automatic dispatch because no unresolved continuation candidates were available for persistence.",
            ))
        }
        candidates => {
            let blocked_summary = format_unresolved_dispatch_candidate_summary(candidates);
            return Err(CommandError::user_fixable(
                "workflow_transition_gate_unmet",
                format!(
                    "Cadence skipped automatic dispatch because unresolved continuation metadata was ambiguous ({blocked_summary})."
                ),
            ));
        }
    };

    let filtered_gates: Vec<&WorkflowAutomaticDispatchUnresolvedGateCandidate> =
        match candidate.gate_requirement.as_deref() {
            Some(required_gate) => candidate
                .unresolved_gates
                .iter()
                .filter(|gate| gate.gate_key == required_gate)
                .collect(),
            None => candidate.unresolved_gates.iter().collect(),
        };

    let gate = match filtered_gates.as_slice() {
        [gate] => *gate,
        [] => {
            return Err(CommandError::user_fixable(
                "workflow_transition_gate_unmet",
                format!(
                    "Cadence skipped automatic dispatch for `{}` -> `{}` ({}) because required gate linkage could not be resolved from unresolved metadata.",
                    candidate.from_node_id, candidate.to_node_id, candidate.transition_kind
                ),
            ));
        }
        _ => {
            return Err(CommandError::user_fixable(
                "workflow_transition_gate_unmet",
                format!(
                    "Cadence skipped automatic dispatch for `{}` -> `{}` ({}) because unresolved gate metadata was ambiguous for deterministic approval persistence.",
                    candidate.from_node_id, candidate.to_node_id, candidate.transition_kind
                ),
            ));
        }
    };

    if gate.gate_node_id != candidate.to_node_id {
        return Err(CommandError::system_fault(
            "runtime_action_request_invalid",
            format!(
                "Cadence could not persist gate-unmet auto-dispatch approval because gate node `{}` did not match continuation target `{}`.",
                gate.gate_node_id, candidate.to_node_id
            ),
        ));
    }

    if let Some(required_gate) = candidate.gate_requirement.as_deref() {
        if gate.gate_key != required_gate {
            return Err(CommandError::system_fault(
                "runtime_action_request_invalid",
                format!(
                    "Cadence could not persist gate-unmet auto-dispatch approval because gate `{}` did not match required transition gate `{required_gate}`.",
                    gate.gate_key
                ),
            ));
        }
    }

    let action_type = gate
        .action_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_transition_gate_unmet",
                format!(
                    "Cadence skipped automatic dispatch for `{}` -> `{}` ({}) because unresolved gate `{}` is non-actionable (missing `action_type`).",
                    candidate.from_node_id,
                    candidate.to_node_id,
                    candidate.transition_kind,
                    gate.gate_key,
                ),
            )
        })?;
    let title = gate
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_transition_gate_unmet",
                format!(
                    "Cadence skipped automatic dispatch for `{}` -> `{}` ({}) because unresolved gate `{}` is non-actionable (missing `title`).",
                    candidate.from_node_id,
                    candidate.to_node_id,
                    candidate.transition_kind,
                    gate.gate_key,
                ),
            )
        })?;
    let detail = gate
        .detail
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_transition_gate_unmet",
                format!(
                    "Cadence skipped automatic dispatch for `{}` -> `{}` ({}) because unresolved gate `{}` is non-actionable (missing `detail`).",
                    candidate.from_node_id,
                    candidate.to_node_id,
                    candidate.transition_kind,
                    gate.gate_key,
                ),
            )
        })?;

    let gate_link = OperatorApprovalGateLink {
        gate_node_id: gate.gate_node_id.clone(),
        gate_key: gate.gate_key.clone(),
        transition_from_node_id: candidate.from_node_id.clone(),
        transition_to_node_id: candidate.to_node_id.clone(),
        transition_kind: candidate.transition_kind.clone(),
    };

    let (session_id, flow_id) = derive_auto_dispatch_operator_scope(
        transaction,
        database_path,
        project_id,
        trigger_transition,
    )?;

    let created_at = crate::auth::now_timestamp();
    let action_id = upsert_pending_operator_approval_row(
        transaction,
        database_path,
        project_id,
        &session_id,
        flow_id.as_deref(),
        action_type,
        title,
        detail,
        &created_at,
        Some(&gate_link),
    )?;

    read_operator_approval_by_action_id(transaction, database_path, project_id, &action_id)?
        .ok_or_else(|| {
            CommandError::system_fault(
                "operator_approval_missing_after_persist",
                format!(
                    "Cadence persisted gate-unmet auto-dispatch approval `{action_id}` in {} but could not read it back.",
                    database_path.display()
                ),
            )
        })
}

fn resolve_automatic_dispatch_candidate(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    completed_node_id: &str,
) -> Result<WorkflowAutomaticDispatchCandidateResolution, CommandError> {
    let nodes = read_workflow_graph_nodes(transaction, database_path, project_id)?;
    if !nodes.iter().any(|node| node.node_id == completed_node_id) {
        return Err(CommandError::user_fixable(
            "workflow_transition_auto_dispatch_source_missing",
            format!(
                "Cadence cannot resolve automatic continuation from `{completed_node_id}` because the workflow node is missing."
            ),
        ));
    }

    let mut outgoing_edges: Vec<WorkflowGraphEdgeRecord> =
        read_workflow_graph_edges(transaction, database_path, project_id)?
            .into_iter()
            .filter(|edge| edge.from_node_id == completed_node_id)
            .collect();

    outgoing_edges.sort_by(|left, right| {
        left.to_node_id
            .cmp(&right.to_node_id)
            .then_with(|| left.transition_kind.cmp(&right.transition_kind))
            .then_with(|| left.gate_requirement.cmp(&right.gate_requirement))
    });

    if outgoing_edges.is_empty() {
        return Ok(WorkflowAutomaticDispatchCandidateResolution::NoContinuation);
    }

    let gates = read_workflow_gate_metadata(transaction, database_path, project_id)?;
    let mut gates_by_node: HashMap<String, Vec<WorkflowGateMetadataRecord>> = HashMap::new();
    for gate in gates {
        gates_by_node
            .entry(gate.node_id.clone())
            .or_default()
            .push(gate);
    }

    let node_ids = nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect::<std::collections::HashSet<_>>();

    let mut legal_candidates = Vec::new();
    let mut blocked_candidates = Vec::new();

    for edge in outgoing_edges {
        if !node_ids.contains(edge.to_node_id.as_str()) {
            return Err(CommandError::user_fixable(
                "workflow_transition_illegal_edge",
                format!(
                    "Cadence cannot automatically dispatch `{}` -> `{}` ({}) because target node `{}` does not exist.",
                    edge.from_node_id,
                    edge.to_node_id,
                    edge.transition_kind,
                    edge.to_node_id
                ),
            ));
        }

        let target_gates = gates_by_node
            .get(edge.to_node_id.as_str())
            .cloned()
            .unwrap_or_default();

        if let Some(required_gate) = edge.gate_requirement.as_deref() {
            let required_gate_present = target_gates
                .iter()
                .any(|gate| gate.gate_key == required_gate);

            if !required_gate_present {
                return Err(CommandError::system_fault(
                    "workflow_transition_auto_dispatch_gate_mapping_invalid",
                    format!(
                        "Cadence found invalid automatic-dispatch gate mapping for `{}` -> `{}` ({}) because required gate `{required_gate}` is missing on target node `{}`.",
                        edge.from_node_id,
                        edge.to_node_id,
                        edge.transition_kind,
                        edge.to_node_id,
                    ),
                ));
            }
        }

        let unresolved_gates: Vec<WorkflowAutomaticDispatchUnresolvedGateCandidate> = target_gates
            .iter()
            .filter(|gate| {
                matches!(
                    gate.gate_state,
                    WorkflowGateState::Pending | WorkflowGateState::Blocked
                )
            })
            .map(|gate| WorkflowAutomaticDispatchUnresolvedGateCandidate {
                gate_node_id: gate.node_id.clone(),
                gate_key: gate.gate_key.clone(),
                gate_state: gate.gate_state.clone(),
                action_type: gate.action_type.clone(),
                title: gate.title.clone(),
                detail: gate.detail.clone(),
            })
            .collect();

        if unresolved_gates.is_empty() {
            legal_candidates.push(WorkflowAutomaticDispatchCandidate {
                from_node_id: edge.from_node_id,
                to_node_id: edge.to_node_id,
                transition_kind: edge.transition_kind,
                gate_requirement: edge.gate_requirement,
            });
        } else {
            blocked_candidates.push(WorkflowAutomaticDispatchUnresolvedContinuationCandidate {
                from_node_id: edge.from_node_id,
                to_node_id: edge.to_node_id,
                transition_kind: edge.transition_kind,
                gate_requirement: edge.gate_requirement,
                unresolved_gates,
            });
        }
    }

    match legal_candidates.as_slice() {
        [] if blocked_candidates.is_empty() => {
            Ok(WorkflowAutomaticDispatchCandidateResolution::NoContinuation)
        }
        [] => Ok(WorkflowAutomaticDispatchCandidateResolution::Unresolved {
            completed_node_id: completed_node_id.to_string(),
            blocked_candidates,
        }),
        [single] => Ok(WorkflowAutomaticDispatchCandidateResolution::Candidate(
            single.clone(),
        )),
        candidates => {
            let options = candidates
                .iter()
                .map(|candidate| {
                    format!(
                        "{}->{}:{}",
                        candidate.from_node_id, candidate.to_node_id, candidate.transition_kind
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            Err(CommandError::user_fixable(
                "workflow_transition_ambiguous_next_step",
                format!(
                    "Cadence cannot auto-dispatch from `{completed_node_id}` because multiple legal continuation edges exist ({options})."
                ),
            ))
        }
    }
}

fn automatic_dispatch_outcome_from_error(error: CommandError) -> WorkflowAutomaticDispatchOutcome {
    WorkflowAutomaticDispatchOutcome::Skipped {
        code: error.code,
        message: error.message,
    }
}

fn assert_transition_edge_exists(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    from_node_id: &str,
    to_node_id: &str,
    transition_kind: &str,
    required_gate_requirement: Option<&str>,
    error_profile: &WorkflowTransitionMutationErrorProfile,
    map_sql_error: WorkflowTransitionSqlErrorMapper,
) -> Result<(), CommandError> {
    let edge_exists: i64 = transaction
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM workflow_graph_edges
            WHERE project_id = ?1
              AND from_node_id = ?2
              AND to_node_id = ?3
              AND transition_kind = ?4
              AND (?5 IS NULL OR gate_requirement = ?5)
            "#,
            params![
                project_id,
                from_node_id,
                to_node_id,
                transition_kind,
                required_gate_requirement,
            ],
            |row| row.get(0),
        )
        .map_err(|error| {
            map_sql_error(
                error_profile.edge_check_failed_code,
                database_path,
                error,
                error_profile.edge_check_failed_message,
            )
        })?;

    if edge_exists == 0 {
        if let Some(gate_requirement) = required_gate_requirement {
            return Err(CommandError::user_fixable(
                "workflow_transition_illegal_edge",
                format!(
                    "Cadence cannot transition from `{from_node_id}` to `{to_node_id}` with kind `{transition_kind}` and gate `{gate_requirement}` because no legal workflow edge exists."
                ),
            ));
        }

        return Err(CommandError::user_fixable(
            "workflow_transition_illegal_edge",
            format!(
                "Cadence cannot transition from `{from_node_id}` to `{to_node_id}` with kind `{transition_kind}` because no legal workflow edge exists."
            ),
        ));
    }

    Ok(())
}

fn validate_non_empty_text(value: &str, field: &str, code: &str) -> Result<(), CommandError> {
    if value.trim().is_empty() {
        return Err(CommandError::user_fixable(
            code,
            format!("Field `{field}` must be a non-empty string."),
        ));
    }

    Ok(())
}

fn parse_workflow_gate_state(value: &str) -> Result<WorkflowGateState, String> {
    match value {
        "pending" => Ok(WorkflowGateState::Pending),
        "satisfied" => Ok(WorkflowGateState::Satisfied),
        "blocked" => Ok(WorkflowGateState::Blocked),
        "skipped" => Ok(WorkflowGateState::Skipped),
        other => Err(format!(
            "Field `gate_state` must be a known workflow gate state, found `{other}`."
        )),
    }
}

fn workflow_gate_state_sql_value(value: &WorkflowGateState) -> &'static str {
    match value {
        WorkflowGateState::Pending => "pending",
        WorkflowGateState::Satisfied => "satisfied",
        WorkflowGateState::Blocked => "blocked",
        WorkflowGateState::Skipped => "skipped",
    }
}

fn parse_workflow_transition_gate_decision(
    value: &str,
) -> Result<WorkflowTransitionGateDecision, String> {
    match value {
        "approved" => Ok(WorkflowTransitionGateDecision::Approved),
        "rejected" => Ok(WorkflowTransitionGateDecision::Rejected),
        "blocked" => Ok(WorkflowTransitionGateDecision::Blocked),
        "not_applicable" => Ok(WorkflowTransitionGateDecision::NotApplicable),
        other => Err(format!(
            "Field `gate_decision` must be a known transition gate decision, found `{other}`."
        )),
    }
}

fn workflow_transition_gate_decision_sql_value(
    value: &WorkflowTransitionGateDecision,
) -> &'static str {
    match value {
        WorkflowTransitionGateDecision::Approved => "approved",
        WorkflowTransitionGateDecision::Rejected => "rejected",
        WorkflowTransitionGateDecision::Blocked => "blocked",
        WorkflowTransitionGateDecision::NotApplicable => "not_applicable",
    }
}

fn phase_status_sql_value(value: &PhaseStatus) -> &'static str {
    match value {
        PhaseStatus::Complete => "complete",
        PhaseStatus::Active => "active",
        PhaseStatus::Pending => "pending",
        PhaseStatus::Blocked => "blocked",
    }
}

fn phase_step_sql_value(value: &PhaseStep) -> &'static str {
    match value {
        PhaseStep::Discuss => "discuss",
        PhaseStep::Plan => "plan",
        PhaseStep::Execute => "execute",
        PhaseStep::Verify => "verify",
        PhaseStep::Ship => "ship",
    }
}

fn map_workflow_graph_transaction_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_workflow_graph_write_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_workflow_graph_commit_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_workflow_handoff_transaction_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_workflow_handoff_write_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_workflow_handoff_commit_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_workflow_handoff_insert_error(
    database_path: &Path,
    error: SqlError,
    project_id: &str,
    handoff_transition_id: &str,
) -> CommandError {
    if let SqlError::SqliteFailure(inner, message) = &error {
        if inner.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY {
            return CommandError::user_fixable(
                "workflow_handoff_linkage_missing",
                format!(
                    "Cadence cannot persist workflow handoff package `{handoff_transition_id}` for project `{project_id}` because the linked workflow transition or node rows are missing in {}.",
                    database_path.display()
                ),
            );
        }

        if inner.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_CHECK {
            return CommandError::user_fixable(
                "workflow_handoff_request_invalid",
                format!(
                    "Workflow handoff package `{handoff_transition_id}` violated table validation rules in {}: {}.",
                    database_path.display(),
                    message
                        .as_deref()
                        .unwrap_or("SQLite CHECK constraint failed")
                ),
            );
        }
    }

    map_workflow_handoff_write_error(
        "workflow_handoff_persist_failed",
        database_path,
        error,
        "Cadence could not persist the workflow handoff-package row.",
    )
}

fn parse_operator_approval_status(value: &str) -> Result<OperatorApprovalStatus, String> {
    match value {
        "pending" => Ok(OperatorApprovalStatus::Pending),
        "approved" => Ok(OperatorApprovalStatus::Approved),
        "rejected" => Ok(OperatorApprovalStatus::Rejected),
        other => Err(format!(
            "Field `status` must be a known approval status, found `{other}`."
        )),
    }
}

fn parse_verification_record_status(value: &str) -> Result<VerificationRecordStatus, String> {
    match value {
        "pending" => Ok(VerificationRecordStatus::Pending),
        "passed" => Ok(VerificationRecordStatus::Passed),
        "failed" => Ok(VerificationRecordStatus::Failed),
        other => Err(format!(
            "Field `status` must be a known verification status, found `{other}`."
        )),
    }
}

fn normalize_runtime_resume_history_summary(summary: &str, fallback: &str) -> String {
    let candidate = if summary.trim().is_empty() {
        fallback.trim()
    } else {
        summary.trim()
    };

    if find_prohibited_runtime_persistence_content(candidate).is_some() {
        return normalize_runtime_checkpoint_summary(fallback);
    }

    normalize_runtime_checkpoint_summary(candidate)
}

fn resume_history_status_sql_value(value: &ResumeHistoryStatus) -> &'static str {
    match value {
        ResumeHistoryStatus::Started => "started",
        ResumeHistoryStatus::Failed => "failed",
    }
}

fn parse_resume_history_status(value: &str) -> Result<ResumeHistoryStatus, String> {
    match value {
        "started" => Ok(ResumeHistoryStatus::Started),
        "failed" => Ok(ResumeHistoryStatus::Failed),
        other => Err(format!(
            "Field `status` must be a known resume-history status, found `{other}`."
        )),
    }
}

fn validate_operator_approval_gate_link_input(
    gate_link: &OperatorApprovalGateLinkInput,
    action_type: &str,
) -> Result<OperatorApprovalGateLink, CommandError> {
    let gate_node_id = normalize_operator_gate_link_field(
        gate_link.gate_node_id.as_str(),
        "gateNodeId",
        action_type,
    )?;
    let gate_key =
        normalize_operator_gate_link_field(gate_link.gate_key.as_str(), "gateKey", action_type)?;
    let transition_from_node_id = normalize_operator_gate_link_field(
        gate_link.transition_from_node_id.as_str(),
        "transitionFromNodeId",
        action_type,
    )?;
    let transition_to_node_id = normalize_operator_gate_link_field(
        gate_link.transition_to_node_id.as_str(),
        "transitionToNodeId",
        action_type,
    )?;
    let transition_kind = normalize_operator_gate_link_field(
        gate_link.transition_kind.as_str(),
        "transitionKind",
        action_type,
    )?;

    if gate_node_id != transition_to_node_id {
        return Err(CommandError::system_fault(
            "runtime_action_request_invalid",
            format!(
                "Cadence could not persist gate-linked runtime action `{action_type}` because gate node `{gate_node_id}` does not match transition target `{transition_to_node_id}`."
            ),
        ));
    }

    Ok(OperatorApprovalGateLink {
        gate_node_id,
        gate_key,
        transition_from_node_id,
        transition_to_node_id,
        transition_kind,
    })
}

fn normalize_operator_gate_link_field(
    value: &str,
    field: &str,
    action_type: &str,
) -> Result<String, CommandError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CommandError::system_fault(
            "runtime_action_request_invalid",
            format!(
                "Cadence could not persist gate-linked runtime action `{action_type}` because `{field}` was empty."
            ),
        ));
    }

    Ok(value.to_string())
}

fn resolve_operator_approval_gate_link(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    action_type: &str,
    title: &str,
    detail: &str,
) -> Result<Option<OperatorApprovalGateLink>, CommandError> {
    let mut statement = transaction
        .prepare(
            r#"
            SELECT
                node_id,
                gate_key,
                title,
                detail
            FROM workflow_gate_metadata
            WHERE project_id = ?1
              AND gate_state IN ('pending', 'blocked')
              AND action_type = ?2
            ORDER BY node_id ASC, gate_key ASC
            "#,
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "operator_approval_gate_lookup_failed",
                database_path,
                error,
                "Cadence could not load unresolved workflow gates for operator approval persistence.",
            )
        })?;

    let gate_candidates = statement
        .query_map(params![project_id, action_type], |row| {
            Ok(OperatorApprovalGateCandidate {
                node_id: row.get(0)?,
                gate_key: row.get(1)?,
                title: row.get(2)?,
                detail: row.get(3)?,
            })
        })
        .map_err(|error| {
            map_operator_loop_write_error(
                "operator_approval_gate_lookup_failed",
                database_path,
                error,
                "Cadence could not query unresolved workflow gates for operator approval persistence.",
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            CommandError::system_fault(
                "operator_approval_gate_decode_failed",
                format!(
                    "Cadence could not decode unresolved workflow gate rows from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    if gate_candidates.is_empty() {
        return Ok(None);
    }

    let mut filtered_candidates: Vec<OperatorApprovalGateCandidate> = gate_candidates
        .iter()
        .filter(|candidate| candidate.title == title && candidate.detail == detail)
        .cloned()
        .collect();

    if filtered_candidates.is_empty() {
        filtered_candidates = gate_candidates;
    }

    if filtered_candidates.len() > 1 {
        let mut active_node_statement = transaction
            .prepare(
                r#"
                SELECT node_id
                FROM workflow_graph_nodes
                WHERE project_id = ?1
                  AND status = 'active'
                ORDER BY sort_order ASC, node_id ASC
                LIMIT 1
                "#,
            )
            .map_err(|error| {
                map_operator_loop_write_error(
                    "operator_approval_gate_lookup_failed",
                    database_path,
                    error,
                    "Cadence could not load active workflow-node context for gate-link disambiguation.",
                )
            })?;

        let active_node_id: Option<String> = active_node_statement
            .query_row(params![project_id], |row| row.get(0))
            .optional()
            .map_err(|error| {
                map_operator_loop_write_error(
                    "operator_approval_gate_lookup_failed",
                    database_path,
                    error,
                    "Cadence could not query active workflow-node context for gate-link disambiguation.",
                )
            })?;

        if let Some(active_node_id) = active_node_id {
            let active_candidates: Vec<OperatorApprovalGateCandidate> = filtered_candidates
                .iter()
                .filter(|candidate| candidate.node_id == active_node_id)
                .cloned()
                .collect();

            if !active_candidates.is_empty() {
                filtered_candidates = active_candidates;
            }
        }
    }

    if filtered_candidates.len() != 1 {
        let candidates = filtered_candidates
            .iter()
            .map(|candidate| format!("{}:{}", candidate.node_id, candidate.gate_key))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(CommandError::user_fixable(
            "operator_approval_gate_ambiguous",
            format!(
                "Cadence cannot persist action-required item `{action_type}` because it matches multiple unresolved workflow gates ({candidates})."
            ),
        ));
    }

    let selected = &filtered_candidates[0];

    let mut edge_statement = transaction
        .prepare(
            r#"
            SELECT
                from_node_id,
                to_node_id,
                transition_kind
            FROM workflow_graph_edges
            WHERE project_id = ?1
              AND to_node_id = ?2
              AND gate_requirement = ?3
            ORDER BY from_node_id ASC, to_node_id ASC, transition_kind ASC
            "#,
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "operator_approval_transition_lookup_failed",
                database_path,
                error,
                "Cadence could not load workflow continuation edges for gate-linked operator approval.",
            )
        })?;

    let transitions = edge_statement
        .query_map(
            params![project_id, selected.node_id.as_str(), selected.gate_key.as_str()],
            |row| {
                Ok(OperatorApprovalGateLink {
                    gate_node_id: selected.node_id.clone(),
                    gate_key: selected.gate_key.clone(),
                    transition_from_node_id: row.get(0)?,
                    transition_to_node_id: row.get(1)?,
                    transition_kind: row.get(2)?,
                })
            },
        )
        .map_err(|error| {
            map_operator_loop_write_error(
                "operator_approval_transition_lookup_failed",
                database_path,
                error,
                "Cadence could not query workflow continuation edges for gate-linked operator approval.",
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            CommandError::system_fault(
                "operator_approval_transition_decode_failed",
                format!(
                    "Cadence could not decode workflow continuation edges from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    match transitions.as_slice() {
        [] => Err(CommandError::user_fixable(
            "operator_approval_transition_missing",
            format!(
                "Cadence cannot persist gate-linked operator request `{action_type}` because gate `{}` on node `{}` has no legal continuation edge.",
                selected.gate_key, selected.node_id
            ),
        )),
        [transition] => Ok(Some(transition.clone())),
        _ => {
            let candidates = transitions
                .iter()
                .map(|transition| {
                    format!(
                        "{}->{}:{}",
                        transition.transition_from_node_id,
                        transition.transition_to_node_id,
                        transition.transition_kind
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            Err(CommandError::user_fixable(
                "operator_approval_transition_ambiguous",
                format!(
                    "Cadence cannot persist gate-linked operator request `{action_type}` because gate `{}` on node `{}` maps to multiple continuation edges ({candidates}).",
                    selected.gate_key, selected.node_id
                ),
            ))
        }
    }
}

fn decode_operator_resume_transition_context(
    approval_request: &OperatorApprovalDto,
    action_id: &str,
) -> Result<Option<OperatorResumeTransitionContext>, CommandError> {
    let gate_fields_populated =
        approval_request.gate_node_id.is_some() || approval_request.gate_key.is_some();
    let transition_fields_populated = approval_request.transition_from_node_id.is_some()
        || approval_request.transition_to_node_id.is_some()
        || approval_request.transition_kind.is_some();

    if !gate_fields_populated && !transition_fields_populated {
        return Ok(None);
    }

    let gate_node_id = approval_request
        .gate_node_id
        .as_deref()
        .ok_or_else(|| {
            CommandError::retryable(
                "operator_resume_gate_link_missing",
                format!(
                    "Cadence cannot resume gate-linked operator request `{action_id}` because `gateNodeId` is missing."
                ),
            )
        })?
        .to_string();
    let gate_key = approval_request
        .gate_key
        .as_deref()
        .ok_or_else(|| {
            CommandError::retryable(
                "operator_resume_gate_link_missing",
                format!(
                    "Cadence cannot resume gate-linked operator request `{action_id}` because `gateKey` is missing."
                ),
            )
        })?
        .to_string();
    let transition_from_node_id = approval_request
        .transition_from_node_id
        .as_deref()
        .ok_or_else(|| {
            CommandError::retryable(
                "operator_resume_transition_context_missing",
                format!(
                    "Cadence cannot resume gate-linked operator request `{action_id}` because `transitionFromNodeId` is missing."
                ),
            )
        })?
        .to_string();
    let transition_to_node_id = approval_request
        .transition_to_node_id
        .as_deref()
        .ok_or_else(|| {
            CommandError::retryable(
                "operator_resume_transition_context_missing",
                format!(
                    "Cadence cannot resume gate-linked operator request `{action_id}` because `transitionToNodeId` is missing."
                ),
            )
        })?
        .to_string();
    let transition_kind = approval_request
        .transition_kind
        .as_deref()
        .ok_or_else(|| {
            CommandError::retryable(
                "operator_resume_transition_context_missing",
                format!(
                    "Cadence cannot resume gate-linked operator request `{action_id}` because `transitionKind` is missing."
                ),
            )
        })?
        .to_string();

    if gate_node_id != transition_to_node_id {
        return Err(CommandError::retryable(
            "operator_resume_transition_context_invalid",
            format!(
                "Cadence cannot resume gate-linked operator request `{action_id}` because gate node `{gate_node_id}` does not match transition target `{transition_to_node_id}`."
            ),
        ));
    }

    let user_answer = approval_request.user_answer.as_deref().ok_or_else(|| {
        CommandError::user_fixable(
            "operator_resume_answer_missing",
            format!(
                "Cadence cannot resume gate-linked operator request `{action_id}` because no user answer was recorded with the approval."
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

    Ok(Some(OperatorResumeTransitionContext {
        gate_node_id,
        gate_key,
        transition_from_node_id,
        transition_to_node_id,
        transition_kind,
        user_answer: user_answer.to_string(),
    }))
}

fn decode_runtime_operator_resume_target(
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

fn read_latest_transition_id(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
) -> Result<Option<String>, CommandError> {
    transaction
        .query_row(
            r#"
            SELECT transition_id
            FROM workflow_transition_events
            WHERE project_id = ?1
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            "#,
            params![project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            map_operator_loop_write_error(
                "operator_resume_transition_lookup_failed",
                database_path,
                error,
                "Cadence could not load prior workflow transition context for resume causality.",
            )
        })
}

fn derive_operator_scope_prefix(
    session_id: &str,
    flow_id: Option<&str>,
) -> Result<String, CommandError> {
    flow_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("flow:{value}"))
        .or_else(|| {
            let session_id = session_id.trim();
            (!session_id.is_empty()).then(|| format!("session:{session_id}"))
        })
        .ok_or_else(|| {
            CommandError::system_fault(
                "runtime_action_request_invalid",
                "Cadence could not persist the runtime approval because the action-required item was missing both flow and session identifiers.",
            )
        })
}

fn derive_operator_action_id(
    session_id: &str,
    flow_id: Option<&str>,
    action_type: &str,
    gate_link: Option<&OperatorApprovalGateLink>,
) -> Result<String, CommandError> {
    let action_type = action_type.trim();
    if action_type.is_empty() {
        return Err(CommandError::system_fault(
            "runtime_action_request_invalid",
            "Cadence could not persist the runtime approval because the action-required item was missing a stable action type.",
        ));
    }

    let stable_scope = derive_operator_scope_prefix(session_id, flow_id)?;

    if let Some(gate_link) = gate_link {
        return Ok(format!(
            "{stable_scope}:gate:{}:{}:{action_type}",
            gate_link.gate_node_id, gate_link.gate_key
        ));
    }

    Ok(format!("{stable_scope}:{action_type}"))
}

fn derive_runtime_action_id(
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

fn operator_approval_status_label(status: &OperatorApprovalStatus) -> &'static str {
    match status {
        OperatorApprovalStatus::Pending => "pending",
        OperatorApprovalStatus::Approved => "approved",
        OperatorApprovalStatus::Rejected => "rejected",
    }
}

fn map_operator_loop_transaction_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_operator_loop_write_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_operator_loop_commit_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_runtime_run_transaction_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_runtime_run_write_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn map_runtime_run_commit_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!("{message} {}: {error}", sqlite_path_suffix(database_path)),
        )
    }
}

fn sqlite_path_suffix(database_path: &Path) -> String {
    format!("against {}.", database_path.display())
}

fn is_unique_constraint_violation(error: &SqlError) -> bool {
    match error {
        SqlError::SqliteFailure(inner, _) => {
            matches!(inner.code, ErrorCode::ConstraintViolation)
        }
        _ => false,
    }
}

fn is_retryable_sql_error(error: &SqlError) -> bool {
    match error {
        SqlError::SqliteFailure(inner, _) => {
            matches!(
                inner.code,
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked
            )
        }
        _ => false,
    }
}

fn decode_snapshot_row_id(
    value: i64,
    field: &str,
    database_path: &Path,
    code: &str,
) -> Result<u32, CommandError> {
    u32::try_from(value).map_err(|_| {
        map_snapshot_decode_error(
            code,
            database_path,
            format!("Field `{field}` must be a non-negative 32-bit integer, found {value}."),
        )
    })
}

fn decode_runtime_run_checkpoint_sequence(
    value: i64,
    field: &str,
    database_path: &Path,
) -> Result<u32, CommandError> {
    u32::try_from(value).map_err(|_| {
        map_runtime_run_decode_error(
            database_path,
            format!("Field `{field}` must be a non-negative 32-bit integer, found {value}."),
        )
    })
}

fn require_runtime_run_non_empty_owned(
    value: String,
    field: &str,
    database_path: &Path,
) -> Result<String, CommandError> {
    if value.trim().is_empty() {
        Err(map_runtime_run_decode_error(
            database_path,
            format!("Field `{field}` must be a non-empty string."),
        ))
    } else {
        Ok(value)
    }
}

fn decode_runtime_run_optional_non_empty_text(
    value: Option<String>,
    field: &str,
    database_path: &Path,
) -> Result<Option<String>, CommandError> {
    match value {
        Some(value) if value.trim().is_empty() => Err(map_runtime_run_decode_error(
            database_path,
            format!("Field `{field}` must be null or a non-empty string."),
        )),
        other => Ok(other),
    }
}

fn decode_runtime_run_bool(
    value: i64,
    field: &str,
    database_path: &Path,
) -> Result<bool, CommandError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(map_runtime_run_decode_error(
            database_path,
            format!("Field `{field}` must be 0 or 1, found {other}."),
        )),
    }
}

fn decode_runtime_run_reason(
    code: Option<String>,
    message: Option<String>,
    field: &str,
    database_path: &Path,
) -> Result<Option<RuntimeRunDiagnosticRecord>, CommandError> {
    match (code, message) {
        (None, None) => Ok(None),
        (Some(code), Some(message)) => Ok(Some(RuntimeRunDiagnosticRecord {
            code: require_runtime_run_non_empty_owned(
                code,
                &format!("{field}_code"),
                database_path,
            )?,
            message: require_runtime_run_non_empty_owned(
                message,
                &format!("{field}_message"),
                database_path,
            )?,
        })),
        _ => Err(map_runtime_run_decode_error(
            database_path,
            format!("Field `{field}` must have both code and message populated together."),
        )),
    }
}

fn require_runtime_run_checkpoint_non_empty_owned(
    value: String,
    field: &str,
    database_path: &Path,
) -> Result<String, CommandError> {
    if value.trim().is_empty() {
        Err(map_runtime_run_checkpoint_decode_error(
            database_path,
            format!("Field `{field}` must be a non-empty string."),
        ))
    } else {
        Ok(value)
    }
}

fn require_non_empty_owned(
    value: String,
    field: &str,
    database_path: &Path,
    code: &str,
) -> Result<String, CommandError> {
    if value.trim().is_empty() {
        Err(map_snapshot_decode_error(
            code,
            database_path,
            format!("Field `{field}` must be a non-empty string."),
        ))
    } else {
        Ok(value)
    }
}

fn decode_optional_non_empty_text(
    value: Option<String>,
    field: &str,
    database_path: &Path,
    code: &str,
) -> Result<Option<String>, CommandError> {
    match value {
        Some(value) if value.trim().is_empty() => Err(map_snapshot_decode_error(
            code,
            database_path,
            format!("Field `{field}` must be null or a non-empty string."),
        )),
        other => Ok(other),
    }
}

fn parse_notification_dispatch_status(value: &str) -> Result<NotificationDispatchStatus, String> {
    match value {
        "pending" => Ok(NotificationDispatchStatus::Pending),
        "sent" => Ok(NotificationDispatchStatus::Sent),
        "failed" => Ok(NotificationDispatchStatus::Failed),
        "claimed" => Ok(NotificationDispatchStatus::Claimed),
        other => Err(format!(
            "Field `status` must be a known notification-dispatch status, found `{other}`."
        )),
    }
}

fn notification_dispatch_status_sql_value(value: &NotificationDispatchStatus) -> &'static str {
    match value {
        NotificationDispatchStatus::Pending => "pending",
        NotificationDispatchStatus::Sent => "sent",
        NotificationDispatchStatus::Failed => "failed",
        NotificationDispatchStatus::Claimed => "claimed",
    }
}

fn parse_notification_reply_claim_status(
    value: &str,
) -> Result<NotificationReplyClaimStatus, String> {
    match value {
        "accepted" => Ok(NotificationReplyClaimStatus::Accepted),
        "rejected" => Ok(NotificationReplyClaimStatus::Rejected),
        other => Err(format!(
            "Field `status` must be a known notification-reply claim status, found `{other}`."
        )),
    }
}

fn parse_phase_status(value: &str) -> Result<PhaseStatus, String> {
    match value {
        "complete" => Ok(PhaseStatus::Complete),
        "active" => Ok(PhaseStatus::Active),
        "pending" => Ok(PhaseStatus::Pending),
        "blocked" => Ok(PhaseStatus::Blocked),
        other => Err(format!("Unknown phase status `{other}`.")),
    }
}

fn parse_phase_step(value: &str) -> Result<PhaseStep, String> {
    match value {
        "discuss" => Ok(PhaseStep::Discuss),
        "plan" => Ok(PhaseStep::Plan),
        "execute" => Ok(PhaseStep::Execute),
        "verify" => Ok(PhaseStep::Verify),
        "ship" => Ok(PhaseStep::Ship),
        other => Err(format!("Unknown phase current_step `{other}`.")),
    }
}

fn map_snapshot_decode_error(code: &str, database_path: &Path, details: String) -> CommandError {
    CommandError::system_fault(
        code,
        format!(
            "Cadence could not decode selected-project operator-loop metadata from {}: {details}",
            database_path.display()
        ),
    )
}

fn map_runtime_decode_error(database_path: &Path, details: String) -> CommandError {
    CommandError::system_fault(
        "runtime_session_decode_failed",
        format!(
            "Cadence could not decode runtime-session metadata from {}: {details}",
            database_path.display()
        ),
    )
}

fn map_runtime_run_decode_error(database_path: &Path, details: String) -> CommandError {
    CommandError::system_fault(
        "runtime_run_decode_failed",
        format!(
            "Cadence could not decode durable runtime-run metadata from {}: {details}",
            database_path.display()
        ),
    )
}

fn map_runtime_run_checkpoint_decode_error(database_path: &Path, details: String) -> CommandError {
    CommandError::system_fault(
        "runtime_run_checkpoint_decode_failed",
        format!(
            "Cadence could not decode durable runtime-run checkpoints from {}: {details}",
            database_path.display()
        ),
    )
}

fn map_project_query_error(
    error: SqlError,
    database_path: &Path,
    repo_root: &Path,
    expected_project_id: &str,
) -> CommandError {
    match error {
        SqlError::QueryReturnedNoRows => CommandError::system_fault(
            "project_registry_mismatch",
            format!(
                "Registry entry for {} expected project `{expected_project_id}`, but {} did not contain that project row.",
                repo_root.display(),
                database_path.display()
            ),
        ),
        other => CommandError::system_fault(
            "project_summary_query_failed",
            format!(
                "Cadence could not read the project summary from {}: {other}",
                database_path.display()
            ),
        ),
    }
}
