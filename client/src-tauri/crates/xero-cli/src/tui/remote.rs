//! Cloud relay support for the standalone TUI.
//!
//! The desktop app already owns the canonical remote bridge. The TUI uses the
//! same relay contracts so cloud can list, attach to, and send messages to TUI
//! sessions without a separate cloud-side code path.

use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, TryRecvError},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::Duration,
};

use serde_json::{json, Value as JsonValue};
use xero_remote_bridge::{
    BridgeError, BridgeResult, DesktopBridgeLoopOptions, FileIdentityStore, IdentityStore,
    InboundCommand, InboundCommandKind, RemoteBridge,
};

use crate::{
    generate_id,
    project_cli::{
        ensure_global_computer_use_project, GLOBAL_COMPUTER_USE_AGENT_SESSION_ID,
        GLOBAL_COMPUTER_USE_PROJECT_ID, GLOBAL_COMPUTER_USE_PROJECT_NAME,
    },
    CliError, GlobalOptions,
};

use super::app::invoke_json;

type TuiRemoteBridge = RemoteBridge<FileIdentityStore>;
const REMOTE_RUN_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(150);
const REMOTE_RUN_FINAL_LOAD_RETRY_LIMIT: u8 = 20;
const REMOTE_COMPUTER_USE_SESSION_ID: &str = "__computer_use__";

#[derive(Default)]
struct TuiRemoteBridgeState {
    bridge: Mutex<Option<Arc<TuiRemoteBridge>>>,
    shutdown: Mutex<Option<Arc<AtomicBool>>>,
    worker: Mutex<Option<thread::JoinHandle<BridgeResult<()>>>>,
    inbound_worker: Mutex<Option<thread::JoinHandle<()>>>,
    ui_events: Mutex<Option<mpsc::Sender<RemoteUiEvent>>>,
}

#[derive(Debug, Clone)]
struct LocatedRemoteProject {
    project_id: String,
    project_name: Option<String>,
}

#[derive(Debug, Clone)]
struct LocatedRemoteSession {
    project_id: String,
    project_name: Option<String>,
    session: JsonValue,
    remote_session_id: String,
}

#[derive(Debug, Clone)]
struct ProviderEntry {
    provider_id: String,
    label: String,
    default_model: String,
    catalog_kind: String,
    profiles: Vec<String>,
    models: Vec<ProviderModelEntry>,
}

#[derive(Debug, Clone)]
struct ProviderModelEntry {
    model_id: String,
    display_name: String,
    thinking_supported: bool,
    thinking_effort_options: Vec<String>,
    default_thinking_effort: Option<String>,
}

#[derive(Debug, Clone)]
struct ProviderChoice {
    provider_id: String,
    provider_profile_id: Option<String>,
    model_id: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct TuiRemotePreferences {
    runtime_agent_id: Option<String>,
    thinking_effort: Option<String>,
    provider_id: Option<String>,
    model_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct RemoteControlsUpdate {
    pub session_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub runtime_agent_id: String,
    pub thinking_effort: String,
}

#[derive(Debug, Clone)]
pub(crate) enum RemoteUiEvent {
    ControlsUpdated(RemoteControlsUpdate),
}

#[derive(Debug, Clone)]
struct RemoteRunSpec {
    project_id: String,
    project_name: Option<String>,
    session: JsonValue,
    session_id: String,
    remote_session_id: String,
    run_id: String,
    choice: ProviderChoice,
    runtime_agent_id: String,
    thinking_effort: String,
    prompt: String,
    attachments: Vec<JsonValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StreamEventsOutcome {
    last_event_id: i64,
    terminal_event_seen: bool,
}

pub(crate) fn ensure_started(globals: &GlobalOptions) -> Result<(), CliError> {
    remote_state().start_if_registered(globals).map(|_| ())
}

pub(crate) fn shutdown() {
    remote_state().shutdown();
}

pub(crate) fn subscribe_ui_events() -> mpsc::Receiver<RemoteUiEvent> {
    remote_state().subscribe_ui_events()
}

pub(crate) fn publish_session_added(
    globals: &GlobalOptions,
    project_id: &str,
    session_id: &str,
) -> Result<(), CliError> {
    let Some(bridge) = remote_state().start_if_registered(globals)? else {
        return Ok(());
    };
    let located = locate_remote_session_in_project(globals, project_id, session_id, true)?;
    if located.remote_session_id == REMOTE_COMPUTER_USE_SESSION_ID {
        return Ok(());
    }
    publish_session_added_to_bridge(&bridge, &located)
}

pub(crate) fn publish_session_snapshot_with_run(
    globals: &GlobalOptions,
    project_id: &str,
    session_id: &str,
    latest_run: Option<&JsonValue>,
) -> Result<(), CliError> {
    let Some(bridge) = remote_state().start_if_registered(globals)? else {
        return Ok(());
    };
    let located = locate_remote_session_in_project(globals, project_id, session_id, false)?;
    publish_session_snapshot_to_bridge(&bridge, globals, &located, latest_run)
}

pub(crate) fn publish_session_controls(
    globals: &GlobalOptions,
    project_id: &str,
    session_id: &str,
    provider_id: &str,
    model_id: &str,
    runtime_agent_id: &str,
    thinking_effort: &str,
) -> Result<(), CliError> {
    let Some(bridge) = remote_state().start_if_registered(globals)? else {
        return Ok(());
    };
    let payload = json!({
        "providerId": provider_id,
        "modelId": model_id,
        "agent": runtime_agent_id,
        "thinkingEffort": thinking_effort,
    });
    let choice = provider_choice_from_payload(globals, &payload)?;
    let located = locate_remote_session_in_project(globals, project_id, session_id, true)?;
    let run = remote_runtime_run_payload(
        project_id,
        session_id,
        "pending",
        &choice,
        runtime_agent_id,
        thinking_effort,
        "completed",
    );
    send_command_ok(
        &bridge,
        &located.remote_session_id,
        "xero.remote_session_controls_updated.v1",
        json!({ "run": run }),
    )?;
    publish_session_snapshot_to_bridge(&bridge, globals, &located, Some(&run))
}

impl TuiRemoteBridgeState {
    fn bridge(&self, globals: &GlobalOptions) -> Result<Arc<TuiRemoteBridge>, CliError> {
        let mut guard = self.bridge.lock().map_err(|_| {
            CliError::system_fault(
                "xero_tui_remote_bridge_lock_failed",
                "Xero TUI could not lock the remote bridge state.",
            )
        })?;
        if let Some(bridge) = guard.as_ref() {
            return Ok(Arc::clone(bridge));
        }

        let bridge = Arc::new(new_bridge_for_globals(globals));
        *guard = Some(Arc::clone(&bridge));
        Ok(bridge)
    }

    fn start_if_registered(
        &self,
        globals: &GlobalOptions,
    ) -> Result<Option<Arc<TuiRemoteBridge>>, CliError> {
        if !registered_identity_exists(globals)? {
            return Ok(None);
        }
        self.ensure_started(globals).map(Some)
    }

    fn ensure_started(&self, globals: &GlobalOptions) -> Result<Arc<TuiRemoteBridge>, CliError> {
        let bridge = self.bridge(globals)?;
        let mut worker = self.worker.lock().map_err(|_| {
            CliError::system_fault(
                "xero_tui_remote_worker_lock_failed",
                "Xero TUI could not lock the remote bridge worker state.",
            )
        })?;
        if worker.as_ref().is_some_and(|handle| !handle.is_finished()) {
            return Ok(bridge);
        }

        let shutdown = Arc::new(AtomicBool::new(false));
        let handle = Arc::clone(&bridge)
            .spawn_desktop_loop(Arc::clone(&shutdown), DesktopBridgeLoopOptions::default());
        self.ensure_inbound_worker(globals, Arc::clone(&bridge), Arc::clone(&shutdown))?;
        *self.shutdown.lock().map_err(|_| {
            CliError::system_fault(
                "xero_tui_remote_shutdown_lock_failed",
                "Xero TUI could not lock the remote bridge shutdown state.",
            )
        })? = Some(shutdown);
        *worker = Some(handle);
        Ok(bridge)
    }

    fn ensure_inbound_worker(
        &self,
        globals: &GlobalOptions,
        bridge: Arc<TuiRemoteBridge>,
        shutdown: Arc<AtomicBool>,
    ) -> Result<(), CliError> {
        let mut worker = self.inbound_worker.lock().map_err(|_| {
            CliError::system_fault(
                "xero_tui_remote_inbound_worker_lock_failed",
                "Xero TUI could not lock the remote bridge inbound worker state.",
            )
        })?;
        if worker.as_ref().is_some_and(|handle| !handle.is_finished()) {
            return Ok(());
        }

        let mut inbound = bridge.subscribe_inbound();
        let globals = globals.clone();
        let handle = thread::spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                match inbound.try_recv() {
                    Ok(command) => {
                        if let Err(error) =
                            handle_inbound_command(&globals, Arc::clone(&bridge), command)
                        {
                            eprintln!("[tui-remote] inbound command failed: {}", error.message);
                        }
                    }
                    Err(_) => thread::sleep(Duration::from_millis(100)),
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
        if let Ok(mut ui_events) = self.ui_events.lock() {
            ui_events.take();
        }
    }

    fn subscribe_ui_events(&self) -> mpsc::Receiver<RemoteUiEvent> {
        let (sender, receiver) = mpsc::channel();
        if let Ok(mut ui_events) = self.ui_events.lock() {
            *ui_events = Some(sender);
        }
        receiver
    }

    fn send_ui_event(&self, event: RemoteUiEvent) {
        if let Ok(ui_events) = self.ui_events.lock() {
            if let Some(sender) = ui_events.as_ref() {
                let _ = sender.send(event);
            }
        }
    }
}

fn remote_state() -> &'static TuiRemoteBridgeState {
    static STATE: OnceLock<TuiRemoteBridgeState> = OnceLock::new();
    STATE.get_or_init(TuiRemoteBridgeState::default)
}

fn new_bridge_for_globals(globals: &GlobalOptions) -> TuiRemoteBridge {
    RemoteBridge::new(
        xero_remote_bridge::BridgeConfig::from_env_or_local("Xero TUI"),
        FileIdentityStore::new(remote_identity_path(globals)),
    )
}

fn remote_identity_path(globals: &GlobalOptions) -> std::path::PathBuf {
    crate::cli_app_data_root(globals)
        .join("remote")
        .join("desktop-identity.json")
}

fn registered_identity_exists(globals: &GlobalOptions) -> Result<bool, CliError> {
    let store = FileIdentityStore::new(remote_identity_path(globals));
    let identity = store.load().map_err(map_bridge_error)?;
    Ok(identity
        .and_then(|identity| identity.desktop_jwt)
        .is_some_and(|jwt| !jwt.trim().is_empty()))
}

fn handle_inbound_command(
    globals: &GlobalOptions,
    bridge: Arc<TuiRemoteBridge>,
    command: InboundCommand,
) -> Result<(), CliError> {
    let response_session = command
        .session_id
        .as_deref()
        .unwrap_or("__sessions__")
        .to_owned();
    let result = route_inbound_command(globals, Arc::clone(&bridge), command);
    if let Err(error) = &result {
        let _ = bridge.forward_control_event(
            &response_session,
            json!({
                "schema": "xero.remote_command_result.v1",
                "ok": false,
                "error": cli_error_payload(error),
            }),
        );
    }
    result.map(|_| ())
}

fn route_inbound_command(
    globals: &GlobalOptions,
    bridge: Arc<TuiRemoteBridge>,
    command: InboundCommand,
) -> Result<(), CliError> {
    if matches!(command.kind, InboundCommandKind::AuthorizeSessionJoin) {
        return route_authorize_session_join(globals, &bridge, command);
    }

    ensure_known_web_device(&bridge, &command.device_id)?;
    match command.kind.clone() {
        InboundCommandKind::ListSessions => publish_remote_session_list(globals, &bridge),
        InboundCommandKind::ListProjects => publish_remote_project_list(globals, &bridge),
        InboundCommandKind::AuthorizeSessionJoin => unreachable!("handled before device gate"),
        InboundCommandKind::ArchiveSession => route_archive_session(globals, &bridge, command),
        InboundCommandKind::SessionAttached => route_session_attached(globals, &bridge, command),
        InboundCommandKind::StartSession => {
            route_start_session(globals, Arc::clone(&bridge), command)
        }
        InboundCommandKind::SendMessage => {
            route_send_message(globals, Arc::clone(&bridge), command)
        }
        InboundCommandKind::ContextSnapshot => route_context_snapshot(globals, &bridge, command),
        InboundCommandKind::UpdateSessionControls => {
            route_update_session_controls(globals, &bridge, command)
        }
        InboundCommandKind::StageAttachment => route_stage_attachment(globals, &bridge, command),
        InboundCommandKind::DiscardAttachment => {
            route_discard_attachment(globals, &bridge, command)
        }
        InboundCommandKind::ResolveOperatorAction => {
            route_resolve_operator_action(globals, &bridge, command)
        }
        InboundCommandKind::CancelRun => route_cancel_run(globals, &bridge, command),
        InboundCommandKind::FetchRuntimeMediaArtifact => Err(CliError::user_fixable(
            "remote_runtime_media_unavailable",
            "Runtime media artifact fetches are only supported by the desktop app.",
        )),
    }
}

fn ensure_known_web_device(bridge: &TuiRemoteBridge, device_id: &str) -> Result<(), CliError> {
    if device_id.trim().is_empty() {
        return Err(CliError::usage("Missing remote web device id."));
    }
    let devices = bridge.list_account_devices().map_err(map_bridge_error)?;
    if devices
        .iter()
        .any(|device| device.kind == "web" && device.revoked_at.is_none() && device.id == device_id)
    {
        return Ok(());
    }

    Err(CliError::user_fixable(
        "xero_tui_remote_device_denied",
        "Remote command rejected because the web device is not linked or has been revoked.",
    ))
}

fn route_authorize_session_join(
    globals: &GlobalOptions,
    bridge: &TuiRemoteBridge,
    command: InboundCommand,
) -> Result<(), CliError> {
    let session_id = required_command_session(&command)?.to_owned();
    let join_ref = required_payload_string(&command.payload, &["joinRef", "join_ref"])?;
    let auth_topic = required_payload_string(&command.payload, &["authTopic", "auth_topic"])?;
    let authorized = ensure_known_web_device(bridge, &command.device_id).is_ok()
        && locate_remote_session(globals, &session_id).is_ok();
    bridge
        .authorize_session_join(join_ref, auth_topic, &session_id, authorized)
        .map_err(map_bridge_error)
}

fn route_archive_session(
    globals: &GlobalOptions,
    bridge: &TuiRemoteBridge,
    command: InboundCommand,
) -> Result<(), CliError> {
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
    if session_id == REMOTE_COMPUTER_USE_SESSION_ID {
        return Err(CliError::usage(
            "Computer Use is global and cannot be archived from the project session list.",
        ));
    }
    invoke_json(
        globals,
        &["session", "archive", "--project-id", project_id, session_id],
    )?;
    publish_remote_session_list(globals, bridge)?;
    bridge
        .forward_session_removed(
            session_id,
            json!({
                "schema": "xero.remote_session_removed.v1",
                "projectId": project_id,
                "sessionId": session_id,
            }),
        )
        .map_err(map_bridge_error)?;
    Ok(())
}

fn route_session_attached(
    globals: &GlobalOptions,
    bridge: &TuiRemoteBridge,
    command: InboundCommand,
) -> Result<(), CliError> {
    let session_id = required_command_session(&command)?;
    match session_id {
        "__sessions__" => return publish_remote_session_list(globals, bridge),
        "__projects__" => return publish_remote_project_list(globals, bridge),
        "__new__" => return Ok(()),
        _ => {}
    }

    let located = locate_remote_session(globals, session_id)?;
    let last_seq = payload_u64(&command.payload, &["lastSeq", "last_seq"]).unwrap_or(0);
    if last_seq > 0
        && bridge
            .queue_replay_after(session_id, last_seq)
            .map_err(map_bridge_error)?
            > 0
    {
        return Ok(());
    }
    publish_session_snapshot_to_bridge(bridge, globals, &located, None)
}

fn route_start_session(
    globals: &GlobalOptions,
    bridge: Arc<TuiRemoteBridge>,
    command: InboundCommand,
) -> Result<(), CliError> {
    let session_kind = remote_session_kind_from_payload(&command.payload)?;
    ensure_remote_payload_matches_session_kind(&command.payload, session_kind)?;
    if session_kind == "computer_use" {
        let prompt =
            payload_string(&command.payload, &["prompt", "message"]).map(ToOwned::to_owned);
        return route_start_computer_use_session(globals, bridge, command, prompt.as_deref());
    }

    let project = locate_project_for_remote_start(globals, &command.payload)?;
    let session_id = generate_id("session");
    let title = payload_string(&command.payload, &["title"])
        .unwrap_or("New Chat")
        .to_owned();
    let created = invoke_json(
        globals,
        &[
            "session",
            "create",
            "--project-id",
            &project.project_id,
            "--session-id",
            &session_id,
            "--title",
            &title,
            "--session-kind",
            session_kind,
        ],
    )?;
    let session = created.get("session").cloned().unwrap_or(created);
    let located = LocatedRemoteSession {
        project_id: project.project_id.clone(),
        project_name: project.project_name.clone(),
        session,
        remote_session_id: session_id.clone(),
    };
    let session_payload = remote_session_result_payload(&located);

    bridge
        .forward_control_event(
            "__new__",
            json!({
                "schema": "xero.remote_session_started.v1",
                "result": session_payload.clone(),
            }),
        )
        .map_err(map_bridge_error)?;
    publish_session_added_to_bridge(&bridge, &located)?;

    let Some(prompt) = payload_string(&command.payload, &["prompt", "message"])
        .filter(|prompt| !prompt.trim().is_empty())
    else {
        return Ok(());
    };

    run_remote_prompt(globals, bridge, &located, &command.payload, prompt)
}

fn route_start_computer_use_session(
    globals: &GlobalOptions,
    bridge: Arc<TuiRemoteBridge>,
    command: InboundCommand,
    prompt: Option<&str>,
) -> Result<(), CliError> {
    let located = locate_global_computer_use_session(globals)?;
    let mut session_payload = remote_session_result_payload(&located);
    if let Some(payload) = session_payload.as_object_mut() {
        payload.insert("run".to_owned(), JsonValue::Null);
    }

    bridge
        .forward_control_event(
            REMOTE_COMPUTER_USE_SESSION_ID,
            json!({
                "schema": "xero.remote_session_started.v1",
                "result": session_payload.clone(),
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

    let Some(prompt) = prompt.filter(|prompt| !prompt.trim().is_empty()) else {
        return Ok(());
    };
    run_remote_prompt(globals, bridge, &located, &command.payload, prompt)
}

fn route_send_message(
    globals: &GlobalOptions,
    bridge: Arc<TuiRemoteBridge>,
    command: InboundCommand,
) -> Result<(), CliError> {
    let session_id = required_command_session(&command)?;
    let message = required_payload_string(&command.payload, &["message", "prompt"])?;
    let located = locate_remote_session(globals, session_id)?;
    ensure_remote_payload_matches_session_kind(
        &command.payload,
        remote_session_kind_value(&located.session),
    )?;
    run_remote_prompt(globals, bridge, &located, &command.payload, message)
}

fn run_remote_prompt(
    globals: &GlobalOptions,
    bridge: Arc<TuiRemoteBridge>,
    located: &LocatedRemoteSession,
    payload: &JsonValue,
    prompt: &str,
) -> Result<(), CliError> {
    let project_id = located.project_id.as_str();
    let session_id = required_session_id(&located.session)?.to_owned();
    let remote_session_id = located.remote_session_id.clone();
    let attachments = remote_attachments_from_payload(payload)?;
    let choice = provider_choice_from_payload(globals, payload)?;
    let session_kind = remote_session_kind_value(&located.session);
    ensure_remote_payload_matches_session_kind(payload, session_kind)?;
    let runtime_agent_id =
        payload_string(payload, &["agent", "runtimeAgentId", "runtime_agent_id"])
            .unwrap_or(if session_kind == "computer_use" {
                "computer_use"
            } else {
                "engineer"
            })
            .to_owned();
    let thinking_effort = payload_string(payload, &["thinkingEffort", "thinking_effort"])
        .unwrap_or("high")
        .to_owned();
    let run_id = generate_id("tui-run");
    let accepted_run = remote_runtime_run_payload(
        project_id,
        &session_id,
        &run_id,
        &choice,
        &runtime_agent_id,
        &thinking_effort,
        "running",
    );
    send_command_ok(
        &bridge,
        &remote_session_id,
        "xero.remote_message_accepted.v1",
        json!({ "run": accepted_run }),
    )?;

    let spec = RemoteRunSpec {
        project_id: project_id.to_owned(),
        project_name: located.project_name.clone(),
        session: located.session.clone(),
        session_id,
        remote_session_id,
        run_id,
        choice,
        runtime_agent_id,
        thinking_effort,
        prompt: prompt.to_owned(),
        attachments,
    };
    let globals = globals.clone();
    thread::spawn(move || drive_remote_prompt_run(globals, bridge, spec));
    Ok(())
}

fn invoke_agent_prompt(
    globals: &GlobalOptions,
    spec: &RemoteRunSpec,
) -> Result<JsonValue, CliError> {
    let mut owned = vec![
        "agent".to_owned(),
        "exec".to_owned(),
        "--project-id".to_owned(),
        spec.project_id.clone(),
        "--session-id".to_owned(),
        spec.session_id.clone(),
        "--run-id".to_owned(),
        spec.run_id.clone(),
        "--provider".to_owned(),
        spec.choice.provider_id.clone(),
        "--model".to_owned(),
        spec.choice.model_id.clone(),
        "--runtime-agent-id".to_owned(),
        spec.runtime_agent_id.clone(),
        "--agent-definition-id".to_owned(),
        spec.runtime_agent_id.clone(),
        "--thinking-effort".to_owned(),
        spec.thinking_effort.clone(),
        "--prompt".to_owned(),
        spec.prompt.clone(),
    ];
    if !spec.attachments.is_empty() {
        let attachments_json = serde_json::to_string(&spec.attachments).map_err(|error| {
            CliError::system_fault(
                "xero_tui_remote_attachments_encode_failed",
                format!("Could not encode remote attachments for agent exec: {error}"),
            )
        })?;
        owned.push("--attachments-json".to_owned());
        owned.push(attachments_json);
    }
    let borrowed = owned.iter().map(String::as_str).collect::<Vec<_>>();
    let value = invoke_json(globals, &borrowed)?;
    Ok(value.get("snapshot").cloned().unwrap_or(value))
}

fn drive_remote_prompt_run(
    globals: GlobalOptions,
    bridge: Arc<TuiRemoteBridge>,
    spec: RemoteRunSpec,
) {
    let (sender, receiver) = mpsc::channel();
    let exec_globals = globals.clone();
    let exec_spec = spec.clone();
    thread::spawn(move || {
        let _ = sender.send(invoke_agent_prompt(&exec_globals, &exec_spec));
    });

    let mut last_event_id = 0_i64;
    let mut latest_snapshot = None;
    let mut exec_finished = false;
    let mut load_failures_after_exec = 0_u8;

    loop {
        match load_run_snapshot(&globals, &spec.project_id, &spec.run_id) {
            Ok(snapshot) => {
                load_failures_after_exec = 0;
                match stream_snapshot_events(&bridge, &spec, &snapshot, last_event_id) {
                    Ok(outcome) => {
                        last_event_id = outcome.last_event_id;
                        if outcome.terminal_event_seen || snapshot_status_is_quiescent(&snapshot) {
                            latest_snapshot = Some(snapshot);
                            break;
                        }
                    }
                    Err(error) => {
                        eprintln!(
                            "[tui-remote] runtime event forwarding failed for {}: {}",
                            spec.run_id, error.message
                        );
                    }
                }
                latest_snapshot = Some(snapshot);
            }
            Err(error) if exec_finished => {
                eprintln!(
                    "[tui-remote] final run snapshot load failed for {}: {}",
                    spec.run_id, error.message
                );
                load_failures_after_exec = load_failures_after_exec.saturating_add(1);
                if load_failures_after_exec >= REMOTE_RUN_FINAL_LOAD_RETRY_LIMIT {
                    break;
                }
            }
            Err(_) => {}
        }

        match receiver.try_recv() {
            Ok(Ok(snapshot)) => {
                exec_finished = true;
                match stream_snapshot_events(&bridge, &spec, &snapshot, last_event_id) {
                    Ok(outcome) => {
                        last_event_id = outcome.last_event_id;
                        if outcome.terminal_event_seen || snapshot_status_is_quiescent(&snapshot) {
                            latest_snapshot = Some(snapshot);
                            break;
                        }
                    }
                    Err(error) => {
                        eprintln!(
                            "[tui-remote] runtime event forwarding failed for {}: {}",
                            spec.run_id, error.message
                        );
                    }
                }
                latest_snapshot = Some(snapshot);
            }
            Ok(Err(error)) => {
                let _ = forward_remote_run_failed_event(
                    &bridge,
                    &spec,
                    last_event_id.saturating_add(1).max(1),
                    &error,
                );
                let _ = bridge.forward_control_event(
                    &spec.remote_session_id,
                    json!({
                        "schema": "xero.remote_command_result.v1",
                        "ok": false,
                        "error": cli_error_payload(&error),
                    }),
                );
                publish_remote_run_snapshot_best_effort(
                    &globals,
                    &bridge,
                    &spec,
                    latest_snapshot.as_ref(),
                );
                return;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                let error = CliError::system_fault(
                    "xero_tui_remote_agent_worker_disconnected",
                    "The TUI remote agent worker stopped before returning a result.",
                );
                let _ = forward_remote_run_failed_event(
                    &bridge,
                    &spec,
                    last_event_id.saturating_add(1).max(1),
                    &error,
                );
                publish_remote_run_snapshot_best_effort(
                    &globals,
                    &bridge,
                    &spec,
                    latest_snapshot.as_ref(),
                );
                return;
            }
        }

        thread::sleep(REMOTE_RUN_EVENT_POLL_INTERVAL);
    }

    let final_snapshot = load_run_snapshot(&globals, &spec.project_id, &spec.run_id)
        .ok()
        .or(latest_snapshot);
    if let Some(snapshot) = final_snapshot.as_ref() {
        let _ = stream_snapshot_events(&bridge, &spec, snapshot, last_event_id);
    }
    publish_remote_run_snapshot_best_effort(&globals, &bridge, &spec, final_snapshot.as_ref());
}

fn load_run_snapshot(
    globals: &GlobalOptions,
    project_id: &str,
    run_id: &str,
) -> Result<JsonValue, CliError> {
    let value = invoke_json(
        globals,
        &["conversation", "show", "--project-id", project_id, run_id],
    )?;
    Ok(value.get("snapshot").cloned().unwrap_or(value))
}

fn stream_snapshot_events(
    bridge: &TuiRemoteBridge,
    spec: &RemoteRunSpec,
    snapshot: &JsonValue,
    after_event_id: i64,
) -> Result<StreamEventsOutcome, CliError> {
    let mut last_event_id = after_event_id;
    let mut terminal_event_seen = false;
    for event in snapshot
        .get("events")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
    {
        let Some(event_id) = remote_event_id(event) else {
            continue;
        };
        if event_id <= last_event_id {
            continue;
        }
        let Some(runtime_event) = remote_runtime_event_payload(spec, event, event_id) else {
            continue;
        };
        bridge
            .forward(&spec.remote_session_id, runtime_event)
            .map_err(map_bridge_error)?;
        last_event_id = event_id;
        if remote_event_kind(event).is_some_and(|kind| remote_event_kind_is_terminal(&kind)) {
            terminal_event_seen = true;
        }
    }
    Ok(StreamEventsOutcome {
        last_event_id,
        terminal_event_seen,
    })
}

fn remote_runtime_event_payload(
    spec: &RemoteRunSpec,
    event: &JsonValue,
    event_id: i64,
) -> Option<JsonValue> {
    let event_kind = remote_event_kind(event)?;
    Some(json!({
        "schema": "xero.remote_runtime_event.v1",
        "projectId": string_field(event, "projectId").unwrap_or_else(|| spec.project_id.clone()),
        "agentSessionId": &spec.session_id,
        "runId": string_field(event, "runId").unwrap_or_else(|| spec.run_id.clone()),
        "eventId": event_id,
        "eventKind": event_kind,
        "payload": event.get("payload").cloned().unwrap_or(JsonValue::Null),
        "createdAt": string_field(event, "createdAt"),
    }))
}

fn forward_remote_run_failed_event(
    bridge: &TuiRemoteBridge,
    spec: &RemoteRunSpec,
    event_id: i64,
    error: &CliError,
) -> Result<(), CliError> {
    bridge
        .forward(
            &spec.remote_session_id,
            json!({
                "schema": "xero.remote_runtime_event.v1",
                "projectId": &spec.project_id,
                "agentSessionId": &spec.session_id,
                "runId": &spec.run_id,
                "eventId": event_id,
                "eventKind": "run_failed",
                "payload": cli_error_payload(error),
            }),
        )
        .map_err(map_bridge_error)?;
    Ok(())
}

fn publish_remote_run_snapshot_best_effort(
    globals: &GlobalOptions,
    bridge: &TuiRemoteBridge,
    spec: &RemoteRunSpec,
    latest_snapshot: Option<&JsonValue>,
) {
    auto_name_remote_session_best_effort(globals, &spec.project_id, &spec.session_id);
    let mut located =
        locate_remote_session_in_project(globals, &spec.project_id, &spec.session_id, true)
            .unwrap_or_else(|_| LocatedRemoteSession {
                project_id: spec.project_id.clone(),
                project_name: spec.project_name.clone(),
                session: spec.session.clone(),
                remote_session_id: spec.remote_session_id.clone(),
            });
    located.remote_session_id = spec.remote_session_id.clone();
    if located.remote_session_id != REMOTE_COMPUTER_USE_SESSION_ID {
        let _ = publish_session_added_to_bridge(bridge, &located);
    }
    let _ = publish_session_snapshot_to_bridge(bridge, globals, &located, latest_snapshot);
}

fn auto_name_remote_session_best_effort(
    globals: &GlobalOptions,
    project_id: &str,
    session_id: &str,
) {
    let _ = invoke_json(
        globals,
        &[
            "session",
            "auto-name",
            "--project-id",
            project_id,
            session_id,
        ],
    );
}

fn remote_event_id(event: &JsonValue) -> Option<i64> {
    event
        .get("id")
        .or_else(|| event.get("eventId"))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().map(|id| id as i64))
        })
}

fn remote_event_kind(event: &JsonValue) -> Option<String> {
    string_field(event, "eventKind").or_else(|| string_field(event, "event_kind"))
}

fn remote_event_kind_is_terminal(kind: &str) -> bool {
    matches!(kind, "run_completed" | "run_failed" | "run_paused")
}

fn snapshot_status_is_quiescent(snapshot: &JsonValue) -> bool {
    string_field(snapshot, "status").is_some_and(|status| {
        matches!(
            status.as_str(),
            "paused" | "cancelled" | "handed_off" | "completed" | "failed"
        )
    })
}

fn route_context_snapshot(
    globals: &GlobalOptions,
    bridge: &TuiRemoteBridge,
    command: InboundCommand,
) -> Result<(), CliError> {
    let session_id = required_command_session(&command)?;
    let located = locate_remote_session(globals, session_id)?;
    let local_session_id = required_session_id(&located.session)?.to_owned();
    let request_id =
        payload_string(&command.payload, &["requestId", "request_id"]).map(ToOwned::to_owned);
    let context_result = load_remote_context_snapshot(
        globals,
        &located.project_id,
        &local_session_id,
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
            "error": cli_error_payload(&error),
        }),
    };
    bridge
        .forward_control_event(&located.remote_session_id, payload)
        .map_err(map_bridge_error)?;
    Ok(())
}

fn route_resolve_operator_action(
    globals: &GlobalOptions,
    bridge: &TuiRemoteBridge,
    command: InboundCommand,
) -> Result<(), CliError> {
    let session_id = required_command_session(&command)?;
    let located = locate_remote_session(globals, session_id)?;
    let action_id = required_payload_string(&command.payload, &["actionId", "action_id"])?;
    let decision = required_payload_string(&command.payload, &["decision"])?;
    let user_answer = payload_string(&command.payload, &["userAnswer", "user_answer"]);
    let mut owned = vec![
        "operator-action".to_owned(),
        "resolve".to_owned(),
        "--project-id".to_owned(),
        located.project_id.clone(),
        "--action-id".to_owned(),
        action_id.to_owned(),
        "--decision".to_owned(),
        decision.to_owned(),
    ];
    if let Some(user_answer) = user_answer {
        owned.push("--user-answer".to_owned());
        owned.push(user_answer.to_owned());
    }
    let borrowed = owned.iter().map(String::as_str).collect::<Vec<_>>();
    let value = invoke_json(globals, &borrowed)?;
    send_command_ok(
        bridge,
        &located.remote_session_id,
        "xero.remote_operator_action_resolved.v1",
        json!({ "response": value.get("response").cloned().unwrap_or(value) }),
    )
}

fn route_cancel_run(
    globals: &GlobalOptions,
    bridge: &TuiRemoteBridge,
    command: InboundCommand,
) -> Result<(), CliError> {
    let session_id = required_command_session(&command)?;
    let located = locate_remote_session(globals, session_id)?;
    let local_session_id = required_session_id(&located.session)?.to_owned();
    let run_id = match payload_string(&command.payload, &["runId", "run_id"]) {
        Some(run_id) => run_id.to_owned(),
        None => latest_remote_session_run_id(globals, &located.project_id, &local_session_id)?,
    };
    let value = invoke_json(
        globals,
        &[
            "conversation",
            "cancel",
            "--project-id",
            &located.project_id,
            &run_id,
        ],
    )?;
    let run = value
        .get("snapshot")
        .cloned()
        .unwrap_or_else(|| value.clone());
    send_command_ok(
        bridge,
        &located.remote_session_id,
        "xero.remote_run_cancelled.v1",
        json!({ "run": run }),
    )?;
    let remote_session_id = located.remote_session_id.clone();
    let mut located =
        locate_remote_session_in_project(globals, &located.project_id, &local_session_id, true)
            .unwrap_or(located);
    located.remote_session_id = remote_session_id;
    publish_session_snapshot_to_bridge(bridge, globals, &located, value.get("snapshot"))?;
    Ok(())
}

fn route_update_session_controls(
    globals: &GlobalOptions,
    bridge: &TuiRemoteBridge,
    command: InboundCommand,
) -> Result<(), CliError> {
    let session_id = required_command_session(&command)?;
    let located = locate_remote_session(globals, session_id)?;
    let local_session_id = required_session_id(&located.session)?.to_owned();
    let session_kind = remote_session_kind_value(&located.session);
    ensure_remote_payload_matches_session_kind(&command.payload, session_kind)?;
    let choice = provider_choice_from_payload(globals, &command.payload)?;
    let runtime_agent_id = payload_string(
        &command.payload,
        &["agent", "runtimeAgentId", "runtime_agent_id"],
    )
    .unwrap_or(if session_kind == "computer_use" {
        "computer_use"
    } else {
        "engineer"
    });
    let thinking_effort =
        payload_string(&command.payload, &["thinkingEffort", "thinking_effort"]).unwrap_or("high");
    let run = remote_runtime_run_payload(
        &located.project_id,
        &local_session_id,
        "pending",
        &choice,
        runtime_agent_id,
        thinking_effort,
        "completed",
    );
    send_command_ok(
        bridge,
        &located.remote_session_id,
        "xero.remote_session_controls_updated.v1",
        json!({ "run": run }),
    )?;
    remote_state().send_ui_event(RemoteUiEvent::ControlsUpdated(RemoteControlsUpdate {
        session_id: session_id.to_owned(),
        provider_id: choice.provider_id,
        model_id: choice.model_id,
        runtime_agent_id: runtime_agent_id.to_owned(),
        thinking_effort: thinking_effort.to_owned(),
    }));
    Ok(())
}

fn route_stage_attachment(
    globals: &GlobalOptions,
    bridge: &TuiRemoteBridge,
    command: InboundCommand,
) -> Result<(), CliError> {
    let session_id = required_command_session(&command)?;
    let attachment_id =
        required_payload_string(&command.payload, &["attachmentId", "attachment_id"])?;
    let located = locate_remote_session(globals, session_id)?;
    let original_name =
        required_payload_string(&command.payload, &["originalName", "original_name"])?;
    let media_type = required_payload_string(&command.payload, &["mediaType", "media_type"])?;
    let bytes_base64 = required_payload_string(&command.payload, &["bytesBase64", "bytes_base64"])?;
    let run_id = payload_string(&command.payload, &["runId", "run_id"]).unwrap_or("pending");
    let result = invoke_json(
        globals,
        &[
            "attachment",
            "stage",
            "--project-id",
            &located.project_id,
            "--run-id",
            run_id,
            "--original-name",
            original_name,
            "--media-type",
            media_type,
            "--bytes-base64",
            bytes_base64,
        ],
    );
    let payload = match result {
        Ok(value) => json!({
            "schema": "xero.remote_attachment_staged.v1",
            "ok": true,
            "attachmentId": attachment_id,
            "attachment": value.get("attachment").cloned().unwrap_or(value),
        }),
        Err(error) => json!({
            "schema": "xero.remote_attachment_staged.v1",
            "ok": false,
            "attachmentId": attachment_id,
            "error": cli_error_payload(&error),
        }),
    };
    bridge
        .forward_control_event(&located.remote_session_id, payload)
        .map_err(map_bridge_error)?;
    Ok(())
}

fn route_discard_attachment(
    globals: &GlobalOptions,
    bridge: &TuiRemoteBridge,
    command: InboundCommand,
) -> Result<(), CliError> {
    let session_id = required_command_session(&command)?;
    let attachment_id =
        required_payload_string(&command.payload, &["attachmentId", "attachment_id"])?;
    let located = locate_remote_session(globals, session_id)?;
    let absolute_path =
        required_payload_string(&command.payload, &["absolutePath", "absolute_path"])?;
    let result = invoke_json(
        globals,
        &[
            "attachment",
            "discard",
            "--project-id",
            &located.project_id,
            "--absolute-path",
            absolute_path,
        ],
    );
    let payload = match result {
        Ok(_) => json!({
            "schema": "xero.remote_attachment_discarded.v1",
            "ok": true,
            "attachmentId": attachment_id,
        }),
        Err(error) => json!({
            "schema": "xero.remote_attachment_discarded.v1",
            "ok": false,
            "attachmentId": attachment_id,
            "error": cli_error_payload(&error),
        }),
    };
    bridge
        .forward_control_event(&located.remote_session_id, payload)
        .map_err(map_bridge_error)?;
    Ok(())
}

fn publish_remote_session_list(
    globals: &GlobalOptions,
    bridge: &TuiRemoteBridge,
) -> Result<(), CliError> {
    let sessions = remote_session_summaries(globals)?;
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

fn publish_remote_project_list(
    globals: &GlobalOptions,
    bridge: &TuiRemoteBridge,
) -> Result<(), CliError> {
    let projects = remote_project_summaries(globals)?;
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

fn publish_session_added_to_bridge(
    bridge: &TuiRemoteBridge,
    located: &LocatedRemoteSession,
) -> Result<(), CliError> {
    bridge
        .forward_control_event(
            "__sessions__",
            json!({
                "schema": "xero.remote_session_added.v1",
                "result": remote_session_result_payload(located),
            }),
        )
        .map_err(map_bridge_error)?;
    Ok(())
}

fn publish_session_snapshot_to_bridge(
    bridge: &TuiRemoteBridge,
    globals: &GlobalOptions,
    located: &LocatedRemoteSession,
    latest_run: Option<&JsonValue>,
) -> Result<(), CliError> {
    let snapshot = remote_session_snapshot(globals, located, latest_run)?;
    bridge
        .snapshot(&located.remote_session_id, snapshot)
        .map_err(map_bridge_error)?;
    Ok(())
}

fn remote_project_summaries(globals: &GlobalOptions) -> Result<Vec<JsonValue>, CliError> {
    let value = invoke_json(globals, &["project", "list"])?;
    Ok(value
        .get("projects")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .map(remote_project_summary_payload)
        .filter(|project| string_field(project, "projectId").is_some())
        .collect())
}

fn remote_project_summary_payload(project: &JsonValue) -> JsonValue {
    let project_id = string_field(project, "projectId").unwrap_or_default();
    let project_name = string_field(project, "name")
        .or_else(|| string_field(project, "projectName"))
        .unwrap_or_else(|| project_id.clone());
    json!({
        "projectId": project_id,
        "projectName": project_name,
    })
}

fn remote_session_summaries(globals: &GlobalOptions) -> Result<Vec<JsonValue>, CliError> {
    let mut sessions = Vec::new();
    for project in remote_project_summaries(globals)? {
        let Some(project_id) = string_field(&project, "projectId") else {
            continue;
        };
        let project_name = string_field(&project, "projectName");
        let value = invoke_json(globals, &["session", "list", "--project-id", &project_id])?;
        for session in value
            .get("sessions")
            .and_then(JsonValue::as_array)
            .into_iter()
            .flatten()
        {
            if remote_session_kind_value(session) == "computer_use" {
                continue;
            }
            sessions.push(remote_session_summary_payload(
                &project_id,
                project_name.as_deref(),
                session,
            ));
        }
    }
    Ok(sessions)
}

fn remote_session_summary_payload(
    project_id: &str,
    project_name: Option<&str>,
    session: &JsonValue,
) -> JsonValue {
    let status = string_field(session, "status").unwrap_or_default();
    json!({
        "projectId": project_id,
        "projectName": project_name.unwrap_or(project_id),
        "session": {
            "agentSessionId": string_field(session, "agentSessionId").unwrap_or_default(),
            "sessionKind": remote_session_kind_value(session),
            "title": string_field(session, "title").unwrap_or_else(|| "New Chat".to_owned()),
            "remoteVisible": status != "archived",
            "createdAt": string_field(session, "createdAt"),
            "updatedAt": string_field(session, "updatedAt"),
            "lastActivityAt": string_field(session, "updatedAt"),
        },
    })
}

fn remote_session_result_payload(located: &LocatedRemoteSession) -> JsonValue {
    json!({
        "projectId": located.project_id,
        "projectName": located.project_name.as_deref().unwrap_or(&located.project_id),
        "session": located.session,
    })
}

fn remote_session_snapshot(
    globals: &GlobalOptions,
    located: &LocatedRemoteSession,
    latest_run: Option<&JsonValue>,
) -> Result<JsonValue, CliError> {
    let session_id = required_session_id(&located.session)?;
    let mut runs = session_run_snapshots(globals, &located.project_id, session_id)?;
    if let Some(latest_run) = latest_run {
        if string_field(latest_run, "runId").as_deref() != Some("pending") {
            merge_latest_run_snapshot(&mut runs, latest_run);
        }
    }
    let runtime_run = latest_run
        .cloned()
        .or_else(|| runs.last().cloned())
        .or_else(|| {
            draft_runtime_run_from_preferences(globals, located)
                .ok()
                .flatten()
        })
        .unwrap_or(JsonValue::Null);
    let (context_snapshot, context_snapshot_error) = match load_remote_context_snapshot(
        globals,
        &located.project_id,
        session_id,
        None,
        None,
        None,
        None,
    ) {
        Ok(snapshot) => (snapshot, JsonValue::Null),
        Err(error) => (JsonValue::Null, cli_error_payload(&error)),
    };
    Ok(json!({
        "schema": "xero.remote_session_snapshot.v1",
        "projectId": located.project_id,
        "session": located.session,
        "runtimeRun": runtime_run,
        "runs": runs,
        "availableAgents": remote_available_agents(),
        "availableModels": remote_available_models(globals)?,
        "contextSnapshot": context_snapshot,
        "contextSnapshotError": context_snapshot_error,
    }))
}

fn draft_runtime_run_from_preferences(
    globals: &GlobalOptions,
    located: &LocatedRemoteSession,
) -> Result<Option<JsonValue>, CliError> {
    let session_id = required_session_id(&located.session)?;
    let preferences = tui_remote_preferences(globals);
    let payload = json!({
        "providerId": preferences.provider_id,
        "modelId": preferences.model_id,
    });
    let choice = provider_choice_from_payload(globals, &payload)?;
    let session_kind = remote_session_kind_value(&located.session);
    let runtime_agent_id = if session_kind == "computer_use" {
        "computer_use"
    } else {
        preferences
            .runtime_agent_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("generalist")
    };
    let thinking_effort = preferences
        .thinking_effort
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("high");
    Ok(Some(remote_runtime_run_payload(
        &located.project_id,
        session_id,
        "pending",
        &choice,
        runtime_agent_id,
        thinking_effort,
        "completed",
    )))
}

fn tui_remote_preferences(globals: &GlobalOptions) -> TuiRemotePreferences {
    let path = globals.state_dir.join("tui-preferences.json");
    let Ok(bytes) = std::fs::read(path) else {
        return TuiRemotePreferences::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn load_remote_context_snapshot(
    globals: &GlobalOptions,
    project_id: &str,
    session_id: &str,
    run_id: Option<&str>,
    provider_id: Option<&str>,
    model_id: Option<&str>,
    pending_prompt: Option<&str>,
) -> Result<JsonValue, CliError> {
    let mut owned = vec![
        "session-context".to_owned(),
        "snapshot".to_owned(),
        "--project-id".to_owned(),
        project_id.to_owned(),
        "--session-id".to_owned(),
        session_id.to_owned(),
    ];
    if let Some(run_id) = run_id {
        owned.push("--run-id".to_owned());
        owned.push(run_id.to_owned());
    }
    if let Some(provider_id) = provider_id {
        owned.push("--provider-id".to_owned());
        owned.push(provider_id.to_owned());
    }
    if let Some(model_id) = model_id {
        owned.push("--model-id".to_owned());
        owned.push(model_id.to_owned());
    }
    if let Some(pending_prompt) = pending_prompt {
        owned.push("--pending-prompt".to_owned());
        owned.push(pending_prompt.to_owned());
    }
    let borrowed = owned.iter().map(String::as_str).collect::<Vec<_>>();
    let value = invoke_json(globals, &borrowed)?;
    Ok(value.get("contextSnapshot").cloned().unwrap_or(value))
}

fn session_run_snapshots(
    globals: &GlobalOptions,
    project_id: &str,
    session_id: &str,
) -> Result<Vec<JsonValue>, CliError> {
    let value = invoke_json(
        globals,
        &["conversation", "list", "--project-id", project_id],
    )?;
    let mut summaries = value
        .get("runs")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter(|run| {
            string_field(run, "agentSessionId").as_deref() == Some(session_id)
                || string_field(run, "agent_session_id").as_deref() == Some(session_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    summaries.reverse();

    let mut runs = Vec::new();
    for summary in summaries {
        let Some(run_id) = string_field(&summary, "runId") else {
            continue;
        };
        let value = invoke_json(
            globals,
            &["conversation", "show", "--project-id", project_id, &run_id],
        )?;
        runs.push(value.get("snapshot").cloned().unwrap_or(value));
    }
    Ok(runs)
}

fn latest_remote_session_run_id(
    globals: &GlobalOptions,
    project_id: &str,
    session_id: &str,
) -> Result<String, CliError> {
    let value = invoke_json(
        globals,
        &["conversation", "list", "--project-id", project_id],
    )?;
    let runs = value
        .get("runs")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| {
            CliError::user_fixable(
                "xero_tui_remote_run_not_found",
                format!("Xero TUI could not find a run for session `{session_id}`."),
            )
        })?;
    let session_runs = runs
        .iter()
        .filter(|run| {
            string_field(run, "agentSessionId").as_deref() == Some(session_id)
                || string_field(run, "agent_session_id").as_deref() == Some(session_id)
        })
        .collect::<Vec<_>>();
    if let Some(active) = session_runs
        .iter()
        .find(|run| string_field(run, "status").is_some_and(|status| run_status_is_active(&status)))
    {
        return string_field(active, "runId").ok_or_else(|| {
            CliError::user_fixable(
                "xero_tui_remote_run_not_found",
                format!("Xero TUI could not identify the active run for session `{session_id}`."),
            )
        });
    }
    session_runs
        .into_iter()
        .find_map(|run| string_field(run, "runId"))
        .ok_or_else(|| {
            CliError::user_fixable(
                "xero_tui_remote_run_not_found",
                format!("Xero TUI could not find a run for session `{session_id}`."),
            )
        })
}

fn run_status_is_active(status: &str) -> bool {
    matches!(status, "starting" | "running" | "paused" | "cancelling")
}

fn merge_latest_run_snapshot(runs: &mut Vec<JsonValue>, latest_run: &JsonValue) {
    let Some(run_id) = string_field(latest_run, "runId") else {
        runs.push(latest_run.clone());
        return;
    };
    if let Some(existing) = runs
        .iter_mut()
        .find(|run| string_field(run, "runId").as_deref() == Some(run_id.as_str()))
    {
        *existing = latest_run.clone();
    } else {
        runs.push(latest_run.clone());
    }
}

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

fn remote_available_models(globals: &GlobalOptions) -> Result<Vec<JsonValue>, CliError> {
    Ok(available_model_payloads(&provider_entries(globals)?))
}

fn available_model_payloads(providers: &[ProviderEntry]) -> Vec<JsonValue> {
    let mut seen = BTreeSet::new();
    let mut options = providers
        .iter()
        .filter(|provider| provider.is_remote_model_provider())
        .flat_map(|provider| {
            let profile_id = provider
                .profiles
                .first()
                .cloned()
                .unwrap_or_else(|| provider.provider_id.clone());
            let models = if provider.models.is_empty() {
                vec![ProviderModelEntry {
                    model_id: provider.default_model.clone(),
                    display_name: provider.default_model.clone(),
                    thinking_supported: true,
                    thinking_effort_options: vec![
                        "none".into(),
                        "minimal".into(),
                        "low".into(),
                        "medium".into(),
                        "high".into(),
                        "x_high".into(),
                    ],
                    default_thinking_effort: Some("high".into()),
                }]
            } else {
                provider.models.clone()
            };
            models.into_iter().filter_map(move |model| {
                let model_id = model.model_id.trim();
                if model_id.is_empty() {
                    return None;
                }
                let display_name = model.display_name.trim();
                let label = if display_name.is_empty() {
                    model_id
                } else {
                    display_name
                };
                Some(json!({
                    "id": format!("{}:{}", profile_id, model_id),
                    "label": label,
                    "modelId": model_id,
                    "providerId": provider.provider_id,
                    "providerLabel": provider.label,
                    "providerProfileId": profile_id,
                    "thinkingSupported": model.thinking_supported,
                    "thinkingEffortOptions": model.thinking_effort_options,
                    "defaultThinkingEffort": model.default_thinking_effort,
                }))
            })
        })
        .filter(|option| {
            let id = string_field(option, "id").unwrap_or_default();
            !id.is_empty() && seen.insert(id)
        })
        .collect::<Vec<_>>();

    options.sort_by(|left, right| {
        string_field(left, "providerLabel")
            .cmp(&string_field(right, "providerLabel"))
            .then(string_field(left, "label").cmp(&string_field(right, "label")))
            .then(string_field(left, "modelId").cmp(&string_field(right, "modelId")))
    });

    options
}

impl ProviderEntry {
    fn is_remote_model_provider(&self) -> bool {
        self.catalog_kind == "model_provider" && !self.default_model.trim().is_empty()
    }

    fn matches_selector(&self, selector: &str) -> bool {
        self.provider_id == selector || self.profiles.iter().any(|profile| profile == selector)
    }
}

fn provider_choice_from_payload(
    globals: &GlobalOptions,
    payload: &JsonValue,
) -> Result<ProviderChoice, CliError> {
    let providers = provider_entries(globals)?;
    let provider_selector = payload_string(
        payload,
        &[
            "providerId",
            "provider_id",
            "providerProfileId",
            "provider_profile_id",
        ],
    );
    let requested_model =
        payload_string(payload, &["modelId", "model_id", "model"]).map(ToOwned::to_owned);

    if let Some(selector) = provider_selector {
        if let Some(provider) = providers
            .iter()
            .find(|provider| provider.matches_selector(selector))
        {
            let provider_profile_id = provider
                .profiles
                .iter()
                .find(|profile| profile.as_str() == selector)
                .cloned()
                .or_else(|| provider.profiles.first().cloned())
                .or_else(|| Some(provider.provider_id.clone()));
            return Ok(ProviderChoice {
                provider_id: provider.provider_id.clone(),
                provider_profile_id,
                model_id: requested_model.unwrap_or_else(|| provider.default_model.clone()),
            });
        }
    }

    if let Some(model_id) = requested_model {
        if let Some(provider) = providers.iter().find(|provider| {
            provider.default_model == model_id && provider.is_remote_model_provider()
        }) {
            return Ok(ProviderChoice {
                provider_id: provider.provider_id.clone(),
                provider_profile_id: provider
                    .profiles
                    .first()
                    .cloned()
                    .or_else(|| Some(provider.provider_id.clone())),
                model_id,
            });
        }
    }

    if let Some(provider) = providers.iter().find(|provider| {
        provider.provider_id != "fake_provider" && provider.is_remote_model_provider()
    }) {
        return Ok(ProviderChoice {
            provider_id: provider.provider_id.clone(),
            provider_profile_id: provider
                .profiles
                .first()
                .cloned()
                .or_else(|| Some(provider.provider_id.clone())),
            model_id: provider.default_model.clone(),
        });
    }

    Ok(ProviderChoice {
        provider_id: "fake_provider".to_owned(),
        provider_profile_id: Some("fake_provider".to_owned()),
        model_id: "fake-model".to_owned(),
    })
}

fn provider_entries(globals: &GlobalOptions) -> Result<Vec<ProviderEntry>, CliError> {
    let value = invoke_json(globals, &["provider", "list"])?;
    Ok(value
        .get("providers")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .map(provider_entry_from_json)
        .collect())
}

fn provider_entry_from_json(provider: &JsonValue) -> ProviderEntry {
    let profiles = provider
        .get("profiles")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(JsonValue::as_str)
        .map(ToOwned::to_owned)
        .collect();
    ProviderEntry {
        provider_id: string_field(provider, "providerId").unwrap_or_default(),
        label: string_field(provider, "label").unwrap_or_default(),
        default_model: string_field(provider, "defaultModel").unwrap_or_default(),
        catalog_kind: string_field(provider, "catalogKind").unwrap_or_default(),
        profiles,
        models: provider_model_entries_from_json(provider),
    }
}

fn provider_model_entries_from_json(provider: &JsonValue) -> Vec<ProviderModelEntry> {
    provider
        .get("models")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let model_id = string_field(model, "modelId")?;
            Some(ProviderModelEntry {
                display_name: string_field(model, "displayName")
                    .unwrap_or_else(|| model_id.clone()),
                model_id,
                thinking_supported: model
                    .get("thinkingSupported")
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(false),
                thinking_effort_options: model
                    .get("thinkingEffortOptions")
                    .and_then(JsonValue::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(JsonValue::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect(),
                default_thinking_effort: string_field(model, "defaultThinkingEffort"),
            })
        })
        .collect()
}

fn locate_project_for_remote_start(
    globals: &GlobalOptions,
    payload: &JsonValue,
) -> Result<LocatedRemoteProject, CliError> {
    let projects = remote_project_summaries(globals)?;
    if let Some(project_id) = payload_string(payload, &["projectId", "project_id"]) {
        return projects
            .into_iter()
            .find(|project| string_field(project, "projectId").as_deref() == Some(project_id))
            .map(located_project_from_summary)
            .ok_or_else(|| {
                CliError::user_fixable(
                    "xero_tui_remote_project_not_found",
                    format!("Xero TUI could not find project `{project_id}`."),
                )
            });
    }

    match projects.as_slice() {
        [project] => Ok(located_project_from_summary(project.clone())),
        [] => Err(CliError::user_fixable(
            "xero_tui_remote_project_not_found",
            "Remote start requires a registered TUI project.",
        )),
        _ => Err(CliError::user_fixable(
            "xero_tui_remote_project_required",
            "Remote start requires `projectId` because more than one TUI project is registered.",
        )),
    }
}

fn located_project_from_summary(project: JsonValue) -> LocatedRemoteProject {
    let project_id = string_field(&project, "projectId").unwrap_or_default();
    LocatedRemoteProject {
        project_name: string_field(&project, "projectName"),
        project_id,
    }
}

fn locate_remote_session(
    globals: &GlobalOptions,
    session_id: &str,
) -> Result<LocatedRemoteSession, CliError> {
    if session_id.trim().is_empty() {
        return Err(CliError::usage("Missing agent session id."));
    }
    if session_id == REMOTE_COMPUTER_USE_SESSION_ID {
        return locate_global_computer_use_session(globals);
    }
    for project in remote_project_summaries(globals)? {
        let Some(project_id) = string_field(&project, "projectId") else {
            continue;
        };
        if let Ok(located) =
            locate_remote_session_in_project(globals, &project_id, session_id, false)
        {
            return Ok(located);
        }
    }
    Err(CliError::user_fixable(
        "xero_tui_remote_session_not_found",
        format!("Xero TUI could not find session `{session_id}`."),
    ))
}

fn locate_global_computer_use_session(
    globals: &GlobalOptions,
) -> Result<LocatedRemoteSession, CliError> {
    ensure_global_computer_use_project(globals)?;
    let mut located = locate_remote_session_in_project(
        globals,
        GLOBAL_COMPUTER_USE_PROJECT_ID,
        GLOBAL_COMPUTER_USE_AGENT_SESSION_ID,
        true,
    )?;
    located.project_name = Some(GLOBAL_COMPUTER_USE_PROJECT_NAME.to_owned());
    located.remote_session_id = REMOTE_COMPUTER_USE_SESSION_ID.to_owned();
    Ok(located)
}

fn locate_remote_session_in_project(
    globals: &GlobalOptions,
    project_id: &str,
    session_id: &str,
    include_archived: bool,
) -> Result<LocatedRemoteSession, CliError> {
    let mut args = vec!["session", "list", "--project-id", project_id];
    if include_archived {
        args.push("--include-archived");
    }
    let value = invoke_json(globals, &args)?;
    let session = value
        .get("sessions")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .find(|session| string_field(session, "agentSessionId").as_deref() == Some(session_id))
        .cloned()
        .ok_or_else(|| {
            CliError::user_fixable(
                "xero_tui_remote_session_not_found",
                format!("Xero TUI could not find session `{session_id}`."),
            )
        })?;
    let project_name = if project_id == GLOBAL_COMPUTER_USE_PROJECT_ID {
        Some(GLOBAL_COMPUTER_USE_PROJECT_NAME.to_owned())
    } else {
        project_name_for_id(globals, project_id).ok().flatten()
    };
    let remote_session_id = if project_id == GLOBAL_COMPUTER_USE_PROJECT_ID
        && session_id == GLOBAL_COMPUTER_USE_AGENT_SESSION_ID
    {
        REMOTE_COMPUTER_USE_SESSION_ID
    } else {
        session_id
    }
    .to_owned();
    Ok(LocatedRemoteSession {
        project_id: project_id.to_owned(),
        project_name,
        session,
        remote_session_id,
    })
}

fn project_name_for_id(
    globals: &GlobalOptions,
    project_id: &str,
) -> Result<Option<String>, CliError> {
    Ok(remote_project_summaries(globals)?
        .into_iter()
        .find(|project| string_field(project, "projectId").as_deref() == Some(project_id))
        .and_then(|project| string_field(&project, "projectName")))
}

fn remote_attachments_from_payload(payload: &JsonValue) -> Result<Vec<JsonValue>, CliError> {
    let Some(value) = payload.get("attachments") else {
        return Ok(Vec::new());
    };
    let attachments = value.as_array().ok_or_else(|| {
        CliError::usage("Remote command `attachments` must be an array when provided.")
    })?;
    if attachments.iter().any(|attachment| !attachment.is_object()) {
        return Err(CliError::usage(
            "Remote command attachments must be staged attachment objects.",
        ));
    }
    Ok(attachments.clone())
}

fn remote_runtime_run_payload(
    project_id: &str,
    session_id: &str,
    run_id: &str,
    choice: &ProviderChoice,
    runtime_agent_id: &str,
    thinking_effort: &str,
    status: &str,
) -> JsonValue {
    json!({
        "projectId": project_id,
        "agentSessionId": session_id,
        "runId": run_id,
        "providerId": choice.provider_id,
        "modelId": choice.model_id,
        "status": status,
        "controls": {
            "active": {
                "runtimeAgentId": runtime_agent_id,
                "agentDefinitionId": runtime_agent_id,
                "providerProfileId": choice.provider_profile_id,
                "modelId": choice.model_id,
                "thinkingEffort": thinking_effort,
                "autoCompactEnabled": true,
            }
        }
    })
}

fn send_command_ok(
    bridge: &TuiRemoteBridge,
    session_id: &str,
    schema: &str,
    result: JsonValue,
) -> Result<(), CliError> {
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

fn required_command_session(command: &InboundCommand) -> Result<&str, CliError> {
    command
        .session_id
        .as_deref()
        .filter(|session_id| !session_id.trim().is_empty())
        .ok_or_else(|| CliError::usage("Missing remote session id."))
}

fn required_session_id(session: &JsonValue) -> Result<&str, CliError> {
    payload_string(
        session,
        &[
            "agentSessionId",
            "agent_session_id",
            "sessionId",
            "session_id",
        ],
    )
    .ok_or_else(|| CliError::usage("Missing agent session id."))
}

fn remote_session_kind_from_payload(payload: &JsonValue) -> Result<&'static str, CliError> {
    if let Some(value) = payload_string(payload, &["sessionKind", "session_kind"]) {
        return match value {
            "standard" => Ok("standard"),
            "computer_use" => Ok("computer_use"),
            other => Err(CliError::usage(format!(
                "Unsupported remote session kind `{other}`."
            ))),
        };
    }

    if remote_payload_agent_id(payload) == Some("computer_use") {
        return Ok("computer_use");
    }

    Ok("standard")
}

fn remote_session_kind_value(session: &JsonValue) -> &'static str {
    match payload_string(session, &["sessionKind", "session_kind"]) {
        Some("computer_use") => "computer_use",
        _ => "standard",
    }
}

fn remote_payload_agent_id(payload: &JsonValue) -> Option<&str> {
    payload_string(payload, &["agent", "runtimeAgentId", "runtime_agent_id"])
}

fn ensure_remote_payload_matches_session_kind(
    payload: &JsonValue,
    session_kind: &str,
) -> Result<(), CliError> {
    if let Some(agent_id) = remote_payload_agent_id(payload) {
        ensure_remote_agent_matches_session_kind(session_kind, agent_id)?;
    }
    Ok(())
}

fn ensure_remote_agent_matches_session_kind(
    session_kind: &str,
    agent_id: &str,
) -> Result<(), CliError> {
    match (session_kind, agent_id) {
        ("computer_use", "computer_use") => Ok(()),
        ("computer_use", other) => Err(CliError::usage(format!(
            "Computer Use sessions must use the `computer_use` agent, got `{other}`."
        ))),
        ("standard", "computer_use") => Err(CliError::usage(
            "The `computer_use` agent requires a Computer Use session.",
        )),
        _ => Ok(()),
    }
}

fn required_payload_string<'a>(payload: &'a JsonValue, keys: &[&str]) -> Result<&'a str, CliError> {
    payload_string(payload, keys).ok_or_else(|| {
        CliError::usage(format!(
            "Missing `{}` in remote command payload.",
            keys.first().copied().unwrap_or("field")
        ))
    })
}

fn payload_string<'a>(payload: &'a JsonValue, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        payload
            .get(*key)
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn payload_u64(payload: &JsonValue, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| payload.get(*key).and_then(JsonValue::as_u64))
}

fn string_field(value: &JsonValue, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn cli_error_payload(error: &CliError) -> JsonValue {
    json!({
        "code": error.code,
        "message": error.message,
        "exitCode": error.exit_code,
    })
}

fn map_bridge_error(error: BridgeError) -> CliError {
    CliError::system_fault("xero_tui_remote_bridge_failed", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::Path, sync::Arc};

    struct ProviderProfilesAdapter;

    impl crate::TuiCommandAdapter for ProviderProfilesAdapter {
        fn invoke_json(
            &self,
            _state_dir: &Path,
            args: &[String],
        ) -> Option<Result<JsonValue, CliError>> {
            match args {
                [command, subcommand] if command == "provider" && subcommand == "list" => {
                    Some(Ok(json!({
                        "providers": [{
                            "providerId": "openai_codex",
                            "label": "OpenAI Codex",
                            "defaultModel": "gpt-5.5",
                            "catalogKind": "model_provider",
                            "profiles": ["openai_codex-default"],
                            "models": [{
                                "modelId": "gpt-5.5",
                                "displayName": "GPT-5.5",
                                "thinkingSupported": true,
                                "thinkingEffortOptions": ["minimal", "low", "medium", "high"],
                                "defaultThinkingEffort": "medium"
                            }]
                        }]
                    })))
                }
                _ => None,
            }
        }
    }

    #[test]
    fn remote_session_summary_matches_cloud_visible_contract() {
        let session = json!({
            "agentSessionId": "session-1",
            "title": "Build the bridge",
            "status": "active",
            "createdAt": "2026-05-22T00:00:00Z",
            "updatedAt": "2026-05-22T00:05:00Z"
        });

        let payload = remote_session_summary_payload("project-1", Some("Xero"), &session);

        assert_eq!(payload["projectId"], json!("project-1"));
        assert_eq!(payload["projectName"], json!("Xero"));
        assert_eq!(payload["session"]["agentSessionId"], json!("session-1"));
        assert_eq!(payload["session"]["title"], json!("Build the bridge"));
        assert_eq!(payload["session"]["remoteVisible"], json!(true));
        assert_eq!(
            payload["session"]["lastActivityAt"],
            json!("2026-05-22T00:05:00Z")
        );
    }

    #[test]
    fn available_models_keep_provider_identity_for_duplicate_model_ids() {
        let providers = vec![
            ProviderEntry {
                provider_id: "openai_codex".into(),
                label: "OpenAI Codex".into(),
                default_model: "gpt-5.5".into(),
                catalog_kind: "model_provider".into(),
                profiles: Vec::new(),
                models: Vec::new(),
            },
            ProviderEntry {
                provider_id: "openai_api".into(),
                label: "OpenAI API".into(),
                default_model: "gpt-5.5".into(),
                catalog_kind: "model_provider".into(),
                profiles: vec!["profile-openai".into()],
                models: Vec::new(),
            },
        ];

        let models = available_model_payloads(&providers);

        assert_eq!(models.len(), 2);
        let codex = models
            .iter()
            .find(|model| model["id"] == json!("openai_codex:gpt-5.5"))
            .expect("codex model option");
        let openai_api = models
            .iter()
            .find(|model| model["id"] == json!("profile-openai:gpt-5.5"))
            .expect("openai api model option");
        assert_eq!(codex["providerProfileId"], json!("openai_codex"));
        assert_eq!(openai_api["providerProfileId"], json!("profile-openai"));
    }

    #[test]
    fn provider_id_selector_resolves_to_provider_profile_id_for_cloud_model_ids() {
        let mut globals = crate::tui::app::test_only_globals();
        globals.tui_adapter = Some(Arc::new(ProviderProfilesAdapter));

        let choice = provider_choice_from_payload(
            &globals,
            &json!({
                "providerId": "openai_codex",
                "modelId": "gpt-5.5"
            }),
        )
        .expect("provider choice");

        assert_eq!(choice.provider_id, "openai_codex");
        assert_eq!(
            choice.provider_profile_id.as_deref(),
            Some("openai_codex-default")
        );
        assert_eq!(choice.model_id, "gpt-5.5");
    }

    #[test]
    fn remote_runtime_event_payload_matches_desktop_bridge_contract() {
        let spec = remote_run_spec_fixture();
        let event = json!({
            "id": 42,
            "projectId": "project-1",
            "runId": "run-1",
            "eventKind": "message_delta",
            "payload": {
                "role": "assistant",
                "text": "streamed"
            },
            "createdAt": "2026-05-22T00:00:01Z"
        });

        let payload =
            remote_runtime_event_payload(&spec, &event, 42).expect("runtime event payload");

        assert_eq!(payload["schema"], json!("xero.remote_runtime_event.v1"));
        assert_eq!(payload["projectId"], json!("project-1"));
        assert_eq!(payload["agentSessionId"], json!("session-1"));
        assert_eq!(payload["runId"], json!("run-1"));
        assert_eq!(payload["eventId"], json!(42));
        assert_eq!(payload["eventKind"], json!("message_delta"));
        assert_eq!(payload["payload"]["text"], json!("streamed"));
        assert_eq!(payload["createdAt"], json!("2026-05-22T00:00:01Z"));
    }

    #[test]
    fn remote_attachments_accepts_staged_attachment_objects() {
        let payload = json!({
            "attachments": [{
                "kind": "image",
                "absolutePath": "/tmp/staged.png",
                "mediaType": "image/png",
                "originalName": "staged.png",
                "sizeBytes": 12
            }]
        });

        let attachments =
            remote_attachments_from_payload(&payload).expect("attachments should parse");

        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0]["absolutePath"], json!("/tmp/staged.png"));
    }

    fn remote_run_spec_fixture() -> RemoteRunSpec {
        RemoteRunSpec {
            project_id: "project-1".into(),
            project_name: Some("Xero".into()),
            session: json!({
                "agentSessionId": "session-1",
                "title": "Session"
            }),
            session_id: "session-1".into(),
            remote_session_id: "session-1".into(),
            run_id: "run-1".into(),
            choice: ProviderChoice {
                provider_id: "provider-1".into(),
                provider_profile_id: Some("profile-1".into()),
                model_id: "model-1".into(),
            },
            runtime_agent_id: "engineer".into(),
            thinking_effort: "high".into(),
            prompt: "hello".into(),
            attachments: Vec::new(),
        }
    }
}
