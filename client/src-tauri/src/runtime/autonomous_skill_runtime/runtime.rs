use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};

use crate::{
    commands::{validate_non_empty, CommandError, CommandResult},
    state::DesktopState,
};

use super::{
    cache::{
        sha256_hex, AutonomousSkillCacheError, AutonomousSkillCacheStatus,
        AutonomousSkillCacheStore, FilesystemAutonomousSkillCacheStore,
    },
    inspection::{
        build_cache_manifest, discover_candidates_from_tree, inspect_tree_for_skill,
        join_relative_path, normalize_relative_source_path, normalize_skill_id,
        normalize_source_ref, normalize_source_repo, parse_skill_frontmatter, skill_matches_query,
        validate_source_metadata, InspectedSkill,
    },
    source::{
        github_token_from_env, map_source_error_for_discovery, map_source_error_for_source,
        AutonomousSkillSource, AutonomousSkillSourceFileRequest, AutonomousSkillSourceMetadata,
        AutonomousSkillSourceTreeRequest, AutonomousSkillSourceTreeResponse,
        GithubAutonomousSkillSource, GITHUB_API_BASE_URL,
    },
};

pub const AUTONOMOUS_SKILL_SOURCE_REPO: &str = "vercel-labs/skills";
pub const AUTONOMOUS_SKILL_SOURCE_REF: &str = "main";
pub const AUTONOMOUS_SKILL_SOURCE_ROOT: &str = "skills";

const DEFAULT_TIMEOUT_MS: u64 = 8_000;
const MAX_TIMEOUT_MS: u64 = 20_000;
const DEFAULT_DISCOVER_RESULT_LIMIT: usize = 5;
const MAX_DISCOVER_RESULT_LIMIT: usize = 10;
const MAX_DISCOVER_QUERY_CHARS: usize = 128;
pub(crate) const MAX_SKILL_FILES: usize = 32;
pub(crate) const MAX_SKILL_FILE_BYTES: usize = 128 * 1024;
pub(crate) const MAX_TOTAL_SKILL_BYTES: usize = 512 * 1024;
pub(crate) const ALLOWED_TEXT_EXTENSIONS: &[&str] = &[
    "md", "txt", "json", "yaml", "yml", "toml", "sh", "bash", "py", "js", "ts", "tsx", "jsx",
    "cjs", "mjs",
];

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSkillRegistryOperation {
    Install,
    Invoke,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousSkillRegistrySuccess {
    pub operation: AutonomousSkillRegistryOperation,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub user_invocable: Option<bool>,
    pub source: AutonomousSkillSourceMetadata,
    pub cache_key: String,
    pub cache_directory: String,
    pub cache_status: AutonomousSkillCacheStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousSkillRegistryFailure {
    pub operation: AutonomousSkillRegistryOperation,
    pub skill_id: String,
    pub source: AutonomousSkillSourceMetadata,
    pub cache_key: String,
    pub diagnostic: CommandError,
}

pub trait AutonomousSkillRegistrySink: Send + Sync {
    fn record_success(&self, event: &AutonomousSkillRegistrySuccess) -> CommandResult<()>;
    fn record_failure(&self, event: &AutonomousSkillRegistryFailure) -> CommandResult<()>;
}

#[derive(Clone)]
pub struct AutonomousSkillRuntime {
    config: AutonomousSkillRuntimeConfig,
    source: Arc<dyn AutonomousSkillSource>,
    cache: Arc<dyn AutonomousSkillCacheStore>,
    registry: Option<Arc<dyn AutonomousSkillRegistrySink>>,
}

impl fmt::Debug for AutonomousSkillRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AutonomousSkillRuntime")
            .field("config", &self.config)
            .field("has_source_override", &true)
            .field("has_cache_override", &true)
            .field("has_registry", &self.registry.is_some())
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
            registry: None,
        }
    }

    pub fn with_installed_skill_registry(
        mut self,
        registry: Arc<dyn AutonomousSkillRegistrySink>,
    ) -> Self {
        self.registry = Some(registry);
        self
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
                    "Xero requires autonomous skill discovery queries to be {} characters or fewer.",
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
        let source_for_failure = validate_source_metadata(&request.source).ok();
        let result = self.install_inner(request);
        match &result {
            Ok(output) => {
                self.record_registry_success(AutonomousSkillRegistrySuccess {
                    operation: AutonomousSkillRegistryOperation::Install,
                    skill_id: output.skill_id.clone(),
                    name: output.name.clone(),
                    description: output.description.clone(),
                    user_invocable: output.user_invocable,
                    source: output.source.clone(),
                    cache_key: output.cache_key.clone(),
                    cache_directory: output.cache_directory.clone(),
                    cache_status: output.cache_status,
                })?;
            }
            Err(error) => {
                if let Some(source) = source_for_failure {
                    self.record_registry_failure(AutonomousSkillRegistryFailure {
                        operation: AutonomousSkillRegistryOperation::Install,
                        skill_id: skill_id_for_source_path(&source.path),
                        cache_key: cache_key_for_source(&source),
                        source,
                        diagnostic: error.clone(),
                    });
                }
            }
        }
        result
    }

    fn install_inner(
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
        let source_for_failure = validate_source_metadata(&request.source).ok();
        let result = self.invoke_inner(request);
        match &result {
            Ok(output) => {
                self.record_registry_success(AutonomousSkillRegistrySuccess {
                    operation: AutonomousSkillRegistryOperation::Invoke,
                    skill_id: output.skill_id.clone(),
                    name: output.name.clone(),
                    description: output.description.clone(),
                    user_invocable: output.user_invocable,
                    source: output.source.clone(),
                    cache_key: output.cache_key.clone(),
                    cache_directory: output.cache_directory.clone(),
                    cache_status: output.cache_status,
                })?;
            }
            Err(error) => {
                if let Some(source) = source_for_failure {
                    self.record_registry_failure(AutonomousSkillRegistryFailure {
                        operation: AutonomousSkillRegistryOperation::Invoke,
                        skill_id: skill_id_for_source_path(&source.path),
                        cache_key: cache_key_for_source(&source),
                        source,
                        diagnostic: error.clone(),
                    });
                }
            }
        }
        result
    }

    fn invoke_inner(
        &self,
        request: AutonomousSkillInvokeRequest,
    ) -> CommandResult<AutonomousSkillInvokeOutput> {
        let install = self.install_inner(AutonomousSkillInstallRequest {
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
                    "Xero could not reload the installed autonomous skill cache manifest.",
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

    fn record_registry_success(&self, event: AutonomousSkillRegistrySuccess) -> CommandResult<()> {
        if let Some(registry) = self.registry.as_ref() {
            registry.record_success(&event)?;
        }
        Ok(())
    }

    fn record_registry_failure(&self, event: AutonomousSkillRegistryFailure) {
        if let Some(registry) = self.registry.as_ref() {
            let _ = registry.record_failure(&event);
        }
    }

    fn fetch_install_files(
        &self,
        inspected: &InspectedSkill,
        timeout_ms: u64,
    ) -> CommandResult<Vec<super::cache::AutonomousSkillCacheInstallFile>> {
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
                        "Xero rejected `{}` because skill assets must be {} bytes or smaller.",
                        asset.relative_path, self.config.limits.max_skill_file_bytes
                    ),
                ));
            }
            total_bytes = total_bytes.saturating_add(bytes.len());
            if total_bytes > self.config.limits.max_total_skill_bytes {
                return Err(CommandError::user_fixable(
                    "autonomous_skill_layout_unsupported",
                    format!(
                        "Xero rejected `{}` because the resolved autonomous skill exceeds the {} byte total cache budget.",
                        inspected.skill_id, self.config.limits.max_total_skill_bytes
                    ),
                ));
            }
            let text = String::from_utf8(bytes.clone()).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_skill_layout_unsupported",
                    format!(
                        "Xero rejected `{}` because skill asset `{}` was not valid UTF-8 text: {error}",
                        inspected.skill_id, asset.relative_path
                    ),
                )
            })?;
            if asset.relative_path == "SKILL.md" {
                parse_skill_frontmatter(&text)?;
            }
            files.push(super::cache::AutonomousSkillCacheInstallFile {
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
                    "Xero rejected `{}` because SKILL.md exceeds the {} byte limit.",
                    inspected.skill_id, self.config.limits.max_skill_file_bytes
                ),
            ));
        }
        let skill_markdown = String::from_utf8(skill_markdown).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_skill_document_invalid",
                format!(
                    "Xero rejected `{}` because SKILL.md was not valid UTF-8 text: {error}",
                    inspected.skill_id
                ),
            )
        })?;
        let frontmatter = parse_skill_frontmatter(&skill_markdown)?;
        if frontmatter.name != inspected.skill_id {
            return Err(CommandError::user_fixable(
                "autonomous_skill_document_invalid",
                format!(
                    "Xero rejected `{}` because SKILL.md frontmatter name `{}` did not match the resolved skill id.",
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
        let tree_request = AutonomousSkillSourceTreeRequest {
            repo: repo.to_owned(),
            reference: reference.to_owned(),
            timeout_ms,
        };
        match stage {
            SkillStage::Discovery => self
                .source
                .list_tree(&tree_request)
                .map_err(map_source_error_for_discovery),
            SkillStage::Source => self
                .source
                .list_tree(&tree_request)
                .map_err(map_source_error_for_source),
        }
    }
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
                "Xero requires autonomous skill discovery result_limit to be between 1 and {max_value}."
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
            format!("Xero requires {label} to be between 1 and {max_timeout_ms}."),
        ));
    }
    Ok(timeout_ms)
}

fn cache_key_for_source(source: &AutonomousSkillSourceMetadata) -> String {
    let skill_id = skill_id_for_source_path(&source.path);
    let digest = sha256_hex(format!("{}:{}", source.repo, source.path).as_bytes());
    format!("{}-{}", skill_id, &digest[..12])
}

fn skill_id_for_source_path(path: &str) -> String {
    path.rsplit('/').next().unwrap_or("skill").to_owned()
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
