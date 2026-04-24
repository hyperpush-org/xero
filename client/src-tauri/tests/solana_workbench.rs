//! Integration tests for the Solana workbench Phase 1 surface.
//!
//! These tests drive the public API in `cadence_desktop_lib::commands::solana`
//! end-to-end — they do not spawn a real `solana-test-validator`. Instead
//! they swap in an injectable launcher and account fetcher so we can run in
//! CI without a Solana toolchain.
//!
//! The three acceptance criteria from the plan we care about here:
//!
//!   1. Spin-restore-spin cycle runs three times in a row (validator
//!      supervisor is idempotent and the snapshot store is bit-identical).
//!   2. Failover: killing the primary RPC mid-session routes the next call
//!      to the next healthy endpoint.
//!   3. Missing-toolchain state renders a predictable struct shape.
//!
//! Matches the layout of `runtime_supervisor.rs` — a single top-level file
//! with focused submodule files under `tests/solana/`.

#[path = "solana/support.rs"]
mod support;

#[path = "solana/spin_restore_cycle.rs"]
mod spin_restore_cycle;

#[path = "solana/rpc_failover.rs"]
mod rpc_failover;

#[path = "solana/toolchain_shape.rs"]
mod toolchain_shape;

#[path = "solana/persona_lifecycle.rs"]
mod persona_lifecycle;

#[path = "solana/audit_engine.rs"]
mod audit_engine;

#[test]
fn spin_restore_cycle_runs_three_consecutive_times() {
    spin_restore_cycle::spin_restore_cycle_runs_three_consecutive_times();
}

#[test]
fn snapshot_restore_is_bit_identical_across_process_boundary() {
    spin_restore_cycle::snapshot_restore_is_bit_identical_across_process_boundary();
}

#[test]
fn starting_second_cluster_replaces_the_first() {
    spin_restore_cycle::starting_second_cluster_replaces_the_first();
}

#[test]
fn rpc_router_fails_over_when_primary_endpoint_goes_down() {
    rpc_failover::rpc_router_fails_over_when_primary_endpoint_goes_down();
}

#[test]
fn rpc_router_recovers_when_primary_endpoint_comes_back() {
    rpc_failover::rpc_router_recovers_when_primary_endpoint_comes_back();
}

#[test]
fn rpc_router_set_endpoints_replaces_default_pool() {
    rpc_failover::rpc_router_set_endpoints_replaces_default_pool();
}

#[test]
fn toolchain_probe_returns_well_shaped_struct_on_this_host() {
    toolchain_shape::toolchain_probe_returns_well_shaped_struct_on_this_host();
}

#[test]
fn toolchain_probe_serializes_to_camel_case_json() {
    toolchain_shape::toolchain_probe_serializes_to_camel_case_json();
}

#[test]
fn whale_persona_created_under_budget_on_localnet() {
    persona_lifecycle::whale_persona_created_under_budget_on_localnet();
}

#[test]
fn persona_mainnet_operations_are_policy_denied() {
    persona_lifecycle::persona_mainnet_operations_are_policy_denied();
}

#[test]
fn localnet_keypair_import_works() {
    persona_lifecycle::localnet_keypair_import_works();
}

#[test]
fn self_contained_scenario_runs_end_to_end() {
    persona_lifecycle::self_contained_scenario_runs_end_to_end();
}

#[test]
fn pipeline_scenario_pre_stages_on_mainnet_fork() {
    persona_lifecycle::pipeline_scenario_pre_stages_on_mainnet_fork();
}

#[test]
fn fund_command_rejects_empty_delta() {
    persona_lifecycle::fund_command_rejects_empty_delta();
}

// -- Phase 6 — audit engine ------------------------------------------------

#[test]
fn audit_static_lints_stream_findings_in_phase_order() {
    audit_engine::static_lints_stream_findings_in_phase_order();
}

#[test]
fn audit_external_analyzer_not_installed_returns_informational_finding() {
    audit_engine::external_analyzer_not_installed_returns_informational_finding();
}

#[test]
fn audit_external_analyzer_parses_scripted_json_output() {
    audit_engine::external_analyzer_parses_scripted_json_output();
}

#[test]
fn audit_fuzz_reports_crashes_with_reproducer() {
    audit_engine::fuzz_engine_reports_crashes_with_reproducer();
}

#[test]
fn audit_coverage_parses_instruction_rollups() {
    audit_engine::coverage_parses_instruction_rollups();
}

#[test]
fn audit_replay_catalog_returns_four_exploits_and_refuses_mainnet() {
    audit_engine::replay_catalog_returns_four_exploits_and_refuses_mainnet();
}

#[test]
fn audit_replay_scripted_runner_emits_expected_bad_state_finding() {
    audit_engine::replay_scripted_runner_emits_expected_bad_state_finding();
}

#[test]
fn audit_twenty_instruction_anchor_program_audit_is_fast() {
    audit_engine::twenty_instruction_anchor_program_audit_is_fast();
}
