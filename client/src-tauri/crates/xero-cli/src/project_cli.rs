use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use super::{
    canonicalize_existing_path, cli_app_data_root, generate_id, global_database_path,
    now_timestamp, read_json_file, response, stable_project_id_for_repo_root, take_bool_flag,
    take_help, take_option, validate_required_cli, workspace_project_database_path,
    write_json_file, CliError, CliResponse, GlobalOptions, BENCHMARK_PROJECT_SCHEMA,
};

const TUI_SETTINGS_FILE: &str = "tui-settings.json";
const DEFAULT_AGENT_SESSION_TITLE: &str = "New Chat";
const MAX_SESSION_TITLE_CHARS: usize = 64;
pub(crate) const GLOBAL_COMPUTER_USE_PROJECT_ID: &str = "global-computer-use";
pub(crate) const GLOBAL_COMPUTER_USE_PROJECT_NAME: &str = "Computer Use";
pub(crate) const GLOBAL_COMPUTER_USE_AGENT_SESSION_ID: &str = "agent-session-global-computer-use";

const GLOBAL_COMPUTER_USE_REPOSITORY_ID: &str = "global-computer-use-repository";
const GLOBAL_COMPUTER_USE_DIR: &str = "computer-use";
const GLOBAL_COMPUTER_USE_STATE_DB: &str = "state.db";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TuiSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRecord {
    project_id: String,
    repository_id: String,
    name: String,
    root_path: String,
    database_path: String,
    branch: Option<String>,
    head_sha: Option<String>,
    start_targets: JsonValue,
    selected: bool,
    root_exists: bool,
    state_exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StartTargetRecord {
    pub id: String,
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionRecord {
    project_id: String,
    agent_session_id: String,
    title: String,
    summary: String,
    session_kind: String,
    status: String,
    selected: bool,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
    last_run_id: Option<String>,
    last_provider_id: Option<String>,
}

pub(crate) fn dispatch_project(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("list") => command_project_list(globals, args[1..].to_vec()),
        Some("import") => command_project_import(globals, args[1..].to_vec()),
        Some("create") => command_project_create(globals, args[1..].to_vec()),
        Some("remove") => command_project_remove(globals, args[1..].to_vec()),
        Some("snapshot") => command_project_snapshot(globals, args[1..].to_vec()),
        Some("select") => command_project_select(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero project list|import|create|remove|snapshot|select",
            json!({ "command": "project" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown project command `{other}`. Use list, import, create, remove, snapshot, or select."
        ))),
    }
}

pub(crate) fn dispatch_session(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("list") => command_session_list(globals, args[1..].to_vec()),
        Some("create") => command_session_create(globals, args[1..].to_vec()),
        Some("rename") | Some("update") => command_session_rename(globals, args[1..].to_vec()),
        Some("auto-name") => command_session_auto_name(globals, args[1..].to_vec()),
        Some("archive") => command_session_status(globals, args[1..].to_vec(), "archived"),
        Some("restore") => command_session_status(globals, args[1..].to_vec(), "active"),
        Some("delete") => command_session_delete(globals, args[1..].to_vec()),
        Some("resume") | Some("select") => command_session_select(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero session list|create|rename|auto-name|archive|restore|delete|resume|select --project-id ID",
            json!({ "command": "session" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown session command `{other}`. Use list, create, rename, auto-name, archive, restore, delete, resume, or select."
        ))),
    }
}

pub(crate) fn dispatch_git(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("status") => command_git_status(globals, args[1..].to_vec()),
        Some("diff") => command_git_diff(globals, args[1..].to_vec()),
        Some("stage") => command_git_passthrough(globals, args[1..].to_vec(), "add", false),
        Some("unstage") => {
            command_git_passthrough(globals, args[1..].to_vec(), "restore-staged", false)
        }
        Some("discard") => {
            command_git_passthrough(globals, args[1..].to_vec(), "restore-worktree", true)
        }
        Some("commit") => command_git_commit(globals, args[1..].to_vec()),
        Some("fetch") => command_git_simple(globals, args[1..].to_vec(), &["fetch"]),
        Some("pull") => command_git_simple(globals, args[1..].to_vec(), &["pull", "--ff-only"]),
        Some("push") => command_git_simple(globals, args[1..].to_vec(), &["push"]),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero git status|diff|stage|unstage|discard|commit|fetch|pull|push [--project-id ID | --repo PATH]",
            json!({ "command": "git" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown git command `{other}`. Use status, diff, stage, unstage, discard, commit, fetch, pull, or push."
        ))),
    }
}

fn command_project_list(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero project list",
            json!({ "command": "project list" }),
        ));
    }
    reject_project_unknown_options(&args)?;
    let projects = list_projects(&globals)?;
    let text = if projects.is_empty() {
        "No Xero projects are registered in app-data.".into()
    } else {
        projects
            .iter()
            .map(|project| {
                format!(
                    "{} {:<22} {}",
                    if project.selected { "*" } else { " " },
                    project.project_id,
                    project.root_path
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({ "kind": "projectList", "appDataRoot": cli_app_data_root(&globals), "projects": projects }),
    ))
}

fn command_project_import(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero project import [--path PATH]",
            json!({ "command": "project import" }),
        ));
    }
    let path = take_option(&mut args, "--path")?.unwrap_or_else(|| ".".into());
    reject_project_unknown_options(&args)?;
    let project = import_existing_project(&globals, &path, ProjectKind::Brownfield)?;
    let mut settings = load_tui_settings(&globals)?;
    settings.selected_project_id = Some(project.project_id.clone());
    save_tui_settings(&globals, &settings)?;
    Ok(response(
        &globals,
        format!(
            "Imported `{}` as project `{}`.",
            project.root_path, project.project_id
        ),
        json!({ "kind": "projectImport", "project": project }),
    ))
}

fn command_project_create(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero project create --parent PATH --name NAME",
            json!({ "command": "project create" }),
        ));
    }
    let parent = take_option(&mut args, "--parent")?.unwrap_or_else(|| ".".into());
    let name = take_option(&mut args, "--name")?
        .or_else(|| (!args.is_empty()).then(|| args.remove(0)))
        .ok_or_else(|| CliError::usage("Missing project name."))?;
    reject_project_unknown_options(&args)?;
    validate_project_name(&name)?;
    let parent = canonicalize_existing_path(&parent)?;
    if !parent.is_dir() {
        return Err(CliError::user_fixable(
            "xero_cli_project_parent_not_directory",
            format!("Project parent `{}` is not a directory.", parent.display()),
        ));
    }
    let project_path = parent.join(name);
    if project_path.exists() {
        return Err(CliError::user_fixable(
            "xero_cli_project_create_exists",
            format!("Project path `{}` already exists.", project_path.display()),
        ));
    }
    fs::create_dir(&project_path).map_err(|error| {
        CliError::system_fault(
            "xero_cli_project_create_failed",
            format!("Could not create `{}`: {error}", project_path.display()),
        )
    })?;
    run_command_checked(&project_path, "git", &["init"])?;
    let project = import_existing_project(
        &globals,
        &project_path.to_string_lossy(),
        ProjectKind::Greenfield,
    )?;
    let mut settings = load_tui_settings(&globals)?;
    settings.selected_project_id = Some(project.project_id.clone());
    save_tui_settings(&globals, &settings)?;
    Ok(response(
        &globals,
        format!(
            "Created `{}` as project `{}`.",
            project.root_path, project.project_id
        ),
        json!({ "kind": "projectCreate", "project": project }),
    ))
}

fn command_project_remove(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero project remove PROJECT_ID [--delete-state] [--yes]",
            json!({ "command": "project remove" }),
        ));
    }
    let delete_state = take_bool_flag(&mut args, "--delete-state");
    let yes = take_bool_flag(&mut args, "--yes");
    let project_id = take_project_arg(&mut args, "remove")?;
    reject_project_unknown_options(&args)?;
    if delete_state && !yes {
        return Err(CliError::usage(
            "Project state deletion requires `--yes --delete-state`.",
        ));
    }
    let project = project_by_id(&globals, &project_id)?;
    let connection = open_global_registry(&globals)?;
    connection
        .execute("DELETE FROM projects WHERE id = ?1", params![project_id])
        .map_err(|error| sqlite_error("xero_cli_project_remove_failed", error))?;
    if delete_state {
        let state_path = PathBuf::from(&project.database_path);
        if let Some(parent) = state_path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }
    let mut settings = load_tui_settings(&globals)?;
    if settings.selected_project_id.as_deref() == Some(&project.project_id) {
        settings.selected_project_id = None;
        settings.selected_session_id = None;
        save_tui_settings(&globals, &settings)?;
    }
    Ok(response(
        &globals,
        format!(
            "Removed project `{}` from the app-data registry.",
            project_id
        ),
        json!({ "kind": "projectRemove", "project": project, "deletedState": delete_state }),
    ))
}

fn command_project_snapshot(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero project snapshot [PROJECT_ID]",
            json!({ "command": "project snapshot" }),
        ));
    }
    let project_id = take_option(&mut args, "--project-id")?
        .or_else(|| (!args.is_empty()).then(|| args.remove(0)))
        .or_else(|| {
            load_tui_settings(&globals)
                .ok()
                .and_then(|settings| settings.selected_project_id)
        })
        .ok_or_else(|| CliError::usage("Missing project id."))?;
    reject_project_unknown_options(&args)?;
    let project = project_by_id(&globals, &project_id)?;
    let sessions = list_sessions(&globals, &project_id, true)?;
    let runs = super::list_conversation_runs(&globals, Some(&project_id))?;
    let usage = project_usage_metadata(&project);
    let git_status = git_output_lossy(
        Path::new(&project.root_path),
        &["status", "--short", "--branch"],
    )
    .unwrap_or_else(|error| format!("unavailable: {}", error.message));
    Ok(response(
        &globals,
        format!(
            "Project: {}\nRoot: {}\nSessions: {}\nRuns: {}\nUsage: {} token(s), {} cost micros\nGit:\n{}",
            project.project_id,
            project.root_path,
            sessions.len(),
            runs.len(),
            usage["totalTokens"].as_i64().unwrap_or(0),
            usage["estimatedCostMicros"].as_i64().unwrap_or(0),
            git_status
        ),
        json!({
            "kind": "projectSnapshot",
            "project": project,
            "sessions": sessions,
            "runs": runs,
            "usage": usage,
            "gitStatus": git_status,
        }),
    ))
}

fn command_project_select(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let project_id = take_project_arg(&mut args, "select")?;
    reject_project_unknown_options(&args)?;
    let project = project_by_id(&globals, &project_id)?;
    let mut settings = load_tui_settings(&globals)?;
    settings.selected_project_id = Some(project.project_id.clone());
    save_tui_settings(&globals, &settings)?;
    Ok(response(
        &globals,
        format!("Selected project `{project_id}` for the terminal UI."),
        json!({ "kind": "projectSelect", "project": project, "settings": settings }),
    ))
}

fn command_session_list(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let include_archived = take_bool_flag(&mut args, "--include-archived");
    let project_id = take_project_id_option_or_selected(&globals, &mut args)?;
    reject_project_unknown_options(&args)?;
    let sessions = list_sessions(&globals, &project_id, include_archived)?;
    let text = if sessions.is_empty() {
        format!("No sessions found for project `{project_id}`.")
    } else {
        sessions
            .iter()
            .map(|session| {
                format!(
                    "{} {:<28} {:<10} {}",
                    if session.selected { "*" } else { " " },
                    session.agent_session_id,
                    session.status,
                    session.title
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({ "kind": "sessionList", "projectId": project_id, "sessions": sessions }),
    ))
}

fn command_session_create(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let project_id = take_project_id_option_or_selected(&globals, &mut args)?;
    let session_id =
        take_option(&mut args, "--session-id")?.unwrap_or_else(|| generate_id("session"));
    let title =
        take_option(&mut args, "--title")?.unwrap_or_else(|| title_from_session_id(&session_id));
    let session_kind =
        take_option(&mut args, "--session-kind")?.unwrap_or_else(|| "standard".to_owned());
    match session_kind.trim() {
        "standard" => {}
        "computer_use" => {
            return Err(CliError::usage(
                "Computer Use is a global TUI capability and cannot be created inside a project.",
            ));
        }
        other => {
            return Err(CliError::usage(format!(
                "`--session-kind` must be `standard` for project sessions, got `{other}`."
            )));
        }
    }
    reject_project_unknown_options(&args)?;
    upsert_session(&globals, &project_id, &session_id, &title, &session_kind)?;
    select_session(&globals, &project_id, &session_id)?;
    let session = session_by_id(&globals, &project_id, &session_id)?;
    Ok(response(
        &globals,
        format!("Created session `{session_id}` for project `{project_id}`."),
        json!({ "kind": "sessionCreate", "session": session }),
    ))
}

fn command_session_rename(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let project_id = take_project_id_option_or_selected(&globals, &mut args)?;
    let title =
        take_option(&mut args, "--title")?.ok_or_else(|| CliError::usage("Missing `--title`."))?;
    let session_id = take_session_arg(&mut args, "rename")?;
    reject_project_unknown_options(&args)?;
    let connection = project_connection(&globals, &project_id)?;
    connection
        .execute(
            "UPDATE agent_sessions SET title = ?3, updated_at = ?4 WHERE project_id = ?1 AND agent_session_id = ?2",
            params![project_id, session_id, title, now_timestamp()],
        )
        .map_err(|error| sqlite_error("xero_cli_session_rename_failed", error))?;
    let session = session_by_id(&globals, &project_id, &session_id)?;
    Ok(response(
        &globals,
        format!("Renamed session `{session_id}`."),
        json!({ "kind": "sessionRename", "session": session }),
    ))
}

fn command_session_auto_name(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let project_id = take_project_id_option_or_selected(&globals, &mut args)?;
    let session_id = take_session_arg(&mut args, "auto-name")?;
    reject_project_unknown_options(&args)?;
    let title = latest_prompt_for_session(&globals, &project_id, &session_id)?
        .map(|prompt| title_from_prompt(&prompt))
        .unwrap_or_else(|| title_from_session_id(&session_id));
    let connection = project_connection(&globals, &project_id)?;
    connection
        .execute(
            "UPDATE agent_sessions SET title = ?3, updated_at = ?4 WHERE project_id = ?1 AND agent_session_id = ?2",
            params![project_id, session_id, title, now_timestamp()],
        )
        .map_err(|error| sqlite_error("xero_cli_session_auto_name_failed", error))?;
    let session = session_by_id(&globals, &project_id, &session_id)?;
    Ok(response(
        &globals,
        format!(
            "Auto-named session `{}` as `{}`.",
            session_id, session.title
        ),
        json!({ "kind": "sessionAutoName", "session": session }),
    ))
}

fn command_session_status(
    globals: GlobalOptions,
    mut args: Vec<String>,
    status: &'static str,
) -> Result<CliResponse, CliError> {
    let project_id = take_project_id_option_or_selected(&globals, &mut args)?;
    let session_id = take_session_arg(&mut args, status)?;
    reject_project_unknown_options(&args)?;
    let archived_at = if status == "archived" {
        Some(now_timestamp())
    } else {
        None
    };
    let connection = project_connection(&globals, &project_id)?;
    connection
        .execute(
            "UPDATE agent_sessions SET status = ?3, archived_at = ?4, selected = 0, updated_at = ?5 WHERE project_id = ?1 AND agent_session_id = ?2",
            params![project_id, session_id, status, archived_at, now_timestamp()],
        )
        .map_err(|error| sqlite_error("xero_cli_session_status_failed", error))?;
    let session = session_by_id(&globals, &project_id, &session_id)?;
    Ok(response(
        &globals,
        format!("Marked session `{session_id}` as {status}."),
        json!({ "kind": "sessionStatus", "session": session }),
    ))
}

fn command_session_delete(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let project_id = take_project_id_option_or_selected(&globals, &mut args)?;
    let yes = take_bool_flag(&mut args, "--yes");
    let session_id = take_session_arg(&mut args, "delete")?;
    reject_project_unknown_options(&args)?;
    if !yes {
        return Err(CliError::usage(
            "Deleting a session requires `--yes` so accidental history loss is explicit.",
        ));
    }
    let connection = project_connection(&globals, &project_id)?;
    let deleted = connection
        .execute(
            "DELETE FROM agent_sessions WHERE project_id = ?1 AND agent_session_id = ?2",
            params![project_id, session_id],
        )
        .map_err(|error| sqlite_error("xero_cli_session_delete_failed", error))?;
    Ok(response(
        &globals,
        format!("Deleted session `{session_id}` ({deleted} row)."),
        json!({ "kind": "sessionDelete", "projectId": project_id, "sessionId": session_id, "deleted": deleted }),
    ))
}

fn command_session_select(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let project_id = take_project_id_option_or_selected(&globals, &mut args)?;
    let session_id = take_session_arg(&mut args, "select")?;
    reject_project_unknown_options(&args)?;
    select_session(&globals, &project_id, &session_id)?;
    let session = session_by_id(&globals, &project_id, &session_id)?;
    Ok(response(
        &globals,
        format!("Selected session `{session_id}`."),
        json!({ "kind": "sessionSelect", "session": session }),
    ))
}

fn command_git_status(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let repo = take_repo_path(&globals, &mut args)?;
    reject_project_unknown_options(&args)?;
    let output = git_output_lossy(&repo, &["status", "--short", "--branch"])?;
    Ok(response(
        &globals,
        output.clone(),
        json!({ "kind": "gitStatus", "repo": repo, "output": output }),
    ))
}

fn command_git_diff(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let staged = take_bool_flag(&mut args, "--staged") || take_bool_flag(&mut args, "--cached");
    let repo = take_repo_path(&globals, &mut args)?;
    reject_project_unknown_options(&args)?;
    let mut git_args = vec!["diff"];
    if staged {
        git_args.push("--cached");
    }
    let output = git_output_lossy(&repo, &git_args)?;
    Ok(response(
        &globals,
        output.clone(),
        json!({ "kind": "gitDiff", "repo": repo, "staged": staged, "output": output }),
    ))
}

fn command_git_passthrough(
    globals: GlobalOptions,
    mut args: Vec<String>,
    operation: &'static str,
    destructive: bool,
) -> Result<CliResponse, CliError> {
    let confirmed = take_bool_flag(&mut args, "--yes");
    let repo = take_repo_path(&globals, &mut args)?;
    let paths = args;
    if paths.is_empty() {
        return Err(CliError::usage(format!(
            "`xero git {operation}` requires at least one path."
        )));
    }
    if destructive && !confirmed {
        return Err(CliError::usage(
            "Discarding worktree changes requires `--yes`.",
        ));
    }
    let mut git_args = match operation {
        "add" => vec!["add", "--"],
        "restore-staged" => vec!["restore", "--staged", "--"],
        "restore-worktree" => vec!["restore", "--worktree", "--"],
        _ => unreachable!("known operation"),
    };
    git_args.extend(paths.iter().map(String::as_str));
    let output = git_output_lossy(&repo, &git_args)?;
    Ok(response(
        &globals,
        if output.is_empty() {
            "Git operation completed.".into()
        } else {
            output.clone()
        },
        json!({ "kind": "gitOperation", "operation": operation, "repo": repo, "paths": paths, "output": output }),
    ))
}

fn command_git_commit(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    let message = take_option(&mut args, "--message")?
        .or_else(|| take_option(&mut args, "-m").ok().flatten())
        .ok_or_else(|| CliError::usage("Missing `--message`."))?;
    let repo = take_repo_path(&globals, &mut args)?;
    reject_project_unknown_options(&args)?;
    let output = git_output_lossy(&repo, &["commit", "-m", &message])?;
    Ok(response(
        &globals,
        output.clone(),
        json!({ "kind": "gitCommit", "repo": repo, "message": message, "output": output }),
    ))
}

fn command_git_simple(
    globals: GlobalOptions,
    mut args: Vec<String>,
    git_args: &[&str],
) -> Result<CliResponse, CliError> {
    let repo = take_repo_path(&globals, &mut args)?;
    reject_project_unknown_options(&args)?;
    let output = git_output_lossy(&repo, git_args)?;
    Ok(response(
        &globals,
        output.clone(),
        json!({ "kind": "gitOperation", "repo": repo, "operation": git_args[0], "output": output }),
    ))
}

fn import_existing_project(
    globals: &GlobalOptions,
    selected_path: &str,
    kind: ProjectKind,
) -> Result<ProjectRecord, CliError> {
    let root = resolve_git_root(selected_path)?;
    let root_path = root.to_string_lossy().into_owned();
    let project_id = stable_project_id_for_repo_root(&root);
    let repository_id = format!("repo_{}", project_id.trim_start_matches("project_"));
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&root_path)
        .to_owned();
    let branch = git_output_lossy(&root, &["branch", "--show-current"])
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let head_sha = git_output_lossy(&root, &["rev-parse", "HEAD"])
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    upsert_global_project(
        globals,
        &project_id,
        &repository_id,
        &name,
        &root_path,
        branch.as_deref(),
        head_sha.as_deref(),
    )?;
    ensure_project_state(
        globals,
        ProjectStateSeed {
            project_id: &project_id,
            repository_id: &repository_id,
            name: &name,
            root_path: &root_path,
            branch: branch.as_deref(),
            head_sha: head_sha.as_deref(),
            kind,
        },
    )?;
    project_by_id(globals, &project_id)
}

fn list_projects(globals: &GlobalOptions) -> Result<Vec<ProjectRecord>, CliError> {
    let database_path = global_database_path(globals);
    if !database_path.exists() {
        return Ok(Vec::new());
    }
    let settings = load_tui_settings(globals)?;
    let connection = open_global_registry(globals)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT projects.id, repositories.id, projects.name, repositories.root_path,
                   repositories.branch, repositories.head_sha, projects.start_targets
            FROM projects
            JOIN repositories ON repositories.project_id = projects.id
            WHERE projects.id != ?1
            ORDER BY projects.updated_at DESC, repositories.updated_at DESC, repositories.root_path ASC
            "#,
        )
        .map_err(|error| sqlite_error("xero_cli_project_list_prepare_failed", error))?;
    let app_data_root = cli_app_data_root(globals);
    let rows = statement
        .query_map(params![GLOBAL_COMPUTER_USE_PROJECT_ID], |row| {
            let project_id: String = row.get(0)?;
            let root_path: String = row.get(3)?;
            let raw_start_targets: String = row.get(6)?;
            let database_path =
                super::workspace_project_database_path_for_app_root(&app_data_root, &project_id);
            Ok(ProjectRecord {
                selected: settings.selected_project_id.as_deref() == Some(project_id.as_str()),
                root_exists: Path::new(&root_path).is_dir(),
                state_exists: database_path.exists(),
                database_path: database_path.to_string_lossy().into_owned(),
                project_id,
                repository_id: row.get(1)?,
                name: row.get(2)?,
                root_path,
                branch: row.get(4)?,
                head_sha: row.get(5)?,
                start_targets: serde_json::from_str(&raw_start_targets)
                    .unwrap_or_else(|_| json!([])),
            })
        })
        .map_err(|error| sqlite_error("xero_cli_project_list_failed", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlite_error("xero_cli_project_list_decode_failed", error))
}

fn project_by_id(globals: &GlobalOptions, project_id: &str) -> Result<ProjectRecord, CliError> {
    list_projects(globals)?
        .into_iter()
        .find(|project| project.project_id == project_id)
        .ok_or_else(|| {
            CliError::user_fixable(
                "xero_cli_project_unknown",
                format!("Project `{project_id}` is not registered in Xero app-data."),
            )
        })
}

fn list_sessions(
    globals: &GlobalOptions,
    project_id: &str,
    include_archived: bool,
) -> Result<Vec<SessionRecord>, CliError> {
    let connection = project_connection(globals, project_id)?;
    let settings = load_tui_settings(globals)?;
    let mut sql = String::from(
        r#"
        SELECT project_id, agent_session_id, title, summary, session_kind, status, selected,
               created_at, updated_at, archived_at, last_run_id, last_provider_id
        FROM agent_sessions
        WHERE project_id = ?1
        "#,
    );
    if !include_archived {
        sql.push_str(" AND status != 'archived'");
    }
    sql.push_str(" ORDER BY selected DESC, updated_at DESC, agent_session_id ASC");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| sqlite_error("xero_cli_session_list_prepare_failed", error))?;
    let rows = statement
        .query_map(params![project_id], |row| {
            let agent_session_id: String = row.get(1)?;
            let db_selected = row.get::<_, i64>(6)? != 0;
            Ok(SessionRecord {
                selected: settings.selected_session_id.as_deref()
                    == Some(agent_session_id.as_str())
                    || db_selected,
                project_id: row.get(0)?,
                agent_session_id,
                title: row.get(2)?,
                summary: row.get(3)?,
                session_kind: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
                archived_at: row.get(9)?,
                last_run_id: row.get(10)?,
                last_provider_id: row.get(11)?,
            })
        })
        .map_err(|error| sqlite_error("xero_cli_session_list_failed", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlite_error("xero_cli_session_list_decode_failed", error))
}

fn session_by_id(
    globals: &GlobalOptions,
    project_id: &str,
    session_id: &str,
) -> Result<SessionRecord, CliError> {
    list_sessions(globals, project_id, true)?
        .into_iter()
        .find(|session| session.agent_session_id == session_id)
        .ok_or_else(|| {
            CliError::user_fixable(
                "xero_cli_session_unknown",
                format!("Session `{session_id}` does not exist in project `{project_id}`."),
            )
        })
}

fn latest_prompt_for_session(
    globals: &GlobalOptions,
    project_id: &str,
    session_id: &str,
) -> Result<Option<String>, CliError> {
    let connection = project_connection(globals, project_id)?;
    connection
        .query_row(
            r#"
            SELECT prompt
            FROM agent_runs
            WHERE project_id = ?1 AND agent_session_id = ?2
            ORDER BY updated_at DESC, started_at DESC, run_id DESC
            LIMIT 1
            "#,
            params![project_id, session_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| sqlite_error("xero_cli_session_auto_name_failed", error))
}

fn upsert_session(
    globals: &GlobalOptions,
    project_id: &str,
    session_id: &str,
    title: &str,
    session_kind: &str,
) -> Result<(), CliError> {
    validate_required_cli(session_id, "sessionId")?;
    validate_required_cli(title, "title")?;
    validate_session_kind(session_kind)?;
    let connection = project_connection(globals, project_id)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_sessions (
                project_id, agent_session_id, title, summary, session_kind, status, selected, updated_at
            )
            VALUES (?1, ?2, ?3, '', ?4, 'active', 0, ?5)
            ON CONFLICT(project_id, agent_session_id) DO UPDATE SET
                title = excluded.title,
                session_kind = excluded.session_kind,
                status = 'active',
                archived_at = NULL,
                updated_at = excluded.updated_at
            "#,
            params![project_id, session_id, title, session_kind, now_timestamp()],
        )
        .map_err(|error| sqlite_error("xero_cli_session_upsert_failed", error))?;
    Ok(())
}

fn validate_session_kind(value: &str) -> Result<(), CliError> {
    match value.trim() {
        "standard" | "computer_use" => Ok(()),
        other => Err(CliError::usage(format!(
            "Session kind must be `standard` or `computer_use`, got `{other}`."
        ))),
    }
}

fn title_from_session_id(session_id: &str) -> String {
    let _ = session_id;
    DEFAULT_AGENT_SESSION_TITLE.into()
}

fn title_from_prompt(prompt: &str) -> String {
    let cleaned = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = cleaned
        .trim_start_matches(|character: char| {
            matches!(character, '#' | '-' | '*' | '>' | '"' | '\'' | '`')
        })
        .trim();
    let title = truncate_session_title(
        &trim_trailing_session_title_punctuation(cleaned),
        MAX_SESSION_TITLE_CHARS,
    );

    if title.trim().is_empty() || is_generic_session_title(&title) {
        DEFAULT_AGENT_SESSION_TITLE.into()
    } else {
        title
    }
}

fn truncate_session_title(title: &str, max_chars: usize) -> String {
    let trimmed = title.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_owned();
    }

    let mut output = String::new();
    for word in trimmed.split_whitespace() {
        let next_len =
            output.chars().count() + if output.is_empty() { 0 } else { 1 } + word.chars().count();
        if next_len > max_chars {
            break;
        }
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(word);
    }

    if output.is_empty() {
        trimmed.chars().take(max_chars).collect()
    } else {
        output
    }
}

fn trim_trailing_session_title_punctuation(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(|character: char| {
            matches!(
                character,
                '.' | ',' | ':' | ';' | '!' | '?' | '-' | '_' | '"' | '\'' | '`'
            )
        })
        .trim()
        .to_owned()
}

fn is_generic_session_title(title: &str) -> bool {
    let normalized = title
        .trim()
        .trim_matches(|character: char| matches!(character, '"' | '\'' | '`'))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "main"
            | "new chat"
            | "new session"
            | "untitled"
            | "untitled session"
            | "chat"
            | "session"
            | "conversation"
            | "developer conversation"
            | "developer assistant conversation"
    )
}

fn select_session(
    globals: &GlobalOptions,
    project_id: &str,
    session_id: &str,
) -> Result<(), CliError> {
    let connection = project_connection(globals, project_id)?;
    connection
        .execute(
            "UPDATE agent_sessions SET selected = 0 WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(|error| sqlite_error("xero_cli_session_select_failed", error))?;
    let changed = connection
        .execute(
            "UPDATE agent_sessions SET selected = 1, updated_at = ?3 WHERE project_id = ?1 AND agent_session_id = ?2",
            params![project_id, session_id, now_timestamp()],
        )
        .map_err(|error| sqlite_error("xero_cli_session_select_failed", error))?;
    if changed == 0 {
        return Err(CliError::user_fixable(
            "xero_cli_session_unknown",
            format!("Session `{session_id}` does not exist in project `{project_id}`."),
        ));
    }
    let mut settings = load_tui_settings(globals)?;
    settings.selected_project_id = Some(project_id.to_owned());
    settings.selected_session_id = Some(session_id.to_owned());
    save_tui_settings(globals, &settings)
}

fn open_global_registry(globals: &GlobalOptions) -> Result<Connection, CliError> {
    let path = global_database_path(globals);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::system_fault(
                "xero_cli_project_registry_prepare_failed",
                format!("Could not create `{}`: {error}", parent.display()),
            )
        })?;
    }
    let connection = Connection::open(&path).map_err(|error| {
        CliError::system_fault(
            "xero_cli_project_registry_open_failed",
            format!("Could not open `{}`: {error}", path.display()),
        )
    })?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| sqlite_error("xero_cli_project_registry_config_failed", error))?;
    connection
        .execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                milestone TEXT NOT NULL DEFAULT '',
                total_phases INTEGER NOT NULL DEFAULT 0 CHECK (total_phases >= 0),
                completed_phases INTEGER NOT NULL DEFAULT 0 CHECK (completed_phases >= 0),
                active_phase INTEGER NOT NULL DEFAULT 0 CHECK (active_phase >= 0),
                branch TEXT,
                runtime TEXT,
                start_targets TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
            CREATE TABLE IF NOT EXISTS repositories (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                root_path TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                branch TEXT,
                head_sha TEXT,
                is_git_repo INTEGER NOT NULL DEFAULT 1 CHECK (is_git_repo IN (0, 1)),
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );
            CREATE INDEX IF NOT EXISTS idx_repositories_project_id ON repositories(project_id);
            CREATE INDEX IF NOT EXISTS idx_repositories_root_path ON repositories(root_path);
            "#,
        )
        .map_err(|error| sqlite_error("xero_cli_project_registry_migrate_failed", error))?;
    ensure_project_start_targets_column(&connection)?;
    ensure_agent_session_kind_column(&connection)?;
    Ok(connection)
}

fn upsert_global_project(
    globals: &GlobalOptions,
    project_id: &str,
    repository_id: &str,
    name: &str,
    root_path: &str,
    branch: Option<&str>,
    head_sha: Option<&str>,
) -> Result<(), CliError> {
    let connection = open_global_registry(globals)?;
    connection
        .execute(
            r#"
            INSERT INTO projects (id, name, branch, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                branch = excluded.branch,
                updated_at = excluded.updated_at
            "#,
            params![project_id, name, branch, now_timestamp()],
        )
        .map_err(|error| sqlite_error("xero_cli_project_registry_write_failed", error))?;
    connection
        .execute(
            r#"
            INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7)
            ON CONFLICT(id) DO UPDATE SET
                project_id = excluded.project_id,
                root_path = excluded.root_path,
                display_name = excluded.display_name,
                branch = excluded.branch,
                head_sha = excluded.head_sha,
                is_git_repo = excluded.is_git_repo,
                updated_at = excluded.updated_at
            "#,
            params![repository_id, project_id, root_path, name, branch, head_sha, now_timestamp()],
        )
        .map_err(|error| sqlite_error("xero_cli_project_registry_write_failed", error))?;
    Ok(())
}

pub(crate) fn global_computer_use_root(globals: &GlobalOptions) -> PathBuf {
    cli_app_data_root(globals).join(GLOBAL_COMPUTER_USE_DIR)
}

pub(crate) fn global_computer_use_database_path(globals: &GlobalOptions) -> PathBuf {
    global_computer_use_root(globals).join(GLOBAL_COMPUTER_USE_STATE_DB)
}

pub(crate) fn ensure_global_computer_use_project(globals: &GlobalOptions) -> Result<(), CliError> {
    let root_path = global_computer_use_root(globals);
    fs::create_dir_all(&root_path).map_err(|error| {
        CliError::system_fault(
            "xero_cli_computer_use_root_prepare_failed",
            format!(
                "Could not create Computer Use app-data root `{}`: {error}",
                root_path.display()
            ),
        )
    })?;

    let root_path_string = root_path.to_string_lossy().into_owned();
    upsert_global_computer_use_registry(globals, &root_path_string)?;
    ensure_global_computer_use_state(globals, &root_path_string)
}

fn upsert_global_computer_use_registry(
    globals: &GlobalOptions,
    root_path: &str,
) -> Result<(), CliError> {
    let connection = open_global_registry(globals)?;
    let now = now_timestamp();
    connection
        .execute(
            r#"
            INSERT INTO projects (id, name, branch, updated_at)
            VALUES (?1, ?2, NULL, ?3)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                branch = NULL,
                updated_at = excluded.updated_at
            "#,
            params![
                GLOBAL_COMPUTER_USE_PROJECT_ID,
                GLOBAL_COMPUTER_USE_PROJECT_NAME,
                now,
            ],
        )
        .map_err(|error| sqlite_error("xero_cli_computer_use_registry_write_failed", error))?;
    connection
        .execute(
            r#"
            INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo, updated_at)
            VALUES (?1, ?2, ?3, ?4, NULL, NULL, 0, ?5)
            ON CONFLICT(id) DO UPDATE SET
                project_id = excluded.project_id,
                root_path = excluded.root_path,
                display_name = excluded.display_name,
                branch = NULL,
                head_sha = NULL,
                is_git_repo = 0,
                updated_at = excluded.updated_at
            "#,
            params![
                GLOBAL_COMPUTER_USE_REPOSITORY_ID,
                GLOBAL_COMPUTER_USE_PROJECT_ID,
                root_path,
                GLOBAL_COMPUTER_USE_PROJECT_NAME,
                now,
            ],
        )
        .map_err(|error| sqlite_error("xero_cli_computer_use_registry_write_failed", error))?;
    Ok(())
}

fn ensure_global_computer_use_state(
    globals: &GlobalOptions,
    root_path: &str,
) -> Result<(), CliError> {
    let database_path = global_computer_use_database_path(globals);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::system_fault(
                "xero_cli_computer_use_state_prepare_failed",
                format!(
                    "Could not create Computer Use state directory `{}`: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    let connection = Connection::open(&database_path)
        .map_err(|error| sqlite_error("xero_cli_computer_use_state_open_failed", error))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| sqlite_error("xero_cli_computer_use_state_config_failed", error))?;
    connection
        .execute_batch(BENCHMARK_PROJECT_SCHEMA)
        .map_err(|error| sqlite_error("xero_cli_computer_use_state_migrate_failed", error))?;
    ensure_project_start_targets_column(&connection)?;
    ensure_agent_session_kind_column(&connection)?;

    let now = now_timestamp();
    let tx = connection
        .unchecked_transaction()
        .map_err(|error| sqlite_error("xero_cli_computer_use_state_write_failed", error))?;
    tx.execute(
        r#"
        INSERT INTO projects (id, name, branch, updated_at)
        VALUES (?1, ?2, NULL, ?3)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            branch = NULL,
            updated_at = excluded.updated_at
        "#,
        params![
            GLOBAL_COMPUTER_USE_PROJECT_ID,
            GLOBAL_COMPUTER_USE_PROJECT_NAME,
            now,
        ],
    )
    .map_err(|error| sqlite_error("xero_cli_computer_use_state_write_failed", error))?;
    tx.execute(
        r#"
        INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo, updated_at)
        VALUES (?1, ?2, ?3, ?4, NULL, NULL, 0, ?5)
        ON CONFLICT(id) DO UPDATE SET
            project_id = excluded.project_id,
            root_path = excluded.root_path,
            display_name = excluded.display_name,
            branch = NULL,
            head_sha = NULL,
            is_git_repo = 0,
            updated_at = excluded.updated_at
        "#,
        params![
            GLOBAL_COMPUTER_USE_REPOSITORY_ID,
            GLOBAL_COMPUTER_USE_PROJECT_ID,
            root_path,
            GLOBAL_COMPUTER_USE_PROJECT_NAME,
            now,
        ],
    )
    .map_err(|error| sqlite_error("xero_cli_computer_use_state_write_failed", error))?;
    tx.execute(
        r#"
        INSERT INTO agent_sessions (
            project_id,
            agent_session_id,
            title,
            summary,
            session_kind,
            status,
            selected,
            remote_visible,
            updated_at
        )
        VALUES (?1, ?2, ?3, '', 'computer_use', 'active', 1, 0, ?4)
        ON CONFLICT(project_id, agent_session_id) DO UPDATE SET
            session_kind = 'computer_use',
            title = CASE
                WHEN trim(agent_sessions.title) = '' THEN excluded.title
                ELSE agent_sessions.title
            END,
            status = 'active',
            selected = 1,
            remote_visible = 0,
            archived_at = NULL
        "#,
        params![
            GLOBAL_COMPUTER_USE_PROJECT_ID,
            GLOBAL_COMPUTER_USE_AGENT_SESSION_ID,
            GLOBAL_COMPUTER_USE_PROJECT_NAME,
            now,
        ],
    )
    .map_err(|error| sqlite_error("xero_cli_computer_use_state_write_failed", error))?;
    tx.commit()
        .map_err(|error| sqlite_error("xero_cli_computer_use_state_write_failed", error))?;
    Ok(())
}

struct ProjectStateSeed<'a> {
    project_id: &'a str,
    repository_id: &'a str,
    name: &'a str,
    root_path: &'a str,
    branch: Option<&'a str>,
    head_sha: Option<&'a str>,
    kind: ProjectKind,
}

fn ensure_project_state(
    globals: &GlobalOptions,
    seed: ProjectStateSeed<'_>,
) -> Result<(), CliError> {
    let ProjectStateSeed {
        project_id,
        repository_id,
        name,
        root_path,
        branch,
        head_sha,
        kind,
    } = seed;
    let database_path = workspace_project_database_path(globals, project_id);
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliError::system_fault(
                "xero_cli_project_state_prepare_failed",
                format!("Could not create `{}`: {error}", parent.display()),
            )
        })?;
    }
    let connection = Connection::open(&database_path)
        .map_err(|error| sqlite_error("xero_cli_project_state_open_failed", error))?;
    connection
        .execute_batch(BENCHMARK_PROJECT_SCHEMA)
        .map_err(|error| sqlite_error("xero_cli_project_state_migrate_failed", error))?;
    ensure_project_start_targets_column(&connection)?;
    connection
        .execute(
            r#"
            INSERT INTO projects (id, name, branch, updated_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                branch = excluded.branch,
                updated_at = excluded.updated_at
            "#,
            params![project_id, name, branch, now_timestamp()],
        )
        .map_err(|error| sqlite_error("xero_cli_project_state_write_failed", error))?;
    connection
        .execute(
            r#"
            INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7)
            ON CONFLICT(id) DO UPDATE SET
                project_id = excluded.project_id,
                root_path = excluded.root_path,
                display_name = excluded.display_name,
                branch = excluded.branch,
                head_sha = excluded.head_sha,
                is_git_repo = excluded.is_git_repo,
                updated_at = excluded.updated_at
            "#,
            params![repository_id, project_id, root_path, name, branch, head_sha, now_timestamp()],
        )
        .map_err(|error| sqlite_error("xero_cli_project_state_write_failed", error))?;
    connection
        .execute(
            r#"
            INSERT INTO agent_sessions (
                project_id, agent_session_id, title, summary, status, selected, updated_at
            )
            VALUES (?1, 'agent-session-main', ?2, '', 'active', 1, ?3)
            ON CONFLICT(project_id, agent_session_id) DO UPDATE SET
                status = 'active',
                selected = CASE WHEN agent_sessions.status = 'active' THEN 1 ELSE agent_sessions.selected END,
                updated_at = excluded.updated_at
            "#,
            params![
                project_id,
                match kind {
                    ProjectKind::Brownfield => DEFAULT_AGENT_SESSION_TITLE,
                    ProjectKind::Greenfield => DEFAULT_AGENT_SESSION_TITLE,
                },
                now_timestamp(),
            ],
        )
        .map_err(|error| sqlite_error("xero_cli_project_state_write_failed", error))?;
    Ok(())
}

fn ensure_project_start_targets_column(connection: &Connection) -> Result<(), CliError> {
    let mut statement = connection
        .prepare("PRAGMA table_info(projects)")
        .map_err(|error| sqlite_error("xero_cli_project_start_targets_probe_failed", error))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| sqlite_error("xero_cli_project_start_targets_probe_failed", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlite_error("xero_cli_project_start_targets_probe_failed", error))?;
    if columns.iter().any(|column| column == "start_targets") {
        return Ok(());
    }
    connection
        .execute(
            "ALTER TABLE projects ADD COLUMN start_targets TEXT NOT NULL DEFAULT '[]'",
            [],
        )
        .map_err(|error| sqlite_error("xero_cli_project_start_targets_migrate_failed", error))?;
    Ok(())
}

fn ensure_agent_session_kind_column(connection: &Connection) -> Result<(), CliError> {
    let mut statement = connection
        .prepare("PRAGMA table_info(agent_sessions)")
        .map_err(|error| sqlite_error("xero_cli_session_kind_probe_failed", error))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| sqlite_error("xero_cli_session_kind_probe_failed", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlite_error("xero_cli_session_kind_probe_failed", error))?;
    if columns.is_empty() {
        return Ok(());
    }
    if columns.iter().any(|column| column == "session_kind") {
        return Ok(());
    }
    connection
        .execute(
            "ALTER TABLE agent_sessions ADD COLUMN session_kind TEXT NOT NULL DEFAULT 'standard'",
            [],
        )
        .map_err(|error| sqlite_error("xero_cli_session_kind_migrate_failed", error))?;
    Ok(())
}

fn decode_start_targets(value: &JsonValue) -> Result<Vec<StartTargetRecord>, CliError> {
    let targets = value
        .as_array()
        .map(|targets| {
            targets
                .iter()
                .map(|target| {
                    serde_json::from_value::<StartTargetRecord>(target.clone()).map_err(|error| {
                        CliError::user_fixable(
                            "xero_cli_project_start_targets_decode_failed",
                            format!("Could not decode project start target: {error}"),
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(targets)
}

pub(crate) fn project_connection(
    globals: &GlobalOptions,
    project_id: &str,
) -> Result<Connection, CliError> {
    validate_required_cli(project_id, "projectId")?;
    if project_id == GLOBAL_COMPUTER_USE_PROJECT_ID {
        ensure_global_computer_use_project(globals)?;
        let database_path = global_computer_use_database_path(globals);
        let connection = Connection::open(&database_path)
            .map_err(|error| sqlite_error("xero_cli_project_state_open_failed", error))?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|error| sqlite_error("xero_cli_project_state_config_failed", error))?;
        ensure_project_start_targets_column(&connection)?;
        ensure_agent_session_kind_column(&connection)?;
        return Ok(connection);
    }
    let project = project_by_id(globals, project_id)?;
    if !Path::new(&project.database_path).exists() {
        return Err(CliError::user_fixable(
            "xero_cli_project_state_missing",
            format!(
                "Project `{project_id}` is registered but `{}` does not exist.",
                project.database_path
            ),
        ));
    }
    let connection = Connection::open(&project.database_path)
        .map_err(|error| sqlite_error("xero_cli_project_state_open_failed", error))?;
    ensure_project_start_targets_column(&connection)?;
    Ok(connection)
}

pub(crate) fn project_root_path(
    globals: &GlobalOptions,
    project_id: &str,
) -> Result<PathBuf, CliError> {
    if project_id == GLOBAL_COMPUTER_USE_PROJECT_ID {
        ensure_global_computer_use_project(globals)?;
        return Ok(global_computer_use_root(globals));
    }
    Ok(PathBuf::from(project_by_id(globals, project_id)?.root_path))
}

pub(crate) fn project_start_targets(
    globals: &GlobalOptions,
    project_id: &str,
) -> Result<Vec<StartTargetRecord>, CliError> {
    let project = project_by_id(globals, project_id)?;
    decode_start_targets(&project.start_targets)
}

pub(crate) fn update_project_start_targets(
    globals: &GlobalOptions,
    project_id: &str,
    targets: &[StartTargetRecord],
) -> Result<(), CliError> {
    let encoded = serde_json::to_string(targets).map_err(|error| {
        CliError::system_fault(
            "xero_cli_project_start_targets_encode_failed",
            format!("Could not encode project start targets: {error}"),
        )
    })?;
    let updated_at = now_timestamp();
    let global = open_global_registry(globals)?;
    global
        .execute(
            "UPDATE projects SET start_targets = ?2, updated_at = ?3 WHERE id = ?1",
            params![project_id, encoded, updated_at],
        )
        .map_err(|error| sqlite_error("xero_cli_project_start_targets_write_failed", error))?;

    let project_connection = project_connection(globals, project_id)?;
    project_connection
        .execute(
            "UPDATE projects SET start_targets = ?2, updated_at = ?3 WHERE id = ?1",
            params![project_id, encoded, updated_at],
        )
        .map_err(|error| sqlite_error("xero_cli_project_start_targets_write_failed", error))?;
    Ok(())
}

fn project_usage_metadata(project: &ProjectRecord) -> serde_json::Value {
    if !Path::new(&project.database_path).exists() {
        return json!({
            "available": false,
            "inputTokens": 0,
            "outputTokens": 0,
            "totalTokens": 0,
            "estimatedCostMicros": 0,
            "reason": "project_state_missing",
        });
    }
    let Ok(connection) = Connection::open(&project.database_path) else {
        return json!({
            "available": false,
            "inputTokens": 0,
            "outputTokens": 0,
            "totalTokens": 0,
            "estimatedCostMicros": 0,
            "reason": "project_state_unreadable",
        });
    };
    let table_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'agent_usage')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false);
    if !table_exists {
        return json!({
            "available": false,
            "inputTokens": 0,
            "outputTokens": 0,
            "totalTokens": 0,
            "estimatedCostMicros": 0,
            "reason": "agent_usage_table_missing",
        });
    }
    let totals = connection
        .query_row(
            r#"
            SELECT
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(estimated_cost_micros), 0)
            FROM agent_usage
            WHERE project_id = ?1
            "#,
            params![project.project_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .unwrap_or((0, 0, 0, 0));
    json!({
        "available": true,
        "inputTokens": totals.0,
        "outputTokens": totals.1,
        "totalTokens": totals.2,
        "estimatedCostMicros": totals.3,
    })
}

pub(crate) fn take_repo_path(
    globals: &GlobalOptions,
    args: &mut Vec<String>,
) -> Result<PathBuf, CliError> {
    if let Some(repo) = take_option(args, "--repo")? {
        return canonicalize_existing_path(&repo);
    }
    let project_id = take_option(args, "--project-id")?.or_else(|| {
        load_tui_settings(globals)
            .ok()
            .and_then(|settings| settings.selected_project_id)
    });
    if let Some(project_id) = project_id {
        return Ok(PathBuf::from(
            project_by_id(globals, &project_id)?.root_path,
        ));
    }
    canonicalize_existing_path(".")
}

pub(crate) fn take_project_id_option_or_selected(
    globals: &GlobalOptions,
    args: &mut Vec<String>,
) -> Result<String, CliError> {
    take_option(args, "--project-id")?
        .or_else(|| {
            load_tui_settings(globals)
                .ok()
                .and_then(|settings| settings.selected_project_id)
        })
        .ok_or_else(|| CliError::usage("Missing `--project-id` and no TUI project is selected."))
}

fn take_project_arg(args: &mut Vec<String>, command: &str) -> Result<String, CliError> {
    if args.is_empty() {
        Err(CliError::usage(format!(
            "Missing project id for `{command}`."
        )))
    } else {
        Ok(args.remove(0))
    }
}

fn take_session_arg(args: &mut Vec<String>, command: &str) -> Result<String, CliError> {
    if args.is_empty() {
        Err(CliError::usage(format!(
            "Missing session id for `{command}`."
        )))
    } else {
        Ok(args.remove(0))
    }
}

fn load_tui_settings(globals: &GlobalOptions) -> Result<TuiSettings, CliError> {
    let path = tui_settings_path(globals);
    if !path.exists() {
        return Ok(TuiSettings::default());
    }
    read_json_file(&path)
}

fn save_tui_settings(globals: &GlobalOptions, settings: &TuiSettings) -> Result<(), CliError> {
    write_json_file(&tui_settings_path(globals), settings)
}

fn tui_settings_path(globals: &GlobalOptions) -> PathBuf {
    globals.state_dir.join(TUI_SETTINGS_FILE)
}

fn resolve_git_root(path: &str) -> Result<PathBuf, CliError> {
    let selected = canonicalize_existing_path(path)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(&selected)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_git_failed",
                format!("Could not run git in `{}`: {error}", selected.display()),
            )
        })?;
    if !output.status.success() {
        return Err(CliError::user_fixable(
            "xero_cli_git_repository_required",
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    canonicalize_existing_path(String::from_utf8_lossy(&output.stdout).trim())
}

fn git_output_lossy(repo: &Path, args: &[&str]) -> Result<String, CliError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_git_failed",
                format!("Could not run git in `{}`: {error}", repo.display()),
            )
        })?;
    if !output.status.success() {
        return Err(CliError::user_fixable(
            "xero_cli_git_failed",
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn run_command_checked(cwd: &Path, program: &str, args: &[&str]) -> Result<(), CliError> {
    let output = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_command_failed",
                format!("Could not run `{program}` in `{}`: {error}", cwd.display()),
            )
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(CliError::user_fixable(
            "xero_cli_command_failed",
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

fn validate_project_name(name: &str) -> Result<(), CliError> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed == "."
        || trimmed == ".."
    {
        return Err(CliError::usage(
            "Project name must be a single non-empty path segment.",
        ));
    }
    Ok(())
}

fn reject_project_unknown_options(args: &[String]) -> Result<(), CliError> {
    super::reject_unknown_options(args)
}

fn sqlite_error(code: &'static str, error: rusqlite::Error) -> CliError {
    CliError::system_fault(code, format!("SQLite operation failed: {error}"))
}

#[derive(Debug, Clone, Copy)]
enum ProjectKind {
    Brownfield,
    Greenfield,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        env::temp_dir().join(format!("xero-cli-{label}-{nonce}"))
    }

    fn globals_for(path: PathBuf) -> GlobalOptions {
        GlobalOptions {
            output_mode: super::super::OutputMode::Json,
            ci: false,
            state_dir: path,
            tui_adapter: None,
        }
    }

    #[test]
    fn project_name_rejects_path_segments() {
        assert!(validate_project_name("demo").is_ok());
        assert!(validate_project_name("../demo").is_err());
        assert!(validate_project_name("nested/demo").is_err());
    }

    #[test]
    fn tui_settings_round_trip_uses_state_dir() {
        let root = temp_path("settings");
        let globals = globals_for(root.clone());
        let settings = TuiSettings {
            selected_project_id: Some("project-1".into()),
            selected_session_id: Some("session-1".into()),
        };
        save_tui_settings(&globals, &settings).expect("save settings");
        let loaded = load_tui_settings(&globals).expect("load settings");
        assert_eq!(loaded.selected_project_id.as_deref(), Some("project-1"));
        assert_eq!(loaded.selected_session_id.as_deref(), Some("session-1"));
        assert!(root.join(TUI_SETTINGS_FILE).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn session_auto_title_uses_prompt_prefix() {
        assert_eq!(
            title_from_prompt("Implement the TUI session browser now please"),
            "Implement the TUI session browser now please"
        );
        assert_eq!(
            title_from_prompt("  - Fix cloud session naming!!!\nIgnore this line"),
            "Fix cloud session naming"
        );
        assert_eq!(title_from_prompt("Session"), "New Chat");
        assert_eq!(title_from_prompt("   "), "New Chat");
        assert_eq!(
            title_from_session_id("session-1234567890abcdef"),
            "New Chat"
        );
    }

    #[test]
    fn ensure_project_state_seeds_default_session_as_new_chat() {
        let root = temp_path("default-session-title");
        let globals = globals_for(root.clone());
        let root_path = root.to_string_lossy().into_owned();

        for (project_id, kind) in [
            ("project-brownfield", ProjectKind::Brownfield),
            ("project-greenfield", ProjectKind::Greenfield),
        ] {
            let repository_id = format!("repo-{project_id}");
            ensure_project_state(
                &globals,
                ProjectStateSeed {
                    project_id,
                    repository_id: &repository_id,
                    name: "Project",
                    root_path: &root_path,
                    branch: Some("main"),
                    head_sha: Some("abc123"),
                    kind,
                },
            )
            .expect("ensure project state");

            let connection =
                Connection::open(workspace_project_database_path(&globals, project_id))
                    .expect("open project database");
            let title: String = connection
                .query_row(
                    "SELECT title FROM agent_sessions WHERE project_id = ?1 AND agent_session_id = 'agent-session-main'",
                    params![project_id],
                    |row| row.get(0),
                )
                .expect("default agent session title");
            assert_eq!(title, DEFAULT_AGENT_SESSION_TITLE);
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn global_computer_use_project_is_app_data_backed_and_hidden() {
        let root = temp_path("computer-use");
        let globals = globals_for(root.clone());

        ensure_global_computer_use_project(&globals).expect("ensure global computer use project");

        let database_path = global_computer_use_database_path(&globals);
        assert!(database_path.exists());
        assert!(database_path.starts_with(cli_app_data_root(&globals)));
        assert!(list_projects(&globals)
            .expect("list projects")
            .iter()
            .all(|project| project.project_id != GLOBAL_COMPUTER_USE_PROJECT_ID));

        let connection =
            project_connection(&globals, GLOBAL_COMPUTER_USE_PROJECT_ID).expect("project db");
        let (title, session_kind, remote_visible): (String, String, i64) = connection
            .query_row(
                r#"
                SELECT title, session_kind, remote_visible
                FROM agent_sessions
                WHERE project_id = ?1 AND agent_session_id = ?2
                "#,
                params![
                    GLOBAL_COMPUTER_USE_PROJECT_ID,
                    GLOBAL_COMPUTER_USE_AGENT_SESSION_ID
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("global computer use session");
        assert_eq!(title, GLOBAL_COMPUTER_USE_PROJECT_NAME);
        assert_eq!(session_kind, "computer_use");
        assert_eq!(remote_visible, 0);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn project_session_create_rejects_computer_use_kind() {
        let root = temp_path("project-computer-use-session-kind");
        let globals = globals_for(root.clone());

        let error = command_session_create(
            globals.clone(),
            vec![
                "--project-id".into(),
                "project-1".into(),
                "--session-kind".into(),
                "computer_use".into(),
            ],
        )
        .expect_err("computer use cannot be project-scoped");

        assert!(error
            .message
            .contains("Computer Use is a global TUI capability"));

        let _ = fs::remove_dir_all(root);
    }
}
