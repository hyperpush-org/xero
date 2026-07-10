use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Condvar, Mutex},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use url::Url;
use xero_agent_core::{
    provider_capability_catalog, ProviderCapabilityCatalog, ProviderCapabilityCatalogInput,
    DEFAULT_PROVIDER_CATALOG_TTL_SECONDS,
};

use crate::{
    auth::{
        anthropic::{
            discovered_anthropic_family_models, AnthropicDiscoveredModel,
            AnthropicDiscoveredThinkingEffort, AnthropicFamilyProfileInput,
        },
        openai_compatible::{
            fetch_openai_compatible_models, missing_openai_compatible_api_key_error,
            resolve_openai_compatible_endpoint_for_profile, OpenAiCompatibleDiscoveredModel,
            OpenAiCompatibleDiscoveredThinkingEffort, OpenAiCompatibleModelListStrategy,
            ResolvedOpenAiCompatibleEndpoint,
        },
        openrouter::{fetch_openrouter_models, OpenRouterDiscoveredModel},
    },
    commands::{
        provider_credentials::load_provider_credentials_view, resolve_context_limit, CommandError,
        CommandResult, SessionContextLimitConfidenceDto, SessionContextLimitSourceDto,
    },
    provider_credentials::{
        ProviderCredentialKind, ProviderCredentialProfile, ProviderCredentialReadinessStatus,
        ProviderCredentialsView,
    },
    runtime::{
        is_supported_xai_reasoning_effort_model_id, is_supported_xai_text_model_id,
        ANTHROPIC_PROVIDER_ID, AZURE_OPENAI_PROVIDER_ID, BEDROCK_PROVIDER_ID, CURSOR_AUTO_MODEL_ID,
        CURSOR_DEFAULT_MODEL_ID, CURSOR_PROVIDER_ID, DEEPSEEK_PROVIDER_ID,
        GEMINI_AI_STUDIO_PROVIDER_ID, GITHUB_MODELS_PROVIDER_ID, OLLAMA_PROVIDER_ID,
        OPENAI_API_PROVIDER_ID, OPENAI_CODEX_PROVIDER_ID, OPENAI_CODEX_SUPPORTED_MODEL_IDS,
        OPENROUTER_PROVIDER_ID, VERTEX_PROVIDER_ID, XAI_PROVIDER_ID, XAI_SUPPORTED_TEXT_MODEL_IDS,
    },
    state::DesktopState,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderModelCatalogSource {
    Live,
    Cache,
    Manual,
    Unavailable,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ProviderModelThinkingEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderModelThinkingCapability {
    pub supported: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effort_options: Vec<ProviderModelThinkingEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<ProviderModelThinkingEffort>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderModelRecord {
    pub model_id: String,
    pub display_name: String,
    pub thinking: ProviderModelThinkingCapability,
    pub input_modalities: Vec<String>,
    pub input_modalities_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit_source: Option<SessionContextLimitSourceDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit_confidence: Option<SessionContextLimitConfidenceDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit_fetched_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderModelCatalogDiagnostic {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderModelCatalog {
    pub profile_id: String,
    pub provider_id: String,
    pub configured_model_id: String,
    pub source: ProviderModelCatalogSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh_error: Option<ProviderModelCatalogDiagnostic>,
    pub models: Vec<ProviderModelRecord>,
}

pub fn provider_capability_catalog_for_catalog(
    catalog: &ProviderModelCatalog,
    model_id: Option<&str>,
) -> ProviderCapabilityCatalog {
    let selected_model_id = model_id
        .map(str::trim)
        .filter(|model_id| !model_id.is_empty())
        .unwrap_or(catalog.configured_model_id.as_str());
    let model = catalog
        .models
        .iter()
        .find(|model| model.model_id == selected_model_id)
        .or_else(|| {
            catalog
                .models
                .iter()
                .find(|model| model.model_id == catalog.configured_model_id)
        });
    provider_capability_catalog_for_parts(
        catalog.provider_id.as_str(),
        selected_model_id,
        &catalog.source,
        catalog.fetched_at.as_deref(),
        catalog.last_success_at.as_deref(),
        None,
        model,
    )
}

pub fn provider_capability_catalog_for_model(
    catalog: &ProviderModelCatalog,
    model: &ProviderModelRecord,
) -> ProviderCapabilityCatalog {
    provider_capability_catalog_for_parts(
        catalog.provider_id.as_str(),
        model.model_id.as_str(),
        &catalog.source,
        catalog.fetched_at.as_deref(),
        catalog.last_success_at.as_deref(),
        None,
        Some(model),
    )
}

#[derive(Debug, Clone, Default)]
pub struct ProviderModelCatalogRefreshRegistry {
    inner: Arc<Mutex<BTreeMap<String, Arc<ProviderModelCatalogRefreshSlot>>>>,
}

#[derive(Debug, Default)]
struct ProviderModelCatalogRefreshSlot {
    state: Mutex<ProviderModelCatalogRefreshState>,
    cvar: Condvar,
}

#[derive(Debug, Clone, Default)]
struct ProviderModelCatalogRefreshState {
    running: bool,
    result: Option<ProviderModelCatalog>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachedProviderModelCatalogScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    configured_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effective_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model_list_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachedProviderModelCatalogRow {
    provider_id: String,
    #[serde(default)]
    scope: CachedProviderModelCatalogScope,
    fetched_at: String,
    last_success_at: String,
    models: Vec<ProviderModelRecord>,
}

#[derive(Debug, Clone)]
struct ProviderModelCatalogRefreshContext {
    cached_row: Option<CachedProviderModelCatalogRow>,
    cache_catalogs: BTreeMap<String, CachedProviderModelCatalogRow>,
    cache_path: PathBuf,
    cache_write_allowed: bool,
    cache_read_diagnostic: Option<ProviderModelCatalogDiagnostic>,
}

#[derive(Debug, Clone, Default)]
struct ProviderModelCatalogCacheLoad {
    catalogs: BTreeMap<String, CachedProviderModelCatalogRow>,
    write_allowed: bool,
    file_error: Option<ProviderModelCatalogDiagnostic>,
    row_errors: BTreeMap<String, ProviderModelCatalogDiagnostic>,
}

#[derive(Debug, Clone)]
enum ProviderModelCatalogRefreshTarget {
    OpenAiCodex,
    Xai,
    Cursor,
    OpenRouter,
    Anthropic,
    AnthropicAmbient,
    OpenAiCompatible(ResolvedOpenAiCompatibleEndpoint),
}

impl ProviderModelCatalogRefreshTarget {
    fn cache_scope(&self, profile: &ProviderCredentialProfile) -> CachedProviderModelCatalogScope {
        match self {
            Self::OpenAiCompatible(endpoint) => CachedProviderModelCatalogScope {
                preset_id: endpoint.preset_id.clone(),
                configured_base_url: normalized_optional_string(profile.base_url.as_deref()),
                effective_base_url: Some(endpoint.effective_base_url.clone()),
                api_version: endpoint.api_version.clone(),
                model_list_strategy: Some(match endpoint.model_list_strategy {
                    OpenAiCompatibleModelListStrategy::Live => "live".into(),
                    OpenAiCompatibleModelListStrategy::Manual => "manual".into(),
                }),
            },
            Self::Xai => CachedProviderModelCatalogScope::default(),
            _ => CachedProviderModelCatalogScope::default(),
        }
    }
}

impl ProviderModelCatalogRefreshRegistry {
    pub fn run(
        &self,
        profile_id: &str,
        operation: impl FnOnce() -> ProviderModelCatalog,
    ) -> ProviderModelCatalog {
        let slot = {
            let mut slots = self
                .inner
                .lock()
                .expect("provider model refresh registry lock poisoned");
            slots
                .entry(profile_id.to_owned())
                .or_insert_with(|| Arc::new(ProviderModelCatalogRefreshSlot::default()))
                .clone()
        };

        let mut state = slot
            .state
            .lock()
            .expect("provider model refresh slot lock poisoned");
        if state.running {
            while state.running {
                state = slot
                    .cvar
                    .wait(state)
                    .expect("provider model refresh wait lock poisoned");
            }

            if let Some(result) = &state.result {
                return result.clone();
            }
        }

        state.running = true;
        state.result = None;
        drop(state);

        let result = operation();

        let mut state = slot
            .state
            .lock()
            .expect("provider model refresh slot lock poisoned");
        state.running = false;
        state.result = Some(result.clone());
        slot.cvar.notify_all();
        result
    }
}

impl ProviderModelCatalogCacheLoad {
    fn requested_cache_row(
        &self,
        profile: &ProviderCredentialProfile,
        expected_scope: &CachedProviderModelCatalogScope,
    ) -> Option<CachedProviderModelCatalogRow> {
        self.catalogs
            .get(&profile.provider_id)
            .filter(|row| row.provider_id == profile.provider_id && row.scope == *expected_scope)
            .cloned()
    }

    fn requested_diagnostic(&self, profile_id: &str) -> Option<ProviderModelCatalogDiagnostic> {
        self.row_errors
            .get(profile_id)
            .cloned()
            .or_else(|| self.file_error.clone())
    }
}

pub fn load_provider_model_catalog<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    profile_id: &str,
    force_refresh: bool,
) -> CommandResult<ProviderModelCatalog> {
    let profile_id = profile_id.trim();
    if profile_id.is_empty() {
        return Err(CommandError::invalid_request("profileId"));
    }

    let provider_profiles = load_provider_credentials_view(app, state)?;
    let profile = provider_profiles
        .profile(profile_id)
        .or_else(|| {
            provider_profiles
                .profiles()
                .iter()
                .find(|profile| profile.provider_id == profile_id)
        })
        .cloned()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "provider_not_found",
                format!("Xero could not find provider `{profile_id}`."),
            )
        })?;

    let cache_path = state.global_db_path(app)?;
    let cache_load = load_provider_model_catalog_cache(&cache_path);
    let refresh_target = resolve_provider_model_catalog_refresh_target(&profile, state)
        .map_err(diagnostic_into_command_error)?;
    let expected_scope = refresh_target.cache_scope(&profile);
    let cache_supported = !matches!(
        refresh_target,
        ProviderModelCatalogRefreshTarget::OpenAiCodex
    );
    let cached_row = if cache_supported {
        cache_load.requested_cache_row(&profile, &expected_scope)
    } else {
        None
    };
    let cache_key = profile.provider_id.clone();
    let profile_diagnostic = readiness_diagnostic(&profile);

    if let Some(diagnostic) = profile_diagnostic.clone() {
        return Ok(match cached_row.as_ref() {
            Some(cached) => catalog_from_cached_row(&profile, cached, Some(diagnostic)),
            None => unavailable_or_manual_catalog(&profile, &refresh_target, Some(diagnostic)),
        });
    }

    if !force_refresh {
        if let Some(cached) = cached_row.as_ref() {
            return Ok(catalog_from_cached_row(&profile, cached, None));
        }
    }

    let cache_read_diagnostic = if cache_supported {
        cache_load.requested_diagnostic(&cache_key)
    } else {
        None
    };
    let cache_catalogs = cache_load.catalogs.clone();
    let cache_write_allowed = cache_supported && cache_load.write_allowed;
    let provider_profiles = provider_profiles.clone();
    let profile = profile.clone();
    let cache_path = cache_path.clone();

    let refresh_context = ProviderModelCatalogRefreshContext {
        cached_row,
        cache_catalogs,
        cache_path: cache_path.clone(),
        cache_write_allowed,
        cache_read_diagnostic,
    };

    Ok(state
        .provider_model_catalog_refresh_registry()
        .run(&cache_key, move || {
            refresh_provider_model_catalog(
                &profile,
                &provider_profiles,
                state,
                &refresh_context,
                &refresh_target,
            )
        }))
}

fn refresh_provider_model_catalog(
    profile: &ProviderCredentialProfile,
    provider_profiles: &ProviderCredentialsView,
    state: &DesktopState,
    refresh_context: &ProviderModelCatalogRefreshContext,
    refresh_target: &ProviderModelCatalogRefreshTarget,
) -> ProviderModelCatalog {
    let live_models = match refresh_target {
        ProviderModelCatalogRefreshTarget::OpenAiCodex => Ok(openai_codex_projection()),
        ProviderModelCatalogRefreshTarget::Cursor => {
            let Some(secret) =
                provider_profiles.matched_api_key_credential_for_profile(&profile.profile_id)
            else {
                let diagnostic = missing_cursor_credential_diagnostic(profile);
                return match refresh_context.cached_row.as_ref() {
                    Some(cached) => catalog_from_cached_row(profile, cached, Some(diagnostic)),
                    None => {
                        unavailable_or_manual_catalog(profile, refresh_target, Some(diagnostic))
                    }
                };
            };

            fetch_cursor_models(&secret.api_key).map(normalize_cursor_models)
        }
        ProviderModelCatalogRefreshTarget::Xai => {
            let Some(token) = xai_catalog_bearer_token(profile, provider_profiles) else {
                let diagnostic = missing_xai_credential_diagnostic(profile);
                return match refresh_context.cached_row.as_ref() {
                    Some(cached) => catalog_from_cached_row(profile, cached, Some(diagnostic)),
                    None => {
                        unavailable_or_manual_catalog(profile, refresh_target, Some(diagnostic))
                    }
                };
            };

            fetch_xai_models(&token, &state.xai_auth_config())
                .map(normalize_xai_models)
                .map_err(diagnostic_from_auth_error)
        }
        ProviderModelCatalogRefreshTarget::OpenRouter => {
            let Some(secret) =
                provider_profiles.matched_api_key_credential_for_profile(&profile.profile_id)
            else {
                let diagnostic = missing_openrouter_credential_diagnostic(profile);
                return match refresh_context.cached_row.as_ref() {
                    Some(cached) => catalog_from_cached_row(profile, cached, Some(diagnostic)),
                    None => {
                        unavailable_or_manual_catalog(profile, refresh_target, Some(diagnostic))
                    }
                };
            };

            fetch_openrouter_models(Some(&secret.api_key), &state.openrouter_auth_config())
                .map(normalize_openrouter_models)
                .map_err(diagnostic_from_auth_error)
        }
        ProviderModelCatalogRefreshTarget::Anthropic => {
            let profile_input = anthropic_family_profile_input(profile, provider_profiles);
            discovered_anthropic_family_models(&profile_input, &state.anthropic_auth_config())
                .map(|models| normalize_anthropic_models(profile.provider_id.as_str(), models))
                .map_err(diagnostic_from_auth_error)
        }
        ProviderModelCatalogRefreshTarget::AnthropicAmbient => {
            let profile_input = anthropic_family_profile_input(profile, provider_profiles);
            discovered_anthropic_family_models(&profile_input, &state.anthropic_auth_config())
                .map(|models| normalize_anthropic_models(profile.provider_id.as_str(), models))
                .map_err(diagnostic_from_auth_error)
        }
        ProviderModelCatalogRefreshTarget::OpenAiCompatible(endpoint) => {
            let readiness = profile.readiness();
            if matches!(
                readiness.status,
                ProviderCredentialReadinessStatus::Malformed
            ) {
                let diagnostic = ProviderModelCatalogDiagnostic {
                    code: "provider_credentials_unavailable".into(),
                    message: format!(
                        "Xero cannot discover models for provider `{}` because the redacted credential metadata no longer matches the saved app-local secret state.",
                        profile.provider_id
                    ),
                    retryable: false,
                };
                return match refresh_context.cached_row.as_ref() {
                    Some(cached) => catalog_from_cached_row(profile, cached, Some(diagnostic)),
                    None => {
                        unavailable_or_manual_catalog(profile, refresh_target, Some(diagnostic))
                    }
                };
            }

            let api_key = provider_profiles
                .matched_api_key_credential_for_profile(&profile.profile_id)
                .map(|secret| secret.api_key.as_str());

            match endpoint.model_list_strategy {
                OpenAiCompatibleModelListStrategy::Live => fetch_openai_compatible_models(
                    api_key,
                    endpoint,
                    &state.openai_compatible_auth_config(),
                )
                .map(|models| {
                    normalize_openai_compatible_models(endpoint.provider_id.as_str(), models)
                })
                .map_err(diagnostic_from_auth_error),
                OpenAiCompatibleModelListStrategy::Manual => {
                    Ok(manual_openai_compatible_projection(profile))
                }
            }
        }
    };

    match live_models {
        Ok(models) => {
            let now = crate::auth::now_timestamp();
            let mut models = models
                .into_iter()
                .map(|mut model| {
                    if model.context_limit_source == Some(SessionContextLimitSourceDto::LiveCatalog)
                    {
                        model.context_limit_fetched_at = Some(now.clone());
                    }
                    model
                })
                .collect::<Vec<_>>();
            enrich_model_modalities_from_openrouter(
                state,
                profile.provider_id.as_str(),
                &mut models,
            );
            let source = if matches!(
                refresh_target,
                ProviderModelCatalogRefreshTarget::AnthropicAmbient
                    | ProviderModelCatalogRefreshTarget::OpenAiCompatible(
                        ResolvedOpenAiCompatibleEndpoint {
                            model_list_strategy: OpenAiCompatibleModelListStrategy::Manual,
                            ..
                        }
                    )
            ) {
                ProviderModelCatalogSource::Manual
            } else {
                ProviderModelCatalogSource::Live
            };
            let new_row = CachedProviderModelCatalogRow {
                provider_id: profile.provider_id.clone(),
                scope: refresh_target.cache_scope(profile),
                fetched_at: now.clone(),
                last_success_at: now.clone(),
                models: models.clone(),
            };

            let mut catalog = ProviderModelCatalog {
                profile_id: profile.profile_id.clone(),
                provider_id: profile.provider_id.clone(),
                configured_model_id: profile.model_id.clone(),
                source: source.clone(),
                fetched_at: Some(now.clone()),
                last_success_at: Some(now),
                last_refresh_error: refresh_context.cache_read_diagnostic.clone(),
                models,
            };

            if source == ProviderModelCatalogSource::Manual || !refresh_context.cache_write_allowed
            {
                return catalog;
            }

            if refresh_context
                .cached_row
                .as_ref()
                .map(|cached| materially_changed(cached, &new_row))
                .unwrap_or(true)
            {
                if let Err(error) = persist_provider_model_catalog_cache(
                    &refresh_context.cache_path,
                    &refresh_context.cache_catalogs,
                    &profile.provider_id,
                    &new_row,
                ) {
                    catalog.last_refresh_error = Some(diagnostic_from_command_error(error));
                }
            }

            catalog
        }
        Err(diagnostic) => match refresh_context.cached_row.as_ref() {
            Some(cached) => catalog_from_cached_row(profile, cached, Some(diagnostic)),
            None => unavailable_or_manual_catalog(profile, refresh_target, Some(diagnostic)),
        },
    }
}

fn enrich_model_modalities_from_openrouter(
    state: &DesktopState,
    provider_id: &str,
    models: &mut [ProviderModelRecord],
) {
    if models
        .iter()
        .all(|model| !model.input_modalities.is_empty())
    {
        return;
    }
    let Some(namespace) = openrouter_namespace_for_provider(provider_id) else {
        return;
    };
    let Ok(openrouter_models) = fetch_openrouter_models(None, &state.openrouter_auth_config())
    else {
        return;
    };
    let by_id = openrouter_models
        .into_iter()
        .map(|model| (model.id.clone(), model))
        .collect::<BTreeMap<_, _>>();

    for model in models
        .iter_mut()
        .filter(|model| model.input_modalities.is_empty())
    {
        for candidate in openrouter_model_id_candidates(namespace, &model.model_id) {
            let Some(openrouter_model) = by_id.get(&candidate) else {
                continue;
            };
            if openrouter_model.input_modalities.is_empty() {
                continue;
            }
            model.input_modalities =
                normalize_input_modalities(openrouter_model.input_modalities.clone());
            model.input_modalities_source = "openrouter_public_models_api".into();
            break;
        }
    }
}

fn openrouter_namespace_for_provider(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        OPENAI_API_PROVIDER_ID => Some("openai"),
        OPENROUTER_PROVIDER_ID => Some("openrouter"),
        GITHUB_MODELS_PROVIDER_ID => Some(""),
        ANTHROPIC_PROVIDER_ID | BEDROCK_PROVIDER_ID | VERTEX_PROVIDER_ID => Some("anthropic"),
        DEEPSEEK_PROVIDER_ID => Some("deepseek"),
        GEMINI_AI_STUDIO_PROVIDER_ID => Some("google"),
        XAI_PROVIDER_ID => Some("x-ai"),
        _ => None,
    }
}

fn openrouter_model_id_candidates(namespace: &str, model_id: &str) -> Vec<String> {
    let trimmed = model_id.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    if trimmed.contains('/') {
        candidates.push(trimmed.to_owned());
    } else if namespace.is_empty() {
        return Vec::new();
    } else {
        candidates.push(format!("{namespace}/{trimmed}"));
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn openai_codex_projection() -> Vec<ProviderModelRecord> {
    OPENAI_CODEX_SUPPORTED_MODEL_IDS
        .iter()
        .map(|model_id| {
            let display_name = match *model_id {
                "gpt-5.2" => "GPT-5.2",
                "gpt-5.3-codex" => "GPT-5.3 Codex",
                "gpt-5.3-codex-spark" => "GPT-5.3 Codex Spark",
                "gpt-5.4" => "GPT-5.4",
                "gpt-5.5" => "GPT-5.5",
                other => other,
            }
            .into();
            provider_model_record(
                OPENAI_CODEX_PROVIDER_ID,
                (*model_id).into(),
                display_name,
                openai_codex_thinking_capability(model_id),
                openai_codex_input_modalities(model_id),
                openai_codex_input_modalities_source(model_id).into(),
                None,
                None,
            )
        })
        .collect()
}

fn xai_projection() -> Vec<ProviderModelRecord> {
    XAI_SUPPORTED_TEXT_MODEL_IDS
        .iter()
        .map(|model_id| xai_model_record((*model_id).into(), Vec::new(), "unknown".into()))
        .collect()
}

fn cursor_projection() -> Vec<ProviderModelRecord> {
    vec![
        provider_model_record(
            CURSOR_PROVIDER_ID,
            CURSOR_AUTO_MODEL_ID.into(),
            "Auto".into(),
            unsupported_thinking_capability(),
            Vec::new(),
            "external_agent".into(),
            None,
            None,
        ),
        provider_model_record(
            CURSOR_PROVIDER_ID,
            CURSOR_DEFAULT_MODEL_ID.into(),
            "Composer Latest".into(),
            unsupported_thinking_capability(),
            Vec::new(),
            "external_agent".into(),
            None,
            None,
        ),
    ]
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorDiscoveredModel {
    id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorBridgeModelListEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    models: Vec<CursorDiscoveredModel>,
}

fn fetch_cursor_models(
    api_key: &str,
) -> Result<Vec<CursorDiscoveredModel>, ProviderModelCatalogDiagnostic> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(ProviderModelCatalogDiagnostic {
            code: "cursor_auth_missing".into(),
            message: "Xero cannot refresh Cursor models because the saved Cursor API key is empty."
                .into(),
            retryable: false,
        });
    }

    let bridge_path = default_cursor_bridge_path();
    if !bridge_path.is_file() {
        return Err(ProviderModelCatalogDiagnostic {
            code: "cursor_model_catalog_unavailable".into(),
            message: format!(
                "Xero cannot refresh Cursor models because the Cursor SDK bridge script `{}` was not found.",
                bridge_path.display()
            ),
            retryable: false,
        });
    }

    let output = Command::new("node")
        .arg(&bridge_path)
        .arg("--list-models")
        .env("CURSOR_API_KEY", api_key)
        .output()
        .map_err(|error| ProviderModelCatalogDiagnostic {
            code: "cursor_model_catalog_unavailable".into(),
            message: format!(
                "Xero could not run the Cursor SDK bridge model listing command: {error}"
            ),
            retryable: true,
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let event = cursor_bridge_model_list_event(&stdout);
    if output.status.success() {
        if let Some(event) = event {
            if event.event_type == "completed" {
                return Ok(event.models);
            }
        }
        return Err(ProviderModelCatalogDiagnostic {
            code: "cursor_model_catalog_unavailable".into(),
            message: "Cursor model listing completed without a model catalog payload.".into(),
            retryable: true,
        });
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(ProviderModelCatalogDiagnostic {
        code: event
            .as_ref()
            .and_then(|event| event.code.clone())
            .unwrap_or_else(|| "cursor_model_catalog_unavailable".into()),
        message: event.and_then(|event| event.message).unwrap_or_else(|| {
            let combined = format!("{}{}", stdout.trim(), stderr.trim());
            if combined.trim().is_empty() {
                format!(
                    "Cursor model listing exited with status {:?}.",
                    output.status.code()
                )
            } else {
                truncate_chars(combined.trim(), 2048)
            }
        }),
        retryable: true,
    })
}

fn default_cursor_bridge_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/cursor-sdk-bridge.mjs")
}

fn cursor_bridge_model_list_event(stdout: &str) -> Option<CursorBridgeModelListEvent> {
    stdout.lines().rev().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        serde_json::from_str::<CursorBridgeModelListEvent>(trimmed).ok()
    })
}

fn normalize_cursor_models(models: Vec<CursorDiscoveredModel>) -> Vec<ProviderModelRecord> {
    let mut records = cursor_projection();
    let mut seen = records
        .iter()
        .map(|model| model.model_id.clone())
        .collect::<BTreeSet<_>>();

    for model in models {
        let model_id = model.id.trim();
        if !model_id.is_empty()
            && !is_cursor_auto_alias(model_id)
            && model_id != CURSOR_DEFAULT_MODEL_ID
            && !seen.contains(model_id)
        {
            seen.insert(model_id.to_owned());
            records.push(cursor_model_record(
                model_id,
                cursor_catalog_display_name(model_id, &model.display_name),
            ));
        }

        for alias in model.aliases {
            let alias = alias.trim();
            if alias.is_empty()
                || is_cursor_auto_alias(alias)
                || alias == CURSOR_DEFAULT_MODEL_ID
                || seen.contains(alias)
            {
                continue;
            }
            seen.insert(alias.to_owned());
            records.push(cursor_model_record(
                alias,
                cursor_catalog_display_name(alias, alias),
            ));
        }
    }

    records.sort_by(|left, right| {
        cursor_model_sort_rank(&left.model_id)
            .cmp(&cursor_model_sort_rank(&right.model_id))
            .then(left.display_name.cmp(&right.display_name))
            .then(left.model_id.cmp(&right.model_id))
    });
    records
}

fn cursor_model_record(model_id: &str, display_name: String) -> ProviderModelRecord {
    provider_model_record(
        CURSOR_PROVIDER_ID,
        model_id.into(),
        display_name,
        unsupported_thinking_capability(),
        Vec::new(),
        "external_agent".into(),
        None,
        None,
    )
}

fn cursor_catalog_display_name(model_id: &str, display_name: &str) -> String {
    let trimmed = display_name.trim();
    if !trimmed.is_empty() {
        return trimmed.into();
    }
    model_id.into()
}

fn is_cursor_auto_alias(model_id: &str) -> bool {
    matches!(
        model_id
            .trim()
            .replace('_', "-")
            .to_ascii_lowercase()
            .as_str(),
        "auto" | "default" | CURSOR_AUTO_MODEL_ID
    )
}

fn cursor_model_sort_rank(model_id: &str) -> u8 {
    match model_id {
        CURSOR_AUTO_MODEL_ID => 0,
        CURSOR_DEFAULT_MODEL_ID => 1,
        _ => 2,
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.into();
    }
    value.chars().take(max_chars).collect()
}

fn xai_model_record(
    model_id: String,
    input_modalities: Vec<String>,
    input_modalities_source: String,
) -> ProviderModelRecord {
    let (input_modalities, input_modalities_source) =
        xai_model_input_modalities(&model_id, input_modalities, input_modalities_source);
    provider_model_record(
        XAI_PROVIDER_ID,
        model_id.clone(),
        xai_display_name(&model_id),
        xai_thinking_capability(&model_id),
        input_modalities,
        input_modalities_source,
        xai_context_window_tokens(&model_id),
        None,
    )
}

fn xai_model_input_modalities(
    model_id: &str,
    input_modalities: Vec<String>,
    input_modalities_source: String,
) -> (Vec<String>, String) {
    let mut normalized = normalize_input_modalities(input_modalities);
    let mut source = normalize_modality_source(input_modalities_source);
    if !is_supported_xai_text_model_id(model_id) {
        return (normalized, source);
    }

    let original_len = normalized.len();
    if !normalized.iter().any(|modality| modality == "text") {
        normalized.push("text".into());
    }
    if !normalized.iter().any(|modality| modality == "image") {
        normalized.push("image".into());
    }
    normalized.sort();
    normalized.dedup();

    if normalized.len() != original_len && source == "unknown" {
        source = "xai_text_runtime_default".into();
    }

    (normalized, source)
}

#[derive(Debug, Deserialize)]
struct XaiModelListResponse {
    #[serde(default)]
    models: Vec<XaiModelEntry>,
    #[serde(default)]
    data: Vec<XaiModelEntry>,
}

#[derive(Debug, Deserialize)]
struct XaiModelEntry {
    id: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    input_modalities: Vec<String>,
}

fn fetch_xai_models(
    bearer_token: &str,
    config: &crate::auth::XaiAuthConfig,
) -> Result<Vec<XaiModelEntry>, crate::auth::AuthFlowError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(config.timeout)
        .build()
        .map_err(|error| {
            crate::auth::AuthFlowError::terminal(
                "xai_model_catalog_http_client_unavailable",
                crate::commands::RuntimeAuthPhase::Failed,
                format!("Xero could not build the xAI model catalog HTTP client: {error}"),
            )
        })?;
    let response = client
        .get("https://api.x.ai/v1/language-models")
        .bearer_auth(bearer_token.trim())
        .send()
        .map_err(|error| {
            crate::auth::AuthFlowError::retryable(
                "xai_model_catalog_unreachable",
                crate::commands::RuntimeAuthPhase::Failed,
                format!("Xero could not reach the xAI model catalog: {error}"),
            )
        })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(crate::auth::AuthFlowError::new(
            if matches!(status.as_u16(), 401 | 403) {
                "xai_model_catalog_auth_failed"
            } else if status.is_server_error() {
                "xai_model_catalog_unavailable"
            } else {
                "xai_model_catalog_rejected"
            },
            crate::commands::RuntimeAuthPhase::Failed,
            format!(
                "xAI returned HTTP {} while discovering models.{}",
                status.as_u16(),
                if body.trim().is_empty() {
                    String::new()
                } else {
                    format!(" Response: {}", body.trim())
                }
            ),
            status.is_server_error(),
        ));
    }

    let payload: XaiModelListResponse = response.json().map_err(|error| {
        crate::auth::AuthFlowError::terminal(
            "xai_model_catalog_decode_failed",
            crate::commands::RuntimeAuthPhase::Failed,
            format!("Xero could not decode the xAI model catalog response: {error}"),
        )
    })?;
    Ok(if payload.models.is_empty() {
        payload.data
    } else {
        payload.models
    })
}

fn normalize_xai_models(models: Vec<XaiModelEntry>) -> Vec<ProviderModelRecord> {
    let normalized = models
        .into_iter()
        .flat_map(|model| {
            let input_modalities = model.input_modalities;
            let mut ids = std::iter::once(model.id)
                .chain(model.aliases)
                .filter_map(|model_id| {
                    let model_id = model_id.trim().to_owned();
                    (!model_id.is_empty() && is_supported_xai_text_model_id(&model_id))
                        .then_some(model_id)
                })
                .collect::<Vec<_>>();
            ids.sort();
            ids.dedup();
            ids.into_iter().map(move |model_id| {
                xai_model_record(
                    model_id,
                    input_modalities.clone(),
                    "xai_language_models_api".into(),
                )
            })
        })
        .collect::<Vec<_>>();

    finalize_xai_models(normalized)
}

fn xai_cached_models(models: &[ProviderModelRecord]) -> Vec<ProviderModelRecord> {
    let normalized = models
        .iter()
        .filter_map(|model| {
            let model_id = model.model_id.trim().to_owned();
            if model_id.is_empty() || !is_supported_xai_text_model_id(&model_id) {
                return None;
            }
            Some(xai_model_record(
                model_id,
                model.input_modalities.clone(),
                model.input_modalities_source.clone(),
            ))
        })
        .collect::<Vec<_>>();

    finalize_xai_models(normalized)
}

fn finalize_xai_models(mut normalized: Vec<ProviderModelRecord>) -> Vec<ProviderModelRecord> {
    for projection in xai_projection() {
        if !normalized
            .iter()
            .any(|model| model.model_id == projection.model_id)
        {
            normalized.push(projection);
        }
    }
    normalized.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then(left.model_id.cmp(&right.model_id))
    });
    normalized.dedup_by(|left, right| left.model_id == right.model_id);
    normalized
}

fn xai_display_name(model_id: &str) -> String {
    match model_id {
        "grok-4.5" => "Grok 4.5".into(),
        "grok-latest" => "Grok Latest".into(),
        "grok-4.3" => "Grok 4.3".into(),
        "grok-build-0.1" => "Grok Build 0.1".into(),
        other => {
            let parts = other
                .split(['-', '_'])
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            let mut out = Vec::new();
            let mut index = 0;
            while index < parts.len() {
                let part = parts[index];
                let lower = part.to_ascii_lowercase();
                if part.len() == 4 && part.chars().all(|c| c.is_ascii_digit()) {
                    index += 1;
                    continue;
                }
                let next = parts.get(index + 1).map(|value| value.to_ascii_lowercase());
                if lower == "non" && next.as_deref() == Some("reasoning") {
                    out.push("Non-reasoning".to_owned());
                    index += 2;
                    continue;
                }
                if lower == "multi" && next.as_deref() == Some("agent") {
                    out.push("Multi-agent".to_owned());
                    index += 2;
                    continue;
                }
                if lower == "grok" {
                    out.push("Grok".to_owned());
                    index += 1;
                    continue;
                }
                if lower == "xai" {
                    out.push("xAI".to_owned());
                    index += 1;
                    continue;
                }
                if part.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    out.push(normalize_xai_version_part(part));
                    index += 1;
                    continue;
                }
                let mut chars = lower.chars();
                if let Some(first) = chars.next() {
                    out.push(format!("{}{}", first.to_ascii_uppercase(), chars.as_str()));
                }
                index += 1;
            }
            if out.is_empty() {
                other.into()
            } else {
                out.join(" ")
            }
        }
    }
}

fn normalize_xai_version_part(part: &str) -> String {
    let Some((major, minor)) = part.split_once('.') else {
        return part.to_owned();
    };
    let normalized_minor = minor.trim_end_matches('0');
    format!(
        "{major}.{}",
        if normalized_minor.is_empty() {
            "0"
        } else {
            normalized_minor
        }
    )
}

fn xai_thinking_capability(model_id: &str) -> ProviderModelThinkingCapability {
    if is_supported_xai_reasoning_effort_model_id(model_id) {
        supported_thinking_capability_with_default(
            vec![
                ProviderModelThinkingEffort::None,
                ProviderModelThinkingEffort::Low,
                ProviderModelThinkingEffort::Medium,
                ProviderModelThinkingEffort::High,
            ],
            ProviderModelThinkingEffort::Low,
        )
    } else {
        unsupported_thinking_capability()
    }
}

fn xai_context_window_tokens(model_id: &str) -> Option<u64> {
    let model_id = model_id
        .trim()
        .rsplit('/')
        .next()
        .unwrap_or(model_id)
        .to_ascii_lowercase();
    match model_id.as_str() {
        "grok-4.5" => Some(500_000),
        "grok-build-0.1" => Some(256_000),
        _ if is_supported_xai_text_model_id(&model_id) => Some(1_000_000),
        _ => None,
    }
}

fn openai_codex_thinking_capability(model_id: &str) -> ProviderModelThinkingCapability {
    let mut effort_options = vec![
        ProviderModelThinkingEffort::Minimal,
        ProviderModelThinkingEffort::Low,
        ProviderModelThinkingEffort::Medium,
        ProviderModelThinkingEffort::High,
    ];
    if openai_codex_supports_x_high_thinking(model_id) {
        effort_options.push(ProviderModelThinkingEffort::XHigh);
    }

    supported_thinking_capability(effort_options)
}

fn openai_codex_input_modalities(model_id: &str) -> Vec<String> {
    openai_gpt_attachment_input_modalities(OPENAI_CODEX_PROVIDER_ID, model_id).unwrap_or_default()
}

fn openai_codex_input_modalities_source(model_id: &str) -> &'static str {
    if openai_gpt_attachment_input_modalities(OPENAI_CODEX_PROVIDER_ID, model_id).is_some() {
        "openai_static_gpt_multimodal"
    } else {
        "unknown"
    }
}

fn openai_codex_supports_x_high_thinking(model_id: &str) -> bool {
    let model_id = model_id.trim().to_ascii_lowercase();
    ["gpt-5.2", "gpt-5.3", "gpt-5.4", "gpt-5.5"]
        .iter()
        .any(|marker| model_id.contains(marker))
}

#[expect(
    clippy::too_many_arguments,
    reason = "provider catalog records are normalized from the complete upstream model payload"
)]
fn provider_model_record(
    provider_id: &str,
    model_id: String,
    display_name: String,
    thinking: ProviderModelThinkingCapability,
    input_modalities: Vec<String>,
    input_modalities_source: String,
    live_context_window_tokens: Option<u64>,
    live_max_output_tokens: Option<u64>,
) -> ProviderModelRecord {
    let context_resolution = resolve_context_limit(provider_id, &model_id);
    let has_live_limit = live_context_window_tokens
        .filter(|tokens| *tokens > 0)
        .is_some();
    ProviderModelRecord {
        model_id,
        display_name,
        thinking,
        input_modalities: normalize_input_modalities(input_modalities),
        input_modalities_source: normalize_modality_source(input_modalities_source),
        context_window_tokens: live_context_window_tokens
            .or(context_resolution.context_window_tokens),
        max_output_tokens: live_max_output_tokens.or(context_resolution.max_output_tokens),
        context_limit_source: Some(if has_live_limit {
            SessionContextLimitSourceDto::LiveCatalog
        } else {
            context_resolution.source
        }),
        context_limit_confidence: Some(if has_live_limit {
            SessionContextLimitConfidenceDto::High
        } else {
            context_resolution.confidence
        }),
        context_limit_fetched_at: None,
    }
}

fn normalize_input_modalities(modalities: Vec<String>) -> Vec<String> {
    let mut out = modalities
        .into_iter()
        .filter_map(|modality| {
            let normalized = modality.trim().replace('-', "_").to_ascii_lowercase();
            (!normalized.is_empty()).then_some(normalized)
        })
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

fn normalize_modality_source(source: String) -> String {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        "unknown".into()
    } else {
        trimmed.into()
    }
}

fn openai_gpt_cached_models(
    provider_id: &str,
    models: &[ProviderModelRecord],
) -> Vec<ProviderModelRecord> {
    models
        .iter()
        .map(|model| {
            let mut model = model.clone();
            apply_openai_gpt_attachment_defaults(provider_id, &mut model);
            model
        })
        .collect()
}

fn apply_openai_gpt_attachment_defaults(provider_id: &str, model: &mut ProviderModelRecord) {
    let Some(default_modalities) =
        openai_gpt_attachment_input_modalities(provider_id, &model.model_id)
    else {
        return;
    };
    let original = normalize_input_modalities(model.input_modalities.clone());
    let mut modalities = original.clone();
    for modality in default_modalities {
        if !modalities.iter().any(|existing| existing == &modality) {
            modalities.push(modality);
        }
    }
    let normalized = normalize_input_modalities(modalities);
    if normalized != original {
        model.input_modalities = normalized;
        model.input_modalities_source = "openai_static_gpt_multimodal".into();
    }
}

fn openai_gpt_attachment_input_modalities(
    provider_id: &str,
    model_id: &str,
) -> Option<Vec<String>> {
    if !matches!(
        provider_id,
        OPENAI_API_PROVIDER_ID | OPENAI_CODEX_PROVIDER_ID
    ) {
        return None;
    }
    if !is_openai_gpt_attachment_model(model_id) {
        return None;
    }
    Some(vec!["file".into(), "image".into(), "text".into()])
}

fn is_openai_gpt_attachment_model(model_id: &str) -> bool {
    let model_id = model_id
        .trim()
        .rsplit('/')
        .next()
        .unwrap_or(model_id)
        .to_ascii_lowercase();
    if model_id == "chat-latest" {
        return true;
    }
    if !model_id.starts_with("gpt-") {
        return false;
    }
    if model_id.starts_with("gpt-image")
        || model_id.starts_with("gpt-audio")
        || model_id.starts_with("gpt-realtime")
        || model_id.contains("search")
        || model_id.contains("transcribe")
        || model_id.contains("tts")
    {
        return false;
    }
    model_id == "gpt-5"
        || model_id.starts_with("gpt-5.")
        || model_id.starts_with("gpt-5-")
        || model_id == "gpt-4.1"
        || model_id.starts_with("gpt-4.1-")
        || model_id == "gpt-4o"
        || model_id.starts_with("gpt-4o-")
}

fn provider_capability_catalog_for_parts(
    provider_id: &str,
    model_id: &str,
    catalog_source: &ProviderModelCatalogSource,
    fetched_at: Option<&str>,
    last_success_at: Option<&str>,
    credential_proof: Option<String>,
    model: Option<&ProviderModelRecord>,
) -> ProviderCapabilityCatalog {
    let thinking = model.map(|model| &model.thinking);
    provider_capability_catalog(ProviderCapabilityCatalogInput {
        provider_id: provider_id.into(),
        model_id: model_id.into(),
        catalog_source: catalog_source_string(catalog_source).into(),
        fetched_at: fetched_at.map(str::to_owned),
        last_success_at: last_success_at.map(str::to_owned),
        cache_age_seconds: fetched_at.and_then(catalog_age_seconds),
        cache_ttl_seconds: Some(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
        credential_proof,
        context_window_tokens: model.and_then(|model| model.context_window_tokens),
        max_output_tokens: model.and_then(|model| model.max_output_tokens),
        context_limit_source: model
            .and_then(|model| model.context_limit_source.as_ref())
            .map(session_context_limit_source_string),
        context_limit_confidence: model
            .and_then(|model| model.context_limit_confidence.as_ref())
            .map(session_context_limit_confidence_string),
        thinking_supported: thinking.is_some_and(|thinking| thinking.supported),
        thinking_efforts: thinking
            .map(|thinking| {
                thinking
                    .effort_options
                    .iter()
                    .map(provider_model_thinking_effort_string)
                    .collect()
            })
            .unwrap_or_default(),
        thinking_default_effort: thinking
            .and_then(|thinking| thinking.default_effort.as_ref())
            .map(provider_model_thinking_effort_string),
        input_modalities: model
            .map(|model| model.input_modalities.clone())
            .unwrap_or_default(),
        input_modalities_source: model.map(|model| model.input_modalities_source.clone()),
    })
}

fn catalog_source_string(source: &ProviderModelCatalogSource) -> &'static str {
    match source {
        ProviderModelCatalogSource::Live => "live",
        ProviderModelCatalogSource::Cache => "cache",
        ProviderModelCatalogSource::Manual => "manual",
        ProviderModelCatalogSource::Unavailable => "unavailable",
    }
}

fn provider_model_thinking_effort_string(effort: &ProviderModelThinkingEffort) -> String {
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

fn session_context_limit_source_string(source: &SessionContextLimitSourceDto) -> String {
    match source {
        SessionContextLimitSourceDto::LiveCatalog => "live_catalog",
        SessionContextLimitSourceDto::AppProfile => "app_profile",
        SessionContextLimitSourceDto::BuiltInRegistry => "built_in_registry",
        SessionContextLimitSourceDto::Heuristic => "heuristic",
        SessionContextLimitSourceDto::Unknown => "unknown",
    }
    .into()
}

fn session_context_limit_confidence_string(
    confidence: &SessionContextLimitConfidenceDto,
) -> String {
    match confidence {
        SessionContextLimitConfidenceDto::High => "high",
        SessionContextLimitConfidenceDto::Medium => "medium",
        SessionContextLimitConfidenceDto::Low => "low",
        SessionContextLimitConfidenceDto::Unknown => "unknown",
    }
    .into()
}

pub fn catalog_age_seconds(fetched_at: &str) -> Option<i64> {
    let fetched_at = OffsetDateTime::parse(fetched_at, &Rfc3339).ok()?;
    let duration = OffsetDateTime::now_utc() - fetched_at;
    Some(duration.whole_seconds().max(0))
}

fn normalize_openrouter_models(models: Vec<OpenRouterDiscoveredModel>) -> Vec<ProviderModelRecord> {
    let mut normalized = models
        .into_iter()
        .map(|model| {
            let thinking = openrouter_thinking_capability(&model.supported_parameters);
            provider_model_record(
                OPENROUTER_PROVIDER_ID,
                model.id,
                model.display_name,
                thinking,
                model.input_modalities,
                model.input_modalities_source,
                model.context_window_tokens,
                model.max_output_tokens,
            )
        })
        .collect::<Vec<_>>();

    normalized.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then(left.model_id.cmp(&right.model_id))
    });
    normalized.dedup_by(|left, right| left.model_id == right.model_id);
    normalized
}

fn normalize_anthropic_models(
    provider_id: &str,
    models: Vec<AnthropicDiscoveredModel>,
) -> Vec<ProviderModelRecord> {
    let mut normalized = models
        .into_iter()
        .map(|model| {
            let thinking = anthropic_thinking_capability(&model);
            provider_model_record(
                provider_id,
                model.id,
                model.display_name,
                thinking,
                model.input_modalities,
                model.input_modalities_source,
                None,
                None,
            )
        })
        .collect::<Vec<_>>();

    normalized.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then(left.model_id.cmp(&right.model_id))
    });
    normalized
}

fn normalize_openai_compatible_models(
    provider_id: &str,
    models: Vec<OpenAiCompatibleDiscoveredModel>,
) -> Vec<ProviderModelRecord> {
    let mut normalized = models
        .into_iter()
        .map(|model| {
            let thinking = openai_compatible_thinking_capability(&model);
            let mut record = provider_model_record(
                provider_id,
                model.id,
                model.display_name,
                thinking,
                model.input_modalities,
                model.input_modalities_source,
                model.context_window_tokens,
                model.max_output_tokens,
            );
            apply_openai_gpt_attachment_defaults(provider_id, &mut record);
            record
        })
        .collect::<Vec<_>>();

    normalized.sort_by(|left, right| {
        left.display_name
            .cmp(&right.display_name)
            .then(left.model_id.cmp(&right.model_id))
    });
    normalized
}

fn manual_provider_projection(profile: &ProviderCredentialProfile) -> Vec<ProviderModelRecord> {
    match profile.provider_id.as_str() {
        CURSOR_PROVIDER_ID => cursor_projection(),
        BEDROCK_PROVIDER_ID | VERTEX_PROVIDER_ID => manual_anthropic_family_projection(profile),
        _ => manual_openai_compatible_projection(profile),
    }
}

fn manual_openai_compatible_projection(
    profile: &ProviderCredentialProfile,
) -> Vec<ProviderModelRecord> {
    let mut record = provider_model_record(
        profile.provider_id.as_str(),
        profile.model_id.clone(),
        profile.model_id.clone(),
        unsupported_thinking_capability(),
        Vec::new(),
        "unknown".into(),
        None,
        None,
    );
    apply_openai_gpt_attachment_defaults(profile.provider_id.as_str(), &mut record);
    vec![record]
}

fn manual_anthropic_family_projection(
    profile: &ProviderCredentialProfile,
) -> Vec<ProviderModelRecord> {
    let supports_thinking = profile.model_id.to_ascii_lowercase().contains("claude");
    let thinking = if supports_thinking {
        supported_thinking_capability(vec![
            ProviderModelThinkingEffort::Low,
            ProviderModelThinkingEffort::Medium,
            ProviderModelThinkingEffort::High,
        ])
    } else {
        unsupported_thinking_capability()
    };

    vec![provider_model_record(
        profile.provider_id.as_str(),
        profile.model_id.clone(),
        profile.model_id.clone(),
        thinking,
        Vec::new(),
        "unknown".into(),
        None,
        None,
    )]
}

fn openai_compatible_thinking_capability(
    model: &OpenAiCompatibleDiscoveredModel,
) -> ProviderModelThinkingCapability {
    if !model.thinking.supported {
        return unsupported_thinking_capability();
    }

    ProviderModelThinkingCapability {
        supported: true,
        effort_options: model
            .thinking
            .effort_levels
            .iter()
            .map(|effort| match effort {
                OpenAiCompatibleDiscoveredThinkingEffort::Minimal => {
                    ProviderModelThinkingEffort::Minimal
                }
                OpenAiCompatibleDiscoveredThinkingEffort::Low => ProviderModelThinkingEffort::Low,
                OpenAiCompatibleDiscoveredThinkingEffort::Medium => {
                    ProviderModelThinkingEffort::Medium
                }
                OpenAiCompatibleDiscoveredThinkingEffort::High => ProviderModelThinkingEffort::High,
                OpenAiCompatibleDiscoveredThinkingEffort::XHigh => {
                    ProviderModelThinkingEffort::XHigh
                }
            })
            .collect(),
        default_effort: model.thinking.default_effort.map(|effort| match effort {
            OpenAiCompatibleDiscoveredThinkingEffort::Minimal => {
                ProviderModelThinkingEffort::Minimal
            }
            OpenAiCompatibleDiscoveredThinkingEffort::Low => ProviderModelThinkingEffort::Low,
            OpenAiCompatibleDiscoveredThinkingEffort::Medium => ProviderModelThinkingEffort::Medium,
            OpenAiCompatibleDiscoveredThinkingEffort::High => ProviderModelThinkingEffort::High,
            OpenAiCompatibleDiscoveredThinkingEffort::XHigh => ProviderModelThinkingEffort::XHigh,
        }),
    }
}

fn anthropic_thinking_capability(
    model: &AnthropicDiscoveredModel,
) -> ProviderModelThinkingCapability {
    if !model.thinking_supported {
        return unsupported_thinking_capability();
    }

    supported_thinking_capability(
        model
            .effort_levels
            .iter()
            .map(|effort| match effort {
                AnthropicDiscoveredThinkingEffort::Low => ProviderModelThinkingEffort::Low,
                AnthropicDiscoveredThinkingEffort::Medium => ProviderModelThinkingEffort::Medium,
                AnthropicDiscoveredThinkingEffort::High => ProviderModelThinkingEffort::High,
                AnthropicDiscoveredThinkingEffort::XHigh => ProviderModelThinkingEffort::XHigh,
            })
            .collect(),
    )
}

fn openrouter_thinking_capability(
    supported_parameters: &[String],
) -> ProviderModelThinkingCapability {
    if supports_openrouter_reasoning(supported_parameters) {
        supported_thinking_capability(vec![
            ProviderModelThinkingEffort::Minimal,
            ProviderModelThinkingEffort::Low,
            ProviderModelThinkingEffort::Medium,
            ProviderModelThinkingEffort::High,
            ProviderModelThinkingEffort::XHigh,
        ])
    } else {
        unsupported_thinking_capability()
    }
}

fn supports_openrouter_reasoning(supported_parameters: &[String]) -> bool {
    supported_parameters.iter().any(|parameter| {
        let normalized = parameter.trim().to_ascii_lowercase();
        normalized == "reasoning"
            || normalized == "reasoning.effort"
            || normalized == "reasoning.max_tokens"
            || normalized == "include_reasoning"
            || normalized == "thinking_budget"
            || normalized.starts_with("reasoning.")
    })
}

fn supported_thinking_capability(
    effort_options: Vec<ProviderModelThinkingEffort>,
) -> ProviderModelThinkingCapability {
    supported_thinking_capability_with_default(effort_options, ProviderModelThinkingEffort::Medium)
}

fn supported_thinking_capability_with_default(
    effort_options: Vec<ProviderModelThinkingEffort>,
    preferred_default: ProviderModelThinkingEffort,
) -> ProviderModelThinkingCapability {
    ProviderModelThinkingCapability {
        supported: true,
        default_effort: effort_options
            .iter()
            .copied()
            .find(|effort| *effort == preferred_default)
            .or_else(|| effort_options.first().copied()),
        effort_options,
    }
}

fn unsupported_thinking_capability() -> ProviderModelThinkingCapability {
    ProviderModelThinkingCapability {
        supported: false,
        effort_options: Vec::new(),
        default_effort: None,
    }
}

fn catalog_from_cached_row(
    profile: &ProviderCredentialProfile,
    cached: &CachedProviderModelCatalogRow,
    diagnostic: Option<ProviderModelCatalogDiagnostic>,
) -> ProviderModelCatalog {
    let models = if profile.provider_id == XAI_PROVIDER_ID {
        xai_cached_models(&cached.models)
    } else if matches!(
        profile.provider_id.as_str(),
        OPENAI_API_PROVIDER_ID | OPENAI_CODEX_PROVIDER_ID
    ) {
        openai_gpt_cached_models(profile.provider_id.as_str(), &cached.models)
    } else {
        cached.models.clone()
    };
    ProviderModelCatalog {
        profile_id: profile.profile_id.clone(),
        provider_id: profile.provider_id.clone(),
        configured_model_id: profile.model_id.clone(),
        source: ProviderModelCatalogSource::Cache,
        fetched_at: Some(cached.fetched_at.clone()),
        last_success_at: Some(cached.last_success_at.clone()),
        last_refresh_error: diagnostic,
        models,
    }
}

fn unavailable_or_manual_catalog(
    profile: &ProviderCredentialProfile,
    refresh_target: &ProviderModelCatalogRefreshTarget,
    diagnostic: Option<ProviderModelCatalogDiagnostic>,
) -> ProviderModelCatalog {
    match refresh_target {
        ProviderModelCatalogRefreshTarget::Cursor => ProviderModelCatalog {
            profile_id: profile.profile_id.clone(),
            provider_id: profile.provider_id.clone(),
            configured_model_id: profile.model_id.clone(),
            source: ProviderModelCatalogSource::Manual,
            fetched_at: Some(profile.updated_at.clone()),
            last_success_at: Some(profile.updated_at.clone()),
            last_refresh_error: diagnostic,
            models: cursor_projection(),
        },
        ProviderModelCatalogRefreshTarget::Xai => ProviderModelCatalog {
            profile_id: profile.profile_id.clone(),
            provider_id: profile.provider_id.clone(),
            configured_model_id: profile.model_id.clone(),
            source: ProviderModelCatalogSource::Manual,
            fetched_at: Some(profile.updated_at.clone()),
            last_success_at: Some(profile.updated_at.clone()),
            last_refresh_error: diagnostic,
            models: xai_projection(),
        },
        ProviderModelCatalogRefreshTarget::AnthropicAmbient
        | ProviderModelCatalogRefreshTarget::OpenAiCompatible(ResolvedOpenAiCompatibleEndpoint {
            model_list_strategy: OpenAiCompatibleModelListStrategy::Manual,
            ..
        }) => ProviderModelCatalog {
            profile_id: profile.profile_id.clone(),
            provider_id: profile.provider_id.clone(),
            configured_model_id: profile.model_id.clone(),
            source: ProviderModelCatalogSource::Manual,
            fetched_at: Some(profile.updated_at.clone()),
            last_success_at: Some(profile.updated_at.clone()),
            last_refresh_error: diagnostic,
            models: manual_provider_projection(profile),
        },
        _ => unavailable_catalog(profile, diagnostic),
    }
}

fn unavailable_catalog(
    profile: &ProviderCredentialProfile,
    diagnostic: Option<ProviderModelCatalogDiagnostic>,
) -> ProviderModelCatalog {
    ProviderModelCatalog {
        profile_id: profile.profile_id.clone(),
        provider_id: profile.provider_id.clone(),
        configured_model_id: profile.model_id.clone(),
        source: ProviderModelCatalogSource::Unavailable,
        fetched_at: None,
        last_success_at: None,
        last_refresh_error: diagnostic,
        models: Vec::new(),
    }
}

fn resolve_provider_model_catalog_refresh_target(
    profile: &ProviderCredentialProfile,
    state: &DesktopState,
) -> Result<ProviderModelCatalogRefreshTarget, ProviderModelCatalogDiagnostic> {
    match profile.provider_id.as_str() {
        OPENAI_CODEX_PROVIDER_ID => Ok(ProviderModelCatalogRefreshTarget::OpenAiCodex),
        XAI_PROVIDER_ID => Ok(ProviderModelCatalogRefreshTarget::Xai),
        CURSOR_PROVIDER_ID => Ok(ProviderModelCatalogRefreshTarget::Cursor),
        OPENROUTER_PROVIDER_ID => Ok(ProviderModelCatalogRefreshTarget::OpenRouter),
        ANTHROPIC_PROVIDER_ID => Ok(ProviderModelCatalogRefreshTarget::Anthropic),
        BEDROCK_PROVIDER_ID | VERTEX_PROVIDER_ID => {
            Ok(ProviderModelCatalogRefreshTarget::AnthropicAmbient)
        }
        OPENAI_API_PROVIDER_ID
        | DEEPSEEK_PROVIDER_ID
        | OLLAMA_PROVIDER_ID
        | AZURE_OPENAI_PROVIDER_ID
        | GITHUB_MODELS_PROVIDER_ID
        | GEMINI_AI_STUDIO_PROVIDER_ID => resolve_openai_compatible_endpoint_for_profile(
            profile,
            &state.openai_compatible_auth_config(),
        )
        .map(ProviderModelCatalogRefreshTarget::OpenAiCompatible)
        .map_err(diagnostic_from_auth_error),
        other => Err(ProviderModelCatalogDiagnostic {
            code: "provider_model_provider_unsupported".into(),
            message: format!(
                "Xero cannot discover models for provider `{other}` because that provider is not supported by the desktop host yet."
            ),
            retryable: false,
        }),
    }
}

fn materially_changed(
    current: &CachedProviderModelCatalogRow,
    next: &CachedProviderModelCatalogRow,
) -> bool {
    current.provider_id != next.provider_id
        || current.scope != next.scope
        || current.models != next.models
}

fn persist_provider_model_catalog_cache(
    path: &Path,
    _current: &BTreeMap<String, CachedProviderModelCatalogRow>,
    profile_id: &str,
    next: &CachedProviderModelCatalogRow,
) -> CommandResult<()> {
    let payload = serde_json::to_string(next).map_err(|error| {
        CommandError::system_fault(
            "provider_model_catalog_cache_serialize_failed",
            format!("Xero could not serialize the app-local provider-model catalog cache: {error}"),
        )
    })?;

    let connection = crate::global_db::open_global_database(path)?;
    connection
        .execute(
            "INSERT INTO provider_model_catalog_cache (profile_id, payload, fetched_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(profile_id) DO UPDATE SET
                payload = excluded.payload,
                fetched_at = excluded.fetched_at",
            rusqlite::params![profile_id, payload, next.fetched_at],
        )
        .map_err(|error| {
            CommandError::retryable(
                "provider_model_catalog_cache_write_failed",
                format!("Xero could not write provider model catalog cache row: {error}"),
            )
        })?;

    Ok(())
}

fn load_provider_model_catalog_cache(path: &Path) -> ProviderModelCatalogCacheLoad {
    let mut load = ProviderModelCatalogCacheLoad {
        write_allowed: true,
        ..ProviderModelCatalogCacheLoad::default()
    };

    let connection = match crate::global_db::open_global_database(path) {
        Ok(connection) => connection,
        Err(error) => {
            load.write_allowed = false;
            load.file_error = Some(ProviderModelCatalogDiagnostic {
                code: "provider_model_catalog_cache_read_failed".into(),
                message: format!(
                    "Xero could not open the global database for the provider-model catalog cache at {}: {}",
                    path.display(),
                    error.message
                ),
                retryable: error.retryable,
            });
            return load;
        }
    };

    let mut stmt = match connection
        .prepare("SELECT profile_id, payload FROM provider_model_catalog_cache ORDER BY profile_id")
    {
        Ok(stmt) => stmt,
        Err(error) => {
            load.write_allowed = false;
            load.file_error = Some(ProviderModelCatalogDiagnostic {
                code: "provider_model_catalog_cache_read_failed".into(),
                message: format!(
                    "Xero could not prepare provider-model catalog cache read: {error}"
                ),
                retryable: true,
            });
            return load;
        }
    };

    let rows = match stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) {
        Ok(rows) => rows,
        Err(error) => {
            load.write_allowed = false;
            load.file_error = Some(ProviderModelCatalogDiagnostic {
                code: "provider_model_catalog_cache_read_failed".into(),
                message: format!("Xero could not read provider-model catalog cache rows: {error}"),
                retryable: true,
            });
            return load;
        }
    };

    for row in rows {
        match row {
            Ok((profile_id, payload)) => {
                match serde_json::from_str::<CachedProviderModelCatalogRow>(&payload) {
                    Ok(parsed) => {
                        if let Err(error) = validate_cached_catalog_row(path, &profile_id, &parsed)
                        {
                            load.write_allowed = false;
                            load.row_errors.insert(profile_id, error);
                        } else {
                            load.catalogs.insert(profile_id, parsed);
                        }
                    }
                    Err(error) => {
                        load.write_allowed = false;
                        load.row_errors.insert(
                            profile_id.clone(),
                            ProviderModelCatalogDiagnostic {
                                code: "provider_model_catalog_cache_decode_failed".into(),
                                message: format!(
                                    "Xero could not decode the cached provider-model catalog row for provider connection `{profile_id}`: {error}"
                                ),
                                retryable: false,
                            },
                        );
                    }
                }
            }
            Err(error) => {
                load.write_allowed = false;
                load.file_error = Some(ProviderModelCatalogDiagnostic {
                    code: "provider_model_catalog_cache_read_failed".into(),
                    message: format!(
                        "Xero could not decode provider-model catalog cache row: {error}"
                    ),
                    retryable: true,
                });
                return load;
            }
        }
    }

    load
}

fn validate_cached_catalog_row(
    path: &Path,
    profile_id: &str,
    row: &CachedProviderModelCatalogRow,
) -> Result<(), ProviderModelCatalogDiagnostic> {
    if row.provider_id.trim().is_empty() {
        return Err(ProviderModelCatalogDiagnostic {
            code: "provider_model_catalog_cache_invalid".into(),
            message: format!(
                "Xero rejected the cached provider-model catalog row for provider connection `{profile_id}` at {} because providerId was blank.",
                path.display()
            ),
            retryable: false,
        });
    }

    for value in [
        row.scope.preset_id.as_deref(),
        row.scope.configured_base_url.as_deref(),
        row.scope.effective_base_url.as_deref(),
        row.scope.api_version.as_deref(),
        row.scope.model_list_strategy.as_deref(),
    ] {
        if value.is_some_and(|value| value.trim().is_empty()) {
            return Err(ProviderModelCatalogDiagnostic {
                code: "provider_model_catalog_cache_invalid".into(),
                message: format!(
                    "Xero rejected the cached provider-model catalog row for provider connection `{profile_id}` at {} because one cache-scope field was blank.",
                    path.display()
                ),
                retryable: false,
            });
        }
    }

    for model in &row.models {
        if model.model_id.trim().is_empty() {
            return Err(ProviderModelCatalogDiagnostic {
                code: "provider_model_catalog_cache_invalid".into(),
                message: format!(
                    "Xero rejected the cached provider-model catalog row for provider connection `{profile_id}` at {} because one modelId was blank.",
                    path.display()
                ),
                retryable: false,
            });
        }

        if model.display_name.trim().is_empty() {
            return Err(ProviderModelCatalogDiagnostic {
                code: "provider_model_catalog_cache_invalid".into(),
                message: format!(
                    "Xero rejected the cached provider-model catalog row for provider connection `{profile_id}` at {} because one displayName was blank.",
                    path.display()
                ),
                retryable: false,
            });
        }
    }

    Ok(())
}

fn readiness_diagnostic(
    profile: &ProviderCredentialProfile,
) -> Option<ProviderModelCatalogDiagnostic> {
    if !matches!(
        profile.provider_id.as_str(),
        OPENROUTER_PROVIDER_ID
            | ANTHROPIC_PROVIDER_ID
            | BEDROCK_PROVIDER_ID
            | VERTEX_PROVIDER_ID
            | XAI_PROVIDER_ID
            | CURSOR_PROVIDER_ID
            | OPENAI_API_PROVIDER_ID
            | DEEPSEEK_PROVIDER_ID
            | OLLAMA_PROVIDER_ID
            | AZURE_OPENAI_PROVIDER_ID
            | GITHUB_MODELS_PROVIDER_ID
            | GEMINI_AI_STUDIO_PROVIDER_ID
    ) {
        return None;
    }

    let readiness = profile.readiness();
    match readiness.status {
        ProviderCredentialReadinessStatus::Ready => None,
        ProviderCredentialReadinessStatus::Missing => Some(match profile.provider_id.as_str() {
            OPENROUTER_PROVIDER_ID => missing_openrouter_credential_diagnostic(profile),
            XAI_PROVIDER_ID => missing_xai_credential_diagnostic(profile),
            CURSOR_PROVIDER_ID => missing_cursor_credential_diagnostic(profile),
            ANTHROPIC_PROVIDER_ID => missing_anthropic_credential_diagnostic(profile),
            BEDROCK_PROVIDER_ID => missing_bedrock_ambient_diagnostic(profile),
            VERTEX_PROVIDER_ID => missing_vertex_ambient_diagnostic(profile),
            OPENAI_API_PROVIDER_ID
            | DEEPSEEK_PROVIDER_ID
            | OLLAMA_PROVIDER_ID
            | AZURE_OPENAI_PROVIDER_ID
            | GITHUB_MODELS_PROVIDER_ID
            | GEMINI_AI_STUDIO_PROVIDER_ID => {
                if openai_compatible_profile_uses_local_auth(profile) {
                    return None;
                }
                diagnostic_from_auth_error(missing_openai_compatible_api_key_error(
                    profile.provider_id.as_str(),
                    "discover",
                ))
            }
            _ => return None,
        }),
        ProviderCredentialReadinessStatus::Malformed => Some(match profile.provider_id.as_str() {
            OPENROUTER_PROVIDER_ID => ProviderModelCatalogDiagnostic {
                code: "provider_credentials_unavailable".into(),
                message: format!(
                    "Xero cannot discover OpenRouter models for provider `{}` because the redacted credential metadata no longer matches the saved app-local secret state.",
                    profile.provider_id
                ),
                retryable: false,
            },
            XAI_PROVIDER_ID => ProviderModelCatalogDiagnostic {
                code: "provider_credentials_unavailable".into(),
                message: format!(
                    "Xero cannot discover xAI models for provider `{}` because the redacted credential metadata no longer matches the saved app-local secret state.",
                    profile.provider_id
                ),
                retryable: false,
            },
            CURSOR_PROVIDER_ID => ProviderModelCatalogDiagnostic {
                code: "provider_credentials_unavailable".into(),
                message: format!(
                    "Xero cannot load Cursor models for provider `{}` because the redacted credential metadata no longer matches the saved app-local secret state.",
                    profile.provider_id
                ),
                retryable: false,
            },
            ANTHROPIC_PROVIDER_ID => ProviderModelCatalogDiagnostic {
                code: "provider_credentials_unavailable".into(),
                message: format!(
                    "Xero cannot discover Anthropic models for provider `{}` because the redacted credential metadata no longer matches the saved app-local secret state.",
                    profile.provider_id
                ),
                retryable: false,
            },
            OPENAI_API_PROVIDER_ID
            | DEEPSEEK_PROVIDER_ID
            | OLLAMA_PROVIDER_ID
            | AZURE_OPENAI_PROVIDER_ID
            | GITHUB_MODELS_PROVIDER_ID
            | GEMINI_AI_STUDIO_PROVIDER_ID => ProviderModelCatalogDiagnostic {
                code: "provider_credentials_unavailable".into(),
                message: format!(
                    "Xero cannot discover models for provider `{}` because the redacted credential metadata no longer matches the saved app-local secret state.",
                    profile.provider_id
                ),
                retryable: false,
            },
            _ => return None,
        }),
    }
}

fn openai_compatible_profile_uses_local_auth(profile: &ProviderCredentialProfile) -> bool {
    profile.provider_id == OLLAMA_PROVIDER_ID
        || (profile.provider_id == OPENAI_API_PROVIDER_ID
            && profile
                .base_url
                .as_deref()
                .is_some_and(is_local_openai_compatible_base_url))
}

fn is_local_openai_compatible_base_url(base_url: &str) -> bool {
    Url::parse(base_url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
        .is_some_and(|host| matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1"))
}

fn xai_catalog_bearer_token(
    _profile: &ProviderCredentialProfile,
    provider_profiles: &ProviderCredentialsView,
) -> Option<String> {
    provider_profiles
        .record_for_provider(XAI_PROVIDER_ID)
        .filter(|record| record.kind == ProviderCredentialKind::OAuthSession)
        .and_then(|record| record.oauth_access_token.clone())
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

fn anthropic_family_profile_input(
    profile: &ProviderCredentialProfile,
    provider_profiles: &ProviderCredentialsView,
) -> AnthropicFamilyProfileInput {
    AnthropicFamilyProfileInput {
        provider_id: profile.provider_id.clone(),
        model_id: profile.model_id.clone(),
        updated_at: profile.updated_at.clone(),
        region: profile.region.clone(),
        project_id: profile.project_id.clone(),
        api_key: provider_profiles
            .matched_api_key_credential_for_profile(&profile.profile_id)
            .map(|entry| entry.api_key.clone()),
        api_key_updated_at: provider_profiles
            .matched_api_key_credential_for_profile(&profile.profile_id)
            .map(|entry| entry.updated_at.clone()),
    }
}

fn missing_xai_credential_diagnostic(
    profile: &ProviderCredentialProfile,
) -> ProviderModelCatalogDiagnostic {
    ProviderModelCatalogDiagnostic {
        code: "xai_credential_missing".into(),
        message: format!(
            "Xero cannot discover xAI models for provider `{}` because no xAI API key or sign-in session is configured.",
            profile.provider_id
        ),
        retryable: false,
    }
}

fn missing_cursor_credential_diagnostic(
    profile: &ProviderCredentialProfile,
) -> ProviderModelCatalogDiagnostic {
    ProviderModelCatalogDiagnostic {
        code: "cursor_api_key_missing".into(),
        message: format!(
            "Xero cannot load Cursor models for provider `{}` because no app-local Cursor API key is configured.",
            profile.provider_id
        ),
        retryable: false,
    }
}

fn missing_openrouter_credential_diagnostic(
    profile: &ProviderCredentialProfile,
) -> ProviderModelCatalogDiagnostic {
    ProviderModelCatalogDiagnostic {
        code: "openrouter_api_key_missing".into(),
        message: format!(
            "Xero cannot discover OpenRouter models for provider `{}` because no app-local API key is configured.",
            profile.provider_id
        ),
        retryable: false,
    }
}

fn missing_anthropic_credential_diagnostic(
    profile: &ProviderCredentialProfile,
) -> ProviderModelCatalogDiagnostic {
    ProviderModelCatalogDiagnostic {
        code: "anthropic_api_key_missing".into(),
        message: format!(
            "Xero cannot discover Anthropic models for provider `{}` because no app-local API key is configured.",
            profile.provider_id
        ),
        retryable: false,
    }
}

fn missing_bedrock_ambient_diagnostic(
    profile: &ProviderCredentialProfile,
) -> ProviderModelCatalogDiagnostic {
    ProviderModelCatalogDiagnostic {
        code: "bedrock_ambient_proof_missing".into(),
        message: format!(
            "Xero cannot validate Amazon Bedrock model availability for provider `{}` because its ambient readiness proof link is missing. Save the provider again so Xero records ambient-auth intent.",
            profile.provider_id
        ),
        retryable: false,
    }
}

fn missing_vertex_ambient_diagnostic(
    profile: &ProviderCredentialProfile,
) -> ProviderModelCatalogDiagnostic {
    ProviderModelCatalogDiagnostic {
        code: "vertex_ambient_proof_missing".into(),
        message: format!(
            "Xero cannot validate Google Vertex AI model availability for provider `{}` because its ambient readiness proof link is missing. Save the provider again so Xero records ambient-auth intent.",
            profile.provider_id
        ),
        retryable: false,
    }
}

fn diagnostic_from_auth_error(error: crate::auth::AuthFlowError) -> ProviderModelCatalogDiagnostic {
    ProviderModelCatalogDiagnostic {
        code: error.code,
        message: error.message,
        retryable: error.retryable,
    }
}

fn diagnostic_from_command_error(error: CommandError) -> ProviderModelCatalogDiagnostic {
    ProviderModelCatalogDiagnostic {
        code: error.code,
        message: error.message,
        retryable: error.retryable,
    }
}

fn diagnostic_into_command_error(diagnostic: ProviderModelCatalogDiagnostic) -> CommandError {
    if diagnostic.retryable {
        CommandError::retryable(diagnostic.code, diagnostic.message)
    } else {
        CommandError::user_fixable(diagnostic.code, diagnostic.message)
    }
}

fn normalized_optional_string(value: Option<&str>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::auth::openai_compatible::OpenAiCompatibleDiscoveredThinkingCapability;
    use crate::runtime::XAI_DEFAULT_MODEL_ID;

    #[test]
    fn openai_codex_projection_exposes_gsd_thinking_levels_for_openai_choices() {
        let models = openai_codex_projection();
        let model_ids = models
            .iter()
            .map(|model| model.model_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            model_ids,
            vec![
                "gpt-5.2",
                "gpt-5.3-codex",
                "gpt-5.3-codex-spark",
                "gpt-5.4",
                "gpt-5.5",
            ]
        );

        for model in models {
            assert_eq!(
                model.input_modalities,
                vec!["file", "image", "text"],
                "{} should expose OpenAI GPT attachment input modalities",
                model.model_id
            );
            assert_eq!(
                model.thinking.effort_options,
                vec![
                    ProviderModelThinkingEffort::Minimal,
                    ProviderModelThinkingEffort::Low,
                    ProviderModelThinkingEffort::Medium,
                    ProviderModelThinkingEffort::High,
                    ProviderModelThinkingEffort::XHigh,
                ],
                "{} should expose GSD-style OpenAI Codex thinking levels",
                model.model_id
            );
            assert_eq!(
                model.context_window_tokens,
                Some(272_000),
                "{} should use Codex model-manager context-window metadata",
                model.model_id
            );
        }
    }

    #[test]
    fn openai_codex_thinking_capability_matches_gsd_x_high_patch() {
        assert_eq!(
            openai_codex_thinking_capability("gpt-5.1").effort_options,
            vec![
                ProviderModelThinkingEffort::Minimal,
                ProviderModelThinkingEffort::Low,
                ProviderModelThinkingEffort::Medium,
                ProviderModelThinkingEffort::High,
            ]
        );
        assert_eq!(
            openai_codex_thinking_capability("openai/gpt-5.4").effort_options,
            vec![
                ProviderModelThinkingEffort::Minimal,
                ProviderModelThinkingEffort::Low,
                ProviderModelThinkingEffort::Medium,
                ProviderModelThinkingEffort::High,
                ProviderModelThinkingEffort::XHigh,
            ]
        );
    }

    #[test]
    fn openai_codex_projection_exposes_gpt_5_5_display_name() {
        let models = openai_codex_projection();
        let gpt_5_5 = models
            .iter()
            .find(|model| model.model_id == "gpt-5.5")
            .expect("gpt-5.5 model choice");

        assert_eq!(gpt_5_5.display_name, "GPT-5.5");
        assert_eq!(gpt_5_5.input_modalities, vec!["file", "image", "text"]);
        assert_eq!(
            gpt_5_5.input_modalities_source,
            "openai_static_gpt_multimodal"
        );
        assert_eq!(
            gpt_5_5.thinking.effort_options,
            vec![
                ProviderModelThinkingEffort::Minimal,
                ProviderModelThinkingEffort::Low,
                ProviderModelThinkingEffort::Medium,
                ProviderModelThinkingEffort::High,
                ProviderModelThinkingEffort::XHigh,
            ]
        );
    }

    #[test]
    fn xai_projection_seeds_grok_models_with_reasoning_and_context() {
        let models = xai_projection();
        let grok_4_5 = models
            .iter()
            .find(|model| model.model_id == XAI_DEFAULT_MODEL_ID)
            .expect("grok-4.5 model choice");
        let grok = models
            .iter()
            .find(|model| model.model_id == "grok-4.3")
            .expect("grok-4.3 model choice");
        let version_latest = models
            .iter()
            .find(|model| model.model_id == "grok-4.3-latest")
            .expect("grok-4.3-latest model choice");
        let latest_alias = models
            .iter()
            .find(|model| model.model_id == "grok-latest")
            .expect("grok-latest model choice");
        let build = models
            .iter()
            .find(|model| model.model_id == "grok-build-0.1")
            .expect("grok-build-0.1 model choice");

        assert_eq!(grok_4_5.display_name, "Grok 4.5");
        assert_eq!(grok_4_5.context_window_tokens, Some(500_000));
        assert_eq!(grok_4_5.input_modalities, vec!["image", "text"]);
        assert_eq!(latest_alias.display_name, "Grok Latest");
        assert_eq!(latest_alias.context_window_tokens, Some(1_000_000));
        assert_eq!(latest_alias.input_modalities, vec!["image", "text"]);
        assert_eq!(grok.display_name, "Grok 4.3");
        assert_eq!(grok.context_window_tokens, Some(1_000_000));
        assert_eq!(grok.input_modalities, vec!["image", "text"]);
        assert_eq!(version_latest.input_modalities, vec!["image", "text"]);
        assert_eq!(build.display_name, "Grok Build 0.1");
        assert_eq!(build.context_window_tokens, Some(256_000));
        assert_eq!(build.input_modalities, vec!["image", "text"]);
        assert!(!build.thinking.supported);
        assert!(build.thinking.effort_options.is_empty());
        assert_eq!(
            grok.thinking.effort_options,
            vec![
                ProviderModelThinkingEffort::None,
                ProviderModelThinkingEffort::Low,
                ProviderModelThinkingEffort::Medium,
                ProviderModelThinkingEffort::High,
            ]
        );
        assert_eq!(
            grok.thinking.default_effort,
            Some(ProviderModelThinkingEffort::Low)
        );
    }

    #[test]
    fn xai_cached_models_upgrade_legacy_empty_modalities_for_grok() {
        let models = xai_cached_models(&[ProviderModelRecord {
            model_id: "grok-4.3".into(),
            display_name: "Grok 4.3".into(),
            thinking: unsupported_thinking_capability(),
            input_modalities: Vec::new(),
            input_modalities_source: "unknown".into(),
            context_window_tokens: None,
            max_output_tokens: None,
            context_limit_source: None,
            context_limit_confidence: None,
            context_limit_fetched_at: None,
        }]);
        let grok = models
            .iter()
            .find(|model| model.model_id == "grok-4.3")
            .expect("cached grok model");
        let grok_4_5 = models
            .iter()
            .find(|model| model.model_id == XAI_DEFAULT_MODEL_ID)
            .expect("projected grok 4.5 model");
        let latest = models
            .iter()
            .find(|model| model.model_id == "grok-4.3-latest")
            .expect("projected latest grok model");
        let build = models
            .iter()
            .find(|model| model.model_id == "grok-build-0.1")
            .expect("projected grok build model");

        assert_eq!(grok.input_modalities, vec!["image", "text"]);
        assert_eq!(grok.input_modalities_source, "xai_text_runtime_default");
        assert_eq!(grok_4_5.context_window_tokens, Some(500_000));
        assert_eq!(grok_4_5.input_modalities, vec!["image", "text"]);
        assert_eq!(latest.input_modalities, vec!["image", "text"]);
        assert_eq!(build.context_window_tokens, Some(256_000));
    }

    #[test]
    fn openai_compatible_models_seed_gpt_attachment_modalities_when_omitted() {
        let models = normalize_openai_compatible_models(
            OPENAI_API_PROVIDER_ID,
            vec![
                OpenAiCompatibleDiscoveredModel {
                    id: "gpt-5.5".into(),
                    display_name: "GPT-5.5".into(),
                    thinking: OpenAiCompatibleDiscoveredThinkingCapability {
                        supported: false,
                        effort_levels: Vec::new(),
                        default_effort: None,
                    },
                    input_modalities: Vec::new(),
                    input_modalities_source: "unknown".into(),
                    context_window_tokens: None,
                    max_output_tokens: None,
                },
                OpenAiCompatibleDiscoveredModel {
                    id: "gpt-audio".into(),
                    display_name: "GPT Audio".into(),
                    thinking: OpenAiCompatibleDiscoveredThinkingCapability {
                        supported: false,
                        effort_levels: Vec::new(),
                        default_effort: None,
                    },
                    input_modalities: Vec::new(),
                    input_modalities_source: "unknown".into(),
                    context_window_tokens: None,
                    max_output_tokens: None,
                },
            ],
        );

        let gpt_5_5 = models
            .iter()
            .find(|model| model.model_id == "gpt-5.5")
            .expect("gpt-5.5 model");
        let gpt_audio = models
            .iter()
            .find(|model| model.model_id == "gpt-audio")
            .expect("gpt-audio model");

        assert_eq!(gpt_5_5.input_modalities, vec!["file", "image", "text"]);
        assert_eq!(
            gpt_5_5.input_modalities_source,
            "openai_static_gpt_multimodal"
        );
        assert!(gpt_audio.input_modalities.is_empty());
        assert_eq!(gpt_audio.input_modalities_source, "unknown");
    }

    #[test]
    fn openai_cached_models_upgrade_legacy_empty_modalities_for_gpt() {
        let models = openai_gpt_cached_models(
            OPENAI_API_PROVIDER_ID,
            &[ProviderModelRecord {
                model_id: "openai/gpt-5.4".into(),
                display_name: "GPT-5.4".into(),
                thinking: unsupported_thinking_capability(),
                input_modalities: Vec::new(),
                input_modalities_source: "unknown".into(),
                context_window_tokens: None,
                max_output_tokens: None,
                context_limit_source: None,
                context_limit_confidence: None,
                context_limit_fetched_at: None,
            }],
        );
        let gpt = models
            .iter()
            .find(|model| model.model_id == "openai/gpt-5.4")
            .expect("cached gpt model");

        assert_eq!(gpt.input_modalities, vec!["file", "image", "text"]);
        assert_eq!(gpt.input_modalities_source, "openai_static_gpt_multimodal");
    }

    #[test]
    fn cursor_projection_exposes_auto_and_composer_latest_without_owned_model_capabilities() {
        let models = cursor_projection();
        assert_eq!(models.len(), 2);
        let auto = &models[0];
        let composer = &models[1];

        assert_eq!(auto.model_id, CURSOR_AUTO_MODEL_ID);
        assert_eq!(auto.display_name, "Auto");
        assert!(!auto.thinking.supported);
        assert!(auto.thinking.effort_options.is_empty());
        assert_eq!(composer.model_id, CURSOR_DEFAULT_MODEL_ID);
        assert_eq!(composer.display_name, "Composer Latest");
        assert!(!composer.thinking.supported);
        assert!(composer.thinking.effort_options.is_empty());
    }

    #[test]
    fn cursor_catalog_normalization_keeps_auto_and_composer_first() {
        let models = normalize_cursor_models(vec![
            CursorDiscoveredModel {
                id: "router".into(),
                display_name: "Cursor Router".into(),
                aliases: vec!["auto".into(), "default".into(), "claude-4-sonnet".into()],
            },
            CursorDiscoveredModel {
                id: "composer-latest".into(),
                display_name: "Composer Latest".into(),
                aliases: vec![],
            },
            CursorDiscoveredModel {
                id: "gpt-5.4".into(),
                display_name: "GPT-5.4".into(),
                aliases: vec!["cursor_auto".into()],
            },
        ]);
        let model_ids = models
            .iter()
            .map(|model| model.model_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            &model_ids[0..2],
            &[CURSOR_AUTO_MODEL_ID, CURSOR_DEFAULT_MODEL_ID]
        );
        assert!(model_ids.contains(&"claude-4-sonnet"));
        assert!(model_ids.contains(&"gpt-5.4"));
        assert!(model_ids.contains(&"router"));
        assert_eq!(models[0].display_name, "Auto");
        assert_eq!(models[1].display_name, "Composer Latest");
    }

    #[test]
    fn cursor_bridge_model_list_event_accepts_sdk_catalog_metadata() {
        let event = cursor_bridge_model_list_event(
            r#"{"type":"completed","bridgeVersion":"xero-cursor-sdk-bridge.v1","modelCount":1,"catalogSource":"cursor.models.list","autoAliases":["auto"],"models":[{"id":"router","displayName":"Router","description":"Account router","aliases":["auto"],"parameters":[],"variants":[]}]}"#,
        )
        .expect("bridge model-list event");

        assert_eq!(event.event_type, "completed");
        assert_eq!(event.models.len(), 1);
        assert_eq!(event.models[0].id, "router");
        assert_eq!(event.models[0].aliases, vec!["auto"]);
    }

    #[test]
    fn xai_catalog_only_exposes_supported_grok_text_models() {
        let models = normalize_xai_models(vec![
            XaiModelEntry {
                id: "grok-4.20-0309-non-reasoning".into(),
                aliases: Vec::new(),
                input_modalities: Vec::new(),
            },
            XaiModelEntry {
                id: "grok-4.20-0309-reasoning".into(),
                aliases: Vec::new(),
                input_modalities: Vec::new(),
            },
            XaiModelEntry {
                id: "grok-4.20-multi-agent-0309".into(),
                aliases: Vec::new(),
                input_modalities: Vec::new(),
            },
            XaiModelEntry {
                id: "grok-imagine-image-quality".into(),
                aliases: Vec::new(),
                input_modalities: Vec::new(),
            },
            XaiModelEntry {
                id: "grok-imagine-video".into(),
                aliases: Vec::new(),
                input_modalities: Vec::new(),
            },
            XaiModelEntry {
                id: "grok-4.5".into(),
                aliases: Vec::new(),
                input_modalities: vec!["text".into(), "image".into()],
            },
            XaiModelEntry {
                id: "grok-latest".into(),
                aliases: vec!["grok-4.3".into()],
                input_modalities: vec!["text".into(), "image".into()],
            },
            XaiModelEntry {
                id: "grok-4.3-latest".into(),
                aliases: Vec::new(),
                input_modalities: vec!["text".into(), "image".into()],
            },
            XaiModelEntry {
                id: "grok-build-0.1".into(),
                aliases: vec!["grok-code-fast".into()],
                input_modalities: vec!["text".into(), "image".into()],
            },
        ]);

        let model_ids = models
            .iter()
            .map(|model| model.model_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            model_ids,
            vec![
                "grok-4.3",
                "grok-4.3-latest",
                "grok-4.5",
                "grok-build-0.1",
                "grok-latest"
            ]
        );
        assert_eq!(models[0].input_modalities, vec!["image", "text"]);
        assert_eq!(models[1].input_modalities, vec!["image", "text"]);
        assert_eq!(models[2].display_name, "Grok 4.5");
        assert_eq!(models[2].context_window_tokens, Some(500_000));
        assert_eq!(models[3].display_name, "Grok Build 0.1");
        assert_eq!(models[3].context_window_tokens, Some(256_000));
        assert!(!models[3].thinking.supported);
        assert!(models[3].thinking.effort_options.is_empty());
        assert_eq!(
            models[4].thinking.effort_options,
            vec![
                ProviderModelThinkingEffort::None,
                ProviderModelThinkingEffort::Low,
                ProviderModelThinkingEffort::Medium,
                ProviderModelThinkingEffort::High,
            ]
        );
    }
}
