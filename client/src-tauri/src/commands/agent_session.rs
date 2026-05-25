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
    let before = load_persisted_runtime_run(repo_root, project_id, agent_session_id)?;
    let Some(snapshot) = before.as_ref() else {
        return Ok(());
    };

    if snapshot.run.supervisor_kind != crate::runtime::OWNED_AGENT_SUPERVISOR_KIND {
        return Ok(());
    }

    if !matches!(
        snapshot.run.status,
        project_store::RuntimeRunStatus::Starting
            | project_store::RuntimeRunStatus::Running
            | project_store::RuntimeRunStatus::Stale
    ) {
        return Ok(());
    }

    if state
        .agent_run_supervisor()
        .is_active(&snapshot.run.run_id)?
    {
        return Ok(());
    }

    let after = stop_owned_runtime_run(repo_root, snapshot)?;
    emit_runtime_run_updated_if_changed(app, project_id, agent_session_id, &before, &Some(after))?;
    Ok(())
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
