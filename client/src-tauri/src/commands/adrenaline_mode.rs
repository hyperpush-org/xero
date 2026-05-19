use std::{
    fmt,
    path::Path,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    state::DesktopState,
};

const ADRENALINE_MODE_SETTINGS_SCHEMA_VERSION: u32 = 1;
const CLOSED_LID_MODE_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AdrenalineModeAssertionKindDto {
    #[default]
    PreventIdleSystemSleep,
    PreventIdleDisplaySleep,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdrenalineModeActiveStatusDto {
    Active,
    Inactive,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdrenalineModeSettingsDto {
    pub enabled: bool,
    pub assertion_kind: AdrenalineModeAssertionKindDto,
    pub active: bool,
    pub active_status: AdrenalineModeActiveStatusDto,
    pub platform_supported: bool,
    pub updated_at: Option<String>,
    pub diagnostic_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertAdrenalineModeSettingsRequestDto {
    pub enabled: bool,
    pub assertion_kind: AdrenalineModeAssertionKindDto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClosedLidModeActiveStatusDto {
    Active,
    Inactive,
    NeedsAttention,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClosedLidModeSettingsDto {
    pub enabled: bool,
    pub active: bool,
    pub active_status: ClosedLidModeActiveStatusDto,
    pub platform_supported: bool,
    pub authorization_required: bool,
    pub current_disablesleep: Option<bool>,
    pub previous_disablesleep: Option<bool>,
    pub updated_at: Option<String>,
    pub diagnostic_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertClosedLidModeSettingsRequestDto {
    pub enabled: bool,
    pub acknowledge_global_power_change: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AdrenalineModeSettingsFile {
    schema_version: u32,
    enabled: bool,
    assertion_kind: AdrenalineModeAssertionKindDto,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClosedLidModeSettingsFile {
    schema_version: u32,
    enabled: bool,
    #[serde(default)]
    previous_disablesleep: Option<bool>,
    #[serde(default)]
    previous_lid_close_ac: Option<u32>,
    #[serde(default)]
    previous_lid_close_dc: Option<u32>,
    updated_at: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClosedLidPowerRestoreState {
    #[serde(default)]
    disablesleep: Option<bool>,
    #[serde(default)]
    lid_close_ac: Option<u32>,
    #[serde(default)]
    lid_close_dc: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClosedLidPowerSnapshot {
    active: Option<bool>,
    restore_state: ClosedLidPowerRestoreState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveAssertion {
    id: PowerAssertionId,
    kind: AdrenalineModeAssertionKindDto,
}

#[derive(Debug, Default)]
struct AdrenalineModeStateInner {
    active: Option<ActiveAssertion>,
    diagnostic_message: Option<String>,
}

type PowerAssertionId = u64;

trait PowerAssertionDriver: Send + Sync {
    fn platform_supported(&self) -> bool;
    fn acquire(&self, kind: AdrenalineModeAssertionKindDto) -> CommandResult<PowerAssertionId>;
    fn release(
        &self,
        id: PowerAssertionId,
        kind: AdrenalineModeAssertionKindDto,
    ) -> CommandResult<()>;
}

trait ClosedLidPowerDriver: Send + Sync {
    fn platform_supported(&self) -> bool;
    fn read_snapshot(&self) -> CommandResult<ClosedLidPowerSnapshot>;
    fn enable(&self) -> CommandResult<()>;
    fn restore(&self, restore_state: ClosedLidPowerRestoreState) -> CommandResult<()>;
}

#[derive(Clone)]
pub struct AdrenalineModeState {
    inner: Arc<Mutex<AdrenalineModeStateInner>>,
    driver: Arc<dyn PowerAssertionDriver>,
}

#[derive(Clone)]
pub struct ClosedLidModeState {
    driver: Arc<dyn ClosedLidPowerDriver>,
}

impl fmt::Debug for AdrenalineModeState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdrenalineModeState")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for ClosedLidModeState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClosedLidModeState")
            .finish_non_exhaustive()
    }
}

impl Default for AdrenalineModeState {
    fn default() -> Self {
        Self::with_driver(Arc::new(SystemPowerAssertionDriver))
    }
}

impl Default for ClosedLidModeState {
    fn default() -> Self {
        Self::with_driver(Arc::new(SystemClosedLidPowerDriver))
    }
}

impl AdrenalineModeState {
    fn with_driver(driver: Arc<dyn PowerAssertionDriver>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(AdrenalineModeStateInner::default())),
            driver,
        }
    }

    fn platform_supported(&self) -> bool {
        self.driver.platform_supported()
    }

    fn enable(&self, kind: AdrenalineModeAssertionKindDto) -> CommandResult<()> {
        if !self.platform_supported() {
            self.set_diagnostic(Some(
                "Adrenaline Mode is currently available on macOS and Windows only.".to_owned(),
            ))?;
            return Ok(());
        }

        let mut inner = self.inner.lock().map_err(|_| {
            CommandError::system_fault(
                "adrenaline_mode_state_unavailable",
                "Xero could not access Adrenaline Mode state.",
            )
        })?;

        if let Some(active) = inner.active {
            if active.kind == kind {
                inner.diagnostic_message = None;
                return Ok(());
            }

            inner.active = None;
            self.driver
                .release(active.id, active.kind)
                .inspect_err(|error| {
                    inner.active = Some(active);
                    inner.diagnostic_message = Some(error.message.clone());
                })?;
        }

        let id = self.driver.acquire(kind).inspect_err(|error| {
            inner.diagnostic_message = Some(error.message.clone());
        })?;
        inner.active = Some(ActiveAssertion { id, kind });
        inner.diagnostic_message = None;
        Ok(())
    }

    fn disable(&self) -> CommandResult<()> {
        let mut inner = self.inner.lock().map_err(|_| {
            CommandError::system_fault(
                "adrenaline_mode_state_unavailable",
                "Xero could not access Adrenaline Mode state.",
            )
        })?;

        let Some(active) = inner.active.take() else {
            if self.platform_supported() {
                inner.diagnostic_message = None;
            }
            return Ok(());
        };

        self.driver
            .release(active.id, active.kind)
            .inspect_err(|error| {
                inner.active = Some(active);
                inner.diagnostic_message = Some(error.message.clone());
            })?;
        inner.diagnostic_message = None;
        Ok(())
    }

    fn snapshot(
        &self,
        enabled: bool,
        assertion_kind: AdrenalineModeAssertionKindDto,
        updated_at: Option<String>,
    ) -> CommandResult<AdrenalineModeSettingsDto> {
        let inner = self.inner.lock().map_err(|_| {
            CommandError::system_fault(
                "adrenaline_mode_state_unavailable",
                "Xero could not access Adrenaline Mode state.",
            )
        })?;
        let platform_supported = self.platform_supported();
        let active = platform_supported
            && inner
                .active
                .is_some_and(|active| active.kind == assertion_kind);
        let active_status = if !platform_supported {
            AdrenalineModeActiveStatusDto::Unsupported
        } else if active {
            AdrenalineModeActiveStatusDto::Active
        } else {
            AdrenalineModeActiveStatusDto::Inactive
        };

        Ok(AdrenalineModeSettingsDto {
            enabled: enabled && platform_supported,
            assertion_kind,
            active,
            active_status,
            platform_supported,
            updated_at,
            diagnostic_message: inner.diagnostic_message.clone(),
        })
    }

    fn set_diagnostic(&self, diagnostic_message: Option<String>) -> CommandResult<()> {
        let mut inner = self.inner.lock().map_err(|_| {
            CommandError::system_fault(
                "adrenaline_mode_state_unavailable",
                "Xero could not access Adrenaline Mode state.",
            )
        })?;
        inner.diagnostic_message = diagnostic_message;
        Ok(())
    }
}

impl ClosedLidModeState {
    fn with_driver(driver: Arc<dyn ClosedLidPowerDriver>) -> Self {
        Self { driver }
    }

    fn platform_supported(&self) -> bool {
        self.driver.platform_supported()
    }

    fn read_snapshot(&self) -> CommandResult<ClosedLidPowerSnapshot> {
        self.driver.read_snapshot()
    }

    fn enable_closed_lid_mode(&self) -> CommandResult<()> {
        self.driver.enable()
    }

    fn restore_closed_lid_mode(
        &self,
        restore_state: ClosedLidPowerRestoreState,
    ) -> CommandResult<()> {
        self.driver.restore(restore_state)
    }

    fn snapshot(
        &self,
        file: &ClosedLidModeSettingsFile,
        updated_at: Option<String>,
    ) -> CommandResult<ClosedLidModeSettingsDto> {
        let platform_supported = self.platform_supported();
        let mut diagnostic_message = None;
        let current_snapshot = if platform_supported {
            match self.read_snapshot() {
                Ok(value) => value,
                Err(error) => {
                    diagnostic_message = Some(error.message);
                    ClosedLidPowerSnapshot {
                        active: None,
                        restore_state: file.restore_state(),
                    }
                }
            }
        } else {
            ClosedLidPowerSnapshot {
                active: None,
                restore_state: ClosedLidPowerRestoreState::default(),
            }
        };

        let active = current_snapshot.active == Some(true);
        let active_status = if !platform_supported {
            ClosedLidModeActiveStatusDto::Unsupported
        } else if file.enabled && active {
            ClosedLidModeActiveStatusDto::Active
        } else if !file.enabled && !active {
            ClosedLidModeActiveStatusDto::Inactive
        } else {
            ClosedLidModeActiveStatusDto::NeedsAttention
        };

        Ok(ClosedLidModeSettingsDto {
            enabled: file.enabled && platform_supported,
            active,
            active_status,
            platform_supported,
            authorization_required: platform_supported,
            current_disablesleep: current_snapshot.active,
            previous_disablesleep: file.previous_disablesleep,
            updated_at,
            diagnostic_message,
        })
    }
}

impl ClosedLidModeSettingsFile {
    fn restore_state(&self) -> ClosedLidPowerRestoreState {
        ClosedLidPowerRestoreState {
            disablesleep: self.previous_disablesleep,
            lid_close_ac: self.previous_lid_close_ac,
            lid_close_dc: self.previous_lid_close_dc,
        }
    }
}

#[derive(Debug)]
struct SystemPowerAssertionDriver;

#[derive(Debug)]
struct SystemClosedLidPowerDriver;

impl PowerAssertionDriver for SystemPowerAssertionDriver {
    fn platform_supported(&self) -> bool {
        platform::supported()
    }

    fn acquire(&self, kind: AdrenalineModeAssertionKindDto) -> CommandResult<PowerAssertionId> {
        platform::acquire(kind)
    }

    fn release(
        &self,
        id: PowerAssertionId,
        kind: AdrenalineModeAssertionKindDto,
    ) -> CommandResult<()> {
        platform::release(id, kind)
    }
}

impl ClosedLidPowerDriver for SystemClosedLidPowerDriver {
    fn platform_supported(&self) -> bool {
        closed_lid_platform::supported()
    }

    fn read_snapshot(&self) -> CommandResult<ClosedLidPowerSnapshot> {
        closed_lid_platform::read_snapshot()
    }

    fn enable(&self) -> CommandResult<()> {
        closed_lid_platform::enable()
    }

    fn restore(&self, restore_state: ClosedLidPowerRestoreState) -> CommandResult<()> {
        closed_lid_platform::restore(restore_state)
    }
}

#[tauri::command]
pub fn adrenaline_mode_settings<R: Runtime>(
    app: AppHandle<R>,
    desktop_state: State<'_, DesktopState>,
    adrenaline_state: State<'_, AdrenalineModeState>,
) -> CommandResult<AdrenalineModeSettingsDto> {
    load_adrenaline_mode_settings(&app, desktop_state.inner(), adrenaline_state.inner())
}

#[tauri::command]
pub fn adrenaline_mode_update_settings<R: Runtime>(
    app: AppHandle<R>,
    desktop_state: State<'_, DesktopState>,
    adrenaline_state: State<'_, AdrenalineModeState>,
    request: UpsertAdrenalineModeSettingsRequestDto,
) -> CommandResult<AdrenalineModeSettingsDto> {
    let path = desktop_state.global_db_path(&app)?;
    let next = adrenaline_mode_settings_file_from_request(request)?;

    if next.enabled {
        adrenaline_state.enable(next.assertion_kind)?;
        if let Err(error) = persist_adrenaline_mode_settings_file(&path, &next) {
            let _ = adrenaline_state.disable();
            return Err(error);
        }
    } else {
        adrenaline_state.disable()?;
        persist_adrenaline_mode_settings_file(&path, &next)?;
    }

    adrenaline_state.snapshot(next.enabled, next.assertion_kind, Some(next.updated_at))
}

#[tauri::command]
pub fn closed_lid_mode_settings<R: Runtime>(
    app: AppHandle<R>,
    desktop_state: State<'_, DesktopState>,
    closed_lid_state: State<'_, ClosedLidModeState>,
) -> CommandResult<ClosedLidModeSettingsDto> {
    load_closed_lid_mode_settings(&app, desktop_state.inner(), closed_lid_state.inner())
}

#[tauri::command]
pub fn closed_lid_mode_update_settings<R: Runtime>(
    app: AppHandle<R>,
    desktop_state: State<'_, DesktopState>,
    closed_lid_state: State<'_, ClosedLidModeState>,
    request: UpsertClosedLidModeSettingsRequestDto,
) -> CommandResult<ClosedLidModeSettingsDto> {
    let path = desktop_state.global_db_path(&app)?;
    update_closed_lid_mode_settings_from_path(&path, closed_lid_state.inner(), request)
}

pub(crate) fn load_adrenaline_mode_settings<R: Runtime>(
    app: &AppHandle<R>,
    desktop_state: &DesktopState,
    adrenaline_state: &AdrenalineModeState,
) -> CommandResult<AdrenalineModeSettingsDto> {
    load_adrenaline_mode_settings_from_path(&desktop_state.global_db_path(app)?, adrenaline_state)
}

pub(crate) fn load_closed_lid_mode_settings<R: Runtime>(
    app: &AppHandle<R>,
    desktop_state: &DesktopState,
    closed_lid_state: &ClosedLidModeState,
) -> CommandResult<ClosedLidModeSettingsDto> {
    load_closed_lid_mode_settings_from_path(&desktop_state.global_db_path(app)?, closed_lid_state)
}

pub(crate) fn apply_persisted_settings_on_startup<R: Runtime>(
    app: &AppHandle<R>,
) -> CommandResult<()> {
    let desktop_state = app.state::<DesktopState>();
    let adrenaline_state = app.state::<AdrenalineModeState>();
    let file = load_adrenaline_mode_settings_file_from_path(&desktop_state.global_db_path(app)?)?;

    if file.enabled {
        adrenaline_state.enable(file.assertion_kind)?;
    } else {
        adrenaline_state.disable()?;
    }

    Ok(())
}

pub fn shutdown_on_close<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<AdrenalineModeState>() else {
        return;
    };
    let _ = state.disable();
}

fn load_adrenaline_mode_settings_from_path(
    path: &Path,
    state: &AdrenalineModeState,
) -> CommandResult<AdrenalineModeSettingsDto> {
    let Some(file) = load_adrenaline_mode_settings_file_option_from_path(path)? else {
        return state.snapshot(
            false,
            AdrenalineModeAssertionKindDto::PreventIdleSystemSleep,
            None,
        );
    };

    state.snapshot(file.enabled, file.assertion_kind, Some(file.updated_at))
}

fn load_adrenaline_mode_settings_file_from_path(
    path: &Path,
) -> CommandResult<AdrenalineModeSettingsFile> {
    Ok(load_adrenaline_mode_settings_file_option_from_path(path)?
        .unwrap_or_else(default_adrenaline_mode_settings_file))
}

fn load_adrenaline_mode_settings_file_option_from_path(
    path: &Path,
) -> CommandResult<Option<AdrenalineModeSettingsFile>> {
    let connection = crate::global_db::open_global_database(path)?;

    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM adrenaline_mode_settings WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .ok();

    let Some(payload) = payload else {
        return Ok(None);
    };

    let parsed = serde_json::from_str::<AdrenalineModeSettingsFile>(&payload).map_err(|error| {
        CommandError::user_fixable(
            "adrenaline_mode_settings_decode_failed",
            format!(
                "Xero could not decode Adrenaline Mode settings stored in the global database: {error}"
            ),
        )
    })?;

    validate_adrenaline_mode_settings_file(parsed, "adrenaline_mode_settings_decode_failed")
        .map(Some)
}

fn persist_adrenaline_mode_settings_file(
    path: &Path,
    settings: &AdrenalineModeSettingsFile,
) -> CommandResult<()> {
    let payload = serde_json::to_string(settings).map_err(|error| {
        CommandError::system_fault(
            "adrenaline_mode_settings_serialize_failed",
            format!("Xero could not serialize Adrenaline Mode settings: {error}"),
        )
    })?;

    let connection = crate::global_db::open_global_database(path)?;
    connection
        .execute(
            "INSERT INTO adrenaline_mode_settings (id, payload, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
                payload = excluded.payload,
                updated_at = excluded.updated_at",
            rusqlite::params![payload, settings.updated_at],
        )
        .map_err(|error| {
            CommandError::retryable(
                "adrenaline_mode_settings_write_failed",
                format!("Xero could not persist Adrenaline Mode settings: {error}"),
            )
        })?;
    Ok(())
}

fn load_closed_lid_mode_settings_from_path(
    path: &Path,
    state: &ClosedLidModeState,
) -> CommandResult<ClosedLidModeSettingsDto> {
    let Some(file) = load_closed_lid_mode_settings_file_option_from_path(path)? else {
        return state.snapshot(&default_closed_lid_mode_settings_file(), None);
    };

    state.snapshot(&file, Some(file.updated_at.clone()))
}

fn load_closed_lid_mode_settings_file_from_path(
    path: &Path,
) -> CommandResult<ClosedLidModeSettingsFile> {
    Ok(load_closed_lid_mode_settings_file_option_from_path(path)?
        .unwrap_or_else(default_closed_lid_mode_settings_file))
}

fn load_closed_lid_mode_settings_file_option_from_path(
    path: &Path,
) -> CommandResult<Option<ClosedLidModeSettingsFile>> {
    let connection = crate::global_db::open_global_database(path)?;

    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM closed_lid_mode_settings WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .ok();

    let Some(payload) = payload else {
        return Ok(None);
    };

    let parsed = serde_json::from_str::<ClosedLidModeSettingsFile>(&payload).map_err(|error| {
        CommandError::user_fixable(
            "closed_lid_mode_settings_decode_failed",
            format!(
                "Xero could not decode Closed-Lid Mode settings stored in the global database: {error}"
            ),
        )
    })?;

    validate_closed_lid_mode_settings_file(parsed, "closed_lid_mode_settings_decode_failed")
        .map(Some)
}

fn persist_closed_lid_mode_settings_file(
    path: &Path,
    settings: &ClosedLidModeSettingsFile,
) -> CommandResult<()> {
    let payload = serde_json::to_string(settings).map_err(|error| {
        CommandError::system_fault(
            "closed_lid_mode_settings_serialize_failed",
            format!("Xero could not serialize Closed-Lid Mode settings: {error}"),
        )
    })?;

    let connection = crate::global_db::open_global_database(path)?;
    connection
        .execute(
            "INSERT INTO closed_lid_mode_settings (id, payload, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
                payload = excluded.payload,
                updated_at = excluded.updated_at",
            rusqlite::params![payload, settings.updated_at],
        )
        .map_err(|error| {
            CommandError::retryable(
                "closed_lid_mode_settings_write_failed",
                format!("Xero could not persist Closed-Lid Mode settings: {error}"),
            )
        })?;
    Ok(())
}

fn update_closed_lid_mode_settings_from_path(
    path: &Path,
    state: &ClosedLidModeState,
    request: UpsertClosedLidModeSettingsRequestDto,
) -> CommandResult<ClosedLidModeSettingsDto> {
    if !state.platform_supported() {
        return Err(CommandError::user_fixable(
            "closed_lid_mode_platform_unsupported",
            "Closed-Lid Mode is currently available on macOS and Windows only.",
        ));
    }

    if request.enabled && !request.acknowledge_global_power_change {
        return Err(CommandError::user_fixable(
            "closed_lid_mode_acknowledgement_required",
            "Closed-Lid Mode changes global system power settings and requires explicit acknowledgement.",
        ));
    }

    let current = state.read_snapshot()?;
    let previous_file = load_closed_lid_mode_settings_file_from_path(path)?;
    let next = closed_lid_mode_settings_file_from_request(
        &previous_file,
        &request,
        current.restore_state,
    )?;

    if request.enabled {
        if current.active != Some(true) {
            state.enable_closed_lid_mode()?;
        }

        if let Err(error) = persist_closed_lid_mode_settings_file(path, &next) {
            if current.active != Some(true) {
                let _ = state.restore_closed_lid_mode(current.restore_state);
            }
            return Err(error);
        }
    } else {
        let restore_state = previous_file.restore_state();
        if current.restore_state != restore_state {
            state.restore_closed_lid_mode(restore_state)?;
        }
        persist_closed_lid_mode_settings_file(path, &next)?;
    }

    state.snapshot(&next, Some(next.updated_at.clone()))
}

fn adrenaline_mode_settings_file_from_request(
    request: UpsertAdrenalineModeSettingsRequestDto,
) -> CommandResult<AdrenalineModeSettingsFile> {
    validate_adrenaline_mode_settings_file(
        AdrenalineModeSettingsFile {
            schema_version: ADRENALINE_MODE_SETTINGS_SCHEMA_VERSION,
            enabled: request.enabled,
            assertion_kind: request.assertion_kind,
            updated_at: now_timestamp(),
        },
        "adrenaline_mode_settings_request_invalid",
    )
}

fn closed_lid_mode_settings_file_from_request(
    previous: &ClosedLidModeSettingsFile,
    request: &UpsertClosedLidModeSettingsRequestDto,
    current_restore_state: ClosedLidPowerRestoreState,
) -> CommandResult<ClosedLidModeSettingsFile> {
    let previous_restore_state = if request.enabled {
        if previous.enabled {
            previous.restore_state()
        } else {
            current_restore_state
        }
    } else {
        ClosedLidPowerRestoreState::default()
    };

    validate_closed_lid_mode_settings_file(
        ClosedLidModeSettingsFile {
            schema_version: CLOSED_LID_MODE_SETTINGS_SCHEMA_VERSION,
            enabled: request.enabled,
            previous_disablesleep: previous_restore_state.disablesleep,
            previous_lid_close_ac: previous_restore_state.lid_close_ac,
            previous_lid_close_dc: previous_restore_state.lid_close_dc,
            updated_at: now_timestamp(),
        },
        "closed_lid_mode_settings_request_invalid",
    )
}

fn validate_adrenaline_mode_settings_file(
    file: AdrenalineModeSettingsFile,
    error_code: &'static str,
) -> CommandResult<AdrenalineModeSettingsFile> {
    if file.schema_version != ADRENALINE_MODE_SETTINGS_SCHEMA_VERSION {
        return Err(CommandError::user_fixable(
            error_code,
            format!(
                "Xero rejected Adrenaline Mode settings version `{}` because only version `{ADRENALINE_MODE_SETTINGS_SCHEMA_VERSION}` is supported.",
                file.schema_version
            ),
        ));
    }

    Ok(AdrenalineModeSettingsFile {
        schema_version: ADRENALINE_MODE_SETTINGS_SCHEMA_VERSION,
        enabled: file.enabled,
        assertion_kind: file.assertion_kind,
        updated_at: normalize_timestamp(file.updated_at),
    })
}

fn validate_closed_lid_mode_settings_file(
    file: ClosedLidModeSettingsFile,
    error_code: &'static str,
) -> CommandResult<ClosedLidModeSettingsFile> {
    if file.schema_version != CLOSED_LID_MODE_SETTINGS_SCHEMA_VERSION {
        return Err(CommandError::user_fixable(
            error_code,
            format!(
                "Xero rejected Closed-Lid Mode settings version `{}` because only version `{CLOSED_LID_MODE_SETTINGS_SCHEMA_VERSION}` is supported.",
                file.schema_version
            ),
        ));
    }

    Ok(ClosedLidModeSettingsFile {
        schema_version: CLOSED_LID_MODE_SETTINGS_SCHEMA_VERSION,
        enabled: file.enabled,
        previous_disablesleep: file.previous_disablesleep,
        previous_lid_close_ac: file.previous_lid_close_ac,
        previous_lid_close_dc: file.previous_lid_close_dc,
        updated_at: normalize_timestamp(file.updated_at),
    })
}

fn default_adrenaline_mode_settings_file() -> AdrenalineModeSettingsFile {
    AdrenalineModeSettingsFile {
        schema_version: ADRENALINE_MODE_SETTINGS_SCHEMA_VERSION,
        enabled: false,
        assertion_kind: AdrenalineModeAssertionKindDto::PreventIdleSystemSleep,
        updated_at: now_timestamp(),
    }
}

fn default_closed_lid_mode_settings_file() -> ClosedLidModeSettingsFile {
    ClosedLidModeSettingsFile {
        schema_version: CLOSED_LID_MODE_SETTINGS_SCHEMA_VERSION,
        enabled: false,
        previous_disablesleep: None,
        previous_lid_close_ac: None,
        previous_lid_close_dc: None,
        updated_at: now_timestamp(),
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

#[cfg(target_os = "macos")]
mod platform {
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};

    use super::{AdrenalineModeAssertionKindDto, CommandError, CommandResult, PowerAssertionId};

    const K_IO_RETURN_SUCCESS: i32 = 0;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOPMAssertionCreateWithDescription(
            assertion_type: CFStringRef,
            name: CFStringRef,
            details: CFStringRef,
            human_readable_reason: CFStringRef,
            localization_bundle_path: CFStringRef,
            timeout: f64,
            timeout_action: CFStringRef,
            assertion_id: *mut u32,
        ) -> i32;

        fn IOPMAssertionRelease(assertion_id: u32) -> i32;
    }

    pub(super) fn supported() -> bool {
        true
    }

    pub(super) fn acquire(kind: AdrenalineModeAssertionKindDto) -> CommandResult<PowerAssertionId> {
        let assertion_type = CFString::new(assertion_type_value(kind));
        let name = CFString::new("Xero Adrenaline Mode");
        let details = CFString::new("Xero is keeping this Mac awake while the app is running.");
        let reason = CFString::new("Adrenaline Mode is enabled in Xero settings.");
        let mut assertion_id = 0u32;

        let result = unsafe {
            IOPMAssertionCreateWithDescription(
                assertion_type.as_concrete_TypeRef(),
                name.as_concrete_TypeRef(),
                details.as_concrete_TypeRef(),
                reason.as_concrete_TypeRef(),
                std::ptr::null(),
                0.0,
                std::ptr::null(),
                &mut assertion_id,
            )
        };

        if result == K_IO_RETURN_SUCCESS {
            return Ok(u64::from(assertion_id));
        }

        Err(CommandError::retryable(
            "adrenaline_mode_assertion_create_failed",
            format!("Xero could not activate Adrenaline Mode. IOKit returned {result}."),
        ))
    }

    pub(super) fn release(
        id: PowerAssertionId,
        _kind: AdrenalineModeAssertionKindDto,
    ) -> CommandResult<()> {
        let id = u32::try_from(id).map_err(|_| {
            CommandError::system_fault(
                "adrenaline_mode_assertion_release_failed",
                format!("Xero could not release invalid Adrenaline Mode assertion {id}."),
            )
        })?;
        let result = unsafe { IOPMAssertionRelease(id) };
        if result == K_IO_RETURN_SUCCESS {
            return Ok(());
        }

        Err(CommandError::retryable(
            "adrenaline_mode_assertion_release_failed",
            format!(
                "Xero could not release Adrenaline Mode assertion {id}. IOKit returned {result}."
            ),
        ))
    }

    fn assertion_type_value(kind: AdrenalineModeAssertionKindDto) -> &'static str {
        match kind {
            AdrenalineModeAssertionKindDto::PreventIdleSystemSleep => "NoIdleSleepAssertion",
            AdrenalineModeAssertionKindDto::PreventIdleDisplaySleep => "NoDisplaySleepAssertion",
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::ffi::c_void;

    use super::{AdrenalineModeAssertionKindDto, CommandError, CommandResult, PowerAssertionId};

    type Bool = i32;
    type Dword = u32;
    type Handle = *mut c_void;

    const POWER_REQUEST_CONTEXT_VERSION: Dword = 0;
    const POWER_REQUEST_CONTEXT_SIMPLE_STRING: Dword = 0x1;
    const POWER_REQUEST_DISPLAY_REQUIRED: Dword = 0;
    const POWER_REQUEST_SYSTEM_REQUIRED: Dword = 1;
    const INVALID_HANDLE_VALUE: isize = -1;

    #[repr(C)]
    struct ReasonContext {
        version: Dword,
        flags: Dword,
        reason: ReasonContextUnion,
    }

    #[repr(C)]
    union ReasonContextUnion {
        simple_reason_string: *mut u16,
        detailed: ReasonContextDetailed,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct ReasonContextDetailed {
        localized_reason_module: Handle,
        localized_reason_id: Dword,
        reason_string_count: Dword,
        reason_strings: *mut *mut u16,
    }

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn PowerCreateRequest(context: *mut ReasonContext) -> Handle;
        fn PowerSetRequest(power_request: Handle, request_type: Dword) -> Bool;
        fn PowerClearRequest(power_request: Handle, request_type: Dword) -> Bool;
        fn CloseHandle(handle: Handle) -> Bool;
        fn GetLastError() -> Dword;
    }

    pub(super) fn supported() -> bool {
        true
    }

    pub(super) fn acquire(kind: AdrenalineModeAssertionKindDto) -> CommandResult<PowerAssertionId> {
        let mut reason = wide_string("Xero Adrenaline Mode is enabled.");
        let mut context = ReasonContext {
            version: POWER_REQUEST_CONTEXT_VERSION,
            flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            reason: ReasonContextUnion {
                simple_reason_string: reason.as_mut_ptr(),
            },
        };

        let handle = unsafe { PowerCreateRequest(&mut context) };
        if handle.is_null() || handle as isize == INVALID_HANDLE_VALUE {
            return Err(last_windows_error(
                "adrenaline_mode_power_request_create_failed",
                "Xero could not create a Windows power request for Adrenaline Mode.",
            ));
        }

        if !set_request(handle, POWER_REQUEST_SYSTEM_REQUIRED) {
            let error = last_windows_error(
                "adrenaline_mode_power_request_set_failed",
                "Xero could not keep Windows awake for Adrenaline Mode.",
            );
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(error);
        }

        if kind == AdrenalineModeAssertionKindDto::PreventIdleDisplaySleep
            && !set_request(handle, POWER_REQUEST_DISPLAY_REQUIRED)
        {
            let error = last_windows_error(
                "adrenaline_mode_power_request_set_failed",
                "Xero could not keep the Windows display awake for Adrenaline Mode.",
            );
            unsafe {
                let _ = PowerClearRequest(handle, POWER_REQUEST_SYSTEM_REQUIRED);
                let _ = CloseHandle(handle);
            }
            return Err(error);
        }

        Ok(handle as usize as u64)
    }

    pub(super) fn release(
        id: PowerAssertionId,
        kind: AdrenalineModeAssertionKindDto,
    ) -> CommandResult<()> {
        let handle = id as usize as Handle;
        if handle.is_null() {
            return Ok(());
        }

        let mut failed = None;
        if kind == AdrenalineModeAssertionKindDto::PreventIdleDisplaySleep
            && !clear_request(handle, POWER_REQUEST_DISPLAY_REQUIRED)
        {
            failed = Some(last_windows_error(
                "adrenaline_mode_power_request_release_failed",
                "Xero could not release the Windows display power request.",
            ));
        }
        if !clear_request(handle, POWER_REQUEST_SYSTEM_REQUIRED) && failed.is_none() {
            failed = Some(last_windows_error(
                "adrenaline_mode_power_request_release_failed",
                "Xero could not release the Windows system power request.",
            ));
        }

        unsafe {
            let _ = CloseHandle(handle);
        }

        if let Some(error) = failed {
            return Err(error);
        }
        Ok(())
    }

    fn set_request(handle: Handle, request_type: Dword) -> bool {
        unsafe { PowerSetRequest(handle, request_type) != 0 }
    }

    fn clear_request(handle: Handle, request_type: Dword) -> bool {
        unsafe { PowerClearRequest(handle, request_type) != 0 }
    }

    fn wide_string(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn last_windows_error(code: &'static str, message: &'static str) -> CommandError {
        let error_code = unsafe { GetLastError() };
        CommandError::retryable(code, format!("{message} Windows error {error_code}."))
    }

    #[cfg(test)]
    mod tests {
        use super::wide_string;

        #[test]
        fn wide_string_is_null_terminated() {
            let encoded = wide_string("Xero");
            assert_eq!(encoded.last(), Some(&0));
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::{AdrenalineModeAssertionKindDto, CommandError, CommandResult, PowerAssertionId};

    pub(super) fn supported() -> bool {
        false
    }

    pub(super) fn acquire(
        _kind: AdrenalineModeAssertionKindDto,
    ) -> CommandResult<PowerAssertionId> {
        Err(CommandError::user_fixable(
            "adrenaline_mode_platform_unsupported",
            "Adrenaline Mode is currently available on macOS and Windows only.",
        ))
    }

    pub(super) fn release(
        _id: PowerAssertionId,
        _kind: AdrenalineModeAssertionKindDto,
    ) -> CommandResult<()> {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod closed_lid_platform {
    use std::process::Command;

    use super::{ClosedLidPowerRestoreState, ClosedLidPowerSnapshot, CommandError, CommandResult};

    pub(super) fn supported() -> bool {
        true
    }

    pub(super) fn read_snapshot() -> CommandResult<ClosedLidPowerSnapshot> {
        let live = run_pmset_read(["-g", "live"])?;
        if let Some(value) = parse_disablesleep_value(&live) {
            return Ok(snapshot(value));
        }

        let custom = run_pmset_read(["-g", "custom"])?;
        Ok(snapshot(parse_disablesleep_value(&custom).unwrap_or(false)))
    }

    pub(super) fn enable() -> CommandResult<()> {
        set_disablesleep(true)
    }

    pub(super) fn restore(restore_state: ClosedLidPowerRestoreState) -> CommandResult<()> {
        set_disablesleep(restore_state.disablesleep.unwrap_or(false))
    }

    fn set_disablesleep(enabled: bool) -> CommandResult<()> {
        let value = if enabled { "1" } else { "0" };
        let shell_command = format!("/usr/bin/pmset -a disablesleep {value}");
        let script = format!("do shell script \"{shell_command}\" with administrator privileges");
        let output = Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(script)
            .output()
            .map_err(|error| {
                CommandError::retryable(
                    "closed_lid_mode_authorization_failed",
                    format!("Xero could not ask macOS to update Closed-Lid Mode: {error}"),
                )
            })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("osascript exited with {}", output.status)
        };
        let code = if detail.contains("-128") || detail.to_ascii_lowercase().contains("canceled") {
            "closed_lid_mode_authorization_cancelled"
        } else {
            "closed_lid_mode_pmset_failed"
        };

        Err(CommandError::retryable(
            code,
            format!("macOS did not apply Closed-Lid Mode: {detail}"),
        ))
    }

    fn snapshot(value: bool) -> ClosedLidPowerSnapshot {
        ClosedLidPowerSnapshot {
            active: Some(value),
            restore_state: ClosedLidPowerRestoreState {
                disablesleep: Some(value),
                lid_close_ac: None,
                lid_close_dc: None,
            },
        }
    }

    fn run_pmset_read<const N: usize>(args: [&str; N]) -> CommandResult<String> {
        let output = Command::new("/usr/bin/pmset")
            .args(args)
            .output()
            .map_err(|error| {
                CommandError::retryable(
                    "closed_lid_mode_pmset_read_failed",
                    format!("Xero could not read macOS power settings: {error}"),
                )
            })?;

        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(CommandError::retryable(
            "closed_lid_mode_pmset_read_failed",
            format!("Xero could not read macOS power settings: {stderr}"),
        ))
    }

    fn parse_disablesleep_value(output: &str) -> Option<bool> {
        output.lines().find_map(|line| {
            let mut parts = line.split_whitespace();
            let key = parts.next()?;
            if key != "disablesleep" && key != "SleepDisabled" {
                return None;
            }

            match parts.next()? {
                "1" => Some(true),
                "0" => Some(false),
                _ => None,
            }
        })
    }

    #[cfg(test)]
    mod tests {
        use super::parse_disablesleep_value;

        #[test]
        fn parses_disablesleep_from_pmset_output() {
            assert_eq!(
                parse_disablesleep_value(" sleep 1\n disablesleep        1\n"),
                Some(true)
            );
            assert_eq!(
                parse_disablesleep_value("disablesleep 0 (Sleep disabled)\n"),
                Some(false)
            );
            assert_eq!(
                parse_disablesleep_value(" SleepDisabled\t\t1\n"),
                Some(true)
            );
            assert_eq!(parse_disablesleep_value("sleep 1\n"), None);
        }
    }
}

#[cfg(target_os = "windows")]
mod closed_lid_platform {
    use std::process::Command;

    use super::{ClosedLidPowerRestoreState, ClosedLidPowerSnapshot, CommandError, CommandResult};

    const LID_ACTION_DO_NOTHING: u32 = 0;
    const LID_ACTION_SLEEP: u32 = 1;

    pub(super) fn supported() -> bool {
        true
    }

    pub(super) fn read_snapshot() -> CommandResult<ClosedLidPowerSnapshot> {
        let output = run_powercfg(["/q", "SCHEME_CURRENT", "SUB_BUTTONS", "LIDACTION"])?;
        let ac = parse_powercfg_setting_value(&output, "Current AC Power Setting Index");
        let dc = parse_powercfg_setting_value(&output, "Current DC Power Setting Index");
        let active = match (ac, dc) {
            (Some(ac), Some(dc)) => {
                Some(ac == LID_ACTION_DO_NOTHING && dc == LID_ACTION_DO_NOTHING)
            }
            _ => None,
        };

        Ok(ClosedLidPowerSnapshot {
            active,
            restore_state: ClosedLidPowerRestoreState {
                disablesleep: None,
                lid_close_ac: ac,
                lid_close_dc: dc,
            },
        })
    }

    pub(super) fn enable() -> CommandResult<()> {
        set_lid_close_actions(LID_ACTION_DO_NOTHING, LID_ACTION_DO_NOTHING)
    }

    pub(super) fn restore(restore_state: ClosedLidPowerRestoreState) -> CommandResult<()> {
        set_lid_close_actions(
            restore_state.lid_close_ac.unwrap_or(LID_ACTION_SLEEP),
            restore_state.lid_close_dc.unwrap_or(LID_ACTION_SLEEP),
        )
    }

    fn set_lid_close_actions(ac: u32, dc: u32) -> CommandResult<()> {
        let ac = ac.to_string();
        let dc = dc.to_string();
        run_powercfg_status([
            "/setacvalueindex",
            "SCHEME_CURRENT",
            "SUB_BUTTONS",
            "LIDACTION",
            &ac,
        ])?;
        run_powercfg_status([
            "/setdcvalueindex",
            "SCHEME_CURRENT",
            "SUB_BUTTONS",
            "LIDACTION",
            &dc,
        ])?;
        run_powercfg_status(["/setactive", "SCHEME_CURRENT"])
    }

    fn run_powercfg<const N: usize>(args: [&str; N]) -> CommandResult<String> {
        let output = Command::new("powercfg")
            .args(args)
            .output()
            .map_err(|error| {
                CommandError::retryable(
                    "closed_lid_mode_powercfg_failed",
                    format!("Xero could not run powercfg for Closed-Lid Mode: {error}"),
                )
            })?;

        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(CommandError::retryable(
            "closed_lid_mode_powercfg_failed",
            format!("Windows did not report Closed-Lid Mode power settings: {stderr}"),
        ))
    }

    fn run_powercfg_status<const N: usize>(args: [&str; N]) -> CommandResult<()> {
        run_powercfg(args).map(|_| ())
    }

    fn parse_powercfg_setting_value(output: &str, label: &str) -> Option<u32> {
        output.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            if key.trim() != label {
                return None;
            }

            let trimmed = value.trim();
            let without_prefix = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
                .unwrap_or(trimmed);
            u32::from_str_radix(without_prefix, 16).ok()
        })
    }

    #[cfg(test)]
    mod tests {
        use super::parse_powercfg_setting_value;

        #[test]
        fn parses_windows_powercfg_lid_action_values() {
            let output = "\
                Current AC Power Setting Index: 0x00000000\n\
                Current DC Power Setting Index: 0x00000001\n";
            assert_eq!(
                parse_powercfg_setting_value(output, "Current AC Power Setting Index"),
                Some(0)
            );
            assert_eq!(
                parse_powercfg_setting_value(output, "Current DC Power Setting Index"),
                Some(1)
            );
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod closed_lid_platform {
    use super::{ClosedLidPowerRestoreState, ClosedLidPowerSnapshot, CommandError, CommandResult};

    pub(super) fn supported() -> bool {
        false
    }

    pub(super) fn read_snapshot() -> CommandResult<ClosedLidPowerSnapshot> {
        Ok(ClosedLidPowerSnapshot {
            active: None,
            restore_state: ClosedLidPowerRestoreState::default(),
        })
    }

    pub(super) fn enable() -> CommandResult<()> {
        Err(CommandError::user_fixable(
            "closed_lid_mode_platform_unsupported",
            "Closed-Lid Mode is currently available on macOS and Windows only.",
        ))
    }

    pub(super) fn restore(_restore_state: ClosedLidPowerRestoreState) -> CommandResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    };

    #[derive(Debug)]
    struct FakePowerAssertionDriver {
        supported: bool,
        next_id: AtomicU64,
        events: Mutex<Vec<String>>,
    }

    #[derive(Debug)]
    struct FakeClosedLidPowerDriver {
        supported: bool,
        current_disablesleep: Mutex<Option<bool>>,
        fail_set: Mutex<Option<CommandError>>,
        events: Mutex<Vec<String>>,
    }

    impl FakePowerAssertionDriver {
        fn new(supported: bool) -> Arc<Self> {
            Arc::new(Self {
                supported,
                next_id: AtomicU64::new(1),
                events: Mutex::new(Vec::new()),
            })
        }

        fn events(&self) -> Vec<String> {
            self.events.lock().expect("events mutex").clone()
        }
    }

    impl PowerAssertionDriver for FakePowerAssertionDriver {
        fn platform_supported(&self) -> bool {
            self.supported
        }

        fn acquire(&self, kind: AdrenalineModeAssertionKindDto) -> CommandResult<PowerAssertionId> {
            let id = self.next_id.fetch_add(1, Ordering::SeqCst);
            self.events
                .lock()
                .expect("events mutex")
                .push(format!("acquire:{kind:?}:{id}"));
            Ok(id)
        }

        fn release(
            &self,
            id: PowerAssertionId,
            _kind: AdrenalineModeAssertionKindDto,
        ) -> CommandResult<()> {
            self.events
                .lock()
                .expect("events mutex")
                .push(format!("release:{id}"));
            Ok(())
        }
    }

    impl FakeClosedLidPowerDriver {
        fn new(supported: bool, current_disablesleep: Option<bool>) -> Arc<Self> {
            Arc::new(Self {
                supported,
                current_disablesleep: Mutex::new(current_disablesleep),
                fail_set: Mutex::new(None),
                events: Mutex::new(Vec::new()),
            })
        }

        fn fail_next_set(&self, error: CommandError) {
            *self.fail_set.lock().expect("fail_set mutex") = Some(error);
        }

        fn events(&self) -> Vec<String> {
            self.events.lock().expect("events mutex").clone()
        }
    }

    impl ClosedLidPowerDriver for FakeClosedLidPowerDriver {
        fn platform_supported(&self) -> bool {
            self.supported
        }

        fn read_snapshot(&self) -> CommandResult<ClosedLidPowerSnapshot> {
            let current = *self
                .current_disablesleep
                .lock()
                .expect("current_disablesleep mutex");
            Ok(ClosedLidPowerSnapshot {
                active: current,
                restore_state: ClosedLidPowerRestoreState {
                    disablesleep: current,
                    lid_close_ac: None,
                    lid_close_dc: None,
                },
            })
        }

        fn enable(&self) -> CommandResult<()> {
            if let Some(error) = self.fail_set.lock().expect("fail_set mutex").take() {
                return Err(error);
            }

            self.events
                .lock()
                .expect("events mutex")
                .push("set:true".to_owned());
            *self
                .current_disablesleep
                .lock()
                .expect("current_disablesleep mutex") = Some(true);
            Ok(())
        }

        fn restore(&self, restore_state: ClosedLidPowerRestoreState) -> CommandResult<()> {
            if let Some(error) = self.fail_set.lock().expect("fail_set mutex").take() {
                return Err(error);
            }

            let enabled = restore_state.disablesleep.unwrap_or(false);
            self.events
                .lock()
                .expect("events mutex")
                .push(format!("set:{enabled}"));
            *self
                .current_disablesleep
                .lock()
                .expect("current_disablesleep mutex") = Some(enabled);
            Ok(())
        }
    }

    fn settings_path(root: &tempfile::TempDir) -> std::path::PathBuf {
        root.path().join("xero.db")
    }

    #[test]
    fn adrenaline_mode_settings_default_to_disabled_when_no_row_exists() {
        let root = tempfile::tempdir().expect("temp dir");
        let driver = FakePowerAssertionDriver::new(true);
        let state = AdrenalineModeState::with_driver(driver);
        let settings = load_adrenaline_mode_settings_from_path(&settings_path(&root), &state)
            .expect("load default Adrenaline Mode settings");

        assert!(!settings.enabled);
        assert!(!settings.active);
        assert_eq!(
            settings.assertion_kind,
            AdrenalineModeAssertionKindDto::PreventIdleSystemSleep
        );
        assert_eq!(
            settings.active_status,
            AdrenalineModeActiveStatusDto::Inactive
        );
        assert!(settings.platform_supported);
        assert_eq!(settings.updated_at, None);
    }

    #[test]
    fn adrenaline_mode_settings_persist_and_round_trip() {
        let root = tempfile::tempdir().expect("temp dir");
        let path = settings_path(&root);
        let file =
            adrenaline_mode_settings_file_from_request(UpsertAdrenalineModeSettingsRequestDto {
                enabled: true,
                assertion_kind: AdrenalineModeAssertionKindDto::PreventIdleDisplaySleep,
            })
            .expect("valid Adrenaline Mode settings");

        persist_adrenaline_mode_settings_file(&path, &file)
            .expect("persist Adrenaline Mode settings");

        let loaded = load_adrenaline_mode_settings_file_from_path(&path)
            .expect("load persisted Adrenaline Mode settings");
        assert!(loaded.enabled);
        assert_eq!(
            loaded.assertion_kind,
            AdrenalineModeAssertionKindDto::PreventIdleDisplaySleep
        );
    }

    #[test]
    fn adrenaline_mode_settings_reject_unknown_schema_version() {
        let error = validate_adrenaline_mode_settings_file(
            AdrenalineModeSettingsFile {
                schema_version: ADRENALINE_MODE_SETTINGS_SCHEMA_VERSION + 1,
                enabled: true,
                assertion_kind: AdrenalineModeAssertionKindDto::PreventIdleSystemSleep,
                updated_at: "2026-05-18T12:00:00Z".into(),
            },
            "adrenaline_mode_settings_decode_failed",
        )
        .expect_err("unsupported schema version should fail closed");

        assert_eq!(error.code, "adrenaline_mode_settings_decode_failed");
    }

    #[test]
    fn adrenaline_mode_enable_is_idempotent() {
        let driver = FakePowerAssertionDriver::new(true);
        let state = AdrenalineModeState::with_driver(driver.clone());

        state
            .enable(AdrenalineModeAssertionKindDto::PreventIdleSystemSleep)
            .expect("enable Adrenaline Mode");
        state
            .enable(AdrenalineModeAssertionKindDto::PreventIdleSystemSleep)
            .expect("enable Adrenaline Mode again");

        assert_eq!(
            driver.events(),
            vec!["acquire:PreventIdleSystemSleep:1".to_owned()]
        );
    }

    #[test]
    fn adrenaline_mode_disable_is_idempotent() {
        let driver = FakePowerAssertionDriver::new(true);
        let state = AdrenalineModeState::with_driver(driver.clone());

        state
            .enable(AdrenalineModeAssertionKindDto::PreventIdleSystemSleep)
            .expect("enable Adrenaline Mode");
        state.disable().expect("disable Adrenaline Mode");
        state.disable().expect("disable Adrenaline Mode again");

        assert_eq!(
            driver.events(),
            vec![
                "acquire:PreventIdleSystemSleep:1".to_owned(),
                "release:1".to_owned()
            ]
        );
    }

    #[test]
    fn adrenaline_mode_switching_assertion_kind_releases_old_before_acquiring_new() {
        let driver = FakePowerAssertionDriver::new(true);
        let state = AdrenalineModeState::with_driver(driver.clone());

        state
            .enable(AdrenalineModeAssertionKindDto::PreventIdleSystemSleep)
            .expect("enable idle sleep assertion");
        state
            .enable(AdrenalineModeAssertionKindDto::PreventIdleDisplaySleep)
            .expect("switch to display sleep assertion");

        assert_eq!(
            driver.events(),
            vec![
                "acquire:PreventIdleSystemSleep:1".to_owned(),
                "release:1".to_owned(),
                "acquire:PreventIdleDisplaySleep:2".to_owned()
            ]
        );
    }

    #[test]
    fn adrenaline_mode_unsupported_platform_reports_unsupported_without_panicking() {
        let root = tempfile::tempdir().expect("temp dir");
        let driver = FakePowerAssertionDriver::new(false);
        let state = AdrenalineModeState::with_driver(driver);
        let settings = load_adrenaline_mode_settings_from_path(&settings_path(&root), &state)
            .expect("load unsupported Adrenaline Mode settings");

        assert!(!settings.enabled);
        assert!(!settings.active);
        assert!(!settings.platform_supported);
        assert_eq!(
            settings.active_status,
            AdrenalineModeActiveStatusDto::Unsupported
        );
    }

    #[test]
    fn closed_lid_mode_settings_default_to_disabled() {
        let root = tempfile::tempdir().expect("temp dir");
        let driver = FakeClosedLidPowerDriver::new(true, Some(false));
        let state = ClosedLidModeState::with_driver(driver);
        let settings = load_closed_lid_mode_settings_from_path(&settings_path(&root), &state)
            .expect("load default Closed-Lid Mode settings");

        assert!(!settings.enabled);
        assert!(!settings.active);
        assert_eq!(
            settings.active_status,
            ClosedLidModeActiveStatusDto::Inactive
        );
        assert!(settings.platform_supported);
        assert!(settings.authorization_required);
        assert_eq!(settings.current_disablesleep, Some(false));
        assert_eq!(settings.previous_disablesleep, None);
    }

    #[test]
    fn closed_lid_mode_enable_requires_acknowledgement() {
        let root = tempfile::tempdir().expect("temp dir");
        let driver = FakeClosedLidPowerDriver::new(true, Some(false));
        let state = ClosedLidModeState::with_driver(driver);
        let error = update_closed_lid_mode_settings_from_path(
            &settings_path(&root),
            &state,
            UpsertClosedLidModeSettingsRequestDto {
                enabled: true,
                acknowledge_global_power_change: false,
            },
        )
        .expect_err("enabling Closed-Lid Mode without acknowledgement should fail");

        assert_eq!(error.code, "closed_lid_mode_acknowledgement_required");
    }

    #[test]
    fn closed_lid_mode_enable_persists_previous_global_value() {
        let root = tempfile::tempdir().expect("temp dir");
        let driver = FakeClosedLidPowerDriver::new(true, Some(false));
        let state = ClosedLidModeState::with_driver(driver.clone());
        let settings = update_closed_lid_mode_settings_from_path(
            &settings_path(&root),
            &state,
            UpsertClosedLidModeSettingsRequestDto {
                enabled: true,
                acknowledge_global_power_change: true,
            },
        )
        .expect("enable Closed-Lid Mode");

        assert!(settings.enabled);
        assert!(settings.active);
        assert_eq!(settings.active_status, ClosedLidModeActiveStatusDto::Active);
        assert_eq!(settings.previous_disablesleep, Some(false));
        assert_eq!(driver.events(), vec!["set:true".to_owned()]);

        let file = load_closed_lid_mode_settings_file_from_path(&settings_path(&root))
            .expect("load persisted Closed-Lid Mode settings");
        assert!(file.enabled);
        assert_eq!(file.previous_disablesleep, Some(false));
        assert_eq!(file.previous_lid_close_ac, None);
        assert_eq!(file.previous_lid_close_dc, None);
    }

    #[test]
    fn closed_lid_mode_file_persists_windows_lid_action_restore_values() {
        let previous = default_closed_lid_mode_settings_file();
        let current_restore_state = ClosedLidPowerRestoreState {
            disablesleep: None,
            lid_close_ac: Some(1),
            lid_close_dc: Some(2),
        };

        let enabled = closed_lid_mode_settings_file_from_request(
            &previous,
            &UpsertClosedLidModeSettingsRequestDto {
                enabled: true,
                acknowledge_global_power_change: true,
            },
            current_restore_state,
        )
        .expect("valid Closed-Lid Mode settings");

        assert!(enabled.enabled);
        assert_eq!(enabled.previous_disablesleep, None);
        assert_eq!(enabled.previous_lid_close_ac, Some(1));
        assert_eq!(enabled.previous_lid_close_dc, Some(2));

        let disabled = closed_lid_mode_settings_file_from_request(
            &enabled,
            &UpsertClosedLidModeSettingsRequestDto {
                enabled: false,
                acknowledge_global_power_change: false,
            },
            ClosedLidPowerRestoreState {
                disablesleep: None,
                lid_close_ac: Some(0),
                lid_close_dc: Some(0),
            },
        )
        .expect("valid Closed-Lid Mode settings");

        assert!(!disabled.enabled);
        assert_eq!(disabled.previous_disablesleep, None);
        assert_eq!(disabled.previous_lid_close_ac, None);
        assert_eq!(disabled.previous_lid_close_dc, None);
    }

    #[test]
    fn closed_lid_mode_disable_restores_previous_global_value() {
        let root = tempfile::tempdir().expect("temp dir");
        let driver = FakeClosedLidPowerDriver::new(true, Some(false));
        let state = ClosedLidModeState::with_driver(driver.clone());
        update_closed_lid_mode_settings_from_path(
            &settings_path(&root),
            &state,
            UpsertClosedLidModeSettingsRequestDto {
                enabled: true,
                acknowledge_global_power_change: true,
            },
        )
        .expect("enable Closed-Lid Mode");

        let settings = update_closed_lid_mode_settings_from_path(
            &settings_path(&root),
            &state,
            UpsertClosedLidModeSettingsRequestDto {
                enabled: false,
                acknowledge_global_power_change: false,
            },
        )
        .expect("disable Closed-Lid Mode");

        assert!(!settings.enabled);
        assert!(!settings.active);
        assert_eq!(
            settings.active_status,
            ClosedLidModeActiveStatusDto::Inactive
        );
        assert_eq!(settings.previous_disablesleep, None);
        assert_eq!(
            driver.events(),
            vec!["set:true".to_owned(), "set:false".to_owned()]
        );
    }

    #[test]
    fn closed_lid_mode_preserves_preexisting_enabled_global_value() {
        let root = tempfile::tempdir().expect("temp dir");
        let driver = FakeClosedLidPowerDriver::new(true, Some(true));
        let state = ClosedLidModeState::with_driver(driver.clone());

        let enabled = update_closed_lid_mode_settings_from_path(
            &settings_path(&root),
            &state,
            UpsertClosedLidModeSettingsRequestDto {
                enabled: true,
                acknowledge_global_power_change: true,
            },
        )
        .expect("enable Closed-Lid Mode");
        assert_eq!(enabled.previous_disablesleep, Some(true));
        assert!(driver.events().is_empty());

        let disabled = update_closed_lid_mode_settings_from_path(
            &settings_path(&root),
            &state,
            UpsertClosedLidModeSettingsRequestDto {
                enabled: false,
                acknowledge_global_power_change: false,
            },
        )
        .expect("disable Closed-Lid Mode");
        assert!(disabled.active);
        assert_eq!(
            disabled.active_status,
            ClosedLidModeActiveStatusDto::NeedsAttention
        );
        assert!(driver.events().is_empty());
    }

    #[test]
    fn closed_lid_mode_surfaces_pmset_failures_without_persisting() {
        let root = tempfile::tempdir().expect("temp dir");
        let path = root.path().join("missing-parent").join("xero.db");
        let driver = FakeClosedLidPowerDriver::new(true, Some(false));
        let state = ClosedLidModeState::with_driver(driver.clone());
        driver.fail_next_set(CommandError::retryable(
            "closed_lid_mode_pmset_failed",
            "macOS refused the power setting.",
        ));

        let error = update_closed_lid_mode_settings_from_path(
            &path,
            &state,
            UpsertClosedLidModeSettingsRequestDto {
                enabled: true,
                acknowledge_global_power_change: true,
            },
        )
        .expect_err("pmset failure should be surfaced");

        assert_eq!(error.code, "closed_lid_mode_pmset_failed");
        assert!(driver.events().is_empty());
    }

    #[test]
    fn closed_lid_mode_unsupported_platform_reports_unsupported() {
        let root = tempfile::tempdir().expect("temp dir");
        let driver = FakeClosedLidPowerDriver::new(false, None);
        let state = ClosedLidModeState::with_driver(driver);
        let settings = load_closed_lid_mode_settings_from_path(&settings_path(&root), &state)
            .expect("load unsupported Closed-Lid Mode settings");

        assert!(!settings.enabled);
        assert!(!settings.active);
        assert!(!settings.platform_supported);
        assert_eq!(
            settings.active_status,
            ClosedLidModeActiveStatusDto::Unsupported
        );
    }
}
