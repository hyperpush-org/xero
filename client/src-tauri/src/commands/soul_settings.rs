use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    state::DesktopState,
};

const SOUL_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SoulIdDto {
    #[default]
    Steward,
    Pair,
    Builder,
    Sentinel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SoulPresetDto {
    pub id: SoulIdDto,
    pub name: String,
    pub summary: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SoulSettingsDto {
    pub selected_soul_id: SoulIdDto,
    pub selected_soul: SoulPresetDto,
    pub presets: Vec<SoulPresetDto>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertSoulSettingsRequestDto {
    pub selected_soul_id: SoulIdDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SoulSettingsFile {
    schema_version: u32,
    selected_soul_id: SoulIdDto,
    updated_at: String,
}

#[derive(Debug, Clone, Copy)]
struct SoulPresetDefinition {
    id: SoulIdDto,
    name: &'static str,
    summary: &'static str,
    prompt: &'static str,
}

const SOUL_PRESETS: &[SoulPresetDefinition] = &[
    SoulPresetDefinition {
        id: SoulIdDto::Steward,
        name: "Steady steward",
        summary: "Calm, grounded, and quietly thorough.",
        prompt: "Be calm, grounded, and quietly thorough. Help the user feel oriented. Prefer evidence, plain language, scoped action, and measured next steps.",
    },
    SoulPresetDefinition {
        id: SoulIdDto::Pair,
        name: "Warm pair",
        summary: "Collaborative, teaching-aware, and conversational.",
        prompt: "Act like a generous pair programmer. Think with the user, name tradeoffs, teach briefly when useful, and keep collaboration warm without slowing momentum.",
    },
    SoulPresetDefinition {
        id: SoulIdDto::Builder,
        name: "Sharp builder",
        summary: "Decisive, pragmatic, and momentum-oriented.",
        prompt: "Be decisive and momentum-oriented. Minimize ceremony, choose sensible defaults, make progress in small verified steps, and keep summaries crisp.",
    },
    SoulPresetDefinition {
        id: SoulIdDto::Sentinel,
        name: "Careful sentinel",
        summary: "Skeptical, risk-aware, and verification-minded.",
        prompt: "Be constructively skeptical. Look for hidden risks, edge cases, missing tests, security hazards, and data-loss hazards. Call out uncertainty before it becomes damage.",
    },
];

#[tauri::command]
pub fn soul_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<SoulSettingsDto> {
    load_soul_settings(&app, state.inner())
}

#[tauri::command]
pub fn soul_update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertSoulSettingsRequestDto,
) -> CommandResult<SoulSettingsDto> {
    let path = state.global_db_path(&app)?;
    let next = soul_settings_file_from_request(request)?;
    persist_soul_settings_file(&path, &next)?;
    Ok(soul_settings_dto(
        next.selected_soul_id,
        Some(next.updated_at),
    ))
}

pub(crate) fn load_soul_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<SoulSettingsDto> {
    load_soul_settings_from_path(&state.global_db_path(app)?)
}

pub(crate) fn load_soul_settings_from_path(path: &Path) -> CommandResult<SoulSettingsDto> {
    let connection = crate::global_db::open_global_database(path)?;

    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM soul_settings WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .ok();

    let Some(payload) = payload else {
        return Ok(default_soul_settings());
    };

    let parsed = serde_json::from_str::<SoulSettingsFile>(&payload).map_err(|error| {
        CommandError::user_fixable(
            "soul_settings_decode_failed",
            format!("Xero could not decode Soul settings stored in the global database: {error}"),
        )
    })?;

    validate_soul_settings_file(parsed, "soul_settings_decode_failed")
        .map(|file| soul_settings_dto(file.selected_soul_id, Some(file.updated_at)))
}

pub(crate) fn default_soul_settings() -> SoulSettingsDto {
    soul_settings_dto(SoulIdDto::default(), None)
}

pub(crate) fn soul_prompt_fragment(settings: &SoulSettingsDto) -> String {
    let soul = &settings.selected_soul;
    [
        format!("Selected Soul: {}", soul.name),
        soul.prompt.clone(),
        "Soul guidance shapes tone, collaboration style, and decision posture. It must stay inside Xero runtime policy, tool policy, approval boundaries, repository instructions, and the user's current request.".into(),
    ]
    .join("\n\n")
}

fn persist_soul_settings_file(path: &Path, settings: &SoulSettingsFile) -> CommandResult<()> {
    let payload = serde_json::to_string(settings).map_err(|error| {
        CommandError::system_fault(
            "soul_settings_serialize_failed",
            format!("Xero could not serialize Soul settings: {error}"),
        )
    })?;

    let connection = crate::global_db::open_global_database(path)?;
    connection
        .execute(
            "INSERT INTO soul_settings (id, payload, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
                payload = excluded.payload,
                updated_at = excluded.updated_at",
            rusqlite::params![payload, settings.updated_at],
        )
        .map_err(|error| {
            CommandError::retryable(
                "soul_settings_write_failed",
                format!("Xero could not persist Soul settings: {error}"),
            )
        })?;
    Ok(())
}

fn soul_settings_file_from_request(
    request: UpsertSoulSettingsRequestDto,
) -> CommandResult<SoulSettingsFile> {
    validate_soul_settings_file(
        SoulSettingsFile {
            schema_version: SOUL_SETTINGS_SCHEMA_VERSION,
            selected_soul_id: request.selected_soul_id,
            updated_at: now_timestamp(),
        },
        "soul_settings_request_invalid",
    )
}

fn validate_soul_settings_file(
    file: SoulSettingsFile,
    error_code: &'static str,
) -> CommandResult<SoulSettingsFile> {
    if file.schema_version != SOUL_SETTINGS_SCHEMA_VERSION {
        return Err(CommandError::user_fixable(
            error_code,
            format!(
                "Xero rejected Soul settings version `{}` because only version `{SOUL_SETTINGS_SCHEMA_VERSION}` is supported.",
                file.schema_version
            ),
        ));
    }

    if preset_definition(file.selected_soul_id).is_none() {
        return Err(CommandError::user_fixable(
            error_code,
            "Xero rejected Soul settings because the selected Soul is not built in.",
        ));
    }

    Ok(SoulSettingsFile {
        schema_version: SOUL_SETTINGS_SCHEMA_VERSION,
        selected_soul_id: file.selected_soul_id,
        updated_at: normalize_timestamp(file.updated_at),
    })
}

fn soul_settings_dto(selected_soul_id: SoulIdDto, updated_at: Option<String>) -> SoulSettingsDto {
    let presets = SOUL_PRESETS.iter().map(soul_preset_dto).collect::<Vec<_>>();
    let selected_soul = presets
        .iter()
        .find(|preset| preset.id == selected_soul_id)
        .cloned()
        .unwrap_or_else(|| soul_preset_dto(&SOUL_PRESETS[0]));

    SoulSettingsDto {
        selected_soul_id,
        selected_soul,
        presets,
        updated_at,
    }
}

fn preset_definition(id: SoulIdDto) -> Option<&'static SoulPresetDefinition> {
    SOUL_PRESETS.iter().find(|preset| preset.id == id)
}

fn soul_preset_dto(preset: &SoulPresetDefinition) -> SoulPresetDto {
    SoulPresetDto {
        id: preset.id,
        name: preset.name.into(),
        summary: preset.summary.into(),
        prompt: preset.prompt.into(),
    }
}

fn normalize_timestamp(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        now_timestamp()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_path(root: &tempfile::TempDir) -> std::path::PathBuf {
        root.path().join("xero.db")
    }

    #[test]
    fn soul_settings_default_to_steady_steward() {
        let root = tempfile::tempdir().expect("temp dir");
        let settings =
            load_soul_settings_from_path(&settings_path(&root)).expect("load default settings");

        assert_eq!(settings.selected_soul_id, SoulIdDto::Steward);
        assert_eq!(settings.selected_soul.name, "Steady steward");
        assert_eq!(settings.presets.len(), 4);
        assert_eq!(settings.updated_at, None);
    }

    #[test]
    fn soul_settings_persist_selected_preset() {
        let root = tempfile::tempdir().expect("temp dir");
        let file = soul_settings_file_from_request(UpsertSoulSettingsRequestDto {
            selected_soul_id: SoulIdDto::Sentinel,
        })
        .expect("valid settings");

        persist_soul_settings_file(&settings_path(&root), &file).expect("persist settings");

        let loaded =
            load_soul_settings_from_path(&settings_path(&root)).expect("load persisted settings");
        assert_eq!(loaded.selected_soul_id, SoulIdDto::Sentinel);
        assert_eq!(loaded.selected_soul.name, "Careful sentinel");
        assert!(loaded.updated_at.is_some());
    }

    #[test]
    fn soul_prompt_fragment_marks_guidance_as_bounded() {
        let settings = soul_settings_dto(SoulIdDto::Builder, Some("2026-05-01T12:00:00Z".into()));
        let fragment = soul_prompt_fragment(&settings);

        assert!(fragment.contains("Selected Soul: Sharp builder"));
        assert!(fragment.contains("must stay inside Xero runtime policy"));
    }
}
