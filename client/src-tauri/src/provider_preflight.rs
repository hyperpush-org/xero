use std::{path::Path, time::Instant};

use rusqlite::{params, OptionalExtension};
use tauri::{AppHandle, Runtime};
use xero_agent_core::{
    provider_capability_catalog, provider_preflight_snapshot,
    run_openai_compatible_provider_preflight_probe, OpenAiCompatibleProviderPreflightProbeRequest,
    ProviderCapabilityCatalogInput, ProviderPreflightError, ProviderPreflightErrorClass,
    ProviderPreflightInput, ProviderPreflightRequiredFeatures, ProviderPreflightSnapshot,
    ProviderPreflightSource, DEFAULT_PROVIDER_CATALOG_TTL_SECONDS,
};

use crate::{
    auth::{
        load_latest_openai_codex_session, load_openai_codex_session_for_profile_link,
        openai_compatible::resolve_openai_compatible_endpoint_for_profile,
    },
    commands::{
        get_runtime_settings::runtime_settings_snapshot_for_provider_profile, CommandError,
        CommandResult, SessionContextLimitConfidenceDto, SessionContextLimitSourceDto,
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
        ANTHROPIC_PROVIDER_ID, AZURE_OPENAI_PROVIDER_ID, BEDROCK_PROVIDER_ID,
        GEMINI_AI_STUDIO_PROVIDER_ID, GITHUB_MODELS_PROVIDER_ID, OLLAMA_PROVIDER_ID,
        OPENAI_API_PROVIDER_ID, OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID,
        VERTEX_PROVIDER_ID,
    },
    state::DesktopState,
};

const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
const PROVIDER_PREFLIGHT_LIVE_PROBE_TIMEOUT_MS: u64 = 5_000;

pub(crate) fn run_selected_provider_preflight<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile_id: &str,
    model_id: Option<&str>,
    force_refresh: bool,
    required_features: ProviderPreflightRequiredFeatures,
) -> CommandResult<ProviderPreflightSnapshot> {
    let started = Instant::now();
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
        None => live_openai_compatible_preflight_for_profile(
            state,
            &provider_profiles,
            profile,
            selected_model_id,
            required_features,
            &catalog,
        )?,
    };
    let snapshot = bind_provider_preflight_cache_for_profile(
        state,
        &provider_profiles,
        profile,
        live_snapshot.unwrap_or(catalog_snapshot),
    )?;
    persist_provider_preflight_snapshot(&state.global_db_path(app)?, &snapshot)?;
    eprintln!(
        "[runtime-latency] run_selected_provider_preflight profile_id={profile_id} provider_id={} model_id={} source={:?} duration_ms={}",
        snapshot.provider_id,
        snapshot.model_id,
        snapshot.source,
        started.elapsed().as_millis()
    );
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
        OPENAI_API_PROVIDER_ID
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
            context_window_tokens: None,
            max_output_tokens: None,
            context_limit_source: Some("unknown".into()),
            context_limit_confidence: Some("unknown".into()),
            thinking_supported: false,
            thinking_efforts: Vec::new(),
            thinking_default_effort: None,
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
    let (base_url, api_key) = match profile.provider_id.as_str() {
        OPENAI_API_PROVIDER_ID
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
                runtime_settings.provider_api_key.clone(),
            )
        }
        OPENROUTER_PROVIDER_ID => (
            OPENROUTER_BASE_URL.into(),
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
        ProviderModelThinkingEffort::Minimal => "minimal",
        ProviderModelThinkingEffort::Low => "low",
        ProviderModelThinkingEffort::Medium => "medium",
        ProviderModelThinkingEffort::High => "high",
        ProviderModelThinkingEffort::XHigh => "xhigh",
    }
    .into()
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
    if text.contains("401") || text.contains("unauthorized") || text.contains("auth") {
        ProviderPreflightErrorClass::Authentication
    } else if text.contains("403") || text.contains("forbidden") {
        ProviderPreflightErrorClass::Authorization
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
    use crate::provider_credentials::ProviderCredentialLink;
    use crate::provider_models::{
        ProviderModelRecord, ProviderModelThinkingCapability, ProviderModelThinkingEffort,
    };

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
}
