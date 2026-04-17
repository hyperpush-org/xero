use std::path::Path;

use crate::{
    commands::{CommandError, CommandResult, RepositoryStatusResponseDto},
    git::repository::{resolve_repository, CanonicalRepository},
    registry::{self, RegistryProjectRecord},
};

pub fn load_repository_status(
    expected_project_id: &str,
    registry_path: &Path,
) -> CommandResult<RepositoryStatusResponseDto> {
    let repository = resolve_project_repository(expected_project_id, registry_path)?;
    Ok(repository.repository_status())
}

pub fn resolve_project_repository(
    expected_project_id: &str,
    registry_path: &Path,
) -> Result<CanonicalRepository, CommandError> {
    let candidates = lookup_registry_candidates(expected_project_id, registry_path)?;
    let mut first_error: Option<CommandError> = None;

    for RegistryProjectRecord {
        project_id,
        repository_id,
        root_path,
    } in candidates
    {
        match resolve_repository(&root_path) {
            Ok(repository) => {
                if repository.project_id != project_id || repository.repository_id != repository_id
                {
                    return Err(CommandError::system_fault(
                        "project_registry_mismatch",
                        format!(
                            "Registry entry for project `{project_id}` no longer matches the repository discovered at {root_path}."
                        ),
                    ));
                }

                return Ok(repository);
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(CommandError::project_not_found))
}

pub fn lookup_registry_candidates(
    expected_project_id: &str,
    registry_path: &Path,
) -> Result<Vec<RegistryProjectRecord>, CommandError> {
    let registry = registry::read_registry(registry_path)?;
    let mut live_root_records = Vec::new();
    let mut snapshot_candidates = Vec::new();
    let mut pruned_stale_roots = false;

    for record in registry.projects {
        if !Path::new(&record.root_path).is_dir() {
            pruned_stale_roots = true;
            continue;
        }

        if record.project_id == expected_project_id {
            snapshot_candidates.push(record.clone());
        }
        live_root_records.push(record);
    }

    if pruned_stale_roots {
        let _ = registry::replace_projects(registry_path, live_root_records);
    }

    if snapshot_candidates.is_empty() {
        return Err(CommandError::project_not_found());
    }

    Ok(snapshot_candidates)
}
