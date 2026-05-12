use std::path::Path;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::{
    commands::{CommandError, ProjectOriginDto, ProjectSummaryDto},
    global_db::open_global_database,
    state::ImportFailpoints,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryProjectRecord {
    pub project_id: String,
    pub repository_id: String,
    pub root_path: String,
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

pub fn read_registry(path: &Path) -> Result<ProjectRegistry, CommandError> {
    let connection = open_global_database(path)?;

    let mut stmt = connection
        .prepare(
            "SELECT repositories.id, repositories.project_id, repositories.root_path \
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
                },
                project: ProjectSummaryDto {
                    id: row.get(1)?,
                    name: row.get(3)?,
                    description: row.get(4)?,
                    milestone: row.get(5)?,
                    project_origin: ProjectOriginDto::Unknown,
                    total_phases: row.get::<_, u32>(6)?,
                    completed_phases: row.get::<_, u32>(7)?,
                    active_phase: row.get::<_, u32>(8)?,
                    branch: row.get(9)?,
                    runtime: row.get(10)?,
                    start_targets: serde_json::from_str(&row.get::<_, String>(11)?).unwrap_or_default(),
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
            "SELECT repositories.id, repositories.project_id, repositories.root_path \
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
        "INSERT INTO repositories (id, project_id, root_path, display_name)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
            project_id = excluded.project_id,
            root_path = excluded.root_path,
            display_name = excluded.display_name,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        params![
            entry.repository_id,
            entry.project_id,
            entry.root_path,
            display_name,
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
            "INSERT INTO repositories (id, project_id, root_path, display_name)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                entry.repository_id,
                entry.project_id,
                entry.root_path,
                display_name,
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
}
