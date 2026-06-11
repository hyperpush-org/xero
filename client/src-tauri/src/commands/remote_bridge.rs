use std::{
    collections::{BTreeSet, HashSet, VecDeque},
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{
            Receiver as InboundReceiver, RecvTimeoutError as InboundRecvTimeoutError,
            TryRecvError as InboundTryRecvError,
        },
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use xero_remote_bridge::{
    AccountDevice, AuthStatus, BridgeAccount, BridgeConfig, BridgeError, BridgeResult,
    BridgeTelemetry, DesktopBridgeLoopOptions, FileIdentityStore, IdentityStore, InboundCommand,
    InboundCommandKind, RemoteBridge,
};

use crate::{
    commands::{
        agent_run_dto,
        agent_session::{agent_session_dto, stop_idle_owned_runtime_run_before_archive},
        desktop_control::load_desktop_control_settings,
        global_computer_use::{
            ensure_global_computer_use_session_record, reset_global_computer_use_session_record,
            GLOBAL_COMPUTER_USE_AGENT_SESSION_ID, GLOBAL_COMPUTER_USE_PROJECT_ID,
            REMOTE_COMPUTER_USE_SESSION_ID,
        },
        list_projects::load_visible_project_summaries_from_registry,
        project_state::{read_app_ui_state_value, write_app_ui_state_value},
        provider_credentials::load_provider_credentials_view,
        resolve_operator_action::resolve_operator_action_blocking,
        runtime_media::{
            extract_runtime_media_attachments, read_runtime_media_artifact,
            RemoteRuntimeMediaContext, RuntimeMediaExtractionRequest,
        },
        runtime_support::{
            emit_project_updated, load_persisted_runtime_run, resolve_project_root,
            runtime_run_dto_from_snapshot,
        },
        session_history::build_session_context_snapshot,
        stage_agent_attachment::{
            discard_agent_attachment_blocking, stage_agent_attachment_blocking,
        },
        start_runtime_run::start_runtime_run_blocking,
        stop_runtime_run::stop_runtime_run_blocking,
        update_runtime_run_controls::update_runtime_run_controls_blocking,
        validate_non_empty, AgentRunDto, AgentRunEventKindDto, CommandError, CommandResult,
        DiscardAgentAttachmentRequestDto, ProjectUpdateReason, ProviderModelThinkingEffortDto,
        ResolveOperatorActionRequestDto, RuntimeAgentIdDto, RuntimeRunApprovalModeDto,
        RuntimeRunControlInputDto, StageAgentAttachmentRequestDto, StagedAgentAttachmentDto,
        StartRuntimeRunRequestDto, StopRuntimeRunRequestDto, UpdateRuntimeRunControlsRequestDto,
    },
    db::project_store::{
        self, AgentEventRecord, AgentSessionCreateRecord, AgentSessionRecord,
        COMPUTER_USE_AGENT_SESSION_TITLE, DEFAULT_AGENT_SESSION_TITLE,
    },
    provider_models::{
        load_provider_model_catalog, ProviderModelRecord, ProviderModelThinkingCapability,
        ProviderModelThinkingEffort,
    },
    registry::RegistryProjectSummaryRecord,
    runtime::{
        AutonomousDesktopControlAction, AutonomousDesktopControlRequest,
        AutonomousDesktopIceCandidate, AutonomousDesktopIceServer, AutonomousDesktopMouseButton,
        AutonomousDesktopObserveAction, AutonomousDesktopObserveRequest,
        AutonomousDesktopScreenshot, AutonomousDesktopSessionDescription,
        AutonomousDesktopStreamAction, AutonomousDesktopStreamQuality,
        AutonomousDesktopStreamRequest, AutonomousDesktopStreamTransport,
        AutonomousDesktopToolOutput, AutonomousDesktopToolStatus, AutonomousToolOutput,
        AutonomousToolRuntime,
    },
    state::DesktopState,
};

const REMOTE_DIR: &str = "remote";
const IDENTITY_FILE: &str = "desktop-identity.json";
const THEME_CONTROL_SESSION_ID: &str = "__theme__";
const THEME_APP_STATE_KEY: &str = "theme.active.v1";
const CUSTOM_THEMES_APP_STATE_KEY: &str = "theme.custom.v1";
const DEFAULT_THEME_ID: &str = "dusk";
const CUSTOM_THEME_ID_PREFIX: &str = "custom-";
const COMPOSER_SETTINGS_APP_STATE_KEY: &str = "xero.agent.composer.settings.v1";
const COMPOSER_SETTINGS_UPDATED_EVENT: &str = "agent:composer_settings_updated";
const COMPOSER_SETTINGS_VERSION: u64 = 1;
const PROJECT_REMOTE_SESSION_ID_PREFIX: &str = "project:";
const STREAM_FALLBACK_FRAME_MAX_BYTES: usize = 5 * 1024 * 1024;
const STREAM_FALLBACK_JPEG_QUALITY: u8 = 74;
const COMMAND_OUTCOME_SCHEMA: &str = "xero.remote_command_outcome.v1";
const MAX_DEDUPE_COMMAND_IDS: usize = 2048;
const INBOUND_COMMAND_WAIT_TIMEOUT: Duration = Duration::from_millis(100);

type AppRemoteBridge = RemoteBridge<FileIdentityStore>;

#[derive(Default)]
pub struct RemoteBridgeRuntimeState {
    bridge: Mutex<Option<Arc<AppRemoteBridge>>>,
    shutdown: Mutex<Option<Arc<AtomicBool>>>,
    worker: Mutex<Option<std::thread::JoinHandle<BridgeResult<()>>>>,
    inbound_worker: Mutex<Option<std::thread::JoinHandle<()>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatusResponseDto {
    pub schema: String,
    pub connected: bool,
    pub relay_url: String,
    pub signed_in: bool,
    pub account: Option<BridgeAccount>,
    pub devices: Vec<AccountDevice>,
    pub telemetry: BridgeTelemetry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub devices_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeRevokeDeviceRequestDto {
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgePollGithubLoginRequestDto {
    pub flow_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeThemeSyncRequestDto {
    pub theme_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_theme: Option<JsonValue>,
}

#[tauri::command]
pub fn bridge_status<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    remote_state: State<'_, RemoteBridgeRuntimeState>,
) -> CommandResult<BridgeStatusResponseDto> {
    let bridge = remote_state.bridge(&app, state.inner())?;
    remote_state.start_if_registered(&app, state.inner())?;
    let status = bridge.status().map_err(map_bridge_error)?;

    Ok(BridgeStatusResponseDto {
        schema: "xero.remote_bridge_status.v1".into(),
        connected: status.connected,
        relay_url: status.relay_url,
        signed_in: status.signed_in,
        account: status.account,
        devices: status.devices,
        telemetry: status.telemetry,
        devices_error: status.devices_error,
    })
}

#[tauri::command]
pub fn bridge_sign_in<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    remote_state: State<'_, RemoteBridgeRuntimeState>,
) -> CommandResult<AuthStatus> {
    let bridge = remote_state.bridge(&app, state.inner())?;
    bridge.sign_in_with_github().map_err(map_bridge_error)
}

#[tauri::command]
pub fn bridge_poll_github_login<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    remote_state: State<'_, RemoteBridgeRuntimeState>,
    request: BridgePollGithubLoginRequestDto,
) -> CommandResult<AuthStatus> {
    validate_non_empty(&request.flow_id, "flowId")?;
    let bridge = remote_state.bridge(&app, state.inner())?;
    let status = bridge
        .poll_github_login(request.flow_id.trim())
        .map_err(map_bridge_error)?;
    if status.signed_in {
        remote_state.start_if_registered(&app, state.inner())?;
    }
    Ok(status)
}

#[tauri::command]
pub fn bridge_sign_out<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    remote_state: State<'_, RemoteBridgeRuntimeState>,
) -> CommandResult<()> {
    let bridge = remote_state.bridge(&app, state.inner())?;
    bridge.sign_out().map_err(map_bridge_error)
}

#[tauri::command]
pub fn bridge_revoke_device<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    remote_state: State<'_, RemoteBridgeRuntimeState>,
    request: BridgeRevokeDeviceRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.device_id, "deviceId")?;
    let bridge = remote_state.bridge(&app, state.inner())?;
    bridge
        .revoke_device(request.device_id.trim())
        .map_err(map_bridge_error)
}

#[tauri::command]
pub fn bridge_publish_theme<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    remote_state: State<'_, RemoteBridgeRuntimeState>,
    request: BridgeThemeSyncRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.theme_id, "themeId")?;
    let theme_id = request.theme_id.trim().to_string();
    let bridge = remote_state.bridge(&app, state.inner())?;
    if !registered_identity_exists(&app, state.inner())? {
        return Ok(());
    }
    remote_state.start_if_registered(&app, state.inner())?;
    publish_theme_to_cloud(
        &bridge,
        &theme_id,
        custom_theme_for_theme_id(&theme_id, request.custom_theme),
    )
    .map_err(map_bridge_error)
}

pub fn start_remote_bridge_if_registered<R: Runtime + 'static>(
    app: &AppHandle<R>,
) -> CommandResult<()> {
    let state = app.state::<DesktopState>();
    let remote_state = app.state::<RemoteBridgeRuntimeState>();
    remote_state
        .start_if_registered(app, state.inner())
        .map(|_| ())
}

pub fn shutdown_on_close<R: Runtime>(app: &AppHandle<R>) {
    app.state::<RemoteBridgeRuntimeState>().shutdown();
}

pub fn publish_remote_project_list_to_cloud<R: Runtime>(app: &AppHandle<R>, state: &DesktopState) {
    let Some(bridge) = runtime_event_forwarder() else {
        return;
    };
    let path = match state.global_db_path(app) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("[remote-bridge] project list publish skipped: {error}");
            return;
        }
    };
    let projects = match load_visible_project_summaries_from_registry(&path)
        .and_then(remote_project_summaries_from_projects)
    {
        Ok(projects) => projects,
        Err(error) => {
            eprintln!("[remote-bridge] project list publish skipped: {error}");
            return;
        }
    };
    if let Err(error) = bridge.forward_control_event(
        "__projects__",
        json!({
            "schema": "xero.remote_projects.v1",
            "projects": projects,
        }),
    ) {
        eprintln!("[remote-bridge] project list publish skipped: {error}");
    }
}

fn publish_current_theme_to_cloud<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
) -> CommandResult<()> {
    let theme_id = read_app_ui_state_value(app, state, THEME_APP_STATE_KEY)?
        .and_then(|value| value.as_str().map(str::trim).map(ToOwned::to_owned))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_THEME_ID.to_string());
    let custom_theme = if theme_id.starts_with(CUSTOM_THEME_ID_PREFIX) {
        read_app_ui_state_value(app, state, CUSTOM_THEMES_APP_STATE_KEY)?
            .and_then(|value| custom_theme_from_state_value(&theme_id, value))
    } else {
        None
    };
    publish_theme_to_cloud(bridge, &theme_id, custom_theme).map_err(map_bridge_error)
}

fn publish_theme_to_cloud(
    bridge: &AppRemoteBridge,
    theme_id: &str,
    custom_theme: Option<JsonValue>,
) -> BridgeResult<()> {
    let mut payload = json!({
        "schema": "xero.cloud_theme.v1",
        "themeId": theme_id,
    });
    if let Some(custom_theme) = custom_theme_for_theme_id(theme_id, custom_theme) {
        payload["customTheme"] = custom_theme;
    }
    bridge.forward_control_event(THEME_CONTROL_SESSION_ID, payload)?;
    Ok(())
}

fn custom_theme_for_theme_id(theme_id: &str, custom_theme: Option<JsonValue>) -> Option<JsonValue> {
    if theme_id.starts_with(CUSTOM_THEME_ID_PREFIX) {
        custom_theme
    } else {
        None
    }
}

fn custom_theme_from_state_value(theme_id: &str, value: JsonValue) -> Option<JsonValue> {
    value.as_array().and_then(|themes| {
        themes
            .iter()
            .find(|theme| theme.get("id").and_then(JsonValue::as_str) == Some(theme_id))
            .cloned()
    })
}

pub(crate) fn publish_agent_session_remote_state<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    session: &AgentSessionRecord,
) {
    if let Err(error) = publish_agent_session_remote_state_inner(app, state, project_id, session) {
        eprintln!("[remote-bridge] session publish skipped: {error}");
    }
}

fn publish_agent_session_remote_state_inner<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    session: &AgentSessionRecord,
) -> CommandResult<()> {
    if matches!(
        session.session_kind,
        project_store::AgentSessionKind::ComputerUse
    ) {
        return Ok(());
    }

    let Some(bridge) = runtime_event_forwarder() else {
        return Ok(());
    };
    let Some(project_name) = project_name_for_id(app, state, project_id)? else {
        return Ok(());
    };
    let payload = remote_session_result_payload(project_id, Some(&project_name), session);
    bridge
        .forward_control_event(
            "__sessions__",
            json!({
                "schema": "xero.remote_session_added.v1",
                "result": payload,
            }),
        )
        .map_err(map_bridge_error)?;
    Ok(())
}

pub fn forward_agent_event(repo_root: &Path, event: &AgentEventRecord) {
    let Some(bridge) = runtime_event_forwarder() else {
        return;
    };
    let run =
        match project_store::load_agent_run_record(repo_root, &event.project_id, &event.run_id) {
            Ok(run) => run,
            Err(error) => {
                eprintln!("[remote-bridge] runtime event lookup skipped: {error}");
                return;
            }
        };
    let mut payload = serde_json::from_str(&event.payload_json).unwrap_or_else(|_| {
        json!({
            "raw": event.payload_json,
        })
    });
    let event_kind = serde_json::to_value(&event.event_kind)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{:?}", event.event_kind));
    let remote_session_id = remote_session_id_for(&event.project_id, &run.agent_session_id);
    if matches!(
        event.event_kind,
        project_store::AgentRunEventKind::ToolCompleted
    ) {
        if let (Some(output), Some(computer_id)) =
            (payload.get("output"), bridge.computer_id().ok().flatten())
        {
            let attachments = extract_runtime_media_attachments(RuntimeMediaExtractionRequest {
                repo_root,
                project_id: &event.project_id,
                run_id: &event.run_id,
                event_id: event.id,
                tool_call_id: payload.get("toolCallId").and_then(JsonValue::as_str),
                tool_name: payload.get("toolName").and_then(JsonValue::as_str),
                output,
                asset_state: None,
                remote_context: Some(RemoteRuntimeMediaContext {
                    computer_id: &computer_id,
                    session_id: &remote_session_id,
                }),
            });
            if !attachments.is_empty() {
                if let Some(object) = payload.as_object_mut() {
                    object.insert("mediaAttachments".into(), json!(attachments));
                    object.remove("modelVisibleToolResult");
                }
            }
        }
    }
    let runtime_event = json!({
        "schema": "xero.remote_runtime_event.v1",
        "projectId": &event.project_id,
        "agentSessionId": &run.agent_session_id,
        "runId": &event.run_id,
        "eventId": event.id,
        "eventKind": event_kind,
        "payload": payload,
        "createdAt": &event.created_at,
    });
    if let Err(error) = bridge.forward(&remote_session_id, runtime_event) {
        eprintln!("[remote-bridge] runtime event forward skipped: {error}");
    }
}

pub(crate) fn handle_deleted_agent_session_remote_state<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    remote_state: &RemoteBridgeRuntimeState,
    project_id: &str,
    session: &AgentSessionRecord,
) {
    if let Err(error) =
        publish_deleted_agent_session_remote_state(app, state, remote_state, project_id, session)
    {
        eprintln!("[remote-bridge] session delete notification skipped: {error}");
    }
}

fn publish_deleted_agent_session_remote_state<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    remote_state: &RemoteBridgeRuntimeState,
    project_id: &str,
    session: &AgentSessionRecord,
) -> CommandResult<()> {
    let bridge = remote_state.bridge(app, state)?;
    if !registered_identity_exists(app, state)? {
        return Ok(());
    }

    remote_state.start_if_registered(app, state)?;
    let remote_session_id = remote_session_id_for(project_id, &session.agent_session_id);
    bridge
        .forward_session_removed(
            &remote_session_id,
            json!({
                "schema": "xero.remote_session_removed.v1",
                "projectId": project_id,
                "remoteSessionId": remote_session_id,
                "sessionId": &session.agent_session_id,
                "agentSessionId": &session.agent_session_id,
            }),
        )
        .map_err(map_bridge_error)?;
    Ok(())
}

impl RemoteBridgeRuntimeState {
    fn bridge<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        state: &DesktopState,
    ) -> CommandResult<Arc<AppRemoteBridge>> {
        let mut guard = self.bridge.lock().map_err(|_| {
            CommandError::system_fault(
                "remote_bridge_state_lock_failed",
                "Xero could not lock the remote bridge runtime state.",
            )
        })?;
        if let Some(bridge) = guard.as_ref() {
            return Ok(Arc::clone(bridge));
        }

        let bridge = Arc::new(new_bridge_for_app(app, state)?);
        *guard = Some(Arc::clone(&bridge));
        Ok(bridge)
    }

    fn start_if_registered<R: Runtime + 'static>(
        &self,
        app: &AppHandle<R>,
        state: &DesktopState,
    ) -> CommandResult<Option<Arc<AppRemoteBridge>>> {
        if !registered_identity_exists(app, state)? {
            return Ok(None);
        }
        self.ensure_started(app, state).map(Some)
    }

    fn ensure_started<R: Runtime + 'static>(
        &self,
        app: &AppHandle<R>,
        state: &DesktopState,
    ) -> CommandResult<Arc<AppRemoteBridge>> {
        let bridge = self.bridge(app, state)?;
        let mut worker = self.worker.lock().map_err(|_| {
            CommandError::system_fault(
                "remote_bridge_worker_lock_failed",
                "Xero could not lock the remote bridge worker state.",
            )
        })?;
        if worker.as_ref().is_some_and(|handle| !handle.is_finished()) {
            return Ok(bridge);
        }

        let shutdown = Arc::new(AtomicBool::new(false));
        let handle = Arc::clone(&bridge)
            .spawn_desktop_loop(Arc::clone(&shutdown), DesktopBridgeLoopOptions::default());
        set_runtime_event_forwarder(Arc::clone(&bridge));
        self.ensure_inbound_worker(app, state, Arc::clone(&bridge), Arc::clone(&shutdown))?;
        *self.shutdown.lock().map_err(|_| {
            CommandError::system_fault(
                "remote_bridge_shutdown_lock_failed",
                "Xero could not lock the remote bridge shutdown state.",
            )
        })? = Some(shutdown);
        *worker = Some(handle);
        Ok(bridge)
    }

    fn ensure_inbound_worker<R: Runtime + 'static>(
        &self,
        app: &AppHandle<R>,
        state: &DesktopState,
        bridge: Arc<AppRemoteBridge>,
        shutdown: Arc<AtomicBool>,
    ) -> CommandResult<()> {
        let mut worker = self.inbound_worker.lock().map_err(|_| {
            CommandError::system_fault(
                "remote_bridge_inbound_worker_lock_failed",
                "Xero could not lock the remote bridge inbound worker state.",
            )
        })?;
        if worker.as_ref().is_some_and(|handle| !handle.is_finished()) {
            return Ok(());
        }

        let inbound = bridge.subscribe_inbound().map_err(map_bridge_error)?;
        let app = app.clone();
        let state = state.clone();
        let handle = thread::spawn(move || {
            let mut pending = VecDeque::new();
            while !shutdown.load(Ordering::Relaxed) {
                let command = match pending.pop_front() {
                    Some(command) => Some(command),
                    None => match inbound.recv_timeout(INBOUND_COMMAND_WAIT_TIMEOUT) {
                        Ok(command) => Some(command),
                        Err(InboundRecvTimeoutError::Timeout) => None,
                        Err(InboundRecvTimeoutError::Disconnected) => break,
                    },
                };
                let Some(command) = command else {
                    continue;
                };
                let command = coalesce_inbound_pointer_command(command, &inbound, &mut pending);
                if let Err(error) =
                    handle_inbound_command(&app, &state, Arc::clone(&bridge), command)
                {
                    eprintln!("[remote-bridge] inbound command failed: {error}");
                }
            }
        });
        *worker = Some(handle);
        Ok(())
    }

    fn shutdown(&self) {
        if let Ok(mut shutdown) = self.shutdown.lock() {
            if let Some(flag) = shutdown.take() {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }
}

fn coalesce_inbound_pointer_command(
    command: InboundCommand,
    inbound: &InboundReceiver<InboundCommand>,
    pending: &mut VecDeque<InboundCommand>,
) -> InboundCommand {
    let Some(coalesce_key) = inbound_pointer_coalesce_key(&command) else {
        return command;
    };
    let mut latest = command;
    loop {
        match inbound.try_recv() {
            Ok(next) if inbound_pointer_coalesce_key(&next).as_deref() == Some(&coalesce_key) => {
                latest = next;
            }
            Ok(next) => {
                pending.push_back(next);
                break;
            }
            Err(InboundTryRecvError::Empty) | Err(InboundTryRecvError::Disconnected) => break,
        }
    }
    latest
}

fn inbound_pointer_coalesce_key(command: &InboundCommand) -> Option<String> {
    if command.kind != InboundCommandKind::ComputerUseManualControlInput {
        return None;
    }
    let action = payload_string(&command.payload, &["action"])?;
    if action != "mouse_move" && action != "mouse_drag_move" {
        return None;
    }
    Some(format!(
        "{}:{}:{}",
        command.session_id.as_deref().unwrap_or_default(),
        payload_string(&command.payload, &["manualControlId", "manual_control_id"])
            .unwrap_or_default(),
        action,
    ))
}

fn set_runtime_event_forwarder(bridge: Arc<AppRemoteBridge>) {
    if let Ok(mut forwarder) = runtime_event_forwarder_cell().lock() {
        *forwarder = Some(bridge);
    }
}

fn runtime_event_forwarder() -> Option<Arc<AppRemoteBridge>> {
    runtime_event_forwarder_cell()
        .lock()
        .ok()
        .and_then(|forwarder| forwarder.as_ref().map(Arc::clone))
}

fn runtime_event_forwarder_cell() -> &'static Mutex<Option<Arc<AppRemoteBridge>>> {
    static FORWARDER: OnceLock<Mutex<Option<Arc<AppRemoteBridge>>>> = OnceLock::new();
    FORWARDER.get_or_init(|| Mutex::new(None))
}

fn handle_inbound_command<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: Arc<AppRemoteBridge>,
    command: InboundCommand,
) -> CommandResult<()> {
    let response_session = command
        .session_id
        .as_deref()
        .unwrap_or("__sessions__")
        .to_string();
    if let Some(error) = command_freshness_error(&command) {
        let outcome = if error.code == "remote_command_expired" {
            "stale"
        } else {
            "rejected"
        };
        bridge
            .forward_control_event(
                &response_session,
                command_outcome_payload(&command, outcome, Some(error.code.as_str())),
            )
            .map_err(map_bridge_error)?;
        return Err(error);
    }
    if command_is_duplicate(&command) {
        bridge
            .forward_control_event(
                &response_session,
                command_outcome_payload(&command, "duplicate", Some("duplicate_command")),
            )
            .map_err(map_bridge_error)?;
        return Ok(());
    }
    let result = route_inbound_command(app, state, Arc::clone(&bridge), command.clone());
    if let Err(error) = &result {
        let mut payload = json!({
            "schema": "xero.remote_command_result.v1",
            "ok": false,
            "error": error,
        });
        attach_command_context(&mut payload, &command, Some("rejected"));
        let _ = bridge.forward_control_event(&response_session, payload);
    }
    if let Err(error) = &result {
        let _ = bridge.forward_control_event(
            &response_session,
            command_outcome_payload(&command, "rejected", Some(error.code.as_str())),
        );
    }
    result.map(|_| ())
}

#[derive(Default)]
struct CommandDedupeState {
    seen: HashSet<String>,
    order: VecDeque<String>,
}

fn command_is_duplicate(command: &InboundCommand) -> bool {
    if !is_critical_command(&command.kind) {
        return false;
    }
    let Some(key) = command_dedupe_key(command) else {
        return false;
    };
    let Ok(mut state) = command_dedupe_state().lock() else {
        return false;
    };
    if state.seen.contains(&key) {
        return true;
    }
    state.seen.insert(key.clone());
    state.order.push_back(key);
    while state.order.len() > MAX_DEDUPE_COMMAND_IDS {
        if let Some(expired) = state.order.pop_front() {
            state.seen.remove(&expired);
        }
    }
    false
}

fn command_dedupe_state() -> &'static Mutex<CommandDedupeState> {
    static STATE: OnceLock<Mutex<CommandDedupeState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(CommandDedupeState::default()))
}

fn command_dedupe_key(command: &InboundCommand) -> Option<String> {
    command
        .client_command_id
        .as_deref()
        .or(command.dedupe_key.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn is_critical_command(kind: &InboundCommandKind) -> bool {
    matches!(
        kind,
        InboundCommandKind::ComputerUseManualControlRequest
            | InboundCommandKind::ComputerUseManualControlGrant
            | InboundCommandKind::ComputerUseManualControlHeartbeat
            | InboundCommandKind::ComputerUseManualControlInput
            | InboundCommandKind::ComputerUseManualControlRelease
            | InboundCommandKind::ComputerUseStreamOffer
            | InboundCommandKind::ComputerUseStreamAnswer
            | InboundCommandKind::ComputerUseStreamIceCandidate
    )
}

fn command_requires_freshness(kind: &InboundCommandKind) -> bool {
    matches!(
        kind,
        InboundCommandKind::ComputerUseManualControlRequest
            | InboundCommandKind::ComputerUseManualControlGrant
            | InboundCommandKind::ComputerUseManualControlHeartbeat
            | InboundCommandKind::ComputerUseManualControlInput
            | InboundCommandKind::ComputerUseManualControlRelease
            | InboundCommandKind::ComputerUseStreamRequest
            | InboundCommandKind::ComputerUseStreamOffer
            | InboundCommandKind::ComputerUseStreamAnswer
            | InboundCommandKind::ComputerUseStreamIceCandidate
            | InboundCommandKind::ComputerUseStreamStop
            | InboundCommandKind::ComputerUseStreamStatus
            | InboundCommandKind::ComputerUseStreamSetQuality
            | InboundCommandKind::ComputerUseStreamRequestKeyframe
    )
}

fn command_freshness_error(command: &InboundCommand) -> Option<CommandError> {
    if !command_requires_freshness(&command.kind) {
        return None;
    }
    let Some(expires_at) = command.expires_at.as_ref() else {
        return Some(CommandError::new(
            "remote_command_expiry_missing",
            crate::commands::CommandErrorClass::PolicyDenied,
            "Remote Computer Use command was rejected because it did not include `expiresAt`.",
            false,
        ));
    };
    let Some(expires_at) = json_unix_millis(expires_at) else {
        return Some(CommandError::new(
            "remote_command_expiry_invalid",
            crate::commands::CommandErrorClass::PolicyDenied,
            "Remote Computer Use command was rejected because `expiresAt` was invalid.",
            false,
        ));
    };
    if expires_at <= current_unix_millis() {
        return Some(CommandError::new(
            "remote_command_expired",
            crate::commands::CommandErrorClass::PolicyDenied,
            "Remote Computer Use command was rejected because it expired before the desktop processed it.",
            false,
        ));
    }
    None
}

fn command_outcome_payload(
    command: &InboundCommand,
    outcome: &str,
    reason: Option<&str>,
) -> JsonValue {
    let mut payload = json!({
        "schema": COMMAND_OUTCOME_SCHEMA,
        "clientCommandId": command.client_command_id.as_deref(),
        "clientSeq": command.client_seq,
        "kind": command_kind_wire_value(&command.kind),
        "outcome": outcome,
        "priority": command_priority_wire_value(command),
        "reason": reason,
        "sentAt": command.sent_at.clone(),
        "dedupeKey": command.dedupe_key.as_deref(),
        "expiresAt": command.expires_at.clone(),
        "receivedAt": crate::auth::now_timestamp(),
    });
    attach_command_context(&mut payload, command, None);
    payload
}

fn attach_command_context(
    payload: &mut JsonValue,
    command: &InboundCommand,
    outcome: Option<&str>,
) {
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    object.insert(
        "clientCommandId".into(),
        command
            .client_command_id
            .as_deref()
            .map(JsonValue::from)
            .unwrap_or(JsonValue::Null),
    );
    object.insert("clientSeq".into(), json!(command.client_seq));
    object.insert(
        "kind".into(),
        JsonValue::from(command_kind_wire_value(&command.kind)),
    );
    object.insert(
        "priority".into(),
        JsonValue::from(command_priority_wire_value(command)),
    );
    object.insert(
        "sentAt".into(),
        command.sent_at.clone().unwrap_or(JsonValue::Null),
    );
    object.insert(
        "dedupeKey".into(),
        command
            .dedupe_key
            .as_deref()
            .map(JsonValue::from)
            .unwrap_or(JsonValue::Null),
    );
    object.insert(
        "expiresAt".into(),
        command.expires_at.clone().unwrap_or(JsonValue::Null),
    );
    if let Some(outcome) = outcome {
        object.insert("outcome".into(), JsonValue::from(outcome));
    }
    if let Some(stream_id) = payload_string(&command.payload, &["streamId", "stream_id"]) {
        object.insert("streamId".into(), JsonValue::from(stream_id));
    }
    if let Some(manual_control_id) =
        payload_string(&command.payload, &["manualControlId", "manual_control_id"])
    {
        object.insert("manualControlId".into(), JsonValue::from(manual_control_id));
    }
}

fn command_outcome_for_desktop_output(
    output: Option<&AutonomousDesktopToolOutput>,
) -> &'static str {
    match output.map(|output| output.status) {
        Some(AutonomousDesktopToolStatus::Executed | AutonomousDesktopToolStatus::Stopped) => {
            "executed"
        }
        Some(AutonomousDesktopToolStatus::Starting) | None => "accepted",
        Some(
            AutonomousDesktopToolStatus::ApprovalRequired
            | AutonomousDesktopToolStatus::Denied
            | AutonomousDesktopToolStatus::Unavailable
            | AutonomousDesktopToolStatus::Failed,
        ) => "rejected",
    }
}

fn command_kind_wire_value(kind: &InboundCommandKind) -> &'static str {
    match kind {
        InboundCommandKind::AuthorizeSessionJoin => "authorize_session_join",
        InboundCommandKind::SendMessage => "send_message",
        InboundCommandKind::ListSessions => "list_sessions",
        InboundCommandKind::ListProjects => "list_projects",
        InboundCommandKind::ArchiveSession => "archive_session",
        InboundCommandKind::SessionAttached => "session_attached",
        InboundCommandKind::StartSession => "start_session",
        InboundCommandKind::ResolveOperatorAction => "resolve_operator_action",
        InboundCommandKind::CancelRun => "cancel_run",
        InboundCommandKind::ContextSnapshot => "context_snapshot",
        InboundCommandKind::StageAttachment => "stage_attachment",
        InboundCommandKind::DiscardAttachment => "discard_attachment",
        InboundCommandKind::UpdateSessionControls => "update_session_controls",
        InboundCommandKind::FetchRuntimeMediaArtifact => "fetch_runtime_media_artifact",
        InboundCommandKind::ComputerUseStreamRequest => "computer_use_stream_request",
        InboundCommandKind::ComputerUseStreamOffer => "computer_use_stream_offer",
        InboundCommandKind::ComputerUseStreamAnswer => "computer_use_stream_answer",
        InboundCommandKind::ComputerUseStreamIceCandidate => "computer_use_stream_ice_candidate",
        InboundCommandKind::ComputerUseStreamStop => "computer_use_stream_stop",
        InboundCommandKind::ComputerUseStreamStatus => "computer_use_stream_status",
        InboundCommandKind::ComputerUseStreamSetQuality => "computer_use_stream_set_quality",
        InboundCommandKind::ComputerUseStreamRequestKeyframe => {
            "computer_use_stream_request_keyframe"
        }
        InboundCommandKind::ComputerUseManualControlRequest => {
            "computer_use_manual_control_request"
        }
        InboundCommandKind::ComputerUseManualControlGrant => "computer_use_manual_control_grant",
        InboundCommandKind::ComputerUseManualControlHeartbeat => {
            "computer_use_manual_control_heartbeat"
        }
        InboundCommandKind::ComputerUseManualControlInput => "computer_use_manual_control_input",
        InboundCommandKind::ComputerUseManualControlRelease => {
            "computer_use_manual_control_release"
        }
    }
}

fn command_priority_wire_value(command: &InboundCommand) -> &'static str {
    match command.priority {
        Some(xero_remote_bridge::InboundCommandPriority::CriticalReliable) => "critical_reliable",
        Some(xero_remote_bridge::InboundCommandPriority::CoalescedBestEffort) => {
            "coalesced_best_effort"
        }
        Some(xero_remote_bridge::InboundCommandPriority::ReliableIdempotent) => {
            "reliable_idempotent"
        }
        None if is_critical_command(&command.kind) => "critical_reliable",
        None => "reliable_idempotent",
    }
}

fn route_inbound_command<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: Arc<AppRemoteBridge>,
    command: InboundCommand,
) -> CommandResult<()> {
    if matches!(command.kind, InboundCommandKind::AuthorizeSessionJoin) {
        return route_authorize_session_join(app, state, &bridge, command);
    }
    ensure_known_web_device(&bridge, &command.device_id)?;
    match command.kind.clone() {
        InboundCommandKind::ListSessions => route_list_sessions(app, state, &bridge),
        InboundCommandKind::ListProjects => route_list_projects(app, state, &bridge),
        InboundCommandKind::AuthorizeSessionJoin => unreachable!("handled before device gate"),
        InboundCommandKind::ArchiveSession => route_archive_session(app, state, &bridge, command),
        InboundCommandKind::SessionAttached => route_session_attached(app, state, &bridge, command),
        InboundCommandKind::StartSession => route_start_session(app, state, &bridge, command),
        InboundCommandKind::SendMessage => route_send_message(app, state, &bridge, command),
        InboundCommandKind::ResolveOperatorAction => {
            route_resolve_operator_action(app, state, &bridge, command)
        }
        InboundCommandKind::CancelRun => route_cancel_run(app, state, &bridge, command),
        InboundCommandKind::ContextSnapshot => route_context_snapshot(app, state, &bridge, command),
        InboundCommandKind::StageAttachment => route_stage_attachment(app, state, &bridge, command),
        InboundCommandKind::DiscardAttachment => {
            route_discard_attachment(app, state, &bridge, command)
        }
        InboundCommandKind::UpdateSessionControls => {
            route_update_session_controls(app, state, &bridge, command)
        }
        InboundCommandKind::FetchRuntimeMediaArtifact => {
            route_fetch_runtime_media_artifact(app, state, &bridge, command)
        }
        InboundCommandKind::ComputerUseStreamRequest
        | InboundCommandKind::ComputerUseStreamOffer
        | InboundCommandKind::ComputerUseStreamAnswer
        | InboundCommandKind::ComputerUseStreamIceCandidate
        | InboundCommandKind::ComputerUseStreamStop
        | InboundCommandKind::ComputerUseStreamStatus
        | InboundCommandKind::ComputerUseStreamSetQuality
        | InboundCommandKind::ComputerUseStreamRequestKeyframe => {
            route_computer_use_stream_command(app, state, &bridge, command)
        }
        InboundCommandKind::ComputerUseManualControlRequest
        | InboundCommandKind::ComputerUseManualControlGrant
        | InboundCommandKind::ComputerUseManualControlHeartbeat
        | InboundCommandKind::ComputerUseManualControlInput
        | InboundCommandKind::ComputerUseManualControlRelease => {
            route_computer_use_manual_control_command(app, state, &bridge, command)
        }
    }
}

fn ensure_known_web_device(bridge: &AppRemoteBridge, device_id: &str) -> CommandResult<()> {
    validate_non_empty(device_id, "deviceId")?;
    let devices = bridge.list_account_devices().map_err(map_bridge_error)?;
    if account_devices_include_web_device(&devices, device_id) {
        return Ok(());
    }

    let devices = bridge.refresh_account_devices().map_err(map_bridge_error)?;
    if account_devices_include_web_device(&devices, device_id) {
        return Ok(());
    }

    Err(CommandError::policy_denied(
        "Remote command rejected because the web device is not linked or has been revoked.",
    ))
}

fn account_devices_include_web_device(devices: &[AccountDevice], device_id: &str) -> bool {
    devices
        .iter()
        .any(|device| device.kind == "web" && device.revoked_at.is_none() && device.id == device_id)
}

fn route_authorize_session_join<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?.to_string();
    let join_ref = required_payload_string(&command.payload, &["joinRef", "join_ref"])?;
    let auth_topic = required_payload_string(&command.payload, &["authTopic", "auth_topic"])?;
    let located = locate_remote_session(app, state, &session_id).ok();
    let authorized =
        ensure_known_web_device(bridge, &command.device_id).is_ok() && located.as_ref().is_some();
    let run_id = located
        .as_ref()
        .and_then(|located| located.session.last_run_id.as_deref());

    bridge
        .authorize_session_join(join_ref, auth_topic, &session_id, authorized, run_id)
        .map_err(map_bridge_error)
}

fn route_list_sessions<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
) -> CommandResult<()> {
    publish_remote_session_list(app, state, bridge)
}

fn publish_remote_session_list<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
) -> CommandResult<()> {
    let sessions = remote_session_summaries(app, state)?;
    bridge
        .forward_control_event(
            "__sessions__",
            json!({
                "schema": "xero.remote_sessions.v1",
                "sessions": sessions,
            }),
        )
        .map_err(map_bridge_error)?;
    Ok(())
}

fn route_list_projects<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
) -> CommandResult<()> {
    publish_remote_project_list(app, state, bridge)
}

fn publish_remote_project_list<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
) -> CommandResult<()> {
    let projects = remote_project_summaries(app, state)?;
    bridge
        .forward_control_event(
            "__projects__",
            json!({
                "schema": "xero.remote_projects.v1",
                "projects": projects,
            }),
        )
        .map_err(map_bridge_error)?;
    Ok(())
}

fn remote_project_summaries<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<Vec<JsonValue>> {
    remote_project_summaries_from_projects(visible_remote_project_summaries(app, state)?)
}

fn route_archive_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let project_id = required_payload_string(&command.payload, &["projectId", "project_id"])?;
    let session_id = required_payload_string(
        &command.payload,
        &[
            "agentSessionId",
            "agent_session_id",
            "sessionId",
            "session_id",
        ],
    )?;
    let repo_root = resolve_project_root(app, state, project_id)?;
    stop_idle_owned_runtime_run_before_archive(app, state, &repo_root, project_id, session_id)?;
    let session = project_store::archive_agent_session(&repo_root, project_id, session_id)?;
    emit_project_updated(
        app,
        &repo_root,
        project_id,
        ProjectUpdateReason::MetadataChanged,
    )?;

    publish_remote_session_list(app, state, bridge)?;
    let remote_session_id = remote_session_id_for(project_id, &session.agent_session_id);
    bridge
        .forward_session_removed(
            &remote_session_id,
            json!({
                "schema": "xero.remote_session_removed.v1",
                "projectId": project_id,
                "remoteSessionId": remote_session_id,
                "sessionId": &session.agent_session_id,
                "agentSessionId": &session.agent_session_id,
            }),
        )
        .map_err(map_bridge_error)?;
    Ok(())
}

fn publish_remote_session_snapshot(
    app: &AppHandle<impl Runtime>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    located: LocatedRemoteSession,
) -> CommandResult<()> {
    let session_id = located.remote_session_id.clone();
    let computer_id = bridge.computer_id().ok().flatten();
    let snapshot = remote_session_snapshot(app, state, &located, computer_id.as_deref())?;
    bridge
        .snapshot(&session_id, snapshot)
        .map_err(map_bridge_error)?;
    Ok(())
}

fn route_session_attached<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?;
    match session_id {
        "__sessions__" => return route_list_sessions(app, state, bridge),
        "__projects__" => return route_list_projects(app, state, bridge),
        "__new__" => return Ok(()),
        THEME_CONTROL_SESSION_ID => return publish_current_theme_to_cloud(app, state, bridge),
        _ => {}
    }

    let located = locate_remote_session(app, state, session_id)?;
    let last_seq = payload_u64(&command.payload, &["lastSeq", "last_seq"]).unwrap_or(0);
    if last_seq > 0
        && bridge
            .queue_replay_after(session_id, last_seq)
            .map_err(map_bridge_error)?
            > 0
    {
        return Ok(());
    }

    publish_remote_session_snapshot(app, state, bridge, located)
}

fn route_start_session<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let prompt = payload_string(&command.payload, &["prompt", "message"]).map(ToOwned::to_owned);
    let session_kind = remote_session_kind_from_payload(&command.payload)?;
    ensure_remote_payload_matches_session_kind(session_kind, &command.payload)?;
    if matches!(session_kind, project_store::AgentSessionKind::ComputerUse) {
        return route_start_computer_use_session(app, state, bridge, command, prompt.as_deref());
    }

    let located_project = locate_project_for_remote_start(app, state, &command.payload)?;
    let title = payload_string(&command.payload, &["title"])
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| match session_kind {
            project_store::AgentSessionKind::Standard => DEFAULT_AGENT_SESSION_TITLE.to_string(),
            project_store::AgentSessionKind::ComputerUse => {
                COMPUTER_USE_AGENT_SESSION_TITLE.to_string()
            }
        });
    let session = project_store::create_agent_session(
        &located_project.repo_root,
        &AgentSessionCreateRecord {
            project_id: located_project.project_id.clone(),
            title,
            summary: String::new(),
            selected: true,
            session_kind,
        },
    )?;
    emit_project_updated(
        app,
        &located_project.repo_root,
        &located_project.project_id,
        ProjectUpdateReason::MetadataChanged,
    )?;

    let run = match prompt.as_deref() {
        Some(prompt) => {
            let controls = remote_run_controls_from_payload(
                &command.payload,
                None,
                remote_default_agent_for_session_kind(session_kind),
            )?;
            ensure_remote_controls_match_session_kind(session_kind, controls.as_ref())?;
            if let Some(controls) = controls.as_ref() {
                persist_remote_composer_settings(
                    app,
                    state,
                    session_kind,
                    controls,
                    payload_string(&command.payload, &["providerId", "provider_id"]),
                )?;
            }
            let initial_attachments = remote_attachments_from_payload(&command.payload)?;
            Some(start_runtime_run_blocking(
                app.clone(),
                state.clone(),
                StartRuntimeRunRequestDto {
                    project_id: located_project.project_id.clone(),
                    agent_session_id: session.agent_session_id.clone(),
                    initial_controls: controls,
                    initial_prompt: Some(prompt.to_string()),
                    initial_attachments,
                },
            )?)
        }
        None => None,
    };

    let mut session_payload = remote_session_result_payload(
        &located_project.project_id,
        located_project.project_name.as_deref(),
        &session,
    );
    if let Some(payload) = session_payload.as_object_mut() {
        payload.insert("run".to_string(), json!(run));
    }
    bridge
        .forward_control_event(
            "__new__",
            json!({
                "schema": "xero.remote_session_started.v1",
                "result": session_payload,
            }),
        )
        .map_err(map_bridge_error)?;
    bridge
        .forward_control_event(
            "__sessions__",
            json!({
                "schema": "xero.remote_session_added.v1",
                "result": session_payload,
            }),
        )
        .map_err(map_bridge_error)?;
    Ok(())
}

fn route_start_computer_use_session<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
    prompt: Option<&str>,
) -> CommandResult<()> {
    let reset_existing = payload_bool(
        &command.payload,
        &[
            "resetExisting",
            "reset_existing",
            "clearExisting",
            "clear_existing",
            "clearChat",
            "clear_chat",
        ],
    )
    .unwrap_or(false);
    let global = if reset_existing {
        reset_global_computer_use_session_record(app, state)?
    } else {
        ensure_global_computer_use_session_record(app, state)?
    };
    let run = match prompt.map(str::trim).filter(|prompt| !prompt.is_empty()) {
        Some(prompt) => {
            let existing = load_persisted_runtime_run(
                &global.repo_root,
                &global.project_id,
                &global.session.agent_session_id,
            )?;
            let attachments = remote_attachments_from_payload(&command.payload)?;
            Some(match existing {
                Some(snapshot) => {
                    let selected_controls = selected_runtime_run_controls(&snapshot);
                    let controls = remote_run_controls_from_payload(
                        &command.payload,
                        Some(&selected_controls),
                        Some(RuntimeAgentIdDto::ComputerUse),
                    )?;
                    ensure_remote_controls_match_session_kind(
                        project_store::AgentSessionKind::ComputerUse,
                        controls.as_ref(),
                    )?;
                    if let Some(controls) = controls.as_ref() {
                        persist_remote_composer_settings(
                            app,
                            state,
                            project_store::AgentSessionKind::ComputerUse,
                            controls,
                            payload_string(&command.payload, &["providerId", "provider_id"]),
                        )?;
                    }
                    update_runtime_run_controls_blocking(
                        app.clone(),
                        state.clone(),
                        UpdateRuntimeRunControlsRequestDto {
                            project_id: global.project_id.clone(),
                            agent_session_id: global.session.agent_session_id.clone(),
                            run_id: snapshot.run.run_id,
                            controls,
                            prompt: Some(prompt.to_string()),
                            attachments,
                        },
                    )?
                }
                None => {
                    let controls = remote_run_controls_from_payload(
                        &command.payload,
                        None,
                        Some(RuntimeAgentIdDto::ComputerUse),
                    )?;
                    ensure_remote_controls_match_session_kind(
                        project_store::AgentSessionKind::ComputerUse,
                        controls.as_ref(),
                    )?;
                    if let Some(controls) = controls.as_ref() {
                        persist_remote_composer_settings(
                            app,
                            state,
                            project_store::AgentSessionKind::ComputerUse,
                            controls,
                            payload_string(&command.payload, &["providerId", "provider_id"]),
                        )?;
                    }
                    start_runtime_run_blocking(
                        app.clone(),
                        state.clone(),
                        StartRuntimeRunRequestDto {
                            project_id: global.project_id.clone(),
                            agent_session_id: global.session.agent_session_id.clone(),
                            initial_controls: controls,
                            initial_prompt: Some(prompt.to_string()),
                            initial_attachments: attachments,
                        },
                    )?
                }
            })
        }
        None => None,
    };

    let mut session_payload = remote_session_result_payload(
        &global.project_id,
        Some(GLOBAL_COMPUTER_USE_PROJECT_ID),
        &global.session,
    );
    if let Some(payload) = session_payload.as_object_mut() {
        payload.insert("run".to_string(), json!(run));
    }
    bridge
        .forward_control_event(
            REMOTE_COMPUTER_USE_SESSION_ID,
            json!({
                "schema": "xero.remote_session_started.v1",
                "result": session_payload,
            }),
        )
        .map_err(map_bridge_error)?;
    bridge
        .forward_control_event(
            "__new__",
            json!({
                "schema": "xero.remote_session_started.v1",
                "result": session_payload,
            }),
        )
        .map_err(map_bridge_error)?;
    if reset_existing {
        publish_remote_session_snapshot(
            app,
            state,
            bridge,
            LocatedRemoteSession {
                project_id: global.project_id,
                repo_root: global.repo_root,
                session: global.session,
                remote_session_id: REMOTE_COMPUTER_USE_SESSION_ID.into(),
            },
        )?;
    }
    Ok(())
}

fn route_send_message<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?;
    let message = required_payload_string(&command.payload, &["message", "prompt"])?;
    let located = locate_remote_session(app, state, session_id)?;
    let agent_session_id = local_agent_session_id(&located).to_string();
    let existing =
        load_persisted_runtime_run(&located.repo_root, &located.project_id, &agent_session_id)?;
    let attachments = remote_attachments_from_payload(&command.payload)?;
    ensure_remote_payload_matches_session_kind(located.session.session_kind, &command.payload)?;
    let run = match existing {
        Some(snapshot) => {
            let selected_controls = selected_runtime_run_controls(&snapshot);
            let controls = remote_run_controls_from_payload(
                &command.payload,
                Some(&selected_controls),
                remote_default_agent_for_session_kind(located.session.session_kind),
            )?;
            ensure_remote_controls_match_session_kind(
                located.session.session_kind,
                controls.as_ref(),
            )?;
            if let Some(controls) = controls.as_ref() {
                persist_remote_composer_settings(
                    app,
                    state,
                    located.session.session_kind,
                    controls,
                    payload_string(&command.payload, &["providerId", "provider_id"]),
                )?;
            }
            update_runtime_run_controls_blocking(
                app.clone(),
                state.clone(),
                UpdateRuntimeRunControlsRequestDto {
                    project_id: located.project_id.clone(),
                    agent_session_id: agent_session_id.clone(),
                    run_id: snapshot.run.run_id,
                    controls,
                    prompt: Some(message.to_string()),
                    attachments,
                },
            )?
        }
        None => {
            let controls = remote_run_controls_from_payload(
                &command.payload,
                None,
                remote_default_agent_for_session_kind(located.session.session_kind),
            )?;
            ensure_remote_controls_match_session_kind(
                located.session.session_kind,
                controls.as_ref(),
            )?;
            if let Some(controls) = controls.as_ref() {
                persist_remote_composer_settings(
                    app,
                    state,
                    located.session.session_kind,
                    controls,
                    payload_string(&command.payload, &["providerId", "provider_id"]),
                )?;
            }
            start_runtime_run_blocking(
                app.clone(),
                state.clone(),
                StartRuntimeRunRequestDto {
                    project_id: located.project_id.clone(),
                    agent_session_id: agent_session_id.clone(),
                    initial_controls: controls,
                    initial_prompt: Some(message.to_string()),
                    initial_attachments: attachments,
                },
            )?
        }
    };

    send_command_ok(
        bridge,
        session_id,
        "xero.remote_message_accepted.v1",
        json!({ "run": run }),
    )
}

fn route_update_session_controls<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?;
    let located = locate_remote_session(app, state, session_id)?;
    let agent_session_id = local_agent_session_id(&located).to_string();
    let existing =
        load_persisted_runtime_run(&located.repo_root, &located.project_id, &agent_session_id)?;
    ensure_remote_payload_matches_session_kind(located.session.session_kind, &command.payload)?;
    let selected_controls = existing.as_ref().map(selected_runtime_run_controls);
    let controls = remote_run_controls_from_payload(
        &command.payload,
        selected_controls.as_ref(),
        remote_default_agent_for_session_kind(located.session.session_kind),
    )?;
    ensure_remote_controls_match_session_kind(located.session.session_kind, controls.as_ref())?;
    let provider_id = payload_string(&command.payload, &["providerId", "provider_id"]);
    let control_payload = controls
        .as_ref()
        .map(|controls| {
            persist_remote_composer_settings(
                app,
                state,
                located.session.session_kind,
                controls,
                provider_id,
            )
        })
        .transpose()?;
    if let Some(existing) = existing {
        let run = update_runtime_run_controls_blocking(
            app.clone(),
            state.clone(),
            UpdateRuntimeRunControlsRequestDto {
                project_id: located.project_id.clone(),
                agent_session_id,
                run_id: existing.run.run_id,
                controls,
                prompt: None,
                attachments: Vec::new(),
            },
        )?;
        return send_command_ok(
            bridge,
            session_id,
            "xero.remote_session_controls_updated.v1",
            json!({ "run": run, "controls": control_payload }),
        );
    }

    send_command_ok(
        bridge,
        session_id,
        "xero.remote_session_controls_updated.v1",
        json!({ "controls": control_payload }),
    )
}

fn route_resolve_operator_action<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?;
    let located = locate_remote_session(app, state, session_id)?;
    let action_id = required_payload_string(&command.payload, &["actionId", "action_id"])?;
    let decision = required_payload_string(&command.payload, &["decision"])?;
    let response = resolve_operator_action_blocking(
        app.clone(),
        state.clone(),
        ResolveOperatorActionRequestDto {
            project_id: located.project_id,
            action_id: action_id.to_string(),
            decision: decision.to_string(),
            user_answer: payload_string(&command.payload, &["userAnswer", "user_answer"])
                .map(ToOwned::to_owned),
        },
    )?;

    send_command_ok(
        bridge,
        session_id,
        "xero.remote_operator_action_resolved.v1",
        json!({ "response": response }),
    )
}

fn route_cancel_run<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?;
    let located = locate_remote_session(app, state, session_id)?;
    let agent_session_id = local_agent_session_id(&located).to_string();
    let run_id = match payload_string(&command.payload, &["runId", "run_id"]) {
        Some(run_id) => run_id.to_string(),
        None => {
            let snapshot =
                load_persisted_runtime_run(&located.repo_root, &located.project_id, &agent_session_id)?
                    .ok_or_else(|| {
                        CommandError::user_fixable(
                            "runtime_run_missing",
                            format!(
                                "Xero cannot cancel session `{session_id}` because it has no durable runtime run."
                            ),
                        )
                    })?;
            snapshot.run.run_id
        }
    };
    let run = stop_runtime_run_blocking(
        app.clone(),
        state.clone(),
        StopRuntimeRunRequestDto {
            project_id: located.project_id,
            agent_session_id,
            run_id,
        },
    )?;

    send_command_ok(
        bridge,
        session_id,
        "xero.remote_run_cancelled.v1",
        json!({ "run": run }),
    )
}

fn route_context_snapshot<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?;
    let located = locate_remote_session(app, state, session_id)?;
    let agent_session_id = local_agent_session_id(&located).to_string();
    let request_id =
        payload_string(&command.payload, &["requestId", "request_id"]).map(ToOwned::to_owned);
    let context_result = build_session_context_snapshot(
        &located.repo_root,
        &located.project_id,
        &agent_session_id,
        payload_string(&command.payload, &["runId", "run_id"]),
        payload_string(&command.payload, &["providerId", "provider_id"]),
        payload_string(&command.payload, &["modelId", "model_id"]),
        payload_string(
            &command.payload,
            &["pendingPrompt", "pending_prompt", "prompt"],
        ),
    );

    let payload = match context_result {
        Ok(snapshot) => json!({
            "schema": "xero.remote_context_snapshot.v1",
            "ok": true,
            "requestId": request_id,
            "contextSnapshot": snapshot,
        }),
        Err(error) => json!({
            "schema": "xero.remote_context_snapshot.v1",
            "ok": false,
            "requestId": request_id,
            "error": error,
        }),
    };

    bridge
        .forward_control_event(session_id, payload)
        .map_err(map_bridge_error)?;
    Ok(())
}

fn send_command_ok(
    bridge: &AppRemoteBridge,
    session_id: &str,
    schema: &str,
    result: JsonValue,
) -> CommandResult<()> {
    bridge
        .forward_control_event(
            session_id,
            json!({
                "schema": schema,
                "ok": true,
                "result": result,
            }),
        )
        .map_err(map_bridge_error)?;
    Ok(())
}

fn route_stage_attachment<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?.to_string();
    let attachment_id = payload_string(&command.payload, &["attachmentId", "attachment_id"])
        .ok_or_else(|| CommandError::invalid_request("attachmentId"))?
        .to_string();
    let located = locate_remote_session(app, state, &session_id)?;
    let project_id = located.project_id;
    let original_name =
        required_payload_string(&command.payload, &["originalName", "original_name"])?.to_string();
    let media_type =
        required_payload_string(&command.payload, &["mediaType", "media_type"])?.to_string();
    let bytes_b64 = required_payload_string(&command.payload, &["bytesBase64", "bytes_base64"])?;
    let bytes = decode_attachment_bytes(bytes_b64)?;
    let run_id = payload_string(&command.payload, &["runId", "run_id"])
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "pending".to_string());

    let result = stage_agent_attachment_blocking(
        app,
        state,
        StageAgentAttachmentRequestDto {
            project_id: project_id.clone(),
            run_id,
            original_name,
            media_type,
            bytes,
        },
    );
    let payload = match result {
        Ok(staged) => attachment_staged_payload(&attachment_id, &staged),
        Err(error) => attachment_error_payload(&attachment_id, &error),
    };

    bridge
        .forward_control_event(&session_id, payload)
        .map_err(map_bridge_error)?;
    Ok(())
}

fn route_discard_attachment<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?.to_string();
    let attachment_id = payload_string(&command.payload, &["attachmentId", "attachment_id"])
        .ok_or_else(|| CommandError::invalid_request("attachmentId"))?
        .to_string();
    let located = locate_remote_session(app, state, &session_id)?;
    let absolute_path =
        required_payload_string(&command.payload, &["absolutePath", "absolute_path"])?.to_string();

    let result = discard_agent_attachment_blocking(
        app,
        state,
        DiscardAgentAttachmentRequestDto {
            project_id: located.project_id,
            absolute_path,
        },
    );
    let payload = match result {
        Ok(()) => json!({
            "schema": "xero.remote_attachment_discarded.v1",
            "ok": true,
            "attachmentId": attachment_id,
        }),
        Err(error) => json!({
            "schema": "xero.remote_attachment_discarded.v1",
            "ok": false,
            "attachmentId": attachment_id,
            "error": error,
        }),
    };

    bridge
        .forward_control_event(&session_id, payload)
        .map_err(map_bridge_error)?;
    Ok(())
}

fn route_fetch_runtime_media_artifact<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?.to_string();
    let Some(artifact_id) = payload_string(&command.payload, &["artifactId", "artifact_id"]) else {
        bridge
            .forward_control_event(
                &session_id,
                json!({
                    "schema": "xero.remote_runtime_media_artifact.v1",
                    "ok": false,
                    "artifactId": "",
                    "mediaType": "application/octet-stream",
                    "error": CommandError::user_fixable(
                        "remote_command_payload_invalid",
                        "Remote runtime media request is missing an artifact id.",
                    ),
                }),
            )
            .map_err(map_bridge_error)?;
        return Ok(());
    };
    let artifact_id = artifact_id.to_string();
    let located = locate_remote_session(app, state, &session_id)?;
    let result = read_runtime_media_artifact(&located.repo_root, &artifact_id);
    let payload = match result {
        Ok(artifact) => {
            use base64::Engine as _;
            let bytes_base64 =
                base64::engine::general_purpose::STANDARD.encode(artifact.bytes.as_slice());
            json!({
                "schema": "xero.remote_runtime_media_artifact.v1",
                "ok": true,
                "artifactId": artifact.artifact_id,
                "mediaType": artifact.media_type,
                "bytesBase64": bytes_base64,
                "sizeBytes": artifact.bytes.len(),
            })
        }
        Err(error) => json!({
            "schema": "xero.remote_runtime_media_artifact.v1",
            "ok": false,
            "artifactId": artifact_id,
            "error": error,
        }),
    };

    bridge
        .forward_control_event(&session_id, payload)
        .map_err(map_bridge_error)?;
    Ok(())
}

fn route_computer_use_stream_command<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?.to_string();
    let located = locate_remote_session(app, state, &session_id)?;
    ensure_computer_use_remote_session(&located)?;
    let schema = computer_use_stream_schema(&command.kind);
    let stream_id =
        payload_string(&command.payload, &["streamId", "stream_id"]).map(ToOwned::to_owned);
    let settings = load_desktop_control_settings(app, state)?;
    let desktop_output = match command.kind {
        InboundCommandKind::ComputerUseStreamRequest => {
            if !settings.cloud_streaming_enabled {
                return forward_computer_use_desktop_rejection(
                    bridge,
                    &session_id,
                    schema,
                    &command,
                    stream_id.as_deref(),
                    None,
                    "cloud_streaming_disabled",
                    "Cloud desktop viewing is disabled in the local desktop app.",
                );
            }
            Some(run_desktop_stream_command(
                &located,
                &command,
                AutonomousDesktopStreamAction::StreamStart,
            )?)
        }
        InboundCommandKind::ComputerUseStreamStop => Some(run_desktop_stream_command(
            &located,
            &command,
            AutonomousDesktopStreamAction::StreamStop,
        )?),
        InboundCommandKind::ComputerUseStreamStatus => Some(run_desktop_stream_command(
            &located,
            &command,
            AutonomousDesktopStreamAction::StreamStatus,
        )?),
        InboundCommandKind::ComputerUseStreamSetQuality => Some(run_desktop_stream_command(
            &located,
            &command,
            AutonomousDesktopStreamAction::StreamSetQuality,
        )?),
        InboundCommandKind::ComputerUseStreamRequestKeyframe => Some(run_desktop_stream_command(
            &located,
            &command,
            AutonomousDesktopStreamAction::StreamRequestKeyframe,
        )?),
        InboundCommandKind::ComputerUseStreamOffer => Some(run_desktop_stream_command(
            &located,
            &command,
            AutonomousDesktopStreamAction::StreamOffer,
        )?),
        InboundCommandKind::ComputerUseStreamAnswer => Some(run_desktop_stream_command(
            &located,
            &command,
            AutonomousDesktopStreamAction::StreamAnswer,
        )?),
        InboundCommandKind::ComputerUseStreamIceCandidate => Some(run_desktop_stream_command(
            &located,
            &command,
            AutonomousDesktopStreamAction::StreamIceCandidate,
        )?),
        _ => None,
    };
    let desktop_frame = if let Some(output) = desktop_output.as_ref() {
        fallback_frame_for_stream_output(&located, &command, output)?
    } else {
        None
    };
    let stream_id = desktop_output
        .as_ref()
        .and_then(|output| output.stream.as_ref())
        .and_then(|stream| stream.stream_id.clone())
        .or(stream_id);
    let stream_signal_payload = desktop_output
        .as_ref()
        .and_then(stream_signal_payload_for_output);
    let forwarded_schema = desktop_output
        .as_ref()
        .and_then(stream_signal_schema_for_output)
        .unwrap_or(schema);
    let forwarded_payload = stream_signal_payload.unwrap_or_else(|| {
        remote_stream_payload_for_forward(&command.kind, command.payload.clone())
    });
    let response_outcome = command_outcome_for_desktop_output(desktop_output.as_ref());
    let mut response_payload = json!({
        "schema": forwarded_schema,
        "ok": response_outcome != "rejected",
        "commandSeq": command.seq,
        "deviceId": command.device_id.as_str(),
        "sessionId": session_id.as_str(),
        "streamId": stream_id,
        "receivedAt": crate::auth::now_timestamp(),
        "payload": forwarded_payload,
        "desktop": desktop_output,
        "desktopFrame": desktop_frame,
    });
    attach_command_context(&mut response_payload, &command, Some(response_outcome));
    bridge
        .forward_control_event(&session_id, response_payload)
        .map_err(map_bridge_error)?;
    Ok(())
}

fn route_computer_use_manual_control_command<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?.to_string();
    let located = locate_remote_session(app, state, &session_id)?;
    ensure_computer_use_remote_session(&located)?;
    let schema = computer_use_manual_control_schema(&command.kind);
    let settings = load_desktop_control_settings(app, state)?;
    let manual_control_id =
        payload_string(&command.payload, &["manualControlId", "manual_control_id"])
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("manual_{}_{}", command.seq, crate::auth::now_timestamp()));
    let desktop_output = match command.kind {
        InboundCommandKind::ComputerUseManualControlRequest
        | InboundCommandKind::ComputerUseManualControlGrant => {
            if !settings.manual_cloud_control_enabled {
                return forward_computer_use_desktop_rejection(
                    bridge,
                    &session_id,
                    schema,
                    &command,
                    None,
                    Some(manual_control_id.as_str()),
                    "manual_cloud_control_disabled",
                    "Cloud manual control is disabled in the local desktop app.",
                );
            }
            let runtime = desktop_runtime_for_located(&located, &command)?;
            Some(runtime.desktop_acquire_manual_control(
                &manual_control_id,
                payload_string(&command.payload, &["reason"]),
            )?)
        }
        InboundCommandKind::ComputerUseManualControlHeartbeat => {
            if !settings.manual_cloud_control_enabled {
                return forward_computer_use_desktop_rejection(
                    bridge,
                    &session_id,
                    schema,
                    &command,
                    None,
                    Some(manual_control_id.as_str()),
                    "manual_cloud_control_disabled",
                    "Cloud manual control is disabled in the local desktop app.",
                );
            }
            let runtime = desktop_runtime_for_located(&located, &command)?;
            Some(
                runtime.desktop_refresh_manual_control(
                    &manual_control_id,
                    payload_string(&command.payload, &["reason"])
                        .unwrap_or("cloud_manual_control_heartbeat"),
                )?,
            )
        }
        InboundCommandKind::ComputerUseManualControlInput => {
            if !settings.manual_cloud_control_enabled {
                return forward_computer_use_desktop_rejection(
                    bridge,
                    &session_id,
                    schema,
                    &command,
                    None,
                    Some(manual_control_id.as_str()),
                    "manual_cloud_control_disabled",
                    "Cloud manual control is disabled in the local desktop app.",
                );
            }
            let request = manual_control_input_request(&command.payload)?;
            let runtime = desktop_runtime_for_located(&located, &command)?;
            let result = runtime.desktop_control_as_manual_control_with_operator_approval(
                request,
                &manual_control_id,
            )?;
            Some(desktop_control_output_from_result(result.output)?)
        }
        InboundCommandKind::ComputerUseManualControlRelease => {
            let runtime = desktop_runtime_for_located(&located, &command)?;
            Some(runtime.desktop_release_manual_control(
                Some(&manual_control_id),
                "cloud_manual_control_release",
            )?)
        }
        _ => None,
    };
    let response_outcome = command_outcome_for_desktop_output(desktop_output.as_ref());
    let mut response_payload = json!({
        "schema": schema,
        "ok": response_outcome != "rejected",
        "commandSeq": command.seq,
        "deviceId": command.device_id.as_str(),
        "sessionId": session_id.as_str(),
        "manualControlId": manual_control_id,
        "receivedAt": crate::auth::now_timestamp(),
        "payload": remote_desktop_payload_for_forward(command.payload.clone()),
        "desktop": desktop_output,
        "brokered": true,
    });
    attach_command_context(&mut response_payload, &command, Some(response_outcome));
    bridge
        .forward_control_event(&session_id, response_payload)
        .map_err(map_bridge_error)?;
    Ok(())
}

fn run_desktop_stream_command(
    located: &LocatedRemoteSession,
    command: &InboundCommand,
    action: AutonomousDesktopStreamAction,
) -> CommandResult<AutonomousDesktopToolOutput> {
    let request = AutonomousDesktopStreamRequest {
        action,
        session_id: Some(located.remote_session_id.clone()),
        run_id: payload_string(&command.payload, &["runId", "run_id"]).map(ToOwned::to_owned),
        display_id: payload_string(&command.payload, &["displayId", "display_id"])
            .map(ToOwned::to_owned),
        stream_id: payload_string(&command.payload, &["streamId", "stream_id"])
            .map(ToOwned::to_owned),
        max_width: payload_u64(&command.payload, &["maxWidth", "max_width"])
            .and_then(|value| u32::try_from(value).ok()),
        max_frame_rate: payload_u64(&command.payload, &["maxFrameRate", "max_frame_rate"])
            .and_then(|value| u32::try_from(value).ok()),
        include_cursor: payload_bool(&command.payload, &["includeCursor", "include_cursor"]),
        quality: payload_string(&command.payload, &["quality"]).and_then(stream_quality_from_str),
        ice_servers: desktop_stream_ice_servers_from_payload(&command.payload)?,
        session_description: desktop_stream_session_description_from_payload(&command.payload)?,
        ice_candidate: desktop_stream_ice_candidate_from_payload(&command.payload)?,
    };
    let runtime = desktop_runtime_for_located(located, command)?;
    let result = runtime.desktop_stream_with_operator_approval(request)?;
    desktop_control_output_from_result(result.output)
}

fn remote_stream_payload_for_forward(kind: &InboundCommandKind, payload: JsonValue) -> JsonValue {
    let mut payload = remote_desktop_payload_for_forward(payload);
    if matches!(
        kind,
        InboundCommandKind::ComputerUseStreamOffer
            | InboundCommandKind::ComputerUseStreamAnswer
            | InboundCommandKind::ComputerUseStreamIceCandidate
    ) {
        strip_sensitive_keys(
            &mut payload,
            &[
                "type",
                "sdp",
                "candidate",
                "sessionDescription",
                "session_description",
                "iceCandidate",
                "ice_candidate",
            ],
        );
    }
    payload
}

fn stream_signal_schema_for_output(output: &AutonomousDesktopToolOutput) -> Option<&'static str> {
    let signal = output.stream_signal.as_ref()?;
    if let Some(description) = signal.session_description.as_ref() {
        return match description.sdp_type.as_str() {
            "offer" => Some("xero.computer_use_stream_offer.v1"),
            "answer" | "pranswer" => Some("xero.computer_use_stream_answer.v1"),
            _ => None,
        };
    }
    signal
        .ice_candidate
        .as_ref()
        .map(|_| "xero.computer_use_stream_ice_candidate.v1")
}

fn stream_signal_payload_for_output(output: &AutonomousDesktopToolOutput) -> Option<JsonValue> {
    let signal = output.stream_signal.as_ref()?;
    if let Some(description) = signal.session_description.as_ref() {
        return Some(json!({
            "type": description.sdp_type.as_str(),
            "sdp": description.sdp.as_str(),
        }));
    }
    let candidate = signal.ice_candidate.as_ref()?;
    Some(json!({
        "candidate": {
            "candidate": candidate.candidate,
            "sdpMid": candidate.sdp_mid.as_deref(),
            "sdpMLineIndex": candidate.sdp_m_line_index,
            "usernameFragment": candidate.username_fragment.as_deref(),
        }
    }))
}

fn desktop_stream_ice_servers_from_payload(
    payload: &JsonValue,
) -> CommandResult<Vec<AutonomousDesktopIceServer>> {
    let Some(value) = payload_value(payload, &["iceServers", "ice_servers"]) else {
        return Ok(Vec::new());
    };
    let value = normalize_webrtc_field_aliases(value.clone());
    serde_json::from_value::<Vec<AutonomousDesktopIceServer>>(value).map_err(|error| {
        CommandError::user_fixable(
            "invalid_request",
            format!("Field `iceServers` is invalid: {error}"),
        )
    })
}

fn desktop_stream_session_description_from_payload(
    payload: &JsonValue,
) -> CommandResult<Option<AutonomousDesktopSessionDescription>> {
    if let Some(value) = payload_value(payload, &["sessionDescription", "session_description"]) {
        let value = normalize_webrtc_field_aliases(value.clone());
        return serde_json::from_value::<AutonomousDesktopSessionDescription>(value)
            .map(Some)
            .map_err(|error| {
                CommandError::user_fixable(
                    "invalid_request",
                    format!("Field `sessionDescription` is invalid: {error}"),
                )
            });
    }
    let Some(sdp) = payload_string(payload, &["sdp"]) else {
        return Ok(None);
    };
    Ok(Some(AutonomousDesktopSessionDescription {
        sdp_type: payload_string(payload, &["type"])
            .unwrap_or("answer")
            .to_string(),
        sdp: sdp.to_string(),
    }))
}

fn desktop_stream_ice_candidate_from_payload(
    payload: &JsonValue,
) -> CommandResult<Option<AutonomousDesktopIceCandidate>> {
    let Some(value) = payload_value(payload, &["iceCandidate", "ice_candidate", "candidate"])
    else {
        return Ok(None);
    };
    if let Some(candidate) = value.as_str() {
        return Ok(Some(AutonomousDesktopIceCandidate {
            candidate: candidate.to_string(),
            sdp_mid: None,
            sdp_m_line_index: None,
            username_fragment: None,
        }));
    }
    let value = normalize_webrtc_field_aliases(value.clone());
    serde_json::from_value::<AutonomousDesktopIceCandidate>(value)
        .map(Some)
        .map_err(|error| {
            CommandError::user_fixable(
                "invalid_request",
                format!("Field `iceCandidate` is invalid: {error}"),
            )
        })
}

fn normalize_webrtc_field_aliases(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(values) => JsonValue::Array(
            values
                .into_iter()
                .map(normalize_webrtc_field_aliases)
                .collect(),
        ),
        JsonValue::Object(mut object) => {
            rename_json_field(&mut object, "credential_type", "credentialType");
            rename_json_field(&mut object, "sdp_mid", "sdpMid");
            rename_json_field(&mut object, "sdp_m_line_index", "sdpMLineIndex");
            rename_json_field(&mut object, "username_fragment", "usernameFragment");
            JsonValue::Object(object)
        }
        other => other,
    }
}

fn rename_json_field(object: &mut JsonMap<String, JsonValue>, from: &str, to: &str) {
    if object.contains_key(to) {
        return;
    }
    if let Some(value) = object.remove(from) {
        object.insert(to.to_string(), value);
    }
}

fn fallback_frame_for_stream_output(
    located: &LocatedRemoteSession,
    command: &InboundCommand,
    output: &AutonomousDesktopToolOutput,
) -> CommandResult<Option<JsonValue>> {
    let Some(stream) = output.stream.as_ref() else {
        return Ok(None);
    };
    if stream.transport != AutonomousDesktopStreamTransport::ScreenshotFallback {
        return Ok(None);
    }
    if !matches!(
        command.kind,
        InboundCommandKind::ComputerUseStreamRequest
            | InboundCommandKind::ComputerUseStreamStatus
            | InboundCommandKind::ComputerUseStreamSetQuality
            | InboundCommandKind::ComputerUseStreamRequestKeyframe
    ) {
        return Ok(None);
    }
    capture_desktop_stream_fallback_frame(located, command, stream.max_width)
}

fn capture_desktop_stream_fallback_frame(
    located: &LocatedRemoteSession,
    command: &InboundCommand,
    max_width: u32,
) -> CommandResult<Option<JsonValue>> {
    let request = AutonomousDesktopObserveRequest {
        action: AutonomousDesktopObserveAction::Screenshot,
        display_id: payload_string(&command.payload, &["displayId", "display_id"])
            .map(ToOwned::to_owned),
        window_id: None,
        region: None,
        x: None,
        y: None,
        include_data: None,
        max_bytes: None,
    };
    let runtime = desktop_runtime_for_located(located, command)?;
    let result = runtime.desktop_observe_with_operator_approval(request)?;
    let output = desktop_control_output_from_result(result.output)?;
    let Some(screenshot) = output.screenshot else {
        return Ok(None);
    };
    let path = Path::new(&screenshot.path);
    let bytes = fs::read(path).map_err(|error| {
        CommandError::system_fault(
            "stream_fallback_frame_read_failed",
            format!("Xero could not read the desktop fallback frame: {error}"),
        )
    })?;
    let _ = fs::remove_file(path);
    let encoded = encode_stream_fallback_frame(&bytes, &screenshot, max_width)?;
    if encoded.bytes.len() > STREAM_FALLBACK_FRAME_MAX_BYTES {
        return Ok(Some(json!({
            "schema": "xero.computer_use_stream_frame.v1",
            "ok": false,
            "transport": "screenshot_fallback",
            "error": {
                "code": "stream_fallback_frame_too_large",
                "message": "The desktop fallback frame exceeded the relay size budget."
            },
            "mediaType": encoded.media_type,
            "sizeBytes": encoded.bytes.len(),
            "maxSizeBytes": STREAM_FALLBACK_FRAME_MAX_BYTES,
            "width": encoded.width,
            "height": encoded.height,
            "capturedAt": screenshot.captured_at,
        })));
    }
    let size_bytes = encoded.bytes.len();
    let bytes_base64 = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(encoded.bytes)
    };
    Ok(Some(json!({
        "schema": "xero.computer_use_stream_frame.v1",
        "ok": true,
        "transport": "screenshot_fallback",
        "mediaType": encoded.media_type,
        "bytesBase64": bytes_base64,
        "sizeBytes": size_bytes,
        "width": encoded.width,
        "height": encoded.height,
        "scaleFactor": encoded.scale_factor,
        "capturedAt": screenshot.captured_at,
    })))
}

#[derive(Debug, Clone, PartialEq)]
struct EncodedStreamFallbackFrame {
    bytes: Vec<u8>,
    media_type: &'static str,
    width: u32,
    height: u32,
    scale_factor: f32,
}

fn encode_stream_fallback_frame(
    source_bytes: &[u8],
    screenshot: &AutonomousDesktopScreenshot,
    max_width: u32,
) -> CommandResult<EncodedStreamFallbackFrame> {
    let image = image::load_from_memory(source_bytes).map_err(|error| {
        CommandError::system_fault(
            "stream_fallback_frame_decode_failed",
            format!("Xero could not decode the desktop fallback frame: {error}"),
        )
    })?;
    let source_width = image.width();
    let source_height = image.height();
    if source_width == 0 || source_height == 0 {
        return Err(CommandError::system_fault(
            "stream_fallback_frame_empty",
            "Xero captured an empty desktop fallback frame.",
        ));
    }

    let (target_width, target_height) =
        stream_fallback_dimensions(source_width, source_height, max_width);
    let frame = if target_width == source_width && target_height == source_height {
        image
    } else {
        image.resize_exact(
            target_width,
            target_height,
            image::imageops::FilterType::Triangle,
        )
    };
    let rgb = frame.to_rgb8();
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, STREAM_FALLBACK_JPEG_QUALITY)
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ColorType::Rgb8.into(),
        )
        .map_err(|error| {
            CommandError::system_fault(
                "stream_fallback_frame_encode_failed",
                format!("Xero could not encode the desktop fallback frame: {error}"),
            )
        })?;

    let scale_ratio = rgb.width() as f32 / source_width as f32;
    Ok(EncodedStreamFallbackFrame {
        bytes,
        media_type: "image/jpeg",
        width: rgb.width(),
        height: rgb.height(),
        scale_factor: screenshot.scale_factor * scale_ratio,
    })
}

fn stream_fallback_dimensions(source_width: u32, source_height: u32, max_width: u32) -> (u32, u32) {
    let target_width = source_width.min(max_width.max(1));
    if target_width == source_width {
        return (source_width, source_height);
    }

    let target_height = (u64::from(source_height) * u64::from(target_width))
        .div_ceil(u64::from(source_width))
        .clamp(1, u64::from(u32::MAX)) as u32;
    (target_width, target_height)
}

fn desktop_runtime_for_located(
    located: &LocatedRemoteSession,
    command: &InboundCommand,
) -> CommandResult<AutonomousToolRuntime> {
    let run_id = payload_string(&command.payload, &["runId", "run_id"])
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("remote-desktop-{}", command.seq));
    AutonomousToolRuntime::new(&located.repo_root).map(|runtime| {
        runtime.with_agent_run_context(
            located.project_id.clone(),
            local_agent_session_id(located).to_owned(),
            run_id,
        )
    })
}

fn desktop_control_output_from_result(
    output: AutonomousToolOutput,
) -> CommandResult<AutonomousDesktopToolOutput> {
    match output {
        AutonomousToolOutput::DesktopControl(output)
        | AutonomousToolOutput::DesktopStream(output) => Ok(output),
        AutonomousToolOutput::DesktopObserve(output) => Ok(output),
        _ => Err(CommandError::system_fault(
            "desktop_control_output_mismatch",
            "Xero could not decode the desktop broker output.",
        )),
    }
}

fn manual_control_input_request(
    payload: &JsonValue,
) -> CommandResult<AutonomousDesktopControlRequest> {
    let action_name = required_payload_string(payload, &["action"])?;
    let action = serde_json::from_value::<AutonomousDesktopControlAction>(json!(action_name))
        .map_err(|_| {
            CommandError::user_fixable(
                "remote_manual_control_action_invalid",
                format!("Remote manual-control action `{action_name}` is not supported."),
            )
        })?;
    if !remote_manual_control_action_allowed(&action) {
        return Err(CommandError::user_fixable(
            "remote_manual_control_action_denied",
            format!(
                "Remote manual-control action `{action_name}` is outside the manual input allowlist."
            ),
        ));
    }
    Ok(AutonomousDesktopControlRequest {
        action,
        display_id: payload_string(payload, &["displayId", "display_id"]).map(ToOwned::to_owned),
        window_id: payload_string(payload, &["windowId", "window_id"]).map(ToOwned::to_owned),
        app_name: payload_string(payload, &["appName", "app_name"]).map(ToOwned::to_owned),
        bundle_id: payload_string(payload, &["bundleId", "bundle_id"]).map(ToOwned::to_owned),
        element_id: payload_string(payload, &["elementId", "element_id"]).map(ToOwned::to_owned),
        x: payload_i32(payload, &["x"]),
        y: payload_i32(payload, &["y"]),
        source_width: payload_u64(payload, &["sourceWidth", "source_width"])
            .and_then(|value| u32::try_from(value).ok()),
        source_height: payload_u64(payload, &["sourceHeight", "source_height"])
            .and_then(|value| u32::try_from(value).ok()),
        to_x: payload_i32(payload, &["toX", "to_x"]),
        to_y: payload_i32(payload, &["toY", "to_y"]),
        delta_x: payload_i32(payload, &["deltaX", "delta_x"]),
        delta_y: payload_i32(payload, &["deltaY", "delta_y"]),
        width: payload_u64(payload, &["width"]).and_then(|value| u32::try_from(value).ok()),
        height: payload_u64(payload, &["height"]).and_then(|value| u32::try_from(value).ok()),
        include_data: payload_bool(payload, &["includeData", "include_data"]),
        max_bytes: payload_u64(payload, &["maxBytes", "max_bytes"])
            .and_then(|value| usize::try_from(value).ok()),
        media_type: payload_string(payload, &["mediaType", "media_type"]).map(ToOwned::to_owned),
        image_data_base64: payload_string(payload, &["imageDataBase64", "image_data_base64"])
            .map(ToOwned::to_owned),
        file_paths: payload_string_array(payload, &["filePaths", "file_paths"]),
        button: payload_string(payload, &["button"]).and_then(mouse_button_from_str),
        clicks: payload_u64(payload, &["clicks"]).and_then(|value| u8::try_from(value).ok()),
        key: payload_string(payload, &["key"]).map(ToOwned::to_owned),
        keys: payload_string_array(payload, &["keys"]),
        text: payload_string(payload, &["text"]).map(ToOwned::to_owned),
        html: payload_string(payload, &["html"]).map(ToOwned::to_owned),
        rtf: payload_string(payload, &["rtf"]).map(ToOwned::to_owned),
        alt_text: payload_string(payload, &["altText", "alt_text"]).map(ToOwned::to_owned),
        target_label: payload_string(payload, &["targetLabel", "target_label"])
            .map(ToOwned::to_owned),
        selection_start: payload_u64(payload, &["selectionStart", "selection_start"])
            .and_then(|value| usize::try_from(value).ok()),
        selection_end: payload_u64(payload, &["selectionEnd", "selection_end"])
            .and_then(|value| usize::try_from(value).ok()),
        value: payload_string(payload, &["value"]).map(ToOwned::to_owned),
        menu_path: payload_string_array(payload, &["menuPath", "menu_path"]),
        reason: payload_string(payload, &["reason"])
            .map(ToOwned::to_owned)
            .or_else(|| Some("cloud_manual_control_input".into())),
        sensitivity: None,
    })
}

fn remote_manual_control_action_allowed(action: &AutonomousDesktopControlAction) -> bool {
    matches!(
        action,
        AutonomousDesktopControlAction::MouseDown
            | AutonomousDesktopControlAction::MouseMove
            | AutonomousDesktopControlAction::MouseClick
            | AutonomousDesktopControlAction::MouseDoubleClick
            | AutonomousDesktopControlAction::MouseRightClick
            | AutonomousDesktopControlAction::MouseDrag
            | AutonomousDesktopControlAction::MouseDragMove
            | AutonomousDesktopControlAction::MouseUp
            | AutonomousDesktopControlAction::Scroll
            | AutonomousDesktopControlAction::KeyPress
            | AutonomousDesktopControlAction::Hotkey
            | AutonomousDesktopControlAction::TypeText
            | AutonomousDesktopControlAction::PasteText
            | AutonomousDesktopControlAction::ClipboardWriteText
            | AutonomousDesktopControlAction::CancelCurrentAction
    )
}

fn stream_quality_from_str(value: &str) -> Option<AutonomousDesktopStreamQuality> {
    match value {
        "low" => Some(AutonomousDesktopStreamQuality::Low),
        "balanced" => Some(AutonomousDesktopStreamQuality::Balanced),
        "high" => Some(AutonomousDesktopStreamQuality::High),
        _ => None,
    }
}

fn mouse_button_from_str(value: &str) -> Option<AutonomousDesktopMouseButton> {
    match value {
        "left" => Some(AutonomousDesktopMouseButton::Left),
        "right" => Some(AutonomousDesktopMouseButton::Right),
        "middle" => Some(AutonomousDesktopMouseButton::Middle),
        _ => None,
    }
}

fn remote_desktop_payload_for_forward(payload: JsonValue) -> JsonValue {
    let JsonValue::Object(mut object) = payload else {
        return payload;
    };
    object.remove("streamToken");
    object.remove("stream_token");
    JsonValue::Object(object)
}

fn strip_sensitive_keys(payload: &mut JsonValue, keys: &[&str]) {
    let JsonValue::Object(object) = payload else {
        return;
    };
    for key in keys {
        object.remove(*key);
    }
}

#[allow(clippy::too_many_arguments)]
fn forward_computer_use_desktop_rejection(
    bridge: &AppRemoteBridge,
    session_id: &str,
    schema: &str,
    command: &InboundCommand,
    stream_id: Option<&str>,
    manual_control_id: Option<&str>,
    code: &str,
    message: &str,
) -> CommandResult<()> {
    let mut payload = json!({
        "schema": schema,
        "ok": false,
        "commandSeq": command.seq,
        "deviceId": command.device_id.as_str(),
        "sessionId": session_id,
        "streamId": stream_id,
        "manualControlId": manual_control_id,
        "receivedAt": crate::auth::now_timestamp(),
        "payload": remote_desktop_payload_for_forward(command.payload.clone()),
        "error": {
            "code": code,
            "message": message,
        },
    });
    attach_command_context(&mut payload, command, Some("rejected"));
    bridge
        .forward_control_event(session_id, payload)
        .map(|_| ())
        .map_err(map_bridge_error)
}

fn ensure_computer_use_remote_session(located: &LocatedRemoteSession) -> CommandResult<()> {
    if matches!(
        located.session.session_kind,
        project_store::AgentSessionKind::ComputerUse
    ) {
        return Ok(());
    }
    Err(CommandError::policy_denied(
        "Remote desktop stream and manual control commands require a Computer Use session.",
    ))
}

fn computer_use_stream_schema(kind: &InboundCommandKind) -> &'static str {
    match kind {
        InboundCommandKind::ComputerUseStreamRequest => "xero.computer_use_stream_request.v1",
        InboundCommandKind::ComputerUseStreamOffer => "xero.computer_use_stream_offer.v1",
        InboundCommandKind::ComputerUseStreamAnswer => "xero.computer_use_stream_answer.v1",
        InboundCommandKind::ComputerUseStreamIceCandidate => {
            "xero.computer_use_stream_ice_candidate.v1"
        }
        InboundCommandKind::ComputerUseStreamStop => "xero.computer_use_stream_stop.v1",
        InboundCommandKind::ComputerUseStreamStatus => "xero.computer_use_stream_status.v1",
        InboundCommandKind::ComputerUseStreamSetQuality => {
            "xero.computer_use_stream_set_quality.v1"
        }
        InboundCommandKind::ComputerUseStreamRequestKeyframe => {
            "xero.computer_use_stream_request_keyframe.v1"
        }
        _ => "xero.computer_use_stream_unknown.v1",
    }
}

fn computer_use_manual_control_schema(kind: &InboundCommandKind) -> &'static str {
    match kind {
        InboundCommandKind::ComputerUseManualControlRequest => {
            "xero.computer_use_manual_control_request.v1"
        }
        InboundCommandKind::ComputerUseManualControlGrant => {
            "xero.computer_use_manual_control_grant.v1"
        }
        InboundCommandKind::ComputerUseManualControlHeartbeat => {
            "xero.computer_use_manual_control_heartbeat.v1"
        }
        InboundCommandKind::ComputerUseManualControlInput => {
            "xero.computer_use_manual_control_input.v1"
        }
        InboundCommandKind::ComputerUseManualControlRelease => {
            "xero.computer_use_manual_control_release.v1"
        }
        _ => "xero.computer_use_manual_control_unknown.v1",
    }
}

fn attachment_staged_payload(attachment_id: &str, staged: &StagedAgentAttachmentDto) -> JsonValue {
    json!({
        "schema": "xero.remote_attachment_staged.v1",
        "ok": true,
        "attachmentId": attachment_id,
        "attachment": staged,
    })
}

fn attachment_error_payload(attachment_id: &str, error: &CommandError) -> JsonValue {
    json!({
        "schema": "xero.remote_attachment_staged.v1",
        "ok": false,
        "attachmentId": attachment_id,
        "error": error,
    })
}

fn decode_attachment_bytes(value: &str) -> CommandResult<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(value.as_bytes())
        .map_err(|error| {
            CommandError::user_fixable(
                "remote_attachment_invalid_bytes",
                format!("Remote attachment bytes were not valid base64: {error}"),
            )
        })
}

#[derive(Debug, Clone)]
struct LocatedRemoteProject {
    project_id: String,
    project_name: Option<String>,
    repo_root: std::path::PathBuf,
}

#[derive(Debug, Clone)]
struct LocatedRemoteSession {
    project_id: String,
    repo_root: std::path::PathBuf,
    session: AgentSessionRecord,
    remote_session_id: String,
}

fn remote_session_id_for(project_id: &str, agent_session_id: &str) -> String {
    if project_id == GLOBAL_COMPUTER_USE_PROJECT_ID
        && agent_session_id == GLOBAL_COMPUTER_USE_AGENT_SESSION_ID
    {
        return REMOTE_COMPUTER_USE_SESSION_ID.into();
    }

    project_scoped_remote_session_id(project_id, agent_session_id)
}

fn project_scoped_remote_session_id(project_id: &str, agent_session_id: &str) -> String {
    format!(
        "{PROJECT_REMOTE_SESSION_ID_PREFIX}{}:{}{}",
        project_id.len(),
        project_id,
        agent_session_id
    )
}

fn parse_project_scoped_remote_session_id(remote_session_id: &str) -> Option<(&str, &str)> {
    let rest = remote_session_id.strip_prefix(PROJECT_REMOTE_SESSION_ID_PREFIX)?;
    let (project_len, scoped_id) = rest.split_once(':')?;
    let project_len = project_len.parse::<usize>().ok()?;
    if project_len == 0 || scoped_id.len() <= project_len {
        return None;
    }
    let (project_id, agent_session_id) = scoped_id.split_at(project_len);
    if project_id.is_empty() || agent_session_id.is_empty() {
        return None;
    }
    Some((project_id, agent_session_id))
}

fn remote_agent_session_dto(project_id: &str, session: &AgentSessionRecord) -> JsonValue {
    let mut value = json!(agent_session_dto(session));
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "remoteSessionId".to_string(),
            json!(remote_session_id_for(project_id, &session.agent_session_id)),
        );
    }
    value
}

fn locate_project_for_remote_start<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    payload: &JsonValue,
) -> CommandResult<LocatedRemoteProject> {
    let projects = visible_remote_project_summaries(app, state)?;

    if let Some(project_id) = payload_string(payload, &["projectId", "project_id"]) {
        return projects
            .into_iter()
            .find(|project| project.registry.project_id == project_id)
            .map(|project| project_summary_location(&project))
            .ok_or_else(CommandError::project_not_found);
    }

    match projects.as_slice() {
        [project] => Ok(project_summary_location(project)),
        [] => Err(CommandError::project_not_found()),
        _ => Err(CommandError::user_fixable(
            "remote_project_required",
            "Remote start requires `projectId` because more than one desktop project is registered.",
        )),
    }
}

fn locate_remote_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    session_id: &str,
) -> CommandResult<LocatedRemoteSession> {
    validate_non_empty(session_id, "agentSessionId")?;
    if session_id == REMOTE_COMPUTER_USE_SESSION_ID {
        let global = ensure_global_computer_use_session_record(app, state)?;
        return Ok(LocatedRemoteSession {
            project_id: global.project_id,
            repo_root: global.repo_root,
            session: global.session,
            remote_session_id: REMOTE_COMPUTER_USE_SESSION_ID.into(),
        });
    }
    if let Some((project_id, agent_session_id)) = parse_project_scoped_remote_session_id(session_id)
    {
        let location = locate_visible_remote_project(app, state, project_id)?;
        let session = project_store::get_agent_session(
            &location.repo_root,
            &location.project_id,
            agent_session_id,
        )?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "remote_session_not_found",
                format!("Xero could not find session `{agent_session_id}`."),
            )
        })?;
        if matches!(session.status, project_store::AgentSessionStatus::Archived) {
            return Err(CommandError::policy_denied(
                "Remote command rejected because this session is archived.",
            ));
        }
        return Ok(LocatedRemoteSession {
            project_id: location.project_id,
            repo_root: location.repo_root,
            session,
            remote_session_id: session_id.to_string(),
        });
    }

    for project in visible_remote_project_summaries(app, state)? {
        let location = project_summary_location(&project);
        if let Some(session) =
            project_store::get_agent_session(&location.repo_root, &location.project_id, session_id)?
        {
            if matches!(session.status, project_store::AgentSessionStatus::Archived) {
                return Err(CommandError::policy_denied(
                    "Remote command rejected because this session is archived.",
                ));
            }
            let remote_session_id = remote_session_id_for(&location.project_id, session_id);
            return Ok(LocatedRemoteSession {
                project_id: location.project_id,
                repo_root: location.repo_root,
                session,
                remote_session_id,
            });
        }
    }

    Err(CommandError::user_fixable(
        "remote_session_not_found",
        format!("Xero could not find session `{session_id}`."),
    ))
}

fn local_agent_session_id(located: &LocatedRemoteSession) -> &str {
    located.session.agent_session_id.as_str()
}

fn remote_session_summaries<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<Vec<JsonValue>> {
    remote_session_summaries_from_projects(visible_remote_project_summaries(app, state)?)
}

fn remote_project_summaries_from_projects(
    projects: Vec<RegistryProjectSummaryRecord>,
) -> CommandResult<Vec<JsonValue>> {
    let mut summaries = Vec::new();
    let mut successful_projects = 0_usize;
    let mut first_error: Option<CommandError> = None;

    for project in projects {
        let location = project_summary_location(&project);
        match project_store::list_agent_sessions(&location.repo_root, &location.project_id, false) {
            Ok(_) => {
                let project_id = location.project_id.clone();
                let project_name = location.project_name.unwrap_or_else(|| project_id.clone());
                successful_projects += 1;
                summaries.push(json!({
                    "projectId": project_id,
                    "projectName": project_name,
                }));
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error.clone());
                }
                log_remote_project_list_skip("project list", &location, &error);
            }
        }
    }

    if successful_projects == 0 {
        if let Some(error) = first_error {
            return Err(error);
        }
    }

    Ok(summaries)
}

fn remote_session_summaries_from_projects(
    projects: Vec<RegistryProjectSummaryRecord>,
) -> CommandResult<Vec<JsonValue>> {
    let mut sessions = Vec::new();
    let mut successful_projects = 0_usize;
    let mut first_error: Option<CommandError> = None;

    for project in projects {
        let location = project_summary_location(&project);
        match project_store::list_agent_sessions(&location.repo_root, &location.project_id, false) {
            Ok(project_sessions) => {
                successful_projects += 1;
                for session in project_sessions {
                    if matches!(
                        session.session_kind,
                        project_store::AgentSessionKind::ComputerUse
                    ) {
                        continue;
                    }
                    sessions.push(remote_session_summary_payload(
                        &location.project_id,
                        location.project_name.as_deref(),
                        &session,
                    ));
                }
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error.clone());
                }
                log_remote_project_list_skip("session list", &location, &error);
            }
        }
    }

    if successful_projects == 0 {
        if let Some(error) = first_error {
            return Err(error);
        }
    }

    Ok(sessions)
}

fn remote_session_summary_payload(
    project_id: &str,
    project_name: Option<&str>,
    session: &AgentSessionRecord,
) -> JsonValue {
    json!({
        "projectId": project_id,
        "projectName": project_name.unwrap_or(project_id),
        "session": {
            "remoteSessionId": remote_session_id_for(project_id, &session.agent_session_id),
            "agentSessionId": &session.agent_session_id,
            "sessionKind": agent_session_kind_value(session.session_kind),
            "title": &session.title,
            "remoteVisible": !matches!(session.status, project_store::AgentSessionStatus::Archived)
                && !matches!(session.session_kind, project_store::AgentSessionKind::ComputerUse),
            "createdAt": &session.created_at,
            "updatedAt": &session.updated_at,
        },
    })
}

fn log_remote_project_list_skip(
    list_name: &str,
    location: &LocatedRemoteProject,
    error: &CommandError,
) {
    eprintln!(
        "[remote-bridge] skipped {} for project {} at {}: {} ({})",
        list_name,
        location.project_id,
        location.repo_root.display(),
        error.message,
        error.code
    );
}

fn remote_session_result_payload(
    project_id: &str,
    project_name: Option<&str>,
    session: &AgentSessionRecord,
) -> JsonValue {
    json!({
        "projectId": project_id,
        "projectName": project_name.unwrap_or(project_id),
        "session": remote_agent_session_dto(project_id, session),
    })
}

fn remote_session_snapshot<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    located: &LocatedRemoteSession,
    computer_id: Option<&str>,
) -> CommandResult<JsonValue> {
    let remote_context = computer_id.map(|computer_id| RemoteRuntimeMediaContext {
        computer_id,
        session_id: located.remote_session_id.as_str(),
    });
    let runs = project_store::load_agent_session_run_snapshots(
        &located.repo_root,
        &located.project_id,
        &located.session.agent_session_id,
    )?
    .into_iter()
    .map(|(snapshot, _usage)| {
        let mut run = agent_run_dto(snapshot);
        enrich_remote_run_media_attachments(
            &located.repo_root,
            &located.project_id,
            remote_context,
            &mut run,
        );
        run
    })
    .collect::<Vec<_>>();
    let runtime_run = load_persisted_runtime_run(
        &located.repo_root,
        &located.project_id,
        &located.session.agent_session_id,
    )?
    .as_ref()
    .map(runtime_run_dto_from_snapshot);
    Ok(json!({
        "schema": "xero.remote_session_snapshot.v1",
        "projectId": located.project_id,
        "session": remote_agent_session_dto(&located.project_id, &located.session),
        "runtimeRun": runtime_run,
        "selectedControls": remote_selected_composer_controls(app, state, located.session.session_kind)?,
        "runs": runs,
        "availableAgents": remote_available_agents(),
        "availableModels": remote_available_models(app, state)?,
        "contextSnapshot": JsonValue::Null,
        "contextSnapshotError": JsonValue::Null,
    }))
}

fn enrich_remote_run_media_attachments(
    repo_root: &Path,
    project_id: &str,
    remote_context: Option<RemoteRuntimeMediaContext<'_>>,
    run: &mut AgentRunDto,
) {
    let Some(remote_context) = remote_context else {
        return;
    };
    for event in &mut run.events {
        if event.event_kind != AgentRunEventKindDto::ToolCompleted {
            continue;
        }
        if !event
            .payload
            .get("ok")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let Some(output) = event.payload.get("output") else {
            continue;
        };
        let media_attachments = extract_runtime_media_attachments(RuntimeMediaExtractionRequest {
            repo_root,
            project_id,
            run_id: &run.run_id,
            event_id: event.id,
            tool_call_id: event.payload.get("toolCallId").and_then(JsonValue::as_str),
            tool_name: event.payload.get("toolName").and_then(JsonValue::as_str),
            output,
            asset_state: None,
            remote_context: Some(remote_context),
        });
        if media_attachments.is_empty() {
            continue;
        }
        if let Some(object) = event.payload.as_object_mut() {
            object.insert("mediaAttachments".into(), json!(media_attachments));
            object.remove("modelVisibleToolResult");
        }
    }
}

fn remote_selected_composer_controls<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    session_kind: project_store::AgentSessionKind,
) -> CommandResult<JsonValue> {
    let Some(value) = read_app_ui_state_value(app, state, COMPOSER_SETTINGS_APP_STATE_KEY)? else {
        return Ok(JsonValue::Null);
    };
    Ok(
        composer_settings_controls_from_state_value(&value, session_kind)
            .unwrap_or(JsonValue::Null),
    )
}

fn composer_settings_controls_from_state_value(
    value: &JsonValue,
    session_kind: project_store::AgentSessionKind,
) -> Option<JsonValue> {
    if value.get("version").and_then(JsonValue::as_u64) != Some(COMPOSER_SETTINGS_VERSION) {
        return None;
    }
    let model_id = json_string_field(value, "modelId")?;
    let mut payload = JsonMap::new();
    payload.insert("modelId".into(), json!(model_id));
    if let Some(provider_profile_id) = json_string_field(value, "providerProfileId") {
        payload.insert("providerProfileId".into(), json!(provider_profile_id));
    }
    if let Some(provider_id) = json_string_field(value, "providerId") {
        payload.insert("providerId".into(), json!(provider_id));
    }
    let runtime_agent_id = match session_kind {
        project_store::AgentSessionKind::ComputerUse => Some(RuntimeAgentIdDto::ComputerUse),
        project_store::AgentSessionKind::Standard => json_string_field(value, "runtimeAgentId")
            .and_then(|agent| parse_runtime_agent_id(agent).ok())
            .filter(|agent| !matches!(agent, RuntimeAgentIdDto::ComputerUse)),
    };
    if let Some(runtime_agent_id) = runtime_agent_id {
        payload.insert("runtimeAgentId".into(), json!(runtime_agent_id.as_str()));
    }
    if let Some(thinking_effort) = json_string_field(value, "thinkingEffort") {
        payload.insert("thinkingEffort".into(), json!(thinking_effort));
    }
    if let Some(approval_mode) = json_string_field(value, "approvalMode") {
        payload.insert("approvalMode".into(), json!(approval_mode));
    }
    if let Some(auto_compact_enabled) = value.get("autoCompactEnabled").and_then(JsonValue::as_bool)
    {
        payload.insert("autoCompactEnabled".into(), json!(auto_compact_enabled));
    }
    Some(JsonValue::Object(payload))
}

fn persist_remote_composer_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    session_kind: project_store::AgentSessionKind,
    controls: &RuntimeRunControlInputDto,
    provider_id: Option<&str>,
) -> CommandResult<JsonValue> {
    let payload = composer_settings_payload_from_controls(session_kind, controls, provider_id);
    write_app_ui_state_value(app, state, COMPOSER_SETTINGS_APP_STATE_KEY, Some(&payload))?;
    let _ = app.emit(COMPOSER_SETTINGS_UPDATED_EVENT, payload.clone());
    Ok(payload)
}

fn composer_settings_payload_from_controls(
    session_kind: project_store::AgentSessionKind,
    controls: &RuntimeRunControlInputDto,
    provider_id: Option<&str>,
) -> JsonValue {
    let mut payload = remote_control_payload_from_controls(controls, provider_id);
    if let Some(object) = payload.as_object_mut() {
        object.insert("version".into(), json!(COMPOSER_SETTINGS_VERSION));
        object.insert(
            "sessionKind".into(),
            json!(agent_session_kind_value(session_kind)),
        );
        object.insert("updatedAt".into(), json!(crate::auth::now_timestamp()));
    }
    payload
}

fn remote_control_payload_from_controls(
    controls: &RuntimeRunControlInputDto,
    provider_id: Option<&str>,
) -> JsonValue {
    let mut payload = JsonMap::new();
    payload.insert(
        "runtimeAgentId".into(),
        json!(controls.runtime_agent_id.as_str()),
    );
    if let Some(agent_definition_id) = controls
        .agent_definition_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("agentDefinitionId".into(), json!(agent_definition_id));
    }
    if let Some(provider_profile_id) = controls
        .provider_profile_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload.insert("providerProfileId".into(), json!(provider_profile_id));
    }
    if let Some(provider_id) = provider_id.map(str::trim).filter(|value| !value.is_empty()) {
        payload.insert("providerId".into(), json!(provider_id));
    }
    payload.insert("modelId".into(), json!(controls.model_id));
    if let Some(thinking_effort) = controls.thinking_effort.as_ref() {
        payload.insert(
            "thinkingEffort".into(),
            json!(thinking_effort_dto_wire_value(thinking_effort)),
        );
    }
    payload.insert(
        "approvalMode".into(),
        json!(approval_mode_wire_value(&controls.approval_mode)),
    );
    payload.insert(
        "planModeRequired".into(),
        json!(controls.plan_mode_required),
    );
    payload.insert(
        "autoCompactEnabled".into(),
        json!(controls.auto_compact_enabled),
    );
    JsonValue::Object(payload)
}

/// Static list of runtime agents the cloud composer can dispatch.
/// Mirrors `parse_runtime_agent_id` (any change there must be reflected here).
fn remote_available_agents() -> Vec<JsonValue> {
    vec![
        json!({ "id": "ask", "label": "Ask" }),
        json!({ "id": "computer_use", "label": "Computer Use" }),
        json!({ "id": "plan", "label": "Plan" }),
        json!({ "id": "engineer", "label": "Engineer" }),
        json!({ "id": "debug", "label": "Debug" }),
        json!({ "id": "crawl", "label": "Crawl" }),
        json!({ "id": "agent_create", "label": "Agent Create" }),
        json!({ "id": "generalist", "label": "Agent" }),
    ]
}

#[derive(Debug, Clone)]
struct RemoteModelOption {
    id: String,
    provider_profile_id: String,
    provider_id: String,
    provider_label: String,
    model_id: String,
    display_name: String,
    thinking: ProviderModelThinkingCapability,
    input_modalities: Vec<String>,
}

/// Returns the credential-backed models the cloud composer surfaces in its dropdown.
fn remote_available_models<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<Vec<JsonValue>> {
    let provider_profiles = load_provider_credentials_view(app, state)?;
    let mut seen = BTreeSet::new();
    let mut options = Vec::new();

    for profile in provider_profiles.profiles() {
        if !profile.readiness().ready {
            continue;
        }

        match load_provider_model_catalog(app, state, &profile.profile_id, false) {
            Ok(catalog) => {
                for model in &catalog.models {
                    add_remote_model_option(
                        &mut options,
                        &mut seen,
                        &profile.profile_id,
                        &profile.provider_id,
                        &profile.label,
                        model,
                    );
                }
                add_remote_model_fallback_option(
                    &mut options,
                    &mut seen,
                    &profile.profile_id,
                    &profile.provider_id,
                    &profile.label,
                    &catalog.configured_model_id,
                );
            }
            Err(error) => {
                eprintln!(
                    "[remote-bridge] provider model catalog unavailable for `{}`: {error}",
                    profile.profile_id
                );
                add_remote_model_fallback_option(
                    &mut options,
                    &mut seen,
                    &profile.profile_id,
                    &profile.provider_id,
                    &profile.label,
                    &profile.model_id,
                );
            }
        }
    }

    options.sort_by(|left, right| {
        left.provider_label
            .cmp(&right.provider_label)
            .then(left.display_name.cmp(&right.display_name))
            .then(left.model_id.cmp(&right.model_id))
    });

    let provider_count = options
        .iter()
        .map(|option| option.provider_profile_id.as_str())
        .collect::<BTreeSet<_>>()
        .len();

    Ok(options
        .into_iter()
        .map(|option| {
            let label = if provider_count > 1 {
                format!("{} · {}", option.display_name, option.provider_label)
            } else {
                option.display_name.clone()
            };
            let effort_options: Vec<&'static str> = option
                .thinking
                .effort_options
                .iter()
                .map(thinking_effort_wire_value)
                .collect();
            let default_effort = option
                .thinking
                .default_effort
                .as_ref()
                .map(thinking_effort_wire_value);
            json!({
                "id": option.id,
                "label": label,
                "modelId": option.model_id,
                "providerId": option.provider_id,
                "providerLabel": option.provider_label,
                "providerProfileId": option.provider_profile_id,
                "thinkingSupported": option.thinking.supported,
                "thinkingEffortOptions": effort_options,
                "defaultThinkingEffort": default_effort,
                "inputModalities": option.input_modalities,
            })
        })
        .collect())
}

fn thinking_effort_wire_value(effort: &ProviderModelThinkingEffort) -> &'static str {
    match effort {
        ProviderModelThinkingEffort::None => "none",
        ProviderModelThinkingEffort::Minimal => "minimal",
        ProviderModelThinkingEffort::Low => "low",
        ProviderModelThinkingEffort::Medium => "medium",
        ProviderModelThinkingEffort::High => "high",
        ProviderModelThinkingEffort::XHigh => "x_high",
    }
}

fn thinking_effort_dto_wire_value(effort: &ProviderModelThinkingEffortDto) -> &'static str {
    match effort {
        ProviderModelThinkingEffortDto::None => "none",
        ProviderModelThinkingEffortDto::Minimal => "minimal",
        ProviderModelThinkingEffortDto::Low => "low",
        ProviderModelThinkingEffortDto::Medium => "medium",
        ProviderModelThinkingEffortDto::High => "high",
        ProviderModelThinkingEffortDto::XHigh => "x_high",
    }
}

fn approval_mode_wire_value(mode: &RuntimeRunApprovalModeDto) -> &'static str {
    match mode {
        RuntimeRunApprovalModeDto::Suggest => "suggest",
        RuntimeRunApprovalModeDto::AutoEdit => "auto_edit",
        RuntimeRunApprovalModeDto::Yolo => "yolo",
    }
}

fn add_remote_model_option(
    options: &mut Vec<RemoteModelOption>,
    seen: &mut BTreeSet<String>,
    provider_profile_id: &str,
    provider_id: &str,
    provider_label: &str,
    model: &ProviderModelRecord,
) {
    let model_id = model.model_id.trim();
    if model_id.is_empty() {
        return;
    }
    let display_name = model.display_name.trim();
    push_remote_model_option(
        options,
        seen,
        RemoteModelProviderContext {
            provider_profile_id,
            provider_id,
            provider_label,
        },
        RemoteModelOptionInput {
            model_id,
            display_name: if display_name.is_empty() {
                model_id
            } else {
                display_name
            },
            thinking: model.thinking.clone(),
            input_modalities: model.input_modalities.clone(),
        },
    );
}

fn add_remote_model_fallback_option(
    options: &mut Vec<RemoteModelOption>,
    seen: &mut BTreeSet<String>,
    provider_profile_id: &str,
    provider_id: &str,
    provider_label: &str,
    model_id: &str,
) {
    let model_id = model_id.trim();
    if model_id.is_empty() {
        return;
    }
    push_remote_model_option(
        options,
        seen,
        RemoteModelProviderContext {
            provider_profile_id,
            provider_id,
            provider_label,
        },
        RemoteModelOptionInput {
            model_id,
            display_name: model_id,
            thinking: ProviderModelThinkingCapability {
                supported: false,
                effort_options: Vec::new(),
                default_effort: None,
            },
            input_modalities: Vec::new(),
        },
    );
}

struct RemoteModelProviderContext<'a> {
    provider_profile_id: &'a str,
    provider_id: &'a str,
    provider_label: &'a str,
}

struct RemoteModelOptionInput<'a> {
    model_id: &'a str,
    display_name: &'a str,
    thinking: ProviderModelThinkingCapability,
    input_modalities: Vec<String>,
}

fn push_remote_model_option(
    options: &mut Vec<RemoteModelOption>,
    seen: &mut BTreeSet<String>,
    provider: RemoteModelProviderContext<'_>,
    model: RemoteModelOptionInput<'_>,
) {
    let id = remote_model_option_id(provider.provider_profile_id, model.model_id);
    if !seen.insert(id.clone()) {
        return;
    }
    options.push(RemoteModelOption {
        id,
        provider_profile_id: provider.provider_profile_id.to_string(),
        provider_id: provider.provider_id.to_string(),
        provider_label: provider.provider_label.to_string(),
        model_id: model.model_id.to_string(),
        display_name: model.display_name.to_string(),
        thinking: model.thinking,
        input_modalities: model.input_modalities,
    });
}

fn remote_model_option_id(provider_profile_id: &str, model_id: &str) -> String {
    format!("{}:{}", provider_profile_id.trim(), model_id.trim())
}

fn project_summary_location(project: &RegistryProjectSummaryRecord) -> LocatedRemoteProject {
    LocatedRemoteProject {
        project_id: project.registry.project_id.clone(),
        project_name: Some(project.project.name.clone()),
        repo_root: std::path::PathBuf::from(&project.registry.root_path),
    }
}

fn locate_visible_remote_project<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
) -> CommandResult<LocatedRemoteProject> {
    visible_remote_project_summaries(app, state)?
        .into_iter()
        .find(|project| project.registry.project_id == project_id)
        .map(|project| project_summary_location(&project))
        .ok_or_else(CommandError::project_not_found)
}

fn visible_remote_project_summaries<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<Vec<RegistryProjectSummaryRecord>> {
    let registry_path = state.global_db_path(app)?;
    load_visible_project_summaries_from_registry(&registry_path)
}

fn project_name_for_id<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
) -> CommandResult<Option<String>> {
    Ok(visible_remote_project_summaries(app, state)?
        .into_iter()
        .find(|project| project.registry.project_id == project_id)
        .map(|project| project.project.name))
}

fn remote_attachments_from_payload(
    payload: &JsonValue,
) -> CommandResult<Vec<StagedAgentAttachmentDto>> {
    let Some(array) = payload
        .get("attachments")
        .and_then(|value| value.as_array())
    else {
        return Ok(Vec::new());
    };
    let mut attachments = Vec::with_capacity(array.len());
    for entry in array {
        let dto: StagedAgentAttachmentDto =
            serde_json::from_value(entry.clone()).map_err(|error| {
                CommandError::user_fixable(
                    "remote_attachment_invalid",
                    format!("Remote attachment payload was malformed: {error}"),
                )
            })?;
        attachments.push(dto);
    }
    Ok(attachments)
}

fn remote_session_kind_from_payload(
    payload: &JsonValue,
) -> CommandResult<project_store::AgentSessionKind> {
    if let Some(value) = payload_string(payload, &["sessionKind", "session_kind"]) {
        return match value.trim() {
            "standard" => Ok(project_store::AgentSessionKind::Standard),
            "computer_use" => Ok(project_store::AgentSessionKind::ComputerUse),
            other => Err(CommandError::user_fixable(
                "remote_session_kind_unsupported",
                format!("Remote start does not support session kind `{other}`."),
            )),
        };
    }

    if remote_payload_runtime_agent_id(payload)? == Some(RuntimeAgentIdDto::ComputerUse) {
        return Ok(project_store::AgentSessionKind::ComputerUse);
    }

    Ok(project_store::AgentSessionKind::Standard)
}

fn remote_default_agent_for_session_kind(
    session_kind: project_store::AgentSessionKind,
) -> Option<RuntimeAgentIdDto> {
    match session_kind {
        project_store::AgentSessionKind::Standard => None,
        project_store::AgentSessionKind::ComputerUse => Some(RuntimeAgentIdDto::ComputerUse),
    }
}

fn remote_payload_runtime_agent_id(
    payload: &JsonValue,
) -> CommandResult<Option<RuntimeAgentIdDto>> {
    payload_string(payload, &["agent", "runtimeAgentId", "runtime_agent_id"])
        .map(parse_runtime_agent_id)
        .transpose()
}

fn ensure_remote_payload_matches_session_kind(
    session_kind: project_store::AgentSessionKind,
    payload: &JsonValue,
) -> CommandResult<()> {
    if let Some(runtime_agent_id) = remote_payload_runtime_agent_id(payload)? {
        ensure_remote_agent_matches_session_kind(session_kind, runtime_agent_id)?;
    }
    Ok(())
}

fn ensure_remote_controls_match_session_kind(
    session_kind: project_store::AgentSessionKind,
    controls: Option<&RuntimeRunControlInputDto>,
) -> CommandResult<()> {
    if let Some(controls) = controls {
        ensure_remote_agent_matches_session_kind(session_kind, controls.runtime_agent_id)?;
    }
    Ok(())
}

fn ensure_remote_agent_matches_session_kind(
    session_kind: project_store::AgentSessionKind,
    runtime_agent_id: RuntimeAgentIdDto,
) -> CommandResult<()> {
    match (session_kind, runtime_agent_id) {
        (project_store::AgentSessionKind::ComputerUse, RuntimeAgentIdDto::ComputerUse) => Ok(()),
        (project_store::AgentSessionKind::ComputerUse, _) => Err(CommandError::user_fixable(
            "computer_use_agent_required",
            "Computer Use sessions must run with the Computer Use agent.",
        )),
        (project_store::AgentSessionKind::Standard, RuntimeAgentIdDto::ComputerUse) => {
            Err(CommandError::user_fixable(
                "computer_use_session_required",
                "The Computer Use agent can only run inside a Computer Use session.",
            ))
        }
        (project_store::AgentSessionKind::Standard, _) => Ok(()),
    }
}

fn agent_session_kind_value(session_kind: project_store::AgentSessionKind) -> &'static str {
    match session_kind {
        project_store::AgentSessionKind::Standard => "standard",
        project_store::AgentSessionKind::ComputerUse => "computer_use",
    }
}

fn remote_run_controls_from_payload(
    payload: &JsonValue,
    fallback: Option<&RuntimeRunControlInputDto>,
    default_runtime_agent_id: Option<RuntimeAgentIdDto>,
) -> CommandResult<Option<RuntimeRunControlInputDto>> {
    let runtime_agent_id =
        match payload_string(payload, &["agent", "runtimeAgentId", "runtime_agent_id"]) {
            Some(agent) => parse_runtime_agent_id(agent)?,
            None => match default_runtime_agent_id {
                Some(agent_id) => agent_id,
                None => return Ok(None),
            },
        };
    let Some(model_id) = payload_string(payload, &["modelId", "model_id"]).or_else(|| {
        fallback
            .map(|controls| controls.model_id.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }) else {
        return Ok(None);
    };
    let thinking_effort = match payload_string(payload, &["thinkingEffort", "thinking_effort"]) {
        Some(value) => Some(parse_thinking_effort(value)?),
        None => fallback.and_then(|controls| controls.thinking_effort.clone()),
    };
    let approval_mode = match payload_string(payload, &["approvalMode", "approval_mode"]) {
        Some(value) => parse_approval_mode(value)?,
        None => fallback
            .map(|controls| controls.approval_mode.clone())
            .unwrap_or(RuntimeRunApprovalModeDto::Suggest),
    };
    Ok(Some(RuntimeRunControlInputDto {
        runtime_agent_id,
        agent_definition_id: Some(runtime_agent_id.as_str().to_string()),
        agent_definition_version: None,
        provider_profile_id: payload_string(payload, &["providerProfileId", "provider_profile_id"])
            .map(ToOwned::to_owned)
            .or_else(|| fallback.and_then(|controls| controls.provider_profile_id.clone())),
        model_id: model_id.to_string(),
        thinking_effort,
        approval_mode,
        plan_mode_required: payload_bool(payload, &["planModeRequired", "plan_mode_required"])
            .or_else(|| fallback.map(|controls| controls.plan_mode_required))
            .unwrap_or(false),
        auto_compact_enabled: payload_bool(
            payload,
            &["autoCompactEnabled", "auto_compact_enabled"],
        )
        .or_else(|| fallback.map(|controls| controls.auto_compact_enabled))
        .unwrap_or(true),
    }))
}

fn selected_runtime_run_controls(
    snapshot: &crate::db::project_store::RuntimeRunSnapshotRecord,
) -> RuntimeRunControlInputDto {
    if let Some(pending) = snapshot.controls.pending.as_ref() {
        return RuntimeRunControlInputDto {
            runtime_agent_id: pending.runtime_agent_id,
            agent_definition_id: pending.agent_definition_id.clone(),
            agent_definition_version: pending.agent_definition_version,
            provider_profile_id: pending.provider_profile_id.clone(),
            model_id: pending.model_id.clone(),
            thinking_effort: pending.thinking_effort.clone(),
            approval_mode: pending.approval_mode.clone(),
            plan_mode_required: pending.plan_mode_required,
            auto_compact_enabled: pending.auto_compact_enabled,
        };
    }

    RuntimeRunControlInputDto {
        runtime_agent_id: snapshot.controls.active.runtime_agent_id,
        agent_definition_id: snapshot.controls.active.agent_definition_id.clone(),
        agent_definition_version: snapshot.controls.active.agent_definition_version,
        provider_profile_id: snapshot.controls.active.provider_profile_id.clone(),
        model_id: snapshot.controls.active.model_id.clone(),
        thinking_effort: snapshot.controls.active.thinking_effort.clone(),
        approval_mode: snapshot.controls.active.approval_mode.clone(),
        plan_mode_required: snapshot.controls.active.plan_mode_required,
        auto_compact_enabled: snapshot.controls.active.auto_compact_enabled,
    }
}

fn parse_thinking_effort(value: &str) -> CommandResult<ProviderModelThinkingEffortDto> {
    match value.trim() {
        "none" => Ok(ProviderModelThinkingEffortDto::None),
        "minimal" => Ok(ProviderModelThinkingEffortDto::Minimal),
        "low" => Ok(ProviderModelThinkingEffortDto::Low),
        "medium" => Ok(ProviderModelThinkingEffortDto::Medium),
        "high" => Ok(ProviderModelThinkingEffortDto::High),
        "x_high" | "xhigh" => Ok(ProviderModelThinkingEffortDto::XHigh),
        other => Err(CommandError::user_fixable(
            "remote_thinking_effort_unsupported",
            format!("Remote command does not support thinking effort `{other}`."),
        )),
    }
}

fn parse_approval_mode(value: &str) -> CommandResult<RuntimeRunApprovalModeDto> {
    match value.trim().replace('-', "_").to_ascii_lowercase().as_str() {
        "suggest" => Ok(RuntimeRunApprovalModeDto::Suggest),
        "auto_edit" | "autoedit" => Ok(RuntimeRunApprovalModeDto::AutoEdit),
        "yolo" => Ok(RuntimeRunApprovalModeDto::Yolo),
        other => Err(CommandError::user_fixable(
            "remote_approval_mode_unsupported",
            format!("Remote command does not support approval mode `{other}`."),
        )),
    }
}

fn parse_runtime_agent_id(value: &str) -> CommandResult<RuntimeAgentIdDto> {
    match value.trim() {
        "ask" => Ok(RuntimeAgentIdDto::Ask),
        "computer_use" | "computer" => Ok(RuntimeAgentIdDto::ComputerUse),
        "plan" => Ok(RuntimeAgentIdDto::Plan),
        "engineer" => Ok(RuntimeAgentIdDto::Engineer),
        "debug" => Ok(RuntimeAgentIdDto::Debug),
        "crawl" => Ok(RuntimeAgentIdDto::Crawl),
        "agent_create" => Ok(RuntimeAgentIdDto::AgentCreate),
        "generalist" | "agent" => Ok(RuntimeAgentIdDto::Generalist),
        other => Err(CommandError::user_fixable(
            "remote_agent_unsupported",
            format!("Remote start does not support agent `{other}`."),
        )),
    }
}

fn required_command_session(command: &InboundCommand) -> CommandResult<&str> {
    command
        .session_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CommandError::invalid_request("sessionId"))
}

fn required_payload_string<'a>(payload: &'a JsonValue, keys: &[&str]) -> CommandResult<&'a str> {
    payload_string(payload, keys).ok_or_else(|| {
        CommandError::user_fixable(
            "remote_command_payload_invalid",
            format!("Remote command payload must include `{}`.", keys[0]),
        )
    })
}

fn payload_string<'a>(payload: &'a JsonValue, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| payload.get(*key).and_then(JsonValue::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn payload_value<'a>(payload: &'a JsonValue, keys: &[&str]) -> Option<&'a JsonValue> {
    keys.iter().find_map(|key| payload.get(*key))
}

fn json_string_field<'a>(payload: &'a JsonValue, key: &str) -> Option<&'a str> {
    payload
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn payload_u64(payload: &JsonValue, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| payload.get(*key).and_then(JsonValue::as_u64))
}

fn payload_i32(payload: &JsonValue, keys: &[&str]) -> Option<i32> {
    keys.iter().find_map(|key| {
        payload
            .get(*key)
            .and_then(JsonValue::as_i64)
            .and_then(|value| i32::try_from(value).ok())
    })
}

fn payload_bool(payload: &JsonValue, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| payload.get(*key).and_then(JsonValue::as_bool))
}

fn payload_string_array(payload: &JsonValue, keys: &[&str]) -> Vec<String> {
    keys.iter()
        .find_map(|key| payload.get(*key).and_then(JsonValue::as_array))
        .map(|values| {
            values
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn json_unix_millis(value: &JsonValue) -> Option<i128> {
    if let Some(value) = value.as_i64() {
        return Some(value as i128);
    }
    if let Some(value) = value.as_u64() {
        return Some(value as i128);
    }
    value.as_str()?.trim().parse::<i128>().ok()
}

fn current_unix_millis() -> i128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i128)
        .unwrap_or_default()
}

fn new_bridge_for_app<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<AppRemoteBridge> {
    let remote_dir = state.app_data_dir(app)?.join(REMOTE_DIR);

    Ok(RemoteBridge::new(
        BridgeConfig::from_env_or_local("Xero Desktop"),
        FileIdentityStore::new(remote_dir.join(IDENTITY_FILE)),
    ))
}

fn registered_identity_exists<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<bool> {
    let remote_dir = state.app_data_dir(app)?.join(REMOTE_DIR);
    let identity_store = FileIdentityStore::new(remote_dir.join(IDENTITY_FILE));
    Ok(identity_store
        .load()
        .map_err(map_bridge_error)?
        .and_then(|identity| identity.desktop_jwt)
        .is_some())
}

fn map_bridge_error(error: BridgeError) -> CommandError {
    match error {
        BridgeError::Http(_)
        | BridgeError::HttpStatus { .. }
        | BridgeError::InvalidRelayUrl { .. }
        | BridgeError::UnsupportedUrlScheme(_)
        | BridgeError::WebSocket(_)
        | BridgeError::Io(_) => {
            CommandError::retryable("remote_bridge_relay_unavailable", error.to_string())
        }
        BridgeError::IdentityRead { .. }
        | BridgeError::IdentityWrite { .. }
        | BridgeError::IdentityDecode { .. }
        | BridgeError::StateRead { .. }
        | BridgeError::StateWrite { .. }
        | BridgeError::StateDecode { .. }
        | BridgeError::Encode(_)
        | BridgeError::Decode(_)
        | BridgeError::Json(_)
        | BridgeError::MissingServerField(_)
        | BridgeError::OutboundQueueFull { .. }
        | BridgeError::LockPoisoned => {
            CommandError::system_fault("remote_bridge_failed", error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Cursor, path::Path};

    use rusqlite::{params, Connection};

    use crate::{
        commands::{
            AgentRunEventDto, AgentRunStatusDto, ProjectOriginDto, ProjectSummaryDto,
            HARNESS_FIXTURE_PROJECT_ID,
        },
        db::{
            configure_connection, migrations::migrations, register_project_database_path_for_tests,
        },
        registry::{self, RegistryProjectRecord},
    };

    fn seed_project_database(repo_root: &Path, project_id: &str) {
        fs::create_dir_all(repo_root).expect("repo root");
        let database_path = repo_root
            .parent()
            .expect("repo parent")
            .join("app-data")
            .join("projects")
            .join(project_id)
            .join("state.db");
        fs::create_dir_all(database_path.parent().expect("database parent")).expect("database dir");

        let mut connection = Connection::open(&database_path).expect("open project database");
        configure_connection(&connection).expect("configure project database");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate project database");
        connection
            .execute(
                "INSERT INTO projects (id, name, description, milestone) VALUES (?1, 'Project', '', '')",
                params![project_id],
            )
            .expect("insert project row");
        connection
            .execute(
                r#"
                INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo)
                VALUES (?1, ?2, ?3, 'Project', 'main', 'abc123', 1)
                "#,
                params![
                    format!("repo-{project_id}"),
                    project_id,
                    repo_root.to_string_lossy().as_ref()
                ],
            )
            .expect("insert repository row");

        register_project_database_path_for_tests(repo_root, database_path);
    }

    fn project_summary(
        project_id: &str,
        repository_id: &str,
        repo_root: &Path,
    ) -> RegistryProjectSummaryRecord {
        RegistryProjectSummaryRecord {
            registry: RegistryProjectRecord {
                project_id: project_id.into(),
                repository_id: repository_id.into(),
                root_path: repo_root.to_string_lossy().into_owned(),
            },
            project: ProjectSummaryDto {
                id: project_id.into(),
                name: repo_root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(project_id)
                    .into(),
                description: String::new(),
                milestone: String::new(),
                project_origin: ProjectOriginDto::Unknown,
                total_phases: 0,
                completed_phases: 0,
                active_phase: 0,
                branch: None,
                runtime: None,
                start_targets: Vec::new(),
            },
        }
    }

    fn fallback_screenshot(width: u32, height: u32) -> AutonomousDesktopScreenshot {
        AutonomousDesktopScreenshot {
            path: "/tmp/xero-fallback.png".into(),
            width,
            height,
            scale_factor: 2.0,
            captured_at: "2026-05-26T10:00:00Z".into(),
        }
    }

    fn sample_png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8, 255])
        });
        let mut cursor = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut cursor, image::ImageFormat::Png)
            .expect("encode sample png");
        cursor.into_inner()
    }

    fn fallback_controls(model_id: &str) -> RuntimeRunControlInputDto {
        RuntimeRunControlInputDto {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: Some("engineer".into()),
            agent_definition_version: None,
            provider_profile_id: Some("profile-openai".into()),
            model_id: model_id.into(),
            thinking_effort: None,
            approval_mode: RuntimeRunApprovalModeDto::Yolo,
            plan_mode_required: true,
            auto_compact_enabled: true,
        }
    }

    #[test]
    fn remote_controls_use_payload_model_when_present() {
        let fallback = fallback_controls("gpt-5.4");
        let controls = remote_run_controls_from_payload(
            &json!({
                "agent": "ask",
                "modelId": "gpt-5.5",
                "providerProfileId": "openai_codex-default",
            }),
            Some(&fallback),
            None,
        )
        .expect("controls should parse")
        .expect("controls should be present");

        assert_eq!(controls.runtime_agent_id, RuntimeAgentIdDto::Ask);
        assert_eq!(controls.model_id, "gpt-5.5");
        assert_eq!(
            controls.provider_profile_id.as_deref(),
            Some("openai_codex-default")
        );
        assert_eq!(controls.approval_mode, RuntimeRunApprovalModeDto::Yolo);
        assert!(controls.plan_mode_required);
    }

    #[test]
    fn remote_controls_use_payload_approval_mode_when_present() {
        let fallback = fallback_controls("gpt-5.4");
        let controls = remote_run_controls_from_payload(
            &json!({
                "agent": "engineer",
                "modelId": "gpt-5.5",
                "approvalMode": "auto_edit",
            }),
            Some(&fallback),
            None,
        )
        .expect("controls should parse")
        .expect("controls should be present");

        assert_eq!(controls.approval_mode, RuntimeRunApprovalModeDto::AutoEdit);
    }

    #[test]
    fn remote_model_option_id_scopes_models_by_provider_profile() {
        assert_eq!(
            remote_model_option_id("openai_codex-default", "gpt-5.5"),
            "openai_codex-default:gpt-5.5"
        );
        assert_eq!(
            remote_model_option_id("bedrock-default", "anthropic.claude-v1:0"),
            "bedrock-default:anthropic.claude-v1:0"
        );
    }

    #[test]
    fn project_scoped_remote_session_ids_round_trip_duplicate_agent_session_ids() {
        let mesh = remote_session_id_for("mesh-lang", "agent-session-main");
        let xero = remote_session_id_for("xero", "agent-session-main");

        assert_eq!(mesh, "project:9:mesh-langagent-session-main");
        assert_eq!(xero, "project:4:xeroagent-session-main");
        assert_ne!(mesh, xero);
        assert_eq!(
            parse_project_scoped_remote_session_id(&mesh),
            Some(("mesh-lang", "agent-session-main"))
        );
        assert_eq!(
            parse_project_scoped_remote_session_id(&xero),
            Some(("xero", "agent-session-main"))
        );
        assert_eq!(parse_project_scoped_remote_session_id("session-main"), None);
    }

    #[test]
    fn cloud_theme_payload_includes_custom_tokens_only_for_custom_ids() {
        let custom = json!({
            "id": "custom-ember",
            "colors": { "background": "#fff1e8" },
        });

        assert_eq!(
            custom_theme_for_theme_id("custom-ember", Some(custom.clone())),
            Some(custom)
        );
        assert_eq!(custom_theme_for_theme_id("midnight", Some(json!({}))), None);
    }

    #[test]
    fn cloud_theme_state_finds_matching_custom_theme() {
        let themes = json!([
            { "id": "custom-ocean", "colors": { "background": "#001122" } },
            { "id": "custom-ember", "colors": { "background": "#fff1e8" } }
        ]);

        let theme = custom_theme_from_state_value("custom-ember", themes).expect("custom theme");

        assert_eq!(theme["id"], json!("custom-ember"));
        assert_eq!(theme["colors"]["background"], json!("#fff1e8"));
    }

    #[test]
    fn remote_controls_fall_back_to_current_settings() {
        let fallback = fallback_controls("gpt-5.5");
        let controls = remote_run_controls_from_payload(
            &json!({
                "agent": "ask",
                "modelId": null,
            }),
            Some(&fallback),
            None,
        )
        .expect("controls should parse")
        .expect("controls should be present");

        assert_eq!(controls.runtime_agent_id, RuntimeAgentIdDto::Ask);
        assert_eq!(controls.model_id, "gpt-5.5");
        assert_eq!(
            controls.provider_profile_id.as_deref(),
            Some("profile-openai")
        );
        assert_eq!(controls.approval_mode, RuntimeRunApprovalModeDto::Yolo);
        assert!(controls.plan_mode_required);
    }

    #[test]
    fn composer_settings_state_supplies_model_but_not_computer_use_agent_for_standard_sessions() {
        let value = json!({
            "version": COMPOSER_SETTINGS_VERSION,
            "runtimeAgentId": "computer_use",
            "providerProfileId": "xai-default",
            "providerId": "xai",
            "modelId": "grok-4.3",
            "thinkingEffort": "low",
            "autoCompactEnabled": false,
        });

        let standard = composer_settings_controls_from_state_value(
            &value,
            project_store::AgentSessionKind::Standard,
        )
        .expect("standard composer controls");
        assert_eq!(standard["modelId"], json!("grok-4.3"));
        assert_eq!(standard["providerProfileId"], json!("xai-default"));
        assert_eq!(standard.get("runtimeAgentId"), None);

        let computer_use = composer_settings_controls_from_state_value(
            &value,
            project_store::AgentSessionKind::ComputerUse,
        )
        .expect("computer use composer controls");
        assert_eq!(computer_use["runtimeAgentId"], json!("computer_use"));
        assert_eq!(computer_use["modelId"], json!("grok-4.3"));
    }

    #[test]
    fn composer_settings_payload_round_trips_runtime_controls_for_cloud() {
        let controls = RuntimeRunControlInputDto {
            runtime_agent_id: RuntimeAgentIdDto::Debug,
            agent_definition_id: Some("debug".into()),
            agent_definition_version: None,
            provider_profile_id: Some("xai-default".into()),
            model_id: "grok-4.3".into(),
            thinking_effort: Some(ProviderModelThinkingEffortDto::Low),
            approval_mode: RuntimeRunApprovalModeDto::Yolo,
            plan_mode_required: false,
            auto_compact_enabled: false,
        };

        let payload = composer_settings_payload_from_controls(
            project_store::AgentSessionKind::Standard,
            &controls,
            Some("xai"),
        );

        assert_eq!(payload["version"], json!(COMPOSER_SETTINGS_VERSION));
        assert_eq!(payload["runtimeAgentId"], json!("debug"));
        assert_eq!(payload["providerId"], json!("xai"));
        assert_eq!(payload["providerProfileId"], json!("xai-default"));
        assert_eq!(payload["modelId"], json!("grok-4.3"));
        assert_eq!(payload["thinkingEffort"], json!("low"));
        assert_eq!(payload["approvalMode"], json!("yolo"));
        assert_eq!(payload["autoCompactEnabled"], json!(false));
    }

    #[test]
    fn remote_controls_are_omitted_without_agent_and_model_pair() {
        assert!(remote_run_controls_from_payload(
            &json!({
                "agent": "ask",
                "modelId": null,
            }),
            None,
            None,
        )
        .expect("controls should parse")
        .is_none());

        assert!(remote_run_controls_from_payload(
            &json!({
                "message": "What is 1+1?",
            }),
            Some(&fallback_controls("gpt-5.5")),
            None,
        )
        .expect("controls should parse")
        .is_none());
    }

    #[test]
    fn computer_use_session_kind_infers_and_locks_remote_controls() {
        assert_eq!(
            remote_session_kind_from_payload(&json!({
                "agent": "computer_use",
            }))
            .expect("session kind"),
            project_store::AgentSessionKind::ComputerUse
        );

        let controls = remote_run_controls_from_payload(
            &json!({
                "sessionKind": "computer_use",
                "modelId": "gpt-5.5",
            }),
            None,
            Some(RuntimeAgentIdDto::ComputerUse),
        )
        .expect("controls")
        .expect("computer-use controls");

        assert_eq!(controls.runtime_agent_id, RuntimeAgentIdDto::ComputerUse);
        assert_eq!(
            controls.agent_definition_id.as_deref(),
            Some("computer_use")
        );
        assert!(ensure_remote_payload_matches_session_kind(
            project_store::AgentSessionKind::ComputerUse,
            &json!({ "agent": "engineer" }),
        )
        .is_err());
    }

    #[test]
    fn remote_session_result_payload_matches_cloud_directory_shape() {
        let session = AgentSessionRecord {
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            session_kind: project_store::AgentSessionKind::Standard,
            title: "Simple Addition".into(),
            summary: String::new(),
            status: project_store::AgentSessionStatus::Active,
            selected: true,
            remote_visible: false,
            created_at: "2026-05-20T20:40:00Z".into(),
            updated_at: "2026-05-20T20:42:00Z".into(),
            archived_at: None,
            last_run_id: None,
            last_runtime_kind: None,
            last_provider_id: None,
            lineage: None,
        };

        let payload = remote_session_result_payload("project-1", Some("Mesh Lang"), &session);

        assert_eq!(payload["projectId"], "project-1");
        assert_eq!(payload["projectName"], "Mesh Lang");
        assert_eq!(
            payload["session"]["remoteSessionId"],
            "project:9:project-1session-1"
        );
        assert_eq!(payload["session"]["agentSessionId"], "session-1");
        assert_eq!(payload["session"]["sessionKind"], "standard");
        assert_eq!(payload["session"]["title"], "Simple Addition");
        assert_eq!(payload["session"]["remoteVisible"], true);
        assert_eq!(payload["session"]["updatedAt"], "2026-05-20T20:42:00Z");
    }

    #[test]
    fn enrich_remote_run_media_attachments_adds_remote_artifacts_to_snapshot_events() {
        const ONE_BY_ONE_PNG: &str =
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMB/axj3nQAAAAASUVORK5CYII=";
        let tempdir = tempfile::tempdir().expect("tempdir");
        let screenshot_path = tempdir.path().join("screen.png");
        let png_bytes = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .decode(ONE_BY_ONE_PNG)
                .expect("png")
        };
        fs::write(&screenshot_path, png_bytes).expect("write screenshot");
        let mut run = AgentRunDto {
            runtime_agent_id: RuntimeAgentIdDto::ComputerUse,
            agent_definition_id: "computer_use".into(),
            agent_definition_version: 1,
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-1".into(),
            trace_id: "trace-1".into(),
            lineage_kind: "root".into(),
            parent_run_id: None,
            parent_trace_id: None,
            parent_subagent_id: None,
            subagent_role: None,
            provider_id: "openai".into(),
            model_id: "gpt-5.5".into(),
            status: AgentRunStatusDto::Completed,
            prompt: "take a screenshot".into(),
            system_prompt: String::new(),
            started_at: "2026-05-30T19:00:00Z".into(),
            last_heartbeat_at: None,
            completed_at: Some("2026-05-30T19:01:00Z".into()),
            cancelled_at: None,
            last_error_code: None,
            last_error: None,
            updated_at: "2026-05-30T19:01:00Z".into(),
            messages: Vec::new(),
            events: vec![AgentRunEventDto {
                id: 42,
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                event_kind: AgentRunEventKindDto::ToolCompleted,
                payload: json!({
                    "toolCallId": "call-macos-screenshot",
                    "toolName": "macos_automation",
                    "ok": true,
                    "modelVisibleToolResult": "Screenshot captured.",
                    "output": {
                        "kind": "macos_automation",
                        "screenshot": {
                            "path": screenshot_path.to_string_lossy(),
                            "width": 1,
                            "height": 1,
                        },
                    },
                }),
                created_at: "2026-05-30T19:00:30Z".into(),
            }],
            tool_calls: Vec::new(),
            file_changes: Vec::new(),
            checkpoints: Vec::new(),
            action_requests: Vec::new(),
        };

        enrich_remote_run_media_attachments(
            tempdir.path(),
            "project-1",
            Some(RemoteRuntimeMediaContext {
                computer_id: "computer-1",
                session_id: "remote-session-1",
            }),
            &mut run,
        );

        let payload = &run.events[0].payload;
        let attachments = payload["mediaAttachments"]
            .as_array()
            .expect("media attachments");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0]["mediaType"], "image/png");
        assert_eq!(attachments[0]["title"], "macOS screenshot");
        assert_eq!(attachments[0]["source"]["kind"], "remote_artifact");
        assert_eq!(attachments[0]["source"]["computer_id"], "computer-1");
        assert_eq!(attachments[0]["source"]["session_id"], "remote-session-1");
        assert!(attachments[0]["source"]["artifact_id"]
            .as_str()
            .is_some_and(|artifact_id| artifact_id.contains("run-1-42")));
        assert!(payload.get("modelVisibleToolResult").is_none());
    }

    #[test]
    fn remote_session_summaries_skip_stale_project_registry_entries() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let valid_root = tempdir.path().join("valid");
        let stale_root = tempdir.path().join("stale");
        seed_project_database(&valid_root, "project-valid");
        seed_project_database(&stale_root, "project-other");

        let session = project_store::create_agent_session(
            &valid_root,
            &AgentSessionCreateRecord {
                project_id: "project-valid".into(),
                title: "Main".into(),
                summary: String::new(),
                selected: true,
                session_kind: project_store::AgentSessionKind::Standard,
            },
        )
        .expect("create valid session");

        let summaries = remote_session_summaries_from_projects(vec![
            project_summary("project-stale", "repo-stale", &stale_root),
            project_summary("project-valid", "repo-valid", &valid_root),
        ])
        .expect("session summaries");

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0]["projectId"], "project-valid");
        assert_eq!(
            summaries[0]["session"]["remoteSessionId"],
            format!(
                "project:{}:{}{}",
                "project-valid".len(),
                "project-valid",
                session.agent_session_id
            )
        );
        assert_eq!(
            summaries[0]["session"]["agentSessionId"],
            session.agent_session_id
        );
    }

    #[test]
    fn remote_session_summaries_follow_client_visible_project_filter() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let registry_path = tempdir.path().join("xero.db");
        let visible_root = tempdir.path().join("clippster-mono");
        let fixture_root = tempdir
            .path()
            .join("developer-tool-harness")
            .join("fixture");
        seed_project_database(&visible_root, "project-visible");
        seed_project_database(&fixture_root, HARNESS_FIXTURE_PROJECT_ID);

        project_store::create_agent_session(
            &visible_root,
            &AgentSessionCreateRecord {
                project_id: "project-visible".into(),
                title: "Visible Chat".into(),
                summary: String::new(),
                selected: true,
                session_kind: project_store::AgentSessionKind::Standard,
            },
        )
        .expect("create visible session");
        project_store::create_agent_session(
            &fixture_root,
            &AgentSessionCreateRecord {
                project_id: HARNESS_FIXTURE_PROJECT_ID.into(),
                title: "Fixture Chat".into(),
                summary: String::new(),
                selected: true,
                session_kind: project_store::AgentSessionKind::Standard,
            },
        )
        .expect("create fixture session");

        registry::replace_projects(
            &registry_path,
            vec![
                RegistryProjectRecord {
                    project_id: HARNESS_FIXTURE_PROJECT_ID.into(),
                    repository_id: "repo-fixture".into(),
                    root_path: fixture_root.to_string_lossy().into_owned(),
                },
                RegistryProjectRecord {
                    project_id: "project-visible".into(),
                    repository_id: "repo-visible".into(),
                    root_path: visible_root.to_string_lossy().into_owned(),
                },
            ],
        )
        .expect("seed registry");

        let visible_projects = load_visible_project_summaries_from_registry(&registry_path)
            .expect("client-visible project summaries");
        assert_eq!(visible_projects.len(), 1);
        assert_eq!(visible_projects[0].registry.project_id, "project-visible");

        let summaries = remote_session_summaries_from_projects(visible_projects)
            .expect("remote session summaries");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0]["projectId"], "project-visible");
        assert_eq!(summaries[0]["session"]["title"], "Visible Chat");
    }

    #[test]
    fn remote_session_summaries_error_when_every_project_is_unreadable() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let stale_root = tempdir.path().join("stale");
        seed_project_database(&stale_root, "project-other");

        let error = remote_session_summaries_from_projects(vec![project_summary(
            "project-stale",
            "repo-stale",
            &stale_root,
        )])
        .expect_err("all-stale project list should remain an error");

        assert_eq!(error.code, "project_registry_mismatch");
    }

    #[test]
    fn manual_control_input_payload_maps_to_desktop_control_request() {
        let request = manual_control_input_request(&json!({
            "action": "mouse_click",
            "x": 42,
            "y": 64,
            "sourceWidth": 1280,
            "sourceHeight": 720,
            "button": "right",
            "clicks": 1,
        }))
        .expect("manual input request");

        assert_eq!(request.action, AutonomousDesktopControlAction::MouseClick);
        assert_eq!(request.x, Some(42));
        assert_eq!(request.y, Some(64));
        assert_eq!(request.source_width, Some(1280));
        assert_eq!(request.source_height, Some(720));
        assert_eq!(request.button, Some(AutonomousDesktopMouseButton::Right));
        assert_eq!(
            request.reason.as_deref(),
            Some("cloud_manual_control_input")
        );
    }

    #[test]
    fn manual_control_drag_payload_maps_to_desktop_control_request() {
        let request = manual_control_input_request(&json!({
            "action": "mouse_drag",
            "x": 42,
            "y": 64,
            "toX": 320,
            "toY": 240,
            "sourceWidth": 1280,
            "sourceHeight": 720,
            "button": "left",
        }))
        .expect("manual drag input request");

        assert_eq!(request.action, AutonomousDesktopControlAction::MouseDrag);
        assert_eq!(request.x, Some(42));
        assert_eq!(request.y, Some(64));
        assert_eq!(request.to_x, Some(320));
        assert_eq!(request.to_y, Some(240));
        assert_eq!(request.source_width, Some(1280));
        assert_eq!(request.source_height, Some(720));
        assert_eq!(request.button, Some(AutonomousDesktopMouseButton::Left));
        assert_eq!(
            request.reason.as_deref(),
            Some("cloud_manual_control_input")
        );
    }

    #[test]
    fn manual_control_stateful_drag_payloads_map_to_desktop_control_requests() {
        let down = manual_control_input_request(&json!({
            "action": "mouse_down",
            "x": 42,
            "y": 64,
            "sourceWidth": 1280,
            "sourceHeight": 720,
            "button": "left",
        }))
        .expect("manual mouse down input request");
        let drag_move = manual_control_input_request(&json!({
            "action": "mouse_drag_move",
            "x": 320,
            "y": 240,
            "sourceWidth": 1280,
            "sourceHeight": 720,
            "button": "left",
        }))
        .expect("manual mouse drag move input request");
        let up = manual_control_input_request(&json!({
            "action": "mouse_up",
            "x": 320,
            "y": 240,
            "sourceWidth": 1280,
            "sourceHeight": 720,
            "button": "left",
        }))
        .expect("manual mouse up input request");

        assert_eq!(down.action, AutonomousDesktopControlAction::MouseDown);
        assert_eq!(down.x, Some(42));
        assert_eq!(down.y, Some(64));
        assert_eq!(down.button, Some(AutonomousDesktopMouseButton::Left));
        assert_eq!(
            drag_move.action,
            AutonomousDesktopControlAction::MouseDragMove
        );
        assert_eq!(drag_move.x, Some(320));
        assert_eq!(drag_move.y, Some(240));
        assert_eq!(up.action, AutonomousDesktopControlAction::MouseUp);
        assert_eq!(up.x, Some(320));
        assert_eq!(up.y, Some(240));
    }

    #[test]
    fn manual_control_keyboard_payloads_map_to_desktop_control_requests() {
        let text_request = manual_control_input_request(&json!({
            "action": "type_text",
            "text": "hello",
        }))
        .expect("type text request");
        assert_eq!(
            text_request.action,
            AutonomousDesktopControlAction::TypeText
        );
        assert_eq!(text_request.text.as_deref(), Some("hello"));

        let key_request = manual_control_input_request(&json!({
            "action": "key_press",
            "key": "Enter",
        }))
        .expect("key press request");
        assert_eq!(key_request.action, AutonomousDesktopControlAction::KeyPress);
        assert_eq!(key_request.key.as_deref(), Some("Enter"));

        let hotkey_request = manual_control_input_request(&json!({
            "action": "hotkey",
            "keys": ["command", "a"],
        }))
        .expect("hotkey request");
        assert_eq!(
            hotkey_request.action,
            AutonomousDesktopControlAction::Hotkey
        );
        assert_eq!(
            hotkey_request.keys,
            vec!["command".to_string(), "a".to_string()]
        );

        let paste_request = manual_control_input_request(&json!({
            "action": "paste_text",
            "text": "pasted text",
        }))
        .expect("paste text request");
        assert_eq!(
            paste_request.action,
            AutonomousDesktopControlAction::PasteText
        );
        assert_eq!(paste_request.text.as_deref(), Some("pasted text"));
    }

    #[test]
    fn manual_control_rejects_unknown_desktop_action() {
        let error = manual_control_input_request(&json!({
            "action": "shell_exec",
        }))
        .expect_err("unsupported desktop action must be rejected");

        assert_eq!(error.code, "remote_manual_control_action_invalid");
    }

    #[test]
    fn manual_control_rejects_non_manual_desktop_actions() {
        for action in [
            "launch_app",
            "window_close",
            "ax_press",
            "clipboard_write_files",
        ] {
            let error = manual_control_input_request(&json!({
                "action": action,
                "appName": "Calculator",
                "elementId": "element-1",
                "filePaths": ["/tmp/example.txt"],
            }))
            .expect_err("non-manual action must be rejected");

            assert_eq!(error.code, "remote_manual_control_action_denied");
        }
    }

    #[test]
    fn manual_control_allows_plain_clipboard_text_write() {
        let request = manual_control_input_request(&json!({
            "action": "clipboard_write_text",
            "text": "hello",
        }))
        .expect("clipboard text write request");

        assert_eq!(
            request.action,
            AutonomousDesktopControlAction::ClipboardWriteText
        );
        assert_eq!(request.text.as_deref(), Some("hello"));
    }

    #[test]
    fn stream_fallback_encoder_downscales_png_to_jpeg() {
        let png = sample_png(320, 160);
        let screenshot = fallback_screenshot(320, 160);

        let frame =
            encode_stream_fallback_frame(&png, &screenshot, 160).expect("encoded fallback frame");

        assert_eq!(frame.media_type, "image/jpeg");
        assert_eq!(frame.width, 160);
        assert_eq!(frame.height, 80);
        assert_eq!(frame.scale_factor, 1.0);
        assert_eq!(
            image::guess_format(&frame.bytes).expect("encoded image format"),
            image::ImageFormat::Jpeg
        );
    }

    #[test]
    fn stream_fallback_encoder_does_not_upscale_frames() {
        let png = sample_png(100, 50);
        let screenshot = fallback_screenshot(100, 50);

        let frame =
            encode_stream_fallback_frame(&png, &screenshot, 640).expect("encoded fallback frame");

        assert_eq!((frame.width, frame.height), (100, 50));
        assert_eq!(frame.scale_factor, 2.0);
    }

    #[test]
    fn stream_fallback_encoder_rejects_invalid_image_bytes() {
        let error = encode_stream_fallback_frame(&[1, 2, 3], &fallback_screenshot(10, 10), 10)
            .expect_err("invalid frame bytes should fail");

        assert_eq!(error.code, "stream_fallback_frame_decode_failed");
    }

    #[test]
    fn remote_desktop_payload_forwarding_strips_stream_tokens() {
        let payload = remote_desktop_payload_for_forward(json!({
            "streamId": "stream-1",
            "streamToken": "secret-token",
            "stream_token": "legacy-secret-token",
            "quality": "balanced",
        }));

        assert_eq!(
            payload,
            json!({
                "streamId": "stream-1",
                "quality": "balanced",
            })
        );
    }

    #[test]
    fn remote_stream_signal_forwarding_strips_echoed_sdp_and_candidates() {
        let payload = remote_stream_payload_for_forward(
            &InboundCommandKind::ComputerUseStreamIceCandidate,
            json!({
                "streamId": "stream-1",
                "streamToken": "secret-token",
                "candidate": {
                    "candidate": "candidate:1",
                    "sdpMid": "0",
                    "sdpMLineIndex": 0
                },
                "quality": "balanced",
            }),
        );

        assert_eq!(
            payload,
            json!({
                "streamId": "stream-1",
                "quality": "balanced",
            })
        );
    }

    #[test]
    fn native_stream_offer_signal_forwards_offer_schema_and_payload() {
        let output = desktop_output_with_stream_signal(json!({
            "sessionDescription": {
                "type": "offer",
                "sdp": "v=0\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n"
            }
        }));

        assert_eq!(
            stream_signal_schema_for_output(&output),
            Some("xero.computer_use_stream_offer.v1")
        );
        assert_eq!(
            stream_signal_payload_for_output(&output),
            Some(json!({
                "type": "offer",
                "sdp": "v=0\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n",
            }))
        );
    }

    #[test]
    fn native_stream_ice_signal_forwards_candidate_schema_and_payload() {
        let output = desktop_output_with_stream_signal(json!({
            "iceCandidate": {
                "candidate": "candidate:1 1 udp 1 127.0.0.1 9 typ host",
                "sdpMid": "0",
                "sdpMLineIndex": 0,
                "usernameFragment": "ufrag"
            }
        }));

        assert_eq!(
            stream_signal_schema_for_output(&output),
            Some("xero.computer_use_stream_ice_candidate.v1")
        );
        assert_eq!(
            stream_signal_payload_for_output(&output),
            Some(json!({
                "candidate": {
                    "candidate": "candidate:1 1 udp 1 127.0.0.1 9 typ host",
                    "sdpMid": "0",
                    "sdpMLineIndex": 0,
                    "usernameFragment": "ufrag"
                }
            }))
        );
    }

    #[test]
    fn stream_signaling_payload_maps_to_typed_desktop_request_fields() {
        let payload = json!({
            "iceServers": [
                {
                    "urls": "turn:turn.example.test:3478",
                    "username": "user",
                    "credential": "pass",
                    "credential_type": "password"
                }
            ],
            "type": "answer",
            "sdp": "v=0",
            "candidate": {
                "candidate": "candidate:1",
                "sdp_mid": "0",
                "sdp_m_line_index": 0,
                "username_fragment": "ufrag"
            }
        });

        let ice_servers = desktop_stream_ice_servers_from_payload(&payload).expect("ice servers");
        let description = desktop_stream_session_description_from_payload(&payload)
            .expect("session description")
            .expect("session description");
        let candidate = desktop_stream_ice_candidate_from_payload(&payload)
            .expect("ice candidate")
            .expect("ice candidate");

        assert_eq!(ice_servers.len(), 1);
        assert_eq!(description.sdp_type, "answer");
        assert_eq!(description.sdp, "v=0");
        assert_eq!(candidate.candidate, "candidate:1");
        assert_eq!(candidate.sdp_mid.as_deref(), Some("0"));
    }

    #[test]
    fn stream_quality_commands_use_stable_contract_schemas() {
        assert_eq!(
            computer_use_stream_schema(&InboundCommandKind::ComputerUseStreamSetQuality),
            "xero.computer_use_stream_set_quality.v1"
        );
        assert_eq!(
            computer_use_stream_schema(&InboundCommandKind::ComputerUseStreamRequestKeyframe),
            "xero.computer_use_stream_request_keyframe.v1"
        );
    }

    #[test]
    fn manual_control_heartbeat_uses_stable_contract_schema() {
        assert_eq!(
            computer_use_manual_control_schema(
                &InboundCommandKind::ComputerUseManualControlHeartbeat
            ),
            "xero.computer_use_manual_control_heartbeat.v1"
        );
    }

    #[test]
    fn command_outcome_payload_includes_envelope_and_desktop_context() {
        let command = inbound_command(
            InboundCommandKind::ComputerUseManualControlInput,
            json!({
                "manualControlId": "manual-1",
                "streamId": "stream-1",
                "action": "mouse_click"
            }),
        );

        let payload = command_outcome_payload(&command, "duplicate", Some("duplicate_command"));

        assert_eq!(payload["schema"], COMMAND_OUTCOME_SCHEMA);
        assert_eq!(payload["clientCommandId"], "client-command-1");
        assert_eq!(payload["clientSeq"], 7);
        assert_eq!(payload["kind"], "computer_use_manual_control_input");
        assert_eq!(payload["outcome"], "duplicate");
        assert_eq!(payload["priority"], "critical_reliable");
        assert_eq!(payload["manualControlId"], "manual-1");
        assert_eq!(payload["streamId"], "stream-1");
        assert_eq!(payload["dedupeKey"], "dedupe-client-command-1");
    }

    #[test]
    fn command_dedupe_only_marks_repeated_critical_command_ids() {
        let unique = format!(
            "client-command-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        );
        let mut command = inbound_command(
            InboundCommandKind::ComputerUseManualControlInput,
            json!({"manualControlId": "manual-1", "action": "mouse_click"}),
        );
        command.client_command_id = Some(unique.clone());
        command.dedupe_key = Some(format!("dedupe-{unique}"));

        assert!(!command_is_duplicate(&command));
        assert!(command_is_duplicate(&command));

        command.kind = InboundCommandKind::ListSessions;
        assert!(!command_is_duplicate(&command));
    }

    #[test]
    fn computer_use_commands_require_fresh_expiry() {
        let mut command = inbound_command(
            InboundCommandKind::ComputerUseManualControlInput,
            json!({"manualControlId": "manual-1", "action": "mouse_click"}),
        );
        command.expires_at = Some(json!(current_unix_millis() + 1_000));

        assert!(command_freshness_error(&command).is_none());

        command.expires_at = Some(json!(current_unix_millis() - 1));
        assert_eq!(
            command_freshness_error(&command)
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("remote_command_expired")
        );

        command.expires_at = None;
        assert_eq!(
            command_freshness_error(&command)
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("remote_command_expiry_missing")
        );

        command.expires_at = Some(json!("not-millis"));
        assert_eq!(
            command_freshness_error(&command)
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("remote_command_expiry_invalid")
        );
    }

    #[test]
    fn non_desktop_commands_do_not_require_expiry() {
        let mut command = inbound_command(InboundCommandKind::ListSessions, json!({}));
        command.expires_at = None;

        assert!(command_freshness_error(&command).is_none());
    }

    #[test]
    fn known_web_device_check_ignores_revoked_and_desktop_devices() {
        let devices = vec![
            AccountDevice {
                id: "desktop-1".into(),
                account_id: "account-1".into(),
                kind: "desktop".into(),
                name: Some("Xero Desktop".into()),
                user_agent: None,
                last_seen: None,
                created_at: "2026-05-31T00:00:00Z".into(),
                revoked_at: None,
            },
            AccountDevice {
                id: "web-1".into(),
                account_id: "account-1".into(),
                kind: "web".into(),
                name: Some("Old Web".into()),
                user_agent: Some("browser".into()),
                last_seen: None,
                created_at: "2026-05-31T00:00:00Z".into(),
                revoked_at: Some("2026-05-31T00:05:00Z".into()),
            },
            AccountDevice {
                id: "web-2".into(),
                account_id: "account-1".into(),
                kind: "web".into(),
                name: Some("Xero Web".into()),
                user_agent: Some("browser".into()),
                last_seen: None,
                created_at: "2026-05-31T00:10:00Z".into(),
                revoked_at: None,
            },
        ];

        assert!(!account_devices_include_web_device(&devices, "desktop-1"));
        assert!(!account_devices_include_web_device(&devices, "web-1"));
        assert!(account_devices_include_web_device(&devices, "web-2"));
    }

    #[test]
    fn inbound_pointer_coalescing_keeps_latest_contiguous_move() {
        let (sender, receiver) = std::sync::mpsc::sync_channel(8);
        let first = inbound_command(
            InboundCommandKind::ComputerUseManualControlInput,
            json!({"manualControlId": "manual-1", "action": "mouse_move", "x": 10}),
        );
        sender
            .send(inbound_command(
                InboundCommandKind::ComputerUseManualControlInput,
                json!({"manualControlId": "manual-1", "action": "mouse_move", "x": 20}),
            ))
            .expect("second move");
        sender
            .send(inbound_command(
                InboundCommandKind::ComputerUseManualControlInput,
                json!({"manualControlId": "manual-1", "action": "type_text", "text": "hi"}),
            ))
            .expect("non-pointer input");
        let mut pending = VecDeque::new();

        let coalesced = coalesce_inbound_pointer_command(first, &receiver, &mut pending);

        assert_eq!(coalesced.payload["x"], 20);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].payload["action"], "type_text");
    }

    #[test]
    fn desktop_output_status_maps_to_command_outcomes() {
        let executed = desktop_output_with_status("executed");
        let starting = desktop_output_with_status("starting");
        let denied = desktop_output_with_status("denied");

        assert_eq!(
            command_outcome_for_desktop_output(Some(&executed)),
            "executed"
        );
        assert_eq!(
            command_outcome_for_desktop_output(Some(&starting)),
            "accepted"
        );
        assert_eq!(
            command_outcome_for_desktop_output(Some(&denied)),
            "rejected"
        );
    }

    fn desktop_output_with_stream_signal(stream_signal: JsonValue) -> AutonomousDesktopToolOutput {
        let mut output = desktop_output_with_status("starting");
        output.stream_signal = Some(serde_json::from_value(stream_signal).expect("stream signal"));
        output
    }

    fn desktop_output_with_status(status: &str) -> AutonomousDesktopToolOutput {
        serde_json::from_value(json!({
            "tool": "desktop_stream",
            "action": "stream_start",
            "requestId": "desktop_request_test",
            "phase": "phase_computer_use_desktop_control",
            "status": status,
            "platform": "test",
            "sidecar": {
                "schemaVersion": 1,
                "platform": "test",
                "transport": "sidecar",
                "authenticated": true,
                "health": "ready",
                "message": "ready"
            },
            "capabilities": {
                "platform": "test",
                "schemaVersion": 1,
                "displayList": true,
                "screenshot": true,
                "windowList": true,
                "appList": true,
                "foregroundState": true,
                "cursorState": true,
                "accessibilitySnapshot": false,
                "ocrSnapshot": false,
                "mouseInput": true,
                "keyboardInput": true,
                "clipboard": true,
                "accessibilityActions": false,
                "menuSelect": false,
                "webrtcStream": true,
                "screenshotFallbackStream": true,
                "manualCloudControl": true
            },
            "permissions": [],
            "policy": {
                "category": "stream_safe",
                "decision": "allowed",
                "decisionId": "policy_test",
                "code": "allowed",
                "reason": "test",
                "approvalRequired": false,
                "userActionRequired": false
            },
            "message": "ok"
        }))
        .expect("desktop output")
    }

    fn inbound_command(kind: InboundCommandKind, payload: JsonValue) -> InboundCommand {
        InboundCommand {
            v: 1,
            seq: 42,
            computer_id: "desktop-1".into(),
            session_id: Some("session-1".into()),
            kind,
            device_id: "web-1".into(),
            client_command_id: Some("client-command-1".into()),
            client_seq: Some(7),
            priority: Some(xero_remote_bridge::InboundCommandPriority::CriticalReliable),
            sent_at: Some(json!(1_700_000_000_000_i64)),
            dedupe_key: Some("dedupe-client-command-1".into()),
            expires_at: Some(json!(1_700_000_008_000_i64)),
            payload,
        }
    }
}
