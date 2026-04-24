#[path = "runtime_event_stream/support.rs"]
mod support;

#[path = "runtime_event_stream/preflight.rs"]
mod preflight;

#[path = "runtime_event_stream/attach_replay.rs"]
mod attach_replay;

#[path = "runtime_event_stream/item_contracts.rs"]
mod item_contracts;

#[test]
fn subscribe_runtime_stream_rejects_missing_channel_and_unsupported_kind_lists_activity() {
    preflight::subscribe_runtime_stream_rejects_missing_channel_and_unsupported_kind_lists_activity(
    );
}

#[test]
fn subscribe_runtime_stream_fails_closed_without_an_attachable_durable_run() {
    preflight::subscribe_runtime_stream_fails_closed_without_an_attachable_durable_run();
}

#[test]
fn subscribe_runtime_stream_returns_run_scoped_response_for_an_attachable_run() {
    preflight::subscribe_runtime_stream_returns_run_scoped_response_for_an_attachable_run();
}

#[test]
fn runtime_stream_replays_real_supervisor_events_after_fresh_host_reload() {
    attach_replay::runtime_stream_replays_real_supervisor_events_after_fresh_host_reload();
}

#[test]
fn runtime_stream_replays_first_class_skill_events_with_source_metadata_after_fresh_host_reload() {
    attach_replay::runtime_stream_replays_first_class_skill_events_with_source_metadata_after_fresh_host_reload();
}

#[test]
fn runtime_stream_appends_pending_action_required_after_replay_with_monotonic_sequence() {
    item_contracts::runtime_stream_appends_pending_action_required_after_replay_with_monotonic_sequence();
}

#[test]
fn runtime_stream_dedupes_replayed_action_required_against_durable_pending_queue() {
    item_contracts::runtime_stream_dedupes_replayed_action_required_against_durable_pending_queue();
}

#[test]
fn runtime_stream_dropped_channel_does_not_poison_resubscribe() {
    attach_replay::runtime_stream_dropped_channel_does_not_poison_resubscribe();
}

#[test]
fn runtime_stream_redacts_secret_bearing_replay_without_leaking_tokens() {
    item_contracts::runtime_stream_redacts_secret_bearing_replay_without_leaking_tokens();
}

#[test]
fn runtime_stream_replays_mcp_tool_summary_variant_with_monotonic_sequence() {
    item_contracts::runtime_stream_replays_mcp_tool_summary_variant_with_monotonic_sequence();
}

#[test]
fn runtime_stream_replays_browser_computer_use_tool_summary_variant_with_monotonic_sequence() {
    item_contracts::runtime_stream_replays_browser_computer_use_tool_summary_variant_with_monotonic_sequence();
}

#[test]
fn runtime_stream_emits_typed_failure_when_supervisor_sequence_is_invalid() {
    attach_replay::runtime_stream_emits_typed_failure_when_supervisor_sequence_is_invalid();
}

#[test]
fn runtime_stream_contract_serialization_exposes_run_id_sequence_and_activity() {
    item_contracts::runtime_stream_contract_serialization_exposes_run_id_sequence_and_activity();
}
