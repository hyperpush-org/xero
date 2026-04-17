use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime};
use url::Url;

use super::{
    now_timestamp, store, AuthDiagnostic, AuthFlowError, RuntimeAuthStateSnapshot,
    OPENAI_CODEX_PROVIDER_ID,
};
use crate::{
    commands::RuntimeAuthPhase,
    runtime::{
        bind_openai_callback_listener, default_openai_callback_policy,
        resolve_openai_callback_policy, OpenAiCallbackBindResult,
    },
    state::DesktopState,
};

const DEFAULT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEFAULT_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const DEFAULT_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const DEFAULT_SCOPE: &str = "openid profile email offline_access";
const DEFAULT_ORIGINATOR: &str = "cadence";
const SUCCESS_HTML: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\" /><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" /><title>Authentication successful</title></head><body><p>Authentication successful. Return to Cadence to continue.</p></body></html>";
const JWT_CLAIM_PATH: &str = "https://api.openai.com/auth";

#[derive(Debug, Clone)]
pub struct OpenAiCodexAuthConfig {
    pub client_id: String,
    pub authorize_url: String,
    pub token_url: String,
    pub callback_host: String,
    pub callback_port: u16,
    pub callback_path: String,
    pub scope: String,
    pub originator: String,
    pub timeout: Duration,
}

impl Default for OpenAiCodexAuthConfig {
    fn default() -> Self {
        let callback_policy = default_openai_callback_policy();
        Self {
            client_id: DEFAULT_CLIENT_ID.into(),
            authorize_url: DEFAULT_AUTHORIZE_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
            callback_host: callback_policy.host,
            callback_port: callback_policy.preferred_port,
            callback_path: callback_policy.path,
            scope: DEFAULT_SCOPE.into(),
            originator: DEFAULT_ORIGINATOR.into(),
            timeout: Duration::from_secs(30),
        }
    }
}

impl OpenAiCodexAuthConfig {
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
                    format!("Cadence could not build the OpenAI OAuth HTTP client: {error}"),
                )
            })
    }
}

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

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

impl ActiveOpenAiCodexFlow {
    pub fn flow_id(&self) -> &str {
        &self.flow_id
    }

    pub fn snapshot(&self) -> RuntimeAuthStateSnapshot {
        let observation = self
            .observation
            .lock()
            .expect("openai oauth flow lock poisoned");

        RuntimeAuthStateSnapshot {
            provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
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

    fn set_callback_diagnostic(&self, diagnostic: AuthDiagnostic, phase: RuntimeAuthPhase) {
        let mut observation = self
            .observation
            .lock()
            .expect("openai oauth flow lock poisoned");
        observation.last_error = Some(diagnostic);
        observation.phase = phase;
        observation.updated_at = now_timestamp();
    }

    fn store_callback_code(&self, code: String) {
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

    fn is_cancelled(&self) -> bool {
        self.observation
            .lock()
            .expect("openai oauth flow lock poisoned")
            .cancelled
    }
}

pub fn start_openai_codex_flow(
    state: &DesktopState,
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

    state.active_auth_flows().insert(flow);

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
    let selected_flow = state
        .active_auth_flows()
        .with_flow(flow_id, |flow| flow.clone())
        .ok_or_else(|| {
            AuthFlowError::terminal(
                "auth_flow_not_found",
                RuntimeAuthPhase::Failed,
                format!("Cadence could not find the active OpenAI auth flow `{flow_id}`."),
            )
        })?;

    if selected_flow.is_cancelled() {
        return Err(AuthFlowError::terminal(
            "auth_flow_cancelled",
            RuntimeAuthPhase::Cancelled,
            "The OpenAI login flow was cancelled before completion.",
        ));
    }

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
        provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
        session_id: random_hex(16)?,
        account_id: account_id.clone(),
        expires_at: token_response.expires_at,
        updated_at: now_timestamp(),
    };
    let auth_store_path = state.auth_store_file(app)?;
    store::persist_openai_codex_session(
        &auth_store_path,
        store::StoredOpenAiCodexSession {
            provider_id: session.provider_id.clone(),
            session_id: session.session_id.clone(),
            account_id: session.account_id.clone(),
            access_token: token_response.access_token,
            refresh_token: token_response.refresh_token,
            expires_at: session.expires_at,
            updated_at: session.updated_at.clone(),
        },
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

    let auth_store_path = state.auth_store_file(app)?;
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
        provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
        session_id: stored_session.session_id.clone(),
        account_id: refreshed_account_id.clone(),
        expires_at: refreshed.expires_at,
        updated_at: now_timestamp(),
    };

    store::persist_openai_codex_session(
        &auth_store_path,
        store::StoredOpenAiCodexSession {
            provider_id: updated_session.provider_id.clone(),
            session_id: updated_session.session_id.clone(),
            account_id: refreshed_account_id,
            access_token: refreshed.access_token,
            refresh_token: refreshed.refresh_token,
            expires_at: updated_session.expires_at,
            updated_at: updated_session.updated_at.clone(),
        },
    )?;

    Ok(updated_session)
}

pub fn cancel_openai_codex_flow(
    state: &DesktopState,
    flow_id: &str,
) -> Result<RuntimeAuthStateSnapshot, AuthFlowError> {
    state
        .active_auth_flows()
        .with_flow(flow_id, |flow| {
            flow.mark_cancelled();
            flow.snapshot()
        })
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

fn spawn_callback_listener(listener: TcpListener, flow: ActiveOpenAiCodexFlow) {
    thread::spawn(move || {
        if listener.set_nonblocking(true).is_err() {
            flow.set_callback_diagnostic(
                AuthDiagnostic {
                    code: "callback_listener_configuration_failed".into(),
                    message: "Cadence could not configure the local OpenAI callback listener."
                        .into(),
                    retryable: false,
                },
                RuntimeAuthPhase::AwaitingManualInput,
            );
            return;
        }

        loop {
            if flow.is_cancelled() {
                break;
            }

            match listener.accept() {
                Ok((stream, _)) => handle_callback_connection(stream, &flow),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => {
                    flow.set_callback_diagnostic(
                        AuthDiagnostic {
                            code: "callback_listener_accept_failed".into(),
                            message: format!(
                                "Cadence could not accept the OpenAI callback connection: {error}"
                            ),
                            retryable: false,
                        },
                        RuntimeAuthPhase::AwaitingManualInput,
                    );
                    break;
                }
            }
        }
    });
}

fn handle_callback_connection(mut stream: TcpStream, flow: &ActiveOpenAiCodexFlow) {
    let mut request_line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        if reader.read_line(&mut request_line).is_err() {
            let _ = write_plain_response(&mut stream, 500, "Internal error");
            flow.set_callback_diagnostic(
                AuthDiagnostic {
                    code: "callback_request_read_failed".into(),
                    message: "Cadence could not read the OpenAI callback request.".into(),
                    retryable: false,
                },
                RuntimeAuthPhase::AwaitingBrowserCallback,
            );
            return;
        }

        let mut discard = String::new();
        while reader
            .read_line(&mut discard)
            .ok()
            .filter(|_| discard != "\r\n")
            .is_some()
        {
            discard.clear();
        }
    }

    let target = match request_line.split_whitespace().nth(1) {
        Some(value) => value,
        None => {
            let _ = write_plain_response(&mut stream, 400, "Bad request");
            flow.set_callback_diagnostic(
                AuthDiagnostic {
                    code: "callback_request_malformed".into(),
                    message: "Cadence received a malformed OpenAI callback request line.".into(),
                    retryable: false,
                },
                RuntimeAuthPhase::AwaitingBrowserCallback,
            );
            return;
        }
    };

    let url = match Url::parse(&format!("http://localhost{target}")) {
        Ok(url) => url,
        Err(_) => {
            let _ = write_plain_response(&mut stream, 400, "Bad callback URL");
            flow.set_callback_diagnostic(
                AuthDiagnostic {
                    code: "callback_query_malformed".into(),
                    message: "Cadence received a malformed OpenAI callback query string.".into(),
                    retryable: false,
                },
                RuntimeAuthPhase::AwaitingBrowserCallback,
            );
            return;
        }
    };

    if url.path() != flow.callback_path.as_str() {
        let _ = write_plain_response(&mut stream, 404, "Not found");
        return;
    }

    let returned_state = url
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()));
    if returned_state.as_deref() != Some(flow.expected_state.as_str()) {
        let _ = write_plain_response(&mut stream, 400, "State mismatch");
        flow.set_callback_diagnostic(
            AuthDiagnostic {
                code: "callback_state_mismatch".into(),
                message:
                    "Cadence rejected the OpenAI callback because the OAuth state did not match."
                        .into(),
                retryable: false,
            },
            RuntimeAuthPhase::AwaitingBrowserCallback,
        );
        return;
    }

    let code = url
        .query_pairs()
        .find_map(|(key, value)| (key == "code").then(|| value.into_owned()));
    let Some(code) = code else {
        let _ = write_plain_response(&mut stream, 400, "Missing authorization code");
        flow.set_callback_diagnostic(
            AuthDiagnostic {
                code: "callback_code_missing".into(),
                message: "Cadence rejected the OpenAI callback because the authorization code was missing.".into(),
                retryable: false,
            },
            RuntimeAuthPhase::AwaitingBrowserCallback,
        );
        return;
    };

    flow.store_callback_code(code);
    let _ = write_html_response(&mut stream, SUCCESS_HTML);
}

fn write_html_response(stream: &mut TcpStream, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn write_plain_response(
    stream: &mut TcpStream,
    status_code: u16,
    body: &str,
) -> std::io::Result<()> {
    let reason = match status_code {
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_code,
        reason,
        body.len(),
        body
    )
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
            "The pasted OpenAI authorization value was empty.",
        ));
    }

    if value.contains("://") {
        let url = Url::parse(value).map_err(|_| {
            AuthFlowError::invalid_manual_input(
                "malformed_redirect_url",
                "Cadence could not parse the pasted OpenAI redirect URL.",
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
                    "The pasted OpenAI redirect URL did not include an authorization code.",
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
                "The pasted OpenAI authorization value did not include an authorization code.",
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
                    "The pasted OpenAI parameters did not include an authorization code.",
                )
            });
    }

    Ok(ParsedAuthorizationInput {
        code: value.into(),
        state: None,
    })
}

fn exchange_authorization_code(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    config: &OpenAiCodexAuthConfig,
) -> Result<TokenSuccess, AuthFlowError> {
    let client = config.http_client()?;
    let response = client
        .post(&config.token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", config.client_id.as_str()),
            ("code", code),
            ("code_verifier", verifier),
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
    config: &OpenAiCodexAuthConfig,
) -> Result<TokenSuccess, AuthFlowError> {
    let client = config.http_client()?;
    let response = client
        .post(&config.token_url)
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
    response: reqwest::blocking::Response,
    phase: RuntimeAuthPhase,
    prefix: &str,
) -> Result<TokenSuccess, AuthFlowError> {
    let status = response.status();
    if !status.is_success() {
        let code = if status.is_server_error() {
            format!("{prefix}_server_error")
        } else {
            format!("{prefix}_rejected")
        };
        let retryable = status.is_server_error();
        let body = response.text().unwrap_or_default();
        return Err(AuthFlowError::new(
            code,
            phase,
            format!(
                "OpenAI returned HTTP {} during {}.{}",
                status.as_u16(),
                prefix.replace('_', " "),
                if body.trim().is_empty() {
                    String::new()
                } else {
                    format!(" Response: {}", body.trim())
                }
            ),
            retryable,
        ));
    }

    let payload: TokenResponse = response.json().map_err(|error| {
        AuthFlowError::terminal(
            format!("{prefix}_decode_failed"),
            phase.clone(),
            format!(
                "Cadence could not decode the OpenAI {} response: {error}",
                prefix.replace('_', " ")
            ),
        )
    })?;

    let access_token = payload.access_token.ok_or_else(|| {
        AuthFlowError::terminal(
            format!("{prefix}_decode_failed"),
            phase.clone(),
            format!(
                "Cadence rejected the OpenAI {} response because access_token was missing.",
                prefix.replace('_', " ")
            ),
        )
    })?;
    let refresh_token = payload.refresh_token.ok_or_else(|| {
        AuthFlowError::terminal(
            format!("{prefix}_decode_failed"),
            phase.clone(),
            format!(
                "Cadence rejected the OpenAI {} response because refresh_token was missing.",
                prefix.replace('_', " ")
            ),
        )
    })?;
    let expires_in = payload.expires_in.ok_or_else(|| {
        AuthFlowError::terminal(
            format!("{prefix}_decode_failed"),
            phase,
            format!(
                "Cadence rejected the OpenAI {} response because expires_in was missing.",
                prefix.replace('_', " ")
            ),
        )
    })?;

    Ok(TokenSuccess {
        access_token,
        refresh_token,
        expires_at: current_unix_timestamp() + expires_in,
    })
}

fn map_http_error(error: reqwest::Error, phase: RuntimeAuthPhase, prefix: &str) -> AuthFlowError {
    if error.is_timeout() {
        return AuthFlowError::retryable(
            format!("{prefix}_timeout"),
            phase,
            format!("OpenAI {} timed out.", prefix.replace('_', " ")),
        );
    }

    AuthFlowError::retryable(
        format!("{prefix}_request_failed"),
        phase,
        format!(
            "Cadence could not complete the OpenAI {} request: {error}",
            prefix.replace('_', " ")
        ),
    )
}

fn extract_account_id(access_token: &str) -> Result<String, AuthFlowError> {
    let payload = decode_jwt_payload(access_token)?;
    payload
        .get(JWT_CLAIM_PATH)
        .and_then(|value| value.get("chatgpt_account_id"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AuthFlowError::terminal(
                "account_id_missing",
                RuntimeAuthPhase::Failed,
                "Cadence could not extract the OpenAI account id from the access token.",
            )
        })
}

fn decode_jwt_payload(access_token: &str) -> Result<serde_json::Value, AuthFlowError> {
    let parts = access_token.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(AuthFlowError::terminal(
            "access_token_malformed",
            RuntimeAuthPhase::Failed,
            "Cadence received a malformed OpenAI access token.",
        ));
    }

    let payload = URL_SAFE_NO_PAD.decode(parts[1]).map_err(|_| {
        AuthFlowError::terminal(
            "access_token_malformed",
            RuntimeAuthPhase::Failed,
            "Cadence could not base64url-decode the OpenAI access token payload.",
        )
    })?;

    serde_json::from_slice(&payload).map_err(|_| {
        AuthFlowError::terminal(
            "access_token_malformed",
            RuntimeAuthPhase::Failed,
            "Cadence could not parse the OpenAI access token payload JSON.",
        )
    })
}

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}

struct TokenSuccess {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}
