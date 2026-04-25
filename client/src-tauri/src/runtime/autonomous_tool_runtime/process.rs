use std::{
    collections::BTreeMap,
    env,
    io::Read,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use super::{
    policy::{CommandPolicyDecision, PreparedCommandRequest},
    repo_scope::display_relative_or_root,
    AutonomousCommandOutput, AutonomousCommandPolicyOutcome, AutonomousCommandPolicyTrace,
    AutonomousCommandSessionChunk, AutonomousCommandSessionOperation,
    AutonomousCommandSessionOutput, AutonomousCommandSessionReadRequest,
    AutonomousCommandSessionStartRequest, AutonomousCommandSessionStopRequest,
    AutonomousCommandSessionStream, AutonomousToolCommandResult, AutonomousToolOutput,
    AutonomousToolResult, AutonomousToolRuntime, AUTONOMOUS_TOOL_COMMAND,
    AUTONOMOUS_TOOL_COMMAND_SESSION_READ, AUTONOMOUS_TOOL_COMMAND_SESSION_START,
    AUTONOMOUS_TOOL_COMMAND_SESSION_STOP,
};

use crate::{
    commands::{CommandError, CommandResult},
    runtime::{
        cancelled_error,
        process_tree::{
            cleanup_process_group_after_root_exit, configure_process_tree_root,
            terminate_process_tree,
        },
        redaction::{
            find_prohibited_persistence_content, redact_command_argv_for_persistence,
            render_command_for_persistence,
        },
    },
};

const REDACTED_COMMAND_OUTPUT_SUMMARY: &str =
    "Command output was redacted before durable persistence.";
const COMMAND_SESSION_INITIAL_DRAIN: Duration = Duration::from_millis(150);
const DEFAULT_COMMAND_SESSION_READ_BYTES: usize = 16 * 1024;
const MAX_COMMAND_SESSION_READ_BYTES: usize = 64 * 1024;
const MAX_COMMAND_SESSIONS: usize = 8;
const MAX_COMMAND_SESSION_STORED_CHUNKS: usize = 512;
const MAX_COMMAND_SESSION_STORED_BYTES: usize = 1024 * 1024;
const SAFE_COMMAND_ENV_KEYS: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "TMPDIR",
    "TMP",
    "TEMP",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "PNPM_HOME",
    "NPM_CONFIG_CACHE",
    "npm_config_cache",
    "COREPACK_HOME",
    "SystemRoot",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
];

#[derive(Debug, Default)]
pub(super) struct ProcessSessionRegistry {
    sessions: Mutex<BTreeMap<String, Arc<ProcessSession>>>,
    next_id: AtomicU64,
}

impl ProcessSessionRegistry {
    fn next_session_id(&self) -> String {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        format!("cmd-session-{id}")
    }

    fn insert(&self, session: Arc<ProcessSession>) -> CommandResult<()> {
        let mut sessions = self.sessions.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Cadence could not lock the command session registry.",
            )
        })?;
        if sessions.len() >= MAX_COMMAND_SESSIONS {
            return Err(CommandError::user_fixable(
                "autonomous_tool_command_session_limit_reached",
                format!(
                    "Cadence limits autonomous command sessions to {MAX_COMMAND_SESSIONS} concurrent process(es). Stop an existing session before starting another."
                ),
            ));
        }
        sessions.insert(session.session_id.clone(), session);
        Ok(())
    }

    fn ensure_capacity(&self) -> CommandResult<()> {
        let sessions = self.sessions.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Cadence could not lock the command session registry.",
            )
        })?;
        if sessions.len() >= MAX_COMMAND_SESSIONS {
            return Err(CommandError::user_fixable(
                "autonomous_tool_command_session_limit_reached",
                format!(
                    "Cadence limits autonomous command sessions to {MAX_COMMAND_SESSIONS} concurrent process(es). Stop an existing session before starting another."
                ),
            ));
        }
        Ok(())
    }

    fn get(&self, session_id: &str) -> CommandResult<Arc<ProcessSession>> {
        let sessions = self.sessions.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Cadence could not lock the command session registry.",
            )
        })?;
        sessions.get(session_id).cloned().ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_command_session_not_found",
                format!("Cadence could not find command session `{session_id}`."),
            )
        })
    }

    fn remove(&self, session_id: &str) -> CommandResult<Arc<ProcessSession>> {
        let mut sessions = self.sessions.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Cadence could not lock the command session registry.",
            )
        })?;
        sessions.remove(session_id).ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_command_session_not_found",
                format!("Cadence could not find command session `{session_id}`."),
            )
        })
    }
}

impl Drop for ProcessSessionRegistry {
    fn drop(&mut self) {
        if let Ok(sessions) = self.sessions.get_mut() {
            for session in sessions.values() {
                let _ = session.kill();
            }
        }
    }
}

#[derive(Debug)]
struct ProcessSession {
    session_id: String,
    argv: Vec<String>,
    cwd: String,
    child: Mutex<Option<Child>>,
    chunks: Mutex<Vec<AutonomousCommandSessionChunk>>,
    next_sequence: AtomicU64,
    exit_code: Mutex<Option<i32>>,
}

impl ProcessSession {
    fn new(session_id: String, argv: Vec<String>, cwd: String, child: Child) -> Self {
        Self {
            session_id,
            argv,
            cwd,
            child: Mutex::new(Some(child)),
            chunks: Mutex::new(Vec::new()),
            next_sequence: AtomicU64::new(1),
            exit_code: Mutex::new(None),
        }
    }

    fn push_chunk(
        &self,
        stream: AutonomousCommandSessionStream,
        capture: SanitizedCommandOutput,
    ) -> CommandResult<()> {
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        let mut chunks = self.chunks.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Cadence could not lock command session output.",
            )
        })?;
        chunks.push(AutonomousCommandSessionChunk {
            sequence,
            stream,
            text: capture.text,
            truncated: capture.truncated,
            redacted: capture.redacted,
        });
        prune_command_session_chunks(&mut chunks);
        Ok(())
    }

    fn read_chunks_after(
        &self,
        after_sequence: u64,
        max_bytes: usize,
    ) -> CommandResult<Vec<AutonomousCommandSessionChunk>> {
        let chunks = self.chunks.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Cadence could not lock command session output.",
            )
        })?;
        let mut selected = Vec::new();
        let mut bytes = 0_usize;
        for chunk in chunks
            .iter()
            .filter(|chunk| chunk.sequence > after_sequence)
            .cloned()
        {
            let chunk_bytes = chunk.text.as_deref().map(str::len).unwrap_or_default();
            if !selected.is_empty() && bytes.saturating_add(chunk_bytes) > max_bytes {
                break;
            }
            bytes = bytes.saturating_add(chunk_bytes);
            selected.push(chunk);
            if bytes >= max_bytes {
                break;
            }
        }
        Ok(selected)
    }

    fn next_sequence_value(&self) -> u64 {
        self.next_sequence.load(Ordering::Relaxed)
    }

    fn poll_exit(&self) -> CommandResult<Option<i32>> {
        if let Some(exit_code) = *self.exit_code.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Cadence could not lock command session exit state.",
            )
        })? {
            return Ok(Some(exit_code));
        }

        let mut child = self.child.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Cadence could not lock command session process state.",
            )
        })?;
        let Some(child_ref) = child.as_mut() else {
            return Ok(*self.exit_code.lock().map_err(|_| {
                CommandError::system_fault(
                    "autonomous_tool_command_session_lock_failed",
                    "Cadence could not lock command session exit state.",
                )
            })?);
        };
        match child_ref.try_wait() {
            Ok(Some(status)) => {
                let exit_code = status.code();
                cleanup_process_group_after_root_exit(child_ref.id());
                *self.exit_code.lock().map_err(|_| {
                    CommandError::system_fault(
                        "autonomous_tool_command_session_lock_failed",
                        "Cadence could not lock command session exit state.",
                    )
                })? = exit_code;
                *child = None;
                Ok(exit_code)
            }
            Ok(None) => Ok(None),
            Err(error) => Err(CommandError::retryable(
                "autonomous_tool_command_session_wait_failed",
                format!(
                    "Cadence could not observe command session `{}`: {error}",
                    self.session_id
                ),
            )),
        }
    }

    fn kill(&self) -> CommandResult<Option<i32>> {
        let mut child = self.child.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Cadence could not lock command session process state.",
            )
        })?;
        let Some(child_ref) = child.as_mut() else {
            return Ok(*self.exit_code.lock().map_err(|_| {
                CommandError::system_fault(
                    "autonomous_tool_command_session_lock_failed",
                    "Cadence could not lock command session exit state.",
                )
            })?);
        };
        match child_ref.try_wait() {
            Ok(Some(status)) => {
                let exit_code = status.code();
                cleanup_process_group_after_root_exit(child_ref.id());
                *self.exit_code.lock().map_err(|_| {
                    CommandError::system_fault(
                        "autonomous_tool_command_session_lock_failed",
                        "Cadence could not lock command session exit state.",
                    )
                })? = exit_code;
                *child = None;
                Ok(exit_code)
            }
            Ok(None) => {
                let status = terminate_process_tree(child_ref).map_err(|error| {
                    CommandError::retryable(
                        "autonomous_tool_command_session_stop_failed",
                        format!(
                            "Cadence could not stop command session `{}`: {error}",
                            self.session_id
                        ),
                    )
                })?;
                let exit_code = status.code();
                *self.exit_code.lock().map_err(|_| {
                    CommandError::system_fault(
                        "autonomous_tool_command_session_lock_failed",
                        "Cadence could not lock command session exit state.",
                    )
                })? = exit_code;
                *child = None;
                Ok(exit_code)
            }
            Err(error) => Err(CommandError::retryable(
                "autonomous_tool_command_session_wait_failed",
                format!(
                    "Cadence could not observe command session `{}` before stopping it: {error}",
                    self.session_id
                ),
            )),
        }
    }
}

impl AutonomousToolRuntime {
    pub fn command(
        &self,
        request: super::AutonomousCommandRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.command_with_approval(request, false)
    }

    pub fn command_with_operator_approval(
        &self,
        request: super::AutonomousCommandRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.command_with_approval(request, true)
    }

    fn command_with_approval(
        &self,
        request: super::AutonomousCommandRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let decision = self.evaluate_command_policy(self.prepare_command_request(request)?)?;

        match decision {
            CommandPolicyDecision::Allow { prepared, policy } => {
                self.spawn_command(prepared, policy)
            }
            CommandPolicyDecision::Escalate { prepared, policy } if operator_approved => {
                let policy = operator_approved_policy(policy, &prepared.argv);
                self.spawn_command(prepared, policy)
            }
            CommandPolicyDecision::Escalate { prepared, policy } => {
                self.unspawned_command_approval_result(prepared, policy)
            }
        }
    }

    fn spawn_command(
        &self,
        prepared: PreparedCommandRequest,
        policy: AutonomousCommandPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        let mut command = Command::new(&prepared.argv[0]);
        command
            .args(prepared.argv.iter().skip(1))
            .current_dir(&prepared.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_process_tree_root(&mut command);
        apply_sanitized_command_environment(&mut command);

        let mut child = command.spawn().map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError::user_fixable(
                "autonomous_tool_command_not_found",
                format!("Cadence could not find command `{}`.", prepared.argv[0]),
            ),
            _ => CommandError::system_fault(
                "autonomous_tool_command_spawn_failed",
                format!(
                    "Cadence could not launch command `{}`: {error}",
                    prepared.argv[0]
                ),
            ),
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            CommandError::system_fault(
                "autonomous_tool_command_stdout_missing",
                "Cadence could not capture command stdout.",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            CommandError::system_fault(
                "autonomous_tool_command_stderr_missing",
                "Cadence could not capture command stderr.",
            )
        })?;

        let stdout_handle = spawn_capture(stdout, self.limits.max_command_capture_bytes);
        let stderr_handle = spawn_capture(stderr, self.limits.max_command_capture_bytes);
        let started_at = Instant::now();
        let timeout_duration = Duration::from_millis(prepared.timeout_ms);

        let (status, timed_out, cancelled) = loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    cleanup_process_group_after_root_exit(child.id());
                    break (status, false, false);
                }
                Ok(None) if self.is_cancelled() => {
                    let status = terminate_process_tree(&mut child).map_err(|error| {
                        CommandError::system_fault(
                            "autonomous_tool_command_wait_failed",
                            format!(
                                "Cadence could not stop cancelled command `{}`: {error}",
                                prepared.argv[0]
                            ),
                        )
                    })?;
                    break (status, false, true);
                }
                Ok(None) if started_at.elapsed() >= timeout_duration => {
                    let status = terminate_process_tree(&mut child).map_err(|error| {
                        CommandError::system_fault(
                            "autonomous_tool_command_wait_failed",
                            format!(
                                "Cadence could not stop timed-out command `{}`: {error}",
                                prepared.argv[0]
                            ),
                        )
                    })?;
                    break (status, true, false);
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    let _ = terminate_process_tree(&mut child);
                    return Err(CommandError::system_fault(
                        "autonomous_tool_command_wait_failed",
                        format!(
                            "Cadence could not observe command `{}` while it was running: {error}",
                            prepared.argv[0]
                        ),
                    ));
                }
            }
        };

        let stdout_capture = join_capture(stdout_handle)?;
        let stderr_capture = join_capture(stderr_handle)?;

        if cancelled {
            return Err(cancelled_error());
        }

        if timed_out {
            return Err(CommandError::retryable(
                "autonomous_tool_command_timeout",
                format!(
                    "Cadence timed out command `{}` after {}ms.",
                    render_command_for_summary(&prepared.argv),
                    prepared.timeout_ms,
                ),
            ));
        }

        let stdout_excerpt = sanitize_command_output(
            stdout_capture.excerpt.as_slice(),
            stdout_capture.truncated,
            self.limits.max_command_excerpt_chars,
        );
        let stderr_excerpt = sanitize_command_output(
            stderr_capture.excerpt.as_slice(),
            stderr_capture.truncated,
            self.limits.max_command_excerpt_chars,
        );

        let exit_code = status.code();
        let command_result = AutonomousToolCommandResult {
            exit_code,
            timed_out: false,
            summary: command_result_summary(&prepared.argv, exit_code),
            policy: policy.clone(),
        };
        let summary = match exit_code {
            Some(0) => format!(
                "Command `{}` exited successfully in `{}` under active `{}` policy.",
                render_command_for_summary(&prepared.argv),
                display_relative_or_root(&self.repo_root, &prepared.cwd),
                approval_mode_label(&policy.approval_mode),
            ),
            Some(code) => format!(
                "Command `{}` exited with code {code} in `{}` under active `{}` policy.",
                render_command_for_summary(&prepared.argv),
                display_relative_or_root(&self.repo_root, &prepared.cwd),
                approval_mode_label(&policy.approval_mode),
            ),
            None => format!(
                "Command `{}` terminated without an exit code in `{}` under active `{}` policy.",
                render_command_for_summary(&prepared.argv),
                display_relative_or_root(&self.repo_root, &prepared.cwd),
                approval_mode_label(&policy.approval_mode),
            ),
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_COMMAND.into(),
            summary,
            command_result: Some(command_result),
            output: AutonomousToolOutput::Command(AutonomousCommandOutput {
                argv: redact_command_argv_for_persistence(&prepared.argv),
                cwd: display_relative_or_root(&self.repo_root, &prepared.cwd),
                stdout: stdout_excerpt.text,
                stderr: stderr_excerpt.text,
                stdout_truncated: stdout_excerpt.truncated,
                stderr_truncated: stderr_excerpt.truncated,
                stdout_redacted: stdout_excerpt.redacted,
                stderr_redacted: stderr_excerpt.redacted,
                exit_code,
                timed_out: false,
                spawned: true,
                policy,
            }),
        })
    }

    fn unspawned_command_approval_result(
        &self,
        prepared: PreparedCommandRequest,
        policy: AutonomousCommandPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        let cwd = prepared
            .cwd_relative
            .as_ref()
            .map(|path| display_relative_or_root(&self.repo_root, &self.repo_root.join(path)))
            .unwrap_or_else(|| ".".into());
        let summary = format!(
            "Command `{}` requires operator review before Cadence can run it.",
            render_command_for_summary(&prepared.argv)
        );

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_COMMAND.into(),
            summary: summary.clone(),
            command_result: Some(AutonomousToolCommandResult {
                exit_code: None,
                timed_out: false,
                summary,
                policy: policy.clone(),
            }),
            output: AutonomousToolOutput::Command(AutonomousCommandOutput {
                argv: redact_command_argv_for_persistence(&prepared.argv),
                cwd,
                stdout: None,
                stderr: None,
                stdout_truncated: false,
                stderr_truncated: false,
                stdout_redacted: false,
                stderr_redacted: false,
                exit_code: None,
                timed_out: false,
                spawned: false,
                policy,
            }),
        })
    }
}

impl AutonomousToolRuntime {
    pub fn command_session_start(
        &self,
        request: AutonomousCommandSessionStartRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.command_session_start_with_approval(request, false)
    }

    pub fn command_session_start_with_operator_approval(
        &self,
        request: AutonomousCommandSessionStartRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.command_session_start_with_approval(request, true)
    }

    fn command_session_start_with_approval(
        &self,
        request: AutonomousCommandSessionStartRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let prepared_request = super::AutonomousCommandRequest {
            argv: request.argv,
            cwd: request.cwd,
            timeout_ms: request.timeout_ms,
        };
        let decision =
            self.evaluate_command_policy(self.prepare_command_request(prepared_request)?)?;

        match decision {
            CommandPolicyDecision::Allow { prepared, policy } => {
                self.spawn_command_session_start(prepared, policy)
            }
            CommandPolicyDecision::Escalate { prepared, policy } if operator_approved => {
                let policy = operator_approved_policy(policy, &prepared.argv);
                self.spawn_command_session_start(prepared, policy)
            }
            CommandPolicyDecision::Escalate { prepared, policy } => {
                self.unspawned_command_session_approval_result(prepared, policy)
            }
        }
    }

    fn spawn_command_session_start(
        &self,
        prepared: PreparedCommandRequest,
        policy: AutonomousCommandPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        self.process_sessions.ensure_capacity()?;
        let mut command = Command::new(&prepared.argv[0]);
        command
            .args(prepared.argv.iter().skip(1))
            .current_dir(&prepared.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_process_tree_root(&mut command);
        apply_sanitized_command_environment(&mut command);

        let mut child = command.spawn().map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError::user_fixable(
                "autonomous_tool_command_not_found",
                format!("Cadence could not find command `{}`.", prepared.argv[0]),
            ),
            _ => CommandError::system_fault(
                "autonomous_tool_command_session_spawn_failed",
                format!(
                    "Cadence could not launch command session `{}`: {error}",
                    prepared.argv[0]
                ),
            ),
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            CommandError::system_fault(
                "autonomous_tool_command_stdout_missing",
                "Cadence could not capture command session stdout.",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            CommandError::system_fault(
                "autonomous_tool_command_stderr_missing",
                "Cadence could not capture command session stderr.",
            )
        })?;

        let session_id = self.process_sessions.next_session_id();
        let cwd = display_relative_or_root(&self.repo_root, &prepared.cwd);
        let session = Arc::new(ProcessSession::new(
            session_id.clone(),
            redact_command_argv_for_persistence(&prepared.argv),
            cwd.clone(),
            child,
        ));
        spawn_session_reader(
            Arc::clone(&session),
            stdout,
            AutonomousCommandSessionStream::Stdout,
            self.limits.max_command_excerpt_chars,
        );
        spawn_session_reader(
            Arc::clone(&session),
            stderr,
            AutonomousCommandSessionStream::Stderr,
            self.limits.max_command_excerpt_chars,
        );
        if let Err(error) = self.process_sessions.insert(Arc::clone(&session)) {
            let _ = session.kill();
            return Err(error);
        }
        thread::sleep(COMMAND_SESSION_INITIAL_DRAIN);
        let exit_code = session.poll_exit()?;
        let chunks = session.read_chunks_after(0, DEFAULT_COMMAND_SESSION_READ_BYTES)?;
        let running = exit_code.is_none();

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_COMMAND_SESSION_START.into(),
            summary: if running {
                format!(
                    "Started command session `{session_id}` for `{}` in `{cwd}`.",
                    render_command_for_summary(&prepared.argv)
                )
            } else {
                format!(
                    "Command session `{session_id}` for `{}` exited during startup.",
                    render_command_for_summary(&prepared.argv)
                )
            },
            command_result: None,
            output: AutonomousToolOutput::CommandSession(AutonomousCommandSessionOutput {
                operation: AutonomousCommandSessionOperation::Start,
                session_id,
                argv: redact_command_argv_for_persistence(&prepared.argv),
                cwd,
                running,
                exit_code,
                spawned: true,
                chunks,
                next_sequence: session.next_sequence_value(),
                policy: Some(policy),
            }),
        })
    }

    fn unspawned_command_session_approval_result(
        &self,
        prepared: PreparedCommandRequest,
        policy: AutonomousCommandPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        let cwd = prepared
            .cwd_relative
            .as_ref()
            .map(|path| display_relative_or_root(&self.repo_root, &self.repo_root.join(path)))
            .unwrap_or_else(|| ".".into());
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_COMMAND_SESSION_START.into(),
            summary: format!(
                "Command session `{}` requires operator review before Cadence can start it.",
                render_command_for_summary(&prepared.argv)
            ),
            command_result: None,
            output: AutonomousToolOutput::CommandSession(AutonomousCommandSessionOutput {
                operation: AutonomousCommandSessionOperation::Start,
                session_id: "unstarted".into(),
                argv: redact_command_argv_for_persistence(&prepared.argv),
                cwd,
                running: false,
                exit_code: None,
                spawned: false,
                chunks: Vec::new(),
                next_sequence: 0,
                policy: Some(policy),
            }),
        })
    }

    pub fn command_session_read(
        &self,
        request: AutonomousCommandSessionReadRequest,
    ) -> CommandResult<AutonomousToolResult> {
        crate::commands::validate_non_empty(&request.session_id, "sessionId")?;
        let session = self.process_sessions.get(request.session_id.trim())?;
        let exit_code = session.poll_exit()?;
        let max_bytes = request
            .max_bytes
            .unwrap_or(DEFAULT_COMMAND_SESSION_READ_BYTES)
            .clamp(1, MAX_COMMAND_SESSION_READ_BYTES);
        let chunks = session.read_chunks_after(request.after_sequence.unwrap_or(0), max_bytes)?;
        let running = exit_code.is_none();

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_COMMAND_SESSION_READ.into(),
            summary: if running {
                format!(
                    "Read {} output chunk(s) from command session `{}`.",
                    chunks.len(),
                    session.session_id
                )
            } else {
                format!(
                    "Read {} output chunk(s) from completed command session `{}`.",
                    chunks.len(),
                    session.session_id
                )
            },
            command_result: None,
            output: AutonomousToolOutput::CommandSession(AutonomousCommandSessionOutput {
                operation: AutonomousCommandSessionOperation::Read,
                session_id: session.session_id.clone(),
                argv: session.argv.clone(),
                cwd: session.cwd.clone(),
                running,
                exit_code,
                spawned: true,
                chunks,
                next_sequence: session.next_sequence_value(),
                policy: None,
            }),
        })
    }

    pub fn command_session_stop(
        &self,
        request: AutonomousCommandSessionStopRequest,
    ) -> CommandResult<AutonomousToolResult> {
        crate::commands::validate_non_empty(&request.session_id, "sessionId")?;
        let session = self.process_sessions.remove(request.session_id.trim())?;
        let exit_code = session.kill()?;
        thread::sleep(COMMAND_SESSION_INITIAL_DRAIN);
        let chunks = session.read_chunks_after(0, MAX_COMMAND_SESSION_READ_BYTES)?;

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_COMMAND_SESSION_STOP.into(),
            summary: format!("Stopped command session `{}`.", session.session_id),
            command_result: None,
            output: AutonomousToolOutput::CommandSession(AutonomousCommandSessionOutput {
                operation: AutonomousCommandSessionOperation::Stop,
                session_id: session.session_id.clone(),
                argv: session.argv.clone(),
                cwd: session.cwd.clone(),
                running: false,
                exit_code,
                spawned: true,
                chunks,
                next_sequence: session.next_sequence_value(),
                policy: None,
            }),
        })
    }
}

fn render_command_for_summary(argv: &[String]) -> String {
    render_command_for_persistence(argv)
}

pub(super) fn apply_sanitized_command_environment(command: &mut Command) {
    command.env_clear();
    for key in SAFE_COMMAND_ENV_KEYS {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
    if env::var_os("PATH").is_none() {
        command.env(
            "PATH",
            "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin",
        );
    }
    command.env("CADENCE_AGENT_SANITIZED_ENV", "1");
}

fn prune_command_session_chunks(chunks: &mut Vec<AutonomousCommandSessionChunk>) {
    let mut total_bytes = chunks
        .iter()
        .map(command_session_chunk_bytes)
        .sum::<usize>();
    let mut drop_count = 0;
    while chunks.len().saturating_sub(drop_count) > MAX_COMMAND_SESSION_STORED_CHUNKS
        || total_bytes > MAX_COMMAND_SESSION_STORED_BYTES
    {
        let Some(chunk) = chunks.get(drop_count) else {
            break;
        };
        total_bytes = total_bytes.saturating_sub(command_session_chunk_bytes(chunk));
        drop_count += 1;
    }

    if drop_count > 0 {
        chunks.drain(0..drop_count);
    }
}

fn command_session_chunk_bytes(chunk: &AutonomousCommandSessionChunk) -> usize {
    chunk.text.as_deref().map(str::len).unwrap_or_default()
}

fn operator_approved_policy(
    mut policy: AutonomousCommandPolicyTrace,
    argv: &[String],
) -> AutonomousCommandPolicyTrace {
    policy.outcome = AutonomousCommandPolicyOutcome::Allowed;
    policy.code = "policy_allowed_after_operator_approval".into();
    policy.reason = format!(
        "Operator approval allowed previously escalated command `{}` to run.",
        render_command_for_summary(argv)
    );
    policy
}

fn spawn_session_reader(
    session: Arc<ProcessSession>,
    mut reader: impl Read + Send + 'static,
    stream: AutonomousCommandSessionStream,
    max_excerpt_chars: usize,
) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let capture =
                        sanitize_command_output(&buffer[..read], false, max_excerpt_chars);
                    let _ = session.push_chunk(stream.clone(), capture);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    let capture = SanitizedCommandOutput {
                        text: Some(format!("Command session output read failed: {error}")),
                        truncated: false,
                        redacted: false,
                    };
                    let _ = session.push_chunk(stream.clone(), capture);
                    break;
                }
            }
        }
    });
}

fn approval_mode_label(mode: &crate::commands::RuntimeRunApprovalModeDto) -> &'static str {
    match mode {
        crate::commands::RuntimeRunApprovalModeDto::Suggest => "suggest",
        crate::commands::RuntimeRunApprovalModeDto::AutoEdit => "auto_edit",
        crate::commands::RuntimeRunApprovalModeDto::Yolo => "yolo",
    }
}

fn command_result_summary(argv: &[String], exit_code: Option<i32>) -> String {
    match exit_code {
        Some(0) => format!(
            "Command `{}` exited successfully.",
            render_command_for_summary(argv)
        ),
        Some(code) => format!(
            "Command `{}` exited with code {code}.",
            render_command_for_summary(argv)
        ),
        None => format!(
            "Command `{}` terminated without an exit code.",
            render_command_for_summary(argv)
        ),
    }
}

#[derive(Debug)]
struct OutputCapture {
    excerpt: Vec<u8>,
    truncated: bool,
}

fn spawn_capture(
    mut reader: impl Read + Send + 'static,
    max_capture_bytes: usize,
) -> thread::JoinHandle<std::io::Result<OutputCapture>> {
    thread::spawn(move || {
        let mut excerpt = Vec::new();
        let mut truncated = false;
        let mut buffer = [0_u8; 4096];

        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }

            let remaining = max_capture_bytes.saturating_sub(excerpt.len());
            if remaining > 0 {
                let to_copy = remaining.min(read);
                excerpt.extend_from_slice(&buffer[..to_copy]);
                if to_copy < read {
                    truncated = true;
                }
            } else {
                truncated = true;
            }
        }

        Ok(OutputCapture { excerpt, truncated })
    })
}

fn join_capture(
    handle: thread::JoinHandle<std::io::Result<OutputCapture>>,
) -> CommandResult<OutputCapture> {
    match handle.join() {
        Ok(Ok(capture)) => Ok(capture),
        Ok(Err(error)) => Err(CommandError::system_fault(
            "autonomous_tool_command_output_failed",
            format!("Cadence could not capture command output: {error}"),
        )),
        Err(_) => Err(CommandError::system_fault(
            "autonomous_tool_command_output_failed",
            "Cadence could not join the command output capture thread.",
        )),
    }
}

#[derive(Debug)]
struct SanitizedCommandOutput {
    text: Option<String>,
    truncated: bool,
    redacted: bool,
}

fn sanitize_command_output(
    bytes: &[u8],
    truncated: bool,
    excerpt_chars: usize,
) -> SanitizedCommandOutput {
    if bytes.is_empty() {
        return SanitizedCommandOutput {
            text: None,
            truncated,
            redacted: false,
        };
    }

    let decoded = String::from_utf8_lossy(bytes).into_owned();
    if find_prohibited_persistence_content(&decoded).is_some() {
        return SanitizedCommandOutput {
            text: Some(REDACTED_COMMAND_OUTPUT_SUMMARY.into()),
            truncated,
            redacted: true,
        };
    }

    let collapsed = decoded.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return SanitizedCommandOutput {
            text: None,
            truncated,
            redacted: false,
        };
    }

    SanitizedCommandOutput {
        text: Some(truncate_chars(trimmed, excerpt_chars)),
        truncated,
        redacted: false,
    }
}

fn truncate_chars(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }

    let truncated = value
        .chars()
        .take(limit.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}
