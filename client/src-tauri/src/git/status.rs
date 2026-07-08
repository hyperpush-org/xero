use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use crate::{
    commands::{
        payload_budget::{
            estimate_serialized_payload_bytes, payload_budget_diagnostic,
            REPOSITORY_STATUS_BUDGET_BYTES,
        },
        CommandError, CommandResult, RepositoryStatusResponseDto, RepositorySummaryDto,
    },
    db,
    git::repository::{self, CanonicalRepository},
    registry::{self, RegistryProjectRecord},
};

const REPOSITORY_STATUS_CACHE_TTL: Duration = Duration::from_millis(1_500);

#[derive(Debug, Clone)]
struct CachedRepositoryStatus {
    stored_at: Instant,
    response: RepositoryStatusResponseDto,
}

static REPOSITORY_STATUS_CACHE: OnceLock<Mutex<HashMap<String, CachedRepositoryStatus>>> =
    OnceLock::new();

pub fn empty_repository_status(repository: RepositorySummaryDto) -> RepositoryStatusResponseDto {
    let mut response = RepositoryStatusResponseDto {
        repository,
        branch: None,
        last_commit: None,
        entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
        additions: 0,
        deletions: 0,
        payload_budget: None,
    };
    let observed_bytes = estimate_serialized_payload_bytes(&response);
    response.payload_budget = payload_budget_diagnostic(
        "repository_status",
        "repository status",
        REPOSITORY_STATUS_BUDGET_BYTES,
        observed_bytes,
        false,
        false,
    );
    response
}

pub fn load_repository_status(
    expected_project_id: &str,
    registry_path: &Path,
) -> CommandResult<RepositoryStatusResponseDto> {
    let candidates = lookup_registry_candidates(expected_project_id, registry_path)?;
    let mut first_error: Option<CommandError> = None;

    for record in candidates {
        let root_path = Path::new(&record.root_path);
        if !record.is_git_repo {
            return empty_status_for_registry_record(&record);
        }

        match repository::open_repository_root(root_path) {
            Ok(repository) => {
                if repository.project_id() != record.project_id
                    || repository.repository_id() != record.repository_id
                {
                    return Err(CommandError::system_fault(
                        "project_registry_mismatch",
                        format!(
                            "Registry entry for project `{}` no longer matches the repository discovered at {}.",
                            record.project_id, record.root_path
                        ),
                    ));
                }

                return load_repository_status_from_handle(&repository);
            }
            Err(error) => {
                if let Some(response) = empty_status_for_plain_project_root(root_path)? {
                    return Ok(response);
                }
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(CommandError::project_not_found))
}

fn load_repository_status_from_handle(
    repository: &repository::RepositoryHandle,
) -> CommandResult<RepositoryStatusResponseDto> {
    let cache_key = repository.status_cache_signature();
    if let Some(response) = cached_repository_status(&cache_key) {
        return Ok(response);
    }

    let response = repository.canonical_repository()?.repository_status();
    store_repository_status(cache_key, response.clone());
    Ok(response)
}

pub fn load_repository_status_from_root(
    root_path: &Path,
) -> CommandResult<RepositoryStatusResponseDto> {
    match repository::open_repository_root(root_path) {
        Ok(repository) => load_repository_status_from_handle(&repository),
        Err(error) => match empty_status_for_plain_project_root(root_path)? {
            Some(response) => Ok(response),
            None => Err(error),
        },
    }
}

pub fn resolve_project_repository(
    expected_project_id: &str,
    registry_path: &Path,
) -> Result<CanonicalRepository, CommandError> {
    resolve_project_repository_handle(expected_project_id, registry_path)?.canonical_repository()
}

pub fn resolve_project_repository_handle(
    expected_project_id: &str,
    registry_path: &Path,
) -> Result<repository::RepositoryHandle, CommandError> {
    let candidates = lookup_registry_candidates(expected_project_id, registry_path)?;
    let mut first_error: Option<CommandError> = None;

    for record in candidates {
        match repository::open_repository_root(Path::new(&record.root_path)) {
            Ok(repository) => {
                if repository.project_id() != record.project_id
                    || repository.repository_id() != record.repository_id
                {
                    return Err(CommandError::system_fault(
                        "project_registry_mismatch",
                        format!(
                            "Registry entry for project `{}` no longer matches the repository discovered at {}.",
                            record.project_id, record.root_path
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

fn empty_status_for_registry_record(
    record: &RegistryProjectRecord,
) -> CommandResult<RepositoryStatusResponseDto> {
    let root_path = Path::new(&record.root_path);
    let repository = db::repository_summary_for_repo_root(root_path)?
        .unwrap_or_else(|| record.repository_summary());
    Ok(empty_repository_status(repository))
}

fn empty_status_for_plain_project_root(
    root_path: &Path,
) -> CommandResult<Option<RepositoryStatusResponseDto>> {
    let Some(repository) = db::repository_summary_for_repo_root(root_path)? else {
        return Ok(None);
    };
    if repository.is_git_repo {
        return Ok(None);
    }
    Ok(Some(empty_repository_status(repository)))
}

fn cached_repository_status(cache_key: &str) -> Option<RepositoryStatusResponseDto> {
    let cache = REPOSITORY_STATUS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().ok()?;
    let cached = guard.get(cache_key)?;
    if cached.stored_at.elapsed() <= REPOSITORY_STATUS_CACHE_TTL {
        return Some(cached.response.clone());
    }
    guard.remove(cache_key);
    None
}

fn store_repository_status(cache_key: String, response: RepositoryStatusResponseDto) {
    let cache = REPOSITORY_STATUS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut guard) = cache.lock() else {
        return;
    };
    guard.insert(
        cache_key,
        CachedRepositoryStatus {
            stored_at: Instant::now(),
            response,
        },
    );
    if guard.len() > 64 {
        guard.retain(|_, value| value.stored_at.elapsed() <= REPOSITORY_STATUS_CACHE_TTL);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, registry};

    #[test]
    fn load_repository_status_returns_empty_status_for_plain_folder_project() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let registry_path = tempdir.path().join("app-data").join("xero.db");
        let project_root = tempdir.path().join("plain-folder");
        std::fs::create_dir_all(&project_root).expect("project root");
        db::configure_project_database_paths(&registry_path);

        let imported =
            db::import_project_directory(&project_root, &crate::state::ImportFailpoints::default())
                .expect("import plain folder");
        registry::upsert_project(
            &registry_path,
            RegistryProjectRecord {
                project_id: imported.project.id.clone(),
                repository_id: imported.repository.id.clone(),
                root_path: imported.repository.root_path.clone(),
                is_git_repo: imported.repository.is_git_repo,
            },
            &crate::state::ImportFailpoints::default(),
        )
        .expect("registry write");

        let status =
            load_repository_status(&imported.project.id, &registry_path).expect("load status");

        assert_eq!(status.repository.id, imported.repository.id);
        assert!(!status.repository.is_git_repo);
        assert!(status.entries.is_empty());
        assert!(!status.has_staged_changes);
        assert!(!status.has_unstaged_changes);
        assert!(!status.has_untracked_changes);
        assert_eq!(status.additions, 0);
        assert_eq!(status.deletions, 0);
    }
}
