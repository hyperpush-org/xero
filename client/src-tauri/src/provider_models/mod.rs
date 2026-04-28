use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use url::Url;

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
    commands::{provider_credentials::load_provider_credentials_view, CommandError, CommandResult},
    provider_credentials::{
        ProviderCredentialProfile, ProviderCredentialReadinessStatus, ProviderCredentialsView,
    },
    runtime::{
        ANTHROPIC_PROVIDER_ID, AZURE_OPENAI_PROVIDER_ID, BEDROCK_PROVIDER_ID,
        GEMINI_AI_STUDIO_PROVIDER_ID, GITHUB_MODELS_PROVIDER_ID, OLLAMA_PROVIDER_ID,
        OPENAI_API_PROVIDER_ID, OPENAI_CODEX_PROVIDER_ID, OPENAI_CODEX_SUPPORTED_MODEL_IDS,
        OPENROUTER_PROVIDER_ID, VERTEX_PROVIDER_ID,
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
        .cloned()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "provider_profile_not_found",
                format!("Cadence could not find provider profile `{profile_id}`."),
            )
        })?;

    let cache_path = state.provider_model_catalog_cache_file(app)?;
    let cache_load = load_provider_model_catalog_cache(&cache_path);
    let refresh_target = resolve_provider_model_catalog_refresh_target(&profile, state)
        .map_err(diagnostic_into_command_error)?;
    let expected_scope = refresh_target.cache_scope(&profile);
    let cached_row = cache_load.requested_cache_row(&profile, &expected_scope);
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

    let cache_read_diagnostic = cache_load.requested_diagnostic(&cache_key);
    let cache_catalogs = cache_load.catalogs.clone();
    let cache_write_allowed = cache_load.write_allowed;
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
        ProviderModelCatalogRefreshTarget::OpenRouter => {
            let Some(secret) = provider_profiles.openrouter_credential(&profile.profile_id) else {
                let diagnostic = missing_openrouter_credential_diagnostic(profile);
                return match refresh_context.cached_row.as_ref() {
                    Some(cached) => catalog_from_cached_row(profile, cached, Some(diagnostic)),
                    None => {
                        unavailable_or_manual_catalog(profile, refresh_target, Some(diagnostic))
                    }
                };
            };

            fetch_openrouter_models(&secret.api_key, &state.openrouter_auth_config())
                .map(normalize_openrouter_models)
                .map_err(diagnostic_from_auth_error)
        }
        ProviderModelCatalogRefreshTarget::Anthropic => {
            let profile_input = anthropic_family_profile_input(profile, provider_profiles);
            discovered_anthropic_family_models(&profile_input, &state.anthropic_auth_config())
                .map(normalize_anthropic_models)
                .map_err(diagnostic_from_auth_error)
        }
        ProviderModelCatalogRefreshTarget::AnthropicAmbient => {
            let profile_input = anthropic_family_profile_input(profile, provider_profiles);
            discovered_anthropic_family_models(&profile_input, &state.anthropic_auth_config())
                .map(normalize_anthropic_models)
                .map_err(diagnostic_from_auth_error)
        }
        ProviderModelCatalogRefreshTarget::OpenAiCompatible(endpoint) => {
            let readiness = profile.readiness();
            if matches!(
                readiness.status,
                ProviderCredentialReadinessStatus::Malformed
            ) {
                let diagnostic = ProviderModelCatalogDiagnostic {
                    code: "provider_profile_credentials_unavailable".into(),
                    message: format!(
                        "Cadence cannot discover models for provider profile `{}` because the redacted credential metadata no longer matches the saved app-local secret state.",
                        profile.profile_id
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
                .map(normalize_openai_compatible_models)
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

fn openai_codex_projection() -> Vec<ProviderModelRecord> {
    OPENAI_CODEX_SUPPORTED_MODEL_IDS
        .iter()
        .map(|model_id| ProviderModelRecord {
            model_id: (*model_id).into(),
            display_name: match *model_id {
                "gpt-5.2" => "GPT-5.2",
                "gpt-5.3-codex" => "GPT-5.3 Codex",
                "gpt-5.3-codex-spark" => "GPT-5.3 Codex Spark",
                "gpt-5.4" => "GPT-5.4",
                other => other,
            }
            .into(),
            thinking: supported_thinking_capability(vec![
                ProviderModelThinkingEffort::Low,
                ProviderModelThinkingEffort::Medium,
                ProviderModelThinkingEffort::High,
            ]),
        })
        .collect()
}

fn normalize_openrouter_models(models: Vec<OpenRouterDiscoveredModel>) -> Vec<ProviderModelRecord> {
    let mut normalized = models
        .into_iter()
        .map(|model| ProviderModelRecord {
            model_id: model.id,
            display_name: model.display_name,
            thinking: openrouter_thinking_capability(&model.supported_parameters),
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

fn normalize_anthropic_models(models: Vec<AnthropicDiscoveredModel>) -> Vec<ProviderModelRecord> {
    let mut normalized = models
        .into_iter()
        .map(|model| {
            let thinking = anthropic_thinking_capability(&model);
            ProviderModelRecord {
                model_id: model.id,
                display_name: model.display_name,
                thinking,
            }
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
    models: Vec<OpenAiCompatibleDiscoveredModel>,
) -> Vec<ProviderModelRecord> {
    let mut normalized = models
        .into_iter()
        .map(|model| {
            let thinking = openai_compatible_thinking_capability(&model);
            ProviderModelRecord {
                model_id: model.id,
                display_name: model.display_name,
                thinking,
            }
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
        BEDROCK_PROVIDER_ID | VERTEX_PROVIDER_ID => manual_anthropic_family_projection(profile),
        _ => manual_openai_compatible_projection(profile),
    }
}

fn manual_openai_compatible_projection(
    profile: &ProviderCredentialProfile,
) -> Vec<ProviderModelRecord> {
    vec![ProviderModelRecord {
        model_id: profile.model_id.clone(),
        display_name: profile.model_id.clone(),
        thinking: unsupported_thinking_capability(),
    }]
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

    vec![ProviderModelRecord {
        model_id: profile.model_id.clone(),
        display_name: profile.model_id.clone(),
        thinking,
    }]
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
    ProviderModelThinkingCapability {
        supported: true,
        default_effort: effort_options
            .iter()
            .copied()
            .find(|effort| *effort == ProviderModelThinkingEffort::Medium)
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
    ProviderModelCatalog {
        profile_id: profile.profile_id.clone(),
        provider_id: profile.provider_id.clone(),
        configured_model_id: profile.model_id.clone(),
        source: ProviderModelCatalogSource::Cache,
        fetched_at: Some(cached.fetched_at.clone()),
        last_success_at: Some(cached.last_success_at.clone()),
        last_refresh_error: diagnostic,
        models: cached.models.clone(),
    }
}

fn unavailable_or_manual_catalog(
    profile: &ProviderCredentialProfile,
    refresh_target: &ProviderModelCatalogRefreshTarget,
    diagnostic: Option<ProviderModelCatalogDiagnostic>,
) -> ProviderModelCatalog {
    match refresh_target {
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
        OPENROUTER_PROVIDER_ID => Ok(ProviderModelCatalogRefreshTarget::OpenRouter),
        ANTHROPIC_PROVIDER_ID => Ok(ProviderModelCatalogRefreshTarget::Anthropic),
        BEDROCK_PROVIDER_ID | VERTEX_PROVIDER_ID => {
            Ok(ProviderModelCatalogRefreshTarget::AnthropicAmbient)
        }
        OPENAI_API_PROVIDER_ID
        | OLLAMA_PROVIDER_ID
        | AZURE_OPENAI_PROVIDER_ID
        | GITHUB_MODELS_PROVIDER_ID
        | GEMINI_AI_STUDIO_PROVIDER_ID => {
            resolve_openai_compatible_endpoint_for_profile(
                profile,
                &state.openai_compatible_auth_config(),
            )
            .map(ProviderModelCatalogRefreshTarget::OpenAiCompatible)
            .map_err(diagnostic_from_auth_error)
        }
        other => Err(ProviderModelCatalogDiagnostic {
            code: "provider_model_provider_unsupported".into(),
            message: format!(
                "Cadence cannot discover models for provider `{other}` because that provider is not supported by the desktop host yet."
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
            format!(
                "Cadence could not serialize the app-local provider-model catalog cache: {error}"
            ),
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
                format!("Cadence could not write provider model catalog cache row: {error}"),
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
                    "Cadence could not open the global database for the provider-model catalog cache at {}: {}",
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
                    "Cadence could not prepare provider-model catalog cache read: {error}"
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
                message: format!(
                    "Cadence could not read provider-model catalog cache rows: {error}"
                ),
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
                                    "Cadence could not decode the cached provider-model catalog row for profile `{profile_id}`: {error}"
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
                        "Cadence could not decode provider-model catalog cache row: {error}"
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
                "Cadence rejected the cached provider-model catalog row for profile `{profile_id}` at {} because providerId was blank.",
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
                    "Cadence rejected the cached provider-model catalog row for profile `{profile_id}` at {} because one cache-scope field was blank.",
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
                    "Cadence rejected the cached provider-model catalog row for profile `{profile_id}` at {} because one modelId was blank.",
                    path.display()
                ),
                retryable: false,
            });
        }

        if model.display_name.trim().is_empty() {
            return Err(ProviderModelCatalogDiagnostic {
                code: "provider_model_catalog_cache_invalid".into(),
                message: format!(
                    "Cadence rejected the cached provider-model catalog row for profile `{profile_id}` at {} because one displayName was blank.",
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
            | OPENAI_API_PROVIDER_ID
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
            ANTHROPIC_PROVIDER_ID => missing_anthropic_credential_diagnostic(profile),
            BEDROCK_PROVIDER_ID => missing_bedrock_ambient_diagnostic(profile),
            VERTEX_PROVIDER_ID => missing_vertex_ambient_diagnostic(profile),
            OPENAI_API_PROVIDER_ID
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
                code: "provider_profile_credentials_unavailable".into(),
                message: format!(
                    "Cadence cannot discover OpenRouter models for provider profile `{}` because the redacted credential metadata no longer matches the saved app-local secret state.",
                    profile.profile_id
                ),
                retryable: false,
            },
            ANTHROPIC_PROVIDER_ID => ProviderModelCatalogDiagnostic {
                code: "provider_profile_credentials_unavailable".into(),
                message: format!(
                    "Cadence cannot discover Anthropic models for provider profile `{}` because the redacted credential metadata no longer matches the saved app-local secret state.",
                    profile.profile_id
                ),
                retryable: false,
            },
            OPENAI_API_PROVIDER_ID
            | OLLAMA_PROVIDER_ID
            | AZURE_OPENAI_PROVIDER_ID
            | GITHUB_MODELS_PROVIDER_ID
            | GEMINI_AI_STUDIO_PROVIDER_ID => {
                ProviderModelCatalogDiagnostic {
                    code: "provider_profile_credentials_unavailable".into(),
                    message: format!(
                        "Cadence cannot discover models for provider profile `{}` because the redacted credential metadata no longer matches the saved app-local secret state.",
                        profile.profile_id
                    ),
                    retryable: false,
                }
            }
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
            .anthropic_credential(&profile.profile_id)
            .map(|entry| entry.api_key.clone()),
        api_key_updated_at: provider_profiles
            .anthropic_credential(&profile.profile_id)
            .map(|entry| entry.updated_at.clone()),
    }
}

fn missing_openrouter_credential_diagnostic(
    profile: &ProviderCredentialProfile,
) -> ProviderModelCatalogDiagnostic {
    ProviderModelCatalogDiagnostic {
        code: "openrouter_api_key_missing".into(),
        message: format!(
            "Cadence cannot discover OpenRouter models for provider profile `{}` because no app-local API key is configured for that profile.",
            profile.profile_id
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
            "Cadence cannot discover Anthropic models for provider profile `{}` because no app-local API key is configured for that profile.",
            profile.profile_id
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
            "Cadence cannot validate Amazon Bedrock model availability for provider profile `{}` because the profile is missing its ambient readiness proof link. Save the profile again so Cadence records ambient-auth intent.",
            profile.profile_id
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
            "Cadence cannot validate Google Vertex AI model availability for provider profile `{}` because the profile is missing its ambient readiness proof link. Save the profile again so Cadence records ambient-auth intent.",
            profile.profile_id
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
