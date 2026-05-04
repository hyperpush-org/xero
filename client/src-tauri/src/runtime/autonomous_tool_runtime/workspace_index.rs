use serde::{Deserialize, Serialize};

use super::{
    AutonomousToolOutput, AutonomousToolResult, AutonomousToolRuntime,
    AUTONOMOUS_TOOL_WORKSPACE_INDEX,
};
use crate::commands::{
    workspace_index::{
        workspace_explain_at_root, workspace_query_at_root, workspace_status_at_root,
    },
    CommandResult, WorkspaceExplainRequestDto, WorkspaceIndexDiagnosticDto,
    WorkspaceIndexStatusDto, WorkspaceQueryModeDto, WorkspaceQueryRequestDto,
    WorkspaceQueryResultDto,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousWorkspaceIndexAction {
    Status,
    Query,
    SymbolLookup,
    RelatedTests,
    ChangeImpact,
    Explain,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWorkspaceIndexRequest {
    pub action: AutonomousWorkspaceIndexAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWorkspaceIndexOutput {
    pub action: AutonomousWorkspaceIndexAction,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<WorkspaceIndexStatusDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<WorkspaceQueryResultDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<WorkspaceIndexDiagnosticDto>,
}

impl Eq for AutonomousWorkspaceIndexOutput {}

impl AutonomousToolRuntime {
    pub fn workspace_index(
        &self,
        request: AutonomousWorkspaceIndexRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let output = self.execute_workspace_index(request)?;
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_WORKSPACE_INDEX.into(),
            summary: output.message.clone(),
            command_result: None,
            output: AutonomousToolOutput::WorkspaceIndex(output),
        })
    }

    fn execute_workspace_index(
        &self,
        request: AutonomousWorkspaceIndexRequest,
    ) -> CommandResult<AutonomousWorkspaceIndexOutput> {
        let run_context = self.agent_run_context().cloned().ok_or_else(|| {
            crate::commands::CommandError::system_fault(
                "workspace_index_run_context_missing",
                "Workspace-index tools require an active owned-agent run context.",
            )
        })?;
        match request.action {
            AutonomousWorkspaceIndexAction::Status => {
                let status = workspace_status_at_root(self.repo_root(), &run_context.project_id)?;
                Ok(AutonomousWorkspaceIndexOutput {
                    action: request.action,
                    message: format!(
                        "Workspace index is {:?} with {} of {} files indexed.",
                        status.state, status.indexed_files, status.total_files
                    ),
                    diagnostics: status.diagnostics.clone(),
                    status: Some(status),
                    results: Vec::new(),
                    signals: Vec::new(),
                })
            }
            AutonomousWorkspaceIndexAction::Query
            | AutonomousWorkspaceIndexAction::SymbolLookup
            | AutonomousWorkspaceIndexAction::RelatedTests
            | AutonomousWorkspaceIndexAction::ChangeImpact => {
                let query = request
                    .query
                    .as_ref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| crate::commands::CommandError::invalid_request("query"))?;
                let mode = match request.action {
                    AutonomousWorkspaceIndexAction::SymbolLookup => WorkspaceQueryModeDto::Symbol,
                    AutonomousWorkspaceIndexAction::RelatedTests => {
                        WorkspaceQueryModeDto::RelatedTests
                    }
                    AutonomousWorkspaceIndexAction::ChangeImpact => WorkspaceQueryModeDto::Impact,
                    _ => WorkspaceQueryModeDto::Semantic,
                };
                let response = workspace_query_at_root(
                    self.repo_root(),
                    WorkspaceQueryRequestDto {
                        project_id: run_context.project_id,
                        query: query.to_owned(),
                        mode,
                        limit: request.limit,
                        paths: request.path.into_iter().collect(),
                    },
                )?;
                Ok(AutonomousWorkspaceIndexOutput {
                    action: request.action,
                    message: format!(
                        "Workspace index returned {} result(s) for `{}`.",
                        response.result_count, response.query
                    ),
                    status: None,
                    results: response.results,
                    signals: Vec::new(),
                    diagnostics: response.diagnostics,
                })
            }
            AutonomousWorkspaceIndexAction::Explain => {
                let response = workspace_explain_at_root(
                    self.repo_root(),
                    WorkspaceExplainRequestDto {
                        project_id: run_context.project_id,
                        query: request.query,
                        path: request.path,
                    },
                )?;
                Ok(AutonomousWorkspaceIndexOutput {
                    action: request.action,
                    message: response.summary,
                    diagnostics: response.diagnostics,
                    status: Some(response.status),
                    results: Vec::new(),
                    signals: response.top_signals,
                })
            }
        }
    }
}
