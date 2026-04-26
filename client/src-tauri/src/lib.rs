pub mod auth;
pub mod commands;
pub mod db;
pub mod mcp;
pub mod notifications;
pub mod provider_models;
pub mod provider_profiles;
pub mod registry;
pub mod runtime;
pub mod state;
pub mod window_state;

pub mod git {
    pub mod diff;
    pub mod operations;
    pub mod repository;
    pub mod status;
}

pub fn configure_builder_with_state<R: tauri::Runtime>(
    builder: tauri::Builder<R>,
    desktop_state: state::DesktopState,
) -> tauri::Builder<R> {
    builder
        .manage(desktop_state)
        .manage(commands::BrowserState::default())
        .manage(commands::EmulatorState::default())
        .manage(commands::SolanaState::default())
        .register_asynchronous_uri_scheme_protocol(
            commands::emulator::URI_SCHEME,
            commands::emulator::handle_uri_scheme,
        )
        .register_asynchronous_uri_scheme_protocol(
            commands::solana::URI_SCHEME,
            commands::solana::handle_uri_scheme,
        )
        .setup(|app| {
            commands::solana::toolchain::configure_tauri_roots(app.handle());
            window_state::configure_main_window(app.handle().clone());

            // Sweep leftover emulator-related processes from a previous
            // crash. Best-effort — we only log the suspects so the user
            // can clean up manually.
            let zombies = commands::emulator::shutdown::zombie_processes();
            if !zombies.is_empty() {
                eprintln!(
                    "[emulator] found {} leftover process(es) from a previous session: {}",
                    zombies.len(),
                    zombies
                        .iter()
                        .map(|z| format!("{} (pid {})", z.name, z.pid))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                use tauri::Manager;
                commands::emulator::shutdown::shutdown_on_close(window.app_handle());
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::import_repository::import_repository,
            commands::list_projects::list_projects,
            commands::remove_project::remove_project,
            commands::agent_session::create_agent_session,
            commands::agent_session::list_agent_sessions,
            commands::agent_session::get_agent_session,
            commands::agent_session::update_agent_session,
            commands::agent_session::archive_agent_session,
            commands::agent_session::restore_agent_session,
            commands::agent_session::delete_agent_session,
            commands::agent_task::start_agent_task,
            commands::agent_task::send_agent_message,
            commands::agent_task::cancel_agent_run,
            commands::agent_task::resume_agent_run,
            commands::agent_task::get_agent_run,
            commands::agent_task::list_agent_runs,
            commands::agent_task::subscribe_agent_stream,
            commands::session_history::get_session_transcript,
            commands::session_history::export_session_transcript,
            commands::session_history::save_session_transcript_export,
            commands::session_history::search_session_transcripts,
            commands::session_history::get_session_context_snapshot,
            commands::session_history::compact_session_history,
            commands::session_history::list_session_memories,
            commands::session_history::extract_session_memory_candidates,
            commands::session_history::update_session_memory,
            commands::session_history::delete_session_memory,
            commands::get_project_snapshot::get_project_snapshot,
            commands::get_repository_status::get_repository_status,
            commands::get_repository_diff::get_repository_diff,
            commands::git_operations::git_stage_paths,
            commands::git_operations::git_unstage_paths,
            commands::git_operations::git_discard_changes,
            commands::git_operations::git_commit,
            commands::git_operations::git_fetch,
            commands::git_operations::git_pull,
            commands::git_operations::git_push,
            commands::project_files::list_project_files,
            commands::project_files::read_project_file,
            commands::project_files::write_project_file,
            commands::project_files::create_project_entry,
            commands::project_files::rename_project_entry,
            commands::project_files::delete_project_entry,
            commands::search_project::search_project,
            commands::search_project::replace_in_project,
            commands::get_autonomous_run::get_autonomous_run,
            commands::get_runtime_run::get_runtime_run,
            commands::get_runtime_session::get_runtime_session,
            commands::get_runtime_settings::get_runtime_settings,
            commands::list_mcp_servers::list_mcp_servers,
            commands::upsert_mcp_server::upsert_mcp_server,
            commands::remove_mcp_server::remove_mcp_server,
            commands::import_mcp_servers::import_mcp_servers,
            commands::list_mcp_servers::refresh_mcp_server_statuses,
            commands::skills::list_skill_registry,
            commands::skills::reload_skill_registry,
            commands::skills::set_skill_enabled,
            commands::skills::remove_skill,
            commands::skills::upsert_skill_local_root,
            commands::skills::remove_skill_local_root,
            commands::skills::update_project_skill_source,
            commands::skills::update_github_skill_source,
            commands::skills::upsert_plugin_root,
            commands::skills::remove_plugin_root,
            commands::skills::set_plugin_enabled,
            commands::skills::remove_plugin,
            commands::provider_model_catalog::get_provider_model_catalog,
            commands::doctor_report::run_doctor_report,
            commands::provider_diagnostics::check_provider_profile,
            commands::provider_profiles::list_provider_profiles,
            commands::provider_profiles::upsert_provider_profile,
            commands::provider_profiles::set_active_provider_profile,
            commands::start_openai_login::start_openai_login,
            commands::submit_openai_callback::submit_openai_callback,
            commands::logout_runtime_session::logout_runtime_session,
            commands::start_autonomous_run::start_autonomous_run,
            commands::start_runtime_run::start_runtime_run,
            commands::update_runtime_run_controls::update_runtime_run_controls,
            commands::start_runtime_session::start_runtime_session,
            commands::cancel_autonomous_run::cancel_autonomous_run,
            commands::stop_runtime_run::stop_runtime_run,
            commands::subscribe_runtime_stream::subscribe_runtime_stream,
            commands::resolve_operator_action::resolve_operator_action,
            commands::resume_operator_run::resume_operator_run,
            commands::list_notification_routes::list_notification_routes,
            commands::list_notification_dispatches::list_notification_dispatches,
            commands::upsert_notification_route::upsert_notification_route,
            commands::upsert_notification_route_credentials::upsert_notification_route_credentials,
            commands::upsert_runtime_settings::upsert_runtime_settings,
            commands::record_notification_dispatch_outcome::record_notification_dispatch_outcome,
            commands::submit_notification_reply::submit_notification_reply,
            commands::sync_notification_adapters::sync_notification_adapters,
            commands::browser::browser_show,
            commands::browser::browser_resize,
            commands::browser::browser_hide,
            commands::browser::browser_eval,
            commands::browser::browser_current_url,
            commands::browser::browser_screenshot,
            commands::browser::browser_navigate,
            commands::browser::browser_back,
            commands::browser::browser_forward,
            commands::browser::browser_reload,
            commands::browser::browser_stop,
            commands::browser::browser_click,
            commands::browser::browser_type,
            commands::browser::browser_scroll,
            commands::browser::browser_press_key,
            commands::browser::browser_read_text,
            commands::browser::browser_query,
            commands::browser::browser_wait_for_selector,
            commands::browser::browser_wait_for_load,
            commands::browser::browser_history_state,
            commands::browser::browser_cookies_get,
            commands::browser::browser_cookies_set,
            commands::browser::browser_storage_read,
            commands::browser::browser_storage_write,
            commands::browser::browser_storage_clear,
            commands::browser::browser_tab_list,
            commands::browser::browser_tab_focus,
            commands::browser::browser_tab_close,
            commands::browser::browser_internal_reply,
            commands::browser::browser_internal_event,
            commands::browser::cookie_import::browser_list_cookie_sources,
            commands::browser::cookie_import::browser_import_cookies,
            commands::emulator::emulator_sdk_status,
            commands::emulator::emulator_ios_request_ax_permission,
            commands::emulator::emulator_ios_open_accessibility_settings,
            commands::emulator::android::provision::emulator_android_provision,
            commands::emulator::android::provision::emulator_android_provision_status,
            commands::emulator::emulator_list_devices,
            commands::emulator::emulator_start,
            commands::emulator::emulator_stop,
            commands::emulator::emulator_input,
            commands::emulator::emulator_rotate,
            commands::emulator::emulator_subscribe_ready,
            commands::emulator::emulator_screenshot,
            commands::emulator::emulator_ui_dump,
            commands::emulator::emulator_find,
            commands::emulator::emulator_tap,
            commands::emulator::emulator_swipe,
            commands::emulator::emulator_type,
            commands::emulator::emulator_press_key,
            commands::emulator::emulator_list_apps,
            commands::emulator::emulator_install_app,
            commands::emulator::emulator_uninstall_app,
            commands::emulator::emulator_launch_app,
            commands::emulator::emulator_terminate_app,
            commands::emulator::emulator_set_location,
            commands::emulator::emulator_push_notification,
            commands::emulator::emulator_logs_subscribe,
            commands::emulator::emulator_logs_unsubscribe,
            commands::emulator::emulator_logs_get_recent,
            commands::solana::solana_toolchain_install,
            commands::solana::solana_toolchain_install_status,
            commands::solana::solana_toolchain_status,
            commands::solana::solana_cluster_list,
            commands::solana::solana_cluster_start,
            commands::solana::solana_cluster_stop,
            commands::solana::solana_cluster_status,
            commands::solana::solana_snapshot_create,
            commands::solana::solana_snapshot_list,
            commands::solana::solana_snapshot_restore,
            commands::solana::solana_snapshot_delete,
            commands::solana::solana_rpc_health,
            commands::solana::solana_rpc_endpoints_set,
            commands::solana::solana_persona_list,
            commands::solana::solana_persona_roles,
            commands::solana::solana_persona_create,
            commands::solana::solana_persona_fund,
            commands::solana::solana_persona_delete,
            commands::solana::solana_persona_import_keypair,
            commands::solana::solana_persona_export_keypair,
            commands::solana::solana_scenario_list,
            commands::solana::solana_scenario_run,
            commands::solana::solana_tx_build,
            commands::solana::solana_tx_simulate,
            commands::solana::solana_tx_send,
            commands::solana::solana_tx_explain,
            commands::solana::solana_priority_fee_estimate,
            commands::solana::solana_cpi_resolve,
            commands::solana::solana_alt_create,
            commands::solana::solana_alt_extend,
            commands::solana::solana_alt_resolve,
            commands::solana::solana_idl_load,
            commands::solana::solana_idl_fetch,
            commands::solana::solana_idl_get,
            commands::solana::solana_idl_watch,
            commands::solana::solana_idl_unwatch,
            commands::solana::solana_idl_drift,
            commands::solana::solana_idl_publish,
            commands::solana::solana_codama_generate,
            commands::solana::solana_pda_derive,
            commands::solana::solana_pda_scan,
            commands::solana::solana_pda_predict,
            commands::solana::solana_pda_analyse_bump,
            commands::solana::solana_program_build,
            commands::solana::solana_program_upgrade_check,
            commands::solana::solana_program_deploy,
            commands::solana::solana_program_rollback,
            commands::solana::solana_squads_proposal_create,
            commands::solana::solana_verified_build_submit,
            commands::solana::solana_audit_static,
            commands::solana::solana_audit_external,
            commands::solana::solana_audit_fuzz,
            commands::solana::solana_audit_fuzz_scaffold,
            commands::solana::solana_audit_coverage,
            commands::solana::solana_replay_exploit,
            commands::solana::solana_replay_list,
            commands::solana::solana_logs_subscribe,
            commands::solana::solana_logs_unsubscribe,
            commands::solana::solana_logs_recent,
            commands::solana::solana_logs_active,
            commands::solana::solana_indexer_scaffold,
            commands::solana::solana_indexer_run,
            commands::solana::solana_token_extension_matrix,
            commands::solana::solana_token_create,
            commands::solana::solana_metaplex_mint,
            commands::solana::solana_wallet_scaffold_list,
            commands::solana::solana_wallet_scaffold_generate,
            commands::solana::solana_secrets_scan,
            commands::solana::solana_secrets_patterns,
            commands::solana::solana_secrets_scope_check,
            commands::solana::solana_cluster_drift_check,
            commands::solana::solana_cluster_drift_tracked_programs,
            commands::solana::solana_cost_snapshot,
            commands::solana::solana_cost_record,
            commands::solana::solana_cost_reset,
            commands::solana::solana_doc_catalog,
            commands::solana::solana_doc_snippets,
            commands::solana::solana_subscribe_ready,
        ])
}

pub fn configure_builder<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    configure_builder_with_state(builder, state::DesktopState::default())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_builder(tauri::Builder::default())
        .run(tauri::generate_context!())
        .expect("error while running Cadence desktop host");
}
