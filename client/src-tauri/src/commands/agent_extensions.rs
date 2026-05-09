use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tauri::{AppHandle, Runtime, State};
use xero_agent_core::{
    ToolApprovalRequirement, ToolDescriptorV2, ToolEffectClass, ToolExtensionManifest,
    ToolMutability, ToolSandboxRequirement,
};

use crate::{
    commands::{runtime_support::resolve_project_root, validate_non_empty, CommandResult},
    state::DesktopState,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidateAgentToolExtensionManifestRequestDto {
    pub project_id: String,
    pub manifest: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolExtensionPermissionSummaryDto {
    pub permission_id: String,
    pub label: String,
    pub effect_class: ToolEffectClass,
    pub risk_class: String,
    pub audit_label: String,
    pub mutability: ToolMutability,
    pub sandbox_requirement: ToolSandboxRequirement,
    pub approval_requirement: ToolApprovalRequirement,
    pub capability_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolExtensionValidationDiagnosticDto {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolExtensionManifestValidationDto {
    pub schema: String,
    pub project_id: String,
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descriptor: Option<ToolDescriptorV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<AgentToolExtensionPermissionSummaryDto>,
    pub fixture_count: usize,
    pub fixture_ids: Vec<String>,
    pub diagnostics: Vec<AgentToolExtensionValidationDiagnosticDto>,
    pub ui_deferred: bool,
}

#[tauri::command]
pub fn validate_agent_tool_extension_manifest<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ValidateAgentToolExtensionManifestRequestDto,
) -> CommandResult<AgentToolExtensionManifestValidationDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let _repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;

    Ok(validate_tool_extension_manifest_payload(
        request.project_id,
        request.manifest,
    ))
}

fn validate_tool_extension_manifest_payload(
    project_id: String,
    manifest_payload: JsonValue,
) -> AgentToolExtensionManifestValidationDto {
    match serde_json::from_value::<ToolExtensionManifest>(manifest_payload) {
        Ok(manifest) => validation_from_manifest(project_id, manifest),
        Err(error) => AgentToolExtensionManifestValidationDto {
            schema: "xero.agent_tool_extension_manifest_validation.v1".into(),
            project_id,
            valid: false,
            extension_id: None,
            tool_name: None,
            descriptor: None,
            permission: None,
            fixture_count: 0,
            fixture_ids: Vec::new(),
            diagnostics: vec![AgentToolExtensionValidationDiagnosticDto {
                code: "agent_tool_extension_manifest_malformed".into(),
                message: error.to_string(),
            }],
            ui_deferred: true,
        },
    }
}

fn validation_from_manifest(
    project_id: String,
    manifest: ToolExtensionManifest,
) -> AgentToolExtensionManifestValidationDto {
    let extension_id = Some(manifest.extension_id.clone());
    let tool_name = Some(manifest.tool_name.clone());
    let fixture_ids = manifest
        .test_fixtures
        .iter()
        .map(|fixture| fixture.fixture_id.clone())
        .collect::<Vec<_>>();
    let fixture_count = fixture_ids.len();
    let permission = Some(AgentToolExtensionPermissionSummaryDto {
        permission_id: manifest.permission.permission_id.clone(),
        label: manifest.permission.label.clone(),
        effect_class: manifest.permission.effect_class.clone(),
        risk_class: manifest.permission.risk_class.clone(),
        audit_label: manifest.permission.audit_label.clone(),
        mutability: manifest.mutability,
        sandbox_requirement: manifest.sandbox_requirement,
        approval_requirement: manifest.approval_requirement,
        capability_tags: manifest.capability_tags.clone(),
    });

    match manifest.validate() {
        Ok(()) => AgentToolExtensionManifestValidationDto {
            schema: "xero.agent_tool_extension_manifest_validation.v1".into(),
            project_id,
            valid: true,
            extension_id,
            tool_name,
            descriptor: Some(manifest.descriptor()),
            permission,
            fixture_count,
            fixture_ids,
            diagnostics: Vec::new(),
            ui_deferred: true,
        },
        Err(error) => AgentToolExtensionManifestValidationDto {
            schema: "xero.agent_tool_extension_manifest_validation.v1".into(),
            project_id,
            valid: false,
            extension_id,
            tool_name,
            descriptor: None,
            permission,
            fixture_count,
            fixture_ids,
            diagnostics: vec![AgentToolExtensionValidationDiagnosticDto {
                code: error.code,
                message: error.message,
            }],
            ui_deferred: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    fn valid_manifest() -> JsonValue {
        json!({
            "contractVersion": 1,
            "extensionId": "demo_extension",
            "toolName": "demo_tool",
            "label": "Demo Tool",
            "description": "Runs a deterministic extension fixture.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            },
            "permission": {
                "permissionId": "demo_extension_read",
                "label": "Demo extension read",
                "effectClass": "observe",
                "riskClass": "low",
                "auditLabel": "Demo extension read"
            },
            "mutability": "read_only",
            "sandboxRequirement": "read_only",
            "approvalRequirement": "policy",
            "capabilityTags": ["demo", "extension"],
            "testFixtures": [
                {
                    "fixtureId": "basic_read",
                    "input": { "query": "hello" },
                    "expectedSummaryContains": "hello"
                }
            ]
        })
    }

    #[test]
    fn s20_extension_manifest_validation_reports_descriptor_and_fixture_metadata() {
        let report = validate_tool_extension_manifest_payload("project-1".into(), valid_manifest());

        assert!(report.valid);
        assert_eq!(
            report.schema,
            "xero.agent_tool_extension_manifest_validation.v1"
        );
        assert_eq!(report.extension_id.as_deref(), Some("demo_extension"));
        assert_eq!(report.tool_name.as_deref(), Some("demo_tool"));
        assert_eq!(report.fixture_ids, vec!["basic_read"]);
        assert_eq!(
            report
                .permission
                .as_ref()
                .expect("permission")
                .permission_id,
            "demo_extension_read"
        );
        assert_eq!(
            report.descriptor.as_ref().expect("descriptor").name,
            "demo_tool"
        );
        assert!(report.ui_deferred);
    }

    #[test]
    fn s20_extension_manifest_validation_reports_missing_fixtures_without_ui() {
        let mut manifest = valid_manifest();
        manifest["testFixtures"] = json!([]);

        let report = validate_tool_extension_manifest_payload("project-1".into(), manifest);

        assert!(!report.valid);
        assert_eq!(report.extension_id.as_deref(), Some("demo_extension"));
        assert_eq!(report.fixture_count, 0);
        assert_eq!(
            report
                .diagnostics
                .first()
                .map(|diagnostic| diagnostic.code.as_str()),
            Some("agent_tool_extension_fixture_missing")
        );
        assert!(report.ui_deferred);
    }
}
