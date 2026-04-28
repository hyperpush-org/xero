use std::{
    ffi::{c_char, c_void, CStr, CString},
    fmt,
    path::Path,
    ptr::NonNull,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{
    ipc::{Channel, JavaScriptChannelId},
    AppHandle, Manager, Runtime, State, Webview,
};

use crate::commands::{
    ActiveDictationSessionDto, CommandError, CommandResult, DictationEngineDto,
    DictationEnginePreferenceDto, DictationEngineStatusDto, DictationEventDto,
    DictationModernAssetStatusDto, DictationModernAssetsDto, DictationPermissionStateDto,
    DictationPlatformDto, DictationPrivacyModeDto, DictationSettingsDto, DictationStartRequestDto,
    DictationStartResponseDto, DictationStatusDto, DictationStopReasonDto,
    UpsertDictationSettingsRequestDto,
};
use crate::state::DesktopState;

static DICTATION_SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);
const DICTATION_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct DictationCancellationHandle {
    cancelled: Arc<AtomicBool>,
}

impl Default for DictationCancellationHandle {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl DictationCancellationHandle {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    #[cfg(test)]
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Debug)]
struct ActiveDictationSession {
    session_id: String,
    engine: DictationEngineDto,
    cancellation: DictationCancellationHandle,
    native: Option<native_shim::Session>,
}

impl ActiveDictationSession {
    fn to_dto(&self) -> ActiveDictationSessionDto {
        ActiveDictationSessionDto {
            session_id: self.session_id.clone(),
            engine: self.engine,
        }
    }

    fn stop_native(mut self) -> CommandResult<()> {
        if let Some(native) = self.native.take() {
            native
                .stop()
                .map_err(NativeOperationError::into_command_error)?;
        }
        Ok(())
    }

    fn cancel_native(mut self) -> CommandResult<()> {
        self.cancellation.cancel();
        if let Some(native) = self.native.take() {
            native
                .cancel()
                .map_err(NativeOperationError::into_command_error)?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct DictationStateInner {
    active: Option<ActiveDictationSession>,
}

/// Process-wide dictation state. The state owns the one-session guard and the
/// native session handle so command, channel, native, and window-close paths
/// all clean up the same active session.
#[derive(Debug, Clone, Default)]
pub struct DictationState {
    inner: Arc<Mutex<DictationStateInner>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DictationSettingsFile {
    schema_version: u32,
    engine_preference: DictationEnginePreferenceDto,
    privacy_mode: DictationPrivacyModeDto,
    locale: Option<String>,
    updated_at: String,
}

impl DictationState {
    fn active_session(&self) -> Option<ActiveDictationSessionDto> {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.active.as_ref().map(ActiveDictationSession::to_dto))
    }

    fn begin_session(
        &self,
        session_id: String,
        engine: DictationEngineDto,
        cancellation: DictationCancellationHandle,
    ) -> CommandResult<()> {
        let mut inner = self.inner.lock().map_err(|_| {
            CommandError::system_fault(
                "dictation_state_unavailable",
                "Cadence could not access dictation state.",
            )
        })?;

        if let Some(active) = inner.active.as_ref() {
            return Err(CommandError::user_fixable(
                "dictation_session_active",
                format!(
                    "Dictation session `{}` is already active. Stop it before starting another.",
                    active.session_id
                ),
            ));
        }

        inner.active = Some(ActiveDictationSession {
            session_id,
            engine,
            cancellation,
            native: None,
        });
        Ok(())
    }

    fn attach_native_session(
        &self,
        session_id: &str,
        engine: DictationEngineDto,
        native: native_shim::Session,
    ) -> Result<(), native_shim::Session> {
        let Ok(mut inner) = self.inner.lock() else {
            return Err(native);
        };
        let Some(active) = inner.active.as_mut() else {
            return Err(native);
        };
        if active.session_id != session_id {
            return Err(native);
        }

        active.engine = engine;
        active.native = Some(native);
        Ok(())
    }

    fn take_session(&self, session_id: &str, cancel: bool) -> Option<ActiveDictationSession> {
        let Ok(mut inner) = self.inner.lock() else {
            return None;
        };
        let active = inner.active.as_ref()?;
        if active.session_id != session_id {
            return None;
        }

        let active = inner
            .active
            .take()
            .expect("active session was just checked");
        if cancel {
            active.cancellation.cancel();
        }
        Some(active)
    }

    fn take_active_session(&self, cancel: bool) -> Option<ActiveDictationSession> {
        let Ok(mut inner) = self.inner.lock() else {
            return None;
        };
        let active = inner.active.take()?;
        if cancel {
            active.cancellation.cancel();
        }
        Some(active)
    }

    #[cfg(test)]
    fn clear_session(&self, session_id: &str, cancel: bool) -> bool {
        self.take_session(session_id, cancel).is_some()
    }

    fn cancel_active_for_shutdown(&self) {
        if let Some(active) = self.take_active_session(true) {
            let _ = active.cancel_native();
        }
    }
}

#[tauri::command]
pub fn speech_dictation_status(
    state: State<'_, DictationState>,
) -> CommandResult<DictationStatusDto> {
    let mut status = probe_dictation_status();
    status.active_session = state.active_session();
    Ok(status)
}

#[tauri::command]
pub fn speech_dictation_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<DictationSettingsDto> {
    load_dictation_settings(&app, state.inner())
}

#[tauri::command]
pub fn speech_dictation_update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertDictationSettingsRequestDto,
) -> CommandResult<DictationSettingsDto> {
    let path = state.global_db_path(&app)?;
    let next = dictation_settings_file_from_request(request)?;
    persist_dictation_settings_file(&path, &next)?;
    Ok(next.into_dto())
}

#[tauri::command]
pub fn speech_dictation_start<R: Runtime>(
    webview: Webview<R>,
    state: State<'_, DictationState>,
    request: DictationStartRequestDto,
) -> CommandResult<DictationStartResponseDto> {
    let channel = resolve_channel(&webview, request.channel.as_deref())?;
    let request = normalize_start_request(request);
    let status = probe_dictation_status();
    ensure_macos_platform(&status)?;

    let engine = select_engine(&status, request.engine_preference)?;
    let locale = request
        .locale
        .clone()
        .or_else(|| status.default_locale.clone())
        .unwrap_or_else(|| "en_US".to_string());
    let session_id = next_session_id();
    let cancellation = DictationCancellationHandle::default();

    state.begin_session(session_id.clone(), engine, cancellation)?;

    let context = Arc::new(NativeCallbackContext {
        session_id: session_id.clone(),
        state: state.inner().clone(),
        channel,
    });
    let mut native_request = NativeSessionRequest {
        session_id: session_id.clone(),
        engine,
        locale: locale.clone(),
        privacy_mode: request.privacy_mode,
        contextual_phrases: request.contextual_phrases,
    };

    let (native, start_response) = match create_and_start_native_session(&native_request, &context)
    {
        Ok(started) => started,
        Err(error) => {
            if should_fallback_to_legacy(request.engine_preference, engine, &status, &error) {
                native_request.engine = DictationEngineDto::Legacy;
                match create_and_start_native_session(&native_request, &context) {
                    Ok(started) => started,
                    Err(fallback_error) => {
                        state.take_session(&session_id, true);
                        return Err(fallback_error.into_command_error());
                    }
                }
            } else {
                state.take_session(&session_id, true);
                return Err(error.into_command_error());
            }
        }
    };

    let response_engine = start_response.engine.unwrap_or(native_request.engine);

    let response = DictationStartResponseDto {
        session_id: start_response
            .session_id
            .unwrap_or_else(|| session_id.clone()),
        engine: response_engine,
        locale: normalize_optional_text(start_response.locale).unwrap_or(locale),
    };
    if response.session_id != session_id {
        state.take_session(&session_id, true);
        return Err(CommandError::system_fault(
            "dictation_native_session_mismatch",
            "Cadence received a native dictation response for a different session.",
        ));
    }

    if state
        .attach_native_session(&session_id, response.engine, native)
        .is_err()
    {
        return Err(CommandError::retryable(
            "dictation_session_closed",
            "Cadence started dictation, but the session closed before the native handle could be attached.",
        ));
    }

    Ok(response)
}

#[tauri::command]
pub fn speech_dictation_stop(state: State<'_, DictationState>) -> CommandResult<()> {
    if let Some(active) = state.take_active_session(false) {
        active.stop_native()?;
    }
    Ok(())
}

#[tauri::command]
pub fn speech_dictation_cancel(state: State<'_, DictationState>) -> CommandResult<()> {
    if let Some(active) = state.take_active_session(true) {
        active.cancel_native()?;
    }
    Ok(())
}

pub fn shutdown_on_close<R: Runtime>(app: &AppHandle<R>) {
    if let Some(state) = app.try_state::<DictationState>() {
        state.cancel_active_for_shutdown();
    }
}

fn create_and_start_native_session(
    request: &NativeSessionRequest,
    context: &Arc<NativeCallbackContext>,
) -> Result<(native_shim::Session, NativeOperationResponse), NativeStartError> {
    let native = native_shim::Session::create(request, Arc::clone(context))
        .map_err(NativeStartError::Create)?;
    let start_response = native.start().map_err(NativeStartError::Start)?;
    Ok((native, start_response))
}

fn should_fallback_to_legacy(
    preference: DictationEnginePreferenceDto,
    attempted_engine: DictationEngineDto,
    status: &DictationStatusDto,
    error: &NativeStartError,
) -> bool {
    if preference != DictationEnginePreferenceDto::Automatic
        || attempted_engine != DictationEngineDto::Modern
        || !status.legacy.available
    {
        return false;
    }

    match error {
        NativeStartError::Start(error) => !matches!(
            error.code.as_str(),
            "dictation_microphone_permission_denied" | "dictation_speech_permission_denied"
        ),
        NativeStartError::Create(_) => false,
    }
}

#[derive(Debug, Clone)]
struct NormalizedDictationStartRequest {
    locale: Option<String>,
    engine_preference: DictationEnginePreferenceDto,
    privacy_mode: DictationPrivacyModeDto,
    contextual_phrases: Vec<String>,
}

fn normalize_start_request(request: DictationStartRequestDto) -> NormalizedDictationStartRequest {
    NormalizedDictationStartRequest {
        locale: normalize_optional_text(request.locale),
        engine_preference: request
            .engine_preference
            .unwrap_or(DictationEnginePreferenceDto::Automatic),
        privacy_mode: request
            .privacy_mode
            .unwrap_or(DictationPrivacyModeDto::OnDevicePreferred),
        contextual_phrases: request
            .contextual_phrases
            .into_iter()
            .filter_map(|phrase| normalize_optional_text(Some(phrase)))
            .collect(),
    }
}

pub(crate) fn load_dictation_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<DictationSettingsDto> {
    load_dictation_settings_from_path(&state.global_db_path(app)?)
}

fn load_dictation_settings_from_path(path: &Path) -> CommandResult<DictationSettingsDto> {
    let connection = crate::global_db::open_global_database(path)?;

    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM dictation_settings WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .ok();

    let Some(payload) = payload else {
        return Ok(default_dictation_settings());
    };

    let parsed = serde_json::from_str::<DictationSettingsFile>(&payload).map_err(|error| {
        CommandError::user_fixable(
            "dictation_settings_decode_failed",
            format!(
                "Cadence could not decode dictation settings stored in the global database: {error}"
            ),
        )
    })?;

    validate_dictation_settings_file(parsed, "dictation_settings_decode_failed")
        .map(DictationSettingsFile::into_dto)
}

fn persist_dictation_settings_file(
    path: &Path,
    settings: &DictationSettingsFile,
) -> CommandResult<()> {
    let payload = serde_json::to_string(settings).map_err(|error| {
        CommandError::system_fault(
            "dictation_settings_serialize_failed",
            format!("Cadence could not serialize dictation settings: {error}"),
        )
    })?;

    let connection = crate::global_db::open_global_database(path)?;
    connection
        .execute(
            "INSERT INTO dictation_settings (id, payload, updated_at) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
                payload = excluded.payload,
                updated_at = excluded.updated_at",
            rusqlite::params![payload, settings.updated_at],
        )
        .map_err(|error| {
            CommandError::retryable(
                "dictation_settings_write_failed",
                format!("Cadence could not persist dictation settings: {error}"),
            )
        })?;
    Ok(())
}

fn dictation_settings_file_from_request(
    request: UpsertDictationSettingsRequestDto,
) -> CommandResult<DictationSettingsFile> {
    validate_dictation_settings_file(
        DictationSettingsFile {
            schema_version: DICTATION_SETTINGS_SCHEMA_VERSION,
            engine_preference: request.engine_preference,
            privacy_mode: request.privacy_mode,
            locale: normalize_optional_text(request.locale),
            updated_at: crate::auth::now_timestamp(),
        },
        "dictation_settings_request_invalid",
    )
}

fn validate_dictation_settings_file(
    file: DictationSettingsFile,
    error_code: &'static str,
) -> CommandResult<DictationSettingsFile> {
    if file.schema_version != DICTATION_SETTINGS_SCHEMA_VERSION {
        return Err(CommandError::user_fixable(
            error_code,
            format!(
                "Cadence rejected dictation settings version `{}` because only version `{DICTATION_SETTINGS_SCHEMA_VERSION}` is supported.",
                file.schema_version
            ),
        ));
    }

    Ok(DictationSettingsFile {
        schema_version: DICTATION_SETTINGS_SCHEMA_VERSION,
        engine_preference: file.engine_preference,
        privacy_mode: file.privacy_mode,
        locale: normalize_optional_text(file.locale),
        updated_at: normalize_timestamp(file.updated_at),
    })
}

fn default_dictation_settings() -> DictationSettingsDto {
    DictationSettingsDto {
        engine_preference: DictationEnginePreferenceDto::Automatic,
        privacy_mode: DictationPrivacyModeDto::OnDevicePreferred,
        locale: None,
        updated_at: None,
    }
}

fn normalize_timestamp(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        crate::auth::now_timestamp()
    } else {
        trimmed.to_owned()
    }
}

impl DictationSettingsFile {
    fn into_dto(self) -> DictationSettingsDto {
        DictationSettingsDto {
            engine_preference: self.engine_preference,
            privacy_mode: self.privacy_mode,
            locale: self.locale,
            updated_at: Some(self.updated_at),
        }
    }
}

fn ensure_macos_platform(status: &DictationStatusDto) -> CommandResult<()> {
    if status.platform == DictationPlatformDto::Macos {
        return Ok(());
    }

    Err(CommandError::user_fixable(
        "dictation_unsupported_platform",
        "Cadence dictation is only available on macOS in this release.",
    ))
}

fn select_engine(
    status: &DictationStatusDto,
    preference: DictationEnginePreferenceDto,
) -> CommandResult<DictationEngineDto> {
    match preference {
        DictationEnginePreferenceDto::Modern if status.modern.available => {
            Ok(DictationEngineDto::Modern)
        }
        DictationEnginePreferenceDto::Modern => Err(engine_unavailable_error(
            "dictation_modern_unavailable",
            "modern",
            &status.modern,
        )),
        DictationEnginePreferenceDto::Legacy if status.legacy.available => {
            Ok(DictationEngineDto::Legacy)
        }
        DictationEnginePreferenceDto::Legacy => Err(engine_unavailable_error(
            "dictation_legacy_unavailable",
            "legacy",
            &status.legacy,
        )),
        DictationEnginePreferenceDto::Automatic if status.modern.available => {
            Ok(DictationEngineDto::Modern)
        }
        DictationEnginePreferenceDto::Automatic if status.legacy.available => {
            Ok(DictationEngineDto::Legacy)
        }
        DictationEnginePreferenceDto::Automatic => Err(CommandError::user_fixable(
            "dictation_engine_unavailable",
            "Cadence could not find an available native macOS dictation engine.",
        )),
    }
}

fn engine_unavailable_error(
    code: &'static str,
    engine_label: &'static str,
    status: &DictationEngineStatusDto,
) -> CommandError {
    let detail = status
        .reason
        .as_deref()
        .map(|reason| format!(" Reason: {reason}."))
        .unwrap_or_default();
    CommandError::user_fixable(
        code,
        format!("Cadence could not start the requested {engine_label} dictation engine.{detail}"),
    )
}

fn resolve_channel<R: Runtime>(
    webview: &Webview<R>,
    raw_channel: Option<&str>,
) -> CommandResult<Channel<DictationEventDto>> {
    let Some(raw_channel) = raw_channel else {
        return Err(CommandError::user_fixable(
            "dictation_channel_missing",
            "Cadence requires a dictation channel before it can stream native speech events.",
        ));
    };

    let channel_id = JavaScriptChannelId::from_str(raw_channel).map_err(|_| {
        CommandError::user_fixable(
            "dictation_channel_invalid",
            "Cadence received an invalid dictation channel handle from the desktop shell.",
        )
    })?;

    Ok(channel_id.channel_on(webview.clone()))
}

fn next_session_id() -> String {
    let counter = DICTATION_SESSION_COUNTER.fetch_add(1, Ordering::SeqCst);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("dictation-{now:x}-{counter:x}")
}

#[derive(Debug, Clone)]
struct DictationProbe {
    platform: DictationPlatformDto,
    os_version: Option<String>,
    default_locale: Option<String>,
    supported_locales: Vec<String>,
    modern_compiled: bool,
    modern_runtime_supported: bool,
    modern_assets: DictationModernAssetsDto,
    legacy_runtime_supported: bool,
    legacy_recognizer_available: bool,
    microphone_permission: DictationPermissionStateDto,
    speech_permission: DictationPermissionStateDto,
    fallback_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeDictationProbe {
    platform: Option<String>,
    os_version: Option<String>,
    default_locale: Option<String>,
    supported_locales: Option<Vec<String>>,
    modern_compiled: Option<bool>,
    modern_runtime_supported: Option<bool>,
    modern_assets_status: Option<String>,
    modern_asset_locale: Option<String>,
    modern_assets_reason: Option<String>,
    legacy_runtime_supported: Option<bool>,
    legacy_recognizer_available: Option<bool>,
    microphone_permission: Option<String>,
    speech_permission: Option<String>,
}

impl DictationProbe {
    fn into_status(self) -> DictationStatusDto {
        let modern_reason = if self.platform == DictationPlatformDto::Unsupported {
            Some("unsupported_platform".into())
        } else if let Some(reason) = self.fallback_reason.clone() {
            Some(reason)
        } else if !self.modern_compiled {
            Some("modern_sdk_unavailable".into())
        } else if !self.modern_runtime_supported {
            Some("runtime_too_old".into())
        } else {
            None
        };

        let legacy_reason = if self.platform == DictationPlatformDto::Unsupported {
            Some("unsupported_platform".into())
        } else if let Some(reason) = self.fallback_reason {
            Some(reason)
        } else if !self.legacy_runtime_supported {
            Some("legacy_runtime_unavailable".into())
        } else if !self.legacy_recognizer_available {
            Some("legacy_recognizer_unavailable".into())
        } else {
            None
        };

        DictationStatusDto {
            platform: self.platform,
            os_version: self.os_version,
            default_locale: self.default_locale,
            supported_locales: self.supported_locales,
            modern: DictationEngineStatusDto {
                available: self.modern_compiled && self.modern_runtime_supported,
                compiled: self.modern_compiled,
                runtime_supported: self.modern_runtime_supported,
                reason: modern_reason,
            },
            legacy: DictationEngineStatusDto {
                available: self.legacy_runtime_supported && self.legacy_recognizer_available,
                compiled: self.legacy_runtime_supported,
                runtime_supported: self.legacy_runtime_supported,
                reason: legacy_reason,
            },
            modern_assets: self.modern_assets,
            microphone_permission: self.microphone_permission,
            speech_permission: self.speech_permission,
            active_session: None,
        }
    }
}

pub(crate) fn probe_dictation_status() -> DictationStatusDto {
    native_probe().unwrap_or_else(fallback_probe).into_status()
}

fn native_probe() -> Result<DictationProbe, String> {
    let raw = native_shim::capability_status_json()?;
    let probe: NativeDictationProbe =
        serde_json::from_str(&raw).map_err(|error| format!("native_status_malformed: {error}"))?;

    Ok(DictationProbe {
        platform: match probe.platform.as_deref() {
            Some("macos") => DictationPlatformDto::Macos,
            _ => DictationPlatformDto::Unsupported,
        },
        os_version: normalize_optional_text(probe.os_version),
        default_locale: normalize_optional_text(probe.default_locale),
        supported_locales: normalize_locale_list(probe.supported_locales.unwrap_or_default()),
        modern_compiled: probe.modern_compiled.unwrap_or(false),
        modern_runtime_supported: probe.modern_runtime_supported.unwrap_or(false),
        modern_assets: DictationModernAssetsDto {
            status: modern_asset_status(probe.modern_assets_status.as_deref()),
            locale: normalize_optional_text(probe.modern_asset_locale),
            reason: normalize_optional_text(probe.modern_assets_reason),
        },
        legacy_runtime_supported: probe.legacy_runtime_supported.unwrap_or(false),
        legacy_recognizer_available: probe.legacy_recognizer_available.unwrap_or(false),
        microphone_permission: permission_state(probe.microphone_permission.as_deref()),
        speech_permission: permission_state(probe.speech_permission.as_deref()),
        fallback_reason: None,
    })
}

fn fallback_probe(reason: String) -> DictationProbe {
    DictationProbe {
        platform: fallback_platform(),
        os_version: None,
        default_locale: None,
        supported_locales: Vec::new(),
        modern_compiled: option_env!("CADENCE_DICTATION_MODERN_COMPILED") == Some("1"),
        modern_runtime_supported: false,
        modern_assets: DictationModernAssetsDto {
            status: DictationModernAssetStatusDto::Unavailable,
            locale: None,
            reason: Some(reason.clone()),
        },
        legacy_runtime_supported: false,
        legacy_recognizer_available: false,
        microphone_permission: fallback_permission(),
        speech_permission: fallback_permission(),
        fallback_reason: Some(reason),
    }
}

#[cfg(target_os = "macos")]
fn fallback_platform() -> DictationPlatformDto {
    DictationPlatformDto::Macos
}

#[cfg(not(target_os = "macos"))]
fn fallback_platform() -> DictationPlatformDto {
    DictationPlatformDto::Unsupported
}

#[cfg(target_os = "macos")]
fn fallback_permission() -> DictationPermissionStateDto {
    DictationPermissionStateDto::Unknown
}

#[cfg(not(target_os = "macos"))]
fn fallback_permission() -> DictationPermissionStateDto {
    DictationPermissionStateDto::Unsupported
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn normalize_locale_list(locales: Vec<String>) -> Vec<String> {
    let mut locales = locales
        .into_iter()
        .filter_map(|locale| normalize_optional_text(Some(locale)))
        .collect::<Vec<_>>();
    locales.sort();
    locales.dedup();
    locales
}

fn modern_asset_status(value: Option<&str>) -> DictationModernAssetStatusDto {
    match value {
        Some("installed") => DictationModernAssetStatusDto::Installed,
        Some("not_installed") => DictationModernAssetStatusDto::NotInstalled,
        Some("unavailable") => DictationModernAssetStatusDto::Unavailable,
        Some("unsupported_locale") => DictationModernAssetStatusDto::UnsupportedLocale,
        _ => DictationModernAssetStatusDto::Unknown,
    }
}

fn permission_state(value: Option<&str>) -> DictationPermissionStateDto {
    match value {
        Some("authorized") => DictationPermissionStateDto::Authorized,
        Some("denied") => DictationPermissionStateDto::Denied,
        Some("restricted") => DictationPermissionStateDto::Restricted,
        Some("not_determined") => DictationPermissionStateDto::NotDetermined,
        Some("unsupported") => DictationPermissionStateDto::Unsupported,
        _ => DictationPermissionStateDto::Unknown,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeSessionRequest {
    session_id: String,
    engine: DictationEngineDto,
    locale: String,
    privacy_mode: DictationPrivacyModeDto,
    contextual_phrases: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeOperationResponse {
    ok: bool,
    session_id: Option<String>,
    engine: Option<DictationEngineDto>,
    locale: Option<String>,
    code: Option<String>,
    message: Option<String>,
    retryable: Option<bool>,
}

#[derive(Debug, Clone)]
struct NativeOperationError {
    code: String,
    message: String,
    retryable: bool,
}

impl NativeOperationError {
    fn into_command_error(self) -> CommandError {
        if self.retryable {
            CommandError::retryable(self.code, self.message)
        } else {
            CommandError::user_fixable(self.code, self.message)
        }
    }
}

#[derive(Debug, Clone)]
enum NativeStartError {
    Create(String),
    Start(NativeOperationError),
}

impl NativeStartError {
    fn into_command_error(self) -> CommandError {
        match self {
            NativeStartError::Create(error) => CommandError::system_fault(
                "dictation_native_session_unavailable",
                format!("Cadence could not create a native dictation session: {error}"),
            ),
            NativeStartError::Start(error) => error.into_command_error(),
        }
    }
}

fn native_operation_result(
    response: NativeOperationResponse,
) -> Result<NativeOperationResponse, NativeOperationError> {
    if response.ok {
        Ok(response)
    } else {
        Err(NativeOperationError {
            code: response
                .code
                .unwrap_or_else(|| "dictation_native_operation_failed".into()),
            message: response.message.unwrap_or_else(|| {
                "Cadence could not complete the native dictation operation.".into()
            }),
            retryable: response.retryable.unwrap_or(false),
        })
    }
}

struct NativeCallbackContext {
    session_id: String,
    state: DictationState,
    channel: Channel<DictationEventDto>,
}

impl NativeCallbackContext {
    fn handle_payload(&self, payload: &str) {
        let event = match serde_json::from_str::<NativeDictationEvent>(payload) {
            Ok(event) => match event.into_dto(&self.session_id) {
                Ok(event) => event,
                Err(error) => NativeEventOutcome {
                    event: DictationEventDto::Error {
                        session_id: Some(self.session_id.clone()),
                        code: error.code,
                        message: error.message,
                        retryable: error.retryable,
                    },
                    terminal: true,
                },
            },
            Err(error) => NativeEventOutcome {
                event: DictationEventDto::Error {
                    session_id: Some(self.session_id.clone()),
                    code: "dictation_native_event_malformed".into(),
                    message: format!(
                        "Cadence received a malformed native dictation event: {error}"
                    ),
                    retryable: true,
                },
                terminal: true,
            },
        };

        if self.channel.send(event.event.clone()).is_err() {
            drop(self.state.take_session(&self.session_id, true));
            return;
        }

        if event.terminal {
            drop(self.state.take_session(&self.session_id, false));
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum NativeDictationEvent {
    Permission {
        microphone: DictationPermissionStateDto,
        speech: DictationPermissionStateDto,
    },
    Started {
        #[serde(rename = "sessionId")]
        session_id: String,
        engine: DictationEngineDto,
        locale: String,
    },
    AssetInstalling {
        progress: Option<f32>,
    },
    Partial {
        #[serde(rename = "sessionId")]
        session_id: String,
        text: String,
        sequence: u64,
    },
    Final {
        #[serde(rename = "sessionId")]
        session_id: String,
        text: String,
        sequence: u64,
    },
    Stopped {
        #[serde(rename = "sessionId")]
        session_id: String,
        reason: DictationStopReasonDto,
    },
    Error {
        #[serde(rename = "sessionId")]
        session_id: Option<String>,
        code: String,
        message: String,
        retryable: bool,
    },
}

#[derive(Debug, Clone)]
struct NativeEventOutcome {
    event: DictationEventDto,
    terminal: bool,
}

impl NativeDictationEvent {
    fn into_dto(
        self,
        expected_session_id: &str,
    ) -> Result<NativeEventOutcome, NativeOperationError> {
        match self {
            NativeDictationEvent::Permission { microphone, speech } => Ok(NativeEventOutcome {
                event: DictationEventDto::Permission { microphone, speech },
                terminal: false,
            }),
            NativeDictationEvent::Started {
                session_id,
                engine,
                locale,
            } => {
                validate_native_session_id(expected_session_id, &session_id)?;
                Ok(NativeEventOutcome {
                    event: DictationEventDto::Started {
                        session_id,
                        engine,
                        locale,
                    },
                    terminal: false,
                })
            }
            NativeDictationEvent::AssetInstalling { progress } => Ok(NativeEventOutcome {
                event: DictationEventDto::AssetInstalling { progress },
                terminal: false,
            }),
            NativeDictationEvent::Partial {
                session_id,
                text,
                sequence,
            } => {
                validate_native_session_id(expected_session_id, &session_id)?;
                Ok(NativeEventOutcome {
                    event: DictationEventDto::Partial {
                        session_id,
                        text,
                        sequence,
                    },
                    terminal: false,
                })
            }
            NativeDictationEvent::Final {
                session_id,
                text,
                sequence,
            } => {
                validate_native_session_id(expected_session_id, &session_id)?;
                Ok(NativeEventOutcome {
                    event: DictationEventDto::Final {
                        session_id,
                        text,
                        sequence,
                    },
                    terminal: false,
                })
            }
            NativeDictationEvent::Stopped { session_id, reason } => {
                validate_native_session_id(expected_session_id, &session_id)?;
                Ok(NativeEventOutcome {
                    event: DictationEventDto::Stopped { session_id, reason },
                    terminal: true,
                })
            }
            NativeDictationEvent::Error {
                session_id,
                code,
                message,
                retryable,
            } => {
                if let Some(session_id) = session_id.as_deref() {
                    validate_native_session_id(expected_session_id, session_id)?;
                }
                Ok(NativeEventOutcome {
                    event: DictationEventDto::Error {
                        session_id,
                        code,
                        message,
                        retryable,
                    },
                    terminal: true,
                })
            }
        }
    }
}

fn validate_native_session_id(
    expected_session_id: &str,
    actual_session_id: &str,
) -> Result<(), NativeOperationError> {
    if actual_session_id == expected_session_id {
        return Ok(());
    }

    Err(NativeOperationError {
        code: "dictation_native_session_mismatch".into(),
        message: "Cadence received a native dictation event for a different session.".into(),
        retryable: false,
    })
}

extern "C" fn native_event_callback(context: *mut c_void, payload: *const c_char) {
    if context.is_null() || payload.is_null() {
        return;
    }

    unsafe {
        Arc::increment_strong_count(context as *const NativeCallbackContext);
    }
    let context = unsafe { Arc::from_raw(context as *const NativeCallbackContext) };
    let payload = unsafe { CStr::from_ptr(payload).to_string_lossy().into_owned() };
    context.handle_payload(&payload);
}

#[cfg(all(target_os = "macos", cadence_dictation_native_shim))]
mod native_shim {
    use super::*;

    type EventCallback = extern "C" fn(*mut c_void, *const c_char);

    extern "C" {
        fn cadence_dictation_capability_status_json() -> *mut c_char;
        fn cadence_dictation_create_session(
            request_json: *const c_char,
            callback: EventCallback,
            context: *mut c_void,
        ) -> *mut c_void;
        fn cadence_dictation_start_session(session: *mut c_void) -> *mut c_char;
        fn cadence_dictation_stop_session(session: *mut c_void) -> *mut c_char;
        fn cadence_dictation_cancel_session(session: *mut c_void) -> *mut c_char;
        fn cadence_dictation_release_session(session: *mut c_void);
        fn cadence_dictation_free_string(value: *mut c_char);
    }

    pub(super) struct Session {
        ptr: NonNull<c_void>,
        context: *const NativeCallbackContext,
    }

    unsafe impl Send for Session {}

    impl fmt::Debug for Session {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("Session")
                .field("ptr", &self.ptr)
                .finish_non_exhaustive()
        }
    }

    impl Session {
        pub(super) fn create(
            request: &NativeSessionRequest,
            context: Arc<NativeCallbackContext>,
        ) -> Result<Self, String> {
            let request_json = serde_json::to_string(request)
                .map_err(|error| format!("native_session_request_malformed: {error}"))?;
            let request_json = CString::new(request_json)
                .map_err(|error| format!("native_session_request_contains_nul: {error}"))?;
            let context = Arc::into_raw(context);
            let ptr = unsafe {
                cadence_dictation_create_session(
                    request_json.as_ptr(),
                    native_event_callback,
                    context as *mut c_void,
                )
            };
            let Some(ptr) = NonNull::new(ptr) else {
                unsafe {
                    drop(Arc::from_raw(context));
                }
                return Err("native_session_create_failed".into());
            };

            Ok(Self { ptr, context })
        }

        pub(super) fn start(&self) -> Result<NativeOperationResponse, NativeOperationError> {
            decode_operation_response(unsafe { cadence_dictation_start_session(self.ptr.as_ptr()) })
        }

        pub(super) fn stop(&self) -> Result<(), NativeOperationError> {
            decode_operation_response(unsafe { cadence_dictation_stop_session(self.ptr.as_ptr()) })
                .map(|_| ())
        }

        pub(super) fn cancel(&self) -> Result<(), NativeOperationError> {
            decode_operation_response(unsafe {
                cadence_dictation_cancel_session(self.ptr.as_ptr())
            })
            .map(|_| ())
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                cadence_dictation_release_session(self.ptr.as_ptr());
                drop(Arc::from_raw(self.context));
            }
        }
    }

    pub(super) fn capability_status_json() -> Result<String, String> {
        let ptr = unsafe { cadence_dictation_capability_status_json() };
        if ptr.is_null() {
            return Err("native_status_unavailable".into());
        }

        let value = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
        unsafe { cadence_dictation_free_string(ptr) };
        Ok(value)
    }

    fn decode_operation_response(
        ptr: *mut c_char,
    ) -> Result<NativeOperationResponse, NativeOperationError> {
        if ptr.is_null() {
            return Err(NativeOperationError {
                code: "dictation_native_response_missing".into(),
                message: "Cadence did not receive a native dictation response.".into(),
                retryable: true,
            });
        }

        let value = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
        unsafe { cadence_dictation_free_string(ptr) };
        let response =
            serde_json::from_str::<NativeOperationResponse>(&value).map_err(|error| {
                NativeOperationError {
                    code: "dictation_native_response_malformed".into(),
                    message: format!(
                        "Cadence received a malformed native dictation response: {error}"
                    ),
                    retryable: true,
                }
            })?;
        native_operation_result(response)
    }
}

#[cfg(not(all(target_os = "macos", cadence_dictation_native_shim)))]
mod native_shim {
    use super::*;

    pub(super) struct Session;

    impl fmt::Debug for Session {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.debug_struct("Session").finish_non_exhaustive()
        }
    }

    impl Session {
        pub(super) fn create(
            _request: &NativeSessionRequest,
            _context: Arc<NativeCallbackContext>,
        ) -> Result<Self, String> {
            Err("native_shim_unavailable".into())
        }

        pub(super) fn start(&self) -> Result<NativeOperationResponse, NativeOperationError> {
            Err(native_unavailable_error())
        }

        pub(super) fn stop(&self) -> Result<(), NativeOperationError> {
            Ok(())
        }

        pub(super) fn cancel(&self) -> Result<(), NativeOperationError> {
            Ok(())
        }
    }

    pub(super) fn capability_status_json() -> Result<String, String> {
        Err("native_shim_unavailable".into())
    }

    fn native_unavailable_error() -> NativeOperationError {
        NativeOperationError {
            code: "dictation_native_shim_unavailable".into(),
            message: "Cadence was built without the native macOS dictation shim.".into(),
            retryable: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn available_status() -> DictationStatusDto {
        DictationStatusDto {
            platform: DictationPlatformDto::Macos,
            os_version: Some("26.0.0".into()),
            default_locale: Some("en_US".into()),
            supported_locales: vec!["en_US".into()],
            modern: DictationEngineStatusDto {
                available: false,
                compiled: false,
                runtime_supported: false,
                reason: Some("modern_sdk_unavailable".into()),
            },
            legacy: DictationEngineStatusDto {
                available: true,
                compiled: true,
                runtime_supported: true,
                reason: None,
            },
            modern_assets: DictationModernAssetsDto {
                status: DictationModernAssetStatusDto::Unavailable,
                locale: None,
                reason: Some("modern_sdk_unavailable".into()),
            },
            microphone_permission: DictationPermissionStateDto::NotDetermined,
            speech_permission: DictationPermissionStateDto::NotDetermined,
            active_session: None,
        }
    }

    #[test]
    fn session_guard_allows_only_one_active_session() {
        let state = DictationState::default();
        let first_cancel = DictationCancellationHandle::default();
        let second_cancel = DictationCancellationHandle::default();

        state
            .begin_session(
                "session-1".into(),
                DictationEngineDto::Legacy,
                first_cancel.clone(),
            )
            .expect("first session should start");

        let error = state
            .begin_session(
                "session-2".into(),
                DictationEngineDto::Modern,
                second_cancel,
            )
            .expect_err("second session should be rejected");
        assert_eq!(error.code, "dictation_session_active");

        assert_eq!(
            state.active_session(),
            Some(ActiveDictationSessionDto {
                session_id: "session-1".into(),
                engine: DictationEngineDto::Legacy,
            })
        );
        assert!(!first_cancel.is_cancelled());
    }

    #[test]
    fn clearing_session_cancels_and_releases_guard() {
        let state = DictationState::default();
        let cancel = DictationCancellationHandle::default();

        state
            .begin_session(
                "session-1".into(),
                DictationEngineDto::Modern,
                cancel.clone(),
            )
            .expect("session should start");

        assert!(!state.clear_session("wrong-session", true));
        assert_eq!(
            state.active_session().map(|session| session.session_id),
            Some("session-1".into())
        );

        assert!(state.clear_session("session-1", true));
        assert!(cancel.is_cancelled());
        assert_eq!(state.active_session(), None);

        state
            .begin_session(
                "session-2".into(),
                DictationEngineDto::Legacy,
                DictationCancellationHandle::default(),
            )
            .expect("guard should be released");
    }

    #[test]
    fn stop_and_cancel_state_transitions_are_idempotent() {
        let state = DictationState::default();
        let stop_cancel = DictationCancellationHandle::default();

        state
            .begin_session(
                "session-stop".into(),
                DictationEngineDto::Legacy,
                stop_cancel.clone(),
            )
            .expect("session should start");
        let stopped = state
            .take_active_session(false)
            .expect("active session should be present");
        assert_eq!(stopped.session_id, "session-stop");
        assert!(!stop_cancel.is_cancelled());
        assert!(state.take_active_session(false).is_none());

        let cancel_cancel = DictationCancellationHandle::default();
        state
            .begin_session(
                "session-cancel".into(),
                DictationEngineDto::Legacy,
                cancel_cancel.clone(),
            )
            .expect("session should start");
        drop(state.take_active_session(true));
        assert!(cancel_cancel.is_cancelled());
        assert!(state.take_active_session(true).is_none());
    }

    #[test]
    fn engine_selection_honors_preferences_and_reports_user_fixable_errors() {
        let status = available_status();
        assert_eq!(
            select_engine(&status, DictationEnginePreferenceDto::Automatic).unwrap(),
            DictationEngineDto::Legacy
        );
        assert_eq!(
            select_engine(&status, DictationEnginePreferenceDto::Legacy).unwrap(),
            DictationEngineDto::Legacy
        );

        let error = select_engine(&status, DictationEnginePreferenceDto::Modern)
            .expect_err("modern should be unavailable");
        assert_eq!(error.code, "dictation_modern_unavailable");
    }

    #[test]
    fn engine_selection_covers_automatic_and_unavailable_matrix() {
        let mut status = available_status();
        status.modern = DictationEngineStatusDto {
            available: true,
            compiled: true,
            runtime_supported: true,
            reason: None,
        };

        assert_eq!(
            select_engine(&status, DictationEnginePreferenceDto::Automatic).unwrap(),
            DictationEngineDto::Modern
        );
        assert_eq!(
            select_engine(&status, DictationEnginePreferenceDto::Legacy).unwrap(),
            DictationEngineDto::Legacy
        );

        status.modern.available = false;
        status.legacy.available = false;

        let error = select_engine(&status, DictationEnginePreferenceDto::Automatic)
            .expect_err("automatic should fail when no engine is available");
        assert_eq!(error.code, "dictation_engine_unavailable");

        let error = select_engine(&status, DictationEnginePreferenceDto::Legacy)
            .expect_err("legacy should report a targeted failure");
        assert_eq!(error.code, "dictation_legacy_unavailable");
    }

    #[test]
    fn automatic_mode_falls_back_only_for_modern_startup_failures() {
        let mut status = available_status();
        status.modern = DictationEngineStatusDto {
            available: true,
            compiled: true,
            runtime_supported: true,
            reason: None,
        };

        let startup_error = NativeStartError::Start(NativeOperationError {
            code: "dictation_modern_asset_install_failed".into(),
            message: "asset install failed".into(),
            retryable: true,
        });
        assert!(should_fallback_to_legacy(
            DictationEnginePreferenceDto::Automatic,
            DictationEngineDto::Modern,
            &status,
            &startup_error,
        ));

        let permission_error = NativeStartError::Start(NativeOperationError {
            code: "dictation_microphone_permission_denied".into(),
            message: "permission denied".into(),
            retryable: false,
        });
        assert!(!should_fallback_to_legacy(
            DictationEnginePreferenceDto::Automatic,
            DictationEngineDto::Modern,
            &status,
            &permission_error,
        ));

        assert!(!should_fallback_to_legacy(
            DictationEnginePreferenceDto::Modern,
            DictationEngineDto::Modern,
            &status,
            &startup_error,
        ));
    }

    #[test]
    fn native_events_convert_to_dictation_dtos() {
        let event: NativeDictationEvent = serde_json::from_str(
            r#"{"kind":"started","sessionId":"session-1","engine":"legacy","locale":"en_US"}"#,
        )
        .expect("event should parse");
        let event = event
            .into_dto("session-1")
            .expect("event should convert to dto");
        assert!(!event.terminal);
        assert_eq!(
            event.event,
            DictationEventDto::Started {
                session_id: "session-1".into(),
                engine: DictationEngineDto::Legacy,
                locale: "en_US".into(),
            }
        );

        let mismatched: NativeDictationEvent =
            serde_json::from_str(r#"{"kind":"stopped","sessionId":"other","reason":"user"}"#)
                .expect("event should parse");
        let error = mismatched
            .into_dto("session-1")
            .expect_err("mismatched session should fail");
        assert_eq!(error.code, "dictation_native_session_mismatch");
    }

    #[test]
    fn channel_delivery_failure_clears_and_cancels_active_session() {
        let state = DictationState::default();
        let cancel = DictationCancellationHandle::default();
        state
            .begin_session(
                "session-1".into(),
                DictationEngineDto::Legacy,
                cancel.clone(),
            )
            .expect("session should start");

        let channel = tauri::ipc::Channel::<DictationEventDto>::new(move |_body| {
            Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "channel dropped").into())
        });
        let context = NativeCallbackContext {
            session_id: "session-1".into(),
            state: state.clone(),
            channel,
        };

        context.handle_payload(
            r#"{"kind":"partial","sessionId":"session-1","text":"hello","sequence":1}"#,
        );

        assert_eq!(state.active_session(), None);
        assert!(cancel.is_cancelled());
    }

    #[test]
    fn malformed_native_permissions_normalize_to_unknown() {
        assert_eq!(
            permission_state(Some("unexpected")),
            DictationPermissionStateDto::Unknown
        );
        assert_eq!(
            permission_state(Some("not_determined")),
            DictationPermissionStateDto::NotDetermined
        );
    }

    #[test]
    fn capability_probe_returns_status_contract_shape() {
        let status = probe_dictation_status();

        if cfg!(target_os = "macos") {
            assert_eq!(status.platform, DictationPlatformDto::Macos);
        } else {
            assert_eq!(status.platform, DictationPlatformDto::Unsupported);
        }

        assert_eq!(status.active_session, None);
        assert!(
            matches!(
                status.microphone_permission,
                DictationPermissionStateDto::Authorized
                    | DictationPermissionStateDto::Denied
                    | DictationPermissionStateDto::Restricted
                    | DictationPermissionStateDto::NotDetermined
                    | DictationPermissionStateDto::Unsupported
                    | DictationPermissionStateDto::Unknown
            ),
            "microphone permission must map to a known DTO variant"
        );
        assert!(
            matches!(
                status.speech_permission,
                DictationPermissionStateDto::Authorized
                    | DictationPermissionStateDto::Denied
                    | DictationPermissionStateDto::Restricted
                    | DictationPermissionStateDto::NotDetermined
                    | DictationPermissionStateDto::Unsupported
                    | DictationPermissionStateDto::Unknown
            ),
            "speech permission must map to a known DTO variant"
        );
    }
}
