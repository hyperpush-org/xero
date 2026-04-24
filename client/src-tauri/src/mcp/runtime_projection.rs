use std::path::{Path, PathBuf};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
};

use super::{load_mcp_registry_from_path, persist_mcp_registry, McpConnectionStatus, McpRegistry};

pub const RUNTIME_MCP_PROJECTION_DIRECTORY_NAME: &str = "runtime-mcp-projections";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMcpProjectionOutcome {
    pub projection_path: PathBuf,
    pub connected_server_ids: Vec<String>,
}

pub fn materialize_runtime_mcp_projection_for_run(
    registry_path: &Path,
    projection_root: &Path,
    run_id: &str,
) -> CommandResult<RuntimeMcpProjectionOutcome> {
    let run_id = run_id.trim();
    if run_id.is_empty() {
        return Err(CommandError::invalid_request("runId"));
    }

    let registry = load_mcp_registry_from_path(registry_path)?;
    let projection_path = projection_root.join(format!("{run_id}.json"));

    let connected_servers = registry
        .servers
        .into_iter()
        .filter(|server| server.connection.status == McpConnectionStatus::Connected)
        .collect::<Vec<_>>();
    let connected_server_ids = connected_servers
        .iter()
        .map(|server| server.id.clone())
        .collect::<Vec<_>>();

    let projection_registry = McpRegistry {
        version: registry.version,
        servers: connected_servers,
        updated_at: now_timestamp(),
    };
    let persisted = persist_mcp_registry(&projection_path, &projection_registry)?;
    assert_projection_registry_connected(&persisted, &projection_path)?;

    Ok(RuntimeMcpProjectionOutcome {
        projection_path,
        connected_server_ids,
    })
}

pub fn load_runtime_mcp_projection_contract(path: &Path) -> CommandResult<McpRegistry> {
    let projection = load_mcp_registry_from_path(path)?;
    assert_projection_registry_connected(&projection, path)?;
    Ok(projection)
}

fn assert_projection_registry_connected(registry: &McpRegistry, path: &Path) -> CommandResult<()> {
    if let Some(disconnected) = registry
        .servers
        .iter()
        .find(|server| server.connection.status != McpConnectionStatus::Connected)
    {
        return Err(CommandError::user_fixable(
            "runtime_mcp_projection_invalid",
            format!(
                "Cadence rejected runtime MCP projection contract at {} because server `{}` had non-connected status `{:?}`.",
                path.display(),
                disconnected.id,
                disconnected.connection.status,
            ),
        ));
    }

    Ok(())
}
