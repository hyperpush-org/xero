#[path = "runtime_run_persistence/runtime_rows.rs"]
mod runtime_rows;
#[path = "runtime_run_persistence/support.rs"]
mod support;

#[test]
fn runtime_run_recovery_distinguishes_running_stale_stopped_and_failed_states() {
    runtime_rows::runtime_run_recovery_distinguishes_running_stale_stopped_and_failed_states();
}

#[test]
fn runtime_run_persists_active_and_pending_control_snapshots_with_queued_prompt() {
    runtime_rows::runtime_run_persists_active_and_pending_control_snapshots_with_queued_prompt();
}

#[test]
fn runtime_run_persistence_isolates_runs_by_agent_session() {
    runtime_rows::runtime_run_persistence_isolates_runs_by_agent_session();
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
