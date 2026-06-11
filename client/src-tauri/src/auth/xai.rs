use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use reqwest::blocking::{Client, Response};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime};
use url::Url;

use super::{auth_flow_error_from_command_error, now_timestamp, AuthDiagnostic, AuthFlowError};
use crate::{
    commands::{CommandError, RuntimeAuthPhase},
    global_db::open_global_database,
    provider_credentials::{
        delete_provider_credential, load_provider_credential,
        load_provider_credentials_view_or_default, upsert_provider_credential,
        ProviderCredentialKind, ProviderCredentialLink, ProviderCredentialRecord,
        ProviderCredentialsView, XAI_DEFAULT_PROFILE_ID,
    },
    runtime::{
        bind_openai_callback_listener, resolve_openai_callback_policy, OpenAiCallbackBindResult,
        XAI_DEFAULT_MODEL_ID, XAI_PROVIDER_ID,
    },
    state::DesktopState,
};

const DEFAULT_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const DEFAULT_DISCOVERY_URL: &str = "https://auth.x.ai/.well-known/openid-configuration";
const DEFAULT_AUTHORIZE_URL: &str = "https://auth.x.ai/oauth2/authorize";
const DEFAULT_TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
const DEFAULT_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
const DEFAULT_CALLBACK_HOST: &str = "127.0.0.1";
const DEFAULT_CALLBACK_PORT: u16 = 56121;
const DEFAULT_CALLBACK_PATH: &str = "/callback";
const DEFAULT_REFERRER: &str = "xero";
const CALLBACK_CORS_ALLOWLIST: &[&str] = &["auth.x.ai", "accounts.x.ai"];
const BUILD_XAI_OAUTH_CLIENT_ID: Option<&str> = option_env!("XERO_XAI_OAUTH_CLIENT_ID");
const LEGACY_BUILD_XAI_OAUTH_CLIENT_ID: Option<&str> = option_env!("XAI_OAUTH_CLIENT_ID");

#[derive(Debug, Clone)]
pub struct XaiAuthConfig {
    pub client_id: String,
    pub discovery_url: String,
    pub authorize_url: String,
    pub token_url: String,
    pub callback_host: String,
    pub callback_port: u16,
    pub callback_path: String,
    pub scope: String,
    pub timeout: Duration,
}

impl Default for XaiAuthConfig {
    fn default() -> Self {
        Self {
            client_id: configured_xai_oauth_client_id(),
            discovery_url: DEFAULT_DISCOVERY_URL.into(),
            authorize_url: DEFAULT_AUTHORIZE_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
            callback_host: DEFAULT_CALLBACK_HOST.into(),
            callback_port: DEFAULT_CALLBACK_PORT,
            callback_path: DEFAULT_CALLBACK_PATH.into(),
            scope: DEFAULT_SCOPE.into(),
            timeout: Duration::from_secs(30),
        }
    }
}

impl XaiAuthConfig {
    pub fn for_platform() -> Self {
        Self::default()
    }

    fn http_client(&self) -> Result<Client, AuthFlowError> {
        Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|error| {
                AuthFlowError::terminal(
                    "oauth_http_client_unavailable",
                    RuntimeAuthPhase::Failed,
                    format!("Xero could not build the xAI OAuth HTTP client: {error}"),
                )
            })
    }

    fn require_client_id(&self, phase: RuntimeAuthPhase) -> Result<(), AuthFlowError> {
        let client_id = self.client_id.trim();
        if client_id.is_empty() {
            return Err(AuthFlowError::terminal(
                "xai_oauth_client_unconfigured",
                phase,
                "This Xero build does not include xAI OAuth sign-in. Install a Xero build with xAI OAuth enabled.",
            ));
        }
        if looks_like_x_developer_portal_client_id(client_id) {
            return Err(AuthFlowError::terminal(
                "xai_oauth_client_wrong_issuer",
                phase,
                "The configured xAI OAuth client id looks like an X Developer Portal OAuth client id. auth.x.ai does not accept ordinary X Developer Portal client ids; Xero needs an xAI-issued OAuth client id.",
            ));
        }
        Ok(())
    }
}

fn configured_xai_oauth_client_id() -> String {
    std::env::var("XERO_XAI_OAUTH_CLIENT_ID")
        .ok()
        .or_else(|| std::env::var("XAI_OAUTH_CLIENT_ID").ok())
        .or_else(|| BUILD_XAI_OAUTH_CLIENT_ID.map(str::to_owned))
        .or_else(|| LEGACY_BUILD_XAI_OAUTH_CLIENT_ID.map(str::to_owned))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .filter(|value| !looks_like_x_developer_portal_client_id(value))
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_owned())
}

fn looks_like_x_developer_portal_client_id(value: &str) -> bool {
    let value = value.trim();
    value.len() >= 32
        && !value.contains('-')
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedXaiFlow {
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
pub struct XaiAuthSession {
    pub provider_id: String,
    pub session_id: String,
    pub account_id: String,
    pub expires_at: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredXaiSession {
    pub provider_id: String,
    pub session_id: String,
    pub account_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ActiveXaiFlow {
    scope_id: String,
    profile_id: String,
    flow_id: String,
    verifier: String,
    challenge: String,
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

impl ActiveXaiFlow {
    pub(crate) fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub(crate) fn flow_id(&self) -> &str {
        &self.flow_id
    }

    pub(crate) fn snapshot(&self) -> super::RuntimeAuthStateSnapshot {
        let observation = self
            .observation
            .lock()
            .expect("xai oauth flow lock poisoned");
        super::RuntimeAuthStateSnapshot {
            scope_id: self.scope_id.clone(),
            profile_id: self.profile_id.clone(),
            provider_id: XAI_PROVIDER_ID.into(),
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
            .expect("xai oauth flow lock poisoned");
        observation.phase = error.phase.clone();
        observation.last_error = Some(error.diagnostic());
        observation.updated_at = now_timestamp();
    }

    fn set_phase(&self, phase: RuntimeAuthPhase) {
        let mut observation = self
            .observation
            .lock()
            .expect("xai oauth flow lock poisoned");
        observation.phase = phase;
        observation.updated_at = now_timestamp();
    }

    fn mark_authenticated(&self, session_id: String, account_id: String) {
        let mut observation = self
            .observation
            .lock()
            .expect("xai oauth flow lock poisoned");
        observation.phase = RuntimeAuthPhase::Authenticated;
        observation.session_id = Some(session_id);
        observation.account_id = Some(account_id);
        observation.last_error = None;
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
            .expect("xai oauth flow lock poisoned");
        observation.last_error = Some(diagnostic);
        observation.phase = phase;
        observation.updated_at = now_timestamp();
    }

    pub(super) fn store_callback_code(&self, code: String) {
        let mut observation = self
            .observation
            .lock()
            .expect("xai oauth flow lock poisoned");
        observation.callback_code = Some(code);
        observation.updated_at = now_timestamp();
    }

    fn take_callback_code(&self) -> Option<String> {
        let mut observation = self
            .observation
            .lock()
            .expect("xai oauth flow lock poisoned");
        let code = observation.callback_code.take();
        if code.is_some() {
            observation.updated_at = now_timestamp();
        }
        code
    }

    fn mark_cancelled(&self) {
        let mut observation = self
            .observation
            .lock()
            .expect("xai oauth flow lock poisoned");
        observation.phase = RuntimeAuthPhase::Cancelled;
        observation.cancelled = true;
        observation.updated_at = now_timestamp();
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.observation
            .lock()
            .expect("xai oauth flow lock poisoned")
            .cancelled
    }

    pub(super) fn expected_state(&self) -> &str {
        &self.expected_state
    }

    pub(super) fn callback_path(&self) -> &str {
        &self.callback_path
    }
}

pub fn start_xai_flow(
    state: &DesktopState,
    scope_id: &str,
    profile_id: &str,
    config: XaiAuthConfig,
) -> Result<StartedXaiFlow, AuthFlowError> {
    config.require_client_id(RuntimeAuthPhase::Starting)?;
    let endpoints = resolve_oauth_endpoints(&config, RuntimeAuthPhase::Starting)?;
    let flow_id = random_hex(16)?;
    let expected_state = random_hex(16)?;
    let nonce = random_hex(16)?;
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
        &endpoints.authorize_url,
        &config,
        &expected_state,
        &nonce,
        &challenge,
        &redirect_uri,
    )?;
    let updated_at = now_timestamp();
    let last_error_code = last_error
        .as_ref()
        .map(|diagnostic| diagnostic.code.clone());
    let flow = ActiveXaiFlow {
        scope_id: scope_id.to_owned(),
        profile_id: profile_id.to_owned(),
        flow_id: flow_id.clone(),
        verifier,
        challenge,
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
    Ok(StartedXaiFlow {
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

pub fn complete_xai_flow<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    flow_id: &str,
    manual_input: Option<&str>,
    config: &XaiAuthConfig,
) -> Result<XaiAuthSession, AuthFlowError> {
    let selected_flow = state.active_auth_flows().flow(flow_id).ok_or_else(|| {
        AuthFlowError::terminal(
            "auth_flow_not_found",
            RuntimeAuthPhase::Failed,
            format!("Xero could not find the active xAI auth flow `{flow_id}`."),
        )
    })?;
    let super::ActiveAuthFlow::Xai(selected_flow) = selected_flow else {
        return Err(AuthFlowError::terminal(
            "auth_flow_provider_mismatch",
            RuntimeAuthPhase::Failed,
            format!("Xero rejected auth flow `{flow_id}` because it is not an xAI login."),
        ));
    };

    if selected_flow.is_cancelled() {
        return Err(AuthFlowError::terminal(
            "auth_flow_cancelled",
            RuntimeAuthPhase::Cancelled,
            "The xAI login flow was cancelled before completion.",
        ));
    }

    ensure_xai_profile_target(
        app,
        state,
        selected_flow.profile_id(),
        RuntimeAuthPhase::Failed,
        "complete xAI login",
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
                    "The pasted xAI redirect did not match the active login state.",
                );
                selected_flow.record_error(&error);
                return Err(error);
            }
        }
        selected_flow.set_phase(RuntimeAuthPhase::AwaitingManualInput);
        parsed.code
    } else {
        return Err(AuthFlowError::retryable(
            "authorization_code_pending",
            RuntimeAuthPhase::AwaitingBrowserCallback,
            "xAI login is still waiting for either the browser callback or a pasted redirect URL.",
        ));
    };

    selected_flow.set_phase(RuntimeAuthPhase::ExchangingCode);
    config.require_client_id(RuntimeAuthPhase::ExchangingCode)?;
    let endpoints = resolve_oauth_endpoints(config, RuntimeAuthPhase::ExchangingCode)?;
    let token_response = exchange_authorization_code(
        &code,
        &selected_flow.verifier,
        &selected_flow.challenge,
        &selected_flow.redirect_uri,
        &endpoints,
        config,
    )
    .inspect_err(|error| selected_flow.record_error(error))?;
    let account_id = extract_xai_account_id(&token_response)
        .inspect_err(|error| selected_flow.record_error(error))?;

    let session = XaiAuthSession {
        provider_id: XAI_PROVIDER_ID.into(),
        session_id: random_hex(16)?,
        account_id: account_id.clone(),
        expires_at: token_response.expires_at,
        updated_at: now_timestamp(),
    };
    let stored_session = StoredXaiSession {
        provider_id: session.provider_id.clone(),
        session_id: session.session_id.clone(),
        account_id: session.account_id.clone(),
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token,
        expires_at: session.expires_at,
        updated_at: session.updated_at.clone(),
    };
    persist_xai_session_path(
        &state
            .global_db_path(app)
            .map_err(auth_flow_error_from_command_error)?,
        &stored_session,
    )
    .inspect_err(|error| selected_flow.record_error(error))?;

    selected_flow.mark_authenticated(session.session_id.clone(), session.account_id.clone());
    Ok(session)
}

pub fn refresh_xai_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    account_id: &str,
    config: &XaiAuthConfig,
) -> Result<XaiAuthSession, AuthFlowError> {
    if account_id.trim().is_empty() {
        return Err(AuthFlowError::invalid_manual_input(
            "empty_account_id",
            "The xAI account id must not be empty when refreshing a runtime session.",
        ));
    }

    config.require_client_id(RuntimeAuthPhase::Refreshing)?;
    let endpoints = resolve_oauth_endpoints(config, RuntimeAuthPhase::Refreshing)?;
    let auth_store_path = state
        .global_db_path(app)
        .map_err(auth_flow_error_from_command_error)?;
    let stored_session = load_xai_session(&auth_store_path, account_id)?.ok_or_else(|| {
        AuthFlowError::terminal(
            "auth_session_not_found",
            RuntimeAuthPhase::Refreshing,
            format!("Xero does not have an app-local xAI auth session for account `{account_id}`."),
        )
    })?;

    let refreshed = refresh_access_token(&stored_session.refresh_token, &endpoints, config)?;
    let refreshed_account_id = extract_xai_account_id(&refreshed)?;
    let updated_session = XaiAuthSession {
        provider_id: XAI_PROVIDER_ID.into(),
        session_id: stored_session.session_id.clone(),
        account_id: refreshed_account_id.clone(),
        expires_at: refreshed.expires_at,
        updated_at: now_timestamp(),
    };
    let stored_session = StoredXaiSession {
        provider_id: updated_session.provider_id.clone(),
        session_id: updated_session.session_id.clone(),
        account_id: refreshed_account_id,
        access_token: refreshed.access_token,
        refresh_token: refreshed.refresh_token,
        expires_at: updated_session.expires_at,
        updated_at: updated_session.updated_at.clone(),
    };
    persist_xai_session_path(&auth_store_path, &stored_session)?;

    Ok(updated_session)
}

pub fn cancel_xai_flow(
    state: &DesktopState,
    flow_id: &str,
) -> Result<super::RuntimeAuthStateSnapshot, AuthFlowError> {
    state
        .active_auth_flows()
        .with_flow(
            flow_id,
            |flow| -> Result<super::RuntimeAuthStateSnapshot, AuthFlowError> {
                let super::ActiveAuthFlow::Xai(flow) = flow else {
                    return Err(AuthFlowError::terminal(
                        "auth_flow_provider_mismatch",
                        RuntimeAuthPhase::Failed,
                        format!(
                            "Xero rejected auth flow `{flow_id}` because it is not an xAI login."
                        ),
                    ));
                };
                flow.mark_cancelled();
                Ok(flow.snapshot())
            },
        )
        .transpose()?
        .ok_or_else(|| {
            AuthFlowError::terminal(
                "auth_flow_not_found",
                RuntimeAuthPhase::Failed,
                format!("Xero could not find the active xAI auth flow `{flow_id}`."),
            )
        })
}

pub fn ensure_xai_profile_target<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile_id: &str,
    phase: RuntimeAuthPhase,
    action: &str,
) -> Result<(), AuthFlowError> {
    let view = load_provider_credentials_view(app, state)?;
    validate_target_xai_profile(&view, profile_id, phase, action)
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
    load_provider_credentials_view_or_default(&connection).map_err(map_provider_credentials_error)
}

fn validate_target_xai_profile(
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

    let Some(profile) = view.profile(profile_id) else {
        if profile_id == XAI_DEFAULT_PROFILE_ID {
            return Ok(());
        }
        return Err(AuthFlowError::terminal(
            "provider_profile_missing",
            phase,
            format!(
                "Xero rejected {action} because provider profile `{profile_id}` was not found. Select a different xAI credential."
            ),
        ));
    };

    if profile.provider_id != XAI_PROVIDER_ID {
        return Err(AuthFlowError::terminal(
            "provider_profile_provider_mismatch",
            phase,
            format!(
                "Xero rejected {action} because provider profile `{profile_id}` belongs to provider `{}` instead of `{XAI_PROVIDER_ID}`. Select an xAI credential.",
                profile.provider_id
            ),
        ));
    }

    Ok(())
}

pub fn load_xai_session(
    path: &Path,
    account_id: &str,
) -> Result<Option<StoredXaiSession>, AuthFlowError> {
    let connection = open_global_database(path).map_err(map_command_error_to_auth_error)?;
    let record = load_provider_credential(&connection, XAI_PROVIDER_ID)
        .map_err(map_provider_credentials_error)?;
    Ok(record.and_then(|record| {
        stored_xai_session_from_record(record).filter(|session| session.account_id == account_id)
    }))
}

pub fn load_latest_xai_session(path: &Path) -> Result<Option<StoredXaiSession>, AuthFlowError> {
    let connection = open_global_database(path).map_err(map_command_error_to_auth_error)?;
    let record = load_provider_credential(&connection, XAI_PROVIDER_ID)
        .map_err(map_provider_credentials_error)?;
    Ok(record.and_then(stored_xai_session_from_record))
}

pub fn load_xai_session_for_profile_link(
    path: &Path,
    link: &ProviderCredentialLink,
) -> Result<Option<StoredXaiSession>, AuthFlowError> {
    let ProviderCredentialLink::Xai {
        account_id,
        session_id,
        ..
    } = link
    else {
        return Err(AuthFlowError::terminal(
            "provider_profiles_invalid",
            RuntimeAuthPhase::Failed,
            "Xero rejected the active xAI provider profile because it referenced a non-xAI credential link.",
        ));
    };

    let session = load_latest_xai_session(path)?;
    Ok(session
        .filter(|stored| stored.account_id == *account_id || stored.session_id == *session_id))
}

pub fn persist_xai_session_path(
    path: &Path,
    session: &StoredXaiSession,
) -> Result<(), AuthFlowError> {
    let connection = open_global_database(path).map_err(map_command_error_to_auth_error)?;
    upsert_provider_credential(
        &connection,
        &ProviderCredentialRecord {
            provider_id: XAI_PROVIDER_ID.into(),
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
            default_model_id: Some(XAI_DEFAULT_MODEL_ID.into()),
            updated_at: session.updated_at.clone(),
        },
    )
    .map_err(map_provider_credentials_error)
}

pub fn remove_xai_session(path: &Path) -> Result<(), AuthFlowError> {
    let connection = open_global_database(path).map_err(map_command_error_to_auth_error)?;
    delete_provider_credential(&connection, XAI_PROVIDER_ID).map_err(map_provider_credentials_error)
}

fn stored_xai_session_from_record(record: ProviderCredentialRecord) -> Option<StoredXaiSession> {
    if record.provider_id != XAI_PROVIDER_ID || record.kind != ProviderCredentialKind::OAuthSession
    {
        return None;
    }
    Some(StoredXaiSession {
        provider_id: record.provider_id,
        session_id: record.oauth_session_id?,
        account_id: record.oauth_account_id?,
        access_token: record.oauth_access_token?,
        refresh_token: record.oauth_refresh_token?,
        expires_at: record.oauth_expires_at?,
        updated_at: record.updated_at,
    })
}

#[derive(Debug, Clone)]
struct OAuthEndpoints {
    authorize_url: String,
    token_url: String,
}

#[derive(Debug, Deserialize)]
struct DiscoveryDocument {
    authorization_endpoint: Option<String>,
    token_endpoint: Option<String>,
}

fn resolve_oauth_endpoints(
    config: &XaiAuthConfig,
    phase: RuntimeAuthPhase,
) -> Result<OAuthEndpoints, AuthFlowError> {
    let client = config.http_client()?;
    let discovered = client
        .get(&config.discovery_url)
        .send()
        .ok()
        .and_then(|response| response.error_for_status().ok())
        .and_then(|response| response.json::<DiscoveryDocument>().ok());

    let authorize_url = discovered
        .as_ref()
        .and_then(|doc| doc.authorization_endpoint.clone())
        .unwrap_or_else(|| config.authorize_url.clone());
    let token_url = discovered
        .as_ref()
        .and_then(|doc| doc.token_endpoint.clone())
        .unwrap_or_else(|| config.token_url.clone());
    Url::parse(&authorize_url).map_err(|error| {
        AuthFlowError::terminal(
            "xai_authorize_url_invalid",
            phase.clone(),
            format!("Xero could not parse the xAI authorize URL: {error}"),
        )
    })?;
    Url::parse(&token_url).map_err(|error| {
        AuthFlowError::terminal(
            "xai_token_url_invalid",
            phase,
            format!("Xero could not parse the xAI token URL: {error}"),
        )
    })?;

    Ok(OAuthEndpoints {
        authorize_url,
        token_url,
    })
}

fn build_authorization_url(
    authorize_url: &str,
    config: &XaiAuthConfig,
    state: &str,
    nonce: &str,
    challenge: &str,
    redirect_uri: &str,
) -> Result<String, AuthFlowError> {
    let mut url = Url::parse(authorize_url).map_err(|error| {
        AuthFlowError::terminal(
            "authorize_url_invalid",
            RuntimeAuthPhase::Starting,
            format!("Xero could not parse the xAI authorize URL: {error}"),
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
        .append_pair("nonce", nonce)
        .append_pair("plan", "generic")
        .append_pair("referrer", DEFAULT_REFERRER);

    Ok(url.to_string())
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    id_token: Option<String>,
}

#[derive(Debug)]
struct TokenSuccess {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    id_token: Option<String>,
}

fn exchange_authorization_code(
    code: &str,
    verifier: &str,
    challenge: &str,
    redirect_uri: &str,
    endpoints: &OAuthEndpoints,
    config: &XaiAuthConfig,
) -> Result<TokenSuccess, AuthFlowError> {
    let client = config.http_client()?;
    let response = client
        .post(&endpoints.token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", config.client_id.as_str()),
            ("code", code),
            ("code_verifier", verifier),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .map_err(|error| {
            map_http_error(error, RuntimeAuthPhase::ExchangingCode, "token_exchange")
        })?;

    parse_token_response(response, RuntimeAuthPhase::ExchangingCode, "token_exchange")
}

fn refresh_access_token(
    refresh_token: &str,
    endpoints: &OAuthEndpoints,
    config: &XaiAuthConfig,
) -> Result<TokenSuccess, AuthFlowError> {
    let client = config.http_client()?;
    let response = client
        .post(&endpoints.token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", config.client_id.as_str()),
        ])
        .send()
        .map_err(|error| map_http_error(error, RuntimeAuthPhase::Refreshing, "token_refresh"))?;

    parse_token_response(response, RuntimeAuthPhase::Refreshing, "token_refresh")
}

fn parse_token_response(
    response: Response,
    phase: RuntimeAuthPhase,
    prefix: &str,
) -> Result<TokenSuccess, AuthFlowError> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(AuthFlowError::new(
            if status.is_server_error() {
                format!("{prefix}_server_error")
            } else {
                format!("{prefix}_rejected")
            },
            phase,
            format!(
                "xAI returned HTTP {} during {}.{}",
                status.as_u16(),
                prefix.replace('_', " "),
                if body.trim().is_empty() {
                    String::new()
                } else {
                    format!(" Response: {}", body.trim())
                }
            ),
            status.is_server_error(),
        ));
    }

    let payload: TokenResponse = response.json().map_err(|error| {
        AuthFlowError::terminal(
            format!("{prefix}_decode_failed"),
            phase.clone(),
            format!(
                "Xero could not decode the xAI {} response: {error}",
                prefix.replace('_', " ")
            ),
        )
    })?;
    let access_token = payload.access_token.ok_or_else(|| {
        AuthFlowError::terminal(
            format!("{prefix}_decode_failed"),
            phase.clone(),
            format!(
                "Xero rejected the xAI {} response because access_token was missing.",
                prefix.replace('_', " ")
            ),
        )
    })?;
    let refresh_token = payload.refresh_token.ok_or_else(|| {
        AuthFlowError::terminal(
            format!("{prefix}_decode_failed"),
            phase.clone(),
            format!(
                "Xero rejected the xAI {} response because refresh_token was missing.",
                prefix.replace('_', " ")
            ),
        )
    })?;
    let expires_in = payload.expires_in.unwrap_or(3600).max(1);

    Ok(TokenSuccess {
        access_token,
        refresh_token,
        expires_at: current_unix_timestamp() + expires_in,
        id_token: payload.id_token,
    })
}

fn extract_xai_account_id(token_response: &TokenSuccess) -> Result<String, AuthFlowError> {
    for token in [
        token_response.id_token.as_deref(),
        Some(token_response.access_token.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        if let Ok(payload) = decode_jwt_payload(token) {
            if let Some(account) = payload
                .get("sub")
                .and_then(JsonValue::as_str)
                .or_else(|| payload.get("email").and_then(JsonValue::as_str))
                .or_else(|| {
                    payload
                        .get("preferred_username")
                        .and_then(JsonValue::as_str)
                })
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                return Ok(account.to_owned());
            }
        }
    }

    let digest = Sha256::digest(token_response.access_token.as_bytes());
    let suffix = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("xai-acct-{suffix}"))
}

fn decode_jwt_payload(token: &str) -> Result<JsonValue, AuthFlowError> {
    let parts = token.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(AuthFlowError::terminal(
            "access_token_malformed",
            RuntimeAuthPhase::Failed,
            "Xero received a malformed xAI access token.",
        ));
    }

    let payload = URL_SAFE_NO_PAD.decode(parts[1]).map_err(|_| {
        AuthFlowError::terminal(
            "access_token_malformed",
            RuntimeAuthPhase::Failed,
            "Xero could not base64url-decode the xAI access token payload.",
        )
    })?;

    serde_json::from_slice(&payload).map_err(|_| {
        AuthFlowError::terminal(
            "access_token_malformed",
            RuntimeAuthPhase::Failed,
            "Xero could not parse the xAI access token payload JSON.",
        )
    })
}

struct ParsedAuthorizationInput {
    code: String,
    state: Option<String>,
}

fn parse_authorization_input(input: &str) -> Result<ParsedAuthorizationInput, AuthFlowError> {
    let value = input.trim();
    if value.is_empty() {
        return Err(AuthFlowError::invalid_manual_input(
            "empty_auth_state",
            "The pasted xAI authorization value was empty.",
        ));
    }

    if value.contains("://") {
        let url = Url::parse(value).map_err(|_| {
            AuthFlowError::invalid_manual_input(
                "malformed_redirect_url",
                "Xero could not parse the pasted xAI redirect URL.",
            )
        })?;
        let code = url
            .query_pairs()
            .find_map(|(key, value)| (key == "code").then(|| value.into_owned()));
        let state = url
            .query_pairs()
            .find_map(|(key, value)| (key == "state").then(|| value.into_owned()));
        return code
            .map(|code| ParsedAuthorizationInput { code, state })
            .ok_or_else(|| {
                AuthFlowError::invalid_manual_input(
                    "authorization_code_missing",
                    "The pasted xAI redirect URL did not include an authorization code.",
                )
            });
    }

    if value.contains('#') {
        let mut parts = value.splitn(2, '#');
        let code = parts.next().unwrap_or_default().trim();
        let state = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if code.is_empty() {
            return Err(AuthFlowError::invalid_manual_input(
                "authorization_code_missing",
                "The pasted xAI authorization value did not include an authorization code.",
            ));
        }
        return Ok(ParsedAuthorizationInput {
            code: code.into(),
            state: state.map(ToOwned::to_owned),
        });
    }

    if value.contains('=') {
        let params = url::form_urlencoded::parse(value.as_bytes());
        let mut code = None;
        let mut state = None;
        for (key, value) in params {
            match key.as_ref() {
                "code" => code = Some(value.into_owned()),
                "state" => state = Some(value.into_owned()),
                _ => {}
            }
        }
        return code
            .map(|code| ParsedAuthorizationInput { code, state })
            .ok_or_else(|| {
                AuthFlowError::invalid_manual_input(
                    "authorization_code_missing",
                    "The pasted xAI parameters did not include an authorization code.",
                )
            });
    }

    Ok(ParsedAuthorizationInput {
        code: value.into(),
        state: None,
    })
}

fn spawn_callback_listener(listener: TcpListener, flow: ActiveXaiFlow) {
    thread::spawn(move || {
        if let Err(error) = listener.set_nonblocking(false) {
            flow.set_callback_diagnostic(
                AuthDiagnostic {
                    code: "callback_listener_configuration_failed".into(),
                    message: format!(
                        "Xero could not configure the local xAI callback listener: {error}"
                    ),
                    retryable: true,
                },
                RuntimeAuthPhase::AwaitingManualInput,
            );
            return;
        }

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    if handle_callback_connection(stream, &flow) {
                        break;
                    }
                }
                Err(error) => {
                    flow.set_callback_diagnostic(
                        AuthDiagnostic {
                            code: "callback_listener_accept_failed".into(),
                            message: format!(
                                "Xero could not accept the xAI callback connection: {error}"
                            ),
                            retryable: true,
                        },
                        RuntimeAuthPhase::AwaitingManualInput,
                    );
                    break;
                }
            }
            if flow.is_cancelled() {
                break;
            }
        }
    });
}

fn handle_callback_connection(mut stream: TcpStream, flow: &ActiveXaiFlow) -> bool {
    let mut request_line = String::new();
    let mut origin = None;
    let mut requested_headers = None;
    {
        let mut reader = BufReader::new(&mut stream);
        if reader.read_line(&mut request_line).is_err() {
            let _ = write_plain_response(&mut stream, 400, "Bad callback request");
            flow.set_callback_diagnostic(
                AuthDiagnostic {
                    code: "callback_request_read_failed".into(),
                    message: "Xero could not read the xAI callback request.".into(),
                    retryable: true,
                },
                RuntimeAuthPhase::AwaitingBrowserCallback,
            );
            return false;
        }

        let mut header_line = String::new();
        loop {
            header_line.clear();
            let read = reader.read_line(&mut header_line).unwrap_or_default();
            if read == 0 || header_line == "\r\n" || header_line == "\n" {
                break;
            }
            if let Some((name, value)) = header_line.split_once(':') {
                let value = value.trim().to_owned();
                match name.trim().to_ascii_lowercase().as_str() {
                    "origin" => origin = Some(value),
                    "access-control-request-headers" => requested_headers = Some(value),
                    _ => {}
                }
            }
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    if target.is_empty() {
        let _ = write_cors_plain_response(
            &mut stream,
            400,
            "Bad callback request",
            trusted_xai_callback_origin(origin.as_deref()).as_deref(),
        );
        flow.set_callback_diagnostic(
            AuthDiagnostic {
                code: "callback_request_malformed".into(),
                message: "Xero received a malformed xAI callback request line.".into(),
                retryable: false,
            },
            RuntimeAuthPhase::AwaitingBrowserCallback,
        );
        return false;
    }

    let cors_origin = trusted_xai_callback_origin(origin.as_deref());
    let url = match Url::parse(&format!("http://localhost{target}")) {
        Ok(url) => url,
        Err(_) => {
            let _ = write_cors_plain_response(
                &mut stream,
                400,
                "Bad callback URL",
                cors_origin.as_deref(),
            );
            flow.set_callback_diagnostic(
                AuthDiagnostic {
                    code: "callback_query_malformed".into(),
                    message: "Xero received a malformed xAI callback query string.".into(),
                    retryable: false,
                },
                RuntimeAuthPhase::AwaitingBrowserCallback,
            );
            return false;
        }
    };

    if url.path() != flow.callback_path() {
        let _ = write_cors_plain_response(
            &mut stream,
            404,
            "Unknown callback path",
            cors_origin.as_deref(),
        );
        return false;
    }

    if method == "OPTIONS" {
        let _ = write_cors_preflight_response(
            &mut stream,
            cors_origin.as_deref(),
            requested_headers.as_deref(),
        );
        return false;
    }

    if method != "GET" {
        let _ = write_cors_plain_response(
            &mut stream,
            405,
            "Method not allowed",
            cors_origin.as_deref(),
        );
        return false;
    }

    let state = url
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()));
    if state.as_deref() != Some(flow.expected_state()) {
        let _ = write_cors_plain_response(
            &mut stream,
            400,
            "OAuth state mismatch",
            cors_origin.as_deref(),
        );
        flow.set_callback_diagnostic(
            AuthDiagnostic {
                code: "callback_state_mismatch".into(),
                message: "Xero rejected the xAI callback because the OAuth state did not match."
                    .into(),
                retryable: false,
            },
            RuntimeAuthPhase::AwaitingBrowserCallback,
        );
        return true;
    }

    let code = url
        .query_pairs()
        .find_map(|(key, value)| (key == "code").then(|| value.into_owned()));
    let Some(code) = code.filter(|code| !code.trim().is_empty()) else {
        let _ = write_cors_plain_response(
            &mut stream,
            400,
            "Authorization code missing",
            cors_origin.as_deref(),
        );
        flow.set_callback_diagnostic(
            AuthDiagnostic {
                code: "callback_code_missing".into(),
                message:
                    "Xero rejected the xAI callback because the authorization code was missing."
                        .into(),
                retryable: false,
            },
            RuntimeAuthPhase::AwaitingBrowserCallback,
        );
        return true;
    };

    flow.store_callback_code(code);
    let _ = write_cors_plain_response(
        &mut stream,
        200,
        "xAI login captured. You can return to Xero.",
        cors_origin.as_deref(),
    );
    true
}

fn write_plain_response(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    write_cors_plain_response(stream, status, body, None)
}

fn write_cors_preflight_response(
    stream: &mut TcpStream,
    cors_origin: Option<&str>,
    requested_headers: Option<&str>,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 204 No Content\r\n{}Access-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: {}\r\nAccess-Control-Allow-Private-Network: true\r\nAccess-Control-Max-Age: 600\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        cors_response_headers(cors_origin),
        requested_headers
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("content-type")
    )
}

fn write_cors_plain_response(
    stream: &mut TcpStream,
    status: u16,
    body: &str,
    cors_origin: Option<&str>,
) -> std::io::Result<()> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {status_text}\r\n{}Content-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        cors_response_headers(cors_origin),
        body.len(),
        body
    )
}

fn cors_response_headers(cors_origin: Option<&str>) -> String {
    cors_origin
        .map(|origin| {
            format!(
                "Access-Control-Allow-Origin: {origin}\r\nVary: Origin, Access-Control-Request-Method, Access-Control-Request-Headers\r\n"
            )
        })
        .unwrap_or_default()
}

fn trusted_xai_callback_origin(origin: Option<&str>) -> Option<String> {
    let origin = origin?;
    let parsed = Url::parse(origin).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    CALLBACK_CORS_ALLOWLIST
        .iter()
        .any(|allowed| host == *allowed)
        .then(|| parsed.origin().ascii_serialization())
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

fn map_http_error(error: reqwest::Error, phase: RuntimeAuthPhase, prefix: &str) -> AuthFlowError {
    if error.is_timeout() {
        return AuthFlowError::retryable(
            format!("{prefix}_timeout"),
            phase,
            format!("xAI {} timed out.", prefix.replace('_', " ")),
        );
    }

    AuthFlowError::retryable(
        format!("{prefix}_request_failed"),
        phase,
        format!(
            "Xero could not complete the xAI {} request: {error}",
            prefix.replace('_', " ")
        ),
    )
}

fn map_provider_credentials_error(error: CommandError) -> AuthFlowError {
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

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xai_config_requires_owned_oauth_client_id() {
        let config = XaiAuthConfig {
            client_id: String::new(),
            ..XaiAuthConfig::default()
        };
        let error = config
            .require_client_id(RuntimeAuthPhase::Starting)
            .expect_err("empty client id should fail");
        assert_eq!(error.code, "xai_oauth_client_unconfigured");
        assert!(error.message.contains("This Xero build"));
        assert!(!error.message.contains("XAI_OAUTH_CLIENT_ID"));
    }

    #[test]
    fn xai_oauth_defaults_match_live_discovery_shape() {
        let config = XaiAuthConfig::default();

        assert_eq!(config.client_id, DEFAULT_CLIENT_ID);
        assert_eq!(
            config.discovery_url,
            "https://auth.x.ai/.well-known/openid-configuration"
        );
        assert_eq!(config.authorize_url, "https://auth.x.ai/oauth2/authorize");
        assert_eq!(config.token_url, "https://auth.x.ai/oauth2/token");
        assert!(config.scope.contains("openid"));
        assert!(config.scope.contains("offline_access"));
        assert!(config.scope.contains("email"));
        assert!(config.scope.contains("grok-cli:access"));
        assert!(config.scope.contains("api:access"));
        assert_eq!(config.callback_host, "127.0.0.1");
        assert_eq!(config.callback_port, 56121);
        assert_eq!(config.callback_path, "/callback");
    }

    #[test]
    fn xai_config_rejects_x_developer_portal_oauth_client_ids() {
        let config = XaiAuthConfig {
            client_id: "dVQyOUFiN0xDWHhUajJQektQYzM6MTpjaQ".into(),
            ..XaiAuthConfig::default()
        };

        let error = config
            .require_client_id(RuntimeAuthPhase::Starting)
            .expect_err("X Developer Portal client id should not be accepted for auth.x.ai");

        assert_eq!(error.code, "xai_oauth_client_wrong_issuer");
        assert!(error.message.contains("X Developer Portal"));
        assert!(error.message.contains("auth.x.ai"));
    }

    #[test]
    fn xai_account_id_falls_back_to_stable_token_hash() {
        let token = TokenSuccess {
            access_token: "opaque-token".into(),
            refresh_token: "refresh".into(),
            expires_at: 10,
            id_token: None,
        };
        let account_id = extract_xai_account_id(&token).expect("account id");
        assert!(account_id.starts_with("xai-acct-"));
    }

    #[test]
    fn xai_authorization_url_uses_pkce_and_xero_callback() {
        let config = XaiAuthConfig {
            client_id: "xero-client".into(),
            scope: "openid offline_access".into(),
            ..XaiAuthConfig::default()
        };

        let url = build_authorization_url(
            DEFAULT_AUTHORIZE_URL,
            &config,
            "state-1",
            "nonce-1",
            "challenge-1",
            "http://127.0.0.1:56121/callback",
        )
        .expect("authorization url");
        let parsed = Url::parse(&url).expect("authorization URL should parse");
        let params = parsed
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(
            parsed.as_str().split('?').next(),
            Some(DEFAULT_AUTHORIZE_URL)
        );
        assert_eq!(
            params.get("response_type").map(|value| value.as_ref()),
            Some("code")
        );
        assert_eq!(
            params.get("client_id").map(|value| value.as_ref()),
            Some("xero-client")
        );
        assert_eq!(
            params.get("code_challenge").map(|value| value.as_ref()),
            Some("challenge-1")
        );
        assert_eq!(
            params
                .get("code_challenge_method")
                .map(|value| value.as_ref()),
            Some("S256")
        );
        assert_eq!(
            params.get("state").map(|value| value.as_ref()),
            Some("state-1")
        );
        assert_eq!(
            params.get("nonce").map(|value| value.as_ref()),
            Some("nonce-1")
        );
        assert_eq!(
            params.get("plan").map(|value| value.as_ref()),
            Some("generic")
        );
        assert_eq!(
            params.get("referrer").map(|value| value.as_ref()),
            Some("xero")
        );
        assert_eq!(
            params.get("redirect_uri").map(|value| value.as_ref()),
            Some("http://127.0.0.1:56121/callback")
        );
    }

    #[test]
    fn xai_callback_cors_only_trusts_xai_origins() {
        assert_eq!(
            trusted_xai_callback_origin(Some("https://auth.x.ai")),
            Some("https://auth.x.ai".into())
        );
        assert_eq!(
            trusted_xai_callback_origin(Some("https://accounts.x.ai")),
            Some("https://accounts.x.ai".into())
        );
        assert_eq!(trusted_xai_callback_origin(Some("http://auth.x.ai")), None);
        assert_eq!(
            trusted_xai_callback_origin(Some("https://evil.example")),
            None
        );
        assert_eq!(trusted_xai_callback_origin(None), None);
    }
}
