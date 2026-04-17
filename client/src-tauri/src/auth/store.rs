use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use super::{now_timestamp, AuthFlowError, OPENAI_CODEX_PROVIDER_ID};
use crate::commands::RuntimeAuthPhase;

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
