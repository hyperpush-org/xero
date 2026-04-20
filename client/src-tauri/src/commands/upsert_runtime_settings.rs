use std::fs;

use serde::Serialize;
use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{CommandResult, RuntimeSettingsDto, UpsertRuntimeSettingsRequestDto},
    state::DesktopState,
};

use super::get_runtime_settings::{
    read_openrouter_credentials_file, read_runtime_settings_file, remove_file_if_exists,
    runtime_settings_file_from_request, validate_runtime_settings_file, write_json_file_atomically,
    OpenRouterCredentialFile, RuntimeSettingsFile,
};

#[tauri::command]
pub fn upsert_runtime_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertRuntimeSettingsRequestDto,
) -> CommandResult<RuntimeSettingsDto> {
    let settings_path = state.runtime_settings_file(&app)?;
    let credentials_path = state.openrouter_credential_file(&app)?;

    let current_settings = read_runtime_settings_file(&settings_path)?;
    let current_credentials = read_openrouter_credentials_file(&credentials_path)?;

    if request.openrouter_api_key.is_none()
        && current_settings
            .as_ref()
            .map(|settings| settings.openrouter_api_key_configured)
            != Some(current_credentials.is_some())
        && current_settings.is_some()
    {
        return Err(crate::commands::CommandError::user_fixable(
            "runtime_settings_contract_failed",
            format!(
                "Cadence found mismatched runtime settings at {} and OpenRouter credentials at {}. Save an explicit OpenRouter API key value to repair the app-local settings state.",
                settings_path.display(),
                credentials_path.display()
            ),
        ));
    }

    let requested_key = request
        .openrouter_api_key
        .as_ref()
        .map(|value| value.trim().to_owned());
    let next_credentials = match requested_key.as_deref() {
        Some("") => None,
        Some(value) => Some(OpenRouterCredentialFile {
            api_key: value.to_owned(),
            updated_at: credential_updated_at(&current_credentials, Some(value)),
        }),
        None => current_credentials.clone(),
    };

    let mut next_settings = runtime_settings_file_from_request(
        &request.provider_id,
        &request.model_id,
        next_credentials.is_some(),
    )?;
    next_settings.updated_at = settings_updated_at(
        &current_settings,
        &next_settings,
    );
    let next_settings = validate_runtime_settings_file(next_settings, "runtime_settings_request_invalid")?;

    persist_runtime_settings_update(
        &settings_path,
        &credentials_path,
        current_settings.as_ref(),
        current_credentials.as_ref(),
        &next_settings,
        next_credentials.as_ref(),
    )?;

    Ok(RuntimeSettingsDto {
        provider_id: next_settings.provider_id,
        model_id: next_settings.model_id,
        openrouter_api_key_configured: next_settings.openrouter_api_key_configured,
    })
}

fn persist_runtime_settings_update(
    settings_path: &std::path::Path,
    credentials_path: &std::path::Path,
    current_settings: Option<&RuntimeSettingsFile>,
    current_credentials: Option<&OpenRouterCredentialFile>,
    next_settings: &RuntimeSettingsFile,
    next_credentials: Option<&OpenRouterCredentialFile>,
) -> CommandResult<()> {
    let previous_settings_bytes = read_existing_bytes(settings_path)?;
    let settings_json = serialize_pretty_json(next_settings, "runtime_settings")?;
    write_json_file_atomically(settings_path, &settings_json, "runtime_settings")?;

    if let Err(error) = persist_openrouter_credential_state(
        credentials_path,
        current_credentials,
        next_credentials,
    ) {
        return match rollback_settings_file(settings_path, previous_settings_bytes) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(crate::commands::CommandError::retryable(
                "runtime_settings_rollback_failed",
                format!(
                    "Cadence failed to persist the paired OpenRouter credential update after writing runtime settings, and then could not restore the prior settings file at {}: {}. Original credential error: {}",
                    settings_path.display(),
                    rollback_error.message,
                    error.message
                ),
            )),
        };
    }

    // A successful credential write leaves both files in the same redacted state contract.
    let _ = current_settings;
    Ok(())
}

fn persist_openrouter_credential_state(
    path: &std::path::Path,
    current: Option<&OpenRouterCredentialFile>,
    next: Option<&OpenRouterCredentialFile>,
) -> CommandResult<()> {
    if current == next {
        return Ok(());
    }

    match next {
        Some(credentials) => {
            let json = serialize_pretty_json(credentials, "openrouter_credentials")?;
            write_json_file_atomically(path, &json, "openrouter_credentials")
        }
        None => remove_file_if_exists(path, "openrouter_credentials"),
    }
}

fn rollback_settings_file(
    path: &std::path::Path,
    previous_bytes: Option<Vec<u8>>,
) -> CommandResult<()> {
    match previous_bytes {
        Some(bytes) => write_json_file_atomically(path, &bytes, "runtime_settings_rollback"),
        None => remove_file_if_exists(path, "runtime_settings_rollback"),
    }
}

fn read_existing_bytes(path: &std::path::Path) -> CommandResult<Option<Vec<u8>>> {
    if !path.exists() {
        return Ok(None);
    }

    fs::read(path).map(Some).map_err(|error| {
        crate::commands::CommandError::retryable(
            "runtime_settings_read_failed",
            format!(
                "Cadence could not snapshot the previous runtime settings file at {} before updating it: {error}",
                path.display()
            ),
        )
    })
}

fn serialize_pretty_json<T: Serialize>(value: &T, operation: &str) -> CommandResult<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|error| {
        crate::commands::CommandError::system_fault(
            format!("{operation}_serialize_failed"),
            format!(
                "Cadence could not serialize the app-local settings update for {operation}: {error}"
            ),
        )
    })
}

fn settings_updated_at(
    current: &Option<RuntimeSettingsFile>,
    next: &RuntimeSettingsFile,
) -> String {
    match current {
        Some(current)
            if current.provider_id == next.provider_id
                && current.model_id == next.model_id
                && current.openrouter_api_key_configured == next.openrouter_api_key_configured =>
        {
            current.updated_at.clone()
        }
        _ => crate::auth::now_timestamp(),
    }
}

fn credential_updated_at(
    current: &Option<OpenRouterCredentialFile>,
    next_api_key: Option<&str>,
) -> String {
    match (current, next_api_key) {
        (Some(current), Some(next_api_key)) if current.api_key == next_api_key => {
            current.updated_at.clone()
        }
        _ => crate::auth::now_timestamp(),
    }
}
