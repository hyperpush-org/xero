use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandError, CommandResult, CreateProjectEntryRequestDto,
        CreateProjectEntryResponseDto, DeleteProjectEntryResponseDto, ListProjectFilesResponseDto,
        ProjectEntryKindDto, ProjectFileNodeDto, ProjectFileRequestDto, ProjectIdRequestDto,
        ReadProjectFileResponseDto, RenameProjectEntryRequestDto,
        RenameProjectEntryResponseDto, WriteProjectFileRequestDto,
        WriteProjectFileResponseDto,
    },
    registry,
    state::DesktopState,
};

const SKIPPED_DIRECTORY_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    ".next",
    "dist",
    "build",
    "target",
    ".turbo",
    ".pnpm-store",
    ".yarn",
    ".cache",
];

#[tauri::command]
pub fn list_project_files<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<ListProjectFilesResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let root = build_tree(&project_root)?;

    Ok(ListProjectFilesResponseDto {
        project_id: request.project_id,
        root,
    })
}

#[tauri::command]
pub fn read_project_file<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectFileRequestDto,
) -> CommandResult<ReadProjectFileResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.path, "path")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let (resolved_path, normalized_path) = resolve_virtual_path(&project_root, &request.path, "path", false)?;
    let metadata = read_metadata(&resolved_path)?;

    if metadata.is_dir() {
        return Err(CommandError::user_fixable(
            "project_file_is_directory",
            format!(
                "Cadence cannot open `{normalized_path}` because it is a directory, not a text file."
            ),
        ));
    }

    let content = fs::read_to_string(&resolved_path).map_err(|error| match error.kind() {
        ErrorKind::InvalidData => CommandError::user_fixable(
            "project_file_not_text",
            format!(
                "Cadence cannot open `{normalized_path}` because it is not a UTF-8 text file."
            ),
        ),
        ErrorKind::NotFound => CommandError::user_fixable(
            "project_file_not_found",
            format!("Cadence could not find `{normalized_path}` in the selected project."),
        ),
        _ => io_error(
            "project_file_read_failed",
            &resolved_path,
            format!(
                "Cadence could not read `{normalized_path}` from the selected project: {error}"
            ),
        ),
    })?;

    Ok(ReadProjectFileResponseDto {
        project_id: request.project_id,
        path: normalized_path,
        content,
    })
}

#[tauri::command]
pub fn write_project_file<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: WriteProjectFileRequestDto,
) -> CommandResult<WriteProjectFileResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.path, "path")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let (resolved_path, normalized_path) = resolve_virtual_path(&project_root, &request.path, "path", false)?;
    let metadata = read_metadata(&resolved_path)?;

    if metadata.is_dir() {
        return Err(CommandError::user_fixable(
            "project_file_is_directory",
            format!(
                "Cadence cannot save `{normalized_path}` because it is a directory, not a text file."
            ),
        ));
    }

    fs::write(&resolved_path, request.content).map_err(|error| match error.kind() {
        ErrorKind::NotFound => CommandError::user_fixable(
            "project_file_not_found",
            format!("Cadence could not find `{normalized_path}` in the selected project."),
        ),
        _ => io_error(
            "project_file_write_failed",
            &resolved_path,
            format!(
                "Cadence could not save `{normalized_path}` in the selected project: {error}"
            ),
        ),
    })?;

    Ok(WriteProjectFileResponseDto {
        project_id: request.project_id,
        path: normalized_path,
    })
}

#[tauri::command]
pub fn create_project_entry<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CreateProjectEntryRequestDto,
) -> CommandResult<CreateProjectEntryResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.parent_path, "parentPath")?;
    let entry_name = validate_entry_name(&request.name, "name")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let (parent_path, normalized_parent_path) =
        resolve_virtual_path(&project_root, &request.parent_path, "parentPath", true)?;
    let parent_metadata = read_metadata(&parent_path)?;

    if !parent_metadata.is_dir() {
        return Err(CommandError::user_fixable(
            "project_parent_not_directory",
            format!(
                "Cadence cannot create a new entry inside `{normalized_parent_path}` because it is not a directory."
            ),
        ));
    }

    let created_path = parent_path.join(&entry_name);
    if created_path.exists() {
        let normalized_path = child_virtual_path(&normalized_parent_path, &entry_name);
        return Err(CommandError::user_fixable(
            "project_entry_exists",
            format!(
                "Cadence cannot create `{normalized_path}` because that path already exists in the selected project."
            ),
        ));
    }

    match request.entry_type {
        ProjectEntryKindDto::File => fs::write(&created_path, ""),
        ProjectEntryKindDto::Folder => fs::create_dir(&created_path),
    }
    .map_err(|error| {
        io_error(
            "project_entry_create_failed",
            &created_path,
            format!(
                "Cadence could not create `{}` in the selected project: {error}",
                created_path.display()
            ),
        )
    })?;

    Ok(CreateProjectEntryResponseDto {
        project_id: request.project_id,
        path: child_virtual_path(&normalized_parent_path, &entry_name),
    })
}

#[tauri::command]
pub fn rename_project_entry<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RenameProjectEntryRequestDto,
) -> CommandResult<RenameProjectEntryResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.path, "path")?;
    let new_name = validate_entry_name(&request.new_name, "newName")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let (resolved_path, normalized_path) = resolve_virtual_path(&project_root, &request.path, "path", false)?;
    read_metadata(&resolved_path)?;

    let parent_path = resolved_path.parent().ok_or_else(|| {
        CommandError::system_fault(
            "project_entry_parent_missing",
            format!(
                "Cadence could not determine the parent directory for `{normalized_path}`."
            ),
        )
    })?;

    let renamed_path = parent_path.join(&new_name);
    if renamed_path.exists() {
        let parent_virtual_path = parent_virtual_path(&normalized_path);
        let normalized_new_path = child_virtual_path(&parent_virtual_path, &new_name);
        return Err(CommandError::user_fixable(
            "project_entry_exists",
            format!(
                "Cadence cannot rename `{normalized_path}` to `{normalized_new_path}` because the destination already exists."
            ),
        ));
    }

    fs::rename(&resolved_path, &renamed_path).map_err(|error| {
        io_error(
            "project_entry_rename_failed",
            &resolved_path,
            format!(
                "Cadence could not rename `{normalized_path}` inside the selected project: {error}"
            ),
        )
    })?;

    Ok(RenameProjectEntryResponseDto {
        project_id: request.project_id,
        path: child_virtual_path(&parent_virtual_path(&normalized_path), &new_name),
    })
}

#[tauri::command]
pub fn delete_project_entry<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectFileRequestDto,
) -> CommandResult<DeleteProjectEntryResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.path, "path")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let (resolved_path, normalized_path) = resolve_virtual_path(&project_root, &request.path, "path", false)?;
    let metadata = read_metadata(&resolved_path)?;

    if metadata.is_dir() {
        fs::remove_dir_all(&resolved_path).map_err(|error| {
            io_error(
                "project_directory_delete_failed",
                &resolved_path,
                format!(
                    "Cadence could not delete `{normalized_path}` from the selected project: {error}"
                ),
            )
        })?;
    } else {
        fs::remove_file(&resolved_path).map_err(|error| {
            io_error(
                "project_file_delete_failed",
                &resolved_path,
                format!(
                    "Cadence could not delete `{normalized_path}` from the selected project: {error}"
                ),
            )
        })?;
    }

    Ok(DeleteProjectEntryResponseDto {
        project_id: request.project_id,
        path: normalized_path,
    })
}

fn resolve_project_root<R: Runtime>(
    app: &AppHandle<R>,
    state: &State<'_, DesktopState>,
    project_id: &str,
) -> CommandResult<PathBuf> {
    let registry_path = state.registry_file(app)?;
    let registry = registry::read_registry(&registry_path)?;
    let mut live_root_records = Vec::new();
    let mut pruned_stale_roots = false;
    let mut resolved_root = None;

    for record in registry.projects {
        if !Path::new(&record.root_path).is_dir() {
            pruned_stale_roots = true;
            continue;
        }

        if record.project_id == project_id && resolved_root.is_none() {
            resolved_root = Some(PathBuf::from(&record.root_path));
        }

        live_root_records.push(record);
    }

    if pruned_stale_roots {
        let _ = registry::replace_projects(&registry_path, live_root_records);
    }

    resolved_root.ok_or_else(CommandError::project_not_found)
}

fn build_tree(project_root: &Path) -> CommandResult<ProjectFileNodeDto> {
    Ok(ProjectFileNodeDto {
        name: "root".into(),
        path: "/".into(),
        r#type: ProjectEntryKindDto::Folder,
        children: read_child_nodes(project_root, "/")?,
    })
}

fn read_child_nodes(directory: &Path, parent_virtual_path: &str) -> CommandResult<Vec<ProjectFileNodeDto>> {
    let mut children = fs::read_dir(directory)
        .map_err(|error| {
            io_error(
                "project_tree_read_failed",
                directory,
                format!(
                    "Cadence could not read the selected project tree at {}: {error}",
                    directory.display()
                ),
            )
        })?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy().into_owned();
            let entry_path = entry.path();
            let metadata = fs::symlink_metadata(&entry_path).ok()?;

            if metadata.file_type().is_symlink() {
                return None;
            }

            if metadata.is_dir() && SKIPPED_DIRECTORY_NAMES.contains(&name.as_str()) {
                return None;
            }

            Some((name, entry_path, metadata.is_dir()))
        })
        .collect::<Vec<_>>();

    children.sort_by(|left, right| match (left.2, right.2) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => left.0.to_lowercase().cmp(&right.0.to_lowercase()),
    });

    children
        .into_iter()
        .map(|(name, path, is_dir)| {
            let virtual_path = child_virtual_path(parent_virtual_path, &name);
            if is_dir {
                Ok(ProjectFileNodeDto {
                    name,
                    path: virtual_path.clone(),
                    r#type: ProjectEntryKindDto::Folder,
                    children: read_child_nodes(&path, &virtual_path)?,
                })
            } else {
                Ok(ProjectFileNodeDto {
                    name,
                    path: virtual_path,
                    r#type: ProjectEntryKindDto::File,
                    children: Vec::new(),
                })
            }
        })
        .collect()
}

fn read_metadata(path: &Path) -> CommandResult<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).map_err(|error| match error.kind() {
        ErrorKind::NotFound => CommandError::user_fixable(
            "project_path_not_found",
            format!("Cadence could not find `{}` in the selected project.", path.display()),
        ),
        _ => io_error(
            "project_path_metadata_failed",
            path,
            format!(
                "Cadence could not inspect `{}` in the selected project: {error}",
                path.display()
            ),
        ),
    })?;

    if metadata.file_type().is_symlink() {
        return Err(CommandError::policy_denied(format!(
            "Cadence refuses to operate on symlinked project paths such as `{}`.",
            path.display()
        )));
    }

    Ok(metadata)
}

fn resolve_virtual_path(
    project_root: &Path,
    raw_path: &str,
    field: &'static str,
    allow_root: bool,
) -> CommandResult<(PathBuf, String)> {
    let segments = split_virtual_path(raw_path, field, allow_root)?;
    let mut resolved = project_root.to_path_buf();
    let mut normalized = String::from("/");

    for segment in segments {
        resolved.push(&segment);
        if resolved.exists() {
            let metadata = read_metadata(&resolved)?;
            if metadata.file_type().is_symlink() {
                return Err(CommandError::policy_denied(format!(
                    "Cadence refuses to follow symlinked project paths such as `{}`.",
                    resolved.display()
                )));
            }
        }

        if normalized.len() > 1 {
            normalized.push('/');
        }
        normalized.push_str(&segment);
    }

    Ok((resolved, normalized))
}

fn split_virtual_path(
    raw_path: &str,
    field: &'static str,
    allow_root: bool,
) -> CommandResult<Vec<String>> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }

    if trimmed == "/" {
        return if allow_root {
            Ok(Vec::new())
        } else {
            Err(CommandError::policy_denied(
                "Cadence cannot operate on the repository root path directly.",
            ))
        };
    }

    let stripped = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let mut segments = Vec::new();
    for segment in stripped.split('/') {
        let normalized = validate_entry_name(segment, field)?;
        segments.push(normalized);
    }

    if segments.is_empty() {
        return Err(CommandError::invalid_request(field));
    }

    Ok(segments)
}

fn validate_entry_name(value: &str, field: &'static str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }

    if trimmed == "." || trimmed == ".." || trimmed.contains('/') || trimmed.contains('\\') {
        return Err(CommandError::policy_denied(format!(
            "Field `{field}` must not contain path traversal or path separator segments."
        )));
    }

    Ok(trimmed.to_owned())
}

fn child_virtual_path(parent_path: &str, child_name: &str) -> String {
    if parent_path == "/" {
        format!("/{child_name}")
    } else {
        format!("{parent_path}/{child_name}")
    }
}

fn parent_virtual_path(path: &str) -> String {
    let mut segments = path.split('/').filter(|segment| !segment.is_empty()).collect::<Vec<_>>();
    segments.pop();
    if segments.is_empty() {
        "/".into()
    } else {
        format!("/{}", segments.join("/"))
    }
}

fn io_error(code: &str, path: &Path, message: String) -> CommandError {
    let normalized_message = if message.is_empty() {
        format!("Cadence hit an I/O error while working with {}.", path.display())
    } else {
        message
    };

    CommandError::retryable(code, normalized_message)
}
