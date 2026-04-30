use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    AutonomousToolOutput, AutonomousToolResult, AutonomousToolRuntime,
    AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT,
};
use crate::{
    commands::{validate_non_empty, CommandError, CommandResult},
    environment::service,
    global_db::environment_profile::{
        EnvironmentCapability, EnvironmentDiagnostic, EnvironmentDiagnosticSeverity,
        EnvironmentPermissionRequest, EnvironmentPlatform, EnvironmentProfileStatus,
        EnvironmentToolCategory, EnvironmentToolSummary,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousEnvironmentContextAction {
    Summary,
    Tool,
    Category,
    Capability,
    Refresh,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousEnvironmentContextRequest {
    pub action: AutonomousEnvironmentContextAction,
    #[serde(default)]
    pub tool_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<EnvironmentToolCategory>,
    #[serde(default)]
    pub capability_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousEnvironmentContextOutput {
    pub action: AutonomousEnvironmentContextAction,
    pub status: EnvironmentProfileStatus,
    pub stale: bool,
    pub refresh_started: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refreshed_at: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<EnvironmentPlatform>,
    #[serde(default)]
    pub tool_groups: BTreeMap<String, Vec<EnvironmentToolSummary>>,
    #[serde(default)]
    pub capabilities: Vec<EnvironmentCapability>,
    #[serde(default)]
    pub permission_requests: Vec<EnvironmentPermissionRequest>,
    #[serde(default)]
    pub diagnostics: Vec<EnvironmentDiagnostic>,
}

impl AutonomousToolRuntime {
    pub fn environment_context(
        &self,
        request: AutonomousEnvironmentContextRequest,
    ) -> CommandResult<AutonomousToolResult> {
        validate_environment_context_request(&request)?;
        let database_path = self.environment_profile_database_path()?;
        let mut status = service::environment_discovery_status(&database_path)?;
        let mut refresh_started = false;
        if request.action == AutonomousEnvironmentContextAction::Refresh && status.should_start {
            status = service::start_environment_discovery(database_path.clone())?;
            refresh_started = true;
        }

        let summary = service::environment_profile_summary(&database_path)?;
        let output = environment_context_output(request, status, summary, refresh_started);
        let summary_text = output.message.clone();
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT.into(),
            summary: summary_text,
            command_result: None,
            output: AutonomousToolOutput::EnvironmentContext(output),
        })
    }

    fn environment_profile_database_path(&self) -> CommandResult<PathBuf> {
        self.environment_profile_database_path
            .clone()
            .ok_or_else(|| {
                CommandError::system_fault(
                    "environment_context_database_unavailable",
                    "Xero could not read the environment profile because the global app-data database is not wired into this owned-agent runtime.",
                )
            })
    }
}

fn validate_environment_context_request(
    request: &AutonomousEnvironmentContextRequest,
) -> CommandResult<()> {
    match request.action {
        AutonomousEnvironmentContextAction::Tool => {
            if request.tool_ids.is_empty() {
                return Err(CommandError::invalid_request("toolIds"));
            }
            for id in &request.tool_ids {
                validate_non_empty(id, "toolIds")?;
            }
        }
        AutonomousEnvironmentContextAction::Category => {
            if request.category.is_none() {
                return Err(CommandError::invalid_request("category"));
            }
        }
        AutonomousEnvironmentContextAction::Capability => {
            if request.capability_ids.is_empty() {
                return Err(CommandError::invalid_request("capabilityIds"));
            }
            for id in &request.capability_ids {
                validate_non_empty(id, "capabilityIds")?;
            }
        }
        AutonomousEnvironmentContextAction::Summary
        | AutonomousEnvironmentContextAction::Refresh => {}
    }
    Ok(())
}

fn environment_context_output(
    request: AutonomousEnvironmentContextRequest,
    status: service::EnvironmentDiscoveryStatus,
    summary: Option<crate::global_db::environment_profile::EnvironmentProfileSummary>,
    refresh_started: bool,
) -> AutonomousEnvironmentContextOutput {
    let Some(summary) = summary else {
        return AutonomousEnvironmentContextOutput {
            action: request.action,
            status: status.status,
            stale: status.stale,
            refresh_started,
            refreshed_at: status.refreshed_at,
            message: "No environment profile is available yet.".into(),
            platform: None,
            tool_groups: BTreeMap::new(),
            capabilities: Vec::new(),
            permission_requests: status.permission_requests,
            diagnostics: status.diagnostics,
        };
    };

    let tools = filtered_tools(&request, &summary.tools);
    let capabilities = filtered_capabilities(&request, &summary.capabilities);
    let mut diagnostics = summary.diagnostics.clone();
    diagnostics.extend(missing_requested_tool_diagnostics(&request, &summary.tools));
    diagnostics.extend(missing_requested_capability_diagnostics(
        &request,
        &summary.capabilities,
    ));
    let tool_groups = group_tools_by_category(tools);
    let tool_count = tool_groups.values().map(Vec::len).sum::<usize>();
    let message = environment_context_message(
        &request.action,
        tool_count,
        capabilities.len(),
        status.stale,
        refresh_started,
    );

    AutonomousEnvironmentContextOutput {
        action: request.action,
        status: status.status,
        stale: status.stale,
        refresh_started,
        refreshed_at: summary.refreshed_at,
        message,
        platform: Some(summary.platform),
        tool_groups,
        capabilities,
        permission_requests: summary.permission_requests,
        diagnostics,
    }
}

fn filtered_tools(
    request: &AutonomousEnvironmentContextRequest,
    tools: &[EnvironmentToolSummary],
) -> Vec<EnvironmentToolSummary> {
    match request.action {
        AutonomousEnvironmentContextAction::Tool => tools
            .iter()
            .filter(|tool| request.tool_ids.iter().any(|id| id == &tool.id))
            .cloned()
            .collect(),
        AutonomousEnvironmentContextAction::Category => tools
            .iter()
            .filter(|tool| Some(tool.category) == request.category)
            .cloned()
            .collect(),
        AutonomousEnvironmentContextAction::Capability => Vec::new(),
        AutonomousEnvironmentContextAction::Summary
        | AutonomousEnvironmentContextAction::Refresh => tools.to_vec(),
    }
}

fn filtered_capabilities(
    request: &AutonomousEnvironmentContextRequest,
    capabilities: &[EnvironmentCapability],
) -> Vec<EnvironmentCapability> {
    match request.action {
        AutonomousEnvironmentContextAction::Capability => capabilities
            .iter()
            .filter(|capability| request.capability_ids.iter().any(|id| id == &capability.id))
            .cloned()
            .collect(),
        AutonomousEnvironmentContextAction::Tool | AutonomousEnvironmentContextAction::Category => {
            Vec::new()
        }
        AutonomousEnvironmentContextAction::Summary
        | AutonomousEnvironmentContextAction::Refresh => capabilities.to_vec(),
    }
}

fn group_tools_by_category(
    tools: Vec<EnvironmentToolSummary>,
) -> BTreeMap<String, Vec<EnvironmentToolSummary>> {
    let mut groups = BTreeMap::<String, Vec<EnvironmentToolSummary>>::new();
    for tool in tools {
        groups
            .entry(environment_tool_category_id(tool.category).into())
            .or_default()
            .push(tool);
    }
    groups
}

fn missing_requested_tool_diagnostics(
    request: &AutonomousEnvironmentContextRequest,
    tools: &[EnvironmentToolSummary],
) -> Vec<EnvironmentDiagnostic> {
    if request.action != AutonomousEnvironmentContextAction::Tool {
        return Vec::new();
    }
    request
        .tool_ids
        .iter()
        .filter(|id| !tools.iter().any(|tool| &tool.id == *id))
        .map(|id| EnvironmentDiagnostic {
            code: "environment_tool_unknown".into(),
            severity: EnvironmentDiagnosticSeverity::Warning,
            message: format!("No environment tool fact is recorded for `{id}`."),
            retryable: false,
            tool_id: Some(id.clone()),
        })
        .collect()
}

fn missing_requested_capability_diagnostics(
    request: &AutonomousEnvironmentContextRequest,
    capabilities: &[EnvironmentCapability],
) -> Vec<EnvironmentDiagnostic> {
    if request.action != AutonomousEnvironmentContextAction::Capability {
        return Vec::new();
    }
    request
        .capability_ids
        .iter()
        .filter(|id| !capabilities.iter().any(|capability| &capability.id == *id))
        .map(|id| EnvironmentDiagnostic {
            code: "environment_capability_unknown".into(),
            severity: EnvironmentDiagnosticSeverity::Warning,
            message: format!("No environment capability fact is recorded for `{id}`."),
            retryable: false,
            tool_id: None,
        })
        .collect()
}

fn environment_context_message(
    action: &AutonomousEnvironmentContextAction,
    tool_count: usize,
    capability_count: usize,
    stale: bool,
    refresh_started: bool,
) -> String {
    let freshness = if refresh_started {
        " A non-permission refresh was started."
    } else if stale {
        " The profile is stale."
    } else {
        ""
    };
    match action {
        AutonomousEnvironmentContextAction::Summary
        | AutonomousEnvironmentContextAction::Refresh => format!(
            "Returned environment profile summary with {tool_count} tool fact(s) and {capability_count} capability fact(s).{freshness}"
        ),
        AutonomousEnvironmentContextAction::Tool => {
            format!("Returned {tool_count} requested environment tool fact(s).{freshness}")
        }
        AutonomousEnvironmentContextAction::Category => {
            format!("Returned {tool_count} environment tool fact(s) for the requested category.{freshness}")
        }
        AutonomousEnvironmentContextAction::Capability => format!(
            "Returned {capability_count} requested environment capability fact(s).{freshness}"
        ),
    }
}

fn environment_tool_category_id(category: EnvironmentToolCategory) -> &'static str {
    match category {
        EnvironmentToolCategory::BaseDeveloperTool => "base_developer_tool",
        EnvironmentToolCategory::PackageManager => "package_manager",
        EnvironmentToolCategory::PlatformPackageManager => "platform_package_manager",
        EnvironmentToolCategory::LanguageRuntime => "language_runtime",
        EnvironmentToolCategory::ContainerOrchestration => "container_orchestration",
        EnvironmentToolCategory::MobileTooling => "mobile_tooling",
        EnvironmentToolCategory::CloudDeployment => "cloud_deployment",
        EnvironmentToolCategory::DatabaseCli => "database_cli",
        EnvironmentToolCategory::SolanaTooling => "solana_tooling",
        EnvironmentToolCategory::AgentAiCli => "agent_ai_cli",
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::{params, Connection};
    use tempfile::tempdir;

    use super::*;
    use crate::{
        auth::now_timestamp,
        global_db::{
            configure_connection,
            environment_profile::{
                EnvironmentCapabilityState, EnvironmentProfileSummary, EnvironmentToolProbeStatus,
                ENVIRONMENT_PROFILE_SCHEMA_VERSION,
            },
            migrations,
        },
    };

    #[test]
    fn tool_search_discovers_environment_context_without_baseline_activation() {
        let repo = tempdir().expect("repo");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        let search = runtime
            .tool_search(
                crate::runtime::autonomous_tool_runtime::AutonomousToolSearchRequest {
                    query: "installed cli tools protoc node rust".into(),
                    limit: Some(5),
                },
            )
            .expect("tool search");
        let AutonomousToolOutput::ToolSearch(output) = search.output else {
            panic!("unexpected output");
        };

        assert!(output
            .matches
            .iter()
            .any(|tool_match| tool_match.tool_name == AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT));
    }

    #[test]
    fn tool_access_can_grant_environment_context_exact_tool() {
        let repo = tempdir().expect("repo");
        let runtime = AutonomousToolRuntime::new(repo.path()).expect("runtime");
        let result = runtime
            .tool_access(crate::runtime::autonomous_tool_runtime::AutonomousToolAccessRequest {
                action: crate::runtime::autonomous_tool_runtime::AutonomousToolAccessAction::Request,
                groups: Vec::new(),
                tools: vec![AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT.into()],
                reason: Some("Diagnose command availability.".into()),
            })
            .expect("tool access");
        let AutonomousToolOutput::ToolAccess(output) = result.output else {
            panic!("unexpected output");
        };

        assert_eq!(
            output.granted_tools,
            vec![AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT.to_string()]
        );
        assert!(output.denied_tools.is_empty());
    }

    #[test]
    fn environment_context_returns_redacted_scoped_tool_facts() {
        let repo = tempdir().expect("repo");
        let db = tempdir().expect("db");
        let db_path = db.path().join("xero.db");
        seed_profile(&db_path);
        let runtime = AutonomousToolRuntime::new(repo.path())
            .expect("runtime")
            .with_environment_profile_database_path(db_path);

        let result = runtime
            .environment_context(AutonomousEnvironmentContextRequest {
                action: AutonomousEnvironmentContextAction::Tool,
                tool_ids: vec!["node".into(), "unknown_tool".into()],
                category: None,
                capability_ids: Vec::new(),
            })
            .expect("environment context");

        let AutonomousToolOutput::EnvironmentContext(output) = result.output else {
            panic!("unexpected output");
        };
        let encoded = serde_json::to_string(&output).expect("encode output");
        assert!(encoded.contains("node"));
        assert!(encoded.contains("~/bin/node"));
        assert!(!encoded.contains("/Users/alice"));
        assert!(output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "environment_tool_unknown"));
    }

    fn seed_profile(path: &std::path::Path) {
        let mut connection = Connection::open(path).expect("open db");
        configure_connection(&connection).expect("configure");
        migrations::migrations()
            .to_latest(&mut connection)
            .expect("migrate");
        let timestamp = now_timestamp();
        let summary = sample_summary(timestamp.clone());
        let payload = serde_json::json!({
            "schemaVersion": ENVIRONMENT_PROFILE_SCHEMA_VERSION,
            "platform": summary.platform,
            "path": { "entryCount": 1, "fingerprint": "sha256-demo", "sources": ["tauri-process-path"] },
            "tools": [{
                "id": "node",
                "category": "language_runtime",
                "command": "node",
                "present": true,
                "path": "/Users/alice/bin/node",
                "version": "v20.11.1",
                "source": "path",
                "probeStatus": "ok",
                "durationMs": 18
            }],
            "capabilities": summary.capabilities,
            "permissions": [],
            "diagnostics": []
        });
        connection
            .execute(
                "INSERT INTO environment_profile (
                    id, schema_version, status, os_kind, os_version, arch, default_shell,
                    path_fingerprint, payload_json, summary_json, permission_requests_json,
                    diagnostics_json, probe_started_at, probe_completed_at, refreshed_at
                ) VALUES (1, ?1, 'ready', 'macos', '15.4', 'aarch64', 'zsh', 'sha256-demo', ?2, ?3, '[]', '[]', ?4, ?4, ?4)",
                params![
                    ENVIRONMENT_PROFILE_SCHEMA_VERSION,
                    payload.to_string(),
                    serde_json::to_string(&summary).expect("summary"),
                    timestamp,
                ],
            )
            .expect("insert profile");
    }

    fn sample_summary(refreshed_at: String) -> EnvironmentProfileSummary {
        EnvironmentProfileSummary {
            schema_version: ENVIRONMENT_PROFILE_SCHEMA_VERSION,
            status: EnvironmentProfileStatus::Ready,
            platform: EnvironmentPlatform {
                os_kind: "macos".into(),
                os_version: Some("15.4".into()),
                arch: "aarch64".into(),
                default_shell: Some("zsh".into()),
            },
            refreshed_at: Some(refreshed_at),
            tools: vec![EnvironmentToolSummary {
                id: "node".into(),
                category: EnvironmentToolCategory::LanguageRuntime,
                present: true,
                version: Some("v20.11.1".into()),
                display_path: Some("~/bin/node".into()),
                probe_status: EnvironmentToolProbeStatus::Ok,
            }],
            capabilities: vec![EnvironmentCapability {
                id: "node_project_ready".into(),
                state: EnvironmentCapabilityState::Ready,
                evidence: vec!["node".into()],
                message: None,
            }],
            permission_requests: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}
