use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::notifications::{NotificationAdapterError, NotificationRouteKind};

use super::{
    readiness::NotificationCredentialReadinessProjector,
    sql::{load_store, open_db, write_store},
    validation::{
        require_identifier, sanitize_upsert_credentials, NotificationCredentialUpsertInput,
    },
};

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

/// Backed by the global `cadence.db` database. Phase 2.3 swapped the JSON I/O for SQL writes
/// against `notification_credentials` and `notification_inbound_cursors`. The struct still owns a
/// path so the existing call sites remain untouched; that path is now the global database file.
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
        let connection = open_db(&self.path)?;
        load_store(&connection)
    }

    pub(crate) fn write_store_file(
        &self,
        store: &NotificationCredentialStoreFile,
    ) -> Result<(), NotificationAdapterError> {
        let mut connection = open_db(&self.path)?;
        write_store(&mut connection, store)
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
