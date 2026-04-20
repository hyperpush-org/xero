use std::{fmt, io::Read, sync::Arc, time::Duration};

use reqwest::{blocking::Client, header::CONTENT_TYPE, redirect::Policy};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::commands::{validate_non_empty, CommandError, CommandResult};

pub const AUTONOMOUS_TOOL_WEB_SEARCH: &str = "web_search";
pub const AUTONOMOUS_TOOL_WEB_FETCH: &str = "web_fetch";

const SEARCH_PROVIDER_URL_ENV: &str = "CADENCE_AUTONOMOUS_WEB_SEARCH_URL";
const SEARCH_PROVIDER_BEARER_TOKEN_ENV: &str = "CADENCE_AUTONOMOUS_WEB_SEARCH_BEARER_TOKEN";
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
            search_provider: search_provider_from_env(),
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
pub struct ReqwestAutonomousWebTransport;

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

    pub fn search(
        &self,
        request: AutonomousWebSearchRequest,
    ) -> CommandResult<AutonomousWebSearchOutput> {
        validate_non_empty(&request.query, "query")?;
        if request.query.chars().count() > self.config.limits.max_search_query_chars {
            return Err(CommandError::user_fixable(
                "autonomous_web_search_query_too_large",
                format!(
                    "Cadence requires web search queries to be {} characters or fewer.",
                    self.config.limits.max_search_query_chars
                ),
            ));
        }

        let result_count = normalize_bounded_usize(
            request.result_count,
            self.config.limits.default_search_result_count,
            self.config.limits.max_search_result_count,
            "autonomous_web_search_result_count_invalid",
            "web search resultCount",
        )?;
        let timeout_ms = normalize_timeout_ms(
            request.timeout_ms,
            self.config.limits.default_timeout_ms,
            self.config.limits.max_timeout_ms,
            "autonomous_web_search_timeout_invalid",
            "web search timeout_ms",
        )?;

        let provider = self.config.search_provider.as_ref().ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_web_search_provider_unavailable",
                "Cadence cannot execute `web_search` because no backend search provider is configured.",
            )
        })?;

        let mut url = parse_http_url(
            &provider.endpoint,
            "autonomous_web_search_provider_config_invalid",
            "Cadence requires the configured autonomous web search provider endpoint to be a valid absolute HTTP or HTTPS URL.",
        )?;
        url.query_pairs_mut()
            .append_pair("q", request.query.trim())
            .append_pair("limit", &result_count.to_string());

        let mut headers = Vec::new();
        if let Some(token) = provider
            .bearer_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            headers.push(("Authorization".into(), format!("Bearer {token}")));
        }

        let response = self.execute_transport(AutonomousWebTransportRequest {
            url: url.to_string(),
            headers,
            timeout_ms,
            max_response_bytes: self.config.limits.max_response_bytes,
        })?;

        if !is_success_status(response.status) {
            return Err(map_search_status_error(response.status));
        }
        if response.body_truncated {
            return Err(CommandError::user_fixable(
                "autonomous_web_search_response_too_large",
                format!(
                    "Cadence refused the configured web search provider response because it exceeded the {} byte body limit.",
                    self.config.limits.max_response_bytes
                ),
            ));
        }

        let body = decode_utf8_body(
            &response.body,
            false,
            "autonomous_web_search_decode_failed",
            "Cadence could not decode the configured web search provider response as UTF-8 text.",
        )?;
        let decoded: SearchProviderResponse = serde_json::from_str(&body).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_web_search_decode_failed",
                format!(
                    "Cadence could not decode the configured web search provider payload: {error}"
                ),
            )
        })?;

        let mut results = Vec::new();
        let mut truncated = decoded.results.len() > result_count;
        for result in decoded.results.iter().take(result_count) {
            let title = normalize_non_empty_text(
                &result.title,
                "autonomous_web_search_decode_failed",
                "Cadence rejected a web search result with a blank title.",
            )?;
            let normalized_url = parse_http_url(
                &result.url,
                "autonomous_web_search_decode_failed",
                "Cadence rejected a web search result with an unsupported URL.",
            )?
            .to_string();
            let snippet = result
                .snippet
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(decode_html_entities);
            let (title, title_truncated) =
                truncate_chars_with_flag(&title, self.config.limits.max_title_chars);
            let (snippet, snippet_truncated) = match snippet {
                Some(value) => {
                    let (value, was_truncated) =
                        truncate_chars_with_flag(&value, self.config.limits.max_snippet_chars);
                    (Some(value), was_truncated)
                }
                None => (None, false),
            };
            truncated |= title_truncated || snippet_truncated;
            results.push(AutonomousWebSearchResult {
                title,
                url: normalized_url,
                snippet,
            });
        }

        Ok(AutonomousWebSearchOutput {
            query: request.query,
            results,
            truncated,
        })
    }

    pub fn fetch(
        &self,
        request: AutonomousWebFetchRequest,
    ) -> CommandResult<AutonomousWebFetchOutput> {
        validate_non_empty(&request.url, "url")?;
        let url = parse_http_url(
            &request.url,
            "autonomous_web_fetch_url_invalid",
            "Cadence requires `web_fetch` URLs to be valid absolute HTTP or HTTPS URLs.",
        )?;
        let timeout_ms = normalize_timeout_ms(
            request.timeout_ms,
            self.config.limits.default_timeout_ms,
            self.config.limits.max_timeout_ms,
            "autonomous_web_fetch_timeout_invalid",
            "web fetch timeout_ms",
        )?;
        let max_chars = normalize_bounded_usize(
            request.max_chars,
            self.config.limits.default_fetch_max_chars,
            self.config.limits.max_fetch_max_chars,
            "autonomous_web_fetch_max_chars_invalid",
            "web fetch maxChars",
        )?;

        let response = self.execute_transport(AutonomousWebTransportRequest {
            url: url.to_string(),
            headers: Vec::new(),
            timeout_ms,
            max_response_bytes: self.config.limits.max_response_bytes,
        })?;

        if !is_success_status(response.status) {
            return Err(map_fetch_status_error(response.status));
        }

        let content_type = normalize_content_type(response.content_type.as_deref());
        let content_kind = classify_fetch_content_kind(content_type.as_deref())?;
        let decoded_body = decode_utf8_body(
            &response.body,
            response.body_truncated,
            "autonomous_web_fetch_decode_failed",
            "Cadence could not decode the fetched response body as UTF-8 text.",
        )?;

        let (title, extracted_content) = match content_kind {
            AutonomousWebFetchContentKind::Html => (
                extract_html_title(&decoded_body),
                extract_html_text(&decoded_body),
            ),
            AutonomousWebFetchContentKind::PlainText => {
                (None, normalize_extracted_text(&decoded_body))
            }
        };

        let (content, content_truncated) = truncate_chars_with_flag(&extracted_content, max_chars);
        Ok(AutonomousWebFetchOutput {
            url: request.url,
            final_url: response.final_url,
            content_type,
            content_kind,
            title,
            content,
            truncated: response.body_truncated || content_truncated,
        })
    }

    fn execute_transport(
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

fn search_provider_from_env() -> Option<AutonomousWebSearchProviderConfig> {
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

fn map_search_status_error(status: u16) -> CommandError {
    match status {
        401 | 403 => CommandError::user_fixable(
            "autonomous_web_search_provider_rejected",
            format!("Cadence received HTTP {status} from the configured web search provider."),
        ),
        408 | 429 => CommandError::retryable(
            "autonomous_web_search_rate_limited",
            format!("Cadence received HTTP {status} from the configured web search provider."),
        ),
        500..=599 => CommandError::retryable(
            "autonomous_web_search_provider_unavailable",
            format!("Cadence received HTTP {status} from the configured web search provider."),
        ),
        _ => CommandError::user_fixable(
            "autonomous_web_search_status_error",
            format!("Cadence received HTTP {status} from the configured web search provider."),
        ),
    }
}

fn map_fetch_status_error(status: u16) -> CommandError {
    match status {
        408 | 429 => CommandError::retryable(
            "autonomous_web_fetch_rate_limited",
            format!("Cadence received HTTP {status} while fetching the requested URL."),
        ),
        500..=599 => CommandError::retryable(
            "autonomous_web_fetch_provider_unavailable",
            format!("Cadence received HTTP {status} while fetching the requested URL."),
        ),
        _ => CommandError::user_fixable(
            "autonomous_web_fetch_status_error",
            format!("Cadence received HTTP {status} while fetching the requested URL."),
        ),
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
                "Cadence requires `web_fetch` URLs to use the HTTP or HTTPS scheme."
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
            format!("Cadence requires {label} to be between 1 and {max_timeout_ms}."),
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
            format!("Cadence requires {label} to be between 1 and {max_value}."),
        ));
    }
    Ok(value)
}

fn normalize_non_empty_text(
    value: &str,
    error_code: &'static str,
    message: &'static str,
) -> CommandResult<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(CommandError::user_fixable(error_code, message));
    }
    Ok(decode_html_entities(normalized))
}

fn normalize_content_type(content_type: Option<&str>) -> Option<String> {
    content_type.and_then(|value| {
        let normalized = value
            .split(';')
            .next()
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    })
}

fn classify_fetch_content_kind(
    content_type: Option<&str>,
) -> CommandResult<AutonomousWebFetchContentKind> {
    match content_type {
        Some("text/html") | Some("application/xhtml+xml") => {
            Ok(AutonomousWebFetchContentKind::Html)
        }
        Some("text/plain") | None => Ok(AutonomousWebFetchContentKind::PlainText),
        Some(other) => Err(CommandError::user_fixable(
            "autonomous_web_fetch_content_type_unsupported",
            format!(
                "Cadence only supports `text/plain`, `text/html`, or `application/xhtml+xml` responses for `web_fetch`; received `{other}`."
            ),
        )),
    }
}

fn decode_utf8_body(
    body: &[u8],
    was_truncated: bool,
    error_code: &'static str,
    message: &'static str,
) -> CommandResult<String> {
    match std::str::from_utf8(body) {
        Ok(value) => Ok(value.to_string()),
        Err(error) if was_truncated && error.valid_up_to() > 0 => {
            Ok(std::str::from_utf8(&body[..error.valid_up_to()])
                .unwrap_or_default()
                .to_string())
        }
        Err(_) => Err(CommandError::user_fixable(error_code, message)),
    }
}

fn truncate_chars_with_flag(value: &str, max_chars: usize) -> (String, bool) {
    if max_chars == 0 {
        return (String::new(), !value.is_empty());
    }

    let truncated = value.chars().count() > max_chars;
    let truncated_value = value.chars().take(max_chars).collect::<String>();
    (truncated_value, truncated)
}

fn normalize_extracted_text(value: &str) -> String {
    let mut normalized_lines = Vec::new();
    let mut previous_blank = false;

    for raw_line in value.lines() {
        let line = raw_line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            if !previous_blank && !normalized_lines.is_empty() {
                normalized_lines.push(String::new());
            }
            previous_blank = true;
            continue;
        }

        previous_blank = false;
        normalized_lines.push(line);
    }

    normalized_lines.join("\n").trim().to_string()
}

fn extract_html_title(html: &str) -> Option<String> {
    let lowercase = html.to_ascii_lowercase();
    let title_start = lowercase.find("<title")?;
    let after_open = html[title_start..].find('>')? + title_start + 1;
    let title_end = lowercase[after_open..].find("</title>")? + after_open;
    let title = decode_html_entities(&html[after_open..title_end]);
    let title = normalize_extracted_text(&title);
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn extract_html_text(html: &str) -> String {
    let lowercase = html.to_ascii_lowercase();
    let mut cursor = 0;
    let mut extracted = String::new();

    while let Some(relative_lt) = html[cursor..].find('<') {
        let lt = cursor + relative_lt;
        extracted.push_str(&decode_html_entities(&html[cursor..lt]));

        let Some(relative_gt) = html[lt..].find('>') else {
            cursor = lt;
            break;
        };
        let gt = lt + relative_gt;
        let raw_tag = html[lt + 1..gt].trim();
        let tag_name = raw_tag
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();

        if tag_name == "script" || tag_name == "style" {
            let closing_tag = format!("</{tag_name}>");
            if let Some(relative_end) = lowercase[gt + 1..].find(&closing_tag) {
                cursor = gt + 1 + relative_end + closing_tag.len();
                continue;
            }

            break;
        }

        if matches!(
            tag_name.as_str(),
            "br" | "p"
                | "div"
                | "li"
                | "tr"
                | "td"
                | "th"
                | "section"
                | "article"
                | "header"
                | "footer"
                | "main"
                | "aside"
                | "nav"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "title"
        ) {
            extracted.push('\n');
        }

        cursor = gt + 1;
    }

    if cursor < html.len() {
        extracted.push_str(&decode_html_entities(&html[cursor..]));
    }

    normalize_extracted_text(&extracted)
}

fn decode_html_entities(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    let mut decoded = String::with_capacity(value.len());
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != '&' {
            decoded.push(chars[index]);
            index += 1;
            continue;
        }

        let mut end = index + 1;
        while end < chars.len() && end - index <= 10 && chars[end] != ';' {
            end += 1;
        }

        if end < chars.len() && chars[end] == ';' {
            let entity = chars[index + 1..end].iter().collect::<String>();
            if let Some(character) = decode_html_entity(&entity) {
                decoded.push(character);
                index = end + 1;
                continue;
            }
        }

        decoded.push(chars[index]);
        index += 1;
    }

    decoded
}

fn decode_html_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            if let Some(hex) = entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
            {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(decimal) = entity.strip_prefix('#') {
                decimal.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                None
            }
        }
    }
}

fn is_success_status(status: u16) -> bool {
    (200..=299).contains(&status)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchProviderResponse {
    results: Vec<SearchProviderResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchProviderResult {
    title: String,
    url: String,
    snippet: Option<String>,
}
