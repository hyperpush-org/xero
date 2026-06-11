use std::{collections::BTreeMap, path::Path, time::Instant};

use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use url::Url;

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    global_db::open_global_database,
    provider_credentials::{
        delete_provider_credential, load_provider_credential, upsert_provider_credential,
        web_search_credential_provider_id, ProviderCredentialKind, ProviderCredentialRecord,
    },
    runtime::{
        is_supported_xai_text_model_id, AgentProviderConfig, AnthropicProviderConfig,
        AutonomousWebConfig, AutonomousWebManagedSearchConfig, AutonomousWebManagedSearchKind,
        AutonomousWebRuntime, AutonomousWebRuntimeLimits, AutonomousWebSearchMode,
        AutonomousWebSearchProviderConfig, AutonomousWebSearchProviderKind,
        AutonomousWebSearchRequest, ANTHROPIC_PROVIDER_ID, AZURE_OPENAI_PROVIDER_ID,
        GEMINI_AI_STUDIO_PROVIDER_ID, OPENAI_API_PROVIDER_ID, OPENROUTER_PROVIDER_ID,
        VERTEX_PROVIDER_ID, XAI_PROVIDER_ID,
    },
    state::DesktopState,
};

const AUTONOMOUS_WEB_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWebSearchSettingsDto {
    pub mode: AutonomousWebSearchMode,
    pub active_provider_id: Option<String>,
    pub providers: Vec<AutonomousWebSearchProviderProfileDto>,
    pub provider_kinds: Vec<AutonomousWebSearchProviderKindMetadataDto>,
    pub provider_managed: AutonomousWebProviderManagedStatusDto,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWebSearchProviderProfileDto {
    pub profile_id: String,
    pub kind: AutonomousWebSearchProviderKind,
    pub display_name: String,
    pub enabled: bool,
    pub endpoint: Option<String>,
    pub base_url: Option<String>,
    pub google_cse_cx: Option<String>,
    pub result_limit: Option<usize>,
    pub timeout_ms: Option<u64>,
    pub region: Option<String>,
    pub language: Option<String>,
    pub freshness: Option<String>,
    pub safe_search: Option<bool>,
    pub has_api_key: bool,
    pub api_key_updated_at: Option<String>,
    pub readiness: AutonomousWebSearchProviderReadinessDto,
    pub last_check: Option<AutonomousWebSearchProviderCheckDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWebSearchProviderReadinessDto {
    pub ready: bool,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWebSearchProviderKindMetadataDto {
    pub kind: AutonomousWebSearchProviderKind,
    pub label: String,
    pub requires_api_key: bool,
    pub supports_locale: bool,
    pub supports_freshness: bool,
    pub supports_safe_search: bool,
    pub self_hosted: bool,
    pub requires_endpoint: bool,
    pub requires_google_cse_cx: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWebProviderManagedStatusDto {
    pub mode_available: bool,
    pub status: String,
    pub message: String,
    pub supported_sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertAutonomousWebSearchSettingsRequestDto {
    pub mode: AutonomousWebSearchMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertAutonomousWebSearchProviderRequestDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub kind: AutonomousWebSearchProviderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_api_key: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub google_cse_cx: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freshness: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_search: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteAutonomousWebSearchProviderRequestDto {
    pub provider_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetActiveAutonomousWebSearchProviderRequestDto {
    pub provider_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckAutonomousWebSearchProviderRequestDto {
    pub provider_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWebSearchProviderCheckDto {
    pub status: String,
    pub code: String,
    pub message: String,
    pub latency_ms: u64,
    pub sample_result_count: usize,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AutonomousWebSettingsFile {
    schema_version: u32,
    mode: AutonomousWebSearchMode,
    active_provider_id: Option<String>,
    #[serde(default)]
    providers: Vec<AutonomousWebProviderProfileFile>,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AutonomousWebProviderProfileFile {
    profile_id: String,
    kind: AutonomousWebSearchProviderKind,
    display_name: String,
    enabled: bool,
    endpoint: Option<String>,
    base_url: Option<String>,
    google_cse_cx: Option<String>,
    result_limit: Option<usize>,
    timeout_ms: Option<u64>,
    region: Option<String>,
    language: Option<String>,
    freshness: Option<String>,
    safe_search: Option<bool>,
    last_check: Option<AutonomousWebSearchProviderCheckDto>,
    created_at: String,
    updated_at: String,
}

#[tauri::command]
pub fn autonomous_web_search_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<AutonomousWebSearchSettingsDto> {
    load_autonomous_web_search_settings(&app, state.inner())
}

#[tauri::command]
pub fn autonomous_web_search_update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertAutonomousWebSearchSettingsRequestDto,
) -> CommandResult<AutonomousWebSearchSettingsDto> {
    let path = state.global_db_path(&app)?;
    let connection = open_global_database(&path)?;
    let mut file = load_settings_file(&connection)?;
    file.mode = request.mode;
    file.updated_at = now_timestamp();
    persist_settings_file(&connection, &file)?;
    settings_dto(&connection, file)
}

#[tauri::command]
pub fn autonomous_web_search_upsert_provider<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertAutonomousWebSearchProviderRequestDto,
) -> CommandResult<AutonomousWebSearchSettingsDto> {
    let path = state.global_db_path(&app)?;
    let connection = open_global_database(&path)?;
    let mut file = load_settings_file(&connection)?;
    let now = now_timestamp();
    let profile_id = match request.profile_id.as_deref() {
        Some(value) => normalize_profile_id(value)?,
        None => generated_profile_id(request.kind),
    };
    let existing_index = file
        .providers
        .iter()
        .position(|provider| provider.profile_id == profile_id);
    let mut provider = existing_index
        .and_then(|index| file.providers.get(index).cloned())
        .unwrap_or_else(|| default_provider_profile(profile_id.clone(), request.kind, &now));

    if provider.kind != request.kind && existing_index.is_some() {
        return Err(CommandError::user_fixable(
            "autonomous_web_search_provider_kind_locked",
            "Xero cannot change a saved web-search provider's kind. Delete it and create a new provider instead.",
        ));
    }

    if let Some(display_name) = normalize_optional_text(request.display_name) {
        provider.display_name = display_name;
    }
    if let Some(enabled) = request.enabled {
        provider.enabled = enabled;
    }
    provider.endpoint = normalize_optional_url_field(request.endpoint, provider.endpoint.take())?;
    provider.base_url = normalize_optional_url_field(request.base_url, provider.base_url.take())?;
    provider.google_cse_cx =
        normalize_optional_text(request.google_cse_cx).or(provider.google_cse_cx.take());
    provider.result_limit = request
        .result_limit
        .or(provider.result_limit)
        .map(validate_result_limit)
        .transpose()?;
    provider.timeout_ms = request
        .timeout_ms
        .or(provider.timeout_ms)
        .map(validate_timeout_ms)
        .transpose()?;
    provider.region = normalize_optional_short_text(request.region).or(provider.region.take());
    provider.language =
        normalize_optional_short_text(request.language).or(provider.language.take());
    provider.freshness =
        normalize_optional_short_text(request.freshness).or(provider.freshness.take());
    if request.safe_search.is_some() {
        provider.safe_search = request.safe_search;
    }
    provider.updated_at = now.clone();
    validate_provider_profile(&provider)?;

    let credential_id = web_search_credential_provider_id(&provider.profile_id);
    if request.clear_api_key.unwrap_or(false) {
        delete_provider_credential(&connection, &credential_id)?;
    }
    if let Some(api_key) = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        upsert_provider_credential(
            &connection,
            &ProviderCredentialRecord {
                provider_id: credential_id,
                kind: ProviderCredentialKind::ApiKey,
                api_key: Some(api_key.to_owned()),
                oauth_account_id: None,
                oauth_session_id: None,
                oauth_access_token: None,
                oauth_refresh_token: None,
                oauth_expires_at: None,
                base_url: None,
                api_version: None,
                region: None,
                project_id: None,
                default_model_id: None,
                updated_at: now.clone(),
            },
        )?;
    }

    match existing_index {
        Some(index) => file.providers[index] = provider,
        None => file.providers.push(provider),
    }
    file.providers
        .sort_by(|left, right| left.display_name.cmp(&right.display_name));
    if active_provider_is_ready(&connection, &file)? {
        // Keep the existing active provider when it is still runnable.
    } else if let Some(ready_provider_id) = first_ready_provider_id(&connection, &file)? {
        file.active_provider_id = Some(ready_provider_id);
    } else {
        file.active_provider_id = None;
    }
    file.updated_at = now;
    persist_settings_file(&connection, &file)?;
    settings_dto(&connection, file)
}

#[tauri::command]
pub fn autonomous_web_search_delete_provider<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: DeleteAutonomousWebSearchProviderRequestDto,
) -> CommandResult<AutonomousWebSearchSettingsDto> {
    let provider_id = normalize_profile_id(&request.provider_id)?;
    let path = state.global_db_path(&app)?;
    let connection = open_global_database(&path)?;
    let mut file = load_settings_file(&connection)?;
    let before = file.providers.len();
    file.providers
        .retain(|provider| provider.profile_id != provider_id);
    if file.providers.len() == before {
        return Err(CommandError::user_fixable(
            "autonomous_web_search_provider_missing",
            format!("Xero could not find web-search provider `{provider_id}`."),
        ));
    }
    delete_provider_credential(
        &connection,
        &web_search_credential_provider_id(&provider_id),
    )?;
    if file.active_provider_id.as_deref() == Some(provider_id.as_str()) {
        file.active_provider_id = first_ready_provider_id(&connection, &file)?;
    }
    file.updated_at = now_timestamp();
    persist_settings_file(&connection, &file)?;
    settings_dto(&connection, file)
}

#[tauri::command]
pub fn autonomous_web_search_set_active_provider<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SetActiveAutonomousWebSearchProviderRequestDto,
) -> CommandResult<AutonomousWebSearchSettingsDto> {
    let provider_id = normalize_profile_id(&request.provider_id)?;
    let path = state.global_db_path(&app)?;
    let connection = open_global_database(&path)?;
    let mut file = load_settings_file(&connection)?;
    let provider = file
        .providers
        .iter()
        .find(|provider| provider.profile_id == provider_id)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_web_search_provider_missing",
                format!("Xero could not find web-search provider `{provider_id}`."),
            )
        })?;
    let readiness = provider_readiness(&connection, provider)?;
    if !readiness.ready {
        return Err(CommandError::user_fixable(
            "autonomous_web_search_provider_not_ready",
            readiness.message,
        ));
    }
    file.active_provider_id = Some(provider_id);
    file.updated_at = now_timestamp();
    persist_settings_file(&connection, &file)?;
    settings_dto(&connection, file)
}

#[tauri::command]
pub fn autonomous_web_search_check_provider<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CheckAutonomousWebSearchProviderRequestDto,
) -> CommandResult<AutonomousWebSearchSettingsDto> {
    let provider_id = normalize_profile_id(&request.provider_id)?;
    let path = state.global_db_path(&app)?;
    let connection = open_global_database(&path)?;
    let mut file = load_settings_file(&connection)?;
    let provider_index = file
        .providers
        .iter()
        .position(|provider| provider.profile_id == provider_id)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_web_search_provider_missing",
                format!("Xero could not find web-search provider `{provider_id}`."),
            )
        })?;
    let provider_config = provider_runtime_config(&connection, &file.providers[provider_index])?;
    let query = request
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Xero web search provider check");
    let started = Instant::now();
    let checked_at = now_timestamp();
    let runtime = AutonomousWebRuntime::new(AutonomousWebConfig {
        search_mode: AutonomousWebSearchMode::ConfiguredProviderOnly,
        managed_search: None,
        search_provider: Some(provider_config),
        limits: AutonomousWebRuntimeLimits::default(),
    });
    let check = match runtime.search(AutonomousWebSearchRequest {
        query: query.into(),
        result_count: Some(3),
        timeout_ms: Some(8_000),
    }) {
        Ok(output) => AutonomousWebSearchProviderCheckDto {
            status: "passed".into(),
            code: "ok".into(),
            message: "Provider returned usable web search results.".into(),
            latency_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            sample_result_count: output.results.len(),
            checked_at,
        },
        Err(error) => AutonomousWebSearchProviderCheckDto {
            status: "failed".into(),
            code: error.code,
            message: redact_provider_check_message(&error.message),
            latency_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            sample_result_count: 0,
            checked_at,
        },
    };
    file.providers[provider_index].last_check = Some(check);
    file.providers[provider_index].updated_at = now_timestamp();
    file.updated_at = now_timestamp();
    persist_settings_file(&connection, &file)?;
    settings_dto(&connection, file)
}

pub(crate) fn load_autonomous_web_search_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<AutonomousWebSearchSettingsDto> {
    let connection = open_global_database(&state.global_db_path(app)?)?;
    let file = load_settings_file(&connection)?;
    settings_dto(&connection, file)
}

pub(crate) fn resolve_autonomous_web_config<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider_config: Option<&AgentProviderConfig>,
) -> CommandResult<AutonomousWebConfig> {
    if let Some(config) = state.autonomous_web_config_override() {
        return Ok(config);
    }

    let connection = open_global_database(&state.global_db_path(app)?)?;
    let file = load_settings_file(&connection)?;
    let configured = file
        .active_provider_id
        .as_deref()
        .and_then(|active_id| {
            file.providers
                .iter()
                .find(|provider| provider.profile_id == active_id && provider.enabled)
        })
        .map(|provider| provider_runtime_config(&connection, provider))
        .transpose()?;
    Ok(AutonomousWebConfig {
        search_mode: file.mode,
        managed_search: provider_config.and_then(managed_config_from_agent_provider_config),
        search_provider: configured,
        limits: AutonomousWebRuntimeLimits::default(),
    })
}

fn load_settings_file(connection: &Connection) -> CommandResult<AutonomousWebSettingsFile> {
    let payload = connection
        .query_row(
            "SELECT payload FROM autonomous_web_settings WHERE id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| {
            CommandError::retryable(
                "autonomous_web_settings_read_failed",
                format!("Xero could not read Web Search settings: {error}"),
            )
        })?;
    let Some(payload) = payload else {
        return Ok(default_settings_file());
    };
    let parsed = serde_json::from_str::<AutonomousWebSettingsFile>(&payload).map_err(|error| {
        CommandError::user_fixable(
            "autonomous_web_settings_decode_failed",
            format!("Xero could not decode Web Search settings: {error}"),
        )
    })?;
    validate_settings_file(parsed)
}

fn persist_settings_file(
    connection: &Connection,
    file: &AutonomousWebSettingsFile,
) -> CommandResult<()> {
    let file = validate_settings_file(file.clone())?;
    let payload = serde_json::to_string(&file).map_err(|error| {
        CommandError::system_fault(
            "autonomous_web_settings_serialize_failed",
            format!("Xero could not serialize Web Search settings: {error}"),
        )
    })?;
    connection
        .execute(
            "INSERT INTO autonomous_web_settings (id, payload, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET payload = excluded.payload, updated_at = excluded.updated_at",
            params![payload, file.updated_at],
        )
        .map_err(|error| {
            CommandError::retryable(
                "autonomous_web_settings_write_failed",
                format!("Xero could not persist Web Search settings: {error}"),
            )
        })?;
    Ok(())
}

fn validate_settings_file(
    mut file: AutonomousWebSettingsFile,
) -> CommandResult<AutonomousWebSettingsFile> {
    if file.schema_version != AUTONOMOUS_WEB_SETTINGS_SCHEMA_VERSION {
        return Err(CommandError::user_fixable(
            "autonomous_web_settings_decode_failed",
            format!(
                "Xero rejected Web Search settings version `{}` because only version `{AUTONOMOUS_WEB_SETTINGS_SCHEMA_VERSION}` is supported.",
                file.schema_version
            ),
        ));
    }
    let mut seen = BTreeMap::new();
    for provider in &mut file.providers {
        provider.profile_id = normalize_profile_id(&provider.profile_id)?;
        validate_provider_profile(provider)?;
        if seen.insert(provider.profile_id.clone(), ()).is_some() {
            return Err(CommandError::user_fixable(
                "autonomous_web_settings_decode_failed",
                format!(
                    "Xero found duplicate Web Search provider id `{}`.",
                    provider.profile_id
                ),
            ));
        }
    }
    if let Some(active_id) = file.active_provider_id.take() {
        let active_id = normalize_profile_id(&active_id)?;
        if file
            .providers
            .iter()
            .any(|provider| provider.profile_id == active_id)
        {
            file.active_provider_id = Some(active_id);
        }
    }
    if file.updated_at.trim().is_empty() {
        file.updated_at = now_timestamp();
    }
    Ok(file)
}

fn settings_dto(
    connection: &Connection,
    file: AutonomousWebSettingsFile,
) -> CommandResult<AutonomousWebSearchSettingsDto> {
    let providers = file
        .providers
        .iter()
        .map(|provider| provider_dto(connection, provider))
        .collect::<CommandResult<Vec<_>>>()?;
    Ok(AutonomousWebSearchSettingsDto {
        mode: file.mode,
        active_provider_id: file.active_provider_id,
        providers,
        provider_kinds: provider_kind_metadata(),
        provider_managed: provider_managed_status(),
        updated_at: Some(file.updated_at),
    })
}

fn provider_dto(
    connection: &Connection,
    provider: &AutonomousWebProviderProfileFile,
) -> CommandResult<AutonomousWebSearchProviderProfileDto> {
    let credential = load_provider_credential(
        connection,
        &web_search_credential_provider_id(&provider.profile_id),
    )?;
    let readiness = provider_readiness(connection, provider)?;
    Ok(AutonomousWebSearchProviderProfileDto {
        profile_id: provider.profile_id.clone(),
        kind: provider.kind,
        display_name: provider.display_name.clone(),
        enabled: provider.enabled,
        endpoint: provider.endpoint.clone(),
        base_url: provider.base_url.clone(),
        google_cse_cx: provider.google_cse_cx.clone(),
        result_limit: provider.result_limit,
        timeout_ms: provider.timeout_ms,
        region: provider.region.clone(),
        language: provider.language.clone(),
        freshness: provider.freshness.clone(),
        safe_search: provider.safe_search,
        has_api_key: credential
            .as_ref()
            .and_then(|record| record.api_key.as_deref())
            .is_some_and(|value| !value.trim().is_empty()),
        api_key_updated_at: credential.map(|record| record.updated_at),
        readiness,
        last_check: provider.last_check.clone(),
        created_at: provider.created_at.clone(),
        updated_at: provider.updated_at.clone(),
    })
}

fn provider_runtime_config(
    connection: &Connection,
    provider: &AutonomousWebProviderProfileFile,
) -> CommandResult<AutonomousWebSearchProviderConfig> {
    let api_key = load_provider_credential(
        connection,
        &web_search_credential_provider_id(&provider.profile_id),
    )?
    .and_then(|record| record.api_key)
    .map(|value| value.trim().to_owned())
    .filter(|value| !value.is_empty());
    Ok(AutonomousWebSearchProviderConfig {
        profile_id: provider.profile_id.clone(),
        kind: provider.kind,
        display_name: provider.display_name.clone(),
        endpoint: provider.endpoint.clone(),
        base_url: provider.base_url.clone(),
        api_key,
        google_cse_cx: provider.google_cse_cx.clone(),
        result_limit: provider.result_limit,
        timeout_ms: provider.timeout_ms,
        region: provider.region.clone(),
        language: provider.language.clone(),
        freshness: provider.freshness.clone(),
        safe_search: provider.safe_search,
    })
}

fn provider_readiness(
    connection: &Connection,
    provider: &AutonomousWebProviderProfileFile,
) -> CommandResult<AutonomousWebSearchProviderReadinessDto> {
    if !provider.enabled {
        return Ok(readiness(false, "disabled", "Provider is disabled."));
    }
    if provider.kind.requires_api_key() {
        let has_api_key = load_provider_credential(
            connection,
            &web_search_credential_provider_id(&provider.profile_id),
        )?
        .and_then(|record| record.api_key)
        .is_some_and(|value| !value.trim().is_empty());
        if !has_api_key {
            return Ok(readiness(
                false,
                "missing_api_key",
                "Provider needs an API key.",
            ));
        }
    }
    if let Err(error) = validate_provider_profile(provider) {
        return Ok(readiness(false, "invalid_settings", &error.message));
    }
    Ok(readiness(true, "ready", "Provider is ready."))
}

fn readiness(
    ready: bool,
    status: impl Into<String>,
    message: impl Into<String>,
) -> AutonomousWebSearchProviderReadinessDto {
    AutonomousWebSearchProviderReadinessDto {
        ready,
        status: status.into(),
        message: message.into(),
    }
}

fn first_ready_provider_id(
    connection: &Connection,
    file: &AutonomousWebSettingsFile,
) -> CommandResult<Option<String>> {
    for provider in &file.providers {
        if provider_readiness(connection, provider)?.ready {
            return Ok(Some(provider.profile_id.clone()));
        }
    }
    Ok(None)
}

fn active_provider_is_ready(
    connection: &Connection,
    file: &AutonomousWebSettingsFile,
) -> CommandResult<bool> {
    let Some(active_provider_id) = file.active_provider_id.as_deref() else {
        return Ok(false);
    };
    let Some(provider) = file
        .providers
        .iter()
        .find(|provider| provider.profile_id == active_provider_id)
    else {
        return Ok(false);
    };
    provider_readiness(connection, provider).map(|readiness| readiness.ready)
}

fn default_settings_file() -> AutonomousWebSettingsFile {
    AutonomousWebSettingsFile {
        schema_version: AUTONOMOUS_WEB_SETTINGS_SCHEMA_VERSION,
        mode: AutonomousWebSearchMode::Auto,
        active_provider_id: None,
        providers: Vec::new(),
        updated_at: now_timestamp(),
    }
}

fn default_provider_profile(
    profile_id: String,
    kind: AutonomousWebSearchProviderKind,
    now: &str,
) -> AutonomousWebProviderProfileFile {
    AutonomousWebProviderProfileFile {
        profile_id,
        kind,
        display_name: provider_kind_label(kind).into(),
        enabled: true,
        endpoint: default_endpoint_for_kind(kind).map(str::to_owned),
        base_url: None,
        google_cse_cx: None,
        result_limit: Some(5),
        timeout_ms: Some(8_000),
        region: Some("us".into()),
        language: Some("en".into()),
        freshness: None,
        safe_search: Some(true),
        last_check: None,
        created_at: now.into(),
        updated_at: now.into(),
    }
}

fn validate_provider_profile(provider: &AutonomousWebProviderProfileFile) -> CommandResult<()> {
    normalize_profile_id(&provider.profile_id)?;
    if provider.display_name.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "autonomous_web_search_provider_invalid",
            "Xero requires web-search providers to have a display name.",
        ));
    }
    if let Some(endpoint) = &provider.endpoint {
        validate_http_url(endpoint)?;
    }
    if let Some(base_url) = &provider.base_url {
        validate_http_url(base_url)?;
    }
    if matches!(
        provider.kind,
        AutonomousWebSearchProviderKind::CustomEndpoint
            | AutonomousWebSearchProviderKind::SearxngJson
    ) && provider
        .endpoint
        .as_deref()
        .or(provider.base_url.as_deref())
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err(CommandError::user_fixable(
            "autonomous_web_search_provider_invalid",
            "Xero requires this web-search provider kind to have an HTTP or HTTPS endpoint.",
        ));
    }
    if provider.kind == AutonomousWebSearchProviderKind::GoogleCse
        && provider
            .google_cse_cx
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(CommandError::user_fixable(
            "autonomous_web_search_provider_invalid",
            "Xero requires Google CSE providers to include a search engine id (`cx`).",
        ));
    }
    if let Some(limit) = provider.result_limit {
        validate_result_limit(limit)?;
    }
    if let Some(timeout_ms) = provider.timeout_ms {
        validate_timeout_ms(timeout_ms)?;
    }
    Ok(())
}

fn managed_config_from_agent_provider_config(
    config: &AgentProviderConfig,
) -> Option<AutonomousWebManagedSearchConfig> {
    match config {
        AgentProviderConfig::OpenAiResponses(config)
            if openai_model_supports_web_search(&config.model_id) =>
        {
            Some(AutonomousWebManagedSearchConfig {
                kind: AutonomousWebManagedSearchKind::OpenAiNativeWebSearch,
                provider_id: config.provider_id.clone(),
                model_id: config.model_id.clone(),
                base_url: config.base_url.clone(),
                api_key: config.api_key.clone(),
                account_id: None,
                session_id: None,
                api_version: None,
                timeout_ms: (config.timeout_ms > 0).then_some(config.timeout_ms),
            })
        }
        AgentProviderConfig::OpenAiCodexResponses(config)
            if openai_model_supports_web_search(&config.model_id) =>
        {
            Some(AutonomousWebManagedSearchConfig {
                kind: AutonomousWebManagedSearchKind::OpenAiNativeWebSearch,
                provider_id: config.provider_id.clone(),
                model_id: config.model_id.clone(),
                base_url: config.base_url.clone(),
                api_key: config.access_token.clone(),
                account_id: Some(config.account_id.clone()),
                session_id: config.session_id.clone(),
                api_version: None,
                timeout_ms: (config.timeout_ms > 0).then_some(config.timeout_ms),
            })
        }
        AgentProviderConfig::OpenAiCompatible(config)
            if config.provider_id == OPENROUTER_PROVIDER_ID =>
        {
            Some(AutonomousWebManagedSearchConfig {
                kind: AutonomousWebManagedSearchKind::OpenRouterServerWebSearch,
                provider_id: config.provider_id.clone(),
                model_id: config.model_id.clone(),
                base_url: config.base_url.clone(),
                api_key: config.api_key.clone()?,
                account_id: None,
                session_id: None,
                api_version: config.api_version.clone(),
                timeout_ms: (config.timeout_ms > 0).then_some(config.timeout_ms),
            })
        }
        AgentProviderConfig::OpenAiCompatible(config)
            if config.provider_id == GEMINI_AI_STUDIO_PROVIDER_ID
                && gemini_model_supports_google_search(&config.model_id) =>
        {
            Some(AutonomousWebManagedSearchConfig {
                kind: AutonomousWebManagedSearchKind::GeminiGroundingGoogleSearch,
                provider_id: config.provider_id.clone(),
                model_id: config.model_id.clone(),
                base_url: "https://generativelanguage.googleapis.com".into(),
                api_key: config.api_key.clone()?,
                account_id: None,
                session_id: None,
                api_version: config.api_version.clone(),
                timeout_ms: (config.timeout_ms > 0).then_some(config.timeout_ms),
            })
        }
        AgentProviderConfig::OpenAiCompatible(config)
            if (config.provider_id == OPENAI_API_PROVIDER_ID
                || config.provider_id == AZURE_OPENAI_PROVIDER_ID)
                && openai_model_supports_web_search(&config.model_id) =>
        {
            Some(AutonomousWebManagedSearchConfig {
                kind: AutonomousWebManagedSearchKind::OpenAiNativeWebSearch,
                provider_id: config.provider_id.clone(),
                model_id: config.model_id.clone(),
                base_url: config.base_url.clone(),
                api_key: config.api_key.clone()?,
                account_id: None,
                session_id: None,
                api_version: config.api_version.clone(),
                timeout_ms: (config.timeout_ms > 0).then_some(config.timeout_ms),
            })
        }
        AgentProviderConfig::XaiResponses(config)
            if is_supported_xai_text_model_id(&config.model_id) =>
        {
            Some(AutonomousWebManagedSearchConfig {
                kind: AutonomousWebManagedSearchKind::XaiNativeWebSearch,
                provider_id: XAI_PROVIDER_ID.into(),
                model_id: config.model_id.clone(),
                base_url: config.base_url.clone(),
                api_key: config.bearer_token.clone(),
                account_id: None,
                session_id: None,
                api_version: None,
                timeout_ms: (config.timeout_ms > 0).then_some(config.timeout_ms),
            })
        }
        AgentProviderConfig::Anthropic(config)
            if anthropic_model_may_support_web_search(&config.model_id) =>
        {
            Some(anthropic_managed_config(config))
        }
        AgentProviderConfig::Vertex(config)
            if anthropic_model_may_support_web_search(&config.model_id) =>
        {
            Some(vertex_anthropic_managed_config(config))
        }
        AgentProviderConfig::Bedrock(_) => None,
        _ => None,
    }
}

fn openai_model_supports_web_search(model_id: &str) -> bool {
    let model_id = normalized_model_leaf(model_id);
    if model_id == "gpt-4.1-nano" {
        return false;
    }
    model_id.starts_with("gpt-4")
        || model_id.starts_with("gpt-5")
        || model_id.starts_with("o4")
        || model_id.contains("search")
}

fn gemini_model_supports_google_search(model_id: &str) -> bool {
    let model_id = normalized_model_leaf(model_id);
    model_id.starts_with("gemini-2.0")
        || model_id.starts_with("gemini-2.5")
        || model_id.starts_with("gemini-3")
}

fn anthropic_model_may_support_web_search(model_id: &str) -> bool {
    normalized_model_leaf(model_id).contains("claude")
}

fn normalized_model_leaf(model_id: &str) -> String {
    model_id
        .trim()
        .rsplit('/')
        .next()
        .unwrap_or(model_id)
        .trim()
        .to_ascii_lowercase()
}

fn anthropic_managed_config(config: &AnthropicProviderConfig) -> AutonomousWebManagedSearchConfig {
    AutonomousWebManagedSearchConfig {
        kind: AutonomousWebManagedSearchKind::AnthropicNativeWebSearch,
        provider_id: ANTHROPIC_PROVIDER_ID.into(),
        model_id: config.model_id.clone(),
        base_url: config.base_url.clone(),
        api_key: config.api_key.clone(),
        account_id: None,
        session_id: None,
        api_version: Some(config.anthropic_version.clone()),
        timeout_ms: (config.timeout_ms > 0).then_some(config.timeout_ms),
    }
}

fn vertex_anthropic_managed_config(
    config: &crate::runtime::VertexProviderConfig,
) -> AutonomousWebManagedSearchConfig {
    AutonomousWebManagedSearchConfig {
        kind: AutonomousWebManagedSearchKind::AnthropicNativeWebSearch,
        provider_id: VERTEX_PROVIDER_ID.into(),
        model_id: config.model_id.clone(),
        base_url: vertex_anthropic_raw_predict_url(config),
        api_key: String::new(),
        account_id: None,
        session_id: None,
        api_version: Some("vertex-2023-10-16".into()),
        timeout_ms: (config.timeout_ms > 0).then_some(config.timeout_ms),
    }
}

fn vertex_anthropic_raw_predict_url(config: &crate::runtime::VertexProviderConfig) -> String {
    format!(
        "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/anthropic/models/{}:rawPredict",
        config.region.trim(),
        config.project_id.trim(),
        config.region.trim(),
        config.model_id.trim(),
    )
}

fn provider_kind_metadata() -> Vec<AutonomousWebSearchProviderKindMetadataDto> {
    [
        AutonomousWebSearchProviderKind::CustomEndpoint,
        AutonomousWebSearchProviderKind::BraveSearch,
        AutonomousWebSearchProviderKind::TavilySearch,
        AutonomousWebSearchProviderKind::ExaSearch,
        AutonomousWebSearchProviderKind::FirecrawlSearch,
        AutonomousWebSearchProviderKind::YouSearch,
        AutonomousWebSearchProviderKind::LinkupSearch,
        AutonomousWebSearchProviderKind::KagiSearch,
        AutonomousWebSearchProviderKind::SearxngJson,
        AutonomousWebSearchProviderKind::SerpapiGoogle,
        AutonomousWebSearchProviderKind::SearchapiGoogle,
        AutonomousWebSearchProviderKind::GoogleCse,
    ]
    .into_iter()
    .map(|kind| AutonomousWebSearchProviderKindMetadataDto {
        kind,
        label: provider_kind_label(kind).into(),
        requires_api_key: kind.requires_api_key(),
        supports_locale: matches!(
            kind,
            AutonomousWebSearchProviderKind::BraveSearch
                | AutonomousWebSearchProviderKind::SearxngJson
                | AutonomousWebSearchProviderKind::SerpapiGoogle
                | AutonomousWebSearchProviderKind::SearchapiGoogle
                | AutonomousWebSearchProviderKind::GoogleCse
        ),
        supports_freshness: matches!(
            kind,
            AutonomousWebSearchProviderKind::BraveSearch
                | AutonomousWebSearchProviderKind::TavilySearch
                | AutonomousWebSearchProviderKind::LinkupSearch
        ),
        supports_safe_search: matches!(
            kind,
            AutonomousWebSearchProviderKind::BraveSearch
                | AutonomousWebSearchProviderKind::SearxngJson
                | AutonomousWebSearchProviderKind::SerpapiGoogle
                | AutonomousWebSearchProviderKind::SearchapiGoogle
                | AutonomousWebSearchProviderKind::GoogleCse
        ),
        self_hosted: kind == AutonomousWebSearchProviderKind::SearxngJson,
        requires_endpoint: matches!(
            kind,
            AutonomousWebSearchProviderKind::CustomEndpoint
                | AutonomousWebSearchProviderKind::SearxngJson
        ),
        requires_google_cse_cx: kind == AutonomousWebSearchProviderKind::GoogleCse,
    })
    .collect()
}

fn provider_managed_status() -> AutonomousWebProviderManagedStatusDto {
    AutonomousWebProviderManagedStatusDto {
        mode_available: true,
        status: "depends_on_selected_model".into(),
        message: "Provider-managed search is evaluated when a run starts with the selected provider and model.".into(),
        supported_sources: vec![
            "anthropic_native_web_search".into(),
            "gemini_grounding_google_search".into(),
            "openai_native_web_search".into(),
            "openrouter_server_web_search".into(),
            "xai_native_web_search".into(),
        ],
    }
}

fn provider_kind_label(kind: AutonomousWebSearchProviderKind) -> &'static str {
    match kind {
        AutonomousWebSearchProviderKind::CustomEndpoint => "Custom endpoint",
        AutonomousWebSearchProviderKind::BraveSearch => "Brave Search",
        AutonomousWebSearchProviderKind::TavilySearch => "Tavily",
        AutonomousWebSearchProviderKind::ExaSearch => "Exa",
        AutonomousWebSearchProviderKind::FirecrawlSearch => "Firecrawl",
        AutonomousWebSearchProviderKind::YouSearch => "You.com",
        AutonomousWebSearchProviderKind::LinkupSearch => "Linkup",
        AutonomousWebSearchProviderKind::KagiSearch => "Kagi",
        AutonomousWebSearchProviderKind::SearxngJson => "SearXNG JSON",
        AutonomousWebSearchProviderKind::SerpapiGoogle => "SerpApi Google",
        AutonomousWebSearchProviderKind::SearchapiGoogle => "SearchApi Google",
        AutonomousWebSearchProviderKind::GoogleCse => "Google CSE",
    }
}

fn default_endpoint_for_kind(kind: AutonomousWebSearchProviderKind) -> Option<&'static str> {
    match kind {
        AutonomousWebSearchProviderKind::CustomEndpoint
        | AutonomousWebSearchProviderKind::SearxngJson => None,
        AutonomousWebSearchProviderKind::BraveSearch => {
            Some("https://api.search.brave.com/res/v1/web/search")
        }
        AutonomousWebSearchProviderKind::TavilySearch => Some("https://api.tavily.com/search"),
        AutonomousWebSearchProviderKind::ExaSearch => Some("https://api.exa.ai/search"),
        AutonomousWebSearchProviderKind::FirecrawlSearch => {
            Some("https://api.firecrawl.dev/v2/search")
        }
        AutonomousWebSearchProviderKind::YouSearch => Some("https://api.ydc-index.io/v1/search"),
        AutonomousWebSearchProviderKind::LinkupSearch => Some("https://api.linkup.so/v1/search"),
        AutonomousWebSearchProviderKind::KagiSearch => Some("https://kagi.com/api/v1/search"),
        AutonomousWebSearchProviderKind::SerpapiGoogle => Some("https://serpapi.com/search.json"),
        AutonomousWebSearchProviderKind::SearchapiGoogle => {
            Some("https://www.searchapi.io/api/v1/search")
        }
        AutonomousWebSearchProviderKind::GoogleCse => {
            Some("https://www.googleapis.com/customsearch/v1")
        }
    }
}

fn generated_profile_id(kind: AutonomousWebSearchProviderKind) -> String {
    let mut bytes = [0_u8; 4];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "{}-{:02x}{:02x}{:02x}{:02x}",
        kind.as_str(),
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3]
    )
}

fn normalize_profile_id(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > 80
        || !trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        return Err(CommandError::user_fixable(
            "autonomous_web_search_provider_id_invalid",
            "Xero requires web-search provider ids to use only letters, numbers, `_`, or `-`.",
        ));
    }
    Ok(trimmed.to_owned())
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_optional_short_text(value: Option<String>) -> Option<String> {
    normalize_optional_text(value).map(|value| value.chars().take(80).collect())
}

fn normalize_optional_url_field(
    next: Option<String>,
    current: Option<String>,
) -> CommandResult<Option<String>> {
    match normalize_optional_text(next) {
        Some(value) => {
            validate_http_url(&value)?;
            Ok(Some(value))
        }
        None => Ok(current),
    }
}

fn validate_http_url(value: &str) -> CommandResult<()> {
    let parsed = Url::parse(value.trim()).map_err(|_| {
        CommandError::user_fixable(
            "autonomous_web_search_provider_url_invalid",
            "Xero requires web-search provider URLs to be absolute HTTP or HTTPS URLs.",
        )
    })?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(CommandError::user_fixable(
            "autonomous_web_search_provider_url_invalid",
            "Xero requires web-search provider URLs to use HTTP or HTTPS.",
        ));
    }
    Ok(())
}

fn validate_result_limit(value: usize) -> CommandResult<usize> {
    if value == 0 || value > AutonomousWebRuntimeLimits::default().max_search_result_count {
        return Err(CommandError::user_fixable(
            "autonomous_web_search_provider_limit_invalid",
            "Xero requires web-search provider result limits to be between 1 and 10.",
        ));
    }
    Ok(value)
}

fn validate_timeout_ms(value: u64) -> CommandResult<u64> {
    if value == 0 || value > AutonomousWebRuntimeLimits::default().max_timeout_ms {
        return Err(CommandError::user_fixable(
            "autonomous_web_search_provider_timeout_invalid",
            "Xero requires web-search provider timeouts to be between 1 and 20000 milliseconds.",
        ));
    }
    Ok(value)
}

fn redact_provider_check_message(message: &str) -> String {
    message
        .replace("api_key=", "api_key=<redacted>")
        .replace("key=", "key=<redacted>")
        .replace("Authorization", "authorization")
}

#[allow(dead_code)]
pub(crate) fn load_autonomous_web_settings_from_path(
    path: &Path,
) -> CommandResult<AutonomousWebSearchSettingsDto> {
    let connection = open_global_database(path)?;
    settings_dto(&connection, load_settings_file(&connection)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global_db::migrations::migrations;

    fn open_in_memory() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open in-memory db");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        migrations()
            .to_latest(&mut connection)
            .expect("walk migrations to latest");
        connection
    }

    #[test]
    fn default_settings_are_auto_without_configured_provider() {
        let connection = open_in_memory();
        let settings = settings_dto(&connection, load_settings_file(&connection).expect("load"))
            .expect("settings dto");

        assert_eq!(settings.mode, AutonomousWebSearchMode::Auto);
        assert!(settings.active_provider_id.is_none());
        assert!(settings.providers.is_empty());
        assert!(settings.provider_kinds.len() >= 12);
    }

    #[test]
    fn provider_profile_uses_provider_credentials_for_secret_readiness() {
        let connection = open_in_memory();
        let now = "2026-05-31T12:00:00Z";
        let provider = AutonomousWebProviderProfileFile {
            profile_id: "brave-main".into(),
            kind: AutonomousWebSearchProviderKind::BraveSearch,
            display_name: "Brave".into(),
            enabled: true,
            endpoint: None,
            base_url: None,
            google_cse_cx: None,
            result_limit: Some(5),
            timeout_ms: Some(8_000),
            region: Some("us".into()),
            language: Some("en".into()),
            freshness: None,
            safe_search: Some(true),
            last_check: None,
            created_at: now.into(),
            updated_at: now.into(),
        };

        let missing_key = provider_readiness(&connection, &provider).expect("readiness");
        assert!(!missing_key.ready);
        assert_eq!(missing_key.status, "missing_api_key");

        upsert_provider_credential(
            &connection,
            &ProviderCredentialRecord {
                provider_id: web_search_credential_provider_id("brave-main"),
                kind: ProviderCredentialKind::ApiKey,
                api_key: Some("secret-key".into()),
                oauth_account_id: None,
                oauth_session_id: None,
                oauth_access_token: None,
                oauth_refresh_token: None,
                oauth_expires_at: None,
                base_url: None,
                api_version: None,
                region: None,
                project_id: None,
                default_model_id: None,
                updated_at: now.into(),
            },
        )
        .expect("upsert secret");

        let ready = provider_readiness(&connection, &provider).expect("readiness");
        assert!(ready.ready);
        let dto = provider_dto(&connection, &provider).expect("provider dto");
        assert!(dto.has_api_key);
        assert_eq!(dto.api_key_updated_at.as_deref(), Some(now));

        let runtime_config =
            provider_runtime_config(&connection, &provider).expect("runtime config");
        assert_eq!(runtime_config.api_key.as_deref(), Some("secret-key"));
    }

    #[test]
    fn provider_managed_config_maps_openai_codex_sessions_to_native_search() {
        let managed =
            managed_config_from_agent_provider_config(&AgentProviderConfig::OpenAiCodexResponses(
                crate::runtime::OpenAiCodexResponsesProviderConfig {
                    provider_id: crate::runtime::OPENAI_CODEX_PROVIDER_ID.into(),
                    model_id: "gpt-5.5".into(),
                    base_url: "https://chatgpt.com/backend-api".into(),
                    access_token: "codex-access-token".into(),
                    account_id: "account-1".into(),
                    session_id: Some("session-1".into()),
                    timeout_ms: 12_000,
                },
            ))
            .expect("managed search config");

        assert_eq!(
            managed.kind,
            AutonomousWebManagedSearchKind::OpenAiNativeWebSearch
        );
        assert_eq!(
            managed.provider_id,
            crate::runtime::OPENAI_CODEX_PROVIDER_ID
        );
        assert_eq!(managed.model_id, "gpt-5.5");
        assert_eq!(managed.api_key, "codex-access-token");
        assert_eq!(managed.account_id.as_deref(), Some("account-1"));
        assert_eq!(managed.session_id.as_deref(), Some("session-1"));
        assert_eq!(managed.timeout_ms, Some(12_000));
    }

    #[test]
    fn provider_managed_config_maps_supported_llm_search_sources() {
        let azure =
            managed_config_from_agent_provider_config(&AgentProviderConfig::OpenAiCompatible(
                crate::runtime::OpenAiCompatibleProviderConfig {
                    provider_id: crate::runtime::AZURE_OPENAI_PROVIDER_ID.into(),
                    model_id: "gpt-4.1".into(),
                    base_url: "https://example-resource.openai.azure.com/openai/v1".into(),
                    api_key: Some("azure-key".into()),
                    api_version: Some("2026-03-01-preview".into()),
                    timeout_ms: 8_000,
                },
            ))
            .expect("azure managed search config");
        assert_eq!(
            azure.kind,
            AutonomousWebManagedSearchKind::OpenAiNativeWebSearch
        );
        assert_eq!(azure.provider_id, crate::runtime::AZURE_OPENAI_PROVIDER_ID);
        assert_eq!(azure.api_key, "azure-key");
        assert_eq!(azure.api_version.as_deref(), Some("2026-03-01-preview"));

        let gemini =
            managed_config_from_agent_provider_config(&AgentProviderConfig::OpenAiCompatible(
                crate::runtime::OpenAiCompatibleProviderConfig {
                    provider_id: crate::runtime::GEMINI_AI_STUDIO_PROVIDER_ID.into(),
                    model_id: "gemini-2.5-pro".into(),
                    base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
                    api_key: Some("gemini-key".into()),
                    api_version: None,
                    timeout_ms: 0,
                },
            ))
            .expect("gemini managed search config");
        assert_eq!(
            gemini.kind,
            AutonomousWebManagedSearchKind::GeminiGroundingGoogleSearch
        );

        let xai = managed_config_from_agent_provider_config(&AgentProviderConfig::XaiResponses(
            crate::runtime::XaiResponsesProviderConfig {
                provider_id: crate::runtime::XAI_PROVIDER_ID.into(),
                model_id: "grok-4.3".into(),
                base_url: "https://api.x.ai/v1".into(),
                bearer_token: "xai-token".into(),
                timeout_ms: 0,
            },
        ))
        .expect("xai managed search config");
        assert_eq!(xai.kind, AutonomousWebManagedSearchKind::XaiNativeWebSearch);

        let anthropic = managed_config_from_agent_provider_config(&AgentProviderConfig::Anthropic(
            crate::runtime::AnthropicProviderConfig {
                provider_id: crate::runtime::ANTHROPIC_PROVIDER_ID.into(),
                model_id: "claude-sonnet-4-5".into(),
                api_key: "anthropic-key".into(),
                base_url: "https://api.anthropic.com".into(),
                anthropic_version: "2023-06-01".into(),
                timeout_ms: 0,
            },
        ))
        .expect("anthropic managed search config");
        assert_eq!(
            anthropic.kind,
            AutonomousWebManagedSearchKind::AnthropicNativeWebSearch
        );

        let openrouter =
            managed_config_from_agent_provider_config(&AgentProviderConfig::OpenAiCompatible(
                crate::runtime::OpenAiCompatibleProviderConfig {
                    provider_id: crate::runtime::OPENROUTER_PROVIDER_ID.into(),
                    model_id: "openai/gpt-5.2".into(),
                    base_url: "https://openrouter.ai/api/v1".into(),
                    api_key: Some("openrouter-key".into()),
                    api_version: None,
                    timeout_ms: 0,
                },
            ))
            .expect("openrouter managed search config");
        assert_eq!(
            openrouter.kind,
            AutonomousWebManagedSearchKind::OpenRouterServerWebSearch
        );

        let vertex = managed_config_from_agent_provider_config(&AgentProviderConfig::Vertex(
            crate::runtime::VertexProviderConfig {
                model_id: "claude-sonnet-4-5".into(),
                region: "us-east5".into(),
                project_id: "project-1".into(),
                timeout_ms: 0,
            },
        ))
        .expect("vertex managed search config");
        assert_eq!(vertex.provider_id, crate::runtime::VERTEX_PROVIDER_ID);
        assert!(vertex.base_url.contains("/publishers/anthropic/models/"));
        assert!(vertex.api_key.is_empty());
    }

    #[test]
    fn provider_managed_config_skips_known_unsupported_or_legacy_search_models() {
        let old_openai =
            managed_config_from_agent_provider_config(&AgentProviderConfig::OpenAiCompatible(
                crate::runtime::OpenAiCompatibleProviderConfig {
                    provider_id: crate::runtime::OPENAI_API_PROVIDER_ID.into(),
                    model_id: "gpt-3.5-turbo".into(),
                    base_url: "https://api.openai.com/v1".into(),
                    api_key: Some("openai-key".into()),
                    api_version: None,
                    timeout_ms: 0,
                },
            ));
        assert!(old_openai.is_none());

        let openai_nano =
            managed_config_from_agent_provider_config(&AgentProviderConfig::OpenAiCompatible(
                crate::runtime::OpenAiCompatibleProviderConfig {
                    provider_id: crate::runtime::OPENAI_API_PROVIDER_ID.into(),
                    model_id: "gpt-4.1-nano".into(),
                    base_url: "https://api.openai.com/v1".into(),
                    api_key: Some("openai-key".into()),
                    api_version: None,
                    timeout_ms: 0,
                },
            ));
        assert!(openai_nano.is_none());

        let old_gemini =
            managed_config_from_agent_provider_config(&AgentProviderConfig::OpenAiCompatible(
                crate::runtime::OpenAiCompatibleProviderConfig {
                    provider_id: crate::runtime::GEMINI_AI_STUDIO_PROVIDER_ID.into(),
                    model_id: "gemini-1.5-pro".into(),
                    base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
                    api_key: Some("gemini-key".into()),
                    api_version: None,
                    timeout_ms: 0,
                },
            ));
        assert!(old_gemini.is_none());

        let unsupported_xai = managed_config_from_agent_provider_config(
            &AgentProviderConfig::XaiResponses(crate::runtime::XaiResponsesProviderConfig {
                provider_id: crate::runtime::XAI_PROVIDER_ID.into(),
                model_id: "grok-imagine-image-quality".into(),
                base_url: "https://api.x.ai/v1".into(),
                bearer_token: "xai-token".into(),
                timeout_ms: 0,
            }),
        );
        assert!(unsupported_xai.is_none());

        let bedrock = managed_config_from_agent_provider_config(&AgentProviderConfig::Bedrock(
            crate::runtime::BedrockProviderConfig {
                model_id: "anthropic.claude-3-5-sonnet-20241022-v2:0".into(),
                region: "us-east-1".into(),
                timeout_ms: 0,
            },
        ));
        assert!(bedrock.is_none());
    }

    #[test]
    fn settings_validation_requires_google_cse_cx_and_custom_endpoint() {
        let connection = open_in_memory();
        let now = "2026-05-31T12:00:00Z";
        let google = default_provider_profile(
            "google".into(),
            AutonomousWebSearchProviderKind::GoogleCse,
            now,
        );
        let google_error =
            validate_provider_profile(&google).expect_err("Google CSE without cx should fail");
        assert_eq!(google_error.code, "autonomous_web_search_provider_invalid");

        let custom = default_provider_profile(
            "custom".into(),
            AutonomousWebSearchProviderKind::CustomEndpoint,
            now,
        );
        let custom_error =
            validate_provider_profile(&custom).expect_err("custom without endpoint should fail");
        assert_eq!(custom_error.code, "autonomous_web_search_provider_invalid");

        let mut file = default_settings_file();
        file.providers.push(AutonomousWebProviderProfileFile {
            endpoint: Some("https://search.example/api".into()),
            ..custom
        });
        persist_settings_file(&connection, &file).expect("persist valid custom settings");
        let loaded = load_settings_file(&connection).expect("load persisted settings");
        assert_eq!(loaded.providers.len(), 1);
    }
}
