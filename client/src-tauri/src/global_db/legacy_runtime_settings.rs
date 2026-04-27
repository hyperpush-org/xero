//! Phase 6 cleanup: legacy JSON helpers used by the one-shot importer that promotes
//! pre-Phase-2 `runtime-settings.json` + `openrouter-credentials.json` payloads into the
//! global SQLite database. These helpers used to live in `commands::get_runtime_settings`
//! while the runtime still read JSON; now that production reads go through SQLite, the
//! legacy decode/validate code is owned by the importer module.
//!
//! Nothing here is reachable from the runtime hot path. Each function exits early when the
//! legacy file does not exist on disk.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    commands::{
        get_runtime_settings::{RuntimeSettingsFile, RuntimeSettingsSnapshot},
        CommandError, CommandResult,
    },
    runtime::{
        normalize_openai_codex_model_id, resolve_runtime_provider_identity, ANTHROPIC_PROVIDER_ID,
        OPENAI_CODEX_DEFAULT_MODEL_ID, OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID,
    },
};

const DEFAULT_RUNTIME_PROVIDER_ID: &str = OPENAI_CODEX_PROVIDER_ID;
const DEFAULT_RUNTIME_MODEL_ID: &str = OPENAI_CODEX_DEFAULT_MODEL_ID;

/// On-disk shape of the legacy `openrouter-credentials.json` payload. Kept private to the
/// importer module; production reads come from `provider_profile_credentials` in the
/// global database.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LegacyOpenRouterCredentialFile {
    pub api_key: String,
    pub updated_at: String,
}

/// Decode a legacy `runtime-settings.json` + `openrouter-credentials.json` pair into the
/// in-memory `RuntimeSettingsSnapshot` that the provider-profile importer feeds into its
/// migration builder. Returns the default snapshot when neither file exists.
pub(crate) fn load_legacy_runtime_settings_snapshot_from_paths(
    settings_path: &Path,
    credentials_path: &Path,
) -> CommandResult<RuntimeSettingsSnapshot> {
    let settings = read_legacy_runtime_settings_file(settings_path)?;
    let credentials = read_legacy_openrouter_credentials_file(credentials_path)?;

    match (settings, credentials) {
        (None, None) => Ok(default_legacy_runtime_settings_snapshot()),
        (None, Some(_)) => Err(CommandError::user_fixable(
            "runtime_settings_contract_failed",
            format!(
                "Cadence found OpenRouter credentials at {} without the matching runtime settings file at {}.",
                credentials_path.display(),
                settings_path.display()
            ),
        )),
        (Some(settings), credentials) => validate_legacy_runtime_settings_contract(
            settings_path,
            credentials_path,
            &settings,
            credentials,
        ),
    }
}

fn read_legacy_runtime_settings_file(
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

    Ok(Some(validate_legacy_runtime_settings_file(
        parsed,
        "runtime_settings_decode_failed",
    )?))
}

fn read_legacy_openrouter_credentials_file(
    path: &Path,
) -> CommandResult<Option<LegacyOpenRouterCredentialFile>> {
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

    let parsed =
        serde_json::from_str::<LegacyOpenRouterCredentialFile>(&contents).map_err(|error| {
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

    Ok(Some(LegacyOpenRouterCredentialFile {
        api_key: api_key.to_owned(),
        updated_at: normalize_updated_at(parsed.updated_at),
    }))
}

fn validate_legacy_runtime_settings_file(
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

    if !supports_legacy_runtime_settings_provider(provider.provider_id) {
        return Err(CommandError::user_fixable(
            error_code,
            format!(
                "Cadence only supports legacy runtime-settings compatibility files for `openai_codex`, `openrouter`, or `anthropic`, not `{}`.",
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
        openrouter_api_key_configured: file.openrouter_api_key_configured,
        updated_at: normalize_updated_at(file.updated_at),
    })
}

fn validate_legacy_runtime_settings_contract(
    settings_path: &Path,
    credentials_path: &Path,
    settings: &RuntimeSettingsFile,
    credentials: Option<LegacyOpenRouterCredentialFile>,
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

    let openrouter_api_key = credentials
        .as_ref()
        .map(|credentials| credentials.api_key.clone());
    let openrouter_updated_at = credentials
        .as_ref()
        .map(|credentials| credentials.updated_at.clone());

    Ok(RuntimeSettingsSnapshot {
        settings: settings.clone(),
        runtime_kind: settings.provider_id.clone(),
        provider_api_key: if settings.provider_id == OPENROUTER_PROVIDER_ID {
            openrouter_api_key.clone()
        } else {
            None
        },
        provider_api_key_updated_at: if settings.provider_id == OPENROUTER_PROVIDER_ID {
            openrouter_updated_at.clone()
        } else {
            None
        },
        preset_id: None,
        base_url: None,
        api_version: None,
        region: None,
        project_id: None,
        openrouter_api_key,
        openrouter_credentials_updated_at: openrouter_updated_at,
        anthropic_api_key: None,
        anthropic_credentials_updated_at: None,
    })
}

fn default_legacy_runtime_settings_snapshot() -> RuntimeSettingsSnapshot {
    RuntimeSettingsSnapshot {
        settings: RuntimeSettingsFile {
            provider_id: DEFAULT_RUNTIME_PROVIDER_ID.into(),
            model_id: DEFAULT_RUNTIME_MODEL_ID.into(),
            openrouter_api_key_configured: false,
            updated_at: crate::auth::now_timestamp(),
        },
        runtime_kind: DEFAULT_RUNTIME_PROVIDER_ID.into(),
        provider_api_key: None,
        provider_api_key_updated_at: None,
        preset_id: None,
        base_url: None,
        api_version: None,
        region: None,
        project_id: None,
        openrouter_api_key: None,
        openrouter_credentials_updated_at: None,
        anthropic_api_key: None,
        anthropic_credentials_updated_at: None,
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

fn supports_legacy_runtime_settings_provider(provider_id: &str) -> bool {
    provider_id == OPENAI_CODEX_PROVIDER_ID
        || provider_id == OPENROUTER_PROVIDER_ID
        || provider_id == ANTHROPIC_PROVIDER_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use tempfile::tempdir;

    fn write(path: &Path, contents: &str) {
        fs::write(path, contents).expect("write fixture");
    }

    #[test]
    fn missing_files_produce_default_snapshot() {
        let dir = tempdir().expect("tempdir");
        let settings = dir.path().join("runtime-settings.json");
        let credentials = dir.path().join("openrouter-credentials.json");

        let snapshot = load_legacy_runtime_settings_snapshot_from_paths(&settings, &credentials)
            .expect("default snapshot");
        assert_eq!(snapshot.settings.provider_id, DEFAULT_RUNTIME_PROVIDER_ID);
        assert_eq!(snapshot.settings.model_id, DEFAULT_RUNTIME_MODEL_ID);
        assert!(!snapshot.settings.openrouter_api_key_configured);
        assert!(snapshot.openrouter_api_key.is_none());
    }

    #[test]
    fn credentials_without_settings_is_rejected() {
        let dir = tempdir().expect("tempdir");
        let settings = dir.path().join("runtime-settings.json");
        let credentials = dir.path().join("openrouter-credentials.json");
        write(
            &credentials,
            r#"{"apiKey":"sk-or-test","updatedAt":"2025-01-01T00:00:00Z"}"#,
        );

        let error = load_legacy_runtime_settings_snapshot_from_paths(&settings, &credentials)
            .expect_err("must error");
        assert_eq!(error.code, "runtime_settings_contract_failed");
    }

    #[test]
    fn openai_codex_settings_round_trip() {
        let dir = tempdir().expect("tempdir");
        let settings = dir.path().join("runtime-settings.json");
        let credentials = dir.path().join("openrouter-credentials.json");
        write(
            &settings,
            r#"{
                "providerId":"openai_codex",
                "modelId":"gpt-5.4",
                "openrouterApiKeyConfigured":false,
                "updatedAt":"2025-01-02T03:04:05Z"
            }"#,
        );

        let snapshot = load_legacy_runtime_settings_snapshot_from_paths(&settings, &credentials)
            .expect("snapshot");
        assert_eq!(snapshot.settings.provider_id, OPENAI_CODEX_PROVIDER_ID);
        assert_eq!(snapshot.settings.model_id, "gpt-5.4");
        assert!(!snapshot.settings.openrouter_api_key_configured);
        assert_eq!(snapshot.runtime_kind, OPENAI_CODEX_PROVIDER_ID);
        assert!(snapshot.openrouter_api_key.is_none());
    }

    #[test]
    fn openrouter_settings_with_credentials() {
        let dir = tempdir().expect("tempdir");
        let settings = dir.path().join("runtime-settings.json");
        let credentials = dir.path().join("openrouter-credentials.json");
        write(
            &settings,
            r#"{
                "providerId":"openrouter",
                "modelId":"qwen/qwen3-coder:free",
                "openrouterApiKeyConfigured":true,
                "updatedAt":"2025-01-02T03:04:05Z"
            }"#,
        );
        write(
            &credentials,
            r#"{"apiKey":"sk-or-secret","updatedAt":"2025-01-02T03:04:05Z"}"#,
        );

        let snapshot = load_legacy_runtime_settings_snapshot_from_paths(&settings, &credentials)
            .expect("snapshot");
        assert_eq!(snapshot.settings.provider_id, OPENROUTER_PROVIDER_ID);
        assert!(snapshot.settings.openrouter_api_key_configured);
        assert_eq!(snapshot.openrouter_api_key.as_deref(), Some("sk-or-secret"));
        assert_eq!(snapshot.provider_api_key.as_deref(), Some("sk-or-secret"));
    }

    #[test]
    fn flag_mismatch_with_credentials_rejected() {
        let dir = tempdir().expect("tempdir");
        let settings = dir.path().join("runtime-settings.json");
        let credentials = dir.path().join("openrouter-credentials.json");
        write(
            &settings,
            r#"{
                "providerId":"openrouter",
                "modelId":"qwen/qwen3-coder:free",
                "openrouterApiKeyConfigured":false,
                "updatedAt":"2025-01-02T03:04:05Z"
            }"#,
        );
        write(
            &credentials,
            r#"{"apiKey":"sk-or-secret","updatedAt":"2025-01-02T03:04:05Z"}"#,
        );

        let error = load_legacy_runtime_settings_snapshot_from_paths(&settings, &credentials)
            .expect_err("must error");
        assert_eq!(error.code, "runtime_settings_contract_failed");
    }

    #[test]
    fn blank_provider_rejected() {
        let dir = tempdir().expect("tempdir");
        let settings = dir.path().join("runtime-settings.json");
        let credentials = dir.path().join("openrouter-credentials.json");
        write(
            &settings,
            r#"{
                "providerId":"",
                "modelId":"gpt-5.4",
                "openrouterApiKeyConfigured":false,
                "updatedAt":"2025-01-02T03:04:05Z"
            }"#,
        );

        let error = load_legacy_runtime_settings_snapshot_from_paths(&settings, &credentials)
            .expect_err("must error");
        assert_eq!(error.code, "runtime_settings_decode_failed");
    }

    #[test]
    fn blank_api_key_rejected() {
        let dir = tempdir().expect("tempdir");
        let settings = dir.path().join("runtime-settings.json");
        let credentials = dir.path().join("openrouter-credentials.json");
        write(
            &settings,
            r#"{
                "providerId":"openai_codex",
                "modelId":"gpt-5.4",
                "openrouterApiKeyConfigured":true,
                "updatedAt":"2025-01-02T03:04:05Z"
            }"#,
        );
        write(
            &credentials,
            r#"{"apiKey":"   ","updatedAt":"2025-01-02T03:04:05Z"}"#,
        );

        let error = load_legacy_runtime_settings_snapshot_from_paths(&settings, &credentials)
            .expect_err("must error");
        assert_eq!(error.code, "openrouter_credentials_invalid");
    }

    #[test]
    fn unsupported_provider_rejected() {
        let dir = tempdir().expect("tempdir");
        let settings = dir.path().join("runtime-settings.json");
        let credentials = dir.path().join("openrouter-credentials.json");
        write(
            &settings,
            r#"{
                "providerId":"ollama",
                "modelId":"llama-3",
                "openrouterApiKeyConfigured":false,
                "updatedAt":"2025-01-02T03:04:05Z"
            }"#,
        );

        let error = load_legacy_runtime_settings_snapshot_from_paths(&settings, &credentials)
            .expect_err("must error");
        assert_eq!(error.code, "runtime_settings_decode_failed");
    }
}
