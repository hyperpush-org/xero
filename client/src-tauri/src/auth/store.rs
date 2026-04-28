use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};

use super::{
    now_timestamp,
    sql::{
        clear_openai_codex_sessions as sql_clear, load_latest_openai_codex_session as sql_latest,
        load_openai_codex_session_by_account as sql_load_by_account,
        load_openai_codex_session_by_session_id as sql_load_by_session,
        remove_openai_codex_session as sql_remove, upsert_openai_codex_session as sql_upsert,
    },
    AuthFlowError, OPENAI_CODEX_PROVIDER_ID,
};
use crate::{
    commands::{CommandError, RuntimeAuthPhase},
    global_db::open_global_database,
    provider_credentials::{
        delete_provider_credential as cred_delete, load_provider_credentials_view_or_default,
        upsert_provider_credential as cred_upsert, ProviderCredentialKind, ProviderCredentialLink,
        ProviderCredentialRecord, ProviderCredentialsView, OPENAI_CODEX_DEFAULT_PROFILE_ID,
    },
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

pub fn load_openai_codex_session(
    path: &Path,
    account_id: &str,
) -> Result<Option<StoredOpenAiCodexSession>, AuthFlowError> {
    let connection = open_db(path)?;
    sql_load_by_account(&connection, account_id)
}

pub fn load_openai_codex_session_for_profile_link(
    path: &Path,
    link: &ProviderCredentialLink,
) -> Result<Option<StoredOpenAiCodexSession>, AuthFlowError> {
    let ProviderCredentialLink::OpenAiCodex {
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

    let connection = open_db(path)?;
    if let Some(stored) = sql_load_by_account(&connection, account_id)? {
        return Ok(Some(stored));
    }

    if let Some(stored) = sql_load_by_session(&connection, session_id)? {
        return Ok(Some(stored));
    }

    sql_latest(&connection)
}

pub fn load_latest_openai_codex_session(
    path: &Path,
) -> Result<Option<StoredOpenAiCodexSession>, AuthFlowError> {
    let connection = open_db(path)?;
    sql_latest(&connection)
}

pub fn persist_openai_codex_session(
    path: &Path,
    session: StoredOpenAiCodexSession,
) -> Result<(), AuthFlowError> {
    let connection = open_db(path)?;
    sql_upsert(&connection, &session)
}

pub fn remove_openai_codex_session(path: &Path, account_id: &str) -> Result<(), AuthFlowError> {
    let connection = open_db(path)?;
    sql_remove(&connection, account_id)
}

pub fn clear_openai_codex_sessions(path: &Path) -> Result<(), AuthFlowError> {
    let connection = open_db(path)?;
    sql_clear(&connection)
}

fn open_db(path: &Path) -> Result<rusqlite::Connection, AuthFlowError> {
    open_global_database(path).map_err(map_command_error_to_auth_error)
}

pub fn sync_openai_profile_link<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    preferred_profile_id: Option<&str>,
    session: Option<&StoredOpenAiCodexSession>,
) -> Result<(), AuthFlowError> {
    let connection = open_global_database(
        &state
            .global_db_path(app)
            .map_err(map_command_error_to_auth_error)?,
    )
    .map_err(map_command_error_to_auth_error)?;
    let view = load_provider_credentials_view_or_default(&connection)
        .map_err(map_provider_profiles_error)?;

    let next_link = session.map(openai_profile_link_from_session).transpose()?;
    let target_profile_ids =
        resolve_openai_profile_sync_targets(&view, preferred_profile_id, next_link.as_ref())?;
    if target_profile_ids.is_empty() {
        return Ok(());
    }

    let updated_at = next_link
        .as_ref()
        .map(profile_link_updated_at)
        .unwrap_or_else(now_timestamp);

    // Source of truth is provider_credentials. The profile-shaped runtime view
    // projects from this row on the next load.
    mirror_openai_credential(&connection, session, &updated_at)
}

/// Phase 2.3 (write-through): keep the new flat `provider_credentials` row for
/// `openai_codex` in sync with the OAuth session writes that still flow through
/// the legacy profile machinery. When `session` is `Some`, upsert the
/// credential row; when it's `None`, drop the row so the new readers see the
/// signed-out state.
fn mirror_openai_credential(
    connection: &rusqlite::Connection,
    session: Option<&StoredOpenAiCodexSession>,
    fallback_updated_at: &str,
) -> Result<(), AuthFlowError> {
    let result = match session {
        Some(session) => cred_upsert(
            connection,
            &ProviderCredentialRecord {
                provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
                kind: ProviderCredentialKind::OAuthSession,
                api_key: None,
                oauth_account_id: Some(session.account_id.clone()),
                oauth_session_id: Some(session.session_id.clone()),
                oauth_access_token: Some(session.access_token.clone()),
                oauth_refresh_token: Some(session.refresh_token.clone()),
                oauth_expires_at: Some(session.expires_at),
                base_url: None,
                api_version: None,
                region: None,
                project_id: None,
                default_model_id: None,
                updated_at: if session.updated_at.trim().is_empty() {
                    fallback_updated_at.to_owned()
                } else {
                    session.updated_at.clone()
                },
            },
        ),
        None => cred_delete(connection, OPENAI_CODEX_PROVIDER_ID),
    };
    result.map_err(map_provider_profiles_error)
}

pub fn ensure_openai_profile_target<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile_id: &str,
    phase: RuntimeAuthPhase,
    action: &str,
) -> Result<(), AuthFlowError> {
    let view = load_provider_credentials_view(app, state)?;
    validate_target_openai_profile(&view, profile_id, phase, action)
}

fn load_provider_credentials_view<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> Result<ProviderCredentialsView, AuthFlowError> {
    let connection = open_global_database(
        &state
            .global_db_path(app)
            .map_err(map_command_error_to_auth_error)?,
    )
    .map_err(map_command_error_to_auth_error)?;
    load_provider_credentials_view_or_default(&connection).map_err(map_provider_profiles_error)
}

fn validate_target_openai_profile(
    view: &ProviderCredentialsView,
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

    let profile = view.profile(profile_id).ok_or_else(|| {
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
    view: &ProviderCredentialsView,
    preferred_profile_id: Option<&str>,
    next_link: Option<&ProviderCredentialLink>,
) -> Result<Vec<String>, AuthFlowError> {
    let preferred_profile_id = preferred_profile_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(preferred_profile_id) = preferred_profile_id {
        validate_target_openai_profile(
            view,
            preferred_profile_id,
            RuntimeAuthPhase::Failed,
            "sync OpenAI auth onto the selected provider profile",
        )?;
    }

    let mut profile_ids = view
        .profiles()
        .iter()
        .filter(|profile| profile.provider_id == OPENAI_CODEX_PROVIDER_ID)
        .map(|profile| profile.profile_id.clone())
        .collect::<Vec<_>>();

    if profile_ids.is_empty() {
        profile_ids.push(
            preferred_profile_id
                .map(str::to_owned)
                .or_else(|| select_openai_profile_id(view, next_link))
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

fn openai_profile_link_from_session(
    session: &StoredOpenAiCodexSession,
) -> Result<ProviderCredentialLink, AuthFlowError> {
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

    Ok(ProviderCredentialLink::OpenAiCodex {
        account_id: account_id.to_owned(),
        session_id: session_id.to_owned(),
        updated_at: normalize_updated_at(&session.updated_at),
    })
}

fn select_openai_profile_id(
    view: &ProviderCredentialsView,
    next_link: Option<&ProviderCredentialLink>,
) -> Option<String> {
    if let Some(ProviderCredentialLink::OpenAiCodex {
        account_id,
        session_id,
        ..
    }) = next_link
    {
        if let Some(profile) = view.profiles().iter().find(|profile| {
            profile.provider_id == OPENAI_CODEX_PROVIDER_ID
                && matches!(
                    profile.credential_link.as_ref(),
                    Some(ProviderCredentialLink::OpenAiCodex {
                        account_id: linked_account_id,
                        session_id: linked_session_id,
                        ..
                    }) if linked_account_id == account_id || linked_session_id == session_id
                )
        }) {
            return Some(profile.profile_id.clone());
        }
    }

    view.active_profile()
        .filter(|profile| profile.provider_id == OPENAI_CODEX_PROVIDER_ID)
        .map(|profile| profile.profile_id.clone())
        .or_else(|| {
            view.profile(OPENAI_CODEX_DEFAULT_PROFILE_ID)
                .filter(|profile| profile.provider_id == OPENAI_CODEX_PROVIDER_ID)
                .map(|profile| profile.profile_id.clone())
        })
        .or_else(|| {
            view.profiles()
                .iter()
                .find(|profile| profile.provider_id == OPENAI_CODEX_PROVIDER_ID)
                .map(|profile| profile.profile_id.clone())
        })
}

fn profile_link_updated_at(link: &ProviderCredentialLink) -> String {
    match link {
        ProviderCredentialLink::OpenAiCodex { updated_at, .. }
        | ProviderCredentialLink::ApiKey { updated_at }
        | ProviderCredentialLink::Local { updated_at }
        | ProviderCredentialLink::Ambient { updated_at } => normalize_updated_at(updated_at),
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
