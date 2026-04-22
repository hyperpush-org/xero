use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{
    commands::{
        get_runtime_settings::{remove_file_if_exists, write_json_file_atomically},
        CommandError, CommandResult,
    },
    runtime::{
        resolve_runtime_provider_identity, OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID,
    },
};

pub const PROVIDER_PROFILES_FILE_NAME: &str = "provider-profiles.json";
pub const PROVIDER_PROFILE_CREDENTIAL_STORE_FILE_NAME: &str = "provider-profile-credentials.json";
pub const OPENAI_CODEX_DEFAULT_PROFILE_ID: &str = "openai_codex-default";
pub const OPENROUTER_DEFAULT_PROFILE_ID: &str = "openrouter-default";
pub const OPENROUTER_FALLBACK_MODEL_ID: &str = "openai/gpt-4.1-mini";
const PROVIDER_PROFILES_SCHEMA_VERSION: u32 = 1;
const OPENAI_CODEX_DEFAULT_PROFILE_LABEL: &str = "OpenAI Codex";
const OPENROUTER_DEFAULT_PROFILE_LABEL: &str = "OpenRouter";

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
    pub label: String,
    pub model_id: String,
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
    #[serde(rename = "openrouter")]
    OpenRouter { updated_at: String },
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfileCredentialsFile {
    #[serde(default)]
    pub openrouter_api_keys: Vec<OpenRouterProfileCredentialEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenRouterProfileCredentialEntry {
    pub profile_id: String,
    pub api_key: String,
    pub updated_at: String,
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
                && matches!(
                    profile.credential_link,
                    Some(ProviderProfileCredentialLink::OpenRouter { .. })
                )
        })
    }

    pub fn preferred_openrouter_credential(&self) -> Option<&OpenRouterProfileCredentialEntry> {
        self.active_profile()
            .filter(|profile| profile.provider_id == OPENROUTER_PROVIDER_ID)
            .and_then(|profile| self.openrouter_credential(&profile.profile_id))
            .or_else(|| self.credentials.openrouter_api_keys.first())
    }

    pub fn openrouter_credential(
        &self,
        profile_id: &str,
    ) -> Option<&OpenRouterProfileCredentialEntry> {
        self.credentials
            .openrouter_api_keys
            .iter()
            .find(|entry| entry.profile_id == profile_id)
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
            Some(ProviderProfileCredentialLink::OpenRouter { updated_at }) => {
                let has_secret = credentials
                    .openrouter_api_keys
                    .iter()
                    .any(|entry| entry.profile_id == self.profile_id);
                if has_secret {
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

    let parsed = serde_json::from_str::<ProviderProfileCredentialsFile>(&contents).map_err(|error| {
        CommandError::user_fixable(
            "provider_profile_credentials_decode_failed",
            format!(
                "Cadence could not decode the app-local provider-profile credential file at {}: {error}",
                path.display()
            ),
        )
    })?;

    Ok(Some(parsed))
}

pub(crate) fn validate_provider_profiles_contract(
    metadata: ProviderProfilesMetadataFile,
    credentials: ProviderProfileCredentialsFile,
    metadata_path: &Path,
    credentials_path: &Path,
) -> CommandResult<ProviderProfilesSnapshot> {
    let metadata = validate_provider_profiles_metadata(metadata, metadata_path)?;
    let credentials = validate_provider_profile_credentials(credentials, credentials_path)?;
    let credential_map = credentials_by_profile(&credentials);

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
                if credential_map.contains_key(profile.profile_id.as_str()) {
                    return Err(CommandError::user_fixable(
                        "provider_profiles_contract_failed",
                        format!(
                            "Cadence found an unexpected OpenRouter secret entry for non-OpenRouter profile `{}` in {}.",
                            profile.profile_id,
                            credentials_path.display()
                        ),
                    ));
                }
            }
            Some(ProviderProfileCredentialLink::OpenRouter { updated_at }) => {
                if profile.provider_id != OPENROUTER_PROVIDER_ID {
                    return Err(CommandError::user_fixable(
                        "provider_profiles_invalid",
                        format!(
                            "Cadence rejected provider profile `{}` because the OpenRouter credential linkage does not match provider `{}`.",
                            profile.profile_id, profile.provider_id
                        ),
                    ));
                }

                let Some(secret) = credential_map.get(profile.profile_id.as_str()) else {
                    return Err(CommandError::user_fixable(
                        "provider_profiles_contract_failed",
                        format!(
                            "Cadence found provider profile `{}` marked with an OpenRouter credential link in {} but no matching secret entry in {}.",
                            profile.profile_id,
                            metadata_path.display(),
                            credentials_path.display()
                        ),
                    ));
                };

                if secret.updated_at != *updated_at {
                    return Err(CommandError::user_fixable(
                        "provider_profiles_contract_failed",
                        format!(
                            "Cadence found mismatched redacted OpenRouter credential metadata for profile `{}` between {} and {}.",
                            profile.profile_id,
                            metadata_path.display(),
                            credentials_path.display()
                        ),
                    ));
                }
            }
            None => {
                if credential_map.contains_key(profile.profile_id.as_str()) {
                    return Err(CommandError::user_fixable(
                        "provider_profiles_contract_failed",
                        format!(
                            "Cadence found an unexpected provider-profile secret entry for `{}` in {} without matching redacted metadata in {}.",
                            profile.profile_id,
                            credentials_path.display(),
                            metadata_path.display()
                        ),
                    ));
                }
            }
        }
    }

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

    let credential_result = if next.credentials.openrouter_api_keys.is_empty() {
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
        label: OPENAI_CODEX_DEFAULT_PROFILE_LABEL.into(),
        model_id: OPENAI_CODEX_PROVIDER_ID.into(),
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
        label: OPENROUTER_DEFAULT_PROFILE_LABEL.into(),
        model_id: model_id.trim().to_owned(),
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
    if metadata.version != PROVIDER_PROFILES_SCHEMA_VERSION {
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
        let validated = validate_provider_profile_record(profile, path)?;
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
        version: metadata.version,
        active_profile_id: active_profile_id.to_owned(),
        profiles: validated_profiles,
        updated_at: normalize_updated_at(metadata.updated_at),
        migration: metadata.migration.map(validate_migration_state),
    })
}

fn validate_provider_profile_record(
    profile: ProviderProfileRecord,
    path: &Path,
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

    let provider = resolve_runtime_provider_identity(Some(provider_id), Some(provider_id))
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
        Some(ProviderProfileCredentialLink::OpenRouter { updated_at }) => {
            Some(ProviderProfileCredentialLink::OpenRouter {
                updated_at: normalize_updated_at(updated_at),
            })
        }
        None => None,
    };

    Ok(ProviderProfileRecord {
        profile_id: profile_id.to_owned(),
        provider_id: provider.provider_id.to_owned(),
        label: label.to_owned(),
        model_id: model_id.to_owned(),
        credential_link,
        migrated_from_legacy: profile.migrated_from_legacy,
        migrated_at: profile.migrated_at.map(normalize_updated_at),
        updated_at: normalize_updated_at(profile.updated_at),
    })
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
    let mut seen_profile_ids = BTreeSet::new();
    let mut validated = Vec::with_capacity(credentials.openrouter_api_keys.len());

    for entry in credentials.openrouter_api_keys {
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
                    "Cadence rejected the app-local provider-profile credential file at {} because the OpenRouter apiKey for profile `{}` was blank.",
                    path.display(),
                    profile_id
                ),
            ));
        }

        validated.push(OpenRouterProfileCredentialEntry {
            profile_id: profile_id.to_owned(),
            api_key: api_key.to_owned(),
            updated_at: normalize_updated_at(entry.updated_at),
        });
    }

    Ok(ProviderProfileCredentialsFile {
        openrouter_api_keys: validated,
    })
}

fn credentials_by_profile(
    credentials: &ProviderProfileCredentialsFile,
) -> BTreeMap<&str, &OpenRouterProfileCredentialEntry> {
    let mut entries = BTreeMap::new();
    for entry in &credentials.openrouter_api_keys {
        entries.insert(entry.profile_id.as_str(), entry);
    }
    entries
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
        .openrouter_api_keys
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

const fn provider_profiles_schema_version() -> u32 {
    PROVIDER_PROFILES_SCHEMA_VERSION
}
