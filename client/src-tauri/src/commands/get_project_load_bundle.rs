use std::path::Path;

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        get_project_snapshot::project_snapshot_record_for_project,
        get_runtime_session::reconcile_runtime_session, map_notification_dispatch_record,
        map_notification_route_credential_readiness, map_notification_route_record,
        validate_non_empty, CommandError, CommandResult, ProjectLoadBundleDiagnosticDto,
        ProjectLoadBundleDto, ProjectLoadBundleRequestDto, RuntimeRunDto,
    },
    db::project_store,
    git::status,
    notifications::{FileNotificationCredentialStore, NotificationRouteKind},
    state::DesktopState,
};

use super::runtime_support::{
    emit_runtime_run_updated_if_changed, load_persisted_runtime_run, load_runtime_session_status,
    runtime_run_dto_from_snapshot, runtime_run_status_from_persisted, sync_autonomous_run_state,
    AutonomousSyncIntent,
};

#[tauri::command]
pub async fn get_project_load_bundle<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectLoadBundleRequestDto,
) -> CommandResult<ProjectLoadBundleDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let jobs = state.backend_jobs().clone();
    let state = state.inner().clone();
    let job_project_id = request.project_id.clone();
    jobs.run_blocking_latest(
        "project-load-bundle",
        "project load bundle",
        move |cancellation| {
            cancellation.check_cancelled("project load bundle")?;
            get_project_load_bundle_blocking(app, state, request)
        },
    )
    .await
    .map_err(|error| {
        if error.code == "backend_job_stale_result" || error.code == "backend_job_cancelled" {
            CommandError::retryable(
                "project_load_bundle_superseded",
                format!(
                    "Xero skipped stale project load work for `{job_project_id}` because a newer project selection replaced it."
                ),
            )
        } else {
            error
        }
    })
}

fn get_project_load_bundle_blocking<R: Runtime>(
    app: AppHandle<R>,
    state: DesktopState,
    request: ProjectLoadBundleRequestDto,
) -> CommandResult<ProjectLoadBundleDto> {
    let project_id = request.project_id;
    let project_record = project_snapshot_record_for_project(&app, &state, &project_id)?;
    let project_snapshot = project_record.snapshot;
    let repo_root = project_record.repo_root;

    let mut diagnostics = Vec::new();

    let repository_status = section_result(
        "repositoryStatus",
        status::load_repository_status_from_root(&repo_root),
        &mut diagnostics,
    );

    let runtime_session = section_result(
        "runtimeSession",
        load_runtime_session_status(&state, &repo_root, &project_id)
            .and_then(|runtime| reconcile_runtime_session(&app, &state, &repo_root, runtime)),
        &mut diagnostics,
    );

    let selected_agent_session_id = project_snapshot
        .agent_sessions
        .iter()
        .find(|session| session.selected)
        .map(|session| session.agent_session_id.clone());

    let (runtime_run, autonomous_run) = if let Some(agent_session_id) = selected_agent_session_id {
        let before = load_persisted_runtime_run(&repo_root, &project_id, &agent_session_id);
        let after = match &before {
            Ok(before) => section_result(
                "runtimeRun",
                Ok(runtime_run_status_from_persisted(before)),
                &mut diagnostics,
            ),
            Err(error) => {
                diagnostics.push(bundle_diagnostic("runtimeRun", error));
                None
            }
        };
        if let (Ok(before), Some(after)) = (&before, &after) {
            if let Err(error) = emit_runtime_run_updated_if_changed(
                &app,
                &project_id,
                &agent_session_id,
                before,
                after,
            ) {
                diagnostics.push(bundle_diagnostic("runtimeRun", &error));
            }
        }
        let runtime_run: Option<RuntimeRunDto> = after
            .as_ref()
            .and_then(|run| run.as_ref().map(runtime_run_dto_from_snapshot));
        let autonomous_run = section_result(
            "autonomousRun",
            sync_autonomous_run_state(
                &repo_root,
                &project_id,
                &agent_session_id,
                after.as_ref().and_then(|run| run.as_ref()),
                AutonomousSyncIntent::Observe,
            ),
            &mut diagnostics,
        );
        (runtime_run, autonomous_run)
    } else {
        (None, None)
    };

    let notification_dispatches = section_result(
        "notificationDispatches",
        project_store::load_notification_dispatches(&repo_root, &project_id, None).map(
            |dispatches| {
                dispatches
                    .into_iter()
                    .map(map_notification_dispatch_record)
                    .collect::<Vec<_>>()
            },
        ),
        &mut diagnostics,
    )
    .unwrap_or_default();

    let notification_routes = if request.include_notification_routes {
        section_result(
            "notificationRoutes",
            load_notification_routes(&app, &state, &repo_root, &project_id),
            &mut diagnostics,
        )
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(ProjectLoadBundleDto {
        project_id,
        project_snapshot,
        repository_status,
        runtime_session,
        runtime_run,
        autonomous_run,
        notification_dispatches,
        notification_routes,
        diagnostics,
    })
}

fn section_result<T>(
    section: &'static str,
    result: CommandResult<T>,
    diagnostics: &mut Vec<ProjectLoadBundleDiagnosticDto>,
) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            diagnostics.push(bundle_diagnostic(section, &error));
            None
        }
    }
}

fn bundle_diagnostic(
    section: &'static str,
    error: &CommandError,
) -> ProjectLoadBundleDiagnosticDto {
    ProjectLoadBundleDiagnosticDto {
        section: section.into(),
        code: error.code.clone(),
        message: error.message.clone(),
        retryable: error.retryable,
    }
}

fn load_notification_routes<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<Vec<crate::commands::NotificationRouteDto>> {
    let routes = project_store::load_notification_routes(repo_root, project_id)?;
    let credential_store_path = state.global_db_path(app)?;
    let credential_store = FileNotificationCredentialStore::new(credential_store_path);
    let readiness_projector = credential_store.load_readiness_projector();

    routes
        .into_iter()
        .map(|route| {
            let mut mapped_route = map_notification_route_record(route, None)?;
            let route_kind = match mapped_route.route_kind {
                crate::commands::NotificationRouteKindDto::Telegram => {
                    NotificationRouteKind::Telegram
                }
                crate::commands::NotificationRouteKindDto::Discord => {
                    NotificationRouteKind::Discord
                }
            };
            let readiness = readiness_projector.project_route(
                &mapped_route.project_id,
                &mapped_route.route_id,
                route_kind,
            );
            mapped_route.credential_readiness =
                Some(map_notification_route_credential_readiness(readiness));
            Ok(mapped_route)
        })
        .collect()
}
