use serde::{Deserialize, Serialize};

pub const PROVIDER_CAPABILITY_CATALOG_CONTRACT_VERSION: u32 = 1;
pub const DEFAULT_PROVIDER_CATALOG_TTL_SECONDS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderCapabilityCatalogInput {
    pub provider_id: String,
    pub model_id: String,
    pub catalog_source: String,
    pub fetched_at: Option<String>,
    pub last_success_at: Option<String>,
    pub cache_age_seconds: Option<i64>,
    pub cache_ttl_seconds: Option<i64>,
    pub credential_proof: Option<String>,
    pub context_window_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub context_limit_source: Option<String>,
    pub context_limit_confidence: Option<String>,
    pub thinking_supported: bool,
    pub thinking_efforts: Vec<String>,
    pub thinking_default_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderCapabilityCatalog {
    pub contract_version: u32,
    pub provider_id: String,
    pub provider_label: String,
    pub default_model_id: String,
    pub runtime_family: String,
    pub runtime_kind: String,
    pub auth_method: String,
    pub credential_proof: Option<String>,
    pub transport_mode: String,
    pub endpoint_shape: String,
    pub catalog_kind: String,
    pub model_list_strategy: String,
    pub external_agent_adapter: bool,
    pub cache: ProviderCatalogCacheMetadata,
    pub request_preview: ProviderRedactedRequestPreview,
    pub capabilities: ProviderCapabilityFeatureSet,
    pub known_limitations: Vec<String>,
    pub remediations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderCatalogCacheMetadata {
    pub source: String,
    pub fetched_at: Option<String>,
    pub last_success_at: Option<String>,
    pub age_seconds: Option<i64>,
    pub ttl_seconds: i64,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderRedactedRequestPreview {
    pub route: String,
    pub model_id: String,
    pub enabled_features: Vec<String>,
    pub tool_schema_names: Vec<String>,
    pub headers: Vec<String>,
    pub metadata: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderCapabilityFeatureSet {
    pub streaming: ProviderFeatureCapability,
    pub tool_calls: ProviderToolCallCapability,
    pub reasoning: ProviderReasoningCapability,
    pub attachments: ProviderAttachmentCapability,
    pub context_limits: ProviderContextLimitCapability,
    pub cost_hints: ProviderFeatureCapability,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderFeatureCapability {
    pub status: String,
    pub source: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderToolCallCapability {
    pub status: String,
    pub source: String,
    pub strictness_behavior: String,
    pub schema_dialect: String,
    pub parallel_call_behavior: String,
    pub known_incompatibilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderReasoningCapability {
    pub status: String,
    pub source: String,
    pub effort_levels: Vec<String>,
    pub default_effort: Option<String>,
    pub summary_support: String,
    pub clamping: String,
    pub unsupported_model_fallback: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderAttachmentCapability {
    pub status: String,
    pub source: String,
    pub image_input: String,
    pub document_input: String,
    pub supported_types: Vec<String>,
    pub limits: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderContextLimitCapability {
    pub status: String,
    pub source: String,
    pub confidence: String,
    pub context_window_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct ProviderStaticCapability {
    provider_id: &'static str,
    provider_label: &'static str,
    default_model_id: &'static str,
    runtime_family: &'static str,
    runtime_kind: &'static str,
    auth_method: &'static str,
    transport_mode: &'static str,
    endpoint_shape: &'static str,
    catalog_kind: &'static str,
    external_agent_adapter: bool,
}

pub fn provider_capability_catalog(
    input: ProviderCapabilityCatalogInput,
) -> ProviderCapabilityCatalog {
    let provider = provider_static_capability(&input.provider_id);
    let cache_ttl_seconds = input
        .cache_ttl_seconds
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS);
    let model_id = normalize_model_id(&input.model_id, provider.default_model_id);
    let model_list_strategy = model_list_strategy(&input.catalog_source, provider.provider_id);
    let capabilities = provider_capability_feature_set(provider, &input, &model_id);
    let known_limitations = provider_known_limitations(provider, &capabilities);
    let remediations = provider_remediations(provider, &capabilities, &input.catalog_source);
    let request_preview = provider_request_preview(provider, &model_id, &capabilities);

    ProviderCapabilityCatalog {
        contract_version: PROVIDER_CAPABILITY_CATALOG_CONTRACT_VERSION,
        provider_id: provider.provider_id.into(),
        provider_label: provider.provider_label.into(),
        default_model_id: provider.default_model_id.into(),
        runtime_family: provider.runtime_family.into(),
        runtime_kind: provider.runtime_kind.into(),
        auth_method: provider.auth_method.into(),
        credential_proof: input.credential_proof,
        transport_mode: provider.transport_mode.into(),
        endpoint_shape: provider.endpoint_shape.into(),
        catalog_kind: provider.catalog_kind.into(),
        model_list_strategy,
        external_agent_adapter: provider.external_agent_adapter,
        cache: ProviderCatalogCacheMetadata {
            source: input.catalog_source.clone(),
            fetched_at: input.fetched_at,
            last_success_at: input.last_success_at,
            age_seconds: input.cache_age_seconds,
            ttl_seconds: cache_ttl_seconds,
            stale: input.catalog_source == "cache"
                || input
                    .cache_age_seconds
                    .is_some_and(|age| age > cache_ttl_seconds),
        },
        request_preview,
        capabilities,
        known_limitations,
        remediations,
    }
}

fn provider_capability_feature_set(
    provider: ProviderStaticCapability,
    input: &ProviderCapabilityCatalogInput,
    model_id: &str,
) -> ProviderCapabilityFeatureSet {
    ProviderCapabilityFeatureSet {
        streaming: streaming_capability(provider),
        tool_calls: tool_call_capability(provider),
        reasoning: reasoning_capability(provider, input, model_id),
        attachments: attachment_capability(provider),
        context_limits: context_limit_capability(input),
        cost_hints: cost_hint_capability(provider, &input.catalog_source),
    }
}

fn provider_static_capability(provider_id: &str) -> ProviderStaticCapability {
    match provider_id {
        "fake_provider" => ProviderStaticCapability {
            provider_id: "fake_provider",
            provider_label: "Fake Provider",
            default_model_id: "fake-model",
            runtime_family: "fake",
            runtime_kind: "fake_provider",
            auth_method: "none",
            transport_mode: "hosted_api",
            endpoint_shape: "fake_provider",
            catalog_kind: "owned_agent_provider",
            external_agent_adapter: false,
        },
        "openai_codex" => ProviderStaticCapability {
            provider_id: "openai_codex",
            provider_label: "OpenAI Codex",
            default_model_id: "gpt-5.4",
            runtime_family: "openai_codex",
            runtime_kind: "openai_codex",
            auth_method: "oauth",
            transport_mode: "hosted_api",
            endpoint_shape: "openai_responses",
            catalog_kind: "model_provider",
            external_agent_adapter: false,
        },
        "openrouter" => ProviderStaticCapability {
            provider_id: "openrouter",
            provider_label: "OpenRouter",
            default_model_id: "openai/gpt-5.4",
            runtime_family: "openrouter",
            runtime_kind: "openrouter",
            auth_method: "api_key",
            transport_mode: "hosted_api",
            endpoint_shape: "openai_chat_completions",
            catalog_kind: "model_provider",
            external_agent_adapter: false,
        },
        "anthropic" => ProviderStaticCapability {
            provider_id: "anthropic",
            provider_label: "Anthropic",
            default_model_id: "claude-sonnet-4-5",
            runtime_family: "anthropic",
            runtime_kind: "anthropic",
            auth_method: "api_key",
            transport_mode: "hosted_api",
            endpoint_shape: "anthropic_messages",
            catalog_kind: "model_provider",
            external_agent_adapter: false,
        },
        "github_models" => ProviderStaticCapability {
            provider_id: "github_models",
            provider_label: "GitHub Models",
            default_model_id: "openai/gpt-4.1",
            runtime_family: "openai_compatible",
            runtime_kind: "openai_compatible",
            auth_method: "api_key",
            transport_mode: "hosted_api",
            endpoint_shape: "openai_chat_completions",
            catalog_kind: "model_provider",
            external_agent_adapter: false,
        },
        "openai_api" => ProviderStaticCapability {
            provider_id: "openai_api",
            provider_label: "OpenAI-compatible",
            default_model_id: "gpt-4.1-mini",
            runtime_family: "openai_compatible",
            runtime_kind: "openai_compatible",
            auth_method: "api_key",
            transport_mode: "openai_compatible_api",
            endpoint_shape: "openai_chat_completions",
            catalog_kind: "model_provider",
            external_agent_adapter: false,
        },
        "ollama" => ProviderStaticCapability {
            provider_id: "ollama",
            provider_label: "Ollama",
            default_model_id: "llama3.2",
            runtime_family: "openai_compatible",
            runtime_kind: "openai_compatible",
            auth_method: "local",
            transport_mode: "local_api",
            endpoint_shape: "openai_chat_completions",
            catalog_kind: "model_provider",
            external_agent_adapter: false,
        },
        "azure_openai" => ProviderStaticCapability {
            provider_id: "azure_openai",
            provider_label: "Azure OpenAI",
            default_model_id: "deployment",
            runtime_family: "openai_compatible",
            runtime_kind: "openai_compatible",
            auth_method: "api_key",
            transport_mode: "openai_compatible_api",
            endpoint_shape: "azure_openai_chat_completions",
            catalog_kind: "model_provider",
            external_agent_adapter: false,
        },
        "gemini_ai_studio" | "gemini" => ProviderStaticCapability {
            provider_id: "gemini_ai_studio",
            provider_label: "Gemini AI Studio",
            default_model_id: "gemini-2.5-pro",
            runtime_family: "gemini",
            runtime_kind: "gemini",
            auth_method: "api_key",
            transport_mode: "hosted_api",
            endpoint_shape: "openai_chat_completions",
            catalog_kind: "model_provider",
            external_agent_adapter: false,
        },
        "bedrock" => ProviderStaticCapability {
            provider_id: "bedrock",
            provider_label: "Amazon Bedrock",
            default_model_id: "anthropic.claude-3-5-sonnet",
            runtime_family: "anthropic",
            runtime_kind: "anthropic",
            auth_method: "ambient",
            transport_mode: "cloud_cli_bridge",
            endpoint_shape: "bedrock_invoke_model",
            catalog_kind: "model_provider",
            external_agent_adapter: false,
        },
        "vertex" => ProviderStaticCapability {
            provider_id: "vertex",
            provider_label: "Vertex AI",
            default_model_id: "claude-sonnet-4@20250514",
            runtime_family: "anthropic",
            runtime_kind: "anthropic",
            auth_method: "ambient",
            transport_mode: "ambient_cloud_api",
            endpoint_shape: "vertex_anthropic_raw_predict",
            catalog_kind: "model_provider",
            external_agent_adapter: false,
        },
        "external_codex_cli" => {
            external_agent_static("external_codex_cli", "Codex CLI", "codex-cli")
        }
        "external_claude_code" => {
            external_agent_static("external_claude_code", "Claude Code", "claude-code")
        }
        "external_gemini_cli" => {
            external_agent_static("external_gemini_cli", "Gemini CLI", "gemini-cli")
        }
        "external_custom_agent" => external_agent_static(
            "external_custom_agent",
            "Custom External Agent",
            "external-agent",
        ),
        _ => ProviderStaticCapability {
            provider_id: "unknown",
            provider_label: "Unknown provider",
            default_model_id: "unknown-model",
            runtime_family: "unknown",
            runtime_kind: "unknown",
            auth_method: "none",
            transport_mode: "hosted_api",
            endpoint_shape: "unknown",
            catalog_kind: "model_provider",
            external_agent_adapter: false,
        },
    }
}

fn external_agent_static(
    provider_id: &'static str,
    provider_label: &'static str,
    default_model_id: &'static str,
) -> ProviderStaticCapability {
    ProviderStaticCapability {
        provider_id,
        provider_label,
        default_model_id,
        runtime_family: "external_agent",
        runtime_kind: "external_agent",
        auth_method: "external_process",
        transport_mode: "external_agent_cli",
        endpoint_shape: "external_agent_cli",
        catalog_kind: "external_agent_adapter",
        external_agent_adapter: true,
    }
}

fn normalize_model_id(model_id: &str, default_model_id: &str) -> String {
    let trimmed = model_id.trim();
    if trimmed.is_empty() {
        default_model_id.into()
    } else {
        trimmed.into()
    }
}

fn model_list_strategy(catalog_source: &str, provider_id: &str) -> String {
    match catalog_source {
        "live" => "live_provider_catalog".into(),
        "cache" => "cached_provider_catalog".into(),
        "manual" => "manual_unverified_catalog".into(),
        "unavailable" => "unavailable".into(),
        _ if provider_id.starts_with("external_") => "external_agent_adapter".into(),
        _ => "unknown".into(),
    }
}

fn streaming_capability(provider: ProviderStaticCapability) -> ProviderFeatureCapability {
    match provider.provider_id {
        "fake_provider" => feature("probed", "static", "Fake provider streams deterministic harness events."),
        "bedrock" => feature(
            "unavailable",
            "static",
            "The Bedrock owned adapter uses invoke-model through the AWS CLI and returns a complete response.",
        ),
        "vertex" => feature(
            "unavailable",
            "static",
            "The Vertex Anthropic adapter uses rawPredict and returns a complete response.",
        ),
        id if id.starts_with("external_") => feature(
            "not_applicable",
            "static",
            "External agent CLIs are hosted as subprocess runs, not normal model streams.",
        ),
        _ => feature(
            "supported",
            "static",
            "The owned adapter sends streaming provider requests for normal turns.",
        ),
    }
}

fn tool_call_capability(provider: ProviderStaticCapability) -> ProviderToolCallCapability {
    match provider.provider_id {
        id if id.starts_with("external_") => ProviderToolCallCapability {
            status: "not_applicable".into(),
            source: "static".into(),
            strictness_behavior: "external_agent_owned".into(),
            schema_dialect: "external_cli".into(),
            parallel_call_behavior: "external_agent_owned".into(),
            known_incompatibilities: vec![
                "External subscription-backed CLIs do not expose Xero-owned tool schemas.".into(),
            ],
        },
        "openai_codex" => ProviderToolCallCapability {
            status: "supported".into(),
            source: "static".into(),
            strictness_behavior: "strict_null_passthrough".into(),
            schema_dialect: "openai_responses_json_schema".into(),
            parallel_call_behavior: "parallel_enabled".into(),
            known_incompatibilities: Vec::new(),
        },
        "anthropic" | "bedrock" | "vertex" => ProviderToolCallCapability {
            status: "supported".into(),
            source: "static".into(),
            strictness_behavior: "anthropic_input_schema".into(),
            schema_dialect: "json_schema_object".into(),
            parallel_call_behavior: "provider_decides".into(),
            known_incompatibilities: Vec::new(),
        },
        _ => ProviderToolCallCapability {
            status: "supported".into(),
            source: "static".into(),
            strictness_behavior: "openai_function_schema".into(),
            schema_dialect: "json_schema_object".into(),
            parallel_call_behavior: "provider_decides".into(),
            known_incompatibilities: Vec::new(),
        },
    }
}

fn reasoning_capability(
    provider: ProviderStaticCapability,
    input: &ProviderCapabilityCatalogInput,
    model_id: &str,
) -> ProviderReasoningCapability {
    if provider.provider_id.starts_with("external_") {
        return ProviderReasoningCapability {
            status: "not_applicable".into(),
            source: "static".into(),
            effort_levels: Vec::new(),
            default_effort: None,
            summary_support: "external_agent_owned".into(),
            clamping: "external_agent_owned".into(),
            unsupported_model_fallback: "choose_owned_model_provider".into(),
        };
    }

    if input.thinking_supported {
        return ProviderReasoningCapability {
            status: "supported".into(),
            source: if input.catalog_source == "live" {
                "live".into()
            } else if input.catalog_source == "cache" {
                "cached".into()
            } else {
                "static".into()
            },
            effort_levels: input.thinking_efforts.clone(),
            default_effort: input.thinking_default_effort.clone(),
            summary_support: if provider.provider_id == "openai_codex"
                || provider.provider_id == "openai_api"
            {
                "auto_summary_supported".into()
            } else {
                "provider_default".into()
            },
            clamping: "unsupported_effort_dropped_before_request".into(),
            unsupported_model_fallback: "disable_reasoning_control".into(),
        };
    }

    ProviderReasoningCapability {
        status: if model_id.trim().is_empty() {
            "unknown".into()
        } else {
            "unavailable".into()
        },
        source: "static".into(),
        effort_levels: Vec::new(),
        default_effort: None,
        summary_support: "unavailable".into(),
        clamping: "reasoning_control_disabled".into(),
        unsupported_model_fallback: "send_without_reasoning_control".into(),
    }
}

fn attachment_capability(provider: ProviderStaticCapability) -> ProviderAttachmentCapability {
    match provider.provider_id {
        "anthropic" | "bedrock" | "vertex" => ProviderAttachmentCapability {
            status: "supported".into(),
            source: "static".into(),
            image_input: "supported".into(),
            document_input: "supported".into(),
            supported_types: vec![
                "image/*".into(),
                "application/pdf".into(),
                "text/plain".into(),
                "text/markdown".into(),
            ],
            limits: vec![
                "Documents are sent as provider-native PDF blocks where supported.".into(),
            ],
        },
        id if id.starts_with("external_") => ProviderAttachmentCapability {
            status: "not_applicable".into(),
            source: "static".into(),
            image_input: "external_agent_owned".into(),
            document_input: "external_agent_owned".into(),
            supported_types: Vec::new(),
            limits: vec![
                "External agent CLIs own their own attachment and file-ingest behavior.".into(),
            ],
        },
        _ => ProviderAttachmentCapability {
            status: "unavailable".into(),
            source: "static".into(),
            image_input: "not_wired_in_owned_adapter".into(),
            document_input: "not_wired_in_owned_adapter".into(),
            supported_types: Vec::new(),
            limits: vec!["This owned adapter currently sends text and tool calls only.".into()],
        },
    }
}

fn context_limit_capability(
    input: &ProviderCapabilityCatalogInput,
) -> ProviderContextLimitCapability {
    let known = input.context_window_tokens.is_some() || input.max_output_tokens.is_some();
    ProviderContextLimitCapability {
        status: if known { "supported" } else { "unknown" }.into(),
        source: input
            .context_limit_source
            .clone()
            .unwrap_or_else(|| "unknown".into()),
        confidence: input
            .context_limit_confidence
            .clone()
            .unwrap_or_else(|| "unknown".into()),
        context_window_tokens: input.context_window_tokens,
        max_output_tokens: input.max_output_tokens,
    }
}

fn cost_hint_capability(
    provider: ProviderStaticCapability,
    catalog_source: &str,
) -> ProviderFeatureCapability {
    if provider.provider_id == "openrouter" && catalog_source == "live" {
        return feature(
            "supported",
            "live",
            "OpenRouter exposes price metadata in its model catalog when available; Xero treats it as a hint.",
        );
    }

    feature(
        "unknown",
        "unverified",
        "No trusted cost metadata is available for this provider catalog entry.",
    )
}

fn provider_request_preview(
    provider: ProviderStaticCapability,
    model_id: &str,
    capabilities: &ProviderCapabilityFeatureSet,
) -> ProviderRedactedRequestPreview {
    let route = match provider.provider_id {
        "openai_codex" => "POST /codex/responses",
        "anthropic" => "POST /v1/messages",
        "bedrock" => "aws bedrock-runtime invoke-model",
        "vertex" => "POST /v1/projects/{project}/locations/{region}/publishers/anthropic/models/{model}:rawPredict",
        id if id.starts_with("external_") => "external-agent-cli",
        _ => "POST /chat/completions",
    }
    .to_string();
    let mut enabled_features = Vec::new();
    if capabilities.streaming.status == "supported" || capabilities.streaming.status == "probed" {
        enabled_features.push("streaming".into());
    }
    if capabilities.tool_calls.status == "supported" || capabilities.tool_calls.status == "probed" {
        enabled_features.push("tool_calls".into());
    }
    if capabilities.reasoning.status == "supported" || capabilities.reasoning.status == "probed" {
        enabled_features.push("reasoning".into());
    }
    if capabilities.attachments.status == "supported" || capabilities.attachments.status == "probed"
    {
        enabled_features.push("attachments".into());
    }

    let headers = match provider.auth_method {
        "oauth" => vec![
            "Authorization: Bearer [redacted]".into(),
            "chatgpt-account-id: [redacted]".into(),
        ],
        "api_key" => vec!["Authorization/x-api-key: [redacted]".into()],
        "ambient" => vec!["ambient-cloud-auth: [redacted]".into()],
        _ => Vec::new(),
    };

    ProviderRedactedRequestPreview {
        route,
        model_id: model_id.into(),
        enabled_features,
        tool_schema_names: vec!["xero_echo_probe".into()],
        headers,
        metadata: vec![
            format!("transportMode={}", provider.transport_mode),
            format!("endpointShape={}", provider.endpoint_shape),
        ],
    }
}

fn provider_known_limitations(
    provider: ProviderStaticCapability,
    capabilities: &ProviderCapabilityFeatureSet,
) -> Vec<String> {
    let mut limitations = Vec::new();
    if capabilities.streaming.status == "unavailable" {
        limitations.push("Provider turns are not streamed by this adapter.".into());
    }
    if capabilities.attachments.status == "unavailable" {
        limitations
            .push("Image and document attachments are not sent by this owned adapter.".into());
    }
    if provider.external_agent_adapter {
        limitations.push(
            "External agent adapters are isolated from normal model-provider credentials.".into(),
        );
    }
    limitations
}

fn provider_remediations(
    provider: ProviderStaticCapability,
    capabilities: &ProviderCapabilityFeatureSet,
    catalog_source: &str,
) -> Vec<String> {
    let mut remediations = Vec::new();
    if catalog_source == "manual" {
        remediations.push(
            "Run provider diagnostics or choose a live-catalog provider to verify the model id."
                .into(),
        );
    }
    if catalog_source == "unavailable" {
        remediations.push("Repair credentials, endpoint metadata, or provider availability, then refresh the catalog.".into());
    }
    if capabilities.tool_calls.status == "unavailable" {
        remediations.push(
            "Choose a model/provider with tool-call support before running an agent task.".into(),
        );
    }
    if provider.transport_mode == "local_api" {
        remediations.push("Start the local model server before running connection checks.".into());
    }
    remediations
}

fn feature(status: &str, source: &str, detail: &str) -> ProviderFeatureCapability {
    ProviderFeatureCapability {
        status: status.into(),
        source: source.into(),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(provider_id: &str) -> ProviderCapabilityCatalogInput {
        ProviderCapabilityCatalogInput {
            provider_id: provider_id.into(),
            model_id: String::new(),
            catalog_source: "live".into(),
            fetched_at: Some("2026-04-26T12:00:00Z".into()),
            last_success_at: Some("2026-04-26T12:00:00Z".into()),
            cache_age_seconds: Some(30),
            cache_ttl_seconds: Some(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
            credential_proof: Some("stored_secret".into()),
            context_window_tokens: Some(128_000),
            max_output_tokens: Some(16_384),
            context_limit_source: Some("live_catalog".into()),
            context_limit_confidence: Some("high".into()),
            thinking_supported: true,
            thinking_efforts: vec!["low".into(), "medium".into(), "high".into()],
            thinking_default_effort: Some("medium".into()),
        }
    }

    #[test]
    fn external_agent_capabilities_are_separate_from_model_providers() {
        let catalog = provider_capability_catalog(input("external_codex_cli"));

        assert!(catalog.external_agent_adapter);
        assert_eq!(catalog.catalog_kind, "external_agent_adapter");
        assert_eq!(catalog.transport_mode, "external_agent_cli");
        assert_eq!(catalog.capabilities.tool_calls.status, "not_applicable");
        assert_eq!(catalog.request_preview.route, "external-agent-cli");
    }

    #[test]
    fn ambient_cloud_bridges_classify_streaming_and_attachments() {
        let catalog = provider_capability_catalog(input("bedrock"));

        assert_eq!(catalog.transport_mode, "cloud_cli_bridge");
        assert_eq!(catalog.auth_method, "ambient");
        assert_eq!(catalog.capabilities.streaming.status, "unavailable");
        assert_eq!(catalog.capabilities.attachments.status, "supported");
    }

    #[test]
    fn live_openrouter_catalog_exposes_cost_hints_and_redacted_probe_preview() {
        let catalog = provider_capability_catalog(input("openrouter"));

        assert_eq!(catalog.capabilities.cost_hints.status, "supported");
        assert!(catalog
            .request_preview
            .tool_schema_names
            .iter()
            .any(|name| name == "xero_echo_probe"));
        assert!(catalog
            .request_preview
            .headers
            .iter()
            .all(|header| header.contains("[redacted]")));
    }
}
