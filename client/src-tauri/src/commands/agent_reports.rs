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
    validate_handoff_context_summary_selector(&request)?;
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
    unreachable!("handoff selector validation requires exactly one identifier")
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
            | "skill_runtime_tool"
            | "attached_skill_context"
    ) {
        Ok(())
    } else {
        Err(crate::commands::CommandError::invalid_request(
            "subjectKind",
        ))
    }
}

fn validate_handoff_context_summary_selector(
    request: &GetAgentHandoffContextSummaryRequestDto,
) -> CommandResult<()> {
    let selector_count = [
        request.handoff_id.as_ref(),
        request.target_run_id.as_ref(),
        request.source_run_id.as_ref(),
    ]
    .into_iter()
    .filter(|value| value.is_some())
    .count();

    match selector_count {
        1 => Ok(()),
        0 => Err(CommandError::user_fixable(
            "agent_handoff_context_summary_identifier_required",
            "Provide exactly one of handoffId, targetRunId, or sourceRunId.",
        )),
        _ => Err(CommandError::user_fixable(
            "agent_handoff_context_summary_identifier_ambiguous",
            "Provide only one of handoffId, targetRunId, or sourceRunId.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_subject_validation_matches_the_frontend_contract() {
        for kind in [
            "custom_agent",
            "tool_pack",
            "external_integration",
            "browser_control",
            "destructive_write",
            "skill_runtime_tool",
            "attached_skill_context",
        ] {
            validate_capability_subject_kind(kind)
                .unwrap_or_else(|error| panic!("{kind} should be accepted: {error:?}"));
        }

        let error = validate_capability_subject_kind("unknown")
            .expect_err("unknown capability kinds must fail closed");
        assert_eq!(error.code, "invalid_request");
    }

    #[test]
    fn capability_permission_command_projects_supported_skill_kinds() {
        for kind in ["skill_runtime_tool", "attached_skill_context"] {
            let value = get_capability_permission_explanation(
                GetCapabilityPermissionExplanationRequestDto {
                    subject_kind: kind.into(),
                    subject_id: "fixture-skill".into(),
                },
            )
            .unwrap_or_else(|error| panic!("{kind} should resolve: {error:?}"));

            assert_eq!(value["subjectKind"], kind);
            assert_eq!(value["subjectId"], "fixture-skill");
            assert_eq!(value["riskClass"], kind);
        }
    }

    #[test]
    fn handoff_summary_selector_requires_exactly_one_identifier() {
        let request =
            |handoff_id: Option<&str>, target_run_id: Option<&str>, source_run_id: Option<&str>| {
                GetAgentHandoffContextSummaryRequestDto {
                    project_id: "project-1".into(),
                    handoff_id: handoff_id.map(Into::into),
                    target_run_id: target_run_id.map(Into::into),
                    source_run_id: source_run_id.map(Into::into),
                }
            };

        for valid in [
            request(Some("handoff-1"), None, None),
            request(None, Some("run-target"), None),
            request(None, None, Some("run-source")),
        ] {
            validate_handoff_context_summary_selector(&valid)
                .expect("one handoff selector should be accepted");
        }

        let missing = validate_handoff_context_summary_selector(&request(None, None, None))
            .expect_err("a missing selector must be rejected");
        assert_eq!(
            missing.code,
            "agent_handoff_context_summary_identifier_required"
        );

        let ambiguous = validate_handoff_context_summary_selector(&request(
            Some("handoff-1"),
            Some("run-target"),
            None,
        ))
        .expect_err("multiple selectors must be rejected");
        assert_eq!(
            ambiguous.code,
            "agent_handoff_context_summary_identifier_ambiguous"
        );
    }
}
