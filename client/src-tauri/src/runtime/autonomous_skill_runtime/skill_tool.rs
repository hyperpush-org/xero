use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    commands::{CommandError, CommandResult},
    runtime::redaction::find_prohibited_persistence_content,
};

use super::{
    contract::{
        XeroSkillSourceKind, XeroSkillSourceRecord, XeroSkillSourceState, XeroSkillTrustState,
    },
    inspection::{normalize_relative_source_path, normalize_skill_id},
    runtime::{ALLOWED_TEXT_EXTENSIONS, MAX_SKILL_FILE_BYTES},
};

pub const XERO_SKILL_TOOL_CONTRACT_VERSION: u32 = 1;
pub const XERO_SKILL_TOOL_MAX_QUERY_CHARS: usize = 128;
pub const XERO_SKILL_TOOL_DEFAULT_LIMIT: usize = 25;
pub const XERO_SKILL_TOOL_MAX_LIMIT: usize = 100;
pub const XERO_SKILL_TOOL_MAX_CONTEXT_ASSETS: usize = 32;
pub const XERO_SKILL_TOOL_MAX_CONTEXT_MARKDOWN_BYTES: usize = MAX_SKILL_FILE_BYTES;
pub const XERO_SKILL_TOOL_MAX_CONTEXT_ASSET_BYTES: usize = MAX_SKILL_FILE_BYTES;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum XeroSkillToolOperation {
    List,
    Resolve,
    Install,
    Invoke,
    Reload,
    CreateDynamic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum XeroSkillToolInput {
    #[serde(rename_all = "camelCase")]
    List {
        query: Option<String>,
        include_unavailable: bool,
        limit: Option<usize>,
    },
    #[serde(rename_all = "camelCase")]
    Resolve {
        source_id: Option<String>,
        skill_id: Option<String>,
        include_unavailable: bool,
    },
    #[serde(rename_all = "camelCase")]
    Install {
        source_id: String,
        approval_grant_id: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Invoke {
        source_id: String,
        approval_grant_id: Option<String>,
        include_supporting_assets: bool,
    },
    #[serde(rename_all = "camelCase")]
    Reload {
        source_id: Option<String>,
        source_kind: Option<XeroSkillSourceKind>,
    },
    #[serde(rename_all = "camelCase")]
    CreateDynamic {
        skill_id: String,
        markdown: String,
        supporting_assets: Vec<XeroSkillToolDynamicAssetInput>,
        source_run_id: Option<String>,
        source_artifact_id: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum XeroSkillToolAccessStatus {
    Allowed,
    ApprovalRequired,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroSkillToolAccessDecision {
    pub operation: XeroSkillToolOperation,
    pub source_id: String,
    pub status: XeroSkillToolAccessStatus,
    pub model_visible: bool,
    pub reason: Option<XeroSkillToolDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroSkillToolDiagnostic {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub redacted: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum XeroSkillToolLifecycleResult {
    Succeeded,
    Failed,
    ApprovalRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroSkillToolLifecycleEvent {
    pub contract_version: u32,
    pub operation: XeroSkillToolOperation,
    pub result: XeroSkillToolLifecycleResult,
    pub source_id: Option<String>,
    pub skill_id: Option<String>,
    pub detail: String,
    pub diagnostic: Option<XeroSkillToolDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroSkillToolContextPayload {
    pub contract_version: u32,
    pub source_id: String,
    pub skill_id: String,
    pub markdown: XeroSkillToolContextDocument,
    #[serde(default)]
    pub supporting_assets: Vec<XeroSkillToolContextAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroSkillToolContextDocument {
    pub relative_path: String,
    pub sha256: String,
    pub bytes: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroSkillToolContextAsset {
    pub relative_path: String,
    pub sha256: String,
    pub bytes: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroSkillToolDynamicAssetInput {
    pub relative_path: String,
    pub content: String,
}

impl XeroSkillToolInput {
    pub fn operation(&self) -> XeroSkillToolOperation {
        match self {
            Self::List { .. } => XeroSkillToolOperation::List,
            Self::Resolve { .. } => XeroSkillToolOperation::Resolve,
            Self::Install { .. } => XeroSkillToolOperation::Install,
            Self::Invoke { .. } => XeroSkillToolOperation::Invoke,
            Self::Reload { .. } => XeroSkillToolOperation::Reload,
            Self::CreateDynamic { .. } => XeroSkillToolOperation::CreateDynamic,
        }
    }
}

impl XeroSkillToolLifecycleEvent {
    pub fn succeeded(
        operation: XeroSkillToolOperation,
        source_id: Option<String>,
        skill_id: Option<String>,
        detail: impl Into<String>,
    ) -> CommandResult<Self> {
        validate_non_empty_text(detail.into(), "detail").map(|detail| Self {
            contract_version: XERO_SKILL_TOOL_CONTRACT_VERSION,
            operation,
            result: XeroSkillToolLifecycleResult::Succeeded,
            source_id,
            skill_id,
            detail,
            diagnostic: None,
        })
    }

    pub fn failed(
        operation: XeroSkillToolOperation,
        source_id: Option<String>,
        skill_id: Option<String>,
        detail: impl Into<String>,
        error: &CommandError,
    ) -> CommandResult<Self> {
        let detail = validate_non_empty_text(detail.into(), "detail")?;
        Ok(Self {
            contract_version: XERO_SKILL_TOOL_CONTRACT_VERSION,
            operation,
            result: XeroSkillToolLifecycleResult::Failed,
            source_id,
            skill_id,
            detail,
            diagnostic: Some(skill_tool_diagnostic_from_command_error(error)),
        })
    }

    pub fn approval_required(
        operation: XeroSkillToolOperation,
        source_id: Option<String>,
        skill_id: Option<String>,
        detail: impl Into<String>,
        diagnostic: XeroSkillToolDiagnostic,
    ) -> CommandResult<Self> {
        let detail = validate_non_empty_text(detail.into(), "detail")?;
        Ok(Self {
            contract_version: XERO_SKILL_TOOL_CONTRACT_VERSION,
            operation,
            result: XeroSkillToolLifecycleResult::ApprovalRequired,
            source_id,
            skill_id,
            detail,
            diagnostic: Some(diagnostic),
        })
    }
}

pub fn validate_skill_tool_input(input: XeroSkillToolInput) -> CommandResult<XeroSkillToolInput> {
    match input {
        XeroSkillToolInput::List {
            query,
            include_unavailable,
            limit,
        } => Ok(XeroSkillToolInput::List {
            query: normalize_optional_query(query)?,
            include_unavailable,
            limit: normalize_limit(limit)?,
        }),
        XeroSkillToolInput::Resolve {
            source_id,
            skill_id,
            include_unavailable,
        } => {
            let source_id = source_id.map(normalize_source_id).transpose()?;
            let skill_id = skill_id.as_deref().map(normalize_skill_id).transpose()?;
            match (&source_id, &skill_id) {
                (None, None) => Err(CommandError::invalid_request("sourceId or skillId")),
                (Some(_), Some(_)) => Err(CommandError::user_fixable(
                    "skill_tool_selector_ambiguous",
                    "Xero SkillTool resolve requests must select by sourceId or skillId, not both.",
                )),
                _ => Ok(XeroSkillToolInput::Resolve {
                    source_id,
                    skill_id,
                    include_unavailable,
                }),
            }
        }
        XeroSkillToolInput::Install {
            source_id,
            approval_grant_id,
        } => Ok(XeroSkillToolInput::Install {
            source_id: normalize_source_id(source_id)?,
            approval_grant_id: approval_grant_id
                .map(|value| validate_non_empty_text(value, "approvalGrantId"))
                .transpose()?,
        }),
        XeroSkillToolInput::Invoke {
            source_id,
            approval_grant_id,
            include_supporting_assets,
        } => Ok(XeroSkillToolInput::Invoke {
            source_id: normalize_source_id(source_id)?,
            approval_grant_id: approval_grant_id
                .map(|value| validate_non_empty_text(value, "approvalGrantId"))
                .transpose()?,
            include_supporting_assets,
        }),
        XeroSkillToolInput::Reload {
            source_id,
            source_kind,
        } => Ok(XeroSkillToolInput::Reload {
            source_id: source_id.map(normalize_source_id).transpose()?,
            source_kind,
        }),
        XeroSkillToolInput::CreateDynamic {
            skill_id,
            markdown,
            supporting_assets,
            source_run_id,
            source_artifact_id,
        } => {
            let skill_id = normalize_skill_id(&skill_id)?;
            let markdown = validate_non_empty_text(markdown, "markdown")?;
            if markdown.len() > XERO_SKILL_TOOL_MAX_CONTEXT_MARKDOWN_BYTES {
                return Err(CommandError::user_fixable(
                    "skill_tool_context_too_large",
                    format!(
                        "Xero requires dynamic SkillTool markdown to be {} bytes or smaller.",
                        XERO_SKILL_TOOL_MAX_CONTEXT_MARKDOWN_BYTES
                    ),
                ));
            }
            let mut normalized_assets = Vec::with_capacity(supporting_assets.len());
            if supporting_assets.len() > XERO_SKILL_TOOL_MAX_CONTEXT_ASSETS {
                return Err(CommandError::user_fixable(
                    "skill_tool_context_too_large",
                    format!(
                        "Xero rejected dynamic SkillTool input because it contained more than {XERO_SKILL_TOOL_MAX_CONTEXT_ASSETS} supporting assets."
                    ),
                ));
            }
            for asset in supporting_assets {
                let relative_path = normalize_relative_source_path(&asset.relative_path)?;
                validate_context_asset(XeroSkillToolContextAsset {
                    relative_path: relative_path.clone(),
                    sha256: "0".repeat(64),
                    bytes: asset.content.len(),
                    content: asset.content.clone(),
                })?;
                normalized_assets.push(XeroSkillToolDynamicAssetInput {
                    relative_path,
                    content: asset.content,
                });
            }
            Ok(XeroSkillToolInput::CreateDynamic {
                skill_id,
                markdown,
                supporting_assets: normalized_assets,
                source_run_id: source_run_id
                    .map(|value| validate_non_empty_text(value, "sourceRunId"))
                    .transpose()?,
                source_artifact_id: source_artifact_id
                    .map(|value| validate_non_empty_text(value, "sourceArtifactId"))
                    .transpose()?,
            })
        }
    }
}

pub fn model_may_discover_skill_source(record: &XeroSkillSourceRecord) -> bool {
    record.trust != XeroSkillTrustState::Blocked
        && !matches!(
            record.state,
            XeroSkillSourceState::Disabled | XeroSkillSourceState::Blocked
        )
}

pub fn decide_skill_tool_access(
    record: &XeroSkillSourceRecord,
    operation: XeroSkillToolOperation,
) -> CommandResult<XeroSkillToolAccessDecision> {
    let record = record.clone().validate()?;
    let source_id = record.source_id.clone();
    let model_visible = model_may_discover_skill_source(&record);

    if record.state == XeroSkillSourceState::Blocked || record.trust == XeroSkillTrustState::Blocked
    {
        return Ok(access_decision(
            operation,
            source_id,
            XeroSkillToolAccessStatus::Denied,
            false,
            Some(diagnostic(
                "skill_tool_source_blocked",
                "Xero blocked this skill source and will not expose it to the model.",
                false,
                false,
            )?),
        ));
    }

    match operation {
        XeroSkillToolOperation::List | XeroSkillToolOperation::Resolve => {
            if model_visible {
                Ok(access_decision(
                    operation,
                    source_id,
                    XeroSkillToolAccessStatus::Allowed,
                    true,
                    None,
                ))
            } else {
                Ok(access_decision(
                    operation,
                    source_id,
                    XeroSkillToolAccessStatus::Denied,
                    false,
                    Some(diagnostic(
                        "skill_tool_source_hidden",
                        "Xero will not expose disabled skill sources to the model unless a user-facing diagnostic flow asks for them.",
                        false,
                        false,
                    )?),
                ))
            }
        }
        XeroSkillToolOperation::Install | XeroSkillToolOperation::Reload => {
            if record.state == XeroSkillSourceState::Disabled {
                return Ok(access_decision(
                    operation,
                    source_id,
                    XeroSkillToolAccessStatus::Denied,
                    false,
                    Some(diagnostic(
                        "skill_tool_source_disabled",
                        "Xero requires the user to re-enable this skill source before installation or reload.",
                        false,
                        false,
                    )?),
                ));
            }
            Ok(approval_aware_access_decision(
                operation,
                source_id,
                model_visible,
                record.trust,
            )?)
        }
        XeroSkillToolOperation::Invoke => {
            if record.state != XeroSkillSourceState::Enabled {
                return Ok(access_decision(
                    operation,
                    source_id,
                    XeroSkillToolAccessStatus::Denied,
                    model_visible,
                    Some(diagnostic(
                        "skill_tool_source_not_enabled",
                        "Xero requires a skill source to be enabled before model invocation.",
                        false,
                        false,
                    )?),
                ));
            }
            Ok(approval_aware_access_decision(
                operation,
                source_id,
                true,
                record.trust,
            )?)
        }
        XeroSkillToolOperation::CreateDynamic => Ok(access_decision(
            operation,
            source_id,
            XeroSkillToolAccessStatus::Allowed,
            true,
            None,
        )),
    }
}

pub fn validate_skill_tool_context_payload(
    payload: XeroSkillToolContextPayload,
) -> CommandResult<XeroSkillToolContextPayload> {
    if payload.contract_version != XERO_SKILL_TOOL_CONTRACT_VERSION {
        return Err(CommandError::user_fixable(
            "skill_tool_contract_version_unsupported",
            format!(
                "Xero rejected SkillTool context contract version `{}` because only version `{XERO_SKILL_TOOL_CONTRACT_VERSION}` is supported.",
                payload.contract_version
            ),
        ));
    }
    if payload.supporting_assets.len() > XERO_SKILL_TOOL_MAX_CONTEXT_ASSETS {
        return Err(CommandError::user_fixable(
            "skill_tool_context_too_large",
            format!(
                "Xero rejected SkillTool context because it contained more than {XERO_SKILL_TOOL_MAX_CONTEXT_ASSETS} supporting assets."
            ),
        ));
    }

    let markdown = validate_context_document(
        payload.markdown,
        true,
        XERO_SKILL_TOOL_MAX_CONTEXT_MARKDOWN_BYTES,
    )?;
    let mut supporting_assets = Vec::with_capacity(payload.supporting_assets.len());
    for asset in payload.supporting_assets {
        supporting_assets.push(validate_context_asset(asset)?);
    }

    Ok(XeroSkillToolContextPayload {
        contract_version: XERO_SKILL_TOOL_CONTRACT_VERSION,
        source_id: normalize_source_id(payload.source_id)?,
        skill_id: normalize_skill_id(&payload.skill_id)?,
        markdown,
        supporting_assets,
    })
}

pub fn skill_tool_diagnostic_from_command_error(error: &CommandError) -> XeroSkillToolDiagnostic {
    let (message, redacted) = sanitize_skill_tool_model_text(&error.message);
    XeroSkillToolDiagnostic {
        code: if error.code.trim().is_empty() {
            "skill_tool_failed".into()
        } else {
            error.code.trim().into()
        },
        message,
        retryable: error.retryable,
        redacted,
    }
}

pub fn sanitize_skill_tool_model_text(value: &str) -> (String, bool) {
    redact_skill_tool_model_text(value)
}

pub fn validate_skill_tool_lifecycle_event(
    event: XeroSkillToolLifecycleEvent,
) -> CommandResult<XeroSkillToolLifecycleEvent> {
    if event.contract_version != XERO_SKILL_TOOL_CONTRACT_VERSION {
        return Err(CommandError::user_fixable(
            "skill_tool_contract_version_unsupported",
            format!(
                "Xero rejected SkillTool lifecycle contract version `{}` because only version `{XERO_SKILL_TOOL_CONTRACT_VERSION}` is supported.",
                event.contract_version
            ),
        ));
    }

    let detail = validate_non_empty_text(event.detail, "detail")?;
    let source_id = event.source_id.map(normalize_source_id).transpose()?;
    let skill_id = event
        .skill_id
        .as_deref()
        .map(normalize_skill_id)
        .transpose()?;

    match (&event.result, &event.diagnostic) {
        (XeroSkillToolLifecycleResult::Succeeded, Some(_)) => {
            return Err(CommandError::user_fixable(
                "skill_tool_lifecycle_invalid",
                "Xero rejected a successful SkillTool lifecycle event with failure diagnostics.",
            ));
        }
        (XeroSkillToolLifecycleResult::Failed, None)
        | (XeroSkillToolLifecycleResult::ApprovalRequired, None) => {
            return Err(CommandError::user_fixable(
                "skill_tool_lifecycle_invalid",
                "Xero rejected a SkillTool lifecycle event that requires typed diagnostics but omitted them.",
            ));
        }
        _ => {}
    }

    Ok(XeroSkillToolLifecycleEvent {
        contract_version: XERO_SKILL_TOOL_CONTRACT_VERSION,
        operation: event.operation,
        result: event.result,
        source_id,
        skill_id,
        detail,
        diagnostic: event.diagnostic,
    })
}

fn approval_aware_access_decision(
    operation: XeroSkillToolOperation,
    source_id: String,
    model_visible: bool,
    trust: XeroSkillTrustState,
) -> CommandResult<XeroSkillToolAccessDecision> {
    match trust {
        XeroSkillTrustState::Trusted | XeroSkillTrustState::UserApproved => Ok(access_decision(
            operation,
            source_id,
            XeroSkillToolAccessStatus::Allowed,
            model_visible,
            None,
        )),
        XeroSkillTrustState::ApprovalRequired | XeroSkillTrustState::Untrusted => {
            Ok(access_decision(
                operation,
                source_id,
                XeroSkillToolAccessStatus::ApprovalRequired,
                model_visible,
                Some(diagnostic(
                    "skill_tool_user_approval_required",
                    "Xero requires user approval before this skill source can be installed, reloaded, or invoked by the model.",
                    false,
                    false,
                )?),
            ))
        }
        XeroSkillTrustState::Blocked => Ok(access_decision(
            operation,
            source_id,
            XeroSkillToolAccessStatus::Denied,
            false,
            Some(diagnostic(
                "skill_tool_source_blocked",
                "Xero blocked this skill source and will not expose it to the model.",
                false,
                false,
            )?),
        )),
    }
}

fn access_decision(
    operation: XeroSkillToolOperation,
    source_id: String,
    status: XeroSkillToolAccessStatus,
    model_visible: bool,
    reason: Option<XeroSkillToolDiagnostic>,
) -> XeroSkillToolAccessDecision {
    XeroSkillToolAccessDecision {
        operation,
        source_id,
        status,
        model_visible,
        reason,
    }
}

fn diagnostic(
    code: impl Into<String>,
    message: impl Into<String>,
    retryable: bool,
    redacted: bool,
) -> CommandResult<XeroSkillToolDiagnostic> {
    Ok(XeroSkillToolDiagnostic {
        code: validate_non_empty_text(code.into(), "code")?,
        message: validate_non_empty_text(message.into(), "message")?,
        retryable,
        redacted,
    })
}

fn validate_context_document(
    document: XeroSkillToolContextDocument,
    is_markdown: bool,
    max_bytes: usize,
) -> CommandResult<XeroSkillToolContextDocument> {
    let relative_path = normalize_relative_source_path(&document.relative_path)?;
    if is_markdown && relative_path != "SKILL.md" {
        return Err(CommandError::user_fixable(
            "skill_tool_context_invalid",
            "Xero requires SkillTool markdown context to come from SKILL.md.",
        ));
    }
    if !is_markdown && relative_path == "SKILL.md" {
        return Err(CommandError::user_fixable(
            "skill_tool_context_invalid",
            "Xero requires SKILL.md to be represented as markdown context, not a supporting asset.",
        ));
    }
    validate_context_file_shape(
        &relative_path,
        &document.sha256,
        document.bytes,
        &document.content,
        max_bytes,
    )?;
    Ok(XeroSkillToolContextDocument {
        relative_path,
        sha256: document.sha256.to_ascii_lowercase(),
        bytes: document.bytes,
        content: document.content,
    })
}

fn validate_context_asset(
    asset: XeroSkillToolContextAsset,
) -> CommandResult<XeroSkillToolContextAsset> {
    let document = validate_context_document(
        XeroSkillToolContextDocument {
            relative_path: asset.relative_path,
            sha256: asset.sha256,
            bytes: asset.bytes,
            content: asset.content,
        },
        false,
        XERO_SKILL_TOOL_MAX_CONTEXT_ASSET_BYTES,
    )?;
    let extension = Path::new(&document.relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| {
            CommandError::user_fixable(
                "skill_tool_context_invalid",
                format!(
                    "Xero rejected SkillTool supporting asset `{}` because extensionless assets are not allowed into model context.",
                    document.relative_path
                ),
            )
        })?;
    if !ALLOWED_TEXT_EXTENSIONS.contains(&extension.as_str()) {
        return Err(CommandError::user_fixable(
            "skill_tool_context_invalid",
            format!(
                "Xero rejected SkillTool supporting asset `{}` because `.{extension}` assets are not allowed into model context.",
                document.relative_path
            ),
        ));
    }
    Ok(XeroSkillToolContextAsset {
        relative_path: document.relative_path,
        sha256: document.sha256,
        bytes: document.bytes,
        content: document.content,
    })
}

fn validate_context_file_shape(
    relative_path: &str,
    sha256: &str,
    declared_bytes: usize,
    content: &str,
    max_bytes: usize,
) -> CommandResult<()> {
    validate_sha256(sha256, "sha256")?;
    let actual_bytes = content.len();
    if actual_bytes == 0 {
        return Err(CommandError::user_fixable(
            "skill_tool_context_invalid",
            format!("Xero rejected empty SkillTool context file `{relative_path}`."),
        ));
    }
    if actual_bytes > max_bytes {
        return Err(CommandError::user_fixable(
            "skill_tool_context_too_large",
            format!(
                "Xero rejected SkillTool context file `{relative_path}` because it exceeded the {max_bytes} byte limit."
            ),
        ));
    }
    if declared_bytes != actual_bytes {
        return Err(CommandError::user_fixable(
            "skill_tool_context_invalid",
            format!(
                "Xero rejected SkillTool context file `{relative_path}` because declared bytes did not match content bytes."
            ),
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &'static str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.len() != 64
        || !trimmed
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(CommandError::user_fixable(
            "skill_tool_context_invalid",
            format!("Xero requires SkillTool context `{field}` values to be lowercase SHA-256 hex digests."),
        ));
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn normalize_optional_query(query: Option<String>) -> CommandResult<Option<String>> {
    match query {
        None => Ok(None),
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().count() > XERO_SKILL_TOOL_MAX_QUERY_CHARS {
                return Err(CommandError::user_fixable(
                    "skill_tool_query_too_long",
                    format!(
                        "Xero requires SkillTool list queries to be {XERO_SKILL_TOOL_MAX_QUERY_CHARS} characters or fewer."
                    ),
                ));
            }
            Ok(Some(trimmed.into()))
        }
    }
}

fn normalize_limit(limit: Option<usize>) -> CommandResult<Option<usize>> {
    match limit {
        None => Ok(Some(XERO_SKILL_TOOL_DEFAULT_LIMIT)),
        Some(value) if (1..=XERO_SKILL_TOOL_MAX_LIMIT).contains(&value) => Ok(Some(value)),
        Some(_) => Err(CommandError::user_fixable(
            "skill_tool_limit_invalid",
            format!(
                "Xero requires SkillTool list limits to be between 1 and {XERO_SKILL_TOOL_MAX_LIMIT}."
            ),
        )),
    }
}

fn normalize_source_id(value: String) -> CommandResult<String> {
    let trimmed = validate_non_empty_text(value, "sourceId")?;
    if !trimmed.starts_with("skill-source:v") {
        return Err(CommandError::user_fixable(
            "skill_tool_source_id_invalid",
            "Xero requires SkillTool source ids to use the canonical skill-source contract prefix.",
        ));
    }
    Ok(trimmed)
}

fn validate_non_empty_text(value: String, field: &'static str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(trimmed.into())
}

fn redact_skill_tool_model_text(value: &str) -> (String, bool) {
    let mut redacted = false;
    let words = value
        .split_whitespace()
        .map(|word| {
            let bare = word.trim_matches(|character: char| {
                matches!(
                    character,
                    ',' | ';' | ':' | '.' | ')' | '(' | '[' | ']' | '"' | '\''
                )
            });
            if find_prohibited_persistence_content(bare).is_some() {
                redacted = true;
                "[redacted]".to_owned()
            } else if looks_like_raw_local_path(bare) {
                redacted = true;
                word.replace(bare, "[redacted-path]")
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>();
    let joined = words.join(" ");

    if find_prohibited_persistence_content(&joined).is_some() {
        (
            "Xero redacted sensitive SkillTool failure details.".into(),
            true,
        )
    } else {
        (joined, redacted)
    }
}

fn looks_like_raw_local_path(value: &str) -> bool {
    value.starts_with("/Users/")
        || value.starts_with("/home/")
        || value.starts_with("/var/folders/")
        || value.starts_with("/tmp/")
        || value.starts_with("~/")
        || value.starts_with("\\Users\\")
        || value.contains(":\\Users\\")
}
