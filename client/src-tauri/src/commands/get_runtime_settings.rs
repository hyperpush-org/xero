use std::{fs, io::Write, path::Path};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use tempfile::NamedTempFile;

use crate::{
    commands::{
        provider_profiles::load_provider_profiles_snapshot, CommandError, CommandResult,
        RuntimeSettingsDto,
    },
    provider_profiles::ProviderProfilesSnapshot,
    runtime::{
        resolve_runtime_provider_identity, OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID,
    },
    state::DesktopState,
};

const DEFAULT_RUNTIME_PROVIDER_ID: &str = OPENAI_CODEX_PROVIDER_ID;
const DEFAULT_RUNTIME_MODEL_ID: &str = OPENAI_CODEX_PROVIDER_ID;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RuntimeSettingsFile {
    pub provider_id: String,
    pub model_id: String,
    pub openrouter_api_key_configured: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OpenRouterCredentialFile {
    pub api_key: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeSettingsSnapshot {
    pub settings: RuntimeSettingsFile,
    pub openrouter_api_key: Option<String>,
    pub openrouter_credentials_updated_at: Option<String>,
}

impl RuntimeSettingsSnapshot {
    pub(crate) fn dto(&self) -> RuntimeSettingsDto {
        RuntimeSettingsDto {
            provider_id: self.settings.provider_id.clone(),
            model_id: self.settings.model_id.clone(),
            openrouter_api_key_configured: self.settings.openrouter_api_key_configured,
        }
    }
}

#[tauri::command]
pub fn get_runtime_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<RuntimeSettingsDto> {
    Ok(load_runtime_settings_snapshot(&app, state.inner())?.dto())
}

pub(crate) fn load_runtime_settings_snapshot<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<RuntimeSettingsSnapshot> {
    let provider_profiles = load_provider_profiles_snapshot(app, state)?;
    runtime_settings_snapshot_from_provider_profiles(&provider_profiles)
}

pub(crate) fn load_runtime_settings_snapshot_from_paths(
    settings_path: &Path,
    credentials_path: &Path,
) -> CommandResult<RuntimeSettingsSnapshot> {
    let settings = read_runtime_settings_file(settings_path)?;
    let credentials = read_openrouter_credentials_file(credentials_path)?;

    match (settings, credentials) {
        (None, None) => Ok(default_runtime_settings_snapshot()),
        (None, Some(_)) => Err(CommandError::user_fixable(
            "runtime_settings_contract_failed",
            format!(
                "Cadence found OpenRouter credentials at {} without the matching runtime settings file at {}.",
                credentials_path.display(),
                settings_path.display()
            ),
        )),
        (Some(settings), credentials) => {
            validate_runtime_settings_contract(settings_path, credentials_path, &settings, credentials)
        }
    }
}

pub(crate) fn read_runtime_settings_file(
    path: &Path,
) -> CommandResult<Option<RuntimeSettingsFile>> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path).map_err(|error| {
        CommandError::retryable(
            "runtime_settings_read_failed",
            format!(
                "Cadence could not read the app-local runtime settings file at {}: {error}",
                path.display()
            ),
        )
    })?;

    let parsed = serde_json::from_str::<RuntimeSettingsFile>(&contents).map_err(|error| {
        CommandError::user_fixable(
            "runtime_settings_decode_failed",
            format!(
                "Cadence could not decode the app-local runtime settings file at {}: {error}",
                path.display()
            ),
        )
    })?;

    Ok(Some(validate_runtime_settings_file(
        parsed,
        "runtime_settings_decode_failed",
    )?))
}

pub(crate) fn read_openrouter_credentials_file(
    path: &Path,
) -> CommandResult<Option<OpenRouterCredentialFile>> {
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(path).map_err(|error| {
        CommandError::retryable(
            "openrouter_credentials_read_failed",
            format!(
                "Cadence could not read the app-local OpenRouter credential file at {}: {error}",
                path.display()
            ),
        )
    })?;

    let parsed = serde_json::from_str::<OpenRouterCredentialFile>(&contents).map_err(|error| {
        CommandError::user_fixable(
            "openrouter_credentials_decode_failed",
            format!(
                "Cadence could not decode the app-local OpenRouter credential file at {}: {error}",
                path.display()
            ),
        )
    })?;

    let api_key = parsed.api_key.trim();
    if api_key.is_empty() {
        return Err(CommandError::user_fixable(
            "openrouter_credentials_invalid",
            format!(
                "Cadence rejected the app-local OpenRouter credential file at {} because apiKey was blank.",
                path.display()
            ),
        ));
    }

    Ok(Some(OpenRouterCredentialFile {
        api_key: api_key.to_owned(),
        updated_at: normalize_updated_at(parsed.updated_at),
    }))
}

pub(crate) fn validate_runtime_settings_file(
    file: RuntimeSettingsFile,
    error_code: &'static str,
) -> CommandResult<RuntimeSettingsFile> {
    let provider_id = file.provider_id.trim();
    if provider_id.is_empty() {
        return Err(CommandError::user_fixable(
            error_code,
            "Cadence rejected the app-local runtime settings because providerId was blank.",
        ));
    }

    let model_id = file.model_id.trim();
    if model_id.is_empty() {
        return Err(CommandError::user_fixable(
            error_code,
            "Cadence rejected the app-local runtime settings because modelId was blank.",
        ));
    }

    let provider = resolve_runtime_provider_identity(Some(provider_id), Some(provider_id))
        .map_err(|diagnostic| CommandError::user_fixable(error_code, diagnostic.message))?;

    if provider.provider_id == OPENAI_CODEX_PROVIDER_ID && model_id != OPENAI_CODEX_PROVIDER_ID {
        return Err(CommandError::user_fixable(
            error_code,
            format!(
                "Cadence only supports modelId `{OPENAI_CODEX_PROVIDER_ID}` for provider `{OPENAI_CODEX_PROVIDER_ID}`."
            ),
        ));
    }

    Ok(RuntimeSettingsFile {
        provider_id: provider.provider_id.to_owned(),
        model_id: model_id.to_owned(),
        openrouter_api_key_configured: file.openrouter_api_key_configured,
        updated_at: normalize_updated_at(file.updated_at),
    })
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

    if provider.provider_id == OPENAI_CODEX_PROVIDER_ID && model_id != OPENAI_CODEX_PROVIDER_ID {
        return Err(CommandError::user_fixable(
            "runtime_settings_request_invalid",
            format!(
                "Cadence only supports modelId `{OPENAI_CODEX_PROVIDER_ID}` for provider `{OPENAI_CODEX_PROVIDER_ID}`."
            ),
        ));
    }

    Ok(RuntimeSettingsFile {
        provider_id: provider.provider_id.to_owned(),
        model_id: model_id.to_owned(),
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

pub(crate) fn remove_file_if_exists(path: &Path, operation: &str) -> CommandResult<()> {
    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(path).map_err(|error| {
        CommandError::retryable(
            format!("{operation}_write_failed"),
            format!(
                "Cadence could not remove the app-local settings file at {}: {error}",
                path.display()
            ),
        )
    })
}

pub(crate) fn default_runtime_settings_snapshot() -> RuntimeSettingsSnapshot {
    RuntimeSettingsSnapshot {
        settings: RuntimeSettingsFile {
            provider_id: DEFAULT_RUNTIME_PROVIDER_ID.into(),
            model_id: DEFAULT_RUNTIME_MODEL_ID.into(),
            openrouter_api_key_configured: false,
            updated_at: crate::auth::now_timestamp(),
        },
        openrouter_api_key: None,
        openrouter_credentials_updated_at: None,
    }
}

fn validate_runtime_settings_contract(
    settings_path: &Path,
    credentials_path: &Path,
    settings: &RuntimeSettingsFile,
    credentials: Option<OpenRouterCredentialFile>,
) -> CommandResult<RuntimeSettingsSnapshot> {
    let key_present = credentials.is_some();
    if settings.openrouter_api_key_configured != key_present {
        return Err(CommandError::user_fixable(
            "runtime_settings_contract_failed",
            format!(
                "Cadence found mismatched runtime settings at {} and OpenRouter credentials at {}. The redacted key-configured flag no longer matches the credential file state.",
                settings_path.display(),
                credentials_path.display()
            ),
        ));
    }

    if settings.provider_id == OPENROUTER_PROVIDER_ID
        && settings.openrouter_api_key_configured
        && credentials.is_none()
    {
        return Err(CommandError::user_fixable(
            "runtime_settings_contract_failed",
            format!(
                "Cadence could not load the selected OpenRouter provider because the credential file at {} is missing.",
                credentials_path.display()
            ),
        ));
    }

    Ok(RuntimeSettingsSnapshot {
        settings: settings.clone(),
        openrouter_api_key: credentials
            .as_ref()
            .map(|credentials| credentials.api_key.clone()),
        openrouter_credentials_updated_at: credentials.map(|credentials| credentials.updated_at),
    })
}

pub(crate) fn runtime_settings_snapshot_from_provider_profiles(
    provider_profiles: &ProviderProfilesSnapshot,
) -> CommandResult<RuntimeSettingsSnapshot> {
    let active_profile = provider_profiles.active_profile().ok_or_else(|| {
        CommandError::user_fixable(
            "provider_profiles_invalid",
            "Cadence could not project runtime settings because the active provider profile was missing.",
        )
    })?;

    let preferred_openrouter_credential = provider_profiles.preferred_openrouter_credential();

    Ok(RuntimeSettingsSnapshot {
        settings: RuntimeSettingsFile {
            provider_id: active_profile.provider_id.clone(),
            model_id: active_profile.model_id.clone(),
            openrouter_api_key_configured: provider_profiles.any_openrouter_api_key_configured(),
            updated_at: active_profile.updated_at.clone(),
        },
        openrouter_api_key: preferred_openrouter_credential.map(|entry| entry.api_key.clone()),
        openrouter_credentials_updated_at: preferred_openrouter_credential
            .map(|entry| entry.updated_at.clone()),
    })
}

fn map_auth_store_error_to_command_error(error: crate::auth::AuthFlowError) -> CommandError {
    if error.retryable {
        CommandError::retryable(error.code, error.message)
    } else {
        CommandError::user_fixable(error.code, error.message)
    }
}

fn normalize_updated_at(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        crate::auth::now_timestamp()
    } else {
        trimmed.to_owned()
    }
}
