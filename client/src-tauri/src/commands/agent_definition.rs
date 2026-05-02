use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        agent_definition_summary_dto, agent_definition_version_summary_dto, validate_non_empty,
        AgentDefinitionSummaryDto, AgentDefinitionVersionSummaryDto,
        ArchiveAgentDefinitionRequestDto, CommandResult, GetAgentDefinitionVersionRequestDto,
        ListAgentDefinitionsRequestDto, ListAgentDefinitionsResponseDto,
    },
    db::project_store,
    state::DesktopState,
};

use super::runtime_support::resolve_project_root;

#[tauri::command]
pub fn list_agent_definitions<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListAgentDefinitionsRequestDto,
) -> CommandResult<ListAgentDefinitionsResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let definitions = project_store::list_agent_definitions(&repo_root, request.include_archived)?
        .into_iter()
        .map(agent_definition_summary_dto)
        .collect();
    Ok(ListAgentDefinitionsResponseDto { definitions })
}

#[tauri::command]
pub fn archive_agent_definition<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ArchiveAgentDefinitionRequestDto,
) -> CommandResult<AgentDefinitionSummaryDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.definition_id, "definitionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let archived = project_store::archive_agent_definition(
        &repo_root,
        &request.definition_id,
        &now_timestamp(),
    )?;
    Ok(agent_definition_summary_dto(archived))
}

#[tauri::command]
pub fn get_agent_definition_version<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetAgentDefinitionVersionRequestDto,
) -> CommandResult<Option<AgentDefinitionVersionSummaryDto>> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.definition_id, "definitionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let version = project_store::load_agent_definition_version(
        &repo_root,
        &request.definition_id,
        request.version,
    )?;
    Ok(version.map(agent_definition_version_summary_dto))
}
