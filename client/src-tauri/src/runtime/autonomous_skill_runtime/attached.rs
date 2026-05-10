use serde::{Deserialize, Serialize};

use crate::commands::{CommandError, CommandResult};

use super::{
    skill_tool::XeroSkillToolContextPayload, XeroSkillSourceKind, XeroSkillSourceScope,
    XeroSkillToolDiagnostic,
};

pub const XERO_ATTACHED_SKILL_RESOLUTION_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum XeroAttachedSkillScope {
    Global,
    Project,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum XeroAttachedSkillResolutionStatus {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum XeroAttachedSkillRepairHint {
    EnableSource,
    ApproveSource,
    RefreshPin,
    RemoveAttachment,
    Retry,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroAttachedSkillRef {
    pub id: String,
    pub source_id: String,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub source_kind: XeroSkillSourceKind,
    pub scope: XeroAttachedSkillScope,
    pub version_hash: String,
    pub include_supporting_assets: bool,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroResolvedAttachedSkill {
    pub id: String,
    pub source_id: String,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub source_kind: XeroSkillSourceKind,
    pub scope: XeroAttachedSkillScope,
    pub version_hash: String,
    pub include_supporting_assets: bool,
    pub required: bool,
    pub content_hash: String,
    pub context: XeroSkillToolContextPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroAttachedSkillDiagnostic {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub redacted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_hint: Option<XeroAttachedSkillRepairHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroAttachedSkillResolutionRequest {
    pub project_id: String,
    pub run_id: String,
    pub attached_skills: Vec<XeroAttachedSkillRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroAttachedSkillResolutionSnapshot {
    pub contract_version: u32,
    pub project_id: String,
    pub run_id: String,
    pub resolved_at: String,
    pub attached_skills: Vec<XeroResolvedAttachedSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroAttachedSkillResolutionReport {
    pub contract_version: u32,
    pub status: XeroAttachedSkillResolutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<XeroAttachedSkillResolutionSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<XeroAttachedSkillDiagnostic>,
}

impl XeroAttachedSkillScope {
    pub fn from_source_scope(scope: &XeroSkillSourceScope) -> Self {
        match scope {
            XeroSkillSourceScope::Global => Self::Global,
            XeroSkillSourceScope::Project { .. } => Self::Project,
        }
    }
}

impl XeroAttachedSkillDiagnostic {
    pub fn from_skill_tool_diagnostic(
        diagnostic: XeroSkillToolDiagnostic,
        attachment: &XeroAttachedSkillRef,
        repair_hint: Option<XeroAttachedSkillRepairHint>,
    ) -> Self {
        Self {
            code: diagnostic.code,
            message: diagnostic.message,
            retryable: diagnostic.retryable,
            redacted: diagnostic.redacted,
            attachment_id: Some(attachment.id.clone()),
            source_id: Some(attachment.source_id.clone()),
            skill_id: Some(attachment.skill_id.clone()),
            repair_hint,
        }
    }

    pub fn user_fixable(
        code: impl Into<String>,
        message: impl Into<String>,
        attachment: &XeroAttachedSkillRef,
        repair_hint: XeroAttachedSkillRepairHint,
    ) -> CommandResult<Self> {
        Ok(Self {
            code: validate_non_empty_text(code.into(), "code")?,
            message: validate_non_empty_text(message.into(), "message")?,
            retryable: false,
            redacted: false,
            attachment_id: Some(attachment.id.clone()),
            source_id: Some(attachment.source_id.clone()),
            skill_id: Some(attachment.skill_id.clone()),
            repair_hint: Some(repair_hint),
        })
    }
}

pub fn validate_attached_skill_resolution_request(
    request: XeroAttachedSkillResolutionRequest,
) -> CommandResult<XeroAttachedSkillResolutionRequest> {
    let project_id = validate_non_empty_text(request.project_id, "projectId")?;
    let run_id = validate_non_empty_text(request.run_id, "runId")?;
    let mut attached_skills = Vec::with_capacity(request.attached_skills.len());
    let mut ids = std::collections::BTreeSet::new();
    let mut source_ids = std::collections::BTreeSet::new();

    for skill in request.attached_skills {
        let skill = validate_attached_skill_ref(skill)?;
        if !ids.insert(skill.id.clone()) {
            return Err(CommandError::user_fixable(
                "attached_skill_duplicate_id",
                format!(
                    "Xero rejected attached skill `{}` because attachment ids must be unique within a run.",
                    skill.id
                ),
            ));
        }
        if !source_ids.insert(skill.source_id.clone()) {
            return Err(CommandError::user_fixable(
                "attached_skill_duplicate_source_id",
                format!(
                    "Xero rejected attached skill source `{}` because source ids must be unique within a run.",
                    skill.source_id
                ),
            ));
        }
        attached_skills.push(skill);
    }

    Ok(XeroAttachedSkillResolutionRequest {
        project_id,
        run_id,
        attached_skills,
    })
}

fn validate_attached_skill_ref(skill: XeroAttachedSkillRef) -> CommandResult<XeroAttachedSkillRef> {
    if !skill.required {
        return Err(CommandError::user_fixable(
            "attached_skill_required_invalid",
            "Xero requires attached skills to be marked required in this release.",
        ));
    }

    Ok(XeroAttachedSkillRef {
        id: validate_non_empty_text(skill.id, "id")?,
        source_id: validate_source_id(skill.source_id)?,
        skill_id: validate_non_empty_text(skill.skill_id, "skillId")?,
        name: validate_non_empty_text(skill.name, "name")?,
        description: skill.description,
        source_kind: skill.source_kind,
        scope: skill.scope,
        version_hash: validate_non_empty_text(skill.version_hash, "versionHash")?,
        include_supporting_assets: skill.include_supporting_assets,
        required: true,
    })
}

fn validate_source_id(value: String) -> CommandResult<String> {
    let source_id = validate_non_empty_text(value, "sourceId")?;
    if !source_id.starts_with("skill-source:v") {
        return Err(CommandError::user_fixable(
            "attached_skill_source_id_invalid",
            "Xero requires attached skill source ids to use the canonical skill-source contract prefix.",
        ));
    }
    Ok(source_id)
}

fn validate_non_empty_text(value: String, field: &'static str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(trimmed.to_owned())
}
