pub mod openai_codex;
pub mod store;

pub use openai_codex::{
    cancel_openai_codex_flow, complete_openai_codex_flow, refresh_openai_codex_session,
    start_openai_codex_flow, OpenAiCodexAuthConfig, OpenAiCodexAuthSession, StartedOpenAiCodexFlow,
};
pub use store::{
    load_latest_openai_codex_session, load_openai_codex_session, persist_openai_codex_session,
    remove_openai_codex_session, StoredOpenAiCodexSession,
};

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::commands::RuntimeAuthPhase;

pub const OPENAI_CODEX_PROVIDER_ID: &str = "openai_codex";
pub const OPENAI_CODEX_AUTH_STORE_FILE_NAME: &str = "openai-codex-auth.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthDiagnostic {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeAuthStateSnapshot {
    pub provider_id: String,
    pub flow_id: String,
    pub session_id: Option<String>,
    pub account_id: Option<String>,
    pub phase: RuntimeAuthPhase,
    pub authorization_url: String,
    pub redirect_uri: String,
    pub callback_bound: bool,
    pub last_error_code: Option<String>,
    pub last_error: Option<AuthDiagnostic>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Error)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[error("{message}")]
pub struct AuthFlowError {
    pub code: String,
    pub phase: RuntimeAuthPhase,
    pub message: String,
    pub retryable: bool,
}

impl AuthFlowError {
    pub fn new(
        code: impl Into<String>,
        phase: RuntimeAuthPhase,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            code: code.into(),
            phase,
            message: message.into(),
            retryable,
        }
    }

    pub fn invalid_manual_input(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, RuntimeAuthPhase::AwaitingManualInput, message, false)
    }

    pub fn retryable(
        code: impl Into<String>,
        phase: RuntimeAuthPhase,
        message: impl Into<String>,
    ) -> Self {
        Self::new(code, phase, message, true)
    }

    pub fn terminal(
        code: impl Into<String>,
        phase: RuntimeAuthPhase,
        message: impl Into<String>,
    ) -> Self {
        Self::new(code, phase, message, false)
    }

    pub fn diagnostic(&self) -> AuthDiagnostic {
        AuthDiagnostic {
            code: self.code.clone(),
            message: self.message.clone(),
            retryable: self.retryable,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ActiveAuthFlowRegistry {
    inner: Arc<Mutex<HashMap<String, openai_codex::ActiveOpenAiCodexFlow>>>,
}

impl ActiveAuthFlowRegistry {
    pub fn insert(&self, flow: openai_codex::ActiveOpenAiCodexFlow) {
        self.inner
            .lock()
            .expect("active auth flow registry lock poisoned")
            .insert(flow.flow_id().to_owned(), flow);
    }

    pub fn with_flow<T>(
        &self,
        flow_id: &str,
        operation: impl FnOnce(&mut openai_codex::ActiveOpenAiCodexFlow) -> T,
    ) -> Option<T> {
        let mut flows = self
            .inner
            .lock()
            .expect("active auth flow registry lock poisoned");
        let flow = flows.get_mut(flow_id)?;
        Some(operation(flow))
    }

    pub fn snapshot(&self, flow_id: &str) -> Option<RuntimeAuthStateSnapshot> {
        self.inner
            .lock()
            .expect("active auth flow registry lock poisoned")
            .get(flow_id)
            .map(openai_codex::ActiveOpenAiCodexFlow::snapshot)
    }
}

pub fn now_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("rfc3339 timestamp formatting should succeed")
}
