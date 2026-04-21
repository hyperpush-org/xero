use std::{fmt, time::Duration};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use reqwest::{
    blocking::Client,
    header::{AUTHORIZATION, USER_AGENT},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::commands::CommandError;

pub(crate) const GITHUB_API_BASE_URL: &str = "https://api.github.com";
const GITHUB_USER_AGENT_VALUE: &str = "Cadence-autonomous-skill-runtime";
const MAX_REDIRECTS: usize = 5;
const GITHUB_TOKEN_ENV_VARS: &[&str] = &["GITHUB_TOKEN", "GH_TOKEN"];

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

pub(crate) fn map_source_error_for_discovery(error: AutonomousSkillSourceError) -> CommandError {
    match error {
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
            _ => CommandError::user_fixable("autonomous_skill_discovery_transport_failed", message),
        },
        AutonomousSkillSourceError::Transport(message) => {
            CommandError::retryable("autonomous_skill_discovery_transport_failed", message)
        }
        AutonomousSkillSourceError::Decode(message) => {
            CommandError::user_fixable("autonomous_skill_discovery_source_invalid", message)
        }
    }
}

pub(crate) fn map_source_error_for_source(error: AutonomousSkillSourceError) -> CommandError {
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

pub(crate) fn github_token_from_env() -> Option<String> {
    GITHUB_TOKEN_ENV_VARS.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
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
