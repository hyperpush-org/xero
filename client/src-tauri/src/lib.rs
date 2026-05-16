pub mod auth;
pub mod commands;
pub mod db;
pub mod developer_tool_harness_terminal;
pub mod developer_tool_harness_tui;
pub mod environment;
pub mod global_db;
pub mod mcp;
pub mod notifications;
pub mod provider_credentials;
pub mod provider_models;
pub mod provider_preflight;
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

pub fn configure_builder_with_state<R: tauri::Runtime + 'static>(
    builder: tauri::Builder<R>,
    desktop_state: state::DesktopState,
) -> tauri::Builder<R> {
    builder
        .manage(desktop_state)
        .manage(commands::BrowserState::default())
        .manage(commands::DictationState::default())
        .manage(commands::EmulatorState::default())
        .manage(commands::remote_bridge::RemoteBridgeRuntimeState::default())
        .manage(commands::project_assets::ProjectAssetState::default())
        .register_asynchronous_uri_scheme_protocol(
            commands::project_assets::URI_SCHEME,
            commands::project_assets::handle,
        )
        .register_asynchronous_uri_scheme_protocol(
            commands::solana::URI_SCHEME,
            commands::solana::handle_uri_scheme,
        )
        .setup(|app| {
            commands::solana::toolchain::configure_tauri_roots(app.handle());
            window_state::configure_main_window(app.handle().clone());

            // Solana workbench state is rooted under Tauri's app-data dir.
            // This app is new, so we deliberately do not migrate any older
            // dirs::data_dir()/xero-solana-* locations.
            {
                use tauri::Manager;
                let app_handle = app.handle().clone();
                let desktop_state = app_handle.state::<state::DesktopState>();
                let solana_state = match desktop_state.app_data_dir(&app_handle) {
                    Ok(app_data_dir) => commands::SolanaState::with_app_data_dir(app_data_dir),
                    Err(error) => {
                        eprintln!(
                            "[solana] app-data root unavailable, using temporary fallback: {error}"
                        );
                        commands::SolanaState::default()
                    }
                };
                let _ = app.manage(solana_state);
            }

            // Configure the current global database path and tighten file-mode permissions on
            // app-data storage so credentials at rest are not world-readable on multi-user systems.
            {
                use tauri::Manager;
                let app_handle = app.handle().clone();
                let desktop_state = app_handle.state::<state::DesktopState>();
                if let Ok(app_data_dir) = desktop_state.app_data_dir(&app_handle) {
                    let global_db_path = desktop_state
                        .global_db_path(&app_handle)
                        .unwrap_or_else(|_| app_data_dir.join(global_db::GLOBAL_DATABASE_FILE_NAME));
                    db::configure_project_database_paths(&global_db_path);

                    if let Err(error) =
                        global_db::permissions::harden_global_paths(&app_data_dir, &global_db_path)
                    {
                        eprintln!("[storage] permission hardening skipped: {error}");
                    }
                }
            }

            // Bridge agent-usage updates from the provider loop (which has no
            // AppHandle in scope) to the frontend. The closure captures a
            // clone of the AppHandle and forwards each call to tauri::Emitter.
            {
                use tauri::Emitter;
                let app_handle = app.handle().clone();
                runtime::usage_events::set_usage_event_emitter(move |payload| {
                    let _ = app_handle.emit(
                        runtime::usage_events::AGENT_USAGE_UPDATED_EVENT,
                        &payload,
                    );
                });
            }

            {
                let app_handle = app.handle().clone();
                if let Err(error) =
                    commands::remote_bridge::start_remote_bridge_if_registered(&app_handle)
                {
                    eprintln!("[remote-bridge] startup skipped: {error}");
                }
            }

            // One-shot backfill: rows written before pricing was wired in (or
            // after a pricing-table update) need their cost recomputed. We
            // walk the registry and price every zero-cost row that has tokens.
            // Best-effort; failures are logged but never block boot.
            {
                use std::path::Path;
                use tauri::Manager;
                let app_handle = app.handle().clone();
                let desktop_state = app_handle.state::<state::DesktopState>();
                if let Ok(registry_path) = desktop_state.global_db_path(&app_handle) {
                    if let Ok(reg) = registry::read_registry(&registry_path) {
                        for record in reg.projects {
                            let root = Path::new(&record.root_path);
                            if !root.is_dir() {
                                continue;
                            }
                            let updated = runtime::pricing::backfill_agent_usage_costs(root);
                            if updated > 0 {
                                eprintln!(
                                    "[pricing] backfilled cost for {updated} agent_usage row(s) in {}",
                                    record.root_path
                                );
                            }

                            // Phase 6: harden the per-project state.db (and -wal/-shm
                            // sidecars) once we know which file the registry points to.
                            // Best-effort — we log and continue if chmod fails.
                            let project_db_path = db::database_path_for_repo(root);
                            if let Err(error) =
                                global_db::permissions::harden_project_database(&project_db_path)
                            {
                                eprintln!(
                                    "[storage] permission hardening skipped for {}: {error}",
                                    project_db_path.display()
                                );
                            }

                            match db::project_store::maintain_code_rollback_storage(
                                root,
                                &record.project_id,
                            ) {
                                Ok(report) => {
                                    for diagnostic in report
                                        .diagnostics
                                        .iter()
                                        .chain(report.prune.diagnostics.iter())
                                    {
                                        eprintln!(
                                            "[storage] code rollback diagnostic for {}: {} - {}",
                                            record.root_path, diagnostic.code, diagnostic.message
                                        );
                                    }
                                    if report.prune.pruned_blob_count > 0 {
                                        eprintln!(
                                            "[storage] pruned {} unreferenced code rollback blob(s) for {}",
                                            report.prune.pruned_blob_count, record.root_path
                                        );
                                    }
                                }
                                Err(error) => {
                                    eprintln!(
                                        "[storage] code rollback maintenance skipped for {}: {error}",
                                        record.root_path
                                    );
                                }
                            }
                        }
                    }
                }
            }

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
                commands::dictation::shutdown_on_close(window.app_handle());
                commands::emulator::shutdown::shutdown_on_close(window.app_handle());
                commands::remote_bridge::shutdown_on_close(window.app_handle());
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(updater_plugin())
        .invoke_handler(tauri::generate_handler![
            commands::import_repository::import_repository,
            commands::create_repository::create_repository,
            commands::platform::desktop_platform,
            commands::local_environment::get_launch_mode,
            commands::local_environment::get_local_environment_config,
            commands::local_environment::save_local_environment_config,
            commands::local_environment::regenerate_secret_key_base,
            commands::dock_icon::set_theme_dock_icon,
            commands::developer_tool_harness::developer_tool_catalog,
            commands::developer_tool_harness::developer_tool_dry_run,
            commands::developer_tool_harness::developer_tool_harness_project,
            commands::developer_tool_harness::developer_tool_model_run,
            commands::developer_tool_harness::developer_tool_sequence_delete,
            commands::developer_tool_harness::developer_tool_sequence_list,
            commands::developer_tool_harness::developer_tool_sequence_upsert,
            commands::developer_tool_harness::developer_tool_synthetic_run,
            commands::development_storage::developer_storage_overview,
            commands::development_storage::developer_storage_read_table,
            commands::list_projects::list_projects,
            commands::remove_project::remove_project,
            commands::wipe_data::wipe_project_data,
            commands::wipe_data::wipe_all_xero_data,
            commands::get_project_load_bundle::get_project_load_bundle,
            commands::project_state::create_project_state_backup,
            commands::project_state::list_project_state_backups,
            commands::project_state::restore_project_state_backup,
            commands::project_state::repair_project_state,
            commands::project_state::read_app_ui_state,
            commands::project_state::read_project_ui_state,
            commands::project_state::write_app_ui_state,
            commands::project_state::write_project_ui_state,
            commands::remote_bridge::bridge_status,
            commands::remote_bridge::bridge_sign_in,
            commands::remote_bridge::bridge_poll_github_login,
            commands::remote_bridge::bridge_sign_out,
            commands::remote_bridge::bridge_revoke_device,
            commands::remote_bridge::set_session_remote_visibility,
            commands::project_records::list_project_context_records,
            commands::project_records::delete_project_context_record,
            commands::project_records::supersede_project_context_record,
            commands::agent_definition::list_agent_definitions,
            commands::agent_definition::archive_agent_definition,
            commands::agent_definition::get_agent_definition_version,
            commands::agent_definition::get_agent_definition_version_diff,
            commands::agent_definition::preview_agent_definition,
            commands::agent_definition::save_agent_definition,
            commands::agent_definition::update_agent_definition,
            commands::agent_extensions::validate_agent_tool_extension_manifest,
            commands::workflow_agents::list_workflow_agents,
            commands::workflow_agents::get_workflow_agent_detail,
            commands::workflow_agents::get_workflow_agent_graph_projection,
            commands::workflow_agents::get_agent_authoring_catalog,
            commands::workflow_agents::search_agent_authoring_skills,
            commands::workflow_agents::resolve_agent_authoring_skill,
            commands::workflow_agents::get_agent_tool_pack_catalog,
            commands::agent_reports::get_agent_run_start_explanation,
            commands::agent_reports::get_agent_knowledge_inspection,
            commands::agent_reports::get_agent_handoff_context_summary,
            commands::agent_reports::get_agent_support_diagnostics_bundle,
            commands::agent_reports::get_capability_permission_explanation,
            commands::agent_reports::get_agent_database_touchpoint_explanation,
            commands::agent_session::create_agent_session,
            commands::agent_session::list_agent_sessions,
            commands::agent_session::get_agent_session,
            commands::agent_session::update_agent_session,
            commands::agent_session_title::auto_name_agent_session,
            commands::agent_session::archive_agent_session,
            commands::agent_session::restore_agent_session,
            commands::agent_session::delete_agent_session,
            commands::agent_task::start_agent_task,
            commands::agent_task::send_agent_message,
            commands::agent_task::cancel_agent_run,
            commands::agent_task::resume_agent_run,
            commands::agent_task::get_agent_run,
            commands::agent_task::export_agent_trace,
            commands::agent_task::list_agent_runs,
            commands::agent_task::subscribe_agent_stream,
            commands::session_history::get_session_transcript,
            commands::session_history::export_session_transcript,
            commands::session_history::save_session_transcript_export,
            commands::session_history::search_session_transcripts,
            commands::session_history::get_session_context_snapshot,
            commands::session_history::compact_session_history,
            commands::session_history::branch_agent_session,
            commands::session_history::rewind_agent_session,
            commands::session_history::list_session_memories,
            commands::session_history::get_session_memory_review_queue,
            commands::session_history::extract_session_memory_candidates,
            commands::session_history::update_session_memory,
            commands::session_history::correct_session_memory,
            commands::session_history::delete_session_memory,
            commands::get_project_snapshot::get_project_snapshot,
            commands::get_project_usage_summary::get_project_usage_summary,
            commands::get_repository_status::get_repository_status,
            commands::get_repository_diff::get_repository_diff,
            commands::code_rollback::apply_selective_undo,
            commands::code_rollback::apply_session_rollback,
            commands::git_operations::git_stage_paths,
            commands::git_operations::git_unstage_paths,
            commands::git_operations::git_discard_changes,
            commands::git_operations::git_revert_patch,
            commands::git_operations::git_commit,
            commands::git_commit_message::git_generate_commit_message,
            commands::git_operations::git_fetch,
            commands::git_operations::git_pull,
            commands::git_operations::git_push,
            commands::project_files::list_project_file_index,
            commands::project_files::list_project_files,
            commands::project_files::read_project_file,
            commands::project_files::write_project_file,
            commands::project_files::stat_project_files,
            commands::project_assets::revoke_project_asset_tokens,
            commands::project_files::open_project_file_external,
            commands::project_files::create_project_entry,
            commands::project_files::rename_project_entry,
            commands::project_files::move_project_entry,
            commands::project_files::delete_project_entry,
            commands::search_project::search_project,
            commands::search_project::replace_in_project,
            commands::editor_diagnostics::run_project_typecheck,
            commands::editor_workflows::format_project_document,
            commands::editor_workflows::run_project_lint,
            commands::workspace_index::workspace_index,
            commands::workspace_index::workspace_status,
            commands::workspace_index::workspace_query,
            commands::workspace_index::workspace_explain,
            commands::workspace_index::workspace_reset,
            commands::get_autonomous_run::get_autonomous_run,
            commands::get_runtime_run::get_runtime_run,
            commands::get_runtime_session::get_runtime_session,
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
            commands::project_runner::update_project_start_targets,
            commands::project_runner::suggest_project_start_targets,
            commands::project_runner::terminal_open,
            commands::project_runner::terminal_write,
            commands::project_runner::terminal_resize,
            commands::project_runner::terminal_close,
            commands::skills::update_github_skill_source,
            commands::skills::upsert_plugin_root,
            commands::skills::remove_plugin_root,
            commands::skills::set_plugin_enabled,
            commands::skills::remove_plugin,
            commands::provider_model_catalog::get_provider_model_catalog,
            commands::provider_preflight::preflight_provider_profile,
            commands::doctor_report::run_doctor_report,
            commands::environment_discovery::get_environment_discovery_status,
            commands::environment_discovery::get_environment_profile_summary,
            commands::environment_discovery::refresh_environment_discovery,
            commands::environment_discovery::resolve_environment_permission_requests,
            commands::environment_discovery::start_environment_discovery,
            commands::environment_user_tools::environment_verify_user_tool,
            commands::environment_user_tools::environment_save_user_tool,
            commands::environment_user_tools::environment_remove_user_tool,
            commands::provider_diagnostics::check_provider_profile,
            commands::provider_credentials::list_provider_credentials,
            commands::provider_credentials::upsert_provider_credential,
            commands::provider_credentials::delete_provider_credential,
            commands::start_openai_login::start_openai_login,
            commands::submit_openai_callback::submit_openai_callback,
            commands::start_oauth_login::start_oauth_login,
            commands::complete_oauth_callback::complete_oauth_callback,
            commands::logout_runtime_session::logout_runtime_session,
            commands::start_autonomous_run::start_autonomous_run,
            commands::stage_agent_attachment::stage_agent_attachment,
            commands::stage_agent_attachment::discard_agent_attachment,
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
            commands::record_notification_dispatch_outcome::record_notification_dispatch_outcome,
            commands::submit_notification_reply::submit_notification_reply,
            commands::sync_notification_adapters::sync_notification_adapters,
            commands::dictation::speech_dictation_status,
            commands::dictation::speech_dictation_settings,
            commands::dictation::speech_dictation_update_settings,
            commands::dictation::speech_dictation_start,
            commands::dictation::speech_dictation_stop,
            commands::dictation::speech_dictation_cancel,
            commands::soul_settings::soul_settings,
            commands::soul_settings::soul_update_settings,
            commands::agent_tooling_settings::agent_tooling_settings,
            commands::agent_tooling_settings::agent_tooling_update_settings,
            commands::browser::browser_show,
            commands::browser::browser_resize,
            commands::browser::browser_hide,
            commands::browser::settings::browser_control_settings,
            commands::browser::settings::browser_control_update_settings,
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
            commands::emulator::emulator_frame,
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
            commands::solana::solana_logs_view,
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

fn updater_plugin<R: tauri::Runtime + 'static>(
) -> tauri::plugin::TauriPlugin<R, tauri_plugin_updater::Config> {
    if std::any::TypeId::of::<R>() == std::any::TypeId::of::<tauri::test::MockRuntime>() {
        return tauri::plugin::Builder::<R, tauri_plugin_updater::Config>::new("updater").build();
    }

    tauri_plugin_updater::Builder::new().build()
}

pub fn configure_builder<R: tauri::Runtime + 'static>(
    builder: tauri::Builder<R>,
) -> tauri::Builder<R> {
    configure_builder_with_state(builder, state::DesktopState::default())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_builder(tauri::Builder::default())
        .build(tauri::generate_context!())
        .expect("error while building Xero desktop host")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            {
                if let tauri::RunEvent::Reopen {
                    has_visible_windows,
                    ..
                } = event
                {
                    if !has_visible_windows {
                        window_state::restore_main_window(app);
                    }
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                let _ = app;
                let _ = event;
            }
        });
}
