use std::{
    collections::HashMap,
    fs,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rand::{distributions::Alphanumeric, Rng};
use tauri::http;
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::{
    commands::{
        project_files::{read_metadata, resolve_project_root, resolve_virtual_path},
        validate_non_empty, CommandResult, ProjectFileRendererKindDto,
        RevokeProjectAssetTokensRequestDto,
    },
    state::DesktopState,
};

pub const URI_SCHEME: &str = "project-asset";

const TOKEN_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
pub(crate) struct ProjectAssetGrant {
    pub project_id: String,
    pub path: String,
    pub byte_length: u64,
    pub modified_at: String,
    pub content_hash: String,
    pub mime_type: String,
    pub renderer_kind: ProjectFileRendererKindDto,
}

#[derive(Debug, Clone)]
struct ProjectAssetToken {
    grant: ProjectAssetGrant,
    expires_at: Instant,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectAssetState {
    tokens: Arc<Mutex<HashMap<String, ProjectAssetToken>>>,
}

impl ProjectAssetState {
    pub(crate) fn issue_preview_url(&self, grant: ProjectAssetGrant) -> String {
        let token = random_token();
        let mut tokens = self
            .tokens
            .lock()
            .expect("project asset token lock poisoned");
        prune_expired_tokens(&mut tokens);
        tokens.insert(
            token.clone(),
            ProjectAssetToken {
                grant,
                expires_at: Instant::now() + TOKEN_TTL,
            },
        );

        format!("{URI_SCHEME}://{token}")
    }

    fn take_valid_grant(&self, token: &str) -> Option<ProjectAssetGrant> {
        let mut tokens = self
            .tokens
            .lock()
            .expect("project asset token lock poisoned");
        prune_expired_tokens(&mut tokens);
        tokens.get(token).map(|entry| entry.grant.clone())
    }

    fn revoke(&self, token: &str) {
        let mut tokens = self
            .tokens
            .lock()
            .expect("project asset token lock poisoned");
        tokens.remove(token);
    }

    pub(crate) fn revoke_project_tokens(&self, project_id: &str, paths: &[String]) {
        let path_set = paths
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        let mut tokens = self
            .tokens
            .lock()
            .expect("project asset token lock poisoned");
        tokens.retain(|_, entry| {
            if entry.grant.project_id != project_id {
                return true;
            }
            !path_set.is_empty() && !path_set.contains(entry.grant.path.as_str())
        });
    }
}

#[tauri::command]
pub async fn revoke_project_asset_tokens(
    asset_state: tauri::State<'_, ProjectAssetState>,
    request: RevokeProjectAssetTokensRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    asset_state.revoke_project_tokens(&request.project_id, &request.paths);
    Ok(())
}

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: http::Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    std::thread::spawn(move || {
        responder.respond(serve_request(&app, request));
    });
}

fn serve_request<R: Runtime>(
    app: &AppHandle<R>,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    let Some(asset_state) = app.try_state::<ProjectAssetState>() else {
        return not_found("project asset state unavailable");
    };
    let Some(desktop_state) = app.try_state::<DesktopState>() else {
        return not_found("desktop state unavailable");
    };
    let Some(token) = token_from_uri(request.uri()) else {
        return bad_request("missing project asset token");
    };
    let Some(grant) = asset_state.take_valid_grant(&token) else {
        return not_found("project asset token is expired or unknown");
    };

    let project_root = match resolve_project_root(app, desktop_state.inner(), &grant.project_id) {
        Ok(root) => root,
        Err(error) => return command_error_response(http::StatusCode::NOT_FOUND, &error.message),
    };
    let (resolved_path, normalized_path) =
        match resolve_virtual_path(&project_root, &grant.path, "path", false) {
            Ok(value) => value,
            Err(error) => {
                asset_state.revoke(&token);
                return command_error_response(http::StatusCode::FORBIDDEN, &error.message);
            }
        };
    if normalized_path != grant.path {
        asset_state.revoke(&token);
        return forbidden("project asset path changed");
    }

    let metadata = match read_metadata(&resolved_path) {
        Ok(metadata) => metadata,
        Err(error) => {
            asset_state.revoke(&token);
            return command_error_response(http::StatusCode::NOT_FOUND, &error.message);
        }
    };
    if !metadata.is_file() {
        asset_state.revoke(&token);
        return forbidden("project asset is not a file");
    }

    let modified_at = crate::commands::project_files::metadata_modified_at(&metadata);
    if metadata.len() != grant.byte_length || modified_at != grant.modified_at {
        asset_state.revoke(&token);
        return gone("project asset changed on disk");
    }

    let range = match request
        .headers()
        .get(http::header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(|value| parse_range_header(value, grant.byte_length))
        .transpose()
    {
        Ok(value) => value.flatten(),
        Err(()) => {
            return range_not_satisfiable(grant.byte_length);
        }
    };

    match range {
        Some(range) => serve_byte_range(&resolved_path, &grant, range, request.method()),
        None => serve_full_file(&resolved_path, &grant, request.method()),
    }
}

fn serve_full_file(
    resolved_path: &PathBuf,
    grant: &ProjectAssetGrant,
    method: &http::Method,
) -> http::Response<Vec<u8>> {
    let body = if method == http::Method::HEAD {
        Vec::new()
    } else {
        match fs::read(resolved_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return server_error(&format!(
                    "could not read project asset `{}`: {error}",
                    resolved_path.display()
                ));
            }
        }
    };

    response_with_asset_headers(http::StatusCode::OK, grant, grant.byte_length, None, body)
}

fn serve_byte_range(
    resolved_path: &PathBuf,
    grant: &ProjectAssetGrant,
    range: ByteRange,
    method: &http::Method,
) -> http::Response<Vec<u8>> {
    let range_len = range.len();
    let body = if method == http::Method::HEAD {
        Vec::new()
    } else {
        match read_file_range(resolved_path, range) {
            Ok(bytes) => bytes,
            Err(error) => {
                return server_error(&format!(
                    "could not read project asset range `{}`: {error}",
                    resolved_path.display()
                ));
            }
        }
    };

    response_with_asset_headers(
        http::StatusCode::PARTIAL_CONTENT,
        grant,
        range_len,
        Some(range),
        body,
    )
}

fn response_with_asset_headers(
    status: http::StatusCode,
    grant: &ProjectAssetGrant,
    content_length: u64,
    range: Option<ByteRange>,
    body: Vec<u8>,
) -> http::Response<Vec<u8>> {
    let mut builder = http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, grant.mime_type.as_str())
        .header(http::header::CONTENT_LENGTH, content_length.to_string())
        .header(http::header::CACHE_CONTROL, "private, max-age=60")
        .header(http::header::ETAG, etag(&grant.content_hash))
        .header(http::header::ACCEPT_RANGES, "bytes")
        .header(
            "X-Xero-Renderer-Kind",
            renderer_kind_label(&grant.renderer_kind),
        );

    if let Some(range) = range {
        builder = builder.header(
            http::header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", range.start, range.end, grant.byte_length),
        );
    }

    builder
        .body(body)
        .unwrap_or_else(|_| empty_response(http::StatusCode::INTERNAL_SERVER_ERROR))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    fn len(self) -> u64 {
        self.end - self.start + 1
    }
}

fn parse_range_header(value: &str, total_len: u64) -> Result<Option<ByteRange>, ()> {
    let Some(spec) = value.trim().strip_prefix("bytes=") else {
        return Err(());
    };
    if spec.contains(',') {
        return Err(());
    }
    if total_len == 0 {
        return Err(());
    }

    let Some((start_raw, end_raw)) = spec.split_once('-') else {
        return Err(());
    };

    if start_raw.is_empty() {
        let suffix_len = end_raw.trim().parse::<u64>().map_err(|_| ())?;
        if suffix_len == 0 {
            return Err(());
        }
        let start = total_len.saturating_sub(suffix_len);
        return Ok(Some(ByteRange {
            start,
            end: total_len - 1,
        }));
    }

    let start = start_raw.trim().parse::<u64>().map_err(|_| ())?;
    if start >= total_len {
        return Err(());
    }

    let end = if end_raw.trim().is_empty() {
        total_len - 1
    } else {
        end_raw
            .trim()
            .parse::<u64>()
            .map_err(|_| ())?
            .min(total_len - 1)
    };
    if end < start {
        return Err(());
    }

    Ok(Some(ByteRange { start, end }))
}

fn read_file_range(path: &PathBuf, range: ByteRange) -> std::io::Result<Vec<u8>> {
    let len = usize::try_from(range.len()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "project asset range is too large to allocate",
        )
    })?;
    let mut file = fs::File::open(path)?;
    file.seek(SeekFrom::Start(range.start))?;
    let mut bytes = vec![0_u8; len];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn token_from_uri(uri: &http::Uri) -> Option<String> {
    let host = uri.host().filter(|host| !host.trim().is_empty());
    let path_token = uri.path().split('/').find(|segment| !segment.is_empty());
    host.or(path_token)
        .filter(|token| is_safe_token(token))
        .map(ToOwned::to_owned)
}

fn is_safe_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn random_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect()
}

fn prune_expired_tokens(tokens: &mut HashMap<String, ProjectAssetToken>) {
    let now = Instant::now();
    tokens.retain(|_, entry| entry.expires_at > now);
}

fn renderer_kind_label(kind: &ProjectFileRendererKindDto) -> &'static str {
    match kind {
        ProjectFileRendererKindDto::Code => "code",
        ProjectFileRendererKindDto::Svg => "svg",
        ProjectFileRendererKindDto::Markdown => "markdown",
        ProjectFileRendererKindDto::Csv => "csv",
        ProjectFileRendererKindDto::Html => "html",
        ProjectFileRendererKindDto::Image => "image",
        ProjectFileRendererKindDto::Pdf => "pdf",
        ProjectFileRendererKindDto::Audio => "audio",
        ProjectFileRendererKindDto::Video => "video",
    }
}

fn etag(content_hash: &str) -> String {
    format!("\"sha256:{content_hash}\"")
}

fn bad_request(message: &str) -> http::Response<Vec<u8>> {
    status_response(http::StatusCode::BAD_REQUEST, message)
}

fn forbidden(message: &str) -> http::Response<Vec<u8>> {
    status_response(http::StatusCode::FORBIDDEN, message)
}

fn gone(message: &str) -> http::Response<Vec<u8>> {
    status_response(http::StatusCode::GONE, message)
}

fn not_found(message: &str) -> http::Response<Vec<u8>> {
    status_response(http::StatusCode::NOT_FOUND, message)
}

fn server_error(message: &str) -> http::Response<Vec<u8>> {
    status_response(http::StatusCode::INTERNAL_SERVER_ERROR, message)
}

fn command_error_response(status: http::StatusCode, message: &str) -> http::Response<Vec<u8>> {
    status_response(status, message)
}

fn range_not_satisfiable(total_len: u64) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(http::StatusCode::RANGE_NOT_SATISFIABLE)
        .header(http::header::CONTENT_RANGE, format!("bytes */{total_len}"))
        .header(http::header::CONTENT_LENGTH, "0")
        .body(Vec::new())
        .unwrap_or_else(|_| empty_response(http::StatusCode::RANGE_NOT_SATISFIABLE))
}

fn status_response(status: http::StatusCode, message: &str) -> http::Response<Vec<u8>> {
    let body = message.as_bytes().to_vec();
    http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(http::header::CONTENT_LENGTH, body.len().to_string())
        .body(body)
        .unwrap_or_else(|_| empty_response(status))
}

fn empty_response(status: http::StatusCode) -> http::Response<Vec<u8>> {
    let mut response = http::Response::new(Vec::new());
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use crate::commands::ReadProjectFileResponseDto;
    use tauri::Manager;

    #[test]
    fn parses_standard_and_suffix_ranges() {
        assert_eq!(
            parse_range_header("bytes=2-5", 10).unwrap(),
            Some(ByteRange { start: 2, end: 5 })
        );
        assert_eq!(
            parse_range_header("bytes=4-", 10).unwrap(),
            Some(ByteRange { start: 4, end: 9 })
        );
        assert_eq!(
            parse_range_header("bytes=-3", 10).unwrap(),
            Some(ByteRange { start: 7, end: 9 })
        );
    }

    #[test]
    fn rejects_invalid_ranges() {
        assert!(parse_range_header("items=0-1", 10).is_err());
        assert!(parse_range_header("bytes=9-4", 10).is_err());
        assert!(parse_range_header("bytes=20-21", 10).is_err());
        assert!(parse_range_header("bytes=0-1,2-3", 10).is_err());
    }

    #[test]
    fn serves_partial_content_with_range_headers() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("video.mp4");
        fs::write(&path, b"0123456789").expect("write file");
        let grant = ProjectAssetGrant {
            project_id: "project-1".into(),
            path: "/video.mp4".into(),
            byte_length: 10,
            modified_at: "2026-01-01T00:00:00Z".into(),
            content_hash: "abc123".into(),
            mime_type: "video/mp4".into(),
            renderer_kind: ProjectFileRendererKindDto::Video,
        };

        let response = serve_byte_range(
            &path,
            &grant,
            ByteRange { start: 2, end: 5 },
            &http::Method::GET,
        );

        assert_eq!(response.status(), http::StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.body(), b"2345");
        assert_eq!(
            response.headers().get(http::header::CONTENT_RANGE).unwrap(),
            "bytes 2-5/10"
        );
        assert_eq!(
            response
                .headers()
                .get(http::header::CONTENT_LENGTH)
                .unwrap(),
            "4"
        );
    }

    #[test]
    fn project_asset_protocol_serves_classified_preview_bytes_and_ranges() {
        let registry_root = tempfile::tempdir().expect("registry temp dir");
        let project_root = tempfile::tempdir().expect("project temp dir");
        let bytes = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x55, 0x66];
        fs::write(project_root.path().join("pixel.png"), bytes).expect("write png");
        let app = build_project_asset_test_app(&registry_root, project_root.path());

        let response = read_project_asset_test_file(&app, "/pixel.png");
        let ReadProjectFileResponseDto::Renderable {
            preview_url,
            content_hash,
            ..
        } = response
        else {
            panic!("expected renderable response");
        };

        let full_response = serve_request(
            app.handle(),
            http::Request::builder()
                .method(http::Method::GET)
                .uri(preview_url.as_str())
                .body(Vec::new())
                .expect("full request"),
        );

        assert_eq!(full_response.status(), http::StatusCode::OK);
        assert_eq!(full_response.body(), &bytes);
        assert_eq!(
            full_response
                .headers()
                .get(http::header::CONTENT_TYPE)
                .unwrap(),
            "image/png"
        );
        assert_eq!(
            full_response
                .headers()
                .get(http::header::CONTENT_LENGTH)
                .unwrap(),
            "10"
        );
        assert_eq!(
            full_response
                .headers()
                .get(http::header::CACHE_CONTROL)
                .unwrap(),
            "private, max-age=60"
        );
        assert_eq!(
            full_response.headers().get(http::header::ETAG).unwrap(),
            &etag(&content_hash)
        );
        assert_eq!(
            full_response.headers().get("X-Xero-Renderer-Kind").unwrap(),
            "image"
        );

        let range_response = serve_request(
            app.handle(),
            http::Request::builder()
                .method(http::Method::GET)
                .uri(preview_url.as_str())
                .header(http::header::RANGE, "bytes=8-9")
                .body(Vec::new())
                .expect("range request"),
        );

        assert_eq!(range_response.status(), http::StatusCode::PARTIAL_CONTENT);
        assert_eq!(range_response.body(), &[0x55, 0x66]);
        assert_eq!(
            range_response
                .headers()
                .get(http::header::CONTENT_RANGE)
                .unwrap(),
            "bytes 8-9/10"
        );
    }

    #[test]
    fn project_asset_protocol_expires_and_revokes_tokens() {
        let state = ProjectAssetState::default();
        let token = token_from_preview_url(&state.issue_preview_url(test_grant("/preview.png")));
        {
            let mut tokens = state.tokens.lock().expect("token lock");
            tokens.get_mut(&token).expect("issued token").expires_at =
                Instant::now() - Duration::from_secs(1);
        }

        assert!(state.take_valid_grant(&token).is_none());
        assert!(!state
            .tokens
            .lock()
            .expect("token lock")
            .contains_key(&token));

        let project_one_a = token_from_preview_url(&state.issue_preview_url(test_grant("/a.png")));
        let project_one_b = token_from_preview_url(&state.issue_preview_url(test_grant("/b.png")));
        let project_two = token_from_preview_url(&state.issue_preview_url(ProjectAssetGrant {
            project_id: "project-2".into(),
            path: "/a.png".into(),
            ..test_grant("/a.png")
        }));

        state.revoke_project_tokens("project-1", &["/a.png".into()]);

        assert!(state.take_valid_grant(&project_one_a).is_none());
        assert!(state.take_valid_grant(&project_one_b).is_some());
        assert!(state.take_valid_grant(&project_two).is_some());

        state.revoke_project_tokens("project-1", &[]);

        assert!(state.take_valid_grant(&project_one_b).is_none());
    }

    #[test]
    fn project_asset_protocol_rejects_unsafe_or_changed_grants() {
        let registry_root = tempfile::tempdir().expect("registry temp dir");
        let project_root = tempfile::tempdir().expect("project temp dir");
        let path = project_root.path().join("payload.png");
        fs::write(&path, [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]).expect("write png");
        let app = build_project_asset_test_app(&registry_root, project_root.path());

        let response = read_project_asset_test_file(&app, "/payload.png");
        let ReadProjectFileResponseDto::Renderable { preview_url, .. } = response else {
            panic!("expected renderable response");
        };
        fs::write(
            &path,
            [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x44],
        )
        .expect("change file");

        let changed_response = serve_request(
            app.handle(),
            http::Request::builder()
                .method(http::Method::GET)
                .uri(preview_url.as_str())
                .body(Vec::new())
                .expect("changed request"),
        );
        assert_eq!(changed_response.status(), http::StatusCode::GONE);

        let revoked_response = serve_request(
            app.handle(),
            http::Request::builder()
                .method(http::Method::GET)
                .uri(preview_url.as_str())
                .body(Vec::new())
                .expect("revoked request"),
        );
        assert_eq!(revoked_response.status(), http::StatusCode::NOT_FOUND);

        let unsafe_url = app
            .state::<ProjectAssetState>()
            .issue_preview_url(ProjectAssetGrant {
                path: "/../escape.png".into(),
                ..test_grant("/../escape.png")
            });
        let unsafe_response = serve_request(
            app.handle(),
            http::Request::builder()
                .method(http::Method::GET)
                .uri(unsafe_url.as_str())
                .body(Vec::new())
                .expect("unsafe request"),
        );

        assert_eq!(unsafe_response.status(), http::StatusCode::FORBIDDEN);
    }

    #[cfg(unix)]
    #[test]
    fn project_asset_protocol_denies_symlinked_paths() {
        use std::os::unix::fs::symlink;

        let registry_root = tempfile::tempdir().expect("registry temp dir");
        let project_root = tempfile::tempdir().expect("project temp dir");
        fs::write(
            project_root.path().join("real.png"),
            [0x89, b'P', b'N', b'G'],
        )
        .expect("write real file");
        symlink(
            project_root.path().join("real.png"),
            project_root.path().join("link.png"),
        )
        .expect("create symlink");
        let app = build_project_asset_test_app(&registry_root, project_root.path());
        let url = app
            .state::<ProjectAssetState>()
            .issue_preview_url(ProjectAssetGrant {
                path: "/link.png".into(),
                ..test_grant("/link.png")
            });

        let response = serve_request(
            app.handle(),
            http::Request::builder()
                .method(http::Method::GET)
                .uri(url.as_str())
                .body(Vec::new())
                .expect("symlink request"),
        );

        assert_eq!(response.status(), http::StatusCode::FORBIDDEN);
    }

    fn build_project_asset_test_app(
        registry_root: &tempfile::TempDir,
        project_root: &Path,
    ) -> tauri::App<tauri::test::MockRuntime> {
        let registry_path = registry_root.path().join("app-data").join("xero.db");
        crate::registry::upsert_project(
            &registry_path,
            crate::registry::RegistryProjectRecord {
                project_id: "project-1".into(),
                repository_id: "repository-1".into(),
                root_path: project_root.to_string_lossy().into_owned(),
            },
            &crate::state::ImportFailpoints::default(),
        )
        .expect("register project");

        crate::configure_builder_with_state(
            tauri::test::mock_builder(),
            crate::state::DesktopState::default().with_global_db_path_override(registry_path),
        )
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("build app")
    }

    fn read_project_asset_test_file(
        app: &tauri::App<tauri::test::MockRuntime>,
        path: &str,
    ) -> ReadProjectFileResponseDto {
        tauri::async_runtime::block_on(crate::commands::project_files::read_project_file(
            app.handle().clone(),
            app.state::<crate::state::DesktopState>(),
            app.state::<ProjectAssetState>(),
            crate::commands::ProjectFileRequestDto {
                project_id: "project-1".into(),
                path: path.into(),
            },
        ))
        .expect("read project file")
    }

    fn test_grant(path: &str) -> ProjectAssetGrant {
        ProjectAssetGrant {
            project_id: "project-1".into(),
            path: path.into(),
            byte_length: 10,
            modified_at: "2026-01-01T00:00:00Z".into(),
            content_hash: "abc123".into(),
            mime_type: "image/png".into(),
            renderer_kind: ProjectFileRendererKindDto::Image,
        }
    }

    fn token_from_preview_url(preview_url: &str) -> String {
        preview_url
            .strip_prefix("project-asset://")
            .expect("project asset URL")
            .to_owned()
    }
}
