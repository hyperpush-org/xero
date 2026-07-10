mod extract;
mod fetch;
mod managed;
mod search;
mod transport;

use std::{fmt, sync::Arc};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::commands::{CommandError, CommandResult};

pub use transport::{
    AutonomousWebHttpMethod, AutonomousWebTransport, AutonomousWebTransportError,
    AutonomousWebTransportRequest, AutonomousWebTransportResponse,
};

pub const AUTONOMOUS_TOOL_WEB_SEARCH: &str = "web_search";
pub const AUTONOMOUS_TOOL_WEB_FETCH: &str = "web_fetch";

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousWebSearchMode {
    #[default]
    Auto,
    ProviderManagedOnly,
    ConfiguredProviderOnly,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousWebSearchProviderKind {
    CustomEndpoint,
    BraveSearch,
    TavilySearch,
    ExaSearch,
    FirecrawlSearch,
    YouSearch,
    LinkupSearch,
    KagiSearch,
    SearxngJson,
    SerpapiGoogle,
    SearchapiGoogle,
    GoogleCse,
}

impl AutonomousWebSearchProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CustomEndpoint => "custom_endpoint",
            Self::BraveSearch => "brave_search",
            Self::TavilySearch => "tavily_search",
            Self::ExaSearch => "exa_search",
            Self::FirecrawlSearch => "firecrawl_search",
            Self::YouSearch => "you_search",
            Self::LinkupSearch => "linkup_search",
            Self::KagiSearch => "kagi_search",
            Self::SearxngJson => "searxng_json",
            Self::SerpapiGoogle => "serpapi_google",
            Self::SearchapiGoogle => "searchapi_google",
            Self::GoogleCse => "google_cse",
        }
    }

    pub fn requires_api_key(self) -> bool {
        !matches!(self, Self::CustomEndpoint | Self::SearxngJson)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AutonomousWebSearchProviderConfig {
    pub profile_id: String,
    pub kind: AutonomousWebSearchProviderKind,
    pub display_name: String,
    pub endpoint: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub google_cse_cx: Option<String>,
    pub result_limit: Option<usize>,
    pub timeout_ms: Option<u64>,
    pub region: Option<String>,
    pub language: Option<String>,
    pub freshness: Option<String>,
    pub safe_search: Option<bool>,
}

impl AutonomousWebSearchProviderConfig {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            profile_id: "custom_endpoint".into(),
            kind: AutonomousWebSearchProviderKind::CustomEndpoint,
            display_name: "Custom endpoint".into(),
            endpoint: Some(endpoint.into()),
            base_url: None,
            api_key: None,
            google_cse_cx: None,
            result_limit: None,
            timeout_ms: None,
            region: None,
            language: None,
            freshness: None,
            safe_search: None,
        }
    }

    pub fn with_bearer_token(mut self, bearer_token: impl Into<String>) -> Self {
        self.api_key = Some(bearer_token.into());
        self
    }

    pub fn source_label(&self) -> String {
        format!("configured_provider:{}", self.profile_id)
    }
}

impl fmt::Debug for AutonomousWebSearchProviderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AutonomousWebSearchProviderConfig")
            .field("profile_id", &self.profile_id)
            .field("kind", &self.kind)
            .field("display_name", &self.display_name)
            .field("endpoint", &self.endpoint)
            .field("base_url", &self.base_url)
            .field("google_cse_cx", &self.google_cse_cx)
            .field("result_limit", &self.result_limit)
            .field("timeout_ms", &self.timeout_ms)
            .field("region", &self.region)
            .field("language", &self.language)
            .field("freshness", &self.freshness)
            .field("safe_search", &self.safe_search)
            .field(
                "has_api_key",
                &self
                    .api_key
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomousWebManagedSearchKind {
    AnthropicNativeWebSearch,
    GeminiGroundingGoogleSearch,
    OpenAiNativeWebSearch,
    OpenRouterServerWebSearch,
    XaiNativeWebSearch,
}

impl AutonomousWebManagedSearchKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AnthropicNativeWebSearch => "anthropic_native_web_search",
            Self::GeminiGroundingGoogleSearch => "gemini_grounding_google_search",
            Self::OpenAiNativeWebSearch => "openai_native_web_search",
            Self::OpenRouterServerWebSearch => "openrouter_server_web_search",
            Self::XaiNativeWebSearch => "xai_native_web_search",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct AutonomousWebManagedSearchConfig {
    pub kind: AutonomousWebManagedSearchKind,
    pub provider_id: String,
    pub model_id: String,
    pub base_url: String,
    pub api_key: String,
    pub account_id: Option<String>,
    pub session_id: Option<String>,
    pub api_version: Option<String>,
    pub timeout_ms: Option<u64>,
}

impl AutonomousWebManagedSearchConfig {
    pub fn source_label(&self) -> String {
        format!("provider_managed:{}", self.provider_id)
    }
}

impl fmt::Debug for AutonomousWebManagedSearchConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AutonomousWebManagedSearchConfig")
            .field("kind", &self.kind)
            .field("provider_id", &self.provider_id)
            .field("model_id", &self.model_id)
            .field("base_url", &self.base_url)
            .field("api_version", &self.api_version)
            .field("timeout_ms", &self.timeout_ms)
            .field("has_api_key", &!self.api_key.trim().is_empty())
            .field(
                "has_account_id",
                &self
                    .account_id
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
            )
            .field(
                "has_session_id",
                &self
                    .session_id
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty()),
            )
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutonomousWebConfig {
    pub search_mode: AutonomousWebSearchMode,
    pub managed_search: Option<AutonomousWebManagedSearchConfig>,
    pub search_provider: Option<AutonomousWebSearchProviderConfig>,
    pub limits: AutonomousWebRuntimeLimits,
}

impl AutonomousWebConfig {
    pub fn for_platform() -> Self {
        Self {
            search_mode: AutonomousWebSearchMode::default(),
            managed_search: None,
            search_provider: None,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone)]
    struct StaticTransport {
        response: AutonomousWebTransportResponse,
        last_request: Arc<Mutex<Option<AutonomousWebTransportRequest>>>,
    }

    impl AutonomousWebTransport for StaticTransport {
        fn execute(
            &self,
            request: &AutonomousWebTransportRequest,
        ) -> Result<AutonomousWebTransportResponse, AutonomousWebTransportError> {
            *self.last_request.lock().expect("transport request lock") = Some(request.clone());
            Ok(self.response.clone())
        }
    }

    fn runtime_with_response(
        config: AutonomousWebConfig,
        response: AutonomousWebTransportResponse,
    ) -> (
        AutonomousWebRuntime,
        Arc<Mutex<Option<AutonomousWebTransportRequest>>>,
    ) {
        let last_request = Arc::new(Mutex::new(None));
        let transport = Arc::new(StaticTransport {
            response,
            last_request: Arc::clone(&last_request),
        });
        (
            AutonomousWebRuntime::with_transport(config, transport),
            last_request,
        )
    }

    #[test]
    fn search_uses_configured_provider_and_normalizes_results() {
        let response = AutonomousWebTransportResponse {
            status: 200,
            final_url: "https://search.example.test/api?q=Rust&limit=1".into(),
            content_type: Some("application/json".into()),
            body: br#"{"results":[{"title":" &lt;Rust&gt; docs ","url":"https://www.rust-lang.org/learn","snippet":"Current &amp; stable"},{"title":"Second","url":"https://example.com/second","snippet":null}]}"#.to_vec(),
            body_truncated: false,
        };
        let config = AutonomousWebConfig {
            search_provider: Some(
                AutonomousWebSearchProviderConfig::new("https://search.example.test/api")
                    .with_bearer_token("test-token"),
            ),
            ..Default::default()
        };
        let (runtime, last_request) = runtime_with_response(config, response);

        let output = runtime
            .search(AutonomousWebSearchRequest {
                query: " Rust web ".into(),
                result_count: Some(1),
                timeout_ms: Some(5_000),
            })
            .expect("web search output");

        assert_eq!(output.query, " Rust web ");
        assert!(output.truncated);
        assert_eq!(output.results.len(), 1);
        assert_eq!(output.results[0].title, "<Rust> docs");
        assert_eq!(output.results[0].url, "https://www.rust-lang.org/learn");
        assert_eq!(
            output.results[0].snippet.as_deref(),
            Some("Current & stable")
        );

        let request = last_request
            .lock()
            .expect("transport request lock")
            .clone()
            .expect("transport request");
        let url = Url::parse(&request.url).expect("provider url");
        let query_pairs = url.query_pairs().collect::<Vec<_>>();
        assert!(query_pairs
            .iter()
            .any(|(key, value)| key == "q" && value == "Rust web"));
        assert!(query_pairs
            .iter()
            .any(|(key, value)| key == "limit" && value == "1"));
        assert_eq!(request.timeout_ms, 5_000);
        assert_eq!(
            request.headers,
            vec![
                ("Accept".into(), "application/json".into()),
                ("Authorization".into(), "Bearer test-token".into()),
            ]
        );
    }

    #[test]
    fn search_uses_openai_codex_provider_managed_request_contract() {
        let response = AutonomousWebTransportResponse {
            status: 200,
            final_url: "https://chatgpt.com/backend-api/codex/responses".into(),
            content_type: Some("text/event-stream".into()),
            body: br#"event: response.output_text.done
data: {"type":"response.output_text.done","text":"Official source: https://tauri.app/blog/tauri-2-0/"}

event: response.completed
data: {"type":"response.completed","response":{"status":"completed"}}

"#
            .to_vec(),
            body_truncated: false,
        };
        let config = AutonomousWebConfig {
            managed_search: Some(AutonomousWebManagedSearchConfig {
                kind: AutonomousWebManagedSearchKind::OpenAiNativeWebSearch,
                provider_id: "openai_codex".into(),
                model_id: "gpt-5.5".into(),
                base_url: "https://chatgpt.com/backend-api".into(),
                api_key: "codex-access-token".into(),
                account_id: Some("account-1".into()),
                session_id: Some("session-1".into()),
                api_version: None,
                timeout_ms: None,
            }),
            ..Default::default()
        };
        let (runtime, last_request) = runtime_with_response(config, response);

        let output = runtime
            .search(AutonomousWebSearchRequest {
                query: "tauri v2 OTA example".into(),
                result_count: Some(3),
                timeout_ms: Some(5_000),
            })
            .expect("managed web search output");

        assert_eq!(output.results.len(), 1);
        assert_eq!(
            output.source.as_deref(),
            Some("provider_managed:openai_codex")
        );
        assert_eq!(output.results[0].url, "https://tauri.app/blog/tauri-2-0/");

        let request = last_request
            .lock()
            .expect("transport request lock")
            .clone()
            .expect("transport request");
        assert_eq!(
            request.url,
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(
            header_value(&request.headers, "Authorization"),
            Some("Bearer codex-access-token")
        );
        assert_eq!(
            header_value(&request.headers, "Accept"),
            Some("text/event-stream")
        );
        assert_eq!(
            header_value(&request.headers, "chatgpt-account-id"),
            Some("account-1")
        );
        assert_eq!(
            header_value(&request.headers, "OpenAI-Beta"),
            Some("responses=experimental")
        );
        assert_eq!(header_value(&request.headers, "originator"), Some("pi"));
        assert_eq!(
            header_value(&request.headers, "session_id"),
            Some("session-1")
        );

        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_deref().expect("request body"))
                .expect("request json");
        assert_eq!(body["model"], "gpt-5.5");
        assert_eq!(body["stream"], true);
        assert!(body["instructions"]
            .as_str()
            .is_some_and(|value| value.contains("Search the web")));
        assert_eq!(body["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(body["text"]["verbosity"], "medium");
        assert_eq!(body["tools"][0]["type"], "web_search");
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["parallel_tool_calls"], true);
        assert_eq!(body["prompt_cache_key"], "session-1");
        assert_eq!(body["store"], false);
    }

    #[test]
    fn search_uses_azure_openai_provider_managed_request_contract() {
        let response = AutonomousWebTransportResponse {
            status: 200,
            final_url: "https://example-resource.openai.azure.com/openai/v1/responses".into(),
            content_type: Some("application/json".into()),
            body: br#"{"output":[{"type":"web_search_call","status":"completed"},{"type":"message","content":[{"type":"output_text","text":"Found it.","annotations":[{"type":"url_citation","url":"https://learn.microsoft.com/azure/ai-foundry/openai/how-to/web-search","title":"Web search"}]}]}]}"#.to_vec(),
            body_truncated: false,
        };
        let config = AutonomousWebConfig {
            managed_search: Some(AutonomousWebManagedSearchConfig {
                kind: AutonomousWebManagedSearchKind::OpenAiNativeWebSearch,
                provider_id: "azure_openai".into(),
                model_id: "gpt-4.1".into(),
                base_url: "https://example-resource.openai.azure.com/openai/v1".into(),
                api_key: "azure-key".into(),
                account_id: None,
                session_id: None,
                api_version: Some("2026-03-01-preview".into()),
                timeout_ms: None,
            }),
            ..Default::default()
        };
        let (runtime, last_request) = runtime_with_response(config, response);

        let output = runtime
            .search(AutonomousWebSearchRequest {
                query: "azure openai web search".into(),
                result_count: Some(3),
                timeout_ms: Some(5_000),
            })
            .expect("managed web search output");

        assert_eq!(
            output.source.as_deref(),
            Some("provider_managed:azure_openai")
        );
        assert_eq!(
            output.results[0].url,
            "https://learn.microsoft.com/azure/ai-foundry/openai/how-to/web-search"
        );

        let request = last_request
            .lock()
            .expect("transport request lock")
            .clone()
            .expect("transport request");
        assert_eq!(
            request.url,
            "https://example-resource.openai.azure.com/openai/v1/responses?api-version=2026-03-01-preview"
        );
        assert_eq!(header_value(&request.headers, "api-key"), Some("azure-key"));
        assert_eq!(header_value(&request.headers, "Authorization"), None);

        let body: serde_json::Value =
            serde_json::from_slice(request.body.as_deref().expect("request body"))
                .expect("request json");
        assert_eq!(body["tools"][0]["type"], "web_search");
        assert_eq!(body["tool_choice"], "required");
    }

    #[test]
    fn search_without_provider_returns_user_fixable_error() {
        let runtime = AutonomousWebRuntime::new(AutonomousWebConfig::default());

        let error = runtime
            .search(AutonomousWebSearchRequest {
                query: "current docs".into(),
                result_count: None,
                timeout_ms: None,
            })
            .expect_err("missing provider should fail");

        assert_eq!(error.code, "autonomous_web_search_provider_unavailable");
    }

    #[test]
    fn fetch_reads_http_text_without_search_provider() {
        let response = AutonomousWebTransportResponse {
            status: 200,
            final_url: "https://example.com/docs".into(),
            content_type: Some("text/html; charset=utf-8".into()),
            body: br#"<html><head><title>Example Docs</title></head><body><h1>Alpha</h1><p>Beta &amp; Gamma</p></body></html>"#.to_vec(),
            body_truncated: false,
        };
        let (runtime, last_request) =
            runtime_with_response(AutonomousWebConfig::default(), response);

        let output = runtime
            .fetch(AutonomousWebFetchRequest {
                url: "https://example.com/docs".into(),
                max_chars: Some(80),
                timeout_ms: Some(4_000),
            })
            .expect("web fetch output");

        assert_eq!(output.final_url, "https://example.com/docs");
        assert_eq!(output.content_type.as_deref(), Some("text/html"));
        assert_eq!(output.content_kind, AutonomousWebFetchContentKind::Html);
        assert_eq!(output.title.as_deref(), Some("Example Docs"));
        assert!(output.content.contains("Alpha"));
        assert!(output.content.contains("Beta & Gamma"));
        assert!(!output.truncated);

        let request = last_request
            .lock()
            .expect("transport request lock")
            .clone()
            .expect("transport request");
        assert_eq!(request.url, "https://example.com/docs");
        assert_eq!(request.timeout_ms, 4_000);
        assert!(request.headers.is_empty());
    }

    fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
        headers
            .iter()
            .find(|(header_name, _)| header_name == name)
            .map(|(_, value)| value.as_str())
    }
}
