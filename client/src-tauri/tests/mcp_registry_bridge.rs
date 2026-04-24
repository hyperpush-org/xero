use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    thread,
    time::Duration,
};

use cadence_desktop_lib::{
    commands::{
        import_mcp_servers::import_mcp_servers,
        list_mcp_servers::{list_mcp_servers, refresh_mcp_server_statuses},
        remove_mcp_server::remove_mcp_server,
        upsert_mcp_server::upsert_mcp_server,
        CommandError, ImportMcpServersRequestDto, McpConnectionStatusDto,
        McpEnvironmentReferenceDto, McpRegistryDto, McpServerDto, McpTransportDto,
        RefreshMcpServerStatusesRequestDto, RemoveMcpServerRequestDto, UpsertMcpServerRequestDto,
    },
    configure_builder_with_state,
    mcp::{
        persist_mcp_registry, McpConnectionDiagnostic, McpConnectionState, McpConnectionStatus,
        McpEnvironmentReference, McpRegistry, McpServerRecord, McpTransport,
    },
    state::DesktopState,
};
use tauri::Manager;
use tempfile::TempDir;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> DesktopState {
    let app_data = root.path().join("app-data");

    DesktopState::default()
        .with_registry_file_override(app_data.join("project-registry.json"))
        .with_auth_store_file_override(app_data.join("openai-auth.json"))
        .with_provider_profiles_file_override(app_data.join("provider-profiles.json"))
        .with_provider_profile_credential_store_file_override(
            app_data.join("provider-profile-credentials.json"),
        )
        .with_runtime_settings_file_override(app_data.join("runtime-settings.json"))
        .with_mcp_registry_file_override(app_data.join("mcp-registry.json"))
        .with_openrouter_credential_file_override(app_data.join("openrouter-credentials.json"))
}

fn mcp_registry_path(root: &TempDir) -> PathBuf {
    root.path().join("app-data").join("mcp-registry.json")
}

fn spawn_single_response_server(
    status: u16,
    content_type: &str,
    body: &str,
    response_delay: Duration,
) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test HTTP listener");
    let address = listener.local_addr().expect("listener address");
    let content_type = content_type.to_owned();
    let body = body.to_owned();

    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };

        let mut reader = BufReader::new(match stream.try_clone() {
            Ok(clone) => clone,
            Err(_) => return,
        });

        let mut line = String::new();
        loop {
            line.clear();
            let Ok(bytes_read) = reader.read_line(&mut line) else {
                return;
            };
            if bytes_read == 0 || line == "\r\n" {
                break;
            }
        }

        if !response_delay.is_zero() {
            thread::sleep(response_delay);
        }

        let response = format!(
            "HTTP/1.1 {status} Test\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        );

        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });

    format!("http://{address}")
}

fn stale_connection() -> McpConnectionState {
    McpConnectionState {
        status: McpConnectionStatus::Stale,
        diagnostic: Some(McpConnectionDiagnostic {
            code: "mcp_status_unchecked".into(),
            message: "Cadence has not checked this MCP server yet.".into(),
            retryable: true,
        }),
        last_checked_at: None,
        last_healthy_at: None,
    }
}

fn connected_connection(timestamp: &str) -> McpConnectionState {
    McpConnectionState {
        status: McpConnectionStatus::Connected,
        diagnostic: None,
        last_checked_at: Some(timestamp.to_owned()),
        last_healthy_at: Some(timestamp.to_owned()),
    }
}

fn server_record(
    id: &str,
    name: &str,
    transport: McpTransport,
    env: Vec<McpEnvironmentReference>,
    connection: McpConnectionState,
) -> McpServerRecord {
    McpServerRecord {
        id: id.to_owned(),
        name: name.to_owned(),
        transport,
        env,
        cwd: None,
        connection,
        updated_at: "2026-04-24T00:00:00Z".into(),
    }
}

fn server_by_id<'a>(registry: &'a McpRegistryDto, id: &str) -> &'a McpServerDto {
    registry
        .servers
        .iter()
        .find(|server| server.id == id)
        .unwrap_or_else(|| panic!("expected MCP server `{id}` in projection"))
}

#[test]
fn mcp_commands_validate_requests_and_persist_crud_import_without_secret_values() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let registry_path = mcp_registry_path(&root);

    let initial = list_mcp_servers(app.handle().clone(), app.state::<DesktopState>())
        .expect("list MCP servers should return a default registry");
    assert!(initial.servers.is_empty());
    assert!(!registry_path.exists());

    let invalid_id = upsert_mcp_server(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertMcpServerRequestDto {
            id: "   ".into(),
            name: "Invalid".into(),
            transport: McpTransportDto::Stdio {
                command: "node".into(),
                args: Vec::new(),
            },
            env: Vec::new(),
            cwd: None,
        },
    )
    .expect_err("blank upsert id should fail");
    assert_eq!(invalid_id, CommandError::invalid_request("id"));

    let invalid_transport = upsert_mcp_server(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertMcpServerRequestDto {
            id: "invalid-url".into(),
            name: "Invalid URL".into(),
            transport: McpTransportDto::Http {
                url: "ftp://example.invalid/mcp".into(),
            },
            env: Vec::new(),
            cwd: None,
        },
    )
    .expect_err("unsupported transport URL should fail closed");
    assert_eq!(invalid_transport.code, "mcp_registry_invalid");
    assert!(
        !registry_path.exists(),
        "failed updates must not create an MCP registry file"
    );

    let upserted = upsert_mcp_server(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertMcpServerRequestDto {
            id: "memory".into(),
            name: "Memory".into(),
            transport: McpTransportDto::Stdio {
                command: "npx".into(),
                args: vec![
                    "@modelcontextprotocol/server-memory".into(),
                    "--stdio".into(),
                ],
            },
            env: vec![McpEnvironmentReferenceDto {
                key: "API_TOKEN".into(),
                from_env: "MCP_BRIDGE_SECRET_TOKEN".into(),
            }],
            cwd: Some(" /tmp ".into()),
        },
    )
    .expect("valid MCP upsert should persist");

    assert_eq!(upserted.servers.len(), 1);
    assert_eq!(upserted.servers[0].id, "memory");
    assert_eq!(
        upserted.servers[0].connection.status,
        McpConnectionStatusDto::Stale
    );
    assert_eq!(
        upserted.servers[0]
            .connection
            .diagnostic
            .as_ref()
            .map(|item| item.code.as_str()),
        Some("mcp_status_unchecked")
    );

    let persisted = std::fs::read_to_string(&registry_path).expect("read persisted MCP registry");
    assert!(persisted.contains("\"fromEnv\": \"MCP_BRIDGE_SECRET_TOKEN\""));
    assert!(
        !persisted.contains("super-secret-bridge-token"),
        "persisted MCP registry must not include resolved secret values"
    );

    let import_path = root.path().join("mcp-import.json");
    std::fs::write(
        &import_path,
        serde_json::to_vec_pretty(&serde_json::json!([
            {
                "id": "filesystem",
                "name": "Filesystem",
                "transport": { "kind": "http", "url": "https://files.example.com/mcp" },
                "updatedAt": "2026-04-24T00:01:00Z"
            },
            {
                "id": "filesystem",
                "name": "Filesystem Duplicate",
                "transport": { "kind": "http", "url": "https://files.example.com/mcp" },
                "updatedAt": "2026-04-24T00:01:01Z"
            },
            {
                "id": "bad-url",
                "name": "Bad URL",
                "transport": { "kind": "http", "url": "ftp://files.example.com/mcp" },
                "updatedAt": "2026-04-24T00:01:02Z"
            }
        ]))
        .expect("serialize MCP import fixture"),
    )
    .expect("write MCP import fixture");

    let imported = import_mcp_servers(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ImportMcpServersRequestDto {
            path: import_path.to_string_lossy().into_owned(),
        },
    )
    .expect("MCP import should succeed with diagnostics for bad entries");

    assert_eq!(imported.registry.servers.len(), 2);
    assert_eq!(imported.registry.servers[0].id, "filesystem");
    assert_eq!(imported.registry.servers[1].id, "memory");
    assert_eq!(imported.diagnostics.len(), 2);
    assert!(imported
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.code == "mcp_registry_import_invalid"));

    let blank_remove = remove_mcp_server(
        app.handle().clone(),
        app.state::<DesktopState>(),
        RemoveMcpServerRequestDto {
            server_id: "   ".into(),
        },
    )
    .expect_err("blank remove id should fail");
    assert_eq!(blank_remove, CommandError::invalid_request("serverId"));

    let removed = remove_mcp_server(
        app.handle().clone(),
        app.state::<DesktopState>(),
        RemoveMcpServerRequestDto {
            server_id: "filesystem".into(),
        },
    )
    .expect("remove should delete imported server");
    assert_eq!(removed.servers.len(), 1);
    assert_eq!(removed.servers[0].id, "memory");

    let missing_remove = remove_mcp_server(
        app.handle().clone(),
        app.state::<DesktopState>(),
        RemoveMcpServerRequestDto {
            server_id: "missing".into(),
        },
    )
    .expect_err("removing unknown server should fail closed");
    assert_eq!(missing_remove.code, "mcp_server_not_found");
}

#[test]
fn refresh_mcp_server_statuses_projects_fail_closed_truth_and_preserves_last_healthy_snapshot() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let registry_path = mcp_registry_path(&root);

    let connected_url =
        spawn_single_response_server(200, "application/json", "{\"ok\":true}", Duration::ZERO);
    let misconfigured_sse_url =
        spawn_single_response_server(200, "text/plain", "not-sse", Duration::ZERO);
    let stale_url = spawn_single_response_server(
        200,
        "application/json",
        "{\"slow\":true}",
        Duration::from_millis(1_600),
    );

    let previous_healthy_at = "2026-04-24T00:00:00Z";
    let seeded_registry = McpRegistry {
        version: 1,
        servers: vec![
            server_record(
                "connected-http",
                "Connected HTTP",
                McpTransport::Http {
                    url: connected_url.clone(),
                },
                Vec::new(),
                stale_connection(),
            ),
            server_record(
                "blocked-env",
                "Blocked Env",
                McpTransport::Http { url: connected_url },
                vec![McpEnvironmentReference {
                    key: "API_TOKEN".into(),
                    from_env: "__CADENCE_TEST_MCP_MISSING_ENV_53D9E7F2__".into(),
                }],
                stale_connection(),
            ),
            server_record(
                "misconfigured-sse",
                "Misconfigured SSE",
                McpTransport::Sse {
                    url: misconfigured_sse_url,
                },
                Vec::new(),
                stale_connection(),
            ),
            server_record(
                "failed-stdio",
                "Failed Stdio",
                McpTransport::Stdio {
                    command: "cadence-mcp-this-command-should-not-exist".into(),
                    args: Vec::new(),
                },
                Vec::new(),
                stale_connection(),
            ),
            server_record(
                "stale-timeout",
                "Stale Timeout",
                McpTransport::Http { url: stale_url },
                Vec::new(),
                stale_connection(),
            ),
            server_record(
                "previously-healthy",
                "Previously Healthy",
                McpTransport::Stdio {
                    command: "cadence-mcp-this-command-should-not-exist-2".into(),
                    args: Vec::new(),
                },
                Vec::new(),
                connected_connection(previous_healthy_at),
            ),
        ],
        updated_at: "2026-04-24T00:00:00Z".into(),
    };

    persist_mcp_registry(&registry_path, &seeded_registry)
        .expect("persist seeded MCP registry for refresh test");

    let refreshed = refresh_mcp_server_statuses(
        app.handle().clone(),
        app.state::<DesktopState>(),
        RefreshMcpServerStatusesRequestDto {
            server_ids: Vec::new(),
        },
    )
    .expect("refresh MCP statuses should project per-server truth");

    let connected = server_by_id(&refreshed, "connected-http");
    assert_eq!(
        connected.connection.status,
        McpConnectionStatusDto::Connected
    );
    assert!(connected.connection.diagnostic.is_none());
    assert!(connected.connection.last_checked_at.is_some());
    assert!(connected.connection.last_healthy_at.is_some());

    let blocked = server_by_id(&refreshed, "blocked-env");
    assert_eq!(blocked.connection.status, McpConnectionStatusDto::Blocked);
    assert_eq!(
        blocked
            .connection
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("mcp_probe_env_missing")
    );

    let misconfigured = server_by_id(&refreshed, "misconfigured-sse");
    assert_eq!(
        misconfigured.connection.status,
        McpConnectionStatusDto::Misconfigured
    );
    assert_eq!(
        misconfigured
            .connection
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("mcp_probe_sse_malformed_response")
    );

    let failed = server_by_id(&refreshed, "failed-stdio");
    assert_eq!(failed.connection.status, McpConnectionStatusDto::Failed);
    assert_eq!(
        failed
            .connection
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("mcp_probe_spawn_failed")
    );

    let stale = server_by_id(&refreshed, "stale-timeout");
    assert_eq!(stale.connection.status, McpConnectionStatusDto::Stale);
    let stale_diagnostic = stale
        .connection
        .diagnostic
        .as_ref()
        .expect("stale status should include timeout diagnostics");
    assert_eq!(stale_diagnostic.code, "mcp_probe_timeout");
    assert!(stale_diagnostic.retryable);

    let previously_healthy = server_by_id(&refreshed, "previously-healthy");
    assert_eq!(
        previously_healthy.connection.status,
        McpConnectionStatusDto::Failed
    );
    assert_eq!(
        previously_healthy.connection.last_healthy_at.as_deref(),
        Some(previous_healthy_at),
        "refresh failures must retain the last truthful healthy timestamp"
    );

    let listed = list_mcp_servers(app.handle().clone(), app.state::<DesktopState>())
        .expect("listing after refresh should return persisted status truth");
    assert_eq!(
        server_by_id(&listed, "previously-healthy")
            .connection
            .last_healthy_at,
        previously_healthy.connection.last_healthy_at,
    );

    let missing_selection = refresh_mcp_server_statuses(
        app.handle().clone(),
        app.state::<DesktopState>(),
        RefreshMcpServerStatusesRequestDto {
            server_ids: vec!["missing-server".into()],
        },
    )
    .expect_err("refreshing an unknown server id should fail closed");
    assert_eq!(missing_selection.code, "mcp_server_not_found");
}
