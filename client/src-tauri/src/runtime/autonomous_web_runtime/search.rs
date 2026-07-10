use std::collections::BTreeSet;

use serde_json::{json, Map as JsonMap, Value as JsonValue};
use url::Url;

use crate::commands::{validate_non_empty, CommandError, CommandResult};

use super::{
    extract::{decode_html_entities, decode_utf8_body, truncate_chars_with_flag},
    is_success_status, normalize_bounded_usize, normalize_timeout_ms, parse_http_url,
    transport::{AutonomousWebHttpMethod, AutonomousWebTransportRequest},
    AutonomousWebRuntime, AutonomousWebRuntimeLimits, AutonomousWebSearchMode,
    AutonomousWebSearchOutput, AutonomousWebSearchProviderConfig, AutonomousWebSearchProviderKind,
    AutonomousWebSearchRequest, AutonomousWebSearchResult,
};

const BRAVE_SEARCH_URL: &str = "https://api.search.brave.com/res/v1/web/search";
const TAVILY_SEARCH_URL: &str = "https://api.tavily.com/search";
const EXA_SEARCH_URL: &str = "https://api.exa.ai/search";
const FIRECRAWL_SEARCH_URL: &str = "https://api.firecrawl.dev/v2/search";
const YOU_SEARCH_URL: &str = "https://api.ydc-index.io/v1/search";
const LINKUP_SEARCH_URL: &str = "https://api.linkup.so/v1/search";
const KAGI_SEARCH_URL: &str = "https://kagi.com/api/v1/search";
const SERPAPI_GOOGLE_URL: &str = "https://serpapi.com/search.json";
const SEARCHAPI_GOOGLE_URL: &str = "https://www.searchapi.io/api/v1/search";
const GOOGLE_CSE_URL: &str = "https://www.googleapis.com/customsearch/v1";

impl AutonomousWebRuntime {
    pub fn search(
        &self,
        request: AutonomousWebSearchRequest,
    ) -> CommandResult<AutonomousWebSearchOutput> {
        validate_non_empty(&request.query, "query")?;
        if request.query.chars().count() > self.config.limits.max_search_query_chars {
            return Err(CommandError::user_fixable(
                "autonomous_web_search_query_too_large",
                format!(
                    "Xero requires web search queries to be {} characters or fewer.",
                    self.config.limits.max_search_query_chars
                ),
            ));
        }

        if self.config.search_mode == AutonomousWebSearchMode::Disabled {
            return Err(CommandError::user_fixable(
                "autonomous_web_search_disabled",
                "Xero cannot execute `web_search` because Web Search is disabled in Settings.",
            ));
        }

        let configured_defaults = self.config.search_provider.as_ref();
        let default_result_count = configured_defaults
            .and_then(|provider| provider.result_limit)
            .unwrap_or(self.config.limits.default_search_result_count);
        let default_timeout_ms = configured_defaults
            .and_then(|provider| provider.timeout_ms)
            .or_else(|| {
                self.config
                    .managed_search
                    .as_ref()
                    .and_then(|managed| managed.timeout_ms)
            })
            .unwrap_or(self.config.limits.default_timeout_ms);

        let result_count = normalize_bounded_usize(
            request.result_count,
            default_result_count.min(self.config.limits.max_search_result_count),
            self.config.limits.max_search_result_count,
            "autonomous_web_search_result_count_invalid",
            "web search resultCount",
        )?;
        let timeout_ms = normalize_timeout_ms(
            request.timeout_ms,
            default_timeout_ms.min(self.config.limits.max_timeout_ms),
            self.config.limits.max_timeout_ms,
            "autonomous_web_search_timeout_invalid",
            "web search timeout_ms",
        )?;

        let output_query = request.query.clone();
        let query = request.query.trim();
        let managed_enabled = matches!(
            self.config.search_mode,
            AutonomousWebSearchMode::Auto | AutonomousWebSearchMode::ProviderManagedOnly
        );
        let configured_enabled = matches!(
            self.config.search_mode,
            AutonomousWebSearchMode::Auto | AutonomousWebSearchMode::ConfiguredProviderOnly
        );

        let mut managed_error = None;
        if managed_enabled {
            match self.config.managed_search.as_ref() {
                Some(managed) => match self.managed_search(
                    output_query.as_str(),
                    query,
                    result_count,
                    timeout_ms,
                ) {
                    Ok(output) if !output.results.is_empty() => return Ok(output),
                    Ok(_) => {
                        managed_error = Some(CommandError::retryable(
                            "autonomous_web_provider_managed_no_sources",
                            format!(
                                "Xero could not use {} because it returned no usable source URLs.",
                                managed.kind.as_str()
                            ),
                        ));
                    }
                    Err(error) => {
                        managed_error = Some(error);
                    }
                },
                None if self.config.search_mode == AutonomousWebSearchMode::ProviderManagedOnly => {
                    return Err(CommandError::user_fixable(
                        "autonomous_web_provider_managed_unavailable",
                        "Xero cannot execute `web_search` because the selected provider/model does not expose provider-managed web search.",
                    ));
                }
                None => {}
            }
        }

        if configured_enabled {
            if let Some(provider) = self.config.search_provider.as_ref() {
                return self.configured_provider_search(
                    provider,
                    output_query.as_str(),
                    query,
                    result_count,
                    timeout_ms,
                );
            }
        }

        if self.config.search_mode == AutonomousWebSearchMode::ProviderManagedOnly {
            return Err(managed_error.unwrap_or_else(|| {
                CommandError::user_fixable(
                    "autonomous_web_provider_managed_unavailable",
                    "Xero cannot execute `web_search` because provider-managed web search is unavailable for the selected model.",
                )
            }));
        }

        if self.config.search_mode == AutonomousWebSearchMode::ConfiguredProviderOnly {
            return Err(CommandError::user_fixable(
                "autonomous_web_search_provider_unavailable",
                "Xero cannot execute `web_search` because no enabled configured web-search provider is selected in Settings.",
            ));
        }

        Err(managed_error.unwrap_or_else(|| {
            CommandError::user_fixable(
                "autonomous_web_search_provider_unavailable",
                "Xero cannot execute `web_search` because Web Search has no ready provider-managed source or configured fallback provider in Settings.",
            )
        }))
    }

    fn configured_provider_search(
        &self,
        provider: &AutonomousWebSearchProviderConfig,
        output_query: &str,
        query: &str,
        result_count: usize,
        timeout_ms: u64,
    ) -> CommandResult<AutonomousWebSearchOutput> {
        if provider.kind.requires_api_key()
            && provider
                .api_key
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err(CommandError::user_fixable(
                "autonomous_web_search_api_key_missing",
                format!(
                    "Xero cannot execute `web_search` because `{}` has no saved API key.",
                    provider.display_name
                ),
            ));
        }

        let request = configured_provider_request(
            provider,
            query,
            result_count,
            timeout_ms,
            &self.config.limits,
        )?;
        let response = self.execute_transport(request)?;

        if !is_success_status(response.status) {
            return Err(map_search_status_error(
                response.status,
                provider.display_name.as_str(),
            ));
        }
        if response.body_truncated {
            return Err(CommandError::user_fixable(
                "autonomous_web_search_response_too_large",
                format!(
                    "Xero refused the configured web search provider response because it exceeded the {} byte body limit.",
                    self.config.limits.max_response_bytes
                ),
            ));
        }

        let body = decode_utf8_body(
            &response.body,
            false,
            "autonomous_web_search_decode_failed",
            "Xero could not decode the configured web search provider response as UTF-8 text.",
        )?;
        let decoded: JsonValue = serde_json::from_str(&body).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_web_search_decode_failed",
                format!(
                    "Xero could not decode the configured web search provider payload: {error}"
                ),
            )
        })?;
        let (results, truncated) =
            normalize_json_search_results(&decoded, result_count, &self.config.limits)?;

        Ok(AutonomousWebSearchOutput {
            query: output_query.to_owned(),
            results,
            truncated,
            source: Some(provider.source_label()),
        })
    }
}

fn configured_provider_request(
    provider: &AutonomousWebSearchProviderConfig,
    query: &str,
    result_count: usize,
    timeout_ms: u64,
    limits: &AutonomousWebRuntimeLimits,
) -> CommandResult<AutonomousWebTransportRequest> {
    match provider.kind {
        AutonomousWebSearchProviderKind::CustomEndpoint => {
            let endpoint = provider.endpoint.as_deref().ok_or_else(|| {
                CommandError::user_fixable(
                    "autonomous_web_search_provider_config_invalid",
                    "Xero requires custom web-search providers to have an endpoint URL.",
                )
            })?;
            let mut url = parse_provider_url(endpoint)?;
            url.query_pairs_mut()
                .append_pair("q", query)
                .append_pair("limit", &result_count.to_string());
            let mut headers = accept_json_headers();
            push_bearer_header(&mut headers, provider.api_key.as_deref());
            get_request(url, headers, timeout_ms, limits)
        }
        AutonomousWebSearchProviderKind::BraveSearch => {
            let mut url = parse_provider_url(provider_url(provider, BRAVE_SEARCH_URL))?;
            {
                let mut query_pairs = url.query_pairs_mut();
                query_pairs
                    .append_pair("q", query)
                    .append_pair("count", &result_count.to_string());
                if let Some(region) = provider.region.as_deref().filter(|value| !value.is_empty()) {
                    query_pairs.append_pair("country", region);
                }
                if let Some(language) = provider
                    .language
                    .as_deref()
                    .filter(|value| !value.is_empty())
                {
                    query_pairs.append_pair("search_lang", language);
                }
                if let Some(freshness) = provider
                    .freshness
                    .as_deref()
                    .filter(|value| !value.is_empty())
                {
                    query_pairs.append_pair("freshness", freshness);
                }
                if let Some(safe_search) = provider.safe_search {
                    query_pairs
                        .append_pair("safesearch", if safe_search { "strict" } else { "off" });
                }
            }
            let mut headers = accept_json_headers();
            push_required_header(
                &mut headers,
                "X-Subscription-Token",
                provider.api_key.as_deref(),
                provider.display_name.as_str(),
            )?;
            get_request(url, headers, timeout_ms, limits)
        }
        AutonomousWebSearchProviderKind::TavilySearch => {
            let headers =
                bearer_json_headers(provider.api_key.as_deref(), provider.display_name.as_str())?;
            let body = json!({
                "query": query,
                "max_results": result_count,
                "include_answer": false,
                "include_raw_content": false,
                "topic": "general",
            });
            post_json_request(
                provider_url(provider, TAVILY_SEARCH_URL),
                headers,
                body,
                timeout_ms,
                limits,
            )
        }
        AutonomousWebSearchProviderKind::ExaSearch => {
            let mut headers = json_headers();
            push_required_header(
                &mut headers,
                "x-api-key",
                provider.api_key.as_deref(),
                provider.display_name.as_str(),
            )?;
            let body = json!({
                "query": query,
                "numResults": result_count,
                "type": "auto",
            });
            post_json_request(
                provider_url(provider, EXA_SEARCH_URL),
                headers,
                body,
                timeout_ms,
                limits,
            )
        }
        AutonomousWebSearchProviderKind::FirecrawlSearch => {
            let headers =
                bearer_json_headers(provider.api_key.as_deref(), provider.display_name.as_str())?;
            let body = json!({
                "query": query,
                "limit": result_count,
                "scrapeOptions": {
                    "formats": []
                }
            });
            post_json_request(
                provider_url(provider, FIRECRAWL_SEARCH_URL),
                headers,
                body,
                timeout_ms,
                limits,
            )
        }
        AutonomousWebSearchProviderKind::YouSearch => {
            let mut url = parse_provider_url(provider_url(provider, YOU_SEARCH_URL))?;
            url.query_pairs_mut()
                .append_pair("query", query)
                .append_pair("num_web_results", &result_count.to_string());
            let mut headers = accept_json_headers();
            push_required_header(
                &mut headers,
                "X-API-Key",
                provider.api_key.as_deref(),
                provider.display_name.as_str(),
            )?;
            get_request(url, headers, timeout_ms, limits)
        }
        AutonomousWebSearchProviderKind::LinkupSearch => {
            let headers =
                bearer_json_headers(provider.api_key.as_deref(), provider.display_name.as_str())?;
            let body = json!({
                "q": query,
                "depth": provider.freshness.as_deref().unwrap_or("standard"),
                "outputType": "searchResults",
            });
            post_json_request(
                provider_url(provider, LINKUP_SEARCH_URL),
                headers,
                body,
                timeout_ms,
                limits,
            )
        }
        AutonomousWebSearchProviderKind::KagiSearch => {
            let mut url = parse_provider_url(provider_url(provider, KAGI_SEARCH_URL))?;
            url.query_pairs_mut()
                .append_pair("q", query)
                .append_pair("limit", &result_count.to_string());
            let mut headers = accept_json_headers();
            let token = provider
                .api_key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| missing_api_key_error(provider.display_name.as_str()))?;
            headers.push(("Authorization".into(), format!("Bot {token}")));
            get_request(url, headers, timeout_ms, limits)
        }
        AutonomousWebSearchProviderKind::SearxngJson => {
            let endpoint = provider_url(provider, "");
            if endpoint.trim().is_empty() {
                return Err(CommandError::user_fixable(
                    "autonomous_web_search_provider_config_invalid",
                    "Xero requires SearXNG providers to have an instance URL.",
                ));
            }
            let mut url = parse_provider_url(endpoint)?;
            if url.path().trim_matches('/').is_empty() {
                url.set_path("/search");
            }
            {
                let mut query_pairs = url.query_pairs_mut();
                query_pairs
                    .append_pair("q", query)
                    .append_pair("format", "json")
                    .append_pair("categories", "general");
                if let Some(language) = provider
                    .language
                    .as_deref()
                    .filter(|value| !value.is_empty())
                {
                    query_pairs.append_pair("language", language);
                }
                if let Some(safe_search) = provider.safe_search {
                    query_pairs.append_pair("safesearch", if safe_search { "1" } else { "0" });
                }
            }
            let mut headers = accept_json_headers();
            push_bearer_header(&mut headers, provider.api_key.as_deref());
            get_request(url, headers, timeout_ms, limits)
        }
        AutonomousWebSearchProviderKind::SerpapiGoogle => {
            let mut url = parse_provider_url(provider_url(provider, SERPAPI_GOOGLE_URL))?;
            {
                let mut query_pairs = url.query_pairs_mut();
                query_pairs
                    .append_pair("engine", "google")
                    .append_pair("q", query)
                    .append_pair("num", &result_count.to_string());
                let api_key =
                    required_api_key(provider.api_key.as_deref(), provider.display_name.as_str())?;
                query_pairs.append_pair("api_key", api_key);
                append_locale_query_pairs(&mut query_pairs, provider);
            }
            get_request(url, accept_json_headers(), timeout_ms, limits)
        }
        AutonomousWebSearchProviderKind::SearchapiGoogle => {
            let mut url = parse_provider_url(provider_url(provider, SEARCHAPI_GOOGLE_URL))?;
            {
                let mut query_pairs = url.query_pairs_mut();
                query_pairs
                    .append_pair("engine", "google")
                    .append_pair("q", query)
                    .append_pair("num", &result_count.to_string());
                let api_key =
                    required_api_key(provider.api_key.as_deref(), provider.display_name.as_str())?;
                query_pairs.append_pair("api_key", api_key);
                append_locale_query_pairs(&mut query_pairs, provider);
            }
            get_request(url, accept_json_headers(), timeout_ms, limits)
        }
        AutonomousWebSearchProviderKind::GoogleCse => {
            let mut url = parse_provider_url(provider_url(provider, GOOGLE_CSE_URL))?;
            {
                let mut query_pairs = url.query_pairs_mut();
                query_pairs
                    .append_pair("q", query)
                    .append_pair("num", &result_count.min(10).to_string());
                let api_key =
                    required_api_key(provider.api_key.as_deref(), provider.display_name.as_str())?;
                query_pairs.append_pair("key", api_key);
                let cx = provider
                    .google_cse_cx
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        CommandError::user_fixable(
                            "autonomous_web_search_provider_config_invalid",
                            "Xero requires Google CSE providers to include a search engine id (`cx`).",
                        )
                    })?;
                query_pairs.append_pair("cx", cx);
                if let Some(language) = provider
                    .language
                    .as_deref()
                    .filter(|value| !value.is_empty())
                {
                    query_pairs.append_pair("lr", language);
                }
                if let Some(safe_search) = provider.safe_search {
                    query_pairs.append_pair("safe", if safe_search { "active" } else { "off" });
                }
            }
            get_request(url, accept_json_headers(), timeout_ms, limits)
        }
    }
}

pub(super) fn normalize_json_search_results(
    value: &JsonValue,
    result_count: usize,
    limits: &AutonomousWebRuntimeLimits,
) -> CommandResult<(Vec<AutonomousWebSearchResult>, bool)> {
    let candidate_arrays = candidate_result_arrays(value);
    let mut candidates = Vec::new();
    if candidate_arrays.is_empty() {
        collect_result_like_objects(value, &mut candidates);
    } else {
        for array in candidate_arrays {
            candidates.extend(array.iter());
        }
    }

    let mut seen_urls = BTreeSet::new();
    let mut results = Vec::new();
    let mut truncated = candidates.len() > result_count;
    for candidate in &candidates {
        if results.len() >= result_count {
            break;
        }
        let Some(object) = candidate.as_object() else {
            continue;
        };
        let Some(raw_url) = object_string(object, URL_KEYS) else {
            continue;
        };
        let Ok(url) = parse_http_url(
            &raw_url,
            "autonomous_web_search_decode_failed",
            "Xero rejected a web search result with an unsupported URL.",
        ) else {
            continue;
        };
        let normalized_url = url.to_string();
        if !seen_urls.insert(normalized_url.clone()) {
            continue;
        }

        let title = object_string(object, TITLE_KEYS)
            .or_else(|| host_title(&url))
            .unwrap_or_else(|| normalized_url.clone());
        let snippet = object_string(object, SNIPPET_KEYS);
        let (title, title_truncated) =
            truncate_chars_with_flag(&decode_html_entities(title.trim()), limits.max_title_chars);
        let (snippet, snippet_truncated) = match snippet {
            Some(value) => {
                let (value, was_truncated) = truncate_chars_with_flag(
                    &decode_html_entities(value.trim()),
                    limits.max_snippet_chars,
                );
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

    if results.is_empty() && !candidates.is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_web_search_decode_failed",
            "Xero could not find any usable HTTP/HTTPS source URLs in the web search provider response.",
        ));
    }

    Ok((results, truncated))
}

pub(super) fn map_search_status_error(status: u16, provider_label: &str) -> CommandError {
    match status {
        401 | 403 => CommandError::user_fixable(
            "autonomous_web_search_provider_rejected",
            format!("Xero received HTTP {status} from {provider_label}."),
        ),
        408 | 429 => CommandError::retryable(
            "autonomous_web_search_rate_limited",
            format!("Xero received HTTP {status} from {provider_label}."),
        ),
        500..=599 => CommandError::retryable(
            "autonomous_web_search_provider_unavailable",
            format!("Xero received HTTP {status} from {provider_label}."),
        ),
        _ => CommandError::user_fixable(
            "autonomous_web_search_status_error",
            format!("Xero received HTTP {status} from {provider_label}."),
        ),
    }
}

fn provider_url<'a>(
    provider: &'a AutonomousWebSearchProviderConfig,
    default_url: &'a str,
) -> &'a str {
    provider
        .endpoint
        .as_deref()
        .or(provider.base_url.as_deref())
        .unwrap_or(default_url)
}

fn parse_provider_url(value: &str) -> CommandResult<Url> {
    parse_http_url(
        value,
        "autonomous_web_search_provider_config_invalid",
        "Xero requires web-search provider URLs to be valid absolute HTTP or HTTPS URLs.",
    )
}

fn get_request(
    url: Url,
    headers: Vec<(String, String)>,
    timeout_ms: u64,
    limits: &AutonomousWebRuntimeLimits,
) -> CommandResult<AutonomousWebTransportRequest> {
    Ok(AutonomousWebTransportRequest {
        method: AutonomousWebHttpMethod::Get,
        url: url.to_string(),
        headers,
        body: None,
        timeout_ms,
        max_response_bytes: limits.max_response_bytes,
    })
}

fn post_json_request(
    url: &str,
    headers: Vec<(String, String)>,
    body: JsonValue,
    timeout_ms: u64,
    limits: &AutonomousWebRuntimeLimits,
) -> CommandResult<AutonomousWebTransportRequest> {
    let url = parse_provider_url(url)?;
    let body = serde_json::to_vec(&body).map_err(|error| {
        CommandError::system_fault(
            "autonomous_web_search_request_encode_failed",
            format!("Xero could not encode the web-search provider request: {error}"),
        )
    })?;
    Ok(AutonomousWebTransportRequest {
        method: AutonomousWebHttpMethod::Post,
        url: url.to_string(),
        headers,
        body: Some(body),
        timeout_ms,
        max_response_bytes: limits.max_response_bytes,
    })
}

fn accept_json_headers() -> Vec<(String, String)> {
    vec![("Accept".into(), "application/json".into())]
}

fn json_headers() -> Vec<(String, String)> {
    vec![
        ("Accept".into(), "application/json".into()),
        ("Content-Type".into(), "application/json".into()),
    ]
}

fn bearer_json_headers(
    api_key: Option<&str>,
    provider_label: &str,
) -> CommandResult<Vec<(String, String)>> {
    let token = api_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| missing_api_key_error(provider_label))?;
    let mut headers = json_headers();
    headers.push(("Authorization".into(), format!("Bearer {token}")));
    Ok(headers)
}

fn push_bearer_header(headers: &mut Vec<(String, String)>, api_key: Option<&str>) {
    if let Some(token) = api_key.map(str::trim).filter(|value| !value.is_empty()) {
        headers.push(("Authorization".into(), format!("Bearer {token}")));
    }
}

fn push_required_header(
    headers: &mut Vec<(String, String)>,
    name: &str,
    value: Option<&str>,
    provider_label: &str,
) -> CommandResult<()> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| missing_api_key_error(provider_label))?;
    headers.push((name.into(), value.into()));
    Ok(())
}

fn append_locale_query_pairs<T: url::form_urlencoded::Target>(
    query_pairs: &mut url::form_urlencoded::Serializer<'_, T>,
    provider: &AutonomousWebSearchProviderConfig,
) {
    if let Some(region) = provider.region.as_deref().filter(|value| !value.is_empty()) {
        query_pairs.append_pair("gl", region);
    }
    if let Some(language) = provider
        .language
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        query_pairs.append_pair("hl", language);
    }
    if let Some(safe_search) = provider.safe_search {
        query_pairs.append_pair("safe", if safe_search { "active" } else { "off" });
    }
}

fn required_api_key<'a>(api_key: Option<&'a str>, provider_label: &str) -> CommandResult<&'a str> {
    api_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| missing_api_key_error(provider_label))
}

fn missing_api_key_error(provider_label: &str) -> CommandError {
    CommandError::user_fixable(
        "autonomous_web_search_api_key_missing",
        format!(
            "Xero cannot execute `web_search` because `{provider_label}` has no saved API key."
        ),
    )
}

fn candidate_result_arrays(value: &JsonValue) -> Vec<&Vec<JsonValue>> {
    const POINTERS: &[&str] = &[
        "/results",
        "/web/results",
        "/organic_results",
        "/items",
        "/data",
        "/sources",
        "/hits",
        "/documents",
        "/choices/0/message/annotations",
        "/output/0/content/0/annotations",
    ];

    POINTERS
        .iter()
        .filter_map(|pointer| value.pointer(pointer).and_then(JsonValue::as_array))
        .collect()
}

fn collect_result_like_objects<'a>(value: &'a JsonValue, output: &mut Vec<&'a JsonValue>) {
    match value {
        JsonValue::Object(object) => {
            if object_string(object, URL_KEYS).is_some() {
                output.push(value);
            }
            for (key, child) in object {
                if should_skip_recursive_key(key) {
                    continue;
                }
                collect_result_like_objects(child, output);
            }
        }
        JsonValue::Array(array) => {
            for child in array {
                collect_result_like_objects(child, output);
            }
        }
        _ => {}
    }
}

fn should_skip_recursive_key(key: &str) -> bool {
    matches!(
        key,
        "search_metadata" | "searchParameters" | "search_parameters" | "usage" | "request"
    )
}

const URL_KEYS: &[&str] = &[
    "url",
    "uri",
    "link",
    "href",
    "source_url",
    "sourceUrl",
    "displayed_link",
];
const TITLE_KEYS: &[&str] = &["title", "name", "source", "site_name", "siteName"];
const SNIPPET_KEYS: &[&str] = &[
    "snippet",
    "description",
    "content",
    "text",
    "body",
    "summary",
    "markdown",
    "highlight",
];

fn object_string(object: &JsonMap<String, JsonValue>, keys: &[&str]) -> Option<String> {
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        if let Some(text) = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(text.to_owned());
        }
        if let Some(array) = value.as_array() {
            let joined = array
                .iter()
                .filter_map(|item| item.as_str().map(str::trim))
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if !joined.is_empty() {
                return Some(joined);
            }
        }
    }
    None
}

fn host_title(url: &Url) -> Option<String> {
    url.host_str()
        .map(|host| host.trim_start_matches("www.").to_owned())
        .filter(|host| !host.is_empty())
}
