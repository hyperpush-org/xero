use std::sync::Arc;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use tauri::{AppHandle, Manager, Runtime};

use crate::commands::emulator::{
    self,
    automation::{
        BundleIdRequest, HardwareKeyRequest, InstallAppRequest, LaunchAppRequest, LocationRequest,
        LogSubscribeRequest, PushNotificationRequest, Selector, SubscriptionToken, SwipeRequest,
        TapTarget, TypeRequest,
    },
    EmulatorInputRequest, EmulatorRotateRequest, EmulatorState,
};
use crate::commands::{CommandError, CommandResult};

pub const AUTONOMOUS_TOOL_EMULATOR: &str = "emulator";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousEmulatorAction {
    SdkStatus,
    AndroidProvision,
    AndroidProvisionStatus,
    ListDevices,
    Start,
    Stop,
    Input,
    Rotate,
    SubscribeReady,
    Screenshot,
    UiDump,
    Find,
    Tap,
    Swipe,
    Type,
    PressKey,
    ListApps,
    InstallApp,
    UninstallApp,
    LaunchApp,
    TerminateApp,
    SetLocation,
    PushNotification,
    LogsSubscribe,
    LogsUnsubscribe,
    LogsRecent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousEmulatorRequest {
    pub action: AutonomousEmulatorAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousEmulatorOutput {
    pub action: String,
    pub value_json: String,
}

pub trait EmulatorExecutor: Send + Sync + std::fmt::Debug {
    fn execute(
        &self,
        action: AutonomousEmulatorAction,
        input: Option<JsonValue>,
    ) -> CommandResult<AutonomousEmulatorOutput>;
}

pub fn execute_action_with_app<R: Runtime>(
    app: &AppHandle<R>,
    action: AutonomousEmulatorAction,
    input: Option<JsonValue>,
) -> CommandResult<AutonomousEmulatorOutput> {
    let action_name = emulator_action_name(&action).to_string();
    let value = match action {
        AutonomousEmulatorAction::SdkStatus => {
            ensure_empty_input(&action_name, input)?;
            to_json_value(emulator::emulator_sdk_status(app.clone())?)?
        }
        AutonomousEmulatorAction::AndroidProvision => {
            ensure_empty_input(&action_name, input)?;
            emulator::emulator_android_provision(app.clone())?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::AndroidProvisionStatus => {
            ensure_empty_input(&action_name, input)?;
            to_json_value(emulator::emulator_android_provision_status(app.clone())?)?
        }
        AutonomousEmulatorAction::ListDevices => to_json_value(emulator::emulator_list_devices(
            app.clone(),
            decode_input(&action_name, input)?,
        )?)?,
        AutonomousEmulatorAction::Start => {
            let state = emulator_state(app)?;
            to_json_value(emulator::emulator_start(
                app.clone(),
                state,
                decode_input(&action_name, input)?,
            )?)?
        }
        AutonomousEmulatorAction::Stop => {
            ensure_empty_input(&action_name, input)?;
            let state = emulator_state(app)?;
            emulator::emulator_stop(app.clone(), state)?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::Input => {
            let state = emulator_state(app)?;
            emulator::emulator_input(
                state,
                decode_input::<EmulatorInputRequest>(&action_name, input)?,
            )?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::Rotate => {
            let state = emulator_state(app)?;
            emulator::emulator_rotate(
                state,
                decode_input::<EmulatorRotateRequest>(&action_name, input)?,
            )?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::SubscribeReady => {
            ensure_empty_input(&action_name, input)?;
            let state = emulator_state(app)?;
            to_json_value(emulator::emulator_subscribe_ready(app.clone(), state)?)?
        }
        AutonomousEmulatorAction::Screenshot => {
            ensure_empty_input(&action_name, input)?;
            let state = emulator_state(app)?;
            to_json_value(emulator::emulator_screenshot(state)?)?
        }
        AutonomousEmulatorAction::UiDump => {
            ensure_empty_input(&action_name, input)?;
            let state = emulator_state(app)?;
            to_json_value(emulator::emulator_ui_dump(state)?)?
        }
        AutonomousEmulatorAction::Find => {
            let state = emulator_state(app)?;
            to_json_value(emulator::emulator_find(
                state,
                decode_input::<Selector>(&action_name, input)?,
            )?)?
        }
        AutonomousEmulatorAction::Tap => {
            let state = emulator_state(app)?;
            emulator::emulator_tap(state, decode_input::<TapTarget>(&action_name, input)?)?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::Swipe => {
            let state = emulator_state(app)?;
            emulator::emulator_swipe(state, decode_input::<SwipeRequest>(&action_name, input)?)?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::Type => {
            let state = emulator_state(app)?;
            emulator::emulator_type(state, decode_input::<TypeRequest>(&action_name, input)?)?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::PressKey => {
            let state = emulator_state(app)?;
            emulator::emulator_press_key(
                state,
                decode_input::<HardwareKeyRequest>(&action_name, input)?,
            )?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::ListApps => {
            ensure_empty_input(&action_name, input)?;
            let state = emulator_state(app)?;
            to_json_value(emulator::emulator_list_apps(state)?)?
        }
        AutonomousEmulatorAction::InstallApp => {
            let state = emulator_state(app)?;
            to_json_value(emulator::emulator_install_app(
                state,
                decode_input::<InstallAppRequest>(&action_name, input)?,
            )?)?
        }
        AutonomousEmulatorAction::UninstallApp => {
            let state = emulator_state(app)?;
            emulator::emulator_uninstall_app(
                state,
                decode_input::<BundleIdRequest>(&action_name, input)?,
            )?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::LaunchApp => {
            let state = emulator_state(app)?;
            emulator::emulator_launch_app(
                state,
                decode_input::<LaunchAppRequest>(&action_name, input)?,
            )?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::TerminateApp => {
            let state = emulator_state(app)?;
            emulator::emulator_terminate_app(
                state,
                decode_input::<BundleIdRequest>(&action_name, input)?,
            )?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::SetLocation => {
            let state = emulator_state(app)?;
            emulator::emulator_set_location(
                state,
                decode_input::<LocationRequest>(&action_name, input)?,
            )?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::PushNotification => {
            let state = emulator_state(app)?;
            emulator::emulator_push_notification(
                state,
                decode_input::<PushNotificationRequest>(&action_name, input)?,
            )?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::LogsSubscribe => {
            let state = emulator_state(app)?;
            to_json_value(emulator::emulator_logs_subscribe(
                app.clone(),
                state,
                decode_input::<LogSubscribeRequest>(&action_name, input)?,
            )?)?
        }
        AutonomousEmulatorAction::LogsUnsubscribe => {
            let state = emulator_state(app)?;
            emulator::emulator_logs_unsubscribe(
                state,
                decode_input::<SubscriptionToken>(&action_name, input)?,
            )?;
            JsonValue::Null
        }
        AutonomousEmulatorAction::LogsRecent => {
            let state = emulator_state(app)?;
            let input = decode_input::<RecentLogsInput>(&action_name, input)?;
            to_json_value(emulator::emulator_logs_get_recent(state, input.limit)?)?
        }
    };

    Ok(AutonomousEmulatorOutput {
        action: action_name,
        value_json: serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string()),
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecentLogsInput {
    #[serde(default)]
    limit: Option<usize>,
}

fn emulator_state<R: Runtime>(
    app: &AppHandle<R>,
) -> CommandResult<tauri::State<'_, EmulatorState>> {
    app.try_state::<EmulatorState>().ok_or_else(|| {
        CommandError::system_fault(
            "emulator_executor_state_missing",
            "Emulator state is not registered on the app handle.",
        )
    })
}

fn decode_input<T: DeserializeOwned>(
    action_name: &str,
    input: Option<JsonValue>,
) -> CommandResult<T> {
    serde_json::from_value(input.unwrap_or_else(|| JsonValue::Object(JsonMap::new()))).map_err(
        |error| {
            CommandError::user_fixable(
                "autonomous_emulator_input_invalid",
                format!("Cadence could not decode emulator action `{action_name}` input: {error}"),
            )
        },
    )
}

fn ensure_empty_input(action_name: &str, input: Option<JsonValue>) -> CommandResult<()> {
    match input {
        None => Ok(()),
        Some(JsonValue::Object(map)) if map.is_empty() => Ok(()),
        Some(JsonValue::Null) => Ok(()),
        Some(_) => Err(CommandError::user_fixable(
            "autonomous_emulator_input_invalid",
            format!("Emulator action `{action_name}` does not accept input."),
        )),
    }
}

fn to_json_value(value: impl Serialize) -> CommandResult<JsonValue> {
    serde_json::to_value(value).map_err(|error| {
        CommandError::system_fault(
            "autonomous_emulator_output_serialize_failed",
            format!("Cadence could not serialize emulator tool output: {error}"),
        )
    })
}

fn emulator_action_name(action: &AutonomousEmulatorAction) -> &'static str {
    match action {
        AutonomousEmulatorAction::SdkStatus => "sdk_status",
        AutonomousEmulatorAction::AndroidProvision => "android_provision",
        AutonomousEmulatorAction::AndroidProvisionStatus => "android_provision_status",
        AutonomousEmulatorAction::ListDevices => "list_devices",
        AutonomousEmulatorAction::Start => "start",
        AutonomousEmulatorAction::Stop => "stop",
        AutonomousEmulatorAction::Input => "input",
        AutonomousEmulatorAction::Rotate => "rotate",
        AutonomousEmulatorAction::SubscribeReady => "subscribe_ready",
        AutonomousEmulatorAction::Screenshot => "screenshot",
        AutonomousEmulatorAction::UiDump => "ui_dump",
        AutonomousEmulatorAction::Find => "find",
        AutonomousEmulatorAction::Tap => "tap",
        AutonomousEmulatorAction::Swipe => "swipe",
        AutonomousEmulatorAction::Type => "type",
        AutonomousEmulatorAction::PressKey => "press_key",
        AutonomousEmulatorAction::ListApps => "list_apps",
        AutonomousEmulatorAction::InstallApp => "install_app",
        AutonomousEmulatorAction::UninstallApp => "uninstall_app",
        AutonomousEmulatorAction::LaunchApp => "launch_app",
        AutonomousEmulatorAction::TerminateApp => "terminate_app",
        AutonomousEmulatorAction::SetLocation => "set_location",
        AutonomousEmulatorAction::PushNotification => "push_notification",
        AutonomousEmulatorAction::LogsSubscribe => "logs_subscribe",
        AutonomousEmulatorAction::LogsUnsubscribe => "logs_unsubscribe",
        AutonomousEmulatorAction::LogsRecent => "logs_recent",
    }
}

#[derive(Debug, Default)]
pub struct UnavailableEmulatorExecutor;

impl EmulatorExecutor for UnavailableEmulatorExecutor {
    fn execute(
        &self,
        _action: AutonomousEmulatorAction,
        _input: Option<JsonValue>,
    ) -> CommandResult<AutonomousEmulatorOutput> {
        Err(CommandError::policy_denied(
            "Emulator actions require the desktop runtime and an active emulator session.",
        ))
    }
}

pub fn tauri_emulator_executor<R: Runtime>(app: AppHandle<R>) -> Arc<dyn EmulatorExecutor> {
    Arc::new(TauriEmulatorExecutor { app })
}

struct TauriEmulatorExecutor<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> std::fmt::Debug for TauriEmulatorExecutor<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TauriEmulatorExecutor").finish()
    }
}

impl<R: Runtime> EmulatorExecutor for TauriEmulatorExecutor<R> {
    fn execute(
        &self,
        action: AutonomousEmulatorAction,
        input: Option<JsonValue>,
    ) -> CommandResult<AutonomousEmulatorOutput> {
        execute_action_with_app(&self.app, action, input)
    }
}

pub fn emulator_schema() -> JsonValue {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["action"],
        "properties": {
            "action": {
                "type": "string",
                "description": "Emulator/mobile app action to execute.",
                "enum": [
                    "sdk_status",
                    "android_provision",
                    "android_provision_status",
                    "list_devices",
                    "start",
                    "stop",
                    "input",
                    "rotate",
                    "subscribe_ready",
                    "screenshot",
                    "ui_dump",
                    "find",
                    "tap",
                    "swipe",
                    "type",
                    "press_key",
                    "list_apps",
                    "install_app",
                    "uninstall_app",
                    "launch_app",
                    "terminate_app",
                    "set_location",
                    "push_notification",
                    "logs_subscribe",
                    "logs_unsubscribe",
                    "logs_recent"
                ]
            },
            "input": {
                "type": "object",
                "description": "Action-specific input object matching the corresponding emulator Tauri command payload."
            }
        }
    })
}
