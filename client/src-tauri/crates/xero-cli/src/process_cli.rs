use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::json;

use super::{
    generate_id, now_timestamp, project_cli, response, take_bool_flag, take_help, take_option,
    validate_required_cli, CliError, CliResponse, GlobalOptions,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessSessionRow {
    project_id: String,
    session_id: String,
    target_id: String,
    target_name: String,
    command: String,
    cwd: String,
    pid: Option<u32>,
    status: String,
    exit_code: Option<i32>,
    log_path: String,
    started_at: String,
    stopped_at: Option<String>,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct RefreshedProcessStatus {
    status: String,
    exit_code: Option<i32>,
    stopped_at: Option<String>,
}

pub(crate) fn dispatch_process(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("targets") | Some("target-list") => command_process_targets(globals, args[1..].to_vec()),
        Some("add-target") | Some("target-add") => {
            command_process_add_target(globals, args[1..].to_vec())
        }
        Some("remove-target") | Some("target-remove") => {
            command_process_remove_target(globals, args[1..].to_vec())
        }
        Some("start") | Some("run") => command_process_start(globals, args[1..].to_vec()),
        Some("list") | Some("sessions") => command_process_list(globals, args[1..].to_vec()),
        Some("status") => command_process_status(globals, args[1..].to_vec()),
        Some("tail") => command_process_tail(globals, args[1..].to_vec()),
        Some("stop") => command_process_stop(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero process targets|add-target|remove-target|start|list|status|tail|stop [--project-id ID]\nUses the existing project start-target app-data contract and stores terminal process sessions in OS app-data project state.",
            json!({ "command": "process" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown process command `{other}`. Use targets, add-target, remove-target, start, list, status, tail, or stop."
        ))),
    }
}

fn command_process_targets(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero process targets [--project-id ID]",
            json!({ "command": "process targets" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    reject_process_unknown_options(&args)?;
    let targets = project_cli::project_start_targets(&globals, &project_id)?;
    let text = if targets.is_empty() {
        format!("No start targets configured for project `{project_id}`.")
    } else {
        targets
            .iter()
            .map(|target| format!("{:<20} {:<20} {}", target.id, target.name, target.command))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "processTargets",
            "projectId": project_id,
            "targets": targets,
            "storageScope": "os_app_data"
        }),
    ))
}

fn command_process_add_target(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero process add-target --name NAME --command COMMAND [--id ID] [--project-id ID]",
            json!({ "command": "process add-target" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let name = take_option(&mut args, "--name")?
        .or_else(|| take_optional_positional(&mut args))
        .ok_or_else(|| CliError::usage("Missing start-target name."))?;
    let command = take_option(&mut args, "--command")?
        .ok_or_else(|| CliError::usage("Missing `--command`."))?;
    let id = take_option(&mut args, "--id")?.unwrap_or_else(|| target_id_from_name(&name));
    reject_process_unknown_options(&args)?;
    validate_target(&id, &name, &command)?;

    let mut targets = project_cli::project_start_targets(&globals, &project_id)?;
    let target = project_cli::StartTargetRecord { id, name, command };
    if let Some(existing) = targets.iter_mut().find(|existing| existing.id == target.id) {
        *existing = target.clone();
    } else {
        targets.push(target.clone());
    }
    project_cli::update_project_start_targets(&globals, &project_id, &targets)?;
    Ok(response(
        &globals,
        format!(
            "Saved start target `{}` for project `{}`.",
            target.id, project_id
        ),
        json!({
            "kind": "processTargetUpsert",
            "projectId": project_id,
            "target": target,
            "targets": targets
        }),
    ))
}

fn command_process_remove_target(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero process remove-target TARGET_ID [--project-id ID]",
            json!({ "command": "process remove-target" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let target_id = take_option(&mut args, "--target")?
        .or_else(|| take_optional_positional(&mut args))
        .ok_or_else(|| CliError::usage("Missing target id."))?;
    reject_process_unknown_options(&args)?;
    let mut targets = project_cli::project_start_targets(&globals, &project_id)?;
    let before = targets.len();
    targets.retain(|target| target.id != target_id && target.name != target_id);
    if targets.len() == before {
        return Err(CliError::user_fixable(
            "xero_cli_process_target_unknown",
            format!("Start target `{target_id}` was not found."),
        ));
    }
    project_cli::update_project_start_targets(&globals, &project_id, &targets)?;
    Ok(response(
        &globals,
        format!("Removed start target `{target_id}` from project `{project_id}`."),
        json!({
            "kind": "processTargetRemove",
            "projectId": project_id,
            "targetId": target_id,
            "targets": targets
        }),
    ))
}

fn command_process_start(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero process start TARGET_ID [--project-id ID] | xero process start --all [--project-id ID]",
            json!({ "command": "process start" }),
        ));
    }
    let all = take_bool_flag(&mut args, "--all");
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let target_selector =
        take_option(&mut args, "--target")?.or_else(|| take_optional_positional(&mut args));
    reject_process_unknown_options(&args)?;
    if !all && target_selector.is_none() {
        return Err(CliError::usage(
            "Missing target id. Use `xero process start TARGET_ID` or `--all`.",
        ));
    }

    let root = project_cli::project_root_path(&globals, &project_id)?;
    let targets = project_cli::project_start_targets(&globals, &project_id)?;
    let selected = if all {
        targets
    } else {
        let selector = target_selector.expect("checked target selector");
        vec![resolve_target(&targets, &selector)?]
    };
    if selected.is_empty() {
        return Err(CliError::user_fixable(
            "xero_cli_process_no_targets",
            format!("Project `{project_id}` has no start targets."),
        ));
    }

    let connection = process_connection(&globals, &project_id)?;
    let mut started = Vec::new();
    for target in selected {
        started.push(spawn_target(
            &globals,
            &connection,
            &project_id,
            &root,
            &target,
        )?);
    }
    let text = started
        .iter()
        .map(|session| {
            format!(
                "{} target={} pid={} log={}",
                session.session_id,
                session.target_id,
                session
                    .pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                session.log_path
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "processStart",
            "projectId": project_id,
            "sessions": started
        }),
    ))
}

fn command_process_list(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero process list [--project-id ID]",
            json!({ "command": "process list" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    reject_process_unknown_options(&args)?;
    let connection = process_connection(&globals, &project_id)?;
    let sessions = list_process_sessions(&connection, &project_id)?;
    let text = if sessions.is_empty() {
        format!("No process sessions recorded for project `{project_id}`.")
    } else {
        sessions
            .iter()
            .map(|session| {
                format!(
                    "{:<28} {:<10} {:<16} pid={}",
                    session.session_id,
                    session.status,
                    session.target_id,
                    session
                        .pid
                        .map(|pid| pid.to_string())
                        .unwrap_or_else(|| "-".into())
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({ "kind": "processList", "projectId": project_id, "sessions": sessions }),
    ))
}

fn command_process_status(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero process status SESSION_ID [--project-id ID]",
            json!({ "command": "process status" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let session_id = take_session_selector(&mut args)?;
    reject_process_unknown_options(&args)?;
    let connection = process_connection(&globals, &project_id)?;
    let session = process_session_by_id(&connection, &project_id, &session_id)?;
    Ok(response(
        &globals,
        format!(
            "{} status={} target={} pid={}",
            session.session_id,
            session.status,
            session.target_id,
            session
                .pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "-".into())
        ),
        json!({ "kind": "processStatus", "projectId": project_id, "session": session }),
    ))
}

fn command_process_tail(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero process tail SESSION_ID [--bytes N] [--project-id ID]",
            json!({ "command": "process tail" }),
        ));
    }
    let bytes = take_option(&mut args, "--bytes")?
        .map(|value| parse_tail_bytes(&value))
        .transpose()?
        .unwrap_or(16 * 1024);
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let session_id = take_session_selector(&mut args)?;
    reject_process_unknown_options(&args)?;
    let connection = process_connection(&globals, &project_id)?;
    let session = process_session_by_id(&connection, &project_id, &session_id)?;
    let output = tail_file(Path::new(&session.log_path), bytes)?;
    Ok(response(
        &globals,
        output.clone(),
        json!({
            "kind": "processTail",
            "projectId": project_id,
            "sessionId": session.session_id,
            "bytes": bytes,
            "output": output,
            "session": session
        }),
    ))
}

fn command_process_stop(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero process stop SESSION_ID [--project-id ID]",
            json!({ "command": "process stop" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let session_id = take_session_selector(&mut args)?;
    reject_process_unknown_options(&args)?;
    let connection = process_connection(&globals, &project_id)?;
    let mut session = process_session_by_id(&connection, &project_id, &session_id)?;
    if session.status == "running" {
        if let Some(pid) = session.pid {
            stop_pid(pid)?;
        }
        let stopped_at = now_timestamp();
        update_session_status(
            &connection,
            &project_id,
            &session_id,
            "stopped",
            None,
            Some(&stopped_at),
        )?;
        session = process_session_by_id(&connection, &project_id, &session_id)?;
    }
    Ok(response(
        &globals,
        format!("Stopped process session `{}`.", session.session_id),
        json!({ "kind": "processStop", "projectId": project_id, "session": session }),
    ))
}

fn process_connection(globals: &GlobalOptions, project_id: &str) -> Result<Connection, CliError> {
    let connection = project_cli::project_connection(globals, project_id)?;
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS project_process_sessions (
                project_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                target_name TEXT NOT NULL,
                command TEXT NOT NULL,
                cwd TEXT NOT NULL,
                pid INTEGER,
                status TEXT NOT NULL,
                exit_code INTEGER,
                log_path TEXT NOT NULL,
                started_at TEXT NOT NULL,
                stopped_at TEXT,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (project_id, session_id)
            );
            CREATE INDEX IF NOT EXISTS idx_project_process_sessions_project_status
                ON project_process_sessions(project_id, status, updated_at DESC);
            "#,
        )
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_process_schema_failed",
                format!("Could not prepare process session state: {error}"),
            )
        })?;
    Ok(connection)
}

fn spawn_target(
    globals: &GlobalOptions,
    connection: &Connection,
    project_id: &str,
    root: &Path,
    target: &project_cli::StartTargetRecord,
) -> Result<ProcessSessionRow, CliError> {
    let session_id = generate_id("process");
    let log_path = process_log_path(globals, project_id, &session_id);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::system_fault(
                "xero_cli_process_log_prepare_failed",
                format!(
                    "Could not create process log directory `{}`: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_process_log_open_failed",
                format!(
                    "Could not open process log `{}`: {error}",
                    log_path.display()
                ),
            )
        })?;
    let stderr = log.try_clone().map_err(|error| {
        CliError::system_fault(
            "xero_cli_process_log_clone_failed",
            format!(
                "Could not prepare process stderr log `{}`: {error}",
                log_path.display()
            ),
        )
    })?;
    let script = format!(
        "printf '[xero-process-start:{}]\\n'\n{}\nstatus=$?\nprintf '\\n[xero-process-exit:%s]\\n' \"$status\"\nexit \"$status\"",
        session_id, target.command
    );
    let child = Command::new("sh")
        .arg("-lc")
        .arg(script)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_process_spawn_failed",
                format!(
                    "Could not start target `{}` in `{}`: {error}",
                    target.id,
                    root.display()
                ),
            )
        })?;
    let started_at = now_timestamp();
    let pid = child.id();
    drop(child);
    connection
        .execute(
            r#"
            INSERT INTO project_process_sessions (
                project_id, session_id, target_id, target_name, command, cwd, pid,
                status, exit_code, log_path, started_at, stopped_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'running', NULL, ?8, ?9, NULL, ?9)
            "#,
            params![
                project_id,
                session_id,
                target.id,
                target.name,
                target.command,
                root.to_string_lossy(),
                i64::from(pid),
                log_path.to_string_lossy(),
                started_at,
            ],
        )
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_process_session_write_failed",
                format!("Could not record process session: {error}"),
            )
        })?;
    process_session_by_id(connection, project_id, &session_id)
}

fn list_process_sessions(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<ProcessSessionRow>, CliError> {
    refresh_process_sessions(connection, project_id)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT project_id, session_id, target_id, target_name, command, cwd, pid,
                   status, exit_code, log_path, started_at, stopped_at, updated_at
            FROM project_process_sessions
            WHERE project_id = ?1
            ORDER BY updated_at DESC, started_at DESC
            "#,
        )
        .map_err(|error| sqlite_process_error("xero_cli_process_list_prepare_failed", error))?;
    let rows = statement
        .query_map(params![project_id], read_process_session_row)
        .map_err(|error| sqlite_process_error("xero_cli_process_list_failed", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlite_process_error("xero_cli_process_list_decode_failed", error))
}

fn process_session_by_id(
    connection: &Connection,
    project_id: &str,
    session_id: &str,
) -> Result<ProcessSessionRow, CliError> {
    refresh_process_sessions(connection, project_id)?;
    connection
        .query_row(
            r#"
            SELECT project_id, session_id, target_id, target_name, command, cwd, pid,
                   status, exit_code, log_path, started_at, stopped_at, updated_at
            FROM project_process_sessions
            WHERE project_id = ?1 AND session_id = ?2
            "#,
            params![project_id, session_id],
            read_process_session_row,
        )
        .optional()
        .map_err(|error| sqlite_process_error("xero_cli_process_lookup_failed", error))?
        .ok_or_else(|| {
            CliError::user_fixable(
                "xero_cli_process_session_unknown",
                format!("Process session `{session_id}` was not found."),
            )
        })
}

fn refresh_process_sessions(connection: &Connection, project_id: &str) -> Result<(), CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT session_id, pid, status, exit_code, log_path, stopped_at
            FROM project_process_sessions
            WHERE project_id = ?1
            "#,
        )
        .map_err(|error| sqlite_process_error("xero_cli_process_refresh_prepare_failed", error))?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<i32>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|error| sqlite_process_error("xero_cli_process_refresh_failed", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlite_process_error("xero_cli_process_refresh_decode_failed", error))?;
    drop(statement);

    for (session_id, pid, status, exit_code, log_path, stopped_at) in rows {
        let refreshed = refresh_status_from_process(
            pid.and_then(|pid| u32::try_from(pid).ok()),
            &status,
            exit_code,
            &log_path,
            stopped_at,
        );
        if refreshed.status != status || refreshed.exit_code != exit_code {
            update_session_status(
                connection,
                project_id,
                &session_id,
                &refreshed.status,
                refreshed.exit_code,
                refreshed.stopped_at.as_deref(),
            )?;
        }
    }
    Ok(())
}

fn refresh_status_from_process(
    pid: Option<u32>,
    current_status: &str,
    current_exit_code: Option<i32>,
    log_path: &str,
    current_stopped_at: Option<String>,
) -> RefreshedProcessStatus {
    if matches!(current_status, "stopped" | "exited" | "exited_unknown") {
        return RefreshedProcessStatus {
            status: current_status.to_owned(),
            exit_code: current_exit_code,
            stopped_at: current_stopped_at,
        };
    }
    if let Some(exit_code) = process_exit_code_from_log(Path::new(log_path)) {
        return RefreshedProcessStatus {
            status: "exited".into(),
            exit_code: Some(exit_code),
            stopped_at: current_stopped_at.or_else(|| Some(now_timestamp())),
        };
    }
    if pid.is_some_and(pid_is_running) {
        return RefreshedProcessStatus {
            status: "running".into(),
            exit_code: None,
            stopped_at: None,
        };
    }
    RefreshedProcessStatus {
        status: "exited_unknown".into(),
        exit_code: None,
        stopped_at: current_stopped_at.or_else(|| Some(now_timestamp())),
    }
}

fn update_session_status(
    connection: &Connection,
    project_id: &str,
    session_id: &str,
    status: &str,
    exit_code: Option<i32>,
    stopped_at: Option<&str>,
) -> Result<(), CliError> {
    connection
        .execute(
            r#"
            UPDATE project_process_sessions
            SET status = ?3, exit_code = ?4, stopped_at = ?5, updated_at = ?6
            WHERE project_id = ?1 AND session_id = ?2
            "#,
            params![
                project_id,
                session_id,
                status,
                exit_code,
                stopped_at,
                now_timestamp()
            ],
        )
        .map_err(|error| sqlite_process_error("xero_cli_process_status_write_failed", error))?;
    Ok(())
}

fn read_process_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProcessSessionRow> {
    Ok(ProcessSessionRow {
        project_id: row.get(0)?,
        session_id: row.get(1)?,
        target_id: row.get(2)?,
        target_name: row.get(3)?,
        command: row.get(4)?,
        cwd: row.get(5)?,
        pid: row
            .get::<_, Option<i64>>(6)?
            .and_then(|pid| u32::try_from(pid).ok()),
        status: row.get(7)?,
        exit_code: row.get(8)?,
        log_path: row.get(9)?,
        started_at: row.get(10)?,
        stopped_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn process_log_path(globals: &GlobalOptions, project_id: &str, session_id: &str) -> PathBuf {
    globals
        .state_dir
        .join("projects")
        .join(project_id)
        .join("processes")
        .join(format!("{session_id}.log"))
}

fn process_exit_code_from_log(path: &Path) -> Option<i32> {
    let content = fs::read_to_string(path).ok()?;
    let marker = "[xero-process-exit:";
    let start = content.rfind(marker)? + marker.len();
    let rest = content.get(start..)?;
    let end = rest.find(']')?;
    rest.get(..end)?.trim().parse::<i32>().ok()
}

fn tail_file(path: &Path, max_bytes: usize) -> Result<String, CliError> {
    let mut file = File::open(path).map_err(|error| {
        CliError::user_fixable(
            "xero_cli_process_log_missing",
            format!("Could not open process log `{}`: {error}", path.display()),
        )
    })?;
    let len = file
        .metadata()
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_process_log_metadata_failed",
                format!(
                    "Could not inspect process log `{}`: {error}",
                    path.display()
                ),
            )
        })?
        .len();
    let read_bytes = max_bytes.min(len as usize);
    file.seek(SeekFrom::Start(len.saturating_sub(read_bytes as u64)))
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_process_log_seek_failed",
                format!("Could not seek process log `{}`: {error}", path.display()),
            )
        })?;
    let mut buffer = Vec::with_capacity(read_bytes);
    file.read_to_end(&mut buffer).map_err(|error| {
        CliError::system_fault(
            "xero_cli_process_log_read_failed",
            format!("Could not read process log `{}`: {error}", path.display()),
        )
    })?;
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

fn resolve_target(
    targets: &[project_cli::StartTargetRecord],
    selector: &str,
) -> Result<project_cli::StartTargetRecord, CliError> {
    targets
        .iter()
        .find(|target| target.id == selector || target.name == selector)
        .cloned()
        .ok_or_else(|| {
            CliError::user_fixable(
                "xero_cli_process_target_unknown",
                format!("Start target `{selector}` was not found."),
            )
        })
}

fn validate_target(id: &str, name: &str, command: &str) -> Result<(), CliError> {
    validate_required_cli(id, "id")?;
    validate_required_cli(name, "name")?;
    validate_required_cli(command, "command")?;
    if id.contains('/') || id.contains('\\') || id == "." || id == ".." {
        return Err(CliError::usage(
            "Start target id must be a simple file-name-safe token.",
        ));
    }
    Ok(())
}

fn target_id_from_name(name: &str) -> String {
    let mut id = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while id.contains("--") {
        id = id.replace("--", "-");
    }
    let id = id.trim_matches('-').to_owned();
    if id.is_empty() {
        "target".into()
    } else {
        id
    }
}

fn take_session_selector(args: &mut Vec<String>) -> Result<String, CliError> {
    take_option(args, "--session-id")?
        .or_else(|| take_optional_positional(args))
        .ok_or_else(|| CliError::usage("Missing process session id."))
}

fn take_optional_positional(args: &mut Vec<String>) -> Option<String> {
    if args.first().is_some_and(|arg| !arg.starts_with('-')) {
        Some(args.remove(0))
    } else {
        None
    }
}

fn parse_tail_bytes(value: &str) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .ok()
        .filter(|value| (1..=1_048_576).contains(value))
        .ok_or_else(|| CliError::usage("`--bytes` must be between 1 and 1048576."))
}

#[cfg(unix)]
fn pid_is_running(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn pid_is_running(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
fn stop_pid(pid: u32) -> Result<(), CliError> {
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_process_stop_failed",
                format!("Could not signal process `{pid}`: {error}"),
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::user_fixable(
            "xero_cli_process_stop_failed",
            format!("Could not stop process `{pid}`."),
        ))
    }
}

#[cfg(not(unix))]
fn stop_pid(pid: u32) -> Result<(), CliError> {
    Err(CliError::user_fixable(
        "xero_cli_process_stop_unsupported",
        format!("Stopping process `{pid}` is not supported on this platform yet."),
    ))
}

fn reject_process_unknown_options(args: &[String]) -> Result<(), CliError> {
    if let Some(option) = args.iter().find(|arg| arg.starts_with('-')) {
        return Err(CliError::usage(format!("Unknown option `{option}`.")));
    }
    if let Some(extra) = args.first() {
        return Err(CliError::usage(format!("Unexpected argument `{extra}`.")));
    }
    Ok(())
}

fn sqlite_process_error(code: &'static str, error: rusqlite::Error) -> CliError {
    CliError::system_fault(code, format!("Process session storage failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn process_targets_are_stored_in_project_start_target_contract() {
        let state_dir = temp_dir("process-target-state");
        let repo = temp_git_dir("process-target-repo");
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

        let added = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "process",
            "add-target",
            "--name",
            "Dev Server",
            "--command",
            "printf hello",
        ])
        .expect("add target");
        assert_eq!(added.json["kind"], json!("processTargetUpsert"));
        assert_eq!(added.json["target"]["id"], json!("dev-server"));

        let listed = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "process",
            "targets",
        ])
        .expect("targets");
        assert!(listed.json["targets"]
            .as_array()
            .expect("targets array")
            .iter()
            .any(|target| target["id"] == json!("dev-server")));
    }

    #[test]
    fn process_start_tail_and_status_use_project_state_sessions() {
        let state_dir = temp_dir("process-session-state");
        let repo = temp_git_dir("process-session-repo");
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
        crate::run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "process",
            "add-target",
            "--name",
            "Echo",
            "--command",
            "printf process-ok",
        ])
        .expect("add target");

        let started = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "process",
            "start",
            "echo",
        ])
        .expect("start target");
        let session_id = started.json["sessions"][0]["sessionId"]
            .as_str()
            .expect("session id")
            .to_owned();
        thread::sleep(Duration::from_millis(250));

        let tail = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "process",
            "tail",
            &session_id,
        ])
        .expect("tail");
        assert!(tail.json["output"]
            .as_str()
            .expect("output")
            .contains("process-ok"));

        let status = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "process",
            "status",
            &session_id,
        ])
        .expect("status");
        assert_eq!(status.json["session"]["status"], json!("exited"));
        assert_eq!(status.json["session"]["exitCode"], json!(0));
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

    fn temp_git_dir(prefix: &str) -> PathBuf {
        let path = temp_dir(prefix);
        Command::new("git")
            .arg("init")
            .current_dir(&path)
            .output()
            .expect("git init");
        path
    }
}
