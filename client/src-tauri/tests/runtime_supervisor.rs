#[path = "runtime_supervisor/support.rs"]
mod support;

#[path = "runtime_supervisor/launch_probe_stop.rs"]
mod launch_probe_stop;

#[path = "runtime_supervisor/attach_replay.rs"]
mod attach_replay;

#[path = "runtime_supervisor/live_boundary.rs"]
mod live_boundary;

#[test]
fn detached_supervisor_launches_and_recovers_after_fresh_host_probe() {
    launch_probe_stop::detached_supervisor_launches_and_recovers_after_fresh_host_probe();
}

#[test]
fn detached_supervisor_probe_marks_unreachable_run_stale() {
    launch_probe_stop::detached_supervisor_probe_marks_unreachable_run_stale();
}

#[test]
fn detached_supervisor_probe_preserves_provider_identity_after_stale_recovery() {
    launch_probe_stop::detached_supervisor_probe_preserves_provider_identity_after_stale_recovery();
}

#[test]
fn detached_supervisor_rejects_missing_shell_program() {
    launch_probe_stop::detached_supervisor_rejects_missing_shell_program();
}

#[test]
fn detached_supervisor_rejects_provider_runtime_kind_mismatch_launch_context() {
    launch_probe_stop::detached_supervisor_rejects_provider_runtime_kind_mismatch_launch_context();
}

#[test]
fn detached_supervisor_rejects_duplicate_running_project_launches() {
    launch_probe_stop::detached_supervisor_rejects_duplicate_running_project_launches();
}

#[test]
fn detached_supervisor_marks_fast_nonzero_exit_as_failed_without_live_attach() {
    launch_probe_stop::detached_supervisor_marks_fast_nonzero_exit_as_failed_without_live_attach();
}

#[test]
fn detached_supervisor_launches_anthropic_child_with_context_env_and_secret_free_persistence() {
    launch_probe_stop::detached_supervisor_launches_anthropic_child_with_context_env_and_secret_free_persistence();
}

#[test]
fn detached_supervisor_rejects_anthropic_launch_without_api_key_env() {
    launch_probe_stop::detached_supervisor_rejects_anthropic_launch_without_api_key_env();
}

#[test]
fn detached_supervisor_launches_bedrock_child_with_context_env_and_ambient_auth() {
    launch_probe_stop::detached_supervisor_launches_bedrock_child_with_context_env_and_ambient_auth(
    );
}

#[test]
fn detached_supervisor_rejects_bedrock_launch_without_aws_credentials() {
    launch_probe_stop::detached_supervisor_rejects_bedrock_launch_without_aws_credentials();
}

#[test]
fn detached_supervisor_launches_vertex_child_with_context_env_and_ambient_auth() {
    launch_probe_stop::detached_supervisor_launches_vertex_child_with_context_env_and_ambient_auth(
    );
}

#[test]
fn detached_supervisor_rejects_vertex_launch_without_adc() {
    launch_probe_stop::detached_supervisor_rejects_vertex_launch_without_adc();
}

#[test]
fn detached_supervisor_launches_openai_compatible_child_with_context_env_and_secret_free_persistence(
) {
    launch_probe_stop::detached_supervisor_launches_openai_compatible_child_with_context_env_and_secret_free_persistence();
}

#[test]
fn detached_supervisor_launches_github_models_child_with_context_env_and_secret_free_persistence() {
    launch_probe_stop::detached_supervisor_launches_github_models_child_with_context_env_and_secret_free_persistence();
}

#[test]
fn detached_supervisor_launches_ollama_child_without_api_key_env() {
    launch_probe_stop::detached_supervisor_launches_ollama_child_without_api_key_env();
}

#[test]
fn detached_supervisor_rejects_openai_compatible_launch_without_api_key_env() {
    launch_probe_stop::detached_supervisor_rejects_openai_compatible_launch_without_api_key_env();
}

#[test]
fn detached_supervisor_rejects_github_models_launch_without_token_env() {
    launch_probe_stop::detached_supervisor_rejects_github_models_launch_without_token_env();
}

#[test]
fn detached_supervisor_rejects_openai_compatible_launch_without_base_url_env() {
    launch_probe_stop::detached_supervisor_rejects_openai_compatible_launch_without_base_url_env();
}

#[test]
fn detached_supervisor_attach_replays_buffered_events_after_fresh_host_probe() {
    attach_replay::detached_supervisor_attach_replays_buffered_events_after_fresh_host_probe();
}

#[test]
fn detached_supervisor_attach_rejects_identity_mismatch_without_mutating_run() {
    attach_replay::detached_supervisor_attach_rejects_identity_mismatch_without_mutating_run();
}

#[test]
fn detached_supervisor_attach_rejects_invalid_cursor_without_mutating_run() {
    attach_replay::detached_supervisor_attach_rejects_invalid_cursor_without_mutating_run();
}

#[test]
fn detached_supervisor_attach_replays_only_bounded_ring_window() {
    attach_replay::detached_supervisor_attach_replays_only_bounded_ring_window();
}

#[test]
fn detached_supervisor_live_event_redacts_secret_bearing_output_in_replay_and_checkpoint() {
    live_boundary::detached_supervisor_live_event_redacts_secret_bearing_output_in_replay_and_checkpoint();
}

#[test]
fn detached_supervisor_live_event_drops_unsupported_structured_payload_kind() {
    live_boundary::detached_supervisor_live_event_drops_unsupported_structured_payload_kind();
}

#[test]
fn detached_supervisor_attach_rejects_finished_run() {
    attach_replay::detached_supervisor_attach_rejects_finished_run();
}

#[test]
fn detached_supervisor_persists_redacted_interactive_boundary_and_replays_same_action_identity() {
    live_boundary::detached_supervisor_persists_redacted_interactive_boundary_and_replays_same_action_identity();
}

#[test]
fn detached_supervisor_persists_matching_autonomous_boundary_once_before_reload() {
    live_boundary::detached_supervisor_persists_matching_autonomous_boundary_once_before_reload();
}

#[test]
fn detached_supervisor_coalesces_repeated_prompt_churn_into_one_boundary() {
    live_boundary::detached_supervisor_coalesces_repeated_prompt_churn_into_one_boundary();
}

#[test]
fn detached_supervisor_submit_input_routes_bytes_through_owned_writer() {
    live_boundary::detached_supervisor_submit_input_routes_bytes_through_owned_writer();
}

#[test]
fn submit_runtime_run_input_rejects_mismatched_ack_and_preserves_running_projection() {
    live_boundary::submit_runtime_run_input_rejects_mismatched_ack_and_preserves_running_projection(
    );
}

#[test]
fn submit_runtime_run_input_preserves_running_projection_on_malformed_control_response() {
    live_boundary::submit_runtime_run_input_preserves_running_projection_on_malformed_control_response();
}

#[test]
fn detached_supervisor_persists_structured_shell_review_boundary_and_replays_same_action_identity()
{
    live_boundary::detached_supervisor_persists_structured_shell_review_boundary_and_replays_same_action_identity();
}

#[test]
fn detached_supervisor_rejects_structured_shell_review_boundary_with_malformed_action_identity() {
    live_boundary::detached_supervisor_rejects_structured_shell_review_boundary_with_malformed_action_identity();
}
