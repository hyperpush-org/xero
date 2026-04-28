use std::{fs, io::Write, path::Path};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{
    commands::{CommandError, CommandResult},
    provider_credentials::{ProviderCredentialProfile, ProviderCredentialsView},
    runtime::{
        normalize_openai_codex_model_id, resolve_runtime_provider_identity, ANTHROPIC_PROVIDER_ID,
        OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RuntimeSettingsFile {
    pub provider_id: String,
    pub model_id: String,
    pub openrouter_api_key_configured: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeSettingsSnapshot {
    pub settings: RuntimeSettingsFile,
    pub runtime_kind: String,
    pub provider_api_key: Option<String>,
    pub provider_api_key_updated_at: Option<String>,
    pub preset_id: Option<String>,
    pub base_url: Option<String>,
    pub api_version: Option<String>,
    pub region: Option<String>,
    pub project_id: Option<String>,
    pub openrouter_api_key: Option<String>,
    pub openrouter_credentials_updated_at: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub anthropic_credentials_updated_at: Option<String>,
}

pub(crate) fn runtime_settings_file_from_request(
    provider_id: &str,
    model_id: &str,
    openrouter_api_key_configured: bool,
) -> CommandResult<RuntimeSettingsFile> {
    let provider_id = provider_id.trim();
    if provider_id.is_empty() {
        return Err(CommandError::invalid_request("providerId"));
    }

    let model_id = model_id.trim();
    if model_id.is_empty() {
        return Err(CommandError::invalid_request("modelId"));
    }

    let provider = resolve_runtime_provider_identity(Some(provider_id), Some(provider_id))
        .map_err(|diagnostic| {
            CommandError::user_fixable("runtime_settings_request_invalid", diagnostic.message)
        })?;

    if !supports_runtime_settings_compatibility_provider(provider.provider_id) {
        return Err(CommandError::user_fixable(
            "runtime_settings_request_invalid",
            format!(
                "Cadence only accepts runtime-settings compatibility writes for `openai_codex`, `openrouter`, or `anthropic`. Use provider profiles for provider `{}`.",
                provider.provider_id
            ),
        ));
    }

    let model_id = if provider.provider_id == OPENAI_CODEX_PROVIDER_ID {
        normalize_openai_codex_model_id(model_id)
    } else {
        model_id.to_owned()
    };

    Ok(RuntimeSettingsFile {
        provider_id: provider.provider_id.to_owned(),
        model_id,
        openrouter_api_key_configured,
        updated_at: crate::auth::now_timestamp(),
    })
}

pub(crate) fn write_json_file_atomically(
    path: &Path,
    json: &[u8],
    operation: &str,
) -> CommandResult<()> {
    let parent = path.parent().ok_or_else(|| {
        CommandError::system_fault(
            format!("{operation}_parent_missing"),
            format!(
                "Cadence could not determine the parent directory for {}.",
                path.display()
            ),
        )
    })?;

    fs::create_dir_all(parent).map_err(|error| {
        CommandError::retryable(
            format!("{operation}_directory_unavailable"),
            format!(
                "Cadence could not prepare the app-local settings directory at {}: {error}",
                parent.display()
            ),
        )
    })?;

    let mut temp_file = NamedTempFile::new_in(parent).map_err(|error| {
        CommandError::retryable(
            format!("{operation}_tempfile_failed"),
            format!("Cadence could not stage the app-local settings update: {error}"),
        )
    })?;

    temp_file.write_all(json).map_err(|error| {
        CommandError::retryable(
            format!("{operation}_write_failed"),
            format!("Cadence could not write the staged app-local settings update: {error}"),
        )
    })?;
    temp_file.flush().map_err(|error| {
        CommandError::retryable(
            format!("{operation}_write_failed"),
            format!("Cadence could not flush the staged app-local settings update: {error}"),
        )
    })?;

    temp_file.persist(path).map_err(|error| {
        CommandError::retryable(
            format!("{operation}_write_failed"),
            format!(
                "Cadence could not atomically persist the app-local settings file at {}: {}",
                path.display(),
                error.error
            ),
        )
    })?;

    Ok(())
}

pub(crate) fn runtime_settings_snapshot_for_provider_profile(
    provider_profiles: &ProviderCredentialsView,
    profile: &ProviderCredentialProfile,
) -> CommandResult<RuntimeSettingsSnapshot> {
    let preferred_openrouter_credential = provider_profiles.preferred_openrouter_credential();
    let preferred_anthropic_credential = provider_profiles.preferred_anthropic_credential();
    let active_api_key_credential =
        provider_profiles.matched_api_key_credential_for_profile(&profile.profile_id);

    Ok(RuntimeSettingsSnapshot {
        settings: RuntimeSettingsFile {
            provider_id: profile.provider_id.clone(),
            model_id: profile.model_id.clone(),
            openrouter_api_key_configured: provider_profiles.any_openrouter_api_key_configured(),
            updated_at: profile.updated_at.clone(),
        },
        runtime_kind: profile.runtime_kind.clone(),
        provider_api_key: active_api_key_credential.map(|entry| entry.api_key.clone()),
        provider_api_key_updated_at: active_api_key_credential
            .map(|entry| entry.updated_at.clone()),
        preset_id: profile.preset_id.clone(),
        base_url: profile.base_url.clone(),
        api_version: profile.api_version.clone(),
        region: profile.region.clone(),
        project_id: profile.project_id.clone(),
        openrouter_api_key: preferred_openrouter_credential.map(|entry| entry.api_key.clone()),
        openrouter_credentials_updated_at: preferred_openrouter_credential
            .map(|entry| entry.updated_at.clone()),
        anthropic_api_key: preferred_anthropic_credential.map(|entry| entry.api_key.clone()),
        anthropic_credentials_updated_at: preferred_anthropic_credential
            .map(|entry| entry.updated_at.clone()),
    })
}

fn supports_runtime_settings_compatibility_provider(provider_id: &str) -> bool {
    provider_id == OPENAI_CODEX_PROVIDER_ID
        || provider_id == OPENROUTER_PROVIDER_ID
        || provider_id == ANTHROPIC_PROVIDER_ID
}
