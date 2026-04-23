#[path = "runtime_run_bridge/support.rs"]
mod support;

#[path = "runtime_run_bridge/runtime_recovery.rs"]
mod runtime_recovery;

#[path = "runtime_run_bridge/autonomous_recovery.rs"]
mod autonomous_recovery;

#[path = "runtime_run_bridge/workflow_progression.rs"]
mod workflow_progression;

#[path = "runtime_run_bridge/notification_operator.rs"]
mod notification_operator;

#[path = "runtime_run_bridge/control_contract.rs"]
mod control_contract;

// Keep this bridge target on its documented serialized rerun path when detached-supervisor
// timing is under load; preserve the behavior contract instead of adding sleeps or retries.

#[test]
fn get_runtime_run_returns_none_when_selected_project_has_no_durable_run() {
    runtime_recovery::get_runtime_run_returns_none_when_selected_project_has_no_durable_run();
}

#[test]
fn get_runtime_run_fails_closed_for_malformed_durable_rows_without_projection_event_drift() {
    runtime_recovery::get_runtime_run_fails_closed_for_malformed_durable_rows_without_projection_event_drift();
}

#[test]
fn start_runtime_run_requires_authenticated_runtime_session() {
    runtime_recovery::start_runtime_run_requires_authenticated_runtime_session();
}

#[test]
fn start_runtime_run_reconnects_existing_run_without_duplicate_launch_or_auth_event_drift() {
    runtime_recovery::start_runtime_run_reconnects_existing_run_without_duplicate_launch_or_auth_event_drift();
}

#[test]
fn get_runtime_run_recovers_truthful_running_state_after_fresh_host_reload() {
    runtime_recovery::get_runtime_run_recovers_truthful_running_state_after_fresh_host_reload();
}

#[test]
fn start_autonomous_run_reuses_existing_boundary_and_persists_duplicate_start_visibility() {
    autonomous_recovery::start_autonomous_run_reuses_existing_boundary_and_persists_duplicate_start_visibility();
}

#[test]
fn autonomous_run_rehydrates_same_boundary_after_reload_and_prevents_duplicate_continuation() {
    autonomous_recovery::autonomous_run_rehydrates_same_boundary_after_reload_and_prevents_duplicate_continuation();
}

#[test]
fn get_autonomous_run_recovers_stale_boundary_after_fresh_host_reload() {
    autonomous_recovery::get_autonomous_run_recovers_stale_boundary_after_fresh_host_reload();
}

#[test]
fn get_runtime_run_recovers_stale_unreachable_state_once_after_fresh_host_reload() {
    runtime_recovery::get_runtime_run_recovers_stale_unreachable_state_once_after_fresh_host_reload(
    );
}

#[test]
fn apply_workflow_transition_gate_pause_returns_skipped_diagnostics_and_truthful_project_update() {
    workflow_progression::apply_workflow_transition_gate_pause_returns_skipped_diagnostics_and_truthful_project_update();
}

#[test]
fn gate_linked_resume_gate_pause_returns_skipped_diagnostics_without_runtime_event_drift() {
    workflow_progression::gate_linked_resume_gate_pause_returns_skipped_diagnostics_without_runtime_event_drift();
}

#[test]
fn gate_linked_resume_auto_dispatch_emits_project_update_without_runtime_event_drift() {
    workflow_progression::gate_linked_resume_auto_dispatch_emits_project_update_without_runtime_event_drift();
}

#[test]
fn runtime_action_required_persistence_enqueues_notification_dispatches_once_per_route() {
    notification_operator::runtime_action_required_persistence_enqueues_notification_dispatches_once_per_route();
}

#[test]
fn submit_notification_reply_first_wins_and_rejects_forged_and_duplicate_replies() {
    notification_operator::submit_notification_reply_first_wins_and_rejects_forged_and_duplicate_replies();
}

#[test]
fn submit_notification_reply_cross_channel_race_accepts_single_winner_and_preserves_resume_truth() {
    notification_operator::submit_notification_reply_cross_channel_race_accepts_single_winner_and_preserves_resume_truth();
}

#[test]
fn planning_lifecycle_completion_branch_auto_dispatches_to_roadmap_without_duplicate_rows() {
    workflow_progression::planning_lifecycle_completion_branch_auto_dispatches_to_roadmap_without_duplicate_rows();
}

#[test]
fn planning_lifecycle_gate_pause_branch_requires_explicit_resume_without_duplicate_rows() {
    workflow_progression::planning_lifecycle_gate_pause_branch_requires_explicit_resume_without_duplicate_rows();
}

#[test]
fn start_autonomous_run_mints_fresh_child_unit_and_attempt_identity_per_stage() {
    workflow_progression::start_autonomous_run_mints_fresh_child_unit_and_attempt_identity_per_stage();
}

#[test]
fn get_autonomous_run_fails_closed_when_workflow_graph_has_no_active_node() {
    workflow_progression::get_autonomous_run_fails_closed_when_workflow_graph_has_no_active_node();
}

#[test]
fn get_autonomous_run_fails_closed_for_invalid_workflow_node_to_unit_mapping() {
    workflow_progression::get_autonomous_run_fails_closed_for_invalid_workflow_node_to_unit_mapping(
    );
}

#[test]
fn get_autonomous_run_rejects_stale_workflow_linkage_after_active_stage_drift() {
    workflow_progression::get_autonomous_run_rejects_stale_workflow_linkage_after_active_stage_drift();
}

#[test]
fn resume_operator_run_delivers_approved_terminal_input_without_auth_event_drift() {
    notification_operator::resume_operator_run_delivers_approved_terminal_input_without_auth_event_drift();
}

#[test]
fn submit_notification_reply_persists_autonomous_boundary_and_resume_evidence_exactly_once() {
    notification_operator::submit_notification_reply_persists_autonomous_boundary_and_resume_evidence_exactly_once();
}

#[test]
fn resume_operator_run_records_failed_history_when_runtime_identity_session_is_stale() {
    notification_operator::resume_operator_run_records_failed_history_when_runtime_identity_session_is_stale();
}

#[test]
fn resume_operator_run_records_failed_history_when_detached_control_channel_is_unreachable() {
    notification_operator::resume_operator_run_records_failed_history_when_detached_control_channel_is_unreachable();
}

#[test]
fn submit_notification_reply_resumes_shell_review_boundary_without_duplicate_rows() {
    notification_operator::submit_notification_reply_resumes_shell_review_boundary_without_duplicate_rows();
}

#[test]
fn start_runtime_run_replaces_stale_row_with_new_reachable_run() {
    runtime_recovery::start_runtime_run_replaces_stale_row_with_new_reachable_run();
}

#[test]
fn stop_runtime_run_rejects_mismatched_run_id_and_marks_unreachable_sidecar_stale() {
    runtime_recovery::stop_runtime_run_rejects_mismatched_run_id_and_marks_unreachable_sidecar_stale();
}

#[test]
fn queued_runtime_controls_apply_on_next_model_boundary_and_recover_reload_truth() {
    control_contract::queued_runtime_controls_apply_on_next_model_boundary_and_recover_reload_truth(
    );
}

#[test]
fn queued_runtime_controls_duplicate_boundary_is_idempotent() {
    control_contract::queued_runtime_controls_duplicate_boundary_is_idempotent();
}

#[test]
fn get_runtime_run_fails_closed_for_malformed_control_state_without_fake_apply_transition() {
    control_contract::get_runtime_run_fails_closed_for_malformed_control_state_without_fake_apply_transition();
}

#[test]
fn stop_runtime_run_returns_existing_terminal_snapshot_after_sidecar_exit() {
    runtime_recovery::stop_runtime_run_returns_existing_terminal_snapshot_after_sidecar_exit();
}

#[test]
fn start_runtime_run_launches_anthropic_with_truthful_provider_identity_and_secret_free_persistence(
) {
    runtime_recovery::start_runtime_run_launches_anthropic_with_truthful_provider_identity_and_secret_free_persistence();
}

#[test]
fn start_runtime_run_launches_openai_compatible_with_truthful_provider_identity_and_secret_free_persistence(
) {
    runtime_recovery::start_runtime_run_launches_openai_compatible_with_truthful_provider_identity_and_secret_free_persistence();
}
