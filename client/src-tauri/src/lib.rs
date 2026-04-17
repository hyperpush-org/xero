pub mod auth;
pub mod commands;
pub mod db;
pub mod notifications;
pub mod registry;
pub mod runtime;
pub mod state;

pub mod git {
    pub mod diff;
    pub mod repository;
    pub mod status;
}

pub fn configure_builder_with_state<R: tauri::Runtime>(
    builder: tauri::Builder<R>,
    desktop_state: state::DesktopState,
) -> tauri::Builder<R> {
    builder
        .manage(desktop_state)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::import_repository::import_repository,
            commands::list_projects::list_projects,
            commands::get_project_snapshot::get_project_snapshot,
            commands::get_repository_status::get_repository_status,
            commands::get_repository_diff::get_repository_diff,
            commands::get_runtime_run::get_runtime_run,
            commands::get_runtime_session::get_runtime_session,
            commands::start_openai_login::start_openai_login,
            commands::submit_openai_callback::submit_openai_callback,
            commands::logout_runtime_session::logout_runtime_session,
            commands::start_runtime_run::start_runtime_run,
            commands::start_runtime_session::start_runtime_session,
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
            commands::upsert_workflow_graph::upsert_workflow_graph,
            commands::apply_workflow_transition::apply_workflow_transition,
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
