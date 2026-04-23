use std::{
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use url::Url;

use crate::{
    auth::now_timestamp,
    commands::{validate_non_empty, CommandError},
    db::project_store::{
        self, RuntimeRunRecord, RuntimeRunStatus, RuntimeRunTransportLiveness,
        RuntimeRunTransportRecord, RuntimeRunUpsertRecord,
    },
};

use super::boundary::emit_interactive_boundary_if_detected;
use super::live_events::{diagnostic_live_event, emit_normalized_events};
use super::persistence::{
    persist_runtime_row_from_shared, persist_sidecar_exit, persist_sidecar_runtime_error,
};
use super::{
    control::spawn_control_listener, runtime_supervisor_thinking_effort_env_value,
    validate_runtime_supervisor_launch_context, write_json_line, PtyEventNormalizer,
    RuntimeSupervisorSidecarArgs, SharedPtyWriter, SidecarSharedState, SupervisorEventHub,
    ANTHROPIC_API_KEY_ENV, CADENCE_RUNTIME_FLOW_ID_ENV, CADENCE_RUNTIME_MODEL_ID_ENV,
    CADENCE_RUNTIME_PROVIDER_ID_ENV, CADENCE_RUNTIME_SESSION_ID_ENV,
    CADENCE_RUNTIME_THINKING_EFFORT_ENV, HEARTBEAT_INTERVAL, OPENAI_API_KEY_ENV,
    OPENAI_API_VERSION_ENV, OPENAI_BASE_URL_ENV, TERMINAL_ATTACH_GRACE_PERIOD,
};
use crate::runtime::protocol::{
    RuntimeSupervisorLaunchContext, SupervisorProcessStatus, SupervisorStartupMessage,
    SUPERVISOR_KIND_DETACHED_PTY, SUPERVISOR_PROTOCOL_VERSION, SUPERVISOR_TRANSPORT_KIND_TCP,
};

pub(super) fn run_supervisor_sidecar_from_env() -> Result<(), CommandError> {
    match parse_sidecar_args(std::env::args().skip(1)) {
        Ok(args) => run_supervisor_sidecar(args),
        Err(error) => emit_startup_error(error),
    }
}

fn parse_sidecar_args(
    args: impl IntoIterator<Item = String>,
) -> Result<RuntimeSupervisorSidecarArgs, CommandError> {
    let mut project_id = None;
    let mut repo_root = None;
    let mut runtime_kind = None;
    let mut run_id = None;
    let mut session_id = None;
    let mut flow_id = None;
    let mut launch_context_json = None;
    let mut control_state_json = None;
    let mut program = None;
    let mut command_args = Vec::new();

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--project-id" => project_id = args.next(),
            "--repo-root" => repo_root = args.next().map(PathBuf::from),
            "--runtime-kind" => runtime_kind = args.next(),
            "--run-id" => run_id = args.next(),
            "--session-id" => session_id = args.next(),
            "--flow-id" => flow_id = args.next(),
            "--launch-context-json" => launch_context_json = args.next(),
            "--control-state-json" => control_state_json = args.next(),
            "--program" => program = args.next(),
            "--command-arg" => {
                let Some(value) = args.next() else {
                    return Err(CommandError::user_fixable(
                        "runtime_supervisor_request_invalid",
                        "Cadence received a detached supervisor command arg flag without a value.",
                    ));
                };
                command_args.push(value);
            }
            other => {
                return Err(CommandError::user_fixable(
                    "runtime_supervisor_request_invalid",
                    format!("Cadence received unsupported detached supervisor argument `{other}`."),
                ))
            }
        }
    }

    let args = RuntimeSupervisorSidecarArgs {
        project_id: project_id.ok_or_else(|| CommandError::invalid_request("projectId"))?,
        repo_root: repo_root.ok_or_else(|| CommandError::invalid_request("repoRoot"))?,
        runtime_kind: runtime_kind.ok_or_else(|| CommandError::invalid_request("runtimeKind"))?,
        run_id: run_id.ok_or_else(|| CommandError::invalid_request("runId"))?,
        session_id: session_id.ok_or_else(|| CommandError::invalid_request("sessionId"))?,
        flow_id,
        launch_context: serde_json::from_str::<RuntimeSupervisorLaunchContext>(
            &launch_context_json
                .ok_or_else(|| CommandError::invalid_request("launchContextJson"))?,
        )
        .map_err(|error| {
            CommandError::user_fixable(
                "runtime_supervisor_request_invalid",
                format!(
                    "Cadence could not decode the detached runtime supervisor launch context JSON: {error}"
                ),
            )
        })?,
        program: program.ok_or_else(|| CommandError::invalid_request("program"))?,
        args: command_args,
        run_controls: serde_json::from_str(
            &control_state_json.ok_or_else(|| CommandError::invalid_request("controlStateJson"))?,
        )
        .map_err(|error| {
            CommandError::user_fixable(
                "runtime_supervisor_request_invalid",
                format!(
                    "Cadence could not decode the detached runtime supervisor control seed JSON: {error}"
                ),
            )
        })?,
    };

    validate_non_empty(&args.project_id, "projectId")?;
    validate_non_empty(&args.runtime_kind, "runtimeKind")?;
    validate_non_empty(&args.run_id, "runId")?;
    validate_non_empty(&args.session_id, "sessionId")?;
    if let Some(flow_id) = args.flow_id.as_deref() {
        validate_non_empty(flow_id, "flowId")?;
    }
    validate_non_empty(&args.program, "program")?;
    validate_runtime_supervisor_launch_context(
        &args.runtime_kind,
        &args.session_id,
        args.flow_id.as_deref(),
        &args.run_controls,
        &args.launch_context,
    )?;

    Ok(args)
}

fn emit_startup_error(error: CommandError) -> Result<(), CommandError> {
    emit_startup_message(&SupervisorStartupMessage::Error {
        protocol_version: SUPERVISOR_PROTOCOL_VERSION,
        code: error.code,
        message: error.message,
        retryable: error.retryable,
    })
}

fn validate_inherited_launch_environment(
    launch_context: &RuntimeSupervisorLaunchContext,
) -> Result<(), CommandError> {
    match launch_context.provider_id.as_str() {
        crate::runtime::ANTHROPIC_PROVIDER_ID => {
            let anthropic_api_key = std::env::var(ANTHROPIC_API_KEY_ENV)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty());
            if anthropic_api_key.is_none() {
                return Err(CommandError::user_fixable(
                    "anthropic_api_key_missing",
                    "Cadence cannot launch the detached Anthropic runtime because the app-local API key was not injected into the launch environment.",
                ));
            }
        }
        crate::runtime::OPENAI_API_PROVIDER_ID
        | crate::runtime::AZURE_OPENAI_PROVIDER_ID
        | crate::runtime::GITHUB_MODELS_PROVIDER_ID
        | crate::runtime::GEMINI_AI_STUDIO_PROVIDER_ID => {
            let openai_api_key = std::env::var(OPENAI_API_KEY_ENV)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty());
            if openai_api_key.is_none() {
                return Err(CommandError::user_fixable(
                    match launch_context.provider_id.as_str() {
                        crate::runtime::OPENAI_API_PROVIDER_ID => "openai_api_key_missing",
                        crate::runtime::AZURE_OPENAI_PROVIDER_ID => "azure_openai_api_key_missing",
                        crate::runtime::GITHUB_MODELS_PROVIDER_ID => {
                            "github_models_token_missing"
                        }
                        crate::runtime::GEMINI_AI_STUDIO_PROVIDER_ID => {
                            "gemini_ai_studio_api_key_missing"
                        }
                        _ => "provider_api_key_missing",
                    },
                    "Cadence cannot launch the detached OpenAI-compatible runtime because the app-local API key was not injected into the launch environment.",
                ));
            }

            let openai_base_url = std::env::var(OPENAI_BASE_URL_ENV)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    CommandError::user_fixable(
                        "openai_compatible_base_url_missing",
                        "Cadence cannot launch the detached OpenAI-compatible runtime because the compatibility base URL was not injected into the launch environment.",
                    )
                })?;
            Url::parse(&openai_base_url).map_err(|error| {
                CommandError::user_fixable(
                    "openai_compatible_base_url_invalid",
                    format!(
                        "Cadence cannot launch the detached OpenAI-compatible runtime because OPENAI_BASE_URL `{openai_base_url}` was invalid: {error}"
                    ),
                )
            })?;

            if launch_context.provider_id == crate::runtime::AZURE_OPENAI_PROVIDER_ID {
                let api_version = std::env::var(OPENAI_API_VERSION_ENV)
                    .ok()
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty());
                if api_version.is_none() {
                    return Err(CommandError::user_fixable(
                        "openai_compatible_api_version_missing",
                        "Cadence cannot launch the detached Azure OpenAI runtime because OPENAI_API_VERSION was not injected into the launch environment.",
                    ));
                }
            }
        }
        _ => {}
    }

    Ok(())
}

fn apply_launch_context_to_child_environment(
    builder: &mut CommandBuilder,
    launch_context: &RuntimeSupervisorLaunchContext,
) {
    builder.env_remove(CADENCE_RUNTIME_PROVIDER_ID_ENV);
    builder.env_remove(CADENCE_RUNTIME_SESSION_ID_ENV);
    builder.env_remove(CADENCE_RUNTIME_FLOW_ID_ENV);
    builder.env_remove(CADENCE_RUNTIME_MODEL_ID_ENV);
    builder.env_remove(CADENCE_RUNTIME_THINKING_EFFORT_ENV);
    builder.env_remove(ANTHROPIC_API_KEY_ENV);
    builder.env_remove(OPENAI_API_KEY_ENV);
    builder.env_remove(OPENAI_BASE_URL_ENV);
    builder.env_remove(OPENAI_API_VERSION_ENV);

    builder.env(CADENCE_RUNTIME_PROVIDER_ID_ENV, &launch_context.provider_id);
    builder.env(CADENCE_RUNTIME_SESSION_ID_ENV, &launch_context.session_id);
    builder.env(CADENCE_RUNTIME_MODEL_ID_ENV, &launch_context.model_id);

    if let Some(flow_id) = launch_context.flow_id.as_deref() {
        builder.env(CADENCE_RUNTIME_FLOW_ID_ENV, flow_id);
    }

    if let Some(thinking_effort) = launch_context.thinking_effort.as_ref() {
        builder.env(
            CADENCE_RUNTIME_THINKING_EFFORT_ENV,
            runtime_supervisor_thinking_effort_env_value(thinking_effort),
        );
    }

    if launch_context.provider_id == crate::runtime::ANTHROPIC_PROVIDER_ID {
        if let Ok(api_key) = std::env::var(ANTHROPIC_API_KEY_ENV) {
            builder.env(ANTHROPIC_API_KEY_ENV, api_key);
        }
        return;
    }

    if matches!(
        launch_context.provider_id.as_str(),
        crate::runtime::OPENAI_API_PROVIDER_ID
            | crate::runtime::AZURE_OPENAI_PROVIDER_ID
            | crate::runtime::GITHUB_MODELS_PROVIDER_ID
            | crate::runtime::GEMINI_AI_STUDIO_PROVIDER_ID
    ) {
        if let Ok(api_key) = std::env::var(OPENAI_API_KEY_ENV) {
            builder.env(OPENAI_API_KEY_ENV, api_key);
        }
        if let Ok(base_url) = std::env::var(OPENAI_BASE_URL_ENV) {
            builder.env(OPENAI_BASE_URL_ENV, base_url);
        }
        if let Ok(api_version) = std::env::var(OPENAI_API_VERSION_ENV) {
            builder.env(OPENAI_API_VERSION_ENV, api_version);
        }
    }
}

fn run_supervisor_sidecar(args: RuntimeSupervisorSidecarArgs) -> Result<(), CommandError> {
    if let Err(error) = validate_inherited_launch_environment(&args.launch_context) {
        return emit_startup_error(error);
    }

    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_bind_failed",
            "Cadence could not bind the detached PTY supervisor control listener.",
        )
    })?;
    listener.set_nonblocking(true).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_bind_failed",
            "Cadence could not configure the detached PTY supervisor control listener.",
        )
    })?;
    let endpoint = listener
        .local_addr()
        .map_err(|_| {
            CommandError::retryable(
                "runtime_supervisor_bind_failed",
                "Cadence could not read the detached PTY supervisor control listener address.",
            )
        })?
        .to_string();

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize::default()).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_pty_failed",
            "Cadence could not allocate a PTY for the detached supervisor.",
        )
    })?;
    let mut builder = CommandBuilder::new(&args.program);
    builder.args(&args.args);
    builder.cwd(&args.repo_root);
    apply_launch_context_to_child_environment(&mut builder, &args.launch_context);

    let mut child = match pair.slave.spawn_command(builder) {
        Ok(child) => child,
        Err(_) => {
            emit_startup_message(&SupervisorStartupMessage::Error {
                protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                code: "runtime_supervisor_pty_failed".into(),
                message:
                    "Cadence could not spawn the requested command inside the detached PTY supervisor."
                        .into(),
                retryable: true,
            })?;
            return Ok(());
        }
    };

    let writer = match pair.master.take_writer() {
        Ok(writer) => writer,
        Err(_) => {
            let _ = child.kill();
            emit_startup_message(&SupervisorStartupMessage::Error {
                protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                code: "runtime_supervisor_writer_unavailable".into(),
                message: "Cadence could not take exclusive ownership of the detached PTY writer."
                    .into(),
                retryable: true,
            })?;
            return Ok(());
        }
    };
    let writer: SharedPtyWriter = Arc::new(Mutex::new(writer));

    let child_pid = child.process_id();
    let mut killer = child.clone_killer();
    let started_at = now_timestamp();

    let initial_snapshot = project_store::upsert_runtime_run(
        &args.repo_root,
        &RuntimeRunUpsertRecord {
            run: RuntimeRunRecord {
                project_id: args.project_id.clone(),
                run_id: args.run_id.clone(),
                runtime_kind: args.runtime_kind.clone(),
                provider_id: args.launch_context.provider_id.clone(),
                supervisor_kind: SUPERVISOR_KIND_DETACHED_PTY.into(),
                status: RuntimeRunStatus::Running,
                transport: RuntimeRunTransportRecord {
                    kind: SUPERVISOR_TRANSPORT_KIND_TCP.into(),
                    endpoint: endpoint.clone(),
                    liveness: RuntimeRunTransportLiveness::Reachable,
                },
                started_at: started_at.clone(),
                last_heartbeat_at: Some(started_at.clone()),
                stopped_at: None,
                last_error: None,
                updated_at: started_at.clone(),
            },
            checkpoint: None,
            control_state: Some(args.run_controls.clone()),
        },
    )
    .map_err(|_| {
        let _ = killer.kill();
        CommandError::retryable(
            "runtime_supervisor_persist_failed",
            "Cadence could not persist detached supervisor startup metadata.",
        )
    })?;

    let shared = Arc::new(Mutex::new(SidecarSharedState {
        project_id: args.project_id.clone(),
        run_id: args.run_id.clone(),
        runtime_kind: args.runtime_kind.clone(),
        provider_id: args.launch_context.provider_id.clone(),
        session_id: args.launch_context.session_id.clone(),
        flow_id: args.launch_context.flow_id.clone(),
        endpoint: endpoint.clone(),
        started_at: initial_snapshot.run.started_at.clone(),
        child_pid,
        status: SupervisorProcessStatus::Running,
        stop_requested: false,
        last_heartbeat_at: initial_snapshot.run.last_heartbeat_at.clone(),
        last_checkpoint_sequence: initial_snapshot.last_checkpoint_sequence,
        last_checkpoint_at: initial_snapshot.last_checkpoint_at.clone(),
        last_error: None,
        stopped_at: None,
        control_state: initial_snapshot.controls.clone(),
        next_boundary_serial: 0,
        active_boundary: None,
    }));
    let event_hub = Arc::new(Mutex::new(SupervisorEventHub::default()));
    let persistence_lock = Arc::new(Mutex::new(()));
    let shutdown = Arc::new(AtomicBool::new(false));

    let control_thread = spawn_control_listener(
        listener,
        super::control::ControlListenerContext {
            repo_root: args.repo_root.clone(),
            shared: shared.clone(),
            event_hub: event_hub.clone(),
            persistence_lock: persistence_lock.clone(),
            writer: writer.clone(),
            shutdown: shutdown.clone(),
            killer: Arc::new(Mutex::new(killer)),
        },
    );

    emit_startup_message(&SupervisorStartupMessage::Ready {
        protocol_version: SUPERVISOR_PROTOCOL_VERSION,
        project_id: args.project_id.clone(),
        run_id: args.run_id.clone(),
        supervisor_kind: SUPERVISOR_KIND_DETACHED_PTY.into(),
        transport_kind: SUPERVISOR_TRANSPORT_KIND_TCP.into(),
        endpoint: endpoint.clone(),
        started_at: initial_snapshot.run.started_at.clone(),
        supervisor_pid: std::process::id(),
        child_pid,
        status: SupervisorProcessStatus::Running,
        launch_context: args.launch_context.clone(),
    })?;

    let reader_thread = spawn_pty_reader(
        pair.master.try_clone_reader().map_err(|_| {
            CommandError::retryable(
                "runtime_supervisor_pty_failed",
                "Cadence could not clone the detached PTY supervisor reader.",
            )
        })?,
        args.repo_root.clone(),
        shared.clone(),
        event_hub.clone(),
        persistence_lock.clone(),
        shutdown.clone(),
    );
    let heartbeat_thread = spawn_heartbeat_loop(
        args.repo_root.clone(),
        shared.clone(),
        persistence_lock.clone(),
        shutdown.clone(),
    );

    let exit_status = child.wait().map_err(|_| {
        shutdown.store(true, Ordering::SeqCst);
        CommandError::retryable(
            "runtime_supervisor_wait_failed",
            "Cadence lost the detached PTY child before it returned an exit status.",
        )
    })?;

    persist_sidecar_exit(&args.repo_root, &shared, &persistence_lock, exit_status)?;
    thread::sleep(TERMINAL_ATTACH_GRACE_PERIOD);
    shutdown.store(true, Ordering::SeqCst);

    let _ = control_thread.join();
    let _ = reader_thread.join();
    let _ = heartbeat_thread.join();

    Ok(())
}

fn emit_startup_message(message: &SupervisorStartupMessage) -> Result<(), CommandError> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    write_json_line(&mut stdout, message).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_handshake_write_failed",
            "Cadence could not emit the detached PTY supervisor startup handshake.",
        )
    })
}

fn spawn_pty_reader(
    mut reader: Box<dyn std::io::Read + Send>,
    repo_root: PathBuf,
    shared: Arc<Mutex<SidecarSharedState>>,
    event_hub: Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: Arc<Mutex<()>>,
    shutdown: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        let mut normalizer = PtyEventNormalizer::default();

        while !shutdown.load(Ordering::SeqCst) {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    emit_normalized_events(
                        &repo_root,
                        &shared,
                        &event_hub,
                        &persistence_lock,
                        normalizer.finish(),
                    );
                    break;
                }
                Ok(bytes_read) => {
                    emit_normalized_events(
                        &repo_root,
                        &shared,
                        &event_hub,
                        &persistence_lock,
                        normalizer.push_chunk(&buffer[..bytes_read]),
                    );
                    emit_interactive_boundary_if_detected(
                        &repo_root,
                        &shared,
                        &event_hub,
                        &persistence_lock,
                        &mut normalizer,
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    emit_normalized_events(
                        &repo_root,
                        &shared,
                        &event_hub,
                        &persistence_lock,
                        vec![diagnostic_live_event(
                            "runtime_supervisor_reader_failed",
                            "Runtime stream read failed",
                            "Cadence lost the detached PTY reader before the child exited.",
                        )],
                    );
                    let _ = persist_sidecar_runtime_error(
                        &repo_root,
                        &shared,
                        &persistence_lock,
                        "runtime_supervisor_reader_failed",
                        "Cadence lost the detached PTY reader before the child exited.",
                    );
                    break;
                }
            }
        }
    })
}

fn spawn_heartbeat_loop(
    repo_root: PathBuf,
    shared: Arc<Mutex<SidecarSharedState>>,
    persistence_lock: Arc<Mutex<()>>,
    shutdown: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !shutdown.load(Ordering::SeqCst) {
            thread::sleep(HEARTBEAT_INTERVAL);
            if shutdown.load(Ordering::SeqCst) {
                break;
            }

            {
                let mut snapshot = shared.lock().expect("sidecar state lock poisoned");
                if matches!(
                    snapshot.status,
                    SupervisorProcessStatus::Stopped | SupervisorProcessStatus::Failed
                ) {
                    break;
                }
                snapshot.last_heartbeat_at = Some(now_timestamp());
            }

            let _ = persist_runtime_row_from_shared(&repo_root, &shared, &persistence_lock);
        }
    })
}
