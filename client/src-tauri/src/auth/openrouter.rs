use std::time::Duration;

use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime};

use super::{AuthDiagnostic, AuthFlowError};
use crate::{
    commands::{get_runtime_settings::RuntimeSettingsSnapshot, RuntimeAuthPhase},
    runtime::{openrouter_provider, OPENROUTER_PROVIDER_ID},
    state::DesktopState,
};

const DEFAULT_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

#[derive(Debug, Clone)]
pub struct OpenRouterAuthConfig {
    pub models_url: String,
    pub timeout: Duration,
}

impl Default for OpenRouterAuthConfig {
    fn default() -> Self {
        Self {
            models_url: DEFAULT_MODELS_URL.into(),
            timeout: Duration::from_secs(10),
        }
    }
}

impl OpenRouterAuthConfig {
    pub fn for_platform() -> Self {
        Self::default()
    }

    fn http_client(&self) -> Result<Client, AuthFlowError> {
        Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|error| {
                AuthFlowError::terminal(
                    "openrouter_http_client_unavailable",
                    RuntimeAuthPhase::Failed,
                    format!(
                        "Cadence could not build the OpenRouter HTTP client for the models probe: {error}"
                    ),
                )
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRouterRuntimeSessionBinding {
    pub provider_id: String,
    pub session_id: String,
    pub account_id: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenRouterBindOutcome {
    Ready(OpenRouterRuntimeSessionBinding),
    SignedOut(AuthDiagnostic),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenRouterReconcileOutcome {
    Authenticated(OpenRouterRuntimeSessionBinding),
    SignedOut(AuthDiagnostic),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelsResponse {
    data: Vec<ModelSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelSummary {
    id: String,
}

pub(crate) fn bind_openrouter_runtime_session<R: Runtime>(
    _app: &AppHandle<R>,
    state: &DesktopState,
    settings: &RuntimeSettingsSnapshot,
) -> Result<OpenRouterBindOutcome, AuthFlowError> {
    let Some(api_key) = settings.openrouter_api_key.as_deref() else {
        return Ok(OpenRouterBindOutcome::SignedOut(AuthDiagnostic {
            code: "openrouter_api_key_missing".into(),
            message: "Cadence cannot bind the selected OpenRouter runtime because no app-global API key is configured in Settings.".into(),
            retryable: false,
        }));
    };

    validate_openrouter_models_probe(api_key, &settings.settings.model_id, &state.openrouter_auth_config())?;
    Ok(OpenRouterBindOutcome::Ready(synthetic_binding(settings, api_key)))
}

pub(crate) fn reconcile_openrouter_runtime_session<R: Runtime>(
    _app: &AppHandle<R>,
    state: &DesktopState,
    account_id: Option<&str>,
    session_id: Option<&str>,
    settings: &RuntimeSettingsSnapshot,
) -> Result<OpenRouterReconcileOutcome, AuthFlowError> {
    let Some(api_key) = settings.openrouter_api_key.as_deref() else {
        return Ok(OpenRouterReconcileOutcome::SignedOut(AuthDiagnostic {
            code: "openrouter_api_key_missing".into(),
            message: "Cadence cannot reconcile the selected OpenRouter runtime because no app-global API key is configured in Settings.".into(),
            retryable: false,
        }));
    };

    let expected = synthetic_binding(settings, api_key);
    let account_id = normalized(account_id);
    let session_id = normalized(session_id);
    if account_id != Some(expected.account_id.as_str())
        || session_id != Some(expected.session_id.as_str())
    {
        return Ok(OpenRouterReconcileOutcome::SignedOut(AuthDiagnostic {
            code: "openrouter_binding_stale".into(),
            message: "Cadence rejected the persisted OpenRouter runtime binding because the saved app-global provider settings or API key changed. Rebind the runtime session from Settings.".into(),
            retryable: false,
        }));
    }

    validate_openrouter_models_probe(
        api_key,
        &settings.settings.model_id,
        &state.openrouter_auth_config(),
    )?;

    Ok(OpenRouterReconcileOutcome::Authenticated(expected))
}

fn validate_openrouter_models_probe(
    api_key: &str,
    model_id: &str,
    config: &OpenRouterAuthConfig,
) -> Result<(), AuthFlowError> {
    let client = config.http_client()?;
    let response = client
        .get(&config.models_url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .map_err(map_probe_transport_error)?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(map_probe_status_error(status.as_u16(), body.trim()));
    }

    let models: ModelsResponse = response.json().map_err(|error| {
        AuthFlowError::terminal(
            "openrouter_models_decode_failed",
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence could not decode the OpenRouter models response: {error}"
            ),
        )
    })?;

    if !models
        .data
        .iter()
        .any(|model| model.id.trim() == model_id)
    {
        return Err(AuthFlowError::terminal(
            "openrouter_model_unavailable",
            RuntimeAuthPhase::Failed,
            format!(
                "Cadence could not find the configured OpenRouter model `{model_id}` in the provider models response."
            ),
        ));
    }

    Ok(())
}

fn map_probe_transport_error(error: reqwest::Error) -> AuthFlowError {
    if error.is_timeout() {
        return AuthFlowError::retryable(
            "openrouter_provider_unavailable",
            RuntimeAuthPhase::Failed,
            "The OpenRouter models probe timed out. Try again once the provider is reachable.",
        );
    }

    AuthFlowError::retryable(
        "openrouter_provider_unavailable",
        RuntimeAuthPhase::Failed,
        format!(
            "Cadence could not reach the OpenRouter models endpoint: {error}"
        ),
    )
}

fn map_probe_status_error(status: u16, body: &str) -> AuthFlowError {
    let suffix = if body.is_empty() {
        String::new()
    } else {
        format!(" Response: {body}")
    };

    match status {
        401 => AuthFlowError::terminal(
            "openrouter_invalid_api_key",
            RuntimeAuthPhase::Failed,
            format!(
                "OpenRouter rejected the configured API key with HTTP 401.{suffix}"
            ),
        ),
        402 => AuthFlowError::terminal(
            "openrouter_insufficient_credits",
            RuntimeAuthPhase::Failed,
            format!(
                "OpenRouter rejected the configured API key with HTTP 402 due to insufficient credits.{suffix}"
            ),
        ),
        429 => AuthFlowError::retryable(
            "openrouter_rate_limited",
            RuntimeAuthPhase::Failed,
            format!("OpenRouter rate limited the models probe with HTTP 429.{suffix}"),
        ),
        500..=599 => AuthFlowError::retryable(
            "openrouter_provider_unavailable",
            RuntimeAuthPhase::Failed,
            format!(
                "OpenRouter returned HTTP {status} while validating the configured API key.{suffix}"
            ),
        ),
        _ => AuthFlowError::terminal(
            "openrouter_provider_unavailable",
            RuntimeAuthPhase::Failed,
            format!(
                "OpenRouter returned HTTP {status} while validating the configured API key.{suffix}"
            ),
        ),
    }
}

fn synthetic_binding(
    settings: &RuntimeSettingsSnapshot,
    api_key: &str,
) -> OpenRouterRuntimeSessionBinding {
    let provider = openrouter_provider();
    let key_fingerprint = sha256_hex(format!("{OPENROUTER_PROVIDER_ID}:{api_key}"));
    let effective_timestamp = settings
        .openrouter_credentials_updated_at
        .as_deref()
        .unwrap_or(settings.settings.updated_at.as_str());
    let session_fingerprint = sha256_hex(format!(
        "{}:{}:{}:{}",
        key_fingerprint,
        settings.settings.provider_id,
        settings.settings.model_id,
        effective_timestamp,
    ));

    OpenRouterRuntimeSessionBinding {
        provider_id: provider.provider_id.into(),
        account_id: format!("openrouter-acct-{}", &key_fingerprint[..16]),
        session_id: format!("openrouter-session-{}", &session_fingerprint[..16]),
        updated_at: crate::auth::now_timestamp(),
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
