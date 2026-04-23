use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    commands::{
        get_runtime_settings::{remove_file_if_exists, write_json_file_atomically},
        CommandError, CommandResult,
    },
    runtime::{
        resolve_runtime_provider_identity, ANTHROPIC_PROVIDER_ID, AZURE_OPENAI_PROVIDER_ID,
        GEMINI_AI_STUDIO_PROVIDER_ID, GEMINI_RUNTIME_KIND, GITHUB_MODELS_PROVIDER_ID,
        OPENAI_API_PROVIDER_ID, OPENAI_CODEX_PROVIDER_ID, OPENAI_COMPATIBLE_RUNTIME_KIND,
        OPENROUTER_PROVIDER_ID,
    },
};

pub const PROVIDER_PROFILES_FILE_NAME: &str = "provider-profiles.json";
pub const PROVIDER_PROFILE_CREDENTIAL_STORE_FILE_NAME: &str = "provider-profile-credentials.json";
pub const OPENAI_CODEX_DEFAULT_PROFILE_ID: &str = "openai_codex-default";
pub const OPENROUTER_DEFAULT_PROFILE_ID: &str = "openrouter-default";
pub const ANTHROPIC_DEFAULT_PROFILE_ID: &str = "anthropic-default";
pub const GITHUB_MODELS_DEFAULT_PROFILE_ID: &str = "github_models-default";
pub const OPENROUTER_FALLBACK_MODEL_ID: &str = "openai/gpt-4.1-mini";
const PROVIDER_PROFILES_SCHEMA_VERSION: u32 = 2;
const OPENAI_CODEX_DEFAULT_PROFILE_LABEL: &str = "OpenAI Codex";
const OPENROUTER_DEFAULT_PROFILE_LABEL: &str = "OpenRouter";
const ANTHROPIC_DEFAULT_PROFILE_LABEL: &str = "Anthropic";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfilesMetadataFile {
    #[serde(default = "provider_profiles_schema_version")]
    pub version: u32,
    pub active_profile_id: String,
    #[serde(default)]
    pub profiles: Vec<ProviderProfileRecord>,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<ProviderProfilesMigrationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfileRecord {
    pub profile_id: String,
    pub provider_id: String,
    #[serde(default)]
    pub runtime_kind: String,
    pub label: String,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_link: Option<ProviderProfileCredentialLink>,
    #[serde(default)]
    pub migrated_from_legacy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migrated_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderProfileCredentialLink {
    #[serde(rename = "openai_codex")]
    OpenAiCodex {
        account_id: String,
        session_id: String,
        updated_at: String,
    },
    #[serde(rename = "api_key", alias = "openrouter", alias = "anthropic")]
    ApiKey { updated_at: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfilesMigrationState {
    pub source: String,
    pub migrated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_settings_updated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openrouter_credentials_updated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_auth_updated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openrouter_model_inferred: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderApiKeyCredentialEntry {
    pub profile_id: String,
    pub api_key: String,
    pub updated_at: String,
}

pub type OpenRouterProfileCredentialEntry = ProviderApiKeyCredentialEntry;
pub type AnthropicProfileCredentialEntry = ProviderApiKeyCredentialEntry;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfileCredentialsFile {
    #[serde(default)]
    pub api_keys: Vec<ProviderApiKeyCredentialEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyProviderProfileCredentialsFile {
    #[serde(default)]
    openrouter_api_keys: Vec<ProviderApiKeyCredentialEntry>,
    #[serde(default)]
    anthropic_api_keys: Vec<ProviderApiKeyCredentialEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderProfilesSnapshot {
    pub metadata: ProviderProfilesMetadataFile,
    pub credentials: ProviderProfileCredentialsFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderProfileReadinessStatus {
    Ready,
    Missing,
    Malformed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderProfileReadinessProjection {
    pub ready: bool,
    pub status: ProviderProfileReadinessStatus,
    pub credential_updated_at: Option<String>,
}

impl ProviderProfilesSnapshot {
    pub fn active_profile(&self) -> Option<&ProviderProfileRecord> {
        self.metadata
            .profiles
            .iter()
            .find(|profile| profile.profile_id == self.metadata.active_profile_id)
    }

    pub fn profile(&self, profile_id: &str) -> Option<&ProviderProfileRecord> {
        self.metadata
            .profiles
            .iter()
            .find(|profile| profile.profile_id == profile_id)
    }

    pub fn any_openrouter_api_key_configured(&self) -> bool {
        self.metadata.profiles.iter().any(|profile| {
            profile.provider_id == OPENROUTER_PROVIDER_ID
                && profile.readiness(&self.credentials).ready
        })
    }

    pub fn any_anthropic_api_key_configured(&self) -> bool {
        self.metadata.profiles.iter().any(|profile| {
            profile.provider_id == ANTHROPIC_PROVIDER_ID
                && profile.readiness(&self.credentials).ready
        })
    }

    pub fn preferred_openrouter_credential(&self) -> Option<&OpenRouterProfileCredentialEntry> {
        self.preferred_api_key_credential_for_provider(OPENROUTER_PROVIDER_ID)
    }

    pub fn preferred_anthropic_credential(&self) -> Option<&AnthropicProfileCredentialEntry> {
        self.preferred_api_key_credential_for_provider(ANTHROPIC_PROVIDER_ID)
    }

    pub fn api_key_credential(&self, profile_id: &str) -> Option<&ProviderApiKeyCredentialEntry> {
        self.credentials
            .api_keys
            .iter()
            .find(|entry| entry.profile_id == profile_id)
    }

    pub fn openrouter_credential(
        &self,
        profile_id: &str,
    ) -> Option<&OpenRouterProfileCredentialEntry> {
        self.api_key_credential(profile_id)
    }

    pub fn anthropic_credential(
        &self,
        profile_id: &str,
    ) -> Option<&AnthropicProfileCredentialEntry> {
        self.api_key_credential(profile_id)
    }

    pub fn matched_api_key_credential_for_profile(
        &self,
        profile_id: &str,
    ) -> Option<&ProviderApiKeyCredentialEntry> {
        self.profile(profile_id)
            .and_then(|profile| self.matched_api_key_credential(profile))
    }

    fn preferred_api_key_credential_for_provider(
        &self,
        provider_id: &str,
    ) -> Option<&ProviderApiKeyCredentialEntry> {
        self.active_profile()
            .filter(|profile| profile.provider_id == provider_id)
            .and_then(|profile| self.matched_api_key_credential(profile))
            .or_else(|| {
                self.metadata
                    .profiles
                    .iter()
                    .filter(|profile| profile.provider_id == provider_id)
                    .find_map(|profile| self.matched_api_key_credential(profile))
            })
    }

    fn matched_api_key_credential(
        &self,
        profile: &ProviderProfileRecord,
    ) -> Option<&ProviderApiKeyCredentialEntry> {
        let ProviderProfileCredentialLink::ApiKey { updated_at } = profile.credential_link.as_ref()?
        else {
            return None;
        };

        self.api_key_credential(&profile.profile_id)
            .filter(|entry| entry.updated_at == *updated_at)
    }
}

impl ProviderProfileRecord {
    pub fn readiness(
        &self,
        credentials: &ProviderProfileCredentialsFile,
    ) -> ProviderProfileReadinessProjection {
        match &self.credential_link {
            Some(ProviderProfileCredentialLink::OpenAiCodex { updated_at, .. }) => {
                ProviderProfileReadinessProjection {
                    ready: true,
                    status: ProviderProfileReadinessStatus::Ready,
                    credential_updated_at: Some(updated_at.clone()),
                }
            }
            Some(ProviderProfileCredentialLink::ApiKey { updated_at }) => {
                let matched_secret = credentials.api_keys.iter().any(|entry| {
                    entry.profile_id == self.profile_id && entry.updated_at == *updated_at
                });
                if matched_secret {
                    ProviderProfileReadinessProjection {
                        ready: true,
                        status: ProviderProfileReadinessStatus::Ready,
                        credential_updated_at: Some(updated_at.clone()),
                    }
                } else {
                    ProviderProfileReadinessProjection {
                        ready: false,
                        status: ProviderProfileReadinessStatus::Malformed,
                        credential_updated_at: Some(updated_at.clone()),
                    }
                }
            }
            None => ProviderProfileReadinessProjection {
                ready: false,
                status: ProviderProfileReadinessStatus::Missing,
                credential_updated_at: None,
            },
        }
    }
}

pub fn default_provider_profiles_snapshot() -> ProviderProfilesSnapshot {
    let timestamp = crate::auth::now_timestamp();
    ProviderProfilesSnapshot {
        metadata: ProviderProfilesMetadataFile {
            version: PROVIDER_PROFILES_SCHEMA_VERSION,
            active_profile_id: OPENAI_CODEX_DEFAULT_PROFILE_ID.into(),
            profiles: vec![build_openai_default_profile(None, None, &timestamp)],
            updated_at: timestamp,
            migration: None,
        },
        credentials: ProviderProfileCredentialsFile::default(),
    }
}

pub fn load_provider_profiles_from_paths(
    metadata_path: &Path,
    credentials_path: &Path,
) -> CommandResult<Option<ProviderProfilesSnapshot>> {
    let metadata_exists = metadata_path.exists();
    let credentials_exists = credentials_path.exists();

    if !metadata_exists && !credentials_exists {
        return Ok(None);
    }

    if !metadata_exists && credentials_exists {
        return Err(CommandError::user_fixable(
            "provider_profiles_contract_failed",
            format!(
                "Cadence found provider-profile credentials at {} without the matching provider-profile metadata file at {}.",
                credentials_path.display(),
                metadata_path.display()
            ),
        ));
    }

    let metadata = read_provider_profiles_metadata_file(metadata_path)?.expect("metadata exists");
    let credentials = read_provider_profile_credentials_file(credentials_path)?.unwrap_or_default();

    Ok(Some(validate_provider_profiles_contract(
        metadata,
        credentials,
        metadata_path,
        credentials_path,
    )?))
}

pub(crate) fn read_provider_profiles_metadata_file(
    path: &Path,
) -> CommandResult<Option<ProviderProfilesMetadataFile>> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path).map_err(|error| {
        CommandError::retryable(
            "provider_profiles_read_failed",
            format!(
                "Cadence could not read the app-local provider-profile metadata file at {}: {error}",
                path.display()
            ),
        )
    })?;

    let parsed = serde_json::from_str::<ProviderProfilesMetadataFile>(&contents).map_err(|error| {
        CommandError::user_fixable(
            "provider_profiles_decode_failed",
            format!(
                "Cadence could not decode the app-local provider-profile metadata file at {}: {error}",
                path.display()
            ),
        )
    })?;

    Ok(Some(parsed))
}

pub(crate) fn read_provider_profile_credentials_file(
    path: &Path,
) -> CommandResult<Option<ProviderProfileCredentialsFile>> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path).map_err(|error| {
        CommandError::retryable(
            "provider_profile_credentials_read_failed",
            format!(
                "Cadence could not read the app-local provider-profile credential file at {}: {error}",
                path.display()
            ),
        )
    })?;

    let parsed = decode_provider_profile_credentials_file(&contents, path)?;
    Ok(Some(parsed))
}

fn decode_provider_profile_credentials_file(
    contents: &str,
    path: &Path,
) -> CommandResult<ProviderProfileCredentialsFile> {
    let value = serde_json::from_str::<serde_json::Value>(contents).map_err(|error| {
        CommandError::user_fixable(
            "provider_profile_credentials_decode_failed",
            format!(
                "Cadence could not decode the app-local provider-profile credential file at {}: {error}",
                path.display()
            ),
        )
    })?;

    if value.get("apiKeys").is_some() {
        return serde_json::from_value::<ProviderProfileCredentialsFile>(value).map_err(|error| {
            CommandError::user_fixable(
                "provider_profile_credentials_decode_failed",
                format!(
                    "Cadence could not decode the app-local provider-profile credential file at {}: {error}",
                    path.display()
                ),
            )
        });
    }

    let legacy = serde_json::from_value::<LegacyProviderProfileCredentialsFile>(value).map_err(
        |error| {
            CommandError::user_fixable(
                "provider_profile_credentials_decode_failed",
                format!(
                    "Cadence could not decode the app-local provider-profile credential file at {}: {error}",
                    path.display()
                ),
            )
        },
    )?;

    let mut api_keys = legacy.openrouter_api_keys;
    api_keys.extend(legacy.anthropic_api_keys);
    Ok(ProviderProfileCredentialsFile { api_keys })
}

pub(crate) fn validate_provider_profiles_contract(
    metadata: ProviderProfilesMetadataFile,
    credentials: ProviderProfileCredentialsFile,
    metadata_path: &Path,
    credentials_path: &Path,
) -> CommandResult<ProviderProfilesSnapshot> {
    let metadata = validate_provider_profiles_metadata(metadata, metadata_path)?;
    let credentials = validate_provider_profile_credentials(credentials, credentials_path)?;
    let credentials_by_profile = api_key_credentials_by_profile(&credentials);

    for profile in &metadata.profiles {
        match &profile.credential_link {
            Some(ProviderProfileCredentialLink::OpenAiCodex { .. }) => {
                if profile.provider_id != OPENAI_CODEX_PROVIDER_ID {
                    return Err(CommandError::user_fixable(
                        "provider_profiles_invalid",
                        format!(
                            "Cadence rejected provider profile `{}` because the OpenAI auth linkage does not match provider `{}`.",
                            profile.profile_id, profile.provider_id
                        ),
                    ));
                }
                ensure_no_api_key_secret(
                    profile.profile_id.as_str(),
                    &credentials_by_profile,
                    credentials_path,
                )?;
            }
            Some(ProviderProfileCredentialLink::ApiKey { .. }) => {
                if profile.provider_id == OPENAI_CODEX_PROVIDER_ID {
                    return Err(CommandError::user_fixable(
                        "provider_profiles_invalid",
                        format!(
                            "Cadence rejected provider profile `{}` because OpenAI Codex profiles cannot use api_key credential links.",
                            profile.profile_id
                        ),
                    ));
                }
            }
            None => {
                ensure_no_api_key_secret(
                    profile.profile_id.as_str(),
                    &credentials_by_profile,
                    credentials_path,
                )?;
            }
        }
    }

    ensure_all_credentials_reference_profiles(&metadata, &credentials_by_profile, credentials_path)?;

    Ok(ProviderProfilesSnapshot {
        metadata,
        credentials,
    })
}

pub(crate) fn persist_provider_profiles_snapshot(
    metadata_path: &Path,
    credentials_path: &Path,
    next: &ProviderProfilesSnapshot,
) -> CommandResult<()> {
    let next = normalize_snapshot_for_persist(next.clone())?;

    let previous_metadata = snapshot_existing_file(metadata_path, "provider_profiles")?;
    let previous_credentials =
        snapshot_existing_file(credentials_path, "provider_profile_credentials")?;

    let metadata_json = serialize_pretty_json(&next.metadata, "provider_profiles")?;
    write_json_file_atomically(metadata_path, &metadata_json, "provider_profiles")?;

    let credential_result = if next.credentials.api_keys.is_empty() {
        remove_file_if_exists(credentials_path, "provider_profile_credentials")
    } else {
        let credentials_json =
            serialize_pretty_json(&next.credentials, "provider_profile_credentials")?;
        write_json_file_atomically(
            credentials_path,
            &credentials_json,
            "provider_profile_credentials",
        )
    };

    if let Err(error) = credential_result {
        return match restore_file_snapshot(
            metadata_path,
            previous_metadata.as_deref(),
            "provider_profiles_rollback",
        ) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(CommandError::retryable(
                "provider_profiles_rollback_failed",
                format!(
                    "Cadence failed to persist provider-profile credentials after writing metadata, and then could not restore the previous metadata file at {}: {}. Original credential error: {}",
                    metadata_path.display(),
                    rollback_error.message,
                    error.message
                ),
            )),
        };
    }

    if let Err(error) = validate_provider_profiles_contract(
        next.metadata.clone(),
        next.credentials.clone(),
        metadata_path,
        credentials_path,
    ) {
        let metadata_rollback = restore_file_snapshot(
            metadata_path,
            previous_metadata.as_deref(),
            "provider_profiles_rollback",
        );
        let credentials_rollback = restore_file_snapshot(
            credentials_path,
            previous_credentials.as_deref(),
            "provider_profile_credentials_rollback",
        );

        return match (metadata_rollback, credentials_rollback) {
            (Ok(()), Ok(())) => Err(error),
            (metadata_result, credentials_result) => Err(CommandError::retryable(
                "provider_profiles_rollback_failed",
                format!(
                    "Cadence rejected the persisted provider-profile state after writing it and could not fully restore the previous files (metadata rollback: {}; credential rollback: {}). Validation error: {}",
                    rollback_message(metadata_result),
                    rollback_message(credentials_result),
                    error.message
                ),
            )),
        };
    }

    Ok(())
}

pub(crate) fn snapshot_existing_file(
    path: &Path,
    operation: &str,
) -> CommandResult<Option<Vec<u8>>> {
    if !path.exists() {
        return Ok(None);
    }

    fs::read(path).map(Some).map_err(|error| {
        CommandError::retryable(
            format!("{operation}_read_failed"),
            format!(
                "Cadence could not snapshot the existing file at {} before updating it: {error}",
                path.display()
            ),
        )
    })
}

pub(crate) fn restore_file_snapshot(
    path: &Path,
    snapshot: Option<&[u8]>,
    operation: &str,
) -> CommandResult<()> {
    match snapshot {
        Some(bytes) => write_json_file_atomically(path, bytes, operation),
        None => remove_file_if_exists(path, operation),
    }
}

pub(crate) fn build_openai_default_profile(
    credential_link: Option<ProviderProfileCredentialLink>,
    migrated_at: Option<&str>,
    updated_at: &str,
) -> ProviderProfileRecord {
    ProviderProfileRecord {
        profile_id: OPENAI_CODEX_DEFAULT_PROFILE_ID.into(),
        provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
        runtime_kind: OPENAI_CODEX_PROVIDER_ID.into(),
        label: OPENAI_CODEX_DEFAULT_PROFILE_LABEL.into(),
        model_id: OPENAI_CODEX_PROVIDER_ID.into(),
        preset_id: None,
        base_url: None,
        api_version: None,
        credential_link,
        migrated_from_legacy: migrated_at.is_some(),
        migrated_at: migrated_at.map(str::to_owned),
        updated_at: updated_at.to_owned(),
    }
}

pub(crate) fn build_openrouter_default_profile(
    model_id: &str,
    credential_link: Option<ProviderProfileCredentialLink>,
    migrated_at: Option<&str>,
    updated_at: &str,
) -> ProviderProfileRecord {
    ProviderProfileRecord {
        profile_id: OPENROUTER_DEFAULT_PROFILE_ID.into(),
        provider_id: OPENROUTER_PROVIDER_ID.into(),
        runtime_kind: OPENROUTER_PROVIDER_ID.into(),
        label: OPENROUTER_DEFAULT_PROFILE_LABEL.into(),
        model_id: model_id.trim().to_owned(),
        preset_id: Some(OPENROUTER_PROVIDER_ID.into()),
        base_url: None,
        api_version: None,
        credential_link,
        migrated_from_legacy: migrated_at.is_some(),
        migrated_at: migrated_at.map(str::to_owned),
        updated_at: updated_at.to_owned(),
    }
}

pub(crate) fn build_anthropic_default_profile(
    model_id: &str,
    credential_link: Option<ProviderProfileCredentialLink>,
    migrated_at: Option<&str>,
    updated_at: &str,
) -> ProviderProfileRecord {
    ProviderProfileRecord {
        profile_id: ANTHROPIC_DEFAULT_PROFILE_ID.into(),
        provider_id: ANTHROPIC_PROVIDER_ID.into(),
        runtime_kind: ANTHROPIC_PROVIDER_ID.into(),
        label: ANTHROPIC_DEFAULT_PROFILE_LABEL.into(),
        model_id: model_id.trim().to_owned(),
        preset_id: Some(ANTHROPIC_PROVIDER_ID.into()),
        base_url: None,
        api_version: None,
        credential_link,
        migrated_from_legacy: migrated_at.is_some(),
        migrated_at: migrated_at.map(str::to_owned),
        updated_at: updated_at.to_owned(),
    }
}

fn validate_provider_profiles_metadata(
    metadata: ProviderProfilesMetadataFile,
    path: &Path,
) -> CommandResult<ProviderProfilesMetadataFile> {
    if metadata.version != 1 && metadata.version != PROVIDER_PROFILES_SCHEMA_VERSION {
        return Err(CommandError::user_fixable(
            "provider_profiles_invalid",
            format!(
                "Cadence rejected the app-local provider-profile metadata file at {} because schema version {} is unsupported.",
                path.display(),
                metadata.version
            ),
        ));
    }

    let active_profile_id = metadata.active_profile_id.trim();
    if active_profile_id.is_empty() {
        return Err(CommandError::user_fixable(
            "provider_profiles_invalid",
            format!(
                "Cadence rejected the app-local provider-profile metadata file at {} because activeProfileId was blank.",
                path.display()
            ),
        ));
    }

    if metadata.profiles.is_empty() {
        return Err(CommandError::user_fixable(
            "provider_profiles_invalid",
            format!(
                "Cadence rejected the app-local provider-profile metadata file at {} because it did not contain any profiles.",
                path.display()
            ),
        ));
    }

    let mut seen_profile_ids = BTreeSet::new();
    let mut validated_profiles = Vec::with_capacity(metadata.profiles.len());
    for profile in metadata.profiles {
        let validated = validate_provider_profile_record(profile, path, metadata.version)?;
        if !seen_profile_ids.insert(validated.profile_id.clone()) {
            return Err(CommandError::user_fixable(
                "provider_profiles_invalid",
                format!(
                    "Cadence rejected the app-local provider-profile metadata file at {} because profileId `{}` was duplicated.",
                    path.display(),
                    validated.profile_id
                ),
            ));
        }
        validated_profiles.push(validated);
    }

    if !validated_profiles
        .iter()
        .any(|profile| profile.profile_id == active_profile_id)
    {
        return Err(CommandError::user_fixable(
            "provider_profiles_invalid",
            format!(
                "Cadence rejected the app-local provider-profile metadata file at {} because activeProfileId `{}` did not match any stored profile.",
                path.display(),
                active_profile_id
            ),
        ));
    }

    Ok(ProviderProfilesMetadataFile {
        version: PROVIDER_PROFILES_SCHEMA_VERSION,
        active_profile_id: active_profile_id.to_owned(),
        profiles: validated_profiles,
        updated_at: normalize_updated_at(metadata.updated_at),
        migration: metadata.migration.map(validate_migration_state),
    })
}

fn validate_provider_profile_record(
    profile: ProviderProfileRecord,
    path: &Path,
    schema_version: u32,
) -> CommandResult<ProviderProfileRecord> {
    let profile_id = profile.profile_id.trim();
    if profile_id.is_empty() {
        return Err(CommandError::user_fixable(
            "provider_profiles_invalid",
            format!(
                "Cadence rejected the app-local provider-profile metadata file at {} because profileId was blank.",
                path.display()
            ),
        ));
    }

    let provider_id = profile.provider_id.trim();
    if provider_id.is_empty() {
        return Err(CommandError::user_fixable(
            "provider_profiles_invalid",
            format!(
                "Cadence rejected provider profile `{}` in {} because providerId was blank.",
                profile_id,
                path.display()
            ),
        ));
    }

    let runtime_kind = profile.runtime_kind.trim();
    let runtime_kind = if runtime_kind.is_empty() {
        if schema_version == 1 {
            provider_id
        } else {
            return Err(CommandError::user_fixable(
                "provider_profiles_invalid",
                format!(
                    "Cadence rejected provider profile `{}` in {} because runtimeKind was blank.",
                    profile_id,
                    path.display()
                ),
            ));
        }
    } else {
        runtime_kind
    };

    let provider = resolve_runtime_provider_identity(Some(provider_id), Some(runtime_kind))
        .map_err(|diagnostic| {
            CommandError::user_fixable("provider_profiles_invalid", diagnostic.message)
        })?;

    let label = profile.label.trim();
    if label.is_empty() {
        return Err(CommandError::user_fixable(
            "provider_profiles_invalid",
            format!(
                "Cadence rejected provider profile `{}` in {} because label was blank.",
                profile_id,
                path.display()
            ),
        ));
    }

    let model_id = profile.model_id.trim();
    if model_id.is_empty() {
        return Err(CommandError::user_fixable(
            "provider_profiles_invalid",
            format!(
                "Cadence rejected provider profile `{}` in {} because modelId was blank.",
                profile_id,
                path.display()
            ),
        ));
    }

    if provider.provider_id == OPENAI_CODEX_PROVIDER_ID && model_id != OPENAI_CODEX_PROVIDER_ID {
        return Err(CommandError::user_fixable(
            "provider_profiles_invalid",
            format!(
                "Cadence rejected provider profile `{}` in {} because OpenAI Codex profiles must use modelId `{}`.",
                profile_id,
                path.display(),
                OPENAI_CODEX_PROVIDER_ID
            ),
        ));
    }

    let preset_id = normalize_optional_text(profile.preset_id);
    let base_url = normalize_optional_text(profile.base_url);
    let api_version = normalize_optional_text(profile.api_version);
    let (preset_id, base_url, api_version) = validate_cloud_profile_metadata(
        provider.provider_id,
        provider.runtime_kind,
        profile_id,
        path,
        schema_version,
        preset_id,
        base_url,
        api_version,
    )?;

    let credential_link = match profile.credential_link {
        Some(ProviderProfileCredentialLink::OpenAiCodex {
            account_id,
            session_id,
            updated_at,
        }) => {
            let account_id = account_id.trim();
            if account_id.is_empty() {
                return Err(CommandError::user_fixable(
                    "provider_profiles_invalid",
                    format!(
                        "Cadence rejected provider profile `{}` in {} because the migrated OpenAI accountId was blank.",
                        profile_id,
                        path.display()
                    ),
                ));
            }

            let session_id = session_id.trim();
            if session_id.is_empty() {
                return Err(CommandError::user_fixable(
                    "provider_profiles_invalid",
                    format!(
                        "Cadence rejected provider profile `{}` in {} because the migrated OpenAI sessionId was blank.",
                        profile_id,
                        path.display()
                    ),
                ));
            }

            Some(ProviderProfileCredentialLink::OpenAiCodex {
                account_id: account_id.to_owned(),
                session_id: session_id.to_owned(),
                updated_at: normalize_updated_at(updated_at),
            })
        }
        Some(ProviderProfileCredentialLink::ApiKey { updated_at }) => {
            Some(ProviderProfileCredentialLink::ApiKey {
                updated_at: normalize_updated_at(updated_at),
            })
        }
        None => None,
    };

    Ok(ProviderProfileRecord {
        profile_id: profile_id.to_owned(),
        provider_id: provider.provider_id.to_owned(),
        runtime_kind: provider.runtime_kind.to_owned(),
        label: label.to_owned(),
        model_id: model_id.to_owned(),
        preset_id,
        base_url,
        api_version,
        credential_link,
        migrated_from_legacy: profile.migrated_from_legacy,
        migrated_at: profile.migrated_at.map(normalize_updated_at),
        updated_at: normalize_updated_at(profile.updated_at),
    })
}

fn validate_cloud_profile_metadata(
    provider_id: &str,
    runtime_kind: &str,
    profile_id: &str,
    path: &Path,
    schema_version: u32,
    preset_id: Option<String>,
    base_url: Option<String>,
    api_version: Option<String>,
) -> CommandResult<(Option<String>, Option<String>, Option<String>)> {
    let mut preset_id = preset_id;
    let base_url = base_url;
    let api_version = api_version;

    if schema_version == 1 && provider_id != OPENAI_CODEX_PROVIDER_ID && preset_id.is_none() {
        preset_id = Some(provider_id.to_owned());
    }

    match provider_id {
        OPENAI_CODEX_PROVIDER_ID => {
            ensure_absent_metadata_field(profile_id, path, "presetId", preset_id.as_deref())?;
            ensure_absent_metadata_field(profile_id, path, "baseUrl", base_url.as_deref())?;
            ensure_absent_metadata_field(profile_id, path, "apiVersion", api_version.as_deref())?;
        }
        OPENROUTER_PROVIDER_ID | ANTHROPIC_PROVIDER_ID | GITHUB_MODELS_PROVIDER_ID
        | GEMINI_AI_STUDIO_PROVIDER_ID => {
            require_exact_preset_id(profile_id, path, provider_id, preset_id.as_deref())?;
            ensure_absent_metadata_field(profile_id, path, "baseUrl", base_url.as_deref())?;
            ensure_absent_metadata_field(profile_id, path, "apiVersion", api_version.as_deref())?;
        }
        OPENAI_API_PROVIDER_ID => {
            if base_url.is_none() {
                require_exact_preset_id(profile_id, path, OPENAI_API_PROVIDER_ID, preset_id.as_deref())?;
            } else if let Some(preset_id) = preset_id.as_deref() {
                if preset_id != OPENAI_API_PROVIDER_ID {
                    return Err(CommandError::user_fixable(
                        "provider_profiles_invalid",
                        format!(
                            "Cadence rejected provider profile `{}` in {} because custom OpenAI-compatible profiles only accept presetId `{}` when presetId is provided.",
                            profile_id,
                            path.display(),
                            OPENAI_API_PROVIDER_ID
                        ),
                    ));
                }
            }

            if let Some(base_url) = base_url.as_deref() {
                validate_base_url(profile_id, path, base_url)?;
            }
            if base_url.is_none() && api_version.is_some() {
                return Err(CommandError::user_fixable(
                    "provider_profiles_invalid",
                    format!(
                        "Cadence rejected provider profile `{}` in {} because apiVersion requires a custom baseUrl for provider `{}`.",
                        profile_id,
                        path.display(),
                        OPENAI_API_PROVIDER_ID
                    ),
                ));
            }
        }
        AZURE_OPENAI_PROVIDER_ID => {
            if runtime_kind != OPENAI_COMPATIBLE_RUNTIME_KIND {
                return Err(CommandError::user_fixable(
                    "provider_profiles_invalid",
                    format!(
                        "Cadence rejected provider profile `{}` in {} because Azure OpenAI profiles must use runtimeKind `{}`.",
                        profile_id,
                        path.display(),
                        OPENAI_COMPATIBLE_RUNTIME_KIND
                    ),
                ));
            }
            require_exact_preset_id(profile_id, path, AZURE_OPENAI_PROVIDER_ID, preset_id.as_deref())?;
            let base_url = base_url.as_deref().ok_or_else(|| {
                CommandError::user_fixable(
                    "provider_profiles_invalid",
                    format!(
                        "Cadence rejected provider profile `{}` in {} because Azure OpenAI profiles require baseUrl.",
                        profile_id,
                        path.display()
                    ),
                )
            })?;
            validate_base_url(profile_id, path, base_url)?;
            if api_version.is_none() {
                return Err(CommandError::user_fixable(
                    "provider_profiles_invalid",
                    format!(
                        "Cadence rejected provider profile `{}` in {} because Azure OpenAI profiles require apiVersion.",
                        profile_id,
                        path.display()
                    ),
                ));
            }
        }
        other => {
            return Err(CommandError::user_fixable(
                "provider_profiles_invalid",
                format!(
                    "Cadence rejected provider profile `{}` in {} because provider `{other}` is not supported by the provider-profile store.",
                    profile_id,
                    path.display()
                ),
            ));
        }
    }

    if runtime_kind == GEMINI_RUNTIME_KIND && provider_id != GEMINI_AI_STUDIO_PROVIDER_ID {
        return Err(CommandError::user_fixable(
            "provider_profiles_invalid",
            format!(
                "Cadence rejected provider profile `{}` in {} because runtimeKind `{}` currently requires providerId `{}`.",
                profile_id,
                path.display(),
                GEMINI_RUNTIME_KIND,
                GEMINI_AI_STUDIO_PROVIDER_ID
            ),
        ));
    }

    Ok((preset_id, base_url, api_version))
}

fn require_exact_preset_id(
    profile_id: &str,
    path: &Path,
    expected: &str,
    actual: Option<&str>,
) -> CommandResult<()> {
    if actual == Some(expected) {
        return Ok(());
    }

    Err(CommandError::user_fixable(
        "provider_profiles_invalid",
        format!(
            "Cadence rejected provider profile `{}` in {} because presetId `{}` is required.",
            profile_id,
            path.display(),
            expected
        ),
    ))
}

fn ensure_absent_metadata_field(
    profile_id: &str,
    path: &Path,
    field: &str,
    value: Option<&str>,
) -> CommandResult<()> {
    if value.is_none() {
        return Ok(());
    }

    Err(CommandError::user_fixable(
        "provider_profiles_invalid",
        format!(
            "Cadence rejected provider profile `{}` in {} because field `{}` is not allowed for that provider.",
            profile_id,
            path.display(),
            field
        ),
    ))
}

fn validate_base_url(profile_id: &str, path: &Path, base_url: &str) -> CommandResult<()> {
    let parsed = Url::parse(base_url).map_err(|error| {
        CommandError::user_fixable(
            "provider_profiles_invalid",
            format!(
                "Cadence rejected provider profile `{}` in {} because baseUrl `{}` was not a valid absolute URL: {}",
                profile_id,
                path.display(),
                base_url,
                error
            ),
        )
    })?;

    match parsed.scheme() {
        "http" | "https" => Ok(()),
        other => Err(CommandError::user_fixable(
            "provider_profiles_invalid",
            format!(
                "Cadence rejected provider profile `{}` in {} because baseUrl `{}` used unsupported scheme `{}`.",
                profile_id,
                path.display(),
                base_url,
                other
            ),
        )),
    }
}

fn validate_migration_state(
    migration: ProviderProfilesMigrationState,
) -> ProviderProfilesMigrationState {
    ProviderProfilesMigrationState {
        source: if migration.source.trim().is_empty() {
            "legacy_runtime_settings_v1".into()
        } else {
            migration.source.trim().to_owned()
        },
        migrated_at: normalize_updated_at(migration.migrated_at),
        runtime_settings_updated_at: migration
            .runtime_settings_updated_at
            .map(normalize_updated_at),
        openrouter_credentials_updated_at: migration
            .openrouter_credentials_updated_at
            .map(normalize_updated_at),
        openai_auth_updated_at: migration.openai_auth_updated_at.map(normalize_updated_at),
        openrouter_model_inferred: migration.openrouter_model_inferred,
    }
}

fn validate_provider_profile_credentials(
    credentials: ProviderProfileCredentialsFile,
    path: &Path,
) -> CommandResult<ProviderProfileCredentialsFile> {
    Ok(ProviderProfileCredentialsFile {
        api_keys: validate_provider_api_key_credentials(credentials.api_keys, path)?,
    })
}

fn validate_provider_api_key_credentials(
    entries: Vec<ProviderApiKeyCredentialEntry>,
    path: &Path,
) -> CommandResult<Vec<ProviderApiKeyCredentialEntry>> {
    let mut seen_profile_ids = BTreeSet::new();
    let mut validated = Vec::with_capacity(entries.len());

    for entry in entries {
        let profile_id = entry.profile_id.trim();
        if profile_id.is_empty() {
            return Err(CommandError::user_fixable(
                "provider_profile_credentials_invalid",
                format!(
                    "Cadence rejected the app-local provider-profile credential file at {} because profileId was blank.",
                    path.display()
                ),
            ));
        }

        if !seen_profile_ids.insert(profile_id.to_owned()) {
            return Err(CommandError::user_fixable(
                "provider_profile_credentials_invalid",
                format!(
                    "Cadence rejected the app-local provider-profile credential file at {} because profileId `{}` was duplicated.",
                    path.display(),
                    profile_id
                ),
            ));
        }

        let api_key = entry.api_key.trim();
        if api_key.is_empty() {
            return Err(CommandError::user_fixable(
                "provider_profile_credentials_invalid",
                format!(
                    "Cadence rejected the app-local provider-profile credential file at {} because the apiKey for profile `{}` was blank.",
                    path.display(),
                    profile_id
                ),
            ));
        }

        validated.push(ProviderApiKeyCredentialEntry {
            profile_id: profile_id.to_owned(),
            api_key: api_key.to_owned(),
            updated_at: normalize_updated_at(entry.updated_at),
        });
    }

    Ok(validated)
}

fn api_key_credentials_by_profile(
    credentials: &ProviderProfileCredentialsFile,
) -> BTreeMap<&str, &ProviderApiKeyCredentialEntry> {
    let mut entries = BTreeMap::new();
    for entry in &credentials.api_keys {
        entries.insert(entry.profile_id.as_str(), entry);
    }
    entries
}

fn ensure_no_api_key_secret<'a>(
    profile_id: &str,
    credentials: &BTreeMap<&'a str, &'a ProviderApiKeyCredentialEntry>,
    credentials_path: &Path,
) -> CommandResult<()> {
    if credentials.contains_key(profile_id) {
        return Err(CommandError::user_fixable(
            "provider_profiles_contract_failed",
            format!(
                "Cadence found an unexpected apiKey secret entry for profile `{profile_id}` in {}.",
                credentials_path.display()
            ),
        ));
    }

    Ok(())
}

fn ensure_all_credentials_reference_profiles<'a>(
    metadata: &ProviderProfilesMetadataFile,
    credentials: &BTreeMap<&'a str, &'a ProviderApiKeyCredentialEntry>,
    credentials_path: &Path,
) -> CommandResult<()> {
    for profile_id in credentials.keys() {
        if !metadata
            .profiles
            .iter()
            .any(|profile| profile.profile_id == *profile_id)
        {
            return Err(CommandError::user_fixable(
                "provider_profiles_contract_failed",
                format!(
                    "Cadence found an unexpected apiKey secret entry for `{profile_id}` in {} without matching provider-profile metadata.",
                    credentials_path.display()
                ),
            ));
        }
    }

    Ok(())
}

fn normalize_snapshot_for_persist(
    snapshot: ProviderProfilesSnapshot,
) -> CommandResult<ProviderProfilesSnapshot> {
    let mut validated = validate_provider_profiles_contract(
        snapshot.metadata,
        snapshot.credentials,
        Path::new(PROVIDER_PROFILES_FILE_NAME),
        Path::new(PROVIDER_PROFILE_CREDENTIAL_STORE_FILE_NAME),
    )?;

    validated
        .metadata
        .profiles
        .sort_by(|left, right| left.profile_id.cmp(&right.profile_id));
    validated
        .credentials
        .api_keys
        .sort_by(|left, right| left.profile_id.cmp(&right.profile_id));

    Ok(validated)
}

fn rollback_message(result: CommandResult<()>) -> String {
    match result {
        Ok(()) => "ok".into(),
        Err(error) => format!("{}: {}", error.code, error.message),
    }
}

fn serialize_pretty_json<T: Serialize>(value: &T, operation: &str) -> CommandResult<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|error| {
        CommandError::system_fault(
            format!("{operation}_serialize_failed"),
            format!(
                "Cadence could not serialize the app-local provider-profile update for {operation}: {error}"
            ),
        )
    })
}

fn normalize_updated_at(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        crate::auth::now_timestamp()
    } else {
        trimmed.to_owned()
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

const fn provider_profiles_schema_version() -> u32 {
    PROVIDER_PROFILES_SCHEMA_VERSION
}
