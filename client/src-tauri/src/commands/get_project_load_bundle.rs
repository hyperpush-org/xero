use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        get_project_snapshot::project_snapshot_record_for_project,
        get_runtime_session::reconcile_runtime_session, validate_non_empty, AgentSessionDto,
        AgentSessionStatusDto, CommandError, CommandResult, ProjectLoadBundleDiagnosticDto,
        ProjectLoadBundleDto, ProjectLoadBundleRequestDto, RuntimeRunDto,
    },
    git::status,
    state::DesktopState,
};

use super::global_computer_use::GLOBAL_COMPUTER_USE_PROJECT_ID;
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
            let _perf = crate::perf::PerfSpan::new("project_load_bundle")
                .field("projectId", request.project_id.clone());
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

    let is_global_computer_use = project_id == GLOBAL_COMPUTER_USE_PROJECT_ID;
    let repository_status = if is_global_computer_use {
        None
    } else {
        section_result(
            "repositoryStatus",
            status::load_repository_status_from_root(&repo_root),
            &mut diagnostics,
        )
    };

    let runtime_session = section_result(
        "runtimeSession",
        load_runtime_session_status(&state, &repo_root, &project_id)
            .and_then(|runtime| reconcile_runtime_session(&app, &state, &repo_root, runtime)),
        &mut diagnostics,
    );

    let selected_agent_session_id =
        resolve_selected_agent_session_id(&project_snapshot.agent_sessions);

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

    Ok(ProjectLoadBundleDto {
        project_id,
        project_snapshot,
        repository_status,
        runtime_session,
        runtime_run,
        autonomous_run,
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

fn resolve_selected_agent_session_id(agent_sessions: &[AgentSessionDto]) -> Option<String> {
    agent_sessions
        .iter()
        .find(|session| session.selected && session.status == AgentSessionStatusDto::Active)
        .or_else(|| {
            agent_sessions
                .iter()
                .find(|session| session.status == AgentSessionStatusDto::Active)
        })
        .map(|session| session.agent_session_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(
        agent_session_id: &str,
        selected: bool,
        status: AgentSessionStatusDto,
    ) -> AgentSessionDto {
        AgentSessionDto {
            project_id: "project-1".into(),
            agent_session_id: agent_session_id.into(),
            title: agent_session_id.into(),
            summary: String::new(),
            session_kind: crate::commands::AgentSessionKindDto::Standard,
            status: status.clone(),
            selected,
            remote_visible: false,
            created_at: "2026-05-19T00:00:00Z".into(),
            updated_at: "2026-05-19T00:00:00Z".into(),
            archived_at: (status == AgentSessionStatusDto::Archived)
                .then(|| "2026-05-19T00:00:00Z".into()),
            last_run_id: None,
            last_runtime_kind: None,
            last_provider_id: None,
            lineage: None,
        }
    }

    #[test]
    fn selected_agent_session_id_prefers_selected_active_session() {
        let sessions = vec![
            session("session-a", false, AgentSessionStatusDto::Active),
            session("session-b", true, AgentSessionStatusDto::Active),
        ];

        assert_eq!(
            resolve_selected_agent_session_id(&sessions),
            Some("session-b".into())
        );
    }

    #[test]
    fn selected_agent_session_id_falls_back_to_first_active_session() {
        let sessions = vec![
            session("session-a", false, AgentSessionStatusDto::Active),
            session("session-b", false, AgentSessionStatusDto::Active),
        ];

        assert_eq!(
            resolve_selected_agent_session_id(&sessions),
            Some("session-a".into())
        );
    }

    #[test]
    fn selected_agent_session_id_ignores_archived_selected_session() {
        let sessions = vec![
            session("session-archived", true, AgentSessionStatusDto::Archived),
            session("session-active", false, AgentSessionStatusDto::Active),
        ];

        assert_eq!(
            resolve_selected_agent_session_id(&sessions),
            Some("session-active".into())
        );
    }
}
