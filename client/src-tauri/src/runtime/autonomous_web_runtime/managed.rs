use std::{process::Command, process::Stdio};

use serde_json::{json, Value as JsonValue};

use crate::commands::{CommandError, CommandResult};

use super::{
    extract::{decode_utf8_body, truncate_chars_with_flag},
    is_success_status,
    search::{map_search_status_error, normalize_json_search_results},
    transport::{AutonomousWebHttpMethod, AutonomousWebTransportRequest},
    AutonomousWebManagedSearchConfig, AutonomousWebManagedSearchKind, AutonomousWebRuntime,
    AutonomousWebSearchOutput,
};

const OPENAI_CODEX_PROVIDER_ID: &str = "openai_codex";
const AZURE_OPENAI_PROVIDER_ID: &str = "azure_openai";
const VERTEX_PROVIDER_ID: &str = "vertex";
const OPENAI_CODEX_BETA_HEADER: &str = "responses=experimental";
const OPENAI_CODEX_ORIGINATOR: &str = "pi";
const OPENAI_CODEX_TEXT_VERBOSITY: &str = "medium";

impl AutonomousWebRuntime {
    pub(super) fn managed_search(
        &self,
        output_query: &str,
        query: &str,
        result_count: usize,
        timeout_ms: u64,
    ) -> CommandResult<AutonomousWebSearchOutput> {
        let managed = self.config.managed_search.as_ref().ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_web_provider_managed_unavailable",
                "Xero cannot execute provider-managed web search because no active provider/model capability is configured.",
            )
        })?;
        if managed_search_requires_static_api_key(managed) && managed.api_key.trim().is_empty() {
            return Err(CommandError::user_fixable(
                "autonomous_web_provider_managed_credentials_missing",
                format!(
                    "Xero cannot execute provider-managed web search because `{}` has no usable provider credential.",
                    managed.provider_id
                ),
            ));
        }

        let request = managed_search_request(
            managed,
            query,
            result_count,
            timeout_ms,
            self.config.limits.max_response_bytes,
        )?;
        let response = self.execute_transport(request)?;
        let provider_label = managed.kind.as_str();
        if !is_success_status(response.status) {
            return Err(map_search_status_error(response.status, provider_label));
        }
        if response.body_truncated {
            return Err(CommandError::user_fixable(
                "autonomous_web_provider_managed_response_too_large",
                format!(
                    "Xero refused the provider-managed web search response because it exceeded the {} byte body limit.",
                    self.config.limits.max_response_bytes
                ),
            ));
        }

        let body = decode_utf8_body(
            &response.body,
            false,
            "autonomous_web_provider_managed_decode_failed",
            "Xero could not decode the provider-managed web search response as UTF-8 text.",
        )?;
        let decoded = if managed.provider_id == OPENAI_CODEX_PROVIDER_ID {
            openai_codex_sse_payload(&body)?
        } else {
            serde_json::from_str(&body).map_err(|error| {
                CommandError::user_fixable(
                    "autonomous_web_provider_managed_decode_failed",
                    format!(
                        "Xero could not decode the provider-managed web search payload: {error}"
                    ),
                )
            })?
        };
        let (mut results, truncated) =
            normalize_json_search_results(&decoded, result_count, &self.config.limits)?;

        if results.is_empty() {
            return Ok(AutonomousWebSearchOutput {
                query: output_query.to_owned(),
                results,
                truncated,
                source: Some(managed.source_label()),
            });
        }

        for result in &mut results {
            if result.snippet.is_none() {
                let (snippet, _) = truncate_chars_with_flag(
                    &format!(
                        "Source returned by {} for `{query}`.",
                        managed.kind.as_str()
                    ),
                    self.config.limits.max_snippet_chars,
                );
                result.snippet = Some(snippet);
            }
        }

        Ok(AutonomousWebSearchOutput {
            query: output_query.to_owned(),
            results,
            truncated,
            source: Some(managed.source_label()),
        })
    }
}

fn managed_search_request(
    config: &AutonomousWebManagedSearchConfig,
    query: &str,
    result_count: usize,
    timeout_ms: u64,
    max_response_bytes: usize,
) -> CommandResult<AutonomousWebTransportRequest> {
    let prompt = managed_search_prompt(query, result_count);
    let (url, headers, body) = match config.kind {
        AutonomousWebManagedSearchKind::AnthropicNativeWebSearch => {
            if config.provider_id == VERTEX_PROVIDER_ID {
                let body = json!({
                    "anthropic_version": config
                        .api_version
                        .clone()
                        .unwrap_or_else(|| "vertex-2023-10-16".into()),
                    "max_tokens": 1024,
                    "messages": [{
                        "role": "user",
                        "content": prompt,
                    }],
                    "tools": [{
                        "type": "web_search_20250305",
                        "name": "web_search",
                        "max_uses": 1,
                    }],
                });
                let headers = bearer_json_headers(&vertex_access_token()?);
                (config.base_url.clone(), headers, body)
            } else {
                let body = json!({
                    "model": config.model_id,
                    "max_tokens": 1024,
                    "messages": [{
                        "role": "user",
                        "content": prompt,
                    }],
                    "tools": [{
                        "type": "web_search_20250305",
                        "name": "web_search",
                        "max_uses": 1,
                    }],
                });
                let headers = vec![
                    ("Accept".into(), "application/json".into()),
                    ("Content-Type".into(), "application/json".into()),
                    ("x-api-key".into(), config.api_key.clone()),
                    (
                        "anthropic-version".into(),
                        config
                            .api_version
                            .clone()
                            .unwrap_or_else(|| "2023-06-01".into()),
                    ),
                ];
                (join_url(&config.base_url, "/v1/messages"), headers, body)
            }
        }
        AutonomousWebManagedSearchKind::GeminiGroundingGoogleSearch => {
            let model_id = encode_path_segment(&config.model_id);
            let body = json!({
                "contents": [{
                    "role": "user",
                    "parts": [{ "text": prompt }],
                }],
                "tools": [{ "google_search": {} }],
            });
            let headers = vec![
                ("Accept".into(), "application/json".into()),
                ("Content-Type".into(), "application/json".into()),
                ("x-goog-api-key".into(), config.api_key.clone()),
            ];
            (
                join_url(
                    &config.base_url,
                    &format!("/v1beta/models/{model_id}:generateContent"),
                ),
                headers,
                body,
            )
        }
        AutonomousWebManagedSearchKind::OpenAiNativeWebSearch => {
            if config.provider_id == OPENAI_CODEX_PROVIDER_ID {
                (
                    openai_codex_responses_url(&config.base_url),
                    openai_codex_json_headers(config)?,
                    openai_codex_web_search_body(config, &prompt),
                )
            } else if config.provider_id == AZURE_OPENAI_PROVIDER_ID {
                let body = openai_native_web_search_body(config, &prompt);
                (
                    join_url_with_api_version(
                        &config.base_url,
                        "/responses",
                        config.api_version.as_deref(),
                    ),
                    azure_openai_json_headers(&config.api_key),
                    body,
                )
            } else {
                let body = openai_native_web_search_body(config, &prompt);
                let headers = bearer_json_headers(&config.api_key);
                (join_url(&config.base_url, "/responses"), headers, body)
            }
        }
        AutonomousWebManagedSearchKind::XaiNativeWebSearch => {
            let body = json!({
                "model": config.model_id,
                "input": prompt,
                "tools": [{ "type": "web_search" }],
                "tool_choice": "auto",
            });
            let headers = bearer_json_headers(&config.api_key);
            (join_url(&config.base_url, "/responses"), headers, body)
        }
        AutonomousWebManagedSearchKind::OpenRouterServerWebSearch => {
            let body = json!({
                "model": config.model_id,
                "messages": [{
                    "role": "user",
                    "content": prompt,
                }],
                "tools": [{ "type": "openrouter:web_search" }],
                "tool_choice": "auto",
            });
            let headers = bearer_json_headers(&config.api_key);
            (
                join_url(&config.base_url, "/chat/completions"),
                headers,
                body,
            )
        }
    };

    let body = serde_json::to_vec(&body).map_err(|error| {
        CommandError::system_fault(
            "autonomous_web_provider_managed_request_encode_failed",
            format!("Xero could not encode the provider-managed web search request: {error}"),
        )
    })?;

    Ok(AutonomousWebTransportRequest {
        method: AutonomousWebHttpMethod::Post,
        url,
        headers,
        body: Some(body),
        timeout_ms,
        max_response_bytes,
    })
}

fn managed_search_requires_static_api_key(config: &AutonomousWebManagedSearchConfig) -> bool {
    config.provider_id != VERTEX_PROVIDER_ID
}

fn managed_search_prompt(query: &str, result_count: usize) -> String {
    format!(
        "Search the web for the following query and return up to {result_count} source URLs. Prefer official or primary sources when available. Query: {query}"
    )
}

fn openai_native_web_search_body(
    config: &AutonomousWebManagedSearchConfig,
    prompt: &str,
) -> JsonValue {
    json!({
        "model": config.model_id,
        "input": prompt,
        "store": false,
        "stream": false,
        "tools": [{ "type": "web_search" }],
        "tool_choice": "required",
    })
}

fn openai_codex_web_search_body(
    config: &AutonomousWebManagedSearchConfig,
    prompt: &str,
) -> JsonValue {
    let mut body = json!({
        "model": config.model_id,
        "store": false,
        "stream": true,
        "instructions": "Search the web for the user's query and return source URLs. Prefer official or primary sources when available.",
        "input": [{
            "role": "user",
            "content": [{ "type": "input_text", "text": prompt }],
        }],
        "text": { "verbosity": OPENAI_CODEX_TEXT_VERBOSITY },
        "include": ["reasoning.encrypted_content"],
        "tool_choice": "auto",
        "parallel_tool_calls": true,
        "tools": [{ "type": "web_search" }],
    });
    if let Some(session_id) = config
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(object) = body.as_object_mut() {
            object.insert("prompt_cache_key".into(), json!(session_id));
        }
    }
    body
}

fn bearer_json_headers(api_key: &str) -> Vec<(String, String)> {
    vec![
        ("Accept".into(), "application/json".into()),
        ("Content-Type".into(), "application/json".into()),
        ("Authorization".into(), format!("Bearer {}", api_key.trim())),
    ]
}

fn azure_openai_json_headers(api_key: &str) -> Vec<(String, String)> {
    vec![
        ("Accept".into(), "application/json".into()),
        ("Content-Type".into(), "application/json".into()),
        ("api-key".into(), api_key.trim().into()),
    ]
}

fn openai_codex_json_headers(
    config: &AutonomousWebManagedSearchConfig,
) -> CommandResult<Vec<(String, String)>> {
    let account_id = config
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_web_provider_managed_credentials_missing",
                "Xero cannot execute provider-managed web search because the OpenAI Codex session has no account id.",
            )
        })?;
    let mut headers = vec![
        ("Accept".into(), "text/event-stream".into()),
        ("Content-Type".into(), "application/json".into()),
        (
            "Authorization".into(),
            format!("Bearer {}", config.api_key.trim()),
        ),
    ];
    headers.push(("chatgpt-account-id".into(), account_id.into()));
    headers.push(("OpenAI-Beta".into(), OPENAI_CODEX_BETA_HEADER.into()));
    headers.push(("originator".into(), OPENAI_CODEX_ORIGINATOR.into()));
    headers.push(("User-Agent".into(), openai_codex_user_agent()));
    if let Some(session_id) = config
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        headers.push(("session_id".into(), session_id.into()));
    }
    Ok(headers)
}

fn openai_codex_sse_payload(body: &str) -> CommandResult<JsonValue> {
    let mut events = Vec::new();
    let mut extracted_results = Vec::new();
    let mut data_lines = Vec::new();

    for line in body.lines() {
        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start());
            continue;
        }
        if line.trim().is_empty() {
            flush_openai_codex_sse_event(&mut data_lines, &mut events, &mut extracted_results)?;
        }
    }
    flush_openai_codex_sse_event(&mut data_lines, &mut events, &mut extracted_results)?;

    Ok(json!({
        "results": extracted_results,
        "events": events,
    }))
}

fn flush_openai_codex_sse_event(
    data_lines: &mut Vec<&str>,
    events: &mut Vec<JsonValue>,
    extracted_results: &mut Vec<JsonValue>,
) -> CommandResult<()> {
    if data_lines.is_empty() {
        return Ok(());
    }
    let data = data_lines.join("\n");
    data_lines.clear();
    if data.trim().is_empty() || data.trim() == "[DONE]" {
        return Ok(());
    }
    let event: JsonValue = serde_json::from_str(&data).map_err(|error| {
        CommandError::user_fixable(
            "autonomous_web_provider_managed_decode_failed",
            format!("Xero could not decode the OpenAI Codex web-search stream: {error}"),
        )
    })?;
    if openai_codex_event_may_contain_final_text(&event) {
        extract_url_results_from_json_strings(&event, extracted_results);
    }
    events.push(event);
    Ok(())
}

fn openai_codex_event_may_contain_final_text(event: &JsonValue) -> bool {
    let Some(event_type) = event.get("type").and_then(JsonValue::as_str) else {
        return false;
    };
    event_type.ends_with(".done") || event_type.ends_with(".completed")
}

fn extract_url_results_from_json_strings(value: &JsonValue, output: &mut Vec<JsonValue>) {
    match value {
        JsonValue::String(text) => extract_url_results_from_text(text, output),
        JsonValue::Array(items) => {
            for item in items {
                extract_url_results_from_json_strings(item, output);
            }
        }
        JsonValue::Object(object) => {
            for value in object.values() {
                extract_url_results_from_json_strings(value, output);
            }
        }
        _ => {}
    }
}

fn extract_url_results_from_text(text: &str, output: &mut Vec<JsonValue>) {
    for candidate in text.split(|character: char| character.is_whitespace()) {
        let url = candidate.trim_matches(|character: char| {
            matches!(
                character,
                '"' | '\'' | '`' | '<' | '>' | '[' | ']' | '(' | ')' | ',' | ';'
            )
        });
        let url = url.trim_end_matches(|character: char| {
            matches!(character, '.' | ':' | '!' | '?' | ')' | ']')
        });
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            continue;
        }
        output.push(json!({
            "url": url,
            "title": url,
            "snippet": text,
        }));
    }
}

fn vertex_access_token() -> CommandResult<String> {
    if let Ok(token) = std::env::var("GOOGLE_OAUTH_ACCESS_TOKEN") {
        let token = token.trim().to_owned();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    let output = Command::new("gcloud")
        .arg("auth")
        .arg("application-default")
        .arg("print-access-token")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError::user_fixable(
                "vertex_gcloud_missing",
                "Xero needs GOOGLE_OAUTH_ACCESS_TOKEN or the gcloud CLI to execute Vertex AI provider-managed web search.",
            ),
            _ => CommandError::retryable(
                "vertex_gcloud_failed",
                format!("Xero could not start gcloud to obtain a Vertex AI access token: {error}"),
            ),
        })?;
    if !output.status.success() {
        return Err(CommandError::user_fixable(
            "vertex_adc_missing",
            "Xero could not obtain a Vertex AI access token from Application Default Credentials.",
        ));
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if token.is_empty() {
        return Err(CommandError::user_fixable(
            "vertex_adc_missing",
            "Xero received an empty Vertex AI access token from gcloud.",
        ));
    }
    Ok(token)
}

fn openai_codex_responses_url(base_url: &str) -> String {
    let normalized = base_url.trim().trim_end_matches('/');
    if normalized.ends_with("/codex/responses") {
        normalized.to_owned()
    } else if normalized.ends_with("/codex") {
        format!("{normalized}/responses")
    } else {
        format!("{normalized}/codex/responses")
    }
}

fn openai_codex_user_agent() -> String {
    format!("pi ({}; {})", std::env::consts::OS, std::env::consts::ARCH)
}

fn join_url_with_api_version(base_url: &str, path: &str, api_version: Option<&str>) -> String {
    let mut url = join_url(base_url, path);
    if let Some(api_version) = api_version.map(str::trim).filter(|value| !value.is_empty()) {
        let separator = if url.contains('?') { '&' } else { '?' };
        url.push(separator);
        url.push_str("api-version=");
        url.push_str(
            &url::form_urlencoded::byte_serialize(api_version.as_bytes()).collect::<String>(),
        );
    }
    url
}

fn join_url(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim().trim_end_matches('/'),
        path.trim().trim_start_matches('/')
    )
}

fn encode_path_segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
