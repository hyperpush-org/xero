use std::{
    fs,
    path::{Path, PathBuf},
};

use git2::{BranchType, DiffOptions, Repository, Status, StatusOptions};
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::commands::{
    payload_budget::{
        estimate_serialized_payload_bytes, payload_budget_diagnostic,
        REPOSITORY_STATUS_BUDGET_BYTES,
    },
    BranchSummaryDto, BranchUpstreamSummaryDto, ChangeKind, CommandError, CommandResult,
    LastCommitSummaryDto, RepositoryStatusEntryDto, RepositoryStatusResponseDto,
    RepositorySummaryDto,
};

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
    pub last_commit: Option<LastCommitSummaryDto>,
    pub status_entries: Vec<RepositoryStatusEntryDto>,
    pub has_staged_changes: bool,
    pub has_unstaged_changes: bool,
    pub has_untracked_changes: bool,
    pub additions: u32,
    pub deletions: u32,
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
        let mut response = RepositoryStatusResponseDto {
            repository: self.repository_summary(),
            branch: self.branch.clone(),
            last_commit: self.last_commit.clone(),
            entries: self.status_entries.clone(),
            has_staged_changes: self.has_staged_changes,
            has_unstaged_changes: self.has_unstaged_changes,
            has_untracked_changes: self.has_untracked_changes,
            additions: self.additions,
            deletions: self.deletions,
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
}

pub struct RepositoryHandle {
    pub repository: Repository,
    pub root_path: PathBuf,
    pub root_path_string: String,
    pub common_git_dir: PathBuf,
    pub display_name: String,
    pub branch_name: Option<String>,
    pub head_sha: Option<String>,
    pub branch: Option<BranchSummaryDto>,
    pub last_commit: Option<LastCommitSummaryDto>,
}

impl RepositoryHandle {
    pub fn project_id(&self) -> String {
        format!("project_{}", stable_digest(&self.root_path_string))
    }

    pub fn repository_id(&self) -> String {
        format!("repo_{}", stable_digest(&self.root_path_string))
    }

    pub fn repository_summary(&self) -> RepositorySummaryDto {
        RepositorySummaryDto {
            id: self.repository_id(),
            project_id: self.project_id(),
            root_path: self.root_path_string.clone(),
            display_name: self.display_name.clone(),
            branch: self.branch_name.clone(),
            head_sha: self.head_sha.clone(),
            is_git_repo: true,
        }
    }

    pub fn canonical_repository(&self) -> Result<CanonicalRepository, CommandError> {
        let status_entries = read_status_entries(&self.repository)?;
        let has_staged_changes = status_entries.iter().any(|entry| entry.staged.is_some());
        let has_unstaged_changes = status_entries.iter().any(|entry| entry.unstaged.is_some());
        let has_untracked_changes = status_entries.iter().any(|entry| entry.untracked);
        let (additions, deletions) = read_diff_line_counts(&self.repository);

        Ok(CanonicalRepository {
            project_id: self.project_id(),
            repository_id: self.repository_id(),
            root_path: self.root_path.clone(),
            root_path_string: self.root_path_string.clone(),
            common_git_dir: self.common_git_dir.clone(),
            display_name: self.display_name.clone(),
            branch_name: self.branch_name.clone(),
            head_sha: self.head_sha.clone(),
            branch: self.branch.clone(),
            last_commit: self.last_commit.clone(),
            status_entries,
            has_staged_changes,
            has_unstaged_changes,
            has_untracked_changes,
            additions,
            deletions,
        })
    }
}

pub fn resolve_repository(selected_path: &str) -> CommandResult<CanonicalRepository> {
    open_repository(selected_path)?.canonical_repository()
}

pub fn open_repository(selected_path: &str) -> CommandResult<RepositoryHandle> {
    open_repository_internal(selected_path.trim())
}

pub fn open_repository_root(root_path: &Path) -> CommandResult<RepositoryHandle> {
    let root_path = root_path.to_string_lossy();
    open_repository_internal(root_path.as_ref())
}

fn open_repository_internal(selected_path: &str) -> CommandResult<RepositoryHandle> {
    let canonical_selected_path =
        fs::canonicalize(selected_path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError::user_fixable(
                "repository_path_not_found",
                format!("Repository path `{selected_path}` does not exist."),
            ),
            _ => CommandError::user_fixable(
                "repository_path_invalid",
                format!("Xero could not read repository path `{selected_path}`: {error}"),
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
            "Xero can only import repositories with a working tree.",
        )
    })?;

    let canonical_root_path = fs::canonicalize(workdir).map_err(|error| {
        CommandError::system_fault(
            "repository_root_canonicalize_failed",
            format!(
                "Xero could not canonicalize the repository root at {}: {error}",
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

    let head_details = read_head_details(&repository);

    Ok(RepositoryHandle {
        repository,
        root_path: canonical_root_path,
        root_path_string,
        common_git_dir,
        display_name,
        branch_name: head_details.branch_name,
        head_sha: head_details.head_sha,
        branch: head_details.branch,
        last_commit: head_details.last_commit,
    })
}

struct HeadDetails {
    branch: Option<BranchSummaryDto>,
    branch_name: Option<String>,
    head_sha: Option<String>,
    last_commit: Option<LastCommitSummaryDto>,
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

fn read_head_details(repository: &Repository) -> HeadDetails {
    let head = match repository.head() {
        Ok(head) => head,
        Err(_) => {
            return HeadDetails {
                branch: None,
                branch_name: None,
                head_sha: None,
                last_commit: None,
            };
        }
    };

    let branch_name = head.shorthand().map(ToOwned::to_owned);
    let detached = repository.head_detached().unwrap_or(false);
    let commit = head.peel_to_commit().ok();
    let head_oid = commit
        .as_ref()
        .map(|commit| commit.id())
        .or_else(|| head.target());
    let head_sha = head_oid.map(|oid| oid.to_string());
    let upstream = if detached {
        None
    } else {
        read_branch_upstream(repository, branch_name.as_deref(), head_oid)
    };
    let branch = branch_name
        .as_ref()
        .or(head_sha.as_ref())
        .map(|_| BranchSummaryDto {
            name: branch_name.clone().unwrap_or_else(|| "HEAD".into()),
            head_sha: head_sha.clone(),
            detached,
            upstream,
        });

    HeadDetails {
        branch,
        branch_name,
        head_sha,
        last_commit: commit.as_ref().and_then(map_last_commit),
    }
}

fn read_branch_upstream(
    repository: &Repository,
    branch_name: Option<&str>,
    head_oid: Option<git2::Oid>,
) -> Option<BranchUpstreamSummaryDto> {
    let branch_name = branch_name?;
    let head_oid = head_oid?;
    let branch = repository
        .find_branch(branch_name, BranchType::Local)
        .ok()?;
    let upstream = branch.upstream().ok()?;
    let upstream_name = upstream.name().ok().flatten()?.to_owned();
    let upstream_oid = upstream.get().target().or_else(|| {
        upstream
            .get()
            .peel_to_commit()
            .ok()
            .map(|commit| commit.id())
    })?;
    let (ahead, behind) = repository.graph_ahead_behind(head_oid, upstream_oid).ok()?;

    Some(BranchUpstreamSummaryDto {
        name: upstream_name,
        ahead: u32::try_from(ahead).unwrap_or(u32::MAX),
        behind: u32::try_from(behind).unwrap_or(u32::MAX),
    })
}

fn map_last_commit(commit: &git2::Commit<'_>) -> Option<LastCommitSummaryDto> {
    let summary = commit
        .summary()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            commit.message().and_then(|message| {
                message
                    .lines()
                    .map(str::trim)
                    .find(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            })
        })?;

    Some(LastCommitSummaryDto {
        sha: commit.id().to_string(),
        summary,
        committed_at: git_timestamp_to_rfc3339(commit.time().seconds()),
    })
}

fn git_timestamp_to_rfc3339(unix_timestamp: i64) -> Option<String> {
    OffsetDateTime::from_unix_timestamp(unix_timestamp)
        .ok()
        .and_then(|value| value.format(&Rfc3339).ok())
}

/// Best-effort additions/deletions across staged + unstaged tracked files.
/// Untracked files are surfaced through status entries, but their contents are
/// not diffed here: reading every new file made project switches crawl in large
/// worktrees. Returns (0, 0) on any error so the badge degrades gracefully.
fn read_diff_line_counts(repository: &Repository) -> (u32, u32) {
    let mut diff_options = DiffOptions::new();
    diff_options
        .include_untracked(false)
        .include_typechange(true)
        .ignore_submodules(true);

    let head_tree = repository
        .head()
        .ok()
        .and_then(|head| head.peel_to_tree().ok());

    let diff = match repository
        .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_options))
    {
        Ok(diff) => diff,
        Err(_) => return (0, 0),
    };

    let stats = match diff.stats() {
        Ok(stats) => stats,
        Err(_) => return (0, 0),
    };

    let insertions = u32::try_from(stats.insertions()).unwrap_or(u32::MAX);
    let deletions = u32::try_from(stats.deletions()).unwrap_or(u32::MAX);
    (insertions, deletions)
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
            format!("Xero could not read repository status: {error}"),
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

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use git2::{Repository, Signature};
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn diff_line_counts_skip_untracked_file_contents() {
        let (temp_dir, repository) = repository_with_committed_file("tracked.txt", "alpha\nbeta\n");
        fs::write(
            temp_dir.path().join("tracked.txt"),
            "alpha\nbeta\ntracked addition\n",
        )
        .expect("modify tracked file");
        fs::write(
            temp_dir.path().join("untracked.txt"),
            "untracked one\nuntracked two\nuntracked three\n",
        )
        .expect("write untracked file");

        assert_eq!(read_diff_line_counts(&repository), (1, 0));

        let status_entries = read_status_entries(&repository).expect("status entries");
        assert!(status_entries
            .iter()
            .any(|entry| entry.path == "untracked.txt" && entry.untracked));
    }

    fn repository_with_committed_file(path: &str, content: &str) -> (TempDir, Repository) {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repository = Repository::init(temp_dir.path()).expect("init repository");
        fs::write(temp_dir.path().join(path), content).expect("write initial file");
        stage_path(&repository, path);
        commit_index(&repository, "initial commit");
        (temp_dir, repository)
    }

    fn stage_path(repository: &Repository, path: &str) {
        let mut index = repository.index().expect("index");
        index.add_path(Path::new(path)).expect("add path");
        index.write().expect("write index");
    }

    fn commit_index(repository: &Repository, message: &str) {
        let signature = Signature::now("Xero Test", "xero@example.test").expect("signature");
        let mut index = repository.index().expect("index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repository.find_tree(tree_id).expect("tree");
        repository
            .commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
            .expect("commit");
    }
}
