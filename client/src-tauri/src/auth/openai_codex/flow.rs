use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime};
use url::Url;

use super::super::{now_timestamp, store, AuthDiagnostic, AuthFlowError, RuntimeAuthStateSnapshot};
use super::{
    callback::spawn_callback_listener,
    config::OpenAiCodexAuthConfig,
    jwt::extract_account_id,
    manual_input::parse_authorization_input,
    oauth_http::{exchange_authorization_code, refresh_access_token},
};
use crate::{
    commands::RuntimeAuthPhase,
    runtime::{
        bind_openai_callback_listener, openai_codex_provider, resolve_openai_callback_policy,
        OpenAiCallbackBindResult,
    },
    state::DesktopState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedOpenAiCodexFlow {
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
pub struct OpenAiCodexAuthSession {
    pub provider_id: String,
    pub session_id: String,
    pub account_id: String,
    pub expires_at: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ActiveOpenAiCodexFlow {
    project_id: String,
    profile_id: String,
    flow_id: String,
    verifier: String,
    expected_state: String,
    authorization_url: String,
    redirect_uri: String,
    callback_path: String,
    observation: Arc<Mutex<FlowObservation>>,
}

#[derive(Debug, Clone)]
struct FlowObservation {
    phase: RuntimeAuthPhase,
    callback_bound: bool,
    callback_code: Option<String>,
    session_id: Option<String>,
    account_id: Option<String>,
    last_error: Option<AuthDiagnostic>,
    updated_at: String,
    cancelled: bool,
}

impl ActiveOpenAiCodexFlow {
    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub(crate) fn flow_id(&self) -> &str {
        &self.flow_id
    }

    pub(crate) fn snapshot(&self) -> RuntimeAuthStateSnapshot {
        let observation = self
            .observation
            .lock()
            .expect("openai oauth flow lock poisoned");

        RuntimeAuthStateSnapshot {
            project_id: self.project_id.clone(),
            profile_id: self.profile_id.clone(),
            provider_id: openai_codex_provider().provider_id.into(),
            flow_id: self.flow_id.clone(),
            session_id: observation.session_id.clone(),
            account_id: observation.account_id.clone(),
            phase: observation.phase.clone(),
            authorization_url: self.authorization_url.clone(),
            redirect_uri: self.redirect_uri.clone(),
            callback_bound: observation.callback_bound,
            last_error_code: observation
                .last_error
                .as_ref()
                .map(|error| error.code.clone()),
            last_error: observation.last_error.clone(),
            updated_at: observation.updated_at.clone(),
        }
    }

    fn record_error(&self, error: &AuthFlowError) {
        let mut observation = self
            .observation
            .lock()
            .expect("openai oauth flow lock poisoned");
        observation.phase = error.phase.clone();
        observation.last_error = Some(error.diagnostic());
        observation.updated_at = now_timestamp();
    }

    fn set_phase(&self, phase: RuntimeAuthPhase) {
        let mut observation = self
            .observation
            .lock()
            .expect("openai oauth flow lock poisoned");
        observation.phase = phase;
        observation.updated_at = now_timestamp();
    }

    pub(super) fn set_callback_diagnostic(
        &self,
        diagnostic: AuthDiagnostic,
        phase: RuntimeAuthPhase,
    ) {
        let mut observation = self
            .observation
            .lock()
            .expect("openai oauth flow lock poisoned");
        observation.last_error = Some(diagnostic);
        observation.phase = phase;
        observation.updated_at = now_timestamp();
    }

    pub(super) fn store_callback_code(&self, code: String) {
        let mut observation = self
            .observation
            .lock()
            .expect("openai oauth flow lock poisoned");
        observation.callback_code = Some(code);
        observation.updated_at = now_timestamp();
    }

    fn take_callback_code(&self) -> Option<String> {
        let mut observation = self
            .observation
            .lock()
            .expect("openai oauth flow lock poisoned");
        let code = observation.callback_code.take();
        if code.is_some() {
            observation.updated_at = now_timestamp();
        }
        code
    }

    fn mark_authenticated(&self, session_id: String, account_id: String) {
        let mut observation = self
            .observation
            .lock()
            .expect("openai oauth flow lock poisoned");
        observation.phase = RuntimeAuthPhase::Authenticated;
        observation.session_id = Some(session_id);
        observation.account_id = Some(account_id);
        observation.last_error = None;
        observation.updated_at = now_timestamp();
    }

    fn mark_cancelled(&self) {
        let mut observation = self
            .observation
            .lock()
            .expect("openai oauth flow lock poisoned");
        observation.phase = RuntimeAuthPhase::Cancelled;
        observation.cancelled = true;
        observation.updated_at = now_timestamp();
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.observation
            .lock()
            .expect("openai oauth flow lock poisoned")
            .cancelled
    }

    pub(super) fn expected_state(&self) -> &str {
        &self.expected_state
    }

    pub(super) fn callback_path(&self) -> &str {
        &self.callback_path
    }
}

pub fn start_openai_codex_flow(
    state: &DesktopState,
    project_id: &str,
    profile_id: &str,
    config: OpenAiCodexAuthConfig,
    originator: Option<&str>,
) -> Result<StartedOpenAiCodexFlow, AuthFlowError> {
    let flow_id = random_hex(16)?;
    let expected_state = random_hex(16)?;
    let verifier = generate_pkce_verifier()?;
    let challenge = generate_pkce_challenge(&verifier);

    let callback_policy = resolve_openai_callback_policy(
        &config.callback_host,
        config.callback_port,
        &config.callback_path,
    )?;
    let bind_result = bind_openai_callback_listener(&callback_policy)?;
    let (listener, callback_bound, redirect_uri, phase, last_error) = match bind_result {
        OpenAiCallbackBindResult::Bound {
            listener,
            redirect_uri,
        } => (
            Some(listener),
            true,
            redirect_uri,
            RuntimeAuthPhase::AwaitingBrowserCallback,
            None,
        ),
        OpenAiCallbackBindResult::ManualFallback {
            redirect_uri,
            diagnostic,
        } => (
            None,
            false,
            redirect_uri,
            RuntimeAuthPhase::AwaitingManualInput,
            Some(diagnostic),
        ),
    };

    let authorization_url = build_authorization_url(
        &config,
        &expected_state,
        &challenge,
        &redirect_uri,
        originator,
    )?;

    let updated_at = now_timestamp();
    let last_error_code = last_error
        .as_ref()
        .map(|diagnostic| diagnostic.code.clone());
    let flow = ActiveOpenAiCodexFlow {
        project_id: project_id.to_owned(),
        profile_id: profile_id.to_owned(),
        flow_id: flow_id.clone(),
        verifier,
        expected_state: expected_state.clone(),
        authorization_url: authorization_url.clone(),
        redirect_uri: redirect_uri.clone(),
        callback_path: callback_policy.path,
        observation: Arc::new(Mutex::new(FlowObservation {
            phase: phase.clone(),
            callback_bound,
            callback_code: None,
            session_id: None,
            account_id: None,
            last_error,
            updated_at: updated_at.clone(),
            cancelled: false,
        })),
    };

    if let Some(listener) = listener {
        spawn_callback_listener(listener, flow.clone());
    }

    state.active_auth_flows().insert(flow.into());

    Ok(StartedOpenAiCodexFlow {
        flow_id,
        authorization_url,
        redirect_uri,
        expected_state,
        phase,
        callback_bound,
        last_error_code,
        updated_at,
    })
}

pub fn complete_openai_codex_flow<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    flow_id: &str,
    manual_input: Option<&str>,
    config: &OpenAiCodexAuthConfig,
) -> Result<OpenAiCodexAuthSession, AuthFlowError> {
    let selected_flow = state.active_auth_flows().flow(flow_id).ok_or_else(|| {
        AuthFlowError::terminal(
            "auth_flow_not_found",
            RuntimeAuthPhase::Failed,
            format!("Cadence could not find the active OpenAI auth flow `{flow_id}`."),
        )
    })?;
    let super::super::ActiveAuthFlow::OpenAiCodex(selected_flow) = selected_flow;

    if selected_flow.is_cancelled() {
        return Err(AuthFlowError::terminal(
            "auth_flow_cancelled",
            RuntimeAuthPhase::Cancelled,
            "The OpenAI login flow was cancelled before completion.",
        ));
    }

    store::ensure_openai_profile_target(
        app,
        state,
        selected_flow.profile_id(),
        RuntimeAuthPhase::Failed,
        "complete OpenAI login",
    )
    .inspect_err(|error| selected_flow.record_error(error))?;

    let code = if let Some(callback_code) = selected_flow.take_callback_code() {
        callback_code
    } else if let Some(input) = manual_input {
        let parsed = parse_authorization_input(input)?;
        if let Some(returned_state) = parsed.state {
            if returned_state != selected_flow.expected_state {
                let error = AuthFlowError::invalid_manual_input(
                    "callback_state_mismatch",
                    "The pasted OpenAI redirect did not match the active login state.",
                );
                selected_flow.record_error(&error);
                return Err(error);
            }
        }
        selected_flow.set_phase(RuntimeAuthPhase::AwaitingManualInput);
        parsed.code
    } else {
        let error = AuthFlowError::retryable(
            "authorization_code_pending",
            RuntimeAuthPhase::AwaitingManualInput,
            "OpenAI login is still waiting for either the browser callback or a pasted redirect URL.",
        );
        selected_flow.record_error(&error);
        return Err(error);
    };

    selected_flow.set_phase(RuntimeAuthPhase::ExchangingCode);
    let token_response = exchange_authorization_code(
        &code,
        &selected_flow.verifier,
        &selected_flow.redirect_uri,
        config,
    )
    .inspect_err(|error| selected_flow.record_error(error))?;
    let account_id = extract_account_id(&token_response.access_token).inspect_err(|error| {
        selected_flow.record_error(error);
    })?;

    let session = OpenAiCodexAuthSession {
        provider_id: openai_codex_provider().provider_id.into(),
        session_id: random_hex(16)?,
        account_id: account_id.clone(),
        expires_at: token_response.expires_at,
        updated_at: now_timestamp(),
    };
    store::ensure_openai_profile_target(
        app,
        state,
        selected_flow.profile_id(),
        RuntimeAuthPhase::Failed,
        "sync OpenAI auth onto the selected provider profile",
    )
    .inspect_err(|error| selected_flow.record_error(error))?;
    let auth_store_path = state.auth_store_file_for_provider(app, openai_codex_provider())?;
    let stored_session = store::StoredOpenAiCodexSession {
        provider_id: session.provider_id.clone(),
        session_id: session.session_id.clone(),
        account_id: session.account_id.clone(),
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        expires_at: session.expires_at,
        updated_at: session.updated_at.clone(),
    };
    store::persist_openai_codex_session(&auth_store_path, stored_session.clone())
        .inspect_err(|error| selected_flow.record_error(error))?;
    store::sync_openai_profile_link(
        app,
        state,
        Some(selected_flow.profile_id()),
        Some(&stored_session),
    )
    .inspect_err(|error| selected_flow.record_error(error))?;

    selected_flow.mark_authenticated(session.session_id.clone(), session.account_id.clone());
    Ok(session)
}

pub fn refresh_openai_codex_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    account_id: &str,
    config: &OpenAiCodexAuthConfig,
) -> Result<OpenAiCodexAuthSession, AuthFlowError> {
    if account_id.trim().is_empty() {
        return Err(AuthFlowError::invalid_manual_input(
            "empty_account_id",
            "The OpenAI account id must not be empty when refreshing a runtime session.",
        ));
    }

    let auth_store_path = state.auth_store_file_for_provider(app, openai_codex_provider())?;
    let stored_session = store::load_openai_codex_session(&auth_store_path, account_id)?
        .ok_or_else(|| {
            AuthFlowError::terminal(
                "auth_session_not_found",
                RuntimeAuthPhase::Refreshing,
                format!(
                    "Cadence does not have an app-local OpenAI auth session for account `{account_id}`."
                ),
            )
        })?;

    let refreshed = refresh_access_token(&stored_session.refresh_token, config)?;
    let refreshed_account_id = extract_account_id(&refreshed.access_token)?;
    let updated_session = OpenAiCodexAuthSession {
        provider_id: openai_codex_provider().provider_id.into(),
        session_id: stored_session.session_id.clone(),
        account_id: refreshed_account_id.clone(),
        expires_at: refreshed.expires_at,
        updated_at: now_timestamp(),
    };

    let stored_session = store::StoredOpenAiCodexSession {
        provider_id: updated_session.provider_id.clone(),
        session_id: updated_session.session_id.clone(),
        account_id: refreshed_account_id,
        access_token: refreshed.access_token,
        refresh_token: refreshed.refresh_token,
        expires_at: updated_session.expires_at,
        updated_at: updated_session.updated_at.clone(),
    };
    store::persist_openai_codex_session(&auth_store_path, stored_session.clone())?;
    store::sync_openai_profile_link(app, state, None, Some(&stored_session))?;

    Ok(updated_session)
}

pub fn cancel_openai_codex_flow(
    state: &DesktopState,
    flow_id: &str,
) -> Result<RuntimeAuthStateSnapshot, AuthFlowError> {
    state
        .active_auth_flows()
        .with_flow(
            flow_id,
            |flow| -> Result<RuntimeAuthStateSnapshot, AuthFlowError> {
                let super::super::ActiveAuthFlow::OpenAiCodex(flow) = flow;
                flow.mark_cancelled();
                Ok(flow.snapshot())
            },
        )
        .transpose()?
        .ok_or_else(|| {
            AuthFlowError::terminal(
                "auth_flow_not_found",
                RuntimeAuthPhase::Failed,
                format!("Cadence could not find the active OpenAI auth flow `{flow_id}`."),
            )
        })
}

fn build_authorization_url(
    config: &OpenAiCodexAuthConfig,
    state: &str,
    challenge: &str,
    redirect_uri: &str,
    originator: Option<&str>,
) -> Result<String, AuthFlowError> {
    let mut url = Url::parse(&config.authorize_url).map_err(|error| {
        AuthFlowError::terminal(
            "authorize_url_invalid",
            RuntimeAuthPhase::Starting,
            format!("Cadence could not parse the OpenAI authorize URL: {error}"),
        )
    })?;

    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", &config.scope)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", originator.unwrap_or(&config.originator));

    Ok(url.to_string())
}

fn generate_pkce_verifier() -> Result<String, AuthFlowError> {
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn generate_pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn random_hex(bytes: usize) -> Result<String, AuthFlowError> {
    let mut buffer = vec![0_u8; bytes];
    rand::thread_rng().fill_bytes(&mut buffer);
    Ok(buffer.iter().map(|byte| format!("{byte:02x}")).collect())
}
