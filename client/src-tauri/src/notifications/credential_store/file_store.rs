use std::{fs, io::Write, path::PathBuf};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::notifications::{NotificationAdapterError, NotificationRouteKind};

use super::{
    readiness::NotificationCredentialReadinessProjector,
    validation::{
        require_identifier, sanitize_upsert_credentials, NotificationCredentialUpsertInput,
    },
};

pub const NOTIFICATION_CREDENTIAL_STORE_FILE_NAME: &str = "notification-credentials.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationCredentialUpsertReceipt {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: NotificationRouteKind,
    pub has_bot_token: bool,
    pub has_chat_id: bool,
    pub has_webhook_url: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct FileNotificationCredentialStore {
    path: PathBuf,
}

impl FileNotificationCredentialStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn load_readiness_projector(&self) -> NotificationCredentialReadinessProjector {
        NotificationCredentialReadinessProjector::from_store_result(self.load_store_file())
    }

    pub fn upsert_route_credentials(
        &self,
        project_id: &str,
        route_id: &str,
        route_kind: NotificationRouteKind,
        credentials: NotificationCredentialUpsertInput,
        updated_at: &str,
    ) -> Result<NotificationCredentialUpsertReceipt, NotificationAdapterError> {
        let project_id = require_identifier(project_id, "projectId")?;
        let route_id = require_identifier(route_id, "routeId")?;
        let updated_at = require_identifier(updated_at, "updatedAt")?;

        let (bot_token, chat_id, webhook_url) =
            sanitize_upsert_credentials(route_kind, credentials, &project_id, &route_id)?;

        let mut store = self.load_store_file()?;

        if let Some(existing) = store
            .routes
            .iter_mut()
            .find(|entry| entry.project_id == project_id && entry.route_id == route_id)
        {
            existing.route_kind = route_kind.as_str().to_string();
            existing.bot_token = bot_token.clone();
            existing.chat_id = chat_id.clone();
            existing.webhook_url = webhook_url.clone();
        } else {
            store.routes.push(NotificationCredentialStoreEntry {
                project_id: project_id.clone(),
                route_id: route_id.clone(),
                route_kind: route_kind.as_str().to_string(),
                bot_token: bot_token.clone(),
                chat_id: chat_id.clone(),
                webhook_url: webhook_url.clone(),
            });
        }

        self.write_store_file(&store)?;

        Ok(NotificationCredentialUpsertReceipt {
            project_id,
            route_id,
            route_kind,
            has_bot_token: bot_token
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
            has_chat_id: chat_id
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
            has_webhook_url: webhook_url
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
            updated_at,
        })
    }

    pub(crate) fn load_store_file(
        &self,
    ) -> Result<NotificationCredentialStoreFile, NotificationAdapterError> {
        if !self.path.exists() {
            return Ok(NotificationCredentialStoreFile::default());
        }

        let contents = fs::read_to_string(&self.path).map_err(|error| {
            NotificationAdapterError::credentials_read_failed(format!(
                "Cadence could not read the app-local notification credential store at {}: {error}",
                self.path.display()
            ))
        })?;

        serde_json::from_str::<NotificationCredentialStoreFile>(&contents).map_err(|error| {
            NotificationAdapterError::credentials_malformed(format!(
                "Cadence could not decode the app-local notification credential store at {}: {error}",
                self.path.display()
            ))
        })
    }

    pub(crate) fn write_store_file(
        &self,
        store: &NotificationCredentialStoreFile,
    ) -> Result<(), NotificationAdapterError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                NotificationAdapterError::new(
                    "notification_adapter_credentials_directory_unavailable",
                    format!(
                        "Cadence could not prepare the app-local notification credential directory at {}: {error}",
                        parent.display()
                    ),
                    true,
                )
            })?;
        }

        let json = serde_json::to_string_pretty(store).map_err(|error| {
            NotificationAdapterError::new(
                "notification_adapter_credentials_encode_failed",
                format!(
                    "Cadence could not serialize the app-local notification credential store at {}: {error}",
                    self.path.display()
                ),
                false,
            )
        })?;

        let parent = self
            .path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));

        let mut temp_file = NamedTempFile::new_in(&parent).map_err(|error| {
            NotificationAdapterError::new(
                "notification_adapter_credentials_write_failed",
                format!(
                    "Cadence could not prepare a temporary app-local notification credential store file near {}: {error}",
                    self.path.display()
                ),
                true,
            )
        })?;

        temp_file.write_all(json.as_bytes()).map_err(|error| {
            NotificationAdapterError::new(
                "notification_adapter_credentials_write_failed",
                format!(
                    "Cadence could not write temporary app-local notification credential data near {}: {error}",
                    self.path.display()
                ),
                true,
            )
        })?;

        temp_file.as_file_mut().sync_all().map_err(|error| {
            NotificationAdapterError::new(
                "notification_adapter_credentials_write_failed",
                format!(
                    "Cadence could not flush temporary app-local notification credential data near {}: {error}",
                    self.path.display()
                ),
                true,
            )
        })?;

        temp_file.persist(&self.path).map_err(|error| {
            NotificationAdapterError::new(
                "notification_adapter_credentials_write_failed",
                format!(
                    "Cadence could not persist the app-local notification credential store at {}: {}",
                    self.path.display(),
                    error.error
                ),
                true,
            )
        })?;

        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationCredentialStoreFile {
    #[serde(default)]
    pub routes: Vec<NotificationCredentialStoreEntry>,
    #[serde(default)]
    pub inbound_cursors: Vec<NotificationInboundCursorEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationCredentialStoreEntry {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub bot_token: Option<String>,
    pub chat_id: Option<String>,
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationInboundCursorEntry {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub cursor: String,
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{FileNotificationCredentialStore, NOTIFICATION_CREDENTIAL_STORE_FILE_NAME};
    use crate::notifications::{
        NotificationCredentialReadinessStatus, NotificationCredentialUpsertInput,
        NotificationRouteKind,
    };

    #[test]
    fn readiness_projector_marks_unreadable_store_as_unavailable() {
        let root = tempdir().expect("temp dir");
        let store_path = root.path().join(NOTIFICATION_CREDENTIAL_STORE_FILE_NAME);
        fs::create_dir_all(&store_path).expect("create unreadable store directory");

        let store = FileNotificationCredentialStore::new(store_path);
        let readiness = store.load_readiness_projector().project_route(
            "project-1",
            "route-1",
            NotificationRouteKind::Telegram,
        );

        assert_eq!(
            readiness.status,
            NotificationCredentialReadinessStatus::Unavailable
        );
        assert_eq!(
            readiness
                .diagnostic
                .as_ref()
                .map(|diagnostic| diagnostic.code.as_str()),
            Some("notification_adapter_credentials_read_failed")
        );
    }

    #[test]
    fn upsert_route_credentials_fails_when_parent_directory_is_unavailable() {
        let root = tempdir().expect("temp dir");
        let parent_file = root.path().join("not-a-directory");
        fs::write(&parent_file, "occupied").expect("seed parent file");

        let store = FileNotificationCredentialStore::new(
            parent_file.join(NOTIFICATION_CREDENTIAL_STORE_FILE_NAME),
        );
        let error = store
            .upsert_route_credentials(
                "project-1",
                "route-1",
                NotificationRouteKind::Telegram,
                NotificationCredentialUpsertInput::Telegram {
                    bot_token: "bot-token".into(),
                    chat_id: "chat-id".into(),
                },
                "2026-04-20T20:44:00Z",
            )
            .expect_err("parent file should block app-local store writes");

        assert_eq!(
            error.code,
            "notification_adapter_credentials_directory_unavailable"
        );
    }
}
