mod attached_skills;
mod db_touchpoints;
mod extends;
mod handoff;
mod identity;
mod memory;
mod output;
mod profile;
mod retrieval;
mod safety_limits;
mod tool_policy;
mod workflow;

use std::path::Path;

use serde_json::Value as JsonValue;

use super::{
    AutonomousAgentDefinitionValidationDiagnostic, AutonomousAgentDefinitionValidationReport,
    AutonomousAgentDefinitionValidationStatus,
};

type Diagnostics = Vec<AutonomousAgentDefinitionValidationDiagnostic>;

pub(super) fn validate_definition_snapshot_with_registry(
    snapshot: &JsonValue,
    repo_root: Option<&Path>,
    mcp_registry_path: Option<&Path>,
) -> AutonomousAgentDefinitionValidationReport {
    let mut diagnostics = Vec::new();

    identity::validate(snapshot, &mut diagnostics);
    let base_profile = profile::validate(snapshot, &mut diagnostics);
    extends::validate(snapshot, &base_profile, repo_root, &mut diagnostics);
    tool_policy::validate(snapshot, &base_profile, mcp_registry_path, &mut diagnostics);
    workflow::validate(snapshot, &mut diagnostics);
    output::validate(snapshot, &mut diagnostics);
    db_touchpoints::validate(snapshot, &mut diagnostics);
    memory::validate(snapshot, &mut diagnostics);
    retrieval::validate(snapshot.get("retrievalDefaults"), &mut diagnostics);
    handoff::validate(snapshot, &mut diagnostics);
    attached_skills::validate(snapshot, repo_root, &mut diagnostics);
    safety_limits::validate(snapshot, &mut diagnostics);

    let status = if diagnostics.is_empty() {
        AutonomousAgentDefinitionValidationStatus::Valid
    } else {
        AutonomousAgentDefinitionValidationStatus::Invalid
    };
    AutonomousAgentDefinitionValidationReport {
        status,
        diagnostics,
    }
}

#[cfg(test)]
fn diagnostic_codes(diagnostics: &[AutonomousAgentDefinitionValidationDiagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.clone())
        .collect()
}

#[cfg(test)]
fn minimal_definition() -> JsonValue {
    serde_json::json!({
        "schema": super::AGENT_DEFINITION_SCHEMA,
        "schemaVersion": super::AGENT_DEFINITION_SCHEMA_VERSION,
        "id": "release_notes_helper",
        "version": 1,
        "displayName": "Release Notes Helper",
        "shortLabel": "Release",
        "description": "Drafts release notes from approved project context.",
        "taskPurpose": "Summarize reviewed release facts without editing files.",
        "scope": "project_custom",
        "lifecycleState": "active",
        "baseCapabilityProfile": "observe_only",
        "defaultApprovalMode": "suggest",
        "allowedApprovalModes": ["suggest"],
        "toolPolicy": {
            "allowedEffectClasses": ["observe"],
            "allowedToolGroups": [],
            "allowedToolPacks": [],
            "allowedTools": ["read"],
            "deniedTools": [],
            "deniedToolPacks": [],
            "externalServiceAllowed": false,
            "browserControlAllowed": false,
            "skillRuntimeAllowed": false,
            "subagentAllowed": false,
            "commandAllowed": false,
            "destructiveWriteAllowed": false
        },
        "workflowContract": "Read approved release context and draft release notes.",
        "finalResponseContract": "Return concise release notes with open risks.",
        "prompts": [{"id": "intent", "role": "developer", "body": "Draft release notes from reviewed context."}],
        "tools": [],
        "output": {
            "contract": "answer",
            "sections": [{"id": "summary", "label": "Summary", "description": "Release summary.", "emphasis": "core", "producedByTools": ["read"]}]
        },
        "dbTouchpoints": {
            "reads": [{"table": "project_context_records", "kind": "read", "purpose": "Read approved facts.", "triggers": [], "columns": ["summary"]}],
            "writes": [],
            "encouraged": []
        },
        "consumes": [],
        "projectDataPolicy": {"recordKinds": ["project_fact"], "structuredSchemas": ["xero.project_record.v1"]},
        "memoryCandidatePolicy": {"memoryKinds": ["project_fact"], "reviewRequired": true},
        "retrievalDefaults": {"enabled": true, "recordKinds": ["project_fact"], "memoryKinds": ["project_fact"], "limit": 6},
        "handoffPolicy": {"enabled": true, "preserveDefinitionVersion": true},
        "examplePrompts": ["Draft release notes.", "Summarize fixes.", "List release risks."],
        "refusalEscalationCases": ["Refuse edits.", "Escalate missing context.", "Refuse invented claims."],
        "attachedSkills": []
    })
}
