use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult, McpRegistryDto, RemoveMcpServerRequestDto},
    mcp::{load_mcp_registry_from_path, persist_mcp_registry},
    state::DesktopState,
};

use super::list_mcp_servers::mcp_registry_dto_from_snapshot;

#[tauri::command]
pub fn remove_mcp_server<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RemoveMcpServerRequestDto,
) -> CommandResult<McpRegistryDto> {
    let server_id = request.server_id.trim();
    if server_id.is_empty() {
        return Err(CommandError::invalid_request("serverId"));
    }

    let path = state.global_db_path(&app)?;
    let current = load_mcp_registry_from_path(&path)?;

    if !current.servers.iter().any(|server| server.id == server_id) {
        return Err(CommandError::user_fixable(
            "mcp_server_not_found",
            format!("Xero could not find MCP server `{server_id}`."),
        ));
    }

    let mut next = current.clone();
    next.servers.retain(|server| server.id != server_id);
    next.updated_at = now_timestamp();

    let persisted = persist_mcp_registry(&path, &next)?;
    Ok(mcp_registry_dto_from_snapshot(&persisted))
}
