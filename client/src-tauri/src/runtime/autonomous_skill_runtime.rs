use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use reqwest::{
    blocking::Client,
    header::{AUTHORIZATION, USER_AGENT},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime};
use url::Url;

use crate::{
    commands::{
        get_runtime_settings::write_json_file_atomically, validate_non_empty, CommandError,
        CommandResult,
    },
    state::DesktopState,
};

pub const AUTONOMOUS_SKILL_SOURCE_REPO: &str = "vercel-labs/skills";
pub const AUTONOMOUS_SKILL_SOURCE_REF: &str = "main";
pub const AUTONOMOUS_SKILL_SOURCE_ROOT: &str = "skills";

const DEFAULT_TIMEOUT_MS: u64 = 8_000;
const MAX_TIMEOUT_MS: u64 = 20_000;
const DEFAULT_DISCOVER_RESULT_LIMIT: usize = 5;
const MAX_DISCOVER_RESULT_LIMIT: usize = 10;
const MAX_DISCOVER_QUERY_CHARS: usize = 128;
const MAX_SKILL_FILES: usize = 32;
const MAX_SKILL_FILE_BYTES: usize = 128 * 1024;
const MAX_TOTAL_SKILL_BYTES: usize = 512 * 1024;
const CACHE_VERSION: u32 = 1;
const CACHE_MANIFEST_FILE_NAME: &str = "manifest.json";
const CACHE_TREES_DIRECTORY_NAME: &str = "trees";
const GITHUB_API_BASE_URL: &str = "https://api.github.com";
const GITHUB_USER_AGENT_VALUE: &str = "cadence-autonomous-skill-runtime";
const MAX_REDIRECTS: usize = 5;
const ALLOWED_TEXT_EXTENSIONS: &[&str] = &[
    "md", "txt", "json", "yaml", "yml", "toml", "sh", "bash", "py", "js", "ts", "tsx", "jsx",
    "cjs", "mjs",
];
const GITHUB_TOKEN_ENV_VARS: &[&str] = &["GITHUB_TOKEN", "GH_TOKEN"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutonomousSkillRuntimeLimits {
    pub default_timeout_ms: u64,
    pub max_timeout_ms: u64,
    pub default_discover_result_limit: usize,
    pub max_discover_result_limit: usize,
    pub max_discover_query_chars: usize,
    pub max_skill_files: usize,
    pub max_skill_file_bytes: usize,
    pub max_total_skill_bytes: usize,
}

impl Default for AutonomousSkillRuntimeLimits {
    fn default() -> Self {
        Self {
            default_timeout_ms: DEFAULT_TIMEOUT_MS,
            max_timeout_ms: MAX_TIMEOUT_MS,
            default_discover_result_limit: DEFAULT_DISCOVER_RESULT_LIMIT,
            max_discover_result_limit: MAX_DISCOVER_RESULT_LIMIT,
            max_discover_query_chars: MAX_DISCOVER_QUERY_CHARS,
            max_skill_files: MAX_SKILL_FILES,
            max_skill_file_bytes: MAX_SKILL_FILE_BYTES,
            max_total_skill_bytes: MAX_TOTAL_SKILL_BYTES,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AutonomousSkillRuntimeConfig {
    pub default_source_repo: String,
    pub default_source_ref: String,
    pub default_source_root: String,
    pub github_api_base_url: String,
    pub github_token: Option<String>,
    pub limits: AutonomousSkillRuntimeLimits,
}

impl AutonomousSkillRuntimeConfig {
    pub fn for_platform() -> Self {
        Self {
            default_source_repo: AUTONOMOUS_SKILL_SOURCE_REPO.into(),
            default_source_ref: AUTONOMOUS_SKILL_SOURCE_REF.into(),
            default_source_root: AUTONOMOUS_SKILL_SOURCE_ROOT.into(),
            github_api_base_url: GITHUB_API_BASE_URL.into(),
            github_token: github_token_from_env(),
            limits: AutonomousSkillRuntimeLimits::default(),
        }
    }
}

impl Default for AutonomousSkillRuntimeConfig {
    fn default() -> Self {
        Self::for_platform()
    }
}

impl fmt::Debug for AutonomousSkillRuntimeConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AutonomousSkillRuntimeConfig")
            .field("default_source_repo", &self.default_source_repo)
            .field("default_source_ref", &self.default_source_ref)
            .field("default_source_root", &self.default_source_root)
            .field("github_api_base_url", &self.github_api_base_url)
            .field(
                "has_github_token",
                &self
                    .github_token
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
            )
            .field("limits", &self.limits)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillDiscoverRequest {
    pub query: String,
    pub result_limit: Option<usize>,
    pub timeout_ms: Option<u64>,
    pub source_repo: Option<String>,
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillResolveRequest {
    pub skill_id: String,
    pub timeout_ms: Option<u64>,
    pub source_repo: Option<String>,
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillInstallRequest {
    pub source: AutonomousSkillSourceMetadata,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillInvokeRequest {
    pub source: AutonomousSkillSourceMetadata,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillDiscoverOutput {
    pub query: String,
    pub source_repo: String,
    pub source_ref: String,
    pub candidates: Vec<AutonomousSkillDiscoveryCandidate>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillDiscoveryCandidate {
    pub skill_id: String,
    pub source: AutonomousSkillSourceMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillResolveOutput {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub user_invocable: Option<bool>,
    pub source: AutonomousSkillSourceMetadata,
    pub asset_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillInstallOutput {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub user_invocable: Option<bool>,
    pub source: AutonomousSkillSourceMetadata,
    pub cache_key: String,
    pub cache_directory: String,
    pub cache_status: AutonomousSkillCacheStatus,
    pub asset_paths: Vec<String>,
    pub total_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillInvokeOutput {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub user_invocable: Option<bool>,
    pub source: AutonomousSkillSourceMetadata,
    pub cache_key: String,
    pub cache_directory: String,
    pub cache_status: AutonomousSkillCacheStatus,
    #[serde(default, skip_serializing, skip_deserializing)]
    pub skill_markdown: String,
    #[serde(default, skip_serializing, skip_deserializing)]
    pub supporting_assets: Vec<AutonomousSkillInvocationAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillInvocationAsset {
    pub relative_path: String,
    pub absolute_path: String,
    pub sha256: String,
    pub bytes: usize,
    #[serde(default, skip_serializing, skip_deserializing)]
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillSourceMetadata {
    pub repo: String,
    pub path: String,
    pub reference: String,
    pub tree_hash: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSkillCacheStatus {
    Miss,
    Hit,
    Refreshed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSkillSourceEntryKind {
    Blob,
    Tree,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillSourceTreeRequest {
    pub repo: String,
    pub reference: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillSourceTreeResponse {
    pub entries: Vec<AutonomousSkillSourceTreeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillSourceTreeEntry {
    pub path: String,
    pub kind: AutonomousSkillSourceEntryKind,
    pub hash: String,
    pub bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillSourceFileRequest {
    pub repo: String,
    pub reference: String,
    pub path: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillSourceFileResponse {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousSkillSourceError {
    Setup(String),
    Timeout(String),
    Status { status: u16, message: String },
    Transport(String),
    Decode(String),
}

pub trait AutonomousSkillSource: Send + Sync {
    fn list_tree(
        &self,
        request: &AutonomousSkillSourceTreeRequest,
    ) -> Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError>;

    fn fetch_file(
        &self,
        request: &AutonomousSkillSourceFileRequest,
    ) -> Result<AutonomousSkillSourceFileResponse, AutonomousSkillSourceError>;
}

#[derive(Clone, Default)]
pub struct GithubAutonomousSkillSource {
    api_base_url: String,
    github_token: Option<String>,
}

impl GithubAutonomousSkillSource {
    pub fn new(api_base_url: impl Into<String>, github_token: Option<String>) -> Self {
        Self {
            api_base_url: api_base_url.into(),
            github_token: github_token.filter(|value| !value.trim().is_empty()),
        }
    }

    fn client(&self, timeout_ms: u64) -> Result<Client, AutonomousSkillSourceError> {
        Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .redirect(Policy::limited(MAX_REDIRECTS))
            .build()
            .map_err(|error| {
                AutonomousSkillSourceError::Setup(format!(
                    "Cadence could not initialize the autonomous skill source client: {error}"
                ))
            })
    }

    fn add_common_headers(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        let request = request.header(USER_AGENT, GITHUB_USER_AGENT_VALUE);
        match self
            .github_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(token) => request.header(AUTHORIZATION, format!("Bearer {token}")),
            None => request,
        }
    }
}

impl fmt::Debug for GithubAutonomousSkillSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GithubAutonomousSkillSource")
            .field("api_base_url", &self.api_base_url)
            .field(
                "has_github_token",
                &self
                    .github_token
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
            )
            .finish()
    }
}

impl AutonomousSkillSource for GithubAutonomousSkillSource {
    fn list_tree(
        &self,
        request: &AutonomousSkillSourceTreeRequest,
    ) -> Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError> {
        let (owner, repo) = split_github_repo(&request.repo)?;
        let mut url = Url::parse(&self.api_base_url).map_err(|error| {
            AutonomousSkillSourceError::Setup(format!(
                "Cadence could not parse the autonomous skill API base URL: {error}"
            ))
        })?;
        {
            let mut segments = url.path_segments_mut().map_err(|_| {
                AutonomousSkillSourceError::Setup(
                    "Cadence could not build the autonomous skill tree API URL.".into(),
                )
            })?;
            segments.pop_if_empty();
            segments.extend([
                "repos",
                owner,
                repo,
                "git",
                "trees",
                request.reference.as_str(),
            ]);
        }
        url.query_pairs_mut().append_pair("recursive", "1");

        let client = self.client(request.timeout_ms)?;
        let response = self
            .add_common_headers(client.get(url))
            .send()
            .map_err(map_reqwest_source_error)?;
        let status = response.status().as_u16();
        if !(200..=299).contains(&status) {
            return Err(AutonomousSkillSourceError::Status {
                status,
                message: format!(
                    "Cadence received HTTP {status} while listing autonomous skill source tree `{}` at ref `{}`.",
                    request.repo, request.reference
                ),
            });
        }

        let payload = response.text().map_err(|error| {
            AutonomousSkillSourceError::Transport(format!(
                "Cadence could not read the autonomous skill tree response: {error}"
            ))
        })?;
        let decoded: GithubTreeResponse = serde_json::from_str(&payload).map_err(|error| {
            AutonomousSkillSourceError::Decode(format!(
                "Cadence could not decode the autonomous skill tree response: {error}"
            ))
        })?;

        let mut entries = Vec::with_capacity(decoded.tree.len());
        for entry in decoded.tree {
            let kind = match entry.kind.as_str() {
                "blob" => AutonomousSkillSourceEntryKind::Blob,
                "tree" => AutonomousSkillSourceEntryKind::Tree,
                other => {
                    return Err(AutonomousSkillSourceError::Decode(format!(
                        "Cadence received unsupported source tree entry kind `{other}` for `{}`.",
                        entry.path
                    )));
                }
            };
            let hash = entry.sha.ok_or_else(|| {
                AutonomousSkillSourceError::Decode(format!(
                    "Cadence received source metadata for `{}` without a tree/blob hash.",
                    entry.path
                ))
            })?;
            entries.push(AutonomousSkillSourceTreeEntry {
                path: entry.path,
                kind,
                hash,
                bytes: entry.size,
            });
        }

        Ok(AutonomousSkillSourceTreeResponse { entries })
    }

    fn fetch_file(
        &self,
        request: &AutonomousSkillSourceFileRequest,
    ) -> Result<AutonomousSkillSourceFileResponse, AutonomousSkillSourceError> {
        let (owner, repo) = split_github_repo(&request.repo)?;
        let mut url = Url::parse(&self.api_base_url).map_err(|error| {
            AutonomousSkillSourceError::Setup(format!(
                "Cadence could not parse the autonomous skill API base URL: {error}"
            ))
        })?;
        {
            let mut segments = url.path_segments_mut().map_err(|_| {
                AutonomousSkillSourceError::Setup(
                    "Cadence could not build the autonomous skill contents API URL.".into(),
                )
            })?;
            segments.pop_if_empty();
            segments.extend(["repos", owner, repo, "contents"]);
            for segment in request.path.split('/') {
                segments.push(segment);
            }
        }
        url.query_pairs_mut()
            .append_pair("ref", request.reference.as_str());

        let client = self.client(request.timeout_ms)?;
        let response = self
            .add_common_headers(client.get(url))
            .send()
            .map_err(map_reqwest_source_error)?;
        let status = response.status().as_u16();
        if !(200..=299).contains(&status) {
            return Err(AutonomousSkillSourceError::Status {
                status,
                message: format!(
                    "Cadence received HTTP {status} while fetching autonomous skill file `{}` from `{}` at ref `{}`.",
                    request.path, request.repo, request.reference
                ),
            });
        }

        let payload = response.text().map_err(|error| {
            AutonomousSkillSourceError::Transport(format!(
                "Cadence could not read the autonomous skill contents response: {error}"
            ))
        })?;
        let decoded: GithubContentsResponse = serde_json::from_str(&payload).map_err(|error| {
            AutonomousSkillSourceError::Decode(format!(
                "Cadence could not decode the autonomous skill contents response: {error}"
            ))
        })?;
        if decoded.kind != "file" {
            return Err(AutonomousSkillSourceError::Decode(format!(
                "Cadence expected `{}` to resolve to a file but received `{}`.",
                request.path, decoded.kind
            )));
        }
        if decoded.encoding != "base64" {
            return Err(AutonomousSkillSourceError::Decode(format!(
                "Cadence expected `{}` to be base64 encoded but received `{}`.",
                request.path, decoded.encoding
            )));
        }

        let normalized = decoded.content.replace('\n', "");
        let bytes = BASE64_STANDARD
            .decode(normalized.as_bytes())
            .map_err(|error| {
                AutonomousSkillSourceError::Decode(format!(
                    "Cadence could not decode base64 skill content for `{}`: {error}",
                    request.path
                ))
            })?;

        Ok(AutonomousSkillSourceFileResponse { bytes })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousSkillCacheError {
    Setup(String),
    Read(String),
    Write(String),
    Decode(String),
    Contract(String),
}

pub trait AutonomousSkillCacheStore: Send + Sync {
    fn load_manifest(
        &self,
        cache_key: &str,
    ) -> Result<Option<AutonomousSkillCacheManifest>, AutonomousSkillCacheError>;

    fn verify_manifest(
        &self,
        cache_key: &str,
        manifest: &AutonomousSkillCacheManifest,
    ) -> Result<String, AutonomousSkillCacheError>;

    fn install(
        &self,
        cache_key: &str,
        manifest: &AutonomousSkillCacheManifest,
        files: &[AutonomousSkillCacheInstallFile],
    ) -> Result<String, AutonomousSkillCacheError>;

    fn load_text_file(
        &self,
        cache_key: &str,
        tree_hash: &str,
        relative_path: &str,
    ) -> Result<String, AutonomousSkillCacheError>;
}

#[derive(Debug, Clone)]
pub struct FilesystemAutonomousSkillCacheStore {
    root: PathBuf,
}

impl FilesystemAutonomousSkillCacheStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn cache_directory(&self, cache_key: &str) -> PathBuf {
        self.root.join(cache_key)
    }

    fn manifest_path(&self, cache_key: &str) -> PathBuf {
        self.cache_directory(cache_key)
            .join(CACHE_MANIFEST_FILE_NAME)
    }

    fn tree_directory(&self, cache_key: &str, tree_hash: &str) -> PathBuf {
        self.cache_directory(cache_key)
            .join(CACHE_TREES_DIRECTORY_NAME)
            .join(tree_hash)
    }
}

impl AutonomousSkillCacheStore for FilesystemAutonomousSkillCacheStore {
    fn load_manifest(
        &self,
        cache_key: &str,
    ) -> Result<Option<AutonomousSkillCacheManifest>, AutonomousSkillCacheError> {
        let manifest_path = self.manifest_path(cache_key);
        if !manifest_path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&manifest_path).map_err(|error| {
            AutonomousSkillCacheError::Read(format!(
                "Cadence could not read the autonomous skill cache manifest at {}: {error}",
                manifest_path.display()
            ))
        })?;

        let manifest =
            serde_json::from_str::<AutonomousSkillCacheManifest>(&contents).map_err(|error| {
                AutonomousSkillCacheError::Decode(format!(
                    "Cadence could not decode the autonomous skill cache manifest at {}: {error}",
                    manifest_path.display()
                ))
            })?;

        validate_cache_manifest(&manifest)?;
        Ok(Some(manifest))
    }

    fn verify_manifest(
        &self,
        cache_key: &str,
        manifest: &AutonomousSkillCacheManifest,
    ) -> Result<String, AutonomousSkillCacheError> {
        validate_cache_manifest(manifest)?;
        let tree_directory = self.tree_directory(cache_key, &manifest.source.tree_hash);
        if !tree_directory.is_dir() {
            return Err(AutonomousSkillCacheError::Contract(format!(
                "Cadence expected an autonomous skill cache tree at {} but it was missing.",
                tree_directory.display()
            )));
        }

        let actual_paths = collect_relative_files(&tree_directory)?;
        let expected_paths = manifest
            .files
            .iter()
            .map(|record| record.relative_path.clone())
            .collect::<BTreeSet<_>>();
        if actual_paths != expected_paths {
            return Err(AutonomousSkillCacheError::Contract(format!(
                "Cadence detected autonomous skill cache drift for `{}` because the cached file set no longer matches the manifest.",
                manifest.skill_id
            )));
        }

        for record in &manifest.files {
            let path = tree_directory.join(&record.relative_path);
            let bytes = fs::read(&path).map_err(|error| {
                AutonomousSkillCacheError::Read(format!(
                    "Cadence could not read cached autonomous skill file {}: {error}",
                    path.display()
                ))
            })?;
            let digest = sha256_hex(&bytes);
            if digest != record.sha256 || bytes.len() != record.bytes {
                return Err(AutonomousSkillCacheError::Contract(format!(
                    "Cadence detected autonomous skill cache drift for `{}` at `{}`.",
                    manifest.skill_id, record.relative_path
                )));
            }
        }

        Ok(tree_directory.display().to_string())
    }

    fn install(
        &self,
        cache_key: &str,
        manifest: &AutonomousSkillCacheManifest,
        files: &[AutonomousSkillCacheInstallFile],
    ) -> Result<String, AutonomousSkillCacheError> {
        validate_cache_manifest(manifest)?;
        let cache_directory = self.cache_directory(cache_key);
        let tree_directory = self.tree_directory(cache_key, &manifest.source.tree_hash);
        fs::create_dir_all(&cache_directory).map_err(|error| {
            AutonomousSkillCacheError::Write(format!(
                "Cadence could not prepare the autonomous skill cache directory at {}: {error}",
                cache_directory.display()
            ))
        })?;

        if tree_directory.exists() {
            fs::remove_dir_all(&tree_directory).map_err(|error| {
                AutonomousSkillCacheError::Write(format!(
                    "Cadence could not clear the staged autonomous skill cache tree at {}: {error}",
                    tree_directory.display()
                ))
            })?;
        }
        fs::create_dir_all(&tree_directory).map_err(|error| {
            AutonomousSkillCacheError::Write(format!(
                "Cadence could not create the autonomous skill cache tree at {}: {error}",
                tree_directory.display()
            ))
        })?;

        for file in files {
            let path = tree_directory.join(&file.relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    AutonomousSkillCacheError::Write(format!(
                        "Cadence could not prepare the autonomous skill cache subdirectory at {}: {error}",
                        parent.display()
                    ))
                })?;
            }
            fs::write(&path, &file.bytes).map_err(|error| {
                AutonomousSkillCacheError::Write(format!(
                    "Cadence could not write cached autonomous skill file {}: {error}",
                    path.display()
                ))
            })?;
        }

        self.verify_manifest(cache_key, manifest)?;

        let manifest_bytes = serde_json::to_vec_pretty(manifest).map_err(|error| {
            AutonomousSkillCacheError::Write(format!(
                "Cadence could not serialize the autonomous skill cache manifest for `{}`: {error}",
                manifest.skill_id
            ))
        })?;
        write_json_file_atomically(
            &self.manifest_path(cache_key),
            &manifest_bytes,
            "autonomous_skill_cache",
        )
        .map_err(|error| {
            AutonomousSkillCacheError::Write(format!(
                "Cadence could not persist the autonomous skill cache manifest for `{}`: {error}",
                manifest.skill_id
            ))
        })?;

        Ok(tree_directory.display().to_string())
    }

    fn load_text_file(
        &self,
        cache_key: &str,
        tree_hash: &str,
        relative_path: &str,
    ) -> Result<String, AutonomousSkillCacheError> {
        let path = self
            .tree_directory(cache_key, tree_hash)
            .join(relative_path);
        let bytes = fs::read(&path).map_err(|error| {
            AutonomousSkillCacheError::Read(format!(
                "Cadence could not read cached autonomous skill file {}: {error}",
                path.display()
            ))
        })?;
        String::from_utf8(bytes).map_err(|error| {
            AutonomousSkillCacheError::Decode(format!(
                "Cadence could not decode cached autonomous skill file {} as UTF-8: {error}",
                path.display()
            ))
        })
    }
}

#[derive(Clone)]
pub struct AutonomousSkillRuntime {
    config: AutonomousSkillRuntimeConfig,
    source: Arc<dyn AutonomousSkillSource>,
    cache: Arc<dyn AutonomousSkillCacheStore>,
}

impl fmt::Debug for AutonomousSkillRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AutonomousSkillRuntime")
            .field("config", &self.config)
            .field("has_source_override", &true)
            .field("has_cache_override", &true)
            .finish()
    }
}

impl AutonomousSkillRuntime {
    pub fn new(config: AutonomousSkillRuntimeConfig, cache_root: impl AsRef<Path>) -> Self {
        let source = Arc::new(GithubAutonomousSkillSource::new(
            config.github_api_base_url.clone(),
            config.github_token.clone(),
        ));
        let cache = Arc::new(FilesystemAutonomousSkillCacheStore::new(
            cache_root.as_ref().to_path_buf(),
        ));
        Self::with_source_and_cache(config, source, cache)
    }

    pub fn for_app<R: Runtime>(app: &AppHandle<R>, state: &DesktopState) -> CommandResult<Self> {
        let cache_root = state.autonomous_skill_cache_dir(app)?;
        Ok(Self::new(
            AutonomousSkillRuntimeConfig::for_platform(),
            cache_root,
        ))
    }

    pub fn with_source_and_cache(
        config: AutonomousSkillRuntimeConfig,
        source: Arc<dyn AutonomousSkillSource>,
        cache: Arc<dyn AutonomousSkillCacheStore>,
    ) -> Self {
        Self {
            config,
            source,
            cache,
        }
    }

    pub fn config(&self) -> &AutonomousSkillRuntimeConfig {
        &self.config
    }

    pub fn discover(
        &self,
        request: AutonomousSkillDiscoverRequest,
    ) -> CommandResult<AutonomousSkillDiscoverOutput> {
        validate_non_empty(&request.query, "query")?;
        if request.query.chars().count() > self.config.limits.max_discover_query_chars {
            return Err(CommandError::user_fixable(
                "autonomous_skill_discovery_query_too_large",
                format!(
                    "Cadence requires autonomous skill discovery queries to be {} characters or fewer.",
                    self.config.limits.max_discover_query_chars
                ),
            ));
        }

        let result_limit = normalize_result_limit(
            request.result_limit,
            self.config.limits.default_discover_result_limit,
            self.config.limits.max_discover_result_limit,
        )?;
        let timeout_ms = normalize_timeout_ms(
            request.timeout_ms,
            self.config.limits.default_timeout_ms,
            self.config.limits.max_timeout_ms,
            "autonomous_skill_discovery_timeout_invalid",
            "autonomous skill discovery timeout_ms",
        )?;
        let source_repo = normalize_source_repo(
            request
                .source_repo
                .as_deref()
                .unwrap_or(&self.config.default_source_repo),
        )?;
        let source_ref = normalize_source_ref(
            request
                .source_ref
                .as_deref()
                .unwrap_or(&self.config.default_source_ref),
        )?;
        let source_root = normalize_relative_source_path(&self.config.default_source_root)?;
        let tree = self.load_tree(&source_repo, &source_ref, timeout_ms, SkillStage::Discovery)?;
        let candidates =
            discover_candidates_from_tree(&tree, &source_repo, &source_ref, &source_root)?;
        let mut matched = candidates
            .into_iter()
            .filter(|candidate| skill_matches_query(&candidate.skill_id, &request.query))
            .collect::<Vec<_>>();
        matched.sort_by(|left, right| left.skill_id.cmp(&right.skill_id));
        let truncated = matched.len() > result_limit;
        matched.truncate(result_limit);

        Ok(AutonomousSkillDiscoverOutput {
            query: request.query,
            source_repo,
            source_ref,
            candidates: matched,
            truncated,
        })
    }

    pub fn resolve(
        &self,
        request: AutonomousSkillResolveRequest,
    ) -> CommandResult<AutonomousSkillResolveOutput> {
        validate_non_empty(&request.skill_id, "skillId")?;
        let skill_id = normalize_skill_id(&request.skill_id)?;
        let timeout_ms = normalize_timeout_ms(
            request.timeout_ms,
            self.config.limits.default_timeout_ms,
            self.config.limits.max_timeout_ms,
            "autonomous_skill_source_timeout_invalid",
            "autonomous skill source timeout_ms",
        )?;
        let source_repo = normalize_source_repo(
            request
                .source_repo
                .as_deref()
                .unwrap_or(&self.config.default_source_repo),
        )?;
        let source_ref = normalize_source_ref(
            request
                .source_ref
                .as_deref()
                .unwrap_or(&self.config.default_source_ref),
        )?;
        let source_root = normalize_relative_source_path(&self.config.default_source_root)?;
        let skill_path = join_relative_path(&source_root, &skill_id);
        let inspected = self.inspect_skill(
            SkillInspectionTarget::BySkillPath {
                repo: source_repo,
                reference: source_ref,
                path: skill_path,
                expected_tree_hash: None,
            },
            timeout_ms,
        )?;

        Ok(AutonomousSkillResolveOutput {
            skill_id: inspected.skill_id,
            name: inspected.name,
            description: inspected.description,
            user_invocable: inspected.user_invocable,
            source: inspected.source,
            asset_paths: inspected
                .assets
                .iter()
                .map(|asset| asset.relative_path.clone())
                .collect(),
        })
    }

    pub fn install(
        &self,
        request: AutonomousSkillInstallRequest,
    ) -> CommandResult<AutonomousSkillInstallOutput> {
        let source = validate_source_metadata(&request.source)?;
        let timeout_ms = normalize_timeout_ms(
            request.timeout_ms,
            self.config.limits.default_timeout_ms,
            self.config.limits.max_timeout_ms,
            "autonomous_skill_source_timeout_invalid",
            "autonomous skill source timeout_ms",
        )?;
        let cache_key = cache_key_for_source(&source);
        let existing_manifest = self
            .cache
            .load_manifest(&cache_key)
            .map_err(map_cache_error_for_install)?;
        if let Some(manifest) = existing_manifest.as_ref() {
            if manifest.source == source {
                let cache_directory = self
                    .cache
                    .verify_manifest(&cache_key, manifest)
                    .map_err(map_cache_error_for_install)?;
                return Ok(AutonomousSkillInstallOutput {
                    skill_id: manifest.skill_id.clone(),
                    name: manifest.name.clone(),
                    description: manifest.description.clone(),
                    user_invocable: manifest.user_invocable,
                    source: manifest.source.clone(),
                    cache_key,
                    cache_directory,
                    cache_status: AutonomousSkillCacheStatus::Hit,
                    asset_paths: manifest
                        .files
                        .iter()
                        .map(|record| record.relative_path.clone())
                        .collect(),
                    total_bytes: manifest.files.iter().map(|record| record.bytes).sum(),
                });
            }
        }

        let inspected = self.inspect_skill(
            SkillInspectionTarget::ResolvedSource(source.clone()),
            timeout_ms,
        )?;
        let files = self.fetch_install_files(&inspected, timeout_ms)?;
        let manifest = build_cache_manifest(&inspected, &files);
        let cache_directory = self
            .cache
            .install(&cache_key, &manifest, &files)
            .map_err(map_cache_error_for_install)?;
        let cache_status = if existing_manifest.is_some() {
            AutonomousSkillCacheStatus::Refreshed
        } else {
            AutonomousSkillCacheStatus::Miss
        };

        Ok(AutonomousSkillInstallOutput {
            skill_id: manifest.skill_id,
            name: manifest.name,
            description: manifest.description,
            user_invocable: manifest.user_invocable,
            source: manifest.source,
            cache_key,
            cache_directory,
            cache_status,
            asset_paths: manifest
                .files
                .iter()
                .map(|record| record.relative_path.clone())
                .collect(),
            total_bytes: manifest.files.iter().map(|record| record.bytes).sum(),
        })
    }

    pub fn invoke(
        &self,
        request: AutonomousSkillInvokeRequest,
    ) -> CommandResult<AutonomousSkillInvokeOutput> {
        let install = self.install(AutonomousSkillInstallRequest {
            source: request.source,
            timeout_ms: request.timeout_ms,
        })?;
        let manifest = self
            .cache
            .load_manifest(&install.cache_key)
            .map_err(map_cache_error_for_invoke)?
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "autonomous_skill_cache_contract_failed",
                    "Cadence could not reload the installed autonomous skill cache manifest.",
                )
            })?;
        let skill_markdown = self
            .cache
            .load_text_file(&install.cache_key, &manifest.source.tree_hash, "SKILL.md")
            .map_err(map_cache_error_for_invoke)?;
        parse_skill_frontmatter(&skill_markdown)?;

        let mut supporting_assets = Vec::new();
        for file in &manifest.files {
            if file.relative_path == "SKILL.md" {
                continue;
            }
            let content = self
                .cache
                .load_text_file(
                    &install.cache_key,
                    &manifest.source.tree_hash,
                    &file.relative_path,
                )
                .map_err(map_cache_error_for_invoke)?;
            let absolute_path = PathBuf::from(&install.cache_directory)
                .join(&file.relative_path)
                .display()
                .to_string();
            supporting_assets.push(AutonomousSkillInvocationAsset {
                relative_path: file.relative_path.clone(),
                absolute_path,
                sha256: file.sha256.clone(),
                bytes: file.bytes,
                content,
            });
        }

        Ok(AutonomousSkillInvokeOutput {
            skill_id: manifest.skill_id,
            name: manifest.name,
            description: manifest.description,
            user_invocable: manifest.user_invocable,
            source: manifest.source,
            cache_key: install.cache_key,
            cache_directory: install.cache_directory,
            cache_status: install.cache_status,
            skill_markdown,
            supporting_assets,
        })
    }

    fn fetch_install_files(
        &self,
        inspected: &InspectedSkill,
        timeout_ms: u64,
    ) -> CommandResult<Vec<AutonomousSkillCacheInstallFile>> {
        let mut files = Vec::with_capacity(inspected.assets.len());
        let mut total_bytes = 0usize;

        for asset in &inspected.assets {
            let bytes = if asset.relative_path == "SKILL.md" {
                inspected.skill_markdown.as_bytes().to_vec()
            } else {
                self.source
                    .fetch_file(&AutonomousSkillSourceFileRequest {
                        repo: inspected.source.repo.clone(),
                        reference: inspected.source.reference.clone(),
                        path: asset.source_path.clone(),
                        timeout_ms,
                    })
                    .map_err(map_source_error_for_source)?
                    .bytes
            };
            if bytes.len() > self.config.limits.max_skill_file_bytes {
                return Err(CommandError::user_fixable(
                    "autonomous_skill_layout_unsupported",
                    format!(
                        "Cadence rejected `{}` because skill assets must be {} bytes or smaller.",
                        asset.relative_path, self.config.limits.max_skill_file_bytes
                    ),
                ));
            }
            total_bytes = total_bytes.saturating_add(bytes.len());
            if total_bytes > self.config.limits.max_total_skill_bytes {
                return Err(CommandError::user_fixable(
                    "autonomous_skill_layout_unsupported",
                    format!(
                        "Cadence rejected `{}` because the resolved autonomous skill exceeds the {} byte total cache budget.",
                        inspected.skill_id, self.config.limits.max_total_skill_bytes
                    ),
                ));
            }
            let text = String::from_utf8(bytes.clone()).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_skill_layout_unsupported",
                    format!(
                        "Cadence rejected `{}` because skill asset `{}` was not valid UTF-8 text: {error}",
                        inspected.skill_id, asset.relative_path
                    ),
                )
            })?;
            if asset.relative_path == "SKILL.md" {
                parse_skill_frontmatter(&text)?;
            }
            files.push(AutonomousSkillCacheInstallFile {
                relative_path: asset.relative_path.clone(),
                bytes: text.into_bytes(),
            });
        }

        Ok(files)
    }

    fn inspect_skill(
        &self,
        target: SkillInspectionTarget,
        timeout_ms: u64,
    ) -> CommandResult<InspectedSkill> {
        let (repo, reference, skill_path, expected_tree_hash) = match target {
            SkillInspectionTarget::BySkillPath {
                repo,
                reference,
                path,
                expected_tree_hash,
            } => (repo, reference, path, expected_tree_hash),
            SkillInspectionTarget::ResolvedSource(source) => (
                source.repo,
                source.reference,
                source.path,
                Some(source.tree_hash),
            ),
        };

        let tree = self.load_tree(&repo, &reference, timeout_ms, SkillStage::Source)?;
        let inspected = inspect_tree_for_skill(
            &tree,
            &repo,
            &reference,
            &skill_path,
            expected_tree_hash.as_deref(),
            self.config.limits.max_skill_files,
        )?;
        let skill_markdown = self
            .source
            .fetch_file(&AutonomousSkillSourceFileRequest {
                repo: repo.clone(),
                reference: reference.clone(),
                path: format!("{skill_path}/SKILL.md"),
                timeout_ms,
            })
            .map_err(map_source_error_for_source)?
            .bytes;
        if skill_markdown.len() > self.config.limits.max_skill_file_bytes {
            return Err(CommandError::user_fixable(
                "autonomous_skill_layout_unsupported",
                format!(
                    "Cadence rejected `{}` because SKILL.md exceeds the {} byte limit.",
                    inspected.skill_id, self.config.limits.max_skill_file_bytes
                ),
            ));
        }
        let skill_markdown = String::from_utf8(skill_markdown).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_skill_document_invalid",
                format!(
                    "Cadence rejected `{}` because SKILL.md was not valid UTF-8 text: {error}",
                    inspected.skill_id
                ),
            )
        })?;
        let frontmatter = parse_skill_frontmatter(&skill_markdown)?;
        if frontmatter.name != inspected.skill_id {
            return Err(CommandError::user_fixable(
                "autonomous_skill_document_invalid",
                format!(
                    "Cadence rejected `{}` because SKILL.md frontmatter name `{}` did not match the resolved skill id.",
                    inspected.skill_id, frontmatter.name
                ),
            ));
        }

        Ok(InspectedSkill {
            skill_id: inspected.skill_id,
            name: frontmatter.name,
            description: frontmatter.description,
            user_invocable: frontmatter.user_invocable,
            source: inspected.source,
            assets: inspected.assets,
            skill_markdown,
        })
    }

    fn load_tree(
        &self,
        repo: &str,
        reference: &str,
        timeout_ms: u64,
        stage: SkillStage,
    ) -> CommandResult<AutonomousSkillSourceTreeResponse> {
        self.source
            .list_tree(&AutonomousSkillSourceTreeRequest {
                repo: repo.to_owned(),
                reference: reference.to_owned(),
                timeout_ms,
            })
            .map_err(|error| map_source_error(stage, error))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InspectedSkill {
    skill_id: String,
    name: String,
    description: String,
    user_invocable: Option<bool>,
    source: AutonomousSkillSourceMetadata,
    assets: Vec<InspectedSkillAsset>,
    skill_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InspectedSkillAsset {
    relative_path: String,
    source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TreeSkillInspection {
    skill_id: String,
    source: AutonomousSkillSourceMetadata,
    assets: Vec<InspectedSkillAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillFrontmatter {
    name: String,
    description: String,
    user_invocable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillCacheManifest {
    pub version: u32,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub user_invocable: Option<bool>,
    pub source: AutonomousSkillSourceMetadata,
    pub cached_at: String,
    pub files: Vec<AutonomousSkillCacheManifestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillCacheManifestFile {
    pub relative_path: String,
    pub sha256: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousSkillCacheInstallFile {
    pub relative_path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillStage {
    Discovery,
    Source,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SkillInspectionTarget {
    BySkillPath {
        repo: String,
        reference: String,
        path: String,
        expected_tree_hash: Option<String>,
    },
    ResolvedSource(AutonomousSkillSourceMetadata),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GithubTreeResponse {
    tree: Vec<GithubTreeEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GithubTreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    sha: Option<String>,
    size: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GithubContentsResponse {
    #[serde(rename = "type")]
    kind: String,
    encoding: String,
    content: String,
}

fn build_cache_manifest(
    inspected: &InspectedSkill,
    files: &[AutonomousSkillCacheInstallFile],
) -> AutonomousSkillCacheManifest {
    AutonomousSkillCacheManifest {
        version: CACHE_VERSION,
        skill_id: inspected.skill_id.clone(),
        name: inspected.name.clone(),
        description: inspected.description.clone(),
        user_invocable: inspected.user_invocable,
        source: inspected.source.clone(),
        cached_at: crate::auth::now_timestamp(),
        files: files
            .iter()
            .map(|file| AutonomousSkillCacheManifestFile {
                relative_path: file.relative_path.clone(),
                sha256: sha256_hex(&file.bytes),
                bytes: file.bytes.len(),
            })
            .collect(),
    }
}

fn discover_candidates_from_tree(
    tree: &AutonomousSkillSourceTreeResponse,
    repo: &str,
    reference: &str,
    source_root: &str,
) -> CommandResult<Vec<AutonomousSkillDiscoveryCandidate>> {
    let source_root_prefix = format!("{source_root}/");
    let mut candidates = Vec::new();

    for entry in &tree.entries {
        if entry.kind != AutonomousSkillSourceEntryKind::Tree {
            continue;
        }
        if !entry.path.starts_with(&source_root_prefix) {
            continue;
        }
        let relative = entry
            .path
            .strip_prefix(&source_root_prefix)
            .unwrap_or_default();
        if relative.is_empty() || relative.contains('/') {
            continue;
        }
        let skill_id = normalize_skill_id(relative)?;
        candidates.push(AutonomousSkillDiscoveryCandidate {
            skill_id,
            source: AutonomousSkillSourceMetadata {
                repo: repo.to_owned(),
                path: entry.path.clone(),
                reference: reference.to_owned(),
                tree_hash: entry.hash.clone(),
            },
        });
    }

    Ok(candidates)
}

fn inspect_tree_for_skill(
    tree: &AutonomousSkillSourceTreeResponse,
    repo: &str,
    reference: &str,
    skill_path: &str,
    expected_tree_hash: Option<&str>,
    max_skill_files: usize,
) -> CommandResult<TreeSkillInspection> {
    let skill_path = normalize_relative_source_path(skill_path)?;
    let skill_id = skill_path
        .rsplit('/')
        .next()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_skill_source_metadata_invalid",
                "Cadence requires autonomous skill source paths to include a skill id.",
            )
        })?
        .to_owned();
    normalize_skill_id(&skill_id)?;

    let root_entry = tree
        .entries
        .iter()
        .find(|entry| {
            entry.kind == AutonomousSkillSourceEntryKind::Tree && entry.path == skill_path
        })
        .ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_skill_not_found",
                format!("Cadence could not find autonomous skill `{skill_id}` at `{skill_path}`."),
            )
        })?;

    if let Some(expected_tree_hash) = expected_tree_hash {
        if root_entry.hash != expected_tree_hash {
            return Err(CommandError::user_fixable(
                "autonomous_skill_source_changed",
                format!(
                    "Cadence resolved `{skill_id}` at tree hash `{expected_tree_hash}`, but the latest source now reports `{}`.",
                    root_entry.hash
                ),
            ));
        }
    }

    let mut assets = Vec::new();
    let asset_prefix = format!("{skill_path}/");
    for entry in &tree.entries {
        if entry.kind != AutonomousSkillSourceEntryKind::Blob {
            continue;
        }
        if !entry.path.starts_with(&asset_prefix) {
            continue;
        }
        let relative_path = entry.path.strip_prefix(&asset_prefix).unwrap_or_default();
        validate_skill_asset_path(relative_path)?;
        if let Some(bytes) = entry.bytes {
            if bytes > MAX_SKILL_FILE_BYTES {
                return Err(CommandError::user_fixable(
                    "autonomous_skill_layout_unsupported",
                    format!(
                        "Cadence rejected `{skill_id}` because asset `{relative_path}` exceeded the {} byte per-file limit.",
                        MAX_SKILL_FILE_BYTES
                    ),
                ));
            }
        }
        assets.push(InspectedSkillAsset {
            relative_path: relative_path.to_owned(),
            source_path: entry.path.clone(),
        });
    }

    if assets.is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_missing",
            format!(
                "Cadence rejected `{skill_id}` because the source path `{skill_path}` contained no files."
            ),
        ));
    }
    if assets.len() > max_skill_files {
        return Err(CommandError::user_fixable(
            "autonomous_skill_layout_unsupported",
            format!(
                "Cadence rejected `{skill_id}` because it exceeded the {} file limit for autonomous skill assets.",
                max_skill_files
            ),
        ));
    }
    if !assets.iter().any(|asset| asset.relative_path == "SKILL.md") {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_missing",
            format!(
                "Cadence rejected `{skill_id}` because SKILL.md was missing from `{skill_path}`."
            ),
        ));
    }

    assets.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    Ok(TreeSkillInspection {
        skill_id,
        source: AutonomousSkillSourceMetadata {
            repo: repo.to_owned(),
            path: skill_path,
            reference: reference.to_owned(),
            tree_hash: root_entry.hash.clone(),
        },
        assets,
    })
}

fn parse_skill_frontmatter(markdown: &str) -> CommandResult<SkillFrontmatter> {
    let mut lines = markdown.lines();
    if lines.next() != Some("---") {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            "Cadence requires autonomous skill SKILL.md files to start with YAML frontmatter delimited by `---`.",
        ));
    }

    let mut entries = BTreeMap::new();
    let mut found_closing = false;
    for line in lines {
        if line.trim() == "---" {
            found_closing = true;
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let Some((raw_key, raw_value)) = line.split_once(':') else {
            return Err(CommandError::user_fixable(
                "autonomous_skill_document_invalid",
                format!(
                    "Cadence rejected SKILL.md because frontmatter line `{}` was not `key: value`.",
                    line.trim()
                ),
            ));
        };
        let key = raw_key.trim().to_owned();
        let value = raw_value.trim().to_owned();
        if key.is_empty() || value.is_empty() {
            return Err(CommandError::user_fixable(
                "autonomous_skill_document_invalid",
                format!(
                    "Cadence rejected SKILL.md because frontmatter line `{}` was missing a key or value.",
                    line.trim()
                ),
            ));
        }
        entries.insert(key, strip_wrapping_quotes(&value));
    }

    if !found_closing {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            "Cadence rejected SKILL.md because the YAML frontmatter block was not closed.",
        ));
    }

    let name = entries.remove("name").ok_or_else(|| {
        CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            "Cadence rejected SKILL.md because frontmatter `name` was missing.",
        )
    })?;
    let description = entries.remove("description").ok_or_else(|| {
        CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            "Cadence rejected SKILL.md because frontmatter `description` was missing.",
        )
    })?;
    let name = normalize_skill_id(&name)?;
    if description.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            "Cadence rejected SKILL.md because frontmatter `description` was blank.",
        ));
    }

    let user_invocable = match entries.remove("user-invocable") {
        Some(value) => Some(parse_frontmatter_bool(&value)?),
        None => None,
    };

    Ok(SkillFrontmatter {
        name,
        description: description.trim().to_owned(),
        user_invocable,
    })
}

fn parse_frontmatter_bool(value: &str) -> CommandResult<bool> {
    match value.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(CommandError::user_fixable(
            "autonomous_skill_document_invalid",
            format!(
                "Cadence rejected SKILL.md because frontmatter boolean value `{other}` was invalid."
            ),
        )),
    }
}

fn validate_skill_asset_path(relative_path: &str) -> CommandResult<()> {
    let normalized = normalize_relative_source_path(relative_path)?;
    if normalized == "SKILL.md" {
        return Ok(());
    }
    if normalized.ends_with("/SKILL.md") {
        return Err(CommandError::user_fixable(
            "autonomous_skill_layout_unsupported",
            format!(
                "Cadence rejected skill layout because nested SKILL.md file `{normalized}` is not supported."
            ),
        ));
    }
    let extension = Path::new(&normalized)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_skill_layout_unsupported",
                format!(
                    "Cadence rejected skill asset `{normalized}` because extensionless supporting files are not supported."
                ),
            )
        })?;
    if !ALLOWED_TEXT_EXTENSIONS
        .iter()
        .any(|allowed| *allowed == extension.as_str())
    {
        return Err(CommandError::user_fixable(
            "autonomous_skill_layout_unsupported",
            format!(
                "Cadence rejected skill asset `{normalized}` because `.{extension}` files are not supported in the autonomous skill cache."
            ),
        ));
    }

    Ok(())
}

fn validate_source_metadata(
    source: &AutonomousSkillSourceMetadata,
) -> CommandResult<AutonomousSkillSourceMetadata> {
    let repo = normalize_source_repo(&source.repo)?;
    let path = normalize_relative_source_path(&source.path)?;
    let reference = normalize_source_ref(&source.reference)?;
    let tree_hash = normalize_tree_hash(&source.tree_hash)?;
    Ok(AutonomousSkillSourceMetadata {
        repo,
        path,
        reference,
        tree_hash,
    })
}

fn validate_cache_manifest(
    manifest: &AutonomousSkillCacheManifest,
) -> Result<(), AutonomousSkillCacheError> {
    if manifest.version != CACHE_VERSION {
        return Err(AutonomousSkillCacheError::Contract(format!(
            "Cadence rejected cached autonomous skill manifest version `{}` because only version `{CACHE_VERSION}` is supported.",
            manifest.version
        )));
    }
    normalize_skill_id(&manifest.skill_id).map_err(command_error_to_cache_contract)?;
    validate_source_metadata(&manifest.source).map_err(command_error_to_cache_contract)?;
    if manifest.name.trim().is_empty() || manifest.description.trim().is_empty() {
        return Err(AutonomousSkillCacheError::Contract(
            "Cadence rejected a cached autonomous skill manifest because name/description were blank."
                .into(),
        ));
    }
    let mut seen_paths = BTreeSet::new();
    for record in &manifest.files {
        let normalized = normalize_relative_source_path(&record.relative_path)
            .map_err(command_error_to_cache_contract)?;
        if !seen_paths.insert(normalized.clone()) {
            return Err(AutonomousSkillCacheError::Contract(format!(
                "Cadence rejected cached autonomous skill manifest for `{}` because `{normalized}` was duplicated.",
                manifest.skill_id
            )));
        }
        if record.sha256.trim().is_empty() || record.bytes == 0 {
            return Err(AutonomousSkillCacheError::Contract(format!(
                "Cadence rejected cached autonomous skill manifest for `{}` because `{normalized}` had incomplete file metadata.",
                manifest.skill_id
            )));
        }
    }
    if !seen_paths.contains("SKILL.md") {
        return Err(AutonomousSkillCacheError::Contract(format!(
            "Cadence rejected cached autonomous skill manifest for `{}` because SKILL.md was missing.",
            manifest.skill_id
        )));
    }
    Ok(())
}

fn collect_relative_files(root: &Path) -> Result<BTreeSet<String>, AutonomousSkillCacheError> {
    let mut files = BTreeSet::new();
    collect_relative_files_inner(root, root, &mut files)?;
    Ok(files)
}

fn collect_relative_files_inner(
    root: &Path,
    current: &Path,
    files: &mut BTreeSet<String>,
) -> Result<(), AutonomousSkillCacheError> {
    let entries = fs::read_dir(current).map_err(|error| {
        AutonomousSkillCacheError::Read(format!(
            "Cadence could not enumerate cached autonomous skill directory {}: {error}",
            current.display()
        ))
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            AutonomousSkillCacheError::Read(format!(
                "Cadence could not inspect a cached autonomous skill directory entry under {}: {error}",
                current.display()
            ))
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_relative_files_inner(root, &path, files)?;
            continue;
        }
        let relative = path.strip_prefix(root).map_err(|error| {
            AutonomousSkillCacheError::Read(format!(
                "Cadence could not normalize cached autonomous skill path {}: {error}",
                path.display()
            ))
        })?;
        let normalized = path_to_forward_slash(relative);
        files.insert(normalized);
    }

    Ok(())
}

fn normalize_result_limit(
    value: Option<usize>,
    default_value: usize,
    max_value: usize,
) -> CommandResult<usize> {
    let value = value.unwrap_or(default_value);
    if value == 0 || value > max_value {
        return Err(CommandError::user_fixable(
            "autonomous_skill_discovery_result_limit_invalid",
            format!(
                "Cadence requires autonomous skill discovery result_limit to be between 1 and {max_value}."
            ),
        ));
    }
    Ok(value)
}

fn normalize_timeout_ms(
    value: Option<u64>,
    default_timeout_ms: u64,
    max_timeout_ms: u64,
    error_code: &'static str,
    label: &'static str,
) -> CommandResult<u64> {
    let timeout_ms = value.unwrap_or(default_timeout_ms);
    if timeout_ms == 0 || timeout_ms > max_timeout_ms {
        return Err(CommandError::user_fixable(
            error_code,
            format!("Cadence requires {label} to be between 1 and {max_timeout_ms}."),
        ));
    }
    Ok(timeout_ms)
}

fn normalize_source_repo(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("sourceRepo"));
    }
    if trimmed.contains("://") || trimmed.starts_with("git@") {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_repo_invalid",
            "Cadence currently supports only GitHub `owner/repo` shorthand for autonomous skill sources.",
        ));
    }
    let mut segments = trimmed.split('/');
    let Some(owner) = segments.next() else {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_repo_invalid",
            "Cadence requires autonomous skill source repositories to use `owner/repo` format.",
        ));
    };
    let Some(repo) = segments.next() else {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_repo_invalid",
            "Cadence requires autonomous skill source repositories to use `owner/repo` format.",
        ));
    };
    if segments.next().is_some() || owner.trim().is_empty() || repo.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_repo_invalid",
            "Cadence requires autonomous skill source repositories to use `owner/repo` format.",
        ));
    }
    Ok(format!("{}/{}", owner.trim(), repo.trim()))
}

fn normalize_source_ref(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("sourceRef"));
    }
    if trimmed.contains("://") || trimmed.starts_with("refs/") {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_ref_invalid",
            "Cadence requires autonomous skill source refs to be a branch, tag, or commit identifier, not a URL or raw `refs/...` path.",
        ));
    }
    Ok(trimmed.to_owned())
}

fn normalize_tree_hash(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.len() != 40 || !trimmed.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_metadata_invalid",
            "Cadence requires autonomous skill source tree_hash values to be 40-character hexadecimal Git tree hashes.",
        ));
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn normalize_skill_id(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("skillId"));
    }
    if !trimmed
        .chars()
        .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == '-')
    {
        return Err(CommandError::user_fixable(
            "autonomous_skill_id_invalid",
            "Cadence requires autonomous skill ids to be lowercase kebab-case values.",
        ));
    }
    Ok(trimmed.to_owned())
}

fn normalize_relative_source_path(value: &str) -> CommandResult<String> {
    let path = Path::new(value.trim());
    if value.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_metadata_invalid",
            "Cadence requires autonomous skill source paths to be non-empty relative paths.",
        ));
    }

    let mut normalized = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => {
                let segment = segment.to_str().ok_or_else(|| {
                    CommandError::user_fixable(
                        "autonomous_skill_source_metadata_invalid",
                        "Cadence requires autonomous skill source paths to be valid UTF-8.",
                    )
                })?;
                if segment.is_empty() {
                    return Err(CommandError::user_fixable(
                        "autonomous_skill_source_metadata_invalid",
                        "Cadence rejected an autonomous skill source path with an empty segment.",
                    ));
                }
                normalized.push(segment.to_owned());
            }
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(CommandError::user_fixable(
                    "autonomous_skill_source_metadata_invalid",
                    "Cadence requires autonomous skill source paths to stay within a relative skill root.",
                ));
            }
        }
    }

    if normalized.is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_skill_source_metadata_invalid",
            "Cadence requires autonomous skill source paths to be non-empty relative paths.",
        ));
    }

    Ok(normalized.join("/"))
}

fn strip_wrapping_quotes(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        trimmed[1..trimmed.len() - 1].to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn cache_key_for_source(source: &AutonomousSkillSourceMetadata) -> String {
    let skill_id = source.path.rsplit('/').next().unwrap_or("skill");
    let digest = sha256_hex(format!("{}:{}", source.repo, source.path).as_bytes());
    format!("{}-{}", skill_id, &digest[..12])
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn path_to_forward_slash(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(segment) => segment.to_str().map(ToOwned::to_owned),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn join_relative_path(prefix: &str, leaf: &str) -> String {
    format!("{prefix}/{leaf}")
}

fn skill_matches_query(skill_id: &str, query: &str) -> bool {
    let haystack = skill_id.replace('-', " ").to_ascii_lowercase();
    query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .all(|term| haystack.contains(&term))
}

fn split_github_repo(repo: &str) -> Result<(&str, &str), AutonomousSkillSourceError> {
    let (owner, name) = repo.split_once('/').ok_or_else(|| {
        AutonomousSkillSourceError::Setup(
            "Cadence requires autonomous skill source repositories to use `owner/repo` format."
                .into(),
        )
    })?;
    Ok((owner, name))
}

fn command_error_to_cache_contract(error: CommandError) -> AutonomousSkillCacheError {
    AutonomousSkillCacheError::Contract(error.message)
}

fn map_reqwest_source_error(error: reqwest::Error) -> AutonomousSkillSourceError {
    if error.is_timeout() {
        return AutonomousSkillSourceError::Timeout(
            "Cadence timed out while contacting the autonomous skill source.".into(),
        );
    }

    AutonomousSkillSourceError::Transport(format!(
        "Cadence could not contact the autonomous skill source: {error}"
    ))
}

fn map_source_error(stage: SkillStage, error: AutonomousSkillSourceError) -> CommandError {
    match stage {
        SkillStage::Discovery => match error {
            AutonomousSkillSourceError::Setup(message) => {
                CommandError::system_fault("autonomous_skill_discovery_unavailable", message)
            }
            AutonomousSkillSourceError::Timeout(message) => {
                CommandError::retryable("autonomous_skill_discovery_timeout", message)
            }
            AutonomousSkillSourceError::Status { status, message } => match status {
                401 | 403 | 404 => {
                    CommandError::user_fixable("autonomous_skill_discovery_source_invalid", message)
                }
                408 | 429 | 500..=599 => {
                    CommandError::retryable("autonomous_skill_discovery_transport_failed", message)
                }
                _ => CommandError::user_fixable(
                    "autonomous_skill_discovery_transport_failed",
                    message,
                ),
            },
            AutonomousSkillSourceError::Transport(message) => {
                CommandError::retryable("autonomous_skill_discovery_transport_failed", message)
            }
            AutonomousSkillSourceError::Decode(message) => {
                CommandError::user_fixable("autonomous_skill_discovery_source_invalid", message)
            }
        },
        SkillStage::Source => map_source_error_for_source(error),
    }
}

fn map_source_error_for_source(error: AutonomousSkillSourceError) -> CommandError {
    match error {
        AutonomousSkillSourceError::Setup(message) => {
            CommandError::system_fault("autonomous_skill_source_unavailable", message)
        }
        AutonomousSkillSourceError::Timeout(message) => {
            CommandError::retryable("autonomous_skill_source_timeout", message)
        }
        AutonomousSkillSourceError::Status { status, message } => match status {
            401 | 403 | 404 => {
                CommandError::user_fixable("autonomous_skill_source_invalid", message)
            }
            408 | 429 | 500..=599 => {
                CommandError::retryable("autonomous_skill_source_transport_failed", message)
            }
            _ => CommandError::user_fixable("autonomous_skill_source_transport_failed", message),
        },
        AutonomousSkillSourceError::Transport(message) => {
            CommandError::retryable("autonomous_skill_source_transport_failed", message)
        }
        AutonomousSkillSourceError::Decode(message) => {
            CommandError::user_fixable("autonomous_skill_source_invalid", message)
        }
    }
}

fn map_cache_error_for_install(error: AutonomousSkillCacheError) -> CommandError {
    match error {
        AutonomousSkillCacheError::Setup(message) => {
            CommandError::system_fault("autonomous_skill_cache_unavailable", message)
        }
        AutonomousSkillCacheError::Read(message) => {
            CommandError::retryable("autonomous_skill_cache_read_failed", message)
        }
        AutonomousSkillCacheError::Write(message) => {
            CommandError::retryable("autonomous_skill_cache_write_failed", message)
        }
        AutonomousSkillCacheError::Decode(message) => {
            CommandError::user_fixable("autonomous_skill_cache_decode_failed", message)
        }
        AutonomousSkillCacheError::Contract(message) => {
            CommandError::user_fixable("autonomous_skill_cache_drift", message)
        }
    }
}

fn map_cache_error_for_invoke(error: AutonomousSkillCacheError) -> CommandError {
    match error {
        AutonomousSkillCacheError::Setup(message) => {
            CommandError::system_fault("autonomous_skill_cache_unavailable", message)
        }
        AutonomousSkillCacheError::Read(message) => {
            CommandError::retryable("autonomous_skill_cache_read_failed", message)
        }
        AutonomousSkillCacheError::Write(message) => {
            CommandError::retryable("autonomous_skill_cache_write_failed", message)
        }
        AutonomousSkillCacheError::Decode(message) => {
            CommandError::user_fixable("autonomous_skill_cache_decode_failed", message)
        }
        AutonomousSkillCacheError::Contract(message) => {
            CommandError::user_fixable("autonomous_skill_cache_contract_failed", message)
        }
    }
}

fn github_token_from_env() -> Option<String> {
    GITHUB_TOKEN_ENV_VARS.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}
