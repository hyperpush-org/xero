use std::path::{Path, PathBuf};

use serde_json::json;
use xero_desktop_lib::{
    commands::CommandError,
    global_db::open_global_database,
    mcp::{
        apply_mcp_registry_import, default_mcp_registry, load_mcp_registry_from_path,
        parse_mcp_registry_import_file, persist_mcp_registry, McpConnectionDiagnostic,
        McpConnectionState, McpConnectionStatus, McpEnvironmentReference, McpRegistry,
        McpServerRecord, McpTransport,
    },
};

fn registry_path(root: &tempfile::TempDir) -> PathBuf {
    root.path().join("app-data").join("xero.db")
}

fn write_registry_payload_rows(path: &Path, rows: Vec<serde_json::Value>) {
    let connection = open_global_database(path).expect("open global database");
    for (index, row) in rows.into_iter().enumerate() {
        let payload = serde_json::to_string(&row).expect("serialize registry row");
        let server_id = format!("fixture-row-{index}");
        connection
            .execute(
                "INSERT INTO mcp_registry (server_id, payload, updated_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![server_id, payload, "2026-04-23T23:00:00Z"],
            )
            .expect("insert registry row");
    }
}

fn stale_connection() -> McpConnectionState {
    McpConnectionState {
        status: McpConnectionStatus::Stale,
        diagnostic: Some(McpConnectionDiagnostic {
            code: "mcp_status_unchecked".into(),
            message: "not checked yet".into(),
            retryable: true,
        }),
        last_checked_at: None,
        last_healthy_at: None,
    }
}

fn server(id: &str, name: &str, transport: McpTransport) -> McpServerRecord {
    McpServerRecord {
        id: id.into(),
        name: name.into(),
        transport,
        env: Vec::new(),
        cwd: None,
        connection: stale_connection(),
        updated_at: "2026-04-23T23:00:00Z".into(),
    }
}

#[test]
fn load_bootstraps_default_registry_when_file_is_missing() {
    let root = tempfile::tempdir().expect("temp dir");
    let path = registry_path(&root);

    let registry = load_mcp_registry_from_path(&path).expect("load default registry");

    assert_eq!(registry.version, 1);
    assert!(registry.servers.is_empty());
    assert!(path.exists());
}

#[test]
fn load_accepts_empty_global_database_bootstrap_shape() {
    let root = tempfile::tempdir().expect("temp dir");
    let path = registry_path(&root);
    open_global_database(&path).expect("create empty global database");

    let registry = load_mcp_registry_from_path(&path).expect("load empty registry bootstrap");

    assert_eq!(registry.version, 1);
    assert!(registry.servers.is_empty());
    assert!(!registry.updated_at.trim().is_empty());
}

#[test]
fn persist_creates_registry_file_and_normalizes_ordering() {
    let root = tempfile::tempdir().expect("temp dir");
    let path = registry_path(&root);

    let mut beta = server(
        "beta",
        "  Beta Server  ",
        McpTransport::Stdio {
            command: "  npx ".into(),
            args: vec![
                "  @modelcontextprotocol/server-memory ".into(),
                "--stdio".into(),
            ],
        },
    );
    beta.env = vec![
        McpEnvironmentReference {
            key: "ZETA_KEY".into(),
            from_env: "MCP_ZETA_KEY".into(),
        },
        McpEnvironmentReference {
            key: "ALPHA_KEY".into(),
            from_env: "MCP_ALPHA_KEY".into(),
        },
    ];

    let alpha = server(
        "alpha",
        "Alpha Server",
        McpTransport::Http {
            url: "https://mcp.example.com/http".into(),
        },
    );

    let persisted = persist_mcp_registry(
        &path,
        &McpRegistry {
            version: 1,
            servers: vec![beta, alpha],
            updated_at: "2026-04-23T23:00:00Z".into(),
        },
    )
    .expect("persist registry");

    assert!(path.exists());
    assert_eq!(persisted.servers.len(), 2);
    assert_eq!(persisted.servers[0].id, "alpha");
    assert_eq!(persisted.servers[1].id, "beta");

    assert_eq!(persisted.servers[1].name, "Beta Server");
    assert_eq!(persisted.servers[1].env[0].key, "ALPHA_KEY");
    assert_eq!(persisted.servers[1].env[1].key, "ZETA_KEY");

    match &persisted.servers[1].transport {
        McpTransport::Stdio { command, args } => {
            assert_eq!(command, "npx");
            assert_eq!(
                args,
                &vec![
                    "@modelcontextprotocol/server-memory".to_string(),
                    "--stdio".to_string()
                ]
            );
        }
        other => panic!("expected stdio transport, got {other:?}"),
    }
}

#[test]
fn load_rejects_blank_server_id() {
    let root = tempfile::tempdir().expect("temp dir");
    let path = registry_path(&root);

    write_registry_payload_rows(
        &path,
        vec![json!({
            "id": "   ",
            "name": "Bad",
            "transport": {"kind": "stdio", "command": "node"},
            "updatedAt": "2026-04-23T23:00:00Z"
        })],
    );

    let error = load_mcp_registry_from_path(&path).expect_err("blank server id should fail");

    assert_eq!(error.code, "mcp_registry_invalid");
    assert!(error.message.contains("`id` was blank"));
}

#[test]
fn load_rejects_unsupported_transport_url_scheme() {
    let root = tempfile::tempdir().expect("temp dir");
    let path = registry_path(&root);

    write_registry_payload_rows(
        &path,
        vec![json!({
            "id": "bad-http",
            "name": "Bad",
            "transport": {"kind": "http", "url": "ftp://mcp.example.com"},
            "updatedAt": "2026-04-23T23:00:00Z"
        })],
    );

    let error =
        load_mcp_registry_from_path(&path).expect_err("unsupported transport scheme should fail");

    assert_eq!(error.code, "mcp_registry_invalid");
    assert!(error.message.contains("unsupported scheme"));
}

#[test]
fn load_rejects_duplicate_server_ids() {
    let root = tempfile::tempdir().expect("temp dir");
    let path = registry_path(&root);

    write_registry_payload_rows(
        &path,
        vec![
            json!({
                "id": "dupe",
                "name": "First",
                "transport": {"kind": "stdio", "command": "node"},
                "updatedAt": "2026-04-23T23:00:00Z"
            }),
            json!({
                "id": "dupe",
                "name": "Second",
                "transport": {"kind": "stdio", "command": "node"},
                "updatedAt": "2026-04-23T23:00:00Z"
            }),
        ],
    );

    let error = load_mcp_registry_from_path(&path).expect_err("duplicate server id should fail");

    assert_eq!(error.code, "mcp_registry_invalid");
    assert!(error.message.contains("duplicated"));
}

#[test]
fn parse_import_file_reports_typed_error_for_malformed_json() {
    let root = tempfile::tempdir().expect("temp dir");
    let path = root.path().join("import.json");
    std::fs::write(&path, "{ definitely not json").expect("write malformed json");

    let error = parse_mcp_registry_import_file(&path).expect_err("malformed import should fail");

    assert_eq!(error.code, "mcp_registry_import_invalid");
    assert!(error.message.contains(path.to_string_lossy().as_ref()));
}

#[test]
fn import_normalizes_valid_entries_and_reports_invalid_rows() {
    let current = McpRegistry {
        version: 1,
        servers: vec![server(
            "alpha",
            "Alpha",
            McpTransport::Http {
                url: "https://alpha.example.com/mcp".into(),
            },
        )],
        updated_at: "2026-04-23T23:00:00Z".into(),
    };

    let source_path = PathBuf::from("/tmp/import.json");
    let result = apply_mcp_registry_import(
        &current,
        vec![
            json!({
                "id": "alpha",
                "name": "Alpha Updated",
                "transport": {"kind": "http", "url": "https://alpha-updated.example.com/mcp"},
                "updatedAt": "2026-04-23T23:00:01Z"
            }),
            json!({
                "id": "beta",
                "name": "Beta",
                "transport": {"kind": "stdio", "command": "npx", "args": ["@scope/server"]},
                "env": [
                    {"key": "ZETA_KEY", "fromEnv": "MCP_ZETA_KEY"},
                    {"key": "ALPHA_KEY", "fromEnv": "MCP_ALPHA_KEY"}
                ],
                "updatedAt": "2026-04-23T23:00:01Z"
            }),
            json!({
                "id": "gamma",
                "name": "   ",
                "transport": {"kind": "stdio", "command": "node"},
                "updatedAt": "2026-04-23T23:00:01Z"
            }),
            json!({
                "id": "delta",
                "name": "Delta",
                "transport": {"kind": "socket", "url": "https://delta.example.com"},
                "updatedAt": "2026-04-23T23:00:01Z"
            }),
            json!({
                "id": "beta",
                "name": "Beta Duplicate",
                "transport": {"kind": "stdio", "command": "node"},
                "updatedAt": "2026-04-23T23:00:01Z"
            }),
        ],
        &source_path,
    );

    assert_eq!(result.registry.servers.len(), 2);
    assert_eq!(result.registry.servers[0].id, "alpha");
    assert_eq!(result.registry.servers[0].name, "Alpha Updated");
    assert_eq!(result.registry.servers[1].id, "beta");
    assert_eq!(result.registry.servers[1].env[0].key, "ALPHA_KEY");
    assert_eq!(result.registry.servers[1].env[1].key, "ZETA_KEY");

    assert_eq!(result.diagnostics.len(), 3);
    assert!(result
        .diagnostics
        .iter()
        .all(|diagnostic| diagnostic.code == "mcp_registry_import_invalid"));
}

#[test]
fn import_handles_large_entry_lists_with_deterministic_order() {
    let current = default_mcp_registry();

    let entries = (0..250)
        .rev()
        .map(|index| {
            json!({
                "id": format!("server-{index:03}"),
                "name": format!("Server {index:03}"),
                "transport": {"kind": "http", "url": format!("https://example.com/{index:03}")},
                "updatedAt": "2026-04-23T23:00:01Z"
            })
        })
        .collect::<Vec<_>>();

    let result = apply_mcp_registry_import(&current, entries, Path::new("/tmp/large-import.json"));

    assert!(result.diagnostics.is_empty());
    assert_eq!(result.registry.servers.len(), 250);
    assert_eq!(result.registry.servers[0].id, "server-000");
    assert_eq!(result.registry.servers[249].id, "server-249");
}

#[test]
fn persist_returns_typed_error_when_registry_parent_is_not_a_directory() {
    let root = tempfile::tempdir().expect("temp dir");
    let blocked_parent = root.path().join("blocked-parent");
    std::fs::write(&blocked_parent, "not-a-directory").expect("write blocking file");
    let path = blocked_parent.join("xero.db");

    let error = persist_mcp_registry(&path, &default_mcp_registry())
        .expect_err("write should fail when parent is a file");

    assert_eq!(error.code, "global_database_dir_unavailable");
}

#[test]
fn persist_returns_typed_error_when_atomic_persist_target_is_directory() {
    let root = tempfile::tempdir().expect("temp dir");
    let path = registry_path(&root);
    std::fs::create_dir_all(&path).expect("create destination directory");

    let error = persist_mcp_registry(&path, &default_mcp_registry())
        .expect_err("persist should fail when destination is a directory");

    assert_eq!(error.code, "global_database_open_failed");
}

#[test]
fn persist_keeps_existing_snapshot_when_validation_fails() {
    let root = tempfile::tempdir().expect("temp dir");
    let path = registry_path(&root);

    let baseline = McpRegistry {
        version: 1,
        servers: vec![server(
            "stable",
            "Stable",
            McpTransport::Http {
                url: "https://stable.example.com/mcp".into(),
            },
        )],
        updated_at: "2026-04-23T23:00:00Z".into(),
    };
    persist_mcp_registry(&path, &baseline).expect("persist baseline");
    let before = std::fs::read(&path).expect("read baseline bytes");

    let invalid = McpRegistry {
        version: 1,
        servers: vec![server(
            "   ",
            "Broken",
            McpTransport::Http {
                url: "https://broken.example.com/mcp".into(),
            },
        )],
        updated_at: "2026-04-23T23:10:00Z".into(),
    };

    let error: CommandError = persist_mcp_registry(&path, &invalid)
        .expect_err("invalid update should fail before overwrite");
    assert_eq!(error.code, "mcp_registry_invalid");

    let after = std::fs::read(&path).expect("read bytes after failed update");
    assert_eq!(before, after);
}
