pub mod agent_definition;
pub mod agent_extensions;
pub mod agent_reports;
pub mod agent_session;
pub mod agent_session_title;
pub mod agent_task;
pub mod agent_tooling_settings;
pub mod backend_jobs;
pub mod browser;
pub mod cancel_autonomous_run;
pub mod code_rollback;
pub mod complete_oauth_callback;
pub mod create_repository;
pub mod developer_tool_harness;
pub mod development_storage;
pub mod dictation;
pub mod dock_icon;
pub mod doctor_report;
pub mod editor_diagnostics;
pub mod editor_workflows;
pub mod emulator;
pub mod environment_discovery;
pub mod environment_user_tools;
pub mod get_autonomous_run;
pub mod get_project_load_bundle;
pub mod get_project_snapshot;
pub mod get_project_usage_summary;
pub mod get_repository_diff;
pub mod get_repository_status;
pub mod get_runtime_run;
pub mod get_runtime_session;
pub mod get_runtime_settings;
pub mod git_commit_message;
pub mod git_operations;
pub mod import_mcp_servers;
pub mod import_repository;
pub mod list_mcp_servers;
pub mod list_notification_dispatches;
pub mod list_notification_routes;
pub mod list_projects;
pub mod local_environment;
pub mod logout_runtime_session;
pub mod payload_budget;
pub mod platform;
pub mod project_assets;
pub mod project_files;
pub mod project_records;
pub mod project_runner;
pub mod project_state;
pub mod provider_credentials;
pub mod provider_diagnostics;
pub mod provider_model_catalog;
pub mod provider_preflight;
pub mod record_notification_dispatch_outcome;
pub mod remove_mcp_server;
pub mod remove_project;
pub mod resolve_operator_action;
pub mod resume_operator_run;
pub mod search_project;
pub mod session_history;
pub mod skills;
pub mod solana;
pub mod soul_settings;
pub mod stage_agent_attachment;
pub mod start_autonomous_run;
pub mod start_oauth_login;
pub mod start_openai_login;
pub mod start_runtime_run;
pub mod start_runtime_session;
pub mod stop_runtime_run;
pub mod submit_notification_reply;
pub mod submit_openai_callback;
pub mod subscribe_runtime_stream;
pub mod sync_notification_adapters;
pub mod update_runtime_run_controls;
pub mod upsert_mcp_server;
pub mod upsert_notification_route;
pub mod upsert_notification_route_credentials;
pub mod workflow_agents;
pub mod workspace_index;

mod contracts;
pub(crate) mod runtime_support;

pub use agent_definition::{
    archive_agent_definition, get_agent_definition_version, get_agent_definition_version_diff,
    list_agent_definitions, preview_agent_definition, save_agent_definition,
    update_agent_definition,
};
pub use agent_extensions::{
    validate_agent_tool_extension_manifest, AgentToolExtensionManifestValidationDto,
    AgentToolExtensionPermissionSummaryDto, AgentToolExtensionValidationDiagnosticDto,
    ValidateAgentToolExtensionManifestRequestDto,
};
pub use agent_reports::{
    get_agent_database_touchpoint_explanation, get_agent_handoff_context_summary,
    get_agent_knowledge_inspection, get_agent_run_start_explanation,
    get_agent_support_diagnostics_bundle, get_capability_permission_explanation,
    GetAgentDatabaseTouchpointExplanationRequestDto, GetAgentHandoffContextSummaryRequestDto,
    GetAgentKnowledgeInspectionRequestDto, GetAgentRunStartExplanationRequestDto,
    GetAgentSupportDiagnosticsBundleRequestDto, GetCapabilityPermissionExplanationRequestDto,
};
pub use agent_session::{
    archive_agent_session, create_agent_session, delete_agent_session, get_agent_session,
    list_agent_sessions, restore_agent_session, update_agent_session,
};
pub use agent_session_title::auto_name_agent_session;
pub use agent_task::{
    cancel_agent_run, export_agent_trace, get_agent_run, list_agent_runs, resume_agent_run,
    send_agent_message, start_agent_task, subscribe_agent_stream,
};
pub use agent_tooling_settings::{
    agent_tooling_settings, agent_tooling_update_settings, AgentToolingModelOverrideDto,
    AgentToolingSettingsDto, UpsertAgentToolingModelOverrideRequestDto,
    UpsertAgentToolingSettingsRequestDto,
};
pub use browser::{
    browser_back, browser_click, browser_control_settings, browser_control_update_settings,
    browser_cookies_get, browser_cookies_set, browser_current_url, browser_eval, browser_forward,
    browser_hide, browser_history_state, browser_internal_event, browser_internal_reply,
    browser_navigate, browser_press_key, browser_query, browser_read_text, browser_reload,
    browser_resize, browser_screenshot, browser_scroll, browser_show, browser_stop,
    browser_storage_clear, browser_storage_read, browser_storage_write, browser_tab_close,
    browser_tab_focus, browser_tab_list, browser_type, browser_wait_for_load,
    browser_wait_for_selector, BrowserControlPreferenceDto, BrowserControlSettingsDto,
    BrowserState, BrowserTabMetadata, UpsertBrowserControlSettingsRequestDto,
    BROWSER_CONSOLE_EVENT, BROWSER_DIALOG_EVENT, BROWSER_DOWNLOAD_EVENT, BROWSER_LOAD_STATE_EVENT,
    BROWSER_TAB_PREFIX, BROWSER_TAB_UPDATED_EVENT, BROWSER_URL_CHANGED_EVENT,
};
pub use cancel_autonomous_run::cancel_autonomous_run;
pub use code_rollback::{apply_selective_undo, apply_session_rollback};
pub use complete_oauth_callback::complete_oauth_callback;
pub use create_repository::create_repository;
pub use developer_tool_harness::{
    developer_tool_catalog, developer_tool_dry_run, developer_tool_harness_project,
    developer_tool_model_run, developer_tool_sequence_delete, developer_tool_sequence_list,
    developer_tool_sequence_upsert, developer_tool_synthetic_run, HARNESS_FIXTURE_PROJECT_ID,
};
pub use development_storage::{developer_storage_overview, developer_storage_read_table};
pub use dictation::{
    speech_dictation_cancel, speech_dictation_settings, speech_dictation_start,
    speech_dictation_status, speech_dictation_stop, speech_dictation_update_settings,
    DictationState,
};
pub use dock_icon::set_theme_dock_icon;
pub use doctor_report::run_doctor_report;
pub use editor_diagnostics::{
    run_project_typecheck, EditorLspInstallCommandDto, EditorLspInstallSuggestionDto,
    EditorLspServerStatusDto, ProjectDiagnosticDto, ProjectDiagnosticSeverityDto,
    ProjectTypecheckResponseDto, ProjectTypecheckStatusDto, RunProjectTypecheckRequestDto,
};
pub use editor_workflows::{
    format_project_document, run_project_lint, FormatProjectDocumentRangeDto,
    FormatProjectDocumentRequestDto, FormatProjectDocumentResponseDto,
    FormatProjectDocumentStatusDto, ProjectLintResponseDto, ProjectLintStatusDto,
    RunProjectLintRequestDto,
};
pub use emulator::{
    emulator_android_provision, emulator_android_provision_status, emulator_input,
    emulator_ios_open_accessibility_settings, emulator_ios_request_ax_permission,
    emulator_list_devices, emulator_rotate, emulator_sdk_status, emulator_start, emulator_stop,
    emulator_subscribe_ready, EmulatorState,
};
pub use environment_discovery::{
    get_environment_discovery_status, get_environment_profile_summary,
    refresh_environment_discovery, resolve_environment_permission_requests,
    start_environment_discovery,
};
pub use environment_user_tools::{
    environment_remove_user_tool, environment_save_user_tool, environment_verify_user_tool,
};
pub use get_autonomous_run::get_autonomous_run;
pub use get_project_load_bundle::get_project_load_bundle;
pub use get_project_snapshot::get_project_snapshot;
pub use get_repository_diff::get_repository_diff;
pub use get_repository_status::get_repository_status;
pub use get_runtime_run::get_runtime_run;
pub use get_runtime_session::get_runtime_session;
pub use git_commit_message::git_generate_commit_message;
pub use git_operations::{
    git_commit, git_discard_changes, git_fetch, git_pull, git_push, git_revert_patch,
    git_stage_paths, git_unstage_paths,
};
pub use import_mcp_servers::import_mcp_servers;
pub use import_repository::import_repository;
pub use list_mcp_servers::{list_mcp_servers, refresh_mcp_server_statuses};
pub use list_notification_dispatches::list_notification_dispatches;
pub use list_notification_routes::list_notification_routes;
pub use list_projects::list_projects;
pub use local_environment::{
    get_launch_mode, get_local_environment_config, regenerate_secret_key_base,
    save_local_environment_config, LocalEnvironmentConfig, SaveLocalEnvironmentConfigRequest,
};
pub use logout_runtime_session::logout_runtime_session;
pub use platform::desktop_platform;
pub use project_assets::{revoke_project_asset_tokens, ProjectAssetState};
pub use project_files::{
    create_project_entry, delete_project_entry, list_project_file_index, list_project_files,
    move_project_entry, open_project_file_external, read_project_file, rename_project_entry,
    stat_project_files, write_project_file,
};
pub use project_records::{
    delete_project_context_record, list_project_context_records, supersede_project_context_record,
    DeleteProjectContextRecordRequestDto, DeleteProjectContextRecordResponseDto,
    ListProjectContextRecordsRequestDto, ListProjectContextRecordsResponseDto,
    ProjectContextRecordSummaryDto, SupersedeProjectContextRecordRequestDto,
    SupersedeProjectContextRecordResponseDto,
};
pub use project_runner::{
    suggest_project_start_targets, terminal_close, terminal_open, terminal_resize, terminal_write,
    update_project_start_targets, OpenTerminalRequestDto, OpenTerminalResponseDto,
    StartTargetInputDto, SuggestProjectStartTargetsRequestDto, SuggestedStartTargetDto,
    SuggestedStartTargetsDto, TerminalIdRequestDto, TerminalResizeRequestDto,
    TerminalWriteRequestDto, UpdateProjectStartTargetsRequestDto,
};
pub use project_state::{
    create_project_state_backup, list_project_state_backups, read_app_ui_state,
    read_project_ui_state, repair_project_state, restore_project_state_backup, write_app_ui_state,
    write_project_ui_state, AppUiStateResponseDto, CreateProjectStateBackupRequestDto,
    ListProjectStateBackupsRequestDto, ListProjectStateBackupsResponseDto,
    ProjectStateBackupListingEntryDto, ProjectStateBackupResponseDto,
    ProjectStateRepairDiagnosticDto, ProjectStateRepairResponseDto, ProjectStateRestoreResponseDto,
    ProjectUiStateResponseDto, ReadAppUiStateRequestDto, ReadProjectUiStateRequestDto,
    RepairProjectStateRequestDto, RestoreProjectStateBackupRequestDto, WriteAppUiStateRequestDto,
    WriteProjectUiStateRequestDto,
};
pub use provider_credentials::{
    delete_provider_credential, list_provider_credentials, upsert_provider_credential,
};
pub use provider_diagnostics::check_provider_profile;
pub use provider_model_catalog::get_provider_model_catalog;
pub use provider_preflight::preflight_provider_profile;
pub use record_notification_dispatch_outcome::record_notification_dispatch_outcome;
pub use remove_mcp_server::remove_mcp_server;
pub use remove_project::remove_project;
pub use resolve_operator_action::resolve_operator_action;
pub use resume_operator_run::resume_operator_run;
pub use search_project::{replace_in_project, search_project};
pub use session_history::{
    branch_agent_session, compact_session_history, correct_session_memory, delete_session_memory,
    export_session_transcript, extract_session_memory_candidates, get_session_context_snapshot,
    get_session_memory_review_queue, get_session_transcript, list_session_memories,
    rewind_agent_session, save_session_transcript_export, search_session_transcripts,
    update_session_memory,
};
pub use skills::{
    list_skill_registry, reload_skill_registry, remove_plugin, remove_plugin_root, remove_skill,
    remove_skill_local_root, set_plugin_enabled, set_skill_enabled, update_github_skill_source,
    update_project_skill_source, upsert_plugin_root, upsert_skill_local_root,
};
pub use solana::{
    solana_alt_create, solana_alt_extend, solana_alt_resolve, solana_cluster_drift_check,
    solana_cluster_drift_tracked_programs, solana_cluster_list, solana_cluster_start,
    solana_cluster_status, solana_cluster_stop, solana_codama_generate, solana_cost_record,
    solana_cost_reset, solana_cost_snapshot, solana_cpi_resolve, solana_doc_catalog,
    solana_doc_snippets, solana_idl_drift, solana_idl_fetch, solana_idl_get, solana_idl_load,
    solana_idl_publish, solana_idl_unwatch, solana_idl_watch, solana_indexer_run,
    solana_indexer_scaffold, solana_logs_active, solana_logs_recent, solana_logs_subscribe,
    solana_logs_unsubscribe, solana_logs_view, solana_pda_analyse_bump, solana_pda_derive,
    solana_pda_predict, solana_pda_scan, solana_persona_create, solana_persona_delete,
    solana_persona_export_keypair, solana_persona_fund, solana_persona_import_keypair,
    solana_persona_list, solana_persona_roles, solana_priority_fee_estimate, solana_program_build,
    solana_program_deploy, solana_program_rollback, solana_program_upgrade_check,
    solana_rpc_endpoints_set, solana_rpc_health, solana_scenario_list, solana_scenario_run,
    solana_secrets_patterns, solana_secrets_scan, solana_secrets_scope_check,
    solana_snapshot_create, solana_snapshot_delete, solana_snapshot_list, solana_snapshot_restore,
    solana_squads_proposal_create, solana_subscribe_ready, solana_toolchain_install,
    solana_toolchain_install_status, solana_toolchain_status, solana_tx_build, solana_tx_explain,
    solana_tx_send, solana_tx_simulate, solana_verified_build_submit, SolanaState,
};
pub use soul_settings::{
    soul_settings, soul_update_settings, SoulIdDto, SoulPresetDto, SoulSettingsDto,
    UpsertSoulSettingsRequestDto,
};
pub use stage_agent_attachment::{discard_agent_attachment, stage_agent_attachment};
pub use start_autonomous_run::start_autonomous_run;
pub use start_oauth_login::start_oauth_login;
pub use start_openai_login::start_openai_login;
pub use start_runtime_run::start_runtime_run;
pub use start_runtime_session::start_runtime_session;
pub use stop_runtime_run::stop_runtime_run;
pub use submit_notification_reply::submit_notification_reply;
pub use submit_openai_callback::submit_openai_callback;
pub use subscribe_runtime_stream::subscribe_runtime_stream;
pub use sync_notification_adapters::sync_notification_adapters;
pub use update_runtime_run_controls::update_runtime_run_controls;
pub use upsert_mcp_server::upsert_mcp_server;
pub use upsert_notification_route::upsert_notification_route;
pub use upsert_notification_route_credentials::upsert_notification_route_credentials;
pub use workflow_agents::{
    get_agent_authoring_catalog, get_agent_tool_pack_catalog, get_workflow_agent_detail,
    get_workflow_agent_graph_projection, list_workflow_agents, resolve_agent_authoring_skill,
    search_agent_authoring_skills,
};
pub use workspace_index::{
    workspace_explain, workspace_index, workspace_query, workspace_reset, workspace_status,
};

pub use crate::environment::service::EnvironmentDiscoveryStatus;
pub use contracts::{
    agent::*, autonomous::*, code_history::*, developer_tool_harness::*, dictation::*, error::*,
    mcp::*, notifications::*, runtime::*, session_context::*, skills::*, surface::*, usage::*,
    workflow::*, workflow_agents::*,
};

pub(crate) use contracts::{
    error::validate_non_empty,
    notifications::{
        map_notification_dispatch_record, map_notification_reply_claim_record,
        map_notification_route_credential_readiness, map_notification_route_record,
        parse_notification_route_kind,
    },
};
pub(crate) use soul_settings::{default_soul_settings, load_soul_settings, soul_prompt_fragment};
