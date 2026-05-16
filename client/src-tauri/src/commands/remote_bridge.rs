use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Manager, Runtime, State};
use tokio::sync::broadcast::error::TryRecvError as BroadcastTryRecvError;
use xero_remote_bridge::{
    AccountDevice, AuthStatus, BridgeAccount, BridgeConfig, BridgeError, BridgeResult,
    DesktopBridgeLoopOptions, FileIdentityStore, FileSessionVisibilityStore, IdentityStore,
    InboundCommand, InboundCommandKind, RemoteBridge,
};

use crate::{
    commands::{
        agent_run_dto,
        agent_session::agent_session_dto,
        resolve_operator_action::resolve_operator_action_blocking,
        runtime_support::{
            load_persisted_runtime_run, resolve_project_root, runtime_run_dto_from_snapshot,
        },
        start_runtime_run::start_runtime_run_blocking,
        stop_runtime_run::stop_runtime_run_blocking,
        update_runtime_run_controls::update_runtime_run_controls_blocking,
        validate_non_empty, AgentSessionDto, CommandError, CommandResult,
        ResolveOperatorActionRequestDto, RuntimeAgentIdDto, RuntimeRunApprovalModeDto,
        RuntimeRunControlInputDto, StartRuntimeRunRequestDto, StopRuntimeRunRequestDto,
        UpdateRuntimeRunControlsRequestDto,
    },
    db::project_store::{
        self, AgentEventRecord, AgentSessionCreateRecord, AgentSessionRecord,
        DEFAULT_AGENT_SESSION_TITLE,
    },
    registry::{read_registry, RegistryProjectRecord},
    state::DesktopState,
};

const REMOTE_DIR: &str = "remote";
const IDENTITY_FILE: &str = "desktop-identity.json";
const VISIBILITY_FILE: &str = "remote-visibility.json";

type AppRemoteBridge = RemoteBridge<FileIdentityStore, FileSessionVisibilityStore>;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetSessionRemoteVisibilityRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SetSessionRemoteVisibilityResponseDto {
    pub schema: String,
    pub session: AgentSessionDto,
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
pub fn set_session_remote_visibility<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    remote_state: State<'_, RemoteBridgeRuntimeState>,
    request: SetSessionRemoteVisibilityRequestDto,
) -> CommandResult<SetSessionRemoteVisibilityResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let session = project_store::set_agent_session_remote_visibility(
        &repo_root,
        &request.project_id,
        &request.agent_session_id,
        request.visible,
    )?;
    let bridge = remote_state.bridge(&app, state.inner())?;
    bridge
        .set_session_visibility(&request.agent_session_id, request.visible)
        .map_err(map_bridge_error)?;
    remote_state.start_if_registered(&app, state.inner())?;

    Ok(SetSessionRemoteVisibilityResponseDto {
        schema: "xero.session_remote_visibility.v1".into(),
        session: agent_session_dto(&session),
    })
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
    let payload = serde_json::from_str(&event.payload_json).unwrap_or_else(|_| {
        json!({
            "raw": event.payload_json,
        })
    });
    let event_kind = serde_json::to_value(&event.event_kind)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{:?}", event.event_kind));
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
    if let Err(error) = bridge.forward(&run.agent_session_id, runtime_event) {
        eprintln!("[remote-bridge] runtime event forward skipped: {error}");
    }
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

        let mut inbound = bridge.subscribe_inbound();
        let app = app.clone();
        let state = state.clone();
        let handle = thread::spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                match inbound.try_recv() {
                    Ok(command) => {
                        if let Err(error) =
                            handle_inbound_command(&app, &state, Arc::clone(&bridge), command)
                        {
                            eprintln!("[remote-bridge] inbound command failed: {error}");
                        }
                    }
                    Err(BroadcastTryRecvError::Empty) => {
                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(BroadcastTryRecvError::Lagged(_)) => {}
                    Err(BroadcastTryRecvError::Closed) => break,
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
    let result = route_inbound_command(app, state, Arc::clone(&bridge), command);
    if let Err(error) = &result {
        let _ = bridge.forward_control_event(
            &response_session,
            json!({
                "schema": "xero.remote_command_result.v1",
                "ok": false,
                "error": error,
            }),
        );
    }
    result.map(|_| ())
}

fn route_inbound_command<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: Arc<AppRemoteBridge>,
    command: InboundCommand,
) -> CommandResult<()> {
    ensure_known_web_device(&bridge, &command.device_id)?;
    match command.kind.clone() {
        InboundCommandKind::ListSessions => route_list_sessions(app, state, &bridge),
        InboundCommandKind::SessionAttached => route_session_attached(app, state, &bridge, command),
        InboundCommandKind::StartSession => route_start_session(app, state, &bridge, command),
        InboundCommandKind::SendMessage => route_send_message(app, state, &bridge, command),
        InboundCommandKind::ResolveOperatorAction => {
            route_resolve_operator_action(app, state, &bridge, command)
        }
        InboundCommandKind::CancelRun => route_cancel_run(app, state, &bridge, command),
    }
}

fn ensure_known_web_device(bridge: &AppRemoteBridge, device_id: &str) -> CommandResult<()> {
    validate_non_empty(device_id, "deviceId")?;
    let devices = bridge.list_account_devices().map_err(map_bridge_error)?;
    if devices
        .iter()
        .any(|device| device.kind == "web" && device.revoked_at.is_none() && device.id == device_id)
    {
        return Ok(());
    }

    Err(CommandError::policy_denied(
        "Remote command rejected because the web device is not linked or has been revoked.",
    ))
}

fn route_list_sessions<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
) -> CommandResult<()> {
    let sessions = visible_remote_sessions(app, state)?;
    bridge
        .forward_control_event(
            "__sessions__",
            json!({
                "schema": "xero.remote_visible_sessions.v1",
                "sessions": sessions,
            }),
        )
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
        "__new__" => return Ok(()),
        _ => {}
    }

    let located = locate_visible_remote_session(app, state, session_id)?;
    let last_seq = payload_u64(&command.payload, &["lastSeq", "last_seq"]).unwrap_or(0);
    if last_seq > 0
        && bridge
            .queue_replay_after(session_id, last_seq)
            .map_err(map_bridge_error)?
            > 0
    {
        return Ok(());
    }

    let snapshot = remote_session_snapshot(&located)?;
    bridge
        .snapshot(session_id, snapshot)
        .map_err(map_bridge_error)?;
    Ok(())
}

fn route_start_session<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let prompt = required_payload_string(&command.payload, &["prompt", "message"])?;
    let located_project = locate_project_for_remote_start(app, state, &command.payload)?;
    let title = payload_string(&command.payload, &["title"])
        .unwrap_or(DEFAULT_AGENT_SESSION_TITLE)
        .to_string();
    let session = project_store::create_agent_session(
        &located_project.repo_root,
        &AgentSessionCreateRecord {
            project_id: located_project.project_id.clone(),
            title,
            summary: String::new(),
            selected: true,
        },
    )?;
    let session = project_store::set_agent_session_remote_visibility(
        &located_project.repo_root,
        &located_project.project_id,
        &session.agent_session_id,
        true,
    )?;
    bridge
        .set_session_visibility(&session.agent_session_id, true)
        .map_err(map_bridge_error)?;

    let controls = remote_run_controls_from_payload(&command.payload)?;
    let run = start_runtime_run_blocking(
        app.clone(),
        state.clone(),
        StartRuntimeRunRequestDto {
            project_id: located_project.project_id.clone(),
            agent_session_id: session.agent_session_id.clone(),
            initial_controls: controls,
            initial_prompt: Some(prompt.to_string()),
            initial_attachments: Vec::new(),
        },
    )?;

    let session_payload = json!({
        "projectId": located_project.project_id,
        "session": agent_session_dto(&session),
        "run": run,
    });
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

fn route_send_message<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?;
    let message = required_payload_string(&command.payload, &["message", "prompt"])?;
    let located = locate_visible_remote_session(app, state, session_id)?;
    let existing = load_persisted_runtime_run(&located.repo_root, &located.project_id, session_id)?;
    let run = match existing {
        Some(snapshot) => update_runtime_run_controls_blocking(
            app.clone(),
            state.clone(),
            UpdateRuntimeRunControlsRequestDto {
                project_id: located.project_id.clone(),
                agent_session_id: session_id.to_string(),
                run_id: snapshot.run.run_id,
                controls: remote_run_controls_from_payload(&command.payload)?,
                prompt: Some(message.to_string()),
                attachments: Vec::new(),
                auto_compact: None,
            },
        )?,
        None => start_runtime_run_blocking(
            app.clone(),
            state.clone(),
            StartRuntimeRunRequestDto {
                project_id: located.project_id.clone(),
                agent_session_id: session_id.to_string(),
                initial_controls: remote_run_controls_from_payload(&command.payload)?,
                initial_prompt: Some(message.to_string()),
                initial_attachments: Vec::new(),
            },
        )?,
    };

    send_command_ok(
        bridge,
        session_id,
        "xero.remote_message_accepted.v1",
        json!({ "run": run }),
    )
}

fn route_resolve_operator_action<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    bridge: &AppRemoteBridge,
    command: InboundCommand,
) -> CommandResult<()> {
    let session_id = required_command_session(&command)?;
    let located = locate_visible_remote_session(app, state, session_id)?;
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
    let located = locate_visible_remote_session(app, state, session_id)?;
    let run_id = match payload_string(&command.payload, &["runId", "run_id"]) {
        Some(run_id) => run_id.to_string(),
        None => {
            let snapshot =
                load_persisted_runtime_run(&located.repo_root, &located.project_id, session_id)?
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
            agent_session_id: session_id.to_string(),
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

#[derive(Debug, Clone)]
struct LocatedRemoteProject {
    project_id: String,
    repo_root: std::path::PathBuf,
}

#[derive(Debug, Clone)]
struct LocatedRemoteSession {
    project_id: String,
    repo_root: std::path::PathBuf,
    session: AgentSessionRecord,
}

fn locate_project_for_remote_start<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    payload: &JsonValue,
) -> CommandResult<LocatedRemoteProject> {
    if let Some(project_id) = payload_string(payload, &["projectId", "project_id"]) {
        let repo_root = resolve_project_root(app, state, project_id)?;
        return Ok(LocatedRemoteProject {
            project_id: project_id.to_string(),
            repo_root,
        });
    }

    let registry = read_registry(&state.global_db_path(app)?)?;
    match registry.projects.as_slice() {
        [project] => Ok(project_location(project)),
        [] => Err(CommandError::project_not_found()),
        _ => Err(CommandError::user_fixable(
            "remote_project_required",
            "Remote start requires `projectId` because more than one desktop project is registered.",
        )),
    }
}

fn locate_visible_remote_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    session_id: &str,
) -> CommandResult<LocatedRemoteSession> {
    validate_non_empty(session_id, "agentSessionId")?;
    let registry = read_registry(&state.global_db_path(app)?)?;
    for project in registry.projects {
        let location = project_location(&project);
        if let Some(session) =
            project_store::get_agent_session(&location.repo_root, &location.project_id, session_id)?
        {
            if !session.remote_visible {
                return Err(CommandError::policy_denied(
                    "Remote command rejected because this session is not remotely visible.",
                ));
            }
            return Ok(LocatedRemoteSession {
                project_id: location.project_id,
                repo_root: location.repo_root,
                session,
            });
        }
    }

    Err(CommandError::user_fixable(
        "remote_session_not_found",
        format!("Xero could not find remotely visible session `{session_id}`."),
    ))
}

fn visible_remote_sessions<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<Vec<JsonValue>> {
    let registry = read_registry(&state.global_db_path(app)?)?;
    let mut sessions = Vec::new();
    for project in registry.projects {
        let location = project_location(&project);
        for session in
            project_store::list_agent_sessions(&location.repo_root, &location.project_id, false)?
        {
            if session.remote_visible {
                sessions.push(json!({
                    "projectId": location.project_id,
                    "session": agent_session_dto(&session),
                }));
            }
        }
    }
    Ok(sessions)
}

fn remote_session_snapshot(located: &LocatedRemoteSession) -> CommandResult<JsonValue> {
    let runs = project_store::load_agent_session_run_snapshots(
        &located.repo_root,
        &located.project_id,
        &located.session.agent_session_id,
    )?
    .into_iter()
    .map(|(snapshot, _usage)| agent_run_dto(snapshot))
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
        "session": agent_session_dto(&located.session),
        "runtimeRun": runtime_run,
        "runs": runs,
        "availableAgents": remote_available_agents(),
        "availableModels": remote_available_models(),
    }))
}

/// Static list of runtime agents the cloud composer can dispatch.
/// Mirrors `parse_runtime_agent_id` (any change there must be reflected here).
fn remote_available_agents() -> Vec<JsonValue> {
    vec![
        json!({ "id": "ask", "label": "Ask" }),
        json!({ "id": "plan", "label": "Plan" }),
        json!({ "id": "engineer", "label": "Engineer" }),
        json!({ "id": "debug", "label": "Debug" }),
        json!({ "id": "crawl", "label": "Crawl" }),
        json!({ "id": "agent_create", "label": "Agent Create" }),
        json!({ "id": "generalist", "label": "Generalist" }),
    ]
}

/// Returns the models the cloud composer surfaces in its dropdown.
/// Placeholder list — a richer per-project loader will populate this from the
/// active provider profile's catalog in a follow-up.
fn remote_available_models() -> Vec<JsonValue> {
    Vec::new()
}

fn project_location(project: &RegistryProjectRecord) -> LocatedRemoteProject {
    LocatedRemoteProject {
        project_id: project.project_id.clone(),
        repo_root: std::path::PathBuf::from(&project.root_path),
    }
}

fn remote_run_controls_from_payload(
    payload: &JsonValue,
) -> CommandResult<Option<RuntimeRunControlInputDto>> {
    let Some(agent) = payload_string(payload, &["agent", "runtimeAgentId", "runtime_agent_id"])
    else {
        return Ok(None);
    };
    let runtime_agent_id = parse_runtime_agent_id(agent)?;
    Ok(Some(RuntimeRunControlInputDto {
        runtime_agent_id,
        agent_definition_id: Some(agent.trim().to_string()),
        provider_profile_id: payload_string(payload, &["providerProfileId", "provider_profile_id"])
            .map(ToOwned::to_owned),
        model_id: payload_string(payload, &["modelId", "model_id"])
            .unwrap_or("")
            .to_string(),
        thinking_effort: None,
        approval_mode: RuntimeRunApprovalModeDto::Suggest,
        plan_mode_required: payload_bool(payload, &["planModeRequired", "plan_mode_required"])
            .unwrap_or(false),
    }))
}

fn parse_runtime_agent_id(value: &str) -> CommandResult<RuntimeAgentIdDto> {
    match value.trim() {
        "ask" => Ok(RuntimeAgentIdDto::Ask),
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

fn payload_u64(payload: &JsonValue, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| payload.get(*key).and_then(JsonValue::as_u64))
}

fn payload_bool(payload: &JsonValue, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| payload.get(*key).and_then(JsonValue::as_bool))
}

fn new_bridge_for_app<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<AppRemoteBridge> {
    let remote_dir = state.app_data_dir(app)?.join(REMOTE_DIR);

    Ok(RemoteBridge::new(
        BridgeConfig::from_env_or_local("Xero Desktop"),
        FileIdentityStore::new(remote_dir.join(IDENTITY_FILE)),
        FileSessionVisibilityStore::new(remote_dir.join(VISIBILITY_FILE)),
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
        | BridgeError::LockPoisoned => {
            CommandError::system_fault("remote_bridge_failed", error.to_string())
        }
    }
}
