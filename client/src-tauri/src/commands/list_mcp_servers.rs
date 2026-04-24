use std::collections::BTreeSet;

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        CommandError, CommandResult, McpConnectionDiagnosticDto, McpConnectionStateDto,
        McpConnectionStatusDto, McpEnvironmentReferenceDto, McpImportDiagnosticDto, McpRegistryDto,
        McpServerDto, McpTransportDto, RefreshMcpServerStatusesRequestDto,
    },
    mcp::{
        load_mcp_registry_from_path, persist_mcp_registry, refresh_mcp_connection_truth,
        McpConnectionDiagnostic, McpConnectionState, McpConnectionStatus, McpRegistry,
        McpRegistryImportDiagnostic, McpServerRecord, McpTransport,
    },
    state::DesktopState,
};

#[tauri::command]
pub fn list_mcp_servers<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<McpRegistryDto> {
    let registry = load_mcp_registry_snapshot(&app, state.inner())?;
    Ok(mcp_registry_dto_from_snapshot(&registry))
}

#[tauri::command]
pub fn refresh_mcp_server_statuses<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RefreshMcpServerStatusesRequestDto,
) -> CommandResult<McpRegistryDto> {
    let path = state.mcp_registry_file(&app)?;
    let current = load_mcp_registry_from_path(&path)?;

    let selection = parse_refresh_selection(&current, &request.server_ids)?;
    let next = refresh_mcp_connection_truth(&current, selection.as_ref());

    let snapshot = if next == current {
        current
    } else {
        persist_mcp_registry(&path, &next)?
    };

    Ok(mcp_registry_dto_from_snapshot(&snapshot))
}

pub(crate) fn load_mcp_registry_snapshot<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<McpRegistry> {
    let path = state.mcp_registry_file(app)?;
    load_mcp_registry_from_path(&path)
}

pub(crate) fn mcp_registry_dto_from_snapshot(registry: &McpRegistry) -> McpRegistryDto {
    McpRegistryDto {
        servers: registry
            .servers
            .iter()
            .map(mcp_server_dto_from_record)
            .collect::<Vec<_>>(),
        updated_at: registry.updated_at.clone(),
    }
}

pub(crate) fn mcp_server_dto_from_record(record: &McpServerRecord) -> McpServerDto {
    McpServerDto {
        id: record.id.clone(),
        name: record.name.clone(),
        transport: mcp_transport_dto_from_record(&record.transport),
        env: record
            .env
            .iter()
            .map(|entry| McpEnvironmentReferenceDto {
                key: entry.key.clone(),
                from_env: entry.from_env.clone(),
            })
            .collect::<Vec<_>>(),
        cwd: record.cwd.clone(),
        connection: mcp_connection_state_dto_from_record(&record.connection),
        updated_at: record.updated_at.clone(),
    }
}

pub(crate) fn mcp_transport_dto_from_record(transport: &McpTransport) -> McpTransportDto {
    match transport {
        McpTransport::Stdio { command, args } => McpTransportDto::Stdio {
            command: command.clone(),
            args: args.clone(),
        },
        McpTransport::Http { url } => McpTransportDto::Http { url: url.clone() },
        McpTransport::Sse { url } => McpTransportDto::Sse { url: url.clone() },
    }
}

pub(crate) fn mcp_transport_record_from_dto(transport: &McpTransportDto) -> McpTransport {
    match transport {
        McpTransportDto::Stdio { command, args } => McpTransport::Stdio {
            command: command.clone(),
            args: args.clone(),
        },
        McpTransportDto::Http { url } => McpTransport::Http { url: url.clone() },
        McpTransportDto::Sse { url } => McpTransport::Sse { url: url.clone() },
    }
}

pub(crate) fn mcp_connection_state_dto_from_record(
    state: &McpConnectionState,
) -> McpConnectionStateDto {
    McpConnectionStateDto {
        status: mcp_connection_status_dto_from_record(state.status.clone()),
        diagnostic: state
            .diagnostic
            .as_ref()
            .map(mcp_connection_diagnostic_dto_from_record),
        last_checked_at: state.last_checked_at.clone(),
        last_healthy_at: state.last_healthy_at.clone(),
    }
}

fn mcp_connection_status_dto_from_record(status: McpConnectionStatus) -> McpConnectionStatusDto {
    match status {
        McpConnectionStatus::Connected => McpConnectionStatusDto::Connected,
        McpConnectionStatus::Failed => McpConnectionStatusDto::Failed,
        McpConnectionStatus::Blocked => McpConnectionStatusDto::Blocked,
        McpConnectionStatus::Misconfigured => McpConnectionStatusDto::Misconfigured,
        McpConnectionStatus::Stale => McpConnectionStatusDto::Stale,
    }
}

fn mcp_connection_diagnostic_dto_from_record(
    diagnostic: &McpConnectionDiagnostic,
) -> McpConnectionDiagnosticDto {
    McpConnectionDiagnosticDto {
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        retryable: diagnostic.retryable,
    }
}

pub(crate) fn mcp_import_diagnostic_dto_from_record(
    diagnostic: &McpRegistryImportDiagnostic,
) -> McpImportDiagnosticDto {
    McpImportDiagnosticDto {
        index: diagnostic.index,
        server_id: diagnostic.server_id.clone(),
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
    }
}

fn parse_refresh_selection(
    registry: &McpRegistry,
    requested_server_ids: &[String],
) -> CommandResult<Option<BTreeSet<String>>> {
    if requested_server_ids.is_empty() {
        return Ok(None);
    }

    let mut selection = BTreeSet::new();
    for server_id in requested_server_ids {
        let server_id = server_id.trim();
        if server_id.is_empty() {
            return Err(CommandError::invalid_request("serverIds"));
        }

        if !registry.servers.iter().any(|server| server.id == server_id) {
            return Err(CommandError::user_fixable(
                "mcp_server_not_found",
                format!("Cadence could not find MCP server `{server_id}`."),
            ));
        }

        selection.insert(server_id.to_owned());
    }

    Ok(Some(selection))
}
