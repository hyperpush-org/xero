use std::fs;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value as JsonValue};

use super::{
    global_database_path, now_timestamp, response, take_help, take_option, CliError, CliResponse,
    GlobalOptions,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentStatusSummary {
    has_profile: bool,
    status: String,
    should_start: bool,
    refreshed_at: Option<String>,
    probe_started_at: Option<String>,
    probe_completed_at: Option<String>,
    permission_request_count: usize,
    diagnostic_count: usize,
    tool_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UserEnvironmentToolSummary {
    id: String,
    category: String,
    command: String,
    args: Vec<String>,
    updated_at: String,
}

pub(crate) fn dispatch_environment(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("status") => command_environment_status(globals, args[1..].to_vec()),
        Some("profile") => command_environment_profile(globals, args[1..].to_vec()),
        Some("user-tools") | Some("tools") => {
            command_environment_user_tools(globals, args[1..].to_vec())
        }
        Some("save-tool") | Some("upsert-tool") => {
            command_environment_save_tool(globals, args[1..].to_vec())
        }
        Some("remove-tool") | Some("delete-tool") => {
            command_environment_remove_tool(globals, args[1..].to_vec())
        }
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero environment status|profile|user-tools|save-tool|remove-tool\nReads and mutates shared global environment-discovery user-tool tables; probing and permission resolution stay on the desktop service until extracted.",
            json!({ "command": "environment" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown environment command `{other}`. Use status, profile, user-tools, save-tool, or remove-tool."
        ))),
    }
}

pub(crate) fn dispatch_settings(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("agent-tooling") => command_agent_tooling_settings(globals, args[1..].to_vec()),
        Some("browser-control") => command_browser_control_settings(globals, args[1..].to_vec()),
        Some("soul") | Some("behavior") => command_soul_settings(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero settings agent-tooling|browser-control|soul\nReads shared global settings tables used by the desktop autonomous runtime.",
            json!({ "command": "settings" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown settings command `{other}`. Use agent-tooling, browser-control, or soul."
        ))),
    }
}

fn command_environment_status(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero environment status",
            json!({ "command": "environment status" }),
        ));
    }
    reject_settings_unknown_options(&args)?;
    let summary = environment_status_summary(&globals)?;
    Ok(response(
        &globals,
        if summary.has_profile {
            format!(
                "Environment discovery status={} permissions={} diagnostics={} tools={}.",
                summary.status,
                summary.permission_request_count,
                summary.diagnostic_count,
                summary.tool_count
            )
        } else {
            "No environment profile has been discovered in global app-data yet.".into()
        },
        json!({ "kind": "environmentStatus", "status": summary }),
    ))
}

fn command_environment_profile(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero environment profile",
            json!({ "command": "environment profile" }),
        ));
    }
    reject_settings_unknown_options(&args)?;
    let profile = environment_profile_summary(&globals)?;
    let text = if profile.is_null() {
        "No environment profile summary is available.".into()
    } else {
        let status = profile
            .get("status")
            .and_then(JsonValue::as_str)
            .unwrap_or("ready");
        let tools = profile
            .get("tools")
            .and_then(JsonValue::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        format!("Environment profile status={status} tools={tools}.")
    };
    Ok(response(
        &globals,
        text,
        json!({ "kind": "environmentProfile", "profile": profile }),
    ))
}

fn command_environment_user_tools(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero environment user-tools",
            json!({ "command": "environment user-tools" }),
        ));
    }
    reject_settings_unknown_options(&args)?;
    let tools = load_user_environment_tools(&globals)?;
    let text = if tools.is_empty() {
        "No user-added environment tools are saved in global app-data.".into()
    } else {
        tools
            .iter()
            .map(|tool| format!("{} [{}] {}", tool.id, tool.category, tool.command))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({ "kind": "environmentUserTools", "tools": tools }),
    ))
}

fn command_environment_save_tool(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero environment save-tool TOOL_ID --category CATEGORY --command COMMAND [--args JSON_ARRAY]",
            json!({ "command": "environment save-tool" }),
        ));
    }
    let tool_id = take_option(&mut args, "--id")?
        .or_else(|| take_optional_positional(&mut args))
        .ok_or_else(|| CliError::usage("Missing environment tool id."))?;
    let category = take_option(&mut args, "--category")?
        .ok_or_else(|| CliError::usage("Missing `--category`."))?;
    let command = take_option(&mut args, "--command")?
        .ok_or_else(|| CliError::usage("Missing `--command`."))?;
    let tool_args = take_option(&mut args, "--args")?
        .map(|payload| parse_string_array_payload("args", &payload))
        .transpose()?
        .unwrap_or_default();
    reject_settings_unknown_options(&args)?;
    validate_environment_tool_input(&tool_id, &category, &command)?;
    let connection = open_global_database_for_write(&globals)?;
    ensure_user_environment_tools_table(&connection)?;
    let args_json = serde_json::to_string(&tool_args).map_err(|error| {
        CliError::system_fault(
            "xero_cli_environment_tools_encode_failed",
            format!("Could not encode environment tool args: {error}"),
        )
    })?;
    let updated_at = now_timestamp();
    connection
        .execute(
            r#"
            INSERT INTO user_added_environment_tools (id, category, command, args_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET
                category = excluded.category,
                command = excluded.command,
                args_json = excluded.args_json,
                updated_at = excluded.updated_at
            "#,
            params![tool_id, category, command, args_json, updated_at],
        )
        .map_err(|error| sqlite_settings_error("xero_cli_environment_tools_write_failed", error))?;
    let tools = load_user_environment_tools(&globals)?;
    Ok(response(
        &globals,
        format!("Saved user environment tool `{tool_id}`."),
        json!({
            "kind": "environmentToolSave",
            "toolId": tool_id,
            "tools": tools
        }),
    ))
}

fn command_environment_remove_tool(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero environment remove-tool TOOL_ID",
            json!({ "command": "environment remove-tool" }),
        ));
    }
    let tool_id = take_option(&mut args, "--id")?
        .or_else(|| take_optional_positional(&mut args))
        .ok_or_else(|| CliError::usage("Missing environment tool id."))?;
    reject_settings_unknown_options(&args)?;
    let connection = open_global_database_for_write(&globals)?;
    ensure_user_environment_tools_table(&connection)?;
    let removed = connection
        .execute(
            "DELETE FROM user_added_environment_tools WHERE id = ?1",
            params![tool_id],
        )
        .map_err(|error| {
            sqlite_settings_error("xero_cli_environment_tools_remove_failed", error)
        })?;
    if removed == 0 {
        return Err(CliError::user_fixable(
            "xero_cli_environment_tool_unknown",
            format!("User environment tool `{tool_id}` was not found."),
        ));
    }
    let tools = load_user_environment_tools(&globals)?;
    Ok(response(
        &globals,
        format!("Removed user environment tool `{tool_id}`."),
        json!({
            "kind": "environmentToolRemove",
            "toolId": tool_id,
            "tools": tools
        }),
    ))
}

fn command_agent_tooling_settings(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero settings agent-tooling",
            json!({ "command": "settings agent-tooling" }),
        ));
    }
    reject_settings_unknown_options(&args)?;
    let settings = load_settings_payload(
        &globals,
        "agent_tooling_settings",
        default_agent_tooling_settings(),
    )?;
    let global_default = settings
        .get("globalDefault")
        .and_then(JsonValue::as_str)
        .unwrap_or("balanced");
    let overrides = settings
        .get("modelOverrides")
        .and_then(JsonValue::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    Ok(response(
        &globals,
        format!("Agent tooling global default={global_default} overrides={overrides}."),
        json!({ "kind": "agentToolingSettings", "settings": settings }),
    ))
}

fn command_browser_control_settings(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero settings browser-control",
            json!({ "command": "settings browser-control" }),
        ));
    }
    reject_settings_unknown_options(&args)?;
    let settings = load_settings_payload(
        &globals,
        "browser_control_settings",
        default_browser_control_settings(),
    )?;
    let preference = settings
        .get("preference")
        .and_then(JsonValue::as_str)
        .unwrap_or("default");
    Ok(response(
        &globals,
        format!("Browser-control preference={preference}."),
        json!({ "kind": "browserControlSettings", "settings": settings }),
    ))
}

fn command_soul_settings(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero settings soul",
            json!({ "command": "settings soul" }),
        ));
    }
    reject_settings_unknown_options(&args)?;
    let settings = load_settings_payload(&globals, "soul_settings", default_soul_settings())?;
    let selected = settings
        .get("selectedSoulId")
        .and_then(JsonValue::as_str)
        .unwrap_or("steward");
    Ok(response(
        &globals,
        format!("Soul/behavior preset={selected}."),
        json!({ "kind": "soulSettings", "settings": settings }),
    ))
}

fn environment_status_summary(
    globals: &GlobalOptions,
) -> Result<EnvironmentStatusSummary, CliError> {
    let Some(connection) = open_existing_global_database(globals)? else {
        return Ok(EnvironmentStatusSummary {
            has_profile: false,
            status: "missing".into(),
            should_start: true,
            refreshed_at: None,
            probe_started_at: None,
            probe_completed_at: None,
            permission_request_count: 0,
            diagnostic_count: 0,
            tool_count: 0,
        });
    };
    if !table_exists(&connection, "environment_profile")? {
        return Ok(EnvironmentStatusSummary {
            has_profile: false,
            status: "missing".into(),
            should_start: true,
            refreshed_at: None,
            probe_started_at: None,
            probe_completed_at: None,
            permission_request_count: 0,
            diagnostic_count: 0,
            tool_count: 0,
        });
    }
    let row = connection
        .query_row(
            "SELECT status, refreshed_at, probe_started_at, probe_completed_at, permission_requests_json, diagnostics_json, summary_json FROM environment_profile WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|error| sqlite_settings_error("xero_cli_environment_status_read_failed", error))?;
    let Some((
        status,
        refreshed_at,
        probe_started_at,
        probe_completed_at,
        permissions_json,
        diagnostics_json,
        summary_json,
    )) = row
    else {
        return Ok(EnvironmentStatusSummary {
            has_profile: false,
            status: "missing".into(),
            should_start: true,
            refreshed_at: None,
            probe_started_at: None,
            probe_completed_at: None,
            permission_request_count: 0,
            diagnostic_count: 0,
            tool_count: 0,
        });
    };
    let permissions = json_array_len(&permissions_json)?;
    let diagnostics = json_array_len(&diagnostics_json)?;
    let summary = parse_json_payload(&summary_json)?;
    let tool_count = summary
        .get("tools")
        .and_then(JsonValue::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    Ok(EnvironmentStatusSummary {
        has_profile: true,
        should_start: matches!(status.as_str(), "failed" | "pending"),
        status,
        refreshed_at,
        probe_started_at,
        probe_completed_at,
        permission_request_count: permissions,
        diagnostic_count: diagnostics,
        tool_count,
    })
}

fn environment_profile_summary(globals: &GlobalOptions) -> Result<JsonValue, CliError> {
    let Some(connection) = open_existing_global_database(globals)? else {
        return Ok(JsonValue::Null);
    };
    if !table_exists(&connection, "environment_profile")? {
        return Ok(JsonValue::Null);
    }
    let payload = connection
        .query_row(
            "SELECT summary_json FROM environment_profile WHERE id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| {
            sqlite_settings_error("xero_cli_environment_profile_read_failed", error)
        })?;
    payload
        .map(|payload| parse_json_payload(&payload))
        .transpose()
        .map(|value| value.unwrap_or(JsonValue::Null))
}

fn load_user_environment_tools(
    globals: &GlobalOptions,
) -> Result<Vec<UserEnvironmentToolSummary>, CliError> {
    let Some(connection) = open_existing_global_database(globals)? else {
        return Ok(Vec::new());
    };
    if !table_exists(&connection, "user_added_environment_tools")? {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare(
            "SELECT id, category, command, args_json, updated_at FROM user_added_environment_tools ORDER BY id ASC",
        )
        .map_err(|error| sqlite_settings_error("xero_cli_environment_tools_read_failed", error))?;
    let rows = statement
        .query_map([], |row| {
            let args_json = row.get::<_, String>(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                args_json,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| sqlite_settings_error("xero_cli_environment_tools_read_failed", error))?;
    let mut tools = Vec::new();
    for row in rows {
        let (id, category, command, args_json, updated_at) = row.map_err(|error| {
            sqlite_settings_error("xero_cli_environment_tools_read_failed", error)
        })?;
        let args = serde_json::from_str::<Vec<String>>(&args_json).map_err(|error| {
            CliError::system_fault(
                "xero_cli_environment_tools_decode_failed",
                format!("Could not decode environment tool args JSON: {error}"),
            )
        })?;
        tools.push(UserEnvironmentToolSummary {
            id,
            category,
            command,
            args,
            updated_at,
        });
    }
    Ok(tools)
}

fn load_settings_payload(
    globals: &GlobalOptions,
    table: &str,
    default_payload: JsonValue,
) -> Result<JsonValue, CliError> {
    let Some(connection) = open_existing_global_database(globals)? else {
        return Ok(default_payload);
    };
    if !table_exists(&connection, table)? {
        return Ok(default_payload);
    }
    let sql = format!("SELECT payload FROM {table} WHERE id = 1");
    let payload = connection
        .query_row(sql.as_str(), [], |row| row.get::<_, String>(0))
        .optional()
        .map_err(|error| sqlite_settings_error("xero_cli_settings_read_failed", error))?;
    payload
        .map(|payload| parse_json_payload(&payload))
        .transpose()
        .map(|value| value.unwrap_or(default_payload))
}

fn default_agent_tooling_settings() -> JsonValue {
    json!({
        "schemaVersion": 1,
        "globalDefault": "balanced",
        "modelOverrides": [],
        "updatedAt": JsonValue::Null,
    })
}

fn default_browser_control_settings() -> JsonValue {
    json!({
        "schemaVersion": 1,
        "preference": "default",
        "updatedAt": JsonValue::Null,
    })
}

fn default_soul_settings() -> JsonValue {
    json!({
        "schemaVersion": 1,
        "selectedSoulId": "steward",
        "updatedAt": JsonValue::Null,
    })
}

fn open_existing_global_database(globals: &GlobalOptions) -> Result<Option<Connection>, CliError> {
    let path = global_database_path(globals);
    if !path.exists() {
        return Ok(None);
    }
    Connection::open(&path).map(Some).map_err(|error| {
        CliError::system_fault(
            "xero_cli_global_settings_open_failed",
            format!(
                "Could not open global app-data database `{}`: {error}",
                path.display()
            ),
        )
    })
}

fn open_global_database_for_write(globals: &GlobalOptions) -> Result<Connection, CliError> {
    let path = global_database_path(globals);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::system_fault(
                "xero_cli_global_settings_prepare_failed",
                format!(
                    "Could not create global app-data directory `{}`: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    Connection::open(&path).map_err(|error| {
        CliError::system_fault(
            "xero_cli_global_settings_open_failed",
            format!(
                "Could not open global app-data database `{}`: {error}",
                path.display()
            ),
        )
    })
}

fn ensure_user_environment_tools_table(connection: &Connection) -> Result<(), CliError> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS user_added_environment_tools (
                id TEXT PRIMARY KEY,
                category TEXT NOT NULL,
                command TEXT NOT NULL,
                args_json TEXT NOT NULL DEFAULT '[]',
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .map_err(|error| sqlite_settings_error("xero_cli_environment_tools_schema_failed", error))
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, CliError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            params![table],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(|error| sqlite_settings_error("xero_cli_settings_table_probe_failed", error))
}

fn parse_string_array_payload(label: &str, payload: &str) -> Result<Vec<String>, CliError> {
    serde_json::from_str::<Vec<String>>(payload).map_err(|error| {
        CliError::usage(format!(
            "`--{label}` must be a JSON array of strings: {error}"
        ))
    })
}

fn validate_environment_tool_input(
    tool_id: &str,
    category: &str,
    command: &str,
) -> Result<(), CliError> {
    if tool_id.trim().is_empty() {
        return Err(CliError::usage("Environment tool id cannot be empty."));
    }
    if category.trim().is_empty() {
        return Err(CliError::usage(
            "Environment tool category cannot be empty.",
        ));
    }
    if command.trim().is_empty() {
        return Err(CliError::usage("Environment tool command cannot be empty."));
    }
    Ok(())
}

fn take_optional_positional(args: &mut Vec<String>) -> Option<String> {
    if args.first().is_some_and(|arg| !arg.starts_with('-')) {
        Some(args.remove(0))
    } else {
        None
    }
}

fn json_array_len(payload: &str) -> Result<usize, CliError> {
    parse_json_payload(payload).map(|value| value.as_array().map(Vec::len).unwrap_or(0))
}

fn parse_json_payload(payload: &str) -> Result<JsonValue, CliError> {
    serde_json::from_str(payload).map_err(|error| {
        CliError::system_fault(
            "xero_cli_settings_json_decode_failed",
            format!("Could not decode shared settings JSON: {error}"),
        )
    })
}

fn reject_settings_unknown_options(args: &[String]) -> Result<(), CliError> {
    if let Some(option) = args.iter().find(|arg| arg.starts_with('-')) {
        return Err(CliError::usage(format!("Unknown option `{option}`.")));
    }
    if let Some(extra) = args.first() {
        return Err(CliError::usage(format!("Unexpected argument `{extra}`.")));
    }
    Ok(())
}

fn sqlite_settings_error(code: &'static str, error: rusqlite::Error) -> CliError {
    CliError::system_fault(code, format!("SQLite operation failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn environment_status_reads_shared_global_profile_tables() {
        let state_dir = temp_dir("settings-cli-state");
        let globals = GlobalOptions {
            output_mode: super::super::OutputMode::Json,
            ci: false,
            state_dir: state_dir.clone(),
            tui_adapter: None,
        };
        fs::create_dir_all(&state_dir).expect("state dir");
        let connection = Connection::open(state_dir.join("xero.db")).expect("open db");
        connection
            .execute_batch(
                r#"
                CREATE TABLE environment_profile (
                    id INTEGER PRIMARY KEY,
                    status TEXT NOT NULL,
                    refreshed_at TEXT,
                    probe_started_at TEXT,
                    probe_completed_at TEXT,
                    permission_requests_json TEXT NOT NULL,
                    diagnostics_json TEXT NOT NULL,
                    summary_json TEXT NOT NULL
                );
                INSERT INTO environment_profile (
                    id, status, refreshed_at, probe_started_at, probe_completed_at,
                    permission_requests_json, diagnostics_json, summary_json
                )
                VALUES (
                    1, 'ready', '2026-05-15T00:00:00Z', NULL, '2026-05-15T00:00:01Z',
                    '[{"id":"path"}]', '[{"code":"ok"}]', '{"tools":[{"id":"git"},{"id":"node"}]}'
                );
                "#,
            )
            .expect("seed environment profile");

        let summary = environment_status_summary(&globals).expect("environment status");
        assert!(summary.has_profile);
        assert_eq!(summary.status, "ready");
        assert_eq!(summary.permission_request_count, 1);
        assert_eq!(summary.diagnostic_count, 1);
        assert_eq!(summary.tool_count, 2);
    }

    #[test]
    fn settings_defaults_match_shared_runtime_contracts_when_tables_absent() {
        let globals = GlobalOptions {
            output_mode: super::super::OutputMode::Json,
            ci: false,
            state_dir: temp_dir("settings-cli-empty"),
            tui_adapter: None,
        };
        let agent_tooling = load_settings_payload(
            &globals,
            "agent_tooling_settings",
            default_agent_tooling_settings(),
        )
        .expect("agent tooling settings");
        assert_eq!(agent_tooling["globalDefault"], json!("balanced"));

        let browser = load_settings_payload(
            &globals,
            "browser_control_settings",
            default_browser_control_settings(),
        )
        .expect("browser settings");
        assert_eq!(browser["preference"], json!("default"));
    }

    #[test]
    fn environment_save_and_remove_tool_mutates_shared_global_table() {
        let state_dir = temp_dir("settings-tool-state");
        let saved = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "environment",
            "save-tool",
            "local-protoc",
            "--category",
            "build",
            "--command",
            "protoc",
            "--args",
            "[\"--version\"]",
        ])
        .expect("save tool");
        assert_eq!(saved.json["kind"], json!("environmentToolSave"));
        assert_eq!(saved.json["tools"][0]["id"], json!("local-protoc"));

        let listed = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "environment",
            "user-tools",
        ])
        .expect("list tools");
        assert_eq!(listed.json["tools"][0]["args"][0], json!("--version"));

        let removed = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "environment",
            "remove-tool",
            "local-protoc",
        ])
        .expect("remove tool");
        assert!(removed.json["tools"].as_array().expect("tools").is_empty());
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("xero-cli-{prefix}-{nonce}"));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }
}
