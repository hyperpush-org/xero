use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value as JsonValue};

use super::{
    now_timestamp, project_cli, response, take_bool_flag, take_help, take_option, CliError,
    CliResponse, GlobalOptions,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotificationRouteRow {
    project_id: String,
    route_id: String,
    route_kind: String,
    route_target: String,
    enabled: bool,
    metadata: Option<JsonValue>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotificationDispatchRow {
    id: i64,
    project_id: String,
    action_id: String,
    route_id: String,
    correlation_key: String,
    status: String,
    attempt_count: u32,
    last_attempt_at: Option<String>,
    delivered_at: Option<String>,
    claimed_at: Option<String>,
    last_error_code: Option<String>,
    last_error_message: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NotificationReplyRow {
    id: i64,
    project_id: String,
    action_id: String,
    route_id: String,
    correlation_key: String,
    responder_id: Option<String>,
    reply_text: String,
    status: String,
    rejection_code: Option<String>,
    rejection_message: Option<String>,
    created_at: String,
}

pub(crate) fn dispatch_notification(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("routes") => command_notification_routes(globals, args[1..].to_vec()),
        Some("upsert-route") | Some("route-upsert") => {
            command_notification_upsert_route(globals, args[1..].to_vec())
        }
        Some("remove-route") | Some("route-remove") | Some("delete-route") => {
            command_notification_remove_route(globals, args[1..].to_vec())
        }
        Some("dispatches") => command_notification_dispatches(globals, args[1..].to_vec()),
        Some("replies") => command_notification_replies(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero notification routes|upsert-route|remove-route|dispatches|replies [--project-id ID]\nUses the saved notification approval route, dispatch, and reply tables in project app-data.",
            json!({ "command": "notification" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown notification command `{other}`. Use routes, upsert-route, remove-route, dispatches, or replies."
        ))),
    }
}

fn command_notification_routes(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero notification routes [--project-id ID]",
            json!({ "command": "notification routes" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    reject_notification_unknown_options(&args)?;
    let routes = load_notification_routes(&globals, &project_id)?;
    let text = if routes.is_empty() {
        "No notification routes are configured.".into()
    } else {
        routes
            .iter()
            .map(|route| {
                format!(
                    "{} {:<18} {:<10} {}",
                    if route.enabled { "*" } else { " " },
                    route.route_id,
                    route.route_kind,
                    route.route_target
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({ "kind": "notificationRoutes", "projectId": project_id, "routes": routes }),
    ))
}

fn command_notification_upsert_route(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero notification upsert-route ROUTE_ID --kind KIND --target TARGET [--disabled] [--metadata JSON] [--project-id ID]",
            json!({ "command": "notification upsert-route" }),
        ));
    }
    let disabled = take_bool_flag(&mut args, "--disabled");
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let route_id = take_option(&mut args, "--route-id")?
        .or_else(|| take_optional_positional(&mut args))
        .ok_or_else(|| CliError::usage("Missing route id."))?;
    let route_kind =
        take_option(&mut args, "--kind")?.ok_or_else(|| CliError::usage("Missing `--kind`."))?;
    let route_target = take_option(&mut args, "--target")?
        .ok_or_else(|| CliError::usage("Missing `--target`."))?;
    let metadata = take_option(&mut args, "--metadata")?
        .map(|payload| parse_json_payload_for_cli("metadata", &payload))
        .transpose()?;
    reject_notification_unknown_options(&args)?;
    validate_notification_route_input(&route_id, &route_kind, &route_target)?;
    let connection = project_cli::project_connection(&globals, &project_id)?;
    ensure_notification_routes_table(&connection)?;
    let now = now_timestamp();
    let metadata_json = metadata
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_notification_metadata_encode_failed",
                format!("Could not encode notification metadata: {error}"),
            )
        })?;
    connection
        .execute(
            r#"
            INSERT INTO notification_routes (
                project_id, route_id, route_kind, route_target, enabled,
                metadata_json, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
            ON CONFLICT(project_id, route_id) DO UPDATE SET
                route_kind = excluded.route_kind,
                route_target = excluded.route_target,
                enabled = excluded.enabled,
                metadata_json = excluded.metadata_json,
                updated_at = excluded.updated_at
            "#,
            params![
                project_id,
                route_id,
                route_kind,
                route_target,
                if disabled { 0 } else { 1 },
                metadata_json,
                now
            ],
        )
        .map_err(|error| sqlite_notification_error("route_upsert", error))?;
    let routes = load_notification_routes(&globals, &project_id)?;
    Ok(response(
        &globals,
        format!("Saved notification route `{route_id}` for project `{project_id}`."),
        json!({
            "kind": "notificationRouteUpsert",
            "projectId": project_id,
            "routeId": route_id,
            "routes": routes
        }),
    ))
}

fn command_notification_remove_route(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero notification remove-route ROUTE_ID [--project-id ID]",
            json!({ "command": "notification remove-route" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let route_id = take_option(&mut args, "--route-id")?
        .or_else(|| take_optional_positional(&mut args))
        .ok_or_else(|| CliError::usage("Missing route id."))?;
    reject_notification_unknown_options(&args)?;
    let connection = project_cli::project_connection(&globals, &project_id)?;
    ensure_notification_routes_table(&connection)?;
    let removed = connection
        .execute(
            "DELETE FROM notification_routes WHERE project_id = ?1 AND route_id = ?2",
            params![project_id, route_id],
        )
        .map_err(|error| sqlite_notification_error("route_remove", error))?;
    if removed == 0 {
        return Err(CliError::user_fixable(
            "xero_cli_notification_route_unknown",
            format!("Notification route `{route_id}` was not found."),
        ));
    }
    let routes = load_notification_routes(&globals, &project_id)?;
    Ok(response(
        &globals,
        format!("Removed notification route `{route_id}` from project `{project_id}`."),
        json!({
            "kind": "notificationRouteRemove",
            "projectId": project_id,
            "routeId": route_id,
            "routes": routes
        }),
    ))
}

fn command_notification_dispatches(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero notification dispatches [--project-id ID] [--action-id ACTION_ID]",
            json!({ "command": "notification dispatches" }),
        ));
    }
    let action_id = take_option(&mut args, "--action-id")?;
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    reject_notification_unknown_options(&args)?;
    let dispatches = load_notification_dispatches(&globals, &project_id, action_id.as_deref())?;
    let text = if dispatches.is_empty() {
        "No notification dispatches recorded.".into()
    } else {
        dispatches
            .iter()
            .map(|dispatch| {
                format!(
                    "#{:<4} {:<10} {:<18} attempts={} {}",
                    dispatch.id,
                    dispatch.status,
                    dispatch.route_id,
                    dispatch.attempt_count,
                    dispatch.action_id
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "notificationDispatches",
            "projectId": project_id,
            "actionId": action_id,
            "dispatches": dispatches
        }),
    ))
}

fn command_notification_replies(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero notification replies [--project-id ID] [--action-id ACTION_ID]",
            json!({ "command": "notification replies" }),
        ));
    }
    let action_id = take_option(&mut args, "--action-id")?;
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    reject_notification_unknown_options(&args)?;
    let replies = load_notification_replies(&globals, &project_id, action_id.as_deref())?;
    let text = if replies.is_empty() {
        "No notification replies recorded.".into()
    } else {
        replies
            .iter()
            .map(|reply| {
                format!(
                    "#{:<4} {:<10} {:<18} {}",
                    reply.id, reply.status, reply.route_id, reply.action_id
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "notificationReplies",
            "projectId": project_id,
            "actionId": action_id,
            "replies": replies
        }),
    ))
}

fn load_notification_routes(
    globals: &GlobalOptions,
    project_id: &str,
) -> Result<Vec<NotificationRouteRow>, CliError> {
    let connection = project_cli::project_connection(globals, project_id)?;
    if !table_exists(&connection, "notification_routes")? {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare(
            r#"
            SELECT project_id, route_id, route_kind, route_target, enabled,
                   metadata_json, created_at, updated_at
            FROM notification_routes
            WHERE project_id = ?1
            ORDER BY enabled DESC, updated_at DESC, route_id ASC
            "#,
        )
        .map_err(|error| sqlite_notification_error("routes_prepare", error))?;
    let rows = statement
        .query_map(params![project_id], |row| {
            let metadata_json: Option<String> = row.get(5)?;
            Ok(NotificationRouteRow {
                project_id: row.get(0)?,
                route_id: row.get(1)?,
                route_kind: row.get(2)?,
                route_target: row.get(3)?,
                enabled: row.get::<_, i64>(4)? != 0,
                metadata: metadata_json
                    .as_deref()
                    .map(|value| parse_json_column("metadata_json", value))
                    .transpose()?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|error| sqlite_notification_error("routes_query", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlite_notification_error("routes_decode", error))
}

fn load_notification_dispatches(
    globals: &GlobalOptions,
    project_id: &str,
    action_id: Option<&str>,
) -> Result<Vec<NotificationDispatchRow>, CliError> {
    let connection = project_cli::project_connection(globals, project_id)?;
    if !table_exists(&connection, "notification_dispatches")? {
        return Ok(Vec::new());
    }
    let sql = if action_id.is_some() {
        r#"
            SELECT id, project_id, action_id, route_id, correlation_key, status,
                   attempt_count, last_attempt_at, delivered_at, claimed_at,
                   last_error_code, last_error_message, created_at, updated_at
            FROM notification_dispatches
            WHERE project_id = ?1 AND action_id = ?2
            ORDER BY created_at ASC, id ASC
            LIMIT 100
            "#
    } else {
        r#"
            SELECT id, project_id, action_id, route_id, correlation_key, status,
                   attempt_count, last_attempt_at, delivered_at, claimed_at,
                   last_error_code, last_error_message, created_at, updated_at
            FROM notification_dispatches
            WHERE project_id = ?1
            ORDER BY updated_at DESC, id DESC
            LIMIT 100
            "#
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| sqlite_notification_error("dispatches_prepare", error))?;
    if let Some(action_id) = action_id {
        let rows = statement
            .query_map(params![project_id, action_id], notification_dispatch_row)
            .map_err(|error| sqlite_notification_error("dispatches_query", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| sqlite_notification_error("dispatches_decode", error))
    } else {
        let rows = statement
            .query_map(params![project_id], notification_dispatch_row)
            .map_err(|error| sqlite_notification_error("dispatches_query", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| sqlite_notification_error("dispatches_decode", error))
    }
}

fn load_notification_replies(
    globals: &GlobalOptions,
    project_id: &str,
    action_id: Option<&str>,
) -> Result<Vec<NotificationReplyRow>, CliError> {
    let connection = project_cli::project_connection(globals, project_id)?;
    if !table_exists(&connection, "notification_reply_claims")? {
        return Ok(Vec::new());
    }
    let sql = if action_id.is_some() {
        r#"
            SELECT id, project_id, action_id, route_id, correlation_key,
                   responder_id, reply_text, status, rejection_code,
                   rejection_message, created_at
            FROM notification_reply_claims
            WHERE project_id = ?1 AND action_id = ?2
            ORDER BY created_at DESC, id DESC
            LIMIT 100
            "#
    } else {
        r#"
            SELECT id, project_id, action_id, route_id, correlation_key,
                   responder_id, reply_text, status, rejection_code,
                   rejection_message, created_at
            FROM notification_reply_claims
            WHERE project_id = ?1
            ORDER BY created_at DESC, id DESC
            LIMIT 100
            "#
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| sqlite_notification_error("replies_prepare", error))?;
    if let Some(action_id) = action_id {
        let rows = statement
            .query_map(params![project_id, action_id], notification_reply_row)
            .map_err(|error| sqlite_notification_error("replies_query", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| sqlite_notification_error("replies_decode", error))
    } else {
        let rows = statement
            .query_map(params![project_id], notification_reply_row)
            .map_err(|error| sqlite_notification_error("replies_query", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| sqlite_notification_error("replies_decode", error))
    }
}

fn notification_dispatch_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NotificationDispatchRow> {
    Ok(NotificationDispatchRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        action_id: row.get(2)?,
        route_id: row.get(3)?,
        correlation_key: row.get(4)?,
        status: row.get(5)?,
        attempt_count: row.get(6)?,
        last_attempt_at: row.get(7)?,
        delivered_at: row.get(8)?,
        claimed_at: row.get(9)?,
        last_error_code: row.get(10)?,
        last_error_message: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn notification_reply_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NotificationReplyRow> {
    Ok(NotificationReplyRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        action_id: row.get(2)?,
        route_id: row.get(3)?,
        correlation_key: row.get(4)?,
        responder_id: row.get(5)?,
        reply_text: row.get(6)?,
        status: row.get(7)?,
        rejection_code: row.get(8)?,
        rejection_message: row.get(9)?,
        created_at: row.get(10)?,
    })
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, CliError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            params![table],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| sqlite_notification_error("table_probe", error))
}

fn ensure_notification_routes_table(connection: &Connection) -> Result<(), CliError> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS notification_routes (
                project_id TEXT NOT NULL,
                route_id TEXT NOT NULL,
                route_kind TEXT NOT NULL,
                route_target TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                metadata_json TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (project_id, route_id)
            );
            "#,
        )
        .map_err(|error| sqlite_notification_error("route_schema", error))
}

fn parse_json_column(label: &str, value: &str) -> Result<JsonValue, rusqlite::Error> {
    serde_json::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Could not decode {label}: {error}"),
            )),
        )
    })
}

fn parse_json_payload_for_cli(label: &str, value: &str) -> Result<JsonValue, CliError> {
    serde_json::from_str(value).map_err(|error| {
        CliError::usage(format!(
            "Could not decode {label} JSON for notification route: {error}"
        ))
    })
}

fn validate_notification_route_input(
    route_id: &str,
    route_kind: &str,
    route_target: &str,
) -> Result<(), CliError> {
    if route_id.trim().is_empty() {
        return Err(CliError::usage("Notification route id cannot be empty."));
    }
    if route_kind.trim().is_empty() {
        return Err(CliError::usage("Notification route kind cannot be empty."));
    }
    if route_target.trim().is_empty() {
        return Err(CliError::usage(
            "Notification route target cannot be empty.",
        ));
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

fn reject_notification_unknown_options(args: &[String]) -> Result<(), CliError> {
    if let Some(option) = args.iter().find(|arg| arg.starts_with('-')) {
        return Err(CliError::usage(format!("Unknown option `{option}`.")));
    }
    if let Some(extra) = args.first() {
        return Err(CliError::usage(format!("Unexpected argument `{extra}`.")));
    }
    Ok(())
}

fn sqlite_notification_error(operation: &str, error: rusqlite::Error) -> CliError {
    CliError::system_fault(
        "xero_cli_notification_sql_failed",
        format!("Notification {operation} failed: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn notification_visibility_reads_routes_dispatches_and_replies() {
        let state_dir = temp_dir("notification-state");
        let repo = temp_dir("notification-repo");
        crate::run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "project",
            "import",
            "--path",
            repo.to_str().expect("repo"),
        ])
        .expect("project import");
        let project_id = selected_project_id(&state_dir);
        seed_notification_rows(&state_dir, &project_id);

        let routes = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "notification",
            "routes",
        ])
        .expect("routes");
        assert_eq!(routes.json["routes"][0]["routeId"], json!("route-1"));

        let dispatches = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "notification",
            "dispatches",
            "--action-id",
            "action-1",
        ])
        .expect("dispatches");
        assert_eq!(
            dispatches.json["dispatches"][0]["correlationKey"],
            json!("corr-1")
        );

        let replies = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "notification",
            "replies",
        ])
        .expect("replies");
        assert_eq!(replies.json["replies"][0]["status"], json!("accepted"));
    }

    #[test]
    fn notification_route_upsert_and_remove_mutate_project_route_table() {
        let state_dir = temp_dir("notification-route-state");
        let repo = temp_dir("notification-route-repo");
        crate::run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "project",
            "import",
            "--path",
            repo.to_str().expect("repo"),
        ])
        .expect("project import");

        let upserted = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "notification",
            "upsert-route",
            "route-cli",
            "--kind",
            "webhook",
            "--target",
            "https://example.test/hook",
            "--metadata",
            "{\"redacted\":true}",
        ])
        .expect("route upsert");
        assert_eq!(upserted.json["kind"], json!("notificationRouteUpsert"));
        assert!(upserted.json["routes"]
            .as_array()
            .expect("routes")
            .iter()
            .any(|route| route["routeId"] == json!("route-cli")));

        let removed = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "notification",
            "remove-route",
            "route-cli",
        ])
        .expect("route remove");
        assert_eq!(removed.json["kind"], json!("notificationRouteRemove"));
        assert!(!removed.json["routes"]
            .as_array()
            .expect("routes")
            .iter()
            .any(|route| route["routeId"] == json!("route-cli")));
    }

    fn seed_notification_rows(state_dir: &Path, project_id: &str) {
        let globals = crate::GlobalOptions {
            output_mode: crate::OutputMode::Json,
            ci: false,
            state_dir: state_dir.to_path_buf(),
            tui_adapter: None,
        };
        let connection = project_cli::project_connection(&globals, project_id).expect("connection");
        connection
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS notification_routes (
                    project_id TEXT NOT NULL,
                    route_id TEXT NOT NULL,
                    route_kind TEXT NOT NULL,
                    route_target TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    metadata_json TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY (project_id, route_id)
                );
                CREATE TABLE IF NOT EXISTS notification_dispatches (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    action_id TEXT NOT NULL,
                    route_id TEXT NOT NULL,
                    correlation_key TEXT NOT NULL,
                    status TEXT NOT NULL,
                    attempt_count INTEGER NOT NULL DEFAULT 0,
                    last_attempt_at TEXT,
                    delivered_at TEXT,
                    claimed_at TEXT,
                    last_error_code TEXT,
                    last_error_message TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS notification_reply_claims (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    action_id TEXT NOT NULL,
                    route_id TEXT NOT NULL,
                    correlation_key TEXT NOT NULL,
                    responder_id TEXT,
                    reply_text TEXT NOT NULL,
                    status TEXT NOT NULL,
                    rejection_code TEXT,
                    rejection_message TEXT,
                    created_at TEXT NOT NULL
                );
                "#,
            )
            .expect("create notification tables");
        connection
            .execute(
                r#"
                INSERT INTO notification_routes (
                    project_id, route_id, route_kind, route_target, enabled,
                    metadata_json, created_at, updated_at
                ) VALUES (?1, 'route-1', 'telegram', 'telegram:chat-1', 1,
                    '{"redacted":true}', '2026-05-15T00:00:00Z', '2026-05-15T00:00:00Z')
                "#,
                params![project_id],
            )
            .expect("insert route");
        connection
            .execute(
                r#"
                INSERT INTO notification_dispatches (
                    project_id, action_id, route_id, correlation_key, status,
                    attempt_count, created_at, updated_at
                ) VALUES (?1, 'action-1', 'route-1', 'corr-1', 'sent', 1,
                    '2026-05-15T00:00:01Z', '2026-05-15T00:00:02Z')
                "#,
                params![project_id],
            )
            .expect("insert dispatch");
        connection
            .execute(
                r#"
                INSERT INTO notification_reply_claims (
                    project_id, action_id, route_id, correlation_key,
                    responder_id, reply_text, status, created_at
                ) VALUES (?1, 'action-1', 'route-1', 'corr-1',
                    'operator-1', 'approve', 'accepted', '2026-05-15T00:00:03Z')
                "#,
                params![project_id],
            )
            .expect("insert reply");
    }

    fn selected_project_id(state_dir: &Path) -> String {
        let output = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "project",
            "list",
        ])
        .expect("project list");
        output.json["projects"][0]["projectId"]
            .as_str()
            .expect("project id")
            .to_owned()
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{nonce}"));
        fs::create_dir_all(&path).expect("create temp dir");
        Command::new("git")
            .arg("init")
            .current_dir(&path)
            .output()
            .expect("git init");
        path
    }
}
