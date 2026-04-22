use std::path::Path;

use crate::{
    auth::{load_latest_openai_codex_session, AuthFlowError, StoredOpenAiCodexSession},
    commands::{
        get_runtime_settings::{
            load_runtime_settings_snapshot_from_paths, RuntimeSettingsSnapshot,
        },
        CommandError, CommandResult,
    },
    runtime::{OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID},
};

use super::store::{
    build_openai_default_profile, build_openrouter_default_profile,
    default_provider_profiles_snapshot, load_provider_profiles_from_paths,
    persist_provider_profiles_snapshot, restore_file_snapshot, snapshot_existing_file,
    OpenRouterProfileCredentialEntry, ProviderProfileCredentialLink,
    ProviderProfileCredentialsFile, ProviderProfilesMetadataFile, ProviderProfilesMigrationState,
    ProviderProfilesSnapshot, OPENAI_CODEX_DEFAULT_PROFILE_ID, OPENROUTER_DEFAULT_PROFILE_ID,
    OPENROUTER_FALLBACK_MODEL_ID,
};

const LEGACY_MIGRATION_SOURCE: &str = "legacy_runtime_settings_v1";

pub fn load_or_migrate_provider_profiles_from_paths(
    metadata_path: &Path,
    credentials_path: &Path,
    legacy_settings_path: &Path,
    legacy_openrouter_credentials_path: &Path,
    legacy_openai_auth_path: &Path,
) -> CommandResult<ProviderProfilesSnapshot> {
    if let Some(snapshot) = load_provider_profiles_from_paths(metadata_path, credentials_path)? {
        return Ok(snapshot);
    }

    let Some(snapshot) = build_legacy_provider_profiles_snapshot(
        legacy_settings_path,
        legacy_openrouter_credentials_path,
        legacy_openai_auth_path,
    )?
    else {
        return Ok(default_provider_profiles_snapshot());
    };

    persist_migrated_provider_profiles(
        metadata_path,
        credentials_path,
        legacy_settings_path,
        legacy_openrouter_credentials_path,
        &snapshot,
    )?;

    Ok(snapshot)
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
                .map(|updated_at| ProviderProfileCredentialLink::OpenRouter {
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
                credentials
                    .openrouter_api_keys
                    .push(OpenRouterProfileCredentialEntry {
                        profile_id: OPENROUTER_DEFAULT_PROFILE_ID.into(),
                        api_key: api_key.clone(),
                        updated_at: updated_at.clone(),
                    });
            }
        }
    }

    Ok(Some(ProviderProfilesSnapshot {
        metadata: ProviderProfilesMetadataFile {
            version: 1,
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

fn persist_migrated_provider_profiles(
    metadata_path: &Path,
    credentials_path: &Path,
    legacy_settings_path: &Path,
    legacy_openrouter_credentials_path: &Path,
    snapshot: &ProviderProfilesSnapshot,
) -> CommandResult<()> {
    let previous_metadata = snapshot_existing_file(metadata_path, "provider_profiles")?;
    let previous_credentials =
        snapshot_existing_file(credentials_path, "provider_profile_credentials")?;
    let previous_legacy_settings =
        snapshot_existing_file(legacy_settings_path, "provider_profiles_migration")?;
    let previous_legacy_openrouter_credentials = snapshot_existing_file(
        legacy_openrouter_credentials_path,
        "provider_profiles_migration",
    )?;

    persist_provider_profiles_snapshot(metadata_path, credentials_path, snapshot)?;

    let cleanup_result = (|| -> CommandResult<()> {
        restore_file_snapshot(
            legacy_settings_path,
            None,
            "provider_profiles_migration_cleanup",
        )?;
        restore_file_snapshot(
            legacy_openrouter_credentials_path,
            None,
            "provider_profiles_migration_cleanup",
        )?;
        Ok(())
    })();

    if let Err(error) = cleanup_result {
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
        let legacy_settings_rollback = restore_file_snapshot(
            legacy_settings_path,
            previous_legacy_settings.as_deref(),
            "provider_profiles_migration_rollback",
        );
        let legacy_openrouter_rollback = restore_file_snapshot(
            legacy_openrouter_credentials_path,
            previous_legacy_openrouter_credentials.as_deref(),
            "provider_profiles_migration_rollback",
        );

        return match (
            metadata_rollback,
            credentials_rollback,
            legacy_settings_rollback,
            legacy_openrouter_rollback,
        ) {
            (Ok(()), Ok(()), Ok(()), Ok(())) => Err(error),
            (metadata_result, credentials_result, settings_result, openrouter_result) => {
                Err(CommandError::retryable(
                    "provider_profiles_migration_rollback_failed",
                    format!(
                        "Cadence failed to finalize legacy provider-profile migration and could not fully restore the previous app-local file set (metadata rollback: {}; credential rollback: {}; runtime-settings rollback: {}; openrouter-credentials rollback: {}). Cleanup error: {}",
                        rollback_message(metadata_result),
                        rollback_message(credentials_result),
                        rollback_message(settings_result),
                        rollback_message(openrouter_result),
                        error.message
                    ),
                ))
            }
        };
    }

    Ok(())
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

fn openai_latest_session(
    legacy_openai_auth_path: &Path,
) -> CommandResult<Option<StoredOpenAiCodexSession>> {
    load_latest_openai_codex_session(legacy_openai_auth_path).map_err(map_auth_store_error)
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

fn map_auth_store_error(error: AuthFlowError) -> CommandError {
    let code = format!("provider_profiles_migration_{}", error.code);
    if error.retryable {
        CommandError::retryable(code, error.message)
    } else {
        CommandError::user_fixable(code, error.message)
    }
}

fn rollback_message(result: CommandResult<()>) -> String {
    match result {
        Ok(()) => "ok".into(),
        Err(error) => format!("{}: {}", error.code, error.message),
    }
}
