use std::{fs, io::Write, path::Path};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{
    commands::{CommandError, CommandResult},
    provider_credentials::{ProviderCredentialProfile, ProviderCredentialsView},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RuntimeSettingsFile {
    pub provider_id: String,
    pub model_id: String,
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
                "Xero could not determine the parent directory for {}.",
                path.display()
            ),
        )
    })?;

    fs::create_dir_all(parent).map_err(|error| {
        CommandError::retryable(
            format!("{operation}_directory_unavailable"),
            format!(
                "Xero could not prepare the app-local settings directory at {}: {error}",
                parent.display()
            ),
        )
    })?;

    let mut temp_file = NamedTempFile::new_in(parent).map_err(|error| {
        CommandError::retryable(
            format!("{operation}_tempfile_failed"),
            format!("Xero could not stage the app-local settings update: {error}"),
        )
    })?;

    temp_file.write_all(json).map_err(|error| {
        CommandError::retryable(
            format!("{operation}_write_failed"),
            format!("Xero could not write the staged app-local settings update: {error}"),
        )
    })?;
    temp_file.flush().map_err(|error| {
        CommandError::retryable(
            format!("{operation}_write_failed"),
            format!("Xero could not flush the staged app-local settings update: {error}"),
        )
    })?;

    temp_file.persist(path).map_err(|error| {
        CommandError::retryable(
            format!("{operation}_write_failed"),
            format!(
                "Xero could not atomically persist the app-local settings file at {}: {}",
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
    let active_api_key_credential =
        provider_profiles.matched_api_key_credential_for_profile(&profile.profile_id);

    Ok(RuntimeSettingsSnapshot {
        settings: RuntimeSettingsFile {
            provider_id: profile.provider_id.clone(),
            model_id: profile.model_id.clone(),
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
    })
}
