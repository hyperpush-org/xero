mod autonomous;
mod connection;
mod notifications;
mod operator;
mod project_snapshot;
mod runtime;
mod runtime_boundary;
pub(crate) mod workflow;

pub use autonomous::*;
pub(crate) use connection::{open_project_database, open_runtime_database};
pub use notifications::*;
pub(crate) use notifications::{
    enqueue_notification_dispatches_best_effort_with_connection,
    format_notification_dispatch_enqueue_outcome,
};
pub use operator::*;
pub(crate) use operator::{
    decode_optional_non_empty_text, decode_snapshot_row_id, derive_operator_action_id,
    derive_operator_scope_prefix, is_retryable_sql_error, is_unique_constraint_violation,
    map_operator_loop_commit_error, map_operator_loop_transaction_error,
    map_operator_loop_write_error, map_project_query_error, map_snapshot_decode_error,
    operator_approval_status_label, parse_phase_status, parse_phase_step,
    read_operator_approval_by_action_id, read_operator_approvals, read_resume_history,
    read_resume_history_entry_by_id, read_verification_records, require_non_empty_owned,
    sqlite_path_suffix, ProjectSummaryRow,
};
pub use project_snapshot::{load_project_snapshot, load_project_summary, ProjectSnapshotRecord};
pub(crate) use project_snapshot::{
    planning_lifecycle_stage_label, read_phase_summaries, read_planning_lifecycle_projection,
};
pub use runtime::*;
pub(crate) use runtime::{
    find_prohibited_runtime_persistence_content, find_prohibited_transition_diagnostic_content,
    map_runtime_run_write_error, normalize_runtime_checkpoint_summary, read_runtime_run_row,
    read_runtime_run_snapshot, read_runtime_session_row, runtime_run_checkpoint_kind_sql_value,
    validate_runtime_action_required_payload,
};
pub(crate) use runtime_boundary::classify_operator_answer_requirement;
pub use runtime_boundary::*;
pub use workflow::*;
pub(crate) use workflow::{
    apply_workflow_transition_mutation, attempt_automatic_dispatch_after_transition,
    compute_workflow_handoff_package_hash, decode_operator_resume_transition_context,
    derive_resume_transition_id, is_plan_mode_required_gate_key, read_latest_transition_id,
    read_project_row, read_transition_event_by_transition_id,
    read_workflow_handoff_package_by_transition_id, resolve_operator_approval_gate_link,
    validate_non_empty_text, validate_workflow_handoff_package_hash,
    validate_workflow_handoff_package_transition_linkage, OperatorApprovalGateLink,
    ResolveOperatorAnswerRequirement, RuntimeOperatorResumeTarget,
    WorkflowTransitionGateMutationRecord, WorkflowTransitionMutationApplyOutcome,
    WorkflowTransitionMutationRecord, OPERATOR_RESUME_MUTATION_ERROR_PROFILE,
};
