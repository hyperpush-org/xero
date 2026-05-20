use std::{
    fs, io,
    path::{Path, PathBuf},
};

use rand::RngCore;
use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, AgentAttachmentKindDto, CommandError, CommandResult,
        DiscardAgentAttachmentRequestDto, StageAgentAttachmentRequestDto, StagedAgentAttachmentDto,
    },
    db::project_app_data_dir_for_repo,
    state::DesktopState,
};

use super::runtime_support::resolve_project_root;

pub const MAX_BYTES_PER_ATTACHMENT: usize = 20 * 1024 * 1024;
pub const MAX_TOTAL_ATTACHMENT_BYTES: usize = 50 * 1024 * 1024;

#[tauri::command]
pub fn stage_agent_attachment<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StageAgentAttachmentRequestDto,
) -> CommandResult<StagedAgentAttachmentDto> {
    stage_agent_attachment_blocking(&app, state.inner(), request)
}

pub fn stage_agent_attachment_blocking<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    request: StageAgentAttachmentRequestDto,
) -> CommandResult<StagedAgentAttachmentDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(app, state, &request.project_id)?;
    stage_agent_attachment_for_repo(
        &repo_root,
        StageAgentAttachmentInput {
            run_id: request.run_id,
            original_name: request.original_name,
            media_type: request.media_type,
            bytes: request.bytes,
        },
    )
}

pub struct StageAgentAttachmentInput {
    pub run_id: String,
    pub original_name: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

pub fn stage_agent_attachment_path_for_repo(
    repo_root: &Path,
    run_id: impl Into<String>,
    source_path: &Path,
) -> CommandResult<StagedAgentAttachmentDto> {
    let metadata = fs::metadata(source_path).map_err(|error| {
        CommandError::user_fixable(
            "agent_attachment_source_missing",
            format!(
                "Xero could not read attachment source `{}`: {error}",
                source_path.display()
            ),
        )
    })?;
    if !metadata.is_file() {
        return Err(CommandError::user_fixable(
            "agent_attachment_source_not_file",
            format!(
                "Xero can only attach files; `{}` is not a regular file.",
                source_path.display()
            ),
        ));
    }
    let original_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_attachment_name_invalid",
                format!(
                    "Xero could not derive a file name for attachment `{}`.",
                    source_path.display()
                ),
            )
        })?;
    let media_type = media_type_for_path(source_path, &original_name).ok_or_else(|| {
        CommandError::user_fixable(
            "agent_attachment_unsupported_kind",
            format!(
                "Xero does not support attachments with this file type: `{}`.",
                original_name
            ),
        )
    })?;
    let bytes = fs::read(source_path).map_err(|error| {
        CommandError::system_fault(
            "agent_attachment_source_read_failed",
            format!(
                "Xero could not read attachment `{}`: {error}",
                source_path.display()
            ),
        )
    })?;

    stage_agent_attachment_for_repo(
        repo_root,
        StageAgentAttachmentInput {
            run_id: run_id.into(),
            original_name,
            media_type,
            bytes,
        },
    )
}

pub fn stage_agent_attachment_for_repo(
    repo_root: &Path,
    input: StageAgentAttachmentInput,
) -> CommandResult<StagedAgentAttachmentDto> {
    validate_non_empty(&input.run_id, "runId")?;
    validate_non_empty(&input.original_name, "originalName")?;
    validate_non_empty(&input.media_type, "mediaType")?;

    if input.bytes.is_empty() {
        return Err(CommandError::user_fixable(
            "agent_attachment_empty",
            "Xero refused to stage an empty attachment.",
        ));
    }
    if input.bytes.len() > MAX_BYTES_PER_ATTACHMENT {
        return Err(CommandError::user_fixable(
            "agent_attachment_too_large",
            format!(
                "Xero rejected attachment `{}` because it is {} bytes (limit {} bytes).",
                input.original_name,
                input.bytes.len(),
                MAX_BYTES_PER_ATTACHMENT
            ),
        ));
    }

    let media_type = resolve_attachment_media_type(&input.media_type, &input.original_name)
        .ok_or_else(|| unsupported_attachment_error(&input.media_type, &input.original_name))?;
    let kind = classify_attachment_kind(&media_type)
        .ok_or_else(|| unsupported_attachment_error(&media_type, &input.original_name))?;

    let attachments_dir = project_app_data_dir_for_repo(repo_root)
        .join("attachments")
        .join(&input.run_id);
    fs::create_dir_all(&attachments_dir).map_err(|error| {
        CommandError::system_fault(
            "agent_attachment_dir_create_failed",
            format!(
                "Xero could not create the attachments directory at `{}`: {error}",
                attachments_dir.display()
            ),
        )
    })?;
    let existing_total = attachment_dir_total_bytes(&attachments_dir)?;
    if existing_total.saturating_add(input.bytes.len()) > MAX_TOTAL_ATTACHMENT_BYTES {
        return Err(CommandError::user_fixable(
            "agent_attachment_total_too_large",
            format!(
                "Xero rejected attachment `{}` because pending attachments would total {} bytes (limit {} bytes).",
                input.original_name,
                existing_total.saturating_add(input.bytes.len()),
                MAX_TOTAL_ATTACHMENT_BYTES
            ),
        ));
    }

    let extension = extension_from_original_name(&input.original_name)
        .or_else(|| extension_from_media_type(&media_type))
        .unwrap_or_else(|| "bin".to_string());
    let mut id_bytes = [0_u8; 12];
    rand::thread_rng().fill_bytes(&mut id_bytes);
    let file_id: String = id_bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    let filename = format!("{file_id}.{extension}");
    let storage_path: PathBuf = attachments_dir.join(&filename);

    fs::write(&storage_path, &input.bytes).map_err(|error| {
        CommandError::system_fault(
            "agent_attachment_write_failed",
            format!(
                "Xero could not write attachment `{}` to disk: {error}",
                input.original_name
            ),
        )
    })?;

    let (width, height) = match kind {
        AgentAttachmentKindDto::Image => probe_image_dimensions(&input.bytes),
        _ => (None, None),
    };

    Ok(StagedAgentAttachmentDto {
        kind,
        absolute_path: storage_path.to_string_lossy().into_owned(),
        media_type,
        original_name: input.original_name,
        size_bytes: input.bytes.len() as i64,
        width,
        height,
    })
}

#[tauri::command]
pub fn discard_agent_attachment<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: DiscardAgentAttachmentRequestDto,
) -> CommandResult<()> {
    discard_agent_attachment_blocking(&app, state.inner(), request)
}

pub fn discard_agent_attachment_blocking<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    request: DiscardAgentAttachmentRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.absolute_path, "absolutePath")?;

    let repo_root = resolve_project_root(app, state, &request.project_id)?;
    discard_agent_attachment_for_repo(&repo_root, &request.absolute_path)
}

pub fn discard_agent_attachment_for_repo(
    repo_root: &Path,
    absolute_path: &str,
) -> CommandResult<()> {
    validate_non_empty(absolute_path, "absolutePath")?;

    let attachments_root = project_app_data_dir_for_repo(repo_root).join("attachments");
    let path = PathBuf::from(absolute_path);
    let canonical_root = canonicalize_existing_path(&attachments_root)?;
    let canonical_path = canonicalize_attachment_target(&path)?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(CommandError::user_fixable(
            "agent_attachment_path_outside_project",
            format!(
                "Xero refused to discard `{}` because it is outside the project's attachments directory.",
                absolute_path
            ),
        ));
    }
    if canonical_path.exists() {
        fs::remove_file(&canonical_path).map_err(|error| {
            CommandError::system_fault(
                "agent_attachment_remove_failed",
                format!(
                    "Xero could not remove staged attachment `{}`: {error}",
                    absolute_path
                ),
            )
        })?;
    }
    Ok(())
}

fn classify_attachment_kind(media_type: &str) -> Option<AgentAttachmentKindDto> {
    let lower = media_type.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp"
    ) {
        return Some(AgentAttachmentKindDto::Image);
    }
    if lower == "application/pdf" {
        return Some(AgentAttachmentKindDto::Document);
    }
    if is_text_media_type(&lower) {
        return Some(AgentAttachmentKindDto::Text);
    }
    None
}

fn is_text_media_type(media_type: &str) -> bool {
    if media_type.starts_with("text/") {
        return true;
    }
    matches!(
        media_type,
        "application/json"
            | "application/javascript"
            | "application/x-typescript"
            | "application/typescript"
            | "application/xml"
            | "application/x-yaml"
            | "application/x-toml"
            | "application/sql"
            | "application/x-sh"
    )
}

fn extension_from_original_name(name: &str) -> Option<String> {
    PathBuf::from(name)
        .extension()
        .and_then(|os| os.to_str())
        .filter(|ext| ext.chars().all(|c| c.is_ascii_alphanumeric()))
        .map(str::to_lowercase)
}

fn extension_from_media_type(media_type: &str) -> Option<String> {
    let lower = media_type.to_ascii_lowercase();
    Some(
        match lower.as_str() {
            "image/png" => "png",
            "image/jpeg" | "image/jpg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "application/pdf" => "pdf",
            "text/plain" => "txt",
            "text/markdown" => "md",
            "text/html" => "html",
            "text/css" => "css",
            "text/csv" => "csv",
            "application/json" => "json",
            "application/javascript" => "js",
            "application/x-typescript" | "application/typescript" => "ts",
            "application/xml" | "text/xml" => "xml",
            "application/x-yaml" | "text/yaml" => "yaml",
            "application/x-toml" => "toml",
            "application/sql" => "sql",
            "application/x-sh" => "sh",
            _ => return None,
        }
        .to_string(),
    )
}

fn resolve_attachment_media_type(reported: &str, original_name: &str) -> Option<String> {
    let trimmed = reported.trim().to_ascii_lowercase();
    if !trimmed.is_empty() && trimmed != "application/octet-stream" {
        return Some(trimmed);
    }
    media_type_from_extension(original_name)
}

fn media_type_for_path(path: &Path, original_name: &str) -> Option<String> {
    if let Some(media_type) = media_type_from_extension(original_name) {
        return Some(media_type);
    }
    let guessed = mime_guess::from_path(path)
        .first()
        .map(|mime| mime.essence_str().to_ascii_lowercase())
        .unwrap_or_default();
    resolve_attachment_media_type(&guessed, original_name)
}

fn media_type_from_extension(file_name: &str) -> Option<String> {
    let lower_name = file_name.to_ascii_lowercase();
    if lower_name == "dockerfile" {
        return Some("text/plain".to_string());
    }
    let ext = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())?
        .to_ascii_lowercase();
    Some(
        match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "pdf" => "application/pdf",
            "txt" => "text/plain",
            "md" | "markdown" => "text/markdown",
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "csv" => "text/csv",
            "json" => "application/json",
            "js" | "mjs" | "cjs" | "jsx" => "application/javascript",
            "ts" | "tsx" => "application/x-typescript",
            "xml" => "application/xml",
            "yml" | "yaml" => "application/x-yaml",
            "toml" => "application/x-toml",
            "sql" => "application/sql",
            "sh" | "bash" | "zsh" => "application/x-sh",
            "rs" | "py" | "go" | "rb" | "c" | "h" | "cpp" | "hpp" | "swift" | "kt" | "java"
            | "log" | "conf" | "ini" | "env" => "text/plain",
            _ => return None,
        }
        .to_string(),
    )
}

fn unsupported_attachment_error(media_type: &str, original_name: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_attachment_unsupported_kind",
        format!(
            "Xero does not support attachments of media type `{}` for `{}`.",
            media_type, original_name
        ),
    )
}

fn attachment_dir_total_bytes(path: &Path) -> CommandResult<usize> {
    let mut total = 0usize;
    let entries = fs::read_dir(path).map_err(|error| {
        CommandError::system_fault(
            "agent_attachment_dir_read_failed",
            format!(
                "Xero could not read the attachments directory at `{}`: {error}",
                path.display()
            ),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            CommandError::system_fault(
                "agent_attachment_dir_read_failed",
                format!(
                    "Xero could not inspect an attachment directory entry at `{}`: {error}",
                    path.display()
                ),
            )
        })?;
        let metadata = entry.metadata().map_err(|error| {
            CommandError::system_fault(
                "agent_attachment_metadata_failed",
                format!(
                    "Xero could not inspect staged attachment `{}`: {error}",
                    entry.path().display()
                ),
            )
        })?;
        if metadata.is_file() {
            total = total.saturating_add(metadata.len() as usize);
        }
    }
    Ok(total)
}

fn canonicalize_existing_path(path: &Path) -> CommandResult<PathBuf> {
    fs::canonicalize(path).map_err(|error| {
        CommandError::system_fault(
            "agent_attachment_path_canonicalize_failed",
            format!(
                "Xero could not validate attachment path `{}`: {error}",
                path.display()
            ),
        )
    })
}

fn canonicalize_attachment_target(path: &Path) -> CommandResult<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| {
                CommandError::user_fixable(
                    "agent_attachment_path_invalid",
                    format!(
                        "Xero refused to discard invalid attachment path `{}`.",
                        path.display()
                    ),
                )
            })?;
            let parent = canonicalize_existing_path(parent)?;
            let file_name = path.file_name().ok_or_else(|| {
                CommandError::user_fixable(
                    "agent_attachment_path_invalid",
                    format!(
                        "Xero refused to discard invalid attachment path `{}`.",
                        path.display()
                    ),
                )
            })?;
            Ok(parent.join(file_name))
        }
        Err(error) => Err(CommandError::system_fault(
            "agent_attachment_path_canonicalize_failed",
            format!(
                "Xero could not validate attachment path `{}`: {error}",
                path.display()
            ),
        )),
    }
}

fn probe_image_dimensions(bytes: &[u8]) -> (Option<i64>, Option<i64>) {
    match image::load_from_memory(bytes) {
        Ok(image) => (Some(image.width() as i64), Some(image.height() as i64)),
        Err(_) => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp repo");
        fs::create_dir_all(dir.path().join("src")).expect("repo src");
        dir
    }

    fn configure_temp_app_data() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp app data");
        crate::db::configure_project_database_paths(&dir.path().join("xero.db"));
        dir
    }

    #[test]
    fn stage_supported_text_file_under_app_data() {
        let _app_data = configure_temp_app_data();
        let repo = temp_repo();
        let file = repo.path().join("notes.md");
        fs::write(&file, "# hello\n").expect("write text");

        let staged = stage_agent_attachment_path_for_repo(repo.path(), "run-1", &file)
            .expect("stage text attachment");

        assert_eq!(staged.kind, AgentAttachmentKindDto::Text);
        assert_eq!(staged.media_type, "text/markdown");
        assert!(Path::new(&staged.absolute_path).exists());
        assert!(Path::new(&staged.absolute_path)
            .starts_with(project_app_data_dir_for_repo(repo.path()).join("attachments")));
        assert!(!staged.absolute_path.contains(".xero"));
    }

    #[test]
    fn stage_supported_pdf_and_rust_extension_fallbacks() {
        let _app_data = configure_temp_app_data();
        let repo = temp_repo();
        let pdf = repo.path().join("paper.pdf");
        fs::write(&pdf, b"%PDF-1.4\n").expect("write pdf");
        let rust = repo.path().join("src/lib.rs");
        fs::write(&rust, "fn main() {}\n").expect("write rust");

        let pdf =
            stage_agent_attachment_path_for_repo(repo.path(), "run-1", &pdf).expect("stage pdf");
        let rust =
            stage_agent_attachment_path_for_repo(repo.path(), "run-2", &rust).expect("stage rust");

        assert_eq!(pdf.kind, AgentAttachmentKindDto::Document);
        assert_eq!(pdf.media_type, "application/pdf");
        assert_eq!(rust.kind, AgentAttachmentKindDto::Text);
        assert_eq!(rust.media_type, "text/plain");
    }

    #[test]
    fn stage_rejects_empty_and_per_file_limit() {
        let _app_data = configure_temp_app_data();
        let repo = temp_repo();

        let empty = stage_agent_attachment_for_repo(
            repo.path(),
            StageAgentAttachmentInput {
                run_id: "run-empty".into(),
                original_name: "empty.txt".into(),
                media_type: "text/plain".into(),
                bytes: Vec::new(),
            },
        )
        .expect_err("empty rejected");
        assert_eq!(empty.code, "agent_attachment_empty");

        let too_large = stage_agent_attachment_for_repo(
            repo.path(),
            StageAgentAttachmentInput {
                run_id: "run-large".into(),
                original_name: "large.txt".into(),
                media_type: "text/plain".into(),
                bytes: vec![b'a'; MAX_BYTES_PER_ATTACHMENT + 1],
            },
        )
        .expect_err("large rejected");
        assert_eq!(too_large.code, "agent_attachment_too_large");
    }

    #[test]
    fn stage_rejects_total_pending_limit() {
        let _app_data = configure_temp_app_data();
        let repo = temp_repo();
        let run_id = "run-total";
        for (name, size) in [
            ("one.txt", MAX_BYTES_PER_ATTACHMENT),
            ("two.txt", MAX_BYTES_PER_ATTACHMENT),
            ("three.txt", 10 * 1024 * 1024 - 8),
        ] {
            stage_agent_attachment_for_repo(
                repo.path(),
                StageAgentAttachmentInput {
                    run_id: run_id.into(),
                    original_name: name.into(),
                    media_type: "text/plain".into(),
                    bytes: vec![b'a'; size],
                },
            )
            .expect("attachment under per-file and total limits");
        }

        let error = stage_agent_attachment_for_repo(
            repo.path(),
            StageAgentAttachmentInput {
                run_id: run_id.into(),
                original_name: "two.txt".into(),
                media_type: "text/plain".into(),
                bytes: vec![b'b'; 16],
            },
        )
        .expect_err("total limit rejected");

        assert_eq!(error.code, "agent_attachment_total_too_large");
    }

    #[test]
    fn discard_validates_project_attachment_containment() {
        let _app_data = configure_temp_app_data();
        let repo = temp_repo();
        let file = repo.path().join("notes.txt");
        fs::write(&file, "hello\n").expect("write text");
        let staged = stage_agent_attachment_path_for_repo(repo.path(), "run-1", &file)
            .expect("stage text attachment");

        discard_agent_attachment_for_repo(repo.path(), &staged.absolute_path)
            .expect("discard staged file");
        assert!(!Path::new(&staged.absolute_path).exists());

        let outside = repo.path().join("outside.txt");
        fs::write(&outside, "outside\n").expect("outside");
        let error = discard_agent_attachment_for_repo(repo.path(), &outside.to_string_lossy())
            .expect_err("outside rejected");
        assert_eq!(error.code, "agent_attachment_path_outside_project");
    }
}
