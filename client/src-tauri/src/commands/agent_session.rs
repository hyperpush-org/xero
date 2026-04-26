use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, AgentSessionDto, AgentSessionLineageBoundaryKindDto,
        AgentSessionLineageDiagnosticDto, AgentSessionLineageDto, AgentSessionStatusDto,
        ArchiveAgentSessionRequestDto, CommandResult, CreateAgentSessionRequestDto,
        DeleteAgentSessionRequestDto, GetAgentSessionRequestDto, ListAgentSessionsRequestDto,
        ListAgentSessionsResponseDto, RestoreAgentSessionRequestDto, UpdateAgentSessionRequestDto,
    },
    db::project_store::{
        self, AgentSessionCreateRecord, AgentSessionLineageBoundaryKind, AgentSessionLineageRecord,
        AgentSessionRecord, AgentSessionStatus, AgentSessionUpdateRecord,
        DEFAULT_AGENT_SESSION_TITLE,
    },
    state::DesktopState,
};

use super::runtime_support::resolve_project_root;

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

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let session = project_store::create_agent_session(
        &repo_root,
        &AgentSessionCreateRecord {
            project_id: request.project_id,
            title: request
                .title
                .unwrap_or_else(|| DEFAULT_AGENT_SESSION_TITLE.to_string()),
            summary: request.summary,
            selected: request.selected,
        },
    )?;

    Ok(agent_session_dto(&session))
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
        sessions: sessions.iter().map(agent_session_dto).collect(),
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

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
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
    Ok(agent_session_dto(&session))
}

#[tauri::command]
pub fn archive_agent_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ArchiveAgentSessionRequestDto,
) -> CommandResult<AgentSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let session = project_store::archive_agent_session(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?;
    Ok(agent_session_dto(&session))
}

#[tauri::command]
pub fn restore_agent_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RestoreAgentSessionRequestDto,
) -> CommandResult<AgentSessionDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let session = project_store::restore_agent_session(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?;
    Ok(agent_session_dto(&session))
}

#[tauri::command]
pub fn delete_agent_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: DeleteAgentSessionRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    project_store::delete_agent_session(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?;
    Ok(())
}

pub(crate) fn agent_session_dto(record: &AgentSessionRecord) -> AgentSessionDto {
    AgentSessionDto {
        project_id: record.project_id.clone(),
        agent_session_id: record.agent_session_id.clone(),
        title: record.title.clone(),
        summary: record.summary.clone(),
        status: match record.status {
            AgentSessionStatus::Active => AgentSessionStatusDto::Active,
            AgentSessionStatus::Archived => AgentSessionStatusDto::Archived,
        },
        selected: record.selected,
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
