#[path = "operator_loop_persistence/gate_linked.rs"]
mod gate_linked;
#[path = "operator_loop_persistence/legacy_upgrade.rs"]
mod legacy_upgrade;
#[path = "operator_loop_persistence/notification_claims.rs"]
mod notification_claims;
#[path = "operator_loop_persistence/resolve_resume.rs"]
mod resolve_resume;
#[path = "operator_loop_persistence/snapshot_projection.rs"]
mod snapshot_projection;
#[path = "operator_loop_persistence/support.rs"]
mod support;

#[test]
fn project_snapshot_returns_empty_operator_loop_arrays_when_no_rows_exist() {
    snapshot_projection::project_snapshot_returns_empty_operator_loop_arrays_when_no_rows_exist();
}

#[test]
fn project_snapshot_persists_operator_loop_metadata_across_reopens_in_stable_order() {
    snapshot_projection::project_snapshot_persists_operator_loop_metadata_across_reopens_in_stable_order();
}

#[test]
fn project_snapshot_scopes_operator_loop_rows_to_the_selected_project() {
    snapshot_projection::project_snapshot_scopes_operator_loop_rows_to_the_selected_project();
}

#[test]
fn malformed_operator_loop_rows_fail_closed_during_snapshot_decode() {
    snapshot_projection::malformed_operator_loop_rows_fail_closed_during_snapshot_decode();
}

#[test]
fn legacy_repo_local_state_is_upgraded_before_selected_project_snapshot_reads() {
    legacy_upgrade::legacy_repo_local_state_is_upgraded_before_selected_project_snapshot_reads();
}

#[test]
fn legacy_repo_local_state_upgrade_adds_workflow_handoff_package_schema() {
    legacy_upgrade::legacy_repo_local_state_upgrade_adds_workflow_handoff_package_schema();
}

#[test]
fn resolve_operator_action_persists_decision_and_verification_rows() {
    resolve_resume::resolve_operator_action_persists_decision_and_verification_rows();
}

#[test]
fn resume_operator_run_requires_approved_request_and_records_history() {
    resolve_resume::resume_operator_run_requires_approved_request_and_records_history();
}

#[test]
fn resolve_operator_action_rejects_wrong_project_and_already_resolved_requests() {
    resolve_resume::resolve_operator_action_rejects_wrong_project_and_already_resolved_requests();
}

#[test]
fn runtime_scoped_resume_rejects_conflicting_user_answer_without_persisting_history() {
    resolve_resume::runtime_scoped_resume_rejects_conflicting_user_answer_without_persisting_history();
}

#[test]
fn runtime_scoped_resume_rejects_corrupted_approved_answer_metadata_without_persisting_history() {
    resolve_resume::runtime_scoped_resume_rejects_corrupted_approved_answer_metadata_without_persisting_history();
}

#[test]
fn runtime_scoped_approval_requires_non_secret_user_answer_at_resolve_time() {
    resolve_resume::runtime_scoped_approval_requires_non_secret_user_answer_at_resolve_time();
}

#[test]
fn runtime_scoped_approval_rejects_malformed_runtime_identity_without_partial_writes() {
    resolve_resume::runtime_scoped_approval_rejects_malformed_runtime_identity_without_partial_writes();
}

#[test]
fn runtime_scoped_resume_rejects_already_resumed_autonomous_boundary_before_second_submit() {
    resolve_resume::runtime_scoped_resume_rejects_already_resumed_autonomous_boundary_before_second_submit();
}

#[test]
fn gate_linked_resume_applies_transition_and_records_causal_event() {
    gate_linked::gate_linked_resume_applies_transition_and_records_causal_event();
}

#[test]
fn gate_linked_resume_auto_dispatches_next_legal_edge_and_replays_idempotently() {
    gate_linked::gate_linked_resume_auto_dispatches_next_legal_edge_and_replays_idempotently();
}

#[test]
fn command_and_gate_linked_resume_persist_equivalent_transition_shapes() {
    gate_linked::command_and_gate_linked_resume_persist_equivalent_transition_shapes();
}

#[test]
fn gate_linked_resume_rejects_illegal_edge_without_side_effects() {
    gate_linked::gate_linked_resume_rejects_illegal_edge_without_side_effects();
}

#[test]
fn gate_linked_resume_rejects_unresolved_target_gates_without_side_effects() {
    gate_linked::gate_linked_resume_rejects_unresolved_target_gates_without_side_effects();
}

#[test]
fn gate_linked_resume_rejects_secret_user_answer_input_without_side_effects() {
    gate_linked::gate_linked_resume_rejects_secret_user_answer_input_without_side_effects();
}

#[test]
fn gate_linked_resume_rejects_missing_transition_context_without_side_effects() {
    gate_linked::gate_linked_resume_rejects_missing_transition_context_without_side_effects();
}

#[test]
fn gate_linked_approval_requires_non_secret_user_answer() {
    gate_linked::gate_linked_approval_requires_non_secret_user_answer();
}

#[test]
fn gate_linked_upsert_rejects_ambiguous_gate_context() {
    gate_linked::gate_linked_upsert_rejects_ambiguous_gate_context();
}

#[test]
fn repeated_action_type_uses_gate_scoped_action_ids() {
    gate_linked::repeated_action_type_uses_gate_scoped_action_ids();
}

#[test]
fn plan_mode_required_resume_unblocks_implementation_continuation_without_duplicate_rows() {
    gate_linked::plan_mode_required_resume_unblocks_implementation_continuation_without_duplicate_rows();
}

#[test]
fn runtime_scoped_resume_rejects_run_identity_mismatch_without_resumed_evidence_drift() {
    gate_linked::runtime_scoped_resume_rejects_run_identity_mismatch_without_resumed_evidence_drift(
    );
}

#[test]
fn runtime_scoped_resume_rejects_missing_boundary_identity_without_resumed_evidence_drift() {
    gate_linked::runtime_scoped_resume_rejects_missing_boundary_identity_without_resumed_evidence_drift();
}

#[test]
fn notification_dispatch_claim_flow_is_idempotent_for_pending_operator_approvals() {
    notification_claims::notification_dispatch_claim_flow_is_idempotent_for_pending_operator_approvals();
}
