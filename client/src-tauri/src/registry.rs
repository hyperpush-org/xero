use std::{fs, io::Write, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::NamedTempFile;

use crate::{commands::CommandError, state::ImportFailpoints};

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

impl Default for ProjectRegistry {
    fn default() -> Self {
        Self {
            version: 1,
            projects: Vec::new(),
        }
    }
}

pub fn read_registry(path: &Path) -> Result<ProjectRegistry, CommandError> {
    if !path.exists() {
        return Ok(ProjectRegistry::default());
    }

    let contents = fs::read_to_string(path).map_err(|error| {
        CommandError::retryable(
            "registry_read_failed",
            format!(
                "Cadence could not read the desktop project registry at {}: {error}",
                path.display()
            ),
        )
    })?;

    match serde_json::from_str::<Value>(&contents) {
        Ok(value) => parse_registry_value(path, value),
        Err(_) => recover_malformed_registry(path),
    }
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

    let mut registry = read_registry(path)?;
    registry.projects.retain(|project| {
        project.project_id != entry.project_id && project.root_path != entry.root_path
    });
    registry.projects.push(entry);
    normalize_projects(&mut registry.projects);

    write_registry(path, &registry)?;
    Ok(registry)
}

pub fn replace_projects(
    path: &Path,
    projects: Vec<RegistryProjectRecord>,
) -> Result<ProjectRegistry, CommandError> {
    let mut registry = ProjectRegistry {
        version: 1,
        projects,
    };
    normalize_projects(&mut registry.projects);
    write_registry(path, &registry)?;
    Ok(registry)
}

fn parse_registry_value(path: &Path, value: Value) -> Result<ProjectRegistry, CommandError> {
    let Some(object) = value.as_object() else {
        return recover_malformed_registry(path);
    };

    let version = object
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(1);

    let Some(project_values) = object.get("projects").and_then(Value::as_array) else {
        return recover_malformed_registry(path);
    };

    let mut projects = project_values
        .iter()
        .filter_map(|value| serde_json::from_value::<RegistryProjectRecord>(value.clone()).ok())
        .collect::<Vec<_>>();
    normalize_projects(&mut projects);

    Ok(ProjectRegistry { version, projects })
}

fn write_registry(path: &Path, registry: &ProjectRegistry) -> Result<(), CommandError> {
    let parent = path.parent().ok_or_else(|| {
        CommandError::system_fault(
            "registry_parent_missing",
            format!(
                "Cadence could not determine the registry directory for {}.",
                path.display()
            ),
        )
    })?;

    fs::create_dir_all(parent).map_err(|error| {
        CommandError::retryable(
            "registry_directory_unavailable",
            format!(
                "Cadence could not prepare the desktop registry directory at {}: {error}",
                parent.display()
            ),
        )
    })?;

    let json = serde_json::to_vec_pretty(registry).map_err(|error| {
        CommandError::system_fault(
            "registry_serialize_failed",
            format!("Cadence could not serialize the desktop registry: {error}"),
        )
    })?;

    let mut temp_file = NamedTempFile::new_in(parent).map_err(|error| {
        CommandError::retryable(
            "registry_tempfile_failed",
            format!("Cadence could not stage the desktop registry update: {error}"),
        )
    })?;

    temp_file.write_all(&json).map_err(|error| {
        CommandError::retryable(
            "registry_write_failed",
            format!("Cadence could not write the desktop registry update: {error}"),
        )
    })?;
    temp_file.flush().map_err(|error| {
        CommandError::retryable(
            "registry_write_failed",
            format!("Cadence could not flush the desktop registry update: {error}"),
        )
    })?;

    temp_file.persist(path).map_err(|error| {
        CommandError::retryable(
            "registry_write_failed",
            format!(
                "Cadence could not atomically persist the desktop registry at {}: {}",
                path.display(),
                error.error
            ),
        )
    })?;

    Ok(())
}

fn normalize_projects(projects: &mut Vec<RegistryProjectRecord>) {
    projects.sort_by(|left, right| left.root_path.cmp(&right.root_path));
}

fn recover_malformed_registry(path: &Path) -> Result<ProjectRegistry, CommandError> {
    let backup_path = path.with_extension("json.corrupt");

    if backup_path.exists() {
        fs::remove_file(&backup_path).map_err(|error| {
            CommandError::retryable(
                "registry_recovery_failed",
                format!(
                    "Cadence could not clear the previous corrupt-registry backup at {}: {error}",
                    backup_path.display()
                ),
            )
        })?;
    }

    fs::rename(path, &backup_path).map_err(|error| {
        CommandError::retryable(
            "registry_recovery_failed",
            format!(
                "Cadence could not quarantine the malformed desktop registry at {}: {error}",
                path.display()
            ),
        )
    })?;

    Ok(ProjectRegistry::default())
}
