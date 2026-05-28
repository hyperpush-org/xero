use serde::{Deserialize, Serialize};

pub const SPEECH_DICTATION_START_COMMAND: &str = "speech_dictation_start";
pub const SPEECH_DICTATION_STOP_COMMAND: &str = "speech_dictation_stop";
pub const SPEECH_DICTATION_CANCEL_COMMAND: &str = "speech_dictation_cancel";
pub const SPEECH_DICTATION_SETTINGS_COMMAND: &str = "speech_dictation_settings";
pub const SPEECH_DICTATION_UPDATE_SETTINGS_COMMAND: &str = "speech_dictation_update_settings";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DictationPlatformDto {
    Macos,
    Windows,
    Linux,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DictationEngineDto {
    Modern,
    Legacy,
    WindowsSdk,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DictationEnginePreferenceDto {
    Automatic,
    Modern,
    Legacy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DictationPrivacyModeDto {
    OnDevicePreferred,
    OnDeviceRequired,
    AllowNetwork,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DictationPermissionStateDto {
    Authorized,
    Denied,
    Restricted,
    NotDetermined,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DictationStopReasonDto {
    User,
    Cancelled,
    Error,
    ChannelClosed,
    AppClosing,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DictationModernAssetStatusDto {
    Installed,
    NotInstalled,
    Unavailable,
    UnsupportedLocale,
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DictationEngineStatusDto {
    pub available: bool,
    pub compiled: bool,
    pub runtime_supported: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DictationModernAssetsDto {
    pub status: DictationModernAssetStatusDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActiveDictationSessionDto {
    pub session_id: String,
    pub engine: DictationEngineDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DictationStatusDto {
    pub platform: DictationPlatformDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    pub default_locale: Option<String>,
    #[serde(default)]
    pub supported_locales: Vec<String>,
    pub modern: DictationEngineStatusDto,
    pub legacy: DictationEngineStatusDto,
    #[serde(default)]
    pub windows_sdk: DictationEngineStatusDto,
    pub modern_assets: DictationModernAssetsDto,
    pub microphone_permission: DictationPermissionStateDto,
    pub speech_permission: DictationPermissionStateDto,
    pub active_session: Option<ActiveDictationSessionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DictationSettingsDto {
    pub engine_preference: DictationEnginePreferenceDto,
    pub privacy_mode: DictationPrivacyModeDto,
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertDictationSettingsRequestDto {
    pub engine_preference: DictationEnginePreferenceDto,
    pub privacy_mode: DictationPrivacyModeDto,
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DictationStartRequestDto {
    pub locale: Option<String>,
    pub engine_preference: Option<DictationEnginePreferenceDto>,
    pub privacy_mode: Option<DictationPrivacyModeDto>,
    #[serde(default)]
    pub contextual_phrases: Vec<String>,
    #[serde(default)]
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DictationStartResponseDto {
    pub session_id: String,
    pub engine: DictationEngineDto,
    pub locale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DictationEventDto {
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
    AudioLevel {
        #[serde(rename = "sessionId")]
        session_id: String,
        level: f32,
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
