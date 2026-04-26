use std::{
    ffi::{c_char, CStr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use serde::Deserialize;
use tauri::State;

use crate::commands::{
    ActiveDictationSessionDto, CommandError, CommandResult, DictationEngineDto,
    DictationEngineStatusDto, DictationPermissionStateDto, DictationPlatformDto,
    DictationStatusDto,
};

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

#[derive(Debug, Clone)]
struct ActiveDictationSession {
    session_id: String,
    engine: DictationEngineDto,
    cancellation: DictationCancellationHandle,
}

impl ActiveDictationSession {
    fn to_dto(&self) -> ActiveDictationSessionDto {
        ActiveDictationSessionDto {
            session_id: self.session_id.clone(),
            engine: self.engine,
        }
    }
}

#[derive(Debug, Default)]
struct DictationStateInner {
    active: Option<ActiveDictationSession>,
}

/// Process-wide dictation state. Phase 0 only exposes status, but the state
/// owns the one-session guard and cancellation handle that later phases use.
#[derive(Debug, Default)]
pub struct DictationState {
    inner: Mutex<DictationStateInner>,
}

impl DictationState {
    fn active_session(&self) -> Option<ActiveDictationSessionDto> {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.active.as_ref().map(ActiveDictationSession::to_dto))
    }

    #[allow(dead_code)]
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
        });
        Ok(())
    }

    #[allow(dead_code)]
    fn clear_session(&self, session_id: &str, cancel: bool) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };

        let Some(active) = inner.active.as_ref() else {
            return false;
        };

        if active.session_id != session_id {
            return false;
        }

        let active = inner
            .active
            .take()
            .expect("active session was just checked");
        if cancel {
            active.cancellation.cancel();
        }
        true
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

#[derive(Debug, Clone)]
struct DictationProbe {
    platform: DictationPlatformDto,
    default_locale: Option<String>,
    modern_compiled: bool,
    modern_runtime_supported: bool,
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
    default_locale: Option<String>,
    modern_compiled: Option<bool>,
    modern_runtime_supported: Option<bool>,
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
            default_locale: self.default_locale,
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
            microphone_permission: self.microphone_permission,
            speech_permission: self.speech_permission,
            active_session: None,
        }
    }
}

fn probe_dictation_status() -> DictationStatusDto {
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
        default_locale: normalize_optional_text(probe.default_locale),
        modern_compiled: probe.modern_compiled.unwrap_or(false),
        modern_runtime_supported: probe.modern_runtime_supported.unwrap_or(false),
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
        default_locale: None,
        modern_compiled: option_env!("CADENCE_DICTATION_MODERN_COMPILED") == Some("1"),
        modern_runtime_supported: false,
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

#[cfg(all(target_os = "macos", cadence_dictation_native_shim))]
mod native_shim {
    use super::*;

    extern "C" {
        fn cadence_dictation_capability_status_json() -> *mut c_char;
        fn cadence_dictation_free_string(value: *mut c_char);
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
}

#[cfg(not(all(target_os = "macos", cadence_dictation_native_shim)))]
mod native_shim {
    pub(super) fn capability_status_json() -> Result<String, String> {
        Err("native_shim_unavailable".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
