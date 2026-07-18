use std::{
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, OptionalExtension};
use tauri::{AppHandle, Runtime};
use xero_agent_core::{
    provider_capability_catalog, provider_preflight_snapshot,
    run_openai_compatible_provider_preflight_probe, run_xai_provider_preflight_probe,
    OpenAiCompatibleProviderPreflightProbeRequest, ProviderCapabilityCatalogInput,
    ProviderPreflightError, ProviderPreflightErrorClass, ProviderPreflightInput,
    ProviderPreflightRequiredFeatures, ProviderPreflightSnapshot, ProviderPreflightSource,
    XaiProviderPreflightProbeRequest, DEFAULT_PROVIDER_CATALOG_TTL_SECONDS,
};

use crate::{
    auth::{
        load_latest_openai_codex_session, load_latest_xai_session,
        load_openai_codex_session_for_profile_link, load_xai_session,
        load_xai_session_for_profile_link,
        openai_compatible::resolve_openai_compatible_endpoint_for_profile,
        refresh_provider_auth_session, StoredXaiSession,
    },
    commands::{
        get_runtime_settings::runtime_settings_snapshot_for_provider_profile,
        resolve_context_limit, CommandError, CommandResult, SessionContextLimitConfidenceDto,
        SessionContextLimitSourceDto,
    },
    provider_credentials::{
        ProviderCredentialLink, ProviderCredentialProfile, ProviderCredentialsView,
    },
    provider_models::{
        catalog_age_seconds, load_provider_model_catalog, provider_capability_catalog_for_catalog,
        ProviderModelCatalog, ProviderModelCatalogSource, ProviderModelRecord,
        ProviderModelThinkingEffort,
    },
    runtime::{
        RuntimeProvider, ANTHROPIC_PROVIDER_ID, AZURE_OPENAI_PROVIDER_ID, BEDROCK_PROVIDER_ID,
        CURSOR_PROVIDER_ID, DEEPSEEK_PROVIDER_ID, GEMINI_AI_STUDIO_PROVIDER_ID,
        GITHUB_MODELS_PROVIDER_ID, OLLAMA_PROVIDER_ID, OPENAI_API_PROVIDER_ID,
        OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID, VERTEX_PROVIDER_ID, XAI_PROVIDER_ID,
    },
    state::DesktopState,
};

const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
const XAI_BASE_URL: &str = "https://api.x.ai/v1";
const PROVIDER_PREFLIGHT_LIVE_PROBE_TIMEOUT_MS: u64 = 5_000;
const XAI_PREFLIGHT_REFRESH_SKEW_SECONDS: i64 = 60;

pub(crate) fn run_selected_provider_preflight<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile_id: &str,
    model_id: Option<&str>,
    force_refresh: bool,
    required_features: ProviderPreflightRequiredFeatures,
) -> CommandResult<ProviderPreflightSnapshot> {
    let started = Instant::now();
    refresh_xai_session_before_preflight_if_needed(app, state, profile_id)?;
    let catalog = load_provider_model_catalog(app, state, profile_id, force_refresh)?;
    let provider_profiles =
        crate::commands::provider_credentials::load_provider_credentials_view(app, state)?;
    let profile = provider_profiles
        .profile(profile_id)
        .or_else(|| {
            provider_profiles
                .profiles()
                .iter()
                .find(|profile| profile.provider_id == profile_id)
        })
        .ok_or_else(|| {
            CommandError::user_fixable(
                "provider_not_found",
                format!("Xero could not find provider `{profile_id}` for preflight."),
            )
        })?;
    let selected_model_id = model_id
        .map(str::trim)
        .filter(|model_id| !model_id.is_empty())
        .unwrap_or(catalog.configured_model_id.as_str());
    let credential_ready = provider_credentials_ready_for_preflight(app, state, profile)?;
    let catalog_snapshot = provider_preflight_from_catalog(
        &catalog,
        selected_model_id,
        required_features.clone(),
        credential_ready,
    );
    let live_snapshot = match live_openai_codex_preflight_for_profile(
        app,
        state,
        profile,
        selected_model_id,
        required_features.clone(),
        &catalog,
    )? {
        Some(snapshot) => Some(snapshot),
        None => match live_xai_preflight_for_profile(
            &provider_profiles,
            profile,
            selected_model_id,
            required_features.clone(),
            &catalog,
        )? {
            Some(snapshot) => Some(snapshot),
            None => live_openai_compatible_preflight_for_profile(
                state,
                &provider_profiles,
                profile,
                selected_model_id,
                required_features,
                &catalog,
            )?,
        },
    };
    let snapshot = bind_provider_preflight_cache_for_profile(
        state,
        &provider_profiles,
        profile,
        live_snapshot.unwrap_or(catalog_snapshot),
    )?;
    persist_provider_preflight_snapshot(&state.global_db_path(app)?, &snapshot)?;
    if std::env::var_os("XERO_RUNTIME_LATENCY_LOG").is_some() {
        eprintln!(
            "[runtime-latency] run_selected_provider_preflight profile_id={profile_id} provider_id={} model_id={} source={:?} duration_ms={}",
            snapshot.provider_id,
            snapshot.model_id,
            snapshot.source,
            started.elapsed().as_millis()
        );
    }
    Ok(snapshot)
}

pub(crate) fn provider_catalog_preflight_snapshot_for_run<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile_id: &str,
    provider_id: &str,
    model_id: &str,
    required_features: ProviderPreflightRequiredFeatures,
) -> CommandResult<ProviderPreflightSnapshot> {
    refresh_xai_session_before_preflight_if_needed(app, state, profile_id)?;
    let catalog = load_provider_model_catalog(app, state, profile_id, false)?;
    let provider_profiles =
        crate::commands::provider_credentials::load_provider_credentials_view(app, state)?;
    let profile = provider_profiles
        .profile(profile_id)
        .or_else(|| {
            provider_profiles
                .profiles()
                .iter()
                .find(|profile| profile.provider_id == profile_id)
        })
        .ok_or_else(|| {
            CommandError::user_fixable(
                "provider_not_found",
                format!("Xero could not find provider `{profile_id}` for cached preflight."),
            )
        })?;
    if profile.provider_id != provider_id {
        return Err(CommandError::user_fixable(
            "provider_preflight_profile_mismatch",
            format!(
                "Provider profile `{profile_id}` resolves to `{}`, but the run requested `{provider_id}`.",
                profile.provider_id
            ),
        ));
    }

    let selected_model_id = match model_id.trim() {
        "" => catalog.configured_model_id.as_str(),
        trimmed => trimmed,
    };
    let credential_ready = provider_credentials_ready_for_preflight(app, state, profile)?;
    let catalog_snapshot = provider_preflight_from_catalog(
        &catalog,
        selected_model_id,
        required_features.clone(),
        credential_ready,
    );
    let live_snapshot = match live_openai_codex_preflight_for_profile(
        app,
        state,
        profile,
        selected_model_id,
        required_features.clone(),
        &catalog,
    )? {
        Some(snapshot) => Some(snapshot),
        None => match live_xai_preflight_for_profile(
            &provider_profiles,
            profile,
            selected_model_id,
            required_features.clone(),
            &catalog,
        )? {
            Some(snapshot) => Some(snapshot),
            None => live_openai_compatible_preflight_for_profile(
                state,
                &provider_profiles,
                profile,
                selected_model_id,
                required_features,
                &catalog,
            )?,
        },
    };
    let snapshot = live_snapshot.unwrap_or(catalog_snapshot);
    let snapshot =
        bind_provider_preflight_cache_for_profile(state, &provider_profiles, profile, snapshot)?;
    persist_provider_preflight_snapshot(&state.global_db_path(app)?, &snapshot)?;
    Ok(snapshot)
}

pub(crate) fn current_provider_preflight_cache_binding<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile_id: &str,
    provider_id: &str,
    model_id: &str,
    required_features: &ProviderPreflightRequiredFeatures,
) -> CommandResult<Option<xero_agent_core::ProviderPreflightCacheBinding>> {
    let provider_profiles =
        crate::commands::provider_credentials::load_provider_credentials_view(app, state)?;
    let profile = provider_profiles
        .profile(profile_id)
        .or_else(|| {
            provider_profiles
                .profiles()
                .iter()
                .find(|profile| profile.provider_id == profile_id)
        })
        .ok_or_else(|| {
            CommandError::user_fixable(
                "provider_not_found",
                format!("Xero could not find provider `{profile_id}` for preflight cache reuse."),
            )
        })?;
    if profile.provider_id != provider_id {
        return Ok(None);
    }
    let (endpoint_fingerprint, account_class) =
        provider_preflight_cache_binding_parts(state, &provider_profiles, profile)?;
    Ok(Some(xero_agent_core::provider_preflight_cache_binding(
        provider_id,
        model_id,
        &endpoint_fingerprint,
        &account_class,
        required_features,
    )))
}

pub(crate) fn latest_provider_preflight_snapshot<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile_id: &str,
    provider_id: &str,
    model_id: &str,
) -> CommandResult<Option<ProviderPreflightSnapshot>> {
    load_provider_preflight_snapshot(
        &state.global_db_path(app)?,
        profile_id,
        provider_id,
        model_id,
    )
}

fn refresh_xai_session_before_preflight_if_needed<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile_id: &str,
) -> CommandResult<()> {
    let provider_profiles =
        crate::commands::provider_credentials::load_provider_credentials_view(app, state)?;
    let Some(profile) = provider_profiles.profile(profile_id).or_else(|| {
        provider_profiles
            .profiles()
            .iter()
            .find(|profile| profile.provider_id == profile_id)
    }) else {
        return Ok(());
    };
    if profile.provider_id != XAI_PROVIDER_ID {
        return Ok(());
    }

    let auth_store_path = state.global_db_path(app)?;
    let session = match profile.credential_link.as_ref() {
        Some(link @ ProviderCredentialLink::Xai { .. }) => {
            load_xai_session_for_profile_link(&auth_store_path, link)
                .map_err(crate::commands::runtime_support::command_error_from_auth)?
        }
        Some(ProviderCredentialLink::ApiKey { .. }) => return Ok(()),
        _ => load_latest_xai_session(&auth_store_path)
            .map_err(crate::commands::runtime_support::command_error_from_auth)?,
    };
    let Some(session) = session else {
        return Ok(());
    };
    if !xai_session_needs_preflight_refresh(&session, current_unix_timestamp()) {
        return Ok(());
    }

    let refreshed = refresh_provider_auth_session(
        app,
        state,
        RuntimeProvider::Xai,
        session.account_id.as_str(),
    )
    .map_err(crate::commands::runtime_support::command_error_from_auth)?;
    load_xai_session(&auth_store_path, refreshed.account_id.as_str())
        .map_err(crate::commands::runtime_support::command_error_from_auth)?
        .ok_or_else(|| {
            CommandError::retryable(
                "xai_auth_refresh_missing",
                "Xero refreshed xAI auth before provider preflight, but the refreshed session was not available in the app-local credential store.",
            )
        })?;

    Ok(())
}

fn xai_session_needs_preflight_refresh(session: &StoredXaiSession, now: i64) -> bool {
    session.expires_at <= now.saturating_add(XAI_PREFLIGHT_REFRESH_SKEW_SECONDS)
}

fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn bind_provider_preflight_cache_for_profile(
    state: &DesktopState,
    provider_profiles: &ProviderCredentialsView,
    profile: &ProviderCredentialProfile,
    snapshot: ProviderPreflightSnapshot,
) -> CommandResult<ProviderPreflightSnapshot> {
    let (endpoint_fingerprint, account_class) =
        provider_preflight_cache_binding_parts(state, provider_profiles, profile)?;
    Ok(xero_agent_core::bind_provider_preflight_cache(
        snapshot,
        &endpoint_fingerprint,
        &account_class,
    ))
}

fn provider_preflight_cache_binding_parts(
    state: &DesktopState,
    provider_profiles: &ProviderCredentialsView,
    profile: &ProviderCredentialProfile,
) -> CommandResult<(String, String)> {
    let endpoint_fingerprint = match profile.provider_id.as_str() {
        OPENAI_CODEX_PROVIDER_ID => "https://chatgpt.com/backend-api/codex/responses".into(),
        XAI_PROVIDER_ID => format!("{XAI_BASE_URL}/responses"),
        CURSOR_PROVIDER_ID => "cursor-sdk://local-harness".into(),
        OPENAI_API_PROVIDER_ID
        | DEEPSEEK_PROVIDER_ID
        | GITHUB_MODELS_PROVIDER_ID
        | AZURE_OPENAI_PROVIDER_ID
        | GEMINI_AI_STUDIO_PROVIDER_ID
        | OLLAMA_PROVIDER_ID => {
            let endpoint = resolve_openai_compatible_endpoint_for_profile(
                profile,
                &state.openai_compatible_auth_config(),
            )
            .map_err(|error| {
                CommandError::user_fixable(
                    error.code,
                    format!(
                        "Xero could not resolve provider preflight cache endpoint: {}",
                        error.message
                    ),
                )
            })?;
            match endpoint.api_version.as_deref() {
                Some(api_version) if !api_version.trim().is_empty() => {
                    format!("{}?api-version={api_version}", endpoint.effective_base_url)
                }
                _ => endpoint.effective_base_url,
            }
        }
        OPENROUTER_PROVIDER_ID => OPENROUTER_BASE_URL.into(),
        ANTHROPIC_PROVIDER_ID => profile
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com".into()),
        BEDROCK_PROVIDER_ID => format!(
            "bedrock:{}",
            profile
                .region
                .as_deref()
                .map(str::trim)
                .filter(|region| !region.is_empty())
                .unwrap_or("unknown-region")
        ),
        VERTEX_PROVIDER_ID => format!(
            "vertex:{}:{}",
            profile
                .project_id
                .as_deref()
                .map(str::trim)
                .filter(|project_id| !project_id.is_empty())
                .unwrap_or("unknown-project"),
            profile
                .region
                .as_deref()
                .map(str::trim)
                .filter(|region| !region.is_empty())
                .unwrap_or("unknown-region")
        ),
        _ => format!("{}:{}", profile.runtime_kind, profile.provider_id),
    };

    let account_class = match profile.credential_link.as_ref() {
        Some(ProviderCredentialLink::OpenAiCodex {
            account_id,
            updated_at,
            ..
        }) => format!(
            "openai_codex:{}:{updated_at}",
            xero_agent_core::runtime_trace_id("provider-account", &[account_id])
        ),
        Some(ProviderCredentialLink::Xai {
            account_id,
            updated_at,
            ..
        }) => format!(
            "xai:{}:{updated_at}",
            xero_agent_core::runtime_trace_id("provider-account", &[account_id])
        ),
        Some(ProviderCredentialLink::ApiKey { updated_at }) => format!("api_key:{updated_at}"),
        Some(ProviderCredentialLink::Local { updated_at }) => format!("local:{updated_at}"),
        Some(ProviderCredentialLink::Ambient { updated_at }) => format!("ambient:{updated_at}"),
        None => {
            let runtime_settings =
                runtime_settings_snapshot_for_provider_profile(provider_profiles, profile)?;
            if runtime_settings.provider_api_key.is_some() {
                "api_key:unlinked".into()
            } else {
                "none".into()
            }
        }
    };

    Ok((endpoint_fingerprint, account_class))
}

pub(crate) fn static_provider_preflight_snapshot(
    provider_id: &str,
    model_id: &str,
    required_features: ProviderPreflightRequiredFeatures,
) -> ProviderPreflightSnapshot {
    let now = crate::auth::now_timestamp();
    let context_limit = resolve_context_limit(provider_id, model_id);
    provider_preflight_snapshot(ProviderPreflightInput {
        profile_id: provider_id.into(),
        provider_id: provider_id.into(),
        model_id: model_id.into(),
        source: ProviderPreflightSource::StaticManual,
        checked_at: now,
        age_seconds: None,
        ttl_seconds: None,
        required_features,
        capabilities: provider_capability_catalog(ProviderCapabilityCatalogInput {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            catalog_source: "manual".into(),
            fetched_at: None,
            last_success_at: None,
            cache_age_seconds: None,
            cache_ttl_seconds: Some(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
            credential_proof: None,
            context_window_tokens: context_limit.context_window_tokens,
            max_output_tokens: context_limit.max_output_tokens,
            context_limit_source: Some(
                session_context_limit_source_name(&context_limit.source).into(),
            ),
            context_limit_confidence: Some(
                session_context_limit_confidence_name(&context_limit.confidence).into(),
            ),
            thinking_supported: false,
            thinking_efforts: Vec::new(),
            thinking_default_effort: None,
            input_modalities: Vec::new(),
            input_modalities_source: Some("unknown".into()),
        }),
        credential_ready: None,
        endpoint_reachable: None,
        model_available: None,
        streaming_route_available: None,
        tool_schema_accepted: None,
        reasoning_controls_accepted: None,
        attachments_accepted: None,
        context_limit_known: None,
        provider_error: None,
    })
}

fn session_context_limit_source_name(source: &SessionContextLimitSourceDto) -> &'static str {
    match source {
        SessionContextLimitSourceDto::LiveCatalog => "live_catalog",
        SessionContextLimitSourceDto::AppProfile => "app_profile",
        SessionContextLimitSourceDto::BuiltInRegistry => "built_in_registry",
        SessionContextLimitSourceDto::Heuristic => "heuristic",
        SessionContextLimitSourceDto::Unknown => "unknown",
    }
}

fn session_context_limit_confidence_name(
    confidence: &SessionContextLimitConfidenceDto,
) -> &'static str {
    match confidence {
        SessionContextLimitConfidenceDto::High => "high",
        SessionContextLimitConfidenceDto::Medium => "medium",
        SessionContextLimitConfidenceDto::Low => "low",
        SessionContextLimitConfidenceDto::Unknown => "unknown",
    }
}

fn live_xai_preflight_for_profile(
    provider_profiles: &ProviderCredentialsView,
    profile: &ProviderCredentialProfile,
    selected_model_id: &str,
    required_features: ProviderPreflightRequiredFeatures,
    catalog: &ProviderModelCatalog,
) -> CommandResult<Option<ProviderPreflightSnapshot>> {
    if profile.provider_id != XAI_PROVIDER_ID {
        return Ok(None);
    }

    let selected_model = catalog
        .models
        .iter()
        .find(|model| model.model_id == selected_model_id);
    if selected_model.is_none() {
        return Ok(None);
    }
    let context_window_tokens = selected_model.and_then(|model| model.context_window_tokens);
    let max_output_tokens = selected_model.and_then(|model| model.max_output_tokens);

    Ok(Some(run_xai_provider_preflight_probe(
        XaiProviderPreflightProbeRequest {
            profile_id: profile.profile_id.clone(),
            provider_id: profile.provider_id.clone(),
            model_id: selected_model_id.into(),
            base_url: XAI_BASE_URL.into(),
            bearer_token: xai_preflight_bearer_token(provider_profiles, profile),
            timeout_ms: PROVIDER_PREFLIGHT_LIVE_PROBE_TIMEOUT_MS,
            required_features,
            credential_proof: Some("app_data_profile".into()),
            context_window_tokens,
            max_output_tokens,
            context_limit_source: selected_model
                .and_then(|model| model.context_limit_source.as_ref())
                .map(context_limit_source_label)
                .or_else(|| context_window_tokens.map(|_| "provider_catalog".into())),
            context_limit_confidence: selected_model
                .and_then(|model| model.context_limit_confidence.as_ref())
                .map(context_limit_confidence_label)
                .or_else(|| context_window_tokens.map(|_| "medium".into())),
            thinking_supported: selected_model
                .map(|model| model.thinking.supported)
                .unwrap_or(false),
            thinking_efforts: selected_model
                .map(|model| {
                    model
                        .thinking
                        .effort_options
                        .iter()
                        .map(thinking_effort_label)
                        .collect()
                })
                .unwrap_or_default(),
            thinking_default_effort: selected_model
                .and_then(|model| model.thinking.default_effort.as_ref())
                .map(thinking_effort_label),
            input_modalities: selected_model
                .map(|model| model.input_modalities.clone())
                .unwrap_or_default(),
            input_modalities_source: selected_model
                .map(|model| model.input_modalities_source.clone()),
        },
    )))
}

fn live_openai_compatible_preflight_for_profile(
    state: &DesktopState,
    provider_profiles: &ProviderCredentialsView,
    profile: &ProviderCredentialProfile,
    selected_model_id: &str,
    required_features: ProviderPreflightRequiredFeatures,
    catalog: &ProviderModelCatalog,
) -> CommandResult<Option<ProviderPreflightSnapshot>> {
    let runtime_settings =
        runtime_settings_snapshot_for_provider_profile(provider_profiles, profile)?;
    let (base_url, api_version, api_key) = match profile.provider_id.as_str() {
        OPENAI_API_PROVIDER_ID
        | DEEPSEEK_PROVIDER_ID
        | GITHUB_MODELS_PROVIDER_ID
        | AZURE_OPENAI_PROVIDER_ID
        | GEMINI_AI_STUDIO_PROVIDER_ID
        | OLLAMA_PROVIDER_ID => {
            let endpoint = resolve_openai_compatible_endpoint_for_profile(
                profile,
                &state.openai_compatible_auth_config(),
            )
            .map_err(|error| {
                let code = error.code;
                let message = error.message;
                CommandError::user_fixable(
                    code,
                    format!("Xero could not resolve provider preflight endpoint: {message}"),
                )
            })?;
            (
                endpoint.effective_base_url,
                endpoint.api_version,
                runtime_settings.provider_api_key.clone(),
            )
        }
        OPENROUTER_PROVIDER_ID => (
            OPENROUTER_BASE_URL.into(),
            None,
            runtime_settings.provider_api_key.clone(),
        ),
        _ => return Ok(None),
    };

    let selected_model = catalog
        .models
        .iter()
        .find(|model| model.model_id == selected_model_id);
    let context_window_tokens = selected_model.and_then(|model| model.context_window_tokens);
    let max_output_tokens = selected_model.and_then(|model| model.max_output_tokens);
    let context_limit_confidence = selected_model
        .and_then(|model| model.context_limit_confidence.as_ref())
        .map(context_limit_confidence_label)
        .or_else(|| context_window_tokens.map(|_| "medium".into()));

    Ok(Some(run_openai_compatible_provider_preflight_probe(
        OpenAiCompatibleProviderPreflightProbeRequest {
            profile_id: profile.profile_id.clone(),
            provider_id: profile.provider_id.clone(),
            model_id: selected_model_id.into(),
            base_url,
            api_version,
            api_key,
            timeout_ms: PROVIDER_PREFLIGHT_LIVE_PROBE_TIMEOUT_MS,
            required_features,
            credential_proof: Some("app_data_profile".into()),
            context_window_tokens,
            max_output_tokens,
            context_limit_source: selected_model
                .and_then(|model| model.context_limit_source.as_ref())
                .map(context_limit_source_label)
                .or_else(|| context_window_tokens.map(|_| "provider_catalog".into())),
            context_limit_confidence,
            thinking_supported: selected_model
                .map(|model| model.thinking.supported)
                .unwrap_or(false),
            thinking_efforts: selected_model
                .map(|model| {
                    model
                        .thinking
                        .effort_options
                        .iter()
                        .map(thinking_effort_label)
                        .collect()
                })
                .unwrap_or_default(),
            thinking_default_effort: selected_model
                .and_then(|model| model.thinking.default_effort.as_ref())
                .map(thinking_effort_label),
            input_modalities: selected_model
                .map(|model| model.input_modalities.clone())
                .unwrap_or_default(),
            input_modalities_source: selected_model
                .map(|model| model.input_modalities_source.clone()),
        },
    )))
}

fn live_openai_codex_preflight_for_profile<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile: &ProviderCredentialProfile,
    selected_model_id: &str,
    required_features: ProviderPreflightRequiredFeatures,
    catalog: &ProviderModelCatalog,
) -> CommandResult<Option<ProviderPreflightSnapshot>> {
    if profile.provider_id != OPENAI_CODEX_PROVIDER_ID {
        return Ok(None);
    }

    let auth_store_path = state.global_db_path(app)?;
    let session_available = match profile.credential_link.as_ref() {
        Some(link) => load_openai_codex_session_for_profile_link(&auth_store_path, link)
            .map_err(crate::commands::runtime_support::command_error_from_auth)?,
        None => load_latest_openai_codex_session(&auth_store_path)
            .map_err(crate::commands::runtime_support::command_error_from_auth)?,
    }
    .is_some();

    Ok(Some(openai_codex_authenticated_preflight_snapshot(
        profile,
        selected_model_id,
        required_features,
        catalog,
        session_available,
    )))
}

fn openai_codex_authenticated_preflight_snapshot(
    profile: &ProviderCredentialProfile,
    selected_model_id: &str,
    required_features: ProviderPreflightRequiredFeatures,
    catalog: &ProviderModelCatalog,
    session_available: bool,
) -> ProviderPreflightSnapshot {
    let now = crate::auth::now_timestamp();
    let selected_model = catalog
        .models
        .iter()
        .find(|model| model.model_id == selected_model_id);
    let capability = openai_codex_capability_catalog_with_credential_proof(
        catalog,
        selected_model_id,
        selected_model,
        session_available,
        &now,
    );
    let context_limit_known = selected_model.is_some_and(|model| {
        model.context_window_tokens.is_some()
            && model
                .context_limit_confidence
                .as_ref()
                .is_some_and(|confidence| {
                    !matches!(confidence, SessionContextLimitConfidenceDto::Unknown)
                })
    });
    let streaming_supported = capability.capabilities.streaming.status == "supported"
        || capability.capabilities.streaming.status == "probed";
    let tool_schema_supported = capability.capabilities.tool_calls.status == "supported"
        || capability.capabilities.tool_calls.status == "probed";
    let reasoning_supported = capability.capabilities.reasoning.status == "supported"
        || capability.capabilities.reasoning.status == "probed";
    let attachments_supported = capability.capabilities.attachments.status == "supported"
        || capability.capabilities.attachments.status == "probed";

    provider_preflight_snapshot(ProviderPreflightInput {
        profile_id: profile.profile_id.clone(),
        provider_id: profile.provider_id.clone(),
        model_id: selected_model_id.into(),
        source: ProviderPreflightSource::LiveProbe,
        checked_at: now,
        age_seconds: Some(0),
        ttl_seconds: None,
        required_features: required_features.clone(),
        capabilities: capability,
        credential_ready: Some(session_available),
        endpoint_reachable: session_available.then_some(true),
        model_available: Some(selected_model.is_some()),
        streaming_route_available: required_features.streaming.then_some(streaming_supported),
        tool_schema_accepted: required_features.tool_calls.then_some(tool_schema_supported),
        reasoning_controls_accepted: required_features
            .reasoning_controls
            .then_some(reasoning_supported),
        attachments_accepted: required_features
            .attachments
            .then_some(attachments_supported),
        context_limit_known: Some(context_limit_known),
        provider_error: (!session_available).then(|| ProviderPreflightError {
            code: "provider_preflight_credentials_missing".into(),
            message:
                "No app-local OpenAI Codex auth session is available for the selected provider profile."
                    .into(),
            class: ProviderPreflightErrorClass::Authentication,
            retryable: false,
        }),
    })
}

fn openai_codex_capability_catalog_with_credential_proof(
    catalog: &ProviderModelCatalog,
    selected_model_id: &str,
    selected_model: Option<&ProviderModelRecord>,
    session_available: bool,
    now: &str,
) -> xero_agent_core::ProviderCapabilityCatalog {
    let thinking = selected_model.map(|model| &model.thinking);
    provider_capability_catalog(ProviderCapabilityCatalogInput {
        provider_id: catalog.provider_id.clone(),
        model_id: selected_model_id.into(),
        catalog_source: "live".into(),
        fetched_at: Some(now.into()),
        last_success_at: Some(now.into()),
        cache_age_seconds: Some(0),
        cache_ttl_seconds: Some(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
        credential_proof: session_available.then(|| "app_data_openai_codex_session".into()),
        context_window_tokens: selected_model.and_then(|model| model.context_window_tokens),
        max_output_tokens: selected_model.and_then(|model| model.max_output_tokens),
        context_limit_source: selected_model
            .and_then(|model| model.context_limit_source.as_ref())
            .map(context_limit_source_label),
        context_limit_confidence: selected_model
            .and_then(|model| model.context_limit_confidence.as_ref())
            .map(context_limit_confidence_label),
        thinking_supported: thinking.is_some_and(|thinking| thinking.supported),
        thinking_efforts: thinking
            .map(|thinking| {
                thinking
                    .effort_options
                    .iter()
                    .map(thinking_effort_label)
                    .collect()
            })
            .unwrap_or_default(),
        thinking_default_effort: thinking
            .and_then(|thinking| thinking.default_effort.as_ref())
            .map(thinking_effort_label),
        input_modalities: selected_model
            .map(|model| model.input_modalities.clone())
            .unwrap_or_default(),
        input_modalities_source: selected_model.map(|model| model.input_modalities_source.clone()),
    })
}

fn provider_preflight_from_catalog(
    catalog: &ProviderModelCatalog,
    selected_model_id: &str,
    required_features: ProviderPreflightRequiredFeatures,
    credential_ready: bool,
) -> ProviderPreflightSnapshot {
    let capability = provider_capability_catalog_for_catalog(catalog, Some(selected_model_id));
    let selected_model = catalog
        .models
        .iter()
        .find(|model| model.model_id == selected_model_id);
    let source = preflight_source_for_catalog(catalog);
    let live_verified = matches!(source, ProviderPreflightSource::LiveCatalog);
    let model_available = if live_verified || matches!(source, ProviderPreflightSource::CachedProbe)
    {
        Some(selected_model.is_some())
    } else if matches!(source, ProviderPreflightSource::Unavailable) {
        Some(false)
    } else {
        None
    };
    let streaming_supported = capability.capabilities.streaming.status == "supported"
        || capability.capabilities.streaming.status == "probed";
    let context_limit_known = selected_model.is_some_and(|model| {
        model.context_window_tokens.is_some()
            && model
                .context_limit_confidence
                .as_ref()
                .is_some_and(|confidence| {
                    !matches!(confidence, SessionContextLimitConfidenceDto::Unknown)
                })
    });
    provider_preflight_snapshot(ProviderPreflightInput {
        profile_id: catalog.profile_id.clone(),
        provider_id: catalog.provider_id.clone(),
        model_id: selected_model_id.into(),
        source,
        checked_at: catalog
            .fetched_at
            .clone()
            .unwrap_or_else(crate::auth::now_timestamp),
        age_seconds: catalog.fetched_at.as_deref().and_then(catalog_age_seconds),
        ttl_seconds: Some(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
        required_features,
        capabilities: capability,
        credential_ready: Some(credential_ready),
        endpoint_reachable: match source {
            ProviderPreflightSource::LiveCatalog => Some(true),
            ProviderPreflightSource::Unavailable => Some(false),
            ProviderPreflightSource::LiveProbe
            | ProviderPreflightSource::CachedProbe
            | ProviderPreflightSource::StaticManual => None,
        },
        model_available,
        streaming_route_available: if live_verified {
            Some(streaming_supported)
        } else {
            None
        },
        tool_schema_accepted: None,
        reasoning_controls_accepted: None,
        attachments_accepted: None,
        context_limit_known: Some(context_limit_known),
        provider_error: catalog.last_refresh_error.as_ref().map(|diagnostic| {
            ProviderPreflightError {
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
                class: classify_provider_preflight_error(&diagnostic.code, &diagnostic.message),
                retryable: diagnostic.retryable,
            }
        }),
    })
}

fn preflight_source_for_catalog(catalog: &ProviderModelCatalog) -> ProviderPreflightSource {
    match catalog.source {
        ProviderModelCatalogSource::Live if catalog.provider_id != OPENAI_CODEX_PROVIDER_ID => {
            ProviderPreflightSource::LiveCatalog
        }
        ProviderModelCatalogSource::Live => ProviderPreflightSource::StaticManual,
        ProviderModelCatalogSource::Cache => ProviderPreflightSource::CachedProbe,
        ProviderModelCatalogSource::Manual => ProviderPreflightSource::StaticManual,
        ProviderModelCatalogSource::Unavailable => ProviderPreflightSource::Unavailable,
    }
}

fn context_limit_confidence_label(confidence: &SessionContextLimitConfidenceDto) -> String {
    match confidence {
        SessionContextLimitConfidenceDto::High => "high",
        SessionContextLimitConfidenceDto::Medium => "medium",
        SessionContextLimitConfidenceDto::Low => "low",
        SessionContextLimitConfidenceDto::Unknown => "unknown",
    }
    .into()
}

fn context_limit_source_label(source: &SessionContextLimitSourceDto) -> String {
    match source {
        SessionContextLimitSourceDto::LiveCatalog => "live_catalog",
        SessionContextLimitSourceDto::AppProfile => "app_profile",
        SessionContextLimitSourceDto::BuiltInRegistry => "built_in_registry",
        SessionContextLimitSourceDto::Heuristic => "heuristic",
        SessionContextLimitSourceDto::Unknown => "unknown",
    }
    .into()
}

fn thinking_effort_label(effort: &ProviderModelThinkingEffort) -> String {
    match effort {
        ProviderModelThinkingEffort::None => "none",
        ProviderModelThinkingEffort::Minimal => "minimal",
        ProviderModelThinkingEffort::Low => "low",
        ProviderModelThinkingEffort::Medium => "medium",
        ProviderModelThinkingEffort::High => "high",
        ProviderModelThinkingEffort::XHigh => "x_high",
    }
    .into()
}

fn xai_preflight_bearer_token(
    provider_profiles: &ProviderCredentialsView,
    profile: &ProviderCredentialProfile,
) -> Option<String> {
    provider_profiles
        .matched_api_key_credential_for_profile(&profile.profile_id)
        .map(|entry| entry.api_key.clone())
        .or_else(|| {
            provider_profiles
                .record_for_provider(XAI_PROVIDER_ID)
                .and_then(|record| record.oauth_access_token.clone())
        })
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

fn provider_credentials_ready_for_preflight<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile: &ProviderCredentialProfile,
) -> CommandResult<bool> {
    if profile.provider_id != OPENAI_CODEX_PROVIDER_ID {
        return Ok(profile.readiness().ready);
    }

    let auth_store_path = state.global_db_path(app)?;
    let session = match profile.credential_link.as_ref() {
        Some(link) => load_openai_codex_session_for_profile_link(&auth_store_path, link)
            .map_err(crate::commands::runtime_support::command_error_from_auth)?,
        None => load_latest_openai_codex_session(&auth_store_path)
            .map_err(crate::commands::runtime_support::command_error_from_auth)?,
    };
    Ok(session.is_some())
}

pub(crate) fn persist_provider_preflight_snapshot(
    database_path: &Path,
    snapshot: &ProviderPreflightSnapshot,
) -> CommandResult<()> {
    let payload = serde_json::to_string(snapshot).map_err(|error| {
        CommandError::system_fault(
            "provider_preflight_serialize_failed",
            format!("Xero could not serialize provider preflight metadata: {error}"),
        )
    })?;
    let required_features =
        serde_json::to_string(&snapshot.required_features).map_err(|error| {
            CommandError::system_fault(
                "provider_preflight_required_features_serialize_failed",
                format!("Xero could not serialize provider preflight required features: {error}"),
            )
        })?;
    let connection = crate::global_db::open_global_database(database_path)?;
    connection
        .execute(
            "INSERT INTO provider_preflight_results (
                profile_id, provider_id, model_id, source, status, checked_at,
                required_features_json, payload
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(profile_id, provider_id, model_id) DO UPDATE SET
                source = excluded.source,
                status = excluded.status,
                checked_at = excluded.checked_at,
                required_features_json = excluded.required_features_json,
                payload = excluded.payload",
            params![
                snapshot.profile_id,
                snapshot.provider_id,
                snapshot.model_id,
                snapshot.source.as_str(),
                snapshot.status.as_str(),
                snapshot.checked_at,
                required_features,
                payload,
            ],
        )
        .map_err(|error| {
            CommandError::retryable(
                "provider_preflight_write_failed",
                format!("Xero could not persist provider preflight metadata: {error}"),
            )
        })?;
    Ok(())
}

pub(crate) fn load_provider_preflight_snapshot(
    database_path: &Path,
    profile_id: &str,
    provider_id: &str,
    model_id: &str,
) -> CommandResult<Option<ProviderPreflightSnapshot>> {
    let connection = crate::global_db::open_global_database(database_path)?;
    let payload = connection
        .query_row(
            "SELECT payload FROM provider_preflight_results
             WHERE profile_id = ?1 AND provider_id = ?2 AND model_id = ?3",
            params![profile_id, provider_id, model_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| {
            CommandError::retryable(
                "provider_preflight_read_failed",
                format!("Xero could not read provider preflight metadata: {error}"),
            )
        })?;

    payload
        .map(|payload| {
            serde_json::from_str::<ProviderPreflightSnapshot>(&payload).map_err(|error| {
                CommandError::user_fixable(
                    "provider_preflight_decode_failed",
                    format!("Xero rejected persisted provider preflight metadata: {error}"),
                )
            })
        })
        .transpose()
}

fn classify_provider_preflight_error(code: &str, message: &str) -> ProviderPreflightErrorClass {
    let text = format!("{code} {message}").to_ascii_lowercase();
    if text.contains("402")
        || xero_agent_core::provider_preflight_message_indicates_credit_limit(&text)
    {
        ProviderPreflightErrorClass::CreditLimit
    } else if text.contains("403") || text.contains("forbidden") {
        ProviderPreflightErrorClass::Authorization
    } else if text.contains("authorization") || text.contains("not authorized") {
        ProviderPreflightErrorClass::Authorization
    } else if text.contains("401") || text.contains("unauthorized") || text.contains("auth") {
        ProviderPreflightErrorClass::Authentication
    } else if text.contains("404") || text.contains("model") && text.contains("not") {
        ProviderPreflightErrorClass::ModelUnavailable
    } else if text.contains("429") || text.contains("rate") {
        ProviderPreflightErrorClass::RateLimited
    } else if text.contains("timeout") || text.contains("reach") || text.contains("connect") {
        ProviderPreflightErrorClass::EndpointUnreachable
    } else if text.contains("decode") || text.contains("json") {
        ProviderPreflightErrorClass::Decode
    } else {
        ProviderPreflightErrorClass::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::SessionContextLimitConfidenceDto;
    use crate::provider_credentials::{
        ProviderApiKeyCredentialEntry, ProviderCredentialKind, ProviderCredentialLink,
        ProviderCredentialRecord, ProviderCredentialsView,
    };
    use crate::provider_models::{
        ProviderModelCatalogDiagnostic, ProviderModelRecord, ProviderModelThinkingCapability,
        ProviderModelThinkingEffort,
    };

    fn profile(provider_id: &str) -> ProviderCredentialProfile {
        ProviderCredentialProfile {
            profile_id: format!("{provider_id}-default"),
            provider_id: provider_id.into(),
            runtime_kind: provider_id.into(),
            label: provider_id.into(),
            model_id: "model-1".into(),
            preset_id: None,
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            credential_link: None,
            updated_at: "2026-07-17T00:00:00Z".into(),
        }
    }

    fn model(model_id: &str) -> ProviderModelRecord {
        ProviderModelRecord {
            model_id: model_id.into(),
            display_name: model_id.into(),
            thinking: ProviderModelThinkingCapability {
                supported: true,
                effort_options: vec![
                    ProviderModelThinkingEffort::Low,
                    ProviderModelThinkingEffort::High,
                ],
                default_effort: Some(ProviderModelThinkingEffort::High),
            },
            input_modalities: vec!["text".into()],
            input_modalities_source: "test".into(),
            context_window_tokens: Some(128_000),
            max_output_tokens: Some(4_096),
            context_limit_source: Some(SessionContextLimitSourceDto::LiveCatalog),
            context_limit_confidence: Some(SessionContextLimitConfidenceDto::High),
            context_limit_fetched_at: Some("2026-07-17T00:00:00Z".into()),
        }
    }

    fn catalog(provider_id: &str, source: ProviderModelCatalogSource) -> ProviderModelCatalog {
        ProviderModelCatalog {
            profile_id: format!("{provider_id}-default"),
            provider_id: provider_id.into(),
            configured_model_id: "model-1".into(),
            source,
            fetched_at: Some("2026-07-17T00:00:00Z".into()),
            last_success_at: Some("2026-07-17T00:00:00Z".into()),
            last_refresh_error: None,
            models: vec![model("model-1")],
        }
    }

    #[test]
    fn static_preflight_advertises_builtin_context_limits_for_known_models() {
        let snapshot = static_provider_preflight_snapshot(
            OPENAI_API_PROVIDER_ID,
            "gpt-4.1",
            ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
        );
        let limits = snapshot.capabilities.capabilities.context_limits;

        assert_eq!(limits.context_window_tokens, Some(128_000));
        assert!(limits.max_output_tokens.is_some_and(|tokens| tokens > 0));
        assert_eq!(limits.source, "built_in_registry");
        assert_eq!(limits.confidence, "medium");
    }

    #[test]
    fn manual_catalog_preflight_does_not_report_live_tool_schema_success() {
        let catalog = ProviderModelCatalog {
            profile_id: "openrouter-work".into(),
            provider_id: "openrouter".into(),
            configured_model_id: "openai/gpt-5.4".into(),
            source: ProviderModelCatalogSource::Manual,
            fetched_at: Some("2026-05-04T00:00:00Z".into()),
            last_success_at: Some("2026-05-04T00:00:00Z".into()),
            last_refresh_error: None,
            models: vec![ProviderModelRecord {
                model_id: "openai/gpt-5.4".into(),
                display_name: "GPT-5.4".into(),
                thinking: ProviderModelThinkingCapability {
                    supported: true,
                    effort_options: vec![ProviderModelThinkingEffort::Medium],
                    default_effort: Some(ProviderModelThinkingEffort::Medium),
                },
                input_modalities: Vec::new(),
                input_modalities_source: "unknown".into(),
                context_window_tokens: Some(128_000),
                max_output_tokens: Some(16_384),
                context_limit_source: None,
                context_limit_confidence: Some(SessionContextLimitConfidenceDto::High),
                context_limit_fetched_at: None,
            }],
        };

        let snapshot = provider_preflight_from_catalog(
            &catalog,
            "openai/gpt-5.4",
            ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
            true,
        );
        assert_eq!(snapshot.source, ProviderPreflightSource::StaticManual);
        assert!(snapshot.checks.iter().any(|check| {
            check.code == "provider_preflight_tool_schema"
                && check.status == xero_agent_core::ProviderPreflightStatus::Warning
        }));
    }

    #[test]
    fn live_catalog_preflight_admits_supported_image_attachments_without_live_probe() {
        let catalog = ProviderModelCatalog {
            profile_id: "xai-default".into(),
            provider_id: XAI_PROVIDER_ID.into(),
            configured_model_id: "grok-4.3-latest".into(),
            source: ProviderModelCatalogSource::Live,
            fetched_at: Some("2026-06-04T16:06:35Z".into()),
            last_success_at: Some("2026-06-04T16:06:35Z".into()),
            last_refresh_error: None,
            models: vec![ProviderModelRecord {
                model_id: "grok-4.3-latest".into(),
                display_name: "Grok 4.3 Latest".into(),
                thinking: ProviderModelThinkingCapability {
                    supported: true,
                    effort_options: vec![ProviderModelThinkingEffort::Low],
                    default_effort: Some(ProviderModelThinkingEffort::Low),
                },
                input_modalities: vec!["image".into(), "text".into()],
                input_modalities_source: "xai_text_runtime_default".into(),
                context_window_tokens: Some(1_000_000),
                max_output_tokens: Some(4_096),
                context_limit_source: Some(SessionContextLimitSourceDto::LiveCatalog),
                context_limit_confidence: Some(SessionContextLimitConfidenceDto::High),
                context_limit_fetched_at: Some("2026-06-04T16:06:35Z".into()),
            }],
        };
        let mut required_features = ProviderPreflightRequiredFeatures::owned_agent_text_turn();
        required_features.set_attachment_input_modalities(["image"]);

        let snapshot =
            provider_preflight_from_catalog(&catalog, "grok-4.3-latest", required_features, true);

        assert_eq!(snapshot.source, ProviderPreflightSource::LiveCatalog);
        assert_eq!(
            snapshot.status,
            xero_agent_core::ProviderPreflightStatus::Passed
        );
        assert!(xero_agent_core::provider_preflight_blockers(&snapshot).is_empty());
    }

    #[test]
    fn openai_codex_authenticated_preflight_admits_text_turn() {
        let profile = ProviderCredentialProfile {
            profile_id: "openai_codex-default".into(),
            provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
            runtime_kind: OPENAI_CODEX_PROVIDER_ID.into(),
            label: "OpenAI Codex".into(),
            model_id: "gpt-5.4".into(),
            preset_id: None,
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            credential_link: Some(ProviderCredentialLink::OpenAiCodex {
                account_id: "acct-1".into(),
                session_id: "session-1".into(),
                updated_at: "2026-05-05T15:57:13Z".into(),
            }),
            updated_at: "2026-05-05T15:57:13Z".into(),
        };
        let catalog = ProviderModelCatalog {
            profile_id: profile.profile_id.clone(),
            provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
            configured_model_id: "gpt-5.4".into(),
            source: ProviderModelCatalogSource::Live,
            fetched_at: Some("2026-05-05T15:57:13Z".into()),
            last_success_at: Some("2026-05-05T15:57:13Z".into()),
            last_refresh_error: None,
            models: vec![ProviderModelRecord {
                model_id: "gpt-5.4".into(),
                display_name: "GPT-5.4".into(),
                thinking: ProviderModelThinkingCapability {
                    supported: true,
                    effort_options: vec![ProviderModelThinkingEffort::Medium],
                    default_effort: Some(ProviderModelThinkingEffort::Medium),
                },
                input_modalities: Vec::new(),
                input_modalities_source: "unknown".into(),
                context_window_tokens: Some(400_000),
                max_output_tokens: Some(16_384),
                context_limit_source: Some(SessionContextLimitSourceDto::BuiltInRegistry),
                context_limit_confidence: Some(SessionContextLimitConfidenceDto::Medium),
                context_limit_fetched_at: None,
            }],
        };

        let snapshot = openai_codex_authenticated_preflight_snapshot(
            &profile,
            "gpt-5.4",
            ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
            &catalog,
            true,
        );

        assert_eq!(snapshot.source, ProviderPreflightSource::LiveProbe);
        assert_eq!(
            snapshot.status,
            xero_agent_core::ProviderPreflightStatus::Passed
        );
        assert!(xero_agent_core::provider_preflight_blockers(&snapshot).is_empty());
    }

    #[test]
    fn openai_codex_authenticated_preflight_admits_gpt_5_5_attachments() {
        let profile = ProviderCredentialProfile {
            profile_id: "openai_codex-default".into(),
            provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
            runtime_kind: OPENAI_CODEX_PROVIDER_ID.into(),
            label: "OpenAI Codex".into(),
            model_id: "gpt-5.5".into(),
            preset_id: None,
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            credential_link: Some(ProviderCredentialLink::OpenAiCodex {
                account_id: "acct-1".into(),
                session_id: "session-1".into(),
                updated_at: "2026-05-05T15:57:13Z".into(),
            }),
            updated_at: "2026-05-05T15:57:13Z".into(),
        };
        let catalog = ProviderModelCatalog {
            profile_id: profile.profile_id.clone(),
            provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
            configured_model_id: "gpt-5.5".into(),
            source: ProviderModelCatalogSource::Live,
            fetched_at: Some("2026-05-05T15:57:13Z".into()),
            last_success_at: Some("2026-05-05T15:57:13Z".into()),
            last_refresh_error: None,
            models: vec![ProviderModelRecord {
                model_id: "gpt-5.5".into(),
                display_name: "GPT-5.5".into(),
                thinking: ProviderModelThinkingCapability {
                    supported: true,
                    effort_options: vec![ProviderModelThinkingEffort::High],
                    default_effort: Some(ProviderModelThinkingEffort::High),
                },
                input_modalities: vec!["file".into(), "image".into(), "text".into()],
                input_modalities_source: "openai_codex_static_multimodal".into(),
                context_window_tokens: Some(272_000),
                max_output_tokens: Some(16_384),
                context_limit_source: Some(SessionContextLimitSourceDto::BuiltInRegistry),
                context_limit_confidence: Some(SessionContextLimitConfidenceDto::Medium),
                context_limit_fetched_at: None,
            }],
        };
        let mut required_features = ProviderPreflightRequiredFeatures::owned_agent_text_turn();
        required_features.set_attachment_input_modalities(["file", "image"]);

        let snapshot = openai_codex_authenticated_preflight_snapshot(
            &profile,
            "gpt-5.5",
            required_features,
            &catalog,
            true,
        );

        assert_eq!(snapshot.source, ProviderPreflightSource::LiveProbe);
        assert_eq!(
            snapshot.status,
            xero_agent_core::ProviderPreflightStatus::Passed
        );
        assert_eq!(
            snapshot.capabilities.capabilities.attachments.image_input,
            "supported"
        );
        assert_eq!(
            snapshot
                .capabilities
                .capabilities
                .attachments
                .document_input,
            "supported"
        );
        assert!(xero_agent_core::provider_preflight_blockers(&snapshot).is_empty());
    }

    #[test]
    fn openai_codex_preflight_blocks_without_auth_session() {
        let profile = ProviderCredentialProfile {
            profile_id: "openai_codex-default".into(),
            provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
            runtime_kind: OPENAI_CODEX_PROVIDER_ID.into(),
            label: "OpenAI Codex".into(),
            model_id: "gpt-5.4".into(),
            preset_id: None,
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            credential_link: None,
            updated_at: "2026-05-05T15:57:13Z".into(),
        };
        let catalog = ProviderModelCatalog {
            profile_id: profile.profile_id.clone(),
            provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
            configured_model_id: "gpt-5.4".into(),
            source: ProviderModelCatalogSource::Live,
            fetched_at: Some("2026-05-05T15:57:13Z".into()),
            last_success_at: Some("2026-05-05T15:57:13Z".into()),
            last_refresh_error: None,
            models: vec![ProviderModelRecord {
                model_id: "gpt-5.4".into(),
                display_name: "GPT-5.4".into(),
                thinking: ProviderModelThinkingCapability {
                    supported: true,
                    effort_options: vec![ProviderModelThinkingEffort::Medium],
                    default_effort: Some(ProviderModelThinkingEffort::Medium),
                },
                input_modalities: Vec::new(),
                input_modalities_source: "unknown".into(),
                context_window_tokens: Some(400_000),
                max_output_tokens: Some(16_384),
                context_limit_source: Some(SessionContextLimitSourceDto::BuiltInRegistry),
                context_limit_confidence: Some(SessionContextLimitConfidenceDto::Medium),
                context_limit_fetched_at: None,
            }],
        };

        let snapshot = openai_codex_authenticated_preflight_snapshot(
            &profile,
            "gpt-5.4",
            ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
            &catalog,
            false,
        );

        assert_eq!(snapshot.source, ProviderPreflightSource::LiveProbe);
        assert!(xero_agent_core::provider_preflight_blockers(&snapshot)
            .iter()
            .any(|check| check.code == "provider_preflight_credentials"));
    }

    #[test]
    fn xai_preflight_refresh_uses_expiry_skew() {
        let now = 1_000;
        let session = |expires_at| StoredXaiSession {
            provider_id: XAI_PROVIDER_ID.into(),
            session_id: "session-123".into(),
            account_id: "acct-123".into(),
            access_token: "access-token".into(),
            refresh_token: "refresh-token".into(),
            expires_at,
            updated_at: "2026-05-05T15:57:13Z".into(),
        };

        assert!(xai_session_needs_preflight_refresh(
            &session(now + XAI_PREFLIGHT_REFRESH_SKEW_SECONDS),
            now
        ));
        assert!(!xai_session_needs_preflight_refresh(
            &session(now + XAI_PREFLIGHT_REFRESH_SKEW_SECONDS + 1),
            now
        ));
    }

    #[test]
    fn xai_live_preflight_is_attempted_before_catalog_fallback() {
        let profile = ProviderCredentialProfile {
            profile_id: "xai-default".into(),
            provider_id: XAI_PROVIDER_ID.into(),
            runtime_kind: XAI_PROVIDER_ID.into(),
            label: "xAI".into(),
            model_id: "grok-4.3-latest".into(),
            preset_id: Some(XAI_PROVIDER_ID.into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            credential_link: None,
            updated_at: "2026-06-11T23:48:32Z".into(),
        };
        let provider_profiles = ProviderCredentialsView::from_projected_profiles_for_tests(
            profile.profile_id.clone(),
            vec![profile.clone()],
            Vec::new(),
        );
        let catalog = ProviderModelCatalog {
            profile_id: profile.profile_id.clone(),
            provider_id: XAI_PROVIDER_ID.into(),
            configured_model_id: "grok-4.3-latest".into(),
            source: ProviderModelCatalogSource::Cache,
            fetched_at: Some("2026-06-05T19:49:35Z".into()),
            last_success_at: Some("2026-06-05T19:49:35Z".into()),
            last_refresh_error: None,
            models: vec![ProviderModelRecord {
                model_id: "grok-4.3-latest".into(),
                display_name: "Grok 4.3 Latest".into(),
                thinking: ProviderModelThinkingCapability {
                    supported: true,
                    effort_options: vec![ProviderModelThinkingEffort::Low],
                    default_effort: Some(ProviderModelThinkingEffort::Low),
                },
                input_modalities: vec!["image".into(), "text".into()],
                input_modalities_source: "xai_language_models_api".into(),
                context_window_tokens: Some(1_000_000),
                max_output_tokens: Some(4_096),
                context_limit_source: Some(SessionContextLimitSourceDto::LiveCatalog),
                context_limit_confidence: Some(SessionContextLimitConfidenceDto::High),
                context_limit_fetched_at: Some("2026-06-05T19:49:35Z".into()),
            }],
        };

        let snapshot = live_xai_preflight_for_profile(
            &provider_profiles,
            &profile,
            "grok-4.3-latest",
            ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
            &catalog,
        )
        .expect("xai live preflight should not error")
        .expect("xai provider should return a live preflight snapshot");

        assert_eq!(snapshot.source, ProviderPreflightSource::LiveProbe);
        assert!(snapshot
            .checks
            .iter()
            .any(|check| check.code == "provider_preflight_credentials"
                && check.status == xero_agent_core::ProviderPreflightStatus::Failed));
    }

    #[test]
    fn provider_error_classifier_distinguishes_every_failure_class() {
        for (code, message, expected) in [
            (
                "payment_required",
                "HTTP 402 insufficient credits",
                ProviderPreflightErrorClass::CreditLimit,
            ),
            (
                "authentication_failed",
                "bad credential",
                ProviderPreflightErrorClass::Authentication,
            ),
            (
                "authorization_error",
                "access denied",
                ProviderPreflightErrorClass::Authorization,
            ),
            (
                "provider_error",
                "403 forbidden",
                ProviderPreflightErrorClass::Authorization,
            ),
            (
                "model_error",
                "model is not available",
                ProviderPreflightErrorClass::ModelUnavailable,
            ),
            (
                "rate_limit",
                "HTTP 429",
                ProviderPreflightErrorClass::RateLimited,
            ),
            (
                "transport_error",
                "connection timeout",
                ProviderPreflightErrorClass::EndpointUnreachable,
            ),
            (
                "decode_error",
                "invalid JSON",
                ProviderPreflightErrorClass::Decode,
            ),
            (
                "provider_error",
                "unexpected response",
                ProviderPreflightErrorClass::Unknown,
            ),
        ] {
            assert_eq!(classify_provider_preflight_error(code, message), expected);
        }
    }

    #[test]
    fn catalog_sources_map_to_preflight_evidence_strength() {
        for (provider_id, source, expected) in [
            (
                XAI_PROVIDER_ID,
                ProviderModelCatalogSource::Live,
                ProviderPreflightSource::LiveCatalog,
            ),
            (
                OPENAI_CODEX_PROVIDER_ID,
                ProviderModelCatalogSource::Live,
                ProviderPreflightSource::StaticManual,
            ),
            (
                XAI_PROVIDER_ID,
                ProviderModelCatalogSource::Cache,
                ProviderPreflightSource::CachedProbe,
            ),
            (
                XAI_PROVIDER_ID,
                ProviderModelCatalogSource::Manual,
                ProviderPreflightSource::StaticManual,
            ),
            (
                XAI_PROVIDER_ID,
                ProviderModelCatalogSource::Unavailable,
                ProviderPreflightSource::Unavailable,
            ),
        ] {
            assert_eq!(
                preflight_source_for_catalog(&catalog(provider_id, source)),
                expected
            );
        }
    }

    #[test]
    fn catalog_preflight_reports_missing_models_credentials_and_refresh_errors() {
        let mut unavailable = catalog(XAI_PROVIDER_ID, ProviderModelCatalogSource::Unavailable);
        unavailable.last_refresh_error = Some(ProviderModelCatalogDiagnostic {
            code: "authorization_error".into(),
            message: "403 forbidden".into(),
            retryable: false,
        });
        let snapshot = provider_preflight_from_catalog(
            &unavailable,
            "missing-model",
            ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
            false,
        );
        assert_eq!(snapshot.source, ProviderPreflightSource::Unavailable);
        assert!(snapshot.checks.iter().any(|check| {
            check.code == "provider_preflight_provider_error"
                && check.message.contains("authorization")
        }));
        assert!(snapshot.checks.iter().any(|check| {
            check.code == "provider_preflight_model"
                && check.status == xero_agent_core::ProviderPreflightStatus::Failed
        }));

        let cached = provider_preflight_from_catalog(
            &catalog(XAI_PROVIDER_ID, ProviderModelCatalogSource::Cache),
            "missing-model",
            ProviderPreflightRequiredFeatures::default(),
            true,
        );
        assert_eq!(cached.source, ProviderPreflightSource::CachedProbe);
        assert!(cached.checks.iter().any(|check| {
            check.code == "provider_preflight_model"
                && check.status == xero_agent_core::ProviderPreflightStatus::Failed
        }));

        let manual = provider_preflight_from_catalog(
            &catalog(XAI_PROVIDER_ID, ProviderModelCatalogSource::Manual),
            "missing-model",
            ProviderPreflightRequiredFeatures::default(),
            true,
        );
        assert_eq!(manual.source, ProviderPreflightSource::StaticManual);
        assert!(!manual.checks.iter().any(|check| {
            check.code == "provider_preflight_model"
                && check.status == xero_agent_core::ProviderPreflightStatus::Failed
        }));
    }

    #[test]
    fn context_and_thinking_labels_cover_all_wire_values() {
        for (source, expected) in [
            (SessionContextLimitSourceDto::LiveCatalog, "live_catalog"),
            (SessionContextLimitSourceDto::AppProfile, "app_profile"),
            (
                SessionContextLimitSourceDto::BuiltInRegistry,
                "built_in_registry",
            ),
            (SessionContextLimitSourceDto::Heuristic, "heuristic"),
            (SessionContextLimitSourceDto::Unknown, "unknown"),
        ] {
            assert_eq!(session_context_limit_source_name(&source), expected);
            assert_eq!(context_limit_source_label(&source), expected);
        }
        for (confidence, expected) in [
            (SessionContextLimitConfidenceDto::High, "high"),
            (SessionContextLimitConfidenceDto::Medium, "medium"),
            (SessionContextLimitConfidenceDto::Low, "low"),
            (SessionContextLimitConfidenceDto::Unknown, "unknown"),
        ] {
            assert_eq!(session_context_limit_confidence_name(&confidence), expected);
            assert_eq!(context_limit_confidence_label(&confidence), expected);
        }
        for (effort, expected) in [
            (ProviderModelThinkingEffort::None, "none"),
            (ProviderModelThinkingEffort::Minimal, "minimal"),
            (ProviderModelThinkingEffort::Low, "low"),
            (ProviderModelThinkingEffort::Medium, "medium"),
            (ProviderModelThinkingEffort::High, "high"),
            (ProviderModelThinkingEffort::XHigh, "x_high"),
        ] {
            assert_eq!(thinking_effort_label(&effort), expected);
        }
    }

    #[test]
    fn xai_bearer_token_prefers_profile_api_key_then_oauth_record() {
        let profile = profile(XAI_PROVIDER_ID);
        let api_key_view = ProviderCredentialsView::from_projected_profiles_for_tests(
            profile.profile_id.clone(),
            vec![profile.clone()],
            vec![ProviderApiKeyCredentialEntry {
                profile_id: profile.profile_id.clone(),
                api_key: "  api-key  ".into(),
                updated_at: "2026-07-17T00:00:00Z".into(),
            }],
        );
        assert_eq!(
            xai_preflight_bearer_token(&api_key_view, &profile).as_deref(),
            Some("api-key")
        );

        let oauth_view = ProviderCredentialsView::from_records(vec![ProviderCredentialRecord {
            provider_id: XAI_PROVIDER_ID.into(),
            kind: ProviderCredentialKind::OAuthSession,
            api_key: None,
            oauth_account_id: Some("account".into()),
            oauth_session_id: Some("session".into()),
            oauth_access_token: Some("  oauth-token  ".into()),
            oauth_refresh_token: Some("refresh".into()),
            oauth_expires_at: Some(i64::MAX),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            default_model_id: Some("model-1".into()),
            updated_at: "2026-07-17T00:00:00Z".into(),
        }]);
        let oauth_profile = oauth_view
            .active_profile()
            .expect("synthesized xAI profile");
        assert_eq!(
            xai_preflight_bearer_token(&oauth_view, oauth_profile).as_deref(),
            Some("oauth-token")
        );
    }

    #[test]
    fn non_xai_and_missing_model_skip_xai_live_probe() {
        let profiles = ProviderCredentialsView::from_projected_profiles_for_tests(
            "openai_api-default".into(),
            vec![profile(OPENAI_API_PROVIDER_ID)],
            Vec::new(),
        );
        assert!(live_xai_preflight_for_profile(
            &profiles,
            &profile(OPENAI_API_PROVIDER_ID),
            "model-1",
            ProviderPreflightRequiredFeatures::default(),
            &catalog(OPENAI_API_PROVIDER_ID, ProviderModelCatalogSource::Manual),
        )
        .expect("non-xAI provider")
        .is_none());

        let xai_profile = profile(XAI_PROVIDER_ID);
        let xai_profiles = ProviderCredentialsView::from_projected_profiles_for_tests(
            xai_profile.profile_id.clone(),
            vec![xai_profile.clone()],
            Vec::new(),
        );
        assert!(live_xai_preflight_for_profile(
            &xai_profiles,
            &xai_profile,
            "missing-model",
            ProviderPreflightRequiredFeatures::default(),
            &catalog(XAI_PROVIDER_ID, ProviderModelCatalogSource::Manual),
        )
        .expect("missing model")
        .is_none());
    }

    #[test]
    fn preflight_snapshot_persistence_round_trips_upserts_and_rejects_corruption() {
        let temp = tempfile::tempdir().expect("temp directory");
        let database_path = temp.path().join("global.sqlite");
        let mut snapshot = static_provider_preflight_snapshot(
            OPENAI_API_PROVIDER_ID,
            "gpt-4.1",
            ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
        );
        snapshot.profile_id = "profile-1".into();
        persist_provider_preflight_snapshot(&database_path, &snapshot).expect("persist snapshot");
        assert_eq!(
            load_provider_preflight_snapshot(
                &database_path,
                "profile-1",
                OPENAI_API_PROVIDER_ID,
                "gpt-4.1",
            )
            .expect("load snapshot"),
            Some(snapshot.clone())
        );
        assert!(load_provider_preflight_snapshot(
            &database_path,
            "missing-profile",
            OPENAI_API_PROVIDER_ID,
            "gpt-4.1",
        )
        .expect("missing snapshot")
        .is_none());

        snapshot.checked_at = "2026-07-18T00:00:00Z".into();
        persist_provider_preflight_snapshot(&database_path, &snapshot).expect("upsert snapshot");
        assert_eq!(
            load_provider_preflight_snapshot(
                &database_path,
                "profile-1",
                OPENAI_API_PROVIDER_ID,
                "gpt-4.1",
            )
            .expect("load updated snapshot")
            .map(|snapshot| snapshot.checked_at),
            Some("2026-07-18T00:00:00Z".into())
        );

        crate::global_db::open_global_database(&database_path)
            .expect("open global database")
            .execute(
                "UPDATE provider_preflight_results SET payload = '{}' WHERE profile_id = 'profile-1'",
                [],
            )
            .expect("corrupt persisted payload");
        assert_eq!(
            load_provider_preflight_snapshot(
                &database_path,
                "profile-1",
                OPENAI_API_PROVIDER_ID,
                "gpt-4.1",
            )
            .expect_err("corrupt snapshot must fail")
            .code,
            "provider_preflight_decode_failed"
        );
    }
}
