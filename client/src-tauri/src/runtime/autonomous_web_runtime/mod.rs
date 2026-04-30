mod extract;
mod fetch;
mod search;
mod transport;

use std::{fmt, sync::Arc};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::commands::{CommandError, CommandResult};

pub use transport::{
    AutonomousWebTransport, AutonomousWebTransportError, AutonomousWebTransportRequest,
    AutonomousWebTransportResponse,
};

pub const AUTONOMOUS_TOOL_WEB_SEARCH: &str = "web_search";
pub const AUTONOMOUS_TOOL_WEB_FETCH: &str = "web_fetch";

const SEARCH_PROVIDER_URL_ENV: &str = "XERO_AUTONOMOUS_WEB_SEARCH_URL";
const SEARCH_PROVIDER_BEARER_TOKEN_ENV: &str = "XERO_AUTONOMOUS_WEB_SEARCH_BEARER_TOKEN";
const DEFAULT_TIMEOUT_MS: u64 = 8_000;
const MAX_TIMEOUT_MS: u64 = 20_000;
const DEFAULT_SEARCH_RESULT_COUNT: usize = 5;
const MAX_SEARCH_RESULT_COUNT: usize = 10;
const MAX_SEARCH_QUERY_CHARS: usize = 256;
const DEFAULT_FETCH_MAX_CHARS: usize = 8_000;
const MAX_FETCH_MAX_CHARS: usize = 12_000;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_TITLE_CHARS: usize = 200;
const MAX_SNIPPET_CHARS: usize = 400;
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutonomousWebRuntimeLimits {
    pub default_timeout_ms: u64,
    pub max_timeout_ms: u64,
    pub default_search_result_count: usize,
    pub max_search_result_count: usize,
    pub max_search_query_chars: usize,
    pub default_fetch_max_chars: usize,
    pub max_fetch_max_chars: usize,
    pub max_response_bytes: usize,
    pub max_title_chars: usize,
    pub max_snippet_chars: usize,
}

impl Default for AutonomousWebRuntimeLimits {
    fn default() -> Self {
        Self {
            default_timeout_ms: DEFAULT_TIMEOUT_MS,
            max_timeout_ms: MAX_TIMEOUT_MS,
            default_search_result_count: DEFAULT_SEARCH_RESULT_COUNT,
            max_search_result_count: MAX_SEARCH_RESULT_COUNT,
            max_search_query_chars: MAX_SEARCH_QUERY_CHARS,
            default_fetch_max_chars: DEFAULT_FETCH_MAX_CHARS,
            max_fetch_max_chars: MAX_FETCH_MAX_CHARS,
            max_response_bytes: MAX_RESPONSE_BYTES,
            max_title_chars: MAX_TITLE_CHARS,
            max_snippet_chars: MAX_SNIPPET_CHARS,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AutonomousWebSearchProviderConfig {
    pub endpoint: String,
    pub bearer_token: Option<String>,
}

impl AutonomousWebSearchProviderConfig {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            bearer_token: None,
        }
    }

    pub fn with_bearer_token(mut self, bearer_token: impl Into<String>) -> Self {
        self.bearer_token = Some(bearer_token.into());
        self
    }
}

impl fmt::Debug for AutonomousWebSearchProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AutonomousWebSearchProviderConfig")
            .field("endpoint", &self.endpoint)
            .field(
                "has_bearer_token",
                &self
                    .bearer_token
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
            )
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutonomousWebConfig {
    pub search_provider: Option<AutonomousWebSearchProviderConfig>,
    pub limits: AutonomousWebRuntimeLimits,
}

impl AutonomousWebConfig {
    pub fn for_platform() -> Self {
        Self {
            search_provider: transport::search_provider_from_env(),
            limits: AutonomousWebRuntimeLimits::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWebSearchRequest {
    pub query: String,
    pub result_count: Option<usize>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWebFetchRequest {
    pub url: String,
    pub max_chars: Option<usize>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWebSearchOutput {
    pub query: String,
    pub results: Vec<AutonomousWebSearchResult>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousWebFetchContentKind {
    Html,
    PlainText,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWebFetchOutput {
    pub url: String,
    pub final_url: String,
    pub content_type: Option<String>,
    pub content_kind: AutonomousWebFetchContentKind,
    pub title: Option<String>,
    pub content: String,
    pub truncated: bool,
}

#[derive(Clone)]
pub struct AutonomousWebRuntime {
    config: AutonomousWebConfig,
    transport: Option<Arc<dyn AutonomousWebTransport>>,
}

impl fmt::Debug for AutonomousWebRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AutonomousWebRuntime")
            .field("config", &self.config)
            .field("has_test_transport", &self.transport.is_some())
            .finish()
    }
}

impl AutonomousWebRuntime {
    pub fn new(config: AutonomousWebConfig) -> Self {
        Self {
            config,
            transport: None,
        }
    }

    pub fn with_transport(
        config: AutonomousWebConfig,
        transport: Arc<dyn AutonomousWebTransport>,
    ) -> Self {
        Self {
            config,
            transport: Some(transport),
        }
    }

    pub fn config(&self) -> &AutonomousWebConfig {
        &self.config
    }
}

fn parse_http_url(
    value: &str,
    error_code: &'static str,
    message: &'static str,
) -> CommandResult<Url> {
    let parsed =
        Url::parse(value.trim()).map_err(|_| CommandError::user_fixable(error_code, message))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(CommandError::user_fixable(
            if error_code == "autonomous_web_fetch_url_invalid" {
                "autonomous_web_fetch_scheme_unsupported"
            } else {
                error_code
            },
            if error_code == "autonomous_web_fetch_url_invalid" {
                "Xero requires `web_fetch` URLs to use the HTTP or HTTPS scheme."
            } else {
                message
            },
        ));
    }

    Ok(parsed)
}

fn normalize_timeout_ms(
    value: Option<u64>,
    default_timeout_ms: u64,
    max_timeout_ms: u64,
    error_code: &'static str,
    label: &'static str,
) -> CommandResult<u64> {
    let timeout_ms = value.unwrap_or(default_timeout_ms);
    if timeout_ms == 0 || timeout_ms > max_timeout_ms {
        return Err(CommandError::user_fixable(
            error_code,
            format!("Xero requires {label} to be between 1 and {max_timeout_ms}."),
        ));
    }
    Ok(timeout_ms)
}

fn normalize_bounded_usize(
    value: Option<usize>,
    default_value: usize,
    max_value: usize,
    error_code: &'static str,
    label: &'static str,
) -> CommandResult<usize> {
    let value = value.unwrap_or(default_value);
    if value == 0 || value > max_value {
        return Err(CommandError::user_fixable(
            error_code,
            format!("Xero requires {label} to be between 1 and {max_value}."),
        ));
    }
    Ok(value)
}

fn is_success_status(status: u16) -> bool {
    (200..=299).contains(&status)
}
