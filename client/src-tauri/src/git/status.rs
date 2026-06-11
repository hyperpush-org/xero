use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use crate::{
    commands::{CommandError, CommandResult, RepositoryStatusResponseDto},
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

pub fn load_repository_status(
    expected_project_id: &str,
    registry_path: &Path,
) -> CommandResult<RepositoryStatusResponseDto> {
    let repository = resolve_project_repository_handle(expected_project_id, registry_path)?;
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
    let repository = repository::open_repository_root(root_path)?;
    let cache_key = repository.status_cache_signature();
    if let Some(response) = cached_repository_status(&cache_key) {
        return Ok(response);
    }

    let response = repository.canonical_repository()?.repository_status();
    store_repository_status(cache_key, response.clone());
    Ok(response)
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
