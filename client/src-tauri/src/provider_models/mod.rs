use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Arc, Condvar, Mutex},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};

use crate::{
    auth::openrouter::{fetch_openrouter_models, OpenRouterDiscoveredModel},
    commands::{
        get_runtime_settings::write_json_file_atomically,
        provider_profiles::load_provider_profiles_snapshot, CommandError, CommandResult,
    },
    provider_profiles::{
        ProviderProfileReadinessStatus, ProviderProfileRecord, ProviderProfilesSnapshot,
    },
    runtime::{OPENAI_CODEX_PROVIDER_ID, OPENROUTER_PROVIDER_ID},
    state::DesktopState,
};

pub const PROVIDER_MODEL_CATALOG_CACHE_FILE_NAME: &str = "provider-model-catalogs.json";
const PROVIDER_MODEL_CATALOG_CACHE_SCHEMA_VERSION: u32 = 1;
const OPENAI_CODEX_MODEL_DISPLAY_NAME: &str = "OpenAI Codex";
const PROVIDER_MODEL_CACHE_OPERATION: &str = "provider_model_catalog_cache";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderModelCatalogSource {
    Live,
    Cache,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachedProviderModelCatalogRow {
    provider_id: String,
    fetched_at: String,
    last_success_at: String,
    models: Vec<ProviderModelRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderModelCatalogCacheFile {
    #[serde(default = "provider_model_catalog_cache_schema_version")]
    version: u32,
    #[serde(default)]
    catalogs: BTreeMap<String, CachedProviderModelCatalogRow>,
}

#[derive(Debug, Clone, Default)]
struct ProviderModelCatalogCacheLoad {
    catalogs: BTreeMap<String, CachedProviderModelCatalogRow>,
    write_allowed: bool,
    file_error: Option<ProviderModelCatalogDiagnostic>,
    row_errors: BTreeMap<String, ProviderModelCatalogDiagnostic>,
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
        profile: &ProviderProfileRecord,
    ) -> Option<CachedProviderModelCatalogRow> {
        self.catalogs
            .get(&profile.profile_id)
            .filter(|row| row.provider_id == profile.provider_id)
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

    let provider_profiles = load_provider_profiles_snapshot(app, state)?;
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
    let cached_row = cache_load.requested_cache_row(&profile);
    let profile_diagnostic = readiness_diagnostic(&profile, &provider_profiles);

    if let Some(diagnostic) = profile_diagnostic.clone() {
        return Ok(match cached_row.as_ref() {
            Some(cached) => catalog_from_cached_row(&profile, cached, Some(diagnostic)),
            None => unavailable_catalog(&profile, Some(diagnostic)),
        });
    }

    if !force_refresh {
        if let Some(cached) = cached_row.as_ref() {
            return Ok(catalog_from_cached_row(&profile, cached, None));
        }
    }

    let cache_read_diagnostic = cache_load.requested_diagnostic(profile_id);
    let cache_catalogs = cache_load.catalogs.clone();
    let cache_write_allowed = cache_load.write_allowed;
    let provider_profiles = provider_profiles.clone();
    let profile = profile.clone();
    let cache_path = cache_path.clone();

    Ok(state
        .provider_model_catalog_refresh_registry()
        .run(profile_id, move || {
            refresh_provider_model_catalog(
                &profile,
                &provider_profiles,
                state,
                cached_row.as_ref(),
                &cache_catalogs,
                &cache_path,
                cache_write_allowed,
                cache_read_diagnostic,
            )
        }))
}

fn refresh_provider_model_catalog(
    profile: &ProviderProfileRecord,
    provider_profiles: &ProviderProfilesSnapshot,
    state: &DesktopState,
    cached_row: Option<&CachedProviderModelCatalogRow>,
    cache_catalogs: &BTreeMap<String, CachedProviderModelCatalogRow>,
    cache_path: &Path,
    cache_write_allowed: bool,
    cache_read_diagnostic: Option<ProviderModelCatalogDiagnostic>,
) -> ProviderModelCatalog {
    let live_models = match profile.provider_id.as_str() {
        OPENAI_CODEX_PROVIDER_ID => Ok(openai_codex_projection()),
        OPENROUTER_PROVIDER_ID => {
            let Some(secret) = provider_profiles.openrouter_credential(&profile.profile_id) else {
                let diagnostic = missing_openrouter_credential_diagnostic(profile);
                return match cached_row {
                    Some(cached) => catalog_from_cached_row(profile, cached, Some(diagnostic)),
                    None => unavailable_catalog(profile, Some(diagnostic)),
                };
            };

            fetch_openrouter_models(&secret.api_key, &state.openrouter_auth_config())
                .map(normalize_openrouter_models)
                .map_err(diagnostic_from_auth_error)
        }
        other => Err(ProviderModelCatalogDiagnostic {
            code: "provider_model_provider_unsupported".into(),
            message: format!(
                "Cadence cannot discover models for provider `{other}` because that provider is not supported by the desktop host yet."
            ),
            retryable: false,
        }),
    };

    match live_models {
        Ok(models) => {
            let now = crate::auth::now_timestamp();
            let new_row = CachedProviderModelCatalogRow {
                provider_id: profile.provider_id.clone(),
                fetched_at: now.clone(),
                last_success_at: now.clone(),
                models: models.clone(),
            };

            let mut catalog = ProviderModelCatalog {
                profile_id: profile.profile_id.clone(),
                provider_id: profile.provider_id.clone(),
                configured_model_id: profile.model_id.clone(),
                source: ProviderModelCatalogSource::Live,
                fetched_at: Some(now.clone()),
                last_success_at: Some(now),
                last_refresh_error: cache_read_diagnostic.clone(),
                models,
            };

            if !cache_write_allowed {
                return catalog;
            }

            if cached_row
                .map(|cached| materially_changed(cached, &new_row))
                .unwrap_or(true)
            {
                if let Err(error) = persist_provider_model_catalog_cache(
                    cache_path,
                    cache_catalogs,
                    &profile.profile_id,
                    &new_row,
                ) {
                    catalog.last_refresh_error = Some(diagnostic_from_command_error(error));
                }
            }

            catalog
        }
        Err(diagnostic) => match cached_row {
            Some(cached) => catalog_from_cached_row(profile, cached, Some(diagnostic)),
            None => unavailable_catalog(profile, Some(diagnostic)),
        },
    }
}

fn openai_codex_projection() -> Vec<ProviderModelRecord> {
    vec![ProviderModelRecord {
        model_id: OPENAI_CODEX_PROVIDER_ID.into(),
        display_name: OPENAI_CODEX_MODEL_DISPLAY_NAME.into(),
        thinking: supported_thinking_capability(vec![
            ProviderModelThinkingEffort::Low,
            ProviderModelThinkingEffort::Medium,
            ProviderModelThinkingEffort::High,
        ]),
    }]
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
    profile: &ProviderProfileRecord,
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

fn unavailable_catalog(
    profile: &ProviderProfileRecord,
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

fn materially_changed(
    current: &CachedProviderModelCatalogRow,
    next: &CachedProviderModelCatalogRow,
) -> bool {
    current.provider_id != next.provider_id || current.models != next.models
}

fn persist_provider_model_catalog_cache(
    path: &Path,
    current: &BTreeMap<String, CachedProviderModelCatalogRow>,
    profile_id: &str,
    next: &CachedProviderModelCatalogRow,
) -> CommandResult<()> {
    let mut catalogs = current.clone();
    catalogs.insert(profile_id.to_owned(), next.clone());

    let payload = ProviderModelCatalogCacheFile {
        version: PROVIDER_MODEL_CATALOG_CACHE_SCHEMA_VERSION,
        catalogs,
    };
    let json = serde_json::to_vec_pretty(&payload).map_err(|error| {
        CommandError::system_fault(
            "provider_model_catalog_cache_serialize_failed",
            format!(
                "Cadence could not serialize the app-local provider-model catalog cache: {error}"
            ),
        )
    })?;

    write_json_file_atomically(path, &json, PROVIDER_MODEL_CACHE_OPERATION)
}

fn load_provider_model_catalog_cache(path: &Path) -> ProviderModelCatalogCacheLoad {
    let mut load = ProviderModelCatalogCacheLoad {
        write_allowed: true,
        ..ProviderModelCatalogCacheLoad::default()
    };

    if !path.exists() {
        return load;
    }

    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => {
            load.write_allowed = false;
            load.file_error = Some(ProviderModelCatalogDiagnostic {
                code: "provider_model_catalog_cache_read_failed".into(),
                message: format!(
                    "Cadence could not read the app-local provider-model catalog cache at {}: {error}",
                    path.display()
                ),
                retryable: true,
            });
            return load;
        }
    };

    let parsed = match serde_json::from_str::<serde_json::Value>(&contents) {
        Ok(parsed) => parsed,
        Err(error) => {
            load.write_allowed = false;
            load.file_error = Some(ProviderModelCatalogDiagnostic {
                code: "provider_model_catalog_cache_decode_failed".into(),
                message: format!(
                    "Cadence could not decode the app-local provider-model catalog cache at {}: {error}",
                    path.display()
                ),
                retryable: false,
            });
            return load;
        }
    };

    let Some(root) = parsed.as_object() else {
        load.write_allowed = false;
        load.file_error = Some(ProviderModelCatalogDiagnostic {
            code: "provider_model_catalog_cache_invalid".into(),
            message: format!(
                "Cadence rejected the app-local provider-model catalog cache at {} because the top-level value was not an object.",
                path.display()
            ),
            retryable: false,
        });
        return load;
    };

    let version = root.get("version").and_then(serde_json::Value::as_u64);
    if version != Some(PROVIDER_MODEL_CATALOG_CACHE_SCHEMA_VERSION as u64) {
        load.write_allowed = false;
        load.file_error = Some(ProviderModelCatalogDiagnostic {
            code: "provider_model_catalog_cache_invalid".into(),
            message: format!(
                "Cadence rejected the app-local provider-model catalog cache at {} because schema version {:?} is unsupported.",
                path.display(),
                version
            ),
            retryable: false,
        });
        return load;
    }

    let Some(catalogs) = root.get("catalogs").and_then(serde_json::Value::as_object) else {
        load.write_allowed = false;
        load.file_error = Some(ProviderModelCatalogDiagnostic {
            code: "provider_model_catalog_cache_invalid".into(),
            message: format!(
                "Cadence rejected the app-local provider-model catalog cache at {} because `catalogs` was missing or not an object.",
                path.display()
            ),
            retryable: false,
        });
        return load;
    };

    for (profile_id, row) in catalogs {
        match serde_json::from_value::<CachedProviderModelCatalogRow>(row.clone()) {
            Ok(row) => {
                if let Err(error) = validate_cached_catalog_row(path, profile_id, &row) {
                    load.write_allowed = false;
                    load.row_errors.insert(profile_id.clone(), error);
                } else {
                    load.catalogs.insert(profile_id.clone(), row);
                }
            }
            Err(error) => {
                load.write_allowed = false;
                load.row_errors.insert(
                    profile_id.clone(),
                    ProviderModelCatalogDiagnostic {
                        code: "provider_model_catalog_cache_decode_failed".into(),
                        message: format!(
                            "Cadence could not decode the cached provider-model catalog row for profile `{profile_id}` at {}: {error}",
                            path.display()
                        ),
                        retryable: false,
                    },
                );
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
    profile: &ProviderProfileRecord,
    provider_profiles: &ProviderProfilesSnapshot,
) -> Option<ProviderModelCatalogDiagnostic> {
    if profile.provider_id != OPENROUTER_PROVIDER_ID {
        return None;
    }

    let readiness = profile.readiness(&provider_profiles.credentials);
    match readiness.status {
        ProviderProfileReadinessStatus::Ready => None,
        ProviderProfileReadinessStatus::Missing => Some(missing_openrouter_credential_diagnostic(profile)),
        ProviderProfileReadinessStatus::Malformed => Some(ProviderModelCatalogDiagnostic {
            code: "provider_profile_credentials_unavailable".into(),
            message: format!(
                "Cadence cannot discover OpenRouter models for provider profile `{}` because the redacted credential metadata no longer matches the saved app-local secret state.",
                profile.profile_id
            ),
            retryable: false,
        }),
    }
}

fn missing_openrouter_credential_diagnostic(
    profile: &ProviderProfileRecord,
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

const fn provider_model_catalog_cache_schema_version() -> u32 {
    PROVIDER_MODEL_CATALOG_CACHE_SCHEMA_VERSION
}
