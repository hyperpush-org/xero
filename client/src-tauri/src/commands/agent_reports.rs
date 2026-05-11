use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        runtime_support::resolve_project_root, validate_non_empty, CommandError, CommandResult,
    },
    db::project_store::{
        capability_permission_explanation, load_agent_database_touchpoint_explanation,
        load_agent_handoff_context_summary, load_agent_handoff_context_summary_by_source_run,
        load_agent_handoff_context_summary_by_target_run, load_agent_knowledge_inspection,
        load_agent_run_start_explanation, load_project_support_diagnostics_bundle,
    },
    state::DesktopState,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentRunStartExplanationRequestDto {
    pub project_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentKnowledgeInspectionRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentHandoffContextSummaryRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentSupportDiagnosticsBundleRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetCapabilityPermissionExplanationRequestDto {
    pub subject_kind: String,
    pub subject_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentDatabaseTouchpointExplanationRequestDto {
    pub project_id: String,
    pub definition_id: String,
    pub version: u32,
}

#[tauri::command]
pub fn get_capability_permission_explanation(
    request: GetCapabilityPermissionExplanationRequestDto,
) -> CommandResult<JsonValue> {
    validate_capability_subject_kind(&request.subject_kind)?;
    validate_non_empty(&request.subject_id, "subjectId")?;
    Ok(capability_permission_explanation(
        &request.subject_kind,
        &request.subject_id,
    ))
}

#[tauri::command]
pub fn get_agent_database_touchpoint_explanation<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetAgentDatabaseTouchpointExplanationRequestDto,
) -> CommandResult<JsonValue> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.definition_id, "definitionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    load_agent_database_touchpoint_explanation(
        &repo_root,
        &request.project_id,
        &request.definition_id,
        request.version,
    )
}

#[tauri::command]
pub fn get_agent_run_start_explanation<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetAgentRunStartExplanationRequestDto,
) -> CommandResult<JsonValue> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    load_agent_run_start_explanation(&repo_root, &request.project_id, &request.run_id)
}

#[tauri::command]
pub fn get_agent_knowledge_inspection<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetAgentKnowledgeInspectionRequestDto,
) -> CommandResult<JsonValue> {
    validate_non_empty(&request.project_id, "projectId")?;
    if let Some(agent_session_id) = request.agent_session_id.as_deref() {
        validate_non_empty(agent_session_id, "agentSessionId")?;
    }
    if let Some(run_id) = request.run_id.as_deref() {
        validate_non_empty(run_id, "runId")?;
    }
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    load_agent_knowledge_inspection(
        &repo_root,
        &request.project_id,
        request.agent_session_id.as_deref(),
        request.run_id.as_deref(),
        request.limit.unwrap_or(25),
    )
}

#[tauri::command]
pub fn get_agent_handoff_context_summary<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetAgentHandoffContextSummaryRequestDto,
) -> CommandResult<JsonValue> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    if let Some(handoff_id) = request.handoff_id.as_deref() {
        validate_non_empty(handoff_id, "handoffId")?;
        return load_agent_handoff_context_summary(&repo_root, &request.project_id, handoff_id);
    }
    if let Some(target_run_id) = request.target_run_id.as_deref() {
        validate_non_empty(target_run_id, "targetRunId")?;
        return load_agent_handoff_context_summary_by_target_run(
            &repo_root,
            &request.project_id,
            target_run_id,
        );
    }
    if let Some(source_run_id) = request.source_run_id.as_deref() {
        validate_non_empty(source_run_id, "sourceRunId")?;
        return load_agent_handoff_context_summary_by_source_run(
            &repo_root,
            &request.project_id,
            source_run_id,
        );
    }
    Err(CommandError::invalid_request(
        "agent_handoff_context_summary_identifier_required",
    ))
}

#[tauri::command]
pub fn get_agent_support_diagnostics_bundle<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetAgentSupportDiagnosticsBundleRequestDto,
) -> CommandResult<JsonValue> {
    validate_non_empty(&request.project_id, "projectId")?;
    if let Some(run_id) = request.run_id.as_deref() {
        validate_non_empty(run_id, "runId")?;
    }
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    load_project_support_diagnostics_bundle(
        &repo_root,
        &request.project_id,
        request.run_id.as_deref(),
        &now_timestamp(),
    )
}

fn validate_capability_subject_kind(kind: &str) -> CommandResult<()> {
    validate_non_empty(kind, "subjectKind")?;
    if matches!(
        kind,
        "custom_agent"
            | "tool_pack"
            | "external_integration"
            | "browser_control"
            | "destructive_write"
    ) {
        Ok(())
    } else {
        Err(crate::commands::CommandError::invalid_request(
            "subjectKind",
        ))
    }
}
