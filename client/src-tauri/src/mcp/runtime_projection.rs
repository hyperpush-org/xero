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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectionRegistryContext {
    SourceRegistry,
    ProjectionContract,
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

    let registry = load_mcp_registry_from_path(registry_path).map_err(|error| {
        map_projection_registry_error(
            error,
            registry_path,
            ProjectionRegistryContext::SourceRegistry,
        )
    })?;
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
    let persisted =
        persist_mcp_registry(&projection_path, &projection_registry).map_err(|error| {
            map_projection_registry_error(
                error,
                &projection_path,
                ProjectionRegistryContext::ProjectionContract,
            )
        })?;
    assert_projection_registry_connected(
        &persisted,
        &projection_path,
        ProjectionRegistryContext::ProjectionContract,
    )?;

    Ok(RuntimeMcpProjectionOutcome {
        projection_path,
        connected_server_ids,
    })
}

pub fn load_runtime_mcp_projection_contract(path: &Path) -> CommandResult<McpRegistry> {
    let projection = load_mcp_registry_from_path(path).map_err(|error| {
        map_projection_registry_error(error, path, ProjectionRegistryContext::ProjectionContract)
    })?;
    assert_projection_registry_connected(
        &projection,
        path,
        ProjectionRegistryContext::ProjectionContract,
    )?;
    Ok(projection)
}

fn map_projection_registry_error(
    error: CommandError,
    path: &Path,
    context: ProjectionRegistryContext,
) -> CommandError {
    let (code, retryable) = match error.code.as_str() {
        "mcp_registry_read_failed" => (projection_unreadable_code(context), true),
        "mcp_registry_decode_failed" | "mcp_registry_invalid" => {
            (projection_malformed_code(context), false)
        }
        "mcp_registry_serialize_failed"
        | "mcp_registry_write_failed"
        | "mcp_registry_rollback_failed" => ("runtime_mcp_projection_contract_write_failed", true),
        _ => return error,
    };

    let message = format!(
        "Xero rejected runtime MCP projection data at {} because validation failed: {} ({}).",
        path.display(),
        error.message,
        error.code,
    );

    if retryable {
        CommandError::retryable(code, message)
    } else {
        CommandError::user_fixable(code, message)
    }
}

fn assert_projection_registry_connected(
    registry: &McpRegistry,
    path: &Path,
    context: ProjectionRegistryContext,
) -> CommandResult<()> {
    if let Some(disconnected) = registry
        .servers
        .iter()
        .find(|server| server.connection.status != McpConnectionStatus::Connected)
    {
        return Err(CommandError::user_fixable(
            projection_disconnected_code(context),
            format!(
                "Xero rejected runtime MCP projection contract at {} because server `{}` had non-connected status `{:?}`.",
                path.display(),
                disconnected.id,
                disconnected.connection.status,
            ),
        ));
    }

    Ok(())
}

fn projection_unreadable_code(context: ProjectionRegistryContext) -> &'static str {
    match context {
        ProjectionRegistryContext::SourceRegistry => "runtime_mcp_projection_registry_unreadable",
        ProjectionRegistryContext::ProjectionContract => {
            "runtime_mcp_projection_contract_unreadable"
        }
    }
}

fn projection_malformed_code(context: ProjectionRegistryContext) -> &'static str {
    match context {
        ProjectionRegistryContext::SourceRegistry => "runtime_mcp_projection_registry_malformed",
        ProjectionRegistryContext::ProjectionContract => {
            "runtime_mcp_projection_contract_malformed"
        }
    }
}

fn projection_disconnected_code(context: ProjectionRegistryContext) -> &'static str {
    match context {
        ProjectionRegistryContext::SourceRegistry => "runtime_mcp_projection_registry_disconnected",
        ProjectionRegistryContext::ProjectionContract => {
            "runtime_mcp_projection_contract_disconnected"
        }
    }
}
