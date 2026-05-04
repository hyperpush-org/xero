use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use reqwest::blocking::{Client, Response};
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER, USER_AGENT,
};
use serde::Deserialize;
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use tempfile::NamedTempFile;
use url::Url;

use super::{
    AgentToolCall, AgentToolDescriptor, FakeProviderAdapter, MessageAttachment,
    MessageAttachmentKind, ProviderAdapter, ProviderMessage, ProviderStreamEvent,
    ProviderTurnOutcome, ProviderTurnRequest, ProviderUsage,
};
use crate::{
    commands::{CommandError, CommandResult, ProviderModelThinkingEffortDto},
    runtime::{
        process_tree::{
            cleanup_process_group_after_root_exit, configure_process_tree_root,
            terminate_process_tree,
        },
        redaction::find_prohibited_persistence_content,
        ANTHROPIC_PROVIDER_ID, AZURE_OPENAI_PROVIDER_ID, BEDROCK_PROVIDER_ID,
        GEMINI_AI_STUDIO_PROVIDER_ID, GITHUB_MODELS_PROVIDER_ID, OLLAMA_PROVIDER_ID,
        OPENAI_API_PROVIDER_ID, OPENAI_CODEX_DEFAULT_MODEL_ID, OPENAI_CODEX_PROVIDER_ID,
        OPENROUTER_PROVIDER_ID, VERTEX_PROVIDER_ID,
    },
};

const DEFAULT_PROVIDER_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 4096;
const MAX_PROVIDER_ATTEMPTS: usize = 3;
const ANTHROPIC_API_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const BEDROCK_ANTHROPIC_VERSION: &str = "bedrock-2023-05-31";
const VERTEX_ANTHROPIC_VERSION: &str = "vertex-2023-10-16";
const GITHUB_MODELS_API_VERSION: &str = "2026-03-10";
const OPENAI_CODEX_API_BASE_URL: &str = "https://chatgpt.com/backend-api";
const OPENAI_CODEX_BETA_HEADER: &str = "responses=experimental";
const OPENAI_CODEX_ORIGINATOR: &str = "pi";
const OPENAI_CODEX_TEXT_VERBOSITY: &str = "medium";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProviderConfig {
    Fake,
    OpenAiResponses(OpenAiResponsesProviderConfig),
    OpenAiCodexResponses(OpenAiCodexResponsesProviderConfig),
    OpenAiCompatible(OpenAiCompatibleProviderConfig),
    Anthropic(AnthropicProviderConfig),
    Bedrock(BedrockProviderConfig),
    Vertex(VertexProviderConfig),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiResponsesProviderConfig {
    pub provider_id: String,
    pub model_id: String,
    pub base_url: String,
    pub api_key: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCodexResponsesProviderConfig {
    pub provider_id: String,
    pub model_id: String,
    pub base_url: String,
    pub access_token: String,
    pub account_id: String,
    pub session_id: Option<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleProviderConfig {
    pub provider_id: String,
    pub model_id: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub api_version: Option<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicProviderConfig {
    pub provider_id: String,
    pub model_id: String,
    pub api_key: String,
    pub base_url: String,
    pub anthropic_version: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BedrockProviderConfig {
    pub model_id: String,
    pub region: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexProviderConfig {
    pub model_id: String,
    pub region: String,
    pub project_id: String,
    pub timeout_ms: u64,
}

pub fn create_provider_adapter(
    config: AgentProviderConfig,
) -> CommandResult<Box<dyn ProviderAdapter>> {
    match config {
        AgentProviderConfig::Fake => Ok(Box::new(FakeProviderAdapter)),
        AgentProviderConfig::OpenAiResponses(config) => {
            OpenAiResponsesAdapter::new(config).map(|adapter| Box::new(adapter) as _)
        }
        AgentProviderConfig::OpenAiCodexResponses(config) => {
            OpenAiCodexResponsesAdapter::new(config).map(|adapter| Box::new(adapter) as _)
        }
        AgentProviderConfig::OpenAiCompatible(config) => {
            OpenAiCompatibleAdapter::new(config).map(|adapter| Box::new(adapter) as _)
        }
        AgentProviderConfig::Anthropic(config) => {
            AnthropicAdapter::new(config).map(|adapter| Box::new(adapter) as _)
        }
        AgentProviderConfig::Bedrock(config) => {
            BedrockCliAdapter::new(config).map(|adapter| Box::new(adapter) as _)
        }
        AgentProviderConfig::Vertex(config) => {
            VertexAnthropicAdapter::new(config).map(|adapter| Box::new(adapter) as _)
        }
    }
}

#[derive(Debug)]
struct OpenAiCodexResponsesAdapter {
    config: OpenAiCodexResponsesProviderConfig,
    client: Client,
}

impl OpenAiCodexResponsesAdapter {
    fn new(mut config: OpenAiCodexResponsesProviderConfig) -> CommandResult<Self> {
        normalize_required(&mut config.provider_id, "providerId")?;
        normalize_required(&mut config.model_id, "modelId")?;
        normalize_required(&mut config.base_url, "baseUrl")?;
        normalize_required(&mut config.access_token, "accessToken")?;
        normalize_required(&mut config.account_id, "accountId")?;
        config.session_id = config
            .session_id
            .map(|session_id| session_id.trim().to_owned())
            .filter(|session_id| !session_id.is_empty());
        let client = provider_http_client(config.timeout_ms)?;
        Ok(Self { config, client })
    }
}

impl ProviderAdapter for OpenAiCodexResponsesAdapter {
    fn provider_id(&self) -> &str {
        self.config.provider_id.as_str()
    }

    fn model_id(&self) -> &str {
        self.config.model_id.as_str()
    }

    fn stream_turn(
        &self,
        request: &ProviderTurnRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        let url = openai_codex_responses_url(&self.config.base_url)?;
        let headers = openai_codex_request_headers(&self.config)?;
        let body = openai_codex_responses_request_body(
            self.provider_id(),
            &self.config.model_id,
            request,
            self.config.session_id.as_deref(),
        )?;
        let response = send_provider_json_request(self.provider_id(), || {
            self.client
                .post(url.clone())
                .headers(headers.clone())
                .json(&body)
        })?;
        parse_openai_responses_sse(self.provider_id(), response, emit)
    }
}

#[derive(Debug)]
struct OpenAiResponsesAdapter {
    config: OpenAiResponsesProviderConfig,
    client: Client,
}

impl OpenAiResponsesAdapter {
    fn new(mut config: OpenAiResponsesProviderConfig) -> CommandResult<Self> {
        normalize_required(&mut config.provider_id, "providerId")?;
        normalize_required(&mut config.model_id, "modelId")?;
        normalize_required(&mut config.base_url, "baseUrl")?;
        normalize_required(&mut config.api_key, "apiKey")?;
        let client = provider_http_client(config.timeout_ms)?;
        Ok(Self { config, client })
    }
}

impl ProviderAdapter for OpenAiResponsesAdapter {
    fn provider_id(&self) -> &str {
        self.config.provider_id.as_str()
    }

    fn model_id(&self) -> &str {
        self.config.model_id.as_str()
    }

    fn stream_turn(
        &self,
        request: &ProviderTurnRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        let url = responses_url(&self.config.base_url)?;
        let body =
            openai_responses_request_body(self.provider_id(), &self.config.model_id, request)?;
        let response = send_provider_json_request(self.provider_id(), || {
            self.client
                .post(url.clone())
                .bearer_auth(&self.config.api_key)
                .json(&body)
        })?;
        parse_openai_responses_sse(self.provider_id(), response, emit)
    }
}

#[derive(Debug)]
struct OpenAiCompatibleAdapter {
    config: OpenAiCompatibleProviderConfig,
    client: Client,
}

impl OpenAiCompatibleAdapter {
    fn new(mut config: OpenAiCompatibleProviderConfig) -> CommandResult<Self> {
        normalize_required(&mut config.provider_id, "providerId")?;
        normalize_required(&mut config.model_id, "modelId")?;
        normalize_required(&mut config.base_url, "baseUrl")?;
        if let Some(api_key) = config.api_key.as_mut() {
            normalize_required(api_key, "apiKey")?;
        }
        let client = provider_http_client(config.timeout_ms)?;
        Ok(Self { config, client })
    }
}

impl ProviderAdapter for OpenAiCompatibleAdapter {
    fn provider_id(&self) -> &str {
        self.config.provider_id.as_str()
    }

    fn model_id(&self) -> &str {
        self.config.model_id.as_str()
    }

    fn stream_turn(
        &self,
        request: &ProviderTurnRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        let url =
            openai_compatible_chat_url(&self.config.base_url, self.config.api_version.as_deref())?;
        let body = openai_chat_request_body(self.provider_id(), &self.config.model_id, request)?;
        let response = send_provider_json_request(self.provider_id(), || {
            let mut http_request = self
                .client
                .post(url.clone())
                .header("Content-Type", "application/json")
                .header("X-Title", "Xero");
            http_request =
                apply_openai_compatible_provider_headers(self.provider_id(), http_request);
            if let Some(api_key) = self.config.api_key.as_deref() {
                http_request =
                    apply_openai_compatible_auth_header(self.provider_id(), http_request, api_key);
            }
            http_request.json(&body)
        })?;
        parse_openai_chat_sse(self.provider_id(), response, emit)
    }
}

#[derive(Debug)]
struct AnthropicAdapter {
    config: AnthropicProviderConfig,
    client: Client,
}

impl AnthropicAdapter {
    fn new(mut config: AnthropicProviderConfig) -> CommandResult<Self> {
        normalize_required(&mut config.provider_id, "providerId")?;
        normalize_required(&mut config.model_id, "modelId")?;
        normalize_required(&mut config.api_key, "apiKey")?;
        normalize_required(&mut config.base_url, "baseUrl")?;
        normalize_required(&mut config.anthropic_version, "anthropicVersion")?;
        let client = provider_http_client(config.timeout_ms)?;
        Ok(Self { config, client })
    }
}

impl ProviderAdapter for AnthropicAdapter {
    fn provider_id(&self) -> &str {
        self.config.provider_id.as_str()
    }

    fn model_id(&self) -> &str {
        self.config.model_id.as_str()
    }

    fn stream_turn(
        &self,
        request: &ProviderTurnRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        let url = anthropic_messages_url(&self.config.base_url)?;
        let body = anthropic_request_body(
            Some(&self.config.model_id),
            &self.config.anthropic_version,
            request,
            true,
        )?;
        let response = send_provider_json_request(self.provider_id(), || {
            self.client
                .post(url.clone())
                .header("x-api-key", &self.config.api_key)
                .header("anthropic-version", &self.config.anthropic_version)
                .json(&body)
        })?;
        parse_anthropic_sse(self.provider_id(), response, emit)
    }
}

#[derive(Debug)]
struct BedrockCliAdapter {
    config: BedrockProviderConfig,
}

impl BedrockCliAdapter {
    fn new(mut config: BedrockProviderConfig) -> CommandResult<Self> {
        normalize_required(&mut config.model_id, "modelId")?;
        normalize_required(&mut config.region, "region")?;
        Ok(Self { config })
    }
}

impl ProviderAdapter for BedrockCliAdapter {
    fn provider_id(&self) -> &str {
        BEDROCK_PROVIDER_ID
    }

    fn model_id(&self) -> &str {
        self.config.model_id.as_str()
    }

    fn stream_turn(
        &self,
        request: &ProviderTurnRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        let body = anthropic_request_body(None, BEDROCK_ANTHROPIC_VERSION, request, false)?;
        let mut body_file = NamedTempFile::new().map_err(|error| {
            CommandError::retryable(
                "bedrock_tempfile_failed",
                format!("Xero could not allocate a Bedrock request file: {error}"),
            )
        })?;
        serde_json::to_writer(body_file.as_file_mut(), &body).map_err(|error| {
            CommandError::retryable(
                "bedrock_request_write_failed",
                format!("Xero could not write the Bedrock request body: {error}"),
            )
        })?;
        body_file.as_file_mut().flush().map_err(|error| {
            CommandError::retryable(
                "bedrock_request_write_failed",
                format!("Xero could not flush the Bedrock request body: {error}"),
            )
        })?;
        let body_arg = format!("fileb://{}", body_file.path().display());
        let output = NamedTempFile::new().map_err(|error| {
            CommandError::retryable(
                "bedrock_tempfile_failed",
                format!("Xero could not allocate a Bedrock response file: {error}"),
            )
        })?;
        let output_path = output.path().to_path_buf();
        let mut command = Command::new("aws");
        command
            .arg("bedrock-runtime")
            .arg("invoke-model")
            .arg("--region")
            .arg(&self.config.region)
            .arg("--model-id")
            .arg(&self.config.model_id)
            .arg("--content-type")
            .arg("application/json")
            .arg("--accept")
            .arg("application/json")
            .arg("--cli-binary-format")
            .arg("raw-in-base64-out")
            .arg("--body")
            .arg(body_arg)
            .arg(&output_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_process_tree_root(&mut command);
        let mut child = command.spawn().map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError::user_fixable(
                "bedrock_aws_cli_missing",
                "Xero needs the AWS CLI to invoke Amazon Bedrock from the owned provider adapter.",
            ),
            _ => CommandError::retryable(
                "bedrock_invoke_spawn_failed",
                format!("Xero could not start the AWS CLI Bedrock invocation: {error}"),
            ),
        })?;
        let status = wait_provider_cli(
            &mut child,
            Duration::from_millis(normalize_timeout(self.config.timeout_ms)),
            BEDROCK_PROVIDER_ID,
        )?;
        if !status.success() {
            let stderr = read_child_stderr(child)?;
            return Err(provider_status_error(
                BEDROCK_PROVIDER_ID,
                status.code().unwrap_or(-1),
                &stderr,
            ));
        }

        let response_text = std::fs::read_to_string(&output_path).map_err(|error| {
            CommandError::retryable(
                "bedrock_response_read_failed",
                format!("Xero could not read the Bedrock response body: {error}"),
            )
        })?;
        parse_anthropic_json_response(BEDROCK_PROVIDER_ID, &response_text, emit)
    }
}

#[derive(Debug)]
struct VertexAnthropicAdapter {
    config: VertexProviderConfig,
    client: Client,
}

impl VertexAnthropicAdapter {
    fn new(mut config: VertexProviderConfig) -> CommandResult<Self> {
        normalize_required(&mut config.model_id, "modelId")?;
        normalize_required(&mut config.region, "region")?;
        normalize_required(&mut config.project_id, "projectId")?;
        let client = provider_http_client(config.timeout_ms)?;
        Ok(Self { config, client })
    }
}

impl ProviderAdapter for VertexAnthropicAdapter {
    fn provider_id(&self) -> &str {
        VERTEX_PROVIDER_ID
    }

    fn model_id(&self) -> &str {
        self.config.model_id.as_str()
    }

    fn stream_turn(
        &self,
        request: &ProviderTurnRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        let token = vertex_access_token()?;
        let body = anthropic_request_body(None, VERTEX_ANTHROPIC_VERSION, request, false)?;
        let url = vertex_anthropic_raw_predict_url(&self.config)?;
        let response = send_provider_json_request(VERTEX_PROVIDER_ID, || {
            self.client
                .post(url.clone())
                .bearer_auth(token.clone())
                .json(&body)
        })?;
        let text = response.text().map_err(|error| {
            CommandError::retryable(
                "vertex_response_read_failed",
                format!("Xero could not read the Vertex AI response body: {error}"),
            )
        })?;
        parse_anthropic_json_response(VERTEX_PROVIDER_ID, &text, emit)
    }
}

fn provider_http_client(timeout_ms: u64) -> CommandResult<Client> {
    Client::builder()
        .timeout(Duration::from_millis(normalize_timeout(timeout_ms)))
        .build()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_provider_http_client_unavailable",
                format!("Xero could not build an HTTP client for the provider adapter: {error}"),
            )
        })
}

fn normalize_timeout(timeout_ms: u64) -> u64 {
    if timeout_ms == 0 {
        DEFAULT_PROVIDER_TIMEOUT_MS
    } else {
        timeout_ms
    }
}

fn normalize_required(value: &mut String, field: &'static str) -> CommandResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    if trimmed.len() != value.len() {
        *value = trimmed.to_owned();
    }
    Ok(())
}

fn responses_url(base_url: &str) -> CommandResult<Url> {
    provider_url(base_url, "responses", None)
}

fn openai_codex_responses_url(base_url: &str) -> CommandResult<Url> {
    let normalized = base_url.trim().trim_end_matches('/');
    let raw = if normalized.ends_with("/codex/responses") {
        normalized.to_owned()
    } else if normalized.ends_with("/codex") {
        format!("{normalized}/responses")
    } else {
        format!("{normalized}/codex/responses")
    };
    let url = Url::parse(&raw).map_err(|error| {
        CommandError::user_fixable(
            "agent_provider_url_invalid",
            format!("Xero rejected provider base URL `{base_url}`: {error}"),
        )
    })?;
    validate_provider_url_scheme(base_url, &url)?;
    Ok(url)
}

fn anthropic_messages_url(base_url: &str) -> CommandResult<Url> {
    provider_url(base_url, "v1/messages", None)
}

fn openai_compatible_chat_url(base_url: &str, api_version: Option<&str>) -> CommandResult<Url> {
    provider_url(base_url, "chat/completions", api_version)
}

fn provider_url(base_url: &str, path: &str, api_version: Option<&str>) -> CommandResult<Url> {
    let mut url = Url::parse(&format!(
        "{}/{}",
        base_url.trim().trim_end_matches('/'),
        path.trim_start_matches('/')
    ))
    .map_err(|error| {
        CommandError::user_fixable(
            "agent_provider_url_invalid",
            format!("Xero rejected provider base URL `{base_url}`: {error}"),
        )
    })?;
    validate_provider_url_scheme(base_url, &url)?;
    if let Some(api_version) = api_version.map(str::trim).filter(|value| !value.is_empty()) {
        url.query_pairs_mut()
            .append_pair("api-version", api_version);
    }
    Ok(url)
}

fn validate_provider_url_scheme(base_url: &str, url: &Url) -> CommandResult<()> {
    match url.scheme() {
        "https" => Ok(()),
        "http" if url.host_str().is_some_and(is_local_provider_host) => Ok(()),
        "http" => Err(CommandError::user_fixable(
            "agent_provider_url_insecure",
            format!(
                "Xero refused provider base URL `{base_url}` because hosted provider traffic must use HTTPS. HTTP is only allowed for local providers."
            ),
        )),
        scheme => Err(CommandError::user_fixable(
            "agent_provider_url_invalid",
            format!(
                "Xero rejected provider base URL `{base_url}` because scheme `{scheme}` is not supported."
            ),
        )),
    }
}

fn is_local_provider_host(host: &str) -> bool {
    let normalized = host.trim_matches(['[', ']']).to_ascii_lowercase();
    normalized == "localhost"
        || normalized == "::1"
        || normalized == "0.0.0.0"
        || normalized.starts_with("127.")
}

fn vertex_anthropic_raw_predict_url(config: &VertexProviderConfig) -> CommandResult<Url> {
    let raw = format!(
        "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/anthropic/models/{}:rawPredict",
        config.region.trim(),
        config.project_id.trim(),
        config.region.trim(),
        config.model_id.trim(),
    );
    Url::parse(&raw).map_err(|error| {
        CommandError::user_fixable(
            "vertex_endpoint_invalid",
            format!("Xero could not build the Vertex AI Anthropic endpoint: {error}"),
        )
    })
}

fn openai_chat_request_body(
    provider_id: &str,
    model_id: &str,
    request: &ProviderTurnRequest,
) -> CommandResult<JsonValue> {
    let mut body = JsonMap::new();
    body.insert("model".into(), json!(model_id));
    body.insert(
        "messages".into(),
        JsonValue::Array(openai_chat_messages(request)?),
    );
    body.insert(
        "tools".into(),
        JsonValue::Array(request.tools.iter().map(openai_chat_tool).collect()),
    );
    body.insert("tool_choice".into(), json!("auto"));
    body.insert("stream".into(), json!(true));
    if provider_supports_openai_stream_options(provider_id) {
        body.insert("stream_options".into(), json!({ "include_usage": true }));
    }
    Ok(JsonValue::Object(body))
}

fn provider_supports_openai_stream_options(provider_id: &str) -> bool {
    matches!(
        provider_id,
        OPENAI_API_PROVIDER_ID
            | OPENAI_CODEX_PROVIDER_ID
            | OPENROUTER_PROVIDER_ID
            | GITHUB_MODELS_PROVIDER_ID
            | AZURE_OPENAI_PROVIDER_ID
    )
}

fn openai_chat_messages(request: &ProviderTurnRequest) -> CommandResult<Vec<JsonValue>> {
    let mut messages = vec![json!({
        "role": "system",
        "content": request.system_prompt,
    })];
    for message in &request.messages {
        match message {
            ProviderMessage::User { content, .. } => {
                messages.push(json!({ "role": "user", "content": content }));
            }
            ProviderMessage::Assistant {
                content,
                tool_calls,
            } => {
                let mut object = JsonMap::new();
                object.insert("role".into(), json!("assistant"));
                object.insert("content".into(), json!(content));
                if !tool_calls.is_empty() {
                    object.insert(
                        "tool_calls".into(),
                        JsonValue::Array(tool_calls.iter().map(openai_chat_tool_call).collect()),
                    );
                }
                messages.push(JsonValue::Object(object));
            }
            ProviderMessage::Tool {
                tool_call_id,
                content,
                ..
            } => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": content,
                }));
            }
        }
    }
    Ok(messages)
}

fn openai_chat_tool(tool: &AgentToolDescriptor) -> JsonValue {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
}

fn openai_chat_tool_call(tool_call: &AgentToolCall) -> JsonValue {
    json!({
        "id": tool_call.tool_call_id,
        "type": "function",
        "function": {
            "name": tool_call.tool_name,
            "arguments": tool_call.input.to_string(),
        }
    })
}

fn openai_responses_request_body(
    provider_id: &str,
    model_id: &str,
    request: &ProviderTurnRequest,
) -> CommandResult<JsonValue> {
    let mut body = JsonMap::new();
    body.insert("model".into(), json!(model_id));
    body.insert("instructions".into(), json!(request.system_prompt));
    body.insert(
        "input".into(),
        JsonValue::Array(openai_response_input(request)?),
    );
    body.insert(
        "tools".into(),
        JsonValue::Array(request.tools.iter().map(openai_response_tool).collect()),
    );
    body.insert("stream".into(), json!(true));
    body.insert("max_output_tokens".into(), json!(DEFAULT_MAX_OUTPUT_TOKENS));
    if let Some(effort) = request.controls.active.thinking_effort.as_ref() {
        body.insert(
            "reasoning".into(),
            json!({
                "effort": openai_responses_thinking_effort_value(provider_id, model_id, effort),
                "summary": "auto",
            }),
        );
    }
    Ok(JsonValue::Object(body))
}

fn openai_codex_responses_request_body(
    provider_id: &str,
    model_id: &str,
    request: &ProviderTurnRequest,
    prompt_cache_key: Option<&str>,
) -> CommandResult<JsonValue> {
    let mut body = JsonMap::new();
    body.insert("model".into(), json!(model_id));
    body.insert("store".into(), json!(false));
    body.insert("stream".into(), json!(true));
    body.insert("instructions".into(), json!(request.system_prompt));
    body.insert(
        "input".into(),
        JsonValue::Array(openai_codex_response_input(request)?),
    );
    body.insert(
        "text".into(),
        json!({ "verbosity": OPENAI_CODEX_TEXT_VERBOSITY }),
    );
    body.insert("include".into(), json!(["reasoning.encrypted_content"]));
    if let Some(prompt_cache_key) = prompt_cache_key
        .map(str::trim)
        .filter(|prompt_cache_key| !prompt_cache_key.is_empty())
    {
        body.insert("prompt_cache_key".into(), json!(prompt_cache_key));
    }
    body.insert("tool_choice".into(), json!("auto"));
    body.insert("parallel_tool_calls".into(), json!(true));
    if !request.tools.is_empty() {
        body.insert(
            "tools".into(),
            JsonValue::Array(
                request
                    .tools
                    .iter()
                    .map(openai_codex_response_tool)
                    .collect(),
            ),
        );
    }
    if let Some(effort) = request.controls.active.thinking_effort.as_ref() {
        body.insert(
            "reasoning".into(),
            json!({
                "effort": openai_responses_thinking_effort_value(provider_id, model_id, effort),
                "summary": "auto",
            }),
        );
    }
    Ok(JsonValue::Object(body))
}

fn openai_response_input(request: &ProviderTurnRequest) -> CommandResult<Vec<JsonValue>> {
    let mut input = Vec::new();
    for message in &request.messages {
        match message {
            ProviderMessage::User { content, .. } => {
                input.push(json!({ "role": "user", "content": content }));
            }
            ProviderMessage::Assistant {
                content,
                tool_calls,
            } => {
                if !content.trim().is_empty() {
                    input.push(json!({ "role": "assistant", "content": content }));
                }
                for tool_call in tool_calls {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": tool_call.tool_call_id,
                        "name": tool_call.tool_name,
                        "arguments": tool_call.input.to_string(),
                    }));
                }
            }
            ProviderMessage::Tool {
                tool_call_id,
                content,
                ..
            } => {
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": tool_call_id,
                    "output": content,
                }));
            }
        }
    }
    Ok(input)
}

fn openai_codex_response_input(request: &ProviderTurnRequest) -> CommandResult<Vec<JsonValue>> {
    let mut input = Vec::new();
    for (index, message) in request.messages.iter().enumerate() {
        match message {
            ProviderMessage::User { content, .. } => {
                input.push(json!({
                    "role": "user",
                    "content": [{ "type": "input_text", "text": content }],
                }));
            }
            ProviderMessage::Assistant {
                content,
                tool_calls,
            } => {
                if !content.trim().is_empty() {
                    input.push(json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": content,
                            "annotations": [],
                        }],
                        "status": "completed",
                        "id": format!("msg_{index}"),
                    }));
                }
                for tool_call in tool_calls {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": tool_call.tool_call_id,
                        "name": tool_call.tool_name,
                        "arguments": tool_call.input.to_string(),
                    }));
                }
            }
            ProviderMessage::Tool {
                tool_call_id,
                content,
                ..
            } => {
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": tool_call_id,
                    "output": content,
                }));
            }
        }
    }
    Ok(input)
}

fn openai_response_tool(tool: &AgentToolDescriptor) -> JsonValue {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.input_schema,
    })
}

fn openai_codex_response_tool(tool: &AgentToolDescriptor) -> JsonValue {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.input_schema,
        "strict": JsonValue::Null,
    })
}

fn openai_codex_request_headers(
    config: &OpenAiCodexResponsesProviderConfig,
) -> CommandResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        provider_header_value(
            "authorization",
            &format!("Bearer {}", config.access_token.trim()),
        )?,
    );
    headers.insert(
        "chatgpt-account-id",
        provider_header_value("chatgptAccountId", config.account_id.trim())?,
    );
    headers.insert(
        "OpenAI-Beta",
        provider_header_value("openaiBeta", OPENAI_CODEX_BETA_HEADER)?,
    );
    headers.insert(
        "originator",
        provider_header_value("originator", OPENAI_CODEX_ORIGINATOR)?,
    );
    headers.insert(
        USER_AGENT,
        provider_header_value("userAgent", &openai_codex_user_agent())?,
    );
    headers.insert(
        ACCEPT,
        provider_header_value("accept", "text/event-stream")?,
    );
    headers.insert(
        CONTENT_TYPE,
        provider_header_value("contentType", "application/json")?,
    );
    if let Some(session_id) = config
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty())
    {
        headers.insert(
            "session_id",
            provider_header_value("sessionId", session_id)?,
        );
    }
    Ok(headers)
}

fn provider_header_value(field: &'static str, value: &str) -> CommandResult<HeaderValue> {
    HeaderValue::from_str(value).map_err(|_| CommandError::invalid_request(field))
}

fn openai_codex_user_agent() -> String {
    format!("pi ({}; {})", std::env::consts::OS, std::env::consts::ARCH)
}

fn apply_openai_compatible_auth_header(
    provider_id: &str,
    request: reqwest::blocking::RequestBuilder,
    api_key: &str,
) -> reqwest::blocking::RequestBuilder {
    match provider_id {
        AZURE_OPENAI_PROVIDER_ID => request.header("api-key", api_key),
        _ => request.bearer_auth(api_key),
    }
}

fn apply_openai_compatible_provider_headers(
    provider_id: &str,
    request: reqwest::blocking::RequestBuilder,
) -> reqwest::blocking::RequestBuilder {
    match provider_id {
        GITHUB_MODELS_PROVIDER_ID => request
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", GITHUB_MODELS_API_VERSION),
        _ => request,
    }
}

#[cfg(test)]
fn openai_compatible_auth_header_name(provider_id: &str) -> &'static str {
    match provider_id {
        AZURE_OPENAI_PROVIDER_ID => "api-key",
        _ => "Authorization",
    }
}

fn thinking_effort_value(effort: &ProviderModelThinkingEffortDto) -> &'static str {
    match effort {
        ProviderModelThinkingEffortDto::Minimal => "minimal",
        ProviderModelThinkingEffortDto::Low => "low",
        ProviderModelThinkingEffortDto::Medium => "medium",
        ProviderModelThinkingEffortDto::High => "high",
        ProviderModelThinkingEffortDto::XHigh => "xhigh",
    }
}

fn openai_responses_thinking_effort_value(
    provider_id: &str,
    model_id: &str,
    effort: &ProviderModelThinkingEffortDto,
) -> &'static str {
    let effort = thinking_effort_value(effort);
    if provider_id == OPENAI_CODEX_PROVIDER_ID {
        return clamp_openai_codex_reasoning_effort(model_id, effort);
    }
    effort
}

fn clamp_openai_codex_reasoning_effort(model_id: &str, effort: &'static str) -> &'static str {
    let model_id = model_id.rsplit('/').next().unwrap_or(model_id);
    let model_id = model_id.trim().to_ascii_lowercase();

    if ["gpt-5.2", "gpt-5.3", "gpt-5.4", "gpt-5.5"]
        .iter()
        .any(|prefix| model_id.starts_with(prefix))
        && effort == "minimal"
    {
        return "low";
    }
    if model_id == "gpt-5.1" && effort == "xhigh" {
        return "high";
    }
    if model_id == "gpt-5.1-codex-mini" {
        return if effort == "high" || effort == "xhigh" {
            "high"
        } else {
            "medium"
        };
    }

    effort
}

fn anthropic_request_body(
    model_id: Option<&str>,
    anthropic_version: &str,
    request: &ProviderTurnRequest,
    stream: bool,
) -> CommandResult<JsonValue> {
    let mut body = JsonMap::new();
    if let Some(model_id) = model_id {
        body.insert("model".into(), json!(model_id));
    }
    body.insert("system".into(), json!(request.system_prompt));
    body.insert("max_tokens".into(), json!(DEFAULT_MAX_OUTPUT_TOKENS));
    body.insert("stream".into(), json!(stream));
    body.insert(
        "messages".into(),
        JsonValue::Array(anthropic_messages(request)?),
    );
    body.insert(
        "tools".into(),
        JsonValue::Array(request.tools.iter().map(anthropic_tool).collect()),
    );
    if anthropic_version.starts_with("bedrock-") || anthropic_version.starts_with("vertex-") {
        body.insert("anthropic_version".into(), json!(anthropic_version));
    }
    Ok(JsonValue::Object(body))
}

fn anthropic_messages(request: &ProviderTurnRequest) -> CommandResult<Vec<JsonValue>> {
    let mut messages = Vec::new();
    for message in &request.messages {
        match message {
            ProviderMessage::User {
                content,
                attachments,
            } => {
                let blocks = anthropic_user_content_blocks(content, attachments)?;
                if !blocks.is_empty() {
                    messages.push(json!({ "role": "user", "content": blocks }));
                }
            }
            ProviderMessage::Assistant {
                content,
                tool_calls,
            } => {
                let mut blocks = Vec::new();
                if !content.trim().is_empty() {
                    blocks.push(json!({ "type": "text", "text": content }));
                }
                blocks.extend(tool_calls.iter().map(|tool_call| {
                    json!({
                        "type": "tool_use",
                        "id": tool_call.tool_call_id,
                        "name": tool_call.tool_name,
                        "input": tool_call.input,
                    })
                }));
                if !blocks.is_empty() {
                    messages.push(json!({ "role": "assistant", "content": blocks }));
                }
            }
            ProviderMessage::Tool {
                tool_call_id,
                content,
                ..
            } => {
                messages.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": content,
                    }],
                }));
            }
        }
    }
    Ok(messages)
}

fn anthropic_user_content_blocks(
    content: &str,
    attachments: &[MessageAttachment],
) -> CommandResult<Vec<JsonValue>> {
    let mut blocks: Vec<JsonValue> = Vec::with_capacity(attachments.len() + 1);
    for attachment in attachments {
        match attachment.kind {
            MessageAttachmentKind::Image => {
                let bytes = std::fs::read(&attachment.absolute_path).map_err(|error| {
                    CommandError::system_fault(
                        "agent_attachment_read_failed",
                        format!(
                            "Xero could not read attached image `{}` from disk: {error}",
                            attachment.original_name
                        ),
                    )
                })?;
                let data = BASE64_STANDARD.encode(&bytes);
                blocks.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": attachment.media_type,
                        "data": data,
                    },
                }));
            }
            MessageAttachmentKind::Document => {
                let bytes = std::fs::read(&attachment.absolute_path).map_err(|error| {
                    CommandError::system_fault(
                        "agent_attachment_read_failed",
                        format!(
                            "Xero could not read attached document `{}` from disk: {error}",
                            attachment.original_name
                        ),
                    )
                })?;
                let data = BASE64_STANDARD.encode(&bytes);
                blocks.push(json!({
                    "type": "document",
                    "source": {
                        "type": "base64",
                        "media_type": "application/pdf",
                        "data": data,
                    },
                }));
            }
            MessageAttachmentKind::Text => {
                let text = std::fs::read_to_string(&attachment.absolute_path).map_err(|error| {
                    CommandError::system_fault(
                        "agent_attachment_read_failed",
                        format!(
                            "Xero could not read attached text file `{}` from disk: {error}",
                            attachment.original_name
                        ),
                    )
                })?;
                blocks.push(json!({
                    "type": "text",
                    "text": format!(
                        "<attached_file name=\"{}\">\n{}\n</attached_file>",
                        attachment.original_name, text
                    ),
                }));
            }
        }
    }
    if !content.is_empty() {
        blocks.push(json!({ "type": "text", "text": content }));
    } else if blocks.is_empty() {
        blocks.push(json!({ "type": "text", "text": "" }));
    }
    Ok(blocks)
}

fn anthropic_tool(tool: &AgentToolDescriptor) -> JsonValue {
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    })
}

#[derive(Debug, Default)]
struct PartialToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatChunk {
    #[serde(default)]
    choices: Vec<OpenAiChatChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatChoice {
    #[serde(default)]
    delta: OpenAiChatDelta,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiChatDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OpenAiToolCallDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAiFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
    #[serde(default)]
    prompt_tokens_details: Option<OpenAiUsagePromptDetails>,
    #[serde(default)]
    cost: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsagePromptDetails {
    #[serde(default)]
    cached_tokens: u64,
    #[serde(default)]
    cache_write_tokens: u64,
}

fn parse_openai_chat_sse(
    provider_id: &str,
    response: Response,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<ProviderTurnOutcome> {
    let mut message = String::new();
    let mut partial_calls = BTreeMap::<usize, PartialToolCall>::new();
    let mut usage = None;

    for line in BufReader::new(response).lines() {
        let line = line.map_err(|error| {
            CommandError::retryable(
                "agent_provider_stream_read_failed",
                format!("Xero lost the {provider_id} response stream: {error}"),
            )
        })?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            break;
        }
        let chunk: OpenAiChatChunk = serde_json::from_str(data).map_err(|error| {
            CommandError::retryable(
                "agent_provider_stream_decode_failed",
                format!("Xero could not decode a {provider_id} stream chunk: {error}"),
            )
        })?;
        if let Some(next_usage) = chunk.usage {
            let mapped = openai_provider_usage(
                next_usage.prompt_tokens,
                next_usage.completion_tokens,
                next_usage.total_tokens,
                openai_usage_cache_read_tokens(&next_usage),
                openai_usage_cache_creation_tokens(&next_usage),
                openai_reported_cost_micros(provider_id, &next_usage),
            );
            emit(ProviderStreamEvent::Usage(mapped.clone()))?;
            usage = Some(mapped);
        }
        for choice in chunk.choices {
            let OpenAiChatDelta {
                content,
                reasoning,
                reasoning_content,
                tool_calls,
            } = choice.delta;
            if let Some(reasoning) = reasoning_content
                .or(reasoning)
                .filter(|reasoning| !reasoning.is_empty())
            {
                emit(ProviderStreamEvent::ReasoningSummary(reasoning))?;
            }
            if let Some(content) = content {
                message.push_str(&content);
                emit(ProviderStreamEvent::MessageDelta(content))?;
            }
            for tool_call in tool_calls {
                let partial = partial_calls.entry(tool_call.index).or_default();
                if let Some(id) = tool_call.id {
                    partial.id = Some(id);
                }
                if let Some(function) = tool_call.function {
                    if let Some(name) = function.name {
                        partial.name = Some(name);
                    }
                    if let Some(arguments) = function.arguments {
                        partial.arguments.push_str(&arguments);
                        emit(ProviderStreamEvent::ToolDelta {
                            tool_call_id: partial.id.clone(),
                            tool_name: partial.name.clone(),
                            arguments_delta: arguments,
                        })?;
                    }
                }
            }
        }
    }

    finish_provider_turn(provider_id, message, partial_calls, usage)
}

fn parse_openai_responses_sse(
    provider_id: &str,
    response: Response,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<ProviderTurnOutcome> {
    let mut message = String::new();
    let mut partial_calls = BTreeMap::<usize, PartialToolCall>::new();
    let mut completed_call_count = 0_usize;
    let mut usage = None;

    for line in BufReader::new(response).lines() {
        let line = line.map_err(|error| {
            CommandError::retryable(
                "agent_provider_stream_read_failed",
                format!("Xero lost the {provider_id} Responses stream: {error}"),
            )
        })?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let value: JsonValue = serde_json::from_str(data).map_err(|error| {
            CommandError::retryable(
                "agent_provider_stream_decode_failed",
                format!("Xero could not decode a {provider_id} Responses chunk: {error}"),
            )
        })?;
        if emit_openai_responses_reasoning_summary_event(&value, emit)? {
            continue;
        }
        match value
            .get("type")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
        {
            "error" => return Err(openai_responses_stream_error(provider_id, &value)),
            "response.failed" => return Err(openai_responses_stream_error(provider_id, &value)),
            "response.output_text.delta" => {
                let delta = value
                    .get("delta")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_string();
                if !delta.is_empty() {
                    message.push_str(&delta);
                    emit(ProviderStreamEvent::MessageDelta(delta))?;
                }
            }
            "response.function_call_arguments.delta" => {
                let index = value
                    .get("output_index")
                    .and_then(JsonValue::as_u64)
                    .unwrap_or(completed_call_count as u64) as usize;
                let delta = value
                    .get("delta")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_string();
                let partial = partial_calls.entry(index).or_default();
                partial.arguments.push_str(&delta);
                emit(ProviderStreamEvent::ToolDelta {
                    tool_call_id: partial.id.clone(),
                    tool_name: partial.name.clone(),
                    arguments_delta: delta,
                })?;
            }
            "response.output_item.added" => {
                apply_openai_response_function_call_item(
                    &mut partial_calls,
                    &value,
                    completed_call_count,
                );
            }
            "response.output_item.done" => {
                if apply_openai_response_function_call_item(
                    &mut partial_calls,
                    &value,
                    completed_call_count,
                ) {
                    completed_call_count = completed_call_count.saturating_add(1);
                }
            }
            "response.completed" | "response.done" => {
                if let Some(mapped) = value
                    .get("response")
                    .and_then(|response| response.get("usage"))
                    .map(openai_responses_usage)
                {
                    emit(ProviderStreamEvent::Usage(mapped.clone()))?;
                    usage = Some(mapped);
                }
            }
            _ => {}
        }
    }

    finish_provider_turn(provider_id, message, partial_calls, usage)
}

fn emit_openai_responses_reasoning_summary_event(
    value: &JsonValue,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<bool> {
    match value
        .get("type")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
    {
        "response.reasoning_summary_text.delta" => {
            let delta = value
                .get("delta")
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_string();
            if !delta.is_empty() {
                emit(ProviderStreamEvent::ReasoningSummary(delta))?;
            }
            Ok(true)
        }
        "response.reasoning_summary_part.done" | "response.reasoning_summary_text.done" => {
            emit(ProviderStreamEvent::ReasoningSummary("\n\n".into()))?;
            Ok(true)
        }
        "response.reasoning_summary_part.added" => Ok(true),
        _ => Ok(false),
    }
}

fn apply_openai_response_function_call_item(
    partial_calls: &mut BTreeMap<usize, PartialToolCall>,
    event: &JsonValue,
    fallback_index: usize,
) -> bool {
    let item = event.get("item").cloned().unwrap_or(JsonValue::Null);
    if item.get("type").and_then(JsonValue::as_str) != Some("function_call") {
        return false;
    }

    let index = event
        .get("output_index")
        .or_else(|| item.get("output_index"))
        .and_then(JsonValue::as_u64)
        .unwrap_or(fallback_index as u64) as usize;
    let partial = partial_calls.entry(index).or_default();
    partial.id = item
        .get("call_id")
        .or_else(|| item.get("id"))
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| partial.id.clone());
    partial.name = item
        .get("name")
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| partial.name.clone());
    if partial.arguments.trim().is_empty() {
        partial.arguments = item
            .get("arguments")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string();
    }
    true
}

fn openai_responses_stream_error(provider_id: &str, event: &JsonValue) -> CommandError {
    let event_type = event
        .get("type")
        .and_then(JsonValue::as_str)
        .unwrap_or("error");
    let nested_error = event.get("error");
    let response_error = event
        .get("response")
        .and_then(|response| response.get("error"));
    let error_node = nested_error.or(response_error);
    let code = error_node
        .and_then(|error| error.get("code"))
        .or_else(|| event.get("code"))
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    let error_type = error_node
        .and_then(|error| error.get("type"))
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    let message = error_node
        .and_then(|error| error.get("message"))
        .or_else(|| event.get("message"))
        .and_then(JsonValue::as_str)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| {
            if event_type == "response.failed" {
                "provider response failed"
            } else {
                "provider stream returned an error"
            }
        });
    let prefix = if provider_id == OPENAI_CODEX_PROVIDER_ID {
        if error_type.is_empty() {
            "Codex error".to_string()
        } else {
            format!("Codex {error_type}")
        }
    } else {
        format!("Provider `{provider_id}` stream error")
    };
    let suffix = if code.is_empty() {
        String::new()
    } else {
        format!(" ({code})")
    };
    CommandError::retryable(
        format!("{provider_id}_stream_failed"),
        format!("{prefix}: {message}{suffix}"),
    )
}

fn openai_responses_usage(value: &JsonValue) -> ProviderUsage {
    let input_tokens = value
        .get("input_tokens")
        .and_then(JsonValue::as_u64)
        .unwrap_or_default();
    let output_tokens = value
        .get("output_tokens")
        .and_then(JsonValue::as_u64)
        .unwrap_or_default();
    let cache_read_tokens = value
        .get("input_tokens_details")
        .and_then(|details| details.get("cached_tokens"))
        .and_then(JsonValue::as_u64)
        .unwrap_or_default();
    let total_tokens = value
        .get("total_tokens")
        .and_then(JsonValue::as_u64)
        .unwrap_or_default();
    openai_provider_usage(
        input_tokens,
        output_tokens,
        total_tokens,
        cache_read_tokens,
        0,
        None,
    )
}

fn openai_provider_usage(
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    reported_cost_micros: Option<u64>,
) -> ProviderUsage {
    let billable_input_tokens = input_tokens
        .saturating_sub(cache_read_tokens)
        .saturating_sub(cache_creation_tokens);
    ProviderUsage {
        input_tokens: billable_input_tokens,
        output_tokens,
        total_tokens: if total_tokens > 0 {
            total_tokens
        } else {
            billable_input_tokens
                .saturating_add(output_tokens)
                .saturating_add(cache_read_tokens)
                .saturating_add(cache_creation_tokens)
        },
        cache_read_tokens,
        cache_creation_tokens,
        reported_cost_micros,
    }
}

fn openai_usage_cache_read_tokens(usage: &OpenAiUsage) -> u64 {
    usage
        .prompt_tokens_details
        .as_ref()
        .map(|details| details.cached_tokens)
        .unwrap_or_default()
}

fn openai_usage_cache_creation_tokens(usage: &OpenAiUsage) -> u64 {
    usage
        .prompt_tokens_details
        .as_ref()
        .map(|details| details.cache_write_tokens)
        .unwrap_or_default()
}

fn openai_reported_cost_micros(provider_id: &str, usage: &OpenAiUsage) -> Option<u64> {
    if provider_id != OPENROUTER_PROVIDER_ID {
        return None;
    }
    usage.cost.and_then(usd_cost_to_micros)
}

fn usd_cost_to_micros(cost: f64) -> Option<u64> {
    if !cost.is_finite() || cost < 0.0 {
        return None;
    }
    let micros = (cost * 1_000_000.0).round();
    if micros > u64::MAX as f64 {
        None
    } else {
        Some(micros as u64)
    }
}

fn parse_anthropic_sse(
    provider_id: &str,
    response: Response,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<ProviderTurnOutcome> {
    let mut message = String::new();
    let mut partial_calls = BTreeMap::<usize, PartialToolCall>::new();
    let mut usage = ProviderUsage::default();

    for line in BufReader::new(response).lines() {
        let line = line.map_err(|error| {
            CommandError::retryable(
                "agent_provider_stream_read_failed",
                format!("Xero lost the {provider_id} response stream: {error}"),
            )
        })?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        let value: JsonValue = serde_json::from_str(data).map_err(|error| {
            CommandError::retryable(
                "agent_provider_stream_decode_failed",
                format!("Xero could not decode a {provider_id} stream chunk: {error}"),
            )
        })?;
        match value
            .get("type")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
        {
            "message_start" => {
                let usage_node = value
                    .get("message")
                    .and_then(|message| message.get("usage"));
                if let Some(usage_node) = usage_node {
                    usage.input_tokens = usage_node
                        .get("input_tokens")
                        .and_then(JsonValue::as_u64)
                        .unwrap_or_default();
                    usage.cache_read_tokens = usage_node
                        .get("cache_read_input_tokens")
                        .and_then(JsonValue::as_u64)
                        .unwrap_or_default();
                    usage.cache_creation_tokens = usage_node
                        .get("cache_creation_input_tokens")
                        .and_then(JsonValue::as_u64)
                        .unwrap_or_default();
                }
            }
            "content_block_start" => {
                let index = value
                    .get("index")
                    .and_then(JsonValue::as_u64)
                    .unwrap_or_default() as usize;
                let block = value
                    .get("content_block")
                    .cloned()
                    .unwrap_or(JsonValue::Null);
                if block.get("type").and_then(JsonValue::as_str) == Some("tool_use") {
                    partial_calls.insert(
                        index,
                        PartialToolCall {
                            id: block
                                .get("id")
                                .and_then(JsonValue::as_str)
                                .map(ToOwned::to_owned),
                            name: block
                                .get("name")
                                .and_then(JsonValue::as_str)
                                .map(ToOwned::to_owned),
                            arguments: String::new(),
                        },
                    );
                }
            }
            "content_block_delta" => {
                let delta = value.get("delta").cloned().unwrap_or(JsonValue::Null);
                match delta
                    .get("type")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                {
                    "text_delta" => {
                        let text = delta
                            .get("text")
                            .and_then(JsonValue::as_str)
                            .unwrap_or_default()
                            .to_string();
                        if !text.is_empty() {
                            message.push_str(&text);
                            emit(ProviderStreamEvent::MessageDelta(text))?;
                        }
                    }
                    "input_json_delta" => {
                        let index = value
                            .get("index")
                            .and_then(JsonValue::as_u64)
                            .unwrap_or_default() as usize;
                        let partial_json = delta
                            .get("partial_json")
                            .and_then(JsonValue::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let partial = partial_calls.entry(index).or_default();
                        partial.arguments.push_str(&partial_json);
                        emit(ProviderStreamEvent::ToolDelta {
                            tool_call_id: partial.id.clone(),
                            tool_name: partial.name.clone(),
                            arguments_delta: partial_json,
                        })?;
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(usage_node) = value.get("usage") {
                    usage.output_tokens = usage_node
                        .get("output_tokens")
                        .and_then(JsonValue::as_u64)
                        .unwrap_or(usage.output_tokens);
                    if let Some(cache_read) = usage_node
                        .get("cache_read_input_tokens")
                        .and_then(JsonValue::as_u64)
                    {
                        usage.cache_read_tokens = cache_read;
                    }
                    if let Some(cache_write) = usage_node
                        .get("cache_creation_input_tokens")
                        .and_then(JsonValue::as_u64)
                    {
                        usage.cache_creation_tokens = cache_write;
                    }
                }
            }
            "message_stop" => {}
            _ => {}
        }
    }
    usage.total_tokens = usage
        .input_tokens
        .saturating_add(usage.output_tokens)
        .saturating_add(usage.cache_read_tokens)
        .saturating_add(usage.cache_creation_tokens);
    let usage = (usage.total_tokens > 0).then_some(usage);
    if let Some(usage) = usage.as_ref() {
        emit(ProviderStreamEvent::Usage(usage.clone()))?;
    }
    finish_provider_turn(provider_id, message, partial_calls, usage)
}

fn parse_anthropic_json_response(
    provider_id: &str,
    response_text: &str,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<ProviderTurnOutcome> {
    let value: JsonValue = serde_json::from_str(response_text).map_err(|error| {
        CommandError::retryable(
            "agent_provider_response_decode_failed",
            format!("Xero could not decode the {provider_id} response: {error}"),
        )
    })?;
    let mut message = String::new();
    let mut partial_calls = BTreeMap::new();
    for (index, block) in value
        .get("content")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        match block
            .get("type")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
        {
            "text" => {
                let text = block
                    .get("text")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_string();
                if !text.is_empty() {
                    message.push_str(&text);
                    emit(ProviderStreamEvent::MessageDelta(text))?;
                }
            }
            "tool_use" => {
                partial_calls.insert(
                    index,
                    PartialToolCall {
                        id: block
                            .get("id")
                            .and_then(JsonValue::as_str)
                            .map(ToOwned::to_owned),
                        name: block
                            .get("name")
                            .and_then(JsonValue::as_str)
                            .map(ToOwned::to_owned),
                        arguments: block
                            .get("input")
                            .map(JsonValue::to_string)
                            .unwrap_or_else(|| "{}".into()),
                    },
                );
            }
            _ => {}
        }
    }
    let usage = value.get("usage").map(|usage| {
        let input_tokens = usage
            .get("input_tokens")
            .and_then(JsonValue::as_u64)
            .unwrap_or_default();
        let output_tokens = usage
            .get("output_tokens")
            .and_then(JsonValue::as_u64)
            .unwrap_or_default();
        let cache_read_tokens = usage
            .get("cache_read_input_tokens")
            .and_then(JsonValue::as_u64)
            .unwrap_or_default();
        let cache_creation_tokens = usage
            .get("cache_creation_input_tokens")
            .and_then(JsonValue::as_u64)
            .unwrap_or_default();
        ProviderUsage {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens
                .saturating_add(output_tokens)
                .saturating_add(cache_read_tokens)
                .saturating_add(cache_creation_tokens),
            cache_read_tokens,
            cache_creation_tokens,
            reported_cost_micros: None,
        }
    });
    if let Some(usage) = usage.as_ref() {
        emit(ProviderStreamEvent::Usage(usage.clone()))?;
    }
    finish_provider_turn(provider_id, message, partial_calls, usage)
}

fn finish_provider_turn(
    provider_id: &str,
    message: String,
    partial_calls: BTreeMap<usize, PartialToolCall>,
    usage: Option<ProviderUsage>,
) -> CommandResult<ProviderTurnOutcome> {
    let mut tool_calls = Vec::new();
    let mut seen_ids = BTreeSet::new();
    for (index, partial) in partial_calls {
        let mut name = partial.name.ok_or_else(|| {
            CommandError::retryable(
                "agent_provider_tool_name_missing",
                format!(
                    "Xero received a {provider_id} tool call at index {index} without a function name."
                ),
            )
        })?;
        normalize_required(&mut name, "toolName")?;
        let input = if partial.arguments.trim().is_empty() {
            JsonValue::Object(JsonMap::new())
        } else {
            serde_json::from_str(&partial.arguments).map_err(|error| {
                CommandError::user_fixable(
                    "agent_provider_tool_arguments_invalid",
                    format!(
                        "Xero could not decode {provider_id} tool call `{name}` arguments as JSON: {error}"
                    ),
                )
            })?
        };
        let mut tool_call_id = partial
            .id
            .unwrap_or_else(|| format!("{provider_id}-tool-call-{}", index + 1));
        normalize_required(&mut tool_call_id, "toolCallId")?;
        if !seen_ids.insert(tool_call_id.clone()) {
            return Err(CommandError::retryable(
                "agent_provider_tool_call_duplicate",
                format!(
                    "Xero received duplicate {provider_id} tool call id `{tool_call_id}` in the same provider turn."
                ),
            ));
        }
        tool_calls.push(AgentToolCall {
            tool_call_id,
            tool_name: name,
            input,
        });
    }

    if tool_calls.is_empty() {
        Ok(ProviderTurnOutcome::Complete { message, usage })
    } else {
        Ok(ProviderTurnOutcome::ToolCalls {
            message,
            tool_calls,
            usage,
        })
    }
}

fn send_provider_json_request<F>(provider_id: &str, mut build: F) -> CommandResult<Response>
where
    F: FnMut() -> reqwest::blocking::RequestBuilder,
{
    let mut last_transport_error = None;
    for attempt in 0..MAX_PROVIDER_ATTEMPTS {
        match build().send() {
            Ok(response) => {
                let status = response.status().as_u16();
                if response.status().is_success() {
                    return Ok(response);
                }
                if is_retryable_provider_status(status) && attempt + 1 < MAX_PROVIDER_ATTEMPTS {
                    let delay = retry_after_delay(response.headers())
                        .unwrap_or_else(|| retry_backoff(attempt));
                    std::thread::sleep(delay);
                    continue;
                }
                return ensure_success(provider_id, response);
            }
            Err(error) => {
                if attempt + 1 < MAX_PROVIDER_ATTEMPTS {
                    last_transport_error = Some(error.to_string());
                    std::thread::sleep(retry_backoff(attempt));
                    continue;
                }
                return Err(map_provider_transport_error(provider_id, error));
            }
        }
    }

    Err(CommandError::retryable(
        format!("{provider_id}_provider_unavailable"),
        format!(
            "Xero exhausted provider `{provider_id}` retry attempts{}.",
            last_transport_error
                .map(|error| format!(" Last transport error: {error}"))
                .unwrap_or_default()
        ),
    ))
}

fn is_retryable_provider_status(status: u16) -> bool {
    matches!(status, 408 | 409 | 425 | 429 | 500..=599)
}

fn retry_backoff(attempt: usize) -> Duration {
    Duration::from_millis(250_u64.saturating_mul(1_u64 << attempt.min(3)))
}

fn retry_after_delay(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    value
        .parse::<u64>()
        .ok()
        .map(|seconds| Duration::from_secs(seconds.min(30)))
}

fn ensure_success(provider_id: &str, response: Response) -> CommandResult<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let status_code = status.as_u16();
    let body = response.text().unwrap_or_default();
    Err(provider_http_status_error(provider_id, status_code, &body))
}

fn map_provider_transport_error(provider_id: &str, error: reqwest::Error) -> CommandError {
    if error.is_timeout() {
        return CommandError::retryable(
            format!("{provider_id}_provider_timeout"),
            format!("Xero timed out while waiting for provider `{provider_id}`."),
        );
    }
    CommandError::retryable(
        format!("{provider_id}_provider_unavailable"),
        format!("Xero could not reach provider `{provider_id}`: {error}"),
    )
}

fn provider_http_status_error(provider_id: &str, status: u16, body: &str) -> CommandError {
    let excerpt = if provider_id == OPENAI_CODEX_PROVIDER_ID {
        openai_codex_friendly_error_message(status, body)
            .unwrap_or_else(|| redact_provider_error_body(body))
    } else {
        redact_provider_error_body(body)
    };
    let message = if excerpt.is_empty() {
        format!("Provider `{provider_id}` returned HTTP {status}.")
    } else {
        format!("Provider `{provider_id}` returned HTTP {status}: {excerpt}")
    };
    let message = if matches!(status, 401 | 403) && provider_id == OPENAI_CODEX_PROVIDER_ID {
        format!(
            "{message} Xero has a saved OpenAI Codex sign-in, but OpenAI rejected the live request. Reconnect OpenAI Codex in Settings, then retry."
        )
    } else {
        message
    };
    match status {
        401 | 403 => CommandError::user_fixable(format!("{provider_id}_auth_failed"), message),
        408 | 409 | 425 | 429 | 500..=599 => {
            CommandError::retryable(format!("{provider_id}_provider_unavailable"), message)
        }
        _ => CommandError::user_fixable(format!("{provider_id}_request_rejected"), message),
    }
}

fn openai_codex_friendly_error_message(status: u16, body: &str) -> Option<String> {
    let parsed: JsonValue = serde_json::from_str(body).ok()?;
    let error = parsed.get("error")?;
    let code = error
        .get("code")
        .or_else(|| error.get("type"))
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    if status != 429 && !is_openai_codex_usage_limit_code(code) {
        return error
            .get("message")
            .and_then(JsonValue::as_str)
            .map(redact_provider_error_body)
            .filter(|message| !message.is_empty());
    }

    let plan = error
        .get("plan_type")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
        .map(|plan| format!(" ({} plan)", plan.to_ascii_lowercase()))
        .unwrap_or_default();
    let when = error
        .get("resets_at")
        .and_then(JsonValue::as_i64)
        .and_then(openai_codex_usage_limit_reset_copy)
        .unwrap_or_default();
    Some(format!(
        "You have hit your ChatGPT usage limit{plan}.{when}"
    ))
}

fn is_openai_codex_usage_limit_code(code: &str) -> bool {
    let normalized = code.to_ascii_lowercase();
    normalized.contains("usage_limit_reached")
        || normalized.contains("usage_not_included")
        || normalized.contains("rate_limit_exceeded")
}

fn openai_codex_usage_limit_reset_copy(resets_at: i64) -> Option<String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    let seconds = resets_at.saturating_sub(now);
    let minutes = ((seconds as f64) / 60.0).round().max(0.0) as i64;
    Some(format!(" Try again in ~{minutes} min."))
}

fn provider_status_error(provider_id: &str, status: i32, stderr: &str) -> CommandError {
    let excerpt = redact_provider_error_body(stderr);
    CommandError::retryable(
        format!("{provider_id}_provider_unavailable"),
        format!("Provider `{provider_id}` command exited with status {status}: {excerpt}"),
    )
}

fn redact_provider_error_body(body: &str) -> String {
    let mut text = body.replace('\n', " ");
    if find_prohibited_persistence_content(&text).is_some() {
        return "provider error body redacted because it may contain credential material.".into();
    }
    if text.chars().count() > 600 {
        text = text.chars().take(599).collect::<String>();
        text.push_str("...");
    }
    text
}

fn wait_provider_cli(
    child: &mut std::process::Child,
    timeout: Duration,
    provider_id: &str,
) -> CommandResult<std::process::ExitStatus> {
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                cleanup_process_group_after_root_exit(child.id());
                return Ok(status);
            }
            Ok(None) if started.elapsed() >= timeout => {
                let _ = terminate_process_tree(child);
                return Err(CommandError::retryable(
                    format!("{provider_id}_provider_timeout"),
                    format!("Xero timed out while waiting for provider `{provider_id}`."),
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = terminate_process_tree(child);
                return Err(CommandError::retryable(
                    format!("{provider_id}_provider_wait_failed"),
                    format!("Xero could not observe provider `{provider_id}`: {error}"),
                ));
            }
        }
    }
}

fn read_child_stderr(mut child: std::process::Child) -> CommandResult<String> {
    use std::io::Read as _;

    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    Ok(stderr)
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
                "Xero needs GOOGLE_OAUTH_ACCESS_TOKEN or the gcloud CLI to invoke Vertex AI from the owned provider adapter.",
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

impl Default for OpenAiResponsesProviderConfig {
    fn default() -> Self {
        Self {
            provider_id: OPENAI_API_PROVIDER_ID.into(),
            model_id: "gpt-5.4".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: String::new(),
            timeout_ms: DEFAULT_PROVIDER_TIMEOUT_MS,
        }
    }
}

impl Default for OpenAiCodexResponsesProviderConfig {
    fn default() -> Self {
        Self {
            provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
            model_id: OPENAI_CODEX_DEFAULT_MODEL_ID.into(),
            base_url: OPENAI_CODEX_API_BASE_URL.into(),
            access_token: String::new(),
            account_id: String::new(),
            session_id: None,
            timeout_ms: DEFAULT_PROVIDER_TIMEOUT_MS,
        }
    }
}

impl Default for AnthropicProviderConfig {
    fn default() -> Self {
        Self {
            provider_id: ANTHROPIC_PROVIDER_ID.into(),
            model_id: String::new(),
            api_key: String::new(),
            base_url: ANTHROPIC_API_BASE_URL.into(),
            anthropic_version: ANTHROPIC_API_VERSION.into(),
            timeout_ms: DEFAULT_PROVIDER_TIMEOUT_MS,
        }
    }
}

#[allow(dead_code)]
fn _known_provider_ids() -> [&'static str; 10] {
    [
        OPENAI_CODEX_PROVIDER_ID,
        OPENAI_API_PROVIDER_ID,
        OPENROUTER_PROVIDER_ID,
        ANTHROPIC_PROVIDER_ID,
        GITHUB_MODELS_PROVIDER_ID,
        OLLAMA_PROVIDER_ID,
        AZURE_OPENAI_PROVIDER_ID,
        GEMINI_AI_STUDIO_PROVIDER_ID,
        BEDROCK_PROVIDER_ID,
        VERTEX_PROVIDER_ID,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{
        RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto,
        RuntimeRunControlStateDto,
    };

    fn test_request() -> ProviderTurnRequest {
        ProviderTurnRequest {
            system_prompt: "system".into(),
            messages: vec![ProviderMessage::User {
                content: "do work".into(),
                attachments: Vec::new(),
            }],
            tools: vec![AgentToolDescriptor {
                name: "read".into(),
                description: "Read a file.".into(),
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["path"],
                    "properties": { "path": { "type": "string" } },
                }),
            }],
            turn_index: 0,
            controls: RuntimeRunControlStateDto {
                active: RuntimeRunActiveControlSnapshotDto {
                    runtime_agent_id: RuntimeAgentIdDto::Engineer,
                    agent_definition_id: None,
                    agent_definition_version: None,
                    provider_profile_id: None,
                    model_id: "model".into(),
                    thinking_effort: None,
                    approval_mode: RuntimeRunApprovalModeDto::Yolo,
                    plan_mode_required: false,
                    revision: 1,
                    applied_at: "2026-04-24T00:00:00Z".into(),
                },
                pending: None,
            },
        }
    }

    #[test]
    fn openai_compatible_body_uses_stream_options_only_for_providers_that_support_it() {
        let request = test_request();

        let openrouter =
            openai_chat_request_body(OPENROUTER_PROVIDER_ID, "openai/gpt-4.1-mini", &request)
                .expect("openrouter body");
        assert_eq!(openrouter["stream"], true);
        assert_eq!(openrouter["stream_options"]["include_usage"], true);
        assert_eq!(openrouter["tools"][0]["function"]["name"], "read");

        let github =
            openai_chat_request_body(GITHUB_MODELS_PROVIDER_ID, "openai/gpt-4.1-mini", &request)
                .expect("github body");
        assert_eq!(github["stream_options"]["include_usage"], true);

        let ollama = openai_chat_request_body(OLLAMA_PROVIDER_ID, "llama3.1", &request)
            .expect("ollama body");
        assert_eq!(ollama["stream"], true);
        assert!(ollama.get("stream_options").is_none());

        let gemini =
            openai_chat_request_body(GEMINI_AI_STUDIO_PROVIDER_ID, "gemini-2.5-pro", &request)
                .expect("gemini body");
        assert_eq!(gemini["stream"], true);
        assert!(gemini.get("stream_options").is_none());
    }

    #[test]
    fn openai_usage_maps_cached_input_to_cache_read_bucket() {
        let usage = openai_provider_usage(1_000, 200, 1_200, 300, 0, None);

        assert_eq!(usage.input_tokens, 700);
        assert_eq!(usage.output_tokens, 200);
        assert_eq!(usage.cache_read_tokens, 300);
        assert_eq!(usage.total_tokens, 1_200);
        assert_eq!(usage.reported_cost_micros, None);

        let response_usage = openai_responses_usage(&json!({
            "input_tokens": 1_000,
            "output_tokens": 200,
            "input_tokens_details": { "cached_tokens": 300 }
        }));
        assert_eq!(response_usage.input_tokens, 700);
        assert_eq!(response_usage.cache_read_tokens, 300);
        assert_eq!(response_usage.total_tokens, 1_200);
    }

    #[test]
    fn openrouter_usage_carries_reported_cost_and_cache_write_tokens() {
        let usage: OpenAiUsage = serde_json::from_value(json!({
            "prompt_tokens": 1_000,
            "completion_tokens": 200,
            "total_tokens": 1_200,
            "cost": 0.123456,
            "prompt_tokens_details": {
                "cached_tokens": 300,
                "cache_write_tokens": 100
            }
        }))
        .expect("openrouter usage");

        let mapped = openai_provider_usage(
            usage.prompt_tokens,
            usage.completion_tokens,
            usage.total_tokens,
            openai_usage_cache_read_tokens(&usage),
            openai_usage_cache_creation_tokens(&usage),
            openai_reported_cost_micros(OPENROUTER_PROVIDER_ID, &usage),
        );

        assert_eq!(mapped.input_tokens, 600);
        assert_eq!(mapped.cache_read_tokens, 300);
        assert_eq!(mapped.cache_creation_tokens, 100);
        assert_eq!(mapped.total_tokens, 1_200);
        assert_eq!(mapped.reported_cost_micros, Some(123_456));
        assert_eq!(
            openai_reported_cost_micros(OPENAI_API_PROVIDER_ID, &usage),
            None
        );
    }

    #[test]
    fn openai_codex_responses_body_matches_gsd_reasoning_clamps() {
        let mut request = test_request();
        request.controls.active.thinking_effort = Some(ProviderModelThinkingEffortDto::Minimal);

        let gpt_5_4 = openai_codex_responses_request_body(
            OPENAI_CODEX_PROVIDER_ID,
            "gpt-5.4",
            &request,
            Some("session-1"),
        )
        .expect("gpt-5.4 body");
        assert_eq!(gpt_5_4["reasoning"]["effort"], "low");
        assert_eq!(gpt_5_4["reasoning"]["summary"], "auto");
        assert_eq!(gpt_5_4["store"], false);
        assert_eq!(gpt_5_4["stream"], true);
        assert_eq!(gpt_5_4["text"]["verbosity"], "medium");
        assert_eq!(gpt_5_4["include"][0], "reasoning.encrypted_content");
        assert_eq!(gpt_5_4["prompt_cache_key"], "session-1");
        assert_eq!(gpt_5_4["tool_choice"], "auto");
        assert_eq!(gpt_5_4["parallel_tool_calls"], true);
        assert_eq!(gpt_5_4["tools"][0]["strict"], JsonValue::Null);
        assert_eq!(gpt_5_4["input"][0]["content"][0]["type"], "input_text");

        let gpt_5_5 = openai_codex_responses_request_body(
            OPENAI_CODEX_PROVIDER_ID,
            "openai/gpt-5.5",
            &request,
            None,
        )
        .expect("gpt-5.5 body");
        assert_eq!(gpt_5_5["reasoning"]["effort"], "low");
        assert!(gpt_5_5.get("prompt_cache_key").is_none());

        let openai_api = openai_responses_request_body(OPENAI_API_PROVIDER_ID, "gpt-5.4", &request)
            .expect("openai api body");
        assert_eq!(openai_api["reasoning"]["effort"], "minimal");
        assert_eq!(openai_api["reasoning"]["summary"], "auto");

        request.controls.active.thinking_effort = Some(ProviderModelThinkingEffortDto::XHigh);
        let gpt_5_1 = openai_codex_responses_request_body(
            OPENAI_CODEX_PROVIDER_ID,
            "gpt-5.1",
            &request,
            None,
        )
        .expect("gpt-5.1 body");
        assert_eq!(gpt_5_1["reasoning"]["effort"], "high");
    }

    #[test]
    fn openai_codex_url_and_headers_match_chatgpt_backend_contract() {
        assert_eq!(
            openai_codex_responses_url("https://chatgpt.com/backend-api")
                .expect("base url")
                .as_str(),
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(
            openai_codex_responses_url("https://chatgpt.com/backend-api/codex")
                .expect("codex url")
                .as_str(),
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(
            openai_codex_responses_url("https://chatgpt.com/backend-api/codex/responses")
                .expect("full url")
                .as_str(),
            "https://chatgpt.com/backend-api/codex/responses"
        );

        let config = OpenAiCodexResponsesProviderConfig {
            access_token: "oauth-token".into(),
            account_id: "acct_123".into(),
            session_id: Some("session-123".into()),
            ..OpenAiCodexResponsesProviderConfig::default()
        };
        let headers = openai_codex_request_headers(&config).expect("codex headers");
        assert_eq!(
            headers
                .get(AUTHORIZATION)
                .expect("authorization")
                .to_str()
                .expect("authorization value"),
            "Bearer oauth-token"
        );
        assert_eq!(
            headers
                .get("chatgpt-account-id")
                .expect("account")
                .to_str()
                .expect("account value"),
            "acct_123"
        );
        assert_eq!(
            headers
                .get("OpenAI-Beta")
                .expect("beta")
                .to_str()
                .expect("beta value"),
            "responses=experimental"
        );
        assert_eq!(
            headers
                .get("originator")
                .expect("originator")
                .to_str()
                .expect("originator value"),
            "pi"
        );
        assert_eq!(
            headers
                .get(ACCEPT)
                .expect("accept")
                .to_str()
                .expect("accept value"),
            "text/event-stream"
        );
        assert_eq!(
            headers
                .get(CONTENT_TYPE)
                .expect("content type")
                .to_str()
                .expect("content type value"),
            "application/json"
        );
        assert_eq!(
            headers
                .get("session_id")
                .expect("session")
                .to_str()
                .expect("session value"),
            "session-123"
        );
        assert!(headers
            .get(USER_AGENT)
            .expect("user agent")
            .to_str()
            .expect("user agent value")
            .starts_with("pi ("));
    }

    #[test]
    fn provider_urls_require_https_except_for_local_endpoints() {
        let hosted = openai_compatible_chat_url("http://api.example.com/v1", None)
            .expect_err("hosted HTTP provider should be rejected");
        assert_eq!(hosted.code, "agent_provider_url_insecure");

        let localhost = openai_compatible_chat_url("http://127.0.0.1:11434/v1", None)
            .expect("local HTTP provider should be allowed");
        assert_eq!(
            localhost.as_str(),
            "http://127.0.0.1:11434/v1/chat/completions"
        );

        let https = openai_compatible_chat_url("https://api.example.com/v1", Some("2026-01-01"))
            .expect("hosted HTTPS provider should be allowed");
        assert_eq!(
            https.as_str(),
            "https://api.example.com/v1/chat/completions?api-version=2026-01-01"
        );
    }

    #[test]
    fn anthropic_family_body_uses_provider_specific_model_placement() {
        let request = test_request();

        let native =
            anthropic_request_body(Some("claude-sonnet-4-5"), "2023-06-01", &request, true)
                .expect("native anthropic body");
        assert_eq!(native["model"], "claude-sonnet-4-5");
        assert!(native.get("anthropic_version").is_none());
        assert_eq!(native["stream"], true);

        let bedrock = anthropic_request_body(None, BEDROCK_ANTHROPIC_VERSION, &request, false)
            .expect("bedrock body");
        assert!(bedrock.get("model").is_none());
        assert_eq!(bedrock["anthropic_version"], BEDROCK_ANTHROPIC_VERSION);
        assert_eq!(bedrock["stream"], false);

        let vertex = anthropic_request_body(None, VERTEX_ANTHROPIC_VERSION, &request, false)
            .expect("vertex body");
        assert!(vertex.get("model").is_none());
        assert_eq!(vertex["anthropic_version"], VERTEX_ANTHROPIC_VERSION);
    }

    #[test]
    fn finish_provider_turn_decodes_tool_arguments_and_rejects_malformed_json() {
        let mut partial_calls = BTreeMap::new();
        partial_calls.insert(
            0,
            PartialToolCall {
                id: Some("call-1".into()),
                name: Some("read".into()),
                arguments: r#"{"path":"src/lib.rs"}"#.into(),
            },
        );

        let outcome = finish_provider_turn("test-provider", String::new(), partial_calls, None)
            .expect("provider turn");
        match outcome {
            ProviderTurnOutcome::ToolCalls { tool_calls, .. } => {
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].tool_call_id, "call-1");
                assert_eq!(tool_calls[0].tool_name, "read");
                assert_eq!(tool_calls[0].input["path"], "src/lib.rs");
            }
            ProviderTurnOutcome::Complete { .. } => panic!("expected tool call turn"),
        }

        let mut malformed = BTreeMap::new();
        malformed.insert(
            0,
            PartialToolCall {
                id: Some("call-2".into()),
                name: Some("read".into()),
                arguments: "{".into(),
            },
        );
        let error = finish_provider_turn("test-provider", String::new(), malformed, None)
            .expect_err("malformed tool JSON should fail");
        assert_eq!(error.code, "agent_provider_tool_arguments_invalid");
    }

    #[test]
    fn openai_responses_tool_item_uses_event_output_index() {
        let mut partial_calls = BTreeMap::new();
        partial_calls.insert(
            3,
            PartialToolCall {
                id: None,
                name: None,
                arguments: r#"{"path":"src/lib.rs"}"#.into(),
            },
        );
        let event = json!({
            "type": "response.output_item.done",
            "output_index": 3,
            "item": {
                "type": "function_call",
                "call_id": "call-1",
                "name": "read"
            }
        });

        assert!(apply_openai_response_function_call_item(
            &mut partial_calls,
            &event,
            0,
        ));
        let outcome =
            finish_provider_turn(OPENAI_CODEX_PROVIDER_ID, String::new(), partial_calls, None)
                .expect("provider turn");

        match outcome {
            ProviderTurnOutcome::ToolCalls { tool_calls, .. } => {
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].tool_call_id, "call-1");
                assert_eq!(tool_calls[0].tool_name, "read");
                assert_eq!(tool_calls[0].input["path"], "src/lib.rs");
            }
            ProviderTurnOutcome::Complete { .. } => panic!("expected tool call turn"),
        }
    }

    #[test]
    fn openai_responses_reasoning_summary_events_emit_thinking_deltas() {
        let mut events = Vec::new();
        let mut emit = |event| {
            events.push(event);
            Ok(())
        };

        assert!(emit_openai_responses_reasoning_summary_event(
            &json!({
                "type": "response.reasoning_summary_text.delta",
                "delta": "I should inspect the failing test"
            }),
            &mut emit,
        )
        .expect("delta event handled"));
        assert!(emit_openai_responses_reasoning_summary_event(
            &json!({ "type": "response.reasoning_summary_part.done" }),
            &mut emit,
        )
        .expect("part done event handled"));
        assert!(!emit_openai_responses_reasoning_summary_event(
            &json!({ "type": "response.output_text.delta", "delta": "Done." }),
            &mut emit,
        )
        .expect("text event ignored"));

        assert_eq!(
            events,
            vec![
                ProviderStreamEvent::ReasoningSummary("I should inspect the failing test".into()),
                ProviderStreamEvent::ReasoningSummary("\n\n".into()),
            ]
        );
    }

    #[test]
    fn finish_provider_turn_rejects_blank_and_duplicate_tool_identity() {
        let mut blank = BTreeMap::new();
        blank.insert(
            0,
            PartialToolCall {
                id: Some("   ".into()),
                name: Some("read".into()),
                arguments: r#"{"path":"src/lib.rs"}"#.into(),
            },
        );
        let error = finish_provider_turn("test-provider", String::new(), blank, None)
            .expect_err("blank tool id should fail");
        assert_eq!(error.code, "invalid_request");

        let mut duplicate = BTreeMap::new();
        duplicate.insert(
            0,
            PartialToolCall {
                id: Some("call-1".into()),
                name: Some("read".into()),
                arguments: r#"{"path":"src/lib.rs"}"#.into(),
            },
        );
        duplicate.insert(
            1,
            PartialToolCall {
                id: Some("call-1".into()),
                name: Some("read".into()),
                arguments: r#"{"path":"client/src-tauri/src/lib.rs"}"#.into(),
            },
        );
        let error = finish_provider_turn("test-provider", String::new(), duplicate, None)
            .expect_err("duplicate tool id should fail");
        assert_eq!(error.code, "agent_provider_tool_call_duplicate");

        let mut blank_name = BTreeMap::new();
        blank_name.insert(
            0,
            PartialToolCall {
                id: Some("call-1".into()),
                name: Some("   ".into()),
                arguments: "{}".into(),
            },
        );
        let error = finish_provider_turn("test-provider", String::new(), blank_name, None)
            .expect_err("blank tool name should fail");
        assert_eq!(error.code, "invalid_request");
    }

    #[test]
    fn provider_adapter_factory_constructs_every_supported_provider_family() {
        let configs = vec![
            AgentProviderConfig::OpenAiCodexResponses(OpenAiCodexResponsesProviderConfig {
                provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
                model_id: OPENAI_CODEX_DEFAULT_MODEL_ID.into(),
                base_url: "https://chatgpt.com/backend-api".into(),
                access_token: "test-token".into(),
                account_id: "acct_123".into(),
                session_id: Some("session-123".into()),
                timeout_ms: 1_000,
            }),
            AgentProviderConfig::OpenAiResponses(OpenAiResponsesProviderConfig {
                provider_id: OPENAI_API_PROVIDER_ID.into(),
                model_id: "gpt-5.4".into(),
                base_url: "https://api.openai.com/v1".into(),
                api_key: "test-key".into(),
                timeout_ms: 1_000,
            }),
            AgentProviderConfig::OpenAiCompatible(OpenAiCompatibleProviderConfig {
                provider_id: OPENAI_API_PROVIDER_ID.into(),
                model_id: "gpt-4.1-mini".into(),
                base_url: "https://api.openai.com/v1".into(),
                api_key: Some("test-key".into()),
                api_version: None,
                timeout_ms: 1_000,
            }),
            AgentProviderConfig::OpenAiCompatible(OpenAiCompatibleProviderConfig {
                provider_id: OPENROUTER_PROVIDER_ID.into(),
                model_id: "openai/gpt-4.1-mini".into(),
                base_url: "https://openrouter.ai/api/v1".into(),
                api_key: Some("test-key".into()),
                api_version: None,
                timeout_ms: 1_000,
            }),
            AgentProviderConfig::OpenAiCompatible(OpenAiCompatibleProviderConfig {
                provider_id: GITHUB_MODELS_PROVIDER_ID.into(),
                model_id: "openai/gpt-4.1-mini".into(),
                base_url: "https://models.github.ai/inference".into(),
                api_key: Some("test-key".into()),
                api_version: None,
                timeout_ms: 1_000,
            }),
            AgentProviderConfig::OpenAiCompatible(OpenAiCompatibleProviderConfig {
                provider_id: OLLAMA_PROVIDER_ID.into(),
                model_id: "llama3.1".into(),
                base_url: "http://127.0.0.1:11434/v1".into(),
                api_key: None,
                api_version: None,
                timeout_ms: 1_000,
            }),
            AgentProviderConfig::OpenAiCompatible(OpenAiCompatibleProviderConfig {
                provider_id: AZURE_OPENAI_PROVIDER_ID.into(),
                model_id: "deployment-name".into(),
                base_url: "https://example.openai.azure.com/openai/deployments/deployment-name"
                    .into(),
                api_key: Some("test-key".into()),
                api_version: Some("2025-04-01-preview".into()),
                timeout_ms: 1_000,
            }),
            AgentProviderConfig::OpenAiCompatible(OpenAiCompatibleProviderConfig {
                provider_id: GEMINI_AI_STUDIO_PROVIDER_ID.into(),
                model_id: "gemini-2.5-pro".into(),
                base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
                api_key: Some("test-key".into()),
                api_version: None,
                timeout_ms: 1_000,
            }),
            AgentProviderConfig::Anthropic(AnthropicProviderConfig {
                provider_id: ANTHROPIC_PROVIDER_ID.into(),
                model_id: "claude-sonnet-4-5".into(),
                api_key: "test-key".into(),
                base_url: ANTHROPIC_API_BASE_URL.into(),
                anthropic_version: ANTHROPIC_API_VERSION.into(),
                timeout_ms: 1_000,
            }),
            AgentProviderConfig::Bedrock(BedrockProviderConfig {
                model_id: "anthropic.claude-3-5-sonnet-20241022-v2:0".into(),
                region: "us-east-1".into(),
                timeout_ms: 1_000,
            }),
            AgentProviderConfig::Vertex(VertexProviderConfig {
                model_id: "claude-sonnet-4-5".into(),
                region: "us-central1".into(),
                project_id: "test-project".into(),
                timeout_ms: 1_000,
            }),
        ];

        for config in configs {
            let adapter = create_provider_adapter(config).expect("adapter should construct");
            assert!(!adapter.provider_id().trim().is_empty());
            assert!(!adapter.model_id().trim().is_empty());
        }
    }

    #[test]
    fn openai_compatible_auth_header_matches_provider_wire_contract() {
        assert_eq!(
            openai_compatible_auth_header_name(AZURE_OPENAI_PROVIDER_ID),
            "api-key"
        );
        assert_eq!(
            openai_compatible_auth_header_name(OPENAI_API_PROVIDER_ID),
            "Authorization"
        );
        assert_eq!(
            openai_compatible_auth_header_name(GITHUB_MODELS_PROVIDER_ID),
            "Authorization"
        );
        assert_eq!(
            openai_compatible_auth_header_name(GEMINI_AI_STUDIO_PROVIDER_ID),
            "Authorization"
        );
    }

    #[test]
    fn provider_error_redaction_catches_common_secret_shapes() {
        assert_eq!(
            redact_provider_error_body("upstream echoed x-api-key: github_pat_live_secret"),
            "provider error body redacted because it may contain credential material."
        );
        assert_eq!(
            redact_provider_error_body("request failed with client-secret=abc123"),
            "provider error body redacted because it may contain credential material."
        );
    }

    #[test]
    fn openai_codex_auth_failure_explains_saved_session_can_still_be_rejected() {
        let error = provider_http_status_error(OPENAI_CODEX_PROVIDER_ID, 401, "unauthorized");

        assert_eq!(error.code, "openai_codex_auth_failed");
        assert!(error.message.contains("saved OpenAI Codex sign-in"));
        assert!(error.message.contains("OpenAI rejected the live request"));
        assert!(error.message.contains("Reconnect OpenAI Codex"));
    }

    #[test]
    fn openai_codex_provider_errors_map_usage_limits_and_stream_failures() {
        let usage = provider_http_status_error(
            OPENAI_CODEX_PROVIDER_ID,
            429,
            r#"{"error":{"code":"usage_limit_reached","plan_type":"PLUS","resets_at":1777488000}}"#,
        );
        assert_eq!(usage.code, "openai_codex_provider_unavailable");
        assert!(usage.message.contains("ChatGPT usage limit"));
        assert!(usage.message.contains("plus plan"));

        let stream_error = openai_responses_stream_error(
            OPENAI_CODEX_PROVIDER_ID,
            &json!({
                "type": "error",
                "error": {
                    "type": "server_error",
                    "code": "server_error",
                    "message": "temporary outage"
                }
            }),
        );
        assert_eq!(stream_error.code, "openai_codex_stream_failed");
        assert!(stream_error.message.contains("Codex server_error"));

        let failed = openai_responses_stream_error(
            OPENAI_CODEX_PROVIDER_ID,
            &json!({
                "type": "response.failed",
                "response": {
                    "error": {
                        "message": "bad response"
                    }
                }
            }),
        );
        assert_eq!(failed.code, "openai_codex_stream_failed");
        assert!(failed.message.contains("bad response"));
    }

    #[test]
    fn anthropic_user_blocks_emit_image_document_and_text_blocks() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("temp dir");

        let image_path = dir.path().join("snap.png");
        std::fs::write(&image_path, b"\x89PNG\r\n\x1a\nfake-image-bytes")
            .expect("write image fixture");

        let pdf_path = dir.path().join("notes.pdf");
        std::fs::write(&pdf_path, b"%PDF-1.4 fake-pdf-bytes").expect("write pdf fixture");

        let text_path = dir.path().join("scratch.md");
        let mut text_file = std::fs::File::create(&text_path).expect("create text fixture");
        text_file
            .write_all(b"# heading\nbody line")
            .expect("write text fixture");

        let attachments = vec![
            MessageAttachment {
                kind: MessageAttachmentKind::Image,
                absolute_path: image_path.clone(),
                media_type: "image/png".into(),
                original_name: "snap.png".into(),
                size_bytes: 10,
                width: Some(640),
                height: Some(480),
            },
            MessageAttachment {
                kind: MessageAttachmentKind::Document,
                absolute_path: pdf_path.clone(),
                media_type: "application/pdf".into(),
                original_name: "notes.pdf".into(),
                size_bytes: 20,
                width: None,
                height: None,
            },
            MessageAttachment {
                kind: MessageAttachmentKind::Text,
                absolute_path: text_path.clone(),
                media_type: "text/markdown".into(),
                original_name: "scratch.md".into(),
                size_bytes: 0,
                width: None,
                height: None,
            },
        ];

        let blocks = anthropic_user_content_blocks("describe these", &attachments)
            .expect("build content blocks");

        assert_eq!(blocks.len(), 4, "image, document, text-file, prompt");
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["source"]["type"], "base64");
        assert_eq!(blocks[0]["source"]["media_type"], "image/png");
        assert!(!blocks[0]["source"]["data"].as_str().unwrap().is_empty());

        assert_eq!(blocks[1]["type"], "document");
        assert_eq!(blocks[1]["source"]["type"], "base64");
        assert_eq!(blocks[1]["source"]["media_type"], "application/pdf");

        assert_eq!(blocks[2]["type"], "text");
        let inlined = blocks[2]["text"].as_str().expect("text block");
        assert!(inlined.contains("scratch.md"));
        assert!(inlined.contains("# heading"));

        assert_eq!(blocks[3]["type"], "text");
        assert_eq!(blocks[3]["text"], "describe these");
    }

    #[test]
    fn anthropic_user_blocks_handles_empty_prompt_with_attachment() {
        let dir = tempfile::tempdir().expect("temp dir");
        let image_path = dir.path().join("solo.png");
        std::fs::write(&image_path, b"\x89PNG").expect("write png fixture");

        let attachments = vec![MessageAttachment {
            kind: MessageAttachmentKind::Image,
            absolute_path: image_path,
            media_type: "image/png".into(),
            original_name: "solo.png".into(),
            size_bytes: 4,
            width: None,
            height: None,
        }];

        let blocks = anthropic_user_content_blocks("", &attachments).expect("build content blocks");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "image");
    }

    #[test]
    fn anthropic_user_blocks_with_no_inputs_emits_empty_text_block() {
        let blocks = anthropic_user_content_blocks("", &[]).expect("empty blocks");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[0]["text"], "");
    }
}
