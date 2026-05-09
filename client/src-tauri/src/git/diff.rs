use std::path::Path;

use git2::{Delta, DiffDelta, DiffFormat, DiffHunk, DiffLine, DiffOptions};

use crate::{
    commands::{
        payload_budget::{
            estimate_serialized_payload_bytes, payload_budget_diagnostic,
            REPOSITORY_DIFF_BUDGET_BYTES,
        },
        BranchSummaryDto, ChangeKind, CommandError, CommandResult, RepositoryDiffFileDto,
        RepositoryDiffHunkDto, RepositoryDiffResponseDto, RepositoryDiffRowDto,
        RepositoryDiffRowKindDto, RepositoryDiffScope,
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

    let mut response = RepositoryDiffResponseDto {
        repository: repository.repository_summary(),
        scope,
        patch: rendered.patch,
        files: rendered.files,
        truncated: rendered.truncated,
        base_revision,
        payload_budget: None,
    };
    let observed_bytes = estimate_serialized_payload_bytes(&response);
    response.payload_budget = payload_budget_diagnostic(
        "repository_diff",
        "repository diff",
        REPOSITORY_DIFF_BUDGET_BYTES,
        observed_bytes,
        response.truncated,
        false,
    );

    Ok(RepositoryDiffProjection {
        branch: repository.branch.clone(),
        changed_files: rendered.changed_files,
        response,
    })
}

struct RenderedPatch {
    patch: String,
    files: Vec<RepositoryDiffFileDto>,
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
    let mut files = diff
        .deltas()
        .map(|delta| repository_diff_file_from_delta(&delta))
        .collect::<Vec<_>>();
    let mut patch = String::new();
    let mut truncated = false;
    let mut bytes_written = 0usize;
    let mut current_file_index: Option<usize> = None;

    let print_result = diff.print(DiffFormat::Patch, |delta, hunk, line| {
        let file_key = repository_diff_file_key(&delta);
        let file_index = match current_file_index {
            Some(index)
                if files.get(index).map(|file| file.cache_key.as_str())
                    == Some(file_key.as_str()) =>
            {
                index
            }
            _ => {
                let index = files
                    .iter()
                    .position(|file| file.cache_key == file_key)
                    .unwrap_or_else(|| {
                        files.push(repository_diff_file_from_delta(&delta));
                        files.len() - 1
                    });
                current_file_index = Some(index);
                index
            }
        };

        let rendered_line = render_diff_line(&line);

        if bytes_written >= MAX_PATCH_BYTES {
            truncated = true;
            mark_file_truncated(&mut files[file_index]);
            return false;
        }

        let content_bytes = rendered_line.len();
        let remaining = MAX_PATCH_BYTES - bytes_written;

        if content_bytes > remaining {
            let end = char_boundary_before(&rendered_line, remaining);
            if end > 0 {
                patch.push_str(&rendered_line[..end]);
                files[file_index].patch.push_str(&rendered_line[..end]);
                bytes_written += end;
            }
            truncated = true;
            mark_file_truncated(&mut files[file_index]);
            return false;
        }

        patch.push_str(&rendered_line);
        files[file_index].patch.push_str(&rendered_line);
        bytes_written += content_bytes;

        append_structured_diff_line(&mut files[file_index], hunk.as_ref(), &line);
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
    if truncated {
        for file in files.iter_mut().filter(|file| file.patch.is_empty()) {
            mark_file_truncated(file);
        }
    }

    Ok(RenderedPatch {
        patch,
        files,
        truncated,
        changed_files,
    })
}

fn render_diff_line(line: &DiffLine<'_>) -> String {
    let mut rendered_line = String::new();
    if matches!(line.origin(), ' ' | '+' | '-' | '\\') {
        rendered_line.push(line.origin());
    }
    rendered_line.push_str(&String::from_utf8_lossy(line.content()));
    rendered_line
}

fn append_structured_diff_line(
    file: &mut RepositoryDiffFileDto,
    hunk: Option<&DiffHunk<'_>>,
    line: &DiffLine<'_>,
) {
    let origin = line.origin();
    let Some(hunk) = hunk else {
        return;
    };

    if origin == 'H' {
        ensure_repository_diff_hunk(file, hunk);
        return;
    }

    let kind = match origin {
        ' ' => RepositoryDiffRowKindDto::Context,
        '+' => RepositoryDiffRowKindDto::Add,
        '-' => RepositoryDiffRowKindDto::Remove,
        '\\' => RepositoryDiffRowKindDto::NoNewline,
        _ => return,
    };
    let hunk_index = ensure_repository_diff_hunk(file, hunk);
    file.hunks[hunk_index].rows.push(RepositoryDiffRowDto {
        kind,
        prefix: match origin {
            '\\' => "\\".into(),
            value => value.to_string(),
        },
        text: diff_line_text(line),
        old_line_number: line.old_lineno(),
        new_line_number: line.new_lineno(),
    });
}

fn ensure_repository_diff_hunk(file: &mut RepositoryDiffFileDto, hunk: &DiffHunk<'_>) -> usize {
    let header = diff_hunk_header(hunk);
    let matches_last = file.hunks.last().is_some_and(|existing| {
        existing.header == header
            && existing.old_start == hunk.old_start()
            && existing.old_lines == hunk.old_lines()
            && existing.new_start == hunk.new_start()
            && existing.new_lines == hunk.new_lines()
    });
    if matches_last {
        return file.hunks.len() - 1;
    }

    file.hunks.push(RepositoryDiffHunkDto {
        header,
        old_start: hunk.old_start(),
        old_lines: hunk.old_lines(),
        new_start: hunk.new_start(),
        new_lines: hunk.new_lines(),
        rows: Vec::new(),
        truncated: false,
    });
    file.hunks.len() - 1
}

fn mark_file_truncated(file: &mut RepositoryDiffFileDto) {
    file.truncated = true;
    if let Some(hunk) = file.hunks.last_mut() {
        hunk.truncated = true;
    }
}

fn repository_diff_file_from_delta(delta: &DiffDelta<'_>) -> RepositoryDiffFileDto {
    let old_path = diff_file_path(delta.old_file().path());
    let new_path = diff_file_path(delta.new_file().path());
    let display_path = new_path
        .clone()
        .or_else(|| old_path.clone())
        .unwrap_or_else(|| "(unknown)".into());
    let status = change_kind_from_delta(delta.status());
    let cache_key =
        repository_diff_file_cache_key(&status, old_path.as_deref(), new_path.as_deref());

    RepositoryDiffFileDto {
        old_path,
        new_path,
        display_path,
        status,
        hunks: Vec::new(),
        patch: String::new(),
        truncated: false,
        cache_key,
    }
}

fn repository_diff_file_key(delta: &DiffDelta<'_>) -> String {
    repository_diff_file_cache_key(
        &change_kind_from_delta(delta.status()),
        diff_file_path(delta.old_file().path()).as_deref(),
        diff_file_path(delta.new_file().path()).as_deref(),
    )
}

fn repository_diff_file_cache_key(
    status: &ChangeKind,
    old_path: Option<&str>,
    new_path: Option<&str>,
) -> String {
    format!(
        "{status:?}\u{0}{}\u{0}{}",
        old_path.unwrap_or(""),
        new_path.unwrap_or("")
    )
}

fn change_kind_from_delta(delta: Delta) -> ChangeKind {
    match delta {
        Delta::Added | Delta::Untracked => ChangeKind::Added,
        Delta::Deleted => ChangeKind::Deleted,
        Delta::Renamed => ChangeKind::Renamed,
        Delta::Copied => ChangeKind::Copied,
        Delta::Typechange => ChangeKind::TypeChange,
        Delta::Conflicted => ChangeKind::Conflicted,
        Delta::Modified | Delta::Ignored | Delta::Unreadable | Delta::Unmodified => {
            ChangeKind::Modified
        }
    }
}

fn diff_file_path(path: Option<&Path>) -> Option<String> {
    path.map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| !path.trim().is_empty())
}

fn diff_hunk_header(hunk: &DiffHunk<'_>) -> String {
    String::from_utf8_lossy(hunk.header())
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

fn diff_line_text(line: &DiffLine<'_>) -> String {
    String::from_utf8_lossy(line.content())
        .trim_end_matches(['\r', '\n'])
        .to_owned()
}

fn char_boundary_before(text: &str, max_bytes: usize) -> usize {
    let mut end = max_bytes.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

fn scope_label(scope: RepositoryDiffScope) -> &'static str {
    match scope {
        RepositoryDiffScope::Staged => "staged",
        RepositoryDiffScope::Unstaged => "unstaged",
        RepositoryDiffScope::Worktree => "worktree",
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use git2::{Repository, Signature};
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn unstaged_diff_returns_structured_files_hunks_and_rows() {
        let (temp_dir, _repository) = repository_with_committed_file("file.txt", "alpha\n");
        fs::write(temp_dir.path().join("file.txt"), "alpha\nbeta\n").unwrap();

        let projection =
            load_repository_diff_from_root(temp_dir.path(), RepositoryDiffScope::Unstaged).unwrap();
        let response = projection.response;

        assert!(response.patch.contains("diff --git a/file.txt b/file.txt"));
        assert!(!response.truncated);
        assert_eq!(response.files.len(), 1);

        let file = &response.files[0];
        assert_eq!(file.display_path, "file.txt");
        assert_eq!(file.status, ChangeKind::Modified);
        assert_eq!(file.old_path.as_deref(), Some("file.txt"));
        assert_eq!(file.new_path.as_deref(), Some("file.txt"));
        assert!(file.patch.contains("+beta"));
        assert_eq!(file.hunks.len(), 1);

        let hunk = &file.hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.new_start, 1);
        assert!(hunk.header.starts_with("@@ -1"));
        assert!(hunk.rows.iter().any(|row| {
            row.kind == RepositoryDiffRowKindDto::Add
                && row.text == "beta"
                && row.old_line_number.is_none()
                && row.new_line_number == Some(2)
        }));
    }

    #[test]
    fn structured_diff_keeps_one_file_entry_per_delta() {
        let (temp_dir, repository) = repository_with_committed_file("file.txt", "alpha\n");
        fs::write(temp_dir.path().join("file.txt"), "alpha\nbeta\n").unwrap();
        fs::write(temp_dir.path().join("new.txt"), "new\n").unwrap();
        stage_path(&repository, "new.txt");

        let unstaged =
            load_repository_diff_from_root(temp_dir.path(), RepositoryDiffScope::Unstaged)
                .unwrap()
                .response;
        let staged = load_repository_diff_from_root(temp_dir.path(), RepositoryDiffScope::Staged)
            .unwrap()
            .response;

        assert_eq!(unstaged.files.len(), 1);
        assert_eq!(unstaged.files[0].display_path, "file.txt");
        assert_eq!(staged.files.len(), 1);
        assert_eq!(staged.files[0].display_path, "new.txt");
        assert_eq!(staged.files[0].status, ChangeKind::Added);
    }

    fn repository_with_committed_file(path: &str, content: &str) -> (TempDir, Repository) {
        let temp_dir = tempfile::tempdir().unwrap();
        let repository = Repository::init(temp_dir.path()).unwrap();
        fs::write(temp_dir.path().join(path), content).unwrap();
        stage_path(&repository, path);
        commit_index(&repository, "initial commit");
        (temp_dir, repository)
    }

    fn stage_path(repository: &Repository, path: &str) {
        let mut index = repository.index().unwrap();
        index.add_path(Path::new(path)).unwrap();
        index.write().unwrap();
    }

    fn commit_index(repository: &Repository, message: &str) {
        let signature = Signature::now("Xero Test", "xero@example.test").unwrap();
        let mut index = repository.index().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        repository
            .commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
            .unwrap();
    }
}
