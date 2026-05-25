use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::{
    commands::{
        project_assets::{ProjectAppDataAssetGrant, ProjectAssetState},
        project_files::metadata_modified_at,
        CommandError, CommandResult, ProjectFileRendererKindDto, RuntimeStreamMediaAttachmentDto,
        RuntimeStreamMediaKindDto, RuntimeStreamMediaSourceDto,
    },
    db::project_app_data_dir_for_repo,
};

const CONVERSATION_MEDIA_DIR: &str = "tool-artifacts/conversation-media";
const MAX_RUNTIME_IMAGE_BYTES: usize = 24 * 1024 * 1024;
const MAX_MEDIA_ATTACHMENTS_PER_TOOL: usize = 8;

#[derive(Debug, Clone, Copy)]
pub(crate) struct RemoteRuntimeMediaContext<'a> {
    pub computer_id: &'a str,
    pub session_id: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RuntimeMediaExtractionRequest<'a> {
    pub repo_root: &'a Path,
    pub project_id: &'a str,
    pub run_id: &'a str,
    pub event_id: i64,
    pub tool_call_id: Option<&'a str>,
    pub tool_name: Option<&'a str>,
    pub output: &'a JsonValue,
    pub asset_state: Option<&'a ProjectAssetState>,
    pub remote_context: Option<RemoteRuntimeMediaContext<'a>>,
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeMediaArtifactBytes {
    pub artifact_id: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ExtractedImage {
    media_type: String,
    bytes: Vec<u8>,
    title: Option<String>,
    alt: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

pub(crate) fn extract_runtime_media_attachments(
    request: RuntimeMediaExtractionRequest<'_>,
) -> Vec<RuntimeStreamMediaAttachmentDto> {
    let mut images = Vec::new();
    collect_tool_output_images(request.output, &mut images);
    if images.is_empty() {
        return Vec::new();
    }

    images
        .into_iter()
        .take(MAX_MEDIA_ATTACHMENTS_PER_TOOL)
        .enumerate()
        .filter_map(|(index, image)| store_runtime_image_attachment(&request, index, image).ok())
        .collect()
}

pub(crate) fn read_runtime_media_artifact(
    repo_root: &Path,
    artifact_id: &str,
) -> CommandResult<RuntimeMediaArtifactBytes> {
    if !is_safe_artifact_id(artifact_id) {
        return Err(CommandError::invalid_request("artifactId"));
    }

    let root = conversation_media_root(repo_root);
    for (extension, media_type) in [
        ("png", "image/png"),
        ("jpg", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("gif", "image/gif"),
        ("webp", "image/webp"),
    ] {
        let path = root.join(format!("{artifact_id}.{extension}"));
        if !path.exists() {
            continue;
        }
        let bytes = fs::read(&path).map_err(|error| {
            CommandError::system_fault(
                "runtime_media_artifact_read_failed",
                format!("Xero could not read runtime media artifact `{artifact_id}`: {error}"),
            )
        })?;
        return Ok(RuntimeMediaArtifactBytes {
            artifact_id: artifact_id.to_string(),
            media_type: media_type.to_string(),
            bytes,
        });
    }

    Err(CommandError::user_fixable(
        "runtime_media_artifact_not_found",
        format!("Xero could not find runtime media artifact `{artifact_id}`."),
    ))
}

pub(crate) fn runtime_media_source_url(
    repo_root: &Path,
    project_id: &str,
    asset_state: Option<&ProjectAssetState>,
    absolute_path: &Path,
    media_type: &str,
    content_hash: &str,
) -> Option<String> {
    let asset_state = asset_state?;
    let metadata = fs::metadata(absolute_path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(
        asset_state.issue_app_data_preview_url(ProjectAppDataAssetGrant {
            project_id: project_id.to_string(),
            root_path: conversation_media_root(repo_root),
            absolute_path: absolute_path.to_path_buf(),
            byte_length: metadata.len(),
            modified_at: metadata_modified_at(&metadata),
            content_hash: content_hash.to_string(),
            mime_type: media_type.to_string(),
            renderer_kind: ProjectFileRendererKindDto::Image,
        }),
    )
}

fn collect_tool_output_images(output: &JsonValue, images: &mut Vec<ExtractedImage>) {
    if images.len() >= MAX_MEDIA_ATTACHMENTS_PER_TOOL {
        return;
    }

    let output = normalized_tool_output(output);
    match json_string(output, &["kind"]).as_deref() {
        Some("browser") => collect_browser_image(output, images),
        Some("read") => collect_read_image(output, images),
        Some("macos_automation") => collect_macos_image(output, images),
        Some("mcp") => collect_mcp_output_images(output, images),
        _ => collect_mcp_content_images(output, images),
    }
}

fn collect_browser_image(output: &JsonValue, images: &mut Vec<ExtractedImage>) {
    if json_string(output, &["action"]).as_deref() != Some("screenshot") {
        return;
    }
    let Some(value_json) = json_string(output, &["valueJson", "value_json"]) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<JsonValue>(&value_json) else {
        return;
    };
    let Some(raw_base64) = value.as_str() else {
        return;
    };
    let Some(bytes) = decode_image_base64(raw_base64) else {
        return;
    };
    let media_type = image_media_type_from_bytes(&bytes).unwrap_or("image/png");
    let (width, height) = image_dimensions(&bytes);
    images.push(ExtractedImage {
        media_type: media_type.to_string(),
        bytes,
        title: Some("Browser screenshot".into()),
        alt: Some("Browser screenshot".into()),
        width,
        height,
    });
}

fn collect_read_image(output: &JsonValue, images: &mut Vec<ExtractedImage>) {
    if json_string(output, &["contentKind", "content_kind"]).as_deref() != Some("image") {
        return;
    }
    let Some(raw_base64) = json_string(output, &["previewBase64", "preview_base64"]) else {
        return;
    };
    let Some(bytes) = decode_image_base64(&raw_base64) else {
        return;
    };
    let media_type = normalize_supported_media_type(
        json_string(output, &["mediaType", "media_type"]).as_deref(),
    )
    .or_else(|| image_media_type_from_bytes(&bytes))
    .unwrap_or("image/png");
    let (detected_width, detected_height) = image_dimensions(&bytes);
    let width = json_u32(output, &["imageWidth", "image_width"]).or(detected_width);
    let height = json_u32(output, &["imageHeight", "image_height"]).or(detected_height);
    let title = json_string(output, &["path"])
        .and_then(|path| {
            Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .or_else(|| Some("Image preview".into()));
    images.push(ExtractedImage {
        media_type: media_type.to_string(),
        bytes,
        title: title.clone(),
        alt: title,
        width,
        height,
    });
}

fn collect_macos_image(output: &JsonValue, images: &mut Vec<ExtractedImage>) {
    let Some(screenshot) = output.get("screenshot").and_then(JsonValue::as_object) else {
        return;
    };
    let Some(path) = screenshot.get("path").and_then(JsonValue::as_str) else {
        return;
    };
    let Ok(bytes) = fs::read(path) else {
        return;
    };
    if bytes.len() > MAX_RUNTIME_IMAGE_BYTES {
        return;
    }
    let Some(media_type) = image_media_type_from_bytes(&bytes) else {
        return;
    };
    let (detected_width, detected_height) = image_dimensions(&bytes);
    let width = json_u32_object(screenshot, &["width"]).or(detected_width);
    let height = json_u32_object(screenshot, &["height"]).or(detected_height);
    images.push(ExtractedImage {
        media_type: media_type.to_string(),
        bytes,
        title: Some("macOS screenshot".into()),
        alt: Some("macOS screenshot".into()),
        width,
        height,
    });
}

fn collect_mcp_output_images(output: &JsonValue, images: &mut Vec<ExtractedImage>) {
    if let Some(result) = output.get("result") {
        collect_mcp_content_images(result, images);
    }

    if images.len() >= MAX_MEDIA_ATTACHMENTS_PER_TOOL {
        return;
    }

    let Some(artifact) = output
        .get("resultArtifact")
        .or_else(|| output.get("result_artifact"))
    else {
        return;
    };
    let Some(path) = artifact.get("path").and_then(JsonValue::as_str) else {
        return;
    };
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<JsonValue>(&text) else {
        return;
    };
    collect_mcp_content_images(&value, images);
}

fn collect_mcp_content_images(value: &JsonValue, images: &mut Vec<ExtractedImage>) {
    if images.len() >= MAX_MEDIA_ATTACHMENTS_PER_TOOL {
        return;
    }

    match value {
        JsonValue::Array(values) => {
            for value in values {
                collect_mcp_content_images(value, images);
                if images.len() >= MAX_MEDIA_ATTACHMENTS_PER_TOOL {
                    break;
                }
            }
        }
        JsonValue::Object(object) => {
            if let Some(image) = mcp_content_image(value) {
                images.push(image);
                return;
            }
            for key in ["content", "result", "items"] {
                if let Some(child) = object.get(key) {
                    collect_mcp_content_images(child, images);
                    if images.len() >= MAX_MEDIA_ATTACHMENTS_PER_TOOL {
                        break;
                    }
                }
            }
        }
        _ => {}
    }
}

fn mcp_content_image(value: &JsonValue) -> Option<ExtractedImage> {
    let item_type = json_string(value, &["type"])?;
    if item_type != "image" && item_type != "blob" {
        return None;
    }
    let media_type = normalize_supported_media_type(
        json_string(value, &["mimeType", "mediaType", "mime_type", "media_type"]).as_deref(),
    )?;
    if !media_type.starts_with("image/") {
        return None;
    }
    let raw_base64 = json_string(value, &["data", "bytes", "blob"])?;
    let bytes = decode_image_base64(&raw_base64)?;
    let (width, height) = image_dimensions(&bytes);
    let title = json_string(value, &["name", "title"]).or_else(|| Some("Tool image".into()));
    let alt = json_string(value, &["alt", "description"]).or_else(|| title.clone());
    Some(ExtractedImage {
        media_type: media_type.to_string(),
        bytes,
        title,
        alt,
        width,
        height,
    })
}

fn store_runtime_image_attachment(
    request: &RuntimeMediaExtractionRequest<'_>,
    index: usize,
    mut image: ExtractedImage,
) -> CommandResult<RuntimeStreamMediaAttachmentDto> {
    if image.bytes.len() > MAX_RUNTIME_IMAGE_BYTES {
        return Err(CommandError::system_fault(
            "runtime_media_image_too_large",
            "Runtime image output exceeded the maximum displayable size.",
        ));
    }
    let media_type = normalize_supported_media_type(Some(&image.media_type)).ok_or_else(|| {
        CommandError::system_fault(
            "runtime_media_unsupported_type",
            format!(
                "Unsupported runtime image media type `{}`.",
                image.media_type
            ),
        )
    })?;
    image.media_type = media_type.to_string();

    let hash = sha256_hex(&image.bytes);
    let extension = extension_for_media_type(media_type);
    let artifact_id = runtime_media_artifact_id(request, index, &hash);
    let root = conversation_media_root(request.repo_root);
    fs::create_dir_all(&root).map_err(|error| {
        CommandError::system_fault(
            "runtime_media_artifact_dir_failed",
            format!("Xero could not create the runtime media artifact directory: {error}"),
        )
    })?;
    let artifact_path = root.join(format!("{artifact_id}.{extension}"));
    fs::write(&artifact_path, &image.bytes).map_err(|error| {
        CommandError::system_fault(
            "runtime_media_artifact_write_failed",
            format!("Xero could not write runtime media artifact `{artifact_id}`: {error}"),
        )
    })?;

    let render_url = runtime_media_source_url(
        request.repo_root,
        request.project_id,
        request.asset_state,
        &artifact_path,
        media_type,
        &hash,
    );
    let source = if let Some(remote) = request.remote_context {
        RuntimeStreamMediaSourceDto::RemoteArtifact {
            artifact_id: artifact_id.clone(),
            computer_id: remote.computer_id.to_string(),
            session_id: remote.session_id.to_string(),
        }
    } else {
        RuntimeStreamMediaSourceDto::AppDataPath {
            absolute_path: artifact_path.to_string_lossy().into_owned(),
        }
    };

    Ok(RuntimeStreamMediaAttachmentDto {
        id: format!("media:{artifact_id}"),
        kind: RuntimeStreamMediaKindDto::Image,
        media_type: media_type.to_string(),
        title: non_empty_string(image.title),
        alt: non_empty_string(image.alt),
        size_bytes: Some(image.bytes.len() as u64),
        width: image.width,
        height: image.height,
        source,
        render_url,
    })
}

fn normalized_tool_output(output: &JsonValue) -> &JsonValue {
    output
        .get("output")
        .filter(|nested| nested.get("kind").is_some())
        .unwrap_or(output)
}

fn conversation_media_root(repo_root: &Path) -> PathBuf {
    project_app_data_dir_for_repo(repo_root).join(CONVERSATION_MEDIA_DIR)
}

fn runtime_media_artifact_id(
    request: &RuntimeMediaExtractionRequest<'_>,
    index: usize,
    hash: &str,
) -> String {
    let tool = request.tool_call_id.or(request.tool_name).unwrap_or("tool");
    format!(
        "{}-{}-{}-{}-{}",
        sanitize_id_segment(request.run_id),
        request.event_id.max(0),
        sanitize_id_segment(tool),
        index,
        &hash[..16.min(hash.len())],
    )
}

fn sanitize_id_segment(value: &str) -> String {
    let mut output = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' {
            output.push(ch);
        } else if !output.ends_with('-') {
            output.push('-');
        }
    }
    let output = output.trim_matches('-');
    if output.is_empty() {
        "item".into()
    } else {
        output.chars().take(80).collect()
    }
}

fn is_safe_artifact_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn decode_image_base64(value: &str) -> Option<Vec<u8>> {
    let payload = value
        .split_once(',')
        .filter(|(prefix, _)| prefix.trim_start().starts_with("data:image/"))
        .map(|(_, data)| data)
        .unwrap_or(value);
    let bytes = BASE64_STANDARD.decode(payload.trim().as_bytes()).ok()?;
    if bytes.is_empty() || bytes.len() > MAX_RUNTIME_IMAGE_BYTES {
        return None;
    }
    Some(bytes)
}

fn image_media_type_from_bytes(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn normalize_supported_media_type(value: Option<&str>) -> Option<&'static str> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some("image/png"),
        "image/jpg" | "image/jpeg" => Some("image/jpeg"),
        "image/gif" => Some("image/gif"),
        "image/webp" => Some("image/webp"),
        _ => None,
    }
}

fn extension_for_media_type(media_type: &str) -> &'static str {
    match media_type {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    }
}

fn image_dimensions(bytes: &[u8]) -> (Option<u32>, Option<u32>) {
    let Ok(image) = image::load_from_memory(bytes) else {
        return (None, None);
    };
    (Some(image.width()), Some(image.height()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn json_string(value: &JsonValue, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(JsonValue::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn json_u32(value: &JsonValue, keys: &[&str]) -> Option<u32> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(JsonValue::as_u64))
        .and_then(|value| u32::try_from(value).ok())
}

fn json_u32_object(object: &serde_json::Map<String, JsonValue>, keys: &[&str]) -> Option<u32> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(JsonValue::as_u64))
        .and_then(|value| u32::try_from(value).ok())
}

fn non_empty_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ONE_BY_ONE_PNG: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMB/axj3nQAAAAASUVORK5CYII=";

    #[test]
    fn extracts_browser_screenshot_to_app_data_artifact() {
        let repo = tempfile::tempdir().expect("repo");
        let output = json!({
            "kind": "browser",
            "action": "screenshot",
            "valueJson": serde_json::to_string(&ONE_BY_ONE_PNG).unwrap(),
        });

        let attachments = extract_runtime_media_attachments(RuntimeMediaExtractionRequest {
            repo_root: repo.path(),
            project_id: "project-1",
            run_id: "run-1",
            event_id: 42,
            tool_call_id: Some("call-browser"),
            tool_name: Some("browser"),
            output: &output,
            asset_state: None,
            remote_context: None,
        });

        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].media_type, "image/png");
        assert!(matches!(
            attachments[0].source,
            RuntimeStreamMediaSourceDto::AppDataPath { .. }
        ));
    }

    #[test]
    fn rejects_unsafe_remote_artifact_ids() {
        let repo = tempfile::tempdir().expect("repo");
        let error = read_runtime_media_artifact(repo.path(), "../escape").unwrap_err();
        assert_eq!(error.code, "invalid_request");
    }
}
