use tauri::{AppHandle, Runtime, State};

use serde_json::json;

use crate::{
    auth::now_timestamp,
    commands::{
        agent_definition_summary_dto, agent_definition_version_summary_dto, validate_non_empty,
        AgentDefinitionSummaryDto, AgentDefinitionValidationDiagnosticDto,
        AgentDefinitionValidationReportDto, AgentDefinitionValidationStatusDto,
        AgentDefinitionVersionSummaryDto, AgentDefinitionWriteResponseDto,
        ArchiveAgentDefinitionRequestDto, CommandError, CommandResult,
        GetAgentDefinitionVersionDiffRequestDto, GetAgentDefinitionVersionRequestDto,
        ListAgentDefinitionsRequestDto, ListAgentDefinitionsResponseDto,
        PreviewAgentDefinitionRequestDto, SaveAgentDefinitionRequestDto,
        UpdateAgentDefinitionRequestDto,
    },
    db::project_store,
    runtime::autonomous_tool_runtime::{
        AutonomousAgentDefinitionAction, AutonomousAgentDefinitionOutput,
        AutonomousAgentDefinitionRequest, AutonomousAgentDefinitionValidationDiagnostic,
        AutonomousAgentDefinitionValidationReport, AutonomousAgentDefinitionValidationStatus,
        AutonomousToolOutput, AutonomousToolRuntime,
    },
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

#[tauri::command]
pub fn get_agent_definition_version_diff<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetAgentDefinitionVersionDiffRequestDto,
) -> CommandResult<serde_json::Value> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.definition_id, "definitionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    project_store::load_agent_definition_version_diff(
        &repo_root,
        &request.definition_id,
        request.from_version,
        request.to_version,
    )
}

#[tauri::command]
pub fn save_agent_definition<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SaveAgentDefinitionRequestDto,
) -> CommandResult<AgentDefinitionWriteResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let runtime = AutonomousToolRuntime::new(&repo_root)?;
    let runtime_request = AutonomousAgentDefinitionRequest {
        action: AutonomousAgentDefinitionAction::Save,
        definition_id: request.definition_id,
        source_definition_id: None,
        include_archived: false,
        definition: Some(request.definition),
    };
    let result = runtime.agent_definition_with_operator_approval(runtime_request)?;
    let output = unwrap_agent_definition_output(result.output)?;
    write_response_from_output(&repo_root, output)
}

#[tauri::command]
pub fn update_agent_definition<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateAgentDefinitionRequestDto,
) -> CommandResult<AgentDefinitionWriteResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.definition_id, "definitionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let runtime = AutonomousToolRuntime::new(&repo_root)?;
    let runtime_request = AutonomousAgentDefinitionRequest {
        action: AutonomousAgentDefinitionAction::Update,
        definition_id: Some(request.definition_id),
        source_definition_id: None,
        include_archived: false,
        definition: Some(request.definition),
    };
    let result = runtime.agent_definition_with_operator_approval(runtime_request)?;
    let output = unwrap_agent_definition_output(result.output)?;
    write_response_from_output(&repo_root, output)
}

#[tauri::command]
pub fn preview_agent_definition<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: PreviewAgentDefinitionRequestDto,
) -> CommandResult<serde_json::Value> {
    validate_non_empty(&request.project_id, "projectId")?;
    if let Some(definition_id) = request.definition_id.as_deref() {
        validate_non_empty(definition_id, "definitionId")?;
    }
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let runtime = AutonomousToolRuntime::new(&repo_root)?;
    let runtime_request = AutonomousAgentDefinitionRequest {
        action: AutonomousAgentDefinitionAction::Preview,
        definition_id: request.definition_id,
        source_definition_id: None,
        include_archived: false,
        definition: Some(request.definition),
    };
    let result = runtime.agent_definition_with_operator_approval(runtime_request)?;
    let output = unwrap_agent_definition_output(result.output)?;
    let validation = output
        .validation_report
        .as_ref()
        .map(map_validation_report)
        .unwrap_or_else(default_valid_report);
    Ok(json!({
        "schema": "xero.agent_definition_preview_command.v1",
        "projectId": request.project_id,
        "applied": output.applied,
        "message": output.message,
        "definition": output.definition,
        "validation": validation,
        "effectiveRuntimePreview": output.effective_runtime_preview,
        "uiDeferred": true,
    }))
}

fn unwrap_agent_definition_output(
    output: AutonomousToolOutput,
) -> CommandResult<AutonomousAgentDefinitionOutput> {
    match output {
        AutonomousToolOutput::AgentDefinition(value) => Ok(value),
        _ => Err(CommandError::system_fault(
            "agent_definition_unexpected_output",
            "Xero received an unexpected runtime response while writing the agent definition.",
        )),
    }
}

fn write_response_from_output(
    repo_root: &std::path::Path,
    output: AutonomousAgentDefinitionOutput,
) -> CommandResult<AgentDefinitionWriteResponseDto> {
    let validation = output
        .validation_report
        .as_ref()
        .map(map_validation_report)
        .unwrap_or_else(default_valid_report);

    let summary = if output.applied {
        if let Some(saved) = output
            .definition
            .as_ref()
            .map(|definition| definition.definition_id.clone())
        {
            project_store::load_agent_definition(repo_root, &saved)?
                .map(agent_definition_summary_dto)
        } else {
            None
        }
    } else {
        None
    };

    Ok(AgentDefinitionWriteResponseDto {
        applied: output.applied,
        message: output.message,
        summary,
        validation,
    })
}

fn map_validation_report(
    report: &AutonomousAgentDefinitionValidationReport,
) -> AgentDefinitionValidationReportDto {
    AgentDefinitionValidationReportDto {
        status: match report.status {
            AutonomousAgentDefinitionValidationStatus::Valid => {
                AgentDefinitionValidationStatusDto::Valid
            }
            AutonomousAgentDefinitionValidationStatus::Invalid => {
                AgentDefinitionValidationStatusDto::Invalid
            }
        },
        diagnostics: report
            .diagnostics
            .iter()
            .map(map_validation_diagnostic)
            .collect(),
    }
}

fn map_validation_diagnostic(
    diagnostic: &AutonomousAgentDefinitionValidationDiagnostic,
) -> AgentDefinitionValidationDiagnosticDto {
    AgentDefinitionValidationDiagnosticDto {
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        path: diagnostic.path.clone(),
        denied_tool: diagnostic.denied_tool.clone(),
        denied_effect_class: diagnostic.denied_effect_class.clone(),
        base_capability_profile: diagnostic.base_capability_profile.clone(),
        reason: diagnostic.reason.clone(),
        repair_hint: diagnostic.repair_hint.clone(),
    }
}

fn default_valid_report() -> AgentDefinitionValidationReportDto {
    AgentDefinitionValidationReportDto {
        status: AgentDefinitionValidationStatusDto::Valid,
        diagnostics: Vec::new(),
    }
}
