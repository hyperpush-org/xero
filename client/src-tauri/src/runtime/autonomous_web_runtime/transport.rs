use std::{io::Read, time::Duration};

use reqwest::{blocking::Client, header::CONTENT_TYPE, redirect::Policy};

use crate::commands::{CommandError, CommandResult};

use super::{
    AutonomousWebRuntime, AutonomousWebSearchProviderConfig, MAX_REDIRECTS,
    SEARCH_PROVIDER_BEARER_TOKEN_ENV, SEARCH_PROVIDER_URL_ENV,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousWebTransportRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub timeout_ms: u64,
    pub max_response_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousWebTransportResponse {
    pub status: u16,
    pub final_url: String,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
    pub body_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousWebTransportError {
    Setup(String),
    Timeout(String),
    Redirect(String),
    Transport(String),
}

pub trait AutonomousWebTransport: Send + Sync {
    fn execute(
        &self,
        request: &AutonomousWebTransportRequest,
    ) -> Result<AutonomousWebTransportResponse, AutonomousWebTransportError>;
}

#[derive(Debug, Clone, Default)]
pub(super) struct ReqwestAutonomousWebTransport;

impl AutonomousWebTransport for ReqwestAutonomousWebTransport {
    fn execute(
        &self,
        request: &AutonomousWebTransportRequest,
    ) -> Result<AutonomousWebTransportResponse, AutonomousWebTransportError> {
        let client = Client::builder()
            .timeout(Duration::from_millis(request.timeout_ms))
            .redirect(Policy::limited(MAX_REDIRECTS))
            .build()
            .map_err(|error| {
                AutonomousWebTransportError::Setup(format!(
                    "Cadence could not initialize the autonomous web HTTP client: {error}"
                ))
            })?;

        let mut http_request = client.get(&request.url);
        for (name, value) in &request.headers {
            http_request = http_request.header(name, value);
        }

        let mut response = http_request.send().map_err(map_transport_error)?;
        let status = response.status().as_u16();
        let final_url = response.url().to_string();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());
        let mut body = Vec::new();
        response
            .by_ref()
            .take(request.max_response_bytes.saturating_add(1) as u64)
            .read_to_end(&mut body)
            .map_err(|error| {
                AutonomousWebTransportError::Transport(format!(
                    "Cadence could not read the autonomous web response body: {error}"
                ))
            })?;
        let body_truncated = body.len() > request.max_response_bytes;
        if body_truncated {
            body.truncate(request.max_response_bytes);
        }

        Ok(AutonomousWebTransportResponse {
            status,
            final_url,
            content_type,
            body,
            body_truncated,
        })
    }
}

impl AutonomousWebRuntime {
    pub(super) fn execute_transport(
        &self,
        request: AutonomousWebTransportRequest,
    ) -> CommandResult<AutonomousWebTransportResponse> {
        let result = match &self.transport {
            Some(transport) => transport.execute(&request),
            None => ReqwestAutonomousWebTransport.execute(&request),
        };

        result.map_err(map_transport_failure)
    }
}

pub(super) fn search_provider_from_env() -> Option<AutonomousWebSearchProviderConfig> {
    let endpoint = std::env::var(SEARCH_PROVIDER_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    let bearer_token = std::env::var(SEARCH_PROVIDER_BEARER_TOKEN_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Some(AutonomousWebSearchProviderConfig {
        endpoint,
        bearer_token,
    })
}

fn map_transport_error(error: reqwest::Error) -> AutonomousWebTransportError {
    if error.is_timeout() {
        return AutonomousWebTransportError::Timeout(
            "Cadence timed out while waiting for the autonomous web response.".into(),
        );
    }

    if error.is_redirect() {
        return AutonomousWebTransportError::Redirect(
            "Cadence rejected the autonomous web request because the redirect chain was invalid or exceeded the configured limit.".into(),
        );
    }

    AutonomousWebTransportError::Transport(format!(
        "Cadence could not execute the autonomous web request: {error}"
    ))
}

fn map_transport_failure(error: AutonomousWebTransportError) -> CommandError {
    match error {
        AutonomousWebTransportError::Setup(message) => {
            CommandError::system_fault("autonomous_web_transport_unavailable", message)
        }
        AutonomousWebTransportError::Timeout(message) => {
            CommandError::retryable("autonomous_web_timeout", message)
        }
        AutonomousWebTransportError::Redirect(message) => {
            CommandError::user_fixable("autonomous_web_redirect_invalid", message)
        }
        AutonomousWebTransportError::Transport(message) => {
            CommandError::retryable("autonomous_web_transport_failed", message)
        }
    }
}
