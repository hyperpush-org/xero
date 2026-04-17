use std::{fs, path::PathBuf};

use git2::{Repository, Status, StatusOptions};
use sha2::{Digest, Sha256};

use crate::{
    commands::{
        BranchSummaryDto, ChangeKind, CommandError, CommandResult, RepositoryStatusEntryDto,
        RepositoryStatusResponseDto, RepositorySummaryDto,
    },
    state::ImportFailpoints,
};

const CADENCE_EXCLUDE_ENTRY: &str = ".cadence/";

#[derive(Debug, Clone)]
pub struct CanonicalRepository {
    pub project_id: String,
    pub repository_id: String,
    pub root_path: PathBuf,
    pub root_path_string: String,
    pub common_git_dir: PathBuf,
    pub display_name: String,
    pub branch_name: Option<String>,
    pub head_sha: Option<String>,
    pub branch: Option<BranchSummaryDto>,
    pub status_entries: Vec<RepositoryStatusEntryDto>,
    pub has_staged_changes: bool,
    pub has_unstaged_changes: bool,
    pub has_untracked_changes: bool,
}

impl CanonicalRepository {
    pub fn repository_summary(&self) -> RepositorySummaryDto {
        RepositorySummaryDto {
            id: self.repository_id.clone(),
            project_id: self.project_id.clone(),
            root_path: self.root_path_string.clone(),
            display_name: self.display_name.clone(),
            branch: self.branch_name.clone(),
            head_sha: self.head_sha.clone(),
            is_git_repo: true,
        }
    }

    pub fn repository_status(&self) -> RepositoryStatusResponseDto {
        RepositoryStatusResponseDto {
            repository: self.repository_summary(),
            branch: self.branch.clone(),
            entries: self.status_entries.clone(),
            has_staged_changes: self.has_staged_changes,
            has_unstaged_changes: self.has_unstaged_changes,
            has_untracked_changes: self.has_untracked_changes,
        }
    }
}

pub fn resolve_repository(selected_path: &str) -> CommandResult<CanonicalRepository> {
    let trimmed_path = selected_path.trim();
    let canonical_selected_path =
        fs::canonicalize(trimmed_path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError::user_fixable(
                "repository_path_not_found",
                format!("Repository path `{trimmed_path}` does not exist."),
            ),
            _ => CommandError::user_fixable(
                "repository_path_invalid",
                format!("Cadence could not read repository path `{trimmed_path}`: {error}"),
            ),
        })?;

    let repository = Repository::discover(&canonical_selected_path).map_err(|_| {
        CommandError::user_fixable(
            "git_repository_not_found",
            "Selected path is not inside a Git repository.",
        )
    })?;

    let workdir = repository.workdir().ok_or_else(|| {
        CommandError::user_fixable(
            "git_worktree_required",
            "Cadence can only import repositories with a working tree.",
        )
    })?;

    let canonical_root_path = fs::canonicalize(workdir).map_err(|error| {
        CommandError::system_fault(
            "repository_root_canonicalize_failed",
            format!(
                "Cadence could not canonicalize the repository root at {}: {error}",
                workdir.display()
            ),
        )
    })?;

    let common_git_dir = fs::canonicalize(repository.commondir())
        .unwrap_or_else(|_| repository.commondir().to_path_buf());
    let root_path_string = canonical_root_path.to_string_lossy().into_owned();
    let display_name = canonical_root_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| root_path_string.clone());

    let digest = stable_digest(&root_path_string);
    let (branch, branch_name, head_sha) = read_branch(&repository);
    let status_entries = read_status_entries(&repository)?;
    let has_staged_changes = status_entries.iter().any(|entry| entry.staged.is_some());
    let has_unstaged_changes = status_entries.iter().any(|entry| entry.unstaged.is_some());
    let has_untracked_changes = status_entries.iter().any(|entry| entry.untracked);

    Ok(CanonicalRepository {
        project_id: format!("project_{digest}"),
        repository_id: format!("repo_{digest}"),
        root_path: canonical_root_path,
        root_path_string,
        common_git_dir,
        display_name,
        branch_name,
        head_sha,
        branch,
        status_entries,
        has_staged_changes,
        has_unstaged_changes,
        has_untracked_changes,
    })
}

pub fn ensure_cadence_excluded(
    repository: &CanonicalRepository,
    failpoints: &ImportFailpoints,
) -> CommandResult<()> {
    if failpoints.fail_exclude_write {
        return Err(CommandError::retryable(
            "git_exclude_write_failed",
            "Test failpoint forced the .git/info/exclude update to fail.",
        ));
    }

    let exclude_path = repository.common_git_dir.join("info").join("exclude");
    if let Some(parent) = exclude_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "git_exclude_write_failed",
                format!(
                    "Cadence could not prepare the git exclude directory at {}: {error}",
                    parent.display()
                ),
            )
        })?;
    }

    let existing_contents = fs::read_to_string(&exclude_path).unwrap_or_default();
    if existing_contents
        .lines()
        .any(|line| matches!(line.trim(), ".cadence" | ".cadence/"))
    {
        return Ok(());
    }

    let mut next_contents = existing_contents;
    if !next_contents.is_empty() && !next_contents.ends_with('\n') {
        next_contents.push('\n');
    }
    next_contents.push_str(CADENCE_EXCLUDE_ENTRY);
    next_contents.push('\n');

    fs::write(&exclude_path, next_contents).map_err(|error| {
        CommandError::retryable(
            "git_exclude_write_failed",
            format!(
                "Cadence could not write .git/info/exclude for {}: {error}",
                repository.root_path.display()
            ),
        )
    })
}

fn stable_digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read_branch(
    repository: &Repository,
) -> (Option<BranchSummaryDto>, Option<String>, Option<String>) {
    let head = match repository.head() {
        Ok(head) => head,
        Err(_) => return (None, None, None),
    };

    let branch_name = head.shorthand().map(ToOwned::to_owned);
    let head_sha = head.target().map(|oid| oid.to_string());
    let detached = repository.head_detached().unwrap_or(false);

    let branch = branch_name
        .as_ref()
        .or(head_sha.as_ref())
        .map(|_| BranchSummaryDto {
            name: branch_name.clone().unwrap_or_else(|| "HEAD".into()),
            head_sha: head_sha.clone(),
            detached,
        });

    (branch, branch_name, head_sha)
}

fn read_status_entries(
    repository: &Repository,
) -> Result<Vec<RepositoryStatusEntryDto>, CommandError> {
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true)
        .include_ignored(false)
        .include_unmodified(false);

    let statuses = repository.statuses(Some(&mut options)).map_err(|error| {
        CommandError::retryable(
            "git_status_read_failed",
            format!("Cadence could not read repository status: {error}"),
        )
    })?;

    Ok(statuses.iter().filter_map(map_status_entry).collect())
}

fn map_status_entry(entry: git2::StatusEntry<'_>) -> Option<RepositoryStatusEntryDto> {
    let path = entry.path()?.to_string();
    let status = entry.status();

    Some(RepositoryStatusEntryDto {
        path,
        staged: map_staged_change(status),
        unstaged: map_unstaged_change(status),
        untracked: status.contains(Status::WT_NEW),
    })
}

fn map_staged_change(status: Status) -> Option<ChangeKind> {
    if status.is_conflicted() {
        return Some(ChangeKind::Conflicted);
    }

    if status.contains(Status::INDEX_NEW) {
        return Some(ChangeKind::Added);
    }
    if status.contains(Status::INDEX_MODIFIED) {
        return Some(ChangeKind::Modified);
    }
    if status.contains(Status::INDEX_DELETED) {
        return Some(ChangeKind::Deleted);
    }
    if status.contains(Status::INDEX_RENAMED) {
        return Some(ChangeKind::Renamed);
    }
    if status.contains(Status::INDEX_TYPECHANGE) {
        return Some(ChangeKind::TypeChange);
    }

    None
}

fn map_unstaged_change(status: Status) -> Option<ChangeKind> {
    if status.is_conflicted() {
        return Some(ChangeKind::Conflicted);
    }

    if status.contains(Status::WT_MODIFIED) {
        return Some(ChangeKind::Modified);
    }
    if status.contains(Status::WT_DELETED) {
        return Some(ChangeKind::Deleted);
    }
    if status.contains(Status::WT_RENAMED) {
        return Some(ChangeKind::Renamed);
    }
    if status.contains(Status::WT_TYPECHANGE) {
        return Some(ChangeKind::TypeChange);
    }

    None
}
