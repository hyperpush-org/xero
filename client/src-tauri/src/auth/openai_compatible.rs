use std::time::Duration;

use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;

use super::{AuthDiagnostic, AuthFlowError};
use crate::{
    commands::{get_runtime_settings::RuntimeSettingsSnapshot, RuntimeAuthPhase},
    provider_credentials::ProviderCredentialProfile,
    runtime::{
        ResolvedRuntimeProvider, AZURE_OPENAI_PROVIDER_ID, GEMINI_AI_STUDIO_PROVIDER_ID,
        GEMINI_RUNTIME_KIND, GITHUB_MODELS_PROVIDER_ID, OLLAMA_PROVIDER_ID, OPENAI_API_PROVIDER_ID,
        OPENAI_COMPATIBLE_RUNTIME_KIND,
    },
};

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434/v1";
const DEFAULT_GITHUB_MODELS_BASE_URL: &str = "https://models.github.ai/inference";
const DEFAULT_GITHUB_MODELS_CATALOG_URL: &str = "https://models.github.ai/catalog/models";
const DEFAULT_GEMINI_AI_STUDIO_BASE_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/openai";

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleAuthConfig {
    pub openai_base_url: String,
    pub github_models_base_url: String,
    pub github_models_catalog_url: String,
    pub gemini_ai_studio_base_url: String,
    pub timeout: Duration,
}

impl Default for OpenAiCompatibleAuthConfig {
    fn default() -> Self {
        Self {
            openai_base_url: DEFAULT_OPENAI_BASE_URL.into(),
            github_models_base_url: DEFAULT_GITHUB_MODELS_BASE_URL.into(),
            github_models_catalog_url: DEFAULT_GITHUB_MODELS_CATALOG_URL.into(),
            gemini_ai_studio_base_url: DEFAULT_GEMINI_AI_STUDIO_BASE_URL.into(),
            timeout: Duration::from_secs(10),
        }
    }
}

impl OpenAiCompatibleAuthConfig {
    pub fn for_platform() -> Self {
        Self::default()
    }

    fn http_client(&self) -> Result<Client, AuthFlowError> {
        Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|error| {
                AuthFlowError::terminal(
                    "openai_compatible_http_client_unavailable",
                    RuntimeAuthPhase::Failed,
                    format!(
                        "Xero could not build the OpenAI-compatible HTTP client for the models probe: {error}"
                    ),
                )
            })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum OpenAiCompatibleModelListStrategy {
    Live,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOpenAiCompatibleEndpoint {
    pub provider_id: String,
    pub runtime_kind: String,
    pub preset_id: Option<String>,
    pub effective_base_url: String,
    pub api_version: Option<String>,
    pub model_list_strategy: OpenAiCompatibleModelListStrategy,
    pub model_list_url_override: Option<String>,
}

impl ResolvedOpenAiCompatibleEndpoint {
    pub fn models_url(&self) -> Result<Url, AuthFlowError> {
        if let Some(model_list_url) = self.model_list_url_override.as_deref() {
            return Url::parse(model_list_url).map_err(|error| {
                AuthFlowError::terminal(
                    "openai_compatible_models_url_invalid",
                    RuntimeAuthPhase::Failed,
                    format!(
                        "Xero could not build the {} catalog endpoint because URL `{model_list_url}` was invalid: {error}",
                        provider_display_label(self.provider_id.as_str())
                    ),
                )
            });
        }

        let base_url = normalize_base_url(&self.effective_base_url);
        let mut url = Url::parse(&format!("{base_url}/models")).map_err(|error| {
            AuthFlowError::terminal(
                "openai_compatible_base_url_invalid",
                RuntimeAuthPhase::Failed,
                format!(
                    "Xero could not build the OpenAI-compatible models endpoint because base URL `{}` was invalid: {error}",
                    self.effective_base_url
                ),
            )
        })?;

        if let Some(api_version) = self.api_version.as_deref() {
            url.query_pairs_mut()
                .append_pair("api-version", api_version);
        }

        Ok(url)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OpenAiCompatibleDiscoveredThinkingEffort {
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleDiscoveredThinkingCapability {
    pub supported: bool,
    pub effort_levels: Vec<OpenAiCompatibleDiscoveredThinkingEffort>,
    pub default_effort: Option<OpenAiCompatibleDiscoveredThinkingEffort>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleDiscoveredModel {
    pub id: String,
    pub display_name: String,
    pub thinking: OpenAiCompatibleDiscoveredThinkingCapability,
    pub context_window_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleRuntimeSessionBinding {
    pub provider_id: String,
    pub session_id: String,
    pub account_id: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenAiCompatibleBindOutcome {
    Ready(OpenAiCompatibleRuntimeSessionBinding),
    SignedOut(AuthDiagnostic),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenAiCompatibleReconcileOutcome {
    Authenticated(OpenAiCompatibleRuntimeSessionBinding),
    SignedOut(AuthDiagnostic),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelsResponse {
    data: Vec<ModelSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitHubCatalogModelSummary {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, alias = "context_window_tokens", alias = "context_length")]
    context_window_tokens: Option<u64>,
    #[serde(default, alias = "max_output_tokens", alias = "max_completion_tokens")]
    max_output_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelSummary {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    capabilities: OpenAiCompatibleCapabilities,
    #[serde(default, alias = "context_window_tokens", alias = "context_length")]
    context_window_tokens: Option<u64>,
    #[serde(default, alias = "max_input_tokens", alias = "max_context_length")]
    max_input_tokens: Option<u64>,
    #[serde(default, alias = "max_output_tokens", alias = "max_completion_tokens")]
    max_output_tokens: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenAiCompatibleCapabilities {
    #[serde(default)]
    reasoning: Option<OpenAiCompatibleThinkingPayload>,
    #[serde(default)]
    thinking: Option<OpenAiCompatibleThinkingPayload>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenAiCompatibleThinkingPayload {
    #[serde(default)]
    supported: bool,
    #[serde(default)]
    effort_options: Vec<String>,
    #[serde(default)]
    default_effort: Option<String>,
}

pub(crate) fn resolve_openai_compatible_endpoint_for_profile(
    profile: &ProviderCredentialProfile,
    config: &OpenAiCompatibleAuthConfig,
) -> Result<ResolvedOpenAiCompatibleEndpoint, AuthFlowError> {
    resolve_openai_compatible_endpoint(
        profile.provider_id.as_str(),
        profile.runtime_kind.as_str(),
        profile.preset_id.as_deref(),
        profile.base_url.as_deref(),
        profile.api_version.as_deref(),
        config,
    )
}

pub(crate) fn resolve_openai_compatible_endpoint_for_settings(
    provider: ResolvedRuntimeProvider,
    settings: &RuntimeSettingsSnapshot,
    config: &OpenAiCompatibleAuthConfig,
) -> Result<ResolvedOpenAiCompatibleEndpoint, AuthFlowError> {
    resolve_openai_compatible_endpoint(
        provider.provider_id,
        provider.runtime_kind,
        settings.preset_id.as_deref(),
        settings.base_url.as_deref(),
        settings.api_version.as_deref(),
        config,
    )
}

pub(crate) fn missing_openai_compatible_api_key_error(
    provider_id: &str,
    operation: &str,
) -> AuthFlowError {
    AuthFlowError::terminal(
        missing_api_key_code(provider_id),
        RuntimeAuthPhase::Failed,
        missing_api_key_message(provider_id, operation),
    )
}

pub(crate) fn bind_openai_compatible_runtime_session(
    provider: ResolvedRuntimeProvider,
    settings: &RuntimeSettingsSnapshot,
    config: &OpenAiCompatibleAuthConfig,
) -> Result<OpenAiCompatibleBindOutcome, AuthFlowError> {
    let endpoint = resolve_openai_compatible_endpoint_for_settings(provider, settings, config)?;
    let api_key = normalize_optional(settings.provider_api_key.as_deref());
    if api_key_required_for_endpoint(&endpoint) && api_key.is_none() {
        return Ok(OpenAiCompatibleBindOutcome::SignedOut(AuthDiagnostic {
            code: missing_api_key_code(provider.provider_id).into(),
            message: missing_api_key_message(provider.provider_id, "bind"),
            retryable: false,
        }));
    }

    Ok(OpenAiCompatibleBindOutcome::Ready(synthetic_binding(
        provider, settings, &endpoint, api_key,
    )))
}

pub(crate) fn reconcile_openai_compatible_runtime_session(
    provider: ResolvedRuntimeProvider,
    account_id: Option<&str>,
    session_id: Option<&str>,
    settings: &RuntimeSettingsSnapshot,
    config: &OpenAiCompatibleAuthConfig,
) -> Result<OpenAiCompatibleReconcileOutcome, AuthFlowError> {
    let endpoint = resolve_openai_compatible_endpoint_for_settings(provider, settings, config)?;
    let api_key = normalize_optional(settings.provider_api_key.as_deref());
    if api_key_required_for_endpoint(&endpoint) && api_key.is_none() {
        return Ok(OpenAiCompatibleReconcileOutcome::SignedOut(
            AuthDiagnostic {
                code: missing_api_key_code(provider.provider_id).into(),
                message: missing_api_key_message(provider.provider_id, "reconcile"),
                retryable: false,
            },
        ));
    }

    let expected = synthetic_binding(provider, settings, &endpoint, api_key);
    let account_id = normalized(account_id);
    let session_id = normalized(session_id);
    if account_id != Some(expected.account_id.as_str())
        || session_id != Some(expected.session_id.as_str())
    {
        return Ok(OpenAiCompatibleReconcileOutcome::SignedOut(AuthDiagnostic {
            code: cloud_binding_stale_code(provider.provider_id).into(),
            message: format!(
                "Xero rejected the persisted {} runtime binding because the selected provider profile, model, endpoint, or API key changed. Rebind the runtime session from the active profile.",
                provider_display_label(provider.provider_id)
            ),
            retryable: false,
        }));
    }

    Ok(OpenAiCompatibleReconcileOutcome::Authenticated(expected))
}

pub(crate) fn fetch_openai_compatible_models(
    api_key: Option<&str>,
    endpoint: &ResolvedOpenAiCompatibleEndpoint,
    config: &OpenAiCompatibleAuthConfig,
) -> Result<Vec<OpenAiCompatibleDiscoveredModel>, AuthFlowError> {
    let api_key = normalize_optional(api_key);
    if api_key_required_for_endpoint(endpoint) && api_key.is_none() {
        return Err(missing_openai_compatible_api_key_error(
            endpoint.provider_id.as_str(),
            "discover",
        ));
    }

    let client = config.http_client()?;
    let mut request = client.get(endpoint.models_url()?);
    if let Some(api_key) = api_key {
        request = request.header("Authorization", format!("Bearer {api_key}"));
    }
    let response = request
        .send()
        .map_err(|error| map_probe_transport_error(endpoint.provider_id.as_str(), error))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(map_probe_status_error(
            endpoint,
            status.as_u16(),
            body.trim(),
        ));
    }

    let body = response.text().map_err(|error| {
        AuthFlowError::retryable(
            provider_unavailable_code(endpoint.provider_id.as_str()),
            RuntimeAuthPhase::Failed,
            format!(
                "Xero could not read the {} catalog response body: {error}",
                provider_display_label(endpoint.provider_id.as_str())
            ),
        )
    })?;

    if endpoint.provider_id == GITHUB_MODELS_PROVIDER_ID {
        let models: Vec<GitHubCatalogModelSummary> =
            serde_json::from_str(&body).map_err(|error| {
                AuthFlowError::terminal(
                    models_decode_failed_code(endpoint.provider_id.as_str()),
                    RuntimeAuthPhase::Failed,
                    format!(
                        "Xero could not decode the {} catalog response: {error}",
                        provider_display_label(endpoint.provider_id.as_str())
                    ),
                )
            })?;

        return normalize_github_models_catalog(endpoint.provider_id.as_str(), models);
    }

    let models: ModelsResponse = serde_json::from_str(&body).map_err(|error| {
        AuthFlowError::terminal(
            models_decode_failed_code(endpoint.provider_id.as_str()),
            RuntimeAuthPhase::Failed,
            format!(
                "Xero could not decode the {} models response: {error}",
                provider_display_label(endpoint.provider_id.as_str())
            ),
        )
    })?;

    normalize_models(endpoint, models)
}

fn resolve_openai_compatible_endpoint(
    provider_id: &str,
    runtime_kind: &str,
    preset_id: Option<&str>,
    base_url: Option<&str>,
    api_version: Option<&str>,
    config: &OpenAiCompatibleAuthConfig,
) -> Result<ResolvedOpenAiCompatibleEndpoint, AuthFlowError> {
    let provider_id = provider_id.trim();
    let runtime_kind = runtime_kind.trim();
    let preset_id = normalize_optional(preset_id);
    let base_url = normalize_optional(base_url);
    let api_version = normalize_optional(api_version);

    let (expected_runtime_kind, default_base_url, model_list_strategy, model_list_url_override) =
        match provider_id {
            GITHUB_MODELS_PROVIDER_ID => (
                OPENAI_COMPATIBLE_RUNTIME_KIND,
                Some(config.github_models_base_url.as_str()),
                OpenAiCompatibleModelListStrategy::Live,
                Some(config.github_models_catalog_url.as_str()),
            ),
            OPENAI_API_PROVIDER_ID => (
                OPENAI_COMPATIBLE_RUNTIME_KIND,
                Some(config.openai_base_url.as_str()),
                OpenAiCompatibleModelListStrategy::Live,
                None,
            ),
            OLLAMA_PROVIDER_ID => (
                OPENAI_COMPATIBLE_RUNTIME_KIND,
                Some(DEFAULT_OLLAMA_BASE_URL),
                OpenAiCompatibleModelListStrategy::Live,
                None,
            ),
            AZURE_OPENAI_PROVIDER_ID => (
                OPENAI_COMPATIBLE_RUNTIME_KIND,
                None,
                OpenAiCompatibleModelListStrategy::Manual,
                None,
            ),
            GEMINI_AI_STUDIO_PROVIDER_ID => (
                GEMINI_RUNTIME_KIND,
                Some(config.gemini_ai_studio_base_url.as_str()),
                OpenAiCompatibleModelListStrategy::Live,
                None,
            ),
            other => {
                return Err(AuthFlowError::terminal(
                "openai_compatible_provider_unsupported",
                RuntimeAuthPhase::Failed,
                format!(
                    "Xero cannot resolve OpenAI-compatible endpoint metadata for unsupported provider `{other}`."
                ),
            ));
            }
        };

    if runtime_kind != expected_runtime_kind {
        return Err(AuthFlowError::terminal(
            "runtime_provider_invalid",
            RuntimeAuthPhase::Failed,
            format!(
                "Xero rejected provider `{provider_id}` because runtime kind `{runtime_kind}` must be `{expected_runtime_kind}`."
            ),
        ));
    }

    let effective_base_url = match provider_id {
        GITHUB_MODELS_PROVIDER_ID | OPENAI_API_PROVIDER_ID | OLLAMA_PROVIDER_ID => base_url
            .or(default_base_url)
            .ok_or_else(|| {
                AuthFlowError::terminal(
                    "openai_compatible_base_url_missing",
                    RuntimeAuthPhase::Failed,
                    "Xero could not resolve the OpenAI-compatible base URL because neither a preset nor a custom base URL was available.",
                )
            })?
            .to_owned(),
        AZURE_OPENAI_PROVIDER_ID => base_url
            .ok_or_else(|| {
                AuthFlowError::terminal(
                    "openai_compatible_base_url_missing",
                    RuntimeAuthPhase::Failed,
                    "Xero could not resolve the Azure OpenAI base URL because the active provider profile omitted baseUrl.",
                )
            })?
            .to_owned(),
        GEMINI_AI_STUDIO_PROVIDER_ID => default_base_url
            .ok_or_else(|| {
                AuthFlowError::terminal(
                    "openai_compatible_base_url_missing",
                    RuntimeAuthPhase::Failed,
                    "Xero could not resolve the Gemini AI Studio compatibility base URL.",
                )
            })?
            .to_owned(),
        _ => unreachable!("validated above"),
    };

    Url::parse(&normalize_base_url(&effective_base_url)).map_err(|error| {
        AuthFlowError::terminal(
            "openai_compatible_base_url_invalid",
            RuntimeAuthPhase::Failed,
            format!(
                "Xero rejected the {} base URL `{effective_base_url}` because it was invalid: {error}",
                provider_display_label(provider_id)
            ),
        )
    })?;

    if provider_id == AZURE_OPENAI_PROVIDER_ID && api_version.is_none() {
        return Err(AuthFlowError::terminal(
            "openai_compatible_api_version_missing",
            RuntimeAuthPhase::Failed,
            "Xero could not resolve Azure OpenAI endpoint metadata because apiVersion is required.",
        ));
    }

    Ok(ResolvedOpenAiCompatibleEndpoint {
        provider_id: provider_id.to_owned(),
        runtime_kind: runtime_kind.to_owned(),
        preset_id: preset_id.map(str::to_owned),
        effective_base_url,
        api_version: api_version.map(str::to_owned),
        model_list_strategy,
        model_list_url_override: model_list_url_override.map(str::to_owned),
    })
}

fn normalize_models(
    endpoint: &ResolvedOpenAiCompatibleEndpoint,
    response: ModelsResponse,
) -> Result<Vec<OpenAiCompatibleDiscoveredModel>, AuthFlowError> {
    let mut model_ids = std::collections::BTreeSet::new();
    let mut normalized = Vec::with_capacity(response.data.len());

    for model in response.data {
        let id = model.id.trim();
        if id.is_empty() {
            return Err(AuthFlowError::terminal(
                models_decode_failed_code(endpoint.provider_id.as_str()),
                RuntimeAuthPhase::Failed,
                format!(
                    "Xero could not decode the {} models response because one model id was blank.",
                    provider_display_label(endpoint.provider_id.as_str())
                ),
            ));
        }

        if !model_ids.insert(id.to_owned()) {
            return Err(AuthFlowError::terminal(
                models_decode_failed_code(endpoint.provider_id.as_str()),
                RuntimeAuthPhase::Failed,
                format!(
                    "Xero could not decode the {} models response because model `{id}` appeared more than once.",
                    provider_display_label(endpoint.provider_id.as_str())
                ),
            ));
        }

        let display_name = model
            .display_name
            .as_deref()
            .or(model.name.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(id)
            .to_owned();
        let thinking = normalize_thinking_capability(endpoint, id, &model.capabilities)?;

        normalized.push(OpenAiCompatibleDiscoveredModel {
            id: id.to_owned(),
            display_name,
            thinking,
            context_window_tokens: model
                .context_window_tokens
                .or(model.max_input_tokens)
                .filter(|tokens| *tokens > 0),
            max_output_tokens: model.max_output_tokens.filter(|tokens| *tokens > 0),
        });
    }

    normalized.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then(left.id.cmp(&right.id))
    });

    Ok(normalized)
}

fn normalize_github_models_catalog(
    provider_id: &str,
    response: Vec<GitHubCatalogModelSummary>,
) -> Result<Vec<OpenAiCompatibleDiscoveredModel>, AuthFlowError> {
    let mut model_ids = std::collections::BTreeSet::new();
    let mut normalized = Vec::with_capacity(response.len());

    for model in response {
        let id = model.id.trim();
        if id.is_empty() {
            return Err(AuthFlowError::terminal(
                models_decode_failed_code(provider_id),
                RuntimeAuthPhase::Failed,
                format!(
                    "Xero could not decode the {} catalog response because one model id was blank.",
                    provider_display_label(provider_id)
                ),
            ));
        }

        if !model_ids.insert(id.to_owned()) {
            return Err(AuthFlowError::terminal(
                models_decode_failed_code(provider_id),
                RuntimeAuthPhase::Failed,
                format!(
                    "Xero could not decode the {} catalog response because model `{id}` appeared more than once.",
                    provider_display_label(provider_id)
                ),
            ));
        }

        let display_name = model
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(id)
            .to_owned();

        normalized.push(OpenAiCompatibleDiscoveredModel {
            id: id.to_owned(),
            display_name,
            thinking: unsupported_thinking_capability(),
            context_window_tokens: model.context_window_tokens.filter(|tokens| *tokens > 0),
            max_output_tokens: model.max_output_tokens.filter(|tokens| *tokens > 0),
        });
    }

    normalized.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then(left.id.cmp(&right.id))
    });

    Ok(normalized)
}

fn normalize_thinking_capability(
    endpoint: &ResolvedOpenAiCompatibleEndpoint,
    model_id: &str,
    capabilities: &OpenAiCompatibleCapabilities,
) -> Result<OpenAiCompatibleDiscoveredThinkingCapability, AuthFlowError> {
    let payload = capabilities
        .reasoning
        .as_ref()
        .or(capabilities.thinking.as_ref());

    if let Some(payload) = payload {
        return normalize_thinking_payload(endpoint.provider_id.as_str(), model_id, payload);
    }

    if endpoint.provider_id == GITHUB_MODELS_PROVIDER_ID {
        return Ok(unsupported_thinking_capability());
    }

    if endpoint.provider_id == GEMINI_AI_STUDIO_PROVIDER_ID {
        return Ok(default_gemini_thinking_capability(model_id));
    }

    Ok(unsupported_thinking_capability())
}

fn normalize_thinking_payload(
    provider_id: &str,
    model_id: &str,
    payload: &OpenAiCompatibleThinkingPayload,
) -> Result<OpenAiCompatibleDiscoveredThinkingCapability, AuthFlowError> {
    if !payload.supported {
        if !payload.effort_options.is_empty() || payload.default_effort.is_some() {
            return Err(AuthFlowError::terminal(
                models_decode_failed_code(provider_id),
                RuntimeAuthPhase::Failed,
                format!(
                    "Xero could not decode the {} models response because thinking capability for model `{model_id}` declared unsupported while still providing effort metadata.",
                    provider_display_label(provider_id)
                ),
            ));
        }

        return Ok(unsupported_thinking_capability());
    }

    let mut effort_levels = Vec::with_capacity(payload.effort_options.len());
    let mut seen = std::collections::BTreeSet::new();
    for effort in &payload.effort_options {
        let normalized = parse_thinking_effort(provider_id, model_id, effort)?;
        if !seen.insert(normalized) {
            return Err(AuthFlowError::terminal(
                models_decode_failed_code(provider_id),
                RuntimeAuthPhase::Failed,
                format!(
                    "Xero could not decode the {} models response because thinking capability for model `{model_id}` repeated effort `{}`.",
                    provider_display_label(provider_id),
                    effort.trim()
                ),
            ));
        }
        effort_levels.push(normalized);
    }

    if effort_levels.is_empty() {
        return Err(AuthFlowError::terminal(
            models_decode_failed_code(provider_id),
            RuntimeAuthPhase::Failed,
            format!(
                "Xero could not decode the {} models response because thinking capability for model `{model_id}` did not include any effort options.",
                provider_display_label(provider_id)
            ),
        ));
    }

    let default_effort = payload
        .default_effort
        .as_deref()
        .map(|value| parse_thinking_effort(provider_id, model_id, value))
        .transpose()?;
    if let Some(default_effort) = default_effort {
        if !effort_levels.contains(&default_effort) {
            return Err(AuthFlowError::terminal(
                models_decode_failed_code(provider_id),
                RuntimeAuthPhase::Failed,
                format!(
                    "Xero could not decode the {} models response because thinking default effort for model `{model_id}` was not present in effortOptions.",
                    provider_display_label(provider_id)
                ),
            ));
        }
    }

    Ok(OpenAiCompatibleDiscoveredThinkingCapability {
        supported: true,
        default_effort: default_effort.or_else(|| {
            effort_levels
                .iter()
                .copied()
                .find(|effort| *effort == OpenAiCompatibleDiscoveredThinkingEffort::Medium)
                .or_else(|| effort_levels.first().copied())
        }),
        effort_levels,
    })
}

fn default_gemini_thinking_capability(
    model_id: &str,
) -> OpenAiCompatibleDiscoveredThinkingCapability {
    let normalized = model_id.trim().to_ascii_lowercase();
    let supports_reasoning = normalized.starts_with("gemini-2.5")
        || normalized.starts_with("gemini-3")
        || normalized.contains("thinking")
        || normalized.contains("reasoning");

    if !supports_reasoning {
        return unsupported_thinking_capability();
    }

    OpenAiCompatibleDiscoveredThinkingCapability {
        supported: true,
        effort_levels: vec![
            OpenAiCompatibleDiscoveredThinkingEffort::Minimal,
            OpenAiCompatibleDiscoveredThinkingEffort::Low,
            OpenAiCompatibleDiscoveredThinkingEffort::Medium,
            OpenAiCompatibleDiscoveredThinkingEffort::High,
        ],
        default_effort: Some(OpenAiCompatibleDiscoveredThinkingEffort::Medium),
    }
}

fn unsupported_thinking_capability() -> OpenAiCompatibleDiscoveredThinkingCapability {
    OpenAiCompatibleDiscoveredThinkingCapability {
        supported: false,
        effort_levels: Vec::new(),
        default_effort: None,
    }
}

fn parse_thinking_effort(
    provider_id: &str,
    model_id: &str,
    value: &str,
) -> Result<OpenAiCompatibleDiscoveredThinkingEffort, AuthFlowError> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "minimal" => Ok(OpenAiCompatibleDiscoveredThinkingEffort::Minimal),
        "low" => Ok(OpenAiCompatibleDiscoveredThinkingEffort::Low),
        "medium" => Ok(OpenAiCompatibleDiscoveredThinkingEffort::Medium),
        "high" => Ok(OpenAiCompatibleDiscoveredThinkingEffort::High),
        "x_high" | "xhigh" => Ok(OpenAiCompatibleDiscoveredThinkingEffort::XHigh),
        _ => Err(AuthFlowError::terminal(
            models_decode_failed_code(provider_id),
            RuntimeAuthPhase::Failed,
            format!(
                "Xero could not decode the {} models response because model `{model_id}` declared unsupported thinking effort `{}`.",
                provider_display_label(provider_id),
                value.trim()
            ),
        )),
    }
}

fn synthetic_binding(
    provider: ResolvedRuntimeProvider,
    settings: &RuntimeSettingsSnapshot,
    endpoint: &ResolvedOpenAiCompatibleEndpoint,
    api_key: Option<&str>,
) -> OpenAiCompatibleRuntimeSessionBinding {
    let auth_fingerprint = normalize_optional(api_key)
        .map(|api_key| sha256_hex(format!("secret:{}:{api_key}", provider.provider_id)))
        .unwrap_or_else(|| {
            sha256_hex(format!(
                "local:{}:{}:{}",
                provider.provider_id,
                endpoint.effective_base_url,
                endpoint.api_version.as_deref().unwrap_or("none"),
            ))
        });
    let effective_timestamp = settings
        .provider_api_key_updated_at
        .as_deref()
        .unwrap_or(settings.settings.updated_at.as_str());
    let session_fingerprint = sha256_hex(format!(
        "{}:{}:{}:{}:{}:{}:{}:{}",
        auth_fingerprint,
        settings.settings.provider_id,
        settings.runtime_kind,
        settings.settings.model_id,
        settings.preset_id.as_deref().unwrap_or("none"),
        settings.base_url.as_deref().unwrap_or("none"),
        settings.api_version.as_deref().unwrap_or("none"),
        effective_timestamp,
    ));

    OpenAiCompatibleRuntimeSessionBinding {
        provider_id: provider.provider_id.into(),
        account_id: format!("{}-acct-{}", provider.provider_id, &auth_fingerprint[..16]),
        session_id: format!(
            "{}-session-{}",
            provider.provider_id,
            &session_fingerprint[..16]
        ),
        updated_at: crate::auth::now_timestamp(),
    }
}

fn api_key_required_for_endpoint(endpoint: &ResolvedOpenAiCompatibleEndpoint) -> bool {
    !endpoint_uses_local_optional_auth(endpoint)
}

fn endpoint_uses_local_optional_auth(endpoint: &ResolvedOpenAiCompatibleEndpoint) -> bool {
    endpoint.provider_id == OLLAMA_PROVIDER_ID || is_local_base_url(&endpoint.effective_base_url)
}

fn is_local_base_url(base_url: &str) -> bool {
    Url::parse(&normalize_base_url(base_url))
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
        .is_some_and(|host| matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"))
}

fn map_probe_transport_error(provider_id: &str, error: reqwest::Error) -> AuthFlowError {
    if error.is_timeout() {
        return AuthFlowError::retryable(
            provider_unavailable_code(provider_id),
            RuntimeAuthPhase::Failed,
            format!(
                "The {} models probe timed out. Try again once the provider is reachable.",
                provider_display_label(provider_id)
            ),
        );
    }

    AuthFlowError::retryable(
        provider_unavailable_code(provider_id),
        RuntimeAuthPhase::Failed,
        format!(
            "Xero could not reach the {} models endpoint. Check the active base URL and try again once the provider is reachable.",
            provider_display_label(provider_id)
        ),
    )
}

fn map_probe_status_error(
    endpoint: &ResolvedOpenAiCompatibleEndpoint,
    status: u16,
    body: &str,
) -> AuthFlowError {
    let context = if body.is_empty() {
        String::new()
    } else {
        format!(" Provider said: {body}")
    };
    let provider_id = endpoint.provider_id.as_str();

    match status {
        401 | 403 if api_key_required_for_endpoint(endpoint) => AuthFlowError::terminal(
            missing_api_key_code(provider_id),
            RuntimeAuthPhase::Failed,
            format!(
                "Xero rejected the {} models probe because the API key was unauthorized.{context}",
                provider_display_label(provider_id)
            ),
        ),
        401 | 403 => AuthFlowError::terminal(
            provider_unavailable_code(provider_id),
            RuntimeAuthPhase::Failed,
            format!(
                "Xero rejected the {} models probe because the local endpoint returned HTTP {status}.{context}",
                provider_display_label(provider_id)
            ),
        ),
        429 => AuthFlowError::retryable(
            provider_unavailable_code(provider_id),
            RuntimeAuthPhase::Failed,
            format!(
                "The {} models probe was rate limited. Try again after the provider cooldown.{context}",
                provider_display_label(provider_id)
            ),
        ),
        500..=599 => AuthFlowError::retryable(
            provider_unavailable_code(provider_id),
            RuntimeAuthPhase::Failed,
            format!(
                "The {} models probe failed because the provider returned {status}.{context}",
                provider_display_label(provider_id)
            ),
        ),
        _ => AuthFlowError::terminal(
            models_decode_failed_code(provider_id),
            RuntimeAuthPhase::Failed,
            format!(
                "Xero rejected the {} models probe because the provider returned unexpected status {status}.{context}",
                provider_display_label(provider_id)
            ),
        ),
    }
}

fn provider_display_label(provider_id: &str) -> &'static str {
    match provider_id {
        GITHUB_MODELS_PROVIDER_ID => "GitHub Models",
        OPENAI_API_PROVIDER_ID => "OpenAI-compatible",
        OLLAMA_PROVIDER_ID => "Ollama",
        AZURE_OPENAI_PROVIDER_ID => "Azure OpenAI",
        GEMINI_AI_STUDIO_PROVIDER_ID => "Gemini AI Studio",
        _ => "OpenAI-compatible",
    }
}

fn missing_api_key_code(provider_id: &str) -> &'static str {
    match provider_id {
        GITHUB_MODELS_PROVIDER_ID => "github_models_token_missing",
        OPENAI_API_PROVIDER_ID => "openai_api_key_missing",
        OLLAMA_PROVIDER_ID => "ollama_api_key_missing",
        AZURE_OPENAI_PROVIDER_ID => "azure_openai_api_key_missing",
        GEMINI_AI_STUDIO_PROVIDER_ID => "gemini_ai_studio_api_key_missing",
        _ => "provider_api_key_missing",
    }
}

fn missing_api_key_message(provider_id: &str, operation: &str) -> String {
    match provider_id {
        GITHUB_MODELS_PROVIDER_ID => format!(
            "Xero cannot {operation} the selected GitHub Models runtime because no app-local GitHub token is configured for the active provider profile."
        ),
        _ => format!(
            "Xero cannot {operation} the selected {} runtime because no app-local API key is configured for the active provider profile.",
            provider_display_label(provider_id)
        ),
    }
}

fn cloud_binding_stale_code(provider_id: &str) -> &'static str {
    match provider_id {
        GITHUB_MODELS_PROVIDER_ID => "github_models_binding_stale",
        OPENAI_API_PROVIDER_ID => "openai_binding_stale",
        OLLAMA_PROVIDER_ID => "ollama_binding_stale",
        AZURE_OPENAI_PROVIDER_ID => "azure_openai_binding_stale",
        GEMINI_AI_STUDIO_PROVIDER_ID => "gemini_ai_studio_binding_stale",
        _ => "runtime_provider_binding_stale",
    }
}

fn provider_unavailable_code(provider_id: &str) -> &'static str {
    match provider_id {
        GITHUB_MODELS_PROVIDER_ID => "github_models_provider_unavailable",
        OPENAI_API_PROVIDER_ID => "openai_provider_unavailable",
        OLLAMA_PROVIDER_ID => "ollama_provider_unavailable",
        AZURE_OPENAI_PROVIDER_ID => "azure_openai_provider_unavailable",
        GEMINI_AI_STUDIO_PROVIDER_ID => "gemini_ai_studio_provider_unavailable",
        _ => "openai_compatible_provider_unavailable",
    }
}

fn models_decode_failed_code(provider_id: &str) -> &'static str {
    match provider_id {
        GITHUB_MODELS_PROVIDER_ID => "github_models_models_decode_failed",
        OPENAI_API_PROVIDER_ID => "openai_models_decode_failed",
        OLLAMA_PROVIDER_ID => "ollama_models_decode_failed",
        AZURE_OPENAI_PROVIDER_ID => "azure_openai_models_decode_failed",
        GEMINI_AI_STUDIO_PROVIDER_ID => "gemini_ai_studio_models_decode_failed",
        _ => "openai_compatible_models_decode_failed",
    }
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_owned()
}

fn normalize_optional(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn sha256_hex(value: String) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn normalized(value: Option<&str>) -> Option<&str> {
    normalize_optional(value)
}
