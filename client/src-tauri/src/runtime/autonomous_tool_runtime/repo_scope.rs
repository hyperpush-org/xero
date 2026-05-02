use std::{
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
};

use globset::{GlobBuilder, GlobMatcher};
use tauri::{AppHandle, Runtime};

use super::AutonomousToolRuntime;

use crate::{
    commands::{validate_non_empty, CommandError, CommandErrorClass, CommandResult},
    db::project_store,
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};

const SKIPPED_DIRECTORIES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".next",
    "dist",
    "build",
    "coverage",
    ".turbo",
    ".yarn",
    ".pnpm-store",
];

#[derive(Debug, Default)]
pub(super) struct WalkState {
    pub(super) scanned_files: usize,
    pub(super) truncated: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct WalkErrorCodes {
    pub(super) metadata_failed: &'static str,
    pub(super) read_dir_failed: &'static str,
}

impl AutonomousToolRuntime {
    pub(super) fn walk_scope<F>(
        &self,
        scope: &Path,
        error_codes: WalkErrorCodes,
        walk: &mut WalkState,
        visit_file: &mut F,
    ) -> CommandResult<()>
    where
        F: FnMut(&Path, &mut WalkState) -> CommandResult<()>,
    {
        if walk.truncated {
            return Ok(());
        }

        let metadata = fs::symlink_metadata(scope).map_err(|error| {
            CommandError::retryable(
                error_codes.metadata_failed,
                format!("Xero could not inspect {}: {error}", scope.display()),
            )
        })?;

        if metadata.file_type().is_symlink() {
            return Ok(());
        }

        if metadata.is_dir() {
            if self.should_skip_directory(scope) {
                return Ok(());
            }

            for entry in self.read_sorted_directory_entries(scope, error_codes.read_dir_failed)? {
                if walk.truncated {
                    break;
                }
                self.walk_scope(&entry.path(), error_codes, walk, visit_file)?;
            }
            return Ok(());
        }

        walk.scanned_files = walk.scanned_files.saturating_add(1);
        visit_file(scope, walk)
    }

    pub(super) fn repo_relative_path(&self, path: &Path) -> CommandResult<PathBuf> {
        path.strip_prefix(&self.repo_root)
            .map(|relative| relative.to_path_buf())
            .map_err(|_| {
                CommandError::new(
                    "autonomous_tool_path_denied",
                    CommandErrorClass::PolicyDenied,
                    format!(
                        "Xero denied access to `{}` because it resolves outside the imported repository root.",
                        path.display()
                    ),
                    false,
                )
            })
    }

    pub(super) fn should_skip_directory(&self, path: &Path) -> bool {
        path != self.repo_root
            && path
                .file_name()
                .is_some_and(|name| SKIPPED_DIRECTORIES.contains(&name.to_string_lossy().as_ref()))
    }

    pub(super) fn resolve_existing_path(&self, relative_path: &Path) -> CommandResult<PathBuf> {
        let candidate = self.repo_root.join(relative_path);
        if !candidate.exists() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_path_not_found",
                format!(
                    "Xero could not find `{}` inside the imported repository.",
                    path_to_forward_slash(relative_path)
                ),
            ));
        }

        let resolved = fs::canonicalize(&candidate).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_path_resolve_failed",
                format!("Xero could not resolve {}: {error}", candidate.display()),
            )
        })?;
        self.ensure_inside_root(&resolved, relative_path)
    }

    pub(super) fn resolve_existing_directory(
        &self,
        relative_path: &Path,
    ) -> CommandResult<PathBuf> {
        let resolved = self.resolve_existing_path(relative_path)?;
        if !resolved.is_dir() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_directory_required",
                format!(
                    "Xero requires `{}` to resolve to a directory inside the imported repository.",
                    path_to_forward_slash(relative_path)
                ),
            ));
        }
        Ok(resolved)
    }

    pub(super) fn resolve_writable_path(&self, relative_path: &Path) -> CommandResult<PathBuf> {
        let candidate = self.repo_root.join(relative_path);
        if candidate.exists() {
            let resolved = fs::canonicalize(&candidate).map_err(|error| {
                CommandError::retryable(
                    "autonomous_tool_path_resolve_failed",
                    format!("Xero could not resolve {}: {error}", candidate.display()),
                )
            })?;
            return self.ensure_inside_root(&resolved, relative_path);
        }

        let mut missing_components = Vec::<OsString>::new();
        let mut ancestor = candidate.as_path();
        while !ancestor.exists() {
            let Some(file_name) = ancestor.file_name() else {
                return Err(CommandError::new(
                    "autonomous_tool_path_denied",
                    CommandErrorClass::PolicyDenied,
                    "Xero denied a path that escaped the imported repository.",
                    false,
                ));
            };
            missing_components.push(file_name.to_os_string());
            ancestor = ancestor.parent().ok_or_else(|| {
                CommandError::new(
                    "autonomous_tool_path_denied",
                    CommandErrorClass::PolicyDenied,
                    "Xero denied a path that escaped the imported repository.",
                    false,
                )
            })?;
        }

        let resolved_ancestor = fs::canonicalize(ancestor).map_err(|error| {
            CommandError::retryable(
                "autonomous_tool_path_resolve_failed",
                format!("Xero could not resolve {}: {error}", ancestor.display()),
            )
        })?;
        let mut resolved = self.ensure_inside_root(&resolved_ancestor, relative_path)?;
        for component in missing_components.into_iter().rev() {
            resolved.push(component);
        }
        Ok(resolved)
    }

    fn ensure_inside_root(&self, resolved: &Path, relative_path: &Path) -> CommandResult<PathBuf> {
        if resolved == self.repo_root || resolved.starts_with(&self.repo_root) {
            return Ok(resolved.to_path_buf());
        }

        Err(CommandError::new(
            "autonomous_tool_path_denied",
            CommandErrorClass::PolicyDenied,
            format!(
                "Xero denied access to `{}` because it resolves outside the imported repository root.",
                path_to_forward_slash(relative_path)
            ),
            false,
        ))
    }

    pub(super) fn read_sorted_directory_entries(
        &self,
        scope: &Path,
        error_code: &'static str,
    ) -> CommandResult<Vec<fs::DirEntry>> {
        let mut entries = fs::read_dir(scope)
            .map_err(|error| {
                CommandError::retryable(
                    error_code,
                    format!(
                        "Xero could not enumerate directory {}: {error}",
                        scope.display()
                    ),
                )
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                CommandError::retryable(
                    error_code,
                    format!(
                        "Xero could not enumerate directory {}: {error}",
                        scope.display()
                    ),
                )
            })?;
        entries.sort_by(|left, right| {
            left.file_name()
                .to_string_lossy()
                .cmp(&right.file_name().to_string_lossy())
        });
        Ok(entries)
    }
}

pub fn resolve_imported_repo_root<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
) -> CommandResult<PathBuf> {
    let registry_path = state.global_db_path(app)?;
    resolve_imported_repo_root_from_registry(&registry_path, project_id)
}

pub fn resolve_imported_repo_root_from_registry(
    registry_path: &Path,
    project_id: &str,
) -> CommandResult<PathBuf> {
    crate::db::configure_project_database_paths(registry_path);
    let registry = registry::read_registry(registry_path)?;
    let mut live_root_records = Vec::new();
    let mut candidates = Vec::new();
    let mut pruned_stale_roots = false;

    for record in registry.projects {
        if !Path::new(&record.root_path).is_dir() {
            pruned_stale_roots = true;
            continue;
        }

        if record.project_id == project_id {
            candidates.push(record.clone());
        }
        live_root_records.push(record);
    }

    if pruned_stale_roots {
        let _ = registry::replace_projects(registry_path, live_root_records);
    }

    if candidates.is_empty() {
        return Err(CommandError::project_not_found());
    }

    let mut first_error: Option<CommandError> = None;
    for RegistryProjectRecord {
        project_id,
        root_path,
        ..
    } in candidates
    {
        match project_store::load_project_summary(Path::new(&root_path), &project_id) {
            Ok(_) => return Ok(PathBuf::from(root_path)),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(CommandError::project_not_found))
}

pub(super) fn normalize_relative_path(value: &str, field: &'static str) -> CommandResult<PathBuf> {
    validate_non_empty(value, field)?;
    let mut normalized = PathBuf::new();
    for component in Path::new(value.trim()).components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => continue,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CommandError::new(
                    "autonomous_tool_path_denied",
                    CommandErrorClass::PolicyDenied,
                    format!(
                        "Xero denied `{}` because autonomous tools may only access paths relative to the imported repository root.",
                        value.trim()
                    ),
                    false,
                ));
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(CommandError::invalid_request(field));
    }

    Ok(normalized)
}

pub(super) fn normalize_optional_relative_path(
    value: Option<&str>,
    field: &'static str,
) -> CommandResult<Option<PathBuf>> {
    value
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| normalize_relative_path(path, field))
        .transpose()
}

pub(super) fn normalize_glob_pattern(value: &str) -> CommandResult<String> {
    validate_non_empty(value, "pattern")?;
    let normalized = value.trim().replace('\\', "/");
    let mut segments = Vec::new();

    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." {
            return Err(CommandError::user_fixable(
                "autonomous_tool_find_pattern_invalid",
                format!(
                    "Xero requires glob pattern `{}` to use non-empty repo-relative segments.",
                    value.trim()
                ),
            ));
        }

        if segment == ".." {
            return Err(CommandError::new(
                "autonomous_tool_path_denied",
                CommandErrorClass::PolicyDenied,
                format!(
                    "Xero denied glob pattern `{}` because autonomous tools may only access paths relative to the imported repository root.",
                    value.trim()
                ),
                false,
            ));
        }

        segments.push(segment);
    }

    let normalized = segments.join("/");
    if normalized.is_empty() {
        return Err(CommandError::invalid_request("pattern"));
    }

    Ok(normalized)
}

pub(super) fn build_glob_matcher(pattern: &str) -> CommandResult<GlobMatcher> {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|error| {
            CommandError::user_fixable(
                "autonomous_tool_find_pattern_invalid",
                format!("Xero could not parse glob pattern `{pattern}`: {error}"),
            )
        })
}

pub(super) fn path_to_forward_slash(path: &Path) -> String {
    let parts = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(segment) => Some(segment.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    parts.join("/")
}

pub(super) fn display_relative_or_root(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .map(path_to_forward_slash)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ".".into())
}

pub(super) fn scope_relative_match_path(
    repo_relative: &Path,
    scope_relative: Option<&Path>,
    scope_is_file: bool,
) -> CommandResult<PathBuf> {
    if scope_is_file {
        return repo_relative
            .file_name()
            .map(PathBuf::from)
            .ok_or_else(|| CommandError::invalid_request("path"));
    }

    match scope_relative {
        Some(scope_relative) => repo_relative
            .strip_prefix(scope_relative)
            .map(|relative| relative.to_path_buf())
            .map_err(|_| {
                CommandError::new(
                    "autonomous_tool_path_denied",
                    CommandErrorClass::PolicyDenied,
                    format!(
                        "Xero denied access to `{}` because it escaped the scoped search root.",
                        path_to_forward_slash(repo_relative)
                    ),
                    false,
                )
            }),
        None => Ok(repo_relative.to_path_buf()),
    }
}
