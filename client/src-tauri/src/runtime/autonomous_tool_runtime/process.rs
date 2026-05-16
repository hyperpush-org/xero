use std::{
    collections::BTreeMap,
    env, fs,
    io::Read,
    process::{Child, Command},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use super::{
    policy::{command_tool_scope_escalation, CommandPolicyDecision, PreparedCommandRequest},
    repo_scope::display_relative_or_root,
    AutonomousCommandOutput, AutonomousCommandOutputArtifact, AutonomousCommandOutputChunk,
    AutonomousCommandPolicyOutcome, AutonomousCommandPolicyProfile, AutonomousCommandPolicyTrace,
    AutonomousCommandSessionChunk, AutonomousCommandSessionOperation,
    AutonomousCommandSessionOutput, AutonomousCommandSessionReadRequest,
    AutonomousCommandSessionStartRequest, AutonomousCommandSessionStopRequest,
    AutonomousCommandSessionStream, AutonomousToolCommandResult, AutonomousToolOutput,
    AutonomousToolResult, AutonomousToolRuntime, AUTONOMOUS_TOOL_COMMAND,
    AUTONOMOUS_TOOL_COMMAND_SESSION_READ, AUTONOMOUS_TOOL_COMMAND_SESSION_START,
    AUTONOMOUS_TOOL_COMMAND_SESSION_STOP,
};

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    commands::{CommandError, CommandResult, RepositoryStatusEntryDto},
    db::project_app_data_dir_for_repo,
    git::status,
    runtime::{
        cancelled_error,
        process_tree::{cleanup_process_group_after_root_exit, terminate_process_tree},
        redaction::{
            find_prohibited_persistence_content, redact_command_argv_for_persistence,
            render_command_for_persistence,
        },
    },
};
use xero_agent_core::{
    PermissionProfileSandbox, ProjectTrustState, SandboxApprovalSource, SandboxExecutionContext,
    SandboxExecutionMetadata, SandboxExitClassification, SandboxPlatform, SandboxedProcessRequest,
    SandboxedProcessRunner, SandboxedProcessSpawnRequest, SandboxedProcessStdin,
    ToolApprovalRequirement, ToolCallInput, ToolDescriptorV2, ToolEffectClass,
    ToolExecutionContext, ToolMutability, ToolSandbox, ToolSandboxRequirement,
};

const REDACTED_COMMAND_OUTPUT_SUMMARY: &str =
    "Command output was redacted before durable persistence.";
const COMMAND_SESSION_INITIAL_DRAIN: Duration = Duration::from_millis(150);
const DEFAULT_COMMAND_SESSION_READ_BYTES: usize = 16 * 1024;
const MAX_COMMAND_SESSION_READ_BYTES: usize = 64 * 1024;
const MAX_COMMAND_SESSIONS: usize = 8;
const MAX_COMMAND_SESSION_STORED_CHUNKS: usize = 512;
const MAX_COMMAND_SESSION_STORED_BYTES: usize = 1024 * 1024;
const MAX_COMMAND_CHANGED_FILES: usize = 32;
pub(super) const SAFE_COMMAND_ENV_KEYS: &[&str] = &[
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

struct CommandOutputArtifactRequest<'a> {
    tool_name: &'a str,
    prepared: &'a PreparedCommandRequest,
    stdout_bytes: &'a [u8],
    stderr_bytes: &'a [u8],
    stdout_excerpt: &'a SanitizedCommandOutput,
    stderr_excerpt: &'a SanitizedCommandOutput,
    exit_code: Option<i32>,
}

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
                "Xero could not lock the command session registry.",
            )
        })?;
        if sessions.len() >= MAX_COMMAND_SESSIONS {
            return Err(CommandError::user_fixable(
                "autonomous_tool_command_session_limit_reached",
                format!(
                    "Xero limits autonomous command sessions to {MAX_COMMAND_SESSIONS} concurrent process(es). Stop an existing session before starting another."
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
                "Xero could not lock the command session registry.",
            )
        })?;
        if sessions.len() >= MAX_COMMAND_SESSIONS {
            return Err(CommandError::user_fixable(
                "autonomous_tool_command_session_limit_reached",
                format!(
                    "Xero limits autonomous command sessions to {MAX_COMMAND_SESSIONS} concurrent process(es). Stop an existing session before starting another."
                ),
            ));
        }
        Ok(())
    }

    fn get(&self, session_id: &str) -> CommandResult<Arc<ProcessSession>> {
        let sessions = self.sessions.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Xero could not lock the command session registry.",
            )
        })?;
        sessions.get(session_id).cloned().ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_command_session_not_found",
                format!("Xero could not find command session `{session_id}`."),
            )
        })
    }

    fn remove(&self, session_id: &str) -> CommandResult<Arc<ProcessSession>> {
        let mut sessions = self.sessions.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Xero could not lock the command session registry.",
            )
        })?;
        sessions.remove(session_id).ok_or_else(|| {
            CommandError::user_fixable(
                "autonomous_tool_command_session_not_found",
                format!("Xero could not find command session `{session_id}`."),
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
    sandbox_metadata: Mutex<SandboxExecutionMetadata>,
}

impl ProcessSession {
    fn new(
        session_id: String,
        argv: Vec<String>,
        cwd: String,
        child: Child,
        sandbox_metadata: SandboxExecutionMetadata,
    ) -> Self {
        Self {
            session_id,
            argv,
            cwd,
            child: Mutex::new(Some(child)),
            chunks: Mutex::new(Vec::new()),
            next_sequence: AtomicU64::new(1),
            exit_code: Mutex::new(None),
            sandbox_metadata: Mutex::new(sandbox_metadata),
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
                "Xero could not lock command session output.",
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
                "Xero could not lock command session output.",
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

    fn sandbox_metadata(&self) -> CommandResult<SandboxExecutionMetadata> {
        self.sandbox_metadata
            .lock()
            .map_err(|_| {
                CommandError::system_fault(
                    "autonomous_tool_command_session_lock_failed",
                    "Xero could not lock command session sandbox metadata.",
                )
            })
            .map(|metadata| metadata.clone())
    }

    fn set_sandbox_exit_classification(
        &self,
        classification: SandboxExitClassification,
    ) -> CommandResult<()> {
        let mut metadata = self.sandbox_metadata.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Xero could not lock command session sandbox metadata.",
            )
        })?;
        metadata.exit_classification = classification;
        Ok(())
    }

    fn poll_exit(&self) -> CommandResult<Option<i32>> {
        if let Some(exit_code) = *self.exit_code.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Xero could not lock command session exit state.",
            )
        })? {
            return Ok(Some(exit_code));
        }

        let mut child = self.child.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Xero could not lock command session process state.",
            )
        })?;
        let Some(child_ref) = child.as_mut() else {
            return Ok(*self.exit_code.lock().map_err(|_| {
                CommandError::system_fault(
                    "autonomous_tool_command_session_lock_failed",
                    "Xero could not lock command session exit state.",
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
                        "Xero could not lock command session exit state.",
                    )
                })? = exit_code;
                self.set_sandbox_exit_classification(exit_classification_from_code(exit_code))?;
                *child = None;
                Ok(exit_code)
            }
            Ok(None) => Ok(None),
            Err(error) => Err(CommandError::retryable(
                "autonomous_tool_command_session_wait_failed",
                format!(
                    "Xero could not observe command session `{}`: {error}",
                    self.session_id
                ),
            )),
        }
    }

    fn kill(&self) -> CommandResult<Option<i32>> {
        let mut child = self.child.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_command_session_lock_failed",
                "Xero could not lock command session process state.",
            )
        })?;
        let Some(child_ref) = child.as_mut() else {
            return Ok(*self.exit_code.lock().map_err(|_| {
                CommandError::system_fault(
                    "autonomous_tool_command_session_lock_failed",
                    "Xero could not lock command session exit state.",
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
                        "Xero could not lock command session exit state.",
                    )
                })? = exit_code;
                self.set_sandbox_exit_classification(exit_classification_from_code(exit_code))?;
                *child = None;
                Ok(exit_code)
            }
            Ok(None) => {
                let status = terminate_process_tree(child_ref).map_err(|error| {
                    CommandError::retryable(
                        "autonomous_tool_command_session_stop_failed",
                        format!(
                            "Xero could not stop command session `{}`: {error}",
                            self.session_id
                        ),
                    )
                })?;
                let exit_code = status.code();
                *self.exit_code.lock().map_err(|_| {
                    CommandError::system_fault(
                        "autonomous_tool_command_session_lock_failed",
                        "Xero could not lock command session exit state.",
                    )
                })? = exit_code;
                self.set_sandbox_exit_classification(SandboxExitClassification::Cancelled)?;
                *child = None;
                Ok(exit_code)
            }
            Err(error) => Err(CommandError::retryable(
                "autonomous_tool_command_session_wait_failed",
                format!(
                    "Xero could not observe command session `{}` before stopping it: {error}",
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
        self.command_with_approval_for_tool(AUTONOMOUS_TOOL_COMMAND, request, operator_approved)
    }

    pub(crate) fn command_with_approval_for_tool(
        &self,
        tool_name: &str,
        request: super::AutonomousCommandRequest,
        operator_approved: bool,
    ) -> CommandResult<AutonomousToolResult> {
        let decision = self.command_policy_decision_for_tool(tool_name, request)?;

        match decision {
            CommandPolicyDecision::Allow { prepared, policy } => {
                self.spawn_command(tool_name, prepared, policy)
            }
            CommandPolicyDecision::Escalate { prepared, policy } if operator_approved => {
                let policy = operator_approved_policy(policy, &prepared.argv);
                self.spawn_command(tool_name, prepared, policy)
            }
            CommandPolicyDecision::Escalate { prepared, policy } => {
                self.unspawned_command_approval_result(tool_name, prepared, policy)
            }
        }
    }

    pub(crate) fn command_with_approval_and_output_callback_for_tool(
        &self,
        tool_name: &str,
        request: super::AutonomousCommandRequest,
        operator_approved: bool,
        on_chunk: &mut impl FnMut(&AutonomousCommandOutputChunk),
    ) -> CommandResult<AutonomousToolResult> {
        let decision = self.command_policy_decision_for_tool(tool_name, request)?;

        match decision {
            CommandPolicyDecision::Allow { prepared, policy } => {
                self.spawn_command_streaming(tool_name, prepared, policy, on_chunk)
            }
            CommandPolicyDecision::Escalate { prepared, policy } if operator_approved => {
                let policy = operator_approved_policy(policy, &prepared.argv);
                self.spawn_command_streaming(tool_name, prepared, policy, on_chunk)
            }
            CommandPolicyDecision::Escalate { prepared, policy } => {
                self.unspawned_command_approval_result(tool_name, prepared, policy)
            }
        }
    }

    fn command_policy_decision_for_tool(
        &self,
        tool_name: &str,
        request: super::AutonomousCommandRequest,
    ) -> CommandResult<CommandPolicyDecision> {
        let decision = self.evaluate_command_policy(self.prepare_command_request(request)?)?;
        Ok(match decision {
            CommandPolicyDecision::Allow { prepared, policy } => {
                if let Some(policy) = command_tool_scope_escalation(tool_name, &prepared, &policy) {
                    CommandPolicyDecision::Escalate { prepared, policy }
                } else {
                    CommandPolicyDecision::Allow { prepared, policy }
                }
            }
            escalated @ CommandPolicyDecision::Escalate { .. } => escalated,
        })
    }

    fn spawn_command(
        &self,
        tool_name: &str,
        prepared: PreparedCommandRequest,
        policy: AutonomousCommandPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        let sandbox_metadata = self.command_sandbox_metadata(
            tool_name,
            &prepared,
            sandbox_approval_source_for_policy(&policy),
        )?;
        let sandbox_output = SandboxedProcessRunner::new()
            .run(
                SandboxedProcessRequest {
                    argv: prepared.argv.clone(),
                    cwd: Some(prepared.cwd.to_string_lossy().into_owned()),
                    timeout_ms: Some(prepared.timeout_ms),
                    stdout_limit_bytes: self.limits.max_command_capture_bytes,
                    stderr_limit_bytes: self.limits.max_command_capture_bytes,
                    metadata: sandbox_metadata,
                },
                || self.is_cancelled(),
            )
            .map_err(|error| {
                sandbox_runner_error_to_command_error(
                    error,
                    &prepared.argv,
                    prepared.timeout_ms,
                    "autonomous_tool_command",
                )
            })?;

        let stdout_excerpt = sanitize_command_output(
            sandbox_output
                .stdout
                .as_deref()
                .unwrap_or_default()
                .as_bytes(),
            sandbox_output.stdout_truncated,
            self.limits.max_command_excerpt_chars,
        );
        let stderr_excerpt = sanitize_command_output(
            sandbox_output
                .stderr
                .as_deref()
                .unwrap_or_default()
                .as_bytes(),
            sandbox_output.stderr_truncated,
            self.limits.max_command_excerpt_chars,
        );
        let stdout_bytes = sandbox_output
            .stdout
            .as_deref()
            .unwrap_or_default()
            .as_bytes();
        let stderr_bytes = sandbox_output
            .stderr
            .as_deref()
            .unwrap_or_default()
            .as_bytes();

        let exit_code = sandbox_output.exit_code;
        let output_artifact =
            self.command_output_artifact_if_needed(CommandOutputArtifactRequest {
                tool_name,
                prepared: &prepared,
                stdout_bytes,
                stderr_bytes,
                stdout_excerpt: &stdout_excerpt,
                stderr_excerpt: &stderr_excerpt,
                exit_code,
            })?;
        let (changed_files, changed_files_truncated) = self.changed_files_after_command();
        let suggested_next_actions = command_suggested_next_actions(
            true,
            exit_code,
            &policy,
            stdout_excerpt.truncated || stderr_excerpt.truncated,
            output_artifact.as_ref(),
            changed_files.len(),
            changed_files_truncated,
        );
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
            tool_name: tool_name.into(),
            summary,
            command_result: Some(command_result),
            output: AutonomousToolOutput::Command(AutonomousCommandOutput {
                argv: redact_command_argv_for_persistence(&prepared.argv),
                cwd: display_relative_or_root(&self.repo_root, &prepared.cwd),
                intent: command_intent_label(&policy).into(),
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
                changed_files,
                changed_files_truncated,
                output_artifact,
                suggested_next_actions,
                sandbox: Some(sandbox_output.metadata),
            }),
        })
    }

    fn spawn_command_streaming(
        &self,
        tool_name: &str,
        prepared: PreparedCommandRequest,
        policy: AutonomousCommandPolicyTrace,
        on_chunk: &mut impl FnMut(&AutonomousCommandOutputChunk),
    ) -> CommandResult<AutonomousToolResult> {
        let sandbox_metadata = self.command_sandbox_metadata(
            tool_name,
            &prepared,
            sandbox_approval_source_for_policy(&policy),
        )?;
        let mut sandboxed_process = SandboxedProcessRunner::new()
            .spawn(SandboxedProcessSpawnRequest {
                argv: prepared.argv.clone(),
                cwd: Some(prepared.cwd.to_string_lossy().into_owned()),
                stdin: SandboxedProcessStdin::Null,
                metadata: sandbox_metadata,
            })
            .map_err(|error| {
                sandbox_runner_error_to_command_error(
                    error,
                    &prepared.argv,
                    prepared.timeout_ms,
                    "autonomous_tool_command",
                )
            })?;
        let stdout = sandboxed_process.child.stdout.take().ok_or_else(|| {
            CommandError::retryable(
                "autonomous_tool_command_stdout_missing",
                "Xero could not capture command stdout.",
            )
        })?;
        let stderr = sandboxed_process.child.stderr.take().ok_or_else(|| {
            CommandError::retryable(
                "autonomous_tool_command_stderr_missing",
                "Xero could not capture command stderr.",
            )
        })?;

        let (output_sender, output_receiver) = mpsc::channel();
        let stdout_handle = spawn_command_output_reader(
            stdout,
            AutonomousCommandSessionStream::Stdout,
            output_sender.clone(),
        );
        let stderr_handle = spawn_command_output_reader(
            stderr,
            AutonomousCommandSessionStream::Stderr,
            output_sender,
        );
        let mut stdout_capture = StreamingCommandCapture::default();
        let mut stderr_capture = StreamingCommandCapture::default();
        let started_at = Instant::now();
        let timeout = Duration::from_millis(prepared.timeout_ms.max(1));

        let status = loop {
            drain_command_output_events(
                &output_receiver,
                &mut stdout_capture,
                &mut stderr_capture,
                self.limits.max_command_capture_bytes,
                self.limits.max_command_excerpt_chars,
                on_chunk,
            );

            match sandboxed_process.child.try_wait() {
                Ok(Some(status)) => {
                    cleanup_process_group_after_root_exit(sandboxed_process.child.id());
                    break status;
                }
                Ok(None) if self.is_cancelled() => {
                    let _ = terminate_process_tree(&mut sandboxed_process.child);
                    let _ = stdout_handle.join();
                    let _ = stderr_handle.join();
                    drain_command_output_events(
                        &output_receiver,
                        &mut stdout_capture,
                        &mut stderr_capture,
                        self.limits.max_command_capture_bytes,
                        self.limits.max_command_excerpt_chars,
                        on_chunk,
                    );
                    return Err(cancelled_error());
                }
                Ok(None) if started_at.elapsed() >= timeout => {
                    let _ = terminate_process_tree(&mut sandboxed_process.child);
                    let _ = stdout_handle.join();
                    let _ = stderr_handle.join();
                    drain_command_output_events(
                        &output_receiver,
                        &mut stdout_capture,
                        &mut stderr_capture,
                        self.limits.max_command_capture_bytes,
                        self.limits.max_command_excerpt_chars,
                        on_chunk,
                    );
                    return Err(CommandError::retryable(
                        "autonomous_tool_command_timeout",
                        format!(
                            "Xero timed out command `{}` after {}ms.",
                            render_command_for_summary(&prepared.argv),
                            prepared.timeout_ms
                        ),
                    ));
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    let _ = terminate_process_tree(&mut sandboxed_process.child);
                    let _ = stdout_handle.join();
                    let _ = stderr_handle.join();
                    drain_command_output_events(
                        &output_receiver,
                        &mut stdout_capture,
                        &mut stderr_capture,
                        self.limits.max_command_capture_bytes,
                        self.limits.max_command_excerpt_chars,
                        on_chunk,
                    );
                    return Err(CommandError::retryable(
                        "autonomous_tool_command_wait_failed",
                        format!(
                            "Xero could not observe command `{}`: {error}",
                            render_command_for_summary(&prepared.argv)
                        ),
                    ));
                }
            }
        };

        let _ = stdout_handle.join();
        let _ = stderr_handle.join();
        drain_command_output_events(
            &output_receiver,
            &mut stdout_capture,
            &mut stderr_capture,
            self.limits.max_command_capture_bytes,
            self.limits.max_command_excerpt_chars,
            on_chunk,
        );

        let exit_code = status.code();
        sandboxed_process.metadata.exit_classification = exit_classification_from_code(exit_code);
        let stdout_excerpt = sanitize_command_output(
            &stdout_capture.bytes,
            stdout_capture.truncated,
            self.limits.max_command_excerpt_chars,
        );
        let stderr_excerpt = sanitize_command_output(
            &stderr_capture.bytes,
            stderr_capture.truncated,
            self.limits.max_command_excerpt_chars,
        );
        let output_artifact =
            self.command_output_artifact_if_needed(CommandOutputArtifactRequest {
                tool_name,
                prepared: &prepared,
                stdout_bytes: &stdout_capture.bytes,
                stderr_bytes: &stderr_capture.bytes,
                stdout_excerpt: &stdout_excerpt,
                stderr_excerpt: &stderr_excerpt,
                exit_code,
            })?;
        let (changed_files, changed_files_truncated) = self.changed_files_after_command();
        let suggested_next_actions = command_suggested_next_actions(
            true,
            exit_code,
            &policy,
            stdout_excerpt.truncated || stderr_excerpt.truncated,
            output_artifact.as_ref(),
            changed_files.len(),
            changed_files_truncated,
        );
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
            tool_name: tool_name.into(),
            summary,
            command_result: Some(command_result),
            output: AutonomousToolOutput::Command(AutonomousCommandOutput {
                argv: redact_command_argv_for_persistence(&prepared.argv),
                cwd: display_relative_or_root(&self.repo_root, &prepared.cwd),
                intent: command_intent_label(&policy).into(),
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
                changed_files,
                changed_files_truncated,
                output_artifact,
                suggested_next_actions,
                sandbox: Some(sandboxed_process.metadata),
            }),
        })
    }

    fn unspawned_command_approval_result(
        &self,
        tool_name: &str,
        prepared: PreparedCommandRequest,
        policy: AutonomousCommandPolicyTrace,
    ) -> CommandResult<AutonomousToolResult> {
        let cwd = prepared
            .cwd_relative
            .as_ref()
            .map(|path| display_relative_or_root(&self.repo_root, &self.repo_root.join(path)))
            .unwrap_or_else(|| ".".into());
        let summary = format!(
            "Command `{}` requires operator review before Xero can run it.",
            render_command_for_summary(&prepared.argv)
        );
        let suggested_next_actions =
            command_suggested_next_actions(false, None, &policy, false, None, 0, false);

        Ok(AutonomousToolResult {
            tool_name: tool_name.into(),
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
                intent: command_intent_label(&policy).into(),
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
                changed_files: Vec::new(),
                changed_files_truncated: false,
                output_artifact: None,
                suggested_next_actions,
                sandbox: None,
            }),
        })
    }

    fn changed_files_after_command(&self) -> (Vec<RepositoryStatusEntryDto>, bool) {
        let Ok(response) = status::load_repository_status_from_root(&self.repo_root) else {
            return (Vec::new(), false);
        };
        let total = response.entries.len();
        let mut entries = response
            .entries
            .into_iter()
            .take(MAX_COMMAND_CHANGED_FILES)
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.path.clone());
        (entries, total > MAX_COMMAND_CHANGED_FILES)
    }

    fn command_output_artifact_if_needed(
        &self,
        request: CommandOutputArtifactRequest<'_>,
    ) -> CommandResult<Option<AutonomousCommandOutputArtifact>> {
        let CommandOutputArtifactRequest {
            tool_name,
            prepared,
            stdout_bytes,
            stderr_bytes,
            stdout_excerpt,
            stderr_excerpt,
            exit_code,
        } = request;
        let stdout_needs_artifact =
            stdout_excerpt.truncated || stdout_bytes.len() > self.limits.max_command_excerpt_chars;
        let stderr_needs_artifact =
            stderr_excerpt.truncated || stderr_bytes.len() > self.limits.max_command_excerpt_chars;
        if !stdout_needs_artifact && !stderr_needs_artifact {
            return Ok(None);
        }

        let redacted = stdout_excerpt.redacted || stderr_excerpt.redacted;
        let payload = json!({
            "schema": "xero.command_output_artifact.v1",
            "toolName": tool_name,
            "argv": redact_command_argv_for_persistence(&prepared.argv),
            "cwd": display_relative_or_root(&self.repo_root, &prepared.cwd),
            "exitCode": exit_code,
            "stdoutTruncated": stdout_excerpt.truncated,
            "stderrTruncated": stderr_excerpt.truncated,
            "stdoutRedacted": stdout_excerpt.redacted,
            "stderrRedacted": stderr_excerpt.redacted,
            "stdoutBytes": stdout_bytes.len(),
            "stderrBytes": stderr_bytes.len(),
            "stdout": command_artifact_stream_text(stdout_bytes, stdout_excerpt),
            "stderr": command_artifact_stream_text(stderr_bytes, stderr_excerpt),
        });
        let bytes = serde_json::to_vec_pretty(&payload).map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_command_artifact_failed",
                format!("Xero could not serialize command output artifact: {error}"),
            )
        })?;
        let digest = sha256_hex(&bytes);
        let artifact_dir = project_app_data_dir_for_repo(&self.repo_root)
            .join("tool-artifacts")
            .join("command");
        fs::create_dir_all(&artifact_dir).map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_command_artifact_failed",
                format!(
                    "Xero could not create command artifact directory {}: {error}",
                    artifact_dir.display()
                ),
            )
        })?;
        let artifact_path = artifact_dir.join(format!("output-{digest}.json"));
        fs::write(&artifact_path, &bytes).map_err(|error| {
            CommandError::system_fault(
                "autonomous_tool_command_artifact_failed",
                format!(
                    "Xero could not write command artifact {}: {error}",
                    artifact_path.display()
                ),
            )
        })?;

        Ok(Some(AutonomousCommandOutputArtifact {
            path: artifact_path.to_string_lossy().into_owned(),
            byte_count: bytes.len(),
            stdout_bytes: stdout_bytes.len(),
            stderr_bytes: stderr_bytes.len(),
            redacted,
            truncated: stdout_excerpt.truncated || stderr_excerpt.truncated,
        }))
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
        let sandbox_metadata = self.command_sandbox_metadata(
            AUTONOMOUS_TOOL_COMMAND_SESSION_START,
            &prepared,
            sandbox_approval_source_for_policy(&policy),
        )?;
        let mut sandboxed_process = SandboxedProcessRunner::new()
            .spawn(SandboxedProcessSpawnRequest {
                argv: prepared.argv.clone(),
                cwd: Some(prepared.cwd.to_string_lossy().into_owned()),
                stdin: SandboxedProcessStdin::Null,
                metadata: sandbox_metadata,
            })
            .map_err(|error| {
                sandbox_runner_error_to_command_error(
                    error,
                    &prepared.argv,
                    prepared.timeout_ms,
                    "autonomous_tool_command_session",
                )
            })?;

        let stdout = sandboxed_process.child.stdout.take().ok_or_else(|| {
            CommandError::system_fault(
                "autonomous_tool_command_stdout_missing",
                "Xero could not capture command session stdout.",
            )
        })?;
        let stderr = sandboxed_process.child.stderr.take().ok_or_else(|| {
            CommandError::system_fault(
                "autonomous_tool_command_stderr_missing",
                "Xero could not capture command session stderr.",
            )
        })?;

        let session_id = self.process_sessions.next_session_id();
        let cwd = display_relative_or_root(&self.repo_root, &prepared.cwd);
        let session = Arc::new(ProcessSession::new(
            session_id.clone(),
            redact_command_argv_for_persistence(&prepared.argv),
            cwd.clone(),
            sandboxed_process.child,
            sandboxed_process.metadata,
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
                sandbox: Some(session.sandbox_metadata()?),
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
                "Command session `{}` requires operator review before Xero can start it.",
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
                sandbox: None,
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
                sandbox: Some(session.sandbox_metadata()?),
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
                sandbox: Some(session.sandbox_metadata()?),
            }),
        })
    }
}

impl AutonomousToolRuntime {
    fn command_sandbox_metadata(
        &self,
        tool_name: &str,
        prepared: &PreparedCommandRequest,
        approval_source: SandboxApprovalSource,
    ) -> CommandResult<SandboxExecutionMetadata> {
        let descriptor = ToolDescriptorV2 {
            name: tool_name.into(),
            description: "Launch a repo-scoped subprocess through the production sandbox runner."
                .into(),
            input_schema: json!({ "type": "object" }),
            capability_tags: vec!["subprocess".into(), "workspace".into()],
            application_metadata: Default::default(),
            effect_class: ToolEffectClass::CommandExecution,
            mutability: ToolMutability::Mutating,
            sandbox_requirement: ToolSandboxRequirement::FullLocal,
            approval_requirement: ToolApprovalRequirement::Policy,
            telemetry_attributes: BTreeMap::from([
                ("xero.tool.name".into(), tool_name.into()),
                ("xero.sandbox.runner".into(), "production".into()),
            ]),
            result_truncation: Default::default(),
        };
        let app_data_roots = self
            .environment_profile_database_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(|path| vec![path.to_string_lossy().into_owned()])
            .unwrap_or_default();
        let sandbox = PermissionProfileSandbox::new(SandboxExecutionContext {
            workspace_root: self.repo_root.to_string_lossy().into_owned(),
            app_data_roots,
            project_trust: ProjectTrustState::Trusted,
            approval_source,
            platform: SandboxPlatform::current(),
            explicit_git_mutation_allowed: false,
            legacy_xero_migration_allowed: false,
            preserved_environment_keys: SAFE_COMMAND_ENV_KEYS
                .iter()
                .map(|key| (*key).to_owned())
                .collect(),
        });
        let call = ToolCallInput {
            tool_call_id: format!("{tool_name}-subprocess"),
            tool_name: tool_name.into(),
            input: json!({
                "argv": &prepared.argv,
                "cwd": prepared.cwd.to_string_lossy(),
                "timeoutMs": prepared.timeout_ms,
            }),
        };
        sandbox
            .evaluate(&descriptor, &call, &ToolExecutionContext::default())
            .map_err(|denied| CommandError::user_fixable(denied.error.code, denied.error.message))
    }
}

fn render_command_for_summary(argv: &[String]) -> String {
    render_command_for_persistence(argv)
}

fn sandbox_approval_source_for_policy(
    policy: &AutonomousCommandPolicyTrace,
) -> SandboxApprovalSource {
    if policy.code == "policy_allowed_after_operator_approval" {
        SandboxApprovalSource::Operator
    } else {
        SandboxApprovalSource::Policy
    }
}

fn sandbox_runner_error_to_command_error(
    error: xero_agent_core::SandboxedProcessError,
    argv: &[String],
    timeout_ms: u64,
    timeout_code_prefix: &str,
) -> CommandError {
    match error.code.as_str() {
        "sandboxed_process_cancelled" => cancelled_error(),
        "sandboxed_process_timeout" => CommandError::retryable(
            format!("{timeout_code_prefix}_timeout"),
            format!(
                "Xero timed out command `{}` after {}ms.",
                render_command_for_summary(argv),
                timeout_ms,
            ),
        ),
        "sandboxed_process_not_found" => CommandError::user_fixable(
            "autonomous_tool_command_not_found",
            format!(
                "Xero could not find command `{}`.",
                argv.first().cloned().unwrap_or_else(|| "<empty>".into())
            ),
        ),
        code if code.contains("unavailable")
            || code.contains("sandbox")
            || error.metadata.exit_classification == SandboxExitClassification::DeniedBySandbox =>
        {
            CommandError::user_fixable(error.code, error.message)
        }
        _ if error.retryable => CommandError::retryable(error.code, error.message),
        _ => CommandError::system_fault(error.code, error.message),
    }
}

pub(super) fn apply_sanitized_command_environment(command: &mut Command) {
    command.env_clear();
    for key in SAFE_COMMAND_ENV_KEYS {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
    if env::var_os("PATH").is_none() {
        command.env("PATH", default_sanitized_path());
    }
    command.env("XERO_AGENT_SANITIZED_ENV", "1");
}

fn default_sanitized_path() -> &'static str {
    #[cfg(windows)]
    {
        r"C:\Windows\System32;C:\Windows;C:\Windows\System32\WindowsPowerShell\v1.0"
    }

    #[cfg(target_os = "macos")]
    {
        "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin"
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin:/sbin"
    }

    #[cfg(not(any(unix, windows)))]
    {
        ""
    }
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

#[derive(Debug)]
enum CommandOutputReadEvent {
    Chunk {
        stream: AutonomousCommandSessionStream,
        bytes: Vec<u8>,
    },
    ReadFailed {
        stream: AutonomousCommandSessionStream,
        message: String,
    },
}

#[derive(Debug, Default)]
struct StreamingCommandCapture {
    bytes: Vec<u8>,
    truncated: bool,
}

impl StreamingCommandCapture {
    fn append(&mut self, bytes: &[u8], max_capture_bytes: usize) {
        let remaining = max_capture_bytes.saturating_sub(self.bytes.len());
        if remaining > 0 {
            let to_copy = remaining.min(bytes.len());
            self.bytes.extend_from_slice(&bytes[..to_copy]);
            if to_copy < bytes.len() {
                self.truncated = true;
            }
        } else if !bytes.is_empty() {
            self.truncated = true;
        }
    }
}

fn spawn_command_output_reader(
    mut reader: impl Read + Send + 'static,
    stream: AutonomousCommandSessionStream,
    sender: mpsc::Sender<CommandOutputReadEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if sender
                        .send(CommandOutputReadEvent::Chunk {
                            stream: stream.clone(),
                            bytes: buffer[..read].to_vec(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    let _ = sender.send(CommandOutputReadEvent::ReadFailed {
                        stream: stream.clone(),
                        message: format!("Command output read failed: {error}"),
                    });
                    break;
                }
            }
        }
    })
}

fn drain_command_output_events(
    receiver: &mpsc::Receiver<CommandOutputReadEvent>,
    stdout_capture: &mut StreamingCommandCapture,
    stderr_capture: &mut StreamingCommandCapture,
    max_capture_bytes: usize,
    max_excerpt_chars: usize,
    on_chunk: &mut impl FnMut(&AutonomousCommandOutputChunk),
) {
    while let Ok(event) = receiver.try_recv() {
        let (stream, bytes) = match event {
            CommandOutputReadEvent::Chunk { stream, bytes } => (stream, bytes),
            CommandOutputReadEvent::ReadFailed { stream, message } => {
                (stream, message.into_bytes())
            }
        };
        match &stream {
            AutonomousCommandSessionStream::Stdout => {
                stdout_capture.append(&bytes, max_capture_bytes)
            }
            AutonomousCommandSessionStream::Stderr => {
                stderr_capture.append(&bytes, max_capture_bytes)
            }
        }

        let capture = sanitize_command_output(&bytes, false, max_excerpt_chars);
        let chunk = AutonomousCommandOutputChunk {
            stream,
            text: capture.text,
            truncated: capture.truncated,
            redacted: capture.redacted,
        };
        if chunk.text.is_some() || chunk.truncated || chunk.redacted {
            on_chunk(&chunk);
        }
    }
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

fn command_intent_label(policy: &AutonomousCommandPolicyTrace) -> &'static str {
    match policy.profile {
        AutonomousCommandPolicyProfile::ReadOnlyVerification => "read_only_verification",
        AutonomousCommandPolicyProfile::GeneratedFileMutation => "generated_file_mutation",
        AutonomousCommandPolicyProfile::DependencyInstallation => "dependency_installation",
        AutonomousCommandPolicyProfile::ExternalNetwork => "external_network",
        AutonomousCommandPolicyProfile::DestructiveOperation => "destructive_operation",
        AutonomousCommandPolicyProfile::GeneralExecution => "general_execution",
    }
}

fn command_suggested_next_actions(
    spawned: bool,
    exit_code: Option<i32>,
    policy: &AutonomousCommandPolicyTrace,
    stream_truncated: bool,
    output_artifact: Option<&AutonomousCommandOutputArtifact>,
    changed_file_count: usize,
    changed_files_truncated: bool,
) -> Vec<String> {
    let mut actions = Vec::new();
    if !spawned {
        actions.push(
            "Request operator approval or choose a narrower native tool before retrying.".into(),
        );
        return actions;
    }
    if !matches!(exit_code, Some(0)) {
        actions.push(
            "Use the compact stdout/stderr evidence to fix the failure, then rerun a focused command_verify.".into(),
        );
    }
    if output_artifact.is_some() || stream_truncated {
        actions.push(
            "Use outputArtifact.path as the continuation for captured stdout/stderr details if the compact stream is insufficient.".into(),
        );
    }
    if changed_file_count > 0 || changed_files_truncated {
        actions.push(
            "Inspect changedFiles with git_diff or targeted native reads before summarizing repository effects.".into(),
        );
    } else if matches!(
        policy.profile,
        AutonomousCommandPolicyProfile::GeneratedFileMutation
    ) {
        actions.push(
            "Run git_status before assuming the build command left no generated output.".into(),
        );
    }
    actions
}

fn exit_classification_from_code(exit_code: Option<i32>) -> SandboxExitClassification {
    match exit_code {
        Some(0) => SandboxExitClassification::Success,
        Some(_) | None => SandboxExitClassification::Failed,
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

    let excerpt_truncated = trimmed.chars().count() > excerpt_chars;
    SanitizedCommandOutput {
        text: Some(truncate_chars(trimmed, excerpt_chars)),
        truncated: truncated || excerpt_truncated,
        redacted: false,
    }
}

fn command_artifact_stream_text(bytes: &[u8], excerpt: &SanitizedCommandOutput) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    if excerpt.redacted {
        return Some(REDACTED_COMMAND_OUTPUT_SUMMARY.into());
    }
    let decoded = String::from_utf8_lossy(bytes).into_owned();
    let collapsed = decoded.replace("\r\n", "\n").replace('\r', "\n");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
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
