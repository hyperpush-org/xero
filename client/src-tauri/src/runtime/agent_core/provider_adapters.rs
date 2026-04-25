use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    time::Duration,
};

use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use serde::Deserialize;
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use tempfile::NamedTempFile;
use url::Url;

use super::{
    AgentToolCall, AgentToolDescriptor, FakeProviderAdapter, ProviderAdapter, ProviderMessage,
    ProviderStreamEvent, ProviderTurnOutcome, ProviderTurnRequest, ProviderUsage,
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
        OPENAI_API_PROVIDER_ID, OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID,
        VERTEX_PROVIDER_ID,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProviderConfig {
    Fake,
    OpenAiResponses(OpenAiResponsesProviderConfig),
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
        let body = openai_responses_request_body(&self.config.model_id, request)?;
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
                .header("X-Title", "Cadence");
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
                format!("Cadence could not allocate a Bedrock request file: {error}"),
            )
        })?;
        serde_json::to_writer(body_file.as_file_mut(), &body).map_err(|error| {
            CommandError::retryable(
                "bedrock_request_write_failed",
                format!("Cadence could not write the Bedrock request body: {error}"),
            )
        })?;
        body_file.as_file_mut().flush().map_err(|error| {
            CommandError::retryable(
                "bedrock_request_write_failed",
                format!("Cadence could not flush the Bedrock request body: {error}"),
            )
        })?;
        let body_arg = format!("fileb://{}", body_file.path().display());
        let output = NamedTempFile::new().map_err(|error| {
            CommandError::retryable(
                "bedrock_tempfile_failed",
                format!("Cadence could not allocate a Bedrock response file: {error}"),
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
                    "Cadence needs the AWS CLI to invoke Amazon Bedrock from the owned provider adapter.",
                ),
                _ => CommandError::retryable(
                    "bedrock_invoke_spawn_failed",
                    format!("Cadence could not start the AWS CLI Bedrock invocation: {error}"),
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
                format!("Cadence could not read the Bedrock response body: {error}"),
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
                format!("Cadence could not read the Vertex AI response body: {error}"),
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
                format!("Cadence could not build an HTTP client for the provider adapter: {error}"),
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
            format!("Cadence rejected provider base URL `{base_url}`: {error}"),
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
                "Cadence refused provider base URL `{base_url}` because hosted provider traffic must use HTTPS. HTTP is only allowed for local providers."
            ),
        )),
        scheme => Err(CommandError::user_fixable(
            "agent_provider_url_invalid",
            format!(
                "Cadence rejected provider base URL `{base_url}` because scheme `{scheme}` is not supported."
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
            format!("Cadence could not build the Vertex AI Anthropic endpoint: {error}"),
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
            ProviderMessage::User { content } => {
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
            json!({ "effort": thinking_effort_value(effort) }),
        );
    }
    Ok(JsonValue::Object(body))
}

fn openai_response_input(request: &ProviderTurnRequest) -> CommandResult<Vec<JsonValue>> {
    let mut input = Vec::new();
    for message in &request.messages {
        match message {
            ProviderMessage::User { content } => {
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

fn openai_response_tool(tool: &AgentToolDescriptor) -> JsonValue {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.input_schema,
    })
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
        JsonValue::Array(anthropic_messages(request)),
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

fn anthropic_messages(request: &ProviderTurnRequest) -> Vec<JsonValue> {
    let mut messages = Vec::new();
    for message in &request.messages {
        match message {
            ProviderMessage::User { content } => {
                messages.push(json!({
                    "role": "user",
                    "content": [{ "type": "text", "text": content }],
                }));
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
    messages
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
                format!("Cadence lost the {provider_id} response stream: {error}"),
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
                format!("Cadence could not decode a {provider_id} stream chunk: {error}"),
            )
        })?;
        if let Some(next_usage) = chunk.usage {
            let mapped = ProviderUsage {
                input_tokens: next_usage.prompt_tokens,
                output_tokens: next_usage.completion_tokens,
                total_tokens: next_usage.total_tokens,
            };
            emit(ProviderStreamEvent::Usage(mapped.clone()))?;
            usage = Some(mapped);
        }
        for choice in chunk.choices {
            if let Some(content) = choice.delta.content {
                message.push_str(&content);
                emit(ProviderStreamEvent::MessageDelta(content))?;
            }
            for tool_call in choice.delta.tool_calls {
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
                format!("Cadence lost the {provider_id} Responses stream: {error}"),
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
                format!("Cadence could not decode a {provider_id} Responses chunk: {error}"),
            )
        })?;
        match value
            .get("type")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
        {
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
            "response.output_item.done" => {
                let item = value.get("item").cloned().unwrap_or(JsonValue::Null);
                if item.get("type").and_then(JsonValue::as_str) == Some("function_call") {
                    let index = item
                        .get("output_index")
                        .and_then(JsonValue::as_u64)
                        .unwrap_or(completed_call_count as u64)
                        as usize;
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
                    completed_call_count = completed_call_count.saturating_add(1);
                }
            }
            "response.completed" => {
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

fn openai_responses_usage(value: &JsonValue) -> ProviderUsage {
    let input_tokens = value
        .get("input_tokens")
        .and_then(JsonValue::as_u64)
        .unwrap_or_default();
    let output_tokens = value
        .get("output_tokens")
        .and_then(JsonValue::as_u64)
        .unwrap_or_default();
    ProviderUsage {
        input_tokens,
        output_tokens,
        total_tokens: value
            .get("total_tokens")
            .and_then(JsonValue::as_u64)
            .unwrap_or_else(|| input_tokens.saturating_add(output_tokens)),
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
                format!("Cadence lost the {provider_id} response stream: {error}"),
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
                format!("Cadence could not decode a {provider_id} stream chunk: {error}"),
            )
        })?;
        match value
            .get("type")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
        {
            "message_start" => {
                usage.input_tokens = value
                    .get("message")
                    .and_then(|message| message.get("usage"))
                    .and_then(|usage| usage.get("input_tokens"))
                    .and_then(JsonValue::as_u64)
                    .unwrap_or_default();
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
                usage.output_tokens = value
                    .get("usage")
                    .and_then(|usage| usage.get("output_tokens"))
                    .and_then(JsonValue::as_u64)
                    .unwrap_or(usage.output_tokens);
            }
            "message_stop" => {}
            _ => {}
        }
    }
    usage.total_tokens = usage.input_tokens.saturating_add(usage.output_tokens);
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
            format!("Cadence could not decode the {provider_id} response: {error}"),
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
        ProviderUsage {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens.saturating_add(output_tokens),
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
                    "Cadence received a {provider_id} tool call at index {index} without a function name."
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
                        "Cadence could not decode {provider_id} tool call `{name}` arguments as JSON: {error}"
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
                    "Cadence received duplicate {provider_id} tool call id `{tool_call_id}` in the same provider turn."
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
            "Cadence exhausted provider `{provider_id}` retry attempts{}.",
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
            format!("Cadence timed out while waiting for provider `{provider_id}`."),
        );
    }
    CommandError::retryable(
        format!("{provider_id}_provider_unavailable"),
        format!("Cadence could not reach provider `{provider_id}`: {error}"),
    )
}

fn provider_http_status_error(provider_id: &str, status: u16, body: &str) -> CommandError {
    let excerpt = redact_provider_error_body(body);
    let message = if excerpt.is_empty() {
        format!("Provider `{provider_id}` returned HTTP {status}.")
    } else {
        format!("Provider `{provider_id}` returned HTTP {status}: {excerpt}")
    };
    match status {
        401 | 403 => CommandError::user_fixable(format!("{provider_id}_auth_failed"), message),
        408 | 409 | 425 | 429 | 500..=599 => {
            CommandError::retryable(format!("{provider_id}_provider_unavailable"), message)
        }
        _ => CommandError::user_fixable(format!("{provider_id}_request_rejected"), message),
    }
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
                    format!("Cadence timed out while waiting for provider `{provider_id}`."),
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = terminate_process_tree(child);
                return Err(CommandError::retryable(
                    format!("{provider_id}_provider_wait_failed"),
                    format!("Cadence could not observe provider `{provider_id}`: {error}"),
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
                "Cadence needs GOOGLE_OAUTH_ACCESS_TOKEN or the gcloud CLI to invoke Vertex AI from the owned provider adapter.",
            ),
            _ => CommandError::retryable(
                "vertex_gcloud_failed",
                format!("Cadence could not start gcloud to obtain a Vertex AI access token: {error}"),
            ),
        })?;
    if !output.status.success() {
        return Err(CommandError::user_fixable(
            "vertex_adc_missing",
            "Cadence could not obtain a Vertex AI access token from Application Default Credentials.",
        ));
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if token.is_empty() {
        return Err(CommandError::user_fixable(
            "vertex_adc_missing",
            "Cadence received an empty Vertex AI access token from gcloud.",
        ));
    }
    Ok(token)
}

impl Default for OpenAiResponsesProviderConfig {
    fn default() -> Self {
        Self {
            provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
            model_id: "gpt-5.2-codex".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: String::new(),
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
        RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto, RuntimeRunControlStateDto,
    };

    fn test_request() -> ProviderTurnRequest {
        ProviderTurnRequest {
            system_prompt: "system".into(),
            messages: vec![ProviderMessage::User {
                content: "do work".into(),
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
            AgentProviderConfig::OpenAiResponses(OpenAiResponsesProviderConfig {
                provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
                model_id: "gpt-5.2-codex".into(),
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
}
