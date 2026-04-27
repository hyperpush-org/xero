use std::{collections::HashMap, fs, path::Path};

use rusqlite::Connection;
use serde::Deserialize;

use crate::{
    auth::StoredOpenAiCodexSession,
    commands::{
        get_runtime_settings::{
            load_runtime_settings_snapshot_from_paths, RuntimeSettingsSnapshot,
        },
        CommandError, CommandResult,
    },
    runtime::{OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID},
};

use super::{
    sql::persist_provider_profiles_to_db,
    store::{
        build_openai_default_profile, build_openrouter_default_profile,
        decode_provider_profile_credentials_file, validate_provider_profiles_contract,
        OpenRouterProfileCredentialEntry, ProviderProfileCredentialLink,
        ProviderProfileCredentialsFile, ProviderProfilesMetadataFile,
        ProviderProfilesMigrationState, ProviderProfilesSnapshot, OPENAI_CODEX_DEFAULT_PROFILE_ID,
        OPENROUTER_DEFAULT_PROFILE_ID, OPENROUTER_FALLBACK_MODEL_ID,
    },
};

const LEGACY_MIGRATION_SOURCE: &str = "legacy_runtime_settings_v1";

/// Imports legacy provider-profile JSON state into the global SQLite database.
///
/// Idempotent: if the global DB already has a metadata row, this returns Ok(()) without doing
/// anything. Otherwise it tries:
///   1. Current-schema JSON (`provider-profiles.json` + `provider-profile-credentials.json`)
///   2. Pre-profiles legacy (runtime-settings.json + openrouter-credentials.json + openai-auth.json)
///      If neither is found, the function returns Ok(()) and the application creates a default
///      snapshot on first read.
///
/// JSON files are deleted only after a successful SQL write.
pub fn import_legacy_provider_profiles(
    connection: &mut Connection,
    metadata_path: &Path,
    credentials_path: &Path,
    legacy_settings_path: &Path,
    legacy_openrouter_credentials_path: &Path,
    legacy_openai_auth_path: &Path,
) -> CommandResult<()> {
    if metadata_row_exists(connection)? {
        return Ok(());
    }

    let snapshot = if let Some(snapshot) =
        load_provider_profiles_from_paths(metadata_path, credentials_path)?
    {
        Some((snapshot, ImportedFrom::Current))
    } else {
        build_legacy_provider_profiles_snapshot(
            legacy_settings_path,
            legacy_openrouter_credentials_path,
            legacy_openai_auth_path,
        )?
        .map(|snapshot| (snapshot, ImportedFrom::Legacy))
    };

    let Some((snapshot, source)) = snapshot else {
        return Ok(());
    };

    persist_provider_profiles_to_db(connection, &snapshot)?;

    match source {
        ImportedFrom::Current => {
            remove_file_if_exists(metadata_path)?;
            remove_file_if_exists(credentials_path)?;
        }
        ImportedFrom::Legacy => {
            remove_file_if_exists(legacy_settings_path)?;
            remove_file_if_exists(legacy_openrouter_credentials_path)?;
        }
    }

    Ok(())
}

enum ImportedFrom {
    Current,
    Legacy,
}

fn metadata_row_exists(connection: &Connection) -> CommandResult<bool> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM provider_profiles_metadata WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| {
            CommandError::retryable(
                "provider_profiles_read_failed",
                format!("Cadence could not probe provider_profiles_metadata: {error}"),
            )
        })?;
    Ok(count > 0)
}

fn load_provider_profiles_from_paths(
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

    let metadata = read_metadata_file(metadata_path)?.expect("metadata exists");
    let credentials = read_credentials_file(credentials_path)?.unwrap_or_default();

    Ok(Some(validate_provider_profiles_contract(
        metadata,
        credentials,
        metadata_path,
        credentials_path,
    )?))
}

fn read_metadata_file(path: &Path) -> CommandResult<Option<ProviderProfilesMetadataFile>> {
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

fn read_credentials_file(path: &Path) -> CommandResult<Option<ProviderProfileCredentialsFile>> {
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

    Ok(Some(decode_provider_profile_credentials_file(
        &contents, path,
    )?))
}

fn remove_file_if_exists(path: &Path) -> CommandResult<()> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(path).map_err(|error| {
        CommandError::retryable(
            "provider_profiles_legacy_cleanup_failed",
            format!(
                "Cadence imported {} into the global database but could not delete the legacy file: {error}",
                path.display()
            ),
        )
    })
}

fn build_legacy_provider_profiles_snapshot(
    legacy_settings_path: &Path,
    legacy_openrouter_credentials_path: &Path,
    legacy_openai_auth_path: &Path,
) -> CommandResult<Option<ProviderProfilesSnapshot>> {
    let settings_exists = legacy_settings_path.exists();
    let openrouter_credentials_exist = legacy_openrouter_credentials_path.exists();
    let openai_auth_exists = legacy_openai_auth_path.exists();

    if !settings_exists && !openrouter_credentials_exist && !openai_auth_exists {
        return Ok(None);
    }

    if !settings_exists && openrouter_credentials_exist {
        return Err(CommandError::user_fixable(
            "provider_profiles_migration_contract_failed",
            format!(
                "Cadence found legacy OpenRouter credentials at {} without the matching runtime settings file at {}.",
                legacy_openrouter_credentials_path.display(),
                legacy_settings_path.display()
            ),
        ));
    }

    let runtime_settings = if settings_exists {
        Some(load_runtime_settings_snapshot_from_paths(
            legacy_settings_path,
            legacy_openrouter_credentials_path,
        )?)
    } else {
        None
    };

    let openai_session = openai_latest_session(legacy_openai_auth_path)?;
    let openai_link = openai_session_link(legacy_openai_auth_path, openai_session.as_ref())?;

    if runtime_settings.is_none() && openai_link.is_none() {
        return Ok(None);
    }

    let migrated_at = crate::auth::now_timestamp();
    let active_provider_id = runtime_settings
        .as_ref()
        .map(|snapshot| snapshot.settings.provider_id.as_str())
        .unwrap_or(OPENAI_CODEX_PROVIDER_ID);

    let mut profiles = vec![build_openai_default_profile(
        openai_link,
        Some(&migrated_at),
        &migrated_at,
    )];

    let mut credentials = ProviderProfileCredentialsFile::default();
    let mut inferred_openrouter_model = None;

    if should_create_openrouter_profile(runtime_settings.as_ref()) {
        let model_id = runtime_settings
            .as_ref()
            .and_then(|snapshot| {
                (snapshot.settings.provider_id == OPENROUTER_PROVIDER_ID)
                    .then(|| snapshot.settings.model_id.clone())
            })
            .unwrap_or_else(|| {
                inferred_openrouter_model = Some(true);
                OPENROUTER_FALLBACK_MODEL_ID.into()
            });

        let openrouter_link = runtime_settings.as_ref().and_then(|snapshot| {
            snapshot
                .openrouter_credentials_updated_at
                .as_ref()
                .map(|updated_at| ProviderProfileCredentialLink::ApiKey {
                    updated_at: updated_at.clone(),
                })
        });

        profiles.push(build_openrouter_default_profile(
            &model_id,
            openrouter_link,
            Some(&migrated_at),
            &migrated_at,
        ));

        if let Some(snapshot) = runtime_settings.as_ref() {
            if let (Some(api_key), Some(updated_at)) = (
                snapshot.openrouter_api_key.as_ref(),
                snapshot.openrouter_credentials_updated_at.as_ref(),
            ) {
                credentials.api_keys.push(OpenRouterProfileCredentialEntry {
                    profile_id: OPENROUTER_DEFAULT_PROFILE_ID.into(),
                    api_key: api_key.clone(),
                    updated_at: updated_at.clone(),
                });
            }
        }
    }

    Ok(Some(ProviderProfilesSnapshot {
        metadata: ProviderProfilesMetadataFile {
            version: 3,
            active_profile_id: if active_provider_id == OPENROUTER_PROVIDER_ID {
                OPENROUTER_DEFAULT_PROFILE_ID.into()
            } else {
                OPENAI_CODEX_DEFAULT_PROFILE_ID.into()
            },
            profiles,
            updated_at: migrated_at.clone(),
            migration: Some(ProviderProfilesMigrationState {
                source: LEGACY_MIGRATION_SOURCE.into(),
                migrated_at,
                runtime_settings_updated_at: runtime_settings
                    .as_ref()
                    .map(|snapshot| snapshot.settings.updated_at.clone()),
                openrouter_credentials_updated_at: runtime_settings
                    .as_ref()
                    .and_then(|snapshot| snapshot.openrouter_credentials_updated_at.clone()),
                openai_auth_updated_at: profile_openai_updated_at(
                    openai_auth_exists,
                    openai_session.as_ref(),
                ),
                openrouter_model_inferred: inferred_openrouter_model,
            }),
        },
        credentials,
    }))
}

fn openai_session_link(
    legacy_openai_auth_path: &Path,
    session: Option<&StoredOpenAiCodexSession>,
) -> CommandResult<Option<ProviderProfileCredentialLink>> {
    let Some(session) = session else {
        return Ok(None);
    };

    let account_id = session.account_id.trim();
    if account_id.is_empty() {
        return Err(CommandError::user_fixable(
            "provider_profiles_migration_openai_link_invalid",
            format!(
                "Cadence rejected legacy OpenAI auth data at {} because accountId was blank.",
                legacy_openai_auth_path.display()
            ),
        ));
    }

    let session_id = session.session_id.trim();
    if session_id.is_empty() {
        return Err(CommandError::user_fixable(
            "provider_profiles_migration_openai_link_invalid",
            format!(
                "Cadence rejected legacy OpenAI auth data at {} because sessionId was blank.",
                legacy_openai_auth_path.display()
            ),
        ));
    }

    let provider_id = session.provider_id.trim();
    if !provider_id.is_empty() && provider_id != OPENAI_CODEX_PROVIDER_ID {
        return Err(CommandError::user_fixable(
            "provider_profiles_migration_openai_link_invalid",
            format!(
                "Cadence rejected legacy OpenAI auth data at {} because providerId `{}` was not `{}`.",
                legacy_openai_auth_path.display(),
                provider_id,
                OPENAI_CODEX_PROVIDER_ID
            ),
        ));
    }

    Ok(Some(ProviderProfileCredentialLink::OpenAiCodex {
        account_id: account_id.to_owned(),
        session_id: session_id.to_owned(),
        updated_at: session.updated_at.trim().to_owned(),
    }))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyOpenAiAuthFile {
    #[serde(default)]
    openai_codex_sessions: HashMap<String, StoredOpenAiCodexSession>,
    #[allow(dead_code)]
    #[serde(default)]
    updated_at: String,
}

/// Reads the legacy `openai-auth.json` directly so the provider-profiles importer can build a
/// migration link to the latest OpenAI session before the auth importer copies the rows into
/// `openai_codex_sessions`. After Phase 2.2 the runtime auth-store helpers expect SQLite, so we
/// can no longer reuse `load_latest_openai_codex_session` for this legacy JSON path.
fn openai_latest_session(
    legacy_openai_auth_path: &Path,
) -> CommandResult<Option<StoredOpenAiCodexSession>> {
    if !legacy_openai_auth_path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(legacy_openai_auth_path).map_err(|error| {
        CommandError::retryable(
            "provider_profiles_migration_auth_store_read_failed",
            format!(
                "Cadence could not read the legacy auth store at {} during provider-profile migration: {error}",
                legacy_openai_auth_path.display()
            ),
        )
    })?;

    let parsed: LegacyOpenAiAuthFile = serde_json::from_str(&contents).map_err(|error| {
        CommandError::user_fixable(
            "provider_profiles_migration_auth_store_decode_failed",
            format!(
                "Cadence could not decode the legacy auth store at {} during provider-profile migration: {error}",
                legacy_openai_auth_path.display()
            ),
        )
    })?;

    Ok(parsed
        .openai_codex_sessions
        .into_values()
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at)))
}

fn should_create_openrouter_profile(runtime_settings: Option<&RuntimeSettingsSnapshot>) -> bool {
    runtime_settings.is_some_and(|snapshot| {
        snapshot.settings.provider_id == OPENROUTER_PROVIDER_ID
            || snapshot.settings.openrouter_api_key_configured
    })
}

fn profile_openai_updated_at(
    openai_auth_exists: bool,
    session: Option<&StoredOpenAiCodexSession>,
) -> Option<String> {
    if !openai_auth_exists {
        return None;
    }

    session.map(|session| session.updated_at.trim().to_owned())
}
