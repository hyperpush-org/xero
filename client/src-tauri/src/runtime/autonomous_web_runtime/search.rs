use serde::Deserialize;

use crate::commands::{validate_non_empty, CommandError, CommandResult};

use super::{
    extract::{decode_html_entities, decode_utf8_body, truncate_chars_with_flag},
    is_success_status, normalize_bounded_usize, normalize_timeout_ms, parse_http_url,
    transport::AutonomousWebTransportRequest,
    AutonomousWebRuntime, AutonomousWebSearchOutput, AutonomousWebSearchRequest,
    AutonomousWebSearchResult,
};

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
                "Xero cannot execute `web_search` because no backend search provider is configured.",
            )
        })?;

        let mut url = parse_http_url(
            &provider.endpoint,
            "autonomous_web_search_provider_config_invalid",
            "Xero requires the configured autonomous web search provider endpoint to be a valid absolute HTTP or HTTPS URL.",
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
        let decoded: SearchProviderResponse = serde_json::from_str(&body).map_err(|error| {
            CommandError::user_fixable(
                "autonomous_web_search_decode_failed",
                format!(
                    "Xero could not decode the configured web search provider payload: {error}"
                ),
            )
        })?;

        let mut results = Vec::new();
        let mut truncated = decoded.results.len() > result_count;
        for result in decoded.results.iter().take(result_count) {
            let title = normalize_non_empty_text(
                &result.title,
                "autonomous_web_search_decode_failed",
                "Xero rejected a web search result with a blank title.",
            )?;
            let normalized_url = parse_http_url(
                &result.url,
                "autonomous_web_search_decode_failed",
                "Xero rejected a web search result with an unsupported URL.",
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
}

fn map_search_status_error(status: u16) -> CommandError {
    match status {
        401 | 403 => CommandError::user_fixable(
            "autonomous_web_search_provider_rejected",
            format!("Xero received HTTP {status} from the configured web search provider."),
        ),
        408 | 429 => CommandError::retryable(
            "autonomous_web_search_rate_limited",
            format!("Xero received HTTP {status} from the configured web search provider."),
        ),
        500..=599 => CommandError::retryable(
            "autonomous_web_search_provider_unavailable",
            format!("Xero received HTTP {status} from the configured web search provider."),
        ),
        _ => CommandError::user_fixable(
            "autonomous_web_search_status_error",
            format!("Xero received HTTP {status} from the configured web search provider."),
        ),
    }
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
