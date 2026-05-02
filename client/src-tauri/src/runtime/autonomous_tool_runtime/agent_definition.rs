use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};

use super::{
    tool_access_group_tools, tool_effect_class, AutonomousToolEffectClass, AutonomousToolOutput,
    AutonomousToolResult, AutonomousToolRuntime, AUTONOMOUS_TOOL_CODE_INTEL,
    AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT, AUTONOMOUS_TOOL_FIND, AUTONOMOUS_TOOL_GIT_DIFF,
    AUTONOMOUS_TOOL_GIT_STATUS, AUTONOMOUS_TOOL_HASH, AUTONOMOUS_TOOL_LIST, AUTONOMOUS_TOOL_LSP,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT, AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_SEARCH,
    AUTONOMOUS_TOOL_TOOL_SEARCH,
};
use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::project_store,
    runtime::redaction::find_prohibited_persistence_content,
};

pub const AUTONOMOUS_TOOL_AGENT_DEFINITION: &str = "agent_definition";

const AGENT_DEFINITION_SCHEMA: &str = "xero.agent_definition.v1";
const MAX_DEFINITION_ID_CHARS: usize = 80;
const MAX_DISPLAY_NAME_CHARS: usize = 80;
const MAX_SHORT_LABEL_CHARS: usize = 24;
const MAX_DESCRIPTION_CHARS: usize = 500;
const MAX_PROMPT_FIELD_CHARS: usize = 4_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousAgentDefinitionAction {
    Draft,
    Validate,
    Save,
    Update,
    Archive,
    Clone,
    List,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousAgentDefinitionRequest {
    pub action: AutonomousAgentDefinitionAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_definition_id: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousAgentDefinitionOutput {
    pub action: AutonomousAgentDefinitionAction,
    pub message: String,
    pub applied: bool,
    pub approval_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<AutonomousAgentDefinitionSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub definitions: Vec<AutonomousAgentDefinitionSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_report: Option<AutonomousAgentDefinitionValidationReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousAgentDefinitionSummary {
    pub definition_id: String,
    pub version: u32,
    pub display_name: String,
    pub short_label: String,
    pub description: String,
    pub scope: String,
    pub lifecycle_state: String,
    pub base_capability_profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousAgentDefinitionValidationStatus {
    Valid,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousAgentDefinitionValidationReport {
    pub status: AutonomousAgentDefinitionValidationStatus,
    pub diagnostics: Vec<AutonomousAgentDefinitionValidationDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousAgentDefinitionValidationDiagnostic {
    pub code: String,
    pub message: String,
    pub path: String,
}

impl AutonomousToolRuntime {
    pub fn agent_definition(
        &self,
        request: AutonomousAgentDefinitionRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.agent_definition_with_approval(request, false)
    }

    pub fn agent_definition_with_operator_approval(
        &self,
        request: AutonomousAgentDefinitionRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.agent_definition_with_approval(request, true)
    }

    fn agent_definition_with_approval(
        &self,
        request: AutonomousAgentDefinitionRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let output = match request.action {
            AutonomousAgentDefinitionAction::Draft => self.draft_agent_definition(request)?,
            AutonomousAgentDefinitionAction::Validate => self.validate_agent_definition(request)?,
            AutonomousAgentDefinitionAction::Save => {
                self.save_agent_definition(request, operator_approved)?
            }
            AutonomousAgentDefinitionAction::Update => {
                self.update_agent_definition(request, operator_approved)?
            }
            AutonomousAgentDefinitionAction::Archive => {
                self.archive_agent_definition(request, operator_approved)?
            }
            AutonomousAgentDefinitionAction::Clone => {
                self.clone_agent_definition(request, operator_approved)?
            }
            AutonomousAgentDefinitionAction::List => self.list_agent_definitions(request)?,
        };
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_AGENT_DEFINITION.into(),
            summary: output.message.clone(),
            command_result: None,
            output: AutonomousToolOutput::AgentDefinition(output),
        })
    }

    fn draft_agent_definition(
        &self,
        request: AutonomousAgentDefinitionRequest,
    ) -> CommandResult<AutonomousAgentDefinitionOutput> {
        let draft = normalize_definition_snapshot(
            required_definition(request.definition.as_ref())?,
            None,
            1,
            true,
        )?;
        let validation_report = validate_definition_snapshot(&draft);
        let summary = summary_from_snapshot(&draft)?;
        Ok(AutonomousAgentDefinitionOutput {
            action: request.action,
            message: format!(
                "Drafted agent definition `{}` for review.",
                summary.definition_id
            ),
            applied: false,
            approval_required: false,
            definition: Some(summary),
            definitions: Vec::new(),
            validation_report: Some(validation_report),
        })
    }

    fn validate_agent_definition(
        &self,
        request: AutonomousAgentDefinitionRequest,
    ) -> CommandResult<AutonomousAgentDefinitionOutput> {
        let snapshot = normalize_definition_snapshot(
            required_definition(request.definition.as_ref())?,
            request.definition_id.as_deref(),
            1,
            false,
        )?;
        let validation_report = validate_definition_snapshot(&snapshot);
        let summary = summary_from_snapshot(&snapshot)?;
        let valid = validation_report.status == AutonomousAgentDefinitionValidationStatus::Valid;
        Ok(AutonomousAgentDefinitionOutput {
            action: request.action,
            message: if valid {
                format!(
                    "Agent definition `{}` passed validation.",
                    summary.definition_id
                )
            } else {
                format!(
                    "Agent definition `{}` failed validation with {} diagnostic(s).",
                    summary.definition_id,
                    validation_report.diagnostics.len()
                )
            },
            applied: false,
            approval_required: false,
            definition: Some(summary),
            definitions: Vec::new(),
            validation_report: Some(validation_report),
        })
    }

    fn save_agent_definition(
        &self,
        request: AutonomousAgentDefinitionRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousAgentDefinitionOutput> {
        let mut snapshot = normalize_definition_snapshot(
            required_definition(request.definition.as_ref())?,
            request.definition_id.as_deref(),
            1,
            false,
        )?;
        set_snapshot_string(&mut snapshot, "lifecycleState", "active");
        let validation_report = validate_definition_snapshot(&snapshot);
        let summary = summary_from_snapshot(&snapshot)?;
        if validation_report.status != AutonomousAgentDefinitionValidationStatus::Valid {
            return Ok(invalid_output(
                request.action,
                summary,
                validation_report,
                "Agent definition failed validation and was not saved.",
            ));
        }
        ensure_custom_definition_summary(&summary)?;
        if project_store::load_agent_definition(&self.repo_root, &summary.definition_id)?.is_some()
        {
            return Err(CommandError::user_fixable(
                "agent_definition_already_exists",
                format!(
                    "Xero cannot save `{}` because an agent definition with that id already exists.",
                    summary.definition_id
                ),
            ));
        }
        if !operator_approved {
            return Ok(approval_required_output(
                request.action,
                summary,
                validation_report,
                "Saving this agent definition requires explicit operator approval.",
            ));
        }

        let now = now_timestamp();
        let saved = project_store::insert_agent_definition(
            &self.repo_root,
            &project_store::NewAgentDefinitionRecord {
                definition_id: summary.definition_id.clone(),
                version: 1,
                display_name: summary.display_name.clone(),
                short_label: summary.short_label.clone(),
                description: summary.description.clone(),
                scope: summary.scope.clone(),
                lifecycle_state: "active".into(),
                base_capability_profile: summary.base_capability_profile.clone(),
                snapshot,
                validation_report: Some(validation_report_json(&validation_report)?),
                created_at: now.clone(),
                updated_at: now,
            },
        )?;
        let saved_summary = summary_from_record(saved, None);
        Ok(AutonomousAgentDefinitionOutput {
            action: request.action,
            message: format!(
                "Saved active custom agent definition `{}` at version 1.",
                saved_summary.definition_id
            ),
            applied: true,
            approval_required: false,
            definition: Some(saved_summary),
            definitions: Vec::new(),
            validation_report: Some(validation_report),
        })
    }

    fn update_agent_definition(
        &self,
        request: AutonomousAgentDefinitionRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousAgentDefinitionOutput> {
        let definition_id =
            required_request_text(request.definition_id.as_deref(), "definitionId")?;
        let existing = load_custom_definition(&self.repo_root, &definition_id)?;
        let next_version = existing.current_version.saturating_add(1);
        let snapshot = normalize_definition_snapshot(
            required_definition(request.definition.as_ref())?,
            Some(&definition_id),
            next_version,
            false,
        )?;
        let validation_report = validate_definition_snapshot(&snapshot);
        let summary = summary_from_snapshot(&snapshot)?;
        if validation_report.status != AutonomousAgentDefinitionValidationStatus::Valid {
            return Ok(invalid_output(
                request.action,
                summary,
                validation_report,
                "Agent definition failed validation and was not updated.",
            ));
        }
        ensure_custom_definition_summary(&summary)?;
        if !operator_approved {
            return Ok(approval_required_output(
                request.action,
                summary,
                validation_report,
                "Updating this agent definition requires explicit operator approval.",
            ));
        }

        let now = now_timestamp();
        let saved = project_store::insert_agent_definition(
            &self.repo_root,
            &project_store::NewAgentDefinitionRecord {
                definition_id: summary.definition_id.clone(),
                version: next_version,
                display_name: summary.display_name.clone(),
                short_label: summary.short_label.clone(),
                description: summary.description.clone(),
                scope: summary.scope.clone(),
                lifecycle_state: summary.lifecycle_state.clone(),
                base_capability_profile: summary.base_capability_profile.clone(),
                snapshot,
                validation_report: Some(validation_report_json(&validation_report)?),
                created_at: now.clone(),
                updated_at: now,
            },
        )?;
        let saved_summary = summary_from_record(saved, None);
        Ok(AutonomousAgentDefinitionOutput {
            action: request.action,
            message: format!(
                "Updated custom agent definition `{}` to version {}.",
                saved_summary.definition_id, saved_summary.version
            ),
            applied: true,
            approval_required: false,
            definition: Some(saved_summary),
            definitions: Vec::new(),
            validation_report: Some(validation_report),
        })
    }

    fn archive_agent_definition(
        &self,
        request: AutonomousAgentDefinitionRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousAgentDefinitionOutput> {
        let definition_id =
            required_request_text(request.definition_id.as_deref(), "definitionId")?;
        let existing = load_custom_definition(&self.repo_root, &definition_id)?;
        let summary = summary_from_record(existing, None);
        if !operator_approved {
            return Ok(AutonomousAgentDefinitionOutput {
                action: request.action,
                message: "Archiving this agent definition requires explicit operator approval."
                    .into(),
                applied: false,
                approval_required: true,
                definition: Some(summary),
                definitions: Vec::new(),
                validation_report: None,
            });
        }
        let archived = project_store::archive_agent_definition(
            &self.repo_root,
            &definition_id,
            &now_timestamp(),
        )?;
        let archived_summary = summary_from_record(archived, None);
        Ok(AutonomousAgentDefinitionOutput {
            action: request.action,
            message: format!(
                "Archived custom agent definition `{}`.",
                archived_summary.definition_id
            ),
            applied: true,
            approval_required: false,
            definition: Some(archived_summary),
            definitions: Vec::new(),
            validation_report: None,
        })
    }

    fn clone_agent_definition(
        &self,
        request: AutonomousAgentDefinitionRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousAgentDefinitionOutput> {
        let source_definition_id = required_request_text(
            request.source_definition_id.as_deref(),
            "sourceDefinitionId",
        )?;
        let source = project_store::load_agent_definition(&self.repo_root, &source_definition_id)?
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "agent_definition_not_found",
                    format!("Xero could not find agent definition `{source_definition_id}`."),
                )
            })?;
        let source_version = project_store::load_agent_definition_version(
            &self.repo_root,
            &source.definition_id,
            source.current_version,
        )?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_definition_version_not_found",
                format!(
                    "Xero could not load `{}` version {} for cloning.",
                    source.definition_id, source.current_version
                ),
            )
        })?;
        let merged = merge_clone_snapshot(&source_version.snapshot, request.definition.as_ref())?;
        let mut snapshot =
            normalize_definition_snapshot(&merged, request.definition_id.as_deref(), 1, false)?;
        set_snapshot_string(&mut snapshot, "lifecycleState", "active");
        let validation_report = validate_definition_snapshot(&snapshot);
        let summary = summary_from_snapshot(&snapshot)?;
        if validation_report.status != AutonomousAgentDefinitionValidationStatus::Valid {
            return Ok(invalid_output(
                request.action,
                summary,
                validation_report,
                "Agent definition clone failed validation and was not saved.",
            ));
        }
        ensure_custom_definition_summary(&summary)?;
        if project_store::load_agent_definition(&self.repo_root, &summary.definition_id)?.is_some()
        {
            return Err(CommandError::user_fixable(
                "agent_definition_already_exists",
                format!(
                    "Xero cannot clone to `{}` because an agent definition with that id already exists.",
                    summary.definition_id
                ),
            ));
        }
        if !operator_approved {
            return Ok(approval_required_output(
                request.action,
                summary,
                validation_report,
                "Cloning this agent definition requires explicit operator approval.",
            ));
        }

        let now = now_timestamp();
        let saved = project_store::insert_agent_definition(
            &self.repo_root,
            &project_store::NewAgentDefinitionRecord {
                definition_id: summary.definition_id.clone(),
                version: 1,
                display_name: summary.display_name.clone(),
                short_label: summary.short_label.clone(),
                description: summary.description.clone(),
                scope: summary.scope.clone(),
                lifecycle_state: "active".into(),
                base_capability_profile: summary.base_capability_profile.clone(),
                snapshot,
                validation_report: Some(validation_report_json(&validation_report)?),
                created_at: now.clone(),
                updated_at: now,
            },
        )?;
        let saved_summary = summary_from_record(saved, None);
        Ok(AutonomousAgentDefinitionOutput {
            action: request.action,
            message: format!(
                "Cloned `{source_definition_id}` into custom agent definition `{}`.",
                saved_summary.definition_id
            ),
            applied: true,
            approval_required: false,
            definition: Some(saved_summary),
            definitions: Vec::new(),
            validation_report: Some(validation_report),
        })
    }

    fn list_agent_definitions(
        &self,
        request: AutonomousAgentDefinitionRequest,
    ) -> CommandResult<AutonomousAgentDefinitionOutput> {
        let definitions =
            project_store::list_agent_definitions(&self.repo_root, request.include_archived)?
                .into_iter()
                .map(|record| summary_from_record(record, None))
                .collect::<Vec<_>>();
        Ok(AutonomousAgentDefinitionOutput {
            action: request.action,
            message: format!("Listed {} agent definition(s).", definitions.len()),
            applied: false,
            approval_required: false,
            definition: None,
            definitions,
            validation_report: None,
        })
    }
}

fn required_definition(value: Option<&JsonValue>) -> CommandResult<&JsonValue> {
    value.ok_or_else(|| CommandError::invalid_request("definition"))
}

fn required_request_text(value: Option<&str>, field: &'static str) -> CommandResult<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| CommandError::invalid_request(field))
}

fn load_custom_definition(
    repo_root: &std::path::Path,
    definition_id: &str,
) -> CommandResult<project_store::AgentDefinitionRecord> {
    let definition =
        project_store::load_agent_definition(repo_root, definition_id)?.ok_or_else(|| {
            CommandError::user_fixable(
                "agent_definition_not_found",
                format!("Xero could not find custom agent definition `{definition_id}`."),
            )
        })?;
    if definition.scope == "built_in" {
        return Err(CommandError::user_fixable(
            "agent_definition_builtin_immutable",
            format!(
                "Xero cannot mutate built-in agent definition `{}`.",
                definition.definition_id
            ),
        ));
    }
    Ok(definition)
}

fn normalize_definition_snapshot(
    raw: &JsonValue,
    forced_definition_id: Option<&str>,
    version: u32,
    draft_mode: bool,
) -> CommandResult<JsonValue> {
    let object = raw
        .as_object()
        .ok_or_else(|| CommandError::invalid_request("definition"))?;
    let display_name = text_alias(object, &["displayName", "label", "name"])
        .unwrap_or_else(|| "Untitled Agent".into());
    let definition_id = forced_definition_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| text_alias(object, &["id", "definitionId"]))
        .unwrap_or_else(|| stable_agent_definition_id(&display_name));
    let short_label =
        text_alias(object, &["shortLabel"]).unwrap_or_else(|| default_short_label(&display_name));
    let description = text_alias(object, &["description"])
        .unwrap_or_else(|| format!("Custom agent definition for {display_name}."));
    let task_purpose =
        text_alias(object, &["taskPurpose", "purpose"]).unwrap_or_else(|| description.clone());
    let scope = text_alias(object, &["scope"]).unwrap_or_else(|| "global_custom".into());
    let lifecycle_state = if draft_mode {
        "draft".to_string()
    } else {
        text_alias(object, &["lifecycleState"]).unwrap_or_else(|| "active".into())
    };
    let base_capability_profile =
        text_alias(object, &["baseCapabilityProfile"]).unwrap_or_else(|| "observe_only".into());
    let default_approval_mode = text_alias(object, &["defaultApprovalMode"])
        .unwrap_or_else(|| default_approval_mode_for_profile(&base_capability_profile).into());
    let allowed_approval_modes = string_array_alias(object, &["allowedApprovalModes"])
        .unwrap_or_else(|| default_allowed_approval_modes(&base_capability_profile));
    let tool_policy = object
        .get("toolPolicy")
        .cloned()
        .unwrap_or_else(|| default_tool_policy(&base_capability_profile));
    let example_prompts = object
        .get("examplePrompts")
        .or_else(|| object.get("examples"))
        .cloned()
        .unwrap_or_else(|| json!([]));
    let refusal_escalation_cases = object
        .get("refusalEscalationCases")
        .or_else(|| object.get("escalationCases"))
        .cloned()
        .unwrap_or_else(|| json!([]));

    let mut snapshot = JsonMap::new();
    snapshot.insert(
        "schema".into(),
        JsonValue::String(AGENT_DEFINITION_SCHEMA.into()),
    );
    snapshot.insert("id".into(), JsonValue::String(definition_id));
    snapshot.insert("version".into(), json!(version));
    snapshot.insert("displayName".into(), JsonValue::String(display_name));
    snapshot.insert("shortLabel".into(), JsonValue::String(short_label));
    snapshot.insert("description".into(), JsonValue::String(description));
    snapshot.insert("taskPurpose".into(), JsonValue::String(task_purpose));
    snapshot.insert("scope".into(), JsonValue::String(scope));
    snapshot.insert("lifecycleState".into(), JsonValue::String(lifecycle_state));
    snapshot.insert(
        "baseCapabilityProfile".into(),
        JsonValue::String(base_capability_profile),
    );
    snapshot.insert(
        "defaultApprovalMode".into(),
        JsonValue::String(default_approval_mode),
    );
    snapshot.insert("allowedApprovalModes".into(), json!(allowed_approval_modes));
    snapshot.insert("toolPolicy".into(), tool_policy);
    snapshot.insert(
        "promptFragments".into(),
        object
            .get("promptFragments")
            .cloned()
            .unwrap_or_else(|| json!({})),
    );
    snapshot.insert(
        "workflowContract".into(),
        object
            .get("workflowContract")
            .cloned()
            .unwrap_or_else(|| JsonValue::String(String::new())),
    );
    snapshot.insert(
        "finalResponseContract".into(),
        object
            .get("finalResponseContract")
            .cloned()
            .unwrap_or_else(|| JsonValue::String(String::new())),
    );
    snapshot.insert(
        "projectDataPolicy".into(),
        object
            .get("projectDataPolicy")
            .cloned()
            .unwrap_or_else(default_project_data_policy),
    );
    snapshot.insert(
        "memoryCandidatePolicy".into(),
        object
            .get("memoryCandidatePolicy")
            .cloned()
            .unwrap_or_else(default_memory_candidate_policy),
    );
    snapshot.insert(
        "retrievalDefaults".into(),
        object
            .get("retrievalDefaults")
            .cloned()
            .unwrap_or_else(default_retrieval_defaults),
    );
    snapshot.insert(
        "handoffPolicy".into(),
        object
            .get("handoffPolicy")
            .cloned()
            .unwrap_or_else(default_handoff_policy),
    );
    snapshot.insert("examplePrompts".into(), example_prompts);
    snapshot.insert("refusalEscalationCases".into(), refusal_escalation_cases);

    if let Some(default_model) = object.get("defaultModel") {
        snapshot.insert("defaultModel".into(), default_model.clone());
    }
    if let Some(capabilities) = object.get("capabilities") {
        snapshot.insert("capabilities".into(), capabilities.clone());
    }
    if let Some(safety_limits) = object.get("safetyLimits") {
        snapshot.insert("safetyLimits".into(), safety_limits.clone());
    }

    Ok(JsonValue::Object(snapshot))
}

fn validate_definition_snapshot(snapshot: &JsonValue) -> AutonomousAgentDefinitionValidationReport {
    let mut diagnostics = Vec::new();
    let object = snapshot.as_object();
    validate_text_field(object, "id", MAX_DEFINITION_ID_CHARS, &mut diagnostics);
    validate_text_field(
        object,
        "displayName",
        MAX_DISPLAY_NAME_CHARS,
        &mut diagnostics,
    );
    validate_text_field(
        object,
        "shortLabel",
        MAX_SHORT_LABEL_CHARS,
        &mut diagnostics,
    );
    validate_text_field(
        object,
        "description",
        MAX_DESCRIPTION_CHARS,
        &mut diagnostics,
    );
    validate_text_field(
        object,
        "taskPurpose",
        MAX_DESCRIPTION_CHARS,
        &mut diagnostics,
    );

    let scope = snapshot_text(snapshot, "scope").unwrap_or_default();
    if !["global_custom", "project_custom"].contains(&scope.as_str()) {
        diagnostics.push(diagnostic(
            "agent_definition_scope_invalid",
            "Custom agent definitions saved by Agent Create must be global_custom or project_custom.",
            "scope",
        ));
    }
    let lifecycle_state = snapshot_text(snapshot, "lifecycleState").unwrap_or_default();
    if !["draft", "active", "archived"].contains(&lifecycle_state.as_str()) {
        diagnostics.push(diagnostic(
            "agent_definition_lifecycle_invalid",
            "Lifecycle state must be draft, active, or archived.",
            "lifecycleState",
        ));
    }
    let base_profile = snapshot_text(snapshot, "baseCapabilityProfile").unwrap_or_default();
    if !["observe_only", "engineering", "debugging", "agent_builder"]
        .contains(&base_profile.as_str())
    {
        diagnostics.push(diagnostic(
            "agent_definition_base_profile_invalid",
            "Base capability profile must be observe_only, engineering, debugging, or agent_builder.",
            "baseCapabilityProfile",
        ));
    }

    validate_approval_modes(snapshot, &base_profile, &mut diagnostics);
    validate_tool_policy(snapshot.get("toolPolicy"), &base_profile, &mut diagnostics);
    validate_required_contract_text(snapshot, "workflowContract", &mut diagnostics);
    validate_required_contract_text(snapshot, "finalResponseContract", &mut diagnostics);
    validate_examples(
        snapshot.get("examplePrompts"),
        "examplePrompts",
        &mut diagnostics,
    );
    validate_examples(
        snapshot.get("refusalEscalationCases"),
        "refusalEscalationCases",
        &mut diagnostics,
    );
    validate_policy_kinds(snapshot.get("projectDataPolicy"), &mut diagnostics);
    validate_memory_policy(snapshot.get("memoryCandidatePolicy"), &mut diagnostics);
    validate_instruction_hierarchy(snapshot, &mut diagnostics);

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

fn validate_text_field(
    object: Option<&JsonMap<String, JsonValue>>,
    field: &'static str,
    max_chars: usize,
    diagnostics: &mut Vec<AutonomousAgentDefinitionValidationDiagnostic>,
) {
    let value = object
        .and_then(|object| object.get(field))
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if value.is_empty() {
        diagnostics.push(diagnostic(
            "agent_definition_text_required",
            format!("{field} must be non-empty."),
            field,
        ));
    }
    if value.chars().count() > max_chars {
        diagnostics.push(diagnostic(
            "agent_definition_text_too_long",
            format!("{field} must be at most {max_chars} characters."),
            field,
        ));
    }
}

fn validate_approval_modes(
    snapshot: &JsonValue,
    base_profile: &str,
    diagnostics: &mut Vec<AutonomousAgentDefinitionValidationDiagnostic>,
) {
    let default_mode = snapshot_text(snapshot, "defaultApprovalMode").unwrap_or_default();
    let allowed_modes = snapshot
        .get("allowedApprovalModes")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !["suggest", "auto_edit", "yolo"].contains(&default_mode.as_str()) {
        diagnostics.push(diagnostic(
            "agent_definition_default_approval_invalid",
            "defaultApprovalMode must be suggest, auto_edit, or yolo.",
            "defaultApprovalMode",
        ));
    }
    if allowed_modes.is_empty() {
        diagnostics.push(diagnostic(
            "agent_definition_allowed_approvals_required",
            "allowedApprovalModes must include at least suggest.",
            "allowedApprovalModes",
        ));
    }
    if !allowed_modes.iter().any(|mode| mode == "suggest") {
        diagnostics.push(diagnostic(
            "agent_definition_suggest_approval_required",
            "allowedApprovalModes must include suggest.",
            "allowedApprovalModes",
        ));
    }
    if matches!(base_profile, "observe_only" | "agent_builder")
        && (default_mode != "suggest" || allowed_modes.iter().any(|mode| mode != "suggest"))
    {
        diagnostics.push(diagnostic(
            "agent_definition_approval_exceeds_profile",
            "observe_only and agent_builder profiles can only use suggest approval mode.",
            "allowedApprovalModes",
        ));
    }
}

fn validate_tool_policy(
    value: Option<&JsonValue>,
    base_profile: &str,
    diagnostics: &mut Vec<AutonomousAgentDefinitionValidationDiagnostic>,
) {
    let Some(value) = value else {
        diagnostics.push(diagnostic(
            "agent_definition_tool_policy_required",
            "toolPolicy is required.",
            "toolPolicy",
        ));
        return;
    };
    if let Some(policy) = value.as_str() {
        let allowed = match base_profile {
            "observe_only" => policy == "observe_only",
            "agent_builder" => policy == "agent_builder" || policy == "observe_only",
            "engineering" | "debugging" => ["observe_only", "engineering"].contains(&policy),
            _ => false,
        };
        if !allowed {
            diagnostics.push(diagnostic(
                "agent_definition_tool_policy_exceeds_profile",
                "String toolPolicy must not exceed the base capability profile.",
                "toolPolicy",
            ));
        }
        return;
    }
    let Some(object) = value.as_object() else {
        diagnostics.push(diagnostic(
            "agent_definition_tool_policy_invalid",
            "toolPolicy must be a string or object.",
            "toolPolicy",
        ));
        return;
    };

    for effect_class in string_array(object.get("allowedEffectClasses")) {
        if !effect_allowed_by_profile(base_profile, &effect_class) {
            diagnostics.push(diagnostic(
                "agent_definition_effect_class_exceeds_profile",
                format!(
                    "Effect class `{effect_class}` is not allowed by base profile `{base_profile}`."
                ),
                "toolPolicy.allowedEffectClasses",
            ));
        }
    }
    for group in string_array(object.get("allowedToolGroups")) {
        match tool_access_group_tools(&group) {
            Some(tools) => {
                for tool in tools {
                    if !tool_allowed_by_profile(base_profile, tool) {
                        diagnostics.push(diagnostic(
                            "agent_definition_tool_group_exceeds_profile",
                            format!(
                                "Tool group `{group}` includes `{tool}`, which is not allowed by `{base_profile}`."
                            ),
                            "toolPolicy.allowedToolGroups",
                        ));
                    }
                }
            }
            None => diagnostics.push(diagnostic(
                "agent_definition_tool_group_unknown",
                format!("Tool group `{group}` is not known to Xero."),
                "toolPolicy.allowedToolGroups",
            )),
        }
    }
    for tool in string_array(object.get("allowedTools")) {
        if !tool_allowed_by_profile(base_profile, &tool) {
            diagnostics.push(diagnostic(
                "agent_definition_tool_exceeds_profile",
                format!("Tool `{tool}` is not allowed by base profile `{base_profile}`."),
                "toolPolicy.allowedTools",
            ));
        }
    }
    for (field, effect_class) in [
        ("externalServiceAllowed", "external_service"),
        ("browserControlAllowed", "browser_control"),
        ("skillRuntimeAllowed", "skill_runtime"),
        ("subagentAllowed", "agent_delegation"),
        ("commandAllowed", "command"),
        ("destructiveWriteAllowed", "destructive_write"),
    ] {
        if object
            .get(field)
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
            && !effect_allowed_by_profile(base_profile, effect_class)
        {
            diagnostics.push(diagnostic(
                "agent_definition_tool_policy_flag_exceeds_profile",
                format!("{field} is not allowed by base profile `{base_profile}`."),
                format!("toolPolicy.{field}"),
            ));
        }
    }
}

fn validate_required_contract_text(
    snapshot: &JsonValue,
    field: &'static str,
    diagnostics: &mut Vec<AutonomousAgentDefinitionValidationDiagnostic>,
) {
    let content = snapshot
        .get(field)
        .map(render_value_text)
        .unwrap_or_default();
    if content.trim().is_empty() {
        diagnostics.push(diagnostic(
            "agent_definition_contract_required",
            format!("{field} must be non-empty."),
            field,
        ));
    }
    if content.chars().count() > MAX_PROMPT_FIELD_CHARS {
        diagnostics.push(diagnostic(
            "agent_definition_contract_too_long",
            format!("{field} must be at most {MAX_PROMPT_FIELD_CHARS} characters."),
            field,
        ));
    }
}

fn validate_examples(
    value: Option<&JsonValue>,
    path: &'static str,
    diagnostics: &mut Vec<AutonomousAgentDefinitionValidationDiagnostic>,
) {
    let count = value
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| !render_value_text(item).trim().is_empty())
                .count()
        })
        .unwrap_or(0);
    if count < 3 {
        diagnostics.push(diagnostic(
            "agent_definition_examples_required",
            format!("{path} must include at least three entries."),
            path,
        ));
    }
}

fn validate_policy_kinds(
    value: Option<&JsonValue>,
    diagnostics: &mut Vec<AutonomousAgentDefinitionValidationDiagnostic>,
) {
    let known = [
        "agent_handoff",
        "project_fact",
        "decision",
        "constraint",
        "plan",
        "finding",
        "verification",
        "question",
        "artifact",
        "context_note",
        "diagnostic",
    ];
    if let Some(object) = value.and_then(JsonValue::as_object) {
        for kind in string_array(object.get("recordKinds")) {
            if !known.contains(&kind.as_str()) {
                diagnostics.push(diagnostic(
                    "agent_definition_project_record_kind_unknown",
                    format!("Project record kind `{kind}` is not known to Xero."),
                    "projectDataPolicy.recordKinds",
                ));
            }
        }
    }
}

fn validate_memory_policy(
    value: Option<&JsonValue>,
    diagnostics: &mut Vec<AutonomousAgentDefinitionValidationDiagnostic>,
) {
    let known = [
        "project_fact",
        "user_preference",
        "decision",
        "session_summary",
        "troubleshooting",
    ];
    if let Some(object) = value.and_then(JsonValue::as_object) {
        for kind in string_array(object.get("memoryKinds")) {
            if !known.contains(&kind.as_str()) {
                diagnostics.push(diagnostic(
                    "agent_definition_memory_kind_unknown",
                    format!("Memory kind `{kind}` is not known to Xero."),
                    "memoryCandidatePolicy.memoryKinds",
                ));
            }
        }
    }
}

fn validate_instruction_hierarchy(
    snapshot: &JsonValue,
    diagnostics: &mut Vec<AutonomousAgentDefinitionValidationDiagnostic>,
) {
    let text = render_value_text(snapshot);
    if let Some(secret_hint) = find_prohibited_persistence_content(&text) {
        diagnostics.push(diagnostic(
            "agent_definition_secret_like_content",
            format!("Definition content contains prohibited secret-like material: {secret_hint}."),
            "definition",
        ));
    }
    let lowered = text.to_ascii_lowercase();
    for phrase in [
        "ignore previous instructions",
        "ignore all previous instructions",
        "override system",
        "bypass approval",
        "disable tool policy",
        "reveal hidden prompt",
        "reveal system prompt",
        "exfiltrate secret",
    ] {
        if lowered.contains(phrase) {
            diagnostics.push(diagnostic(
                "agent_definition_instruction_hierarchy_violation",
                format!("Definition content cannot contain instruction-hierarchy override phrase `{phrase}`."),
                "definition",
            ));
        }
    }
}

fn tool_allowed_by_profile(base_profile: &str, tool: &str) -> bool {
    if tool == AUTONOMOUS_TOOL_AGENT_DEFINITION {
        return base_profile == "agent_builder";
    }
    effect_allowed_by_profile(base_profile, tool_effect_class(tool).as_str())
}

fn effect_allowed_by_profile(base_profile: &str, effect_class: &str) -> bool {
    match base_profile {
        "observe_only" => effect_class == AutonomousToolEffectClass::Observe.as_str(),
        "agent_builder" => matches!(effect_class, "observe" | "runtime_state"),
        "engineering" | "debugging" => matches!(
            effect_class,
            "observe"
                | "runtime_state"
                | "write"
                | "destructive_write"
                | "command"
                | "process_control"
                | "browser_control"
                | "device_control"
                | "external_service"
                | "skill_runtime"
                | "agent_delegation"
        ),
        _ => false,
    }
}

fn ensure_custom_definition_summary(
    summary: &AutonomousAgentDefinitionSummary,
) -> CommandResult<()> {
    if summary.scope == "built_in" {
        return Err(CommandError::user_fixable(
            "agent_definition_builtin_scope_forbidden",
            "Agent Create cannot save or mutate built-in agent definitions.",
        ));
    }
    Ok(())
}

fn invalid_output(
    action: AutonomousAgentDefinitionAction,
    summary: AutonomousAgentDefinitionSummary,
    validation_report: AutonomousAgentDefinitionValidationReport,
    message: &'static str,
) -> AutonomousAgentDefinitionOutput {
    AutonomousAgentDefinitionOutput {
        action,
        message: message.into(),
        applied: false,
        approval_required: false,
        definition: Some(summary),
        definitions: Vec::new(),
        validation_report: Some(validation_report),
    }
}

fn approval_required_output(
    action: AutonomousAgentDefinitionAction,
    summary: AutonomousAgentDefinitionSummary,
    validation_report: AutonomousAgentDefinitionValidationReport,
    message: &'static str,
) -> AutonomousAgentDefinitionOutput {
    AutonomousAgentDefinitionOutput {
        action,
        message: message.into(),
        applied: false,
        approval_required: true,
        definition: Some(summary),
        definitions: Vec::new(),
        validation_report: Some(validation_report),
    }
}

fn validation_report_json(
    report: &AutonomousAgentDefinitionValidationReport,
) -> CommandResult<JsonValue> {
    serde_json::to_value(report).map_err(|error| {
        CommandError::system_fault(
            "agent_definition_validation_report_serialize_failed",
            format!("Xero could not serialize agent-definition validation output: {error}"),
        )
    })
}

fn summary_from_record(
    record: project_store::AgentDefinitionRecord,
    snapshot: Option<JsonValue>,
) -> AutonomousAgentDefinitionSummary {
    AutonomousAgentDefinitionSummary {
        definition_id: record.definition_id,
        version: record.current_version,
        display_name: record.display_name,
        short_label: record.short_label,
        description: record.description,
        scope: record.scope,
        lifecycle_state: record.lifecycle_state,
        base_capability_profile: record.base_capability_profile,
        snapshot,
    }
}

fn summary_from_snapshot(snapshot: &JsonValue) -> CommandResult<AutonomousAgentDefinitionSummary> {
    Ok(AutonomousAgentDefinitionSummary {
        definition_id: snapshot_required_text(snapshot, "id")?,
        version: snapshot
            .get("version")
            .and_then(JsonValue::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(1),
        display_name: snapshot_required_text(snapshot, "displayName")?,
        short_label: snapshot_required_text(snapshot, "shortLabel")?,
        description: snapshot_required_text(snapshot, "description")?,
        scope: snapshot_required_text(snapshot, "scope")?,
        lifecycle_state: snapshot_required_text(snapshot, "lifecycleState")?,
        base_capability_profile: snapshot_required_text(snapshot, "baseCapabilityProfile")?,
        snapshot: Some(snapshot.clone()),
    })
}

fn snapshot_required_text(snapshot: &JsonValue, field: &'static str) -> CommandResult<String> {
    snapshot_text(snapshot, field)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CommandError::invalid_request(field))
}

fn snapshot_text(snapshot: &JsonValue, field: &'static str) -> Option<String> {
    snapshot
        .get(field)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn set_snapshot_string(snapshot: &mut JsonValue, field: &'static str, value: &'static str) {
    if let Some(object) = snapshot.as_object_mut() {
        object.insert(field.into(), JsonValue::String(value.into()));
    }
}

fn text_alias(object: &JsonMap<String, JsonValue>, aliases: &[&str]) -> Option<String> {
    aliases.iter().find_map(|alias| {
        object
            .get(*alias)
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn string_array_alias(
    object: &JsonMap<String, JsonValue>,
    aliases: &[&str],
) -> Option<Vec<String>> {
    aliases
        .iter()
        .find_map(|alias| object.get(*alias).map(|value| string_array(Some(value))))
        .filter(|values| !values.is_empty())
}

fn string_array(value: Option<&JsonValue>) -> Vec<String> {
    value
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn render_value_text(value: &JsonValue) -> String {
    match value {
        JsonValue::String(value) => value.clone(),
        JsonValue::Array(items) => items
            .iter()
            .map(render_value_text)
            .collect::<Vec<_>>()
            .join("\n"),
        JsonValue::Object(object) => object
            .values()
            .map(render_value_text)
            .collect::<Vec<_>>()
            .join("\n"),
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => value.to_string(),
    }
}

fn diagnostic(
    code: impl Into<String>,
    message: impl Into<String>,
    path: impl Into<String>,
) -> AutonomousAgentDefinitionValidationDiagnostic {
    AutonomousAgentDefinitionValidationDiagnostic {
        code: code.into(),
        message: message.into(),
        path: path.into(),
    }
}

fn stable_agent_definition_id(display_name: &str) -> String {
    let mut id = String::new();
    let mut last_was_separator = false;
    for character in display_name.chars() {
        if character.is_ascii_alphanumeric() {
            id.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator && !id.is_empty() {
            id.push('_');
            last_was_separator = true;
        }
        if id.len() >= MAX_DEFINITION_ID_CHARS {
            break;
        }
    }
    let id = id.trim_matches('_').to_string();
    if id.is_empty() {
        "custom_agent".into()
    } else {
        id
    }
}

fn default_short_label(display_name: &str) -> String {
    let trimmed = display_name.trim();
    if trimmed.chars().count() <= MAX_SHORT_LABEL_CHARS {
        return trimmed.to_string();
    }
    trimmed.chars().take(MAX_SHORT_LABEL_CHARS).collect()
}

fn default_approval_mode_for_profile(profile: &str) -> &'static str {
    match profile {
        "engineering" | "debugging" => "suggest",
        "agent_builder" | "observe_only" => "suggest",
        _ => "suggest",
    }
}

fn default_allowed_approval_modes(profile: &str) -> Vec<String> {
    match profile {
        "engineering" | "debugging" => ["suggest", "auto_edit", "yolo"]
            .into_iter()
            .map(ToOwned::to_owned)
            .collect(),
        _ => vec!["suggest".into()],
    }
}

fn default_tool_policy(profile: &str) -> JsonValue {
    match profile {
        "engineering" | "debugging" => json!({
            "allowedEffectClasses": ["observe", "runtime_state", "write", "destructive_write", "command", "process_control"],
            "allowedToolGroups": ["core", "mutation", "command_readonly"],
            "allowedTools": [],
            "deniedTools": [],
            "externalServiceAllowed": false,
            "browserControlAllowed": false,
            "skillRuntimeAllowed": false,
            "subagentAllowed": false,
            "commandAllowed": true,
            "destructiveWriteAllowed": true
        }),
        "agent_builder" => json!({
            "allowedEffectClasses": ["observe", "runtime_state"],
            "allowedToolGroups": ["core", "agent_builder"],
            "allowedTools": [AUTONOMOUS_TOOL_AGENT_DEFINITION],
            "deniedTools": [],
            "externalServiceAllowed": false,
            "browserControlAllowed": false,
            "skillRuntimeAllowed": false,
            "subagentAllowed": false,
            "commandAllowed": false,
            "destructiveWriteAllowed": false
        }),
        _ => json!({
            "allowedEffectClasses": ["observe"],
            "allowedToolGroups": [],
            "allowedTools": [
                AUTONOMOUS_TOOL_READ,
                AUTONOMOUS_TOOL_SEARCH,
                AUTONOMOUS_TOOL_FIND,
                AUTONOMOUS_TOOL_GIT_STATUS,
                AUTONOMOUS_TOOL_GIT_DIFF,
                AUTONOMOUS_TOOL_LIST,
                AUTONOMOUS_TOOL_HASH,
                AUTONOMOUS_TOOL_CODE_INTEL,
                AUTONOMOUS_TOOL_LSP,
                AUTONOMOUS_TOOL_TOOL_SEARCH,
                AUTONOMOUS_TOOL_PROJECT_CONTEXT,
                AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT
            ],
            "deniedTools": [],
            "externalServiceAllowed": false,
            "browserControlAllowed": false,
            "skillRuntimeAllowed": false,
            "subagentAllowed": false,
            "commandAllowed": false,
            "destructiveWriteAllowed": false
        }),
    }
}

fn default_project_data_policy() -> JsonValue {
    json!({
        "recordKinds": ["project_fact", "decision", "constraint", "plan", "question", "context_note", "diagnostic"],
        "structuredSchemas": ["xero.project_record.v1"]
    })
}

fn default_memory_candidate_policy() -> JsonValue {
    json!({
        "memoryKinds": ["project_fact", "user_preference", "decision", "session_summary", "troubleshooting"],
        "reviewRequired": true
    })
}

fn default_retrieval_defaults() -> JsonValue {
    json!({
        "enabled": true,
        "recordKinds": ["project_fact", "decision", "constraint", "plan", "finding", "question", "context_note", "diagnostic"],
        "memoryKinds": ["project_fact", "user_preference", "decision", "session_summary", "troubleshooting"],
        "limit": 6
    })
}

fn default_handoff_policy() -> JsonValue {
    json!({
        "enabled": true,
        "preserveDefinitionVersion": true
    })
}

fn merge_clone_snapshot(
    source_snapshot: &JsonValue,
    override_definition: Option<&JsonValue>,
) -> CommandResult<JsonValue> {
    let mut merged = source_snapshot
        .as_object()
        .cloned()
        .ok_or_else(|| CommandError::invalid_request("sourceDefinitionSnapshot"))?;
    let source_id = source_snapshot
        .get("id")
        .and_then(JsonValue::as_str)
        .map(stable_agent_definition_id)
        .unwrap_or_else(|| "custom_agent".into());
    let source_display_name = source_snapshot
        .get("displayName")
        .and_then(JsonValue::as_str)
        .unwrap_or("Custom Agent");
    merged.remove("validationReport");
    merged.insert("id".into(), JsonValue::String(format!("{source_id}_copy")));
    merged.insert(
        "displayName".into(),
        JsonValue::String(format!("{source_display_name} Copy")),
    );
    merged.insert("version".into(), json!(1));
    merged.insert("scope".into(), JsonValue::String("global_custom".into()));
    merged.insert("lifecycleState".into(), JsonValue::String("active".into()));
    if let Some(override_definition) = override_definition.and_then(JsonValue::as_object) {
        for (key, value) in override_definition {
            merged.insert(key.clone(), value.clone());
        }
    }
    Ok(JsonValue::Object(merged))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{fs, path::Path};

    use rusqlite::{params, Connection};

    use crate::{
        commands::{RuntimeAgentIdDto, RuntimeRunApprovalModeDto, RuntimeRunControlInputDto},
        db::{configure_connection, database_path_for_repo, migrations::migrations},
        runtime::{
            agent_core::runtime_controls_from_request, AutonomousToolRequest, ToolRegistry,
            ToolRegistryOptions, FAKE_PROVIDER_ID, OPENAI_CODEX_PROVIDER_ID,
        },
    };

    fn create_project_database(repo_root: &Path, project_id: &str) {
        let database_path = database_path_for_repo(repo_root);
        fs::create_dir_all(database_path.parent().expect("database parent"))
            .expect("create database dir");
        let mut connection = Connection::open(&database_path).expect("open project database");
        configure_connection(&connection).expect("configure project database");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate project database");
        connection
            .execute(
                "INSERT INTO projects (id, name, description, milestone) VALUES (?1, 'Project', '', '')",
                params![project_id],
            )
            .expect("insert project");
        connection
            .execute(
                r#"
                INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo)
                VALUES ('repo-1', ?1, ?2, 'Project', 'main', 'abc123', 1)
                "#,
                params![project_id, repo_root.to_string_lossy().as_ref()],
            )
            .expect("insert repository");
    }

    fn agent_create_runtime(repo_root: &Path) -> AutonomousToolRuntime {
        let controls = runtime_controls_from_request(Some(&RuntimeRunControlInputDto {
            runtime_agent_id: RuntimeAgentIdDto::AgentCreate,
            agent_definition_id: None,
            provider_profile_id: Some(FAKE_PROVIDER_ID.into()),
            model_id: OPENAI_CODEX_PROVIDER_ID.into(),
            thinking_effort: None,
            approval_mode: RuntimeRunApprovalModeDto::Suggest,
            plan_mode_required: false,
        }));
        AutonomousToolRuntime::new(repo_root)
            .expect("runtime")
            .with_runtime_run_controls(controls)
    }

    fn valid_observe_only_definition() -> JsonValue {
        json!({
            "id": "release_notes_helper",
            "displayName": "Release Notes Helper",
            "shortLabel": "Release",
            "description": "Draft release notes from reviewed project context without changing repository files.",
            "taskPurpose": "Answer release-note questions using source-cited project context and approved memory.",
            "scope": "project_custom",
            "baseCapabilityProfile": "observe_only",
            "defaultApprovalMode": "suggest",
            "allowedApprovalModes": ["suggest"],
            "toolPolicy": {
                "allowedEffectClasses": ["observe"],
                "allowedToolGroups": [],
                "allowedTools": ["read", "search", "find", "git_status", "git_diff", "project_context", "tool_search"],
                "deniedTools": ["write", "patch", "command", "browser", "emulator"],
                "externalServiceAllowed": false,
                "browserControlAllowed": false,
                "skillRuntimeAllowed": false,
                "subagentAllowed": false,
                "commandAllowed": false,
                "destructiveWriteAllowed": false
            },
            "workflowContract": "Clarify the release range, retrieve relevant reviewed context, draft concise notes, and cite uncertainty.",
            "finalResponseContract": "Return release notes grouped by user-visible changes, fixes, risks, and unknowns.",
            "projectDataPolicy": {
                "recordKinds": ["project_fact", "decision", "constraint", "context_note"],
                "structuredSchemas": ["xero.project_record.v1"]
            },
            "memoryCandidatePolicy": {
                "memoryKinds": ["project_fact", "decision", "session_summary"],
                "reviewRequired": true
            },
            "retrievalDefaults": {
                "enabled": true,
                "recordKinds": ["project_fact", "decision", "constraint", "context_note"],
                "memoryKinds": ["project_fact", "decision", "session_summary"],
                "limit": 6
            },
            "handoffPolicy": {
                "enabled": true,
                "preserveDefinitionVersion": true
            },
            "examplePrompts": [
                "Draft release notes for the current milestone.",
                "Summarize user-visible fixes from reviewed context.",
                "List release risks that still need confirmation."
            ],
            "refusalEscalationCases": [
                "Refuse to edit files or run commands.",
                "Escalate when release context is missing.",
                "Refuse to invent unreviewed release claims."
            ]
        })
    }

    #[test]
    fn agent_create_saves_valid_observe_only_definition_without_repo_mutation() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        fs::write(repo_root.join("README.md"), "project file\n").expect("repo file");
        create_project_database(&repo_root, "project-agent-create");
        let before_repo_entries = fs::read_dir(&repo_root).expect("read repo").count();
        let runtime = agent_create_runtime(&repo_root);
        let request = AutonomousAgentDefinitionRequest {
            action: AutonomousAgentDefinitionAction::Save,
            definition_id: None,
            source_definition_id: None,
            include_archived: false,
            definition: Some(valid_observe_only_definition()),
        };

        let unapproved = runtime
            .agent_definition(request.clone())
            .expect("unapproved save response");
        let AutonomousToolOutput::AgentDefinition(unapproved_output) = unapproved.output else {
            panic!("expected agent definition output");
        };
        assert!(!unapproved_output.applied);
        assert!(unapproved_output.approval_required);
        assert!(
            project_store::load_agent_definition(&repo_root, "release_notes_helper")
                .expect("load definition")
                .is_none()
        );

        let approved = runtime
            .agent_definition_with_operator_approval(request)
            .expect("approved save");
        let AutonomousToolOutput::AgentDefinition(output) = approved.output else {
            panic!("expected agent definition output");
        };
        assert!(output.applied);
        assert!(!output.approval_required);
        assert_eq!(
            output
                .validation_report
                .as_ref()
                .expect("validation report")
                .status,
            AutonomousAgentDefinitionValidationStatus::Valid
        );

        let saved = project_store::load_agent_definition(&repo_root, "release_notes_helper")
            .expect("load saved")
            .expect("saved definition");
        assert_eq!(saved.scope, "project_custom");
        assert_eq!(saved.lifecycle_state, "active");
        assert_eq!(saved.base_capability_profile, "observe_only");
        let saved_version =
            project_store::load_agent_definition_version(&repo_root, "release_notes_helper", 1)
                .expect("load saved version")
                .expect("saved version");
        assert_eq!(saved_version.snapshot["id"], json!("release_notes_helper"));
        assert_eq!(
            database_path_for_repo(&repo_root).file_name().unwrap(),
            "state.db"
        );
        assert_eq!(
            fs::read_dir(&repo_root).expect("read repo").count(),
            before_repo_entries
        );
        assert_eq!(
            fs::read_to_string(repo_root.join("README.md")).expect("read repo file"),
            "project file\n"
        );
    }

    #[test]
    fn agent_definition_tool_is_only_registered_for_agent_create() {
        for agent_id in [
            RuntimeAgentIdDto::Ask,
            RuntimeAgentIdDto::Engineer,
            RuntimeAgentIdDto::Debug,
        ] {
            let registry = ToolRegistry::for_tool_names_with_options(
                [AUTONOMOUS_TOOL_AGENT_DEFINITION.to_string()].into(),
                ToolRegistryOptions {
                    runtime_agent_id: agent_id,
                    ..ToolRegistryOptions::default()
                },
            );
            assert!(registry
                .descriptor(AUTONOMOUS_TOOL_AGENT_DEFINITION)
                .is_none());
        }

        let registry = ToolRegistry::for_tool_names_with_options(
            [AUTONOMOUS_TOOL_AGENT_DEFINITION.to_string()].into(),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::AgentCreate,
                ..ToolRegistryOptions::default()
            },
        );
        let request = registry
            .decode_call(&crate::runtime::AgentToolCall {
                tool_call_id: "call-agent-definition".into(),
                tool_name: AUTONOMOUS_TOOL_AGENT_DEFINITION.into(),
                input: json!({
                    "action": "validate",
                    "definition": valid_observe_only_definition()
                }),
            })
            .expect("decode agent definition tool");
        assert!(matches!(request, AutonomousToolRequest::AgentDefinition(_)));
    }

    #[test]
    fn observe_only_validation_rejects_mutating_tool_policy() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        create_project_database(&repo_root, "project-agent-validation");
        let runtime = agent_create_runtime(&repo_root);
        let mut definition = valid_observe_only_definition();
        definition["toolPolicy"]["allowedTools"] = json!(["read", "write"]);
        let result = runtime
            .agent_definition(AutonomousAgentDefinitionRequest {
                action: AutonomousAgentDefinitionAction::Validate,
                definition_id: None,
                source_definition_id: None,
                include_archived: false,
                definition: Some(definition),
            })
            .expect("validation response");
        let AutonomousToolOutput::AgentDefinition(output) = result.output else {
            panic!("expected agent definition output");
        };
        let report = output.validation_report.expect("validation report");
        assert_eq!(
            report.status,
            AutonomousAgentDefinitionValidationStatus::Invalid
        );
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "agent_definition_tool_exceeds_profile"));
        assert!(
            project_store::load_agent_definition(&repo_root, "release_notes_helper")
                .expect("load definition")
                .is_none()
        );
    }
}
