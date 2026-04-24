#[path = "runtime_run_persistence/autonomous_runs.rs"]
mod autonomous_runs;
#[path = "runtime_run_persistence/runtime_rows.rs"]
mod runtime_rows;
#[path = "runtime_run_persistence/structured_payloads.rs"]
mod structured_payloads;
#[path = "runtime_run_persistence/support.rs"]
mod support;

#[test]
fn legacy_repo_local_state_is_upgraded_before_runtime_run_reads() {
    runtime_rows::legacy_repo_local_state_is_upgraded_before_runtime_run_reads();
}

#[test]
fn runtime_run_recovery_distinguishes_running_stale_stopped_and_failed_states() {
    runtime_rows::runtime_run_recovery_distinguishes_running_stale_stopped_and_failed_states();
}

#[test]
fn runtime_run_persists_active_and_pending_control_snapshots_with_queued_prompt() {
    runtime_rows::runtime_run_persists_active_and_pending_control_snapshots_with_queued_prompt();
}

#[test]
fn runtime_run_checkpoint_writes_reject_secret_bearing_summaries_and_preserve_prior_sequence() {
    runtime_rows::runtime_run_checkpoint_writes_reject_secret_bearing_summaries_and_preserve_prior_sequence();
}

#[test]
fn runtime_run_decode_fails_closed_for_malformed_status_transport_and_checkpoint_kind() {
    runtime_rows::runtime_run_decode_fails_closed_for_malformed_status_transport_checkpoint_kind_and_controls();
}

#[test]
fn runtime_run_checkpoint_sequence_must_increase_monotonically() {
    runtime_rows::runtime_run_checkpoint_sequence_must_increase_monotonically();
}

#[test]
fn autonomous_run_persistence_tracks_current_unit_duplicate_start_and_cancel_metadata() {
    autonomous_runs::autonomous_run_persistence_tracks_current_unit_duplicate_start_and_cancel_metadata();
}

#[test]
fn autonomous_run_persistence_persists_explicit_workflow_linkage_and_replays_idempotently() {
    autonomous_runs::autonomous_run_persistence_persists_explicit_workflow_linkage_and_replays_idempotently();
}

#[test]
fn autonomous_run_persistence_rejects_blank_workflow_linkage_fields() {
    autonomous_runs::autonomous_run_persistence_rejects_blank_workflow_linkage_fields();
}

#[test]
fn autonomous_run_decode_fails_closed_for_cross_project_workflow_linkage_tampering() {
    autonomous_runs::autonomous_run_decode_fails_closed_for_cross_project_workflow_linkage_tampering();
}

#[test]
fn autonomous_run_persistence_canonicalizes_structured_artifact_payloads_and_reloads_them() {
    structured_payloads::autonomous_run_persistence_canonicalizes_structured_artifact_payloads_and_reloads_them();
}

#[test]
fn autonomous_run_persistence_canonicalizes_mcp_tool_result_payloads_and_reloads_them() {
    structured_payloads::autonomous_run_persistence_canonicalizes_mcp_tool_result_payloads_and_reloads_them();
}

#[test]
fn autonomous_run_persistence_canonicalizes_verification_evidence_payloads_and_reloads_them() {
    structured_payloads::autonomous_run_persistence_canonicalizes_verification_evidence_payloads_and_reloads_them();
}

#[test]
fn autonomous_run_persistence_canonicalizes_skill_lifecycle_payloads_and_reloads_them() {
    structured_payloads::autonomous_run_persistence_canonicalizes_skill_lifecycle_payloads_and_reloads_them();
}

#[test]
fn autonomous_run_persistence_rejects_verification_evidence_action_boundary_mismatch() {
    structured_payloads::autonomous_run_persistence_rejects_verification_evidence_action_boundary_mismatch();
}

#[test]
fn autonomous_run_persistence_rejects_structured_artifact_payload_linkage_mismatch() {
    structured_payloads::autonomous_run_persistence_rejects_structured_artifact_payload_linkage_mismatch();
}

#[test]
fn autonomous_run_persistence_rejects_mcp_tool_summary_with_command_result() {
    structured_payloads::autonomous_run_persistence_rejects_mcp_tool_summary_with_command_result();
}

#[test]
fn autonomous_run_persistence_rejects_secret_bearing_structured_payload_content() {
    structured_payloads::autonomous_run_persistence_rejects_secret_bearing_structured_payload_content();
}

#[test]
fn autonomous_run_persistence_rejects_skill_lifecycle_payloads_without_tree_hash() {
    structured_payloads::autonomous_run_persistence_rejects_skill_lifecycle_payloads_without_tree_hash();
}

#[test]
fn autonomous_run_persistence_rejects_skill_lifecycle_kind_mismatch() {
    structured_payloads::autonomous_run_persistence_rejects_skill_lifecycle_kind_mismatch();
}

#[test]
fn autonomous_run_persistence_rejects_successful_skill_lifecycle_payloads_with_diagnostics() {
    structured_payloads::autonomous_run_persistence_rejects_successful_skill_lifecycle_payloads_with_diagnostics();
}

#[test]
fn autonomous_run_persistence_rejects_policy_denied_artifacts_without_stable_code() {
    structured_payloads::autonomous_run_persistence_rejects_policy_denied_artifacts_without_stable_code();
}

#[test]
fn autonomous_run_decode_fails_closed_when_structured_payload_json_is_tampered() {
    structured_payloads::autonomous_run_decode_fails_closed_when_structured_payload_json_is_tampered();
}

#[test]
fn autonomous_run_decode_fails_closed_when_mcp_capability_kind_is_tampered() {
    structured_payloads::autonomous_run_decode_fails_closed_when_mcp_capability_kind_is_tampered();
}

#[test]
fn autonomous_run_decode_fails_closed_when_skill_lifecycle_payload_stage_is_tampered() {
    structured_payloads::autonomous_run_decode_fails_closed_when_skill_lifecycle_payload_stage_is_tampered();
}

#[test]
fn autonomous_skill_lifecycle_persistence_is_replay_safe_across_stage_upserts() {
    structured_payloads::autonomous_skill_lifecycle_persistence_is_replay_safe_across_stage_upserts(
    );
}

#[test]
fn autonomous_run_decode_fails_closed_when_unit_row_is_missing() {
    autonomous_runs::autonomous_run_decode_fails_closed_when_unit_row_is_missing();
}
