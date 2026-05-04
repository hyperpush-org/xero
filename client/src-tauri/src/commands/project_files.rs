use std::{
    fmt::Write as _,
    fs,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
};

use ignore::{DirEntry, WalkBuilder};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime, State};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    commands::{
        backend_jobs::BackendCancellationToken,
        payload_budget::{
            estimate_serialized_payload_bytes, payload_budget_diagnostic,
            PROJECT_TREE_BUDGET_BYTES, PROJECT_TREE_NODE_BUDGET,
        },
        project_assets::{ProjectAssetGrant, ProjectAssetState},
        validate_non_empty, CommandError, CommandResult, CreateProjectEntryRequestDto,
        CreateProjectEntryResponseDto, DeleteProjectEntryResponseDto, ListProjectFilesRequestDto,
        ListProjectFilesResponseDto, MoveProjectEntryRequestDto, MoveProjectEntryResponseDto,
        ProjectEntryKindDto, ProjectFileNodeDto, ProjectFileRendererKindDto, ProjectFileRequestDto,
        ReadProjectFileResponseDto, RenameProjectEntryRequestDto, RenameProjectEntryResponseDto,
        WriteProjectFileRequestDto, WriteProjectFileResponseDto,
    },
    registry,
    state::DesktopState,
};

pub(crate) const PROJECT_FILE_TEXT_SIZE_LIMIT_BYTES: u64 = 2 * 1024 * 1024;
pub(crate) const PROJECT_FILE_PREVIEW_SIZE_LIMIT_BYTES: u64 = 512 * 1024 * 1024;
const EMPTY_CONTENT_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

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
pub async fn list_project_files<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListProjectFilesRequestDto,
) -> CommandResult<ListProjectFilesResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.path, "path")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let (folder_path, normalized_path) =
        resolve_virtual_path(&project_root, &request.path, "path", true)?;
    let metadata = read_metadata(&folder_path)?;
    if !metadata.is_dir() {
        return Err(CommandError::user_fixable(
            "project_folder_required",
            format!("Xero cannot list `{normalized_path}` because it is a file, not a folder."),
        ));
    }
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id;
    drop(app);

    jobs.run_blocking_latest(
        format!("project-tree:{project_id}:{normalized_path}"),
        "project tree",
        move |cancellation| {
            let built_tree = build_folder_listing(
                &folder_path,
                &normalized_path,
                PROJECT_TREE_NODE_BUDGET,
                &cancellation,
            )?;
            let mut response = ListProjectFilesResponseDto {
                project_id,
                path: normalized_path,
                root: built_tree.root,
                truncated: built_tree.truncated,
                omitted_entry_count: built_tree.omitted_entry_count,
                payload_budget: None,
            };
            let observed_bytes = estimate_serialized_payload_bytes(&response);
            response.payload_budget = payload_budget_diagnostic(
                "project_tree",
                "project tree",
                PROJECT_TREE_BUDGET_BYTES,
                observed_bytes,
                response.truncated,
                false,
            );

            Ok(response)
        },
    )
    .await
}

#[tauri::command]
pub async fn read_project_file<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    asset_state: State<'_, ProjectAssetState>,
    request: ProjectFileRequestDto,
) -> CommandResult<ReadProjectFileResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.path, "path")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let (resolved_path, normalized_path) =
        resolve_virtual_path(&project_root, &request.path, "path", false)?;
    let jobs = state.backend_jobs().clone();
    let asset_state = asset_state.inner().clone();
    let project_id = request.project_id;
    drop(app);

    jobs.run_blocking_latest(
        "project-file-read:visible",
        "project file read",
        move |cancellation| {
            cancellation.check_cancelled("project file read")?;
            read_project_file_at_path(project_id, resolved_path, normalized_path, asset_state)
        },
    )
    .await
}

fn read_project_file_at_path(
    project_id: String,
    resolved_path: PathBuf,
    normalized_path: String,
    asset_state: ProjectAssetState,
) -> CommandResult<ReadProjectFileResponseDto> {
    let metadata = read_metadata(&resolved_path)?;
    read_project_file_at_path_with_limits(
        project_id,
        resolved_path,
        normalized_path,
        metadata,
        asset_state,
        FileContentLimits::default(),
    )
}

fn read_project_file_at_path_with_limits(
    project_id: String,
    resolved_path: PathBuf,
    normalized_path: String,
    metadata: fs::Metadata,
    asset_state: ProjectAssetState,
    limits: FileContentLimits,
) -> CommandResult<ReadProjectFileResponseDto> {
    let byte_length = metadata.len();
    let modified_at = metadata_modified_at(&metadata);
    if metadata.is_dir() {
        return Ok(ReadProjectFileResponseDto::Unsupported {
            project_id,
            path: normalized_path,
            byte_length,
            modified_at,
            content_hash: EMPTY_CONTENT_HASH.into(),
            mime_type: None,
            renderer_kind: None,
            reason: "directory".into(),
        });
    }

    let sniff_bytes = read_file_prefix(&resolved_path, SNIFF_BYTE_LIMIT)?;
    let detected = detect_project_file_type(&resolved_path, &sniff_bytes);
    let content_hash = sha256_file(&resolved_path)?;

    match detected.renderer_kind {
        ProjectFileRendererKindDto::Image
        | ProjectFileRendererKindDto::Pdf
        | ProjectFileRendererKindDto::Audio
        | ProjectFileRendererKindDto::Video => {
            if byte_length > limits.preview_bytes {
                return Ok(ReadProjectFileResponseDto::Unsupported {
                    project_id,
                    path: normalized_path,
                    byte_length,
                    modified_at,
                    content_hash,
                    mime_type: Some(detected.mime_type),
                    renderer_kind: Some(detected.renderer_kind),
                    reason: "too_large_for_preview".into(),
                });
            }

            let preview_url = asset_state.issue_preview_url(ProjectAssetGrant {
                project_id: project_id.clone(),
                path: normalized_path.clone(),
                byte_length,
                modified_at: modified_at.clone(),
                content_hash: content_hash.clone(),
                mime_type: detected.mime_type.clone(),
                renderer_kind: detected.renderer_kind.clone(),
            });

            Ok(ReadProjectFileResponseDto::Renderable {
                project_id,
                path: normalized_path,
                byte_length,
                modified_at,
                content_hash,
                mime_type: detected.mime_type,
                renderer_kind: detected.renderer_kind,
                preview_url,
            })
        }
        ProjectFileRendererKindDto::Code
        | ProjectFileRendererKindDto::Svg
        | ProjectFileRendererKindDto::Markdown
        | ProjectFileRendererKindDto::Csv
        | ProjectFileRendererKindDto::Html => {
            if !detected.is_text {
                return Ok(ReadProjectFileResponseDto::Unsupported {
                    project_id,
                    path: normalized_path,
                    byte_length,
                    modified_at,
                    content_hash,
                    mime_type: Some(detected.mime_type),
                    renderer_kind: Some(detected.renderer_kind),
                    reason: "binary".into(),
                });
            }

            if byte_length > limits.text_bytes {
                return Ok(ReadProjectFileResponseDto::Unsupported {
                    project_id,
                    path: normalized_path,
                    byte_length,
                    modified_at,
                    content_hash,
                    mime_type: Some(detected.mime_type),
                    renderer_kind: Some(detected.renderer_kind),
                    reason: "too_large_for_text_editing".into(),
                });
            }

            let bytes = fs::read(&resolved_path).map_err(|error| match error.kind() {
                ErrorKind::NotFound => CommandError::user_fixable(
                    "project_file_not_found",
                    format!("Xero could not find `{normalized_path}` in the selected project."),
                ),
                _ => io_error(
                    "project_file_read_failed",
                    &resolved_path,
                    format!(
                        "Xero could not read `{normalized_path}` from the selected project: {error}"
                    ),
                ),
            })?;
            let text = match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(_) => {
                    return Ok(ReadProjectFileResponseDto::Unsupported {
                        project_id,
                        path: normalized_path,
                        byte_length,
                        modified_at,
                        content_hash,
                        mime_type: Some(detected.mime_type),
                        renderer_kind: Some(detected.renderer_kind),
                        reason: "binary".into(),
                    });
                }
            };

            Ok(ReadProjectFileResponseDto::Text {
                project_id,
                path: normalized_path,
                byte_length,
                modified_at,
                content_hash,
                mime_type: detected.mime_type,
                renderer_kind: detected.renderer_kind,
                text,
            })
        }
    }
}

const SNIFF_BYTE_LIMIT: usize = 8 * 1024;

#[derive(Debug, Clone, Copy)]
struct FileContentLimits {
    text_bytes: u64,
    preview_bytes: u64,
}

impl Default for FileContentLimits {
    fn default() -> Self {
        Self {
            text_bytes: PROJECT_FILE_TEXT_SIZE_LIMIT_BYTES,
            preview_bytes: PROJECT_FILE_PREVIEW_SIZE_LIMIT_BYTES,
        }
    }
}

#[derive(Debug, Clone)]
struct DetectedProjectFileType {
    mime_type: String,
    renderer_kind: ProjectFileRendererKindDto,
    is_text: bool,
}

fn detect_project_file_type(path: &Path, sniff_bytes: &[u8]) -> DetectedProjectFileType {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let guessed_mime = mime_type_for_extension(&extension).or_else(|| {
        mime_guess::from_path(path)
            .first_raw()
            .map(ToOwned::to_owned)
    });
    let sniffed = sniff_binary_type(sniff_bytes, &extension);

    if let Some((mime_type, renderer_kind)) = sniffed {
        return DetectedProjectFileType {
            mime_type,
            renderer_kind,
            is_text: false,
        };
    }

    if extension == "svg" || looks_like_svg(sniff_bytes) {
        return DetectedProjectFileType {
            mime_type: "image/svg+xml".into(),
            renderer_kind: ProjectFileRendererKindDto::Svg,
            is_text: looks_like_text(sniff_bytes),
        };
    }

    if is_markdown_extension(&extension) {
        return DetectedProjectFileType {
            mime_type: "text/markdown; charset=utf-8".into(),
            renderer_kind: ProjectFileRendererKindDto::Markdown,
            is_text: looks_like_text(sniff_bytes),
        };
    }

    if extension == "csv" || extension == "tsv" {
        return DetectedProjectFileType {
            mime_type: if extension == "tsv" {
                "text/tab-separated-values; charset=utf-8".into()
            } else {
                "text/csv; charset=utf-8".into()
            },
            renderer_kind: ProjectFileRendererKindDto::Csv,
            is_text: looks_like_text(sniff_bytes),
        };
    }

    if matches!(extension.as_str(), "html" | "htm") {
        return DetectedProjectFileType {
            mime_type: "text/html; charset=utf-8".into(),
            renderer_kind: ProjectFileRendererKindDto::Html,
            is_text: looks_like_text(sniff_bytes),
        };
    }

    if is_audio_extension(&extension) {
        return DetectedProjectFileType {
            mime_type: guessed_mime.unwrap_or_else(|| "application/octet-stream".into()),
            renderer_kind: ProjectFileRendererKindDto::Audio,
            is_text: false,
        };
    }

    if is_video_extension(&extension) {
        return DetectedProjectFileType {
            mime_type: guessed_mime.unwrap_or_else(|| "application/octet-stream".into()),
            renderer_kind: ProjectFileRendererKindDto::Video,
            is_text: false,
        };
    }

    if let Some(mime_type) = guessed_mime {
        if mime_type.starts_with("image/") {
            return DetectedProjectFileType {
                mime_type,
                renderer_kind: ProjectFileRendererKindDto::Image,
                is_text: false,
            };
        }
        if mime_type == "application/pdf" {
            return DetectedProjectFileType {
                mime_type,
                renderer_kind: ProjectFileRendererKindDto::Pdf,
                is_text: false,
            };
        }
        if mime_type.starts_with("audio/") {
            return DetectedProjectFileType {
                mime_type,
                renderer_kind: ProjectFileRendererKindDto::Audio,
                is_text: false,
            };
        }
        if mime_type.starts_with("video/") {
            return DetectedProjectFileType {
                mime_type,
                renderer_kind: ProjectFileRendererKindDto::Video,
                is_text: false,
            };
        }
        if is_text_mime(&mime_type) {
            return DetectedProjectFileType {
                mime_type: ensure_charset(&mime_type),
                renderer_kind: ProjectFileRendererKindDto::Code,
                is_text: looks_like_text(sniff_bytes),
            };
        }
    }

    let is_text = looks_like_text(sniff_bytes);
    DetectedProjectFileType {
        mime_type: if is_text {
            "text/plain; charset=utf-8".into()
        } else {
            "application/octet-stream".into()
        },
        renderer_kind: ProjectFileRendererKindDto::Code,
        is_text,
    }
}

fn mime_type_for_extension(extension: &str) -> Option<String> {
    match extension {
        "md" | "markdown" | "mdown" | "mkd" => Some("text/markdown".into()),
        "csv" => Some("text/csv".into()),
        "tsv" => Some("text/tab-separated-values".into()),
        "svg" => Some("image/svg+xml".into()),
        "html" | "htm" => Some("text/html".into()),
        "txt" | "log" => Some("text/plain".into()),
        _ => None,
    }
}

fn sniff_binary_type(
    bytes: &[u8],
    extension: &str,
) -> Option<(String, ProjectFileRendererKindDto)> {
    if bytes.starts_with(b"%PDF-") {
        return Some(("application/pdf".into(), ProjectFileRendererKindDto::Pdf));
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some(("image/png".into(), ProjectFileRendererKindDto::Image));
    }
    if bytes.len() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff {
        return Some(("image/jpeg".into(), ProjectFileRendererKindDto::Image));
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(("image/gif".into(), ProjectFileRendererKindDto::Image));
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(("image/webp".into(), ProjectFileRendererKindDto::Image));
    }
    if bytes.starts_with(b"BM") {
        return Some(("image/bmp".into(), ProjectFileRendererKindDto::Image));
    }
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some(("image/x-icon".into(), ProjectFileRendererKindDto::Image));
    }
    if bytes.starts_with(b"ID3")
        || (bytes.len() >= 2 && bytes[0] == 0xff && (bytes[1] & 0xe0) == 0xe0)
    {
        return Some(("audio/mpeg".into(), ProjectFileRendererKindDto::Audio));
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WAVE" {
        return Some(("audio/wav".into(), ProjectFileRendererKindDto::Audio));
    }
    if bytes.starts_with(b"OggS") {
        return Some((
            if extension == "ogv" {
                "video/ogg".into()
            } else {
                "audio/ogg".into()
            },
            if extension == "ogv" {
                ProjectFileRendererKindDto::Video
            } else {
                ProjectFileRendererKindDto::Audio
            },
        ));
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        return Some(("video/mp4".into(), ProjectFileRendererKindDto::Video));
    }
    if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        return Some((
            if extension == "webm" {
                "video/webm".into()
            } else {
                "application/octet-stream".into()
            },
            if extension == "webm" {
                ProjectFileRendererKindDto::Video
            } else {
                ProjectFileRendererKindDto::Code
            },
        ));
    }

    None
}

fn is_markdown_extension(extension: &str) -> bool {
    matches!(extension, "md" | "markdown" | "mdown" | "mkd" | "mdx")
}

fn is_audio_extension(extension: &str) -> bool {
    matches!(
        extension,
        "mp3" | "wav" | "ogg" | "oga" | "flac" | "m4a" | "aac" | "opus"
    )
}

fn is_video_extension(extension: &str) -> bool {
    matches!(
        extension,
        "mp4" | "m4v" | "mov" | "webm" | "ogv" | "avi" | "mkv"
    )
}

fn is_text_mime(mime_type: &str) -> bool {
    mime_type.starts_with("text/")
        || matches!(
            mime_type,
            "application/json"
                | "application/javascript"
                | "application/typescript"
                | "application/xml"
                | "application/x-yaml"
                | "application/toml"
                | "application/graphql"
        )
}

fn ensure_charset(mime_type: &str) -> String {
    if mime_type.starts_with("text/") && !mime_type.contains("charset=") {
        format!("{mime_type}; charset=utf-8")
    } else {
        mime_type.to_owned()
    }
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let trimmed = trim_utf8_bom_and_ascii_ws(bytes);
    trimmed.starts_with(b"<svg")
        || trimmed.starts_with(b"<?xml")
            && trimmed
                .windows(4)
                .any(|window| window.eq_ignore_ascii_case(b"<svg"))
}

fn looks_like_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    if bytes.contains(&0) || std::str::from_utf8(bytes).is_err() {
        return false;
    }

    bytes
        .iter()
        .all(|byte| matches!(*byte, b'\n' | b'\r' | b'\t') || *byte >= 0x20 && *byte != 0x7f)
}

fn trim_utf8_bom_and_ascii_ws(bytes: &[u8]) -> &[u8] {
    let without_bom = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let mut start = 0;
    while start < without_bom.len() && without_bom[start].is_ascii_whitespace() {
        start += 1;
    }
    &without_bom[start..]
}

fn read_file_prefix(path: &Path, limit: usize) -> CommandResult<Vec<u8>> {
    let mut file = fs::File::open(path).map_err(|error| match error.kind() {
        ErrorKind::NotFound => CommandError::user_fixable(
            "project_file_not_found",
            format!(
                "Xero could not find `{}` in the selected project.",
                path.display()
            ),
        ),
        _ => io_error(
            "project_file_read_failed",
            path,
            format!(
                "Xero could not read `{}` from the selected project: {error}",
                path.display()
            ),
        ),
    })?;
    let mut bytes = Vec::with_capacity(limit);
    let mut handle = file.by_ref().take(limit as u64);
    handle.read_to_end(&mut bytes).map_err(|error| {
        io_error(
            "project_file_read_failed",
            path,
            format!(
                "Xero could not read `{}` from the selected project: {error}",
                path.display()
            ),
        )
    })?;
    Ok(bytes)
}

fn sha256_file(path: &Path) -> CommandResult<String> {
    let mut file = fs::File::open(path).map_err(|error| match error.kind() {
        ErrorKind::NotFound => CommandError::user_fixable(
            "project_file_not_found",
            format!(
                "Xero could not find `{}` in the selected project.",
                path.display()
            ),
        ),
        _ => io_error(
            "project_file_hash_failed",
            path,
            format!(
                "Xero could not hash `{}` in the selected project: {error}",
                path.display()
            ),
        ),
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            io_error(
                "project_file_hash_failed",
                path,
                format!(
                    "Xero could not hash `{}` in the selected project: {error}",
                    path.display()
                ),
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_digest(hasher.finalize().as_slice()))
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

pub(crate) fn metadata_modified_at(metadata: &fs::Metadata) -> String {
    metadata
        .modified()
        .ok()
        .map(OffsetDateTime::from)
        .and_then(|value| value.format(&Rfc3339).ok())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".into())
}

#[tauri::command]
pub async fn write_project_file<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: WriteProjectFileRequestDto,
) -> CommandResult<WriteProjectFileResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.path, "path")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let (resolved_path, normalized_path) =
        resolve_virtual_path(&project_root, &request.path, "path", false)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id;
    let content = request.content;
    drop(app);

    jobs.run_blocking_project_lane(
        project_id.clone(),
        "file",
        "project file write",
        move || write_project_file_at_path(project_id, resolved_path, normalized_path, content),
    )
    .await
}

fn write_project_file_at_path(
    project_id: String,
    resolved_path: PathBuf,
    normalized_path: String,
    content: String,
) -> CommandResult<WriteProjectFileResponseDto> {
    let metadata = read_metadata(&resolved_path)?;

    if metadata.is_dir() {
        return Err(CommandError::user_fixable(
            "project_file_is_directory",
            format!(
                "Xero cannot save `{normalized_path}` because it is a directory, not a text file."
            ),
        ));
    }

    fs::write(&resolved_path, content).map_err(|error| match error.kind() {
        ErrorKind::NotFound => CommandError::user_fixable(
            "project_file_not_found",
            format!("Xero could not find `{normalized_path}` in the selected project."),
        ),
        _ => io_error(
            "project_file_write_failed",
            &resolved_path,
            format!("Xero could not save `{normalized_path}` in the selected project: {error}"),
        ),
    })?;

    Ok(WriteProjectFileResponseDto {
        project_id,
        path: normalized_path,
    })
}

#[tauri::command]
pub async fn open_project_file_external<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectFileRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.path, "path")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let (resolved_path, normalized_path) =
        resolve_virtual_path(&project_root, &request.path, "path", false)?;
    let metadata = read_metadata(&resolved_path)?;
    if !metadata.is_file() {
        return Err(CommandError::user_fixable(
            "project_file_required",
            format!("Xero cannot open `{normalized_path}` externally because it is not a file."),
        ));
    }

    tauri_plugin_opener::open_path(&resolved_path, None::<&str>).map_err(|error| {
        CommandError::system_fault(
            "project_file_open_external_failed",
            format!("Xero could not open `{normalized_path}` in the system viewer: {error}"),
        )
    })?;

    Ok(())
}

#[tauri::command]
pub async fn create_project_entry<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CreateProjectEntryRequestDto,
) -> CommandResult<CreateProjectEntryResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.parent_path, "parentPath")?;
    let entry_name = validate_entry_name(&request.name, "name")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id.clone();
    drop(app);

    jobs.run_blocking_project_lane(project_id, "file", "project entry create", move || {
        create_project_entry_at_root(project_root, request, entry_name)
    })
    .await
}

fn create_project_entry_at_root(
    project_root: PathBuf,
    request: CreateProjectEntryRequestDto,
    entry_name: String,
) -> CommandResult<CreateProjectEntryResponseDto> {
    let (parent_path, normalized_parent_path) =
        resolve_virtual_path(&project_root, &request.parent_path, "parentPath", true)?;
    let parent_metadata = read_metadata(&parent_path)?;

    if !parent_metadata.is_dir() {
        return Err(CommandError::user_fixable(
            "project_parent_not_directory",
            format!(
                "Xero cannot create a new entry inside `{normalized_parent_path}` because it is not a directory."
            ),
        ));
    }

    let created_path = parent_path.join(&entry_name);
    if created_path.exists() {
        let normalized_path = child_virtual_path(&normalized_parent_path, &entry_name);
        return Err(CommandError::user_fixable(
            "project_entry_exists",
            format!(
                "Xero cannot create `{normalized_path}` because that path already exists in the selected project."
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
                "Xero could not create `{}` in the selected project: {error}",
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
pub async fn rename_project_entry<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RenameProjectEntryRequestDto,
) -> CommandResult<RenameProjectEntryResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.path, "path")?;
    let new_name = validate_entry_name(&request.new_name, "newName")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id.clone();
    drop(app);

    jobs.run_blocking_project_lane(project_id, "file", "project entry rename", move || {
        rename_project_entry_at_root(project_root, request, new_name)
    })
    .await
}

fn rename_project_entry_at_root(
    project_root: PathBuf,
    request: RenameProjectEntryRequestDto,
    new_name: String,
) -> CommandResult<RenameProjectEntryResponseDto> {
    let (resolved_path, normalized_path) =
        resolve_virtual_path(&project_root, &request.path, "path", false)?;
    read_metadata(&resolved_path)?;

    let parent_path = resolved_path.parent().ok_or_else(|| {
        CommandError::system_fault(
            "project_entry_parent_missing",
            format!("Xero could not determine the parent directory for `{normalized_path}`."),
        )
    })?;

    let renamed_path = parent_path.join(&new_name);
    if renamed_path.exists() {
        let parent_virtual_path = parent_virtual_path(&normalized_path);
        let normalized_new_path = child_virtual_path(&parent_virtual_path, &new_name);
        return Err(CommandError::user_fixable(
            "project_entry_exists",
            format!(
                "Xero cannot rename `{normalized_path}` to `{normalized_new_path}` because the destination already exists."
            ),
        ));
    }

    fs::rename(&resolved_path, &renamed_path).map_err(|error| {
        io_error(
            "project_entry_rename_failed",
            &resolved_path,
            format!(
                "Xero could not rename `{normalized_path}` inside the selected project: {error}"
            ),
        )
    })?;

    Ok(RenameProjectEntryResponseDto {
        project_id: request.project_id,
        path: child_virtual_path(&parent_virtual_path(&normalized_path), &new_name),
    })
}

#[tauri::command]
pub async fn move_project_entry<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: MoveProjectEntryRequestDto,
) -> CommandResult<MoveProjectEntryResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.path, "path")?;
    validate_non_empty(&request.target_parent_path, "targetParentPath")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id.clone();
    drop(app);

    jobs.run_blocking_project_lane(project_id, "file", "project entry move", move || {
        move_project_entry_at_root(project_root, request)
    })
    .await
}

fn move_project_entry_at_root(
    project_root: PathBuf,
    request: MoveProjectEntryRequestDto,
) -> CommandResult<MoveProjectEntryResponseDto> {
    let (resolved_path, normalized_path) =
        resolve_virtual_path(&project_root, &request.path, "path", false)?;
    read_metadata(&resolved_path)?;

    let (target_parent_path, normalized_target_parent_path) = resolve_virtual_path(
        &project_root,
        &request.target_parent_path,
        "targetParentPath",
        true,
    )?;
    let target_parent_metadata = read_metadata(&target_parent_path)?;

    if !target_parent_metadata.is_dir() {
        return Err(CommandError::user_fixable(
            "project_target_parent_not_directory",
            format!(
                "Xero cannot move `{normalized_path}` into `{normalized_target_parent_path}` because the target is not a directory."
            ),
        ));
    }

    let current_parent_path = parent_virtual_path(&normalized_path);
    if normalized_target_parent_path == current_parent_path {
        return Ok(MoveProjectEntryResponseDto {
            project_id: request.project_id,
            path: normalized_path,
        });
    }

    if normalized_target_parent_path == normalized_path
        || normalized_target_parent_path.starts_with(&format!("{normalized_path}/"))
    {
        return Err(CommandError::user_fixable(
            "project_move_into_self",
            format!("Xero cannot move `{normalized_path}` into itself or one of its descendants."),
        ));
    }

    let entry_name = resolved_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            CommandError::system_fault(
                "project_entry_name_missing",
                format!("Xero could not determine the name for `{normalized_path}`."),
            )
        })?
        .to_owned();
    let destination_path = target_parent_path.join(&entry_name);
    let normalized_destination_path =
        child_virtual_path(&normalized_target_parent_path, &entry_name);

    if destination_path.exists() {
        return Err(CommandError::user_fixable(
            "project_entry_exists",
            format!(
                "Xero cannot move `{normalized_path}` to `{normalized_destination_path}` because the destination already exists."
            ),
        ));
    }

    fs::rename(&resolved_path, &destination_path).map_err(|error| {
        io_error(
            "project_entry_move_failed",
            &resolved_path,
            format!("Xero could not move `{normalized_path}` inside the selected project: {error}"),
        )
    })?;

    Ok(MoveProjectEntryResponseDto {
        project_id: request.project_id,
        path: normalized_destination_path,
    })
}

#[tauri::command]
pub async fn delete_project_entry<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectFileRequestDto,
) -> CommandResult<DeleteProjectEntryResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.path, "path")?;

    let project_root = resolve_project_root(&app, &state, &request.project_id)?;
    let (resolved_path, normalized_path) =
        resolve_virtual_path(&project_root, &request.path, "path", false)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id;
    drop(app);

    jobs.run_blocking_project_lane(
        project_id.clone(),
        "file",
        "project entry delete",
        move || delete_project_entry_at_path(project_id, resolved_path, normalized_path),
    )
    .await
}

fn delete_project_entry_at_path(
    project_id: String,
    resolved_path: PathBuf,
    normalized_path: String,
) -> CommandResult<DeleteProjectEntryResponseDto> {
    let metadata = read_metadata(&resolved_path)?;

    if metadata.is_dir() {
        fs::remove_dir_all(&resolved_path).map_err(|error| {
            io_error(
                "project_directory_delete_failed",
                &resolved_path,
                format!(
                    "Xero could not delete `{normalized_path}` from the selected project: {error}"
                ),
            )
        })?;
    } else {
        fs::remove_file(&resolved_path).map_err(|error| {
            io_error(
                "project_file_delete_failed",
                &resolved_path,
                format!(
                    "Xero could not delete `{normalized_path}` from the selected project: {error}"
                ),
            )
        })?;
    }

    Ok(DeleteProjectEntryResponseDto {
        project_id,
        path: normalized_path,
    })
}

pub(crate) fn resolve_project_root<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
) -> CommandResult<PathBuf> {
    let registry_path = state.global_db_path(app)?;
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

struct BuiltProjectTree {
    root: ProjectFileNodeDto,
    truncated: bool,
    omitted_entry_count: u32,
}

fn build_folder_listing(
    directory: &Path,
    parent_virtual_path: &str,
    node_budget: usize,
    cancellation: &BackendCancellationToken,
) -> CommandResult<BuiltProjectTree> {
    cancellation.check_cancelled("project tree")?;
    let ListingChildren {
        children,
        truncated,
        omitted_entry_count,
    } = read_child_nodes(directory, parent_virtual_path, node_budget, cancellation)?;
    let root = ProjectFileNodeDto {
        name: if parent_virtual_path == "/" {
            "root".into()
        } else {
            directory
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("folder")
                .to_owned()
        },
        path: parent_virtual_path.into(),
        r#type: ProjectEntryKindDto::Folder,
        children,
        children_loaded: true,
        truncated,
        omitted_entry_count,
    };

    Ok(BuiltProjectTree {
        root,
        truncated,
        omitted_entry_count,
    })
}

struct ListingChildren {
    children: Vec<ProjectFileNodeDto>,
    truncated: bool,
    omitted_entry_count: u32,
}

fn read_child_nodes(
    directory: &Path,
    parent_virtual_path: &str,
    node_budget: usize,
    cancellation: &BackendCancellationToken,
) -> CommandResult<ListingChildren> {
    cancellation.check_cancelled("project tree")?;
    let mut walk_builder = WalkBuilder::new(directory);
    walk_builder
        .max_depth(Some(1))
        .hidden(true)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .parents(true)
        .follow_links(false)
        .filter_entry(|entry| !is_skipped_project_directory_entry(entry));

    let mut children = Vec::new();
    for entry in walk_builder.build() {
        cancellation.check_cancelled("project tree")?;
        let Ok(entry) = entry else { continue };
        if entry.depth() == 0 {
            continue;
        }

        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = file_type.is_dir();
        children.push((name, is_dir));
    }

    children.sort_by(|left, right| match (left.1, right.1) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => left.0.to_lowercase().cmp(&right.0.to_lowercase()),
    });

    let mut omitted_entry_count = 0_u32;
    let nodes = children
        .into_iter()
        .enumerate()
        .filter_map(|(index, (name, is_dir))| {
            if index >= node_budget {
                omitted_entry_count = omitted_entry_count.saturating_add(1);
                return None;
            }

            let virtual_path = child_virtual_path(parent_virtual_path, &name);
            Some(ProjectFileNodeDto {
                name,
                path: virtual_path,
                r#type: if is_dir {
                    ProjectEntryKindDto::Folder
                } else {
                    ProjectEntryKindDto::File
                },
                children: Vec::new(),
                children_loaded: !is_dir,
                truncated: false,
                omitted_entry_count: 0,
            })
        })
        .collect::<Vec<_>>();

    Ok(ListingChildren {
        children: nodes,
        truncated: omitted_entry_count > 0,
        omitted_entry_count,
    })
}

pub(crate) fn is_skipped_project_directory_entry(entry: &DirEntry) -> bool {
    entry
        .file_type()
        .map(|file_type| {
            file_type.is_dir()
                && is_skipped_project_directory_name(&entry.file_name().to_string_lossy())
        })
        .unwrap_or(false)
}

pub(crate) fn is_skipped_project_directory_name(name: &str) -> bool {
    SKIPPED_DIRECTORY_NAMES.contains(&name)
}

pub(crate) fn read_metadata(path: &Path) -> CommandResult<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).map_err(|error| match error.kind() {
        ErrorKind::NotFound => CommandError::user_fixable(
            "project_path_not_found",
            format!(
                "Xero could not find `{}` in the selected project.",
                path.display()
            ),
        ),
        _ => io_error(
            "project_path_metadata_failed",
            path,
            format!(
                "Xero could not inspect `{}` in the selected project: {error}",
                path.display()
            ),
        ),
    })?;

    if metadata.file_type().is_symlink() {
        return Err(CommandError::policy_denied(format!(
            "Xero refuses to operate on symlinked project paths such as `{}`.",
            path.display()
        )));
    }

    Ok(metadata)
}

pub(crate) fn resolve_virtual_path(
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
                    "Xero refuses to follow symlinked project paths such as `{}`.",
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
                "Xero cannot operate on the repository root path directly.",
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
    let mut segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    segments.pop();
    if segments.is_empty() {
        "/".into()
    } else {
        format!("/{}", segments.join("/"))
    }
}

fn io_error(code: &str, path: &Path, message: String) -> CommandError {
    let normalized_message = if message.is_empty() {
        format!(
            "Xero hit an I/O error while working with {}.",
            path.display()
        )
    } else {
        message
    };

    CommandError::retryable(code, normalized_message)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::commands::{
        backend_jobs::BackendCancellationToken, ProjectFileRendererKindDto,
        ReadProjectFileResponseDto,
    };

    use super::{
        build_folder_listing, detect_project_file_type, read_metadata,
        read_project_file_at_path_with_limits, resolve_virtual_path, FileContentLimits,
    };

    #[test]
    fn project_tree_lists_only_the_requested_folder() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        fs::create_dir(temp_dir.path().join("src")).expect("src dir");
        fs::write(temp_dir.path().join("src").join("main.rs"), "fn main() {}").expect("main");
        fs::write(temp_dir.path().join("README.md"), "# Xero").expect("readme");

        let root = build_folder_listing(
            temp_dir.path(),
            "/",
            100,
            &BackendCancellationToken::default(),
        )
        .expect("root listing");
        let src = build_folder_listing(
            &temp_dir.path().join("src"),
            "/src",
            100,
            &BackendCancellationToken::default(),
        )
        .expect("src listing");

        assert_eq!(
            root.root
                .children
                .iter()
                .map(|node| (node.path.as_str(), node.children_loaded))
                .collect::<Vec<_>>(),
            vec![("/src", false), ("/README.md", true)]
        );
        assert_eq!(
            src.root
                .children
                .iter()
                .map(|node| node.path.as_str())
                .collect::<Vec<_>>(),
            vec!["/src/main.rs"]
        );
    }

    #[test]
    fn project_tree_applies_ignore_rules_and_skipped_directory_names() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        fs::create_dir(temp_dir.path().join(".git")).expect("git dir");
        fs::write(
            temp_dir.path().join(".gitignore"),
            "ignored.txt\nignored_dir/\n",
        )
        .expect("gitignore");
        fs::write(temp_dir.path().join("visible.txt"), "visible").expect("visible");
        fs::write(temp_dir.path().join("ignored.txt"), "ignored").expect("ignored");
        fs::create_dir(temp_dir.path().join("ignored_dir")).expect("ignored dir");
        fs::create_dir(temp_dir.path().join("node_modules")).expect("node_modules");

        let listing = build_folder_listing(
            temp_dir.path(),
            "/",
            100,
            &BackendCancellationToken::default(),
        )
        .expect("listing");
        let paths = listing
            .root
            .children
            .iter()
            .map(|node| node.path.as_str())
            .collect::<Vec<_>>();

        assert_eq!(paths, vec!["/visible.txt"]);
    }

    #[test]
    fn project_tree_marks_payload_truncation_when_node_budget_is_exhausted() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        fs::write(temp_dir.path().join("a.txt"), "a").expect("write a");
        fs::write(temp_dir.path().join("b.txt"), "b").expect("write b");
        fs::write(temp_dir.path().join("c.txt"), "c").expect("write c");

        let tree = build_folder_listing(
            temp_dir.path(),
            "/",
            2,
            &BackendCancellationToken::default(),
        )
        .expect("tree");

        assert!(tree.truncated);
        assert_eq!(tree.root.children.len(), 2);
        assert_eq!(tree.omitted_entry_count, 1);
        assert_eq!(tree.root.omitted_entry_count, 1);
    }

    #[test]
    fn project_tree_rejects_unsafe_virtual_paths() {
        let temp_dir = tempfile::tempdir().expect("temp dir");

        assert!(resolve_virtual_path(temp_dir.path(), "/../escape", "path", true).is_err());
        assert!(resolve_virtual_path(temp_dir.path(), "/safe/../escape", "path", true).is_err());
        assert!(resolve_virtual_path(temp_dir.path(), "/", "path", true).is_ok());
    }

    #[test]
    fn project_file_classification_reads_utf8_text() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("notes.md");
        fs::write(&path, "# Xero\n").expect("write markdown");

        let response = read_project_file_at_path_with_limits(
            "project-1".into(),
            path.clone(),
            "/notes.md".into(),
            read_metadata(&path).expect("metadata"),
            crate::commands::project_assets::ProjectAssetState::default(),
            FileContentLimits {
                text_bytes: 1024,
                preview_bytes: 1024,
            },
        )
        .expect("classified response");

        match response {
            ReadProjectFileResponseDto::Text {
                text,
                renderer_kind,
                mime_type,
                byte_length,
                ..
            } => {
                assert_eq!(text, "# Xero\n");
                assert_eq!(renderer_kind, ProjectFileRendererKindDto::Markdown);
                assert_eq!(mime_type, "text/markdown; charset=utf-8");
                assert_eq!(byte_length, 7);
            }
            other => panic!("expected text response, got {other:?}"),
        }
    }

    #[test]
    fn project_file_classification_returns_unsupported_for_unknown_binary() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("payload.bin");
        fs::write(&path, [0_u8, 0xff, 0x01, 0x02]).expect("write binary");

        let response = read_project_file_at_path_with_limits(
            "project-1".into(),
            path.clone(),
            "/payload.bin".into(),
            read_metadata(&path).expect("metadata"),
            crate::commands::project_assets::ProjectAssetState::default(),
            FileContentLimits {
                text_bytes: 1024,
                preview_bytes: 1024,
            },
        )
        .expect("classified response");

        match response {
            ReadProjectFileResponseDto::Unsupported {
                reason, mime_type, ..
            } => {
                assert_eq!(reason, "binary");
                assert_eq!(mime_type.as_deref(), Some("application/octet-stream"));
            }
            other => panic!("expected unsupported response, got {other:?}"),
        }
    }

    #[test]
    fn project_file_classification_detects_svg_as_text_backed_preview() {
        let detected =
            detect_project_file_type(std::path::Path::new("logo.svg"), br#"<svg></svg>"#);

        assert!(detected.is_text);
        assert_eq!(detected.mime_type, "image/svg+xml");
        assert_eq!(detected.renderer_kind, ProjectFileRendererKindDto::Svg);
    }

    #[test]
    fn project_file_classification_detects_raster_images_as_renderable() {
        let detected = detect_project_file_type(
            std::path::Path::new("pixel.png"),
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
        );

        assert!(!detected.is_text);
        assert_eq!(detected.mime_type, "image/png");
        assert_eq!(detected.renderer_kind, ProjectFileRendererKindDto::Image);
    }

    #[test]
    fn project_file_classification_detects_pdf_audio_and_video_as_renderable() {
        let pdf = detect_project_file_type(std::path::Path::new("paper.pdf"), b"%PDF-1.7\n");
        assert!(!pdf.is_text);
        assert_eq!(pdf.mime_type, "application/pdf");
        assert_eq!(pdf.renderer_kind, ProjectFileRendererKindDto::Pdf);

        let audio = detect_project_file_type(std::path::Path::new("theme.mp3"), b"ID3\x03\x00");
        assert!(!audio.is_text);
        assert_eq!(audio.mime_type, "audio/mpeg");
        assert_eq!(audio.renderer_kind, ProjectFileRendererKindDto::Audio);

        let video = detect_project_file_type(
            std::path::Path::new("demo.mp4"),
            &[
                0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm',
            ],
        );
        assert!(!video.is_text);
        assert_eq!(video.mime_type, "video/mp4");
        assert_eq!(video.renderer_kind, ProjectFileRendererKindDto::Video);
    }

    #[test]
    fn project_file_classification_keeps_unsafe_svg_text_backed() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("unsafe.svg");
        fs::write(
            &path,
            r#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#,
        )
        .expect("write svg");

        let response = read_project_file_at_path_with_limits(
            "project-1".into(),
            path.clone(),
            "/unsafe.svg".into(),
            read_metadata(&path).expect("metadata"),
            crate::commands::project_assets::ProjectAssetState::default(),
            FileContentLimits {
                text_bytes: 1024,
                preview_bytes: 1024,
            },
        )
        .expect("classified response");

        match response {
            ReadProjectFileResponseDto::Text {
                text,
                renderer_kind,
                mime_type,
                ..
            } => {
                assert!(text.contains("<script>"));
                assert_eq!(renderer_kind, ProjectFileRendererKindDto::Svg);
                assert_eq!(mime_type, "image/svg+xml");
            }
            other => panic!("expected SVG text response, got {other:?}"),
        }
    }

    #[test]
    fn project_file_classification_enforces_text_size_limit() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("large.txt");
        fs::write(&path, "abcd").expect("write text");

        let response = read_project_file_at_path_with_limits(
            "project-1".into(),
            path.clone(),
            "/large.txt".into(),
            read_metadata(&path).expect("metadata"),
            crate::commands::project_assets::ProjectAssetState::default(),
            FileContentLimits {
                text_bytes: 3,
                preview_bytes: 1024,
            },
        )
        .expect("classified response");

        match response {
            ReadProjectFileResponseDto::Unsupported { reason, .. } => {
                assert_eq!(reason, "too_large_for_text_editing");
            }
            other => panic!("expected unsupported response, got {other:?}"),
        }
    }

    #[test]
    fn project_file_classification_enforces_preview_size_limit() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("large.png");
        fs::write(&path, [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]).expect("write image");

        let response = read_project_file_at_path_with_limits(
            "project-1".into(),
            path.clone(),
            "/large.png".into(),
            read_metadata(&path).expect("metadata"),
            crate::commands::project_assets::ProjectAssetState::default(),
            FileContentLimits {
                text_bytes: 1024,
                preview_bytes: 3,
            },
        )
        .expect("classified response");

        match response {
            ReadProjectFileResponseDto::Unsupported {
                reason,
                renderer_kind,
                ..
            } => {
                assert_eq!(reason, "too_large_for_preview");
                assert_eq!(renderer_kind, Some(ProjectFileRendererKindDto::Image));
            }
            other => panic!("expected unsupported response, got {other:?}"),
        }
    }
}
