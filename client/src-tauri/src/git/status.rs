use std::path::Path;

use crate::{
    commands::{CommandError, CommandResult, RepositoryStatusResponseDto},
    git::repository::{self, CanonicalRepository},
    registry::{self, RegistryProjectRecord},
};

pub fn load_repository_status(
    expected_project_id: &str,
    registry_path: &Path,
) -> CommandResult<RepositoryStatusResponseDto> {
    let repository = resolve_project_repository(expected_project_id, registry_path)?;
    Ok(repository.repository_status())
}

pub fn load_repository_status_from_root(
    root_path: &Path,
) -> CommandResult<RepositoryStatusResponseDto> {
    let repository = repository::open_repository_root(root_path)?;
    Ok(repository.canonical_repository()?.repository_status())
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
        match repository::open_repository_root(Path::new(&root_path)) {
            Ok(repository) => {
                if repository.project_id() != project_id
                    || repository.repository_id() != repository_id
                {
                    return Err(CommandError::system_fault(
                        "project_registry_mismatch",
                        format!(
                            "Registry entry for project `{project_id}` no longer matches the repository discovered at {root_path}."
                        ),
                    ));
                }

                return repository.canonical_repository();
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
    let snapshot_candidates = registry::read_project_records(registry_path, expected_project_id)?
        .into_iter()
        .filter(|record| Path::new(&record.root_path).is_dir())
        .collect::<Vec<_>>();

    if snapshot_candidates.is_empty() {
        return Err(CommandError::project_not_found());
    }

    Ok(snapshot_candidates)
}
