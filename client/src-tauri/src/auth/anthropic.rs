use std::{collections::BTreeSet, path::PathBuf, time::Duration};

use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime};

use super::{AuthDiagnostic, AuthFlowError};
use crate::{
    commands::{get_runtime_settings::RuntimeSettingsSnapshot, RuntimeAuthPhase},
    runtime::{
        resolve_runtime_provider_identity, ANTHROPIC_PROVIDER_ID, BEDROCK_PROVIDER_ID,
        VERTEX_PROVIDER_ID,
    },
    state::DesktopState,
};

const DEFAULT_MODELS_URL: &str = "https://api.anthropic.com/v1/models";
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";
const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
const BEDROCK_ENABLE_ENV: &str = "CLAUDE_CODE_USE_BEDROCK";
const BEDROCK_REGION_ENV: &str = "AWS_REGION";
const BEDROCK_DEFAULT_REGION_ENV: &str = "AWS_DEFAULT_REGION";
const VERTEX_ENABLE_ENV: &str = "CLAUDE_CODE_USE_VERTEX";
const VERTEX_REGION_ENV: &str = "CLOUD_ML_REGION";
const VERTEX_PROJECT_ENV: &str = "ANTHROPIC_VERTEX_PROJECT_ID";

#[derive(Debug, Clone)]
pub struct AnthropicAuthConfig {
    pub models_url: String,
    pub anthropic_version: String,
    pub timeout: Duration,
}

impl Default for AnthropicAuthConfig {
    fn default() -> Self {
        Self {
            models_url: DEFAULT_MODELS_URL.into(),
            anthropic_version: DEFAULT_ANTHROPIC_VERSION.into(),
            timeout: Duration::from_secs(10),
        }
    }
}

impl AnthropicAuthConfig {
    pub fn for_platform() -> Self {
        Self::default()
    }

    fn http_client(&self) -> Result<Client, AuthFlowError> {
        Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|error| {
                AuthFlowError::terminal(
                    "anthropic_http_client_unavailable",
                    RuntimeAuthPhase::Failed,
                    format!(
                        "Cadence could not build the Anthropic HTTP client for the models probe: {error}"
                    ),
                )
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicRuntimeSessionBinding {
    pub provider_id: String,
    pub session_id: String,
    pub account_id: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnthropicBindOutcome {
    Ready(AnthropicRuntimeSessionBinding),
    SignedOut(AuthDiagnostic),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnthropicReconcileOutcome {
    Authenticated(AnthropicRuntimeSessionBinding),
    SignedOut(AuthDiagnostic),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AnthropicDiscoveredThinkingEffort {
    Low,
    Medium,
    High,
    XHigh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicDiscoveredModel {
    pub id: String,
    pub display_name: String,
    pub thinking_supported: bool,
    pub effort_levels: Vec<AnthropicDiscoveredThinkingEffort>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnthropicFamilyProfileInput {
    pub provider_id: String,
    pub model_id: String,
    pub updated_at: String,
    pub region: Option<String>,
    pub project_id: Option<String>,
    pub api_key: Option<String>,
    pub api_key_updated_at: Option<String>,
}

impl From<&RuntimeSettingsSnapshot> for AnthropicFamilyProfileInput {
    fn from(settings: &RuntimeSettingsSnapshot) -> Self {
        Self {
            provider_id: settings.settings.provider_id.clone(),
            model_id: settings.settings.model_id.clone(),
            updated_at: settings.settings.updated_at.clone(),
            region: settings.region.clone(),
            project_id: settings.project_id.clone(),
            api_key: settings.provider_api_key.clone(),
            api_key_updated_at: settings.provider_api_key_updated_at.clone(),
        }
    }
}

impl AnthropicFamilyProfileInput {
    fn provider(&self) -> Result<crate::runtime::ResolvedRuntimeProvider, AuthFlowError> {
        resolve_runtime_provider_identity(
            Some(self.provider_id.as_str()),
            Some(ANTHROPIC_PROVIDER_ID),
        )
        .map_err(|diagnostic| {
            AuthFlowError::terminal(
                "runtime_provider_mismatch",
                RuntimeAuthPhase::Failed,
                diagnostic.message,
            )
        })
    }

    fn effective_timestamp(&self) -> &str {
        self.api_key_updated_at
            .as_deref()
            .unwrap_or(self.updated_at.as_str())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelsResponse {
    data: Vec<ModelSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelSummary {
    id: String,
    display_name: Option<String>,
    #[serde(default)]
    capabilities: ModelCapabilities,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelCapabilities {
    #[serde(default)]
    effort: AnthropicEffortCapability,
    #[serde(default)]
    thinking: AnthropicThinkingCapability,
}

#[derive(Debug, Default, Deserialize)]
struct AnthropicCapabilitySupport {
    #[serde(default)]
    supported: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnthropicEffortCapability {
    #[serde(default)]
    supported: bool,
    #[serde(default)]
    low: AnthropicCapabilitySupport,
    #[serde(default)]
    medium: AnthropicCapabilitySupport,
    #[serde(default)]
    high: AnthropicCapabilitySupport,
    #[serde(default)]
    xhigh: AnthropicCapabilitySupport,
    #[serde(default, rename = "max")]
    _max: AnthropicCapabilitySupport,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnthropicThinkingCapability {
    #[serde(default)]
    supported: bool,
    #[serde(default)]
    types: AnthropicThinkingTypes,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnthropicThinkingTypes {
    #[serde(default)]
    adaptive: AnthropicCapabilitySupport,
    #[serde(default)]
    enabled: AnthropicCapabilitySupport,
}

pub(crate) fn bind_anthropic_runtime_session<R: Runtime>(
    _app: &AppHandle<R>,
    state: &DesktopState,
    settings: &RuntimeSettingsSnapshot,
) -> Result<AnthropicBindOutcome, AuthFlowError> {
    let profile = AnthropicFamilyProfileInput::from(settings);
    if profile.provider_id == ANTHROPIC_PROVIDER_ID && profile.api_key.is_none() {
        return Ok(AnthropicBindOutcome::SignedOut(AuthDiagnostic {
            code: "anthropic_api_key_missing".into(),
            message: "Cadence cannot bind the selected Anthropic runtime because no app-local API key is configured for the active provider profile.".into(),
            retryable: false,
        }));
    }

    validate_anthropic_family_models_probe(&profile, &state.anthropic_auth_config())?;
    Ok(AnthropicBindOutcome::Ready(synthetic_binding(&profile)?))
}

pub(crate) fn reconcile_anthropic_runtime_session<R: Runtime>(
    _app: &AppHandle<R>,
    state: &DesktopState,
    account_id: Option<&str>,
    session_id: Option<&str>,
    settings: &RuntimeSettingsSnapshot,
) -> Result<AnthropicReconcileOutcome, AuthFlowError> {
    let profile = AnthropicFamilyProfileInput::from(settings);
    if profile.provider_id == ANTHROPIC_PROVIDER_ID && profile.api_key.is_none() {
        return Ok(AnthropicReconcileOutcome::SignedOut(AuthDiagnostic {
            code: "anthropic_api_key_missing".into(),
            message: "Cadence cannot reconcile the selected Anthropic runtime because no app-local API key is configured for the active provider profile.".into(),
            retryable: false,
        }));
    }

    let expected = synthetic_binding(&profile)?;
    let account_id = normalized(account_id);
    let session_id = normalized(session_id);
    if account_id != Some(expected.account_id.as_str())
        || session_id != Some(expected.session_id.as_str())
    {
        return Ok(AnthropicReconcileOutcome::SignedOut(
            binding_stale_diagnostic(profile.provider_id.as_str()),
        ));
    }

    validate_anthropic_family_models_probe(&profile, &state.anthropic_auth_config())?;

    Ok(AnthropicReconcileOutcome::Authenticated(expected))
}

pub(crate) fn discovered_anthropic_family_models(
    profile: &AnthropicFamilyProfileInput,
    config: &AnthropicAuthConfig,
) -> Result<Vec<AnthropicDiscoveredModel>, AuthFlowError> {
    match profile.provider_id.as_str() {
        ANTHROPIC_PROVIDER_ID => {
            let Some(api_key) = profile.api_key.as_deref() else {
                return Err(AuthFlowError::terminal(
                    "anthropic_api_key_missing",
                    RuntimeAuthPhase::Failed,
                    "Cadence cannot probe Anthropic models because no app-local API key is configured for the selected provider profile.",
                ));
            };
            fetch_anthropic_models(api_key, config)
        }
        BEDROCK_PROVIDER_ID => {
            let region = required_region(profile, BEDROCK_PROVIDER_ID)?;
            validate_bedrock_ambient_auth()?;
            Ok(manual_ambient_family_models(profile, Some(region), None))
        }
        VERTEX_PROVIDER_ID => {
            let region = required_region(profile, VERTEX_PROVIDER_ID)?;
            let project_id = required_project_id(profile)?;
            validate_vertex_ambient_auth()?;
            Ok(manual_ambient_family_models(
                profile,
                Some(region),
                Some(project_id),
            ))
        }
        other => Err(AuthFlowError::terminal(
            "runtime_provider_mismatch",
            RuntimeAuthPhase::Failed,
            format!("Cadence cannot use the Anthropic family bridge for provider `{other}`."),
        )),
    }
}

pub(crate) fn resolve_anthropic_family_launch_env(
    profile: &AnthropicFamilyProfileInput,
) -> Result<Vec<(&'static str, String)>, AuthFlowError> {
    match profile.provider_id.as_str() {
        ANTHROPIC_PROVIDER_ID => {
            let Some(api_key) = profile.api_key.as_deref() else {
                return Err(AuthFlowError::terminal(
                    "anthropic_api_key_missing",
                    RuntimeAuthPhase::Failed,
                    "Cadence cannot launch the detached Anthropic runtime because no app-local API key is configured for the active provider profile.",
                ));
            };
            Ok(vec![(ANTHROPIC_API_KEY_ENV, api_key.to_owned())])
        }
        BEDROCK_PROVIDER_ID => {
            let region = required_region(profile, BEDROCK_PROVIDER_ID)?;
            validate_bedrock_ambient_auth()?;
            Ok(vec![
                (BEDROCK_ENABLE_ENV, "1".into()),
                (BEDROCK_REGION_ENV, region.to_owned()),
                (BEDROCK_DEFAULT_REGION_ENV, region.to_owned()),
            ])
        }
        VERTEX_PROVIDER_ID => {
            let region = required_region(profile, VERTEX_PROVIDER_ID)?;
            let project_id = required_project_id(profile)?;
            validate_vertex_ambient_auth()?;
            Ok(vec![
                (VERTEX_ENABLE_ENV, "1".into()),
                (VERTEX_REGION_ENV, region.to_owned()),
                (VERTEX_PROJECT_ENV, project_id.to_owned()),
            ])
        }
        other => Err(AuthFlowError::terminal(
            "runtime_provider_mismatch",
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence cannot build detached launch metadata for provider `{other}` on the Anthropic family bridge."
            ),
        )),
    }
}

pub(crate) fn fetch_anthropic_models(
    api_key: &str,
    config: &AnthropicAuthConfig,
) -> Result<Vec<AnthropicDiscoveredModel>, AuthFlowError> {
    let client = config.http_client()?;
    let response = client
        .get(&config.models_url)
        .header("x-api-key", api_key)
        .header("anthropic-version", &config.anthropic_version)
        .send()
        .map_err(map_probe_transport_error)?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(map_probe_status_error(status.as_u16(), body.trim()));
    }

    let models: ModelsResponse = response.json().map_err(|error| {
        AuthFlowError::terminal(
            "anthropic_models_decode_failed",
            RuntimeAuthPhase::Failed,
            format!("Cadence could not decode the Anthropic models response: {error}"),
        )
    })?;

    normalize_anthropic_models(models)
}

fn validate_anthropic_family_models_probe(
    profile: &AnthropicFamilyProfileInput,
    config: &AnthropicAuthConfig,
) -> Result<(), AuthFlowError> {
    let models = discovered_anthropic_family_models(profile, config)?;

    if !models
        .iter()
        .any(|model| model.id.trim() == profile.model_id.trim())
    {
        return Err(AuthFlowError::terminal(
            model_unavailable_code(profile.provider_id.as_str()),
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence could not find the configured {} model `{}` in the provider models response.",
                provider_display_label(profile.provider_id.as_str()),
                profile.model_id,
            ),
        ));
    }

    Ok(())
}

fn normalize_anthropic_models(
    response: ModelsResponse,
) -> Result<Vec<AnthropicDiscoveredModel>, AuthFlowError> {
    let mut seen_ids = BTreeSet::new();
    let mut normalized = Vec::with_capacity(response.data.len());

    for model in response.data {
        let id = model.id.trim();
        if id.is_empty() {
            return Err(AuthFlowError::terminal(
                "anthropic_models_decode_failed",
                RuntimeAuthPhase::Failed,
                "Cadence could not decode the Anthropic models response because one model id was blank.",
            ));
        }

        if !seen_ids.insert(id.to_owned()) {
            return Err(AuthFlowError::terminal(
                "anthropic_models_decode_failed",
                RuntimeAuthPhase::Failed,
                format!(
                    "Cadence rejected the Anthropic models response because model `{id}` appeared more than once."
                ),
            ));
        }

        let display_name = model
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(id)
            .to_owned();
        let (thinking_supported, effort_levels) =
            normalize_anthropic_model_thinking(id, &model.capabilities)?;

        normalized.push(AnthropicDiscoveredModel {
            id: id.to_owned(),
            display_name,
            thinking_supported,
            effort_levels,
        });
    }

    Ok(normalized)
}

fn normalize_anthropic_model_thinking(
    model_id: &str,
    capabilities: &ModelCapabilities,
) -> Result<(bool, Vec<AnthropicDiscoveredThinkingEffort>), AuthFlowError> {
    let effort_levels = [
        (
            capabilities.effort.low.supported,
            AnthropicDiscoveredThinkingEffort::Low,
        ),
        (
            capabilities.effort.medium.supported,
            AnthropicDiscoveredThinkingEffort::Medium,
        ),
        (
            capabilities.effort.high.supported,
            AnthropicDiscoveredThinkingEffort::High,
        ),
        (
            capabilities.effort.xhigh.supported,
            AnthropicDiscoveredThinkingEffort::XHigh,
        ),
    ]
    .into_iter()
    .filter_map(|(supported, effort)| supported.then_some(effort))
    .collect::<Vec<_>>();

    if capabilities.effort.supported {
        if effort_levels.is_empty() {
            return Err(AuthFlowError::terminal(
                "anthropic_models_decode_failed",
                RuntimeAuthPhase::Failed,
                format!(
                    "Cadence rejected the Anthropic models response because model `{model_id}` declared only unsupported effort levels."
                ),
            ));
        }
        return Ok((true, effort_levels));
    }

    if capabilities.thinking.supported {
        if capabilities.thinking.types.enabled.supported
            || capabilities.thinking.types.adaptive.supported
        {
            return Ok((true, Vec::new()));
        }

        return Err(AuthFlowError::terminal(
            "anthropic_models_decode_failed",
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence rejected the Anthropic models response because model `{model_id}` declared thinking support without any supported thinking type."
            ),
        ));
    }

    Ok((false, Vec::new()))
}

fn map_probe_transport_error(error: reqwest::Error) -> AuthFlowError {
    if error.is_timeout() {
        return AuthFlowError::retryable(
            "anthropic_provider_unavailable",
            RuntimeAuthPhase::Failed,
            "The Anthropic models probe timed out. Try again once the provider is reachable.",
        );
    }

    AuthFlowError::retryable(
        "anthropic_provider_unavailable",
        RuntimeAuthPhase::Failed,
        format!("Cadence could not reach the Anthropic models endpoint: {error}"),
    )
}

fn map_probe_status_error(status: u16, body: &str) -> AuthFlowError {
    let suffix = if body.is_empty() {
        String::new()
    } else {
        format!(" Response: {body}")
    };

    match status {
        401 | 403 => AuthFlowError::terminal(
            "anthropic_invalid_api_key",
            RuntimeAuthPhase::Failed,
            format!("Anthropic rejected the configured API key with HTTP {status}.{suffix}"),
        ),
        429 => AuthFlowError::retryable(
            "anthropic_rate_limited",
            RuntimeAuthPhase::Failed,
            format!("Anthropic rate limited the models probe with HTTP 429.{suffix}"),
        ),
        500..=599 => AuthFlowError::retryable(
            "anthropic_provider_unavailable",
            RuntimeAuthPhase::Failed,
            format!(
                "Anthropic returned HTTP {status} while validating the configured API key.{suffix}"
            ),
        ),
        _ => AuthFlowError::terminal(
            "anthropic_provider_unavailable",
            RuntimeAuthPhase::Failed,
            format!(
                "Anthropic returned HTTP {status} while validating the configured API key.{suffix}"
            ),
        ),
    }
}

fn manual_ambient_family_models(
    profile: &AnthropicFamilyProfileInput,
    _region: Option<&str>,
    _project_id: Option<&str>,
) -> Vec<AnthropicDiscoveredModel> {
    let thinking_supported = looks_like_claude_model(profile.model_id.as_str());
    let effort_levels = if thinking_supported {
        vec![
            AnthropicDiscoveredThinkingEffort::Low,
            AnthropicDiscoveredThinkingEffort::Medium,
            AnthropicDiscoveredThinkingEffort::High,
        ]
    } else {
        Vec::new()
    };

    vec![AnthropicDiscoveredModel {
        id: profile.model_id.clone(),
        display_name: profile.model_id.clone(),
        thinking_supported,
        effort_levels,
    }]
}

fn looks_like_claude_model(model_id: &str) -> bool {
    let normalized = model_id.trim().to_ascii_lowercase();
    normalized.contains("claude")
}

fn synthetic_binding(
    profile: &AnthropicFamilyProfileInput,
) -> Result<AnthropicRuntimeSessionBinding, AuthFlowError> {
    let provider = profile.provider()?;
    let auth_fingerprint = match provider.provider_id {
        ANTHROPIC_PROVIDER_ID => {
            let Some(api_key) = profile.api_key.as_deref() else {
                return Err(AuthFlowError::terminal(
                    "anthropic_api_key_missing",
                    RuntimeAuthPhase::Failed,
                    "Cadence cannot derive an Anthropic runtime binding because no app-local API key is configured for the active provider profile.",
                ));
            };
            sha256_hex(format!("{}:{api_key}", provider.provider_id))
        }
        BEDROCK_PROVIDER_ID => sha256_hex(format!(
            "{}:{}",
            provider.provider_id,
            required_region(profile, BEDROCK_PROVIDER_ID)?
        )),
        VERTEX_PROVIDER_ID => sha256_hex(format!(
            "{}:{}:{}",
            provider.provider_id,
            required_region(profile, VERTEX_PROVIDER_ID)?,
            required_project_id(profile)?
        )),
        other => {
            return Err(AuthFlowError::terminal(
                "runtime_provider_mismatch",
                RuntimeAuthPhase::Failed,
                format!(
                "Cadence cannot derive an Anthropic family runtime binding for provider `{other}`."
            ),
            ))
        }
    };

    let session_fingerprint = sha256_hex(format!(
        "{}:{}:{}:{}:{}:{}",
        auth_fingerprint,
        provider.provider_id,
        profile.model_id,
        profile.region.as_deref().unwrap_or_default(),
        profile.project_id.as_deref().unwrap_or_default(),
        profile.effective_timestamp(),
    ));

    Ok(AnthropicRuntimeSessionBinding {
        provider_id: provider.provider_id.into(),
        account_id: format!("{}-acct-{}", provider.provider_id, &auth_fingerprint[..16]),
        session_id: format!(
            "{}-session-{}",
            provider.provider_id,
            &session_fingerprint[..16]
        ),
        updated_at: crate::auth::now_timestamp(),
    })
}

fn required_region<'a>(
    profile: &'a AnthropicFamilyProfileInput,
    provider_id: &str,
) -> Result<&'a str, AuthFlowError> {
    normalized(profile.region.as_deref()).ok_or_else(|| {
        AuthFlowError::terminal(
            region_missing_code(provider_id),
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence cannot use {} provider profile metadata because field `region` is required.",
                provider_display_label(provider_id),
            ),
        )
    })
}

fn required_project_id(profile: &AnthropicFamilyProfileInput) -> Result<&str, AuthFlowError> {
    normalized(profile.project_id.as_deref()).ok_or_else(|| {
        AuthFlowError::terminal(
            "vertex_project_id_missing",
            RuntimeAuthPhase::Failed,
            "Cadence cannot use Google Vertex AI provider profile metadata because field `projectId` is required.",
        )
    })
}

fn validate_bedrock_ambient_auth() -> Result<(), AuthFlowError> {
    if bedrock_ambient_credentials_available() {
        return Ok(());
    }

    Err(AuthFlowError::terminal(
        "bedrock_aws_credentials_missing",
        RuntimeAuthPhase::Failed,
        "Cadence could not find ambient AWS credentials for Amazon Bedrock. Configure the default AWS SDK credential chain (for example AWS_PROFILE, AWS access keys, SSO, or web identity) before using this provider profile.",
    ))
}

fn validate_vertex_ambient_auth() -> Result<(), AuthFlowError> {
    if vertex_adc_available() {
        return Ok(());
    }

    Err(AuthFlowError::terminal(
        "vertex_adc_missing",
        RuntimeAuthPhase::Failed,
        "Cadence could not find Application Default Credentials for Google Vertex AI. Configure ADC with gcloud, a service-account key file, or GOOGLE_APPLICATION_CREDENTIALS before using this provider profile.",
    ))
}

fn bedrock_ambient_credentials_available() -> bool {
    (env_var_present("AWS_ACCESS_KEY_ID") && env_var_present("AWS_SECRET_ACCESS_KEY"))
        || env_var_present("AWS_PROFILE")
        || env_var_present("AWS_DEFAULT_PROFILE")
        || (env_var_present("AWS_WEB_IDENTITY_TOKEN_FILE") && env_var_present("AWS_ROLE_ARN"))
        || env_var_present("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI")
        || env_var_present("AWS_CONTAINER_CREDENTIALS_FULL_URI")
        || existing_env_path("AWS_SHARED_CREDENTIALS_FILE").is_some()
        || existing_env_path("AWS_CONFIG_FILE").is_some()
        || default_aws_config_paths()
            .into_iter()
            .any(|path| path.is_file())
}

fn vertex_adc_available() -> bool {
    existing_env_path("GOOGLE_APPLICATION_CREDENTIALS").is_some()
        || default_vertex_adc_path().is_some_and(|path| path.is_file())
}

fn env_var_present(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .is_some_and(|value| !value.is_empty())
}

fn existing_env_path(key: &str) -> Option<PathBuf> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

fn default_aws_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".aws").join("credentials"));
        paths.push(home.join(".aws").join("config"));
    }
    paths
}

fn default_vertex_adc_path() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .ok()
            .map(PathBuf::from)
            .map(|base| {
                base.join("gcloud")
                    .join("application_default_credentials.json")
            })
    } else {
        dirs::home_dir().map(|home| {
            home.join(".config")
                .join("gcloud")
                .join("application_default_credentials.json")
        })
    }
}

fn provider_display_label(provider_id: &str) -> &'static str {
    match provider_id {
        ANTHROPIC_PROVIDER_ID => "Anthropic",
        BEDROCK_PROVIDER_ID => "Amazon Bedrock",
        VERTEX_PROVIDER_ID => "Google Vertex AI",
        _ => "Anthropic family",
    }
}

fn binding_stale_diagnostic(provider_id: &str) -> AuthDiagnostic {
    AuthDiagnostic {
        code: binding_stale_code(provider_id).into(),
        message: format!(
            "Cadence rejected the persisted {} runtime binding because the selected provider profile metadata changed. Rebind the runtime session from the active profile.",
            provider_display_label(provider_id),
        ),
        retryable: false,
    }
}

fn binding_stale_code(provider_id: &str) -> &'static str {
    match provider_id {
        ANTHROPIC_PROVIDER_ID => "anthropic_binding_stale",
        BEDROCK_PROVIDER_ID => "bedrock_binding_stale",
        VERTEX_PROVIDER_ID => "vertex_binding_stale",
        _ => "anthropic_family_binding_stale",
    }
}

fn region_missing_code(provider_id: &str) -> &'static str {
    match provider_id {
        BEDROCK_PROVIDER_ID => "bedrock_region_missing",
        VERTEX_PROVIDER_ID => "vertex_region_missing",
        _ => "provider_region_missing",
    }
}

fn model_unavailable_code(provider_id: &str) -> &'static str {
    match provider_id {
        ANTHROPIC_PROVIDER_ID => "anthropic_model_unavailable",
        BEDROCK_PROVIDER_ID => "bedrock_model_unavailable",
        VERTEX_PROVIDER_ID => "vertex_model_unavailable",
        _ => "anthropic_family_model_unavailable",
    }
}

fn sha256_hex(value: String) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}
