use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc, Mutex, OnceLock,
    },
    thread::JoinHandle,
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER, USER_AGENT,
};
use reqwest::{
    blocking::{Client as BlockingClient, Response as BlockingResponse},
    Client as AsyncClient, RequestBuilder as AsyncRequestBuilder, Response as AsyncResponse,
};
use serde::Deserialize;
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use url::Url;

use super::{
    cancelled_error, AgentRunCancellationToken, AgentToolCall, AgentToolDescriptor,
    FakeProviderAdapter, MessageAttachment, MessageAttachmentKind, ProviderAdapter,
    ProviderMessage, ProviderStreamEvent, ProviderTurnOutcome, ProviderTurnOutputAllowance,
    ProviderTurnRequest, ProviderUsage, SessionContextLimitResolutionDto,
    SessionContextLimitSourceDto,
};
#[cfg(test)]
use super::{ProviderContextProvenance, ProviderContextSourceKind};
use crate::{
    commands::{
        heuristic_token_estimate, resolve_context_limit_with_provider_preflight, CommandError,
        CommandResult, ProviderModelThinkingEffortDto, SessionContextEstimateConfidenceDto,
        SessionContextEstimateDto, SessionContextEstimateSourceDto,
    },
    runtime::{
        is_supported_xai_reasoning_effort_model_id, is_supported_xai_text_model_id,
        process_tree::{
            cleanup_process_group_after_root_exit, configure_process_tree_root,
            register_process_tree_root, terminate_process_tree,
        },
        redaction::find_prohibited_persistence_content,
        ANTHROPIC_PROVIDER_ID, AZURE_OPENAI_PROVIDER_ID, BEDROCK_PROVIDER_ID, DEEPSEEK_PROVIDER_ID,
        GEMINI_AI_STUDIO_PROVIDER_ID, GITHUB_MODELS_PROVIDER_ID, OLLAMA_PROVIDER_ID,
        OPENAI_API_PROVIDER_ID, OPENAI_CODEX_DEFAULT_MODEL_ID, OPENAI_CODEX_PROVIDER_ID,
        OPENROUTER_PROVIDER_ID, VERTEX_PROVIDER_ID, XAI_DEFAULT_MODEL_ID, XAI_PROVIDER_ID,
    },
};

const DEFAULT_PROVIDER_TIMEOUT_MS: u64 = 120_000;
const MAX_PROVIDER_ATTEMPTS: usize = 3;
const ANTHROPIC_API_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const ANTHROPIC_ASSISTANT_CONTEXT_PREAMBLE: &str =
    "Continue from the assistant context provided in the next message.";
const BEDROCK_ANTHROPIC_VERSION: &str = "bedrock-2023-05-31";
const VERTEX_ANTHROPIC_VERSION: &str = "vertex-2023-10-16";
const GITHUB_MODELS_API_VERSION: &str = "2026-03-10";
const OPENAI_CODEX_API_BASE_URL: &str = "https://chatgpt.com/backend-api";
const OPENAI_CODEX_BETA_HEADER: &str = "responses=experimental";
const OPENAI_CODEX_ORIGINATOR: &str = "pi";
const OPENAI_CODEX_TEXT_VERBOSITY: &str = "medium";
const OPENAI_REASONING_SUMMARY_DETAIL: &str = "detailed";
const XAI_API_BASE_URL: &str = "https://api.x.ai/v1";
const MAX_PROVIDER_STREAM_LINE_BYTES: usize = 2 * 1_024 * 1_024;
const MAX_PROVIDER_STREAM_TOTAL_BYTES: usize = 64 * 1_024 * 1_024;
const MAX_PROVIDER_STREAM_LINES: usize = 262_144;
const MAX_PROVIDER_STREAM_EVENTS: usize = 131_072;
const MAX_PROVIDER_MESSAGE_BYTES: usize = 16 * 1_024 * 1_024;
const MAX_PROVIDER_REASONING_BYTES: usize = 32 * 1_024 * 1_024;
const MAX_PROVIDER_TOOL_ARGUMENT_BYTES: usize = 8 * 1_024 * 1_024;
const MAX_PROVIDER_TOOL_CALLS: usize = 256;
const MAX_PROVIDER_REASONING_ITEMS: usize = 4_096;
const MAX_PROVIDER_ITEMS_PER_EVENT: usize = 4_096;
const MAX_PROVIDER_REASONING_ID_BYTES: usize = 1_024;
const MAX_PROVIDER_NON_STREAM_BODY_BYTES: usize = MAX_PROVIDER_STREAM_TOTAL_BYTES;
const MAX_PROVIDER_TOKEN_COUNT_BODY_BYTES: usize = 256 * 1_024;
const MAX_PROVIDER_ERROR_BODY_BYTES: usize = 64 * 1_024;
const PROVIDER_STREAM_CHANNEL_CAPACITY: usize = 8;
const PROVIDER_STREAM_CHANNEL_CHUNK_BYTES: usize = 16 * 1_024;
const PROVIDER_STREAM_ABORT_POLL_INTERVAL: Duration = Duration::from_millis(5);
const PROVIDER_STREAM_BACKPRESSURE_POLL_INTERVAL: Duration = Duration::from_millis(1);

#[derive(Debug, Clone, Copy)]
struct ProviderStreamLimits {
    max_line_bytes: usize,
    max_total_bytes: usize,
    max_lines: usize,
    max_events: usize,
}

const PROVIDER_STREAM_LIMITS: ProviderStreamLimits = ProviderStreamLimits {
    max_line_bytes: MAX_PROVIDER_STREAM_LINE_BYTES,
    max_total_bytes: MAX_PROVIDER_STREAM_TOTAL_BYTES,
    max_lines: MAX_PROVIDER_STREAM_LINES,
    max_events: MAX_PROVIDER_STREAM_EVENTS,
};

#[derive(Debug)]
enum ProviderStreamTransportMessage {
    Chunk(Vec<u8>),
    Complete,
    Error(CommandError),
}

struct ProviderStreamReader {
    receiver: Receiver<ProviderStreamTransportMessage>,
    current_chunk: Vec<u8>,
    current_offset: usize,
    stop: Arc<AtomicBool>,
    transport: Option<JoinHandle<()>>,
}

impl ProviderStreamReader {
    fn join_transport(&mut self) -> std::io::Result<()> {
        let Some(transport) = self.transport.take() else {
            return Ok(());
        };
        transport.join().map_err(|_| {
            command_error_as_io_error(CommandError::system_fault(
                "agent_provider_stream_transport_panicked",
                "Xero's provider stream transport stopped unexpectedly.",
            ))
        })
    }

    #[cfg(test)]
    fn transport_is_joined(&self) -> bool {
        self.transport.is_none()
    }
}

impl Read for ProviderStreamReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            if self.current_offset < self.current_chunk.len() {
                let available = &self.current_chunk[self.current_offset..];
                let count = available.len().min(output.len());
                output[..count].copy_from_slice(&available[..count]);
                self.current_offset = self.current_offset.saturating_add(count);
                return Ok(count);
            }

            self.current_chunk.clear();
            self.current_offset = 0;
            match self.receiver.recv() {
                Ok(ProviderStreamTransportMessage::Chunk(chunk)) if chunk.is_empty() => continue,
                Ok(ProviderStreamTransportMessage::Chunk(chunk)) => self.current_chunk = chunk,
                Ok(ProviderStreamTransportMessage::Complete) => {
                    self.join_transport()?;
                    return Ok(0);
                }
                Ok(ProviderStreamTransportMessage::Error(error)) => {
                    self.join_transport()?;
                    return Err(command_error_as_io_error(error));
                }
                Err(_) => {
                    self.join_transport()?;
                    return Err(command_error_as_io_error(CommandError::system_fault(
                        "agent_provider_stream_transport_closed",
                        "Xero's provider stream transport closed without a terminal result.",
                    )));
                }
            }
        }
    }
}

impl Drop for ProviderStreamReader {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.join_transport();
    }
}

fn command_error_as_io_error(error: CommandError) -> std::io::Error {
    std::io::Error::other(error)
}

fn provider_stream_read_error(
    provider_id: &str,
    stream_name: &str,
    error: std::io::Error,
) -> CommandError {
    if let Some(command_error) = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<CommandError>())
    {
        return command_error.clone();
    }
    CommandError::retryable(
        "agent_provider_stream_read_failed",
        format!("Xero lost the {provider_id} {stream_name}: {error}"),
    )
}

struct BoundedSseReader<R> {
    reader: R,
    limits: ProviderStreamLimits,
    total_bytes: usize,
    line_count: usize,
    event_count: usize,
}

impl<R: BufRead> BoundedSseReader<R> {
    fn new(reader: R, limits: ProviderStreamLimits) -> Self {
        Self {
            reader,
            limits,
            total_bytes: 0,
            line_count: 0,
            event_count: 0,
        }
    }

    fn next_data(&mut self, provider_id: &str, stream_name: &str) -> CommandResult<Option<String>> {
        loop {
            let Some(mut line) = self.read_line(provider_id, stream_name)? else {
                return Ok(None);
            };
            while matches!(line.last(), Some(b'\n' | b'\r')) {
                line.pop();
            }
            let line = String::from_utf8(line).map_err(|error| {
                CommandError::retryable(
                    "agent_provider_stream_decode_failed",
                    format!(
                        "Xero could not decode a {provider_id} {stream_name} line as UTF-8: {error}"
                    ),
                )
            })?;
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            self.event_count = self.event_count.checked_add(1).ok_or_else(|| {
                provider_stream_limit_error(provider_id, stream_name, "event count overflowed")
            })?;
            if self.event_count > self.limits.max_events {
                return Err(provider_stream_limit_error(
                    provider_id,
                    stream_name,
                    &format!("event count exceeded {}", self.limits.max_events),
                ));
            }
            return Ok(Some(data.trim().to_owned()));
        }
    }

    fn read_line(
        &mut self,
        provider_id: &str,
        stream_name: &str,
    ) -> CommandResult<Option<Vec<u8>>> {
        let mut line = Vec::with_capacity(self.limits.max_line_bytes.min(8 * 1_024));
        loop {
            let available = self
                .reader
                .fill_buf()
                .map_err(|error| provider_stream_read_error(provider_id, stream_name, error))?;
            if available.is_empty() {
                if line.is_empty() {
                    return Ok(None);
                }
                break;
            }

            let bytes_to_take = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(available.len(), |index| index.saturating_add(1));
            let next_line_len = line.len().checked_add(bytes_to_take).ok_or_else(|| {
                provider_stream_limit_error(provider_id, stream_name, "line length overflowed")
            })?;
            if next_line_len > self.limits.max_line_bytes {
                return Err(provider_stream_limit_error(
                    provider_id,
                    stream_name,
                    &format!("line exceeded {} bytes", self.limits.max_line_bytes),
                ));
            }
            let next_total = self.total_bytes.checked_add(bytes_to_take).ok_or_else(|| {
                provider_stream_limit_error(provider_id, stream_name, "byte count overflowed")
            })?;
            if next_total > self.limits.max_total_bytes {
                return Err(provider_stream_limit_error(
                    provider_id,
                    stream_name,
                    &format!("stream exceeded {} bytes", self.limits.max_total_bytes),
                ));
            }

            line.extend_from_slice(&available[..bytes_to_take]);
            self.reader.consume(bytes_to_take);
            self.total_bytes = next_total;
            if line.last() == Some(&b'\n') {
                break;
            }
        }

        self.line_count = self.line_count.checked_add(1).ok_or_else(|| {
            provider_stream_limit_error(provider_id, stream_name, "line count overflowed")
        })?;
        if self.line_count > self.limits.max_lines {
            return Err(provider_stream_limit_error(
                provider_id,
                stream_name,
                &format!("line count exceeded {}", self.limits.max_lines),
            ));
        }
        Ok(Some(line))
    }
}

fn provider_stream_limit_error(provider_id: &str, stream_name: &str, detail: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_provider_stream_limit_exceeded",
        format!(
            "Xero stopped the {provider_id} {stream_name} because its {detail}. Try again or choose a different provider/model."
        ),
    )
}

fn provider_response_limit_error(provider_id: &str, body_name: &str, detail: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_provider_response_limit_exceeded",
        format!(
            "Xero rejected the {provider_id} {body_name} because its {detail}. Try again or choose a different provider/model."
        ),
    )
}

fn ensure_provider_response_item_capacity(
    current_items: usize,
    additional_items: usize,
    max_items: usize,
    provider_id: &str,
    item_name: &str,
) -> CommandResult<()> {
    let next_items = current_items.checked_add(additional_items).ok_or_else(|| {
        provider_response_limit_error(
            provider_id,
            "response body",
            &format!("{item_name} count overflowed"),
        )
    })?;
    if next_items > max_items {
        return Err(provider_response_limit_error(
            provider_id,
            "response body",
            &format!("{item_name} count exceeded {max_items}"),
        ));
    }
    Ok(())
}

fn append_provider_response_text(
    target: &mut String,
    delta: &str,
    max_bytes: usize,
    provider_id: &str,
    field_name: &str,
) -> CommandResult<()> {
    let next_len = target.len().checked_add(delta.len()).ok_or_else(|| {
        provider_response_limit_error(
            provider_id,
            "response body",
            &format!("{field_name} byte count overflowed"),
        )
    })?;
    if next_len > max_bytes {
        return Err(provider_response_limit_error(
            provider_id,
            "response body",
            &format!("{field_name} exceeded {max_bytes} bytes"),
        ));
    }
    target.push_str(delta);
    Ok(())
}

fn append_provider_stream_text(
    target: &mut String,
    delta: &str,
    max_bytes: usize,
    provider_id: &str,
    field_name: &str,
) -> CommandResult<()> {
    let next_len = target.len().checked_add(delta.len()).ok_or_else(|| {
        provider_stream_limit_error(
            provider_id,
            "response stream",
            &format!("{field_name} byte count overflowed"),
        )
    })?;
    if next_len > max_bytes {
        return Err(provider_stream_limit_error(
            provider_id,
            "response stream",
            &format!("{field_name} exceeded {max_bytes} bytes"),
        ));
    }
    target.push_str(delta);
    Ok(())
}

fn read_provider_text_bounded<R: Read>(
    reader: &mut R,
    max_bytes: usize,
    provider_id: &str,
    body_name: &str,
) -> CommandResult<String> {
    let bytes = read_provider_bytes_bounded(reader, max_bytes, provider_id, body_name)?;
    String::from_utf8(bytes).map_err(|error| {
        CommandError::retryable(
            "agent_provider_response_decode_failed",
            format!("Xero could not decode the {provider_id} {body_name} as UTF-8: {error}"),
        )
    })
}

fn read_provider_bytes_bounded<R: Read>(
    reader: &mut R,
    max_bytes: usize,
    provider_id: &str,
    body_name: &str,
) -> CommandResult<Vec<u8>> {
    let read_limit = u64::try_from(max_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::with_capacity(max_bytes.min(8 * 1_024));
    reader
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            CommandError::retryable(
                "agent_provider_response_read_failed",
                format!("Xero could not read the {provider_id} {body_name}: {error}"),
            )
        })?;
    if bytes.len() > max_bytes {
        return Err(provider_response_limit_error(
            provider_id,
            body_name,
            &format!("size exceeded {max_bytes} bytes"),
        ));
    }
    Ok(bytes)
}

fn read_provider_error_text_bounded<R: Read>(
    reader: &mut R,
    max_bytes: usize,
    provider_id: &str,
    body_name: &str,
) -> String {
    match read_provider_bytes_bounded(reader, max_bytes, provider_id, body_name) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) if error.code == "agent_provider_response_limit_exceeded" => {
            "provider error body exceeded the safe read limit and was omitted.".into()
        }
        Err(_) => "provider error body could not be read.".into(),
    }
}

fn ensure_provider_stream_item_capacity(
    current_items: usize,
    additional_items: usize,
    max_items: usize,
    provider_id: &str,
    item_name: &str,
) -> CommandResult<()> {
    let next_items = current_items.checked_add(additional_items).ok_or_else(|| {
        provider_stream_limit_error(
            provider_id,
            "response stream",
            &format!("{item_name} count overflowed"),
        )
    })?;
    if next_items > max_items {
        return Err(provider_stream_limit_error(
            provider_id,
            "response stream",
            &format!("{item_name} count exceeded {max_items}"),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProviderConfig {
    Fake,
    OpenAiResponses(OpenAiResponsesProviderConfig),
    OpenAiCodexResponses(OpenAiCodexResponsesProviderConfig),
    XaiResponses(XaiResponsesProviderConfig),
    OpenAiCompatible(OpenAiCompatibleProviderConfig),
    DeepSeek(DeepSeekProviderConfig),
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
pub struct XaiResponsesProviderConfig {
    pub provider_id: String,
    pub model_id: String,
    pub base_url: String,
    pub bearer_token: String,
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
pub struct DeepSeekProviderConfig {
    pub model_id: String,
    pub base_url: String,
    pub api_key: String,
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

#[derive(Debug, Clone)]
pub(crate) struct ProviderTurnBudgetResolution {
    pub context_limit: SessionContextLimitResolutionDto,
    pub output_allowance: ProviderTurnOutputAllowance,
}

pub(crate) fn resolve_provider_turn_budget(
    provider_id: &str,
    model_id: &str,
    provider_preflight: Option<&xero_agent_core::ProviderPreflightSnapshot>,
    thinking_effort: Option<&ProviderModelThinkingEffortDto>,
) -> CommandResult<ProviderTurnBudgetResolution> {
    // The fake adapter intentionally uses its provider id as a synthetic model id in tests and
    // local development. It has no wire request or external capability catalog.
    let synthetic_fake_model =
        provider_id == OPENAI_CODEX_PROVIDER_ID && model_id == OPENAI_CODEX_PROVIDER_ID;
    if let Some(snapshot) = provider_preflight {
        if !provider_id
            .trim()
            .eq_ignore_ascii_case(snapshot.provider_id.trim())
            || !model_id
                .trim()
                .eq_ignore_ascii_case(snapshot.model_id.trim())
        {
            return Err(CommandError::user_fixable(
                "agent_provider_output_limit_capability_mismatch",
                format!(
                    "Xero refused to submit `{provider_id}/{model_id}` because the admitted provider capabilities belong to `{}/{}`. Refresh provider diagnostics and retry the run.",
                    snapshot.provider_id, snapshot.model_id
                ),
            ));
        }
        if !synthetic_fake_model {
            let advertised = &snapshot.capabilities.capabilities.context_limits;
            let context_window_tokens = advertised.context_window_tokens.ok_or_else(|| {
                CommandError::user_fixable(
                    "agent_provider_context_limit_missing",
                    format!(
                        "Xero refused to submit `{provider_id}/{model_id}` because its admitted capabilities do not advertise a context-window limit. Refresh the provider model catalog or choose a model with known limits."
                    ),
                )
            })?;
            let max_output_tokens = advertised.max_output_tokens.ok_or_else(|| {
                CommandError::user_fixable(
                    "agent_provider_output_limit_missing",
                    format!(
                        "Xero refused to submit `{provider_id}/{model_id}` because its admitted capabilities do not advertise a maximum output limit. Refresh the provider model catalog or choose a model with known limits."
                    ),
                )
            })?;
            validate_advertised_provider_limits(
                provider_id,
                model_id,
                context_window_tokens,
                max_output_tokens,
            )?;
        }
    }

    let context_limit =
        resolve_context_limit_with_provider_preflight(provider_id, model_id, provider_preflight);
    let context_window_tokens = context_limit
        .context_window_tokens
        .filter(|tokens| *tokens > 0)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_provider_context_limit_missing",
                format!(
                    "Xero refused to submit `{provider_id}/{model_id}` because no positive context-window limit could be resolved. Refresh provider diagnostics or choose a model with known limits."
                ),
            )
        });
    let max_output_tokens = context_limit
        .max_output_tokens
        .filter(|tokens| *tokens > 0)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_provider_output_limit_missing",
                format!(
                    "Xero refused to submit `{provider_id}/{model_id}` because no positive maximum output limit could be resolved. Refresh provider diagnostics or choose a model with known limits."
                ),
            )
        });

    let (context_window_tokens, max_output_tokens) = if synthetic_fake_model {
        (
            context_limit.context_window_tokens.unwrap_or(128_000),
            context_limit.max_output_tokens.unwrap_or(4_096),
        )
    } else {
        if matches!(
            context_limit.source,
            SessionContextLimitSourceDto::Unknown | SessionContextLimitSourceDto::Heuristic
        ) {
            return Err(CommandError::user_fixable(
                "agent_provider_output_limit_unverified",
                format!(
                    "Xero refused to submit `{provider_id}/{model_id}` because its output limit is not backed by provider capabilities or the built-in model registry. Refresh provider diagnostics or choose a model with verified limits."
                ),
            ));
        }
        (context_window_tokens?, max_output_tokens?)
    };
    validate_advertised_provider_limits(
        provider_id,
        model_id,
        context_window_tokens,
        max_output_tokens,
    )?;
    if !synthetic_fake_model && context_limit.output_reserve_tokens != max_output_tokens {
        return Err(CommandError::user_fixable(
            "agent_provider_output_limit_inconsistent",
            format!(
                "Xero refused to submit `{provider_id}/{model_id}` because context budgeting reserved {} output tokens while the resolved wire allowance is {max_output_tokens}. Refresh provider diagnostics and retry the run.",
                context_limit.output_reserve_tokens
            ),
        ));
    }

    let output_allowance = if matches!(
        provider_id,
        ANTHROPIC_PROVIDER_ID | BEDROCK_PROVIDER_ID | VERTEX_PROVIDER_ID
    ) {
        let reasoning_tokens = thinking_effort
            .and_then(anthropic_thinking_budget_tokens)
            .unwrap_or_default();
        if reasoning_tokens >= max_output_tokens {
            return Err(CommandError::user_fixable(
                "agent_provider_reasoning_output_limit_exceeded",
                format!(
                    "Xero refused to submit `{provider_id}/{model_id}` because the selected thinking effort requires {reasoning_tokens} tokens, leaving no visible output within the advertised {max_output_tokens}-token maximum. Choose a lower thinking effort or a model with a larger output limit."
                ),
            ));
        }
        ProviderTurnOutputAllowance::split(
            max_output_tokens,
            reasoning_tokens,
            max_output_tokens - reasoning_tokens,
        )?
    } else {
        ProviderTurnOutputAllowance::unified(max_output_tokens)?
    };

    Ok(ProviderTurnBudgetResolution {
        context_limit,
        output_allowance,
    })
}

fn validate_advertised_provider_limits(
    provider_id: &str,
    model_id: &str,
    context_window_tokens: u64,
    max_output_tokens: u64,
) -> CommandResult<()> {
    if context_window_tokens == 0 || max_output_tokens == 0 {
        return Err(CommandError::user_fixable(
            "agent_provider_output_limit_invalid",
            format!(
                "Xero refused to submit `{provider_id}/{model_id}` because its advertised context/output limits must both be greater than zero (context: {context_window_tokens}, output: {max_output_tokens}). Refresh the provider model catalog or choose a different model."
            ),
        ));
    }
    if max_output_tokens > context_window_tokens {
        return Err(CommandError::user_fixable(
            "agent_provider_output_limit_inconsistent",
            format!(
                "Xero refused to submit `{provider_id}/{model_id}` because its advertised output limit ({max_output_tokens}) exceeds its context window ({context_window_tokens}). Refresh the provider model catalog or choose a different model."
            ),
        ));
    }
    Ok(())
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
        AgentProviderConfig::XaiResponses(config) => {
            XaiResponsesAdapter::new(config).map(|adapter| Box::new(adapter) as _)
        }
        AgentProviderConfig::OpenAiCompatible(config) => {
            OpenAiCompatibleAdapter::new(config).map(|adapter| Box::new(adapter) as _)
        }
        AgentProviderConfig::DeepSeek(config) => {
            OpenAiCompatibleAdapter::new(OpenAiCompatibleProviderConfig {
                provider_id: DEEPSEEK_PROVIDER_ID.into(),
                model_id: config.model_id,
                base_url: config.base_url,
                api_key: Some(config.api_key),
                api_version: None,
                timeout_ms: config.timeout_ms,
            })
            .map(|adapter| Box::new(adapter) as _)
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
    client: AsyncClient,
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
        cancellation: &AgentRunCancellationToken,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        let provider_id = self.provider_id().to_owned();
        let url = openai_codex_responses_url(&self.config.base_url)?;
        let headers = openai_codex_request_headers(&self.config)?;
        let body = openai_codex_responses_request_body(
            &provider_id,
            &self.config.model_id,
            request,
            self.config.session_id.as_deref(),
        )?;
        let client = self.client.clone();
        let response = start_provider_json_stream(
            &provider_id,
            Duration::from_millis(normalize_timeout(self.config.timeout_ms)),
            cancellation.clone(),
            move || {
                client
                    .post(url.clone())
                    .headers(headers.clone())
                    .json(&body)
            },
        )?;
        parse_openai_responses_sse(&provider_id, response, emit)
    }

    fn estimate_context_tokens(
        &self,
        request: &ProviderTurnRequest,
    ) -> CommandResult<SessionContextEstimateDto> {
        let body = openai_codex_responses_request_body(
            self.provider_id(),
            &self.config.model_id,
            request,
            self.config.session_id.as_deref(),
        )?;
        estimate_provider_wire_context_tokens(
            self.provider_id(),
            &self.config.model_id,
            "openai_codex_responses_wire_request",
            body,
        )
    }
}

#[derive(Debug)]
struct OpenAiResponsesAdapter {
    config: OpenAiResponsesProviderConfig,
    client: AsyncClient,
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
        cancellation: &AgentRunCancellationToken,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        let provider_id = self.provider_id().to_owned();
        let url = responses_url(&self.config.base_url)?;
        let body = openai_responses_request_body(&provider_id, &self.config.model_id, request)?;
        let client = self.client.clone();
        let api_key = self.config.api_key.clone();
        let response = start_provider_json_stream(
            &provider_id,
            Duration::from_millis(normalize_timeout(self.config.timeout_ms)),
            cancellation.clone(),
            move || client.post(url.clone()).bearer_auth(&api_key).json(&body),
        )?;
        parse_openai_responses_sse(&provider_id, response, emit)
    }

    fn estimate_context_tokens(
        &self,
        request: &ProviderTurnRequest,
    ) -> CommandResult<SessionContextEstimateDto> {
        let body =
            openai_responses_request_body(self.provider_id(), &self.config.model_id, request)?;
        estimate_provider_wire_context_tokens(
            self.provider_id(),
            &self.config.model_id,
            "openai_responses_wire_request",
            body,
        )
    }
}

#[derive(Debug)]
struct XaiResponsesAdapter {
    config: XaiResponsesProviderConfig,
    client: AsyncClient,
}

impl XaiResponsesAdapter {
    fn new(mut config: XaiResponsesProviderConfig) -> CommandResult<Self> {
        normalize_required(&mut config.provider_id, "providerId")?;
        normalize_required(&mut config.model_id, "modelId")?;
        normalize_required(&mut config.base_url, "baseUrl")?;
        normalize_required(&mut config.bearer_token, "bearerToken")?;
        if !is_supported_xai_text_model_id(&config.model_id) {
            return Err(CommandError::user_fixable(
                "xai_model_not_supported_by_text_runtime",
                format!(
                    "Xero's xAI owned runtime currently supports Grok 4.3 and Grok Build text models only; `{}` is not available for agent turns.",
                    config.model_id
                ),
            ));
        }
        let client = provider_http_client(config.timeout_ms)?;
        Ok(Self { config, client })
    }
}

impl ProviderAdapter for XaiResponsesAdapter {
    fn provider_id(&self) -> &str {
        self.config.provider_id.as_str()
    }

    fn model_id(&self) -> &str {
        self.config.model_id.as_str()
    }

    fn stream_turn(
        &self,
        request: &ProviderTurnRequest,
        cancellation: &AgentRunCancellationToken,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        let provider_id = self.provider_id().to_owned();
        let url = responses_url(&self.config.base_url)?;
        let body = xai_responses_request_body(&provider_id, &self.config.model_id, request)?;
        let client = self.client.clone();
        let bearer_token = self.config.bearer_token.clone();
        let response = start_provider_json_stream(
            &provider_id,
            Duration::from_millis(normalize_timeout(self.config.timeout_ms)),
            cancellation.clone(),
            move || {
                client
                    .post(url.clone())
                    .bearer_auth(&bearer_token)
                    .json(&body)
            },
        )?;
        parse_openai_responses_sse(&provider_id, response, emit)
    }

    fn estimate_context_tokens(
        &self,
        request: &ProviderTurnRequest,
    ) -> CommandResult<SessionContextEstimateDto> {
        let body = xai_responses_request_body(self.provider_id(), &self.config.model_id, request)?;
        estimate_provider_wire_context_tokens(
            self.provider_id(),
            &self.config.model_id,
            "xai_responses_wire_request",
            body,
        )
    }
}

#[derive(Debug)]
struct OpenAiCompatibleAdapter {
    config: OpenAiCompatibleProviderConfig,
    client: AsyncClient,
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
        cancellation: &AgentRunCancellationToken,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        let provider_id = self.provider_id().to_owned();
        let url =
            openai_compatible_chat_url(&self.config.base_url, self.config.api_version.as_deref())?;
        let body = openai_chat_request_body(&provider_id, &self.config.model_id, request)?;
        let client = self.client.clone();
        let api_key = self.config.api_key.clone();
        let closure_provider_id = provider_id.clone();
        let response = start_provider_json_stream(
            &provider_id,
            Duration::from_millis(normalize_timeout(self.config.timeout_ms)),
            cancellation.clone(),
            move || {
                let mut http_request = client
                    .post(url.clone())
                    .header("Content-Type", "application/json")
                    .header("X-Title", "Xero");
                http_request =
                    apply_openai_compatible_provider_headers(&closure_provider_id, http_request);
                if let Some(api_key) = api_key.as_deref() {
                    http_request = apply_openai_compatible_auth_header(
                        &closure_provider_id,
                        http_request,
                        api_key,
                    );
                }
                http_request.json(&body)
            },
        )?;
        parse_openai_chat_sse(&provider_id, response, emit)
    }

    fn estimate_context_tokens(
        &self,
        request: &ProviderTurnRequest,
    ) -> CommandResult<SessionContextEstimateDto> {
        let body = openai_chat_request_body(self.provider_id(), &self.config.model_id, request)?;
        estimate_provider_wire_context_tokens(
            self.provider_id(),
            &self.config.model_id,
            "openai_chat_completions_wire_request",
            body,
        )
    }
}

#[derive(Debug)]
struct AnthropicAdapter {
    config: AnthropicProviderConfig,
    client: AsyncClient,
    context_client: BlockingClient,
}

impl AnthropicAdapter {
    fn new(mut config: AnthropicProviderConfig) -> CommandResult<Self> {
        normalize_required(&mut config.provider_id, "providerId")?;
        normalize_required(&mut config.model_id, "modelId")?;
        normalize_required(&mut config.api_key, "apiKey")?;
        normalize_required(&mut config.base_url, "baseUrl")?;
        normalize_required(&mut config.anthropic_version, "anthropicVersion")?;
        let client = provider_http_client(config.timeout_ms)?;
        let context_client = provider_blocking_http_client(config.timeout_ms)?;
        Ok(Self {
            config,
            client,
            context_client,
        })
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
        cancellation: &AgentRunCancellationToken,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        let provider_id = self.provider_id().to_owned();
        let url = anthropic_messages_url(&self.config.base_url)?;
        let body = anthropic_request_body(
            Some(&self.config.model_id),
            &self.config.anthropic_version,
            request,
            true,
        )?;
        let client = self.client.clone();
        let api_key = self.config.api_key.clone();
        let anthropic_version = self.config.anthropic_version.clone();
        let response = start_provider_json_stream(
            &provider_id,
            Duration::from_millis(normalize_timeout(self.config.timeout_ms)),
            cancellation.clone(),
            move || {
                client
                    .post(url.clone())
                    .header("x-api-key", &api_key)
                    .header("anthropic-version", &anthropic_version)
                    .json(&body)
            },
        )?;
        parse_anthropic_sse(&provider_id, response, emit)
    }

    fn estimate_context_tokens(
        &self,
        request: &ProviderTurnRequest,
    ) -> CommandResult<SessionContextEstimateDto> {
        estimate_anthropic_context_tokens(&self.config, &self.context_client, request)
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
        cancellation: &AgentRunCancellationToken,
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
        let stderr_output = NamedTempFile::new().map_err(|error| {
            CommandError::retryable(
                "bedrock_tempfile_failed",
                format!("Xero could not allocate a Bedrock stderr file: {error}"),
            )
        })?;
        let stderr_writer = stderr_output.reopen().map_err(|error| {
            CommandError::retryable(
                "bedrock_tempfile_failed",
                format!("Xero could not open the Bedrock stderr file: {error}"),
            )
        })?;
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
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr_writer));
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
        if let Err(error) = register_process_tree_root(&child) {
            let _ = terminate_process_tree(&mut child);
            return Err(CommandError::retryable(
                "bedrock_invoke_process_tree_registration_failed",
                format!("Xero could not establish ownership of the AWS CLI process tree: {error}"),
            ));
        }
        let status = wait_provider_cli(
            &mut child,
            Duration::from_millis(normalize_timeout(self.config.timeout_ms)),
            BEDROCK_PROVIDER_ID,
            cancellation,
        )?;
        if !status.success() {
            drop(child);
            let mut stderr_reader = stderr_output.reopen().map_err(|error| {
                CommandError::retryable(
                    "bedrock_response_read_failed",
                    format!("Xero could not open the Bedrock stderr body: {error}"),
                )
            })?;
            let stderr = read_provider_error_text_bounded(
                &mut stderr_reader,
                MAX_PROVIDER_ERROR_BODY_BYTES,
                BEDROCK_PROVIDER_ID,
                "stderr body",
            );
            return Err(provider_status_error(
                BEDROCK_PROVIDER_ID,
                status.code().unwrap_or(-1),
                &stderr,
            ));
        }

        let mut response_file = std::fs::File::open(&output_path).map_err(|error| {
            CommandError::retryable(
                "bedrock_response_read_failed",
                format!("Xero could not open the Bedrock response body: {error}"),
            )
        })?;
        let response_text = read_provider_text_bounded(
            &mut response_file,
            MAX_PROVIDER_NON_STREAM_BODY_BYTES,
            BEDROCK_PROVIDER_ID,
            "response body",
        )?;
        parse_anthropic_json_response(BEDROCK_PROVIDER_ID, &response_text, emit)
    }

    fn estimate_context_tokens(
        &self,
        request: &ProviderTurnRequest,
    ) -> CommandResult<SessionContextEstimateDto> {
        let body = anthropic_request_body(None, BEDROCK_ANTHROPIC_VERSION, request, false)?;
        estimate_provider_wire_context_tokens(
            BEDROCK_PROVIDER_ID,
            &self.config.model_id,
            "bedrock_anthropic_wire_request",
            body,
        )
    }
}

#[derive(Debug)]
struct VertexAnthropicAdapter {
    config: VertexProviderConfig,
    client: AsyncClient,
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
        cancellation: &AgentRunCancellationToken,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        let token = vertex_access_token()?;
        let body = anthropic_request_body(None, VERTEX_ANTHROPIC_VERSION, request, false)?;
        let url = vertex_anthropic_raw_predict_url(&self.config)?;
        let client = self.client.clone();
        let response = start_provider_json_stream(
            VERTEX_PROVIDER_ID,
            Duration::from_millis(normalize_timeout(self.config.timeout_ms)),
            cancellation.clone(),
            move || {
                client
                    .post(url.clone())
                    .bearer_auth(token.clone())
                    .json(&body)
            },
        )?;
        let mut response = response;
        let text = read_provider_text_bounded(
            &mut response,
            MAX_PROVIDER_NON_STREAM_BODY_BYTES,
            VERTEX_PROVIDER_ID,
            "response body",
        )?;
        parse_anthropic_json_response(VERTEX_PROVIDER_ID, &text, emit)
    }

    fn estimate_context_tokens(
        &self,
        request: &ProviderTurnRequest,
    ) -> CommandResult<SessionContextEstimateDto> {
        let body = anthropic_request_body(None, VERTEX_ANTHROPIC_VERSION, request, false)?;
        estimate_provider_wire_context_tokens(
            VERTEX_PROVIDER_ID,
            &self.config.model_id,
            "vertex_anthropic_wire_request",
            body,
        )
    }
}

fn provider_http_client(timeout_ms: u64) -> CommandResult<AsyncClient> {
    let timeout = Duration::from_millis(normalize_timeout(timeout_ms));
    AsyncClient::builder()
        .connect_timeout(timeout)
        .tcp_keepalive(Duration::from_secs(30))
        .build()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_provider_http_client_unavailable",
                format!("Xero could not build an HTTP client for the provider adapter: {error}"),
            )
        })
}

fn provider_blocking_http_client(timeout_ms: u64) -> CommandResult<BlockingClient> {
    let timeout = Duration::from_millis(normalize_timeout(timeout_ms));
    BlockingClient::builder()
        .connect_timeout(timeout)
        .timeout(timeout)
        .tcp_keepalive(Duration::from_secs(30))
        .build()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_provider_http_client_unavailable",
                format!(
                    "Xero could not build a blocking HTTP client for provider metadata: {error}"
                ),
            )
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderStreamAbort {
    Stopped,
    Cancelled,
}

fn start_provider_json_stream<F>(
    provider_id: &str,
    idle_timeout: Duration,
    cancellation: AgentRunCancellationToken,
    build: F,
) -> CommandResult<ProviderStreamReader>
where
    F: FnMut() -> AsyncRequestBuilder + Send + 'static,
{
    cancellation.check_cancelled()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_provider_stream_runtime_unavailable",
                format!("Xero could not start the provider stream runtime: {error}"),
            )
        })?;
    let (sender, receiver) = mpsc::sync_channel(PROVIDER_STREAM_CHANNEL_CAPACITY);
    let stop = Arc::new(AtomicBool::new(false));
    let transport_stop = stop.clone();
    let provider_id = provider_id.to_owned();
    let transport = std::thread::Builder::new()
        .name(format!("provider-stream-{provider_id}"))
        .spawn(move || {
            let outcome = runtime.block_on(run_provider_stream_transport(
                &provider_id,
                idle_timeout,
                &cancellation,
                transport_stop.as_ref(),
                build,
                &sender,
            ));
            if transport_stop.load(Ordering::SeqCst) {
                return;
            }
            let terminal = match outcome {
                Ok(()) => ProviderStreamTransportMessage::Complete,
                Err(error) => ProviderStreamTransportMessage::Error(error),
            };
            send_provider_stream_terminal(&sender, terminal, transport_stop.as_ref());
        })
        .map_err(|error| {
            CommandError::system_fault(
                "agent_provider_stream_thread_unavailable",
                format!("Xero could not start the provider stream transport: {error}"),
            )
        })?;

    Ok(ProviderStreamReader {
        receiver,
        current_chunk: Vec::new(),
        current_offset: 0,
        stop,
        transport: Some(transport),
    })
}

async fn run_provider_stream_transport<F>(
    provider_id: &str,
    idle_timeout: Duration,
    cancellation: &AgentRunCancellationToken,
    stop: &AtomicBool,
    build: F,
    sender: &SyncSender<ProviderStreamTransportMessage>,
) -> CommandResult<()>
where
    F: FnMut() -> AsyncRequestBuilder,
{
    let Some(mut response) =
        send_provider_json_request_async(provider_id, idle_timeout, cancellation, stop, build)
            .await?
    else {
        return Ok(());
    };

    loop {
        let chunk = tokio::select! {
            abort = wait_for_provider_stream_abort(stop, cancellation) => {
                return match abort {
                    ProviderStreamAbort::Stopped => Ok(()),
                    ProviderStreamAbort::Cancelled => Err(cancelled_error()),
                };
            }
            _ = tokio::time::sleep(idle_timeout) => {
                return Err(provider_stream_idle_timeout_error(provider_id, idle_timeout));
            }
            chunk = response.chunk() => chunk.map_err(|error| {
                map_provider_transport_error(provider_id, error)
            })?,
        };
        let Some(chunk) = chunk else {
            return Ok(());
        };
        for bounded_chunk in chunk.chunks(PROVIDER_STREAM_CHANNEL_CHUNK_BYTES) {
            if !send_provider_stream_chunk(sender, bounded_chunk.to_vec(), stop, cancellation)
                .await?
            {
                return Ok(());
            }
        }
    }
}

async fn send_provider_json_request_async<F>(
    provider_id: &str,
    idle_timeout: Duration,
    cancellation: &AgentRunCancellationToken,
    stop: &AtomicBool,
    mut build: F,
) -> CommandResult<Option<AsyncResponse>>
where
    F: FnMut() -> AsyncRequestBuilder,
{
    let mut last_transport_error = None;
    for attempt in 0..MAX_PROVIDER_ATTEMPTS {
        let response = tokio::select! {
            abort = wait_for_provider_stream_abort(stop, cancellation) => {
                return match abort {
                    ProviderStreamAbort::Stopped => Ok(None),
                    ProviderStreamAbort::Cancelled => Err(cancelled_error()),
                };
            }
            _ = tokio::time::sleep(idle_timeout) => {
                return Err(provider_stream_idle_timeout_error(provider_id, idle_timeout));
            }
            response = build().send() => response,
        };
        match response {
            Ok(mut response) => {
                let status = response.status().as_u16();
                if response.status().is_success() {
                    return Ok(Some(response));
                }
                if is_retryable_provider_status(status) && attempt + 1 < MAX_PROVIDER_ATTEMPTS {
                    let delay = retry_after_delay(response.headers())
                        .unwrap_or_else(|| retry_backoff(attempt));
                    drop(response);
                    if !wait_provider_stream_delay(delay, stop, cancellation).await? {
                        return Ok(None);
                    }
                    continue;
                }
                let body = read_provider_error_body_async(
                    provider_id,
                    &mut response,
                    idle_timeout,
                    cancellation,
                    stop,
                )
                .await?;
                return Err(provider_http_status_error(provider_id, status, &body));
            }
            Err(error) => {
                if attempt + 1 < MAX_PROVIDER_ATTEMPTS {
                    last_transport_error = Some(error.to_string());
                    if !wait_provider_stream_delay(retry_backoff(attempt), stop, cancellation)
                        .await?
                    {
                        return Ok(None);
                    }
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

async fn read_provider_error_body_async(
    provider_id: &str,
    response: &mut AsyncResponse,
    idle_timeout: Duration,
    cancellation: &AgentRunCancellationToken,
    stop: &AtomicBool,
) -> CommandResult<String> {
    let mut body = Vec::new();
    loop {
        let chunk = tokio::select! {
            abort = wait_for_provider_stream_abort(stop, cancellation) => {
                return match abort {
                    ProviderStreamAbort::Stopped => Ok(String::new()),
                    ProviderStreamAbort::Cancelled => Err(cancelled_error()),
                };
            }
            _ = tokio::time::sleep(idle_timeout) => {
                return Err(provider_stream_idle_timeout_error(provider_id, idle_timeout));
            }
            chunk = response.chunk() => chunk.map_err(|error| {
                map_provider_transport_error(provider_id, error)
            })?,
        };
        let Some(chunk) = chunk else {
            break;
        };
        if body.len().saturating_add(chunk.len()) > MAX_PROVIDER_ERROR_BODY_BYTES {
            return Ok("provider error body exceeded the safe read limit and was omitted.".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

async fn wait_for_provider_stream_abort(
    stop: &AtomicBool,
    cancellation: &AgentRunCancellationToken,
) -> ProviderStreamAbort {
    loop {
        if stop.load(Ordering::SeqCst) {
            return ProviderStreamAbort::Stopped;
        }
        if cancellation.is_cancelled() {
            return ProviderStreamAbort::Cancelled;
        }
        tokio::time::sleep(PROVIDER_STREAM_ABORT_POLL_INTERVAL).await;
    }
}

async fn wait_provider_stream_delay(
    delay: Duration,
    stop: &AtomicBool,
    cancellation: &AgentRunCancellationToken,
) -> CommandResult<bool> {
    tokio::select! {
        abort = wait_for_provider_stream_abort(stop, cancellation) => match abort {
            ProviderStreamAbort::Stopped => Ok(false),
            ProviderStreamAbort::Cancelled => Err(cancelled_error()),
        },
        _ = tokio::time::sleep(delay) => Ok(true),
    }
}

async fn send_provider_stream_chunk(
    sender: &SyncSender<ProviderStreamTransportMessage>,
    mut chunk: Vec<u8>,
    stop: &AtomicBool,
    cancellation: &AgentRunCancellationToken,
) -> CommandResult<bool> {
    loop {
        match sender.try_send(ProviderStreamTransportMessage::Chunk(chunk)) {
            Ok(()) => return Ok(true),
            Err(TrySendError::Disconnected(_)) => return Ok(false),
            Err(TrySendError::Full(ProviderStreamTransportMessage::Chunk(pending))) => {
                chunk = pending;
            }
            Err(TrySendError::Full(_)) => {
                return Err(CommandError::system_fault(
                    "agent_provider_stream_channel_invalid",
                    "Xero's provider stream channel received an invalid message while applying backpressure.",
                ));
            }
        }
        if stop.load(Ordering::SeqCst) {
            return Ok(false);
        }
        cancellation.check_cancelled()?;
        tokio::time::sleep(PROVIDER_STREAM_BACKPRESSURE_POLL_INTERVAL).await;
    }
}

fn send_provider_stream_terminal(
    sender: &SyncSender<ProviderStreamTransportMessage>,
    mut terminal: ProviderStreamTransportMessage,
    stop: &AtomicBool,
) {
    loop {
        match sender.try_send(terminal) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => return,
            Err(TrySendError::Full(pending)) => terminal = pending,
        }
        if stop.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(PROVIDER_STREAM_BACKPRESSURE_POLL_INTERVAL);
    }
}

fn provider_stream_idle_timeout_error(provider_id: &str, idle_timeout: Duration) -> CommandError {
    CommandError::retryable(
        "agent_provider_stream_idle_timeout",
        format!(
            "Xero stopped provider `{provider_id}` because its response stream was idle for {} milliseconds.",
            idle_timeout.as_millis()
        ),
    )
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

fn trusted_system_text(request: &ProviderTurnRequest) -> String {
    let mut trusted = request.system_prompt.clone();
    for content in request.messages.iter().filter_map(|message| match message {
        ProviderMessage::Developer { content } => Some(content.trim()),
        _ => None,
    }) {
        if content.is_empty() {
            continue;
        }
        if !trusted.is_empty() {
            trusted.push_str("\n\n");
        }
        trusted.push_str(content);
    }
    trusted
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

fn anthropic_count_tokens_url(base_url: &str) -> CommandResult<Url> {
    provider_url(base_url, "v1/messages/count_tokens", None)
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
    let max_output_tokens = request.output_allowance.wire_max_output_tokens()?;
    let mut body = JsonMap::new();
    body.insert("model".into(), json!(model_id));
    body.insert(
        "messages".into(),
        JsonValue::Array(openai_chat_messages(provider_id, request)?),
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
    let output_limit_field = if openai_chat_uses_max_completion_tokens(provider_id, model_id) {
        "max_completion_tokens"
    } else {
        "max_tokens"
    };
    body.insert(output_limit_field.into(), json!(max_output_tokens));
    if provider_id == DEEPSEEK_PROVIDER_ID {
        body.insert("thinking".into(), json!({ "type": "enabled" }));
        if let Some(effort) = request.controls.active.thinking_effort.as_ref() {
            body.insert(
                "reasoning_effort".into(),
                json!(deepseek_thinking_effort_value(effort)),
            );
        }
    } else if provider_id == OPENROUTER_PROVIDER_ID {
        if let Some(effort) = request.controls.active.thinking_effort.as_ref() {
            body.insert(
                "reasoning".into(),
                json!({ "effort": openrouter_reasoning_effort_value(effort) }),
            );
        }
    } else if matches!(
        provider_id,
        OPENAI_API_PROVIDER_ID
            | AZURE_OPENAI_PROVIDER_ID
            | GITHUB_MODELS_PROVIDER_ID
            | GEMINI_AI_STUDIO_PROVIDER_ID
    ) {
        if let Some(effort) = request.controls.active.thinking_effort.as_ref() {
            if let Some(value) = openai_chat_reasoning_effort_value(effort) {
                body.insert("reasoning_effort".into(), json!(value));
            }
        }
    }
    Ok(JsonValue::Object(body))
}

fn openai_chat_uses_max_completion_tokens(provider_id: &str, model_id: &str) -> bool {
    if provider_id == OPENAI_API_PROVIDER_ID {
        return true;
    }
    if provider_id != AZURE_OPENAI_PROVIDER_ID {
        return false;
    }

    let model_id = model_id.trim().to_ascii_lowercase();
    ["gpt-5", "o1", "o3", "o4"]
        .iter()
        .any(|reasoning_model| model_id.contains(reasoning_model))
}

/// Chat-completions `reasoning_effort` for OpenAI-compatible hosts. Clamped to
/// the `low`/`medium`/`high` values every reasoning-capable model on these
/// hosts accepts; `minimal`/`xhigh` and `none` exist only on specific model
/// generations and would hard-fail the request elsewhere. `None` (thinking
/// off) omits the parameter so non-reasoning models keep working.
fn openai_chat_reasoning_effort_value(
    effort: &ProviderModelThinkingEffortDto,
) -> Option<&'static str> {
    match effort {
        ProviderModelThinkingEffortDto::None => None,
        ProviderModelThinkingEffortDto::Minimal | ProviderModelThinkingEffortDto::Low => {
            Some("low")
        }
        ProviderModelThinkingEffortDto::Medium => Some("medium"),
        ProviderModelThinkingEffortDto::High | ProviderModelThinkingEffortDto::XHigh => {
            Some("high")
        }
    }
}

fn provider_supports_openai_stream_options(provider_id: &str) -> bool {
    matches!(
        provider_id,
        OPENAI_API_PROVIDER_ID
            | OPENAI_CODEX_PROVIDER_ID
            | OPENROUTER_PROVIDER_ID
            | DEEPSEEK_PROVIDER_ID
            | GITHUB_MODELS_PROVIDER_ID
            | AZURE_OPENAI_PROVIDER_ID
    )
}

fn estimate_provider_wire_context_tokens(
    provider_id: &str,
    model_id: &str,
    counted_shape: &'static str,
    body: JsonValue,
) -> CommandResult<SessionContextEstimateDto> {
    let mut sanitization = ProviderWireEstimateSanitization::default();
    let sanitized_body = sanitize_provider_wire_request_for_estimate(body, &mut sanitization);
    let serialized = serde_json::to_string(&sanitized_body).map_err(|error| {
        CommandError::system_fault(
            "agent_context_provider_wire_estimate_serialize_failed",
            format!(
                "Xero could not serialize the `{counted_shape}` provider request body for `{provider_id}/{model_id}`: {error}"
            ),
        )
    })?;
    let mut estimate = heuristic_token_estimate(&serialized, counted_shape);
    estimate.tokens = estimate
        .tokens
        .saturating_add(sanitization.image_data_url_estimated_tokens);
    estimate.diagnostics = vec![format!(
        "Estimated tokens from the provider-specific wire request body for `{provider_id}/{model_id}`; tokenizer fallback remains conservative."
    )];
    if sanitization.image_data_url_count > 0 {
        estimate.diagnostics.push(format!(
            "Omitted {} inline image data URL payload(s), {} encoded byte(s), from text-token estimation and added {} estimated image token(s) instead.",
            sanitization.image_data_url_count,
            sanitization.image_data_url_encoded_bytes,
            sanitization.image_data_url_estimated_tokens
        ));
    }
    Ok(estimate)
}

#[derive(Debug, Default, Clone, Copy)]
struct ProviderWireEstimateSanitization {
    image_data_url_count: u64,
    image_data_url_encoded_bytes: u64,
    image_data_url_estimated_tokens: u64,
}

fn sanitize_provider_wire_request_for_estimate(
    value: JsonValue,
    sanitization: &mut ProviderWireEstimateSanitization,
) -> JsonValue {
    match value {
        JsonValue::Array(items) => JsonValue::Array(
            items
                .into_iter()
                .map(|item| sanitize_provider_wire_request_for_estimate(item, sanitization))
                .collect(),
        ),
        JsonValue::Object(object) => {
            // Anthropic-family attachment blocks carry raw base64 (no `data:` prefix) in
            // `source.data` alongside `type: "base64"`. Omit that payload from the estimate so
            // a multi-MB image/PDF is not counted as text.
            let is_base64_source = object.get("type").and_then(JsonValue::as_str) == Some("base64")
                && object.get("data").and_then(JsonValue::as_str).is_some();
            JsonValue::Object(
                object
                    .into_iter()
                    .map(|(key, value)| {
                        if is_base64_source && key == "data" {
                            if let JsonValue::String(payload) = &value {
                                return (
                                    key,
                                    sanitize_base64_payload_for_estimate(payload, sanitization),
                                );
                            }
                        }
                        (
                            key,
                            sanitize_provider_wire_request_for_estimate(value, sanitization),
                        )
                    })
                    .collect(),
            )
        }
        JsonValue::String(value) => {
            sanitize_provider_wire_string_for_estimate(&value, sanitization)
                .map(JsonValue::String)
                .unwrap_or(JsonValue::String(value))
        }
        other => other,
    }
}

fn sanitize_provider_wire_string_for_estimate(
    value: &str,
    sanitization: &mut ProviderWireEstimateSanitization,
) -> Option<String> {
    let (media_type, encoded_payload) = image_data_url_payload(value)?;
    let (encoded_bytes, estimated_tokens) = account_base64_estimate(encoded_payload, sanitization);
    Some(format!(
        "data:{media_type};base64,<omitted {encoded_bytes} encoded bytes; estimated_tokens={estimated_tokens}>"
    ))
}

/// Replace a raw base64 payload (e.g. Anthropic `source.data`) with a bounded placeholder and
/// account its estimated token cost, so the fail-closed budget gate does not treat the encoded
/// bytes as ~1 token per 4 characters.
fn sanitize_base64_payload_for_estimate(
    payload: &str,
    sanitization: &mut ProviderWireEstimateSanitization,
) -> JsonValue {
    let (encoded_bytes, estimated_tokens) = account_base64_estimate(payload, sanitization);
    JsonValue::String(format!(
        "<omitted {encoded_bytes} encoded bytes; estimated_tokens={estimated_tokens}>"
    ))
}

fn account_base64_estimate(
    encoded_payload: &str,
    sanitization: &mut ProviderWireEstimateSanitization,
) -> (u64, u64) {
    let encoded_bytes = encoded_payload.len() as u64;
    let decoded_bytes = estimated_base64_decoded_bytes(encoded_payload);
    let estimated_tokens = estimate_inline_image_tokens(decoded_bytes);
    sanitization.image_data_url_count = sanitization.image_data_url_count.saturating_add(1);
    sanitization.image_data_url_encoded_bytes = sanitization
        .image_data_url_encoded_bytes
        .saturating_add(encoded_bytes);
    sanitization.image_data_url_estimated_tokens = sanitization
        .image_data_url_estimated_tokens
        .saturating_add(estimated_tokens);
    (encoded_bytes, estimated_tokens)
}

fn image_data_url_payload(value: &str) -> Option<(&str, &str)> {
    let lower = value.to_ascii_lowercase();
    // Match any base64 data URL, not just images: PDF/document attachments
    // (`data:application/pdf;base64,...`) were previously counted as raw text at ~1 token / 4
    // chars, wildly over-estimating and tripping the fail-closed context-budget gate on
    // requests the provider would accept.
    if !lower.starts_with("data:") {
        return None;
    }
    let marker = ";base64,";
    let marker_index = lower.find(marker)?;
    let media_type = &value["data:".len()..marker_index];
    let payload_start = marker_index + marker.len();
    Some((media_type, &value[payload_start..]))
}

fn estimated_base64_decoded_bytes(encoded_payload: &str) -> u64 {
    let trimmed = encoded_payload.trim_end_matches('=');
    trimmed.len().saturating_mul(3).saturating_add(3) as u64 / 4
}

fn estimate_inline_image_tokens(decoded_bytes: u64) -> u64 {
    if decoded_bytes == 0 {
        return 0;
    }
    decoded_bytes
        .saturating_add(511)
        .saturating_div(512)
        .clamp(256, 4_096)
}

fn provider_count_cache() -> &'static Mutex<HashMap<String, SessionContextEstimateDto>> {
    static CACHE: OnceLock<Mutex<HashMap<String, SessionContextEstimateDto>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn provider_count_cache_key(
    provider_id: &str,
    model_id: &str,
    counted_shape: &str,
    body: &JsonValue,
) -> CommandResult<String> {
    let serialized = serde_json::to_string(body).map_err(|error| {
        CommandError::system_fault(
            "agent_context_provider_count_cache_key_failed",
            format!(
                "Xero could not serialize the `{counted_shape}` count request for `{provider_id}/{model_id}`: {error}"
            ),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(provider_id.as_bytes());
    hasher.update([0]);
    hasher.update(model_id.as_bytes());
    hasher.update([0]);
    hasher.update(counted_shape.as_bytes());
    hasher.update([0]);
    hasher.update(serialized.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

fn cached_provider_count_estimate(cache_key: &str) -> Option<SessionContextEstimateDto> {
    provider_count_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(cache_key).cloned())
}

fn store_provider_count_estimate(cache_key: String, estimate: SessionContextEstimateDto) {
    if let Ok(mut cache) = provider_count_cache().lock() {
        cache.insert(cache_key, estimate);
    }
}

fn openai_chat_messages(
    provider_id: &str,
    request: &ProviderTurnRequest,
) -> CommandResult<Vec<JsonValue>> {
    let mut messages = vec![json!({
        "role": "system",
        "content": trusted_system_text(request),
    })];
    for message in &request.messages {
        match message {
            ProviderMessage::Developer { .. } => {}
            ProviderMessage::User {
                content,
                attachments,
            } => {
                messages.push(json!({
                    "role": "user",
                    "content": openai_chat_user_content(content, attachments)?,
                }));
            }
            ProviderMessage::Assistant {
                content,
                tool_calls,
                reasoning_content,
                reasoning_details,
            } => {
                let mut object = JsonMap::new();
                object.insert("role".into(), json!("assistant"));
                object.insert("content".into(), json!(content));
                if provider_replays_openai_reasoning_content(provider_id) {
                    if let Some(reasoning_content) = reasoning_content
                        .as_deref()
                        .map(str::trim)
                        .filter(|reasoning| !reasoning.is_empty())
                    {
                        object.insert("reasoning_content".into(), json!(reasoning_content));
                    }
                }
                if provider_replays_openai_reasoning_details(provider_id) {
                    if let Some(reasoning_details) = reasoning_details {
                        object.insert("reasoning_details".into(), reasoning_details.clone());
                    }
                }
                if !tool_calls.is_empty() {
                    object.insert(
                        "tool_calls".into(),
                        JsonValue::Array(tool_calls.iter().map(openai_chat_tool_call).collect()),
                    );
                }
                messages.push(JsonValue::Object(object));
            }
            ProviderMessage::AssistantContext { content, .. } => {
                if !content.trim().is_empty() {
                    messages.push(json!({
                        "role": "assistant",
                        "content": content,
                    }));
                }
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

fn openai_chat_user_content(
    content: &str,
    attachments: &[MessageAttachment],
) -> CommandResult<JsonValue> {
    if attachments.is_empty() {
        return Ok(json!(content));
    }

    let mut blocks = Vec::with_capacity(attachments.len() + 1);
    if !content.trim().is_empty() {
        blocks.push(json!({ "type": "text", "text": content }));
    }
    for attachment in attachments {
        match attachment.kind {
            MessageAttachmentKind::Image => {
                blocks.push(json!({
                    "type": "image_url",
                    "image_url": {
                        "url": attachment_data_url(attachment)?,
                        "detail": "auto",
                    },
                }));
            }
            MessageAttachmentKind::Document => {
                blocks.push(json!({
                    "type": "file",
                    "file": {
                        "filename": attachment.original_name,
                        "file_data": attachment_data_url(attachment)?,
                    },
                }));
            }
            MessageAttachmentKind::Text => {
                blocks.push(json!({
                    "type": "text",
                    "text": inline_text_attachment(attachment)?,
                }));
            }
        }
    }
    if blocks.is_empty() {
        blocks.push(json!({ "type": "text", "text": "" }));
    }
    Ok(JsonValue::Array(blocks))
}

fn provider_replays_openai_reasoning_content(provider_id: &str) -> bool {
    matches!(provider_id, DEEPSEEK_PROVIDER_ID | OPENROUTER_PROVIDER_ID)
}

fn provider_replays_openai_reasoning_details(provider_id: &str) -> bool {
    provider_id == OPENROUTER_PROVIDER_ID
}

fn collect_reasoning_details(buffer: &mut Vec<JsonValue>, reasoning_details: JsonValue) {
    match reasoning_details {
        JsonValue::Array(items) => {
            buffer.extend(items.into_iter().filter(|item| !item.is_null()));
        }
        value if !value.is_null() => buffer.push(value),
        _ => {}
    }
}

fn merged_reasoning_details(buffer: Vec<JsonValue>) -> Option<JsonValue> {
    if buffer.is_empty() {
        None
    } else {
        Some(JsonValue::Array(buffer))
    }
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
    let max_output_tokens = request.output_allowance.wire_max_output_tokens()?;
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
    body.insert("include".into(), json!(["reasoning.encrypted_content"]));
    body.insert("stream".into(), json!(true));
    body.insert("max_output_tokens".into(), json!(max_output_tokens));
    if let Some(effort) = request.controls.active.thinking_effort.as_ref() {
        body.insert(
            "reasoning".into(),
            json!({
                "effort": openai_responses_thinking_effort_value(provider_id, model_id, effort),
                "summary": OPENAI_REASONING_SUMMARY_DETAIL,
            }),
        );
    }
    Ok(JsonValue::Object(body))
}

fn xai_responses_request_body(
    _provider_id: &str,
    model_id: &str,
    request: &ProviderTurnRequest,
) -> CommandResult<JsonValue> {
    let max_output_tokens = request.output_allowance.wire_max_output_tokens()?;
    let mut body = JsonMap::new();
    body.insert("model".into(), json!(model_id));
    body.insert("instructions".into(), json!(trusted_system_text(request)));
    body.insert(
        "input".into(),
        JsonValue::Array(xai_response_input(request)?),
    );
    if !request.tools.is_empty() {
        body.insert(
            "tools".into(),
            JsonValue::Array(request.tools.iter().map(xai_response_tool).collect()),
        );
        body.insert("tool_choice".into(), json!("auto"));
    }
    body.insert("stream".into(), json!(true));
    body.insert("max_output_tokens".into(), json!(max_output_tokens));
    if let Some(effort) = request
        .controls
        .active
        .thinking_effort
        .as_ref()
        .and_then(xai_reasoning_effort_value)
        .filter(|_| is_supported_xai_reasoning_effort_model_id(model_id))
    {
        body.insert("reasoning".into(), json!({ "effort": effort }));
    }
    Ok(JsonValue::Object(body))
}

fn openai_codex_responses_request_body(
    provider_id: &str,
    model_id: &str,
    request: &ProviderTurnRequest,
    prompt_cache_key: Option<&str>,
) -> CommandResult<JsonValue> {
    request.output_allowance.validated()?;
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
                "summary": OPENAI_REASONING_SUMMARY_DETAIL,
            }),
        );
    }
    Ok(JsonValue::Object(body))
}

fn attachment_data_url(attachment: &MessageAttachment) -> CommandResult<String> {
    let bytes = std::fs::read(&attachment.absolute_path).map_err(|error| {
        CommandError::system_fault(
            "agent_attachment_read_failed",
            format!(
                "Xero could not read attachment `{}` from disk: {error}",
                attachment.original_name
            ),
        )
    })?;
    Ok(format!(
        "data:{};base64,{}",
        attachment.media_type,
        BASE64_STANDARD.encode(bytes)
    ))
}

fn inline_text_attachment(attachment: &MessageAttachment) -> CommandResult<String> {
    let text = std::fs::read_to_string(&attachment.absolute_path).map_err(|error| {
        CommandError::system_fault(
            "agent_attachment_read_failed",
            format!(
                "Xero could not read attached text file `{}` from disk: {error}",
                attachment.original_name
            ),
        )
    })?;
    Ok(format!(
        "<attached_file name=\"{}\">\n{}\n</attached_file>",
        attachment.original_name, text
    ))
}

fn openai_response_user_content(
    content: &str,
    attachments: &[MessageAttachment],
) -> CommandResult<JsonValue> {
    if attachments.is_empty() {
        return Ok(json!(content));
    }

    let mut blocks = Vec::with_capacity(attachments.len() + 1);
    for attachment in attachments {
        match attachment.kind {
            MessageAttachmentKind::Image => {
                blocks.push(json!({
                    "type": "input_image",
                    "image_url": attachment_data_url(attachment)?,
                    "detail": "auto",
                }));
            }
            MessageAttachmentKind::Document => {
                blocks.push(json!({
                    "type": "input_file",
                    "filename": attachment.original_name,
                    "file_data": attachment_data_url(attachment)?,
                }));
            }
            MessageAttachmentKind::Text => {
                blocks.push(json!({
                    "type": "input_text",
                    "text": inline_text_attachment(attachment)?,
                }));
            }
        }
    }
    if !content.trim().is_empty() {
        blocks.push(json!({ "type": "input_text", "text": content }));
    } else if blocks.is_empty() {
        blocks.push(json!({ "type": "input_text", "text": "" }));
    }
    Ok(JsonValue::Array(blocks))
}

fn openai_response_input(request: &ProviderTurnRequest) -> CommandResult<Vec<JsonValue>> {
    openai_response_input_with_reasoning(request, true)
}

fn xai_response_input(request: &ProviderTurnRequest) -> CommandResult<Vec<JsonValue>> {
    openai_response_input_with_reasoning(request, false)
}

fn openai_response_input_with_reasoning(
    request: &ProviderTurnRequest,
    include_openai_native_items: bool,
) -> CommandResult<Vec<JsonValue>> {
    let mut input = Vec::new();
    for message in &request.messages {
        match message {
            ProviderMessage::Developer { content } => {
                if include_openai_native_items && !content.trim().is_empty() {
                    input.push(json!({
                        "role": "developer",
                        "content": content,
                    }));
                }
            }
            ProviderMessage::User {
                content,
                attachments,
            } => {
                input.push(json!({
                    "role": "user",
                    "content": openai_response_user_content(content, attachments)?,
                }));
            }
            ProviderMessage::Assistant {
                content,
                reasoning_details,
                tool_calls,
                ..
            } => {
                if include_openai_native_items {
                    input.extend(replayable_openai_reasoning_items(
                        reasoning_details.as_ref(),
                    )?);
                }
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
            ProviderMessage::AssistantContext { content, .. } => {
                if !content.trim().is_empty() {
                    input.push(json!({
                        "role": "assistant",
                        "content": content,
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
            ProviderMessage::Developer { content } => {
                if !content.trim().is_empty() {
                    input.push(json!({
                        "role": "developer",
                        "content": [{
                            "type": "input_text",
                            "text": content,
                        }],
                    }));
                }
            }
            ProviderMessage::User {
                content,
                attachments,
            } => {
                input.push(json!({
                    "role": "user",
                    "content": openai_codex_user_content(content, attachments)?,
                }));
            }
            ProviderMessage::Assistant {
                content,
                reasoning_details,
                tool_calls,
                ..
            } => {
                input.extend(replayable_openai_reasoning_items(
                    reasoning_details.as_ref(),
                )?);
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
            ProviderMessage::AssistantContext { content, .. } => {
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

fn replayable_openai_reasoning_items(
    reasoning_details: Option<&JsonValue>,
) -> CommandResult<Vec<JsonValue>> {
    let Some(items) = reasoning_details.and_then(JsonValue::as_array) else {
        return Ok(Vec::new());
    };
    ensure_provider_stream_item_capacity(
        0,
        items.len(),
        MAX_PROVIDER_REASONING_ITEMS,
        OPENAI_API_PROVIDER_ID,
        "stored reasoning items",
    )?;

    let mut replayable = Vec::new();
    let mut seen_ids = BTreeSet::new();
    let mut total_bytes = 0_usize;
    for item in items {
        let Some((id, normalized)) = normalize_openai_reasoning_item(item) else {
            continue;
        };
        if !seen_ids.insert(id) {
            continue;
        }
        let item_bytes = json_string_storage_bytes(&normalized).ok_or_else(|| {
            provider_stream_limit_error(
                OPENAI_API_PROVIDER_ID,
                "reasoning replay",
                "stored reasoning byte count overflowed",
            )
        })?;
        total_bytes = total_bytes.checked_add(item_bytes).ok_or_else(|| {
            provider_stream_limit_error(
                OPENAI_API_PROVIDER_ID,
                "reasoning replay",
                "stored reasoning byte count overflowed",
            )
        })?;
        if total_bytes > MAX_PROVIDER_REASONING_BYTES {
            return Err(provider_stream_limit_error(
                OPENAI_API_PROVIDER_ID,
                "reasoning replay",
                &format!("stored reasoning exceeded {MAX_PROVIDER_REASONING_BYTES} bytes"),
            ));
        }
        replayable.push(normalized);
    }
    Ok(replayable)
}

fn normalize_openai_reasoning_item(item: &JsonValue) -> Option<(String, JsonValue)> {
    let object = item.as_object()?;
    if object.get("type").and_then(JsonValue::as_str) != Some("reasoning") {
        return None;
    }
    let id = object.get("id").and_then(JsonValue::as_str)?;
    if id.is_empty() || id.trim() != id || id.len() > MAX_PROVIDER_REASONING_ID_BYTES {
        return None;
    }
    let encrypted_content = object
        .get("encrypted_content")
        .and_then(JsonValue::as_str)?;
    if encrypted_content.is_empty() || encrypted_content.len() > MAX_PROVIDER_REASONING_BYTES {
        return None;
    }

    let mut normalized = JsonMap::new();
    normalized.insert("type".into(), json!("reasoning"));
    normalized.insert("id".into(), json!(id));
    normalized.insert("encrypted_content".into(), json!(encrypted_content));
    if let Some(summary) = object
        .get("summary")
        .and_then(normalize_openai_reasoning_summary)
    {
        normalized.insert("summary".into(), summary);
    }
    Some((id.to_owned(), JsonValue::Object(normalized)))
}

fn normalize_openai_reasoning_summary(summary: &JsonValue) -> Option<JsonValue> {
    let parts = summary.as_array()?;
    if parts.len() > MAX_PROVIDER_REASONING_ITEMS {
        return None;
    }
    let mut normalized = Vec::with_capacity(parts.len());
    for part in parts {
        let part = part.as_object()?;
        if part.get("type").and_then(JsonValue::as_str) != Some("summary_text") {
            return None;
        }
        let text = part.get("text").and_then(JsonValue::as_str)?;
        if text.len() > MAX_PROVIDER_REASONING_BYTES {
            return None;
        }
        normalized.push(json!({ "type": "summary_text", "text": text }));
    }
    Some(JsonValue::Array(normalized))
}

fn openai_codex_user_content(
    content: &str,
    attachments: &[MessageAttachment],
) -> CommandResult<JsonValue> {
    if attachments.is_empty() {
        return Ok(json!([{ "type": "input_text", "text": content }]));
    }
    openai_response_user_content(content, attachments)
}

fn openai_response_tool(tool: &AgentToolDescriptor) -> JsonValue {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.input_schema,
    })
}

fn xai_response_tool(tool: &AgentToolDescriptor) -> JsonValue {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": xai_sanitize_tool_schema(tool.input_schema.clone()),
    })
}

fn xai_sanitize_tool_schema(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Object(mut object) => {
            for key in [
                "minLength",
                "maxLength",
                "minItems",
                "maxItems",
                "minProperties",
                "maxProperties",
                "uniqueItems",
            ] {
                object.remove(key);
            }
            for value in object.values_mut() {
                let sanitized = xai_sanitize_tool_schema(std::mem::take(value));
                *value = sanitized;
            }
            JsonValue::Object(object)
        }
        JsonValue::Array(items) => {
            JsonValue::Array(items.into_iter().map(xai_sanitize_tool_schema).collect())
        }
        value => value,
    }
}

fn openai_codex_response_tool(tool: &AgentToolDescriptor) -> JsonValue {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": openai_codex_sanitize_tool_schema(tool.input_schema.clone()),
        "strict": JsonValue::Null,
    })
}

fn openai_codex_sanitize_tool_schema(value: JsonValue) -> JsonValue {
    let JsonValue::Object(mut root) = value else {
        return json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {},
        });
    };

    root.insert("type".into(), json!("object"));
    root.remove("enum");
    root.remove("not");

    let mut merged_disjunction = false;
    for key in ["oneOf", "anyOf", "allOf"] {
        let Some(JsonValue::Array(branches)) = root.remove(key) else {
            continue;
        };
        merged_disjunction = true;
        for branch in branches {
            merge_openai_codex_tool_schema_branch(&mut root, branch);
        }
    }
    if merged_disjunction {
        root.remove("required");
    }

    JsonValue::Object(root)
}

fn merge_openai_codex_tool_schema_branch(root: &mut JsonMap<String, JsonValue>, branch: JsonValue) {
    let JsonValue::Object(branch) = branch else {
        return;
    };

    if let Some(JsonValue::Object(branch_properties)) = branch.get("properties") {
        let root_properties = root
            .entry("properties")
            .or_insert_with(|| JsonValue::Object(JsonMap::new()));
        if let JsonValue::Object(root_properties) = root_properties {
            for (key, value) in branch_properties {
                root_properties
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
        }
    }

    if !root.contains_key("additionalProperties") {
        if let Some(additional_properties) = branch.get("additionalProperties") {
            root.insert("additionalProperties".into(), additional_properties.clone());
        }
    }
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
    request: AsyncRequestBuilder,
    api_key: &str,
) -> AsyncRequestBuilder {
    match provider_id {
        AZURE_OPENAI_PROVIDER_ID => request.header("api-key", api_key),
        _ => request.bearer_auth(api_key),
    }
}

fn apply_openai_compatible_provider_headers(
    provider_id: &str,
    request: AsyncRequestBuilder,
) -> AsyncRequestBuilder {
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
        ProviderModelThinkingEffortDto::None => "none",
        ProviderModelThinkingEffortDto::Minimal => "minimal",
        ProviderModelThinkingEffortDto::Low => "low",
        ProviderModelThinkingEffortDto::Medium => "medium",
        ProviderModelThinkingEffortDto::High => "high",
        ProviderModelThinkingEffortDto::XHigh => "xhigh",
    }
}

fn deepseek_thinking_effort_value(effort: &ProviderModelThinkingEffortDto) -> &'static str {
    match effort {
        ProviderModelThinkingEffortDto::XHigh => "max",
        ProviderModelThinkingEffortDto::None
        | ProviderModelThinkingEffortDto::Minimal
        | ProviderModelThinkingEffortDto::Low
        | ProviderModelThinkingEffortDto::Medium
        | ProviderModelThinkingEffortDto::High => "high",
    }
}

fn openrouter_reasoning_effort_value(effort: &ProviderModelThinkingEffortDto) -> &'static str {
    match effort {
        ProviderModelThinkingEffortDto::XHigh => "xhigh",
        ProviderModelThinkingEffortDto::None
        | ProviderModelThinkingEffortDto::Minimal
        | ProviderModelThinkingEffortDto::Low
        | ProviderModelThinkingEffortDto::Medium
        | ProviderModelThinkingEffortDto::High => "high",
    }
}

fn xai_reasoning_effort_value(effort: &ProviderModelThinkingEffortDto) -> Option<&'static str> {
    match effort {
        ProviderModelThinkingEffortDto::None => Some("none"),
        ProviderModelThinkingEffortDto::Low => Some("low"),
        ProviderModelThinkingEffortDto::Medium => Some("medium"),
        ProviderModelThinkingEffortDto::High => Some("high"),
        ProviderModelThinkingEffortDto::Minimal | ProviderModelThinkingEffortDto::XHigh => None,
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

    if ["gpt-5.2", "gpt-5.3", "gpt-5.4", "gpt-5.5", "gpt-5.6"]
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
    let output_allowance = request.output_allowance.validated()?;
    let (Some(reasoning_tokens), Some(visible_output_tokens)) = (
        output_allowance.reasoning_tokens,
        output_allowance.visible_output_tokens,
    ) else {
        return Err(CommandError::user_fixable(
            "agent_provider_output_allowance_invalid",
            "Xero refused to submit an Anthropic-family turn because its resolved allowance did not include the required reasoning/visible-output split. Refresh provider diagnostics and retry the run.",
        ));
    };
    let expected_reasoning_tokens = request
        .controls
        .active
        .thinking_effort
        .as_ref()
        .and_then(anthropic_thinking_budget_tokens)
        .unwrap_or_default();
    if reasoning_tokens != expected_reasoning_tokens {
        return Err(CommandError::user_fixable(
            "agent_provider_output_allowance_invalid",
            format!(
                "Xero refused to submit an Anthropic-family turn because its resolved reasoning allowance ({reasoning_tokens}) does not match the active thinking policy ({expected_reasoning_tokens}). Rebuild the turn with current provider controls and retry."
            ),
        ));
    }
    if visible_output_tokens == 0 {
        return Err(CommandError::user_fixable(
            "agent_provider_reasoning_output_limit_exceeded",
            "Xero refused to submit an Anthropic-family turn because its thinking budget leaves no visible output allowance. Choose a lower thinking effort or a model with a larger output limit.",
        ));
    }
    let mut body = JsonMap::new();
    if let Some(model_id) = model_id {
        body.insert("model".into(), json!(model_id));
    }
    // Bedrock (InvokeModel) and Vertex (rawPredict/streamRawPredict) select streaming via the
    // API operation, not a body field, and their strict Anthropic-on-* schema rejects an
    // extraneous `stream` key with a ValidationException. Only the native Anthropic Messages
    // API takes `stream` in the body.
    let is_native_anthropic =
        !anthropic_version.starts_with("bedrock-") && !anthropic_version.starts_with("vertex-");
    let trusted_system = trusted_system_text(request);
    // Prompt-cache breakpoints only on the native Messages API; the Bedrock/Vertex
    // Anthropic schemas are stricter and cache support varies by region/model there.
    if is_native_anthropic {
        body.insert(
            "system".into(),
            json!([{
                "type": "text",
                "text": trusted_system,
                "cache_control": { "type": "ephemeral" },
            }]),
        );
    } else {
        body.insert("system".into(), json!(trusted_system));
    }
    body.insert(
        "max_tokens".into(),
        json!(output_allowance.max_output_tokens),
    );
    let thinking_budget = (reasoning_tokens > 0).then_some(reasoning_tokens);
    if let Some(budget) = thinking_budget {
        body.insert(
            "thinking".into(),
            json!({ "type": "enabled", "budget_tokens": budget }),
        );
    }
    if is_native_anthropic {
        body.insert("stream".into(), json!(stream));
    }
    let mut messages = anthropic_messages(request, thinking_budget.is_some())?;
    if is_native_anthropic {
        if let Some(last_block) = messages
            .last_mut()
            .and_then(|message| message.get_mut("content"))
            .and_then(JsonValue::as_array_mut)
            .and_then(|blocks| blocks.last_mut())
        {
            attach_anthropic_cache_control(last_block);
        }
    }
    body.insert("messages".into(), JsonValue::Array(messages));
    let mut tools: Vec<JsonValue> = request.tools.iter().map(anthropic_tool).collect();
    if is_native_anthropic {
        if let Some(last_tool) = tools.last_mut() {
            attach_anthropic_cache_control(last_tool);
        }
    }
    body.insert("tools".into(), JsonValue::Array(tools));
    if !is_native_anthropic {
        body.insert("anthropic_version".into(), json!(anthropic_version));
    }
    Ok(JsonValue::Object(body))
}

fn attach_anthropic_cache_control(target: &mut JsonValue) {
    if let JsonValue::Object(object) = target {
        object.insert("cache_control".into(), json!({ "type": "ephemeral" }));
    }
}

fn anthropic_thinking_budget_tokens(effort: &ProviderModelThinkingEffortDto) -> Option<u64> {
    match effort {
        ProviderModelThinkingEffortDto::None => None,
        ProviderModelThinkingEffortDto::Minimal => Some(1_024),
        ProviderModelThinkingEffortDto::Low => Some(2_048),
        ProviderModelThinkingEffortDto::Medium => Some(4_096),
        ProviderModelThinkingEffortDto::High => Some(8_192),
        ProviderModelThinkingEffortDto::XHigh => Some(16_384),
    }
}

fn anthropic_count_tokens_body(
    model_id: &str,
    anthropic_version: &str,
    request: &ProviderTurnRequest,
) -> CommandResult<JsonValue> {
    let mut body = anthropic_request_body(Some(model_id), anthropic_version, request, false)?;
    if let JsonValue::Object(object) = &mut body {
        object.remove("stream");
        object.remove("max_tokens");
    }
    Ok(body)
}

fn estimate_anthropic_context_tokens(
    config: &AnthropicProviderConfig,
    client: &BlockingClient,
    request: &ProviderTurnRequest,
) -> CommandResult<SessionContextEstimateDto> {
    let counted_shape = "anthropic_messages_count_tokens";
    let count_body =
        anthropic_count_tokens_body(&config.model_id, &config.anthropic_version, request)?;
    let fallback_body = count_body.clone();
    let cache_key = provider_count_cache_key(
        &config.provider_id,
        &config.model_id,
        counted_shape,
        &count_body,
    )?;
    if let Some(estimate) = cached_provider_count_estimate(&cache_key) {
        return Ok(estimate);
    }

    let url = anthropic_count_tokens_url(&config.base_url)?;
    let response = send_provider_json_request(&config.provider_id, || {
        client
            .post(url.clone())
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", &config.anthropic_version)
            .json(&count_body)
    });
    let estimate = match response {
        Ok(mut response) => {
            let text = read_provider_text_bounded(
                &mut response,
                MAX_PROVIDER_TOKEN_COUNT_BODY_BYTES,
                &config.provider_id,
                "token-count response body",
            );
            match text.and_then(|text| {
                serde_json::from_str::<JsonValue>(&text).map_err(|error| {
                    CommandError::retryable(
                        "anthropic_count_tokens_response_decode_failed",
                        format!(
                            "Xero could not decode the Anthropic token-count response for `{}`: {error}",
                            config.model_id
                        ),
                    )
                })
            }) {
                Ok(value) => {
                    if let Some(tokens) = value
                        .get("input_tokens")
                        .and_then(JsonValue::as_u64)
                        .filter(|tokens| *tokens > 0)
                    {
                        SessionContextEstimateDto {
                            tokens,
                            source: SessionContextEstimateSourceDto::ProviderCountApi,
                            confidence: SessionContextEstimateConfidenceDto::High,
                            counted_shape: counted_shape.into(),
                            diagnostics: vec![format!(
                                "Counted with Anthropic `messages/count_tokens` for `{}`.",
                                config.model_id
                            )],
                        }
                    } else {
                        let mut fallback = estimate_provider_wire_context_tokens(
                            &config.provider_id,
                            &config.model_id,
                            "anthropic_messages_wire_request",
                            fallback_body.clone(),
                        )?;
                        fallback.diagnostics.push(
                            "Anthropic token-count response did not include `input_tokens`; fell back to local wire-shape estimate."
                                .into(),
                        );
                        fallback
                    }
                }
                Err(error) => {
                    let mut fallback = estimate_provider_wire_context_tokens(
                        &config.provider_id,
                        &config.model_id,
                        "anthropic_messages_wire_request",
                        fallback_body.clone(),
                    )?;
                    fallback.diagnostics.push(format!(
                        "Anthropic token-count API response was unavailable: {}",
                        error.message
                    ));
                    fallback
                }
            }
        }
        Err(error) => {
            let mut fallback = estimate_provider_wire_context_tokens(
                &config.provider_id,
                &config.model_id,
                "anthropic_messages_wire_request",
                fallback_body,
            )?;
            fallback.diagnostics.push(format!(
                "Anthropic token-count API was unavailable: {}",
                error.message
            ));
            fallback
        }
    };
    store_provider_count_estimate(cache_key, estimate.clone());
    Ok(estimate)
}

fn anthropic_messages(
    request: &ProviderTurnRequest,
    include_thinking: bool,
) -> CommandResult<Vec<JsonValue>> {
    let mut messages = Vec::new();
    for message in &request.messages {
        match message {
            ProviderMessage::Developer { .. } => {}
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
                reasoning_details,
                tool_calls,
                ..
            } => {
                let mut blocks = Vec::new();
                // With extended thinking enabled, Anthropic requires assistant
                // tool_use turns to replay their signed thinking blocks ahead
                // of the other content blocks; without thinking enabled the
                // request must not carry thinking blocks at all.
                if include_thinking {
                    if let Some(JsonValue::Array(details)) = reasoning_details {
                        blocks.extend(
                            details
                                .iter()
                                .filter(|block| anthropic_replayable_thinking_block(block))
                                .cloned(),
                        );
                    }
                }
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
            ProviderMessage::AssistantContext { content, .. } => {
                if !content.trim().is_empty() {
                    messages.push(json!({
                        "role": "assistant",
                        "content": [{ "type": "text", "text": content }],
                    }));
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
    if messages
        .first()
        .and_then(|message| message.get("role"))
        .and_then(JsonValue::as_str)
        == Some("assistant")
    {
        messages.insert(
            0,
            json!({
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": ANTHROPIC_ASSISTANT_CONTEXT_PREAMBLE,
                }],
            }),
        );
    }
    Ok(messages)
}

/// Only replay blocks that are structurally valid Anthropic thinking blocks.
/// Assistant messages can carry reasoning_details from other providers (e.g.
/// OpenRouter) after a mid-run model switch; replaying those as thinking
/// blocks would fail Anthropic's request validation.
fn anthropic_replayable_thinking_block(block: &JsonValue) -> bool {
    match block.get("type").and_then(JsonValue::as_str) {
        Some("thinking") => {
            block
                .get("thinking")
                .and_then(JsonValue::as_str)
                .is_some_and(|thinking| !thinking.is_empty())
                && block
                    .get("signature")
                    .and_then(JsonValue::as_str)
                    .is_some_and(|signature| !signature.is_empty())
        }
        Some("redacted_thinking") => block
            .get("data")
            .and_then(JsonValue::as_str)
            .is_some_and(|data| !data.is_empty()),
        _ => false,
    }
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

fn validate_partial_tool_calls(
    provider_id: &str,
    partial_calls: &BTreeMap<usize, PartialToolCall>,
) -> CommandResult<()> {
    ensure_provider_stream_item_capacity(
        0,
        partial_calls.len(),
        MAX_PROVIDER_TOOL_CALLS,
        provider_id,
        "tool calls",
    )?;
    for partial in partial_calls.values() {
        if partial.arguments.len() > MAX_PROVIDER_TOOL_ARGUMENT_BYTES {
            return Err(provider_stream_limit_error(
                provider_id,
                "response stream",
                &format!("tool arguments exceeded {MAX_PROVIDER_TOOL_ARGUMENT_BYTES} bytes"),
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct OpenAiChatChunk {
    #[serde(default)]
    choices: Vec<OpenAiChatChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
    // OpenRouter and some compatible gateways deliver a mid-stream `{"error": {...}}` frame
    // instead of an SSE `error` event. Capture it so it is surfaced rather than silently
    // parsed as a chunk with no choices.
    #[serde(default)]
    error: Option<JsonValue>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatChoice {
    #[serde(default)]
    delta: OpenAiChatDelta,
    #[serde(default)]
    finish_reason: Option<String>,
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
    reasoning_details: Option<JsonValue>,
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

fn parse_openai_chat_sse<R: Read>(
    provider_id: &str,
    response: R,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<ProviderTurnOutcome> {
    let mut message = String::new();
    let mut reasoning_content_buffer = String::new();
    let mut reasoning_details_buffer = Vec::<JsonValue>::new();
    let mut partial_calls = BTreeMap::<usize, PartialToolCall>::new();
    let mut usage = None;
    let mut finish_reason: Option<String> = None;
    let mut stream = BoundedSseReader::new(BufReader::new(response), PROVIDER_STREAM_LIMITS);

    while let Some(data) = stream.next_data(provider_id, "response stream")? {
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            break;
        }
        let chunk: OpenAiChatChunk = serde_json::from_str(&data).map_err(|error| {
            CommandError::retryable(
                "agent_provider_stream_decode_failed",
                format!("Xero could not decode a {provider_id} stream chunk: {error}"),
            )
        })?;
        if let Some(error) = chunk.error.as_ref() {
            return Err(anthropic_stream_error(
                provider_id,
                &json!({ "error": error }),
            ));
        }
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
        ensure_provider_stream_item_capacity(
            0,
            chunk.choices.len(),
            MAX_PROVIDER_ITEMS_PER_EVENT,
            provider_id,
            "choices in one event",
        )?;
        for choice in chunk.choices {
            if let Some(reason) = choice.finish_reason {
                finish_reason = Some(reason);
            }
            let OpenAiChatDelta {
                content,
                reasoning,
                reasoning_content,
                reasoning_details,
                tool_calls,
            } = choice.delta;
            if provider_replays_openai_reasoning_details(provider_id) {
                if let Some(reasoning_details) = reasoning_details {
                    collect_reasoning_details(&mut reasoning_details_buffer, reasoning_details);
                    ensure_provider_stream_item_capacity(
                        0,
                        reasoning_details_buffer.len(),
                        MAX_PROVIDER_REASONING_ITEMS,
                        provider_id,
                        "reasoning detail items",
                    )?;
                }
            }
            if let Some(reasoning) = reasoning_content
                .or(reasoning)
                .filter(|reasoning| !reasoning.is_empty())
            {
                append_provider_stream_text(
                    &mut reasoning_content_buffer,
                    &reasoning,
                    MAX_PROVIDER_REASONING_BYTES,
                    provider_id,
                    "reasoning text",
                )?;
                emit(ProviderStreamEvent::ReasoningSummary(reasoning))?;
            }
            if let Some(content) = content {
                append_provider_stream_text(
                    &mut message,
                    &content,
                    MAX_PROVIDER_MESSAGE_BYTES,
                    provider_id,
                    "message text",
                )?;
                emit(ProviderStreamEvent::MessageDelta(content))?;
            }
            ensure_provider_stream_item_capacity(
                0,
                tool_calls.len(),
                MAX_PROVIDER_ITEMS_PER_EVENT,
                provider_id,
                "tool call deltas in one event",
            )?;
            for tool_call in tool_calls {
                if !partial_calls.contains_key(&tool_call.index) {
                    ensure_provider_stream_item_capacity(
                        partial_calls.len(),
                        1,
                        MAX_PROVIDER_TOOL_CALLS,
                        provider_id,
                        "tool calls",
                    )?;
                }
                let partial = partial_calls.entry(tool_call.index).or_default();
                if let Some(id) = tool_call.id {
                    partial.id = Some(id);
                }
                if let Some(function) = tool_call.function {
                    if let Some(name) = function.name {
                        partial.name = Some(name);
                    }
                    if let Some(arguments) = function.arguments {
                        append_provider_stream_text(
                            &mut partial.arguments,
                            &arguments,
                            MAX_PROVIDER_TOOL_ARGUMENT_BYTES,
                            provider_id,
                            "tool arguments",
                        )?;
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

    // `tool_calls` and `stop`/`end_turn` are normal; only `length` signals the model was cut
    // off at the output-token limit. Surface truncation instead of returning partial text.
    ensure_provider_output_not_truncated(provider_id, finish_reason.as_deref())?;
    let reasoning_content = provider_replays_openai_reasoning_content(provider_id)
        .then(|| reasoning_content_buffer.trim().to_owned())
        .filter(|reasoning| !reasoning.is_empty());
    let reasoning_details = provider_replays_openai_reasoning_details(provider_id)
        .then(|| merged_reasoning_details(reasoning_details_buffer))
        .flatten();
    finish_provider_turn(
        provider_id,
        message,
        reasoning_content,
        reasoning_details,
        partial_calls,
        usage,
    )
}

fn parse_openai_responses_sse<R: Read>(
    provider_id: &str,
    response: R,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<ProviderTurnOutcome> {
    let mut message = String::new();
    let mut partial_calls = BTreeMap::<usize, PartialToolCall>::new();
    let mut completed_call_count = 0_usize;
    let mut reasoning_summary_state = OpenAiResponsesReasoningSummaryState::default();
    let mut usage = None;
    let mut stream = BoundedSseReader::new(BufReader::new(response), PROVIDER_STREAM_LIMITS);

    while let Some(data) = stream.next_data(provider_id, "Responses stream")? {
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            break;
        }
        let value: JsonValue = serde_json::from_str(&data).map_err(|error| {
            CommandError::retryable(
                "agent_provider_stream_decode_failed",
                format!("Xero could not decode a {provider_id} Responses chunk: {error}"),
            )
        })?;
        if emit_openai_responses_reasoning_summary_event(
            provider_id,
            &value,
            &mut reasoning_summary_state,
            emit,
        )? {
            validate_openai_responses_reasoning_summary_state(
                provider_id,
                &reasoning_summary_state,
            )?;
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
                    append_provider_stream_text(
                        &mut message,
                        &delta,
                        MAX_PROVIDER_MESSAGE_BYTES,
                        provider_id,
                        "message text",
                    )?;
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
                if !partial_calls.contains_key(&index) {
                    ensure_provider_stream_item_capacity(
                        partial_calls.len(),
                        1,
                        MAX_PROVIDER_TOOL_CALLS,
                        provider_id,
                        "tool calls",
                    )?;
                }
                let partial = partial_calls.entry(index).or_default();
                append_provider_stream_text(
                    &mut partial.arguments,
                    &delta,
                    MAX_PROVIDER_TOOL_ARGUMENT_BYTES,
                    provider_id,
                    "tool arguments",
                )?;
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
                validate_partial_tool_calls(provider_id, &partial_calls)?;
            }
            "response.output_item.done" => {
                if apply_openai_response_function_call_item(
                    &mut partial_calls,
                    &value,
                    completed_call_count,
                ) {
                    completed_call_count = completed_call_count.saturating_add(1);
                }
                validate_partial_tool_calls(provider_id, &partial_calls)?;
            }
            "response.completed" | "response.done" => {
                emit_openai_responses_completed_reasoning_summaries(
                    provider_id,
                    &value,
                    &mut reasoning_summary_state,
                    emit,
                )?;
                validate_openai_responses_reasoning_summary_state(
                    provider_id,
                    &reasoning_summary_state,
                )?;
                if let Some(mapped) = value
                    .get("response")
                    .and_then(|response| response.get("usage"))
                    .map(openai_responses_usage)
                {
                    emit(ProviderStreamEvent::Usage(mapped.clone()))?;
                    usage = Some(mapped);
                }
            }
            // The response ended without finishing (most often reasoning + output exhausted
            // `max_output_tokens`). Record usage, then surface it rather than returning the
            // partial (possibly empty) output as a complete turn.
            "response.incomplete" => {
                if let Some(mapped) = value
                    .get("response")
                    .and_then(|response| response.get("usage"))
                    .map(openai_responses_usage)
                {
                    emit(ProviderStreamEvent::Usage(mapped.clone()))?;
                }
                let reason = value
                    .get("response")
                    .and_then(|response| response.get("incomplete_details"))
                    .and_then(|details| details.get("reason"))
                    .and_then(JsonValue::as_str);
                let normalized = match reason {
                    Some("max_output_tokens") => Some("max_tokens"),
                    other => other,
                };
                ensure_provider_output_not_truncated(provider_id, normalized)?;
                return Err(CommandError::user_fixable(
                    "agent_provider_output_incomplete",
                    format!(
                        "The {provider_id} response ended before completing ({}).",
                        reason.unwrap_or("unknown reason")
                    ),
                ));
            }
            _ => {}
        }
    }

    validate_openai_responses_reasoning_summary_state(provider_id, &reasoning_summary_state)?;
    flush_openai_responses_reasoning_summary_pending(
        provider_id,
        &mut reasoning_summary_state,
        emit,
    )?;
    validate_openai_responses_reasoning_summary_state(provider_id, &reasoning_summary_state)?;
    let reasoning_details = reasoning_summary_state.replayable_reasoning_details();

    finish_provider_turn(
        provider_id,
        message,
        None,
        reasoning_details,
        partial_calls,
        usage,
    )
}

fn emit_openai_responses_completed_reasoning_summaries(
    provider_id: &str,
    value: &JsonValue,
    state: &mut OpenAiResponsesReasoningSummaryState,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<()> {
    let Some(output) = value
        .get("response")
        .and_then(|response| response.get("output"))
        .and_then(JsonValue::as_array)
    else {
        return Ok(());
    };
    ensure_provider_stream_item_capacity(
        0,
        output.len(),
        MAX_PROVIDER_ITEMS_PER_EVENT,
        provider_id,
        "response output items in one event",
    )?;

    let mut emitted_any = false;
    for (fallback_output_index, item) in output.iter().enumerate() {
        if item.get("type").and_then(JsonValue::as_str) != Some("reasoning") {
            continue;
        }
        let output_index = item
            .get("output_index")
            .and_then(JsonValue::as_u64)
            .unwrap_or(fallback_output_index as u64);
        collect_openai_responses_reasoning_item(provider_id, state, item)?;
        emitted_any |= emit_openai_responses_reasoning_summary_item(
            provider_id,
            output_index,
            item,
            state,
            emit,
        )?;
    }

    if emitted_any {
        emit(ProviderStreamEvent::ReasoningSummary("\n\n".into()))?;
    }
    Ok(())
}

#[derive(Default)]
struct OpenAiResponsesReasoningSummaryState {
    pending_text: BTreeMap<(u64, u64), String>,
    emitted_text: BTreeMap<(u64, u64), String>,
    replayable_items: Vec<JsonValue>,
    replayable_item_indices: BTreeMap<String, usize>,
}

impl OpenAiResponsesReasoningSummaryState {
    fn replayable_reasoning_details(&self) -> Option<JsonValue> {
        (!self.replayable_items.is_empty()).then(|| JsonValue::Array(self.replayable_items.clone()))
    }
}

fn provider_replays_openai_responses_reasoning_items(provider_id: &str) -> bool {
    matches!(
        provider_id,
        OPENAI_API_PROVIDER_ID | OPENAI_CODEX_PROVIDER_ID
    )
}

fn collect_openai_responses_reasoning_item(
    provider_id: &str,
    state: &mut OpenAiResponsesReasoningSummaryState,
    item: &JsonValue,
) -> CommandResult<()> {
    if !provider_replays_openai_responses_reasoning_items(provider_id) {
        return Ok(());
    }
    let Some((id, normalized)) = normalize_openai_reasoning_item(item) else {
        return Ok(());
    };

    if let Some(index) = state.replayable_item_indices.get(&id).copied() {
        let existing = &mut state.replayable_items[index];
        if existing.get("encrypted_content") != normalized.get("encrypted_content") {
            return Err(CommandError::retryable(
                "agent_provider_reasoning_item_conflict",
                format!(
                    "The {provider_id} Responses stream reused reasoning item `{id}` with different encrypted content."
                ),
            ));
        }
        if existing.get("summary").is_none() {
            if let Some(summary) = normalized.get("summary") {
                existing["summary"] = summary.clone();
            }
        }
        return validate_openai_responses_replayable_items(provider_id, state);
    }

    ensure_provider_stream_item_capacity(
        state.replayable_items.len(),
        1,
        MAX_PROVIDER_REASONING_ITEMS,
        provider_id,
        "replayable reasoning items",
    )?;
    let index = state.replayable_items.len();
    state.replayable_items.push(normalized);
    state.replayable_item_indices.insert(id, index);
    validate_openai_responses_replayable_items(provider_id, state)
}

fn validate_openai_responses_replayable_items(
    provider_id: &str,
    state: &OpenAiResponsesReasoningSummaryState,
) -> CommandResult<()> {
    ensure_provider_stream_item_capacity(
        0,
        state.replayable_items.len(),
        MAX_PROVIDER_REASONING_ITEMS,
        provider_id,
        "replayable reasoning items",
    )?;
    let total_bytes = state
        .replayable_items
        .iter()
        .try_fold(0_usize, |total, item| {
            json_string_storage_bytes(item).and_then(|bytes| total.checked_add(bytes))
        })
        .ok_or_else(|| {
            provider_stream_limit_error(
                provider_id,
                "Responses stream",
                "replayable reasoning byte count overflowed",
            )
        })?;
    if total_bytes > MAX_PROVIDER_REASONING_BYTES {
        return Err(provider_stream_limit_error(
            provider_id,
            "Responses stream",
            &format!("replayable reasoning items exceeded {MAX_PROVIDER_REASONING_BYTES} bytes"),
        ));
    }
    Ok(())
}

fn validate_openai_responses_reasoning_summary_state(
    provider_id: &str,
    state: &OpenAiResponsesReasoningSummaryState,
) -> CommandResult<()> {
    validate_openai_responses_replayable_items(provider_id, state)?;
    ensure_provider_stream_item_capacity(
        0,
        state.pending_text.len(),
        MAX_PROVIDER_REASONING_ITEMS,
        provider_id,
        "pending reasoning summary items",
    )?;
    ensure_provider_stream_item_capacity(
        0,
        state.emitted_text.len(),
        MAX_PROVIDER_REASONING_ITEMS,
        provider_id,
        "emitted reasoning summary items",
    )?;
    let total_bytes = state
        .pending_text
        .values()
        .chain(state.emitted_text.values())
        .try_fold(0_usize, |total, text| total.checked_add(text.len()))
        .ok_or_else(|| {
            provider_stream_limit_error(
                provider_id,
                "Responses stream",
                "reasoning summary byte count overflowed",
            )
        })?;
    if total_bytes > MAX_PROVIDER_REASONING_BYTES {
        return Err(provider_stream_limit_error(
            provider_id,
            "Responses stream",
            &format!("reasoning summaries exceeded {MAX_PROVIDER_REASONING_BYTES} bytes"),
        ));
    }
    Ok(())
}

fn emit_openai_responses_reasoning_summary_event(
    provider_id: &str,
    value: &JsonValue,
    state: &mut OpenAiResponsesReasoningSummaryState,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<bool> {
    match value
        .get("type")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
    {
        "response.reasoning_summary_text.delta" => {
            let key = openai_responses_reasoning_summary_key(value);
            let delta = value
                .get("delta")
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_owned();
            if !delta.is_empty() {
                if !state.pending_text.contains_key(&key) {
                    ensure_provider_stream_item_capacity(
                        state.pending_text.len(),
                        1,
                        MAX_PROVIDER_REASONING_ITEMS,
                        provider_id,
                        "pending reasoning summary items",
                    )?;
                }
                append_provider_stream_text(
                    state.pending_text.entry(key).or_default(),
                    &delta,
                    MAX_PROVIDER_REASONING_BYTES,
                    provider_id,
                    "reasoning summary text",
                )?;
            }
            Ok(true)
        }
        "response.reasoning_summary_text.done" => {
            let key = openai_responses_reasoning_summary_key(value);
            let pending = state.pending_text.remove(&key).unwrap_or_default();
            let text = value
                .get("text")
                .and_then(JsonValue::as_str)
                .filter(|text| !text.is_empty())
                .unwrap_or(pending.as_str());
            emit_openai_responses_reasoning_summary_text(provider_id, key, text, state, emit)?;
            Ok(true)
        }
        "response.reasoning_summary_part.done" => {
            let key = openai_responses_reasoning_summary_key(value);
            let pending = state.pending_text.remove(&key);
            let fallback = pending
                .as_deref()
                .or_else(|| {
                    value
                        .get("part")
                        .and_then(|part| part.get("text"))
                        .and_then(JsonValue::as_str)
                })
                .unwrap_or_default();
            let emitted = emit_openai_responses_reasoning_summary_text(
                provider_id,
                key,
                fallback,
                state,
                emit,
            )? || state.emitted_text.contains_key(&key);
            if emitted {
                emit(ProviderStreamEvent::ReasoningSummary("\n\n".into()))?;
            }
            Ok(true)
        }
        "response.reasoning_summary_part.added" => {
            let key = openai_responses_reasoning_summary_key(value);
            if let Some(text) = value
                .get("part")
                .and_then(|part| part.get("text"))
                .and_then(JsonValue::as_str)
                .filter(|text| !text.is_empty())
            {
                if !state.pending_text.contains_key(&key) {
                    ensure_provider_stream_item_capacity(
                        state.pending_text.len(),
                        1,
                        MAX_PROVIDER_REASONING_ITEMS,
                        provider_id,
                        "pending reasoning summary items",
                    )?;
                }
                append_provider_stream_text(
                    state.pending_text.entry(key).or_default(),
                    text,
                    MAX_PROVIDER_REASONING_BYTES,
                    provider_id,
                    "reasoning summary text",
                )?;
            }
            Ok(true)
        }
        "response.output_item.done" => {
            let item = value.get("item").unwrap_or(&JsonValue::Null);
            if item.get("type").and_then(JsonValue::as_str) != Some("reasoning") {
                return Ok(false);
            }
            collect_openai_responses_reasoning_item(provider_id, state, item)?;
            let output_index = value
                .get("output_index")
                .or_else(|| item.get("output_index"))
                .and_then(JsonValue::as_u64)
                .unwrap_or_default();
            let emitted_any = emit_openai_responses_reasoning_summary_item(
                provider_id,
                output_index,
                item,
                state,
                emit,
            )?;
            if emitted_any {
                emit(ProviderStreamEvent::ReasoningSummary("\n\n".into()))?;
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn flush_openai_responses_reasoning_summary_pending(
    provider_id: &str,
    state: &mut OpenAiResponsesReasoningSummaryState,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<()> {
    let pending = std::mem::take(&mut state.pending_text);
    for (key, text) in pending {
        if emit_openai_responses_reasoning_summary_text(provider_id, key, &text, state, emit)? {
            emit(ProviderStreamEvent::ReasoningSummary("\n\n".into()))?;
        }
    }
    Ok(())
}

fn emit_openai_responses_reasoning_summary_item(
    provider_id: &str,
    output_index: u64,
    item: &JsonValue,
    state: &mut OpenAiResponsesReasoningSummaryState,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<bool> {
    let mut emitted_any = false;
    if let Some(summary) = item.get("summary").and_then(JsonValue::as_array) {
        for (summary_index, part) in summary.iter().enumerate() {
            let key = (output_index, summary_index as u64);
            let text = part
                .get("text")
                .and_then(JsonValue::as_str)
                .unwrap_or_default();
            emitted_any |=
                emit_openai_responses_reasoning_summary_text(provider_id, key, text, state, emit)?;
        }
    }
    Ok(emitted_any)
}

fn openai_responses_reasoning_summary_key(value: &JsonValue) -> (u64, u64) {
    (
        value
            .get("output_index")
            .and_then(JsonValue::as_u64)
            .unwrap_or_default(),
        value
            .get("summary_index")
            .and_then(JsonValue::as_u64)
            .unwrap_or_default(),
    )
}

fn emit_openai_responses_reasoning_summary_text(
    provider_id: &str,
    key: (u64, u64),
    text: &str,
    state: &mut OpenAiResponsesReasoningSummaryState,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<bool> {
    if text.is_empty() {
        return Ok(false);
    }
    if text.len() > MAX_PROVIDER_REASONING_BYTES {
        return Err(provider_stream_limit_error(
            provider_id,
            "Responses stream",
            &format!("reasoning summary text exceeded {MAX_PROVIDER_REASONING_BYTES} bytes"),
        ));
    }
    if let Some(previous) = state.emitted_text.get(&key) {
        if previous == text {
            return Ok(false);
        }
        if let Some(delta) = text.strip_prefix(previous) {
            if delta.is_empty() {
                return Ok(false);
            }
            emit(ProviderStreamEvent::ReasoningSummary(delta.to_owned()))?;
            state.emitted_text.insert(key, text.to_owned());
            validate_openai_responses_reasoning_summary_state(provider_id, state)?;
            return Ok(true);
        }
        return Ok(false);
    }
    if !state.emitted_text.contains_key(&key) {
        ensure_provider_stream_item_capacity(
            state.emitted_text.len(),
            1,
            MAX_PROVIDER_REASONING_ITEMS,
            provider_id,
            "emitted reasoning summary items",
        )?;
    }
    emit(ProviderStreamEvent::ReasoningSummary(text.to_owned()))?;
    state.emitted_text.insert(key, text.to_owned());
    validate_openai_responses_reasoning_summary_state(provider_id, state)?;
    Ok(true)
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
        input_tokens,
        billable_input_tokens,
        output_tokens,
        total_tokens: if total_tokens > 0 {
            total_tokens
        } else {
            input_tokens.saturating_add(output_tokens)
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

fn parse_anthropic_sse<R: Read>(
    provider_id: &str,
    response: R,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<ProviderTurnOutcome> {
    let mut message = String::new();
    let mut partial_calls = BTreeMap::<usize, PartialToolCall>::new();
    let mut thinking_blocks = BTreeMap::<usize, JsonValue>::new();
    let mut usage = ProviderUsage::default();
    let mut stop_reason: Option<String> = None;
    let mut stream = BoundedSseReader::new(BufReader::new(response), PROVIDER_STREAM_LIMITS);

    while let Some(data) = stream.next_data(provider_id, "response stream")? {
        if data.is_empty() {
            continue;
        }
        let value: JsonValue = serde_json::from_str(&data).map_err(|error| {
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
                match block.get("type").and_then(JsonValue::as_str) {
                    Some("tool_use") => {
                        if !partial_calls.contains_key(&index) {
                            ensure_provider_stream_item_capacity(
                                partial_calls.len(),
                                1,
                                MAX_PROVIDER_TOOL_CALLS,
                                provider_id,
                                "tool calls",
                            )?;
                        }
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
                    Some("thinking") => {
                        if !thinking_blocks.contains_key(&index) {
                            ensure_provider_stream_item_capacity(
                                thinking_blocks.len(),
                                1,
                                MAX_PROVIDER_REASONING_ITEMS,
                                provider_id,
                                "thinking blocks",
                            )?;
                        }
                        thinking_blocks.insert(
                            index,
                            json!({ "type": "thinking", "thinking": "", "signature": "" }),
                        );
                    }
                    Some("redacted_thinking") => {
                        if !thinking_blocks.contains_key(&index) {
                            ensure_provider_stream_item_capacity(
                                thinking_blocks.len(),
                                1,
                                MAX_PROVIDER_REASONING_ITEMS,
                                provider_id,
                                "thinking blocks",
                            )?;
                        }
                        thinking_blocks.insert(index, block.clone());
                        validate_anthropic_thinking_blocks(provider_id, &thinking_blocks)?;
                    }
                    _ => {}
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
                            append_provider_stream_text(
                                &mut message,
                                &text,
                                MAX_PROVIDER_MESSAGE_BYTES,
                                provider_id,
                                "message text",
                            )?;
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
                        if !partial_calls.contains_key(&index) {
                            ensure_provider_stream_item_capacity(
                                partial_calls.len(),
                                1,
                                MAX_PROVIDER_TOOL_CALLS,
                                provider_id,
                                "tool calls",
                            )?;
                        }
                        let partial = partial_calls.entry(index).or_default();
                        append_provider_stream_text(
                            &mut partial.arguments,
                            &partial_json,
                            MAX_PROVIDER_TOOL_ARGUMENT_BYTES,
                            provider_id,
                            "tool arguments",
                        )?;
                        emit(ProviderStreamEvent::ToolDelta {
                            tool_call_id: partial.id.clone(),
                            tool_name: partial.name.clone(),
                            arguments_delta: partial_json,
                        })?;
                    }
                    "thinking_delta" => {
                        let index = value
                            .get("index")
                            .and_then(JsonValue::as_u64)
                            .unwrap_or_default() as usize;
                        let thinking = delta
                            .get("thinking")
                            .and_then(JsonValue::as_str)
                            .unwrap_or_default()
                            .to_string();
                        if !thinking.is_empty() {
                            append_anthropic_thinking_delta(
                                &mut thinking_blocks,
                                index,
                                "thinking",
                                &thinking,
                                provider_id,
                            )?;
                            emit(ProviderStreamEvent::ReasoningSummary(thinking))?;
                        }
                    }
                    "signature_delta" => {
                        let index = value
                            .get("index")
                            .and_then(JsonValue::as_u64)
                            .unwrap_or_default() as usize;
                        let signature = delta
                            .get("signature")
                            .and_then(JsonValue::as_str)
                            .unwrap_or_default();
                        if !signature.is_empty() {
                            append_anthropic_thinking_delta(
                                &mut thinking_blocks,
                                index,
                                "signature",
                                signature,
                                provider_id,
                            )?;
                        }
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(reason) = value
                    .get("delta")
                    .and_then(|delta| delta.get("stop_reason"))
                    .and_then(JsonValue::as_str)
                {
                    stop_reason = Some(reason.to_string());
                }
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
            // Anthropic can deliver a mid-stream `error` event (e.g. `overloaded_error`).
            // Surface it as a retryable failure instead of silently returning a truncated,
            // "successful" turn.
            "error" => {
                return Err(anthropic_stream_error(provider_id, &value));
            }
            "message_stop" => {}
            _ => {}
        }
    }
    validate_partial_tool_calls(provider_id, &partial_calls)?;
    validate_anthropic_thinking_blocks(provider_id, &thinking_blocks)?;
    ensure_provider_output_not_truncated(provider_id, stop_reason.as_deref())?;
    usage.billable_input_tokens = usage.input_tokens;
    usage.input_tokens = usage
        .input_tokens
        .saturating_add(usage.cache_read_tokens)
        .saturating_add(usage.cache_creation_tokens);
    usage.total_tokens = usage.input_tokens.saturating_add(usage.output_tokens);
    let usage = (usage.total_tokens > 0).then_some(usage);
    if let Some(usage) = usage.as_ref() {
        emit(ProviderStreamEvent::Usage(usage.clone()))?;
    }
    let (reasoning_content, reasoning_details) =
        collect_anthropic_thinking_outcome(thinking_blocks);
    finish_provider_turn(
        provider_id,
        message,
        reasoning_content,
        reasoning_details,
        partial_calls,
        usage,
    )
}

fn append_anthropic_thinking_delta(
    thinking_blocks: &mut BTreeMap<usize, JsonValue>,
    index: usize,
    field: &str,
    delta: &str,
    provider_id: &str,
) -> CommandResult<()> {
    if !thinking_blocks.contains_key(&index) {
        ensure_provider_stream_item_capacity(
            thinking_blocks.len(),
            1,
            MAX_PROVIDER_REASONING_ITEMS,
            provider_id,
            "thinking blocks",
        )?;
    }
    let block = thinking_blocks
        .entry(index)
        .or_insert_with(|| json!({ "type": "thinking", "thinking": "", "signature": "" }));
    if let Some(JsonValue::String(value)) = block.get_mut(field) {
        append_provider_stream_text(
            value,
            delta,
            MAX_PROVIDER_REASONING_BYTES,
            provider_id,
            "thinking data",
        )?;
    }
    validate_anthropic_thinking_blocks(provider_id, thinking_blocks)
}

fn validate_anthropic_thinking_blocks(
    provider_id: &str,
    thinking_blocks: &BTreeMap<usize, JsonValue>,
) -> CommandResult<()> {
    ensure_provider_stream_item_capacity(
        0,
        thinking_blocks.len(),
        MAX_PROVIDER_REASONING_ITEMS,
        provider_id,
        "thinking blocks",
    )?;
    let total_bytes = thinking_blocks
        .values()
        .try_fold(0_usize, |total, block| {
            json_string_storage_bytes(block).and_then(|bytes| total.checked_add(bytes))
        })
        .ok_or_else(|| {
            provider_stream_limit_error(
                provider_id,
                "response stream",
                "thinking data byte count overflowed",
            )
        })?;
    if total_bytes > MAX_PROVIDER_REASONING_BYTES {
        return Err(provider_stream_limit_error(
            provider_id,
            "response stream",
            &format!("thinking data exceeded {MAX_PROVIDER_REASONING_BYTES} bytes"),
        ));
    }
    Ok(())
}

fn validate_anthropic_response_thinking_blocks(
    provider_id: &str,
    thinking_blocks: &BTreeMap<usize, JsonValue>,
) -> CommandResult<()> {
    ensure_provider_response_item_capacity(
        0,
        thinking_blocks.len(),
        MAX_PROVIDER_REASONING_ITEMS,
        provider_id,
        "thinking blocks",
    )?;
    let total_bytes = thinking_blocks
        .values()
        .try_fold(0_usize, |total, block| {
            json_string_storage_bytes(block).and_then(|bytes| total.checked_add(bytes))
        })
        .ok_or_else(|| {
            provider_response_limit_error(
                provider_id,
                "response body",
                "thinking data byte count overflowed",
            )
        })?;
    if total_bytes > MAX_PROVIDER_REASONING_BYTES {
        return Err(provider_response_limit_error(
            provider_id,
            "response body",
            &format!("thinking data exceeded {MAX_PROVIDER_REASONING_BYTES} bytes"),
        ));
    }
    Ok(())
}

fn json_string_storage_bytes(value: &JsonValue) -> Option<usize> {
    match value {
        JsonValue::String(value) => Some(value.len()),
        JsonValue::Array(values) => values.iter().try_fold(0_usize, |total, value| {
            json_string_storage_bytes(value).and_then(|bytes| total.checked_add(bytes))
        }),
        JsonValue::Object(values) => values.iter().try_fold(0_usize, |total, (key, value)| {
            total.checked_add(key.len()).and_then(|total| {
                json_string_storage_bytes(value).and_then(|bytes| total.checked_add(bytes))
            })
        }),
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => Some(0),
    }
}

/// Fold streamed thinking blocks into the turn outcome: the concatenated
/// thinking text becomes reasoning_content (display/persistence) and the
/// ordered signed blocks become reasoning_details so the next Anthropic
/// request can replay them verbatim.
fn collect_anthropic_thinking_outcome(
    thinking_blocks: BTreeMap<usize, JsonValue>,
) -> (Option<String>, Option<JsonValue>) {
    if thinking_blocks.is_empty() {
        return (None, None);
    }
    let blocks: Vec<JsonValue> = thinking_blocks.into_values().collect();
    let reasoning_content = blocks
        .iter()
        .filter_map(|block| block.get("thinking").and_then(JsonValue::as_str))
        .filter(|thinking| !thinking.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (
        (!reasoning_content.is_empty()).then_some(reasoning_content),
        Some(JsonValue::Array(blocks)),
    )
}

fn parse_anthropic_json_response(
    provider_id: &str,
    response_text: &str,
    emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
) -> CommandResult<ProviderTurnOutcome> {
    if response_text.len() > MAX_PROVIDER_NON_STREAM_BODY_BYTES {
        return Err(provider_response_limit_error(
            provider_id,
            "response body",
            &format!("size exceeded {MAX_PROVIDER_NON_STREAM_BODY_BYTES} bytes"),
        ));
    }
    let value: JsonValue = serde_json::from_str(response_text).map_err(|error| {
        CommandError::retryable(
            "agent_provider_response_decode_failed",
            format!("Xero could not decode the {provider_id} response: {error}"),
        )
    })?;
    let mut message = String::new();
    let mut partial_calls = BTreeMap::new();
    let mut thinking_blocks = BTreeMap::<usize, JsonValue>::new();
    let content = value
        .get("content")
        .and_then(JsonValue::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    ensure_provider_response_item_capacity(
        0,
        content.len(),
        MAX_PROVIDER_ITEMS_PER_EVENT,
        provider_id,
        "content items in one response",
    )?;
    for (index, block) in content.iter().enumerate() {
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
                    append_provider_response_text(
                        &mut message,
                        &text,
                        MAX_PROVIDER_MESSAGE_BYTES,
                        provider_id,
                        "message text",
                    )?;
                    emit(ProviderStreamEvent::MessageDelta(text))?;
                }
            }
            "tool_use" => {
                ensure_provider_response_item_capacity(
                    partial_calls.len(),
                    1,
                    MAX_PROVIDER_TOOL_CALLS,
                    provider_id,
                    "tool calls",
                )?;
                let arguments = block
                    .get("input")
                    .map(JsonValue::to_string)
                    .unwrap_or_else(|| "{}".into());
                let mut bounded_arguments = String::new();
                append_provider_response_text(
                    &mut bounded_arguments,
                    &arguments,
                    MAX_PROVIDER_TOOL_ARGUMENT_BYTES,
                    provider_id,
                    "tool arguments",
                )?;
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
                        arguments: bounded_arguments,
                    },
                );
            }
            "thinking" => {
                ensure_provider_response_item_capacity(
                    thinking_blocks.len(),
                    1,
                    MAX_PROVIDER_REASONING_ITEMS,
                    provider_id,
                    "thinking blocks",
                )?;
                thinking_blocks.insert(index, block.clone());
                validate_anthropic_response_thinking_blocks(provider_id, &thinking_blocks)?;
                if let Some(thinking) = block.get("thinking").and_then(JsonValue::as_str) {
                    if !thinking.is_empty() {
                        emit(ProviderStreamEvent::ReasoningSummary(thinking.to_string()))?;
                    }
                }
            }
            "redacted_thinking" => {
                ensure_provider_response_item_capacity(
                    thinking_blocks.len(),
                    1,
                    MAX_PROVIDER_REASONING_ITEMS,
                    provider_id,
                    "thinking blocks",
                )?;
                thinking_blocks.insert(index, block.clone());
                validate_anthropic_response_thinking_blocks(provider_id, &thinking_blocks)?;
            }
            _ => {}
        }
    }
    validate_partial_tool_calls(provider_id, &partial_calls)?;
    validate_anthropic_response_thinking_blocks(provider_id, &thinking_blocks)?;
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
            input_tokens: input_tokens
                .saturating_add(cache_read_tokens)
                .saturating_add(cache_creation_tokens),
            billable_input_tokens: input_tokens,
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
    ensure_provider_output_not_truncated(
        provider_id,
        value.get("stop_reason").and_then(JsonValue::as_str),
    )?;
    let (reasoning_content, reasoning_details) =
        collect_anthropic_thinking_outcome(thinking_blocks);
    finish_provider_turn(
        provider_id,
        message,
        reasoning_content,
        reasoning_details,
        partial_calls,
        usage,
    )
}

/// Fail when a provider stopped generating because it hit the output-token limit. Without
/// this, the harness accepts the truncated text (or a mid-object tool-call JSON) as the final
/// answer. `max_tokens` is Anthropic's reason; `length` is the OpenAI-family equivalent.
fn ensure_provider_output_not_truncated(
    provider_id: &str,
    stop_reason: Option<&str>,
) -> CommandResult<()> {
    if matches!(stop_reason, Some("max_tokens") | Some("length")) {
        return Err(CommandError::user_fixable(
            "agent_provider_output_truncated",
            format!(
                "The {provider_id} response stopped at the output token limit before finishing (stop reason `{}`). Ask for a shorter response or raise the model's max output tokens.",
                stop_reason.unwrap_or_default()
            ),
        ));
    }
    Ok(())
}

/// Convert an Anthropic mid-stream `error` event into a retryable failure. These are
/// transient conditions (e.g. `overloaded_error`) that must not be dropped, which would leave
/// the loop treating a partial stream as a complete turn.
fn anthropic_stream_error(provider_id: &str, value: &JsonValue) -> CommandError {
    let error_node = value.get("error");
    let error_type = error_node
        .and_then(|error| error.get("type"))
        .and_then(JsonValue::as_str)
        .unwrap_or("error");
    let message = error_node
        .and_then(|error| error.get("message"))
        .and_then(JsonValue::as_str)
        .unwrap_or("The provider reported a streaming error.");
    CommandError::retryable(
        "agent_provider_stream_error",
        format!(
            "The {provider_id} stream returned an error ({error_type}): {}",
            redact_provider_error_body(message)
        ),
    )
}

fn finish_provider_turn(
    provider_id: &str,
    message: String,
    reasoning_content: Option<String>,
    reasoning_details: Option<JsonValue>,
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
            parse_tool_arguments(provider_id, &name, &partial.arguments)?
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
        Ok(ProviderTurnOutcome::Complete {
            message,
            reasoning_content,
            reasoning_details,
            usage,
        })
    } else {
        Ok(ProviderTurnOutcome::ToolCalls {
            message,
            reasoning_content,
            reasoning_details,
            tool_calls,
            usage,
        })
    }
}

fn parse_tool_arguments(
    provider_id: &str,
    tool_name: &str,
    arguments: &str,
) -> CommandResult<JsonValue> {
    match serde_json::from_str(arguments) {
        Ok(value) => Ok(value),
        Err(original_error) if provider_id == XAI_PROVIDER_ID => {
            let decoded = decode_basic_html_entities(arguments);
            if decoded != arguments {
                if let Ok(value) = serde_json::from_str(&decoded) {
                    return Ok(value);
                }
            }
            Err(tool_arguments_decode_error(
                provider_id,
                tool_name,
                original_error,
            ))
        }
        Err(error) => Err(tool_arguments_decode_error(provider_id, tool_name, error)),
    }
}

fn tool_arguments_decode_error(
    provider_id: &str,
    tool_name: &str,
    error: serde_json::Error,
) -> CommandError {
    CommandError::user_fixable(
        "agent_provider_tool_arguments_invalid",
        format!(
            "Xero could not decode {provider_id} tool call `{tool_name}` arguments as JSON: {error}"
        ),
    )
}

fn decode_basic_html_entities(value: &str) -> String {
    let mut decoded = value.to_owned();
    for _ in 0..2 {
        let next = decoded
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#34;", "\"")
            .replace("&#x22;", "\"")
            .replace("&#X22;", "\"")
            .replace("&apos;", "'")
            .replace("&#39;", "'")
            .replace("&#x27;", "'")
            .replace("&#X27;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">");
        if next == decoded {
            break;
        }
        decoded = next;
    }
    decoded
}

fn send_provider_json_request<F>(provider_id: &str, mut build: F) -> CommandResult<BlockingResponse>
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

fn ensure_success(
    provider_id: &str,
    mut response: BlockingResponse,
) -> CommandResult<BlockingResponse> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let status_code = status.as_u16();
    let body = read_provider_error_text_bounded(
        &mut response,
        MAX_PROVIDER_ERROR_BODY_BYTES,
        provider_id,
        "error response body",
    );
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
    cancellation: &AgentRunCancellationToken,
) -> CommandResult<std::process::ExitStatus> {
    let started = std::time::Instant::now();
    loop {
        if let Err(error) = cancellation.check_cancelled() {
            let _ = terminate_process_tree(child);
            return Err(error);
        }
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

impl Default for XaiResponsesProviderConfig {
    fn default() -> Self {
        Self {
            provider_id: XAI_PROVIDER_ID.into(),
            model_id: XAI_DEFAULT_MODEL_ID.into(),
            base_url: XAI_API_BASE_URL.into(),
            bearer_token: String::new(),
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
fn _known_provider_ids() -> [&'static str; 12] {
    [
        OPENAI_CODEX_PROVIDER_ID,
        OPENAI_API_PROVIDER_ID,
        OPENROUTER_PROVIDER_ID,
        DEEPSEEK_PROVIDER_ID,
        XAI_PROVIDER_ID,
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
    use crate::runtime::{agent_core::builtin_tool_descriptors, AUTONOMOUS_TOOL_PATCH};
    use std::{
        net::{TcpListener, TcpStream},
        sync::mpsc,
        thread,
        time::Instant,
    };

    #[test]
    fn openai_codex_gpt_5_6_clamps_minimal_reasoning_to_low() {
        assert_eq!(
            clamp_openai_codex_reasoning_effort("gpt-5.6-sol", "minimal"),
            "low"
        );
    }

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
            output_allowance: ProviderTurnOutputAllowance::unified(4_096)
                .expect("test output allowance"),
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
                    auto_compact_enabled: true,
                    revision: 1,
                    applied_at: "2026-04-24T00:00:00Z".into(),
                },
                pending: None,
            },
        }
    }

    fn test_provider_preflight(
        provider_id: &str,
        model_id: &str,
        context_window_tokens: u64,
        max_output_tokens: Option<u64>,
    ) -> xero_agent_core::ProviderPreflightSnapshot {
        let mut preflight = crate::provider_preflight::static_provider_preflight_snapshot(
            provider_id,
            model_id,
            xero_agent_core::ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
        );
        let limits = &mut preflight.capabilities.capabilities.context_limits;
        limits.context_window_tokens = Some(context_window_tokens);
        limits.max_output_tokens = max_output_tokens;
        limits.source = "live_probe".into();
        limits.confidence = "high".into();
        preflight
    }

    #[test]
    fn resolved_output_limits_reach_supported_wire_families() {
        let small_preflight =
            test_provider_preflight(OPENAI_API_PROVIDER_ID, "gpt-5.4", 128_000, Some(1_024));
        let small = resolve_provider_turn_budget(
            OPENAI_API_PROVIDER_ID,
            "gpt-5.4",
            Some(&small_preflight),
            None,
        )
        .expect("small output budget");
        let mut request = test_request();
        request.output_allowance = small.output_allowance;

        let responses = openai_responses_request_body(OPENAI_API_PROVIDER_ID, "gpt-5.4", &request)
            .expect("OpenAI Responses body");
        let openai_chat = openai_chat_request_body(OPENAI_API_PROVIDER_ID, "gpt-5.4", &request)
            .expect("OpenAI chat body");
        let compatible_chat =
            openai_chat_request_body(OPENROUTER_PROVIDER_ID, "openai/gpt-5.4", &request)
                .expect("compatible chat body");

        assert_eq!(responses["max_output_tokens"], 1_024);
        assert_eq!(openai_chat["max_completion_tokens"], 1_024);
        assert_eq!(compatible_chat["max_tokens"], 1_024);

        let large_preflight =
            test_provider_preflight(XAI_PROVIDER_ID, XAI_DEFAULT_MODEL_ID, 256_000, Some(65_536));
        let large = resolve_provider_turn_budget(
            XAI_PROVIDER_ID,
            XAI_DEFAULT_MODEL_ID,
            Some(&large_preflight),
            None,
        )
        .expect("large output budget");
        request.output_allowance = large.output_allowance;

        let xai = xai_responses_request_body(XAI_PROVIDER_ID, XAI_DEFAULT_MODEL_ID, &request)
            .expect("xAI Responses body");

        assert_eq!(xai["max_output_tokens"], 65_536);
    }

    #[test]
    fn openai_codex_body_omits_unsupported_max_output_tokens() {
        let mut request = test_request();
        request.output_allowance =
            ProviderTurnOutputAllowance::unified(65_536).expect("output allowance");

        let body = openai_codex_responses_request_body(
            OPENAI_CODEX_PROVIDER_ID,
            "gpt-5.6-luna",
            &request,
            None,
        )
        .expect("Codex Responses body");

        assert!(body.get("max_output_tokens").is_none());
    }

    #[test]
    fn anthropic_reasoning_and_visible_output_share_the_advertised_maximum() {
        let preflight = test_provider_preflight(
            ANTHROPIC_PROVIDER_ID,
            "claude-sonnet-4-5",
            200_000,
            Some(16_384),
        );
        let effort = ProviderModelThinkingEffortDto::High;
        let resolved = resolve_provider_turn_budget(
            ANTHROPIC_PROVIDER_ID,
            "claude-sonnet-4-5",
            Some(&preflight),
            Some(&effort),
        )
        .expect("Anthropic reasoning budget");

        assert_eq!(resolved.output_allowance.max_output_tokens, 16_384);
        assert_eq!(resolved.output_allowance.reasoning_tokens, Some(8_192));
        assert_eq!(resolved.output_allowance.visible_output_tokens, Some(8_192));

        let mut request = test_request();
        request.controls.active.thinking_effort = Some(effort);
        request.output_allowance = resolved.output_allowance;
        let body = anthropic_request_body(
            Some("claude-sonnet-4-5"),
            ANTHROPIC_API_VERSION,
            &request,
            true,
        )
        .expect("Anthropic body");

        assert_eq!(body["thinking"]["budget_tokens"], 8_192);
        assert_eq!(body["max_tokens"], 16_384);
    }

    #[test]
    fn fake_provider_accepts_static_preflight_without_external_limits() {
        let synthetic = crate::provider_preflight::static_provider_preflight_snapshot(
            OPENAI_CODEX_PROVIDER_ID,
            OPENAI_CODEX_PROVIDER_ID,
            xero_agent_core::ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
        );
        let synthetic_budget = resolve_provider_turn_budget(
            OPENAI_CODEX_PROVIDER_ID,
            OPENAI_CODEX_PROVIDER_ID,
            Some(&synthetic),
            None,
        )
        .expect("fake provider has no external wire capability requirement");
        assert_eq!(synthetic_budget.output_allowance.max_output_tokens, 4_096);
    }

    #[test]
    fn provider_turn_budget_fails_closed_for_missing_or_inconsistent_limits() {
        let missing = test_provider_preflight(OPENAI_API_PROVIDER_ID, "gpt-5.4", 128_000, None);
        let missing_error =
            resolve_provider_turn_budget(OPENAI_API_PROVIDER_ID, "gpt-5.4", Some(&missing), None)
                .expect_err("missing output limit must fail closed");
        assert_eq!(missing_error.code, "agent_provider_output_limit_missing");

        let inconsistent =
            test_provider_preflight(OPENAI_API_PROVIDER_ID, "gpt-5.4", 4_096, Some(8_192));
        let inconsistent_error = resolve_provider_turn_budget(
            OPENAI_API_PROVIDER_ID,
            "gpt-5.4",
            Some(&inconsistent),
            None,
        )
        .expect_err("output limit above context window must fail closed");
        assert_eq!(
            inconsistent_error.code,
            "agent_provider_output_limit_inconsistent"
        );

        let constrained =
            test_provider_preflight(ANTHROPIC_PROVIDER_ID, "claude-small", 32_000, Some(8_192));
        let reasoning_error = resolve_provider_turn_budget(
            ANTHROPIC_PROVIDER_ID,
            "claude-small",
            Some(&constrained),
            Some(&ProviderModelThinkingEffortDto::High),
        )
        .expect_err("thinking budget that consumes output maximum must fail closed");
        assert_eq!(
            reasoning_error.code,
            "agent_provider_reasoning_output_limit_exceeded"
        );
    }

    fn test_compaction_provenance() -> ProviderContextProvenance {
        ProviderContextProvenance {
            source_kind: ProviderContextSourceKind::Compaction,
            source_id: "compact-1".into(),
            source_hash: "sha256:compact".into(),
        }
    }

    #[test]
    fn bounded_sse_reader_rejects_a_line_over_the_configured_limit() {
        let input = format!("data: {}\n", "x".repeat(32));
        let mut reader = BoundedSseReader::new(
            std::io::Cursor::new(input),
            ProviderStreamLimits {
                max_line_bytes: 16,
                max_total_bytes: 1_024,
                max_lines: 16,
                max_events: 16,
            },
        );

        let error = reader
            .next_data("test-provider", "test stream")
            .expect_err("oversized SSE line must be rejected");

        assert_eq!(error.code, "agent_provider_stream_limit_exceeded");
    }

    #[test]
    fn bounded_stream_append_rejects_text_over_the_configured_limit() {
        let mut text = "1234".to_string();

        let error =
            append_provider_stream_text(&mut text, "56", 5, "test-provider", "message text")
                .expect_err("oversized accumulated text must be rejected");

        assert_eq!(error.code, "agent_provider_stream_limit_exceeded");
    }

    #[test]
    fn bounded_provider_text_reader_rejects_a_body_over_the_byte_limit() {
        let mut input = std::io::Cursor::new(b"123456789".to_vec());

        let error = read_provider_text_bounded(&mut input, 8, "test-provider", "response body")
            .expect_err("body larger than its byte limit must be rejected");

        assert_eq!(error.code, "agent_provider_response_limit_exceeded");
    }

    #[test]
    fn bounded_provider_error_reader_omits_a_body_over_the_byte_limit() {
        let mut input = std::io::Cursor::new(b"123456789".to_vec());

        let text =
            read_provider_error_text_bounded(&mut input, 8, "test-provider", "error response body");

        assert_eq!(
            text,
            "provider error body exceeded the safe read limit and was omitted."
        );
    }

    #[test]
    fn anthropic_json_response_rejects_more_than_the_non_stream_item_limit() {
        let content = (0..=MAX_PROVIDER_ITEMS_PER_EVENT)
            .map(|_| json!({ "type": "unknown" }))
            .collect::<Vec<_>>();
        let response = json!({ "content": content }).to_string();
        let mut emit = |_| Ok(());

        let error = parse_anthropic_json_response("test-provider", &response, &mut emit)
            .expect_err("oversized non-stream content array must be rejected");

        assert_eq!(error.code, "agent_provider_response_limit_exceeded");
    }

    #[test]
    fn anthropic_json_response_rejects_more_than_the_tool_call_limit() {
        let content = (0..=MAX_PROVIDER_TOOL_CALLS)
            .map(|index| {
                json!({
                    "type": "tool_use",
                    "id": format!("call-{index}"),
                    "name": "read",
                    "input": { "path": format!("src/{index}.rs") }
                })
            })
            .collect::<Vec<_>>();
        let response = json!({ "content": content }).to_string();
        let mut emit = |_| Ok(());

        let error = parse_anthropic_json_response("test-provider", &response, &mut emit)
            .expect_err("too many non-stream tool calls must be rejected");

        assert_eq!(error.code, "agent_provider_response_limit_exceeded");
    }

    #[test]
    fn bounded_sse_reader_rejects_a_stream_over_the_total_byte_limit() {
        let mut reader = BoundedSseReader::new(
            std::io::Cursor::new("data: a\ndata: b\n"),
            ProviderStreamLimits {
                max_line_bytes: 64,
                max_total_bytes: 8,
                max_lines: 16,
                max_events: 16,
            },
        );
        assert_eq!(
            reader
                .next_data("test-provider", "test stream")
                .expect("first event"),
            Some("a".into())
        );

        let error = reader
            .next_data("test-provider", "test stream")
            .expect_err("stream over its total byte budget must be rejected");

        assert_eq!(error.code, "agent_provider_stream_limit_exceeded");
    }

    #[test]
    fn bounded_sse_reader_rejects_more_than_the_configured_event_count() {
        let mut reader = BoundedSseReader::new(
            std::io::Cursor::new("data: a\ndata: b\n"),
            ProviderStreamLimits {
                max_line_bytes: 64,
                max_total_bytes: 1_024,
                max_lines: 16,
                max_events: 1,
            },
        );
        assert_eq!(
            reader
                .next_data("test-provider", "test stream")
                .expect("first event"),
            Some("a".into())
        );

        let error = reader
            .next_data("test-provider", "test stream")
            .expect_err("event count over its budget must be rejected");

        assert_eq!(error.code, "agent_provider_stream_limit_exceeded");
    }

    #[test]
    fn provider_stream_item_capacity_rejects_more_than_the_configured_items() {
        let error = ensure_provider_stream_item_capacity(4, 2, 5, "test-provider", "tool calls")
            .expect_err("item count over its budget must be rejected");

        assert_eq!(error.code, "agent_provider_stream_limit_exceeded");
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

        let deepseek = openai_chat_request_body(DEEPSEEK_PROVIDER_ID, "deepseek-v4-pro", &request)
            .expect("deepseek body");
        assert_eq!(deepseek["stream_options"]["include_usage"], true);
        assert_eq!(deepseek["thinking"]["type"], "enabled");
        assert!(deepseek.get("reasoning").is_none());

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
    fn provider_context_estimate_uses_adapter_wire_request_shape() {
        let adapter = OpenAiCompatibleAdapter::new(OpenAiCompatibleProviderConfig {
            provider_id: OPENROUTER_PROVIDER_ID.into(),
            model_id: "openai/gpt-4.1-mini".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            api_key: Some("test-key".into()),
            api_version: None,
            timeout_ms: 1_000,
        })
        .expect("adapter");

        let estimate = adapter
            .estimate_context_tokens(&test_request())
            .expect("estimate");

        assert!(estimate.tokens > 0);
        assert_eq!(estimate.source, SessionContextEstimateSourceDto::Heuristic);
        assert_eq!(
            estimate.confidence,
            SessionContextEstimateConfidenceDto::Low
        );
        assert_eq!(
            estimate.counted_shape,
            "openai_chat_completions_wire_request"
        );
    }

    #[test]
    fn provider_wire_estimate_omits_inline_image_base64_payloads() {
        let image_payload = "A".repeat(600_000);
        let body = json!({
            "model": "gpt-5.5",
            "input": [{
                "role": "user",
                "content": [
                    {
                        "type": "input_image",
                        "image_url": format!("data:image/png;base64,{image_payload}"),
                        "detail": "auto"
                    },
                    {
                        "type": "input_text",
                        "text": "Use this screenshot to inspect the mobile menu."
                    }
                ]
            }]
        });

        let estimate = estimate_provider_wire_context_tokens(
            OPENAI_CODEX_PROVIDER_ID,
            "gpt-5.5",
            "openai_codex_responses_wire_request",
            body,
        )
        .expect("estimate provider wire context");

        assert!(
            estimate.tokens < 10_000,
            "image base64 transport bytes should not dominate prompt estimate: {estimate:?}"
        );
        assert!(estimate.diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("Omitted 1 inline image data URL payload")
                && diagnostic.contains("600000 encoded byte")
        }));
    }

    #[test]
    fn deepseek_body_uses_thinking_effort_and_replays_reasoning_content() {
        let mut request = test_request();
        request.controls.active.thinking_effort = Some(ProviderModelThinkingEffortDto::XHigh);
        request.messages.push(ProviderMessage::Assistant {
            content: "I will read the file.".into(),
            reasoning_content: Some("tool call rationale".into()),
            reasoning_details: None,
            tool_calls: vec![AgentToolCall {
                tool_call_id: "call-1".into(),
                tool_name: "read".into(),
                input: json!({ "path": "src/lib.rs" }),
            }],
        });

        let body = openai_chat_request_body(DEEPSEEK_PROVIDER_ID, "deepseek-v4-pro", &request)
            .expect("deepseek body");
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["reasoning_effort"], "max");
        assert!(body.get("reasoning").is_none());
        assert_eq!(
            body["messages"][2]["reasoning_content"],
            "tool call rationale"
        );
    }

    #[test]
    fn openrouter_replays_reasoning_details_without_sending_deepseek_thinking() {
        let mut request = test_request();
        request.controls.active.thinking_effort = Some(ProviderModelThinkingEffortDto::Medium);
        request.messages.push(ProviderMessage::Assistant {
            content: "I will inspect context.".into(),
            reasoning_content: Some("brief rationale".into()),
            reasoning_details: Some(json!([
                { "type": "reasoning.text", "text": "opaque provider detail" }
            ])),
            tool_calls: Vec::new(),
        });

        let body =
            openai_chat_request_body(OPENROUTER_PROVIDER_ID, "deepseek/deepseek-v4-pro", &request)
                .expect("openrouter body");
        assert_eq!(body["reasoning"]["effort"], "high");
        assert!(body.get("thinking").is_none());
        assert_eq!(body["messages"][2]["reasoning_content"], "brief rationale");
        assert!(body["messages"][2]["reasoning_details"].is_array());
    }

    #[test]
    fn openai_compatible_chat_body_serializes_openrouter_attachments() {
        let dir = tempfile::tempdir().expect("temp dir");
        let image_path = dir.path().join("snap.png");
        std::fs::write(&image_path, b"\x89PNG\r\n\x1a\nfake-image-bytes")
            .expect("write image fixture");
        let pdf_path = dir.path().join("notes.pdf");
        std::fs::write(&pdf_path, b"%PDF-1.4 fake-pdf-bytes").expect("write pdf fixture");
        let text_path = dir.path().join("note.md");
        std::fs::write(&text_path, b"# heading\nbody").expect("write text fixture");

        let mut request = test_request();
        request.messages = vec![ProviderMessage::User {
            content: "read the attachments".into(),
            attachments: vec![
                MessageAttachment {
                    kind: MessageAttachmentKind::Image,
                    absolute_path: image_path,
                    media_type: "image/png".into(),
                    original_name: "snap.png".into(),
                    size_bytes: 10,
                    width: None,
                    height: None,
                },
                MessageAttachment {
                    kind: MessageAttachmentKind::Document,
                    absolute_path: pdf_path,
                    media_type: "application/pdf".into(),
                    original_name: "notes.pdf".into(),
                    size_bytes: 20,
                    width: None,
                    height: None,
                },
                MessageAttachment {
                    kind: MessageAttachmentKind::Text,
                    absolute_path: text_path,
                    media_type: "text/markdown".into(),
                    original_name: "note.md".into(),
                    size_bytes: 12,
                    width: None,
                    height: None,
                },
            ],
        }];

        let body = openai_chat_request_body(OPENROUTER_PROVIDER_ID, "x-ai/grok-4.3", &request)
            .expect("openrouter chat body");
        let content = body["messages"][1]["content"]
            .as_array()
            .expect("multipart chat content");

        assert!(content.iter().any(|block| {
            block["type"] == "image_url" && block["image_url"]["detail"] == "auto"
        }));
        assert!(content.iter().any(|block| block["type"] == "file"));
        assert!(content.iter().any(|block| block["type"] == "text"
            && block["text"]
                .as_str()
                .is_some_and(|text| text.contains("note.md"))));
    }

    #[test]
    fn openai_responses_body_serializes_image_file_and_text_attachments() {
        let dir = tempfile::tempdir().expect("temp dir");
        let image_path = dir.path().join("snap.png");
        std::fs::write(&image_path, b"\x89PNG\r\n\x1a\nfake-image-bytes")
            .expect("write image fixture");
        let pdf_path = dir.path().join("notes.pdf");
        std::fs::write(&pdf_path, b"%PDF-1.4 fake-pdf-bytes").expect("write pdf fixture");
        let text_path = dir.path().join("note.md");
        std::fs::write(&text_path, b"# heading\nbody").expect("write text fixture");

        let mut request = test_request();
        request.messages = vec![ProviderMessage::User {
            content: "read the attachments".into(),
            attachments: vec![
                MessageAttachment {
                    kind: MessageAttachmentKind::Image,
                    absolute_path: image_path,
                    media_type: "image/png".into(),
                    original_name: "snap.png".into(),
                    size_bytes: 10,
                    width: None,
                    height: None,
                },
                MessageAttachment {
                    kind: MessageAttachmentKind::Document,
                    absolute_path: pdf_path,
                    media_type: "application/pdf".into(),
                    original_name: "notes.pdf".into(),
                    size_bytes: 20,
                    width: None,
                    height: None,
                },
                MessageAttachment {
                    kind: MessageAttachmentKind::Text,
                    absolute_path: text_path,
                    media_type: "text/markdown".into(),
                    original_name: "note.md".into(),
                    size_bytes: 12,
                    width: None,
                    height: None,
                },
            ],
        }];

        let body = openai_responses_request_body(OPENAI_API_PROVIDER_ID, "gpt-5.4", &request)
            .expect("responses body");
        let content = body["input"][0]["content"]
            .as_array()
            .expect("multipart responses content");

        assert!(content
            .iter()
            .any(|block| { block["type"] == "input_image" && block["detail"] == "auto" }));
        assert!(content.iter().any(|block| block["type"] == "input_file"));
        assert!(content.iter().any(|block| block["type"] == "input_text"
            && block["text"]
                .as_str()
                .is_some_and(|text| text.contains("note.md"))));
    }

    #[test]
    fn openai_responses_inputs_replay_encrypted_reasoning_before_assistant_output() {
        let reasoning_item = json!({
            "type": "reasoning",
            "id": "rs_123",
            "encrypted_content": "opaque-encrypted-state",
            "summary": [{
                "type": "summary_text",
                "text": "I should inspect the file."
            }]
        });
        let mut request = test_request();
        request.messages.push(ProviderMessage::Assistant {
            content: "I will inspect it.".into(),
            reasoning_content: None,
            reasoning_details: Some(json!([reasoning_item.clone()])),
            tool_calls: vec![AgentToolCall {
                tool_call_id: "call-1".into(),
                tool_name: "read".into(),
                input: json!({ "path": "src/lib.rs" }),
            }],
        });

        let responses_input = openai_response_input(&request).expect("Responses input");
        let codex_input = openai_codex_response_input(&request).expect("Codex input");

        assert_eq!(responses_input[1], reasoning_item);
        assert_eq!(responses_input[2]["role"], "assistant");
        assert_eq!(responses_input[3]["type"], "function_call");
        assert_eq!(codex_input[1], reasoning_item);
        assert_eq!(codex_input[2]["role"], "assistant");
        assert_eq!(codex_input[3]["type"], "function_call");
    }

    #[test]
    fn openai_responses_inputs_whitelist_reasoning_and_drop_foreign_detail_shapes() {
        let expected = json!({
            "type": "reasoning",
            "id": "rs_valid",
            "encrypted_content": "encrypted-valid"
        });
        let mut request = test_request();
        request.messages.push(ProviderMessage::Assistant {
            content: String::new(),
            reasoning_content: None,
            reasoning_details: Some(json!([
                { "type": "reasoning.text", "text": "OpenRouter detail" },
                { "type": "thinking", "thinking": "Anthropic detail", "signature": "sig" },
                { "type": "reasoning", "encrypted_content": "missing-id" },
                { "type": "reasoning", "id": "rs_missing_encrypted" },
                {
                    "type": "reasoning",
                    "id": "rs_valid",
                    "encrypted_content": "encrypted-valid",
                    "summary": [{ "type": "foreign_summary", "text": "drop me" }],
                    "foreign_field": "drop me"
                }
            ])),
            tool_calls: Vec::new(),
        });

        let responses_input = openai_response_input(&request).expect("Responses input");
        let codex_input = openai_codex_response_input(&request).expect("Codex input");
        let xai_input = xai_response_input(&request).expect("xAI input");

        assert_eq!(responses_input.len(), 2);
        assert_eq!(responses_input[1], expected);
        assert_eq!(codex_input.len(), 2);
        assert_eq!(codex_input[1], expected);
        assert_eq!(xai_input.len(), 1);
    }

    #[test]
    fn openai_responses_inputs_preserve_developer_and_assistant_context_roles() {
        let mut request = test_request();
        request.messages = vec![
            ProviderMessage::Developer {
                content: "trusted runtime gate".into(),
            },
            ProviderMessage::AssistantContext {
                content: "prior assistant summary".into(),
                provenance: test_compaction_provenance(),
            },
            ProviderMessage::User {
                content: "actual user request".into(),
                attachments: Vec::new(),
            },
        ];

        let responses = openai_response_input(&request).expect("Responses input");
        let codex = openai_codex_response_input(&request).expect("Codex input");

        assert_eq!(
            responses[0],
            json!({
                "role": "developer",
                "content": "trusted runtime gate"
            })
        );
        assert_eq!(
            responses[1],
            json!({
                "role": "assistant",
                "content": "prior assistant summary"
            })
        );
        assert_eq!(responses[2]["role"], "user");
        assert_eq!(codex[0]["role"], "developer");
        assert_eq!(codex[0]["content"][0]["type"], "input_text");
        assert_eq!(codex[1]["role"], "assistant");
        assert_eq!(codex[1]["content"][0]["text"], "prior assistant summary");
        assert_eq!(codex[2]["role"], "user");
    }

    #[test]
    fn compatible_chat_and_xai_fold_developer_text_without_rewriting_user_messages() {
        let mut request = test_request();
        request.messages = vec![
            ProviderMessage::Developer {
                content: "trusted runtime gate".into(),
            },
            ProviderMessage::AssistantContext {
                content: "prior assistant summary".into(),
                provenance: test_compaction_provenance(),
            },
            ProviderMessage::User {
                content: "actual user request".into(),
                attachments: Vec::new(),
            },
        ];

        let chat = openai_chat_request_body(OPENROUTER_PROVIDER_ID, "model", &request)
            .expect("compatible chat body");
        let xai = xai_responses_request_body(XAI_PROVIDER_ID, XAI_DEFAULT_MODEL_ID, &request)
            .expect("xAI body");

        assert_eq!(chat["messages"][0]["role"], "system");
        assert_eq!(
            chat["messages"][0]["content"],
            "system\n\ntrusted runtime gate"
        );
        assert_eq!(chat["messages"][1]["role"], "assistant");
        assert_eq!(chat["messages"][2]["role"], "user");
        assert_eq!(chat["messages"][2]["content"], "actual user request");
        assert_eq!(xai["instructions"], "system\n\ntrusted runtime gate");
        assert_eq!(xai["input"][0]["role"], "assistant");
        assert_eq!(xai["input"][1]["role"], "user");
        assert_eq!(xai["input"][1]["content"], "actual user request");
    }

    #[test]
    fn anthropic_folds_developer_text_and_prepends_user_before_leading_assistant_context() {
        let mut request = test_request();
        request.output_allowance =
            ProviderTurnOutputAllowance::split(4_096, 0, 4_096).expect("Anthropic test allowance");
        request.messages = vec![
            ProviderMessage::Developer {
                content: "trusted runtime gate".into(),
            },
            ProviderMessage::AssistantContext {
                content: "prior assistant summary".into(),
                provenance: test_compaction_provenance(),
            },
            ProviderMessage::User {
                content: "actual user request".into(),
                attachments: Vec::new(),
            },
        ];

        let native = anthropic_request_body(
            Some("claude-sonnet-4-5"),
            ANTHROPIC_API_VERSION,
            &request,
            true,
        )
        .expect("native Anthropic body");
        let bedrock = anthropic_request_body(None, BEDROCK_ANTHROPIC_VERSION, &request, false)
            .expect("Bedrock body");

        assert_eq!(
            native["system"][0]["text"],
            "system\n\ntrusted runtime gate"
        );
        assert_eq!(native["messages"][0]["role"], "user");
        assert_eq!(
            native["messages"][0]["content"][0]["text"],
            ANTHROPIC_ASSISTANT_CONTEXT_PREAMBLE
        );
        assert_eq!(native["messages"][1]["role"], "assistant");
        assert_eq!(
            native["messages"][1]["content"][0]["text"],
            "prior assistant summary"
        );
        assert_eq!(native["messages"][2]["role"], "user");
        assert_eq!(
            native["messages"][2]["content"][0]["text"],
            "actual user request"
        );
        assert_eq!(bedrock["system"], "system\n\ntrusted runtime gate");
    }

    #[test]
    fn xai_responses_body_serializes_image_attachments() {
        let dir = tempfile::tempdir().expect("temp dir");
        let image_path = dir.path().join("snap.png");
        std::fs::write(&image_path, b"\x89PNG\r\n\x1a\nfake-image-bytes")
            .expect("write image fixture");

        let mut request = test_request();
        request.messages = vec![ProviderMessage::User {
            content: "where is this in the code?".into(),
            attachments: vec![MessageAttachment {
                kind: MessageAttachmentKind::Image,
                absolute_path: image_path,
                media_type: "image/png".into(),
                original_name: "browser-sketch.png".into(),
                size_bytes: 10,
                width: Some(800),
                height: Some(600),
            }],
        }];

        let body = xai_responses_request_body(XAI_PROVIDER_ID, "grok-4.3", &request)
            .expect("xAI responses body");
        let content = body["input"][0]["content"]
            .as_array()
            .expect("xAI multipart content");

        assert!(content.iter().any(|block| {
            block["type"] == "input_image"
                && block["detail"] == "auto"
                && block["image_url"]
                    .as_str()
                    .is_some_and(|url| url.starts_with("data:image/png;base64,"))
        }));
    }

    #[test]
    fn openai_codex_responses_body_serializes_image_file_and_text_attachments() {
        let dir = tempfile::tempdir().expect("temp dir");
        let image_path = dir.path().join("snap.png");
        std::fs::write(&image_path, b"\x89PNG\r\n\x1a\nfake-image-bytes")
            .expect("write image fixture");
        let pdf_path = dir.path().join("notes.pdf");
        std::fs::write(&pdf_path, b"%PDF-1.4 fake-pdf-bytes").expect("write pdf fixture");
        let text_path = dir.path().join("note.md");
        std::fs::write(&text_path, b"# heading\nbody").expect("write text fixture");

        let mut request = test_request();
        request.messages = vec![ProviderMessage::User {
            content: "read the attachments".into(),
            attachments: vec![
                MessageAttachment {
                    kind: MessageAttachmentKind::Image,
                    absolute_path: image_path,
                    media_type: "image/png".into(),
                    original_name: "snap.png".into(),
                    size_bytes: 10,
                    width: None,
                    height: None,
                },
                MessageAttachment {
                    kind: MessageAttachmentKind::Document,
                    absolute_path: pdf_path,
                    media_type: "application/pdf".into(),
                    original_name: "notes.pdf".into(),
                    size_bytes: 20,
                    width: None,
                    height: None,
                },
                MessageAttachment {
                    kind: MessageAttachmentKind::Text,
                    absolute_path: text_path,
                    media_type: "text/markdown".into(),
                    original_name: "note.md".into(),
                    size_bytes: 12,
                    width: None,
                    height: None,
                },
            ],
        }];

        let body = openai_codex_responses_request_body(
            OPENAI_CODEX_PROVIDER_ID,
            "gpt-5.5",
            &request,
            None,
        )
        .expect("codex responses body");
        let content = body["input"][0]["content"]
            .as_array()
            .expect("multipart codex responses content");

        assert!(content
            .iter()
            .any(|block| { block["type"] == "input_image" && block["detail"] == "auto" }));
        assert!(content.iter().any(|block| block["type"] == "input_file"));
        assert!(content.iter().any(|block| block["type"] == "input_text"
            && block["text"]
                .as_str()
                .is_some_and(|text| text.contains("note.md"))));
    }

    #[test]
    fn openai_usage_maps_cached_input_to_cache_read_bucket() {
        let usage = openai_provider_usage(1_000, 200, 1_200, 300, 0, None);

        assert_eq!(usage.input_tokens, 1_000);
        assert_eq!(usage.billable_input_tokens, 700);
        assert_eq!(usage.output_tokens, 200);
        assert_eq!(usage.cache_read_tokens, 300);
        assert_eq!(usage.total_tokens, 1_200);
        assert_eq!(usage.reported_cost_micros, None);

        let response_usage = openai_responses_usage(&json!({
            "input_tokens": 1_000,
            "output_tokens": 200,
            "input_tokens_details": { "cached_tokens": 300 }
        }));
        assert_eq!(response_usage.input_tokens, 1_000);
        assert_eq!(response_usage.billable_input_tokens, 700);
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

        assert_eq!(mapped.input_tokens, 1_000);
        assert_eq!(mapped.billable_input_tokens, 600);
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
        assert_eq!(
            gpt_5_4["reasoning"]["summary"],
            OPENAI_REASONING_SUMMARY_DETAIL
        );
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
        assert_eq!(
            openai_api["reasoning"]["summary"],
            OPENAI_REASONING_SUMMARY_DETAIL
        );
        assert_eq!(openai_api["include"][0], "reasoning.encrypted_content");

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
    fn xai_responses_body_uses_native_reasoning_and_sanitized_tool_schema() {
        let mut request = test_request();
        request.controls.active.thinking_effort = Some(ProviderModelThinkingEffortDto::High);
        request.tools[0].input_schema = json!({
            "type": "object",
            "maxProperties": 1,
            "properties": {
                "path": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 120
                },
                "tags": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 3,
                    "uniqueItems": true,
                    "items": { "type": "string", "maxLength": 16 }
                }
            }
        });

        let body = xai_responses_request_body(XAI_PROVIDER_ID, XAI_DEFAULT_MODEL_ID, &request)
            .expect("xAI body");

        assert_eq!(body["model"], XAI_DEFAULT_MODEL_ID);
        assert_eq!(body["stream"], true);
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["reasoning"]["effort"], "high");
        assert!(body["reasoning"].get("summary").is_none());
        assert!(body["tools"][0]["parameters"]
            .get("maxProperties")
            .is_none());
        assert!(body["tools"][0]["parameters"]["properties"]["path"]
            .get("minLength")
            .is_none());
        assert!(body["tools"][0]["parameters"]["properties"]["path"]
            .get("maxLength")
            .is_none());
        assert!(body["tools"][0]["parameters"]["properties"]["tags"]
            .get("minItems")
            .is_none());
        assert!(body["tools"][0]["parameters"]["properties"]["tags"]
            .get("maxItems")
            .is_none());
        assert!(body["tools"][0]["parameters"]["properties"]["tags"]
            .get("uniqueItems")
            .is_none());
        assert!(
            body["tools"][0]["parameters"]["properties"]["tags"]["items"]
                .get("maxLength")
                .is_none()
        );
    }

    #[test]
    fn xai_responses_body_supports_none_reasoning_and_drops_unsupported_efforts() {
        let mut request = test_request();
        request.controls.active.thinking_effort = Some(ProviderModelThinkingEffortDto::None);

        let body = xai_responses_request_body(XAI_PROVIDER_ID, "grok-4.3-latest", &request)
            .expect("xAI alias body");
        assert_eq!(body["reasoning"]["effort"], "none");

        request.controls.active.thinking_effort = Some(ProviderModelThinkingEffortDto::XHigh);
        let body = xai_responses_request_body(XAI_PROVIDER_ID, XAI_DEFAULT_MODEL_ID, &request)
            .expect("xAI body");
        assert!(body.get("reasoning").is_none());

        request.controls.active.thinking_effort = Some(ProviderModelThinkingEffortDto::High);
        let body = xai_responses_request_body(XAI_PROVIDER_ID, "grok-imagine-video", &request)
            .expect("xAI unsupported body preview");
        assert!(body.get("reasoning").is_none());

        let body = xai_responses_request_body(XAI_PROVIDER_ID, "grok-build-0.1", &request)
            .expect("Grok Build body");
        assert_eq!(body["model"], "grok-build-0.1");
        assert!(body.get("reasoning").is_none());
    }

    #[test]
    fn xai_adapter_rejects_non_text_models() {
        let error = XaiResponsesAdapter::new(XaiResponsesProviderConfig {
            model_id: "grok-imagine-image-quality".into(),
            bearer_token: "test-token".into(),
            ..XaiResponsesProviderConfig::default()
        })
        .expect_err("Imagine model should not bind to text runtime");

        assert_eq!(error.code, "xai_model_not_supported_by_text_runtime");
    }

    #[test]
    fn xai_tool_arguments_retry_after_html_entity_decode() {
        let mut partial_calls = BTreeMap::new();
        partial_calls.insert(
            0,
            PartialToolCall {
                id: Some("call-1".into()),
                name: Some("read".into()),
                arguments: "{&quot;path&quot;:&quot;src/lib.rs&quot;}".into(),
            },
        );

        let outcome = finish_provider_turn(
            XAI_PROVIDER_ID,
            String::new(),
            None,
            None,
            partial_calls,
            None,
        )
        .expect("xAI tool args should decode");

        match outcome {
            ProviderTurnOutcome::ToolCalls { tool_calls, .. } => {
                assert_eq!(tool_calls[0].input["path"], "src/lib.rs");
            }
            ProviderTurnOutcome::Complete { .. } => panic!("expected tool call turn"),
        }
    }

    #[test]
    fn openai_codex_patch_tool_parameters_keep_root_object_type() {
        let mut request = test_request();
        request.tools = vec![builtin_tool_descriptors()
            .into_iter()
            .find(|descriptor| descriptor.name == AUTONOMOUS_TOOL_PATCH)
            .expect("patch descriptor")];

        let body = openai_codex_responses_request_body(
            OPENAI_CODEX_PROVIDER_ID,
            "gpt-5.5",
            &request,
            None,
        )
        .expect("codex body");

        assert_eq!(body["tools"][0]["name"], AUTONOMOUS_TOOL_PATCH);
        assert_eq!(body["tools"][0]["parameters"]["type"], "object");
        assert!(body["tools"][0]["parameters"].get("oneOf").is_none());
        assert!(body["tools"][0]["parameters"].get("anyOf").is_none());
        assert!(body["tools"][0]["parameters"].get("allOf").is_none());
        assert!(body["tools"][0]["parameters"].get("enum").is_none());
        assert!(body["tools"][0]["parameters"].get("not").is_none());
        assert!(body["tools"][0]["parameters"]["properties"]["path"].is_object());
        assert!(body["tools"][0]["parameters"]["properties"]["operations"].is_object());
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
        let mut request = test_request();
        request.output_allowance =
            ProviderTurnOutputAllowance::split(4_096, 0, 4_096).expect("Anthropic test allowance");

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
        // Bedrock's InvokeModel body must not carry a `stream` key (its strict Anthropic-on-*
        // schema rejects it); streaming is selected by the API operation instead.
        assert!(bedrock.get("stream").is_none());

        let vertex = anthropic_request_body(None, VERTEX_ANTHROPIC_VERSION, &request, false)
            .expect("vertex body");
        assert!(vertex.get("model").is_none());
        assert_eq!(vertex["anthropic_version"], VERTEX_ANTHROPIC_VERSION);
        assert!(vertex.get("stream").is_none());
    }

    #[test]
    fn anthropic_body_sets_prompt_cache_breakpoints_on_native_api_only() {
        let mut request = test_request();
        request.output_allowance =
            ProviderTurnOutputAllowance::split(4_096, 0, 4_096).expect("Anthropic test allowance");

        let native =
            anthropic_request_body(Some("claude-sonnet-4-5"), "2023-06-01", &request, true)
                .expect("native anthropic body");
        assert_eq!(native["system"][0]["type"], "text");
        assert_eq!(native["system"][0]["text"], "system");
        assert_eq!(native["system"][0]["cache_control"]["type"], "ephemeral");
        let tools = native["tools"].as_array().expect("native tools");
        assert_eq!(
            tools.last().expect("last tool")["cache_control"]["type"],
            "ephemeral"
        );
        let last_message_blocks = native["messages"]
            .as_array()
            .and_then(|messages| messages.last())
            .and_then(|message| message["content"].as_array())
            .expect("last message content blocks");
        assert_eq!(
            last_message_blocks.last().expect("last block")["cache_control"]["type"],
            "ephemeral"
        );

        let bedrock = anthropic_request_body(None, BEDROCK_ANTHROPIC_VERSION, &request, false)
            .expect("bedrock body");
        assert_eq!(bedrock["system"], "system");
        let bedrock_tools = bedrock["tools"].as_array().expect("bedrock tools");
        assert!(bedrock_tools
            .iter()
            .all(|tool| tool.get("cache_control").is_none()));
        let bedrock_blocks = bedrock["messages"]
            .as_array()
            .and_then(|messages| messages.last())
            .and_then(|message| message["content"].as_array())
            .expect("bedrock message blocks");
        assert!(bedrock_blocks
            .iter()
            .all(|block| block.get("cache_control").is_none()));
    }

    #[test]
    fn anthropic_body_enables_thinking_from_controls_and_replays_signed_blocks() {
        let mut request = test_request();
        request.messages = vec![
            ProviderMessage::User {
                content: "do work".into(),
                attachments: Vec::new(),
            },
            ProviderMessage::Assistant {
                content: String::new(),
                reasoning_content: Some("planning the read".into()),
                reasoning_details: Some(json!([
                    {
                        "type": "thinking",
                        "thinking": "planning the read",
                        "signature": "sig-1",
                    },
                    { "type": "redacted_thinking", "data": "opaque" },
                    // Foreign (OpenRouter-shaped) entry must not be replayed.
                    { "type": "reasoning.text", "text": "foreign" },
                    // Unsigned thinking block must not be replayed.
                    { "type": "thinking", "thinking": "unsigned", "signature": "" },
                ])),
                tool_calls: vec![AgentToolCall {
                    tool_call_id: "call-1".into(),
                    tool_name: "read".into(),
                    input: json!({ "path": "src/lib.rs" }),
                }],
            },
            ProviderMessage::Tool {
                tool_call_id: "call-1".into(),
                tool_name: "read".into(),
                content: "file body".into(),
            },
        ];
        request.controls.active.thinking_effort = Some(ProviderModelThinkingEffortDto::High);
        request.output_allowance = ProviderTurnOutputAllowance::split(16_384, 8_192, 8_192)
            .expect("Anthropic thinking allowance");

        let body = anthropic_request_body(Some("claude-sonnet-4-5"), "2023-06-01", &request, true)
            .expect("anthropic thinking body");
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 8_192);
        assert_eq!(body["max_tokens"], 16_384);
        let assistant_blocks = body["messages"][1]["content"]
            .as_array()
            .expect("assistant blocks");
        assert_eq!(assistant_blocks[0]["type"], "thinking");
        assert_eq!(assistant_blocks[0]["signature"], "sig-1");
        assert_eq!(assistant_blocks[1]["type"], "redacted_thinking");
        assert_eq!(assistant_blocks[2]["type"], "tool_use");
        assert_eq!(assistant_blocks.len(), 3);

        // Thinking off (unset or explicit none): no thinking param, no replayed blocks.
        for effort in [None, Some(ProviderModelThinkingEffortDto::None)] {
            request.controls.active.thinking_effort = effort;
            request.output_allowance = ProviderTurnOutputAllowance::split(16_384, 0, 16_384)
                .expect("Anthropic no-thinking allowance");
            let body =
                anthropic_request_body(Some("claude-sonnet-4-5"), "2023-06-01", &request, true)
                    .expect("anthropic body without thinking");
            assert!(body.get("thinking").is_none());
            assert_eq!(body["max_tokens"], 16_384);
            let assistant_blocks = body["messages"][1]["content"]
                .as_array()
                .expect("assistant blocks");
            assert!(assistant_blocks
                .iter()
                .all(|block| block["type"] != "thinking" && block["type"] != "redacted_thinking"));
        }
    }

    #[test]
    fn openai_chat_body_passes_reasoning_effort_for_openai_compatible_hosts() {
        let mut request = test_request();
        request.controls.active.thinking_effort = Some(ProviderModelThinkingEffortDto::XHigh);

        let azure = openai_chat_request_body(AZURE_OPENAI_PROVIDER_ID, "gpt-5.2", &request)
            .expect("azure body");
        assert_eq!(azure["reasoning_effort"], "high");
        assert_eq!(azure["max_completion_tokens"], 4_096);

        let github = openai_chat_request_body(GITHUB_MODELS_PROVIDER_ID, "openai/o4", &request)
            .expect("github body");
        assert_eq!(github["reasoning_effort"], "high");
        assert_eq!(github["max_tokens"], 4_096);
        assert!(github.get("max_completion_tokens").is_none());

        request.controls.active.thinking_effort = Some(ProviderModelThinkingEffortDto::Minimal);
        let gemini =
            openai_chat_request_body(GEMINI_AI_STUDIO_PROVIDER_ID, "gemini-3-pro", &request)
                .expect("gemini body");
        assert_eq!(gemini["reasoning_effort"], "low");
        assert_eq!(gemini["max_tokens"], 4_096);

        // Ollama has no reasoning_effort support; explicit `none` omits it everywhere.
        let ollama = openai_chat_request_body(OLLAMA_PROVIDER_ID, "llama3.1", &request)
            .expect("ollama body");
        assert!(ollama.get("reasoning_effort").is_none());
        assert_eq!(ollama["max_tokens"], 4_096);
        request.controls.active.thinking_effort = Some(ProviderModelThinkingEffortDto::None);
        let azure_none = openai_chat_request_body(AZURE_OPENAI_PROVIDER_ID, "gpt-5.2", &request)
            .expect("azure body without effort");
        assert!(azure_none.get("reasoning_effort").is_none());
    }

    #[test]
    fn collect_anthropic_thinking_outcome_orders_blocks_and_joins_text() {
        let mut blocks = BTreeMap::new();
        blocks.insert(2, json!({ "type": "redacted_thinking", "data": "opaque" }));
        blocks.insert(
            0,
            json!({ "type": "thinking", "thinking": "first", "signature": "sig-a" }),
        );
        blocks.insert(
            1,
            json!({ "type": "thinking", "thinking": "second", "signature": "sig-b" }),
        );

        let (reasoning_content, reasoning_details) = collect_anthropic_thinking_outcome(blocks);
        assert_eq!(reasoning_content.as_deref(), Some("first\n\nsecond"));
        let details = reasoning_details.expect("details");
        assert_eq!(details[0]["thinking"], "first");
        assert_eq!(details[1]["thinking"], "second");
        assert_eq!(details[2]["type"], "redacted_thinking");

        let (empty_content, empty_details) = collect_anthropic_thinking_outcome(BTreeMap::new());
        assert!(empty_content.is_none());
        assert!(empty_details.is_none());
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

        let outcome = finish_provider_turn(
            "test-provider",
            String::new(),
            None,
            None,
            partial_calls,
            None,
        )
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
        let error =
            finish_provider_turn("test-provider", String::new(), None, None, malformed, None)
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
        let outcome = finish_provider_turn(
            OPENAI_CODEX_PROVIDER_ID,
            String::new(),
            None,
            None,
            partial_calls,
            None,
        )
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
    fn openai_responses_reasoning_summary_events_emit_done_text_without_duplicate_deltas() {
        let mut events = Vec::new();
        let mut emit = |event| {
            events.push(event);
            Ok(())
        };
        let mut state = OpenAiResponsesReasoningSummaryState::default();

        assert!(emit_openai_responses_reasoning_summary_event(
            "test-provider",
            &json!({
                "type": "response.reasoning_summary_text.delta",
                "output_index": 0,
                "summary_index": 0,
                "delta": "**Planning fix**\n\n<!-- -->"
            }),
            &mut state,
            &mut emit,
        )
        .expect("delta event handled"));
        assert!(emit_openai_responses_reasoning_summary_event(
            "test-provider",
            &json!({
                "type": "response.reasoning_summary_text.done",
                "output_index": 0,
                "summary_index": 0,
                "text": "**Planning fix**\n\nI should inspect the failing test."
            }),
            &mut state,
            &mut emit,
        )
        .expect("done event handled"));
        assert!(emit_openai_responses_reasoning_summary_event(
            "test-provider",
            &json!({
                "type": "response.reasoning_summary_part.done",
                "output_index": 0,
                "summary_index": 0,
                "part": {
                    "type": "summary_text",
                    "text": "**Planning fix**\n\nI should inspect the failing test."
                }
            }),
            &mut state,
            &mut emit,
        )
        .expect("part done event handled"));
        assert!(!emit_openai_responses_reasoning_summary_event(
            "test-provider",
            &json!({ "type": "response.output_text.delta", "delta": "Done." }),
            &mut state,
            &mut emit,
        )
        .expect("text event ignored"));

        assert_eq!(
            events,
            vec![
                ProviderStreamEvent::ReasoningSummary(
                    "**Planning fix**\n\nI should inspect the failing test.".into()
                ),
                ProviderStreamEvent::ReasoningSummary("\n\n".into()),
            ]
        );
    }

    #[test]
    fn openai_responses_reasoning_summary_output_item_done_backfills_missing_stream_text() {
        let mut events = Vec::new();
        let mut emit = |event| {
            events.push(event);
            Ok(())
        };
        let mut state = OpenAiResponsesReasoningSummaryState::default();

        assert!(emit_openai_responses_reasoning_summary_event(
            "test-provider",
            &json!({
                "type": "response.output_item.done",
                "output_index": 0,
                "item": {
                    "type": "reasoning",
                    "summary": [
                        {
                            "type": "summary_text",
                            "text": "**Checking context**\n\nI need to read the project files."
                        }
                    ]
                }
            }),
            &mut state,
            &mut emit,
        )
        .expect("reasoning output item handled"));
        assert!(emit_openai_responses_reasoning_summary_event(
            "test-provider",
            &json!({
                "type": "response.output_item.done",
                "output_index": 0,
                "item": {
                    "type": "reasoning",
                    "summary": [
                        {
                            "type": "summary_text",
                            "text": "**Checking context**\n\nI need to read the project files."
                        }
                    ]
                }
            }),
            &mut state,
            &mut emit,
        )
        .expect("duplicate reasoning output item handled"));

        assert_eq!(
            events,
            vec![
                ProviderStreamEvent::ReasoningSummary(
                    "**Checking context**\n\nI need to read the project files.".into()
                ),
                ProviderStreamEvent::ReasoningSummary("\n\n".into()),
            ]
        );
    }

    #[test]
    fn openai_responses_completed_event_backfills_reasoning_output_summary() {
        let mut events = Vec::new();
        let mut emit = |event| {
            events.push(event);
            Ok(())
        };
        let mut state = OpenAiResponsesReasoningSummaryState::default();

        emit_openai_responses_completed_reasoning_summaries(
            "test-provider",
            &json!({
                "type": "response.completed",
                "response": {
                    "output": [
                        {
                            "type": "reasoning",
                            "summary": [
                                {
                                    "type": "summary_text",
                                    "text": "**Planning project inspection**\n\nI need to inspect the package metadata and README."
                                }
                            ]
                        },
                        {
                            "type": "message",
                            "content": [
                                {
                                    "type": "output_text",
                                    "text": "Done."
                                }
                            ]
                        }
                    ]
                }
            }),
            &mut state,
            &mut emit,
        )
        .expect("completed response summaries handled");

        assert_eq!(
            events,
            vec![
                ProviderStreamEvent::ReasoningSummary(
                    "**Planning project inspection**\n\nI need to inspect the package metadata and README."
                        .into()
                ),
                ProviderStreamEvent::ReasoningSummary("\n\n".into()),
            ]
        );
    }

    #[test]
    fn openai_responses_stream_collects_and_deduplicates_replayable_reasoning_items() {
        let first = json!({
            "type": "reasoning",
            "id": "rs_1",
            "encrypted_content": "encrypted-1",
            "summary": [{ "type": "summary_text", "text": "First" }]
        });
        let second = json!({
            "type": "reasoning",
            "id": "rs_2",
            "encrypted_content": "encrypted-2",
            "summary": [{ "type": "summary_text", "text": "Second" }]
        });
        let mut state = OpenAiResponsesReasoningSummaryState::default();
        let mut emit = |_| Ok(());

        emit_openai_responses_reasoning_summary_event(
            OPENAI_API_PROVIDER_ID,
            &json!({
                "type": "response.output_item.done",
                "output_index": 0,
                "item": first.clone()
            }),
            &mut state,
            &mut emit,
        )
        .expect("output item event");
        emit_openai_responses_completed_reasoning_summaries(
            OPENAI_API_PROVIDER_ID,
            &json!({
                "type": "response.completed",
                "response": { "output": [first.clone(), second.clone()] }
            }),
            &mut state,
            &mut emit,
        )
        .expect("completed event");

        assert_eq!(
            state.replayable_reasoning_details(),
            Some(json!([first, second]))
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
        let error = finish_provider_turn("test-provider", String::new(), None, None, blank, None)
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
        let error =
            finish_provider_turn("test-provider", String::new(), None, None, duplicate, None)
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
        let error =
            finish_provider_turn("test-provider", String::new(), None, None, blank_name, None)
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
            AgentProviderConfig::XaiResponses(XaiResponsesProviderConfig {
                provider_id: XAI_PROVIDER_ID.into(),
                model_id: XAI_DEFAULT_MODEL_ID.into(),
                base_url: XAI_API_BASE_URL.into(),
                bearer_token: "test-key".into(),
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
            AgentProviderConfig::DeepSeek(DeepSeekProviderConfig {
                model_id: "deepseek-v4-pro".into(),
                base_url: "https://api.deepseek.com".into(),
                api_key: "test-key".into(),
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

    fn spawn_provider_stream_server(
        handle: impl FnOnce(TcpStream) + Send + 'static,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider test server");
        let address = listener.local_addr().expect("provider test server address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept provider request");
            let mut request = Vec::new();
            let mut byte = [0_u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                stream
                    .read_exact(&mut byte)
                    .expect("read provider request headers");
                request.push(byte[0]);
            }
            let request_headers = String::from_utf8_lossy(&request);
            let content_length = request_headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or_default();
            let mut request_body = vec![0_u8; content_length];
            stream
                .read_exact(&mut request_body)
                .expect("read provider request body");
            handle(stream);
        });
        (format!("http://{address}/stream"), server)
    }

    fn write_provider_stream_headers(stream: &mut TcpStream) {
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
            )
            .expect("write provider stream headers");
        stream.flush().expect("flush provider stream headers");
    }

    fn write_http_chunk(stream: &mut TcpStream, payload: &str) {
        write!(stream, "{:X}\r\n", payload.len()).expect("write HTTP chunk length");
        stream
            .write_all(payload.as_bytes())
            .expect("write HTTP chunk body");
        stream
            .write_all(b"\r\n")
            .expect("write HTTP chunk terminator");
        stream.flush().expect("flush HTTP chunk");
    }

    fn finish_http_chunks(stream: &mut TcpStream) {
        stream.write_all(b"0\r\n\r\n").expect("finish HTTP chunks");
        stream.flush().expect("flush HTTP completion");
    }

    fn test_provider_stream_reader(
        url: String,
        idle_timeout: Duration,
        cancellation: AgentRunCancellationToken,
    ) -> ProviderStreamReader {
        let client = provider_http_client(idle_timeout.as_millis() as u64)
            .expect("build provider test client");
        start_provider_json_stream("test-provider", idle_timeout, cancellation, move || {
            client.post(url.clone()).json(&json!({ "stream": true }))
        })
        .expect("start provider test stream")
    }

    #[test]
    fn provider_stream_idle_timeout_resets_after_each_chunk() {
        let idle_timeout = Duration::from_millis(80);
        let (url, server) = spawn_provider_stream_server(move |mut stream| {
            write_provider_stream_headers(&mut stream);
            for delta in ["a", "b", "c", "d"] {
                write_http_chunk(
                    &mut stream,
                    &format!(
                        "data: {{\"type\":\"response.output_text.delta\",\"delta\":\"{delta}\"}}\n\n"
                    ),
                );
                thread::sleep(Duration::from_millis(45));
            }
            write_http_chunk(&mut stream, "data: [DONE]\n\n");
            finish_http_chunks(&mut stream);
        });
        let reader =
            test_provider_stream_reader(url, idle_timeout, AgentRunCancellationToken::default());
        let started = Instant::now();
        let mut emitted = String::new();

        let outcome = parse_openai_responses_sse("test-provider", reader, &mut |event| {
            if let ProviderStreamEvent::MessageDelta(delta) = event {
                emitted.push_str(&delta);
            }
            Ok(())
        })
        .expect("periodic provider chunks should stay healthy");
        server.join().expect("join provider test server");

        assert!(
            matches!(outcome, ProviderTurnOutcome::Complete { message, .. } if message == "abcd")
                && emitted == "abcd"
                && started.elapsed() > idle_timeout,
            "periodic progress should outlive one idle window"
        );
    }

    #[test]
    fn provider_stream_reports_typed_idle_timeout_after_headers_then_silence() {
        let (url, server) = spawn_provider_stream_server(|mut stream| {
            write_provider_stream_headers(&mut stream);
            thread::sleep(Duration::from_millis(180));
        });
        let mut reader = test_provider_stream_reader(
            url,
            Duration::from_millis(50),
            AgentRunCancellationToken::default(),
        );
        let mut bytes = Vec::new();

        let error = reader
            .read_to_end(&mut bytes)
            .expect_err("silent provider stream must time out");
        server.join().expect("join provider test server");
        let command_error = error
            .get_ref()
            .and_then(|error| error.downcast_ref::<CommandError>())
            .expect("typed provider stream error");

        assert_eq!(command_error.code, "agent_provider_stream_idle_timeout");
        assert!(
            reader.transport_is_joined(),
            "transport thread must be joined"
        );
    }

    #[test]
    fn provider_stream_cancellation_is_prompt_and_disconnects_server() {
        let (disconnect_tx, disconnect_rx) = mpsc::channel();
        let (url, server) = spawn_provider_stream_server(move |mut stream| {
            write_provider_stream_headers(&mut stream);
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .expect("set disconnect observation timeout");
            let mut byte = [0_u8; 1];
            disconnect_tx
                .send(matches!(stream.read(&mut byte), Ok(0)))
                .expect("report provider disconnect");
        });
        let cancellation = AgentRunCancellationToken::default();
        let mut reader =
            test_provider_stream_reader(url, Duration::from_secs(5), cancellation.clone());
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(40));
            cancellation.cancel();
        });
        let started = Instant::now();
        let mut bytes = Vec::new();

        let error = reader
            .read_to_end(&mut bytes)
            .expect_err("cancelled provider stream must stop");
        canceller.join().expect("join provider canceller");
        server.join().expect("join provider test server");
        let command_error = error
            .get_ref()
            .and_then(|error| error.downcast_ref::<CommandError>())
            .expect("typed provider cancellation error");

        assert_eq!(command_error.code, "agent_run_cancelled");
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "cancellation should not wait for the idle timeout"
        );
        assert!(
            disconnect_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("server disconnect observation"),
            "dropping the response must close the provider connection"
        );
        assert!(
            reader.transport_is_joined(),
            "transport thread must be joined"
        );
    }

    #[test]
    fn provider_stream_parser_completion_closes_an_open_response() {
        let (disconnect_tx, disconnect_rx) = mpsc::channel();
        let (url, server) = spawn_provider_stream_server(move |mut stream| {
            write_provider_stream_headers(&mut stream);
            write_http_chunk(
                &mut stream,
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"done\"}\n\n",
            );
            write_http_chunk(&mut stream, "data: [DONE]\n\n");
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .expect("set completion disconnect timeout");
            let mut byte = [0_u8; 1];
            disconnect_tx
                .send(matches!(stream.read(&mut byte), Ok(0)))
                .expect("report completion disconnect");
        });
        let reader = test_provider_stream_reader(
            url,
            Duration::from_secs(5),
            AgentRunCancellationToken::default(),
        );
        let started = Instant::now();

        let outcome = parse_openai_responses_sse("test-provider", reader, &mut |_| Ok(()))
            .expect("parser should complete at the done event");
        server.join().expect("join provider test server");

        assert!(
            matches!(outcome, ProviderTurnOutcome::Complete { message, .. } if message == "done")
                && started.elapsed() < Duration::from_millis(300)
                && disconnect_rx
                    .recv_timeout(Duration::from_secs(1))
                    .expect("completion disconnect observation"),
            "parser completion must drop the response and join its transport"
        );
    }

    #[test]
    fn provider_stream_channel_applies_bounded_backpressure() {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let cancellation = AgentRunCancellationToken::default();
        sender
            .try_send(ProviderStreamTransportMessage::Chunk(vec![1]))
            .expect("fill bounded provider stream channel");
        let (finished_tx, finished_rx) = mpsc::channel();
        let producer = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build provider backpressure test runtime");
            runtime.block_on(async {
                send_provider_stream_chunk(&sender, vec![2], stop.as_ref(), &cancellation)
                    .await
                    .expect("send provider stream chunk");
            });
            finished_tx.send(()).expect("report producer completion");
        });

        assert!(
            finished_rx.recv_timeout(Duration::from_millis(40)).is_err(),
            "producer must wait while the bounded parser channel is full"
        );
        let _ = receiver.recv().expect("drain first provider chunk");
        finished_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("producer should resume after parser drains capacity");
        producer.join().expect("join provider stream producer");
    }
}
