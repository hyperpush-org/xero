use std::path::Path;

use git2::{DiffFormat, DiffOptions};

use crate::{
    commands::{
        BranchSummaryDto, CommandError, CommandResult, RepositoryDiffResponseDto,
        RepositoryDiffScope,
    },
    git::{repository, status},
    registry::RegistryProjectRecord,
};

pub const MAX_PATCH_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryDiffProjection {
    pub branch: Option<BranchSummaryDto>,
    pub changed_files: usize,
    pub response: RepositoryDiffResponseDto,
}

pub fn load_repository_diff(
    expected_project_id: &str,
    scope: RepositoryDiffScope,
    registry_path: &Path,
) -> CommandResult<RepositoryDiffResponseDto> {
    let candidates = status::lookup_registry_candidates(expected_project_id, registry_path)?;
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

                return Ok(load_repository_diff_from_handle(&repository, scope)?.response);
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

pub fn load_repository_diff_from_root(
    root_path: &Path,
    scope: RepositoryDiffScope,
) -> CommandResult<RepositoryDiffProjection> {
    let repository = repository::open_repository_root(root_path)?;
    load_repository_diff_from_handle(&repository, scope)
}

fn load_repository_diff_from_handle(
    repository: &repository::RepositoryHandle,
    scope: RepositoryDiffScope,
) -> CommandResult<RepositoryDiffProjection> {
    let rendered = render_patch(&repository.repository, scope)?;
    let base_revision = match scope {
        RepositoryDiffScope::Unstaged => None,
        RepositoryDiffScope::Staged | RepositoryDiffScope::Worktree => repository.head_sha.clone(),
    };

    Ok(RepositoryDiffProjection {
        branch: repository.branch.clone(),
        changed_files: rendered.changed_files,
        response: RepositoryDiffResponseDto {
            repository: repository.repository_summary(),
            scope,
            patch: rendered.patch,
            truncated: rendered.truncated,
            base_revision,
        },
    })
}

struct RenderedPatch {
    patch: String,
    truncated: bool,
    changed_files: usize,
}

fn render_patch(
    repository: &git2::Repository,
    scope: RepositoryDiffScope,
) -> CommandResult<RenderedPatch> {
    let mut diff_options = DiffOptions::new();
    diff_options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .include_typechange(true)
        .include_typechange_trees(true)
        .ignore_submodules(true);

    let head_tree = repository
        .head()
        .ok()
        .and_then(|head| head.peel_to_tree().ok());
    let index = repository.index().map_err(|error| {
        CommandError::retryable(
            "git_index_read_failed",
            format!("Xero could not read the repository index: {error}"),
        )
    })?;

    let diff = match scope {
        RepositoryDiffScope::Staged => {
            repository.diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut diff_options))
        }
        RepositoryDiffScope::Unstaged => {
            repository.diff_index_to_workdir(Some(&index), Some(&mut diff_options))
        }
        RepositoryDiffScope::Worktree => {
            repository.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_options))
        }
    }
    .map_err(|error| {
        CommandError::retryable(
            "git_diff_read_failed",
            format!(
                "Xero could not inspect the {} diff: {error}",
                scope_label(scope)
            ),
        )
    })?;

    let changed_files = diff.deltas().count();
    let mut patch = String::new();
    let mut truncated = false;
    let mut bytes_written = 0usize;

    let print_result = diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        let prefix = match line.origin() {
            ' ' | '+' | '-' | '\\' => Some(line.origin()),
            _ => None,
        };
        let content = String::from_utf8_lossy(line.content());
        let mut rendered_line = String::new();
        if let Some(prefix) = prefix {
            rendered_line.push(prefix);
        }
        rendered_line.push_str(&content);
        let content_bytes = rendered_line.len();

        if bytes_written >= MAX_PATCH_BYTES {
            truncated = true;
            return false;
        }

        let remaining = MAX_PATCH_BYTES - bytes_written;
        if content_bytes > remaining {
            let mut end = remaining;
            while end > 0 && !rendered_line.is_char_boundary(end) {
                end -= 1;
            }
            patch.push_str(&rendered_line[..end]);
            bytes_written += end;
            truncated = true;
            return false;
        }

        patch.push_str(&rendered_line);
        bytes_written += content_bytes;
        true
    });

    if let Err(error) = print_result {
        if !truncated {
            return Err(CommandError::retryable(
                "git_diff_render_failed",
                format!(
                    "Xero could not render the {} diff: {error}",
                    scope_label(scope)
                ),
            ));
        }
    }

    Ok(RenderedPatch {
        patch,
        truncated,
        changed_files,
    })
}

fn scope_label(scope: RepositoryDiffScope) -> &'static str {
    match scope {
        RepositoryDiffScope::Staged => "staged",
        RepositoryDiffScope::Unstaged => "unstaged",
        RepositoryDiffScope::Worktree => "worktree",
    }
}
