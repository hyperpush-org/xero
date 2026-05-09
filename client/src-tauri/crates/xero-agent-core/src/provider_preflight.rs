use std::time::Duration;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};

use crate::{
    CoreError, CoreResult, ProviderCapabilityCatalog, ProviderCapabilityCatalogInput,
    DEFAULT_PROVIDER_CATALOG_TTL_SECONDS,
};

pub const PROVIDER_PREFLIGHT_CONTRACT_VERSION: u32 = 1;
pub const DEFAULT_PROVIDER_PREFLIGHT_TTL_SECONDS: i64 = 6 * 60 * 60;
const DEFAULT_PROVIDER_PREFLIGHT_TIMEOUT_MS: u64 = 30_000;
const PREFLIGHT_PROBE_TOOL_NAME: &str = "xero_preflight_noop";
const AZURE_OPENAI_PROVIDER_ID: &str = "azure_openai";
const DEEPSEEK_PROVIDER_ID: &str = "deepseek";
const GITHUB_MODELS_PROVIDER_ID: &str = "github_models";
const OPENAI_API_PROVIDER_ID: &str = "openai_api";
const OPENROUTER_PROVIDER_ID: &str = "openrouter";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ProviderPreflightStatus {
    Passed,
    Warning,
    Failed,
    Skipped,
}

impl ProviderPreflightStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Warning => "warning",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderPreflightSource {
    LiveProbe,
    LiveCatalog,
    CachedProbe,
    StaticManual,
    Unavailable,
}

impl ProviderPreflightSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LiveProbe => "live_probe",
            Self::LiveCatalog => "live_catalog",
            Self::CachedProbe => "cached_probe",
            Self::StaticManual => "static_manual",
            Self::Unavailable => "unavailable",
        }
    }

    fn can_green_light_static_capabilities(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderPreflightRequiredFeatures {
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub tool_calls: bool,
    #[serde(default)]
    pub reasoning_controls: bool,
    #[serde(default)]
    pub attachments: bool,
}

impl ProviderPreflightRequiredFeatures {
    pub fn owned_agent_text_turn() -> Self {
        Self {
            streaming: true,
            tool_calls: true,
            reasoning_controls: false,
            attachments: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderPreflightErrorClass {
    Authentication,
    Authorization,
    EndpointUnreachable,
    ModelUnavailable,
    RateLimited,
    ProviderRejectedRequest,
    ProviderServerError,
    Decode,
    Unknown,
}

impl ProviderPreflightErrorClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Authentication => "authentication",
            Self::Authorization => "authorization",
            Self::EndpointUnreachable => "endpoint_unreachable",
            Self::ModelUnavailable => "model_unavailable",
            Self::RateLimited => "rate_limited",
            Self::ProviderRejectedRequest => "provider_rejected_request",
            Self::ProviderServerError => "provider_server_error",
            Self::Decode => "decode",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderPreflightError {
    pub code: String,
    pub message: String,
    pub class: ProviderPreflightErrorClass,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderPreflightCheck {
    pub check_id: String,
    pub status: ProviderPreflightStatus,
    pub code: String,
    pub message: String,
    pub source: ProviderPreflightSource,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderPreflightSnapshot {
    pub contract_version: u32,
    pub profile_id: String,
    pub provider_id: String,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_binding: Option<ProviderPreflightCacheBinding>,
    pub source: ProviderPreflightSource,
    pub checked_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age_seconds: Option<i64>,
    pub ttl_seconds: i64,
    pub stale: bool,
    pub required_features: ProviderPreflightRequiredFeatures,
    pub capabilities: ProviderCapabilityCatalog,
    pub checks: Vec<ProviderPreflightCheck>,
    pub status: ProviderPreflightStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderPreflightCacheBinding {
    pub endpoint_fingerprint: String,
    pub account_class: String,
    pub required_features_fingerprint: String,
    pub cache_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPreflightInput {
    pub profile_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub source: ProviderPreflightSource,
    pub checked_at: String,
    pub age_seconds: Option<i64>,
    pub ttl_seconds: Option<i64>,
    pub required_features: ProviderPreflightRequiredFeatures,
    pub capabilities: ProviderCapabilityCatalog,
    pub credential_ready: Option<bool>,
    pub endpoint_reachable: Option<bool>,
    pub model_available: Option<bool>,
    pub streaming_route_available: Option<bool>,
    pub tool_schema_accepted: Option<bool>,
    pub reasoning_controls_accepted: Option<bool>,
    pub attachments_accepted: Option<bool>,
    pub context_limit_known: Option<bool>,
    pub provider_error: Option<ProviderPreflightError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleProviderPreflightProbeRequest {
    pub profile_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub timeout_ms: u64,
    pub required_features: ProviderPreflightRequiredFeatures,
    pub credential_proof: Option<String>,
    pub context_window_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub context_limit_source: Option<String>,
    pub context_limit_confidence: Option<String>,
    pub thinking_supported: bool,
    pub thinking_efforts: Vec<String>,
    pub thinking_default_effort: Option<String>,
}

pub fn provider_preflight_snapshot(input: ProviderPreflightInput) -> ProviderPreflightSnapshot {
    let ttl_seconds = input
        .ttl_seconds
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_PROVIDER_PREFLIGHT_TTL_SECONDS);
    let stale = input.age_seconds.is_some_and(|age| age > ttl_seconds)
        || matches!(input.source, ProviderPreflightSource::Unavailable);
    let mut checks = vec![
        boolean_check(
            &input,
            "provider_preflight_credentials",
            "credentials",
            input.credential_ready,
            BooleanCheckMessages {
                passed: "Credential or ambient auth is available for the selected provider path.",
                unknown: "Credential or ambient auth was not verified for the selected provider path.",
                failed: "Credential or ambient auth is missing for the selected provider path.",
            },
            true,
        ),
        boolean_check(
            &input,
            "provider_preflight_endpoint",
            "endpoint",
            input.endpoint_reachable,
            BooleanCheckMessages {
                passed: "Provider endpoint reachability was verified.",
                unknown: "Provider endpoint reachability has not been verified by a live probe.",
                failed: "Provider endpoint reachability failed for the selected provider path.",
            },
            true,
        ),
        boolean_check(
            &input,
            "provider_preflight_model",
            "model",
            input.model_available,
            BooleanCheckMessages {
                passed: "Selected model was verified on the selected provider path.",
                unknown: "Selected model existence has not been verified by a live provider source.",
                failed: "Selected model is unavailable on the selected provider path.",
            },
            false,
        ),
        feature_check(
            &input,
            "provider_preflight_streaming",
            "streaming route",
            input.required_features.streaming,
            input.streaming_route_available,
            input.capabilities.capabilities.streaming.status.as_str(),
        ),
        feature_check(
            &input,
            "provider_preflight_tool_schema",
            "minimal tool-call schema",
            input.required_features.tool_calls,
            input.tool_schema_accepted,
            input.capabilities.capabilities.tool_calls.status.as_str(),
        ),
        feature_check(
            &input,
            "provider_preflight_reasoning",
            "reasoning controls",
            input.required_features.reasoning_controls,
            input.reasoning_controls_accepted,
            input.capabilities.capabilities.reasoning.status.as_str(),
        ),
        feature_check(
            &input,
            "provider_preflight_attachments",
            "attachment route",
            input.required_features.attachments,
            input.attachments_accepted,
            input.capabilities.capabilities.attachments.status.as_str(),
        ),
        boolean_check(
            &input,
            "provider_preflight_context_limit",
            "context limit",
            input.context_limit_known,
            BooleanCheckMessages {
                passed: "Context-limit source and confidence are available for the selected model.",
                unknown: "Context-limit source or confidence is only partially known for the selected model.",
                failed: "Context limits are unavailable for the selected model.",
            },
            false,
        ),
    ];

    if let Some(error) = input.provider_error.as_ref() {
        checks.push(ProviderPreflightCheck {
            check_id: check_id(&input, "provider_preflight_provider_error"),
            status: ProviderPreflightStatus::Failed,
            code: "provider_preflight_provider_error".into(),
            message: format!(
                "Provider preflight failed with {}: {}",
                error.class.as_str(),
                error.message
            ),
            source: input.source,
            retryable: error.retryable,
        });
    }

    let status = summarize_preflight_status(&checks);
    ProviderPreflightSnapshot {
        contract_version: PROVIDER_PREFLIGHT_CONTRACT_VERSION,
        profile_id: normalize_or_default(input.profile_id, "default"),
        provider_id: normalize_or_default(input.provider_id, "unknown"),
        model_id: normalize_or_default(input.model_id, "unknown-model"),
        cache_binding: None,
        source: input.source,
        checked_at: normalize_or_default(input.checked_at, "unknown"),
        age_seconds: input.age_seconds,
        ttl_seconds,
        stale,
        required_features: input.required_features,
        capabilities: input.capabilities,
        checks,
        status,
    }
}

pub fn provider_preflight_cache_binding(
    provider_id: &str,
    model_id: &str,
    endpoint_fingerprint: &str,
    account_class: &str,
    required_features: &ProviderPreflightRequiredFeatures,
) -> ProviderPreflightCacheBinding {
    let feature_json = serde_json::to_string(required_features)
        .unwrap_or_else(|_| "provider_preflight_required_features_unserializable".into());
    let required_features_fingerprint =
        crate::runtime_trace_id("provider-preflight-features", &[&feature_json]);
    let cache_key = crate::runtime_trace_id(
        "provider-preflight-cache",
        &[
            provider_id.trim(),
            model_id.trim(),
            endpoint_fingerprint.trim(),
            account_class.trim(),
            &required_features_fingerprint,
        ],
    );

    ProviderPreflightCacheBinding {
        endpoint_fingerprint: normalize_or_default(
            endpoint_fingerprint.trim().to_owned(),
            "unknown_endpoint",
        ),
        account_class: normalize_or_default(account_class.trim().to_owned(), "unknown_account"),
        required_features_fingerprint,
        cache_key,
    }
}

pub fn bind_provider_preflight_cache(
    mut snapshot: ProviderPreflightSnapshot,
    endpoint_fingerprint: &str,
    account_class: &str,
) -> ProviderPreflightSnapshot {
    snapshot.cache_binding = Some(provider_preflight_cache_binding(
        &snapshot.provider_id,
        &snapshot.model_id,
        endpoint_fingerprint,
        account_class,
        &snapshot.required_features,
    ));
    snapshot
}

pub fn provider_preflight_blockers(
    snapshot: &ProviderPreflightSnapshot,
) -> Vec<ProviderPreflightCheck> {
    provider_preflight_admission_blockers(snapshot, &snapshot.required_features)
}

pub fn provider_preflight_admission_blockers(
    snapshot: &ProviderPreflightSnapshot,
    required_features: &ProviderPreflightRequiredFeatures,
) -> Vec<ProviderPreflightCheck> {
    let mut blockers = snapshot
        .checks
        .iter()
        .filter(|check| {
            (check.status == ProviderPreflightStatus::Failed
                && matches!(
                    check.code.as_str(),
                    "provider_preflight_credentials"
                        | "provider_preflight_endpoint"
                        | "provider_preflight_model"
                        | "provider_preflight_streaming"
                        | "provider_preflight_tool_schema"
                        | "provider_preflight_reasoning"
                        | "provider_preflight_attachments"
                        | "provider_preflight_provider_error"
                ))
                || (snapshot.stale
                    && matches!(
                        check.code.as_str(),
                        "provider_preflight_tool_schema" | "provider_preflight_model"
                    ))
        })
        .cloned()
        .collect::<Vec<_>>();

    if &snapshot.required_features != required_features {
        blockers.push(admission_failure(
            snapshot,
            "provider_preflight_required_features_mismatch",
            "Cached provider preflight was recorded for a different required feature set.",
            false,
        ));
    }

    if matches!(snapshot.source, ProviderPreflightSource::Unavailable) {
        blockers.push(admission_failure(
            snapshot,
            "provider_preflight_source_unavailable",
            "Provider preflight is unavailable for the selected provider path.",
            true,
        ));
    }

    if matches!(snapshot.source, ProviderPreflightSource::StaticManual)
        && required_features.requires_live_feature_probe()
    {
        blockers.push(admission_failure(
            snapshot,
            "provider_preflight_static_manual_not_admissible",
            "Static or manual provider capability data cannot admit a production run that requires live provider features.",
            false,
        ));
    }

    if matches!(snapshot.source, ProviderPreflightSource::CachedProbe) && snapshot.stale {
        blockers.push(admission_failure(
            snapshot,
            "provider_preflight_cached_probe_stale",
            "Cached provider preflight is stale for the selected provider path.",
            true,
        ));
    }

    for (required, code) in required_feature_checks(required_features) {
        if !required {
            continue;
        }
        let check = snapshot.checks.iter().find(|check| check.code == code);
        if !matches!(
            check.map(|check| check.status),
            Some(ProviderPreflightStatus::Passed)
        ) {
            if let Some(check) = check {
                blockers.push(check.clone());
            } else {
                blockers.push(admission_failure(
                    snapshot,
                    code,
                    format!("Required provider feature `{code}` was not present in preflight."),
                    false,
                ));
            }
        }
    }

    blockers
}

pub fn provider_preflight_snapshot_as_cached_probe(
    mut snapshot: ProviderPreflightSnapshot,
) -> ProviderPreflightSnapshot {
    if matches!(snapshot.source, ProviderPreflightSource::LiveProbe) {
        snapshot.source = ProviderPreflightSource::CachedProbe;
        for check in &mut snapshot.checks {
            check.source = ProviderPreflightSource::CachedProbe;
        }
    }
    if let Some(age_seconds) = age_seconds_since_checked_at(&snapshot.checked_at) {
        snapshot.age_seconds = Some(age_seconds);
    }
    snapshot.stale = snapshot
        .age_seconds
        .is_some_and(|age| age > snapshot.ttl_seconds);
    snapshot
}

pub fn run_openai_compatible_provider_preflight_probe(
    request: OpenAiCompatibleProviderPreflightProbeRequest,
) -> ProviderPreflightSnapshot {
    let credential_ready = request
        .api_key
        .as_deref()
        .is_some_and(|key| !key.trim().is_empty())
        || is_local_http_endpoint(&request.base_url);
    let capability_input = ProviderCapabilityCatalogInput {
        provider_id: request.provider_id.clone(),
        model_id: request.model_id.clone(),
        catalog_source: "live".into(),
        fetched_at: Some(crate::now_timestamp()),
        last_success_at: Some(crate::now_timestamp()),
        cache_age_seconds: Some(0),
        cache_ttl_seconds: Some(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
        credential_proof: request.credential_proof.clone().or_else(|| {
            if credential_ready {
                Some("live_probe_credentials_available".into())
            } else {
                None
            }
        }),
        context_window_tokens: request.context_window_tokens,
        max_output_tokens: request.max_output_tokens,
        context_limit_source: request
            .context_limit_source
            .clone()
            .or_else(|| request.context_window_tokens.map(|_| "live_probe".into())),
        context_limit_confidence: request
            .context_limit_confidence
            .clone()
            .or_else(|| request.context_window_tokens.map(|_| "medium".into())),
        thinking_supported: request.thinking_supported,
        thinking_efforts: request.thinking_efforts.clone(),
        thinking_default_effort: request.thinking_default_effort.clone(),
    };
    let capabilities = crate::provider_capability_catalog(capability_input);

    if !credential_ready {
        return provider_preflight_snapshot(ProviderPreflightInput {
            profile_id: request.profile_id,
            provider_id: request.provider_id,
            model_id: request.model_id,
            source: ProviderPreflightSource::LiveProbe,
            checked_at: crate::now_timestamp(),
            age_seconds: Some(0),
            ttl_seconds: None,
            required_features: request.required_features,
            capabilities,
            credential_ready: Some(false),
            endpoint_reachable: None,
            model_available: None,
            streaming_route_available: None,
            tool_schema_accepted: None,
            reasoning_controls_accepted: None,
            attachments_accepted: None,
            context_limit_known: request.context_window_tokens.map(|_| true),
            provider_error: Some(ProviderPreflightError {
                code: "provider_preflight_credentials_missing".into(),
                message: "No API key or local endpoint auth path is available for the selected provider profile.".into(),
                class: ProviderPreflightErrorClass::Authentication,
                retryable: false,
            }),
        });
    }

    let url = match openai_compatible_preflight_chat_url(&request.base_url) {
        Ok(url) => url,
        Err(error) => {
            return provider_preflight_snapshot(ProviderPreflightInput {
                profile_id: request.profile_id,
                provider_id: request.provider_id,
                model_id: request.model_id,
                source: ProviderPreflightSource::LiveProbe,
                checked_at: crate::now_timestamp(),
                age_seconds: Some(0),
                ttl_seconds: None,
                required_features: request.required_features,
                capabilities,
                credential_ready: Some(true),
                endpoint_reachable: Some(false),
                model_available: None,
                streaming_route_available: None,
                tool_schema_accepted: None,
                reasoning_controls_accepted: None,
                attachments_accepted: None,
                context_limit_known: request.context_window_tokens.map(|_| true),
                provider_error: Some(ProviderPreflightError {
                    code: error.code,
                    message: error.message,
                    class: ProviderPreflightErrorClass::EndpointUnreachable,
                    retryable: false,
                }),
            });
        }
    };

    let client = match Client::builder()
        .timeout(Duration::from_millis(normalize_preflight_timeout(
            request.timeout_ms,
        )))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return provider_preflight_snapshot(ProviderPreflightInput {
                profile_id: request.profile_id,
                provider_id: request.provider_id,
                model_id: request.model_id,
                source: ProviderPreflightSource::LiveProbe,
                checked_at: crate::now_timestamp(),
                age_seconds: Some(0),
                ttl_seconds: None,
                required_features: request.required_features,
                capabilities,
                credential_ready: Some(true),
                endpoint_reachable: Some(false),
                model_available: None,
                streaming_route_available: None,
                tool_schema_accepted: None,
                reasoning_controls_accepted: None,
                attachments_accepted: None,
                context_limit_known: request.context_window_tokens.map(|_| true),
                provider_error: Some(ProviderPreflightError {
                    code: "provider_preflight_http_client_failed".into(),
                    message: format!(
                        "Xero could not build a provider preflight HTTP client: {error}"
                    ),
                    class: ProviderPreflightErrorClass::Unknown,
                    retryable: true,
                }),
            });
        }
    };

    let body = openai_compatible_preflight_body(&request);
    let mut http_request = client.post(url).json(&body);
    if let Some(api_key) = request
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
    {
        http_request = http_request.bearer_auth(api_key);
    }

    match http_request.send() {
        Ok(response) => {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            if status.is_success() {
                provider_preflight_snapshot(ProviderPreflightInput {
                    profile_id: request.profile_id,
                    provider_id: request.provider_id,
                    model_id: request.model_id,
                    source: ProviderPreflightSource::LiveProbe,
                    checked_at: crate::now_timestamp(),
                    age_seconds: Some(0),
                    ttl_seconds: None,
                    required_features: request.required_features.clone(),
                    capabilities,
                    credential_ready: Some(true),
                    endpoint_reachable: Some(true),
                    model_available: Some(true),
                    streaming_route_available: request.required_features.streaming.then_some(true),
                    tool_schema_accepted: request.required_features.tool_calls.then_some(true),
                    reasoning_controls_accepted: request
                        .required_features
                        .reasoning_controls
                        .then_some(true),
                    attachments_accepted: request.required_features.attachments.then_some(false),
                    context_limit_known: Some(request.context_window_tokens.is_some()),
                    provider_error: None,
                })
            } else {
                let error = classify_provider_preflight_http_error(status.as_u16(), &text);
                provider_preflight_snapshot(ProviderPreflightInput {
                    profile_id: request.profile_id,
                    provider_id: request.provider_id,
                    model_id: request.model_id,
                    source: ProviderPreflightSource::LiveProbe,
                    checked_at: crate::now_timestamp(),
                    age_seconds: Some(0),
                    ttl_seconds: None,
                    required_features: request.required_features,
                    capabilities,
                    credential_ready: Some(true),
                    endpoint_reachable: Some(status.as_u16() != 404),
                    model_available: Some(!matches!(
                        error.class,
                        ProviderPreflightErrorClass::ModelUnavailable
                    )),
                    streaming_route_available: None,
                    tool_schema_accepted: None,
                    reasoning_controls_accepted: None,
                    attachments_accepted: None,
                    context_limit_known: request.context_window_tokens.map(|_| true),
                    provider_error: Some(error),
                })
            }
        }
        Err(error) => provider_preflight_snapshot(ProviderPreflightInput {
            profile_id: request.profile_id,
            provider_id: request.provider_id,
            model_id: request.model_id,
            source: ProviderPreflightSource::LiveProbe,
            checked_at: crate::now_timestamp(),
            age_seconds: Some(0),
            ttl_seconds: None,
            required_features: request.required_features,
            capabilities,
            credential_ready: Some(true),
            endpoint_reachable: Some(false),
            model_available: None,
            streaming_route_available: None,
            tool_schema_accepted: None,
            reasoning_controls_accepted: None,
            attachments_accepted: None,
            context_limit_known: request.context_window_tokens.map(|_| true),
            provider_error: Some(ProviderPreflightError {
                code: "provider_preflight_request_failed".into(),
                message: format!("Provider preflight request failed: {error}"),
                class: ProviderPreflightErrorClass::EndpointUnreachable,
                retryable: true,
            }),
        }),
    }
}

pub fn openai_compatible_preflight_chat_url(base_url: &str) -> CoreResult<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(CoreError::invalid_request(
            "provider_preflight_base_url_missing",
            "A provider base URL is required for live provider preflight.",
        ));
    }
    if trimmed.starts_with("http://") && !is_local_http_endpoint(trimmed) {
        return Err(CoreError::invalid_request(
            "provider_preflight_base_url_insecure",
            "Live provider preflight allows plain HTTP only for localhost endpoints.",
        ));
    }
    Ok(if trimmed.ends_with("/chat/completions") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/chat/completions")
    })
}

struct BooleanCheckMessages<'a> {
    passed: &'a str,
    unknown: &'a str,
    failed: &'a str,
}

fn boolean_check(
    input: &ProviderPreflightInput,
    code: &str,
    label: &str,
    value: Option<bool>,
    messages: BooleanCheckMessages<'_>,
    retryable_on_failure: bool,
) -> ProviderPreflightCheck {
    match value {
        Some(true) => ProviderPreflightCheck {
            check_id: check_id(input, code),
            status: ProviderPreflightStatus::Passed,
            code: code.into(),
            message: messages.passed.into(),
            source: input.source,
            retryable: false,
        },
        Some(false) => ProviderPreflightCheck {
            check_id: check_id(input, code),
            status: ProviderPreflightStatus::Failed,
            code: code.into(),
            message: messages.failed.into(),
            source: input.source,
            retryable: retryable_on_failure,
        },
        None => ProviderPreflightCheck {
            check_id: check_id(input, code),
            status: ProviderPreflightStatus::Warning,
            code: code.into(),
            message: format!("{} Required check: {label}.", messages.unknown),
            source: input.source,
            retryable: false,
        },
    }
}

fn feature_check(
    input: &ProviderPreflightInput,
    code: &str,
    label: &str,
    required: bool,
    probed: Option<bool>,
    capability_status: &str,
) -> ProviderPreflightCheck {
    if !required {
        return ProviderPreflightCheck {
            check_id: check_id(input, code),
            status: ProviderPreflightStatus::Skipped,
            code: code.into(),
            message: format!("{label} is not required for this run."),
            source: input.source,
            retryable: false,
        };
    }

    match probed {
        Some(true) => ProviderPreflightCheck {
            check_id: check_id(input, code),
            status: ProviderPreflightStatus::Passed,
            code: code.into(),
            message: format!("{label} was accepted by a live provider probe."),
            source: input.source,
            retryable: false,
        },
        Some(false) => ProviderPreflightCheck {
            check_id: check_id(input, code),
            status: ProviderPreflightStatus::Failed,
            code: code.into(),
            message: format!("{label} was rejected by the selected provider path."),
            source: input.source,
            retryable: false,
        },
        None => match capability_status {
            "supported" | "probed" if input.source.can_green_light_static_capabilities() => {
                ProviderPreflightCheck {
                    check_id: check_id(input, code),
                    status: ProviderPreflightStatus::Passed,
                    code: code.into(),
                    message: format!(
                        "{label} is supported and the selected provider path has a live preflight."
                    ),
                    source: input.source,
                    retryable: false,
                }
            }
            "supported" | "probed" => ProviderPreflightCheck {
                check_id: check_id(input, code),
                status: ProviderPreflightStatus::Warning,
                code: code.into(),
                message: format!(
                    "{label} is known from capability metadata but was not proven by a live preflight probe."
                ),
                source: input.source,
                retryable: false,
            },
            "not_applicable" => ProviderPreflightCheck {
                check_id: check_id(input, code),
                status: ProviderPreflightStatus::Failed,
                code: code.into(),
                message: format!("{label} is not applicable to this provider path."),
                source: input.source,
                retryable: false,
            },
            "unknown" => ProviderPreflightCheck {
                check_id: check_id(input, code),
                status: ProviderPreflightStatus::Warning,
                code: code.into(),
                message: format!("{label} support is unknown for the selected provider path."),
                source: input.source,
                retryable: false,
            },
            _ => ProviderPreflightCheck {
                check_id: check_id(input, code),
                status: ProviderPreflightStatus::Failed,
                code: code.into(),
                message: format!("{label} is unavailable for the selected provider path."),
                source: input.source,
                retryable: false,
            },
        },
    }
}

impl ProviderPreflightRequiredFeatures {
    fn requires_live_feature_probe(&self) -> bool {
        self.streaming || self.tool_calls || self.reasoning_controls || self.attachments
    }
}

fn required_feature_checks(
    required_features: &ProviderPreflightRequiredFeatures,
) -> [(bool, &'static str); 4] {
    [
        (required_features.streaming, "provider_preflight_streaming"),
        (
            required_features.tool_calls,
            "provider_preflight_tool_schema",
        ),
        (
            required_features.reasoning_controls,
            "provider_preflight_reasoning",
        ),
        (
            required_features.attachments,
            "provider_preflight_attachments",
        ),
    ]
}

fn admission_failure(
    snapshot: &ProviderPreflightSnapshot,
    code: impl Into<String>,
    message: impl Into<String>,
    retryable: bool,
) -> ProviderPreflightCheck {
    let code = code.into();
    ProviderPreflightCheck {
        check_id: format!(
            "provider-preflight:v{}:{}:{}:{}:{}",
            PROVIDER_PREFLIGHT_CONTRACT_VERSION,
            normalize_for_id(&snapshot.profile_id),
            normalize_for_id(&snapshot.provider_id),
            normalize_for_id(&snapshot.model_id),
            normalize_for_id(&code)
        ),
        status: ProviderPreflightStatus::Failed,
        code,
        message: message.into(),
        source: snapshot.source,
        retryable,
    }
}

fn openai_compatible_preflight_body(
    request: &OpenAiCompatibleProviderPreflightProbeRequest,
) -> JsonValue {
    let mut body = JsonMap::new();
    body.insert("model".into(), json!(&request.model_id));
    body.insert(
        "messages".into(),
        json!([
            {
                "role": "system",
                "content": "You are verifying provider compatibility for Xero. Return a tiny acknowledgement."
            },
            {
                "role": "user",
                "content": "preflight"
            }
        ]),
    );
    body.insert("stream".into(), json!(request.required_features.streaming));
    if request.required_features.streaming
        && openai_compatible_preflight_supports_stream_options(&request.provider_id)
    {
        body.insert("stream_options".into(), json!({ "include_usage": true }));
    }
    if request.required_features.tool_calls {
        body.insert("tools".into(), json!([preflight_tool_schema()]));
        body.insert("tool_choice".into(), json!("none"));
    }
    if request.provider_id == DEEPSEEK_PROVIDER_ID {
        body.insert("thinking".into(), json!({ "type": "enabled" }));
        if request.required_features.reasoning_controls {
            body.insert("reasoning_effort".into(), json!("high"));
        }
    } else if request.required_features.reasoning_controls {
        body.insert("reasoning".into(), json!({ "effort": "low" }));
    }
    JsonValue::Object(body)
}

fn openai_compatible_preflight_supports_stream_options(provider_id: &str) -> bool {
    matches!(
        provider_id,
        OPENAI_API_PROVIDER_ID
            | OPENROUTER_PROVIDER_ID
            | DEEPSEEK_PROVIDER_ID
            | GITHUB_MODELS_PROVIDER_ID
            | AZURE_OPENAI_PROVIDER_ID
    )
}

fn preflight_tool_schema() -> JsonValue {
    json!({
        "type": "function",
        "function": {
            "name": PREFLIGHT_PROBE_TOOL_NAME,
            "description": "No-op compatibility probe. The model should not call this tool.",
            "parameters": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }
    })
}

fn classify_provider_preflight_http_error(status: u16, body: &str) -> ProviderPreflightError {
    let body = body.trim();
    let message = if body.is_empty() {
        format!("Provider returned HTTP {status}.")
    } else {
        format!(
            "Provider returned HTTP {status}: {}",
            truncate_text(body, 512)
        )
    };
    let class = match status {
        401 => ProviderPreflightErrorClass::Authentication,
        403 => ProviderPreflightErrorClass::Authorization,
        404 => ProviderPreflightErrorClass::ModelUnavailable,
        429 => ProviderPreflightErrorClass::RateLimited,
        400 | 422 => ProviderPreflightErrorClass::ProviderRejectedRequest,
        500..=599 => ProviderPreflightErrorClass::ProviderServerError,
        _ => ProviderPreflightErrorClass::Unknown,
    };
    let retryable = matches!(
        class,
        ProviderPreflightErrorClass::EndpointUnreachable
            | ProviderPreflightErrorClass::RateLimited
            | ProviderPreflightErrorClass::ProviderServerError
            | ProviderPreflightErrorClass::Unknown
    );
    ProviderPreflightError {
        code: format!("provider_preflight_http_{status}"),
        message,
        class,
        retryable,
    }
}

fn is_local_http_endpoint(base_url: &str) -> bool {
    let lower = base_url.to_ascii_lowercase();
    lower.starts_with("http://localhost")
        || lower.starts_with("http://127.")
        || lower.starts_with("http://[::1]")
        || lower.starts_with("http://0.0.0.0")
}

fn normalize_preflight_timeout(timeout_ms: u64) -> u64 {
    if timeout_ms == 0 {
        DEFAULT_PROVIDER_PREFLIGHT_TIMEOUT_MS
    } else {
        timeout_ms
    }
}

fn age_seconds_since_checked_at(checked_at: &str) -> Option<i64> {
    let checked_seconds = checked_at.strip_prefix("unix:")?.parse::<u64>().ok()?;
    let now_seconds = crate::now_timestamp()
        .strip_prefix("unix:")?
        .parse::<u64>()
        .ok()?;
    Some(now_seconds.saturating_sub(checked_seconds) as i64)
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut truncated = text.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn summarize_preflight_status(checks: &[ProviderPreflightCheck]) -> ProviderPreflightStatus {
    if checks
        .iter()
        .any(|check| check.status == ProviderPreflightStatus::Failed)
    {
        ProviderPreflightStatus::Failed
    } else if checks
        .iter()
        .any(|check| check.status == ProviderPreflightStatus::Warning)
    {
        ProviderPreflightStatus::Warning
    } else {
        ProviderPreflightStatus::Passed
    }
}

fn check_id(input: &ProviderPreflightInput, code: &str) -> String {
    format!(
        "provider-preflight:v{}:{}:{}:{}:{}",
        PROVIDER_PREFLIGHT_CONTRACT_VERSION,
        normalize_for_id(&input.profile_id),
        normalize_for_id(&input.provider_id),
        normalize_for_id(&input.model_id),
        normalize_for_id(code)
    )
}

fn normalize_or_default(value: String, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.into()
    } else {
        trimmed.into()
    }
}

fn normalize_for_id(value: &str) -> String {
    let normalized = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    if normalized.is_empty() {
        "unknown".into()
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        provider_capability_catalog, ProviderCapabilityCatalogInput,
        DEFAULT_PROVIDER_CATALOG_TTL_SECONDS,
    };

    fn capabilities(source: &str) -> ProviderCapabilityCatalog {
        provider_capability_catalog(ProviderCapabilityCatalogInput {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-5.4".into(),
            catalog_source: source.into(),
            fetched_at: None,
            last_success_at: None,
            cache_age_seconds: None,
            cache_ttl_seconds: Some(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
            credential_proof: Some("api_key_env_recorded".into()),
            context_window_tokens: Some(128_000),
            max_output_tokens: Some(16_384),
            context_limit_source: Some("live_catalog".into()),
            context_limit_confidence: Some("high".into()),
            thinking_supported: true,
            thinking_efforts: vec!["low".into(), "medium".into()],
            thinking_default_effort: Some("medium".into()),
        })
    }

    #[test]
    fn static_capability_data_warns_instead_of_green_lighting_tool_schema() {
        let snapshot = provider_preflight_snapshot(ProviderPreflightInput {
            profile_id: "openrouter-work".into(),
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-5.4".into(),
            source: ProviderPreflightSource::StaticManual,
            checked_at: "2026-05-04T00:00:00Z".into(),
            age_seconds: None,
            ttl_seconds: None,
            required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
            capabilities: capabilities("manual"),
            credential_ready: Some(true),
            endpoint_reachable: None,
            model_available: None,
            streaming_route_available: None,
            tool_schema_accepted: None,
            reasoning_controls_accepted: None,
            attachments_accepted: None,
            context_limit_known: Some(true),
            provider_error: None,
        });

        let tool = snapshot
            .checks
            .iter()
            .find(|check| check.code == "provider_preflight_tool_schema")
            .expect("tool schema check");
        assert_eq!(tool.status, ProviderPreflightStatus::Warning);
        assert_eq!(snapshot.status, ProviderPreflightStatus::Warning);
        assert!(provider_preflight_blockers(&snapshot)
            .iter()
            .any(|check| { check.code == "provider_preflight_static_manual_not_admissible" }));
    }

    #[test]
    fn live_probe_can_pass_required_tool_schema() {
        let snapshot = provider_preflight_snapshot(ProviderPreflightInput {
            profile_id: "openrouter-work".into(),
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-5.4".into(),
            source: ProviderPreflightSource::LiveProbe,
            checked_at: "2026-05-04T00:00:00Z".into(),
            age_seconds: Some(30),
            ttl_seconds: None,
            required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
            capabilities: capabilities("live"),
            credential_ready: Some(true),
            endpoint_reachable: Some(true),
            model_available: Some(true),
            streaming_route_available: Some(true),
            tool_schema_accepted: Some(true),
            reasoning_controls_accepted: None,
            attachments_accepted: None,
            context_limit_known: Some(true),
            provider_error: None,
        });

        assert_eq!(snapshot.status, ProviderPreflightStatus::Passed);
        assert!(provider_preflight_blockers(&snapshot).is_empty());
    }

    #[test]
    fn fresh_cached_probe_passes_when_required_features_match() {
        let live = provider_preflight_snapshot(ProviderPreflightInput {
            profile_id: "openrouter-work".into(),
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-5.4".into(),
            source: ProviderPreflightSource::LiveProbe,
            checked_at: "2026-05-04T00:00:00Z".into(),
            age_seconds: Some(30),
            ttl_seconds: Some(120),
            required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
            capabilities: capabilities("live"),
            credential_ready: Some(true),
            endpoint_reachable: Some(true),
            model_available: Some(true),
            streaming_route_available: Some(true),
            tool_schema_accepted: Some(true),
            reasoning_controls_accepted: None,
            attachments_accepted: None,
            context_limit_known: Some(true),
            provider_error: None,
        });
        let cached = provider_preflight_snapshot_as_cached_probe(live);

        assert_eq!(cached.source, ProviderPreflightSource::CachedProbe);
        assert!(provider_preflight_blockers(&cached).is_empty());
    }

    #[test]
    fn cached_probe_blocks_when_required_features_do_not_match() {
        let mut required = ProviderPreflightRequiredFeatures::owned_agent_text_turn();
        required.attachments = true;
        let cached = provider_preflight_snapshot(ProviderPreflightInput {
            profile_id: "openrouter-work".into(),
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-5.4".into(),
            source: ProviderPreflightSource::CachedProbe,
            checked_at: "2026-05-04T00:00:00Z".into(),
            age_seconds: Some(30),
            ttl_seconds: Some(120),
            required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
            capabilities: capabilities("live"),
            credential_ready: Some(true),
            endpoint_reachable: Some(true),
            model_available: Some(true),
            streaming_route_available: Some(true),
            tool_schema_accepted: Some(true),
            reasoning_controls_accepted: None,
            attachments_accepted: None,
            context_limit_known: Some(true),
            provider_error: None,
        });

        let blockers = provider_preflight_admission_blockers(&cached, &required);
        assert!(blockers
            .iter()
            .any(|check| { check.code == "provider_preflight_required_features_mismatch" }));
    }

    #[test]
    fn required_attachments_fail_closed_when_unproven() {
        let mut required = ProviderPreflightRequiredFeatures::owned_agent_text_turn();
        required.attachments = true;
        let snapshot = provider_preflight_snapshot(ProviderPreflightInput {
            profile_id: "openrouter-work".into(),
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-5.4".into(),
            source: ProviderPreflightSource::LiveProbe,
            checked_at: "2026-05-04T00:00:00Z".into(),
            age_seconds: Some(30),
            ttl_seconds: Some(120),
            required_features: required,
            capabilities: capabilities("live"),
            credential_ready: Some(true),
            endpoint_reachable: Some(true),
            model_available: Some(true),
            streaming_route_available: Some(true),
            tool_schema_accepted: Some(true),
            reasoning_controls_accepted: None,
            attachments_accepted: None,
            context_limit_known: Some(true),
            provider_error: None,
        });

        assert!(provider_preflight_blockers(&snapshot)
            .iter()
            .any(|check| { check.code == "provider_preflight_attachments" }));
    }

    #[test]
    fn provider_preflight_cache_binding_changes_with_binding_inputs() {
        let required = ProviderPreflightRequiredFeatures::owned_agent_text_turn();
        let base = provider_preflight_cache_binding(
            "openai_api",
            "gpt-5.4",
            "https://api.openai.com/v1",
            "api_key:2026-05-05",
            &required,
        );
        let different_endpoint = provider_preflight_cache_binding(
            "openai_api",
            "gpt-5.4",
            "http://127.0.0.1:11434/v1",
            "api_key:2026-05-05",
            &required,
        );
        let different_account = provider_preflight_cache_binding(
            "openai_api",
            "gpt-5.4",
            "https://api.openai.com/v1",
            "api_key:2026-05-06",
            &required,
        );
        let mut attachment_required = required.clone();
        attachment_required.attachments = true;
        let different_features = provider_preflight_cache_binding(
            "openai_api",
            "gpt-5.4",
            "https://api.openai.com/v1",
            "api_key:2026-05-05",
            &attachment_required,
        );

        assert_ne!(base.cache_key, different_endpoint.cache_key);
        assert_ne!(base.cache_key, different_account.cache_key);
        assert_ne!(base.cache_key, different_features.cache_key);
        assert_ne!(
            base.required_features_fingerprint,
            different_features.required_features_fingerprint
        );
    }

    #[test]
    fn deepseek_preflight_body_uses_thinking_controls_not_openai_reasoning() {
        let mut required = ProviderPreflightRequiredFeatures::owned_agent_text_turn();
        required.reasoning_controls = true;
        let body =
            openai_compatible_preflight_body(&OpenAiCompatibleProviderPreflightProbeRequest {
                profile_id: "deepseek-default".into(),
                provider_id: DEEPSEEK_PROVIDER_ID.into(),
                model_id: "deepseek-v4-pro".into(),
                base_url: "https://api.deepseek.com".into(),
                api_key: Some("test-key".into()),
                timeout_ms: 1_000,
                required_features: required,
                credential_proof: Some("test".into()),
                context_window_tokens: Some(1_000_000),
                max_output_tokens: Some(384_000),
                context_limit_source: Some("built_in_registry".into()),
                context_limit_confidence: Some("medium".into()),
                thinking_supported: true,
                thinking_efforts: vec!["high".into(), "x_high".into()],
                thinking_default_effort: Some("high".into()),
            });

        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["reasoning_effort"], "high");
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(
            body["tools"][0]["function"]["name"],
            PREFLIGHT_PROBE_TOOL_NAME
        );
        assert!(body.get("reasoning").is_none());
    }

    #[test]
    fn openrouter_preflight_body_keeps_openrouter_reasoning_shape() {
        let mut required = ProviderPreflightRequiredFeatures::owned_agent_text_turn();
        required.reasoning_controls = true;
        let body =
            openai_compatible_preflight_body(&OpenAiCompatibleProviderPreflightProbeRequest {
                profile_id: "openrouter-default".into(),
                provider_id: OPENROUTER_PROVIDER_ID.into(),
                model_id: "deepseek/deepseek-v4-pro".into(),
                base_url: "https://openrouter.ai/api/v1".into(),
                api_key: Some("test-key".into()),
                timeout_ms: 1_000,
                required_features: required,
                credential_proof: Some("test".into()),
                context_window_tokens: Some(1_048_576),
                max_output_tokens: Some(384_000),
                context_limit_source: Some("live_catalog".into()),
                context_limit_confidence: Some("high".into()),
                thinking_supported: true,
                thinking_efforts: vec!["high".into(), "x_high".into()],
                thinking_default_effort: Some("high".into()),
            });

        assert_eq!(body["reasoning"]["effort"], "low");
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert!(body.get("thinking").is_none());
        assert!(body.get("reasoning_effort").is_none());
    }
}
