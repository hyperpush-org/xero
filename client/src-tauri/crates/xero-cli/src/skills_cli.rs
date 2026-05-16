use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::json;

use super::{
    now_timestamp, project_cli, response, take_help, take_option, CliError, CliResponse,
    GlobalOptions,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillRegistryRow {
    source_id: String,
    scope_kind: String,
    project_id: Option<String>,
    skill_id: String,
    name: String,
    source_state: String,
    trust_state: String,
    user_invocable: Option<bool>,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginRegistryRow {
    plugin_id: String,
    name: String,
    version: String,
    plugin_state: String,
    trust_state: String,
    root_id: String,
    updated_at: String,
}

pub(crate) fn dispatch_skill(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("list") | Some("skills") => command_skill_list(globals, args[1..].to_vec()),
        Some("enable") => command_skill_set_enabled(globals, args[1..].to_vec(), true),
        Some("disable") => command_skill_set_enabled(globals, args[1..].to_vec(), false),
        Some("remove") | Some("delete") => command_skill_remove(globals, args[1..].to_vec()),
        Some("plugins") => command_plugin_list(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero skills list|enable|disable|remove|plugins [--project-id ID]\nUses installed skill/plugin records in project app-data; discovery/root configuration remains on the shared skill service.",
            json!({ "command": "skills" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown skills command `{other}`. Use list, enable, disable, remove, or plugins."
        ))),
    }
}

pub(crate) fn dispatch_plugin(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("list") | Some("plugins") => command_plugin_list(globals, args[1..].to_vec()),
        Some("enable") => command_plugin_set_enabled(globals, args[1..].to_vec(), true),
        Some("disable") => command_plugin_set_enabled(globals, args[1..].to_vec(), false),
        Some("remove") | Some("delete") => command_plugin_remove(globals, args[1..].to_vec()),
        None => command_plugin_list(globals, Vec::new()),
        Some("--help") | Some("-h") => Ok(response(
            &globals,
            "Usage: xero plugins list|enable|disable|remove [--project-id ID]\nUses installed plugin records in project app-data; plugin root discovery remains on the shared skill service.",
            json!({ "command": "plugins" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown plugins command `{other}`. Use list, enable, disable, or remove."
        ))),
    }
}

fn command_skill_list(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero skills list [--project-id ID]",
            json!({ "command": "skills list" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    reject_skills_unknown_options(&args)?;
    let skills = load_skill_registry(&globals, &project_id)?;
    let text = if skills.is_empty() {
        "No installed skills recorded for the selected project.".into()
    } else {
        skills
            .iter()
            .map(|skill| {
                format!(
                    "{} [{}:{}] trust={} scope={}",
                    skill.name,
                    skill.skill_id,
                    skill.source_state,
                    skill.trust_state,
                    skill.scope_kind
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "skillRegistry",
            "projectId": project_id,
            "skills": skills
        }),
    ))
}

fn command_skill_set_enabled(
    globals: GlobalOptions,
    mut args: Vec<String>,
    enabled: bool,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero skills enable SOURCE_ID [--project-id ID] | xero skills disable SOURCE_ID [--project-id ID]",
            json!({ "command": "skills enable" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let source_id = take_option(&mut args, "--source-id")?
        .or_else(|| take_optional_positional(&mut args))
        .ok_or_else(|| CliError::usage("Missing skill source id."))?;
    reject_skills_unknown_options(&args)?;
    let connection = project_cli::project_connection(&globals, &project_id)?;
    ensure_table_exists(&connection, "installed_skill_records", "skill")?;
    let changed = connection
        .execute(
            r#"
            UPDATE installed_skill_records
            SET source_state = ?3,
                trust_state = CASE WHEN ?3 = 'enabled' AND trust_state = 'unknown' THEN 'trusted' ELSE trust_state END,
                updated_at = ?4
            WHERE source_id = ?1 AND (project_id IS NULL OR project_id = ?2)
            "#,
            params![
                source_id,
                project_id,
                if enabled { "enabled" } else { "disabled" },
                now_timestamp(),
            ],
        )
        .map_err(|error| sqlite_skills_error("skill_enable", error))?;
    if changed == 0 {
        return Err(CliError::user_fixable(
            "xero_cli_skill_source_unknown",
            format!("Installed skill source `{source_id}` was not found."),
        ));
    }
    let skills = load_skill_registry(&globals, &project_id)?;
    Ok(response(
        &globals,
        format!(
            "{} skill source `{source_id}`.",
            if enabled { "Enabled" } else { "Disabled" }
        ),
        json!({
            "kind": "skillRegistryMutation",
            "projectId": project_id,
            "sourceId": source_id,
            "enabled": enabled,
            "skills": skills
        }),
    ))
}

fn command_skill_remove(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero skills remove SOURCE_ID [--project-id ID]",
            json!({ "command": "skills remove" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let source_id = take_option(&mut args, "--source-id")?
        .or_else(|| take_optional_positional(&mut args))
        .ok_or_else(|| CliError::usage("Missing skill source id."))?;
    reject_skills_unknown_options(&args)?;
    let connection = project_cli::project_connection(&globals, &project_id)?;
    ensure_table_exists(&connection, "installed_skill_records", "skill")?;
    let changed = connection
        .execute(
            "DELETE FROM installed_skill_records WHERE source_id = ?1 AND (project_id IS NULL OR project_id = ?2)",
            params![source_id, project_id],
        )
        .map_err(|error| sqlite_skills_error("skill_remove", error))?;
    if changed == 0 {
        return Err(CliError::user_fixable(
            "xero_cli_skill_source_unknown",
            format!("Installed skill source `{source_id}` was not found."),
        ));
    }
    let skills = load_skill_registry(&globals, &project_id)?;
    Ok(response(
        &globals,
        format!("Removed skill source `{source_id}`."),
        json!({
            "kind": "skillRegistryMutation",
            "projectId": project_id,
            "sourceId": source_id,
            "removed": true,
            "skills": skills
        }),
    ))
}

fn command_plugin_list(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero skills plugins [--project-id ID]",
            json!({ "command": "skills plugins" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    reject_skills_unknown_options(&args)?;
    let plugins = load_plugin_registry(&globals, &project_id)?;
    let text = if plugins.is_empty() {
        "No installed plugins recorded for the selected project.".into()
    } else {
        plugins
            .iter()
            .map(|plugin| {
                format!(
                    "{}@{} [{}] trust={}",
                    plugin.name, plugin.version, plugin.plugin_state, plugin.trust_state
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "pluginRegistry",
            "projectId": project_id,
            "plugins": plugins
        }),
    ))
}

fn command_plugin_set_enabled(
    globals: GlobalOptions,
    mut args: Vec<String>,
    enabled: bool,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero plugins enable PLUGIN_ID [--project-id ID] | xero plugins disable PLUGIN_ID [--project-id ID]",
            json!({ "command": "plugins enable" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let plugin_id = take_option(&mut args, "--plugin-id")?
        .or_else(|| take_optional_positional(&mut args))
        .ok_or_else(|| CliError::usage("Missing plugin id."))?;
    reject_skills_unknown_options(&args)?;
    let connection = project_cli::project_connection(&globals, &project_id)?;
    ensure_table_exists(&connection, "installed_plugin_records", "plugin")?;
    let changed = connection
        .execute(
            r#"
            UPDATE installed_plugin_records
            SET plugin_state = ?2,
                trust_state = CASE WHEN ?2 = 'enabled' AND trust_state = 'unknown' THEN 'trusted' ELSE trust_state END,
                updated_at = ?3
            WHERE plugin_id = ?1
            "#,
            params![
                plugin_id,
                if enabled { "enabled" } else { "disabled" },
                now_timestamp(),
            ],
        )
        .map_err(|error| sqlite_skills_error("plugin_enable", error))?;
    if changed == 0 {
        return Err(CliError::user_fixable(
            "xero_cli_plugin_unknown",
            format!("Installed plugin `{plugin_id}` was not found."),
        ));
    }
    let plugins = load_plugin_registry(&globals, &project_id)?;
    Ok(response(
        &globals,
        format!(
            "{} plugin `{plugin_id}`.",
            if enabled { "Enabled" } else { "Disabled" }
        ),
        json!({
            "kind": "pluginRegistryMutation",
            "projectId": project_id,
            "pluginId": plugin_id,
            "enabled": enabled,
            "plugins": plugins
        }),
    ))
}

fn command_plugin_remove(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero plugins remove PLUGIN_ID [--project-id ID]",
            json!({ "command": "plugins remove" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let plugin_id = take_option(&mut args, "--plugin-id")?
        .or_else(|| take_optional_positional(&mut args))
        .ok_or_else(|| CliError::usage("Missing plugin id."))?;
    reject_skills_unknown_options(&args)?;
    let connection = project_cli::project_connection(&globals, &project_id)?;
    ensure_table_exists(&connection, "installed_plugin_records", "plugin")?;
    let changed = connection
        .execute(
            "DELETE FROM installed_plugin_records WHERE plugin_id = ?1",
            params![plugin_id],
        )
        .map_err(|error| sqlite_skills_error("plugin_remove", error))?;
    if changed == 0 {
        return Err(CliError::user_fixable(
            "xero_cli_plugin_unknown",
            format!("Installed plugin `{plugin_id}` was not found."),
        ));
    }
    let plugins = load_plugin_registry(&globals, &project_id)?;
    Ok(response(
        &globals,
        format!("Removed plugin `{plugin_id}`."),
        json!({
            "kind": "pluginRegistryMutation",
            "projectId": project_id,
            "pluginId": plugin_id,
            "removed": true,
            "plugins": plugins
        }),
    ))
}

pub(crate) fn load_skill_registry(
    globals: &GlobalOptions,
    project_id: &str,
) -> Result<Vec<SkillRegistryRow>, CliError> {
    let connection = project_cli::project_connection(globals, project_id)?;
    if !table_exists(&connection, "installed_skill_records")? {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare(
            r#"
            SELECT source_id, scope_kind, project_id, skill_id, name, source_state, trust_state,
                   user_invocable, updated_at
            FROM installed_skill_records
            WHERE project_id IS NULL OR project_id = ?1
            ORDER BY scope_kind ASC, name ASC, source_id ASC
            "#,
        )
        .map_err(|error| sqlite_skills_error("skill_prepare", error))?;
    let rows = statement
        .query_map(params![project_id], |row| {
            let user_invocable = row.get::<_, Option<i64>>(7)?;
            Ok(SkillRegistryRow {
                source_id: row.get(0)?,
                scope_kind: row.get(1)?,
                project_id: row.get(2)?,
                skill_id: row.get(3)?,
                name: row.get(4)?,
                source_state: row.get(5)?,
                trust_state: row.get(6)?,
                user_invocable: user_invocable.map(|value| value != 0),
                updated_at: row.get(8)?,
            })
        })
        .map_err(|error| sqlite_skills_error("skill_query", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlite_skills_error("skill_decode", error))
}

pub(crate) fn load_plugin_registry(
    globals: &GlobalOptions,
    project_id: &str,
) -> Result<Vec<PluginRegistryRow>, CliError> {
    let connection = project_cli::project_connection(globals, project_id)?;
    if !table_exists(&connection, "installed_plugin_records")? {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare(
            r#"
            SELECT plugin_id, name, version, plugin_state, trust_state, root_id, updated_at
            FROM installed_plugin_records
            ORDER BY plugin_state ASC, name ASC, plugin_id ASC
            "#,
        )
        .map_err(|error| sqlite_skills_error("plugin_prepare", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok(PluginRegistryRow {
                plugin_id: row.get(0)?,
                name: row.get(1)?,
                version: row.get(2)?,
                plugin_state: row.get(3)?,
                trust_state: row.get(4)?,
                root_id: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|error| sqlite_skills_error("plugin_query", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlite_skills_error("plugin_decode", error))
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, CliError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            params![table],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| sqlite_skills_error("table_probe", error))
}

fn ensure_table_exists(connection: &Connection, table: &str, label: &str) -> Result<(), CliError> {
    if table_exists(connection, table)? {
        Ok(())
    } else {
        Err(CliError::user_fixable(
            format!("xero_cli_{label}_registry_missing"),
            format!("No installed {label} registry exists for this project yet."),
        ))
    }
}

fn take_optional_positional(args: &mut Vec<String>) -> Option<String> {
    if args.first().is_some_and(|arg| !arg.starts_with('-')) {
        Some(args.remove(0))
    } else {
        None
    }
}

fn reject_skills_unknown_options(args: &[String]) -> Result<(), CliError> {
    if let Some(option) = args.iter().find(|arg| arg.starts_with('-')) {
        return Err(CliError::usage(format!("Unknown option `{option}`.")));
    }
    if let Some(extra) = args.first() {
        return Err(CliError::usage(format!("Unexpected argument `{extra}`.")));
    }
    Ok(())
}

fn sqlite_skills_error(operation: &str, error: rusqlite::Error) -> CliError {
    CliError::system_fault(
        "xero_cli_skills_sql_failed",
        format!("Skill/plugin registry {operation} failed: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn skills_and_plugins_read_installed_project_records() {
        let state_dir = temp_dir("skills-state");
        let repo = temp_dir("skills-repo");
        Command::new("git")
            .arg("init")
            .current_dir(&repo)
            .status()
            .expect("git init");
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
        seed_skill_and_plugin_rows(&state_dir, &project_id);

        let skills = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "skills",
            "list",
        ])
        .expect("skills list");
        assert_eq!(skills.json["kind"], json!("skillRegistry"));
        assert_eq!(skills.json["skills"][0]["skillId"], json!("skill-a"));

        let plugins = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "skills",
            "plugins",
        ])
        .expect("plugins list");
        assert_eq!(plugins.json["kind"], json!("pluginRegistry"));
        assert_eq!(plugins.json["plugins"][0]["pluginId"], json!("plugin-a"));
    }

    #[test]
    fn skills_and_plugins_enable_disable_remove_installed_records() {
        let state_dir = temp_dir("skills-mutate-state");
        let repo = temp_dir("skills-mutate-repo");
        Command::new("git")
            .arg("init")
            .current_dir(&repo)
            .status()
            .expect("git init");
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
        seed_skill_and_plugin_rows(&state_dir, &project_id);

        let disabled_skill = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "skills",
            "disable",
            "source-a",
        ])
        .expect("disable skill");
        assert_eq!(
            disabled_skill.json["skills"][0]["sourceState"],
            json!("disabled")
        );

        let disabled_plugin = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "plugins",
            "disable",
            "plugin-a",
        ])
        .expect("disable plugin");
        assert_eq!(
            disabled_plugin.json["plugins"][0]["pluginState"],
            json!("disabled")
        );

        let removed_skill = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "skills",
            "remove",
            "source-a",
        ])
        .expect("remove skill");
        assert!(removed_skill.json["skills"]
            .as_array()
            .expect("skills")
            .is_empty());

        let removed_plugin = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "plugins",
            "remove",
            "plugin-a",
        ])
        .expect("remove plugin");
        assert!(removed_plugin.json["plugins"]
            .as_array()
            .expect("plugins")
            .is_empty());
    }

    fn seed_skill_and_plugin_rows(state_dir: &std::path::Path, project_id: &str) {
        let database_path = state_dir.join("projects").join(project_id).join("state.db");
        let connection = Connection::open(database_path).expect("open project db");
        connection
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS installed_skill_records (
                    source_id TEXT PRIMARY KEY,
                    scope_kind TEXT NOT NULL,
                    project_id TEXT,
                    contract_version INTEGER NOT NULL,
                    skill_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    description TEXT NOT NULL,
                    user_invocable INTEGER,
                    source_state TEXT NOT NULL,
                    trust_state TEXT NOT NULL,
                    source_json TEXT NOT NULL,
                    cache_key TEXT,
                    local_location TEXT,
                    version_hash TEXT,
                    installed_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    last_used_at TEXT,
                    last_diagnostic_json TEXT
                );
                CREATE TABLE IF NOT EXISTS installed_plugin_records (
                    plugin_id TEXT PRIMARY KEY,
                    root_id TEXT NOT NULL,
                    root_path TEXT NOT NULL,
                    plugin_root_path TEXT NOT NULL,
                    manifest_path TEXT NOT NULL,
                    manifest_hash TEXT NOT NULL,
                    name TEXT NOT NULL,
                    version TEXT NOT NULL,
                    description TEXT NOT NULL,
                    plugin_state TEXT NOT NULL,
                    trust_state TEXT NOT NULL,
                    manifest_json TEXT NOT NULL,
                    installed_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    last_reloaded_at TEXT,
                    last_diagnostic_json TEXT
                );
                "#,
            )
            .expect("create tables");
        connection
            .execute(
                r#"
                INSERT INTO installed_skill_records (
                    source_id, scope_kind, project_id, contract_version, skill_id, name,
                    description, user_invocable, source_state, trust_state, source_json,
                    local_location, installed_at, updated_at
                )
                VALUES (?1, 'project', ?2, 1, 'skill-a', 'Skill A', 'A test skill', 1,
                    'enabled', 'trusted', '{}', '/tmp/skill-a', '2026-05-15T00:00:00Z',
                    '2026-05-15T00:00:01Z')
                "#,
                params!["source-a", project_id],
            )
            .expect("insert skill");
        connection
            .execute(
                r#"
                INSERT INTO installed_plugin_records (
                    plugin_id, root_id, root_path, plugin_root_path, manifest_path,
                    manifest_hash, name, version, description, plugin_state, trust_state,
                    manifest_json, installed_at, updated_at
                )
                VALUES (
                    'plugin-a', 'root-a', '/tmp/plugins', '/tmp/plugins/plugin-a',
                    '/tmp/plugins/plugin-a/.codex-plugin/plugin.json',
                    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                    'Plugin A', '1.0.0', 'A test plugin', 'enabled', 'trusted', '{}',
                    '2026-05-15T00:00:00Z', '2026-05-15T00:00:01Z'
                )
                "#,
                [],
            )
            .expect("insert plugin");
    }

    fn selected_project_id(state_dir: &std::path::Path) -> String {
        let path = state_dir.join("tui-settings.json");
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path).expect("settings")).expect("json");
        value["selectedProjectId"]
            .as_str()
            .expect("selected project")
            .to_string()
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("xero-cli-{prefix}-{nanos}"));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }
}
