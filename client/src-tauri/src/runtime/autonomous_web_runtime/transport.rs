use std::{io::Read, time::Duration};

use reqwest::{blocking::Client, header::CONTENT_TYPE, redirect::Policy};

use crate::commands::{CommandError, CommandResult};

use super::{AutonomousWebRuntime, MAX_REDIRECTS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomousWebHttpMethod {
    Get,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousWebTransportRequest {
    pub method: AutonomousWebHttpMethod,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
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
                    "Xero could not initialize the autonomous web HTTP client: {error}"
                ))
            })?;

        let mut http_request = match request.method {
            AutonomousWebHttpMethod::Get => client.get(&request.url),
            AutonomousWebHttpMethod::Post => client.post(&request.url),
        };
        for (name, value) in &request.headers {
            http_request = http_request.header(name, value);
        }
        if let Some(body) = &request.body {
            http_request = http_request.body(body.clone());
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
                    "Xero could not read the autonomous web response body: {error}"
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

fn map_transport_error(error: reqwest::Error) -> AutonomousWebTransportError {
    if error.is_timeout() {
        return AutonomousWebTransportError::Timeout(
            "Xero timed out while waiting for the autonomous web response.".into(),
        );
    }

    if error.is_redirect() {
        return AutonomousWebTransportError::Redirect(
            "Xero rejected the autonomous web request because the redirect chain was invalid or exceeded the configured limit.".into(),
        );
    }

    AutonomousWebTransportError::Transport(format!(
        "Xero could not execute the autonomous web request: {}",
        redact_transport_error(&error.to_string())
    ))
}

fn redact_transport_error(message: &str) -> String {
    message
        .split_whitespace()
        .map(redact_possible_url)
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_possible_url(value: &str) -> String {
    let trimmed = value.trim_matches(|ch: char| matches!(ch, '"' | '\'' | ',' | ')' | '('));
    let Ok(mut url) = url::Url::parse(trimmed) else {
        return value.to_owned();
    };
    if url.query().is_some() {
        let pairs = url
            .query_pairs()
            .map(|(key, value)| {
                let redacted = matches!(
                    key.as_ref(),
                    "api_key" | "key" | "token" | "access_token" | "subscription-token"
                );
                (
                    key.into_owned(),
                    if redacted {
                        "<redacted>".to_owned()
                    } else {
                        value.into_owned()
                    },
                )
            })
            .collect::<Vec<_>>();
        url.set_query(None);
        {
            let mut query = url.query_pairs_mut();
            for (key, value) in pairs {
                query.append_pair(&key, &value);
            }
        }
    }
    value.replace(trimmed, url.as_str())
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
