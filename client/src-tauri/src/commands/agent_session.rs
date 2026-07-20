use std::path::Path;

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, AgentSessionDto, AgentSessionKindDto,
        AgentSessionLineageBoundaryKindDto, AgentSessionLineageDiagnosticDto,
        AgentSessionLineageDto, AgentSessionStatusDto, ArchiveAgentSessionRequestDto, CommandError,
        CommandResult, CreateAgentSessionRequestDto, DeleteAgentSessionRequestDto,
        GetAgentSessionRequestDto, ListAgentSessionsRequestDto, ListAgentSessionsResponseDto,
        RestoreAgentSessionRequestDto, RuntimeAgentIdDto, UpdateAgentSessionRequestDto,
    },
    db::project_store::{
        self, AgentSessionCreateRecord, AgentSessionKind, AgentSessionLineageBoundaryKind,
        AgentSessionLineageRecord, AgentSessionRecord, AgentSessionStatus,
        AgentSessionUpdateRecord, COMPUTER_USE_AGENT_SESSION_TITLE, DEFAULT_AGENT_SESSION_TITLE,
    },
    state::DesktopState,
};

use super::global_computer_use::GLOBAL_COMPUTER_USE_PROJECT_ID;
use super::remote_bridge::{
    handle_deleted_agent_session_remote_state, publish_agent_session_remote_state,
    RemoteBridgeRuntimeState,
};
use super::runtime_support::{
    emit_runtime_run_updated_if_changed, load_persisted_runtime_run, resolve_project_root,
    stop_owned_runtime_run,
};

#[tauri::command]
pub fn create_agent_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CreateAgentSessionRequestDto,
) -> CommandResult<AgentSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    if let Some(title) = request.title.as_deref() {
        validate_non_empty(title, "title")?;
    }

    let project_id = request.project_id.clone();
    let repo_root = resolve_project_root(&app, state.inner(), &project_id)?;
    let session_kind = create_request_session_kind(&request)?;
    let session = project_store::create_agent_session(
        &repo_root,
        &AgentSessionCreateRecord {
            project_id: request.project_id,
            title: request.title.unwrap_or_else(|| match session_kind {
                AgentSessionKind::Standard => DEFAULT_AGENT_SESSION_TITLE.to_string(),
                AgentSessionKind::ComputerUse => COMPUTER_USE_AGENT_SESSION_TITLE.to_string(),
            }),
            summary: request.summary,
            selected: request.selected,
            session_kind,
        },
    )?;
    publish_agent_session_remote_state(&app, state.inner(), &project_id, &session);

    Ok(agent_session_dto(&session))
}

fn create_request_session_kind(
    request: &CreateAgentSessionRequestDto,
) -> CommandResult<AgentSessionKind> {
    let inferred = match (request.session_kind, request.runtime_agent_id) {
        (Some(AgentSessionKindDto::ComputerUse), _) => AgentSessionKind::ComputerUse,
        (Some(AgentSessionKindDto::Standard), _) => AgentSessionKind::Standard,
        (None, Some(RuntimeAgentIdDto::ComputerUse)) => AgentSessionKind::ComputerUse,
        (None, _) => AgentSessionKind::Standard,
    };

    if inferred == AgentSessionKind::ComputerUse
        && request
            .runtime_agent_id
            .is_some_and(|agent_id| agent_id != RuntimeAgentIdDto::ComputerUse)
    {
        return Err(CommandError::user_fixable(
            "computer_use_agent_required",
            "Computer Use sessions must start with the Computer Use agent.",
        ));
    }

    if inferred == AgentSessionKind::Standard
        && request.runtime_agent_id == Some(RuntimeAgentIdDto::ComputerUse)
    {
        return Err(CommandError::user_fixable(
            "computer_use_session_required",
            "The Computer Use agent can only be selected for Computer Use sessions.",
        ));
    }

    Ok(inferred)
}

#[tauri::command]
pub fn list_agent_sessions<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListAgentSessionsRequestDto,
) -> CommandResult<ListAgentSessionsResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let sessions = project_store::list_agent_sessions(
        &repo_root,
        &request.project_id,
        request.include_archived,
    )?;
    Ok(ListAgentSessionsResponseDto {
        sessions: sessions
            .iter()
            .filter(|session| {
                request.project_id == GLOBAL_COMPUTER_USE_PROJECT_ID
                    || !matches!(session.session_kind, AgentSessionKind::ComputerUse)
            })
            .map(agent_session_dto)
            .collect(),
    })
}

#[tauri::command]
pub fn get_agent_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetAgentSessionRequestDto,
) -> CommandResult<Option<AgentSessionDto>> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let session = project_store::get_agent_session(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?;
    Ok(session.as_ref().map(agent_session_dto))
}

#[tauri::command]
pub fn update_agent_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateAgentSessionRequestDto,
) -> CommandResult<AgentSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    if let Some(title) = request.title.as_deref() {
        validate_non_empty(title, "title")?;
    }

    let project_id = request.project_id.clone();
    let repo_root = resolve_project_root(&app, state.inner(), &project_id)?;
    let session = project_store::update_agent_session(
        &repo_root,
        &AgentSessionUpdateRecord {
            project_id: request.project_id,
            agent_session_id: request.agent_session_id,
            title: request.title,
            summary: request.summary,
            selected: request.selected,
        },
    )?;
    publish_agent_session_remote_state(&app, state.inner(), &project_id, &session);
    Ok(agent_session_dto(&session))
}

#[tauri::command]
pub fn archive_agent_session<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    remote_state: State<'_, RemoteBridgeRuntimeState>,
    request: ArchiveAgentSessionRequestDto,
) -> CommandResult<AgentSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    stop_idle_owned_runtime_run_before_archive(
        &app,
        state.inner(),
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?;
    let session = project_store::archive_agent_session(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?;
    handle_deleted_agent_session_remote_state(
        &app,
        state.inner(),
        remote_state.inner(),
        &request.project_id,
        &session,
    );
    Ok(agent_session_dto(&session))
}

pub(crate) fn stop_idle_owned_runtime_run_before_archive<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> CommandResult<()> {
    const MAX_STOP_ATTEMPTS: usize = 3;
    for _ in 0..MAX_STOP_ATTEMPTS {
        let before = load_persisted_runtime_run(repo_root, project_id, agent_session_id)?;
        let Some(snapshot) = before.as_ref() else {
            return Ok(());
        };

        if snapshot.run.supervisor_kind != crate::runtime::OWNED_AGENT_SUPERVISOR_KIND
            || !matches!(
                snapshot.run.status,
                project_store::RuntimeRunStatus::Starting
                    | project_store::RuntimeRunStatus::Running
                    | project_store::RuntimeRunStatus::Stale
            )
        {
            return Ok(());
        }

        if state
            .agent_run_supervisor()
            .is_active(&snapshot.run.run_id)?
        {
            return Ok(());
        }

        match stop_owned_runtime_run(repo_root, snapshot) {
            Ok(after) => {
                emit_runtime_run_updated_if_changed(
                    app,
                    project_id,
                    agent_session_id,
                    &before,
                    &Some(after),
                )?;
                return Ok(());
            }
            Err(error) if error.code == "runtime_run_write_conflict" => continue,
            Err(error) => return Err(error),
        }
    }

    Err(CommandError::retryable(
        "runtime_run_write_conflict",
        "Xero could not stop the idle owned runtime before archiving because its durable projection kept changing. Refresh and retry.",
    ))
}

#[tauri::command]
pub fn restore_agent_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RestoreAgentSessionRequestDto,
) -> CommandResult<AgentSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    let project_id = request.project_id.clone();
    let repo_root = resolve_project_root(&app, state.inner(), &project_id)?;
    let session = project_store::restore_agent_session(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?;
    publish_agent_session_remote_state(&app, state.inner(), &project_id, &session);
    Ok(agent_session_dto(&session))
}

#[tauri::command]
pub fn delete_agent_session<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    remote_state: State<'_, RemoteBridgeRuntimeState>,
    request: DeleteAgentSessionRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let deleted_session = project_store::get_agent_session(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?;
    project_store::delete_agent_session(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?;
    if let Some(session) = deleted_session.as_ref() {
        handle_deleted_agent_session_remote_state(
            &app,
            state.inner(),
            remote_state.inner(),
            &request.project_id,
            session,
        );
    }
    Ok(())
}

pub(crate) fn agent_session_dto(record: &AgentSessionRecord) -> AgentSessionDto {
    AgentSessionDto {
        project_id: record.project_id.clone(),
        agent_session_id: record.agent_session_id.clone(),
        session_kind: match record.session_kind {
            AgentSessionKind::Standard => AgentSessionKindDto::Standard,
            AgentSessionKind::ComputerUse => AgentSessionKindDto::ComputerUse,
        },
        title: record.title.clone(),
        summary: record.summary.clone(),
        status: match record.status {
            AgentSessionStatus::Active => AgentSessionStatusDto::Active,
            AgentSessionStatus::Archived => AgentSessionStatusDto::Archived,
        },
        selected: record.selected,
        remote_visible: !matches!(record.status, AgentSessionStatus::Archived)
            && !matches!(record.session_kind, AgentSessionKind::ComputerUse),
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
        archived_at: record.archived_at.clone(),
        last_run_id: record.last_run_id.clone(),
        last_runtime_kind: record.last_runtime_kind.clone(),
        last_provider_id: record.last_provider_id.clone(),
        lineage: record.lineage.as_ref().map(agent_session_lineage_dto),
    }
}

pub(crate) fn agent_session_lineage_dto(
    record: &AgentSessionLineageRecord,
) -> AgentSessionLineageDto {
    AgentSessionLineageDto {
        lineage_id: record.lineage_id.clone(),
        project_id: record.project_id.clone(),
        child_agent_session_id: record.child_agent_session_id.clone(),
        source_agent_session_id: record.source_agent_session_id.clone(),
        source_run_id: record.source_run_id.clone(),
        source_boundary_kind: match &record.source_boundary_kind {
            AgentSessionLineageBoundaryKind::Run => AgentSessionLineageBoundaryKindDto::Run,
            AgentSessionLineageBoundaryKind::Message => AgentSessionLineageBoundaryKindDto::Message,
            AgentSessionLineageBoundaryKind::Checkpoint => {
                AgentSessionLineageBoundaryKindDto::Checkpoint
            }
        },
        source_message_id: record.source_message_id,
        source_checkpoint_id: record.source_checkpoint_id,
        source_compaction_id: record.source_compaction_id.clone(),
        source_title: record.source_title.clone(),
        branch_title: record.branch_title.clone(),
        replay_run_id: record.replay_run_id.clone(),
        file_change_summary: record.file_change_summary.clone(),
        diagnostic: record
            .diagnostic
            .as_ref()
            .map(|diagnostic| AgentSessionLineageDiagnosticDto {
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
            }),
        created_at: record.created_at.clone(),
        source_deleted_at: record.source_deleted_at.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tauri::Manager;

    use super::*;
    use crate::{
        db::{self, project_store::AgentSessionLineageDiagnosticRecord},
        git::repository::CanonicalRepository,
        registry::RegistryProjectRecord,
    };

    fn create_request(
        session_kind: Option<AgentSessionKindDto>,
        runtime_agent_id: Option<RuntimeAgentIdDto>,
    ) -> CreateAgentSessionRequestDto {
        CreateAgentSessionRequestDto {
            project_id: "project".into(),
            title: None,
            summary: String::new(),
            selected: false,
            session_kind,
            runtime_agent_id,
        }
    }

    fn lineage(boundary: AgentSessionLineageBoundaryKind) -> AgentSessionLineageRecord {
        AgentSessionLineageRecord {
            lineage_id: "lineage-1".into(),
            project_id: "project".into(),
            child_agent_session_id: "child".into(),
            source_agent_session_id: Some("source".into()),
            source_run_id: Some("run-1".into()),
            source_boundary_kind: boundary,
            source_message_id: Some(7),
            source_checkpoint_id: Some(9),
            source_compaction_id: Some("compaction-1".into()),
            source_title: "Source".into(),
            branch_title: "Branch".into(),
            replay_run_id: "replay-1".into(),
            file_change_summary: "one file".into(),
            diagnostic: Some(AgentSessionLineageDiagnosticRecord {
                code: "partial_replay".into(),
                message: "Replay omitted an unavailable event".into(),
            }),
            created_at: "2026-07-17T00:00:00Z".into(),
            source_deleted_at: Some("2026-07-18T00:00:00Z".into()),
        }
    }

    fn session(
        kind: AgentSessionKind,
        status: AgentSessionStatus,
        lineage: Option<AgentSessionLineageRecord>,
    ) -> AgentSessionRecord {
        AgentSessionRecord {
            project_id: "project".into(),
            agent_session_id: "session".into(),
            session_kind: kind,
            title: "Title".into(),
            summary: "Summary".into(),
            status,
            selected: true,
            remote_visible: false,
            created_at: "2026-07-17T00:00:00Z".into(),
            updated_at: "2026-07-17T01:00:00Z".into(),
            archived_at: None,
            last_run_id: Some("run-1".into()),
            last_runtime_kind: Some("owned_agent".into()),
            last_provider_id: Some("openai".into()),
            lineage,
        }
    }

    #[test]
    fn create_request_infers_compatible_session_kinds() {
        for (session_kind, runtime_agent_id, expected) in [
            (None, None, AgentSessionKind::Standard),
            (
                None,
                Some(RuntimeAgentIdDto::ComputerUse),
                AgentSessionKind::ComputerUse,
            ),
            (
                Some(AgentSessionKindDto::ComputerUse),
                None,
                AgentSessionKind::ComputerUse,
            ),
            (
                Some(AgentSessionKindDto::ComputerUse),
                Some(RuntimeAgentIdDto::ComputerUse),
                AgentSessionKind::ComputerUse,
            ),
            (
                Some(AgentSessionKindDto::Standard),
                Some(RuntimeAgentIdDto::Engineer),
                AgentSessionKind::Standard,
            ),
        ] {
            assert_eq!(
                create_request_session_kind(&create_request(session_kind, runtime_agent_id))
                    .expect("compatible request"),
                expected
            );
        }
    }

    #[test]
    fn create_request_rejects_cross_kind_agents() {
        let error = create_request_session_kind(&create_request(
            Some(AgentSessionKindDto::ComputerUse),
            Some(RuntimeAgentIdDto::Engineer),
        ))
        .expect_err("standard agent cannot start computer-use session");
        assert_eq!(error.code, "computer_use_agent_required");

        let error = create_request_session_kind(&create_request(
            Some(AgentSessionKindDto::Standard),
            Some(RuntimeAgentIdDto::ComputerUse),
        ))
        .expect_err("computer-use agent cannot start standard session");
        assert_eq!(error.code, "computer_use_session_required");
    }

    #[test]
    fn session_dto_projects_visibility_status_and_lineage() {
        let active = agent_session_dto(&session(
            AgentSessionKind::Standard,
            AgentSessionStatus::Active,
            Some(lineage(AgentSessionLineageBoundaryKind::Message)),
        ));
        assert_eq!(active.session_kind, AgentSessionKindDto::Standard);
        assert_eq!(active.status, AgentSessionStatusDto::Active);
        assert!(active.remote_visible);
        assert_eq!(
            active
                .lineage
                .as_ref()
                .map(|lineage| &lineage.source_boundary_kind),
            Some(&AgentSessionLineageBoundaryKindDto::Message)
        );
        assert_eq!(
            active
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.diagnostic.as_ref())
                .map(|diagnostic| diagnostic.code.as_str()),
            Some("partial_replay")
        );

        let archived = agent_session_dto(&session(
            AgentSessionKind::Standard,
            AgentSessionStatus::Archived,
            None,
        ));
        assert_eq!(archived.status, AgentSessionStatusDto::Archived);
        assert!(!archived.remote_visible);

        let computer_use = agent_session_dto(&session(
            AgentSessionKind::ComputerUse,
            AgentSessionStatus::Active,
            None,
        ));
        assert_eq!(computer_use.session_kind, AgentSessionKindDto::ComputerUse);
        assert!(!computer_use.remote_visible);
    }

    #[test]
    fn lineage_dto_maps_every_boundary_kind() {
        for (boundary, expected) in [
            (
                AgentSessionLineageBoundaryKind::Run,
                AgentSessionLineageBoundaryKindDto::Run,
            ),
            (
                AgentSessionLineageBoundaryKind::Message,
                AgentSessionLineageBoundaryKindDto::Message,
            ),
            (
                AgentSessionLineageBoundaryKind::Checkpoint,
                AgentSessionLineageBoundaryKindDto::Checkpoint,
            ),
        ] {
            let dto = agent_session_lineage_dto(&lineage(boundary));
            assert_eq!(dto.source_boundary_kind, expected);
            assert_eq!(dto.source_message_id, Some(7));
            assert_eq!(dto.source_checkpoint_id, Some(9));
            assert_eq!(
                dto.source_deleted_at.as_deref(),
                Some("2026-07-18T00:00:00Z")
            );
        }
    }

    #[test]
    fn agent_session_command_fixture_covers_crud_archive_restore_filtering_and_validation() {
        let fixture = tempfile::tempdir().expect("session command fixture");
        let repo_root = fixture.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create session repository");
        let repo_root = fs::canonicalize(repo_root).expect("canonical session repository");
        let project_id = "project-session-commands";
        let registry_path = fixture.path().join("app-data/global.db");
        let state = DesktopState::default().with_global_db_path_override(registry_path.clone());
        let app = crate::configure_builder_with_state(tauri::test::mock_builder(), state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build session command app");
        db::configure_project_database_paths(&registry_path);
        let repository = CanonicalRepository {
            project_id: project_id.into(),
            repository_id: "repository-session-commands".into(),
            root_path: repo_root.clone(),
            root_path_string: repo_root.to_string_lossy().into_owned(),
            common_git_dir: repo_root.join(".git"),
            display_name: "Session commands".into(),
            branch_name: Some("main".into()),
            head_sha: Some("abc123".into()),
            branch: None,
            last_commit: None,
            status_entries: Vec::new(),
            has_staged_changes: false,
            has_unstaged_changes: false,
            has_untracked_changes: false,
            additions: 0,
            deletions: 0,
        };
        db::import_project(
            &repository,
            app.state::<DesktopState>().import_failpoints(),
        )
        .expect("import session command project");
        crate::registry::replace_projects(
            &registry_path,
            vec![RegistryProjectRecord {
                project_id: project_id.into(),
                repository_id: repository.repository_id,
                root_path: repo_root.to_string_lossy().into_owned(),
                is_git_repo: true,
            }],
        )
        .expect("seed session command registry");

        assert_eq!(
            create_agent_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                CreateAgentSessionRequestDto {
                    project_id: " ".into(),
                    title: None,
                    summary: String::new(),
                    selected: false,
                    session_kind: None,
                    runtime_agent_id: None,
                },
            )
            .expect_err("blank project is invalid")
            .code,
            "invalid_request"
        );
        assert_eq!(
            create_agent_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                CreateAgentSessionRequestDto {
                    project_id: project_id.into(),
                    title: Some(" ".into()),
                    summary: String::new(),
                    selected: false,
                    session_kind: None,
                    runtime_agent_id: None,
                },
            )
            .expect_err("blank title is invalid")
            .code,
            "invalid_request"
        );

        let standard = create_agent_session(
            app.handle().clone(),
            app.state::<DesktopState>(),
            CreateAgentSessionRequestDto {
                project_id: project_id.into(),
                title: Some("Lifecycle fixture".into()),
                summary: "Initial summary".into(),
                selected: true,
                session_kind: Some(AgentSessionKindDto::Standard),
                runtime_agent_id: Some(RuntimeAgentIdDto::Engineer),
            },
        )
        .expect("create standard session");
        let computer_use = create_agent_session(
            app.handle().clone(),
            app.state::<DesktopState>(),
            CreateAgentSessionRequestDto {
                project_id: project_id.into(),
                title: None,
                summary: String::new(),
                selected: false,
                session_kind: None,
                runtime_agent_id: Some(RuntimeAgentIdDto::ComputerUse),
            },
        )
        .expect("create computer-use session");
        assert_eq!(computer_use.session_kind, AgentSessionKindDto::ComputerUse);
        assert_eq!(computer_use.title, COMPUTER_USE_AGENT_SESSION_TITLE);

        let listed = list_agent_sessions(
            app.handle().clone(),
            app.state::<DesktopState>(),
            ListAgentSessionsRequestDto {
                project_id: project_id.into(),
                include_archived: false,
            },
        )
        .expect("list visible sessions");
        assert!(listed
            .sessions
            .iter()
            .any(|session| session.agent_session_id == standard.agent_session_id));
        assert!(!listed
            .sessions
            .iter()
            .any(|session| session.agent_session_id == computer_use.agent_session_id));

        let fetched = get_agent_session(
            app.handle().clone(),
            app.state::<DesktopState>(),
            GetAgentSessionRequestDto {
                project_id: project_id.into(),
                agent_session_id: standard.agent_session_id.clone(),
            },
        )
        .expect("get standard session")
        .expect("standard session exists");
        assert_eq!(fetched.title, "Lifecycle fixture");

        let updated = update_agent_session(
            app.handle().clone(),
            app.state::<DesktopState>(),
            UpdateAgentSessionRequestDto {
                project_id: project_id.into(),
                agent_session_id: standard.agent_session_id.clone(),
                title: Some("Updated lifecycle".into()),
                summary: Some("Updated summary".into()),
                selected: Some(false),
            },
        )
        .expect("update standard session");
        assert_eq!(updated.title, "Updated lifecycle");
        assert_eq!(updated.summary, "Updated summary");
        assert!(!updated.selected);

        let archived = archive_agent_session(
            app.handle().clone(),
            app.state::<DesktopState>(),
            app.state::<RemoteBridgeRuntimeState>(),
            ArchiveAgentSessionRequestDto {
                project_id: project_id.into(),
                agent_session_id: standard.agent_session_id.clone(),
            },
        )
        .expect("archive standard session");
        assert_eq!(archived.status, AgentSessionStatusDto::Archived);
        assert!(!archived.remote_visible);

        let restored = restore_agent_session(
            app.handle().clone(),
            app.state::<DesktopState>(),
            RestoreAgentSessionRequestDto {
                project_id: project_id.into(),
                agent_session_id: standard.agent_session_id.clone(),
            },
        )
        .expect("restore standard session");
        assert_eq!(restored.status, AgentSessionStatusDto::Active);

        assert_eq!(
            delete_agent_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                app.state::<RemoteBridgeRuntimeState>(),
                DeleteAgentSessionRequestDto {
                    project_id: project_id.into(),
                    agent_session_id: standard.agent_session_id.clone(),
                },
            )
            .expect_err("active nonblank session cannot be deleted")
            .code,
            "agent_session_not_archived"
        );
        archive_agent_session(
            app.handle().clone(),
            app.state::<DesktopState>(),
            app.state::<RemoteBridgeRuntimeState>(),
            ArchiveAgentSessionRequestDto {
                project_id: project_id.into(),
                agent_session_id: standard.agent_session_id.clone(),
            },
        )
        .expect("re-archive standard session");
        delete_agent_session(
            app.handle().clone(),
            app.state::<DesktopState>(),
            app.state::<RemoteBridgeRuntimeState>(),
            DeleteAgentSessionRequestDto {
                project_id: project_id.into(),
                agent_session_id: standard.agent_session_id.clone(),
            },
        )
        .expect("delete standard session");
        assert!(get_agent_session(
            app.handle().clone(),
            app.state::<DesktopState>(),
            GetAgentSessionRequestDto {
                project_id: project_id.into(),
                agent_session_id: standard.agent_session_id.clone(),
            },
        )
        .expect("get deleted session")
        .is_none());
        assert_eq!(
            delete_agent_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                app.state::<RemoteBridgeRuntimeState>(),
                DeleteAgentSessionRequestDto {
                    project_id: project_id.into(),
                    agent_session_id: "missing-session".into(),
                },
            )
            .expect_err("missing session deletion is typed")
            .code,
            "agent_session_missing"
        );
    }
}
