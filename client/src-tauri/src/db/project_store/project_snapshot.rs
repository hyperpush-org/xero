use std::path::{Path, PathBuf};

use rusqlite::{Connection, Error as SqlError};

use crate::{
    commands::{
        CommandError, PhaseSummaryDto, ProjectOriginDto, ProjectSnapshotResponseDto,
        ProjectSummaryDto, RepositorySummaryDto, RuntimeAgentIdDto,
    },
    db::database_path_for_repo,
};

use super::{
    agent_session::{read_agent_sessions_with_connection, AgentSessionRecord},
    map_project_query_error, open_project_database, read_operator_approvals, read_resume_history,
    read_verification_records, ProjectSummaryRow,
};

#[derive(Debug, Clone)]
pub struct ProjectSnapshotRecord {
    pub snapshot: ProjectSnapshotResponseDto,
    pub database_path: PathBuf,
}

#[derive(Debug)]
struct ProjectProjection {
    project: ProjectSummaryDto,
    phases: Vec<PhaseSummaryDto>,
}

pub fn load_project_summary(
    repo_root: &Path,
    expected_project_id: &str,
) -> Result<ProjectSummaryDto, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;

    read_project_projection(&connection, &database_path, repo_root, expected_project_id)
        .map(|projection| projection.project)
}

pub fn load_project_snapshot(
    repo_root: &Path,
    expected_project_id: &str,
) -> Result<ProjectSnapshotRecord, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_snapshot_with_connection(
        &connection,
        &database_path,
        repo_root,
        expected_project_id,
    )
}

pub fn load_project_snapshot_and_agent_sessions(
    repo_root: &Path,
    expected_project_id: &str,
    include_archived_agent_sessions: bool,
) -> Result<(ProjectSnapshotRecord, Vec<AgentSessionRecord>), CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    let snapshot = read_project_snapshot_with_connection(
        &connection,
        &database_path,
        repo_root,
        expected_project_id,
    )?;
    let agent_sessions = read_agent_sessions_with_connection(
        &connection,
        &database_path,
        expected_project_id,
        include_archived_agent_sessions,
    )?;
    Ok((snapshot, agent_sessions))
}

fn read_project_snapshot_with_connection(
    connection: &Connection,
    database_path: &Path,
    repo_root: &Path,
    expected_project_id: &str,
) -> Result<ProjectSnapshotRecord, CommandError> {
    let projection =
        read_project_projection(connection, database_path, repo_root, expected_project_id)?;
    let repository = read_repository_summary(connection, database_path, expected_project_id)?;
    let approval_requests =
        read_operator_approvals(connection, database_path, expected_project_id)?;
    let verification_records =
        read_verification_records(connection, database_path, expected_project_id)?;
    let resume_history = read_resume_history(connection, database_path, expected_project_id)?;
    Ok(ProjectSnapshotRecord {
        snapshot: ProjectSnapshotResponseDto {
            project: projection.project,
            repository,
            phases: projection.phases,
            approval_requests,
            verification_records,
            resume_history,
            agent_sessions: Vec::new(),
            autonomous_run: None,
        },
        database_path: database_path.to_path_buf(),
    })
}

fn read_project_projection(
    connection: &Connection,
    database_path: &Path,
    repo_root: &Path,
    expected_project_id: &str,
) -> Result<ProjectProjection, CommandError> {
    let project_row = read_project_row(connection, database_path, repo_root, expected_project_id)?;
    let phases = read_phase_summaries(connection, database_path, expected_project_id)?;

    Ok(ProjectProjection {
        project: derive_project_summary(project_row, &phases),
        phases,
    })
}

fn derive_project_summary(
    project_row: ProjectSummaryRow,
    _phases: &[PhaseSummaryDto],
) -> ProjectSummaryDto {
    ProjectSummaryDto {
        id: project_row.id,
        name: project_row.name,
        description: project_row.description,
        milestone: project_row.milestone,
        project_origin: project_row.project_origin,
        total_phases: 0,
        completed_phases: 0,
        active_phase: 0,
        branch: project_row.branch,
        runtime: project_row.runtime,
        start_targets: project_row.start_targets,
    }
}

fn read_repository_summary(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Option<RepositorySummaryDto>, CommandError> {
    connection
        .query_row(
            r#"
            SELECT
                id,
                project_id,
                root_path,
                display_name,
                branch,
                head_sha,
                is_git_repo
            FROM repositories
            WHERE project_id = ?1
            ORDER BY updated_at DESC, created_at DESC
            LIMIT 1
            "#,
            [expected_project_id],
            |row| {
                Ok(RepositorySummaryDto {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    root_path: row.get(2)?,
                    display_name: row.get(3)?,
                    branch: row.get(4)?,
                    head_sha: row.get(5)?,
                    is_git_repo: row.get::<_, i64>(6)? == 1,
                })
            },
        )
        .map(Some)
        .or_else(|error| match error {
            SqlError::QueryReturnedNoRows => Ok(None),
            other => Err(CommandError::system_fault(
                "project_repository_query_failed",
                format!(
                    "Xero could not read repository metadata from {}: {other}",
                    database_path.display()
                ),
            )),
        })
}

pub(crate) fn read_phase_summaries(
    _connection: &Connection,
    _database_path: &Path,
    _expected_project_id: &str,
) -> Result<Vec<PhaseSummaryDto>, CommandError> {
    Ok(Vec::new())
}

pub(crate) fn read_project_row(
    connection: &Connection,
    database_path: &Path,
    repo_root: &Path,
    expected_project_id: &str,
) -> Result<ProjectSummaryRow, CommandError> {
    connection
        .query_row(
            r#"
            SELECT
                id,
                name,
                description,
                milestone,
                project_origin,
                branch,
                runtime,
                start_targets
            FROM projects
            WHERE id = ?1
            "#,
            [expected_project_id],
            |row| {
                let raw_targets: String = row.get(7)?;
                Ok(ProjectSummaryRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    milestone: row.get(3)?,
                    project_origin: parse_project_origin(row.get::<_, String>(4)?.as_str()),
                    branch: row.get(5)?,
                    runtime: row.get(6)?,
                    start_targets: serde_json::from_str(&raw_targets).unwrap_or_default(),
                })
            },
        )
        .map_err(|error| {
            map_project_query_error(error, database_path, repo_root, expected_project_id)
        })
}

pub fn load_project_origin(
    repo_root: &Path,
    expected_project_id: &str,
) -> Result<ProjectOriginDto, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, expected_project_id)
        .map(|row| row.project_origin)
}

pub fn ensure_runtime_agent_allowed_for_project(
    repo_root: &Path,
    expected_project_id: &str,
    runtime_agent_id: RuntimeAgentIdDto,
) -> Result<(), CommandError> {
    if runtime_agent_id != RuntimeAgentIdDto::Crawl {
        return Ok(());
    }

    match load_project_origin(repo_root, expected_project_id)? {
        ProjectOriginDto::Brownfield => Ok(()),
        ProjectOriginDto::Greenfield => Err(CommandError::user_fixable(
            "runtime_agent_crawl_unavailable_greenfield",
            "Crawl is only available for projects imported with Open existing. This project was created as new, so use Ask, Engineer, or Debug instead.",
        )),
        ProjectOriginDto::Unknown => Err(CommandError::user_fixable(
            "runtime_agent_crawl_unavailable_unknown_origin",
            "Crawl is only available after Xero knows the project came from Open existing. Re-import the existing repository or choose Ask, Engineer, or Debug.",
        )),
    }
}

fn parse_project_origin(value: &str) -> ProjectOriginDto {
    match value {
        "brownfield" => ProjectOriginDto::Brownfield,
        "greenfield" => ProjectOriginDto::Greenfield,
        _ => ProjectOriginDto::Unknown,
    }
}
