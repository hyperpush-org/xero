use std::path::Path;

use git2::{DiffFormat, DiffOptions, Repository};

use crate::{
    commands::{CommandError, CommandResult, RepositoryDiffResponseDto, RepositoryDiffScope},
    git::status,
};

pub const MAX_PATCH_BYTES: usize = 64 * 1024;

pub fn load_repository_diff(
    expected_project_id: &str,
    scope: RepositoryDiffScope,
    registry_path: &Path,
) -> CommandResult<RepositoryDiffResponseDto> {
    let canonical_repository =
        status::resolve_project_repository(expected_project_id, registry_path)?;
    let repository = Repository::discover(&canonical_repository.root_path).map_err(|error| {
        CommandError::retryable(
            "git_repository_open_failed",
            format!(
                "Cadence could not reopen the imported repository at {}: {error}",
                canonical_repository.root_path.display()
            ),
        )
    })?;

    let rendered = render_patch(&repository, scope)?;
    let base_revision = match scope {
        RepositoryDiffScope::Unstaged => None,
        RepositoryDiffScope::Staged | RepositoryDiffScope::Worktree => {
            canonical_repository.head_sha.clone()
        }
    };

    Ok(RepositoryDiffResponseDto {
        repository: canonical_repository.repository_summary(),
        scope,
        patch: rendered.patch,
        truncated: rendered.truncated,
        base_revision,
    })
}

struct RenderedPatch {
    patch: String,
    truncated: bool,
}

fn render_patch(
    repository: &Repository,
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
            format!("Cadence could not read the repository index: {error}"),
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
                "Cadence could not inspect the {} diff: {error}",
                scope_label(scope)
            ),
        )
    })?;

    let mut patch = String::new();
    let mut truncated = false;
    let mut bytes_written = 0usize;

    let print_result = diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        let content = String::from_utf8_lossy(line.content());
        let content_bytes = content.len();

        if bytes_written >= MAX_PATCH_BYTES {
            truncated = true;
            return false;
        }

        let remaining = MAX_PATCH_BYTES - bytes_written;
        if content_bytes > remaining {
            let mut end = remaining;
            while end > 0 && !content.is_char_boundary(end) {
                end -= 1;
            }
            patch.push_str(&content[..end]);
            bytes_written += end;
            truncated = true;
            return false;
        }

        patch.push_str(&content);
        bytes_written += content_bytes;
        true
    });

    if let Err(error) = print_result {
        if !truncated {
            return Err(CommandError::retryable(
                "git_diff_render_failed",
                format!(
                    "Cadence could not render the {} diff: {error}",
                    scope_label(scope)
                ),
            ));
        }
    }

    Ok(RenderedPatch { patch, truncated })
}

fn scope_label(scope: RepositoryDiffScope) -> &'static str {
    match scope {
        RepositoryDiffScope::Staged => "staged",
        RepositoryDiffScope::Unstaged => "unstaged",
        RepositoryDiffScope::Worktree => "worktree",
    }
}
