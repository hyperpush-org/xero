use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::json;

use super::{project_cli, response, take_help, take_option, CliError, CliResponse, GlobalOptions};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSummaryRow {
    project_id: String,
    provider_id: String,
    model_id: String,
    run_count: i64,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    cache_read_tokens: i64,
    cache_creation_tokens: i64,
    estimated_cost_micros: i64,
    latest_updated_at: Option<String>,
}

pub(crate) fn dispatch_usage(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("summary") => command_usage_summary(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero usage summary [--project-id ID]\nRead-only usage/cost totals from the project agent_usage table.",
            json!({ "command": "usage" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown usage command `{other}`. Use summary."
        ))),
    }
}

fn command_usage_summary(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero usage summary [--project-id ID]",
            json!({ "command": "usage summary" }),
        ));
    }
    let provider_id = take_option(&mut args, "--provider")?;
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    reject_usage_unknown_options(&args)?;
    let rows = load_usage_summary(&globals, &project_id, provider_id.as_deref())?;
    let text = if rows.is_empty() {
        "No usage rows recorded.".into()
    } else {
        rows.iter()
            .map(|row| {
                format!(
                    "{}/{} runs={} tokens={} cost_micros={}",
                    row.provider_id,
                    row.model_id,
                    row.run_count,
                    row.total_tokens,
                    row.estimated_cost_micros
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "usageSummary",
            "projectId": project_id,
            "providerId": provider_id,
            "rows": rows
        }),
    ))
}

fn load_usage_summary(
    globals: &GlobalOptions,
    project_id: &str,
    provider_id: Option<&str>,
) -> Result<Vec<UsageSummaryRow>, CliError> {
    let connection = project_cli::project_connection(globals, project_id)?;
    if !table_exists(&connection, "agent_usage")? {
        return Ok(Vec::new());
    }
    let sql = if provider_id.is_some() {
        r#"
            SELECT project_id, provider_id, model_id, COUNT(*),
                   COALESCE(SUM(input_tokens), 0),
                   COALESCE(SUM(output_tokens), 0),
                   COALESCE(SUM(total_tokens), 0),
                   COALESCE(SUM(cache_read_tokens), 0),
                   COALESCE(SUM(cache_creation_tokens), 0),
                   COALESCE(SUM(estimated_cost_micros), 0),
                   MAX(updated_at)
            FROM agent_usage
            WHERE project_id = ?1 AND provider_id = ?2
            GROUP BY project_id, provider_id, model_id
            ORDER BY estimated_cost_micros DESC, total_tokens DESC, provider_id ASC, model_id ASC
            "#
    } else {
        r#"
            SELECT project_id, provider_id, model_id, COUNT(*),
                   COALESCE(SUM(input_tokens), 0),
                   COALESCE(SUM(output_tokens), 0),
                   COALESCE(SUM(total_tokens), 0),
                   COALESCE(SUM(cache_read_tokens), 0),
                   COALESCE(SUM(cache_creation_tokens), 0),
                   COALESCE(SUM(estimated_cost_micros), 0),
                   MAX(updated_at)
            FROM agent_usage
            WHERE project_id = ?1
            GROUP BY project_id, provider_id, model_id
            ORDER BY estimated_cost_micros DESC, total_tokens DESC, provider_id ASC, model_id ASC
            "#
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| sqlite_usage_error("summary_prepare", error))?;
    if let Some(provider_id) = provider_id {
        let rows = statement
            .query_map(params![project_id, provider_id], usage_summary_row)
            .map_err(|error| sqlite_usage_error("summary_query", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| sqlite_usage_error("summary_decode", error))
    } else {
        let rows = statement
            .query_map(params![project_id], usage_summary_row)
            .map_err(|error| sqlite_usage_error("summary_query", error))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| sqlite_usage_error("summary_decode", error))
    }
}

fn usage_summary_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UsageSummaryRow> {
    Ok(UsageSummaryRow {
        project_id: row.get(0)?,
        provider_id: row.get(1)?,
        model_id: row.get(2)?,
        run_count: row.get(3)?,
        input_tokens: row.get(4)?,
        output_tokens: row.get(5)?,
        total_tokens: row.get(6)?,
        cache_read_tokens: row.get(7)?,
        cache_creation_tokens: row.get(8)?,
        estimated_cost_micros: row.get(9)?,
        latest_updated_at: row.get(10)?,
    })
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, CliError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            params![table],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| sqlite_usage_error("table_probe", error))
}

fn reject_usage_unknown_options(args: &[String]) -> Result<(), CliError> {
    if let Some(option) = args.iter().find(|arg| arg.starts_with('-')) {
        return Err(CliError::usage(format!("Unknown option `{option}`.")));
    }
    if let Some(extra) = args.first() {
        return Err(CliError::usage(format!("Unexpected argument `{extra}`.")));
    }
    Ok(())
}

fn sqlite_usage_error(operation: &str, error: rusqlite::Error) -> CliError {
    CliError::system_fault(
        "xero_cli_usage_sql_failed",
        format!("Usage {operation} failed: {error}"),
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
    fn usage_summary_reads_agent_usage_totals() {
        let state_dir = temp_dir("usage-state");
        let repo = temp_dir("usage-repo");
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
        seed_usage_rows(&state_dir, &project_id);
        let output = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "usage",
            "summary",
        ])
        .expect("usage summary");
        assert_eq!(output.json["kind"], json!("usageSummary"));
        assert_eq!(output.json["rows"][0]["totalTokens"], json!(30));
        assert_eq!(output.json["rows"][0]["estimatedCostMicros"], json!(42));
    }

    fn seed_usage_rows(state_dir: &Path, project_id: &str) {
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
                CREATE TABLE IF NOT EXISTS agent_usage (
                    project_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    agent_definition_id TEXT NOT NULL,
                    agent_definition_version INTEGER NOT NULL,
                    provider_id TEXT NOT NULL,
                    model_id TEXT NOT NULL,
                    input_tokens INTEGER NOT NULL DEFAULT 0,
                    output_tokens INTEGER NOT NULL DEFAULT 0,
                    total_tokens INTEGER NOT NULL DEFAULT 0,
                    estimated_cost_micros INTEGER NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL,
                    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                    cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (project_id, run_id)
                );
                "#,
            )
            .expect("create usage table");
        connection
            .execute(
                r#"
                INSERT INTO agent_usage (
                    project_id, run_id, agent_definition_id,
                    agent_definition_version, provider_id, model_id,
                    input_tokens, output_tokens, total_tokens,
                    estimated_cost_micros, updated_at
                ) VALUES (?1, 'run-1', 'engineer', 1, 'openai_api',
                    'gpt-test', 10, 20, 30, 42, '2026-05-15T00:00:00Z')
                "#,
                params![project_id],
            )
            .expect("insert usage");
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
