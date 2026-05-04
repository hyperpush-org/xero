use std::{fs, path::PathBuf};

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

const MAX_BYTES_PER_ATTACHMENT: usize = 20 * 1024 * 1024;

#[tauri::command]
pub fn stage_agent_attachment<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StageAgentAttachmentRequestDto,
) -> CommandResult<StagedAgentAttachmentDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.original_name, "originalName")?;
    validate_non_empty(&request.media_type, "mediaType")?;

    if request.bytes.is_empty() {
        return Err(CommandError::user_fixable(
            "agent_attachment_empty",
            "Xero refused to stage an empty attachment.",
        ));
    }
    if request.bytes.len() > MAX_BYTES_PER_ATTACHMENT {
        return Err(CommandError::user_fixable(
            "agent_attachment_too_large",
            format!(
                "Xero rejected attachment `{}` because it is {} bytes (limit {} bytes).",
                request.original_name,
                request.bytes.len(),
                MAX_BYTES_PER_ATTACHMENT
            ),
        ));
    }

    let kind = classify_attachment_kind(&request.media_type).ok_or_else(|| {
        CommandError::user_fixable(
            "agent_attachment_unsupported_kind",
            format!(
                "Xero does not support attachments of media type `{}` for `{}`.",
                request.media_type, request.original_name
            ),
        )
    })?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let attachments_dir = project_app_data_dir_for_repo(&repo_root)
        .join("attachments")
        .join(&request.run_id);
    fs::create_dir_all(&attachments_dir).map_err(|error| {
        CommandError::system_fault(
            "agent_attachment_dir_create_failed",
            format!(
                "Xero could not create the attachments directory at `{}`: {error}",
                attachments_dir.display()
            ),
        )
    })?;

    let extension = extension_from_original_name(&request.original_name)
        .or_else(|| extension_from_media_type(&request.media_type))
        .unwrap_or_else(|| "bin".to_string());
    let mut id_bytes = [0_u8; 12];
    rand::thread_rng().fill_bytes(&mut id_bytes);
    let file_id: String = id_bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    let filename = format!("{file_id}.{extension}");
    let storage_path: PathBuf = attachments_dir.join(&filename);

    fs::write(&storage_path, &request.bytes).map_err(|error| {
        CommandError::system_fault(
            "agent_attachment_write_failed",
            format!(
                "Xero could not write attachment `{}` to disk: {error}",
                request.original_name
            ),
        )
    })?;

    let (width, height) = match kind {
        AgentAttachmentKindDto::Image => probe_image_dimensions(&request.bytes),
        _ => (None, None),
    };

    Ok(StagedAgentAttachmentDto {
        kind,
        absolute_path: storage_path.to_string_lossy().into_owned(),
        media_type: request.media_type,
        original_name: request.original_name,
        size_bytes: request.bytes.len() as i64,
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
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.absolute_path, "absolutePath")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let attachments_root = project_app_data_dir_for_repo(&repo_root).join("attachments");
    let path = PathBuf::from(&request.absolute_path);
    if !path.starts_with(&attachments_root) {
        return Err(CommandError::user_fixable(
            "agent_attachment_path_outside_project",
            format!(
                "Xero refused to discard `{}` because it is outside the project's attachments directory.",
                request.absolute_path
            ),
        ));
    }
    if path.exists() {
        fs::remove_file(&path).map_err(|error| {
            CommandError::system_fault(
                "agent_attachment_remove_failed",
                format!(
                    "Xero could not remove staged attachment `{}`: {error}",
                    request.absolute_path
                ),
            )
        })?;
    }
    Ok(())
}

fn classify_attachment_kind(media_type: &str) -> Option<AgentAttachmentKindDto> {
    let lower = media_type.to_ascii_lowercase();
    if lower.starts_with("image/") {
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

fn probe_image_dimensions(bytes: &[u8]) -> (Option<i64>, Option<i64>) {
    match image::load_from_memory(bytes) {
        Ok(image) => (Some(image.width() as i64), Some(image.height() as i64)),
        Err(_) => (None, None),
    }
}
