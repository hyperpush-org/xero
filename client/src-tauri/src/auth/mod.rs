pub mod openai_codex;
pub mod openrouter;
pub mod store;

pub use crate::runtime::{
    openai_codex_provider, openrouter_provider, ResolvedRuntimeProvider, RuntimeProvider,
    OPENAI_CODEX_AUTH_STORE_FILE_NAME, OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID,
};
pub use openai_codex::{
    cancel_openai_codex_flow, complete_openai_codex_flow, refresh_openai_codex_session,
    start_openai_codex_flow, OpenAiCodexAuthConfig, OpenAiCodexAuthSession, StartedOpenAiCodexFlow,
};
pub(crate) use openrouter::{
    bind_openrouter_runtime_session, reconcile_openrouter_runtime_session,
};
pub use openrouter::{
    OpenRouterAuthConfig, OpenRouterBindOutcome, OpenRouterReconcileOutcome,
    OpenRouterRuntimeSessionBinding,
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
use tauri::{AppHandle, Runtime};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{commands::RuntimeAuthPhase, state::DesktopState};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedRuntimeAuthFlow {
    pub provider_id: String,
    pub flow_id: String,
    pub authorization_url: String,
    pub redirect_uri: String,
    pub expected_state: String,
    pub phase: RuntimeAuthPhase,
    pub callback_bound: bool,
    pub last_error_code: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeAuthSession {
    pub provider_id: String,
    pub session_id: String,
    pub account_id: String,
    pub expires_at: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub enum ActiveAuthFlow {
    OpenAiCodex(openai_codex::ActiveOpenAiCodexFlow),
}

impl ActiveAuthFlow {
    pub fn provider(&self) -> ResolvedRuntimeProvider {
        match self {
            Self::OpenAiCodex(_) => openai_codex_provider(),
        }
    }

    pub fn flow_id(&self) -> &str {
        match self {
            Self::OpenAiCodex(flow) => flow.flow_id(),
        }
    }

    pub fn snapshot(&self) -> RuntimeAuthStateSnapshot {
        match self {
            Self::OpenAiCodex(flow) => flow.snapshot(),
        }
    }
}

impl From<openai_codex::ActiveOpenAiCodexFlow> for ActiveAuthFlow {
    fn from(flow: openai_codex::ActiveOpenAiCodexFlow) -> Self {
        Self::OpenAiCodex(flow)
    }
}

impl From<StartedOpenAiCodexFlow> for StartedRuntimeAuthFlow {
    fn from(flow: StartedOpenAiCodexFlow) -> Self {
        Self {
            provider_id: openai_codex_provider().provider_id.into(),
            flow_id: flow.flow_id,
            authorization_url: flow.authorization_url,
            redirect_uri: flow.redirect_uri,
            expected_state: flow.expected_state,
            phase: flow.phase,
            callback_bound: flow.callback_bound,
            last_error_code: flow.last_error_code,
            updated_at: flow.updated_at,
        }
    }
}

impl From<OpenAiCodexAuthSession> for RuntimeAuthSession {
    fn from(session: OpenAiCodexAuthSession) -> Self {
        Self {
            provider_id: session.provider_id,
            session_id: session.session_id,
            account_id: session.account_id,
            expires_at: session.expires_at,
            updated_at: session.updated_at,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ActiveAuthFlowRegistry {
    inner: Arc<Mutex<HashMap<String, ActiveAuthFlow>>>,
}

impl ActiveAuthFlowRegistry {
    pub fn insert(&self, flow: ActiveAuthFlow) {
        self.inner
            .lock()
            .expect("active auth flow registry lock poisoned")
            .insert(flow.flow_id().to_owned(), flow);
    }

    pub fn flow(&self, flow_id: &str) -> Option<ActiveAuthFlow> {
        self.inner
            .lock()
            .expect("active auth flow registry lock poisoned")
            .get(flow_id)
            .cloned()
    }

    pub fn with_flow<T>(
        &self,
        flow_id: &str,
        operation: impl FnOnce(&mut ActiveAuthFlow) -> T,
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
            .map(ActiveAuthFlow::snapshot)
    }
}

pub fn start_provider_auth_flow(
    state: &DesktopState,
    provider: RuntimeProvider,
    originator: Option<&str>,
) -> Result<StartedRuntimeAuthFlow, AuthFlowError> {
    match provider {
        RuntimeProvider::OpenAiCodex => {
            openai_codex::start_openai_codex_flow(state, state.openai_auth_config(), originator)
                .map(Into::into)
        }
        RuntimeProvider::OpenRouter => Err(AuthFlowError::terminal(
            "auth_flow_unavailable",
            RuntimeAuthPhase::Failed,
            "Cadence binds OpenRouter runtime sessions from the saved app-global API key and does not support a browser login flow for that provider.",
        )),
    }
}

pub fn complete_provider_auth_flow<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: RuntimeProvider,
    flow_id: &str,
    manual_input: Option<&str>,
) -> Result<RuntimeAuthSession, AuthFlowError> {
    let requested_provider = provider.resolve();
    let active_flow = state
        .active_auth_flows()
        .flow(flow_id)
        .ok_or_else(|| auth_flow_not_found_error(flow_id, requested_provider))?;
    let actual_provider = active_flow.provider();
    if actual_provider != requested_provider {
        return Err(auth_flow_provider_mismatch_error(
            flow_id,
            requested_provider,
            actual_provider,
        ));
    }

    match provider {
        RuntimeProvider::OpenAiCodex => openai_codex::complete_openai_codex_flow(
            app,
            state,
            flow_id,
            manual_input,
            &state.openai_auth_config(),
        )
        .map(Into::into),
        RuntimeProvider::OpenRouter => Err(AuthFlowError::terminal(
            "auth_flow_unavailable",
            RuntimeAuthPhase::Failed,
            "Cadence does not complete an OpenRouter browser login flow because OpenRouter runtime sessions bind from the saved app-global API key instead.",
        )),
    }
}

pub fn refresh_provider_auth_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider: RuntimeProvider,
    account_id: &str,
) -> Result<RuntimeAuthSession, AuthFlowError> {
    match provider {
        RuntimeProvider::OpenAiCodex => openai_codex::refresh_openai_codex_session(
            app,
            state,
            account_id,
            &state.openai_auth_config(),
        )
        .map(Into::into),
        RuntimeProvider::OpenRouter => Err(AuthFlowError::terminal(
            "auth_refresh_unavailable",
            RuntimeAuthPhase::Failed,
            "Cadence does not refresh OpenRouter runtime sessions through a browser auth store. Rebind from the saved app-global API key instead.",
        )),
    }
}

fn auth_flow_not_found_error(flow_id: &str, provider: ResolvedRuntimeProvider) -> AuthFlowError {
    AuthFlowError::terminal(
        "auth_flow_not_found",
        RuntimeAuthPhase::Failed,
        format!(
            "Cadence could not find the active {} auth flow `{flow_id}`.",
            provider.provider_id
        ),
    )
}

fn auth_flow_provider_mismatch_error(
    flow_id: &str,
    requested_provider: ResolvedRuntimeProvider,
    actual_provider: ResolvedRuntimeProvider,
) -> AuthFlowError {
    AuthFlowError::terminal(
        "auth_flow_provider_mismatch",
        RuntimeAuthPhase::Failed,
        format!(
            "Cadence rejected auth flow `{flow_id}` because it belongs to provider `{}` instead of `{}`. Start a fresh login.",
            actual_provider.provider_id, requested_provider.provider_id
        ),
    )
}

pub fn now_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("rfc3339 timestamp formatting should succeed")
}
