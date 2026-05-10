use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        AgentToolApplicationStyleDto, AgentToolApplicationStyleResolutionSourceDto, CommandError,
        CommandResult, ResolvedAgentToolApplicationStyleDto,
    },
    state::DesktopState,
};

const AGENT_TOOLING_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolingModelOverrideDto {
    pub provider_id: String,
    pub model_id: String,
    pub style: AgentToolApplicationStyleDto,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolingSettingsDto {
    pub global_default: AgentToolApplicationStyleDto,
    pub model_overrides: Vec<AgentToolingModelOverrideDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertAgentToolingModelOverrideRequestDto {
    pub provider_id: String,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<AgentToolApplicationStyleDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertAgentToolingSettingsRequestDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_default: Option<AgentToolApplicationStyleDto>,
    #[serde(default)]
    pub model_overrides: Vec<UpsertAgentToolingModelOverrideRequestDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentToolingSettingsFile {
    schema_version: u32,
    global_default: AgentToolApplicationStyleDto,
    #[serde(default)]
    model_overrides: Vec<AgentToolingModelOverrideDto>,
    updated_at: String,
}

#[tauri::command]
pub fn agent_tooling_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<AgentToolingSettingsDto> {
    load_agent_tooling_settings(&app, state.inner())
}

#[tauri::command]
pub fn agent_tooling_update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertAgentToolingSettingsRequestDto,
) -> CommandResult<AgentToolingSettingsDto> {
    let path = state.global_db_path(&app)?;
    update_agent_tooling_settings_from_path(&path, request)
}

pub(crate) fn load_agent_tooling_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<AgentToolingSettingsDto> {
    load_agent_tooling_settings_from_path(&state.global_db_path(app)?)
}

pub(crate) fn load_agent_tooling_settings_from_path(
    path: &Path,
) -> CommandResult<AgentToolingSettingsDto> {
    load_agent_tooling_settings_file(path).map(settings_dto_from_file)
}

pub(crate) fn resolve_agent_tool_application_style<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider_id: &str,
    model_id: &str,
) -> CommandResult<ResolvedAgentToolApplicationStyleDto> {
    resolve_agent_tool_application_style_from_path(
        &state.global_db_path(app)?,
        provider_id,
        model_id,
    )
}

pub(crate) fn resolve_agent_tool_application_style_from_path(
    path: &Path,
    provider_id: &str,
    model_id: &str,
) -> CommandResult<ResolvedAgentToolApplicationStyleDto> {
    let provider_id = normalize_required_identifier(provider_id, "providerId")?;
    let model_id = normalize_required_identifier(model_id, "modelId")?;
    let file = load_agent_tooling_settings_file(path)?;
    let override_match = file
        .model_overrides
        .iter()
        .find(|entry| entry.provider_id == provider_id && entry.model_id == model_id);

    Ok(match override_match {
        Some(entry) => ResolvedAgentToolApplicationStyleDto {
            provider_id,
            model_id,
            style: entry.style,
            source: AgentToolApplicationStyleResolutionSourceDto::ModelOverride,
            global_updated_at: Some(file.updated_at),
            override_updated_at: Some(entry.updated_at.clone()),
        },
        None => ResolvedAgentToolApplicationStyleDto {
            provider_id,
            model_id,
            style: file.global_default,
            source: AgentToolApplicationStyleResolutionSourceDto::GlobalDefault,
            global_updated_at: Some(file.updated_at),
            override_updated_at: None,
        },
    })
}

fn update_agent_tooling_settings_from_path(
    path: &Path,
    request: UpsertAgentToolingSettingsRequestDto,
) -> CommandResult<AgentToolingSettingsDto> {
    let mut file = load_agent_tooling_settings_file(path)?;
    let updated_at = now_timestamp();
    if let Some(global_default) = request.global_default {
        file.global_default = global_default;
        file.updated_at = updated_at.clone();
    }

    let mut overrides = file
        .model_overrides
        .into_iter()
        .map(|entry| ((entry.provider_id.clone(), entry.model_id.clone()), entry))
        .collect::<BTreeMap<_, _>>();
    let mut seen_request_keys = BTreeMap::new();
    for override_request in request.model_overrides {
        let provider_id =
            normalize_required_identifier(&override_request.provider_id, "providerId")?;
        let model_id = normalize_required_identifier(&override_request.model_id, "modelId")?;
        let key = (provider_id, model_id);
        if seen_request_keys.insert(key.clone(), ()).is_some() {
            return Err(CommandError::user_fixable(
                "agent_tooling_settings_request_invalid",
                format!(
                    "Xero rejected duplicate agent-tooling override for provider `{}` model `{}`.",
                    key.0, key.1
                ),
            ));
        }
        match override_request.style {
            Some(style) => {
                overrides.insert(
                    key.clone(),
                    AgentToolingModelOverrideDto {
                        provider_id: key.0,
                        model_id: key.1,
                        style,
                        updated_at: updated_at.clone(),
                    },
                );
            }
            None => {
                overrides.remove(&key);
            }
        }
        file.updated_at = updated_at.clone();
    }
    file.model_overrides = overrides.into_values().collect();
    persist_agent_tooling_settings_file(path, &file)?;
    Ok(settings_dto_from_file(file))
}

fn load_agent_tooling_settings_file(path: &Path) -> CommandResult<AgentToolingSettingsFile> {
    let connection = crate::global_db::open_global_database(path)?;
    let payload = match connection.query_row(
        "SELECT payload FROM agent_tooling_settings WHERE id = 1",
        [],
        |row| row.get::<_, String>(0),
    ) {
        Ok(payload) => Some(payload),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(error) => {
            return Err(CommandError::retryable(
                "agent_tooling_settings_read_failed",
                format!("Xero could not read Agent Tooling settings: {error}"),
            ));
        }
    };

    let Some(payload) = payload else {
        return Ok(default_agent_tooling_settings_file());
    };
    let parsed = serde_json::from_str::<AgentToolingSettingsFile>(&payload).map_err(|error| {
        CommandError::user_fixable(
            "agent_tooling_settings_decode_failed",
            format!(
                "Xero could not decode Agent Tooling settings stored in the global database: {error}"
            ),
        )
    })?;
    validate_agent_tooling_settings_file(parsed)
}

fn persist_agent_tooling_settings_file(
    path: &Path,
    settings: &AgentToolingSettingsFile,
) -> CommandResult<()> {
    let settings = validate_agent_tooling_settings_file(settings.clone())?;
    let payload = serde_json::to_string(&settings).map_err(|error| {
        CommandError::system_fault(
            "agent_tooling_settings_serialize_failed",
            format!("Xero could not serialize Agent Tooling settings: {error}"),
        )
    })?;

    let connection = crate::global_db::open_global_database(path)?;
    connection
        .execute(
            "INSERT INTO agent_tooling_settings (id, payload, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
                payload = excluded.payload,
                updated_at = excluded.updated_at",
            rusqlite::params![payload, settings.updated_at],
        )
        .map_err(|error| {
            CommandError::retryable(
                "agent_tooling_settings_write_failed",
                format!("Xero could not persist Agent Tooling settings: {error}"),
            )
        })?;
    Ok(())
}

fn validate_agent_tooling_settings_file(
    mut file: AgentToolingSettingsFile,
) -> CommandResult<AgentToolingSettingsFile> {
    if file.schema_version != AGENT_TOOLING_SETTINGS_SCHEMA_VERSION {
        return Err(CommandError::user_fixable(
            "agent_tooling_settings_decode_failed",
            format!(
                "Xero rejected Agent Tooling settings version `{}` because only version `{AGENT_TOOLING_SETTINGS_SCHEMA_VERSION}` is supported.",
                file.schema_version
            ),
        ));
    }

    let mut seen = BTreeMap::new();
    for entry in &mut file.model_overrides {
        entry.provider_id = normalize_required_identifier(&entry.provider_id, "providerId")?;
        entry.model_id = normalize_required_identifier(&entry.model_id, "modelId")?;
        entry.updated_at = normalize_timestamp(entry.updated_at.clone());
        let key = (entry.provider_id.clone(), entry.model_id.clone());
        if seen.insert(key.clone(), ()).is_some() {
            return Err(CommandError::user_fixable(
                "agent_tooling_settings_decode_failed",
                format!(
                    "Xero found duplicate Agent Tooling override for provider `{}` model `{}`.",
                    key.0, key.1
                ),
            ));
        }
    }
    file.model_overrides.sort_by(|left, right| {
        left.provider_id
            .cmp(&right.provider_id)
            .then(left.model_id.cmp(&right.model_id))
    });
    file.updated_at = normalize_timestamp(file.updated_at);
    Ok(file)
}

fn default_agent_tooling_settings_file() -> AgentToolingSettingsFile {
    AgentToolingSettingsFile {
        schema_version: AGENT_TOOLING_SETTINGS_SCHEMA_VERSION,
        global_default: AgentToolApplicationStyleDto::Balanced,
        model_overrides: Vec::new(),
        updated_at: now_timestamp(),
    }
}

fn settings_dto_from_file(file: AgentToolingSettingsFile) -> AgentToolingSettingsDto {
    AgentToolingSettingsDto {
        global_default: file.global_default,
        model_overrides: file.model_overrides,
        updated_at: Some(file.updated_at),
    }
}

fn normalize_required_identifier(value: &str, field: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::user_fixable(
            "agent_tooling_settings_request_invalid",
            format!("Xero requires `{field}` to be non-empty for Agent Tooling settings."),
        ));
    }
    Ok(trimmed.to_owned())
}

fn normalize_timestamp(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        now_timestamp()
    } else {
        trimmed.to_owned()
    }
}
