use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};

use super::{now_timestamp, AuthFlowError, OPENAI_CODEX_PROVIDER_ID};
use crate::{
    commands::{CommandError, RuntimeAuthPhase},
    provider_profiles::{
        build_openai_default_profile, load_or_migrate_provider_profiles_from_paths,
        persist_provider_profiles_snapshot, ProviderProfileCredentialLink,
        ProviderProfilesSnapshot, OPENAI_CODEX_DEFAULT_PROFILE_ID,
    },
    runtime::openai_codex_provider,
    state::DesktopState,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredOpenAiCodexSession {
    pub provider_id: String,
    pub session_id: String,
    pub account_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub updated_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthStoreFile {
    openai_codex_sessions: HashMap<String, StoredOpenAiCodexSession>,
    updated_at: String,
}

pub fn load_openai_codex_session(
    path: &Path,
    account_id: &str,
) -> Result<Option<StoredOpenAiCodexSession>, AuthFlowError> {
    let store = load_store(path)?;
    Ok(store.openai_codex_sessions.get(account_id).cloned())
}

pub fn load_openai_codex_session_for_profile_link(
    path: &Path,
    link: &ProviderProfileCredentialLink,
) -> Result<Option<StoredOpenAiCodexSession>, AuthFlowError> {
    let ProviderProfileCredentialLink::OpenAiCodex {
        account_id,
        session_id,
        ..
    } = link
    else {
        return Err(AuthFlowError::terminal(
            "provider_profiles_invalid",
            RuntimeAuthPhase::Failed,
            "Cadence rejected the active OpenAI provider profile because it referenced a non-OpenAI credential link.",
        ));
    };

    let store = load_store(path)?;
    if let Some(stored) = store.openai_codex_sessions.get(account_id).cloned() {
        return Ok(Some(stored));
    }

    Ok(store
        .openai_codex_sessions
        .values()
        .max_by(|left, right| {
            let left_linked = left.session_id == *session_id;
            let right_linked = right.session_id == *session_id;
            left_linked
                .cmp(&right_linked)
                .then_with(|| left.updated_at.cmp(&right.updated_at))
        })
        .cloned())
}

pub fn load_latest_openai_codex_session(
    path: &Path,
) -> Result<Option<StoredOpenAiCodexSession>, AuthFlowError> {
    let store = load_store(path)?;
    Ok(store
        .openai_codex_sessions
        .values()
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
        .cloned())
}

pub fn persist_openai_codex_session(
    path: &Path,
    session: StoredOpenAiCodexSession,
) -> Result<(), AuthFlowError> {
    let mut store = load_store(path)?;
    store.updated_at = now_timestamp();
    store
        .openai_codex_sessions
        .insert(session.account_id.clone(), session);
    write_store(path, &store)
}

pub fn remove_openai_codex_session(path: &Path, account_id: &str) -> Result<(), AuthFlowError> {
    let mut store = load_store(path)?;
    if store.openai_codex_sessions.remove(account_id).is_none() {
        return Ok(());
    }

    store.updated_at = now_timestamp();
    write_store(path, &store)
}

pub fn clear_openai_codex_sessions(path: &Path) -> Result<(), AuthFlowError> {
    let mut store = load_store(path)?;
    if store.openai_codex_sessions.is_empty() {
        return Ok(());
    }

    store.openai_codex_sessions.clear();
    store.updated_at = now_timestamp();
    write_store(path, &store)
}

pub fn sync_openai_profile_link<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    preferred_profile_id: Option<&str>,
    session: Option<&StoredOpenAiCodexSession>,
) -> Result<(), AuthFlowError> {
    let provider_profiles_path = state
        .provider_profiles_file(app)
        .map_err(map_command_error_to_auth_error)?;
    let provider_profile_credentials_path = state
        .provider_profile_credential_store_file(app)
        .map_err(map_command_error_to_auth_error)?;
    let mut snapshot = load_provider_profiles_snapshot(app, state)?;

    let next_link = session.map(openai_profile_link_from_session).transpose()?;
    let target_profile_ids =
        resolve_openai_profile_sync_targets(&snapshot, preferred_profile_id, next_link.as_ref())?;
    if target_profile_ids.is_empty() {
        return Ok(());
    }

    let updated_at = next_link
        .as_ref()
        .map(profile_link_updated_at)
        .unwrap_or_else(now_timestamp);
    let mut changed = false;
    for target_profile_id in target_profile_ids {
        changed |= upsert_openai_profile_link(
            &mut snapshot,
            &target_profile_id,
            next_link.clone(),
            &updated_at,
        )?;
    }
    if !changed {
        return Ok(());
    }

    snapshot.metadata.updated_at = updated_at;
    persist_provider_profiles_snapshot(
        &provider_profiles_path,
        &provider_profile_credentials_path,
        &snapshot,
    )
    .map_err(map_provider_profiles_error)
}

pub fn ensure_openai_profile_target<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile_id: &str,
    phase: RuntimeAuthPhase,
    action: &str,
) -> Result<(), AuthFlowError> {
    let snapshot = load_provider_profiles_snapshot(app, state)?;
    validate_target_openai_profile(&snapshot, profile_id, phase, action)
}

fn load_provider_profiles_snapshot<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> Result<ProviderProfilesSnapshot, AuthFlowError> {
    let provider_profiles_path = state
        .provider_profiles_file(app)
        .map_err(map_command_error_to_auth_error)?;
    let provider_profile_credentials_path = state
        .provider_profile_credential_store_file(app)
        .map_err(map_command_error_to_auth_error)?;
    let legacy_settings_path = state
        .runtime_settings_file(app)
        .map_err(map_command_error_to_auth_error)?;
    let legacy_openrouter_credentials_path = state
        .openrouter_credential_file(app)
        .map_err(map_command_error_to_auth_error)?;
    let legacy_openai_auth_path =
        state.auth_store_file_for_provider(app, openai_codex_provider())?;

    load_or_migrate_provider_profiles_from_paths(
        &provider_profiles_path,
        &provider_profile_credentials_path,
        &legacy_settings_path,
        &legacy_openrouter_credentials_path,
        &legacy_openai_auth_path,
    )
    .map_err(map_provider_profiles_error)
}

fn validate_target_openai_profile(
    snapshot: &ProviderProfilesSnapshot,
    profile_id: &str,
    phase: RuntimeAuthPhase,
    action: &str,
) -> Result<(), AuthFlowError> {
    let profile_id = profile_id.trim();
    if profile_id.is_empty() {
        return Err(AuthFlowError::terminal(
            "invalid_request",
            phase,
            "Field `profileId` must be a non-empty string.",
        ));
    }

    let profile = snapshot.profile(profile_id).ok_or_else(|| {
        AuthFlowError::terminal(
            "provider_profile_missing",
            phase.clone(),
            format!(
                "Cadence rejected {action} because provider profile `{profile_id}` was not found. Repair the provider-profile metadata or select a different OpenAI profile."
            ),
        )
    })?;

    if profile.provider_id != OPENAI_CODEX_PROVIDER_ID {
        return Err(AuthFlowError::terminal(
            "provider_profile_provider_mismatch",
            phase,
            format!(
                "Cadence rejected {action} because provider profile `{profile_id}` belongs to provider `{}` instead of `{OPENAI_CODEX_PROVIDER_ID}`. Select an OpenAI profile or repair the provider-profile metadata.",
                profile.provider_id
            ),
        ));
    }

    Ok(())
}

fn resolve_openai_profile_sync_targets(
    snapshot: &ProviderProfilesSnapshot,
    preferred_profile_id: Option<&str>,
    next_link: Option<&ProviderProfileCredentialLink>,
) -> Result<Vec<String>, AuthFlowError> {
    let preferred_profile_id = preferred_profile_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(preferred_profile_id) = preferred_profile_id {
        validate_target_openai_profile(
            snapshot,
            preferred_profile_id,
            RuntimeAuthPhase::Failed,
            "sync OpenAI auth onto the selected provider profile",
        )?;
    }

    let mut profile_ids = snapshot
        .metadata
        .profiles
        .iter()
        .filter(|profile| profile.provider_id == OPENAI_CODEX_PROVIDER_ID)
        .map(|profile| profile.profile_id.clone())
        .collect::<Vec<_>>();

    if profile_ids.is_empty() {
        profile_ids.push(
            preferred_profile_id
                .map(str::to_owned)
                .or_else(|| select_openai_profile_id(snapshot, next_link))
                .unwrap_or_else(|| OPENAI_CODEX_DEFAULT_PROFILE_ID.to_owned()),
        );
    } else if let Some(preferred_profile_id) = preferred_profile_id {
        if !profile_ids
            .iter()
            .any(|profile_id| profile_id == preferred_profile_id)
        {
            profile_ids.push(preferred_profile_id.to_owned());
        }
    }

    Ok(profile_ids)
}

fn load_store(path: &Path) -> Result<AuthStoreFile, AuthFlowError> {
    if !path.exists() {
        return Ok(AuthStoreFile {
            openai_codex_sessions: HashMap::new(),
            updated_at: now_timestamp(),
        });
    }

    let contents = fs::read_to_string(path).map_err(|error| {
        AuthFlowError::terminal(
            "auth_store_read_failed",
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence could not read the app-local auth store at {}: {error}",
                path.display()
            ),
        )
    })?;

    serde_json::from_str(&contents).map_err(|error| {
        AuthFlowError::terminal(
            "auth_store_decode_failed",
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence could not decode the app-local auth store at {}: {error}",
                path.display()
            ),
        )
    })
}

fn write_store(path: &Path, store: &AuthStoreFile) -> Result<(), AuthFlowError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AuthFlowError::terminal(
                "auth_store_directory_unavailable",
                RuntimeAuthPhase::Failed,
                format!(
                    "Cadence could not prepare the app-local auth directory at {}: {error}",
                    parent.display()
                ),
            )
        })?;
    }

    let json = serde_json::to_string_pretty(store).map_err(|error| {
        AuthFlowError::terminal(
            "auth_store_encode_failed",
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence could not serialize the {} auth session store: {error}",
                OPENAI_CODEX_PROVIDER_ID
            ),
        )
    })?;

    fs::write(path, json).map_err(|error| {
        AuthFlowError::terminal(
            "auth_store_write_failed",
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence could not persist the app-local auth store at {}: {error}",
                path.display()
            ),
        )
    })
}

fn openai_profile_link_from_session(
    session: &StoredOpenAiCodexSession,
) -> Result<ProviderProfileCredentialLink, AuthFlowError> {
    let account_id = session.account_id.trim();
    if account_id.is_empty() {
        return Err(AuthFlowError::terminal(
            "provider_profiles_invalid",
            RuntimeAuthPhase::Failed,
            "Cadence rejected the OpenAI auth session because accountId was blank while syncing the provider profile.",
        ));
    }

    let session_id = session.session_id.trim();
    if session_id.is_empty() {
        return Err(AuthFlowError::terminal(
            "provider_profiles_invalid",
            RuntimeAuthPhase::Failed,
            "Cadence rejected the OpenAI auth session because sessionId was blank while syncing the provider profile.",
        ));
    }

    let provider_id = session.provider_id.trim();
    if !provider_id.is_empty() && provider_id != OPENAI_CODEX_PROVIDER_ID {
        return Err(AuthFlowError::terminal(
            "provider_profiles_invalid",
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence rejected the OpenAI auth session because providerId `{provider_id}` was not `{OPENAI_CODEX_PROVIDER_ID}` while syncing the provider profile."
            ),
        ));
    }

    Ok(ProviderProfileCredentialLink::OpenAiCodex {
        account_id: account_id.to_owned(),
        session_id: session_id.to_owned(),
        updated_at: normalize_updated_at(&session.updated_at),
    })
}

fn select_openai_profile_id(
    snapshot: &ProviderProfilesSnapshot,
    next_link: Option<&ProviderProfileCredentialLink>,
) -> Option<String> {
    if let Some(ProviderProfileCredentialLink::OpenAiCodex {
        account_id,
        session_id,
        ..
    }) = next_link
    {
        if let Some(profile) = snapshot.metadata.profiles.iter().find(|profile| {
            profile.provider_id == OPENAI_CODEX_PROVIDER_ID
                && matches!(
                    profile.credential_link.as_ref(),
                    Some(ProviderProfileCredentialLink::OpenAiCodex {
                        account_id: linked_account_id,
                        session_id: linked_session_id,
                        ..
                    }) if linked_account_id == account_id || linked_session_id == session_id
                )
        }) {
            return Some(profile.profile_id.clone());
        }
    }

    snapshot
        .active_profile()
        .filter(|profile| profile.provider_id == OPENAI_CODEX_PROVIDER_ID)
        .map(|profile| profile.profile_id.clone())
        .or_else(|| {
            snapshot
                .profile(OPENAI_CODEX_DEFAULT_PROFILE_ID)
                .filter(|profile| profile.provider_id == OPENAI_CODEX_PROVIDER_ID)
                .map(|profile| profile.profile_id.clone())
        })
        .or_else(|| {
            snapshot
                .metadata
                .profiles
                .iter()
                .find(|profile| profile.provider_id == OPENAI_CODEX_PROVIDER_ID)
                .map(|profile| profile.profile_id.clone())
        })
}

fn upsert_openai_profile_link(
    snapshot: &mut ProviderProfilesSnapshot,
    profile_id: &str,
    next_link: Option<ProviderProfileCredentialLink>,
    updated_at: &str,
) -> Result<bool, AuthFlowError> {
    if let Some(existing) = snapshot
        .metadata
        .profiles
        .iter_mut()
        .find(|profile| profile.profile_id == profile_id)
    {
        if existing.provider_id != OPENAI_CODEX_PROVIDER_ID {
            return Err(AuthFlowError::terminal(
                "provider_profiles_invalid",
                RuntimeAuthPhase::Failed,
                format!(
                    "Cadence rejected provider profile `{profile_id}` because OpenAI auth can only sync onto `{OPENAI_CODEX_PROVIDER_ID}` profiles."
                ),
            ));
        }

        if existing.credential_link == next_link {
            return Ok(false);
        }

        existing.credential_link = next_link;
        existing.updated_at = updated_at.to_owned();
        return Ok(true);
    }

    let mut profile = build_openai_default_profile(next_link, None, updated_at);
    profile.profile_id = profile_id.to_owned();
    snapshot.metadata.profiles.push(profile);
    Ok(true)
}

fn profile_link_updated_at(link: &ProviderProfileCredentialLink) -> String {
    match link {
        ProviderProfileCredentialLink::OpenAiCodex { updated_at, .. }
        | ProviderProfileCredentialLink::ApiKey { updated_at }
        | ProviderProfileCredentialLink::Local { updated_at }
        | ProviderProfileCredentialLink::Ambient { updated_at } => normalize_updated_at(updated_at),
    }
}

fn normalize_updated_at(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        now_timestamp()
    } else {
        trimmed.to_owned()
    }
}

fn map_provider_profiles_error(error: CommandError) -> AuthFlowError {
    if error.retryable {
        AuthFlowError::retryable(error.code, RuntimeAuthPhase::Failed, error.message)
    } else {
        AuthFlowError::terminal(error.code, RuntimeAuthPhase::Failed, error.message)
    }
}

fn map_command_error_to_auth_error(error: CommandError) -> AuthFlowError {
    if error.retryable {
        AuthFlowError::retryable(error.code, RuntimeAuthPhase::Failed, error.message)
    } else {
        AuthFlowError::terminal(error.code, RuntimeAuthPhase::Failed, error.message)
    }
}
