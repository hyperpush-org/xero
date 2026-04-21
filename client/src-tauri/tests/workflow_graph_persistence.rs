#[path = "workflow_graph_persistence/support.rs"]
mod support;
#[path = "workflow_graph_persistence/projection_decode.rs"]
mod projection_decode;
#[path = "workflow_graph_persistence/transition_dispatch.rs"]
mod transition_dispatch;
#[path = "workflow_graph_persistence/handoff_packages.rs"]
mod handoff_packages;

#[test]
fn workflow_graph_upsert_projects_phase_projection_in_stable_order() {
    projection_decode::workflow_graph_upsert_projects_phase_projection_in_stable_order();
}

#[test]
fn workflow_transition_is_transactional_and_event_is_durable() {
    transition_dispatch::workflow_transition_is_transactional_and_event_is_durable();
}

#[test]
fn workflow_transition_auto_dispatches_single_legal_next_edge() {
    transition_dispatch::workflow_transition_auto_dispatches_single_legal_next_edge();
}

#[test]
fn workflow_transition_auto_dispatch_fails_closed_on_ambiguous_next_edge() {
    transition_dispatch::workflow_transition_auto_dispatch_fails_closed_on_ambiguous_next_edge();
}

#[test]
fn workflow_transition_auto_dispatch_skips_when_next_edge_has_unresolved_gates() {
    transition_dispatch::workflow_transition_auto_dispatch_skips_when_next_edge_has_unresolved_gates();
}

#[test]
fn workflow_transition_auto_dispatch_replays_idempotently() {
    transition_dispatch::workflow_transition_auto_dispatch_replays_idempotently();
}

#[test]
fn workflow_transition_auto_dispatch_preserves_transition_when_handoff_package_build_fails() {
    transition_dispatch::workflow_transition_auto_dispatch_preserves_transition_when_handoff_package_build_fails();
}

#[test]
fn workflow_transition_auto_dispatch_replay_surfaces_missing_handoff_package_row() {
    transition_dispatch::workflow_transition_auto_dispatch_replay_surfaces_missing_handoff_package_row();
}

#[test]
fn workflow_transition_auto_dispatch_replay_surfaces_decode_failure_as_skipped_package() {
    transition_dispatch::workflow_transition_auto_dispatch_replay_surfaces_decode_failure_as_skipped_package();
}

#[test]
fn workflow_transition_auto_dispatch_replay_surfaces_transition_linkage_mismatch() {
    transition_dispatch::workflow_transition_auto_dispatch_replay_surfaces_transition_linkage_mismatch();
}

#[test]
fn workflow_transition_rejects_secret_bearing_diagnostics_and_preserves_state() {
    transition_dispatch::workflow_transition_rejects_secret_bearing_diagnostics_and_preserves_state();
}

#[test]
fn workflow_transition_rejects_malformed_target_gate_rows_and_preserves_state() {
    transition_dispatch::workflow_transition_rejects_malformed_target_gate_rows_and_preserves_state();
}

#[test]
fn workflow_snapshot_projects_planning_lifecycle_with_gate_and_transition_metadata() {
    projection_decode::workflow_snapshot_projects_planning_lifecycle_with_gate_and_transition_metadata();
}

#[test]
fn lifecycle_alias_collisions_fail_closed_during_projection_decode() {
    projection_decode::lifecycle_alias_collisions_fail_closed_during_projection_decode();
}

#[test]
fn malformed_lifecycle_gate_rows_fail_closed_during_projection_decode() {
    projection_decode::malformed_lifecycle_gate_rows_fail_closed_during_projection_decode();
}

#[test]
fn malformed_graph_rows_fail_closed_during_projection_decode() {
    projection_decode::malformed_graph_rows_fail_closed_during_projection_decode();
}

#[test]
fn malformed_graph_current_step_fails_closed_during_projection_decode() {
    projection_decode::malformed_graph_current_step_fails_closed_during_projection_decode();
}

#[test]
fn workflow_handoff_package_persists_with_transition_linkage_and_replays_idempotently() {
    handoff_packages::workflow_handoff_package_persists_with_transition_linkage_and_replays_idempotently();
}

#[test]
fn workflow_handoff_package_rejects_missing_transition_and_malformed_payload() {
    handoff_packages::workflow_handoff_package_rejects_missing_transition_and_malformed_payload();
}

#[test]
fn workflow_handoff_package_decode_fails_closed_on_corrupted_hash_rows() {
    handoff_packages::workflow_handoff_package_decode_fails_closed_on_corrupted_hash_rows();
}

#[test]
fn workflow_handoff_package_assembly_is_deterministic_for_replay_inputs() {
    handoff_packages::workflow_handoff_package_assembly_is_deterministic_for_replay_inputs();
}

#[test]
fn workflow_handoff_package_assembly_fails_closed_on_secret_bearing_destination_content() {
    handoff_packages::workflow_handoff_package_assembly_fails_closed_on_secret_bearing_destination_content();
}

#[test]
fn workflow_handoff_package_assembly_rejects_missing_target_node_metadata() {
    handoff_packages::workflow_handoff_package_assembly_rejects_missing_target_node_metadata();
}

#[test]
fn workflow_handoff_package_assembly_rejects_malformed_gate_state_rows() {
    handoff_packages::workflow_handoff_package_assembly_rejects_malformed_gate_state_rows();
}

#[test]
fn workflow_handoff_package_assembly_rejects_invalid_lifecycle_projection_shape() {
    handoff_packages::workflow_handoff_package_assembly_rejects_invalid_lifecycle_projection_shape();
}

#[test]
fn workflow_handoff_package_assembly_rejects_auto_transition_without_causal_linkage() {
    handoff_packages::workflow_handoff_package_assembly_rejects_auto_transition_without_causal_linkage();
}
