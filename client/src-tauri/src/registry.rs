use std::path::Path;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::{
    commands::{CommandError, ProjectOriginDto, ProjectSummaryDto, RepositorySummaryDto},
    global_db::open_global_database,
    state::ImportFailpoints,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryProjectRecord {
    pub project_id: String,
    pub repository_id: String,
    pub root_path: String,
    #[serde(default = "default_is_git_repo")]
    pub is_git_repo: bool,
}

impl RegistryProjectRecord {
    pub fn repository_summary(&self) -> RepositorySummaryDto {
        RepositorySummaryDto {
            id: self.repository_id.clone(),
            project_id: self.project_id.clone(),
            root_path: self.root_path.clone(),
            display_name: derive_display_name(&self.root_path),
            branch: None,
            head_sha: None,
            is_git_repo: self.is_git_repo,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectRegistry {
    pub version: u32,
    pub projects: Vec<RegistryProjectRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryProjectSummaryRecord {
    pub registry: RegistryProjectRecord,
    pub project: ProjectSummaryDto,
}

impl Default for ProjectRegistry {
    fn default() -> Self {
        Self {
            version: 1,
            projects: Vec::new(),
        }
    }
}

const REGISTRY_VERSION: u32 = 1;

fn default_is_git_repo() -> bool {
    true
}

pub fn read_registry(path: &Path) -> Result<ProjectRegistry, CommandError> {
    let connection = open_global_database(path)?;

    let mut stmt = connection
        .prepare(
            "SELECT repositories.id, repositories.project_id, repositories.root_path, repositories.is_git_repo \
             FROM repositories JOIN projects ON projects.id = repositories.project_id \
             ORDER BY repositories.root_path",
        )
        .map_err(|error| {
            CommandError::retryable(
                "registry_read_failed",
                format!("Xero could not prepare the desktop registry read: {error}"),
            )
        })?;

    let rows = stmt
        .query_map([], |row| {
            Ok(RegistryProjectRecord {
                repository_id: row.get(0)?,
                project_id: row.get(1)?,
                root_path: row.get(2)?,
                is_git_repo: row.get::<_, i64>(3)? == 1,
            })
        })
        .map_err(|error| {
            CommandError::retryable(
                "registry_read_failed",
                format!("Xero could not read the desktop project registry: {error}"),
            )
        })?;

    let mut projects = Vec::new();
    for row in rows {
        projects.push(row.map_err(|error| {
            CommandError::retryable(
                "registry_read_failed",
                format!("Xero could not decode a desktop project registry row: {error}"),
            )
        })?);
    }

    Ok(ProjectRegistry {
        version: REGISTRY_VERSION,
        projects,
    })
}

pub fn read_project_summaries(
    path: &Path,
) -> Result<Vec<RegistryProjectSummaryRecord>, CommandError> {
    let connection = open_global_database(path)?;

    let mut stmt = connection
        .prepare(
            r#"
            SELECT
                repositories.id,
                repositories.project_id,
                repositories.root_path,
                repositories.is_git_repo,
                projects.name,
                projects.description,
                projects.milestone,
                projects.total_phases,
                projects.completed_phases,
                projects.active_phase,
                projects.branch,
                projects.runtime,
                projects.start_targets
            FROM repositories
            JOIN projects ON projects.id = repositories.project_id
            ORDER BY repositories.root_path
            "#,
        )
        .map_err(|error| {
            CommandError::retryable(
                "registry_read_failed",
                format!("Xero could not prepare the desktop project summary read: {error}"),
            )
        })?;

    let rows = stmt
        .query_map([], |row| {
            Ok(RegistryProjectSummaryRecord {
                registry: RegistryProjectRecord {
                    repository_id: row.get(0)?,
                    project_id: row.get(1)?,
                    root_path: row.get(2)?,
                    is_git_repo: row.get::<_, i64>(3)? == 1,
                },
                project: ProjectSummaryDto {
                    id: row.get(1)?,
                    name: row.get(4)?,
                    description: row.get(5)?,
                    milestone: row.get(6)?,
                    project_origin: ProjectOriginDto::Unknown,
                    total_phases: row.get::<_, u32>(7)?,
                    completed_phases: row.get::<_, u32>(8)?,
                    active_phase: row.get::<_, u32>(9)?,
                    branch: row.get(10)?,
                    runtime: row.get(11)?,
                    start_targets: serde_json::from_str(&row.get::<_, String>(12)?)
                        .unwrap_or_default(),
                },
            })
        })
        .map_err(|error| {
            CommandError::retryable(
                "registry_read_failed",
                format!("Xero could not read the desktop project summaries: {error}"),
            )
        })?;

    let mut projects = Vec::new();
    for row in rows {
        projects.push(row.map_err(|error| {
            CommandError::retryable(
                "registry_read_failed",
                format!("Xero could not decode a desktop project summary row: {error}"),
            )
        })?);
    }

    Ok(projects)
}

pub fn read_project_records(
    path: &Path,
    project_id: &str,
) -> Result<Vec<RegistryProjectRecord>, CommandError> {
    let connection = open_global_database(path)?;

    let mut stmt = connection
        .prepare(
            "SELECT repositories.id, repositories.project_id, repositories.root_path, repositories.is_git_repo \
             FROM repositories JOIN projects ON projects.id = repositories.project_id \
             WHERE repositories.project_id = ?1 \
             ORDER BY repositories.root_path",
        )
        .map_err(|error| {
            CommandError::retryable(
                "registry_read_failed",
                format!("Xero could not prepare the desktop project registry lookup: {error}"),
            )
        })?;

    let rows = stmt
        .query_map(params![project_id], |row| {
            Ok(RegistryProjectRecord {
                repository_id: row.get(0)?,
                project_id: row.get(1)?,
                root_path: row.get(2)?,
                is_git_repo: row.get::<_, i64>(3)? == 1,
            })
        })
        .map_err(|error| {
            CommandError::retryable(
                "registry_read_failed",
                format!("Xero could not read the desktop project registry lookup: {error}"),
            )
        })?;

    let mut projects = Vec::new();
    for row in rows {
        projects.push(row.map_err(|error| {
            CommandError::retryable(
                "registry_read_failed",
                format!("Xero could not decode a desktop project registry lookup row: {error}"),
            )
        })?);
    }

    Ok(projects)
}

pub fn upsert_project(
    path: &Path,
    entry: RegistryProjectRecord,
    failpoints: &ImportFailpoints,
) -> Result<ProjectRegistry, CommandError> {
    if failpoints.fail_registry_write {
        return Err(CommandError::retryable(
            "registry_write_failed",
            "Test failpoint forced the desktop project registry update to fail.",
        ));
    }

    let mut connection = open_global_database(path)?;
    let tx = connection.transaction().map_err(|error| {
        CommandError::retryable(
            "registry_write_failed",
            format!("Xero could not begin the registry transaction: {error}"),
        )
    })?;

    let display_name = derive_display_name(&entry.root_path);

    tx.execute(
        "INSERT INTO projects (id, name) VALUES (?1, ?2)
         ON CONFLICT(id) DO UPDATE SET
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![entry.project_id, display_name],
    )
    .map_err(map_write_error)?;

    // Drop any other repository row pinned to this canonical path so the new entry wins, then
    // upsert the repository associated with this project.
    tx.execute(
        "DELETE FROM repositories WHERE root_path = ?1 AND id != ?2",
        params![entry.root_path, entry.repository_id],
    )
    .map_err(map_write_error)?;
    tx.execute(
        "INSERT INTO repositories (id, project_id, root_path, display_name, is_git_repo)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            project_id = excluded.project_id,
            root_path = excluded.root_path,
            display_name = excluded.display_name,
            is_git_repo = excluded.is_git_repo,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![
            entry.repository_id,
            entry.project_id,
            entry.root_path,
            display_name,
            if entry.is_git_repo { 1 } else { 0 },
        ],
    )
    .map_err(map_write_error)?;

    tx.commit().map_err(|error| {
        CommandError::retryable(
            "registry_write_failed",
            format!("Xero could not commit the desktop registry update: {error}"),
        )
    })?;

    drop(connection);
    read_registry(path)
}

pub fn replace_projects(
    path: &Path,
    projects: Vec<RegistryProjectRecord>,
) -> Result<ProjectRegistry, CommandError> {
    let mut connection = open_global_database(path)?;
    let tx = connection.transaction().map_err(|error| {
        CommandError::retryable(
            "registry_write_failed",
            format!("Xero could not begin the registry transaction: {error}"),
        )
    })?;

    tx.execute("DELETE FROM repositories", [])
        .map_err(map_write_error)?;
    tx.execute("DELETE FROM projects", [])
        .map_err(map_write_error)?;

    for entry in &projects {
        let display_name = derive_display_name(&entry.root_path);
        tx.execute(
            "INSERT INTO projects (id, name) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![entry.project_id, display_name],
        )
        .map_err(map_write_error)?;
        tx.execute(
            "INSERT INTO repositories (id, project_id, root_path, display_name, is_git_repo)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.repository_id,
                entry.project_id,
                entry.root_path,
                display_name,
                if entry.is_git_repo { 1 } else { 0 },
            ],
        )
        .map_err(map_write_error)?;
    }

    tx.commit().map_err(|error| {
        CommandError::retryable(
            "registry_write_failed",
            format!("Xero could not commit the desktop registry update: {error}"),
        )
    })?;

    drop(connection);
    read_registry(path)
}

fn derive_display_name(root_path: &str) -> String {
    Path::new(root_path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| root_path.to_owned())
}

fn map_write_error(error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        "registry_write_failed",
        format!("Xero could not write the desktop registry: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_record(
        project_id: &str,
        repository_id: &str,
        root_path: impl Into<String>,
    ) -> RegistryProjectRecord {
        RegistryProjectRecord {
            project_id: project_id.into(),
            repository_id: repository_id.into(),
            root_path: root_path.into(),
            is_git_repo: true,
        }
    }

    #[test]
    fn read_project_records_filters_before_callers_touch_project_paths() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let registry_path = tempdir.path().join("xero.db");
        let target_root = tempdir.path().join("target");
        std::fs::create_dir_all(&target_root).expect("target root");

        replace_projects(
            &registry_path,
            vec![
                registry_record(
                    "project-stale-a",
                    "repo-stale-a",
                    tempdir.path().join("stale-a").display().to_string(),
                ),
                registry_record(
                    "project-target",
                    "repo-target",
                    target_root.display().to_string(),
                ),
                registry_record(
                    "project-stale-b",
                    "repo-stale-b",
                    tempdir.path().join("stale-b").display().to_string(),
                ),
            ],
        )
        .expect("seed registry");

        let all_records = read_registry(&registry_path).expect("read all records");
        assert_eq!(all_records.projects.len(), 3);

        let target_records =
            read_project_records(&registry_path, "project-target").expect("read target records");
        assert_eq!(
            target_records,
            vec![registry_record(
                "project-target",
                "repo-target",
                target_root.display().to_string(),
            )],
            "project-specific command lookup should not require walking unrelated registry roots",
        );
    }

    #[test]
    fn read_project_records_returns_empty_for_unknown_project() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let registry_path = tempdir.path().join("xero.db");
        replace_projects(
            &registry_path,
            vec![registry_record(
                "project-known",
                "repo-known",
                tempdir.path().join("known").display().to_string(),
            )],
        )
        .expect("seed registry");

        let target_records =
            read_project_records(&registry_path, "project-missing").expect("read target records");
        assert!(target_records.is_empty());
    }

    #[test]
    fn registry_preserves_non_git_project_records() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let registry_path = tempdir.path().join("xero.db");
        let mut record = registry_record(
            "project-plain-folder",
            "repo-plain-folder",
            tempdir.path().join("plain-folder").display().to_string(),
        );
        record.is_git_repo = false;

        replace_projects(&registry_path, vec![record.clone()]).expect("seed registry");

        let all_records = read_registry(&registry_path).expect("read all records");
        assert_eq!(all_records.projects, vec![record.clone()]);

        let project_records =
            read_project_records(&registry_path, "project-plain-folder").expect("read project");
        assert_eq!(project_records, vec![record]);
    }
}
