use std::collections::BTreeSet;

use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        CommandError, CommandResult, McpEnvironmentReferenceDto, McpRegistryDto, McpTransportDto,
        UpsertMcpServerRequestDto,
    },
    mcp::{
        load_mcp_registry_from_path, persist_mcp_registry, stale_after_configuration_change,
        McpConnectionDiagnostic, McpConnectionState, McpConnectionStatus, McpEnvironmentReference,
        McpServerRecord,
    },
    state::DesktopState,
};

use super::list_mcp_servers::{mcp_registry_dto_from_snapshot, mcp_transport_record_from_dto};

#[tauri::command]
pub fn upsert_mcp_server<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertMcpServerRequestDto,
) -> CommandResult<McpRegistryDto> {
    let path = state.global_db_path(&app)?;
    let current = load_mcp_registry_from_path(&path)?;
    let candidate = server_record_from_request(&request, &current.servers)?;

    let mut next = current.clone();
    if let Some(existing) = next
        .servers
        .iter_mut()
        .find(|server| server.id == candidate.id)
    {
        *existing = candidate;
    } else {
        next.servers.push(candidate);
    }
    next.servers.sort_by(|left, right| left.id.cmp(&right.id));

    if next == current {
        return Ok(mcp_registry_dto_from_snapshot(&current));
    }

    next.updated_at = now_timestamp();
    let persisted = persist_mcp_registry(&path, &next)?;
    Ok(mcp_registry_dto_from_snapshot(&persisted))
}

fn server_record_from_request(
    request: &UpsertMcpServerRequestDto,
    current_servers: &[McpServerRecord],
) -> CommandResult<McpServerRecord> {
    let id = request.id.trim();
    if id.is_empty() {
        return Err(CommandError::invalid_request("id"));
    }

    let name = request.name.trim();
    if name.is_empty() {
        return Err(CommandError::invalid_request("name"));
    }

    validate_transport_request(&request.transport)?;
    let env = normalize_environment_request(&request.env)?;
    let cwd = normalize_optional_text(request.cwd.clone());

    let now = now_timestamp();
    let current = current_servers.iter().find(|server| server.id == id);

    let mut connection = current
        .map(|server| server.connection.clone())
        .unwrap_or_else(default_unchecked_connection_state);

    let transport = mcp_transport_record_from_dto(&request.transport);

    if let Some(existing) = current {
        let transport_changed = existing.transport != transport;
        let env_changed = existing.env != env;
        let cwd_changed = existing.cwd != cwd;

        if transport_changed || env_changed || cwd_changed {
            connection = stale_after_configuration_change(&existing.connection);
        }

        let updated_at =
            if existing.name == name && !transport_changed && !env_changed && !cwd_changed {
                existing.updated_at.clone()
            } else {
                now
            };

        return Ok(McpServerRecord {
            id: id.to_owned(),
            name: name.to_owned(),
            transport,
            env,
            cwd,
            connection,
            updated_at,
        });
    }

    Ok(McpServerRecord {
        id: id.to_owned(),
        name: name.to_owned(),
        transport,
        env,
        cwd,
        connection,
        updated_at: now,
    })
}

fn validate_transport_request(transport: &McpTransportDto) -> CommandResult<()> {
    match transport {
        McpTransportDto::Stdio { command, args } => {
            if command.trim().is_empty() {
                return Err(CommandError::invalid_request("transport.command"));
            }
            if args.iter().any(|arg| arg.trim().is_empty()) {
                return Err(CommandError::invalid_request("transport.args"));
            }
        }
        McpTransportDto::Http { url } | McpTransportDto::Sse { url } => {
            if url.trim().is_empty() {
                return Err(CommandError::invalid_request("transport.url"));
            }
        }
    }

    Ok(())
}

fn normalize_environment_request(
    env: &[McpEnvironmentReferenceDto],
) -> CommandResult<Vec<McpEnvironmentReference>> {
    let mut normalized = Vec::with_capacity(env.len());
    let mut keys = BTreeSet::new();

    for entry in env {
        let key = entry.key.trim();
        if key.is_empty() {
            return Err(CommandError::invalid_request("env.key"));
        }

        let from_env = entry.from_env.trim();
        if from_env.is_empty() {
            return Err(CommandError::invalid_request("env.fromEnv"));
        }

        if !keys.insert(key.to_owned()) {
            return Err(CommandError::user_fixable(
                "mcp_registry_request_invalid",
                format!("Xero rejected MCP server request because env key `{key}` was duplicated."),
            ));
        }

        normalized.push(McpEnvironmentReference {
            key: key.to_owned(),
            from_env: from_env.to_owned(),
        });
    }

    normalized.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(normalized)
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

fn default_unchecked_connection_state() -> McpConnectionState {
    McpConnectionState {
        status: McpConnectionStatus::Stale,
        diagnostic: Some(McpConnectionDiagnostic {
            code: "mcp_status_unchecked".into(),
            message: "Xero has not checked this MCP server yet.".into(),
            retryable: true,
        }),
        last_checked_at: None,
        last_healthy_at: None,
    }
}
