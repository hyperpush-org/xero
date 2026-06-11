use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use super::{AutonomousToolOutput, AutonomousToolResult, AutonomousToolRuntime};
use crate::{
    auth::now_timestamp,
    commands::{
        contracts::workflows::{
            WorkflowDefinitionDto, WorkflowDefinitionSummaryDto, WorkflowValidationReportDto,
            WorkflowValidationStatusDto,
        },
        CommandError, CommandResult,
    },
    db::project_store,
    runtime::workflow_orchestrator,
};

pub const AUTONOMOUS_TOOL_WORKFLOW_DEFINITION: &str = "workflow_definition";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousWorkflowDefinitionAction {
    Draft,
    Validate,
    Save,
    Update,
    List,
    Get,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWorkflowDefinitionRequest {
    pub action: AutonomousWorkflowDefinitionAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWorkflowDefinitionOutput {
    pub action: AutonomousWorkflowDefinitionAction,
    pub message: String,
    pub applied: bool,
    pub approval_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<AutonomousWorkflowDefinitionSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub definitions: Vec<WorkflowDefinitionSummaryDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_report: Option<WorkflowValidationReportDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_review: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWorkflowDefinitionSummary {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub description: String,
    pub version: u32,
    pub start_node_id: String,
    pub node_count: usize,
    pub edge_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<JsonValue>,
}

impl AutonomousToolRuntime {
    pub fn workflow_definition(
        &self,
        request: AutonomousWorkflowDefinitionRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.workflow_definition_with_approval(request, false)
    }

    pub fn workflow_definition_with_operator_approval(
        &self,
        request: AutonomousWorkflowDefinitionRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.workflow_definition_with_approval(request, true)
    }

    fn workflow_definition_with_approval(
        &self,
        request: AutonomousWorkflowDefinitionRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let output = match request.action {
            AutonomousWorkflowDefinitionAction::Draft => self.draft_workflow_definition(request)?,
            AutonomousWorkflowDefinitionAction::Validate => {
                self.validate_workflow_definition_tool(request)?
            }
            AutonomousWorkflowDefinitionAction::Save => {
                self.save_workflow_definition(request, operator_approved)?
            }
            AutonomousWorkflowDefinitionAction::Update => {
                self.update_workflow_definition(request, operator_approved)?
            }
            AutonomousWorkflowDefinitionAction::List => self.list_workflow_definitions(request)?,
            AutonomousWorkflowDefinitionAction::Get => self.get_workflow_definition(request)?,
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_WORKFLOW_DEFINITION.into(),
            summary: output.message.clone(),
            command_result: None,
            output: AutonomousToolOutput::WorkflowDefinition(output),
        })
    }

    fn draft_workflow_definition(
        &self,
        request: AutonomousWorkflowDefinitionRequest,
    ) -> CommandResult<AutonomousWorkflowDefinitionOutput> {
        let definition = self.required_workflow_definition(
            request.definition.as_ref(),
            request.project_id.as_deref(),
        )?;
        self.ensure_request_project_matches_definition(
            request.project_id.as_deref(),
            &definition.project_id,
        )?;
        let validation_report = workflow_orchestrator::validate_workflow_definition_with_registry(
            &self.repo_root,
            &definition,
        );
        let summary = workflow_summary_from_definition(&definition, true)?;

        Ok(AutonomousWorkflowDefinitionOutput {
            action: request.action,
            message: format!("Drafted Workflow definition `{}` for review.", summary.id),
            applied: false,
            approval_required: false,
            definition: Some(summary),
            definitions: Vec::new(),
            validation_report: Some(validation_report),
            approval_review: None,
        })
    }

    fn validate_workflow_definition_tool(
        &self,
        request: AutonomousWorkflowDefinitionRequest,
    ) -> CommandResult<AutonomousWorkflowDefinitionOutput> {
        let definition = self.required_workflow_definition(
            request.definition.as_ref(),
            request.project_id.as_deref(),
        )?;
        self.ensure_request_project_matches_definition(
            request.project_id.as_deref(),
            &definition.project_id,
        )?;
        let validation_report = workflow_orchestrator::validate_workflow_definition_with_registry(
            &self.repo_root,
            &definition,
        );
        let summary = workflow_summary_from_definition(&definition, true)?;
        let valid = validation_report.status == WorkflowValidationStatusDto::Valid;

        Ok(AutonomousWorkflowDefinitionOutput {
            action: request.action,
            message: if valid {
                format!("Workflow definition `{}` passed validation.", summary.id)
            } else {
                format!(
                    "Workflow definition `{}` failed validation with {} diagnostic(s).",
                    summary.id,
                    validation_report.diagnostics.len()
                )
            },
            applied: false,
            approval_required: false,
            definition: Some(summary),
            definitions: Vec::new(),
            validation_report: Some(validation_report),
            approval_review: None,
        })
    }

    fn save_workflow_definition(
        &self,
        request: AutonomousWorkflowDefinitionRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousWorkflowDefinitionOutput> {
        let definition = self.required_workflow_definition(
            request.definition.as_ref(),
            request.project_id.as_deref(),
        )?;
        self.ensure_request_project_matches_definition(
            request.project_id.as_deref(),
            &definition.project_id,
        )?;
        let validation_report = workflow_orchestrator::validate_workflow_definition_with_registry(
            &self.repo_root,
            &definition,
        );
        let summary = workflow_summary_from_definition(&definition, true)?;
        if validation_report.status != WorkflowValidationStatusDto::Valid {
            return Ok(invalid_workflow_output(
                request.action,
                summary,
                validation_report,
                "Workflow definition failed validation and was not saved.",
            ));
        }
        if project_store::get_workflow_definition(
            &self.repo_root,
            &definition.project_id,
            &definition.id,
        )?
        .is_some()
        {
            return Err(CommandError::user_fixable(
                "workflow_definition_already_exists",
                format!(
                    "Xero cannot save `{}` because a Workflow definition with that id already exists.",
                    definition.id
                ),
            ));
        }
        if !operator_approved {
            return Ok(approval_required_workflow_output(
                request.action,
                summary,
                validation_report,
                "Saving this Workflow definition requires explicit operator approval.",
                Some(build_workflow_definition_review("save", None, &definition)?),
            ));
        }

        let saved = project_store::create_workflow_definition(&self.repo_root, &definition)?;
        let saved_summary = workflow_summary_from_definition(&saved, true)?;
        Ok(AutonomousWorkflowDefinitionOutput {
            action: request.action,
            message: format!(
                "Saved Workflow definition `{}` at version {}.",
                saved_summary.id, saved_summary.version
            ),
            applied: true,
            approval_required: false,
            definition: Some(saved_summary),
            definitions: Vec::new(),
            validation_report: Some(validation_report),
            approval_review: None,
        })
    }

    fn update_workflow_definition(
        &self,
        request: AutonomousWorkflowDefinitionRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousWorkflowDefinitionOutput> {
        let workflow_id = request
            .workflow_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let definition = self.required_workflow_definition(
            request.definition.as_ref(),
            request.project_id.as_deref(),
        )?;
        self.ensure_request_project_matches_definition(
            request.project_id.as_deref(),
            &definition.project_id,
        )?;
        let workflow_id = workflow_id.unwrap_or_else(|| definition.id.clone());
        if workflow_id != definition.id {
            return Err(CommandError::user_fixable(
                "workflow_definition_id_mismatch",
                "Xero refused to update a Workflow because workflowId and definition.id differ.",
            ));
        }
        let existing = project_store::get_workflow_definition(
            &self.repo_root,
            &definition.project_id,
            &workflow_id,
        )?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_definition_not_found",
                format!("Xero could not find Workflow `{workflow_id}`."),
            )
        })?;
        let validation_report = workflow_orchestrator::validate_workflow_definition_with_registry(
            &self.repo_root,
            &definition,
        );
        let summary = workflow_summary_from_definition(&definition, true)?;
        if validation_report.status != WorkflowValidationStatusDto::Valid {
            return Ok(invalid_workflow_output(
                request.action,
                summary,
                validation_report,
                "Workflow definition failed validation and was not updated.",
            ));
        }
        if !operator_approved {
            return Ok(approval_required_workflow_output(
                request.action,
                summary,
                validation_report,
                "Updating this Workflow definition requires explicit operator approval.",
                Some(build_workflow_definition_review(
                    "update",
                    Some(&existing),
                    &definition,
                )?),
            ));
        }

        let saved =
            project_store::update_workflow_definition(&self.repo_root, &workflow_id, &definition)?;
        let saved_summary = workflow_summary_from_definition(&saved, true)?;
        Ok(AutonomousWorkflowDefinitionOutput {
            action: request.action,
            message: format!(
                "Updated Workflow definition `{}` to version {}.",
                saved_summary.id, saved_summary.version
            ),
            applied: true,
            approval_required: false,
            definition: Some(saved_summary),
            definitions: Vec::new(),
            validation_report: Some(validation_report),
            approval_review: None,
        })
    }

    fn list_workflow_definitions(
        &self,
        request: AutonomousWorkflowDefinitionRequest,
    ) -> CommandResult<AutonomousWorkflowDefinitionOutput> {
        let project_id = self.resolve_workflow_project_id(request.project_id.as_deref())?;
        let definitions = project_store::list_workflow_definitions(&self.repo_root, &project_id)?;
        Ok(AutonomousWorkflowDefinitionOutput {
            action: request.action,
            message: format!(
                "Listed {} Workflow definition(s) for project `{project_id}`.",
                definitions.len()
            ),
            applied: false,
            approval_required: false,
            definition: None,
            definitions,
            validation_report: None,
            approval_review: None,
        })
    }

    fn get_workflow_definition(
        &self,
        request: AutonomousWorkflowDefinitionRequest,
    ) -> CommandResult<AutonomousWorkflowDefinitionOutput> {
        let project_id = self.resolve_workflow_project_id(request.project_id.as_deref())?;
        let workflow_id = required_text(request.workflow_id.as_deref(), "workflowId")?;
        let definition =
            project_store::get_workflow_definition(&self.repo_root, &project_id, &workflow_id)?
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "workflow_definition_not_found",
                        format!("Xero could not find Workflow `{workflow_id}`."),
                    )
                })?;
        let summary = workflow_summary_from_definition(&definition, true)?;
        Ok(AutonomousWorkflowDefinitionOutput {
            action: request.action,
            message: format!(
                "Loaded Workflow definition `{}` version {}.",
                summary.id, summary.version
            ),
            applied: false,
            approval_required: false,
            definition: Some(summary),
            definitions: Vec::new(),
            validation_report: None,
            approval_review: None,
        })
    }

    fn required_workflow_definition(
        &self,
        value: Option<&JsonValue>,
        request_project_id: Option<&str>,
    ) -> CommandResult<WorkflowDefinitionDto> {
        let value = value.ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_definition_required",
                "The workflow_definition tool requires a definition object for this action.",
            )
        })?;
        let mut value = value.clone();
        if let JsonValue::Object(object) = &mut value {
            let has_project_id = object
                .get("projectId")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
            if !has_project_id {
                if let Some(project_id) = request_project_id
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .or_else(|| {
                        self.agent_run_context
                            .as_ref()
                            .map(|context| context.project_id.as_str())
                    })
                {
                    object.insert("projectId".into(), JsonValue::String(project_id.to_owned()));
                }
            }
        }
        let mut definition =
            serde_json::from_value::<WorkflowDefinitionDto>(value).map_err(|error| {
                CommandError::user_fixable(
                    "workflow_definition_decode_failed",
                    format!("Xero could not decode the Workflow definition draft: {error}"),
                )
            })?;
        if definition.project_id.trim().is_empty() {
            if let Some(context) = self.agent_run_context.as_ref() {
                definition.project_id = context.project_id.clone();
            }
        }
        Ok(definition)
    }

    fn resolve_workflow_project_id(&self, value: Option<&str>) -> CommandResult<String> {
        if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
            return Ok(value.to_owned());
        }
        if let Some(context) = self.agent_run_context.as_ref() {
            return Ok(context.project_id.clone());
        }
        Err(CommandError::user_fixable(
            "workflow_definition_project_required",
            "The workflow_definition tool requires projectId when no active agent run project context is available.",
        ))
    }

    fn ensure_request_project_matches_definition(
        &self,
        request_project_id: Option<&str>,
        definition_project_id: &str,
    ) -> CommandResult<()> {
        let Some(request_project_id) = request_project_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(());
        };
        if request_project_id == definition_project_id {
            return Ok(());
        }
        Err(CommandError::user_fixable(
            "workflow_definition_project_mismatch",
            "Xero refused the Workflow definition because projectId and definition.projectId differ.",
        ))
    }
}

fn invalid_workflow_output(
    action: AutonomousWorkflowDefinitionAction,
    definition: AutonomousWorkflowDefinitionSummary,
    validation_report: WorkflowValidationReportDto,
    message: &str,
) -> AutonomousWorkflowDefinitionOutput {
    AutonomousWorkflowDefinitionOutput {
        action,
        message: message.into(),
        applied: false,
        approval_required: false,
        definition: Some(definition),
        definitions: Vec::new(),
        validation_report: Some(validation_report),
        approval_review: None,
    }
}

fn approval_required_workflow_output(
    action: AutonomousWorkflowDefinitionAction,
    definition: AutonomousWorkflowDefinitionSummary,
    validation_report: WorkflowValidationReportDto,
    message: &str,
    approval_review: Option<JsonValue>,
) -> AutonomousWorkflowDefinitionOutput {
    AutonomousWorkflowDefinitionOutput {
        action,
        message: message.into(),
        applied: false,
        approval_required: true,
        definition: Some(definition),
        definitions: Vec::new(),
        validation_report: Some(validation_report),
        approval_review,
    }
}

fn required_text(value: Option<&str>, field: &str) -> CommandResult<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_definition_request_invalid",
                format!("The workflow_definition tool requires `{field}` for this action."),
            )
        })
}

fn workflow_summary_from_definition(
    definition: &WorkflowDefinitionDto,
    include_snapshot: bool,
) -> CommandResult<AutonomousWorkflowDefinitionSummary> {
    Ok(AutonomousWorkflowDefinitionSummary {
        id: definition.id.clone(),
        project_id: definition.project_id.clone(),
        name: definition.name.clone(),
        description: definition.description.clone(),
        version: definition.version,
        start_node_id: definition.start_node_id.clone(),
        node_count: definition.nodes.len(),
        edge_count: definition.edges.len(),
        snapshot: if include_snapshot {
            Some(serde_json::to_value(definition).map_err(|error| {
                CommandError::system_fault(
                    "workflow_definition_summary_encode_failed",
                    format!("Xero could not encode Workflow definition summary: {error}"),
                )
            })?)
        } else {
            None
        },
    })
}

fn build_workflow_definition_review(
    action: &str,
    prior: Option<&WorkflowDefinitionDto>,
    next: &WorkflowDefinitionDto,
) -> CommandResult<JsonValue> {
    let prior_summary = prior
        .map(|definition| workflow_summary_from_definition(definition, false))
        .transpose()?;
    let next_summary = workflow_summary_from_definition(next, false)?;
    Ok(json!({
        "schema": "xero.workflow_definition_pre_save_review.v1",
        "generatedAt": now_timestamp(),
        "action": action,
        "projectId": next.project_id,
        "workflowId": next.id,
        "prior": prior_summary,
        "next": next_summary,
        "definition": next,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::path::Path;

    use rusqlite::{params, Connection};
    use tempfile::TempDir;

    use crate::{
        commands::{
            contracts::{
                workflow_agents::AgentRefDto,
                workflows::{
                    WorkflowEdgeDto, WorkflowEdgeTypeDto, WorkflowNodeDto,
                    WorkflowOutputContractDto, WorkflowTerminalStatusDto,
                },
            },
            RuntimeAgentIdDto,
        },
        db::{configure_connection, database_path_for_project_in_app_data, migrations::migrations},
        runtime::{AutonomousToolRequest, ToolRegistry, ToolRegistryOptions},
    };

    fn repo_with_database(project_id: &str) -> (TempDir, std::path::PathBuf) {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        create_project_database(&repo_root, project_id);
        (tempdir, repo_root)
    }

    fn create_project_database(repo_root: &Path, project_id: &str) {
        let app_data_dir = repo_root.parent().expect("repo parent").join("app-data");
        let database_path = database_path_for_project_in_app_data(&app_data_dir, project_id);
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
        crate::db::register_project_database_path_for_tests(repo_root, database_path);
    }

    fn runtime(repo_root: &Path) -> AutonomousToolRuntime {
        AutonomousToolRuntime::new(repo_root)
            .expect("runtime")
            .with_agent_run_context("project-1", "session-1", "run-1")
    }

    fn valid_definition() -> WorkflowDefinitionDto {
        WorkflowDefinitionDto {
            schema: "xero.workflow_definition.v1".into(),
            id: "workflow-agent-handoff".into(),
            project_id: "project-1".into(),
            name: "Agent handoff".into(),
            description: "Pass one agent output to another.".into(),
            version: 1,
            start_node_id: "intake".into(),
            nodes: vec![
                WorkflowNodeDto::Agent {
                    id: "intake".into(),
                    title: "Intake".into(),
                    description: String::new(),
                    position: Default::default(),
                    agent_ref: AgentRefDto::BuiltIn {
                        runtime_agent_id: RuntimeAgentIdDto::Plan,
                        version: 2,
                    },
                    display_label: None,
                    input_bindings: Vec::new(),
                    output_contract: WorkflowOutputContractDto::default(),
                    run_overrides: None,
                    resource_scopes: Vec::new(),
                    failure_policy: Default::default(),
                },
                WorkflowNodeDto::Agent {
                    id: "work".into(),
                    title: "Work".into(),
                    description: String::new(),
                    position: Default::default(),
                    agent_ref: AgentRefDto::BuiltIn {
                        runtime_agent_id: RuntimeAgentIdDto::Engineer,
                        version: 2,
                    },
                    display_label: None,
                    input_bindings: Vec::new(),
                    output_contract: WorkflowOutputContractDto::default(),
                    run_overrides: None,
                    resource_scopes: Vec::new(),
                    failure_policy: Default::default(),
                },
                WorkflowNodeDto::Terminal {
                    id: "done".into(),
                    title: "Done".into(),
                    description: String::new(),
                    position: Default::default(),
                    terminal_status: WorkflowTerminalStatusDto::Success,
                },
            ],
            edges: vec![
                WorkflowEdgeDto {
                    id: "intake-work".into(),
                    from_node_id: "intake".into(),
                    to_node_id: "work".into(),
                    r#type: WorkflowEdgeTypeDto::Success,
                    label: String::new(),
                    priority: 10,
                    condition: Default::default(),
                    loop_policy: None,
                },
                WorkflowEdgeDto {
                    id: "work-done".into(),
                    from_node_id: "work".into(),
                    to_node_id: "done".into(),
                    r#type: WorkflowEdgeTypeDto::Success,
                    label: String::new(),
                    priority: 10,
                    condition: Default::default(),
                    loop_policy: None,
                },
            ],
            subgraphs: Vec::new(),
            artifact_contracts: Vec::new(),
            run_policy: Default::default(),
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn workflow_definition_tool_is_only_registered_for_agent_create() {
        for agent_id in [
            RuntimeAgentIdDto::Ask,
            RuntimeAgentIdDto::Plan,
            RuntimeAgentIdDto::Engineer,
            RuntimeAgentIdDto::Debug,
            RuntimeAgentIdDto::Crawl,
            RuntimeAgentIdDto::Generalist,
        ] {
            let registry = ToolRegistry::for_tool_names_with_options(
                [AUTONOMOUS_TOOL_WORKFLOW_DEFINITION.to_string()].into(),
                ToolRegistryOptions {
                    runtime_agent_id: agent_id,
                    ..ToolRegistryOptions::default()
                },
            );
            assert!(registry
                .descriptor(AUTONOMOUS_TOOL_WORKFLOW_DEFINITION)
                .is_none());
        }

        let registry = ToolRegistry::for_tool_names_with_options(
            [AUTONOMOUS_TOOL_WORKFLOW_DEFINITION.to_string()].into(),
            ToolRegistryOptions {
                runtime_agent_id: RuntimeAgentIdDto::AgentCreate,
                ..ToolRegistryOptions::default()
            },
        );
        let request = registry
            .decode_call(&crate::runtime::AgentToolCall {
                tool_call_id: "call-workflow-definition".into(),
                tool_name: AUTONOMOUS_TOOL_WORKFLOW_DEFINITION.into(),
                input: json!({
                    "action": "validate",
                    "definition": valid_definition()
                }),
            })
            .expect("decode workflow definition tool");
        assert!(matches!(
            request,
            AutonomousToolRequest::WorkflowDefinition(_)
        ));
    }

    #[test]
    fn workflow_definition_save_requires_operator_approval_before_persisting() {
        let (_tempdir, repo_root) = repo_with_database("project-1");
        let request = AutonomousWorkflowDefinitionRequest {
            action: AutonomousWorkflowDefinitionAction::Save,
            project_id: None,
            workflow_id: None,
            definition: Some(json!(valid_definition())),
        };

        let dry_run = runtime(&repo_root)
            .workflow_definition(request.clone())
            .expect("save dry run");
        let AutonomousToolOutput::WorkflowDefinition(output) = dry_run.output else {
            panic!("unexpected output");
        };
        assert!(output.approval_required);
        assert!(!output.applied);
        assert!(output.approval_review.is_some());
        assert!(project_store::get_workflow_definition(
            &repo_root,
            "project-1",
            "workflow-agent-handoff"
        )
        .expect("load workflow")
        .is_none());

        let approved = runtime(&repo_root)
            .workflow_definition_with_operator_approval(request)
            .expect("approved save");
        let AutonomousToolOutput::WorkflowDefinition(output) = approved.output else {
            panic!("unexpected output");
        };
        assert!(output.applied);
        assert!(project_store::get_workflow_definition(
            &repo_root,
            "project-1",
            "workflow-agent-handoff"
        )
        .expect("load workflow")
        .is_some());
    }

    #[test]
    fn workflow_definition_validate_returns_graph_diagnostics() {
        let (_tempdir, repo_root) = repo_with_database("project-1");
        let mut definition = valid_definition();
        definition.start_node_id = "missing".into();

        let result = runtime(&repo_root)
            .workflow_definition(AutonomousWorkflowDefinitionRequest {
                action: AutonomousWorkflowDefinitionAction::Validate,
                project_id: None,
                workflow_id: None,
                definition: Some(json!(definition)),
            })
            .expect("validation response");
        let AutonomousToolOutput::WorkflowDefinition(output) = result.output else {
            panic!("unexpected output");
        };

        assert_eq!(
            output.validation_report.expect("validation report").status,
            WorkflowValidationStatusDto::Invalid
        );
    }
}
